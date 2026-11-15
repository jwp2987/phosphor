# appci/selftest_r31_1887.py: the pure helpers of the #1887 / #1890 Windows cases (cases_win_input.py) and their registration. No display, no app.
import os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import cases  # noqa: E402
import cases_win_input as W  # noqa: E402
import winmap  # noqa: E402

CHECKS = []


def check(fn):
    CHECKS.append(fn)
    return fn


@check
def r31_1887_ctrl_c_wait_reads_keydown_to_the_etx_write():
    log = "\n".join([
        "input_order hop=pty_write_bytes class=Paste wrote=4096 of_job=65536 bytes=<4096 bytes> t=100.0",
        "input_order hop=ctrl_c_keydown t=110.0",
        "input_order hop=pty_write_bytes class=Paste wrote=4096 of_job=65536 bytes=<4096 bytes> t=120.0",
        "input_order hop=pty_write_bytes class=UserKey wrote=1 of_job=1 bytes=<1 bytes> t=131.5",
    ])
    assert W.ctrl_c_wait_ms(log) == (21.5, "pty_write_bytes", 1), W.ctrl_c_wait_ms(log)
    fallback = "input_order hop=ctrl_c_keydown t=10.0\ninput_order hop=ctrl_c_pty_write bytes=1 t=14.25"
    assert W.ctrl_c_wait_ms(fallback) == (4.25, "ctrl_c_pty_write", 1)
    assert W.ctrl_c_wait_ms("input_order hop=ctrl_c_keydown t=1.0") == (None, None, 1)
    assert W.ctrl_c_wait_ms("nothing") == (None, None, 0)


@check
def r31_1887_net_hand_back_is_told_from_a_release():
    net = "x input_order hop=prompt_hold_handed_back hold=3 kind=PowershellSetup why=abort_marker_give_up chunks=1 bytes=9"
    other = "x input_order hop=prompt_hold_handed_back hold=4 kind=LineEditorActive why=cap chunks=1 bytes=9"
    assert len(W.handed_back_by_net(net + "\n" + other)) == 1
    assert W.handed_back_by_net(other) == []


@check
def r31_1887_payload_is_64k_of_short_lines():
    p = W.big_paste_payload()
    assert len(p) >= 64 * 1024 and max(len(l) for l in p.split("\n")) < 200


@check
def r31_1887_cases_are_registered_proving_and_mapped_for_windows():
    names = [c["name"] for c in cases.CASES]
    for n in ("ctrl_c_behind_big_paste", "powershell_setup_ctrl_c_keys"):
        assert n in names and n in cases.PROVING and n not in cases.SMOKE, n
        assert n in winmap.VARIANT_CASES["pwsh7"], n
    assert "ctrl_c_behind_big_paste" in winmap.BOTH
    assert "powershell_setup_ctrl_c_keys" in winmap.PS_ONLY


@check
def r31b_ctrl_c_case_types_its_follow_up_only_after_the_prompt_returns():
    import inspect
    src = inspect.getsource(W.c_ctrl_c_behind_big_paste)
    assert "precmd_after_ctrl_c" in src
    assert src.index("precmd_after_ctrl_c") < src.index("ctx.append_cmd(ctx.pane_shell(), tok, f)"), "r31b: the follow-up line was typed 70 ms after the ^C, ahead of the prompt"


if __name__ == "__main__":
    bad = 0
    for fn in CHECKS:
        try:
            fn()
            print("ok  ", fn.__name__)
        except Exception as e:  # noqa: BLE001
            bad += 1
            print("FAIL", fn.__name__, repr(e))
    sys.exit(1 if bad else 0)
