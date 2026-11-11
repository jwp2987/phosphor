# appci/backend.py: input / screenshot / clipboard backends for the CI real-app harness.
# Backends: wayland (wtype + grim + wl-clipboard, for headless sway; also used for apps under XWayland),
#           x11 (xdotool + xsel + ffmpeg x11grab, private Xvfb), mac (htype2 + kl3 CGEvent tools + osascript + screencapture),
#           windows (SendInput VK/Unicode events + user32 focus guard + PIL.ImageGrab + user32 clipboard, ctypes only),
#           sim (a bash in a pty: no GUI, exercises the case logic only; `run.py --backend sim`).
# Every backend exposes the same methods; keys are written as 'ctrl+shift+t', 'Return', 'Tab', 'Left', 'a'.
# Prereqs per backend are checked by Backend.preflight(), which returns a list of problems (empty = ok).
import re
import json, os, shutil, subprocess, sys, time

ENVNAME = {" ": "space", ">": "greater", "<": "less", "_": "underscore", "-": "minus", ".": "period",
           "/": "slash", "|": "bar", "$": "dollar", "'": "apostrophe", '"': "quotedbl", "&": "ampersand",
           "(": "parenleft", ")": "parenright", ";": "semicolon", ":": "colon", "=": "equal", "~": "asciitilde",
           "\\": "backslash", ",": "comma", "*": "asterisk", "#": "numbersign", "%": "percent", "@": "at",
           "!": "exclam", "?": "question", "+": "plus", "[": "bracketleft", "]": "bracketright",
           "{": "braceleft", "}": "braceright", "^": "asciicircum", "`": "grave"}
REV_ENVNAME = {v: k for k, v in ENVNAME.items()}
MODS = {"ctrl": "ctrl", "control": "ctrl", "shift": "shift", "alt": "alt", "super": "logo", "cmd": "logo", "meta": "logo"}


def split_key(spec):
    """'ctrl+shift+t' -> (['ctrl','shift'], 't'). A literal '+' key is written 'plus'."""
    parts = spec.split("+")
    return [MODS[p.lower()] for p in parts[:-1]], parts[-1]


class Backend:
    name = "base"
    key_floor_ms = 0.0   # measured cost of one input-tool invocation (harness overhead), filled by calibrate()

    inject_log = None        # path of <run>/inject.log (set by Ctx); Mac, Windows and (since #1509) Wayland/X11 record injector calls
    calls = None             # per-injector-call records, in order; None = this backend cannot say what it posted

    def preflight(self):
        return []

    def inject_enter(self, idx):
        """what the injector says about the Enter of the idx-th recorded call: (posted: bool, text) or None when unknown"""
        return None

    def record_injection(self, argv, t0, t1, err=None):
        """Linux (#1509): one record per logical injector call, in the shape the Mac/Windows backends keep (chars, special, enters_*), in self.calls
        and <run>/inject.log. Typed characters are a COUNT only; named keys are logged in full. `ref` marks keys aimed at a reference terminal
        (guard_override), which the app-side join must not expect Phosphor to have seen."""
        if self.calls is None:
            return
        p = parse_injector(argv)
        enters = [k for k in p["keys"] if k.lower() in ("return", "kp_enter")]
        ok = err is None
        rec = dict(n=len(self.calls), wall0=t0, wall1=t1, enters_want=len(enters), enters_posted=len(enters) if ok else 0,
                   enter_wall=[t0] * len(enters) if ok else [], chars=p["chars"], special=["%s@%.3f" % (k, t0) for k in p["keys"]], pastes=count_pastes(argv),
                   err=err, out=True, span_ms=(t1 - t0) * 1000, ref=bool(getattr(self, "guard_override", None)), tool=argv[0])
        self.calls.append(rec)
        if self.inject_log:
            try:
                with open(self.inject_log, "a") as f:
                    f.write("call=%d tool=%s wall=%.3f..%.3f chars=%d span_ms=%.0f enters_want=%d enters_posted=%d out=True%s%s %s\n" % (
                        rec["n"], rec["tool"], t0, t1, rec["chars"], rec["span_ms"], rec["enters_want"], rec["enters_posted"],
                        (" ERR=" + err) if err else "", " REF" if rec["ref"] else "", " ".join(rec["special"])))
            except OSError:
                pass

    def run_recorded(self, cmds):
        """run the injector commands of ONE logical call (xdotool type + key Return is two processes) and record them as one call"""
        t0 = time.time()
        err = None
        try:
            for c in cmds:
                _run(c)
        except Exception as e:
            err = "%s: %s" % (type(e).__name__, str(e)[:120])
            raise
        finally:
            self.record_injection(cmds[0] + [t for c in cmds[1:] for t in c[1:]], t0, time.time(), err)

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

    def type_lines(self, lines, delay_ms=15, gap_ms=300):
        """Type every line and its Enter. Default: one type() per line; the Wayland backend sends them in ONE tool call."""
        for l in lines:
            self.type(l, delay_ms, enter=True)
            time.sleep(gap_ms / 1000)

    def shot(self, path):
        raise NotImplementedError

    def windows(self):
        """list of descriptions of top-level app windows (best effort; [] = unknown)"""
        return []

    def guard(self):
        """raise RuntimeError when keys must not be sent now (wrong foreground app)"""

    def refocus(self, verify=False):
        """bring Phosphor back to the front (no keys sent); verify=True raises RuntimeError unless it demonstrably has the focus. No-op on
        backends that do nothing between injections (sim)."""

    def guard_ref(self):
        """Linux only (#1500): while a reference terminal is the injection target, refuse (RuntimeError) unless IT has the focus. Called by the
        sampling loops BEFORE a timed burst, never inside one, so it adds nothing to either terminal's measurement. No-op elsewhere
        (macOS and Windows verify the frontmost process inside every injection)."""

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


def focus_matches(want, got):
    """want = dict(pids=set of int, names=set of lowercase app_id/class); got = dict(pid, app_id, class) of the focused window or None.
    True only when the focused window is the reference: its PID, or (for a client that reports no PID) its app_id / X11 class."""
    if not got:
        return False
    if got.get("pid") in want.get("pids", ()):
        return True
    return (got.get("app_id") or got.get("class") or "").lower() in want.get("names", ())


MODIFIER_KEYS = {"shift", "control", "ctrl", "alt", "super", "logo", "meta", "cmd", "hyper", "caps_lock", "iso_level3_shift", "altgr"}


def is_modifier(name):
    n = name.lower()
    for sfx in ("_l", "_r"):
        if n.endswith(sfx):
            n = n[:-2]
    return n in MODIFIER_KEYS


def count_pastes(argv):
    """How many paste chords (ctrl+v, ctrl+shift+v, shift+Insert) one injector invocation posts. A paste is ONE named key in the accounting
    (parse_injector drops the modifiers) but the app logs no per-character key events for it, so injacct must not expect it (r18d8: the
    paste-only rows read 'sent 14; app saw 0')."""
    n = 0
    if not argv:
        return 0
    if argv[0] == "wtype":
        mods = set()
        for i, t in enumerate(argv[:-1]):
            nx = argv[i + 1]
            if t == "-M":
                mods.add(nx.lower())
            elif t == "-m":
                mods.discard(nx.lower())
            elif t == "-k":
                k = nx.lower()
                if (k == "v" and "ctrl" in mods) or (k == "insert" and "shift" in mods):
                    n += 1
    elif argv[0] == "xdotool":
        for t in argv[1:]:
            parts = t.lower().split("+")
            if len(parts) > 1 and ((parts[-1] == "v" and "ctrl" in parts[:-1]) or (parts[-1] == "insert" and "shift" in parts[:-1])):
                n += 1
    return n


def parse_injector(argv):
    """What one wtype / xdotool invocation (xdotool may chain verbs) asks for: {"chars": n typed characters (a COUNT, never the text: typed text
    can be a password), "keys": [names of non-modifier keys pressed, in order]}. Modifier taps (the wtype priming Shift_L) are not keys (#1509)."""
    chars, keys = 0, []
    if not argv:
        return dict(chars=0, keys=[])
    if argv[0] == "wtype":
        a, i = argv[1:], 0
        while i < len(a):
            t = a[i]
            if t in ("-d", "-s", "-M", "-m", "-p"):
                i += 2
            elif t in ("-k", "-P"):
                if i + 1 < len(a) and not is_modifier(a[i + 1]):
                    keys.append(a[i + 1])
                i += 2
            elif t == "--":
                i += 1
            elif t.startswith("-") and len(t) == 2:
                i += 1
            else:
                chars += len(t)
                i += 1
    elif argv[0] == "xdotool":
        verbs = {"type", "key", "keydown", "keyup", "sleep", "windowfocus", "search", "getwindowfocus"}
        a, i = argv[1:], 0
        while i < len(a):
            v = a[i]
            i += 1
            if v == "type":
                while i < len(a) and a[i] in ("--delay", "--window", "--args"):
                    i += 2
                if i < len(a) and a[i] == "--":
                    i += 1
                if i < len(a):
                    chars += len(a[i])
                    i += 1
            elif v == "key":
                while i < len(a) and a[i] not in verbs:
                    if a[i] in ("--delay", "--window", "--repeat"):
                        i += 2
                    elif a[i].startswith("--"):
                        i += 1
                    else:
                        last = a[i].split("+")[-1] or "plus"
                        if not is_modifier(last):
                            keys.append(last)
                        i += 1
            elif v == "keydown":
                if i < len(a) and not is_modifier(a[i]):
                    keys.append(a[i])
                i += 1
            elif v in ("keyup", "sleep"):
                i += 1
    return dict(chars=chars, keys=keys)


INJECT_TIMEOUT_S = 120      # a wedged wtype/xdotool must not hang the case (#1508)


def is_injector(argv):
    """True for a command that injects keys: wtype, and xdotool type/key/keyup (not windowfocus etc.)"""
    return bool(argv) and (argv[0] == "wtype" or (argv[0] == "xdotool" and len(argv) > 1 and argv[1] in ("type", "key", "keyup")))


KL_NAMED = {"DEL": "KEY51", "TAB": "KEY48", "ESC": "KEY53", "CMDP": "KEY35", "CMDK": "KEY40"}      # kl3.swift keyFor: its key names -> htype2's (virtual key codes)
KL_CMD = ("CMDP", "CMDK")                                                                           # the two it posts with Command held


def kl_keys(line):
    """pure: the key-downs ONE kl3 command posts, as (typed characters, [named keys], Cmd chords) in htype2's vocabulary (a Return is KEY36, which
    injacct._enters_keyed counts as an Enter; a Cmd chord may be a menu's, injacct.join). kl3.swift: `K <cap> <key>` one key (keyFor: a name of KL_NAMED,
    AT = shift-2, else the first character); `R <cap>` a Return; `T|F <cap> <text>` the text then a Return; `W <text>` the text; `C ...` and `Q` post
    nothing. An unknown command posts nothing as far as this record knows."""
    p = line.split(" ", 2)
    cmd = p[0] if p else ""
    if cmd == "K" and len(p) == 3 and p[2]:
        if p[2] in KL_NAMED:
            return 0, [KL_NAMED[p[2]]], int(p[2] in KL_CMD)
        return 1, [], 0
    if cmd == "R":
        return 0, ["KEY36"], 0
    if cmd in ("T", "F") and len(p) == 3:
        return len(p[2]), ["KEY36"], 0
    if cmd == "W" and len(line) > 2:
        return len(line[2:]), [], 0
    return 0, [], 0


def kl_posted(out):
    """pure: did kl3 say it posted? Its answer to a key command carries post0=<epoch> (K, R, T, F) or is `W done`; an ERR / BADCHAR / empty line did not."""
    return bool(re.search(r"post0=[\d.]+", out)) or out.startswith("W done")


STDERR_LOG = None        # path of the injector stderr log (<run>/logs/injector-stderr.log), set by Ctx next to inject_log


def _tail(txt, n=240):
    txt = (txt or "").strip().replace("\n", " | ")
    return txt[-n:]


def _log_stderr(tool, rc, err):
    """a helper tool's stderr into the injector stderr log (next to inject.log)"""
    err = (err or b"").decode("utf-8", "replace") if isinstance(err, bytes) else (err or "")
    if STDERR_LOG:
        try:
            with open(STDERR_LOG, "a") as f:
                f.write("%.3f %s exit=%d: %s\n" % (time.time(), tool, rc, _tail(err, 600)))
        except OSError:
            pass


def _run(argv, **kw):
    inj = is_injector(argv)
    if inj:
        kw.setdefault("timeout", INJECT_TIMEOUT_S)
        # r18d8: stderr went to /dev/null, so 'wtype exit 1' had no cause. Keep it: in the log next to inject.log and in the error text.
        r = subprocess.run(argv, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, **kw)
        err = (r.stderr or b"").decode("utf-8", "replace")
        if err.strip() and STDERR_LOG:
            try:
                with open(STDERR_LOG, "a") as f:
                    f.write("%.3f %s exit=%d: %s\n" % (time.time(), argv[0], r.returncode, _tail(err, 600)))
            except OSError:
                pass
        if r.returncode != 0:      # was discarded: a failed injection made the case judge keys that were never typed (#1508)
            raise RuntimeError("key injector failed: %s exit %d%s" % (argv[0], r.returncode, (" (stderr: %s)" % _tail(err)) if err.strip() else " (no stderr)"))
        return r
    return subprocess.run(argv, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, **kw)


class WaylandBackend(Backend):
    """wtype (zwp_virtual_keyboard_v1; sway supports it), grim (screencopy), wl-copy/wl-paste.
    Everything addresses the compositor in WAYLAND_DISPLAY / XDG_RUNTIME_DIR of the process env.
    wtype -d N is the delay between keystrokes: one invocation types a whole string at that cadence,
    and trailing args (-k Return) follow without a spawn gap, so 'type then instant Enter' is one call."""
    name = "wayland"

    def __init__(self):
        self.batch = os.environ.get("APPCI_WL_KEY_MODE", "batch")  # batch | perkey (spawn wtype per character)
        # #1414: every wtype call uploads its own keymap (new virtual keyboard) and under headless sway the FIRST key after it never
        # reached the client. The call is therefore led by a harmless Shift_L tap + a short sleep; the key_loss_control case measures
        # primed vs unprimed vs one-shot vs a reference terminal. APPCI_WL_PRIME_MS=0 turns the priming off.
        self.prime_ms = int(os.environ.get("APPCI_WL_PRIME_MS", "20"))
        # #1414: wayland_session.sh keeps one virtual keyboard alive for the session (a sleeping wtype) and exports how it went:
        # "persistent" (the seat always has a keyboard; every wtype call is a second device), "percall" (none held: the old behaviour,
        # a keyboard created and destroyed per call) or unset when the backend runs outside that script. Reported by key_loss_control.
        self.injector_mode = os.environ.get("APPCI_WL_INJECTOR", "unknown")
        self.calls = []       # injector accounting (#1509)

    def preflight(self):
        p = []
        for t in ("wtype", "grim", "wl-copy", "wl-paste", "swaymsg"):
            if not shutil.which(t):
                p.append("missing tool: " + t)
        for v in ("WAYLAND_DISPLAY", "XDG_RUNTIME_DIR"):
            if not os.environ.get(v):
                p.append("env not set: " + v)
        return p

    guard_override = None     # dict(pids=..., names=...) while a reference terminal is driven (#1500)
    pid_fn = None             # set by run.py: the PID of the Phosphor instance under test (#1514: focusing it between interleaved latency blocks)

    def refocus(self, verify=False):
        """#1514: focus the Phosphor window (sway `[pid=N] focus`, no keys sent). verify=True waits up to 3 s for sway to report it focused."""
        ph = self.pid_fn() if self.pid_fn else None
        if not ph:
            if verify:
                raise RuntimeError("Phosphor PID unknown: cannot focus it")
            return
        try:
            _run(["swaymsg", "[pid=%d] focus" % ph])
        except OSError:
            pass
        if not verify:
            return
        got = None
        for _ in range(30):
            got = self._focused()
            if got and got.get("pid") == ph:
                return
            time.sleep(0.1)
        raise RuntimeError("Phosphor does not have focus (focused: %s): refusing to type, keys would reach the wrong window" % (got,))

    def _focused(self):
        """the focused leaf of sway's tree: dict(pid, app_id, class, name) or None"""
        try:
            tree = json.loads(subprocess.run(["swaymsg", "-t", "get_tree"], capture_output=True, timeout=10).stdout)
        except Exception:
            return None
        found = []

        def walk(n):
            if n.get("focused") and not n.get("nodes"):
                found.append(dict(pid=n.get("pid"), app_id=n.get("app_id"), **{"class": (n.get("window_properties") or {}).get("class")}, name=n.get("name")))
            for c in n.get("nodes", []) + n.get("floating_nodes", []):
                walk(c)
        walk(tree)
        return found[0] if found else None

    def guard(self):
        ov = self.guard_override
        if not ov:
            return
        got = None
        for i in range(30):          # the window is mapped asynchronously after launch: wait for it to take focus, then refuse
            got = self._focused()
            if focus_matches(ov, got):
                return
            if i == 0 and ov.get("pids"):
                try:
                    _run(["swaymsg", "[pid=%d] focus" % min(ov["pids"])])
                except OSError:
                    pass
            time.sleep(0.1)
        raise RuntimeError("reference terminal does not have focus (focused: %s): refusing to type, keys would reach the wrong window" % (got,))

    def guard_ref(self):
        if self.guard_override:
            self.guard()

    def _wtype(self, args, prime=True):
        if prime and self.prime_ms > 0:
            args = ["-k", "Shift_L", "-s", str(self.prime_ms)] + args
        if prime:
            return self.run_recorded([["wtype"] + args])
        return _run(["wtype"] + args)     # noop(): the calibration probe is not a key

    def type_lines(self, lines, delay_ms=15, gap_ms=300):
        a = []
        for l in lines:
            if l.startswith("-"):
                raise ValueError("wtype text must not start with '-'")
            a += ["-d", str(int(delay_ms)), l, "-k", "Return", "-s", str(int(gap_ms))]
        self._wtype(a)

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
        self._wtype(["-M", "shift", "-m", "shift"], prime=False)

    # ---- injector health (r18d8): the persistent virtual keyboard holder (#1414) is checked before every case. A dead holder means every wtype
    # call is a keyboard-per-call again (first key lost) - that is the INJECTOR's fault and must never read as app key loss.
    health_events = None
    _holder = None
    _kbd_baseline = None

    def _keyboards(self):
        """number of keyboard inputs sway sees, or None when swaymsg cannot say"""
        try:
            r = subprocess.run(["swaymsg", "-t", "get_inputs"], capture_output=True, timeout=10)
            return sum(1 for i in json.loads(r.stdout) if i.get("type") == "keyboard")
        except Exception:
            return None

    def _run_dir(self):
        return os.path.dirname(self.inject_log) if self.inject_log else None

    def check_injector(self):
        """Called before each case. persistent mode only. Records 'injector died: <stderr tail>' in self.health_events (drained into the case detail by
        run.py) and restarts the holder."""
        if self.health_events is None:
            self.health_events = []
        if self.injector_mode != "persistent":
            return
        n = self._keyboards()
        if n is None:
            return
        if self._kbd_baseline is None:
            self._kbd_baseline = max(n, 1)
        if n >= self._kbd_baseline and (self._holder is None or self._holder.poll() is None):
            return
        rd = self._run_dir()
        tails = []
        for f in ((os.path.join(rd, "..", "kbd-holder.log") if rd else None), os.path.join(rd, "logs", "kbd-holder-restart.log") if rd else None, STDERR_LOG):
            try:
                if f:
                    tails.append("%s: %s" % (os.path.basename(f), _tail(open(f, errors="replace").read()) or "(empty)"))
            except OSError:
                pass
        ev = "injector died: sway sees %d keyboard input(s), expected >= %d; %s" % (n, self._kbd_baseline, "; ".join(tails) or "no stderr available")
        try:
            os.makedirs(os.path.join(rd, "logs"), exist_ok=True)
            self._holder = subprocess.Popen(["wtype"] + ["-s", "1000000"] * 40, stdout=subprocess.DEVNULL, start_new_session=True,
                                            stderr=open(os.path.join(rd, "logs", "kbd-holder-restart.log"), "a"))
            time.sleep(1.0)
            n2 = self._keyboards()
            ev += "; restarted holder pid %d (sway now sees %s keyboard(s), holder %s)" % (self._holder.pid, n2, "alive" if self._holder.poll() is None else "EXITED again")
            import atexit
            atexit.register(lambda h=self._holder: h.poll() is None and h.terminate())      # exact handle, not by name
        except Exception as e:
            ev += "; restart failed: %r" % e
        self.health_events.append(ev)
        print("[injector] " + ev, flush=True)

    def drain_health(self):
        ev, self.health_events = self.health_events or [], []
        return ev

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


# r18u28 (X11 release-candidate pass): the X11 injector, not the app, produced rollover/keydown_count/intl_wide_*/startup failures that no
# other backend showed. The pieces below are pure so the selftest can pin them without an X server.
X11_SHIFTED = set('~!@#$%^&*()_+{}|:"<>?ABCDEFGHIJKLMNOPQRSTUVWXYZ')   # US layout (Xvfb's default): typed with Shift held
X11_KEYNAMES = {"escape": "Escape", "esc": "Escape", "return": "Return", "enter": "Return", "tab": "Tab", "backspace": "BackSpace",
                "delete": "Delete", "del": "Delete", "left": "Left", "right": "Right", "up": "Up", "down": "Down", "home": "Home",
                "end": "End", "pageup": "Prior", "pagedown": "Next", "insert": "Insert"}


def x11_keyname(k):
    """an xdotool keysym name for a key written the way the cases write keys. X keysym names are case-sensitive: `xdotool key escape` prints
    "No such key name 'escape'. Ignoring it." and exits 0, so the key was silently never sent (injector-stderr.log, r18u28 X11)."""
    return X11_KEYNAMES.get(k.lower(), k) if len(k) > 1 else k


def x11_roll_argv(text, overlap_ms):
    """xdotool argv for rollover (the next key goes down before the previous one is released) that a physical US keyboard could produce.
    r18u28 (X11 rollover/keydown_count): the old chain held `_` (Shift+minus) down while `8` went down, so the app correctly received `*` (and `R`
    for `r`), and pressed `>` / `l` again while the same key was still down, which the X server does not deliver as a second press: typed
    `echo t73_8m81ereqlg >> r_roll.txt`, the app's own keydown log read `echo t73_*m81ereqlg > r_Rol.txt`. As in the Windows backend, a
    Shift-typed character, and a repeat of the key still held, is a plain tap after the held key is released; plain keys still overlap."""
    s = str(overlap_ms / 1000)
    cmd, held = ["xdotool"], None
    for ch in text:
        k = ENVNAME.get(ch, ch)
        if ch in X11_SHIFTED or k == held:
            if held:
                cmd += ["keyup", held]
                held = None
            cmd += ["keydown", k, "sleep", s, "keyup", k, "sleep", s]
            continue
        cmd += ["keydown", k, "sleep", s]
        if held:
            cmd += ["keyup", held]
        cmd += ["sleep", s]
        held = k
    if held:
        cmd += ["keyup", held]
    return cmd


def x11_spare_keycodes(pke, reserve=4):
    """keycodes with no keysym in `xmodmap -pke` output, highest first, minus the `reserve` lowest (left for xdotool's own scratch binding)"""
    free = []
    for line in pke.splitlines():
        m = re.match(r"\s*keycode\s+(\d+)\s*=\s*$", line)
        if m:
            free.append(int(m.group(1)))
    free.sort()
    return sorted(free[reserve:], reverse=True)


def x11_premap_plan(text, bound, spare):
    """Which non-ASCII characters of `text` need a keycode of their own before xdotool types them. -> (xmodmap -e expressions, new bound map,
    new spare list). `bound` {char: keycode} is what earlier calls bound (oldest first; a binding is reused, never rebound); when no spare
    keycode is left the oldest binding that `text` does not use is recycled. Characters that cannot get a keycode are left to xdotool."""
    bound, spare = dict(bound), list(spare)
    need = [c for c in dict.fromkeys(text) if ord(c) > 0x7e]
    exprs = []
    for c in need:
        if c in bound:
            bound[c] = bound.pop(c)              # most recently used last
            continue
        if not spare:
            victim = next((v for v in bound if v not in need), None)
            if victim is None:
                break
            spare.append(bound.pop(victim))
        kc = spare.pop(0)
        bound[c] = kc
        exprs += ["-e", "keycode %d = U%04X" % (kc, ord(c))]
    return exprs, bound, spare


class X11Backend(Backend):
    name = "x11"
    PREMAP_SETTLE_S = 0.4     # the app re-reads the keymap on MappingNotify; no key is sent until it has

    def __init__(self):
        self.calls = []       # injector accounting (#1509)
        self._focus_win = None
        self._bound = {}      # {char: keycode} bound by _premap for the whole run (the keymap is the X server's, it outlives app relaunches)
        self._spare = None
        self.premap = os.environ.get("APPCI_X11_PREMAP", "1") != "0"

    def preflight(self):
        p = [("missing tool: " + t) for t in ("xdotool", "xsel") if not shutil.which(t)]
        if not shutil.which("ffmpeg") and not shutil.which("import"):
            p.append("missing tool: ffmpeg or ImageMagick import (screenshots)")
        if not os.environ.get("DISPLAY"):
            p.append("env not set: DISPLAY")
        return p

    guard_override = None     # dict(pids=...) while a reference terminal is driven (#1500)

    def guard_ref(self):
        if self.guard_override:
            self.guard()

    def run_recorded(self, cmds):
        """every injection goes to a focused Phosphor window (see _ensure_focus), then is recorded as on every backend"""
        self._ensure_focus()
        Backend.run_recorded(self, cmds)

    def _focused(self):
        r = subprocess.run(["xdotool", "getwindowfocus"], capture_output=True, timeout=10)
        return r.stdout.decode().strip() if r.returncode == 0 else ""

    def _phosphor_windows(self):
        r = subprocess.run(["xdotool", "search", "--onlyvisible", "--class", "phosphor"], capture_output=True, timeout=10)
        return r.stdout.decode().split()

    def _ensure_focus(self):
        """r18u28 (X11 ui_startup_time, cb_settings_migration, remote_session_ext, editor_memory_7mb): Xvfb has no window manager, so a
        relaunched app's window never gets the keyboard focus by itself, and run.py focuses Phosphor only at a case's start. Every key typed
        after an in-case relaunch went nowhere (the app's log of that launch has no keydown at all; 'injector sent 15; app saw 0'), and the
        case read it as the burst not landing. Before each injection: one `getwindowfocus`; if that is not the window focused last time, the
        newest visible Phosphor window is focused (a focused Phosphor window is left alone) and verified, else the injection is refused."""
        if self.guard_override:
            self.guard()
            return
        have = self._focused()
        if have and have == self._focus_win:
            return
        ids = self._phosphor_windows()
        if not ids:
            raise RuntimeError("no visible Phosphor window on %s: refusing to type into nothing" % os.environ.get("DISPLAY", "?"))
        if have in ids:
            self._focus_win = have
            return
        for _ in range(20):
            _run(["xdotool", "windowfocus", ids[-1]])
            have = self._focused()
            if have in ids:
                self._focus_win = have
                return
            time.sleep(0.1)
        raise RuntimeError("Phosphor does not get the X input focus (focus %r, its windows %s): refusing to type" % (have, ids))

    def _premap(self, text):
        """r18u28 (X11 intl_wide_type/edit/program): `xdotool type` gives a character that is not in the keymap a scratch keycode, presses it
        and rebinds it for the next one. The app re-reads the keymap from the server when it handles the MappingNotify, i.e. after the
        NEXT rebinding: `日本語のテスト` reached the app's keydown log as `本本テト` (the same applies to any X11 client). Each such character
        gets a keycode of its own once per run (xmodmap), and nothing is typed until the app has settled on that keymap, so xdotool finds
        every character in a keymap that no longer changes while it types. APPCI_X11_PREMAP=0 restores the old behaviour."""
        if not self.premap or not any(ord(c) > 0x7e for c in text) or not shutil.which("xmodmap"):
            return
        if self._spare is None:
            r = subprocess.run(["xmodmap", "-pke"], capture_output=True, timeout=10)
            self._spare = x11_spare_keycodes(r.stdout.decode(errors="replace")) if r.returncode == 0 else []
        exprs, bound, spare = x11_premap_plan(text, self._bound, self._spare)
        if exprs:
            r = subprocess.run(["xmodmap"] + exprs, capture_output=True, timeout=10)
            if r.returncode != 0:
                _log_stderr("xmodmap", r.returncode, r.stderr)
                return
            time.sleep(self.PREMAP_SETTLE_S)
        self._bound, self._spare = bound, spare

    def type(self, text, delay_ms=15, enter=False):
        self._premap(text)
        cmds = [["xdotool", "type", "--delay", str(int(delay_ms)), "--", text]]
        if enter:
            cmds.append(["xdotool", "key", "--delay", "0", "Return"])
        self.run_recorded(cmds)

    def key(self, spec, repeat=1, delay_ms=15):
        mods, k = split_key(spec)
        x = "+".join(mods + [x11_keyname(k)])
        self.run_recorded([["xdotool", "key", "--delay", str(int(delay_ms))] + [x] * repeat])

    def keys(self, specs, delay_ms=15):
        self.run_recorded([["xdotool", "key", "--delay", str(int(delay_ms))] + ["+".join(split_key(s)[0] + [x11_keyname(split_key(s)[1])]) for s in specs]])

    def roll(self, text, overlap_ms=10, enter=True):
        if any(ord(c) > 0x7e for c in text):          # keydown takes keysym names, not characters: such text is typed (as the Mac backend does for ':')
            return self.type(text, delay_ms=max(5, overlap_ms), enter=enter)
        cmd = x11_roll_argv(text, overlap_ms)
        if enter:
            cmd += ["key", "Return"]
        self.run_recorded([cmd])

    def noop(self):
        _run(["xdotool", "keyup", "shift"])

    def guard(self):
        """no window manager in Xvfb: focus (and point at) the newest visible Phosphor window before keys go out. While a reference terminal is
        the target (guard_override), focus ITS window and verify the X input focus really is one of that PID's windows, else refuse (#1500)."""
        ov = self.guard_override
        if ov:
            ids = []
            for pid in sorted(ov.get("pids", ())):
                ids += subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(pid)], capture_output=True).stdout.decode().split()
            if not ids:
                raise RuntimeError("no visible window of the reference terminal on " + os.environ.get("DISPLAY", "?"))
            _run(["xdotool", "windowfocus", ids[-1]])
            have = subprocess.run(["xdotool", "getwindowfocus"], capture_output=True).stdout.decode().strip()
            if have not in ids:
                raise RuntimeError("reference terminal does not have the X input focus (focus %r, its windows %s): refusing to type" % (have, ids))
            return
        r = subprocess.run(["xdotool", "search", "--onlyvisible", "--class", "phosphor"], capture_output=True)
        ids = r.stdout.decode().split()
        if not ids:
            raise RuntimeError("no visible Phosphor window on " + os.environ.get("DISPLAY", "?"))
        _run(["xdotool", "windowfocus", ids[-1]])
        self._focus_win = None            # the next injection re-reads the focus (_ensure_focus)

    def refocus(self, verify=False):
        """focus Phosphor (no keys sent), as every injection does"""
        self._ensure_focus()

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


def mac_windows_of(listing, pid):
    """pure: the rows of `winrect list` (pid<TAB>owner<TAB>layer<TAB>x,y,w,h<TAB>name) that are normal windows of `pid` -> [{'id', 'name', 'rect'}]"""
    out = []
    for i, l in enumerate(listing.splitlines()):
        f = l.split("\t")
        if len(f) < 4 or f[0] != str(pid) or f[2] != "0":
            continue
        try:
            rect = tuple(int(v) for v in f[3].split(","))
        except ValueError:
            continue
        if len(rect) == 4 and rect[2] > 100 and rect[3] > 100:
            out.append({"id": i, "name": f[4] if len(f) > 4 else "", "rect": rect})
    return out


class NeedsOwner(RuntimeError):
    """a macOS system dialog (Local Network / TCC / authorization) is in front: keys would go into it. run.py stops the run
    with exit 3 and NEEDS_OWNER.txt. We NEVER click such a dialog ourselves."""


class MacBackend(Backend):
    """Keys are injected by htype2 (CGEvent posting with real key codes at an exact cadence; source in
    script/realapp/mac/tools, compiled by the job), key->paint latency by kl3 (ScreenCaptureKit). cliclick is no longer
    used for typing (a process spawn per call: 62 ms median gap for a requested 15 ms in the first CI run).
    Safety (docs/testing.md 3.3): guard() runs before EVERY injection and checks the frontmost app name AND that its PID is
    the Phosphor PID the harness started; on a mismatch it tries once to focus that exact PID (no keys) and otherwise raises.
    Cmd-Q/W/Tab/Space/H/M, any Ctrl+Cmd chord, Globe/Fn are refused structurally. Ctrl-D is only ever sent through key(), i.e.
    behind the guard, so it reaches Phosphor's window only.
    #2061 (r14-test-gaps T32), the one exception to that list: Cmd-W, Cmd-Shift-W and Cmd-Q (APP_CHORDS: close tab, close window, quit) are sent by
    app_chord() and by nothing else: never through htype2, never while a reference terminal is the target (guard_override), only to the PID the harness
    started, whose executable must be inside the job's own bundle, through mac/tools/appkey (which checks all of that again in its own process and posts
    the event to that one PID). Every other Cmd chord on NEVER_CMD keys (Cmd-Tab, Cmd-Space, Cmd-H, Cmd-M, Cmd-Shift-Q = log out, Cmd-Alt-W, ...) stays refused.
    The pointer is mac/tools/mptr (ptr()): guarded the same way, and the tool refuses a press where the topmost window is not the app's."""
    name = "mac"
    APP = os.environ.get("APPCI_MAC_PROCESS", "phosphor-oss")
    TOOLS = os.environ.get("APPCI_MAC_TOOLS", "")
    SPECIAL = {"return": 36, "enter": 36, "tab": 48, "escape": 53, "backspace": 51, "delete": 51, "left": 123, "right": 124,
               "down": 125, "up": 126, "home": 115, "end": 119, "space": 49, "pageup": 116, "pagedown": 121}
    LETTERS = {"a": 0, "s": 1, "d": 2, "f": 3, "h": 4, "g": 5, "z": 6, "x": 7, "c": 8, "v": 9, "b": 11, "q": 12, "w": 13, "e": 14,
               "r": 15, "y": 16, "t": 17, "1": 18, "2": 19, "3": 20, "4": 21, "6": 22, "5": 23, "=": 24, "9": 25, "7": 26, "-": 27,
               "8": 28, "0": 29, "]": 30, "o": 31, "u": 32, "[": 33, "i": 34, "p": 35, "l": 37, "j": 38, "'": 39, "k": 40,
               ";": 41, "\\": 42, ",": 43, "/": 44, "n": 45, "m": 46, ".": 47}
    MODBIT = {"shift": 1, "ctrl": 2, "alt": 4, "logo": 8}
    NEVER_CMD = ("q", "w", "tab", "space", "h", "m")
    APP_CHORDS = {(8, "q"): 12, (8, "w"): 13, (9, "w"): 13}       # #2061: (modifier bits, key) -> key code; sent by app_chord() only (appkey)
    MODALIAS = {"control": "ctrl", "super": "logo", "cmd": "logo", "meta": "logo", "option": "alt"}
    BUNDLE = os.environ.get("APPCI_MAC_BUNDLE", "")              # the job's private copy of the app (mac/ci/app_test.sh)
    CHORD_VIA = os.environ.get("APPCI_MAC_CHORD_VIA", "pid")     # appkey delivery: "pid" (CGEvent.postToPid) or "hid" (the HID tap, as htype2)
    pid_fn = None            # set by run.py: returns the PID of the Phosphor instance under test

    def __init__(self):
        self.guard_override = None
        self.calls = []
        self._kl = None
        self.outf = os.path.join(os.environ.get("TMPDIR", "/tmp"), "appci_ht_%d.txt" % os.getpid())

    def _tool(self, n):
        return os.path.join(self.TOOLS, n)

    def preflight(self):
        p = []
        for t in ("htype2", "kl3", "winrect"):
            if not os.access(self._tool(t), os.X_OK):
                p.append("missing compiled tool %s (APPCI_MAC_TOOLS=%r; app_test.sh compiles script/realapp/mac/tools/*.swift)" % (t, self.TOOLS))
        t = "/tmp/appci_pf_%d.png" % os.getpid()
        _run(["screencapture", "-x", t])
        if not os.path.exists(t) or os.path.getsize(t) < 5000:
            p.append("screencapture produced no usable image: runner is not in a GUI session or lacks Screen Recording")
        else:
            os.remove(t)
        r = subprocess.run(["osascript", "-e", 'tell application "System Events" to get name of first process whose frontmost is true'], capture_output=True)
        if r.returncode != 0:
            err = r.stderr.decode().strip()
            # Only a TCC refusal is a permission problem: -1743 (not authorized to send Apple events),
            # -25211 / "assistive access" (Accessibility off). -1719 "Invalid index" means System Events
            # found NO frontmost process: a system dialog or the lock screen is in front, the permission
            # is fine (r14a lab-macos 06:05Z: a UserNotificationCenter dialog was up; screenshots worked).
            if "(-1719)" in err:
                front = subprocess.run(["lsappinfo", "front"], capture_output=True, timeout=10).stdout.decode().strip()
                p.append("no frontmost app (System Events -1719; lsappinfo front=%r): a system dialog or the "
                         "lock screen is in front of the gitrunner session; dismiss it or unlock, Accessibility is "
                         "not the cause" % front[:60])
            elif "-1743" in err or "-25211" in err or "assistive access" in err.lower():
                p.append("osascript/System Events denied by macOS privacy (TCC): grant Accessibility to the "
                         "runner's process chain (%s)" % err[:120])
            else:
                p.append("osascript/System Events failed, cause unknown (not judged a permission problem): %s" % err[:120])
        return p

    def _front(self):
        """(name, pid) of the frontmost process. lsappinfo asks LaunchServices directly: no Apple Events, no TCC
        prompt. (The previous AppleScript put `unix id of p` outside the System Events `tell`, a syntax error that
        returned nothing, so the guard refused every key: first CI run, job 17650494835.)"""
        r = subprocess.run(["lsappinfo", "front"], capture_output=True, timeout=10)
        asn = r.stdout.decode().strip()
        if r.returncode != 0 or not asn:
            return "", None
        r = subprocess.run(["lsappinfo", "info", "-only", "pid", "-only", "name", asn], capture_output=True, timeout=10)
        o = r.stdout.decode()
        m_pid = re.search(r'"pid"\s*=\s*(\d+)', o)
        m_name = re.search(r'"(?:LSDisplayName|name)"\s*=\s*"([^"]*)"', o)
        return (m_name.group(1) if m_name else ""), (int(m_pid.group(1)) if m_pid else None)

    def _focus_pid(self, pid):
        subprocess.run(["osascript", "-e", 'tell application "System Events" to set frontmost of (first process whose unix id is %d) to true' % pid],
                       capture_output=True, timeout=30)
        time.sleep(0.6)

    # processes that own system prompts (Local Network, TCC, authorization). Any on-screen window of one of these means keys could be eaten.
    DIALOG_OWNERS = ("usernotificationcenter", "coreservicesuiagent", "universalaccessauthwarn", "securityagent", "tccd", "localnetworkprompt")

    # Owner decision 2026-11-11: every ad-hoc-signed CI build re-asks macOS's Local Network question, and the owner cannot click it
    # remotely. With APPCI_MAC_AUTO_ALLOW_LOCAL_NETWORK=1 (machine-local ci.env of the Mac runner ONLY, never the repo) the harness
    # answers exactly that question, for Phosphor, with Allow. Any other system dialog still stops the pass as NEEDS OWNER.
    AUTO_ALLOW_ENV = "APPCI_MAC_AUTO_ALLOW_LOCAL_NETWORK"

    @staticmethod
    def is_phosphor_local_network_prompt(texts, buttons):
        """the dialog's static texts and button names are exactly the Local Network question about Phosphor"""
        t = " ".join(texts).lower().replace("“", '"').replace("”", '"')
        b = {x.replace("’", "'").strip().lower() for x in buttons}
        return ('"phosphor"' in t and "local network" in t and "find devices" in t
                and b == {"allow", "don't allow"})

    def auto_allow_local_network(self, owner):
        """True when the Local Network question for Phosphor was on screen and was answered Allow (and is gone); False otherwise,
        and the caller reports the dialog as before. Only with AUTO_ALLOW_ENV=1 and only for UserNotificationCenter."""
        import ci_env  # the machine-local ci.env is read here, not exported into the job's environment (environment wins)
        if ci_env.get(self.AUTO_ALLOW_ENV) != "1" or owner.lower().replace(" ", "") != "usernotificationcenter":
            return False
        se = 'tell application "System Events" to tell process "UserNotificationCenter" to '
        try:
            texts = subprocess.run(["osascript", "-e", se + "get value of every static text of window 1"],
                                   capture_output=True, timeout=20).stdout.decode(errors="replace").strip().split(", ")
            buttons = subprocess.run(["osascript", "-e", se + "get name of every button of window 1"],
                                     capture_output=True, timeout=20).stdout.decode(errors="replace").strip().split(", ")
            if not self.is_phosphor_local_network_prompt(texts, buttons):
                return False
            subprocess.run(["osascript", "-e", se + 'click button "Allow" of window 1'], capture_output=True, timeout=20)
            time.sleep(1.5)
            left = subprocess.run(["osascript", "-e", se + "count windows"], capture_output=True, timeout=20).stdout.decode().strip()
            ok = left == "0"
            print("appci: macOS Local Network question for Phosphor answered Allow by the harness (%s=1): %s"
                  % (self.AUTO_ALLOW_ENV, "dialog gone" if ok else "dialog STILL on screen"), flush=True)
            return ok
        except Exception as e:
            print("appci: auto-allow of the Local Network question failed: %r" % (e,), flush=True)
            return False

    def system_dialog(self, ocr=False):
        """description of a system dialog that is on screen, else None. Cheap path: WindowServer window list (the Local Network
        prompt is NOT frontmost per lsappinfo: the menu bar still says Phosphor, so the front-app check alone cannot see it).
        ocr=True adds a Vision pass over a screenshot for 'Allow' + 'local network' (used once before the first key).
        The Local Network question about Phosphor is answered by the harness when AUTO_ALLOW_ENV=1 (owner decision 2026-11-11)."""
        try:
            r = subprocess.run([self._tool("winrect"), "list"], capture_output=True, timeout=15)
            for l in r.stdout.decode(errors="replace").splitlines():
                f = l.split("\t")
                if len(f) >= 2 and f[1].lower().replace(" ", "") in self.DIALOG_OWNERS:
                    if self.auto_allow_local_network(f[1]):
                        continue
                    return "window of %s (pid %s) on screen: %s" % (f[1], f[0], l[:160])
        except Exception:
            pass
        name, _ = self._front()
        if name.lower().replace(" ", "") in self.DIALOG_OWNERS:
            return "frontmost process is %s" % name
        if ocr and os.access(self._tool("ocr"), os.X_OK):
            t = "/tmp/appci_dlg_%d.png" % os.getpid()
            _run(["screencapture", "-x", t])
            try:
                txt = " ".join(subprocess.run([self._tool("ocr"), t], capture_output=True, timeout=60).stdout.decode(errors="replace").lower().split())
            except Exception:
                txt = ""
            if os.path.exists(t):
                os.remove(t)
            if ("don't allow" in txt or "dont allow" in txt) and ("local network" in txt or "devices on" in txt):
                return "OCR sees an 'Allow / Don't Allow' prompt about local networks"
        return None

    def needs_owner(self, what):
        return NeedsOwner(
            "a macOS system dialog is in front and would take our keys: %s. The owner must click ALLOW once, in the gitrunner GUI session "
            "(Screen Sharing / the console as user gitrunner): the dialog reads 'Allow \"Phosphor\" to find devices on local networks?'. "
            "A new ad-hoc-signed build can re-trigger it (docs/testing.md 3.3). The harness answers ONLY that question, and only with "
            "APPCI_MAC_AUTO_ALLOW_LOCAL_NETWORK=1 in the Mac runner's machine-local ci.env (owner decision 2026-11-11); this dialog was "
            "not that question, or the switch is off. Then re-run the job." % what)

    def guard(self):
        d = self.system_dialog()
        if d:
            raise self.needs_owner(d)
        name, pid = self._front()
        if name.lower() == "loginwindow":
            raise RuntimeError("loginwindow is frontmost (locked screen?): refusing to send keys")
        want = self.guard_override
        if want:
            if want.lower() not in name.lower():
                raise RuntimeError("frontmost app is %r, not %s: refusing to send keys" % (name, want))
            return
        ph = self.pid_fn() if self.pid_fn else None
        if ph is None:
            raise RuntimeError("Phosphor PID unknown (app not running?): refusing to send keys")
        if pid == ph:
            return
        self._focus_pid(ph)
        name2, pid2 = self._front()
        if pid2 != ph:
            raise RuntimeError("frontmost is %r (pid %s), not Phosphor (pid %s): refusing to send keys" % (name2, pid2, ph))

    def refocus(self, verify=False):
        """bring Phosphor back to the front after the reference window closed (System Events frontmost; no keys sent)"""
        ph = self.pid_fn() if self.pid_fn else None
        if ph:
            self._focus_pid(ph)

    def _ht(self, ops, guard=True):
        if guard:
            self.guard()
        try:
            os.remove(self.outf)                 # a stale file from the previous call must never be read as this call's record
        except OSError:
            pass
        t0 = time.time()
        err = None
        try:
            _run([self._tool("htype2"), "0", "0", "0", "0", self.outf] + ops, timeout=120)
        except Exception as e:
            err = "%s: %s" % (type(e).__name__, str(e)[:120])
            raise
        finally:
            self._record_call(ops, t0, time.time(), err)

    def _record_call(self, ops, t0, t1, err):
        """Keep htype2's K-lines (it overwrites its outfile every call) in self.calls and <run>/inject.log. Typed characters are logged as a COUNT
        and first/last time only (never their code points: typed text can be a password); ENTER/KEY/BS/TAB/RIGHT lines are logged in full."""
        rec = dict(n=len(self.calls), wall0=t0, wall1=t1, enters_want=sum(1 for o in ops if o.split(":")[0] == "e"),
                   enter_wall=[], post_wall=[], post_mach_ms=[], clock=None, chars=0, special=[], err=err, out=False,
                   ref=bool(self.guard_override))      # r18u8: keys aimed at the reference terminal (interleaved latency) are not Phosphor's; injacct.join skips ref calls
        rec["cmd_chords"] = self.cmd_chords(ops)
        try:
            lines = open(self.outf, errors="replace").read().splitlines()
        except OSError:
            lines = []
        epoch = mach = None
        first = last = None
        for l in lines:
            w = l.split()
            if w[:1] == ["T0"] and len(w) >= 3:
                try:
                    mach = float(w[1].split("=")[1]); epoch = float(w[2].split("=")[1])
                    rec["out"] = True
                    rec["clock"] = (mach, epoch)      # #1606 rediag: one (uptime ms, epoch s) pair, to put the OS's event timestamps on the wall clock
                except (ValueError, IndexError):
                    pass
            elif w[:1] == ["K"] and len(w) >= 4:
                try:
                    ms = float(w[3])
                except ValueError:
                    continue
                wall = epoch + (ms - mach) / 1000 if epoch is not None and mach is not None else None
                if w[1].isdigit():
                    rec["chars"] += 1
                    first = ms if first is None else first
                    last = ms
                else:
                    rec["special"].append("%s@%s" % (w[1], ("%.3f" % wall) if wall else "?"))
                    if wall is not None:
                        rec["post_wall"].append(wall)      # #1606: the injector's own post time of each named key (latency split)
                    rec["post_mach_ms"].append(ms)         # #1606 rediag: the same post on the OS uptime clock (the CGEvent / NSEvent timestamp clock)
                    if w[1] == "ENTER":
                        rec["enter_wall"].append(wall)
        rec["enters_posted"] = len(rec["enter_wall"])
        rec["span_ms"] = (last - first) if first is not None else 0
        self.calls.append(rec)
        if self.inject_log:
            try:
                with open(self.inject_log, "a") as f:
                    f.write("call=%d wall=%.3f..%.3f chars=%d span_ms=%.0f enters_want=%d enters_posted=%d out=%s%s %s\n" % (
                        rec["n"], t0, t1, rec["chars"], rec["span_ms"], rec["enters_want"], rec["enters_posted"], rec["out"],
                        (" ERR=" + err) if err else "", " ".join(rec["special"])))
            except OSError:
                pass

    @staticmethod
    def cmd_chords(ops):
        """How many of htype2's k:CODE:MODS:GAP ops carry Command (MODS bit 8). Round 10 (injacct): a Cmd chord can be a main-menu key equivalent
        (Cmd-V), which AppKit takes after -[NSApplication sendEvent:] and before the view, so the app logs its os_key_event and no key dispatch
        line. The accounting allows at most this many such key-downs per case; every other key-down must be dispatched."""
        n = 0
        for o in ops:
            p = o.split(":")
            if p[0] == "k" and len(p) >= 3 and p[2].isdigit() and int(p[2]) & 8:
                n += 1
        return n

    def inject_enter(self, idx):
        if idx is None or idx < 0 or idx >= len(self.calls):
            return None
        c = self.calls[idx]
        if c["enters_want"] and not c["enters_posted"]:
            return False, "injector call #%d did NOT post its ENTER (%s)" % (c["n"], c["err"] or ("no htype2 output" if not c["out"] else "no ENTER line in its log"))
        w = c["enter_wall"][0] if c["enter_wall"] else None
        return True, "injector call #%d posted its ENTER at %s" % (c["n"], time.strftime("%H:%M:%S", time.localtime(w)) + (".%03d" % int((w % 1) * 1000)) if w else "?")

    def type(self, text, delay_ms=15, enter=False):
        d = int(delay_ms)
        self._ht(["t:%s:%d:%d" % (text, d, d)] + (["e:0"] if enter else []))

    @classmethod
    def chord_parts(cls, spec):
        """'ctrl+u' / 'Return' / 'cmd+shift+w' -> (modifier bits, lower-case key name); ValueError on an unknown modifier"""
        low = spec.lower()
        parts = low.split("+") if low != "+" else ["+"]
        bits = 0
        for m in parts[:-1]:
            m = cls.MODALIAS.get(m, m)
            if m not in cls.MODBIT:
                raise ValueError("unknown modifier in " + spec)
            bits |= cls.MODBIT[m]
        return bits, parts[-1]

    @classmethod
    def chord_kind(cls, spec):
        """pure: 'app' (Cmd-W, Cmd-Shift-W, Cmd-Q: app_chord() only), 'refused' (never sent by any path), or 'plain' (htype2 behind the guard)"""
        bits, k = cls.chord_parts(spec)
        if (bits, k) in cls.APP_CHORDS:
            return "app"
        if k in ("globe", "fn") or (bits & 2 and bits & 8) or (bits & 8 and k in cls.NEVER_CMD):
            return "refused"
        return "plain"

    def _kspec(self, spec):
        """'ctrl+u' / 'Return' / 'a' -> htype2 op format. Refuses the forbidden chords AND the app chords: htype2 never carries either."""
        bits, k = self.chord_parts(spec)
        kind = self.chord_kind(spec)
        if kind == "refused":
            raise ValueError("refusing forbidden chord " + spec)
        if kind == "app":
            raise ValueError("refusing forbidden chord %s for htype2: it is sent by app_chord() to the app under test only" % spec)
        code = self.SPECIAL.get(k, self.LETTERS.get(k))
        if code is None:
            raise ValueError("no key code for " + spec)
        return "k:%d:%d:%d"

    def key(self, spec, repeat=1, delay_ms=15):
        self.keys([spec] * repeat, delay_ms=delay_ms)

    def keys(self, specs, delay_ms=15):
        """named keys in order. An app chord (#2061) goes out through app_chord(), between the htype2 runs before and after it, so the order holds."""
        run = []
        for s in specs:
            if self.chord_kind(s) == "app":
                if run:
                    self._ht(run)
                    run = []
                if int(delay_ms) > 0:
                    time.sleep(int(delay_ms) / 1000.0)
                self.app_chord(s)
                continue
            fmt = self._kspec(s)
            bits, k = self.chord_parts(s)
            run.append(fmt % (self.SPECIAL.get(k, self.LETTERS.get(k)), bits, int(delay_ms)))
        if run:
            self._ht(run)

    def app_chord_refusal(self, spec, pid):
        """pure: why app_chord() must not send `spec` now, or None. Checked before the frontmost-app guard and before the tool runs."""
        if self.chord_kind(spec) != "app":
            return "%s is not one of the app chords (Cmd-W, Cmd-Shift-W, Cmd-Q)" % spec
        if self.guard_override:
            return "the injection target is the reference terminal (%s): %s is only ever sent to the app under test" % (self.guard_override, spec)
        if not pid:
            return "Phosphor PID unknown (app not running?)"
        if not (self.BUNDLE.startswith("/") and self.BUNDLE.rstrip("/").endswith(".app")):
            return "APPCI_MAC_BUNDLE (%r) is not the job's .app: the app under test cannot be told from an installed one" % self.BUNDLE
        if self.CHORD_VIA not in ("pid", "hid"):
            return "APPCI_MAC_CHORD_VIA is %r, expected pid or hid" % self.CHORD_VIA
        return None

    def app_chord(self, spec):
        """#2061: Cmd-W / Cmd-Shift-W / Cmd-Q to the app under test and to nothing else (see the class docstring). Raises RuntimeError on any doubt."""
        pid = self.pid_fn() if self.pid_fn else None
        why = self.app_chord_refusal(spec, pid)
        if why:
            raise RuntimeError("refusing %s: %s" % (spec, why))
        if not os.access(self._tool("appkey"), os.X_OK):
            raise RuntimeError("refusing %s: the appkey tool is missing (%s; app_test.sh compiles mac/tools/appkey.swift, see swiftc-appkey.log)" % (spec, self._tool("appkey")))
        self.guard()
        bits, k = self.chord_parts(spec)
        code = self.APP_CHORDS[(bits, k)]
        try:
            os.remove(self.outf)
        except OSError:
            pass
        t0, err = time.time(), None
        try:
            r = subprocess.run([self._tool("appkey"), str(pid), self.BUNDLE, str(code), str(bits), self.CHORD_VIA, self.outf], capture_output=True, timeout=30)
            if r.returncode != 0:          # exit 4 = the tool's own check refused (not frontmost, not the job's bundle): nothing was posted
                _log_stderr("appkey", r.returncode, r.stderr)
                raise RuntimeError("appkey exit %d for %s (%s)" % (r.returncode, spec, _tail(r.stderr.decode("utf-8", "replace")) or "no stderr"))
        except Exception as e:
            err = "%s: %s" % (type(e).__name__, str(e)[:200])
            raise
        finally:
            self._record_call(["k:%d:%d:0" % (code, bits)], t0, time.time(), err)

    def ptr(self, ops):
        """#2061: pointer ops (mac/tools/mptr.swift; coordinates in POINTS) at the app under test. The frontmost-app guard runs first; the tool refuses a
        press, a drag or a wheel step where the topmost window is not owned by the app's PID. -> (x, y) of the cursor read back after the last op."""
        if self.guard_override:
            raise RuntimeError("refusing pointer input: the injection target is the reference terminal (%s)" % self.guard_override)
        pid = self.pid_fn() if self.pid_fn else None
        if not pid:
            raise RuntimeError("Phosphor PID unknown (app not running?): refusing pointer input")
        if not os.access(self._tool("mptr"), os.X_OK):
            raise RuntimeError("the mptr tool is missing (%s; app_test.sh compiles mac/tools/mptr.swift, see swiftc-mptr.log)" % self._tool("mptr"))
        self.guard()
        out = self.outf + ".ptr"
        try:
            os.remove(out)
        except OSError:
            pass
        t0 = time.time()
        r = subprocess.run([self._tool("mptr"), str(pid), out] + list(ops), capture_output=True, timeout=90)
        try:
            lines = open(out, errors="replace").read().splitlines()
        except OSError:
            lines = []
        loc = None
        for l in lines:
            w = l.split()
            if w[:1] == ["LOC"] and len(w) == 3:
                try:
                    loc = (float(w[1]), float(w[2]))
                except ValueError:
                    pass
        if self.inject_log:
            try:
                with open(self.inject_log, "a") as f:
                    f.write("ptr wall=%.3f..%.3f rc=%d ops=%s loc=%s\n" % (t0, time.time(), r.returncode, " ".join(ops), loc))
            except OSError:
                pass
        if r.returncode != 0:
            _log_stderr("mptr", r.returncode, r.stderr)
            raise RuntimeError("pointer injector failed: mptr exit %d (%s)" % (r.returncode, _tail(r.stderr.decode("utf-8", "replace")) or "no stderr"))
        return loc

    def ptr_info(self):
        """{'display': (x, y, w, h, pixel_w, pixel_h), 'cursor': (x, y)} from `mptr info` (points; pixels of the display mode), or {} when the tool fails"""
        try:
            r = subprocess.run([self._tool("mptr"), "info"], capture_output=True, timeout=15)
        except Exception:
            return {}
        out = {}
        for l in r.stdout.decode(errors="replace").splitlines():
            w = l.split()
            try:
                if w[:1] == ["display"] and len(w) == 7:
                    out["display"] = tuple(int(v) for v in w[1:])
                elif w[:1] == ["cursor"] and len(w) == 3:
                    out["cursor"] = (float(w[1]), float(w[2]))
            except ValueError:
                pass
        return out

    def windows(self):
        """the app's on-screen normal windows (winrect list, layer 0, wider and taller than 100 points), front to back; [] when unknown"""
        ph = self.pid_fn() if self.pid_fn else None
        if not ph:
            return []
        try:
            r = subprocess.run([self._tool("winrect"), "list"], capture_output=True, timeout=15)
        except Exception:
            return []
        return mac_windows_of(r.stdout.decode(errors="replace"), ph)

    def roll(self, text, overlap_ms=10, enter=True):
        if ":" in text:                                   # the r: op cannot carry ':'
            return self.type(text, delay_ms=max(5, overlap_ms), enter=enter)
        self._ht(["r:%s:%d" % (text, int(overlap_ms))] + (["e:%d" % int(overlap_ms)] if enter else []))

    def paste(self, enter=False):
        self.keys(["cmd+v"] + (["Return"] if enter else []), delay_ms=0)

    def noop(self):
        """one input-tool invocation with nothing to send: the harness floor (process start only)"""
        self._ht([], guard=False)

    def clip_set(self, data):
        if isinstance(data, str):
            data = data.encode()
        subprocess.run(["pbcopy"], input=data, timeout=30)

    def clip_get(self):
        return subprocess.run(["pbpaste"], capture_output=True).stdout

    def shot(self, path):
        _run(["screencapture", "-x", path], timeout=30)
        return os.path.exists(path) and os.path.getsize(path) > 5000

    def window_rect(self, pid=None):
        """(x, y, w, h) of Phosphor's main window (or of process `pid`, e.g. the reference Terminal.app) in points, or None.
        CGWindowList via the winrect helper: no System Events / Apple Events."""
        ph = pid or (self.pid_fn() if self.pid_fn else None)
        if not ph:
            return None
        r = subprocess.run([self._tool("winrect"), "rect", str(ph)], capture_output=True, timeout=15)
        try:
            return tuple(float(x) for x in r.stdout.decode().strip().split(","))
        except ValueError:
            return None

    def kl_start(self, rect):
        self.kl_stop()
        self._kl = subprocess.Popen([self._tool("kl3")] + [str(int(v)) for v in rect], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                    stderr=subprocess.DEVNULL, text=True, bufsize=1, env=dict(os.environ, KSETTLE="350"))
        return self._kl

    def kl(self, line):
        """one kl3 command (guarded); returns its output line. kl3 posts its keys itself (CGEvent), so they are recorded here like an htype2 call
        (#2005: unrecorded, they were the "app logged 39 more than were injected" of every latency_paint / latency_paint_ab / latency_key_views_ab
        row on macOS in rounds 11 and 12)."""
        self.guard()
        t0 = time.time()
        self._kl.stdin.write(line + "\n")
        self._kl.stdin.flush()
        out = self._kl.stdout.readline().strip()
        self._record_kl(line, out, t0, time.time())
        return out

    def _record_kl(self, line, out, t0, t1):
        chars, special, chords = kl_keys(line)
        if not chars and not special:
            return
        posted = kl_posted(out)
        m = re.search(r"post0=([\d.]+)", out)
        wall = float(m.group(1)) if m else None
        named = ["%s@%s" % (k, ("%.3f" % wall) if wall else "?") for k in special] if posted else []
        rec = dict(n=len(self.calls), wall0=t0, wall1=t1, enters_want=0, enters_posted=0, enter_wall=[], post_wall=[wall] * len(named) if wall else [],
                   post_mach_ms=[], clock=None, chars=chars if posted else 0, special=named, err=None if posted else "kl3: %s" % (out[:60] or "no answer"),
                   out=posted, ref=bool(self.guard_override), cmd_chords=chords if posted else 0, span_ms=0, tool="kl3")
        self.calls.append(rec)
        if self.inject_log:
            try:
                with open(self.inject_log, "a") as f:
                    f.write("call=%d tool=kl3 wall=%.3f..%.3f chars=%d span_ms=0 enters_want=0 enters_posted=0 out=%s%s%s %s\n" % (
                        rec["n"], t0, t1, rec["chars"], posted, (" ERR=" + rec["err"]) if rec["err"] else "", " REF" if rec["ref"] else "", " ".join(named)))
            except OSError:
                pass

    def kl_stop(self):
        p, self._kl = self._kl, None
        if p and p.poll() is None:
            try:
                p.stdin.write("Q\n"); p.stdin.flush(); p.wait(3)
            except Exception:
                p.kill()


# Round 15 (r14-test-gaps T45, keybinding_rebind): the function keys, as a separate statement. Every letter chord is one of the app's defaults, so a
# rebind test needs a key nothing is bound to. macOS virtual key codes (kVK_F1 .. kVK_F12); a posted F-key is the key itself, not the media key.
MacBackend.SPECIAL.update({"f1": 122, "f2": 120, "f3": 99, "f4": 118, "f5": 96, "f6": 97, "f7": 98, "f8": 100, "f9": 101, "f10": 109, "f11": 103, "f12": 111})


class WindowsBackend(Backend):
    """Windows 10/11, interactive desktop (the GitLab runner runs on the console). Everything is ctypes, no helper binary:
      keys        SendInput with real virtual-key codes AND scancodes (MapVirtualKey) for printable ASCII on a US layout (a static table, so the VM's
                  layout cannot change what is typed); anything else (CJK, accents, emoji: UTF-16 units) as KEYEVENTF_UNICODE packets.
                  Ctrl-C, Enter, Tab, arrows ... are always real VK events. key_method 'unicode' sends printable characters as packets too
                  (run.py's delivery proof falls back to it when VK events never arrive).
      guard       before EVERY burst (and before every key of a long one): GetForegroundWindow + GetWindowThreadProcessId must give the PID of the
                  Phosphor instance this run started (pid_fn); on a mismatch focus that exact window once (no key is sent to do it), else raise.
                  guard_override = dict(pids=..., hwnds=...) is set while the reference console is driven: only THAT window, never the job's own console.
      screenshots PIL.ImageGrab (all monitors).   clipboard: user32 CF_UNICODETEXT.
      injector    every type()/key()/keys()/roll() call is one record in self.calls (and a line in <run>/inject.log): events asked, events SendInput
                  accepted, wall time of each Return key-down it posted: the Enter control (#1482) reads it like the Mac htype2 record.
    time.time() is replaced by GetSystemTimePreciseAsFileTime so the latency cases compare sub-millisecond timestamps."""
    name = "windows"
    pid_fn = None
    guard_override = None
    UNSUPPORTED_CHORDS = ("alt+f4", "alt+tab", "alt+escape", "ctrl+escape", "ctrl+shift+escape", "ctrl+alt+delete", "ctrl+alt+del")
    NAMED = {"return": 0x0D, "enter": 0x0D, "tab": 0x09, "escape": 0x1B, "esc": 0x1B, "backspace": 0x08, "delete": 0x2E, "del": 0x2E,
             "left": 0x25, "up": 0x26, "right": 0x27, "down": 0x28, "home": 0x24, "end": 0x23, "pageup": 0x21, "pagedown": 0x22,
             "insert": 0x2D, "space": 0x20, "pause": 0x13, "break": 0x03, "cancel": 0x03,     # ctrl+break = VK_CANCEL (#1888)
             "scrollbreak": 0x03,
             "prior": 0x21, "next": 0x22}     # the X keysym names of PageUp / PageDown, as the shared cases spell the tab keys (`ctrl+Next`, cases_wrap_vtabs.KX)
    # #1906: the two keys that are VK_CANCEL, as (vk, scan code, extended), never left to MapVirtualKey. With Ctrl held the Break key sends scan E0 46
    # (extended) and ScrollLock sends scan 46 (not extended); Windows gives both VK_CANCEL and a console reads both as Ctrl+Break. Rounds 8 and 9 sent
    # `ctrl+break` as scan 46 WITHOUT the extended flag (MapVirtualKey drops the E0 prefix and VK_CANCEL was not in EXTENDED): that is Ctrl+ScrollLock,
    # which the app's key layer names differently from the Break key. `break` / `cancel` are now the Break key; `scrollbreak` is Ctrl+ScrollLock.
    # Both are refused without ctrl (no keyboard sends VK_CANCEL without it).
    VK_CANCEL_KEYS = {"break": (0x03, 0x46, True), "cancel": (0x03, 0x46, True), "scrollbreak": (0x03, 0x46, False)}
    EXTENDED = {0x25, 0x26, 0x27, 0x28, 0x24, 0x23, 0x21, 0x22, 0x2D, 0x2E, 0xA5, 0xA3}
    MODVK = {"ctrl": 0xA2, "shift": 0xA0, "alt": 0xA4}
    PUNCT = {"`": (0xC0, 0), "~": (0xC0, 1), "-": (0xBD, 0), "_": (0xBD, 1), "=": (0xBB, 0), "+": (0xBB, 1), "[": (0xDB, 0), "{": (0xDB, 1),
             "]": (0xDD, 0), "}": (0xDD, 1), "\\": (0xDC, 0), "|": (0xDC, 1), ";": (0xBA, 0), ":": (0xBA, 1), "'": (0xDE, 0), '"': (0xDE, 1),
             ",": (0xBC, 0), "<": (0xBC, 1), ".": (0xBE, 0), ">": (0xBE, 1), "/": (0xBF, 0), "?": (0xBF, 1), " ": (0x20, 0)}
    SHIFTED_DIGITS = ")!@#$%^&*("

    def __init__(self):
        import ctypes
        from ctypes import wintypes as wt
        import winapi
        self.w = winapi
        self.ct = ctypes
        self.key_method = os.environ.get("APPCI_WIN_KEY_METHOD", "vk")        # vk | unicode
        self.calls = []
        self.fails = 0
        try:
            ctypes.windll.user32.SetProcessDPIAware()       # physical pixels in window_rect and ImageGrab
        except Exception:
            pass
        time.time = winapi.precise_time

        class KBD(ctypes.Structure):
            _fields_ = [("wVk", wt.WORD), ("wScan", wt.WORD), ("dwFlags", wt.DWORD), ("time", wt.DWORD), ("dwExtraInfo", ctypes.c_size_t)]

        class MOU(ctypes.Structure):
            _fields_ = [("dx", wt.LONG), ("dy", wt.LONG), ("mouseData", wt.DWORD), ("dwFlags", wt.DWORD), ("time", wt.DWORD), ("dwExtraInfo", ctypes.c_size_t)]

        class UN(ctypes.Union):
            _fields_ = [("mi", MOU), ("ki", KBD)]

        class INP(ctypes.Structure):
            _fields_ = [("type", wt.DWORD), ("u", UN)]
        self._INP = INP
        winapi.user32.SendInput.argtypes = [wt.UINT, ctypes.POINTER(INP), ctypes.c_int]
        winapi.user32.SendInput.restype = wt.UINT
        winapi.user32.MapVirtualKeyW.argtypes = [wt.UINT, wt.UINT]
        winapi.user32.MapVirtualKeyW.restype = wt.UINT
        self._cur = None
        self.outf = None

    # ------------------------------------------------------------ preflight
    def preflight(self):
        p = []
        try:
            import PIL.ImageGrab  # noqa: F401
        except ImportError:
            p.append("missing python module: Pillow (PIL.ImageGrab, screenshots)")
        try:
            h, pid = self.w.foreground()
            if not h:
                p.append("no foreground window: the runner is not on an unlocked interactive desktop (locked screen / session 0 service)")
        except Exception as e:
            p.append("user32 not usable: %r" % (e,))
        return p

    # ------------------------------------------------------------ low level
    def _send(self, evs):
        """evs: [(vk, scan, flags)] -> number SendInput accepted (it may refuse when another thread holds an input hook / UIPI)"""
        n = len(evs)
        arr = (self._INP * n)()
        for i, (vk, scan, fl) in enumerate(evs):
            arr[i].type = 1
            arr[i].u.ki.wVk = vk
            arr[i].u.ki.wScan = scan
            arr[i].u.ki.dwFlags = fl
        got = self.w.user32.SendInput(n, arr, self.ct.sizeof(self._INP))
        if self._cur is not None:
            self._cur["events"] += n
            self._cur["accepted"] += got
        if got != n:
            self.fails += 1
        return got

    def _vkev(self, vk, up=False):
        scan = self.w.user32.MapVirtualKeyW(vk, 0)
        fl = (2 if up else 0) | (1 if vk in self.EXTENDED else 0)
        return (vk, scan, fl)

    def _uev(self, unit, up=False):
        return (0, unit, 4 | (2 if up else 0))

    def char_vk(self, ch):
        """(vk, shift) of a printable ASCII character on a US layout, or None"""
        if "a" <= ch <= "z":
            return 0x41 + ord(ch) - 97, 0
        if "A" <= ch <= "Z":
            return 0x41 + ord(ch) - 65, 1
        if "0" <= ch <= "9":
            return 0x30 + ord(ch) - 48, 0
        if ch in self.SHIFTED_DIGITS:
            return 0x30 + self.SHIFTED_DIGITS.index(ch), 1
        return self.PUNCT.get(ch)

    def _char_events(self, ch):
        """key-down+key-up events that type one character (shift wrapped around it when the US layout needs it)"""
        if ch in ("\n", "\r"):
            return [self._vkev(0x0D), self._vkev(0x0D, True)]
        if ch == "\t":
            return [self._vkev(0x09), self._vkev(0x09, True)]
        cv = self.char_vk(ch) if (ord(ch) < 128 and self.key_method != "unicode") else None
        if cv:
            vk, sh = cv
            ev = [self._vkev(vk), self._vkev(vk, True)]
            return ([self._vkev(0xA0)] + ev + [self._vkev(0xA0, True)]) if sh else ev
        ev = []
        for u in self._utf16(ch):
            ev += [self._uev(u), self._uev(u, True)]
        return ev

    @staticmethod
    def _utf16(ch):
        b = ch.encode("utf-16-le")
        return [int.from_bytes(b[i:i + 2], "little") for i in range(0, len(b), 2)]

    # ------------------------------------------------------------ the injector record (#1482)
    def _begin(self, kind, enters_want=0):
        self._cur = dict(n=len(self.calls), kind=kind, wall0=time.time(), wall1=None, enters_want=enters_want, enter_wall=[], enters_posted=0,
                         chars=0, special=[], err=None, out=True, events=0, accepted=0, t_chars=[],
                         ref=bool(self.guard_override))      # r18u8: reference-console keys (interleaved latency) are not Phosphor's; injacct.join skips ref calls
        return self._cur

    def _end(self):
        rec, self._cur = self._cur, None
        rec["wall1"] = time.time()
        rec["span_ms"] = (rec["t_chars"][-1] - rec["t_chars"][0]) * 1000 if len(rec["t_chars"]) > 1 else 0
        if rec["accepted"] != rec["events"] and not rec["err"]:
            rec["err"] = "SendInput accepted %d of %d events (blocked by UIPI / an input hook?)" % (rec["accepted"], rec["events"])
        self.calls.append(rec)
        if self.inject_log:
            try:
                with open(self.inject_log, "a") as f:
                    f.write("call=%d kind=%s wall=%.3f..%.3f chars=%d span_ms=%.0f events=%d accepted=%d enters_want=%d enters_posted=%d out=True%s %s\n" % (
                        rec["n"], rec["kind"], rec["wall0"], rec["wall1"], rec["chars"], rec["span_ms"], rec["events"], rec["accepted"],
                        rec["enters_want"], rec["enters_posted"], (" ERR=" + rec["err"]) if rec["err"] else "", " ".join(rec["special"])))
            except OSError:
                pass
        return rec

    def inject_enter(self, idx):
        if idx is None or idx < 0 or idx >= len(self.calls):
            return None
        c = self.calls[idx]
        if c["enters_want"] and not c["enters_posted"]:
            return False, "injector call #%d did NOT post its ENTER (%s)" % (c["n"], c["err"] or "no Return key-down was accepted by SendInput")
        w = c["enter_wall"][0] if c["enter_wall"] else None
        return True, "injector call #%d posted its ENTER at %s" % (c["n"], (time.strftime("%H:%M:%S", time.localtime(w)) + (".%03d" % int((w % 1) * 1000))) if w else "?")

    def _burst(self, rec, evs, kind=None, enter=False):
        """send one group of events behind the guard; account chars/Enter in the record. Raises RuntimeError when SendInput refused events."""
        self.guard()
        got = self._send(evs)
        if enter and got == len(evs):
            rec["enters_posted"] += 1
            rec["enter_wall"].append(time.time())
        if kind:
            rec["special"].append("%s@%.3f" % (kind, time.time()))
        if got != len(evs):
            rec["err"] = "SendInput accepted %d of %d events (GetLastError %d)" % (got, len(evs), self.ct.get_last_error())
            raise RuntimeError(rec["err"])

    # ------------------------------------------------------------ typing
    def type(self, text, delay_ms=15, enter=False):
        rec = self._begin("type", 1 if enter else 0)
        try:
            n = len(text)
            for i, ch in enumerate(text):
                self._burst(rec, self._char_events(ch))
                rec["chars"] += 1
                rec["t_chars"].append(time.time())
                if delay_ms and i + 1 < n:
                    time.sleep(delay_ms / 1000)
            if enter:
                self._burst(rec, [self._vkev(0x0D), self._vkev(0x0D, True)], "ENTER", enter=True)
        except Exception as e:
            rec["err"] = rec["err"] or "%s: %s" % (type(e).__name__, str(e)[:120])
            raise
        finally:
            self._end()

    def _chord(self, spec):
        """'ctrl+shift+t' / 'Return' / 'Left' / 'a' -> (down events, up events)"""
        low = spec.lower()
        if low in self.UNSUPPORTED_CHORDS:
            raise ValueError("refusing forbidden chord " + spec)
        parts = low.split("+") if low != "+" else ["+"]
        mods, k = parts[:-1], parts[-1]
        want = set()
        for m in mods:
            m = {"control": "ctrl", "option": "alt"}.get(m, m)
            if m in ("super", "cmd", "meta", "logo", "win"):
                raise ValueError("refusing the Windows key in " + spec)
            if m == "altgr":
                want.add("altgr")
            elif m in self.MODVK:
                want.add(m)
            else:
                raise ValueError("unknown modifier in " + spec)
        if k in self.NAMED:
            vk, shift = self.NAMED[k], 0
        elif re.fullmatch(r"f([1-9]|1[0-2])", k):
            vk, shift = 0x70 + int(k[1:]) - 1, 0
        else:
            ch = spec if len(spec) == 1 else REV_ENVNAME.get(k, k)
            cv = self.char_vk(ch) if len(ch) == 1 else None
            if cv is None:
                raise ValueError("no US-layout key for %r" % spec)
            vk, shift = cv
        if shift:
            want.add("shift")
        order = [m for m in ("ctrl", "shift", "alt") if m in want]
        down = [self._vkev(self.MODVK[m]) for m in order]
        up = [self._vkev(self.MODVK[m], True) for m in reversed(order)]
        if "altgr" in want:
            down.append(self._vkev(0xA5))
            up.insert(0, self._vkev(0xA5, True))
        if k in self.VK_CANCEL_KEYS:
            if "ctrl" not in want:
                raise ValueError("%r needs ctrl: VK_CANCEL only exists with Ctrl held (#1906)" % spec)
            key_dn, key_up = self.vk_cancel_events(k)
        else:
            key_dn, key_up = self._vkev(vk), self._vkev(vk, True)
        return down + [key_dn], [key_up] + up, vk

    @classmethod
    def vk_cancel_events(cls, name):
        """(key-down, key-up) SendInput events (vk, scan, flags) of a VK_CANCEL_KEYS key: flags bit 0 = KEYEVENTF_EXTENDEDKEY, bit 1 = KEYEVENTF_KEYUP"""
        vk, scan, ext = cls.VK_CANCEL_KEYS[name]
        fl = 1 if ext else 0
        return (vk, scan, fl), (vk, scan, fl | 2)

    def key(self, spec, repeat=1, delay_ms=15):
        self.keys([spec] * repeat, delay_ms=delay_ms)

    def keys(self, specs, delay_ms=15):
        chords = [self._chord(s) for s in specs]
        enters = sum(1 for c in chords if c[2] == 0x0D)
        rec = self._begin("keys", enters)
        try:
            for i, (dn, up, vk) in enumerate(chords):
                self._burst(rec, dn + up, kind=specs[i], enter=(vk == 0x0D))
                if delay_ms and i + 1 < len(chords):
                    time.sleep(delay_ms / 1000)
        except Exception as e:
            rec["err"] = rec["err"] or "%s: %s" % (type(e).__name__, str(e)[:120])
            raise
        finally:
            self._end()

    def altgr(self, vk):
        """AltGr + key: Right Alt (extended). On a layout with AltGr the system generates the Left Control that goes with it."""
        rec = self._begin("altgr")
        try:
            self._burst(rec, [self._vkev(0xA5), self._vkev(vk), self._vkev(vk, True), self._vkev(0xA5, True)], "ALTGR+%02X" % vk)
        finally:
            self._end()

    def plain_vk(self, vk):
        """one key by virtual-key code alone: the receiving thread's layout decides which character it is"""
        rec = self._begin("vk")
        try:
            self._burst(rec, [self._vkev(vk), self._vkev(vk, True)], "VK%02X" % vk)
        finally:
            self._end()

    def roll(self, text, overlap_ms=10, enter=True):
        """Rollover: the next key goes down before the previous one is released (shifted / non-ASCII characters are plain taps)"""
        rec = self._begin("roll", 1 if enter else 0)
        pending = None          # vk of the key still held down
        try:
            for ch in text:
                cv = self.char_vk(ch) if ord(ch) < 128 and self.key_method != "unicode" else None
                if cv is None or cv[1]:
                    if pending is not None:
                        self._burst(rec, [self._vkev(pending, True)])
                        pending = None
                    self._burst(rec, self._char_events(ch))
                    rec["chars"] += 1
                    rec["t_chars"].append(time.time())
                    time.sleep(overlap_ms / 1000)
                    continue
                self._burst(rec, [self._vkev(cv[0])])
                rec["chars"] += 1
                rec["t_chars"].append(time.time())
                time.sleep(overlap_ms / 1000)
                if pending is not None:
                    self._burst(rec, [self._vkev(pending, True)])
                time.sleep(overlap_ms / 1000)
                pending = cv[0]
            if pending is not None:
                self._burst(rec, [self._vkev(pending, True)])
            if enter:
                self._burst(rec, [self._vkev(0x0D), self._vkev(0x0D, True)], "ENTER", enter=True)
        except Exception as e:
            rec["err"] = rec["err"] or "%s: %s" % (type(e).__name__, str(e)[:120])
            raise
        finally:
            self._end()

    def noop(self):
        """one SendInput with nothing to do (a Shift key-up while Shift is not down): the harness floor, no guard (it must not raise)"""
        self._send([self._vkev(0xA0, True)])

    # ------------------------------------------------------------ focus guard
    def _pname_ok(self, pid, names):
        return self.w.proc_name(pid) in names

    def guard(self):
        h, pid = self.w.foreground()
        ov = self.guard_override
        if ov:
            if pid in ov.get("pids", ()) or h in ov.get("hwnds", ()):
                return
            raise RuntimeError("foreground window is %r (pid %s), not the reference console: refusing to send keys" % (self.w.proc_name(pid), pid))
        ph = self.pid_fn() if self.pid_fn else None
        if ph is None:
            raise RuntimeError("Phosphor PID unknown (app not running?): refusing to send keys")
        if pid == ph:
            return
        self._focus_pid(ph)
        h2, pid2 = self.w.foreground()
        if pid2 != ph:
            raise RuntimeError("foreground window is %r (pid %s), not Phosphor (pid %s): refusing to send keys" % (self.w.proc_name(pid2), pid2, ph))

    def _focus_pid(self, pid):
        h = self.w.main_window(pid)
        if h:
            self.w.focus_window(h)
            time.sleep(0.4)

    def refocus(self, verify=False):
        """bring Phosphor back to the front after the reference window closed (no keys sent)"""
        ph = self.pid_fn() if self.pid_fn else None
        if ph:
            self._focus_pid(ph)

    # ------------------------------------------------------------ pointer and window size (r15 wave 4, #2060; r14-test-gaps T31 / T33)
    # The pointer acts only on the Phosphor window this run started: never while the reference console is driven (guard_override), and a button goes
    # down only when the top-level window under the pointer belongs to that pid. winapi.Pointer reads every move back, so a pointer that did not
    # arrive is a PointerError (harness / machine), not a click the app ignored. Pointer calls are lines in <run>/inject.log (`ptr ...`); they are not
    # records in self.calls: the key accounting (injacct) counts keys.
    _ptr = None
    RESIZE_WAIT = 2.0        # seconds a posted SetWindowPos / ShowWindow is waited for before the rectangle is taken as it is (a give-up net)

    def _app_hwnd(self):
        ph = self.pid_fn() if self.pid_fn else None
        return self.w.main_window(ph) if ph else None

    def _ptr_log(self, text):
        if self.inject_log:
            try:
                with open(self.inject_log, "a") as f:
                    f.write("ptr wall=%.3f %s\n" % (time.time(), text))
            except OSError:
                pass

    def pointer(self):
        if self._ptr is None:
            self._ptr = self.w.Pointer()
        return self._ptr

    def client_origin(self):
        """(x, y) on screen of the app window's client area (what the app's `appci_find` positions are relative to), or None"""
        rc = self.w.client_rect_screen(self._app_hwnd())
        return (rc[0], rc[1]) if rc else None

    def _ptr_guard(self):
        if self.guard_override:
            raise RuntimeError("the pointer is for the Phosphor window only: refusing while the reference console is the target")
        self.guard()

    def ptr_move(self, x, y):
        self._ptr_guard()
        pos = self.pointer().move(x, y)
        self._ptr_log("move %d %d at=%d,%d" % (int(x), int(y), pos[0], pos[1]))

    def ptr_button(self, button, down):
        """press or release a button where the pointer is. A press is refused unless the window under the pointer is Phosphor's; a release always goes out."""
        if down:
            self._ptr_guard()
            pos = self.w.cursor_pos()
            ph = self.pid_fn() if self.pid_fn else None
            pid = self.w.root_window_at(pos[0], pos[1])[1] if pos else 0
            if not pos or pid != ph:
                raise RuntimeError("the window under the pointer at %s belongs to %r (pid %s), not Phosphor (pid %s): refusing to click" % (
                    pos, self.w.proc_name(pid), pid, ph))
        self.pointer().button(button, down)
        self._ptr_log("button %d %s" % (button, "down" if down else "up"))

    def ptr_wheel(self, steps):
        self._ptr_guard()
        self.pointer().wheel(steps)
        self._ptr_log("wheel %+d" % steps)

    def ptr_ready(self):
        """None when the pointer can be used on the app window, else why not. The proof is a move to the middle of the window's client area, read
        back, with the window under it Phosphor's (a hover: nothing is clicked)."""
        h = self._app_hwnd()
        rc = self.w.client_rect_screen(h)
        if not rc or rc[2] < 100 or rc[3] < 100:
            return "the app window's client area could not be read (ClientToScreen / GetClientRect gave %r)" % (rc,)
        x, y = rc[0] + rc[2] // 2, rc[1] + rc[3] // 2
        try:
            self.ptr_move(x, y)
        except Exception as e:
            return str(e)
        ph = self.pid_fn() if self.pid_fn else None
        pid = self.w.root_window_at(x, y)[1]
        if pid != ph:
            return "the middle of the app window (%d, %d) is covered by a window of %r (pid %s)" % (x, y, self.w.proc_name(pid), pid)
        return None

    def mod_hold(self, mod, down):
        """hold or release one modifier (ctrl / shift / alt) around pointer actions; outside the injector records, like the Linux tools' held modifier"""
        if mod not in self.MODVK:
            raise ValueError("unknown modifier %r" % (mod,))
        if down:
            self.guard()
        if self._send([self._vkev(self.MODVK[mod], up=not down)]) != 1:
            raise RuntimeError("SendInput refused the %s key %s" % (mod, "press" if down else "release"))
        held = self.__dict__.setdefault("_mods_held", set())
        (held.add if down else held.discard)(mod)
        self._ptr_log("mod %s %s" % (mod, "down" if down else "up"))

    def release_held(self):
        """release every pointer button and modifier still down (a case that died between press and release) -> what was released, [] when nothing
        was held. run.py calls it after every case (post_case_invariants): a held button or Shift would bend every later key and click. Never raises."""
        out = []
        try:
            ptr = self._ptr
            for b in sorted(getattr(ptr, "held", ()) or ()):
                out.append("mouse button %d" % b)
            if out:
                ptr.release_all()
            for mod in sorted(self.__dict__.get("_mods_held", ())):
                out.append("the %s key" % mod)
                self._send([self._vkev(self.MODVK[mod], up=True)])
            self.__dict__["_mods_held"] = set()
        except Exception as e:
            out.append("(release failed: %r)" % (e,))
        return out

    def window_save(self):
        """the app window as it is now, for window_restore: dict(hwnd, rect=(x, y, w, h), zoomed), or None without a window"""
        h = self._app_hwnd()
        rect = self.w.window_rect(h) if h else None
        return dict(hwnd=h, rect=rect, zoomed=self.w.is_zoomed(h)) if rect else None

    def window_size(self):
        rect = self.w.window_rect(self._app_hwnd())
        return (rect[2], rect[3]) if rect else None

    def _await_rect(self, h, done):
        """the window's rectangle once done(rect) holds; the last one read after RESIZE_WAIT (SetWindowPos is posted, not awaited)"""
        t = time.time()
        rect = self.w.window_rect(h)
        while not (rect and done(rect)) and time.time() - t < self.RESIZE_WAIT:
            time.sleep(0.01)
            rect = self.w.window_rect(h)
        return rect

    def window_resize(self, w, h):
        """give the app window the outer size (w, h) in pixels -> None, or why it did not take. A maximised window is restored first (it keeps its
        size otherwise); the result is read back from GetWindowRect (winapi.resize_verdict: Windows may clamp the size, a change is the proof)."""
        hw = self._app_hwnd()
        before = self.w.window_rect(hw) if hw else None
        if not before:
            return "the app has no window"
        if self.w.user32.IsIconic(hw) or self.w.is_zoomed(hw):
            self.w.show_window(hw, self.w.SW_RESTORE)
            before = self._await_rect(hw, lambda r: not self.w.is_zoomed(hw)) or before
        if (before[2], before[3]) == (int(w), int(h)):
            return None
        if not self.w.set_window_rect(hw, w, h):
            return "SetWindowPos(%d, %d) failed: GetLastError %d" % (w, h, self.ct.get_last_error())
        after = self._await_rect(hw, lambda r: (r[2], r[3]) != (before[2], before[3]))
        err = self.w.resize_verdict(before[2:], (int(w), int(h)), after[2:] if after else None)
        self._ptr_log("resize asked=%dx%d got=%s%s" % (w, h, "%dx%d" % after[2:] if after else "?", " ERR" if err else ""))
        return err

    def window_restore(self, saved):
        """put the window back as window_save found it (place, size, maximised) and give it the keyboard; never raises"""
        try:
            if not saved or not saved.get("rect"):
                return
            hw, (x, y, w, h) = saved["hwnd"], saved["rect"]
            if saved.get("zoomed"):
                self.w.show_window(hw, self.w.SW_MAXIMIZE)
            else:
                if self.w.is_zoomed(hw):
                    self.w.show_window(hw, self.w.SW_RESTORE)
                    self._await_rect(hw, lambda r: not self.w.is_zoomed(hw))
                self.w.set_window_rect(hw, w, h, x, y)
                self._await_rect(hw, lambda r: tuple(r) == (x, y, w, h))
            self.refocus()
        except Exception:
            pass

    # ------------------------------------------------------------ windows, screenshots, clipboard
    def window_rect(self, pid=None):
        ph = pid or (self.pid_fn() if self.pid_fn else None)
        h = self.w.main_window(ph) if ph else None
        if not h:
            return None
        for w in self.w.top_windows(ph):
            if w[0] == h:
                return tuple(float(v) for v in w[2])
        return None

    def windows(self):
        ph = self.pid_fn() if self.pid_fn else None
        return [{"hwnd": int(w[0] or 0), "pid": w[1], "name": w[3], "rect": w[2]} for w in self.w.top_windows(ph) if w[2][2] > 50] if ph else []

    def shot(self, path):
        from PIL import ImageGrab
        try:
            img = ImageGrab.grab(all_screens=True)
        except TypeError:
            img = ImageGrab.grab()
        img.save(path)
        return os.path.exists(path) and os.path.getsize(path) > 1000

    def _clip(self):
        u, k = self.w.user32, self.w.kernel32
        from ctypes import wintypes as wt
        u.OpenClipboard.argtypes = [self.ct.c_void_p]
        u.SetClipboardData.argtypes = [wt.UINT, self.ct.c_void_p]
        u.SetClipboardData.restype = self.ct.c_void_p
        u.GetClipboardData.argtypes = [wt.UINT]
        u.GetClipboardData.restype = self.ct.c_void_p
        k.GlobalAlloc.argtypes = [wt.UINT, self.ct.c_size_t]
        k.GlobalAlloc.restype = self.ct.c_void_p
        k.GlobalLock.argtypes = [self.ct.c_void_p]
        k.GlobalLock.restype = self.ct.c_void_p
        k.GlobalUnlock.argtypes = [self.ct.c_void_p]
        return u, k

    def _open_clip(self, u):
        for _ in range(40):
            if u.OpenClipboard(None):
                return True
            time.sleep(0.05)
        raise RuntimeError("clipboard is held by another process")

    def clip_set(self, data):
        u, k = self._clip()
        text = data.decode("utf-8", "replace") if isinstance(data, (bytes, bytearray)) else data
        raw = (text + "\0").encode("utf-16-le")
        self._open_clip(u)
        try:
            u.EmptyClipboard()
            h = k.GlobalAlloc(0x0002, len(raw))                  # GMEM_MOVEABLE
            p = k.GlobalLock(h)
            self.ct.memmove(p, raw, len(raw))
            k.GlobalUnlock(h)
            if not u.SetClipboardData(13, h):                      # CF_UNICODETEXT
                raise RuntimeError("SetClipboardData failed: %d" % self.ct.get_last_error())
        finally:
            u.CloseClipboard()

    def clip_get(self):
        u, k = self._clip()
        self._open_clip(u)
        try:
            h = u.GetClipboardData(13)
            if not h:
                return b""
            p = k.GlobalLock(h)
            try:
                return self.ct.wstring_at(p).encode("utf-8")
            finally:
                k.GlobalUnlock(h)
        finally:
            u.CloseClipboard()


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
    if name == "windows":
        return WindowsBackend()
    if name == "sim":
        return SimBackend(**kw)
    raise SystemExit("unknown backend " + name)
