# appci/cases_lab_buffers.py: lab case for family c, remote buffer editing (Wave H parity P4), registered through lab_case.
#   lab_remote_edit   open a file of the remote host in the app's code editor, edit it, save it; then change the file from a second ssh connection and save again.
# Arms: nohelper (shell mode: no server-side buffer; the conflict is found at SAVE, design ruling R4) and upload (the helper: a conflict is PUSHED when the file changes under
# unsaved edits). Shells: bash, zsh, fish, nu.
#
# The driver (round 15, #2063; before it the editor layer was UNVERIFIABLE for want of a way to open the file). All by keyboard:
#   * the fixture is a git repository on the host holding ONE file; the user `cd`s into it;
#   * the command palette's "Toggle project explorer" opens the file tree and gives it the focus (Workspace::open_left_panel_view,
#     focus_active_view_on_entry); Down moves the selection and stops at the last row (FileTreeAction::SelectNextItem clamps), which is the one file; Enter
#     opens it (FileTreeEvent::OpenRemoteFile -> Workspace::open_remote_file_with_target);
#   * the app's own line `Opening remote file: host=.. path=<the file>` is the proof that the remote file was opened. Without it NOTHING is typed (the keys
#     would go wherever the focus is) and the row is UNVERIFIABLE with the screenshot: the driver did not get there, which says nothing about the editor;
#   * with it: an edit is typed and saved (Ctrl/Cmd-S), and the host's file must hold it, byte for byte, routed through the arm's backend
#     (`hop=backend_route family=buffers backend=<helper|shell>`);
#   * then the file is changed on the host from a second ssh connection, a second edit is typed and saved. Whatever the editor shows (the helper pushes the
#     change; the shell backend finds it at save, design ruling R4), the host's change must still be in the file: a save that silently replaces it is the
#     defect (#1837).
# A fish / nu login shell has a plain session by decision (#2019): UNVERIFIABLE with that reference.
# The same protocol and the same conflict flow are covered, with real files and both backends, by crates/remote_server/src/backend/contract_tests.rs
# (assert_buffers, the_shell_arm_finds_a_same_size_same_second_change_with_the_hash).
import os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases_lab as CL  # noqa: E402

ROUTE = re.compile(r"hop=backend_route family=(\w+) backend=(helper|shell|unavailable) reason=(\w+)")
SHELLS = ("bash", "zsh", "fish", "nu", "dash", "ash")


def routes(text, family="buffers"):
    """[(backend, reason)] of the router's choices for `family` in `text` (the app log)"""
    return [(m.group(2), m.group(3)) for m in ROUTE.finditer(text) if m.group(1) == family]


def expected_backend(arm):
    return "helper" if arm["helper"] != "nohelper" else "shell"


def remote_bytes(ctx, path):
    return "\n".join(ctx.remote.lines(path))


OPEN_REMOTE = re.compile(r"Opening remote file: host=\S+ path=(\S+)")          # Workspace::open_remote_file_with_target


def opened_remote(text, name):
    """True when the app logged that it opened a remote file whose path ends in `name`"""
    return any(m.group(1).endswith("/" + name) or m.group(1) == name for m in OPEN_REMOTE.finditer(text))


def conflict_verdict(host_text, outside, second_edit):
    """pure: the host file after a save that raced a change made on the host. None = fine (the host's change is still there: the save was refused, or the
    editor had taken the change in first); else the failure text."""
    if outside in host_text:
        return None
    if second_edit in host_text:
        return "the save REPLACED a change made on the host: the file holds the editor's text %r and not %r" % (host_text[:120], outside)
    return "the host file holds neither the host's change nor the editor's second edit: %r" % host_text[:120]


def close_editor(ctx, undo):
    """leave the editor pane: undo the unsaved typing first (a pane with unsaved edits asks before it closes), then the close-pane key"""
    import cases_robust as RB
    ctx.be.guard()
    if undo:
        ctx.be.key("cmd+z" if ctx.be.name == "mac" else "ctrl+z", repeat=undo, delay_ms=40)
        time.sleep(0.4)
    ctx.be.key(RB.CLOSE_PANE)
    time.sleep(1.0)


@CL.lab_case("remote_edit", shells=SHELLS, lat=("lan",), helper=("nohelper", "upload"), modes=("terminal",))
def c_remote_edit(ctx, r, arm):
    """family c: a file on the host -> opened in the code editor from the file tree by keyboard -> edit, save -> a change on the host -> edit, save: the
    host's change is never replaced; every buffer call goes to the arm's backend"""
    lab, u, rt = ctx.lab, arm["user"], CL.rtt_s(arm)
    tok = CL.unique(ctx, 6)
    fx = "edit_%s" % tok
    name = "zz_%s.txt" % tok
    repo, path = CL.work(arm, fx), CL.work(arm, "%s/%s" % (fx, name))
    staged = "staged %s" % tok
    lab.tree(u, {"%s/%s" % (fx, name): staged + "\n"}, git=True)
    off = ctx.log_size()
    if CL.connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    time.sleep(1.0 + 2 * rt)
    why = CL.plain_session_feature_limit(arm["shell"], ctx.log_since(off), "the code editor on a remote file")
    if why:
        r.metric("plain_session_by_decision", arm["shell"])
        r.unverifiable(why)
        ctx.remote.rm(path)
        CL.finish_remote(ctx, r)
        return
    ctx.be.type("cd %s" % repo, CL.type_pace(ctx), enter=True)
    time.sleep(2.0 + 3 * rt)
    c0 = len(ctx.remote.commands())
    save = "cmd+s" if ctx.be.name == "mac" else "ctrl+s"

    # ---- open the file from the file tree, by keyboard
    mark = ctx.log_size()
    CL.palette_run(ctx, CL.PALETTE_PROJECT_EXPLORER, "lab_remote_edit_palette")
    time.sleep(1.0 + 2 * rt)
    ctx.be.key("Down", repeat=4, delay_ms=120)
    time.sleep(0.3)
    ctx.shot("lab_remote_edit_tree_%s" % arm["helper"])
    ctx.be.key("Return")
    opened = bool(ctx.until(lambda: opened_remote(ctx.log_since(mark), name), 8 + 4 * rt, step=0.1))
    if not opened:
        ctx.shot("lab_remote_edit_not_opened_%s" % arm["helper"])
        other = OPEN_REMOTE.search(ctx.log_since(mark))
        r.unverifiable("the keyboard driver did not open %s in the code editor: the palette's '%s', Down x4 and Enter were sent and the app logged %s. Nothing was "
                       "typed after that, so the editor is not judged (shots/*lab_remote_edit_tree_*.png shows what the file tree held)" % (
                           name, CL.PALETTE_PROJECT_EXPLORER, "another remote file opening" if other else "no 'Opening remote file' line"))
        if other:
            close_editor(ctx, 0)
        CL.palette_run(ctx, CL.PALETTE_PROJECT_EXPLORER)        # close the panel again (a toggle); ctx.reset then clears the line whatever has the focus
        ctx.reset()
        r.check(remote_bytes(ctx, path) == staged, "the staged file changed without an edit: %r" % remote_bytes(ctx, path))
        ctx.remote.rm(path)
        CL.finish_remote(ctx, r)
        return
    time.sleep(1.5 + 2 * rt)

    # ---- edit, save: the host's file holds the edit
    t1 = "E1%s_" % tok
    ctx.be.key("cmd+up" if ctx.be.name == "mac" else "ctrl+Home")          # as cb_editor_mac / editor_edit_save
    ctx.be.type(t1, ctx.rng.randint(10, 25))
    time.sleep(0.4)
    ctx.be.key(save)
    ctx.until(lambda: t1 in remote_bytes(ctx, path), 10 + 6 * rt, step=0.25)
    ctx.shot("lab_remote_edit_saved_%s" % arm["helper"])
    got = remote_bytes(ctx, path)
    r.check(got == t1 + staged, "save: the host file holds %r, expected %r; the editor may not have had the focus (shots/*lab_remote_edit_saved_*.png)" % (got[:120], t1 + staged))
    rs = routes(ctx.log_since(off))
    r.check(bool(rs), "[hook] the file was opened and the app logged no `hop=backend_route family=buffers` line: the route is not proven")
    r.check(all(b == expected_backend(arm) for b, _ in rs), "buffers routed to %r, expected every call on %s" % (sorted(set(rs)), expected_backend(arm)))

    # ---- a change on the host, then a second edit and save: the host's change must survive
    outside = "outside_%s" % tok
    lab.ssh(u, "sh -c 'echo %s >> %s'" % (outside, path))
    time.sleep(1.5 + 3 * rt)
    t2 = "E2%s_" % tok
    ctx.be.type(t2, ctx.rng.randint(10, 25))
    time.sleep(0.4)
    ctx.be.key(save)
    time.sleep(3.0 + 4 * rt)
    ctx.shot("lab_remote_edit_conflict_%s" % arm["helper"])
    host = remote_bytes(ctx, path)
    bad = conflict_verdict(host, outside, t2)
    r.check(bad is None, bad or "")
    r.metric("remote_edit_after_conflict", "second edit saved" if t2 in host else "save refused, host text kept")

    # ---- leave: the editor pane, the file tree, the remote session
    close_editor(ctx, len(t2) + 8 if t2 not in host else 0)
    CL.palette_run(ctx, CL.PALETTE_PROJECT_EXPLORER)
    typed = [cmd for _, cmd in ctx.remote.commands()[c0:] if tok in cmd and not cmd.startswith("cd ")]      # the case's own `cd` can be logged late
    r.check(not typed, "the editor's keys reached the remote shell: %r" % typed[:2])
    ctx.remote.rm(path)
    CL.finish_remote(ctx, r)


def declare_coverage():
    """tell the coverage guard (cases_lab_helper.py) that this case covers the buffers family on both arms. Idempotent; called from here when cases_lab_helper is loaded
    and from cases_lab_helper's end otherwise (the two import each other through cases.py)."""
    import cases_lab_helper as CLH
    CLH.declare_external("buffers", helper_case="lab_remote_edit", nohelper_case="lab_remote_edit")


if hasattr(sys.modules.get("cases_lab_helper"), "declare_external"):
    declare_coverage()
