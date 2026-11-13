# appci/cases_lab_helper.py: lab coverage of the remote helper (`phosphor-remote`) AND of the no-helper (shell) arm (package P10; docs/testing.md 2.15 "helper coverage").
# Registered with the same @lab_case machinery as cases_lab.py (one body per arm: lab_<case>.<shell|user>.<lan|150|300>.<helper arm>). Does not touch cases_lab_remote.py
# (the file/tree family cases live there; their covers are declared in EXTERNAL_COVERS below so the coverage guard sees them).
#
# Three groups:
#   1. lifecycle  : helper_quiet_fallback, helper_digest_mismatch, helper_missing_asset, helper_one_version, helper_policy, helper_idle_exit, helper_reconnect,
#                   helper_scp_fallback (+ the existing lab_helper_install: upload / upgrade / never / p_noscp)
#   2. capability : fam_<capability>, ONE case per Family of crates/remote_server/src/backend/mod.rs, with a `helper` arm (helper present: the feature must go through the
#                   helper, no extra sshd exec channel) and a `nohelper` arm (the shell backend over the ControlMaster out-of-band channel: >=1 channel, at most one per prompt).
#                   The capability list, the Family -> capability map and "does this family have a shell arm yet" are READ FROM THE RUST SOURCE (parse_rust), so a family that
#                   lands later flips from SKIP to running with no edit here.
#   3. routing    : helper_route_unsupported, helper_killed; and the no-helper gaps: nohelper_tab_hands_back, nohelper_unavailable, nohelper_listing_single_flight.
# [hook] = rests on an app log line the app must write (hop=backend_route ...); until it exists the check is UNVERIFIABLE (r.unverifiable), never a pass.
# NOT RUN: written without a lab (no cargo, no podman on the authoring machine). Mac/Windows run these through the fixed-port lab by lab.py's public API only
# (Lab.ssh/file/tree/helper/sshd_since/drop/fault); no machine-local path is used.
import os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases  # noqa: E402
import cases_caseremote as CR  # noqa: E402
import cases_lab as CL  # noqa: E402
import lab as L  # noqa: E402

REPO = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", ".."))
RS = os.path.join(REPO, "crates", "remote_server", "src")
ROUTE = re.compile(r"hop=backend_route family=(\w+) backend=(helper|shell)")          # [hook] the router's choice for one call (ids and names only)
INFLIGHT = re.compile(r"hop=listing_inflight session=\d+ inflight=(\d+)")             # [hook] as-you-type remote listing requests in flight
ALL_SHELLS = L.ALL_SHELLS
QUICK_SHELLS = ("bash", "zsh", "fish", "nu")
NO_SHELL_ARM_EVER = {"codebase_index"}          # legacy host-side indexing: the full daemon only (capability.rs), never a shell arm, never a helper capability
# families whose cases live in cases_lab_remote.py (P1/P3); declared here so the guard is satisfied once that file registers them. Names are `lab_<case>`.
EXTERNAL_COVERS = {}                            # {capability: {"helper": "lab_x", "nohelper": "lab_x"}} filled by cases_lab_remote.py through declare_external()

COVERS = {}                                     # case base name -> dict(caps=[..], arms={"helper","nohelper",...})


# ------------------------------------------------------------------ reading the Rust source (the single source of truth)
def parse_rust(rs=RS):
    """-> dict(consts={NAME: "value"}, helper=[value..], families={Variant: capability value}, stubbed={Variant..}, shell_text=str)"""
    cap = open(os.path.join(rs, "capability.rs"), encoding="utf-8").read()
    consts = dict(re.findall(r'pub const (\w+): &str = "([^"]+)";', cap))
    m = re.search(r"pub const HELPER_CAPABILITIES: &\[&str\] =\s*&\[(.*?)\];", cap, re.S)
    helper = [consts[n] for n in re.findall(r"\b([A-Z_]+)\b", m.group(1)) if n in consts] if m else []
    mod = open(os.path.join(rs, "backend", "mod.rs"), encoding="utf-8").read()
    fam = {v: consts[c] for v, c in re.findall(r"Family::(\w+) => capability::(\w+),", mod) if c in consts}
    feat = open(os.path.join(rs, "backend", "features.rs"), encoding="utf-8").read()
    # a method is a stub only while its shell arm is `=> unimplemented`; a family counts as stubbed when ANY of its methods still is
    stubbed = set(re.findall(r"^\s+(\w+), (?:true|false): fn .*=> unimplemented;", feat, re.M))
    shell_text = ""
    for root in (os.path.join(rs, "backend"), os.path.join(rs, "shellcmd")):
        for dp, _, fs in os.walk(root):
            for f in fs:
                if f.endswith(".rs") and not f.endswith("_tests.rs") and f not in ("features.rs", "contract_tests.rs", "mod.rs", "router.rs", "helper.rs", "refresh.rs"):
                    shell_text += open(os.path.join(dp, f), encoding="utf-8", errors="replace").read()
    return dict(consts=consts, helper=helper, families=fam, stubbed=stubbed, shell_text=shell_text)


def shell_arm_implemented(variant, info):
    """does the shell backend serve this family yet? A family still listed in features.rs' stub macro does not; the families with no request method (RunCommand rides
    the legacy ControlMaster executor, always there; git status / github / agent context / chunks) count once the shell code mentions `Family::<Variant>`."""
    if variant == "RunCommand":
        return True
    if variant in info["stubbed"]:
        return False
    return ("Family::%s" % variant) in info["shell_text"]


def declare_external(cap, helper_case=None, nohelper_case=None):
    """cases_lab_remote.py calls this for each family it covers: declare_external("fs_basic", helper_case="lab_remote_fs", nohelper_case="lab_remote_fs")"""
    EXTERNAL_COVERS[cap] = dict(helper=helper_case, nohelper=nohelper_case)


def coverage(info=None):
    """-> (missing_helper, missing_nohelper): capability strings with no registered/declared case with that arm. The self-test fails on a non-empty list."""
    info = info or parse_rust()
    have_h = {c for v in COVERS.values() for c in v["caps"] if "helper" in v["arms"]} | {c for c, v in EXTERNAL_COVERS.items() if v.get("helper")}
    have_n = {c for v in COVERS.values() for c in v["caps"] if "nohelper" in v["arms"]} | {c for c, v in EXTERNAL_COVERS.items() if v.get("nohelper")}
    miss_h = [c for c in info["helper"] if c not in have_h]
    miss_n = [c for c in info["families"].values() if c not in NO_SHELL_ARM_EVER and c not in have_n]
    return miss_h, miss_n


# ------------------------------------------------------------------ registration with a pre-skip (before the app is relaunched for the arm)
def reg(name, fn, caps=(), shells=QUICK_SHELLS, lat=("lan", "150"), helper=("nohelper",), scenarios=(), modes=("terminal",), faults=(), pre=None):
    CL.lab_case(name, shells=shells, lat=lat, helper=helper, scenarios=scenarios, modes=modes, faults=faults)(fn)
    COVERS[name] = dict(caps=list(caps), arms=set(helper))
    if pre:
        for c in cases.CASES:
            if c.get("lab") and c["arm"]["case"] == name and not getattr(c["fn"], "_pre", False):
                c["fn"] = _with_pre(c["fn"], c["arm"], pre)
    return fn


def _with_pre(fn, arm, pre):
    def run(ctx, r):
        why = pre(arm)
        if why:
            r.skip(why)
        return fn(ctx, r)
    run.__name__, run._pre = fn.__name__, True
    return run


# ------------------------------------------------------------------ small lab helpers
# #2163: the daemon pattern is self-match-proof. The ssh wrapper's own `bash -c '<script>'` carries the pattern text in its command line, so a plain
# 'phosphor-remote.* daemon' matches that wrapper too (and counted one daemon too many). `[p]hosphor-remote.* daemon` matches the daemon's command line and
# not its own text (the bracket class never reads `[p]`), and pgrep never lists itself.
DAEMON_PATTERN = "[p]hosphor-remote.* daemon"


def parse_pids(text):
    """pure: the sorted pids in `pgrep` output (one per line); non-numeric lines are ignored"""
    return sorted(int(x) for x in (text or "").split() if x.isdigit())


def daemon_pids(lab, user):
    """[pid] of this user's helper daemons, or None when the count could not be read"""
    out = lab.ssh(user, "pgrep -u $(id -u) -f '%s'; true" % DAEMON_PATTERN)
    if out.returncode not in (0, None):
        return None
    return parse_pids(out.stdout)


def daemons(lab, user):
    pids = daemon_pids(lab, user)
    return None if pids is None else len(pids)


def kill_daemons(ctx, r, lab, user):
    """#2163: kill this user's helper daemons left by earlier cases (the lab kills a user's processes only at lease release, and a daemon idles 10 minutes),
    BEFORE the case connects, so the count the case judges is its own. SIGKILL by pid; waits until none is left; a daemon that survives is noted."""
    for pid in daemon_pids(lab, user) or []:
        lab.ssh(user, "kill -9 %d; true" % pid)
    left = ctx.until(lambda: daemon_pids(lab, user) == [], 10.0, step=0.25)
    if not left:
        r.note("helper daemons of %s still running before the case: %r" % (user, daemon_pids(lab, user)))
    return bool(left)


def new_channels(lab, size0):
    return L.count_starts(lab.sshd_since(size0)["data"])["command"]


def routes(log, cap):
    return [b for c, b in ROUTE.findall(log) if c == cap]


def ocr_flat(ctx, tag):
    t = ctx.ocr(tag)
    return None if t is None else " ".join(t.split())


def has_any(flat, words):
    import cases_caseagent as CAG
    return any(CAG.fuzzy_has(flat, w) for w in words)


def asset_file(ctx):
    import glob
    g = glob.glob(os.path.join(ctx.run, "helper-asset", "phosphor-remote-linux-*.gz"))
    return g[0] if g else None


def session_ok(ctx, r, arm, label=""):
    """the pane is a working remote shell on every arm: a typed command runs once (observed in the shell's own record, which rbash/dash/ash lack: then the prompt count)"""
    tok = CL.unique(ctx, 8)
    c0 = len(ctx.remote.commands())
    ctx.be.type("echo hlp%s" % tok, CL.type_pace(ctx), enter=True)
    time.sleep(1.0 + 2 * CL.rtt_s(arm))
    if arm["shell"] in ("bash", "zsh", "fish", "nu", "pwsh"):
        n = sum(1 for _, c in ctx.remote.commands()[c0:] if "echo hlp%s" % tok in c)
        r.check(n == 1, "%sa command typed into the session ran %d times, expected once" % (label, n))


def settle(ctx, arm, secs=1.0):
    time.sleep(secs + 2 * CL.rtt_s(arm))


# ------------------------------------------------------------------ 1. lifecycle
def pre_needs_pins(arm):
    return None


def c_quiet_fallback(ctx, r, arm):
    """p_rohome (read-only ~) and p_noinstall (read-only ~/.phosphor, no scp): always_install cannot put the helper anywhere. The app falls back to shell mode QUIETLY:
    the session works, the 'shell mode' chip is on screen (OCR), and there is no failure banner. Nothing named phosphor-* appears."""
    off = ctx.log_size()
    if CL.connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    settle(ctx, arm, 2.0)
    flat = ocr_flat(ctx, "lab_quiet_fallback")
    if flat is None:
        r.unverifiable("no OCR: the chip and the absence of a banner are not checked")
    else:
        r.check(has_any(flat, ["shell mode"]), "no 'shell mode' chip after a helper install that could not happen")
        r.check(not has_any(flat, ["install refused", "integrity check", "could not be installed", "install failed"]), "a failure banner is on screen: the fall-back must be quiet")
    files = CL.server_helper_files(ctx.lab, arm)
    r.check(files in ([], None), "the server has %r although the home cannot be written" % (files,))
    session_ok(ctx, r, arm)
    CL.finish_remote(ctx, r)


reg("helper_quiet_fallback", c_quiet_fallback, shells=("bash",), lat=("lan", "150"), helper=("upload",), scenarios=("p_rohome", "p_noinstall"))


def c_digest_mismatch(ctx, r, arm):
    """The bundled helper is not the one this client was built with (a tampered asset): the install is REFUSED before any upload (fail closed), the failure banner shows,
    and nothing named phosphor-* is on the server. Needs an app built WITH pinned digests (APPCI_APP_PINNED_DIGESTS=1: a dev build computes its digest from the asset and
    cannot mismatch), otherwise UNVERIFIABLE."""
    if os.environ.get("APPCI_APP_PINNED_DIGESTS") != "1":
        r.unverifiable("the app under test has no pinned helper digest (set APPCI_APP_PINNED_DIGESTS=1 for a build with PHOSPHOR_REMOTE_SHA256_*): a dev build cannot mismatch")
        return
    import gzip
    f = asset_file(ctx)
    if not f:
        r.unverifiable("no staged helper asset")
        return
    raw = gzip.open(f, "rb").read()
    with gzip.open(f, "wb") as g:
        g.write(raw + b"\0tampered")
    try:
        if CL.connect_or_fail(ctx, r, arm) is None:
            ctx.reset()
            return
        settle(ctx, arm, 3.0)
        r.check(CL.server_helper_files(ctx.lab, arm) == [], "a tampered helper was uploaded: %r" % (CL.server_helper_files(ctx.lab, arm),))
        flat = ocr_flat(ctx, "lab_digest_mismatch")
        if flat is None:
            r.unverifiable("no OCR: the banner is not checked")
        else:
            r.check(has_any(flat, ["does not match", "refused", "verification"]), "no refusal banner for a helper whose digest does not match")
        session_ok(ctx, r, arm)
        CL.finish_remote(ctx, r)
    finally:
        CL.stage_helper_asset(ctx, os.environ["APPCI_LAB_HELPER_BIN"])


reg("helper_digest_mismatch", c_digest_mismatch, shells=("bash",), lat=("lan",), helper=("upload",))


def c_missing_asset(ctx, r, arm):
    """No bundled helper for this platform (the asset file is gone): a QUIET fall-back to shell mode (HELPER_UNAVAILABLE_PREFIX): no banner, chip shows, nothing uploaded."""
    f = asset_file(ctx)
    if not f:
        r.unverifiable("no staged helper asset")
        return
    hidden = f + ".hidden"
    os.rename(f, hidden)
    try:
        if CL.connect_or_fail(ctx, r, arm) is None:
            ctx.reset()
            return
        settle(ctx, arm, 2.0)
        r.check(CL.server_helper_files(ctx.lab, arm) == [], "something was uploaded with no bundled asset")
        flat = ocr_flat(ctx, "lab_missing_asset")
        if flat is None:
            r.unverifiable("no OCR")
        else:
            r.check(has_any(flat, ["shell mode"]), "no 'shell mode' chip when the bundled helper is missing")
            r.check(not has_any(flat, ["install refused", "install failed"]), "a banner is shown for a missing bundled helper: it must be quiet")
        session_ok(ctx, r, arm)
        CL.finish_remote(ctx, r)
    finally:
        os.rename(hidden, f)


reg("helper_missing_asset", c_missing_asset, shells=("bash",), lat=("lan",), helper=("upload",))


def c_one_version(ctx, r, arm):
    """One version per host: old phosphor-remote-<v> and phosphor-oss* seeded (the `upgrade` arm does it) are gone after the connect, only the current remains; a LIVE
    process of another version (a seeded old binary that sleeps) is NOT killed. Complements lab_helper_install.upgrade, which checks the file set only."""
    lab, u = ctx.lab, arm["user"]
    seed = {n: b"#!/bin/sh\nexec sleep 3600\n" for n in CL.SEED_VERSIONS}
    lab.helper(u, seed)
    out = lab.ssh(u, "setsid nohup ~/.phosphor/remote-server/phosphor-remote-v0.0.2 >/dev/null 2>&1 </dev/null & echo $!")
    pid = out.stdout.strip().splitlines()[-1] if out.stdout.strip() else ""
    if not pid.isdigit():
        r.unverifiable("could not start the seeded live process: %r" % out.stderr[-200:])
        return
    if CL.connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    want = arm["helper_file"]
    ok = ctx.until(lambda: CL.server_helper_files(lab, arm) == [want], 120 + 20 * CL.rtt_s(arm), step=1.0)
    r.check(bool(ok), "expected only [%s], found %r" % (want, CL.server_helper_files(lab, arm)))
    alive = lab.ssh(u, "kill -0 %s && echo alive" % pid).stdout.strip()
    r.check(alive == "alive", "the live process of another version (pid %s) was killed by the cleanup" % pid)
    lab.ssh(u, "kill %s 2>/dev/null; true" % pid)
    session_ok(ctx, r, arm)
    CL.finish_remote(ctx, r)


reg("helper_one_version", c_one_version, shells=("bash",), lat=("lan",), helper=("upgrade",))


def c_policy(ctx, r, arm):
    """Per-host lists. With the host in remote_helper_allowed_hosts and the app-wide mode never_install, the helper IS installed; with the host in the denied list and
    always_install nothing is (covered by lab_helper_install.never). The 'Don't ask me again for this host' choice is a click on the in-terminal block: the harness has no
    scripted click for it, so that part is UNVERIFIABLE until it has one."""
    cfg = ctx.prof.home + "/.config/phosphor/settings.toml"
    t = open(cfg).read()
    open(cfg, "w").write(CR.T.set_key(t, "warpify.ssh", "remote_helper_allowed_hosts", '["%s"]' % CL.host_label(arm)))
    if not CR.set_ext_mode(ctx, "never_install"):
        r.skip("the app could not be relaunched")
    ctx.lab_helper_mode = "policy"
    if CL.connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    ok = ctx.until(lambda: CL.server_helper_files(ctx.lab, arm) == [arm["helper_file"]], 120 + 20 * CL.rtt_s(arm), step=1.0)
    r.check(bool(ok), "host in the allowed list but the helper was not installed: %r" % (CL.server_helper_files(ctx.lab, arm),))
    r.unverifiable("'Don't ask me again for this host' (ask mode + a click on the choice block) has no scripted driver yet")
    CL.finish_remote(ctx, r)
    CL.set_denied_hosts(ctx, [])
    CR.T.edit_file(cfg, "warpify.ssh", "remote_helper_allowed_hosts", "[]")


reg("helper_policy", c_policy, shells=("bash",), lat=("lan",), helper=("upload",))


def c_idle_exit(ctx, r, arm):
    """The helper's daemon exits by itself after its grace period (daemon/server.rs GRACE_PERIOD, 10 min) with no proxy connected: connect, `exit`, wait. Slow: only with
    APPCI_LAB_SLOW=1 (a candidate/release run)."""
    lab, u = ctx.lab, arm["user"]
    kill_daemons(ctx, r, lab, u)
    if CL.connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    settle(ctx, arm, 3.0)
    n = daemons(lab, u)
    r.check(n == 1, "with the helper session up there are %s daemons, expected 1" % n)
    CL.finish_remote(ctx, r)
    gone = ctx.until(lambda: daemons(lab, u) == 0, 11 * 60 + 30, step=15)
    r.check(bool(gone), "the helper daemon was still running 11.5 minutes after the last session closed")


reg("helper_idle_exit", c_idle_exit, shells=("bash",), lat=("lan",), helper=("helper",),
    pre=lambda arm: None if os.environ.get("APPCI_LAB_SLOW") == "1" else "slow (11 min grace wait): set APPCI_LAB_SLOW=1 for a candidate/release run")


def c_reconnect(ctx, r, arm):
    """After `lab drop rst` the pane is local; ssh again: the helper session is re-established against the SAME single daemon (no duplicate), files unchanged, features still
    routed to the helper."""
    lab, u = ctx.lab, arm["user"]
    kill_daemons(ctx, r, lab, u)                      # #2163: the daemons of earlier cases are not this case's
    if CL.connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    settle(ctx, arm, 2.0)
    files0 = CL.server_helper_files(lab, arm)
    pids0 = daemon_pids(lab, u)
    lab.drop(u, "rst")
    up = ctx.until(lambda: ctx.where_now() == "local", 30, step=0.5)
    r.check(bool(up), "no local prompt after the connection was reset")
    if CL.connect_or_fail(ctx, r, arm, "reconnect: ") is None:
        ctx.reset()
        return
    settle(ctx, arm, 2.0)
    pids1 = daemon_pids(lab, u)
    r.check(pids1 is not None and len(pids1) == 1, "after the reconnect there are %s helper daemons (pids %r), expected exactly 1 (no duplicate)" % (None if pids1 is None else len(pids1), pids1))
    r.check(pids0 is not None and len(pids0) == 1 and pids0 == pids1, "the reconnect did not attach to the SAME daemon: pids %r before, %r after" % (pids0, pids1))
    r.check(CL.server_helper_files(lab, arm) == files0, "the helper files changed across the reconnect: %r -> %r" % (files0, CL.server_helper_files(lab, arm)))
    session_ok(ctx, r, arm, "reconnect: ")
    CL.finish_remote(ctx, r)


reg("helper_reconnect", c_reconnect, shells=("bash", "zsh"), lat=("lan", "150"), helper=("helper",))


def c_scp_fallback(ctx, r, arm):
    """The in-band upload fails and the app falls back to scp. No lab scenario makes ONLY the in-band stream fail while scp works (p_noscp is the reverse); until a
    scenario user exists for it this is UNVERIFIABLE, registered so the lifecycle table is complete."""
    r.unverifiable("no lab scenario fails the in-band upload while scp works (needs a users.tsv ForceCommand that rejects the upload stream only)")


reg("helper_scp_fallback", c_scp_fallback, shells=("bash",), lat=("lan",), helper=("upload",))


# ------------------------------------------------------------------ 2. one case per Family (both arms)
def drive_repo(ctx, r, arm, fx):
    """cd into the git fixture on the remote and give the chip / listing / branch work time to run; returns (prompts, branch seen on screen or None when no OCR)"""
    p0 = ctx.remote.prompts()
    off = ctx.log_size()
    ctx.be.type("cd %s" % fx["dir"], CL.type_pace(ctx), enter=True)
    seen, txt = None, None
    for i in range(6):
        time.sleep(1.0 + 2 * CL.rtt_s(arm))
        txt = ctx.ocr("lab_fam_%d" % i)
        seen, how = CL.branch_evidence(txt, fx["branch"], ctx.log_since(off))
        if seen is not False:          # on screen / in the app log's chip value, or no OCR and no log line: nothing more to wait for
            break
    ctx.be.type("ls", CL.type_pace(ctx), enter=True)
    settle(ctx, arm)
    return CL.chip_prompt_count(ctx.remote.prompts() - p0, ctx.log_since(off)), seen


DRIVERS = {"run_command": drive_repo, "git_read": drive_repo, "fs_basic": drive_repo}          # capability -> scripted app driver; a family without one gets NO generic case (#2068)


def fam_pre(cap, variant, info):
    def pre(arm):
        if arm["helper"] == "helper" and cap not in info["helper"]:
            return "family %s not in HELPER_CAPABILITIES yet" % cap
        if arm["helper"] == "nohelper":
            if cap in NO_SHELL_ARM_EVER:
                return "%s has no shell arm by design (legacy full daemon only)" % cap
            if not shell_arm_implemented(variant, info):
                return "shell arm for %s not implemented yet" % cap
        return None
    return pre


def fam_body(cap):
    def body(ctx, r, arm):
        lab, u, rt = ctx.lab, arm["user"], CL.rtt_s(arm)
        uid = CL.unique(ctx)
        fx = dict(branch="br" + uid, dir=CL.work(arm) + "fam" + uid)
        lab.tree(u, {"README": "needle\n", "src/a.rs": "fn main(){}\n"}, dir="fam" + uid, git=True, branch=fx["branch"], branches=("feature-x",))
        off = ctx.log_size()
        if CL.connect_or_fail(ctx, r, arm) is None:
            ctx.reset()
            return
        settle(ctx, arm, 2.0)
        if CL.plain_gate(ctx, r, arm, off, "the %s family routes of the managed session" % cap):             # #2160
            return
        CL.wait_helper_takeover(ctx, r, arm, off)                                                           # #2164
        drive = DRIVERS[cap]                    # #2068: a family with no driver is not registered at all (_register_families)
        size0 = lab.sshd_since(0)["size"]
        c0 = len(ctx.remote.commands())
        watch = CL.ChannelWatch(ctx, lab, size0, ctx.log_size()).start()
        prompts, seen = drive(ctx, r, arm, fx)
        watch.stop(r)
        calls = new_channels(lab, size0)
        r.metric("exec_channels", calls)
        r.metric("prompts", prompts)
        if seen is False:
            r.check(False, "branch %s not on screen after cd into the repo on the remote" % fx["branch"])
        if arm["helper"] == "helper":
            r.check(calls == 0, "%d extra sshd exec channels with the helper present: %s must be served through the helper" % (calls, cap))
            want = "helper"
        else:
            r.check(1 <= calls <= prompts, "shell arm: %d out-of-band channels for %d prompt(s), expected 1..prompts" % (calls, prompts))
            want = "shell"
        got = routes(ctx.log_since(off), cap)
        if not got:
            r.unverifiable("[hook] the app logged no `hop=backend_route family=%s backend=..` line: the route is not proven" % cap)
        else:
            r.check(set(got) == {want}, "%s was routed to %s, expected only %s" % (cap, sorted(set(got)), want))
        # R1: the shell backend never types into the user's pty: the shell's own record holds only what this case typed
        typed = [c.strip() for _, c in ctx.remote.commands()[c0:]]
        if arm["shell"] in ("bash", "zsh", "fish", "nu", "pwsh"):
            r.check(all(c.startswith("cd ") or c == "ls" for c in typed), "the remote shell received lines the user did not type: %r" % typed)
        CL.finish_remote(ctx, r)
    return body


def _register_families():
    """#2068: a generic `lab_fam_<cap>` case exists only for a family that has a scripted driver (DRIVERS). Until round 14 one was generated for EVERY family
    and counted for the coverage guard, and the eleven without a driver reported UNVERIFIABLE after the connect ("registered so the guard passes"): 48 arms
    of lab_fam_agent_context alone were NOREF rows in the fix tier (PROVING_LAB's `lab_fam_agent_*`). A family without a driver must now be covered by a
    real case that declares itself (declare_external), or the guard (coverage(), a self-test) fails."""
    info = parse_rust()
    for variant, cap in sorted(info["families"].items(), key=lambda kv: kv[1]):
        if cap in NO_SHELL_ARM_EVER or cap not in DRIVERS:
            continue
        reg("fam_" + cap, fam_body(cap), caps=[cap], shells=ALL_SHELLS, lat=("lan", "150", "300"), helper=("nohelper", "helper"), pre=fam_pre(cap, variant, info))


_register_families()

# Coverage by a real case that lives in a file which does not import this one. Each names the case that JUDGES the capability on both arms:
#   file_write_expect / file_delete_expect (#1837): refinements of file_write (WriteFile.expect, DeleteFile.expect), not families of their own. The stale
#     accept and the stale delete are refused on both arms in lab_fam_agent_edit_conflict / lab_fam_agent_delete_conflict (cases_agent_file_edits.py).
#     Until round 14 two always-SKIP rows (`lab_fam_file_write_expect`, `lab_fam_file_delete_expect`) stood in for these declarations.
#   ripgrep_search: lab_agent_remote_flow (cases_agent_remote.py) has the agent grep and glob the host on the no-helper and the upload arm, judges both
#     results and checks the family's `hop=backend_route` line. Its generic lab_fam_ case had no driver and was the only thing the guard saw.
declare_external("file_write_expect", helper_case="lab_fam_agent_edit_conflict", nohelper_case="lab_fam_agent_edit_conflict")
declare_external("file_delete_expect", helper_case="lab_fam_agent_delete_conflict", nohelper_case="lab_fam_agent_delete_conflict")
declare_external("ripgrep_search", helper_case="lab_agent_remote_flow", nohelper_case="lab_agent_remote_flow")


def unregistered_externals(names=None):
    """-> [(capability, case)] declared in EXTERNAL_COVERS whose case is not registered (no `<case>.` arm in cases.CASES): a declaration that names nothing
    would satisfy the guard with no case behind it. The self-test fails on a non-empty list."""
    names = [c["name"] for c in cases.CASES] if names is None else names
    out = []
    for cap, v in sorted(EXTERNAL_COVERS.items()):
        for case_name in sorted({x for x in (v.get("helper"), v.get("nohelper")) if x}):
            if not any(n == case_name or n.startswith(case_name + ".") for n in names):
                out.append((cap, case_name))
    return out


# ------------------------------------------------------------------ 3. routing
def c_route_unsupported(ctx, r, arm):
    """The helper answers UNSUPPORTED for a family mid-session: the same feature falls back to the shell arm and keeps working (no banner, branch chip still shows).
    Needs a helper that answers UNSUPPORTED for git_read although it advertised it: APPCI_LAB_UNSUPPORTED_HELPER_BIN (a test build of phosphor-remote); else UNVERIFIABLE."""
    bad = os.environ.get("APPCI_LAB_UNSUPPORTED_HELPER_BIN", "")
    if not (bad and os.path.isfile(bad)):
        r.unverifiable("APPCI_LAB_UNSUPPORTED_HELPER_BIN does not name a test helper that answers UNSUPPORTED (no protocol shim exists in the lab)")
        return
    lab, u = ctx.lab, arm["user"]
    lab.helper(u, {arm["helper_file"]: open(bad, "rb").read()})
    fam_body("git_read")(ctx, r, dict(arm, helper="nohelper"))


reg("helper_route_unsupported", c_route_unsupported, caps=[], shells=("bash",), lat=("lan",), helper=("helper",))


def c_helper_killed(ctx, r, arm):
    """The helper daemon is killed mid-session: reads (the branch chip) fall back to the shell arm and keep working (>=1 out-of-band channel afterwards, branch visible);
    a WRITE reports an error and is NOT retried: the remote file holds exactly one write, never two. The write half needs a scripted app write (none yet): UNVERIFIABLE."""
    lab, u = ctx.lab, arm["user"]
    uid = CL.unique(ctx)
    br = "br" + uid
    lab.tree(u, {"README": "x\n"}, dir="kil" + uid, git=True, branch=br)
    kill_daemons(ctx, r, lab, u)                      # #2163: only this case's own daemon is counted and killed
    off0 = ctx.log_size()
    if CL.connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    settle(ctx, arm, 2.0)
    CL.wait_helper_takeover(ctx, r, arm, off0)
    pids = daemon_pids(lab, u)
    r.check(pids is not None and len(pids) == 1, "no single helper daemon to kill: pids %r" % (pids,))
    for pid in pids or []:
        lab.ssh(u, "kill -9 %d; true" % pid)
    time.sleep(1.0 + 2 * CL.rtt_s(arm))
    size0 = lab.sshd_since(0)["size"]
    off = ctx.log_size()
    import cases_ui as UI
    UI.register_find(ctx, br)
    ctx.be.type("cd %skil%s" % (CL.work(arm), uid), CL.type_pace(ctx), enter=True)
    # r25 G: the branch is judged from the app log (the ShellGitBranch chip update naming it) with the screen as a second witness (find_token: the app's find hook
    # first, OCR second): OCR alone misses random tokens. Polled long enough for a 150 ms link (event: the log line; the limit is only a give-up).
    rt = CL.rtt_s(arm)
    in_log = bool(ctx.until(lambda: br in CL.chip_values(ctx.log_since(off)), 15 + 8 * rt, step=0.2))
    on_screen = None
    for i in range(3):
        on_screen, _p = UI.find_token(ctx, "lab_killed_%d" % i, br, since=off, r=r)
        if on_screen or in_log:
            break
        time.sleep(1.0 + 2 * rt)
    r.note("branch %s: app log chip update %s; on screen %s" % (br, "names it" if in_log else "does not name it", "yes" if on_screen else "not found"))
    r.check(in_log or bool(on_screen), "branch %s not shown after the helper was killed (neither the app log's ShellGitBranch chip update nor the screen has it): the read did not fall back to the shell" % br)
    r.check(new_channels(lab, size0) >= 1, "no out-of-band channel after the helper died: the read did not use the shell arm")
    got = routes(ctx.log_since(off), "git_read")
    if got:
        r.check("shell" in got, "after the kill git_read was still routed %s" % sorted(set(got)))
    r.unverifiable("write half (error reported, no second attempt) needs a scripted app write; the remote file count check is not run")
    CL.finish_remote(ctx, r)


reg("helper_killed", c_helper_killed, caps=[], shells=("bash", "zsh"), lat=("lan", "150"), helper=("helper",))


# ------------------------------------------------------------------ no-helper gaps (the rest: lab_tab_remote_dir, lab_path_detect, lab_chips, lab_no_helper_chip, lab_enter_start)
def c_tab_hands_back(ctx, r, arm):
    """No answer to a remote Tab hands the typed line back: the link stalls for 3 s (labd stall), `echo <tok>` + Tab + Enter are typed into it; when the link returns the remote
    shell received the typed line exactly once (with or without a completion), never twice and never lost."""
    tok = CL.unique(ctx, 8)
    if CL.connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    settle(ctx, arm, 1.5)
    c0 = len(ctx.remote.commands())
    ctx.lab.drop(arm["user"], "stall", 3)
    ctx.be.type("echo %s" % tok, CL.type_pace(ctx), enter=False)
    ctx.be.key("Tab")
    ctx.be.key("Return")
    ctx.until(lambda: any(tok in c for _, c in ctx.remote.commands()[c0:]), 30, step=0.5)
    n = sum(1 for _, c in ctx.remote.commands()[c0:] if tok in c)
    r.check(n == 1, "the typed line reached the remote shell %d times, expected exactly once" % n)
    if n != 1:
        CL.recorder_evidence(ctx, r, arm, c0, "nohelper_tab_hands_back")
    CL.finish_remote(ctx, r)


reg("nohelper_tab_hands_back", c_tab_hands_back, shells=("bash", "zsh", "fish", "nu"), lat=("lan", "150"), helper=("nohelper",))


def c_unavailable(ctx, r, arm):
    """A session with no out-of-band channel (a subshell started inside the ssh session: the app cannot tell its prompt from the remote's) never types into the pty: the
    shell's own record holds exactly the lines the case typed. (Owner decision on #2166, 2026-11-12: no 'unavailable here' notice is specified for this state, so none is required.)"""
    if CL.connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    settle(ctx, arm, 1.5)
    c0 = len(ctx.remote.commands())
    typed = []
    for line in ("bash", "cd /work", "ls"):
        typed.append(line)
        ctx.be.type(line, CL.type_pace(ctx), enter=True)
        settle(ctx, arm, 0.8)
    # #2166: the on-screen "unavailable"/"not available" text expectation was dropped by owner decision; the no-typing check below is unchanged.
    rec = [c.strip() for _, c in ctx.remote.commands()[c0:]]
    r.check(all(c in typed for c in rec), "the remote shell received lines nobody typed (the app typed into the pty): %r" % rec)
    ctx.exit_nested(None, r, "nohelper_unavailable: inner bash")        # audit #2: the where probe must land REMOTE before `exit`
    settle(ctx, arm, 0.8)
    CL.finish_remote(ctx, r)


reg("nohelper_unavailable", c_unavailable, shells=("bash",), lat=("lan", "150"), helper=("nohelper",))


def c_single_flight(ctx, r, arm):
    """As-you-type remote listing is single-flight: while typing a long path into a remote directory with many entries at 10-25 ms/key, at most ONE listing request is in
    flight at a time (the latest wins). [hook] hop=listing_inflight; UNVERIFIABLE until the app logs it."""
    uid = CL.unique(ctx)
    ctx.lab.tree(arm["user"], {"d%s/f%02d.txt" % (uid, i): "x\n" for i in range(40)}, dir="sf" + uid)
    off = ctx.log_size()
    if CL.connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    settle(ctx, arm, 1.5)
    ctx.be.type("ls %ssf%s/d%s/f0" % (CL.work(arm), uid, uid), CL.type_pace(ctx), enter=False)
    settle(ctx, arm, 1.5)
    seen = [int(n) for n in INFLIGHT.findall(ctx.log_since(off))]
    if not seen:
        r.unverifiable("[hook] the app logged no `hop=listing_inflight` line: single-flight is not proven")
    else:
        r.check(max(seen) <= 1, "up to %d remote listings in flight at once, expected at most 1" % max(seen))
    ctx.be.key("ctrl+u")
    CL.finish_remote(ctx, r)


reg("nohelper_listing_single_flight", c_single_flight, shells=("bash", "zsh", "fish", "nu"), lat=("lan", "150", "300"), helper=("nohelper",))


import cases_lab_remote as _CLR  # noqa: E402  (P1/P2 coverage declarations; see cases_lab_remote.declare_coverage)
_CLR.declare_coverage()
import cases_lab_skills as _CLS  # noqa: E402  (P8 coverage declaration; see cases_lab_skills.declare_coverage)
_CLS.declare_coverage()
import cases_lab_buffers as _CLB  # noqa: E402  (P4 coverage declaration)
_CLB.declare_coverage()
