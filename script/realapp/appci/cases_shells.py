# appci/cases_shells.py: #1534 the startup_shell_override passes (Windows): cmd.exe and PowerShell 7 as the pane's shell, chosen through the app's own
# `session.startup_shell_override` setting (run.py: APPCI_WIN_SHELLS="pwsh7,cmd"; winmap.VARIANT_CASES says which cases each pass runs).
#   startup_shell_pane   the pane runs the shell the setting names (an effect proves its identity), typed lines at 10-25 ms/key arrive exactly and in
#                        order, a running command is interrupted by Ctrl-C and the next line runs, the app stays up
#   giveup_keys_reach_shell  owner decision 2026-11-09 (reversing #1661): lines and bare Enters typed with no pause arrive exactly and in order, the app log
#                        shows no discard and no young-Enter hand-back, and every give-up wrote its keys to the pty (PROVING)
# Registered with the same @case decorator as cases.py (imported by its last line).
import os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases  # noqa: E402  (cases.py imports this module as its last line, so everything below is already defined)
from cases import case  # noqa: E402

# per variant: (the line that writes the shell's identity into who.txt, what the file must hold (lower case), a long-running command)
IDENT = {
    "pwsh7": ("Add-Content -LiteralPath who.txt -Value ('PS{0}_{1}' -f $PSVersionTable.PSVersion.Major, $PSVersionTable.PSEdition)", "ps7_core", "Start-Sleep 60"),
    "cmd": ("echo CMDOK_%COMSPEC:~-7%>>who.txt", "cmdok_cmd.exe", "ping -n 60 127.0.0.1 >nul"),
}
# r15 (r14-test-gaps T46): the WSL pass. /proc/version of a WSL kernel names Microsoft; Git Bash (msys) has no such line and PowerShell has no `$(`...`)` grep.
IDENT["wsl"] = ("echo WSLOK_$(grep -ci microsoft /proc/version) >> who.txt", "wslok_1", "sleep 60")


def pane_pass_skip_reason(win):
    """pure: why startup_shell_pane does not run outside a pane-shell pass. Since r15 (T46) the app-test-windows job names the passes in APPCI_WIN_SHELLS,
    so on Windows this is the ordinary PowerShell / Git Bash pass saying where the case runs; Linux and macOS have no such pass at all."""
    if win:
        return ("runs in the pane-shell passes only (terminal@pwsh7, terminal@cmd, terminal@wsl: APPCI_WIN_SHELLS of the app-test-windows job names them; "
                "a pass whose shell is not installed is one startup_shell_pass SKIP row with the machine change it needs)")
    return "Windows only: the pane-shell passes (PowerShell 7, cmd.exe, WSL chosen through the app's own shell settings) exist on the Windows runner; no machine change makes it run here"


def identity_ok(variant, lines):
    """True when who.txt (lines) names the shell the variant should be: PowerShell 7 reports edition Core and major version 7, cmd.exe expands %COMSPEC%
    (a PowerShell pane would write the unexpanded text), a WSL shell counts one `microsoft` line in /proc/version"""
    want = IDENT[variant][1]
    return any(l.strip().lower() == want for l in lines)


def identity_hint(variant, lines):
    """pure: what a wrong identity line most likely means, for the failure text"""
    if not lines:
        return " (nothing was written: the line did not run in any shell that understands it)"
    if variant == "cmd" and any("%comspec" in l.replace("\0", "").lower() for l in lines):      # Windows PowerShell's `>>` writes UTF-16
        return " (the text was not expanded: the pane is PowerShell, the app fell back to its default shell; #2084: cmd.exe is not a shell type the app starts)"
    return ""


@case("startup_shell_pane", modes=("terminal",), order=72, gui=True)
def c_startup_shell_pane(ctx, r):
    """#1534: the pane's shell is the one `session.startup_shell_override` names. (1) identity by effect, (2) N lines typed at 10-25 ms/key with an instant
    Enter arrive exactly once and in order, (3) a long-running command is ended by Ctrl-C and the next line runs, (4) the app is still up. Runs only in the
    cmd / pwsh7 / wsl passes (r15: played by the app-test-windows job)."""
    variant = getattr(ctx, "variant", None)
    if variant not in IDENT:
        r.skip(pane_pass_skip_reason(ctx.win))
    ident, _want, longcmd = IDENT[variant]
    ctx.rm("who.txt")
    ctx.be.type(ident, 12, enter=True)
    ctx.until(lambda: ctx.lines("who.txt"), 8)
    got = ctx.lines("who.txt")
    if variant == "pwsh7":      # r26: the resolved executable of the pane's shell, logged (the job-object process list can miss it)
        ctx.rm("who_path.txt")
        ctx.be.type("Add-Content -LiteralPath who_path.txt -Value ([Diagnostics.Process]::GetCurrentProcess().MainModule.FileName)", 12, enter=True)
        ctx.until(lambda: ctx.lines("who_path.txt"), 8)
        r.note("pwsh7 pane shell resolved to: %s" % (ctx.lines("who_path.txt") or ["(not written)"])[0].strip())
    if not r.check(identity_ok(variant, got), "the pane is not %s: who.txt holds %r%s" % (variant, got, identity_hint(variant, got))):
        ctx.shot("startup_shell_%s_identity" % variant)
        ctx.reset()
        return                 # every line below is in this shell's syntax: typed into another shell it would only add noise
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


@case("ctrl_c_no_esc_i_local", order=73, gui=True)
def c_ctrl_c_no_esc_i_local(ctx, r):
    """#1798/#1672 (local, both modes; the ssh lab variant is lab_ctrl_c_no_esc_i): `sleep 30`, then Ctrl-C x1/x2/x3 at gaps 0/50/200 ms, then a typed echo.
    The echo's effect file is exact, the shell received exactly [sleep 30, the echo] (no ESC-i / `^[i` ran as text), the app log never says
    "Session abandoned before bootstrap", and one OCR at the end finds no visible `^[i`. Skips where the pane has no rec.log (Windows PowerShell)."""
    if ctx.win:
        r.skip("the ESC-i line report is a Unix tty echo failure; the Windows pane has no rec.log")
    f = "ccl_%s.txt" % ctx.tok(4)
    ctx.rm(f)
    toks = []
    for nctl in (1, 2, 3):
        for gap in (0, 50, 200):
            label = "x%d gap %d ms" % (nctl, gap)
            off, mark = ctx.log_size(), ctx.cmd_mark()
            ctx.be.type("sleep 30", 12, enter=True)
            time.sleep(0.8)
            for i in range(nctl):
                ctx.be.key("ctrl+c")
                if i < nctl - 1 and gap:
                    time.sleep(gap / 1000.0)
            tok = ctx.tok(8).lower()
            toks.append(tok)
            time.sleep(0.4)
            ctx.be.type(ctx.append_cmd(ctx.pane_shell(), tok, f), 12, enter=True)
            ctx.until(lambda: len(ctx.lines(f)) >= len(toks), 10)
            time.sleep(0.3)
            r.check(ctx.lines(f) == toks, "[%s] effect file %r, expected %r" % (label, ctx.lines(f), toks))
            new = ctx.cmds_since(mark)
            r.check(len(new) == 2 and new[0].strip() == "sleep 30" and tok in new[-1], "[%s] the shell received %r, expected exactly [sleep 30, the echo]" % (label, new))
            r.check(not any("\x1bi" in c or "^[i" in c for c in new), "[%s] an ESC-i (^[i) reached the shell: %r" % (label, new))
            r.check("Session abandoned before bootstrap" not in ctx.log_since(off), "[%s] the app log says 'Session abandoned before bootstrap'" % label)
    txt = ctx.ocr("ctrl_c_no_esc_i_local_end")
    if txt is None:
        r.note("no OCR: a visible ^[i on screen was not looked for")
    else:
        r.check("^[i" not in txt and "^[ i" not in txt, "a literal ^[i is on screen after the Ctrl-C rounds")
    ctx.shot("ctrl_c_no_esc_i_local_end")


# Owner decision 2026-11-09 (reversing #1661): a hold that gives up writes its keys to the pty in typed order, with no toast. One log hop per give-up.
_GIVE_UP_HOP_RE = re.compile(r"hop=give_up_keys_to_pty\b.*?\bwhy=(\w+) keys=(\d+) bytes=(\d+) written=(\d+) to_editor=(\d+) pty_gone=(\d)")
# What a build from before that decision logs when a hold gives up: the #1661 discard, or the young Enter's hand-back to the editor (the one that showed
# "Input was waiting too long, so it was not sent" for every Enter at a prompt without sequence numbers).
_OLD_GIVE_UP_MARKS = ("hop=enter_gate_typed_after_discarded", "hop=gate_typed_after_enter_discarded", "hop=young_hold_handed_back", "typed after Enter were discarded")
_OLD_YOUNG_HAND_BACK_RE = re.compile(r"hop=prompt_hold_handed_back\b[^\n]*\bkind=PromptSeen\b")


def give_up_facts(log):
    """Pure: (give-ups, old-style marks) from an app-log window. A give-up is one `give_up_keys_to_pty` hop: why, keys, bytes, written (to the pty),
    to_editor (bytes that became input text because nothing could take the write) and pty_gone. Old-style marks are the lines only a build that still
    discards or hands back at a give-up writes."""
    ups = [dict(why=m.group(1), keys=int(m.group(2)), bytes=int(m.group(3)), written=int(m.group(4)), to_editor=int(m.group(5)), pty_gone=m.group(6) == "1")
           for m in _GIVE_UP_HOP_RE.finditer(log or "")]
    old = [m for m in _OLD_GIVE_UP_MARKS if m in (log or "")]
    if _OLD_YOUNG_HAND_BACK_RE.search(log or ""):
        old.append("hop=prompt_hold_handed_back kind=PromptSeen")
    return ups, old


# Owner decision 2026-11-09, second ruling ("keep them together"): the give-up of an Enter that did NOT run keeps the keys typed after it WITH its line in
# the input (one hop per such give-up); nothing of them is written to the pty.
_KEPT_WITH_LINE_HOP_RE = re.compile(r"hop=give_up_keys_kept_with_line\b.*?\bwhy=(\w+) keys=(\d+) chars=(\d+) line_chars=(\d+)")


def kept_with_line_facts(log):
    """Pure: the give-ups that kept keys with an unrun line in the input (`give_up_keys_kept_with_line`): why, keys, chars (of text they became) and
    line_chars (the unrun line's length; 0 = an empty input, nothing to run ahead of)."""
    return [dict(why=m.group(1), keys=int(m.group(2)), chars=int(m.group(3)), line_chars=int(m.group(4))) for m in _KEPT_WITH_LINE_HOP_RE.finditer(log or "")]


# #2012 (round 9 Windows, PowerShell): a command the shell started by itself (PowerShell sends a Preexec for an EMPTY typed-ahead line too) took input from
# an editor that already held text; the keys typed while it ran came back in the shell's report and were put in FRONT of that text (`Add-Co` -> `d-AdCo`).
# The app now remembers the editor's text at that Preexec (`handoff_report_anchor_set anchor_chars=N`, N=0: the editor was empty) and places the report
# behind it (`handoff_insert_anchor anchor_chars=N`).
_REPORT_ANCHOR_SET_RE = re.compile(r"hop=handoff_report_anchor_set\b[^\n]*?\banchor_chars=(\d+)")
_REPORT_ANCHOR_USED_RE = re.compile(r"hop=handoff_insert_anchor\b[^\n]*?\banchor_chars=(\d+)")


def report_anchor_facts(log):
    """Pure: (takeovers, with_text, placed) from an app-log window: how often a shell-started command took input from the editor, how often the editor
    held text then (an anchor was recorded), and how many reports were placed behind such text instead of at the front of the input (#2012)."""
    sets = [int(m.group(1)) for m in _REPORT_ANCHOR_SET_RE.finditer(log or "")]
    return len(sets), sum(1 for n in sets if n > 0), len(_REPORT_ANCHOR_USED_RE.findall(log or ""))


def ran_out_of_order(exp, got):
    """Pure: the keep-together rule on the lines that RAN (`got`) against the lines typed (`exp`, in typed order): a line may only have run if every
    line typed before it ran before it, i.e. `got` is a prefix of `exp`. -> None when it holds, else (index, expected, got) of the first line that ran
    out of its place (a later line that ran without the line before it, a line run twice, a line nobody typed)."""
    for i, g in enumerate(got):
        if i >= len(exp) or exp[i] != g:
            return (i, exp[i] if i < len(exp) else None, g)
    return None


@case("giveup_keys_reach_shell", order=74)
def c_giveup_keys_reach_shell(ctx, r):
    """Owner decisions 2026-11-09 (#1661 reversed, docs/design/user-annoyance-review.md item 1; then "keep them together"): keys typed after an Enter are
    never dropped, and a later line never runs without the line typed before it. A hold whose line RAN (or is certainly a shell line) gives up by writing
    its keys to the pty in typed order, as a plain terminal would have sent them; a hold whose line did NOT run keeps the keys typed after it with that
    line in the input, visible, in typed order (`cd build`, Enter, `make clean`, Enter never runs `make clean` alone).
    Ten rounds of: a line, its Enter, a BARE Enter at the empty prompt, the next line and its Enter with no pause, another bare Enter (10-25 ms/key).
    (1) every line's effect is in the file exactly once and in typed order; (1b) KEEP-TOGETHER, judged on its own so a miss is named: no line ran without
    every line typed before it (the lines that ran are a prefix of the lines typed); (2) the app log has no discard and no young-Enter hand-back (the old
    behaviours); (3) every give-up that wrote to the pty wrote all its bytes (none became input text while the shell was alive); (4) a give-up that kept
    keys with an unrun line is reported with its counts: those lines are in the input by design, not run, so (1) fails for them and says why.
    The number of give-ups is a metric: zero means the timers were not exercised in this run (the lines still have to arrive exactly).
    #2012 metrics (`report_anchor_facts`): how often a shell-started command (on PowerShell the bare Enter's empty line) took input from the editor, how
    often the editor held text then, and how many reports were placed behind that text; the round 9 failure was a line of exactly that path."""
    f = "gup_%s.txt" % ctx.tok(4)
    ctx.rm(f)
    off = ctx.log_size()
    sh = ctx.pane_shell()
    exp = []
    for _ in range(10):
        a, b = ctx.tok(6).lower(), ctx.tok(6).lower()
        exp += [a, b]
        ctx.be.type(ctx.append_cmd(sh, a, f), 12, enter=True)
        ctx.be.key("Return")
        ctx.be.type(ctx.append_cmd(sh, b, f), 12, enter=True)
        ctx.be.key("Return")
        ctx.until(lambda: len(ctx.lines(f)) >= len(exp), 15)
    time.sleep(0.5)
    got = [l.strip().lower() for l in ctx.lines(f)]
    log = ctx.log_since(off)
    kept = kept_with_line_facts(log)
    r.metric("give_ups_kept_with_line", len(kept))
    r.check(got == exp, "%d/%d lines exact and in typed order (got %d lines); first difference: %r%s" % (
        sum(1 for x, y in zip(exp, got) if x == y), len(exp), len(got), next(((x, y) for x, y in zip(exp, got) if x != y), None),
        "; %d give-up(s) kept keys with an unrun line in the input (%s): those lines were not run, by the keep-together rule" % (
            len(kept), ", ".join("%s keys=%d line_chars=%d" % (k["why"], k["keys"], k["line_chars"]) for k in kept)) if kept else ""))
    bad = ran_out_of_order(exp, got)
    r.check(bad is None, "KEEP-TOGETHER broken: line %s of the file is %r where %r was typed: a line ran without every line typed before it (or twice)" % (
        (bad[0] + 1, bad[2], bad[1]) if bad else (0, None, None)))
    # #2012: how often this run took the path that reordered a line in round 9 (a metric, not a verdict: the lines above are the verdict).
    takeovers, with_text, placed = report_anchor_facts(log)
    r.metric("shell_started_takeovers", takeovers)
    r.metric("shell_started_takeovers_with_editor_text", with_text)
    r.metric("reports_placed_behind_editor_text", placed)
    ups, old = give_up_facts(log)
    r.metric("give_ups", len(ups))
    r.check(not old, "a hold gave up the old way (keys discarded, or an Enter handed back to the input with the 'waiting too long' toast): %r" % old)
    bounced = [u for u in ups if u["to_editor"] and not u["pty_gone"]]
    r.check(not bounced, "a give-up put keys into the input instead of writing them to the live pty: %r" % bounced)
    if ups:
        r.note("%d give-up(s), all written to the pty: %s" % (len(ups), ", ".join("%s keys=%d written=%d" % (u["why"], u["keys"], u["written"]) for u in ups)))
    else:
        r.note("no hold gave up in this run: the give-up write was not exercised here (unit and view tests cover it); the typed lines arrived exactly")
    ctx.reset()
