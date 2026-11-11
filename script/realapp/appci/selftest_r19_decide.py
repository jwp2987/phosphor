# appci/selftest_r19_decide.py: offline self-tests of the three owner decisions of 2026-11-11 in the harness (docs/design/r19-decide-tests.md; #2114, #2120).
#   (4) Windows big_paste / remote_heredoc_paste: a control that fails too makes the row NOREF with the control's numbers; a control that passes leaves the FAIL.
#   (5) latency_enter on Windows Git Bash: the regression gate judges the app's own segment (enter_begin -> pty_write); the median is reported; per-hop breakdown.
#   (9) dash / ash hosts: the agent-mode lab arms report the plain session as the named known limitation; terminal-mode arms are judged normally.
# Pure Python: no app, no display. selftest.py appends CHECKS to its own list, so `run.py --self-test` runs them.
# Every rule is fed the input it exists for AND the inputs on which it must still FAIL.
import inspect, os, sys, types

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.join(HERE, "..", "..", "..")
sys.path.insert(0, HERE)

import cases  # noqa: E402
import lat  # noqa: E402

CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


def _doc():
    return open(os.path.join(REPO, "docs", "testing.md"), encoding="utf-8").read()


# ------------------------------------------------------------------ (4) big_paste
class _PromptCtx:
    """the part of Ctx big_paste_back_at_prompt uses: a prompt counter that may or may not grow, the Ctrl-C it presses"""
    EXIT_WINDOW_S = 0.0
    mode = "terminal"

    def __init__(self, comes_back_after=None):
        self.n, self.keys, self.polls, self.after = 5, [], 0, comes_back_after
        self.be = types.SimpleNamespace(key=lambda k: self.keys.append(k))

    def prompts(self):
        return self.n

    def until(self, pred, timeout, step=0.05):
        self.polls += 1
        if self.after is not None and self.polls > self.after:
            self.n += 1
        return pred()


@check
def r19_big_paste_no_prompt_is_noref_only_when_the_control_fell_short_too():
    """(4) big_paste: "the shell never came back to a prompt" is NOREF with the conhost control's numbers when that control fell short of the paste too;
    with no such control result it is the FAIL it always was; a shell that does come back is neither"""
    real_sleep = cases.time.sleep
    cases.time.sleep = lambda s: None
    try:
        ctl = "conhost Git Bash received 2 of 1000001 bytes"
        r = cases.Result("big_paste", "terminal@gitbash")
        assert cases.big_paste_back_at_prompt(_PromptCtx(), r, 5, ctl) is False
        assert not r.fails and len(r.noref) == 1, (r.fails, r.noref)
        assert ctl in r.noref[0] and "never came back to a prompt" in r.noref[0] and "PLATFORM by the control" in r.noref[0], r.noref
        # the same pane with NO control result (the control got every byte, could not run, or the paste was byte-exact): a FAIL
        for no_control in (None, ""):
            r = cases.Result("big_paste", "terminal@gitbash")
            ctx = _PromptCtx()
            assert cases.big_paste_back_at_prompt(ctx, r, 5, no_control) is False
            assert len(r.fails) == 1 and "never came back to a prompt" in r.fails[0] and not r.noref, (r.fails, r.noref)
            assert ctx.keys == ["ctrl+c"] * 3, ctx.keys
        # back at once, and back after the second Ctrl-C: no row text at all, with or without a control result
        for after, presses in ((0, 0), (2, 2)):
            for c in (None, ctl):
                r = cases.Result("big_paste", "terminal@gitbash")
                ctx = _PromptCtx(comes_back_after=after)
                assert cases.big_paste_back_at_prompt(ctx, r, 5, c) is True and not r.fails and not r.noref and len(ctx.keys) == presses, (after, c, ctx.keys)
        r = cases.Result("big_paste", "terminal")
        assert cases.big_paste_back_at_prompt(_PromptCtx(), r, None, ctl) is True and not r.fails and not r.noref, "a pane with no prompt.log is not judged"
    finally:
        cases.time.sleep = real_sleep
    # the case hands the helper the control's result ONLY for "same-shortfall", and relaunches the app itself when the pane did not come back
    src = inspect.getsource(cases.c_big)
    body = src[src.index('if kind == "same-shortfall":'):src.index('elif kind == "none":')]
    assert "control_fell_short = why" in body
    assert src.count("control_fell_short = ") == 2 and "control_fell_short = None" in src, "set in one branch only: 'none' and 'exact' leave the FAIL"
    assert "back = big_paste_back_at_prompt(ctx, r, nb, control_fell_short)" in src
    assert "if not back and control_fell_short and not relaunched:" in src and "cases_robust.start_app(ctx, r," in src
    assert 'the conhost control received every byte' in src and "r.check(False," in src[src.index('elif kind == "none":'):], "control exact: the byte FAIL stands"
    # the UI-item budget is not waived by the control (it is the app's own main thread)
    assert cases.judge_big_paste_time(None, 900.0, False), "over the UI-item budget on a short paste: still a failure message"


# ------------------------------------------------------------------ (4) remote_heredoc_paste
def _rec(name, want=10001, timeout=60):
    return dict(label="bash: 10 KB single line" if name == "hd_line.txt" else "bash: 2000-line here-doc", name=name, sent=10032, want=want, got=None, secs=66.5,
                late=None, timeout=timeout, peak={}, wrapped_len=14000, ok=False, exact=False, first=None, sshd_off=0, timed_out=True)


def _ctl(got=-1, exact=False, s=None, why=None, sshd_got=None, sent=10032, want=10001):
    return dict(label="reference console + ssh", sent=sent, want=want, s=s, got=got, exact=exact, peak={}, out=0, why=why, sshd_got=sshd_got)


@check
def r19_heredoc_single_line_is_judged_against_a_control_of_the_same_line():
    """(4) remote_heredoc_paste on Windows: the 10 KB single-line step has its own control (L, the same line in the reference console's ssh). L fails too ->
    platform (NOREF naming L's numbers); L delivers -> app (FAIL); no L, or an L that could not run or sent nothing -> no evidence, the plain FAIL stands"""
    import cases_caseremote as CR
    h_fail = _ctl(why="no file within 120 s", sent=142223, want=142166)
    line = _rec("hd_line.txt")
    # the here-doc control alone never covers the line (the rule that made the row FAIL every run, kept)
    cls, why = CR.classify_timeout(line, {"H": h_fail}, None)
    assert cls is None and "does not cover it" in why, (cls, why)
    # L did not deliver either: platform, and the text carries the control's own numbers
    l_fail = _ctl(why="no file within 60 s; test sshd: 10104 client bytes received of the 10032-byte paste", sshd_got=10104)
    cls, why = CR.classify_timeout(line, {"H": h_fail, "L": l_fail}, None)
    assert cls == "platform" and "10104 client bytes received" in why and "no file within 60 s" in why and "same 60 s budget" in why, (cls, why)
    l_short = _ctl(got=4096, sshd_got=4200)
    cls, why = CR.classify_timeout(line, {"H": h_fail, "L": l_short}, None)
    assert cls == "platform" and "4096 of 10001 bytes" in why and "4200 client bytes" in why, (cls, why)
    # L delivered the line: Phosphor is judged as before, a FAIL of class app
    cls, why = CR.classify_timeout(line, {"H": h_fail, "L": _ctl(got=10001, exact=True, s=3.2)}, None)
    assert cls == "app" and "delivered the same 10032-byte single line in 3.2 s" in why, (cls, why)
    # L could not run, or the reference console pasted nothing at all: it proves nothing
    for bad in (_ctl(why="reference console not available: no conhost"), _ctl(why="control failed: RuntimeError: focus"),
                _ctl(why="no file within 60 s; test sshd: 0 client bytes received of the 10032-byte paste", sshd_got=0)):
        cls, why = CR.classify_timeout(line, {"H": h_fail, "L": bad}, None)
        assert cls is None, (bad["why"], cls, why)
    # an L in the control does not change how the here-doc steps are judged, and H still does not cover a third kind of step
    for name in CR.BASH_WRAPPER_STEPS:
        cls, why = CR.classify_timeout(_rec(name, want=142166, timeout=90), {"H": h_fail, "L": l_fail}, None)
        assert cls == "platform" and "2,000-line here-doc" in why and "no file within 120 s" in why, (name, cls, why)
    assert CR.classify_timeout(_rec("hd_2k.txt"), {"H": _ctl(got=142166, exact=True, s=9.0), "L": l_fail}, None)[0] == "app"
    assert CR.classify_timeout(_rec("hd_zsh.txt"), {"H": h_fail, "L": l_fail}, None)[0] is None
    # POSIX (a W control present) is untouched: the line step is "not a bash wrapper-path step"
    w = dict(_ctl(got=1, exact=True, s=5.0), line=190000)
    assert CR.classify_timeout(line, {"H": _ctl(got=1, exact=True, s=1.0), "W": w, "L": l_fail}, False)[0] is None

    # judge_step turns the classes into the row: platform = NOREF, never a pass and never a FAIL; app / no evidence = FAIL
    def judge(ctl):
        r = cases.Result("remote_heredoc_paste", "terminal@gitbash")
        CR.judge_step(None, r, line, ctl, None, [])
        return r
    r = judge({"H": h_fail, "L": l_fail})
    assert not r.fails and len(r.noref) == 1 and "PLATFORM" in r.noref[0] and "10104 client bytes received" in r.noref[0], (r.fails, r.noref)
    r = judge({"H": h_fail, "L": _ctl(got=10001, exact=True, s=3.2)})
    assert len(r.fails) == 1 and "APP" in r.fails[0] and not r.noref, (r.fails, r.noref)
    r = judge({"H": h_fail})
    assert len(r.fails) == 1 and not r.noref, (r.fails, r.noref)
    r = judge(None)
    assert len(r.fails) == 1 and not r.noref

    # heredoc_control: L is run on Windows only, only when asked for, once per run (the second mode reuses it), with the step's own command and budget
    calls = []
    real = (CR.conhost_ssh_send, CR.plain_ssh_send, dict(CR._CONTROL_CACHE))
    CR.conhost_ssh_send = lambda ctx, label, name, text, expected, timeout: calls.append((name, timeout, text[:9], len(expected))) or _ctl(why="no file within %d s" % timeout)
    CR.plain_ssh_send = lambda ctx, label, name, text, expected, timeout: calls.append(("posix:" + name, timeout)) or dict(_ctl(got=len(expected), exact=True, s=1.0))
    try:
        CR._CONTROL_CACHE.clear()
        win = types.SimpleNamespace(win=True, run="runA")
        r = cases.Result("remote_heredoc_paste", "terminal@gitbash")
        ctl = CR.heredoc_control(win, r, "EOF", ["a", "b"], line=None)
        assert set(ctl) == {"H"} and [c[0] for c in calls] == ["ctl_h.txt"], calls
        r2 = cases.Result("remote_heredoc_paste", "agent@gitbash")
        ctl = CR.heredoc_control(win, r2, "EOF", ["a", "b"], line="x" * 10000)
        assert set(ctl) == {"H", "L"} and calls[-1] == ("ctl_l.txt", 60, "printf '%", 10001), calls
        assert "control_L_bytes_got" in r2.metrics and "control_H_bytes_got" not in r2.metrics and any("reused" in n for n in r2.notes), (r2.metrics, r2.notes)
        n = len(calls)
        CR.heredoc_control(win, cases.Result("x", "agent@gitbash"), "EOF", ["a", "b"], line="x" * 10000)
        assert len(calls) == n, "the control does not involve the app: one L per run"
        posix = types.SimpleNamespace(win=False, run="runB")
        ctl = CR.heredoc_control(posix, cases.Result("x", "terminal"), "EOF", ["a", "b"], line="x" * 10000)
        assert set(ctl) == {"H", "W"}, "no L off Windows: plain ssh there has the wrapper-line control"
    finally:
        CR.conhost_ssh_send, CR.plain_ssh_send = real[0], real[1]
        CR._CONTROL_CACHE.clear()
        CR._CONTROL_CACHE.update(real[2])
    src = inspect.getsource(CR.c_remote_heredoc)
    assert 'line_stuck = any(x["name"] == LINE_STEP and x["got"] is None for x in recs)' in src and "line=long_line if line_stuck else None" in src
    assert CR.LINE_STEP == "hd_line.txt" and '"hd_line.txt"' in src


# ------------------------------------------------------------------ (5) latency_enter on Windows Git Bash
def _log(enters, t0=1000.0):
    """app-log lines of `enters` Enters: [(execute, bookkeeping, flush, to_pty)] ms per hop, one Enter every 100 ms"""
    out = []
    for i, (a, b, c, d) in enumerate(enters):
        t = t0 + i * 100.0
        out += ["x input_order hop=enter_begin deferred=false t=%.3f" % t,
                "x input_order hop=try_execute_done t=%.3f" % (t + a),
                "x input_order hop=enter_submit_done t=%.3f" % (t + a + b),
                "x input_order hop=write_command t=%.3f" % (t + a + b + c),
                "x input_order pty_write bytes=24 t=%.3f" % (t + a + b + c + d),
                "x input_order pty_read bytes=30 since_write_ms=30.0 t=%.3f" % (t + a + b + c + d + 30)]
    return out


@check
def r19_enter_segments_have_the_four_consecutive_hops():
    """(5) the app's own Enter segment, enter_begin -> pty_write, is split into its four consecutive hops from lines the app already logs; a hop whose line
    is missing has no figure (never 0) and the older figures are unchanged"""
    segs = cases.enter_segments(_log([(0.25, 0.03, 0.17, 0.45)] * 5), 0)
    assert segs["enter_to_execute_ms"] == 0.25 and segs["submit_bookkeeping_ms"] == 0.03 and segs["effect_flush_ms"] == 0.17 and segs["command_to_pty_ms"] == 0.45, segs
    assert segs["enter_to_write_ms"] == 0.9 and segs["enter_to_write_n"] == 5 and segs["command_to_pty_n"] == 5 and segs["write_to_first_read_ms"] == 30.0, segs
    assert [k for k, _ in cases.ENTER_HOPS] == ["enter_to_execute_ms", "submit_bookkeeping_ms", "effect_flush_ms", "command_to_pty_ms"]
    assert set(k for k, _ in cases.ENTER_HOPS) <= set(cases.ENTER_SEGMENT_KEYS)
    # a build with no try_execute_done / write_command lines: the whole is still measured, the hops have no figure
    old = [l for l in _log([(0.25, 0.03, 0.17, 0.45)] * 5) if "try_execute_done" not in l and "write_command" not in l and "enter_submit_done" not in l]
    s2 = cases.enter_segments(old, 0)
    assert s2["enter_to_write_ms"] == 0.9 and not any(k in s2 for k, _ in cases.ENTER_HOPS), s2
    # a write_command of something else (no Enter pending) starts no hop, and a stale one does not leak into the next Enter
    stray = ["x input_order hop=write_command t=900.0"] + _log([(0.2, 0.1, 0.1, 0.4)])
    got = cases.enter_segment_samples(stray, 0)["command_to_pty_ms"]
    assert len(got) == 1 and abs(got[0] - 0.4) < 1e-6, got
    # the names the reader looks for are the hops the app logs
    inp = open(os.path.join(REPO, "app", "src", "terminal", "input.rs"), encoding="utf-8").read()
    pc = open(os.path.join(REPO, "app", "src", "terminal", "writeable_pty", "pty_controller.rs"), encoding="utf-8").read()
    for hop in ("enter_begin", "try_execute_done", "enter_submit_done"):
        assert 'log_hop("%s"' % hop in inp, hop
    assert 'log_hop("write_command"' in pc


@check
def r19_enter_hop_breakdown_names_the_hop_that_grew():
    """(5) the attribution control: per hop, this arm against the terminal-mode arm of the same run and against the previous run; the hop with the largest
    positive difference is named, a hop the other side has no figure for is listed as missing (never counted as 0), and nothing is named when nothing grew"""
    term = cases.enter_segments(_log([(0.25, 0.03, 0.17, 0.45)] * 40), 0)
    agent = cases.enter_segments(_log([(1.30, 0.04, 0.18, 0.48)] * 40), 0)
    bd = cases.enter_hop_breakdown(agent, [("the terminal-mode arm of this run", term)])
    v = bd["vs"][0]
    assert bd["whole_ms"] == 2.0 and bd["sum_ms"] == 2.0 and v["whole_delta_ms"] == 1.1 and v["grew"] == "enter_to_execute_ms" and v["delta"]["enter_to_execute_ms"] == 1.05, bd
    assert v["missing"] == [] and v["delta"]["command_to_pty_ms"] == 0.03
    note = cases.enter_hop_note(bd)
    assert "the hop that grew most: enter_to_execute (+1.05 ms" in note and "vs the terminal-mode arm of this run: whole +1.10" in note and "routing" in note, note
    # the growth in another hop is named as that hop
    bd = cases.enter_hop_breakdown(cases.enter_segments(_log([(0.25, 0.03, 0.17, 1.55)] * 40), 0), [("t", term)])
    assert bd["vs"][0]["grew"] == "command_to_pty_ms", bd
    # nothing slower: no hop is named
    bd = cases.enter_hop_breakdown(term, [("t", agent)])
    assert bd["vs"][0]["grew"] is None and "no hop is slower here" in cases.enter_hop_note(bd), bd
    # the previous run recorded only two of the hops (a run before this change): compared on those, the other two listed as missing
    prev = dict(enter_to_write_ms=0.86, enter_to_write_n=63, submit_bookkeeping_ms=0.03, effect_flush_ms=0.18)
    bd = cases.enter_hop_breakdown(agent, [("the previous run, same arm", prev)])
    v = bd["vs"][0]
    assert v["whole_delta_ms"] == 1.14 and set(v["delta"]) == {"submit_bookkeeping_ms", "effect_flush_ms"} and v["missing"] == ["enter_to_execute_ms", "command_to_pty_ms"], v
    assert v["grew"] == "submit_bookkeeping_ms" and "[no figure there for: enter_to_execute, command_to_pty]" in cases.enter_hop_note(bd)
    assert cases.enter_hop_breakdown({"enter_to_write_ms": 0.9}, [("t", term)]) is None and cases.enter_hop_breakdown(None) is None, "no hop logged: no breakdown"
    assert cases.enter_hop_breakdown(agent, [("empty", {})])["vs"] == []
    # record_enter_hops: the terminal arm of a run is kept for the agent arm of the SAME run and pane, and only for that
    saved = dict(cases._ENTER_HOPS_OF_RUN)
    real_prev = lat.prev_case_metric
    lat.prev_case_metric = lambda rid, name: None
    try:
        cases._ENTER_HOPS_OF_RUN.clear()
        ctx = lambda mode, run="r1", pane="gitbash": types.SimpleNamespace(run=run, pane=pane, mode=mode)      # noqa: E731
        r = cases.Result("latency_enter", "terminal@gitbash")
        assert cases.record_enter_hops(ctx("terminal"), r, term, "terminal@gitbash/latency_enter/enter_to_start")["vs"] == []
        r = cases.Result("latency_enter", "agent@gitbash")
        bd = cases.record_enter_hops(ctx("agent"), r, agent, "agent@gitbash/latency_enter/enter_to_start")
        assert bd["vs"][0]["name"] == "the terminal-mode arm of this run" and r.metrics["enter_hops_ms"] is bd and any("grew most: enter_to_execute" in n for n in r.notes)
        for other in (ctx("agent", run="r2"), ctx("agent", pane="powershell")):
            assert cases.record_enter_hops(other, cases.Result("latency_enter", "agent"), agent, "x/latency_enter/enter_to_start")["vs"] == []
        lat.prev_case_metric = lambda rid, name: prev if name == "app_segments_ms" else None
        bd = cases.record_enter_hops(ctx("agent"), cases.Result("latency_enter", "agent@gitbash"), agent, "agent@gitbash/latency_enter/enter_to_start")
        assert [v["name"] for v in bd["vs"]] == ["the terminal-mode arm of this run", "the previous run, same arm"]
    finally:
        lat.prev_case_metric = real_prev
        cases._ENTER_HOPS_OF_RUN.clear()
        cases._ENTER_HOPS_OF_RUN.update(saved)


def _evaluate(share, median=115.0, base=None, prev_doc=None, expected=60, n=60):
    """lat.evaluate for agent@gitbash/latency_enter with a baseline entry and a previous run's document, both swapped in"""
    import json, tempfile
    rid = "agent@gitbash/latency_enter/enter_to_start"
    real = (lat.load_baseline, os.environ.get("APPCI_PREV_RESULTS"))
    lat.load_baseline = lambda platform: ("ok", {rid: base} if base else {})
    tmp = None
    try:
        if prev_doc is not None:
            tmp = tempfile.NamedTemporaryFile("w", suffix=".json", delete=False)
            json.dump(prev_doc, tmp)
            tmp.close()
            os.environ["APPCI_PREV_RESULTS"] = tmp.name
        else:
            os.environ.pop("APPCI_PREV_RESULTS", None)
        phos = [median + (i % 7) * 0.5 - 1.5 for i in range(n)]
        ref = [67.0 + (i % 5) * 0.4 for i in range(n)]
        return lat.evaluate(rid, phos, ref, 2.0, "overhead", "windows", remeasure=lambda: [median] * n, ref_name="conhost (bash)", pane_shell="bash",
                            ref_shell="bash", expected=expected, app_share=share)
    finally:
        lat.load_baseline = real[0]
        if real[1] is None:
            os.environ.pop("APPCI_PREV_RESULTS", None)
        else:
            os.environ["APPCI_PREV_RESULTS"] = real[1]
        if tmp is not None:
            os.unlink(tmp.name)


def _prev_doc(median, share_ms=None, share_n=63, with_record_share=None):
    rec = {"phosphor": {"p50": median, "p95": median + 40, "n": 60}}
    if with_record_share is not None:
        rec["app_share"] = {"what": "enter_to_write", "p50": with_record_share, "n": 63}
    metrics = {"lat": {"enter_to_start": rec}}
    if share_ms is not None:
        metrics["app_segments_ms"] = {"enter_to_write_ms": share_ms, "enter_to_write_n": share_n, "platform": "windows"}
    return {"cases": [{"name": "latency_enter", "mode": "agent@gitbash", "status": "noref", "metrics": metrics}]}


@check
def r19_latency_enter_gitbash_gate_judges_the_app_segment():
    """(5) latency_enter on the Windows Git Bash pane, both modes: the regression verdicts are on the app's own segment (enter_begin -> pty_write); the
    end-to-end median is recorded, not judged. The gate still fails: on a grown segment (vs the previous run's flat metric, vs a baseline that carries the
    figure), on a second measurement that is still grown, and on the 2 ms bound. Other panes and platforms keep the median gate"""
    segs = dict(enter_to_write_ms=0.91, enter_to_write_n=63)
    calls = []

    def ctx(win=True, pane="gitbash", mode="agent", again_segs=None):
        return types.SimpleNamespace(win=win, pane=pane, mode=mode, app_enter_segments=lambda t0: calls.append("segs") or dict(again_segs or segs))
    b = cases.enter_regress_basis(ctx(), segs, 60, 3, lambda: [100.0] * 60)
    assert b["what"] == cases.ENTER_APP_SHARE == "enter_to_write" and b["p50"] == 0.91 and b["n"] == 63 and b["metric"] == "app_segments_ms", b
    assert b["budget_ms"] == cases.ENTER_APP_SHARE_MAX_MS == 2.0 == cases.MACOS_ENTER_APP_SHARE_MAX_MS and callable(b["remeasure"])
    assert cases.enter_regress_basis(ctx(mode="terminal"), segs, 60, 3, lambda: [])["p50"] == 0.91, "both session modes"
    for other in (ctx(win=False), ctx(pane="powershell"), ctx(win=False, pane="gitbash")):
        assert cases.enter_regress_basis(other, segs, 60, 3, lambda: []) is None, "only the Windows Git Bash pane"
    assert cases.enter_regress_basis(ctx(), {}, 60, 3, lambda: []) is None and cases.enter_regress_basis(ctx(), None, 60, 3, lambda: []) is None, "no figure: the median gate"
    again = cases.enter_regress_basis(ctx(again_segs=dict(enter_to_write_ms=0.95, enter_to_write_n=63)), segs, 60, 3, lambda: [101.0] * 60)["remeasure"]()
    assert again == dict(what="enter_to_write", p50=0.95, n=63, samples=[101.0] * 60), again
    assert cases.enter_regress_basis(ctx(again_segs={"x": 1}), segs, 60, 3, lambda: [])["remeasure"]()["p50"] is None, "a second set with no figure judges nothing"

    base = {"p50": 92.77, "p95": 199.58, "n": 60}                       # the committed baseline entry: no app-share figure
    share = lambda p50, n=63, again=None: dict(what="enter_to_write", p50=p50, n=n, budget_ms=2.0, metric="app_segments_ms", **({"remeasure": again} if again else {}))  # noqa: E731
    # round 14's numbers with an unchanged app segment: median +24 % vs baseline, segment 0.91 vs 0.88 in the previous run -> not a regression
    rec = _evaluate(share(0.91), base=base, prev_doc=_prev_doc(93.0, share_ms=0.88))
    assert rec["regress_basis"] == "app_share" and rec["prev_verdict"] == "pass" and rec["baseline_verdict"] == "none", rec
    assert rec["baseline"]["median_verdict"] == "fail" and rec["baseline"]["judged_on"] == "app_share" and rec["baseline"]["delta_pct"] > 20, rec["baseline"]
    assert rec["phosphor"]["p50"] is not None and rec["app_share"]["prev"]["p50"] == 0.88, "the median is still in the record; the previous figure came from the flat metric"
    assert any("no app-share figure (enter_to_write) in that record" in n for n in rec["notes"]), "the baseline has no figure yet: said, not judged"
    # the same median with the app segment grown (0.86 -> 2.00, the round 14 agent arm): FAIL on the segment, re-measured, still FAIL
    grown = lambda: dict(what="enter_to_write", p50=1.95, n=63, samples=[110.0] * 60)      # noqa: E731
    rec = _evaluate(share(2.0, again=grown), base=base, prev_doc=_prev_doc(93.0, share_ms=0.86))
    assert rec["prev_verdict"] == "fail" and rec["verdict"] == "fail" and rec["remeasure"]["app_share"]["p50"] == 1.95 and rec["remeasure"]["prev"]["verdict"] == "fail", rec
    r = cases.Result("latency_enter", "agent@gitbash")
    cases.lat_checks(r, rec, "Enter to command start")
    assert any(f.startswith("REGRESSION Enter to command start: the app's own share (enter_to_write) 2.0 ms vs prev build +1.14 ms") and "re-measured once" in f for f in r.fails), r.fails
    assert any(n.startswith("NOT JUDGED: Enter to command start: median") for n in r.notes), r.notes
    # a clean second set is a WARN, never a pass; a second set with no figure does not clear the first
    clean = lambda: dict(what="enter_to_write", p50=0.87, n=63, samples=[93.0] * 60)       # noqa: E731
    assert _evaluate(share(2.0, again=clean), base=base, prev_doc=_prev_doc(93.0, share_ms=0.86))["prev_verdict"] == "warn"
    none = lambda: dict(what="enter_to_write", p50=None, n=0, samples=[])                  # noqa: E731
    assert _evaluate(share(2.0, again=none), base=base, prev_doc=_prev_doc(93.0, share_ms=0.86))["prev_verdict"] == "fail"
    # a baseline that carries the figure (after the next promotion) judges too; so does a previous record that carries it
    rec = _evaluate(share(2.0), base=dict(base, app_share={"what": "enter_to_write", "p50": 0.86, "n": 63}))
    assert rec["baseline_verdict"] == "fail" and rec["app_share"]["baseline"]["delta_ms"] == 1.14, rec
    assert _evaluate(share(2.0), prev_doc=_prev_doc(93.0, with_record_share=0.86))["prev_verdict"] == "fail"
    assert _evaluate(share(2.0), prev_doc=_prev_doc(93.0, share_ms=0.86, share_n=12))["prev_verdict"] == "none", "a previous figure from too few Enters is no reference"
    # the 2 ms bound fails whatever the comparison says, and however few Enters carry the figure
    for n in (63, 5):
        rec = _evaluate(share(2.01, n=n), base=base, prev_doc=_prev_doc(93.0, share_ms=2.0))
        assert rec["app_share"]["budget_verdict"] == "fail" and rec["verdict"] == "fail", rec
        r = cases.Result("latency_enter", "agent@gitbash")
        cases.lat_checks(r, rec, "Enter to command start")
        assert any("exceeds its 2.0 ms budget" in f for f in r.fails), r.fails
    assert _evaluate(share(2.0), base=base)["app_share"]["budget_verdict"] == "pass"
    # a figure from too few Enters is not the basis: the median gate applies exactly as before (and fails on round 14's numbers)
    rec = _evaluate(share(0.91, n=12), base=base)
    assert rec["regress_basis"] == "median" and rec["baseline_verdict"] == "fail", rec
    assert _evaluate(None, base=base)["baseline_verdict"] == "fail", "no app share handed in (PowerShell pane, Linux, macOS): the median gate"
    # lat.old_app_share reads both shapes and nothing else
    assert lat.old_app_share("enter_to_write", None, {"enter_to_write_ms": 0.88, "enter_to_write_n": 63}) == 0.88
    assert lat.old_app_share("enter_to_write", None, {"enter_to_write_ms": 0.88, "enter_to_write_n": 29}) is None
    assert lat.old_app_share("enter_to_write", None, {"enter_to_write_ms": None, "enter_to_write_n": 63}) is None
    assert lat.old_app_share("enter_to_write", None, {"unavailable": True}) is None
    assert lat.old_app_share("handler_to_write", None, {"handler_to_write": {"p50": 0.65, "n": 30}}) == 0.65, "the Ctrl-C shape, as before"
    # the case wires it: the basis goes to lat.evaluate, the median is reported in the row, the breakdown is recorded
    src = inspect.getsource(cases.c_lat_enter)
    assert "basis = enter_regress_basis(ctx, segs, n, warm, again)" in src and "app_share=basis" in src and "remeasure=again" in src
    assert "REPORTED, not gated on this pane" in src and "record_enter_hops(ctx, r, segs, rid)" in src


# ------------------------------------------------------------------ (9) dash / ash agent-mode lab arms
@check
def r19_lab_dash_ash_agent_arms_2120():
    """(9) owner 2026-11-11: an agent-mode lab arm on a dash / ash host is UNVERIFIABLE with the DECLINED reference when the app log of its connect has the
    plain-login event of the SSH hook; without that event it is judged in full, with no connect at all it FAILs; terminal-mode arms are judged normally"""
    import cases_lab as CL
    import cases_agent_remote as AR, cases_agent_file_edits as FE, cases_lab_remote as LR
    hop = "12:00:01.1 [DEBUG] input_order hop=ssh_wrapper remote_plain_login holding=true held_bytes=13 t=3\n"
    info = "12:00:01.1 [INFO] SSH hook for a login shell the remote command does not bootstrap (dash): a plain login, no remote session is pending\n"
    managed = "12:00:00.9 [INFO] Received ManagedSSHSessionStarting hook\n"
    ignored = "x hop=ssh_wrapper remote_plain_login_ignored state=Off remote_announced=false\n"
    fallback = "15:49:23.421 [INFO] Received PlainSSHFallback hook\n"
    lim = CL.plain_session_limit
    for shell in ("dash", "ash"):
        for log in (managed + hop, info, managed + info + hop):
            why = lim(shell, log)
            assert why and "KNOWN LIMITATION" in why and "#2120" in why and "DECLINED.md" in why and shell in why and "remote_plain_login" in why, (shell, why)
        for log in (managed, ignored, fallback, "", None):
            assert lim(shell, log) is None, (shell, log)          # no plain-login event: judged in full (kill switch off, a regression of #2120)
        # terminal-mode feature cases: dash / ash are judged normally, whatever the log says
        assert CL.plain_session_feature_limit(shell, managed + hop, "the code-review panel of a remote repository") is None
    for shell in ("bash", "zsh", "pwsh", "rbash", "ksh"):
        assert lim(shell, managed + hop) is None and lim(shell, info) is None, shell
    assert lim("nu", hop) is None and lim("fish", info) is None, "fish / nu keep their own premise (#2019)"
    assert lim("nu", fallback) and CL.plain_session_feature_limit("nu", fallback, "f"), "and their own verdict"
    assert CL.PLAIN_LOGIN_SH_SHELLS == ("dash", "ash") and CL.PLAIN_SESSION_LOGIN_SHELLS == ("fish", "nu")
    declined = open(os.path.join(REPO, "DECLINED.md"), encoding="utf-8").read()
    for ref in (CL.PLAIN_LOGIN_SH_DECLINED, CL.PLAIN_SESSION_DECLINED):
        assert ("## " + ref.split("'")[1]) in declined, ref                 # the entry each reason names exists, word for word
    notes = open(os.path.join(REPO, "docs", "release-notes-draft.md"), encoding="utf-8").read()
    assert "login shell is dash or ash" in notes and "#2120" in notes and "Known limits" in notes

    # agent_connect, driven with stubs: a dash arm with no remote prompt at all (#2126) still connects on the event
    def drive(shell, log, prompt=False, left=True):
        calls, state = [], {"prompts": 3}
        ctx = types.SimpleNamespace(log_size=lambda: 0, log_since=lambda off: log, reset=lambda: calls.append("reset"), ssh_diag=lambda: "diag", left_unproven=None,
                                    be=types.SimpleNamespace(type=lambda text, pace, enter=False: calls.append("type " + text) or (prompt and state.update(prompts=4))),
                                    until=lambda pred, timeout, step=0.1: pred())
        r = cases.Result("lab_x.%s.lan.nohelper" % shell, "agent")
        real = (CL.CR.connect, CL.CR.leave, CL.CR.rprompts, CL.time.sleep)
        CL.CR.connect = lambda c, host, timeout: calls.append("connect " + host) or (1.0 if prompt else None)
        CL.CR.leave = lambda c: calls.append("leave") or left
        CL.CR.rprompts = lambda c: state["prompts"]
        CL.time.sleep = lambda s: None
        try:
            ok = CL.agent_connect(ctx, r, dict(shell=shell, user="u_" + shell, lat_ms=0))
        finally:
            CL.CR.connect, CL.CR.leave, CL.CR.rprompts, CL.time.sleep = real
        return ok, r, calls
    for shell in ("dash", "ash"):
        for prompt in (False, True):                 # with and without #2126's login shell (its prompt hook)
            ok, r, calls = drive(shell, managed + hop, prompt=prompt)
            assert ok is False and not r.fails and len(r.noref) == 1 and r.noref[0].startswith("UNVERIFIABLE: KNOWN LIMITATION") and "#2120" in r.noref[0], (r.fails, r.noref)
            assert calls == ["type ssh lab-u_" + shell, "leave", "reset"] and r.metrics.get("plain_session_by_decision") == shell, calls
        ok, r, calls = drive(shell, managed + hop, left=False)       # the pane must still be left: that check is not waived
        assert ok is False and len(r.fails) == 1 and "exit" in r.fails[0] and len(r.noref) == 1
        ok, r, calls = drive(shell, managed, prompt=False)           # neither a prompt nor the event: no connect, a FAIL, never the limitation
        assert ok is False and len(r.fails) == 1 and "no remote prompt and no plain-login event" in r.fails[0] and not r.noref and calls[-1] == "reset", (r.fails, calls)
        ok, r, calls = drive(shell, managed, prompt=True)            # connected with no plain-login event: the arm is judged in full
        assert ok is True and not r.fails and not r.noref and calls == ["type ssh lab-u_" + shell], calls
    ok, r, calls = drive("bash", managed + hop, prompt=True)         # a bash arm never gets it, and connects the way it always did
    assert ok is True and not r.noref and calls == ["connect lab-u_bash"], calls
    ok, r, calls = drive("bash", managed + hop, prompt=False)
    assert ok is False and len(r.fails) == 1 and "no remote prompt" in r.fails[0] and not r.noref
    # every agent-mode lab arm on dash / ash goes through agent_connect, and the rows the decision names are registered
    for fn in (AR.c_agent_remote_flow, AR.c_agent_remote_errors, AR.c_agent_remote_drop, LR.c_read_files_remote, LR.c_diff_apply_remote):
        assert "CL.agent_connect(ctx, r, arm)" in inspect.getsource(fn), fn.__name__
    assert "CL.agent_connect(ctx, r, arm)" in inspect.getsource(FE.lab_ready) and "lab_ready(ctx, r, arm)" in inspect.getsource(FE.c_lab_agent_delete_conflict)
    reg = {c["name"]: c for c in cases.CASES}
    for n in ("lab_agent_remote_flow.dash.lan.nohelper", "lab_agent_remote_flow.ash.lan.nohelper", "lab_agent_remote_drop.dash.lan.nohelper",
              "lab_fam_agent_delete_conflict.dash.lan.nohelper"):
        assert n in reg and reg[n]["modes"] == ("agent",), n
    # the terminal-mode cases about a managed-session feature still ask the fish / nu rule only
    import cases_lab_buffers as LB, cases_lab_diff as LD
    for fn in (LB.c_remote_edit, LD.c_diff_panel_remote):
        assert "CL.plain_session_feature_limit(" in inspect.getsource(fn) and "agent_connect" not in inspect.getsource(fn), fn.__name__


@check
def r19_decide_tests_docs():
    doc = _doc()
    for needle in ("docs/design/r19-decide-tests.md", "selftest_r19_decide.py", "enter_regress_basis", "enter_hops_ms", "APPCI_ENTER_APP_SHARE_MAX_MS",
                   "r19_lab_dash_ash_agent_arms_2120", "control L", "big_paste_back_at_prompt"):
        assert needle in doc, needle
