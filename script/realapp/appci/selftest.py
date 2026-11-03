#!/usr/bin/env python3
# appci/selftest.py (#1522): offline harness self-test. Pure Python: no app, display, backend, network or ssh.
# usage: python3 script/realapp/appci/selftest.py      (also `run.py --self-test`, which runs this after its own checks)
# Covers the parsers and registries the verdicts hang on: Ctx.lines / stamps / parse_t / key_frame_pairs, the app-log readers (log_view_mode,
# mode_now's log-first rule, log_since), Profile.commands, routing classification, case-name validation, tier_plan, and the registry
# consistency checks (SMOKE, winmap, FIX_TIER name only registered cases: a renamed case used to drop out of smoke silently).
import os
import json, os, subprocess, sys, tempfile, shutil, types

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "..", "..", "ci"))
import cases, winmap, profile  # noqa: E402
import cases_caseagent as ca  # noqa: E402

CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


def _ctx(tmp, log_path=None):
    c = types.SimpleNamespace(W=tmp, prof=types.SimpleNamespace(rhome=tmp), log_path=log_path, ocr_calls=0)
    c.lines = lambda name, remote=False: cases.Ctx.lines(c, name, remote)
    c.log_view_mode = lambda: cases.Ctx.log_view_mode(c)
    c.log_size = lambda: cases.Ctx.log_size(c)
    c.log_since = lambda off: cases.Ctx.log_since(c, off)
    return c


@check
def registry_consistency():
    reg = {c["name"] for c in cases.CASES}
    assert len(reg) == len(cases.CASES), "duplicate case names: %s" % sorted(n for n in reg if [c["name"] for c in cases.CASES].count(n) > 1)
    assert not [n for n in cases.SMOKE if n not in reg], "SMOKE names no registered case: %s" % [n for n in cases.SMOKE if n not in reg]
    wm = set(winmap.BOTH) | set(winmap.GITBASH_ONLY) | set(winmap.PS_ONLY) | set(winmap.NOT_WINDOWS)
    assert not [n for n in wm if n not in reg], "winmap names no registered case: %s" % sorted(n for n in wm if n not in reg)
    assert not winmap.unclassified(sorted(reg)), "cases with no Windows classification: %s" % winmap.unclassified(sorted(reg))
    for plat, modes in winmap.FIX_TIER.items():
        for mode, o in modes.items():
            assert all(n in cases.SMOKE for n in (o["cases"] or ())), "FIX_TIER %s/%s names a case outside SMOKE" % (plat, mode)
    for n, spots in winmap.FIX_ONLY_IN.items():
        assert n in reg


@check
def destination_mode_metric_and_kill_switch_smoke():
    """#1532 / #1528: the destination mode is recorded in the results metrics, the legacy smoke pass is a subset of SMOKE, and the shadow
    disagreement counter reads the app log."""
    import app as appmod
    assert cases.dest_mode({}) == "table" and cases.dest_mode({"PHOSPHOR_QUEUE_DESTINATION": "  "}) == "table"
    assert cases.dest_mode({"PHOSPHOR_QUEUE_DESTINATION": "shadow"}) == "shadow" and cases.dest_mode({"PHOSPHOR_QUEUE_DESTINATION": "table"}) == "table"
    for v in ("legacy", "0", "off", "false", "bogus"):
        assert cases.dest_mode({"PHOSPHOR_QUEUE_DESTINATION": v}) == "legacy", v
    assert set(winmap.KILL_SWITCH_SMOKE) <= set(cases.SMOKE), "KILL_SWITCH_SMOKE names a case outside SMOKE"
    assert winmap.smoke_for("table", cases.SMOKE) == tuple(cases.SMOKE) and winmap.smoke_for("shadow", cases.SMOKE) == tuple(cases.SMOKE)
    legacy = winmap.smoke_for("legacy", cases.SMOKE)
    assert legacy and set(legacy) == set(winmap.KILL_SWITCH_SMOKE) and len(legacy) < len(cases.SMOKE) and "mode_proof" in legacy
    # the metric lands in the mode_proof row (fake ctx: no OCR, the log says the view mode)
    saved = os.environ.get("PHOSPHOR_QUEUE_DESTINATION")
    try:
        for env, want in ((None, "table"), ("legacy", "legacy"), ("shadow", "shadow")):
            if env is None:
                os.environ.pop("PHOSPHOR_QUEUE_DESTINATION", None)
            else:
                os.environ["PHOSPHOR_QUEUE_DESTINATION"] = env
            be = types.SimpleNamespace(name="linux", key=lambda *a, **k: None)
            ctx = types.SimpleNamespace(be=be, mode="agent", ollama=False, label="agent", ocr=lambda *a, **k: None, log_view_mode=lambda: "agent",
                                        shot=lambda *a, **k: None)
            r = cases.Result("mode_proof", "agent")
            cases.c_mode_proof(ctx, r)
            assert r.metrics.get("queue_destination") == want, (env, r.metrics)
            assert not r.fails, r.fails
    finally:
        if saved is None:
            os.environ.pop("PHOSPHOR_QUEUE_DESTINATION", None)
        else:
            os.environ["PHOSPHOR_QUEUE_DESTINATION"] = saved
    # macOS passes the variable through `open --env`
    assert appmod.passthrough_env({"PHOSPHOR_QUEUE_DESTINATION": "legacy", "HOME": "/x"}) == {"PHOSPHOR_QUEUE_DESTINATION": "legacy"}
    assert appmod.passthrough_env({"APPCI_APP_ENV": "FOO=1,BAR=2", "FOO": "1"}) == {"FOO": "1"}
    assert appmod.passthrough_env({}) == {}
    # the shadow counter
    log = ("2026 DEBUG input_order kh_dest_disagree row=D7 legacy=pty table=editor flags=x\n"
           "2026 DEBUG input_order kh_dest_disagree row=P1a legacy=dropped table=held flags=x expected=OD5\n"
           "2026 DEBUG input_order keydown key=a\n")
    assert cases.dest_disagreements(log) == (2, 1) and cases.dest_disagreements("") == (0, 0)


@check
def case_name_validation():
    import run
    reg = {"a", "b"}
    assert run.unknown_case_names("a,b", reg) == [] and run.unknown_case_names("", reg) == [] and run.unknown_case_names("a,c,,d", reg) == ["c", "d"]


@check
def file_readers():
    d = tempfile.mkdtemp()
    try:
        c = _ctx(d)
        assert c.lines("nope.txt") == []
        open(os.path.join(d, "f.txt"), "w").write("one\ntwo\n\nthree")
        assert c.lines("f.txt") == ["one", "two", "", "three"]
        open(os.path.join(d, "s.txt"), "w").write("1699999999.123456789\n 1699999999123 \n12\nabc\n1700000000123\x00\n")
        st = cases.stamps(c, "s.txt")
        assert st == [1699999999.123456789, 1699999999.123, 1700000000.123], st
        open(os.path.join(d, "p.txt"), "w").write("0:1699999999123\n1:1699999999456\nx\n1:1700000000000\n")
        assert cases.ps_stamps(c, "p.txt") == {0: 1699999999.123, 1: 1699999999.456}
    finally:
        shutil.rmtree(d)


@check
def stamp_and_log_line_parsers():
    assert cases.parse_t("input_order pty_write n=3 t=1700000000123.5") == 1700000000123.5 and cases.parse_t("no stamp") is None
    txt = "\n".join([
        "input_order keydown key=a ctrl=false alt=false shift=false cmd=false t=100.0 seq=1",
        "input_order keydown key=c ctrl=true alt=false shift=false cmd=false t=101.0 seq=2",
        "input_order keydown key=b ctrl=false alt=false shift=false cmd=false t=102.0 seq=3",
        "input_order pty_write n=2",
        "input_order frame layout=true t=110.0",
        "input_order frame layout=false t=120.0",
        "input_order keydown key=z ctrl=false alt=false shift=false cmd=false t=130.0 seq=4"])
    ds, cnt = cases.key_frame_pairs(txt)
    assert ds == [10.0, 8.0], ds
    assert cnt["keydown"] == 4 and cnt["keydown_ctrl"] == 1 and cnt["frame"] == 2 and cnt["pty_write"] == 1, cnt


@check
def app_log_readers():
    d = tempfile.mkdtemp()
    try:
        log = os.path.join(d, "phosphor.log")
        c = _ctx(d, log)
        assert c.log_view_mode() is None and c.log_size() == 0 and c.log_since(0) == ""
        open(log + ".in_session.1", "w").write("x input_order hop=view_mode mode=terminal pane=1\n")
        open(log + ".in_session.0", "w").write("x input_order hop=view_mode mode=agent pane=1\n")      # N=0 is the newest rotated chunk
        open(log, "w").write("plain line\n")
        assert c.log_view_mode() == "agent", "rotated chunks are read oldest first, N=0 newest"
        open(log, "a").write("y input_order hop=view_mode mode=terminal pane=1\n")
        assert c.log_view_mode() == "terminal", "the active file is the newest"
        off = c.log_size()
        open(log, "a").write("tail\n")
        assert c.log_since(off) == "tail\n"
        # mode_now prefers the log; it must not need OCR when the log answers
        c.ocr = lambda tag: (_ for _ in ()).throw(AssertionError("OCR used although the log has a view_mode line"))
        assert cases.Ctx.mode_now(c, "t") == "terminal"
        c.log_path = None
        c.ocr = lambda tag: "New Phosphor Agent conversation"
        assert cases.Ctx.mode_now(c, "t") == "agent"
        c.ocr = lambda tag: "run commands, or ask the agent"
        assert cases.Ctx.mode_now(c, "t") == "terminal"
        c.ocr = lambda tag: None
        assert cases.Ctx.mode_now(c, "t") is None
    finally:
        shutil.rmtree(d)


@check
def profile_rec_log():
    d = tempfile.mkdtemp()
    try:
        pr = profile.Profile(d)
        for dd in (pr.ctl, pr.refctl):
            os.makedirs(dd)
        open(os.path.join(pr.ctl, "rec.log"), "w").write("C\t1.5\tls -l\nP\t2.0\nC\t3.5\techo a\tb\nbad line\nC\tx\tno ts\n")
        got = pr.commands()
        assert [c for _, c in got][:2] == ["ls -l", "echo a\tb"], got
        open(os.path.join(pr.refctl, "rec.log"), "w").write("C\t9.0\tref\n")
        pr.ref_active = True
        assert [c for _, c in pr.commands()] == ["ref"]
    finally:
        shutil.rmtree(d)


@check
def routing_classification():
    ag = [dict(route="Agent")]
    sh = [dict(route="Shell")]
    assert ca.enter_begin_types("input_order hop=enter_begin deferred=false buffer=<9 chars> input_type=AI t=1\ninput_order hop=enter_begin deferred=false buffer=\"a b\" input_type=Shell t=2") == ["AI", "Shell"]
    assert ca.classify_dest(True, sh, ["Shell"]) == "shell" and ca.classify_dest(True, ag, ["AI"]) == "BOTH"
    assert ca.classify_dest(False, ag, []) == "agent" and ca.classify_dest(False, [], ["AI"]) == "agent"
    assert ca.classify_dest(False, [], ["Shell"]) == "lost" and ca.classify_dest(False, sh, ["Shell"]) == "lost", "a swallowed line is not 'went to agent' (#1511)"
    assert ca.classify_dest(False, [], []) == "unproven"
    m = ca.ROUTE_RE.search('enter: Agent by prose rule (AgentConversation, head "please", 12 bytes)')
    assert m and m.group(1) == "Agent" and m.group(3) == "AgentConversation" and m.group(5) == "12"


@check
def reference_forgiveness_is_capped():
    f = cases.forgiven_by_reference
    assert not f(30, 30, None) and not f(30, 30, dict(bad=0, exact_bad=0)), "a clean reference forgives nothing"
    assert f(30, 29, dict(bad=2)) and f(30, 30, dict(bad=1))
    assert not f(30, 27, dict(bad=5)), "Phosphor losing 3 lines is never forgiven (cap 1, #1514)"
    assert not f(30, 28, dict(bad=1)), "Phosphor losing more than the reference is not forgiven"


@check
def tier_plan_shape():
    assert winmap.tier_plan("windows", None, "terminal", "gitbash", cases.SMOKE)[0] is None
    names, o = winmap.tier_plan("windows", "smoke", "agent", "powershell", cases.SMOKE)
    assert names and set(names) <= set(cases.SMOKE) and "key_loss_control" not in names and not o["ref"]
    names, o = winmap.tier_plan("windows", "smoke", "terminal", "gitbash", cases.SMOKE)
    assert "key_loss_control" in names and o["ref"]


@check
def report_exit_codes():
    rep = os.path.join(HERE, "report.py")

    def run_report(rows):
        d = tempfile.mkdtemp()
        try:
            with open(os.path.join(d, "results.jsonl"), "w") as f:
                for r in rows:
                    f.write(json.dumps(dict(r, mode="terminal", detail=[], seconds=0)) + "\n")
            rc = subprocess.run([sys.executable, rep, d, "platform=t"], capture_output=True).returncode
            assert os.path.exists(os.path.join(d, "results.json")) and os.path.exists(os.path.join(d, "junit.xml"))
            return rc
        finally:
            shutil.rmtree(d)
    assert run_report([dict(name="a", status="pass")]) == 0
    assert run_report([dict(name="a", status="pass"), dict(name="b", status="skip")]) == 0
    assert run_report([dict(name="a", status="fail")]) == 1
    assert run_report([]) == 1, "an empty run must not be green (#1508)"
    assert run_report([dict(name="a", status="skip")]) == 1, "an all-skip run must not be green (#1508)"
    assert run_report([dict(name="a", status="noref")]) == 0


@check
def ci_seq_verdicts():
    import ci_seq
    assert ci_seq.judged_nothing({}) and ci_seq.judged_nothing({"summary": {"skip": 4}}) and not ci_seq.judged_nothing({"summary": {"pass": 1}})
    assert ci_seq.verdict("app-test-macos", "success") == "PASS" and ci_seq.verdict("app-test-macos", "failed") == "FAIL"
    assert ci_seq.verdict("app-test-macos", "success but its run judged NOTHING") == "FAIL"


@check
def overhead_se_and_no_compare():
    import lat, random
    rng = random.Random(7)
    a = [10 + rng.gauss(0, 4) for _ in range(30)]
    b = [8 + rng.gauss(0, 4) for _ in range(30)]
    se = lat.median_diff_se(a, b)
    assert 0.8 < se < 3.5, se                       # ~1.25*sd*sqrt(2/n) = 1.3 for sd 4
    assert lat.median_diff_se(a, b) == se           # deterministic
    assert lat.median_diff_se([1.0], b) is None
    # fail only when overhead - 2*SE > budget; pass only when overhead + 2*SE <= budget; otherwise inconclusive (never pass)
    assert lat.overhead_verdict(10.0, 2.0, 2.0) == "fail"
    assert lat.overhead_verdict(6.0, 2.0, 2.0) == "inconclusive"      # 6 - 4 = 2, not > 2
    assert lat.overhead_verdict(6.1, 2.0, 2.0) == "fail"
    assert lat.overhead_verdict(5.0, 2.0, 2.0) == "inconclusive"
    assert lat.overhead_verdict(-3.0, 0.5, 2.0) == "pass"
    assert lat.overhead_verdict(3.0, None, 2.0) == "fail" and lat.overhead_verdict(1.0, None, 2.0) == "pass"
    # shells that differ: no verdict at all, whatever the numbers
    assert lat.overhead_verdict(50.0, 0.1, 2.0, "/bin/zsh", "bash") == "no-compare"
    assert lat.overhead_verdict(0.0, 0.1, 2.0, "/bin/zsh", "/usr/bin/zsh") == "pass"
    assert lat.overhead_verdict(50.0, 0.1, 2.0, "powershell", "powershell.exe") == "fail"
    assert lat.overhead_verdict(50.0, 0.1, 2.0, None, "bash") == "fail"          # unknown pane shell: not claimed to differ
    rec = lat.evaluate("x", [20.0] * 40, [10.0] * 40, 2.0, "overhead", "linux", pane_shell="zsh", ref_shell="bash")
    assert rec["budget_verdict"] == "no-compare" and rec["verdict"] == "no-compare" and lat.RANK["no-compare"] == 1
    rec = lat.evaluate("x", [20.0] * 40, [10.0] * 40, 2.0, "overhead", "linux", pane_shell="bash", ref_shell="bash")
    assert rec["budget_verdict"] == "fail" and rec["overhead_se_ms"] == 0.0
    # the record keeps both shells (metrics), and a reference-less record has no shell verdict
    rec = lat.evaluate("x", [20.0] * 40, [], 2.0, "overhead", "linux", pane_shell="zsh", ref_shell=None)
    assert rec["verdict"] == "no-reference"
    noisy_a = [10 + (i % 7) * 4 for i in range(30)]
    noisy_b = [8 + (i % 5) * 5 for i in range(30)]
    rec = lat.evaluate("x", noisy_a, noisy_b, 2.0, "overhead", "linux", pane_shell="bash", ref_shell="bash")
    assert rec["budget_verdict"] in ("inconclusive", "pass", "fail") and rec["overhead_se_ms"] > 0


@check
def enter_segment_split():
    L = ["input_order hop=enter_begin t=1000.0", "input_order pty_write bytes=73 t=1000.5", "input_order pty_read bytes=434 since_write_ms=1.3 t=1001.8",
         "input_order pty_read bytes=5 since_write_ms=9.0 t=1009.5", "input_order hop=enter_begin t=2000.0", "input_order pty_write bytes=3 t=2000.2",
         "input_order pty_write bytes=73 t=2001.5", "input_order pty_read bytes=10 since_write_ms=2.1 t=2003.6"]
    s = cases.enter_segments(L, 0)
    assert s["enter_to_write_ms"] == 1.0 and s["enter_to_write_n"] == 2, s          # (0.5, 1.5) -> median 1.0
    assert s["write_to_first_read_ms"] == 1.7 and s["write_to_first_read_n"] == 2, s   # only the FIRST read per write: 1.3, 2.1
    assert "write_to_preexec_ms" not in s                                              # no hop in the log: absent, not zero
    s = cases.enter_segments(L[:2] + ["input_order hop=preexec_received t=1004.5"], 0)
    assert s["write_to_preexec_ms"] == 4.0, s
    assert cases.enter_segments(L, 5000.0) == {}                                       # t0 is wall seconds: older lines ignored


@check
def injector_failures_are_errors():
    import backend
    assert backend.is_injector(["wtype", "-k", "a"]) and backend.is_injector(["xdotool", "type", "x"]) and not backend.is_injector(["xdotool", "windowfocus", "1"])
    try:
        backend._run([sys.executable, "-c", "pass"])
    except Exception:
        raise AssertionError("a non-injector command must not be treated as one")
    try:
        old = backend.is_injector
        backend.is_injector = lambda argv: True
        backend._run([sys.executable, "-c", "import sys; sys.exit(3)"])
        raise AssertionError("a failing injector returned normally")
    except RuntimeError as e:
        assert "exit 3" in str(e)
    finally:
        backend.is_injector = old


@check
def thin_evidence_is_never_a_pass():
    import lat
    good = [10.0 + (i % 3) * 0.1 for i in range(60)]
    ref = [9.5 + (i % 3) * 0.1 for i in range(60)]
    ev = lambda p, r, **k: lat.evaluate("x/y/z", p, r, 2.0, "overhead", "linux-test", **k)
    assert ev(good, ref, expected=60)["budget_verdict"] == "pass"
    assert ev(good[:20], ref[:20], expected=20)["budget_verdict"] == "inconclusive", "n < min_n must not pass"
    assert ev(good, ref, expected=70)["budget_verdict"] == "inconclusive", "< 90 % of the requested samples must not pass"
    assert ev(good, ref, expected=60, resolution_ms=4.17)["budget_verdict"] == "inconclusive", "resolution-limited must not pass"
    slow = [20.0 + (i % 3) * 0.1 for i in range(60)]
    assert ev(slow[:20], ref[:20], expected=20)["budget_verdict"] == "fail", "thin evidence never turns a FAIL into anything else"
    assert ev(slow, ref, expected=60, resolution_ms=4.17)["budget_verdict"] == "fail"
    drift = [10.0] * 30 + [14.0] * 30
    d = ev(drift, ref, expected=60)
    assert d["budget_verdict"] == "inconclusive" and d["drift_ms"]["phosphor"] == 4.0, d
    assert lat.block_drift([1.0] * 5) is None


@check
def previous_run_with_few_samples_is_ignored():
    import lat
    tmp = tempfile.mkdtemp()
    try:
        p = os.path.join(tmp, "prev.json")
        json.dump({"cases": [{"name": "c", "mode": "terminal", "metrics": {"lat": {"k": {"phosphor": {"n": 5, "p50": 1.0, "p95": 1.0}}}}}]}, open(p, "w"))
        os.environ["APPCI_PREV_RESULTS"] = p
        rec = lat.evaluate("terminal/c/k", [9.0] * 40, None, 2.0, "overhead", "linux-test")
        assert rec["prev"] == {"ignored": "n=5 < 30"} and rec["prev_verdict"] == "none", rec["prev"]
        json.dump({"cases": [{"name": "c", "mode": "terminal", "metrics": {"lat": {"k": {"phosphor": {"n": 40, "p50": 1.0, "p95": 1.0}}}}}]}, open(p, "w"))
        assert lat.evaluate("terminal/c/k", [9.0] * 40, None, 2.0, "overhead", "linux-test")["prev_verdict"] == "fail"
    finally:
        os.environ.pop("APPCI_PREV_RESULTS", None)
        shutil.rmtree(tmp)


@check
def case_status_changes_against_previous_build():
    import lat
    prev = {"cases": [{"mode": "t", "name": "a", "status": "pass"}, {"mode": "t", "name": "b", "status": "fail"}, {"mode": "t", "name": "c", "status": "pass"},
                      {"mode": "t", "name": "d", "status": "pass"}]}
    now = [{"mode": "t", "name": "a", "status": "fail"}, {"mode": "t", "name": "b", "status": "pass"}, {"mode": "t", "name": "d", "status": "noref"},
           {"mode": "t", "name": "e", "status": "pass"}]
    got = {(k, c) for k, c, _, _ in lat.changed_since(prev, now)}
    assert got == {("NEW-FAILURE", "t/a"), ("FIXED", "t/b"), ("MISSING", "t/c"), ("CHANGED", "t/d"), ("NEW-CASE", "t/e")}, got
    assert lat.changed_since(None, now) == []


@check
def baseline_promotion_is_gated():
    import baseline
    rec = lambda n, v="pass": {"phosphor": {"n": n, "p50": 5.0, "p95": 6.0}, "verdict": v, "budget_verdict": v}
    doc = lambda **kw: dict({"meta": {"tier": "full", "scale": "1"}, "summary": {"fail": 0}, "cases": [{"mode": "t", "name": "c", "metrics": {"lat": {"k": rec(60)}}}]}, **kw)
    assert baseline.eligibility(doc()) == []
    assert baseline.eligibility(doc(meta={"tier": "smoke"}))
    assert baseline.eligibility(doc(meta={}))                                       # tier unknown: not promotable
    assert baseline.eligibility(doc(meta={"tier": "full", "scale": "0.3"}))
    assert baseline.eligibility(doc(summary={"fail": 1}))
    bad = doc(); bad["cases"][0]["metrics"]["lat"]["k"] = rec(10)
    assert any("n=10" in w for w in baseline.eligibility(bad))
    for v in ("fail", "inconclusive", "no-reference", "no-compare"):
        bad = doc(); bad["cases"][0]["metrics"]["lat"]["k"] = rec(60, v)
        assert baseline.eligibility(bad), v
    assert baseline.eligibility({"meta": {"tier": "full"}, "cases": []})


@check
def latency_cases_have_a_sample_floor_and_flood_line():
    c = types.SimpleNamespace(scale=0.1)
    assert cases.Ctx.count_lat(c, 60) >= 30 and cases.Ctx.count(c, 60) == 6
    fl = types.SimpleNamespace(pane="bash", be=types.SimpleNamespace(name="x11"))
    assert "yes" in cases.ctrlc_line(fl, "f.txt", 0, flood=True) and "sleep 30" in cases.ctrlc_line(fl, "f.txt", 0)
    assert cases.KL3_QUANTUM_MS > 2.0 / 2


@check
def r18d2_routing_expectation_ocr_fuzz_and_ctrlc_window():
    # the r18d2 failures: find the bug in main.rs ran in the shell inside a conversation by the #1183 evidence rule (main.rs is data-shaped)
    w = ca.ambiguous_want
    assert w("find the bug in main.rs", "AgentConversation", "evidence-in-conversation (#1183)") == "shell"
    assert w("find the bug in main.rs", "PlainTerminal", "installed-head (#906/#964)") == "shell"
    assert w("make a PR", "AgentConversation", "no-evidence-in-conversation (#1183)") == "agent"
    assert w("cat the error log and tell me what is wrong", "AgentConversation", "request-shape (#834/#965)") == "agent"
    assert w("some other line", "AgentConversation", "evidence-in-conversation (#1183)") == "shell"
    assert w("some other line", None, None) is None
    # OCR: tesseract read 'keepmet115_qmoods' for 'keepmet115_qm09d8'
    assert ca.fuzzy_has("x keepmet115_qmoods y", "keepmet115_qm09d8")
    assert ca.fuzzy_has("KEEPMET115_QM09D8", "keepmet115_qm09d8")
    assert not ca.fuzzy_has("the quick brown fox jumps over the lazy dog", "keepmet115_qm09d8")
    assert not ca.fuzzy_has("abc", "abcd")      # short needles are exact
    # Ctrl-C window: two presses within 1.3 s exit an empty agent conversation; reset() skips the second one in agent mode
    c = types.SimpleNamespace(ctrlc_ts=[10.0, 10.6, 20.0])
    c.EXIT_WINDOW_S = cases.Ctx.EXIT_WINDOW_S
    assert cases.Ctx.double_ctrlc_since(c, 9.0) and not cases.Ctx.double_ctrlc_since(c, 11.0)
    sent = []
    be = types.SimpleNamespace(key=lambda spec, *a, **k: sent.append(spec), keys=lambda specs, *a, **k: sent.extend(specs))
    ctx = types.SimpleNamespace(ctrlc_ts=[], mode="agent", be=be, EXIT_WINDOW_S=cases.Ctx.EXIT_WINDOW_S)
    cases.Ctx._wrap_ctrlc(ctx, be)
    cases.Ctx.reset(ctx); cases.Ctx.reset(ctx)
    assert sent.count("ctrl+c") == 1 and sent.count("ctrl+u") == 2, sent


@check
def interleaved_block_schedule_and_runner():
    import lat
    sch = lat.block_schedule(60, block=5, warm=3, lead=1)
    assert [x[0] for x in sch] == ["phosphor", "reference"] * 12, [x[0] for x in sch]       # A/B/A/B by block, never per sample
    for side in ("phosphor", "reference"):
        rows = [x for x in sch if x[0] == side]
        assert sum(c - d for _, c, d in rows) == 60, side                                  # exactly n kept samples per side
        assert all(c - d <= 5 for _, c, d in rows), "a block is at most 5 kept samples"
        assert rows[0][2] == 3 + 1 and all(d == 1 for _, _, d in rows[1:]), "warm-up only on a side's first block; 1 lead sample dropped after every switch"
    short = lat.block_schedule(12, block=5, warm=0, lead=0)
    assert [(s_, c) for s_, c, _ in short] == [("phosphor", 5), ("reference", 5), ("phosphor", 5), ("reference", 5), ("phosphor", 2), ("reference", 2)]
    # the runner: a fake reference records each switch; the guard (switch) runs BEFORE every block, never inside one
    events = []
    ref = types.SimpleNamespace(switch=lambda side: events.append(("switch", side)))
    ctx = types.SimpleNamespace(reset=lambda: events.append(("reset",)))
    old_sleep, cases.time.sleep = cases.time.sleep, lambda s_: None
    try:
        def sampler(side, cnt):
            events.append(("sample", side, cnt))
            return [float(i) for i in range(cnt)]
        ph, rf, blocks = cases.run_interleaved(ctx, ref, 12, 2, sampler, block=5, lead=1)
        assert len(ph) == 12 and len(rf) == 12 and len(blocks) == 6
        sw = [e for e in events if e[0] != "reset"]
        assert all(sw[i][0] == "switch" and sw[i + 1][0] == "sample" and sw[i][1] == sw[i + 1][1] for i in range(0, len(sw), 2)), sw
        def failing(side):
            if side == "reference":
                raise RuntimeError("reference terminal does not have focus")
        ref.switch = failing
        try:
            cases.run_interleaved(ctx, ref, 12, 0, sampler, block=5, lead=0)
            raise AssertionError("a focus failure must propagate so the caller can fall back to sequential")
        except RuntimeError:
            pass
    finally:
        cases.time.sleep = old_sleep
    # the record says which mode ran; drift notes name it
    rec = lat.evaluate("x", [1.0] * 30, [1.0] * 30, 2.0, "overhead", "linux", mode="interleaved")
    assert rec["measurement_mode"] == "interleaved" and lat.evaluate("x", [1.0] * 30, [], 2.0, "overhead", "linux")["measurement_mode"] == "sequential"


@check
def idle_schedule_and_loss_bound():
    import lat
    sch = lat.idle_schedule([1.0, 2.0, 5.0], 30, 10, 3)
    for g in (1.0, 2.0, 5.0):
        for side in ("phosphor", "reference"):
            rows = [x for x in sch if x[0] == g and x[1] == side]
            assert sum(c - d for _, _, c, d in rows) == 30 and sum(d for *_, d in rows) == 3 and rows[0][3] == 3, (g, side, rows)
    assert sch[0][1] == "phosphor" and [x for x in sch if x[0] == 2.0][0][1] == "reference"      # the side that goes first alternates per gap
    assert all(a[1] != b[1] for a, b in zip([x for x in sch if x[0] == 1.0], [x for x in sch if x[0] == 1.0][1:]))      # A/B/A/B blocks
    assert {x[1] for x in lat.idle_schedule([1.0], 30, 10, 3, two_sided=False)} == {"phosphor"}
    assert abs(lat.loss_rate_bound(0, 40) - 0.0722) < 0.001 and abs(lat.loss_rate_bound(1, 40) - 0.1132) < 0.001   # 95 % one-sided Clopper-Pearson
    assert lat.loss_rate_bound(40, 40) == 1.0 and lat.loss_rate_bound(0, 0) is None
    assert lat.trials_to_exclude(1 / 40) == 119


@check
def keydown_audit_counts_plain_keys_and_flags_doubles():
    ln = lambda k, t, ctrl="false", shift="false": "input_order keydown key=%s ctrl=%s alt=false shift=%s cmd=false t=%s seq=1\n" % (k, ctrl, shift, t)
    ok = ln("g", 1000.0) + ln("i", 1020.5) + ln("t", 1041.0) + ln("Shift", 1050.0, shift="true") + ln("c", 1060.0, ctrl="true") + ln("Enter", 1080.0)
    a = cases.keydown_audit("junk\n" + ok)
    assert a["count"] == 4 and a["dups"] == [], a            # ctrl press and the bare Shift are not typed keys
    d = cases.keydown_audit(ln("a", 1000.0) + ln("a", 1000.4) + ln("b", 1030.0) + ln("b", 1060.0))   # 0.4 ms apart = the #1159 signature; 30 ms apart = a real repeat
    assert d["count"] == 4 and d["dups"] == [("a", 1000.4, 0.4)], d


@check
def ssh_trial_plan_and_loss_report():
    assert cases.ssh_trial_plan(cases.SSH_GAPS, 3) == [(g, 3) for g in cases.SSH_GAPS]      # default: 12 trials
    assert cases.ssh_trial_plan(cases.SSH_GAPS, 3, 40) == [(g, 10) for g in cases.SSH_GAPS]
    p = cases.ssh_trial_plan(cases.SSH_GAPS, 3, 42)
    assert sum(t for _, t in p) == 42 and [t for _, t in p] == [11, 11, 10, 10]
    z = cases.loss_report(0, 40)
    assert z["upper_bound"] == 0.0722 and "needs 119 clean trials" in z["text"], z
    o = cases.loss_report(1, 40)
    assert o["rate"] == 0.025 and o["upper_bound"] > 0.11
    assert cases.loss_report(0, 150)["upper_bound"] < 0.025 and "needs" not in cases.loss_report(0, 150)["text"]


@check
def profile_build_writes_every_file_in_a_fresh_dir():
    # r18d4 diag: #1550 wrote .refzdot/.zshrc into a directory nobody created; every pass crashed in prof.build. Build both modes offline.

    import profile as P
    for mode in ("terminal", "agent"):
        d = tempfile.mkdtemp()
        pr = P.Profile(os.path.join(d, "run"))
        pr.build(mode, None, "gpt-oss:20b")
        assert os.path.exists(os.path.join(pr.ref_zdot, ".zshrc")) and os.path.exists(pr.ref_rc), mode


@check
def no_function_local_import_shadows_a_module_import():
    # r18d4: `import winmap` inside run.main() made winmap a local of main, so every non-Windows run died with UnboundLocalError
    import ast, glob
    here = os.path.dirname(os.path.abspath(__file__))
    bad = []
    for path in glob.glob(os.path.join(here, "*.py")):
        t = ast.parse(open(path).read())
        top = {a.asname or a.name for n in t.body if isinstance(n, (ast.Import, ast.ImportFrom)) for a in n.names}
        for f in ast.walk(t):
            if isinstance(f, ast.FunctionDef):
                for n in ast.walk(f):
                    if isinstance(n, (ast.Import, ast.ImportFrom)):
                        bad += ["%s:%d %s" % (os.path.basename(path), n.lineno, a.asname or a.name) for a in n.names if (a.asname or a.name) in top]
    assert not bad, bad


@check
def injector_accounting_parse_and_join():
    import backend
    import injacct
    pi = backend.parse_injector
    # wtype: the priming Shift_L tap is not a key; text is a count; -k names are keys
    assert pi(["wtype", "-k", "Shift_L", "-s", "20", "-d", "15", "echo hi", "-k", "Return"]) == dict(chars=7, keys=["Return"])
    assert pi(["wtype", "-M", "ctrl", "-k", "u", "-m", "ctrl"]) == dict(chars=0, keys=["u"])
    assert pi(["wtype", "-P", "a", "-s", "10", "-p", "a", "-k", "Return"]) == dict(chars=0, keys=["a", "Return"])
    # xdotool: type + chained key, plain key specs, roll (keydown/keyup/sleep chain)
    assert pi(["xdotool", "type", "--delay", "15", "--", "abc", "key", "--delay", "0", "Return"]) == dict(chars=3, keys=["Return"])
    assert pi(["xdotool", "key", "--delay", "15", "ctrl+shift+t", "Left", "Left"]) == dict(chars=0, keys=["t", "Left", "Left"])
    assert pi(["xdotool", "keydown", "a", "sleep", "0.01", "keyup", "a", "key", "Return"]) == dict(chars=0, keys=["a", "Return"])
    assert pi(["xdotool", "keyup", "shift"]) == dict(chars=0, keys=[])
    # the recorder: one record per logical call, Enter counted, text never logged
    be = backend.X11Backend()
    be.record_injection(["xdotool", "type", "--", "secret", "key", "Return"], 100.0, 100.2)
    be.record_injection(["xdotool", "key", "Return"], 101.0, 101.1, err="RuntimeError: key injector failed")
    c = be.calls
    assert (c[0]["chars"], c[0]["enters_posted"], c[0]["enters_want"]) == (6, 1, 1) and (c[1]["enters_posted"], c[1]["enters_want"]) == (0, 1), c
    assert "secret" not in repr([x["special"] for x in c])
    # the join: reference-terminal calls are not expected in the app; modifier keydowns are not keys; a keydown and its typed_characters are one key
    be.record_injection(["xdotool", "type", "--", "ab"], 102.0, 102.1)
    be.guard_override = dict(pids={1})
    be.record_injection(["xdotool", "type", "--", "zzzzzzzz"], 103.0, 103.1)
    d = tempfile.mkdtemp()
    log = os.path.join(d, "phosphor.log")
    def kd(k, t, seq):
        return "input_order keydown key=%s ctrl=false alt=false shift=false cmd=false t=%.3f seq=%d\n" % (k, t * 1000, seq)
    open(log, "w").write("".join([kd("a", 100.01, 1), kd("b", 100.02, 2), kd("Shift", 100.03, 3), kd("Enter", 100.15, 4), kd("a", 102.01, 5)])
                         + "input_order typed_characters text=a t=%.3f seq=5\n" % 102010)
    txt, j = injacct.account(be.calls, log, "fast_typing")
    assert (j["sent"], j["seen"], j["lost"], j["enters_seen"]) == (9, 4, 5, 1), j      # sent = 6 + 1 + 2 (the errored call posted nothing: counted in errs, not in sent)
    assert txt.startswith("WARN loss case: injector sent 9; app saw 4; lost-before-app 5"), txt
    assert injacct.account(be.calls, None, "x")[1]["seen"] is None                 # no trace: reported, not compared
    assert injacct.account([], log, "x") == (None, None)


def main():
    failed = 0
    for f in CHECKS:
        try:
            f()
            print("ok   %s" % f.__name__)
        except Exception as e:
            failed += 1
            print("FAIL %s: %s: %s" % (f.__name__, type(e).__name__, e))
    print("selftest: %d checks, %d failed" % (len(CHECKS), failed))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
