# appci/backend.py: input / screenshot / clipboard backends for the CI real-app harness.
# Backends: wayland (wtype + grim + wl-clipboard, for headless sway; also used for apps under XWayland),
#           x11 (xdotool + xsel + ffmpeg x11grab, private Xvfb), mac (cliclick + osascript + screencapture),
#           sim (a bash in a pty: no GUI, exercises the case logic only; `run.py --backend sim`).
# Every backend exposes the same methods; keys are written as 'ctrl+shift+t', 'Return', 'Tab', 'Left', 'a'.
# Prereqs per backend are checked by Backend.preflight(), which returns a list of problems (empty = ok).
import json, os, shutil, subprocess, sys, time

ENVNAME = {" ": "space", ">": "greater", "<": "less", "_": "underscore", "-": "minus", ".": "period",
           "/": "slash", "|": "bar", "$": "dollar", "'": "apostrophe", '"': "quotedbl", "&": "ampersand",
           "(": "parenleft", ")": "parenright", ";": "semicolon", ":": "colon", "=": "equal", "~": "asciitilde",
           "\\": "backslash", ",": "comma", "*": "asterisk", "#": "numbersign", "%": "percent", "@": "at",
           "!": "exclam", "?": "question", "+": "plus", "[": "bracketleft", "]": "bracketright",
           "{": "braceleft", "}": "braceright", "^": "asciicircum", "`": "grave"}
MODS = {"ctrl": "ctrl", "control": "ctrl", "shift": "shift", "alt": "alt", "super": "logo", "cmd": "logo", "meta": "logo"}


def split_key(spec):
    """'ctrl+shift+t' -> (['ctrl','shift'], 't'). A literal '+' key is written 'plus'."""
    parts = spec.split("+")
    return [MODS[p.lower()] for p in parts[:-1]], parts[-1]


class Backend:
    name = "base"
    key_floor_ms = 0.0   # measured cost of one input-tool invocation (harness overhead), filled by calibrate()

    def preflight(self):
        return []

    def type(self, text, delay_ms=15, enter=False):
        raise NotImplementedError

    def key(self, spec, repeat=1, delay_ms=15):
        raise NotImplementedError

    def roll(self, text, overlap_ms=10, enter=True):
        raise NotImplementedError

    def clip_set(self, data):
        raise NotImplementedError

    def clip_get(self):
        raise NotImplementedError

    def paste(self, enter=False):
        self.keys(["ctrl+shift+v"] + (["Return"] if enter else []), delay_ms=0)

    def keys(self, specs, delay_ms=15):
        for s in specs:
            self.key(s, delay_ms=delay_ms)

    def shot(self, path):
        raise NotImplementedError

    def windows(self):
        """list of descriptions of top-level app windows (best effort; [] = unknown)"""
        return []

    def guard(self):
        """raise RuntimeError when keys must not be sent now (wrong foreground app)"""

    def calibrate(self, n=15):
        ts = []
        for _ in range(n):
            t = time.perf_counter()
            self.noop()
            ts.append((time.perf_counter() - t) * 1000)
        ts.sort()
        self.key_floor_ms = ts[len(ts) // 2]
        return self.key_floor_ms

    def noop(self):
        pass


def _run(argv, **kw):
    return subprocess.run(argv, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, **kw)


class WaylandBackend(Backend):
    """wtype (zwp_virtual_keyboard_v1; sway supports it), grim (screencopy), wl-copy/wl-paste.
    Everything addresses the compositor in WAYLAND_DISPLAY / XDG_RUNTIME_DIR of the process env.
    wtype -d N is the delay between keystrokes: one invocation types a whole string at that cadence,
    and trailing args (-k Return) follow without a spawn gap, so 'type then instant Enter' is one call."""
    name = "wayland"

    def __init__(self):
        self.batch = os.environ.get("APPCI_WL_KEY_MODE", "batch")  # batch | perkey (spawn wtype per character)

    def preflight(self):
        p = []
        for t in ("wtype", "grim", "wl-copy", "wl-paste", "swaymsg"):
            if not shutil.which(t):
                p.append("missing tool: " + t)
        for v in ("WAYLAND_DISPLAY", "XDG_RUNTIME_DIR"):
            if not os.environ.get(v):
                p.append("env not set: " + v)
        return p

    def _wtype(self, args):
        return _run(["wtype"] + args)

    def type(self, text, delay_ms=15, enter=False):
        if text.startswith("-"):
            raise ValueError("wtype text must not start with '-'")
        if self.batch == "perkey":
            for ch in text:
                self._wtype(["-k", ENVNAME.get(ch, ch)] if not ch.isalnum() else [ch])
                time.sleep(delay_ms / 1000)
            if enter:
                self.key("Return")
            return
        args = ["-d", str(int(delay_ms)), text]
        if enter:
            args += ["-k", "Return"]
        self._wtype(args)

    def _key_args(self, spec):
        mods, k = split_key(spec)
        a = []
        for m in mods:
            a += ["-M", m]
        a += ["-k", k]
        for m in reversed(mods):
            a += ["-m", m]
        return a

    def key(self, spec, repeat=1, delay_ms=15):
        a = []
        for i in range(repeat):
            a += self._key_args(spec)
            if delay_ms and i + 1 < repeat:
                a += ["-s", str(int(delay_ms))]
        self._wtype(a)

    def keys(self, specs, delay_ms=15):
        a = []
        for i, s in enumerate(specs):
            a += self._key_args(s)
            if delay_ms and i + 1 < len(specs):
                a += ["-s", str(int(delay_ms))]
        self._wtype(a)

    def roll(self, text, overlap_ms=10, enter=True):
        """Rollover: the next key goes down before the previous one is released."""
        a, prev = [], None
        for ch in text:
            k = ENVNAME.get(ch, ch)
            a += ["-P", k, "-s", str(int(overlap_ms))]
            if prev:
                a += ["-p", prev]
            a += ["-s", str(int(overlap_ms))]
            prev = k
        if prev:
            a += ["-p", prev]
        if enter:
            a += ["-k", "Return"]
        self._wtype(a)

    def noop(self):
        self._wtype(["-M", "shift", "-m", "shift"])

    def clip_set(self, data):
        if isinstance(data, str):
            data = data.encode()
        subprocess.run(["wl-copy"], input=data, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=30)

    def clip_get(self):
        r = subprocess.run(["wl-paste", "-n"], capture_output=True, timeout=10)
        return r.stdout

    def shot(self, path):
        _run(["grim", path], timeout=30)
        return os.path.exists(path) and os.path.getsize(path) > 1000

    def windows(self):
        try:
            r = subprocess.run(["swaymsg", "-t", "get_tree"], capture_output=True, timeout=10)
            tree = json.loads(r.stdout)
        except Exception:
            return []
        out = []

        def walk(n):
            if n.get("type") in ("con", "floating_con") and not n.get("nodes") and (n.get("app_id") or n.get("window_properties")):
                wp = n.get("window_properties") or {}
                out.append({"app_id": n.get("app_id") or wp.get("class"), "pid": n.get("pid"), "name": n.get("name")})
            for c in n.get("nodes", []) + n.get("floating_nodes", []):
                walk(c)
        walk(tree)
        return out


class X11Backend(Backend):
    name = "x11"

    def preflight(self):
        p = [("missing tool: " + t) for t in ("xdotool", "xsel") if not shutil.which(t)]
        if not shutil.which("ffmpeg") and not shutil.which("import"):
            p.append("missing tool: ffmpeg or ImageMagick import (screenshots)")
        if not os.environ.get("DISPLAY"):
            p.append("env not set: DISPLAY")
        return p

    def type(self, text, delay_ms=15, enter=False):
        _run(["xdotool", "type", "--delay", str(int(delay_ms)), "--", text])
        if enter:
            _run(["xdotool", "key", "--delay", "0", "Return"])

    def key(self, spec, repeat=1, delay_ms=15):
        mods, k = split_key(spec)
        x = "+".join(mods + [k])
        _run(["xdotool", "key", "--delay", str(int(delay_ms))] + [x] * repeat)

    def keys(self, specs, delay_ms=15):
        _run(["xdotool", "key", "--delay", str(int(delay_ms))] + ["+".join(split_key(s)[0] + [split_key(s)[1]]) for s in specs])

    def roll(self, text, overlap_ms=10, enter=True):
        cmd, prev = ["xdotool"], None
        for ch in text:
            k = ENVNAME.get(ch, ch)
            cmd += ["keydown", k, "sleep", str(overlap_ms / 1000)]
            if prev:
                cmd += ["keyup", prev]
            cmd += ["sleep", str(overlap_ms / 1000)]
            prev = k
        if prev:
            cmd += ["keyup", prev]
        if enter:
            cmd += ["key", "Return"]
        _run(cmd)

    def noop(self):
        _run(["xdotool", "keyup", "shift"])

    def guard(self):
        """no window manager in Xvfb: focus (and point at) the newest visible Phosphor window before keys go out"""
        r = subprocess.run(["xdotool", "search", "--onlyvisible", "--class", "phosphor"], capture_output=True)
        ids = r.stdout.decode().split()
        if not ids:
            raise RuntimeError("no visible Phosphor window on " + os.environ.get("DISPLAY", "?"))
        _run(["xdotool", "windowfocus", ids[-1]])

    def clip_set(self, data):
        if isinstance(data, str):
            data = data.encode()
        subprocess.run(["xsel", "-b", "-i"], input=data, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=30)

    def clip_get(self):
        return subprocess.run(["xsel", "-b", "-o"], capture_output=True, timeout=10).stdout

    def shot(self, path):
        d = os.environ.get("DISPLAY")
        if shutil.which("import"):
            _run(["import", "-window", "root", path], timeout=30)
        else:
            _run(["ffmpeg", "-loglevel", "error", "-y", "-f", "x11grab", "-i", d, "-frames:v", "1", path], timeout=30)
        return os.path.exists(path) and os.path.getsize(path) > 1000

    def windows(self):
        r = subprocess.run(["xdotool", "search", "--onlyvisible", "--class", "phosphor"], capture_output=True)
        return [{"id": w} for w in r.stdout.decode().split()]


class MacBackend(Backend):
    """cliclick types (t:), osascript sends Return / chords (cliclick kp:return does not deliver).
    Needs the runner's process chain to hold Accessibility and Screen Recording; guard() refuses
    to type unless Phosphor is the frontmost app (a locked screen / wrong app takes keys otherwise)."""
    name = "mac"
    APP = os.environ.get("APPCI_MAC_PROCESS", "phosphor-oss")
    KC = {"Return": 36, "Tab": 48, "Escape": 53, "BackSpace": 51, "Left": 123, "Right": 124, "Down": 125, "Up": 126, "Home": 115, "End": 119}
    FORBIDDEN = ("ctrl+cmd", "cmd+q", "cmd+w", "cmd+tab", "cmd+space")  # incident rules in docs/testing.md 3.3

    def preflight(self):
        p = []
        cl = shutil.which("cliclick", path="/opt/homebrew/bin:" + os.environ.get("PATH", ""))
        if not cl:
            p.append("missing tool: cliclick")
        self.cliclick = cl
        t = "/tmp/appci_pf_%d.png" % os.getpid()
        _run(["screencapture", "-x", t])
        if not os.path.exists(t) or os.path.getsize(t) < 5000:
            p.append("screencapture produced no usable image: runner is not in a GUI session or lacks Screen Recording")
        else:
            os.remove(t)
        r = subprocess.run(["osascript", "-e", 'tell application "System Events" to get name of first process whose frontmost is true'], capture_output=True)
        if r.returncode != 0:
            p.append("osascript/System Events denied: grant Accessibility to the runner's process chain (%s)" % r.stderr.decode().strip()[:120])
        return p

    def _front(self):
        r = subprocess.run(["osascript", "-e", 'tell application "System Events" to get name of first process whose frontmost is true'], capture_output=True)
        return r.stdout.decode().strip()

    def guard(self):
        f = self._front()
        want = getattr(self, "guard_override", None)      # set while the Terminal.app reference window is being driven
        if want:
            if want.lower() not in f.lower():
                raise RuntimeError("frontmost app is %r, not %s: refusing to send keys" % (f, want))
        elif self.APP.lower() not in f.lower() and "phosphor" not in f.lower():
            raise RuntimeError("frontmost app is %r, not Phosphor: refusing to send keys" % f)

    def refocus(self):
        """bring Phosphor back to the front after the reference window closed (System Events frontmost; no keys sent)"""
        subprocess.run(["osascript", "-e", 'tell application "System Events" to set frontmost of first process whose name is "%s" to true' % self.APP],
                       capture_output=True, timeout=30)
        time.sleep(0.5)

    def type(self, text, delay_ms=15, enter=False):
        self.guard()
        _run([self.cliclick, "-w", str(int(delay_ms)), "t:" + text])
        if enter:
            self.key("Return")

    def key(self, spec, repeat=1, delay_ms=15):
        self.guard()
        low = spec.lower()
        if any(f in low for f in self.FORBIDDEN):
            raise ValueError("refusing forbidden chord " + spec)
        mods, k = split_key(spec)
        using = {"ctrl": "control down", "shift": "shift down", "alt": "option down", "logo": "command down"}
        mm = ", ".join(using[m] for m in mods)
        for _ in range(repeat):
            if k in self.KC:
                code = 'key code %d' % self.KC[k]
            else:
                code = 'keystroke "%s"' % k
            _run(["osascript", "-e", 'tell application "System Events" to %s%s' % (code, (" using {%s}" % mm) if mm else "")])
            time.sleep(delay_ms / 1000)

    def keys(self, specs, delay_ms=15):
        for s in specs:
            self.key(s, delay_ms=delay_ms)

    def roll(self, text, overlap_ms=10, enter=True):
        self.type(text, delay_ms=max(5, overlap_ms), enter=enter)  # no key overlap through cliclick

    def paste(self, enter=False):
        self.key("cmd+v")
        if enter:
            self.key("Return")

    def noop(self):
        _run(["osascript", "-e", "return 1"])

    def clip_set(self, data):
        if isinstance(data, str):
            data = data.encode()
        subprocess.run(["pbcopy"], input=data, timeout=30)

    def clip_get(self):
        return subprocess.run(["pbpaste"], capture_output=True).stdout

    def shot(self, path):
        _run(["screencapture", "-x", path], timeout=30)
        return os.path.exists(path) and os.path.getsize(path) > 5000


class SimBackend(Backend):
    """A real bash in a pty standing in for the GUI, so the case logic, exact-command checks and the
    JSON report can be exercised without a display. Keys it cannot model (tabs, windows) are no-ops
    and the cases that need them skip on sim (needs_gui)."""
    name = "sim"
    needs_gui_skip = True
    SEQ = {"Return": "\r", "Tab": "\t", "Escape": "\x1b", "BackSpace": "\x7f", "Left": "\x1b[D", "Right": "\x1b[C",
           "Up": "\x1b[A", "Down": "\x1b[B", "Home": "\x01", "End": "\x05", "ctrl+c": "\x03", "ctrl+d": "\x04",
           "ctrl+u": "\x15", "ctrl+w": "\x17", "ctrl+l": "\x0c"}

    def __init__(self, rcfile, home, env):
        import pty
        self.clip = b""
        pid, fd = pty.fork()
        if pid == 0:
            os.chdir(os.path.join(home, "w"))
            e = dict(os.environ)
            e.update(env)
            os.execvpe("bash", ["bash", "--rcfile", rcfile, "-i"], e)
        self.pid, self.fd = pid, fd
        import threading
        self.buf = bytearray()

        def pump():
            while True:
                try:
                    d = os.read(fd, 65536)
                except OSError:
                    return
                if not d:
                    return
                self.buf += d
        threading.Thread(target=pump, daemon=True).start()

    def _w(self, s):
        os.write(self.fd, s.encode() if isinstance(s, str) else s)

    def type(self, text, delay_ms=15, enter=False):
        for ch in text:
            self._w(ch)
            time.sleep(delay_ms / 1000)
        if enter:
            self._w("\r")

    def key(self, spec, repeat=1, delay_ms=15):
        for _ in range(repeat):
            s = self.SEQ.get(spec) or self.SEQ.get(spec.lower())
            if s is None and "+" not in spec:
                s = spec
            if s:
                self._w(s)
            time.sleep(delay_ms / 1000)

    def keys(self, specs, delay_ms=15):
        for s in specs:
            self.key(s, delay_ms=delay_ms)

    def roll(self, text, overlap_ms=10, enter=True):
        self.type(text, overlap_ms, enter)

    def paste(self, enter=False):
        self._w(b"\x1b[200~" + self.clip + b"\x1b[201~")
        if enter:
            self._w("\r")

    def clip_set(self, data):
        self.clip = data.encode() if isinstance(data, str) else data

    def clip_get(self):
        return self.clip

    def shot(self, path):
        open(path.replace(".png", ".txt"), "wb").write(bytes(self.buf[-4000:]))
        return True

    def close(self):
        try:
            os.kill(self.pid, 15)
            os.waitpid(self.pid, 0)
        except Exception:
            pass


def make(name, **kw):
    if name == "wayland":
        return WaylandBackend()
    if name == "x11":
        return X11Backend()
    if name == "mac":
        return MacBackend()
    if name == "sim":
        return SimBackend(**kw)
    raise SystemExit("unknown backend " + name)
