# appci/cases_lab.py: the fake ssh lab cases (Wave 0.5b; script/realapp/sshlab, docs/testing.md 2.15). Registered with @lab_case, which expands one body into
# one case PER ARM: lab_<case>.<shell|scenario user>.<lan|150|300>.<nohelper|helper>[.<fault>]. They run only when --cases names them (the app-test-lab-*
# jobs pass 'lab_*'), only the arms APPCI_LAB_MATRIX selects (quick | full | spec), and need a lab (APPCI_LAB=local on the box, remote from the Mac/Windows
# runners): without one they SKIP with the reason. Before each arm the harness sets the network fault, MEASURES the echo RTT, and a mismatch makes the arm
# UNVERIFIABLE (never a pass). `nohelper` (no remote helper on the server) is the default and always the first arm; `helper` pre-installs the binary named by
# APPCI_LAB_HELPER_BIN (a built phosphor-remote; the lab blocks outbound network, so nothing could be downloaded anyway) with never_install, and is UNVERIFIABLE
# without it. The Wave H arms (#1804) use the same binary as the bundle the app uploads: `upload` (nothing on the server, always_install), `upgrade` (older
# versions pre-seeded), `never` (per-host policy never: nothing may be uploaded, the session still works). Scenario user p_noscp refuses scp/sftp.
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
HOOK_CWD_LISTING = re.compile(r"hop=cwd_listing session=(\d+) kind=([LPT]) entries=(\d+)")            # the remote shell's pushed cwd listing arrived (#1801): ids and counts only
HOOK_PATH_DETECT = re.compile(r"hop=path_detect source=pushed session=(\d+) entries=(\d+) complete=(true|false)")   # path detection answered from that listing, no round trip (ids and counts only)
SESSION_ABANDONED = "Session abandoned before bootstrap"


def lab_case(name, shells=("bash", "zsh", "fish", "nu"), lat=("lan", "150"), helper=("nohelper",), faults=(), scenarios=(), flavor="full", modes=("terminal", "agent")):
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
        # lab_prepare points ctx.remote at the lab arm BEFORE its gates (helper binary, RTT, relaunch), and each gate leaves by `return False` or by a
        # skip/unverifiable exception. Job 17885063245: all eight `.helper` arms left that way (no APPCI_LAB_HELPER_BIN) with the swap still in place, and
        # the runner's leak check reported each as `harness_state_reset_after_<case>` FAIL. The cleanup therefore covers lab_prepare too.
        try:
            if not lab_prepare(ctx, r, arm):
                return
            fn(ctx, r, arm)
        finally:
            lab_cleanup(ctx, arm)
    run.__name__ = arm["name"]
    return run


# helper arm -> (ssh_extension_install_mode, the helper must be offered to the app as its bundled asset)
HELPER_ARM_MODE = {"nohelper": "never_install", "helper": "never_install", "upload": "always_install", "upgrade": "always_install", "never": "always_install"}
HELPER_ASSET_ENV = "PHOSPHOR_REMOTE_HELPER_ASSET_DIR"           # remote_server::setup::HELPER_ASSET_DIR_ENV
SEED_VERSIONS = ("phosphor-remote-v0.0.1", "phosphor-remote-v0.0.2", "phosphor-oss", "phosphor-oss-v0.0.3")   # what an `upgrade` arm finds on the server


def helper_version_line(binary):
    """first line of `<binary> --version`: `phosphor-remote <version> protocol <n>`; None when it does not run"""
    import subprocess
    try:
        out = subprocess.run([binary, "--version"], capture_output=True, text=True, timeout=20, stdin=subprocess.DEVNULL)
    except (OSError, subprocess.SubprocessError):
        return None
    line = (out.stdout.splitlines() or [""])[0]
    return line if out.returncode == 0 and line.startswith("phosphor-remote ") else None


def helper_file_name(version_line):
    """the file the app installs for that helper: phosphor-remote-<version> (setup::helper_file_name)"""
    return "phosphor-remote-" + version_line.split()[1]


def host_label(arm):
    """the key the app stores a host's helper policy under (connection_label_for_session_info: user@host)"""
    return "%s@%s" % (arm["user"], host_of(arm))


def stage_helper_asset(ctx, helper_bin):
    """gzip the helper into <run>/helper-asset/phosphor-remote-linux-<arch>.gz and point the app at it (the env is read at launch). Returns the directory."""
    import gzip, platform, shutil
    arch = {"x86_64": "x86_64", "amd64": "x86_64", "aarch64": "aarch64", "arm64": "aarch64"}.get(platform.machine().lower(), "x86_64")
    d = os.path.join(ctx.run, "helper-asset")
    os.makedirs(d, exist_ok=True)
    with open(helper_bin, "rb") as src, gzip.open(os.path.join(d, "phosphor-remote-linux-%s.gz" % arch), "wb", 9) as dst:
        shutil.copyfileobj(src, dst)
    os.environ[HELPER_ASSET_ENV] = d
    return d


def server_helper_files(lab, arm):
    """the phosphor-* file names in the lab user's ~/.phosphor/remote-server (one-shot ssh as that user), sorted; None when the listing failed"""
    try:
        out = lab.ssh(arm["user"], "ls -1A ~/.phosphor/remote-server 2>/dev/null")
    except Exception:
        return None
    return sorted(n for n in out.stdout.split() if n.startswith("phosphor-"))


def set_denied_hosts(ctx, hosts):
    """warpify.ssh.remote_helper_denied_hosts in the profile's settings.toml (read when the app starts)"""
    cfg = ctx.prof.home + "/.config/phosphor/settings.toml"
    t = open(cfg).read()
    arr = "[" + ", ".join('"%s"' % h for h in hosts) + "]"
    open(cfg, "w").write(CR.T.set_key(t, "warpify.ssh", "remote_helper_denied_hosts", arr))


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
    if arm["helper"] != "nohelper":
        if not (helper_bin and os.path.isfile(helper_bin)):
            r.unverifiable("%s arm: APPCI_LAB_HELPER_BIN does not name a built phosphor-remote binary" % arm["helper"])
            return False
        line = helper_version_line(helper_bin)
        if line is None:
            r.unverifiable("%s arm: `%s --version` did not print `phosphor-remote <version> protocol <n>`; APPCI_LAB_HELPER_BIN must be a built phosphor-remote" % (arm["helper"], helper_bin))
            return False
        arm["helper_file"] = helper_file_name(line)
    if arm["helper"] == "helper":
        # already-installed state; the file name carries the version, which must be the one the app under test computes (built from the same tree)
        lab.helper(arm["user"], {arm["helper_file"]: open(helper_bin, "rb").read()})
    elif arm["helper"] == "upgrade":
        lab.helper(arm["user"], {n: b"#!/bin/sh\nexit 0\n" for n in SEED_VERSIONS})
    else:
        lab.helper(arm["user"])
    if arm["helper"] in ("upload", "upgrade", "never"):
        stage_helper_asset(ctx, helper_bin)
    elif HELPER_ASSET_ENV in os.environ:
        del os.environ[HELPER_ASSET_ENV]
    if arm["helper"] != ctx.lab_helper_mode:
        set_denied_hosts(ctx, [host_label(arm)] if arm["helper"] == "never" else [])
        if not CR.set_ext_mode(ctx, HELPER_ARM_MODE[arm["helper"]]):          # relaunch only when the helper mode changes (arms are ordered helper-major)
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
@lab_case("tab_remote_dir", shells=("bash", "zsh", "fish", "nu"), lat=("lan", "150", "300"), helper=("nohelper", "helper"),
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
@lab_case("path_detect", shells=("bash", "zsh", "fish", "nu"), lat=("lan", "150", "300"), helper=("nohelper", "helper"), scenarios=("p_exec", "p_tinypath"))
def c_path_detect(ctx, r, arm):
    """`cd` into a remote fixture directory, then type a subdirectory's name (no Tab, no Enter): the remote shell pushed its bounded cwd listing with the prompt
    (hop=cwd_listing, #1801: ids and counts only, never a name) and the app detects the typed word from it (hop=path_detect source=pushed), instantly, from the
    cache. Typing it opens ZERO new exec channels on the server (sshd log 'Starting session: command'): the no-helper path must not run a command per keystroke."""
    lab, u, base, rt = ctx.lab, arm["user"], work(arm), rtt_s(arm)
    uid = unique(ctx)
    lab.tree(u, {"pd%s/a.txt" % uid: "", "pd%s/sub/b.txt" % uid: ""})          # the fixture directory holds exactly 2 entries: a.txt and sub/
    if connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    time.sleep(2.0 + 3 * rt)                         # the connection's own start-up channels settle first
    off = ctx.log_size()
    ctx.be.type("cd %spd%s" % (base, uid), type_pace(ctx), enter=True)
    got = ctx.until(lambda: [m for m in HOOK_CWD_LISTING.finditer(ctx.log_since(off)) if m.group(3) == "2"], 10 + 6 * rt, step=0.1)
    if got:
        r.check(got[-1].group(2) == "L", "the pushed listing of the fixture directory is kind %s, expected L (complete)" % got[-1].group(2))
    else:
        r.unverifiable("[hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the shell is not one that does): detection from the pushed listing is not proven")
    time.sleep(0.5 + 2 * rt)
    size0 = lab.sshd_since(0)["size"]
    off = ctx.log_size()
    ctx.be.type("ls sub", 30)                         # a subdirectory of the cwd, typed whole: no Tab, no Enter
    hit = ctx.until(lambda: HOOK_PATH_DETECT.search(ctx.log_since(off)), 1.5 + 3 * rt, step=0.05)
    ctx.shot("lab_path_detect")
    if hit:
        r.check(hit.group(2) == "2" and hit.group(3) == "true", "path detection used a listing of %s entries (complete=%s), expected the fixture's 2 (complete)" % (hit.group(2), hit.group(3)))
    else:
        r.unverifiable("[hook] no hop=path_detect source=pushed line in the app log: detection from the pushed listing is not proven; only the channel count below is checked")
    time.sleep(0.5 + 2 * rt)
    starts = L.count_starts(lab.sshd_since(size0)["data"])
    r.metric("exec_channels_while_typing", starts)
    r.check(starts["command"] == 0 and starts["subsystem"] == 0, "typing a remote path opened %d exec and %d subsystem channel(s) on the server (expected none)" % (starts["command"], starts["subsystem"]))
    ctx.reset()
    finish_remote(ctx, r)


# ------------------------------------------------------------------ 3. git chips
@lab_case("chips", shells=("bash", "zsh", "fish", "nu"), lat=("lan", "150"), helper=("nohelper", "helper"))
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


# ------------------------------------------------------------------ 4b. Enter never waits on the network (#1970)
ENTER_WAIT_BUDGET_MS = 2.0          # owner's Enter budget: the app's own share of Enter; APPCI_LAB_ENTER_WAIT_BUDGET_MS overrides it for a diagnosis run


@lab_case("enter_no_network_wait", shells=("bash", "zsh", "fish", "nu"), lat=("150", "300"), helper=("nohelper", "helper"))
def c_enter_no_network_wait(ctx, r, arm):
    """#1970 ("Enter must never wait on the network"), judged from the APP's own log, no remote clock: lines typed at 10-25 ms/key with an instant Enter while
    the link has the arm's delay, then as many again in the SAME ssh session with the delay removed (the control; the RTT is measured again and the control is
    dropped when it is not LAN). Per Enter the app's `hop=enter_begin` -> line `pty_write` time (ctx.app_enter_write_samples). lab.enter_wait_verdict: no
    delayed sample reaches half the RTT (a wait on any round trip costs a whole one), and the delayed median is within the Enter budget of the undelayed one.
    The effect file holds every line of both phases once, in order. Each line is typed only after the previous one's effect is on the server and its prompt
    had 2 RTT to come back, so an Enter never meets a shell that is still busy. lab_enter_start is the other half: the whole Enter -> start against plain ssh."""
    lab, rt = ctx.lab, rtt_s(arm)
    n = ctx.count_lat(12)
    shell_kind = arm["shell"] if arm["shell"] in ("nu", "pwsh") else "bash"
    eff = work(arm, "enw_%s.txt" % unique(ctx, 4))
    budget = float(os.environ.get("APPCI_LAB_ENTER_WAIT_BUDGET_MS", ENTER_WAIT_BUDGET_MS))
    if connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    time.sleep(1.0 + 2 * rt)
    toks = []

    def phase(settle):
        t0 = time.time()
        for _ in range(n):
            tok = "enw" + unique(ctx, 5)
            toks.append(tok)
            ctx.be.type(ctx.append_cmd(shell_kind, tok, eff), type_pace(ctx), enter=True)
            ctx.until(lambda: any(tok in x for x in ctx.remote.lines(eff)), 10 + 4 * settle, step=0.02)
            time.sleep(0.15 + 2 * settle)
        time.sleep(0.3)
        return ctx.app_enter_write_samples(t0)

    delayed = phase(rt)
    lan_ms = None
    try:
        lab.clear_fault()
        gate = L.rtt_gate_verdict(lab.measure_rtt(15), 0)
        r.metric("control_rtt_gate", gate)
        if gate["verdict"] == "ok":
            lan_ms = phase(0.0)
        else:
            r.note("no undelayed control: after removing the delay the echo RTT measured %s ms (%s); only the half-RTT rule is judged" % (gate.get("median"), gate["verdict"]))
    except L.LabError as e:
        r.note("no undelayed control: the delay could not be removed (%s); only the half-RTT rule is judged" % (e,))
    CA.exact_lines(r, "effect file: every typed line arrived once, in order", toks, [x.strip() for x in ctx.remote.lines(eff)])
    v = L.enter_wait_verdict(delayed, lan_ms, arm["lat_ms"], budget_ms=budget)
    r.metric("enter_to_write_delayed_ms", delayed)
    r.metric("enter_to_write_lan_ms", lan_ms)
    r.metric("enter_wait_verdict", v)
    if v["verdict"] == "no-data":
        r.unverifiable("no Enter -> write times in the app log (%s): input-order logging is off, or the Enters took a path that logs no hop=enter_begin / pty_write" % v["why"])
    else:
        r.check(v["verdict"] == "pass", v["why"])
        if v["verdict"] == "pass":
            r.note(v["why"])
    finish_remote(ctx, r)


# ------------------------------------------------------------------ 5. dropped / reset / stalled connections
@lab_case("ssh_drop_reconnect", modes=("terminal",), shells=("bash", "zsh", "nu"), lat=("lan", "150"), helper=("nohelper", "helper"), faults=("rst", "stall10"))
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
@lab_case("ctrl_c_no_esc_i", shells=("bash", "zsh", "fish", "nu"), lat=("lan", "150"), helper=("nohelper", "helper"))
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


# ------------------------------------------------------------------ 7. The helper install itself (Wave H, #1804)
@lab_case("helper_install", shells=("bash",), lat=("lan", "150"), helper=("upload", "upgrade", "never"), scenarios=("p_noscp",), modes=("terminal",))
def c_helper_install(ctx, r, arm):
    """What the app does about the bundled helper on a host where the lab blocks every outbound connection:
    upload  -- nothing on the server, always_install: exactly one phosphor-remote-<version> appears (uploaded over the ssh connection, so no download is involved),
               it runs, and the pane is a working remote shell. On p_noscp (scp and sftp refused) this proves the in-band upload needs neither.
    upgrade -- older phosphor-remote-* and the previous remote server's phosphor-oss* are on the server: after the connect exactly the new version is left.
    never   -- the host's policy is never: the session works and NOTHING named phosphor-* was created or touched on the server.
    The marker file names come from `<APPCI_LAB_HELPER_BIN> --version`, the same tree the app was built from."""
    lab, rt = ctx.lab, rtt_s(arm)
    want = arm["helper_file"]
    off = ctx.log_size()
    if connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    mode = arm["helper"]
    if mode in ("upload", "upgrade"):
        ok = ctx.until(lambda: server_helper_files(lab, arm) == [want], 120 + 20 * rt, step=1.0)
        files = server_helper_files(lab, arm)
        r.check(bool(ok), "%s: expected exactly [%s] on the server, found %r" % (mode, want, files))
        shown = lab.ssh(arm["user"], "~/.phosphor/remote-server/%s --version" % want)
        r.check(shown.returncode == 0 and shown.stdout.startswith("phosphor-remote "), "the uploaded helper does not run on the server: rc=%s %r" % (shown.returncode, shown.stderr[-200:]))
        link = lab.ssh(arm["user"], "readlink ~/.phosphor/remote-server/current")
        r.check(link.stdout.strip() == want, "the `current` link points at %r, expected %r" % (link.stdout.strip(), want))
        if mode == "upgrade":
            r.check(files == [want], "older versions were not removed: %r" % (files,))
    else:
        time.sleep(3 + 4 * rt)
        files = server_helper_files(lab, arm)
        r.check(files == [], "policy never, but the server has %r" % (files,))
    log = ctx.log_since(off)
    r.check("remote helper install skipped" in log or mode != "never", "the app log never says the helper install was skipped for the denied host (label %s)" % host_label(arm))
    # the session is a working remote shell on every arm: the no-helper features do not depend on the helper
    tok = unique(ctx, 8)
    ctx.be.type("echo %s > %s" % (tok, work(arm, "hi_%s.txt" % tok)), type_pace(ctx), enter=True)
    ctx.until(lambda: ctx.remote.lines("hi_%s.txt" % tok) == [tok], 10 + 4 * rt)
    CA.exact_lines(r, "the command typed into the remote shell", [tok], ctx.remote.lines("hi_%s.txt" % tok))
    ctx.shot("lab_helper_install_%s" % mode)
    finish_remote(ctx, r)

# ------------------------------------------------------------------ 9. The "shell mode" chip (Wave 1.5, wording changed by Wave H parity P0)
@lab_case("no_helper_chip", shells=("bash", "zsh", "fish", "nu", "pwsh", "dash", "ash", "rbash"), lat=("lan",), helper=("nohelper",), modes=("terminal",))
def c_no_helper_chip(ctx, r, arm):
    """A Phosphorized ssh session WITHOUT the remote helper shows the small, informational chip "shell mode" (OCR). It never blocks: a command
    typed right after connecting still runs once, and the chip is not an error banner (no 'ERROR' text next to it)."""
    import cases_caseagent as CAG
    if connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    time.sleep(1.0 + 2 * rtt_s(arm))
    txt = ctx.ocr("lab_no_helper_chip")
    if txt is None:
        r.unverifiable("no OCR on this machine (tesseract): the chip on screen is not checked")
    else:
        flat = " ".join(txt.split())
        r.check(CAG.fuzzy_has(flat, "shell mode"), "no 'shell mode' chip on screen in a no-helper ssh session")
        r.check("ERROR" not in flat, "an ERROR text is on screen next to the no-helper chip: it must stay informational")
    tok = unique(ctx)
    ctx.be.type("echo chip%s" % tok, type_pace(ctx), enter=True)
    time.sleep(1.0 + 2 * rtt_s(arm))
    r.check(sum(1 for _, cmd in ctx.remote.commands() if "echo chip%s" % tok in cmd) == 1, "a command typed after connecting did not run exactly once with the chip up")
    finish_remote(ctx, r)

# ------------------------------------------------------------------ 6b. nushell agent commands: pagers off, restored (#1813)
NU_PAGER_MARKER = "# __phosphor_ap_pagers"          # agent_shell_wrap.rs NU_PAGER_MARKER (selftest keeps them equal)


@lab_case("nu_agent_pager_env", shells=("nu",), lat=("lan",), helper=("nohelper",), modes=("terminal",))
def c_nu_agent_pager_env(ctx, r, arm):
    """#1813 on a REAL remote nushell (u_nu): the lines an agent command is typed as (` <command> # __phosphor_ap_pagers`, the exact wrapper form for Nu) are run
    in the app's ssh session. `cd` and `$env.X =` of the command persist; PAGER/LESS/... are set during it (for nushell and for an external child); after a
    success, after an external command that exits 3 and after a nushell error the user's own PAGER is back and the variables the user never had are gone; an
    unmarked line sees the user's own PAGER. The local twin with a throw-away HOME is script/test_nu_agent_pager_env."""
    tok = unique(ctx, 5)
    eff = work(arm, "pg_%s.txt" % tok)
    if connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    time.sleep(1.0 + 2 * rtt_s(arm))

    def sv(expr):
        return "((%s) + (char nl)) | save -a %s" % (expr, eff)

    def run(line, want_lines):
        ctx.be.type(line, type_pace(ctx), enter=True)
        ctx.until(lambda: len(ctx.remote.lines(eff)) >= want_lines, 10 + 4 * rtt_s(arm), step=0.1)

    after = sv("$\"after:($env.PAGER? | default 'unset'):($env.LESS? | default 'unset'):($env.PGTOK? | default 'none'):($env.PWD | path basename)\"")
    sub = "pgsub" + tok
    run(' $env.PAGER = "mypager"', 0)
    run(" mkdir %s; cd %s; $env.PGTOK = \"%s\"; %s; ^sh -c 'echo ext:$PAGER:$LESS >> %s' %s" % (
        sub, sub, tok, sv("$\"during:($env.PAGER):($env.LESS)\""), eff, NU_PAGER_MARKER), 2)
    run(" " + after, 3)
    run(" ^sh -c 'echo d3:$PAGER >> %s; exit 3' %s" % (eff, NU_PAGER_MARKER), 4)
    run(" " + after, 5)
    run(" %s; error make {msg: \"boom\"} %s" % (sv("$\"d4:($env.PAGER)\""), NU_PAGER_MARKER), 6)
    run(" " + after, 7)
    got = ctx.remote.lines(eff)
    keep = "after:mypager:unset:%s:%s" % (tok, sub)
    r.check(got[:2] == ["during:cat:-FRX", "ext:cat:-FRX"], "pagers not set during the command: %r" % got[:2])
    r.check(got[2:3] == [keep], "after success: cd/$env did not persist or the user's PAGER/LESS was not restored: %r" % got[2:3])
    r.check(got[3:5] == ["d3:cat", keep], "after an external command that exits non-zero: %r" % got[3:5])
    r.check(got[5:7] == ["d4:cat", keep], "after a nushell error: %r" % got[5:7])
    finish_remote(ctx, r)


# ------------------------------------------------------------------ 7. a real password / keyboard-interactive prompt (#1535)
@lab_case("password_prompt", shells=(), lat=("lan", "150"), helper=("nohelper",), scenarios=("p_password", "p_kbdint"))
def c_password_prompt(ctx, r, arm):
    """The owner's secret-prompt rule against a REAL sshd that asks for the unix password (the same-machine harness sshd cannot: no account password, no PAM):
    p_password (password auth) and p_kbdint (keyboard-interactive via PAM). The ssh_password_prompt body (CR.secret_prompt): nothing is typed until ssh's prompt
    is on screen; the secret typed after it logs in; the secret is in no shell's rec.log and not on screen; a wrong secret ends the ssh (NumberOfPasswordPrompts 1)
    and the next command runs in the LOCAL shell. The password is the throw-away one labd leased for this run. Not covered: p_mfa (a TOTP that must be computed at
    typing time, not at prompt time) and the passphrase prompt (ssh_password_prompt covers it on the same-machine sshd)."""
    secret = (getattr(ctx.lab, "leased", None) or {}).get("password")
    if not secret:
        r.unverifiable("the lab lease carries no password for %s" % arm["user"])
        return
    CR.rm_remote(ctx, "pw_labpw.txt")
    CR.secret_prompt(ctx, r, host_of(arm), secret, "password", "labpw")
    ctx.reset()
