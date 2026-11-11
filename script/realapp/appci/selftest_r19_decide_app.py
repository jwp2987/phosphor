# appci/selftest_r19_decide_app.py: offline self-tests of the owner decisions of 2026-11-11 made in the app (docs/design/r10-r19-decide-app.md; #2130
# the word keys, #2131 the idle Ctrl-C copy on Windows, #2132 the editor tab's close question by keyboard). Pure Python: no app, no display.
# selftest.py appends CHECKS to its own list (two lines at its end), so `run.py --self-test` runs them. Every judge is fed a right input AND the
# wrong ones it exists to catch; the Rust the cases rest on is pinned by its text, so a change there fails here.
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
def r19_decide_word_keys_per_platform_2130():
    """the raw half of meta_and_word_keys expects xterm's Ctrl+arrows off macOS and readline's Meta letters on macOS, whatever the Meta setting says
    off macOS; the old form is named in the failure text"""
    import cases  # noqa: F401
    import cases_daily_b as D
    for meta in (False, True):
        other = {x[0]: x[2] for x in D.raw_expect("other", meta)}
        assert (other["Ctrl-Left"], other["Ctrl-Right"]) == (b"\x1b[1;5D", b"\x1b[1;5C"), meta
        assert "Option-Left" not in other
    mac = {x[0]: x[2] for x in D.raw_expect("mac", False)}
    assert (mac["Option-Left"], mac["Option-Right"]) == (b"\x1bb", b"\x1bf") and "Ctrl-Left" not in mac
    mac_meta = {x[0]: x[2] for x in D.raw_expect("mac", True)}
    assert (mac_meta["Option-Left"], mac_meta["Option-Right"]) == (b"\x1b[1;3D", b"\x1b[1;3C")
    assert "PHOSPHOR_WORD_KEYS_XTERM=0" in D.word_key_hint(D.WORD_LEFT_BYTES, D.XTERM_CTRL_LEFT_BYTES)
    assert "word_key_hint(got, expect)" in inspect.getsource(D.c_meta_and_word_keys)
    esc = _src("crates", "warp_terminal", "src", "model", "escape_sequences.rs")
    assert "pub fn word_keys_xterm_enabled_for(value: Option<&str>) -> bool {\n    value != Some(\"0\")\n}" in esc
    tests = _src("crates", "warp_terminal", "src", "model", "escape_sequences_test.rs")
    for name in ("word_keys_are_xterm_ctrl_arrows_off_macos_and_meta_letters_on_macos", "word_keys_off_macos_match_the_encoder_for_ctrl_arrows",
                 "the_word_keys_kill_switch_is_off_only_for_zero"):
        assert "fn %s()" % name in tests, name


@check
def r19_decide_idle_ctrl_c_copy_2131():
    """Windows, Ctrl-C at an idle prompt with a word selected: it copies the word and drops the selection; the copy key pressed next must not give
    the word alone again"""
    import cases  # noqa: F401
    import cases_pointer as P
    S = "SENTINEL"
    assert P.idle_ctrl_c_copy_errors("word\n", S, "word", S) == [], "copied, and the copy key then had nothing to copy"
    assert P.idle_ctrl_c_copy_errors("word", "cat f.txt\nalpha word beta\n", "word", S) == [], "a selected block is not the selection"
    kept = P.idle_ctrl_c_copy_errors("word", "word\n", "word", S)
    assert len(kept) == 1 and "#2131" in kept[0] and P.IDLE_COPY_SWITCH + "=0" in kept[0], "the behaviour before the decision fails"
    assert "nothing was copied" in P.idle_ctrl_c_copy_errors(S, S, "word", S)[0]
    assert "expected the selected text" in P.idle_ctrl_c_copy_errors("other", S, "word", S)[0]
    assert len(P.idle_ctrl_c_copy_errors(S, "word", "word", S)) == 2, "not a copy AND the selection still up: both are said"
    arm = inspect.getsource(P.idle_ctrl_c_arm)
    assert "if not U.is_windows(ctx) or copied.strip() != word:\n        return" in arm, "Windows only, and only with the word proven selected"
    assert arm.index('ctx.be.key("ctrl+c")') < arm.index('ctx.be.key(U.K(ctx, "copy"))') < arm.index("idle_ctrl_c_copy_errors(first, again, word, SENTINEL)")
    assert arm.count("ctx.be.clip_set(SENTINEL)") == 2, "a fresh sentinel before each key"
    assert 'if ctx.mode == "agent" and first == SENTINEL:' in arm and "was NOT judged" in arm, "the agent view's own idle Ctrl-C is a note, never a pass"
    body = inspect.getsource(P.c_selection_modes)
    assert body.index("copy_after(ctx, lambda: m.click(found[w][1], found[w][2], 1, 2)") < body.index("idle_ctrl_c_arm(ctx, r, got, w)") < body.index("# triple click: the line")
    view = _src("app", "src", "terminal", "view.rs")
    assert "fn ctrl_c_copy_drops_text_selection(command_in_front: bool, idle_clears: bool) -> bool {\n    command_in_front || idle_clears\n}" in view
    assert 'std::env::var("%s")' % P.IDLE_COPY_SWITCH in view
    tests = _src("app", "src", "terminal", "view_test.rs")
    for name in ("ctrl_c_copy_drops_the_text_selection_with_a_command_in_front", "ctrl_c_copy_drops_the_text_selection_at_an_idle_prompt",
                 "the_idle_ctrl_c_copy_kill_switch_is_off_only_for_zero"):
        assert "fn %s()" % name in tests, name


@check
def r19_decide_close_question_by_keyboard_2132():
    """the editor tab's close question: the walk Tab, Tab, Shift+Tab must be seen on Don't Save before Space is sent; Space must press Don't Save
    and leave the saved text; Enter alone stays Save"""
    import cases  # noqa: F401
    import cases_r15_w5a as W
    log = ("a input_order hop=native_modal event=shown t=1\n"
           "b input_order hop=native_modal event=selected button=1 t=2\n"
           "c input_order hop=native_modal event=selected button=2 t=3\n"
           "d input_order hop=native_modal event=selected button=1 t=4\n"
           "e input_order hop=native_modal event=pressed button=1 by=space t=5\n"
           "f input_order hop=native_modal event=closed focus_back=false t=6\n")
    assert W.modal_selected(log) == [1, 2, 1] == W.DONT_SAVE_WALK and W.modal_pressed(log) == [(1, "space")]
    assert W.modal_selected("") == [] and W.modal_selected(None) == [] and W.modal_pressed(None) == []
    assert W.close_question_up(log) and not W.close_question_up(log.split("\n", 1)[1]), "the new lines are not 'a question came up'"
    assert (W.CLOSE_Q_SAVE, W.CLOSE_Q_DONT_SAVE, W.CLOSE_Q_CANCEL) == (0, 1, 2) and W.DONT_SAVE_KEYS == ("Tab", "Tab", "shift+Tab")
    assert W.dont_save_walk_verdict([1, 2, 1])[0] == "on_dont_save"
    none = W.dont_save_walk_verdict([])
    assert none[0] == "no_selection" and "#2132" in none[1] and W.CLOSE_KEYS_SWITCH + "=0" in none[1], "the build before the decision: Tab moves nothing"
    assert W.dont_save_walk_verdict([1, 2])[0] == "elsewhere", "Shift+Tab lost: the selection is on Cancel"
    assert W.dont_save_walk_verdict([1, 2, 0])[0] == "elsewhere" and W.dont_save_walk_verdict([1])[0] == "elsewhere"
    kept, edit = "KEEP_text", "DROP_KEEP_text"
    assert W.dont_save_verdict([(1, "space")], kept, kept, edit)[0] == "discarded"
    assert W.dont_save_verdict([], kept, kept, edit)[0] == "not_pressed", "Space is not a key of the question"
    assert W.dont_save_verdict([(0, "space")], edit, kept, edit)[0] == "wrong_button", "Space pressed Save"
    assert W.dont_save_verdict([(1, "enter")], kept, kept, edit)[0] == "wrong_button", "pressed, but not by Space"
    assert W.dont_save_verdict([(1, "space")], edit, kept, edit)[0] == "saved" and W.dont_save_verdict([(1, "space")], "x", kept, edit)[0] == "other"
    arm = inspect.getsource(W._arm_close_question)
    assert (arm.index("for spec in DONT_SAVE_KEYS:") < arm.index("dont_save_walk_verdict(sel)") < arm.index("Space was NOT sent") < arm.index('ctx.be.key("space")')
            < arm.index("dont_save_verdict(pressed, end, t1 + R.EDIT_ORIG, t2 + t1 + R.EDIT_ORIG)") < arm.index('open_editor(ctx, r, path, "close_enter")')
            < arm.index('ctx.be.key("Return")') < arm.index('pressed == [(CLOSE_Q_SAVE, "enter")]'))
    assert 'ctx.shot("edd_close_dont_save")' in arm and arm.count('ctx.be.key("Escape")') >= 3, "every refused answer is cancelled, never guessed"
    nm = _src("app", "src", "workspace", "native_modal.rs")
    assert 'FixedBinding::new(\n            "space",\n            NativeModalAction::PressSelected,\n            id!("NativeModal"),\n        ),' in nm
    assert 'format!("event=selected button={index}")' in nm and 'format!("event=pressed button={index} by={by}")' in nm
    for by in ('"pointer"', '"escape"', '"enter"', '"space"'):
        assert by in nm, by
    assert "fn space_button_index(selected: Option<usize>) -> Option<usize> {\n    selected\n}" in nm
    assert "fn has_focus_ring(selected: Option<usize>, index: usize) -> bool {\n    selected == Some(index)\n}" in nm
    assert ".with_style(if has_focus_ring(self.selected_button, i) {" in nm and "border_width: Some(FOCUS_RING_WIDTH)," in nm
    qw = _src("app", "src", "quit_warning", "mod.rs")
    assert 'std::env::var("%s")' % W.CLOSE_KEYS_SWITCH in qw
    assert "    if is_editor_tab && enabled {\n        save_index\n    } else {\n        None\n    }" in qw
    assert "matches!(state.scope, QuitScope::EditorTab { .. })," in qw and "Some(index) => dialog.with_enter_button(index)," in qw
    assert qw.index("save_index = Some(buttons.len());") < qw.index('crate::t!("common-save"), callback') < qw.index('crate::t!("quit-warning-dont-save")') < qw.index('crate::t!("common-cancel"), callback')
    # the Windows and macOS injectors can post the keys (the X11 / Wayland ones take keysym names)
    import backend
    for spec in W.DONT_SAVE_KEYS + ("space",):
        k = spec.lower().split("+")[-1]
        assert k in backend.WindowsBackend.NAMED, spec


@check
def r19_decide_documented():
    """docs/testing.md names the three decisions, their switches, the changed expectations and this file; DECLINED.md has the IMPROVED rows"""
    doc = _src("docs", "testing.md")
    sec = doc[doc.index("## Round 19: owner decisions of 2026-11-11 in the app"):]
    for word in ("#2130", "#2131", "#2132", "`word_key_hint`", "`idle_ctrl_c_arm`", "`idle_ctrl_c_copy_errors`", "`dont_save_walk_verdict`", "`dont_save_verdict`",
                 "event=selected button=<i>", "event=pressed button=<i> by=<pointer|enter|space|escape>", "selftest_r19_decide_app.py", "r10-r19-decide-app.md"):
        assert word in sec, "docs/testing.md (Round 19: owner decisions) does not mention %s" % word
    for switch in ("PHOSPHOR_WORD_KEYS_XTERM=0", "PHOSPHOR_WIN_CTRL_C_COPY_CLEARS_IDLE=0", "PHOSPHOR_CLOSE_QUESTION_KEYS=0"):
        assert "| `%s` |" % switch in doc, "docs/testing.md section 2.6 has no row for %s" % switch
    dec = _src("DECLINED.md")
    for word in ("(#2130) — IMPROVED", "(#2117, #2131) — IMPROVED", "(#2132) — IMPROVED", "keep:word_key_sequence_for", "keep:close_question_enter_button",
                 "keep:ctrl_c_copy_drops_the_text_selection_at_an_idle_prompt"):
        assert word in dec, "DECLINED.md does not have %s" % word
    assert os.path.isfile(os.path.join(REPO, "docs", "design", "r10-r19-decide-app.md"))
