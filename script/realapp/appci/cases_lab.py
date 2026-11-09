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
