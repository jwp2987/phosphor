# appci/cases_win_input.py: the Windows input-parity cases (docs/design/windows-input-parity-audit.md, findings 3, 7, 8; issues #1884, #1888, #1889).
# Registered with the same @case decorator as cases.py (one import line at its end). Windows panes only (the two *_wrapper cases also run on Linux); each SKIPs elsewhere with its reason.
#   ssh_connect_typing_wrapper / ssh_nested_typeahead_wrapper   (also the Linux pane, #2066) Git Bash pane, the managed-ssh wrapper pinned on (APPCI_SSH_WRAPPER=1, a job variable) and the
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
#   ctrl_c_behind_big_paste                                     both Windows panes (+ pwsh7): a 64 KiB paste to a stdin reader, then ^C at once; the metric is ms from hop=ctrl_c_keydown to the pty
#                                                               write carrying the ETX, judged against a band (#1887; docs/design/1887-1890-win-ctrlc.md)
#   powershell_setup_ctrl_c_keys                                PowerShell pane (+ pwsh7): a PowerShell-in-PowerShell Phosphorize setup, ^C, a line typed at once: it runs once, released by the
#                                                               post-abort prompt, not by a give-up net (#1890)
import os, re, sys, time

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


WRAPPER_PIN_NEEDS = ("this job does not pin the ssh wrapper. NEEDS: APPCI_SSH_WRAPPER=1 in the job's variables (.gitlab-ci.yml sets it for app-test-linux-wayland, "
                     "app-test-macos and app-test-windows; a hand run exports it), so that profile.py writes warpify.ssh.enable_ssh_warpification and enable_legacy_ssh_wrapper "
                     "explicitly (#1884)")


def wrapper_hops(log):
    """pure: (lines, verdict texts) of the app's `hop=ssh_wrapper` lines in `log`"""
    lines = [ln for ln in (log or "").splitlines() if "hop=ssh_wrapper" in ln]
    return lines, [ln.split("hop=ssh_wrapper", 1)[-1].strip()[:60] for ln in lines if "verdict" in ln]


def _wrapper_variant(ctx, r, base):
    """#2066: these two cases were SKIP in every pane of every run, because nothing set APPCI_SSH_WRAPPER=1 and they refused every pane but Windows Git
    Bash. The wrapper's announcement and verdict are handled by the same code on every platform (terminal/view.rs ssh_on_wrapper_announced /
    ssh_on_wrapper_verdict), so they run in any pane whose shell has the wrapper once the job pins it: the Windows Git Bash pane (the born-owned ConPTY
    window, #1884; the one-connection managed ssh is switched on by in_msys_managed_ssh, #2051, since it is off by default), the Linux pane and, since
    round 16 (owner 2026-11-11: the wrapper is pinned on app-test-macos too), the macOS pane (the ControlMaster wrapper on both). The PowerShell pane
    has no wrapper (winmap.py)."""
    if ctx.win and ctx.pane != "gitbash":
        r.skip("runs in the gitbash pane (the PowerShell pane's ssh is not the bash wrapper)")
    if os.environ.get("APPCI_SSH_WRAPPER", "") != "1":
        r.skip(WRAPPER_PIN_NEEDS)
    off = ctx.log_size()
    # #2051: on Windows the managed path these two cases are about is off by default; they switch it on for the pane's shell (a no-op elsewhere)
    R.in_msys_managed_ssh(ctx, lambda: base(ctx, r))
    wrapper, verdicts = wrapper_hops(ctx.log_since(off))
    where = "with the one-connection managed ssh switched on" if ctx.win else "with the ssh wrapper pinned on"
    r.check(any("verdict managed=true" in ln for ln in wrapper), "%s, the wrapper's verdict was managed=true; verdicts seen: %r" % (where, verdicts[:4]))
    r.check(any("announced" in ln for ln in wrapper),
            "the wrapper announcement was handled on the connect window (hop ssh_wrapper announced); hops seen: %d" % len(wrapper))
    r.check(not any("verdict_ignored" in ln for ln in wrapper),
            "no wrapper verdict was ignored (#1884: the born-owned window on ConPTY): %r" % [ln[-120:] for ln in wrapper if "verdict_ignored" in ln][:2])


@case("ssh_connect_typing_wrapper", order=30)
def c_ssh_connect_typing_wrapper(ctx, r):
    """#1884: ssh_connect_typing with the managed-ssh wrapper pinned on (Windows Git Bash pane, Linux pane, macOS pane)"""
    _wrapper_variant(ctx, r, cases.c_ssh)


@case("ssh_nested_typeahead_wrapper", order=30)
def c_ssh_nested_typeahead_wrapper(ctx, r):
    """#1884: ssh_nested_typeahead with the managed-ssh wrapper pinned on (Windows Git Bash pane, Linux pane, macOS pane)"""
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


# ------------------------------------------------------------------ #1887, #1890: Ctrl-C behind a big paste, and Ctrl-C during a PowerShell setup
_HOP_T_RE = re.compile(r" t=(\d+(?:\.\d+)?)")


def _hop_t(line):
    m = _HOP_T_RE.search(line)
    return float(m.group(1)) if m else None


def ctrl_c_wait_ms(log):
    """pure (#1887): -> (ms, which, n_keydowns) for the log text of one `paste, then ^C at once` trial. ms = from the first `hop=ctrl_c_keydown` to the first
    pty write after it that carries the ETX: a `hop=pty_write_bytes` of class UserKey with wrote=1 of_job=1 (a one-byte typed job; the ^C is the only key the
    case sends after the paste, and a text-logging run shows `\\x03`), else the `hop=ctrl_c_pty_write` line the loop logs for a finished write whose job holds
    a 0x03. `which` names the hop used; (None, None, n) when the keydown or the write is missing."""
    lines = log.splitlines()
    down = [i for i, ln in enumerate(lines) if "hop=ctrl_c_keydown" in ln]
    if not down:
        return None, None, 0
    t0 = _hop_t(lines[down[0]])
    for ln in lines[down[0]:]:
        t = _hop_t(ln)
        if t0 is None or t is None:
            continue
        if "hop=pty_write_bytes" in ln and "class=UserKey" in ln and "wrote=1 of_job=1" in ln:
            return round(t - t0, 2), "pty_write_bytes", len(down)
        if "hop=ctrl_c_pty_write" in ln:
            return round(t - t0, 2), "ctrl_c_pty_write", len(down)
    return None, None, len(down)


def ctrl_c_band_key(ctx):
    """band key of the ^C wait: <platform>/<mode incl. pane>/ctrl_c_behind_big_paste/ctrl_c_to_pty_write_ms (owner 2026-11-12: every perf figure is a band)"""
    import bands
    return bands.key(os.environ.get("APPCI_PLATFORM", "windows"), "%s/ctrl_c_behind_big_paste/ctrl_c_to_pty_write_ms" % ctx.label)


def big_paste_payload(n_bytes=64 * 1024):
    """pure: ~n_bytes of short plain lines (under the 4095-byte canonical line limit, nothing a shell would run or complete)"""
    out, tot, i = [], 0, 0
    while tot < n_bytes:
        ln = "pad %05d %s" % (i, "x" * 60)
        out.append(ln)
        tot += len(ln) + 1
        i += 1
    return "\n".join(out)


@case("ctrl_c_behind_big_paste", order=19)
def c_ctrl_c_behind_big_paste(ctx, r):
    """#1887 (owner answer Q-A1, 2026-11-14: measure first): a program is reading stdin (PowerShell `[Console]::In.ReadToEnd()`, Git Bash `cat`), a 64 KiB
    paste goes to the pane, and a Ctrl-C follows at once. The metric is the milliseconds from the app's `hop=ctrl_c_keydown` to the pty write that carries
    the ETX, judged against the case's band (the first runs have no band: NOREF, the value is recorded); the program must also stop (a line typed after it
    runs). The event loop offers a ConPTY write in 4096-byte pieces (local_tty/windows `CONPTY_MAX_WRITE_CHUNK`) and a ^C goes out between two of them; this
    measurement decides whether that bound stays, shrinks, or #1887 closes."""
    if not ctx.win:
        r.skip("Windows only (ConPTY)")
    import bands
    f, rdy = "ccbp.txt", "ccbp_ready.txt"
    ctx.rm(f, rdy)
    tok = ctx.tok(6)
    if ctx.pane == "powershell":
        reader = "%s; $null = [Console]::In.ReadToEnd()" % ctx.append_cmd("pwsh", "READY", rdy)
    else:
        reader = "%s; cat > /dev/null" % ctx.append_cmd("bash", "READY", rdy)
    ctx.be.type(reader, 10, enter=True)
    if not ctx.until(lambda: "READY" in ctx.lines(rdy), 15):
        r.check(False, "the stdin reader never started (file %r)" % ctx.lines(rdy))
        ctx.reset()
        return
    time.sleep(0.5)
    ctx.be.clip_set(big_paste_payload())
    off = ctx.log_size()
    ctx.be.paste()
    ctx.be.key("ctrl+c")                     # at once: while the app is still writing the paste
    ctx.until(lambda: ctrl_c_wait_ms(ctx.log_since(off))[0] is not None, 40)
    ms, which, downs = ctrl_c_wait_ms(ctx.log_since(off))
    r.metric("ctrl_c_to_pty_write_ms", ms)
    r.metric("ctrl_c_hop_used", which)
    r.check(ms is not None, "the ^C reached the pty (hop ctrl_c_keydown, then the write carrying the ETX); keydown hops seen: %d" % downs)
    if ms is not None:
        bands.check(r, ctrl_c_band_key(ctx), ms, "^C behind a 64 KiB paste: %.1f ms from ctrl_c_keydown to its pty write (%s)" % (ms, which))
    # the program stopped: a line typed after the ^C runs
    ctx.be.type(ctx.append_cmd(ctx.pane_shell(), tok, f), 10, enter=True)
    r.check(ctx.until(lambda: tok in ctx.lines(f), 15), "the reader was stopped by the ^C: the next line ran (file %r)" % ctx.lines(f))
    ctx.reset()


def handed_back_by_net(log):
    """pure (#1890): the `hop=prompt_hold_handed_back` lines of a PowerShell setup gate (kind=PowershellSetup) in `log`: the give-up nets (the 5 s marker net,
    the 0.5 s settle, the stall nets) hand keys back as text and log this hop; the post-abort prompt releases them without it."""
    return [ln.split("hop=prompt_hold_handed_back", 1)[-1].strip()[:140] for ln in log.splitlines()
            if "hop=prompt_hold_handed_back" in ln and "kind=PowershellSetup" in ln]


@case("powershell_setup_ctrl_c_keys", order=22)
def c_powershell_setup_ctrl_c_keys(ctx, r):
    """#1890 (owner answers Q-B1, Q-B2, 2026-11-14): a PowerShell-in-PowerShell Phosphorize setup, a Ctrl-C and, at once, a line typed with its Enter. The line
    must run exactly once in the inner shell, and it must reach the shell by the prompt after the abort (the app's `Precmd` handling releases the held keys in
    typed order), never by a give-up net: no `hop=prompt_hold_handed_back kind=PowershellSetup` (the 5 s marker give-up, the 0.5 s settle, the 30 s quiet net)
    after the ^C. When the ^C did not skip the setup (it was not stalled, so it only cancelled the held line; `hop=subshell_setup_aborted_by_user` is absent)
    the run is noted, and only the exactly-once and no-give-up checks apply."""
    _need_windows_pane(ctx, r, "powershell")
    import cases_subshell_ready as SR
    exe = "pwsh" if getattr(ctx, "variant", None) == "pwsh7" else "powershell"
    f = "pscck.txt"
    ctx.rm(f)

    @SR.guarded
    def body(ctx, r):
        off = ctx.log_size()
        ctx.be.type("%s -NoProfile -NoLogo" % exe, 10, enter=True)
        time.sleep(SR.BANNER_S)
        ctx.shot("psck_banner")
        ctx.be.key("ctrl+i")                 # Phosphorize the inner PowerShell: the setup starts
        time.sleep(0.4)
        tok = ctx.tok(6)
        t_cc = time.time()
        ctx.be.key("ctrl+c")
        ctx.be.type(ctx.append_cmd("pwsh", tok, f), 8, enter=True)
        ran = ctx.until(lambda: tok in ctx.lines(f), 45)
        took = time.time() - t_cc
        time.sleep(1.0)
        log = ctx.log_since(off)
        aborted = "hop=subshell_setup_aborted_by_user" in log
        nets = handed_back_by_net(log)
        r.metric("setup_ctrl_c_line_after_ctrl_c_s", round(took, 2) if ran else None)
        r.metric("setup_ctrl_c_aborted_by_user", aborted)
        r.check(ran, "the line typed right after the ^C ran in the inner shell (file %r)" % ctx.lines(f))
        r.check(ctx.lines(f).count(tok) == 1, "it ran exactly once: %r" % ctx.lines(f))
        r.check(not nets, "the keys were not handed back by a give-up net (hop prompt_hold_handed_back kind=PowershellSetup): %r" % nets[:2])
        if aborted and ran:
            r.check(took < 4.5, "the ^C skipped the setup and the line reached the shell after %.1f s, by the post-abort prompt, not at the 5 s net" % took)
        elif not aborted:
            r.note("the ^C did not skip the setup (hop subshell_setup_aborted_by_user absent: not stalled, so it cancelled the held line); the 4.5 s bound was not judged")
        SR.leave_inner(ctx, r, "powershell_setup_ctrl_c_keys")

    body(ctx, r)
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
    R.settle_remote_prompt(ctx)       # #2182: the remote precmd before the line is typed, not a race with the app's generator write
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
    where = ctx.where_now()      # ONE probe per verdict (round 14: the text came from a second probe and contradicted the first)
    if where != "remote":
        r.check(False, "precondition: the pane is not provably in the REMOTE shell after `ssh fake` (probe ran in: %s)" % where)
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


@case("ssh_gitbash_failing_not_local_managed", order=62, gui=True)
def c_ssh_gitbash_failing_not_local_managed(ctx, r):
    """ssh_failing_not_local with the one-connection managed ssh switched ON in the Git Bash pane's shell (PHOSPHOR_MSYS_MANAGED_SSH=1, typed and
    afterwards unset and proven unset by in_msys_managed_ssh). Round 14 ran this combination by accident (the switch was left on by the case
    before) and the line typed behind the failing ssh was not in the local file when the case looked; whether the app lost it or the case
    looked too early could not be told, the logs of that job were not uploaded. Here it is asked on purpose: the wrapper first asks the
    hanging server for its banner, gets none, and must run the user's plain ssh; the keys typed meanwhile (the app holds them: its
    `kh_hold_exit branch=held` is on the row) must reach ssh's terminal in typed order, and when ssh gives up the local shell runs the line
    exactly once, after the failure (owner decision 2026-11-04, the same oracle as ssh_failing_not_local, not relaxed). The wait for "ssh
    failed" covers both connections (cases.wait_ssh_gave_up)."""
    _need_windows_pane(ctx, r, "gitbash")
    R.in_msys_managed_ssh(ctx, lambda: cases.ssh_fail_body(ctx, r))


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
    where = ctx.where_now()
    if where == "remote":
        _remote_line_once(ctx, r, "managed session, bootstrap in the command")
        if not R.leave(ctx):
            r.check(False, "not provably remote (probe ran in: %s), `exit` NOT typed" % ctx.left_unproven)
    else:
        r.check(False, "the pane is not provably in the REMOTE shell after `ssh fake` (probe ran in: %s)" % where)
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
        where = ctx.where_now()
        if where == "remote":
            if not R.leave(ctx):
                r.check(False, "trial %d: `exit` did not bring the local prompt back" % (i + 1))
        else:
            r.check(False, "trial %d: the pane is not provably remote after the connect (probe ran in: %s)" % (i + 1, where))
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
                where = ctx.where_now()
                if where == "remote":
                    _remote_line_once(ctx, r, "plain session, %s" % what)
                    if not R.leave(ctx):
                        r.check(False, "%s: not provably remote (probe ran in: %s), `exit` NOT typed" % (what, ctx.left_unproven))
                else:
                    r.check(False, "%s: the pane is not provably remote after `ssh fake` (probe ran in: %s)" % (what, where))
            ctx.reset()
    finally:
        R.msys_managed_ssh_restore(ctx)      # msys_managed_ssh_lines(False) typed AND proven in the local shell (round 14)
