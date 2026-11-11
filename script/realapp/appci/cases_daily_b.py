# appci/cases_daily_b.py (#2072; docs/design/r14-test-gaps.md 7.1 wave 1, T6-T10; design note docs/design/r10-r14-w1b.md).
# Five daily-use cases, all in cases.PROVING:
#   agent_basics_powershell   T6   Windows PowerShell pane: agent prose byte-exact, a shell line exact in PSReadLine history, nothing of the prose in the shell
#   job_control               T7   Linux, macOS: Ctrl-Z / bg / jobs / fg / Ctrl-C judged from the OS process state, `cat` across a stop
#   hostile_output_recovers   T8   all but the PowerShell pane: 2 MB of binary output and a script that leaves the terminal in a bad state
#   meta_and_word_keys        T9   all five: the readline keys at the prompt (the line the shell received) and into a raw reader (the bytes)
#   first_run_defaults        T10  all five (the Git Bash pane SKIPs): a first run with no config and no state
# Same rules as cases.py: typing at 10-25 ms per key with an instant Enter, the EXACT received line (rec.log; PSReadLine history in the PowerShell
# pane), a SKIP names the missing capability. Every verdict here has a control or a check a wrong result trips; the pure parts are in the self-test
# `r14_w1b_pure_parts`. Registered by the import line at the end of cases.py.
import os, re, shutil, signal, subprocess, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import cases as C  # noqa: E402
import cases_caseagent as CAG  # noqa: E402
import cases_ui as U  # noqa: E402
import hostile_out  # noqa: E402
import injacct  # noqa: E402
from cases import case  # noqa: E402

ISSUE_ALT_WORD = 2079     # Alt-B / Alt-F / Alt-Backspace were bound to Meta only off macOS; since round 15 they are the Alt key's (kill switch PHOSPHOR_ALT_WORD_KEYS=0)


# ---------------------------------------------------------------- shared
def recv_mark(ctx):
    """a mark in the pane's record of received command lines: PSReadLine history in the PowerShell pane (it has no rc hook), else rec.log"""
    if ctx.pane == "powershell":
        return ("ps", len(ctx.ps_history_lines()))
    return ("rec", ctx.cmd_mark())


def recv_since(ctx, mark):
    return ctx.ps_history_lines()[mark[1]:] if mark[0] == "ps" else ctx.cmds_since(mark[1])


def recv_wait(ctx, mark, n=1, timeout=6.0):
    ctx.until(lambda: len(recv_since(ctx, mark)) >= n, timeout)
    return recv_since(ctx, mark)


def program_cmd(ctx, script, *args):
    """the typed line that runs an appci Python program inside the pane (the Windows VM has no python3 on PATH: the private interpreter by full path)"""
    path = os.path.join(HERE, script)
    if ctx.win:
        return '"%s" "%s" %s' % (sys.executable.replace("\\", "/"), path.replace("\\", "/"), " ".join(args))
    return "python3 %s %s" % (path, " ".join(args))


def whole_log(ctx):
    """the app log of the current launch as one text (rotated chunks first); "" without one"""
    return "\n".join(ctx.app_log_all_lines())


# ---------------------------------------------------------------- T6 agent_basics_powershell
def judge_ps_shell_line(new_lines, want, prose_nonces):
    """pure: ('ok' | 'fail' | 'noref', text) for the PSReadLine history lines added during the case. Any line holding a nonce of the agent prose is
    prose that reached the shell; no new line at all is no evidence (history file missing or not written), never a pass; otherwise the new lines
    must be exactly the one shell line."""
    leaked = [l for l in new_lines if any(n in l for n in prose_nonces)]
    if leaked:
        return "fail", "agent prose reached PowerShell (PSReadLine history): %r" % leaked[:2]
    if not new_lines:
        return "noref", "PSReadLine history has no new line (file missing, filtered, or not written): the exact shell line was not verified"
    if new_lines != [want]:
        return "fail", "PowerShell received %r, expected exactly [%r] (PSReadLine history)" % (new_lines[:3], want)
    return "ok", ""


@case("agent_basics_powershell", order=49, gui=True)
def c_agent_basics_powershell(ctx, r):
    """T6 (r14-test-gaps G3): the Windows PowerShell pane, where agent_input_exact and agent_mode_switch cannot run (they read rec.log). Prose into
    the agent input, Escape to the terminal view, one shell line, back to the agent view, prose again. The agent text is byte-exact in the app log
    (`hop=enter_begin buffer=.. input_type=AI`); the shell line is exact in the app log (input_type=Shell), in PSReadLine history (exactly one new
    line in the whole case) and in its effect file; no nonce of the prose is in the history or in the bytes the app wrote to the pty. Ends in the
    view it started in. Needs no model."""
    if not (ctx.win and ctx.pane == "powershell") or ctx.variant:
        r.skip("runs only in the Windows PowerShell pane: it is that pane's twin of agent_input_exact and agent_mode_switch, which the other panes run")
    if not ctx.log_path or not os.path.exists(ctx.log_path):
        r.skip("no app log at %s: the agent input at Enter and the view cannot be read" % ctx.log_path)
    start = ctx.log_view_mode()
    if start is None:
        r.skip("the app log has no hop=view_mode line (a build older than #1568): the view cannot be proven")
    chord = "ctrl+shift+Return"
    t_case = time.time()
    hmark = len(ctx.ps_history_lines())
    tok = ctx.tok(6).lower()
    eff = "abp_%s.txt" % tok
    line = ctx.append_cmd("pwsh", tok, eff)
    nonces = []

    def go(want):
        ctx.be.key(chord if want == "agent" else "Escape")
        got = CAG.wait_view_mode(ctx, want)
        return r.check(got == want, "expected hop=view_mode mode=%s after the key, the app log says %s" % (want, got))

    def prose(tag):
        nonce = "zq" + ctx.tok(6).lower().replace("_", "")
        nonces.append(nonce)
        ctx.reset()
        CAG.submit_prose(ctx, r, "abp_" + tag, "please explain why the %s build isn't passing, it's odd (really)?" % nonce, tag)

    try:
        if start != "agent" and not go("agent"):
            return
        prose("prose1")
        if not go("terminal"):
            return
        ctx.reset()
        off = ctx.log_size()
        CAG.type_fast(ctx, line)
        ctx.until(lambda: tok in ctx.local_lines(eff), 10)
        ctx.until(lambda: len(ctx.ps_history_lines()) > hmark, 5)
        r.check(ctx.local_lines(eff) == [tok], "the shell line's effect file holds %r, expected exactly [%r]" % (ctx.local_lines(eff), tok))
        rows = CAG.enter_buffers(ctx.log_since(off))
        r.check(bool(rows) and rows[-1] == (line, "Shell"), "the shell line's Enter: the app held %r, expected (%r, 'Shell')" % (rows[-1] if rows else None, line))
        ctx.shot("abp_shell_line")
        if go("agent"):
            prose("prose2")
        time.sleep(0.8)
        verdict, text = judge_ps_shell_line(ctx.ps_history_lines()[hmark:], line, nonces)
        if verdict == "noref":
            r.unverifiable(text)
        else:
            r.check(verdict == "ok", text)
        wrote = ctx.app_pty_text(t_case)
        if wrote is None:
            r.note("no pty_write_bytes trace: 'nothing of the prose was written to the pty' rests on PSReadLine history alone")
        else:
            r.check(not [n for n in nonces if n in wrote], "the app wrote agent prose to the pty (hop=pty_write_bytes holds a prose nonce)")
    finally:
        for _ in range(2):
            if ctx.log_view_mode() == start:
                break
            ctx.be.key(chord if start == "agent" else "Escape")
            CAG.wait_view_mode(ctx, start)
        ctx.reset()
        ctx.rm(eff)
    r.check(ctx.log_view_mode() == start, "could not return to the %s view the case started in (log says %s)" % (start, ctx.log_view_mode()))


# ---------------------------------------------------------------- T7 job_control
def parse_ps_rows(text):
    """pure: [(pid, ppid, stat, args)] from `ps -A -o pid= -o ppid= -o stat= -o args=` (Linux and macOS)"""
    rows = []
    for l in text.splitlines():
        f = l.split(None, 3)
        if len(f) == 4 and f[0].isdigit() and f[1].isdigit():
            rows.append((int(f[0]), int(f[1]), f[2], f[3].strip()))
    return rows


def proc_state(rows, args):
    """pure: None (no process with exactly these args), 'stopped' (job-control stop: stat starts with T) or 'running'"""
    mine = [s for _p, _pp, s, a in rows if a == args]
    if not mine:
        return None
    return "stopped" if any(s.startswith("T") for s in mine) else "running"


def jobs_state(lines, needle):
    """pure: what a `jobs` listing says about the job whose line holds `needle`: 'stopped' (bash "Stopped", zsh "suspended"), 'running',
    'absent' (no such line) or 'other'"""
    mine = [l for l in lines if needle in l]
    if not mine:
        return "absent"
    low = " ".join(mine).lower()
    if "stopped" in low or "suspended" in low:
        return "stopped"
    return "running" if "running" in low else "other"


def ps_rows():
    try:
        return parse_ps_rows(subprocess.run(["ps", "-A", "-o", "pid=", "-o", "ppid=", "-o", "stat=", "-o", "args="], capture_output=True, text=True, timeout=10).stdout)
    except Exception:
        return []


def _kill_leftovers(ctx, sleep_cmd):
    """cleanup by exact pid: the case's own `sleep <unique n>`, and a `cat` whose parent is the pane's root shell"""
    root = getattr(ctx, "root_pid", None)
    for pid, ppid, _s, a in ps_rows():
        if a == sleep_cmd or (a == "cat" and root is not None and ppid == root):
            try:
                os.kill(pid, signal.SIGKILL)
            except OSError:
                pass


@case("job_control", order=24, gui=True)
def c_job_control(ctx, r):
    """T7 (r14-test-gaps G15). (1) `sleep <n>` (n unique to this run), Ctrl-Z, `jobs > a`, `bg`, `jobs > b`, `fg`, Ctrl-C, `jobs > c`: the OS says
    the process is stopped after Ctrl-Z, running after `bg`, still there in front after `fg`, gone after Ctrl-C; the three `jobs` files say stopped,
    running, absent; a prompt comes back after each step; the shell received exactly the six lines. (2) `cat > f`, a line, Ctrl-Z, a shell command,
    `fg`, a second line, Ctrl-D: the file holds exactly the two lines, the shell command ran in the shell, and no text line reached the shell as a
    command. Linux and macOS (POSIX job control; winmap gives the Windows reason)."""
    if ctx.win or C.plat().startswith("windows"):
        r.skip("job control (Ctrl-Z, bg, fg) is POSIX tty signalling: not in a Windows pane")
    t = ctx.tok(5).lower()
    n = ctx.rng.randint(300, 899)
    while proc_state(ps_rows(), "sleep %d" % n) is not None:      # never a number some other process already sleeps on
        n += 1
    sleep_cmd = "sleep %d" % n
    fa, fb, fc = "jc_a_%s.txt" % t, "jc_b_%s.txt" % t, "jc_c_%s.txt" % t
    fcat, fmid = "jc_cat_%s.txt" % t, "jc_mid_%s.txt" % t
    ctx.rm(fa, fb, fc, fcat, fmid)

    def typed(text):
        ctx.be.type(text, ctx.rng.randint(10, 25), enter=True)

    def line_then_prompt(text, what):
        p = ctx.prompts()
        typed(text)
        return r.check(bool(ctx.until(lambda: ctx.prompts() > p, 8)), "%s: no prompt came back within 8 s after `%s`" % (what, text))

    def state():
        return proc_state(ps_rows(), sleep_cmd)

    try:
        mark = ctx.cmd_mark()
        off = ctx.log_size()
        p0 = ctx.prompts()
        typed(sleep_cmd)
        if not ctx.until(lambda: state() == "running", 10, step=0.2):
            rows = CAG.enter_buffers(ctx.log_since(off))
            r.check(False, "`%s` never started: the shell did not run the line (shell received %r; the app's Enter: %r)" % (sleep_cmd, ctx.cmds_since(mark)[:2], rows[-1] if rows else None))
            return
        ctx.be.key("ctrl+z")
        stopped = ctx.until(lambda: state() == "stopped", 5, step=0.2)
        if not r.check(bool(stopped), "Ctrl-Z did not stop `%s`: the OS says it is %s (the key did not reach the program as SIGTSTP)" % (sleep_cmd, state())):
            ctx.be.key("ctrl+c")
            return
        r.check(bool(ctx.until(lambda: ctx.prompts() > p0, 8)), "no prompt came back within 8 s after Ctrl-Z")
        line_then_prompt("jobs > %s" % fa, "after Ctrl-Z")
        r.check(jobs_state(ctx.lines(fa), sleep_cmd) == "stopped", "`jobs` after Ctrl-Z: %r, expected the job stopped" % ctx.lines(fa))
        line_then_prompt("bg", "bg")
        r.check(bool(ctx.until(lambda: state() == "running", 5, step=0.2)), "`bg` did not resume `%s`: the OS says it is %s" % (sleep_cmd, state()))
        line_then_prompt("jobs > %s" % fb, "after bg")
        r.check(jobs_state(ctx.lines(fb), sleep_cmd) == "running", "`jobs` after bg: %r, expected the job running" % ctx.lines(fb))
        p = ctx.prompts()
        typed("fg")
        time.sleep(1.2)
        r.check(state() == "running" and ctx.prompts() == p, "`fg`: expected `%s` running in front and no new prompt; the OS says %s, prompts %d -> %d" % (sleep_cmd, state(), p, ctx.prompts()))
        ctx.be.key("ctrl+c")
        r.check(bool(ctx.until(lambda: state() is None, 5, step=0.2)), "Ctrl-C after `fg` did not end `%s`: the OS says it is %s (the key went elsewhere)" % (sleep_cmd, state()))
        r.check(bool(ctx.until(lambda: ctx.prompts() > p, 8)), "no prompt came back within 8 s after fg + Ctrl-C")
        line_then_prompt("jobs > %s" % fc, "after Ctrl-C")
        r.check(os.path.exists(os.path.join(ctx.W, fc)) and jobs_state(ctx.lines(fc), sleep_cmd) == "absent", "`jobs` after Ctrl-C: %r, expected a file with no such job" % ctx.lines(fc))
        want = [sleep_cmd, "jobs > %s" % fa, "bg", "jobs > %s" % fb, "fg", "jobs > %s" % fc]
        got = ctx.await_commands(mark, len(want), 5.0)
        r.check(got == want, "part 1: the shell received %r, expected exactly %r" % (got, want))
        ctx.shot("job_control_sleep")

        # (2) cat across a stop
        before, mid, after = "zqbefore%s" % t, "zqmid%s" % t, "zqafter%s" % t
        mark = ctx.cmd_mark()
        p0 = ctx.prompts()
        typed("cat > %s" % fcat)
        if not ctx.until(lambda: os.path.exists(os.path.join(ctx.W, fcat)), 8):
            r.check(False, "`cat > %s` never started (shell received %r)" % (fcat, ctx.cmds_since(mark)[:2]))
            return
        time.sleep(0.5)
        typed(before)
        r.check(bool(ctx.until(lambda: ctx.lines(fcat) == [before], 5)), "the line typed into cat before the stop: file holds %r, expected [%r]" % (ctx.lines(fcat), before))
        ctx.be.key("ctrl+z")
        if not r.check(bool(ctx.until(lambda: ctx.prompts() > p0, 8)), "no prompt came back within 8 s after Ctrl-Z with cat in front (cat was not stopped)"):
            ctx.be.key("ctrl+d")
            return
        line_then_prompt("echo %s >> %s" % (mid, fmid), "the command typed while cat is stopped")
        r.check(ctx.lines(fmid) == [mid], "the shell command typed while cat was stopped: %s holds %r, expected [%r]" % (fmid, ctx.lines(fmid), mid))
        p = ctx.prompts()
        typed("fg")
        time.sleep(1.2)
        typed(after)
        r.check(bool(ctx.until(lambda: ctx.lines(fcat) == [before, after], 5)), "after `fg` the second line: cat's file holds %r, expected %r" % (ctx.lines(fcat), [before, after]))
        ctx.be.key("ctrl+d")
        r.check(bool(ctx.until(lambda: ctx.prompts() > p, 8)), "no prompt came back within 8 s after Ctrl-D ended cat")
        want = ["cat > %s" % fcat, "echo %s >> %s" % (mid, fmid), "fg"]
        got = ctx.await_commands(mark, len(want), 5.0)
        r.check(got == want, "part 2: the shell received %r, expected exactly %r (a text line here was typed for cat and reached the shell)" % (got, want))
        r.check(ctx.lines(fmid) == [mid], "%s holds %r after the case, expected [%r]: a key went to the wrong program" % (fmid, ctx.lines(fmid), mid))
        ctx.shot("job_control_cat")
    finally:
        _kill_leftovers(ctx, sleep_cmd)
        time.sleep(0.3)
        ctx.reset()
        ctx.rm(fa, fb, fc, fcat, fmid)


# ---------------------------------------------------------------- T8 hostile_output_recovers
HOSTILE_SEED = 20261110
HOSTILE_BYTES = 2_000_000
UI_WARN_MS = 16.0          # the owner's UI budget is 4 ms, warn at 16
_ERR_RX = re.compile(r"\[ERROR\]\s*(.*)")
_PANIC_RX = re.compile(r"panicked at")


def error_signatures(text):
    """pure: {signature: first line} of the `[ERROR]` lines of an app log. The signature is the message with every digit run replaced by `#` and cut
    at 160 chars, so the same error with another id, size or time is the same signature."""
    out = {}
    for l in text.splitlines():
        m = _ERR_RX.search(l)
        if m:
            sig = re.sub(r"\d+", "#", re.sub(r"\st=\S+", "", m.group(1))).strip()[:160]
            out.setdefault(sig, l.strip()[:300])
    return out


def new_errors(control_text, judged_text):
    """pure: (panics, new, known) for a judged log excerpt against a control excerpt of the same build (the log before the case; the ordinary launch).
    panics: the lines holding `panicked at` (always a failure). new: ERROR lines whose signature the control does not have (a failure). known:
    signatures both have (reported, not judged: this case did not cause them)."""
    panics = [l.strip()[:300] for l in judged_text.splitlines() if _PANIC_RX.search(l)]
    ctl, got = error_signatures(control_text), error_signatures(judged_text)
    return panics, [got[s] for s in got if s not in ctl], sorted(s for s in got if s in ctl)


def judge_ui_after(base_max, after_max, warn=UI_WARN_MS):
    """pure: (ok, text). The largest main-thread item while one command is typed AFTER the hostile output must be under the warn bound; on a
    software renderer ordinary frames are over it by themselves, so an item over the bound fails only when it is also more than twice the largest
    item of the same typing BEFORE the output (the control)."""
    if after_max < warn:
        return True, "largest main-thread item after the output %.1f ms (< %.0f ms; before: %.1f ms)" % (after_max, warn, base_max)
    if after_max <= 2 * base_max:
        return True, "largest main-thread item after the output %.1f ms is over %.0f ms, but so was the same typing before it (%.1f ms): the machine's frame cost, not the output's" % (after_max, warn, base_max)
    return False, "largest main-thread item while typing after the hostile output: %.1f ms (warn bound %.0f ms; the same typing before it: %.1f ms)" % (after_max, warn, base_max)


def find_hops(text):
    return [l.split("hop=find_scroll", 1)[1].strip()[:300] for l in text.splitlines() if "hop=find_scroll" in l]


def find_result(hops):
    """pure: 'scroll' (the Find had a match to go to), 'no_match', or None (the app logged no find_scroll hop)"""
    if any(h.startswith("result=scroll") for h in hops):
        return "scroll"
    return "no_match" if any(h.startswith("result=no_match") for h in hops) else None


def judge_find(base, after, absent):
    """pure: ('ok' | 'fail' | 'noref', text) from three Finds: the token before the hostile output (control), the same token after it, and a token
    that was never printed (the control that the hop can say no)."""
    if base != "scroll":
        return "noref", "the Find BEFORE the hostile output gave %s, not a match: Find could not be judged in this pass" % (base or "no find_scroll hop")
    if absent == "scroll":
        return "fail", "TEST control: a Find for a token that was never printed reported a match, so the hop cannot tell"
    if after != "scroll":
        return "fail", "Find matched the token before the hostile output and not after it (%s)" % (after or "no find_scroll hop: the alternate screen may still be active")
    return "ok", ""


def do_find(ctx, token, wait=8.0):
    """open Find, type the token + Enter, read the app's find_scroll hops, Escape. -> find_result"""
    off = ctx.log_size()
    ctx.be.guard()
    ctx.be.key(os.environ.get("APPCI_KEY_FIND", "cmd+f" if ctx.be.name == "mac" else "ctrl+shift+f"))
    time.sleep(0.8)
    ctx.be.type(token, 20, enter=True)
    ctx.until(lambda: find_result(find_hops(ctx.log_since(off))) == "scroll", wait, step=0.25)
    res = find_result(find_hops(ctx.log_since(off)))
    ctx.be.key("Escape")
    time.sleep(0.5)
    return res


def typed_exact(ctx, r, label, fname, word):
    """one `echo <word> >> <fname>` at the owner rate with an instant Enter: the effect file gains exactly that word and the shell received exactly
    that line -> (ok, the largest main-thread item logged while it was typed)"""
    line = "echo %s >> %s" % (word, fname)
    mark, n0, off = ctx.cmd_mark(), len(ctx.lines(fname)), ctx.log_size()
    ctx.be.type(line, ctx.rng.randint(10, 25), enter=True)
    ctx.until(lambda: word in ctx.lines(fname), 10)
    got = ctx.await_commands(mark, 1, 5.0)
    ok = r.check(ctx.lines(fname)[n0:] == [word], "%s: the effect file gained %r, expected exactly [%r]" % (label, ctx.lines(fname)[n0:], word))
    if ctx.prof.commands():
        ok = r.check(got == [line], "%s: the shell received %r, expected exactly [%r]" % (label, got, line)) and ok
    else:
        r.note("%s: rec.log empty (prompt hook not run), effect file checked only" % label)
    return ok, C.ui_item_audit(ctx.log_since(off))["max_ms"]


@case("hostile_output_recovers", order=63, gui=True)
def c_hostile_output_recovers(ctx, r):
    """T8 (r14-test-gaps G16). `cat` of a 2 MB binary file (seeded: the same bytes every run), then hostile_out.py (2000 window titles, a 1 MB OSC 8
    link, mouse reporting left on, the alternate screen never left). After each, one typed command must be exact. Then: no panic, and no ERROR line
    whose kind the launch had not logged before the case; the largest main-thread item while typing afterwards under the warn bound (control: the
    same typing before); Find matches a token after as it did before (control: a token never printed does not match); a last typed command is exact
    (the focus came back from Find). The modes the script left on are switched off at the very end, as cleanup, after every verdict."""
    if ctx.pane == "powershell":
        r.skip("`cat` of a binary file, a POSIX command list and rec.log: runs in the Git Bash pane")
    have_log = bool(ctx.log_path and os.path.exists(ctx.log_path))
    t = ctx.tok(6).lower()
    eff, done, binf = "ho_%s.txt" % t, "ho_done_%s.txt" % t, "ho_%s.bin" % t
    present, absent = "zqfind%s" % t.replace("_", ""), "zqnone%s" % t.replace("_", "")
    ctx.rm(eff, done, binf)
    import hashlib
    data = hostile_out.seeded_bytes(HOSTILE_SEED, max(200_000, int(HOSTILE_BYTES * ctx.scale)))
    with open(os.path.join(ctx.W, binf), "wb") as f:
        f.write(data)
    r.metric("hostile_bytes", dict(n=len(data), sha256=hashlib.sha256(data).hexdigest()[:16], seed=HOSTILE_SEED))
    log_before = whole_log(ctx) if have_log else ""
    off_case = ctx.log_size()

    def hostile(cmd, tag, timeout):
        mark = ctx.cmd_mark()
        ctx.be.type("%s; echo %s >> %s" % (cmd, tag, done), 12, enter=True)
        ran = ctx.until(lambda: tag in ctx.lines(done), timeout, step=0.25)
        r.check(bool(ran), "%s: the command did not finish within %d s (%s has no %s)" % (tag, timeout, done, tag))
        if ctx.prof.commands():
            ctx.until(lambda: any(tag in c for c in ctx.cmds_since(mark)), 10)      # its rec.log line comes with the next prompt
        time.sleep(1.0)
        ctx.shot("hostile_" + tag)
        ctx.reset()                                                                   # what a user does next: Ctrl-C, Ctrl-U
        return bool(ran)

    try:
        _ok, base_ui = typed_exact(ctx, r, "before", eff, present)
        find_base = do_find(ctx, present) if have_log else None
        hostile("cat %s" % binf, "d1", 180)
        _ok, ui1 = typed_exact(ctx, r, "after 2 MB of binary output", eff, "b" + t)
        hostile(program_cmd(ctx, "hostile_out.py", "script", t), "d2", 120)
        _ok, ui2 = typed_exact(ctx, r, "after the hostile script", eff, "c" + t)
        if not have_log:
            r.unverifiable("no app log: ERROR lines, the main-thread bound and Find were not judged")
        else:
            find_after = do_find(ctx, present)
            find_absent = do_find(ctx, absent, wait=4.0)
            r.metric("find", dict(before=find_base, after=find_after, absent_token=find_absent))
            verdict, text = judge_find(find_base, find_after, find_absent)
            if verdict == "noref":
                r.unverifiable(text)
            else:
                r.check(verdict == "ok", text)
            typed_exact(ctx, r, "after Find (focus back in the input)", eff, "d" + t)
            panics, new, known = new_errors(log_before, ctx.log_since(off_case))
            r.metric("errors", dict(panics=len(panics), new=len(new), also_before_the_case=len(known)))
            r.check(not panics, "the app panicked: %r" % panics[:2])
            r.check(not new, "%d kind(s) of ERROR line the launch had not logged before the hostile output: %r" % (len(new), new[:3]))
            if known:
                r.note("ERROR kinds logged during the case AND before it (not judged here): %r" % known[:4])
            ok, text = judge_ui_after(base_ui, max(ui1, ui2))
            r.metric("ui_item_max_ms", dict(before=base_ui, after_bytes=ui1, after_script=ui2, during=C.ui_item_audit(ctx.log_since(off_case))["max_ms"]))
            r.check(ok, text)
            if ok:
                r.note(text)
        r.check(ctx.app.alive() if ctx.app is not None else True, "the app is not alive after the hostile output")
    finally:
        ctx.be.type(hostile_out.RESTORE_PRINTF, 12, enter=True)        # cleanup only, after every verdict (see the docstring)
        time.sleep(0.8)
        ctx.reset()
        ctx.rm(eff, done, binf)


# ---------------------------------------------------------------- T9 meta_and_word_keys
WORD_LEN = 7


def alt_is_meta(toml):
    """pure: does this settings.toml make the LEFT Alt / Option key act as Meta (terminal.input.extra_meta_keys, table or inline form)? The injectors
    press the left key."""
    body = ""
    m = re.search(r"^\[terminal\.input\.extra_meta_keys\][^\n]*\n((?:(?!\[)[^\n]*\n?)*)", toml, re.M)
    if m:
        body += m.group(1)
    m = re.search(r"^\s*extra_meta_keys\s*=\s*\{([^}]*)\}", toml, re.M)
    if m:
        body += "\n" + m.group(1)
    return bool(re.search(r"\bleft_alt\s*=\s*true\b", body))


def key_words(t, i):
    """the six words of step i for run token t (3 chars): each 7 chars, unique to the step and the run, so no earlier line is a ghost-text
    suggestion and no file completes them"""
    c = "0123456789abcdefghijklmnopqrstuvwxyz"[i % 36]
    return {k: "zq%s%s%s" % (k, c, t) for k in "abcdxj"}


def prompt_steps(t, family, meta):
    """pure: the prompt half as data. family 'mac' | 'other' (Linux, Windows); meta: the Alt / Option key acts as Meta. Each step: name, key (the
    key under test, as written for the report), ops (('t', text) types, ('k', spec, n) presses), readline (the line readline semantics give, None
    when this platform's script is not a readline one), default (the line the app's DOCUMENTED different default gives, else None), issue (an open
    issue the readline expectation is known to fail for), skip (a reason this step is not run here), seed (a command to run first).
    Sources: app/src/editor/view/mod.rs (the binding table and its comments), docs/design/r10-r14-w1b.md."""
    mac = family == "mac"
    L = WORD_LEN
    dot = "alt+." if mac else "alt+period"            # the macOS injector names keys by character
    wl, wr = ("alt+Left", "alt+Right") if mac else ("ctrl+Left", "ctrl+Right")
    no_meta = None if (meta or not mac) else "macOS: Option types a character by design unless it is set to act as Meta (terminal.input.extra_meta_keys)"
    alt_issue = None      # #2079 worked in round 15: Alt-B / Alt-F are expected to PASS off macOS; a failure is a regression, not a known issue
    steps = []

    def add(name, key, ops, readline, default=None, issue=None, skip=None, seed=None):
        steps.append(dict(name=name, key=key, ops=ops, readline=readline, default=default, issue=issue, skip=skip, seed=seed))

    w = key_words(t, 1)
    add("alt_b", "Alt-B", [("t", "echo %s %s" % (w["a"], w["c"])), ("k", "alt+b", 1), ("t", w["b"] + " ")], "echo %s %s %s" % (w["a"], w["b"], w["c"]), issue=alt_issue, skip=no_meta)
    w = key_words(t, 2)
    add("alt_f", "Alt-F", [("t", "echo %s %s" % (w["a"], w["c"])), ("k", "Left", 2 * L + 1), ("k", "alt+f", 1), ("t", " " + w["b"])], "echo %s %s %s" % (w["a"], w["b"], w["c"]), issue=alt_issue, skip=no_meta)
    w = key_words(t, 3)
    add("alt_d", "Alt-D", [("t", "echo %s %s %s" % (w["a"], w["x"], w["c"])), ("k", "Left", 2 * L + 1), ("k", "alt+d", 1), ("t", w["b"])], "echo %s %s %s" % (w["a"], w["b"], w["c"]), skip=no_meta)
    w = key_words(t, 4)
    add("alt_dot", "Alt-.", [("t", "echo %s " % w["b"]), ("k", dot, 1)], "echo %s %s" % (w["b"], w["d"]), skip=no_meta, seed="echo %s %s" % (w["a"], w["d"]))
    w = key_words(t, 5)
    add("ctrl_a", "Ctrl-A", [("t", "%s %s" % (w["a"], w["b"])), ("k", "ctrl+a", 1), ("t", "echo ")], "echo %s %s" % (w["a"], w["b"]), default=None if mac else "echo")
    w = key_words(t, 6)
    add("ctrl_e", "Ctrl-E", [("t", "echo %s %s" % (w["a"], w["c"])), ("k", "Left", L), ("t", w["b"] + " "), ("k", "ctrl+e", 1), ("t", " " + w["d"])],
        "echo %s %s %s %s" % (w["a"], w["b"], w["c"], w["d"]), default=None if mac else "echo %s %s  %s%s" % (w["a"], w["b"], w["d"], w["c"]))
    w = key_words(t, 7)
    add("ctrl_k", "Ctrl-K", [("t", "echo %s %s %s %s" % (w["a"], w["b"], w["x"], w["d"])), ("k", "Left", 2 * L + 1), ("k", "ctrl+k", 1), ("t", w["c"])], "echo %s %s %s" % (w["a"], w["b"], w["c"]))
    w = key_words(t, 8)
    add("ctrl_u", "Ctrl-U", [("t", "echo %s %s" % (w["x"], w["j"])), ("k", "ctrl+u", 1), ("t", "echo %s %s" % (w["a"], w["b"]))], "echo %s %s" % (w["a"], w["b"]))
    w = key_words(t, 9)
    add("ctrl_w", "Ctrl-W", [("t", "echo %s %s %s" % (w["a"], w["b"], w["x"])), ("k", "ctrl+w", 1), ("t", w["c"])], "echo %s %s %s" % (w["a"], w["b"], w["c"]))
    w = key_words(t, 10)
    if mac:      # readline: Ctrl-K kills, Ctrl-Y yanks the killed text back where the cursor now is
        add("ctrl_y", "Ctrl-Y", [("t", "echo %s %s" % (w["x"], w["a"])), ("k", "Left", L), ("k", "ctrl+k", 1), ("k", "Left", L + 1), ("k", "ctrl+y", 1)], "echo %s%s" % (w["a"], w["x"]))
    else:        # the documented Linux / Windows default: Ctrl-Y deletes from the cursor to the line start
        add("ctrl_y", "Ctrl-Y", [("t", "%s echo %s" % (w["j"], w["a"])), ("k", "Left", 5 + L), ("k", "ctrl+y", 1)], None, default="echo %s" % w["a"])
    w = key_words(t, 11)
    add("home", "Home", [("t", "%s %s" % (w["a"], w["b"])), ("k", "Home", 1), ("t", "echo ")], "echo %s %s" % (w["a"], w["b"]))
    w = key_words(t, 12)
    add("end", "End", [("t", "echo %s %s" % (w["a"], w["c"])), ("k", "Left", L), ("t", w["b"] + " "), ("k", "End", 1), ("t", " " + w["d"])], "echo %s %s %s %s" % (w["a"], w["b"], w["c"], w["d"]))
    w = key_words(t, 13)
    add("delete", "Delete", [("t", "echo %s x%s" % (w["a"], w["b"])), ("k", "Left", L + 1), ("k", "Delete", 1)], "echo %s %s" % (w["a"], w["b"]),
        skip="macOS: the injector has no forward-Delete key code (its `delete` is Backspace, key code 51)" if mac else None)
    w = key_words(t, 16)     # #2079: Alt-Backspace cuts the word before the cursor (Linux / Windows: a default since round 15; macOS: Option-Backspace always was)
    add("alt_backspace", "Alt-Backspace", [("t", "echo %s %s" % (w["a"], w["x"])), ("k", "alt+BackSpace", 1), ("t", w["b"])], "echo %s %s" % (w["a"], w["b"]))
    w = key_words(t, 14)
    add("word_left", "Option-Left" if mac else "Ctrl-Left", [("t", "echo %s %s" % (w["a"], w["c"])), ("k", wl, 1), ("t", w["b"] + " ")], "echo %s %s %s" % (w["a"], w["b"], w["c"]))
    w = key_words(t, 15)
    add("word_right", "Option-Right" if mac else "Ctrl-Right", [("t", "echo %s %s" % (w["a"], w["c"])), ("k", "Left", 2 * L + 1), ("k", wr, 1), ("t", " " + w["b"])], "echo %s %s %s" % (w["a"], w["b"], w["c"]))
    return steps


def classify_key_line(got, readline, default):
    """pure: 'readline' | 'app_default' | 'fail' for the line the shell received (None: nothing). Trailing blanks are not compared (a shell's
    history may keep or drop them)."""
    g = None if got is None else got.rstrip()
    if g is not None and readline is not None and g == readline.rstrip():
        return "readline"
    if g is not None and default is not None and g == default.rstrip():
        return "app_default"
    return "fail"


def raw_expect(family, meta):
    """pure: the raw half as [(label, key spec, expected bytes or None, reason when None)]: what a program with the terminal gets for each key
    (crates/warp_terminal/src/model/escape_sequences.rs: Alt is the Meta prefix off macOS, Meta on macOS; CSI 1;<mods> for modified arrows)."""
    mac = family == "mac"
    out = []
    alt_why = None
    if mac and not meta:
        alt_why = "macOS: Option types a character by design unless it is set to act as Meta"
    elif not mac and meta:
        alt_why = "an Alt key set to act as Meta changes the encoding off macOS; not derived"
    for ch, name in (("b", "Alt-B"), ("f", "Alt-F"), ("d", "Alt-D"), (".", "Alt-.")):
        spec = "alt+" + ("period" if ch == "." and not mac else ch)
        out.append((name, spec, None if alt_why else b"\x1b" + ch.encode(), alt_why))
    for ch, code in (("a", 1), ("e", 5), ("k", 11), ("u", 21), ("w", 23), ("y", 25)):
        out.append(("Ctrl-" + ch.upper(), "ctrl+" + ch, bytes([code]), None))
    out.append(("Home", "Home", b"\x1b[H", None))
    out.append(("End", "End", b"\x1b[F", None))
    out.append(("Delete", "Delete", None if mac else b"\x1b[3~", "macOS: the injector has no forward-Delete key code" if mac else None))
    if mac:
        out.append(("Option-Left", "alt+Left", b"\x1b[1;3D", None))
        out.append(("Option-Right", "alt+Right", b"\x1b[1;3C", None))
    else:
        out.append(("Ctrl-Left", "ctrl+Left", b"\x1b[1;5D", None))
        out.append(("Ctrl-Right", "ctrl+Right", b"\x1b[1;5C", None))
    return out


def run_ops(ctx, ops, delay):
    """send a step's ops; the Enter rides in the same injector call as the last op (an instant Enter)"""
    for i, op in enumerate(ops):
        last = i == len(ops) - 1
        if op[0] == "t":
            ctx.be.type(op[1], delay, enter=last)
        else:
            ctx.be.keys([op[1]] * op[2] + (["Return"] if last else []), delay_ms=delay)


def _settings_text(ctx):
    try:
        return open(U.settings_path(ctx), encoding="utf-8", errors="replace").read()
    except OSError:
        return ""


@case("meta_and_word_keys", order=96, gui=True)
def c_meta_and_word_keys(ctx, r):
    """T9 (r14-test-gaps G17). (1) At the prompt, one command per key: Alt-B, Alt-F, Alt-D, Alt-., Ctrl-A, E, K, U, W, Y, Home, End, Delete, Alt-Backspace, the
    word-left and word-right keys (Ctrl-arrows; Option-arrows on macOS). Each is built so a key that does nothing, types its letter or cuts the
    wrong span gives another line; the line the shell received (rec.log; PSReadLine history in the PowerShell pane) must be the readline result or,
    where the source documents a different default on purpose (Linux / Windows: Ctrl-A selects all, Ctrl-E does not move, Ctrl-Y deletes to the line
    start), that default, named in the metric as `app_default`. Alt-B / Alt-F / Alt-Backspace are word keys off macOS since #2079 (round 15) and must pass. (2) The same
    keys into a raw reader: the exact bytes (bytes_reader.py; bytes_reader.sh in the Git Bash pane; not in the PowerShell pane, where a program
    gets key records, not bytes). Runs late in a pass: Ctrl-A / Ctrl-E show the Linux "emacs bindings" banner."""
    family = "mac" if ctx.be.name == "mac" else "other"
    meta = alt_is_meta(_settings_text(ctx))
    r.metric("alt_acts_as_meta", meta)
    t = ctx.tok(3).lower()[-3:]
    rows, not_run = [], []
    any_recv = False
    for st in prompt_steps(t, family, meta):
        if st["skip"]:
            not_run.append("%s: %s" % (st["key"], st["skip"]))
            continue
        delay = ctx.rng.randint(10, 25)
        if st["seed"]:
            m = recv_mark(ctx)
            ctx.be.type(st["seed"], delay, enter=True)
            if recv_wait(ctx, m) != [st["seed"]]:
                r.check(False, "%s: its seeding command was not received exactly (%r)" % (st["key"], recv_since(ctx, m)[:2]))
                ctx.reset()
                continue
            time.sleep(0.4)
        m, off = recv_mark(ctx), ctx.log_size()
        run_ops(ctx, st["ops"], delay)
        got = recv_wait(ctx, m)
        any_recv = any_recv or bool(got)
        line = got[0] if len(got) == 1 else None
        outcome = classify_key_line(line, st["readline"], st["default"])
        rows.append(dict(key=st["key"], outcome=outcome, got=got[:2]))
        if outcome == "fail":
            held = CAG.enter_buffers(ctx.log_since(off)) if ctx.log_path else []
            r.check(False, "%s at the prompt: the shell received %r; readline gives %r%s%s; the app's Enter held %r" % (
                st["key"], got[:2], st["readline"], (", the documented app default %r" % st["default"]) if st["default"] else "",
                (" (known: #%d)" % st["issue"]) if st["issue"] else "", held[-1] if held else None))
            if held and held[-1][1] == "AI":
                ctx.be.key("ctrl+c")           # the line went to the agent: stop the request
                time.sleep(C.Ctx.EXIT_WINDOW_S + 0.4)
            ctx.reset()
        time.sleep(0.3)
    r.metric("prompt_keys", rows)
    defaults = [x["key"] for x in rows if x["outcome"] == "app_default"]
    if defaults:
        r.note("app default, not readline (documented in editor/view/mod.rs; an owner question, not a failure): %s" % ", ".join(defaults))
    if not_run:
        r.note("not run at the prompt: " + "; ".join(not_run))
    if ctx.pane == "powershell" and not any_recv:
        r.unverifiable("PSReadLine history gained no line in the whole prompt half (file missing or not written): the received lines were not verified")
    ctx.shot("meta_keys_prompt")
    ctx.reset()

    # (2) a program has the terminal
    if ctx.pane == "powershell":
        r.note("raw half not run in the PowerShell pane: a native console program reads key records, not bytes (the Git Bash pane runs it)")
        return
    rf = "mk_%s_bytes.txt" % t
    ctx.rm(rf, rf + ".ready")
    if ctx.win:
        ctx.be.type('bash "%s" %s 90' % (os.path.join(HERE, "bytes_reader.sh").replace("\\", "/"), rf), 12, enter=True)
    else:
        ctx.be.type("python3 %s %s 90" % (os.path.join(HERE, "bytes_reader.py"), rf), 12, enter=True)
    if not ctx.until(lambda: os.path.exists(os.path.join(ctx.W, rf + ".ready")), 20):
        r.check(False, "the raw reader never started")
        ctx.reset()
        return
    time.sleep(0.4)
    raw_rows = []
    for label, spec, expect, why in raw_expect(family, meta):
        if expect is None:
            not_run.append("%s into a program: %s" % (label, why))
            continue
        n0 = len(U._reader_chunks(ctx, rf))
        ctx.be.key(spec)
        ctx.until(lambda: len(b"".join(U._reader_chunks(ctx, rf)[n0:])) >= len(expect), 3)
        time.sleep(0.3)
        got = b"".join(U._reader_chunks(ctx, rf)[n0:])
        raw_rows.append(dict(key=label, got=got.hex(), want=expect.hex()))
        r.check(got == expect, "%s with a program in front: the pty received %s, expected %s" % (label, got.hex() or "nothing", expect.hex()))
    r.metric("raw_keys", raw_rows)
    if [x for x in not_run if "into a program" in x]:
        r.note("not run into a program: " + "; ".join(x for x in not_run if "into a program" in x))
    ctx.shot("meta_keys_raw")
    ctx.be.key("ctrl+c")                           # ends the reader
    time.sleep(1.6)                                # past the agent view's exit-confirmation window before reset() sends its own Ctrl-C
    ctx.reset()
    ctx.rm(rf, rf + ".ready")


# ---------------------------------------------------------------- T10 first_run_defaults
_PRECMD_RX = re.compile(r"hop=precmd_seen")


def first_run_dirs(kind):
    """pure: (the config directory moved aside and put back, the state directories wiped), relative to the scratch home. Windows keeps its data in
    known folders (WindowsApp.prepare wipes them), so both are empty there. macOS note: preferences the OS keeps outside HOME (cfprefsd) are not
    reachable from the harness."""
    if kind == "linux":
        return ".config/phosphor", [".local/state", ".local/share", ".cache/phosphor"]
    if kind == "mac":
        return ".phosphor", ["Library/Application Support", "Library/Caches", "Library/Preferences", "Library/Saved Application State"]
    return None, []


def judge_first_run_keys(j):
    """pure: ('ok' | 'fail' | 'noref', text) from injacct.join()'s dict for the keys sent after the first-run launch"""
    if not j or j.get("seen") is None:
        return "noref", "no app key trace for the first-run launch: 'keys sent = keys the app saw' was not judged"
    lost_enter = max((j.get("enters_sent") or 0) - (j.get("enters_seen") or 0), 0)
    if j.get("lost") or lost_enter or j.get("app_undelivered"):
        return "fail", "first run: the injector sent %d key(s) (%d Enter), the app's key trace has %d (%d Enter)%s: keys were taken before the app's key dispatch (a dialog, focus)" % (
            j["sent"], j["enters_sent"], j["seen"], j["enters_seen"], ("; %d reached the app and were never dispatched" % j["app_undelivered"]) if j.get("app_undelivered") else "")
    return "ok", "first run: the injector sent %d key(s), the app's key trace has %d" % (j["sent"], j["seen"])


def _first_prompt_s(ctx, kind, t0, timeout):
    """seconds from start() to the first shell prompt of the new instance (prompt.log; the app's precmd hop in the hook-less PowerShell pane)"""
    if kind != "windows":
        if not ctx.until(lambda: any(x >= t0 for x in U.prompt_times(ctx)), timeout, step=0.1):
            return None
        return min(x for x in U.prompt_times(ctx) if x >= t0) - t0
    if not ctx.app.wait_window(60):
        return None

    def seen():
        lp = ctx.app.find_log()
        if lp:
            ctx.log_path = lp
        ts = [x for x, _l in ctx.app_hop_lines(_PRECMD_RX, t0 - 1) if x is not None] if ctx.log_path else []
        return min(ts) if ts else None
    x = ctx.until(seen, timeout, step=0.25)
    return max(x - t0, 0.0) if x else None


@case("first_run_defaults", order=97, gui=True)
def c_first_run_defaults(ctx, r):
    """T10 (r14-test-gaps G18). The app is stopped, its config directory moved aside and its state wiped, and it is launched with NO settings file
    (the harness keeps only its shell rc and the logging environment). Required: a shell prompt within 90 s and within APPCI_STARTUP_MAX (20 s);
    with no Escape and no click first, the first command typed in this pass's view (at most one view-switch key before it) is received exactly;
    every key the injector sent is in the app's key trace (no dialog took them); no panic and no ERROR line whose kind this pass's ordinary launch
    had not logged. The metric `first_run_view` says which view a default first run shows. Afterwards the profile is put back and the app
    relaunched. The Git Bash pane SKIPs: it exists only through a setting."""
    import cases_robust as R
    if ctx.app is None or not (hasattr(ctx.app, "start") and hasattr(ctx.app, "stop")):
        r.skip("needs an app handle with stop()/start() (not the sim backend)")
    if ctx.variant or (ctx.win and ctx.pane != "powershell"):
        r.skip("the %s pane exists only through session.startup_shell_override, a non-default setting: the default-settings first run is judged in the PowerShell pane" % (ctx.variant or ctx.pane))
    home = ctx.prof.home
    if not os.path.realpath(home).startswith(os.path.realpath(ctx.run) + os.sep):
        r.skip("the profile home %s is not inside the run directory: refusing to wipe it" % home)
    kind = "windows" if ctx.win else ("mac" if ctx.be.name == "mac" else "linux")
    limit = float(os.environ.get("APPCI_STARTUP_MAX", "20"))
    ordinary = whole_log(ctx)
    k0 = len(ctx.be.calls) if getattr(ctx.be, "calls", None) is not None else None
    saved, patched = [], False
    ctx.snapshot_log()
    ctx.app.stop()
    try:
        cfg_rel, wipe = first_run_dirs(kind)
        if kind == "windows":
            ctx.app.prepare()
        else:
            src = os.path.join(home, cfg_rel)
            if os.path.isdir(src):
                aside = src + ".appci_saved"
                shutil.rmtree(aside, ignore_errors=True)
                os.rename(src, aside)
                saved.append((aside, src))
            for rel in wipe:
                shutil.rmtree(os.path.join(home, rel), ignore_errors=True)
        if kind in ("mac", "windows"):
            ctx.app._install_settings = lambda: None      # this launch only: start() must not put the profile's settings back
            patched = True
        t0 = time.time()
        ctx.app.start()
        first = _first_prompt_s(ctx, kind, t0, 90)
        r.metric("first_prompt_s", first if first is not None else -1)
        if first is None:
            r.check(False, "first run with default settings: no shell prompt within 90 s (app alive: %s)" % ctx.app.alive())
            ctx.shot("first_run_no_prompt")
            return
        r.check(first <= limit, "first run with default settings: the first prompt took %.1f s (bound %.0f s, APPCI_STARTUP_MAX)" % (first, limit))
        if ctx.log_path:
            ctx.until(lambda: ctx.log_view_mode() is not None, 4, step=0.2)
        time.sleep(1.0 if kind == "windows" else 0.5)
        ctx.shot("first_run_before_keys")
        if kind == "mac":
            d = ctx.be.system_dialog(ocr=True)
            if d:
                r.check(False, "first run: a system dialog is on screen, no key was sent: %s" % d)
                return
        ctx.be.guard()
        view = ctx.log_view_mode() if ctx.log_path else None
        r.metric("first_run_view", view)
        if view is not None and view != ctx.mode:
            ctx.be.key(("cmd+Return" if kind == "mac" else "ctrl+shift+Return") if ctx.mode == "agent" else "Escape")
            got = CAG.wait_view_mode(ctx, ctx.mode)
            r.check(got == ctx.mode, "first run: the view-switch key did not give the %s view (hop=view_mode says %s)" % (ctx.mode, got))
            time.sleep(0.5)
        elif view is None:
            r.note("first run: no hop=view_mode line, the command is typed in whatever view is shown")
        tok = ctx.tok(8).lower()
        fname = "fr_%s.txt" % tok
        ctx.rm(fname)
        if kind == "windows":
            line = "Add-Content '%s' %s" % (os.path.join(ctx.W, fname).replace("\\", "/"), tok)
        else:
            line = "echo %s >> %s" % (tok, fname)
        mark, off = recv_mark(ctx), (ctx.log_size() if ctx.log_path else None)
        ctx.be.type(line, ctx.rng.randint(10, 25), enter=True)
        ctx.until(lambda: tok in ctx.lines(fname), 15)
        got = recv_wait(ctx, mark, 1, 5.0)
        ctx.shot("first_run_after_command")
        held = CAG.enter_buffers(ctx.log_since(off)) if off is not None else []
        r.check(ctx.lines(fname) == [tok], "first run: the first typed command did not run exactly once (%s holds %r, expected [%r]; the app's Enter held %r)" % (
            fname, ctx.lines(fname), tok, held[-1] if held else None))
        if got or kind != "windows":
            r.check(got == [line], "first run: the shell received %r, expected exactly [%r]" % (got[:3], line))
        else:
            r.unverifiable("first run: PSReadLine history has no new line: the exact received line was not verified (the effect file was)")
        time.sleep(1.0)
        if k0 is not None and ctx.log_path:
            calls = ctx.be.calls[k0:]
            w = injacct.window(calls, time.time(), t0)
            j = injacct.join(calls, injacct.app_keys(ctx.log_path, *w), injacct.app_enter_hops(ctx.log_path, *w), trace=injacct.app_trace(ctx.log_path, *w)) if w else None
            verdict, text = judge_first_run_keys(j)
            r.metric("first_run_keys", {k: j.get(k) for k in ("sent", "seen", "lost", "enters_sent", "enters_seen")} if j else None)
            if verdict == "noref":
                r.unverifiable(text)
            else:
                r.check(verdict == "ok", text)
        else:
            r.unverifiable("this backend keeps no injector record (or there is no app log): 'keys sent = keys the app saw' was not judged")
        panics, new, known = new_errors(ordinary, whole_log(ctx))
        r.metric("first_run_errors", dict(panics=len(panics), new=len(new), also_in_the_ordinary_launch=len(known)))
        r.check(not panics, "first run: the app panicked: %r" % panics[:2])
        r.check(not new, "first run: %d kind(s) of ERROR line this pass's ordinary launch had not logged: %r" % (len(new), new[:3]))
        if known:
            r.note("first run: ERROR kinds the ordinary launch logs too (not judged here): %r" % known[:4])
        ctx.rm(fname)
    finally:
        try:
            ctx.app.stop()
            if patched:
                try:
                    del ctx.app._install_settings
                except AttributeError:
                    pass
                if hasattr(ctx.app, "_installed_src"):
                    ctx.app._installed_src = None
            for aside, src in saved:
                shutil.rmtree(src, ignore_errors=True)
                os.rename(aside, src)
            R.start_app(ctx, r, "first_run_restore")      # keeps the first-run log (snapshot_log), then starts on the profile again
        except Exception as e:                            # never mask the case's own verdict
            r.check(False, "first_run_defaults: could not put the profile back and relaunch: %r" % (e,))
