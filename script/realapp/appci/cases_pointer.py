# appci/cases_pointer.py: round 14 wave 3a (docs/design/r14-test-gaps.md 7.1, T21 to T25; design note docs/design/r10-r14-w3a.md). All five are in
# cases.PROVING (owner rule 2026-11-09: a new case runs in the fix tier until the owner graduates it).
#   link_and_path_click        Linux. A printed URL, an OSC 8 link and a `file:line` path, each hovered and Ctrl+clicked. A recording `xdg-open` first on the
#                              app's PATH holds exactly the URL / the link's target; the path reaches the recorder as the absolute path, or the file's own text
#                              at the clicked line is drawn. The whole launch runs under `strace`: no destination beyond loopback and the configured model.
#   selection_modes            Linux. Double click = the word, triple click = the line, a drag over two blocks, Shift+click = the block range (copy-output of
#                              three blocks; control: one block after the plain click), a drag while a program rewrites its last line ten times a second.
#   primary_and_middle_click   Linux. `terminal.copy_on_select` off, then on (written live, restored): PRIMARY holds a dragged and a double-clicked
#                              selection, CLIPBOARD only when the setting is on; a middle click in the input pastes PRIMARY (the received line proves it).
#   wheel_scroll               Linux. The wheel moves the block list's scrollback and comes back, moves `less`, and reaches a program that asked for SGR
#                              mouse reporting as ESC[<64 / ESC[<65, with a click as press + release and the key typed after them last.
#   pane_resize_maximize       Linux, macOS. Split; maximise and restore by key; `Move divider left` from the command palette; on Linux a drag of the
#                              divider. Every step is judged by `stty size` and the shell's pid in the pane that has the keyboard.
# Same rules as cases.py: the EXACT text (clipboard, PRIMARY, effect file, rec.log), typing at 10-25 ms/key with instant Enter, a case that cannot run SKIPS
# with the missing capability. Positions come from the app's find hook when it is on, else OCR; a screen read that finds none of a case's words is
# UNVERIFIABLE, never a pass.
import os, re, shutil, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases as C  # noqa: E402  (cases.py imports this module near its end, so everything below is already defined)
import cases_robust as R  # noqa: E402
import cases_ui as U  # noqa: E402
import cases_wrap_vtabs as WV  # noqa: E402
import toml_text as T  # noqa: E402
from cases import case  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
SENTINEL = WV.SENTINEL
SENT_PRIMARY = "SENTINEL_PRIMARY_UNSET"
WL_AXIS = 60                 # one wheel step on Wayland, in axis units (pixels): about three rows
DIVIDER_SWEEP_PX = 40        # how far left of the right pane's first column the 2 px divider is looked for
DIVIDER_DRAG_PX = 120


# ================================================================ pure helpers (unit-tested in selftest_pointer.py)
def distinct_words(rng, n, size=7, min_dist=2):
    """n OCR-safe words of `size` glyphs, any two differing in at least `min_dist` places (so a one-glyph misread never names another word)"""
    out = []
    while len(out) < n:
        w = "".join(rng.choice(U.OCR_SAFE) for _ in range(size))
        if all(sum(a != b for a, b in zip(w, o)) >= min_dist for o in out):
            out.append(w)
    return out


def word_spans(line):
    """{word: (start, end)} of the space-separated words of `line` (end exclusive)"""
    out, pos = {}, 0
    for w in line.split(" "):
        out[w] = (pos, pos + len(w))
        pos += len(w) + 1
    return out


def _near(a, b):
    """a and b differ by at most one glyph (one changed, or one dropped / added)"""
    if a == b:
        return True
    if len(a) == len(b):
        return sum(x != y for x, y in zip(a, b)) <= 1
    if abs(len(a) - len(b)) != 1:
        return False
    short, long_ = (a, b) if len(a) < len(b) else (b, a)
    return any(long_[:i] + long_[i + 1:] == short for i in range(len(long_)))


def visible_indexes(ws, words):
    """the indexes of the fixture `words` that the screen read `ws` (cases_ui.words rows) shows: an exact word, else one that is one glyph off"""
    idx = {w: i for i, w in enumerate(words)}
    out = set()
    for row in ws:
        t = re.sub(r"[^0-9a-z]", "", row[0].lower())
        if t in idx:
            out.add(idx[t])
        elif words and abs(len(t) - len(words[0])) <= 1:
            hit = next((i for w, i in idx.items() if _near(t, w)), None)
            if hit is not None:
                out.add(hit)
    return out


def scroll_verdict(before, moved, back, toward_start, min_rows=3):
    """-> (status, text). before / moved / back: the sets of fixture line indexes on screen before the wheel, after it, and after the wheel back.
    toward_start: the first wheel movement shows EARLIER lines (wheel up in scrollback); False: later lines (wheel down in a pager at its top).
    'unread' when a read found no fixture line (not judged); 'fail' when the view did not move by `min_rows` or did not come back; else 'ok'."""
    if not before or not moved or not back:
        return "unread", "a screen read found no fixture line (before %d, after the wheel %d, after the wheel back %d)" % (len(before), len(moved), len(back))
    desc = "lines %d-%d before, %d-%d after the wheel, %d-%d after the wheel back" % (min(before), max(before), min(moved), max(moved), min(back), max(back))
    if toward_start:
        if min(moved) > min(before) - min_rows:
            return "fail", "the wheel did not show earlier lines (%s)" % desc
        if max(back) < max(before) - 2:
            return "fail", "the wheel back did not return to the last lines (%s)" % desc
    else:
        if min(moved) < min(before) + min_rows:
            return "fail", "the wheel did not show later lines (%s)" % desc
        if min(back) > min(before) + 2:
            return "fail", "the wheel back did not return to the first lines (%s)" % desc
    return "ok", desc


SGR_RE = re.compile(rb"\x1b\[<(\d+);(\d+);(\d+)([Mm])")


def sgr_events(data):
    """-> ([(button, column, row, 'M' | 'm')], the bytes that are not SGR mouse reports)"""
    return [(int(b), int(c), int(r), k.decode()) for b, c, r, k in SGR_RE.findall(data)], SGR_RE.sub(b"", data)


def sgr_click_verdict(data):
    """None when `data` is exactly one left press and its release at the same cell; else what is wrong"""
    ev, rest = sgr_events(data)
    if not ev:
        return "no SGR mouse report reached the program (it received %s)" % (data.hex() or "nothing")
    if rest:
        return "bytes that are not a mouse report arrived with the click: %s" % rest.hex()
    if [e[0] for e in ev] != [0, 0] or [e[3] for e in ev] != ["M", "m"]:
        return "expected one press ESC[<0;x;yM and one release ESC[<0;x;ym, got %r" % (ev,)
    if ev[0][1:3] != ev[1][1:3] or min(ev[0][1:3]) < 1:
        return "the press and the release name different or impossible cells: %r" % (ev,)
    return None


def sgr_wheel_verdict(data, up):
    """None when `data` is one or more wheel reports of the one direction (button 64 up, 65 down) and nothing else; else what is wrong"""
    want = 64 if up else 65
    ev, rest = sgr_events(data)
    if not ev:
        return "no SGR wheel report reached the program (it received %s)" % (data.hex() or "nothing")
    if rest:
        return "bytes that are not a mouse report arrived with the wheel: %s" % rest.hex()
    bad = [e for e in ev if e[0] != want or e[3] != "M" or min(e[1:3]) < 1]
    if bad:
        return "expected only ESC[<%d;x;yM, got %r" % (want, ev)
    return None


def reporting_expected(settings_text):
    """(mouse, scroll): whether the profile lets a program that asked for it receive clicks and the wheel (terminal.mouse_reporting_enabled,
    terminal.scroll_reporting_enabled; both default true, and the wheel needs both: alt_screen::should_intercept_scroll)"""
    def on(key):
        v = T.get_key(settings_text, "terminal", key)
        return not (v and re.search(r"=\s*false\b", v))
    mouse = on("mouse_reporting_enabled")
    return mouse, mouse and on("scroll_reporting_enabled")


def judge_cross_block(got, line_a, a0, line_b, b1, slack=3):
    """None when `got` (WV.norm_clip text) is a selection dragged from offset a0 of `line_a` (one block's output) to offset b1 of `line_b` (the next
    block's output): its first line is the tail of line_a from about a0, its last line the head of line_b to about b1 (`slack` cells either way),
    with at least one line break between. What lies between (the second block's prompt and command) is the caller's to report."""
    g = got.strip("\n")
    if not g or g == SENTINEL:
        return "nothing was copied"
    ls = g.split("\n")
    if len(ls) < 2:
        return "the clipboard holds one line (%r...): the selection did not cross into the second block" % g[:40]
    first, last = ls[0].rstrip(" "), ls[-1].rstrip(" ")
    want_first, want_last = line_a[a0:], line_b[:b1]
    if not first or not line_a.endswith(first) or abs(len(first) - len(want_first)) > slack:
        return "the first line %r is not the rest of the first block's line from the drag start (%r)" % (first[:40], want_first[:40])
    if not last or not line_b.startswith(last) or abs(len(last) - len(want_last)) > slack:
        return "the last line %r is not the second block's line up to the drag end (%r)" % (last[:40], want_last[:40])
    return None


OPENER_NAMES = ("xdg-open", "gio", "gvfs-open", "gnome-open", "kde-open", "kde-open5", "kde-open6", "wslview", "exo-open", "x-www-browser", "sensible-browser")


def opener_script(log):
    """the recording stand-in for xdg-open and the `open` crate's fallbacks: one tab-separated line per call (`call`, the name it was run as, each
    argument), opens nothing, exits 0"""
    return ("#!/bin/sh\n# appci link_and_path_click: records what the app asked the desktop to open; opens nothing\n"
            "{ printf 'call\\t%%s' \"${0##*/}\"; for a in \"$@\"; do printf '\\t%%s' \"$a\"; done; printf '\\n'; } >> '%s'\nexit 0\n" % log)


def opener_calls(text):
    """[(program, [arguments])] of the recorder's log; `gio open X` is read as X"""
    out = []
    for l in text.splitlines():
        p = l.split("\t")
        if len(p) >= 2 and p[0] == "call":
            args = p[2:]
            if p[1] == "gio" and args[:1] == ["open"]:
                args = args[1:]
            out.append((p[1], args))
    return out


def url_open_verdict(new_calls, want):
    """None when the recorder got exactly one call with exactly `want`; else what is wrong"""
    if not new_calls:
        return "nothing was handed to the desktop opener"
    if len(new_calls) != 1:
        return "the opener was called %d times for one click: %r" % (len(new_calls), new_calls)
    if new_calls[0][1] != [want]:
        return "the opener got %r, expected exactly [%r]" % (new_calls[0][1], want)
    return None


def path_open_verdict(new_calls, abs_path, line_tok_seen, first_tok_seen):
    """-> (how, error). how: 'system' (the desktop opener got the path), 'external' (an editor URL holding it), 'pane' (the file is drawn in the
    app), None. A path click must do exactly one of them; an in-app open must show the clicked LINE."""
    for _prog, args in new_calls:
        if len(args) == 1 and args[0] in (abs_path, "file://" + abs_path):
            return ("system", None) if len(new_calls) == 1 else ("system", "the opener was called %d times for one click: %r" % (len(new_calls), new_calls))
    for _prog, args in new_calls:
        if len(args) == 1 and abs_path in args[0]:
            return "external", None
    if new_calls:
        return None, "the opener got %r, which does not name the clicked file %r" % (new_calls, abs_path)
    if line_tok_seen:
        return "pane", None
    if first_tok_seen:
        return "pane", "the file opened in the app at its first line; the text of the clicked line is not on screen (the :line part was not followed)"
    return None, "nothing opened: no call to the desktop opener and the file's text is not on screen"


def parse_pane_probe(lines):
    """(pid, rows, cols) from the probe file's first line `PID ROWS COLS`, or None"""
    m = re.match(r"\s*(\d+)\s+(\d+)\s+(\d+)\s*$", (lines or [""])[0])
    return (int(m.group(1)), int(m.group(2)), int(m.group(3))) if m else None


def pane_probe_cmd(name):
    """the probe line: the pid of the shell that ran it and that pane's `stty size`, into pr_<name>.txt"""
    return "sh -c 'echo $PPID $(stty size)' >> pr_%s.txt" % name


def split_verdict(a0, a1, b1, slack=6):
    """errors (possibly none) for a right split: the old pane kept its shell, the new one has another, both are narrower than the unsplit pane and
    together about as wide (`slack` columns for the divider and the padding)"""
    errs = []
    if b1[0] == a0[0]:
        errs.append("the probe typed after the split ran in the OLD pane's shell (pid %d): the new pane does not have the keyboard" % a0[0])
    if a1[0] != a0[0]:
        errs.append("after the switch-pane key the probe ran in pid %d, not the first pane's shell %d" % (a1[0], a0[0]))
    if not (a1[2] < a0[2] and b1[2] < a0[2]):
        errs.append("`stty size` did not shrink in both panes: %d columns unsplit, now %d and %d" % (a0[2], a1[2], b1[2]))
    elif abs(a1[2] + b1[2] - a0[2]) > slack:
        errs.append("the two panes are %d + %d columns for an unsplit %d (more than %d off)" % (a1[2], b1[2], a0[2], slack))
    return errs


def maximize_verdict(a0, b1, bmax, brest, slack=2):
    """errors for maximise + restore of the pane with pid b1[0]: maximised it is as wide as the unsplit pane, restored it is exactly as before"""
    errs = []
    if bmax[0] != b1[0] or brest[0] != b1[0]:
        errs.append("the probes ran in pid %d (maximised) and %d (restored), not the focused pane's shell %d" % (bmax[0], brest[0], b1[0]))
    if bmax[2] < a0[2] - slack:
        errs.append("maximised, `stty size` says %d columns; the unsplit pane had %d and the split pane %d" % (bmax[2], a0[2], b1[2]))
    if brest[1:] != b1[1:]:
        errs.append("restored, `stty size` says %dx%d, before the maximise %dx%d" % (brest[1], brest[2], b1[1], b1[2]))
    return errs


def divider_verdict(a_before, b_before, a_after, b_after, want_cols=None, slack=2):
    """errors for the divider moved LEFT between a left pane (a) and a right pane (b): b is wider, a narrower, the sum is kept (one column off
    allowed), and when `want_cols` is given the change is that many columns give or take `slack`"""
    da, db = a_after[2] - a_before[2], b_after[2] - b_before[2]
    errs = []
    if db <= 0 or da >= 0:
        errs.append("the right pane went %+d columns and the left pane %+d: expected the right one wider and the left one narrower" % (db, da))
    elif abs(da + db) > 1:
        errs.append("the right pane went %+d columns and the left pane %+d: the total changed" % (db, da))
    elif want_cols is not None and abs(db - want_cols) > slack:
        errs.append("the right pane went %+d columns for a divider moved about %.1f columns" % (db, want_cols))
    return errs


def hold_argv(backend, mod, ms):
    """-> (argv that presses `mod`, argv that releases it or None). X11: xdotool keydown / keyup. Wayland: ONE wtype that presses, sleeps `ms` and
    releases (a wtype's keyboard goes away with the process), led by the Shift_L tap every wtype call of the backend is led by (#1414)."""
    if backend == "x11":
        return ["xdotool", "keydown", mod], ["xdotool", "keyup", mod]
    return ["wtype", "-k", "Shift_L", "-s", "20", "-M", mod, "-s", str(int(ms)), "-m", mod], None


def primary_argv(backend, write):
    """the tool that writes (stdin) or prints the PRIMARY selection of this job's own display"""
    if backend == "x11":
        return ["xsel", "-p", "-i"] if write else ["xsel", "-p", "-o"]
    return ["wl-copy", "--primary"] if write else ["wl-paste", "--primary", "-n"]


def cos_profile_block(text):
    """why primary_and_middle_click cannot run on this profile's settings text, or None. PRIMARY and the middle-click paste are off by the
    profile's own choice, or terminal.copy_on_select is written in a form toml_text cannot edit without defining `terminal` twice."""
    for table, key, what in (("system", "linux_selection_clipboard", "the PRIMARY selection is not used"),
                             ("terminal.input", "middle_click_paste_enabled", "a middle click does not paste")):
        v = T.get_key(text, table, key)
        if v and re.search(r"=\s*false\b", v):
            return "the profile sets %s.%s = false (%s, by the profile's choice)" % (table, key, what)
    if T.get_key(text, "terminal", "copy_on_select") is None and (
            re.search(r"\bcopy_on_select\b", text) or re.search(r"^\s*terminal\.[\w.]+\s*=", text, re.M) or re.search(r"^\s*terminal\s*=\s*\{", text, re.M)):
        return ("terminal.copy_on_select cannot be edited safely in the profile's settings (a dotted key or an inline table defines `terminal`): the "
                "setting cannot be switched for this case without rewriting the owner's file")
    return None


def stream_script(line, tick_file, ticks=200):
    """a POSIX sh script: prints `line`, then rewrites ONE line (carriage return, no new row) ten times a second and records the tick number, so
    output streams while nothing on screen moves"""
    return ("echo '%s'\ni=0\nwhile [ $i -lt %d ]; do\n  printf '\\rtick %%s ' $i\n  echo $i > %s\n  i=$((i+1))\n  sleep 0.1\ndone\nprintf '\\n'\n"
            % (line, ticks, tick_file))


def sgr_script(banner, out):
    """a POSIX sh script: SGR mouse reporting on, a banner line to aim at, bytes_reader.py until Ctrl-C or 60 s, reporting off"""
    return ("printf '\\033[?1000h\\033[?1006h'\necho '%s'\npython3 '%s' %s 60\nprintf '\\033[?1000l\\033[?1006l'\n"
            % (banner, os.path.join(HERE, "bytes_reader.py"), out))


WINCH_PROG = r'''import os, signal, sys, time
out = sys.argv[1]
def on(*_):
    try:
        s = os.get_terminal_size(0)
        with open(out, "a") as f:
            f.write("%d %d\n" % (s.lines, s.columns))
    except OSError:
        pass
signal.signal(signal.SIGWINCH, on)
open(out + ".ready", "w").close()
end = time.time() + 90
try:
    while time.time() < end:
        time.sleep(0.2)
except KeyboardInterrupt:
    pass
'''


def KP(ctx, name):
    """key spec of a pane action on this platform (app/src/util/bindings.rs: ToggleMaximizePane = ctrl-alt-m, macOS cmd-shift-enter)"""
    return {"maximize": "cmd+shift+Return" if U.mac(ctx) else "ctrl+alt+m"}[name]


PALETTE_DIVIDER_LEFT = "move divider left"      # app/i18n/en/warp.ftl keybinding-desc-pane-group-resize-left = "Resize pane > Move divider left"


# ================================================================ shared steps
def linux_pointer_or_skip(ctx, r, name):
    """the Linux pointer (xdotool, or the Wayland virtual pointer / sway seat); macOS and Windows SKIP naming the missing capability"""
    if ctx.win:
        r.skip("%s: no pointer synthesis in the Windows backend yet (mouse SendInput, gap H1)" % name)
    return U.mouse_or_skip(ctx, r)


def safe_tok(ctx, prefix, n=7):
    """an OCR-safe token: `prefix` (OCR-safe letters) + n glyphs"""
    return prefix + U.ocr_safe(ctx.tok(n).lower().replace("_", ""))[-n:]


def locate_many(ctx, r, tag, tokens, since=None):
    """{token: words() row} for the tokens found on screen: the app's find hook for each, then ONE screen read for the rest"""
    out, missing = {}, []
    for t in tokens:
        h = U._hook_word(ctx, t, since)
        if h:
            out[t] = h
        else:
            missing.append(t)
    if missing:
        ws, _ = U.words(ctx, tag)
        for t in missing:
            hits = U.token_hits(ws, t)
            if hits:
                out[t] = hits[-1]
    return out


def need_all(ctx, r, found, tokens, tag):
    """True when every token has a position; else the case cannot place the pointer: NOREF on Wayland (as the other pointer cases), a failed check
    on X11, where the screen read is judged"""
    missing = [t for t in tokens if t not in found]
    if not missing:
        return True
    U.wl_check(ctx, r, False, "the words %r were not found on screen (shots/*%s.png): cannot place the pointer" % (missing, tag), "OCR")
    return False


def print_file(ctx, r, fname, text, tokens=()):
    """write `text` to <W>/fname, register `tokens` with the find hook, `cat` it at the owner rate -> the app-log offset before the cat, or None
    when the line did not reach the shell as typed"""
    with open(os.path.join(ctx.W, fname), "w", newline="\n") as f:
        f.write(text)
    if tokens:
        U.register_find(ctx, *tokens)
    seen = ctx.log_size()
    mark = ctx.cmd_mark()
    line = "cat %s" % fname
    ran = U.run_cmd(ctx, line)
    if ctx.prof.commands() and not r.check(ran and ctx.cmds_since(mark)[-1:] == [line], "`%s` did not reach the shell as typed (received %r)" % (line, ctx.cmds_since(mark))):
        return None
    time.sleep(1.0)
    return seen


class Held:
    """a modifier held while the pointer acts: `with Held(ctx, "ctrl"): mouse.click(...)`. The body must end within `ms` on Wayland."""

    def __init__(self, ctx, mod, ms=1500):
        self.ctx, self.mod, self.ms, self.proc, self.up = ctx, mod, ms, None, None

    def __enter__(self):
        down, self.up = hold_argv(self.ctx.be.name, self.mod, self.ms)
        if self.up:
            self.ctx.be.refocus()
            subprocess.run(down, timeout=10)
        else:
            self.proc = subprocess.Popen(down, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        time.sleep(0.25)
        return self

    def __exit__(self, *_):
        if self.up:
            subprocess.run(self.up, timeout=10)
        elif self.proc:
            try:
                self.proc.wait(timeout=self.ms / 1000.0 + 5)
            except subprocess.TimeoutExpired:
                self.proc.kill()
        time.sleep(0.2)
        return False


def wheel_or_skip(ctx, r):
    if U.on_wayland(ctx) and not U.wl_pointer_real(ctx):
        r.skip("Wayland without the virtual pointer holder (APPCI_WL_PTR, wlptr.py): the sway seat commands have no wheel")


def wheel(ctx, m, x, y, steps):
    """`steps` wheel steps at (x, y): positive = down (later lines), negative = up"""
    m.move(x, y)
    if ctx.be.name == "x11":
        subprocess.run(["xdotool", "click", "--repeat", str(abs(steps)), "--delay", "40", "5" if steps > 0 else "4"], timeout=20)
    else:
        for _ in range(abs(steps)):
            m._wl("axis %d" % (WL_AXIS if steps > 0 else -WL_AXIS))
            time.sleep(0.04)
    time.sleep(0.6)


def primary_get(ctx):
    """the PRIMARY selection as text ('' when empty or the tool failed)"""
    try:
        return WV.norm_clip(subprocess.run(primary_argv(ctx.be.name, False), capture_output=True, timeout=10).stdout)
    except Exception:
        return ""


def primary_set(ctx, text):
    try:
        subprocess.run(primary_argv(ctx.be.name, True), input=text.encode(), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=30)
    except Exception:
        pass


def clip_now(ctx):
    return WV.norm_clip(ctx.be.clip_get())


def ptr_check(ctx, r, got, err, msg):
    """the verdict of a pointer selection: on Wayland WITHOUT a proven pointer a clipboard that kept its sentinel is NOREF (the events may never
    have reached the app); everything else is a plain check"""
    if err is not None and got in (SENTINEL, SENT_PRIMARY) and U.on_wayland(ctx) and not U.wl_pointer_real(ctx):
        return U.wl_check(ctx, r, False, msg, "pointer")
    return r.check(err is None, msg)


def copy_after(ctx, do, key=None, tag=None):
    """the clipboard after `do()` and the copy key; a sentinel first, so a copy that did nothing cannot pass"""
    ctx.be.clip_set(SENTINEL)
    do()
    ctx.be.key(key or U.K(ctx, "copy"))
    time.sleep(0.5)
    got = clip_now(ctx)
    if tag:
        ctx.shot(tag)
    return got


# ================================================================ T21 link_and_path_click
def read_text(path):
    try:
        return open(path, errors="replace").read()
    except OSError:
        return ""


def hover(m, row):
    """put the pointer on a word and nudge it, so the app's hover scan (async for a file path) has highlighted the link before the click"""
    m.move(row[1], row[2])
    time.sleep(0.7)
    m.move(row[1] + 2, row[2])
    time.sleep(1.0)


@case("link_and_path_click", order=72, gui=True)
def c_link_and_path_click(ctx, r):
    """T21. One launch with a recording `xdg-open` first on the app's PATH, under `strace -e trace=network`. `cat` prints a URL, an OSC 8 link
    whose text is not its target, and `<file>:<line>`. Hover alone opens nothing. Ctrl+click on each: the recorder holds exactly the URL, exactly
    the OSC 8 target, and for the path either the absolute path (the profile opens files with the system handler) or the file's line is drawn in
    the app. The trace shows no destination beyond loopback and the configured model. The app is relaunched normally at the end."""
    import cases_netless as NL
    R.need_linux_app(ctx, "link_and_path_click: the Linux pointer, a recording xdg-open on the app's PATH and strace")
    m = linux_pointer_or_skip(ctx, r, "link_and_path_click")
    U.need_tesseract(r)
    traced = NL.strace_works()
    tok = ctx.tok(6).lower()
    t_url, t_vis, t_tgt, t_path = safe_tok(ctx, "ua"), safe_tok(ctx, "ha"), safe_tok(ctx, "ht"), safe_tok(ctx, "pa")
    t_first, t_line = safe_tok(ctx, "fa"), safe_tok(ctx, "fc")
    url, osc_url = "https://192.0.2.7/%s" % t_url, "https://192.0.2.9/%s" % t_tgt        # TEST-NET-1: never loopback, never routed
    line_no, src = 140, "%s.rs" % t_path
    abs_src = os.path.join(ctx.W, src)
    with open(abs_src, "w", newline="\n") as f:
        f.write("".join("// %s\n" % (t_first if i == 1 else t_line if i == line_no else "row %d" % i) for i in range(1, 221)))
    fixture = "url %s\n\nosc \x1b]8;;%s\x1b\\%s\x1b]8;;\x1b\\ end\n\npath %s:%d\n" % (url, osc_url, t_vis, src, line_no)
    bindir, olog = os.path.join(ctx.prof.home, "lc_bin"), os.path.join(ctx.prof.home, "lc_open_%s.log" % tok)
    os.makedirs(bindir, exist_ok=True)
    for n in OPENER_NAMES:
        with open(os.path.join(bindir, n), "w") as f:
            f.write(opener_script(olog))
        os.chmod(os.path.join(bindir, n), 0o755)
    trace = os.path.join(ctx.run, "lc_%s_%s.strace" % (ctx.label.replace("@", "_"), tok))
    calls = lambda: opener_calls(read_text(olog))      # noqa: E731
    import app as appmod
    old_env = dict(ctx.app.extra_env)
    # App.start() builds the app's PATH the same way; extra_env is applied last, so this is that PATH with the recorder's directory first
    ctx.app.extra_env = dict(old_env, PATH=bindir + os.pathsep + old_env.get("PATH", appmod.with_user_bin_dirs(os.environ.get("PATH", ""))))
    if traced:
        ctx.app.wrap = "strace -f -qq -e trace=network -s 48 -o '%s' " % trace
    try:
        _, up = R.start_app(ctx, r, "lc_traced")
        if not up:
            return
        if ctx.mode == "agent" and ctx.mode_now("lc_mode") == "terminal":
            r.unverifiable("the agent pass's pane shows a terminal session after the case's own relaunch: not run in agent mode")
            return
        seen = print_file(ctx, r, "lc_%s.txt" % tok, fixture, (t_url, t_vis, t_path, t_first, t_line))
        if seen is None:
            return
        found = locate_many(ctx, r, "lc_find", (t_url, t_vis, t_path), seen)
        r.metric("link_positions", {k: list(v[1:3]) for k, v in found.items()})
        if not need_all(ctx, r, found, (t_url, t_vis, t_path), "lc_find"):
            return
        # control: printing the links and resting the pointer on one opens nothing
        hover(m, found[t_url])
        r.check(calls() == [], "the desktop opener was called before any click (printing or hovering opened something): %r" % calls())
        for name, t, want in (("URL", t_url, url), ("OSC 8 link", t_vis, osc_url)):
            n0 = len(calls())
            hover(m, found[t])
            with Held(ctx, "ctrl"):
                m.click(found[t][1], found[t][2])
            ctx.until(lambda: len(calls()) > n0, 6, step=0.2)
            time.sleep(0.5)
            ctx.shot("lc_clicked_%s" % t[:2])
            err = url_open_verdict(calls()[n0:], want)
            msg = ("Ctrl+click on the %s: %s (the pointer hovered the word first; if the app got a plain click, the Ctrl key held by the harness did "
                   "not reach it: shots/*lc_clicked_%s.png shows a link tooltip then)" % (name, err, t[:2]))
            if err and not calls()[n0:] and U.on_wayland(ctx) and not U.wl_pointer_real(ctx):
                U.wl_check(ctx, r, False, msg, "pointer")
            else:
                r.check(err is None, msg)
        # the path
        n0, off = len(calls()), ctx.log_size()
        hover(m, found[t_path])
        with Held(ctx, "ctrl"):
            m.click(found[t_path][1], found[t_path][2])
        ctx.until(lambda: len(calls()) > n0, 4, step=0.2)
        time.sleep(1.0)
        drawn = {} if calls()[n0:] else locate_many(ctx, r, "lc_path_open", (t_line, t_first), off)
        how, err = path_open_verdict(calls()[n0:], abs_src, t_line in drawn, t_first in drawn)
        r.metric("path_opened_by", how or "nothing")
        msg = "Ctrl+click on `%s:%d`: %s" % (src, line_no, err)
        if err and how is None and not calls()[n0:] and U.on_wayland(ctx) and not U.wl_pointer_real(ctx):
            U.wl_check(ctx, r, False, msg, "pointer")
        else:
            r.check(err is None, msg)
        if how in ("system", "external"):
            r.note("the path went to the desktop opener (%s): this profile's code.editor.open_file_editor is not the app's editor, so no file pane is expected" % how)
        r.check(ctx.app.alive(), "the app died while opening links")
    finally:
        ctx.app.wrap = ""
        ctx.app.extra_env = old_env
        try:
            ctx.app.stop()
            text = read_text(trace) if traced else ""
            if not traced:
                r.unverifiable("strace is missing or ptrace is not permitted on this runner: 'no other outbound connection' was not judged")
            elif not text:
                r.unverifiable("the strace output is empty: the trace did not follow the app, 'no other outbound connection' was not judged")
            else:
                prov = NL.provider_addresses()
                bad = [b for b in NL.outbound_attempts(text, ()) if b[1] not in prov]
                r.metric("outbound_attempts", len(bad))
                r.check(not bad, "the app itself tried to reach the network while links were printed, hovered and opened: %s" % "; ".join(
                    "%s %s port %d" % (b[0], b[1] if b[1].startswith("192.0.2.") else "dest#" + NL.fingerprint(b[1], b[2]), b[2]) for b in bad[:8]))
        finally:
            R.start_app(ctx, r, "lc_restore")            # a normally launched app (the real PATH, no trace) for the cases that follow
    U.exact_burst(ctx, r, "after_links", 4)


# ================================================================ T22 selection_modes
@case("selection_modes", order=71, gui=True)
def c_selection_modes(ctx, r):
    """T22. Three blocks of four words each. Double click = that word; triple click = that line; a drag from a word of block 1 to a word of
    block 2 = the rest of line 1 ... the start of line 2; a plain click on block 1 then copy-output = line 1 (control), Shift+click on block 3
    then copy-output = the three lines (the app's Shift+click is the block range); a drag over two words of a running program's first line while
    its last line is rewritten ten times a second = those words. The clipboard is set to a sentinel before every copy."""
    m = linux_pointer_or_skip(ctx, r, "selection_modes")
    U.need_tesseract(r)
    words = distinct_words(ctx.rng, 16)
    la, lb, lc, ls = (" ".join(words[i:i + 4]) for i in (0, 4, 8, 12))
    tok = ctx.tok(5).lower()
    U.register_find(ctx, *words)
    seen = None
    for name, line in (("a", la), ("b", lb), ("c", lc)):
        off = print_file(ctx, r, "sm_%s_%s.txt" % (name, tok), line + "\n")
        if off is None:
            return
        seen = seen if seen is not None else off
    try:
        found = locate_many(ctx, r, "sm_find", words[:12], seen)
        if not need_all(ctx, r, found, words[:12], "sm_find"):
            return
        # double click: the word
        w = words[1]
        got = copy_after(ctx, lambda: m.click(found[w][1], found[w][2], 1, 2), tag="sm_double")
        err = None if got.strip() == w else "clipboard %r" % got[:60]
        ptr_check(ctx, r, got, err, "double click on %r + copy: %s, expected exactly the word" % (w, err))
        WV.back_to_input(ctx, r)
        # triple click: the line
        w = words[6]
        got = copy_after(ctx, lambda: m.click(found[w][1], found[w][2], 1, 3), tag="sm_triple")
        err = None if got.strip("\n").rstrip(" ") == lb else "clipboard %r" % got[:80]
        ptr_check(ctx, r, got, err, "triple click on %r + copy: %s, expected exactly the line %r" % (w, err, lb))
        WV.back_to_input(ctx, r)
        # drag over two blocks
        wa, wb = words[2], words[5]
        a0, b1 = word_spans(la)[wa][0], word_spans(lb)[wb][1]
        if r.check(found[wb][2] > found[wa][2] + 4, "block 2's line is not below block 1's on screen (y %d vs %d): the drag would not cross blocks" % (found[wb][2], found[wa][2])):
            got = copy_after(ctx, lambda: m.drag(found[wa][3] + 1, found[wa][2], found[wb][4] - 1, found[wb][2]), tag="sm_cross")
            err = judge_cross_block(got, la, a0, lb, b1)
            ptr_check(ctx, r, got, err, "drag from %r (block 1) to %r (block 2) + copy: %s" % (wa, wb, err))
            mid = got.strip("\n").split("\n")[1:-1]
            r.metric("cross_block_middle_lines", len(mid))
            if err is None and not any("sm_b_%s.txt" % tok in l for l in mid):
                r.note("the text copied between the two blocks' lines does not hold block 2's command line (%d lines between): recorded, not judged" % len(mid))
        WV.back_to_input(ctx, r)
        # Shift+click: the block range. Control first: the plain click selects ONE block.
        got = copy_after(ctx, lambda: m.click(found[words[0]][1], found[words[0]][2]), key=WV.KX(ctx, "copy_output"), tag="sm_block_one")
        err = "nothing was copied (no block selected)" if got == SENTINEL else WV.judge_wrapped_copy(got, [la])
        one_ok = ptr_check(ctx, r, got, err, "plain click on block 1 + copy-output: %s (expected that block's output only)" % err)
        ctx.be.clip_set(SENTINEL)
        with Held(ctx, "shift"):
            m.click(found[words[11]][1], found[words[11]][2])
        ctx.be.key(WV.KX(ctx, "copy_output"))
        time.sleep(0.5)
        got = clip_now(ctx)
        ctx.shot("sm_block_range")
        err = "nothing was copied" if got == SENTINEL else WV.judge_wrapped_copy(got, [la, lb, lc])
        if one_ok:
            ptr_check(ctx, r, got, err, "Shift+click on block 3 after the click on block 1 + copy-output: %s (expected the three blocks' outputs)" % err)
        else:
            r.note("Shift+click range not judged: the plain click before it selected no single block (clipboard after Shift+click: %r)" % got[:60])
        WV.back_to_input(ctx, r)
        # a drag while output streams
        stream_arm(ctx, r, m, ls, words[12:], tok)
    finally:
        WV.back_to_input(ctx, r)
    U.exact_burst(ctx, r, "after_selection", 4)


def stream_arm(ctx, r, m, line, ws, tok):
    """selection_modes: drag over ws[1]..ws[2] of the first line of a program that keeps rewriting its last line; the tick file proves it was
    running before the drag and after the copy. Ctrl-C ends it."""
    tick, script = "sm_tick_%s.txt" % tok, "sm_stream_%s.sh" % tok
    ctx.rm(tick)
    with open(os.path.join(ctx.W, script), "w", newline="\n") as f:
        f.write(stream_script(line, tick))
    seen = ctx.log_size()
    mark, prompts0 = ctx.cmd_mark(), ctx.prof.prompts()
    ctx.be.type("sh %s" % script, ctx.rng.randint(10, 25), enter=True)

    def ticks():
        try:
            return int((ctx.lines(tick) or ["-1"])[0])
        except ValueError:
            return -1
    try:
        if not r.check(bool(ctx.until(lambda: ticks() >= 3, 10)), "`sh %s` did not start streaming (no tick file): the streaming drag was not run" % script):
            return
        found = locate_many(ctx, r, "sm_stream_find", (ws[1], ws[2]), seen)
        if not need_all(ctx, r, found, (ws[1], ws[2]), "sm_stream_find"):
            return
        a0, b1 = word_spans(line)[ws[1]][0], word_spans(line)[ws[2]][1]
        t0 = ticks()
        got = copy_after(ctx, lambda: m.drag(found[ws[1]][3] + 1, found[ws[1]][2], found[ws[2]][4] - 1, found[ws[2]][2]), tag="sm_stream")
        t1 = ticks()
        time.sleep(0.5)
        t2 = ticks()
        r.metric("stream_ticks_around_drag", [t0, t1, t2])
        if not (t0 < t1 < t2):
            r.unverifiable("the program was not streaming through the drag and the copy (ticks %d, %d, %d): the streaming drag was not judged" % (t0, t1, t2))
            return
        err = "nothing was copied (the clipboard kept its sentinel)" if got == SENTINEL else WV.judge_drag_copy(got, line, a0, b1)
        ptr_check(ctx, r, got, err, "drag over %r..%r while the program streamed (ticks %d -> %d) + copy: %s" % (ws[1], ws[2], t0, t1, err))
    finally:
        ctx.be.key("ctrl+c")
        ctx.until(lambda: ctx.prof.prompts() > prompts0, 8)
        time.sleep(1.6 if ctx.mode == "agent" else 0.4)        # past the agent view's exit-confirmation window before any later Ctrl-C
        if ctx.prof.commands():
            have = ctx.await_commands(mark, 1)
            r.check(have[:1] == ["sh %s" % script], "`sh %s` did not reach the shell as typed (received %r)" % (script, have))


# ================================================================ T23 primary_and_middle_click
def input_position(ctx, r, tag):
    """where the input is on screen: a probe word typed into it and found, then cleared with Ctrl-U (as ui_right_click_paste) -> a words() row or None"""
    probe = safe_tok(ctx, "pw")
    U.register_find(ctx, probe)
    seen = ctx.log_size()
    ctx.be.type(probe, 12)
    time.sleep(0.6)
    p, _ = U.find_token(ctx, tag, probe, since=seen, r=r)
    ctx.be.key("ctrl+u")
    time.sleep(0.3)
    if not p:
        U.wl_check(ctx, r, False, "the probe %r typed into the input was not found on screen (shots/*%s.png): cannot place the pointer" % (probe, tag), "OCR")
    return p


@case("primary_and_middle_click", order=71, gui=True)
def c_primary_and_middle_click(ctx, r):
    """T23. terminal.copy_on_select off, then on (the settings file rewritten while the app runs, its bytes put back). Per arm, with CLIPBOARD and
    PRIMARY holding different sentinels: a drag over two words puts them in PRIMARY; a double click puts exactly the word there; CLIPBOARD keeps
    its sentinel when the setting is off and holds the selection when it is on; then `echo `, a middle click in the input, ` >> file` + Enter:
    the shell received `echo <word> >> file` (CLIPBOARD holds another text at that moment, so a paste from it fails)."""
    m = linux_pointer_or_skip(ctx, r, "primary_and_middle_click")
    U.need_tesseract(r)
    p = U.settings_path(ctx)
    if not os.path.isfile(p):
        r.skip("settings file %s not found" % p)
    raw = open(p, "rb").read()
    why = cos_profile_block(raw.decode("utf-8", errors="replace"))
    if why:
        r.skip(why)
    probe = "PRIMARY_PROBE_%s" % ctx.tok(5)
    primary_set(ctx, probe)
    time.sleep(0.3)
    if primary_get(ctx) != probe:
        r.skip("this display has no PRIMARY selection the harness can write and read back (%s): the app drops its PRIMARY writes there (#2081; before "
               "round 15 it wrote them to CLIPBOARD instead), and primary_absent_clipboard_kept is the case that judges such a display" % " / ".join(" ".join(primary_argv(ctx.be.name, w)) for w in (True, False)))
    words = distinct_words(ctx.rng, 5)
    line, tok = " ".join(words), ctx.tok(5).lower()
    seen = print_file(ctx, r, "pm_%s.txt" % tok, line + "\n", words)
    if seen is None:
        return
    try:
        found = locate_many(ctx, r, "pm_find", words, seen)
        if not need_all(ctx, r, found, words, "pm_find"):
            return
        inp = input_position(ctx, r, "pm_input")
        if not inp:
            return
        a0, b1 = word_spans(line)[words[1]][0], word_spans(line)[words[2]][1]
        for on, word in ((False, words[3]), (True, words[4])):
            arm = "on" if on else "off"
            U.set_setting(ctx, "terminal", "copy_on_select", "true" if on else "false")
            time.sleep(2.0)                                   # the live settings reload (ui_settings_live)
            sent_c = "SENTINEL_CLIP_%s" % arm
            # a drag
            ctx.be.clip_set(sent_c)
            primary_set(ctx, SENT_PRIMARY)
            m.drag(found[words[1]][3] + 1, found[words[1]][2], found[words[2]][4] - 1, found[words[2]][2])
            time.sleep(0.6)
            prim, clip = primary_get(ctx), clip_now(ctx)
            ctx.shot("pm_drag_%s" % arm)
            err = "PRIMARY kept its sentinel" if prim == SENT_PRIMARY else WV.judge_drag_copy(prim, line, a0, b1)
            ptr_ok = ptr_check(ctx, r, prim, err, "[copy_on_select %s] drag over %r..%r: PRIMARY: %s" % (arm, words[1], words[2], err))
            if not ptr_ok and prim == SENT_PRIMARY:
                continue                                      # nothing was selected: the rest of this arm would only repeat it
            if on:
                r.check(clip == prim and clip != sent_c, "[copy_on_select on] after the drag CLIPBOARD holds %r, PRIMARY %r: expected the same selection in both "
                        "(or the settings file write was not applied live)" % (clip[:60], prim[:60]))
            else:
                r.check(clip == sent_c, "[copy_on_select off] the drag changed CLIPBOARD to %r (it held %r): only PRIMARY may take a selection "
                        "(or the settings file write was not applied live)" % (clip[:60], sent_c))
            WV.back_to_input(ctx, r)
            # a double click: exactly the word
            ctx.be.clip_set(sent_c)
            primary_set(ctx, SENT_PRIMARY)
            m.click(found[word][1], found[word][2], 1, 2)
            time.sleep(0.6)
            prim, clip = primary_get(ctx), clip_now(ctx)
            ptr_check(ctx, r, prim, None if prim == word else "PRIMARY %r" % prim[:60], "[copy_on_select %s] double click on %r: PRIMARY holds %r, expected exactly the word" % (arm, word, prim[:60]))
            r.check(clip == (word if on else sent_c), "[copy_on_select %s] after the double click CLIPBOARD holds %r, expected %r" % (arm, clip[:60], word if on else sent_c))
            if prim != word:
                continue
            # the middle click pastes PRIMARY into the input; CLIPBOARD holds another text now
            ctx.be.clip_set("SENTINEL_NOT_THE_PASTE")
            eff = "pm_%s_%s.txt" % (arm, tok)
            ctx.rm(eff)
            want = "echo %s >> %s" % (word, eff)
            mark = ctx.cmd_mark()
            ctx.be.type("echo ", ctx.rng.randint(10, 25))
            m.click(inp[1], inp[2], 2)
            time.sleep(0.6)
            ctx.shot("pm_pasted_%s" % arm)
            ctx.be.type(" >> %s" % eff, ctx.rng.randint(10, 25), enter=True)
            ctx.until(lambda: ctx.lines(eff), 8)
            time.sleep(0.4)
            if ctx.prof.commands():
                have = ctx.await_commands(mark, 1)
                r.check(have == [want], "[copy_on_select %s] `echo ` + middle click in the input + ` >> %s`: the shell received %r, expected exactly [%r] "
                        "(SENTINEL_NOT_THE_PASTE in it = the paste came from CLIPBOARD, not PRIMARY)" % (arm, eff, have, want))
            r.check(ctx.lines(eff) == [word], "[copy_on_select %s] %s = %r, expected [%r]" % (arm, eff, ctx.lines(eff), word))
            ctx.reset()
    finally:
        with open(p, "wb") as f:
            f.write(raw)
        time.sleep(1.5)
        WV.back_to_input(ctx, r)
    U.exact_burst(ctx, r, "after_primary", 3)


# ================================================================ T24 wheel_scroll
def read_visible(ctx, tag, words):
    ws, _ = U.words(ctx, tag)
    return visible_indexes(ws, words), ws


def aim_at(ws, words, fallback):
    """a point on a fixture word near the middle of the ones on screen (the wheel goes where the text is), else `fallback`"""
    rows = sorted((row for row in ws if re.sub(r"[^0-9a-z]", "", row[0].lower()) in set(words)), key=lambda row: row[2])
    return (rows[len(rows) // 2][1], rows[len(rows) // 2][2]) if rows else fallback


def judge_scroll(ctx, r, what, sets, toward_start):
    st, text = scroll_verdict(sets[0], sets[1], sets[2], toward_start)
    r.metric("wheel_%s" % what, text)
    if st == "unread":
        r.unverifiable("wheel in %s: %s: not judged (shots/*ws_%s_*.png)" % (what, text, what))
    else:
        r.check(st == "ok", "wheel in %s: %s" % (what, text))


@case("wheel_scroll", order=72, gui=True)
def c_wheel_scroll(ctx, r):
    """T24. (1) 240 one-word lines printed: wheel up shows earlier lines, wheel down returns to the last ones. (2) `less` on the same file: wheel
    down shows later lines, wheel up returns to the first. (3) A program that switched SGR mouse reporting on (bytes_reader.py): a click ON its
    banner arrives as ESC[<0;x;yM + ESC[<0;x;ym, wheel up as ESC[<64, wheel down as ESC[<65, and the key typed after them arrives last and
    alone. Where the profile turns mouse or scroll reporting off, the program must receive nothing for it."""
    m = linux_pointer_or_skip(ctx, r, "wheel_scroll")
    wheel_or_skip(ctx, r)
    U.need_tesseract(r)
    n = 240
    words = distinct_words(ctx.rng, n)
    tok = ctx.tok(5).lower()
    fname = "ws_%s.txt" % tok
    if print_file(ctx, r, fname, "".join(w + "\n" for w in words)) is None:
        return
    centre = None
    try:
        from PIL import Image
        w_, h_ = Image.open(ctx.shot("ws_size")).size
        centre = (w_ // 2, h_ // 2)
    except Exception:
        pass
    # (1) scrollback
    v0, ws0 = read_visible(ctx, "ws_scrollback_0", words)
    at = aim_at(ws0, words, centre)
    if at is None:
        r.unverifiable("no fixture line was read on screen after printing %d lines and Pillow is not there to find the window centre: nowhere to aim the wheel" % n)
        return
    wheel(ctx, m, at[0], at[1], -8)
    v1, _ = read_visible(ctx, "ws_scrollback_1", words)
    wheel(ctx, m, at[0], at[1], 40)
    v2, _ = read_visible(ctx, "ws_scrollback_2", words)
    judge_scroll(ctx, r, "scrollback", (v0, v1, v2), True)
    # (2) less
    if not ctx.which("less"):
        r.unverifiable("`less` is not installed on this machine: the pager arm was not run")
    else:
        mark, prompts0 = ctx.cmd_mark(), ctx.prof.prompts()
        ctx.be.type("less %s" % fname, ctx.rng.randint(10, 25), enter=True)
        time.sleep(1.5)
        try:
            l0, wsl = read_visible(ctx, "ws_less_0", words)
            at2 = aim_at(wsl, words, at)
            wheel(ctx, m, at2[0], at2[1], 8)
            l1, _ = read_visible(ctx, "ws_less_1", words)
            wheel(ctx, m, at2[0], at2[1], -40)
            l2, _ = read_visible(ctx, "ws_less_2", words)
            judge_scroll(ctx, r, "less", (l0, l1, l2), False)
        finally:
            ctx.be.type("q", 15)
            ctx.until(lambda: ctx.prof.prompts() > prompts0, 8)
            time.sleep(0.5)
        if ctx.prof.commands():
            have = ctx.await_commands(mark, 1)
            r.check(have[:1] == ["less %s" % fname], "`less %s` did not reach the shell as typed (received %r); a `q` in the list means the pager never started" % (fname, have))
    # (3) a program that asked for mouse reporting
    sgr_arm(ctx, r, m, tok)
    U.exact_burst(ctx, r, "after_wheel", 3)


def sgr_arm(ctx, r, m, tok):
    if not ctx.which("python3"):
        r.unverifiable("python3 is not on the pane's PATH: the mouse-reporting arm (bytes_reader.py) was not run")
        return
    banner, out, script = safe_tok(ctx, "wm"), "ws_bytes_%s.txt" % tok, "ws_sgr_%s.sh" % tok
    ctx.rm(out, out + ".ready")
    with open(os.path.join(ctx.W, script), "w", newline="\n") as f:
        f.write(sgr_script("aim %s here" % banner, out))
    try:
        want_mouse, want_wheel = reporting_expected(open(U.settings_path(ctx), encoding="utf-8", errors="replace").read())
    except OSError:
        want_mouse = want_wheel = True
    r.metric("profile_reports_mouse_wheel", [want_mouse, want_wheel])
    U.register_find(ctx, banner)
    seen = ctx.log_size()
    mark, prompts0 = ctx.cmd_mark(), ctx.prof.prompts()
    ctx.be.type("sh %s" % script, ctx.rng.randint(10, 25), enter=True)
    data = lambda: b"".join(U._reader_chunks(ctx, out))      # noqa: E731
    try:
        if not r.check(bool(ctx.until(lambda: os.path.exists(os.path.join(ctx.W, out + ".ready")), 20)), "the raw reader never started (`sh %s`)" % script):
            return
        time.sleep(1.5)                                       # the block counts as long-running; the banner is drawn
        b, _ = U.find_token(ctx, "ws_sgr_find", banner, since=seen, r=r)
        if not b:
            U.wl_check(ctx, r, False, "the program's banner %r was not found on screen (shots/*ws_sgr_find.png): cannot place the pointer in its block" % banner, "OCR")
            return
        steps = []
        for what, act in (("click", lambda: m.click(b[1], b[2])), ("wheel up", lambda: wheel(ctx, m, b[1], b[2], -3)), ("wheel down", lambda: wheel(ctx, m, b[1], b[2], 3))):
            n0 = len(data())
            act()
            ctx.until(lambda: len(data()) > n0, 3)
            time.sleep(0.4)
            steps.append((what, data()[n0:]))
        ctx.shot("ws_sgr_after")
        for what, got in steps:
            expected = want_mouse if what == "click" else want_wheel
            if not expected:
                r.check(got == b"", "%s with reporting switched off in the profile: the program received %s, expected nothing" % (what, got.hex()))
                continue
            err = sgr_click_verdict(got) if what == "click" else sgr_wheel_verdict(got, what == "wheel up")
            msg = "%s on the banner of a program that asked for SGR mouse reporting: %s" % (what, err)
            if err and not got and U.on_wayland(ctx) and not U.wl_pointer_real(ctx):
                U.wl_check(ctx, r, False, msg, "pointer")
            else:
                r.check(err is None, msg)
        n0 = len(data())
        ctx.be.type("k", 20)
        ctx.until(lambda: len(data()) > n0, 5)
        time.sleep(0.3)
        r.check(data()[n0:] == b"k", "the key typed after the mouse reports: the program received %s, expected 6b alone and last" % (data()[n0:].hex() or "nothing"))
    finally:
        ctx.be.key("ctrl+c")                                  # ends the reader (raw tty: it reads the byte and exits); the script then switches reporting off
        ctx.until(lambda: ctx.prof.prompts() > prompts0, 8)
        time.sleep(1.6)                                       # past the agent view's exit-confirmation window before reset() sends its own Ctrl-C
        ctx.reset()
        if ctx.prof.commands():
            have = ctx.await_commands(mark, 1)
            r.check(have[:1] == ["sh %s" % script], "`sh %s` did not reach the shell as typed (received %r)" % (script, have))


# ================================================================ T25 pane_resize_maximize
def pane_probe(ctx, r, name):
    """type the probe into whichever pane has the keyboard -> (pid, rows, cols) or None; the received line is checked exactly"""
    f = "pr_%s.txt" % name
    ctx.rm(f)
    line = pane_probe_cmd(name)
    mark = ctx.cmd_mark()
    ctx.be.type(line, ctx.rng.randint(10, 25), enter=True)
    ctx.until(lambda: ctx.lines(f), 8)
    got = parse_pane_probe(ctx.lines(f))
    if ctx.prof.commands():
        have = ctx.await_commands(mark, 1)
        r.check(have.count(line) == 1, "pane probe %s: the shell received %r, expected the line %r exactly once" % (name, have, line))
    if not r.check(got is not None, "pane probe %s: no `pid rows cols` answer (%r): no shell took the line" % (name, ctx.lines(f))):
        return None
    r.metric("pane_%s" % name, "pid %d %dx%d" % got)
    return got


def palette_run(ctx, text, tag):
    """run a command palette entry by its name (the keyboard route to an action that has no default key)"""
    ctx.be.key(U.K(ctx, "palette"))
    time.sleep(1.0)
    ctx.be.type(text, 15)
    time.sleep(0.9)
    ctx.shot(tag)
    ctx.be.key("Return")
    time.sleep(0.9)


def fast_drag(ctx, m, x, y, dx):
    """press at (x, y), move by dx in three steps, release: about 0.3 s (the divider sweep makes up to 40 of them)"""
    if ctx.be.name == "x11":
        a = ["xdotool", "mousemove", str(int(x)), str(int(y)), "sleep", "0.06", "mousedown", "1", "sleep", "0.05"]
        for i in (1, 2, 3):
            a += ["mousemove", str(int(x + dx * i / 3)), str(int(y)), "sleep", "0.03"]
        subprocess.run(a + ["mouseup", "1"], timeout=15)
    else:
        m.move(x, y)
        m._btn(1, True)
        time.sleep(0.05)
        for i in (1, 2, 3):
            if not m._move(x + dx * i / 3, y):
                subprocess.run(["swaymsg", "seat - cursor set %d %d" % (x + dx * i / 3, y)], capture_output=True, timeout=10)
            time.sleep(0.03)
        m._btn(1, False)
    time.sleep(0.12)


def divider_drag(ctx, r, m, tok, b_before):
    """Linux: with the right pane focused, find the divider left of its first column and drag it DIVIDER_DRAG_PX left. A SIGWINCH logger running
    in that pane says at once when a drag resized it. -> (probe of the right pane after, expected column change) or None when not judged."""
    if not ctx.which("python3"):
        r.unverifiable("python3 is not on the pane's PATH: the divider drag (its SIGWINCH logger) was not run")
        return None
    w1, w2 = distinct_words(ctx.rng, 2)
    seen = print_file(ctx, r, "pr_fix_%s.txt" % tok, "%s %s\n" % (w1, w2), (w1, w2))
    if seen is None:
        return None
    found = locate_many(ctx, r, "pr_find", (w1, w2), seen)
    if not need_all(ctx, r, found, (w1, w2), "pr_find"):
        return None
    cell = (found[w2][3] - found[w1][3]) / float(len(w1) + 1)
    if not r.check(4 <= cell <= 40, "the two words' positions give a cell width of %.1f px: the screen read is not usable for the drag" % cell):
        return None
    out, prog = "pr_winch_%s.txt" % tok, "pr_winch_%s.py" % tok
    ctx.rm(out, out + ".ready")
    with open(os.path.join(ctx.W, prog), "w") as f:
        f.write(WINCH_PROG)
    mark, prompts0 = ctx.cmd_mark(), ctx.prof.prompts()
    ctx.be.type("python3 %s %s" % (prog, out), ctx.rng.randint(10, 25), enter=True)
    hit = None
    try:
        if not r.check(bool(ctx.until(lambda: os.path.exists(os.path.join(ctx.W, out + ".ready")), 15)), "the SIGWINCH logger never started in the right pane"):
            return None
        time.sleep(0.5)
        x0, y = found[w1][3], found[w1][2]
        for d in range(1, DIVIDER_SWEEP_PX + 1):
            n0 = len(ctx.lines(out))
            fast_drag(ctx, m, x0 - d, y, -DIVIDER_DRAG_PX)
            if ctx.until(lambda: len(ctx.lines(out)) > n0, 0.25):
                hit = d
                break
        r.metric("divider_px_left_of_text", hit if hit is not None else -1)
        if hit is None:
            m.click(found[w2][1], found[w2][2])               # the sweep's last presses may have landed in the left pane: the right pane gets the keyboard back
        time.sleep(0.6)
        ctx.shot("pr_dragged")
    finally:
        ctx.be.key("ctrl+c")
        ctx.until(lambda: ctx.prof.prompts() > prompts0, 8)
        time.sleep(1.6 if ctx.mode == "agent" else 0.4)
        if ctx.prof.commands():
            have = ctx.await_commands(mark, 1)             # logged before the next probe takes its mark
            r.check(have[:1] == ["python3 %s %s" % (prog, out)], "`python3 %s %s` did not reach the shell as typed (received %r)" % (prog, out, have))
    if hit is None:
        r.unverifiable("no drag starting 1..%d px left of the right pane's text resized that pane (its SIGWINCH logger saw nothing): the divider was "
                       "not found, the divider drag was not judged (shots/*pr_dragged.png)" % DIVIDER_SWEEP_PX)
        return None
    after = pane_probe(ctx, r, "bdrag")
    if after is None:
        return None
    last = (ctx.lines(out) or [""])[-1].split()
    r.check(last == [str(after[1]), str(after[2])], "the program running in the dragged pane last saw %r on SIGWINCH; `stty size` there now says %dx%d" % (last, after[1], after[2]))
    r.check(after[0] == b_before[0], "after the divider drag the probe ran in pid %d, not the right pane's shell %d" % (after[0], b_before[0]))
    return after, DIVIDER_DRAG_PX / cell


@case("pane_resize_maximize", order=74, gui=True)
def c_pane_resize_maximize(ctx, r):
    """T25. Every step is read from the pane that has the keyboard: `sh -c 'echo $PPID $(stty size)'`. Split right: a new shell, both panes
    narrower. Maximise (key): the same shell, as wide as the unsplit pane. Restore: exactly the split size. `Move divider left` from the command
    palette, twice: the right pane wider, the left narrower, the total kept. Linux: the divider dragged 120 px left: the same, by about that
    many columns, and a program in the pane got SIGWINCH with the final size. The new pane is closed with `exit` and the first pane is as wide
    as at the start."""
    if ctx.win:
        r.skip("pane_resize_maximize: not ported to the Windows panes (the probe is POSIX `sh` + `stty size`)")
    tok = ctx.tok(5).lower()
    a0 = pane_probe(ctx, r, "a0")
    if a0 is None:
        return
    n0, base = len(U.prompt_times(ctx)), ctx.shell_count()
    ctx.be.key(U.K(ctx, "split_right"))
    if not r.check(bool(ctx.until(lambda: len(U.prompt_times(ctx)) > n0, 12)), "split right: no new shell prompt within 12 s (no new pane?)"):
        ctx.shot("pr_split_fail")
        return
    time.sleep(1.0)
    try:
        if ctx.mode == "agent" and ctx.log_view_mode() == "terminal":
            ctx.be.type("/agent", 12, enter=True)             # the agent pass: the new pane in agent view too (as ui_vertical_tabs_many)
            ctx.until(lambda: ctx.log_view_mode() == "agent", 10)
            time.sleep(0.6)
        b1 = pane_probe(ctx, r, "b1")
        ctx.be.key(U.K(ctx, "prev_pane"))
        time.sleep(0.5)
        a1 = pane_probe(ctx, r, "a1")
        ctx.be.key(U.K(ctx, "next_pane"))
        time.sleep(0.5)
        if not (a1 and b1):
            return
        ctx.shot("pr_split")
        errs = split_verdict(a0, a1, b1)
        if not r.check(not errs, "split right: " + "; ".join(errs)):
            return
        # maximise and restore, by key
        ctx.be.key(KP(ctx, "maximize"))
        time.sleep(0.8)
        ctx.shot("pr_max")
        bmax = pane_probe(ctx, r, "bmax")
        ctx.be.key(KP(ctx, "maximize"))
        time.sleep(0.8)
        brest = pane_probe(ctx, r, "brest")
        if not (bmax and brest):
            return
        errs = maximize_verdict(a0, b1, bmax, brest)
        r.check(not errs, "maximise (%s) and restore: %s" % (KP(ctx, "maximize"), "; ".join(errs)))
        if brest[1:] != b1[1:]:
            return                                             # the pane is not in the split state the next steps assume
        # resize by keyboard: the palette entry (no default key on Linux; macOS cmd-ctrl-arrows is a chord the backend refuses)
        for i in (1, 2):
            palette_run(ctx, PALETTE_DIVIDER_LEFT, "pr_palette_%d" % i)
        bk = pane_probe(ctx, r, "bkey")
        ctx.be.key(U.K(ctx, "prev_pane"))
        time.sleep(0.5)
        ak = pane_probe(ctx, r, "akey")
        ctx.be.key(U.K(ctx, "next_pane"))
        time.sleep(0.5)
        if not (ak and bk):
            return
        errs = divider_verdict(a1, brest, ak, bk)
        r.check(not errs, "command palette %r twice: %s (shots/*pr_palette_*.png show the entry that ran)" % (PALETTE_DIVIDER_LEFT, "; ".join(errs)))
        r.check(bk[0] == b1[0] and ak[0] == a0[0], "after the palette resize the probes ran in pids %d and %d, expected the right pane's %d and the left pane's %d" % (bk[0], ak[0], b1[0], a0[0]))
        # Linux: the divider dragged with the pointer
        if ctx.be.name in ("x11", "wayland") and shutil.which("xdotool" if ctx.be.name == "x11" else "swaymsg") and shutil.which("tesseract"):
            m = U.Mouse(ctx.be)
            dragged = divider_drag(ctx, r, m, tok, bk)
            if dragged:
                bd, want = dragged
                ctx.be.key(U.K(ctx, "prev_pane"))
                time.sleep(0.5)
                ad = pane_probe(ctx, r, "adrag")
                ctx.be.key(U.K(ctx, "next_pane"))
                time.sleep(0.5)
                if ad:
                    errs = divider_verdict(ak, bk, ad, bd, want_cols=want)
                    r.check(not errs, "divider dragged %d px left: %s" % (DIVIDER_DRAG_PX, "; ".join(errs)))
        else:
            r.note("the divider drag was not run: %s" % ("macOS has no pointer tool in the harness" if U.mac(ctx) else "xdotool / swaymsg or tesseract is missing"))
    finally:
        ctx.exit_nested(base, r, "pane_resize_maximize")       # closes the new pane only when a probe proves its shell is in front
        time.sleep(0.8)
    end = pane_probe(ctx, r, "end")
    if end:
        r.check(end[0] == a0[0] and end[2] == a0[2], "after the new pane was closed the probe says pid %d, %d columns; the first pane had pid %d, %d columns" % (end[0], end[2], a0[0], a0[2]))
    U.exact_burst(ctx, r, "after_panes", 3)
    r.check(ctx.app.alive(), "app not alive after the pane case")
