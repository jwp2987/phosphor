# appci/selftest_r27_loganchors.py: offline self-tests of the r27 log-anchor fixes (r27a Wayland false FAIL, r27a macOS link_and_path_click). Pure Python.
import os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


REAL = "2026-11-12T19:35:37Z [DEBUG] input_order hop=view_mode mode=agent pane=1496 t=1794512137874.483\n"
# the app logged one message holding a git diff whose added lines are OLD harness log lines (#2191)
DIFF = ("2026-11-12T19:48:55Z [INFO] Failed to get file diff for a/b.log: diff --git a/b.log b/b.log\n"
        "index 111..222 100644\n"
        "--- a/b.log\n"
        "+++ b/b.log\n"
        "@@ -1,2 +1,3 @@\n"
        " context line\n"
        "+2026-11-12T19:59:50Z [DEBUG] input_order hop=view_mode mode=terminal pane=3928 t=1794513590169.769\n"
        "-2026-11-12T19:59:50Z [DEBUG] input_order hop=view_mode mode=terminal pane=3928 t=1\n")


@check
def r27_diff_body_hop_line_does_not_change_view_mode():
    import cases as C
    rows = C.view_mode_rows(REAL + DIFF)
    assert rows == [("agent", "1496")], rows
    assert C.VIEW_MODE_RE.findall(DIFF) == []
    assert C.view_mode_verdict(C.view_mode_rows(REAL + DIFF), "1496") == "agent"
    # a real later line still wins
    later = "2026-11-12T20:00:00Z [DEBUG] input_order hop=view_mode mode=terminal pane=1496 t=2\n"
    assert C.view_mode_rows(REAL + DIFF + later)[-1] == ("terminal", "1496")


@check
def r27_mode_now_reads_the_real_hop_not_the_echo(tmp=None):
    import cases as C, tempfile
    d = tempfile.mkdtemp()
    p = os.path.join(d, "phosphor.log")
    open(p, "w").write(REAL + DIFF)
    ctx = C.Ctx.__new__(C.Ctx)
    ctx.log_path = p
    assert ctx.log_view_mode() == "agent"
    assert ctx.log_view_rows() == [("agent", "1496")]
    off = 0
    assert "hop=view_mode mode=terminal" not in ctx.log_since(off)
    os.remove(p)
    os.rmdir(d)


@check
def r27_clean_keeps_real_lines_and_drops_continuations():
    import logclean as L
    out = L.app_log_clean(REAL + DIFF)
    assert REAL in out and "mode=terminal" not in out
    assert out.count("\n") == 2         # the real view_mode line and the 'Failed to get file diff' first line
    assert L.app_log_clean("") == "" and L.app_log_clean(None) == ""


@check
def r27_anchored_hop_regexes():
    import logclean as L, replay
    rx = L.log_hop_rx(r"hop=kh_x v=(\d)")
    assert rx.findall("2026-11-12T19:35:37Z [DEBUG] input_order hop=kh_x v=1\n+2026-11-12T19:35:37Z [DEBUG] input_order hop=kh_x v=2\n") == ["1"]
    assert replay.VIEW_RE.findall(REAL + DIFF) == ["agent"]


@check
def r27_focus_lost_hops_do_not_count():
    import cases_pointer as P
    lost = ("2026-11-12T19:58:41Z [DEBUG] input_order hop=kh_focus_input seq=373 self_focused=false t=1\n"
            "2026-11-12T19:58:41Z [DEBUG] input_order hop=kh_focus_terminal_view seq=373 self_focused=false ssh_holding=false t=2\n")
    assert P.pane_focus_state(lost) is None
    gained = "2026-11-12T19:58:42Z [DEBUG] input_order hop=kh_focus_input seq=374 self_focused=true t=3\n"
    assert P.pane_focus_state(lost + gained) == "shell"
    assert P.pane_focus_state(gained + lost) == "shell"          # a later LOST hop never takes the state back
    echoed = "+2026-11-12T19:58:42Z [DEBUG] input_order hop=kh_focus_input seq=1 self_focused=true t=3\n"
    assert P.pane_focus_state(echoed) is None


@check
def r27_pane_path_always_closes():
    """the close verdict does not look at focus hops at all: a pane that opened is closed by the key and judged by the screen"""
    import cases_pointer as P, inspect
    assert P.close_code_pane_verdict("cmd+w", {}, True)[0]
    assert not P.close_code_pane_verdict("cmd+w", {"x": 1}, True)[0]
    assert not P.close_code_pane_verdict("", {}, True)[0]
    src = inspect.getsource(P.links_play)
    assert 'if st == "editor"' not in src and "R.CLOSE_PANE" in src and "close_code_pane_verdict(R.CLOSE_PANE, still, have_ocr)" in src
