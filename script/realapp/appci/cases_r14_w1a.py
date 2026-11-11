# appci/cases_r14_w1a.py: round 14, wave 1 (first half) of the new app cases of docs/design/r14-test-gaps.md 7.1. Design note: docs/design/r14-w1a.md.
# All five are in cases.PROVING (owner rule 2026-11-09: a new case runs in the fix tier; graduation is the owner's).
#   history_up_recall         T1, all five panes. Three commands; Up, Up, Enter runs the second again; Up, an edit, Enter runs the edited line; Up then Down
#                             gives an empty line and a new command runs alone. The shell's own record (rec.log; PSReadLine's history file in the
#                             PowerShell pane) holds exactly those six lines, and the effect files agree.
#   scroll_keys_position      T2, Wayland / X11 / macOS (OCR). 500 numbered lines, a background job that prints ten lines a second, PageUp three times, then
#                             block-up and Home: the view holds its place while output arrives, the first key typed brings the last line back, and the
#                             command typed with that key is exact.
#   find_basic                T3, Wayland / X11 / macOS (OCR and the app log), Windows Git Bash (app log only). Ten blocks, a token three times in the third;
#                             Find reports three matches (the app's `hop=find_state`), next and previous move the focus there and back, the match is on
#                             screen, and after Escape a typed command reaches the shell exactly.
#   tab_lifecycle_keys        T4, Wayland / X11 / both Windows panes. A tab running a loop: the close key asks, Escape keeps the loop running, Enter ends
#                             it (no beat after the undo-close window); a tab closed and reopened with the undo key returns in its directory and nothing
#                             runs again.
#   routing_table_powershell  T5, Windows PowerShell pane. The routing corpus for PowerShell: each line reached the shell (exact line in PSReadLine's
#                             history) or the agent (the app's routing decision, nothing in the history), as the table says.
# Same rules as cases.py: the EXACT line the shell received, typing at 10-25 ms per key with an instant Enter, a case that cannot run SKIPS with the
# capability that is missing, and a verdict that rests on evidence that is absent is NOREF, never a pass.
import os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases as C  # noqa: E402  (cases.py imports this module as its last line, so everything below is already defined)
import cases_caseagent as CA  # noqa: E402
import cases_robust as R  # noqa: E402
import cases_ui as U  # noqa: E402
import cases_wrap_vtabs as WV  # noqa: E402
from cases import case  # noqa: E402


# ---------------------------------------------------------------- shared
class Received:
    """The exact lines the pane's shell received since this object was made, from the shell's own record: rec.log (the scratch rc's history hook)
    where the pane has one, PSReadLine's history file in the Windows PowerShell pane (the app starts it with -NoProfile, so there is no rc hook).
    The harness's own probe lines are left out, as Ctx.cmds_since does."""

    def __init__(self, ctx):
        self.ctx = ctx
        self.ps = ctx.pane == "powershell"
        # r15 (T46): a cmd.exe pane has neither; its console keeps the history, read by typing `doskey /history>file` (once here, once per lines())
        self.cmd = ctx.pane == "cmd"
        self._dk_n, self._dk_ok = 0, False
        self.mark = len(self._doskey()) if self.cmd else (len(ctx.ps_history_lines()) if self.ps else ctx.cmd_mark())

    def _doskey(self):
        """the console history of the cmd.exe pane, without the probe lines themselves. One typed line per call: never call it between the steps
        of a recall (the probe would become the newest history entry)."""
        self._dk_n += 1
        f = "dk_%s_%d.txt" % (self.ctx.tok(4).lower(), self._dk_n)
        self.ctx.rm(f)
        self.ctx.be.type(DOSKEY_PROBE + f, 12, enter=True)
        got = self.ctx.until(lambda: self.ctx.lines(f), 6)
        time.sleep(0.3)
        self._dk_ok = self._dk_ok or bool(got)
        return doskey_lines(self.ctx.lines(f))

    def lines(self):
        if self.cmd:
            return [l for l in self._doskey()[self.mark:] if not self.ctx.HARNESS_MARKER_RE.search(l)]
        if self.ps:
            return [l for l in self.ctx.ps_history_lines()[self.mark:] if not self.ctx.HARNESS_MARKER_RE.search(l)]
        return self.ctx.cmds_since(self.mark)

    def recorded(self):
        """False when this pane's shell keeps no record at all (the rc hook did not run; PSReadLine wrote no history file): the exact line cannot
        be judged then, and the case says so instead of passing on the effect file alone"""
        if self.cmd:
            return self._dk_ok
        if self.ps:
            p = self.ctx.ps_history_path()
            return bool(p) and os.path.exists(p)
        return bool(self.ctx.prof.commands())

    def wait(self, want, timeout=6.0):
        """wait until the record holds `want` lines (an int) or satisfies it (a predicate over the list); a give-up net, the comparison decides"""
        if self.cmd:                 # reading the cmd.exe history types a line: no polling, the effect files are what the steps wait for
            return []
        pred = want if callable(want) else (lambda ls: len(ls) >= want)
        self.ctx.until(lambda: pred(self.lines()), timeout)
        return self.lines()


DOSKEY_PROBE = "doskey /history>"


def doskey_lines(lines):
    """pure: the commands in the text `doskey /history` printed, in order, without the probe lines that asked for it and without empty lines"""
    return [l.rstrip() for l in lines if l.strip() and not l.strip().lower().startswith(DOSKEY_PROBE)]


def first_diff(want, got):
    """pure: (index, wanted line or None, received line or None) of the first place two line lists differ, or None when they are equal"""
    for i in range(max(len(want), len(got))):
        a = want[i] if i < len(want) else None
        b = got[i] if i < len(got) else None
        if a != b:
            return i, a, b
    return None


def typed(ctx, text, enter=True):
    """type at the owner's rate: 10-25 ms per key, Enter in the same call"""
    ctx.be.type(text, ctx.rng.randint(10, 25), enter=enter)


def fast_keys(ctx, specs):
    """named keys at the owner's rate (10-25 ms apart), in one injector call"""
    ctx.be.keys(list(specs), delay_ms=ctx.rng.randint(10, 25))


def key_name(ctx, name):
    """a named key as this backend's injector spells it: PageUp is the X keysym `Prior` for wtype / xdotool, `pageup` for the macOS and Windows tables"""
    if name == "pageup":
        return "Prior" if ctx.be.name in ("wayland", "x11") else "pageup"
    return name


FIELD_CLEAR_BACKSPACES = 64


def clear_field(ctx):
    """empty the single-line field that has the keyboard: End, then more BackSpaces than a query of this file has characters (a BackSpace in an
    empty field does nothing). Not select-all + BackSpace: since round 16 (#2079) Ctrl-A is the start of the line in a text field off macOS and
    select-all is Ctrl-Shift-A, which an older build gives to another action; End and BackSpace mean the same on every build and platform."""
    ctx.be.key("End")
    ctx.be.key("BackSpace", repeat=FIELD_CLEAR_BACKSPACES, delay_ms=12)
    time.sleep(0.2)


def need_app_log(ctx, r, what):
    if not ctx.log_path or not os.path.exists(ctx.log_path):
        r.skip("no app log at %s: %s cannot be read" % (ctx.log_path, what))


# ---------------------------------------------------------------- T1 history_up_recall
def hist_edit_plan(shell, fname, tok):
    """pure: the edit made to a recalled line. -> dict(line: the line as first typed, backspaces, typed, edited: the line the shell must receive,
    eff_file / eff_tok: where the edited line's effect lands). bash-family: the file name is the end of the line, so the edit changes the file
    (`hu.txt` -> `hu_e.txt`); PowerShell's Add-Content ends in the token, so the edit changes the token."""
    if shell == "pwsh":
        line = "Add-Content %s %s" % (fname, tok)
        return dict(line=line, backspaces=3, typed="e9z", edited=line[:-3] + "e9z", eff_file=fname, eff_tok=tok[:-3] + "e9z")
    fmt = "echo %s>>%s" if shell == "cmd" else "echo %s >> %s"        # r15 (T46) cmd.exe: no space before `>>` (it would end up in the line)
    line = fmt % (tok, fname)
    stem = fname[:-4] if fname.endswith(".txt") else fname
    return dict(line=line, backspaces=len(fname) - len(stem), typed="_e.txt", edited=fmt % (tok, stem + "_e.txt"), eff_file=stem + "_e.txt", eff_tok=tok)


def hist_expected(shell, fname, toks):
    """pure: toks = 4 tokens (three seeded commands and the new one). -> (lines the shell must receive in order, {effect file: lines})."""
    cmd = (lambda t: "Add-Content %s %s" % (fname, t)) if shell == "pwsh" else (
        (lambda t: "echo %s>>%s" % (t, fname)) if shell == "cmd" else (lambda t: "echo %s >> %s" % (t, fname)))
    plan = hist_edit_plan(shell, fname, toks[1])
    lines = [cmd(toks[0]), cmd(toks[1]), cmd(toks[2]), cmd(toks[1]), plan["edited"], cmd(toks[3])]
    eff = {fname: [toks[0], toks[1], toks[2], toks[1]]}
    eff.setdefault(plan["eff_file"], []).append(plan["eff_tok"])
    eff[fname].append(toks[3])
    return lines, eff


def hist_clear_keys(shell):
    """pure: the keys of step (c), a recalled line given up for an empty one. Up then Down in readline and PSReadLine; cmd.exe's console editor has
    no empty line below the newest entry (Down does nothing there), its key for an empty line is Escape."""
    return ["Up", "Escape"] if shell == "cmd" else ["Up", "Down"]


def hist_diagnose(want, got, seeded):
    """pure: one sentence for a mismatch, naming the usual causes (a recall that picked another line, a doubled line, a merged one)"""
    d = first_diff(want, got)
    if d is None:
        return ""
    i, a, b = d
    step = ["the first typed command", "the second typed command", "the third typed command", "Up, Up, Enter (the second command again)",
            "Up, edit, Enter (the edited second command)", "Up, Down, then a new command"][i] if i < 6 else "an extra line"
    why = ""
    if b is None:
        why = "nothing was received for it (the Enter ran nothing, or an empty line)"
    elif a is None:
        why = "the shell received one line more than was typed"
    elif b in seeded and b != a:
        why = "the recall picked typed command %d" % (seeded.index(b) + 1)
    elif a in b and b != a:
        why = "the line arrived with extra text around it (a recalled line was not cleared, or two lines merged)"
    elif i > 0 and b == got[i - 1]:
        why = "the previous line ran twice"
    return "line %d, %s: expected %r, the shell received %r%s" % (i + 1, step, a, b, " (%s)" % why if why else "")


HIST_INLINE_MAX = 5000                 # app/src/terminal/input/inline_history/data_source.rs INLINE_BUILD_MAX_ENTRIES
ISSUE_UP_RECALL_ASYNC = 2107


def hist_file_lines(ctx, rec):
    """the number of lines of the history the pane's shell loads at start (PSReadLine's file for a PowerShell pane, ~/.bash_history or the zsh
    history of the profile's home elsewhere); None when no such file is readable. A count only: no line is read into a result."""
    try:
        if getattr(rec, "ps", False):
            return len(ctx.ps_history_lines())
        home = ctx.prof.home
        for name in (".bash_history", ".zsh_history", os.path.join(".zdot", ".zsh_history")):
            p = os.path.join(home, name)
            if os.path.isfile(p):
                with open(p, "rb") as f:
                    return sum(1 for _ in f)
    except Exception:
        pass
    return None


@case("history_up_recall", order=64, gui=True)
def c_history_up_recall(ctx, r):
    """T1: Up-arrow history recall in the pane's own shell. Three commands, then (a) Up, Up, Enter: the second command runs again, once; (b) Up, an
    edit at the end of the line, Enter: the edited line runs; (c) Up then Down: the line is empty again and a new command runs alone. The shell's
    record holds exactly those six lines and the effect files hold exactly their tokens: a wrong, doubled, merged or empty line fails."""
    shell = ctx.pane_shell()
    fname = "hu.txt"
    toks = [ctx.tok(8).lower() for _ in range(4)]
    plan = hist_edit_plan(shell, fname, toks[1])
    want, eff = hist_expected(shell, fname, toks)
    ctx.rm(*eff.keys())
    ctx.reset()
    rec = Received(ctx)

    def effect_count():
        return sum(len(ctx.local_lines(f)) for f in eff)

    def step(n_lines, n_effects, wait=8.0):
        """the step is over when its effect exists and the shell has logged it (the rc hook logs a line with the NEXT prompt)"""
        ctx.until(lambda: effect_count() >= n_effects, wait)
        rec.wait(n_lines, 4.0)
        time.sleep(0.4)

    for i in range(3):
        typed(ctx, want[i])
        step(i + 1, i + 1)
    if not r.check(effect_count() == 3, "the three seeding commands did not all run (effect lines %d of 3): the recall below has no history to use" % effect_count()):
        ctx.reset()
        return
    # (a) Up, Up, Enter at the typing rate
    fast_keys(ctx, ["Up", "Up", "Return"])
    step(4, 4)
    ctx.shot("history_up_recall_a")
    # (b) Up (the line just run is the newest), an edit at its end, Enter
    fast_keys(ctx, ["Up"] + ["BackSpace"] * plan["backspaces"])
    typed(ctx, plan["typed"])
    step(5, 5)
    # (c) Up then Down returns to the empty line; the new command is typed onto it
    fast_keys(ctx, hist_clear_keys(shell))
    typed(ctx, want[5])
    step(6, 6)
    ctx.shot("history_up_recall_c")
    time.sleep(0.6)                                   # a doubled line would land now
    got_eff = {f: ctx.local_lines(f) for f in eff}
    r.metric("history_effect_lines", {f: len(v) for f, v in got_eff.items()})
    for f in eff:
        d = first_diff(eff[f], got_eff[f])
        r.check(d is None, "%s holds %r, expected %r (first difference at line %s)" % (f, got_eff[f], eff[f], d[0] + 1 if d else None))
    got = rec.lines()
    r.metric("history_received_lines", len(got))
    # round 19 (#2107): how long the pane's history is. Above INLINE_BUILD_MAX_ENTRIES (5000, inline_history/data_source.rs) the app builds the
    # Up-arrow list on a background thread, and a key typed before it arrives is the suspected cause of the Windows agent-mode failure of the
    # round 14 candidate (the BackSpaces of step (b) had no effect on the recalled line). The number decides that on the next failure.
    hist_n = hist_file_lines(ctx, rec)
    r.metric("pane_history_lines", hist_n)
    if hist_n is not None and hist_n > HIST_INLINE_MAX and got != want:
        r.note("the pane's history holds %d lines, more than the %d the app lists inline: the Up-arrow list was built in the background in this run "
               "(#%d: keys typed before it arrives are applied first)" % (hist_n, HIST_INLINE_MAX, ISSUE_UP_RECALL_ASYNC))
    if not rec.recorded():
        r.unverifiable("this pane's shell kept no record of the lines it received (%s): the effect files were checked, the exact recalled and edited "
                       "lines were NOT" % ("no PSReadLine history file" if rec.ps else (
                           "`doskey /history` wrote nothing" if rec.cmd else "rec.log is empty: the rc hook did not run%s" % (
                               "; a WSL pane has no rc hook the harness can read (its home is inside the distribution). NEEDS: the harness's "
                               "history hook installed in the distribution's ~/.bashrc, which needs WSL on the VM first (owner)" if ctx.pane == "wsl" else ""))))
    elif rec.ps and len(got) != len(want) and all(first_diff(eff[f], got_eff[f]) is None for f in eff):
        r.unverifiable("PSReadLine's history holds %d new line(s) for 6 received (effect files exact): the history file was filtered or not yet "
                       "written, so the exact recalled and edited lines were NOT checked" % len(got))
    else:
        r.check(got == want, "the shell did not receive exactly the six lines: %s" % hist_diagnose(want, got, want[:3]))
        r.check(got.count(want[1]) == 2, "the recalled line %r is in the shell's record %d time(s), expected 2 (typed once, recalled once)" % (want[1], got.count(want[1])))
    ctx.reset()


# ---------------------------------------------------------------- T2 scroll_keys_position
SKP_CODE = "acdefhkmnprtuwxy"          # 16 glyphs tesseract does not confuse (a subset of cases_ui.OCR_SAFE): one base-16 digit each
SKP_LINES = 500
SKP_STREAM_LINES = 900                 # ten a second: longer than the case; the case stops the job itself
SKP_KEPT_MIN = 0.5                     # share of the numbered lines on screen before the wait that must still be there after it


def skp_word(tag, i):
    """pure: the word of numbered line i (0..4095): the tag and three base-16 digits written in SKP_CODE"""
    return tag + SKP_CODE[(i >> 8) & 15] + SKP_CODE[(i >> 4) & 15] + SKP_CODE[i & 15]


def skp_visible(text, tag, hi=None):
    """pure: the line numbers whose words an OCR text holds, sorted. A word is read whole or not at all. `hi`: the number of lines that were
    printed; a word that decodes to a number outside 1..hi cannot be on screen, it is a glyph read wrong (round 14, macOS: one `c` read as `e`
    turned line 394 into 906 of 500, and "the highest line on screen" then said the view had not moved). Such a word is dropped, never
    clamped: the lines read right still place the view, and a view that did not move still reads its real lines."""
    out = set()
    for m in re.finditer(re.escape(tag) + "([%s]{3})" % SKP_CODE, (text or "").lower()):
        a, b, c = (SKP_CODE.index(ch) for ch in m.group(1))
        n = (a << 8) | (b << 4) | c
        if hi is None or 1 <= n <= hi:
            out.add(n)
    return sorted(out)


def skp_tags(rng):
    """two different 3-glyph tags (numbered lines, stream lines); neither is a prefix of a word of the other"""
    a = "".join(rng.choice(SKP_CODE) for _ in range(3))
    while True:
        b = "".join(rng.choice(SKP_CODE) for _ in range(3))
        if b != a:
            return a, b


def skp_hold_verdict(before, after, stream_after, grew, kept_min=SKP_KEPT_MIN):
    """pure: did the view hold its place while output arrived? before / after = numbered lines read on screen before and after the wait, stream_after
    = stream lines read on screen after it, grew = lines the job printed during the wait. -> ("pass" | "fail" | "noref", reason).
    No output during the wait is a fail (nothing was tested); nothing readable before the wait is noref; a stream line on screen, or most of the
    lines that were on screen gone, is the view having moved."""
    if grew < 10:
        return "fail", "the background job printed %d line(s) during the wait, expected about 30: no output arrived, so holding the view proves nothing" % grew
    if len(before) < 3:
        return "noref", "OCR read %d numbered line(s) on screen before the wait: too few to say whether the view moved" % len(before)
    if stream_after:
        return "fail", "after the wait %d of the job's new lines are on screen: the view followed the output to the bottom" % len(stream_after)
    kept = len(set(before) & set(after)) / float(len(before))
    if kept < kept_min:
        return "fail", "after the wait %d%% of the numbered lines that were on screen are still there (lines %s..%s before, %s after): the view moved" % (
            int(kept * 100), before[0], before[-1], ("%s..%s" % (after[0], after[-1])) if after else "none")
    return "pass", ""


def anchor_words(text):
    """pure: the words of an OCR text a later read is compared with: lower-case alphanumeric runs of 5 or more characters"""
    return set(re.findall(r"[a-z0-9]{5,}", (text or "").lower()))


def anchors_kept(before, after):
    """pure: share of `before`'s anchor words still in `after` (0.0 when there were none)"""
    a = anchor_words(before)
    return len(a & anchor_words(after)) / float(len(a)) if a else 0.0


def skp_stream_cmd(src, seen, stop):
    """the background job (bash / zsh): one line of `src` every 100 ms, to the pane and to `seen`, until `stop` exists. In a subshell with `&`, so
    the pane's shell is at its prompt while the lines arrive (and prints no job number)."""
    return '(sleep 1; while read -r l; do [ -e %s ] && break; echo "$l"; echo "$l" >> %s; sleep 0.1; done < %s &)' % (stop, seen, src)


@case("scroll_keys_position", order=66, gui=True)
def c_scroll_keys_position(ctx, r):
    """T2: the view keeps its place while output arrives, and the first key brings the bottom back. 500 numbered lines; a background job prints ten
    lines a second (in the foreground PageUp belongs to the program, #2080); PageUp three times, 3 s of new output: the same numbered lines are still
    on screen and none of the new ones are. Then block-up and Home (top of the scrollback), 3 s more: the screen has not changed. Then one typed key:
    the job's last line is on screen, and the command that key starts reaches the shell exactly. Screen facts are OCR (every line carries a word
    in glyphs OCR does not confuse); a pane with no OCR engine skips."""
    if ctx.win:
        r.skip("Windows: the case reads the scroll position from the screen and the Windows runner has no OCR engine; the app logs no scroll-position hop yet")
    if ctx.ocr("skp_probe") is None:
        raise C.Skip("no OCR engine (tesseract-ocr; Apple Vision on macOS): the scroll position can only be read from the screenshot")
    tag, stag = skp_tags(ctx.rng)
    src, stream, seen, stop, eff = "skp_lines.txt", "skp_stream.txt", "skp_seen.txt", "skp_stop", "skp_eff.txt"
    ctx.rm(src, stream, seen, stop, eff)
    with open(os.path.join(ctx.W, src), "w", newline="\n") as f:
        f.write("".join("%s %d\n" % (skp_word(tag, i), i) for i in range(1, SKP_LINES + 1)))
    with open(os.path.join(ctx.W, stream), "w", newline="\n") as f:
        f.write("".join("%s\n" % skp_word(stag, i) for i in range(1, SKP_STREAM_LINES + 1)))
    stream_no = lambda: len(ctx.lines(seen))
    ctx.reset()
    rec = Received(ctx)
    cat, job = "cat %s" % src, skp_stream_cmd(stream, seen, stop)
    tok = ctx.tok(8).lower()
    last = "echo %s >> %s" % (tok, eff)
    try:
        typed(ctx, cat)
        rec.wait(1, 10)
        time.sleep(1.0)
        at_bottom = skp_visible(ctx.ocr("skp_0_bottom"), tag, SKP_LINES)
        r.metric("skp_lines_read_at_bottom", len(at_bottom))
        if len(at_bottom) < 3:
            r.unverifiable("OCR read %d of the numbered lines on a screen that shows the last of 500 (shots/*skp_0_bottom.png): the scroll position "
                           "cannot be read, nothing was judged" % len(at_bottom))
            return
        r.check(at_bottom[-1] >= SKP_LINES - 15, "control: before any scroll key the last numbered lines are not on screen (highest read %d of %d)" % (at_bottom[-1], SKP_LINES))
        typed(ctx, job)
        if not r.check(bool(ctx.until(lambda: stream_no() >= 3, 12)), "the background job printed nothing within 12 s (%d line(s)): there is no output to hold the view against" % stream_no()):
            return
        # --- PageUp x3 at the prompt, then 3 s of new output
        ctx.be.keys([key_name(ctx, "pageup")] * 3, delay_ms=60)
        time.sleep(0.8)
        t1 = ctx.ocr("skp_1_pageup")
        v1, n1 = skp_visible(t1, tag, SKP_LINES), stream_no()
        r.metric("skp_after_pageup", dict(first=v1[0] if v1 else None, last=v1[-1] if v1 else None, read=len(v1)))
        moved = bool(v1) and v1[-1] < at_bottom[-1] - 10
        r.check(moved, "PageUp three times at the prompt did not move the view up: numbered lines on screen %s (before the keys %d..%d; shots/*skp_1_pageup.png)" % (
            ("%d..%d" % (v1[0], v1[-1])) if v1 else "none read", at_bottom[0], at_bottom[-1]))
        time.sleep(3.0)
        grew = stream_no() - n1
        t2 = ctx.ocr("skp_2_pageup_held")
        v2 = skp_visible(t2, tag, SKP_LINES)
        verdict, why = skp_hold_verdict(v1, v2, skp_visible(t2, stag, SKP_STREAM_LINES), grew)
        r.metric("skp_pageup_hold", dict(verdict=verdict, new_lines=grew, read_before=len(v1), read_after=len(v2)))
        if verdict == "noref":
            r.unverifiable("after PageUp: %s" % why)
        else:
            r.check(verdict == "pass", "after PageUp, 3 s of new output: %s (shots/*skp_2_pageup_held.png)" % why)
        # --- top of the scrollback: select the newest block (the terminal has the keys then), Home
        ctx.be.keys([WV.KX(ctx, "block_up"), "Home"], delay_ms=120)
        time.sleep(0.8)
        t3 = ctx.ocr("skp_3_top")
        v3, n3 = skp_visible(t3, tag, SKP_LINES), stream_no()
        went_up = t3 is not None and (not v3 or not v2 or v3[0] < v2[0])
        r.check(went_up, "block-up then Home did not move the view above where PageUp left it (numbered lines on screen %s, before %s; shots/*skp_3_top.png)" % (
            ("%d..%d" % (v3[0], v3[-1])) if v3 else "none", ("%d..%d" % (v2[0], v2[-1])) if v2 else "none"))
        time.sleep(3.0)
        grew_top = stream_no() - n3
        t4 = ctx.ocr("skp_4_top_held")
        kept, new_on_screen = anchors_kept(t3, t4), skp_visible(t4, stag, SKP_STREAM_LINES)
        r.metric("skp_top_hold", dict(new_lines=grew_top, anchor_words=len(anchor_words(t3)), kept=round(kept, 2), stream_lines_on_screen=len(new_on_screen)))
        if grew_top < 10:
            r.check(False, "at the top of the scrollback the job printed %d line(s) in 3 s, expected about 30: no output arrived, so holding the view proves nothing" % grew_top)
        elif len(anchor_words(t3)) < 5:
            r.unverifiable("at the top of the scrollback OCR read %d word(s) to compare (shots/*skp_3_top.png): whether the view held is not judged" % len(anchor_words(t3)))
        else:
            r.check(not new_on_screen and kept >= SKP_KEPT_MIN, "at the top of the scrollback, 3 s of new output: %d%% of the words on screen are still "
                    "there and %d of the job's new lines are on screen: the view moved (shots/*skp_3_top.png, *skp_4_top_held.png)" % (int(kept * 100), len(new_on_screen)))
        # --- stop the job, then ONE key: the bottom is back; that key starts the command
        R.touch(ctx, stop)
        n = stream_no()
        while True:                                   # the job checks the stop file every 100 ms: wait until it has printed its last line
            time.sleep(0.5)
            if stream_no() == n:
                break
            n = stream_no()
        tail = [l.strip() for l in ctx.lines(seen)[-8:]]
        typed(ctx, last[0], enter=False)
        time.sleep(0.8)
        t5 = ctx.ocr("skp_5_after_key")
        on_screen = [w for w in tail if w and w in (t5 or "").lower()]
        r.metric("skp_tail_lines_on_screen_after_key", len(on_screen))
        r.check(bool(on_screen), "after one typed key none of the job's last %d lines is on screen: the key did not bring the view back to the bottom "
                "(shots/*skp_5_after_key.png)" % len(tail))
        typed(ctx, last[1:])
        ctx.until(lambda: ctx.lines(eff), 8)
        got = rec.wait(3, 5)
        r.check(ctx.lines(eff) == [tok], "the command typed after the scroll keys wrote %r, expected [%r]" % (ctx.lines(eff), tok))
        if rec.recorded():
            r.check(got == [cat, job, last], "the shell received %r, expected exactly the three typed lines (the command whose first key ended the scroll: %r)" % (got, last))
    finally:
        R.touch(ctx, stop)
        WV.back_to_input(ctx, r)
        ctx.reset()


# ---------------------------------------------------------------- T3 find_basic
FIND_STATE_RE = re.compile(r"hop=find_state event=(\w+) count=(\d+) focused=(\d+|none) scanning=(true|false)")     # app/src/terminal/find/model.rs find_state_hop_detail
FIND_BLOCKS, FIND_BLOCK_LINES, FIND_HITS = 10, 30, 3


def find_states(text):
    """pure: every `hop=find_state` line of an app-log excerpt, in order, as dict(event, count, focused (int or None), scanning)"""
    return [dict(event=m.group(1), count=int(m.group(2)), focused=None if m.group(3) == "none" else int(m.group(3)), scanning=m.group(4) == "true")
            for m in FIND_STATE_RE.finditer(text or "")]


def find_nav_verdict(first, after_next, after_prev):
    """pure: failure text or None for the focused-match indexes after the first Enter, a second Enter (next) and Shift+Enter (previous): each is a
    match, next moves to another match, previous returns to the first"""
    if first is None or after_next is None or after_prev is None:
        return "a focused match is missing (first %s, after next %s, after previous %s)" % (first, after_next, after_prev)
    if after_next == first:
        return "next did not move the focus (match %d before and after)" % first
    if after_prev != first:
        return "previous did not return to the match next left (first %d, after next %d, after previous %d)" % (first, after_next, after_prev)
    return None


def find_fixture(rng, k, token=None, neighbour=None):
    """pure: the lines block k prints: FIND_BLOCK_LINES filler lines of OCR-safe words; with a token, line 15 holds it FIND_HITS times and line 14
    holds the neighbour word (which is never typed anywhere, so it is on screen only when that part of the block is)"""
    word = lambda: "".join(rng.choice(U.OCR_SAFE[:16]) for _ in range(6))
    lines = ["fb%02d %s %s" % (k, word(), word()) for _ in range(FIND_BLOCK_LINES)]
    if token:
        lines[13] = "near %s here" % neighbour
        lines[14] = " ".join([token] * FIND_HITS)
    return lines


def find_last_state(ctx, off, event):
    rows = [s for s in find_states(ctx.log_since(off)) if s["event"] == event]
    return rows[-1] if rows else None


def find_press(ctx, off, key, event="next", timeout=4.0):
    """press a key in the Find field and return the find_state hop it caused (None when the app logged none)"""
    n0 = len([s for s in find_states(ctx.log_since(off)) if s["event"] == event])
    ctx.be.key(key)
    ctx.until(lambda: len([s for s in find_states(ctx.log_since(off)) if s["event"] == event]) > n0, timeout)
    rows = [s for s in find_states(ctx.log_since(off)) if s["event"] == event]
    return rows[-1] if len(rows) > n0 else None


# ---- one Find, judged from what its Enter caused (round 19, #2109)
# Round 14 (hostile_output_recovers on four platforms, the control of restore_shows_history): opening the Find bar re-runs the PREVIOUS query
# (app/src/terminal/view.rs show_find_bar: "Start find using the previous query") and logs `hop=find_scroll result=scroll` for it, and every edit of
# the query runs a find too. A case that read every find_scroll line since the Find key therefore saw the earlier token's match in the Find for a
# token that was never printed. Only the lines logged AFTER the Enter belong to the query that was typed.
def find_scroll_hops(text):
    """pure: the payloads of the `hop=find_scroll` lines of an app-log excerpt, in order"""
    return [l.split("hop=find_scroll", 1)[1].strip()[:400] for l in (text or "").splitlines() if "hop=find_scroll" in l]


def find_enter_verdict(after_enter):
    """pure: 'scroll' | 'no_match' | None for the app log since the Enter of ONE Find. scroll: the app focused a match and went to it
    (`find_scroll result=scroll`). no_match: it said it had none (`find_scroll result=no_match`, or `find_state event=next count=0`). None: the
    Enter logged neither (it did not reach the Find field, or the alternate screen is up: no scroll is logged there)."""
    hops = find_scroll_hops(after_enter)
    if any(h.startswith("result=scroll") for h in hops):
        return "scroll"
    if any(h.startswith("result=no_match") for h in hops):
        return "no_match"
    if any(s["event"] == "next" and s["count"] == 0 for s in find_states(after_enter)):
        return "no_match"
    return None


def find_enter(ctx, token, wait=8.0, shot=None):
    """One Find for `token`: the Find key, the field emptied (an earlier query stays in it), the token typed, the scan awaited, THEN Enter; up to
    `wait` s for the app to go to a match; Escape. -> (find_enter_verdict of the log since the Enter, the find_scroll payloads since the Enter)."""
    off0 = ctx.log_size()
    ctx.be.guard()
    ctx.be.key(os.environ.get("APPCI_KEY_FIND", "cmd+f" if U.mac(ctx) else "ctrl+shift+f"))
    time.sleep(0.8)
    clear_field(ctx)
    typed(ctx, token, enter=False)
    ctx.until(lambda: (find_last_state(ctx, off0, "query") or {}).get("scanning") is False, 6)     # a build without hop=find_state: the full 6 s
    time.sleep(0.3)
    off = ctx.log_size()
    ctx.be.key("Return")
    ctx.until(lambda: find_enter_verdict(ctx.log_since(off)) == "scroll", wait, step=0.25)
    text = ctx.log_since(off)
    if shot:
        ctx.shot(shot)
    ctx.be.key("Escape")
    time.sleep(0.5)
    return find_enter_verdict(text), find_scroll_hops(text)


@case("find_basic", order=67, gui=True)
def c_find_basic(ctx, r):
    """T3: Find in an ordinary scrollback. Ten blocks of 30 lines; the third holds a token three times on one line. Open Find, a query that is not
    there (the app reports 0 matches: the control), then the token: the app reports 3 matches, the match is on screen (the app's own geometry;
    with OCR also a neighbour word that was not on screen before), Enter moves to another match and Shift+Enter back, Escape closes Find and the
    next typed command reaches the shell exactly: Find typed nothing into the shell and the focus returned."""
    need_app_log(ctx, r, "the app's find hops")
    if not ctx.prof.commands() and not ctx.prof.local_prompts:
        r.skip("this pane's shell has no rc hook (rec.log): the exact line after Escape cannot be proven here")
    alpha = U.OCR_SAFE[:16]
    token = "fq" + "".join(ctx.rng.choice(alpha) for _ in range(9))
    neighbour = "nb" + "".join(ctx.rng.choice(alpha) for _ in range(8))
    absent = token[::-1] + "q"
    eff = "fb_after.txt"
    ctx.rm(eff)
    for k in range(1, FIND_BLOCKS + 1):
        with open(os.path.join(ctx.W, "fb_%d.txt" % k), "w", newline="\n") as f:
            f.write("\n".join(find_fixture(ctx.rng, k, token if k == 3 else None, neighbour)) + "\n")
    ctx.reset()
    rec = Received(ctx)
    cats = ["cat fb_%d.txt" % k for k in range(1, FIND_BLOCKS + 1)]
    for i, c in enumerate(cats):
        typed(ctx, c)
        rec.wait(i + 1, 8)
    if not r.check(rec.lines() == cats, "the ten `cat` commands did not reach the shell as typed (received %r): there is nothing to search" % rec.lines()):
        return
    time.sleep(1.0)
    before = ctx.ocr("find_basic_before")
    if before is not None:
        r.check(neighbour not in before.lower().replace(" ", ""), "control: the third block's match line is on screen before Find (10 blocks of 30 lines did not scroll it away)")
    off = ctx.log_size()
    after_tok = ctx.tok(8).lower()
    after = "echo %s >> %s" % (after_tok, eff)
    closed = False
    try:
        ctx.be.guard()
        ctx.be.key(os.environ.get("APPCI_KEY_FIND", "cmd+f" if U.mac(ctx) else "ctrl+shift+f"))
        time.sleep(0.8)
        clear_field(ctx)                                                 # an earlier Find's query may still be in the field
        typed(ctx, absent, enter=False)
        ctx.until(lambda: (find_last_state(ctx, off, "query") or {}).get("scanning") is False, 6)
        none = find_last_state(ctx, off, "query")
        if none is None:
            r.unverifiable("the app logged no hop=find_state line after a query was typed into Find (a build without that hop, or Find did not open: "
                           "shots/*find_basic_open.png); the match count cannot be judged")
            ctx.shot("find_basic_open")
            return
        r.check(none["count"] == 0, "control: Find reports %d match(es) for a query that is nowhere in the scrollback" % none["count"])
        clear_field(ctx)
        typed(ctx, token, enter=False)
        time.sleep(1.0)
        # the first Enter: the count the bar shows, and the match it focused (pressed again while the async scan is still counting)
        first = None
        for _ in range(4):
            first = find_press(ctx, off, "Return")
            if first is None or first["count"] >= FIND_HITS or not first["scanning"]:
                break
            time.sleep(2.0)
        if first is None:
            r.check(False, "Enter in the Find field logged no hop=find_state event=next: the key did not reach Find")
            return
        r.metric("find_state_first", first)
        r.check(first["count"] == FIND_HITS, "Find reports %d match(es) for the token, expected %d (it is %d times on one line of the third block and nowhere else)" % (
            first["count"], FIND_HITS, FIND_HITS))
        # on screen: the app's own geometry of the last scroll, and with OCR the neighbour word
        time.sleep(0.6)
        hops = [l.split("hop=find_scroll", 1)[1].strip()[:400] for l in ctx.log_since(off).splitlines() if "hop=find_scroll" in l]
        scrolls = [h for h in hops if h.startswith("result=scroll")]
        geo = R.find_scroll_geometry(scrolls[-1]) if scrolls else None
        r.metric("find_scroll_hops", len(hops))
        shown = ctx.ocr("find_basic_match")
        seen = None if shown is None else neighbour in shown.lower().replace(" ", "")
        r.metric("find_match_on_screen", dict(ocr=seen, app=geo["on_screen"] if geo else None))
        if geo and geo["on_screen"] is False:
            r.check(False, "by the app's own geometry the focused match is OFF screen after Find (line %.1f of a %.1f-line viewport)" % (geo["match_line"], geo["viewport_lines"]))
        elif seen is False:
            r.check(False, "the match line is not on screen after Find: OCR does not read the word next to it (shots/*find_basic_match.png; app geometry: %s)" % (
                "on screen" if geo and geo["on_screen"] else "none logged"))
        elif seen is None and not (geo and geo["on_screen"]):
            r.unverifiable("no OCR engine and no usable find_scroll geometry in the app log (%d hop(s)): whether the match is on screen is not judged" % len(hops))
        nxt = find_press(ctx, off, "Return")
        prv = find_press(ctx, off, "shift+Return")
        bad = find_nav_verdict(first["focused"], nxt["focused"] if nxt else None, prv["focused"] if prv else None)
        r.metric("find_focus_trail", [first["focused"], nxt["focused"] if nxt else None, prv["focused"] if prv else None])
        r.check(bad is None, "next / previous in Find: %s" % bad)
        ctx.be.key("Escape")
        closed = True
        time.sleep(0.5)
        typed(ctx, after)
        ctx.until(lambda: ctx.lines(eff), 8)
        got = rec.wait(len(cats) + 1, 5)
        r.check(ctx.lines(eff) == [after_tok], "after Escape the typed command wrote %r, expected [%r]: the keys did not go to the shell's input" % (ctx.lines(eff), after_tok))
        d = first_diff(cats + [after], got)
        r.check(got == cats + [after], "after Find the shell's record is not the ten `cat` lines and the one command typed after Escape: line %s expected %r, "
                "received %r (Find typed into the shell, or the command did not arrive whole)" % ((d[0] + 1, d[1], d[2]) if d else (None, None, None)))
    finally:
        if not closed:
            ctx.be.key("Escape")
            time.sleep(0.3)
        WV.back_to_input(ctx, r)
        ctx.reset()


# ---------------------------------------------------------------- T4 tab_lifecycle_keys
REOPEN_KEY = os.environ.get("APPCI_KEY_REOPEN", "ctrl+alt+t")          # app/src/util/bindings.rs ReopenClosedSession on Linux and Windows
CLOSE_WARNING_LINE = "Displaying unsaved changes warning for pane"     # app/src/pane_group/mod.rs close_pane_with_confirmation (log::info!)
UNDO_GRACE_S = 60.0                                                    # general.undo_close.grace_period: a closed tab's shell lives this long
MAC_REOPEN_KEY = "cmd+shift+t"                                         # ReopenClosedSession on macOS


def reopen_key(ctx):
    return os.environ.get("APPCI_KEY_REOPEN") or (MAC_REOPEN_KEY if U.mac(ctx) else REOPEN_KEY)


def tl_pwd_cmd(ctx, path):
    """the line that appends the pane's cwd to the file `path` (an absolute path, forward slashes)"""
    if ctx.pane == "powershell":
        return "Add-Content %s (Get-Location).Path" % path
    return "%s >> %s" % (R.PWD(ctx), path)


def tl_cd_cmd(ctx, d):
    return ("Set-Location %s" if ctx.pane == "powershell" else "cd %s") % d


def tl_beat_cmd(shell, beat, stop, pidf=None):
    """pure: the long-running command of the tab: appends a line to `beat` twice a second until `stop` exists. With pidf (POSIX) the tab's shell
    first writes its own pid there."""
    if shell == "pwsh":
        return "while (-not (Test-Path %s)) { Add-Content %s b; Start-Sleep -Milliseconds 500 }" % (stop, beat)
    return "%swhile [ ! -e %s ]; do echo b >> %s; sleep 0.5; done" % ("echo $$ > %s; " % pidf if pidf else "", stop, beat)


def beat_verdict(samples, quiet_s=3.0):
    """pure: samples = [(seconds since the close was confirmed, beat count)], oldest first. -> the time after which the count never changed for at
    least quiet_s (the loop is gone), or None while it still beats at the end."""
    if not samples:
        return None
    last_change = samples[0][0]
    for (_t0, n0), (t1, n1) in zip(samples, samples[1:]):
        if n1 != n0:
            last_change = t1
    return last_change if samples[-1][0] - last_change >= quiet_s else None


def pid_alive(pid):
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False
    except OSError:
        return True


class Tabs:
    """the tab steps of tab_lifecycle_keys, with the pane-specific lines in one place"""

    def __init__(self, ctx, r):
        self.ctx, self.r, self.n = ctx, r, 0
        self.w = R.fwd(ctx.W)
        self.rec = Received(ctx)
        self.typed = []                 # every line this case typed that is not a cwd probe, in order
        self.unlogged = []              # typed lines whose command never returns (the loop): an rc hook logs a line when its command ENDS, so
                                        # rec.log never holds them; PSReadLine's history does. Left out of the comparison on both sides.

    def line(self, text, wait_file=None, wait_lines=1, timeout=8.0):
        self.typed.append(text)
        typed(self.ctx, text)
        if wait_file:
            self.ctx.until(lambda: len(self.ctx.lines(wait_file)) >= wait_lines, timeout)
        self.rec.wait(lambda ls: text in ls, 4.0)

    def where(self):
        """the cwd of the pane that has the keyboard (norm_path), or '<none>'"""
        self.n += 1
        f = "tl_pwd_%d.txt" % self.n
        self.ctx.rm(f)
        typed(self.ctx, tl_pwd_cmd(self.ctx, "%s/%s" % (self.w, f)))
        self.ctx.until(lambda: self.ctx.lines(f), 8)
        got = (self.ctx.lines(f) or ["<none>"])[0].strip()
        if got != "<none>":
            self.rec.wait(lambda ls: any(f in l for l in ls), 4.0)
        return R.norm_path(got) if got != "<none>" else got

    def count(self):
        """how many tabs the window has, from the app's hop=tab_activate at a last-tab key; None when the app logs none"""
        off = self.ctx.log_size()
        self.ctx.be.guard()
        self.ctx.be.key(R.tab_key(self.ctx, 9))
        time.sleep(0.6)
        return WV.window_tab_count(self.ctx, off)

    def new_tab(self, d):
        """open a tab (the agent pass puts it in agent view, as ui_vertical_tabs_many does) and move its shell to directory d"""
        ctx = self.ctx
        os.makedirs(d, exist_ok=True)
        p0, t0 = ctx.prof.prompts(), time.time()
        ctx.be.guard()
        ctx.be.key(R.NEWTAB)
        time.sleep(0.8)
        if ctx.prof.local_prompts:
            ctx.until(lambda: ctx.prof.prompts() > p0, 10, step=0.1)
        elif ctx.precmds_since(t0) is not None:
            ctx.until(lambda: ctx.precmds_since(t0) >= 1, 15)        # no rc hook (PowerShell pane): the app's own prompt report
            time.sleep(0.5)
        else:
            time.sleep(4.0)
        if ctx.mode == "agent" and ctx.log_view_mode() == "terminal":
            typed(ctx, "/agent")
            if not ctx.until(lambda: ctx.log_view_mode() == "agent", 10):
                self.r.note("a new tab opened as a terminal session and /agent did not show the agent view within 10 s")
            time.sleep(0.6)
        self.line(tl_cd_cmd(ctx, R.fwd(d)))
        time.sleep(0.3)

    def received_ok(self):
        """the shell's record, cwd probes left out, is exactly what was typed (nothing ran again, nothing was lost)"""
        if not self.rec.recorded():
            self.r.unverifiable("this pane's shell kept no record of the lines it received: the effect files were checked, 'nothing ran again' by the exact lines was NOT")
            return
        got = [l for l in self.rec.lines() if "tl_pwd_" not in l and l not in self.unlogged]
        want = [l for l in self.typed if l not in self.unlogged]
        if self.rec.ps and len(got) < len(want):
            self.r.unverifiable("PSReadLine's history holds %d of the %d lines typed: the exact lines were NOT checked (effect files were)" % (len(got), len(want)))
            return
        d = first_diff(want, got)
        self.r.check(d is None, "the tabs' shells did not receive exactly the typed lines: line %s expected %r, received %r (a line ran again after the "
                     "undo, or was lost)" % ((d[0] + 1, d[1], d[2]) if d else (None, None, None)))


@case("tab_lifecycle_keys", order=88, gui=True)
def c_tab_lifecycle_keys(ctx, r):
    """T4: closing tabs by key. (1) A tab runs a loop that appends a line twice a second. The close key asks first: Escape keeps the tab and the loop
    (still beating); the close key again and Enter closes the tab, and the loop is gone: no beat after the undo-close window (60 s) and, on POSIX,
    the tab's shell pid no longer exists. (2) A second tab, in its own directory, one command; the close key (nothing runs, no question), the undo
    key: the tab is back in its directory, its command did not run again, and the shell's record holds only the typed lines."""
    # macOS (#2061, r14-test-gaps T32): the close key is Cmd-W, sent to the app under test only (R.close_key -> MacBackend.app_chord); the question is
    # a native alert of the app's own process (NSAlert: Escape = Cancel, Return = the first button, 'Yes, close'), so the same two answers apply
    close = R.close_key(ctx)
    if not close:
        r.skip("%s%s" % (R.NO_CLOSE_WHY, (": " + R.mac_chords_why_not(ctx)) if U.mac(ctx) else ""))
    undo = reopen_key(ctx)
    shell = ctx.pane_shell()
    if shell == "cmd":
        r.skip("cmd.exe pane: no record of the received line and no loop in one line; the PowerShell and Git Bash panes run this case")
    ctx.reset()
    t = Tabs(ctx, r)
    home = R.norm_path(ctx.W)
    bad = WV.first_tab_precondition(t.where(), home)
    if bad:
        r.check(False, bad)
        return
    beat, stop, pidf, eff = "tl_beat.txt", "tl_stop", "tl_pid.txt", "tl_eff.txt"
    ctx.rm(beat, stop, pidf, eff)
    n0 = t.count()
    if n0 is None:
        r.note("the app logged no hop=tab_activate line: tab counts are not checked (the cwd probes and the loop's beat are)")
    d2, d3 = os.path.join(ctx.W, "tl_" + ctx.tok(5).lower()), os.path.join(ctx.W, "tl_" + ctx.tok(5).lower())
    tok3 = ctx.tok(8).lower()
    beats = lambda: len(ctx.lines(beat))
    t_confirm, opened = None, 0
    try:
        # ---- (1) a tab with a running command: close asks, Escape keeps, Enter ends
        t.new_tab(d2)
        opened += 1
        r.check(t.where() == R.norm_path(d2), "the new tab's shell is not in its directory (cwd probe): the lines below would run in the wrong tab")
        loop = tl_beat_cmd(shell, "%s/%s" % (t.w, beat), "%s/%s" % (t.w, stop), None if ctx.win else "%s/%s" % (t.w, pidf))
        t.typed.append(loop)
        t.unlogged.append(loop)
        typed(ctx, loop)
        if not r.check(bool(ctx.until(lambda: beats() >= 2, 15)), "the loop typed into the new tab never ran (no beat in 15 s): there is no running command to close"):
            return
        pid = None
        if not ctx.win:
            m = re.match(r"\s*(\d+)", (ctx.lines(pidf) or [""])[0])
            pid = int(m.group(1)) if m else None
        off = ctx.log_size()
        ctx.be.guard()
        ctx.be.key(close)
        asked = bool(ctx.until(lambda: CLOSE_WARNING_LINE in ctx.log_since(off), 4))
        ctx.shot("tab_lifecycle_dialog")
        warn = CA.setting_bool(ctx, "show_warning_before_quitting")
        r.metric("tl_close_asked", dict(asked=asked, show_warning_setting=warn))
        if not asked:
            b0 = beats()
            time.sleep(2.0)
            still = beats() > b0
            if warn is False:
                r.note("general.show_warning_before_quitting is false in this profile: the close key does not ask, by that setting; the cancel half is not run")
            else:
                r.check(False, "the close key on a tab with a running command did not ask (no %r line in the app log within 4 s); the loop is %s. "
                        "Escape was NOT sent (it would have gone to whatever has the keyboard)" % (CLOSE_WARNING_LINE, "still running" if still else "gone: the tab closed without a question"))
            if still:                                 # leave nothing running in a tab the case can no longer account for
                R.touch(ctx, stop)
            return
        ctx.be.key("Escape")                          # cancel
        time.sleep(0.6)
        b0 = beats()
        time.sleep(2.5)
        r.check(beats() >= b0 + 2, "after Escape on the close question the loop stopped (beats %d -> %d in 2.5 s): cancel did not keep the command" % (b0, beats()))
        if pid:
            r.check(pid_alive(pid), "after Escape on the close question the tab's shell (pid %d) is gone" % pid)
        if n0 is not None:
            n = t.count()
            r.check(n == n0 + 1, "after Escape on the close question the window has %s tab(s), expected %d" % (n, n0 + 1))
        off = ctx.log_size()
        ctx.be.guard()
        ctx.be.key(close)
        if not r.check(bool(ctx.until(lambda: CLOSE_WARNING_LINE in ctx.log_since(off), 4)), "the second close key did not ask again (no %r line within 4 s); Enter was NOT sent" % CLOSE_WARNING_LINE):
            R.touch(ctx, stop)
            return
        ctx.be.key("Return")                          # confirm
        t_confirm = time.time()
        opened -= 1
        time.sleep(1.0)
        if n0 is not None:
            n = t.count()
            r.check(n == n0, "after Enter on the close question the window has %s tab(s), expected %d (the tab did not close)" % (n, n0))
        r.check(t.where() == home, "after the tab closed, the pane that has the keyboard is not the first tab (cwd probe)")
        # ---- (2) close and undo, while the closed tab's undo window runs out
        t.new_tab(d3)
        opened += 1
        r.check(t.where() == R.norm_path(d3), "the second new tab's shell is not in its directory (cwd probe)")
        t.line(ctx.append_cmd(shell, tok3, "%s/%s" % (t.w, eff)), wait_file=eff)
        r.check(ctx.lines(eff) == [tok3], "the tab's command wrote %r, expected [%r]" % (ctx.lines(eff), tok3))
        off = ctx.log_size()
        ctx.be.guard()
        ctx.be.key(close)
        time.sleep(1.2)
        opened -= 1
        r.check(CLOSE_WARNING_LINE not in ctx.log_since(off), "the close key asked about a tab with nothing running")
        r.check(t.where() == home, "after the close key the pane that has the keyboard is not the first tab (cwd probe): the tab did not close")
        ctx.be.guard()
        ctx.be.key(undo)
        time.sleep(1.5)
        back = t.where()
        opened += 1 if back == R.norm_path(d3) else 0
        r.check(back == R.norm_path(d3), "after the undo key (%s) the pane that has the keyboard is in %r, expected the closed tab's directory %r%s" % (
            undo, back, R.norm_path(d3), " (the tab that ran the loop came back instead)" if back == R.norm_path(d2) else ""))
        if n0 is not None:
            n = t.count()
            r.check(n == n0 + 1, "after the undo key the window has %s tab(s), expected %d" % (n, n0 + 1))
        time.sleep(0.8)
        r.check(ctx.lines(eff) == [tok3], "after the undo the tab's command ran again: its file holds %r, expected [%r]" % (ctx.lines(eff), tok3))
        t.received_ok()
        if back == R.norm_path(d3):
            ctx.be.guard()
            ctx.be.key(close)                  # the case leaves one tab
            opened -= 1
            time.sleep(1.0)
        # ---- (1, end) the loop is gone once the undo window is over
        bound = float(os.environ.get("APPCI_UNDO_CLOSE_WAIT_S", str(UNDO_GRACE_S + 15)))
        samples = [(time.time() - t_confirm, beats())]
        gone = None
        while gone is None and time.time() - t_confirm < bound + 4:
            time.sleep(1.0)
            samples.append((time.time() - t_confirm, beats()))
            gone = beat_verdict(samples)
        r.metric("tl_loop_gone_after_s", round(gone, 1) if gone is not None else -1)
        r.check(gone is not None, "the loop of the tab closed with Enter is still running %.0f s after the close (undo window %d s): an orphan" % (time.time() - t_confirm, UNDO_GRACE_S))
        if pid and gone is not None:
            r.check(bool(ctx.until(lambda: not pid_alive(pid), 5)), "the loop stopped but the closed tab's shell (pid %d) still exists" % pid)
    finally:
        R.touch(ctx, stop)                            # whatever happened, nothing keeps beating
        for _ in range(max(0, opened)):               # a tab the case opened and could not account for: close it only while more than one is open
            n = t.count()
            if n is None or n <= 1 or (n0 is not None and n <= n0):
                break
            off = ctx.log_size()
            ctx.be.key(close)
            time.sleep(1.0)
            if CLOSE_WARNING_LINE in ctx.log_since(off):      # the close asked (the loop did not stop yet): confirm it
                ctx.be.key("Return")
                time.sleep(0.8)
        WV.back_to_input(ctx, r)
    r.check(t.where() == home, "at the end the pane that has the keyboard is not the first tab (cwd probe)")
    C.effect_run(ctx, r, "tl_end", 4, 0.0)
    r.check(ctx.app.alive(), "the app is not alive after the tab keys")


# ---------------------------------------------------------------- T5 routing_table_powershell
# The lines the routing corpus pins for a PowerShell session (crates/input_classifier/src/routing_corpus.rs, "#1234 PowerShell" and #1344) that are
# safe to run in a scratch directory, a command on PATH, and the two #1560 lines whose shell structure decides for an unknown first word.
PS_SHELL_LINES = ["Get-Date", "Write-Host hello world", "Get-ChildItem -Recurse", "dir", "git status", "echo $env:PATH", "$x = Get-Date", "$env:PATH -split ';'",
                  "@(1,2,3)", "Get-Content log.txt | Select-String error", "ls | Where-Object { $_.Length -gt 1kb }",
                  "Get-Process | Sort-Object CPU -Descending | Select-Object -First 5",
                  "xyz_nocmd > out_nc.txt", "foo_nocmd --bar | grep x",
                  # #2113 (round 14: intl_wide_type / intl_wide_paste / intl_altgr_ps / soak in the PowerShell pane): a .NET static call and a range are
                  # PowerShell syntax; no rule knew them, the model decided, and in agent mode they went to the agent
                  "[math]::Sqrt(2)", "[IO.File]::Exists('log.txt')", "1..3"]


def ps_routing_table(mode, nld):
    """pure: [(line, kind, want)] for the PowerShell pane. kind 'shell' -> shell; 'prose' -> agent when autodetect applies (agent mode always, terminal
    mode unless nld_in_terminal_enabled is false), else shell; 'ambiguous' -> decided per line from the surface and rule the app logs
    (cases_caseagent.ambiguous_want); 'typo' / 'typo_prose' (#1980) -> judged only when typed in a conversation (cases_caseagent.typo_verdict)."""
    prose_to_agent = True if mode == "agent" else (nld is not False)
    rows = [(l, "shell", "shell") for l in PS_SHELL_LINES]
    rows += [(l, "prose", "agent" if prose_to_agent else "shell") for l in CA.PROSE_LINES]
    rows += [(l, "ambiguous", None) for l in CA.AMBIGUOUS_LINES]
    if mode == "agent":
        rows += [(l, "typo", None) for l in CA.TYPO_LINES] + [(l, "typo_prose", None) for l in CA.TYPO_PROSE_LINES]
    return rows


def ps_row_verdict(kind, want, dest, surface, rule):
    """pure: (ok or None, note) for one typed line. None = no verdict (an ambiguous line with no routing line logged; a #1980 line typed outside a
    conversation). dest is cases_caseagent.classify_dest's word."""
    if kind in ("typo", "typo_prose"):
        v = CA.typo_verdict("typo" if kind == "typo" else "prose", dest, surface, rule)
        return (None, "not typed in a conversation (surface %s)" % surface) if v is None else v
    if want is None:
        return None, "no routing line was logged, so there is no rule to check"
    return dest == want, None


def routing_ps_skip_reason(pane):
    """pure: why the PowerShell routing table is not judged in another Windows pane (r15 T46 runs it in every pane-shell pass and says what is missing)"""
    if pane == "cmd":
        return ("cmd.exe pane: there is no routing table to judge. NEEDS: cmd.exe as a pane shell in the app (#2084: the app does not start it; owner decision) "
                "and cmd.exe rows in the routing corpus (crates/input_classifier/src/routing_corpus.rs has bash and PowerShell rows only)")
    if pane == "wsl":
        return ("WSL pane: the shell is bash, whose table is routing_table, judged from rec.log. NEEDS: the harness's history hook in the distribution's "
                "~/.bashrc (not built: the pane's home is inside the distribution), which needs WSL on the Windows VM first (owner: machine change)")
    return "runs in the PowerShell pane (routing_table judges the Git Bash pane from rec.log)"


@case("routing_table_powershell", order=48, gui=True)
def c_routing_table_powershell(ctx, r):
    """T5: the autodetect routing table in the Windows PowerShell pane (routing_table needs rec.log, so it runs in the Git Bash pane only). Each line
    is typed at 10-25 ms per key with an instant Enter; where it went is proven: shell = PSReadLine's history file holds the exact line; agent =
    the history does not and the app logged the Enter as routed to the agent (`enter: Agent` / `hop=enter_begin input_type=AI`). Commands, cmdlets,
    `$variable` lines and shell structure go to the shell; prose goes to the agent; installed-head English and mistyped commands by their rules."""
    if not ctx.win:
        r.skip("Windows only: the PowerShell pane (routing_table judges the POSIX panes from rec.log)")
    if ctx.pane != "powershell":
        r.skip(routing_ps_skip_reason(ctx.pane))
    need_app_log(ctx, r, "the routing decision")
    for name, text in (("foo.txt", "foo\nbar\n"), ("log.txt", "ok\nerror here\n")):
        with open(os.path.join(ctx.W, name), "w") as f:
            f.write(text)
    # the record this case rests on must exist: one known line, by effect AND in the history file
    ctx.reset()
    hist = Received(ctx)
    probe_tok = ctx.tok(8).lower()
    probe = ctx.append_cmd("pwsh", probe_tok, "rt_ps.txt")
    ctx.rm("rt_ps.txt")
    typed(ctx, probe)
    ran = bool(ctx.until(lambda: probe_tok in ctx.local_lines("rt_ps.txt"), 10))
    logged = bool(hist.wait(lambda ls: probe in ls, 6)) and probe in hist.lines()
    if not r.check(ran, "the probe command did not run in the PowerShell pane within 10 s: nothing below can be judged"):
        return
    if not logged:
        r.unverifiable("PSReadLine's history file (%s) did not record a command that ran: where a typed line went cannot be proven in this pane; "
                       "nothing was judged" % ("exists" if hist.recorded() else "does not exist"))
        return
    nld = CA.setting_bool(ctx, "nld_in_terminal_enabled")
    r.metric("settings_nld_in_terminal", nld)
    results, judged = [], 0
    for line, kind, want in ps_routing_table(ctx.mode, nld):
        ctx.reset()
        off, row = ctx.log_size(), Received(ctx)
        typed(ctx, line)
        expect_shell = want == "shell" or kind == "typo"
        if not row.wait(lambda ls: line in ls, 3.0 if expect_shell else 1.2):
            time.sleep(0.6)
        excerpt = ctx.log_since(off)
        rows = CA.routing_table_rows(ctx, off)
        enter = rows[-1] if rows else None
        if enter and enter["route"] == "Shell" and line not in row.lines():
            row.wait(lambda ls: line in ls, 3.0)
        got = row.lines()
        reached = line in got
        any_rows = CA.typo_rows(excerpt, line) if kind in ("typo", "typo_prose") else None
        dest = CA.classify_dest(reached, ([x for x in any_rows if x["stage"] == "enter"] or any_rows) if any_rows is not None else rows, CA.enter_begin_types(excerpt))
        last = (any_rows[-1] if any_rows else None) if any_rows is not None else enter
        surface, rule = (last["surface"] if last else None), (last["rule"] if last else None)
        if kind == "ambiguous":
            want = CA.ambiguous_want(line, surface, rule)
        gave_up = CA.verdict_give_ups(excerpt) > 0
        ok, note = ps_row_verdict(kind, want, dest, surface, rule)
        results.append(dict(line=line, kind=kind, want=want, got=dest, surface=surface, rule=rule, verdict_give_up=gave_up))
        extra = [l for l in got if l != line]
        if extra:
            r.check(False, "%r: the shell also received %r (text that is not the typed line)" % (line, extra[:2]))
        if gave_up and dest in ("lost", "unproven"):
            r.check(False, "%r (%s) was not sent: its shell-or-chat verdict missed the budget and the line stayed in the editor (#1593; surface %s)" % (line, kind, surface))
        elif dest in ("lost", "unproven") and not (dest == "unproven" and want == "shell" and reached):
            r.check(False, "%r (%s) was LOST or unproven: PSReadLine's history does not hold it and the app log shows no agent-routed Enter (dest %s, surface %s)" % (line, kind, dest, surface))
        elif ok is None:
            r.note("%r (%s): no verdict, went to %s: %s" % (line, kind, dest, note))
        else:
            judged += 1
            r.check(ok, "%r (%s) expected %s, went to %s (surface %s, rule %s); PSReadLine's history got %r" % (
                line, kind, want or ("shell" if kind == "typo" else "agent"), dest, surface, rule, got[:2]))
            if note:
                r.note("%r %s" % (line, note))
        if dest == "agent":
            ctx.be.key("ctrl+c")                      # do not leave a model request running
            time.sleep(0.4)
    ctx.shot("routing_table_powershell_end")
    r.metric("routing_rows", results)
    r.metric("routing_rows_judged", judged)
    r.metric("misroutes", len([x for x in results if x["want"] and x["want"] != x["got"]]))
    ctx.reset()
    r.check(judged >= len(PS_SHELL_LINES), "only %d line(s) got a verdict, fewer than the %d shell lines of the table" % (judged, len(PS_SHELL_LINES)))
