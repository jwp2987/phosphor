# appci/backend.py: input / screenshot / clipboard backends for the CI real-app harness.
# Backends: wayland (wtype + grim + wl-clipboard, for headless sway; also used for apps under XWayland),
#           x11 (xdotool + xsel + ffmpeg x11grab, private Xvfb), mac (htype2 + kl3 CGEvent tools + osascript + screencapture),
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
MODS = {"ctrl": "ctrl", "control": "ctrl", "shift": "shift", "alt": "alt", "super": "logo", "cmd": "logo", "meta": "logo"}


def split_key(spec):
    """'ctrl+shift+t' -> (['ctrl','shift'], 't'). A literal '+' key is written 'plus'."""
    parts = spec.split("+")
    return [MODS[p.lower()] for p in parts[:-1]], parts[-1]


class Backend:
    name = "base"
    key_floor_ms = 0.0   # measured cost of one input-tool invocation (harness overhead), filled by calibrate()

    inject_log = None        # path of <run>/inject.log (set by Ctx); only the Mac backend records injector calls so far (#1482)
    calls = None             # per-injector-call records, in order (Mac only); None = this backend cannot say what it posted

    def preflight(self):
        return []

    def inject_enter(self, idx):
        """what the injector says about the Enter of the idx-th recorded call: (posted: bool, text) or None when unknown"""
        return None

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
        # #1414: every wtype call uploads its own keymap (new virtual keyboard) and under headless sway the FIRST key after it never
        # reached the client. The call is therefore led by a harmless Shift_L tap + a short sleep; the key_loss_control case measures
        # primed vs unprimed vs one-shot vs a reference terminal. APPCI_WL_PRIME_MS=0 turns the priming off.
        self.prime_ms = int(os.environ.get("APPCI_WL_PRIME_MS", "20"))

    def preflight(self):
        p = []
        for t in ("wtype", "grim", "wl-copy", "wl-paste", "swaymsg"):
            if not shutil.which(t):
                p.append("missing tool: " + t)
        for v in ("WAYLAND_DISPLAY", "XDG_RUNTIME_DIR"):
            if not os.environ.get(v):
                p.append("env not set: " + v)
        return p

    def _wtype(self, args, prime=True):
        if prime and self.prime_ms > 0:
            args = ["-k", "Shift_L", "-s", str(self.prime_ms)] + args
        return _run(["wtype"] + args)

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
    behind the guard, so it reaches Phosphor's window only."""
    name = "mac"
    APP = os.environ.get("APPCI_MAC_PROCESS", "phosphor-oss")
    TOOLS = os.environ.get("APPCI_MAC_TOOLS", "")
    SPECIAL = {"return": 36, "enter": 36, "tab": 48, "escape": 53, "backspace": 51, "delete": 51, "left": 123, "right": 124,
               "down": 125, "up": 126, "home": 115, "end": 119, "space": 49}
    LETTERS = {"a": 0, "s": 1, "d": 2, "f": 3, "h": 4, "g": 5, "z": 6, "x": 7, "c": 8, "v": 9, "b": 11, "q": 12, "w": 13, "e": 14,
               "r": 15, "y": 16, "t": 17, "1": 18, "2": 19, "3": 20, "4": 21, "6": 22, "5": 23, "=": 24, "9": 25, "7": 26, "-": 27,
               "8": 28, "0": 29, "]": 30, "o": 31, "u": 32, "[": 33, "i": 34, "p": 35, "l": 37, "j": 38, "'": 39, "k": 40,
               ";": 41, "\\": 42, ",": 43, "/": 44, "n": 45, "m": 46, ".": 47}
    MODBIT = {"shift": 1, "ctrl": 2, "alt": 4, "logo": 8}
    NEVER_CMD = ("q", "w", "tab", "space", "h", "m")
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
            p.append("osascript/System Events denied: grant Accessibility to the runner's process chain (%s)" % r.stderr.decode().strip()[:120])
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

    def system_dialog(self, ocr=False):
        """description of a system dialog that is on screen, else None. Cheap path: WindowServer window list (the Local Network
        prompt is NOT frontmost per lsappinfo: the menu bar still says Phosphor, so the front-app check alone cannot see it).
        ocr=True adds a Vision pass over a screenshot for 'Allow' + 'local network' (used once before the first key)."""
        try:
            r = subprocess.run([self._tool("winrect"), "list"], capture_output=True, timeout=15)
            for l in r.stdout.decode(errors="replace").splitlines():
                f = l.split("\t")
                if len(f) >= 2 and f[1].lower().replace(" ", "") in self.DIALOG_OWNERS:
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
            "A new ad-hoc-signed build can re-trigger it (docs/testing.md 3.3). The harness never clicks it. Then re-run the job." % what)

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

    def refocus(self):
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
                   enter_wall=[], chars=0, special=[], err=err, out=False)
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

    def _kspec(self, spec):
        """'ctrl+u' / 'Return' / 'a' -> htype2 op"""
        low = spec.lower()
        parts = low.split("+") if low != "+" else ["+"]
        mods, k = parts[:-1], parts[-1]
        bits = 0
        for m in mods:
            m = {"control": "ctrl", "super": "logo", "cmd": "logo", "meta": "logo", "option": "alt"}.get(m, m)
            if m not in self.MODBIT:
                raise ValueError("unknown modifier in " + spec)
            bits |= self.MODBIT[m]
        if k in ("globe", "fn") or (bits & 2 and bits & 8) or (bits & 8 and k in self.NEVER_CMD):
            raise ValueError("refusing forbidden chord " + spec)
        code = self.SPECIAL.get(k, self.LETTERS.get(k))
        if code is None:
            raise ValueError("no key code for " + spec)
        return "k:%d:%d:%d"

    def key(self, spec, repeat=1, delay_ms=15):
        self.keys([spec] * repeat, delay_ms=delay_ms)

    def keys(self, specs, delay_ms=15):
        ops = []
        for s in specs:
            fmt = self._kspec(s)
            low = s.lower().split("+")
            bits = 0
            for m in low[:-1]:
                bits |= self.MODBIT[{"control": "ctrl", "super": "logo", "cmd": "logo", "meta": "logo", "option": "alt"}.get(m, m)]
            code = self.SPECIAL.get(low[-1], self.LETTERS.get(low[-1]))
            ops.append(fmt % (code, bits, int(delay_ms)))
        self._ht(ops)

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
        """one kl3 command (guarded); returns its output line"""
        self.guard()
        self._kl.stdin.write(line + "\n")
        self._kl.stdin.flush()
        return self._kl.stdout.readline().strip()

    def kl_stop(self):
        p, self._kl = self._kl, None
        if p and p.poll() is None:
            try:
                p.stdin.write("Q\n"); p.stdin.flush(); p.wait(3)
            except Exception:
                p.kill()


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
