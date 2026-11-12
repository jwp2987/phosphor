# appci/cases_ui.py: real-app cases for completions/history (Tab menu, ghost text, Ctrl-R, command palette), the mouse
# (select + copy, right-click Paste, drag-and-drop, split panes, focus switching, resize) and startup time / live settings
# changes (issue #1462). Same rules as cases.py: the EXACT command the shell received (rec.log) plus an effect file, never
# "something ran"; typing at 10-25 ms/key with instant Enter; a case that cannot run SKIPS with the precise reason and the
# package it needs. Registered by the one import line at the end of cases.py. Every case here is full-tier.
#
# Platform notes (docs/testing.md 2.8): keys follow app/src/util/bindings.rs (Linux ctrl-shift-X, macOS cmd-X); the mouse is
# xdotool on X11 and `swaymsg seat - cursor ...` on the headless-sway Wayland job (no extra package); on macOS it is mac/tools/mptr
# (CGEvent, macptr.MacMouse, #2061) for the cases that ask for it with mouse_or_skip(..., mac=True). Positions come from the app's find
# hook, else from word boxes of a screenshot (tesseract, package tesseract-ocr; Apple Vision through mac/tools/ocrw on macOS).
# Windows (r15 wave 4, #2060): the mouse is SendInput (winapi.Pointer through backend.WindowsBackend.ptr_*), the window size is SetWindowPos, and
# positions come ONLY from the app's find hook (`appci_find` lines; the VM has no OCR engine): a word the hook did not report is UNVERIFIABLE there.
import hashlib, json, os, re, shlex, shutil, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases as C  # noqa: E402  (cases.py imports this module as its LAST line, so everything below is already defined)
import toml_text as T  # noqa: E402
import wlptr  # noqa: E402  (the Wayland virtual pointer, #1789)
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


PILLOW_MSG = "Pillow not installed on this runner (owner: install?)"


def have_pillow():
    try:
        import PIL.Image, PIL.ImageChops  # noqa: F401
        return True
    except ImportError:
        return False


def need_pillow(r):
    """r18u8 (Mac): a case whose verdict is a screenshot diff must SKIP, not pass on a note, when Pillow is missing"""
    if not have_pillow():
        r.skip(PILLOW_MSG)


def img_diff(a, b, color=False):
    """fraction of pixels that differ noticeably between two screenshots; None without Pillow (python3-pil). Default: the grey level of the
    per-pixel difference above 24. color=True: the largest of the three channel differences above 8 (COLOR_DELTA), for a change of COLOUR
    (a theme) or of anti-aliased glyphs (a font size). r18u19 (Mac): a dark theme swapped for another dark theme recoloured the whole window
    and the grey-level test saw 0.3 % of it (a background (30,32,46) -> (10,25,15) differs by ~14 grey levels); the channel test saw 18-67 %
    with two identical screenshots at 0.03 %."""
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
    diff = ImageChops.difference(A, B)
    if color:
        r, g, bl = diff.split()
        d = ImageChops.lighter(ImageChops.lighter(r, g), bl).point(lambda v: 255 if v > COLOR_DELTA else 0)
    else:
        d = diff.convert("L").point(lambda v: 255 if v > 24 else 0)
    hist = d.histogram()
    return hist[255] / float(A.size[0] * A.size[1])


COLOR_DELTA = 8


def sha(path):
    try:
        return hashlib.sha256(open(path, "rb").read()).hexdigest()
    except OSError:
        return None


def shot_changed(a, b, thresh, color=False):
    d = img_diff(a, b, color)
    if d is None:
        return (sha(a) != sha(b)), None
    return d >= thresh, d


def is_windows(ctx):
    return getattr(getattr(ctx, "be", None), "name", None) == "windows"


def need_tesseract(r, ctx=None):
    """SKIP without a word-box reader for screen positions: tesseract, or on macOS (a case that passes its ctx, #2061) Apple Vision through
    mac/tools/ocrw. With `ctx` given, not on Windows: a case ported there places the pointer from the app's find hook first, and a word the hook
    does not report is looked for by Windows.Media.Ocr (winocr, #2060) when the VM has an engine, else that part is UNVERIFIABLE (wl_check), never a pass."""
    if ctx is not None and is_windows(ctx):
        return
    if ctx is not None and mac(ctx):
        import macptr
        if not macptr.tool(ctx.be, "ocrw") and not shutil.which("tesseract"):
            r.skip(macptr.missing_tool_reason(ctx.be, "ocrw"))
        return
    if not shutil.which("tesseract"):
        r.skip("tesseract not installed on this machine (package: tesseract-ocr): screen positions/text cannot be read")


def have_word_ocr(ctx):
    """True when words() can read this platform's screen: tesseract, or Apple Vision word boxes on macOS"""
    if is_windows(ctx):
        import winocr
        return winocr.available()
    if shutil.which("tesseract"):
        return True
    if mac(ctx):
        import macptr
        return bool(macptr.tool(ctx.be, "ocrw"))
    return False


def words(ctx, tag):
    """[(text, cx, cy, left, right, top, bottom)] of every word tesseract finds in a fresh screenshot, in screenshot pixels. On Windows: the words
    Windows.Media.Ocr finds (winocr), in SCREEN pixels (the shot's origin added). No words (and the shot, as evidence) where there is no OCR engine."""
    p = ctx.shot(tag)
    if not p:
        return [], None
    res = []
    if is_windows(ctx):                                   # #2060: Windows.Media.Ocr (winocr), boxes already in screen pixels; none without an engine
        import winocr
        got = winocr.read(p, ctx.be.shot_origin()) if winocr.available() else None
        return (got[1] if got else res), p
    if mac(ctx):                                          # #2061: Apple Vision word boxes first (the engine the Mac harness reads screens with, cases.ocr_engines)
        import macptr
        res += macptr.words(ctx.be, p)
    if not shutil.which("tesseract"):
        return res, p
    out = subprocess.run(["tesseract", p, "-", "--psm", "11", "tsv"], capture_output=True, timeout=90).stdout.decode(errors="replace")
    res += tsv_words(out)
    up = C.ocr_upscaled(ctx.be.name, p)                   # X11: words of a 2x copy are added, boxes scaled back (cases.ocr_upscaled)
    if up:
        res += tsv_words(subprocess.run(["tesseract", up, "-", "--psm", "11", "tsv"], capture_output=True, timeout=90).stdout.decode(errors="replace"), 2)
    return res, p


def tsv_words(out, scale=1):
    """[(text, cx, cy, left, right, top, bottom)] of tesseract tsv output, in pixels of the original shot (an OCR'd copy upscaled by `scale`)"""
    res = []
    for l in out.splitlines()[1:]:
        c = l.split("\t")
        if len(c) >= 12 and c[11].strip() and c[0] == "5":
            x, y, w, h = (int(v) // scale for v in (c[6], c[7], c[8], c[9]))
            res.append((c[11].strip(), x + w // 2, y + h // 2, x, x + w, y, y + h))
    return res


def find_word(ctx, tag, sub, last=True):
    """the (last = lowest on screen) word whose text contains `sub` (lower-case compare); None if absent"""
    ws, p = words(ctx, tag)
    hits = [w for w in ws if sub.lower() in w[0].lower()]
    hits.sort(key=lambda w: w[2])
    return ((hits[-1] if last else hits[0]) if hits else None), p


# r18u22 (Wayland): ui_mouse_select_copy failed in three runs on the SAME token ('mselt409jy2dwo', seed 1) with "not found on screen by OCR",
# while the agent pass found its token ('mselt246eectwt') on the same screen: tesseract reads some glyphs of the terminal font as others
# (0/O, 1/l/I, 5/S, 9/g, ...), and the exact-substring match then never hits. Tokens a case must FIND on screen are drawn from glyphs
# tesseract does not confuse, and the match tolerates one misread glyph.
OCR_SAFE = "acdefhkmnprtuwxy2347"


def ocr_safe(s):
    """s mapped onto OCR_SAFE glyphs, one for one (deterministic: the seeded rng sequence of the run is unchanged)"""
    return "".join(OCR_SAFE[ord(c) % len(OCR_SAFE)] for c in s)


def find_token(ctx, tag, token, last=True, since=None, r=None):
    """(word, path) like find_word, for a whole token: an exact substring hit first, else the word that differs from the token by at most one glyph
    (a misread or a dropped/added glyph at either end). OCR noise around the word (a quote, a colon) is stripped before comparing.
    The app's find hook (#1789) is asked first for a token the case registered (register_find); with `r` given, a missing hook line is noted on it."""
    h = _hook_word(ctx, token, since)
    if not h and is_windows(ctx):
        h = ctx.until(lambda: _hook_word(ctx, token, since), 3.0, step=0.1)      # the hook is the only source there: its line is waited for (a give-up net)
    if h:
        return h, None
    if r is not None and getattr(ctx.be, "name", None) in ("wayland", "x11", "mac"):
        hook_missing_note(r, token)
    ws, p = words(ctx, tag)
    hits = token_hits(ws, token)
    return ((hits[-1] if last else hits[0]) if hits else None), p


def child_map(proc="/proc"):
    """{ppid: [pid, ...]} of every process the kernel lists under `proc` (Linux); {} where that cannot be read. The parent is the field after the
    state in /proc/<pid>/stat, read behind the LAST `)` (a command name may hold spaces and parentheses)."""
    out = {}
    try:
        names = os.listdir(proc)
    except OSError:
        return out
    for n in names:
        if not n.isdigit():
            continue
        try:
            with open(os.path.join(proc, n, "stat"), errors="replace") as f:
                ppid = int(f.read().rsplit(")", 1)[1].split()[1])
        except (OSError, ValueError, IndexError):
            continue
        out.setdefault(ppid, []).append(int(n))
    return out


def descendant_pids(pid, children=None):
    """pure given `children` (child_map()): the descendants of `pid`, nearest generation first, `pid` itself not included. A launch through a
    wrapper (`exec strace -f ... <app>`: link_and_path_click, zero_network_startup) leaves the WRAPPER's PID in app.pid; the window belongs to its
    child."""
    children = child_map() if children is None else children
    out, level, seen = [], [pid], {pid}
    while level:
        nxt = []
        for p in level:
            for c in sorted(children.get(p, ())):
                if c not in seen:
                    seen.add(c)
                    out.append(c)
                    nxt.append(c)
        level = nxt
    return out


def app_window_origin(pid, by_pid, children=None):
    """the window origin of the app launched as `pid`: by_pid(pid) when that process owns a window, else the first descendant's that does (#2141:
    under a wrapper the launcher's PID is the wrapper's, no window carries it, and the find hook then gave no position at all: every word of
    link_and_path_click was located by OCR, and a block that moved after the read was clicked at its old place). by_pid: pid -> (x, y) or None."""
    o = by_pid(pid)
    if o:
        return o
    for c in descendant_pids(pid, children):
        o = by_pid(c)
        if o:
            return o
    return None


def x11_window_origin(pid):
    """(x, y) of the app's X11 window on screen, from xdotool (no window manager under the job's Xvfb, so the window origin is the content origin), or None"""
    try:
        ids = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(pid)], capture_output=True, timeout=10).stdout.decode().split()
        if not ids:
            return None
        g = subprocess.run(["xdotool", "getwindowgeometry", "--shell", ids[-1]], capture_output=True, timeout=10).stdout.decode()
        kv = dict(l.split("=") for l in g.split() if "=" in l)
        return int(kv["X"]), int(kv["Y"])
    except Exception:
        return None


def _hook_word(ctx, token, since=None, all_hits=False):
    """the OCR-free position (hook_find) of `token` on Wayland (window origin from sway), X11 (from xdotool), macOS (mac_window_origin, #2061) and
    Windows (ClientToScreen of the app window: the hook's pixels are the client area's); None without the app hook line (an app without the hook,
    or a token the hook never saw drawn): the caller falls back to OCR and says so (hook_missing_note), or on Windows reports the part as not judged.
    `all_hits`: every sighting, oldest first (hook_find), [] in place of None."""
    none = [] if all_hits else None
    if getattr(ctx.be, "name", None) not in ("wayland", "x11", "mac", "windows"):
        return none
    if os.environ.get("APPCI_NO_FIND_HOOK") == "1":       # kill switch: positions by OCR only, as every Linux run before round 14 was (the launcher never armed the hook)
        return none
    pid = ctx.be.pid_fn() if getattr(ctx.be, "pid_fn", None) else None
    if not pid:
        return none
    if is_windows(ctx):
        origin = ctx.be.client_origin()
    elif mac(ctx):
        origin = mac_window_origin(ctx)
    elif on_wayland(ctx):
        origin = app_window_origin(pid, sway_window_origin)
    else:
        origin = app_window_origin(pid, x11_window_origin)
    return hook_find(ctx, token, origin=origin, since=since, all_hits=all_hits) if origin else none


def hook_missing_note(r, token):
    """the hook path found no `appci_find` line for `token`: the case continues with OCR (the old method) and the report says so"""
    r.note("app find hook gave no position for %r (no `appci_find` line in the app log: an app without the hook, or RUST_LOG without warpui::appci_find=info); located by OCR instead" % token)


def token_hits(ws, token):
    """the words of `ws` (words() rows) that are `token`, top to bottom: exact substring hits if any, else the one-glyph-off ones (find_token)"""
    tl = token.lower()
    exact = sorted((w for w in ws if tl in w[0].lower()), key=lambda w: w[2])
    if exact:
        return exact

    def near(word):
        w = re.sub(r"[^0-9a-z]", "", word.lower())
        if abs(len(w) - len(tl)) > 1 or len(tl) < 6:
            return False
        if len(w) == len(tl):
            return sum(a != b for a, b in zip(w, tl)) <= 1
        short, long_ = (w, tl) if len(w) < len(tl) else (tl, w)
        return any(long_[:i] + long_[i + 1:] == short for i in range(len(long_)))
    return sorted((w for w in ws if near(w[0])), key=lambda w: w[2])


class Mouse:
    """pointer synthesis in the job's own display: xdotool (X11), or on headless Wayland the session's persistent zwlr_virtual_pointer_v1 holder
    (wlptr.py, #1789: sway has no input device, so without a virtual pointer the seat has no pointer capability and no client ever sees `swaymsg
    seat - cursor` events). Without the holder (APPCI_WL_PTR unset or dead) Wayland falls back to the sway seat commands, which are NOT proof of delivery
    (see wl_pointer_real). Windows (r15 wave 4): SendInput through the backend (ptr_move / ptr_button), every move read back from the cursor, a press
    refused unless the window under the pointer is the app's; the same steps and waits as the Wayland arm."""

    def __init__(self, be):
        self.be = be
        self._out = None

    def _wl_extent(self):
        """(w, h) of the headless output in layout pixels (cached)"""
        if self._out is None:
            try:
                o = json.loads(subprocess.run(["swaymsg", "-t", "get_outputs", "-r"], capture_output=True, timeout=10).stdout)
                rect = next(x["rect"] for x in o if x.get("active", True))
                self._out = (int(rect["width"]), int(rect["height"]))
            except Exception:
                self._out = (1600, 1000)
        return self._out

    def _wl(self, line):
        """one command to the holder; False when there is none (the caller falls back to swaymsg)"""
        ctl = wl_pointer_ctl()
        if not ctl:
            return False
        r = wlptr.send_cmd(ctl, line)
        if r != "ok":
            raise RuntimeError("virtual pointer: %s (command %r)" % (r, line))
        return True

    def _move(self, x, y):
        w, h = self._wl_extent()
        return self._wl("abs %d %d %d %d" % (max(0, min(int(x), w - 1)), max(0, min(int(y), h - 1)), w, h))

    def move(self, x, y):
        if self.be.name == "windows":
            self.be.ptr_move(x, y)
        elif self.be.name == "x11":
            subprocess.run(["xdotool", "mousemove", str(int(x)), str(int(y))], timeout=10)
        elif not self._move(x, y):
            subprocess.run(["swaymsg", "seat - cursor set %d %d" % (x, y)], capture_output=True, timeout=10)
        time.sleep(0.15)

    def _btn(self, button, down):
        if self.be.name == "windows":
            self.be.ptr_button(button, down)
            return
        code = wlptr.BTN.get(button, wlptr.BTN[1])
        if not self._wl("btn %d %d" % (code, 1 if down else 0)):
            subprocess.run(["swaymsg", "seat - cursor %s button%d" % ("press" if down else "release", button)], capture_output=True, timeout=10)

    def click(self, x, y, button=1, count=1):
        self.move(x, y)
        if self.be.name == "x11":
            subprocess.run(["xdotool", "click", "--repeat", str(count), "--delay", "70", str(button)], timeout=15)
        else:
            for _ in range(count):
                self._btn(button, True)
                time.sleep(0.03)
                self._btn(button, False)
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
        elif self.be.name == "windows":
            self._btn(1, True)
            try:                                       # the button is released whatever a move raises (a point off the screen): never left held
                time.sleep(0.15)
                for i in range(1, 9):
                    self.be.ptr_move(x0 + (x1 - x0) * i / 8, y0 + (y1 - y0) * i / 8)
                    time.sleep(0.03)
            finally:
                self._btn(1, False)
        else:
            self._btn(1, True)
            time.sleep(0.15)
            for i in range(1, 9):
                if not self._move(x0 + (x1 - x0) * i / 8, y0 + (y1 - y0) * i / 8):
                    subprocess.run(["swaymsg", "seat - cursor set %d %d" % (x0 + (x1 - x0) * i / 8, y0 + (y1 - y0) * i / 8)], capture_output=True, timeout=10)
                time.sleep(0.03)
            self._btn(1, False)
        time.sleep(0.3)


def mac_mouse(ctx):
    """the run's one macptr.MacMouse (the display scale is measured once), or None when the mptr tool is not there"""
    import macptr
    if not macptr.tool(ctx.be, "mptr"):
        return None
    m = getattr(ctx.be, "_appci_mouse", None)
    if m is None:
        m = ctx.be._appci_mouse = macptr.MacMouse(ctx.be)
    return m


def mac_window_origin(ctx):
    """(x, y) of the app window's top-left corner in SCREENSHOT pixels on macOS (winrect points times the display scale), or None. The find hook's
    device-pixel coordinates are added to it (macptr.origin_px says why the window's corner is the content's corner)."""
    import macptr
    m = mac_mouse(ctx)
    if m is None or not m.scale:
        return None
    return macptr.origin_px(ctx.be.window_rect(), m.scale)


def wl_pointer_ctl():
    """the control socket of the session's virtual-pointer holder if it answers (wayland_session.sh exports APPCI_WL_PTR), else None"""
    ctl = os.environ.get("APPCI_WL_PTR")
    if not ctl or not os.path.exists(ctl):
        return None
    try:
        return ctl if wlptr.send_cmd(ctl, "ping", timeout=3) == "ok" else None
    except OSError:
        return None


def wl_pointer_real(ctx):
    """True when pointer events provably reach Wayland clients: a live virtual-pointer holder (wlptr.py). Then a missing pointer effect is an APP failure, not NOREF."""
    return on_wayland(ctx) and wl_pointer_ctl() is not None


def sway_window_rect(pid):
    """(x, y, w, h) of the sway window of `pid` in layout pixels, or None. Exact, no OCR: the content rectangle sway reports."""
    try:
        tree = json.loads(subprocess.run(["swaymsg", "-t", "get_tree"], capture_output=True, timeout=10).stdout)
    except Exception:
        return None
    best = []

    def walk(n):
        if n.get("pid") == pid and not n.get("nodes") and n.get("rect"):
            best.append(n)
        for c in n.get("nodes", []) + n.get("floating_nodes", []):
            walk(c)
    walk(tree)
    if not best:
        return None
    rc = best[0]["rect"]
    return rc["x"], rc["y"], rc["width"], rc["height"]


def sway_window_origin(pid):
    """(x, y) of the sway window of `pid`, or None"""
    rect = sway_window_rect(pid)
    return (rect[0], rect[1]) if rect else None


FIND_RE = re.compile(r"appci_find token=(\S+) x0=(-?[\d.]+) y0=(-?[\d.]+) x1=(-?[\d.]+) y1=(-?[\d.]+)")


def hook_find(ctx, token, origin=(0, 0), scale=1.0, since=None, all_hits=False):
    """OCR-free position of `token` from the app's own log (#1789): the env-gated hook logs `appci_find token=T x0= y0= x1= y1=` (window-content
    pixels of the drawn text) for every token listed in $PHOSPHOR_APPCI_FIND_FILE. Returns a words() row (text, cx, cy, left, right, top, bottom) in
    screen pixels (origin = the window's top-left on screen, scale = device pixels per logical pixel) or None when the app logged nothing (an app
    without the hook, which is today's app): the caller then falls back to OCR. The LAST line for the token wins (the lowest, newest occurrence).
    `since` (a ctx.log_size() offset) limits the search to lines logged after it, so a stale line of an earlier case cannot be taken.
    `all_hits`: every line for the token as a row, oldest first ([] for none). The hook logs a token again when the place it is drawn at changes,
    and each on-screen copy of it separately, so the list is where the text has been and what else showed the same word (cases_pointer.current_row)."""
    none = [] if all_hits else None
    try:
        if since is not None:
            text = ctx.log_since(since)
        else:
            path = ctx.log_path
            with open(path, "rb") as f:
                f.seek(0, 2)
                f.seek(max(0, f.tell() - (2 << 20)))
                text = f.read().decode(errors="replace")
    except Exception:
        return none
    rows = []
    for hit in FIND_RE.finditer(text):
        if hit.group(1) != token:
            continue
        x0, y0, x1, y1 = (float(hit.group(i)) * scale for i in (2, 3, 4, 5))
        l, t, rgt, b = (int(origin[0] + x0), int(origin[1] + y0), int(origin[0] + x1), int(origin[1] + y1))
        rows.append((token, (l + rgt) // 2, (t + b) // 2, l, rgt, t, b))
    if all_hits:
        return rows
    return rows[-1] if rows else None


def register_find(ctx, *tokens):
    """append tokens to the app's find file ($PHOSPHOR_APPCI_FIND_FILE, set by app.py), so a hook-carrying app logs where it draws them"""
    try:
        with open(os.path.join(ctx.prof.home, "appci_find.txt"), "a", encoding="utf-8") as f:
            f.write("".join(t + "\n" for t in tokens))
    except Exception:
        return
    # the app re-reads the file at most every 200 ms; text laid out before that would carry no hit
    time.sleep(0.35)


WIN_POINTER_NEEDS = ("NEEDS: the test runner on the unlocked, connected interactive console session of the Windows VM, the app window on screen and "
                     "not covered (a locked screen or a disconnected RDP session has no input desktop)")

MAC_NOT_ENABLED = ("macOS: this case's macOS arm is not enabled. The pointer tool exists since round 15 (mac/tools/mptr.swift, #2061), and the cases that "
                   "pass mac=True run with it; this one still drives the pointer or reads the screen in a Linux-only way and has not been ported")


def mouse_or_skip(ctx, r, mac=False):
    """the pointer of this backend, or SKIP with the reason. macOS (#2061, r14-test-gaps T32): only for a case that says mac=True; the pointer is
    macptr.MacMouse over mac/tools/mptr (same methods, same unit: screenshot pixels). Windows (#2060): SendInput, proven first, else NOT JUDGED."""
    if is_windows(ctx):
        # r15 wave 4 (#2060): SendInput. The pointer is proven before the case relies on it (a hover on the app window, read back); a pointer that
        # does not arrive is NOT JUDGED (noref, listed by the trend report), never a skip that reads as "does not apply" and never a pass.
        why = ctx.be.ptr_ready()
        if why:
            raise C.HostMissing("TEST CAPABILITY MISSING: the Windows pointer (SendInput) did not reach the app window, so this case was not judged: %s. %s"
                                % (why, WIN_POINTER_NEEDS))
        return Mouse(ctx.be)
    if ctx.be.name == "mac":
        if not mac:
            r.skip(MAC_NOT_ENABLED)
        import macptr
        m = mac_mouse(ctx)
        if m is None:
            r.skip(macptr.missing_tool_reason(ctx.be, "mptr"))
        if not m.scale:
            raise RuntimeError("macOS pointer: the display scale could not be measured (%s): screenshot pixels cannot be turned into pointer positions" % m.scale_how)
        return m
    if ctx.be.name not in ("x11", "wayland"):
        r.skip("backend %s has no pointer synthesis" % ctx.be.name)
    if ctx.be.name == "x11" and not shutil.which("xdotool"):
        r.skip("xdotool missing")
    if ctx.be.name == "wayland" and not shutil.which("swaymsg"):
        r.skip("swaymsg missing")
    return Mouse(ctx.be)


WAYLAND_NOREF = ("Wayland harness cannot synthesize pointer input / read this text by OCR reliably; feature verified manually by the owner on a Wayland desktop "
                 "(2026-11-05); covered on X11/Mac")


def on_wayland(ctx):
    """True only for the Wayland backend (headless sway). X11 and xwayland run the X11 backend (be.name 'x11') and are judged in full."""
    return getattr(ctx.be, "name", None) == "wayland"


def wl_check(ctx, r, ok, msg, what):
    """r.check, except that on the Wayland backend a failure of a pointer/OCR-dependent claim is NOREF (reason stated), never a pass and never a fail.
    `what` names the harness limitation (e.g. 'OCR', 'pointer'). Checks that do not depend on pointer synthesis or OCR use r.check directly.
    #1789: a claim that is ONLY about the pointer (`what` == 'pointer') is a plain check whenever the session holds a live virtual pointer (wl_pointer_real):
    the events provably reach clients, so a missing effect is the app's. 'OCR' and mixed ('pointer/OCR') claims stay NOREF until the app hook gives positions."""
    if not ok and what == "OCR" and is_windows(ctx):
        # Windows: the app's find hook first, Windows.Media.Ocr (winocr, #2060) as the fallback where the VM has an engine. A claim neither could place is
        # missing evidence (an app without the hook, RUST_LOG without warpui::appci_find=info, a text laid out before its token was registered, or an
        # OCR read that missed the word, which is as noisy as tesseract's): not judged, like the Wayland NOREF.
        r.unverifiable("%s [Windows: positions come from the app's find hook (an `appci_find` line), else Windows.Media.Ocr (%s); neither placed it, so this part was not judged]" % (
            msg, "an engine was available" if have_word_ocr(ctx) else "no OCR engine on this VM"))
        return False
    if ok or not on_wayland(ctx) or (what == "pointer" and wl_pointer_real(ctx)):
        return r.check(ok, msg)
    r.no_reference("%s [%s: %s]" % (WAYLAND_NOREF, what, msg))
    return False


def settings_path(ctx):
    """the settings file THE APP reads (and watches). Linux: the profile's own file. macOS: MacApp.settings. Windows (r15 wave 4, #2060):
    %LOCALAPPDATA%\\phosphor\\Phosphor\\config\\settings.toml (WindowsApp.config), where start() installs the profile's file; until then this
    returned the profile's source file there, which the running app never reads, so a live rewrite changed nothing."""
    if getattr(ctx, "win", False) and getattr(ctx.app, "config", None):
        return os.path.join(ctx.app.config, "settings.toml")
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


PALETTE_FRAME_RE = re.compile(r'frame \d+: updated=\[[^\]]*"CommandPaletteView"')


def palette_rendered(text):
    """True if an app-log excerpt has a frame that rendered CommandPaletteView; None if it has no `frame N: updated=[...]` line at all (a log level
    without frame lines: no evidence either way); else False"""
    if PALETTE_FRAME_RE.search(text):
        return True
    return False if re.search(r"frame \d+: updated=\[", text) else None


@case("ui_palette", order=63)
def c_palette(ctx, r):
    """command palette: opens (OCR 'Search for a command' when tesseract exists), a nonsense filter shows 'No results', filter 'split pane
    right' + Enter runs the action: a NEW pane with its own shell appears (prompt.log +1, machine-checked), typing in it stays exact
    and the pane is closed again with `exit`. Metrics palette_first_open_ui_items / palette_reopen_ui_items: the app's main-thread items over
    4 ms on a cold open and on a reopen after Escape (#1194/#1219). A before/after comparison only: it does not prove the reopen is served from the
    layout cache's retired generation, it shows whether the reopen got cheaper between builds."""
    ui_off = ctx.log_size()
    ctx.be.key(K(ctx, "palette"))
    time.sleep(0.8)
    C.ui_item_metrics(ctx, r, ui_off, "palette_first_open")
    ctx.be.key("Escape")
    time.sleep(0.6)
    ui_off = ctx.log_size()
    ctx.be.key(K(ctx, "palette"))
    time.sleep(0.8)
    C.ui_item_metrics(ctx, r, ui_off, "palette_reopen")
    t = ctx.ocr("palette_open")
    # r18e9 (macOS): the caret overlays the first letter of the placeholder, so OCR read "earch for a command" and the exact-phrase check failed on a palette that WAS
    # open (shots/agent_palette_open.png). Match the placeholder's tail; the palette's own command rows are the second, independent proof it opened.
    cmd_names = ("split pane", "new tab", "new window", "open settings", "keybinding", "zoom", "reload")
    # r18u13 (Mac agent pass): the palette WAS open (shots/agent_palette_open.png: placeholder, filter chips, Suggested rows) but the dim placeholder
    # did not OCR and the unfiltered palette lists filter chips, not commands. Two more independent proofs: >= 3 of the palette's own filter chips
    # (white text, OCRs well), and the app log: a frame that rendered `CommandPaletteView` after the reopen key (the view is drawn only while open).
    chip_names = ("workflows", "prompts", "notebooks", "environment variables", "launch configurations", "conversations")
    rendered = palette_rendered(ctx.log_since(ui_off))
    if t is not None:
        flat = " ".join(t.lower().split())
        shown = [n for n in cmd_names if n in flat]
        chips = [n for n in chip_names if n in flat]
        opened = bool(re.search(r"\w{0,3}earch for a comm|for a command", flat)) or len(shown) >= 3 or len(chips) >= 3 or rendered
        r.metric("palette_open_markers", dict(placeholder=bool(re.search(r"for a command", flat)), command_rows=shown, filter_chips=chips, log_rendered=rendered))
        # #1789: the app log is OCR-free evidence. A log that HAS frame lines but none rendering the palette is a real failure on Wayland too.
        msg = ("palette did not open: neither 'Search for a command' (tail), >=3 command rows %s, >=3 filter chips %s on screen, nor a "
               "CommandPaletteView frame in the app log (see shots/*palette_open.png)" % (list(cmd_names), list(chip_names)))
        if rendered is False:
            r.check(opened, msg)
        else:
            wl_check(ctx, r, opened, msg, "OCR")
    else:
        shown = []
        r.metric("palette_open_markers", dict(log_rendered=rendered))
        r.check(rendered is not False, "palette did not open: no frame rendered CommandPaletteView after the palette key (app log)")
    ctx.be.type("zzqqxxnothing", 15)
    time.sleep(0.6)
    t2 = ctx.ocr("palette_noresults")
    if t2 is not None:
        # r18e9: a nonsense filter now shows a "Create zzqqxxnothing..." row instead of "No results found" (shots/agent_palette_noresults.png). The real claim is
        # "the filter matched no COMMAND": none of the command names the unfiltered palette listed may still be on screen. Control: they were listed unfiltered.
        flat2 = " ".join(t2.lower().split())
        left = [n for n in cmd_names if n in flat2]
        r.metric("palette_nonsense_command_rows", left)
        wl_check(ctx, r, not left, "nonsense filter still lists command rows %s (see shots/*palette_noresults.png)" % left, "OCR")
        wl_check(ctx, r, "no results" in flat2 or "create" in flat2 or "zzqqxx" in flat2, "nonsense filter shows neither 'No results', a 'Create ...' row nor the typed filter: the palette may have lost the keys", "OCR")
        if t is not None and not shown:
            r.note("control: no command row was readable in the UNFILTERED palette, so 'no command rows after the filter' is weak evidence here (OCR)")
    # empty the filter: End + BackSpaces, not select-all + BackSpace (since round 16, #2079, Ctrl-A is the start of the line in a text field off
    # macOS; End and BackSpace mean the same on every build and platform)
    ctx.be.key("End")
    ctx.be.key("BackSpace", repeat=48, delay_ms=12)
    time.sleep(0.2)
    ctx.be.type("split pane right", 15)
    time.sleep(0.8)
    ctx.shot("palette_filtered")
    n0, base = len(prompt_times(ctx)), ctx.shell_count()
    ctx.be.key("Return")
    new = ctx.until(lambda: len(prompt_times(ctx)) > n0, 12)
    r.check(bool(new), "palette 'split pane right' + Enter: no new shell prompt (no new pane) within 12 s (see shots/*palette_filtered.png)")
    time.sleep(1.0)
    ctx.shot("palette_split_done")
    if new:
        exact_burst(ctx, r, "palette_pane", 4)
        ctx.exit_nested(base, r, "palette pane")            # closes the new pane only when a probe proves its shell is in front (audit #2)
        time.sleep(0.5)
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
    m = mouse_or_skip(ctx, r, mac=True)
    need_tesseract(r, ctx)
    tok = "msel" + ocr_safe(ctx.tok(6).lower().replace("_", ""))
    register_find(ctx, tok)
    seen = ctx.log_size()
    run_cmd(ctx, "printf 'msel%%s\\n' %s" % tok[4:])
    time.sleep(0.8)
    w, _ = find_token(ctx, "mouse_find", tok, since=seen, r=r)
    if not w:
        wl_check(ctx, r, False, "token %r not found on screen by OCR (shots/*mouse_find.png): cannot place the pointer" % tok, "OCR")
        if on_wayland(ctx):
            exact_burst(ctx, r, "after_mouse", 4)
        return
    r.metric("word", w[0])
    # r18u22 (Wayland agent pass): both copies left the sentinel. Pointer synthesis on Wayland (sway `seat - cursor`) had never got this far
    # (the terminal pass always stopped at the OCR step), so whether the clicks reached the app is unproven. The screen before and after the
    # double-click is compared: no change at all means no selection was drawn (pointer events not delivered, or the app drew none).
    before = ctx.shot("select_before")
    got = copy_check(ctx, r, "dblclick", lambda: m.click(w[1], w[2], 1, 2), tok)
    d = img_diff(before, ctx.shot("select_dblclick_after")) if before else None
    r.metric("dblclick_screen_diff", d if d is not None else -1)
    # Wayland: a copy that left the sentinel means the pointer events never reached the app (unobservable here): NOREF. Any OTHER wrong clipboard is a real failure.
    ptr = lambda g: on_wayland(ctx) and not wl_pointer_real(ctx) and g == "SENTINEL_NOT_COPIED"
    msg1 = "double-click + copy: clipboard holds %r, expected exactly %r" % (got[:80], tok)
    ok1 = wl_check(ctx, r, False, msg1, "pointer") if ptr(got) else r.check(got.strip() == tok, msg1)
    got = copy_check(ctx, r, "drag", lambda: m.drag(w[3] - 2, w[2], w[4] + 2, w[2]), tok)
    msg2 = "drag across the word + copy: clipboard holds %r, expected the token %r (+- a char)" % (got[:80], tok)
    ok2 = wl_check(ctx, r, False, msg2, "pointer") if ptr(got) else r.check(tok in got and len(got) <= len(tok) + 4, msg2)
    if not (ok1 and ok2) and got == "SENTINEL_NOT_COPIED":
        r.note("the clipboard kept the sentinel: nothing was copied. Screen change from the double-click: %s (0 = no selection drawn: the pointer "
               "events did not reach the app, or it drew no selection; > 0 = a selection was drawn and the copy key did not copy it)" % (
                   "n/a (no Pillow)" if d is None else "%.4f" % d))
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
    # r18u22 (Wayland, every run): the input was located by the prompt text '~/w$', which a Phosphor-drawn prompt (owner settings) does not
    # show, so the case never reached the right-click. The input is located by what is typed INTO it instead: an OCR-safe probe word, found
    # on screen, then cleared with Ctrl-U; the right-click goes where the probe began (inside the now empty input).
    probe = "rcpw" + ocr_safe(ctx.tok(6).lower().replace("_", ""))
    register_find(ctx, probe)
    seen = ctx.log_size()
    ctx.be.type(probe, 12)
    time.sleep(0.6)
    p, _ = find_token(ctx, "rc_prompt", probe, since=seen, r=r)
    ctx.be.key("ctrl+u")
    time.sleep(0.3)
    if not p:
        wl_check(ctx, r, False, "the probe %r typed into the input was not found on screen by OCR (shots/*rc_prompt.png): cannot place the pointer" % probe, "OCR")
        return
    mark = ctx.cmd_mark()
    register_find(ctx, "Paste")
    seen = ctx.log_size()
    m.click(p[3] + 4, p[2], 3)
    time.sleep(0.6)
    item = _hook_word(ctx, "Paste", seen)
    if not item:
        item, _ = find_word(ctx, "rc_menu", "paste")
    if not item:
        wl_check(ctx, r, False, "no 'Paste' entry in the context menu (shots/*rc_menu.png)", "pointer/OCR")
        ctx.be.key("Escape")
        return
    m.click(item[1], item[2], 1)
    time.sleep(0.6)
    ctx.shot("rc_pasted")
    ctx.be.key("Return")
    ctx.until(lambda: len(ctx.cmds_since(mark)) >= 1, 8)
    got = ctx.cmds_since(mark)
    # Wayland: nothing received at all = the click did not paste (pointer unobservable): NOREF; a WRONG command is still a failure.
    if on_wayland(ctx) and not wl_pointer_real(ctx) and not got:
        wl_check(ctx, r, False, "right-click Paste + Enter: shell received nothing, expected [%r]" % cmd, "pointer")
    else:
        r.check(got == [cmd], "right-click Paste + Enter: shell received %r, expected [%r]" % (got, cmd))
        r.check(ctx.lines("rc.txt") == ["rc" + tok], "rc.txt = %r" % ctx.lines("rc.txt"))


@case("ui_drag_drop_file", modes=("terminal",), order=72, gui=True)
def c_dnd(ctx, r):
    """drag a file (name WITH a space) from a drag source onto the terminal: the dropped path must reach the input quoted/escaped so the
    shell sees ONE argument equal to the real path. Needs a synthesized drag SOURCE, which xdotool/swaymsg cannot be: on X11 only, with
    `dragon-drop` (https://github.com/mwh/dragon, package dragon-drop) installed; everywhere else the case skips."""
    if ctx.be.name != "x11":
        r.skip("%s: the harness has no drag source for this display system. NEEDS: a harness capability that is not built (a client that owns the drag: wl_data_device on "
               "Wayland, a pasteboard drag on macOS, OLE drag on Windows; r14-test-gaps T38). Only the X11 arm exists, and it needs the dragon-drop package" % ctx.be.name)
    dd = shutil.which("dragon-drop") or shutil.which("dragon")
    if not dd:
        r.skip("no drag source tool on this box. NEEDS: the package dragon-drop (binary `dragon-drop` or `dragon`) installed on the Linux test machine (owner: machine "
               "change). Has never run on any machine")
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
    if is_windows(ctx):
        return ctx.be.window_resize(w, h)              # SetWindowPos, read back (r15 wave 4, #2060)
    return "unsupported backend"


@case("ui_resize_typing", order=73, gui=True)
def c_resize(ctx, r):
    """resize the window small -> large -> back; at each size typing is exact (6 fast commands) and, at the small size, a 150-char command
    that must wrap is received byte-exact. The screenshot must change with the size (else the resize did not apply and the case skips)."""
    g0 = get_geometry(ctx)
    win0 = ctx.be.window_save() if is_windows(ctx) else None
    base = ctx.shot("resize_base")
    try:
        sizes = [("small", 720, 460), ("large", 1280, 800)]
        for name, w, h in sizes:
            err = resize_to(ctx, w, h)
            if err and is_windows(ctx):
                # Windows has the capability (SetWindowPos, read back): a resize that did not take is NOT JUDGED (noref), never a plain skip
                raise C.HostMissing("TEST CAPABILITY MISSING: the app window did not take the size %dx%d, so this case was not judged: %s" % (w, h, err))
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
        elif win0:
            ctx.be.window_restore(win0)
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
        n0, base = len(prompt_times(ctx)), ctx.shell_count()
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
        ctx.exit_nested(base, r, dirn)                               # closes the focused (new) pane, only when a probe proves it is not the root (audit #2)
        time.sleep(0.5)
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


# ---------------------------------------------------------------- (11b) startup with no usable keyring (#1805)
KEYRING_SLOW_RX = re.compile(r"slow model update: (\d+) ms \(([^)]*)\)")
# models that read secure storage: a slow update of one of them is the keyring on the UI thread
KEYRING_MODELS = ("ProxyCredentials", "AgentProviderSecrets", "ApiKeyManager", "KeyringNotice", "LocalControl", "TemplatableMCPServerManager")
KEYRING_NOTICE_LINE = "Showing the keyring-unavailable notice"          # app/src/keyring_notice.rs
KEYRING_ACQUIRE_LINE = "Failed to acquire default Secret Service collection"


def keyring_launch_facts(text):
    """#1805: what ONE launch's app log says about secure storage. first_frame_ms = the `startup timing: ... total=Nms` line (process start ->
    first frame, app/src/lib.rs on_first_frame_drawn; None if the frame was never drawn), the keyring notice count, the 'Failed to acquire'
    count, secure-storage worker give-ups, and every slow model update of a secure-storage model as (ms, model)."""
    m = re.search(r"startup timing: .*\btotal=(\d+)ms", text)
    slow = [(int(ms), who) for ms, who in KEYRING_SLOW_RX.findall(text) if any(k in who for k in KEYRING_MODELS)]
    return dict(first_frame_ms=int(m.group(1)) if m else None, notices=text.count(KEYRING_NOTICE_LINE),
                acquire_errors=text.count(KEYRING_ACQUIRE_LINE), gave_up=text.count("abandoning the worker"), slow_secure=slow)


def keyring_launch_verdicts(facts, variant, first_frame_max_ms):
    """-> list of failure messages for one launch (empty = pass). With no keyring: first frame within the bound, exactly one notice, at most
    one 'Failed to acquire' (one attempt per run), no worker give-up for "absent" (it fails at once), no secure-storage model update >= 100 ms
    on the UI thread. With the normal unlocked keyring: no notice and at most one 'Failed to acquire' (the issue saw two)."""
    out = []
    if facts["first_frame_ms"] is None:
        out.append("%s: no `startup timing` line: the first frame was never drawn" % variant)
    elif variant != "unlocked" and facts["first_frame_ms"] > first_frame_max_ms:
        out.append("%s: first frame after %d ms (bound %d ms)" % (variant, facts["first_frame_ms"], first_frame_max_ms))
    if variant == "unlocked":
        if facts["notices"]:
            out.append("unlocked: the keyring notice was shown %d time(s) with a working keyring" % facts["notices"])
    elif facts["notices"] != 1:
        out.append("%s: the keyring notice was shown %d times (expected exactly once)" % (variant, facts["notices"]))
    if facts["acquire_errors"] > 1:
        out.append("%s: %d 'Failed to acquire default Secret Service collection' lines (one attempt per run expected)" % (variant, facts["acquire_errors"]))
    if variant == "absent" and facts["gave_up"]:
        out.append("absent: the secure-storage worker gave up %d time(s) on a keyring that fails at once" % facts["gave_up"])
    slow = [(ms, who) for ms, who in facts["slow_secure"] if ms >= 100]
    if slow:
        out.append("%s: secure-storage work on the UI thread: %s" % (variant, "; ".join("%d ms %s" % x for x in slow[:4])))
    return out


def _app_log(ctx):
    try:
        return open(ctx.log_path, errors="replace").read() if ctx.log_path else ""
    except OSError:
        return ""


@case("keyring_unavailable_startup", order=86, gui=True)
def c_keyring_unavailable(ctx, r):
    """#1805: launch with NO usable Secret Service ("absent": nothing owns org.freedesktop.secrets; "hang": a service file whose daemon never
    claims the name, so a call waits out the bus's 120 s activation timeout), then the normal unlocked keyring again. Each launch: the first
    frame is drawn within APPCI_KEYRING_FIRST_FRAME_MAX_MS (default 2000), typing right after the first prompt is exact, the keyring notice is
    shown exactly once (never with a working keyring), the keyring is tried once per run, and no secure-storage model blocks the UI thread.
    Linux only: the variants are D-Bus session configs of the Linux launcher (app.py keyring_dbus_files)."""
    if ctx.app is None or not hasattr(ctx.app, "keyring") or mac(ctx) or C.plat().startswith(("macos", "windows")):
        r.skip("needs the Linux App launcher (its private D-Bus session decides which Secret Service exists)")
    if not shutil.which("dbus-run-session"):
        r.skip("dbus-run-session not installed (package: dbus)")
    limit = int(os.environ.get("APPCI_KEYRING_FIRST_FRAME_MAX_MS", "2000"))
    try:
        for variant in ("absent", "hang", "unlocked"):
            ctx.app.keyring = variant
            t = relaunch(ctx, False)
            r.metric("%s_prompt_s" % variant, t if t is not None else -1)
            if t is None:
                r.check(False, "%s: no shell prompt within 90 s" % variant)
                continue
            ctx.be.key("Escape")                                           # as ui_startup_time, but with no settle wait: typing works at once
            exact_burst(ctx, r, "keyring_%s" % variant, 4)
            if variant != "unlocked":
                # the notice follows the keyring's answer: at once for "absent", after the app's 10 s connect give-up for "hang"
                ctx.until(lambda: KEYRING_NOTICE_LINE in _app_log(ctx), 30, step=0.5)
                ctx.shot("keyring_%s_notice" % variant)
            time.sleep(2)                                                  # a second notice would have to show up in this window
            text = _app_log(ctx)
            facts = keyring_launch_facts(text)
            r.metric("%s_facts" % variant, dict(facts, slow_secure=facts["slow_secure"][:4]))
            for msg in keyring_launch_verdicts(facts, variant, limit):
                r.check(False, msg)
            d = os.path.join(ctx.run, "logs", ctx.mode)
            os.makedirs(d, exist_ok=True)
            try:
                import ci_env
                kept = ci_env.redact(text, ci_env.ollama()[0])
            except Exception:
                kept = "(not kept: the redaction helper failed)\n"
            with open(os.path.join(d, "phosphor.keyring_%s.log" % variant), "w") as f:
                f.write(kept)
    finally:
        if ctx.app.keyring != "unlocked":                                  # a failed relaunch leaves the next cases a normal keyring
            ctx.app.keyring = "unlocked"
            relaunch(ctx, False)


def settle_change(ctx, before, tag, thresh, timeout=10, color=False):
    """poll screenshots until one differs from `before` by >= thresh (img_diff's colour test with color=True); -> (path, diff|None) or
    (None, last diff)"""
    t = time.time()
    last = None
    i = 0
    while time.time() - t < timeout:
        i += 1
        p = ctx.shot("%s_%d" % (tag, i))
        if p:
            ch, d = shot_changed(before, p, thresh, color)
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
    # r18u19 (Mac): the theme and font changes WERE applied (shots set_theme_*/set_font_* show the green theme and the doubled font) but the
    # grey-level diff read 0.3 % / 1.5 %; theme and font are judged with img_diff's colour test (same thresholds; identical shots read 0.03 %).
    steps = [("theme", "appearance.themes", "theme", ['"phosphor_green"', '"tokyo_night"'], 0.10, True),
             ("font", "appearance.text", "font_size", ["22.0", "11.0"], 0.02, True),
             ("vtabs", "appearance.vertical_tabs", "enabled", ["false", "true"], 0.004, False)]
    try:
        for name, table, key, vals, thresh, color in steps:
            if name == "theme":
                # r18u13 (Mac): the owner's settings follow the system theme (`appearance.themes.system_theme = true`), and then `theme` is not the
                # theme on screen (settings/theme.rs: the selected system themes are), so writing it changed nothing (diff 0.001 / 0.0 with Pillow;
                # the hash-only run before Pillow "passed" on the caret). Following the system theme is switched off first and the screen allowed to
                # settle, so the two writes below are judged on `theme` alone. The finally block restores the original file.
                set_setting(ctx, "appearance.themes", "system_theme", "false")
                time.sleep(2.5)
                cur = T.get_key(open(p, encoding="utf-8", errors="replace").read(), table, key) or ""
                if vals[0] in cur:                       # the first write must be a change from what is set now
                    vals = list(reversed(vals))
            for i, v in enumerate(vals):
                before = ctx.shot("set_%s_%d_before" % (name, i))
                set_setting(ctx, table, key, v)
                after, d = settle_change(ctx, before, "set_%s_%d_after" % (name, i), thresh, color=color)
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


@case("ui_completions_as_you_type_local", modes=("terminal",), order=64, gui=True)
def c_completions_as_you_type_local(ctx, r):
    """Wave 1.5: completions-as-you-type is ON by default for LOCAL sessions. With the setting not written by the profile, typing the common prefix of two
    files (NO Tab) opens the menu: both candidate names are on screen (OCR). A profile that sets the option explicitly (an explicit value wins) makes the
    case UNVERIFIABLE before its first key (round 16: case_settings.py declares completions_while_typing absent; the skip below is the same rule for a
    caller that runs the function without run.py)."""
    import cases_caseagent as CAG
    explicit = CAG.setting_bool(ctx, "completions_open_while_typing")
    if explicit is not None:
        r.skip("the profile sets completions_open_while_typing explicitly (%s); the default is not under test" % explicit)
    d = os.path.join(ctx.W, "tc")
    for n in ("cayt_alpha.txt", "cayt_bravo.txt"):
        open(os.path.join(d, n), "w").close()
    off = ctx.log_size()
    ctx.be.type("ls tc/cayt_", 15)
    time.sleep(1.2)
    txt = ctx.ocr("cayt_menu")
    if txt is None:
        r.unverifiable("no OCR on this machine (tesseract): the completion menu is not checked")
    else:
        flat = " ".join(txt.split())
        ok = CAG.fuzzy_has(flat, "cayt_alpha") and CAG.fuzzy_has(flat, "cayt_bravo")
        if not ok:      # r26: setup evidence for the row: is the fixture where the pane's cwd is, and what did the completion menu do while typing
            r.note("fixture dir %s exists=%s files=%s; menu hops: %s" % (d, os.path.isdir(d), sorted(os.listdir(d))[:6] if os.path.isdir(d) else None,
                   [l.split("hop=suggestions_mode", 1)[1].strip()[:90] for l in ctx.log_since(off).splitlines() if "hop=suggestions_mode" in l][-6:]))
        r.check(ok, "no completion menu with both candidates after typing a shared prefix without Tab")
    mark = ctx.cmd_mark()
    ctx.be.key("Escape")
    ctx.reset()
    r.check(ctx.cmds_since(mark) == [], "Escape/reset ran a command: %r" % ctx.cmds_since(mark))


def _reader_chunks(ctx, name):
    """the byte chunks bytes_reader.py logged so far (one "epoch hex" line per read)"""
    out = []
    try:
        for l in open(os.path.join(ctx.W, name)):
            p = l.split()
            if len(p) == 2:
                out.append(bytes.fromhex(p[1]))
    except FileNotFoundError:
        pass
    return out


@case("ui_readline_keys", order=63, gui=True)
def c_readline_keys(ctx, r):
    """#1989 / #1988 (owner decisions 2026-11-10): the readline keys on both sides of the prompt. (1) The input editor in front: `echo b<tok> >> `
    + Alt-. + an instant Enter runs exactly `echo b<tok> >> <last word of the previous command>` (rec.log and the effect file): Alt-. is the editor's
    yank-last-arg. (2) A program in front (bytes_reader.py, a raw tty, the stand-in for a nested shell): Alt-. arrives as ESC ., Alt-R as ESC r (no app
    binding claims it) and Ctrl-R as 0x12, the shell's own reverse-i-search key (command search opens only where the input editor has the key).
    Linux backends only: macOS sends Option-. as Meta only with the owner's Option-as-Meta setting, and the Windows chord is not mapped here."""
    if ctx.be.name not in ("x11", "wayland"):
        r.skip("backend %s: Alt-. / Alt-R are sent as X11/Wayland key names here (macOS needs Option-as-Meta, the Windows chord is not mapped)" % ctx.be.name)
    t = ctx.tok(7).lower()
    f = "rk_%s.txt" % t
    ctx.rm(f)
    r.check(run_cmd(ctx, "echo a%s >> %s" % (t, f)), "seeding command was not received")
    time.sleep(0.5)
    want = "echo b%s >> %s" % (t, f)
    mark = ctx.cmd_mark()
    ctx.be.type("echo b%s >> " % t, ctx.rng.randint(10, 25))
    ctx.be.key("alt+period")
    ctx.be.key("Return")
    ctx.until(lambda: len(ctx.cmds_since(mark)) >= 1, 8)
    time.sleep(0.5)
    got = ctx.cmds_since(mark)
    ctx.shot("readline_alt_dot")
    r.check(got == [want], "Alt-. + Enter: shell received %r, expected exactly [%r]" % (got, want))
    r.check(ctx.lines(f) == ["a" + t, "b" + t], "%s = %r, expected ['a%s', 'b%s']" % (f, ctx.lines(f), t, t))
    ctx.reset()

    # (2) a program has the terminal: the keys are encoded for it, none is taken by an app binding
    rf = "rk_%s_bytes.txt" % t
    ctx.rm(rf, rf + ".ready")
    ctx.be.type("python3 %s %s 40" % (os.path.join(os.path.dirname(os.path.abspath(__file__)), "bytes_reader.py"), rf), 12, enter=True)
    if not ctx.until(lambda: os.path.exists(os.path.join(ctx.W, rf + ".ready")), 20):
        r.check(False, "the raw reader never started")
        ctx.reset()
        return
    time.sleep(0.4)
    for spec, expect, what in (("alt+period", b"\x1b.", "Alt-."), ("alt+r", b"\x1br", "Alt-R"), ("ctrl+r", b"\x12", "Ctrl-R")):
        n0 = len(_reader_chunks(ctx, rf))
        ctx.be.key(spec)
        ctx.until(lambda: len(_reader_chunks(ctx, rf)) > n0, 5)
        time.sleep(0.3)
        got = b"".join(_reader_chunks(ctx, rf)[n0:])
        r.check(got == expect, "%s with a program in front: pty received %s, expected %s" % (what, got.hex() or "nothing", expect.hex()))
    ctx.shot("readline_program_in_front")
    ctx.be.key("ctrl+c")                           # ends the reader (raw tty: it reads the byte and exits)
    time.sleep(1.6)                                # past the agent view's exit-confirmation window before reset() sends its own Ctrl-C
    ctx.reset()
