# appci/selftest_r28_ctrlcnu.py: offline self-tests of the #2187 fix to lab_ctrl_c_no_esc_i (r27b: nu lost the last typed line of the x2/x3 rounds because the
# case typed inside the second prompt cycle its own Ctrl-C burst caused). Pure: the settle time and the case's use of it. No display, no lab.
import os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


@check
def t_ctrl_c_burst_settle():
    import lab as L
    assert L.ctrl_c_burst_settle_s(1, 0, 0.15) == 0.0, "one Ctrl-C draws no second prompt: its round still types right behind the first prompt"
    assert L.ctrl_c_burst_settle_s(1, 200, 0.15) == 0.0
    assert abs(L.ctrl_c_burst_settle_s(2, 50, 0.15) - (0.05 + 0.15 + 0.5)) < 1e-9
    assert abs(L.ctrl_c_burst_settle_s(3, 200, 0.15) - (0.4 + 0.15 + 0.5)) < 1e-9
    assert L.ctrl_c_burst_settle_s(2, 0, 0.0) == 0.5
    assert L.ctrl_c_burst_settle_s(3, 50, 0.0) > L.ctrl_c_burst_settle_s(2, 50, 0.0)


@check
def t_ctrl_c_case_waits_for_the_burst():
    src = open(os.path.join(HERE, "cases_lab.py")).read()
    body = src[src.index("def c_ctrl_c_no_esc_i"):src.index("# ------------------------------------------------------------------ 7. The helper install itself")]
    assert "burst_at = time.monotonic()" in body and "L.ctrl_c_burst_settle_s(nctl, gap, rt)" in body
    assert body.index("L.ctrl_c_burst_settle_s") < body.index("tok = unique(ctx, 8)"), "the wait comes before the next line is typed"
    assert "no prompt after the Ctrl-C burst" in body, "the prompt check is still there"
