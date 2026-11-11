# appci/cases_window_use.py: round 14 wave 3 (docs/design/r14-test-gaps.md 7.1, T26-T30; design note docs/design/r14-w3b.md): what a user does to the
# WINDOW rather than to the prompt. All five are in cases.PROVING (owner rule 2026-11-09: a new case runs in the fix tier until the owner graduates it).
#   resize_reflow_output  T26 (Wayland, X11, macOS): soft-wrapped known lines copied before and after the window is resized (the copy is byte-identical: reflow
#                         moved no line break); a program with a SIGWINCH trap streams 700 numbered lines under 60 resizes in 10 s (no line lost, the pty's
#                         final `stty size` is the app's grid, the trap ran); vim open during a resize reports the new size; the size returns at the end.
#   context_menu_actions  T27 (Wayland, X11): right click on a block -> Copy output, Copy command: the clipboard holds exactly the output / the command;
#                         right click in the input -> Paste: the input holds exactly the clipboard text at Enter. Every pointer position comes from the
#                         app's own `appci_find` line (#1789), never from OCR.
#   tab_pointer_use       T28 (Wayland, X11): click a tab, drag a tab onto its neighbour, middle-click a tab; then the same in the other tab layout
#                         (horizontal bar <-> vertical panel). The order is proven by the tab number keys and each tab's own `pwd`; the closed tab's
#                         shell process is gone.
#   focus_cycle           T29 (Wayland, X11, macOS, Windows): the keyboard focus goes to the reference terminal, lines are typed there, the focus comes
#                         back and a line is typed at once: it arrives exactly, nothing typed elsewhere reached Phosphor (shell log and the app's key
#                         trace). A raw reader that asked for focus reporting gets exactly one ESC[O and one ESC[I for one away-and-back (POSIX).
#   minimize_fullscreen   T30 (Wayland, X11, macOS): the window is hidden for 30 s under a stream and shown again: every line is there, a frame is drawn
#                         promptly and no main-thread item after the return is far above the same stream's items while shown; fullscreen on and off
#                         changes `stty size` and returns it; typing is exact after each step.
# Same rules as cases.py: the EXACT text (clipboard, effect file, rec.log), typing at 10-25 ms/key with instant Enter, a case that cannot run SKIPS with the
# missing capability named, a half that cannot be judged is UNVERIFIABLE (status noref), never a pass.
import json, os, re, shutil, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases as C  # noqa: E402  (cases.py imports this module near its end, so everything below is already defined)
import cases_caseagent as CA  # noqa: E402
import cases_coverage_a as CVA  # noqa: E402
import cases_robust as R  # noqa: E402
import cases_ui as U  # noqa: E402
import cases_wrap_vtabs as V  # noqa: E402
import toml_text as T  # noqa: E402
from cases import case  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
READER = os.path.join(HERE, "bytes_reader.py")
STREAM_SH = "wu_stream.sh"
# the stream program of T26 and T30. `bash wu_stream.sh TAG N INTERVAL` prints "TAG line I" for I = 1..N; every SIGWINCH appends the tty's size to
# wu_TAG.winch (bash runs the trap when the `sleep` in front returns); the size at the end goes to wu_TAG.final, then wu_TAG.done appears.
STREAM_PROG = """#!/bin/bash
tag=$1; n=$2; iv=$3
trap 'stty size >> "wu_$tag.winch"' WINCH
: > "wu_$tag.started"
i=0
while [ "$i" -lt "$n" ]; do
  i=$((i+1))
  echo "$tag line $i"
  sleep "$iv"
done
stty size > "wu_$tag.final"
: > "wu_$tag.done"
"""
WIN_RESIZE_WHY = "no window resize or window-state call in the Windows backend yet (r14-test-gaps T33)"
UI_WARN_MS = 16.0             # the owner's UI budget is 4 ms, its warn bound 16 ms (perf budgets)
RESTORE_CONTROL_FACTOR = 3.0  # T30: the largest main-thread item after the window returns, against the same stream's largest item while shown


# ---------------------------------------------------------------- pure helpers (unit-tested in selftest.py)
GRID_HOP_RE = re.compile(r"hop=grid_resize rows=(\d+) cols=(\d+)")
FIND_LINE_RE = re.compile(r"appci_find token=(.+?) x0=(-?[\d.]+) y0=(-?[\d.]+) x1=(-?[\d.]+) y1=(-?[\d.]+)")
FRAME_DRAWN_RE = re.compile(r"hop=frame_drawn .*? t=(\d+(?:\.\d+)?)")
FRAME_ANY_RE = re.compile(r"hop=frame_drawn |frame \d+: updated=\[")


def parse_stty(line):
    """`stty size` output ('24 80') -> (rows, cols), or None"""
    m = re.match(r"\s*(\d+)\s+(\d+)\s*$", line or "")
    return (int(m.group(1)), int(m.group(2))) if m else None


def grid_hops(text):
    """[(rows, cols)] of every `hop=grid_resize` line of an app-log excerpt (app/src/terminal/view/grid_resize_hop.rs), oldest first"""
    return [(int(a), int(b)) for a, b in GRID_HOP_RE.findall(text)]


def find_rows(text, phrase, origin=(0, 0), scale=1.0):
    """every `appci_find` line for `phrase` in an app-log excerpt, oldest first, as cases_ui.words() rows (text, cx, cy, left, right, top, bottom) in
    screen pixels. Unlike cases_ui.FIND_RE the token may hold spaces ('Copy output'): the app trims the token file's lines and nothing else."""
    out = []
    for m in FIND_LINE_RE.finditer(text):
        if m.group(1) != phrase:
            continue
        x0, y0, x1, y1 = (float(m.group(i)) * scale for i in (2, 3, 4, 5))
        l, t, rgt, b = int(origin[0] + x0), int(origin[1] + y0), int(origin[0] + x1), int(origin[1] + y1)
        out.append((phrase, (l + rgt) // 2, (t + b) // 2, l, rgt, t, b))
    return out


def stream_lines(tag, n):
    return ["%s line %d" % (tag, i) for i in range(1, n + 1)]


def judge_stream_copy(got, tag, n):
    """None when `got` (V.norm_clip text, trailing line ends ignored) is exactly the n lines the stream program printed, in order; else what is wrong:
    how many of the lines are there, the first one missing, a line twice, lines out of order, or other text in between."""
    want = stream_lines(tag, n)
    have = got.rstrip("\n").split("\n") if got.strip("\n") else []
    if have == want:
        return None
    rx = re.compile(r"^%s line (\d+)$" % re.escape(tag))
    nums = [int(m.group(1)) for m in (rx.match(l) for l in have) if m]
    other = [l for l in have if not rx.match(l)]
    seen = set(nums)
    missing = [i for i in range(1, n + 1) if i not in seen]
    twice = sorted({i for i in nums if nums.count(i) > 1}) if len(nums) != len(seen) else []
    parts = ["%d of %d lines" % (len(seen & set(range(1, n + 1))), n)]
    if missing:
        parts.append("%d LOST, the first is line %d" % (len(missing), missing[0]))
    if twice:
        parts.append("%d line(s) twice, the first is line %d" % (len(twice), twice[0]))
    if nums != sorted(nums):
        k = next(i for i in range(1, len(nums)) if nums[i] < nums[i - 1])
        parts.append("out of order: line %d comes after line %d" % (nums[k], nums[k - 1]))
    if other:
        parts.append("%d line(s) that the program did not print, the first is %r" % (len(other), other[0][:60]))
    return "; ".join(parts)


def plan_sizes(orig_w, orig_h):
    """the window sizes the resize case asks for, each provably a CHANGE: (narrow, wide, three sizes the rapid resizes cycle through, the size the rapid
    resizes end on, the size vim is resized to). A size within 60 px of the original width is moved 150 px in, so 'the window took it' is observable."""
    raw = [(900, 600), (1300, 800), (1000, 620), (1240, 760), (1120, 700), (1180, 720), (1060, 660)]
    out = []
    for w, h in raw:
        if abs(w - orig_w) < 60:
            w -= 150
        if abs(h - orig_h) < 40:
            h -= 90
        out.append((w, h))
    return dict(narrow=out[0], wide=out[1], rapid=out[2:5], rapid_end=out[5], vim=out[6])


def resize_plan(n, sizes, end):
    """n window sizes for the rapid resizes: `sizes` in turn (never the same size twice in a row), the last one `end` (different from the one before)"""
    plan = [sizes[i % len(sizes)] for i in range(n - 1)]
    if plan and plan[-1] == end:
        plan[-1] = sizes[(n - 1) % len(sizes)] if sizes[(n - 1) % len(sizes)] != end else sizes[n % len(sizes)]
    return plan + [end]


def right_click_pastes(settings_text):
    """does this profile's settings file make a right click PASTE (terminal.input.right_click_behavior = "paste") instead of opening the context menu?"""
    v = T.get_key(settings_text, "terminal.input", "right_click_behavior")
    return bool(v and re.search(r"=\s*[\"']?paste\b", v, re.I))


def pick_tab_spots(cands, vertical, band=14, min_step=6):
    """cands = {tab number: [(cx, cy), ...]}: every place a tab's token was seen drawn (its title in the tab strip, and wherever else it was on screen
    while that tab was shown). -> ({tab: (cx, cy)}, description) with the strip's spot of each tab, or (None, why) when they are not provably the
    strip: a horizontal bar is the TOPMOST sighting of each token, all on one row and left to right in tab order; a vertical panel is the LEFTMOST
    sighting of each, top to bottom in tab order and spread more down than across (V.rows_stacked)."""
    if len(cands) < 2 or any(not v for v in cands.values()):
        none = sorted(k for k, v in cands.items() if not v)
        return None, "the title of tab(s) %s was never seen drawn" % none if none else "fewer than two tabs to locate"
    tabs = sorted(cands)
    if vertical:
        spots = {k: min(cands[k], key=lambda p: (p[0], p[1])) for k in tabs}
        ok, desc = V.rows_stacked([(k, spots[k][0], spots[k][1]) for k in tabs], min_step)
        return (spots, desc) if ok else (None, desc)
    spots = {k: min(cands[k], key=lambda p: (p[1], p[0])) for k in tabs}
    desc = ", ".join("tab %d at (%d, %d)" % (k, spots[k][0], spots[k][1]) for k in tabs)
    ys, xs = [spots[k][1] for k in tabs], [spots[k][0] for k in tabs]
    if max(ys) - min(ys) > band:
        return None, "the topmost sightings are not on one row (a title is not drawn in the tab bar): %s" % desc
    if any(b - a < min_step for a, b in zip(xs, xs[1:])):
        return None, "the titles are not left to right in tab order: %s" % desc
    return spots, desc


def order_after_drag(order):
    """the tab order after the second tab is dragged onto the third: they change places"""
    return [order[0], order[2], order[1]] + list(order[3:])


def drag_points(a, b, vertical, over=12):
    """(x0, y0, x1, y1) of a drag from spot a to `over` px past spot b along the strip's axis (the dragged tab's box then covers its neighbour's)"""
    if vertical:
        return a[0], a[1], a[0], b[1] + (over if b[1] >= a[1] else -over)
    return a[0], a[1], b[0] + (over if b[0] >= a[0] else -over), a[1]


def judge_focus_bytes(got):
    """None when the bytes a raw reader with focus reporting on received over ONE away-and-back are exactly ESC[O, ESC[I, the key typed after the
    return ('k') and the Ctrl-C that ends the reader; else what is wrong. Any other byte is a key that was typed into the other window."""
    out_, in_ = b"\x1b[O", b"\x1b[I"
    body = got[:-1] if got.endswith(b"\x03") else got
    no, ni = body.count(out_), body.count(in_)
    rest = body.replace(out_, b"").replace(in_, b"")
    if not no and not ni:
        return "no focus report reached the program (it received %s): ESC[?1004h was sent and the focus left and came back" % (rest.hex() or "nothing")
    if (no, ni) != (1, 1):
        return "%d focus-out and %d focus-in reports for one away-and-back (received %s)" % (no, ni, body.hex())
    if body.index(out_) > body.index(in_):
        return "focus-in arrived before focus-out (received %s)" % body.hex()
    if rest != b"k":
        return "besides the two focus reports the program received %s, expected only the 'k' (6b) typed after the return: %s" % (
            rest.hex() or "nothing", "a key typed into the other window reached it" if len(rest) > 1 else "the key typed after the return was lost or changed")
    if not body.endswith(b"k"):
        return "the key typed after the return arrived before a focus report (received %s)" % body.hex()
    return None


def first_frame_after(text, t_show):
    """(ms from t_show to the first frame the app drew after it, frames seen, True when the frame lines carry a time) from the app log written since
    the window was shown again. `hop=frame_drawn ... t=<epoch ms>` (winit) carries the time; a `frame N: updated=[` line (frame_cause) only proves a
    frame. No frame at all -> (None, 0, False)."""
    ts = sorted(float(t) / 1000.0 for t in FRAME_DRAWN_RE.findall(text))
    after = [t for t in ts if t >= t_show]
    n = len(FRAME_ANY_RE.findall(text))
    if after:
        return round((after[0] - t_show) * 1000.0, 1), n, True
    return None, n, False


def judge_restore_ui(control_max_ms, restore_max_ms, warn_ms=UI_WARN_MS, factor=RESTORE_CONTROL_FACTOR):
    """(ok, bound ms): the largest main-thread item in the seconds after the window returned must stay within the warn bound, or within `factor` times
    the largest item of the SAME stream while the window was shown (software renderers draw every frame over the 4 ms budget, so the absolute bound
    alone would judge the renderer)."""
    bound = max(warn_ms, factor * control_max_ms)
    return restore_max_ms <= bound, round(bound, 1)


# ---------------------------------------------------------------- shared steps
def safe_token(ctx, prefix):
    """a token of glyphs the hook, OCR and a shell word all take: prefix + lower-case OCR-safe characters"""
    return prefix + U.ocr_safe(ctx.tok(6).lower().replace("_", ""))


def pane_size(ctx, tag):
    """the pane's own `stty size` -> (rows, cols) or None (one typed command)"""
    f = "wu_size_%s.txt" % tag
    ctx.rm(f)
    U.run_cmd(ctx, "stty size > %s" % f)
    ctx.until(lambda: ctx.lines(f), 6)
    return parse_stty((ctx.lines(f) or [""])[0])


def write_stream_script(ctx):
    with open(os.path.join(ctx.W, STREAM_SH), "w", newline="\n") as f:
        f.write(STREAM_PROG)


def copy_last_block_output(ctx, r, tag):
    """select the newest block (block-up key) and copy its output -> text, V.SENTINEL when nothing was copied; the input has the keyboard again after"""
    try:
        ctx.be.key(V.KX(ctx, "block_up"))
        time.sleep(0.6)
        return V.clip_after(ctx, V.KX(ctx, "copy_output"), tag)
    finally:
        V.back_to_input(ctx, r)


def app_pid(ctx):
    fn = getattr(ctx.be, "pid_fn", None)
    return (fn() if fn else None) or (ctx.app.pid() if ctx.app is not None and hasattr(ctx.app, "pid") else None)


def window_origin(ctx):
    """(x, y) of the app window's content on screen (sway / xdotool), or None: what turns the hook's window pixels into pointer positions"""
    pid = app_pid(ctx)
    if not pid:
        return None
    if U.on_wayland(ctx):
        rect = U.sway_window_rect(pid)
        return (rect[0], rect[1]) if rect else None
    return U.x11_window_origin(pid) if ctx.be.name == "x11" else None


def hook_rows(ctx, phrase, since):
    origin = window_origin(ctx)
    return find_rows(ctx.log_since(since), phrase, origin) if origin else []


def wait_hook(ctx, phrase, since, timeout=3.0):
    """the newest place the app drew `phrase` since log offset `since` (a words() row), waiting up to `timeout` for the line; None without one"""
    rows = ctx.until(lambda: hook_rows(ctx, phrase, since), timeout, step=0.1)
    return rows[-1] if rows else None


def ptr_check(ctx, r, ok, msg):
    """a claim that rests only on pointer delivery: a plain check, except on Wayland WITHOUT the virtual pointer holder (NOREF, as cases_ui.wl_check)"""
    return U.wl_check(ctx, r, ok, msg, "pointer")


def _osa(script):
    rc = subprocess.run(["osascript", "-e", script], capture_output=True, timeout=30)
    return rc.returncode, rc.stdout.decode(errors="replace").strip(), rc.stderr.decode(errors="replace").strip()[:120]


class Window:
    """the app window's size and state on this backend. Wayland: sway commands addressed by the app's pid (never 'the focused window'); X11: xdotool
    on the app's window id; macOS: the Accessibility attributes of the app's first window through System Events (the path ui_resize_typing uses).
    restore() undoes everything the case did and never raises."""

    def __init__(self, ctx):
        self.ctx, self.be = ctx, ctx.be.name
        self.orig, self.xid, self.floated, self.ws, self.unmapped = None, None, False, None, False

    def _sway(self, cmd):
        rc = subprocess.run(["swaymsg", cmd], capture_output=True, timeout=10)
        out = rc.stdout.decode(errors="replace")
        ok = rc.returncode == 0 and '"success": false' not in out and '"success":false' not in out
        return None if ok else "swaymsg %r: %s" % (cmd, (rc.stderr.decode(errors="replace") or out).strip()[:120])

    def _sway_node(self):
        pid = app_pid(self.ctx)
        try:
            tree = json.loads(subprocess.run(["swaymsg", "-t", "get_tree"], capture_output=True, timeout=10).stdout)
        except Exception:
            return None
        hit = []

        def walk(n):
            if n.get("pid") == pid and not n.get("nodes") and n.get("rect"):
                hit.append(n)
            for c in n.get("nodes", []) + n.get("floating_nodes", []):
                walk(c)
        walk(tree)
        return hit[0] if hit else None

    def _mac(self, tail):
        return _osa('tell application "System Events" to %s of window 1 of (first process whose unix id is %d)' % (tail, app_pid(self.ctx) or 0))

    def has_wm(self):
        """X11: does a window manager own the display? (the job's Xvfb has none: minimise and fullscreen requests go to a manager)"""
        if shutil.which("xprop"):
            out = subprocess.run(["xprop", "-root", "_NET_SUPPORTING_WM_CHECK"], capture_output=True, timeout=10).stdout.decode(errors="replace")
            return "window id #" in out
        return subprocess.run(["xdotool", "get_desktop"], capture_output=True, timeout=10).returncode == 0

    def capture(self):
        """remember the window as it is -> None, or why this backend cannot control it"""
        if not app_pid(self.ctx):
            return "the app's pid is unknown"
        if self.be == "wayland":
            if not shutil.which("swaymsg"):
                return "swaymsg missing"
            n = self._sway_node()
            if not n:
                return "sway shows no window of the app's pid"
            self.orig = (n["rect"]["width"], n["rect"]["height"])
            self.floated = n.get("type") == "floating_con"
            return None
        if self.be == "x11":
            if not shutil.which("xdotool"):
                return "xdotool missing"
            g = U.get_geometry(self.ctx)
            if not g:
                return "no visible phosphor window"
            self.xid, self.orig = g[0], (g[1], g[2])
            return None
        if self.be == "mac":
            rect = self.ctx.be.window_rect() if hasattr(self.ctx.be, "window_rect") else None
            if not rect:
                return "the window rectangle could not be read (winrect)"
            self.orig = (int(rect[2]), int(rect[3]))
            return None
        return "backend %s has no window control" % self.be

    def resize(self, w, h):
        """-> None or the error"""
        if self.be == "wayland":
            return self._sway("[pid=%d] floating enable, resize set width %d px height %d px" % (app_pid(self.ctx), w, h))
        if self.be == "x11":
            rc = subprocess.run(["xdotool", "windowsize", self.xid, str(w), str(h)], capture_output=True, timeout=10)
            return None if rc.returncode == 0 else "xdotool windowsize: " + rc.stderr.decode(errors="replace")[:100]
        return U.resize_to(self.ctx, w, h)

    def hide(self):
        """take the window off the screen -> (how, error or None)"""
        if self.be == "wayland":
            try:
                ws = json.loads(subprocess.run(["swaymsg", "-t", "get_workspaces"], capture_output=True, timeout=10).stdout)
                self.ws = next(w["name"] for w in ws if w.get("focused"))
            except Exception as e:
                return "sway workspace switch", "the focused workspace could not be read: %r" % (e,)
            return ("sway has no minimise: its workspace is switched away (the window is not shown and gets no frame callbacks)",
                    self._sway("workspace appcihidden"))
        if self.be == "x11":
            wm = self.has_wm()
            rc = subprocess.run(["xdotool", "windowminimize" if wm else "windowunmap", self.xid], capture_output=True, timeout=10)
            self.unmapped = True
            return ("xdotool windowminimize" if wm else "no window manager on the display: the window is unmapped, which is what a manager's minimise does to it",
                    None if rc.returncode == 0 else rc.stderr.decode(errors="replace")[:100])
        rc, _out, err = _osa('tell application "System Events" to set value of attribute "AXMinimized" of window 1 of (first process whose unix id is %d) to true'
                             % (app_pid(self.ctx) or 0))
        return "AXMinimized through System Events", (None if rc == 0 else "System Events refused (Accessibility permission?): " + err)

    def hidden(self):
        """True / False, or None when it cannot be told"""
        try:
            if self.be == "wayland":
                n = self._sway_node()
                return None if n is None or "visible" not in n else not n["visible"]
            if self.be == "x11":
                ids = subprocess.run(["xdotool", "search", "--onlyvisible", "--class", "phosphor"], capture_output=True, timeout=10).stdout.decode().split()
                return self.xid not in ids
            rc, out, _err = self._mac('get value of attribute "AXMinimized"')
            return (out == "true") if rc == 0 else None
        except Exception:
            return None

    def show(self):
        """bring the window back and give it the keyboard -> None or the error"""
        err = None
        if self.be == "wayland":
            if self.ws is not None:
                err = self._sway("workspace %s" % self.ws)
                self.ws = None
        elif self.be == "x11":
            if self.unmapped:
                rc = subprocess.run(["xdotool", "windowmap", self.xid], capture_output=True, timeout=10)
                err = None if rc.returncode == 0 else rc.stderr.decode(errors="replace")[:100]
                self.unmapped = False
                time.sleep(0.4)                                 # mapped before the focus is asked for
        else:
            rc, _out, e = _osa('tell application "System Events" to set value of attribute "AXMinimized" of window 1 of (first process whose unix id is %d) to false'
                               % (app_pid(self.ctx) or 0))
            err = None if rc == 0 else e
        try:
            self.ctx.be.refocus()
        except Exception as e:
            err = err or "refocus: %r" % (e,)
        return err

    def fullscreen(self, on):
        """ask for fullscreen on / off the way a user does where the harness can: the app's own F11 binding (root_view:toggle_fullscreen) on Linux; the
        window's AXFullScreen attribute on macOS (the cmd-ctrl-f chord is one the frontmost-app guard refuses) -> None or the error"""
        if self.be in ("wayland", "x11"):
            self.ctx.be.key("F11")
            return None
        rc, _out, err = _osa('tell application "System Events" to set value of attribute "AXFullScreen" of window 1 of (first process whose unix id is %d) to %s'
                             % (app_pid(self.ctx) or 0, "true" if on else "false"))
        return None if rc == 0 else err

    def restore(self):
        try:
            if self.ws is not None or self.unmapped:
                self.show()
            if self.be == "wayland":
                pid = app_pid(self.ctx)
                if pid:
                    self._sway("[pid=%d] fullscreen disable" % pid)
                    if not self.floated:
                        self._sway("[pid=%d] floating disable" % pid)
                    elif self.orig:
                        self._sway("[pid=%d] resize set width %d px height %d px" % ((pid,) + self.orig))
            elif self.be == "x11" and self.xid and self.orig:
                subprocess.run(["xdotool", "windowsize", self.xid, str(self.orig[0]), str(self.orig[1])], capture_output=True, timeout=10)
            elif self.be == "mac" and self.orig:
                _osa('tell application "System Events" to set value of attribute "AXFullScreen" of window 1 of (first process whose unix id is %d) to false'
                     % (app_pid(self.ctx) or 0))
                time.sleep(1.5)
                U.resize_to(self.ctx, self.orig[0], self.orig[1])
            self.ctx.be.refocus()
        except Exception:
            pass


def need_window(ctx, r):
    """the Window of a case that resizes / hides it, or a SKIP naming what is missing. Called before the first check of the case."""
    if ctx.win:
        r.skip("Windows: " + WIN_RESIZE_WHY)
    if ctx.app is None:
        r.skip("no app process (sim backend)")
    win = Window(ctx)
    err = win.capture()
    if err:
        r.skip("the app window cannot be controlled on %s: %s" % (ctx.be.name, err))
    return win


def apply_size(ctx, win, w, h, off, tag):
    """resize the window and prove it took -> ('hop', rows, cols) from a new `hop=grid_resize` line, ('screen', None, None) from a changed screenshot
    (an app without the hop), or (None, why, None) when neither shows a change"""
    hops0 = grid_hops(ctx.log_since(off))
    base = ctx.shot(tag + "_before")
    err = win.resize(w, h)
    if err:
        return None, err, None

    def fresh():
        hs = grid_hops(ctx.log_since(off))
        return hs[-1] if len(hs) > len(hops0) and hs[-1] != (hops0[-1] if hops0 else None) else None
    hit = ctx.until(fresh, 5, step=0.1)
    time.sleep(0.9)                                   # the reflow of what is on screen and the redraw
    if hit:
        return ("hop",) + grid_hops(ctx.log_since(off))[-1]
    s = ctx.shot(tag)
    changed = bool(base and s) and U.shot_changed(base, s, 0.01)[0]
    return ("screen", None, None) if changed else (None, "no hop=grid_resize line with a new size and no change on screen", None)


def grid_matches(ctx, r, off, size, what):
    """the app's grid (latest hop=grid_resize since `off`) against the pty's own `stty size`; UNVERIFIABLE when the app logged no hop"""
    hops = grid_hops(ctx.log_since(off))
    if not hops:
        r.unverifiable("%s: the app logged no hop=grid_resize line (a build without the hop, or RUST_LOG without warp::input_order=debug): the pty's size %r "
                       "is not compared with the app's grid" % (what, size))
        return
    r.check(size == hops[-1], "%s: the pty is %r rows x columns (`stty size`), the app's grid is %r (latest hop=grid_resize): the last resize did not reach "
                              "the pty, or reached it out of order" % (what, size, hops[-1]))


# ---------------------------------------------------------------- T26 resize_reflow_output
@case("resize_reflow_output", order=73, gui=True)
def c_resize_reflow_output(ctx, r):
    """T26 (G34): what is already on screen survives a resize, and a program learns of every size. (1) Soft-wrapped known lines: the block's copied output
    after a narrower and a wider window is byte-identical to the copy before. (2) A program with a SIGWINCH trap prints 700 numbered lines while the
    window is resized 60 times in 10 s: all 700 lines are in the copied output in order, the trap ran, and the pty's `stty size` afterwards is the
    app's grid (hop=grid_resize). (3) vim open during a resize reports the new size. (4) The original size comes back. Typing stays exact."""
    win = need_window(ctx, r)
    if not V.first_tab_in_front(ctx, r, "rr"):
        return
    off = ctx.log_size()
    size0 = pane_size(ctx, "rr0")
    if not r.check(size0 is not None, "`stty size` gave no size before the first resize: nothing can be compared"):
        return
    sizes = plan_sizes(*win.orig)
    r.metric("window", dict(original=list(win.orig), rows_cols=list(size0)))
    try:
        _reflow_wrapped(ctx, r, win, off, sizes)
        _stream_under_resizes(ctx, r, win, off, sizes)
        _vim_during_resize(ctx, r, win, off, sizes)
    finally:
        win.restore()
        time.sleep(1.5)
        V.back_to_input(ctx, r)
    end = pane_size(ctx, "rr_end")
    r.check(end == size0, "after the window got its original size back `stty size` is %r, it was %r before the case" % (end, size0))
    if end:
        grid_matches(ctx, r, off, end, "back at the original size")
    U.exact_burst(ctx, r, "rr_after", 4)
    r.check(ctx.app.alive(), "the app died in the resize case")


def _reflow_wrapped(ctx, r, win, off, sizes):
    fx = V.print_wrap_fixture(ctx, r, "rr")
    if not fx:
        return
    lines, _spans, cols0, _pick, _seen = fx
    base = copy_last_block_output(ctx, r, "rr_copy_base")
    if base == V.SENTINEL:
        r.unverifiable("the copy-output key left the clipboard unchanged after the block-up key (%s): the printed block was not selected, so the copy "
                       "before and after a reflow is NOT compared (shots/*rr_copy_base.png)" % V.KX(ctx, "block_up"))
        return
    err = V.judge_wrapped_copy(base, lines)
    r.check(err is None, "BEFORE any resize, pane %d columns, the copied output is not what was printed (the subject of ui_copy_wrapped_output): %s" % (cols0, err))
    for name in ("narrow", "wide"):
        w, h = sizes[name]
        how, a, b = apply_size(ctx, win, w, h, off, "rr_" + name)
        if how is None:
            r.unverifiable("the window did not take %dx%d (%s): the reflow at the %s size is not judged" % (w, h, a, name))
            continue
        r.metric("reflow_%s" % name, dict(window=[w, h], proof=how, rows_cols=[a, b]))
        got = copy_last_block_output(ctx, r, "rr_copy_" + name)
        why = "nothing was copied" if got == V.SENTINEL else (V.judge_wrapped_copy(got, lines) or "it differs from the first copy only")
        r.check(got == base, "copied output after the reflow to the %s window (%dx%d%s) is not byte-identical to the copy before it: %s" % (
            name, w, h, ", %s columns" % b if b else "", why))


def _stream_under_resizes(ctx, r, win, off, sizes):
    tag = safe_token(ctx, "rr")
    n = max(200, int(os.environ.get("APPCI_RR_LINES", "700")))
    write_stream_script(ctx)
    for suf in ("started", "winch", "final", "done"):
        ctx.rm("wu_%s.%s" % (tag, suf))
    exists = lambda suf: os.path.exists(os.path.join(ctx.W, "wu_%s.%s" % (tag, suf)))      # noqa: E731
    p0, mark = ctx.prompts(), ctx.cmd_mark()
    line = "bash %s %s %d 0.02" % (STREAM_SH, tag, n)
    ctx.be.type(line, ctx.rng.randint(10, 25), enter=True)
    if not r.check(bool(ctx.until(lambda: exists("started"), 10)), "the stream program did not start within 10 s of %r" % line):
        ctx.reset()
        return
    plan = resize_plan(60, sizes["rapid"], sizes["rapid_end"])
    t0, sent, errs = time.time(), 0, []
    for i, (w, h) in enumerate(plan[:-1]):
        if time.time() - t0 > 11.0 or exists("done"):          # a slow resize call (macOS System Events): as many as fit, the count is recorded
            break
        time.sleep(max(0.0, t0 + i * (10.0 / 60) - time.time()))
        err = win.resize(w, h)
        sent += 0 if err else 1
        if err:
            errs.append(err)
    err = win.resize(*plan[-1])                               # the size the burst ends on is always sent
    sent += 0 if err else 1
    r.metric("rapid_resizes", dict(planned=60, sent=sent, seconds=round(time.time() - t0, 2), errors=errs[:2]))
    done = bool(ctx.until(lambda: exists("done"), n * 0.06 + 30))
    if not r.check(done, "the stream program did not finish within %d s (%d lines at 20 ms): it is stopped with Ctrl-C" % (n * 0.06 + 30, n)):
        ctx.be.key("ctrl+c")
    CVA.wait_program_exited(ctx, p0)
    if ctx.prof.commands():
        r.check(ctx.await_commands(mark, 1) == [line], "the shell received %r, expected exactly [%r]" % (ctx.cmds_since(mark), line))
    if not sent:
        r.unverifiable("no resize call succeeded during the stream (%s): the rapid-resize half is not judged" % (errs[:1] or ["no error text"])[0])
        return
    got = copy_last_block_output(ctx, r, "rr_copy_stream")           # first: the stream's block is still the newest one
    if got == V.SENTINEL:
        r.unverifiable("the copy-output key left the clipboard unchanged: the stream's block was not selected, its line count is NOT checked")
    elif done:
        err = judge_stream_copy(got, tag, n)
        r.check(err is None, "output printed during %d resizes, copied afterwards: %s" % (sent, err))
    winch = [parse_stty(l) for l in ctx.lines("wu_%s.winch" % tag)]
    r.metric("sigwinch_trap_runs", len(winch))
    r.check(len(winch) >= 1, "the program's SIGWINCH trap never ran although the window was resized %d times while it was in front" % sent)
    now = pane_size(ctx, "rr_rapid")
    final = parse_stty((ctx.lines("wu_%s.final" % tag) or [""])[0])
    r.metric("after_rapid", dict(stty=list(now or ()), program_end=list(final or ()), last_trap=list(winch[-1] or ()) if winch else []))
    if r.check(now is not None, "`stty size` gave no size after the rapid resizes"):
        grid_matches(ctx, r, off, now, "after %d resizes in 10 s" % sent)
        if winch and done and final == now:
            r.check(winch[-1] == now, "the last size the SIGWINCH trap read is %r, the pty ended at %r: the program was not told of the last change" % (winch[-1], now))


def _vim_during_resize(ctx, r, win, off, sizes):
    if not ctx.which("vim"):
        CVA.missing(r, "vim", "package vim")
        return
    ctx.rm("wu_vim.txt")
    p0 = ctx.prompts()
    ctx.be.type("vim -u NONE -N", ctx.rng.randint(10, 25), enter=True)
    time.sleep(1.5)
    w, h = sizes["vim"]
    how, a, _b = apply_size(ctx, win, w, h, off, "rr_vim")
    ctx.be.type(":call writefile([&lines.' '.&columns],'wu_vim.txt')", ctx.rng.randint(10, 25), enter=True)
    ctx.until(lambda: ctx.lines("wu_vim.txt"), 8)
    ctx.shot("rr_vim_asked")
    ctx.be.type(":q!", ctx.rng.randint(10, 25), enter=True)
    CVA.wait_program_exited(ctx, p0)
    seen = parse_stty((ctx.lines("wu_vim.txt") or [""])[0])
    if how is None:
        r.unverifiable("the window did not take %dx%d while vim was open (%s): vim's size after a resize is not judged" % (w, h, a))
        return
    now = pane_size(ctx, "rr_aftervim")
    r.metric("vim", dict(window=[w, h], vim=list(seen or ()), stty=list(now or ())))
    if r.check(seen is not None, "vim wrote no size (wu_vim.txt is %r): the `:call writefile` line did not reach it" % (ctx.lines("wu_vim.txt")[:1],)):
        r.check(seen == now, "vim, open while the window was resized to %dx%d, reports %r lines x columns; the pty is %r" % (w, h, seen, now))
    if now:
        grid_matches(ctx, r, off, now, "after the resize with vim open")


# ---------------------------------------------------------------- T27 context_menu_actions
def _menu_pick(ctx, r, m, at, label, what):
    """right click at `at`, then left click the menu entry `label`, both placed from the app's own position lines -> True when the entry was clicked.
    The entry must be drawn AFTER the right click: an older sighting of the same word is never used."""
    seen = ctx.log_size()
    m.click(at[0], at[1], 3)
    item = wait_hook(ctx, label, seen)
    ctx.shot("cm_menu_%s" % label.split()[-1].lower())
    if not item:
        ptr_check(ctx, r, False, "%s: no %r entry was drawn after the right click at %r (no `appci_find` line for it; shots/*cm_menu_*.png)" % (what, label, tuple(at)))
        ctx.be.key("Escape")
        time.sleep(0.3)
        return False
    m.click(item[1], item[2], 1)
    time.sleep(0.6)
    return True


@case("context_menu_actions", order=71, gui=True)
def c_context_menu_actions(ctx, r):
    """T27 (G35, F13): the right-click menus do what they say. On a block: 'Copy output' puts exactly the block's output on the clipboard, 'Copy command'
    exactly its command line (the clipboard holds a sentinel first). In the input: 'Paste' puts exactly the clipboard text in the input (the app's
    hop=enter_begin at the next Enter), and in a terminal session the shell runs exactly it. The pointer is placed from the app's `appci_find`
    lines only (the printed token, the probe typed into the input, the menu entries): no OCR."""
    m = U.mouse_or_skip(ctx, r)
    sp = U.settings_path(ctx)
    if os.path.isfile(sp) and right_click_pastes(open(sp, encoding="utf-8", errors="replace").read()):
        r.skip("this profile sets terminal.input.right_click_behavior to paste: a right click pastes at once and no context menu exists to test")
    if window_origin(ctx) is None:
        r.skip("the app window's position on screen could not be read (sway / xdotool): the app's position lines cannot be turned into pointer positions")
    out_tok = safe_token(ctx, "cmo")
    U.register_find(ctx, out_tok, "Copy output", "Copy command", "Paste")
    cmd = "printf 'cmo%%s\\n' %s" % out_tok[3:]                 # the command line does not hold the token it prints
    seen, mark = ctx.log_size(), ctx.cmd_mark()
    U.run_cmd(ctx, cmd)
    if ctx.prof.commands():
        r.check(ctx.cmds_since(mark)[-1:] == [cmd], "the shell received %r, expected %r: there is no block to click" % (ctx.cmds_since(mark), cmd))
    time.sleep(0.8)
    w = wait_hook(ctx, out_tok, seen, 4.0)
    if not w:
        r.unverifiable("the app logged no `appci_find` line for the printed token %r (an app without the find hook, or RUST_LOG without "
                       "warpui::appci_find=info): the pointer is not placed by OCR in this case, so the menus are NOT judged" % out_tok)
        U.exact_burst(ctx, r, "cm_after", 3)
        return
    r.metric("block_token_at", list(w[1:3]))
    try:
        # two different points of the token: the second menu then opens somewhere else and its entries are logged anew
        for label, x, want, what in (("Copy output", w[3] + (w[4] - w[3]) // 4, out_tok, "the block's output"),
                                     ("Copy command", w[3] + 3 * (w[4] - w[3]) // 4, cmd, "the block's command")):
            ctx.be.clip_set(V.SENTINEL)
            if not _menu_pick(ctx, r, m, (x, w[2]), label, "right click on the block"):
                continue
            got = V.norm_clip(ctx.be.clip_get())
            msg = "block menu %r: the clipboard holds %r, expected exactly %s %r" % (label, got[:120], what, want)
            if got == V.SENTINEL:
                ptr_check(ctx, r, False, msg + " (the sentinel: nothing was copied)")
            else:
                r.check(got.rstrip("\n") == want, msg)
    finally:
        ctx.be.key("Escape")
        V.back_to_input(ctx, r)
    _input_menu_paste(ctx, r, m)
    ctx.reset()
    U.exact_burst(ctx, r, "cm_after", 3)


def _input_menu_paste(ctx, r, m):
    tok = ctx.tok(7).lower()
    f = "cm_paste.txt"
    text = "echo cmp%s >> %s" % (tok, f)
    ctx.rm(f)
    probe = safe_token(ctx, "cmq")
    U.register_find(ctx, probe)
    seen = ctx.log_size()
    ctx.be.type(probe, 12)
    p = wait_hook(ctx, probe, seen, 4.0)
    ctx.shot("cm_input_probe")
    ctx.be.key("ctrl+u")
    time.sleep(0.4)
    if not p:
        r.unverifiable("the app logged no `appci_find` line for the probe %r typed into the input: the input cannot be located without OCR, its Paste "
                       "entry is NOT judged" % probe)
        return
    ctx.be.clip_set(text)
    if not _menu_pick(ctx, r, m, (p[3] + 6, p[2]), "Paste", "right click in the input"):
        ctx.be.key("ctrl+u")
        return
    ctx.shot("cm_input_pasted")
    off, mark = ctx.log_size(), ctx.cmd_mark()
    ctx.be.key("Return")
    ctx.until(lambda: ctx.lines(f) or CA.enter_buffers(ctx.log_since(off)), 8)
    time.sleep(0.6)
    rows = CA.enter_buffers(ctx.log_since(off))
    if rows:
        buf = rows[-1][0]
        ok = buf == len(text) if isinstance(buf, int) else buf == text
        r.check(ok, "input menu 'Paste': the input held %r at Enter, expected exactly the clipboard text %r" % (buf, text))
    ran = ctx.lines(f) == ["cmp" + tok]
    if ran or ctx.mode == "terminal":
        r.check(ran, "input menu 'Paste' + Enter: %s has %r, expected ['cmp%s']" % (f, ctx.lines(f), tok))
        if ctx.prof.commands():
            got = ctx.await_commands(mark, 1)
            r.check(got == [text], "input menu 'Paste' + Enter: the shell received %r, expected exactly [%r]" % (got, text))
    elif rows and rows[-1][1] == "AI":
        r.note("agent view: the pasted line was sent to the agent (hop=enter_begin input_type AI), the input text at Enter was judged; the request is stopped")
        ctx.be.key("ctrl+c")
        time.sleep(1.2)
    else:
        r.check(False, "input menu 'Paste' + Enter: no hop=enter_begin line and nothing ran (%s is %r): the Paste entry put nothing in the input" % (f, ctx.lines(f)))


# ---------------------------------------------------------------- T28 tab_pointer_use
class TabsLayout:
    """which tab layout the profile shows, and the switch to the other one for half of the case: appearance.vertical_tabs.enabled written to the
    settings file (the live reload ui_settings_live proves), the original bytes written back by restore()."""

    def __init__(self, ctx, r):
        self.ctx, self.changed = ctx, False
        self.path = U.settings_path(ctx)
        if not os.path.isfile(self.path):
            r.skip("settings file %s not found" % self.path)
        self.raw = open(self.path, "rb").read()
        text = self.raw.decode("utf-8", errors="replace")
        st = V.vtabs_state(text)
        if st == "unknown":
            r.skip("appearance.vertical_tabs is written in a form the harness cannot read: which tab layout is on screen is unknown")
        self.vertical = st == "on"
        self.editable = V.vtabs_editable(text)

    def switch(self):
        """show the other layout -> True when the screen changed"""
        before = self.ctx.shot("tp_layout_before")
        U.set_setting(self.ctx, "appearance.vertical_tabs", "enabled", "false" if self.vertical else "true")
        self.changed = True
        after, _d = U.settle_change(self.ctx, before, "tp_layout", 0.004)
        if after is not None:
            self.vertical = not self.vertical
        return after is not None

    def restore(self):
        if self.changed:
            with open(self.path, "wb") as f:
                f.write(self.raw)
            self.changed = False
            time.sleep(1.5)


def pid_alive(pid):
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False
    except OSError:
        return True


def open_tabs_tp(ctx, r, toks):
    """as V.open_tabs, with tokens the caller registered with the find hook BEFORE any title holds them, and each tab's shell pid: tab 1 is the pane the
    case started in; tabs 2..n are opened with the new-tab key and moved to <W>/<token>; one line `echo <token> $$ >> <effect file>` runs in each
    -> (dirs, pids) by tab index; every line is checked exactly (effect file and rec.log)."""
    eff = "tp_tabs.txt"
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
                ctx.be.type("/agent", 12, enter=True)
                ctx.until(lambda: ctx.log_view_mode() == "agent", 10)
                time.sleep(0.6)
            cd = "cd %s" % d
            expect.append(cd)
            ctx.be.type(cd, ctx.rng.randint(10, 25), enter=True)
            ctx.until(lambda: cd in ctx.cmds_since(mark), 6)
        line = "echo %s $$ >> %s%s" % (tok, "../" if i else "", eff)
        expect.append(line)
        ctx.be.type(line, ctx.rng.randint(10, 25), enter=True)
        ctx.until(lambda: len(ctx.lines(eff)) >= i + 1, 10)
        time.sleep(0.3)
    rows = [l.split() for l in ctx.lines(eff)]
    ok = [x[0] for x in rows if x] == toks and all(len(x) == 2 and x[1].isdigit() for x in rows)
    r.check(ok, "one command per tab: the effect file has %r, expected one `<token> <pid>` line for each of %r" % (ctx.lines(eff), toks))
    if ctx.prof.commands():
        have = ctx.await_commands(mark, len(expect))
        r.check(have == expect, "the %d tabs' shells received %r, expected %r" % (len(toks), have, expect))
    return (dirs, [int(x[1]) for x in rows]) if ok else (dirs, None)


def tab_spots(ctx, r, toks, tabs, vertical, since, tag):
    """where the titles of `tabs` (tab numbers, 1-based) are in the tab strip -> ({tab: (cx, cy)}, description) or (None, why). The app's position lines
    since `since` first; a tab the hook never reported is looked for by OCR when tesseract is there."""
    cands = {k: [(w[1], w[2]) for w in hook_rows(ctx, toks[k - 1], since)] for k in tabs}
    if any(not v for v in cands.values()) and shutil.which("tesseract"):
        ws, _p = U.words(ctx, tag)
        for k in tabs:
            if not cands[k]:
                cands[k] = [(h[1], h[2]) for h in U.token_hits(ws, toks[k - 1])]
                if cands[k]:
                    r.note("tab %d's title was located by OCR (no `appci_find` line for %r)" % (k, toks[k - 1]))
    return pick_tab_spots(cands, vertical)


def _tab_round(ctx, r, m, toks, dirs, pids, order, vertical, since, tag):
    """one layout: click the third tab, drag the second onto the third, middle-click the last. `order` = the tab ids (0-based indexes into toks / dirs /
    pids) left to right (top to bottom), the first being the case's own tab -> (the order afterwards, known): known is False when a step failed and
    the real order is no longer certain (the caller then judges nothing more that depends on it)."""
    name = "vertical panel" if vertical else "horizontal bar"
    ctx.be.guard()
    ctx.be.key(R.tab_key(ctx, 1))                              # tab 1 shown: the other tabs' tokens are on screen only in the strip
    time.sleep(1.0)
    spots, desc = tab_spots(ctx, r, toks, [t + 1 for t in order[1:]], vertical, since, "tp_%s" % tag)
    ctx.shot("tp_%s_strip" % tag)
    if not spots:
        r.unverifiable("%s: the tabs' titles could not be located in the strip (%s): the hook saw no title with the tab's directory name, or the layout "
                       "shows neither the directory nor the command; the pointer actions in this layout are NOT judged" % (name, desc))
        return order, True                                      # nothing was clicked: the tabs are as they were
    r.note("%s: %s" % (name, desc))
    slot = [None] + [spots[t + 1] for t in order[1:]]          # slot[i] = where the i-th tab of the strip is (i >= 1)
    # click
    m.click(slot[2][0], slot[2][1], 1)
    time.sleep(0.9)
    got = V.tab_answer(ctx, "%s_click" % tag)
    ptr_check(ctx, r, got == dirs[order[2]], "%s: click on the third tab at %r: the pane that answers is in %r, expected %r" % (name, slot[2], got, dirs[order[2]]))
    # drag the second tab onto the third
    ctx.be.guard()
    ctx.be.key(R.tab_key(ctx, 1))
    time.sleep(0.6)
    m.drag(*drag_points(slot[1], slot[2], vertical))
    time.sleep(0.9)
    ctx.shot("tp_%s_dragged" % tag)
    want = order_after_drag(order)
    seen = []
    for pos in (2, 3):
        ctx.be.guard()
        ctx.be.key(R.tab_key(ctx, pos))
        time.sleep(0.6)
        seen.append(V.tab_answer(ctx, "%s_drag%d" % (tag, pos)))
    moved = seen == [dirs[want[1]], dirs[want[2]]]
    ptr_check(ctx, r, moved, "%s: the second tab dragged onto the third: tabs 2 and 3 now answer from %r, expected %r (they change places)" % (
        name, seen, [dirs[want[1]], dirs[want[2]]]))
    if not moved:
        return order, False
    order = want
    # middle click on the last slot
    victim = order[-1]
    off = ctx.log_size()
    m.click(slot[len(order) - 1][0], slot[len(order) - 1][1], 2)
    gone = bool(ctx.until(lambda: not pid_alive(pids[victim]), 15, step=0.2))
    ctx.shot("tp_%s_closed" % tag)
    if not ptr_check(ctx, r, gone, "%s: middle click on the last tab at %r: the shell of that tab (pid %d) is still running 15 s later" % (
            name, slot[len(order) - 1], pids[victim])):
        return order, False
    order = order[:-1]
    ctx.be.guard()
    ctx.be.key(R.tab_key(ctx, 9))
    time.sleep(0.6)
    cnt = V.window_tab_count(ctx, off)
    if cnt is not None:
        r.check(cnt == len(order), "%s: the window has %d tab(s) after the middle click, expected %d" % (name, cnt, len(order)))
    got = V.tab_answer(ctx, "%s_last" % tag)
    r.check(got == dirs[order[-1]], "%s: after the middle click the last tab answers from %r, expected %r" % (name, got, dirs[order[-1]]))
    others = [t for t in order[1:] if not pid_alive(pids[t])]
    r.check(not others, "%s: the middle click also ended the shell of tab(s) %s" % (name, [toks[t] for t in others]))
    return order, not others


@case("tab_pointer_use", order=87, gui=True)
def c_tab_pointer_use(ctx, r):
    """T28 (G36): tabs by pointer, in both layouts. Four tabs, each in its own directory. In the layout the profile shows: a click on the third tab
    brings it in front (its own `pwd`), the second tab dragged onto the third changes their places (tab number keys 2 and 3, each tab's `pwd`), a
    middle click on the last tab closes it (its shell process is gone, the tab count drops by one, no other shell died). Then the other layout is
    switched on live and the same three actions run on the three tabs left. The profile's settings file gets its bytes back."""
    V.vt_need(ctx, r)
    m = U.mouse_or_skip(ctx, r)
    if window_origin(ctx) is None and not shutil.which("tesseract"):
        r.skip("neither the app window's position (sway / xdotool) nor tesseract: a tab's title cannot be located")
    if not V.first_tab_in_front(ctx, r, "tp"):
        return
    lay = TabsLayout(ctx, r)
    toks = V.vt_tokens(ctx, 4)
    U.register_find(ctx, *toks)                                 # before any title holds a token: a title laid out earlier carries no position
    order = [0, 1, 2, 3]
    opened, pids = False, None
    try:
        since = ctx.log_size()
        opened = True
        dirs, pids = open_tabs_tp(ctx, r, toks)
        if pids is None:
            return
        order, known = _tab_round(ctx, r, m, toks, dirs, pids, order, lay.vertical, since, "a")
        other = "horizontal bar" if lay.vertical else "vertical panel"
        if not known:
            r.note("a step of the first layout failed, so the tab order is no longer certain: the %s is not played" % other)
        elif not lay.editable:
            r.unverifiable("appearance.vertical_tabs is written in a form toml_text cannot edit (an inline table or a dotted key): the %s cannot be "
                           "switched on without rewriting the owner's file and is NOT judged" % other)
        else:
            since = ctx.log_size()
            if r.check(lay.switch(), "the screen did not change within 10 s of writing appearance.vertical_tabs.enabled: the %s did not appear" % other):
                order, _known = _tab_round(ctx, r, m, toks, dirs, pids, order, lay.vertical, since, "b")
        r.check(ctx.app.alive(), "the app died in the tab pointer case")
    finally:
        try:
            if opened:
                # the tabs whose shells still run: without the app's tab count close_extra_tabs sends exactly this many closes, never one more
                V.close_extra_tabs(ctx, r, 1 + (sum(1 for p in pids[1:] if pid_alive(p)) if pids else len(toks) - 1))
        finally:
            lay.restore()
    got = V.tab_answer(ctx, "tp_end")
    r.check(got == R.norm_path(ctx.W), "after the tabs were closed the pane that answers is in %r, expected the first tab (%r)" % (got, R.norm_path(ctx.W)))
    U.exact_burst(ctx, r, "tp_after", 3)


# ---------------------------------------------------------------- T29 focus_cycle
@case("focus_cycle", order=75, gui=True)
def c_focus_cycle(ctx, r):
    """T29 (G37): the keyboard focus leaves and comes back. The reference terminal takes the focus and three lines are typed there; the focus returns to
    Phosphor and a line is typed at once, no settle: the pane's shell receives exactly that line (the first key after the return is not lost) and none
    of the reference's lines, and the app's key trace shows no key while the other window had the focus. POSIX: a raw reader that turned focus
    reporting on (ESC[?1004h) receives exactly ESC[O, then ESC[I, then the key typed after the return, for one away-and-back."""
    if ctx.app is None:
        r.skip("no app process (sim backend)")
    sh = ctx.pane_shell()
    ref = C.ref_terminal(ctx)
    if not ref:
        r.skip("no reference terminal to give the keyboard focus to: %s" % (getattr(ctx, "ref_reason", None) or "unavailable"))
    C.ref_note(ctx, r)
    r.metric("reference", ref.name)
    closed = False
    try:
        time.sleep(2.5)                                         # the reference shell has run its rc (as run_enter_control)
        was = ctx.prof.ref_active
        ctx.prof.ref_active = False                             # the PANE's shell log, not the reference's
        pane_mark, hmark = ctx.cmd_mark(), len(ctx.ps_history_lines())
        ctx.prof.ref_active = was
        tag = ctx.tok(5).lower()
        f_ref, f_back = "fc_ref_%s.txt" % tag, "fc_back_%s.txt" % tag
        ctx.rm(f_ref, f_back)
        ref_toks = [ctx.tok(8).lower() for _ in range(3)]
        off_ref = ctx.log_size()
        for t in ref_toks:
            ctx.be.guard_ref()
            ctx.be.type(ctx.append_cmd(sh, t, f_ref), ctx.rng.randint(10, 25), enter=True)
        ctx.until(lambda: len(ctx.lines(f_ref)) >= len(ref_toks), 12)
        away = ctx.lines(f_ref) == ref_toks
        seg = ctx.log_since(off_ref)                            # what the app logged while every key went to the other window
        tok = ctx.tok(8).lower()
        line = ctx.append_cmd(sh, tok, f_back)
        if ref.switch:
            ref.switch("phosphor")
        else:                                                   # Windows: the harness keeps one target window; closing the reference returns the focus
            ref.close()
            closed = True
        ctx.be.type(line, ctx.rng.randint(10, 25), enter=True)  # at once: the first key after the return is the subject
        ctx.until(lambda: ctx.lines(f_back), 8)
        ctx.shot("fc_back")
        if away:
            keys = C.key_event_audit(seg)
            r.metric("app_keys_while_away", keys["count"])
            r.check(keys["count"] == 0, "the app logged %d key event(s) while every key was typed into the reference terminal (%s): keys typed "
                                        "elsewhere reached Phosphor" % (keys["count"], ref.name))
        else:
            r.unverifiable("the reference terminal (%s) wrote %r for the typed tokens %r: the focus was not provably away from Phosphor, so 'nothing "
                           "typed elsewhere reaches Phosphor' is NOT judged" % (ref.name, ctx.lines(f_ref), ref_toks))
        r.check(ctx.lines(f_back) == [tok], "the line typed at once after the focus returned: %s has %r, expected [%r] (a lost first key changes or "
                                            "drops the line)" % (f_back, ctx.lines(f_back), tok))
        if ctx.prof.commands():
            have = ctx.await_commands(pane_mark, lambda cs: line in cs)
            leaked = [c for c in have if any(t in c for t in ref_toks)]
            r.check(not leaked, "lines typed into the reference terminal reached Phosphor's shell: %r" % leaked[:3])
            mine = [c for c in have if ".appci_shell" not in c]          # macOS: ref_terminal asks the pane which shell it runs, once per session
            r.check(mine == [line], "after the focus returned the pane's shell received %r, expected exactly [%r]" % (mine, line))
        elif ctx.pane == "powershell":
            have = ctx.ps_history_lines()[hmark:]
            r.check(not [c for c in have if any(t in c for t in ref_toks)], "lines typed into the reference console reached the pane's PSReadLine history: %r" % have[:4])
            if have:
                r.check(have == [line], "after the focus returned the PowerShell pane received %r, expected exactly [%r] (PSReadLine history)" % (have, line))
            else:
                r.note("the pane's PSReadLine history has no new line: the command text is not verified, the effect file was checked only")
        if ctx.win or not ref.switch:
            r.metric("focus_reporting", "not run")
            r.note("focus reporting half NOT run on this platform: a console program under ConPTY gets focus changes as FOCUS_EVENT records, not as "
                   "ESC[I / ESC[O bytes, bytes_reader.py needs termios, and the harness cannot switch back to a reference console it already left")
        elif not ctx.which("python3"):
            r.unverifiable("python3 missing on the pane's PATH: the focus reporting half is not judged")
        else:
            _focus_reporting(ctx, r, ref, sh, tag)
    finally:
        if not closed:
            C.close_ref(ref)
        time.sleep(1.0)
        try:
            ctx.be.refocus()
        except Exception:
            pass
    ctx.reset()
    U.exact_burst(ctx, r, "fc_after", 3)


def _focus_reporting(ctx, r, ref, sh, tag):
    rf, f2 = "fc_bytes_%s.txt" % tag, "fc_ref2_%s.txt" % tag
    ctx.rm(rf, rf + ".ready", f2)
    ctx.be.type("printf '\\033[?1004h'; python3 %s %s 60; printf '\\033[?1004l'" % (READER, rf), 12, enter=True)
    if not r.check(bool(ctx.until(lambda: os.path.exists(os.path.join(ctx.W, rf + ".ready")), 20)), "the raw reader never started"):
        ctx.reset()
        return
    time.sleep(0.6)
    n0 = len(U._reader_chunks(ctx, rf))
    chunks = lambda: b"".join(U._reader_chunks(ctx, rf)[n0:])       # noqa: E731
    t = ctx.tok(8).lower()
    try:
        ref.switch("reference")
        time.sleep(0.5)
        ctx.be.guard_ref()
        ctx.be.type(ctx.append_cmd(sh, t, f2), ctx.rng.randint(10, 25), enter=True)
        away = bool(ctx.until(lambda: ctx.lines(f2) == [t], 8))
    finally:
        ref.switch("phosphor")
    ctx.be.type("k", 20)
    ctx.until(lambda: b"k" in chunks(), 6)
    time.sleep(0.4)
    ctx.shot("fc_reader")
    ctx.be.key("ctrl+c")                                        # ends the reader (raw tty: it reads the byte and exits)
    time.sleep(1.6)                                             # past the agent view's exit-confirmation window before reset() sends its own Ctrl-C
    got = chunks()
    r.metric("focus_reporting", got.hex())
    if not away:
        r.unverifiable("the reference terminal did not receive its line while the reader ran: the focus was not provably away, the focus reports "
                       "(%s) are NOT judged" % (got.hex() or "none"))
        return
    err = judge_focus_bytes(got)
    r.check(err is None, "a program that turned focus reporting on, one away-and-back: %s" % err)


# ---------------------------------------------------------------- T30 minimize_fullscreen
@case("minimize_fullscreen", order=76, gui=True)
def c_minimize_fullscreen(ctx, r):
    """T30 (G38): the window goes away and comes back. A program streams numbered lines; after 5 s shown (the control) the window is hidden for
    APPCI_MINIMIZE_S (30) seconds and shown again: a frame is drawn within APPCI_RESTORE_FRAME_MAX_MS (2000), the largest main-thread item of the next
    3 s is within the warn bound or 3x the control's, every line of the stream is in the copied output, `stty size` is unchanged and typing is exact.
    Then fullscreen on and off: `stty size` grows, typing is exact, and the size returns."""
    win = need_window(ctx, r)
    if not V.first_tab_in_front(ctx, r, "mf"):
        return
    size0 = pane_size(ctx, "mf0")
    if not r.check(size0 is not None, "`stty size` gave no size before the window was touched: nothing can be compared"):
        return
    try:
        _hide_under_stream(ctx, r, win, size0)
        _fullscreen_twice(ctx, r, win)
    finally:
        win.restore()
        time.sleep(1.5)
        V.back_to_input(ctx, r)
    end = pane_size(ctx, "mf_end")
    r.check(end == size0, "at the end `stty size` is %r, it was %r before the case" % (end, size0))
    U.exact_burst(ctx, r, "mf_after", 3)
    r.check(ctx.app.alive(), "the app died in the minimise / fullscreen case")


def _hide_under_stream(ctx, r, win, size0):
    hidden_s = max(5.0, float(os.environ.get("APPCI_MINIMIZE_S", "30")))
    iv, shown_s = 0.05, 5.0
    n = int((shown_s + hidden_s + 6.0) / iv)
    tag = safe_token(ctx, "mf")
    write_stream_script(ctx)
    for suf in ("started", "winch", "final", "done"):
        ctx.rm("wu_%s.%s" % (tag, suf))
    exists = lambda suf: os.path.exists(os.path.join(ctx.W, "wu_%s.%s" % (tag, suf)))      # noqa: E731
    p0, mark = ctx.prompts(), ctx.cmd_mark()
    line = "bash %s %s %d %s" % (STREAM_SH, tag, n, iv)
    off = ctx.log_size()
    ctx.be.type(line, ctx.rng.randint(10, 25), enter=True)
    if not r.check(bool(ctx.until(lambda: exists("started"), 10)), "the stream program did not start within 10 s of %r" % line):
        ctx.reset()
        return
    time.sleep(1.0)
    t_ctl = time.time()
    time.sleep(shown_s)
    off_hide, t_hide = ctx.log_size(), time.time()
    how, err = win.hide()
    time.sleep(1.0)
    is_hidden = win.hidden() if not err else False
    r.metric("hide", dict(how=how, error=err, hidden=is_hidden, seconds=hidden_s))
    judged = not err and is_hidden is not False
    if judged:
        time.sleep(hidden_s - 1.0)
    frames_hidden = len(FRAME_ANY_RE.findall(ctx.log_since(off_hide)))
    off_show, t_show = ctx.log_size(), time.time()
    err2 = win.show()
    time.sleep(3.0)
    after = ctx.log_since(off_show)
    ctx.shot("mf_shown_again")
    if not judged:
        r.unverifiable("the window could not be hidden on %s (%s; %s): the stream ran with the window shown, so nothing about a hidden window is "
                       "judged" % (ctx.be.name, how, err or "it stayed visible"))
    else:
        r.check(err2 is None, "the window could not be shown again: %s" % err2)
        ms, n_frames, stamped = first_frame_after(after, t_show)
        limit = float(os.environ.get("APPCI_RESTORE_FRAME_MAX_MS", "2000"))
        r.metric("restore", dict(first_frame_ms=ms, frames_in_3s=n_frames, frames_while_hidden=frames_hidden))
        if not FRAME_ANY_RE.search(ctx.log_since(off)):
            r.unverifiable("the app log has no frame line at all (neither hop=frame_drawn nor frame_cause): the first frame after the return is not judged")
        elif stamped:
            r.check(ms <= limit, "the first frame after the window returned came %.0f ms later (bound %.0f ms) with output still streaming" % (ms, limit))
        else:
            r.check(n_frames > 0, "no frame was drawn in the 3 s after the window returned, with output still streaming")
        items = C.ui_items(ctx.log_since(off))
        ctl_max = C.ui_window_max(items, t_ctl, shown_s, t_end=t_hide)[0]
        res_max = C.ui_window_max(items, t_show, 3.0)[0]
        ok, bound = judge_restore_ui(ctl_max, res_max)
        r.metric("restore_ui_ms", dict(control_max=ctl_max, after_return_max=res_max, bound=bound))
        r.check(ok, "the largest main-thread item in the 3 s after the window returned took %.1f ms; the same stream's largest item while the window was "
                    "shown took %.1f ms (bound %.1f ms = the larger of the %.0f ms warn bound and %.0fx the control)" % (
                        res_max, ctl_max, bound, UI_WARN_MS, RESTORE_CONTROL_FACTOR))
    done = bool(ctx.until(lambda: exists("done"), n * (iv + 0.04) + 30))
    if not r.check(done, "the stream program did not finish (%d lines at %d ms): it is stopped with Ctrl-C" % (n, iv * 1000)):
        ctx.be.key("ctrl+c")
    CVA.wait_program_exited(ctx, p0)
    if ctx.prof.commands():
        r.check(ctx.await_commands(mark, 1) == [line], "the shell received %r, expected exactly [%r]" % (ctx.cmds_since(mark), line))
    got = copy_last_block_output(ctx, r, "mf_copy_stream")
    if got == V.SENTINEL:
        r.unverifiable("the copy-output key left the clipboard unchanged: the stream's block was not selected, its line count is NOT checked")
    elif done:
        err = judge_stream_copy(got, tag, n)
        r.check(err is None, "output printed while the window was %s, copied afterwards: %s" % ("hidden for %.0f s" % hidden_s if judged else "shown", err))
    now = pane_size(ctx, "mf_shown")
    r.check(now == size0, "after the window returned `stty size` is %r, it was %r" % (now, size0))
    U.exact_burst(ctx, r, "mf_after_return", 3)


def _fullscreen_twice(ctx, r, win):
    if win.be == "wayland":
        # a tiled sway window with no border already covers the output: made floating and smaller first, so fullscreen is a visible change
        err = win.resize(1000, 640)
        time.sleep(1.5)
        if err:
            r.unverifiable("the window could not be made smaller before fullscreen (%s): the fullscreen half is not judged" % err)
            return
    before = pane_size(ctx, "mf_fs0")
    if not r.check(before is not None, "`stty size` gave no size before fullscreen"):
        return
    err = win.fullscreen(True)
    time.sleep(2.5)
    V.back_to_input(ctx, r)
    full = pane_size(ctx, "mf_fs1")
    ctx.shot("mf_fullscreen")
    r.metric("fullscreen", dict(before=list(before), full=list(full or ()), error=err))
    if full == before and win.be == "x11" and not win.has_wm():
        ctx.be.key("F11")                                       # the app's own fullscreen state goes back to off
        time.sleep(1.0)
        r.unverifiable("fullscreen changed nothing and the X display has no window manager: the app's fullscreen request (_NET_WM_STATE_FULLSCREEN) is "
                       "carried out by a manager, which the job's Xvfb does not run; the fullscreen half is NOT judged on this display")
        return
    grew = bool(full) and full[0] >= before[0] and full[1] >= before[1] and full != before
    if not r.check(grew, "fullscreen on%s: `stty size` went from %r to %r, expected more rows or columns and no fewer of either" % (
            " (%s)" % err if err else "", before, full)):
        return                                                  # Window.restore() leaves fullscreen
    U.exact_burst(ctx, r, "mf_fullscreen", 3)
    win.fullscreen(False)
    time.sleep(2.5)
    V.back_to_input(ctx, r)
    back = pane_size(ctx, "mf_fs2")
    r.check(back == before, "fullscreen off: `stty size` is %r, it was %r before fullscreen" % (back, before))
    U.exact_burst(ctx, r, "mf_fullscreen_off", 3)
