# appci/selftest_r27_mactests.py: offline self-tests of the r25a macOS test-fault fixes. Pure Python, no display.
import os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


@check
def r27_skp_stale_line_is_not_drawn():
    """r25a macOS agent: 1 of 28 printed lines read is hidden output (#2145), not drawn; terminal 12 of 28 stays drawn"""
    import cases_r14_w1a as W
    assert W.skp_stream_state("agent", 28, 1) == "agent_hidden"
    assert W.skp_stream_state("agent", 28, 0) == "agent_hidden"
    assert W.skp_stream_state("terminal", 28, 12) == "drawn"
    assert W.skp_stream_state("agent", 28, 12) == "drawn"
    assert W.skp_stream_state("terminal", 28, 1) == "missing"
    assert W.skp_stream_state("terminal", 3, 0) == "too_early"


@check
def r27_skp_hidden_uses_numbered_lines_and_names_2145():
    import cases_r14_w1a as W
    v, t = W.skp_after_key_verdict("agent_hidden", 0, [480, 499], 500)
    assert v == "noref" and "#2145" in t
    assert W.skp_after_key_verdict("agent_hidden", 0, [10, 20], 500)[0] == "fail"      # the numbered-lines check is not loosened
    assert W.skp_after_key_verdict("agent_hidden", 0, [], 500)[0] == "fail"


@check
def r27_pane_focus_state_from_r25a_hops():
    import cases_pointer as P
    pane = "x hop=kh_focus_editor seq=1 self_focused=true\n"
    assert P.pane_focus_state(pane) == "editor"
    back = pane + "hop=kh_focus_input seq=2 self_focused=true\nhop=kh_focus_terminal_view seq=2\n"
    assert P.pane_focus_state(back) == "shell"
    assert P.pane_focus_state("hop=kh_pane_key target=Editor") is None
    assert P.pane_focus_state(back + "hop=kh_focus_editor seq=3") == "editor"


@check
def r27_close_code_pane_verdict():
    import cases_pointer as P
    assert P.close_code_pane_verdict("editor", "shell", "cmd+w") == (True, "")
    ok, why = P.close_code_pane_verdict("editor", "editor", "cmd+w")
    assert not ok and "nothing was typed" in why
    ok, why = P.close_code_pane_verdict("editor", None, "")
    assert not ok and "no close-pane key" in why


@check
def r27_path_verdicts_unchanged():
    import cases_pointer as P
    assert P.path_open_verdict([], "/a/b.rs", True, True) == ("pane", None)
    assert P.path_open_verdict([], "/a/b.rs", False, True)[1]
    assert P.path_open_verdict([], "/a/b.rs", False, False) == (None, "nothing opened: no call to the desktop opener and the file's text is not on screen")
