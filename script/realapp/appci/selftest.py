#!/usr/bin/env python3
# appci/selftest.py (#1522): offline harness self-test. Pure Python: no app, display, backend, network or ssh.
# usage: python3 script/realapp/appci/selftest.py      (also `run.py --self-test`, which runs this after its own checks)
# Covers the parsers and registries the verdicts hang on: Ctx.lines / stamps / parse_t / key_frame_pairs, the app-log readers (log_view_mode,
# mode_now's log-first rule, log_since), Profile.commands, routing classification, case-name validation, tier_plan, and the registry
# consistency checks (SMOKE, winmap, FIX_TIER name only registered cases: a renamed case used to drop out of smoke silently).
import os
import json, os, subprocess, sys, tempfile, shutil, time, types

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
    # #1176: key -> pty write(2); the ctrl key, the untimed write line and hop lines never pair; two keys before one write both pair with it
    kw = "\n".join([
        "input_order keydown key=a ctrl=false alt=false shift=false cmd=false t=100.0 seq=1",
        "input_order pty_write bytes=1 t=100.4",
        "input_order keydown key=c ctrl=true alt=false shift=false cmd=false t=101.0 seq=2",
        "input_order hop=ctrl_c_pty_write bytes=1 t=101.2",
        "input_order keydown key=b ctrl=false alt=false shift=false cmd=false t=102.0 seq=3",
        "input_order keydown key=d ctrl=false alt=false shift=false cmd=false t=102.5 seq=4",
        "input_order hop=pty_write_partial wrote=1 of=2 t=102.6",
        "input_order pty_write n=2",
        "input_order pty_write bytes=2 t=103.0",
        "input_order keydown key=z ctrl=false alt=false shift=false cmd=false t=130.0 seq=5"])
    assert [round(x, 3) for x in cases.key_write_pairs(kw)] == [0.4, 1.0, 0.5], cases.key_write_pairs(kw)


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
    # #1593: the A5 verdict give-up lines (editor, deferred young Enter, Enter gate) count; other give-ups do not
    gu = ("input_order hop=enter_verdict_give_up seq=4 editor verdict_cap t=1\n"
          "input_order hop=kh_enter_gate_give_up seq=4 by=verdict_cap behind=0 submitted=false t=2\n"
          "input_order hop=enter_verdict_give_up seq=9 young_verdict_cap held_bytes=3 chars=2 t=3\n")
    assert ca.verdict_give_ups(gu) == 3
    assert ca.verdict_give_ups("input_order hop=enter_verdict_give_up seq=5 uncovered_report held_bytes=0 chars=0 t=4\n"
                               "input_order hop=kh_enter_gate_give_up seq=6 by=route_wait behind=1 submitted=true t=5") == 0
    assert ca.verdict_give_ups("") == 0


@check
def agent_input_exact_log_judgement():
    """#1510: the Rust Debug string in hop=enter_begin is unescaped and compared byte for byte"""
    sent = 'please explain why "build 7f3a" isn\'t passing, it\'s odd (really)?'
    log = 'input_order hop=enter_begin deferred=false buffer="please explain why \\"build 7f3a\\" isn\'t passing, it\'s odd (really)?" input_type=AI t=1\n'
    rows = ca.enter_buffers(log)
    assert rows == [(sent, "AI")], rows
    assert ca.judge_agent_input(sent, rows) is None
    assert "differs" in ca.judge_agent_input(sent + "x", rows) and "char" in ca.judge_agent_input("plAase" + sent, rows)
    assert "routed as Shell" in ca.judge_agent_input(sent, [(sent, "Shell")]) and "no hop=enter_begin" in ca.judge_agent_input(sent, [])
    assert ca.judge_agent_input("abcd", ca.enter_buffers("hop=enter_begin deferred=false buffer=<4 chars> input_type=AI")) is None
    assert ca.judge_agent_input("abcd", ca.enter_buffers("hop=enter_begin deferred=false buffer=<3 chars> input_type=AI")) is not None
    assert ca.unescape_rust_debug('"a\\n\\t\\\\ \\u{e9}"') == "a\n\t\\ \u00e9"
    # a lost or doubled character is caught; the LAST Enter is the one judged (an earlier line is history)
    assert ca.judge_agent_input("hi there", [("old", "Shell"), ("hi there", "AI")]) is None
    assert ca.judge_agent_input("hi there", [("hi ther", "AI")]) is not None
    assert "agent_input_exact" in {c["name"] for c in cases.CASES} and "agent_mode_switch" in cases.SMOKE


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
def enter_segments_count_the_powershell_insert_chord():
    """#1401: a PowerShell line is served on the pipe and only the 2-byte chord is written; that write is the line's pty write"""
    L = ["input_order hop=enter_begin t=1000.0", "input_order hop=pwsh_command_via_pipe bytes=83 t=1001.0",
         "input_order pty_write bytes=2 t=1001.5", "input_order hop=preexec_received signal=preexec t=1017.5",
         "input_order hop=enter_begin t=2000.0", "input_order pty_write bytes=2 t=2000.4"]          # a 2-byte write with no chord hop: not the line
    s = cases.enter_segments(L, 0)
    assert s["enter_to_write_ms"] == 1.5 and s["enter_to_write_n"] == 1, s
    assert s["write_to_preexec_ms"] == 16.0, s


@check
def pwsh_hook_timing_marks_land_on_the_app_timeline():
    """#1401: pwsh.ps1 marks (Stopwatch ticks) are placed on the app's epoch timeline through the qpc= calibration of its log lines"""
    import pwsh_timing as pt
    f, base = 10_000_000, 1_793_713_705_000.0          # 10 MHz counter; epoch ms of qpc 0 is `base`
    q = lambda ms: int(round(ms * f / 1000.0))          # noqa: E731  (epoch offset ms -> ticks)
    lines = ["input_order hop=pwsh_command_via_pipe bytes=83 qpc=%d qpf=%d t=%.3f" % (q(100.0), f, base + 100.0),
             "input_order hop=preexec_received signal=preexec t=%.3f qpc=%d qpf=%d" % (base + 120.0, q(120.0), f),
             "input_order hop=ctrl_c_pty_write bytes=1 t=%.3f" % (base + 500.0),
             "input_order hop=line_editor_ready_after_ctrl_c via=line_editor_reading since_ctrl_c_ms=80.5 t=%.3f" % (base + 580.5)]
    assert abs(pt.calibration(lines) - base) < 0.01
    marks = "freq %d\n" % f + "".join("%s %d\n" % (n, q(ms)) for n, ms in
                                      (("k0", 106.0), ("k1", 108.0), ("k2", 109.0), ("r", 112.0), ("p", 114.0), ("q", 115.0),
                                       ("c", 160.0), ("m", 170.0), ("x", 171.0), ("i", 172.0), ("e", 180.0), ("c", 560.0), ("e", 575.0)))
    s = pt.segments(marks, lines, stamps_ms=[base + 130.0])
    assert s["app_write_to_handler"] == 6.0 and s["handler_pipe_read"] == 2.0 and s["accept_line"] == 3.0, s
    assert s["readline_to_preexec"] == 2.0 and s["preexec_osc_to_app"] == 6.0 and s["preexec_tail"] == 1.0, s
    assert s["hook_to_command_start"] == 15.0 and s["prompt_precmd"] == 10.0 and s["readline_maintenance"] == 8.0, s
    assert pt.segments("", lines) == {} and pt.segments(marks, []) == {}            # no marks or no calibration: nothing, not zeros
    c = pt.ctrl_c_segments(marks, lines)
    assert c["ctrl_c_to_prompt"] == 60.0 and c["ctrl_c_to_reading"] == 75.0 and c["line_editor_ready_after_ctrl_c"] == 80.5, c


@check
def enter_split_attributes_every_hop():
    # #1606: the app-side hops of one Enter, in the shape the app logs them (r18e4 agent mode, abbreviated)
    L = ["input_order hop=mac_enter stage=app_send_event keycode=36 ts=1.0 age_ms=4.250 event_window=1 key_window=1 key_window_class=WarpWindow t=1000.0",
         "input_order hop=enter_begin deferred=false buffer=\"x\" input_type=Shell t=1000.9", "input_order hop=try_execute_done executed=true took_ms=0.0 t=1001.2",
         "input_order hop=enter_submit_done since_submit_ms=0.70 t=1001.9", "input_order hop=write_command  t=1002.0", "input_order pty_write bytes=71 t=1002.2",
         "input_order pty_read bytes=420 since_write_ms=1.4 t=1003.6", "input_order hop=pty_lock_wait waited_ms=1.50 buffered=0 t=1005.1",
         "input_order hop=preexec_received signal=preexec t=1005.2"]
    s = cases.enter_segments(L, 0)
    assert s["event_age_ms"] == 4.25 and s["event_to_enter_ms"] == 0.9 and s["enter_to_write_ms"] == 1.3, s
    assert s["submit_bookkeeping_ms"] == 0.7 and s["effect_flush_ms"] == 0.1, s
    assert s["write_to_preexec_ms"] == 3.0 and s["pty_lock_wait_ms"] == 1.5, s
    # a build without the new hops gives exactly the old keys (absent, never zero)
    old = [l for l in L if "age_ms" not in l and "enter_submit_done" not in l and "pty_lock_wait" not in l]
    s = cases.enter_segments(old, 0)
    assert "event_age_ms" not in s and "submit_bookkeeping_ms" not in s and "pty_lock_wait_ms" not in s, s
    # a #1606 build that logged no lock wait for an Enter: 0, not absent
    s = cases.enter_segments([l for l in L if "pty_lock_wait" not in l], 0)
    assert s["pty_lock_wait_ms"] == 0.0, s
    # the shell-side split: post 0, zle-line-finish +2 ms, last preexec +3 ms, stamp +3.5 ms; a stale px (> 1 s) is dropped, not averaged in
    sp = cases.enter_split_summary([dict(t0=99.8, post=100.0, lf=100.002, px=100.003, stamp=100.0035),
                                    dict(t0=199.8, post=200.0, lf=200.002, px=198.0, stamp=200.0035)])
    assert sp["post_to_lf_ms"] == 2.0 and sp["lf_to_px_ms"] == 1.0 and sp["px_to_stamp_ms"] == 0.5 and sp["post_to_stamp_ms"] == 3.5, sp
    assert round(sp["t0_to_post_ms"]) == 200 and sp["n"] == 2, sp
    assert cases.enter_split_summary([dict(t0=1.0, post=None, lf=None, px=None, stamp=1.2)]) == {"n": 1}
    # #1492: no injector post time (Windows, Linux): harness Return -> line accepted, and line accepted -> stamp, from the bash PS0 time
    sp = cases.enter_split_summary([dict(t0=1793758441.0, post=None, lf=cases._epoch_s(1793758441030000.0), px=None, stamp=1793758441.0335)])
    assert round(sp["t0_to_lf_ms"], 2) == 30.0 and round(sp["lf_to_stamp_ms"], 2) == 3.5 and "post_to_lf_ms" not in sp, sp
    assert cases._epoch_s(1793758441853045.0) == 1793758441.853045 and cases._epoch_s(1793758441.85) == 1793758441.85 and cases._epoch_s(None) is None
    # the split stamp line is opt-in (APPCI_ENTER_SPLIT=1); the default line is the old one, byte for byte
    gb = types.SimpleNamespace(pane="gitbash", be=types.SimpleNamespace(name="windows"))
    saved = os.environ.pop("APPCI_ENTER_SPLIT", None)
    try:
        assert cases.stamp_line(gb) == "date +%%s.%%N > lat_%d", cases.stamp_line(gb)
        os.environ["APPCI_ENTER_SPLIT"] = "1"
        line = cases.stamp_line(gb)
        assert line == "date +%%s.%%N > lat_%d;echo $__pa_lf>>lat_%d" and line % ((3,) * line.count("%d")) == "date +%s.%N > lat_3;echo $__pa_lf>>lat_3", line
    finally:
        os.environ.pop("APPCI_ENTER_SPLIT", None)
        if saved is not None:
            os.environ["APPCI_ENTER_SPLIT"] = saved
    d = tempfile.mkdtemp()
    try:
        f = os.path.join(d, "lat_0")
        assert cases._read_fields(f) == []
        open(f, "w").write("1793758441.853 \x00\n1793758441850000\n")
        assert cases._read_fields(f) == ["1793758441.853", "1793758441850000"]
    finally:
        shutil.rmtree(d)
    # `${__pa_px:-0}`: a hook that never ran prints 0, which is "no value", never a time
    assert cases._float_or_none(["1.5", "0", "x"], 1) is None and cases._float_or_none(["1.5", "0", "x"], 2) is None
    assert cases._float_or_none(["1.5", "2.25"], 1) == 2.25 and cases._float_or_none(["1.5"], 1) is None
    # key-event ages: flags filter, a nonsense (negative) age is dropped
    K = ["input_order hop=os_key_event age_ms=0.300 enter=false ctrl=false t=1000.0", "input_order hop=os_key_event age_ms=40.000 enter=false ctrl=true t=1001.0",
         "input_order hop=os_key_event age_ms=-5.000 enter=false ctrl=true t=1002.0", "input_order hop=os_key_event age_ms=2.000 enter=true ctrl=false t=1003.0"]
    assert cases.os_key_ages(K, 0, ctrl=True) == [40.0] and cases.os_key_ages(K, 0, enter=False, ctrl=False) == [0.3]
    assert cases.os_key_ages(K, 1.0025) == [2.0]
    cs = cases.ctrl_c_app_split(K + ["input_order hop=ctrl_c_keydown powershell_setup_hold=Off t=1001.1", "input_order hop=ctrl_c_pty_write bytes=1 t=1001.4",
                                     "input_order hop=drawable_wait wait_ms=18.00 key_frame=false t=1000.5"], 0)
    assert cs["ctrl_event_age_ms"]["p50"] == 40.0 and abs(cs["key_to_write_ms"]["p50"] - 0.3) < 1e-6 and cs["drawable_wait_ms"]["max"] == 18.0, cs


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
    # r18u19: the same request-shaped line typed in the plain terminal is the agent's too (#834/#965, routing corpus)
    assert w("cat the error log and tell me what is wrong", "PlainTerminal", "request-shape (#834/#965)") == "agent"
    assert w("make a PR", "PlainTerminal", "installed-head (#906/#964)") == "shell"
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
def r18u22_probe_ctrl_c_waits_out_the_exit_window():
    """r18u22 (Windows agent@gitbash left_agent_mode_after_fast_typing / _read_x): pane_responsive's FIRST Ctrl-C came 0.8 s after the previous case's reset()
    Ctrl-C and exited the empty agent view. Every probe Ctrl-C in agent mode now waits until the last one is outside EXIT_WINDOW_S + 0.4."""
    slept = []
    c = types.SimpleNamespace(ctrlc_ts=[100.0], mode="agent", EXIT_WINDOW_S=cases.Ctx.EXIT_WINDOW_S)
    left = cases.Ctx.wait_out_exit_window(c, now=100.8, sleep=slept.append)
    assert abs(left - 0.6) < 1e-6 and slept == [left], (left, slept)
    assert cases.Ctx.wait_out_exit_window(c, now=101.5, sleep=slept.append) == 0.0 and len(slept) == 1
    c.mode = "terminal"
    assert cases.Ctx.wait_out_exit_window(c, now=100.1, sleep=slept.append) == 0.0 and len(slept) == 1
    c.mode, c.ctrlc_ts = "agent", []
    assert cases.Ctx.wait_out_exit_window(c, now=100.1, sleep=slept.append) == 0.0
    # the probe itself: a Ctrl-C the case sent just now -> the probe's Ctrl-C goes out only after the window (recorded time, not counted presses)
    sent, t = [], [200.0]
    real_time, real_sleep = cases.time.time, cases.time.sleep
    cases.time.time = lambda: t[0]

    def fake_sleep(d):
        t[0] += d
    cases.time.sleep = fake_sleep
    try:
        be = types.SimpleNamespace(key=lambda spec, *a, **k: sent.append((t[0], spec)), keys=lambda specs, *a, **k: [sent.append((t[0], x)) for x in specs],
                                   type=lambda *a, **k: None)
        ctx = types.SimpleNamespace(ctrlc_ts=[], mode="agent", be=be, EXIT_WINDOW_S=cases.Ctx.EXIT_WINDOW_S, pane="gitbash")
        cases.Ctx._wrap_ctrlc(ctx, be)
        ctx.wait_out_exit_window = lambda: cases.Ctx.wait_out_exit_window(ctx)
        ctx.tok = lambda n: "abcdefgh"[:n]
        ctx.cmd_mark = lambda: 0
        ctx.pane_shell = lambda: "bash"
        ctx.append_cmd = lambda shell, tok, fname: "echo %s >> %s" % (tok, fname)
        ctx.until = lambda pred, timeout=10, step=0.05: pred()
        ctx.local_lines = lambda fname: ["abcdefgh"]
        ctx.prof = types.SimpleNamespace(commands=lambda: [])
        be.key("ctrl+c")                      # the case's own reset() Ctrl-C at t=200.0
        t[0] += 0.8                           # the runner calls the probe 0.8 s later
        assert cases.Ctx.pane_responsive(ctx) == 0
        cc = [ts for ts, k in sent if k == "ctrl+c"]
        assert len(cc) == 2 and cc[1] - cc[0] >= cases.Ctx.EXIT_WINDOW_S + 0.3, cc
        assert not cases.Ctx.double_ctrlc_since(ctx, 0.0), ctx.ctrlc_ts
    finally:
        cases.time.time, cases.time.sleep = real_time, real_sleep


@check
def r18u8_harness_fixes_pure_pieces():
    import types as _t
    # Mac injector records of keys aimed at the reference carry ref=True, so the accounting does not expect Phosphor to have seen them
    import injacct
    calls = [dict(chars=10, special=[], enters_posted=0, ref=False, wall0=1.0, wall1=2.0, pastes=0),
             dict(chars=90, special=["ENTER@3.0"], enters_posted=1, ref=True, wall0=2.0, wall1=3.0, pastes=0)]
    j = injacct.join(calls, [(1.5, 1, None)] * 10)
    assert j["sent"] == 10 and j["enters_sent"] == 0 and j["lost"] == 0, j
    # a blinking caret is not a streaming reply: identical frames settle; with no Pillow, 3 distinct hashes are still "changing"
    import tempfile as _tf
    d = _tf.mkdtemp()
    try:
        ps = []
        for i, b in enumerate((b"a", b"a", b"a")):
            q = os.path.join(d, "f%d.png" % i)
            open(q, "wb").write(b)
            ps.append(q)
        assert ca.frames_settled(ps)[0] is True
        ps2 = []
        for i, b in enumerate((b"a", b"b", b"c")):
            q = os.path.join(d, "g%d.png" % i)
            open(q, "wb").write(b)
            ps2.append(q)
        import cases_ui as _U
        if not _U.have_pillow():
            assert ca.frames_settled(ps2)[0] is False
    finally:
        shutil.rmtree(d, ignore_errors=True)
    # a failed overhead budget against a conhost reference is NOREF (NO-CONPTY-REFERENCE), never a FAIL; against a ConPTY reference it still fails
    rec = dict(budget_verdict="fail", budget_kind="overhead", overhead_ms=40.0, overhead_se_ms=1.0, reference_name="conhost (bash)", budget_ms=2, verdict="fail", notes=[])
    r = cases.Result("x", "terminal")
    cases.lat_checks(r, dict(rec), "Enter")
    assert not r.fails and any("NO-CONPTY-REFERENCE" in n for n in r.notes), (r.fails, r.notes)
    r = cases.Result("x", "terminal")
    cases.lat_checks(r, dict(rec, reference_name="Windows Terminal (bash)"), "Enter")
    assert r.fails, "a Windows Terminal reference keeps the budget verdict"
    # PATH with spaces: the clean-env subshell command quotes its variables
    import cases_caseremote as _cr
    import inspect
    assert 'PATH="$PATH"' in inspect.getsource(_cr.remote_session)


@check
def r18u19_heredoc_dialog_is_read_from_the_app_log():
    import cases_caseremote as rm
    shown = 'x frame 1: updated=["MultilineSubmitConfirmationDialog", "EditorView"] removed=0\n'
    answered = 'x hop=core_enter stage=action_handled step=2 action_type=warp::workspace::multiline_submit_confirmation_dialog::MultilineSubmitConfirmationAction handled=true\n'
    submitted = 'x input_order hop=try_execute_done executed=true took_ms=0.1 t=1\n'
    assert rm.dialog_state("") is None
    assert rm.dialog_state(shown) == "open"
    assert rm.dialog_state(shown + answered) is None
    assert rm.dialog_state(shown + answered + submitted) == "submitted"
    assert rm.dialog_state(submitted) == "submitted"
    assert rm.dialog_state(shown + answered + submitted + shown) == "open"          # asked again for the next paste
    # the core_enter context names the dialog without a quote: not a drawing of it
    assert rm.dialog_state("x hop=core_enter stage=match_step context=MultilineSubmitConfirmationDialog t=1\n") is None
    assert rm.dialog_wait_s(10_000) < 7 and 17 < rm.dialog_wait_s(600_000) < 19 and rm.dialog_wait_s(10**8) == 40.0
    assert rm.PS_CPU_RE.match("  123  98.5 /bin/bash").groups() == ("123", "98.5", "/bin/bash")


@check
def r18u19_settings_probe_and_theme_diff():
    import cases_coverage_b as cb, cases_ui as ui
    # probe 1 swallowed by the settings page, probe 2 ran: probe 2 is judged by its own line
    assert cb.probe_landed(["probe2"], 2) and not cb.probe_landed(["probe2"], 1) and not cb.probe_landed(["probe1"], 2)
    try:
        from PIL import Image
    except ImportError:
        return
    d = tempfile.mkdtemp()
    try:
        a, b, c = (os.path.join(d, n) for n in ("a.png", "b.png", "c.png"))
        Image.new("RGB", (40, 40), (30, 32, 46)).save(a)           # one dark theme ...
        Image.new("RGB", (40, 40), (10, 25, 15)).save(b)           # ... and another: ~14 grey levels apart
        Image.new("RGB", (40, 40), (30, 32, 46)).save(c)
        assert ui.img_diff(a, b) == 0.0                           # the grey-level test misses it (the r18u19 failure)
        assert ui.img_diff(a, b, color=True) == 1.0               # the colour test sees the whole recolour
        assert ui.img_diff(a, c, color=True) == 0.0               # and nothing between identical shots
    finally:
        shutil.rmtree(d, ignore_errors=True)


@check
def r18u19_windows_where_probe_ignores_earlier_runs():
    # the seeded tokens repeat in every run: a token an EARLIER run left in the real home's ~/.appci_where must not make a probe "LOCAL"
    d = tempfile.mkdtemp()
    try:
        f = os.path.join(d, ".appci_where")
        open(f, "wb").write("wT235_x9nmejpp\r\n".encode("utf-16"))         # an earlier run, Windows PowerShell 5.1 `>>` (UTF-16, BOM)
        skip = cases.where_file_size(f)
        assert cases.where_tokens(f) == ["wT235_x9nmejpp"]
        assert cases.where_tokens(f, skip) == []
        open(f, "ab").write("wT236_bpz32gdl\r\n".encode("utf-16-le"))       # this run's append: no BOM
        assert cases.where_tokens(f, skip) == ["wT236_bpz32gdl"]
        open(f, "wb").write(b"wT1_new\n")                                    # replaced since the snapshot: all of it is new
        assert cases.where_tokens(f, skip) == ["wT1_new"]
        assert cases.where_file_size(os.path.join(d, "none")) == 0 and cases.where_tokens(os.path.join(d, "none")) == []
        c = types.SimpleNamespace(prof=types.SimpleNamespace(home=os.path.join(d, "scratch")), pane="gitbash")
        assert cases.Ctx.where_homes(c) == [c.prof.home]
        c.pane = "powershell"
        assert cases.Ctx.where_homes(c) == [c.prof.home, os.path.expanduser("~")]
        c.where_homes = lambda: [d]
        c.where_skip = {d: skip}
        open(f, "wb").write("wT235_x9nmejpp\r\n".encode("utf-16"))
        skip = cases.where_file_size(f)
        c.where_skip = {d: skip}
        assert cases.Ctx.local_where(c) == []
        open(f, "ab").write("wT236_bpz32gdl\r\n".encode("utf-16-le"))
        assert cases.Ctx.local_where(c) == ["wT236_bpz32gdl"]
    finally:
        shutil.rmtree(d, ignore_errors=True)


@check
def r18u19_windows_relaunch_heredoc_io_and_back_line():
    import cases_caseremote as rm
    import inspect
    # editor_memory_7mb trim A/B: a Windows PowerShell pane never moves prompts(); the relaunch waits for the window + fixed settle + key proof instead
    calls = []
    sleeps = []
    real_sleep = time.sleep
    c = types.SimpleNamespace(win=True, prof=types.SimpleNamespace(local_prompts=False), prompts=lambda: 0, snapshot_log=lambda: None,
                              app=types.SimpleNamespace(stop=lambda: calls.append("stop"), start=lambda: calls.append("start"),
                                                        wait_window=lambda t: calls.append("window") or True, alive=lambda: True),
                              be=types.SimpleNamespace(guard=lambda: calls.append("guard")), reprove=lambda: calls.append("reprove") or None,
                              reset=lambda: calls.append("reset"))
    c.until = lambda pred, timeout=10, step=0.05: pred()
    saved = dict(os.environ)
    try:
        time.sleep = lambda s: sleeps.append(s)
        assert cases.editor_mem_relaunch(c, {"PHOSPHOR_HEAP_TRIM": "0"}) is True
        assert calls == ["stop", "start", "window", "guard", "reprove", "reset"], calls
        assert os.environ.get("PHOSPHOR_HEAP_TRIM") == "0" and 12 in sleeps
        c.reprove = lambda: "keys not delivered: x"
        assert cases.editor_mem_relaunch(c, {}) is False and "PHOSPHOR_HEAP_TRIM" not in os.environ
    finally:
        time.sleep = real_sleep
        os.environ.clear()
        os.environ.update(saved)
    # remote_heredoc_paste: the Windows fake sshd's input/output accounting since the paste is attached to a failing step
    d = tempfile.mkdtemp()
    try:
        os.makedirs(os.path.join(d, "ssh"))
        log = os.path.join(d, "ssh", "sshd.log")
        open(log, "w").write("19:39:40 connect from x\n19:39:41 conpty first output after 0.58 s: 68 bytes\n")
        cx = types.SimpleNamespace(run=d)
        off = rm.sshd_log_size(cx)
        open(log, "a").write("19:39:42 client input so far: 131072 bytes in 3 chunks, 65536 written to the pseudo console\n19:39:43 auth x\n"
                             "19:45:22 conpty output ended: 68 bytes sent to the client in total\n")
        got = rm.sshd_io_lines(cx, off)
        assert len(got) == 2 and "131072" in got[0] and "68 bytes" in got[1], got
        assert rm.sshd_io_lines(types.SimpleNamespace(run=os.path.join(d, "none")), 0) == []
    finally:
        shutil.rmtree(d, ignore_errors=True)
    # remote_session: the local command after `exit` is looked for in rec.log until the hook (at the NEXT prompt) has written it
    src = inspect.getsource(rm.remote_session)
    assert "ctx.until(lambda: back in ctx.cmds_since(lmark), 5)" in src
    # the fake sshd accounts every client byte (received vs written) for the next H7 run
    fs = open(os.path.join(HERE, "fakesshd_win.py")).read()
    assert "client input so far" in fs and "client input ended" in fs and "conpty input: writing" in fs


@check
def r18u22_heredoc_wrapper_mirror_and_control_verdicts():
    """#1129 (r18u22): the harness's copy of the app's one-line wrapper is byte-identical to posix_long_command.rs (format string, script head, both
    size thresholds); a step with no file is KNOWN only when the plain-ssh control proves the remote line editor is the bottleneck (and only up to
    v0.1.17), FAIL when plain ssh was clearly faster or the bytes are wrong, NOREF on Windows when the reference console's ssh fails the same way."""
    import re as _re
    import cases_caseremote as rm
    import inspect
    rs = open(os.path.join(HERE, "..", "..", "..", "app", "src", "terminal", "writeable_pty", "posix_long_command.rs")).read()

    def rust_fmt(holder):
        """the format! string literal of wrapper_line that holds `holder`, unescaped"""
        body = rs[rs.index("pub(super) fn wrapper_line"):]
        for m in _re.finditer(r'format!\(\s*"((?:[^"\\]|\\.)*)"', body, _re.S):
            if holder in m.group(1):
                lit = _re.sub(r"\\\n\s*", "", m.group(1))     # a Rust string's backslash-newline skips the newline and the next line's indent
                return lit.replace('\\"', '"').replace("\\n", "\n")
        raise AssertionError("no format! holding %s in wrapper_line" % holder)
    tmpl = rust_fmt("{encoded}").replace("{encoded}", "\0").replace("{{", "{").replace("}}", "}")
    assert tmpl.replace("\0", "QUJD") == rm.wrapper_from_encoded("QUJD"), (tmpl, rm.wrapper_from_encoded("QUJD"))
    head = rust_fmt("{command}").replace("{command}", "\0")
    assert head == rm.WRAPPER_SCRIPT_HEAD + "\0\n", (head, rm.WRAPPER_SCRIPT_HEAD)
    assert "MIN_WRAPPED_BYTES: usize = 128 * 1024;" in rs and rm.WRAPPED_MIN == 128 * 1024
    assert "MIN_WRAPPED_BYTES_BRACKETED: usize = 512 * 1024;" in rs and rm.WRAPPED_MIN_BRACKETED == 512 * 1024
    import base64
    line = rm.wrapper_line("cat > f <<'E'\na\nE")
    enc = _re.search(r"printf %s '([A-Za-z0-9+/=]+)'", line).group(1)
    assert base64.b64decode(enc).decode() == 'rm -f "$__pc"; unset __pc\ncat > f <<\'E\'\na\nE\n'
    assert rm.app_wraps("x" * (128 * 1024), False) and not rm.app_wraps("x" * (128 * 1024 - 1), False)
    assert not rm.app_wraps("x" * (600 * 1024), True) and rm.app_wraps("x\n" * (300 * 1024), True) and not rm.app_wraps("x\n" * 1000, True)
    # the Windows fake sshd's accounting
    assert rm.sshd_input_bytes(["client input so far: 131072 bytes in 3 chunks, 65536 written", "client input ended: 68 bytes in 2 chunks, 68 written"]) == 131072
    assert rm.sshd_input_bytes(["conpty output ended: 9 bytes sent"]) is None
    # verdicts, with the r18u22 macOS numbers: bash 3.2 at 100 %, the app at 52 %, 190 KB wrapper line 86 s, the 600 KB step no file after 180 s

    def rec(name, secs, got=None, want=600008, late=None, ok=False, exact=False, wrapped_len=800_400, sent=600_050, peak=None):
        return dict(label="L", name=name, sent=sent, want=want, got=got, secs=secs, late=late, timeout=180 if name == "hd_big.txt" else 90, peak=peak or {},
                    wrapped_len=wrapped_len, ok=ok, exact=exact, first=None if got is None or exact else 7)
    ctl_env = {"H": dict(s=4.0, got=142166, exact=True, peak={}, why=None, label="h"),
               "W": dict(s=86.0, got=142166, exact=True, peak={"bash": 100.0}, why=None, label="w", line=190_000)}
    ctl_fast = {"H": ctl_env["H"], "W": dict(ctl_env["W"], s=3.0)}
    orig_v = cases.app_version
    try:
        cases.app_version = lambda repo=None: "0.1.17"
        r = cases.Result("t", "terminal")
        rm.judge_step(None, r, rec("hd_big.txt", 180.6, peak={"bash": 100.0, "app": 292.5}), ctl_env, False)
        assert not r.fails and len(r.notes) == 1 and r.notes[0].startswith("KNOWN: L: remote file not byte-exact") and "ENV: plain ssh" in r.notes[0], (r.fails, r.notes)
        assert "ACCEPTED_FOR_RELEASE #1129 v0.1.17" in r.notes[0] and "the app's one-line wrapper is the slow path" in r.notes[0], r.notes
        assert "'bash': 100.0" in r.notes[0], r.notes                               # the numbers travel with the verdict
        cases.app_version = lambda repo=None: "0.1.18"
        r = cases.Result("t", "terminal")
        rm.judge_step(None, r, rec("hd_big.txt", 180.6), ctl_env, False)
        assert len(r.fails) == 1 and "but no ACCEPTED_FOR_RELEASE entry is in force" in r.fails[0] and not r.notes, (r.fails, r.notes)
        cases.app_version = lambda repo=None: "0.1.17"
        for ctl, bracketed, want in ((ctl_fast, False, "APP: plain ssh took 3.0 s"), (None, False, "(control: no control ran)")):
            r = cases.Result("t", "terminal")
            rm.judge_step(None, r, rec("hd_big.txt", 180.6), ctl, bracketed)
            assert len(r.fails) == 1 and want in r.fails[0] and not r.notes, (want, r.fails, r.notes)
        # wrong bytes are never KNOWN
        r = cases.Result("t", "terminal")
        rm.judge_step(None, r, rec("hd_big.txt", 12.0, got=600008), ctl_env, False)
        assert len(r.fails) == 1 and "first difference at byte 7" in r.fails[0], r.fails
        # the 2k step: finished in 86 s next to a plain-ssh 86 s is a pass; 140 s next to 50 s fails as the slow side; late but no slower than plain ssh is ENV
        r = cases.Result("t", "terminal")
        rm.judge_step(None, r, rec("hd_2k.txt", 86.4, got=142166, want=142166, ok=True, exact=True, wrapped_len=190_000, sent=142_200), ctl_env, False)
        assert not r.fails and not r.notes, (r.fails, r.notes)
        r = cases.Result("t", "terminal")
        slow = {"H": ctl_env["H"], "W": dict(ctl_env["W"], s=50.0)}
        rm.judge_step(None, r, rec("hd_2k.txt", 140.0, got=142166, want=142166, ok=True, exact=True, wrapped_len=190_000, sent=142_200), slow, False)
        assert len(r.fails) == 1 and "Phosphor is the slow side" in r.fails[0], r.fails
        r = cases.Result("t", "terminal")
        rm.judge_step(None, r, rec("hd_2k.txt", 90.2, got=142166, want=142166, late=95.0, exact=True, wrapped_len=190_000, sent=142_200), ctl_env, False)
        assert not r.fails and r.notes and r.notes[0].startswith("KNOWN:") and "arrived after 95.0 s" in r.notes[0], (r.fails, r.notes)
        # with bracketed paste (Linux bash 5.x) the 2k here-doc is NOT wrapped: the wrapper control does not cover it
        r = cases.Result("t", "terminal")
        rm.judge_step(None, r, rec("hd_2k.txt", 90.2, want=142166, wrapped_len=190_000, sent=142_200), ctl_env, True)
        assert len(r.fails) == 1 and "not (provably) sent through the app's wrapper" in r.fails[0], r.fails
        # the zsh / 10 KB steps are never judged by the wrapper control
        r = cases.Result("t", "terminal")
        rm.judge_step(None, r, rec("hd_zsh.txt", 120.5, want=142166), ctl_env, False)
        assert len(r.fails) == 1 and "not a bash wrapper-path step" in r.fails[0], r.fails
        # Windows: the reference console's ssh failing the same here-doc -> NOREF with the sshd's byte count; delivering it -> APP FAIL; no control -> FAIL
        io = ["client input ended: 68 bytes in 2 chunks, 68 written to the pseudo console"]
        r = cases.Result("t", "terminal")
        rm.judge_step(None, r, rec("hd_2k.txt", 90.1, want=142166, sent=142_200), {"H": dict(s=None, got=-1, exact=False, peak={}, why="no file within 120 s", label="h")}, None, io)
        assert not r.fails and r.noref and "PLATFORM" in r.noref[0] and "test sshd: 68 client bytes received of the 142200-byte paste" in r.noref[0], (r.fails, r.noref)
        assert r.metrics["hd_2k_txt_sshd_got"] == 68
        r = cases.Result("t", "terminal")
        rm.judge_step(None, r, rec("hd_2k.txt", 90.1, want=142166, sent=142_200), {"H": dict(s=7.5, got=142166, exact=True, peak={}, why=None, label="h")}, None)
        assert len(r.fails) == 1 and "APP: the reference console's ssh delivered the same here-doc in 7.5 s" in r.fails[0], r.fails
        r = cases.Result("t", "terminal")
        rm.judge_step(None, r, rec("hd_2k.txt", 90.1, want=142166, sent=142_200), {"H": dict(s=None, got=-1, exact=False, peak={}, why="reference console not available: x", label="h")}, None)
        assert len(r.fails) == 1 and "could not run" in r.fails[0], r.fails
        r = cases.Result("t", "terminal")
        rm.judge_step(None, r, rec("hd_line.txt", 60.1, want=10001, sent=10_040), {"H": dict(s=None, got=-1, exact=False, peak={}, why="no file within 120 s", label="h")}, None)
        assert len(r.fails) == 1 and "does not cover it" in r.fails[0], r.fails
    finally:
        cases.app_version = orig_v
    assert cases.accepted_for_release("remote_heredoc_remote_editor", "0.1.17")["issue"] == 1129
    assert cases.accepted_for_release("remote_heredoc_remote_editor", "0.1.18") is None
    v = cases.app_version()
    assert v and cases.version_tuple(v) <= (0, 1, 17), "ACCEPTED_FOR_RELEASE remote_heredoc_remote_editor (#1129) was for v0.1.17; app/Cargo.toml is %s: re-decide it" % v
    # the case: the 600 KB step runs LAST (its timeout must not take the zsh/fish steps with it), a step without a file ends the stuck remote shell,
    # the remote shell's pid is recorded on connect, and the verdicts come after the control
    src = inspect.getsource(rm.c_remote_heredoc)
    assert src.index('"hd_big.txt"') > src.index('for key in ("zsh", "fish")'), "the 600 KB step must run after the zsh/fish steps"
    assert "end_stuck_session(ctx, r, label)" in src and "record_rpid(ctx)" in src and src.index("heredoc_control(") < src.index("judge_step(")
    assert "r.check(" not in inspect.getsource(rm._send_step), "_send_step records; judge_step decides"
    if os.name != "nt":
        p = subprocess.Popen(["sleep", "30"])
        try:
            assert p.pid in rm.descendants(os.getpid()), "descendants() misses a child"
        finally:
            p.kill()
            p.wait()
        # the control's pty pump: what is written comes back (cat echoes), nothing blocks on a full pty
        import pty
        pid, fd = pty.fork()
        if pid == 0:
            try:
                os.execv("/bin/cat", ["cat"])
            finally:
                os._exit(127)
        import fcntl
        fcntl.fcntl(fd, fcntl.F_SETFL, fcntl.fcntl(fd, fcntl.F_GETFL) | os.O_NONBLOCK)
        pump = rm._PtyPump(fd)
        pump.start()
        try:
            pump.put(b"abc\r" * 5000)
            t0 = time.time()
            while (pump.left() or pump.read_n < 20000) and time.time() - t0 < 10:
                time.sleep(0.05)
            assert pump.left() == 0 and pump.read_n >= 20000, (pump.left(), pump.read_n)
        finally:
            os.kill(pid, 9)
            pump.stop()
            os.close(fd)
            os.waitpid(pid, 0)


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
    # r18u13: a key the text has doubled (`>>`), handled < 2 ms apart, with exactly as many presses logged as sent = an OS burst, not a dup
    q = lambda c: '"%s"' % c
    b = cases.keydown_audit(ln(q("o"), 990.0) + ln(q(">"), 1000.0) + ln(q(">"), 1000.97) + ln(q("k"), 1030.0), sent_text="o >> k")
    assert b["dups"] == [] and b["bursts"] == [('">"', 1000.97, 0.97)], b
    # ... but an EXTRA press of that key (three logged, two sent) is a dup
    e = cases.keydown_audit(ln(q(">"), 1000.0) + ln(q(">"), 1000.5) + ln(q(">"), 1030.0), sent_text="o >> k")
    assert e["dups"] == [('">"', 1000.5, 0.5)] and e["bursts"] == [], e
    # ... and so is a fast pair on a key the text never has doubled, and any pair when the sent text is not given
    f = cases.keydown_audit(ln(q("a"), 1000.0) + ln(q("a"), 1000.4), sent_text="a b a")
    assert f["dups"] == [('"a"', 1000.4, 0.4)], f
    assert cases.keydown_audit(ln(q(">"), 1000.0) + ln(q(">"), 1000.97))["dups"] == [('">"', 1000.97, 0.97)]


@check
def key_event_audit_counts_typed_only_and_enter_hops():
    kd = lambda k, t, seq: "input_order keydown key=%s ctrl=false alt=false shift=false cmd=false t=%s seq=%d\n" % (k, t, seq)
    ty = lambda seq: "input_order typed_characters text=len1 t=1 seq=%d\n" % seq
    hop = lambda d="false": "input_order hop=enter_begin deferred=%s buffer=x input_type=Shell t=1\n" % d
    # letter: keydown+typed share a seq (one key); symbol: typed_characters alone (one key); Enter: only the hop (one key); deferred re-run: none
    txt = kd("a", 1, 1) + ty(1) + ty(2) + kd("Shift", 2, 3) + kd("Enter", 3, 4) + hop() + hop("true")
    a = cases.key_event_audit(txt)
    assert (a["count"], a["keydown"], a["typed_only"], a["enter_only"]) == (3, 2, 1, 0), a       # a, symbol, Enter(keydown path; its hop is not a 2nd key)
    b = cases.key_event_audit(kd("a", 1, 1) + ty(1) + ty(2) + hop())
    assert (b["count"], b["enter_only"]) == (3, 1), b                                            # a, symbol, editor Enter via hop only
    assert cases.key_event_audit(kd("a", 1, 1) + kd("a", 1.1, 2))["count"] == 2                  # a doubled keydown still counts twice (the #1159 signature)


@check
def ui_item_audit_counts_items_over_budget():
    ln = lambda kind, ms: "input_order hop=ui_item kind=%s ms=%s iter=1 t=1\n" % (kind, ms)
    a = cases.ui_item_audit("junk\n" + ln("frame", "4.25") + ln("frame", "17.00") + ln("action", "5.5"))
    assert (a["over_4ms"], a["over_16ms"], a["max_ms"]) == (3, 1, 17.0), a
    assert a["by_kind"] == {"frame": [2, 17.0], "action": [1, 5.5]}, a
    assert cases.ui_item_audit("input_order hop=enter_begin deferred=false t=1\n") == dict(over_4ms=0, over_16ms=0, max_ms=0.0, by_kind={})
    h = cases.lock_hold_audit("DEBUG model lock held 3.25ms by TerminalView::render\nmodel lock held 812.5µs by TerminalView::render\n"
                              "WARN model lock held 1.5s by BlockListElement::layout (> 8ms; 0 similar holds suppressed)\n")
    assert h == {"TerminalView::render": [2, 3.25], "BlockListElement::layout": [1, 1500.0]}, h


@check
def ctrl_c_segments_split_the_app_share():
    kd = lambda t: 'input_order keydown key="c" ctrl=true alt=false shift=false cmd=false t=%s seq=1\n' % t
    hop = lambda h, t: "input_order hop=%s x=1 t=%s\n" % (h, t)
    one = lambda t0: (kd(t0) + hop("ctrl_c_keydown", t0 + 0.5) + hop("ctrl_c_to_event_loop", t0 + 1.0) + hop("ctrl_c_received", t0 + 1.25)
                      + hop("ctrl_c_received", t0 + 9) + hop("ctrl_c_pty_write", t0 + 2.0))
    a = cases.ctrl_c_segments("junk\n" + one(1000.0) + one(2000.0) + hop("ctrl_c_keydown", 3000.0))
    assert a["n"] == 2, a
    assert a["keydown->ctrl_c_keydown"]["p50"] == 0.5 and a["ctrl_c_received->ctrl_c_pty_write"]["p50"] == 0.75, a
    assert a["keydown_to_write"]["p50"] == 2.0, a
    assert cases.ctrl_c_segments("")["n"] == 0
    assert cases.ctrl_c_segments(one(1000.0) + one(2000.0), skip=1)["n"] == 1    # warm-up presses dropped


@check
def hanging_tcp_server_reports_connecting_then_closed():
    import socket
    srv = cases.HangingTcpServer()
    try:
        assert srv.poll() == (0, 0)
        c = socket.create_connection(("127.0.0.1", srv.port))
        time.sleep(0.1)
        assert srv.poll() == (1, 1)            # accepted, silent, open = an ssh that is still connecting
        c.close()
        time.sleep(0.1)
        assert srv.poll() == (1, 0)            # the client gave up (ConnectTimeout): connecting is over
    finally:
        srv.close()


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
def ssh_trial_facts_name_the_issue():
    """#1292/#1332/#1345/#1394/#1400/#1596/#1614: one ssh typeahead trial's app-log window -> the hop that went wrong, by issue"""
    line = "echo T1_abc >> ~/rl.txt"

    def kd_enter(t):
        return "x input_order keydown key=enter ctrl=false t=%.1f seq=1" % t

    def ty(c, t):
        return 'x input_order typed_characters text="%s" t=%.1f seq=2' % (c, t)

    def held(t):
        return "x input_order hop=kh_iq_hold seq=3 queue=Q7_ssh_typeahead bytes=1 t=%.1f" % t
    ssh = [ty(c, 1.0) for c in "ssh fake"] + [kd_enter(2.0)]
    typed = [ty(c, 3.0 + i) for i, c in enumerate(line)]
    # r18e4 Mac: the ssh Enter given up by the young hold, every key then raw: the ssh hold was never exercised
    mac = ssh + ["x input_order hop=prompt_hold_handed_back hold=25 kind=PromptSeen why=young_hold_cap chunks=1 t=2.5"] + typed + [kd_enter(40.0)]
    v = cases.ssh_trial_facts(mac, line, [])
    assert not v["exact"] and v["text"].startswith("#1614: the `ssh` Enter") and "young_hold_cap" in v["text"] and v["app_saw_line"], v["text"]
    # exact: 10 held, hand-over at precmd, the rest live
    ok = ssh + typed[:10] + [held(3.0 + i) for i in range(10)] + ["x input_order hop=kh_handover seq=9 source=precmd remote_prompt_seen=true held_bytes=10 t=20.0",
                                                               "x input_order hop=kh_finish_end seq=9 delivered=remote buffer_chars=10 t=20.1"] + typed[10:]
    v = cases.ssh_trial_facts(ok, line, ["echo w1 >> ~/.appci_where", line])
    assert v["exact"] and v["text"].startswith("exact") and v["handovers"] == [("precmd", 10)] and "delivered=remote" in v["finish"], v
    # #1394: the key right after the 10 held at the hand-over is lost
    v = cases.ssh_trial_facts(ok, line, [line[:10] + line[11:]])
    assert v["text"].startswith("#1345/#1394: ONE key lost: char 11") and "the 10 held at the hand-over" in v["text"], v["text"]
    # #1400: a run of keys lost
    v = cases.ssh_trial_facts(ok, line, [line[:5] + line[14:]])
    assert v["text"].startswith("#1400: keys lost: chars 6-14"), v["text"]
    # #1332: the delivered Enter refused (the WARN has no t= of its own)
    v = cases.ssh_trial_facts(ok + ["[WARN] Tried to execute command but can_execute_command was false: HistoryNotAppendable"], line, [])
    assert v["text"].startswith("#1332"), v["text"]
    # #1596: handed back and kept until the ssh block ends
    v = cases.ssh_trial_facts(ok + ["x input_order hop=ssh_handed_back chars=23 dest=stashed_until_block_end remote_prompt_seen=false give_up=true"], line, [])
    assert v["text"].startswith("#1596"), v["text"]
    # #1292: held, then the line never reached the remote
    v = cases.ssh_trial_facts(ok, line, [])
    assert v["text"].startswith("#1292: the line never reached"), v["text"]
    # the window reader: a line with no t= takes the t= of the line before it
    d = tempfile.mkdtemp()
    try:
        lp = os.path.join(d, "phosphor.log")
        with open(lp, "w") as fh:
            fh.write("a t=1000.0\n[WARN] no stamp\nb t=5000.0\nc t=9000.0\n")
        c = types.SimpleNamespace(log_path=lp)
        assert cases.Ctx.app_log_window(c, 0.5, 6.0) == ["a t=1000.0", "[WARN] no stamp", "b t=5000.0"]
        assert cases.Ctx.app_log_window(c, 4.0) == ["b t=5000.0", "c t=9000.0"]
    finally:
        shutil.rmtree(d, ignore_errors=True)


@check
def bashrc_moves_a_profile_folder_start_to_the_work_dir():
    """r18u13: Git Bash on Windows starts in %USERPROFILE% (not the scratch $HOME); the rc must move that start to $APPCI_W like a $HOME start,
    and must leave any other start (a restored tab's cwd) alone. Runs the rendered rc in bash with a stand-in cygpath."""
    bash = shutil.which("bash")
    if not bash or os.name == "nt":
        return
    d = tempfile.mkdtemp()
    try:
        home, up, w, other, bin_ = (os.path.join(d, x) for x in ("home", "Users_me", "w", "restored", "bin"))
        for x in (home, up, w, other, bin_, os.path.join(d, "ctl")):
            os.makedirs(x)
        rc = os.path.join(d, "rc")
        open(rc, "w").write(profile.BASHRC % {"ctl": os.path.join(d, "ctl")})
        cyg = os.path.join(bin_, "cygpath")
        open(cyg, "w").write("#!/bin/sh\n[ \"$1\" = -u ] && printf '%s\\n' \"$2\"\n")
        os.chmod(cyg, 0o755)

        def start_in(cwd, userprofile):
            env = dict(HOME=home, APPCI_W=w, PATH=bin_ + os.pathsep + os.environ.get("PATH", ""))
            if userprofile:
                env["USERPROFILE"] = userprofile
            return subprocess.run([bash, "--norc", "--noprofile", "-c", '. "$1" >/dev/null 2>&1; pwd -P', "x", rc], cwd=cwd, env=env,
                                  capture_output=True, text=True, timeout=30).stdout.strip()
        real = lambda x: os.path.realpath(x)          # noqa: E731
        assert start_in(up, up) == real(w), "a start in USERPROFILE is not moved to APPCI_W"
        assert start_in(home, None) == real(w), "a start in HOME is not moved to APPCI_W"
        assert start_in(other, up) == real(other), "a restored tab's cwd was moved"
        assert start_in(up, None) == real(up), "without USERPROFILE a start outside HOME was moved"
    finally:
        shutil.rmtree(d)


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
    # Enter handled by the input editor logs only hop=enter_begin (deferred=true re-runs are not new keys): it counts as the app's Enter
    be2 = backend.X11Backend()
    be2.record_injection(["xdotool", "type", "--", "ab", "key", "Return"], 200.0, 200.2)
    be2.record_injection(["xdotool", "type", "--", "c", "key", "Return"], 201.0, 201.2)
    log2 = os.path.join(d, "p2.log")
    open(log2, "w").write(kd("a", 200.01, 1) + kd("b", 200.02, 2) + kd("c", 201.01, 3)
                          + "input_order hop=enter_begin deferred=false buffer=x input_type=Shell t=%.3f\n" % 200150
                          + "input_order hop=enter_begin deferred=true buffer=x input_type=Shell t=%.3f\n" % 200160
                          + "input_order hop=enter_begin deferred=false buffer=x input_type=Shell t=%.3f\n" % 201150)
    txt2, j2 = injacct.account(be2.calls, log2, "x")
    assert (j2["sent"], j2["seen"], j2["lost"], j2["enters_seen"], j2["enters_sent"]) == (5, 5, 0, 2, 2), (j2, txt2)
    # paste-only row: n/a, not "saw 0"
    be3 = backend.WaylandBackend()
    be3.record_injection(["wtype", "-M", "ctrl", "-M", "shift", "-k", "v", "-m", "shift", "-m", "ctrl"], 300.0, 300.1)
    assert backend.count_pastes(["xdotool", "key", "ctrl+shift+v", "Return"]) == 1 and backend.count_pastes(["wtype", "-k", "v"]) == 0
    txt3, j3 = injacct.account(be3.calls, log2, "x")
    assert j3["na"] and "n/a" in txt3, (j3, txt3)
    assert injacct.account(be.calls, None, "x")[1]["seen"] is None                 # no trace: reported, not compared
    assert injacct.account([], log, "x") == (None, None)


@check
def lifecycle_full_log_line_and_incremental_report():
    """r18d8: the first failing message / NO-REFERENCE reason is logged in full; results.json exists after every emit"""
    import run as runmod
    long = "x" * 500
    line = runmod.row_log_line(dict(mode="agent", name="c", status="fail", detail=[long, "n" * 400]))
    assert long in line and len(line) < len(long) + 260, len(line)
    line = runmod.row_log_line(dict(mode="terminal", name="latency_enter", status="noref", detail=["interleaved 24 blocks", "NO-REFERENCE: " + long]))
    assert "NO-REFERENCE: " + long in line, line[:100]
    d = tempfile.mkdtemp()
    try:
        open(os.path.join(d, "results.jsonl"), "w").write(json.dumps(dict(name="a", mode="terminal", status="pass", seconds=1, detail=[])) + "\n")
        a = types.SimpleNamespace(platform="linux-wayland", backend="wayland", app_display="wayland", seed=1)
        runmod.write_partial_report(d, a)
        doc = json.load(open(os.path.join(d, "results.json")))
        assert doc["summary"]["pass"] == 1 and doc["meta"].get("partial") == "1", doc
        assert os.path.exists(os.path.join(d, "summary.md")) and os.path.exists(os.path.join(d, "junit.xml"))
    finally:
        shutil.rmtree(d, ignore_errors=True)


@check
def deadline_stops_new_cases_and_report_line_untruncated():
    """r18e0: the run's own time budget (no new case when too little is left, explicit skip text) and the full first-failure line in the report table"""
    import deadline
    env = {"CI_JOB_TIMEOUT": "5400", "CI_JOB_STARTED_AT": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(1_000_000))}
    c = deadline.Clock(env, now=1_000_000 + 60)
    assert c.budget == 5400 and c.start == 1_000_000 and c.start_src == "CI_JOB_STARTED_AT", (c.budget, c.start)
    assert c.allows(1500, now=1_000_000 + 60)[0]                                  # 89 min left, needs 25 + 5
    ok, why = c.allows(1500, now=1_000_000 + 5400 - 29 * 60)                       # 29 min left < 30 needed
    assert not ok and "not run: job time budget (29 min left" in why, why
    assert c.allows(0, now=1_000_000 + 5400 - 29 * 60)[0]                          # a 10 min margin case still fits
    assert not c.allows(0, now=1_000_000 + 5400 - 14 * 60)[0] and not c.allows(0, now=1_000_000 + 9999)[0]
    assert deadline.Clock({"APPCI_DEADLINE_S": "100", "CI_JOB_TIMEOUT": "5400"}).budget == 100            # the explicit variable wins
    n = deadline.Clock({}, now=5)
    assert n.budget is None and n.allows(99999)[0] and n.left() is None            # no budget: never stops a local run
    f = deadline.Clock({"CI_JOB_TIMEOUT": "600", "CI_JOB_STARTED_AT": "2001-01-01T00:00:00Z"}, now=1_000_000)
    assert f.start_src == "process start"                                          # an implausible start (older than the budget) is ignored
    d = tempfile.mkdtemp()
    try:
        long = "first failure " + "z" * 700
        open(os.path.join(d, "results.jsonl"), "w").write(json.dumps(dict(name="a", mode="agent", status="fail", seconds=1, detail=[long, "y" * 400])) + "\n")
        out = subprocess.run([sys.executable, os.path.join(HERE, "report.py"), d, "platform=linux"], capture_output=True, text=True).stdout
        assert long in out and "y" * 161 not in out, "the first failure must be in the summary table in full, the rest capped"
        assert long in open(os.path.join(d, "summary.md")).read()
    finally:
        shutil.rmtree(d, ignore_errors=True)
    import run as runmod
    assert runmod.enter_summary([]) is None and "AI=1 Shell=2" in runmod.enter_summary(["Shell", "AI", "Shell"])
    assert runmod.host_load().startswith("loadavg1=") and "cpus=" in runmod.host_load()


@check
def mac_close_key_empty_never_crashes_and_latency_guards():
    """r18e0 follow-up: an empty close key SKIPs soak (never be.key('')); the edit cases skip with precise reasons; median_se / per-side n are
    recorded; an implausibly FASTER reference is NOREF (no-compare), never a PASS; dbg() shows the hex of a ZWJ."""
    import cases_robust as cr, cases_caseintl as ci, lat
    assert "cmd-shift-W" in cr.NO_CLOSE_WHY and "D-Bus" in cr.SKIP_EDITOR_WHY and "Cmd-Q" in cr.SKIP_QUIT_WHY
    ph = [30.0 + (i % 5) for i in range(30)]
    ref = [60.0 + (i % 5) for i in range(30)]
    rec = lat.evaluate("x/y", ph, ref, 5.0, "overhead", "macos", ref_name="T", expected=30, mode="interleaved", implausible_neg_ms=5.0)
    assert rec["budget_verdict"] == "no-compare" and rec.get("implausible"), rec
    assert rec["per_side"]["phosphor"]["n"] == 30 and rec["per_side"]["reference"]["se_median_ms"] is not None, rec["per_side"]
    rec2 = lat.evaluate("x/y", ph, ph, 5.0, "overhead", "macos", ref_name="T", expected=30, mode="interleaved", implausible_neg_ms=5.0)
    assert not rec2.get("implausible"), rec2
    assert "e29d8d" not in ci.dbg("a") and "e2808d" in ci.dbg("a\u200db"), ci.dbg("a\u200db")
    assert ci.WIDE[3][1].count("\u200d") == 2        # the family sequence really contains two U+200D scalars
    res = cases.Result("t", "terminal")
    if res is not None:
        res.unverifiable("trace missing")
        assert res.noref and "UNVERIFIABLE" in res.noref[0] and not res.fails


@check
def r18e4_flood_zwj_routing_hang_fixes():
    import cases_robust as cr
    # 1: the endless producer runs until the stop file exists and never has a fixed end
    cmd = cr.endless_flood("seq", "stopf", 1000, 2)
    assert "while [ ! -e stopf ]" in cmd and "seq 1 500000" in cmd and "-lt 2" in cmd, cmd
    assert "yes | head" in cr.endless_flood("yes", "s")
    # 2: the zsh rc records raw history (fc -l prints a ZWJ as the text \\u200d), and the case text holds the real code point
    assert "history[$((HISTCMD-1))]" in profile.ZSHRC and "fc -ln -1" in profile.ZSHRC
    import cases_caseintl as ci2
    assert "\\u200d" not in ci2.WIDE[3][1] and "\u200d" in ci2.WIDE[3][1] and "\\" not in ci2.WIDE[3][1]
    # 4: `what is 5 > 3` is shell-syntax/evidence, judged as ambiguous (shell by rule), not prose-to-agent
    assert "what is 5 > 3" not in ca.PROSE_LINES and "what is 5 > 3" in ca.AMBIGUOUS_LINES
    assert ca.ambiguous_want("what is 5 > 3", "AgentConversation", "shell-structure-unknown-head (#1560)") == "shell"
    assert ca.ambiguous_want("what is 5 > 3", "AgentConversation", "evidence-in-conversation (#1183)") == "shell"


@check
def r18e9_windows_reference_has_own_ps_history():
    """r18e9: the conhost reference PowerShell must not share the pane's ConsoleHost_history.txt (its `exit` took fast_typing's first slot)"""
    src = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "cases.py"), encoding="utf-8").read()
    i = src.index("def ref_terminal_windows")
    body = src[i:src.index("def pane_shell", i)]
    assert "HistorySavePath" in body and "-NoExit" in body, "the PowerShell reference needs its own history file"


@check
def r18e9_pane_closed_by_own_exit_is_not_an_app_death():
    """r18e9: a pane closed by the case's own `exit` is named so; a panic keeps the crash wording"""
    import run as runmod
    class C:
        def __init__(s, log, ex): s.log, s.ex = log, ex
        def log_since(s, off): return s.log
        def last_typed_exit_since(s, t0): return s.ex
    t0 = time.time() - 1
    m = runmod.classify_death("APP DIED: pid 1 is gone", C("13:51:34 enter_begin buffer=\"exit\"\n13:51:34 WindowId(0) will close\n", True), t0, 0)
    assert "OWN `exit`" in m and "NOT an app crash" in m, m
    m = runmod.classify_death("APP DIED: pid 1 is gone", C("thread 'main' panicked at x.rs:1\nWindowId(0) will close", True), t0, 0)
    assert "OWN" not in m and "crash evidence" in m, m


@check
def r18e9_injector_accounting_spans_relaunches():
    """r18e9: ui_startup_time relaunches 5 times and the app log is overwritten each time; accounting over [earlier copies..., live log] counts every launch, and the
    per-launch seq counters do not collide"""
    import injacct
    d = tempfile.mkdtemp()
    try:
        def log(name, n):
            p = os.path.join(d, name)
            with open(p, "w") as f:
                for i in range(n):
                    f.write("2026 input_order typed_characters chars=1 t=%d seq=%d\n" % (1000 + i, i + 1))
            return p
        a, b = log("l1.log", 5), log("l2.log", 3)
        one = injacct.app_keys(b, 0, 10)
        both = injacct.app_keys([a, b], 0, 10)
        assert len(one) == 3 and len(both) == 8, (len(one), len(both))
    finally:
        shutil.rmtree(d, ignore_errors=True)


@check
def r18e9_controlmaster_cleanup_parses_and_never_raises():
    """r18e9: stop_ssh_controlmasters on a run dir nothing names is a silent no-op; the ControlPath regex reads the wrapper's option"""
    import run as runmod
    runmod.stop_ssh_controlmasters("t", "/nonexistent/appci-r18e9-" + str(os.getpid()))
    assert runmod.SSH_CM_RX.search("/usr/bin/ssh -F x -o ControlMaster=yes -o ControlPath=/tmp/a/cm-1 -N -f h").group(1) == "/tmp/a/cm-1"


@check
def r18e9_enter_taint_splits_lost_from_uncovered():
    """r18e9: a tapped Return with no app_send_event is LOST when neighbours are logged within 3 s, UNCOVERED when the log has nothing near it; a clean case is untainted"""
    import taint
    log = "x hop=mac_enter stage=app_send_event keycode=36 ts=10.000000 t=1\nx hop=mac_enter stage=app_send_event keycode=36 ts=11.000000 t=2\n"
    taps = [(10.0, 5, 1.0), (12.0, 5, 3.0), (500.0, 5, 9.0), (50.0, 99, 5.0)]
    c = taint.classify(taps, {5}, log)
    assert [x[0] for x in c["lost"]] == [12.0] and [x[0] for x in c["uncovered"]] == [500.0] and c["elsewhere"] == 1, c
    d = tempfile.mkdtemp()
    try:
        os.makedirs(os.path.join(d, "run"))
        open(os.path.join(d, "ktap.log"), "w").write("E down keycode=36 ts_ns=10000000000 target_pid=5 source_pid=1 wall=100.0\n")
        note, m = taint.case_taint(os.path.join(d, "run"), "c", "terminal", 99.0, 101.0, {5}, log)
        assert note is None and m["lost"] == 0, (note, m)
        note, m = taint.case_taint(os.path.join(d, "run"), "c", "terminal", 99.0, 101.0, {5}, "")
        assert note and "TAINTED" in note and m["uncovered"] == 1 and os.path.exists(os.path.join(d, "run", "logs", "enter_taint.jsonl"))
    finally:
        shutil.rmtree(d, ignore_errors=True)


@check
def r18e9_pty_write_cr_count():
    """r18e9: per-case count of pty writes that carried a CR"""
    import run as runmod
    L = 'x input_order hop=pty_write_bytes class=UserCommand wrote=2 of_job=2 bytes="\\r" since_precmd_ms=1\nx input_order hop=pty_write_bytes class=UserKey wrote=2 of_job=2 bytes="\\x1b1" s=2\nx input_order hop=enter_begin deferred=false t=1\n'
    assert runmod.pty_write_cr(L) == dict(writes=2, with_cr=1, enters=1), runmod.pty_write_cr(L)


@check
def editor_memory_breakdown_close_and_cycles():
    """#1607/#1407: the editor_memory case parses the app's breakdown lines, computes cycle growth, never sends an empty close key, and the
    macOS footprint reader degrades to None off macOS (no proc_pid_rusage)"""
    tmp = tempfile.mkdtemp()
    try:
        log = os.path.join(tmp, "app.log")
        with open(log, "w") as f:
            f.write("noise\n2026 INFO editor_memory editor_memory open text_bytes=7340032 lines=92000 buffer_items=184001 parked_chunks=360 frame_bytes=1234\n"
                    "2026 INFO editor_memory editor_memory buffer_dropped registered=0 trim=true\n")
        c = _ctx(tmp, log_path=log)
        br = cases.editor_mem_log(c, 0)
        assert br["open"][0]["text_bytes"] == 7340032 and br["open"][0]["parked_chunks"] == 360, br
        assert br["dropped"][0]["registered"] == 0 and br["dropped"][0]["trim"] == "true", br
        assert cases.editor_mem_log(c, os.path.getsize(log)) == {"open": [], "dropped": [], "trim": []}
        assert cases.editor_cycle_growth([100.0, 104.0, 131.0]) == 31.0 and cases.editor_cycle_growth([5.0]) == 0.0
        if sys.platform != "darwin":
            assert cases.mac_footprint_kib(os.getpid()) is None
        # owner 2026-11-03: macOS judges BOTH figures; either one over the budget fails
        r2 = cases.Result("t", "terminal")
        assert not cases.mem_judge(r2, {"rss": 150.0, "phys_footprint": 90.0}, {"rss": 0.0, "phys_footprint": 0.0}, 100, "cost %.0f MB of %s")
        assert r2.fails == ["cost 150 MB of rss"], r2.fails
        r3 = cases.Result("t", "terminal")
        assert not cases.mem_judge(r3, {"rss": 50.0, "phys_footprint": 130.0}, {"rss": 0.0, "phys_footprint": 0.0}, 100, "cost %.0f MB of %s")
        assert r3.fails == ["cost 130 MB of phys_footprint"], r3.fails
        r4 = cases.Result("t", "terminal")
        assert cases.mem_judge(r4, {"rss": 50.0}, {"rss": 0.0}, 100, "x %.0f %s") and not r4.fails
        cases.mem_metrics(r4, "txt_rss_open", {"rss": 120.0, "phys_footprint": 80.0}, {"rss": 20.0, "phys_footprint": 10.0})
        assert r4.metrics["txt_rss_open_mb"] == 120.0 and r4.metrics["txt_rss_open_delta_mb"] == 100.0, r4.metrics
        assert r4.metrics["txt_rss_open_phys_footprint_mb"] == 80.0 and r4.metrics["txt_rss_open_phys_footprint_delta_mb"] == 70.0, r4.metrics
        # hop=ui_item: the open window starts after the load's own (largest) frame; the close window is plain
        t = 1000.0
        logtxt = "".join("x input_order hop=ui_item kind=%s ms=%.2f render_ms=0.50 layout_paint_ms=0.50 t=%.3f\n" % (w, ms, (t + dt) * 1000) for w, ms, dt in (
            ("frame", 2.0, -1.0), ("frame", 250.0, 0.5), ("event", 3.0, 1.0), ("frame", 6.5, 2.0), ("action", 9.0, 8.0)))
        items = cases.ui_items(logtxt)
        assert len(items) == 5 and items[1][1] == 250.0 and items[1][2] == "frame", items
        assert cases.ui_window_max(items, t, after_largest=True) == (6.5, 1, 250.0), cases.ui_window_max(items, t, after_largest=True)
        assert cases.ui_window_max(items, t) == (250.0, 2, None), cases.ui_window_max(items, t)
        assert cases.ui_window_max([], t) == (0.0, 0, None)
        # a bigger item after t_close must not move the open window (r18 re-review)
        later = items + [(t + 6.0, 900.0, "frame")]
        assert cases.ui_window_max(later, t, after_largest=True, t_end=t + 5.5) == (6.5, 1, 250.0), cases.ui_window_max(later, t, after_largest=True, t_end=t + 5.5)
        # heap trims: the UI items that ended during [end - ms, end + 0.25 s]
        trim_log = "z input_order hop=heap_trim why=open ms=500.00 t=%.3f\nz input_order hop=heap_trim why=close ms=10.00 t=%.3f\n" % ((t + 2.1) * 1000, (t + 8.0) * 1000)
        trims = cases.heap_trims(trim_log)
        assert trims == [(t + 2.1, 500.0, "open"), (t + 8.0, 10.0, "close")], trims
        assert cases.ui_trim_max(items, trims, "open") == 6.5            # the 6.5 ms frame at t+2.0 ended inside the open trim
        assert cases.ui_trim_max(items, trims, "close") == 9.0           # the 9 ms action at t+8.0 ended at the close trim's end
        assert cases.ui_trim_max(items, [], "open") == 0.0
        assert cases.ui_item_audit(logtxt)["max_ms"] == 250.0 and cases.ui_items("input_order hop=ui_item what=frame ms=9 t=1") == []
        sent = []
        be = types.SimpleNamespace(key=lambda k: sent.append(k), type=lambda *a, **k: sent.append(("type", a)))
        r = cases.Result("t", "terminal")
        old = {k: os.environ.get(k) for k in ("APPCI_KEY_CLOSE", "APPCI_KEY_CMD_PALETTE")}
        try:
            os.environ["APPCI_KEY_CLOSE"], os.environ["APPCI_KEY_CMD_PALETTE"] = "", ""
            assert cases.editor_close(types.SimpleNamespace(be=be), r) is None and sent == [], sent
            os.environ["APPCI_KEY_CMD_PALETTE"] = "cmd+p"
            orig_sleep, cases.time.sleep = cases.time.sleep, lambda s: None
            try:
                how = cases.editor_close(types.SimpleNamespace(be=be), r)
            finally:
                cases.time.sleep = orig_sleep
            assert how and how.startswith("palette") and sent[0] == "cmd+p" and sent[-1] == "Return", (how, sent)
            assert "" not in sent
        finally:
            for k, v in old.items():
                if v is None:
                    os.environ.pop(k, None)
                else:
                    os.environ[k] = v
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


@check
def editor_memory_close_trim_and_accepted_open_cost():
    """#1607: the heap_trim / buffer_dropped log lines parse (signed freed_estimate); a reopen into reused heap counts as opened when the app
    logged its open line; the open-cost budget is KNOWN (numbers printed, not a failure) only while the ACCEPTED_FOR_RELEASE release has not been
    passed, fails past it and fails closed on an unreadable version; and the acceptance has not outlived v0.1.17 in this tree."""
    tmp = tempfile.mkdtemp()
    try:
        log = os.path.join(tmp, "app.log")
        with open(log, "w") as f:
            f.write("2026 DEBUG editor_memory editor_memory buffer_dropped file_id=FileId(2) registered=0 trim=true\n"
                    "2026 DEBUG editor_memory editor_memory heap_trim why=close ms=3.25 freed_estimate=-4096\n"
                    "2026 DEBUG editor_memory editor_memory heap_trim why=close ms=1.00 freed_estimate=104857600\n"
                    "z input_order hop=heap_trim why=close ms=1.00 freed_estimate=104857600 t=1000.000\n")
        c = _ctx(tmp, log_path=log)
        br = cases.editor_mem_log(c, 0)
        assert br["dropped"][0]["trim"] == "true" and br["dropped"][0]["file_id"] == "FileId(2)", br
        assert [t["freed_estimate"] for t in br["trim"]] == [-4096, 104857600], br["trim"]
        assert br["trim"][0]["why"] == "close" and br["trim"][0]["ms"] == "3.25", br["trim"]
        assert cases.editor_wait_close_trim(c, 0, timeout=0)[-1]["freed_estimate"] == 104857600
        assert cases.heap_trims(open(log).read()) == [(1.0, 1.0, "close")]     # the hop regex still reads the line with freed_estimate
        assert not cases.editor_opened(-5.5, []) and cases.editor_opened(-5.5, [{"text_bytes": 1}]) and cases.editor_opened(9.0, [])
        # versions and the acceptance window
        assert cases.version_tuple("v0.1.17") == (0, 1, 17) and cases.version_tuple("0.1.9") < cases.version_tuple("0.1.17")
        assert cases.version_tuple("") is None and cases.version_tuple("x") is None
        acc = cases.ACCEPTED_FOR_RELEASE["editor_open_cost"]
        assert acc["issue"] == 1607 and acc["release"] == "0.1.17", acc
        assert cases.accepted_for_release("editor_open_cost", "0.1.16") and cases.accepted_for_release("editor_open_cost", "0.1.17")
        assert cases.accepted_for_release("editor_open_cost", "0.1.18") is None
        assert cases.accepted_for_release("editor_open_cost", "garbage") is None       # fail closed
        assert cases.accepted_for_release("no_such_key", "0.1.16") is None
        v = cases.app_version()
        assert v and cases.version_tuple(v), "app/Cargo.toml [package] version unreadable: %r" % v
        assert cases.version_tuple(v) <= cases.version_tuple(acc["release"]), (
            "ACCEPTED_FOR_RELEASE editor_open_cost (#1607) was for v%s; app/Cargo.toml is now %s: re-decide it with the owner (remove the entry or "
            "renew it with a new decision), do not just bump the release" % (acc["release"], v))
        # within the window: over budget is KNOWN with the numbers and the marker, never a failure
        orig = cases.app_version
        try:
            cases.app_version = lambda repo=None: "0.1.17"
            r = cases.Result("t", "terminal")
            assert not cases.mem_judge_known(r, {"rss": 201.0, "phys_footprint": 50.0}, {"rss": 0.0, "phys_footprint": 0.0}, 100, "cost %.0f MB of %s",
                                             "editor_open_cost")
            assert not r.fails and len(r.notes) == 1, (r.fails, r.notes)
            assert r.notes[0].startswith("KNOWN: cost 201 MB of rss") and "ACCEPTED_FOR_RELEASE #1607 v0.1.17" in r.notes[0], r.notes
            cases.app_version = lambda repo=None: "0.1.18"
            r = cases.Result("t", "terminal")
            assert not cases.mem_judge_known(r, {"rss": 201.0}, {"rss": 0.0}, 100, "cost %.0f MB of %s", "editor_open_cost")
            assert r.fails == ["cost 201 MB of rss"] and not r.notes, (r.fails, r.notes)
        finally:
            cases.app_version = orig
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


@check
def latency_known_v0117_enter_and_flood():
    """v0.1.17 (#1606, #1653): an over-budget latency_enter overhead (macOS / Linux) or Ctrl-C-under-flood overhead is KNOWN at 0.1.17 and FAIL at 0.1.18;
    other verdicts (regression vs baseline) and other platforms still fail."""
    rec = {"budget_verdict": "fail", "budget_kind": "overhead", "verdict": "fail", "overhead_ms": 9.5, "overhead_se_ms": 0.4, "reference_name": "xterm",
           "budget_ms": 2, "phosphor": {"p50": 12.0}}
    flood = dict(rec, budget_ms=45)
    orig_v, orig_p = cases.app_version, cases.plat
    try:
        for plat_, key, rc, issue, ok in (("macos", "latency_enter_overhead", rec, 1606, True), ("linux-wayland", "latency_enter_overhead", rec, 1606, True),
                                          ("linux-x11", "latency_enter_overhead", rec, 1606, True), ("windows", "latency_enter_overhead", rec, 1606, False),
                                          ("macos", "ctrl_c_flood", flood, 1653, True), ("linux-x11", "ctrl_c_flood", flood, 1653, True)):
            cases.plat = lambda p=plat_: p
            cases.app_version = lambda repo=None: "0.1.17"
            r = cases.Result("t", "terminal")
            cases.lat_checks(r, rc, "L", known_key=key)
            if ok:
                assert not r.fails and len(r.notes) == 1 and r.notes[0].startswith("KNOWN: L: Phosphor overhead 9.50") and ("ACCEPTED_FOR_RELEASE #%d v0.1.17" % issue) in r.notes[0], (plat_, key, r.fails, r.notes)
            else:
                assert len(r.fails) == 1 and not r.notes, (plat_, key, r.fails, r.notes)
            cases.app_version = lambda repo=None: "0.1.18"
            r = cases.Result("t", "terminal")
            cases.lat_checks(r, rc, "L", known_key=key)
            assert len(r.fails) == 1 and not r.notes, (plat_, key, r.fails, r.notes)
        # a case that names no key never gets the exception
        cases.plat, cases.app_version = (lambda: "linux-x11"), (lambda repo=None: "0.1.17")
        r = cases.Result("t", "terminal")
        cases.lat_checks(r, rec, "L")
        assert len(r.fails) == 1, r.fails
        for k in ("latency_enter_overhead", "ctrl_c_flood"):
            assert cases.accepted_for_release(k, "0.1.17") and cases.accepted_for_release(k, "0.1.18") is None
        assert cases.CTRLC_FLOOD_RELEASE_MS == "45"
    finally:
        cases.app_version, cases.plat = orig_v, orig_p


@check
def pty_bytes_decide_where_a_line_changed():
    """#1490/#1393: line_evidence says app-side vs. after-the-pty-write from the app's own pty_write_bytes, never from typed_characters alone"""
    assert cases.unescape_rust_ascii('a\\x1b[200~b\\r\\n\\"q\\\\\\x10') == 'a\x1b[200~b\r\n"q\\\x10'
    want, have = "echo t134_xy", "34echo t1_xy"
    # keys to the shell's line in order, an `ESC i` request between two of them: the typed line, so the change came after the pty write
    assert cases.pty_line_verdict("echo t1\x1bi34_xy\r", want, have) == "want"
    # the editor command carries the moved keys: the app wrote it that way
    assert cases.pty_line_verdict("echo t1\x1bi\x10\x1b[200~34echo t1_xy\x1b[201~\r", want, have) == "have"
    # split between the shell's line and an editor command: neither contiguously
    assert cases.pty_line_verdict("echo\x10\x1b[200~ t134_xy\x1b[201~\r", want, have) == "neither"
    assert cases.pty_line_verdict(None, want, have) is None
    # a corrupted line that is only a piece of the typed one (a lost tail) proves nothing about the app's writes
    assert cases.pty_line_verdict("echo t134_xy\r", want, "echo t1") == "want"
    # review R5: a line written to the shell's line and then killed (^P, ^U, ^K, ESC 2) before its line end did not reach the shell as typed;
    # a kill AFTER its line end belongs to the next line
    for kill in ("\x10", "\x15", "\x0b", "\x1b2"):
        assert cases.pty_line_verdict("echo t134_xy" + kill + "x\r", want, have) == "neither", repr(kill)
        assert cases.pty_line_verdict("echo t134_xy\r" + kill + "next\r", want, have) == "want", repr(kill)
    # review R5: escape_ascii writes non-ASCII as \xNN bytes; they are bytes, decoded once (a character split over two writes too)
    assert cases.unescape_rust_ascii_bytes("\\xc3\\xa9") == b"\xc3\xa9" and cases.unescape_rust_ascii("caf\\xc3\\xa9") == "caf\u00e9"
    d = tempfile.mkdtemp()
    try:
        log = os.path.join(d, "phosphor.log")
        open(log, "w").write(
            'x [DEBUG] input_order hop=pty_write_bytes class=UserKey wrote=1 of_job=1 bytes="e" since_precmd_ms=1 t=1000000.0\n'
            'x [DEBUG] input_order hop=pty_write_bytes class=UserKey wrote=1 of_job=1 bytes="old" since_precmd_ms=1 t=10.0\n'
            'x [DEBUG] input_order hop=pty_write_bytes class=UserCommand wrote=9 of_job=9 bytes="\\x10\\x1b[200~cho\\x1b[201~\\r" s=1 t=1000001.0\n')
        c = _ctx(d, log)
        assert cases.Ctx.app_pty_text(c, 999.0) == "e\x10\x1b[200~cho\x1b[201~\r", repr(cases.Ctx.app_pty_text(c, 999.0))
        open(log, "w").write(
            'x input_order hop=pty_write_bytes class=UserKey wrote=1 of_job=1 bytes="echo caf\\xc3" t=1000000.0\n'
            'x input_order hop=pty_write_bytes class=UserKey wrote=1 of_job=1 bytes="\\xa9 \\xe2\\x82\\xac\\r" t=1000001.0\n')
        assert cases.Ctx.app_pty_text(c, 999.0) == "echo caf\u00e9 \u20ac\r", repr(cases.Ctx.app_pty_text(c, 999.0))
        assert cases.pty_line_verdict(cases.Ctx.app_pty_text(c, 999.0), "echo caf\u00e9 \u20ac", "echo caf \u20ac") == "want"
        open(log, "w").write('x input_order hop=pty_write_bytes class=UserKey wrote=1 of_job=1 bytes=<1 bytes> t=1.0\n')
        assert cases.Ctx.app_pty_text(c, 0.0) is None, "a log without opted-in text is no trace"
        c.app_saw_line = lambda w, t: True
        c.app_pty_wrote_line = lambda w, h, t: "have"
        ev = cases.line_evidence(c, want, have, 0.0)
        assert "after the app's pty write" not in ev and "Phosphor wrote it that way" in ev, ev
    finally:
        shutil.rmtree(d)


@check
def agent_code_recycle_log_parse():
    """#1267: the agent_code_recycle case reads the app's recycle / rebuild lines and builds the exact snippet lines it expects back"""
    log = ("x DEBUG editor_memory editor_memory agent_code_recycled block=EntityId(42) dropped=12\n"
           "y DEBUG editor_memory editor_memory agent_code_rebuilt block=EntityId(42) rebuilt=12\n")
    assert ca.recycle_events(log) == [("recycled", "EntityId(42)", 12), ("rebuilt", "EntityId(42)", 12)], ca.recycle_events(log)
    assert ca.snippet_lines("AB12", 3) == ["echo RCYAB12_01", "echo RCYAB12_02", "echo RCYAB12_03"]
    assert "agent_code_recycle" in winmap.BOTH


@check
def coverage_b_pure_helpers():
    """g10 coverage cases: the legacy/truncated settings builders, the startup-log and updater-line filters, the picker verdict"""
    import cases_coverage_b as cb
    orig = '[appearance.text]\nfont_size = 14.0\n'
    text, marks = cb.legacy_settings(orig, "t1")
    assert all(m in text for m in marks) and orig in text and len(marks) == 4, text
    bad = cb.corrupt_settings(orig)
    try:
        import tomllib
        try:
            tomllib.loads(bad)
            raise AssertionError("the truncated settings file still parses")
        except tomllib.TOMLDecodeError:
            pass
    except ImportError:
        assert "unterminated" in bad
    log = "INFO started\nthread 'main' panicked at app/x.rs:1\n2026 ERROR migration 0042 failed\n2026 INFO migration ok\n2026 ERROR network blip\n"
    p = cb.startup_log_problems(log)
    assert len(p) == 2 and "panicked" in p[0] and "migration 0042" in p[1], p
    assert cb.startup_log_problems("INFO all fine\n") == []
    assert cb.autoupdate_lines("INFO a\nWARN Checking for updates now\nDEBUG GET https://x/releases/latest\n") != [] and cb.autoupdate_lines("INFO nothing\n") == []
    # r18u8: the startup line's `autoupdate_configured: false` is the updater OFF, not the updater doing something; `: true` still counts
    assert cb.autoupdate_lines("INFO Starting warp with channel state ChannelState { channel: Oss, autoupdate_configured: false, mcp_static_configured: false }\n") == []
    assert cb.autoupdate_lines("INFO ChannelState { autoupdate_configured: true }\n") != []
    assert cb.picker_verdict(0.05, 0.001)[0] is True
    # #1633: an Enter the @ picker took is not a submit; one without enter_menu_accept is.
    took = 'hop=enter_begin deferred=false buffer="@cbm" input_type=AI t=1\nhop=kh_focus_input\nhop=enter_menu_accept menu=ai_context cli_agent=false t=2\n'
    sent = 'hop=enter_begin deferred=false buffer="hello there" input_type=AI t=3\nhop=kh_iq_queue gate_open kind=enter_verdict\n'
    shell = 'hop=enter_begin deferred=false buffer="ls" input_type=Shell t=4\n'
    assert cb.ai_submits(took) == [], cb.ai_submits(took)
    assert cb.ai_submits(took + sent + shell) == [("hello there", "AI")], cb.ai_submits(took + sent + shell)
    assert cb.ai_submits(sent + took) == [("hello there", "AI")]
    assert cb.picker_verdict(0.0005, 0.0)[0] is False and cb.picker_verdict(0.05, 0.04)[0] is False and cb.picker_verdict(None, 0.0)[0] is None
    reg = {c["name"]: c for c in cases.CASES}
    for n in ("cb_agent_pickers", "cb_agent_file_mention", "cb_settings_ui_open", "cb_settings_migration", "cb_state_relaunch", "cb_no_autoupdate", "cb_ui_action_latency", "cb_editor_mac"):
        assert n in reg, n
    assert reg["cb_agent_pickers"]["modes"] == ("agent",) and "cb_agent_pickers" not in cases.SMOKE


@check
def g11_relaunch_cases_run_on_every_platform():
    """#1540/#1541: crash_kill9_restore / crash_corrupt_db are classified for Windows (no longer NOT_WINDOWS), session_restore is Git Bash only there; the
    helpers that make the cases platform-neutral behave"""
    import cases_robust as cr
    import cases_coverage_a as cov_a
    for n in ("crash_kill9_restore", "crash_corrupt_db"):
        assert winmap.skip_reason(n, "powershell") is None and winmap.skip_reason(n, "gitbash") is None, n
    assert winmap.skip_reason("session_restore", "gitbash") is None and winmap.skip_reason("session_restore", "powershell")
    mac = types.SimpleNamespace(be=types.SimpleNamespace(name="mac"), win=False)
    lin = types.SimpleNamespace(be=types.SimpleNamespace(name="wayland"), win=False)
    assert cr.tab_key(mac, 3) == "cmd+3" and cr.tab_key(lin, 9) == "ctrl+9"
    assert cr.fwd("C:\\Users\\x\\w") == "C:/Users/x/w"
    ps = types.SimpleNamespace(pane="powershell")
    bash = types.SimpleNamespace(pane="gitbash")
    assert cov_a.cd_and_append(ps, "C:\\d", "T1", "C:\\f.txt") == "Set-Location -LiteralPath 'C:/d'; Add-Content -LiteralPath 'C:/f.txt' T1"
    assert cov_a.cd_and_append(bash, "/d", "T1", "/f.txt") == "cd /d && echo T1 >> /f.txt"
    assert cr.PWD(types.SimpleNamespace(win=True)) == "pwd -W" and cr.PWD(lin) == "pwd"
    try:
        cr.need_relaunch_app(types.SimpleNamespace(app=None), "x")
        raise AssertionError("the sim backend must skip")
    except cases.Skip:
        pass
    cr.need_relaunch_app(types.SimpleNamespace(app=types.SimpleNamespace(start=1, stop=1)), "x")


@check
def g11_password_prompt_host_and_variants():
    """#1535: the real-password ssh host is in the ssh config of both the POSIX and the Windows profile, and the case lists both secret prompts"""
    import cases_caseremote as cr_
    import sshd
    pw = {"port": 2201, "user": sshd.PW_USER, "password": sshd.PW_PASSWORD}
    blk = profile.Profile.password_host_block(pw, "/h")
    assert "Host fakepass" in blk and "Port 2201" in blk and "User pwuser" in blk and "PreferredAuthentications password" in blk and "PubkeyAuthentication no" in blk, blk
    assert "NumberOfPasswordPrompts 1" in blk and "BatchMode no" in blk and "known_hosts" in blk
    both = types.SimpleNamespace(ssh={"pwident": "k", "passphrase": "pp", "pwhost": pw})
    assert [v[0] for v in cr_.password_variants(both)] == ["fakepw", "fakepass"]
    only_pw = types.SimpleNamespace(ssh={"pwhost": pw})
    assert cr_.password_variants(only_pw) == [("fakepass", sshd.PW_PASSWORD, "password", "password")]
    assert cr_.password_variants(types.SimpleNamespace(ssh={})) == []
    for f in ("linux/bin/fakesshd.py", "appci/fakesshd_win.py"):
        src = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", f.replace("appci/", ""))).read() if f.startswith("linux") else open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "fakesshd_win.py")).read()
        assert "FAKE_PASSWORD" in src and "FAKE_PW_USER" in src, f


@check
def g11_windows_editor_handover_and_classification():
    """#1544: the Windows editor hand-over builds a proper file URL and the editor cases are classified for both panes"""
    assert cases.windows_file_url("C:\\Users\\a b\\x.txt") == "file:///C:/Users/a%20b/x.txt", cases.windows_file_url("C:\\Users\\a b\\x.txt")
    for n in ("editor_edit_save", "editor_paste"):
        assert winmap.skip_reason(n, "powershell") is None and winmap.skip_reason(n, "gitbash") is None, n


@check
def g11_bootstrap_reference_and_bracketed_paste_pieces():
    """#1583 G2 / M3: Phosphor's bash bootstrap composes into one sourceable script with no placeholder left, the reference rc sources it after the plain rc,
    and the M3 zsh commands are the ones whose state file the case reads back"""
    boot = profile.compose_bash_bootstrap(True)
    assert boot and "#include" not in boot and "@@" not in boot and "install_bashpreexec" in boot and "WARP_USING_WINDOWS_CON_PTY=true" in boot, (boot or "")[:200]
    assert "WARP_USING_WINDOWS_CON_PTY=false" in profile.compose_bash_bootstrap(False)
    assert profile.compose_bash_bootstrap(True, bootdir="/nonexistent") is None
    rc = profile.ref_boot_rc_text("C:/h/.refrc", "C:/h/.refboot.sh", True)
    assert rc.index('. "C:/h/.refrc"') < rc.index("WARP_SESSION_ID=1") < rc.index('. "C:/h/.refboot.sh"'), rc
    assert "unset zle_bracketed_paste" in cases.M3_UNSET and "M3_$((${+zle_bracketed_paste}))" in cases.M3_UNSET
    assert "2004h" in cases.M3_RESTORE and "${#zle_bracketed_paste}" in cases.M3_RESTORE


@check
def g11_enter_rediag_1606():
    """#1606 rediag: a macOS sample starts at the injector spawn (after the guard) unless APPCI_ENTER_T0=harness; a stale call record is never this
    sample's; the guard cost is its own split key; the OS split joins the injector post, the ktap Return and the app's dequeue on one clock; an
    M3 batch the app did not see is void"""
    saved = os.environ.pop("APPCI_ENTER_T0", None)
    try:
        assert cases.enter_sample_start(100.0, 100.006) == 100.006 and cases.enter_sample_start(100.0, None) == 100.0
        os.environ["APPCI_ENTER_T0"] = "harness"
        assert cases.enter_sample_start(100.0, 100.006) == 100.0
    finally:
        os.environ.pop("APPCI_ENTER_T0", None)
        if saved is not None:
            os.environ["APPCI_ENTER_T0"] = saved
    be = types.SimpleNamespace(calls=[dict(wall0=99.0, post_mach_ms=[5000.0], clock=(4000.0, 1000.0))])
    ctx = types.SimpleNamespace(be=be)
    assert cases.injector_start_wall(ctx, 100.0) is None                # recorded before t0: an older call
    be.calls.append(dict(wall0=100.004, post_mach_ms=[5100.0], clock=(4000.0, 1000.0)))
    assert cases.injector_start_wall(ctx, 100.0) == 100.004
    assert cases.injector_post_mach(ctx) == (5100.0, (4000.0, 1000.0))
    assert cases.injector_start_wall(types.SimpleNamespace(be=types.SimpleNamespace()), 1.0) is None
    sp = cases.enter_split_summary([dict(t0=100.0, inj=100.005, post=100.025, lf=100.075, px=100.076, stamp=100.0765)])
    assert sp["t0_to_inj_ms"] == 5.0 and sp["inj_to_post_ms"] == 20.0 and sp["t0_to_post_ms"] == 25.0 and sp["post_to_lf_ms"] == 50.0, sp
    # one clock: uptime 4000 ms <-> epoch 1000 s. Post at uptime 5100 ms; the OS stamps the event at 5150 ms; ktap's callback at 5150.4 ms
    # (epoch 1001.1504); the app dequeues it 0.9 ms after the stamp; zle-line-finish at uptime 5154 ms (epoch 1001.154)
    kl = ["K down keycode=0 ts_ns=5090000000 target_pid=7 source_pid=1 wall=1001.0900",
          "E down keycode=36 ts_ns=5150000000 target_pid=7 source_pid=1 wall=1001.150400",
          "E up keycode=36 ts_ns=5151000000 target_pid=7 source_pid=1 wall=1001.151200"]
    app = ["x input_order hop=mac_enter stage=app_send_event keycode=36 ts=5.150000000 age_ms=0.900 event_window=1 key_window=1 t=1001150.9"]
    smp = [dict(post_mach_ms=5100.0, clock=(4000.0, 1000.0), lf=1001.154), dict(post_mach_ms=None, clock=None, lf=1.0)]
    o = cases.enter_os_split(smp, kl, app)
    assert o["n"] == 2 and o["n_matched"] == 1, o
    assert o["post_to_evts_ms"] == 50.0 and o["evts_to_tap_ms"] == 0.4 and o["evts_to_app_ms"] == 0.9 and o["evts_to_lf_ms"] == 4.0, o
    o = cases.enter_os_split(smp, kl)                                     # the reference: no app dequeue
    assert "evts_to_app_ms" not in o and o["post_to_evts_ms"] == 50.0, o
    assert cases.enter_os_split(smp, [])["n_matched"] == 0
    L = ["x input_order hop=enter_begin deferred=false buffer=\"a\" input_type=Shell t=999000.0",
         "x input_order hop=enter_begin deferred=false buffer=\"b\" input_type=Shell t=1000500.0",
         "x input_order hop=enter_begin deferred=false buffer=\"c\" input_type=Shell t=1000600.0"]
    assert cases.m3_app_enters(L, 1000.0) == 2 and cases.m3_app_enters(L, 0) == 3
    assert cases.m3_void(0, 30) and cases.m3_void(29, 30) and not cases.m3_void(32, 30) and not cases.m3_void(0, 0)


@check
def g11_startup_shell_passes():
    """#1534: the cmd / pwsh7 passes: settings TOML names the shell, the family drives the helpers, the case list and effect syntax are right"""
    import cases_shells
    assert winmap.VARIANT_FAMILY == {"pwsh7": "powershell", "cmd": "cmd"}
    for v, names in winmap.VARIANT_CASES.items():
        assert "startup_shell_pane" in names and "mode_proof" in names, v
        reg = {c["name"] for c in cases.CASES}
        assert set(names) <= reg, (v, set(names) - reg)
        for n in names:
            assert winmap.skip_reason(n, winmap.VARIANT_FAMILY[v]) is None or n in ("fast_typing",), (v, n)
    cx = types.SimpleNamespace(pane="cmd")
    assert cases.Ctx.append_cmd(cx, "cmd", "tok1", "f.txt") == "echo tok1>>f.txt"
    assert cases.Ctx.pane_shell(cx) == "cmd" and cases.Ctx.pane_shell(types.SimpleNamespace(pane="powershell")) == "pwsh"
    assert cases_shells.identity_ok("pwsh7", ["PS7_Core"]) and not cases_shells.identity_ok("pwsh7", ["PS5_Desktop"])
    assert cases_shells.identity_ok("cmd", ["CMDOK_CMD.EXE"]) and not cases_shells.identity_ok("cmd", ["CMDOK_%COMSPEC:~-7%"])
    pr = profile.Profile(tempfile.mkdtemp(), pane="cmd")
    pr.win, pr.variant = True, "cmd"
    assert pr.local_prompts is False
    pr.pane, pr.variant = "powershell", "pwsh7"
    assert pr.local_prompts is False
    pr.win, pr.pane, pr.variant = False, None, None
    assert pr.gitbash_toml() is None


@check
def g11_upgrade_fixture_pack_unpack_and_migration_count():
    """#1546: the previous build's state packs and unpacks (home-relative, state dirs only), a hostile archive is refused, and applied migrations are counted"""
    import cases_coverage_b as cb
    import sqlite3
    import tarfile
    src, dst = tempfile.mkdtemp(), tempfile.mkdtemp()
    os.makedirs(os.path.join(src, ".local/share/phosphor"))
    db = os.path.join(src, ".local/share/phosphor/warp.sqlite")
    con = sqlite3.connect(db)
    con.execute("create table __diesel_schema_migrations (version text)")
    con.executemany("insert into __diesel_schema_migrations values (?)", [("1",), ("2",), ("3",)])
    con.commit()
    con.close()
    open(os.path.join(src, "not_state.txt"), "w").write("x")
    tarf = os.path.join(tempfile.mkdtemp(), "f.tar.gz")
    assert cb.pack_state(src, tarf) == 1
    assert cb.unpack_state(tarf, dst) == 1 and not os.path.exists(os.path.join(dst, "not_state.txt"))
    assert cb.db_migrations(os.path.join(dst, ".local/share/phosphor/warp.sqlite")) == 3
    assert cb.db_migrations(os.path.join(dst, "missing.sqlite")) is None
    evil = os.path.join(tempfile.mkdtemp(), "evil.tar")
    with tarfile.open(evil, "w") as t:
        ti = tarfile.TarInfo("../escape.txt")
        ti.size = 0
        t.addfile(ti)
    try:
        cb.unpack_state(evil, dst)
        raise AssertionError("a ../ member must be refused")
    except ValueError:
        pass
    reg = {c["name"]: c for c in cases.CASES}
    assert "upgrade_over_previous" in reg and "upgrade_over_previous" not in cases.SMOKE
    assert winmap.skip_reason("upgrade_over_previous", "gitbash") is None


@check
def g11_remote_server_bench_encoding_and_loop():
    """#1157 part 2: the hand-encoded requests, the request_id decode, and the whole measurement loop against a fake proxy that speaks the frame protocol"""
    ci = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "ci")
    sys.path.insert(0, ci)
    try:
        import remote_server_bench as rb
    finally:
        sys.path.remove(ci)
    assert rb.varint(300) == b"\xac\x02" and rb.varint(1) == b"\x01"
    assert rb.msg_resolve_path("r1", "/tmp").hex() == "0a02723112086a060a042f746d70"
    assert rb.request_id_of(rb.msg_list_directory("abc", "/x")) == "abc" and rb.request_id_of(rb.field(2, b"x")) == ""
    fake = os.path.join(tempfile.mkdtemp(), "fake-phosphor")
    open(fake, "w").write("#!/usr/bin/env python3\nimport struct, sys\n"
                          "assert sys.argv[1] == 'remote-server-proxy' and sys.argv[2] == '--identity-key'\n"
                          "sys.path.insert(0, %r)\nimport remote_server_bench as rb\n"
                          "i, o = sys.stdin.buffer, sys.stdout.buffer\n"
                          "while True:\n"
                          "    h = i.read(4)\n"
                          "    if len(h) < 4: break\n"
                          "    b = i.read(struct.unpack('<I', h)[0])\n"
                          "    o.write(rb.frame(rb.field(1, rb.request_id_of(b)) + rb.field(2, b'ok'))); o.flush()\n" % ci)
    os.chmod(fake, 0o755)
    res = rb.measure(fake, 12, 30)
    assert res["rtt_ms"]["n"] == 12 and res["list_ms"]["n"] >= 20 and res["startup_ms"] > 0 and res["burst_ms_per_request"] >= 0, res
    sz = rb.sizes(fake, {})
    assert sz["binary_bytes"] > 0 and sz["gzip9_bytes"] > 0
    d = rb.delta(dict(size=dict(gzip9_bytes=100), rtt_ms=dict(p50=2.0, p95=3.0, p99=4.0)), dict(size=dict(gzip9_bytes=125), rtt_ms=dict(p50=1.5, p95=2.0, p99=5.0)))
    assert d["gzip9_bytes_ratio"] == 1.25 and d["rtt_ms_p50_delta"] == -0.5 and d["rtt_ms_p99_delta"] == 1.0, d


@check
def r18u13_mac_harness_fixes():
    # agent_stream_cancel: the app's ^C route is read from the log; a ^C sent to the pty while the reply streams, or a first press that did not cancel, fails
    log = ("x input_order hop=ctrl_c_keydown powershell_setup_hold=Off t=1\n"
           "x input_order hop=ctrl_c_route route=cancel_conversation t=2\n")
    assert ca.ctrl_c_routes(log) == ["cancel_conversation"]
    assert ca.cancel_route_error([["cancel_conversation"], []]) is None
    assert ca.cancel_route_error([[], []]) is None                                   # a build without the hop: no verdict from it
    assert "went to the pty" in ca.cancel_route_error([["pty"]])
    assert "did not cancel" in ca.cancel_route_error([["none"], ["cancel_conversation"]])
    assert ca.cancel_route_error([["stop_conversation_and_command"]]) is None
    # zsh rc: a line that reuses a dropped line's history number is still logged (preexec flag), and the flag is cleared at every prompt
    assert "__pa_ran=1" in profile.ZSHRC and "|| -n $__pa_ran" in profile.ZSHRC and "\n  __pa_ran=\n" in profile.ZSHRC
    # ui_palette: a frame that rendered CommandPaletteView proves the palette opened; no frame lines at all is no evidence
    import cases_ui as _U
    assert _U.palette_rendered('[DEBUG] frame 1: updated=["Workspace", "CommandPaletteView", "Menu"] removed=0') is True
    assert _U.palette_rendered('[DEBUG] frame 1: updated=["Workspace", "Menu"] removed=0') is False
    assert _U.palette_rendered("no frames here") is None
    # flood_scrollback_find: the typed command carries the marker reversed (never the marker itself); macOS Find is Cmd-F (Cmd-Shift-F = global search)
    import inspect, cases_robust as _R
    src = inspect.getsource(_R.c_flood_find)
    assert "marker[::-1]" in src and "| rev" in src and '"cmd+f" if ctx.be.name == "mac"' in src and "cmd+shift+f" not in src.split("APPCI_KEY_FIND")[1][:80]
    # r18u23: the marker is its own proven command before the flood; Find must also have scrolled the view away from the flood's tail
    assert "marker_cmd in ctx.cmds_since" in src and "scrolled" in src and "tail_before" in src and src.index("marker_cmd") < src.index("endless_flood")
    import cases as _C
    assert _C.verdict_gave_up("input_order hop=kh_enter_gate_give_up seq=16685 by=verdict_cap behind=0 submitted=false t=1") is True
    assert _C.verdict_gave_up("input_order hop=kh_enter_gate_give_up seq=6 by=route_wait behind=1 submitted=true t=5") is False
    assert "verdict_give_up_retries" in inspect.getsource(_C.c_ssh_nested) and "range(3)" in inspect.getsource(_C.c_ssh_nested)
    # crash_kill9_restore / crash_corrupt_db: the post-restore line names its effect file absolutely (a restored pane keeps its saved cwd)
    import cases_coverage_a as _A
    assert 'local_cmd_exact(ctx, r, "ck_after", absolute=True)' in inspect.getsource(_A.c_crash_kill9)
    assert 'local_cmd_exact(ctx, r, "cd_after", absolute=True)' in inspect.getsource(_A.c_crash_corrupt_db)
    sent = []
    fake = types.SimpleNamespace(tok=lambda n: "tok123", rm=lambda *a: None, cmd_mark=lambda: 0, pane_shell=lambda: "bash", W="/w/dir",
                                 append_cmd=lambda sh, t, f: "echo %s >> %s" % (t, f), be=types.SimpleNamespace(type=lambda line, d, enter: sent.append(line)),
                                 rng=types.SimpleNamespace(randint=lambda a, b: a), until=lambda f, t: None, lines=lambda f: ["tok123"],
                                 prof=types.SimpleNamespace(commands=lambda: []))
    _A.local_cmd_exact(fake, cases.Result("x", "terminal"), "ck_after", absolute=True)
    _A.local_cmd_exact(fake, cases.Result("x", "terminal"), "ck_after")
    assert sent == ["echo tok123 >> /w/dir/ca_local_ck_after.txt", "echo tok123 >> ca_local_ck_after.txt"], sent
    # ui_settings_live: the theme step switches following the system theme off before it writes `theme`
    assert '"system_theme", "false"' in inspect.getsource(_U.c_settings)


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
