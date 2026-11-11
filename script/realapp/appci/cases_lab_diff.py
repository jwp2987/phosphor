# appci/cases_lab_diff.py: lab cases for the git-derived views over ssh (Wave H parity P6, families d and e), registered through lab_case.
#   lab_diff_panel_remote   the code-review diff panel of a remote repo: the snapshot, then an edit made OUTSIDE the terminal
#   lab_gh_chip_remote      the git chip and the PR / repository chip of a remote repo, with a fake `gh` in the lab home
# Arms: nohelper (the shell backend over the ControlMaster side channel, the primary design) and upload (the helper). Shells: bash, zsh, fish, nu.
#
# Outside-the-terminal edit, per arm (the contract this proves):
#   upload   -- the helper's watcher pushes the change on its own, no prompt needed (live);
#   nohelper -- nothing is seen while no prompt returns (no timer, no polling); the NEXT prompt (a git command for the chips) re-reads and the change arrives.
#
# Two layers, like cases_lab_git.py:
#  1. HOST layer (always runs, needs only the lab): the git / gh commands the backends send give the right answers on the real host under this user's login shell
#     (`sh -c '...'` wrapped), including the fake gh (PR 42 for `pr view`, widgets/acme for `repo view`) and a gh that is signed out (the chips must read it as ABSENT).
#  2. APP layer [hook]: the app logs one line per applied value, counts and 32-bit FNV-1a hashes only:
#       hop=diff_state event=<snapshot|delta> source=<helper|shell> files=N path_hash=[...]      (code_review/diff_state_remote.rs)
#       hop=git_chip source=<helper|shell> files=N added=A removed=R ahead=a behind=b upstream=0|1  (code_review/git_status_update_remote.rs)
#       hop=gh_chip event=<pr|repo> source=<helper|shell> pr=<number|none> | repo=<some|none>     (code_review/github_repo_model/remote.rs)
#     Round 15 (#2063): the code-review panel is opened by keyboard, through the command palette ("Toggle code review", CL.open_code_review), and the app says
#     whether it opened (hop=right_panel open=<bool>, trace only). Opened and no hop=diff_state snapshot is a FAIL; no hop=right_panel line at all (an older
#     build) is UNVERIFIABLE, never a pass. A fish / nu login shell has a plain session by decision (#2019): UNVERIFIABLE with that reference. The same
#     assertions run against both backends with real git and a fake gh in crates/remote_server/src/backend/contract_tests.rs.
import base64, os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases_lab as CL  # noqa: E402
import cases_lab_git as CLG  # noqa: E402
import cases_lab_remote as CLR  # noqa: E402
import cases_lab_helper as CLH  # noqa: E402

SHELLS = ("bash", "zsh", "fish", "nu", "dash", "ash")      # + pwsh where the case says so (git over PowerShell: shellcmd/git/pwsh.rs)
HOOK_DIFF = re.compile(r"hop=diff_state event=(snapshot|delta) source=(helper|shell) files=(\d+) path_hash=\[([0-9a-f,]*)\]")
HOOK_CHIP = re.compile(r"hop=git_chip source=(helper|shell) files=(\d+) added=(\d+) removed=(\d+) ahead=(\d+) behind=(\d+) upstream=([01])")
HOOK_GH = re.compile(r"hop=gh_chip event=(pr|repo) source=(helper|shell) (?:pr=(\w+)|repo=(\w+))")

# the fake gh: PR 42 for `pr view`, widgets/acme for `repo view`; mirrors contract_tests.rs FAKE_GH
FAKE_GH = """#!/bin/sh
case "$1 $2" in
  "--version "*) echo "gh version 2.0.0";;
  "pr view") printf '{"number":42,"url":"https://github.com/acme/widgets/pull/42","state":"OPEN","isDraft":false,"baseRefName":"main"}\\n';;
  "repo view") printf '{"name":"widgets","owner":{"login":"acme"},"url":"https://github.com/acme/widgets"}\\n';;
  *) echo "unexpected: $*" >&2; exit 1;;
esac
"""
SIGNED_OUT_GH = "#!/bin/sh\ncase \"$1\" in --version) echo gh;; *) echo 'To get started with GitHub CLI, please run:  gh auth login' >&2; exit 4;; esac\n"
GH_DIRS = ("bin", ".local/bin")        # the places a login shell puts on PATH


def diff_events(text):
    """[(event, source, files, [hash...])] in log order"""
    return [(m.group(1), m.group(2), int(m.group(3)), [h for h in m.group(4).split(",") if h]) for m in HOOK_DIFF.finditer(text)]


def chip_events(text):
    """[(source, files, added, removed, ahead, behind, has_upstream)]"""
    return [(m.group(1),) + tuple(int(g) for g in m.groups()[1:6]) + (m.group(7) == "1",) for m in HOOK_CHIP.finditer(text)]


def gh_events(text):
    """[(event, source, value)] where value is the PR number / 'none' or the repo 'some' / 'none'"""
    return [(m.group(1), m.group(2), m.group(3) or m.group(4)) for m in HOOK_GH.finditer(text)]


def install_gh(lab, arm, script):
    b64 = base64.b64encode(script.encode()).decode()
    cmd = " && ".join("mkdir -p ~/%s && printf %%s %s | base64 -d > ~/%s/gh && chmod +x ~/%s/gh" % (d, b64, d, d) for d in GH_DIRS)
    return CLG.run(lab, arm, cmd)


def remove_gh(lab, arm):
    CLG.run(lab, arm, "rm -f " + " ".join("~/%s/gh" % d for d in GH_DIRS))


def want_source(arm):
    return "helper" if arm["helper"] == "upload" else "shell"


@CL.lab_case("diff_panel_remote", shells=SHELLS + ("pwsh",), lat=("lan",), helper=("nohelper", "upload"), modes=("terminal",))
def c_diff_panel_remote(ctx, r, arm):
    lab, u, rt = ctx.lab, arm["user"], CL.rtt_s(arm)
    uid = CL.unique(ctx)
    fx = "diffp_%s" % uid
    repo = CL.work(arm, fx)
    lab.tree(u, {"a.txt": "one\n", "src/b.txt": "b\n", ".gitignore": "*.log\n"}, dir=fx, git=True)
    # an uncommitted edit and an untracked file, made before the app looks
    rc, out, err = CLG.run(lab, arm, "cd %s && echo two >> a.txt && echo u > untracked.txt" % repo)
    r.check(rc == 0, "fixture edit failed: %r" % err[-200:])

    # ---- 1. HOST layer: the commands the backends send, under this user's login shell
    rc, out, err = CLG.run(lab, arm, "cd %s && git --no-optional-locks status --untracked-files=all --branch --porcelain=2 -z | tr '\\000' '\\n'" % repo)
    r.check(rc == 0 and "a.txt" in out and "untracked.txt" in out and "# branch.head main" in out, "git status on the host did not show the edit and the untracked file: rc=%s %r" % (rc, (out + err)[-300:]))
    rc, out, err = CLG.run(lab, arm, "cd %s && git diff --numstat HEAD" % repo)
    r.check(rc == 0 and out.split() [:3] == ["1", "0", "a.txt"], "git diff --numstat HEAD on the host: %r" % out[-200:])

    # ---- 2. APP layer. The panel is opened by keyboard (round 15: the command palette's "Toggle code review"); nothing is typed into the app after that, so
    # the case does not depend on where the focus is while the panel is open: the prompt the shell backend refreshes on is made to return from outside
    # (CL.hold_line: the foreground waits for a file the harness creates over its own ssh connection).
    off = ctx.log_size()
    if CL.connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    time.sleep(1.0 + 2 * rt)
    why = CL.plain_session_feature_limit(arm["shell"], ctx.log_since(off), "the code-review panel of a remote repository")
    if why:
        r.metric("plain_session_by_decision", arm["shell"])
        r.unverifiable(why)
        CL.finish_remote(ctx, r)
        return
    ctx.be.type("cd %s" % repo, CL.type_pace(ctx), enter=True)
    time.sleep(1.5 + 2 * rt)
    gate = CL.work(arm, "go_%s" % uid)
    held = arm["helper"] != "upload"
    if held:
        ctx.be.type(CL.hold_line(gate), CL.type_pace(ctx), enter=True)
        time.sleep(0.5 + rt)

    def release():
        lab.ssh(u, "sh -c 'touch %s'" % gate)

    def snapshot_seen():
        return any(e == "snapshot" for e, *_ in diff_events(ctx.log_since(off)))
    opened = None
    seen = ctx.until(snapshot_seen, 4 + 2 * rt, step=0.1)           # a panel left open by the user profile needs no key
    if not seen:
        opened = CL.open_code_review(ctx, off, "lab_diff_panel")
        seen = ctx.until(snapshot_seen, 20 + 6 * rt, step=0.1)
    if not seen:
        verdict, msg = panel_no_snapshot_verdict(opened)
        ctx.shot("lab_diff_panel_no_snapshot_%s" % arm["helper"])
        if held:
            release()
            time.sleep(0.5 + rt)
        if opened == "open":
            CL.close_code_review(ctx, r, "lab_diff_panel")
        if verdict == "fail":
            r.check(False, msg)
        else:
            r.unverifiable(msg)
        ctx.reset()
        lab.ssh(u, "sh -c 'rm -f %s'" % gate)
        CL.finish_remote(ctx, r)
        return
    snaps = [x for x in diff_events(ctx.log_since(off)) if x[0] == "snapshot"]
    r.check({s for _e, s, _f, _h in snaps} == {want_source(arm)}, "the snapshot came from %s, expected %s" % (sorted({s for _e, s, _f, _h in snaps}), want_source(arm)))
    r.check(snaps[-1][2] == 2 and CLR.fnv1a32("a.txt") in snaps[-1][3] and CLR.fnv1a32("untracked.txt") in snaps[-1][3],
            "the snapshot did not list the edit and the untracked file: %r" % (snaps[-1],))
    ctx.shot("lab_diff_panel_snapshot_%s" % arm["helper"])

    # an edit OUTSIDE the terminal
    mark = ctx.log_size()
    lab.ssh(u, "sh -c 'echo three >> %s/a.txt'" % repo)
    a_hash = CLR.fnv1a32("a.txt")

    def changed():
        return any(a_hash in h for _e, _s, _f, h in diff_events(ctx.log_since(mark)))
    if arm["helper"] == "upload":
        r.check(bool(ctx.until(changed, 10 + 4 * rt, step=0.1)), "the helper did not push the edit without a prompt (no hop=diff_state line naming a.txt)")
    else:
        time.sleep(2.0 + 2 * rt)
        r.check(not changed(), "the edit was reported before any prompt returned: the shell backend must refresh on prompt events only")
        release()                                                   # the held foreground ends: the prompt comes back, no key typed
        r.check(bool(ctx.until(changed, 15 + 6 * rt, step=0.1)), "the next prompt did not bring the edit into the diff (no hop=diff_state line naming a.txt)")
    ctx.shot("lab_diff_panel_edit_%s" % arm["helper"])
    if opened == "open":
        CL.close_code_review(ctx, r, "lab_diff_panel")
    lab.ssh(u, "sh -c 'rm -f %s'" % gate)
    CL.finish_remote(ctx, r)


def panel_no_snapshot_verdict(opened):
    """pure: what a missing diff snapshot means, by what the app said about the panel. ('fail' | 'noref', message).
      'open'    the app logged hop=right_panel open=true after the palette's "Toggle code review": the panel is open in a remote git repository and showed
                no diff. That is what the user would see: FAIL.
      'unknown' the palette was driven and the app logged no hop=right_panel line: whether the panel opened is not known. UNVERIFIABLE, never a pass.
      None      (kept for a caller that did not drive the palette) the same."""
    if opened == "open":
        return "fail", ("the code-review panel opened (hop=right_panel open=true, by the palette's '%s') in a git repository on the remote host and the app applied "
                        "no diff snapshot for it (no hop=diff_state event=snapshot): the panel has nothing to show for a remote repository" % CL.PALETTE_CODE_REVIEW)
    return "noref", ("[hook] no hop=diff_state snapshot, and the app logged no hop=right_panel line after the palette's '%s': whether the panel opened is not "
                     "known (a build without the round 15 trace line, or the palette ran another entry: shots/*lab_diff_panel_palette_*.png). The host layer above "
                     "ran. NEEDS: an app build with workspace/view/right_panel.rs right_panel_trace_line" % CL.PALETTE_CODE_REVIEW)


@CL.lab_case("gh_chip_remote", shells=SHELLS, lat=("lan",), helper=("nohelper", "upload"), modes=("terminal",))
def c_gh_chip_remote(ctx, r, arm):
    lab, u, rt = ctx.lab, arm["user"], CL.rtt_s(arm)
    uid = CL.unique(ctx)
    fx = "ghchip_%s" % uid
    repo = CL.work(arm, fx)
    lab.tree(u, {"a.txt": "one\n"}, dir=fx, git=True)
    rc, out, err = install_gh(lab, arm, FAKE_GH)
    r.check(rc == 0, "could not install the fake gh: %r" % err[-200:])

    # ---- 1. HOST layer: the lookups the backends run, on the host's PATH as a login shell sees it, and a signed-out gh reads as absent
    gh_path = "PATH=$HOME/bin:$HOME/.local/bin:$PATH"
    rc, out, err = CLG.run(lab, arm, "cd %s && %s gh pr view --json number,url,state,isDraft,baseRefName" % (repo, gh_path))
    r.check(rc == 0 and '"number":42' in out, "the fake gh did not answer `pr view` on the host: rc=%s %r" % (rc, (out + err)[-200:]))
    rc, out, err = CLG.run(lab, arm, "cd %s && %s gh repo view --json name,owner,url" % (repo, gh_path))
    r.check(rc == 0 and '"login":"acme"' in out, "the fake gh did not answer `repo view` on the host: rc=%s %r" % (rc, (out + err)[-200:]))
    install_gh(lab, arm, SIGNED_OUT_GH)
    rc, out, err = CLG.run(lab, arm, "cd %s && %s GH_PROMPT_DISABLED=1 gh pr view --json number </dev/null" % (repo, gh_path))
    r.check(rc != 0 and "gh auth login" in err, "a signed-out gh must fail fast with its own words: rc=%s %r" % (rc, err[-200:]))
    install_gh(lab, arm, FAKE_GH)

    # ---- 2. APP layer
    off = ctx.log_size()
    if CL.connect_or_fail(ctx, r, arm) is None:
        remove_gh(lab, arm)
        ctx.reset()
        return
    time.sleep(1.0 + 2 * rt)
    ctx.be.type("cd %s" % repo, CL.type_pace(ctx), enter=True)
    got_chip = ctx.until(lambda: bool(chip_events(ctx.log_since(off))), 20 + 6 * rt, step=0.1)
    if not got_chip:
        r.unverifiable("[hook] no hop=git_chip line in the app log: the remote git chip did not populate for this session (it needs the repo to be navigated and a git chip model)")
    else:
        sources = {c[0] for c in chip_events(ctx.log_since(off))}
        r.check(sources == {want_source(arm)}, "the git chip came from %s, expected %s" % (sorted(sources), want_source(arm)))
        first = chip_events(ctx.log_since(off))[-1]
        r.check(first[1] == 0, "a clean repo shows no changed files, got %r" % (first,))
        ctx.shot("lab_git_chip_%s" % arm["helper"])
    pr_seen = ctx.until(lambda: any(e == "pr" and v == "42" for e, _s, v in gh_events(ctx.log_since(off))), 20 + 6 * rt, step=0.1)
    if not pr_seen:
        r.unverifiable("[hook] no hop=gh_chip pr=42 line: either the PR chip model was not created for this session or the fake gh in ~/bin and ~/.local/bin is not on this "
                       "shell's login PATH (fish and nu do not read ~/.profile); the host layer ran gh with that PATH explicitly")
    else:
        r.check({s for e, s, _v in gh_events(ctx.log_since(off)) if e == "pr"} == {want_source(arm)}, "the PR chip came from the wrong backend")
        # the branch changes OUTSIDE the terminal
        mark = ctx.log_size()
        lab.ssh(u, "sh -c 'cd %s && git checkout -q -b feature'" % repo)

        def looked_up():
            return any(e == "pr" for e, _s, _v in gh_events(ctx.log_since(mark)))
        if arm["helper"] == "upload":
            r.check(bool(ctx.until(looked_up, 15 + 4 * rt, step=0.1)), "the helper did not look the PR up again after the branch changed (no hop=gh_chip line)")
        else:
            time.sleep(2.0 + 2 * rt)
            r.check(not looked_up(), "the PR was looked up before any prompt returned: the shell backend must refresh on prompt events only")
            ctx.be.type("git status", CL.type_pace(ctx), enter=True)
            r.check(bool(ctx.until(looked_up, 20 + 6 * rt, step=0.1)), "the next git command did not bring a new PR lookup (the branch moved)")
        ctx.shot("lab_gh_chip_%s" % arm["helper"])
    remove_gh(lab, arm)
    CL.finish_remote(ctx, r)


def declare_coverage():
    """tell the coverage guard (cases_lab_helper.py) which cases cover which families; both arms of each body above are registered here. Idempotent."""
    CLH.declare_external("diff_state", helper_case="lab_diff_panel_remote", nohelper_case="lab_diff_panel_remote")
    CLH.declare_external("git_status", helper_case="lab_gh_chip_remote", nohelper_case="lab_gh_chip_remote")
    CLH.declare_external("github", helper_case="lab_gh_chip_remote", nohelper_case="lab_gh_chip_remote")


declare_coverage()
