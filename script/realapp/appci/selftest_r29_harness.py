# appci/selftest_r29_harness.py: offline self-tests of the r29 harness fixes (Windows mode_proof landing screen; macOS link case closes its code pane
# through the command palette). Pure Python; selftest.py appends CHECKS to its own list.
import os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


@check
def r29_mode_proof_landing_screen_is_not_agent_header():
    """r29a Windows terminal mode_proof false FAIL: the fresh session's landing screen reads 'new terminal session ... start a new agent
    conversation'; the prompt to START an agent conversation matched the header 'new agent conversation'."""
    import cases
    f = lambda t: cases.ocr_agent_markers(" ".join(t.lower().split()))
    rec = "gents, files... New terminal session shift •cpj Start a new agent conversation"      # the recorded OCR text, as far as the row printed it
    assert f(rec)[0] is False, f(rec)
    assert f("gents, files... New terminal session shift •cpj Start a new agent convers")[0] is False      # cut short by the row / OCR
    assert f("new terminal session start a new phosphor agent conversation")[0] is False
    # a real agent view still counts, also next to the landing phrase
    assert f("search tabs... new agent conversation esc for terminal")[0] is True
    assert f("new terminal session start a new agent conversation new agent conversation")[0] is True
    assert f("start a new agent conversation esc for terminal")[0] is True


@check
def r29_link_case_closes_code_pane_by_palette_without_close_key():
    """r29a macOS link_and_path_click: no close key (APPCI_KEY_CLOSE empty) left the pane open. The case now uses the palette close (editor_close)
    and still proves the close by the screen; with neither way it fails without typing."""
    import cases, cases_pointer as P, cases_robust as R

    class Be:
        def __init__(s): s.sent = []
        def key(s, k): s.sent.append(("key", k))
        def type(s, t, *a, **k): s.sent.append(("type", t))

    class Ctx:
        def __init__(s): s.be = Be()

    class Rr:
        def note(s, *a): pass

    keep = {k: os.environ.get(k) for k in ("APPCI_KEY_CLOSE", "APPCI_KEY_CMD_PALETTE")}
    real_sleep, cases.time.sleep = cases.time.sleep, lambda *_a, **_k: None
    try:
        os.environ["APPCI_KEY_CLOSE"] = ""
        os.environ["APPCI_KEY_CMD_PALETTE"] = "cmd+p"
        c = Ctx()
        how = cases.editor_close(c, Rr())
        assert how and how.startswith("palette"), how
        assert c.be.sent[0] == ("key", "cmd+p") and ("type", "Close Current Session") in c.be.sent
        assert P.close_code_pane_verdict(how, {}, True) == (True, "")
        assert not P.close_code_pane_verdict(how, {"line": 1}, True)[0]           # still drawn: not closed
        assert not P.close_code_pane_verdict(how, {}, False)[0]                   # no reader: cannot prove
        os.environ["APPCI_KEY_CMD_PALETTE"] = ""
        c = Ctx()
        assert cases.editor_close(c, Rr()) is None and not c.be.sent
        ok, why = P.close_code_pane_verdict("", {}, True)
        assert not ok and "nothing was typed" in why
    finally:
        cases.time.sleep = real_sleep
        for k, v in keep.items():
            if v is None: os.environ.pop(k, None)
            else: os.environ[k] = v
    src = open(os.path.join(HERE, "cases_pointer.py"), encoding="utf-8").read()
    assert "C.editor_close(ctx, r)" in src and "close_code_pane_verdict(closer, still, have_ocr)" in src
