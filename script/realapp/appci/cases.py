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
SMOKE = ("mode_proof", "key_loss_control", "cadence", "fast_typing", "paste_instant_enter", "tab_enter", "ctrl_c_d", "ssh_connect_typing", "latency_enter", "latency_key_log", "latency_paint", "md_viewer")
SMOKE = ("mode_proof", "key_loss_control", "cadence", "fast_typing", "paste_instant_enter", "tab_enter", "ctrl_c_d", "ssh_connect_typing", "shell_fish", "shell_nu", "latency_enter", "latency_key_log", "latency_paint")


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


def plat():
    return os.environ.get("APPCI_PLATFORM", "linux")


def lat_checks(r, rec, label):
    """turn a latency record's verdicts into case failures (FAIL only; warn/no-reference stay visible in the table + notes)"""
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
        r.note("%s: no reference terminal measured in this session, overhead verdict 'no-reference' (absolute numbers recorded)" % label)
    for n_ in rec.get("notes", []):
        r.note("%s: %s" % (label, n_))


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
    ctx.be.key("Escape")
    time.sleep(0.8)
    ctx.shot("proof")
    if txt is None:
        r.note("no OCR engine: screenshot only (shots/%s_proof_prekey.png), mode not machine-verified" % ctx.mode)
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


def ctrlc_samples(ctx, n, fname):
    """n x (start a trap-instrumented sleep, press Ctrl-C, read the trap's timestamp) -> (latencies ms, all delivered)"""
    ctx.rm(fname)
    lat_ms = []
    for i in range(n):
        now = "now" if ctx.be.name == "mac" else "date +%s.%N"     # BSD date has no %N; ~/bin/now is written by profile.py
        ctx.be.type("bash -c 'trap \"" + now + " >> %s; exit\" INT; sleep 30'; echo AFTER%d >> %s" % (fname, i, fname), 10, enter=True)
        time.sleep(1.2)
        t0 = time.time()
        ctx.be.key("ctrl+c")
        ctx.until(lambda: sum(1 for l in ctx.lines(fname) if l.startswith("AFTER")) > i, 5)
        time.sleep(0.2)
        ts = [float(l) for l in ctx.lines(fname) if re.fullmatch(r"\d+\.\d+", l)]
        if len(ts) > len(lat_ms):
            lat_ms.append((ts[len(lat_ms)] - t0) * 1000)
    after = [l for l in ctx.lines(fname) if l.startswith("AFTER")]
    return lat_ms, len(lat_ms) == n and len(after) == n


@case("ctrl_c_d", order=17)
def c_ctrl(ctx, r):
    ctx.rm("cd.txt", "cd2.txt")
    n, warm = ctx.count(30), lat.warmup()
    lat_ms, ok1 = ctrlc_samples(ctx, n + warm, "cc.txt")
    lat_ms = lat_ms[warm:]
    r.metric("ctrl_c_ms", stats(lat_ms))
    r.metric("harness_floor_ms", ctx.be.key_floor_ms)
    r.check(ok1, "Ctrl-C / command-after-Ctrl-C delivered fewer than %d times" % (n + warm))
    if lat_ms:
        # owner budget: Ctrl-C <= 20 ms; the measurement includes the input tool's own spawn, which is subtracted (the harness floor).
        # Absolute budget (no reference terminal), so it is judged on the number: pass/fail, not 'no-reference'.
        rec = lat.evaluate("%s/ctrl_c_d/ctrl_c" % ctx.mode, lat_ms, None, float(os.environ.get("APPCI_CTRLC_MAX_MS", "20")), "absolute", plat(),
                           remeasure=lambda: ctrlc_samples(ctx, n + warm, "cc2.txt")[0][warm:], floor=ctx.be.key_floor_ms)
        r.metrics.setdefault("lat", {})["ctrl_c"] = rec
        lat_checks(r, rec, "Ctrl-C")
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


@case("ssh_enter_then_immediate_command", order=30)
def c_ssh_enter_immediate(ctx, r):
    """#1415: `ssh <host>` Enter immediately followed by `echo <tok>` Enter, no pause at all. The
    second line is typed while the first Enter still waits at its gate; released into the new
    connect it must be held for the remote shell (it used to be dropped with a log line)."""
    if not ctx.ssh:
        r.skip("no local ssh server (needs /usr/sbin/sshd + ssh-keygen, or python3-paramiko for fakesshd.py)")
    trials = int(os.environ.get("APPCI_SSH_TRIALS", "3"))
    try:
        os.remove(os.path.join(ctx.prof.rhome, "rh.txt"))
    except FileNotFoundError:
        pass
    ctx.rm("rh.txt")
    exp = []
    for _ in range(trials):
        tok = ctx.tok(6)
        exp.append(tok)
        ctx.be.type("ssh fake", 12, enter=True)
        ctx.be.type("echo %s >> ~/rh.txt" % tok, 12, enter=True)
        ctx.until(lambda: tok in ctx.lines("rh.txt", remote=True), 20)
        before = ctx.prompts()
        ctx.be.type("exit", 12, enter=True)
        ctx.until(lambda: ctx.prompts() > before, 10)
        time.sleep(0.5)
    got = ctx.lines("rh.txt", remote=True)
    r.check(got == exp, "remote shell received %d/%d immediately-typed commands exactly: %r" % (sum(1 for t in exp if t in got), len(exp), got))
    r.check(not ctx.lines("rh.txt"), "immediately-typed command ran LOCALLY: %r" % ctx.lines("rh.txt"))
    ctx.reset()


@case("ssh_nested_typeahead", order=30)
def c_ssh_nested(ctx, r):
    """#1421: `ssh fake` Enter, then `ssh fake` Enter and `echo <tok>` Enter typed during that first
    connect. At the hand-over the nested ssh is submitted; the echo behind it is held for the
    SECOND connect, so it runs exactly once, remotely, and never locally."""
    if not ctx.ssh:
        r.skip("no local ssh server (needs /usr/sbin/sshd + ssh-keygen, or python3-paramiko for fakesshd.py)")
    try:
        os.remove(os.path.join(ctx.prof.rhome, "rn.txt"))
    except FileNotFoundError:
        pass
    ctx.rm("rn.txt")
    tok = ctx.tok(6)
    ctx.be.type("ssh fake", 12, enter=True)
    ctx.be.type("ssh fake", 12, enter=True)
    ctx.be.type("echo %s >> ~/rn.txt" % tok, 12, enter=True)
    ctx.until(lambda: tok in ctx.lines("rn.txt", remote=True), 30)
    time.sleep(1.0)
    got = ctx.lines("rn.txt", remote=True)
    r.check(got == [tok], "the line typed behind a nested ssh ran exactly once remotely: %r" % got)
    r.check(not ctx.lines("rn.txt"), "the line typed behind a nested ssh ran LOCALLY: %r" % ctx.lines("rn.txt"))
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


def ref_terminal(ctx):
    """Linux: foot (Wayland) when installed, else xterm (via XWayland under the Wayland session, native under X11).
    macOS: Terminal.app, driven by the same cliclick/osascript path as Phosphor. None when there is no reference."""
    home = ctx.prof.home
    if ctx.be.name == "mac":
        # NO Apple Events: `tell application "Terminal" ... do script` needs an Automation grant (a TCC prompt nobody answers in CI; it hung
        # 30 s per sample in run 3015802940). A .command file opened with LaunchServices needs none. Driven by htype2 under the frontmost guard.
        ctx.ref_reason = None
        cmdf = os.path.join(ctx.prof.home, "ref_terminal.command")
        with open(cmdf, "w") as f:
            f.write("#!/bin/zsh\ncd '%s'\nexec env HOME='%s' ZDOTDIR='%s' PA_CTL='%s' zsh -i\n" % (ctx.W, home, home, ctx.prof.ctl))
        os.chmod(cmdf, 0o755)
        subprocess.run(["xattr", "-c", cmdf], capture_output=True)
        rr = subprocess.run(["open", "-a", "Terminal", cmdf], capture_output=True, timeout=20)
        if rr.returncode != 0:
            ctx.ref_reason = "open -a Terminal failed: %s" % rr.stderr.decode().strip()[:120]
            return None
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
            # never Cmd-W/Q: the shell exits cleanly, and Terminal's default profile closes the window of a cleanly exited shell
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
    if wl and shutil.which("foot") and ctx.be.name == "wayland":
        argv, name = ["foot", "-e", "bash", "--rcfile", home + "/.bashrc", "-i"], "foot"
    elif shutil.which("xterm") and disp:
        argv = ["xterm", "-geometry", "120x30+0+0", "-e", "bash", "--rcfile", home + "/.bashrc", "-i"]
        name = "xterm (via XWayland)" if wl and ctx.be.name == "wayland" else "xterm"
    else:
        return None
    return RefTerm(name, proc=subprocess.Popen(argv, env=env, start_new_session=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))


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
            if inject.startswith("oneshot"):
                ctx.be.type_lines(want, 12, 350)
            else:
                for l in want:
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
        r.check(bad_p <= bad_q, "Phosphor lost keys the reference keeps with the SAME injection (%s): Phosphor %d/%d bad lines (first char lost %d), %s %d/%d -- a Phosphor-side drop (#1414)"
                % (inj, bad_p, n, p["lost_first"], ref.name, bad_q, n))
        if bad_q > bad_p:
            r.note("%s: the reference lost more than Phosphor (%d vs %d): injector-side loss" % (inj, bad_q, bad_p))
    # the harness's own default (primed, per-line) must deliver every line, whatever the reference does: otherwise every other case is unreliable
    d = byk.get(("phosphor", "perline"))
    if d:
        r.check(d["lost_first"] + d["other_bad"] + d["missing"] == 0, "harness default injection (per-line%s) still loses lines in Phosphor: %s"
                % (", primed" if is_wl and prime_default else "", {k: d[k] for k in ("lost_first", "other_bad", "missing")}))


def enter_to_start(ctx, n, budget_s=None):
    ds = []
    t_end = time.time() + budget_s if budget_s else None
    for i in range(n):
        if t_end and time.time() > t_end:
            break
        f = os.path.join(ctx.W, "lat_%d" % i)
        if os.path.exists(f):
            os.remove(f)
        stamp = "print -r -- $EPOCHREALTIME > lat_%d" if ctx.be.name == "mac" else "date +%%s.%%N > lat_%d"   # BSD date has no %N
        ctx.be.type(stamp % i, 12)
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
    """Enter -> command start, Phosphor vs a reference terminal in the SAME session with the SAME keys/cadence/timestamp command.
    Verdict is on OVERHEAD (Phosphor median - reference median) against the owner's 2 ms budget; both include one input-tool spawn,
    which cancels. No reference terminal -> verdict 'no-reference' (absolute numbers only). Also checked build-to-build (lat.py)."""
    n, warm = ctx.count(30), lat.warmup()
    ds = enter_to_start(ctx, n + warm)[warm:]
    r.metric("enter_to_start_ms", stats(ds))
    r.metric("harness_floor_ms", ctx.be.key_floor_ms)
    r.check(len(ds) >= n * 0.8, "only %d/%d Enter timings captured" % (len(ds), n))
    ctx.reset()
    rs, ref = [], ref_terminal(ctx)
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
        rec = lat.evaluate("%s/latency_enter/enter_to_start" % ctx.mode, ds, rs, float(os.environ.get("APPCI_ENTER_BUDGET_MS", "2")), "overhead", plat(),
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


@case("latency_key_log", order=42, gui=True)
def c_lat_key(ctx, r):
    """key -> first frame after the echo, from the app's own input_order log (keydown ... pty_write -> pty_read -> frame);
    needs RUST_LOG input_order (set by the launcher). Phosphor-side only: no paint-readback through Wayland."""
    if not ctx.log_path or not os.path.exists(ctx.log_path):
        r.skip("no app log at %s (set APPCI_APP_LOG)" % ctx.log_path)
    n, warm = ctx.count(60), lat.warmup()
    SLOW = re.compile(r"slow ([\w .:/-]+?): (\d+) ms")

    def run_keys(count):
        off = ctx.log_size()
        for i in range(count):
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
        return ds[warm:], [(m.group(1), int(m.group(2))) for m in SLOW.finditer(txt)]

    ds, slow = run_keys(n + warm)
    r.metric("key_to_frame_ms", stats(ds))
    if len(ds) < n * 0.5:
        allt = ctx.log_since(0)
        r.check(False, "only %d key->frame pairs in the app log (expected ~%d); log %s has %d input_order keydown lines (%d with ctrl=false) and %d input_order frame lines "
                "(0 keydown = logging off / wrong path / keys never reached the app; many keydown with ctrl=true = a stuck Control modifier)"
                % (len(ds), n, ctx.log_path, allt.count("input_order keydown"), allt.count("input_order keydown") - allt.count("ctrl=true"),
                   allt.count("input_order frame")))
        return
    # Phosphor-side clock only (keydown -> first frame after the echo); there is no paint read-back for a reference terminal through
    # Wayland / ScreenCaptureKit at this resolution, so the 2 ms overhead budget cannot be judged: verdict 'no-reference'.
    rec = lat.evaluate("%s/latency_key_log/key_to_frame" % ctx.mode, ds, None, float(os.environ.get("APPCI_KEY_BUDGET_MS", "2")), "overhead", plat(),
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


@case("latency_paint", order=43, gui=True)
def c_lat_paint(ctx, r):
    """key -> first changed pixel in Phosphor's window, measured with kl3 (ScreenCaptureKit, 240 Hz frame times; macOS only).
    Alternates 'm' and Delete so every key changes the screen. Reported as metrics with sanity bounds only: the verdict budgets
    live in the latency tooling (lat.py / baselines)."""
    if ctx.be.name != "mac" or not hasattr(ctx.be, "kl_start"):
        r.skip("kl3 (ScreenCaptureKit) is macOS only")
    rect = ctx.be.window_rect()
    if not rect or rect[2] < 100:
        r.skip("no Phosphor window rect from System Events")
    ctx.be.guard()
    ctx.be.kl_start(rect)
    ds = []
    try:
        for i in range(ctx.count(30)):
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
    ctx.reset()
    r.metric("key_to_paint_ms", stats(ds))
    r.check(len(ds) >= 20, "only %d key->paint samples from kl3 (capture region or Screen Recording permission wrong)" % len(ds))
    if ds:
        r.check(statistics.median(ds) <= 120, "key->paint median %.1f ms > 120 ms sanity bound" % statistics.median(ds))


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
        r.note("no OCR engine (tesseract / Apple Vision): screenshot shots/%s_md_small_noocr.png only, table text and diagram labels NOT machine-verified" % ctx.mode)
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
