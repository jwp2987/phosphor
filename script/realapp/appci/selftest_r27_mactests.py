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
    """one stale line of 28 is not drawn output in either mode (#2145, owner decision 2026-11-13: the agent view draws it, so missing output FAILS)"""
    import cases_r14_w1a as W
    assert W.skp_stream_state("agent", 28, 1) == "missing"          # was "agent_hidden" (noref)
    assert W.skp_stream_state("agent", 28, 0) == "missing"          # was "agent_hidden"
    assert W.skp_stream_state("terminal", 28, 12) == "drawn"
    assert W.skp_stream_state("agent", 28, 12) == "drawn"
    assert W.skp_stream_state("terminal", 28, 1) == "missing"
    assert W.skp_stream_state("terminal", 3, 0) == "too_early"


@check
def r31b_skp_tail_hits_tolerates_a_misread_tag_only():
    """r31b Wayland, both modes: the shot showed the job's last lines and the case read 0 of 8. The tag's `c` is read as `e` by OCR; the digits
    decide which line it is, so they stay exact (a view scrolled up must still fail)"""
    import cases_r14_w1a as W
    tail = ["ncaakx", "ncaaky", "ncaama", "ncaamc"]
    assert W.skp_tail_hits(tail, "ncaakx\nneaaky\nnaaama\nncaamc") == tail                      # exact, one tag glyph wrong, one tag glyph wrong, exact
    assert W.skp_tail_hits(["ncaaky"], "neaaky") == ["ncaaky"]
    assert W.skp_tail_hits(["ncaaky"], "neaakt") == []                                           # a neighbour line: another digit glyph
    assert W.skp_tail_hits(["ncaaky"], "nxaaky") == ["ncaaky"] and W.skp_tail_hits(["ncaaky"], "heaaky") == []
    assert W.skp_tail_hits(["ncaaky", ""], "") == [] and W.skp_tail_hits([], "ncaaky") == []
    assert W.skp_ocr_tail_sample("a\n\n b \nc", 2) == ["b", "c"] and W.skp_ocr_tail_sample(None) == []
    import inspect
    body = inspect.getsource(W.c_scroll_keys_position)
    assert "skp_tail_hits(tail, t5)" in body and "skp_ocr_tail_sample(t5)" in body


@check
def r27_skp_verdict_is_judged_in_agent_mode():
    """#2145: the NO-REFERENCE branch is gone; the tail of the job's lines must be on screen after one key in both modes"""
    import cases_r14_w1a as W
    assert W.skp_after_key_verdict("drawn", 3, [480, 499], 500) == ("pass", "")
    assert W.skp_after_key_verdict("missing", 0, [480, 499], 500)[0] == "fail"      # was noref for agent_hidden
    assert W.skp_after_key_verdict("drawn", 0, [], 500)[0] == "fail"


@check
def agent_bg_job_output_verdict_and_registration():
    """#2145: the case is registered (agent mode, PROVING, Git Bash on Windows) and its pure verdict fails each wrong screen"""
    import cases, cases_r14_w1a as W, winmap
    reg = {c["name"]: c for c in cases.CASES}
    assert reg["agent_bg_job_output"]["modes"] == ("agent",) and reg["agent_bg_job_output"]["gui"]
    assert "agent_bg_job_output" in cases.PROVING and "agent_bg_job_output" in winmap.GITBASH_ONLY
    V = W.abj_verdict
    assert V("$ x\nbgdone_acdefh\nmore\n$ echo uwxyka\n", "acdefh", "uwxyka") is None
    assert "not on screen in the agent view" in V("$ echo uwxyka\n", "acdefh", "uwxyka")
    assert "not on screen" in V("bgdone_acdefh\n", "acdefh", "uwxyka")
    assert "2 times" in V("bgdone_acdefh\nbgdone_acdefh\necho uwxyka\n", "acdefh", "uwxyka")
    assert "below" in V("$ echo uwxyka\nbgdone_acdefh\n", "acdefh", "uwxyka")


@check
def r27_pane_focus_state_from_r25a_hops():
    import cases_pointer as P
    pane = "2026-11-12T19:58:41Z [DEBUG] input_order hop=kh_focus_editor seq=1 self_focused=true\n"
    assert P.pane_focus_state(pane) == "editor"
    back = pane + "2026-11-12T19:58:42Z [DEBUG] input_order hop=kh_focus_input seq=2 self_focused=true\n"
    assert P.pane_focus_state(back) == "shell"
    assert P.pane_focus_state("hop=kh_pane_key target=Editor") is None


@check
def r27_close_code_pane_verdict():
    import cases_pointer as P
    assert P.close_code_pane_verdict("cmd+w", {}, True) == (True, "")
    ok, why = P.close_code_pane_verdict("cmd+w", {"tok": 1}, True)
    assert not ok and "nothing was typed" in why and "tok" in why
    ok, why = P.close_code_pane_verdict("", {}, True)
    assert not ok and "no close-pane key" in why
    ok, why = P.close_code_pane_verdict("cmd+w", {}, False)
    assert not ok and "cannot be proved" in why


@check
def r27_path_verdicts_unchanged():
    import cases_pointer as P
    assert P.path_open_verdict([], "/a/b.rs", True, True) == ("pane", None)
    assert P.path_open_verdict([], "/a/b.rs", False, True)[1]
    assert P.path_open_verdict([], "/a/b.rs", False, False) == (None, "nothing opened: no call to the desktop opener and the file's text is not on screen")
