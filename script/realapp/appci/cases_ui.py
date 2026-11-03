# appci/cases_ui.py: real-app cases for completions/history (Tab menu, ghost text, Ctrl-R, command palette), the mouse
# (select + copy, right-click Paste, drag-and-drop, split panes, focus switching, resize) and startup time / live settings
# changes (issue #1462). Same rules as cases.py: the EXACT command the shell received (rec.log) plus an effect file, never
# "something ran"; typing at 10-25 ms/key with instant Enter; a case that cannot run SKIPS with the precise reason and the
# package it needs. Registered by the one import line at the end of cases.py. Every case here is full-tier.
#
# Platform notes (docs/testing.md 2.8): keys follow app/src/util/bindings.rs (Linux ctrl-shift-X, macOS cmd-X); the mouse is
# xdotool on X11 and `swaymsg seat - cursor ...` on the headless-sway Wayland job (no extra package); macOS has no mouse
# tool in the harness, so mouse cases skip there. Positions come from tesseract word boxes of a screenshot (package: tesseract-ocr).
import hashlib, os, re, shlex, shutil, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases as C  # noqa: E402  (cases.py imports this module as its LAST line, so everything below is already defined)
import toml_text as T  # noqa: E402
from cases import case  # noqa: E402


# ---------------------------------------------------------------- helpers
def mac(ctx):
    return ctx.be.name == "mac"


def K(ctx, name):
    """key spec of an app action on this platform (app/src/util/bindings.rs: cmd_or_ctrl_shift = cmd-X on macOS, ctrl-shift-X elsewhere)"""
    m = mac(ctx)
    return {"palette": "cmd+p" if m else "ctrl+shift+p", "copy": "cmd+c" if m else "ctrl+shift+c",
            "split_right": "cmd+d" if m else "ctrl+shift+d", "split_down": "cmd+shift+d" if m else "ctrl+shift+e",
            "next_pane": "cmd+]" if m else "ctrl+shift+braceright", "prev_pane": "cmd+[" if m else "ctrl+shift+braceleft"}[name]


def prompt_times(ctx):
    out = []
    for l in ctx.prof.read("prompt.log"):
        p = l.split("\t")
        if len(p) == 2 and p[0] == "P":
            try:
                out.append(float(p[1]))
            except ValueError:
                pass
    return out


def run_cmd(ctx, text, wait=10):
    """type one line at the owner rate with an instant Enter and wait until the shell logged it (rec.log) -> True/False"""
    mark = ctx.cmd_mark()
    ctx.be.type(text, ctx.rng.randint(10, 25), enter=True)
    return bool(ctx.until(lambda: len(ctx.cmds_since(mark)) >= 1, wait))


def exact_burst(ctx, r, tag, n=6):
    """n typed append-commands; effect file AND shell-received commands must match exactly (cases.effect_run)"""
    return C.effect_run(ctx, r, tag, n, 0.0)


def img_diff(a, b):
    """fraction of pixels that differ noticeably between two screenshots; None without Pillow (python3-pil)"""
    try:
        from PIL import Image, ImageChops
    except ImportError:
        return None
    try:
        A, B = Image.open(a).convert("RGB"), Image.open(b).convert("RGB")
    except Exception:
        return None
    if A.size != B.size:
        return 1.0
    d = ImageChops.difference(A, B).convert("L").point(lambda v: 255 if v > 24 else 0)
    hist = d.histogram()
    return hist[255] / float(A.size[0] * A.size[1])


def sha(path):
    try:
        return hashlib.sha256(open(path, "rb").read()).hexdigest()
    except OSError:
        return None


def shot_changed(a, b, thresh):
    d = img_diff(a, b)
    if d is None:
        return (sha(a) != sha(b)), None
    return d >= thresh, d


def need_tesseract(r):
    if not shutil.which("tesseract"):
        r.skip("tesseract not installed on this machine (package: tesseract-ocr): screen positions/text cannot be read")


def words(ctx, tag):
    """[(text, cx, cy, left, right, top, bottom)] of every word tesseract finds in a fresh screenshot, in screenshot pixels"""
    p = ctx.shot(tag)
    if not p:
        return [], None
    out = subprocess.run(["tesseract", p, "-", "--psm", "11", "tsv"], capture_output=True, timeout=90).stdout.decode(errors="replace")
    res = []
    for l in out.splitlines()[1:]:
        c = l.split("\t")
        if len(c) >= 12 and c[11].strip() and c[0] == "5":
            x, y, w, h = int(c[6]), int(c[7]), int(c[8]), int(c[9])
            res.append((c[11].strip(), x + w // 2, y + h // 2, x, x + w, y, y + h))
    return res, p


def find_word(ctx, tag, sub, last=True):
    """the (last = lowest on screen) word whose text contains `sub` (lower-case compare); None if absent"""
    ws, p = words(ctx, tag)
    hits = [w for w in ws if sub.lower() in w[0].lower()]
    hits.sort(key=lambda w: w[2])
    return ((hits[-1] if last else hits[0]) if hits else None), p


class Mouse:
    """pointer synthesis in the job's own display: xdotool (X11) or sway's seat cursor commands (headless Wayland)"""

    def __init__(self, be):
        self.be = be

    def move(self, x, y):
        if self.be.name == "x11":
            subprocess.run(["xdotool", "mousemove", str(int(x)), str(int(y))], timeout=10)
        else:
            subprocess.run(["swaymsg", "seat - cursor set %d %d" % (x, y)], capture_output=True, timeout=10)
        time.sleep(0.15)

    def click(self, x, y, button=1, count=1):
        self.move(x, y)
        if self.be.name == "x11":
            subprocess.run(["xdotool", "click", "--repeat", str(count), "--delay", "70", str(button)], timeout=15)
        else:
            for _ in range(count):
                subprocess.run(["swaymsg", "seat - cursor press button%d" % button], capture_output=True, timeout=10)
                subprocess.run(["swaymsg", "seat - cursor release button%d" % button], capture_output=True, timeout=10)
                time.sleep(0.07)
        time.sleep(0.25)

    def drag(self, x0, y0, x1, y1):
        self.move(x0, y0)
        if self.be.name == "x11":
            subprocess.run(["xdotool", "mousedown", "1"], timeout=10)
            time.sleep(0.15)
            for i in range(1, 9):
                subprocess.run(["xdotool", "mousemove", str(int(x0 + (x1 - x0) * i / 8)), str(int(y0 + (y1 - y0) * i / 8))], timeout=10)
                time.sleep(0.03)
            subprocess.run(["xdotool", "mouseup", "1"], timeout=10)
        else:
            subprocess.run(["swaymsg", "seat - cursor press button1"], capture_output=True, timeout=10)
            time.sleep(0.15)
            for i in range(1, 9):
                subprocess.run(["swaymsg", "seat - cursor set %d %d" % (x0 + (x1 - x0) * i / 8, y0 + (y1 - y0) * i / 8)], capture_output=True, timeout=10)
                time.sleep(0.03)
            subprocess.run(["swaymsg", "seat - cursor release button1"], capture_output=True, timeout=10)
        time.sleep(0.3)


def mouse_or_skip(ctx, r):
    if ctx.be.name == "mac":
        r.skip("macOS: the harness has no pointer tool (htype2/kl3 are keyboard-only; cliclick is not part of the CI toolset and Retina point/pixel mapping is not calibrated)")
    if ctx.be.name not in ("x11", "wayland"):
        r.skip("backend %s has no pointer synthesis" % ctx.be.name)
    if ctx.be.name == "x11" and not shutil.which("xdotool"):
        r.skip("xdotool missing")
    if ctx.be.name == "wayland" and not shutil.which("swaymsg"):
        r.skip("swaymsg missing")
    return Mouse(ctx.be)


def settings_path(ctx):
    return getattr(ctx.app, "settings", None) or os.path.join(ctx.prof.home, ".config", "phosphor", "settings.toml")


def set_setting(ctx, table, key, literal):
    p = settings_path(ctx)
    t = open(p, encoding="utf-8", errors="replace").read()
    open(p, "w", encoding="utf-8").write(T.set_key(t, table, key, literal))


# ---------------------------------------------------------------- (4) completions and history
@case("ui_tab_menu", order=60)
def c_tab_menu(ctx, r):
    """Tab on an ambiguous path (two candidates) completes the common prefix and opens the menu with NOTHING highlighted (#1452; owner
    decision on #1610: keep it as built). tab_return: Tab then Return runs EXACTLY the typed line (`echo TOK >> tc/menuzz_`, as bash
    does) once, the token lands in tc/menuzz_ and in no candidate. down_return: Tab, Down (highlights one candidate), Return accepts it
    without running, the next Return runs exactly that candidate's full line once (an accepted completion ends in one space, which the
    shell's history may keep); the token lands in that one file only and no prefix-named file appears."""
    d = os.path.join(ctx.W, "tc")
    for n in ("menuzz_one.txt", "menuzz_two.txt", "menuzz_"):
        try:
            os.remove(os.path.join(d, n))
        except FileNotFoundError:
            pass
    for n in ("menuzz_one.txt", "menuzz_two.txt"):
        open(os.path.join(d, n), "w").close()
    for variant in ("tab_return", "down_return"):
        try:                                       # #1610: tab_return's line creates tc/menuzz_, which would be an exact-match
            os.remove(os.path.join(d, "menuzz_"))  # candidate #1 for the next variant
        except FileNotFoundError:
            pass
        tok = ctx.tok(6).lower()
        mark = ctx.cmd_mark()
        ctx.be.type("echo %s >> tc/menuzz_" % tok, 15)
        ctx.be.key("Tab")
        time.sleep(0.6)
        ctx.shot("tabmenu_open_" + variant)
        if variant == "tab_return":
            ctx.be.key("Return")                   # nothing highlighted: runs the visible line
            ctx.until(lambda: len(ctx.cmds_since(mark)) >= 1, 8)
            time.sleep(0.4)
            ran = ctx.cmds_since(mark)
            typed = "echo %s >> tc/menuzz_" % tok
            r.check(ran == [typed], "tab_return: shell received %r, expected exactly [%r] (Return with nothing highlighted runs the typed line, #1452)" % (ran, typed))
            r.check(ctx.lines("tc/menuzz_")[-1:] == [tok], "tab_return: token missing from tc/menuzz_")
            for f in ("one", "two"):
                r.check(tok not in ctx.lines("tc/menuzz_%s.txt" % f), "tab_return: token in tc/menuzz_%s.txt (a candidate was applied)" % f)
        else:
            ctx.be.key("Down")
            time.sleep(0.15)
            ctx.be.key("Return")                   # accepts the highlighted candidate, does not run
            time.sleep(0.8)
            r.check(ctx.cmds_since(mark) == [], "down_return: the accepting Return ran the line %r" % ctx.cmds_since(mark))
            ctx.be.key("Return")
            ctx.until(lambda: len(ctx.cmds_since(mark)) >= 1, 8)
            time.sleep(0.4)
            ran = ctx.cmds_since(mark)
            full = {"one": "echo %s >> tc/menuzz_one.txt" % tok, "two": "echo %s >> tc/menuzz_two.txt" % tok}
            exact = {line + sp: f for f, line in full.items() for sp in ("", " ")}
            r.check(len(ran) == 1 and ran[0] in exact, "down_return: shell received %r, expected exactly one of %r" % (ran, sorted(exact)))
            if len(ran) == 1 and ran[0] in exact:
                f = exact[ran[0]]
                other = "two" if f == "one" else "one"
                r.metric("candidate_chosen", f)
                r.check(ctx.lines("tc/menuzz_%s.txt" % f)[-1:] == [tok], "down_return: token missing from tc/menuzz_%s.txt" % f)
                r.check(tok not in ctx.lines("tc/menuzz_%s.txt" % other), "down_return: token also in tc/menuzz_%s.txt" % other)
            r.check(not os.path.exists(os.path.join(d, "menuzz_")), "down_return: a file named like the uncompleted prefix (tc/menuzz_) was created")
        ctx.reset()


@case("ui_ghost_accept", order=61)
def c_ghost(ctx, r):
    """autosuggestion (ghost text) from history: type a prefix of an earlier command, accept with Right at the app's default binding
    (and with End as a second probe), then an instant Enter: the shell must receive the FULL earlier command, byte-exact, at the owner's
    fast cadence (accept 10-25 ms after the key before it) and after a slow wait."""
    ctx.rm("gh.txt")
    toks = [ctx.tok(7).lower() for _ in range(ctx.count(4))]
    for t in toks:                                           # seed this session's history
        r.check(run_cmd(ctx, "echo %s >> gh.txt" % t), "seeding command was not received")
    time.sleep(0.5)
    mark = ctx.cmd_mark()
    exp = []
    for i, t in enumerate(toks):
        ctx.be.type("echo %s >> gh" % t, ctx.rng.randint(10, 25))
        time.sleep(0.3 if i % 2 == 0 else 1.0)              # ghost text computed (fast / slow)
        if i == 0:
            ctx.shot("ghost_visible")
        ctx.be.key("Right")
        time.sleep(0.02)
        ctx.be.key("Return")
        exp.append("echo %s >> gh.txt" % t)
        time.sleep(0.5)
    ctx.until(lambda: len(ctx.cmds_since(mark)) >= len(exp), 15)
    got = ctx.cmds_since(mark)
    r.check(got == exp, "ghost-text accept (Right) + Enter: shell received %r, expected %r" % (got, exp))
    r.check(ctx.lines("gh.txt") == toks + toks, "gh.txt has %d lines, expected the %d tokens twice" % (len(ctx.lines("gh.txt")), len(toks)))
    t = toks[0]                                              # End as a second probe: records, does not fail (End may be 'cursor to end' only)
    mark = ctx.cmd_mark()
    ctx.be.type("echo %s >> gh" % t, 15)
    time.sleep(0.5)
    ctx.be.key("End")
    time.sleep(0.02)
    ctx.be.key("Return")
    ctx.until(lambda: len(ctx.cmds_since(mark)) >= 1, 6)
    got = ctx.cmds_since(mark)
    r.metric("end_key_accepts_ghost", got == ["echo %s >> gh.txt" % t])
    r.note("End probe received %r" % got)
    ctx.reset()


@case("ui_ctrl_r", order=62)
def c_ctrl_r(ctx, r):
    """Ctrl-R command search: find an earlier command by a unique fragment, accept the result, and the shell receives exactly that earlier
    command (the accept either fills the input and the next Enter runs it, or runs it: both are handled; exactly ONE extra run is allowed)."""
    ctx.rm("hr.txt")
    toks = [ctx.tok(7).lower() for _ in range(2)]
    for t in toks:
        r.check(run_cmd(ctx, "echo hrz%s >> hr.txt" % t), "seeding command was not received")
    time.sleep(0.5)
    for t in toks:
        orig = "echo hrz%s >> hr.txt" % t
        mark = ctx.cmd_mark()
        ctx.be.key("ctrl+r")
        time.sleep(0.6)
        ctx.shot("ctrl_r_open")
        ctx.be.type("hrz%s" % t, 15)
        time.sleep(0.7)
        ctx.shot("ctrl_r_query")
        ctx.be.key("Return")                       # accept the top result
        time.sleep(0.8)
        if not ctx.cmds_since(mark):
            ctx.be.key("Return")                   # the accept only filled the input
        ctx.until(lambda: len(ctx.cmds_since(mark)) >= 1, 8)
        time.sleep(0.5)
        got = ctx.cmds_since(mark)
        r.check(got == [orig], "Ctrl-R %r + accept: shell received %r, expected exactly [%r]" % ("hrz" + t, got, orig))
        ctx.reset()
    r.check(ctx.lines("hr.txt") == ["hrz" + t for t in toks] * 2, "hr.txt = %r, expected each token twice in order" % ctx.lines("hr.txt"))
    ctx.shot("ctrl_r_done")


@case("ui_palette", order=63)
def c_palette(ctx, r):
    """command palette: opens (OCR 'Search for a command' when tesseract exists), a nonsense filter shows 'No results', filter 'split pane
    right' + Enter runs the action: a NEW pane with its own shell appears (prompt.log +1, machine-checked), typing in it stays exact
    and the pane is closed again with `exit`. Metric palette_open_ui_items: the app's main-thread items over 4 ms on the open (#1194/#1219)."""
    ui_off = ctx.log_size()
    ctx.be.key(K(ctx, "palette"))
    time.sleep(0.8)
    C.ui_item_metrics(ctx, r, ui_off, "palette_open")
    t = ctx.ocr("palette_open")
    # r18e9 (macOS): the caret overlays the first letter of the placeholder, so OCR read "earch for a command" and the exact-phrase check failed on a palette that WAS
    # open (shots/agent_palette_open.png). Match the placeholder's tail; the palette's own command rows are the second, independent proof it opened.
    cmd_names = ("split pane", "new tab", "new window", "open settings", "keybinding", "zoom", "reload")
    if t is not None:
        flat = " ".join(t.lower().split())
        shown = [n for n in cmd_names if n in flat]
        opened = bool(re.search(r"\w{0,3}earch for a comm|for a command", flat)) or len(shown) >= 3
        r.metric("palette_open_markers", dict(placeholder=bool(re.search(r"for a command", flat)), command_rows=shown))
        r.check(opened, "palette did not open: neither 'Search for a command' (tail) nor >=3 command rows %s on screen (see shots/*palette_open.png)" % (list(cmd_names),))
    else:
        shown = []
    ctx.be.type("zzqqxxnothing", 15)
    time.sleep(0.6)
    t2 = ctx.ocr("palette_noresults")
    if t2 is not None:
        # r18e9: a nonsense filter now shows a "Create zzqqxxnothing..." row instead of "No results found" (shots/agent_palette_noresults.png). The real claim is
        # "the filter matched no COMMAND": none of the command names the unfiltered palette listed may still be on screen. Control: they were listed unfiltered.
        flat2 = " ".join(t2.lower().split())
        left = [n for n in cmd_names if n in flat2]
        r.metric("palette_nonsense_command_rows", left)
        r.check(not left, "nonsense filter still lists command rows %s (see shots/*palette_noresults.png)" % left)
        r.check("no results" in flat2 or "create" in flat2 or "zzqqxx" in flat2, "nonsense filter shows neither 'No results', a 'Create ...' row nor the typed filter: the palette may have lost the keys")
        if t is not None and not shown:
            r.note("control: no command row was readable in the UNFILTERED palette, so 'no command rows after the filter' is weak evidence here (OCR)")
    ctx.be.keys(["ctrl+a", "BackSpace"], delay_ms=30) if not mac(ctx) else ctx.be.keys(["cmd+a", "BackSpace"], delay_ms=30)
    ctx.be.type("split pane right", 15)
    time.sleep(0.8)
    ctx.shot("palette_filtered")
    n0 = len(prompt_times(ctx))
    ctx.be.key("Return")
    new = ctx.until(lambda: len(prompt_times(ctx)) > n0, 12)
    r.check(bool(new), "palette 'split pane right' + Enter: no new shell prompt (no new pane) within 12 s (see shots/*palette_filtered.png)")
    time.sleep(1.0)
    ctx.shot("palette_split_done")
    if new:
        exact_burst(ctx, r, "palette_pane", 4)
        ctx.be.type("exit", 15, enter=True)
        time.sleep(1.0)
    r.check(ctx.app.alive(), "app not alive after the palette action")
    ctx.shot("palette_after_close")


# ---------------------------------------------------------------- (10) mouse, panes, resize
def copy_check(ctx, r, label, do_select, token):
    ctx.be.clip_set("SENTINEL_NOT_COPIED")
    do_select()
    ctx.be.key(K(ctx, "copy"))
    time.sleep(0.5)
    got = ctx.be.clip_get().decode(errors="replace")
    ctx.shot("select_" + label)
    return got


@case("ui_mouse_select_copy", order=70, gui=True)
def c_mouse_copy(ctx, r):
    """mouse selection in command output + the copy key: print a unique token whose text is NOT in the command line, find it on screen
    (tesseract word box), double-click it -> copy -> the PRIVATE clipboard (wl-paste / xsel of this job's own display) holds exactly the
    token; then a drag across the word -> copy contains the token. The clipboard is pre-set to a sentinel so a no-op copy cannot pass."""
    m = mouse_or_skip(ctx, r)
    need_tesseract(r)
    tok = "msel" + ctx.tok(6).lower().replace("_", "")
    run_cmd(ctx, "printf 'msel%%s\\n' %s" % tok[4:])
    time.sleep(0.8)
    w, _ = find_word(ctx, "mouse_find", tok[:9])
    if not w:
        r.check(False, "token %r not found on screen by OCR (shots/*mouse_find.png): cannot place the pointer" % tok)
        return
    r.metric("word", w[0])
    got = copy_check(ctx, r, "dblclick", lambda: m.click(w[1], w[2], 1, 2), tok)
    r.check(got.strip() == tok, "double-click + copy: clipboard holds %r, expected exactly %r" % (got[:80], tok))
    got = copy_check(ctx, r, "drag", lambda: m.drag(w[3] - 2, w[2], w[4] + 2, w[2]), tok)
    r.check(tok in got and len(got) <= len(tok) + 4, "drag across the word + copy: clipboard holds %r, expected the token %r (+- a char)" % (got[:80], tok))
    exact_burst(ctx, r, "after_mouse", 4)


@case("ui_right_click_paste", modes=("terminal",), order=71, gui=True)
def c_right_click_paste(ctx, r):
    """right-click in the input -> context menu -> Paste (found by OCR) puts the PRIVATE clipboard text in the input; Enter then runs
    exactly it. Terminal mode only: pasting a command into the agent input would send it to the agent."""
    m = mouse_or_skip(ctx, r)
    need_tesseract(r)
    tok = ctx.tok(7).lower()
    cmd = "echo rc%s >> rc.txt" % tok
    ctx.rm("rc.txt")
    ctx.be.clip_set(cmd)
    p, _ = find_word(ctx, "rc_prompt", "w$")
    if not p:
        r.check(False, "prompt '~/w$' not found on screen (shots/*rc_prompt.png)")
        return
    mark = ctx.cmd_mark()
    m.click(p[4] + 40, p[2], 3)
    time.sleep(0.6)
    item, _ = find_word(ctx, "rc_menu", "paste")
    if not item:
        r.check(False, "no 'Paste' entry in the context menu (shots/*rc_menu.png)")
        ctx.be.key("Escape")
        return
    m.click(item[1], item[2], 1)
    time.sleep(0.6)
    ctx.shot("rc_pasted")
    ctx.be.key("Return")
    ctx.until(lambda: len(ctx.cmds_since(mark)) >= 1, 8)
    got = ctx.cmds_since(mark)
    r.check(got == [cmd], "right-click Paste + Enter: shell received %r, expected [%r]" % (got, cmd))
    r.check(ctx.lines("rc.txt") == ["rc" + tok], "rc.txt = %r" % ctx.lines("rc.txt"))


@case("ui_drag_drop_file", modes=("terminal",), order=72, gui=True)
def c_dnd(ctx, r):
    """drag a file (name WITH a space) from a drag source onto the terminal: the dropped path must reach the input quoted/escaped so the
    shell sees ONE argument equal to the real path. Needs a synthesized drag SOURCE, which xdotool/swaymsg cannot be: on X11 only, with
    `dragon-drop` (https://github.com/mwh/dragon, package dragon-drop) installed; everywhere else the case skips."""
    if ctx.be.name != "x11":
        r.skip("%s: no way to synthesize a drag source (XDND needs a client that owns the drag; Wayland wl_data_device likewise)" % ctx.be.name)
    dd = shutil.which("dragon-drop") or shutil.which("dragon")
    if not dd:
        r.skip("no drag source tool on this box (package: dragon-drop / dragon)")
    need_tesseract(r)
    m = mouse_or_skip(ctx, r)
    path = os.path.join(ctx.W, "dnd file %s.txt" % ctx.tok(4).lower())
    open(path, "w").write("x")
    # the target point: just right of the prompt
    p, _ = find_word(ctx, "dnd_prompt", "w$")
    if not p:
        r.check(False, "prompt not found on screen")
        return
    ctx.be.type("echo ", 15)
    src = subprocess.Popen([dd, path], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        time.sleep(1.5)
        ids = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "dragon"], capture_output=True).stdout.decode().split()
        if not ids:
            r.check(False, "dragon window not found")
            return
        g = subprocess.run(["xdotool", "getwindowgeometry", "--shell", ids[-1]], capture_output=True).stdout.decode()
        kv = dict(l.split("=") for l in g.split() if "=" in l)
        sx, sy = int(kv["X"]) + int(kv["WIDTH"]) // 2, int(kv["Y"]) + int(kv["HEIGHT"]) // 2
        mark = ctx.cmd_mark()
        m.drag(sx, sy, p[4] + 60, p[2])
        time.sleep(0.8)
        ctx.shot("dnd_dropped")
        ctx.be.key("Return")
        ctx.until(lambda: len(ctx.cmds_since(mark)) >= 1, 8)
    finally:
        src.terminate()
    got = ctx.cmds_since(mark)
    ok = False
    if got and got[0].startswith("echo "):
        try:
            ok = shlex.split(got[0][5:]) == [path]
        except ValueError:
            ok = False
    r.check(ok, "dropped path: shell received %r, expected `echo` + the quoted path %r as ONE argument" % (got, path))


def get_geometry(ctx):
    if ctx.be.name == "x11":
        ids = subprocess.run(["xdotool", "search", "--onlyvisible", "--class", "phosphor"], capture_output=True).stdout.decode().split()
        if not ids:
            return None
        g = subprocess.run(["xdotool", "getwindowgeometry", "--shell", ids[-1]], capture_output=True).stdout.decode()
        kv = dict(l.split("=") for l in g.split() if "=" in l)
        return ids[-1], int(kv["WIDTH"]), int(kv["HEIGHT"])
    return None


def resize_to(ctx, w, h, orig=None):
    """resize the app window; returns an error string or None"""
    if ctx.be.name == "x11":
        g = get_geometry(ctx)
        if not g:
            return "no phosphor window"
        subprocess.run(["xdotool", "windowsize", g[0], str(w), str(h)], timeout=10)
        return None
    if ctx.be.name == "wayland":
        rc = subprocess.run(["swaymsg", "floating enable, resize set width %d px height %d px" % (w, h)], capture_output=True, timeout=10)
        return None if rc.returncode == 0 else "swaymsg: " + rc.stderr.decode()[:100]
    if mac(ctx):
        pid = ctx.app.pid()
        rc = subprocess.run(["osascript", "-e", 'tell application "System Events" to set size of window 1 of (first process whose unix id is %d) to {%d, %d}' % (pid, w, h)],
                            capture_output=True, timeout=30)
        return None if rc.returncode == 0 else "System Events refused (Accessibility permission?): " + rc.stderr.decode()[:100]
    return "unsupported backend"


@case("ui_resize_typing", order=73, gui=True)
def c_resize(ctx, r):
    """resize the window small -> large -> back; at each size typing is exact (6 fast commands) and, at the small size, a 150-char command
    that must wrap is received byte-exact. The screenshot must change with the size (else the resize did not apply and the case skips)."""
    g0 = get_geometry(ctx)
    base = ctx.shot("resize_base")
    try:
        sizes = [("small", 720, 460), ("large", 1280, 800)]
        for name, w, h in sizes:
            err = resize_to(ctx, w, h)
            if err:
                r.skip("cannot resize the window on %s: %s" % (ctx.be.name, err))
            time.sleep(1.5)
            s = ctx.shot("resize_" + name)
            changed, d = shot_changed(base, s, 0.01)
            if d is not None:
                r.metric("diff_" + name, d)
            if not changed:
                r.skip("screenshot did not change after resizing to %dx%d: the compositor/WM ignored the resize" % (w, h))
            exact_burst(ctx, r, "resize_" + name, 6)
            if name == "small":
                tok = ctx.tok(6).lower()
                long_cmd = "echo %s >> rz_long.txt # %s" % (tok, "x" * 110)
                ctx.rm("rz_long.txt")
                mark = ctx.cmd_mark()
                ctx.be.type(long_cmd, 12, enter=True)
                ctx.until(lambda: len(ctx.cmds_since(mark)) >= 1, 10)
                r.check(ctx.cmds_since(mark) == [long_cmd], "wrapped %d-char command at the small size: shell received %r" % (len(long_cmd), ctx.cmds_since(mark)))
            base = s
    finally:
        if ctx.be.name == "wayland":
            subprocess.run(["swaymsg", "floating disable"], capture_output=True, timeout=10)
        elif g0:
            subprocess.run(["xdotool", "windowsize", g0[0], str(g0[1]), str(g0[2])], timeout=10)
        time.sleep(1.0)
    time.sleep(1.0)
    exact_burst(ctx, r, "resize_back", 4)
    r.check(ctx.app.alive(), "app not alive after resizing")




@case("ui_split_panes", order=74, gui=True)
def c_split(ctx, r):
    """split right (Linux ctrl-shift-D / macOS cmd-D) and split down (ctrl-shift-E / cmd-shift-D): each split gives a NEW shell (prompt.log
    +1); typing in the new pane is exact; the pane-switch keys move focus to the other pane and typing there is exact and lands in that
    pane's shell (one rec.log: all commands in order); `exit` closes the new pane."""
    for dirn in ("split_right", "split_down"):
        n0 = len(prompt_times(ctx))
        ctx.be.key(K(ctx, dirn))
        up = ctx.until(lambda: len(prompt_times(ctx)) > n0, 12)
        if not r.check(bool(up), "%s: no new shell prompt within 12 s (no new pane?)" % dirn):
            ctx.shot("split_fail_" + dirn)
            continue
        time.sleep(1.0)
        ctx.shot("split_" + dirn)
        exact_burst(ctx, r, "%s_new" % dirn, 5)                      # focus is in the new pane
        ctx.be.key(K(ctx, "prev_pane"))
        time.sleep(0.5)
        exact_burst(ctx, r, "%s_old" % dirn, 5)                      # focus moved to the original pane
        ctx.be.key(K(ctx, "next_pane"))
        time.sleep(0.5)
        exact_burst(ctx, r, "%s_new2" % dirn, 3)                     # and back
        ctx.shot("split_focus_back_" + dirn)
        ctx.be.type("exit", 15, enter=True)                          # closes the focused (new) pane
        time.sleep(1.0)
        ctx.shot("split_closed_" + dirn)
        exact_burst(ctx, r, "%s_after_close" % dirn, 3)
    r.check(ctx.app.alive(), "app not alive after the pane cases")


# ---------------------------------------------------------------- (11) startup time and live settings
def relaunch(ctx, wipe_state):
    """stop the app and start it again; seconds from start() to the first shell prompt the NEW instance logged (prompt.log epoch clock)"""
    ctx.snapshot_log()                                                 # r18e9: the next launch overwrites phosphor.log; accounting spans every launch
    ctx.app.stop()
    if wipe_state and not mac(ctx):                                    # first-run state: nothing under .local/{state,share}
        for sub in (".local/state", ".local/share"):
            shutil.rmtree(os.path.join(ctx.prof.home, sub), ignore_errors=True)
    t0 = time.time()
    ctx.app.start()
    ok = ctx.until(lambda: any(t >= t0 for t in prompt_times(ctx)), 90, step=0.1)
    if not ok:
        return None
    first = min(t for t in prompt_times(ctx) if t >= t0)
    return first - t0


@case("ui_startup_time", order=85, gui=True)
def c_startup(ctx, r):
    """launch -> first shell prompt. 'cold' = relaunch with the app's state dirs wiped (first-run work; the OS page cache stays warm: dropping
    it needs root), 'warm' = three relaunches with state. Typing is exact after each launch. Fails if no prompt in 90 s or warm median
    > APPCI_STARTUP_MAX s (default 20); the numbers are metrics for trend comparison."""
    if ctx.app is None:
        r.skip("sim backend: no app process")
    limit = float(os.environ.get("APPCI_STARTUP_MAX", "20"))
    cold = relaunch(ctx, True)
    r.metric("cold_s", cold if cold is not None else -1)
    if cold is None:
        r.check(False, "cold start: no shell prompt within 90 s")
        return
    time.sleep(3)
    ctx.be.key("Escape")
    exact_burst(ctx, r, "after_cold", 4)
    warm = []
    for i in range(3):
        w = relaunch(ctx, False)
        if w is None:
            r.check(False, "warm start %d: no shell prompt within 90 s" % (i + 1))
            break
        warm.append(w)
        time.sleep(2)
    if warm:
        med = sorted(warm)[len(warm) // 2]
        r.metric("warm_s", C.stats(warm))
        r.check(med <= limit, "warm start median %.1f s exceeds %.0f s (cold %.1f s)" % (med, limit, cold))
        ctx.be.key("Escape")
        exact_burst(ctx, r, "after_warm", 4)
    ctx.shot("startup_done")
    lp = ctx.log_path
    if lp and os.path.exists(lp):                                      # the relaunches overwrote phosphor.log: keep the last launch's
        d = os.path.join(ctx.run, "logs", ctx.mode)
        os.makedirs(d, exist_ok=True)
        shutil.copyfile(lp, os.path.join(d, "phosphor.startup.log"))


def settle_change(ctx, before, tag, thresh, timeout=10):
    """poll screenshots until one differs from `before` by >= thresh; -> (path, diff|None) or (None, last diff)"""
    t = time.time()
    last = None
    i = 0
    while time.time() - t < timeout:
        i += 1
        p = ctx.shot("%s_%d" % (tag, i))
        if p:
            ch, d = shot_changed(before, p, thresh)
            last = d
            if ch:
                time.sleep(0.8)                                       # let the relayout finish
                return ctx.shot(tag), d
        time.sleep(0.6)
    return None, last


@case("ui_settings_live", order=86, gui=True)
def c_settings(ctx, r):
    """settings changes applied LIVE through the settings file (the watcher the settings UI also feeds): theme switch, font size, vertical
    tabs on/off. Each must visibly change the screen (screenshot diff, Pillow when present, else a hash) and typing stays exact after the
    change. The original settings file is restored at the end. The in-app settings page itself is not clicked (no stable pointer path)."""
    p = settings_path(ctx)
    if not os.path.isfile(p):
        r.skip("settings file %s not found" % p)
    orig = open(p, "rb").read()
    steps = [("theme", "appearance.themes", "theme", ['"phosphor_green"', '"tokyo_night"'], 0.10),
             ("font", "appearance.text", "font_size", ["22.0", "11.0"], 0.02),
             ("vtabs", "appearance.vertical_tabs", "enabled", ["false", "true"], 0.004)]
    try:
        for name, table, key, vals, thresh in steps:
            for i, v in enumerate(vals):
                before = ctx.shot("set_%s_%d_before" % (name, i))
                set_setting(ctx, table, key, v)
                after, d = settle_change(ctx, before, "set_%s_%d_after" % (name, i), thresh)
                r.metric("%s_%d_diff" % (name, i), d if d is not None else -1)
                if not r.check(after is not None, "%s = %s: the screen did not change (diff %s < %s) within 10 s of the file write (live reload failed?)" % (name, v, d, thresh)):
                    continue
                exact_burst(ctx, r, "set_%s_%d" % (name, i), 4)
        if not C.shutil.which("tesseract"):
            r.note("no OCR engine: font size check is screenshot-diff only")
    finally:
        open(p, "wb").write(orig)
        time.sleep(1.5)
        exact_burst(ctx, r, "set_restored", 3)
        ctx.shot("set_restored")
