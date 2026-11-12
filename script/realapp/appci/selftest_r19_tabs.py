# appci/selftest_r19_tabs.py: offline self-tests of round 19 "tabs" (docs/design/r10-r19-tabs.md; #2108 the app defect, #2109 the test faults of the
# round 14 candidate, #2110 the restore diagnostic). Pure Python: no app, no display. selftest.py appends CHECKS to its own list (two lines at its
# end), so `run.py --self-test` runs them. Every judge is fed a right input AND the wrong ones it exists to catch.
import inspect, os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.join(HERE, "..", "..", "..")
sys.path.insert(0, HERE)

CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


def _src(*parts):
    return open(os.path.join(REPO, *parts), encoding="utf-8").read()


@check
def r19_find_is_judged_from_its_enter():
    """a Find is judged from the lines its Enter caused: the bar's re-run of the previous query when it opens (the round 14 false match) is before
    the Enter and not read; after the Enter a scroll is a match, a no_match line or a zero count is none, nothing is no verdict. Both cases use the
    one step, and the step empties the field and takes its offset before the Enter."""
    import cases  # noqa: F401
    import cases_r14_w1a as W, cases_daily_b as D, cases_r14_state as S
    scroll = "a input_order hop=find_scroll result=scroll block=3 moved=true t=1\n"
    none = "b input_order hop=find_scroll result=no_match pending=false t=2\n"
    assert W.find_enter_verdict(scroll) == "scroll" and W.find_enter_verdict(none) == "no_match"
    assert W.find_enter_verdict(none + scroll) == "scroll", "the async scan focused a match after a first no_match"
    assert W.find_enter_verdict("c input_order hop=find_state event=next count=0 focused=none scanning=false t=3\n") == "no_match"
    assert W.find_enter_verdict("c input_order hop=find_state event=next count=2 focused=1 scanning=false t=3\n") is None, "a count is not a scroll"
    assert W.find_enter_verdict("") is None and W.find_enter_verdict("x hop=other\n") is None
    assert W.find_scroll_hops(scroll + none) == ["result=scroll block=3 moved=true t=1", "result=no_match pending=false t=2"]
    body = inspect.getsource(W.find_enter)
    assert body.index("clear_field(ctx)") < body.index("typed(ctx, token, enter=False)") < body.index("off = ctx.log_size()") < body.index('ctx.be.key("Return")')
    assert "find_enter_verdict(ctx.log_since(off))" in body and "ctx.log_since(off0)" not in body, "only the log since the Enter is judged"
    assert "W1A.find_enter(" in inspect.getsource(D.do_find) and "W1A.find_enter(" in inspect.getsource(S.find_in_pane)
    # the pure judges on top of it still fail the wrong results
    assert D.judge_find("scroll", "scroll", "no_match")[0] == "ok" and D.judge_find("scroll", "scroll", "scroll")[0] == "fail"
    assert D.judge_find("scroll", "no_match", "no_match")[0] == "fail" and D.judge_find(None, "scroll", "no_match")[0] == "noref"
    assert S.find_result(W.find_scroll_hops(none)) == "no_match" and S.find_result([]) == "none" and S.find_result(W.find_scroll_hops(scroll)) == "scroll"
    assert "show_find_bar" in _src("app", "src", "terminal", "view.rs") and "Start find using the previous query" in _src("app", "src", "terminal", "view.rs")


@check
def r19_restore_store_note_2110():
    """the store diagnostic of restore_shows_history names which half lost the output; it is a note in the failure text and a metric, never a verdict"""
    import cases  # noqa: F401
    import cases_r14_state as S
    assert "could not be read" in S.store_note({"after_stop": None, "after_relaunch": None})
    assert "none with the token" in S.store_note({"after_stop": (4, 0), "after_relaunch": (4, 0)})
    assert "the launch dropped the rows" in S.store_note({"after_stop": (4, 1), "after_relaunch": (0, 0)})
    assert "does not show them" in S.store_note({"after_stop": (4, 1), "after_relaunch": (4, 1)}) and "does not show them" in S.store_note({"after_stop": (4, 1)})
    src = inspect.getsource(S.c_restore_shows_history)
    assert 'r.metric("rs_store", store)' in src and "store_note(store)" in src
    assert src.index('store = {"after_stop": stored_blocks_with(ctx, tok)}') < src.index('R.start_app(ctx, r, "rs_restored"'), "read before the next launch"
    assert "stylized_output" in _src("crates", "persistence", "src", "schema.rs"), "the blocks table the diagnostic reads"


@check
def r19_editor_close_question_line_2108():
    """editor_daily's "the close key asked" is the pane path's line OR the in-app dialog's own (the editor's close key closes the editor tab, whose
    question never logs the pane line); neither line = did not ask. The app logs the dialog line where the dialog is shown."""
    import cases  # noqa: F401
    import cases_r15_w5a as E
    assert E.close_question_up("x Displaying unsaved changes warning for pane\n") and E.close_question_up("y input_order hop=native_modal event=shown t=1\n")
    assert not E.close_question_up("z input_order hop=native_modal event=closed focus_back=true t=2\n") and not E.close_question_up("") and not E.close_question_up(None)
    view = _src("app", "src", "workspace", "view.rs")
    assert 'log_hop("native_modal", || "event=shown".to_string())' in view and "event=closed focus_back={focus_back}" in view
    assert "closed_dialog_hands_focus_to_tab(" in view and "PHOSPHOR_MODAL_FOCUS_BACK" in _src("app", "src", "workspace", "native_modal.rs")
    assert "RemoveTabAtIndex" in _src("app", "src", "pane_group", "mod.rs"), "the editor's close key still takes the editor-tab path the comment names"
    for fn in (E._arm_close_question, E.close_editor):
        assert "close_question_up(ctx.log_since(off))" in inspect.getsource(fn), fn.__name__
    assert "press_overwrite(ctx)" in inspect.getsource(E.close_editor), "a Save that meets the conflict banner asks again (#2092): answered"


@check
def r19_tab_pointer_middle_click_and_cleanup():
    """tab_pointer_use: a middle-clicked tab is closed when the last tab answers from the tab before it; its shell is judged after the undo-close
    window, not 15 s after the click; the clean-up never closes a tab that answers from the first tab's directory (or not at all)"""
    import cases  # noqa: F401
    import cases_window_use as W
    assert W.undo_window_left(100.0, 130.0, 75.0) == 45.0 and W.undo_window_left(100.0, 400.0, 75.0) == 0.0
    rnd = inspect.getsource(W._tab_round)
    assert "got == dirs[order[-2]]" in rnd and "closed_at[victim] = time.time()" in rnd
    assert "15, step=0.2" not in rnd, "the shell is not expected to be gone 15 s after the click (undo-close keeps it 60 s)"
    case = inspect.getsource(W.c_tab_pointer_use)
    assert "close_until_first_tab(ctx, r, R.norm_path(ctx.W), len(toks) - 1)" in case and "pid_alive(p)) if pids" not in case
    assert "undo_window_left(t_closed, time.time(), window)" in case and "an orphan" in case
    cl = inspect.getsource(W.close_until_first_tab)
    assert 'in (home, "<none>")' in cl and cl.index("V.tab_answer(") < cl.index("R.close_tab_best_effort(ctx, r)"), "the probe comes before every close"
    assert "CloseTab(index) => self.close_tab(*index, false, true, ctx)" in _src("app", "src", "workspace", "view.rs"), "a middle click closes into the undo stack"


@check
def r19_panel_rows_are_left_of_the_pane():
    """the Windows panel-row locator: with the pane's left edge known, a token sighting in the pane (round 14: x=356, panel at x=149) is not a row;
    rows left of it are; an unknown edge keeps every sighting and a column that is not stacked is then NOT judged, and nothing is clicked"""
    import cases  # noqa: F401
    import cases_wrap_vtabs as V
    row = lambda cx, cy: ("t", cx, cy, cx - 30, cx + 30, cy - 8, cy + 8)       # noqa: E731
    pane, panel = [row(356, 462), row(356, 529)], [row(149, 179)]
    assert V.left_of_pane(pane + panel, 320) == panel and V.left_of_pane(pane, 320) == [] and V.left_of_pane(pane + panel, None) == pane + panel
    assert V.left_of_pane([row(300, 10)], 320) == [], "a token that reaches into the pane is not a panel row"
    agent = {2: [(356, 462)], 3: [(356, 462)], 4: [(356, 529)]}                # the round 14 Git Bash agent pass
    assert V.rows_stacked(V.hook_panel_rows(agent))[0] is False
    src = inspect.getsource(V._panel_rows_from_hook) + inspect.getsource(V.c_vertical_tabs_panel_click)
    assert "left_of_pane(ws, pane_left)" in src and "if not ok and pane_left is None:" in src
    win = src[src.index("if not ok and pane_left is None:"):src.index("U.wl_check(ctx, r, bool(ok)")]
    assert win.count("return") == 2, "neither an unjudged column nor a failed one is clicked"
    edge = inspect.getsource(V._pane_left_edge)
    assert "printf '%%s%%s" in edge and "min(w[3] for w in" in edge, "the probe is in a command's OUTPUT only, at column 0"


@check
def r19_md_viewer_root_pid_and_windows_log():
    """md_viewer_first_frame: the control arm compares with the root shell of its own instance; the Windows launcher lets the two Info lines the
    md_viewer cases read through (the modules come from the Rust files that log them)"""
    import app as A
    import cases, cases_r17_misc as M
    assert cases.viewer_close_verdict(10, 10, True)[0] and not cases.viewer_close_verdict(11, 10, True)[0] and not cases.viewer_close_verdict(None, 10, True)[0]
    assert cases.viewer_close_verdict(11, None, True)[0], "no root on record: a shell that answers is enough"
    cf = inspect.getsource(cases.close_file_viewer)
    assert "root_pid is ROOT_ON_RECORD" in cf
    body = inspect.getsource(M.c_md_viewer_first_frame)
    assert body.index("croot = ctx.shell_pid()") < body.index('_open_and_read(ctx, r, path, "md_viewer_first_frame control")')
    assert 'cases.close_file_viewer(ctx, r, coff, "md_viewer_first_frame control", root_pid=croot)' in body
    assert 'cases.close_file_viewer(ctx, r, off, "md_viewer_first_frame")' in body, "the first arm keeps the root run.py recorded"
    assert A.rust_log_enabled(A.WINDOWS_RUST_LOG, "warpui_core::core::app", "info") and A.rust_log_enabled(A.WINDOWS_RUST_LOG, "warp_editor::render::model", "info")
    assert not A.rust_log_enabled("warn,warp::input_order=debug", "warpui_core::core::app", "info"), "the matcher: a bare warn hides it"
    assert 'log::log!(log_level, "dispatching action for {name:?}")' in _src("crates", "warpui_core", "src", "core", "app.rs")
    assert "rich text: first size {}x{} released {} held layout edit(s)" in _src("crates", "editor", "src", "render", "model", "mod.rs")
    assert 'name = "warp_editor"' in _src("crates", "editor", "Cargo.toml")
    assert A.rust_log_enabled(A.WINDOWS_RUST_LOG, "warp::input_order", "debug"), "hop=native_modal is an input_order hop"


@check
def r19_view_mode_of_the_pass_own_pane():
    """left_agent_mode_after_*: the latest view_mode line of ANOTHER pane (soak's closed throw-away tab) is not this pane's mode. Own pane's line,
    or all panes agreeing: the log answers. Panes disagreeing: 'other' (the screen decides). The own pane leaving agent view still says terminal."""
    import types
    import cases
    rows = cases.view_mode_rows("{0}hop=view_mode mode=agent pane=7 t=1\n{0}hop=view_mode mode=terminal pane=9 t=2\n{0}hop=view_mode mode=agent\n".format("2026-11-12T19:35:37Z [DEBUG] "))
    assert rows == [("agent", "7"), ("terminal", "9"), ("agent", None)]
    V = cases.view_mode_verdict
    assert V([], "7") is None and V([("agent", "7")], None) == "agent" and V([("agent", "7"), ("terminal", "9")], None) == "terminal"
    assert V([("agent", "7"), ("terminal", "9")], "7") == "other", "soak: the throw-away tab's line is the latest"
    assert V([("agent", "7"), ("terminal", "9"), ("terminal", "7")], "7") == "terminal", "the own pane left agent view: still a failure"
    assert V([("agent", "7"), ("terminal", "9"), ("agent", "9")], "7") == "agent", "every pane is in agent view: whichever is in front"
    assert V([("terminal", "7"), ("agent", "9")], "7") == "other" and V([("agent", "7"), ("agent", None)], "7") == "agent"
    # mode_now: 'other' goes to the screen; no reader = no verdict; the pane is noted once per app instance
    c = types.SimpleNamespace(view_pane=(100, "7"), log_view_rows=lambda: [("agent", "7"), ("terminal", "9")], ocr=lambda tag: "Ask anything")
    assert cases.Ctx.mode_now(c, "t") == "agent"
    c.ocr = lambda tag: "run commands, or ask the agent"
    assert cases.Ctx.mode_now(c, "t") == "terminal"
    c.ocr = lambda tag: None
    assert cases.Ctx.mode_now(c, "t") is None
    c.log_view_rows = lambda: [("agent", "7"), ("terminal", "9"), ("terminal", "7")]
    c.ocr = lambda tag: (_ for _ in ()).throw(AssertionError("OCR used although the own pane's line answers"))
    assert cases.Ctx.mode_now(c, "t") == "terminal"
    n = types.SimpleNamespace(view_pane=None, log_view_rows=lambda: [("terminal", "3"), ("agent", "7")])
    cases.Ctx.note_view_pane(n, 100)
    assert n.view_pane == (100, "7")
    n.log_view_rows = lambda: [("agent", "7"), ("agent", "9")]
    cases.Ctx.note_view_pane(n, 100)
    assert n.view_pane == (100, "7"), "kept for the instance"
    cases.Ctx.note_view_pane(n, 200)
    assert n.view_pane == (200, "9"), "another app instance: its own pane ids"
    n.log_view_rows = lambda: [("terminal", "4")]
    cases.Ctx.note_view_pane(n, 300)
    assert n.view_pane is None, "no agent line: not known, mode_now reads the latest line as before"
    run = open(os.path.join(HERE, "run.py"), encoding="utf-8").read()
    assert run.index("ctx.note_view_pane(pid_start)") < run.index('seen = ctx.mode_now("mode_after_" + c["name"])')
    assert 'format!(\n        "mode={} pane={}"' in _src("app", "src", "ai", "blocklist", "agent_view", "controller.rs")


@check
def r19_relay_write_comes_with_the_first_new_prompt():
    """subshell_ready_relay_shape: behind a relay the setup is written at the first NEW prompt (the app's rule), so the case requires the write
    after Ctrl-I or, when that alone did not bring it, after an Enter on an EMPTY line (round 21, #2141: a typed command line is cancelled by the
    app's rule R2, so the case's own line comes after the write), and prints the judge's and the line-end lines when it never comes"""
    import cases  # noqa: F401
    import cases_subshell_ready as SR
    body = inspect.getsource(SR.c_ready_relay_shape)
    assert body.index('ctx.be.key("Return")') < body.index('"the setup was written behind sudo') < body.index("sh -c 'id -u; echo %s' >> %s")
    assert "relay_write" in body and "subshell_ready_judged" in body and "subshell_ready_line_end" in body and "subshell_setup_cancelled" in body
    rules = _src("app", "src", "terminal", "view", "subshell_ready.rs")
    assert "Decision::Wait(WaitWhy::RelayNoPrompt)" in rules and "SubshellTtyKind::Relay =>" in rules
    # the rule the round 19 form of the case ran into: a non-empty line at a shell prompt cancels, an empty one re-arms
    assert "if !non_empty || program_owns_tty {\n        return LineEndVerdict::ReArm;" in rules and "_ => LineEndVerdict::Cancel," in rules
    assert SR.relay_write_when(None, None) == "never" and SR.relay_write_when(3, None) == "at_arm" and SR.relay_write_when(None, 9) == "after_empty_enter"
    assert SR.relay_write_when(3, 9) == "at_arm"


@check
def r19_documented():
    """docs/testing.md names what changed in this round and this file"""
    doc = _src("docs", "testing.md")
    sec = doc[doc.index("## Round 19 tabs"):]
    for word in ("#2108", "#2109", "#2110", "`find_enter`", "`hop=native_modal`", "`close_until_first_tab`", "`left_of_pane`", "`warpui_core::core::app=info`",
                 "`rs_store`", "`note_view_pane`", "`relay_write`", "PHOSPHOR_MODAL_FOCUS_BACK", "selftest_r19_tabs.py"):
        assert word in sec, "docs/testing.md (Round 19 tabs) does not mention %s" % word
