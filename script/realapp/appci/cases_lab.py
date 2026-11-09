# appci/cases_lab.py: the fake ssh lab cases (Wave 0.5b; script/realapp/sshlab, docs/testing.md 2.15). Registered with @lab_case, which expands one body into
# one case PER ARM: lab_<case>.<shell|scenario user>.<lan|150|300>.<nohelper|helper>[.<fault>]. They run only when --cases names them (the app-test-lab-*
# jobs pass 'lab_*'), only the arms APPCI_LAB_MATRIX selects (quick | full | spec), and need a lab (APPCI_LAB=local on the box, remote from the Mac/Windows
# runners): without one they SKIP with the reason. Before each arm the harness sets the network fault, MEASURES the echo RTT, and a mismatch makes the arm
# UNVERIFIABLE (never a pass). `nohelper` (no remote helper on the server) is the default and always the first arm; `helper` pre-installs the binary named by
# APPCI_LAB_HELPER_BIN (the lab blocks outbound network, so the app's own install cannot download one) with never_install, and is UNVERIFIABLE without it.
#
# [hook] marks a check that rests on an app log line the Wave 0 code adds. The patterns below are the agreed names; until the app logs them the check is
# UNVERIFIABLE (r.unverifiable: status noref), never a pass.
import os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases  # noqa: E402  (cases.py imports this module at its end)
import cases_caseremote as CR  # noqa: E402
import cases_coverage_a as CA  # noqa: E402
import lab as L  # noqa: E402

HOOK_TAB_INSERT = re.compile(r"hop=tab_insert\b|completion_inserted")           # [hook] the completion text reached the editor
HOOK_TAB_MENU = re.compile(r"hop=tab_menu\b[^\n]*items=(\[[^\n]*\])")             # [hook] the ambiguous-prefix menu and its items
HOOK_PATH_DETECT = re.compile(r"hop=path_detect\b[^\n]*path=(\S+)")             # [hook] a typed path was recognised as a directory on the remote
SESSION_ABANDONED = "Session abandoned before bootstrap"


def lab_case(name, shells=("bash", "zsh", "fish"), lat=("lan", "150"), helper=("nohelper",), faults=(), scenarios=(), flavor="full", modes=("terminal", "agent")):
    """register the body `fn(ctx, r, arm)` once per arm of the declared matrix. `shells`/`lat`/`helper` are the FULL matrix the case supports; APPCI_LAB_MATRIX
    narrows it at run time (run.py). `faults`: network faults that get their own arm (bash/zsh, LAN and 150 only). `scenarios`: scenario users (bash)."""
    def deco(fn):
        for arm in L.expand_arms(name, shells, lat, helper, faults, scenarios):
            arm = dict(arm, flavor=flavor)
            cases.CASES.append(dict(name=arm["name"], modes=modes, gui=True, order=700 + 100 * L.HELPERS.index(arm["helper"]), fn=_runner(fn, arm), lab=True, arm=arm))
        return fn
    return deco


def _runner(fn, arm):
    def run(ctx, r):
        if not lab_prepare(ctx, r, arm):
            return
        try:
            fn(ctx, r, arm)
        finally:
            lab_cleanup(ctx, arm)
    run.__name__ = arm["name"]
    return run


def lab_prepare(ctx, r, arm):
    """True when the arm may run. Skips (no lab / wrong image), or marks the arm UNVERIFIABLE and returns False (helper binary missing, RTT gate mismatch)."""
    lab = getattr(ctx, "lab", None)
    if lab is None:
        r.skip(getattr(ctx, "lab_why", None) or "no fake ssh lab for this pass (APPCI_LAB, docs/testing.md 2.15)")
    if arm.get("flavor") and lab.info.get("flavor") not in (None, arm["flavor"]):
        r.skip("needs the %s lab image; this pass started the %s one" % (arm["flavor"], lab.info.get("flavor")))
    ctx._lab_prev = (ctx.remote, ctx.prof.remote)          # restored by lab_cleanup: a non-lab case in a mixed run must read the same-machine remote
    ctx.arm = arm
    rem = L.LabRemote(lab, arm["user"], arm["shell"])
    ctx.remote = ctx.prof.remote = rem
    helper_bin = os.environ.get("APPCI_LAB_HELPER_BIN", "")
    if arm["helper"] == "helper":
        if not (helper_bin and os.path.isfile(helper_bin)):
            r.unverifiable("helper arm: APPCI_LAB_HELPER_BIN does not name a built remote-server binary (the lab blocks outbound network, so the app cannot download one)")
            return False
        lab.helper(arm["user"], {"remote-server": open(helper_bin, "rb").read()})
    else:
        lab.helper(arm["user"])
    if arm["helper"] != ctx.lab_helper_mode:
        if not CR.set_ext_mode(ctx, "never_install"):          # relaunch only when the helper mode changes (arms are ordered helper-major)
            r.skip("the app could not be relaunched for the helper mode change")
        ctx.lab_helper_mode = arm["helper"]
    lab.fault(rtt_ms=arm["lat_ms"]) if arm["lat_ms"] else lab.clear_fault()
    gate = L.rtt_gate_verdict(lab.measure_rtt(15), arm["lat_ms"])
    r.metric("rtt_gate", gate)
    if gate["verdict"] != "ok":
        r.unverifiable("RTT gate %s: measured median %s ms for a target of %s ms (tolerance %s); the arm is not run, a latency arm that cannot prove its network proves nothing" % (
            gate["verdict"], gate["median"], gate["target"], gate.get("tol", L.RTT_TOL_MS)))
        lab.clear_fault()
        return False
    try:
        lab.clock_offset()
    except L.LabError:
        pass
    return True


def lab_cleanup(ctx, arm):
    try:
        ctx.lab.clear_fault()
    except Exception:
        pass
    ctx.arm = None
    prev = getattr(ctx, "_lab_prev", None)
    if prev:
        ctx.remote, ctx.prof.remote = prev
        ctx._lab_prev = None


# ------------------------------------------------------------------ helpers shared by the cases
def host_of(arm):
    return "lab-" + arm["user"]


def work(arm, name=""):
    return "/work/%s/%s" % (arm["user"], name)


def connect_or_fail(ctx, r, arm, label=""):
    secs = CR.connect(ctx, host_of(arm), 90 if arm["lat_ms"] < 300 else 150)
    r.check(secs is not None, "%sno remote prompt after `ssh %s`. Test host: %s" % (label, host_of(arm), ctx.ssh_diag() if secs is None else ""))
    return secs


def finish_remote(ctx, r, label=""):
    """every lab case ends with the pane provably LOCAL"""
    ok = CR.leave(ctx)
    r.check(ok, "%s`exit` did not return to the local shell (%s)" % (label, ctx.left_unproven))
    ctx.reset()
    return ok


def rec_ts(ctx, tok):
    """epoch (REMOTE clock) of the first rec.log entry containing tok, or None"""
    for ts, cmd in ctx.remote.commands():
        if tok in cmd:
            return ts
    return None


def rtt_s(arm):
    return arm["lat_ms"] / 1000.0


def type_pace(ctx):
    return ctx.rng.randint(10, 25)


def unique(ctx, n=6):
    return ctx.tok(n).lower().replace("_", "")


# ------------------------------------------------------------------ 1. Tab completion of a remote directory
@lab_case("tab_remote_dir", shells=("bash", "zsh", "fish"), lat=("lan", "150", "300"), helper=("nohelper", "helper"),
          scenarios=("p_banner", "p_colon", "p_question", "p_slowrc", "p_rcerr"))
def c_tab_remote_dir(ctx, r, arm):
    """A unique prefix + Tab completes a REMOTE directory (proof: the completed `cd` really ran there: `&& pwd >> file` writes the exact directory); an ambiguous
    prefix offers the shell's OWN candidates (the lab's /complete runs bash-completion / zpty / fish `complete -C`; the app's menu [hook] must equal it);
    Tab -> insert <= 1 RTT + 100 ms [hook]; and with Enter 30 ms after Tab the COMPLETED line runs (the Tab+Enter rule, owner 2026-10-31)."""
    lab, u, base, rt = ctx.lab, arm["user"], work(arm), rtt_s(arm)
    uid = unique(ctx)
    fx = "tabfx_" + uid
    lab.tree(u, {"%s/zzq%s_only/x" % (fx, uid): "", "%s/amb%s_a/x" % (fx, uid): "", "%s/amb%s_b/x" % (fx, uid): ""})
    CR.rm_remote(ctx, "tab.txt", "tab2.txt")
    if connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    time.sleep(1.0 + 2 * rt)
    # (a) unique prefix
    off = ctx.log_size()
    ctx.be.type("cd %s%s/zzq%s" % (base, fx, uid), type_pace(ctx))
    t0 = time.time()
    ctx.be.key("Tab")
    hit = ctx.until(lambda: HOOK_TAB_INSERT.search(ctx.log_since(off)), 3 + 3 * rt, step=0.02)
    if hit:
        took = time.time() - t0
        r.metric("tab_insert_s", round(took, 3))
        r.check(took <= rt + 0.1 + 0.05, "Tab -> insert took %.0f ms; the budget is 1 RTT (%d ms) + 100 ms (+50 ms poll slack)" % (took * 1000, arm["lat_ms"]))
    else:
        r.unverifiable("[hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not measured until the Wave 0 code logs it")
        time.sleep(0.5 + 2 * rt)
    ctx.be.type(" && pwd >> %stab.txt" % base, type_pace(ctx), enter=True)
    got = ctx.until(lambda: ctx.remote.lines("tab.txt"), 10 + 6 * rt)
    CA.exact_lines(r, "unique prefix + Tab: the remote directory the completed `cd` entered", ["%s%s/zzq%s_only" % (base, fx, uid)], got)
    ctx.shot("lab_tab_unique")
    # (b) ambiguous prefix: the shell's own candidates vs the app's menu
    line = "cd %s%s/amb%s" % (base, fx, uid)
    comp = lab.complete(u, line)
    cand = comp.get("candidates")
    if cand is None:
        r.unverifiable("/complete has no driver for %s: the shell's own candidate list is not compared" % arm["shell"])
    else:
        names = {c.rstrip("/").rsplit("/", 1)[-1] for c in cand}
        r.check({"amb%s_a" % uid, "amb%s_b" % uid} <= names, "the lab's own %s candidates for %r are %s, expected both amb%s_a and amb%s_b" % (arm["shell"], line, sorted(names), uid, uid))
    off = ctx.log_size()
    ctx.be.type(line, type_pace(ctx))
    ctx.be.key("Tab")
    m = ctx.until(lambda: HOOK_TAB_MENU.search(ctx.log_since(off)), 3 + 3 * rt, step=0.05)
    ctx.shot("lab_tab_menu")
    if m and cand is not None:
        shown = {x.strip(" \"'").rstrip("/").rsplit("/", 1)[-1] for x in m.group(1).strip("[]").split(",") if x.strip()}
        r.check(shown == names, "the app's completion menu shows %s, the shell's own candidates are %s" % (sorted(shown), sorted(names)))
    elif not m:
        r.unverifiable("[hook] no tab-menu line in the app log (hop=tab_menu items=[...]): the menu is not compared with the shell's candidates")
    ctx.reset()
    # (c) FAST ENTER: Enter 30 ms after Tab runs the COMPLETED line
    n0 = len(ctx.remote.commands())
    ctx.be.type("cd %s%s/zzq%s" % (base, fx, uid), type_pace(ctx))
    ctx.be.key("Tab")
    time.sleep(0.03)
    ctx.be.key("Return")
    time.sleep(0.5 + 3 * rt)
    ctx.be.type("pwd >> %stab2.txt" % base, type_pace(ctx), enter=True)
    got2 = ctx.until(lambda: ctx.remote.lines("tab2.txt"), 10 + 6 * rt)
    CA.exact_lines(r, "Tab then Enter 30 ms later: the directory the shell entered (the COMPLETED line ran)", ["%s%s/zzq%s_only" % (base, fx, uid)], got2)
    recs = [c for _, c in ctx.remote.commands()[n0:] if c.startswith("cd ")]
    if recs:
        r.check(any(("zzq%s_only" % uid) in c for c in recs), "the shell received %r, not the completed directory" % recs)
    finish_remote(ctx, r)


# ------------------------------------------------------------------ 2. path detection while typing
@lab_case("path_detect", shells=("bash", "zsh", "fish"), lat=("lan", "150", "300"), helper=("nohelper", "helper"), scenarios=("p_exec", "p_tinypath"))
def c_path_detect(ctx, r, arm):
    """A directory path typed at the remote prompt (no Enter) is detected [hook], and typing it opens ZERO new exec channels on the server: the no-helper path must
    not run a command per keystroke (the sshd log counts 'Starting session: command' lines)."""
    lab, u, base, rt = ctx.lab, arm["user"], work(arm), rtt_s(arm)
    uid = unique(ctx)
    lab.tree(u, {"pd%s/a.txt" % uid: "", "pd%s/sub/b.txt" % uid: ""})
    if connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    time.sleep(2.0 + 3 * rt)                         # the connection's own start-up channels settle first
    size0 = lab.sshd_since(0)["size"]
    off = ctx.log_size()
    ctx.be.type("ls %spd%s/sub/" % (base, uid), 30)
    time.sleep(1.5 + 3 * rt)
    hit = HOOK_PATH_DETECT.search(ctx.log_since(off))
    ctx.shot("lab_path_detect")
    if hit:
        r.check("pd%s/sub" % uid in hit.group(1), "the app detected %r, expected the typed directory pd%s/sub" % (hit.group(1), uid))
    else:
        r.unverifiable("[hook] no path-detect line in the app log (hop=path_detect path=...): detection itself is not proven; only the channel count below is checked")
    starts = L.count_starts(lab.sshd_since(size0)["data"])
    r.metric("exec_channels_while_typing", starts)
    r.check(starts["command"] == 0 and starts["subsystem"] == 0, "typing a remote path opened %d exec and %d subsystem channel(s) on the server (expected none)" % (starts["command"], starts["subsystem"]))
    ctx.reset()
    finish_remote(ctx, r)


# ------------------------------------------------------------------ 3. git chips
@lab_case("chips", shells=("bash", "zsh"), lat=("lan", "150"), helper=("nohelper", "helper"))
def c_chips(ctx, r, arm):
    """In a git directory on the remote the branch chip is on screen (OCR) and the app made at most ONE batched side call per prompt (exec channels in the sshd
    log; with the helper it may make none)."""
    lab, u, base, rt = ctx.lab, arm["user"], work(arm), rtt_s(arm)
    uid = unique(ctx)
    br = "br" + uid          # a unique branch name: the chip OCR looks for THIS token, not for "main" (which any other text on screen could contain)
    lab.tree(u, {"README": "x\n", "src/a.rs": "fn main(){}\n"}, dir="chips" + uid, git=True, branch=br, branches=("feature-x",))      # <work>/chips<uid> is the git repo
    if connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    time.sleep(1.0 + 2 * rt)
    size0 = lab.sshd_since(0)["size"]
    p0 = ctx.remote.prompts()
    ctx.be.type("cd %schips%s" % (base, uid), type_pace(ctx), enter=True)
    seen, txt = False, None
    for i in range(6):
        time.sleep(1.0 + 2 * rt)
        txt = ctx.ocr("lab_chips_%d" % i)
        if txt is None:
            break
        if br in txt:
            seen = True
            break
    if txt is None:
        r.unverifiable("no OCR on this machine (tesseract): the branch chip on screen is not checked")
    else:
        r.check(seen, "no branch chip (branch %s) on screen after cd into a git directory on the remote" % br)
    prompts = max(1, ctx.remote.prompts() - p0)
    calls = L.count_starts(lab.sshd_since(size0)["data"])["command"]
    r.metric("side_calls", calls)
    r.metric("prompts", prompts)
    r.check(calls <= prompts, "%d side-call exec channels for %d prompt(s): the app must batch to at most one per prompt" % (calls, prompts))
    if arm["helper"] == "nohelper":
        r.check(calls >= 1, "no side call at all with no helper: where did the branch chip come from?")
    finish_remote(ctx, r)


# ------------------------------------------------------------------ 4. Enter -> command start, paired with plain ssh
@lab_case("enter_start", shells=("bash", "zsh", "fish", "nu", "pwsh", "dash", "ash"), lat=("lan", "150", "300"), helper=("nohelper", "helper"),
          scenarios=("p_sete", "p_colon"))
def c_enter_start(ctx, r, arm):
    """20 lines typed at 10-25 ms/key with an instant Enter, interleaved with the same lines typed into a PLAIN `ssh -tt` session (same lab user, same path):
    median(app) <= median(plain) + max(15 ms, 10 %) (owner 2026-11-08; the +-1 ms budget is for LAN key->echo only). Time = the shell's own record of the line
    (rec.log, remote clock mapped by /health's clock) minus the Enter. The effect file proves every line arrived exactly once, in order. The Enter is stamped just before the
    Return key event (the plain control: just before its CR write), so the injector's own spawn time counts against the app."""
    lab, u, rt = ctx.lab, arm["user"], rtt_s(arm)
    n = ctx.count_lat(20)
    eff = work(arm, "es_%s.txt" % unique(ctx, 4))
    shell_kind = arm["shell"] if arm["shell"] in ("nu", "pwsh") else "bash"
    plain = L.PlainSsh(os.path.join(ctx.prof.home, ".ssh", "config"), u)
    plain_up = False
    try:
        plain_up = plain.open(30 + 5 * rt) if not ctx.win else False
    except Exception as e:  # noqa: BLE001
        r.note("plain ssh control could not start: %r" % (e,))
    if not plain_up:
        r.no_reference("no plain ssh control (%s): the paired verdict cannot be given" % ("Windows" if ctx.win else "its session did not start"))
    if connect_or_fail(ctx, r, arm) is None:
        plain.close()
        ctx.reset()
        return
    time.sleep(1.0 + 2 * rt)
    off = lab.offset or 0.0
    app_ms, plain_ms, app_lines, app_toks = [], [], [], []
    timed = arm["shell"] not in ("fish", "dash", "ash")          # fish_rec.log has whole-second stamps; dash/ash have no history hook
    try:
        for i in range(n):
            tok = "es" + unique(ctx, 5)
            line = ctx.append_cmd(shell_kind, tok, eff)
            ctx.be.type(line, type_pace(ctx))
            t_enter = time.time()          # stamped BEFORE the Return event, like the plain control's CR write; the injector's spawn time then counts against the app
            ctx.be.key("Return")
            app_lines.append(line)
            app_toks.append(tok)
            if timed:
                ts = ctx.until(lambda: rec_ts(ctx, tok), 10 + 4 * rt, step=0.01)
                app_ms.append((ts - off - t_enter) * 1000.0 if ts else None)
            else:
                ctx.until(lambda: tok in ctx.remote.lines(eff), 10 + 4 * rt, step=0.01)
            if plain_up:
                ptok = "ps" + unique(ctx, 5)
                pline = ctx.append_cmd(shell_kind, ptok, work(arm, "es_plain.txt"))
                pt = plain.run_line(pline, rng=ctx.rng)
                if timed:
                    pts = ctx.until(lambda: rec_ts(ctx, ptok), 10 + 4 * rt, step=0.01)
                    plain_ms.append((pts - off - pt) * 1000.0 if pts else None)
                else:
                    ctx.until(lambda: ptok in ctx.remote.lines("es_plain.txt"), 10 + 4 * rt, step=0.01)
            time.sleep(0.05)
    finally:
        plain.close()
    ctx.until(lambda: len(ctx.remote.lines(eff)) >= n, 10 + 4 * rt)
    CA.exact_lines(r, "effect file: every typed line arrived once, in order", app_toks, [x.strip() for x in ctx.remote.lines(eff)])
    if timed:
        recs = [c for _, c in ctx.remote.commands() if any(t in c for t in app_toks)]
        CA.exact_lines(r, "rec.log: the shell received exactly the typed lines", app_lines, recs)
    pv = L.paired_verdict(app_ms, plain_ms) if timed and plain_up else None
    r.metric("enter_start_app_ms", app_ms)
    r.metric("enter_start_plain_ms", plain_ms)
    r.metric("paired_verdict", pv)
    if pv is None:
        r.unverifiable("no paired timing for %s (%s)" % (arm["shell"], "no timestamped history hook for this shell" if not timed else "no plain control"))
    elif pv["verdict"] == "no-data":
        r.unverifiable("paired timing has too few samples: %s" % pv)
    else:
        r.check(pv["verdict"] == "pass", "Enter -> command start: app median %.1f ms vs plain ssh %.1f ms; allowed %.1f ms (over by %.1f ms)" % (pv["app"], pv["plain"], pv["allowed"], pv["over_ms"]))
    finish_remote(ctx, r)


# ------------------------------------------------------------------ 5. dropped / reset / stalled connections
@lab_case("ssh_drop_reconnect", modes=("terminal",), shells=("bash", "zsh"), lat=("lan", "150"), helper=("nohelper", "helper"), faults=("rst", "stall10"))
def c_lab_drop(ctx, r, arm):
    """The ssh_drop_reconnect body (the escape, escape-at-connect with and without Enter, kill -9 of the remote shell) against the lab, plus two network faults
    the same-machine sshd cannot make: the server RESETS the connection (rst: the pane must return to a local prompt, a reconnect must work) and the server STALLS
    for 10 s (SIGSTOP of its session: the pane stays on the remote and the lines typed meanwhile arrive exactly, in order)."""
    if ctx.win:
        r.skip("not yet ported to the Windows panes (see ssh_drop_reconnect)")
    host = host_of(arm)
    if arm["fault"] is None:
        CA.ssh_drop_body(ctx, r, host, ("escape", "escape_connect_enter", "escape_connect_bare", "kill"))
    elif arm["fault"] == "rst":
        CA.ssh_drop_body(ctx, r, host, ("rst",))
    else:
        rt = rtt_s(arm)
        f = "lab_stall.txt"
        CR.rm_remote(ctx, f)
        if connect_or_fail(ctx, r, arm) is None:
            ctx.reset()
            return
        time.sleep(1.0 + 2 * rt)
        toks = [unique(ctx, 8) for _ in range(3)]
        ctx.lab.drop(arm["user"], "stall", 10)
        time.sleep(0.5)
        CR.typed_lines(ctx, ["echo %s >> ~/%s" % (t, f) for t in toks])
        ctx.until(lambda: len(ctx.remote.lines(f)) >= 3, 30 + 6 * rt)
        CA.exact_lines(r, "lines typed during a 10 s server stall", toks, ctx.remote.lines(f))
        ctx.shot("lab_stall_after")
        r.check(ctx.where_now() == "remote", "the pane left the remote shell during a 10 s stall (a stall is not a drop)")
        finish_remote(ctx, r)


# ------------------------------------------------------------------ 6. Ctrl-C bursts (#1798 / #1672)
@lab_case("ctrl_c_no_esc_i", shells=("bash", "zsh", "fish"), lat=("lan", "150"), helper=("nohelper", "helper"))
def c_ctrl_c_no_esc_i(ctx, r, arm):
    """#1798/#1672: `sleep 30`, then Ctrl-C x1/x2/x3 at gaps 0/50/200 ms, then a typed echo. Per round: the effect file is exact, the shell received exactly two
    commands (the sleep and the echo), the prompt log grew by one per command (bash exact; zsh at least), no ESC-i reached the shell (rec.log has no \\x1bi),
    and the app log never says "Session abandoned before bootstrap". One OCR at the end looks for a visible ^[i."""
    lab, rt = ctx.lab, rtt_s(arm)
    if connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    time.sleep(1.0 + 2 * rt)
    f = "cc_%s.txt" % unique(ctx, 4)
    rem, toks = ctx.remote, []
    for nctl in (1, 2, 3):
        for gap in (0, 50, 200):
            label = "x%d gap %d ms" % (nctl, gap)
            off = ctx.log_size()
            c0, p0 = len(rem.commands()), rem.prompts()
            ctx.be.type("sleep 30", type_pace(ctx), enter=True)
            time.sleep(0.8 + 2 * rt)
            for i in range(nctl):
                ctx.be.key("ctrl+c")
                if i < nctl - 1 and gap:
                    time.sleep(gap / 1000.0)
            if arm["shell"] != "fish":
                back = ctx.until(lambda: rem.prompts() > p0, 10 + 3 * rt, step=0.05)
                r.check(bool(back), "[%s] no prompt after the Ctrl-C burst" % label)
            else:
                time.sleep(1.0 + 2 * rt)
            tok = unique(ctx, 8)
            toks.append(tok)
            ctx.be.type("echo %s >> %s" % (tok, work(arm, f)), type_pace(ctx), enter=True)
            ctx.until(lambda: len(rem.lines(f)) >= len(toks), 10 + 4 * rt)
            CA.exact_lines(r, "[%s] effect file" % label, toks, rem.lines(f))
            time.sleep(0.3 + rt)
            new = rem.commands()[c0:]
            r.check(len(new) == 2 and new[0][1].strip() == "sleep 30" and tok in new[-1][1], "[%s] the shell received %r, expected exactly [sleep 30, the echo]" % (label, [c for _, c in new]))
            r.check(not any("\x1bi" in c or "^[i" in c for _, c in new), "[%s] an ESC-i (^[i) reached the shell: %r" % (label, [c for _, c in new]))
            if arm["shell"] == "bash":
                r.check(rem.prompts() - p0 == 2, "[%s] prompt.log grew by %d, expected one per command (2)" % (label, rem.prompts() - p0))
            elif arm["shell"] == "zsh":
                r.check(rem.prompts() - p0 >= 2, "[%s] prompt.log grew by %d, expected at least one per command (2)" % (label, rem.prompts() - p0))
            r.check(SESSION_ABANDONED not in ctx.log_since(off), "[%s] the app log says %r" % (label, SESSION_ABANDONED))
    txt = ctx.ocr("lab_ctrlc_end")
    if txt is None:
        r.note("no OCR: a visible ^[i on screen was not looked for")
    else:
        r.check("^[i" not in txt and "^[ i" not in txt, "a literal ^[i is on screen after the Ctrl-C rounds")
    ctx.shot("lab_ctrlc_end")
    finish_remote(ctx, r)
