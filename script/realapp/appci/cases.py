# appci/cases.py: the release-test matrix of docs/testing.md 3.5 as machine-checkable cases.
# Every case verifies the EXACT command / bytes the shell received (effect files written by the typed commands
# plus the shell's own rec.log from the scratch rc), never "something ran". Cases run once per session mode
# (terminal, agent) unless marked otherwise. A case function gets (ctx, r); r.check(ok, msg) records failures,
# r.metric(k, v) numbers, r.skip(msg) a skip. Typing is at the owner's stress rate: 10-25 ms/key, Enter in the same call.
import hashlib, json, os, random, re, shutil, statistics, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lat  # noqa: E402

CASES = []
# APPCI_TIER=smoke (CI tiers 'fix', docs/testing.md 'Test tiers'): the input-path cases a fix push needs, both modes, ~10 min per platform.
# ONE list for Linux, macOS and Windows (#1488): a case that cannot run on a platform / pane shell SKIPS there with a reason (winmap.py), never silently.
# The two ssh hand-over cases are in it for the ConPTY / hand-over release blocker (#1349).
SMOKE = ("mode_proof", "key_loss_control", "enter_loss_control", "cadence", "fast_typing", "paste_instant_enter", "tab_enter", "ctrl_c_d", "ssh_connect_typing",
         "ssh_enter_then_immediate_command", "ssh_nested_typeahead", "shell_fish", "shell_nu", "latency_enter", "latency_key_log", "latency_paint", "md_viewer",
         # owner 2026-11-02 ("still not doing agent?"): the agent pass must also send REAL prompts, not only shell lines typed into the agent view
         "routing_table", "agent_stream_cancel")


def case(name, modes=("terminal", "agent"), gui=False, order=50):
    def deco(f):
        CASES.append(dict(name=name, modes=modes, gui=gui, order=order, fn=f))
        return f
    return deco


class Skip(Exception):
    pass


class Result:
    def __init__(self, name, mode):
        self.name, self.mode = name, mode
        self.fails, self.notes, self.metrics, self.shots, self.skipped = [], [], {}, [], None
        self.noref = []      # reasons this case could not be judged against a reference terminal: status becomes "noref" (NOT a pass) unless a check failed

    def check(self, ok, msg):
        if not ok:
            self.fails.append(msg)
        return ok

    def note(self, msg):
        self.notes.append(msg)

    def no_reference(self, msg):
        self.noref.append(msg)
        self.notes.append("NO-REFERENCE: " + msg)

    def metric(self, k, v):
        self.metrics[k] = round(v, 3) if isinstance(v, float) else v

    def skip(self, msg):
        raise Skip(msg)


def pct(xs, p):
    xs = sorted(xs)
    return xs[min(len(xs) - 1, int(len(xs) * p))] if xs else None


def stats(xs):
    return {"n": len(xs), "p50": round(statistics.median(xs), 2) if xs else None, "p95": round(pct(xs, 0.95), 2) if xs else None,
            "max": round(max(xs), 2) if xs else None}


def plat():
    return os.environ.get("APPCI_PLATFORM", "linux")


def lat_checks(r, rec, label):
    """turn a latency record's verdicts into case failures (FAIL); WARN stays a note; no-reference makes the case status 'noref' (not a pass, #1499)"""
    if rec["budget_verdict"] == "fail":
        if rec["budget_kind"] == "overhead":
            r.check(False, "%s: Phosphor overhead %.2f ms over %s exceeds the %.0f ms budget" % (label, rec["overhead_ms"], rec["reference_name"], rec["budget_ms"]))
        else:
            r.check(False, "%s: %.2f ms over the harness floor exceeds the %.0f ms budget" % (label, rec["overhead_ms"], rec["budget_ms"]))
    for tag in ("prev", "baseline"):
        x = rec.get(tag) or {}
        if x.get("verdict") in ("fail", "warn") and x.get("delta_ms") is not None:
            msg = "%s: median %s vs %s build %+.2f ms (%+.1f%%)" % (label, rec["phosphor"]["p50"], tag, x["delta_ms"], x["delta_pct"])
            if rec.get("remeasure"):
                msg += "; re-measured once: median %s (%s)" % (rec["remeasure"]["phosphor"]["p50"], rec["remeasure"][tag]["verdict"])
            if x["verdict"] == "fail":
                r.check(False, "REGRESSION " + msg)
            else:
                r.note("WARN " + msg)
    if rec["verdict"] == "no-reference":
        r.no_reference("%s: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as metrics)" % label)
    for n_ in rec.get("notes", []):
        r.note("%s: %s" % (label, n_))


class Ctx:
    def __init__(self, be, prof, app, run, mode, seed=1, ssh=None, ollama=False, log_path=None, ssh_why=None, pane=None):
        self.be, self.prof, self.app, self.run, self.mode = be, prof, app, run, mode
        self.pane = pane                      # Windows only: the shell of the app's pane, 'powershell' | 'gitbash' (None = the POSIX bash/zsh harness)
        self.win = pane is not None
        # label = the mode column of results.json / junit and the id prefix of latency records: 'terminal' everywhere except a non-default
        # Windows pane ('terminal@gitbash'), so the two panes' numbers and previous-build comparisons never mix
        self.label = mode if pane in (None, "powershell") else "%s@%s" % (mode, pane)
        self.t_case0 = time.time()
        self.rng = random.Random(seed)
        self.ssh, self.ollama, self.log_path = ssh, ollama, log_path
        self.ssh_why = ssh_why     # why there is no usable ssh test host (sshd missing, or the setup probe `ssh fake true` failed); None when ctx.ssh is set
        self.W = prof.home + "/w"
        self.opts = {}                # tier options of this pass (winmap.tier_plan): ref, ft_gaps, cc_short; empty = the full tier
        self.enter_control = None     # cached result of run_enter_control (#1482)
        self.cc_integrity = {}        # ctrlc_samples: per output file, how many typed sample lines the shell received exactly
        try:
            be.inject_log = os.path.join(run, "inject.log")
        except Exception:
            pass
        self.n = 0
        self.scale = float(os.environ.get("APPCI_SCALE", "1"))   # <1 shortens counts for smoke runs
        # the app's own pane has no rc hook (Windows PowerShell): rec.log is empty, prompt.log is empty. Cases already treat an empty rec.log as
        # "effect file only"; places that WAIT for a local prompt use a fixed settle instead (leave_remote)

    def count(self, n):
        return max(2, int(n * self.scale))

    def tok(self, n=8):
        self.n += 1
        return "T%d_%s" % (self.n, "".join(self.rng.choice("abcdefghijklmnopqrstuvwxyz0123456789") for _ in range(n)))

    def until(self, pred, timeout=10, step=0.05):
        t = time.time()
        while time.time() - t < timeout:
            v = pred()
            if v:
                return v
            time.sleep(step)
        return pred()

    def lines(self, name, remote=False):
        base = self.prof.rhome if remote else self.W
        for attempt in range(5):
            try:
                return open(os.path.join(base, name), errors="replace").read().splitlines()
            except FileNotFoundError:
                return []
            except PermissionError:       # Windows: the shell still has the file open for writing (r18cm crashed here); read again shortly
                time.sleep(0.05 * (attempt + 1))
        return []

    def rm(self, *names):
        for n in names:
            try:
                os.remove(os.path.join(self.W, n))
            except FileNotFoundError:
                pass

    def append_cmd(self, shell, tok, fname):
        if shell == "nu":
            return '"%s\\n" | save --append %s' % (tok, fname)     # nu's save --append adds no newline: give each line one
        if shell == "pwsh":      # PowerShell: `>>` would write UTF-16 in 5.1; Add-Content writes ASCII lines
            return "Add-Content %s %s" % (fname, tok)
        return "echo %s >> %s" % (tok, fname)

    def pane_shell(self):
        """the shell name append_cmd() takes for the app's own pane: 'pwsh' in a Windows PowerShell pane, else 'bash'"""
        return "pwsh" if self.pane == "powershell" else "bash"

    def which(self, name):
        """shutil.which, on Windows against the PATH the app gets (Git for Windows' bin + usr/bin appended; never WSL's System32 bash.exe)"""
        if not self.win:
            return shutil.which(name)
        import winapi
        return shutil.which(name, path=os.pathsep.join(winapi.git_dirs()))

    def local_lines(self, name):
        """lines of a file the LOCAL shell would have written for `echo X >> ~/name` / `>> name`: the case dir, and in a PowerShell pane also
        the user's real home (PowerShell resolves ~ to USERPROFILE, not the scratch HOME); a home file counts only if created after this case began"""
        out = self.lines(name)
        if self.pane == "powershell":
            hp = os.path.join(os.path.expanduser("~"), name)
            try:
                if os.path.getmtime(hp) >= self.t_case0 - 1:
                    out += open(hp, errors="replace").read().splitlines()
            except OSError:
                pass
        return out

    def prompts(self):
        return self.prof.prompts()

    SHELLS = ("bash", "zsh", "fish", "nu", "sh", "dash")

    def shell_count(self):
        """number of shells (bash/zsh/fish/nu/sh) among the descendants of the Phosphor process, from `ps` (no pgrep/pkill, no name matching of
        the app); None when the app PID or ps is unavailable. The app logs nothing when the local shell exits, so the process tree is the evidence."""
        if self.win:
            return None        # no `ps`; a Windows process count would be fooled by the app's own short-lived -NoProfile PowerShell helpers
        try:
            root = self.app.pid() if self.app else None
            if not root:
                return None
            out = subprocess.run(["ps", "-A", "-o", "pid=,ppid=,comm="], capture_output=True, text=True, timeout=10).stdout
        except Exception:
            return None
        kids, comm = {}, {}
        for l in out.splitlines():
            f = l.split(None, 2)
            if len(f) == 3 and f[0].isdigit() and f[1].isdigit():
                kids.setdefault(int(f[1]), []).append(int(f[0]))
                comm[int(f[0])] = os.path.basename(f[2].strip()).lstrip("-")
        n, todo, seen = 0, [int(root)], set()
        while todo:
            x = todo.pop()
            if x in seen:
                continue
            seen.add(x)
            todo += kids.get(x, [])
            if x != int(root) and comm.get(x) in self.SHELLS:
                n += 1
        return n

    def shell_gone(self, base):
        """True when fewer shells run under the app than at `base` (a shell_count() taken at case start): the LOCAL shell exited"""
        now = self.shell_count()
        return base is not None and now is not None and now < base

    def where_now(self, timeout=8):
        """Which shell is in front, proven by an effect: a fresh token written to ~/.appci_where lands in the REMOTE home ("remote"), the local
        scratch home ("local"), or nowhere ("none")."""
        probe = "w" + self.tok(8)
        self.be.type("echo %s >> ~/.appci_where" % probe, 12, enter=True)
        def local_where():
            try:
                return open(os.path.join(self.prof.home, ".appci_where"), errors="replace").read().split()
            except OSError:
                return []
        self.until(lambda: probe in self.lines(".appci_where", remote=True) or probe in local_where(), timeout)
        if probe in self.lines(".appci_where", remote=True):
            return "remote"
        return "local" if probe in local_where() else "none"

    def return_to_local(self, max_levels=3):
        """After an ssh case, pass or fail: leave every remote level until a probe lands in the LOCAL home. `exit` is typed only while the
        probe proves a remote shell is in front (never at the local shell). Returns "local", "remote" (still remote after max_levels) or
        "none" (no shell answered, even after Ctrl-C/Ctrl-U)."""
        for _ in range(max_levels + 1):
            where = self.where_now()
            if where == "none":
                self.reset()
                where = self.where_now()
            if where != "remote":
                return where
            before = self.prof.prompts()
            self.be.type("exit", 12, enter=True)
            self.until(lambda: self.prof.prompts() > before, 10)
            time.sleep(0.5)
        return "remote"

    def leave_remote(self, r, label, rfile, tok, base, timeout=10, level=False):
        """Type `exit` ONLY if a remote shell is provably in front (tok reached the REMOTE file rfile). Otherwise `exit` would end the
        LOCAL shell and every later key would go to the agent (#1481): record the miss, reset the pane, and return False so the caller stops
        its trials. level=True leaves one level of a nested ssh (waits for a REMOTE prompt). Fails fast when the shell count under the app
        dropped below `base`. Returns True after a clean exit."""
        if tok not in self.lines(rfile, remote=True):
            r.check(False, "%s: %s never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, phosphor.log). Test host: %s" % (label, tok, self.ssh_diag()))
            self.reset()
            return False
        # #1481 follow-up (r18ck): `tok` proves a remote shell was in front at SOME point, not that one still is (a nested ssh that failed
        # left one level, and the second `exit` closed the last LOCAL pane, quitting the app). Prove it again, now, before EVERY exit:
        # a fresh token must land in the REMOTE home, never the local one.
        probe = "w" + self.tok(8)
        self.be.type("echo %s >> ~/.appci_where" % probe, 12, enter=True)
        def local_where():          # `~` in the LOCAL shell is the scratch home, not the work dir that lines() reads
            try:
                return open(os.path.join(self.prof.home, ".appci_where"), errors="replace").read().split()
            except OSError:
                return []
        self.until(lambda: probe in self.lines(".appci_where", remote=True) or probe in local_where(), timeout)
        if probe not in self.lines(".appci_where", remote=True):
            where = "the LOCAL shell" if probe in local_where() else "no shell"
            r.check(False, "%s: before `exit`, a fresh probe ran in %s, not the remote one: `exit` NOT typed; stopping" % (label, where))
            self.reset()
            return False
        count = (lambda: self.prof.prompts(remote=True)) if level else self.prompts
        before = count()
        self.be.type("exit", 12, enter=True)
        if not level and not self.prof.local_prompts:
            time.sleep(min(timeout, 2.0))        # no local prompt hook in this pane (Windows PowerShell): fixed settle
        else:
            self.until(lambda: count() > before, timeout)
        if self.shell_gone(base):
            r.check(False, "%s: the LOCAL shell exited after `exit` (fewer shells under the app than at case start); stopping" % label)
            return False
        time.sleep(0.5)
        return True

    def pane_responsive(self, first=8, second=40):
        """The pane's liveness by an EFFECT: type a marker command that appends to a file and look for the file. Returns 0 when the first try worked, 1 when
        only the retry did (the pane needed Ctrl-C x3 to take typing again: r18co, Git Bash pane after cadence), None when no marker ever appeared (wedged).
        A wedged pane (a command that ignores Ctrl-C) fails every later case for a reason that is not theirs, so run.py checks after a failing case and
        relaunches the app. NOT judged by a prompt: the PowerShell pane has no prompt hook (the app starts it with -NoProfile), r18co's 'no prompt came back'
        wording was wrong, and the probe's file went to the real home because the pane was not in the case dir (local_lines also looks there)."""
        for tries, wait in ((0, first), (1, second)):
            tok = self.tok(8).lower()
            fname = "alive_%s.txt" % tok
            mark = self.cmd_mark()
            try:
                if tries:
                    for _ in range(3):
                        self.be.key("ctrl+c")
                        time.sleep(0.8)
                else:
                    self.be.key("ctrl+c")
                    time.sleep(0.3)
                self.be.key("ctrl+u")
                self.be.type(self.append_cmd(self.pane_shell(), tok, fname), 12, enter=True)
            except Exception:
                return None
            if self.until(lambda: tok in self.local_lines(fname), wait):
                # the shell's own rec.log (Git Bash pane) logs the command AFTER it ran: wait for that line, or it lands in the NEXT case's cmds_since(mark)
                # (r18co: paste_instant_enter 'first mismatch' was this probe's `echo t147... >> alive_...`)
                if self.prof.commands():
                    self.until(lambda: any(tok in c for _, c in self.prof.commands()[mark:]), 5)
                return tries
        return None

    def app_log_chars(self, t0):
        """the characters the APP logged as typed since wall time t0, in order (`input_order typed_characters text="x"`; needs
        WARP_INPUT_ORDER_LOG_TEXT=1, which the Windows harness sets), from the active log plus its rotated chunks. None when there is no such trace."""
        import glob
        src = self.log_path
        if not src:
            return None
        parts = sorted(glob.glob(glob.escape(src) + ".in_session.*"), key=lambda f: int(f.rsplit(".", 1)[1]) if f.rsplit(".", 1)[1].isdigit() else -1, reverse=True) + [src]
        out, seen = [], False
        for f in parts:
            try:
                txt = open(f, errors="replace").read()
            except OSError:
                continue
            for m in re.finditer(r'input_order typed_characters text="((?:[^"\\]|\\.)*)" t=(\d+(?:\.\d+)?)', txt):
                seen = True
                if float(m.group(2)) / 1000.0 >= t0 - 1:
                    out.append(m.group(1).replace('\\"', '"').replace("\\\\", "\\"))
        return "".join(out) if seen else None

    def app_enter_to_write_ms(self, t0):
        """median ms from the app's `enter_begin` hop to the `pty_write` that carries the line (>10 bytes) since wall time t0: the APP's own share
        of Enter->command-start, read from its input_order log (None without a trace). The rest is the shell/ConPTY side."""
        import glob
        src = self.log_path
        if not src:
            return None
        parts = sorted(glob.glob(glob.escape(src) + ".in_session.*"), key=lambda f: int(f.rsplit(".", 1)[1]) if f.rsplit(".", 1)[1].isdigit() else -1, reverse=True) + [src]
        ds, pending = [], None
        for f in parts:
            try:
                lines = open(f, errors="replace").read().splitlines()
            except OSError:
                continue
            for l in lines:
                m = re.search(r" t=(\d+(?:\.\d+)?)", l)
                if not m:
                    continue
                t = float(m.group(1))
                if t / 1000.0 < t0:
                    continue
                if "hop=enter_begin" in l:
                    pending = t
                elif pending is not None and "pty_write bytes=" in l and "handoff" not in l:
                    nb = re.search(r"pty_write bytes=(\d+)", l)
                    if nb and int(nb.group(1)) > 10:
                        ds.append(t - pending)
                        pending = None
        return round(statistics.median(ds), 2) if ds else None

    def app_saw_line(self, want, t0):
        """True: every character of `want` is in the app's own typed-character log, in order and contiguous (the key ARRIVED and was routed);
        False: the app's log has typed characters but not this line (the injector/OS never delivered it, or the log rotated it away); None: no trace."""
        got = self.app_log_chars(t0)
        if got is None:
            return None
        return want in got

    def ssh_diag(self):
        """what the ssh test host saw, for a failing ssh case: the tail of sshd.log (connect / auth / session lines) and fakesshd's stderr"""
        out = []
        for name in ("sshd.log", "fakesshd.stderr"):
            p = os.path.join(self.run, "ssh", name)
            try:
                tail = [l for l in open(p, errors="replace").read().splitlines() if l.strip()][-6:]
            except OSError:
                continue
            out.append("%s: %s" % (name, " | ".join(tail) if tail else "(empty)"))
        if getattr(self.prof, "ssh_hint", None):
            out.append("PROBE: " + self.prof.ssh_hint)
        return "; ".join(out) or "no ssh server log"

    def reset(self):
        self.be.key("ctrl+c")
        time.sleep(0.25)
        self.be.key("ctrl+u")
        time.sleep(0.15)

    def shot(self, tag):
        p = os.path.join(self.prof.shots, "%s_%s.png" % (self.label, tag))
        try:
            self.be.shot(p)
        except Exception:
            return None
        return p

    def cmds_since(self, mark):
        return [c for _, c in self.prof.commands()[mark:]]

    def cmd_mark(self):
        return len(self.prof.commands())

    def log_size(self):
        try:
            return os.path.getsize(self.log_path)
        except Exception:
            return 0

    def log_since(self, off):
        try:
            with open(self.log_path, "rb") as f:
                f.seek(off)
                return f.read().decode(errors="replace")
        except Exception:
            return ""

    def mode_now(self, tag):
        """'agent' | 'terminal' | None (no OCR engine, or the screen says neither), read from a screenshot. Agent view: its header
        'New Phosphor Agent conversation', the 'ESC for terminal' bar, or the 'Ask anything' placeholder. Terminal session: the
        'new /agent conversation' hint or 'New terminal session' (r18cx: the agent pass had silently become a terminal session)."""
        txt = self.ocr(tag)
        if txt is None:
            return None
        low = " ".join(txt.lower().split())
        agent = "new phosphor agent conversation" in low or "for terminal" in low or "ask anything" in low
        term = (("/agent" in low and "conversation" in low) or "new terminal session" in low
                or "run commands, or ask the agent" in low or "to continue conversation" in low)     # r18cz: the terminal prompt's other hints
        if agent and not term:
            return "agent"
        if term and not agent:
            return "terminal"
        return None

    def ocr(self, tag):
        mac_ocr = os.path.join(getattr(self.be, "TOOLS", "") or "", "ocr") if self.be.name == "mac" else ""
        if not shutil.which("tesseract") and not (mac_ocr and os.access(mac_ocr, os.X_OK)):
            return None
        p = self.shot(tag)
        if not p:
            return None
        if shutil.which("tesseract"):
            r = subprocess.run(["tesseract", p, "-"], capture_output=True, timeout=60)
        else:
            r = subprocess.run([mac_ocr, p], capture_output=True, timeout=60)      # Apple Vision (mac/tools/ocr.swift)
        return r.stdout.decode(errors="replace")


# ---------------------------------------------------------------- mode proof
@case("mode_proof", order=1, gui=True)
def c_mode_proof(ctx, r):
    """screenshot proof of the session mode before any key is typed (shots/<mode>_proof.png is the evidence), plus OCR when an engine exists.
    Agent mode = header 'New Phosphor Agent conversation' + 'Ask anything' placeholder (+ model chip with the CI model).
    The hint 'new /agent conversation' under a prompt means a TERMINAL session: it must not count as an agent marker."""
    # The proof is read from the PRE-KEY screenshot (shots/<mode>_proof_prekey.png): Escape in a fresh agent view LEAVES agent mode
    # (macOS CI run 3015802940 showed 'New terminal session' after the Escape in the agent pass), so the Escape is sent only after.
    if ctx.be.name == "mac":
        d = ctx.be.system_dialog(ocr=True)
        if d:
            raise ctx.be.needs_owner(d)
    txt = ctx.ocr("proof_prekey")      # taken before ANY key is sent
    if ctx.mode != "agent":
        # r18cx (macOS): this Escape is the agent view's own "esc: go back to terminal", so in the agent pass it left agent mode and every
        # "agent" case after it ran in a terminal session (owner photo). Only the terminal pass sends it.
        ctx.be.key("Escape")
        time.sleep(0.8)
    ctx.shot("proof")
    if txt is None:
        r.note("no OCR engine: screenshot only (shots/%s_proof_prekey.png), mode not machine-verified" % ctx.label)
        return
    low = " ".join(txt.lower().split())
    hint = "/agent" in low and "conversation" in low                       # terminal-mode hint line
    header = "new phosphor agent conversation" in low
    placeholder = "ask anything" in low
    is_agent = (header or placeholder) and not (hint and not header)
    r.metric("ocr_agent_markers", is_agent)
    r.metric("ocr_terminal_hint", hint)
    if ctx.mode == "agent":
        r.check(is_agent, "agent mode expected but the screen shows no agent markers ('New Phosphor Agent conversation' / 'Ask anything'); terminal hint present: %s" % hint)
        if ctx.ollama:
            chip = "gpt-oss" in low
            r.metric("ocr_model_chip_gpt_oss", chip)
            r.check(chip, "model chip 'gpt-oss:20b' not visible before any key was typed (the CI Ollama model is not the active one)")
    else:
        r.check(not is_agent, "terminal mode expected but the screen shows agent markers ('New Phosphor Agent conversation' / 'Ask anything')")


# ---------------------------------------------------------------- cadence
@case("cadence", order=2, gui=True)
def c_cadence(ctx, r):
    """the input tool really types at 10-25 ms/key: arrival times measured at the pty by ts_reader.py"""
    out = os.path.join(ctx.W, "cad.txt")
    ctx.rm("cad.txt")
    n = 60
    t_start = time.time()
    mark = ctx.cmd_mark()
    prompts0 = ctx.prompts()
    reader = os.path.join(os.path.dirname(os.path.abspath(__file__)), "ts_reader.py")
    if ctx.win:      # the private interpreter by full path (it is not on the VM's PATH); msvcrt.getwch() in ts_reader.py reads the keys the ConPTY delivers
        py, reader = sys.executable.replace("\\", "/"), reader.replace("\\", "/")
        cmd = ("& '%s' '%s' cad.txt %d" if ctx.pane == "powershell" else '"%s" "%s" cad.txt %d') % (py, reader, n)
    else:
        cmd = "python3 %s %s %d" % (reader, "cad.txt", n)
    ctx.be.type(cmd, 12, enter=True)
    time.sleep(2.0)   # python3 start + READY
    delay = 15
    ctx.be.type("".join(chr(97 + i % 26) for i in range(n)), delay)
    ctx.until(lambda: os.path.exists(out), 15)
    try:
        rows = [l.split() for l in open(out)]
    except FileNotFoundError:
        # r18cj: a character of the path was lost in the Git Bash pane ("C:/Users/inters/..." for Winters), bash answered "No such file or directory", and this case
        # blamed "python3 missing". The shell records the typed command once its prompt is back, so judge it here: a lost key is reported as one.
        ctx.until(lambda: bool(ctx.cmds_since(mark)), 5)
        recv = ctx.cmds_since(mark)
        if recv and recv[0] != cmd:
            r.check(False, "the cadence command arrived corrupted, so ts_reader never ran (NOT a missing python): " + line_evidence(ctx, cmd, recv[0], t_start, None))
        else:
            r.check(False, "ts_reader produced no output (the reader did not start or did not receive %d keys; the command as the shell recorded it: %r)" % (n, recv[:1]))
        ctx.reset()
        return
    if ctx.prof.local_prompts:      # let the shell finish ts_reader and print its prompt: its command line is logged at that prompt, and would otherwise land after the next case's mark
        ctx.until(lambda: ctx.prompts() > prompts0, 10)
    ts = [float(b) for _, b in rows]
    gaps = [(b - a) * 1000 for a, b in zip(ts, ts[1:])]
    r.metric("gap_ms", stats(gaps))
    if gaps:
        med = statistics.median(gaps)
        r.check(5 <= med <= 40, "median inter-key gap %.1f ms is outside 5..40 ms for a requested %d ms (input tool cadence wrong)" % (med, delay))
    r.check(len(rows) == n, "only %d of %d characters arrived" % (len(rows), n))
    ctx.reset()


# ---------------------------------------------------------------- typing
def classify_line(want, have):
    """how the shell's received line `have` differs from the typed line `want`: one character lost / one extra / characters moved / other"""
    if len(have) == len(want) - 1 and any(want[:i] + want[i + 1:] == have for i in range(len(want))):
        return "one character LOST"
    if len(have) == len(want) + 1 and any(have[:i] + have[i + 1:] == want for i in range(len(have))):
        return "one EXTRA character"
    if len(have) == len(want) and sorted(have) == sorted(want):
        return "characters REORDERED (a character moved: %s)" % next(("%r came before %r" % (have[i], want[i]) for i in range(len(want)) if have[i] != want[i]), "?")
    return "differs (%d typed, %d received characters)" % (len(want), len(have))


def line_evidence(ctx, want, have, t0, ctrl=None):
    """one sentence of proof for a corrupted line: what changed, whether the APP's own typed-character log shows the line arriving intact and in
    order (then it was lost/moved AFTER the app's pty write: app or the ConPTY it hosts, not the injector), and what the reference terminal did
    with the same injection in this session (a reference that kept every line exactly makes this Phosphor-side)."""
    parts = ["%s: typed %r, shell received %r" % (classify_line(want, have), want[:90], have[:90])]
    saw = ctx.app_saw_line(want, t0)
    if saw is True:
        parts.append("the app's log shows every character of the line arriving in order (typed_characters), so it was changed after the app's pty write")
    elif saw is False:
        parts.append("the app's log does NOT show the line's characters in order (never delivered to the app, or the log rotated it away)")
    else:
        parts.append("no key trace in the app log")
    if ctrl:
        parts.append("reference %s with the same injection: %d/%d lines missing, %s received exactly" % (
            ctrl.get("ref"), ctrl.get("bad", 0), ctrl.get("n", 0), "all" if not ctrl.get("exact_bad") else "NOT all (%d inexact)" % ctrl["exact_bad"]))
    return "; ".join(parts)


def effect_run(ctx, r, tag, n, gap, rollover=False, shell=None, attribute=None):
    """attribute: optional callable(gap) -> enter-control dict (run_enter_control). When given and the run fails with merged lines (a lost
    Enter), the failure is an APP bug only if the reference terminal kept every Enter in the same session (#1482)."""
    fname = "r_%s.txt" % tag
    shell = shell or ctx.pane_shell()
    ctx.rm(fname)
    t_start = time.time()
    mark = ctx.cmd_mark()
    k0 = len(ctx.be.calls) if ctx.be.calls is not None else None
    exp = []
    for i in range(n):
        tok = ctx.tok(10).lower()
        cmd = ctx.append_cmd(shell, tok, fname)
        exp.append(tok)
        if rollover:
            ctx.be.roll(cmd, overlap_ms=ctx.rng.choice([6, 10, 14]), enter=True)
        else:
            ctx.be.type(cmd, ctx.rng.randint(10, 25), enter=True)
        if gap:
            time.sleep(gap)
    ctx.until(lambda: len(ctx.lines(fname)) >= n, 20 + n * 0.5)
    time.sleep(0.5)
    got = ctx.lines(fname)
    ok = got == exp
    bad_i = next((i for i, (a, b) in enumerate(zip(exp + [None], got + [None])) if a != b), None)
    ev = ""
    if not ok and bad_i is not None and bad_i < n:
        # what the injector says about the Enter that should have ended the first bad line (its call is k0+bad_i: one injector call per line)
        v = ctx.be.inject_enter(k0 + bad_i) if k0 is not None else None
        ev = " [first bad line %d: %s]" % (bad_i, v[1] if v else "injector keeps no record on this backend")
    merged = False
    if ctx.prof.commands():
        want0 = [ctx.append_cmd(shell, t, fname) for t in exp]
        have0 = ctx.cmds_since(mark)
        merged = any(sum(1 for w in want0 if w in h) >= 2 for h in have0)
    elif not ok:
        merged = len(got) != n
    verdict = None
    ctrl = None
    if not ok and (merged or ctx.win) and attribute:      # Windows: a character moved/lost inside a line is attributed the same way as a lost Enter
        ctrl = attribute(gap)
        if ctrl is None:
            verdict = " (no reference terminal: lost Enter not attributable)"
        elif ctrl["bad"] > 0 or (ctrl.get("exact_bad") or 0) > 0:
            r.note("%s: injector/OS lost Enter (reference too): reference %s merged/lost %d/%d lines with the same burst%s; Phosphor file has %d/%d lines" % (
                tag, ctrl["ref"], ctrl["bad"], ctrl["n"], ev, len(got), n))
            r.metric("injector_lost_enter_" + tag, dict(reference_bad=ctrl["bad"], n=ctrl["n"], phosphor_lines=len(got)))
            ok = True
            verdict = "skip"
        else:
            verdict = " -- the reference kept every line exactly in the same session (%d/%d lines): Phosphor-side (#1482)" % (ctrl["n"] - ctrl["bad"], ctrl["n"])
    if verdict != "skip":
        r.check(ok, "%s: file has %d/%d lines, exact match %s%s%s%s" % (tag, len(got), n, ok,
                "" if ok else " first diff: exp=%r got=%r" % next(((a, b) for a, b in zip(exp + [None], got + [None]) if a != b), (None, None)), ev, verdict or ""))
    if ctx.prof.commands():
        want = [ctx.append_cmd(shell, t, fname) for t in exp]
        have = ctx.cmds_since(mark)
        if have and verdict == "skip":
            pass
        elif have:
            pair = next(((h, w) for h, w in zip(have, want) if h != w), None)
            ev2 = ""
            if pair and have != want:
                ev2 = " -- " + line_evidence(ctx, pair[1], pair[0], t_start, ctrl)
            r.check(have == want, "%s: shell received %d commands, not the typed ones (first mismatch: %r)%s" % (
                tag, len(have), pair[0] if pair else "count differs %d vs %d" % (len(have), len(want)), ev2))
        else:
            r.note("%s: rec.log empty (PROMPT_COMMAND hook not run), effect file checked only" % tag)
    return ok


@case("fast_typing", order=10)
def c_fast(ctx, r):
    for gap in ctx.opts.get("ft_gaps", (0.0, 0.25)):
        attr = (lambda g: (run_enter_control(ctx, r, ctx.count(30)) or {}).get(g) or None) if ctx.opts.get("ref", True) else None
        effect_run(ctx, r, "ft%d" % int(gap * 100), ctx.count(30), gap, attribute=attr)


@case("rollover", order=11)
def c_roll(ctx, r):
    effect_run(ctx, r, "roll", ctx.count(20), 0.1, rollover=True)


@case("expert_fuzz", order=12)
def c_fuzz(ctx, r):
    """typos fixed with BackSpace / Left+insert / Ctrl-W, seeded; exact command verified"""
    fname = "fz.txt"
    ctx.rm(fname)
    exp = []
    N = ctx.count(25)
    for i in range(N):
        w = "".join(ctx.rng.choice("abcdefghijklmnopqrstuvwxyz") for _ in range(ctx.rng.randint(4, 9)))
        tok = "fz%d%s" % (i, w)
        exp.append(tok)
        k = ctx.rng.randrange(1, len(tok) - 1)
        mode = ctx.rng.choice(["bs", "left", "ctrlw", "plain"])
        dl = ctx.rng.randint(10, 25)
        if mode == "bs":
            ctx.be.type("echo " + tok[:k] + "q", dl)
            ctx.be.key("BackSpace")
            ctx.be.type(tok[k:] + " >> " + fname, dl, enter=True)
        elif mode == "left":
            ctx.be.type("echo " + tok[:k] + tok[k + 1:], dl)
            ctx.be.key("Left", repeat=len(tok) - k - 1, delay_ms=12)
            ctx.be.type(tok[k], dl)
            ctx.be.key("End")
            ctx.be.type(" >> " + fname, dl, enter=True)
        elif mode == "ctrlw":
            ctx.be.type("echo " + tok + " >> " + fname + " xyz", dl)
            ctx.be.key("ctrl+w")
            ctx.be.key("Return")
        else:
            ctx.be.type("echo " + tok + " >> " + fname, dl, enter=True)
        time.sleep(ctx.rng.choice([0, 0, 0.05, 0.3]))
    ctx.until(lambda: len(ctx.lines(fname)) >= N, 30)
    time.sleep(0.5)
    got = ctx.lines(fname)
    r.check(got == exp, "fuzz: %d/%d lines, mismatches %s" % (len(got), N, [(a, b) for a, b in zip(exp, got) if a != b][:3]))


@case("paste_instant_enter", order=13)
def c_paste_enter(ctx, r):
    ctx.rm("pe.txt")
    exp, sent = [], []
    mark = ctx.cmd_mark()
    for i in range(ctx.count(6)):
        tok = ctx.tok(8)
        pad = "w" * ctx.rng.choice([10, 500, 5000])
        if ctx.pane == "powershell":     # no md5sum: the pasted line writes tok+pad itself, compared exactly
            ctx.be.clip_set("Add-Content pe.txt %s%s" % (tok, pad))
            exp.append(tok + pad)
        else:
            ctx.be.clip_set("echo %s%s | md5sum >> pe.txt" % (tok, pad))
            exp.append(hashlib.md5((tok + pad + "\n").encode()).hexdigest())
            sent.append("echo %s%s | md5sum >> pe.txt" % (tok, pad))
        ctx.be.paste(enter=True)
        time.sleep(0.8)
    ctx.until(lambda: len(ctx.lines("pe.txt")) >= len(exp), 15)
    got = ctx.lines("pe.txt")
    if ctx.pane != "powershell":
        # compare the DIGEST: Git for Windows' md5sum prints "<hash> *-" (binary marker), GNU coreutils "<hash>  -" (r18cj: 0/6 in both Git Bash panes while the
        # shell's own rec.log showed all six pasted lines arrived byte-exact, 10/500/5000-character pads)
        got = [l.split()[0] if l.split() else l for l in got]
        recv = [c for c in ctx.cmds_since(mark) if "alive_" not in c] if (ctx.win and ctx.prof.commands()) else None     # a wedge probe's line can be logged late (r18co)
        if recv is not None:
            r.check(recv == sent, "pasted lines reached the shell exactly: %d/%d byte-exact (first mismatch: %r)" % (
                sum(a == b for a, b in zip(sent, recv)), len(sent), next(((b[:60], len(b)) for a, b in zip(sent, recv) if a != b), "count %d vs %d" % (len(recv), len(sent)))))
    r.check(got == exp, "paste + instant Enter: %d/%d %s lines exact%s" % (sum(a == b for a, b in zip(exp, got)), len(exp), "exact" if ctx.pane == "powershell" else "md5 digest",
            "" if got == exp else " (first got %r, expected %r)" % (got[:1], exp[:1])))


@case("arrows_behind_paste", order=14)
def c_paste_arrows(ctx, r):
    ctx.rm("pa.txt")
    exp = []
    for i in range(ctx.count(5)):
        tok = ctx.tok(6)
        pad = "q" * ctx.rng.choice([100, 3000])
        ctx.be.clip_set(tok + pad)
        if ctx.pane == "powershell":
            exp.append(tok + pad + "xzy")
            ctx.be.type("Add-Content pa.txt ", 12)
        else:
            exp.append(hashlib.md5((tok + pad + "xzy\n").encode()).hexdigest())
            ctx.be.type("echo ", 12)
        ctx.be.paste()
        ctx.be.keys(["x", "y", "Left", "z", "End"], delay_ms=8)
        if ctx.pane == "powershell":
            ctx.be.key("Return")
        else:
            ctx.be.type(" | md5sum >> pa.txt", 8, enter=True)
        time.sleep(1.0)
    ctx.until(lambda: len(ctx.lines("pa.txt")) >= len(exp), 15)
    got = ctx.lines("pa.txt")
    if ctx.pane != "powershell":
        got = [l.split()[0] if l.split() else l for l in got]      # digest only: Git for Windows' md5sum prints "<hash> *-"
    r.check(got == exp, "arrow keys behind a paste: %d/%d exact" % (sum(a == b for a, b in zip(exp, got)), len(exp)))


@case("big_paste", order=15, gui=True)
def c_big(ctx, r):
    mb = float(os.environ.get("APPCI_BIG_MB", "1"))
    rr = random.Random(5)
    out, tot, size = [], 0, int(mb * 1_000_000)
    while tot < size:
        l = "".join(rr.choice("abcdefghijklmnopqrstuvwxyz0123456789 ") for _ in range(rr.randint(20, 120)))
        out.append(l)
        tot += len(l) + 1
    data = "\n".join(out)[:size]
    ctx.rm("big.out")
    ctx.be.type("cat > big.out", 12, enter=True)
    time.sleep(1.0)
    ctx.be.clip_set(data)
    ctx.be.paste()

    def stable():
        p = os.path.join(ctx.W, "big.out")
        a = os.path.getsize(p) if os.path.exists(p) else -1
        time.sleep(1.5)
        return a > 0 and a == (os.path.getsize(p) if os.path.exists(p) else -2)
    ctx.until(stable, 30 + 20 * mb, step=0.1)
    ctx.be.key("Return")
    time.sleep(0.5)
    ctx.be.key("ctrl+d")
    time.sleep(2)
    got = open(os.path.join(ctx.W, "big.out"), "rb").read() if os.path.exists(os.path.join(ctx.W, "big.out")) else b""
    r.metric("bytes_expected", len(data) + 1)
    r.metric("bytes_got", len(got))
    r.check(got == (data + "\n").encode(), "%.0f MB paste not byte-exact (expected %d bytes, got %d)" % (mb, len(data) + 1, len(got)))
    ctx.reset()


@case("tab_enter", order=16)
def c_tab(ctx, r):
    """Tab then Enter must run the COMPLETED line (gap 0 / 50 / 150 ms); no stray file from the uncompleted line"""
    target = "tc/zzcompl_alpha_unique.txt"
    ctx.rm(target)
    open(os.path.join(ctx.W, target), "w").close()
    exp = []
    for gap in (0, 0.05, 0.15):
        for i in range(ctx.count(4)):
            tok = ctx.tok(6)
            exp.append(tok)
            ctx.be.type(("Add-Content -Value %s -Path tc/zzcompl_alpha_u" if ctx.pane == "powershell" else "echo %s >> tc/zzcompl_alpha_u") % tok, 15)
            ctx.be.key("Tab")
            if gap:
                time.sleep(gap)
            ctx.be.key("Return")
            time.sleep(0.5)
    ctx.until(lambda: len(ctx.lines(target)) >= len(exp), 10)
    got = ctx.lines(target)
    r.check(got == exp, "Tab+Enter: completed file has %d/%d expected lines" % (len(got), len(exp)))
    stray = [f for f in os.listdir(os.path.join(ctx.W, "tc")) if f != "zzcompl_alpha_unique.txt"]
    r.check(not stray, "Tab+Enter ran the UNCOMPLETED line, stray files: %s" % stray)


def ctrlc_line(ctx, fname, i):
    """The line typed for one Ctrl-C sample. IDENTICAL in Phosphor and the reference terminal (same command, same timestamp source).
    macOS: zsh's $EPOCHREALTIME in the trap (no process start; the old ~/bin/now helper is a python3 start, ~30 ms of harness cost
    that inflated every absolute number). Linux: `date +%s.%N` (one fork of a tiny binary, the same in both terminals)."""
    if ctx.pane == "powershell":
        # Windows PowerShell: the finally block of the interrupted try stamps UTC ms (13 digits; the Windows PowerShell harness of #1487). Ctrl-C stops
        # the whole line, so there is no AFTER marker: the next sample's typed line proves the shell came back
        # .NET in the finally block, by absolute path: a cmdlet (Add-Content) run from the finally of a Ctrl-C-stopped pipeline took 3.5 s in Phosphor AND in
        # the conhost reference and left the pane wedged for good (r18cj), and .NET's cwd is not PowerShell's location, hence the absolute path
        # r18ct: each stamp carries its SAMPLE INDEX ("<i>:<ms>"). Pairing stamps to samples by order made a late stamp (one that landed after the 6 s wait) pair with the NEXT
        # sample's t0, which printed negative latencies (-2273 ms) and hid whether Ctrl-C ever interrupted the sleep.
        return ("try { Start-Sleep 30 } finally { [IO.File]::AppendAllText('%s', ('%d:' + [string]([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()) + [char]10)) }"
                % (os.path.join(ctx.W, fname).replace("\\", "/"), i))
    if ctx.be.name == "mac":
        return "zsh -c 'zmodload zsh/datetime; trap \"print -r -- \\$EPOCHREALTIME >> %s; exit\" INT; sleep 30'; echo AFTER%d >> %s" % (fname, i, fname)
    return "bash -c 'trap \"date +%%s.%%N >> %s; exit\" INT; sleep 30'; echo AFTER%d >> %s" % (fname, i, fname)


def ctrlc_samples(ctx, n, fname, budget_s=None):
    """n x (start a trap-instrumented sleep, press Ctrl-C, read the trap's timestamp) -> (latencies ms, all delivered)"""
    ctx.rm(fname)
    lat_ms, typed, misses = [], [], []
    mark = ctx.cmd_mark()
    t_start = time.time()
    pw = ctx.pane == "powershell"
    t_end = time.time() + budget_s if budget_s else None
    for i in range(n):
        if t_end and time.time() > t_end:
            break
        typed.append(ctrlc_line(ctx, fname, i))
        ctx.be.guard_ref()
        ctx.be.type(typed[-1], 10, enter=True)
        time.sleep(1.5 if pw else 1.2)
        t0 = time.time()
        ctx.be.key("ctrl+c")
        if pw:
            ctx.until(lambda: i in ps_stamps(ctx, fname), 6)
        else:
            ctx.until(lambda: sum(1 for l in ctx.lines(fname) if l.startswith("AFTER")) > i, 5)
        time.sleep(0.2)
        if pw:
            ts = ps_stamps(ctx, fname)
            if i in ts and ts[i] >= t0 - 0.05:
                lat_ms.append((ts[i] - t0) * 1000)
            elif i not in ts:
                # a miss: Ctrl-C did not stop the sleep within 6 s. Press it again, then WAIT for this sample's own stamp (the sleep ends by itself after 30 s) so the
                # next sample's line is not typed behind a still-running command: r18ct queued 33 x `Start-Sleep 30` that way and the pane stayed busy for ~16 minutes
                # (pane_wedged_after_ctrl_c_d).
                ctx.be.key("ctrl+c")
                time.sleep(0.8)
                ctx.be.key("ctrl+c")
                if not ctx.until(lambda: i in ps_stamps(ctx, fname), 36):
                    time.sleep(1)
                misses.append(i)
            else:
                misses.append(i)        # a stamp that PRECEDES this sample's Ctrl-C: the finally ran without our key (not counted as a delivery)
            continue
        ts = stamps(ctx, fname)
        if len(ts) > len(lat_ms):
            lat_ms.append((ts[len(lat_ms)] - t0) * 1000)
    after = [l for l in ctx.lines(fname) if l.startswith("AFTER")]
    if ctx.prof.commands():
        # a sample that never fired is either a Ctrl-C that did not arrive or a typed LINE that arrived corrupted (no trap, no stamp): tell them apart
        have = ctx.cmds_since(mark)
        pairs = [(w, h) for w, h in zip(typed, have) if w != h]
        ctx.cc_integrity[fname] = dict(n=len(typed), got=len(have), bad=len(pairs), examples=[line_evidence(ctx, w, h, t_start, None) for w, h in pairs[:2]])
    if pw and misses:
        ctx.cc_integrity.setdefault("misses", {})[fname] = misses
    return lat_ms, len(lat_ms) == n and (pw or len(after) == n)


def ps_stamps(ctx, fname):
    """PowerShell pane: {sample index: epoch seconds} from the `<i>:<13-digit UTC ms>` lines the finally block writes (see ctrlc_line)"""
    out = {}
    for l in ctx.lines(fname):
        m = re.fullmatch(r"(\d+):(\d{13})", l.strip().replace("\x00", ""))
        if m:
            out.setdefault(int(m.group(1)), int(m.group(2)) / 1000.0)
    return out


def stamps(ctx, fname):
    """epoch seconds written by the trap / finally block: `1699999999.123456789` (date, EPOCHREALTIME) or 13-digit UTC milliseconds (PowerShell)"""
    out = []
    for l in ctx.lines(fname):
        l = l.strip().replace("\x00", "")
        if re.fullmatch(r"\d+\.\d+", l):
            out.append(float(l))
        elif re.fullmatch(r"\d{13}", l):
            out.append(int(l) / 1000.0)
    return out


@case("ctrl_c_d", order=17)
def c_ctrl(ctx, r):
    ctx.rm("cd.txt", "cd2.txt")
    n, warm = ctx.count(30), lat.warmup()
    lat_ms, ok1 = ctrlc_samples(ctx, n + warm, "cc.txt")
    lat_ms = lat_ms[warm:]
    r.metric("ctrl_c_ms", stats(lat_ms))
    r.metric("harness_floor_ms", ctx.be.key_floor_ms)
    # (the delivered-count verdict is below, after the reference ran, so it can say whether the reference lost any sample line)
    # Same measurement in the reference terminal, same session (#1486). Both numbers include the input tool's spawn and the timestamp
    # command's cost, which cancel in the overhead; the owner budget (Ctrl-C <= 20 ms) is judged on that overhead.
    rs, ref = [], None
    short = ctx.opts.get("cc_short", False)
    if lat_ms:
        ctx.reset()
        ref = None if short else ref_terminal(ctx)
        if ref:
            try:
                time.sleep(2.5)
                rs = ctrlc_samples(ctx, n + warm, "cc_ref.txt", budget_s=max(120, (n + warm) * 5))[0][warm:]
                ctx.reset()
            finally:
                ref.close()
            time.sleep(1.0)
            r.metric("reference_ctrl_c_ms", stats(rs))
            r.metric("reference", ref.name)
        elif not short:
            r.note("no reference: absolute only (Ctrl-C): no reference terminal available%s" % (": " + ctx.ref_reason if getattr(ctx, "ref_reason", None) else ""))
        if short:
            r.note("fix tier, agent mode (winmap.FIX_TIER): Ctrl-C delivery and line integrity only; the overhead budget is judged in terminal mode")
        rec = None if short else lat.evaluate("%s/ctrl_c_d/ctrl_c" % ctx.label, lat_ms, rs, float(os.environ.get("APPCI_CTRLC_MAX_MS", "20")), "overhead", plat(),
                           remeasure=None if short else (lambda: ctrlc_samples(ctx, n + warm, "cc2.txt")[0][warm:]), ref_name=ref.name if ref else None, floor=ctx.be.key_floor_ms,
                           notes=["same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; "
                                  "absolute Phosphor median %s ms includes the input-tool spawn (harness floor %s ms)" % (stats(lat_ms).get("p50"), ctx.be.key_floor_ms)])
        if rec:
            r.metrics.setdefault("lat", {})["ctrl_c"] = rec
            lat_checks(r, rec, "Ctrl-C")
    why = ""
    ig, igr = ctx.cc_integrity.get("cc.txt"), ctx.cc_integrity.get("cc_ref.txt")
    if ig:
        why = " -- Phosphor: %d/%d sample lines reached the shell NOT exactly as typed%s" % (ig["bad"], ig["n"], ("; e.g. " + ig["examples"][0]) if ig["examples"] else "")
        if igr:
            why += "; reference: %d/%d" % (igr["bad"], igr["n"])
            if ig["bad"] and not igr["bad"]:
                why += " (the reference kept every line: the corruption is Phosphor-side, the same character loss/move as fast_typing)"
        r.metric("ctrl_c_line_integrity", dict(phosphor=ig, reference=igr))
    mp = ctx.cc_integrity.get("misses", {})
    if mp.get("cc.txt"):
        why += " -- PowerShell: Ctrl-C did not stop the sleep (no stamp within 6 s, or one that predates the key) on %d of %d samples (indexes %s)" % (len(mp["cc.txt"]), n + warm, mp["cc.txt"][:12])
    if mp.get("cc_ref.txt"):
        why += "; reference: %d misses" % len(mp["cc_ref.txt"])
    r.check(ok1, "Ctrl-C / command-after-Ctrl-C delivered fewer than %d times%s" % (n + warm, why))
    if ctx.win:
        r.note("Ctrl-D half not run on Windows: Ctrl-D is not end-of-input in PowerShell, and under ConPTY an msys cat/bash has not been validated to see it as EOF (docs/testing.md 2.8); the Ctrl-C half above ran")
        ctx.reset()
        return
    # Ctrl-D: ends `cat` input, and exits a nested bash back to the outer shell.
    # SAFETY (#1458): a Ctrl-D that reaches the pane's OWN top-level shell exits it and closes the last window, which quits the app.
    # So a Ctrl-D is sent only after the case has PROVEN the thing it is meant to end is running (cat: its output file exists, i.e. the
    # shell executed the line; bash: SHLVL inside it is one more than outside). If a setup line was lost, fail without sending the Ctrl-D.
    ctx.be.type("cat > cd.txt", 12, enter=True)
    cat_up = ctx.until(lambda: os.path.exists(os.path.join(ctx.W, "cd.txt")), 5)
    r.check(bool(cat_up), "`cat > cd.txt` never started (its Enter was lost?): Ctrl-D NOT sent (it would hit the top-level shell)")
    if cat_up:
        ctx.be.type("hello", 12, enter=True)
        time.sleep(0.5)
        ctx.be.key("ctrl+d")
        time.sleep(1.5)
        ctx.be.type("echo done1 >> cd.txt", 12, enter=True)
        time.sleep(1)
        r.check(ctx.lines("cd.txt") == ["hello", "done1"], "Ctrl-D in cat: file %r" % ctx.lines("cd.txt"))
    ctx.be.type("echo L$SHLVL >> cd2.txt", 12, enter=True)
    ctx.until(lambda: len(ctx.lines("cd2.txt")) >= 1, 5)
    ctx.be.type("bash", 12, enter=True)
    time.sleep(2)
    ctx.be.type("echo L$SHLVL >> cd2.txt", 12, enter=True)
    ctx.until(lambda: len(ctx.lines("cd2.txt")) >= 2, 5)
    l = ctx.lines("cd2.txt")
    nested = len(l) >= 2 and l[0].startswith("L") and l[1].startswith("L") and l[0][1:].isdigit() and l[1][1:].isdigit() \
        and int(l[1][1:]) == int(l[0][1:]) + 1
    r.check(nested, "nested bash not entered (SHLVL lines %r): Ctrl-D NOT sent (it would exit the pane's own shell and quit the app)" % l)
    if nested:
        ctx.be.key("ctrl+d")
        time.sleep(1.5)
        ctx.be.type("echo L$SHLVL >> cd2.txt", 12, enter=True)
        ctx.until(lambda: len(ctx.lines("cd2.txt")) >= 3, 5)
        l = ctx.lines("cd2.txt")
        ok = len(l) == 3 and l[0] == l[2]
        r.check(ok, "Ctrl-D leaves the nested shell: SHLVL lines %r" % l)
    ctx.reset()


@case("read_x", order=18)
def c_read(ctx, r):
    ctx.rm("rd.txt")
    tok = ctx.tok()
    ctx.be.type("read x", 10, enter=True)
    time.sleep(0.8)
    ctx.be.type("%s with  spaces ; $HOME 'q'" % tok, 12, enter=True)
    time.sleep(0.8)
    ctx.be.type("printf '%s\\n' \"$x\" >> rd.txt", 12, enter=True)
    ctx.until(lambda: ctx.lines("rd.txt"), 8)
    r.check(ctx.lines("rd.txt") == ["%s with  spaces ; $HOME 'q'" % tok], "read x received %r" % ctx.lines("rd.txt"))


@case("subshells", order=20)
def c_sub(ctx, r):
    ran = 0
    for sh, gap in (("bash", 1.5), ("bash", 0.25), ("zsh", 2.0), ("fish", 2.0), ("nu", 2.5)):
        if not shutil.which(sh):
            r.note("%s not installed: skipped" % sh)
            continue
        ran += 1
        f = "sub_%s_%d.txt" % (sh, int(gap * 100))
        ctx.rm(f)
        tok = ctx.tok(6)
        ctx.be.type(sh, 12, enter=True)
        time.sleep(gap)
        ctx.be.type(ctx.append_cmd(sh, tok, f), 12, enter=True)
        time.sleep(0.8)
        ctx.be.type("sh -c 'ps -o comm= -p $PPID' >> " + f, 12, enter=True)
        ctx.until(lambda: len(ctx.lines(f)) >= 2, 6)
        ctx.be.type("exit", 12, enter=True)
        time.sleep(2.0)
        ctx.be.type(ctx.append_cmd("bash", "back_" + tok, f), 12, enter=True)
        ctx.until(lambda: len(ctx.lines(f)) >= 3, 6)
        l = ctx.lines(f)
        r.check(len(l) == 3 and l[0] == tok and l[2] == "back_" + tok and l[1].startswith(sh[:3]),
                "%s subshell (setup gap %.2fs): lines %r" % (sh, gap, l))
        ctx.reset()
    if not ran:
        r.skip("no subshell binaries installed")


@case("alt_screen", order=21)
def c_alt(ctx, r):
    ctx.rm("al.txt")
    if shutil.which("less"):
        ctx.be.type("seq 1 500 | less", 12, enter=True)
        time.sleep(1.2)
        ctx.be.type(" ", 20)
        ctx.be.type("q", 20)
        time.sleep(0.8)
    if shutil.which("vim"):
        ctx.be.type("vim -u NONE al_edit.txt", 12, enter=True)
        time.sleep(1.5)
        ctx.be.type("ihello", 15)
        ctx.be.key("Escape")
        ctx.be.type(":wq", 15, enter=True)
        time.sleep(1.0)
    ctx.be.type("echo AFTER_ALT >> al.txt", 12, enter=True)
    ctx.until(lambda: ctx.lines("al.txt"), 6)
    r.check(ctx.lines("al.txt") == ["AFTER_ALT"], "shell usable after less/vim: %r" % ctx.lines("al.txt"))
    if shutil.which("vim"):
        r.check(ctx.lines("al_edit.txt") == ["hello"], "vim received typed text: %r" % ctx.lines("al_edit.txt"))


# ---------------------------------------------------------------- ssh (local server)
@case("ssh_connect_typing", order=30)
def c_ssh(ctx, r):
    if not ctx.ssh:
        r.skip(ctx.ssh_why or "no local ssh server (needs /usr/sbin/sshd + ssh-keygen, or python3-paramiko for fakesshd.py)")
    trials = int(os.environ.get("APPCI_SSH_TRIALS", "3"))
    ctx.rm("rl.txt")
    for f in ("rl.txt",):
        try:
            os.remove(os.path.join(ctx.prof.rhome, f))
        except FileNotFoundError:
            pass
    exp, miss, base, stop = [], 0, ctx.shell_count(), False
    for gap in (0.0, 0.1, 0.2, 0.3):
        for i in range(trials):
            tok = ctx.tok(6)
            exp.append(tok)
            ctx.be.type("ssh fake", 12, enter=True)
            time.sleep(gap)
            ctx.be.type("echo %s >> ~/rl.txt" % tok, 15, enter=True)
            if not ctx.until(lambda: tok in ctx.lines("rl.txt", remote=True), 20):
                miss += 1
            if not ctx.leave_remote(r, "ssh_connect_typing (gap %.1f s)" % gap, "rl.txt", tok, base):
                stop = True
                break
        if stop:
            break
    got = ctx.lines("rl.txt", remote=True)
    r.metric("trials", len(exp))
    r.check(got == exp, "remote shell received %d/%d typed-ahead commands exactly (missing %d)" % (sum(1 for t in exp if t in got), len(exp), miss))
    r.check(not ctx.local_lines("rl.txt"), "typed-ahead command ran LOCALLY: %r" % ctx.local_lines("rl.txt"))
    rem = [c for _, c in ctx.prof.commands(remote=True)]
    if rem:
        r.check(all("echo %s >> ~/rl.txt" % t in rem for t in exp), "remote rec.log lacks the exact typed command for some trials")
    ctx.reset()


@case("ssh_enter_then_immediate_command", order=30)
def c_ssh_enter_immediate(ctx, r):
    """#1415: `ssh <host>` Enter immediately followed by `echo <tok>` Enter, no pause at all. The
    second line is typed while the first Enter still waits at its gate; released into the new
    connect it must be held for the remote shell (it used to be dropped with a log line)."""
    if not ctx.ssh:
        r.skip(ctx.ssh_why or "no local ssh server (needs /usr/sbin/sshd + ssh-keygen, or python3-paramiko for fakesshd.py)")
    trials = int(os.environ.get("APPCI_SSH_TRIALS", "3"))
    try:
        os.remove(os.path.join(ctx.prof.rhome, "rh.txt"))
    except FileNotFoundError:
        pass
    ctx.rm("rh.txt")
    exp, base = [], ctx.shell_count()
    for _ in range(trials):
        tok = ctx.tok(6)
        exp.append(tok)
        ctx.be.type("ssh fake", 12, enter=True)
        ctx.be.type("echo %s >> ~/rh.txt" % tok, 12, enter=True)
        ctx.until(lambda: tok in ctx.lines("rh.txt", remote=True), 20)
        if not ctx.leave_remote(r, "ssh_enter_then_immediate_command", "rh.txt", tok, base):
            break
    got = ctx.lines("rh.txt", remote=True)
    r.check(got == exp, "remote shell received %d/%d immediately-typed commands exactly: %r" % (sum(1 for t in exp if t in got), len(exp), got))
    r.check(not ctx.local_lines("rh.txt"), "immediately-typed command ran LOCALLY: %r" % ctx.local_lines("rh.txt"))
    ctx.reset()


@case("ssh_nested_typeahead", order=30)
def c_ssh_nested(ctx, r):
    """#1421: `ssh fake` Enter, then `ssh fake` Enter and `echo <tok>` Enter typed during that first
    connect. At the hand-over the nested ssh is submitted; the echo behind it is held for the
    SECOND connect, so it runs exactly once, remotely, and never locally."""
    if not ctx.ssh:
        r.skip(ctx.ssh_why or "no local ssh server (needs /usr/sbin/sshd + ssh-keygen, or python3-paramiko for fakesshd.py)")
    try:
        os.remove(os.path.join(ctx.prof.rhome, "rn.txt"))
    except FileNotFoundError:
        pass
    ctx.rm("rn.txt")
    base = ctx.shell_count()
    tok = ctx.tok(6)
    ctx.be.type("ssh fake", 12, enter=True)
    ctx.be.type("ssh fake", 12, enter=True)
    ctx.be.type("echo %s >> ~/rn.txt" % tok, 12, enter=True)
    ctx.until(lambda: tok in ctx.lines("rn.txt", remote=True), 30)
    time.sleep(1.0)
    got = ctx.lines("rn.txt", remote=True)
    r.check(got == [tok], "the line typed behind a nested ssh ran exactly once remotely: %r" % got)
    r.check(not ctx.local_lines("rn.txt"), "the line typed behind a nested ssh ran LOCALLY: %r" % ctx.local_lines("rn.txt"))
    # leave both ssh levels, each `exit` only while the remote proof holds (a missing proof means no `exit`, so the local shell survives)
    if ctx.leave_remote(r, "ssh_nested_typeahead (inner)", "rn.txt", tok, base, level=True):
        ctx.leave_remote(r, "ssh_nested_typeahead (outer)", "rn.txt", tok, base)
    ctx.reset()


@case("ssh_failing_not_local", order=31, gui=True)
def c_ssh_fail(ctx, r):
    ctx.rm("sf.txt")
    ctx.be.type("ssh -o ConnectTimeout=3 -o BatchMode=yes no-such-host.invalid", 12, enter=True)
    time.sleep(0.15)
    ctx.be.type(ctx.append_cmd(ctx.pane_shell(), "MUSTNOT", "sf.txt"), 15, enter=True)
    time.sleep(8)
    r.check(not ctx.lines("sf.txt"), "line typed behind a FAILING ssh ran locally: %r" % ctx.lines("sf.txt"))
    ctx.reset()


# ---------------------------------------------------------------- perf
@case("idle_cpu", order=40, gui=True)
def c_idle(ctx, r):
    ctx.reset()
    time.sleep(3)
    t0, rss0 = ctx.app.cpu_ticks()
    w0 = time.time()
    time.sleep(20)
    t1, rss1 = ctx.app.cpu_ticks()
    clk = ctx.app.clk_tck() if hasattr(ctx.app, "clk_tck") else os.sysconf("SC_CLK_TCK")      # Windows: FILETIME 100 ns units
    pct_ = (t1 - t0) / clk / (time.time() - w0) * 100
    r.metric("idle_cpu_pct_of_one_core", pct_)
    r.metric("rss_mb", rss1 / 1024)
    limit = float(os.environ.get("APPCI_IDLE_CPU_MAX", "15"))
    r.check(pct_ <= limit, "idle CPU %.1f%% of one core exceeds CI limit %.0f%% (compositor/GPU path differs from a desktop: compare trends)" % (pct_, limit))


class RefTerm:
    """A reference terminal running the same scratch rc, for same-session comparison. name says how it was reached (XWayland etc.)."""
    def __init__(self, name, proc=None, close=None):
        self.name, self.proc, self._close = name, proc, close

    def close(self):
        if self._close:
            self._close()
        elif self.proc:
            try:
                os.killpg(self.proc.pid, 15)
            except Exception:
                pass


def ref_note(ctx, r):
    """#1485: when Terminal.app was already running, its reference windows are left open (never quit someone's Terminal)"""
    if ctx.win:
        return
    import refterm_mac
    n = refterm_mac.left_open_note()
    if n:
        r.note(n)


def ref_terminal_windows(ctx):
    """Windows: the SAME shell as the pane in a plain console window (conhost: `powershell.exe -NoLogo`, or Git Bash `--rcfile` for the Git Bash
    pane), driven through the same SendInput path under a guard that accepts only THAT window (its PID or its hwnd), never the job's own console.
    APPCI_WIN_REF=wt uses Windows Terminal (wt.exe, when present) instead of conhost. None (with ctx.ref_reason) when the window cannot be identified."""
    import winapi
    ctx.ref_reason = None
    want_wt = os.environ.get("APPCI_WIN_REF", "conhost").lower() == "wt"
    home = ctx.prof.home
    env = dict(ctx.app.env()) if hasattr(ctx.app, "env") else dict(os.environ)
    env["HOME"] = home
    if ctx.pane == "gitbash":
        sh, shname = [winapi.git_bash(), "--rcfile", home + "/.bashrc", "-i"], "bash"
    else:
        sh, shname = ["powershell.exe", "-NoLogo"], "powershell.exe"
    if want_wt:
        wt_exe = shutil.which("wt.exe") or shutil.which("wt")
        if not wt_exe:
            ctx.ref_reason = "APPCI_WIN_REF=wt but wt.exe (Windows Terminal) is not installed"
            return None
        argv, name, flags = [wt_exe, "-d", ctx.W, "new-tab"] + sh, "Windows Terminal (%s)" % shname, 0
    else:
        argv, name, flags = sh, "conhost (%s)" % shname, 0x00000010           # CREATE_NEW_CONSOLE
    before = {w[0] for w in winapi.top_windows(None)}
    proc = subprocess.Popen(argv, cwd=ctx.W, env=env, creationflags=flags, stdin=subprocess.DEVNULL if want_wt else None)
    ctx.ref_proc = proc
    hwnd, wpid = None, None
    for _ in range(60):
        time.sleep(0.4)
        ws = [w for w in winapi.top_windows(None) if w[0] not in before and w[2][2] > 100 and w[2][3] > 50]
        mine = [w for w in ws if w[1] == proc.pid] or [w for w in ws if winapi.proc_name(w[1]) in ("conhost.exe", "openconsole.exe", "windowsterminal.exe", "powershell.exe", "bash.exe")]
        if mine:
            hwnd, wpid = mine[-1][0], mine[-1][1]
            break
    def kill():
        subprocess.run(["taskkill", "/PID", str(proc.pid), "/T", "/F"], capture_output=True, timeout=30)
    if not hwnd:
        ctx.ref_reason = "the reference console window (%s) never appeared" % name
        kill()
        return None
    time.sleep(4)          # the shell prints its banner / loads its rc
    winapi.focus_window(hwnd)
    time.sleep(0.5)
    ctx.be.guard_override = dict(pids={proc.pid, wpid}, hwnds={hwnd})

    def close():
        try:
            ctx.be.type("exit", 12, enter=True)
        except Exception:
            pass
        time.sleep(1.0)
        ctx.be.guard_override = None
        if proc.poll() is None:
            kill()
        ctx.be.refocus()
    return RefTerm(name, close=close)


def ref_terminal(ctx):
    """Linux: foot (Wayland) when installed, else xterm (via XWayland under the Wayland session, native under X11).
    macOS: Terminal.app, driven by the same cliclick/osascript path as Phosphor. None when there is no reference."""
    home = ctx.prof.home
    if ctx.win:
        return ref_terminal_windows(ctx)
    if ctx.be.name == "mac":
        # NO Apple Events: `tell application "Terminal" ... do script` needs an Automation grant (a TCC prompt nobody answers in CI; it hung
        # 30 s per sample in run 3015802940). A .command file opened with LaunchServices needs none (and a .terminal document would import
        # a profile into com.apple.Terminal's prefs: not allowed). Driven by htype2 under the frontmost guard. Windows are cleaned up by
        # quitting a Terminal.app this run launched, by exact PID (#1485, refterm_mac).
        import refterm_mac
        ctx.ref_reason = None
        refterm_mac.snapshot(ctx.run)   # normally already done at harness start; idempotent
        cmdf = os.path.join(ctx.prof.home, "ref_terminal.command")
        with open(cmdf, "w") as f:
            # the SAME shell as Phosphor's pane: the login shell (r18cz: the pane ran /bin/bash 3.2 while this reference ran zsh, so the
            # Enter overhead compared two different shells). Same HOME, so the same harness rc (~/.bashrc / ~/.zshrc) and hooks.
            # The app inherits this process's environment (app.py env()), and Phosphor starts $SHELL (on the Mac runner: /bin/bash, while the
            # account's login shell is zsh).
            import pwd
            login = os.environ.get("SHELL") or pwd.getpwuid(os.getuid()).pw_shell or "/bin/zsh"
            if os.path.basename(login) == "bash":
                f.write("#!/bin/zsh\ncd '%s'\nexec env HOME='%s' PA_CTL='%s' '%s' -i\n" % (ctx.W, home, ctx.prof.ctl, login))
            else:
                f.write("#!/bin/zsh\ncd '%s'\nexec env HOME='%s' ZDOTDIR='%s' PA_CTL='%s' zsh -i\n" % (ctx.W, home, home, ctx.prof.ctl))
            ctx.ref_shell = login
        os.chmod(cmdf, 0o755)
        subprocess.run(["xattr", "-c", cmdf], capture_output=True)
        rr = subprocess.run(["open", "-a", "Terminal", cmdf], capture_output=True, timeout=20)
        if rr.returncode != 0:
            ctx.ref_reason = "open -a Terminal failed: %s" % rr.stderr.decode().strip()[:120]
            return None
        refterm_mac.note_launched(ctx.run)
        for _ in range(16):             # <= 8 s for Terminal to become frontmost
            if ctx.be._front()[0].lower() == "terminal":
                break
            time.sleep(0.5)
        else:
            ctx.ref_reason = "Terminal.app did not become frontmost within 8 s"
            return None
        d = ctx.be.system_dialog()
        if d:
            raise ctx.be.needs_owner(d)
        time.sleep(1.5)
        ctx.be.guard_override = "Terminal"

        def close():
            # never Cmd-W/Q: `exit` ends the shell; the window itself goes when a Terminal.app this run launched is quit by exact PID at
            # the end of the run (refterm_mac), which also removes a shell whose exit was lost
            try:
                ctx.be.type("exit", 12, enter=True)
            except Exception:
                pass
            time.sleep(0.8)
            ctx.be.guard_override = None
            ctx.be.refocus()
        return RefTerm("Terminal.app (.command via open)", close=close)
    disp, wl = os.environ.get("DISPLAY"), os.environ.get("WAYLAND_DISPLAY")
    env = dict(os.environ, HOME=home)
    # its OWN rc and ctl dir (#1500): the reference logs to refctl/rec.log, never the app pane's ctl/rec.log, so a burst that reached the wrong
    # window can never be read back as reference data
    rc = ctx.prof.ref_rc
    if wl and shutil.which("foot") and ctx.be.name == "wayland":
        argv, name, names = ["foot", "-e", "bash", "--rcfile", rc, "-i"], "foot", {"foot"}
    elif shutil.which("xterm") and disp:
        argv = ["xterm", "-geometry", "120x30+0+0", "-e", "bash", "--rcfile", rc, "-i"]
        name = "xterm (via XWayland)" if wl and ctx.be.name == "wayland" else "xterm"
        names = {"xterm"}
    else:
        return None
    proc = subprocess.Popen(argv, env=env, start_new_session=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    ctx.prof.ref_active = True
    ctx.be.guard_override = dict(pids={proc.pid}, names=names)

    def close():
        ctx.be.guard_override = None
        ctx.prof.ref_active = False
        try:
            os.killpg(proc.pid, 15)
        except Exception:
            pass
    try:
        ctx.be.guard()          # refuse up front when the reference never took the focus
    except RuntimeError as e:
        close()
        ctx.ref_reason = str(e)
        return None
    return RefTerm(name, proc=proc, close=close)


@case("key_loss_control", modes=("terminal",), order=3, gui=True)
def c_key_loss(ctx, r):
    """#1414: does the FIRST character of a wtype-injected line reach the client? The same injection sequences go into Phosphor AND a
    reference terminal in the same session: per-line calls (the harness default), per-line with the harness priming off, and ONE
    call for all lines. A Phosphor variant fails only when it loses first characters (or lines) the reference keeps with the
    SAME injection. The loss rates land in results.json (metrics.key_loss) and summary.md. Only the Wayland backend injects with a
    per-call keymap; other backends run it as a plain typing check."""
    n = ctx.count(12)
    is_wl = ctx.be.name == "wayland"
    prime_default = getattr(ctx.be, "prime_ms", 0)

    def variant(target, inject):
        if inject == "perline-unprimed":
            ctx.be.prime_ms = 0
        elif is_wl:
            ctx.be.prime_ms = prime_default
        fname = "kl_%s_%s.txt" % (target[:3], inject.replace("-", ""))
        ctx.rm(fname)
        mark = ctx.cmd_mark()
        want = []
        for _ in range(n):
            want.append("echo %s >> %s" % (ctx.tok(10).lower(), fname))
        try:
            ctx.be.guard_ref()
            if inject.startswith("oneshot"):
                ctx.be.type_lines(want, 12, 350)
            else:
                for l in want:
                    ctx.be.guard_ref()
                    ctx.be.type(l, 12, enter=True)
                    time.sleep(0.35)
        finally:
            if is_wl:
                ctx.be.prime_ms = prime_default
        ctx.until(lambda: len(ctx.cmds_since(mark)) >= n, 15 + n * 0.5)
        time.sleep(0.5)
        have = ctx.cmds_since(mark)
        lost = bad = 0
        for w, h in zip(want, have):
            if h == w:
                continue
            if h.endswith(w[1:]) and not h.endswith(w):
                lost += 1
            else:
                bad += 1
        missing = max(0, n - len(have))
        v = dict(target=target, inject=inject, n=n, lost_first=lost, other_bad=bad, missing=missing)
        if lost:
            v["example"] = next((w, h) for w, h in zip(want, have) if h != w and h.endswith(w[1:]))
        return v

    injections = ["perline-unprimed", "perline", "oneshot"] if is_wl else ["perline"]
    out = []
    for inj in injections:
        out.append(variant("phosphor", inj))
        ctx.reset()
    ref = ref_terminal(ctx)
    if ref:
        ref_note(ctx, r)
    if ref:
        try:
            time.sleep(2.5)
            for inj in injections:
                out.append(variant("reference", inj))
                ctx.reset()
        finally:
            ref.close()
        time.sleep(1.0)
    else:
        r.note("no reference terminal available: loss is reported but no variant can be attributed to Phosphor or the injector")
    r.metric("key_loss", dict(variants=out, reference=ref.name if ref else None))
    byk = {(v["target"], v["inject"]): v for v in out}
    for v in out:
        r.note("%s / %s: first char lost on %d/%d lines, other bad %d, missing %d" % (v["target"], v["inject"], v["lost_first"], v["n"], v["other_bad"], v["missing"]))
    for inj in injections:
        p, q = byk.get(("phosphor", inj)), byk.get(("reference", inj))
        bad_p = p["lost_first"] + p["other_bad"] + p["missing"]
        if q is None:
            continue
        bad_q = q["lost_first"] + q["other_bad"] + q["missing"]
        r.check(q["missing"] < q["n"], "the reference terminal's own log recorded NONE of the %d lines typed at it (%s): the keys did not reach it, so it is no control (#1500)" % (q["n"], inj))
        r.check(bad_p <= bad_q, "Phosphor lost keys the reference keeps with the SAME injection (%s): Phosphor %d/%d bad lines (first char lost %d), %s %d/%d -- a Phosphor-side drop (#1414)"
                % (inj, bad_p, n, p["lost_first"], ref.name, bad_q, n))
        if bad_q > bad_p:
            r.note("%s: the reference lost more than Phosphor (%d vs %d): injector-side loss" % (inj, bad_q, bad_p))
    # the harness's own default (primed, per-line) must deliver every line, whatever the reference does: otherwise every other case is unreliable
    d = byk.get(("phosphor", "perline"))
    if d:
        r.check(d["lost_first"] + d["other_bad"] + d["missing"] == 0, "harness default injection (per-line%s) still loses lines in Phosphor: %s"
                % (", primed" if is_wl and prime_default else "", {k: d[k] for k in ("lost_first", "other_bad", "missing")}))


def run_enter_control(ctx, r, n):
    """#1482: the fast_typing burst (same cadence 10-25 ms/key, instant Enter, same gaps) typed into the REFERENCE terminal's shell in this
    session. Returns {gap: dict(ref, n, bad, posted)} (bad = lines of the burst missing from the file, i.e. merged or lost; posted =
    ENTERs the injector reports posting, None when it keeps no record), or None when there is no reference terminal. Cached on ctx."""
    if ctx.enter_control is not None:
        return ctx.enter_control or None
    ref = ref_terminal(ctx)
    if ref:
        ref_note(ctx, r)
    if not ref:
        ctx.enter_control = {}
        r.note("enter control: no reference terminal (%s): a lost Enter in Phosphor cannot be attributed" % (getattr(ctx, "ref_reason", None) or "unavailable"))
        return None
    out = {}
    try:
        time.sleep(2.5)
        for gap in (0.0, 0.25):
            fname = "el_ref%d.txt" % int(gap * 100)
            ctx.rm(fname)
            k0 = len(ctx.be.calls) if ctx.be.calls is not None else None
            mark = ctx.cmd_mark()
            exp = []
            for _ in range(n):
                tok = ctx.tok(10).lower()
                exp.append(tok)
                ctx.be.guard_ref()
                ctx.be.type(ctx.append_cmd(ctx.pane_shell(), tok, fname), ctx.rng.randint(10, 25), enter=True)
                if gap:
                    time.sleep(gap)
            ctx.until(lambda: len(ctx.lines(fname)) >= n, 20 + n * 0.5)
            time.sleep(0.5)
            got = set(ctx.lines(fname))
            bad = sum(1 for t in exp if t not in got)
            # the same burst, judged on the EXACT command line the reference shell recorded (its rc writes the same rec.log): a character moved or
            # lost inside a line does not always remove the line from the file, so the file check alone under-counts what the reference kept
            exact_bad = None
            if ctx.prof.commands():
                want = [ctx.append_cmd(ctx.pane_shell(), t, fname) for t in exp]
                have = ctx.cmds_since(mark)
                exact_bad = sum(1 for w, h in zip(want, have) if w != h) + abs(len(want) - len(have))
            posted = None
            if k0 is not None:
                posted = sum(1 for c in ctx.be.calls[k0:k0 + n] if c["enters_posted"])
            out[gap] = dict(ref=ref.name, n=n, bad=bad, posted=posted, lines=len(ctx.lines(fname)), exact_bad=exact_bad)
            ctx.reset()
    finally:
        ref.close()
    time.sleep(1.0)
    ctx.enter_control = out
    return out


@case("enter_loss_control", modes=("terminal",), order=4, gui=True)
def c_enter_loss(ctx, r):
    """#1482: does an instant Enter after a fast-typed line survive in the REFERENCE terminal? Reports the control metric (metrics.enter_control)
    for fast_typing's verdict: its lost-Enter failure is an app bug only if this control kept every Enter in the same session. A separate
    case (not an extension of key_loss_control) because it needs the Phosphor-independent 30-line burst at fast_typing's exact cadence, and
    because key_loss_control's Wayland variants measure first-character loss, a different injector fault. Fails only when the injector itself
    reports it did not post an Enter (harness bug); reference loss is reported, not failed (it is the injector/OS, not the app)."""
    n = ctx.count(30)
    res = run_enter_control(ctx, r, n)
    if not res:
        r.note("no reference terminal available: Enter loss is not controlled")
        return
    r.metric("enter_control", dict(reference=next(iter(res.values()))["ref"], runs={str(g): v for g, v in res.items()}))
    for g, v in res.items():
        r.note("reference %s gap %.2fs: %d/%d lines missing (merged/lost Enter), %s lines not received exactly, injector posted %s Enters" % (
            v["ref"], g, v["bad"], v["n"], v.get("exact_bad"), v["posted"]))
        if v["posted"] is not None:
            r.check(v["posted"] == v["n"], "injector posted only %d of %d Enters in the control (gap %.2fs): harness/injector fault" % (v["posted"], v["n"], g))
    if any(v["bad"] or v.get("exact_bad") for v in res.values()):
        r.note("the injector/OS loses instant Enters even in the reference: fast_typing reports such a loss as 'injector/OS lost Enter (reference too)', not an app bug")


def enter_to_start(ctx, n, budget_s=None):
    ds = []
    t_end = time.time() + budget_s if budget_s else None
    for i in range(n):
        if t_end and time.time() > t_end:
            break
        f = os.path.join(ctx.W, "lat_%d" % i)
        if os.path.exists(f):
            os.remove(f)
        # macOS: BSD date has no %N, and the pane runs /bin/bash 3.2 (no $EPOCHREALTIME; zsh's `print` is "command not found" there and,
        # in agent mode, autodetect rightly sends it to the AI: r18ct queued 31 prompts). perl is in the base system; its start-up is
        # paid identically by the reference terminal, which types the same line.
        stamp = "perl -MTime::HiRes=time -e 'printf \"%%.6f\\n\", time' > lat_%d" if ctx.be.name == "mac" else "date +%%s.%%N > lat_%d"
        if ctx.pane == "powershell":
            stamp = "[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() | Out-File lat_%d -Encoding ascii"
        ctx.be.guard_ref()
        ctx.be.type(stamp % i, 12)
        time.sleep(0.3)
        t0 = time.time()
        ctx.be.key("Return")
        ctx.until(lambda: os.path.exists(f) and os.path.getsize(f) > 5, 3, step=0.001)
        try:
            v = float(open(f, errors="replace").read().replace("\x00", "").strip())
            ds.append(((v / 1000.0 if v > 1e11 else v) - t0) * 1000)       # PowerShell stamps UTC milliseconds
        except Exception:
            pass
        if i == 2 and not ds:
            # three stamps in a row never ran: the line is not reaching a shell (wrong shell, wrong destination, wedged pane). Stop
            # rather than type 30 more lines into whatever took them (an agent queue, a pager); the caller reports 0/n captured.
            ctx.be.key("ctrl+c")
            break
        time.sleep(0.2)
    return ds


@case("latency_enter", order=41, gui=True)
def c_lat_enter(ctx, r):
    """Enter -> command start, Phosphor vs a reference terminal in the SAME session with the SAME keys/cadence/timestamp command.
    Verdict is on OVERHEAD (Phosphor median - reference median) against the owner's 2 ms budget; both include one input-tool spawn,
    which cancels. No reference terminal -> verdict 'no-reference' (absolute numbers only). Also checked build-to-build (lat.py)."""
    n, warm = ctx.count(30), lat.warmup()
    t_lat0 = time.time()
    ds = enter_to_start(ctx, n + warm)[warm:]
    r.metric("enter_to_start_ms", stats(ds))
    share = ctx.app_enter_to_write_ms(t_lat0)
    if share is not None:
        # r18cj (Windows Git Bash): Phosphor 95-100 ms vs the conhost reference 69 ms for the SAME bash.exe/rc/`date +%s.%N`/injector. The app's own part is this number
        # (Enter -> the pty write of the whole line); the rest of the overhead is after the write: the shell reading the line (the reference's keys were echoed as typed)
        # plus the shell hooks Phosphor's bootstrap adds. Reported so the overhead can be split instead of argued.
        r.metric("app_enter_to_pty_write_ms", share)
        r.note("app share of Enter->start: enter_begin -> pty_write median %.2f ms (from the app's input_order log); the remaining overhead is shell/ConPTY side" % share)
    r.metric("harness_floor_ms", ctx.be.key_floor_ms)
    r.check(len(ds) >= n * 0.8, "only %d/%d Enter timings captured" % (len(ds), n))
    ctx.reset()
    rs, ref = [], ref_terminal(ctx)
    if ref:
        ref_note(ctx, r)
    if ref:
        try:
            time.sleep(2.5)
            rs = enter_to_start(ctx, n + warm, budget_s=max(60, (n + warm) * 2.5))[warm:]      # overall cap: a dead reference cannot hang the case
            ctx.reset()
        finally:
            ref.close()
        time.sleep(1.0)   # focus returns to Phosphor before any re-measure
        r.metric("reference_enter_to_start_ms", stats(rs))
        r.metric("reference", ref.name)
    else:
        r.note("no reference terminal available (need xterm+DISPLAY / foot / Terminal.app)%s" % (": " + ctx.ref_reason if getattr(ctx, "ref_reason", None) else ""))
    if ds:
        rec = lat.evaluate("%s/latency_enter/enter_to_start" % ctx.label, ds, rs, float(os.environ.get("APPCI_ENTER_BUDGET_MS", "2")), "overhead", plat(),
                           remeasure=lambda: enter_to_start(ctx, n + warm)[warm:], ref_name=ref.name if ref else None, floor=ctx.be.key_floor_ms,
                           notes=["reference measured after Phosphor (sequential, same session); Linux headless-GPU numbers compare builds, not absolute vs hand-measured baselines"])
        r.metrics.setdefault("lat", {})["enter_to_start"] = rec
        lat_checks(r, rec, "Enter to command start")
        if not rs and ds:
            r.check(statistics.median(ds) <= 80, "Enter to command start median %.1f ms > 80 ms CI sanity bound" % statistics.median(ds))
    ctx.reset()


def parse_t(line):
    m = re.search(r"\bt=(\d+\.\d+)", line)
    return float(m.group(1)) if m else None


KEYDOWN_RE = re.compile(r"input_order keydown key=\S+ ctrl=(true|false) alt=\S+ shift=\S+ cmd=\S+ t=(\d+\.\d+)(?: seq=(\d+))?")
FRAME_RE = re.compile(r"input_order frame layout=\S+ t=(\d+\.\d+)")


def key_frame_pairs(txt):
    """Pair every plain (no ctrl) `keydown` with the first `frame` line after it (formats: crates/warpui_core/src/input_order.rs,
    `keydown key=m ctrl=false alt=false shift=false cmd=false t=<ms> seq=N`, `frame layout=<bool> t=<ms>`; t is ms since the epoch).
    A frame is logged only for the first frame after pty output that followed a pty write (#1092), so a key that never reaches the
    pty (typed into the input editor) has no frame at all: keys that pile up before one frame all pair with that frame, and keys
    after the last frame stay unpaired. -> (latencies ms, counts dict)"""
    cnt = dict(keydown=0, keydown_ctrl=0, frame=0, pty_write=0, pty_read=0)
    pending, ds = [], []
    for l in txt.splitlines():
        m = KEYDOWN_RE.search(l)
        if m:
            cnt["keydown"] += 1
            if m.group(1) == "true":
                cnt["keydown_ctrl"] += 1
            else:
                pending.append(float(m.group(2)))
            continue
        m = FRAME_RE.search(l)
        if m:
            cnt["frame"] += 1
            tf = float(m.group(1))
            ds.extend(tf - t for t in pending if tf >= t)
            pending = []
        elif "input_order pty_write" in l:
            cnt["pty_write"] += 1
        elif "input_order pty_read" in l:
            cnt["pty_read"] += 1
    return ds, cnt


@case("latency_key_log", order=42, gui=True)
def c_lat_key(ctx, r):
    """key -> first frame after the echo, from the app's own input_order log (keydown ... pty_write -> pty_read -> frame);
    needs RUST_LOG input_order (set by the launcher). Phosphor-side only: no paint-readback through Wayland."""
    if not ctx.log_path or not os.path.exists(ctx.log_path):
        r.skip("no app log at %s (set APPCI_APP_LOG)" % ctx.log_path)
    n, warm = ctx.count(60), lat.warmup()
    SLOW = re.compile(r"slow ([\w .:/-]+?): (\d+) ms")

    def run_keys(count):
        # Typing at a shell prompt goes into Phosphor's input editor and reaches the pty only on Enter, so it has no pty echo and
        # the app logs no `frame` line for it (#1486). Keys are typed into a running `cat` instead: they go straight to the pty,
        # the tty echoes them, and the first frame after that echo is logged.
        ctx.be.type("cat > /dev/null", 10, enter=True)
        time.sleep(1.0)
        off = ctx.log_size()
        for i in range(count):
            ctx.be.type("m", 10)
            time.sleep(ctx.rng.uniform(0.08, 0.18))
        time.sleep(1)
        txt = ctx.log_since(off)
        ctx.reset()                  # Ctrl-C ends the cat
        ds, cnt = key_frame_pairs(txt)
        run_keys.counts = cnt
        return ds[warm:], [(m.group(1), int(m.group(2))) for m in SLOW.finditer(txt)]

    ds, slow = run_keys(n + warm)
    r.metric("key_to_frame_ms", stats(ds))
    if len(ds) < n * 0.5:
        cnt = getattr(run_keys, "counts", {})
        why = ("no keydown line at all: input_order logging off, wrong log path, or the keys never reached the app" if not cnt.get("keydown") else
               "keydowns but no `frame` line: no key reached the pty (the `cat` did not start, or this mode routes typing elsewhere), and the app only logs "
               "a frame after pty output that followed a pty write" if not cnt.get("frame") else
               "frames exist but few follow a keydown: check for a stuck Control modifier (ctrl=true keydowns)")
        r.check(False, "only %d key->frame pairs from the app log %s (expected ~%d); in the measurement window: %s. Diagnosis: %s"
                % (len(ds), ctx.log_path, n, ", ".join("%s=%d" % kv for kv in sorted(cnt.items())), why))
        return
    # Phosphor-side clock only (keydown -> first frame after the echo); there is no paint read-back for a reference terminal through
    # Wayland / ScreenCaptureKit at this resolution, so the 2 ms overhead budget cannot be judged: verdict 'no-reference'.
    rec = lat.evaluate("%s/latency_key_log/key_to_frame" % ctx.label, ds, None, float(os.environ.get("APPCI_KEY_BUDGET_MS", "2")), "overhead", plat(),
                       remeasure=lambda: run_keys(n + warm)[0], notes=["Phosphor-side keydown to first frame; no same-method reference exists"])
    r.metrics.setdefault("lat", {})["key_to_frame"] = rec
    lat_checks(r, rec, "key to frame")
    # UI work: the app's slow-operation log (#1146) only emits >= 8 ms, so 'pass' means "nothing >= 8 ms"; >= 16 ms is a dropped frame (WARN)
    worst_ms = max([m for _, m in slow], default=0)
    r.metric("ui_slow_ops", dict(count=len(slow), worst_ms=worst_ms, ops=sorted({w for w, _ in slow})[:8]))
    uiv = "warn" if worst_ms >= 16 else "pass"
    r.metrics["lat"]["ui_work"] = dict(phosphor=dict(n=len(slow), p50=None, p95=None, max=float(worst_ms)), reference=None, budget_ms=4, budget_kind="slow-log",
                                       overhead_ms=None, prev=None, baseline={"status": "n/a"}, budget_verdict=uiv, verdict=uiv,
                                       notes=["slow-op logger floor is 8 ms: verdict pass = no UI work item >= 8 ms during typing; warn = >= 16 ms"])
    if worst_ms >= 16:
        r.note("WARN UI work: slow op(s) up to %d ms during typing: %s" % (worst_ms, ", ".join(sorted({w for w, _ in slow})[:5])))
    p50, p99 = statistics.median(ds), pct(ds, 0.99)
    r.check(p50 <= 20, "key->frame p50 %.1f ms > 20 ms CI bound (owner target: next refresh, own work <= 2 ms; software GL here)" % p50)
    r.check(p99 <= 60, "key->frame p99 %.1f ms > 60 ms CI bound" % p99)
    ctx.reset()


def paint_samples(ctx, rect, n):
    """n x (post 'm' or Delete through kl3, ScreenCaptureKit 240 Hz) -> key->first-changed-pixel ms in the window at `rect`.
    The same call for Phosphor and the reference terminal. Alternates 'm' and Delete so every key changes the screen."""
    ds = []
    ctx.be.kl_start(rect)
    try:
        for i in range(n):
            out = ctx.be.kl("K 400 %s" % ("m" if i % 2 == 0 else "DEL"))
            m0 = re.search(r"post0=([\d.]+)", out)
            ch = [float(x.split(":")[0]) for x in (re.search(r"ch=(\S*)", out).group(1) if re.search(r"ch=(\S*)", out) else "").split(",") if x]
            if m0 and ch:
                t0 = float(m0.group(1))
                after = [c for c in ch if c >= t0 - 1]
                if after:
                    ds.append(min(after) - t0)
    finally:
        ctx.be.kl_stop()
    return ds


def pid_of(name):
    """newest exact PID of Terminal.app (the only name used): from `ps` by full executable path, never pgrep (no name matching)"""
    if name != "Terminal":
        return None
    import refterm_mac
    pids = refterm_mac.terminal_pids()
    return max(pids) if pids else None


@case("latency_paint", order=43, gui=True)
def c_lat_paint(ctx, r):
    """key -> first changed pixel in the terminal window, measured with kl3 (ScreenCaptureKit, 240 Hz frame times; macOS only), for
    Phosphor AND for the reference terminal (Terminal.app) with the same kl3 command in the same session. The verdict is on OVERHEAD
    (Phosphor median - reference median) against the owner's 2 ms key->paint budget; both include the same capture/injection cost
    (kl3 post + 240 Hz quantisation), which cancels. No reference -> 'no reference: absolute only' note."""
    if ctx.be.name != "mac" or not hasattr(ctx.be, "kl_start"):
        r.skip("kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare")
    rect = ctx.be.window_rect()
    if not rect or rect[2] < 100:
        r.skip("no Phosphor window rect from System Events")
    ctx.be.guard()
    n, warm = ctx.count(30), lat.warmup()
    ds = paint_samples(ctx, rect, n + warm)[warm:]
    ctx.reset()
    r.metric("key_to_paint_ms", stats(ds))
    r.check(len(ds) >= 20, "only %d key->paint samples from kl3 (capture region or Screen Recording permission wrong)" % len(ds))
    rs, ref = [], ref_terminal(ctx)
    if ref:
        try:
            time.sleep(2.5)
            rrect = ctx.be.window_rect(pid=pid_of("Terminal"))
            if not rrect or rrect[2] < 100:
                r.note("no reference: absolute only (key->paint): no Terminal.app window rect")
            else:
                rs = paint_samples(ctx, rrect, n + warm)[warm:]
            ctx.reset()
        finally:
            ref.close()
        time.sleep(1.0)
        r.metric("reference_key_to_paint_ms", stats(rs))
        r.metric("reference", ref.name)
    else:
        r.note("no reference: absolute only (key->paint): no reference terminal available%s" % (": " + ctx.ref_reason if getattr(ctx, "ref_reason", None) else ""))
    if ds:
        rec = lat.evaluate("%s/latency_paint/key_to_paint" % ctx.label, ds, rs, float(os.environ.get("APPCI_KEY_BUDGET_MS", "2")), "overhead", plat(),
                           remeasure=lambda: paint_samples(ctx, rect, n + warm)[warm:], ref_name=ref.name if ref and rs else None, floor=ctx.be.key_floor_ms,
                           notes=["kl3 measures post-to-first-changed-pixel at 240 Hz in each terminal's own window; reference measured after Phosphor in the same session"])
        r.metrics.setdefault("lat", {})["key_to_paint"] = rec
        lat_checks(r, rec, "Key to paint")
        if not rs:
            r.check(statistics.median(ds) <= 120, "key->paint median %.1f ms > 120 ms sanity bound (no reference measured)" % statistics.median(ds))


# ---------------------------------------------------------------- agent
@case("approval_card_pause", modes=("agent",), order=45, gui=True)
def c_approval(ctx, r):
    """Enter pressed >1 s after an approval card appears must NOT approve it (#1350/#1362). Needs a provider."""
    if not ctx.ollama:
        r.skip("no provider: configure APPCI_OLLAMA_URL (env or the machine-local ~/.config/phosphor-ci/ci.env) to run agent cases that need a model")
    if not shutil.which("tesseract"):
        r.skip("tesseract missing: cannot detect the approval card on screen")
    ctx.rm("ap.txt")
    ctx.be.type("Please run exactly this shell command and nothing else: echo APPROVAL_TOKEN >> ap.txt", 15, enter=True)
    seen = ctx.until(lambda: re.search(r"(?i)\b(reject|approve|run command|accept)\b", ctx.ocr("card") or ""), 120, step=3)
    if not seen:
        r.skip("no approval card appeared within 120 s (model did not propose the command)")
    time.sleep(1.5)
    ctx.be.key("Return")
    time.sleep(2)
    r.check(not ctx.lines("ap.txt"), "Enter after a >1 s pause approved the card (command ran)")
    ctx.shot("card_after_enter")
    ctx.be.key("ctrl+c")


# ---------------------------------------------------------------- tabs and windows (last: leaves the app busy)
# ---------------------------------------------------------------- code editor memory (#1340)
def editor_mem_file(path, kind, target_bytes):
    """a generated ~7 MB source file: 'txt' = plain prose-like lines (~85k lines), 'rs' = Rust-looking code with long lines"""
    words = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega".split()
    rng = random.Random(1340)
    size, i = 0, 0
    with open(path, "w") as f:
        while size < target_bytes:
            if kind == "rs":
                a, b = rng.choice(words), rng.choice(words)
                line = ("    pub fn %s_%d(&self, %s: &str, count: usize) -> Result<Vec<String>, Error> { let mut out = Vec::with_capacity(count); "
                        "for n in 0..count { out.push(format!(\"{}-{}\", %s, n * %d)); } Ok(out) } // %s %s\n" % (a, i, b, b, i % 97, a, b))
            else:
                line = "line %d: %s\n" % (i, " ".join(rng.choice(words) for _ in range(rng.randint(6, 14))))
            f.write(line)
            size += len(line)
            i += 1


def rss_mb(ctx):
    return ctx.app.cpu_ticks()[1] / 1024.0


def settle_rss(ctx, quiet_s=3.0, timeout=150):
    """wait until the app's RSS has stayed within +-2 MB for quiet_s seconds (file load is asynchronous and slow)"""
    t0 = time.time()
    last, since = rss_mb(ctx), time.time()
    while time.time() - t0 < timeout:
        time.sleep(0.75)
        cur = rss_mb(ctx)
        if abs(cur - last) > 2.0:
            last, since = cur, time.time()
        elif time.time() - since >= quiet_s:
            break
    return rss_mb(ctx)


@case("editor_memory_7mb", modes=("terminal",), order=85, gui=True)
def c_editor_mem(ctx, r):
    """#1340 P5: open a generated 7 MB file in the code editor (command palette + absolute path), record RSS after open, close the
    pane and record RSS after the undo-close grace window. Thresholds are the #1340 targets (7 MB plain text <= ~100 MB over baseline,
    7 MB Rust <= ~150 MB, memory released on close). Skips (APPCI_EDITOR_MEM_STRICT=1: fails) when the file does not open, since the
    palette / close key sequence is platform specific: APPCI_KEY_PALETTE (default ctrl+shift+p; mac cmd+p) and APPCI_KEY_CLOSE
    (default ctrl+shift+w; mac cmd+w) override it. Needs the process RSS via ps (Linux, macOS)."""
    try:
        rss_mb(ctx)
    except Exception as e:  # noqa: BLE001 - no ps / no pid on this platform
        r.skip("process RSS is not readable on this backend: %s" % e)
    palette = os.environ.get("APPCI_KEY_PALETTE", "ctrl+shift+p")
    close = os.environ.get("APPCI_KEY_CLOSE", "ctrl+shift+w")   # empty: skip the close phase
    grace = float(os.environ.get("APPCI_UNDO_CLOSE_WAIT_S", "75"))   # general.undo_close.grace_period is 60 s
    strict = os.environ.get("APPCI_EDITOR_MEM_STRICT", "") not in ("", "0")
    limits = {"txt": float(os.environ.get("APPCI_EDITOR_MEM_TXT_MAX_MB", "100")), "rs": float(os.environ.get("APPCI_EDITOR_MEM_RS_MAX_MB", "150"))}
    close_max = float(os.environ.get("APPCI_EDITOR_MEM_CLOSE_MAX_MB", "60"))
    kinds = [k for k in os.environ.get("APPCI_EDITOR_MEM_KINDS", "txt,rs").split(",") if k in limits]
    size = int(float(os.environ.get("APPCI_EDITOR_MEM_MB", "7")) * 1024 * 1024)
    ctx.reset()
    opened_any = False
    for kind in kinds:
        path = os.path.join(ctx.W, "editor_mem_%s.%s" % (kind, "rs" if kind == "rs" else "txt"))
        editor_mem_file(path, kind, size)
        base = settle_rss(ctx)
        ctx.be.key(palette)
        time.sleep(1.5)
        ctx.be.type(path, 12)
        time.sleep(1.0)
        ctx.be.key("enter")
        opened = settle_rss(ctx)
        delta = opened - base
        ctx.shot("editor_mem_%s_open" % kind)
        r.metric("%s_rss_base_mb" % kind, base)
        r.metric("%s_rss_open_mb" % kind, opened)
        r.metric("%s_rss_open_delta_mb" % kind, delta)
        if delta < 8:
            msg = "%s: RSS grew only %.1f MB after opening %s through the palette (%s): the file did not open; set APPCI_KEY_PALETTE" % (kind, delta, path, palette)
            if strict:
                r.check(False, msg)
            else:
                r.note("SKIPPED " + msg)
            ctx.be.key("escape")
            ctx.reset()
            continue
        opened_any = True
        r.check(delta <= limits[kind], "%s: opening a %d MB file cost %.0f MB of RSS (> %.0f MB target)" % (kind, size >> 20, delta, limits[kind]))
        if not close:
            r.note("%s: no close key on this platform (APPCI_KEY_CLOSE empty): release on close not measured" % kind)
            ctx.reset()
            continue
        try:
            ctx.be.key(close)
        except RuntimeError as e:  # the macOS guard refuses some chords
            r.note("%s: close key %r refused by the backend (%s): release on close not measured" % (kind, close, e))
            ctx.reset()
            continue
        time.sleep(grace)
        after = settle_rss(ctx)
        r.metric("%s_rss_close_mb" % kind, after)
        r.metric("%s_rss_close_delta_mb" % kind, after - base)
        ctx.shot("editor_mem_%s_closed" % kind)
        r.check(after - base <= close_max, "%s: %.0f MB of RSS still held %.0f s after closing the editor (> %.0f MB over the pre-open baseline %.0f MB)" % (kind, after - base, grace, close_max, base))
        try:
            os.remove(path)
        except OSError:
            pass
        ctx.reset()
    if not opened_any and not strict:
        r.skip("the generated file never opened in the code editor on this backend (see notes)")
    r.check(ctx.app.alive(), "app died during the editor memory case")


@case("gitbash_subshell", order=22)
def c_gitbash_subshell(ctx, r):
    """Windows (#1464): Git for Windows' bash.exe started from the pane's PowerShell as a subshell: 20 fast-typed lines, 4 pasted lines of 10-5000
    bytes with an instant Enter, 3 Ctrl-C of a trapped `sleep 30`, `exit` back to PowerShell and the next PowerShell command runs there."""
    if not ctx.win:
        r.skip("Windows only: Git for Windows' bash.exe as a subshell of the pane's PowerShell (POSIX hosts: the subshells case)")
    if ctx.pane != "powershell":
        r.skip("the pane IS Git Bash in this pass; bash inside bash is covered by ctrl_c_d / subshells")
    import winapi
    gb = winapi.git_bash()
    if not gb:
        r.skip("Git for Windows not installed (no bash.exe under Program Files\\Git\\bin)")
    names = ("gb_probe", "gb_ft", "gb_pe", "gb_cc", "gb_back")
    ctx.rm(*[n + ".txt" for n in names])
    ctx.be.type("& '%s' --noprofile --norc -i" % gb.replace("\\", "/"), 12, enter=True)
    time.sleep(4)
    ready = False
    for _ in range(4):
        ctx.be.type("echo probe >> gb_probe.txt", 12, enter=True)
        if ctx.until(lambda: ctx.lines("gb_probe.txt"), 4):
            ready = True
            break
    ctx.shot("gitbash_started")
    if not ready:
        r.check(False, "Git Bash never executed a probe line (cwd %s)" % ctx.W)
        ctx.reset()
        return
    rl = lambda n: [l.rstrip("\r") for l in ctx.lines(n)]       # noqa: E731
    exp = []
    for _ in range(20):
        tok = ctx.tok(8).lower()
        exp.append(tok)
        ctx.be.type("echo %s >> gb_ft.txt" % tok, ctx.rng.randint(10, 25), enter=True)
    ctx.until(lambda: len(rl("gb_ft.txt")) >= 20, 30)
    r.check(rl("gb_ft.txt") == exp, "Git Bash fast typing: %d/20 lines, exact match false (expected first=%s got first=%s)" % (len(rl("gb_ft.txt")), exp[0], (rl("gb_ft.txt") or [None])[0]))
    exp = []
    for i in range(4):
        tok, pad = ctx.tok(8).lower(), "w" * (10, 500, 5000, 50)[i]
        exp.append(tok + pad)
        ctx.be.clip_set("echo %s%s >> gb_pe.txt" % (tok, pad))
        ctx.be.paste(enter=True)
        time.sleep(0.9)
    ctx.until(lambda: len(rl("gb_pe.txt")) >= 4, 20)
    r.check(rl("gb_pe.txt") == exp, "Git Bash paste + instant Enter: %d/4 exact lines" % sum(a == b for a, b in zip(exp, rl("gb_pe.txt"))))
    c = 0
    for i in range(3):
        ctx.be.type("bash -c 'trap \"echo INT%d >> gb_cc.txt; exit\" INT; sleep 30'; echo AFTER%d >> gb_cc.txt" % (i, i), 12, enter=True)
        time.sleep(1.8)
        ctx.be.key("ctrl+c")
        if ctx.until(lambda: len(rl("gb_cc.txt")) >= 2 * (i + 1), 8):
            c += 1
    r.check(c == 3, "Git Bash Ctrl-C: the trap and the next line ran %d/3 times: %s" % (c, ",".join(rl("gb_cc.txt"))))
    ctx.be.type("exit", 12, enter=True)
    time.sleep(2)
    tok = ctx.tok(8).lower()
    ctx.be.type("Add-Content gb_back.txt %s" % tok, 12, enter=True)
    ctx.until(lambda: rl("gb_back.txt"), 8)
    r.check(rl("gb_back.txt") == [tok], "after exiting Git Bash the pane's PowerShell did not run the next command: %s" % rl("gb_back.txt"))


def stream_cmd(ctx, i):
    """a command that prints a line every ~20 ms until Ctrl-C, for the pane's own shell"""
    if ctx.pane == "powershell":
        return "while ($true) { 'streaming_%d_line'; Start-Sleep -Milliseconds 20 }" % i
    return "while :; do echo streaming_%d_line; sleep 0.02; done" % i


@case("tabs_streaming", order=91, gui=True)
def c_tabs_streaming(ctx, r):
    """12 tabs each streaming a line every 20 ms, then ONE more tab: 10 fast-typed commands in it arrive exactly, in order, and the app survives
    (the Windows tabs_streaming case of app_test.ps1, now on every platform; tabs_two_windows adds a second window and a paste)"""
    ntabs = int(os.environ.get("APPCI_TABS", "12"))
    newtab = os.environ.get("APPCI_KEY_NEWTAB", "ctrl+shift+t")
    ctx.reset()
    for i in range(ntabs):
        ctx.be.key(newtab)
        time.sleep(0.7)
        ctx.be.type(stream_cmd(ctx, i), 10, enter=True)
    ctx.rm("tw_s.txt")
    ctx.be.key(newtab)
    time.sleep(1.0)
    exp = []
    for _ in range(ctx.count(10)):
        tok = ctx.tok(8).lower()
        exp.append(tok)
        ctx.be.type(ctx.append_cmd(ctx.pane_shell(), tok, "tw_s.txt"), ctx.rng.randint(10, 25), enter=True)
    ctx.until(lambda: len(ctx.lines("tw_s.txt")) >= len(exp), 40)
    time.sleep(0.5)
    ctx.shot("tabs_streaming")
    r.check(ctx.app.alive(), "app died with %d streaming tabs" % ntabs)
    got = ctx.lines("tw_s.txt")
    r.check(got == exp, "typing under %d streaming tabs: %d/%d exact lines" % (ntabs, len(got), len(exp)))


@case("tabs_two_windows", order=90, gui=True)
def c_tabs(ctx, r):
    ntabs = int(os.environ.get("APPCI_TABS", "12"))
    newwin = os.environ.get("APPCI_KEY_NEWWIN", "ctrl+shift+n")
    newtab = os.environ.get("APPCI_KEY_NEWTAB", "ctrl+shift+t")
    ctx.reset()
    for i in range(ntabs):
        ctx.be.key(newtab)
        time.sleep(0.5)
        ctx.be.type(stream_cmd(ctx, i), 10, enter=True)
        time.sleep(0.2)
    ctx.shot("tabs_open")
    alive = ctx.app.alive()
    r.check(alive, "app died while opening %d streaming tabs" % ntabs)
    ctx.be.key(newwin)
    time.sleep(3)
    w = ctx.be.windows()
    r.metric("windows", len(w))
    if w:
        r.check(len(w) >= 2, "second window did not open (%d windows; key %s)" % (len(w), newwin))
    ctx.rm("tw.txt")
    exp = []
    mark = ctx.cmd_mark()
    for i in range(ctx.count(15)):
        tok = ctx.tok(8).lower()
        exp.append(tok)
        ctx.be.type(ctx.append_cmd(ctx.pane_shell(), tok, "tw.txt"), ctx.rng.randint(10, 25), enter=True)
    ctx.be.clip_set(ctx.append_cmd(ctx.pane_shell(), "PASTED_ok", "tw.txt"))
    ctx.be.paste(enter=True)
    exp.append("PASTED_ok")
    ctx.until(lambda: len(ctx.lines("tw.txt")) >= len(exp), 30)
    r.check(ctx.lines("tw.txt") == exp, "typing/paste in window 2 under %d streaming tabs: %d/%d exact lines" % (ntabs, len(ctx.lines("tw.txt")), len(exp)))
    ctx.shot("two_windows")
    r.check(ctx.app.alive(), "app not alive at the end of the tabs/windows case")


# ---------------------------------------------------------------- markdown / mermaid viewer (#1460)
MD_SMALL = """# PhosphorMdProbe

Intro paragraph with `inline code` and a [link](https://example.com).

| Fruit | Count |
|-------|-------|
| ZEBRAAPPLE | 42 |
| QUINCEBERRY | 17 |

```mermaid
graph TD
    A[AlphaNodeStart] --> B[BetaNodeMiddle]
    B --> C[GammaNodeEnd]
```

Closing paragraph.
"""
MD_SMALL_TEXT = ("phosphormdprobe", "zebraapple", "quinceberry")           # heading + table cells (lower-cased, spaces stripped)
MD_SMALL_DIAGRAM = ("alphanodestart", "betanodemiddle", "gammanodeend")    # mermaid node labels, drawn from the SVG


def _squash(txt):
    return "".join((txt or "").lower().split())


def open_file_in_app(ctx, path):
    """Hand a local file to the RUNNING app the way the OS would (file:// URL -> uri::open_file -> markdown viewer pane
    when code.editor.prefer_markdown_viewer, default true). Linux: a second invocation of the same binary inside the app's own
    D-Bus session forwards the URL to the running instance (app_services/linux). macOS: `open -a <bundle> <file>`
    (LaunchServices open-URL event to the running instance). Returns a failure string or None."""
    if ctx.be.name == "mac":
        r = subprocess.run(["open", "-a", ctx.app.bundle, path], capture_output=True, timeout=30)
        return None if r.returncode == 0 else "open -a failed: %s" % r.stderr.decode(errors="replace")[:200]
    if ctx.win:
        raise Skip("no verified way to hand a file:// URL to the RUNNING Windows instance (a second invocation may start a second app instance on the same data dir); not covered on Windows")
    pid = ctx.app.pid()
    try:
        raw = open("/proc/%d/environ" % pid, "rb").read().split(b"\0")
    except Exception as e:
        return "cannot read the app environment (%r)" % (e,)
    env = dict(x.decode(errors="replace").split("=", 1) for x in raw if b"=" in x)
    if not env.get("DBUS_SESSION_BUS_ADDRESS"):
        return "app has no DBUS_SESSION_BUS_ADDRESS: cannot forward a second invocation"
    p = subprocess.Popen([ctx.app.bin, "file://" + path], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
    with open(os.path.join(ctx.run, "pids"), "a") as f:
        f.write("md-open %d\n" % p.pid)
    try:
        p.wait(timeout=30)
    except subprocess.TimeoutExpired:
        os.killpg(p.pid, 9)          # exactly the helper we started, never by name
        return "the forwarding invocation did not exit within 30 s"
    return None


def wait_for_text(ctx, tag, needles, timeout):
    """Poll OCR until every needle (space-less, lower-case) is on screen. Returns (seconds, found_text) ; seconds None on timeout."""
    t0 = time.time()
    txt = ""
    while time.time() - t0 < timeout:
        got = ctx.ocr(tag)
        if got is None:
            return None, None
        txt = _squash(got)
        if all(n in txt for n in needles):
            return time.time() - t0, txt
        time.sleep(0.7)
    return None, txt


@case("md_viewer", order=80, gui=True)
def c_md_viewer(ctx, r):
    """Opens a small .md (table + mermaid flowchart) in the app's markdown viewer pane; screenshot proof, OCR of the table text
    and the diagram's node labels. Agent-mode AI answers are covered by md_viewer_ai_answer. Windows: not covered (harness is
    app_test.ps1; no OCR engine there)."""
    path = os.path.join(ctx.W, "probe.md")
    open(path, "w").write(MD_SMALL)
    off = ctx.log_size()
    err = open_file_in_app(ctx, path)
    r.check(err is None, "could not hand the file to the running app: %s" % err)
    if err:
        return
    time.sleep(2)
    secs, txt = wait_for_text(ctx, "md_small", MD_SMALL_TEXT, 20)
    if txt is None:
        ctx.shot("md_small_noocr")
        r.note("no OCR engine (tesseract / Apple Vision): screenshot shots/%s_md_small_noocr.png only, table text and diagram labels NOT machine-verified" % ctx.label)
    else:
        r.metric("md_small_text_visible_s", round(secs, 2) if secs is not None else None)
        r.check(secs is not None, "markdown viewer: heading/table text %s not visible within 20 s (OCR saw %r)" % (list(MD_SMALL_TEXT), txt[:300]))
        _, txt2 = wait_for_text(ctx, "md_small_diagram", MD_SMALL_DIAGRAM, 15)
        missing = [n for n in MD_SMALL_DIAGRAM if n not in (txt2 or "")]
        r.check(not missing, "mermaid flowchart node labels not visible (blank / failed diagram block?): missing %s; OCR saw %r" % (missing, (txt2 or "")[:300]))
    new = ctx.log_since(off)
    r.check("panicked at" not in new, "app log shows a panic while opening the markdown file")
    r.check(ctx.app.alive(), "app died while opening the markdown file")


@case("md_viewer_big", modes=("terminal",), order=81, gui=True)
def c_md_viewer_big(ctx, r):
    """2 MB markdown file (full tier only; SMOKE keeps the small file): time until its first heading is on screen and the app's RSS growth."""
    if os.environ.get("APPCI_TIER") == "smoke":
        raise Skip("smoke tier: only the small file")
    rr = random.Random(11)
    out, size = ["# BIGDOCSTART", ""], 0
    i = 0
    while size < 2_000_000:
        i += 1
        para = " ".join("".join(rr.choice("abcdefghijklmnopqrstuvwxyz") for _ in range(rr.randint(2, 9))) for _ in range(rr.randint(20, 60)))
        out += ["## Section %d" % i, "", para, "", "- item %d a" % i, "- item %d b" % i, "", "| k | v |", "|---|---|", "| row%d | %d |" % (i, i), ""]
        size += len(para) + 120
    path = os.path.join(ctx.W, "big.md")
    open(path, "w").write("\n".join(out))
    r.metric("md_bytes", os.path.getsize(path))
    rss0 = ctx.app.cpu_ticks()[1]
    t0 = time.time()
    err = open_file_in_app(ctx, path)
    r.check(err is None, "could not hand the file to the running app: %s" % err)
    if err:
        return
    secs, txt = wait_for_text(ctx, "md_big", ("bigdocstart",), 90)
    if txt is None:
        raise Skip("no OCR engine: cannot time the render (screenshot only)")
    rss1 = ctx.app.cpu_ticks()[1]
    r.metric("md_big_render_s", round(time.time() - t0, 2) if secs is not None else None)
    r.metric("md_big_rss_delta_mb", round((rss1 - rss0) / 1024.0, 1))
    # Budgets are first guesses (no measurement yet): tighten after the first CI numbers (docs/testing.md).
    r.check(secs is not None, "2 MB markdown file: first heading not visible within 90 s")
    r.check((rss1 - rss0) / 1024.0 < 3000, "2 MB markdown file grew the app by %.0f MB RSS (budget 3000 MB)" % ((rss1 - rss0) / 1024.0))
    r.check(ctx.app.alive(), "app died rendering the 2 MB markdown file")


@case("md_viewer_ai_answer", modes=("agent",), order=82, gui=True)
def c_md_ai_answer(ctx, r):
    """Agent mode: ask the CI model for a markdown table + mermaid flowchart and look at the rendered answer. Needs the CI Ollama
    model (ctx.ollama); there is no fake-model path in the app. Model output is nondeterministic, so the OCR result is a NOTE and a
    metric, never a failure; a failure is only an app crash / panic."""
    if not ctx.ollama:
        raise Skip("no local/CI model configured (APPCI_OLLAMA_URL): no deterministic AI-answer path exists, so agent-mode markdown rendering is only covered by the unit corpus")
    off = ctx.log_size()
    ctx.be.type("Reply with only this markdown and nothing else: a table with header Fruit and Count and one row ZEBRAAPPLE and 42, then a fenced mermaid block containing graph TD with A[AlphaNodeStart] --> B[BetaNodeMiddle]", 12, enter=True)
    secs, txt = wait_for_text(ctx, "md_ai", ("zebraapple",), 120)
    if txt is None:
        raise Skip("no OCR engine: screenshot only")
    r.metric("ai_table_visible", secs is not None)
    _, txt2 = wait_for_text(ctx, "md_ai_diagram", ("alphanodestart",), 10)
    r.metric("ai_diagram_labels_visible", bool(txt2 and "alphanodestart" in txt2))
    if secs is None:
        r.note("model answer did not show ZEBRAAPPLE within 120 s (model nondeterminism or no answer); see shots/agent_md_ai.png")
    r.check("panicked at" not in ctx.log_since(off), "app log shows a panic while rendering the AI markdown answer")
    r.check(ctx.app.alive(), "app died rendering the AI markdown answer")
    ctx.be.key("ctrl+c")
import cases_caseagent  # noqa: E402,F401  (#1463: agent_stream_cancel, agent_approval, routing_table)
import cases_ui  # noqa: E402,F401  (#1462: completions/history, mouse/panes/resize, startup + live settings cases)
import cases_robust  # noqa: E402,F401  (#1468: session restore, quit with 24 tabs, floods, soak, code editor)
import cases_caseremote  # noqa: E402,F401  (issue #1464: remote work over ssh + per-shell variants)
import cases_caseintl  # noqa: E402,F401  (group 8: international input, #1466)
