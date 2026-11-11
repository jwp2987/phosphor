# appci/cases_wrap_vtabs.py: copy of soft-wrapped lines (#1976) and the vertical tabs panel, the owner's layout (#1990).
# Design note: docs/design/r10-harness.md. All four cases are in cases.PROVING (owner rule 2026-11-09: a new case runs in the fix tier).
#   ui_copy_wrapped_output        keyboard only (Linux, macOS, Windows Git Bash pane): `cat` a file with a line 2.5 pane-widths long, a short line, a line of
#                                 EXACTLY the pane width, one of width + 1 and a last line; select the block (block-up key) and copy it. The clipboard holds the
#                                 file's lines exactly: no line break at a soft wrap, none lost after the exactly-full line. The width is the shell's `stty size`.
#   ui_copy_wrapped_drag          pointer (X11; Wayland with the virtual pointer; Windows Git Bash pane by SendInput, positions from the app's find hook
#                                 only, r15 wave 4): a drag from a word on the first visual row of the long line to a word on the
#                                 second gives the text between them with no line break; a triple click on a word gives the whole logical line.
#   ui_vertical_tabs_many         vertical tabs on (from the profile, else switched on live and restored), APPCI_VT_TABS (8) tabs, one exact command in each; every
#                                 tab reached by its number key in a shuffled order, then next / previous / last tab, each proven by that tab's own `pwd`; the tab
#                                 count from the app's `hop=tab_activate`; exact typing with all tabs open and after they are closed.
#   ui_vertical_tabs_panel_click  4 tabs, each in a directory named by an OCR-safe token and with a last command holding it. With tab 1 shown, the other tabs'
#                                 tokens are on screen only in the panel: they are stacked top to bottom in tab order, and a click on a row puts the next typed
#                                 command in that tab's shell. The panel's own settings (view mode, primary info) stay as the profile has them.
# Windows (r15 wave 4, #2060; r14-test-gaps T31, T33): the two vertical-tabs cases run in the Git Bash pane. The panel is switched on through the file
# the app reads there (cases_ui.settings_path); the panel rows of ui_vertical_tabs_panel_click are located from the app's `appci_find` lines
# (hook_panel_rows), not by OCR, and clicked with SendInput.
# Same rules as cases.py: the EXACT text (clipboard, effect file, rec.log), typing at 10-25 ms/key with instant Enter, a case that cannot run SKIPS with the reason.
import os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases as C  # noqa: E402  (cases.py imports this module near its end, so everything below is already defined)
import cases_robust as R  # noqa: E402
import cases_ui as U  # noqa: E402
import toml_text as T  # noqa: E402
from cases import case  # noqa: E402

SENTINEL = "SENTINEL_NOT_COPIED"
TAB_HOP_RE = re.compile(r"hop=tab_activate index=(\d+) count=(\d+)")


def KX(ctx, name):
    """key spec of an app action on this platform (app/src/util/bindings.rs): SelectBlockAbove = cmdorctrl-up, CopyBlockOutput = cmdorctrl-alt-shift-C,
    FocusInput = cmd_or_ctrl_shift("l") (it also clears the selection), ActivateNextTab / ActivatePreviousTab"""
    m = U.mac(ctx)
    return {"block_up": os.environ.get("APPCI_KEY_BLOCK_UP", "cmd+up" if m else "ctrl+up"),
            "copy_output": "cmd+alt+shift+c" if m else "ctrl+alt+shift+c",
            "focus_input": "cmd+l" if m else "ctrl+shift+l",
            "next_tab": "cmd+shift+]" if m else "ctrl+Next",
            "prev_tab": "cmd+shift+[" if m else "ctrl+Prior"}[name]


# ---------------------------------------------------------------- pure helpers (unit-tested in selftest.py)
def wrap_fixture(cols, rng, tok):
    """-> (lines, spans) for a pane `cols` wide. lines[0] is distinct 7-glyph OCR-safe words joined by single spaces, at least 2.5 x cols long (so it
    covers three visual rows and wraps twice); then a short line, a line of EXACTLY cols characters (it fills the row and does not wrap), one of cols + 1
    (wraps by one cell) and a last line. spans = [(word, start, end)] of the words of lines[0] (end exclusive)."""
    words, seen, n = [], set(), 0
    while n < int(cols * 2.5) + 1:
        w = "".join(rng.choice(U.OCR_SAFE) for _ in range(7))
        if w in seen:
            continue
        seen.add(w)
        words.append(w)
        n += len(w) + 1
    long_line = " ".join(words)
    spans, pos = [], 0
    for w in words:
        spans.append((w, pos, pos + len(w)))
        pos += len(w) + 1
    digits = ("0123456789" * (cols // 10 + 2))
    return [long_line, "short %s" % tok, digits[:cols], digits[:cols] + "Z", "end %s" % tok], spans


def pick_drag_words(spans, cols):
    """-> (a, b): a word wholly on the first visual row and one wholly on the second, each the nearest to the middle of its row (so neither touches the
    pane edge), or None when a row holds no whole word (a pane narrower than a word)."""
    def mid(row):
        inside = [s for s in spans if s[1] >= row * cols and s[2] <= (row + 1) * cols]
        return min(inside, key=lambda s: abs((s[1] + s[2]) / 2.0 - (row + 0.5) * cols)) if inside else None
    a, b = mid(0), mid(1)
    return (a, b) if a and b else None


def norm_clip(got):
    """clipboard bytes / text as the cases compare it: text, LF line ends"""
    if isinstance(got, bytes):
        got = got.decode(errors="replace")
    return (got or "").replace("\r\n", "\n")


def judge_wrapped_copy(got, lines):
    """None when `got` (norm_clip text, trailing line ends ignored) is exactly `lines`; else what is wrong, naming the two defects #1976 is about: a line
    break put in at a soft wrap, and a line break lost (the exactly-full line joined to the next)."""
    want = "\n".join(lines)
    g = got.rstrip("\n")
    if g == want:
        return None
    gl = g.split("\n")
    if g.replace("\n", "") == want.replace("\n", ""):
        i = next((k for k, (x, y) in enumerate(zip(gl + [None], lines + [None])) if x != y), 0)
        if len(gl) > len(lines):
            return ("a line break was INSERTED at a soft wrap: %d lines on the clipboard for %d printed; clipboard line %d is %d chars (%r...), the printed line is %d chars"
                    % (len(gl), len(lines), i + 1, len(gl[i] or ""), (gl[i] or "")[:24], len(lines[i]) if i < len(lines) else 0))
        if len(gl) < len(lines):
            return ("a line break was LOST: %d lines on the clipboard for %d printed; printed line %d (%d chars, %r...) is joined to the next"
                    % (len(gl), len(lines), i + 1, len(lines[i]), lines[i][:24]))
        return "the line breaks moved: clipboard line %d is %d chars, the printed line is %d chars" % (i + 1, len(gl[i] or ""), len(lines[i]))
    d = next((k for k, (x, y) in enumerate(zip(g, want)) if x != y), min(len(g), len(want)))
    return "the text differs from what was printed (%d chars on the clipboard, %d printed); first difference at char %d: clipboard %r, printed %r" % (
        len(g), len(want), d, g[d:d + 24], want[d:d + 24])


def judge_drag_copy(got, line, a0, b1, slack=3):
    """None when `got` (norm_clip text) is the part of `line` a drag from offset a0 to b1 selects, give or take `slack` characters at either end (the pointer
    lands on a cell, not on a glyph edge) and never a line break inside; else what is wrong"""
    core = got.strip("\n")
    if "\n" in core:
        k = core.index("\n")
        return "a line break was INSERTED at the soft wrap: the clipboard has one after %r (the dragged text is one printed line)" % core[max(0, k - 16):k]
    core = core.strip(" ")
    want = line[a0:b1]
    inner = line[a0 + slack:b1 - slack]
    if not core or core not in line:
        return "the clipboard %r... is not a piece of the printed line (characters changed, lost or added, or nothing of it copied)" % core[:48]
    if not inner or inner not in core:
        return "the clipboard %r... does not hold the dragged text %r...%r" % (core[:32], want[:16], want[-16:])
    if abs(len(core) - len(want)) > 2 * slack:
        return "the clipboard has %d chars, the dragged text %d (more than %d off)" % (len(core), len(want), 2 * slack)
    return None


def vtabs_state(text):
    """'on' | 'off' | 'unset' | 'unknown' for appearance.vertical_tabs.enabled in settings.toml text. The [appearance.vertical_tabs] table form is the one
    toml_text can edit; an inline table or a dotted key is read here but never edited ('unknown' when it cannot be read either)."""
    v = T.get_key(text, "appearance.vertical_tabs", "enabled")
    if v is not None:
        return "on" if re.search(r"=\s*true\b", v) else "off"
    m = (re.search(r"^\s*vertical_tabs\.enabled\s*=\s*(true|false)\b", text, re.M)
         or re.search(r"^\s*vertical_tabs\s*=\s*\{[^}]*?\benabled\s*=\s*(true|false)\b", text, re.M | re.S)
         or re.search(r"^\s*appearance\.vertical_tabs\.enabled\s*=\s*(true|false)\b", text, re.M))
    if m:
        return "on" if m.group(1) == "true" else "off"
    return "unknown" if re.search(r"\bvertical_tabs\b", text) else "unset"


def vtabs_editable(text):
    """can toml_text switch the panel on in this file without writing a second definition of the table? Yes when the key is in the
    [appearance.vertical_tabs] table form, or when the file does not mention vertical_tabs at all (the table is then appended)."""
    return T.get_key(text, "appearance.vertical_tabs", "enabled") is not None or not re.search(r"\bvertical_tabs\b", text)


def rows_stacked(rows, min_step=6):
    """rows = [(tab number, cx, cy)] of the panel rows found, any order. -> (ok, message). Stacked means: top to bottom in tab order (each at least
    `min_step` px below the one before) and spread more down the screen than across it (a horizontal tab bar is the opposite: one y, growing x)."""
    rows = sorted(rows)
    if len(rows) < 2:
        return None, "fewer than two rows found"
    ys, xs = [r[2] for r in rows], [r[1] for r in rows]
    desc = ", ".join("tab %d at (%d, %d)" % r for r in rows)
    if any(b - a < min_step for a, b in zip(ys, ys[1:])):
        return False, "the rows are not top to bottom in tab order: %s" % desc
    if max(xs) - min(xs) >= max(ys) - min(ys):
        return False, "the tabs are spread across the screen, not down it (a horizontal tab bar?): %s" % desc
    return True, desc


def hook_panel_rows(sightings, band=3):
    """sightings = {tab number: [(cx, cy), ...]}: every place the app's find hook reported a tab's token, OLDEST FIRST (the token is drawn in the panel
    row and, while that tab is shown, in its pane). -> [(tab number, cx, cy)] with the panel's sighting of each tab that has one: the panel is the
    leftmost column any token was drawn in, so of a tab's sightings within `band` px of that tab's smallest x the NEWEST is taken (a row moves when
    the rows above it change height; the hook logs every new position). Tabs never sighted are left out; the caller judges how many it needs and
    whether they are a stacked column (rows_stacked)."""
    rows = []
    for k in sorted(sightings):
        pts = sightings[k]
        if not pts:
            continue
        x0 = min(p[0] for p in pts)
        near = [p for p in pts if p[0] - x0 <= band]
        rows.append((k, near[-1][0], near[-1][1]))
    return rows


def tab_counts(text):
    """[(index, count)] of every `hop=tab_activate` line of `text`: the tab a key asked for and how many tabs the window had"""
    return [(int(a), int(b)) for a, b in TAB_HOP_RE.findall(text)]


# ---------------------------------------------------------------- shared steps
def clip_after(ctx, key, tag):
    """the clipboard after `key`, as text; the clipboard is set to a sentinel first so a copy that did nothing cannot pass"""
    ctx.be.clip_set(SENTINEL)
    ctx.be.key(key)
    time.sleep(0.6)
    got = norm_clip(ctx.be.clip_get())
    ctx.shot(tag)
    return got


def back_to_input(ctx, r):
    """clear the block / text selection and focus the input (FocusInput), and re-enter the agent view when the keys of this case left it (as agent_code_recycle)"""
    try:
        ctx.be.key(KX(ctx, "focus_input"))
        time.sleep(0.4)
        if ctx.mode == "agent" and ctx.log_view_mode() == "terminal":
            ctx.be.type("/agent", 12, enter=True)
            ok = ctx.until(lambda: ctx.log_view_mode() == "agent", 10)
            r.note("the case's own keys left the agent view; it re-entered with /agent (%s)" % ("agent view again" if ok else "NOT proven: the runner will relaunch"))
    except Exception:
        pass


def print_wrap_fixture(ctx, r, tag, register=False):
    """write the fixture file for THIS pane's width and `cat` it -> (lines, spans, cols, words to drag or None, log offset before the cat) or None when
    the width could not be read or the command did not reach the shell. The width is the shell's own `stty size` (the pty's columns = the block grid's)."""
    size_f, fix_f = "vw_size_%s.txt" % tag, "vw_%s.txt" % tag
    ctx.rm(size_f, fix_f)
    U.run_cmd(ctx, "stty size > %s" % size_f)
    ctx.until(lambda: ctx.lines(size_f), 6)
    m = re.match(r"\s*(\d+)\s+(\d+)\s*$", (ctx.lines(size_f) or [""])[0])
    cols = int(m.group(2)) if m else 0
    r.metric("pane_cols", cols)
    if not r.check(cols >= 40, "could not read a usable pane width from `stty size` (%r): the wrapped line cannot be sized" % (ctx.lines(size_f) or ["<no file>"])[0]):
        return None
    lines, spans = wrap_fixture(cols, ctx.rng, ctx.tok(6).lower())
    with open(os.path.join(ctx.W, fix_f), "w", newline="\n") as f:
        f.write("\n".join(lines) + "\n")
    pick = pick_drag_words(spans, cols)
    if register and pick:
        U.register_find(ctx, pick[0][0], pick[1][0])
    seen = ctx.log_size()
    mark = ctx.cmd_mark()
    ran = U.run_cmd(ctx, "cat %s" % fix_f)
    if ctx.prof.commands() and not r.check(ran and ctx.cmds_since(mark)[-1:] == ["cat %s" % fix_f],
                                           "`cat %s` did not reach the shell as typed (received %r): nothing was printed to copy" % (fix_f, ctx.cmds_since(mark))):
        return None
    time.sleep(1.0)                                   # the output is drawn and the next prompt is up
    return lines, spans, cols, pick, seen


@case("ui_copy_wrapped_output", order=70, gui=True)
def c_copy_wrapped_output(ctx, r):
    """#1976: the output of a command with soft-wrapped lines, copied by keyboard (block-up selects the block, then copy and copy-output): the clipboard
    holds the printed lines exactly. A line break at a soft wrap, or a lost one after a line that exactly fills the pane, fails with which it was."""
    fx = print_wrap_fixture(ctx, r, "out")
    if not fx:
        return
    lines, _spans, cols, _pick, _seen = fx
    try:
        ctx.be.key(KX(ctx, "block_up"))
        time.sleep(0.6)
        # copy = the selected block's command and output (the key agent_code_recycle already proves); the printed text starts at the long line
        both = clip_after(ctx, U.K(ctx, "copy"), "wrap_copy_block")
        out = clip_after(ctx, KX(ctx, "copy_output"), "wrap_copy_output")
        r.metric("clipboard_chars", dict(block=len(both), output=len(out), printed=len("\n".join(lines))))
        head = lines[0][:16]
        if both == SENTINEL and out == SENTINEL:
            r.unverifiable("neither the copy key nor the copy-output key changed the clipboard after the block-up key (%s): the printed block was not "
                           "selected, so the copy of wrapped lines is NOT checked (shots/*wrap_copy_block.png)" % KX(ctx, "block_up"))
            return
        if both != SENTINEL:
            if r.check(head in both, "copy of the selected block does not hold the printed text (clipboard %r...): another block was selected" % both[:60]):
                err = judge_wrapped_copy(both[both.index(head):], lines)
                r.check(err is None, "copy of the block (command and output), pane %d columns: %s" % (cols, err))
        else:
            r.check(False, "the copy key left the clipboard unchanged although copy-output copied the selected block")
        if out != SENTINEL:
            err = judge_wrapped_copy(out, lines)
            r.check(err is None, "copy-output of the block, pane %d columns: %s" % (cols, err))
        else:
            r.check(False, "the copy-output key (%s) left the clipboard unchanged although the block was selected (the copy key copied it)" % KX(ctx, "copy_output"))
    finally:
        back_to_input(ctx, r)
    U.exact_burst(ctx, r, "after_wrap_copy", 4)


@case("ui_copy_wrapped_drag", order=70, gui=True)
def c_copy_wrapped_drag(ctx, r):
    """#1976 with the pointer: a drag from a word on the first visual row of a soft-wrapped line to a word on the second, then copy: the text between the
    two words, no line break. A triple click on a word, then copy: the whole printed line. Positions come from the app's find hook, else OCR."""
    m = U.mouse_or_skip(ctx, r, mac=True)
    U.need_tesseract(r, ctx)
    fx = print_wrap_fixture(ctx, r, "drag", register=True)
    if not fx:
        return
    lines, _spans, cols, pick, seen = fx
    if not r.check(pick is not None, "pane of %d columns holds no whole word on each of the first two rows" % cols):
        return
    (wa, a0, _a1), (wb, _b0, b1) = pick
    try:
        a, _ = U.find_token(ctx, "wrap_find_a", wa, since=seen, r=r)
        b, _ = U.find_token(ctx, "wrap_find_b", wb, since=seen, r=r)
        if not (a and b):
            U.wl_check(ctx, r, False, "the words to drag between (%r, %r) were not found on screen (shots/*wrap_find_*.png): cannot place the pointer" % (wa, wb), "OCR")
            return
        r.metric("drag_from_to", [list(a[1:3]), list(b[1:3])])
        if not r.check(b[2] > a[2] + 4, "the second word is not on a lower row than the first (y %d vs %d): the long line did not wrap where `stty size` "
                                         "(%d columns) says it should, so the drag would not cross a soft wrap" % (b[2], a[2], cols)):
            return
        ptr = lambda g: U.on_wayland(ctx) and not U.wl_pointer_real(ctx) and g == SENTINEL      # noqa: E731  (as ui_mouse_select_copy)
        ctx.be.clip_set(SENTINEL)
        m.drag(a[3] + 1, a[2], b[4] - 1, b[2])
        ctx.be.key(U.K(ctx, "copy"))
        time.sleep(0.5)
        got = norm_clip(ctx.be.clip_get())
        ctx.shot("wrap_drag")
        err = "nothing was copied (the clipboard kept its sentinel)" if got == SENTINEL else judge_drag_copy(got, lines[0], a0, b1)
        msg = "drag across a soft wrap + copy, pane %d columns: %s" % (cols, err)
        if ptr(got):
            U.wl_check(ctx, r, False, msg, "pointer")
        else:
            r.check(err is None, msg)
        ctx.be.clip_set(SENTINEL)
        m.click(a[1], a[2], 1, 3)
        ctx.be.key(U.K(ctx, "copy"))
        time.sleep(0.5)
        got = norm_clip(ctx.be.clip_get())
        ctx.shot("wrap_triple")
        if got == SENTINEL:
            err = "nothing was copied (the clipboard kept its sentinel)"
        elif "\n" in got.strip("\n"):
            err = "a line break was INSERTED at a soft wrap: %d lines on the clipboard for one printed line" % len(got.strip("\n").split("\n"))
        elif got.strip("\n").rstrip(" ") != lines[0]:
            err = "clipboard %d chars (%r...%r), the printed line is %d chars" % (len(got.strip("\n")), got[:16], got.strip("\n")[-16:], len(lines[0]))
        else:
            err = None
        msg = "triple click on a word of a soft-wrapped line + copy: %s (expected the whole printed line)" % err
        if ptr(got):
            U.wl_check(ctx, r, False, msg, "pointer")
        else:
            r.check(err is None, msg)
    finally:
        back_to_input(ctx, r)
    U.exact_burst(ctx, r, "after_wrap_drag", 4)


# ---------------------------------------------------------------- vertical tabs (#1990)
class VTabs:
    """the vertical tabs panel switched on for one case: taken as it is when the profile has it on, else written to the settings file (the live reload
    ui_settings_live proves) and the original bytes restored by restore(). A settings layout that cannot be edited safely SKIPS before anything is typed."""

    def __init__(self, ctx, r):
        self.ctx, self.orig, self.how = ctx, None, None
        p = U.settings_path(ctx)
        if not os.path.isfile(p):
            r.skip("settings file %s not found" % p)
        raw = open(p, "rb").read()
        text = raw.decode("utf-8", errors="replace")
        st = vtabs_state(text)
        if st == "on":
            self.how = "on in the profile's settings (the owner's layout)"
            return
        if not vtabs_editable(text):
            r.skip("appearance.vertical_tabs.enabled is %s in the profile's settings and is written in a form toml_text cannot edit (an inline table or a "
                   "dotted key): the panel cannot be switched on for this case without rewriting the owner's file" % st)
        before = ctx.shot("vt_off")
        self.orig = raw
        U.set_setting(ctx, "appearance.vertical_tabs", "enabled", "true")
        after, d = U.settle_change(ctx, before, "vt_on", 0.004)
        r.metric("vtabs_on_screen_diff", d if d is not None else -1)
        self.how = "switched on live for this case (it was %s)" % st
        if not r.check(after is not None, "appearance.vertical_tabs.enabled = true: the screen did not change within 10 s of the file write (diff %s): the panel did not appear" % d):
            self.restore()
            raise C.Skip("the vertical tabs panel could not be switched on (see the failed check of ui_settings_live for the live reload)")

    def restore(self):
        if self.orig is not None:
            with open(U.settings_path(self.ctx), "wb") as f:
                f.write(self.orig)
            self.orig = None
            time.sleep(1.5)


def vt_tokens(ctx, n):
    """n distinct OCR-safe tokens (the directory names and the words looked for in the panel)"""
    out = []
    while len(out) < n:
        t = "vt" + U.ocr_safe(ctx.tok(5).lower().replace("_", ""))
        if t not in out:
            out.append(t)
    return out


def tab_answer(ctx, name):
    """type the pwd probe into whichever tab has the keyboard -> that tab's cwd (norm_path), or '<none>' when no shell answered"""
    f = "vt_pwd_%s.txt" % name
    ctx.rm(f)
    mark = ctx.cmd_mark()
    ctx.be.type("%s >> %s/%s" % (R.PWD(ctx), R.fwd(ctx.W), f), ctx.rng.randint(10, 25), enter=True)
    ctx.until(lambda: ctx.lines(f), 8)
    got = (ctx.lines(f) or ["<none>"])[0]
    if got != "<none>":
        # Round 10 (Linux Wayland, agent pass; #2027): the probe's effect file exists BEFORE the shell logs the command (the rc hook writes the C line
        # with the next prompt). The exact burst right after the last probe took its command mark in that gap, and the probe was counted as the first
        # of its typed lines ("shell received 5 commands, not the typed ones (first mismatch: 'pwd >> .../vt_pwd_end.txt')") although the four lines
        # arrived exactly. The probe is over only when the shell has logged it, so whatever marks next starts after it. A give-up net, as everywhere
        # in await_commands: a probe that is never logged shows up in the next comparison, as before.
        ctx.await_commands(mark, probe_logged(f))
    return R.norm_path(got) if got != "<none>" else got


def probe_logged(fname):
    """pure: a predicate over the commands logged since a mark: the pwd probe that writes `fname` is among them"""
    return lambda cmds: any(fname in c for c in cmds)


def open_tabs(ctx, r, n, tag, register=False):
    """tab 1 is the pane the case started in (its cwd stays the work dir); tabs 2..n are opened with the new-tab key (in the agent pass each is put in
    agent view when it opened as a terminal session), each moved to its own directory <W>/<token>. One short command holding the tab's token runs in every tab (so the panel row shows it, and the effect file proves where each line
    ran) -> (tokens, dirs, effect file name). Every line is checked exactly: effect file and rec.log. register=True gives the tokens to the app's
    find hook BEFORE anything holds them (the hook attaches a hit when text is laid out: a title laid out earlier never reports)."""
    toks = vt_tokens(ctx, n)
    if register:
        U.register_find(ctx, *toks)
    eff = "vt_%s.txt" % tag
    ctx.rm(eff)
    dirs, expect = [R.norm_path(ctx.W)], []
    mark = ctx.cmd_mark()
    for i, tok in enumerate(toks):
        if i:
            d = R.fwd(os.path.join(ctx.W, tok))
            os.makedirs(d, exist_ok=True)
            dirs.append(R.norm_path(d))
            prompts0 = ctx.prof.prompts()
            ctx.be.guard()
            ctx.be.key(R.NEWTAB)
            time.sleep(0.8)
            if not ctx.until(lambda: ctx.prof.prompts() > prompts0, 10, step=0.1):
                r.note("tab %d: no shell prompt within 10 s of the new-tab key; its commands are typed anyway" % (i + 1))
            if ctx.mode == "agent" and ctx.log_view_mode() == "terminal":
                # the agent pass is the owner's setup: every tab in agent view (as agent_multitab_streaming does). It also keeps the app's LATEST
                # hop=view_mode line 'agent', which is what the runner reads after the case to tell whether the pane left agent mode.
                ctx.be.type("/agent", 12, enter=True)
                if not ctx.until(lambda: ctx.log_view_mode() == "agent", 10):
                    r.note("tab %d opened as a terminal session and /agent did not show the agent view within 10 s" % (i + 1))
                time.sleep(0.6)
            cd = "cd %s" % d
            expect.append(cd)
            ctx.be.type(cd, ctx.rng.randint(10, 25), enter=True)
            ctx.until(lambda: cd in ctx.cmds_since(mark), 6)
        line = "echo %s >> %s%s" % (tok, "../" if i else "", eff)
        expect.append(line)
        ctx.be.type(line, ctx.rng.randint(10, 25), enter=True)
        ctx.until(lambda: len(ctx.lines(eff)) >= i + 1, 10)
        time.sleep(0.3)
    r.check(ctx.lines(eff) == toks, "one command per tab: the effect file has %r, expected %r (a line ran in the wrong tab's directory, twice, or not at all)" % (ctx.lines(eff), toks))
    if ctx.prof.commands():
        have = ctx.await_commands(mark, len(expect))
        r.check(have == expect, "the %d tabs' shells received %r, expected %r" % (n, have, expect))
    return toks, dirs, eff


def window_tab_count(ctx, off):
    """how many tabs the window had at the latest tab key since `off` (hop=tab_activate), or None when the app logged none"""
    hops = tab_counts(ctx.log_since(off))
    return hops[-1][1] if hops else None


def close_extra_tabs(ctx, r, n):
    """close tabs n..2 (last-tab key, then the close key), leaving tab 1. The tab count is read from the app log before every close, so the LAST tab
    is never closed (that would close the window); without the log line exactly n - 1 closes are sent."""
    for _ in range(n - 1):
        off = ctx.log_size()
        ctx.be.guard()
        ctx.be.key(R.tab_key(ctx, 9))
        time.sleep(0.6)
        cnt = window_tab_count(ctx, off)
        if cnt is not None and cnt <= 1:
            break
        R.close_tab_best_effort(ctx, r)
        time.sleep(0.9)
    off = ctx.log_size()
    ctx.be.guard()
    ctx.be.key(R.tab_key(ctx, 9))
    time.sleep(0.6)
    left = window_tab_count(ctx, off)
    r.metric("tabs_left", left if left is not None else -1)
    msg = "%s tab(s) are open after the case closed its %d extra tabs" % (left, n - 1)
    if left is not None and left != 1:
        if R.CLOSE_PANE:
            r.check(False, msg)
        else:
            r.note(msg + " (no close key on this platform; the tabs were ended with Ctrl-D)")


def vt_need(ctx, r):
    """nothing is missing on any platform since r15 wave 4 (#2060): on Windows the settings file the app reads is cases_ui.settings_path's
    (WindowsApp.config), so VTabs switches the panel on live there as on Linux and macOS. Kept as the one place a platform need would be stated."""
    return


def first_tab_precondition(got, want):
    """pure: None when the pane that answered the first `pwd` is the case's own first tab (cwd = the work dir), else the failure text"""
    if got == want:
        return None
    return ("precondition: before the case opened any tab, the pane that has the keyboard %s, expected the pane's shell in %r: an earlier case left "
            "another tab or a non-shell pane in front; nothing else was typed (every line of this case would have gone to the wrong place)"
            % ("answered no command ('<none>': no shell is in front)" if got == "<none>" else "is in %r" % got, want))


def first_tab_in_front(ctx, r, tag):
    """Round 10 macOS: md_viewer had left its viewer tab in front, and this case typed its first line into the viewer, counted 9 tabs for 8 and
    reported 12 wrong answers 260 s later. The case now proves where its first key will go BEFORE it types or opens anything, and stops with that
    one failure otherwise (still a FAIL: the case did not run)."""
    bad = first_tab_precondition(tab_answer(ctx, "pre_" + tag), R.norm_path(ctx.W))
    if bad:
        r.check(False, bad)
    return bad is None


@case("ui_vertical_tabs_many", order=87, gui=True)
def c_vertical_tabs_many(ctx, r):
    """#1990, the owner's layout: vertical tabs on and many tabs. Every tab gets one exact command; every tab is reached by its number key in a shuffled
    order, then by next / previous / last tab; the tab that answers is proven by its own `pwd`. Typing stays exact with all tabs open and after they close."""
    vt_need(ctx, r)
    n = max(3, min(int(os.environ.get("APPCI_VT_TABS", "8")), 8))          # number keys reach tabs 1..8; the 9 key is "last tab"
    if not first_tab_in_front(ctx, r, "many"):
        return
    vt = VTabs(ctx, r)
    r.note("vertical tabs: %s" % vt.how)
    opened = False
    try:
        off = ctx.log_size()
        opened = True
        _toks, dirs, _eff = open_tabs(ctx, r, n, "many")
        ctx.shot("vt_many_open")
        order = list(range(n))
        ctx.rng.shuffle(order)
        wrong = []
        for i in order:
            ctx.be.guard()
            ctx.be.key(R.tab_key(ctx, i + 1))
            time.sleep(0.6)
            got = tab_answer(ctx, "k%d" % i)
            if got != dirs[i]:
                wrong.append((i + 1, got))
        r.check(not wrong, "tab number keys with %d vertical tabs: (tab, cwd that answered) %r; expected each tab's own directory" % (n, wrong))
        hops = tab_counts(ctx.log_since(off))
        if hops:
            r.metric("tab_activate_hops", len(hops))
            r.check({c for _, c in hops} == {n}, "the window had %s tab(s) at the tab keys, expected %d (an earlier case left tabs open, or a new-tab key was lost)" % (sorted({c for _, c in hops}), n))
        else:
            r.note("no hop=tab_activate line in the app log: the tab count is not checked (WARP input-order hops off, or an older build)")
        steps = [("tab 1 key", R.tab_key(ctx, 1), 0), ("next tab", KX(ctx, "next_tab"), 1), ("next tab", KX(ctx, "next_tab"), 2),
                 ("previous tab", KX(ctx, "prev_tab"), 1), ("last tab key", R.tab_key(ctx, 9), n - 1), ("tab 1 key", R.tab_key(ctx, 1), 0)]
        for k, (what, key, want) in enumerate(steps):
            ctx.be.guard()
            ctx.be.key(key)
            time.sleep(0.6)
            got = tab_answer(ctx, "s%d" % k)
            r.check(got == dirs[want], "step %d (%s): the tab that answered is in %r, expected tab %d (%r)" % (k + 1, what, got, want + 1, dirs[want]))
        ctx.shot("vt_many_tab1")
        U.exact_burst(ctx, r, "vt_many_open", 6)                           # tab 1 (cwd = the work dir) with every other tab open
        r.check(ctx.app.alive(), "the app died with %d vertical tabs open" % n)
    finally:
        try:
            if opened:
                close_extra_tabs(ctx, r, n)
        finally:
            vt.restore()
    got = tab_answer(ctx, "end")
    r.check(got == R.norm_path(ctx.W), "after the tabs were closed the pane that answers is in %r, expected the first tab (%r)" % (got, R.norm_path(ctx.W)))
    U.exact_burst(ctx, r, "vt_many_closed", 4)


def left_of_pane(rows, pane_left):
    """pure: the find-hook rows (text, cx, cy, left, right, top, bottom) that lie wholly left of the pane's own left edge: drawn in the panel.
    pane_left None (the edge is not known): every row, as before round 19."""
    return list(rows) if pane_left is None else [w for w in rows if w[4] < pane_left]


def _pane_left_edge(ctx):
    """the x of the pane's leftmost text column, from the find hook: tab 1 prints a probe at column 0 of a command's OUTPUT (the command line never
    holds it: `printf '%s%s\\n' A B`, so no panel row shows it) -> the smallest left edge it was drawn at, or None when the hook reported none.
    Round 14 (#2109, Git Bash agent pass): in the agent view the panel rows do not show a tab's token, and the token's sightings in the PANE were
    taken for panel rows (x=356; the panel is at x=149) and clicked."""
    import cases_window_use as WU                        # imported here: cases_window_use imports this module
    probe = WU.safe_token(ctx, "vtq")
    U.register_find(ctx, probe)
    half = len(probe) // 2
    off = ctx.log_size()
    U.run_cmd(ctx, "printf '%%s%%s\\n' %s %s" % (probe[:half], probe[half:]))
    if not WU.wait_hook(ctx, probe, off, 4.0):
        return None
    return min(w[3] for w in WU.hook_rows(ctx, probe, off))


def _panel_rows_from_hook(ctx, r, toks, n, since):
    """Windows: the panel rows of tabs 2..n from the app's `appci_find` lines since the tabs were opened (hook_panel_rows), only the sightings left
    of the pane's own left edge (_pane_left_edge) -> (rows, pane_left), or (None, pane_left) after reporting that they cannot be located (not
    judged: the VM has no OCR engine to fall back on)"""
    import cases_window_use as WU                        # imported here: cases_window_use imports this module
    origin = ctx.be.client_origin()
    pane_left = _pane_left_edge(ctx) if origin else None
    ctx.shot("vt_panel")
    text = ctx.log_since(since)
    found = {k + 1: WU.find_rows(text, toks[k], origin) for k in range(1, n)} if origin else {}
    sightings = {k: [(w[1], w[2]) for w in left_of_pane(ws, pane_left)] for k, ws in found.items()}
    rows = hook_panel_rows(sightings)
    if len(rows) < 2:
        in_pane = sum(1 for ws in found.values() if ws)
        r.unverifiable("the panel rows of tabs 2..%d could not be located: the app logged `appci_find` lines for %d of %d tab tokens, %d of them left of "
                       "the pane's left edge (x=%s; window origin %r; shots/*vt_panel.png). The profile's panel shows the command or the directory in "
                       "each row (checked before the case, case_settings.py: vertical_tabs_primary_info), so the rows of this session mode showed "
                       "another text, or the build has no find hook; Windows has no OCR engine to fall back on. Stacking and the row click are NOT "
                       "checked" % (n, in_pane, n - 1, len(rows), pane_left, origin))
        return None, pane_left
    r.note("panel rows located from the app's find hook (the newest sighting in each token's leftmost column, left of the pane's edge x=%s), not by OCR" % pane_left)
    return rows, pane_left


@case("ui_vertical_tabs_panel_click", order=87, gui=True)
def c_vertical_tabs_panel_click(ctx, r):
    """#1990: the panel itself. With tab 1 shown, the tokens of tabs 2..4 (each tab's directory name and last command) are on screen only in the panel:
    they are stacked top to bottom in tab order, and a click on a row puts the next typed command in that tab's shell (its own `pwd`)."""
    vt_need(ctx, r)
    m = U.mouse_or_skip(ctx, r, mac=True)
    U.need_tesseract(r, ctx)
    n = 4
    if not first_tab_in_front(ctx, r, "panel"):
        return
    vt = VTabs(ctx, r)
    r.note("vertical tabs: %s" % vt.how)
    opened = False
    try:
        opened = True
        seen = ctx.log_size()
        toks, dirs, _eff = open_tabs(ctx, r, n, "panel", register=U.is_windows(ctx))
        ctx.be.guard()
        ctx.be.key(R.tab_key(ctx, 1))
        time.sleep(1.2)
        got = tab_answer(ctx, "p0")
        if not r.check(got == dirs[0], "the tab 1 key did not show tab 1 (the pane that answers is in %r): the other tabs' tokens may be in the pane, not only in the panel" % got):
            return
        pane_left = None
        if U.is_windows(ctx):
            rows, pane_left = _panel_rows_from_hook(ctx, r, toks, n, seen)
            if rows is None:
                return
        else:
            ws, _p = U.words(ctx, "vt_panel")
            rows = []
            for k in range(1, n):
                hits = U.token_hits(ws, toks[k])
                if hits:
                    rows.append((k + 1, hits[0][1], hits[0][2]))
        r.metric("panel_rows_found", [list(x) for x in rows])
        if len(rows) < 2:
            r.no_reference("the panel rows of tabs 2..%d could not be read by OCR (found %d of %d tokens, shots/*vt_panel.png): the profile's panel shows the command or "
                           "the directory in each row (checked before the case, case_settings.py: vertical_tabs_primary_info), so the rows of this session mode showed "
                           "another text, or the text is too small for tesseract; stacking and the row click are NOT checked"
                           % (n, len(rows), n - 1))
            return
        ok, desc = rows_stacked(rows)
        if U.is_windows(ctx):
            # with the pane's edge known the rows are the panel's: not stacked is the app's. Without it (the probe was not sighted) a column
            # that is not stacked may be the pane's text: not judged. Either way nothing is clicked at places that are not rows.
            if not ok and pane_left is None:
                r.unverifiable("the tokens' leftmost sightings are not a stacked column (%s) and the pane's left edge could not be read from the "
                               "find hook, so they cannot be told from text in the pane: stacking and the row click are NOT checked" % desc)
                return
            if not r.check(bool(ok), "vertical tabs panel (positions from the app's find hook, left of the pane's edge x=%s): %s" % (pane_left, desc)):
                return
        else:
            U.wl_check(ctx, r, bool(ok), "vertical tabs panel: %s" % desc, "OCR")
        r.note("panel rows (%s): %s" % ("find hook" if U.is_windows(ctx) else "OCR", desc))
        ptr_unproven = U.on_wayland(ctx) and not U.wl_pointer_real(ctx)
        for tabno, cx, cy in sorted({rows[-1], rows[0]}, reverse=True):
            m.click(cx, cy, 1)
            time.sleep(0.9)
            got = tab_answer(ctx, "c%d" % tabno)
            msg = "click on the panel row of tab %d at (%d, %d): the pane that answers is in %r, expected %r" % (tabno, cx, cy, got, dirs[tabno - 1])
            if ptr_unproven and got != dirs[tabno - 1]:
                U.wl_check(ctx, r, False, msg, "pointer")
            else:
                r.check(got == dirs[tabno - 1], msg)
        ctx.shot("vt_panel_clicked")
        r.check(ctx.app.alive(), "the app died in the vertical tabs panel case")
    finally:
        try:
            if opened:
                close_extra_tabs(ctx, r, n)
        finally:
            vt.restore()
    got = tab_answer(ctx, "pend")
    r.check(got == R.norm_path(ctx.W), "after the tabs were closed the pane that answers is in %r, expected the first tab (%r)" % (got, R.norm_path(ctx.W)))
    U.exact_burst(ctx, r, "vt_panel_closed", 4)
