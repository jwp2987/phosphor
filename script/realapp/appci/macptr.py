# appci/macptr.py: the macOS pointer for the real-app cases (#2061, r14-test-gaps T32; design note docs/design/r10-r15-w4mac.md).
#
# The cases place the pointer in SCREENSHOT PIXELS (cases_ui.Mouse: xdotool and the Wayland virtual pointer take the same numbers a screenshot
# has). A Mac screenshot of a Retina display has two pixels per point and CGEvent positions are in points, so MacMouse divides by the display
# scale, which it measures once: the width of a real screenshot over the display's width in points (`mptr info`). The tool is
# script/realapp/mac/tools/mptr.swift (CoreGraphics only; the runner has no PyObjC), run through backend.MacBackend.ptr(), i.e. behind the
# frontmost-app guard, and it refuses a press where the topmost window is not the app's.
# Also here, because they are the same enabling work: Apple Vision word boxes (mac/tools/ocrw.swift) as cases_ui.words() rows, the processes whose
# cwd is under a directory without /proc (lsof), and the recording URL handler of link_and_path_click (mac/tools/openrec.swift).
# Pure parts are tested in selftest_macptr.py.
import os
import struct
import subprocess
import time

MODBITS = {"shift": 1, "ctrl": 2, "control": 2, "alt": 4, "option": 4, "cmd": 8, "logo": 8, "meta": 8, "super": 8}
RECORDER_APP = "PhosphorAppciOpenRecorder.app"
RECORDER_NEEDS = ("NEEDS: the recording URL handler installed on the test Mac and made the default handler for http and https for the runner user: the "
                  "owner runs `script/realapp/mac/ci/open_recorder.sh install` once in the runner's GUI session and confirms the macOS dialog (a machine "
                  "change: every http/https link that user opens is then recorded and opens nothing). Phosphor opens a clicked link with "
                  "-[NSWorkspace openURL:], so without it a click would open the runner's browser on a test address and nothing would record it")


# ---------------------------------------------------------------- pure
def png_size(data):
    """(width, height) from the first bytes of a PNG file (the IHDR chunk), or None"""
    if len(data) < 24 or data[:8] != b"\x89PNG\r\n\x1a\n" or data[12:16] != b"IHDR":
        return None
    return struct.unpack(">II", data[16:24])


def display_scale(png_w, pt_w):
    """pixels per point: the screenshot's width over the display's width in points. A whole number (1, 2 or 3) within 2 %, else None: a
    screenshot that is not of this display (a second monitor, a scaled capture) must not be turned into pointer positions."""
    if not png_w or not pt_w:
        return None
    s = float(png_w) / float(pt_w)
    n = round(s)
    return n if n in (1, 2, 3) and abs(s - n) <= 0.02 * n else None


def to_points(x, y, scale):
    """screenshot pixels -> points, to a tenth of a point"""
    return round(float(x) / scale, 1), round(float(y) / scale, 1)


def _n(v):
    return ("%.1f" % v).rstrip("0").rstrip(".")


def mods_bits(names):
    """('ctrl', 'shift') -> mptr's f: bits. The Linux cases hold Ctrl for a link click; on macOS the app's link modifier is Cmd
    (app/src/terminal/links.rs), so 'ctrl' is carried as Cmd here: the case says what a Linux user does, this says what a Mac user does."""
    bits = 0
    for n in names:
        n = n.lower()
        if n not in MODBITS:
            raise ValueError("unknown modifier %r" % n)
        bits |= 8 if n in ("ctrl", "control") else MODBITS[n]
    return bits


def with_mods(ops, bits):
    """the ops with modifiers held around them (mptr releases them itself at exit as well)"""
    return (["f:%d" % bits] + list(ops) + ["f:0"]) if bits else list(ops)


def move_op(x, y, scale):
    px, py = to_points(x, y, scale)
    return "m:%s:%s" % (_n(px), _n(py))


def click_op(x, y, button, count, scale):
    if button not in (1, 2, 3) or not 1 <= count <= 3:
        raise ValueError("button %r / count %r" % (button, count))
    px, py = to_points(x, y, scale)
    return "k:%s:%s:%d:%d" % (_n(px), _n(py), button, count)


def drag_op(x0, y0, x1, y1, scale):
    a, b = to_points(x0, y0, scale), to_points(x1, y1, scale)
    return "g:%s:%s:%s:%s" % (_n(a[0]), _n(a[1]), _n(b[0]), _n(b[1]))


def wheel_op(x, y, steps, scale):
    px, py = to_points(x, y, scale)
    return "w:%s:%s:%d" % (_n(px), _n(py), int(steps))


def arrived(loc, want_pts, slack=1.5):
    """True when the cursor the window server reports (`LOC`, points) is where the last op put it: the events were accepted (a process without
    the Accessibility grant posts into the void and the cursor stays where it was)"""
    return loc is not None and abs(loc[0] - want_pts[0]) <= slack and abs(loc[1] - want_pts[1]) <= slack


def vision_words(tsv):
    """cases_ui.words() rows (text, cx, cy, left, right, top, bottom; image pixels) from ocrw's output (word, x, y, w, h per line)"""
    out = []
    for l in tsv.splitlines():
        c = l.split("\t")
        if len(c) != 5 or not c[0].strip():
            continue
        try:
            x, y, w, h = (int(v) for v in c[1:])
        except ValueError:
            continue
        if w <= 0 or h <= 0:
            continue
        out.append((c[0].strip(), x + w // 2, y + h // 2, x, x + w, y, y + h))
    return out


def origin_px(rect_pts, scale):
    """the app window's top-left corner in screenshot pixels, from its rectangle in points (winrect). The app's windows have no system title bar
    (hide_title_bar: full-size content view), so the window's corner is the corner of what the app draws, which is where the find hook's
    device-pixel coordinates start."""
    if not rect_pts or not scale:
        return None
    return int(round(rect_pts[0] * scale)), int(round(rect_pts[1] * scale))


def cwd_pids(lsof_out, prefix):
    """pids whose current directory is `prefix` or below it, from `lsof -a -d cwd -Fpn` (field output: a `p<pid>` line, then that process's
    `n<path>` line). macOS has no /proc; this is cases_robust.descendants_cwd_under there."""
    out, pid = set(), None
    pre = prefix.rstrip("/")
    for l in lsof_out.splitlines():
        if l.startswith("p") and l[1:].isdigit():
            pid = int(l[1:])
        elif l.startswith("n") and pid is not None:
            path = l[1:]
            if path == pre or path.startswith(pre + "/"):
                out.add(pid)
    return out


def recorder_calls(text):
    """the recording URL handler's log as cases_pointer.opener_calls rows: [(program, [arguments])], one per URL"""
    out = []
    for l in text.splitlines():
        p = l.split("\t")
        if len(p) == 2 and p[0] == "open" and p[1]:
            out.append(("openrec", [p[1]]))
    return out


def recorder_is_default(handler_path):
    """True when the application macOS would open an https URL with is the recorder's bundle"""
    return os.path.basename((handler_path or "").strip().rstrip("/")) == RECORDER_APP


# ---------------------------------------------------------------- the machine
def tool(be, name):
    p = os.path.join(getattr(be, "TOOLS", "") or "", name)
    return p if os.access(p, os.X_OK) else None


def missing_tool_reason(be, name):
    """the SKIP text of a case that needs a compiled Mac tool the job does not have"""
    return ("NEEDS: the %s tool compiled on the Mac runner (script/realapp/mac/tools/%s.swift did not compile or APPCI_MAC_TOOLS is not set: see "
            "swiftc-%s.log in the job artifacts; mac_pointer_chord_proof FAILS in the same run and says the same)" % (name, name, name))


def measure_scale(be):
    """(scale, why): the display scale from one real screenshot and `mptr info`; scale None with the reason when it cannot be trusted"""
    info = be.ptr_info()
    disp = info.get("display")
    if not disp:
        return None, "`mptr info` gave no display line"
    p = os.path.join(os.environ.get("TMPDIR", "/tmp"), "appci_scale_%d.png" % os.getpid())
    try:
        be.shot(p)
        with open(p, "rb") as f:
            size = png_size(f.read(32))
    except OSError:
        size = None
    finally:
        try:
            os.remove(p)
        except OSError:
            pass
    if not size:
        return None, "a screenshot could not be taken or is not a PNG"
    s = display_scale(size[0], disp[2])
    if s is None:
        return None, "the screenshot is %d px wide and the main display %d points: not a whole scale (another display captured?)" % (size[0], disp[2])
    return s, "screenshot %dx%d px over a display of %dx%d points" % (size[0], size[1], disp[2], disp[3])


def words(be, shot_path):
    """cases_ui.words() rows of a screenshot by Apple Vision; [] when the tool is missing or fails"""
    t = tool(be, "ocrw")
    if not t:
        return []
    try:
        r = subprocess.run([t, shot_path], capture_output=True, timeout=90)
    except Exception:
        return []
    return vision_words(r.stdout.decode(errors="replace"))


def pids_cwd_under(prefix):
    try:
        r = subprocess.run(["lsof", "-a", "-d", "cwd", "-Fpn"], capture_output=True, timeout=60)
    except Exception:
        return set()
    return cwd_pids(r.stdout.decode(errors="replace"), os.path.realpath(prefix)) | cwd_pids(r.stdout.decode(errors="replace"), prefix)


class MacMouse:
    """cases_ui.Mouse for macOS: the same methods, the same unit (screenshot pixels). `mods` (mptr f: bits) are held around every action while set
    (cases_pointer.Held sets and clears it: a modifier cannot outlive one mptr process, so it is held inside the call that clicks)."""

    def __init__(self, be):
        self.be = be
        self.mods = 0
        self.scale, self.scale_how = measure_scale(be)
        self.last = None                   # (wanted points, cursor read back) of the last action

    def _do(self, ops, end_px):
        loc = self.be.ptr(with_mods(ops, self.mods))
        self.last = (to_points(end_px[0], end_px[1], self.scale), loc)
        return loc

    def move(self, x, y):
        self._do([move_op(x, y, self.scale)], (x, y))
        time.sleep(0.15)

    def click(self, x, y, button=1, count=1):
        self._do([click_op(x, y, button, count, self.scale)], (x, y))
        time.sleep(0.1)

    def drag(self, x0, y0, x1, y1):
        self._do([drag_op(x0, y0, x1, y1, self.scale)], (x1, y1))
        time.sleep(0.05)

    def wheel(self, x, y, steps):
        self._do([wheel_op(x, y, steps, self.scale)], (x, y))

    def delivered(self):
        """True / False: the last action left the cursor where it was sent; None before any action"""
        return None if self.last is None else arrived(self.last[1], self.last[0])
