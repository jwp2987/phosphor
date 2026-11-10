# appci/cases_lab_remote.py: lab cases for the remote parity families (Wave H parity, docs/design/remote-helper-parity.md), registered through lab_case.
#   lab_read_files_remote   family a: the agent's read_files on a remote ssh session, no helper (shell mode over the ControlMaster) and with the uploaded helper
#   lab_diff_apply_remote   the write half of apply_file_diffs: create a file, then delete it, over the same two arms
# Shells: u_bash (bash), u_dash, u_ash, u_nu, u_pwsh (the login shell the ControlMaster side exec runs the commands in). Agent mode only, against the scripted fake model
# (fake_model.py, APPCI_FAKE_MODEL=1: "Please read exactly this file and nothing else: <path>", "Please create exactly this file: <path> with this content: <text>",
# "Please delete exactly this file and nothing else: <path>"). Oracles: the file on the lab host (create/delete), and the app log's routing line
# `hop=backend_route family=<capability> backend=<helper|shell|unavailable> reason=<...>` (nohelper -> shell, upload -> helper); no key is typed into the remote
# pty by the feature (the remote shell's rec.log only holds what the test typed). Never run without the lab / fake model: lab_prepare and need_model SKIP with the reason.
import os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases_caseagent as AG  # noqa: E402
import cases_lab as CL  # noqa: E402
import cases_replay as CRP  # noqa: E402

ROUTE = re.compile(r"hop=backend_route family=(\w+) backend=(helper|shell|unavailable) reason=(\w+)")
SHELLS = ("bash", "dash", "ash", "nu", "pwsh")


def routes(ctx, off, family):
    return [(m.group(2), m.group(3)) for m in ROUTE.finditer(ctx.log_since(off)) if m.group(1) == family]


def expected_backend(arm):
    return "helper" if arm["helper"] != "nohelper" else "shell"


def ask(ctx, text, family, timeout=90):
    """type `text` to the agent, approve the card if one shows, wait for the routing line of `family`; returns the routes seen"""
    off = ctx.log_size()
    AG.type_fast(ctx, text)
    t_end = time.time() + timeout
    pressed = False
    while time.time() < t_end:
        got = routes(ctx, off, family)
        if got:
            time.sleep(1.5)
            return routes(ctx, off, family)
        if not pressed and AG.count_in_log(ctx, off, "queue_actions") >= 1:
            time.sleep(0.8)
            ctx.be.key("Return")
            time.sleep(0.6)
            ctx.be.key("Return")
            pressed = True
        time.sleep(0.25)
    return []


@CL.lab_case("read_files_remote", shells=SHELLS, lat=("lan",), helper=("nohelper", "upload"), modes=("agent",))
def c_read_files_remote(ctx, r, arm):
    """family a: a file created on the remote host is read back by the agent's read_files; the routing line says which arm served it"""
    CRP.need_model(ctx, r)
    tok = CL.unique(ctx, 6)
    path = CL.work(arm, "rf_%s.txt" % tok)
    if not CL.agent_connect(ctx, r, arm):          # no remote prompt, or the #2019 plain-session limitation (UNVERIFIABLE)
        return
    ask(ctx, "Please create exactly this file: %s with this content: line one %s" % (path, tok), "file_write")
    r.check(any(tok in l for l in ctx.remote.lines(path)), "the setup file was not created on the host")
    got = ask(ctx, "Please read exactly this file and nothing else: %s" % path, "file_context")
    r.check(bool(got), "no backend_route line for file_context: the read never reached the router")
    r.check(all(b == expected_backend(arm) for b, _ in got), "file_context routed to %r, expected %s" % (got, expected_backend(arm)))
    typed = [cmd for _, cmd in ctx.remote.commands() if tok in cmd]
    r.check(not typed, "the feature typed into the remote shell: %r" % typed[:2])
    ctx.remote.rm(path)
    CL.finish_remote(ctx, r)


def start_ps_sampler(ctx, arm, pat_file, out_file, ctl, seconds=45):
    """a detached sampler on the lab host: every few ms it lists all process command lines and appends those holding a pattern from `pat_file` to `out_file`. It also
    starts one short control process whose command line holds `ctl`, so a sampler that sees nothing can be told from one that works. The patterns live in a file so the
    sampler's own command line never holds them. Returns False on a pwsh login shell (no sh/ps there)."""
    import base64
    if arm["shell"] == "pwsh":
        return False
    script = ("( sh -c 'sleep 2' %s & end=$(( $(date +%%s) + %d )); while [ $(date +%%s) -lt $end ]; do ps -eo args | grep -F -f %s | grep -v grep >> %s; "
              "sleep 0.02 2>/dev/null || sleep 1; done ) </dev/null >/dev/null 2>&1 &" % (ctl, seconds, pat_file, out_file))
    wrapped = "printf %%s %s | base64 -d | sh" % base64.b64encode(script.encode()).decode()
    out = ctx.lab.ssh(arm["user"], "sh -c '%s'" % wrapped, timeout=60)
    return out.returncode == 0


@CL.lab_case("diff_apply_remote", shells=SHELLS, lat=("lan",), helper=("nohelper", "upload"), modes=("agent",))
def c_diff_apply_remote(ctx, r, arm):
    """the write half of apply_file_diffs: create, then delete, a file on the remote host; the host's file is the oracle, the pty stays untouched"""
    CRP.need_model(ctx, r)
    tok = CL.unique(ctx, 6)
    path = CL.work(arm, "da_%s.txt" % tok)
    # #1864: the file's content must never be in a process command line on the host (ps, audit logs). Sample every command line while the write runs.
    mk = "SECRETMARK" + CL.unique(ctx, 8)
    import base64
    b64 = base64.b64encode(("hello %s" % mk).encode()).decode()[:16]
    ctl = "CTLMARK" + CL.unique(ctx, 8)
    pat, seen = CL.work(arm, "da_%s.pat" % tok), CL.work(arm, "da_%s.ps" % tok)
    wrote = ctx.lab.ssh(arm["user"], "sh -c 'echo %s > %s; echo %s >> %s; echo %s >> %s'" % (mk, pat, b64, pat, ctl, pat), timeout=30).returncode == 0
    sampling = wrote and start_ps_sampler(ctx, arm, pat, seen, ctl)
    if not CL.agent_connect(ctx, r, arm):          # no remote prompt, or the #2019 plain-session limitation (UNVERIFIABLE)
        ctx.remote.rm(pat, seen)
        return
    got = ask(ctx, "Please create exactly this file: %s with this content: hello %s" % (path, mk), "file_write")
    r.check(ctx.until(lambda: any(mk in l for l in ctx.remote.lines(path)), 30, step=0.5), "the created file is not on the host")
    r.check(all(b == expected_backend(arm) for b, _ in got), "file_write routed to %r, expected %s" % (got, expected_backend(arm)))
    ask(ctx, "Please delete exactly this file and nothing else: %s" % path, "file_write")
    r.check(ctx.until(lambda: not ctx.remote.lines(path), 30, step=0.5), "the deleted file is still on the host")
    typed = [cmd for _, cmd in ctx.remote.commands() if tok in cmd]
    r.check(not typed, "the feature typed into the remote shell: %r" % typed[:2])
    if sampling:
        ctx.until(lambda: any(ctl in l for l in ctx.remote.lines(seen)), 15, step=0.5)
        hits = [l for l in ctx.remote.lines(seen) if mk in l or b64 in l]
        r.check(any(ctl in l for l in ctx.remote.lines(seen)), "the process sampler never saw its own control process: the no-leak check proves nothing")
        r.check(not hits, "file content (raw or base64) was in a process command line on the host: %r" % [h[:80] for h in hits[:2]])
    else:
        r.unverifiable("no process sampler on this login shell (%s): the no-content-in-argv check did not run" % arm["shell"])
    ctx.remote.rm(path, pat, seen)
    CL.finish_remote(ctx, r)


# ---- family b: the file tree (Wave H parity P3). The hop=file_tree lines are the app's own log (manager.rs log_file_tree_event); counts and leaf-name hashes only.
HOOK_FILE_TREE = re.compile(r"hop=file_tree event=(snapshot|update) source=(helper|shell) removed=(\d+) added=(\d+) added_hash=\[([0-9a-f,]*)\]")


def fnv1a32(name):
    """the hash manager.rs log_file_tree_event logs for a leaf name (32-bit FNV-1a over the UTF-8 bytes), 8 hex digits"""
    h = 2166136261
    for b in name.encode():
        h = ((h ^ b) * 16777619) & 0xFFFFFFFF
    return "%08x" % h


def tree_events(text):
    """[(event, source, removed, added, [hash...])] in log order"""
    return [(m.group(1), m.group(2), int(m.group(3)), int(m.group(4)), [h for h in m.group(5).split(",") if h]) for m in HOOK_FILE_TREE.finditer(text)]


def saw_added(text, name, event="update"):
    want = fnv1a32(name)
    return any(e == event and want in hashes for e, _s, _r, _a, hashes in tree_events(text))


@CL.lab_case("file_tree_remote", shells=("bash", "zsh", "fish", "nu", "dash", "ash"), lat=("lan", "150"), helper=("nohelper", "upload"), modes=("terminal",))
def c_file_tree_remote(ctx, r, arm):
    """The remote file tree, with and without the helper. In a git repository on the server the user `cd`s into a subdirectory: the app gets the repo's tree
    (a snapshot from the helper, or from `git ls-files` over the out-of-band channel). Then a file is created OUTSIDE the terminal (a separate ssh connection,
    the way an editor or a build would):
      upload   -- the helper's watcher pushes the update on its own, no prompt needed (live);
      nohelper -- nothing is seen while no prompt returns (no timer, no polling); the next prompt re-reads the tree and the new file arrives.
    The tree event is the app's own log line (hop=file_tree); the user-visible tree panel shares the same events."""
    lab, u, rt = ctx.lab, arm["user"], CL.rtt_s(arm)
    uid = CL.unique(ctx)
    fx = "tree_%s" % uid
    repo = CL.work(arm, fx)
    lab.tree(u, {"%s/src/lib.rs" % fx: "", "%s/a.txt" % fx: "", "%s/.gitignore" % fx: "*.log\n"}, git=True)
    off = ctx.log_size()
    if CL.connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    time.sleep(1.0 + 2 * rt)
    ctx.be.type("cd %s/src" % repo, CL.type_pace(ctx), enter=True)
    snap = ctx.until(lambda: any(e == "snapshot" for e, *_ in tree_events(ctx.log_since(off))), 20 + 6 * rt, step=0.1)
    if not snap:
        if arm["helper"] == "nohelper":
            r.unverifiable("[hook] no hop=file_tree snapshot in the app log: the shell-mode file tree needs the session's out-of-band channel (the ControlMaster adapter), which this build may not register yet")
        else:
            r.check(False, "upload arm: the helper's file-tree snapshot never reached the app (no hop=file_tree event=snapshot in the log)")
        ctx.reset()
        CL.finish_remote(ctx, r)
        return
    sources = {s for e, s, *_ in tree_events(ctx.log_since(off)) if e == "snapshot"}
    want_source = "helper" if arm["helper"] == "upload" else "shell"
    r.check(sources == {want_source}, "the snapshot came from %s, expected %s" % (sorted(sources), want_source))
    ctx.shot("lab_file_tree_snapshot_%s" % arm["helper"])

    # a file appears outside the terminal
    new = "outside_%s.txt" % uid
    mark = ctx.log_size()
    lab.ssh(u, "touch %s/%s" % (repo, new))
    if arm["helper"] == "upload":
        live = ctx.until(lambda: saw_added(ctx.log_since(mark), new), 10 + 4 * rt, step=0.1)
        r.check(bool(live), "the helper did not push the new file without a prompt (no hop=file_tree update naming it)")
    else:
        # no prompt has come back: the shell backend has nothing to react to, and a quiet period shows it neither polls nor guesses
        time.sleep(2.0 + 2 * rt)
        r.check(not saw_added(ctx.log_since(mark), new), "the file was reported before any prompt returned: the shell backend must refresh on prompt events only")
        ctx.be.type("true", CL.type_pace(ctx), enter=True)
        seen = ctx.until(lambda: saw_added(ctx.log_since(mark), new), 15 + 6 * rt, step=0.1)
        r.check(bool(seen), "the next prompt did not bring the new file into the tree (no hop=file_tree update naming it)")
    ctx.shot("lab_file_tree_update_%s" % arm["helper"])

    # and it goes away again the same way
    mark = ctx.log_size()
    lab.ssh(u, "rm -f %s/%s" % (repo, new))
    if arm["helper"] == "nohelper":
        ctx.be.type("true", CL.type_pace(ctx), enter=True)
    gone = ctx.until(lambda: any(e == "update" and rem >= 1 for e, _s, rem, _a, _h in tree_events(ctx.log_since(mark))), 15 + 6 * rt, step=0.1)
    r.check(bool(gone), "the removal of the file was never reported (no hop=file_tree update with removed>=1)")
    CL.finish_remote(ctx, r)


def declare_coverage():
    """tell the coverage guard (cases_lab_helper.py) which cases cover which families; both arms of each body above are registered here. Idempotent. Called from
    here when cases_lab_helper is already loaded and from cases_lab_helper's end otherwise (the two import each other through cases.py)."""
    import cases_lab_helper as CLH
    CLH.declare_external("file_context", helper_case="lab_read_files_remote", nohelper_case="lab_read_files_remote")
    CLH.declare_external("file_write", helper_case="lab_diff_apply_remote", nohelper_case="lab_diff_apply_remote")
    CLH.declare_external("repo_metadata", helper_case="lab_file_tree_remote", nohelper_case="lab_file_tree_remote")
    CLH.declare_external("fs_basic", helper_case="lab_file_tree_remote", nohelper_case="lab_file_tree_remote")


if hasattr(sys.modules.get("cases_lab_helper"), "declare_external"):
    declare_coverage()
