# appci/cases_lab_git.py: the git WRITE family over ssh (wave H parity P7, family f): commit, push, pull, stage a hunk, discard, with a bare repo in the lab as
# the remote. Arms: lab_git_write_remote.<bash|zsh|fish|nu>.lan.<nohelper|upload>. `nohelper` is the shell backend (the primary design), `upload` the helper.
#
# What it proves, in two layers:
#  1. HOST layer (always runs, needs only the lab): the exact script shape the shell backend sends (ShellGitRunner::git_script: `env -u GIT_ASKPASS -u SSH_ASKPASS
#     ... GIT_SSH_COMMAND="${GIT_SSH_COMMAND:-ssh} -o BatchMode=yes" git ...`, patches as base64 on stdin) runs under this user's LOGIN shell wrapped in `sh -c '...'`
#     (fish and nu parse the outer line), and does the right thing on the real host: commit chain + push to the bare origin, a fast-forward pull, `git apply
#     --cached --recount -` staging ONE hunk of two, a discard, the in-progress guard, and a remote that needs credentials fails fast with git's own words while a
#     GIT_ASKPASS that would leave a marker is never run.
#  2. APP layer (round 15, #2063): ONE of those operations, a commit, is made in the app the way a user makes it: the code-review panel is opened by keyboard
#     (the command palette's "Toggle code review"; the app answers hop=right_panel open=true), the panel's `Commit` button and the dialog's `Confirm` button
#     are clicked (the dialog has no key binding of its own: git_dialog/mod.rs binds Escape only), the message is typed between the two. Oracle: the host's
#     last commit carries that message AND the app logged `hop=backend_route family=git_write backend=<shell|helper>` (crates/remote_server/src/backend/
#     router.rs) with backend=shell on `nohelper` and backend=helper on `upload` (app_commit_verdict). Buttons are found by OCR and pressed with the Linux
#     pointer, so on macOS and Windows this layer is UNVERIFIABLE with `NEEDS: pointer synthesis`, and a button the OCR did not find is UNVERIFIABLE too; it
#     is never a pass and never a guess. push / pull / stage / discard through the app stay with the host layer and contract_tests.rs.
import base64, os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases  # noqa: E402
import cases_lab as CL  # noqa: E402
from cases_lab import lab_case, connect_or_fail, finish_remote, unique, work, rtt_s  # noqa: E402

import cases_lab_helper as CLH  # noqa: E402
CLH.declare_external("git_write", helper_case="lab_git_write_remote", nohelper_case="lab_git_write_remote")      # the coverage guard: both arms of the family

HOOK_REMOTE_OP = re.compile(r"hop=backend_route family=git_write backend=(helper|shell)")      # backend::router::route_log_line, logged by RoutedBackend for every call

# mirrors shellcmd::git::runner (GIT_UNSET_ENV, GIT_ENV, GIT_NOPROMPT_ENV, GIT_SSH_COMMAND_SH_ASSIGNMENT) and backend::shell_git_runner::git_script
ENV_PREFIX = 'env -u GIT_ASKPASS -u SSH_ASKPASS GIT_OPTIONAL_LOCKS=0 GIT_TERMINAL_PROMPT=0 SSH_ASKPASS_REQUIRE=never GIT_SSH_COMMAND="${GIT_SSH_COMMAND:-ssh} -o BatchMode=yes"'
GIT = ENV_PREFIX + " git -c diff.autoRefreshIndex=false"
# the first of two hunks of a.txt (seq 1 20 with line 1 and line 20 edited in the work tree)
PATCH = "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,4 +1,4 @@\n-1\n+ONE\n 2\n 3\n 4\n"
# shellcmd::git::operation_probe_script, verbatim for a git dir (parse::operation_probe_script)
PROBE = "for f in MERGE_HEAD CHERRY_PICK_HEAD REVERT_HEAD rebase-merge rebase-apply index.lock; do [ -e %s/\"$f\" ] && printf '%%s\\n' \"$f\"; done; true"


def sh_c(script):
    """the line the ControlMaster executor sends for a non-POSIX login shell: `sh -c '<script>'` (ShellKind::wrap). `script` holds no single quote or backslash, so
    the outer quoting is literal in bash, zsh, fish AND nu"""
    assert "'" not in script and "\\" not in script, script
    return "sh -c '%s'" % script


def run(lab, arm, script, timeout=60):
    """run a script on the lab host as the arm's user, through that user's login shell. Scripts with quotes or backslashes travel base64-encoded"""
    if "'" in script or "\\" in script:
        script = "printf %%s %s | base64 -d | sh" % base64.b64encode(script.encode()).decode()
    out = lab.ssh(arm["user"], sh_c(script), timeout=timeout)
    return out.returncode, out.stdout, out.stderr


@lab_case("git_write_remote", shells=("bash", "zsh", "fish", "nu", "dash", "ash", "pwsh"), lat=("lan",), helper=("nohelper", "upload"), modes=("terminal",))
def c_git_write_remote(ctx, r, arm):
    lab, rt = ctx.lab, rtt_s(arm)
    d = work(arm, "gw_%s" % unique(ctx, 6)).rstrip("/")
    repo, origin = d + "/w", d + "/origin.git"
    g = lambda script, **kw: run(lab, arm, "cd %s && %s" % (repo, script), **kw)

    # ---- fixture: a bare origin and a work repo holding a.txt = 1..20
    rc, out, err = run(lab, arm, " && ".join([
        "mkdir -p %s" % d, "cd %s" % d, "git init -q --bare -b main origin.git", "git init -q -b main w", "cd w", "git config user.email lab@example.com",
        "git config user.name lab", "git remote add origin %s" % origin, "seq 1 20 > a.txt"]))
    r.check(rc == 0, "fixture setup failed: rc=%s %r" % (rc, err[-300:]))
    if rc != 0:
        return

    # ---- 1. commit chain: commit then push --set-upstream; origin has the commit
    rc, out, err = g("%s add -A && %s commit -q -m first && %s push -q --set-upstream origin main" % (GIT, GIT, GIT))
    r.check(rc == 0, "commit + push failed: %r" % err[-300:])
    r.check(run(lab, arm, "git -C %s log --format=%%s main" % origin)[1].split() == ["first"], "origin does not hold exactly the first commit")

    # ---- 2. pull: a second clone pushes, a fast-forward pull brings it in; once the histories diverge the pull is refused with git's words
    other = "cd %s && git clone -q %s other && cd other && git config user.email lab@example.com && git config user.name lab" % (d, origin)
    rc, out, err = run(lab, arm, other + " && echo c > c.txt && git add -A && git commit -q -m third && git push -q origin main")
    r.check(rc == 0, "the second clone could not push: %r" % err[-300:])
    rc, out, err = g("%s pull --ff-only origin main" % GIT)
    r.check(rc == 0 and g("test -f c.txt")[0] == 0, "the ff-only pull did not bring the commit in: rc=%s %r" % (rc, err[-300:]))
    run(lab, arm, "cd %s/other && echo o > o.txt && git add -A && git commit -q -m other2 && git push -q origin main" % d)
    g("echo w > w.txt && %s add -A && %s commit -q -m work2" % (GIT, GIT))
    rc, out, err = g("%s pull --ff-only origin main" % GIT)
    r.check(rc != 0 and err.strip() != "", "a diverged pull must be refused with git's own words: rc=%s %r" % (rc, err))
    g("%s reset -q --hard origin/main" % GIT)

    # ---- 3. stage ONE hunk of two (git apply --cached --recount -, the patch as base64 on stdin), then take it back out with --reverse
    g("sed -i.bak -e 1s/1/ONE/ -e 20s/20/TWENTY/ a.txt && rm -f a.txt.bak")
    apply = "printf %%s %s | { base64 -d 2>/dev/null || base64 -D; } | %s apply --cached --recount {flag}-" % (base64.b64encode(PATCH.encode()).decode(), GIT)
    rc, out, err = g(apply.replace("{flag}", ""))
    cached = g("%s diff --cached" % GIT)[1]
    r.check(rc == 0 and "+ONE" in cached and "TWENTY" not in cached, "exactly the first hunk must be staged: rc=%s %r %r" % (rc, err[-200:], cached[-300:]))
    r.check("TWENTY" in g("%s diff" % GIT)[1], "the second hunk must stay unstaged")
    rc, out, err = g(apply.replace("{flag}", "--reverse "))
    r.check(rc == 0 and g("%s diff --cached" % GIT)[1].strip() == "", "--reverse did not take the hunk back out: %r" % err[-200:])

    # ---- 4. discard: restore the modified file, remove an untracked one
    g("echo u > new.txt")
    rc, out, err = g("%s restore --staged --worktree --source=HEAD -- a.txt && %s clean -fd new.txt" % (GIT, GIT))
    status = g("%s status --porcelain" % GIT)[1]
    r.check(rc == 0 and "a.txt" not in status and "new.txt" not in status, "the discard left the tree dirty: %r %r" % (status, err[-200:]))

    # ---- 5. the in-progress guard: operation_probe_script prints the sentinel that is there
    probe = PROBE % (repo + "/.git")
    r.check(run(lab, arm, probe)[1].strip() == "", "the probe reports a sentinel in a clean repo")
    run(lab, arm, "touch %s/.git/MERGE_HEAD" % repo)
    r.check(run(lab, arm, probe)[1].split() == ["MERGE_HEAD"], "the probe did not see MERGE_HEAD")
    run(lab, arm, "rm -f %s/.git/MERGE_HEAD" % repo)

    # ---- 6. credentials never prompt: an unreachable ssh remote, an askpass that would leave a marker. Fails fast, git's words, no marker
    marker = d + "/askpass_ran"
    run(lab, arm, "printf '#!/bin/sh\\ntouch %s\\n' > %s/askpass.sh; chmod +x %s/askpass.sh" % (marker, d, d))
    g("git remote set-url origin ssh://git@127.0.0.1:1/none.git && echo x > y.txt && %s add -A && %s commit -q -m fourth" % (GIT, GIT))
    t0 = time.time()
    rc, out, err = g("GIT_ASKPASS=%s/askpass.sh SSH_ASKPASS=%s/askpass.sh %s push --set-upstream origin main" % (d, d, GIT))
    r.check(rc != 0 and err.strip() != "", "a push that cannot authenticate must fail with git's words: rc=%s %r" % (rc, err))
    r.check(time.time() - t0 < 30, "the failing push took %.1f s: something waited for a login" % (time.time() - t0))
    r.check(run(lab, arm, "test -e %s && echo ran || echo clean" % marker)[1].strip() == "clean", "the askpass helper ran: credentials prompted")

    # ---- 7. the app layer (see the header): one commit made in the app's code-review panel
    off = ctx.log_size()
    if connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    time.sleep(1.0 + 2 * rt)
    why = CL.plain_session_feature_limit(arm["shell"], ctx.log_since(off), "the code-review git dialog of a remote repository")
    if why:
        r.metric("plain_session_by_decision", arm["shell"])
        r.unverifiable(why)
        finish_remote(ctx, r)
        return
    want = "helper" if arm["helper"] == "upload" else "shell"
    msg = "lab%s" % unique(ctx, 7)
    g("echo drv > drv_%s.txt" % msg)                                   # something to commit
    ctx.be.type("cd %s" % repo, CL.type_pace(ctx), enter=True)
    time.sleep(1.5 + 2 * rt)
    how, detail = drive_commit(ctx, r, arm, msg, off)
    subject = lambda: g("git log -1 --format=%s")[1].strip()
    if how == "driven":
        ctx.until(lambda: subject() == msg, 20 + 6 * rt, step=0.5)
    seen = [m.group(1) for m in HOOK_REMOTE_OP.finditer(ctx.log_since(off))]
    verdict, text = app_commit_verdict(how, detail, seen, want, subject() == msg)
    if verdict == "fail":
        r.check(False, text)
    elif verdict == "noref":
        r.unverifiable(text)
    else:
        r.note(text)
    if how in ("driven", "nobutton"):
        ctx.be.guard()
        ctx.be.key("Escape")                                            # a dialog still open is cancelled (git_dialog: escape = Cancel)
        time.sleep(0.4)
    if how in ("driven", "nobutton"):
        CL.close_code_review(ctx, r, "lab_git_write")
    ctx.reset()
    finish_remote(ctx, r)


COMMIT_WORD, CONFIRM_WORD = "Commit", "Confirm"       # app/i18n/en/warp.ftl common-commit / common-confirm: the panel's button and the dialog's (self-test pins both)


def drive_commit(ctx, r, arm, msg, off):
    """the commit a user makes in the code-review panel: open the panel (keyboard), click `Commit`, type the message, click `Confirm`.
    -> (how, detail): 'nopointer' (this backend has no pointer: the buttons cannot be pressed), 'nopanel' (the app did not say the panel opened),
    'nobutton' (a button was not found on screen), 'driven' (both clicks were made). Only 'driven' goes on to the oracle; nothing here is a verdict."""
    import cases_ui as U
    if ctx.win or ctx.be.name not in ("x11", "wayland"):
        return "nopointer", ctx.be.name
    try:
        m = U.mouse_or_skip(ctx, r)
    except cases.Skip as e:
        return "nopointer", str(e)
    if CL.open_code_review(ctx, off, "lab_git_write") != "open":
        return "nopanel", ""
    time.sleep(2.0 + 4 * CL.rtt_s(arm))
    for word, tag, after in ((COMMIT_WORD, "lab_git_write_panel", lambda: ctx.be.type(msg, 15)), (CONFIRM_WORD, "lab_git_write_dialog", None)):
        ws, _p = U.words(ctx, tag)
        hits = [w for w in ws if w[0].strip() == word]
        if not hits:
            return "nobutton", word
        w = hits[-1] if word == CONFIRM_WORD else hits[0]
        m.click(w[1], w[2])
        time.sleep(1.2)
        if after:
            after()
            time.sleep(0.6)
    return "driven", ""


def app_commit_verdict(how, detail, routed, want, committed):
    """pure: the app layer's verdict. ('pass' | 'fail' | 'noref', text). `routed`: the backends of the app's git_write calls; `committed`: the host's last
    commit carries the message that was typed.
      * a git_write call on the wrong backend is a FAIL however the driver fared;
      * a commit on the host through the right backend is the pass;
      * the app routed a git_write call and the host has no such commit: the app tried and it did not land, FAIL;
      * nothing routed and nothing committed: the driver did not reach the dialog (or could not: no pointer). UNVERIFIABLE with what is missing."""
    wrong = sorted({b for b in routed if b != want})
    if wrong:
        return "fail", "the app's git write ran on the %s backend, expected every one on %s (%d call(s))" % (", ".join(wrong), want, len(routed))
    if committed and routed:
        return "pass", "the commit made in the code-review panel is on the host, through the %s backend (%d git_write call(s))" % (want, len(routed))
    if committed:
        return "fail", "[hook] the commit typed in the app is on the host and the app logged no `hop=backend_route family=git_write` line: the route is not logged"
    if routed:
        return "fail", "the app sent %d git_write call(s) to the %s backend and the host's last commit is not the one typed in the dialog" % (len(routed), want)
    if how == "nopointer":
        return "noref", ("the code-review git dialog is reached by its buttons only (no key binding opens it) and this backend has no pointer (%s): the app-driven "
                         "commit was not run; the host layer above did. NEEDS: pointer synthesis in this backend (docs/design/r14-test-gaps.md H1 Windows, H2 macOS)" % detail)
    if how == "nopanel":
        return "noref", ("the app did not log hop=right_panel open=true after the palette's '%s': the panel's Commit button was not looked for. NEEDS: an app build "
                         "with the round 15 trace line (workspace/view/right_panel.rs)" % CL.PALETTE_CODE_REVIEW)
    if how == "nobutton":
        return "noref", "the word %r was not found on screen (OCR; shots/*lab_git_write_*.png): the pointer could not be placed, the app-driven commit was not run" % detail
    return "noref", ("the pointer clicked `%s` and `%s` and the app made no git_write call: whether the dialog opened is not known (shots/*lab_git_write_*.png); "
                     "not judged" % (COMMIT_WORD, CONFIRM_WORD))
