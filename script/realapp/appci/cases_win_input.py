# appci/cases_win_input.py: the Windows input-parity cases (docs/design/windows-input-parity-audit.md, findings 3, 7, 8; issues #1884, #1888, #1889).
# Registered with the same @case decorator as cases.py (one import line at its end). Windows panes only; each SKIPs elsewhere with its reason.
#   ssh_connect_typing_wrapper / ssh_nested_typeahead_wrapper   Git Bash pane, the managed-ssh wrapper pinned on (APPCI_SSH_WRAPPER=1 in the profile) and the
#                                                               one-connection managed ssh switched on in the pane's shell (#2051): the existing ssh cases, plus
#                                                               proof from the app log that the wrapper announcement was ACCEPTED on the born-owned ConPTY
#                                                               window (hop ssh_wrapper "announced reclaimed=true", #1884) and that the verdict was managed
#   ssh_gitbash_managed / ssh_gitbash_typeahead_managed         Git Bash pane, PHOSPHOR_MSYS_MANAGED_SSH=1 typed into the pane's shell (#2051, owner 2026-11-10:
#                                                               managed ssh from Git Bash on ONE connection, #1933, is OFF unless the shell has that): the
#                                                               session is managed (stage hops auth, auth_done, managed), keys typed during the login stay
#                                                               managed and reach the first remote prompt first, in typed order (#1959)
#   ssh_gitbash_kill_switch                                     Git Bash pane: the DEFAULT (variable not set) and PHOSPHOR_MSYS_MANAGED_SSH=0 are both the
#                                                               user's plain ssh: no stage hop, verdict managed=false
#   ssh_gitbash_bootstrap_in_command                            Git Bash pane, managed ssh switched on: the remote bash gets its bootstrap inside the remote
#                                                               command (#2051): the app writes nothing bootstrap-sized and never gives up on the bootstrap
#   gitbash_long_osc_reaches_app                                Git Bash pane: an escape sequence over 256 bytes printed by an msys program in the pane reaches
#                                                               the app whole (#2007: the form of a managed ssh session's remote hooks, which arrive through
#                                                               msys ssh.exe; kernel32's pseudo console of Windows 10 passes on only its tail)
#   ps_ctrl_break / ps_ctrl_scrolllock_break                    PowerShell pane: the Break key and ScrollLock, each with Ctrl held (both VK_CANCEL), reach
#                                                               a program as CTRL_BREAK_EVENT; a failed trial names the app hop it stopped at (#1888, #1906)
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
    # #2051: the managed path these two cases are about is off by default; they switch it on for the pane's shell
    R.in_msys_managed_ssh(ctx, lambda: base(ctx, r))
    log = ctx.log_since(off)
    wrapper = [ln for ln in log.splitlines() if "hop=ssh_wrapper" in ln]
    r.check(any("verdict managed=true" in ln for ln in wrapper),
            "with the one-connection managed ssh switched on, the wrapper's verdict was managed=true; verdicts seen: %r"
            % [ln.split("hop=ssh_wrapper", 1)[-1].strip()[:60] for ln in wrapper if "verdict" in ln][:4])
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


# #1906: the hops of one Ctrl+Break through the app, in order, as (name, what the app log must contain). The first one missing is where the key stopped.
BREAK_HOPS = (
    ("keydown", "input_order keydown key=pause ctrl=true"),   # winit named the key and it reached dispatch
    ("pane_key", "hop=kh_pane_key"),                             # the pane's key sink saw it
    ("pty_write", "3;70"),                                       # hop=pty_write_bytes carrying the win32-input sequence CSI 3;70;...
)


def break_hops(log):
    """pure: ({hop name: seen}, the first hop missing or None, the app's own `key_unconverted` lines) for the log text after one injected Ctrl+Break"""
    seen = {name: (needle in log) for name, needle in BREAK_HOPS}
    stopped = next((name for name, _ in BREAK_HOPS if not seen[name]), None)
    dropped = [ln.split("input_order ", 1)[-1].strip() for ln in log.splitlines() if "hop=key_unconverted" in ln]
    return seen, stopped, dropped


def _ctrl_break(ctx, r, chord, label):
    """A Python program installs SIGBREAK / SIGINT handlers and waits; `chord` must reach it as SIGBREAK (the console's CTRL_BREAK_EVENT), not as a
    Ctrl-C and not be dropped. Samples are independent runs; every one must write BREAK, and the app log must show the win32-input write each time."""
    _need_windows_pane(ctx, r, "powershell")
    probe, out = "break_probe.py", "brk.txt"
    with open(os.path.join(ctx.W, probe), "w") as f:
        f.write(BREAK_PROBE)
    py = sys.executable.replace("\\", "/")
    trials, got, wrote, stops = 3, [], [], []
    for i in range(trials):
        ctx.rm(out)
        ctx.be.type("& '%s' '%s' '%s'" % (py, os.path.join(ctx.W, probe).replace("\\", "/"), os.path.join(ctx.W, out).replace("\\", "/")), 10, enter=True)
        if not ctx.until(lambda: "READY" in ctx.lines(out), 15):
            r.check(False, "trial %d: the probe program never started" % (i + 1))
            ctx.be.key("ctrl+c")
            time.sleep(1.0)
            continue
        off = ctx.log_size()
        ctx.be.key(chord)
        ctx.until(lambda: any(t in ctx.lines(out) for t in ("BREAK", "INT")), 6)
        seen = ctx.lines(out)
        got.append(seen)
        hops, stopped, dropped = break_hops(ctx.log_since(off))
        wrote.append(hops["pty_write"])
        # #1906: say where this trial stopped, so a failed run names the hop from the job log alone (rounds 8 and 9 could only say "no write")
        stops.append("trial %d: %s%s" % (i + 1, "all app hops seen" if stopped is None else "stopped before `%s`" % stopped,
                                         "; app dropped the key: %s" % dropped[0][:160] if dropped else ""))
        if "BREAK" not in seen and "INT" not in seen:
            ctx.be.key("ctrl+c")           # leave the pane usable
            time.sleep(1.0)
        time.sleep(0.5)
    r.metric("ctrl_break_runs", got)
    r.metric("ctrl_break_hops", stops)
    # round 8: 0/3 with NO ConPTY write in the app log = the key never became the win32-input sequence (injection or key mapping), not a ConPTY/console fault
    r.check(all(wrote) and len(wrote) == trials,
            "the app wrote the %s win32-input sequence (CSI 3;70;...) to the pty in %d/%d runs (the app log's pty_write_bytes); %s"
            % (label, sum(wrote), trials, " | ".join(stops)))
    r.check(all("BREAK" in g for g in got) and len(got) == trials,
            "%s reached the program as SIGBREAK in %d/%d runs: %r" % (label, sum("BREAK" in g for g in got), trials, got))
    r.check(not any("INT" in g for g in got), "%s was not turned into a Ctrl-C: %r" % (label, got))
    ctx.reset()


@case("ps_ctrl_break", order=19)
def c_ps_ctrl_break(ctx, r):
    """#1888 / #1906: the Break key with Ctrl held (VK_CANCEL, scan E0 46) reaches a Python program as SIGBREAK, 3/3, and the app log shows the
    win32-input write each time."""
    _ctrl_break(ctx, r, "ctrl+break", "Ctrl+Break")


@case("ps_ctrl_scrolllock_break", order=19)
def c_ps_ctrl_scrolllock_break(ctx, r):
    """#1906: Ctrl+ScrollLock (VK_CANCEL, scan 46, not extended: what a console also reads as Ctrl+Break, and what the injector sent as `ctrl+break`
    in rounds 8 and 9) reaches the program as SIGBREAK the same way."""
    _ctrl_break(ctx, r, "ctrl+scrollbreak", "Ctrl+ScrollLock")


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


def long_osc_hook(session_id, pad_to):
    """the bytes of one hex hook, `ESC ] 9278 ; d ; <hex JSON> BEL`: a Clear hook for a session id the app does not know, so the app refuses it and
    logs the id (`Rejected DCS hook 'Clear' with unrecognized session_id <id>`), nothing else happens. JSON whitespace pads the sequence to at
    least `pad_to` bytes (0: no padding)."""
    js = '{"hook": "Clear", "value": {%s"session_id": %d}}'
    body = js % ("", session_id)
    if pad_to:
        body = js % (" " * max(0, (pad_to - 10 + 1) // 2 - len(body)), session_id)
    return b"\x1b]9278;d;" + body.encode().hex().encode() + b"\x07"


def long_osc_script(short, long_, done_file, tok):
    """the bash script of gitbash_long_osc_reaches_app: prints both sequences from an msys program to the pane's terminal, then leaves its token"""
    esc = lambda b: "".join("\\%03o" % c for c in b)
    return "printf '%s'\nprintf '%s'\necho %s >> %s\n" % (esc(short), esc(long_), tok, done_file)


def long_osc_verdict(log, short_id, long_id):
    """(ok, text) from the app log of the case: which of the two refused hooks the app logged"""
    seen = lambda n: ("unrecognized session_id %d" % n) in log
    if seen(short_id) and seen(long_id):
        return True, "the app parsed both hooks: an escape sequence over 256 bytes printed by an msys program in the pane arrives whole"
    if seen(short_id):
        return False, ("APP side: the short hook reached the app, the long one (over 256 bytes, the size of a managed ssh session's SSH and InitShell hooks) "
                       "did not: an msys program cannot hand the app a long escape sequence through this pane's console, and msys ssh.exe delivers the "
                       "remote's hooks the same way, so a managed ssh session from Git Bash cannot start whatever the server is (#1933, #2007)")
    return False, ("TEST: the short control hook was not logged either (looked for `unrecognized session_id %d` in the app log): the oracle did not work "
                   "here, nothing is known about the long one" % short_id)


@case("gitbash_long_osc_reaches_app", order=29)
def c_gitbash_long_osc_reaches_app(ctx, r):
    """#2007: the LOCAL leg of a managed ssh session from Git Bash, measured without a server. The remote's SSH and InitShell hooks are single escape
    sequences of about 470 and 280 bytes, and they reach the app through msys ssh.exe writing to the pane's console. The msys console layer hands a
    sequence over in 256-byte pieces; kernel32's pseudo console of Windows 10 then passes on only the last piece (round 9: the test sshd, which used
    that console, never got the hooks out). The app's panes use the app's own console host, which should pass them whole: this case proves or refutes
    that. A bash script (an msys program, like ssh.exe) prints two Clear hooks for session ids the app does not know: a short one (the control: under
    256 bytes) and a long one (480 bytes). The app refuses both and logs each id; nothing else happens. Required: both ids in the app log."""
    _need_windows_pane(ctx, r, "gitbash")
    short_id, long_id = ctx.probe_rng.randrange(10 ** 14, 10 ** 15), ctx.probe_rng.randrange(10 ** 14, 10 ** 15)
    short, long_ = long_osc_hook(short_id, 0), long_osc_hook(long_id, 480)
    tok = ctx.tok(6)
    name = "long_osc_%s.sh" % tok
    ctx.rm("long_osc.txt")
    with open(os.path.join(ctx.W, name), "w", newline="\n") as f:
        f.write(long_osc_script(short, long_, "long_osc.txt", tok))
    off = ctx.log_size()
    ctx.be.type("bash ./%s" % name, 12, enter=True)
    ran = ctx.until(lambda: tok in ctx.lines("long_osc.txt"), 15)
    r.check(bool(ran), "the script that prints the two hooks did not run in the pane within 15 s (long_osc.txt has no %s)" % tok)
    if ran:
        ctx.until(lambda: ("unrecognized session_id %d" % long_id) in ctx.log_since(off), 3, step=0.2)
        ok, text = long_osc_verdict(ctx.log_since(off), short_id, long_id)
        r.metric("long_osc_bytes", dict(short=len(short), long=len(long_)))
        r.check(ok, text)
        if ok:
            r.note(text)
    ctx.rm(name, "long_osc.txt")
    ctx.reset()


@case("ssh_gitbash_managed", order=62)
def c_ssh_gitbash_managed(ctx, r):
    """#1933 (owner 2026-11-09), #2051 (owner 2026-11-10: off unless the shell has PHOSPHOR_MSYS_MANAGED_SSH=1, which this case types into the pane's
    shell first): `ssh fake` from the Git Bash pane is then a MANAGED session on one connection (msys ssh.exe cannot multiplex). Nothing is typed
    while it connects. Evidence, all from the app log and the shells' own rec.log: the wrapper hops show the stages auth,
    auth_done, managed in that order, none ignored, and `verdict managed=true` (round 8 showed `managed=false` on every Git Bash ssh); the pane is
    provably remote; a line typed at the remote prompt runs once there and never locally. The in-band Tab itself is `remote_tab_no_helper`."""
    _need_windows_pane(ctx, r, "gitbash")
    R.need_ssh(ctx, r)
    R.in_msys_managed_ssh(ctx, lambda: _ssh_gitbash_managed(ctx, r))


def _ssh_gitbash_managed(ctx, r):
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


BOOTSTRAP_WRITE_LIMIT = 4096      # bytes: the bash bootstrap is about 62 KB; a key, a sync token or a mode byte is a handful


def bootstrap_delivery(log):
    """pure: what the app log of one managed connect says about how the remote shell got its bootstrap (#2051): whether the app reported the
    in-command delivery (`hop=bootstrap_in_command ... wrote=0`), the largest single pty write it made (`hop=pty_write_bytes ... wrote=<n>`; the typed
    delivery of #1933 was one write of 62000), and whether it gave up on the remote bootstrap (`Bootstrapping failed for shell`)"""
    writes = [int(ln.split(" wrote=", 1)[1].split()[0]) for ln in log.splitlines()
              if "hop=pty_write_bytes" in ln and " wrote=" in ln and ln.split(" wrote=", 1)[1].split()[0].isdigit()]
    return dict(in_command=any("hop=bootstrap_in_command" in ln and "wrote=0" in ln for ln in log.splitlines()),
                max_write=max(writes) if writes else 0,
                failed="Bootstrapping failed for shell" in log)


@case("ssh_gitbash_bootstrap_in_command", order=62)
def c_ssh_gitbash_bootstrap_in_command(ctx, r):
    """#2051: with the one-connection managed ssh switched on, the remote bash gets its bootstrap INSIDE the remote command, not typed: round 12 showed
    62 KB typed through msys ssh.exe's console take 45 to 60 s (about 1.3 KB/s) while the app gives up after 6.5 s. Evidence for one `ssh fake`:
    the app log says `hop=bootstrap_in_command ... wrote=0`, no single pty write during the connect is bootstrap-sized (under 4096 bytes; the typed
    delivery was one write of 62000), the app never logs `Bootstrapping failed`, the stages are auth, auth_done, managed with `verdict
    managed=true`, the pane is provably remote and a line typed at the remote prompt runs once there. The seconds to the remote prompt and the bytes
    the test sshd says the client sent it (Windows fake sshd only) are metrics; the sshd's count must be under 8192 when it logged one (round 12:
    78814 and still coming)."""
    _need_windows_pane(ctx, r, "gitbash")
    R.need_ssh(ctx, r)
    R.in_msys_managed_ssh(ctx, lambda: _ssh_gitbash_bootstrap_in_command(ctx, r))


def _ssh_gitbash_bootstrap_in_command(ctx, r):
    off, sshd_off = ctx.log_size(), R.sshd_log_size(ctx)
    secs = R.connect(ctx, "fake", 60)
    log = ctx.log_since(off)
    how = bootstrap_delivery(log)
    r.metric("gitbash_in_command_delivery", how)
    r.check(secs is not None, "no remote shell prompt after `ssh fake`. Test host: %s" % (ctx.ssh_diag() if secs is None else ""))
    r.check(how["in_command"], "the app log has no `hop=bootstrap_in_command ... wrote=0`: the remote command did not carry the bootstrap "
            "(the wrapper fell back to plain ssh, or the app typed it)")
    r.check(how["max_write"] < BOOTSTRAP_WRITE_LIMIT,
            "the app wrote %d bytes to the pty in one write during the connect (limit %d): a bootstrap was typed" % (how["max_write"], BOOTSTRAP_WRITE_LIMIT))
    r.check(not how["failed"], "the app gave up on the remote bootstrap (`Bootstrapping failed for shell` in its log)")
    hops = _wrapper_hops(ctx, off)
    r.check(_sync_stages(hops) == ["auth", "auth_done", "managed"], "one-connection stages in the app log: %r, want auth, auth_done, managed" % _sync_stages(hops))
    r.check(any("verdict managed=true" in ln for ln in hops), "the wrapper's verdict was managed=true")
    if secs is None:
        ctx.reset()
        return
    r.metric("gitbash_in_command_connect_s", round(secs, 2))
    time.sleep(1.0)
    got = R.sshd_input_bytes(R.sshd_io_lines(ctx, sshd_off))
    if got is None:
        r.note("the test sshd logged no client-input count for this session (only the Windows fake sshd does)")
    else:
        r.metric("gitbash_in_command_client_bytes", got)
        r.check(got < 8192, "the test sshd received %d bytes from the client by the time the remote prompt was up (limit 8192; round 12: 78814): "
                "something bootstrap-sized went through ssh's console" % got)
    if ctx.where_now() == "remote":
        _remote_line_once(ctx, r, "managed session, bootstrap in the command")
        if not R.leave(ctx):
            r.check(False, "not provably remote (probe ran in: %s), `exit` NOT typed" % ctx.left_unproven)
    else:
        r.check(False, "the pane is not provably in the REMOTE shell after `ssh fake` (probe ran in: %s)" % ctx.where_now())
    ctx.reset()


@case("ssh_gitbash_typeahead_managed", order=62)
def c_ssh_gitbash_typeahead_managed(ctx, r):
    """#1959 (owner 2026-11-09; was ssh_gitbash_typeahead_plain, where either outcome passed): a line typed blind right after `ssh fake` + Enter (no
    pause, 10-25 ms keys) must NOT make the session plain. With one connection those keys can be in the console before the session exists; ssh's
    LocalCommand drains them, the app puts them at the FRONT of the keys it holds and answers managed, and the first remote prompt gets the line.
    Required in EVERY trial: the typed line ran exactly once REMOTELY and never locally; the stages are auth, auth_done, managed; the verdict is
    managed=true; no stage or verdict was ignored; the app never reports `typeahead_front adopted=false` (that is the plain fallback for keys with no
    provable place in the hold: a defect here, where one pane types one line). How many trials had keys in the console (`typeahead_front
    adopted=true`) is a note: it depends on how fast the login was, and a trial where every key was typed after auth_done is held the ordinary way.
    #2051: the managed path is off by default, so the case types PHOSPHOR_MSYS_MANAGED_SSH=1 into the pane's shell first and unsets it at the end."""
    _need_windows_pane(ctx, r, "gitbash")
    R.need_ssh(ctx, r)
    R.in_msys_managed_ssh(ctx, lambda: _ssh_gitbash_typeahead_managed(ctx, r))


def _ssh_gitbash_typeahead_managed(ctx, r):
    trials, adopted = 3, 0
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
        hops = _wrapper_hops(ctx, off)
        stages = _sync_stages(hops)
        r.check(stages == ["auth", "auth_done", "managed"],
                "trial %d: one-connection stages in the app log: %r, want auth, auth_done, managed (a final `plain` is the old #1959 behaviour)" % (i + 1, stages))
        r.check(any("verdict managed=true" in ln for ln in hops), "trial %d: the wrapper's verdict was managed=true" % (i + 1))
        r.check(not any("sync_ignored" in ln or "verdict_ignored" in ln for ln in hops),
                "trial %d: no stage or verdict was ignored: %r" % (i + 1, [ln[-140:] for ln in hops if "_ignored" in ln][:3]))
        refused = [ln[-120:] for ln in hops if "typeahead_front adopted=false" in ln]
        r.check(refused == [], "trial %d: the keys drained from the console were not put at the front of the hold (plain fallback): %r" % (i + 1, refused[:2]))
        adopted += any("typeahead_front adopted=true" in ln for ln in hops)
        if i == 0:
            ctx.shot("gitbash_typeahead_managed_remote_prompt")
        if ctx.where_now() == "remote":
            if not R.leave(ctx):
                r.check(False, "trial %d: `exit` did not bring the local prompt back" % (i + 1))
        else:
            r.check(False, "trial %d: the pane is not provably remote after the connect (probe ran in: %s)" % (i + 1, ctx.where_now()))
            ctx.reset()
    r.note("typed-during-connect: in %d of %d trials keys were in the console before the session and were held at the front (typeahead_front "
           "adopted=true); in the others every key was typed after auth_done or consumed by the login" % (adopted, trials))
    ctx.reset()


@case("ssh_gitbash_kill_switch", order=62)
def c_ssh_gitbash_kill_switch(ctx, r):
    """#1933, #2051 (owner 2026-11-10: the one-connection managed ssh of the Git Bash pane is OFF unless the shell has PHOSPHOR_MSYS_MANAGED_SSH=1).
    Two connects, each must be the user's plain ssh (no stage hop, verdict managed=false) with a line typed at the remote prompt running once there:
    first with the variable NOT set (the default every user gets; before 2026-11-10 this connect was managed and only `=0` was plain), then with
    PHOSPHOR_MSYS_MANAGED_SSH=0 typed into the pane's shell (the explicit off still means off). `=1` being managed is ssh_gitbash_managed. The
    variable is unset before the first connect and again at the end."""
    _need_windows_pane(ctx, r, "gitbash")
    R.need_ssh(ctx, r)
    try:
        for what, line in (("the default (variable not set)", R.msys_managed_ssh_lines(False)), ("PHOSPHOR_MSYS_MANAGED_SSH=0", "export %s=0" % R.MSYS_MANAGED_SSH)):
            ctx.be.type(line, 12, enter=True)
            time.sleep(0.5)
            off = ctx.log_size()
            secs = R.connect(ctx, "fake", 60)
            r.check(secs is not None, "%s: no remote shell prompt after `ssh fake`. Test host: %s" % (what, ctx.ssh_diag() if secs is None else ""))
            if secs is not None:
                time.sleep(1.0)
                hops = _wrapper_hops(ctx, off)
                r.check(_sync_stages(hops) == [], "%s: no one-connection stage: %r" % (what, _sync_stages(hops)))
                r.check(any("verdict managed=false" in ln for ln in hops), "%s: the wrapper's verdict was the plain fallback (managed=false)" % what)
                r.check(not any("verdict managed=true" in ln for ln in hops), "%s: the session must not be managed" % what)
                if ctx.where_now() == "remote":
                    _remote_line_once(ctx, r, "plain session, %s" % what)
                    if not R.leave(ctx):
                        r.check(False, "%s: not provably remote (probe ran in: %s), `exit` NOT typed" % (what, ctx.left_unproven))
                else:
                    r.check(False, "%s: the pane is not provably remote after `ssh fake` (probe ran in: %s)" % (what, ctx.where_now()))
            ctx.reset()
    finally:
        ctx.be.type(R.msys_managed_ssh_lines(False), 12, enter=True)
        time.sleep(0.5)
