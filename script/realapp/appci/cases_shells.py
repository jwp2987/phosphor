# appci/cases_shells.py: #1534 the startup_shell_override passes (Windows): cmd.exe and PowerShell 7 as the pane's shell, chosen through the app's own
# `session.startup_shell_override` setting (run.py: APPCI_WIN_SHELLS="pwsh7,cmd"; winmap.VARIANT_CASES says which cases each pass runs).
#   startup_shell_pane   the pane runs the shell the setting names (an effect proves its identity), typed lines at 10-25 ms/key arrive exactly and in
#                        order, a running command is interrupted by Ctrl-C and the next line runs, the app stays up
# Registered with the same @case decorator as cases.py (imported by its last line).
import os, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases  # noqa: E402  (cases.py imports this module as its last line, so everything below is already defined)
from cases import case  # noqa: E402

# per variant: (the line that writes the shell's identity into who.txt, what the file must hold (lower case), a long-running command)
IDENT = {
    "pwsh7": ("Add-Content -LiteralPath who.txt -Value ('PS{0}_{1}' -f $PSVersionTable.PSVersion.Major, $PSVersionTable.PSEdition)", "ps7_core", "Start-Sleep 60"),
    "cmd": ("echo CMDOK_%COMSPEC:~-7%>>who.txt", "cmdok_cmd.exe", "ping -n 60 127.0.0.1 >nul"),
}


def identity_ok(variant, lines):
    """True when who.txt (lines) names the shell the variant should be: PowerShell 7 reports edition Core and major version 7, cmd.exe expands %COMSPEC%
    (a PowerShell pane would write the unexpanded text)"""
    want = IDENT[variant][1]
    return any(l.strip().lower() == want for l in lines)


@case("startup_shell_pane", modes=("terminal",), order=72, gui=True)
def c_startup_shell_pane(ctx, r):
    """#1534: the pane's shell is the one `session.startup_shell_override` names. (1) identity by effect, (2) N lines typed at 10-25 ms/key with an instant
    Enter arrive exactly once and in order, (3) a long-running command is ended by Ctrl-C and the next line runs, (4) the app is still up. Runs only in the
    cmd / pwsh7 passes."""
    variant = getattr(ctx, "variant", None)
    if variant not in IDENT:
        r.skip("runs only in a startup_shell_override pass (APPCI_WIN_SHELLS=pwsh7,cmd)")
    ident, _want, longcmd = IDENT[variant]
    ctx.rm("who.txt")
    ctx.be.type(ident, 12, enter=True)
    ctx.until(lambda: ctx.lines("who.txt"), 8)
    got = ctx.lines("who.txt")
    r.check(identity_ok(variant, got), "the pane is not %s: who.txt holds %r" % (variant, got))
    ctx.shot("startup_shell_%s_identity" % variant)
    n = ctx.count(20)
    f = "ssp_lines.txt"
    ctx.rm(f)
    exp = []
    for _ in range(n):
        tok = ctx.tok(8).lower()
        exp.append(tok)
        ctx.be.type(ctx.append_cmd(ctx.pane_shell(), tok, f), ctx.rng.randint(10, 25), enter=True)
    ctx.until(lambda: len(ctx.lines(f)) >= n, 20 + n * 0.5)
    time.sleep(0.5)
    have = ctx.lines(f)
    r.check(have == exp, "%s: %d/%d typed lines arrived exactly and in order; first difference at %s" % (
        variant, sum(a == b for a, b in zip(exp, have)), n, next((i for i, (a, b) in enumerate(zip(exp + [None], have + [None])) if a != b), None)))
    # Ctrl-C ends a running command and the next line runs
    ctx.rm("ssp_after_cc.txt")
    ctx.be.type(longcmd, 12, enter=True)
    time.sleep(2.0)
    t_cc = time.time()
    ctx.be.key("ctrl+c")
    tok = ctx.tok(8).lower()
    time.sleep(0.6)
    ctx.be.type(ctx.append_cmd(ctx.pane_shell(), tok, "ssp_after_cc.txt"), 12, enter=True)
    ok = ctx.until(lambda: tok in ctx.lines("ssp_after_cc.txt"), 10)
    r.metric("ctrl_c_to_next_line_s", round(time.time() - t_cc, 2))
    r.check(bool(ok), "%s: Ctrl-C did not end the running command: the next line never ran within 10 s" % variant)
    ctx.shot("startup_shell_%s_after_ctrl_c" % variant)
    r.check(ctx.app.alive(), "the app died in the %s pane" % variant)
