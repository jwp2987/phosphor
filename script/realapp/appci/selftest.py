#!/usr/bin/env python3
# appci/selftest.py (#1522): offline harness self-test. Pure Python: no app, display, backend, network or ssh.
# usage: python3 script/realapp/appci/selftest.py      (also `run.py --self-test`, which runs this after its own checks)
# Covers the parsers and registries the verdicts hang on: Ctx.lines / stamps / parse_t / key_frame_pairs, the app-log readers (log_view_mode,
# mode_now's log-first rule, log_since), Profile.commands, routing classification, case-name validation, tier_plan, and the registry
# consistency checks (SMOKE, winmap, FIX_TIER name only registered cases: a renamed case used to drop out of smoke silently).
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
