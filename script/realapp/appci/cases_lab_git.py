# appci/cases_lab_git.py: the git WRITE family over ssh (wave H parity P7, family f): commit, push, pull, stage a hunk, discard, with a bare repo in the lab as
# the remote. Arms: lab_git_write_remote.<bash|zsh|fish|nu>.lan.<nohelper|upload>. `nohelper` is the shell backend (the primary design), `upload` the helper.
#
# What it proves, in two layers:
#  1. HOST layer (always runs, needs only the lab): the exact script shape the shell backend sends (ShellGitRunner::git_script: `env -u GIT_ASKPASS -u SSH_ASKPASS
#     ... GIT_SSH_COMMAND="${GIT_SSH_COMMAND:-ssh} -o BatchMode=yes" git ...`, patches as base64 on stdin) runs under this user's LOGIN shell wrapped in `sh -c '...'`
#     (fish and nu parse the outer line), and does the right thing on the real host: commit chain + push to the bare origin, a fast-forward pull, `git apply
#     --cached --recount -` staging ONE hunk of two, a discard, the in-progress guard, and a remote that needs credentials fails fast with git's own words while a
#     GIT_ASKPASS that would leave a marker is never run.
#  2. APP layer [hook]: the same operations driven in the app (code review > commit / push / pull / stage / discard) must log
#     `hop=remote_op name=<method> backend=<shell|helper> ok=true` (crates/remote_server/src/backend/features.rs) with backend=shell on `nohelper` and
#     backend=helper on `upload`. The code-review git dialog has no keyboard path the harness can use yet, so that layer is UNVERIFIABLE, never a pass; the log
#     lines it will read are pinned by the self-test.
import base64, os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases_lab as CL  # noqa: E402
from cases_lab import lab_case, connect_or_fail, finish_remote, unique, work, rtt_s  # noqa: E402

HOOK_REMOTE_OP = re.compile(r"hop=remote_op name=(git_commit_chain|git_push|git_pull|git_create_pr|discard_files|git_stage) backend=(helper|shell) ok=(true|false)")

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


@lab_case("git_write_remote", shells=("bash", "zsh", "fish", "nu"), lat=("lan",), helper=("nohelper", "upload"), modes=("terminal",))
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
    apply = "printf %%s %s | { base64 -d 2>/dev/null || base64 -D; } | %s apply --cached --recount %s-" % (base64.b64encode(PATCH.encode()).decode(), GIT, "%s")
    rc, out, err = g(apply % "")
    cached = g("%s diff --cached" % GIT)[1]
    r.check(rc == 0 and "+ONE" in cached and "TWENTY" not in cached, "exactly the first hunk must be staged: rc=%s %r %r" % (rc, err[-200:], cached[-300:]))
    r.check("TWENTY" in g("%s diff" % GIT)[1], "the second hunk must stay unstaged")
    rc, out, err = g(apply % "--reverse ")
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

    # ---- 7. [hook] the app layer (see the header)
    if connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    time.sleep(1.0 + 2 * rt)
    ctx.be.type("cd %s" % repo, CL.type_pace(ctx), enter=True)
    time.sleep(0.5 + rt)
    seen = [m.groups() for m in HOOK_REMOTE_OP.finditer(ctx.log_since(0))]
    want = "helper" if arm["helper"] == "upload" else "shell"
    if not seen:
        r.unverifiable("no keyboard path to the code-review git dialog: the app-driven commit/push/pull/stage/discard were not run (the host layer above did)")
    else:
        r.check(all(b == want for _, b, _ in seen), "operations ran on %r, expected every one on the %s backend" % (seen, want))
    finish_remote(ctx, r)
