# appci/cases_win_handover.py (#1882, #1883, #1891; Windows input parity audit 2026-11-09, findings 1, 2, 10): secrets and interactive reads on ConPTY.
# ConPTY has no termios, so the app learns that a running command reads the tty from MARKERS the shell announces (PowerShell: the bootstrap's Read-Host /
# Get-Credential wrappers send a `TtyRead` marker; the app logs `hop=tty_read_marker secret=<bool>` when it handles one) and from the events that frame
# the prompt. These cases prove, in the real app and with effect files as the oracle, that
#   ps_read_host_secure          a secret typed into `Read-Host -AsSecureString` reaches the program (never the line editor, never a command, never the
#                                app log or PSReadLine history), whether the user waits for the prompt or types it at once behind the Enter (#1891: a late
#                                marker of the prompt the Enter answered must not re-baseline the hand-off into the editor)
#   gitbash_precmd_secret        Git Bash: a precmd hook that runs `read -s` after the prompt's Precmd: the PIN typed more than the 1.5 s prompt-hold give-up later
#                                reaches the read, it is not handed to the editor as text (#1883)
#   gitbash_typeahead_reps       Git Bash: the protocol of #1393 as a case: reps of 10 lines typed back to back with an instant Enter, half of them with a
#                                250 ms precmd function (the cost the issue names for `__git_ps1`), every rep's effect file and rec.log exact
# Platforms: Windows only (PowerShell pane / Git Bash pane); they skip elsewhere with their reason. Registered by the import at the end of cases.py.
import os, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases  # noqa: E402  (cases.py imports this module as its last line)
from cases import case  # noqa: E402

TTY_READ_HOP = "hop=tty_read_marker"
HOLD_GIVE_UP_S = 1.5     # CONPTY_PROMPT_HOLD_GIVE_UP (typeahead_handoff.rs): keys typed this long after the prompt are past the give-up


def tty_read_markers(log_text):
    """pure: the `secret=` values of every tty_read_marker hop in an app-log excerpt, in order ('true' / 'false')"""
    out = []
    for ln in (log_text or "").splitlines():
        i = ln.find(TTY_READ_HOP)
        if i >= 0:
            rest = ln[i + len(TTY_READ_HOP):].split()
            out.append(next((w.split("=", 1)[1] for w in rest if w.startswith("secret=")), ""))
    return out


def read_host_line(fname, prompt="pin"):
    """pure: the PowerShell line that reads a secure string and records ONLY its length and a stable check value (never the secret itself)"""
    return ("$s = Read-Host -AsSecureString -Prompt '%s'; Add-Content %s (\"len=\" + $s.Length + \" sum=\" + "
            "(([System.Net.NetworkCredential]::new('', $s).Password.ToCharArray() | ForEach-Object { [int]$_ } | Measure-Object -Sum).Sum))" % (prompt, fname))


def secret_digest(secret):
    """pure: what read_host_line records for `secret`"""
    return "len=%d sum=%d" % (len(secret), sum(ord(c) for c in secret))


def secret_leaks(texts, secret):
    """pure: which of the labelled texts contain the secret (a leak into the app log / a history file / a recorded command)"""
    return [label for label, t in texts if secret in (t or "")]


@case("ps_read_host_secure", modes=("terminal",), order=74, gui=True)
def c_ps_read_host_secure(ctx, r):
    """#1882/#1891: `Read-Host -AsSecureString` in the PowerShell pane. (A) the user waits for the marker (`hop=tty_read_marker secret=true`, the event), then types
    the secret at 10-25 ms/key + Enter: the program records the exact length and character sum, the next typed line still runs, the secret is in no app-log line, no
    PSReadLine history line and no command the shell received. (B) the secret is typed at once behind the command's Enter (no wait for any prompt): the same."""
    if not (ctx.win and ctx.pane == "powershell"):
        r.skip("PowerShell pane only (Read-Host -AsSecureString; the Git Bash pane's bash `read -s` announces nothing on ConPTY, see gitbash_precmd_secret)")
    f = "psrh.txt"
    ctx.rm(f)
    for label, wait_marker in (("A_waited", True), ("B_at_once", False)):
        secret = "Pw" + ctx.tok(8).replace("_", "") + "x"
        off, mark = ctx.log_size(), ctx.cmd_mark()
        ctx.be.type(read_host_line(f), 12, enter=True)
        if wait_marker:
            seen = ctx.until(lambda: "true" in tty_read_markers(ctx.log_since(off)), 20, step=0.2)
            ctx.shot("psrh_%s_prompt" % label)
            r.check(bool(seen), "%s: no `hop=tty_read_marker secret=true` in the app log within 20 s: the Read-Host marker never reached the app" % label)
            time.sleep(0.3)
        ctx.be.type(secret, ctx.rng.randint(10, 25), enter=True)
        want = secret_digest(secret)
        ok = ctx.until(lambda: want in ctx.lines(f), 20, step=0.3)
        ctx.shot("psrh_%s_done" % label)
        r.check(bool(ok), "%s: Read-Host did not receive the secret exactly: psrh.txt holds %r, want a line %r (the keys went to the line editor or were lost)" % (label, ctx.lines(f), want))
        tok = ctx.tok(8).lower()
        ctx.be.type(ctx.append_cmd("pwsh", tok, "psrh_after.txt"), 12, enter=True)
        r.check(bool(ctx.until(lambda: tok in ctx.lines("psrh_after.txt"), 10)), "%s: the line typed after Read-Host did not run" % label)
        time.sleep(0.5)
        leaks = secret_leaks([("app log", ctx.log_since(off)), ("PSReadLine history", "\n".join(ctx.ps_history_lines())),
                              ("commands the shell received", "\n".join(ctx.cmds_since(mark)))], secret)
        r.check(not leaks, "%s: the secret appears in: %s" % (label, ", ".join(leaks)))
    ctx.rm(f, "psrh_after.txt")


@case("gitbash_precmd_secret", modes=("terminal",), order=75, gui=True)
def c_gitbash_precmd_secret(ctx, r):
    """#1883 (port of subshell_ready_secret_after_prompt for ConPTY): a precmd function, registered after the prompt's Precmd hook was sent, runs `read -s` (a PIN
    prompt, a vault unlock). The shell is NOT at its line editor then, so no line-editor signal comes. The PIN is typed in two halves with a pause longer than
    the prompt-hold give-up (1.5 s) between them: it must reach the `read -s` whole, in typed order. Before the fix the give-up handed the first half to the editor
    as visible text. Oracles: the file the `read` wrote, no command received by the shell contains the PIN."""
    if not (ctx.win and ctx.pane == "gitbash"):
        r.skip("Git Bash pane only (bash precmd_functions; ConPTY prompt hold)")
    pin_file, go = "gbpin.txt", "gbgo.txt"
    ctx.rm(pin_file, go)
    pin = "pin" + ctx.tok(6).replace("_", "").lower()
    half = len(pin) // 2
    mark = ctx.cmd_mark()
    off = ctx.log_size()
    ctx.be.type('__pc_pin() { if [ -f %s ]; then rm -f %s; read -s -p "pin: " x; echo "$x" > %s; echo; fi; }; precmd_functions+=(__pc_pin)' % (go, go, pin_file), 12, enter=True)
    time.sleep(2.0)        # the install line's own prompt: the shell signalled at least once this session (the premise of the fix)
    ctx.be.type("touch " + go, 12, enter=True)
    r.check(bool(ctx.until(lambda: os.path.exists(os.path.join(ctx.W, go)), 10)), "the trigger file was not created: the pane did not run `touch`")
    time.sleep(0.6)
    ctx.be.type(pin[:half], ctx.rng.randint(10, 25))
    time.sleep(HOLD_GIVE_UP_S + 1.2)      # past the give-up cap armed by the first key
    ctx.shot("gbps_after_giveup")
    ctx.be.type(pin[half:], ctx.rng.randint(10, 25), enter=True)
    ok = ctx.until(lambda: ctx.lines(pin_file) == [pin], 20, step=0.3)
    ctx.shot("gbps_done")
    r.check(bool(ok), "the PIN did not reach `read -s` whole and in order: %s holds %r, want %r (the first half was handed to the editor as text, or keys were lost)" % (pin_file, ctx.lines(pin_file), [pin]))
    time.sleep(1.0)
    ran = [c for c in ctx.cmds_since(mark) if pin in c or pin[:half] in c]
    r.check(not ran, "the PIN (or half of it) reached the shell as a command: %r" % ran)
    tok = ctx.tok(8).lower()
    ctx.be.type(ctx.append_cmd("bash", tok, "gbps_after.txt"), 12, enter=True)
    r.check(bool(ctx.until(lambda: tok in ctx.lines("gbps_after.txt"), 10)), "the line typed after the read did not run")
    r.check("true" not in tty_read_markers(ctx.log_since(off)), "a bash pane reported a PowerShell tty-read marker")
    ctx.be.type("unset -f __pc_pin", 12, enter=True)
    ctx.rm(pin_file, go, "gbps_after.txt")


TYPEAHEAD_REPS = 24                 # the issue's own count per configuration (#1393: hold ON 2 of 24 reps lost a line, OFF 3 of 24)
TYPEAHEAD_LINES = 10
SLOW_PRECMD_S = 0.25                # what #1393's analysis measured for the prompt's `__git_ps1`: the time the shell is not at its line editor after a command


def typeahead_rep_plan(reps):
    """pure: [(tag, slow_precmd)] for `reps` repetitions: the first half at the pane's own prompt, the second half with the slow precmd function"""
    half = (reps + 1) // 2
    return [("tr%02d" % i, i >= half) for i in range(reps)]


@case("gitbash_typeahead_reps", order=76, gui=True)
def c_gitbash_typeahead_reps(ctx, r):
    """#1393 ("Git Bash 25 ms/key typeahead still loses lines"; 6 of 84 reps lost a line on build/r18bj, every loss permanent): the issue's protocol in the
    harness. Each rep types 10 `echo <token> >> file` lines back to back at 10-25 ms/key with an instant Enter (cases.effect_run, gap 0: the next line is
    typed while the previous command and its prompt are still running) and then demands the effect file AND the shell's rec.log to be exactly the typed
    lines, in order; effect_run waits up to 25 s for the file, so a late line is not a lost one. The second half of the reps runs with a precmd function
    that sleeps 250 ms: every line after the first is then typed while the shell is between a command and its line editor, the window the issue's
    analysis names. No reference terminal forgives a loss here: the runner's injector accounting on the case row says whether the keys reached the app.
    fast_typing types 30 such lines once per run; this case is the 24-rep count the issue was measured with."""
    if not (ctx.win and ctx.pane == "gitbash"):
        r.skip("Git Bash pane only (#1393: the ConPTY prompt hold of a bash pane on Windows)")
    plan = typeahead_rep_plan(ctx.count(TYPEAHEAD_REPS))
    bad, slow_on = [], False
    try:
        for tag, slow in plan:
            if slow and not slow_on:
                ctx.be.type("__tr_slow() { sleep %s; }; precmd_functions+=(__tr_slow)" % SLOW_PRECMD_S, 12, enter=True)
                time.sleep(2.0)
                slow_on = True
            before = len(r.fails)
            cases.effect_run(ctx, r, tag, TYPEAHEAD_LINES, 0.0)
            if len(r.fails) > before:
                bad.append(tag + ("(slow precmd)" if slow else ""))
    finally:
        if slow_on:
            time.sleep(1.0)
            ctx.be.type("__tr_slow() { :; }", 12, enter=True)        # the function stays registered and does nothing: no array surgery typed into the pane
            time.sleep(1.0)
    r.metric("typeahead_reps", dict(reps=len(plan), lines_per_rep=TYPEAHEAD_LINES, slow_precmd_reps=sum(1 for _, s_ in plan if s_), reps_with_a_bad_line=bad))
    ctx.rm(*["r_%s.txt" % tag for tag, _ in plan])
