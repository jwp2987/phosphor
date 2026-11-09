# appci/cases_win_input.py: the Windows input-parity cases (docs/design/windows-input-parity-audit.md, findings 3, 7, 8; issues #1884, #1888, #1889).
# Registered with the same @case decorator as cases.py (one import line at its end). Windows panes only; each SKIPs elsewhere with its reason.
#   ssh_connect_typing_wrapper / ssh_nested_typeahead_wrapper   Git Bash pane, the managed-ssh wrapper pinned on (APPCI_SSH_WRAPPER=1 in the profile):
#                                                               the existing ssh cases, plus proof from the app log that the wrapper announcement was ACCEPTED
#                                                               on the born-owned ConPTY window (hop ssh_wrapper "announced reclaimed=true", #1884)
#   ps_ctrl_break                                               PowerShell pane: Ctrl+Break reaches a program as CTRL_BREAK_EVENT (#1888)
#   ps_fast_command_young_enter                                 PowerShell pane: a fast command's Enter and the next line typed at once run once each, in order (#1889)
import os, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases  # noqa: E402
from cases import case  # noqa: E402

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
    trials, got = 3, []
    for i in range(trials):
        ctx.rm(out)
        ctx.be.type("& '%s' '%s' '%s'" % (py, os.path.join(ctx.W, probe).replace("\\", "/"), os.path.join(ctx.W, out).replace("\\", "/")), 10, enter=True)
        if not ctx.until(lambda: "READY" in ctx.lines(out), 15):
            r.check(False, "trial %d: the probe program never started" % (i + 1))
            ctx.be.key("ctrl+c")
            time.sleep(1.0)
            continue
        ctx.be.key("ctrl+break")
        ctx.until(lambda: any(t in ctx.lines(out) for t in ("BREAK", "INT")), 6)
        seen = ctx.lines(out)
        got.append(seen)
        if "BREAK" not in seen and "INT" not in seen:
            ctx.be.key("ctrl+c")           # leave the pane usable
            time.sleep(1.0)
        time.sleep(0.5)
    r.metric("ctrl_break_runs", got)
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
