# appci/cases.py: the release-test matrix of docs/testing.md 3.5 as machine-checkable cases.
# Every case verifies the EXACT command / bytes the shell received (effect files written by the typed commands
# plus the shell's own rec.log from the scratch rc), never "something ran". Cases run once per session mode
# (terminal, agent) unless marked otherwise. A case function gets (ctx, r); r.check(ok, msg) records failures,
# r.metric(k, v) numbers, r.skip(msg) a skip. Typing is at the owner's stress rate: 10-25 ms/key, Enter in the same call.
import hashlib, json, os, random, re, shutil, statistics, subprocess, sys, time

CASES = []


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

    def check(self, ok, msg):
        if not ok:
            self.fails.append(msg)
        return ok

    def note(self, msg):
        self.notes.append(msg)

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


class Ctx:
    def __init__(self, be, prof, app, run, mode, seed=1, ssh=None, ollama=False, log_path=None):
        self.be, self.prof, self.app, self.run, self.mode = be, prof, app, run, mode
        self.rng = random.Random(seed)
        self.ssh, self.ollama, self.log_path = ssh, ollama, log_path
        self.W = prof.home + "/w"
        self.n = 0
        self.scale = float(os.environ.get("APPCI_SCALE", "1"))   # <1 shortens counts for smoke runs

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
        try:
            return open(os.path.join(base, name), errors="replace").read().splitlines()
        except FileNotFoundError:
            return []

    def rm(self, *names):
        for n in names:
            try:
                os.remove(os.path.join(self.W, n))
            except FileNotFoundError:
                pass

    def append_cmd(self, shell, tok, fname):
        if shell == "nu":
            return '"%s" | save --append %s' % (tok, fname)
        return "echo %s >> %s" % (tok, fname)

    def prompts(self):
        return self.prof.prompts()

    def reset(self):
        self.be.key("ctrl+c")
        time.sleep(0.25)
        self.be.key("ctrl+u")
        time.sleep(0.15)

    def shot(self, tag):
        p = os.path.join(self.prof.shots, "%s_%s.png" % (self.mode, tag))
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

    def ocr(self, tag):
        if not shutil.which("tesseract"):
            return None
        p = self.shot(tag)
        if not p:
            return None
        r = subprocess.run(["tesseract", p, "-"], capture_output=True, timeout=60)
        return r.stdout.decode(errors="replace")


# ---------------------------------------------------------------- mode proof
@case("mode_proof", order=1, gui=True)
def c_mode_proof(ctx, r):
    """screenshot proof of the session mode before any key is typed; OCR checks it when tesseract exists"""
    ctx.be.key("Escape")
    time.sleep(0.8)
    txt = ctx.ocr("proof")
    if txt is None:
        r.note("no tesseract: screenshot only (shots/%s_proof.png), mode not machine-verified" % ctx.mode)
        return
    low = txt.lower()
    agent_markers = ("ask anything", "new phosphor agent", "agent conversation")
    is_agent = any(m in low for m in agent_markers) and "/agent to start" not in low
    r.metric("ocr_agent_markers", is_agent)
    if ctx.mode == "agent":
        r.check(is_agent, "agent mode expected but the screen shows no agent markers ('Ask anything' / 'New Phosphor Agent conversation')")
    else:
        r.check(not is_agent, "terminal mode expected but the screen shows agent markers")


# ---------------------------------------------------------------- cadence
@case("cadence", order=2, gui=True)
def c_cadence(ctx, r):
    """the input tool really types at 10-25 ms/key: arrival times measured at the pty by ts_reader.py"""
    out = os.path.join(ctx.W, "cad.txt")
    ctx.rm("cad.txt")
    n = 60
    ctx.be.type("python3 %s %s %d" % (os.path.join(os.path.dirname(os.path.abspath(__file__)), "ts_reader.py"), "cad.txt", n), 12, enter=True)
    time.sleep(2.0)   # python3 start + READY
    delay = 15
    ctx.be.type("".join(chr(97 + i % 26) for i in range(n)), delay)
    ctx.until(lambda: os.path.exists(out), 15)
    try:
        rows = [l.split() for l in open(out)]
    except FileNotFoundError:
        r.check(False, "ts_reader produced no output (python3 missing or app did not run it)")
        return
    ts = [float(b) for _, b in rows]
    gaps = [(b - a) * 1000 for a, b in zip(ts, ts[1:])]
    r.metric("gap_ms", stats(gaps))
    if gaps:
        med = statistics.median(gaps)
        r.check(5 <= med <= 40, "median inter-key gap %.1f ms is outside 5..40 ms for a requested %d ms (input tool cadence wrong)" % (med, delay))
    r.check(len(rows) == n, "only %d of %d characters arrived" % (len(rows), n))
    ctx.reset()


# ---------------------------------------------------------------- typing
def effect_run(ctx, r, tag, n, gap, rollover=False, shell="bash"):
    fname = "r_%s.txt" % tag
    ctx.rm(fname)
    mark = ctx.cmd_mark()
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
    r.check(ok, "%s: file has %d/%d lines, exact match %s%s" % (tag, len(got), n, ok,
            "" if ok else " first diff: exp=%r got=%r" % next(((a, b) for a, b in zip(exp + [None], got + [None]) if a != b), (None, None))))
    if ctx.prof.commands():
        want = [ctx.append_cmd(shell, t, fname) for t in exp]
        have = ctx.cmds_since(mark)
        if have:
            r.check(have == want, "%s: shell received %d commands, not the typed ones (first mismatch: %r)" % (tag, len(have), next((h for h, w in zip(have, want) if h != w), "count differs %d vs %d" % (len(have), len(want)))))
        else:
            r.note("%s: rec.log empty (PROMPT_COMMAND hook not run), effect file checked only" % tag)
    return ok


@case("fast_typing", order=10)
def c_fast(ctx, r):
    for gap in (0.0, 0.25):
        effect_run(ctx, r, "ft%d" % int(gap * 100), ctx.count(30), gap)


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
    exp = []
    for i in range(ctx.count(6)):
        tok = ctx.tok(8)
        pad = "w" * ctx.rng.choice([10, 500, 5000])
        ctx.be.clip_set("echo %s%s | md5sum >> pe.txt" % (tok, pad))
        exp.append(hashlib.md5((tok + pad + "\n").encode()).hexdigest() + "  -")
        ctx.be.paste(enter=True)
        time.sleep(0.8)
    ctx.until(lambda: len(ctx.lines("pe.txt")) >= len(exp), 15)
    r.check(ctx.lines("pe.txt") == exp, "paste + instant Enter: %d/%d md5 lines exact" % (sum(a == b for a, b in zip(exp, ctx.lines("pe.txt"))), len(exp)))


@case("arrows_behind_paste", order=14)
def c_paste_arrows(ctx, r):
    ctx.rm("pa.txt")
    exp = []
    for i in range(ctx.count(5)):
        tok = ctx.tok(6)
        pad = "q" * ctx.rng.choice([100, 3000])
        ctx.be.clip_set(tok + pad)
        exp.append(hashlib.md5((tok + pad + "xzy\n").encode()).hexdigest() + "  -")
        ctx.be.type("echo ", 12)
        ctx.be.paste()
        ctx.be.keys(["x", "y", "Left", "z", "End"], delay_ms=8)
        ctx.be.type(" | md5sum >> pa.txt", 8, enter=True)
        time.sleep(1.0)
    ctx.until(lambda: len(ctx.lines("pa.txt")) >= len(exp), 15)
    r.check(ctx.lines("pa.txt") == exp, "arrow keys behind a paste: %d/%d exact" % (sum(a == b for a, b in zip(exp, ctx.lines("pa.txt"))), len(exp)))


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
            ctx.be.type("echo %s >> tc/zzcompl_alpha_u" % tok, 15)
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


@case("ctrl_c_d", order=17)
def c_ctrl(ctx, r):
    ctx.rm("cc.txt", "cd.txt", "cd2.txt")
    lat, n = [], ctx.count(6)
    for i in range(n):
        ctx.be.type("bash -c 'trap \"date +%%s.%%N >> cc.txt; exit\" INT; sleep 30'; echo AFTER%d >> cc.txt" % i, 10, enter=True)
        time.sleep(1.2)
        t0 = time.time()
        ctx.be.key("ctrl+c")
        ctx.until(lambda: sum(1 for l in ctx.lines("cc.txt") if l.startswith("AFTER")) > i, 5)
        time.sleep(0.2)
        ts = [float(l) for l in ctx.lines("cc.txt") if re.fullmatch(r"\d+\.\d+", l)]
        if len(ts) > len(lat):
            lat.append((ts[len(lat)] - t0) * 1000)
    r.metric("ctrl_c_ms", stats(lat))
    r.metric("harness_floor_ms", ctx.be.key_floor_ms)
    r.check(len(lat) == n, "Ctrl-C delivered %d/%d times" % (len(lat), n))
    after = [l for l in ctx.lines("cc.txt") if l.startswith("AFTER")]
    r.check(len(after) == n, "command after Ctrl-C ran %d/%d times" % (len(after), n))
    if lat:
        # owner budget: Ctrl-C <= 20 ms under flood; the measurement includes the input tool's own spawn (the floor)
        over = statistics.median(lat) - ctx.be.key_floor_ms
        r.metric("ctrl_c_median_minus_floor_ms", over)
        r.check(over <= 40, "Ctrl-C median %.1f ms over the harness floor exceeds 40 ms (owner budget 20 ms + 20 ms CI slack)" % over)
    # Ctrl-D: ends `cat` input, and exits a nested bash back to the outer shell
    ctx.be.type("cat > cd.txt", 12, enter=True)
    time.sleep(1)
    ctx.be.type("hello", 12, enter=True)
    time.sleep(0.5)
    ctx.be.key("ctrl+d")
    time.sleep(1.5)
    ctx.be.type("echo done1 >> cd.txt", 12, enter=True)
    time.sleep(1)
    r.check(ctx.lines("cd.txt") == ["hello", "done1"], "Ctrl-D in cat: file %r" % ctx.lines("cd.txt"))
    ctx.be.type("echo L$SHLVL >> cd2.txt", 12, enter=True)
    time.sleep(0.8)
    ctx.be.type("bash", 12, enter=True)
    time.sleep(2)
    ctx.be.type("echo L$SHLVL >> cd2.txt", 12, enter=True)
    time.sleep(0.8)
    ctx.be.key("ctrl+d")
    time.sleep(1.5)
    ctx.be.type("echo L$SHLVL >> cd2.txt", 12, enter=True)
    time.sleep(1)
    l = ctx.lines("cd2.txt")
    ok = len(l) == 3 and l[0] == l[2] and l[1] == "L%d" % (int(l[0][1:]) + 1)
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
        r.skip("no local ssh server (needs /usr/sbin/sshd + ssh-keygen, or python3-paramiko for fakesshd.py)")
    trials = int(os.environ.get("APPCI_SSH_TRIALS", "3"))
    ctx.rm("rl.txt")
    for f in ("rl.txt",):
        try:
            os.remove(os.path.join(ctx.prof.rhome, f))
        except FileNotFoundError:
            pass
    exp, miss = [], 0
    for gap in (0.0, 0.1, 0.2, 0.3):
        for i in range(trials):
            tok = ctx.tok(6)
            exp.append(tok)
            ctx.be.type("ssh fake", 12, enter=True)
            time.sleep(gap)
            ctx.be.type("echo %s >> ~/rl.txt" % tok, 15, enter=True)
            if not ctx.until(lambda: tok in ctx.lines("rl.txt", remote=True), 20):
                miss += 1
            before = ctx.prompts()
            ctx.be.type("exit", 12, enter=True)
            ctx.until(lambda: ctx.prompts() > before, 10)
            time.sleep(0.5)
    got = ctx.lines("rl.txt", remote=True)
    r.metric("trials", len(exp))
    r.check(got == exp, "remote shell received %d/%d typed-ahead commands exactly (missing %d)" % (sum(1 for t in exp if t in got), len(exp), miss))
    r.check(not ctx.lines("rl.txt"), "typed-ahead command ran LOCALLY: %r" % ctx.lines("rl.txt"))
    rem = [c for _, c in ctx.prof.commands(remote=True)]
    if rem:
        r.check(all("echo %s >> ~/rl.txt" % t in rem for t in exp), "remote rec.log lacks the exact typed command for some trials")
    ctx.reset()


@case("ssh_failing_not_local", order=31, gui=True)
def c_ssh_fail(ctx, r):
    ctx.rm("sf.txt")
    ctx.be.type("ssh -o ConnectTimeout=3 -o BatchMode=yes no-such-host.invalid", 12, enter=True)
    time.sleep(0.15)
    ctx.be.type("echo MUSTNOT >> sf.txt", 15, enter=True)
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
    pct_ = (t1 - t0) / os.sysconf("SC_CLK_TCK") / (time.time() - w0) * 100
    r.metric("idle_cpu_pct_of_one_core", pct_)
    r.metric("rss_mb", rss1 / 1024)
    limit = float(os.environ.get("APPCI_IDLE_CPU_MAX", "15"))
    r.check(pct_ <= limit, "idle CPU %.1f%% of one core exceeds CI limit %.0f%% (compositor/GPU path differs from a desktop: compare trends)" % (pct_, limit))


def ref_terminal(ctx):
    """optional reference terminal (xterm via XWayland / Xvfb) with the same rc, for same-session comparison"""
    if not shutil.which("xterm") or not os.environ.get("DISPLAY"):
        return None
    return subprocess.Popen(["xterm", "-geometry", "120x30+0+0", "-e", "bash", "--rcfile", ctx.prof.home + "/.bashrc", "-i"],
                            env=dict(os.environ, HOME=ctx.prof.home), start_new_session=True,
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def enter_to_start(ctx, n):
    ds = []
    for i in range(n):
        f = os.path.join(ctx.W, "lat_%d" % i)
        if os.path.exists(f):
            os.remove(f)
        ctx.be.type("date +%%s.%%N > lat_%d" % i, 12)
        time.sleep(0.3)
        t0 = time.time()
        ctx.be.key("Return")
        ctx.until(lambda: os.path.exists(f) and os.path.getsize(f) > 5, 3, step=0.001)
        try:
            ds.append((float(open(f).read()) - t0) * 1000)
        except Exception:
            pass
        time.sleep(0.2)
    return ds


@case("latency_enter", order=41, gui=True)
def c_lat_enter(ctx, r):
    """Enter -> command start. Owner budget 2 ms for Phosphor's own work, FAIL > 5 ms worse than the reference;
    raw numbers include one input-tool spawn (harness_floor_ms), so the verdict is relative to the reference terminal
    when one is available, else an absolute CI sanity bound."""
    n = ctx.count(30)
    ds = enter_to_start(ctx, n)
    r.metric("enter_to_start_ms", stats(ds))
    r.metric("harness_floor_ms", ctx.be.key_floor_ms)
    r.check(len(ds) >= n * 0.8, "only %d/%d Enter timings captured" % (len(ds), n))
    ctx.reset()
    ref = ref_terminal(ctx)
    if ref:
        try:
            time.sleep(2.5)
            rs = enter_to_start(ctx, n)
            r.metric("reference_xterm_enter_to_start_ms", stats(rs))
            if ds and rs:
                d = statistics.median(ds) - statistics.median(rs)
                r.metric("phosphor_minus_reference_median_ms", d)
                r.check(d <= 5, "Enter to command start is %.1f ms worse than xterm in the same session (budget FAIL > 5 ms)" % d)
        finally:
            try:
                os.killpg(ref.pid, 15)
            except Exception:
                pass
    else:
        r.note("no xterm/DISPLAY: no reference terminal; absolute bound only")
        if ds:
            r.check(statistics.median(ds) <= 80, "Enter to command start median %.1f ms > 80 ms CI sanity bound" % statistics.median(ds))


def parse_t(line):
    m = re.search(r"\bt=(\d+\.\d+)", line)
    return float(m.group(1)) if m else None


@case("latency_key_log", order=42, gui=True)
def c_lat_key(ctx, r):
    """key -> first frame after the echo, from the app's own input_order log (keydown ... pty_write -> pty_read -> frame);
    needs RUST_LOG input_order (set by the launcher). Phosphor-side only: no paint-readback through Wayland."""
    if not ctx.log_path or not os.path.exists(ctx.log_path):
        r.skip("no app log at %s (set APPCI_APP_LOG)" % ctx.log_path)
    off = ctx.log_size()
    n = ctx.count(60)
    for i in range(n):
        ctx.be.type("m", 10)
        time.sleep(ctx.rng.uniform(0.08, 0.18))
        if i % 10 == 9:
            ctx.be.key("ctrl+u")
            time.sleep(0.2)
    time.sleep(1)
    txt = ctx.log_since(off)
    ds, kd = [], None
    for l in txt.splitlines():
        if "input_order keydown" in l and "ctrl=false" in l:
            kd = parse_t(l)
        elif "input_order frame" in l and kd is not None:
            t = parse_t(l)
            if t is not None:
                ds.append(t - kd)
            kd = None
    r.metric("key_to_frame_ms", stats(ds))
    if len(ds) < n * 0.5:
        r.check(False, "only %d key->frame pairs in the app log (expected ~%d); input_order logging not on or the log path is wrong (%s)" % (len(ds), n, ctx.log_path))
        return
    p50, p99 = statistics.median(ds), pct(ds, 0.99)
    r.check(p50 <= 20, "key->frame p50 %.1f ms > 20 ms CI bound (owner target: next refresh, own work <= 2 ms; software GL here)" % p50)
    r.check(p99 <= 60, "key->frame p99 %.1f ms > 60 ms CI bound" % p99)
    ctx.reset()


# ---------------------------------------------------------------- agent
@case("approval_card_pause", modes=("agent",), order=45, gui=True)
def c_approval(ctx, r):
    """Enter pressed >1 s after an approval card appears must NOT approve it (#1350/#1362). Needs a provider."""
    if not ctx.ollama:
        r.skip("no provider: set APPCI_OLLAMA_URL (a CI variable) to run agent cases that need a model")
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
@case("tabs_two_windows", order=90, gui=True)
def c_tabs(ctx, r):
    ntabs = int(os.environ.get("APPCI_TABS", "12"))
    newwin = os.environ.get("APPCI_KEY_NEWWIN", "ctrl+shift+n")
    newtab = os.environ.get("APPCI_KEY_NEWTAB", "ctrl+shift+t")
    ctx.reset()
    for i in range(ntabs):
        ctx.be.key(newtab)
        time.sleep(0.5)
        ctx.be.type("while :; do echo streaming_%d_line; sleep 0.02; done" % i, 10, enter=True)
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
        ctx.be.type("echo %s >> tw.txt" % tok, ctx.rng.randint(10, 25), enter=True)
    ctx.be.clip_set("echo PASTED_%s >> tw.txt" % "ok")
    ctx.be.paste(enter=True)
    exp.append("PASTED_ok")
    ctx.until(lambda: len(ctx.lines("tw.txt")) >= len(exp), 30)
    r.check(ctx.lines("tw.txt") == exp, "typing/paste in window 2 under %d streaming tabs: %d/%d exact lines" % (ntabs, len(ctx.lines("tw.txt")), len(exp)))
    ctx.shot("two_windows")
    r.check(ctx.app.alive(), "app not alive at the end of the tabs/windows case")
