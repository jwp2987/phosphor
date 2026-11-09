# appci/cases_lab_remote.py: lab cases for the remote parity families (Wave H parity, docs/design/remote-helper-parity.md), registered through lab_case.
#   lab_read_files_remote   family a: the agent's read_files on a remote ssh session, no helper (shell mode over the ControlMaster) and with the uploaded helper
#   lab_diff_apply_remote   the write half of apply_file_diffs: create a file, then delete it, over the same two arms
# Shells: u_bash (bash), u_dash, u_ash, u_nu, u_pwsh (the login shell the ControlMaster side exec runs the commands in). Agent mode only, against the scripted fake model
# (fake_model.py, APPCI_FAKE_MODEL=1: "Please read exactly this file and nothing else: <path>", "Please create exactly this file: <path> with this content: <text>",
# "Please delete exactly this file and nothing else: <path>"). Oracles: the file on the lab host (create/delete), and the app log's routing line
# `hop=backend_route family=<capability> backend=<helper|shell|unavailable> reason=<...>` (nohelper -> shell, upload -> helper); no key is typed into the remote
# pty by the feature (the remote shell's rec.log only holds what the test typed). Never run without the lab / fake model: lab_prepare and need_model SKIP with the reason.
import os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases_caseagent as AG  # noqa: E402
import cases_lab as CL  # noqa: E402
import cases_replay as CRP  # noqa: E402

ROUTE = re.compile(r"hop=backend_route family=(\w+) backend=(helper|shell|unavailable) reason=(\w+)")
SHELLS = ("bash", "dash", "ash", "nu", "pwsh")


def routes(ctx, off, family):
    return [(m.group(2), m.group(3)) for m in ROUTE.finditer(ctx.log_since(off)) if m.group(1) == family]


def expected_backend(arm):
    return "helper" if arm["helper"] != "nohelper" else "shell"


def ask(ctx, text, family, timeout=90):
    """type `text` to the agent, approve the card if one shows, wait for the routing line of `family`; returns the routes seen"""
    off = ctx.log_size()
    AG.type_fast(ctx, text)
    t_end = time.time() + timeout
    pressed = False
    while time.time() < t_end:
        got = routes(ctx, off, family)
        if got:
            time.sleep(1.5)
            return routes(ctx, off, family)
        if not pressed and AG.count_in_log(ctx, off, "queue_actions") >= 1:
            time.sleep(0.8)
            ctx.be.key("Return")
            time.sleep(0.6)
            ctx.be.key("Return")
            pressed = True
        time.sleep(0.25)
    return []


@CL.lab_case("read_files_remote", shells=SHELLS, lat=("lan",), helper=("nohelper", "upload"), modes=("agent",))
def c_read_files_remote(ctx, r, arm):
    """family a: a file created on the remote host is read back by the agent's read_files; the routing line says which arm served it"""
    CRP.need_model(ctx, r)
    tok = CL.unique(ctx, 6)
    path = CL.work(arm, "rf_%s.txt" % tok)
    if CL.connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    time.sleep(1.0 + 2 * CL.rtt_s(arm))
    ask(ctx, "Please create exactly this file: %s with this content: line one %s" % (path, tok), "file_write")
    r.check(any(tok in l for l in ctx.remote.lines(path)), "the setup file was not created on the host")
    got = ask(ctx, "Please read exactly this file and nothing else: %s" % path, "file_context")
    r.check(bool(got), "no backend_route line for file_context: the read never reached the router")
    r.check(all(b == expected_backend(arm) for b, _ in got), "file_context routed to %r, expected %s" % (got, expected_backend(arm)))
    typed = [cmd for _, cmd in ctx.remote.commands() if tok in cmd]
    r.check(not typed, "the feature typed into the remote shell: %r" % typed[:2])
    ctx.remote.rm(path)
    CL.finish_remote(ctx, r)


@CL.lab_case("diff_apply_remote", shells=SHELLS, lat=("lan",), helper=("nohelper", "upload"), modes=("agent",))
def c_diff_apply_remote(ctx, r, arm):
    """the write half of apply_file_diffs: create, then delete, a file on the remote host; the host's file is the oracle, the pty stays untouched"""
    CRP.need_model(ctx, r)
    tok = CL.unique(ctx, 6)
    path = CL.work(arm, "da_%s.txt" % tok)
    if CL.connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    time.sleep(1.0 + 2 * CL.rtt_s(arm))
    got = ask(ctx, "Please create exactly this file: %s with this content: hello %s" % (path, tok), "file_write")
    r.check(ctx.until(lambda: any(tok in l for l in ctx.remote.lines(path)), 30, step=0.5), "the created file is not on the host")
    r.check(all(b == expected_backend(arm) for b, _ in got), "file_write routed to %r, expected %s" % (got, expected_backend(arm)))
    ask(ctx, "Please delete exactly this file and nothing else: %s" % path, "file_write")
    r.check(ctx.until(lambda: not ctx.remote.lines(path), 30, step=0.5), "the deleted file is still on the host")
    typed = [cmd for _, cmd in ctx.remote.commands() if tok in cmd]
    r.check(not typed, "the feature typed into the remote shell: %r" % typed[:2])
    ctx.remote.rm(path)
    CL.finish_remote(ctx, r)


def declare_coverage():
    """tell the coverage guard (cases_lab_helper.py) which cases cover which families; both arms of each body above are registered here. Idempotent. Called from
    here when cases_lab_helper is already loaded and from cases_lab_helper's end otherwise (the two import each other through cases.py)."""
    import cases_lab_helper as CLH
    CLH.declare_external("file_context", helper_case="lab_read_files_remote", nohelper_case="lab_read_files_remote")
    CLH.declare_external("file_write", helper_case="lab_diff_apply_remote", nohelper_case="lab_diff_apply_remote")


if hasattr(sys.modules.get("cases_lab_helper"), "declare_external"):
    declare_coverage()
