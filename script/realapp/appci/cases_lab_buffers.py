# appci/cases_lab_buffers.py: lab case for family c, remote buffer editing (Wave H parity P4), registered through lab_case.
#   lab_remote_edit   open a file of the remote host in the app's code editor, edit it, save it; then change the file from a second ssh connection and save again.
# Arms: nohelper (shell mode: no server-side buffer; the conflict is found at SAVE, design ruling R4) and upload (the helper: a conflict is PUSHED when the file changes under
# unsaved edits). Shells: bash, zsh, fish, nu.
#
# What this case can and cannot prove today (no result here is ever a pass it did not earn):
#   * It stages a file on the host through the lab's own ssh connection, connects the app (nohelper / upload as the arm says) and checks that connecting did not touch
#     the staged file and that nothing was typed into the remote pty.
#   * The EDITOR layer is UNVERIFIABLE: opening a remote file in the code editor needs a pointer click on the file tree (or on a link in the output) and the harness
#     backends only have keys, typing and screenshots (backend.py). So the open / edit / save / conflict-banner steps below are not driven. The oracle for them is written
#     (remote_bytes, routes, expected_backend) and runs the moment a driver can open the file: set APPCI_LAB_EDIT_OPENED=1 to say the file was opened by hand during the
#     run, and the case then checks the routing line `hop=backend_route family=buffers backend=<helper|shell>` and that the host file kept the staged bytes.
#   * The same protocol and the same conflict flow are covered, with real files and both backends, by crates/remote_server/src/backend/contract_tests.rs
#     (assert_buffers, the_shell_arm_finds_a_same_size_same_second_change_with_the_hash).
import os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases_lab as CL  # noqa: E402

ROUTE = re.compile(r"hop=backend_route family=(\w+) backend=(helper|shell|unavailable) reason=(\w+)")
SHELLS = ("bash", "zsh", "fish", "nu")


def routes(text, family="buffers"):
    """[(backend, reason)] of the router's choices for `family` in `text` (the app log)"""
    return [(m.group(2), m.group(3)) for m in ROUTE.finditer(text) if m.group(1) == family]


def expected_backend(arm):
    return "helper" if arm["helper"] != "nohelper" else "shell"


def remote_bytes(ctx, path):
    return "\n".join(ctx.remote.lines(path))


@CL.lab_case("remote_edit", shells=SHELLS, lat=("lan",), helper=("nohelper", "upload"), modes=("terminal",))
def c_remote_edit(ctx, r, arm):
    """family c: staged file -> connect -> (editor layer UNVERIFIABLE: no pointer primitive) -> the host's file is unchanged and the pty untouched"""
    lab, u, rt = ctx.lab, arm["user"], CL.rtt_s(arm)
    tok = CL.unique(ctx, 6)
    path = CL.work(arm, "edit_%s.txt" % tok)
    staged = "staged %s" % tok
    lab.ssh(u, "printf '%%s\\n' '%s' > %s" % (staged, path))
    off = ctx.log_size()
    if CL.connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    time.sleep(1.0 + 2 * rt)
    c0 = len(ctx.remote.commands())
    if os.environ.get("APPCI_LAB_EDIT_OPENED") == "1":
        got = routes(ctx.log_since(off))
        r.check(bool(got), "no backend_route line for buffers: the file was not opened through the router")
        r.check(all(b == expected_backend(arm) for b, _ in got), "buffers routed to %r, expected %s" % (got, expected_backend(arm)))
    else:
        r.unverifiable("the harness cannot open a remote file in the code editor (no pointer click on the file tree); open/edit/save/conflict are covered by the Rust "
                       "contract tests on both backends, not by this case. Set APPCI_LAB_EDIT_OPENED=1 after opening %s by hand to check the routing line." % os.path.basename(path))
    r.check(remote_bytes(ctx, path) == staged, "the staged file changed without an edit: %r" % remote_bytes(ctx, path))
    typed = [cmd for _, cmd in ctx.remote.commands()[c0:] if tok in cmd]
    r.check(not typed, "the feature typed into the remote shell: %r" % typed[:2])
    ctx.remote.rm(path)
    CL.finish_remote(ctx, r)


def declare_coverage():
    """tell the coverage guard (cases_lab_helper.py) that this case covers the buffers family on both arms. Idempotent; called from here when cases_lab_helper is loaded
    and from cases_lab_helper's end otherwise (the two import each other through cases.py)."""
    import cases_lab_helper as CLH
    CLH.declare_external("buffers", helper_case="lab_remote_edit", nohelper_case="lab_remote_edit")


if hasattr(sys.modules.get("cases_lab_helper"), "declare_external"):
    declare_coverage()
