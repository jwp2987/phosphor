# appci/cases_win_input.py: the Windows input-parity cases (docs/design/windows-input-parity-audit.md, findings 3, 7, 8; issues #1884, #1888, #1889).
# Registered with the same @case decorator as cases.py (one import line at its end). Windows panes only; each SKIPs elsewhere with its reason.
#   ssh_connect_typing_wrapper / ssh_nested_typeahead_wrapper   Git Bash pane, the managed-ssh wrapper pinned on (APPCI_SSH_WRAPPER=1 in the profile):
#                                                               the existing ssh cases, plus proof from the app log that the wrapper announcement was ACCEPTED
#                                                               on the born-owned ConPTY window (hop ssh_wrapper "announced reclaimed=true", #1884)
#   ssh_gitbash_managed / ssh_gitbash_typeahead_plain /         Git Bash pane, the DEFAULT settings (#1933, owner 2026-11-09: managed ssh from Git Bash on ONE
#   ssh_gitbash_kill_switch                                     connection): the session is managed (stage hops auth, auth_done, managed), keys typed during the
#                                                               login make that connection plain and stay in typed order, PHOSPHOR_MSYS_MANAGED_SSH=0 is plain ssh
#   ps_ctrl_break                                               PowerShell pane: Ctrl+Break reaches a program as CTRL_BREAK_EVENT (#1888)
#   ps_fast_command_young_enter                                 PowerShell pane: a fast command's Enter and the next line typed at once run once each, in order (#1889)
import os, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases  # noqa: E402
from cases import case  # noqa: E402
import cases_caseremote as R  # noqa: E402  (connect / leave / remote rec.log helpers)

# the receiver for ps_ctrl_break: records which console signal arrived, then ends. SIGBREAK is Windows-only.
BREAK_PROBE = """import signal, sys, time
out = sys.argv[1]
def note(tag):
    with open(out, 'a') as f:
        f.write(tag + '\\n')
signal.signal(signal.SIGBREAK, lambda *a: (note('BREAK'), sys.exit(0)))
signal.signal(signal.SIGINT, lambda *a: (note('INT'), sys.exit(0)))
note('READY')
t = time.time()
while time.time() - t < 30:
    time.sleep(0.05)
note('TIMEOUT')
"""


def _need_windows_pane(ctx, r, pane):
    if not ctx.win:
        r.skip("Windows only (ConPTY)")
    if ctx.pane != pane:
        r.skip("runs in the %s pane" % pane)


def _wrapper_variant(ctx, r, base):
    _need_windows_pane(ctx, r, "gitbash")
    if os.environ.get("APPCI_SSH_WRAPPER", "") != "1":
        r.skip("set APPCI_SSH_WRAPPER=1 so the profile pins the managed-ssh wrapper on (#1884)")
    off = ctx.log_size()
    base(ctx, r)
    log = ctx.log_since(off)
    wrapper = [ln for ln in log.splitlines() if "hop=ssh_wrapper" in ln]
    r.check(any("announced" in ln for ln in wrapper),
            "the wrapper announcement was handled on the Windows window (hop ssh_wrapper announced); hops seen: %d" % len(wrapper))
    r.check(not any("verdict_ignored" in ln for ln in wrapper),
            "no wrapper verdict was ignored on the born-owned window (#1884): %r" % [ln[-120:] for ln in wrapper if "verdict_ignored" in ln][:2])


@case("ssh_connect_typing_wrapper", order=30)
def c_ssh_connect_typing_wrapper(ctx, r):
    """#1884: ssh_connect_typing with the managed-ssh wrapper pinned on, Git Bash pane"""
    _wrapper_variant(ctx, r, cases.c_ssh)


@case("ssh_nested_typeahead_wrapper", order=30)
def c_ssh_nested_typeahead_wrapper(ctx, r):
    """#1884: ssh_nested_typeahead with the managed-ssh wrapper pinned on, Git Bash pane"""
    _wrapper_variant(ctx, r, cases.c_ssh_nested)


@case("ps_ctrl_break", order=19)
def c_ps_ctrl_break(ctx, r):
    """#1888: a Python program installs SIGBREAK / SIGINT handlers and waits; Ctrl+Break must reach it as SIGBREAK (the console's CTRL_BREAK_EVENT), not
    as a Ctrl-C and not be dropped. Samples are independent runs; every one must write BREAK."""
    _need_windows_pane(ctx, r, "powershell")
    probe, out = "break_probe.py", "brk.txt"
    with open(os.path.join(ctx.W, probe), "w") as f:
        f.write(BREAK_PROBE)
    py = sys.executable.replace("\\", "/")
    trials, got, wrote = 3, [], []
    for i in range(trials):
        ctx.rm(out)
        ctx.be.type("& '%s' '%s' '%s'" % (py, os.path.join(ctx.W, probe).replace("\\", "/"), os.path.join(ctx.W, out).replace("\\", "/")), 10, enter=True)
        if not ctx.until(lambda: "READY" in ctx.lines(out), 15):
            r.check(False, "trial %d: the probe program never started" % (i + 1))
            ctx.be.key("ctrl+c")
            time.sleep(1.0)
            continue
        off = ctx.log_size()
        ctx.be.key("ctrl+break")
        ctx.until(lambda: any(t in ctx.lines(out) for t in ("BREAK", "INT")), 6)
        seen = ctx.lines(out)
        got.append(seen)
        wrote.append("3;70" in ctx.log_since(off))
        if "BREAK" not in seen and "INT" not in seen:
            ctx.be.key("ctrl+c")           # leave the pane usable
            time.sleep(1.0)
        time.sleep(0.5)
    r.metric("ctrl_break_runs", got)
    # round 8: 0/3 with NO ConPTY write in the app log = the key never became the win32-input sequence (injection or key mapping), not a ConPTY/console fault
    r.check(all(wrote) and len(wrote) == trials,
            "the app wrote the Ctrl+Break win32-input sequence (CSI 3;70;...) to the pty in %d/%d runs (the app log's pty_write_bytes)" % (sum(wrote), trials))
    r.check(all("BREAK" in g for g in got) and len(got) == trials,
            "Ctrl+Break reached the program as SIGBREAK in %d/%d runs: %r" % (sum("BREAK" in g for g in got), trials, got))
    r.check(not any("INT" in g for g in got), "Ctrl+Break was not turned into a Ctrl-C: %r" % got)
    ctx.reset()


@case("ps_fast_command_young_enter", order=19)
def c_ps_fast_command_young_enter(ctx, r):
    """#1889: PowerShell pane, a command that finishes at once and the next line typed with no pause: the second Enter is a young Enter (the first command's
    prompt is still in the ConPTY pipe). Each line must run exactly once, in order, never twice, never dropped."""
    _need_windows_pane(ctx, r, "powershell")
    f = "yng.txt"
    ctx.rm(f)
    exp = []
    for _ in range(12):
        a, b = ctx.tok(6), ctx.tok(6)
        exp += [a, b]
        ctx.be.type(ctx.append_cmd("pwsh", a, f), 8, enter=True)
        ctx.be.type(ctx.append_cmd("pwsh", b, f), 8, enter=True)
        ctx.until(lambda: len(ctx.lines(f)) >= len(exp), 8)
    time.sleep(0.5)
    got = ctx.lines(f)
    r.check(got == exp, "%d/%d lines exact and in order (got %d lines); first difference: %r" % (
        sum(1 for x, y in zip(exp, got) if x == y), len(exp), len(got), next(((x, y) for x, y in zip(exp, got) if x != y), None)))
    ctx.reset()


# ------------------------------------------------------------------ #1933: managed ssh from Git Bash on one connection
def _wrapper_hops(ctx, off):
    return [ln for ln in ctx.log_since(off).splitlines() if "hop=ssh_wrapper" in ln]


def _sync_stages(hops):
    """the accepted one-connection stages, in log order ("sync stage=<name> ..."; an ignored one logs "sync_ignored stage=...")"""
    out = []
    for ln in hops:
        i = ln.find(" sync stage=")
        if i < 0:
            i = ln.find("=sync stage=")
        if i >= 0:
            out.append(ln[i:].split("stage=", 1)[1].split()[0])
    return out


def _remote_line_once(ctx, r, what):
    """one fresh line typed at the remote prompt runs exactly once in the REMOTE shell and never locally"""
    tok = ctx.tok(6)
    rmark, lmark = len(ctx.prof.commands(remote=True)), ctx.cmd_mark()
    ctx.be.type("echo %s" % tok, 12, enter=True)
    ctx.until(lambda: R.remote_cmds_after(ctx, rmark), 10)
    time.sleep(0.5)
    got = R.remote_cmds_after(ctx, rmark)
    r.check(got == ["echo %s" % tok], "%s: the remote shell received %r, want exactly one %r" % (what, got, "echo %s" % tok))
    r.check(ctx.cmds_since(lmark) == [], "%s: the LOCAL shell received commands while the remote shell was in front: %r" % (what, ctx.cmds_since(lmark)))


@case("ssh_gitbash_managed", order=62)
def c_ssh_gitbash_managed(ctx, r):
    """#1933 (owner 2026-11-09): `ssh fake` from the Git Bash pane with the DEFAULT settings is a MANAGED session on one connection (msys ssh.exe cannot
    multiplex). Nothing is typed while it connects. Evidence, all from the app log and the shells' own rec.log: the wrapper hops show the stages auth,
    auth_done, managed in that order, none ignored, and `verdict managed=true` (round 8 showed `managed=false` on every Git Bash ssh); the pane is
    provably remote; a line typed at the remote prompt runs once there and never locally. The in-band Tab itself is `remote_tab_no_helper`."""
    _need_windows_pane(ctx, r, "gitbash")
    R.need_ssh(ctx, r)
    off = ctx.log_size()
    secs = R.connect(ctx, "fake", 60)
    r.check(secs is not None, "no remote shell prompt after `ssh fake`. Test host: %s" % (ctx.ssh_diag() if secs is None else ""))
    if secs is None:
        ctx.reset()
        return
    r.metric("gitbash_managed_connect_s", round(secs, 2))
    time.sleep(1.0)
    if ctx.where_now() != "remote":
        r.check(False, "precondition: the pane is not provably in the REMOTE shell after `ssh fake` (probe ran in: %s)" % ctx.where_now())
        ctx.reset()
        return
    hops = _wrapper_hops(ctx, off)
    stages = _sync_stages(hops)
    r.check(stages == ["auth", "auth_done", "managed"], "one-connection stages in the app log: %r, want auth, auth_done, managed (hops: %d)" % (stages, len(hops)))
    r.check(any("verdict managed=true" in ln for ln in hops), "the wrapper's verdict was managed=true (round 8: managed=false, #1933)")
    r.check(not any("sync_ignored" in ln or "verdict_ignored" in ln for ln in hops),
            "no stage or verdict was ignored: %r" % [ln[-140:] for ln in hops if "_ignored" in ln][:3])
    ctx.shot("gitbash_managed_remote_prompt")
    _remote_line_once(ctx, r, "managed session")
    if not R.leave(ctx):
        r.check(False, "not provably remote (probe ran in: %s), `exit` NOT typed" % ctx.left_unproven)
    ctx.reset()


@case("ssh_gitbash_typeahead_plain", order=62)
def c_ssh_gitbash_typeahead_plain(ctx, r):
    """#1933: a line typed blind right after `ssh fake` + Enter (no pause, 10-25 ms keys). With one connection those keys are in the console before the
    session exists; they must never meet the bootstrap. The app's rule: ssh's LocalCommand drains them, the session is PLAIN for this connection, and
    the keys reach the remote shell in typed order. Evidence: the typed line ran exactly once REMOTELY, never locally; and the stages in the app log
    are consistent with what happened: either `plain` (keys were found in front of the token) or `managed` (ssh's login consumed or flushed nothing
    and the keys were typed after auth_done, i.e. held and delivered at the first prompt). Which of the two is a note, not a verdict: it depends on
    how fast the login was. A line lost, doubled, cut or run locally fails either way."""
    _need_windows_pane(ctx, r, "gitbash")
    R.need_ssh(ctx, r)
    trials, plain, managed = 3, 0, 0
    for i in range(trials):
        tok = ctx.tok(6)
        line = "echo %s" % tok
        rmark, lmark = len(ctx.prof.commands(remote=True)), ctx.cmd_mark()
        before = R.rprompts(ctx)
        off = ctx.log_size()
        ctx.be.type("ssh fake", 12, enter=True)
        ctx.be.type(line, ctx.rng.randint(10, 25), enter=True)
        ok = ctx.until(lambda: R.remote_cmds_after(ctx, rmark), 60)
        r.check(bool(ok), "trial %d: the line typed during the connect never reached the remote shell. Test host: %s" % (i + 1, "" if ok else ctx.ssh_diag()))
        ctx.until(lambda: R.rprompts(ctx) > before, 20, step=0.1)
        time.sleep(1.0)
        got = R.remote_cmds_after(ctx, rmark)
        r.check(got == [line], "trial %d: the remote shell received %r, want exactly [%r]" % (i + 1, got, line))
        local = [c for c in ctx.cmds_since(lmark) if not c.startswith("ssh ")]
        r.check(local == [], "trial %d: the LOCAL shell received %r" % (i + 1, local))
        stages = _sync_stages(_wrapper_hops(ctx, off))
        r.check(stages in (["auth", "auth_done", "plain"], ["auth", "auth_done", "managed"]),
                "trial %d: one-connection stages in the app log: %r" % (i + 1, stages))
        plain += stages[-1:] == ["plain"]
        managed += stages[-1:] == ["managed"]
        if ctx.where_now() == "remote":
            if not R.leave(ctx):
                r.check(False, "trial %d: `exit` did not bring the local prompt back" % (i + 1))
        else:
            r.check(False, "trial %d: the pane is not provably remote after the connect (probe ran in: %s)" % (i + 1, ctx.where_now()))
            ctx.reset()
    r.note("typed-during-connect outcome: %d plain, %d managed of %d (plain = the keys were in the console before the session; #1933 design 3.2)" % (
        plain, managed, trials))
    ctx.reset()


@case("ssh_gitbash_kill_switch", order=62)
def c_ssh_gitbash_kill_switch(ctx, r):
    """#1933: PHOSPHOR_MSYS_MANAGED_SSH=0 in the shell turns the one-connection path off for that shell: the ssh is the user's plain ssh (verdict
    managed=false, no stage hops), and a line typed at the remote prompt still runs once there. The variable is set in the pane's shell and unset again."""
    _need_windows_pane(ctx, r, "gitbash")
    R.need_ssh(ctx, r)
    ctx.be.type("export PHOSPHOR_MSYS_MANAGED_SSH=0", 12, enter=True)
    time.sleep(0.5)
    off = ctx.log_size()
    secs = R.connect(ctx, "fake", 60)
    r.check(secs is not None, "no remote shell prompt after `ssh fake` with the kill switch. Test host: %s" % (ctx.ssh_diag() if secs is None else ""))
    if secs is not None:
        time.sleep(1.0)
        hops = _wrapper_hops(ctx, off)
        r.check(_sync_stages(hops) == [], "no one-connection stage with the kill switch: %r" % _sync_stages(hops))
        r.check(any("verdict managed=false" in ln for ln in hops), "the wrapper's verdict was the plain fallback (managed=false)")
        if ctx.where_now() == "remote":
            _remote_line_once(ctx, r, "plain session")
            if not R.leave(ctx):
                r.check(False, "not provably remote (probe ran in: %s), `exit` NOT typed" % ctx.left_unproven)
        else:
            r.check(False, "the pane is not provably remote after `ssh fake` (probe ran in: %s)" % ctx.where_now())
    ctx.reset()
    ctx.be.type("unset PHOSPHOR_MSYS_MANAGED_SSH", 12, enter=True)
    time.sleep(0.5)
