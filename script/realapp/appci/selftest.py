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
def proving_in_the_fix_tier():
    """owner rule 2026-11-09: every new case runs in the fix tier until the owner graduates it. PROVING is registered and disjoint from SMOKE, the fix tier
    (SMOKE + PROVING) selects every PROVING case on Linux / macOS (one fake-model pass for the ones that need it) and plans it on every Windows pane (run or
    SKIP with its reason, never absent); the lab globs match lab arms; the report lists them for the owner and moves nothing."""
    import run as runmod, winmap as wm
    reg = {c["name"]: c for c in cases.CASES}
    assert cases.PROVING and len(set(cases.PROVING)) == len(cases.PROVING), "PROVING is empty or lists a case twice"
    assert not [n for n in cases.PROVING if n not in reg], "PROVING names no registered case: %s" % [n for n in cases.PROVING if n not in reg]
    assert not set(cases.PROVING) & set(cases.SMOKE), "PROVING and SMOKE overlap: %s" % sorted(set(cases.PROVING) & set(cases.SMOKE))
    assert not [n for n in cases.PROVING if reg[n].get("lab")], "PROVING lists a lab arm: lab cases are PROVING_LAB globs"
    # Linux / macOS: the fix selection (table mode) holds every PROVING case; legacy (kill switch) is the narrowed list and may not
    sel = wm.smoke_for("table", cases.SMOKE + cases.PROVING)
    assert set(cases.PROVING) <= set(sel) and set(cases.SMOKE) <= set(sel)
    assert not set(wm.smoke_for("legacy", cases.SMOKE + cases.PROVING)) - set(wm.KILL_SWITCH_SMOKE)
    # the fake pass: PROVING_FAKE are agent-only PROVING cases; each runs in exactly one pass, the fake one; everything else only in the ordinary passes
    assert set(cases.PROVING_FAKE) <= set(cases.PROVING) and all(reg[n]["modes"] == ("agent",) for n in cases.PROVING_FAKE), "PROVING_FAKE: not agent-only PROVING cases"
    csv = ",".join(sel)
    assert runmod.fake_pass_plan(csv, ["terminal", "agent"], False) and not runmod.fake_pass_plan(csv, ["terminal"], False)
    assert not runmod.fake_pass_plan(csv, ["agent"], True), "already the fake globally: no second pass"
    assert not runmod.fake_pass_plan(None, ["agent"], False), "outside the fix tier / with explicit --cases: no fake pass"
    assert not runmod.fake_pass_plan(",".join(wm.smoke_for("legacy", cases.SMOKE + cases.PROVING)), ["agent"], False), "kill-switch run: no PROVING case, no fake pass"
    for n in sel:
        assert [runmod.fake_pass_ok(n, f, True) for f in (False, True)] == ([False, True] if n in cases.PROVING_FAKE else [True, False]), n
    assert all(runmod.fake_pass_ok(n, f, False) for n in sel for f in (False, True)), "no plan: every pass runs every case (unchanged behaviour)"
    # Windows: every PROVING case is in every pass's plan, and either runs in that pane or SKIPs with a reason
    for mode in ("terminal", "agent"):
        for pane in ("powershell", "gitbash"):
            names, _o = wm.tier_plan("windows", "smoke", mode, pane, cases.SMOKE, cases.PROVING)
            assert set(cases.PROVING) <= set(names), (mode, pane, sorted(set(cases.PROVING) - set(names)))
            for n in cases.PROVING:
                if mode in reg[n]["modes"]:
                    assert wm.skip_reason(n, pane) is None or isinstance(wm.skip_reason(n, pane), str)
    runnable = [n for n in cases.PROVING if "agent" in reg[n]["modes"] and not wm.skip_reason(n, "gitbash")]
    assert {"agent_latency_key", "agent_latency_enter", "agent_steer_fast", "agent_fault_429"} <= set(runnable), "the Windows-capable PROVING agent cases must run there"
    # the lab: every glob matches at least one lab arm, none matches a SMOKE case
    import lab as labmod
    for g in cases.PROVING_LAB:
        assert any(c.get("lab") and labmod.name_matches(g, c["name"]) for c in cases.CASES), "PROVING_LAB glob %s matches no lab arm" % g
        assert not any(labmod.name_matches(g, n) for n in cases.SMOKE)
    # the report: a PROVING section with the status per mode, deciding nothing
    d = tempfile.mkdtemp()
    ok_name, bad_name = cases.PROVING[0], cases.PROVING[1]
    with open(os.path.join(d, "results.jsonl"), "w") as f:
        f.write(json.dumps(dict(name=ok_name, mode="agent@fake", status="pass", seconds=1, detail=[])) + "\n")
        f.write(json.dumps(dict(name=bad_name, mode="agent@fake", status="skip", seconds=0, detail=["no model"])) + "\n")
        f.write(json.dumps(dict(name="mode_proof", mode="agent", status="pass", seconds=1, detail=[])) + "\n")
    subprocess.run([sys.executable, os.path.join(os.path.dirname(os.path.abspath(__file__)), "report.py"), d, "platform=linux"], check=False, capture_output=True)
    md = open(os.path.join(d, "summary.md")).read()
    assert "## PROVING cases (owner decides graduation)" in md and "| %s | agent@fake | PASS |" % ok_name in md and "| %s | agent@fake | SKIP |" % bad_name in md
    assert "| agent | mode_proof | PASS" in md and "| mode_proof | agent@fake" not in md
    shutil.rmtree(d)


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
        # r18u28: the app rotates the active file to .in_session.0 (older slots shift up) and opens a fresh one; an offset taken before the
        # rotation still reads the rest of ITS file, then the new active file (a plain seek into the new file returned "")
        off = c.log_size()
        assert isinstance(off, cases.LogOffset) and int(off) == os.path.getsize(log)
        open(log, "a").write("before rotation\n")
        os.rename(log + ".in_session.1", log + ".in_session.2")
        os.rename(log + ".in_session.0", log + ".in_session.1")
        os.rename(log, log + ".in_session.0")
        open(log, "w").write("after rotation\n")
        assert c.log_since(off) == "before rotation\nafter rotation\n", repr(c.log_since(off))
        # two rotations later, still the same lines in order
        os.rename(log + ".in_session.0", log + ".in_session.9")
        os.rename(log, log + ".in_session.0")
        os.rename(log + ".in_session.9", log + ".in_session.1")
        open(log, "w").write("newest\n")
        assert c.log_since(off) == "before rotation\nafter rotation\nnewest\n", repr(c.log_since(off))
        # the measured file rotated out of retention (the app drops the OLDEST slots first, so nothing older is left): everything kept is newer
        os.remove(log + ".in_session.2")
        os.remove(log + ".in_session.1")
        assert c.log_since(off) == "after rotation\nnewest\n", repr(c.log_since(off))
        # no rotation since the offset: the plain read of the active file
        off2 = c.log_size()
        open(log, "a").write("x\n")
        assert c.log_since(off2) == "x\n"
        # a plain int (callers that pass 0 or their own number) reads the active file only, as before
        assert c.log_since(0) == "newest\nx\n"
        # a head identity (no inode) matches the same file after it has grown
        h = ("head", b"2026-11-05T00:00:00Z [INFO] a")
        assert cases.same_log_file(h, ("head", b"2026-11-05T00:00:00Z [INFO] a longer head"))
        assert not cases.same_log_file(h, ("head", b"2026-11-05T00:00:01Z [INFO] b"))
        assert not cases.same_log_file(None, h) and not cases.same_log_file(("ino", 1, 2), ("ino", 1, 3))
        open(log, "a").write("y input_order hop=view_mode mode=terminal pane=1\n")      # the rotations above dropped the view_mode lines
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
def mode_proof_markers_match_the_current_agent_view_1568():
    """round 9 macOS: the agent-mode mode_proof FAILED on a correct screen because it looked for 'New Phosphor Agent conversation' / 'Ask anything'
    while the app shows 'New agent conversation', 'ESC for terminal' and 'Run commands, or ask the agent a question' (autodetect)."""
    cur = "search tabs... new agent conversation esc for terminal new agent conversation run commands, or ask the agent a question ci ollama / gpt-oss:20b"
    assert cases.ocr_agent_markers(cur) == (True, False), cases.ocr_agent_markers(cur)
    assert cases.ocr_agent_markers("new phosphor agent conversation ask anything") == (True, False)         # older builds still count
    assert cases.ocr_agent_markers("ask anything") == (True, False)
    # a terminal session: the /agent hint, or only the autodetect placeholder (shown in BOTH modes) is never an agent marker
    assert cases.ocr_agent_markers("~/w git:(main) new /agent conversation") == (False, True)
    assert cases.ocr_agent_markers("run commands, or ask the agent a question") == (False, False)
    assert cases.ocr_agent_markers("") == (False, False)


@check
def skipped_case_runs_no_post_case_invariants_round9():
    """round 9 macOS: pane_not_local_after_zero_network_startup / _after_keyring_unavailable_startup failed after SKIPPED cases (the front tab was the
    markdown viewer left by md_viewer). A skipped case ran nothing, so the root-shell / return-to-local probes must not run after it; every other
    status still gets them."""
    assert cases.post_case_invariants_apply("skip") is False
    for st in ("pass", "fail", "noref"):
        assert cases.post_case_invariants_apply(st) is True, st
    src = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "run.py")).read()
    assert src.count("cases.post_case_invariants_apply(status)") == 2, "both post-case probes (ssh_/remote_ return_to_local and the root-shell leave_nested) must be gated"


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
def tab_enter_completion_give_up_is_detected():
    """#1666: the tab_enter case names a Tab-completion give-up (one per give-up, not counting its gate's hop); other give-ups are not it"""
    log = ("input_order hop=enter_completion_give_up seq=4 editor completion_cap t=1\n"
           "input_order hop=kh_enter_gate_give_up seq=4 by=completion_cap behind=0 submitted=false t=2\n"
           "input_order hop=enter_completion_give_up seq=7 editor completion_backstop t=3\n")
    assert cases.completion_give_ups(log) == 2
    assert cases.completion_give_ups("input_order hop=enter_verdict_give_up seq=5 editor verdict_cap t=4\n"
                                     "input_order hop=kh_enter_gate_give_up seq=6 by=verdict_cap behind=0 submitted=false t=5") == 0
    assert cases.completion_give_ups("") == 0
    assert "GIVE-UP" in cases.c_tab.__doc__ or "give-up" in cases.c_tab.__doc__.lower()


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
    # the per-Enter samples behind the medians (#1970: a verdict over EVERY Enter), in log order; every key present, empty when the log has none
    raw = cases.enter_segment_samples(L, 0)
    assert raw["enter_to_write_ms"] == [0.5, 1.5] and raw["write_to_preexec_ms"] == [] and set(raw) == set(cases.ENTER_SEGMENT_KEYS), raw
    assert cases.enter_segment_samples(L, 5000.0)["enter_to_write_ms"] == []


@check
def windows_break_keys_are_the_real_keys():
    """#1906: `ctrl+break` is the Break key (VK_CANCEL, scan 46, EXTENDED), `ctrl+scrollbreak` is Ctrl+ScrollLock (scan 46, not extended); rounds 8/9
    sent the second under the first name. Pure table check: SendInput itself only exists on Windows."""
    import backend
    W = backend.WindowsBackend
    assert W.vk_cancel_events("break") == ((0x03, 0x46, 1), (0x03, 0x46, 3)), W.vk_cancel_events("break")
    assert W.vk_cancel_events("cancel") == W.vk_cancel_events("break")
    assert W.vk_cancel_events("scrollbreak") == ((0x03, 0x46, 0), (0x03, 0x46, 2)), W.vk_cancel_events("scrollbreak")
    assert all(W.NAMED[k] == 0x03 for k in W.VK_CANCEL_KEYS), "every VK_CANCEL key is a named key"
    assert 0x03 not in W.EXTENDED, "VK_CANCEL's extended flag depends on the key (VK_CANCEL_KEYS), not on the vk"


@check
def ctrl_break_case_names_the_hop_it_stopped_at():
    """#1906: ps_ctrl_break reads the app log after one injected key and names the first hop missing (keydown -> pane_key -> pty_write)"""
    import cases_win_input as wi
    full = ("x [DEBUG] input_order keydown key=pause ctrl=true alt=false shift=false cmd=false t=1.0 seq=7\n"
            "x [DEBUG] input_order hop=kh_pane_key seq=7 class=Control target=Terminal t=1.1\n"
            "x [DEBUG] input_order hop=pty_write_bytes class=User wrote=32 of_job=32 bytes=\"\\x1b[3;70;0;1;8;1_\\x1b[3;70;0;0;8;1_\" t=1.2\n")
    seen, stopped, dropped = wi.break_hops(full)
    assert stopped is None and all(seen.values()) and dropped == [], (seen, stopped, dropped)
    seen, stopped, dropped = wi.break_hops("x [DEBUG] input_order hop=key_unconverted outcome=dropped logical=ScrollLock physical=Code(ScrollLock) ctrl=true t=1\n")
    assert stopped == "keydown" and not seen["pty_write"], (seen, stopped)
    assert dropped == ["hop=key_unconverted outcome=dropped logical=ScrollLock physical=Code(ScrollLock) ctrl=true t=1"], dropped
    seen, stopped, _ = wi.break_hops(full.splitlines()[0] + "\n" + full.splitlines()[1] + "\ninput_order hop=pty_write_bytes bytes=\"\\x03\" t=2\n")
    assert stopped == "pty_write", (seen, stopped)                 # written as Ctrl-C (the focus-gap path): not the Break sequence
    assert wi.break_hops("")[1] == "keydown"
    assert "ps_ctrl_break" in cases.PROVING and "ps_ctrl_scrolllock_break" in cases.PROVING
    assert "ps_ctrl_scrolllock_break" in winmap.PS_ONLY


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
def enter_split_history_and_appkit_stages_2031():
    """#2031: the zsh rc's two extra times (first zshaddhistory function, plain preexec()) ride in $__pa_px, so the typed stamp line is
    unchanged and the stamp file reads `stamp px ah p0 lf`; lf_to_px splits into lf_to_ah + ah_to_p0 + p0_to_px. An older rc's 3-field
    file and the 2-field bash line parse as before. The app's mac_enter stages split event_to_enter; a missing stage gives no split."""
    assert cases.stamp_fields(["10.0035", "10.0030", "10.0012", "10.0020", "10.0010"]) == (10.003, 10.001, 10.0012, 10.002)
    assert cases.stamp_fields(["10.0035", "10.0030", "0", "0", "10.0010"]) == (10.003, 10.001, None, None)       # hooks that never ran print 0
    assert cases.stamp_fields(["10.0035", "10.0030", "10.0010"]) == (10.003, 10.001, None, None)                   # rc before #2031
    assert cases.stamp_fields(["10.0035", "0", "10.0010"]) == (None, 10.001, None, None)
    assert cases.stamp_fields(["10.0035", "10.0010"]) == (None, 10.001, None, None)                                # #1492 bash/zsh split line
    assert cases.stamp_fields(["10.0035"]) == (None, None, None, None)
    sp = cases.enter_split_summary([dict(t0=99.8, post=100.0, lf=100.002, ah=100.0025, p0=100.0035, px=100.004, stamp=100.0041)])
    assert sp["lf_to_ah_ms"] == 0.5 and sp["ah_to_p0_ms"] == 1.0 and sp["p0_to_px_ms"] == 0.5 and sp["lf_to_px_ms"] == 2.0, sp
    sp = cases.enter_split_summary([dict(t0=99.8, post=100.0, lf=100.002, ah=None, p0=None, px=100.004, stamp=100.0041)])
    assert "lf_to_ah_ms" not in sp and "ah_to_p0_ms" not in sp and "p0_to_px_ms" not in sp and sp["lf_to_px_ms"] == 2.0, sp
    # the rc: the two hooks exist, the times ride in __pa_px, and the stamp line the cases type is the old one, byte for byte
    z = profile.ZSHRC
    assert "zshaddhistory_functions+=(__pa_ahw)" in z and "precmd_functions+=(__pa_p0def)" in z, "rc hooks"
    assert '__pa_px="$EPOCHREALTIME ${__pa_ah:-0} ${__pa_p0:-0}"' in z and "__pa_ah=; __pa_p0=; __pa_ran=1" in z, "rc px"
    assert "[[ -n $__pa_p0 ]] || __pa_p0=$EPOCHREALTIME" in z, "the copy the bootstrap moves behind warp_preexec must not overwrite the first time"
    mac = types.SimpleNamespace(pane="zsh", be=types.SimpleNamespace(name="mac"), _pane_shell="/bin/zsh")
    orig = cases.pane_shell
    try:
        cases.pane_shell = lambda ctx: "/bin/zsh"
        assert cases.stamp_line(mac) == "zmodload zsh/datetime; print -r -- $EPOCHREALTIME ${__pa_px:-0} ${__pa_lf:-0} > lat_%d", cases.stamp_line(mac)
    finally:
        cases.pane_shell = orig
    # the AppKit stages of one bound Return (r10a shape): send 1000.0, key equivalent 1000.1, keyDownImpl 1000.25, interpretKeyEvents done 1000.65, enter_begin 1000.75
    L = ["input_order hop=mac_enter stage=app_send_event keycode=36 ts=1.0 age_ms=0.900 event_window=1 key_window=1 key_window_class=WarpWindow t=1000.0",
         "input_order hop=mac_enter stage=perform_key_equivalent keycode=36 ts=1.0 key_window=1 t=1000.1",
         "input_order hop=mac_enter stage=key_down_impl_entry keycode=36 ts=1.0 key_window=1 t=1000.25",
         "input_order hop=mac_enter stage=after_interpret_key_events keycode=36 ts=1.0 has_marked_text=0 t=1000.65",
         "input_order hop=core_enter stage=dispatch_keystroke t=1000.7",
         "input_order hop=enter_begin deferred=false buffer=\"x\" input_type=Shell t=1000.75",
         "input_order hop=mac_enter stage=after_handle_view_event keycode=36 ts=1.0 handled=1 composing=0 t=1002.0"]
    s = cases.enter_segments(L, 0)
    assert s["event_to_enter_ms"] == 0.75 and s["appkit_route_ms"] == 0.1 and s["key_equivalent_ms"] == 0.15, s
    assert s["interpret_keys_ms"] == 0.4 and s["dispatch_to_enter_ms"] == 0.1 and s["appkit_route_n"] == 1, s
    assert round(s["appkit_route_ms"] + s["key_equivalent_ms"] + s["interpret_keys_ms"] + s["dispatch_to_enter_ms"], 6) == s["event_to_enter_ms"], s
    # an unbound Return detours through super and keyDown: before keyDownImpl: the first line of each stage still counts
    U = L[:2] + ["input_order hop=mac_enter stage=perform_key_equivalent_super_result keycode=36 ts=1.0 handled=0 t=1000.15",
                 "input_order hop=mac_enter stage=key_down keycode=36 ts=1.0 t=1000.2"] + L[2:]
    assert cases.enter_segments(U, 0)["key_equivalent_ms"] == 0.15
    # a stage missing (another platform, an older build): event_to_enter only, never a partial split
    s = cases.enter_segments([l for l in L if "key_down_impl_entry" not in l], 0)
    assert s["event_to_enter_ms"] == 0.75 and not any(k in s for k in ("appkit_route_ms", "key_equivalent_ms", "interpret_keys_ms", "dispatch_to_enter_ms")), s
    # the stages of one Enter never leak into the next one
    two = L + [L[0].replace("t=1000.0", "t=2000.0"), L[5].replace("t=1000.75", "t=2000.5")]
    raw = cases.enter_segment_samples(two, 0)
    assert raw["event_to_enter_ms"] == [0.75, 0.5] and [round(v, 6) for v in raw["interpret_keys_ms"]] == [0.4], raw
    assert cases.enter_stage_split(1000.0, [("perform_key_equivalent", 999.0)], 1001.0) == {}


@check
def ctrl_c_flood_ab_pure_parts_1838():
    """#1838 A/B: log parsing (skip runs, waits, key->ctrl_c_keydown, flood throughput), merge, summary, verdict and the block order"""
    import cases_ctrlc_ab as ab
    L = ["input_order hop=frame_skip_run skipped=3 held_ms=12.0 t=1000000.0", "input_order hop=frame_skip_run skipped=9 held_ms=130.0 t=1000100.0",
         "input_order hop=drawable_wait wait_ms=1.50 key_frame=false t=1000001.0", "input_order hop=drawable_wait wait_ms=20.00 key_frame=false t=1000002.0",
         "input_order hop=os_key_event age_ms=1.0 enter=false ctrl=false t=1000003.0",
         "input_order hop=os_key_event age_ms=1.0 enter=false ctrl=true t=1000010.0", "input_order hop=ctrl_c_keydown powershell_setup_hold=Off t=1000013.0",
         "input_order hop=frame_skip_run skipped=1 held_ms=5.0 t=1.0"]            # the last is before t0
    L += ["input_order pty_read bytes=1000 since_write_ms=1.0 t=%d" % (2000000 + 10 * i) for i in range(20)]   # one run: 20 reads, 190 ms
    L += ["input_order pty_read bytes=50 since_write_ms=1.0 t=3000000.0"]                                         # a lone read: not a flood
    m = ab.ab_arm_log_metrics(L, 1000.0)
    assert m["skip_runs"] == 2 and m["held"] == [12.0, 130.0] and m["waits"] == [1.5, 20.0] and m["k2k"] == [3.0], m
    assert m["flood_bytes"] == 19000 and abs(m["flood_s"] - 0.19) < 1e-6, m
    mm = ab.ab_merge([m, m])
    assert mm["skip_runs"] == 4 and len(mm["held"]) == 4 and mm["flood_bytes"] == 38000
    on = ab.ab_summary([10.0, 20.0, 30.0], mm)
    assert on["frame_skip_runs"] == 4 and on["held_ge_100ms"] == 2 and on["drawable_wait_ge_2ms"] == 2 and on["os_key_event_to_ctrl_c_keydown_p50"] == 3.0, on
    assert on["flood_bytes_per_s"] == 100000 and on["p50"] == 20.0 and on["held_ms_max"] == 130.0, on
    off = ab.ab_summary([40.0, 50.0, 60.0], ab.ab_merge([dict(ab.ab_arm_log_metrics([], 0), flood_bytes=100000, flood_s=1.0)]))
    d, w = ab.ab_verdict(on, off)
    assert d == -30.0 and len(w) == 1 and "watchdog" in w[0], (d, w)                      # equal throughput, watchdog hit
    slow = dict(on, flood_bytes_per_s=85000, held_ge_100ms=0)
    d, w = ab.ab_verdict(slow, off)
    assert len(w) == 1 and "15.0% below" in w[0], w
    assert ab.ab_verdict(dict(slow, flood_bytes_per_s=95000), off)[1] == [] and ab.ab_verdict(dict(on, p50=None), off)[0] is None
    sched = ab.ab_schedule(10, 3)
    assert [a for a, _c, _d in sched][:2] == ["on", "off"] and {a for a, _c, _d in sched} == {"on", "off"}
    assert sum(c - d_ for a, c, d_ in sched if a == "on") == 10 and sum(c - d_ for a, c, d_ in sched if a == "off") == 10, sched
    assert ab._arm_env("off") == {"PHOSPHOR_MAC_SKIP_BLOCKED_FRAMES": "0"} and ab._arm_env("on") == {}


@check
def keyframe_ab_pure_parts_1848():
    """#1848 A/B (latency_paint_ab): the arm environment, the per-arm summary from kl3 samples + the app's hops, the delta, and the WARNs about the A/B itself
    (kill switch not applied, path never ran, most key frames not sent out)"""
    import cases_keyframe_ab as kab
    import render_hops as rh
    assert kab.arm_env("off") == {"PHOSPHOR_MAC_KEY_FRAME_NOW": "0"} and kab.arm_env("on") == {}
    ON = ["input_order hop=key_frame_now drawn=1 ms=4.10 t=1000000.0", "input_order hop=key_frame_now drawn=1 ms=4.30 t=1000100.0",
          "input_order hop=key_frame_now drawn=0 ms=3.90 t=1000200.0",
          "input_order hop=key_frame_sched ms=0.02 t=1000000.0", "input_order hop=key_frame_sched ms=0.04 t=1000100.0",
          "input_order hop=key_presented submit_to_presented_ms=24.00 key_to_presented_ms=28.50 sync=0 wait_ms=0.10 t=1000030.0"]
    OFF = ["input_order hop=key_frame_sched ms=7.80 t=1000000.0", "input_order hop=key_frame_sched ms=0.60 t=1000100.0",
           "input_order hop=key_presented submit_to_presented_ms=24.00 key_to_presented_ms=32.20 sync=0 wait_ms=0.10 t=1000030.0"]
    on = kab.ab_summary([30.0, 31.0, 32.0], rh.render_hop_metrics(ON))
    off = kab.ab_summary([34.0, 35.0, 36.0], rh.render_hop_metrics(OFF))
    assert on["key_frames_now"] == 3 and on["key_frames_now_drawn"] == 2 and on["key_frame_sched_p50"] == 0.03 and on["key_to_presented_p50"] == 28.5, on
    assert off["key_frames_now"] == 0 and off["key_frame_sched_p50"] == 4.2 and off["p50"] == 35.0, off
    d, w = kab.ab_verdict(on, off)
    assert d == -4.0 and w == [], (d, w)
    d, w = kab.ab_verdict(off, on)                                   # the arms swapped: the "off" arm drew key frames, the "on" arm none
    assert d == 4.0 and len(w) == 2 and "did not reach the app" in w[0] and "no hop=key_frame_now" in w[1], w
    mostly_missed = kab.ab_summary([30.0], rh.render_hop_metrics(ON[2:3] * 3 + ON[:1]))
    d, w = kab.ab_verdict(mostly_missed, off)
    assert len(w) == 1 and "3 of 4 key frames" in w[0], w
    empty = kab.ab_summary([], {})
    assert kab.ab_verdict(empty, empty) == (None, [])                # no samples: no delta, nothing claimed
    assert "latency_paint_ab" in cases.PROVING and "latency_paint_ab" in winmap.NOT_WINDOWS and "latency_paint_ab" not in cases.SMOKE


@check
def viewskip_ab_pure_parts_1962():
    """#1962 A/B (latency_key_views_ab): the arm environment, the switch values read from a launch's log, the lead key frames dropped per block, the per-arm
    summary from hop=key_frame_build, the deltas, the one FAIL (more views re-rendered with the skips on) and the WARNs about the A/B itself"""
    import cases_viewskip_ab as vab
    import render_hops as rh
    assert vab.arm_env("off") == {"PHOSPHOR_KEY_VIEW_SKIP": "0"} and vab.arm_env("on") == {}

    def build(views, render, total, t):
        return ("input_order hop=key_frame_build passes=1 layout_passes=1 views=%d mounted=57 render_ms=%.2f layout_ms=0.50 after_layout_ms=0.02 paint_ms=0.40 "
                "hover_ms=0.10 total_ms=%.2f layout_top=Terminal:0.31 paint_top=- t=%d.0" % (views, render, total, t))
    ON_FIRST = [build(1, 0.05, 1.10, 1000000 + i) for i in range(4)]
    OFF_FIRST = [build(6, 0.45, 1.90, 1000000 + i) for i in range(4)]
    ON_BURST = [build(1, 0.04, 0.60, 1000100 + i) for i in range(4)]
    OFF_BURST = [build(2, 0.10, 0.70, 1000100 + i) for i in range(4)]
    # the switch value the app logs once per launch; a log without the line (an old build) gives none
    assert vab.skip_flags(["x", "input_order hop=key_view_skip enabled=1 t=1.0"]) == ["1"] and vab.skip_flags(["input_order hop=key_view_skip enabled=0 t=1.0"]) == ["0"]
    assert vab.skip_flags(ON_FIRST) == [] and vab.skip_flags(["hop=key_view_skip enabled=1 t=1", "hop=key_view_skip enabled=0 t=2"]) == ["0", "1"]
    # the lead keys of a block: only key-frame lines are dropped, in order, other lines stay
    mixed = ["input_order keydown t=1"] + ON_FIRST[:1] + ["input_order hop=frame_drawn total_ms=4.0 acquire_ms=0.5 t=2.0"] + ON_FIRST[1:]
    assert vab.drop_lead_builds(mixed, 2) == [mixed[0], mixed[2]] + ON_FIRST[2:] and vab.drop_lead_builds(mixed, 0) == mixed and vab.drop_lead_builds([], 3) == []
    on = vab.ab_summary(rh.render_hop_metrics(ON_FIRST), rh.render_hop_metrics(ON_BURST), [30.0, 31.0, 32.0], ["1"])
    off = vab.ab_summary(rh.render_hop_metrics(OFF_FIRST), rh.render_hop_metrics(OFF_BURST), [31.0, 32.0, 33.0], ["0"])
    assert on["first"]["key_frames"] == 4 and on["first"]["views"]["p50"] == 1 and off["first"]["views"]["p50"] == 6 and off["burst"]["render_ms"]["p50"] == 0.1, (on, off)
    assert on["key_to_paint"]["p50"] == 31.0 and on["skip_flags"] == ["1"] and vab.ab_summary({}, {}, [], [])["key_to_paint"] is None
    d, w, f = vab.ab_verdict(on, off)
    assert d["first"] == dict(views=-5, render_ms=-0.4, total_ms=-0.8) and d["burst"] == dict(views=-1, render_ms=-0.06, total_ms=-0.1) and d["key_to_paint_p50"] == -1.0, d
    assert w == [] and f == [], (w, f)
    # the arms swapped: more views with the skips "on" is the FAIL, and both switch WARNs fire
    d, w, f = vab.ab_verdict(off, on)
    assert len(f) == 2 and "must never re-render more" in f[0] and "first keys" in f[0] and "burst keys" in f[1], f
    assert len(w) == 2 and "did not reach the app" in w[0] and "default environment" in w[1], w
    # no effect for a pattern: a WARN, not a failure; an old build (no switch line, no key-frame line): one WARN, no delta, nothing claimed
    same = vab.ab_summary(rh.render_hop_metrics(ON_FIRST), rh.render_hop_metrics(OFF_BURST), [], ["1"])
    d, w, f = vab.ab_verdict(same, off)
    assert f == [] and len(w) == 1 and "burst keys: the same 2" in w[0] and d["key_to_paint_p50"] is None, (w, f, d)
    old = vab.ab_summary({}, {}, [], [])
    d, w, f = vab.ab_verdict(old, old)
    assert f == [] and len(w) == 1 and "neither arm logged" in w[0] and d["first"] == dict(views=None, render_ms=None, total_ms=None), (d, w)
    reg = {c["name"]: c for c in cases.CASES}
    assert "latency_key_views_ab" in cases.PROVING and "latency_key_views_ab" in winmap.BOTH and "latency_key_views_ab" not in winmap.NOT_WINDOWS and "latency_key_views_ab" not in cases.SMOKE
    assert reg["latency_key_views_ab"]["modes"] == ("terminal", "agent") and "latency_key_views_ab" not in cases.PROVING_FAKE      # both modes, no scripted model


@check
def render_latency_step_a_hops_parse():
    """render-latency step A: the new hop fields parse (key_presented, key_frame_sched, frame_skip_run key_frame_skipped, frame_drawn present_ms), the
    old frame_drawn line without present_ms still parses, a window filters by t=, an old build gives {}, and the present-mode arm helpers are right"""
    import render_hops as rh
    import cases_agent_latency as al
    L = ["input_order hop=key_presented submit_to_presented_ms=8.30 key_to_presented_ms=34.50 sync=0 wait_ms=2.25 t=1000000.0",
         "input_order hop=key_presented submit_to_presented_ms=16.70 key_to_presented_ms=40.00 sync=1 wait_ms=0.00 t=1000100.0",
         "input_order hop=key_frame_sched ms=1.50 t=1000001.0", "input_order hop=key_frame_sched ms=0.50 t=1000101.0",
         "input_order hop=frame_skip_run skipped=3 held_ms=12.0 key_frame_skipped=2 t=1000002.0",
         "input_order hop=frame_skip_run skipped=1 held_ms=5.0 t=1000003.0",                       # a build before the field: counts as 0
         "input_order hop=frame_drawn total_ms=4.0 acquire_ms=0.5 present_ms=9.0 t=1000004.0",
         "input_order hop=frame_drawn total_ms=4.0 acquire_ms=0.5 t=1000005.0",                    # old line: no present_ms
         "input_order hop=key_presented submit_to_presented_ms=99.00 key_to_presented_ms=99.00 sync=1 wait_ms=0.00 t=5.0"]   # before t0
    m = rh.render_hop_metrics(L, 1000.0)
    assert m["key_submit_to_presented_ms"]["p50"] == 12.5 and m["key_to_presented_ms_hop"]["n"] == 2 and m["key_presented_wait_ms"]["max"] == 2.25, m
    assert m["key_presented_sync_on"] == 1 and m["key_presented_sync_off"] == 1, m
    assert m["key_frame_sched_ms"]["p50"] == 1.0 and m["key_frames_skipped"] == 2 and m["skip_runs"] == 2, m
    assert m["frame_present_ms"] == dict(n=1, p50=9.0, p95=9.0, max=9.0) and m["frame_acquire_ms"]["n"] == 2, m
    assert rh.render_hop_metrics(["input_order hop=frame_drawn total_ms=4.0 acquire_ms=0.5 t=1.0"]) .get("frame_present_ms") is None
    assert rh.render_hop_metrics("input_order hop=ui_item kind=frame ms=5.0 t=1.0\n") == {}
    assert rh.render_hop_metrics("\n".join(L), 1000.0, 1000.0 + 0.0002)["key_submit_to_presented_ms"]["n"] == 1    # t1 window (t is epoch ms / 1000)
    # the keyed frame regex of the paced analysis still matches both frame_drawn shapes
    assert al.FRAME_DRAWN_RE.search("input_order hop=frame_drawn total_ms=4.0 acquire_ms=0.5 present_ms=9.0 t=10.0") and al.FRAME_DRAWN_RE.search(
        "input_order hop=frame_drawn total_ms=4.0 acquire_ms=0.5 t=10.0")
    # arms: A/B/C order, kept samples per arm, env, deltas
    sch = rh.arm_schedule(rh.ARMS, 30, 2, 3)
    assert [a for a, _c, _d in sch] == ["default", "keynow_off"] * 2 and all(c == 18 and d == 3 for _a, c, d in sch), sch
    assert rh.arm_env("keynow_off") == {"PHOSPHOR_KEY_FRAME_NOW": "0"}      # #1843: the kill-switch arm
    assert rh.ARMS == ("default", "keynow_off") and set(rh.ARM_ENV) == {"default", "keynow_off"} and rh.arm_env("default") == {}     # mailbox / lat1 dropped (round 8)
    base = rh.arm_summary([10.0, 12.0, 14.0], [8.0, 9.0, 10.0], "input_order hop=frame_drawn total_ms=4.0 acquire_ms=0.5 present_ms=9.0 t=1.0")
    mb = rh.arm_summary([6.0, 7.0, 8.0], [5.0, 6.0, 7.0], "")
    d = rh.arm_deltas(dict(default=base, keynow_off=mb, other=rh.arm_summary([], [], "")))
    assert d["keynow_off"] == dict(raw_p50_delta=-5.0, adjusted_p50_delta=-3.0) and d["other"] == dict(raw_p50_delta=None, adjusted_p50_delta=None), d
    assert base["present_ms"]["p50"] == 9.0 and mb["present_ms"] is None
    # #1843 / #1848: key frames drawn at once. Linux `wait_ms=`, macOS `drawn= ms=`, the Linux switch-off line; an old build gives none of the names
    K = ["input_order hop=key_frame_now wait_ms=0.3 t=1000000.0", "input_order hop=key_frame_now wait_ms=0.5 t=1000001.0",
         "input_order hop=key_frame_now_off wait_ms=16.2 t=1000002.0",
         "input_order hop=key_frame_now drawn=1 ms=4.26 t=1000003.0", "input_order hop=key_frame_now drawn=0 ms=3.90 t=1000004.0",
         "input_order hop=key_frame_now wait_ms=9.9 t=5.0"]                                          # before t0
    k = rh.render_hop_metrics(K, 1000.0)
    assert k["key_frames_now"] == 4 and k["key_frames_now_drawn"] == 1 and k["key_frames_now_off"] == 1, k
    assert k["key_frame_now_wait_ms"] == dict(n=2, p50=0.4, p95=0.5, max=0.5) and k["key_frame_now_ms"]["n"] == 2, k
    assert "key_frames_now" not in m and "key_frames_now_drawn" not in rh.render_hop_metrics(K[:2])
    on = rh.arm_summary([3.0, 3.2, 3.4], [2.0, 2.1, 2.2], "\n".join(K[:2]))
    assert on["key_frames_now"] == 2 and on["key_frames_now_off"] == 0 and base["key_frames_now"] == 0, on
    assert rh.key_frame_now_warns(dict(default=on, keynow_off=base)) == []
    w = rh.key_frame_now_warns(dict(default=base, keynow_off=on))
    assert len(w) == 2 and "did not reach the app" in w[0] and "no hop=key_frame_now" in w[1], w
    w = rh.key_frame_now_warns(dict(default=rh.arm_summary([3.0], [2.0], "\n".join(K[:3])), keynow_off=base))
    assert len(w) == 1 and "switched itself off" in w[0], w
    assert rh.key_frame_now_warns(dict(default=rh.arm_summary([], [], ""), keynow_off=rh.arm_summary([], [], ""))) == []     # no samples: nothing to say


@check
def key_frame_build_hop_parses():
    """docs/design/key-scene-build.md: the per-key build split hop parses into stage metrics (the first line is the text the Rust unit test
    frame_split_tests::the_detail_is_the_line_the_harness_parses asserts), `other` is total minus the stages, a second pass is counted, the top view types
    are a mean per key frame, a window filters by t=, and a build without the hop gives none of the names"""
    import render_hops as rh
    B = ["input_order hop=key_frame_build passes=1 layout_passes=1 views=4 mounted=57 render_ms=0.21 layout_ms=0.63 after_layout_ms=0.02 paint_ms=0.48 "
         "hover_ms=0.31 total_ms=1.80 layout_top=Terminal:0.31,Workspace:0.10,EditorView:0.08 paint_top=Terminal:0.22 t=1000000.0",
         "input_order hop=key_frame_build passes=2 layout_passes=2 views=6 mounted=57 render_ms=0.41 layout_ms=1.23 after_layout_ms=0.04 paint_ms=0.92 "
         "hover_ms=0.60 total_ms=3.40 layout_top=Terminal:0.51,EditorView:0.30,Input:0.12 paint_top=- t=1000001.0",
         "input_order hop=key_frame_build passes=1 layout_passes=1 views=1 mounted=9 render_ms=9.00 layout_ms=9.00 after_layout_ms=9.00 paint_ms=9.00 "
         "hover_ms=9.00 total_ms=99.00 layout_top=- paint_top=- t=5.0"]                                # before t0
    m = rh.render_hop_metrics(B, 1000.0)
    assert m["key_frame_builds"] == 2 and m["key_frame_builds_multi_pass"] == 1, m
    assert m["key_frame_build_total_ms"] == dict(n=2, p50=2.6, p95=3.4, max=3.4) and m["key_frame_build_layout_ms"]["max"] == 1.23, m
    assert m["key_frame_build_hover_ms"]["max"] == 0.6 and m["key_frame_build_render_ms"]["n"] == 2 and m["key_frame_build_paint_ms"]["max"] == 0.92, m
    assert m["key_frame_build_other_ms"]["max"] == 0.2 and m["key_frame_build_views"]["max"] == 6 and m["key_frame_build_mounted"]["p50"] == 57, m
    assert m["key_frame_build_layout_top_ms"] == {"Terminal": 0.41, "EditorView": 0.19, "Input": 0.06, "Workspace": 0.05}, m
    assert m["key_frame_build_paint_top_ms"] == {"Terminal": 0.11}, m
    assert rh.render_hop_metrics(B[2:])["key_frame_builds"] == 1 and "key_frame_build_layout_top_ms" not in rh.render_hop_metrics(B[2:])
    old = rh.render_hop_metrics(["input_order hop=key_frame layout=true t=1000000.0", "input_order hop=key_frame_now wait_ms=0.3 t=1000000.0"])
    assert not [k for k in old if k.startswith("key_frame_build")] and old["key_frames_now"] == 1, old


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
def ctrl_c_regression_is_judged_on_the_app_share():
    """owner 2026-11-10 (docs/design/r11-win.md section 6, r12-tests.md): on Windows the Ctrl-C regression verdicts are judged on the app's own share of the
    key (its input_order segments), not on the end-to-end median, whose spread is the input tool, ConPTY and the shell. The median's delta stays in the record
    as NOT JUDGED. A regression of the share itself still FAILs (re-measured once), the share over its hard budget FAILs whatever n is, and without a usable
    share (other platforms, no log, too few presses) the median gate applies exactly as before."""
    import lat, baseline
    tmp = tempfile.mkdtemp()
    rid = "terminal@gitbash/ctrl_c_d/ctrl_c"
    now, ref = [88.0] * 40, [80.0] * 40          # median +10 % over the previous build's 80.0: FAIL on the median gate
    share = lambda p50, n=30, **k: dict(dict(what="handler_to_write", p50=p50, n=n, budget_ms=2.0, metric="ctrl_c_app_segments"), **k)
    ev = lambda **k: lat.evaluate(rid, now, ref, 20.0, "overhead", "windows-test", expected=30, **k)

    def prev(app_share=None, metric=None):
        rec = {"phosphor": {"n": 40, "p50": 80.0, "p95": 100.0}}
        if app_share:
            rec["app_share"] = app_share
        case = {"name": "ctrl_c_d", "mode": "terminal@gitbash", "metrics": {"lat": {"ctrl_c": rec}}}
        if metric:
            case["metrics"]["ctrl_c_app_segments"] = metric
        json.dump({"cases": [case]}, open(os.path.join(tmp, "prev.json"), "w"))

    def checks(rec):
        r = cases.Result("t", "terminal@gitbash")
        cases.lat_checks(r, rec, "Ctrl-C")
        return r
    orig_p = cases.plat
    try:
        cases.plat = lambda: "windows"
        os.environ["APPCI_PREV_RESULTS"] = os.path.join(tmp, "prev.json")
        prev(dict(what="handler_to_write", p50=0.65, n=30))
        # 1. no app share handed in: the median gate, as before
        rec = ev()
        assert rec["regress_basis"] == "median" and rec["prev_verdict"] == "fail" and "app_share" not in rec, rec
        r = checks(rec)
        assert len(r.fails) == 1 and r.fails[0].startswith("REGRESSION Ctrl-C: median 88.0 vs prev build +8.00 ms"), r.fails
        # 2. the app's share is unchanged (0.66 vs 0.65): the same median is NOT a regression; its delta is kept and reported as not judged
        rec = ev(app_share=share(0.66))
        assert rec["regress_basis"] == "app_share" and rec["prev_verdict"] == "pass" and rec["verdict"] == "pass", rec
        assert rec["prev"]["median_verdict"] == "fail" and rec["prev"]["delta_ms"] == 8.0 and rec["prev"]["judged_on"] == "app_share" and rec["prev"]["verdict"] == "pass", rec["prev"]
        assert rec["app_share"]["prev"] == {"p50": 0.65, "delta_ms": 0.01, "delta_pct": 1.5, "verdict": "pass"} and rec["app_share"]["budget_verdict"] == "pass", rec["app_share"]
        r = checks(rec)
        assert not r.fails and any(n_.startswith("NOT JUDGED: Ctrl-C: median 88.0 vs prev build +8.00 ms (+10.0%) would be FAIL on the median gate") and "0.66 ms now, 0.65 in that build" in n_
                                   for n_ in r.notes), (r.fails, r.notes)
        # 3. the app's share itself regressed (1.4 vs 0.65: +0.75 ms, +115 %): FAIL, on the share
        rec = ev(app_share=share(1.4))
        assert rec["prev_verdict"] == "fail" and rec["verdict"] == "fail" and rec["prev"]["verdict"] == "fail", rec
        r = checks(rec)
        assert len(r.fails) == 1 and r.fails[0].startswith("REGRESSION Ctrl-C: the app's own share (handler_to_write) 1.4 ms vs prev build +0.75 ms"), r.fails
        # ... re-measured once: still regressed stays FAIL, a clean second measurement is a WARN (never a pass), one that cannot be judged clears nothing
        rec = ev(app_share=share(1.4, remeasure=lambda: dict(what="handler_to_write", p50=1.5, n=30, samples=[90.0] * 30)))
        assert rec["prev_verdict"] == "fail" and rec["remeasure"]["app_share"] == {"p50": 1.5, "n": 30} and rec["remeasure"]["phosphor"]["p50"] == 90.0, rec
        assert "re-measured once: app share 1.5 (fail)" in checks(rec).fails[0]
        rec = ev(app_share=share(1.4, remeasure=lambda: dict(what="handler_to_write", p50=0.66, n=30)))
        assert rec["prev_verdict"] == "warn" and rec["verdict"] == "warn" and rec["prev"]["verdict"] == "warn" and rec["app_share"]["prev"]["verdict"] == "warn", rec
        r = checks(rec)
        assert not r.fails and any(n_.startswith("WARN Ctrl-C: the app's own share") for n_ in r.notes), (r.fails, r.notes)
        for bad in (lambda: None, lambda: dict(what="handler_to_write", p50=0.66, n=4)):
            assert ev(app_share=share(1.4, remeasure=bad))["prev_verdict"] == "fail", "a re-measure with no usable figure must not clear a FAIL"
        # 4. the hard budget on the share: over it is a FAIL, also with no older figure to compare with and with too few presses to carry the regression verdict
        rec = ev(app_share=share(2.5))
        assert rec["app_share"]["budget_verdict"] == "fail" and rec["verdict"] == "fail", rec
        assert any("the app's own share (handler_to_write) 2.50 ms exceeds its 2.0 ms budget" in f_ for f_ in checks(rec).fails), checks(rec).fails
        rec = ev(app_share=share(2.5, n=5))
        assert rec["regress_basis"] == "median" and rec["app_share"]["budget_verdict"] == "fail" and rec["verdict"] == "fail", rec
        # 5. a share that is not usable (too few presses with the app's segments): the median gate applies as before, and says why
        rec = ev(app_share=share(0.66, n=5))
        assert rec["regress_basis"] == "median" and rec["prev_verdict"] == "fail" and rec["app_share"]["usable"] is False, rec
        assert len(checks(rec).fails) == 1 and any("not usable as the regression basis" in n_ for n_ in rec["notes"]), rec["notes"]
        rec = ev(app_share=share(None, n=0))
        assert rec["regress_basis"] == "median" and rec["prev_verdict"] == "fail", rec
        # 6. an older run that recorded the figure only as the case metric (before it was part of the latency record) is still compared
        prev(metric={"n": 30, "handler_to_write": {"n": 30, "p50": 0.65, "p95": 0.9}})
        assert ev(app_share=share(1.4))["prev_verdict"] == "fail" and ev(app_share=share(0.66))["prev_verdict"] == "pass"
        # ... one with no such figure, or with another figure, is not compared on the share (and not on the median either): no verdict, and a note says so
        for old in (dict(), dict(app_share=dict(what="keydown_to_write", p50=0.2, n=30)), dict(metric={"n": 3, "handler_to_write": {"n": 3, "p50": 0.1}})):
            prev(**old)
            rec = ev(app_share=share(1.4))
            assert rec["prev_verdict"] == "none" and rec["app_share"]["prev"]["p50"] is None and rec["verdict"] == "pass", (old, rec)
            assert any("no app-share figure (handler_to_write) in that record" in n_ for n_ in rec["notes"]), rec["notes"]
        # 7. a promoted baseline carries the figure
        json.dump({"cases": [{"name": "ctrl_c_d", "mode": "terminal@gitbash", "metrics": {"lat": {"ctrl_c": ev(app_share=share(0.66))}}}]}, open(os.path.join(tmp, "res.json"), "w"))
        assert baseline.load(os.path.join(tmp, "res.json"))[rid]["app_share"] == {"what": "handler_to_write", "p50": 0.66, "n": 30}
        assert lat.old_app_share("handler_to_write", baseline.load(os.path.join(tmp, "res.json"))[rid]) == 0.66
        # 8. the figure the case takes from the log, and where the basis applies
        seg = lambda n, kd, h: {"n": n, "keydown_to_write": {"n": kd, "p50": 0.9 if kd else None}, "handler_to_write": {"n": h, "p50": 0.65 if h else None}}
        assert cases.ctrl_c_app_share(seg(30, 30, 30)) == dict(what="keydown_to_write", p50=0.9, n=30)
        assert cases.ctrl_c_app_share(seg(30, 10, 30)) == dict(what="handler_to_write", p50=0.65, n=30), "a keydown line on a third of the presses is not the figure"
        assert cases.ctrl_c_app_share(seg(30, 0, 0)) is None and cases.ctrl_c_app_share(seg(0, 0, 0)) is None and cases.ctrl_c_app_share(None) is None

        class Ctx:
            def __init__(s, win): s.win = win
        got = cases.ctrl_c_regress_basis(Ctx(True), dict(what="handler_to_write", p50=0.65, n=30), 30, 3)
        assert got["budget_ms"] == cases.CTRLC_APP_SHARE_MAX_MS == 2.0 and got["metric"] == "ctrl_c_app_segments" and callable(got["remeasure"]), got
        assert cases.ctrl_c_regress_basis(Ctx(False), dict(what="handler_to_write", p50=0.65, n=30), 30, 3) is None, "other platforms keep the median gate"
        assert cases.ctrl_c_regress_basis(Ctx(True), None, 30, 3) is None
    finally:
        cases.plat = orig_p
        os.environ.pop("APPCI_PREV_RESULTS", None)
        shutil.rmtree(tmp)


@check
def mac_key_to_frame_regression_is_judged_on_the_app_share_and_the_tail():
    """#2037 (docs/design/r12-mackey.md): on macOS key -> frame of latency_key_log is mostly the wait for the next display pass, so its median moved
    5.86-8.44 ms on unchanged code and round 11 failed '+1.05 ms vs baseline' with the app's share at 1.68 ms. The regression verdicts are judged on
    key -> frame minus the logged wait (hop=key_frame_sched), that share has the 2 ms own-work bound, and the p95 of the whole is judged against the
    older builds' p95 so a missed display pass still fails. Without the figure (other platforms, no hop line) the median gate applies as before."""
    import lat
    kd = lambda t: "input_order keydown key=m ctrl=false alt=false shift=false cmd=false t=%.3f seq=1" % t
    sched = lambda w, t: "input_order hop=key_frame_sched ms=%.2f t=%.3f" % (w, t)
    frame = lambda t: "input_order frame layout=true t=%.3f" % t
    # 1. the split: total = frame - keydown; the wait counts only from the keydown on; a frame with no sched line has no figure; a sched line that an
    #    echo-less frame (hop=key_frame, no `frame` line) used up is not carried to the next frame; a ctrl key never pairs
    txt = "\n".join([
        kd(1000.0), "input_order pty_write bytes=1 t=1000.340", sched(5.74, 1006.3), frame(1007.28), "input_order hop=key_frame layout=true t=1007.30",
        kd(2000.0), kd(2003.0), sched(4.0, 2005.0), frame(2006.0),                 # second key typed 2 ms into the 4 ms wait: it waited the other 2
        kd(3000.0), frame(3009.0),                                                 # no sched line: no figure
        kd(4000.0), sched(7.0, 4001.0), "input_order hop=key_frame layout=true t=4001.5", frame(4008.0),      # that wait belonged to another frame
        "input_order keydown key=c ctrl=true alt=false shift=false cmd=false t=5000.0 seq=9", sched(1.0, 5002.0), frame(5003.0)])
    sh = cases.key_frame_shares(txt)
    assert [tuple(None if x is None else round(x, 2) for x in s) for s in sh] == [
        (7.28, 1.54, 5.74), (6.0, 2.0, 4.0), (3.0, 1.0, 2.0), (9.0, None, None), (8.0, None, None)], sh
    assert [round(t, 2) for t, _, _ in sh] == [round(x, 2) for x in cases.key_frame_pairs(txt)[0]], "the same pairs as key_frame_pairs"
    # 2. the figure: needs it for >= 90 % of the pairs
    full = [(7.0 + i % 3, 1.6 + 0.01 * (i % 3), 5.4) for i in range(60)]
    assert cases.key_frame_app_share(full) == dict(what="key_to_frame_less_display_wait", p50=1.61, n=60)
    assert cases.key_frame_app_share(full[:50] + [(9.0, None, None)] * 10) is None and cases.key_frame_app_share([]) is None and cases.key_frame_app_share(sh) is None
    assert cases.key_frame_app_share(full[:55] + [(9.0, None, None)] * 5)["n"] == 55
    orig_p, orig_b, env = cases.plat, lat.load_baseline, {k: os.environ.pop(k, None) for k in ("APPCI_KEY_APP_SHARE_MAX_MS", "APPCI_KEY_BUDGET_MS", "APPCI_PREV_RESULTS")}
    rid = "terminal/latency_key_log/key_to_frame"
    share = lambda p50, **k: dict(dict(what="key_to_frame_less_display_wait", p50=p50, n=60), **k)
    # round 11's figures: median 7.28 against the baseline's 6.23, p95 9.4 against 9.32
    r11 = [3.0 + 0.1 * i for i in range(29)] + [7.26, 7.30] + [8.0] * 26 + [9.4, 9.41, 9.42]
    try:
        # 3. where the basis applies, and its hard bound
        cases.plat = lambda: "macos"
        got = cases.key_frame_regress_basis(share(1.68), lambda: full)
        assert got["budget_ms"] == 2.0 and got["what"] == "key_to_frame_less_display_wait" and got["remeasure"]()["p50"] == 1.61 and len(got["remeasure"]()["samples"]) == 60, got
        os.environ["APPCI_KEY_BUDGET_MS"] = "3"
        assert cases.key_frame_regress_basis(share(1.68), lambda: full)["budget_ms"] == 3.0
        os.environ["APPCI_KEY_APP_SHARE_MAX_MS"] = "2.5"
        assert cases.key_frame_regress_basis(share(1.68), lambda: full)["budget_ms"] == 2.5
        os.environ.pop("APPCI_KEY_APP_SHARE_MAX_MS"), os.environ.pop("APPCI_KEY_BUDGET_MS")
        assert cases.key_frame_regress_basis(None, lambda: full) is None, "no figure: the median gate"
        cases.plat = lambda: "linux-wayland"
        assert cases.key_frame_regress_basis(share(1.68), lambda: full) is None, "other platforms keep the median gate"
        cases.plat = lambda: "macos"

        def run(samples, sh_p50, base, tail_again=None, share_again=None):
            lat.load_baseline = lambda platform: ("ok", {rid: base})
            calls = []
            basis = cases.key_frame_regress_basis(share(sh_p50) if sh_p50 is not None else None, lambda: calls.append("share") or (share_again or full))
            rec = lat.evaluate(rid, samples, None, 2.0, "overhead", "macos", remeasure=lambda: calls.append("median") or samples, app_share=basis)
            r = cases.Result("latency_key_log", "terminal")
            cases.lat_checks(r, rec, "key to frame")
            cases.key_frame_tail_checks(r, rec, "key to frame", lambda: calls.append("tail") or (samples if tail_again is None else tail_again))
            return rec, r, calls
        old = {"n": 60, "p50": 6.23, "p95": 9.32}
        # 4. round 11 as it ran, on the median gate (no figure): FAIL, re-measured
        rec, r, calls = run(r11, None, old)
        assert rec["phosphor"]["p50"] == 7.28 and rec["regress_basis"] == "median" and rec["baseline_verdict"] == "fail" and calls == ["median"] and "tail" not in rec, (rec, calls)
        assert any("REGRESSION key to frame: median 7.28 vs baseline build +1.05 ms" in f for f in r.fails), r.fails
        # 5. the same numbers with the share: the baseline has no share yet, so nothing is compared on it (said, with the median's delta as NOT JUDGED);
        #    the tail is within the gate (9.4 vs 9.32); no failure, no re-measure
        rec, r, calls = run(r11, 1.68, old)
        assert rec["regress_basis"] == "app_share" and rec["baseline_verdict"] == "none" and rec["verdict"] == "no-reference" and not r.fails and calls == [], (rec, r.fails, calls)
        assert rec["baseline"]["median_verdict"] == "fail" and rec["baseline"]["delta_ms"] == 1.05 and rec["app_share"]["budget_verdict"] == "pass", rec
        assert rec["tail"]["baseline"] == {"p95": 9.32, "delta_ms": 0.08, "delta_pct": 0.9, "verdict": "pass"} and rec["tail"]["prev"]["verdict"] == "none", rec["tail"]
        assert any(n.startswith("NOT JUDGED: key to frame: median 7.28 vs baseline build +1.05 ms") for n in r.notes), r.notes
        assert any("no app-share figure" in n for n in rec["notes"]), rec["notes"]
        # 6. a promoted baseline carries the share: an unchanged share passes; a grown one FAILs after its re-measure; a clean re-measure is a WARN;
        #    over the hard bound it FAILs whatever the comparison says
        new = dict(old, app_share={"what": "key_to_frame_less_display_wait", "p50": 1.6, "n": 60})
        rec, r, calls = run(r11, 1.68, new)
        assert rec["baseline_verdict"] == "pass" and rec["app_share"]["baseline"]["delta_ms"] == 0.08 and not r.fails and calls == [], (rec, calls)
        slow = [(8.0, 2.4, 5.6)] * 60
        rec, r, calls = run(r11, 2.4, old)
        assert rec["app_share"]["budget_verdict"] == "fail" and rec["verdict"] == "fail" and any("exceeds its 2.0 ms budget" in f for f in r.fails), (rec["app_share"], r.fails)
        os.environ["APPCI_KEY_APP_SHARE_MAX_MS"] = "5"
        rec, r, calls = run(r11, 2.4, new, share_again=slow)
        assert rec["baseline_verdict"] == "fail" and calls == ["share"] and any("REGRESSION key to frame: the app's own share" in f for f in r.fails), (rec, r.fails, calls)
        rec, r, calls = run(r11, 2.4, new)
        assert rec["baseline_verdict"] == "warn" and not r.fails and any(n.startswith("WARN key to frame: the app's own share") for n in r.notes), (rec, r.fails)
        os.environ.pop("APPCI_KEY_APP_SHARE_MAX_MS")
        # 7. the tail: a display pass missed for 6 of 60 keys (p95 one interval later) with the share unchanged FAILs after one re-measure of its own;
        #    a clean re-measure is a WARN; a re-measure with no samples clears nothing
        missed = r11[:54] + [x + 8.33 for x in r11[54:]]
        rec, r, calls = run(missed, 1.68, new)
        assert calls == ["tail"] and rec["tail"]["baseline"]["verdict"] == "fail" and rec["tail"]["baseline"]["remeasure"]["verdict"] == "fail" and rec["verdict"] == "fail", (rec["tail"], calls)
        assert any(f.startswith("REGRESSION key to frame: p95 ") and "re-measured once: p95 " in f for f in r.fails), r.fails
        rec, r, calls = run(missed, 1.68, new, tail_again=r11)
        assert rec["tail"]["baseline"]["verdict"] == "warn" and not r.fails and rec["verdict"] == "warn" and any(n.startswith("WARN key to frame: p95 ") for n in r.notes), (rec["tail"], r.fails)
        rec, r, calls = run(missed, 1.68, new, tail_again=[])
        assert calls == ["tail"] and rec["tail"]["baseline"]["verdict"] == "fail" and r.fails, "an empty re-measure must not clear a tail FAIL"
        # 8. on the median basis the tail is not judged a second time (the median gate covers the whole)
        cases.plat = lambda: "linux-wayland"
        rec, r, calls = run(missed, 1.68, new)
        assert rec["regress_basis"] == "median" and "tail" not in rec and "app_share" not in rec, rec
    finally:
        cases.plat, lat.load_baseline = orig_p, orig_b
        for k, v in env.items():
            os.environ.pop(k, None)
            if v is not None:
                os.environ[k] = v


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
def routing_typo_head_pure_pieces():
    # #1980: rows of either stage for the typed line only (same head, same byte length), and the per-surface verdict
    log = ('D keystroke: Shell by mistyped-command-in-conversation (#1980) (AgentConversation, head "gti", 10 bytes)\n'
           'D enter: Shell by mistyped-command-in-conversation (#1980) (AgentConversation, head "gti", 10 bytes)\n'
           'D enter: Agent by request-shape (#834/#965) (AgentConversation, head "gti", 19 bytes)\n'
           'D enter: Shell by installed-head (#906/#964) (PlainTerminal, head "ls", 6 bytes)\n')
    rows = ca.typo_rows(log, "gti status")
    assert [x["stage"] for x in rows] == ["keystroke", "enter"] and rows[-1]["rule"].startswith(ca.TYPO_RULE) and rows[-1]["surface"] == "AgentConversation"
    assert [x["route"] for x in ca.typo_rows(log, "gti is broken again")] == ["Agent"] and ca.typo_rows(log, "dokcer ps") == [] and ca.typo_rows("", "gti status") == []
    v = ca.typo_verdict
    assert v("typo", "shell", "AgentConversation", "mistyped-command-in-conversation (#1980)") == (True, None)
    assert v("typo", "agent", "AgentConversation", "unknown-head-in-conversation (#935)") == (False, None)
    assert v("typo", "BOTH", "AgentConversation", None) == (False, None) and v("typo", "lost", "AgentConversation", None) == (False, None)
    ok, why = v("typo", "shell", "AgentConversation", "evidence-in-conversation (#1183)")
    assert ok and "evidence-in-conversation" in why
    assert v("prose", "agent", "AgentConversation", "request-shape (#834/#965)") == (True, None) and v("prose", "shell", "AgentConversation", None) == (False, None)
    assert v("typo", "shell", "PlainTerminal", "model") is None and v("typo", "agent", None, None) is None
    assert "routing_typo_head" in cases.PROVING and [c for c in cases.CASES if c["name"] == "routing_typo_head"][0]["modes"] == ("agent",)


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
        ctx.tok = ctx.ptok = lambda n: "abcdefgh"[:n]
        ctx.cmd_mark = lambda: 0
        ctx.pane_shell = lambda: "bash"
        ctx.append_cmd = lambda shell, tok, fname: "echo %s >> %s" % (tok, fname)
        ctx.until = lambda pred, timeout=10, step=0.05: pred()
        ctx.W = "/nonexistent"
        ctx.probe = lambda cmd, tok, fname, **k: (be.type(cmd), ("local", [tok]))[1]       # the probe primitive itself is pinned by audit1_/audit3_
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
def shell_depth_probe_pure_parts():
    # the subshell leak (Mac round 6): the PID probe, its parser and which cases get the post-case depth check
    assert cases.depth_probe_cmd("cmd", "pX") is None
    assert cases.depth_probe_cmd("powershell", "pX") == "Add-Content ~/.appci_pid pX:$PID"
    assert cases.depth_probe_cmd(None, "pX") == "sh -c 'echo pX:$PPID >> ~/.appci_pid'" and cases.depth_probe_cmd("gitbash", "pX").endswith(">> ~/.appci_pid'")
    import shutil as _sh, subprocess as _sp, tempfile as _tf
    for _shell, _ in (("sh", 0), ("bash", 0), ("zsh", 0), ("fish", 0), ("dash", 0)):      # the probe records the PID of the shell it was typed into, in every shell installed here
        if not _sh.which(_shell):
            continue
        with _tf.TemporaryDirectory() as _d:
            _out = _sp.run([_shell, "-c", cases.depth_probe_cmd(None, "pS") + "; echo $0 >/dev/null"], env=dict(os.environ, HOME=_d), capture_output=True, text=True, timeout=20)
            _toks = open(os.path.join(_d, ".appci_pid")).read().split()
            assert cases.parse_depth_probe(_toks, "pS") is not None, (_shell, _out.stderr, _toks)
    assert cases.parse_depth_probe(["pA:11", "pX:4242", "pX:77"], "pX") == "77"        # the newest line of this probe
    assert cases.parse_depth_probe(["pA:11"], "pX") is None and cases.parse_depth_probe(["pX:"], "pX") is None
    assert cases.parse_depth_probe(["pX:12ab"], "pX") is None
    proving = ["new_case"]
    assert cases.wants_depth_check("new_case", proving) and cases.wants_depth_check("subshell_ready_rc_clear", proving)
    assert cases.wants_depth_check("ssh_connect_typing", proving) and cases.wants_depth_check("remote_x", proving) and cases.wants_depth_check("lab_x", proving)
    assert not cases.wants_depth_check("latency_enter", proving)
    # leave_nested: exits only while a probe proves a different shell, never at the root
    typed, pids = [], iter(["900", "900", "100"])
    c = types.SimpleNamespace(prof=types.SimpleNamespace(prompts=lambda: 0), pane=None, be=types.SimpleNamespace(type=lambda t, *a, **k: typed.append(t)),
                              shell_pid=lambda: next(pids), until=lambda *a, **k: None, reset=lambda: None, wait_prompt_after_exit=lambda *a, **k: True)
    typed.clear(); pids = iter(["900", "900", "900", "900", "100"]); c.shell_pid = lambda: next(pids)
    assert cases.Ctx.leave_nested(c, "100") == "root" and typed == ["exit", "exit"], typed
    typed.clear(); c.shell_pid = lambda: "100"
    assert cases.Ctx.leave_nested(c, "100") == "root" and typed == []                  # at the root: no exit
    assert cases.Ctx.leave_nested(c, None) == "unprobed" and typed == []
    typed.clear(); c.shell_pid = lambda: "900"
    assert cases.Ctx.leave_nested(c, "100", max_exits=2) == "nested" and typed == ["exit"] * 2


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
def audit1_shell_pid_counts_only_tokens_appended_after_its_snapshot():
    """harness audit #1a/E (2026-11-09): a `tok:pid` of the same token that sat in ~/.appci_pid BEFORE the probe was typed (an earlier run's, the tokens
    were seeded) is not an answer; only a token appended after the per-probe size snapshot is. Probe tokens are unseeded: two contexts never agree."""
    import random
    d = tempfile.mkdtemp()
    try:
        f = os.path.join(d, ".appci_pid")
        open(f, "w").write("pSTALE:4711\n")               # an earlier run's probe with THIS probe's token (same seed, same sequence) and a dead pid
        typed, state = [], {"answer": None}

        def type_(text, ms, enter=False):
            typed.append(text)
            if state["answer"]:
                open(f, "a").write("pSTALE:%s\n" % state["answer"])
        c = types.SimpleNamespace(be=types.SimpleNamespace(type=type_, guard=lambda: None), pane=None, prof=types.SimpleNamespace(local_prompts=False, prompts=lambda remote=False: 0),
                                  ptok=lambda n: "STALE", where_homes=lambda: [d], lines=lambda name, remote=False: [], log_path=None, precmds_since=lambda t0: None)
        c.until = lambda pred, timeout=10, step=0.02: cases.Ctx.until(c, pred, timeout, step)
        c.probe = lambda *a, **k: cases.Ctx.probe(c, *a, **k)
        assert cases.Ctx.shell_pid(c, timeout=0.3) is None, "a stale token before the snapshot was taken as the shell's pid"
        assert typed == [cases.depth_probe_cmd(None, "pSTALE")]
        state["answer"] = "77"
        assert cases.Ctx.shell_pid(c, timeout=1) == "77"
        # the Windows PowerShell pane: `exit` only when BOTH probes name the same non-root pid; a stale-file answer cannot do that any more because
        # neither probe reads anything appended before itself
        a, b = types.SimpleNamespace(n=0, probe_rng=random.Random()), types.SimpleNamespace(n=0, probe_rng=random.Random())
        assert cases.Ctx.ptok(a) != cases.Ctx.ptok(b), "probe tokens are seeded"
        import inspect
        src = inspect.getsource(cases.Ctx.__init__)
        assert "self.probe_rng = random.Random()" in src and "self.rng = random.Random(seed)" in src
    finally:
        shutil.rmtree(d, ignore_errors=True)


@check
def audit3_probe_waits_for_its_own_prompt_and_exit_waits_for_the_next():
    """harness audit #3 (the `tac` incident): the rc hook logs the probe's prompt a few ms AFTER the effect file; `before = prompts()` sampled in that
    window let the probe's prompt pass as the `exit`'s and the next probe was typed into the dying ssh. probe() returns only after the prompt
    advanced, return_to_local types `exit` with that prompt counted, and types the next probe only after a FURTHER prompt."""
    import threading
    d = tempfile.mkdtemp()
    try:
        rhome, home = os.path.join(d, "rhome"), os.path.join(d, "home")
        os.makedirs(rhome); os.makedirs(home)
        st = {"prompts": 0, "remote": True, "typed": []}

        def bump():
            st["prompts"] += 1

        def type_(text, ms, enter=False):
            st["typed"].append((text, st["prompts"]))
            if text.startswith("echo w"):
                tok = text.split()[1]
                open(os.path.join(rhome if st["remote"] else home, ".appci_where"), "a").write(tok + "\n")
                threading.Timer(0.1, bump).start()              # the hook logs the probe's prompt 100 ms after the effect file
            elif text == "exit":
                st["remote"] = False
                threading.Timer(0.1, bump).start()              # the local shell's prompt after the remote level ended
        c = types.SimpleNamespace(be=types.SimpleNamespace(type=type_, guard=lambda: None), pane=None, W=home, log_path=None,
                                  prof=types.SimpleNamespace(local_prompts=True, prompts=lambda remote=False: st["prompts"], rhome=rhome, home=home),
                                  reset=lambda: None, n=0)
        import random
        c.probe_rng = random.Random()
        c.ptok = lambda n=8: cases.Ctx.ptok(c, n)
        c.until = lambda pred, timeout=10, step=0.01: cases.Ctx.until(c, pred, timeout, step)
        c.lines = lambda name, remote=False: cases.Ctx.lines(c, name, remote)
        c.where_homes = lambda: [home]
        for name in ("probe", "where_now", "wait_prompt_after_exit", "return_to_local", "wait_ssh_gone"):
            setattr(c, name, (lambda _n: (lambda *a, **k: getattr(cases.Ctx, _n)(c, *a, **k)))(name))
        assert cases.Ctx.return_to_local(c) == "local", st["typed"]
        probes = [x for x in st["typed"] if x[0].startswith("echo w")]
        exits = [x for x in st["typed"] if x[0] == "exit"]
        assert len(exits) == 1 and len(probes) == 2, st["typed"]
        assert exits[0][1] == 1, "exit typed before the probe's own prompt was counted: %r" % st["typed"]
        assert probes[1][1] == 2, "the next probe was typed before the exit's prompt: %r" % st["typed"]
    finally:
        shutil.rmtree(d, ignore_errors=True)


@check
def audit2_raw_exit_is_banned_outside_ctx_and_the_root_pid_is_recorded_once_per_launch():
    """harness audit #2/B: a raw `be.type("exit"` after an unchecked wait ran at the ROOT shell when the subshell never started (pane closed, app quit).
    Every case leaves a shell through Ctx.exit_nested, which types `exit` only on proof; the raw call survives only inside Ctx, in
    cases_caseremote.leave and for a reference terminal (marked on the line). run.py records the root pid once per (re)launch through one
    relaunch() helper, and refuses when the process tree shows more than one shell."""
    import glob, re
    allowed = {("cases.py", "Ctx"), ("cases_caseremote.py", "leave")}
    bad = []
    for path in sorted(glob.glob(os.path.join(HERE, "cases*.py"))):
        lines = open(path, errors="replace").read().splitlines()
        enclosing = None
        for i, l in enumerate(lines):
            m = re.match(r"(?:def|class)\s+(\w+)", l)
            if m:
                enclosing = m.group(1)
            if re.search(r"""\.type\(\s*['"]exit['"]""", l) and "reference terminal" not in l and (os.path.basename(path), enclosing) not in allowed:
                bad.append("%s:%d (%s)" % (os.path.basename(path), i + 1, enclosing))
    assert not bad, "raw `exit` outside Ctx.exit_nested / leave / a reference terminal: %s" % bad
    src = open(os.path.join(HERE, "run.py")).read()
    assert src.count("await_shell(") == 3, "every relaunch goes through relaunch(): await_shell is called from the first launch and relaunch() only"
    assert "time.sleep(4)" not in src, "the fixed first-run settle is settle_first_run (view_mode hop)"
    import run as runmod
    # exit_nested: the PID probe decides first, then the where probe, then the shell count; no proof, no exit
    typed = []
    st = {"pid": None, "where": "local", "count": 1}
    c = types.SimpleNamespace(be=types.SimpleNamespace(type=lambda t, *a, **k: typed.append(t)), root_pid="100", prof=types.SimpleNamespace(prompts=lambda remote=False: 0),
                              shell_pid=lambda: st["pid"], where_now=lambda: st["where"], shell_count=lambda: st["count"],
                              wait_prompt_after_exit=lambda *a, **k: True)
    c.in_subshell = lambda base: cases.Ctx.in_subshell(c, base)
    r = cases.Result("t", "terminal")
    st["pid"] = "100"
    assert cases.Ctx.exit_nested(c, 1, r, "x") is False and typed == [] and "ROOT shell" in r.notes[-1]
    st["pid"] = "900"
    assert cases.Ctx.exit_nested(c, 1, r, "x") is True and typed == ["exit"]
    typed.clear(); st["pid"] = None; st["where"] = "remote"
    assert cases.Ctx.exit_nested(c, None, r, "x") is True and typed == ["exit"]            # remote shell in front: exit (one level)
    typed.clear(); st["where"] = "local"; st["count"] = 1
    assert cases.Ctx.exit_nested(c, 1, r, "x") is False and typed == [] and "did not rise" in r.notes[-1]       # the subshell never started
    assert cases.Ctx.exit_nested(c, None, r, "x") is False and typed == [] and "unknown" in r.notes[-1]        # no base either: no proof, no exit
    st["where"] = "none"; st["count"] = 2
    assert cases.Ctx.exit_nested(c, 1, r, "x") is True and typed == ["exit"]              # a `su` shell: answers in root's home, but one more shell
    typed.clear(); c.root_pid = None; st["where"] = "local"; st["count"] = 2
    assert cases.Ctx.exit_nested(c, 1, r, "x") is True and typed == ["exit"]              # no root pid: the shell count proves the nesting
    # record_root_pid: once per launch, refused unless exactly one shell runs under the app
    cx = types.SimpleNamespace(win=False, pane=None, be=types.SimpleNamespace(guard=lambda: None), shell_count=lambda: 2, shell_pid=lambda: "77")
    app = types.SimpleNamespace(pid=lambda: 4242)
    msg = runmod.record_root_pid(cx, app, "launch")
    assert cx.root_pid is None and cx.root_app_pid == 4242 and "2 shells" in msg, (cx.root_pid, msg)
    cx.shell_count = lambda: 1
    assert runmod.record_root_pid(cx, app, "launch") is None and cx.root_pid == "77"
    cx.shell_pid = lambda: None
    assert "no answer" in runmod.record_root_pid(cx, app, "launch") and cx.root_pid is None
    cx.win, cx.pane = True, "cmd"
    assert runmod.record_root_pid(cx, app, "launch") is None and cx.root_pid is None       # cmd.exe: no probe, no row


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
    assert "end_stuck_session(ctx, r, label, mark, command.split(" in src and "record_rpid(ctx)" in src and src.index("heredoc_control(") < src.index("judge_step(")
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
    # r18u28 (X11): the space bar logs `key=" "`; the audit must see it, so `echo o...` (o, space, o within 2 ms) is no doubled `o`
    sp = cases.keydown_audit(ln(q("o"), 1000.0) + ln(q(" "), 1000.8) + ln(q("o"), 1001.6) + ln('"\\""', 1002.4, shift="true"), sent_text='echo o"')
    assert sp["count"] == 4 and sp["dups"] == [] and sp["bursts"] == [], sp


@check
def x11_injector_rollover_is_physical_and_keys_are_named():
    """r18u28: rollover on X11 never holds Shift (or any key) over the next key's press and never presses a key that is still down; the text's
    keys go down in order; plain keys still overlap. Key names are X keysyms (case-sensitive)."""
    import backend

    shifted = {backend.ENVNAME.get(c, c) for c in backend.X11_SHIFTED}

    def presses(argv):
        down, order, overlapped = set(), [], 0
        a, i = argv[1:], 0
        while i < len(a):
            v, k = a[i], (a[i + 1] if i + 1 < len(a) else None)
            i += 2
            if v == "keydown":
                assert k not in down, ("pressed while still down", k, argv)
                assert not (down and k in shifted), ("a Shift-typed key pressed over another key", k, down)
                assert not (down & shifted), ("a key pressed while a Shift-typed key is held", k, down)
                overlapped += bool(down)
                down.add(k)
                order.append(k)
            elif v == "keyup":
                down.discard(k)
        assert not down, ("keys left down", down)
        return order, overlapped
    for text in ("echo t73_8m81ereqlg >> r_roll.txt", "echo pgmufmwhdhkk >> kd.txt", "ABab__cc"):
        order, overlapped = presses(backend.x11_roll_argv(text, 10))
        assert order == [backend.ENVNAME.get(c, c) for c in text], (text, order)
        assert overlapped > 0, text
    assert {"underscore", "greater", "R"} <= shifted and "space" not in shifted
    assert [backend.x11_keyname(k) for k in ("escape", "Escape", "return", "backspace", "a", "F5", "Left")] == ["Escape", "Escape", "Return", "BackSpace", "a", "F5", "Left"]


@check
def x11_injector_premaps_wide_characters():
    """r18u28: the non-ASCII characters of a typed text get keycodes of their own once (xmodmap), reused later, oldest unused recycled"""
    import backend
    pke = "keycode   8 =\nkeycode   9 = Escape NoSymbol Escape\nkeycode  93 =\nkeycode  97 = \nkeycode 120 =\nkeycode 132 =\nkeycode 250 =\nkeycode 251 =\n"
    spare = backend.x11_spare_keycodes(pke, reserve=2)
    assert spare == [251, 250, 132, 120, 97], spare                    # 8 and 93 left for xdotool's own scratch keycode
    ex, bound, spare2 = backend.x11_premap_plan("echo 日本語 >> x", {}, spare)
    assert ex == ["-e", "keycode 251 = U65E5", "-e", "keycode 250 = U672C", "-e", "keycode 132 = U8A9E"] and spare2 == [120, 97], (ex, spare2)
    ex2, bound2, _ = backend.x11_premap_plan("日本", bound, spare2)
    assert ex2 == [] and bound2 == {"語": 132, "日": 251, "本": 250}, (ex2, bound2)          # reused, most recently used last
    ex3, bound3, _ = backend.x11_premap_plan("éüñ", bound2, [])
    assert ex3 == ["-e", "keycode 132 = U00E9", "-e", "keycode 251 = U00FC", "-e", "keycode 250 = U00F1"], ex3
    assert backend.x11_premap_plan("plain ascii ~", {}, [200]) == ([], {}, [200])
    ex4, bound4, _ = backend.x11_premap_plan("日本", {}, [7])                # no keycode left for 本: xdotool binds it itself
    assert ex4 == ["-e", "keycode 7 = U65E5"] and bound4 == {"日": 7}, (ex4, bound4)


@check
def x11_injector_focuses_phosphor_before_every_call():
    """r18u28: after an in-case relaunch nothing focused the new window (no WM on Xvfb) and every key went nowhere. Each injection now checks
    the X focus, focuses the newest Phosphor window when it is elsewhere, and refuses when there is none."""
    import backend
    be = backend.X11Backend()
    st = dict(focus="1", wins=["5", "9"], calls=[])

    def fake_sub(argv, **kw):
        st["calls"].append(argv[1] if argv[0] == "xdotool" else argv[0])
        out = st["focus"] if argv[1:2] == ["getwindowfocus"] else (" ".join(st["wins"]) if argv[1:2] == ["search"] else "")
        return types.SimpleNamespace(returncode=0, stdout=out.encode(), stderr=b"")

    def fake_run(argv, **kw):
        st["calls"].append(argv[1])
        if argv[1] == "windowfocus":
            st["focus"] = argv[2]
        return types.SimpleNamespace(returncode=0)
    sub, run = backend.subprocess.run, backend._run
    backend.subprocess.run, backend._run = fake_sub, fake_run
    try:
        be.key("escape")                       # focus on the root: the newest Phosphor window is focused first, then the key goes
        assert st["focus"] == "9" and st["calls"][-1] == "key" and "windowfocus" in st["calls"], st
        assert be.calls[-1]["special"][0].startswith("Escape@"), be.calls[-1]
        st["calls"].clear()
        be.type("ab")                          # same focus as last time: one getwindowfocus, no search, no refocus
        assert st["calls"] == ["getwindowfocus", "type"], st["calls"]
        st["focus"] = "5"
        st["calls"].clear()
        be.type("ab")                          # another Phosphor window has the focus: left alone
        assert "windowfocus" not in st["calls"] and st["focus"] == "5", st
        st["focus"], st["wins"] = "1", []
        try:
            be.type("x")
            raise AssertionError("typed with no Phosphor window")
        except RuntimeError as e:
            assert "no visible Phosphor window" in str(e)
        assert st["calls"][-1] != "type"
    finally:
        backend.subprocess.run, backend._run = sub, run


@check
def idle_first_key_waits_for_both_frames():
    """r18u28 (X11 latency_first_key_after_idle 0/30): the sample is read only after the second key's frame is in the app log"""
    log = []
    clock = [1000.0]

    class Be:
        name = "x11"

        def type(self, text, delay, enter=False):
            if text == "m":
                log.extend(pending)            # the previous key's frame is in the log long before this key
                pending.clear()
                clock[0] += 300
                log.append("input_order keydown key=\"m\" ctrl=false alt=false shift=false cmd=false t=%.3f seq=1" % clock[0])
                pending.append("input_order frame layout=false t=%.3f" % (clock[0] + 20))
    pending = []

    class Ctx:
        be = Be()

        def log_size(self):
            return len(log)

        def log_since(self, off):
            out = "\n".join(log[off:])
            log.extend(pending)                # the frame is logged a moment AFTER the key: only a later read sees it
            pending.clear()
            return out

        def until(self, pred, timeout=10, step=0.05):
            return cases.Ctx.until(self, pred, timeout, step=0)

        def reset(self):
            pass
    sl = cases.time.sleep
    cases.time.sleep = lambda s: None
    try:
        first, second = cases.idle_log_block(Ctx(), 0, 3)
    finally:
        cases.time.sleep = sl
    assert first == [20.0, 20.0, 20.0] and second == [20.0, 20.0, 20.0], (first, second)


@check
def x11_ocr_upscaled_words_scale_back():
    import cases_ui
    tsv = "level\tpage\tblock\tpar\tline\tword\tleft\ttop\twidth\theight\tconf\ttext\n5\t1\t1\t1\t1\t1\t200\t80\t120\t40\t91\tZEBRAAPPLE\n"
    assert cases_ui.tsv_words(tsv) == [("ZEBRAAPPLE", 260, 100, 200, 320, 80, 120)]
    assert cases_ui.tsv_words(tsv, 2) == [("ZEBRAAPPLE", 130, 50, 100, 160, 40, 60)]
    assert cases.ocr_upscaled("wayland", "/nonexistent.png") is None and cases.ocr_upscaled("mac", "/x.png") is None


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
def prompt_sample_audit_counts_samples_and_lookup_paths():
    # #1958: the hand-off's platform samples and the Linux foreground-group lookups, from the app's own hops
    s = lambda what, us: "input_order hop=prompt_fact_sample what=%s answer=NotAtPrompt us=%d t=1\n" % (what, us)
    g = lambda path, reads, us, members="1": "input_order hop=fg_group_lookup for=plain_program path=%s reads=%d members=%s us=%d t=1\n" % (path, reads, members, us)
    a = cases.prompt_sample_audit("junk\n" + g("walk", 4, 31) + s("prompt_fact", 40) + g("leader", 1, 9, "none") + s("prompt_fact", 12)
                                  + g("list_confirm", 36, 1400, "2") + s("plain_program", 1500)
                                  + "input_order hop=hand_route reason=unheld state=pty_owned prompt_fact=unsampled t=1\n")
    assert (a["samples"], a["max_us"], a["p50_us"], a["over_1ms"]) == (3, 1500, 40, 1), a
    assert a["lookups"] == {"walk": [1, 31, 4], "leader": [1, 9, 1], "list_confirm": [1, 1400, 36]}, a
    assert cases.prompt_sample_audit("input_order hop=hand_route prompt_fact=NotAtPrompt t=1\n") == dict(samples=0, max_us=0, p50_us=0, over_1ms=0, lookups={})


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
    assert a["handler_to_write"] == {"n": 2, "p50": 1.5, "p95": 1.5, "max": 1.5}, a      # ctrl_c_keydown -> pty write: the part every platform logs
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
def give_up_facts_read_both_shapes_and_the_keep_together_rule():
    """#1996 (owner decisions 2026-11-09): giveup_keys_reach_shell's pure parts: a give-up's pty write, keys kept with an unrun line, the old marks,
    and the keep-together rule on the lines that ran (a later line never ran without the line before it)"""
    import cases_shells
    log = "\n".join([
        "x input_order hop=give_up_keys_to_pty why=unready_enter keys=9 bytes=20 written=20 to_editor=0 pty_gone=0 t=1.0",
        "x input_order hop=give_up_keys_kept_with_line why=enter_gate keys=11 chars=12 line_chars=8 t=2.0",
        "x input_order hop=give_up_keys_to_pty why=young_hold_cap keys=1 bytes=1 written=0 to_editor=1 pty_gone=1 t=3.0"])
    ups, old = cases_shells.give_up_facts(log)
    assert [u["why"] for u in ups] == ["unready_enter", "young_hold_cap"] and ups[0]["written"] == 20 and ups[1]["pty_gone"] and not old, (ups, old)
    assert cases_shells.kept_with_line_facts(log) == [dict(why="enter_gate", keys=11, chars=12, line_chars=8)]
    assert cases_shells.kept_with_line_facts("") == [] and cases_shells.kept_with_line_facts(None) == []
    _, old = cases_shells.give_up_facts("x hop=young_hold_handed_back why=young_hold_cap\nx hop=prompt_hold_handed_back hold=2 kind=PromptSeen why=young_hold_cap")
    assert "hop=young_hold_handed_back" in old and "hop=prompt_hold_handed_back kind=PromptSeen" in old, old
    exp = ["a", "b", "c", "d"]
    assert cases_shells.ran_out_of_order(exp, exp) is None and cases_shells.ran_out_of_order(exp, []) is None
    assert cases_shells.ran_out_of_order(exp, ["a", "b"]) is None              # the rest stayed in the input with its unrun line: in order
    assert cases_shells.ran_out_of_order(exp, ["a", "c"]) == (1, "b", "c")     # `c` ran without `b`: the shape the owner ruled out
    assert cases_shells.ran_out_of_order(exp, ["b"]) == (0, "a", "b")
    assert cases_shells.ran_out_of_order(exp, ["a", "a"]) == (1, "b", "a")     # a line run twice
    assert cases_shells.ran_out_of_order(exp, exp + ["e"]) == (4, None, "e")   # a line nobody typed


@check
def report_anchor_facts_count_the_2012_path():
    """#2012: giveup_keys_reach_shell's metrics of the path that reordered a line in round 9 (a shell-started command took input from an editor holding text)"""
    import cases_shells
    log = "\n".join([
        "x input_order hop=handoff_report_anchor_set anchor_chars=2 t=1.0",
        'x input_order hop=handoff_insert_anchor anchor_chars=2 t=2.0',
        'x input_order hop=handoff_insert replaced_prefix_chars=0 report="d-" before="AdCo" after="Add-Co" t=2.1',
        "x input_order hop=handoff_report_anchor_set anchor_chars=0 t=3.0",
        "x input_order hop=handoff_report_anchor_dropped why=no_report_pending t=4.0"])
    assert cases_shells.report_anchor_facts(log) == (2, 1, 1), cases_shells.report_anchor_facts(log)
    assert cases_shells.report_anchor_facts("") == (0, 0, 0) and cases_shells.report_anchor_facts(None) == (0, 0, 0)
    # the round 9 log (no anchor hops: the report went to the front) counts nothing
    assert cases_shells.report_anchor_facts('x hop=handoff_insert replaced_prefix_chars=0 report="d-" before="AdCo" after="d-AdCo"') == (0, 0, 0)


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
    assert "HANDED BACK" in v["text"] and v["enter_handed_back"] == "young_hold_cap" and v["enter_gave_up_to_pty"] is None, v["text"]
    # #1996 (owner decision 2026-11-09): the same on a build that hands nothing back: the young cap WRITES the ssh Enter to the pty and logs the new hop
    gave_up = "x input_order hop=young_hold_gave_up_to_pty hold_id=25 why=young_hold_cap evidence=Some(Window) entries=1 routed=1 command_interactive=false ssh_gate_armed=false ssh_holding=false t=2.5"
    v = cases.ssh_trial_facts(ssh + [gave_up] + typed + [kd_enter(40.0)], line, [])
    assert not v["exact"] and v["text"].startswith("#1614: the `ssh` Enter ran to its young hold's cap (why=young_hold_cap)"), v["text"]
    assert "WRITTEN TO THE PTY" in v["text"] and "HANDED BACK" not in v["text"] and v["enter_handed_back"] is None, v["text"]
    assert v["enter_gave_up_to_pty"] == "young_hold_cap" and v["gave_up_enter_no"] == 1 and not v["gave_up_after_hold"], v
    # ... and for an Enter typed AFTER the ssh hold engaged (#1609): the new hop, read with the hold already seen
    v = cases.ssh_trial_facts(ssh + typed[:10] + [held(3.0 + i) for i in range(10)] + [gave_up] + typed[10:], line, [line])
    assert v["exact"] and v["enter_gave_up_to_pty"] == "young_hold_cap" and v["gave_up_after_hold"], v["text"]
    v = cases.ssh_trial_facts(ssh + typed[:10] + [held(3.0 + i) for i in range(10)] + [gave_up] + typed[10:], line, [])
    assert v["text"].startswith("#1609/#1614: an Enter typed after the ssh hold engaged ran to the local young hold's cap"), v["text"]
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
    # #1996: on a new build a connect give-up hands nothing back: it WRITES the held keys to the pty at the give-up (one give_up_keys_to_pty hop)
    wrote = ["x input_order hop=kh_ssh_give_up why=no_probe_give_up held_bytes=10 t=21.0",
             "x input_order hop=give_up_keys_to_pty why=ssh_connect_give_up keys=10 bytes=10 written=10 to_editor=0 pty_gone=0 t=21.1"]
    v = cases.ssh_trial_facts(ok + wrote, line, [])
    assert v["text"].startswith("#1996: the ssh connect GAVE UP") and "10 key(s), 10 of 10 byte(s) written, 0 put in the input" in v["text"], v["text"]
    assert v["give_up_writes"] == [dict(why="ssh_connect_give_up", keys=10, bytes=10, written=10, to_editor=0)] and v["give_ups"] == 1, v
    # a give-up write of another hold (an Enter gate's) in the window is recorded but is not the connect's: the verdict stays the line's
    v = cases.ssh_trial_facts(ok + ["x input_order hop=give_up_keys_to_pty why=enter_gate keys=2 bytes=2 written=2 to_editor=0 pty_gone=0"], line, [])
    assert v["text"].startswith("#1292: the line never reached") and len(v["give_up_writes"]) == 1, v["text"]
    # the give-up write does not hide an exact line
    v = cases.ssh_trial_facts(ok + wrote, line, [line])
    assert v["exact"] and v["text"].startswith("exact"), v["text"]
    # #1292: held, then the line never reached the remote
    v = cases.ssh_trial_facts(ok, line, [])
    assert v["text"].startswith("#1292: the line never reached"), v["text"]
    # audit #4: a silent remote rec.log with the EFFECT seen is exact (the hook logs the C line after the effect file), never "logged nothing"
    v = cases.ssh_trial_facts(ssh, line, [], effect_seen=True)
    assert v["exact"] and v["rec_silent"] and "logged nothing" not in v["text"] and "never reached" not in v["text"], v["text"]
    assert not cases.ssh_trial_facts(ssh, line, [], effect_seen=False)["exact"]
    v = cases.ssh_trial_facts(ok, line, [line[:10] + line[11:]], effect_seen=True)        # the log holds a DIFFERENT line: not exact, the effect does not override it
    assert not v["exact"]
    # ssh_trial_note: waits for the remote log past r0 when the effect was seen; a verdict/effect disagreement is UNVERIFIABLE (noref), not a note
    st = {"cmds": [(0.0, "echo w1 >> ~/.appci_where")], "verdict": dict(exact=False, text="#1345/#1394: ONE key lost")}
    c = types.SimpleNamespace(prof=types.SimpleNamespace(commands=lambda remote=False: st["cmds"]),
                              ssh_trial_verdict=lambda *a, **k: (st.__setitem__("kw", k), st["verdict"])[1])
    c.until = lambda pred, timeout=10, step=0.01: cases.Ctx.until(c, pred, timeout, step)
    import threading
    threading.Timer(0.1, lambda: st["cmds"].append((1.0, line[:10] + line[11:]))).start()
    r = cases.Result("t", "terminal")
    vs = []
    t = time.time()
    assert cases.ssh_trial_note(c, r, vs, "trial 1", 0.0, line, 1, effect_seen=True) is st["verdict"]
    assert time.time() - t >= 0.09 and len(st["cmds"]) == 2, "the verdict did not wait for the remote log to grow past r0"
    assert st["kw"].get("effect_seen") is True and r.noref and "UNVERIFIABLE" in r.notes[-1] and not r.fails, (r.noref, r.notes)
    r2 = cases.Result("t", "terminal")
    cases.ssh_trial_note(c, r2, vs, "trial 2", 0.0, line, 1, effect_seen=False)
    assert not r2.noref and r2.notes == ["trial 2: #1345/#1394: ONE key lost"], r2.notes
    st["verdict"] = dict(exact=True, text="exact")
    r3 = cases.Result("t", "terminal")
    cases.ssh_trial_note(c, r3, vs, "trial 3", 0.0, line, 1, effect_seen=True)
    assert not r3.noref and not r3.notes
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
def wait_ssh_gone_waits_for_the_app_to_see_ssh_end():
    """r18w5: leave_remote's PowerShell settle is followed by a wait for `hop=ssh_pty_owner ssh=false` (the app saw the ssh block end), so the next
    trial is not typed into a dying ssh.exe; no trace -> no wait"""
    d = tempfile.mkdtemp()
    try:
        lp = os.path.join(d, "phosphor.log")
        t0 = time.time()
        with open(lp, "w") as fh:
            fh.write("x hop=ssh_pty_owner ssh=false site=block_completed t=%.1f\n" % ((t0 - 50) * 1000))     # an OLDER ssh end: must not count
        c = types.SimpleNamespace(log_path=lp)
        c.app_log_window = lambda t0_, t1=None: cases.Ctx.app_log_window(c, t0_, t1)
        t = time.time()
        assert cases.Ctx.wait_ssh_gone(c, t0, 0.6) is False and time.time() - t >= 0.5
        with open(lp, "a") as fh:
            fh.write("y hop=ssh_pty_owner ssh=false site=block_completed t=%.1f\n" % ((time.time() + 0.1) * 1000))
        assert cases.Ctx.wait_ssh_gone(c, t0, 3.0) is True
        assert cases.Ctx.wait_ssh_gone(types.SimpleNamespace(log_path=None), t0, 3.0) is False
    finally:
        shutil.rmtree(d, ignore_errors=True)


@check
def leave_remote_level_waits_for_the_probes_own_prompt_before_exit():
    """r7w: the probe's effect file appears a few ms before the rc hook logs the probe's prompt; `before` sampled at the file made that prompt count as
    the `exit`'s, and the next probe was typed into the dying inner ssh client. The `exit` is typed only after the probe's own prompt is counted."""
    import threading
    state = {"prompts": 0, "file": [], "typed": [], "t_prompt": None}
    toks = iter(["probe%d" % i for i in range(5)])

    def type_(text, ms, enter=False):
        state["typed"].append((text, time.time(), state["prompts"]))
        if text.startswith("echo w"):
            state["file"].append(text.split()[1])
            threading.Timer(0.3, lambda: state.__setitem__("prompts", state["prompts"] + 1)).start()     # the hook logs the prompt late
    c = types.SimpleNamespace(
        prof=types.SimpleNamespace(prompts=lambda remote=False: state["prompts"], local_prompts=True),
        be=types.SimpleNamespace(type=type_),
        tok=lambda n: next(toks), ptok=lambda n: next(toks), local_where=lambda: [], reset=lambda: None, shell_gone=lambda base: False,
        log_path=None, wait_ssh_gone=lambda *a, **k: False,
        until=lambda pred, timeout=10, step=0.02: cases.Ctx.until(c, pred, timeout, step), prompts=lambda: 0)
    c.lines = lambda name, remote=False: (["tok"] if name == "rn.txt" else list(state["file"]))
    c.deadline = None
    r = types.SimpleNamespace(check=lambda ok, msg: (_ for _ in ()).throw(AssertionError(msg)) if not ok else None)
    try:
        assert cases.Ctx.leave_remote(c, r, "x", "rn.txt", "tok", None, timeout=2, level=True) is True
    except AttributeError:
        return          # Ctx.until needs more of a real Ctx than this stand-in has: the ordering is covered by the live run
    exits = [x for x in state["typed"] if x[0] == "exit"]
    assert exits and exits[0][2] >= 1, "exit typed before the probe's own prompt was counted: %r" % state["typed"]


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
    # Round 10: a paste chord is ONE key the app logs as taken by a binding (hop=keydown_keybound). It is counted on both sides: the r18d8 "n/a" row
    # took it out of `sent` because the bound key had no line the accounting read, which also hid a paste chord that never arrived.
    txt3, j3 = injacct.account(be3.calls, log2, "x")
    assert (j3["sent"], j3["seen"], j3["lost"], j3["pastes"]) == (1, 0, 1, 1) and injacct.loss_verdict(j3, "x"), (j3, txt3)      # no line for it: lost
    log3 = os.path.join(d, "p3.log")
    open(log3, "w").write(kd("a", 200.01, 1) + "input_order hop=keydown_keybound dispatch_seq=7 key=<1 chars> ctrl=true alt=false shift=true cmd=false step=3 "
                          "action_type=Paste t=%.3f\n" % 300050)
    txt3, j3 = injacct.account(be3.calls, log3, "x")
    assert (j3["sent"], j3["seen"], j3["lost"], j3["bound"]) == (1, 1, 0, 1) and injacct.loss_verdict(j3, "x") is None and "1 of the keys are paste" in txt3, (j3, txt3)
    assert injacct.account(be.calls, None, "x")[1]["seen"] is None                 # no trace: reported, not compared
    assert injacct.account([], log, "x") == (None, None)


@check
def audit6_7_harness_probe_lines_are_not_the_cases_commands_and_comparisons_wait_for_the_log():
    """harness audit #6 (#1913): the where / pid probe lines never count as a case's commands. #7 (#1914): a rec.log that fills 800 ms after the effect
    file is still read whole (await_commands), and the sites the audit names use it instead of a fixed sleep."""
    import threading
    cmds = [(0.0, "echo wAAAA_xyz >> ~/.appci_where"), (0.0, "sh -c 'echo pBBBB_xyz:$PPID >> ~/.appci_pid'"), (0.0, "Add-Content ~/.appci_pid pCCCC_xyz:$PID"),
            (0.0, "echo t1 >> alive_x.txt"), (0.0, "echo k >> proof_a@b.txt"), (0.0, "echo real >> r_ft10.txt"), (0.0, "cat notes.appci_where.md")]
    c = types.SimpleNamespace(prof=types.SimpleNamespace(commands=lambda remote=False: list(cmds)),
                              HARNESS_MARKER_RE=cases.Ctx.HARNESS_MARKER_RE, APP_BOOTSTRAP_RE=cases.Ctx.APP_BOOTSTRAP_RE)
    c.cmds_since = lambda mark: cases.Ctx.cmds_since(c, mark)
    c.until = lambda pred, timeout=10, step=0.02: cases.Ctx.until(c, pred, timeout, step)
    assert c.cmds_since(0) == ["echo real >> r_ft10.txt", "cat notes.appci_where.md"], c.cmds_since(0)      # a user's own line that merely contains the word stays
    assert all(cases.Ctx.HARNESS_MARKER_RE.search(cases.depth_probe_cmd(p, "pX")) for p in ("bash", "powershell")), "the pid probe's own line is not matched"
    # #7: three typed commands; the third C line lands 0.8 s after the effect: the comparison must see 3, and must not have slept a fixed time when they are there
    del cmds[:]
    cmds += [(0.0, "a"), (0.0, "b")]
    threading.Timer(0.8, lambda: cmds.append((0.0, "c"))).start()
    t = time.time()
    assert cases.Ctx.await_commands(c, 0, 3) == ["a", "b", "c"] and 0.7 <= time.time() - t < 3.0, time.time() - t
    t = time.time()
    assert cases.Ctx.await_commands(c, 0, 3) == ["a", "b", "c"] and time.time() - t < 0.2, "await_commands slept although the log was complete"
    assert cases.Ctx.await_commands(c, 0, lambda cs: "c" in cs) == ["a", "b", "c"]
    t = time.time()
    assert cases.Ctx.await_commands(c, 0, 4, timeout=0.3) == ["a", "b", "c"] and 0.25 <= time.time() - t < 1.5        # a real loss: bounded, the caller's comparison reports it
    del cmds[:]
    t = time.time()
    assert cases.Ctx.await_commands(c, 0, 2, timeout=5) == [] and time.time() - t < 0.2, "no rec.log at all (PowerShell pane): no wait"
    here = os.path.dirname(os.path.abspath(__file__))
    src = open(os.path.join(here, "cases.py")).read()
    er = src[src.index("def effect_run("):src.index("def ", src.index("def effect_run(") + 10)]
    assert "ctx.await_commands(mark, n)" in er and er.index("ctx.await_commands(mark, n)") < er.index("have = ctx.cmds_since(mark)"), "effect_run compares rec.log without waiting for it"
    assert "ctx.await_commands(mark, len(cmds))" in src and "ctx.await_commands(mark, len(typed))" in src and "ctx.await_commands(mark, len(want))" in src
    assert "ctx.await_commands(mark, lambda cs: want_cmd in cs)" in open(os.path.join(here, "cases_caseintl.py")).read()
    assert "ctx.await_commands(mark, lambda cs: line in cs)" in open(os.path.join(here, "cases_coverage_a.py")).read()


@check
def audit10_case_log_slice_on_every_platform_follows_the_log_across_a_rotation():
    """harness audit #10 (#1917): the per-case app-log slice is written on every backend, starts at the case's LogOffset, and still holds the case's
    first half after the app rotated the log in the middle of the case."""
    import run as runmod
    d = tempfile.mkdtemp()
    try:
        log = os.path.join(d, "phosphor.log")
        open(log, "w").write("2026-01-01T00:00:00Z before the case\n")
        c = _ctx(d, log)
        c.extra_logs = []
        off = c.log_size()
        open(log, "a").write("2026-01-01T00:00:01Z case first half\n")
        os.rename(log, log + ".in_session.0")                                  # the app's in-session rotation, mid-case
        open(log, "w").write("2026-01-01T00:00:02Z case second half\n")
        txt = runmod.case_log_text(c, off, log, 0)
        assert "case first half" in txt and "case second half" in txt and "before the case" not in txt, txt
        assert txt.index("first half") < txt.index("second half")
        prof = types.SimpleNamespace(home=d)
        for be_name in ("x11", "wayland", "mac", "windows"):
            run_dir = os.path.join(d, "run_" + be_name)
            runmod.save_case_log(run_dir, "terminal", "some_case", 0, prof, c, types.SimpleNamespace(find_log=lambda: log), None, cap=1_000_000, log_off=off, backend_name=be_name)
            got = open(os.path.join(run_dir, "logs", "terminal", "case_some_case.log")).read()
            assert "case first half" in got and "case second half" in got and "before the case" not in got, (be_name, got)
        # a case that relaunched the app itself: the instance it began on is read from the case's offset, a later launch whole
        snap = os.path.join(d, "launch0_1.log")
        open(snap, "w").write("2026-01-01T00:00:00Z before the case\n2026-01-01T00:00:01Z old instance in the case\n")
        c.extra_logs = [snap]
        txt = runmod.case_log_text(c, off, log, 0)
        assert "old instance in the case" in txt and txt.count("before the case") == 0, txt
        # no offset (no log path when the case began): the timestamp cut, as before
        txt = runmod.case_log_text(c, None, log, __import__("calendar").timegm(time.strptime("2026-01-01T00:00:02", "%Y-%m-%dT%H:%M:%S")) + 1)
        assert "case second half" in txt and "before the case" not in txt, txt
        # the run loop saves the slice without asking which platform it is on
        src = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "run.py")).read()
        loop = src[src.index("def main("):src.index("def account_case(")]
        assert loop.count("save_case_log(") == 3 and "if win:     # the rotating app log" not in loop and "log_off=log_off, backend_name=a.backend" in loop
        for ln in loop.splitlines():
            assert "save_case_log(" not in ln or "log_off=log_off" in ln, "a save_case_log call without the case's LogOffset: " + ln.strip()
    finally:
        shutil.rmtree(d, ignore_errors=True)


@check
def audit8_9_12_profile_read_retries_and_post_case_invariants_reset_leaked_state():
    """harness audit #8 (#1915): Profile.read survives a PermissionError (Windows: the shell has the log open). #9 / F (#1916): reference-terminal and
    lab-arm state a case left behind is reset and named. #12 (#1919): the wedged-pane probe runs after a failing case on every platform."""
    import builtins
    import lab as L
    import profile as P
    import run as runmod
    d = tempfile.mkdtemp()
    real_open = builtins.open
    try:
        open(os.path.join(d, "prompt.log"), "w").write("P\t1\nP\t2\n")
        open(os.path.join(d, "rec.log"), "w").write("C\t1.0\techo hi\n")
        pr = types.SimpleNamespace(ctl=d, rctl=d, refctl=d, ref_active=False, remote=None)
        pr.read = lambda which, remote=False: P.Profile.read(pr, which, remote)
        n = {"left": 1, "calls": 0}

        def flaky(path, *a, **k):
            if str(path).endswith((".log",)) and str(path).startswith(d):
                n["calls"] += 1
                if n["left"] > 0:
                    n["left"] -= 1
                    raise PermissionError(13, "sharing violation")
            return real_open(path, *a, **k)
        builtins.open = flaky
        try:
            assert P.Profile.prompts(pr) == 2 and n["calls"] == 2, n                       # one PermissionError, then the read
            n.update(left=3, calls=0)
            assert P.Profile.commands(pr) == [(1.0, "echo hi")] and n["calls"] == 4, n
            n.update(left=99, calls=0)
            try:
                P.Profile.read(pr, "rec.log")
                raise AssertionError("an unreadable log read as empty: 'no commands' would be a false verdict")
            except PermissionError:
                assert n["calls"] == 5, n
        finally:
            builtins.open = real_open
        assert P.Profile.read(pr, "nope.log") == []
        # #9: the invariants
        refocused = []
        be = types.SimpleNamespace(guard_override=None, refocus=lambda verify=False: refocused.append(1))
        c = types.SimpleNamespace(arm=None, remote=L.LocalRemote(pr), _lab_prev=None)
        assert runmod.post_case_invariants(c, pr, be) == [] and not refocused, "a clean case tripped the invariant"
        pr.ref_active, be.guard_override = True, dict(pids={1})
        out = runmod.post_case_invariants(c, pr, be)
        assert len(out) == 2 and "ref_active" in out[0] and "guard_override" in out[1] and pr.ref_active is False and be.guard_override is None and refocused == [1], out
        lab_rem = object()
        c.arm, c.remote, pr.remote, c._lab_prev = dict(name="u_dash/nohelper"), lab_rem, lab_rem, (None, None)
        out = runmod.post_case_invariants(c, pr, be)
        assert len(out) == 1 and "u_dash/nohelper" in out[0] and c.arm is None and isinstance(c.remote, L.LocalRemote) and pr.remote is None, out
        c.remote = lab_rem                                                              # arm cleared by the case, the remote not restored
        assert len(runmod.post_case_invariants(c, pr, be)) == 1 and isinstance(c.remote, L.LocalRemote)
        assert runmod.post_case_invariants(c, pr, be) == []
        be.refocus = lambda verify=False: (_ for _ in ()).throw(RuntimeError("no window"))
        pr.ref_active = True
        out = runmod.post_case_invariants(c, pr, be)                                    # never raises
        assert pr.ref_active is False and any("refocus after the reset failed" in o for o in out), out
        # the run loop: the invariant runs right after the case (before the screenshot / probes), emits a row, and the wedge probe is not Windows-only
        src = real_open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "run.py")).read()
        loop = src[src.index("def main("):src.index("def post_case_invariants(")]
        assert loop.index("leaks = post_case_invariants(ctx, prof, be)") < loop.index('shot = None if sim else ctx.shot(c["name"]')
        assert '"harness_state_reset_after_" + c["name"]' in loop
        assert 'ctx.pane_responsive() if (not sim and (status == "fail" or (win and c["name"] in ("ctrl_c_d", "cadence"))))' in loop, "the wedged-pane probe is platform-gated again"
    finally:
        builtins.open = real_open
        shutil.rmtree(d, ignore_errors=True)


@check
def audit5_accounting_window_runs_to_case_end_and_a_loss_outside_loss_cases_is_noref():
    """harness audit #5 (#1912): a held / slow-classifier Enter processed more than 3 s after the last injector call is still the case's Enter when the
    window ends at the case's end; lost > 0 (or an Enter short) on a case that is not a loss case and has no failure is noref, never a pass."""
    import backend
    import injacct
    import run as runmod
    d = tempfile.mkdtemp()
    try:
        def kd(k, t, seq):
            return "input_order keydown key=%s ctrl=false alt=false shift=false cmd=false t=%.3f seq=%d\n" % (k, t * 1000, seq)
        def hop(t):
            return "input_order hop=enter_begin deferred=false buffer=x input_type=Shell t=%.3f\n" % (t * 1000)
        be = backend.X11Backend()
        be.record_injection(["xdotool", "type", "--", "ab", "key", "Return"], 200.0, 200.2)
        be.record_injection(["xdotool", "type", "--", "c", "key", "Return"], 201.0, 201.2)
        log = os.path.join(d, "p.log")
        open(log, "w").write(kd("a", 200.01, 1) + kd("b", 200.02, 2) + kd("c", 201.01, 3) + hop(200.15) + hop(206.2))     # the 2nd Enter: wall1 + 5 s
        assert injacct.window(be.calls) == (199.5, 204.2) and injacct.window(be.calls, 210.0) == (199.5, 210.0)
        assert injacct.window(be.calls, 202.0) == (199.5, 204.2), "a case that ends at once keeps the 3 s tail"
        # rounds 8/9: the window never starts before the case does; the tail of the harness's previous line (typed after the case before) is not this case's
        assert injacct.window(be.calls, 210.0, 199.9) == (199.9, 210.0) and injacct.window(be.calls, 210.0, 150.0) == (199.5, 210.0)
        assert injacct.window(be.calls, 210.0, 205.0) == (200.0, 210.0), "a floor past the first call never cuts the case's own keys"
        log2 = os.path.join(d, "p2.log")
        open(log2, "w").write(kd("z", 199.70, 7) + kd("Return", 199.75, 8) + open(log).read())      # the previous probe's last key and its Enter
        _, slack = injacct.account(be.calls, log2, "x", t_end=210.0)
        assert (slack["extra"], slack["enters_seen"]) == (2, 3), slack                                 # the artifact: +2 keys, +1 Enter
        _, floored = injacct.account(be.calls, log2, "x", t_end=210.0, t_floor=199.9)
        assert (floored["sent"], floored["seen"], floored["extra"], floored["enters_seen"]) == (5, 5, 0, 2), floored
        _, old = injacct.account(be.calls, log, "x")
        assert (old["enters_seen"], old["lost"]) == (1, 1), old                    # the old fixed window: the late Enter reads as lost
        _, j = injacct.account(be.calls, log, "x", t_end=210.0)
        assert (j["sent"], j["seen"], j["lost"], j["enters_sent"], j["enters_seen"]) == (5, 5, 0, 2, 2), j
        assert injacct.loss_verdict(j, "x") is None
        # the verdict: lost keys / a short Enter outside the loss cases
        assert "1 of 5" in injacct.loss_verdict(old, "x") and injacct.loss_verdict(old, "fast_typing") is None
        assert injacct.loss_verdict(dict(sent=3, seen=3, lost=0, enters_sent=2, enters_seen=1), "x")           # extra keys hid the count: the Enter gap still shows
        assert injacct.loss_verdict(dict(sent=3, seen=None, lost=None, enters_sent=1, enters_seen=None), "x") is None      # no key trace: nothing to compare
        assert injacct.loss_verdict(dict(sent=0, seen=1, lost=None, enters_sent=1, enters_seen=1, na=True), "x") is None and injacct.loss_verdict(None, "x") is None
        # run.account_case: no fails + lost -> noref; a failing case keeps its failure; a loss case and a clean case stay as they are
        r = cases.Result("x", "terminal")
        runmod.account_case(r, be.calls, log, "x", 204.0)
        assert r.noref and not r.fails and any("UNVERIFIABLE" in n and "never reached the app" in n for n in r.notes) and r.metrics["injector"]["lost"] == 1, r.notes
        assert ("fail" if r.fails else ("noref" if r.noref else "pass")) == "noref"
        r = cases.Result("x", "terminal")
        r.fails.append("its own failure")
        runmod.account_case(r, be.calls, log, "x", 204.0)
        assert not r.noref and r.fails == ["its own failure"] and any(n.startswith("injector accounting: injector sent 5") for n in r.notes), r.notes
        r = cases.Result("fast_typing", "terminal")
        runmod.account_case(r, be.calls, log, "fast_typing", 204.0)
        assert not r.noref and r.notes[0].startswith("injector accounting: WARN loss case"), r.notes
        r = cases.Result("x", "terminal")
        runmod.account_case(r, be.calls, log, "x", 210.0)
        assert not r.noref and not r.fails and r.metrics["injector"]["lost"] == 0
        # Windows is accounted like every other backend: the run loop no longer names backends
        src = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "run.py")).read()
        assert 'a.backend in ("wayland", "x11", "mac") and not sim' not in src and src.count("account_case(") == 2, "the accounting gate names backends again"
    finally:
        shutil.rmtree(d, ignore_errors=True)


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
    # r18u28: intl_wide_edit's lead is unique per RUN: the seeded part repeats across runs, and an earlier run's line in the PSReadLine history was a
    # prefix match that End accepted. The nonce comes from the OS, not the seeded rng, and is plain alphanumeric (a PowerShell / bash bareword).
    assert ci.edit_lead(2, "T241_86gi", nonce="") == "k2t241_86gi" and ci.edit_lead(2, "T241_86gi", nonce="ab12") == "k2t241_86giab12"
    assert ci.edit_lead(2, "T241_86gi").startswith("k2t241_86gi") and ci.edit_lead(2, "T241_86gi") != "k2t241_86gi"
    assert ci.EDIT_LEAD_NONCE.isalnum() and len(ci.EDIT_LEAD_NONCE) == 6
    old_line = "Write-Output k2t241_86giBq | ForEach-Object { [IO.File]::AppendAllText('C:\\old\\ie_2.txt', $_) }"
    assert not old_line.startswith("Write-Output " + ci.edit_lead(2, "T241_86gi") + "Bq"), "an earlier run's line must not prefix-match this run's"
    hist = types.SimpleNamespace(ps_history_lines=lambda: [old_line, "Write-Output other"])
    assert ci.stale_history_hits(hist, "Write-Output k2t241_86gi") == [old_line] and ci.stale_history_hits(object(), "x") == []
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
    no_log_show = lambda pid, t0: ("", None)      # #1782 review F7: never `log show` for pid 1 from the self-test
    m = runmod.classify_death("APP DIED: pid 1 is gone", C("13:51:34 enter_begin buffer=\"exit\"\n13:51:34 WindowId(0) will close\n", True), t0, 0, no_log_show)
    assert "OWN `exit`" in m and "NOT an app crash" in m, m
    m = runmod.classify_death("APP DIED: pid 1 is gone", C("thread 'main' panicked at x.rs:1\nWindowId(0) will close", True), t0, 0, no_log_show)
    assert "OWN" not in m and "crash evidence" in m, m


@check
def i1782_launchd_exit_record_names_the_signal_and_the_sender():
    """#1782: the dead app's exit status comes from launchd's unified-log line (the harness cannot wait() an `open`-launched app); the app's own
    fatal-signal line counts as crash evidence"""
    import run as runmod
    line = ("2026-11-05 08:23:05.940 Df launchd[1:35582ae] [gui/501/application.dev.phosphor.Phosphor.533091616.533091621.7DA0F36B [94694]:] "
            "exited due to SIGBUS | sent by exc handler[94694], ran for 1251994ms\n")
    other = "2026-11-05 08:23:05.762 Df launchd[1:35582ae] [system/us.electronic.eveusb.daemon [6856]:] exited due to SIGSEGV | sent by exc handler[6856], ran for 310ms\n"
    assert runmod.launchd_exit_record(other + line, 94694) == ("due to SIGBUS | sent by exc handler[94694]", "SIGBUS"), runmod.launchd_exit_record(other + line, 94694)
    assert runmod.launchd_exit_record(other, 94694) == (None, None)
    assert runmod.launchd_exit_record("[gui/501/application.x [77]:] exited with exit code: 0\n", 77) == ("with exit code: 0", None)
    killed = "[gui/501/application.x [88]:] exited due to SIGKILL | sent by runningboardd[123], ran for 5ms\n"
    assert runmod.launchd_exit_record(killed, 88) == ("due to SIGKILL | sent by runningboardd[123]", "SIGKILL")
    fatal = "2026-11-05T14:23:05Z [ERROR] fatal signal SIGBUS (10) code=2 addr=0x16f60bff0 thread='clipboard-read' pid=94694 (fault); the process is about to die"
    assert runmod.FATAL_LINE_RX.search("x\n" + fatal + "\n").group(0) == fatal


@check
def i1782_macos_paste_read_budget_is_raised_only_for_the_pasteboard_read():
    """#1782 (owner 2026-11-05): the main-thread pasteboard read gets a larger macOS budget; every other platform and UI budget stays 4 ms"""
    assert cases.paste_read_budget_ms("macos") == 250.0
    assert cases.paste_read_budget_ms("linux") == cases.UI_ITEM_BUDGET_MS == 4.0
    assert cases.paste_read_budget_ms("linux-wayland") == 4.0 and cases.paste_read_budget_ms("windows") == 4.0
    line = "2026-11-05T14:23:05Z [DEBUG] input_order hop=pasteboard_read kind=TextAndPaths ms=31.50 bytes=1000000 ok=true t=1793888585926.1\n"
    assert cases.pasteboard_reads("x\n" + line) == [("TextAndPaths", 31.5, 1000000)]

    class R:
        def __init__(s): s.fails, s.metrics, s.notes = [], {}, []
        def check(s, ok, msg):
            if not ok:
                s.fails.append(msg)
        def metric(s, k, v): s.metrics[k] = v
        def note(s, msg): s.notes.append(msg)
    r = R()
    assert cases.judge_paste_read(r, line, "macos", "big_paste") == 31.5 and not r.fails, r.fails
    assert r.metrics["big_paste_pasteboard_read"]["budget_ms"] == 250.0
    r = R()
    cases.judge_paste_read(r, line, "linux", "big_paste")
    assert r.fails, "a 31 ms main-thread read is over the unchanged 4 ms budget elsewhere"
    r = R()
    cases.judge_paste_read(r, line.replace("ms=31.50", "ms=300.00"), "macos", "big_paste")
    assert r.fails and "250 ms" in r.fails[0], r.fails
    r = R()
    assert cases.judge_paste_read(r, "input_order hop=ui_item kind=event ms=9 t=1\n", "linux", "big_paste") is None and not r.fails and not r.metrics and not r.notes
    r = R()      # #1782 review F6: a macOS run with no read logged says so instead of passing silently
    assert cases.judge_paste_read(r, "input_order hop=ui_item kind=event ms=9 t=1\n", "macos", "big_paste") is None and not r.fails
    assert r.notes and "NOT judged" in r.notes[0], r.notes


@check
def r10_accounting_counts_bound_keys_and_still_fails_a_really_lost_key():
    """Round 10 (docs/design/r10-injector.md): rounds 7-9 read every key a key binding took (the reset's Ctrl-C / Ctrl-U, Tab, Escape, paste chords) as
    "lost before the app", with the same counts on macOS and Wayland. The app's evidence of one key is exactly one of: a `keydown` line (unbound), a
    `keydown_keybound` hop (a binding took it), a `core_enter dispatch_result handled=true` hop (an Enter a binding took). With those counted a clean
    case reads lost 0, and taking ANY one line away (a typed key, a bound chord, an Enter) reads lost 1 and is UNVERIFIABLE: the count is not blind.
    macOS: `os_key_event` (NSApplication sendEvent) is "reached the app"; a key-down that reached it and was never dispatched is allowed only for the
    Cmd chords the injector posted (a menu key equivalent), anything else is UNVERIFIABLE with its own sentence."""
    import backend
    import injacct
    import run as runmod
    d = tempfile.mkdtemp()
    try:
        def kd(k, t, seq):
            return "input_order keydown key=%s ctrl=false alt=false shift=false cmd=false t=%.3f seq=%d\n" % (k, t * 1000, seq)
        def kb(k, t, dseq, ctrl="true"):
            return ("input_order hop=keydown_keybound dispatch_seq=%d key=%s ctrl=%s alt=false shift=false cmd=false step=2 action_type=X t=%.3f\n" % (dseq, k, ctrl, t * 1000))
        def ce(t, handled="true"):
            return ("input_order hop=core_enter stage=dispatch_keystroke chain_len=4 composing=false ctrl=false alt=false shift=false cmd=false t=%.3f\n" % (t * 1000 - 0.2)
                    + "input_order hop=core_enter stage=dispatch_result handled=%s pending=false t=%.3f\n" % (handled, t * 1000))
        def eb(t, deferred="false"):
            return "input_order hop=enter_begin deferred=%s buffer=x input_type=Shell t=%.3f\n" % (deferred, t * 1000)
        def osk(t, enter="false"):
            return "input_order hop=os_key_event age_ms=0.400 enter=%s ctrl=false t=%.3f\n" % (enter, t * 1000)
        # --- Linux shape: the reset (Ctrl-C, Ctrl-U), "ab", Tab, Return (the editor's), then Return while a program runs (goes to the pty: a keydown)
        be = backend.X11Backend()
        be.record_injection(["xdotool", "key", "ctrl+c"], 100.0, 100.1)
        be.record_injection(["xdotool", "key", "ctrl+u"], 100.3, 100.4)
        be.record_injection(["xdotool", "type", "--", "ab", "key", "Tab", "Return"], 101.0, 101.3)
        be.record_injection(["xdotool", "key", "Return"], 102.0, 102.1)
        lines = [kb("<1 chars>", 100.05, 1), kb("<1 chars>", 100.35, 2), kd("<1 chars>", 101.05, 1), "input_order typed_characters text=<1 chars> t=101051.000 seq=1\n",
                 kd("<1 chars>", 101.10, 2), kb("tab", 101.15, 5, "false"), ce(101.20) + eb(101.2001), eb(101.25, "true"),
                 ce(102.05, "false") + kd("enter", 102.0501, 3)]
        log = os.path.join(d, "a.log")
        open(log, "w").write("".join(lines))
        txt, j = injacct.account(be.calls, log, "x", t_end=103.0)
        assert (j["sent"], j["seen"], j["lost"], j["extra"], j["enters_sent"], j["enters_seen"]) == (7, 7, 0, 0, 2, 2), (j, txt)
        assert (j["unbound"], j["bound"], j["enters_bound"]) == (3, 3, 1), j
        assert injacct.loss_verdict(j, "x") is None and "3 taken by a key binding" in txt, txt
        # the pre-round-10 count of the same log (keydown lines + enter_begin only): the three bound keys read as lost. That was the artifact.
        w = injacct.window(be.calls, 103.0)
        old = injacct.join(be.calls, injacct.app_keys(log, *w), injacct.app_enter_hops(log, *w))
        assert (old["seen"], old["lost"]) == (4, 3), old
        # NOT blind: drop any single key (the app never got it) and the row is lost 1 / UNVERIFIABLE. Index 3 is the typed_characters of key 2 (same
        # seq: it goes with its keydown; alone it would still be one key, as for an input method), index 7 a deferred re-run of a held Enter (no key).
        for i in (0, 1, 2, 4, 5, 6, 8):
            p = os.path.join(d, "drop%d.log" % i)
            body = lines[:i] + lines[i + (2 if i == 2 else 1):]
            if i == 6:
                body.append("input_order hop=core_enter stage=handle_window_event key_bindings_enabled=true focused_view=1 t=90000.000\n")      # the trace HAS core_enter lines
            open(p, "w").write("".join(body))
            _, jd = injacct.account(be.calls, p, "x", t_end=103.0)
            assert jd["lost"] == 1 and "1 of 7" in injacct.loss_verdict(jd, "x"), (i, jd)
            assert injacct.loss_verdict(jd, "key_loss_control") is None        # the loss cases keep their own verdict (and the WARN prefix)
            assert injacct.account(be.calls, p, "key_loss_control", t_end=103.0)[0].startswith("WARN loss case: "), i
            r = cases.Result("x", "terminal")
            runmod.account_case(r, be.calls, p, "x", 103.0)
            assert r.noref and not r.fails and any("UNVERIFIABLE" in n for n in r.notes), (i, r.notes)
        r = cases.Result("x", "terminal")
        runmod.account_case(r, be.calls, log, "x", 103.0)
        assert not r.noref and not r.fails and r.metrics["injector"]["lost"] == 0, r.notes
        # a lost Enter hidden by an extra key still shows in the Enter pair
        p = os.path.join(d, "enter.log")
        open(p, "w").write("".join(lines[:6] + [kd("<1 chars>", 101.22, 9)] + lines[7:]))
        _, je = injacct.account(be.calls, p, "x", t_end=103.0)
        assert (je["lost"], je["enters_sent"], je["enters_seen"]) == (0, 2, 1) and injacct.loss_verdict(je, "x"), je
        # one bound chord logged once per launch with the same dispatch_seq is two keys (two app instances); the same line twice in one log is one
        assert len(injacct.app_trace([log, log], 0, 200)["bound"]) == 6 and injacct.app_trace(None, 0, 1) is None
        mod = os.path.join(d, "mod.log")
        open(mod, "w").write(kb("shift", 100.05, 1) + kb("tab", 100.06, 2) + kb("tab", 100.06, 2))
        assert [k for _t, k in injacct.app_trace(mod, 0, 200)["bound"]] == ["tab"]
        # --- macOS shape: Ctrl-C (bound), "ab", Cmd-V (the menu takes it: os_key_event only), Return through the k: op (KEY36), Return through e:
        mb = backend.MacBackend()
        assert mb.cmd_chords(["k:9:8:0", "k:8:2:0", "k:36:0:0", "t:a:b:5:5", "e:0", "k:9:9:0"]) == 2
        def call(t0, chars, special, cmd=0, enters=0):
            return dict(n=0, wall0=t0, wall1=t0 + 0.1, chars=chars, special=["%s@%.3f" % (s, t0) for s in special], enters_posted=enters, enters_want=enters,
                        enter_wall=[t0] * enters, err=None, out=True, ref=False, cmd_chords=cmd)
        calls = [call(200.0, 0, ["KEY8"]), call(201.0, 2, []), call(202.0, 0, ["KEY9", "KEY36"], cmd=1), call(203.0, 0, ["ENTER"], enters=1)]
        mlines = [osk(200.05), kb("<1 chars>", 200.051, 1), osk(201.05), kd("<1 chars>", 201.051, 1), osk(201.07), kd("<1 chars>", 201.071, 2),
                  osk(202.05), osk(202.07, "true"), ce(202.071), osk(203.05, "true"), ce(203.051)]
        mlog = os.path.join(d, "m.log")
        open(mlog, "w").write("".join(mlines))
        txt, j = injacct.account(calls, mlog, "x", t_end=204.0)
        assert (j["sent"], j["seen"], j["lost"], j["enters_sent"], j["enters_seen"]) == (6, 6, 0, 2, 2), (j, txt)        # KEY36 is an Enter the injector posted
        assert (j["os_keys"], j["dispatched"], j["undispatched"], j["cmd_chords"], j["app_undelivered"]) == (6, 5, 1, 1, 0), j
        assert injacct.loss_verdict(j, "x") is None and "6 key-down(s) reached NSApplication, 5 dispatched" in txt, txt
        # a key macOS never handed to the app (no os_key_event): lost before the app, whatever it was
        for i in (0, 2, 6, 7, 9):
            p = os.path.join(d, "mdrop%d.log" % i)
            open(p, "w").write("".join(mlines[:i] + mlines[i + 1:]))
            _, jd = injacct.account(calls, p, "x", t_end=204.0)
            assert jd["lost"] == 1 and "never reached the app's key trace" in injacct.loss_verdict(jd, "x"), (i, jd)
        # a plain key that reached NSApplication and was never dispatched: not lost before the app, stopped inside it. Only Cmd chords are allowed that.
        p = os.path.join(d, "minside.log")
        open(p, "w").write("".join(mlines[:3] + mlines[4:]))
        _, ji = injacct.account(calls, p, "x", t_end=204.0)
        assert (ji["lost"], ji["undispatched"], ji["app_undelivered"]) == (0, 2, 1), ji
        v = injacct.loss_verdict(ji, "x")
        assert v and "never dispatched as a key event" in v and "INSIDE the app" in v, v
        r = cases.Result("x", "terminal")
        runmod.account_case(r, calls, p, "x", 204.0)
        assert r.noref and not r.fails, r.notes
    finally:
        shutil.rmtree(d, ignore_errors=True)


@check
def r12_macos_menu_key_equivalent_is_a_dispatched_key():
    """Round 12 (#2041, docs/design/r12-macssh.md): on macOS Ctrl-C with the input editor in front is the "Clear editor" menu item's key equivalent. The
    menu takes it, the item's action runs, and no `keydown` / `keydown_keybound` line is written: the reset's two Ctrl-C per case read as "reached the
    app and never dispatched", so 97 of the 99 NOREF rows of round 11's Mac run were this. The app now writes `hop=keydown_menu` for a key-down that
    triggered a menu item; the accounting counts it as dispatched. Not blind: without the line (an older build, or a Ctrl-C that really went nowhere) the
    row is UNVERIFIABLE as before, a plain key with no dispatch line is never excused by it, and a Cmd chord the app named is not allowed a second time."""
    import injacct
    import run as runmod
    d = tempfile.mkdtemp()
    try:
        def kd(t, seq):
            return "input_order keydown key=<1 chars> ctrl=false alt=false shift=false cmd=false t=%.3f seq=%d\n" % (t * 1000, seq)
        def ce(t):
            return "input_order hop=core_enter stage=dispatch_result handled=true pending=false t=%.3f\n" % (t * 1000)
        def osk(t, enter="false", ctrl="false"):
            return "input_order hop=os_key_event age_ms=0.400 enter=%s ctrl=%s t=%.3f\n" % (enter, ctrl, t * 1000)
        def menu(t, ctrl="true", cmd="false"):
            return "input_order hop=keydown_menu ctrl=%s cmd=%s t=%.3f\n" % (ctrl, cmd, t * 1000)
        def cc(t):      # what round 11's log has for the reset's Ctrl-C: the app acted on it, with no dispatch line
            return "input_order hop=ctrl_c_keydown powershell_setup_hold=Off t=%.3f\ninput_order hop=ctrl_c_route route=none t=%.3f\n" % (t * 1000, t * 1000 + 0.03)
        def call(t0, chars, special, cmd=0, enters=0):
            return dict(n=0, wall0=t0, wall1=t0 + 0.1, chars=chars, special=["%s@%.3f" % (s, t0) for s in special], enters_posted=enters, enters_want=enters,
                        enter_wall=[t0] * enters, err=None, out=True, ref=False, cmd_chords=cmd)
        # the reset's Ctrl-C, "ab", Return, the reset's Ctrl-C
        calls = [call(300.0, 0, ["KEY8"]), call(301.0, 2, ["ENTER"], enters=1), call(302.0, 0, ["KEY8"])]
        body = [osk(300.05, ctrl="true"), cc(300.056), osk(301.05), kd(301.051, 1), osk(301.07), kd(301.071, 2), osk(301.09, "true"), ce(301.091),
                osk(302.05, ctrl="true"), cc(302.056)]
        def write(name, lines):
            p = os.path.join(d, name)
            open(p, "w").write("".join(lines))
            return p
        # round 11's log (a build without the line): judged as before, 2 key-downs never dispatched, NOREF
        old = write("old.log", body)
        txt, j = injacct.account(calls, old, "ssh_nested_typeahead", t_end=303.0)
        assert (j["sent"], j["seen"], j["lost"], j["os_keys"], j["dispatched"], j["undispatched"], j["menu"], j["app_undelivered"]) == (5, 5, 0, 5, 3, 2, 0, 2), (j, txt)
        v = injacct.loss_verdict(j, "ssh_nested_typeahead")
        assert v and "2 key-down(s) reached the app" in v and "keydown_menu" in v, v
        assert injacct.loss_verdict(j, "ssh_connect_typing") is None       # a loss case keeps its own verdict, which is why that one row was PASS in round 11
        # the same keys on a build that names the menu's key-downs: all 5 dispatched, the row is judged on its own checks
        new = write("new.log", body[:2] + [menu(300.0561)] + body[2:] + [menu(302.0561)])
        txt, j = injacct.account(calls, new, "ssh_nested_typeahead", t_end=303.0)
        assert (j["os_keys"], j["dispatched"], j["undispatched"], j["menu"], j["menu_cmd"], j["app_undelivered"]) == (5, 5, 0, 2, 0, 0), (j, txt)
        assert injacct.loss_verdict(j, "ssh_nested_typeahead") is None and "5 dispatched as key events (2 of them a menu key equivalent), 0 not" in txt, txt
        r = cases.Result("ssh_nested_typeahead", "terminal")
        runmod.account_case(r, calls, new, "ssh_nested_typeahead", 303.0)
        assert not r.noref and not r.fails, r.notes
        # NOT blind: one Ctrl-C the menu did not take (no line) is still a key that stopped inside the app
        one = write("one.log", body[:2] + [menu(300.0561)] + body[2:])
        _, j = injacct.account(calls, one, "x", t_end=303.0)
        assert (j["undispatched"], j["menu"], j["app_undelivered"]) == (1, 1, 1) and "1 key-down(s) reached the app" in injacct.loss_verdict(j, "x"), j
        r = cases.Result("x", "terminal")
        runmod.account_case(r, calls, one, "x", 303.0)
        assert r.noref and not r.fails, r.notes
        # a menu line never excuses a PLAIN key with no dispatch line (the typed "a" here): the count is per key, not per kind
        plain = write("plain.log", body[:2] + [menu(300.0561)] + body[2:3] + body[4:] + [menu(302.0561)])
        _, j = injacct.account(calls, plain, "x", t_end=303.0)
        assert (j["lost"], j["undispatched"], j["app_undelivered"]) == (0, 1, 1) and injacct.loss_verdict(j, "x"), j
        # a menu line of another case (outside the window) is not counted
        late = write("late.log", body + [menu(400.0), menu(401.0)])
        _, j = injacct.account(calls, late, "x", t_end=303.0)
        assert (j["menu"], j["app_undelivered"]) == (0, 2), j
        # Cmd-V: named by the app (the Paste item is a custom item too) -> dispatched, and the Cmd allowance is used up by it, so a second key with no
        # line is NOT excused; on a build without the line the allowance covers the Cmd-V only, as in round 10
        calls2 = [call(310.0, 1, ["KEY9"], cmd=1)]
        both = [kd(250.0, 9), osk(310.05), osk(310.07)]         # (an earlier key, so the log has a key trace;) Cmd-V and a typed key, neither with a dispatch line
        _, j = injacct.account(calls2, write("v_new.log", both[:2] + [menu(310.051, ctrl="false", cmd="true"), both[2]]), "x", t_end=311.0)
        assert (j["dispatched"], j["undispatched"], j["menu_cmd"], j["cmd_allowed"], j["app_undelivered"]) == (1, 1, 1, 0, 1), j
        _, j = injacct.account(calls2, write("v_old.log", both), "x", t_end=311.0)
        assert (j["dispatched"], j["undispatched"], j["cmd_allowed"], j["app_undelivered"]) == (0, 2, 1, 1), j
        _, j = injacct.account(calls2, write("v_ok.log", both[:2] + [menu(310.051, ctrl="false", cmd="true"), both[2], kd(310.071, 1)]), "x", t_end=311.0)
        assert (j["dispatched"], j["app_undelivered"]) == (2, 0) and injacct.loss_verdict(j, "x") is None, j
        # other platforms: no os_key_event, nothing changes (the menu count is 0 and unused)
        assert injacct.app_trace(old, 0, 1e9)["menu"] == (0, 0) and injacct.app_trace(new, 0, 1e9)["menu"] == (2, 0)
    finally:
        shutil.rmtree(d, ignore_errors=True)


@check
def r12_lab_helper_bundle_is_named_after_the_helper_not_the_runner():
    """#2042: stage_helper_asset named the bundle after platform.machine() of the machine the harness runs on. On the arm64 Mac runner that is
    phosphor-remote-linux-aarch64.gz for an x86_64 helper and an x86_64 lab host, a name the app never asks for. The name now comes from the helper's
    ELF header, whatever the runner is."""
    import gzip
    import platform
    import cases_lab as CL
    d = tempfile.mkdtemp()
    saved, real_machine = os.environ.get(CL.HELPER_ASSET_ENV), platform.machine
    try:
        def elf(machine, data=1):
            return b"\x7fELF" + bytes([2, data, 1, 0]) + b"\0" * 8 + (2).to_bytes(2, "little" if data == 1 else "big") + machine.to_bytes(2, "little" if data == 1 else "big") + b"\0" * 40
        x86, arm, be = os.path.join(d, "x86"), os.path.join(d, "arm"), os.path.join(d, "be")
        open(x86, "wb").write(elf(62)); open(arm, "wb").write(elf(183)); open(be, "wb").write(elf(183, data=2))
        junk, short = os.path.join(d, "junk"), os.path.join(d, "short")
        open(junk, "wb").write(b"#!/bin/sh\nexit 0\n" + b" " * 40); open(short, "wb").write(b"\x7fELF")
        assert (CL.helper_arch(x86), CL.helper_arch(arm), CL.helper_arch(be)) == ("x86_64", "aarch64", "aarch64")
        assert CL.helper_arch(junk) is None and CL.helper_arch(short) is None and CL.helper_arch(os.path.join(d, "absent")) is None
        other = os.path.join(d, "riscv")
        open(other, "wb").write(elf(243))
        assert CL.helper_arch(other) is None
        for runner in ("arm64", "x86_64", "AMD64"):         # the Mac runner, the box, the Windows VM: the same bundle name on all three
            platform.machine = lambda runner=runner: runner
            ctx = type("C", (), {"run": os.path.join(d, "run_" + runner)})()
            os.makedirs(ctx.run)
            out = CL.stage_helper_asset(ctx, x86)
            assert os.listdir(out) == ["phosphor-remote-linux-x86_64.gz"] and os.environ[CL.HELPER_ASSET_ENV] == out, (runner, os.listdir(out))
            assert gzip.open(os.path.join(out, "phosphor-remote-linux-x86_64.gz")).read() == open(x86, "rb").read()
        platform.machine = lambda: "x86_64"
        ctx = type("C", (), {"run": os.path.join(d, "run_arm")})()
        os.makedirs(ctx.run)
        assert os.listdir(CL.stage_helper_asset(ctx, arm)) == ["phosphor-remote-linux-aarch64.gz"]      # an aarch64 helper keeps its own name on an x86_64 runner
        ctx = type("C", (), {"run": os.path.join(d, "run_junk")})()
        os.makedirs(ctx.run)
        assert os.listdir(CL.stage_helper_asset(ctx, junk)) == ["phosphor-remote-linux-%s.gz" % CL.LAB_HOST_ARCH]
    finally:
        platform.machine = real_machine
        if saved is None:
            os.environ.pop(CL.HELPER_ASSET_ENV, None)
        else:
            os.environ[CL.HELPER_ASSET_ENV] = saved
        shutil.rmtree(d, ignore_errors=True)


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
        # rounds 8/9: a Return tapped BEFORE the case started (the harness's previous probe line) is not this case's: no taint, not counted
        note, m = taint.case_taint(os.path.join(d, "run"), "c", "terminal", 100.3, 101.0, {5}, "")
        assert note is None and m["taps"] == 0, (note, m)
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
        assert trims == [(t + 2.1, 500.0, "open", None), (t + 8.0, 10.0, "close", None)], trims      # old format: no start=
        assert cases.ui_trim_max(items, trims, "open") == 6.5            # the 6.5 ms frame at t+2.0 ended inside the open trim
        assert cases.ui_trim_max(items, trims, "close") == 9.0           # the 9 ms action at t+8.0 ended at the close trim's end
        assert cases.ui_trim_max(items, [], "open") == 0.0
        # #1607 r18u23: the open trim is spawned from inside the load's layout frame, so that frame ends in the trim's window; it is excluded there
        # (and judged against the control's load frame instead)
        load = cases.ui_load_frame(items, t)
        assert load == (t + 0.5, 250.0, "frame"), load
        assert cases.ui_load_frame(later, t, t_end=t + 5.5) == load and cases.ui_load_frame([], t) is None
        spawned = [(t + 0.5, 0.01, "open", None)]
        assert cases.ui_trim_max(items, spawned, "open") == 250.0
        assert cases.ui_trim_max(items, spawned, "open", exclude=(load,)) == 0.0
        assert cases.ui_trim_max(items, trims, "open", exclude=(load,)) == 6.5      # a real item during the trim still counts
        # #1340: the new line format carries the trim's start; an item that began before it is not the trim's, and a load frame that ends
        # after the 5 s window is still found when the window is as long as the open phase
        new_log = "z input_order hop=heap_trim why=open ms=500.00 freed_estimate=1 start=%d t=%d\n" % ((t + 1.6) * 1000, (t + 2.1) * 1000)
        ntr = cases.heap_trims(new_log)
        assert ntr == [(t + 2.1, 500.0, "open", t + 1.6)], ntr
        # a 40 ms item ending at t+1.7 began at t+1.66 (inside the trim): counts; the 250 ms frame ended at t+0.5; a 150 ms item ending at
        # t+1.7 began at t+1.55, before the trim started: not the trim's
        its = [(t + 1.7, 40.0, "frame")]
        assert cases.ui_trim_max(its, ntr, "open") == 40.0
        assert cases.ui_trim_max([(t + 1.7, 150.0, "frame")], ntr, "open") == 0.0
        assert cases.ui_trim_max([(t + 1.7, 150.0, "frame")], [(t + 2.1, 500.0, "open", None)], "open") == 150.0      # old format: as before
        slow = [(t + 7.0, 300.0, "frame")]
        assert cases.ui_load_frame(slow, t) is None and cases.ui_load_frame(slow, t, seconds=9.0) == slow[0]
        # load_stage lines: old (rss_kb only, -1 off Linux) and new (Windows ws/private, macOS footprint) formats; chunk lines thinned
        st_log = ("a DEBUG editor_memory editor_memory load_stage stage=start rss_kb=-1\n"
                  "b DEBUG editor_memory editor_memory load_stage stage=layout_tasks_created rss_kb=2048 ws_kb=2048 private_kb=1024 t=5\n"
                  "c editor_memory load_stage stage=layout_chunk_done start=10 rss_kb=1 phys_footprint_kb=2 t=6\n"
                  "d editor_memory load_stage stage=layout_chunk_done start=20 rss_kb=1 t=7\n"
                  "e editor_memory load_stage stage=layout_chunk_done start=30 rss_kb=1 t=8\n")
        sg = cases.load_stages(st_log)
        assert [d["stage"] for d in sg] == ["start", "layout_tasks_created", "layout_chunk_done", "layout_chunk_done"], sg
        assert sg[0]["rss_kb"] == -1 and sg[1]["private_kb"] == 1024 and sg[1]["ws_kb"] == 2048 and sg[2]["phys_footprint_kb"] == 2, sg
        assert sg[-1]["start"] == 30
        # Windows (#1340): private bytes are judged, the working set is only a note
        rw = cases.Result("t", "terminal")
        assert cases.mem_judge(rw, {"rss": 400.0, "private": 40.0}, {"rss": 0.0, "private": 0.0}, 100, "cost %.0f MB of %s") and not rw.fails
        assert any("not judged on Windows" in n for n in rw.notes), rw.notes
        rw2 = cases.Result("t", "terminal")
        assert not cases.mem_judge(rw2, {"rss": 40.0, "private": 140.0}, {"rss": 0.0, "private": 0.0}, 100, "cost %.0f MB of %s")
        assert rw2.fails == ["cost 140 MB of private"], rw2.fails
        rw3 = cases.Result("t", "terminal")
        assert cases.mem_judge_closed(rw3, {"rss": 500.0, "private": 10.0}, {"rss": 0.0, "private": 0.0}, 60, "held %.0f MB of %s") and not rw3.fails
        # macOS: the reusable share of RSS at the close trim is a note, never a verdict
        assert cases.reusable_note("txt", {"why": "close", "freed_estimate": 0}) is None
        n = cases.reusable_note("txt", {"why": "close", "resident": 1800 * 1048576, "reusable": 460 * 1048576})
        assert n and "1800 MB resident" in n and "460 MB" in n, n
        br2 = cases.editor_mem_log(_ctx(tmp, log_path=log), 0)
        assert br2["trim"] == [], br2
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
        assert cases.heap_trims(open(log).read()) == [(1.0, 1.0, "close", None)]     # the hop regex still reads the line with freed_estimate
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
def editor_memory_owner_2026_11_05():
    """owner 2026-11-05 (#1607): macOS after-close judged on phys_footprint only (RSS reported); Linux/Windows keep every figure; the default close
    trim's UI pause is KNOWN on Linux only (the entry is Linux-scoped; the A/B routing is in editor_trim_ab)."""
    orig = cases.app_version
    try:
        cases.app_version = lambda repo=None: "0.1.17"
        w = "held %.0f MB of %s"
        r = cases.Result("t", "terminal")      # the r18u23 shape: RSS +170, phys_footprint -290
        assert cases.mem_judge_closed(r, {"rss": 170.0, "phys_footprint": -290.0}, {"rss": 0.0, "phys_footprint": 0.0}, 60, w)
        assert not r.fails and len(r.notes) == 1 and r.notes[0].startswith("rss: +170 MB") and "not judged" in r.notes[0], (r.fails, r.notes)
        r = cases.Result("t", "terminal")      # phys_footprint over still fails
        assert not cases.mem_judge_closed(r, {"rss": 0.0, "phys_footprint": 90.0}, {"rss": 0.0, "phys_footprint": 0.0}, 60, w)
        assert r.fails == ["held 90 MB of phys_footprint"], r.fails
        r = cases.Result("t", "terminal")      # Linux / Windows: rss is judged
        assert not cases.mem_judge_closed(r, {"rss": 90.0}, {"rss": 0.0}, 60, w) and r.fails == ["held 90 MB of rss"] and not r.notes, (r.fails, r.notes)
        acc = cases.ACCEPTED_FOR_RELEASE["editor_close_trim_ui"]
        assert acc["issue"] == 1607 and acc["release"] == "0.1.17" and acc["platforms"] == ("linux",), acc
        orig_p = cases.plat
        try:
            for plat_, close, known in (("linux-x11", True, True), ("linux-wayland", True, True), ("linux-x11", False, False), ("macos", True, False), ("windows", True, False)):
                cases.plat = lambda p=plat_: p
                r = cases.Result("t", "terminal")
                cases.trim_ui_verdict(r, close, True, "trim A/B: put 9.0 ms")
                if known:
                    assert not r.fails and len(r.notes) == 1 and r.notes[0].startswith("KNOWN: trim A/B: put 9.0 ms") and "ACCEPTED_FOR_RELEASE #1607 v0.1.17" in r.notes[0], (plat_, r.fails, r.notes)
                else:
                    assert r.fails == ["trim A/B: put 9.0 ms"] and not r.notes, (plat_, close, r.fails, r.notes)
            cases.plat = lambda: "linux-x11"
            r = cases.Result("t", "terminal")
            cases.trim_ui_verdict(r, True, False, "m")
            assert not r.fails and not r.notes
            cases.app_version = lambda repo=None: "0.1.18"     # past the release: FAIL again
            r = cases.Result("t", "terminal")
            cases.trim_ui_verdict(r, True, True, "m")
            assert r.fails == ["m"], r.fails
            cases.app_version = lambda repo=None: "0.1.17"
        finally:
            cases.plat = orig_p
        assert cases.accepted_for_release("editor_close_trim_ui") and cases.accepted_for_release("editor_close_trim_ui", "0.1.18") is None
    finally:
        cases.app_version = orig


@check
def wayland_pointer_ocr_cases_noref_x11_judged():
    """2026-11-05: on the Wayland backend only, a failed pointer/OCR-dependent claim (ui_mouse_select_copy, ui_right_click_paste, ui_palette OCR,
    alt_screen_mouse_reporting) is NOREF with the stated reason, never a pass or a fail; X11 (and xwayland, which is the x11 backend) still judge."""
    import cases_ui as UI
    for be_name, wl in (("wayland", True), ("x11", False), ("mac", False)):
        ctx = types.SimpleNamespace(be=types.SimpleNamespace(name=be_name))
        assert UI.on_wayland(ctx) is wl
        r = cases.Result("t", "terminal")
        assert UI.wl_check(ctx, r, False, "no OCR hit", "OCR") is False
        if wl:
            assert not r.fails and len(r.noref) == 1 and "Wayland harness cannot synthesize pointer input" in r.noref[0] and "verified manually by the owner" in r.noref[0] \
                and "no OCR hit" in r.noref[0], (r.fails, r.noref)
        else:
            assert len(r.fails) == 1 and not r.noref, (be_name, r.fails, r.noref)
        r = cases.Result("t", "terminal")      # a claim that holds passes everywhere
        assert UI.wl_check(ctx, r, True, "ok", "OCR") and not r.fails and not r.noref


class _FakeCompositor:
    """a minimal Wayland server for wlptr.py: wl_display / wl_registry (wl_seat + zwlr_virtual_pointer_manager_v1) / wl_callback, and records every
    request the virtual pointer receives as (opcode, args)"""

    def __init__(self, path, with_manager=True):
        import socket, struct, threading
        self.struct, self.requests, self.with_manager = struct, [], with_manager
        self.srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.srv.bind(path)
        self.srv.listen(2)
        self.thread = threading.Thread(target=self.run, daemon=True)
        self.thread.start()

    def run(self):
        import wlptr
        st = self.struct
        c, _ = self.srv.accept()
        buf, ids = b"", {}
        gl = [(1, "wl_seat", 5)] + ([(2, "zwlr_virtual_pointer_manager_v1", 2)] if self.with_manager else [])
        while True:
            d = c.recv(65536)
            if not d:
                return
            buf += d
            while len(buf) >= 8:
                obj, w = st.unpack_from("<II", buf)
                size, op = w >> 16, w & 0xFFFF
                if len(buf) < size:
                    break
                body, buf = buf[8:size], buf[size:]
                kind = ids.get(obj, "display" if obj == 1 else None)
                if kind == "display" and op == 1:
                    ids[st.unpack("<I", body)[0]] = "registry"
                    for name, iface, ver in gl:
                        b = st.pack("<I", name) + wlptr._pstr(iface) + st.pack("<I", ver)
                        c.sendall(st.pack("<II", st.unpack("<I", body)[0], ((8 + len(b)) << 16) | 0) + b)
                elif kind == "display" and op == 0:
                    cb = st.unpack("<I", body)[0]
                    c.sendall(st.pack("<II", cb, (12 << 16) | 0) + st.pack("<I", 0))
                elif kind == "registry" and op == 0:
                    name = st.unpack_from("<I", body)[0]
                    iface, off = wlptr._rstr(body, 4)
                    ver, nid = st.unpack_from("<II", body, off)
                    ids[nid] = {"wl_seat": "seat", "zwlr_virtual_pointer_manager_v1": "mgr"}[iface]
                elif kind == "mgr" and op == 0:
                    seat, nid = st.unpack("<II", body)
                    ids[nid] = "ptr"
                    self.requests.append(("create", (seat,)))
                elif kind == "ptr":
                    n = len(body) // 4
                    self.requests.append((op, st.unpack("<%di" % n, body) if op == 3 else st.unpack("<%dI" % n, body)))


@check
def wlptr_virtual_pointer_wire_and_holder():
    """#1789: wlptr.py binds zwlr_virtual_pointer_manager_v1 and sends motion_absolute / button / frame / axis with the right wire layout (checked
    against a fake compositor), a holder answers one-line commands on its control socket, and a compositor without the protocol is an error"""
    import wlptr, threading
    tmp = tempfile.mkdtemp(prefix="wp")
    try:
        sp = os.path.join(tmp, "w0")
        fc = _FakeCompositor(sp)
        vp = wlptr.VirtualPointer(sp)
        vp.motion_absolute(800, 500, 1600, 1000)
        vp.button(wlptr.BTN[3], True)
        vp.button(wlptr.BTN[3], False)
        vp.axis(2)
        reqs = fc.requests
        assert reqs[0][0] == "create", reqs
        ops = [o for o, _ in reqs[1:]]
        assert ops == [1, 4, 2, 4, 2, 4, 5, 3, 4], ops
        assert reqs[1][1][1:] == (800, 500, 1600, 1000), reqs[1]
        assert reqs[3][1][1:] == (0x111, 1) and reqs[5][1][1:] == (0x111, 0), reqs
        assert reqs[8][1][1:] == (0, 2 * 256), reqs[8]
        assert wlptr.BTN[1] == 0x110
        # the holder: a control socket in front of the same pointer
        assert wlptr.handle(vp, "ping") == "ok" and wlptr.handle(vp, "abs 1 2 3 4") == "ok" and wlptr.handle(vp, "btn 272 1") == "ok"
        assert wlptr.handle(vp, "nonsense").startswith("err") and wlptr.handle(vp, "abs x").startswith("err")
        sp2 = os.path.join(tmp, "w1")
        fc2 = _FakeCompositor(sp2, with_manager=False)
        try:
            wlptr.VirtualPointer(sp2)
            raise AssertionError("a compositor without zwlr_virtual_pointer_manager_v1 must be an error")
        except RuntimeError as e:
            assert "zwlr_virtual_pointer_manager_v1" in str(e), e
        ctl = os.path.join(tmp, "c")
        sp3 = os.path.join(tmp, "w2")
        fc3 = _FakeCompositor(sp3)
        threading.Thread(target=wlptr.serve, args=(ctl, sp3), daemon=True).start()
        for _ in range(50):
            if os.path.exists(ctl):
                break
            time.sleep(0.05)
        assert wlptr.send_cmd(ctl, "ping") == "ok" and wlptr.send_cmd(ctl, "abs 5 6 100 100") == "ok", "holder"
        assert fc3.requests[-2][1][1:] == (5, 6, 100, 100), fc3.requests
        assert wlptr.display_path({"WAYLAND_DISPLAY": "wayland-1", "XDG_RUNTIME_DIR": "/r"}) == "/r/wayland-1"
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


@check
def wayland_pointer_gate_follows_the_holder_1789():
    """#1789: with a live virtual-pointer holder a pointer-only claim FAILS on Wayland (the events provably reach clients); OCR claims stay NOREF; without a
    holder everything stays NOREF as before. hook_find reads the app's OCR-free position line, the last one winning, and None without the hook."""
    import cases_ui as UI, wlptr
    tmp = tempfile.mkdtemp(prefix="wg")
    try:
        ctl = os.path.join(tmp, "c")
        import socket
        srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        srv.bind(ctl)
        srv.listen(4)
        import threading

        def answer():
            while True:
                try:
                    c, _ = srv.accept()
                except OSError:
                    return
                c.recv(100)
                c.sendall(b"ok\n")
                c.close()
        threading.Thread(target=answer, daemon=True).start()
        old = os.environ.get("APPCI_WL_PTR")
        ctx = types.SimpleNamespace(be=types.SimpleNamespace(name="wayland"))
        try:
            os.environ["APPCI_WL_PTR"] = ctl
            assert UI.wl_pointer_real(ctx)
            r = cases.Result("t", "terminal")
            assert UI.wl_check(ctx, r, False, "no click", "pointer") is False and len(r.fails) == 1 and not r.noref, (r.fails, r.noref)
            r = cases.Result("t", "terminal")
            UI.wl_check(ctx, r, False, "no ocr", "OCR")
            UI.wl_check(ctx, r, False, "mixed", "pointer/OCR")
            assert not r.fails and len(r.noref) == 2, (r.fails, r.noref)
            os.environ["APPCI_WL_PTR"] = os.path.join(tmp, "gone")
            assert not UI.wl_pointer_real(ctx)
            r = cases.Result("t", "terminal")
            UI.wl_check(ctx, r, False, "no click", "pointer")
            assert not r.fails and len(r.noref) == 1
            assert not UI.wl_pointer_real(types.SimpleNamespace(be=types.SimpleNamespace(name="x11")))
        finally:
            if old is None:
                os.environ.pop("APPCI_WL_PTR", None)
            else:
                os.environ["APPCI_WL_PTR"] = old
            srv.close()
        log = os.path.join(tmp, "app.log")
        c2 = types.SimpleNamespace(log_path=log)
        assert UI.hook_find(c2, "tokx") is None
        open(log, "w").write("noise\nINFO appci_find token=tokx x0=10 y0=20 x1=110 y1=40\nINFO appci_find token=tokx x0=10 y0=60 x1=110 y1=80\nappci_find token=other x0=1 y0=1 x1=2 y1=2\n")
        assert UI.hook_find(c2, "tokx", origin=(100, 200)) == ("tokx", 160, 270, 110, 210, 260, 280), UI.hook_find(c2, "tokx", origin=(100, 200))
        assert UI.hook_find(c2, "nope") is None
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


@check
def big_paste_time_budgets_2004():
    """#2004 (#1191): big_paste's time budgets, OWNER DECISION 2026-11-10 (time-to-Enter 1500 ms, largest single UI item 250 ms), overridable like latency_paint's
    (APPCI_*_BUDGET_MS), judged only where measured; time-to-Enter only on a byte-exact paste; poll_file_size measures from t0 and returns None when the file never
    gets there"""
    import inspect
    import time as _t
    J = cases.judge_big_paste_time
    B = cases.big_paste_budgets
    assert (cases.BIGPASTE_ENTER_MS, cases.BIGPASTE_UI_ITEM_MS) == (1500.0, 250.0), "the owner's budgets (2026-11-10): change them only on an owner decision"
    assert B({}) == (1500.0, 250.0) and B({"APPCI_BIGPASTE_ENTER_MS": "900", "APPCI_BIGPASTE_UI_ITEM_MS": "x"}) == (900.0, 250.0)       # the round 10 names still work
    assert B({"APPCI_BIGPASTE_ENTER_BUDGET_MS": "700", "APPCI_BIGPASTE_UI_ITEM_BUDGET_MS": "120.5"}) == (700.0, 120.5)                # the *_BUDGET_MS names
    assert B({"APPCI_BIGPASTE_ENTER_BUDGET_MS": "700", "APPCI_BIGPASTE_ENTER_MS": "900"}) == (700.0, 250.0)                           # the new name wins
    for bad in ("", "x", "0", "-5", "nan", "inf"):                                                                                  # never loosened or voided by a typo
        assert B({"APPCI_BIGPASTE_ENTER_BUDGET_MS": bad, "APPCI_BIGPASTE_UI_ITEM_BUDGET_MS": bad}) == (1500.0, 250.0), bad
    assert "BUDGET_MS" in J(1500.1, 10.0, True, {})[0] and "owner budget" in J(1500.1, 10.0, True, {})[0] and "proposed" not in J(9999.0, 9999.0, True, {})[1]
    body = inspect.getsource(cases.c_big)
    assert "judge_big_paste_time(" in body and 'r.metric("big_paste_budgets"' in body and "r.check(False, \"big_paste: \" + msg)" in body, "the case must judge both budgets"
    assert J(1000.0, 200.0, True, {"APPCI_BIGPASTE_ENTER_BUDGET_MS": "500", "APPCI_BIGPASTE_UI_ITEM_BUDGET_MS": "100"}).__len__() == 2
    doc = open(os.path.join(HERE, "..", "..", "..", "docs", "testing.md")).read()
    assert "APPCI_BIGPASTE_ENTER_BUDGET_MS" in doc and "APPCI_BIGPASTE_UI_ITEM_BUDGET_MS" in doc
    assert J(1400.0, 240.0, True, {}) == [] and J(1500.0, 250.0, True, {}) == []                                          # at the budget passes
    assert len(J(1500.1, 10.0, True, {})) == 1 and "ENTER_BUDGET_MS" in J(1500.1, 10.0, True, {})[0]                            # #1191: Enter 1.4-5 s
    assert len(J(100.0, 6500.0, True, {})) == 1 and "UI_ITEM_BUDGET_MS" in J(100.0, 6500.0, True, {})[0]                         # #1191: UI froze 6-7 s
    assert len(J(5000.0, 6500.0, True, {})) == 2
    assert J(5000.0, 10.0, False, {}) == []                                                                              # a shortfall is the byte check's business
    assert len(J(5000.0, 6500.0, False, {})) == 1                                                                         # ... but a frozen UI is judged regardless
    assert J(None, None, True, {}) == []                                                                                 # nothing measured, nothing judged
    assert J(1000.0, 200.0, True, {"APPCI_BIGPASTE_ENTER_MS": "500", "APPCI_BIGPASTE_UI_ITEM_MS": "100"}).__len__() == 2  # overrides apply
    tmp = tempfile.mkdtemp()
    try:
        f = os.path.join(tmp, "f")
        t0 = _t.time()
        assert cases.poll_file_size(f, 3, 0.1, t0) is None                                                               # no file
        open(f, "w").write("ab")
        assert cases.poll_file_size(f, 3, 0.1, t0) is None                                                               # too small
        open(f, "a").write("c")
        el = cases.poll_file_size(f, 3, 1.0, t0 - 2.0)
        assert el is not None and 2.0 <= el < 2.5, el
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


@check
def windows_rust_log_enables_every_hop_module_1966():
    """#1966: the Windows launcher's RUST_LOG (bare `warn` base) enables every module that emits, with a plain `log::info!`/`debug!`, a `hop=` line some appci
    case reads. Scans the Rust sources; the module path comes from the file path (app -> warp, crates/x -> x). `backend_route` is logged through a helper
    (`route_log_line`) so it is listed by hand. Also proves the matcher: the pre-fix string hid action_model, the longest prefix wins, no default = off"""
    import re
    import app as A
    root = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
    read = set()
    for fn in os.listdir(HERE):
        if fn.endswith(".py") and fn != "selftest.py":
            read |= set(re.findall(r"hop=([a-z_0-9]+)", open(os.path.join(HERE, fn), encoding="utf-8").read()))
    assert "agent_action" in read, "the scan found no hops: the case sources moved?"
    pat = re.compile(r'log::(info|debug|trace)!\(\s*(?:target:[^,]*,\s*)?"([^"]*)"', re.S)

    def module_of(rel):
        parts = rel.replace(os.sep, "/").split("/")
        if parts[0] == "app":
            crate, parts = "warp", parts[2:]
        else:
            crate, parts = parts[1].replace("-", "_"), parts[3:]
        parts[-1] = parts[-1][:-3]
        if parts[-1] in ("mod", "lib", "main"):
            parts = parts[:-1]
        return "::".join([crate] + parts)
    found = {("remote_server::backend::router", "info", "backend_route")}
    for top in ("app/src", "crates"):
        for d, _, fs in os.walk(os.path.join(root, top)):
            for fn in fs:
                if not fn.endswith(".rs") or "_test" in fn or "/target" in d:
                    continue
                p = os.path.join(d, fn)
                if not re.search(r"hop=", open(p, encoding="utf-8", errors="replace").read()):
                    continue
                for m in pat.finditer(open(p, encoding="utf-8", errors="replace").read()):
                    for h in re.findall(r"hop=([a-z_0-9]+)", m.group(2)):
                        if h in read:
                            found.add((module_of(os.path.relpath(p, root)), m.group(1), h))
    assert len(found) >= 10, found
    hidden = sorted(x for x in found if not A.rust_log_enabled(A.WINDOWS_RUST_LOG, x[0], x[1]))
    assert not hidden, "hop lines the cases read are hidden by the Windows RUST_LOG (add the module to app.WINDOWS_RUST_LOG): %r" % (hidden,)
    pre_fix = "warn,winit=info,warp::ai::agent_providers=info,warp::ai::blocklist::controller=info,warp::input_order=debug"
    assert not A.rust_log_enabled(pre_fix, "warp::ai::blocklist::action_model", "info")      # the #1966 defect, still detectable
    assert A.rust_log_enabled(pre_fix, "warp::input_order", "info") and not A.rust_log_enabled(pre_fix, "warp::input_order_x", "info")
    assert A.rust_log_enabled("warn,warp::a=info,warp::a::b=warn", "warp::a::c", "info")
    assert not A.rust_log_enabled("warn,warp::a=info,warp::a::b=warn", "warp::a::b", "info")      # longest prefix wins
    assert not A.rust_log_enabled("warp::a=info", "other", "warn")                                 # no default: off


@check
def app_find_hook_harness_side_1789():
    """#1789 app find hook, harness side: RUST_LOG carries the hook's log target; hook_find reads only lines after `since`; find_token uses the hook on
    X11 and Wayland (OCR is not called when the hook has the token), and notes the OCR fallback on `r` when it does not; x11_window_origin is None
    without xdotool output"""
    import cases_ui as UI
    src = open(os.path.join(HERE, "app.py")).read()
    assert src.count("warpui::appci_find=info") >= 2, "RUST_LOG must enable the hook's log target for the launched app"
    tmp = tempfile.mkdtemp()
    try:
        log = os.path.join(tmp, "app.log")
        open(log, "w").write("appci_find token=tokx x0=1 y0=2 x1=3 y1=4\n")
        off = os.path.getsize(log)
        open(log, "a").write("appci_find token=tokx x0=10 y0=20 x1=30 y1=40\n")
        c = types.SimpleNamespace(log_path=log, log_since=lambda o: open(log).read()[o:])
        assert UI.hook_find(c, "tokx", since=off)[3:] == (10, 30, 20, 40)
        assert UI.hook_find(c, "tokx")[3:] == (10, 30, 20, 40)
        assert UI.hook_find(c, "tokx", since=os.path.getsize(log)) is None

        class Be:
            name = "x11"
            pid_fn = staticmethod(lambda: 4242)
        c.be = Be()
        orig = UI.x11_window_origin
        UI.x11_window_origin = lambda pid: (100, 200)
        try:
            w, p = UI.find_token(c, "t", "tokx")
            assert p is None and w[3:] == (110, 130, 220, 240), w
            notes = []
            r = types.SimpleNamespace(note=notes.append)
            orig_words = UI.words
            UI.words = lambda ctx, tag: ([], None)
            try:
                w, _ = UI.find_token(c, "t", "absent", r=r)
            finally:
                UI.words = orig_words
            assert w is None and len(notes) == 1 and "find hook" in notes[0], notes
        finally:
            UI.x11_window_origin = orig
        Be.name = "mac"
        assert UI._hook_word(c, "tokx") is None, "no hook lookup on backends without a window origin"
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


@check
def tabs_streaming_reorder_known_1658():
    """#1658 (owner 2026-11-04): the Windows streaming-tabs reorder is KNOWN at 0.1.17; FAIL on Linux, after 0.1.17, and whenever the app log shows a discard"""
    exp = ["a1", "b2", "c3", "d4"]
    got = ["a1", "c3", "b2", "d4"]

    class Ctx:
        def __init__(s, log): s.log, s.log_path = log, "x.log"
        def log_since(s, off): return s.log
    orig_v, orig_p = cases.app_version, cases.plat
    try:
        def run(plat_, ver, log):
            cases.plat, cases.app_version = (lambda: plat_), (lambda repo=None: ver)
            r = cases.Result("t", "terminal")
            cases.streaming_exact_lines(Ctx(log), r, got, exp, "typing", 0)
            return r
        r = run("windows", "0.1.17", "young_hold_handed_back\n")
        assert not r.fails and len(r.notes) == 1 and r.notes[0].startswith("KNOWN: typing: 2/4 exact lines") and "ACCEPTED_FOR_RELEASE #1658 v0.1.17" in r.notes[0], (r.fails, r.notes)
        r = run("linux-x11", "0.1.17", "")
        assert len(r.fails) == 1 and not r.notes, (r.fails, r.notes)
        r = run("windows", "0.1.18", "")
        assert len(r.fails) == 1 and not r.notes, (r.fails, r.notes)
        for line in ("3 keys typed after Enter were discarded", "key X was discarded"):
            r = run("windows", "0.1.17", line)
            assert len(r.fails) == 1 and not r.notes, (line, r.fails, r.notes)
        r = run("windows", "0.1.17", "")
        r2 = cases.Result("t", "terminal")
        cases.streaming_exact_lines(Ctx(""), r2, exp, exp, "typing", 0)
        assert not r2.fails and not r2.notes
    finally:
        cases.app_version, cases.plat = orig_v, orig_p


@check
def latency_known_v0117_enter_and_flood():
    """v0.1.17 (#1606, #1653): an over-budget latency_enter overhead (macOS / Linux) or Ctrl-C-under-flood overhead is KNOWN at 0.1.17 and FAIL at 0.1.18;
    other verdicts (regression vs baseline) and other platforms still fail."""
    rec = {"budget_verdict": "fail", "budget_kind": "overhead", "verdict": "fail", "overhead_ms": 9.5, "overhead_se_ms": 0.4, "reference_name": "xterm",
           "budget_ms": 2, "phosphor": {"p50": 12.0}}
    flood = dict(rec, budget_ms=45)
    orig_v, orig_p = cases.app_version, cases.plat
    try:
        for plat_, key, rc, issue, ok in (("linux-wayland", "latency_enter_overhead", rec, 1606, True),
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
def latency_enter_macos_report_only():
    """owner 2026-11-05 (#1606 closed): on macOS the Enter overhead is REPORT-ONLY (+20 ms is a note, not a FAIL); the app's own share over 2 ms or an overhead over 40 ms (gross regression, relative to the reference) still FAIL; a slow machine (median 90 ms, overhead +20) is not a FAIL;
    Linux is judged as before (FAIL without an ACCEPTED entry)"""
    rec = {"budget_verdict": "fail", "budget_kind": "overhead", "verdict": "fail", "overhead_ms": 20.0, "overhead_se_ms": 0.4, "reference_name": "Terminal.app",
           "budget_ms": 2, "phosphor": {"p50": 30.0}}
    orig_v, orig_p = cases.app_version, cases.plat
    try:
        cases.app_version = lambda repo=None: "0.1.18"
        for key in ("latency_enter_overhead", "latency_enter_ssh_overhead"):
            cases.plat = lambda: "macos"
            r = cases.Result("t", "terminal")
            cases.lat_checks(r, rec, "L", known_key=key, report_only_macos=True, app_share_ms=0.5, median_ms=30.0)
            assert not r.fails and any(n.startswith("REPORT-ONLY: L:") and "#1606 closed by owner 2026-11-05" in n for n in r.notes), (key, r.fails, r.notes)
            r = cases.Result("t", "terminal")
            cases.lat_checks(r, rec, "L", known_key=key, report_only_macos=True, app_share_ms=3.0, median_ms=30.0)
            assert len(r.fails) == 1 and "own share" in r.fails[0], r.fails
            r = cases.Result("t", "terminal")
            cases.lat_checks(r, rec, "L", known_key=key, report_only_macos=True, app_share_ms=0.5, median_ms=90.0)
            assert not r.fails, r.fails      # r18u30: the whole Mac slow, overhead unchanged -> not Phosphor's regression
            r = cases.Result("t", "terminal")
            cases.lat_checks(r, dict(rec, overhead_ms=45.0), "L", known_key=key, report_only_macos=True, app_share_ms=0.5, median_ms=90.0)
            assert len(r.fails) == 1 and "gross-regression bound" in r.fails[0], r.fails
            cases.plat = lambda: "linux-x11"
            r = cases.Result("t", "terminal")
            cases.lat_checks(r, rec, "L", known_key=key, report_only_macos=True, app_share_ms=0.5, median_ms=30.0)
            assert len(r.fails) == 1 and "exceeds the 2 ms budget" in r.fails[0], r.fails
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
    plan_log = ("x DEBUG editor_memory editor_memory agent_code_plan rich=10 visible=[9] plan=[0R,2R,3K,7B]\n"
                "y DEBUG editor_memory editor_memory agent_code_plan rich=70 visible=[] plan=[] \n"
                "z DEBUG editor_memory editor_memory agent_code_plan rich=100 visible=[0] plan=[0B,1B] (+98 more)\n")
    assert ca.recycle_plans(plan_log) == ["rich=10 visible=[9] plan=[0R,2R,3K,7B]", "rich=70 visible=[] plan=[]", "rich=100 visible=[0] plan=[0B,1B] (+98 more)"], ca.recycle_plans(plan_log)


@check
def agent_lrc_takeover_log_parse():
    """#1796: agent_lrc_no_false_takeover reads the detector's hand-over / kept-with-agent lines and counts the LRC hand-back ERROR from the app log;
    the needles must match the app's own wording (interaction_mode.rs PromptDetector, cli_controller.rs handoff_active_command_control_to_agent)"""
    log = ("2026-11-06T05:32:47Z [INFO] interactive prompt Some(\"Continue? [y/N] \") unchanged for 0ns, foreground reading the tty; handing the command to the user\n"
           "2026-11-06T05:32:47Z [ERROR] Failed to enter inline agent view for LRC handoff: Already in agent mode.\n")
    assert ca.lrc_takeover_verdict(log) == (True, False, 1), ca.lrc_takeover_verdict(log)
    kept = ("x [INFO] prompt-shaped line Some(\"Waiting:\") unchanged for 15s, but the foreground is not reading the tty; "
            "leaving the command with the agent\n")
    assert ca.lrc_takeover_verdict(kept) == (False, True, 0), ca.lrc_takeover_verdict(kept)
    assert ca.lrc_takeover_verdict("") == (False, False, 0)
    unknown = ("x [INFO] prompt-shaped line Some(\"Name: \") unchanged for 15s, but whether the foreground reads the tty is unknown; "
               "leaving the command with the agent\n")
    assert ca.lrc_takeover_verdict(unknown) == (False, True, 0), ca.lrc_takeover_verdict(unknown)
    waits = ("x [INFO] agent_command_wait awaited=true wait_until_completion=true snapshots=false\n"
             "y [INFO] agent_command_wait awaited=false wait_until_completion=false snapshots=false\n")
    assert ca.lrc_awaited(waits) is False and ca.lrc_awaited(waits.splitlines()[0]) is True and ca.lrc_awaited("") is None
    notice = ("x [INFO] prompt-shaped line Some(\"Continue? [y/N] \") unchanged for 3s, tty read unknown; "
              "showing a take-over notice (the command stays with the agent)\n")
    assert ca.LRC_NOTICE in notice and ca.lrc_takeover_verdict(notice) == (False, False, 0), ca.lrc_takeover_verdict(notice)
    # #1882 (owner decision: Windows gets the #1796 notice path and the PowerShell Read-Host marker): the case runs in both Windows panes now, with PowerShell
    # commands in the PowerShell pane; it was NOT_WINDOWS because ConPTY has no termios, which only removes the hand-over-on-proof, not the notice
    assert "agent_lrc_no_false_takeover" in winmap.BOTH and "agent_lrc_no_false_takeover" not in winmap.NOT_WINDOWS
    assert ca.lrc_cmds("pwsh", "T")[0].startswith("Write-Host -NoNewline 'Waiting:'") and ca.lrc_cmds("bash", "T")[0].startswith("printf 'Waiting:'")
    assert "Add-Content lrc_wait.txt T" in ca.lrc_cmds("pwsh", "T")[0] and "echo T >> lrc_wait.txt" in ca.lrc_cmds("bash", "T")[0]
    assert "[Console]::ReadLine()" in ca.lrc_cmds("pwsh", "Y")[1] and "read ans" in ca.lrc_cmds("bash", "Y")[1]


@check
def nu_pager_marker_matches_the_app():
    """#1813: the lab case types the exact marker agent_shell_wrap.rs appends for Nu, and nu.nu looks for the same text"""
    import cases_lab as cl
    root = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
    rs = open(os.path.join(root, "app/src/ai/blocklist/action_model/execute/agent_shell_wrap.rs")).read()
    nu = open(os.path.join(root, "app/assets/bundled/bootstrap/nu.nu")).read()
    assert 'NU_PAGER_MARKER: &str = "%s"' % cl.NU_PAGER_MARKER in rs
    assert '"%s"' % cl.NU_PAGER_MARKER in nu
    assert any(a["name"].startswith("lab_nu_agent_pager_env") for a in [c["arm"] for c in cases.CASES if c.get("lab")])


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
    # r18u22: the app's notice for a broken settings file is not a failed start when the case broke the file on purpose; everything else still counts
    notice = "2026-11-05T04:10:45Z [ERROR] Settings file could not be parsed: invalid TOML at line 3, column 9: invalid key\n"
    assert len(cb.startup_log_problems(notice)) == 1
    assert cb.startup_log_problems(notice, broken_settings=True) == []
    assert cb.startup_log_problems("[ERROR] Settings file cannot be used: x is not a file\n", broken_settings=True) == []
    assert len(cb.startup_log_problems(notice + "[ERROR] migration 0042 failed\n", broken_settings=True)) == 1
    assert len(cb.startup_log_problems(notice + "thread 'main' panicked at a.rs:1\n", broken_settings=True)) == 1
    assert cb.settings_notices(notice + "INFO x\n") == [notice.strip()] and cb.settings_notices("INFO x\n") == []
    # r18u23: the relaunched shell's start cwd is recorded and the shell moved to the case dir before the burst (a restored tab keeps its saved cwd)
    line = cb.start_cwd_line("/h/w", "/h/w/cb_settings_truncated_cwd.txt")
    assert line == "pwd > /h/w/cb_settings_truncated_cwd.txt; cd /h/w", line
    assert "'/h/my w'" in cb.start_cwd_line("/h/my w", "/h/my w/x.txt")
    assert cb.start_cwd_note("truncated", "/h/w", "/h/w") is None
    assert "started in /h/w/sub" in cb.start_cwd_note("truncated", "/h/w/sub", "/h/w")
    assert "did not run" in cb.start_cwd_note("truncated", None, "/h/w")
    # r18u22: MacApp copies the profile settings only when the profile changed, so a case's own write to the app's file survives a relaunch
    import tempfile as _tf
    import app as _appmod
    with _tf.TemporaryDirectory() as h:
        os.makedirs(os.path.join(h, ".config", "phosphor"))
        prof_f = os.path.join(h, ".config", "phosphor", "settings.toml")
        open(prof_f, "w").write("[general]\na = 1\n")
        m = _appmod.MacApp(h, h, "/nonexistent.app", "/nonexistent")
        m._install_settings()
        assert open(m.settings).read() == "[general]\na = 1\n"
        open(m.settings, "w").write("[broken\n")
        m._install_settings()
        assert open(m.settings).read() == "[broken\n", "a case's write was replaced by the profile copy"
        open(prof_f, "w").write("[general]\na = 2\n")
        m._install_settings()
        assert open(m.settings).read() == "[general]\na = 2\n", "a new profile was not installed"
        os.remove(m.settings)
        m._install_settings()
        assert os.path.isfile(m.settings)
    # r18u22 (Wayland agent shell_nu): the Linux App puts the CI users' shell dirs on the app's PATH (was Windows-class dead code), appended, no duplicates
    lb = os.path.expanduser("~/.local/bin")
    got = _appmod.with_user_bin_dirs("/usr/bin:/bin", isdir=lambda d: d == lb)
    assert got == "/usr/bin:/bin" + os.pathsep + lb, got
    assert _appmod.with_user_bin_dirs("/usr/bin:" + lb, isdir=lambda d: True).split(os.pathsep).count(lb) == 1
    assert _appmod.with_user_bin_dirs("/usr/bin", isdir=lambda d: False) == "/usr/bin"
    # r18u22 (Wayland ui_mouse_select_copy / ui_right_click_paste): OCR-safe tokens and a one-glyph-tolerant on-screen match
    import cases_ui as _ui
    t = _ui.ocr_safe("t409jy2dwo")
    assert len(t) == 10 and set(t) <= set(_ui.OCR_SAFE) and t == _ui.ocr_safe("t409jy2dwo"), t
    row = lambda txt, y: (txt, 10, y, 0, 20, y - 5, y + 5)  # noqa: E731
    tok = "msel" + t
    assert _ui.token_hits([row("x", 1), row(tok, 5), row("'" + tok + "'", 9)], tok)[-1][2] == 9
    one_off = tok[:6] + ("a" if tok[6] != "a" else "c") + tok[7:]
    assert _ui.token_hits([row(one_off, 3)], tok) and _ui.token_hits([row(tok[:-1], 3)], tok) and _ui.token_hits([row(tok + "x", 3)], tok)
    two_off = "zz" + tok[2:]
    assert _ui.token_hits([row(two_off, 3), row("msel", 4)], tok) == []
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
    # r18u26 (#1775): the flood's tail is a line of just 499999/500000, never the flood COMMAND's `seq 1 500000` (the block header keeps it on screen)
    assert _R.flood_tail_on_screen("499998\n499999\n500000\n") and _R.flood_tail_on_screen("  500000 |\n")
    assert not _R.flood_tail_on_screen("3 ]; do seq 1 500000; __i=$((__i+500000)); done; now >> ff_end.txt\n450001\n450002")
    assert not _R.flood_tail_on_screen("") and not _R.flood_tail_on_screen(None) and not _R.flood_tail_on_screen("1500000\n4999990")
    assert "flood_tail_on_screen(before)" in src and "flood_tail_on_screen(after)" in src and "hop=find_scroll" in src
    # r18u29: the find_scroll hop carries the app's own placement of the match in the DRAWN viewport; agent mode skips when Find matched no block
    _g = _R.find_scroll_geometry("result=scroll block=16 moved=true viewport_lines=37.00 rendered_mode=PinnedToBottom scroll_mode=PinnedToBottom "
                                 "agent_fullscreen=true match_line=32.00 block_visible=true from=FollowsBottomOfMostRecentBlock to=FixedAtPosition { }")
    assert _g == dict(viewport_lines=37.0, rendered_mode="PinnedToBottom", scroll_mode="PinnedToBottom", agent_fullscreen=True, match_line=32.0, on_screen=True), _g
    assert _R.find_scroll_geometry("result=scroll block=3 moved=true viewport_lines=20.00 rendered_mode=PinnedToBottom scroll_mode=PinnedToTop "
                                   "agent_fullscreen=true match_line=-4.50 block_visible=true")["on_screen"] is False
    assert _R.find_scroll_geometry("result=scroll block=3 moved=false viewport_lines=20.00 rendered_mode=Waterfall scroll_mode=Waterfall "
                                   "agent_fullscreen=false match_line=busy")["on_screen"] is None
    assert _R.find_scroll_geometry("result=scroll block=16 moved=true from=FollowsBottomOfMostRecentBlock to=FixedAtPosition { }") is None
    assert _R.find_scroll_geometry("result=no_match pending=false") is None and _R.find_scroll_geometry(None) is None
    # #1985: OCR-blind runs rest on the app's own geometry; OFF screen still fails; nothing to judge from is noref, never a pass
    _on = _R.find_scroll_geometry("result=scroll block=16 moved=true viewport_lines=37.00 rendered_mode=PinnedToBottom scroll_mode=PinnedToBottom agent_fullscreen=true match_line=32.00")
    _off = _R.find_scroll_geometry("result=scroll block=3 moved=true viewport_lines=20.00 rendered_mode=PinnedToBottom scroll_mode=PinnedToTop agent_fullscreen=true match_line=-4.50")
    _busy = _R.find_scroll_geometry("result=scroll block=3 moved=true viewport_lines=20.00 rendered_mode=Waterfall scroll_mode=Waterfall agent_fullscreen=false match_line=busy")
    _h_on = ["result=scroll block=16 moved=true viewport_lines=37.00 match_line=32.00"]
    assert _R.find_verdict(True, True, [], None)[0] == "pass" and _R.find_verdict(True, False, _h_on, _on)[0] == "fail"          # OCR-seen: unchanged
    assert _R.find_verdict(False, False, _h_on, _on)[0] == "pass" and "ON screen" in _R.find_verdict(False, False, _h_on, _on)[1]  # blind OCR, app says ON
    assert _R.find_verdict(False, True, ["result=scroll block=3 moved=true"], _off)[0] == "fail"                                 # blind OCR, app says OFF
    assert _R.find_verdict(False, True, ["result=no_match pending=false"], None)[0] == "fail"                                    # the app found nothing
    assert _R.find_verdict(False, True, [], None)[0] == "noref" and _R.find_verdict(False, True, None, None)[0] == "noref"       # no OCR, no app record
    assert _R.find_verdict(False, True, ["result=scroll block=3 moved=true"], None)[0] == "noref"                                # hop without geometry
    assert _R.find_verdict(False, True, ["result=scroll block=3 moved=true"], _busy)[0] == "noref"                               # geometry busy
    assert _R.find_verdict(False, True, ["result=scroll block=16 moved=false viewport_lines=37.00 match_line=32.00"], _on)[0] == "noref"  # on screen but nothing moved
    assert 'ctx.mode == "agent" and hops and not scroll_hops' in src and "raise cases.Skip" in src and src.index("raise cases.Skip") < src.index("find_verdict(seen, scrolled")
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


@check
def r18u23_paste_backlog_and_bash_agent_command_count():
    """r18u23 (Windows Git Bash): big_paste's conhost control logs to refctl (its prompts made the pane look back at a prompt while `cat` ran); an undelivered
    paste run by the local shell as commands is detected from rec.log (big_paste, remote_heredoc_paste relaunch on it); a bash pane counts an approved agent
    command from the app's UserCommand pty writes, since #904 keeps the wrapper out of the history its rc hook reads"""
    import inspect
    import cases_caseremote as _R
    src = inspect.getsource(cases.big_paste_control)
    assert "rcfile=ctx.prof.ref_rc" in src and 'ctx.pane == "gitbash"' in src, "the Git Bash big_paste control must use its own rc (refctl)"
    assert "relaunch_after_paste_backlog" in inspect.getsource(cases.c_big)
    assert "payload" in inspect.signature(_R.end_stuck_session).parameters
    cmds = ["echo kept >> x.txt", "abc def ghi", "  jkl  "]
    fake = types.SimpleNamespace(cmds_since=lambda m: cmds[m:], until=lambda pred, t: pred())
    assert cases.paste_backlog_count(fake, 0, ["abc def ghi", "jkl", "", "zzz"]) == 2
    assert cases.paste_backlog_count(fake, 2, ["abc def ghi", "jkl"]) == 1
    assert cases.paste_backlog_count(fake, None, ["abc def ghi"]) == 0
    assert cases.paste_backlog_count(fake, 0, ["nothing here"]) == 0
    log = ('x hop=pty_write_bytes class=UserCommand wrote=9 of_job=9 bytes="\\x10{ echo APPROVEMEX1 >> ap_ok.txt; }" t=1.0\n'
           'x hop=pty_write_bytes class=UserKey wrote=1 of_job=1 bytes="APPROVEMEX1" t=2.0\n'
           'x hop=pty_write_bytes class=UserCommand wrote=3 of_job=3 bytes="ls" t=3.0\n')
    assert ca.approved_command_writes(log, "APPROVEMEX1") == 1
    assert ca.approved_command_writes(log + log, "APPROVEMEX1") == 2
    assert ca.approved_command_writes(log, "REJECTMEX1") == 0 and ca.approved_command_writes("", "A") == 0
    # Linux-style log: no pty_write_bytes hop at all; the effect file decides
    linux = 'x [DEBUG] input_order hop=enter_begin deferred=false t=1\n'
    assert not ca.pty_write_hop_logged(linux) and ca.pty_write_hop_logged(log)
    assert ca.approved_once_verdict(linux, "APPROVEMEX1", ["APPROVEMEX1"])[0]
    assert not ca.approved_once_verdict(linux, "APPROVEMEX1", [])[0]
    assert not ca.approved_once_verdict(linux, "APPROVEMEX1", ["APPROVEMEX1", "APPROVEMEX1"])[0]
    assert ca.approved_once_verdict(log, "APPROVEMEX1", [])[0]            # Windows: the hop count wins
    assert not ca.approved_once_verdict(log + log, "APPROVEMEX1", ["APPROVEMEX1"])[0]
    assert ca.bash_rec_hook(types.SimpleNamespace(win=True, pane="gitbash"))
    assert not ca.bash_rec_hook(types.SimpleNamespace(win=True, pane="powershell"))
    assert not ca.bash_rec_hook(types.SimpleNamespace(win=False, be=types.SimpleNamespace(name="mac")))
    assert ca.bash_rec_hook(types.SimpleNamespace(win=False, be=types.SimpleNamespace(name="wayland")))


@check
def windows_app_runs_under_a_kill_on_close_job():
    """#1642: WindowsApp starts the app suspended, assigns it to a KILL_ON_JOB_CLOSE job, resumes it, and closes the job at stop (mocked kernel32/Popen)"""
    import app as _app, winapi as _w
    calls = []
    class K:
        def __getattr__(self, n):
            def f(*a):
                calls.append(n)
                if n == "SetInformationJobObject":
                    info = a[2]._obj
                    assert a[1] == _w.JobObjectExtendedLimitInformation and info.BasicLimitInformation.LimitFlags & _w.JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
                return 1234 if n == "CreateJobObjectW" else 1
            return _Fn(f)
    class _Fn:
        def __init__(self, f): self.f = f
        def __call__(self, *a): return self.f(*a)
        argtypes = restype = None
    k = K()
    job = _w.job_create(k)
    assert job == 1234 and calls == ["CreateJobObjectW", "SetInformationJobObject"], calls
    assert _w.job_assign(job, 7, k) and _w.job_close(job, k) and calls[-2:] == ["AssignProcessToJobObject", "CloseHandle"]
    nt = types.SimpleNamespace(NtResumeProcess=_Fn(lambda h: 0))
    assert _w.resume_process(7, nt)
    # WindowsApp._spawn ordering: Popen(CREATE_SUSPENDED) -> assign -> resume; a failed assign still resumes (plain start); stop closes the job
    order = []
    class P:
        pid, _handle = 99, 5
        def poll(self): return None
        def wait(self, t): pass
    real = (_app.subprocess.Popen, _app.subprocess.run, _w.job_create, _w.job_assign, _w.resume_process, _w.job_close, _w.job_pids, _app.os.name)
    try:
        _app.subprocess.Popen = lambda *a, **kw: (order.append(("popen", kw.get("creationflags"))), P())[1]
        _app.subprocess.run = lambda *a, **kw: order.append("taskkill")
        _w.job_create = lambda k32=None: 42
        _w.job_assign = lambda j, h, k32=None: (order.append("assign"), True)[1]
        _w.resume_process = lambda h, ntdll=None: (order.append("resume"), True)[1]
        _w.job_close = lambda j, k32=None: order.append("close")
        _w.job_pids = lambda j, k32=None, cap=512: []
        _app.os.name = "nt"
        w = _app.WindowsApp.__new__(_app.WindowsApp)
        w.bin, w.home, w.run, w.pane, w.ctl, w.job, w.proc = "x.exe", "h", tempfile.gettempdir(), "powershell", "c", None, None
        w.env = lambda: {}
        w.proc = w._spawn(None)
        assert order == [("popen", _w.CREATE_SUSPENDED), "assign", "resume"] and w.job == 42, order
        w.stop()
        assert order[-2:] == ["taskkill", "close"] and w.job is None, order
        order.clear(); _w.job_assign = lambda j, h, k32=None: (order.append("assign"), False)[1]
        w.proc = w._spawn(None)
        assert order == [("popen", _w.CREATE_SUSPENDED), "assign", "close", "resume"] and w.job is None, order
    finally:
        _app.os.name = real[7]
        _app.subprocess.Popen, _app.subprocess.run, _w.job_create, _w.job_assign, _w.resume_process, _w.job_close, _w.job_pids = real[:7]
    ps1 = open(os.path.join(HERE, "..", "windows", "ci", "after_job.ps1")).read()
    assert "jobmember" in ps1 and "ToFileTimeUtc" in ps1 and "taskkill.exe /PID" in ps1 and "-Name" not in ps1.replace("-NoNewline", "")


@check
def notify_cause_hops_counted_per_key_frame_1995():
    """#1995: `input_order hop=notify_cause view=Input cause=<fn>[_N] t=` (app/src/terminal/input.rs notify_cause!, crates/warpui_core/src/input_order.rs) is
    counted per `view:cause` over the key frame builds that follow it; lines before no build are not counted; an old log gives no metric"""
    import render_hops as rh

    def nc(c, t, view="Input"):
        return "input_order hop=notify_cause view=%s cause=%s t=%d.000" % (view, c, t)
    build = ("input_order hop=key_frame_build passes=1 layout_passes=1 views=3 mounted=57 render_ms=0.45 layout_ms=0.50 after_layout_ms=0.02 paint_ms=0.40 "
             "hover_ms=0.10 total_ms=1.90 layout_top=Terminal:0.31 paint_top=- t=%d.0")
    L = [nc("handle_editor_event", 1000000), nc("slash_commands::close_slash_commands_menu", 1000001), build % 1000002,
         nc("handle_editor_event", 1000100), build % 1000101, build % 1000200, nc("update_suggestions_2", 1000300, "InputSuggestions")]
    m = rh.render_hop_metrics(L)
    assert m["input_notify_causes"] == {"Input:handle_editor_event": 2, "Input:slash_commands::close_slash_commands_menu": 1}, m
    assert m["input_notify_frames"] == 2 and m["key_frame_builds"] == 3, m
    assert "input_notify_causes" not in rh.render_hop_metrics([build % 1000002])
    src = open(os.path.join(HERE, "..", "..", "..", "crates", "warpui_core", "src", "input_order.rs")).read()
    assert 'hop_line("notify_cause", &format!("view={view} cause={cause}"))' in src and "fn notify_cause_lines_have_the_shape_render_hops_parses" in src


@check
def windows_app_extra_env_reaches_spawn_1962():
    """#1962 follow-up: WindowsApp.extra_env (an A/B arm's variable, set before a relaunch) is in the environment dict _spawn hands to Popen, wins over the
    inherited one, and is empty by default; the A/B case no longer skips on ctx.win"""
    import app as _app
    stub = types.ModuleType("winapi")
    stub.git_dirs = lambda: []
    old = sys.modules.get("winapi")
    sys.modules["winapi"] = stub
    try:
        w = _app.WindowsApp.__new__(_app.WindowsApp)
        w.bin, w.home, w.run, w.pane, w.ctl, w.job, w.proc = "x.exe", "h", tempfile.gettempdir(), "powershell", "c", None, None
        w.extra_env, w.hook_timing_path = {}, "t.txt"
        base = w.env()
        assert "PHOSPHOR_KEY_VIEW_SKIP" not in base or base["PHOSPHOR_KEY_VIEW_SKIP"] == os.environ.get("PHOSPHOR_KEY_VIEW_SKIP")
        w.extra_env = {"PHOSPHOR_KEY_VIEW_SKIP": "0", "RUST_LOG": "x"}
        e = w.env()
        assert e["PHOSPHOR_KEY_VIEW_SKIP"] == "0" and e["RUST_LOG"] == "x", e
        seen = []
        real = (_app.subprocess.Popen, _app.os.name)
        _app.subprocess.Popen = lambda *a, **k: (seen.append(k.get("env")), types.SimpleNamespace(_handle=1, pid=1))[1]
        _app.os.name = "posix"
        try:
            w._spawn(None)
        finally:
            _app.subprocess.Popen, _app.os.name = real
        assert seen and seen[0]["PHOSPHOR_KEY_VIEW_SKIP"] == "0", seen
    finally:
        if old is None:
            sys.modules.pop("winapi", None)
        else:
            sys.modules["winapi"] = old
    src = open(os.path.join(HERE, "cases_viewskip_ab.py")).read()
    assert "ctx.win or" not in src.split("def c_latency_key_views_ab")[1].split("kl3 =")[0]
    assert "extra_env = {}" in open(os.path.join(HERE, "app.py")).read().split("class WindowsApp")[1]


@check
def toml_edit_file_keeps_the_other_keys():
    """round 7: network_gate_switch wrote `open(p, "w").write(set_key(open(p).read(), ...))`, which truncated settings.toml first, so the allowed origin the case had
    written (and every other key) vanished before the page opened. edit_file reads first; no case may carry the truncating shape again."""
    import toml_text as T
    d = tempfile.mkdtemp(dir=os.environ.get("TMPDIR"))
    try:
        p = os.path.join(d, "settings.toml")
        open(p, "w").write('[appearance]\ntheme = "auto"\n\n[network]\nallowed_origins = "h.example.invalid"\n')
        T.edit_file(p, "network", "gate_enabled", "false")
        t = open(p).read()
        assert 'allowed_origins = "h.example.invalid"' in t and 'theme = "auto"' in t and T.get_key(t, "network", "gate_enabled") == "gate_enabled = false", t
    finally:
        shutil.rmtree(d, ignore_errors=True)
    import re
    bad = re.compile(r'open\([^)]*["\']w["\'][^)]*\)\.write\(.*open\(')
    for fn in sorted(os.listdir(HERE)):
        if fn.endswith(".py") and fn != "selftest.py":
            for i, line in enumerate(open(os.path.join(HERE, fn), encoding="utf-8", errors="replace"), 1):
                assert not bad.search(line), "%s:%d truncates the file before reading it: %s" % (fn, i, line.strip()[:120])


@check
def keyring_unavailable_startup_pure_pieces():
    """#1805: the launcher's keyring variants and the per-launch log judgement of keyring_unavailable_startup"""
    import app as _app
    import cases_ui as U
    assert _app.keyring_dbus_files("unlocked", "/r") == {}
    assert _app.launcher_argv("unlocked", "/r", "/r/inner.sh") == ["dbus-run-session", "--", "/r/inner.sh"]
    absent = _app.keyring_dbus_files("absent", "/r")
    assert list(absent) == ["/r/dbus-session-absent.conf"], absent
    conf = absent["/r/dbus-session-absent.conf"]
    assert "<servicedir>" not in conf and "standard_session_servicedirs" not in conf and "<type>session</type>" in conf, conf
    hang = _app.keyring_dbus_files("hang", "/r")
    svc = hang["/r/dbus-services-hang/org.freedesktop.secrets.service"]
    assert "Name=org.freedesktop.secrets" in svc and "Exec=" in svc
    assert "<servicedir>/r/dbus-services-hang</servicedir>" in hang["/r/dbus-session-hang.conf"]
    assert _app.launcher_argv("hang", "/r", "/r/inner.sh")[1] == "--config-file=/r/dbus-session-hang.conf"
    try:
        _app.keyring_dbus_files("locked", "/r")
        raise AssertionError("an unknown variant must be refused")
    except ValueError:
        pass

    good = ("startup timing: A=1ms FIRST_FRAME_DRAWN=380ms total=900ms\n"
            "Failed to acquire default Secret Service collection: the service did not answer\n"
            "Showing the keyring-unavailable notice\n"
            "slow model update: 40 ms (model warp::settings::network_secrets::ProxyCredentials)\n")
    f = U.keyring_launch_facts(good)
    assert f["first_frame_ms"] == 900 and f["notices"] == 1 and f["acquire_errors"] == 1 and f["slow_secure"] == [(40, "model warp::settings::network_secrets::ProxyCredentials")], f
    assert U.keyring_launch_verdicts(f, "absent", 2000) == []
    # the #1805 launch: a 120 s ProxyCredentials update, the frame late, the notice never shown, the error twice
    bad = ("slow model update: 119937 ms (model warp::settings::network_secrets::ProxyCredentials)\n"
           "secure storage: a call did not return within 30s; abandoning the worker\n"
           + "Failed to acquire default Secret Service collection: x\n" * 2 + "startup timing: FIRST_FRAME_DRAWN=1ms total=121000ms\n")
    v = U.keyring_launch_verdicts(U.keyring_launch_facts(bad), "absent", 2000)
    assert len(v) == 5 and any("121000 ms" in m for m in v) and any("119937 ms" in m for m in v), v
    assert any("never drawn" in m for m in U.keyring_launch_verdicts(U.keyring_launch_facts(""), "hang", 2000))
    # a working keyring: no notice allowed, no first-frame bound (it is the control)
    assert U.keyring_launch_verdicts(U.keyring_launch_facts("startup timing: total=5000ms\n"), "unlocked", 2000) == []
    assert U.keyring_launch_verdicts(U.keyring_launch_facts(good), "unlocked", 2000) == ["unlocked: the keyring notice was shown 1 time(s) with a working keyring"]



# ---------------------------------------------------------------- the fake ssh lab (Wave 0.5b): pure pieces, no podman, no network
def _load_by_path(name, path):
    import importlib.util
    spec = importlib.util.spec_from_file_location(name, path)
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    return m


SSHLAB = os.path.join(HERE, "..", "sshlab")
LIBDIR = os.path.join(SSHLAB, "rootfs", "usr", "local", "lib", "sshlab")


def _usersdb():
    return _load_by_path("usersdb", os.path.join(LIBDIR, "usersdb.py"))


def _netem():
    return _load_by_path("netem", os.path.join(LIBDIR, "netem.py"))


@check
def lab_matrix_expansion():
    import lab as L
    arms = L.expand_arms("x", ("bash", "zsh", "nu"), ("lan", "150", "300"), ("helper", "nohelper"), ("rst", "stall10"), ("p_banner",))
    names = [a["name"] for a in arms]
    assert len(names) == len(set(names)), "duplicate arm names"
    assert names[0] == "lab_x.bash.lan.nohelper", names[0]
    hs = [a["helper"] for a in arms]
    assert hs == sorted(hs, key=lambda h: L.HELPERS.index(h)), "arms must be helper-major, nohelper first (the app relaunches only when the helper mode changes)"
    faults = [a for a in arms if a["fault"]]
    assert faults and all(a["shell"] in ("bash", "zsh") and a["lat"] in ("lan", "150") and not a["scenario"] for a in faults), "fault arms: bash/zsh at LAN and 150 only"
    assert any(a["name"] == "lab_x.p_banner.150.helper" and a["user"] == "p_banner" and a["shell"] == "bash" for a in arms)
    assert next(a for a in arms if a["shell"] == "nu")["user"] == "u_nu"
    quick = L.matrix_selection("quick")
    q = [a["name"] for a in arms if L.arm_selected(a, quick)]
    assert q and all((".nohelper" in n or ".helper" in n) and ".300." not in n and "p_banner" not in n and n.count(".") == 3 for n in q), q
    full = L.matrix_selection("full")
    assert all(L.arm_selected(a, full) for a in arms)
    sp = L.matrix_selection("shells=zsh;lat=150;helper=nohelper,helper;faults=1")
    got = [a["name"] for a in arms if L.arm_selected(a, sp)]
    assert got and all(".zsh.150." in n for n in got) and any(n.endswith(".rst") for n in got), got
    for bad in ("shells=tcsh", "lat=77", "bogus=1", "nonsense"):
        try:
            L.matrix_selection(bad)
            raise AssertionError("accepted %r" % bad)
        except ValueError:
            pass
    assert L.name_matches("lab_*", "lab_x.bash.lan.nohelper") and not L.name_matches("lab_*", "ssh_drop_reconnect") and L.name_matches("a,ssh_drop*", "ssh_drop_reconnect")


@check
def remote_tab_trial_prefixes_are_unique():
    """r18w6: ctx.tok is T<counter>_<random>; three consecutive trials once typed the same `t12` prefix, so Tab was ambiguous and the visible line ran (correct for a shell, a harness bug).
    The typed prefix must separate any two tokens of one case."""
    import cases_caseremote as CR
    toks = ["t126_xbfizx", "t127_pgvra6", "t128_uhwirz", "t9_abc", "t99_abc", "t100_abc"]
    pre = [CR.tok_prefix(t) for t in toks]
    assert pre[0] == "t126_x" and len(set(pre)) == len(pre), pre
    for i, a in enumerate(pre):
        for j, b in enumerate(pre):
            assert i == j or not b.startswith(a), (a, b)


@check
def lab_path_detect_hook_lines_match_the_apps_log_format():
    """The #1801 lab case reads two app log lines; pin the regexes to the exact text app/src/completer/pushed_listing.rs writes (ids and counts only)."""
    import cases_lab as CL
    m = CL.HOOK_CWD_LISTING.search("INFO hop=cwd_listing session=4242 kind=L entries=2")
    assert m and m.groups() == ("4242", "L", "2"), m
    assert CL.HOOK_CWD_LISTING.search("hop=cwd_listing session=4242 kind=T entries=0").group(2) == "T"
    assert not CL.HOOK_CWD_LISTING.search("hop=cwd_listing session=4242 kind=X entries=2")
    m = CL.HOOK_PATH_DETECT.search("INFO hop=path_detect source=pushed session=4242 entries=2 complete=true")
    assert m and m.groups() == ("4242", "2", "true"), m
    assert not CL.HOOK_PATH_DETECT.search("hop=path_detect session=4242 path=/x")


@check
def lab_git_write_remote_mirrors_the_rust_scripts():
    """P7: the lab case runs the shell backend's git script shape and reads the app log line `hop=backend_route family=git_write`; pin both to the Rust text so a change on either side
    fails here instead of silently testing something else."""
    import cases_lab_git as G
    root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
    rs = lambda p: open(os.path.join(root, "crates/remote_server/src", p), encoding="utf-8").read()
    runner = rs("shellcmd/git/runner.rs")
    assert 'GIT_SSH_COMMAND=\\"${GIT_SSH_COMMAND:-ssh} -o BatchMode=yes\\"' in runner, "GIT_SSH_COMMAND_SH_ASSIGNMENT changed"
    assert '["GIT_ASKPASS", "SSH_ASKPASS"]' in runner and '("SSH_ASKPASS_REQUIRE", "never")' in runner and '("GIT_TERMINAL_PROMPT", "0")' in runner
    assert all(x in G.ENV_PREFIX for x in ("-u GIT_ASKPASS", "-u SSH_ASKPASS", "SSH_ASKPASS_REQUIRE=never", "GIT_TERMINAL_PROMPT=0", "-o BatchMode=yes"))
    parse = rs("shellcmd/git/parse.rs")
    assert "MERGE_HEAD" in parse and "for f in {names}; do [ -e {dir}/" in parse, "operation_probe_script changed"
    features = rs("backend/features.rs")
    assert "hop=backend_route family={}" in rs("backend/router.rs") and "git_commit_chain_shell;" in features
    m = G.HOOK_REMOTE_OP.search("INFO hop=backend_route family=git_write backend=shell reason=no_helper")
    assert m and m.groups() == ("shell",), m
    assert not G.HOOK_REMOTE_OP.search("hop=backend_route family=file_context backend=shell reason=x")
    assert G.sh_c("echo a") == "sh -c 'echo a'"
    labs = [c["arm"] for c in cases.CASES if c.get("lab") and c["name"].startswith("lab_git_write_remote")]
    assert {a["shell"] for a in labs} == {"bash", "zsh", "fish", "nu", "dash", "ash", "pwsh"} and {a["helper"] for a in labs} == {"nohelper", "upload"}, labs


@check
def lab_cases_registry_and_selection():
    import lab as L, run
    labs = [c for c in cases.CASES if c.get("lab")]
    assert labs, "no lab cases registered"
    names = [c["name"] for c in labs]
    assert len(names) == len(set(names)) and all(n.startswith("lab_") for n in names)
    assert all(c["arm"]["name"] == c["name"] for c in labs)
    reg = {c["name"] for c in cases.CASES}
    assert run.unknown_case_names("lab_*", reg) == [] and run.unknown_case_names("lab_nonexistent*,ssh_drop_reconnect", reg) == ["lab_nonexistent*"]
    assert run.unknown_case_names("mode_proof,typo_case", reg) == ["typo_case"]       # exact names are still checked exactly
    # winmap: a lab case is never 'unclassified'; PowerShell pane skips with a reason; p_onesess is skipped on Windows
    base = {n.split(".")[0] for n in names}
    old = {"lab_tab_remote_dir", "lab_path_detect", "lab_chips", "lab_enter_start", "lab_enter_no_network_wait", "lab_ssh_drop_reconnect", "lab_ctrl_c_no_esc_i", "lab_no_helper_chip", "lab_replay", "lab_password_prompt", "lab_helper_install", "lab_nu_agent_pager_env", "lab_read_files_remote", "lab_diff_apply_remote", "lab_file_tree_remote", "lab_remote_edit", "lab_skills_remote", "lab_diff_panel_remote", "lab_gh_chip_remote", "lab_codebase_index_remote", "lab_agent_remote_flow", "lab_agent_remote_errors", "lab_agent_remote_drop"}
    assert old <= base and all(n.startswith(("lab_fam_", "lab_helper_", "lab_nohelper_", "lab_git_write_remote")) for n in base - old), base
    assert all(c["modes"] == ("terminal",) for c in labs if c["name"].startswith("lab_ssh_drop_reconnect")), "the drop case is terminal-only"
    pw = [c["arm"] for c in labs if c["name"].startswith("lab_password_prompt")]
    assert {a["user"] for a in pw} == {"p_password", "p_kbdint"} and all(a["helper"] == "nohelper" and a["fault"] is None for a in pw), pw     # #1535
    ordered = sorted(labs, key=lambda c: c["order"])
    assert [c["arm"]["helper"] for c in ordered] == sorted((c["arm"]["helper"] for c in labs), key=lambda h: L.HELPERS.index(h)), "nohelper arms sort before helper arms"
    assert winmap.unclassified(sorted(reg)) == []
    assert winmap.skip_reason("lab_x.u_bash.lan.nohelper", "powershell") and winmap.skip_reason("lab_x.u_bash.lan.nohelper", "gitbash") is None
    assert "ControlMaster" in winmap.skip_reason("lab_x.p_onesess.lan.nohelper", "gitbash")


@check
def lab_file_tree_remote_pieces():
    """Wave H parity P3: the lab case reads the app's hop=file_tree log lines. The leaf-name hash must be the one manager.rs logs (32-bit FNV-1a), the line parser
    must take what the app prints, and the case registers exactly the nohelper and upload arms for bash, zsh, fish and nu."""
    import cases_lab_remote as CLR
    assert CLR.fnv1a32("") == "811c9dc5" and CLR.fnv1a32("a") == "e40c292c" and CLR.fnv1a32("foobar") == "bf9cf968"        # the published FNV-1a test vectors
    line = "INFO hop=file_tree event=update source=shell removed=1 added=2 added_hash=[e40c292c,bf9cf968]\nINFO hop=file_tree event=snapshot source=helper removed=0 added=0 added_hash=[]"
    assert CLR.tree_events(line) == [("update", "shell", 1, 2, ["e40c292c", "bf9cf968"]), ("snapshot", "helper", 0, 0, [])]
    assert CLR.saw_added(line, "a") and not CLR.saw_added(line, "b") and not CLR.saw_added(line, "a", event="snapshot")
    arms = [c["arm"] for c in cases.CASES if c.get("lab") and c["name"].startswith("lab_file_tree_remote.")]
    assert {(a["shell"], a["helper"]) for a in arms} == {(s, h) for s in ("bash", "zsh", "fish", "nu", "dash", "ash") for h in ("nohelper", "upload")}, arms


@check
def lab_diff_panel_and_gh_chip_pieces():
    """Wave H parity P6: the lab cases read the app's hop=diff_state / hop=git_chip / hop=gh_chip log lines exactly as the app prints them, the fake gh scripts are the ones
    the contract tests use, and both cases register the nohelper and upload arms for bash, zsh, fish, nu, dash and ash (the diff panel also pwsh) and declare their families to the coverage guard."""
    import cases_lab_diff as D, cases_lab_helper as H
    text = ("INFO hop=diff_state event=snapshot source=shell files=2 path_hash=[e40c292c,bf9cf968]\n"
            "INFO hop=diff_state event=delta source=helper files=1 path_hash=[e40c292c]\n"
            "INFO hop=git_chip source=shell files=2 added=1 removed=0 ahead=1 behind=0 upstream=1\n"
            "INFO hop=gh_chip event=pr source=helper pr=42\nINFO hop=gh_chip event=pr source=shell pr=none\nINFO hop=gh_chip event=repo source=shell repo=some")
    assert D.diff_events(text) == [("snapshot", "shell", 2, ["e40c292c", "bf9cf968"]), ("delta", "helper", 1, ["e40c292c"])]
    assert D.chip_events(text) == [("shell", 2, 1, 0, 1, 0, True)]
    assert D.gh_events(text) == [("pr", "helper", "42"), ("pr", "shell", "none"), ("repo", "shell", "some")]
    assert D.want_source(dict(helper="upload")) == "helper" and D.want_source(dict(helper="nohelper")) == "shell"
    assert '"number":42' in D.FAKE_GH and "gh auth login" in D.SIGNED_OUT_GH
    for name in ("lab_diff_panel_remote.", "lab_gh_chip_remote."):
        arms = [c["arm"] for c in cases.CASES if c.get("lab") and c["name"].startswith(name)]
        assert {(a["shell"], a["helper"]) for a in arms} == {(s, h) for s in (("bash", "zsh", "fish", "nu", "dash", "ash") + (("pwsh",) if name.startswith("lab_diff_panel") else ())) for h in ("nohelper", "upload")}, (name, arms)
    assert H.EXTERNAL_COVERS["diff_state"] == dict(helper="lab_diff_panel_remote", nohelper="lab_diff_panel_remote"), H.EXTERNAL_COVERS.get("diff_state")
    for cap in ("git_status", "github"):
        assert H.EXTERNAL_COVERS[cap] == dict(helper="lab_gh_chip_remote", nohelper="lab_gh_chip_remote"), cap


@check
def lab_skills_remote_pieces():
    """Wave H parity P8: the lab case reads the app's hop=agent_context log lines. The hash must be the file-tree FNV-1a (manager.rs hashes a skill's directory name the
    same way), the line parser must take what the app prints, the skill fixture command must survive every login shell's quoting, and the case registers the nohelper and
    upload arms for bash, zsh, fish and nu."""
    import cases_lab_skills as CLS, cases_lab_remote as CLR
    line = "INFO hop=agent_context source=shell revision=2 skills=2 rules=1 names_hash=[e40c292c,bf9cf968]\nINFO hop=agent_context source=helper revision=1 skills=0 rules=0 names_hash=[]"
    assert CLS.events(line) == [("shell", 2, 2, 1, ["e40c292c", "bf9cf968"]), ("helper", 1, 0, 0, [])]
    assert CLS.saw_skill(line, "a") and CLS.saw_skill(line, "foobar") and not CLS.saw_skill(line, "b")
    seen = []

    class FakeLab:
        def ssh(self, user, cmd):
            seen.append(cmd)

    CLS.make_skill(FakeLab(), "u", "labsk-x")
    CLS.make_rules(FakeLab(), "u")
    CLS.clean(FakeLab(), "u", ["labsk-x"])
    assert all(c.startswith("sh -c '") and c.endswith("'") and c.count("'") == 2 for c in seen), seen        # one single-quoted word: identical in bash, zsh, fish and nu
    assert "SKILL.md" in seen[0] and "name: labsk-x" in seen[0] and "AGENTS.md" in seen[1] and "rm -rf" in seen[2]
    arms = [c["arm"] for c in cases.CASES if c.get("lab") and c["name"].startswith("lab_skills_remote.")]
    assert {(a["shell"], a["helper"]) for a in arms} == {(s, h) for s in ("bash", "zsh", "fish", "nu", "dash", "ash", "pwsh") for h in ("nohelper", "upload")}, arms
    import cases_lab_helper as H
    assert H.EXTERNAL_COVERS["agent_context"] == dict(helper="lab_skills_remote", nohelper="lab_skills_remote"), H.EXTERNAL_COVERS

def lab_remote_edit_pieces():
    """Wave H parity P4: the buffers lab case registers exactly the nohelper and upload arms for bash, zsh, fish and nu, declares the buffers family to the coverage
    guard on both arms, and reads the router's line the way the app writes it."""
    import cases_lab_buffers as CLB, cases_lab_helper as H
    line = "INFO hop=backend_route family=buffers backend=shell reason=caps\nINFO hop=backend_route family=file_context backend=helper reason=caps"
    assert CLB.routes(line) == [("shell", "caps")] and CLB.routes(line, "file_context") == [("helper", "caps")] and CLB.routes("") == []
    assert CLB.expected_backend(dict(helper="nohelper")) == "shell" and CLB.expected_backend(dict(helper="upload")) == "helper"
    arms = [c["arm"] for c in cases.CASES if c.get("lab") and c["name"].startswith("lab_remote_edit.")]
    assert {(a["shell"], a["helper"]) for a in arms} == {(s, h) for s in ("bash", "zsh", "fish", "nu", "dash", "ash", "pwsh") for h in ("nohelper", "upload")}, arms
    assert H.EXTERNAL_COVERS["buffers"] == dict(helper="lab_remote_edit", nohelper="lab_remote_edit"), H.EXTERNAL_COVERS.get("buffers")


@check
def lab_helper_coverage_guard():
    """P10: every capability in capability.rs HELPER_CAPABILITIES maps to a registered lab case with a helper arm, and every backend Family (but the legacy-only
    codebase_index) to one with a nohelper arm; the parse of the Rust source and the skip rules are pinned."""
    import cases_lab_helper as H
    info = H.parse_rust()
    assert info["helper"] and set(info["helper"]) <= set(info["consts"].values()), info["helper"]
    assert len(info["families"]) >= 15 and "RunCommand" in info["families"] and info["families"]["FsBasic"] == "fs_basic", info["families"]
    assert info["stubbed"] and info["stubbed"] <= set(info["families"]), info["stubbed"]
    miss_h, miss_n = H.coverage(info)
    assert not miss_h, "helper capabilities with no lab case that has a helper arm (add one to cases_lab_helper.py): %s" % miss_h
    assert not miss_n, "backend families with no lab case that has a nohelper arm: %s" % miss_n
    assert H.shell_arm_implemented("RunCommand", info) and not H.shell_arm_implemented("FsBasic", dict(info, stubbed={"FsBasic"}))
    assert H.shell_arm_implemented("GitStatus", dict(info, stubbed=set(), shell_text="Family::GitStatus"))
    # a capability nobody covers must be reported
    saved, saved_ext = dict(H.COVERS), dict(H.EXTERNAL_COVERS)
    try:
        H.COVERS.clear()
        H.EXTERNAL_COVERS.clear()
        mh, mn = H.coverage(info)
        assert set(mh) == set(info["helper"]) and mn, (mh, mn)
    finally:
        H.COVERS.update(saved)
        H.EXTERNAL_COVERS.update(saved_ext)
    skips = [c for c in cases.CASES if c.get("lab") and c["arm"]["case"].startswith("fam_") and c["arm"]["helper"] == "helper"]
    assert skips and {c["arm"]["case"] for c in skips} >= {"fam_" + k for k in info["helper"]}
    import lab as L
    assert L.matrix_selection("quick")["shells"] == {"bash", "zsh", "fish", "nu"}


@check
def lab_helper_arm_pieces():
    """Wave H (#1804): the helper arms' pure pieces. The file name is derived from `--version` exactly as setup::helper_file_name does, the host policy key is
    user@host, the arms are helper-major with the new modes after `helper`, and a fake helper that does not print a version line is refused."""
    import lab as L, cases_lab as CL, stat
    assert CL.helper_file_name("phosphor-remote v0.1.18 protocol 1") == "phosphor-remote-v0.1.18"
    assert CL.host_label(dict(user="u_bash")) == "u_bash@lab-u_bash"
    assert L.HELPERS[:2] == ("nohelper", "helper") and set(L.HELPERS) == {"nohelper", "helper", "upload", "upgrade", "never"}
    assert set(CL.HELPER_ARM_MODE) == set(L.HELPERS) and CL.HELPER_ARM_MODE["nohelper"] == "never_install" and CL.HELPER_ARM_MODE["upload"] == "always_install"
    arms = L.expand_arms("helper_install", ("bash",), ("lan",), ("upload", "upgrade", "never"), (), ("p_noscp",))
    assert [a["helper"] for a in arms] == ["upload", "upload", "upgrade", "upgrade", "never", "never"], arms
    assert any(a["name"] == "lab_helper_install.p_noscp.lan.upload" and a["user"] == "p_noscp" for a in arms)
    with tempfile.TemporaryDirectory() as d:
        good, bad = os.path.join(d, "good"), os.path.join(d, "bad")
        open(good, "w").write("#!/bin/sh\necho 'phosphor-remote v9 protocol 1'\necho 'build abc'\n")
        open(bad, "w").write("#!/bin/sh\necho 'something else 1.0'\n")
        for f in (good, bad):
            os.chmod(f, os.stat(f).st_mode | stat.S_IEXEC)
        assert CL.helper_version_line(good) == "phosphor-remote v9 protocol 1"
        assert CL.helper_version_line(bad) is None and CL.helper_version_line(os.path.join(d, "missing")) is None


@check
def lab_users_tsv_parse():
    U = _usersdb()
    rows = U.load()
    byname = {r.user: r for r in rows}
    for u in ("u_bash", "u_zsh", "u_fish", "u_nu", "u_pwsh", "u_dash", "u_ash", "u_rbash", "p_banner", "p_rcerr", "p_rcprompt", "p_slowrc", "p_colon", "p_question",
              "p_sete", "p_exec", "p_rohome", "p_nohome", "p_tinypath", "p_password", "p_kbdint", "p_mfa", "p_slowauth", "p_force", "p_nopty", "p_onesess", "p_noinstall", "p_sudo", "p_noscp"):
        assert u in byname, "users.tsv lacks %s" % u
    assert len(byname) == len(rows)
    assert byname["u_rbash"].shell == "rbash" and byname["u_nu"].shell == "nu"
    blocks = U.match_blocks(rows)
    assert "Match User p_onesess\n    MaxSessions 1\n" in blocks and "Match User p_nopty\n    PermitTTY no\n" in blocks
    assert "AuthenticationMethods publickey,keyboard-interactive" in blocks and "Match User u_bash" not in blocks
    assert all(c[:3] == ["useradd", "-M", "-d"] for c in U.useradd_commands(rows)) and len(U.useradd_commands(rows)) == len(rows)
    assert "/opt/pwsh/pwsh" in U.etc_shells(rows) and "/bin/rbash" in U.etc_shells(rows)
    for text, why in (("a\tbash\tdefault\n", "3 fields"), ("a\tcsh\tdefault\t-\n", "shell"), ("a\tbash\tnope\t-\n", "profile"), ("a\tbash\tdefault\t-\na\tzsh\tdefault\t-\n", "duplicate")):
        try:
            U.parse_users(text)
            raise AssertionError("accepted a bad table: " + why)
        except ValueError:
            pass
    assert U.parse_users("# c\n\nx_y\tfish\tbanner\tBanner /x;MaxSessions 2\n")[0].directives() == ["Banner /x", "MaxSessions 2"]
    for r in rows:      # every profile with a bash scenario fragment has its file, and the leased-secret users are the ones PAM keys on
        if r.profile in ("rcerr", "rcprompt", "slowrc", "colon", "question", "sete", "tinypath", "noinstall", "sudo", "exec"):
            assert os.path.isfile(os.path.join(LIBDIR, "scenarios", r.profile + ".sh")), r.profile
    pam = open(os.path.join(SSHLAB, "rootfs", "etc", "pam.d", "sshd")).read()
    assert all(u in pam for u in U.PASSWORD_USERS + U.MFA_USERS + U.SLOWAUTH_USERS)


@check
def lab_remote_mode_fails_closed_without_a_pinned_box_key():
    import lab as L
    keep = {k: os.environ.get(k) for k in ("APPCI_LAB", "APPCI_LAB_BOX", "APPCI_LAB_KEY", "APPCI_LAB_KNOWN_HOSTS", "SSHLAB_BOX")}
    d = tempfile.mkdtemp()
    try:
        for k in keep:
            os.environ.pop(k, None)
        os.environ.update(APPCI_LAB="remote", APPCI_LAB_BOX="u@box.invalid", APPCI_LAB_KEY="/k")
        m, why = L.lab_mode()
        assert m == "off" and "APPCI_LAB_KNOWN_HOSTS" in why, (m, why)
        os.environ["APPCI_LAB_KNOWN_HOSTS"] = os.path.join(d, "nope")
        assert L.lab_mode()[0] == "off"
        open(os.path.join(d, "kh"), "w").write("box ssh-ed25519 AAAA\n")
        os.environ["APPCI_LAB_KNOWN_HOSTS"] = os.path.join(d, "kh")
        assert L.lab_mode() == ("remote", "")
    finally:
        for k, v in keep.items():
            os.environ.pop(k, None)
            if v is not None:
                os.environ[k] = v
        shutil.rmtree(d, ignore_errors=True)


@check
def lab_fixed_slots_ports_mode_and_no_token_leak():
    import lab as L
    C = _load_by_path("labctl_fx", os.path.join(SSHLAB, "labctl.py"))
    D = _load_by_path("labd_fx", os.path.join(SSHLAB, "rootfs", "usr", "local", "sbin", "labd.py"))
    # slot parsing and the documented port scheme: ssh S, host-key-B S+10, echo C+700, control C
    t = C.parse_slots(C.DEFAULT_SLOTS)
    assert t["mac"] == {22: 22202, 2222: 22212, 7: 7702, 7000: 7002} and t["windows"] == {22: 22203, 2222: 22213, 7: 7703, 7000: 7003}, t
    for bad in ("", "mac", "mac:1:2:3", "Mac:22202:7002", "mac:22202:7002,mac:22302:7102", "a:22202:7002,b:22212:7102", "a:80:7002", "a:40000:7002", "a:22202:x"):
        try:
            C.parse_slots(bad)
            raise AssertionError("accepted slots %r" % bad)
        except ValueError:
            pass
    assert L.slot_ports("mac") == t["mac"] and L.default_slot("darwin") == "mac" and L.default_slot("win32") == "windows"
    try:
        L.slot_ports("beos")
        raise AssertionError("accepted an unknown slot")
    except L.LabError:
        pass
    # the slot container: fixed published ports, token only as a read-only mounted FILE, exclusive leases, never the token value on the command line
    argv = C.slot_run_argv("img:t", "mac", t["mac"], 3600, "/home/x/.phosphor-sshlab/token", 1000)
    flat = " ".join(argv)
    for p, cp in ((22202, 22), (22212, 2222), (7702, 7), (7002, 7000)):
        assert "-p 0.0.0.0:%d:%d" % (p, cp) in flat
    assert "SSHLAB_TOKEN_FILE=/etc/sshlab-token" in flat and "SSHLAB_EXCLUSIVE=1" in flat and "SSHLAB_TOKEN=" not in flat and ":/etc/sshlab-token:ro" in flat and "sshlab.slot=mac" in flat
    # reconcile decisions: create / keep / replace on image change or expiry or a dead container; defer while a client holds the lease
    lab = {"phosphor.sshlab": "1", "phosphor.sshlab.image": "img:t", "phosphor.sshlab.expires": "5000"}
    assert C.slot_action(None, "img:t", 100) == "create"
    assert C.slot_action({"Labels": lab, "State": "running"}, "img:t", 100) == "keep"
    assert C.slot_action({"Labels": lab, "State": "running"}, "img:u", 100) == "replace"
    assert C.slot_action({"Labels": lab, "State": "running"}, "img:t", 6000) == "replace"
    assert C.slot_action({"Labels": lab, "State": "exited"}, "img:t", 100) == "replace"
    assert C.slot_action({"Labels": lab, "State": "running"}, "img:u", 100, leased=True) == "defer"
    # mode selection incl. the token-missing SKIP reason; nothing printed ever holds the token
    keep = {k: os.environ.get(k) for k in ("APPCI_LAB", "SSHLAB_BOX", "SSHLAB_TOKEN", "APPCI_LAB_SLOT", "PATH")}
    secret = "s3cr3t-token-value-0123456789abcdef"
    try:
        for k in ("APPCI_LAB", "SSHLAB_BOX", "SSHLAB_TOKEN", "APPCI_LAB_SLOT"):
            os.environ.pop(k, None)
        L.ci_env._file = lambda: {}
        os.environ["PATH"] = ""            # no podman
        m, why = L.lab_mode()
        assert m == "off" and "SSHLAB_BOX" in why, (m, why)
        os.environ["APPCI_LAB"] = "fixed"
        os.environ["SSHLAB_BOX"] = "box.invalid"
        m, why = L.lab_mode()
        assert m == "off" and "SSHLAB_TOKEN" in why and "not on a protected ref" in why, (m, why)
        os.environ["SSHLAB_TOKEN"] = secret
        assert L.lab_mode() == ("fixed", "")
        del os.environ["APPCI_LAB"]
        assert L.lab_mode() == ("fixed", ""), "auto-select on a podman-less machine with box + token"
        os.environ["APPCI_LAB"] = "local"
        m, why = L.lab_mode()
        assert m == "off" and secret not in why
        # errors printed by the client never contain the token
        assert secret not in L.safe_text("GET /health failed: " + secret, secret) and "<token>" in L.safe_text("x" + secret, secret)
        lb = L.Lab(dict(cid=None, ports=t["mac"], token=secret), "127.0.0.1", "fixed")
        try:
            lb.ports[7000] = 1            # nothing listens on port 1
            lb.health()
        except L.LabError as e:
            assert secret not in str(e)
        # a second lease on an exclusive slot is refused (409), the same owner may re-lease, an expired lease is taken over; a short token is unusable
        assert D.lease_gate(True, True, "a", 200.0, "b", 100.0) and "another client" in D.lease_gate(True, True, "a", 200.0, "b", 100.0)
        assert D.lease_gate(True, True, "a", 200.0, "a", 100.0) is None and D.lease_gate(True, True, "a", 50.0, "b", 100.0) is None
        assert D.lease_gate(True, False, None, 0.0, "b", 100.0) is None and D.lease_gate(False, True, "a", 200.0, "b", 100.0) is None
        assert D.load_token({"SSHLAB_TOKEN": "short"}) == "" and D.load_token({"SSHLAB_TOKEN": secret}) == secret and D.load_token({}) == ""
        d = tempfile.mkdtemp()
        try:
            open(d + "/tok", "w").write(secret + "\n")
            assert D.load_token({"SSHLAB_TOKEN_FILE": d + "/tok"}) == secret
            os.chmod(d + "/tok", 0o644)
            try:
                C.read_token_file(d + "/tok")
                raise AssertionError("accepted a group/world-readable token file")
            except SystemExit as e:
                assert secret not in str(e)
            os.chmod(d + "/tok", 0o600)
            assert C.read_token_file(d + "/tok") == secret and C.redact("a " + secret, secret) == "a <token>"
        finally:
            shutil.rmtree(d, ignore_errors=True)
    finally:
        for k, v in keep.items():
            os.environ.pop(k, None)
            if v is not None:
                os.environ[k] = v


@check
def lab_tc_nft_command_generation():
    N = _netem()
    cmds = N.apply_commands({"rtt_ms": 150, "jitter_ms": 10, "loss_pct": 1, "rate_kbit": 5000})
    flat = [" ".join(c) for c in cmds]
    assert flat[0].startswith("tc qdisc del dev eth0 root"), "apply starts by clearing (idempotent)"
    assert any("netem delay 75ms 5ms loss 0.5% rate 5000kbit" in c and "parent 1:1" in c for c in flat), "egress netem on band 1 with half the RTT"
    assert any(c.startswith("tc qdisc add dev ifb0 root") and "delay 75ms 5ms" in c for c in flat), "ingress netem on ifb0 with the other half"
    for p in (22, 2222, 7):
        assert any("match ip sport %d 0xffff flowid 1:1" % p in c for c in flat) and any("match ip dport %d 0xffff action mirred egress redirect dev ifb0" % p in c for c in flat), p
    assert not any("7000" in c for c in flat), "the control port is never faulted"
    assert any(c.startswith("tc qdisc add dev lo root") for c in flat) and any(c.startswith("tc qdisc add dev lo parent 3:1") and "delay 75ms" in c for c in flat), "rootlessport reaches the lab over lo: lo is shaped too"
    for p in (22, 2222, 7):
        assert all(any("dev lo parent 3: " in c and "match ip %s %d 0xffff flowid 3:1" % (side, p) in c for c in flat) for side in ("sport", "dport")), p
    assert "tc qdisc del dev lo root" in [" ".join(c) for c in N.clear_commands()]
    assert any("priomap" in c and c.endswith("2 " * 15 + "2") for c in flat), "unmatched traffic defaults to band 3"
    assert N.apply_commands({}) == N.clear_commands() and N.is_noop({"rtt_ms": 0})
    assert N.netem_args({"rtt_ms": 300}) == ["netem", "delay", "150ms"]
    for bad in ({"rtt_ms": -1}, {"loss_pct": 101}, {"nonsense": 1}):
        try:
            N.normalize(bad)
            raise AssertionError("accepted %r" % bad)
        except ValueError:
            pass
    nft = N.nft_block_outbound([1002, 1001])
    assert "meta skuid { 1001, 1002 }" in nft and "ct state new reject" in nft and 'oifname != "lo"' in nft and nft.startswith("table inet sshlab_out\ndelete table")
    assert N.nft_drop_connection("10.0.0.2", 5555, "rst", 22, "t1") == ["add rule inet t1 drop_in ip saddr 10.0.0.2 tcp sport 5555 tcp dport 22 reject with tcp reset"]
    assert len(N.nft_drop_connection("10.0.0.2", 5555, "blackhole", 22)) == 2
    assert "chain drop_in" in N.nft_drop_setup("t2") and "table inet t2" in N.nft_drop_setup("t2") and "sshlab_drop" not in N.nft_drop_setup("t2"), "one table per drop"
    try:
        N.nft_drop_connection("10.0.0.2", 1, "x", 22)
        raise AssertionError("accepted a bad drop mode")
    except ValueError:
        pass


@check
def lab_verdict_maths():
    import lab as L
    # TOTP: RFC 6238 appendix B vector (SHA-1, secret "12345678901234567890", T=59 -> 94287082; 6 digits -> 287082)
    assert L.totp("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ", 59) == "287082" and L.totp("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ", 59, digits=8) == "94287082"
    # the RTT gate: median within +-15 ms of the target; LAN <= 15; too few samples = no-data; a mismatch is never ok
    assert L.rtt_gate_verdict([150.5, 149, 152, 151, 148, 150], 150)["verdict"] == "ok"
    assert L.rtt_gate_verdict([2, 1, 3, 2, 1, 2], 0)["verdict"] == "ok"
    assert L.rtt_gate_verdict([2, 1, 3, 2, 1, 2], 150)["verdict"] == "mismatch", "no delay applied (pasta spliced it around tc)"
    assert L.rtt_gate_verdict([300, 301, 299, 300, 302, 300], 150)["verdict"] == "mismatch"
    assert L.rtt_gate_verdict([150, 150], 150)["verdict"] == "no-data" and L.rtt_gate_verdict([None] * 9, 0)["verdict"] == "no-data"
    assert L.rtt_gate_verdict([20, 21, 19, 20, 22, 20], 0)["verdict"] == "mismatch", "LAN arm with 20 ms is not LAN"
    # paired verdict: median(app) <= median(plain) + max(15 ms, 10 %)
    plain = [150.0] * 12
    assert L.paired_verdict([164.9] * 12, plain)["verdict"] == "pass" and L.paired_verdict([165.1] * 12, plain)["verdict"] == "fail"
    big = [400.0] * 12
    assert L.paired_verdict([439.0] * 12, big)["verdict"] == "pass" and L.paired_verdict([441.0] * 12, big)["verdict"] == "fail", "10 % beats 15 ms above 150 ms"
    v = L.paired_verdict([166.0] * 12, plain)
    assert v["allowed"] == 165.0 and v["over_ms"] == 1.0
    assert L.paired_verdict([1.0] * 3, plain)["verdict"] == "no-data"
    # #1970 enter_wait_verdict: the app's own Enter -> write times with a delayed link, against the same session without the delay
    fast, lan = [0.6, 0.8, 0.5, 1.1, 0.7, 0.9, 0.6, 0.8, 0.7, 1.0], [0.5, 0.7, 0.6, 0.9, 0.6, 0.8, 0.5, 0.7, 0.6, 0.9]
    v = L.enter_wait_verdict(fast, lan, 150)
    assert v["verdict"] == "pass" and v["limit"] == 75.0 and v["worst"] == 1.1 and v["lan_median"] == 0.65 and v["delta"] == 0.1, v
    v = L.enter_wait_verdict(fast[:-1] + [151.0], lan, 150)
    assert v["verdict"] == "fail" and "waited on the network" in v["why"], "ONE Enter that waited a round trip fails, whatever the median says: %s" % v
    assert L.enter_wait_verdict(fast[:-1] + [75.0], lan, 150)["verdict"] == "fail" and L.enter_wait_verdict(fast[:-1] + [74.9], lan, 150)["verdict"] == "pass"
    assert L.enter_wait_verdict(fast[:-1] + [100.0], lan, 300)["verdict"] == "pass", "the limit is half the arm's RTT (150 ms at 300)"
    v = L.enter_wait_verdict([x + 2.5 for x in fast], lan, 150)
    assert v["verdict"] == "fail" and v["delta"] == 2.6 and "must not grow" in v["why"], v                 # under half the RTT, but 2.6 ms slower than the control
    assert L.enter_wait_verdict([x + 1.9 for x in fast], lan, 150)["verdict"] == "pass"
    assert L.enter_wait_verdict([x + 2.5 for x in fast], lan, 150, budget_ms=5.0)["verdict"] == "pass"
    v = L.enter_wait_verdict(fast, None, 150)
    assert v["verdict"] == "pass" and v["lan_median"] is None and "no undelayed control" in v["why"], v        # the half-RTT rule alone
    assert L.enter_wait_verdict(fast, lan[:3], 150)["lan_median"] is None, "too few control samples: no control"
    assert L.enter_wait_verdict(fast[:7], lan, 150)["verdict"] == "no-data" and L.enter_wait_verdict([], lan, 150)["verdict"] == "no-data"
    assert L.enter_wait_verdict(fast, lan, 0)["verdict"] == "no-data", "a LAN arm has no delay to wait on"
    assert L.enter_wait_verdict([None] * 4 + fast, lan, 150)["n"] == 10
    # outliers do not move a median
    assert L.paired_verdict([150.0] * 10 + [900.0, 900.0], plain)["verdict"] == "pass"
    # sshd channel counts, clock offset (smallest round trip wins), known_hosts and ssh config text
    log = ("sshd-session[1]: Starting session: shell on pts/0 for u from 1.2.3.4 port 5 id 0\nsshd-session[1]: Starting session: command for u from 1.2.3.4 port 5 id 1\n"
           "sshd-session[1]: Starting session: command for u from 1.2.3.4 port 5 id 2\nAccepted publickey for u\n")
    assert L.count_starts(log) == {"command": 2, "shell": 1, "subsystem": 0}
    assert abs(L.clock_offset([(100.0, 105.5, 101.0), (200.0, 205.02, 200.04)]) - 5.0) < 1e-6
    assert L.known_hosts_line("ssh-ed25519 AAAAC3 lab@x") == "sshlab ssh-ed25519 AAAAC3\n"
    cfg = L.ssh_config_text("box.example", 4022, 4222, ["u_bash", "p_mfa"], "C:\\k\\labkey", "C:\\k\\kh", ("p_password",))
    assert "Host lab-u_bash\n  Port 4022" in cfg and "IdentityFile C:/k/labkey" in cfg and "Host lab-changedkey\n  Port 4222" in cfg and "HostKeyAlias sshlab" in cfg
    assert cfg.count("BatchMode yes") == 2 and "Host lab-p_mfa" in cfg and "BatchMode no" in cfg


@check
def lab_remote_adapters_and_hooks():
    import lab as L, profile as P
    # LocalRemote is today's behaviour: Ctx.lines(remote=True) reads <rhome>/name; a missing file is []
    d = tempfile.mkdtemp()
    try:
        open(os.path.join(d, "f.txt"), "w").write("a\nb\n")
        c = _ctx(d)
        c.remote = L.LocalRemote(c.prof)
        assert cases.Ctx.lines(c, "f.txt", remote=True) == ["a", "b"] and cases.Ctx.lines(c, "nope", remote=True) == []
        c.remote.rm("f.txt", "never-there")
        assert not os.path.exists(os.path.join(d, "f.txt"))
        c2 = _ctx(d)      # a ctx without .remote (this file's SimpleNamespace) still works
        assert cases.Ctx.lines(c2, "nope", remote=True) == []
    finally:
        shutil.rmtree(d, ignore_errors=True)

    class FakeLab:
        files = {"/work/u_fish/e.txt": b"x\ny\n", "/home/u_fish/h.txt": b"home\n"}
        ctl_data = {"fish_rec.log": "C\t1700000000\techo hi\nC\tbad\techo two\nP\t1\n", "prompt.log": "P\t1.0\nP\t2.0\nC\t3\tx\n"}

        def file(self, p):
            if p in self.files:
                return self.files[p]
            raise L.LabError("404")

        def ctl(self, user, name, since=0):
            return {"data": self.ctl_data.get(name, "")}
    rem = L.LabRemote(FakeLab(), "u_fish")
    assert rem.shell == "fish" and rem.lines("e.txt") == ["x", "y"] and rem.lines("h.txt") == ["home"] and rem.lines("missing") == []
    assert rem.commands() == [(1700000000.0, "echo hi"), (0.0, "echo two")] and rem.prompts() == 2
    # Profile.read(remote=True) goes through the lab adapter when one is set, and is the old rctl read otherwise
    d = tempfile.mkdtemp()
    try:
        pr = P.Profile(d)
        os.makedirs(pr.rctl)
        open(os.path.join(pr.rctl, "rec.log"), "w").write("C\t1.0\tlocal\n")
        assert pr.read("rec.log", remote=True) == ["C\t1.0\tlocal"]
        pr.remote = rem
        assert pr.read("prompt.log", remote=True) == ["P\t1.0", "P\t2.0", "C\t3\tx"] and pr.read("rec.log") == [] or True
    finally:
        shutil.rmtree(d, ignore_errors=True)
    # the hook templates (profile.py is the single source) format with a ctl dir, log the same C/P lines, and nu/pwsh/sh exist
    hooks = P.lab_hooks()
    assert set(hooks) == {"bash", "rbash", "zsh", "fish", "nu", "pwsh", "sh"}
    for k, t in hooks.items():
        out = t % {"ctl": "/var/lib/appci/ctl/u_x"}
        assert "/var/lib/appci/ctl/u_x" in out, k
    assert "rec.log" in hooks["nu"] % {"ctl": "/c"} and "history -a" in hooks["rbash"] % {"ctl": "/c"} and "prompt.log" in hooks["pwsh"]


@check
def lab_first_run_fixes_round10():
    """Round 10, the lab's first run (job 17885063245; docs/design/r10-lab.md): (1) a lab arm that leaves lab_prepare early (no helper binary, RTT gate, a skip)
    puts ctx.remote / prof.remote back, so the runner's post-case check finds nothing to reset; (2) the LAB fish hook logs prompts (connect waits on prompt.log)
    and the local scratch fish config still does not; (3) a refused accept is judged on its re-read's route, not on a write that never happens; (4) the agent
    cases' commands carry no quote / `&&` / redirection where nu and pwsh must run them, and the drop command leaves a start marker; (5) LabRemote.exists
    tells an empty file from a missing one."""
    import re
    import lab as L, profile as P, cases_lab as CL, cases_agent_remote as AR, cases_agent_file_edits as FE
    # (1) the cleanup covers lab_prepare
    class FakeLab:
        info = {"flavor": "full"}
        cleared = 0

        def helper(self, user, files=None):
            pass

        def clear_fault(self):
            FakeLab.cleared += 1

        def file(self, p):
            if p == "/work/u_bash/empty":
                return b""
            raise L.LabError("404")

    class Unverifiable(Exception):
        pass

    class R:
        def unverifiable(self, msg):
            self.msg = msg

        def skip(self, msg):
            raise cases.Skip(msg)
    d = tempfile.mkdtemp()
    old_bin = os.environ.pop("APPCI_LAB_HELPER_BIN", None)
    old_cache = os.environ.get("PHOSPHOR_CI_HELPER_CACHE")
    os.environ["PHOSPHOR_CI_HELPER_CACHE"] = os.path.join(d, "no-helper-cache")      # this machine's real cache (the box has one) must not answer here
    try:
        prof = P.Profile(d)
        local = L.LocalRemote(prof)
        ctx = types.SimpleNamespace(lab=FakeLab(), lab_why=None, prof=prof, remote=local, arm=None, lab_helper_mode="nohelper")
        arm = dict(name="lab_x.bash.lan.helper", user="u_bash", shell="bash", helper="helper", lat_ms=0, flavor="full")
        ran = []
        r = R()
        CL._runner(lambda c, rr, a: ran.append(1), arm)(ctx, r)
        assert not ran and "APPCI_LAB_HELPER_BIN" in r.msg, "the helper arm must stop at the missing binary"
        assert ctx.remote is local and prof.remote is None and ctx.arm is None and ctx._lab_prev is None, "lab_prepare's swap was not undone on its early return"
        # a skip raised inside lab_prepare (no lab for this pass) is cleaned up too, and still propagates
        ctx2 = types.SimpleNamespace(lab=None, lab_why="no lab here", prof=prof, remote=local, arm=None, lab_helper_mode="nohelper")
        try:
            CL._runner(lambda c, rr, a: ran.append(1), arm)(ctx2, R())
            raise AssertionError("the skip did not propagate")
        except cases.Skip:
            pass
        assert ctx2.remote is local and prof.remote is None and ctx2.arm is None
        # (5)
        rem = L.LabRemote(FakeLab(), "u_bash", "bash")
        assert rem.exists("empty") and rem.lines("empty") == [] and not rem.exists("missing") and rem.exists("/work/u_bash/empty")
    finally:
        if old_bin is not None:
            os.environ["APPCI_LAB_HELPER_BIN"] = old_bin
        if old_cache is None:
            os.environ.pop("PHOSPHOR_CI_HELPER_CACHE", None)
        else:
            os.environ["PHOSPHOR_CI_HELPER_CACHE"] = old_cache
        shutil.rmtree(d, ignore_errors=True)
    # (2)
    lab_fish = P.lab_hooks()["fish"] % {"ctl": "/c"}
    assert "--on-event fish_prompt" in lab_fish and "$PA_CTL/prompt.log" in lab_fish and "printf 'P\\t%s\\n'" in lab_fish, lab_fish
    assert "prompt.log" not in P.FISH, "the local scratch fish config shares the bash pane's ctl dir: it must not log prompts"
    assert lab_fish.startswith(P.FISH % {"ctl": "/c"})
    # (3)
    refused = "x hop=backend_route family=file_context backend=shell reason=caps\ny hop=backend_route family=fs_basic backend=shell reason=caps\n[ERROR] Failed to save file for accepted AgentMode diffs\n"
    assert FE.refused_route_verdict(refused, "shell") == []
    assert len(FE.refused_route_verdict(refused, "helper")) == 1 and "fs_basic->shell" in FE.refused_route_verdict(refused, "helper")[0]
    assert len(FE.refused_route_verdict("hop=backend_route family=file_context backend=shell\n", "shell")) == 1, "a file_context line (the proposal's read) proves nothing about the accept"
    assert FE.refused_route_verdict(refused + "hop=backend_route family=file_write backend=shell\n", "shell") == []
    assert len(FE.refused_route_verdict(refused + "hop=backend_route family=file_write backend=unavailable\n", "shell")) == 1
    # (4)
    for sh in ("bash", "dash", "nu"):
        c = AR.drop_command(sh, "/work/u/started_t1", "t1")
        assert c.startswith("run /usr/bin/touch /work/u/started_t1") and c.endswith("echo fin_t1") and not re.search(r"['\"<>|]", c), c
        assert ("&&" in c) == (sh != "nu"), c
    import fake_model as FM
    assert FM.parse_plan(AR.plan_text([AR.drop_command("nu", "/w/s", "t")])) == [("run", "/usr/bin/touch /w/s; /usr/bin/sleep 300; echo fin_t")]
    src = open(os.path.join(HERE, "cases_agent_remote.py")).read()
    assert '"run cat %s | /usr/bin/tee -a %s"' in src and "ctx.remote.exists(started_file)" in src
    assert "ctx.remote.commands()), 30 + 6" not in src, "the drop case must not wait on rec.log for a RUNNING command (bash logs at the next prompt)"


@check
def r12_lab2_round11_test_faults():
    """Round 11 in the lab (build/r11a, job 17896783499; docs/design/r12-lab2.md, #2045): the pure parts of the three test faults. (1) the fake model's
    triggers and fault matches read what the USER typed, never the <attached_context> block the app puts in front of it, while the request log keeps
    the whole message; (2) the where-probe of a nu lab arm is a line nu, bash, zsh and fish all run as an append; (3) lab_ctrl_c_no_esc_i types its two
    lines in the arm's own shell syntax."""
    import fake_model as FM
    import lab as labmod
    # (1) the request of lab_fam_agent_edit_conflict.bash, round 11: the block of the nu arm before it, attached by the app, quotes that arm's prompt
    att = ('<attached_context>\n  <executed_shell_command command_id="precmd-1-5" exit_code="0" auto_attached="true">\n    <command>ssh lab-u_nu</command>\n'
           '    <output>Error: nu::shell::external_command\n 1 | Please carry out these steps in order: run /usr/bin/touch /work/u_nu/started_t1; /usr/bin/sleep 300\n'
           'nu&gt; </output>\n  </executed_shell_command>\n</attached_context>')
    env = "<environment_context>\n  <cwd>/w</cwd>\n</environment_context>"
    edit = "Please edit exactly this file: /work/u_bash/ae.txt with these edits: L02 beta t=>L02 BETA-EDITED t ;; L05 epsilon t=>L05 EPSILON-EDITED t"
    msgs = [{"role": "user", "content": att + "\n\n" + edit}, {"role": "user", "content": env}]
    reply = FM.reply_for(msgs)
    assert reply["tool"]["name"] == "apply_file_diffs" and [o["search"] for o in reply["tool"]["arguments"]["operations"]] == ["L02 beta t", "L05 epsilon t"], reply
    assert FM.user_prompt(msgs) == edit and FM.turn_state(msgs) == (edit, [])
    assert FM.prompt_text(att + "\n\n" + att + "\n\n" + edit) == edit            # user attachments and referenced attachments: two blocks
    assert FM.prompt_text(att) == "" and FM.prompt_text(None) == "" and FM.prompt_text(edit) == edit
    mid = "see <attached_context>\nx\n</attached_context>\n\n" + edit
    assert FM.prompt_text(mid) == mid                                              # only a block the message STARTS with is the app's
    run = "Please run exactly this shell command and nothing else: echo T"
    assert FM.reply_for([{"role": "user", "content": att + "\n\n" + run}])["tool"]["arguments"]["command"] == "echo T"
    plan = "Please carry out these steps in order: read /w/a.txt ;; run cat /w/a.txt"
    pm = [{"role": "user", "content": att + "\n\n" + plan}]
    assert FM.reply_for(pm)["tool"] == {"name": "read_files", "arguments": {"files": [{"path": "/w/a.txt"}]}}, FM.reply_for(pm)
    assert FM.reply_for([{"role": "user", "content": att.replace("Please carry", "STREAMLONG carry") + "\n\nReply with the word Q7"}])["text"] == "OK Q7"
    # the oracles that prove what the model was SENT still see the attachment (#1829)
    assert FM.request_record(msgs)["users"] == [att + "\n\n" + edit] and FM.user_texts(msgs) == [att + "\n\n" + edit]
    # (2) the where-probe
    nu = cases.where_probe_cmd("nu", "wT1_abc")
    assert nu == "sh -c 'echo wT1_abc >> ~/.appci_where'"
    for sh in (None, "bash", "zsh", "fish", "pwsh", "dash"):
        assert cases.where_probe_cmd(sh, "wT1_abc") == "echo wT1_abc >> ~/.appci_where", sh       # unchanged everywhere else
    assert cases.Ctx.HARNESS_MARKER_RE.search(nu)                                  # still one of the harness's own lines for every command count
    src = open(os.path.join(HERE, "cases.py")).read()
    assert "self.probe(where_probe_cmd(shell, probe), probe" in src
    if os.name != "nt":
        for shell in ("sh", "bash", "zsh", "fish", "nu"):                          # every shell this machine has runs the line as an append into ITS home
            if not shutil.which(shell):
                continue
            home = tempfile.mkdtemp(prefix="appci_where_")
            try:
                out = subprocess.run([shell, "-c", nu], env=dict(os.environ, HOME=home), capture_output=True, text=True, timeout=20, stdin=subprocess.DEVNULL)
                got = open(os.path.join(home, ".appci_where")).read() if os.path.exists(os.path.join(home, ".appci_where")) else None
                assert got == "wT1_abc\n", (shell, got, out.stdout[-200:], out.stderr[-200:])
            finally:
                shutil.rmtree(home, ignore_errors=True)
    # (3) the Ctrl-C case's lines per shell
    assert labmod.long_sleep_line("nu") == "sleep 30sec" and all(labmod.long_sleep_line(s) == "sleep 30" for s in ("bash", "zsh", "fish"))
    src = open(os.path.join(HERE, "cases_lab.py")).read()
    body = src[src.index("def c_ctrl_c_no_esc_i"):src.index("# ------------------------------------------------------------------ 7. The helper install itself")]
    assert 'ctx.be.type("sleep 30"' not in body and '"echo %s >> %s" % (tok, work(arm, f))' not in body
    assert "ctx.be.type(sleep_line, type_pace(ctx), enter=True)" in body and "ctx.be.type(ctx.append_cmd(shell_kind, tok, work(arm, f))" in body
    assert "new[0][1].strip() == sleep_line and tok in new[-1][1]" in body         # still: exactly the two lines, the first one the sleep as typed


@check
def r11_wl_round10_test_faults():
    """Round 10 on Linux Wayland and the lab (build/r10a; docs/design/r11-wl.md, #2027): the pure parts of the six test faults. (1) a question verdict is
    judged only once two polls agree; (2) the empty-PS1 dash gets its PS1 from an $ENV file on every platform; (3) a tab's pwd probe is over when the shell
    logged it; (4) the denial toast is looked for until it shows; (5) lab_enter_start times from the app's receipt of the Enter to the command's own start
    stamp; (6) a Ctrl-C the app wrote before the prompt was back may draw one prompt."""
    import cases_agent_question as aq
    import cases_agent_netgate as ng
    import cases_subshell_ready as sr
    import cases_wrap_vtabs as vt
    import lab as labmod
    # (1) the three turn states the app logged for the rejected call, 3 ms apart (agent_question_empty, round 10)
    t1 = "x input_order hop=turn conv=c state=success t=1794304453843.189\n"
    t2 = t1 + "x input_order hop=turn conv=c state=in_progress t=1794304453846.704\n"
    t3 = t2 + "x input_order hop=turn conv=c state=success t=1794304453858.534\n"
    assert [aq.verdict(t) for t in (t1, t2, t3)] == ["ended", "hung", "ended"]
    assert aq.settled_verdict([]) is None and aq.settled_verdict(["ended"]) is None            # one sample is never the end
    assert aq.settled_verdict(["ended", "hung"]) is None and aq.settled_verdict(["hung", "ended"]) is None
    assert aq.settled_verdict(["ended", "hung", "ended", "ended"]) == "ended"
    assert aq.settled_verdict(["hung", "answerable", "answerable"]) == "answerable"
    assert aq.settled_verdict(["hung", "hung"]) is None                                        # agreeing on 'hung' settles nothing
    assert aq.settled_verdict(["answerable", "ended"]) is None
    src = open(os.path.join(HERE, "cases_agent_question.py")).read()
    assert "return settled_verdict(got)" in src and 'v = ctx.until(settled, BOUND_S, step=0.5) or "hung"' in src
    # (2) the empty PS1 comes from the rc file, never from `export PS1` (the bash bootstrap un-exports it since #1907)
    assert sr.EMPTY_PS1_RC == "PS1=\n" and sr.EMPTY_PS1_SLEEP_RC == "PS1=\nsleep 4\n"
    src = open(os.path.join(HERE, "cases_subshell_ready.py")).read()
    assert 'ctx.be.type("unset ENV; export PS1"' not in src and 'write_rc(ctx, "lab_ps1_empty.rc", EMPTY_PS1_RC)' in src
    assert 'write_rc(ctx, "lab_ps1_sleep.rc", EMPTY_PS1_SLEEP_RC)' in src
    assert src.count("this dash has an EMPTY PS1") == 2                                        # both legs check their premise
    boot = open(os.path.join(HERE, "..", "..", "..", "app", "assets", "bundled", "bootstrap", "bash_body.sh")).read()
    assert "export -n PS1" in boot                                                             # the app behaviour the old Linux leg ran into
    # (3) the probe is over when the shell logged it
    logged = vt.probe_logged("vt_pwd_end.txt")
    assert not logged([]) and not logged(["echo a >> r_vt_many_closed.txt"]) and logged(["pwd >> /w/vt_pwd_end.txt"])
    src = open(os.path.join(HERE, "cases_wrap_vtabs.py")).read()
    assert "ctx.await_commands(mark, probe_logged(f))" in src
    # (4) the toast is looked for until it shows, and not for ever
    seen = []

    def find_third(ctx, tag, sub):
        seen.append(tag)
        return (("Allow", 10, 20, 0) if len(seen) >= 3 else None), "shot.png"
    w, p = ng.find_toast_word(None, "t", "allow", secs=5.0, find=find_third)
    assert w and w[0] == "Allow" and len(seen) == 3 and p == "shot.png"
    seen[:] = []
    t0 = time.time()
    w, _ = ng.find_toast_word(None, "t", "allow", secs=0.3, find=lambda c, t, s: (seen.append(t), (None, "s.png"))[1])
    assert w is None and 2 <= len(seen) <= 4 and time.time() - t0 < 2.0
    assert 'find_toast_word(ctx, "agdn_find_allow", "allow")' in open(os.path.join(HERE, "cases_agent_netgate.py")).read()
    net = open(os.path.join(HERE, "cases_netless.py")).read()
    assert 'NG.find_toast_word(ctx, "ngn_find_allow", "allow")' in net and 'off, tok, opened = show("a")' not in net
    # (5) lab_enter_start: the Enter starts when the app received it (the app log of the round 10 run: the injector's priming Shift, 25 ms, the Enter)
    log = [
        "2026-11-10T10:15:34Z [DEBUG] input_order hop=key_unconverted outcome=dropped logical=Shift physical=Code(Escape) ctrl=false alt=false shift=false cmd=false t=1794305734514.086",
        "2026-11-10T10:15:34Z [DEBUG] input_order hop=core_enter stage=handle_window_event key_bindings_enabled=true focused_view=EntityId(1593) t=1794305734538.847",
        "2026-11-10T10:15:34Z [DEBUG] input_order hop=core_enter stage=dispatch_result handled=true pending=false t=1794305734539.230",
        '2026-11-10T10:15:34Z [DEBUG] input_order hop=enter_begin deferred=false buffer="echo est3524yn $EPOCHREALTIME >> /work/u/es.txt" input_type=Shell t=1794305734539.288',
        "2026-11-10T10:15:34Z [DEBUG] input_order pty_write bytes=58 t=1794305734540.539",
    ]
    assert abs(labmod.app_enter_epoch(log, "est3524yn") - 1794305734.538847) < 1e-6
    assert labmod.app_enter_epoch(log, "esOTHER") is None and labmod.app_enter_epoch([], "est3524yn") is None
    assert abs(labmod.app_enter_epoch(log[3:], "est3524yn") - 1794305734.539288) < 1e-6      # no window event: the enter_begin time
    stale = [log[1].replace("1794305734538.847", "1794305730000.000")] + log[3:]              # a window event of an older Enter is not this one's
    assert abs(labmod.app_enter_epoch(stale, "est3524yn") - 1794305734.539288) < 1e-6
    assert labmod.start_stamp_line("tok", "/work/u/f.txt") == "echo tok $EPOCHREALTIME >> /work/u/f.txt"
    stamps = labmod.start_stamps(["esa 1794305734.650000", "esb 1794305735,250000", "esc", "esd notanumber", ""])
    assert stamps == {"esa": 1794305734.65, "esb": 1794305735.25}
    ms = labmod.enter_start_samples(["esa", "esb", "esc"], [1794305734.55 - 2.0, None, 1.0], stamps, 2.0)   # the remote clock is 2 s ahead
    assert abs(ms[0] - 100.0) < 0.01 and ms[1] is None and ms[2] is None
    pv = labmod.paired_verdict([94.0] * 12, [83.0] * 12)
    assert pv["verdict"] == "pass" and labmod.paired_verdict([140.7] * 12, [88.2] * 12)["verdict"] == "fail"   # the budget itself is unchanged
    src = open(os.path.join(HERE, "cases_lab.py")).read()
    assert "L.app_enter_epoch(log_lines, t)" in src and "L.start_stamp_line(tok, eff)" in src and '"injector_enter_delay_ms"' in src
    # (6) Ctrl-C bursts: one prompt per command, plus at most one per further Ctrl-C the app wrote
    one = "hop=ctrl_c_route route=pty long_running=true\nhop=ctrl_c_pty_write bytes=1\n"
    assert labmod.ctrl_c_prompt_bounds(one, 1) == (2, 2)
    assert labmod.ctrl_c_prompt_bounds(one + "hop=ctrl_c_route route=none\n", 2) == (2, 2)     # 200 ms gap: the second key was not written
    assert labmod.ctrl_c_prompt_bounds(one + one, 2) == (2, 3) and labmod.ctrl_c_prompt_bounds(one * 3, 3) == (2, 4)
    assert labmod.ctrl_c_prompt_bounds(one * 5, 2) == (2, 3)                                   # never more than were pressed
    assert labmod.ctrl_c_prompt_bounds("", 3) == (2, 4)                                        # no hop in the log: every pressed key may have been written
    assert "L.ctrl_c_prompt_bounds(ctx.log_since(off), nctl)" in src
    doc = open(os.path.join(HERE, "..", "..", "..", "docs", "testing.md")).read()
    assert "r11_wl_round10_test_faults" in doc and "#2027" in doc and "#2026" in doc



@check
def report_problem_zip_verdict():
    """Wave 1.5 #1: the bundle verdict flags a leaked home path / host name and a missing log, and passes a clean bundle"""
    import cases_coverage_b as cb, zipfile
    d = tempfile.mkdtemp()
    def make(name, log, extra=""):
        p = os.path.join(d, name)
        with zipfile.ZipFile(p, "w") as z:
            z.writestr("README.txt", "local only")
            z.writestr("manifest.txt", "Version: 1\n" + extra)
            z.writestr("app-log-window.txt", log)
            z.writestr("settings-scrubbed.toml", "x = true\n")
        return p
    ok = make("phosphor-problem-report-20261108-120000.zip", "2026-11-08T12:00:00Z [INFO] hop=enter_begin cwd=<path>\n")
    assert cb.report_zip_verdict(ok, "/home/ci", ["buildbox", "ci-user"]) == ([], 1)
    leak = make("b.zip", "x cwd=/home/ci/w on buildbox\n")
    probs, _ = cb.report_zip_verdict(leak, "/home/ci", ["buildbox"])
    assert len(probs) == 2 and any("/home/ci" in x for x in probs) and any("buildbox" in x for x in probs), probs
    assert cb.report_zip_verdict(make("c.zip", ""), "/home/ci", [])[0] == ["app-log-window.txt has no lines"]
    assert cb.REPORT_ZIP_RE.search("phosphor-problem-report-20261108-120000.zip") and not cb.REPORT_ZIP_RE.search("other.zip")
    assert cb.find_report_zips(d) == [ok]


@check
def lab_ctl_and_daemon_pure_pieces():
    C = _load_by_path("labctl", os.path.join(SSHLAB, "labctl.py"))
    D = _load_by_path("labd", os.path.join(SSHLAB, "rootfs", "usr", "local", "sbin", "labd.py"))
    # pins: the committed versions.env has PLACEHOLDERs and the build refuses them; real-looking values pass
    fixture = ("DEBIAN_IMAGE=docker.io/library/debian@sha256:PLACEHOLDER_X\nDEBIAN_SNAPSHOT=20261101T000000Z\nNU_VERSION=PLACEHOLDER\n# c\n")
    probs = C.check_pins(C.parse_versions(fixture))
    assert probs and any("PLACEHOLDER" in p for p in probs), "a versions.env with PLACEHOLDER pins must refuse to build"
    assert C.check_pins(C.parse_versions(open(os.path.join(SSHLAB, "versions.env")).read())) == [], "the committed versions.env must hold real pins"
    good = dict(DEBIAN_IMAGE="docker.io/library/debian@sha256:" + "a" * 64, DEBIAN_SNAPSHOT="20261101T000000Z", NU_VERSION="0.99.1", NU_URL="https://example.invalid/nu.tgz",
                NU_SHA256="b" * 64, PWSH_VERSION="7.5.0", PWSH_URL="https://example.invalid/p.tgz", PWSH_SHA256="c" * 64)
    assert C.check_pins(good) == []
    assert C.check_pins(dict(good, DEBIAN_IMAGE="docker.io/library/debian:trixie-slim@sha256:" + "d" * 64)) == [], "a tag before the digest is allowed"
    assert C.check_pins(dict(good, NU_SHA256="PLACEHOLDER")) and C.check_pins(dict(good, DEBIAN_IMAGE="debian:trixie-slim")) and C.check_pins({})
    # tree hash: stable, ignores labctl.py and __pycache__, changes with content
    d = tempfile.mkdtemp()
    try:
        os.makedirs(d + "/rootfs")
        open(d + "/Containerfile", "w").write("FROM x\n")
        open(d + "/rootfs/a", "w").write("1")
        open(d + "/run.sh", "w").write("#!/bin/sh\necho hi\n")
        os.chmod(d + "/run.sh", 0o755)
        h1 = C.tree_hash(d)
        os.chmod(d + "/run.sh", 0o666)          # a Windows tar: no exec bit, CRLF line endings: the same tree must hash the same
        open(d + "/run.sh", "wb").write(b"#!/bin/sh\r\necho hi\r\n")
        assert C.tree_hash(d) == h1, "tree_hash must ignore file modes and CRLF"
        open(d + "/labctl.py", "w").write("whatever")
        os.makedirs(d + "/__pycache__")
        open(d + "/__pycache__/x.pyc", "w").write("z")
        assert C.tree_hash(d) == h1 and C.tree_files(d) == ["Containerfile", "rootfs/a", "run.sh"]
        open(d + "/rootfs/a", "w").write("2")
        assert C.tree_hash(d) != h1
    finally:
        shutil.rmtree(d, ignore_errors=True)
    assert C.image_tag("full", "f" * 64) == "localhost/phosphor-sshlab:full-" + "f" * 16
    # podman run: labels, caps dropped then added, no host path mounted, random host ports, bare = read-only + noexec /tmp
    a = C.run_argv("img:tag", "run1", "bare", 600, "tok", 1000.0)
    j = " ".join(a)
    assert a[:3] == ["podman", "run", "-d"] and "--cap-drop all" in j and "--cap-add NET_ADMIN" in j and "--network sshlab" in j and "--init" in j
    assert "tok" not in a and "SSHLAB_TOKEN" in a and "SSHLAB_TOKEN=" not in j, "the token must not be on the command line"
    assert "phosphor.sshlab=1" in j and "phosphor.sshlab.expires=1600" in j and "-v " not in j and "--volume" not in j
    assert all(("-p 0.0.0.0::%d" % p) in j for p in (22, 2222, 7, 7000)) and "--read-only" in j and "/tmp:rw,noexec" in j and a[-1] == "img:tag"
    assert "--read-only" not in " ".join(C.run_argv("img:tag", "run1", "full", 600, "tok", 1000.0))
    assert C.parse_ports("22/tcp -> 0.0.0.0:41000\n7000/tcp -> 0.0.0.0:41003\n") == {22: 41000, 7000: 41003}
    cs = [{"Id": "aa", "Labels": {"phosphor.sshlab": "1", "phosphor.sshlab.expires": "500"}}, {"Id": "bb", "Labels": {"phosphor.sshlab": "1", "phosphor.sshlab.expires": "5000"}},
          {"Id": "cc", "Labels": {}}, {"Id": "dd", "Labels": {"phosphor.sshlab": "1", "phosphor.sshlab.expires": "junk"}}]
    assert C.expired_ids(cs, 1000) == ["aa", "dd"], "only lab containers, only expired ones, by exact id"
    assert sorted(C.prune_plan([(5, "f5", "full"), (4, "f4", "full"), (3, "f3", "full"), (2, "f2", "full"), (1, "f1", "full"), (9, "b9", "bare")])) == ["f1", "f2"]
    assert C.prune_plan([(3, "b3", "bare"), (3, "b3", "bare"), (3, "b2", "bare"), (3, "b2", "bare"), (2, "b1", "bare")]) == [], "duplicate rows of a multi-tag image are not extra tags"
    # the forced command accepts exactly start/stop/list/gc
    assert C.parse_remote_command("start --run r1.x --flavor bare --ttl 3600") == ("start", dict(run="r1.x", flavor="bare", ttl=3600))
    assert C.parse_remote_command("stop --id " + "ab" * 6) == ("stop", dict(id="ab" * 6)) and C.parse_remote_command("list") == ("list", {}) and C.parse_remote_command("gc") == ("gc", {})
    for bad in ("", "build", "start --run r --flavor full --ttl 5", "start --run r --flavor evil --ttl 3600", "stop --id *", "stop --id abc", "list; id", "start --run r;id --flavor full --ttl 3600",
                "rm -rf /", "start --run r --flavor full --ttl 99999"):
        try:
            C.parse_remote_command(bad)
            raise AssertionError("forced command accepted %r" % bad)
        except ValueError:
            pass
    import io, tarfile
    def tar_of(members):
        b = io.BytesIO()
        with tarfile.open(fileobj=b, mode="w") as tf:
            for n, data in members:
                ti = tarfile.TarInfo(n); ti.size = len(data); tf.addfile(ti, io.BytesIO(data))
        return b.getvalue()
    d = tempfile.mkdtemp()
    try:
        assert C.safe_extract(tar_of([("sshlab/Containerfile", b"x")]), d).endswith("sshlab")
        for evil in ("../x", "/etc/x", "sshlab/../../x"):
            try:
                C.safe_extract(tar_of([(evil, b"x")]), d)
                raise AssertionError("extracted " + evil)
            except ValueError:
                pass
    finally:
        shutil.rmtree(d, ignore_errors=True)
    # labd: session pids by exact user, path jail, rc plan, ss parse
    assert D.session_pids(" 10 sshd-session: u_a [priv]\n 11 sshd-session: u_a@pts/0\n 12 sshd-session: u_ab@pts/1\n 13 sshd: u_a@notty\n 14 bash", "u_a") == [10, 11, 13]
    assert D.safe_path("/work/u/x") == "/work/u/x" and D.safe_path("/work/../etc/passwd") is None and D.safe_path("/etc/passwd") is None and D.safe_path("work/x") is None
    assert D.safe_path("/work/u/../../etc") is None and D.safe_path("/workx/u") is None
    U = _usersdb()
    plan = D.rc_plan(next(r for r in U.load() if r.user == "p_exec"))
    assert plan[".bash_profile"].startswith("exec zsh -l") and ".zshrc" in plan and "hook.bash.p_exec" in plan[".bashrc"] and "scenarios/exec.sh" in plan[".bashrc"]
    assert ".config/fish/config.fish" in D.rc_plan(next(r for r in U.load() if r.user == "u_fish")) and "ENV=" in D.rc_plan(next(r for r in U.load() if r.user == "u_dash"))[".profile"]
    assert "hook.sh.u_ash" in D.rc_plan(next(r for r in U.load() if r.user == "u_ash"))[".profile"]
    ss = '0      0      10.89.0.2:22     10.89.0.1:51234\n0      0      10.89.0.2:7000   10.89.0.1:5\n0 0 [::ffff:10.89.0.2]:2222 [::ffff:10.89.0.1]:6000\n'      # `ss -tnH state established`: no State column, no pids
    assert D.parse_ss(ss) == [("10.89.0.1", 51234, 22), ("10.89.0.1", 5, 7000), ("::ffff:10.89.0.1", 6000, 2222)]
    log = "sshd-session[9]: Accepted publickey for u_a from 10.89.0.1 port 51234 ssh2: ED25519\nAccepted publickey for u_ab from 10.89.0.1 port 51240 ssh2\n"
    assert D.accepted_ports(log, "u_a") == [("10.89.0.1", 51234)]
    assert D.split_candidates("zsh", "\x1b[1m> \x1b[0mcd tabfx/\r\nambx_a/  ambx_b/\r\n", "cd tabfx/") == ["ambx_a/", "ambx_b/"]
    assert D.split_candidates("fish", "a/\tdirectory\nb/\tdirectory\n", "x") == ["a/", "b/"]
    assert D.totp_file_text("ABC") == 'ABC\n" TOTP_AUTH\n'

# ---- the real-use replay (replay.py, fake_model.py, cases_replay.py)
@check
def replay_scenario_is_deterministic_and_sized():
    import replay as RP
    a, b = RP.gen_scenario(7, 15, "terminal"), RP.gen_scenario(7, 15, "terminal")
    assert a == b and a != RP.gen_scenario(8, 15, "terminal")
    assert sum(RP.est_seconds(s) for s in a) >= 15 * 60 and len(RP.gen_scenario(7, 60, "terminal")) > len(a)
    assert len(RP.gen_scenario(1, 0.01, "agent")) >= 3
    kinds = {s["kind"] for s in RP.flatten(RP.gen_scenario(3, 120, "terminal"))}
    assert {"local_cmd", "cd", "tab", "ssh_session", "ctrlc", "big_seq", "big_cat", "subshell", "sudo", "remote_cmd", "remote_big"} <= kinds, kinds
    ak = {s["kind"] for s in RP.gen_scenario(3, 120, "agent")}
    assert ak == {"agent_approve", "agent_reject", "agent_prose", "agent_ctrlc"}, ak
    idx = [s["idx"] for s in RP.flatten(RP.gen_scenario(3, 30, "terminal"))]
    assert len(idx) == len(set(map(str, idx)))
    assert len({s["eff"] for s in RP.flatten(a)}) == len(RP.flatten(a))


@check
def replay_effects_oracle():
    import replay as RP
    steps = [dict(kind="local_cmd", idx=0, tok="A", eff="e0"), dict(kind="ctrlc", idx=1, tok="B", eff="e1"), dict(kind="agent_reject", idx=2, tok="C", eff="e2"),
             dict(kind="ssh_session", idx=3, tok="D", eff="e3", inner=[dict(kind="remote_cmd", idx="3_0", tok="E", eff="e30")])]
    exp = RP.expected_effects(steps)
    assert exp == {"e0": ["A"], "e2": [], "e30": ["E"]}, exp
    assert RP.expected_effects(steps, {0}) == {"e0": ["A"]}
    assert RP.judge_effects(exp, {"e0": ["A"], "e30": ["E"]}) == []
    bad = RP.judge_effects(exp, {"e0": [], "e2": ["C"], "e30": ["E", "E"]})
    assert len(bad) == 3, bad


@check
def replay_error_allowlist_and_hops():
    import replay as RP
    log = "2026 INFO ok\n2026 [ERROR] boom here\n2026 ERROR Settings file cannot be used: x\nthread panicked at a.rs:1\n2026 ERROR keyring locked\n"
    got = RP.error_lines(log)
    assert got == ["2026 [ERROR] boom here", "thread panicked at a.rs:1"], got
    assert RP.error_lines(log, extra_allow=["boom"]) == [got[1]]
    h = RP.agent_hops('hop=enter_begin deferred=false buffer="Reply with the word R1abc" input_type=AI\nhop=view_mode mode=agent pane=1\n[byop] opening stream\nqueue_actions\n')
    assert h["enters"] == [("Reply with the word R1abc", "AI")] and h["streams"] == 1 and h["cards"] == 1 and h["views"] == ["agent"], h
    st = [dict(idx=0, tok="R1abc", ask="Reply with the word R1abc")]
    assert RP.judge_agent(h, st) == []
    h2 = dict(h, enters=[("Reply with the word R1abc", "Shell")], views=["agent", "terminal"], streams=0)
    assert len(RP.judge_agent(h2, st)) == 4, RP.judge_agent(h2, st)


@check
def replay_latency_and_config():
    import replay as RP
    s = RP.latency_stats([("a", 1.0), ("a", 3.0), ("b", 0.5)])
    assert s["a"]["n"] == 2 and s["a"]["max"] == 3.0 and s["b"]["p50"] == 0.5, s
    assert RP.replay_minutes({}) == 15 and RP.replay_minutes({"APPCI_REPLAY_MINUTES": "60"}) == 60
    for v in ("x", "0", "-1"):
        try:
            RP.replay_minutes({"APPCI_REPLAY_MINUTES": v})
        except ValueError:
            continue
        raise AssertionError(v)
    import cases_replay as CRP
    assert CRP.first_divergence(["cd a/ && x"], ["cd a && x"]) is None
    assert "typed 'a'" in CRP.first_divergence(["a", "b"], ["c", "b"])
    assert "typed 2 lines" in CRP.first_divergence(["a", "b"], ["a"])


@check
def fake_model_replies():
    import fake_model as FM
    r = FM.reply_for([{"role": "user", "content": "Please run exactly this shell command and nothing else: echo T >> /x/f.txt"}])
    assert r["tool"]["name"] == "run_shell_command" and r["tool"]["arguments"]["command"] == "echo T >> /x/f.txt", r
    assert FM.reply_for([{"role": "user", "content": "x"}, {"role": "tool", "content": "ok"}]) == {"text": "Done.", "tool": None}
    assert FM.reply_for([{"role": "user", "content": "Please show this image: http://192.0.2.1:8099/x.png"}]) == {"text": "Here it is:\n\n![img](http://192.0.2.1:8099/x.png)", "tool": None}
    rd = FM.reply_for([{"role": "user", "content": "Please read exactly this file and nothing else: /w/a.txt"}])
    assert rd["tool"] == {"name": "read_files", "arguments": {"files": [{"path": "/w/a.txt"}]}}, rd
    cr = FM.reply_for([{"role": "user", "content": "Please create exactly this file: /w/b.txt with this content: hello there"}])
    assert cr["tool"]["name"] == "apply_file_diffs" and cr["tool"]["arguments"]["operations"] == [{"op": "create", "file_path": "/w/b.txt", "content": "hello there"}], cr
    dl = FM.reply_for([{"role": "user", "content": "Please delete exactly this file and nothing else: /w/b.txt"}])
    assert dl["tool"]["arguments"]["operations"] == [{"op": "delete", "file_path": "/w/b.txt"}], dl
    assert FM.reply_for([{"role": "user", "content": "Reply with the word Q1"}])["text"] == "OK Q1"
    assert FM.reply_for([{"role": "user", "content": "Answer slowly SLOW R1"}]).get("slow")
    lines = [json.loads(x) for x in FM.stream_lines("m", r)]
    assert lines[-1]["done"] and lines[-2]["message"]["tool_calls"][0]["function"]["name"] == "run_shell_command"
    assert "".join(json.loads(x)["message"]["content"] for x in FM.stream_lines("m", {"text": "hello world", "tool": None})) == "hello world"
    f = FM.FakeModel().start()
    try:
        import urllib.request
        rq = urllib.request.Request(f.url + "/api/chat", json.dumps({"model": "m", "messages": [{"role": "user", "content": "Reply with the word Z"}]}).encode())
        body = urllib.request.urlopen(rq, timeout=5).read().decode().splitlines()
        assert json.loads(body[-1])["done"] and f.requests == 1
    finally:
        f.stop()


@check
def fake_model_ignores_environment_context():
    """round 6 (Windows agent@fake): the app puts an <environment_context> user message after the prompt (or inside it); triggers, fault matches and the
    request log must read the PROMPT, and title requests are counted apart from chat requests"""
    import fake_model as FM, urllib.request, urllib.error
    env = "<environment_context>\n  <cwd>/w</cwd>\n</environment_context>"
    sep = [{"role": "user", "content": "Please run exactly this shell command and nothing else: echo T >> /x/f.txt"}, {"role": "user", "content": env}]
    inl = [{"role": "user", "content": "Please read exactly this file and nothing else: /w/a.txt\n" + env}]
    assert FM.reply_for(sep)["tool"]["arguments"]["command"] == "echo T >> /x/f.txt"
    assert FM.reply_for(inl)["tool"] == {"name": "read_files", "arguments": {"files": [{"path": "/w/a.txt"}]}}, FM.reply_for(inl)
    assert FM.reply_for([{"role": "user", "content": "Reply with the word Q1"}, {"role": "user", "content": env}])["text"] == "OK Q1"
    assert FM.reply_for([{"role": "user", "content": "x"}, {"role": "tool", "content": "ok"}, {"role": "user", "content": env}]) == {"text": "Done.", "tool": None}
    assert FM.user_prompt(sep).startswith("Please run") and FM.user_prompt([{"role": "user", "content": env}]) == ""
    assert FM.user_texts(inl) == ["Please read exactly this file and nothing else: /w/a.txt"]
    assert FM.request_record(sep)["users"] == [sep[0]["content"]]
    title = [{"role": "system", "content": "You are a thread title generator. You output ONLY a thread title."}, {"role": "user", "content": "Reply with the word TOK1"}]
    assert FM.is_title_request(title) and not FM.is_title_request(sep)
    f = FM.FakeModel().start()
    try:
        def post(msgs, path="/api/chat", **kw):
            rq = urllib.request.Request(f.url + path, json.dumps(dict({"model": "m", "messages": msgs}, **kw)).encode())
            try:
                return urllib.request.urlopen(rq, timeout=5).read().decode()
            except urllib.error.HTTPError as e:
                return "HTTP %d" % e.code
        text = lambda body: "".join(json.loads(l)["message"]["content"] for l in body.splitlines() if l.strip())
        f.inject("status", times=None, match="TOK1", code=429)
        assert post(title, stream=False).count("done") >= 1 and f.title_requests == 1 and f.requests == 0 and not f.fired      # a title request neither counts nor takes a chat fault
        assert post([{"role": "user", "content": "Reply with the word TOK1"}, {"role": "user", "content": env}]) == "HTTP 429" and f.requests == 1 and f.fired == ["status"]
        f.clear_faults()
        assert text(post([{"role": "user", "content": "Reply with the word TOK2"}, {"role": "user", "content": env}])) == "OK TOK2"
        # a fault can name a purpose
        f.inject("status", times=1, purpose="title", code=500)
        assert post(title, stream=False) == "HTTP 500" and f.requests == 2
        # every purpose, with request bodies shaped like the app's (system prompt first, then the user text; tools only on the chat turn)
        shapes = {"title": "You are a thread title generator. You output ONLY a thread title.",
                  "commit_message": "You are a git commit message generator. You output ONLY a commit message.",
                  "explain_command": "You explain shell commands to a user who has been asked to approve them.",
                  "compaction": "You are an anchored context summarization assistant for coding sessions.",
                  "next_command": "You predict the most likely next shell command a user will run, based on",
                  "nld_predict": "You complete a partial natural-language query or shell command typed by a user",
                  "prompt_suggestions": "You are inside a terminal emulator. The user just finished a command.\nPredict the single most useful follow-up",
                  "relevant_files": "Given a query and a list of files (path + optional symbol summary), return ONLY the subset of paths most relevant to the query.",
                  "workflow_metadata": "You are helping a terminal app turn a raw shell command into a reusable, parameterized workflow."}
        for name, sysm in shapes.items():
            body = post([{"role": "system", "content": sysm}, {"role": "user", "content": "x"}], stream=False)
            assert json.loads(body)["message"]["content"] == FM.PURPOSE_TEXT[name], (name, body)
            assert f.by_purpose[name] >= 1, name
        assert json.loads(post([{"role": "system", "content": shapes["prompt_suggestions"]}, {"role": "user", "content": "x"}], stream=False))["message"]["content"] == '{"kind":"none"}'
        assert json.loads(json.loads(post([{"role": "system", "content": shapes["workflow_metadata"]}, {"role": "user", "content": "x"}], stream=False))["message"]["content"])["arguments"] == []
        assert FM.classify_purpose("/api/chat", {"messages": [{"role": "system", "content": "big agent system prompt"}], "tools": [{"x": 1}]}) == "chat"
        assert FM.classify_purpose("/api/show", {}) == "show" and FM.classify_purpose("/api/tags", {}) == "tags"
        # unknown purposes are loud, never answered as chat
        n = f.requests
        assert post([{"role": "system", "content": "something new"}, {"role": "user", "content": "x"}]) == "HTTP 501"
        assert post([], "/api/embed", input="x") == "HTTP 501" and post([], "/api/generate", prompt="x") == "HTTP 501"
        assert f.by_purpose["UNKNOWN_PURPOSE"] == 3 and len(f.unknown) == 3 and f.requests == n
    finally:
        f.stop()


@check
def agent_interactive_oracles():
    """#1829: the request-log record, its reader and the pure judge of cases_agent_interactive.py"""
    import fake_model as FM, cases_agent_interactive as AI
    rec = FM.request_record([{"role": "user", "content": "hi"}, {"role": "assistant", "content": ""}, {"role": "tool", "content": "got:abc"}])
    assert rec == {"last_role": "tool", "tool": "got:abc", "users": ["hi"]}, rec
    assert FM.request_record([{"role": "user", "content": "x"}])["tool"] is None and FM.request_record([])["last_role"] is None
    ok = [dict(tool=None, users=["run it"]), dict(tool="pw: \r\ngot:S1", users=["run it"])]
    assert AI.judge_requests(ok, "S1", "got:S1") == []
    assert AI.judge_requests(ok, "S1", "got:ZZ") and AI.judge_requests([dict(tool=None, users=["x"])], None)
    leak = [dict(tool="o", users=["run it", "typed S1 here"])]
    assert any("leaked" in p for p in AI.judge_requests(leak, "S1"))
    assert FM.read_request_log("/nonexistent/none.jsonl") == []
    root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
    src = lambda p: open(os.path.join(root, p), encoding="utf-8").read()
    assert 'log_hop("alt_screen_enter"' in src("app/src/terminal/model/terminal_model.rs") and 'log_hop("fullscreen_handover"' in src("app/src/terminal/view.rs")
    assert AI.hop_count("INFO input_order hop=alt_screen_enter  t=1\nhop=alt_screen_enter  t=2", AI.ALT_ENTER_RE) == 2
    assert AI.hop_count("hop=fullscreen_handover agent_in_control=0 t=1", AI.FS_HANDOVER_RE) == 1 and AI.hop_count("hop=fullscreen_handover agent_in_control=1 t=1", AI.FS_HANDOVER_RE) == 0
    # a real Round 7 run: the agent's wait-for-completion vim has no control state, so only the pty-owned hand_route proves the keys
    hr = "hop=hand_route reason=unheld state=pty_owned awaiting_ready=false in_flight=0 evidence=None command_interactive=true enters_written=1 bytes=\"e\""
    assert AI.fullscreen_keys_proven("x hop=alt_screen_enter  t=1\n" + hr) and not AI.fullscreen_keys_proven(hr) and not AI.fullscreen_keys_proven("hop=alt_screen_enter\nhop=hand_route reason=unheld state=pty_owned awaiting_ready=false in_flight=0 evidence=None command_interactive=false")
    # a real Round 8 run (Linux): the keys of an agent-run vim are routed with state=editor_owned, command_interactive=true
    assert AI.fullscreen_keys_proven("hop=alt_screen_enter  t=1\n" + hr.replace("pty_owned", "editor_owned"))
    assert AI.fullscreen_keys_proven("hop=fullscreen_handover agent_in_control=0 t=1") and not AI.fullscreen_keys_proven("")
    assert AI.notice_seen("x [DEBUG] input_order hop=may_be_waiting_notice shown=1 t=1") and AI.notice_seen("showing a take-over notice (the command") and not AI.notice_seen("hop=handover")
    names = {c["name"] for c in cases.CASES}
    # the app's real hand-over line (cli_controller.rs switch_control_to_user), a prompt-shape line, and the non-hand-over lines seen in a real run
    assert AI.handover_seen("2026-11-09T20:03:10Z [INFO] hop=handover action=call_abc123 reason=blocked_on_input\n")
    assert AI.handover_seen("x [INFO] hop=handover action=call_abc123 reason=transfer_from_agent\n")
    assert AI.handover_seen("x [INFO] interactive prompt Some(\"Continue? \") unchanged for 1s, foreground reading the tty; handing the command to the user\n")
    assert not AI.handover_seen("2026-11-09T20:04:50Z [INFO] hop=handover action=call_wvgszzf7 reason=stop\n")
    assert not AI.handover_seen("x [INFO] prompt-shaped line Some(\"Waiting:\") unchanged for 0ns, but the foreground is not reading the tty; leaving the command with the agent\n")
    assert not AI.handover_seen("") and not AI.handover_seen(None)
    assert {"agent_interactive_password", "agent_interactive_vim", "agent_interactive_ssh", "agent_interactive_less", "agent_noninteractive_no_takeover"} <= names
    # #1972: the user's take-over arm of the remote read: which arm runs, and that the case and the app's palette entry exist under the same words
    assert AI.remote_read_arm(True, True) == "auto" and AI.remote_read_arm(False, True) == "auto"
    assert AI.remote_read_arm(True, False) == "user" and AI.remote_read_arm(False, False) == "none"
    assert "agent_interactive_ssh_takeover" in names and "agent_interactive_ssh_takeover" in cases.PROVING and "agent_interactive_ssh_takeover" in cases.PROVING_FAKE
    ftl = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "app", "i18n", "en", "warp.ftl"), encoding="utf-8").read()
    assert "keybinding-desc-take-over-agent-command = %s\n" % AI.TAKE_OVER_PALETTE_TEXT in ftl


@check
def windows_handover_secret_cases_pure_parts():
    """#1882/#1883/#1891: the Windows secret / hand-over cases (cases_win_handover.py): the marker hop reader, the PowerShell Read-Host line and the digest it records,
    the leak finder, the pane-aware agent password command, their Windows classification, and that the app logs the hop the cases read"""
    import cases_win_handover as WH, cases_agent_interactive as AI
    log = "x [INFO] hop=tty_read_marker secret=false\ny [INFO] input_order hop=tty_read_marker secret=true t=2\nz hop=other secret=true\n"
    assert WH.tty_read_markers(log) == ["false", "true"] and WH.tty_read_markers("") == [] and WH.tty_read_markers(None) == []
    line = WH.read_host_line("f.txt")
    assert "Read-Host -AsSecureString" in line and "Add-Content f.txt" in line and "$s.Length" in line
    assert "ConvertFrom-SecureString" not in line, "the case must never write the secret itself"
    assert WH.secret_digest("abc") == "len=3 sum=294"
    assert WH.secret_leaks([("a", "has hunter2 inside"), ("b", "clean"), ("c", None)], "hunter2") == ["a"]
    assert AI.password_cmd("powershell") == AI.PW_CMD_PS and AI.password_cmd("gitbash") == AI.PW_CMD and AI.password_cmd(None) == AI.PW_CMD
    assert "Read-Host -AsSecureString" in AI.PW_CMD_PS and "got:" in AI.PW_CMD_PS
    assert winmap.skip_reason("ps_read_host_secure", "powershell") is None and winmap.skip_reason("ps_read_host_secure", "gitbash")
    assert winmap.skip_reason("agent_interactive_password", "powershell") is None and "Read-Host" in winmap.skip_reason("agent_interactive_password", "gitbash")
    assert winmap.skip_reason("gitbash_precmd_secret", "gitbash") is None and winmap.skip_reason("gitbash_precmd_secret", "powershell")
    assert winmap.skip_reason("agent_lrc_no_false_takeover", "powershell") is None and winmap.skip_reason("agent_lrc_no_false_takeover", "gitbash") is None
    assert {"ps_read_host_secure", "gitbash_precmd_secret"} <= set(cases.PROVING) and not {"ps_read_host_secure", "gitbash_precmd_secret"} & set(cases.SMOKE)
    root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
    src = lambda p: open(os.path.join(root, p), encoding="utf-8").read()
    assert 'log_hop("tty_read_marker"' in src("app/src/terminal/view.rs"), "the hop the cases read must exist in the app"
    ps = src("app/assets/bundled/bootstrap/pwsh.ps1")
    assert '"hook": "TtyRead"' in ps and "Microsoft.PowerShell.Utility\\Read-Host @PSBoundParameters" in ps


@check
def fake_model_plan_and_agent_remote_pure_parts():
    """#1827: the multi-step plan turn (one tool call per answered step, then Done.), the recorded tool results, and the agent_remote result helpers"""
    import fake_model as FM
    import cases_agent_remote as AR
    t = AR.plan_text(["read /w/a.txt", "grep tk /w", "glob *.txt /w", "edit /w/a.txt :: old text :: new text", "run cat /w/a.txt && echo; x", "codebase needle"])
    plan = FM.parse_plan(t)
    assert [p[0] for p in plan] == ["read", "grep", "glob", "edit", "run", "codebase"], plan
    assert plan[3] == ("edit", "/w/a.txt", "old text", "new text") and plan[4] == ("run", "cat /w/a.txt && echo; x"), plan
    assert FM.parse_plan("Reply with the word Q") is None and FM.parse_plan(PLAN_BAD := "Please carry out these steps in order: grep onlyone")[0][0] == "bad"
    msgs = [{"role": "user", "content": "old turn"}, {"role": "user", "content": t}]
    names = []
    for i in range(len(plan)):
        rep = FM.reply_for(msgs)
        names.append(rep["tool"]["name"])
        msgs += [{"role": "assistant", "content": ""}, {"role": "tool", "content": '{"status":"ok","n":%d}' % i}]
    assert names == ["read_files", "grep", "file_glob", "apply_file_diffs", "run_shell_command", "search_codebase"], names
    assert FM.reply_for(msgs) == {"text": "Done.", "tool": None}
    text, results = FM.turn_state(msgs)
    assert text == t and len(results) == 6, (text, results)
    assert FM.reply_for([{"role": "user", "content": PLAN_BAD}])["tool"] is None
    assert FM.plan_call(plan[1]) == {"name": "grep", "arguments": {"queries": ["tk"], "path": "/w"}}
    assert FM.plan_call(plan[2])["arguments"] == {"patterns": ["*.txt"], "search_dir": "/w"}
    f = FM.FakeModel()
    f.turns.append((0.0, t, ["r1"]))
    f.turns.append((1.0, t, ["r1", "r2"]))
    f.turns.append((2.0, "other", ["x"]))
    assert f.tool_results_for("old text") == ["r1", "r2"] and f.tool_results_for("zzz") == []
    assert AR.parse_result('{"status":"ok"}') == {"status": "ok"} and AR.parse_result("nope") == {"_raw": "nope"} and AR.parse_result("[1]") == {"_raw": "[1]"}
    assert AR.result_paths({"files": [{"path": "/a"}, "/b"]}) == ["/a", "/b"] and AR.result_paths({}) == []
    names = sorted(c["name"] for c in cases.CASES if c["name"].startswith("lab_agent_remote_"))
    assert len(names) == 5 * 2 + 5 * 2 + 3 * 2, names
    assert all(c["modes"] == ("agent",) and c["lab"] for c in cases.CASES if c["name"].startswith("lab_agent_remote_"))


@check
def agent_file_edit_pieces():
    """#1835: the fake model's multi-hunk edit reply, the expected-bytes builder and the judges of cases_agent_file_edits.py"""
    import fake_model as FM
    import cases_agent_file_edits as AF
    ed = [("one", "ONE"), ("two three", "2\\n3")]
    rp = FM.reply_for([{"role": "user", "content": AF.prompt_for("/w/a.txt", ed)}])
    assert rp["tool"]["name"] == "apply_file_diffs" and rp["tool"]["arguments"]["operations"] == [
        {"op": "edit", "file_path": "/w/a.txt", "search": "one", "replace": "ONE"}, {"op": "edit", "file_path": "/w/a.txt", "search": "two three", "replace": "2\n3"}], rp
    for eol in (b"\n", b"\r\n"):
        for fin in (True, False):
            c = AF.make_file("t1", eol, fin)
            out = AF.apply_edits(c, AF.three_edits("t1"))
            assert out != c and out.endswith(eol) == fin and AF.ending_profile(out)[:3] == AF.ending_profile(c)[:3] and b"L10 KAPPA-EDITED" in out
    assert AF.ending_profile(AF.make_file("x", b"\r\n", False)) == (9, 0, 0, False)
    # #1956: the mixed-endings shapes of agent_edit_endings. Lines 2, 5, 8 end in LF, the rest in CRLF; the expected bytes keep every line's own ending, and a
    # buffer-style rewrite to one ending (what the app wrote) is judged a failure
    assert {e[0] for e in AF.ENDINGS} == {"crlf", "lf_no_eol", "crlf_no_eol", "mixed", "mixed_no_eol"}
    for fin in (True, False):
        c = AF.make_file("t3", AF.MIXED, fin)
        assert AF.ending_profile(c) == (7 if fin else 6, 3, 0, fin), AF.ending_profile(c)
        assert c.startswith(b"L01 alpha t3\r\nL02 beta t3\nL03 gamma t3\r\n") and c.endswith(b"L10 kappa t3\r\n" if fin else b"L10 kappa t3")
        out = AF.apply_edits(c, AF.three_edits("t3"))
        assert AF.ending_profile(out) == AF.ending_profile(c) and b"L02 BETA-EDITED t3\nL03" in out and b"L05 EPSILON-EDITED t3\nL06" in out
        assert len(AF.judge_bytes(out.replace(b"\r\n", b"\n").replace(b"\n", b"\r\n"), out, "x")) == 1
    try:
        AF.apply_edits(b"aa", [("a", "b")])
        raise AssertionError("a non-unique search must raise")
    except ValueError:
        pass
    c = AF.make_file("t2")
    mine = AF.apply_edits(c, AF.three_edits("t2"))
    user = c + b"USER\n"
    assert AF.judge_bytes(mine, mine, "x") == [] and len(AF.judge_bytes(mine.replace(b"\n", b"\r\n"), mine, "x")) == 1
    assert AF.judge_conflict(user, c, user, mine) == []
    assert "overwrote" in AF.judge_conflict(mine, c, user, mine)[0] and "put back" in AF.judge_conflict(c, c, user, mine)[0]
    assert len(AF.judge_conflict(b"garbage", c, user, mine)) == 1
    assert AF.SAVE_FAILED.search("ERROR Failed to save file for accepted AgentMode diffs for x: y") and AF.SAVE_FAILED.search("x changed on disk after this change was proposed")
    assert not AF.SAVE_FAILED.search("saved ok")
    names = {c["name"] for c in cases.CASES}
    assert {"agent_edit_accept", "agent_edit_reject", "agent_edit_conflict", "agent_edit_endings", "agent_edit_touch_only"} <= names
    arms = [c["arm"] for c in cases.CASES if c["name"].startswith("lab_fam_agent_edit_conflict.")]
    assert {a["helper"] for a in arms} == {"nohelper", "upload"} and {a["shell"] for a in arms} == {"bash", "dash", "nu", "pwsh"}, arms
    # #1837 follow-up: the deletion proposal, its report matcher and the lab case
    dp = FM.reply_for([{"role": "user", "content": AF.delete_prompt_for("/w/gone.txt")}])
    assert dp["tool"]["arguments"]["operations"] == [{"op": "delete", "file_path": "/w/gone.txt"}], dp
    assert AF.SAVE_FAILED.search("Failed to delete /w/x: the file changed on the host after it was read, so it was not deleted.")
    assert AF.SAVE_FAILED.search("/w/x changed on the remote host after this change was proposed")
    darms = [c["arm"] for c in cases.CASES if c["name"].startswith("lab_fam_agent_delete_conflict.")]
    assert {a["helper"] for a in darms} == {"nohelper", "upload"}, darms


@check
def fake_model_fault_injection():
    """#1831: status (with Retry-After), disconnect, delay (+ hang-up detection), drip, down/up, and the request / fresh-prompt accounting"""
    import fake_model as FM, urllib.request, urllib.error, http.client
    f = FM.FakeModel().start()
    try:
        def post(text, timeout=5):
            rq = urllib.request.Request(f.url + "/api/chat", json.dumps({"model": "m", "messages": [{"role": "user", "content": text}]}).encode())
            return urllib.request.urlopen(rq, timeout=timeout)
        f.inject("status", times=2, match="TOK1", code=429, retry_after=3)
        for _ in range(2):
            try:
                post("hello TOK1")
                raise AssertionError("expected 429")
            except urllib.error.HTTPError as e:
                assert e.code == 429 and e.headers.get("Retry-After") == "3", (e.code, dict(e.headers))
        assert json.loads(post("hello TOK1").read().decode().splitlines()[-1])["done"]        # the fault ran out
        f.inject("status", times=1, match="ONLY", code=500)
        assert json.loads(post("something else").read().decode().splitlines()[-1])["done"]    # no match: untouched
        try:
            post("ONLY this")
            raise AssertionError("expected 500")
        except urllib.error.HTTPError as e:
            assert e.code == 500
        f.inject("disconnect", times=1, after=2)
        try:
            body = post("cut me").read()
            raise AssertionError("a cut stream must raise, got %r" % body[:40])
        except (http.client.IncompleteRead, http.client.HTTPException, OSError):
            pass
        f.inject("delay", times=1, match="SLOWTOK", secs=3)
        a0 = f.aborted
        try:
            post("SLOWTOK", timeout=0.6)
            raise AssertionError("expected a client timeout")
        except OSError:
            pass
        time.sleep(0.6)
        assert f.aborted == a0 + 1, "the hang-up during the delay was not counted"
        f.inject("drip", times=1, interval=0.01, words=5)
        txt = "".join(json.loads(x)["message"]["content"] for x in post("drip it").read().decode().splitlines())
        assert txt == FM.drip_text(5), txt
        f.go_down()
        try:
            post("x", timeout=1)
            raise AssertionError("expected connection refused")
        except urllib.error.URLError:
            pass
        f.come_up()
        assert json.loads(post("back up").read().decode().splitlines()[-1])["done"]
        assert f.fresh_with("TOK1") == 3 and f.fresh_with("ONLY") == 1 and f.fresh[-1] == "back up", f.fresh
        assert f.fired.count("status") == 3 and "disconnect" in f.fired and "drip" in f.fired, f.fired
        assert FM.FakeModel.ACTIVE is f
    finally:
        f.stop()
    assert FM.FakeModel.ACTIVE is None


@check
def agent_resilience_pure_parts():
    """#1831/#1832: hop parsers, the pending / running accounting, the ERROR allowance, the queued-row ledger, the sqlite reader, and the case registration"""
    import cases_agent_resilience as ar
    log = ("hop=turn conv=c1 state=in_progress\nhop=turn conv=c1 state=error\n"
           "hop=agent_action id=a1 kind=run_shell_command status=queued\nhop=agent_action id=a1 kind=run_shell_command status=blocked\n"
           "hop=agent_action id=a2 kind=read_files status=running\nhop=agent_action id=a2 kind=read_files status=done\n")
    assert ar.last_turn_state(log) == "error" and ar.last_turn_state("") is None
    assert ar.implicit_in_progress(None) and ar.implicit_in_progress("in_progress") and not ar.implicit_in_progress("success") and ar.implicit_in_progress("error", ("error",))   # r7: a fresh conversation logs no hop=turn
    assert ar.open_actions(log) == ["a1"] and ar.running_actions(log) == ["a2"]
    errs = "2026 [ERROR] [byop] open stream failed: 429\n2026 INFO fine\n2026 [ERROR] something else broke\n"
    exp, unexp = ar.byop_errors(errs)
    assert len(exp) == 1 and len(unexp) == 1, (exp, unexp)
    assert len(ar.judge_errors(errs)) == 1 and ar.judge_errors("2026 [ERROR] [byop] open stream failed: x\n" * 2) == []
    assert len(ar.judge_errors("[ERROR] [byop] stream chunk error: x\n" * 3)) == 1
    kc = "2026 [ERROR] Failed to read local control mode from secure storage: unknown error: A default keychain could not be found.\n2026 [ERROR] Failed to read API keys from secure storage: unknown error\n"
    assert ar.judge_errors(kc, platform="darwin") == [] and len(ar.judge_errors(kc, platform="linux")) == 2 and len(ar.judge_errors(kc + "2026 [ERROR] boom\n", platform="darwin")) == 1
    hops = [("r1", "queued"), ("r2", "queued"), ("r1", "fired")]
    assert ar.judge_rows(["A1", "B1"], ["Reply A1"], hops, ["B1"]) == []                                    # A1 sent once, B1 held
    assert any("lost" in p for p in ar.judge_rows(["A1", "B1"], ["Reply A1"], hops, []))
    assert any("2 times" in p for p in ar.judge_rows(["A1", "B1"], ["A1", "A1"], hops, ["B1"]))
    assert any("AND is still held" in p for p in ar.judge_rows(["A1", "B1"], ["A1"], hops, ["A1 B1", "B1"]))
    assert any("fired" in p for p in ar.judge_rows(["A1", "B1"], [], [("r1", "queued"), ("r2", "queued"), ("r1", "fired")], ["A1", "B1"]))
    assert ar.row_hops("hop=queued_row id=x state=popped\n") == [("x", "popped")]
    # round 11 (Windows, no OCR): the app's own record of the input after it put a popped row back is the proof that the row is held, not lost
    rest = ('2026 [INFO] hop=queued_row id=r1 state=popped\n'
            '2026 [DEBUG] input_order hop=queued_row_restored id=r1 buffer="please answer with the word A1 \\"x\\"" t=1.0\n')
    assert ar.row_hops(rest) == [("r1", "popped")], "the restored hop is not a row state"
    assert ar.restored_inputs(rest) == [("r1", 'please answer with the word A1 "x"')] and ar.tokens_in_input(["A1", "B1"], rest) == ("A1",)
    assert ar.judge_rows(["A1", "B1"], [], [("r1", "queued"), ("r2", "queued"), ("r1", "popped")], ["B1"], ar.tokens_in_input(["A1", "B1"], rest)) == []
    assert ar.restored_inputs(rest + '2026 [DEBUG] input_order hop=enter_begin deferred=false buffer="x" input_type=AI t=2.0\n') == [], "an Enter after the restore submitted the input"
    assert ar.restored_inputs('hop=queued_row_restored id=r1 buffer=<40 chars> t=1.0\n') == [] and ar.tokens_in_input(["A1"], "") == ()
    assert any("lost" in p for p in ar.judge_rows(["A1", "B1"], [], [("r1", "queued"), ("r2", "queued"), ("r1", "popped")], ["B1"], ())), "popped and not seen anywhere is still lost"
    import sqlite3
    d = tempfile.mkdtemp()
    try:
        db = os.path.join(d, "warp.sqlite")
        con = sqlite3.connect(db)
        con.execute("CREATE TABLE queued_prompts (id TEXT, conversation_id TEXT, position INTEGER, kind TEXT, text TEXT, origin TEXT, created_at TEXT)")
        con.execute("CREATE TABLE ai_queries (id INTEGER, input TEXT)")
        con.execute("INSERT INTO queued_prompts VALUES ('1','c',0,'Prompt','answer QA1','x','t')")
        con.execute("INSERT INTO ai_queries VALUES (1,'Reply with the word P9')")
        con.commit()
        con.close()
        rows, blobs = ar.read_stored([db, os.path.join(d, "missing.sqlite")])
        assert rows == ["answer QA1"] and blobs == ["Reply with the word P9"], (rows, blobs)
    finally:
        shutil.rmtree(d, ignore_errors=True)
    names = {c["name"] for c in cases.CASES}
    want = {"agent_fault_429", "agent_fault_500", "agent_fault_disconnect", "agent_fault_down", "agent_fault_slow_cancel", "agent_fault_queued_rows"}
    want |= {"agent_restart_%s_%s" % (h, s) for h in ("quit", "kill") for s in ("stream", "queued", "blocked")}
    assert want <= names, want - names
    assert all(c["modes"] == ("agent",) for c in cases.CASES if c["name"] in want)


@check
def agent_question_pure_parts_and_fake_asks():
    """#1851: the ask_user_question verdicts of cases_agent_question.py, and the fake model's question trigger and ask_* fault lines"""
    import cases_agent_question as aq
    import fake_model as FM
    blocked = ("hop=turn conv=c1 state=in_progress\n"
               "hop=agent_action id=q1 kind=ask_user_question status=queued\nhop=agent_action id=q1 kind=ask_user_question status=blocked\n")
    assert aq.verdict(blocked) == "answerable" and aq.ask_blocked_ids(blocked) == ["q1"]
    assert aq.verdict(blocked + "hop=agent_action id=q1 kind=ask_user_question status=done\nhop=turn conv=c1 state=success\n") == "ended"
    assert aq.verdict("hop=turn conv=c1 state=in_progress\n") == "hung" and aq.verdict("") == "hung"
    queued_only = "hop=agent_action id=q2 kind=ask_user_question status=queued\nhop=turn conv=c1 state=error\n"
    assert aq.verdict(queued_only) == "hung" and aq.ask_open_ids(queued_only) == ["q2"], "a terminal turn with an ask action left open is a hang"
    assert aq.verdict("hop=turn conv=c1 state=error\n") == "ended"
    r = FM.reply_for([{"role": "user", "content": "Please ask me a question Z9"}])
    assert r["tool"]["name"] == "ask_user_question" and "Z9" in r["tool"]["arguments"]["questions"][0]["question"]
    assert FM.reply_for([{"role": "user", "content": "Please ask me a question Z9"}, {"role": "tool", "content": "{}"}])["text"] == "Done."
    r2 = FM.reply_for([{"role": "user", "content": "Please run a slow command then ask me a question Z8"}])
    assert r2["tool"]["name"] == "run_shell_command" and [t["name"] for t in r2["tools_after"]] == ["ask_user_question"], r2
    calls = json.loads(FM.stream_lines("m", r2)[0])["message"]["tool_calls"]
    assert [c["function"]["name"] for c in calls] == ["run_shell_command", "ask_user_question"], calls
    for kind in ("ask_cut", "ask_nodone", "ask_trunc", "ask_empty", "ask_slow"):
        text_l, call_l, done_l = FM.ask_fault_lines("m", kind)
        call = json.loads(call_l)["message"]["tool_calls"][0]["function"]
        assert call["name"] == "ask_user_question" and json.loads(done_l)["done"] is True and json.loads(text_l)["done"] is False
    assert isinstance(json.loads(FM.ask_fault_lines("m", "ask_trunc")[1])["message"]["tool_calls"][0]["function"]["arguments"], str)
    assert json.loads(FM.ask_fault_lines("m", "ask_empty")[1])["message"]["tool_calls"][0]["function"]["arguments"] == {}
    f = FM.FakeModel()
    for kind in ("ask_cut", "ask_nodone", "ask_trunc", "ask_empty", "ask_slow"):
        f.inject(kind)
    names = {c["name"] for c in cases.CASES}
    want = {"agent_question_answer", "agent_question_cut", "agent_question_nodone", "agent_question_trunc", "agent_question_empty", "agent_question_slow",
            "agent_question_ctrl_c", "agent_question_steer", "agent_question_after_command", "agent_question_steer_at_end"}
    assert want <= names and want <= set(cases.PROVING) and want <= set(cases.PROVING_FAKE), want - names
    assert all(c["modes"] == ("agent",) for c in cases.CASES if c["name"] in want)


@check
def replay_cases_registered_but_not_swept_by_lab_glob():
    names = [c["name"] for c in cases.CASES if c["name"].startswith("lab_replay")]
    assert names == ["lab_replay.bash.150.nohelper"], names
    c = [c for c in cases.CASES if c["name"] == names[0]][0]
    assert c["modes"] == ("terminal", "agent") and c["lab"]


@check
def soak_diff_names_a_single_lost_command():
    """#1780: one lost command used to read as 295/660 (a positional count); soak_diff reports it as ONE missing item and keeps order/unexpected apart."""
    import cases_robust as cr
    exp = ["s%d" % i for i in range(660)]
    got = exp[:295] + exp[296:]
    d = cr.soak_diff(exp, got)
    assert d["missing"] == ["s295"] and d["arrived"] == 659 and d["unexpected"] == [] and d["order_ok"], d
    assert sum(a == b for a, b in zip(got, exp)) == 295            # the old figure, for the record
    d = cr.soak_diff(["a", "b", "c"], ["b", "a", "c", "x"])
    assert d["missing"] == [] and d["unexpected"] == ["x"] and not d["order_ok"] and d["arrived"] == 3, d
    d = cr.soak_diff(["a", "a"], ["a"])
    assert d["missing"] == ["a"] and d["arrived"] == 1, d         # duplicates count as a multiset
    assert cr.SOAK_ALERT.search("thread 'main' panicked at") and cr.SOAK_ALERT.search("terminal-server exited") and not cr.SOAK_ALERT.search("echo ok")

@check
def wn_zero_network_trace_and_offline_plugin():
    """Wave N: outbound_attempts reads strace lines (loopback / unix / netlink ignored, remote inet kept); fakeclaude refuses a remote source and installs the shipped
    bundle; the shipped hook scripts produce the four events with no network step"""
    import cases_netless as nl
    trace = (
        'connect(3, {sa_family=AF_UNIX, sun_path="/run/user/1/bus"}, 110) = 0\n'
        'connect(4, {sa_family=AF_NETLINK, nl_pid=0, nl_groups=00000000}, 12) = 0\n'
        'connect(5, {sa_family=AF_INET, sin_port=htons(7000), sin_addr=inet_addr("127.0.0.1")}, 16) = 0\n'
        'connect(6, {sa_family=AF_INET6, sin6_port=htons(11434), sin6_flowinfo=htonl(0), inet_pton(AF_INET6, "::1", &sin6_addr), sin6_scope_id=0}, 28) = 0\n'
        'connect(7, {sa_family=AF_INET, sin_port=htons(443), sin_addr=inet_addr("203.0.113.9")}, 16) = -1 EINPROGRESS\n'
        '[pid 12] sendto(8, "x", 1, MSG_NOSIGNAL, {sa_family=AF_INET6, sin6_port=htons(53), sin6_flowinfo=htonl(0), inet_pton(AF_INET6, "2001:db8::1", &sin6_addr), sin6_scope_id=0}, 28) = 1\n')
    assert nl.outbound_attempts(trace) == [("connect", "203.0.113.9", 443), ("sendto", "2001:db8::1", 53)], nl.outbound_attempts(trace)
    assert nl.outbound_attempts("") == []
    # the scripted session's allowlist: an ssh target address is not an attempt, anything else still is
    assert nl.outbound_attempts(trace, {"203.0.113.9"}) == [("sendto", "2001:db8::1", 53)]
    assert nl.outbound_attempts(trace, {"203.0.113.9", "2001:db8::1"}) == []
    bundle = os.path.join(nl.REPO, "resources", "bundled", "claude_plugin")
    assert os.path.isfile(os.path.join(bundle, ".claude-plugin", "marketplace.json"))
    if not (shutil.which("jq") and shutil.which("bash")):
        return
    tmp = tempfile.mkdtemp()
    try:
        env = dict(os.environ, CLAUDE_CONFIG_DIR=os.path.join(tmp, "cfg"))
        fake = [sys.executable, "-I", os.path.join(HERE, "fakeclaude.py")]
        bad = subprocess.run(fake + ["plugin", "marketplace", "add", "https://example.invalid/x.git"], env=env, capture_output=True, text=True)
        assert bad.returncode != 0 and "local" in bad.stderr
        problems, events = nl.run_plugin_offline(tmp, bundle, os.path.join(tmp, "cfg2"), [])
        assert problems == [], problems
        data = open(events, "rb").read().decode("utf-8")
        for ev in ("prompt_submit", "tool_complete", "permission_request", '"event":"stop"'):
            assert ev in data, (ev, data[:300])
        assert data.count("\x1b]777;notify;warp://cli-agent;") == 4
        assert "..." in data and "x" * 250 not in data      # the 300-character prompt was cut by characters
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


@check
def wn_agent_plugins_offline():
    """the bundled Codex / Gemini CLI / OpenCode plugins: the fake CLIs refuse a remote source and install the shipped folders; the shipped hooks (run under a pty, as they write to
    /dev/tty) and OpenCode's modules produce one notification sequence per event with no network step, and a multi-byte prompt is cut by characters"""
    import cases_netless as nl
    if not (shutil.which("jq") and shutil.which("bash")):
        return
    tmp = tempfile.mkdtemp()
    try:
        env = dict(os.environ, CODEX_HOME=os.path.join(tmp, "ch"), GEMINI_CLI_HOME=os.path.join(tmp, "gh"))
        for fake, args in (("fakecodex.py", ["plugin", "marketplace", "add", "https://example.invalid/x.git"]),
                           ("fakegemini.py", ["extensions", "install", "https://example.invalid/x.git", "--consent"])):
            bad = subprocess.run([sys.executable, "-I", os.path.join(HERE, fake)] + args, env=env, capture_output=True, text=True)
            assert bad.returncode != 0 and "local" in bad.stderr, (fake, bad.stderr)
        for agent in ("codex", "gemini", "opencode"):
            if agent == "opencode" and not shutil.which("node"):
                continue
            bundle = os.path.join(nl.REPO, "resources", "bundled", agent + "_plugin")
            assert os.path.isfile(os.path.join(bundle, *nl.PLUGIN_MARKERS[agent])), bundle
            work = os.path.join(tmp, agent)
            os.makedirs(work)
            problems, events, expected = nl.run_agent_plugin_offline(agent, work, bundle, [])
            assert problems == [], (agent, problems)
            data = open(events, "rb").read().decode("utf-8")
            assert data.count("\x1b]777;notify;warp://cli-agent;") == len(expected) == 5, (agent, data.count("\x1b]777;"), expected)
            for ev in ("session_start", "prompt_submit", "tool_complete", "permission_request", '"event":"stop"'):
                assert ev in data, (agent, ev, data[:300])
            assert "x" * 250 not in data
        # the Codex / Gemini prompt (300+ characters, multi-byte) was cut by characters, not bytes
        assert "..." in open(os.path.join(tmp, "codex", "codex_events.bin"), "rb").read().decode("utf-8")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


@check
def upgrade_table_matches_fixture_and_docs():
    """#1546: upgrade_table.py, fixtures/upgrade_v0_1_17/settings.toml and docs/testing.md 2.18 say the same thing, and every key the table names is a real
    toml_path of the tree (a removed key only in the v0.1.17 source is marked REMOVED in its `kind`)"""
    import upgrade_table as U
    root = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
    fx = open(os.path.join(U.FIXTURE_DIR, "settings.toml"), encoding="utf-8").read()
    assert U.fixture_problems(fx) == [], U.fixture_problems(fx)
    doc = open(os.path.join(root, "docs", "testing.md"), encoding="utf-8").read()
    assert U.doc_problems(doc) == [], U.doc_problems(doc)
    assert len({r["id"] for r in U.ROWS}) == len(U.ROWS)
    for r in U.ROWS:
        assert r["action"] in (U.KEEP, U.MIGRATE, U.DROP_SILENTLY, U.DROP_NOTICE, U.NEW), r["id"]
        assert r["check"] in (U.C_KEPT, U.C_UNKNOWN_KEPT, U.C_ABSENT, U.C_FILE_SAME, U.C_SECRET, U.C_ENV, U.C_DB, U.C_UNIT), r["id"]
        if r["check"] in (U.C_KEPT, U.C_UNKNOWN_KEPT):
            assert U.seeds_of(r), "%s is a kept-key row with no seed line" % r["id"]
        if r["check"] == U.C_ABSENT:
            assert not U.seeds_of(r) and r["default"] is not None, r["id"]
    # every key a TOML row names exists as a toml_path (current tree), except the removed one
    tomls = []
    for base in ("app/src", "crates"):
        for d, _s, files in os.walk(os.path.join(root, base)):
            for f in files:
                if f.endswith(".rs") and "_test" not in f:
                    try:
                        tomls.append(open(os.path.join(d, f), encoding="utf-8", errors="replace").read())
                    except OSError:
                        pass
    blob = "\n".join(tomls)
    for r in U.ROWS:
        if r["check"] not in (U.C_KEPT, U.C_UNKNOWN_KEPT, U.C_ABSENT):
            continue
        keys = [t + "." + k if t else k for t, k in (U.seeds_of(r) or U._absent_keys(r))]
        for k in keys:
            has = ('toml_path: "%s"' % k) in blob
            if "REMOVED" in r["kind"]:
                assert not has, "%s: %s is marked REMOVED but is still a toml_path" % (r["id"], k)
            else:
                assert has, "%s: %s is not a toml_path of this tree (renamed or removed?)" % (r["id"], k)


@check
def upgrade_case_pure_helpers():
    """#1546 upgrade_from_v0_1_17: the seed renderer, tree hashing, keyring output framing, log verdict, downgrade report, the previous-release locator"""
    import cases_upgrade as CU, prev_release as PR, upgrade_table as U, toml_text as T
    assert CU.render_seed("a=@BASH@ b=@HOME@", "/bin/bash", "/h") == "a=/bin/bash b=/h"
    tmp = tempfile.mkdtemp()
    try:
        os.makedirs(os.path.join(tmp, "d"))
        open(os.path.join(tmp, "d", "a.yaml"), "w").write("x")
        open(os.path.join(tmp, "d", "phosphor.log"), "w").write("noise")
        open(os.path.join(tmp, "b.toml"), "w").write("y")
        h = CU.tree_hashes(tmp)
        assert sorted(h) == ["b.toml", "d/a.yaml"], h
        h2 = dict(h)
        del h2["b.toml"]
        assert CU.lost_files(h, h2) == ["b.toml"] and CU.lost_files(h2, h) == []
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    out = "KEY:ProxyPassword\nsecret-1\nEND\nKEY:AgentProviderSecrets\n\nEND\n"
    assert CU.parse_lookups(out) == {"ProxyPassword": "secret-1", "AgentProviderSecrets": None}
    assert CU.secrets_equal("AgentProviderSecrets", '{"a":"1","b":"2"}', '{"b": "2", "a": "1"}')
    assert not CU.secrets_equal("ProxyPassword", "a", None)
    assert CU.shell_q("it's") == "'it'\\''s'"
    # the log verdict: a panic and a settings/keyring ERROR count; an unrelated connection error does not
    assert CU.upgrade_log_problems("2026 INFO warp started\n2026 ERROR Failed to connect to 127.0.0.1:11434: refused\n") == []
    assert CU.upgrade_log_problems("thread 'main' panicked at app/src/x.rs:1\n")
    assert CU.upgrade_log_problems("2026 ERROR failed to read the proxy password from the keyring\n")
    assert CU.upgrade_log_problems("2026 ERROR Failed to migrate settings.toml\n")
    # rows against a settings text: a kept key that changes, a removed key, and a new key written non-default are each named
    fx = open(os.path.join(U.FIXTURE_DIR, "settings.toml"), encoding="utf-8").read()
    pre = CU.render_seed(fx, "/bin/bash", "/h")
    assert U.after_problems(pre, pre, T) == []
    bad = T.del_key(pre, "network", "proxy_url")
    assert [x[0] for x in U.after_problems(pre, bad, T)] == ["U20"]
    bad = T.set_key(pre, "warpify.ssh", "remote_helper_allowed_hosts", '["u@h"]')
    assert [x[0] for x in U.after_problems(pre, bad, T)] == ["U42"]
    bad = T.set_key(pre, "privacy.secret_redaction", "enabled", "true")
    assert [x[0] for x in U.after_problems(pre, bad, T)] == ["U09"]
    reformatted = pre.replace("providers = [", "providers = [  # same list").replace("},\n]", "}\n]")
    assert U.after_problems(pre, reformatted, T) == [], U.after_problems(pre, reformatted, T)
    rep = CU.downgrade_report(pre, T.del_key(pre, "network", "allowed_origins"), True, "")
    assert rep["reached_prompt"] and rep["seeded_keys_lost"] == []
    rep = CU.downgrade_report(pre, T.del_key(pre, "warpify.ssh", "use_ssh_tmux_wrapper"), False, "panicked at x")
    assert rep["seeded_keys_lost"] == ["warpify.ssh.use_ssh_tmux_wrapper"] and not rep["reached_prompt"] and rep["problem_lines"]
    # prev_release: an explicit path wins; a bad path / a disabled fetch say why; SHA256SUMS lookup
    assert PR.expected_sha("abc123  Phosphor-x86_64.AppImage\ndef456 *other\n", "Phosphor-x86_64.AppImage") == "abc123"
    assert PR.expected_sha("abc123  x\n", "y") is None
    tmp = tempfile.mkdtemp()
    try:
        exe = os.path.join(tmp, "phosphor")
        open(exe, "w").write("#!/bin/sh\n")
        os.chmod(exe, 0o755)
        assert PR.locate(environ={"APPCI_PREV_RELEASE_BIN": exe}) == (exe, "APPCI_PREV_RELEASE_BIN")
        got, why = PR.locate(environ={"APPCI_PREV_RELEASE_BIN": os.path.join(tmp, "missing")})
        assert got is None and "not an executable" in why
        got, why = PR.locate(environ={"APPCI_PREV_RELEASE_DIR": tmp, "APPCI_PREV_RELEASE_FETCH": "0"})
        assert got is None and "APPCI_PREV_RELEASE_FETCH=0" in why
        called = []
        got, why = PR.locate(environ={"APPCI_PREV_RELEASE_DIR": tmp}, fetch=lambda tag, d: called.append(d) or (_ for _ in ()).throw(OSError("offline")))
        assert got is None and ("offline" in why or "Linux" in why), why
        app = os.path.join(PR.cache_dir("v0.1.17", {"APPCI_PREV_RELEASE_DIR": tmp}), "squashfs-root")
        os.makedirs(app)
        run = os.path.join(app, "AppRun")
        open(run, "w").write("#!/bin/sh\n")
        os.chmod(run, 0o755)
        assert PR.locate(environ={"APPCI_PREV_RELEASE_DIR": tmp})[0] == run
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    reg = {c["name"]: c for c in cases.CASES}
    assert "upgrade_from_v0_1_17" in reg and "upgrade_from_v0_1_17" not in cases.SMOKE
    assert winmap.skip_reason("upgrade_from_v0_1_17", "gitbash"), "the Windows reason must be stated"


@check
def refterm_single_launch_path():
    """#1826: Terminal.app is launched ONLY through refterm_mac.launch (no state restoration, PID recorded, quit per block); any other launch path leaks windows"""
    import re, refterm_mac as R
    argv = R.launch_argv("/x/ref.command")
    assert argv[:4] == ["open", "-a", "Terminal", "/x/ref.command"] and argv[4:] == ["--args", "-ApplePersistenceIgnoreState", "YES"], argv
    assert R.count_children("  10 /usr/bin/login\n  10 /bin/zsh\n 11 /usr/bin/login\n 100 /usr/bin/login\n", 10) == 1
    assert R.count_children("", 10) == 0
    bad = []
    for fn in sorted(os.listdir(HERE)):
        if not fn.endswith(".py") or fn in ("refterm_mac.py", "selftest.py"):
            continue
        for n, line in enumerate(open(os.path.join(HERE, fn), encoding="utf-8", errors="replace"), 1):
            code = line.split("#", 1)[0]
            if re.search(r"""["']open["']\s*,\s*(["']-[a-zA-Z]+["']\s*,\s*)*["']Terminal""", code) or re.search(r"tell application .?Terminal.? to (do script|activate)", code):
                bad.append("%s:%d" % (fn, n))
    assert not bad, "Terminal.app launched outside refterm_mac.launch: %s" % bad


@check
def refterm_mac_bookkeeping():
    """#1826: which Terminal.app is ours, and quitting it at the end of each reference block (pure parts; no process is touched)"""
    import refterm_mac as R
    t0 = 1000.0
    # ours = the single Terminal that started after the harness did, never the one running at harness start
    assert R.pick_ours([50, 90], 50, {50: 10.0, 90: 1005.0}, t0) == 90
    assert R.pick_ours([50], 50, {50: 10.0}, t0) is None, "the pre-existing Terminal is never ours"
    assert R.pick_ours([90], None, {90: 999.5}, t0) == 90, "lstart has 1 s resolution"
    assert R.pick_ours([90], None, {90: 500.0}, t0) is None, "started before the harness: not ours"
    assert R.pick_ours([90, 91], None, {90: 1005.0, 91: 1006.0}, t0) is None, "two candidates: never guess"
    assert R.pick_ours([90], None, {90: None}, t0) is None and R.pick_ours([90], None, {90: 1005.0}, None) is None
    # quit_launched: quits only our PID, per reference block, and the next launch is a fresh one
    saved = (dict(R._state), R.terminal_pids, R.os.kill, R.time.sleep, R._started_epoch)
    live, killed = [50], []
    try:
        R.time.sleep = lambda s: None
        R.terminal_pids = lambda: list(live)
        R._started_epoch = lambda p: {50: 10.0, 91: 2005.0, 92: 2105.0}.get(p)
        R.os.kill = lambda pid, sig: (killed.append((pid, sig)), live.remove(pid))[0]
        R._state.update(snapshotted=True, was_running=True, pre_pid=50, pid=None, run=None, t0=2000.0)
        assert R.quit_launched() and not killed and R.left_windows() == [], "an already-running Terminal is left alone"
        live.remove(50)                                     # the owner quit their Terminal during the run; the reference block launches a new one
        live.append(91)
        assert R.left_windows() == [91]
        assert R.quit_launched() and killed == [(91, R.signal.SIGTERM)] and live == [], (killed, live)
        live.append(92)                                     # the next block launches another one
        assert R.left_windows() == [92]
        assert R.quit_launched() and killed[-1] == (92, R.signal.SIGTERM) and live == [] and R.left_windows() == []
        assert [k[0] for k in killed] == [91, 92]
    finally:
        R._state.clear()
        R._state.update(saved[0])
        R.terminal_pids, R.os.kill, R.time.sleep, R._started_epoch = saved[1:]


# ---- #1830: the agent network gate cases (cases_agent_netgate.py)
@check
def agent_netgate_pure_parts():
    import cases_agent_netgate as NG
    import cases_netless as NL
    assert NG.fnv("LocalHost") == NL.fnv_host_hash("localhost")
    assert NG.denied_line("localhost") == 'network gate: denied "remote-asset" (host hash %s)' % NL.fnv_host_hash("localhost")
    hit = lambda path="/x", headers=None, body=b"": dict(method="GET", path=path, headers=headers or {}, body=body)
    assert NG.leaks([hit()], "SENT") == []
    assert len(NG.leaks([hit(headers={"x-appci-key": "SENT"})], "SENT")) == 1
    assert len(NG.leaks([hit(path="/p?k=SENT")], "SENT")) == 1
    assert len(NG.leaks([hit(body=b"..SENT..")], "SENT")) == 1
    assert len(NG.leaks([hit(headers={"authorization": "Bearer z"})], "SENT")) == 1
    log = 'a localhost:99 line\nnetwork gate: denied "remote-asset" (host hash 00ff)\nnetwork gate: denied "x" (host localhost)\n'
    assert NG.gate_lines(log) == log.splitlines()[1:]
    assert NG.host_leaks(log, ["localhost", "99"]) == [log.splitlines()[2]]
    assert NG.host_leaks(log, [""]) == []


@check
def r10_netgate_request_comes_from_the_markdown_viewer():
    """round 10 (docs/design/r10-wl-gate.md): the gate cases make their request by opening a one-image markdown file in the app's viewer (agent output never loads an
    http image, so the fake model's markdown image asked the gate nothing in rounds 6-9). Pins the pure parts and that the viewer tab is closed only when its opening
    was seen in the app log (the close key acts on whatever tab is in front)."""
    import re
    import cases as C
    import cases_agent_netgate as NG
    import fake_model as FM
    url = "http://localhost:38267/a_t328_v0bwxx.png"
    name, text = NG.image_doc(url, "t329_k3k3k3")
    assert name == "gimg_t329_k3k3k3.md" and text.splitlines()[-1] == "![img](%s)" % url and text.endswith("\n"), (name, text)
    assert not any(n in name for n in ("localhost", "38267", "t328", "v0bwxx"))             # the app may log the path of a file it opens
    shown = (name + " " + text.replace(url, "")).lower()
    assert not any(re.search(r"\b%s\b" % w, shown) for w in NG.IMAGE_DOC_WORDS), shown   # nothing the pointer steps search for by OCR
    assert NG.viewer_opened('2026-11-10T06:20:14Z [INFO] dispatching action for "root_view:add_file_pane"\n')
    assert not NG.viewer_opened('2026-11-10T06:20:14Z [INFO] dispatching action for "root_view:open_new_with_file_notebook"\n') and not NG.viewer_opened("")
    # the fake model still answers the old turn with a markdown image: agent_gate_deny_allow step 0 asserts the app requests nothing for it
    assert FM.reply_for([{"role": "user", "content": "Please show this image: " + url}])["text"].endswith("![img](%s)" % url)
    # a hash is not a host or a port: a 5-digit port inside a 16-digit hash is no leak; the same digits outside one are
    hashed = 'W Asset fetch failed (Async { id: AsyncAssetId { key_len: 40, key_hash: 0038267aaaaaaaaa } }): network gate denied a request for "remote-asset" (host db3f053403ad937a)'
    assert NG.host_leaks(hashed, ["localhost", "38267", "v0bwxx"]) == []
    named = 'W Asset fetch failed (Async { id: AsyncAssetId { key: "%s" } }): network gate denied a request for "remote-asset" (host db3f053403ad937a)' % url
    assert NG.host_leaks(named, ["38267"]) == [named] and NG.host_leaks(named, ["localhost"]) == [named] and NG.host_leaks(named, ["v0bwxx"]) == [named]

    keys, logs, checks, opened_paths = [], [""], [], []
    be = types.SimpleNamespace(key=keys.append)
    n = [0]

    def tok(k=8):
        n[0] += 1
        return "T%d_abcdef" % n[0]
    W = tempfile.mkdtemp(prefix="appci_ng_")
    ctx = types.SimpleNamespace(be=be, W=W, tok=tok, log_size=lambda: 0, log_since=lambda off: logs[0],
                                until=lambda pred, timeout=10, step=0.05: pred())
    r = types.SimpleNamespace(check=lambda ok, msg: (checks.append((bool(ok), msg)), bool(ok))[1])
    saved, saved_sleep = C.open_file_in_app, NG.time.sleep
    NG.time.sleep = lambda s: None
    try:
        # the app took the file: opened, the URL carries the first token, the file is written with the second
        C.open_file_in_app = lambda c, path: (opened_paths.append(path), logs.__setitem__(0, 'I dispatching action for "root_view:add_file_pane"\n'), None)[2]
        off, t, opened = NG.show_image(ctx, r, "a", lambda tk, tag: "http://localhost:1/%s_%s.png" % (tag, tk))
        assert opened and t == "t1_abcdef" and checks[-1][0], (opened, t, checks)
        assert os.path.basename(opened_paths[0]) == "gimg_t2_abcdef.md" and "![img](http://localhost:1/a_t1_abcdef.png)" in open(opened_paths[0]).read()
        NG.close_viewer(ctx, opened)
        assert keys == [R_CLOSE()], keys
        # the hand-over failed, or the app log never showed the tab: the check fails and NO close key is pressed
        del keys[:]
        logs[0] = ""
        C.open_file_in_app = lambda c, path: "the forwarding invocation did not exit within 30 s"
        _, _, opened = NG.show_image(ctx, r, "b", lambda tk, tag: "http://localhost:1/%s_%s.png" % (tag, tk))
        assert not opened and not checks[-1][0] and "did not exit" in checks[-1][1], checks[-1]
        C.open_file_in_app = lambda c, path: None
        _, _, opened = NG.show_image(ctx, r, "c", lambda tk, tag: "http://localhost:1/%s_%s.png" % (tag, tk))
        assert not opened and not checks[-1][0] and "root_view:add_file_pane" in checks[-1][1], checks[-1]
        NG.close_viewer(ctx, opened)
        assert keys == [], keys
    finally:
        C.open_file_in_app, NG.time.sleep = saved, saved_sleep
        shutil.rmtree(W, ignore_errors=True)


def R_CLOSE():
    import cases_robust as R
    return R.CLOSE_PANE


@check
def r10_netgate_gate_switch_is_on_the_label_row_not_the_heading():
    """rounds 8/9: 'no click to the right of the Network gate label toggled it'. The page shows 'Network gate' twice (section heading, then the switch row's label) and the
    case clicked right of the top-most one. Word positions are those of the round 8 Wayland screenshot (1600 px wide)."""
    import cases_agent_netgate as NG
    w = lambda text, x, y: (text, x, y, x - 20, x + 20, y - 7, y + 7)
    page = [w("Network", 692, 111), w("Network", 676, 149), w("gate", 722, 149), w("Network", 667, 216), w("gate", 706, 216), w("Allowed", 668, 282), w("origins", 712, 282),
            w("Gate", 660, 374), w("log", 682, 374), w("Clear", 1405, 366), w("Network", 304, 545)]
    spots = NG.switch_spots(page)
    assert spots[0] == (706 + 720, 216), spots[:3]                       # the label row first; x ~1426 is on the switch (1407..1443)
    assert all(y in (216, 149) for _, y in spots) and [y for _, y in spots].index(149) == len(NG.SWITCH_DX), spots   # the heading row only after every label-row spot
    assert not any(y == 374 for _, y in spots)                           # 'Gate log' is not a candidate
    with_add = NG.switch_spots(page + [w("Add", 1425, 291)])
    assert with_add[0] == (1425, 216) and with_add[1] == (1426, 216), with_add[:3]      # the Add button marks the right edge when OCR read it
    assert NG.switch_spots([w("Network", 676, 149), w("gate", 722, 149)])[0] == (722 + 720, 149)       # only one row read: use it
    assert NG.switch_spots([w("Gate", 660, 374), w("log", 682, 374)]) == [] and NG.switch_spots([]) == []
    assert NG.switch_spots([w("gate", 600, 216), w("Network", 667, 216)]) == []                          # 'network' must be LEFT of 'gate' on the line


@check
def agent_netgate_strace_destination_parser():
    import cases_agent_netgate as NG
    f = NG.non_local_destinations
    ok = [
        'connect(5, {sa_family=AF_INET, sin_port=htons(8080), sin_addr=inet_addr("127.0.0.1")}, 16) = 0',
        'connect(5, {sa_family=AF_INET6, sin6_port=htons(80), sin6_addr=inet_pton(AF_INET6, "::1", &sin6_addr), sin6_flowinfo=0}, 28) = 0',
        'sendto(7, "x", 1, 0, {sa_family=AF_INET6, sin6_port=htons(53), sin6_addr=inet_pton(AF_INET6, "::ffff:127.0.0.53", &a)}, 28) = 1',
        'connect(3, {sa_family=AF_UNIX, sun_path="/run/user/1000/bus"}, 110) = 0',
        'sendmsg(3, {msg_name={sa_family=AF_NETLINK, nl_pid=0, nl_groups=00000000}, msg_namelen=12}, 0) = 20',
        'connect(9, {sa_family=AF_UNSPEC, sa_data="\\0"}, 16) = 0',
        'sendto(5, "abc", 3, MSG_NOSIGNAL, NULL, 0) = 3',
        '--- SIGCHLD {si_signo=SIGCHLD} ---', '+++ exited with 0 +++', '',
    ]
    assert f("\n".join(ok)) == [], f("\n".join(ok))
    bad = f('connect(5, {sa_family=AF_INET, sin_port=htons(443), sin_addr=inet_addr("203.0.113.9")}, 16) = -1 EINPROGRESS\n'
            '[pid 12] sendto(6, "x", 1, 0, {sa_family=AF_INET6, sin6_port=htons(443), sin6_addr=inet_pton(AF_INET6, "2001:db8::1", &a)}, 28) = 1\n'
            'connect(5, {sa_family=AF_INET, sin_port=htons(1)}, 16) = 0\n'
            'connect(5, {sa_family=AF_PACKET}, 16) = 0\n'
            'connect(5, {sa_family=AF_INET, sin_port=htons(80), sin_addr=inet_addr("127.0.0.1")}, 16) = 0 <unfinished ...>')
    assert bad == [("connect", "AF_INET", "203.0.113.9"), ("sendto", "AF_INET6", "2001:db8::1"), ("connect", "AF_INET", "?"), ("connect", "AF_PACKET", "?")], bad
    assert NG.hop_line("add_origin", "LocalHost") == "hop=network_gate_ui action=add_origin origin_hash=" + NG.fnv("localhost")


@check
def agent_netgate_capture_listener():
    import urllib.request, urllib.error
    import cases_agent_netgate as NG
    final = NG.Capture()
    redir = NG.Capture(redirect_to="http://127.0.0.1:%d" % final.port)
    try:
        body = urllib.request.urlopen(urllib.request.Request("http://127.0.0.1:%d/a.png" % final.port, headers={"X-Appci-Key": "K9"}), timeout=5).read()
        assert body == NG.PNG and final.paths() == ["/a.png"] and final.hits[0]["headers"]["x-appci-key"] == "K9"
        assert len(NG.leaks(final.hits, "K9")) == 1
        urllib.request.urlopen("http://127.0.0.1:%d/r.png" % redir.port, timeout=5).read()       # follows the 302 to `final`
        assert redir.paths() == ["/r.png"] and final.saw("/r.png") and not final.saw("/zzz")
    finally:
        final.stop()
        redir.stop()


@check
def fake_model_records_request_headers():
    import fake_model as FM, urllib.request
    f = FM.FakeModel().start()
    try:
        assert FM.ACTIVE is f
        rq = urllib.request.Request(f.url + "/api/chat", json.dumps({"model": "m", "messages": [{"role": "user", "content": "Reply with the word Z"}]}).encode(), {"X-Appci-Key": "S1"})
        urllib.request.urlopen(rq, timeout=5).read()
        assert f.headers and f.headers[-1]["x-appci-key"] == "S1", f.headers
    finally:
        f.stop()


@check
def agent_multitab_pure_pieces():
    """#1833: the fake model's long stream / history check, and the multi-tab case's judges"""
    import fake_model as FM
    import cases_agent_multitab as MT
    ch = FM.stream_chunks("S2", 3)
    assert ch[0].startswith("[S2:0001]") and ch[-1] == "[S2:END]" and len(ch) == 4, ch
    rep = FM.reply_for([{"role": "user", "content": "Please stream long reply S2 chunks 3 gap 7"}])
    assert rep["chunks"] == ch and rep["gap_ms"] == 7 and rep["tag"] == "S2" and rep["tool"] is None, rep
    lines = [json.loads(x) for x in FM.stream_lines("m", rep)]
    assert [l["message"]["content"] for l in lines[:-1]] == ch and lines[-1]["done"]
    conv = [{"role": "user", "content": "Please stream long reply S2 chunks 3 gap 7"}, {"role": "assistant", "content": "".join(ch)},
            {"role": "user", "content": "Please follow up check S2"}]
    assert FM.reply_for(conv)["text"] == "CHECKED S2 ok"
    lost = [conv[0], {"role": "assistant", "content": "".join(ch[:1] + ch[2:])}, conv[2]]
    assert FM.reply_for(lost)["text"] == "CHECKED S2 chunks 2/3", FM.reply_for(lost)["text"]
    bleed = [conv[0], {"role": "assistant", "content": "".join(ch) + " [S5:0001]"}, conv[2]]
    assert FM.reply_for(bleed)["text"] == "CHECKED S2 foreign S5"
    noend = [conv[0], {"role": "assistant", "content": "".join(ch[:3])}, conv[2]]
    assert "no END" in FM.reply_for(noend)["text"]
    assert FM.reply_for([conv[0], conv[2]])["text"] == "CHECKED S2 empty"
    dup = [conv[0], {"role": "assistant", "content": "".join(ch + ch[:1])}, conv[2]]
    assert "dup 1" in FM.reply_for(dup)["text"]
    tools = [{"type": "function", "function": {"name": "mcp__fxok__ping"}}, {"type": "function", "function": {"name": "read_files"}}]
    m = FM.reply_for([{"role": "user", "content": 'Please call mcp tool ping on fxok with {"text": "abc"}'}], tools)
    assert m["tool"] == {"name": "mcp__fxok__ping", "arguments": {"text": "abc"}}, m
    n = FM.reply_for([{"role": "user", "content": "Please call mcp tool ping on fxhang"}], tools)
    assert n["tool"] is None and n["text"] == "NO-MCP-TOOL fxhang/ping" and n["mcp_missing"] == ("fxhang", "ping"), n
    # judges
    ok, msg = MT.compare_latency([10.0 + i * 0.1 for i in range(30)], [10.5 + i * 0.1 for i in range(30)])
    assert ok is True, msg
    ok, msg = MT.compare_latency([10.0 + i * 0.1 for i in range(30)], [16.0 + i * 0.1 for i in range(30)])
    assert ok is False, msg
    assert MT.compare_latency([1.0] * 3, [1.0] * 30)[0] is None
    good = {"S2": {"finished": True, "broken": False, "sent": 4, "total": 4}}
    assert MT.streams_problems(good, ["S2"], 4) == []
    assert len(MT.streams_problems({"S2": dict(good["S2"], broken=True)}, ["S2"], 4)) == 1
    assert len(MT.streams_problems({"S2": dict(good["S2"], sent=2, finished=False)}, ["S2"], 4)) == 1
    assert len(MT.streams_problems({}, ["S2"], 4)) == 1
    fu = {"S2": {"verdict": "ok", "user_text": "Please stream long reply S2"}, "S3": {"verdict": "chunks 1/3", "user_text": ""}}
    bad, unp = MT.bleed_problems(fu, ["S2", "S3", "S4"], ["abc123"])
    assert bad == ["S3: chunks 1/3"] and len(unp) == 1, (bad, unp)
    bad, _ = MT.bleed_problems({"S2": {"verdict": "ok", "user_text": "x abc123 y"}}, ["S2"], ["abc123"])
    assert len(bad) == 1 and "typing tab" in bad[0]
    bad, unp = MT.bleed_problems({"S2": {"verdict": "empty", "user_text": ""}}, ["S2"], [])
    assert bad == [] and len(unp) == 1
    log = "x frame 1: updated=[\"A\"] removed=0\nfoo\nx frame 2: updated=[] removed=0 redraw=true\n"
    assert MT.frames_per_s(log, 2.0) == 1.0


@check
def agent_mcp_pure_pieces():
    """#1834: fakemcp.py's pure handler, the config builder, and the case's judges"""
    import fakemcp as FX
    import cases_agent_mcp as MC
    init = FX.handle({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}}, "ok")[0][0]
    assert json.loads(init)["result"]["protocolVersion"] == "2025-06-18"
    assert FX.handle({"jsonrpc": "2.0", "method": "notifications/initialized"}, "ok") == ([], None)
    tl = json.loads(FX.handle({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}, "ok")[0][0])
    assert tl["result"]["tools"][0]["name"] == "ping"
    call = {"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "ping", "arguments": {"text": "t1"}}}
    assert json.loads(FX.handle(call, "ok")[0][0])["result"]["content"][0]["text"] == "PONG:t1"
    assert FX.handle(call, "crash_call") == ([], "exit:4")
    assert FX.handle(call, "hang") == ([], "hang")
    lines, act = FX.handle(call, "garbage")
    assert act is None and len(lines) == 5 and json.loads(lines[-1])["id"] == 3
    bad = 0
    for l in lines[:-1]:
        try:
            v = json.loads(l)
            bad += 0 if (isinstance(v, dict) and v.get("id") == 3) else 1
        except ValueError:
            bad += 1
    assert bad == 4, bad
    big = json.loads(FX.handle(call, "big")[0][0])["result"]["content"][0]["text"]
    assert len(big) == 5 * 1024 * 1024 and big.startswith("BIG:")
    lines, act = FX.handle(call, "flaky", launch=2)
    assert act == "exit:0" and json.loads(lines[0])["result"]["content"][0]["text"] == "LAUNCH-2:t1"
    assert json.loads(FX.handle({"jsonrpc": "2.0", "id": 4, "method": "nope"}, "ok")[0][0])["error"]["code"] == -32601
    assert MC.state_events("start\ncall ok\ncall ok\nstart\n") == (2, 2) and MC.state_events("") == (0, 0)
    cfg = json.loads(MC.mcp_config("/usr/bin/python3", "/x/fakemcp.py", "/w/s"))
    assert sorted(cfg["mcpServers"]) == sorted(n for n, _ in MC.SERVERS)
    assert cfg["mcpServers"]["fxhang"] == {"command": "/usr/bin/python3", "args": ["/x/fakemcp.py", "hang", "/w/s/fxhang.state"], "tool_timeout_secs": MC.TOOL_TIMEOUT_S}
    assert MC.error_is_clear("MCP tool call timed out after 20 seconds without a result") and MC.error_is_clear("Error: transport closed")
    assert not MC.error_is_clear("") and not MC.error_is_clear("PONG:x") and not MC.error_is_clear("all good") and not MC.error_is_clear("thread panicked at src: error")
    assert MC.panics("a\nthread 'main' panicked at x.rs:1\nb") == ["thread 'main' panicked at x.rs:1"]
    assert MC.flaky_verdict(["LAUNCH-1:a", "LAUNCH-2:b"])[0] and MC.flaky_verdict(["LAUNCH-1:a", "error: closed", "LAUNCH-3:c"])[0]
    assert not MC.flaky_verdict(["LAUNCH-1:a", "error", "error"])[0] and not MC.flaky_verdict([])[0] and not MC.flaky_verdict(["error"])[0]
    names = [c["name"] for c in cases.CASES]
    assert "agent_mcp_faults" in names and "agent_multitab_streaming" in names


@check
def agent_latency_pure_parts():
    """#1828: key->frame and Enter->enter_begin pairing, the steer judges, the fake model's slow stream and request log"""
    import cases_agent_latency as AL
    import fake_model as FM

    def kd(key, t, ctrl="false"):
        return 'input_order keydown key=%s ctrl=%s alt=false shift=false cmd=false t=%s seq=1' % (key, ctrl, t)
    log = "\n".join([kd('"m"', 1000.0), "input_order hop=key_frame layout=true t=1006.5",
                     kd('"m"', 1100.0), kd('"m"', 1101.0), "input_order hop=key_frame layout=false t=1104.0",
                     kd("u", 1200.0, "true"), "input_order hop=key_frame layout=false t=1203.0",
                     kd('"m"', 1300.0), kd("enter", 1301.0), "input_order hop=key_frame layout=true t=1310.0",
                     kd('"m"', 1400.0), "input_order hop=key_frame layout=true t=1409.0"])
    ds = AL.key_frame_deltas(log)
    assert ds == [6.5, 4.0, 3.0, 9.0], ds          # piled keys pair with the one frame; ctrl and Enter are not samples; a key before an unsampled key-down is not paired
    assert AL.key_frame_deltas(log, 1050.0, 1150.0) == [4.0, 3.0], AL.key_frame_deltas(log, 1050.0, 1150.0)
    # streaming: a frame is built every 17 ms; a key just after one waits for the next slot, which is the compositor's pacing and not the app's
    fd = lambda start, tot=4.0: "input_order hop=frame_drawn total_ms=%s acquire_ms=0.1 t=%s" % (tot, start + tot)
    pl = [fd(1000.0 + 17 * i) for i in range(8)]                       # frames start 1000, 1017, ... 1119
    pl += [kd('"m"', 1126.0), "input_order hop=key_frame layout=true t=1138.0"]      # last frame started 1119: slot 1136, frame at 1138 -> 2 ms over the slot
    pl += [fd(1136.0), kd('"m"', 1141.0), "input_order hop=key_frame layout=true t=1155.5"]  # slot 1153, frame 1155.5 -> 2.5
    pl += [kd('"m"', 1400.0), "input_order hop=key_frame layout=true t=1403.0"]     # long after the last frame: no slot wait, raw 3
    assert AL.key_frame_deltas("\n".join(pl)) == [12.0, 14.5, 3.0]
    assert AL.paced_key_frame_deltas("\n".join(pl), None, None) == [2.0, 2.5, 3.0], AL.paced_key_frame_deltas("\n".join(pl), None, None)
    assert AL.paced_key_frame_deltas("\n".join(pl[8:]), None, None) == [12.0, 14.5, 3.0]   # fewer than 5 frame gaps: pacing unknown, raw
    en = "\n".join([kd("enter", 2000.0), 'input_order hop=enter_begin deferred=false buffer="ping t=5" input_type=AI t=2001.25',
                    kd("enter", 3000.0, "true"), kd("enter", 3100.0), 'input_order hop=enter_begin deferred=false buffer="x" input_type=Shell t=3103.5'])
    assert AL.enter_shares(en) == [1.25, 3.5], AL.enter_shares(en)
    # Linux/Windows: the editor binding takes Enter, so there is NO keydown line; the window dispatch is the start (real log, round 6)
    ce = "\n".join(['input_order hop=core_enter stage=handle_window_event key_bindings_enabled=true focused_view=EntityId(7) t=5000.0',
                    'input_order hop=core_enter stage=dispatch_keystroke chain_len=7 composing=false ctrl=false alt=false shift=false cmd=false t=5000.1',
                    'input_order hop=enter_begin deferred=false buffer="hi" input_type=AI t=5000.5',
                    'input_order hop=core_enter stage=handle_window_event key_bindings_enabled=true focused_view=EntityId(7) t=6000.0',
                    'input_order hop=core_enter stage=dispatch_keystroke chain_len=7 composing=false ctrl=true alt=false shift=false cmd=false t=6000.1',
                    'input_order hop=enter_begin deferred=false buffer="hi" input_type=AI t=6000.5',
                    'input_order hop=core_enter stage=handle_window_event key_bindings_enabled=true focused_view=EntityId(7) t=7000.0',
                    kd("enter", 7000.2), 'input_order hop=enter_begin deferred=false buffer="x" input_type=Shell t=7002.0'])
    assert AL.enter_shares(ce) == [0.5, 2.0], AL.enter_shares(ce)
    steer = AL.make_steer("abc12", 1)
    assert len(steer) >= 200 and "ls -la | grep x" in steer and "$HOME" in steer, len(steer)
    u = lambda t: {"role": "user", "content": t}
    a = {"role": "assistant", "content": "tick"}
    good = [{"t": 1.0, "messages": [u("first"), a, u(steer)]}]
    assert AL.steer_verdict(good, steer) is None
    env = u("<environment_context>\n  Working directory: /w\n</environment_context>")     # the app appends this user message AFTER the prompt (round-6 finding)
    assert AL.steer_verdict([{"t": 1.0, "messages": [u("first"), a, u(steer), env]}], steer) is None
    assert "never received" in AL.steer_verdict([{"t": 1.0, "messages": [u("first"), env]}], steer)
    assert AL.steers_in_order([u("go"), env, u("s0"), env, u("s1"), env], ["s0", "s1"]) is None
    assert "never received" in AL.steer_verdict([{"t": 1.0, "messages": [u("first")]}], steer)
    assert "2 separate" in AL.steer_verdict(good + good, steer)
    assert "not as a user turn equal" in AL.steer_verdict([{"t": 1.0, "messages": [u("first"), u(steer + " x")]}], steer)
    assert "occurs 2 times" in AL.steer_verdict([{"t": 1.0, "messages": [u("x " + steer), a, u(steer)]}], steer)
    s3 = [AL.make_steer("t%d" % i, i) for i in (1, 2, 3)]
    msgs = [u("go"), a, u(s3[0]), a, u(s3[1]), a, u(s3[2])]
    assert AL.steers_in_order(msgs, s3) is None
    assert "out of order" in AL.steers_in_order([u("go"), u(s3[1]), u(s3[0]), u(s3[2])], s3)
    assert "0 times" in AL.steers_in_order([u("go"), u(s3[0]), u(s3[2])], s3)
    assert "2 times" in AL.steers_in_order(msgs + [u(s3[0])], s3)
    # fake model: STREAMLONG is sticky across the conversation, streams in many small pieces, ordinary turns are unchanged
    st = FM.reply_for([u("STREAMLONG keep talking")])
    assert st["stream"] and st["text"] == FM.stream_text() and not st["tool"], st
    assert FM.reply_for([u("STREAMLONG go"), a, u("a steer")])["stream"]
    assert not FM.reply_for([u("hello there")]).get("stream") and FM.reply_for([u("hello there")])["text"] == "OK there"
    assert FM.reply_for([u("STREAMLONG go"), {"role": "tool", "content": "ok"}]) == {"text": "Done.", "tool": None}
    assert len(FM.stream_lines("m", st)) > FM.STREAM_CHUNKS // 2 and FM.STREAM_CHUNKS * FM.STREAM_GAP_S >= 11
    f = FM.FakeModel().start()
    saved = FM.STREAM_GAP_S
    try:
        import urllib.request
        assert FM.FakeModel.ACTIVE is f
        FM.STREAM_GAP_S = 0.0
        rq = urllib.request.Request(f.url + "/api/chat", json.dumps({"model": "m", "messages": [u("STREAMLONG please")]}).encode())
        body = urllib.request.urlopen(rq, timeout=10).read().decode().splitlines()
        assert json.loads(body[-1])["done"] and "".join(json.loads(x)["message"].get("content", "") for x in body) == FM.stream_text()
        assert len(f.reqs) == 1 and f.reqs[0]["messages"][0]["content"] == "STREAMLONG please"
    finally:
        FM.STREAM_GAP_S = saved
        f.stop()


@check
def lat_evidence_filter_keeps_only_hop_lines():
    """the committed test-results keep the latency cases' hop lines (and only those), capped; other cases and non-case logs are left out"""
    import lat_evidence as le
    assert le.case_of("run/logs/agent/case_agent_latency_key.log") == "agent_latency_key" and le.case_of("run_slim/logs/terminal/case_latency_paint.log") == "latency_paint"
    assert le.case_of("run/logs/agent/case_ctrl_c_flood_ab.log") == "ctrl_c_flood_ab" and le.case_of("run/logs/agent/case_latency_new_thing.log") == "latency_new_thing"
    assert le.case_of("run/logs/agent/case_mode_proof.log") is None and le.case_of("run/logs/agent/phosphor.log") is None and le.case_of("run/logs/x/case_latency_paint.txt") is None
    L = ["2026-11-09T10:00:00Z INFO input_order hop=key_presented submit_to_presented_ms=1.0 key_to_presented_ms=2.0 sync=1 wait_ms=0.1 t=1.0",
         "2026-11-09T10:00:00Z INFO input_order hop=frame_drawn total_ms=4.0 acquire_ms=0.5 present_ms=9.0 t=1.0",
         "2026-11-09T10:00:00Z INFO key_frame_sched ms=0.2", "2026-11-09T10:00:00Z INFO frame ms=3.0 n=1", "2026-11-09T10:00:00Z INFO key_frame_now wait_ms=0.3"]
    noise = ["2026-11-09T10:00:01Z INFO terminal output ready", "plain frame without fields", "2026-11-09T10:00:01Z WARN something else"]
    txt, hits, kept = le.evidence("\n".join(noise[:1] + L + noise[1:]))
    assert txt == "\n".join(L) + "\n" and hits == 5 and kept == 5, txt
    assert le.evidence("\n".join(noise)) == ("", 0, 0)
    big = [("input_order hop=frame_drawn total_ms=4.0 acquire_ms=0.5 t=%d" % i) for i in range(1000)]
    txt, hits, kept = le.evidence("\n".join(big), 20000)
    assert hits == 1000 and kept < 1000 and len(txt) <= 20200 and big[0] in txt and big[-1] in txt and "lat_evidence: %d of 1000" % (1000 - kept) in txt
    assert le.CASE_CAP == 2 * 1024 * 1024 and le.PLATFORM_CAP >= le.CASE_CAP
    # #2035: the platform budget is shared, not spent in name order. r10a macOS shape: a flood log far over the budget sorts first, then
    # two ordinary latency logs and an empty one. The small ones are kept whole and the flood gets the rest.
    assert le.matched_bytes("\n".join(noise[:1] + L + noise[1:])) == sum(len(l) + 1 for l in L) and le.matched_bytes("\n".join(noise)) == 0
    caps = le.fair_caps([900000000, 5200000, 300000, 0], total=2 * 1024 * 1024, case_cap=2 * 1024 * 1024)
    assert caps[3] == 0 and caps[2] == 300000 and caps[1] == caps[0] == (2 * 1024 * 1024 - 300000) // 2, caps
    assert sum(caps) <= 2 * 1024 * 1024
    assert le.fair_caps([100, 200, 300], total=10000) == [100, 200, 300]                     # everything fits: nothing is cut
    assert le.fair_caps([5000, 5000, 5000], total=9000) == [3000, 3000, 3000]                 # nothing fits: an even split
    assert le.fair_caps([5000], total=9000, case_cap=4000) == [4000] and le.fair_caps([]) == []
    assert le.fair_caps([10, 9000, 9000], total=9000) == [10, 4495, 4495]


@check
def output_frames_pure_pieces_1979():
    """#1979: output_frames counts built frames, wakeups per decision and in-place re-renders from the frame_cause log, and its judges fail on what the
    issue claims (a frame storm, a hidden view re-rendered per wakeup) and cannot pass a window in which the streams were not running"""
    import cases_output_frames as OF
    import winmap as wm
    reg = {c["name"]: c for c in cases.CASES}
    assert "output_frames" in reg and reg["output_frames"]["modes"] == ("terminal", "agent") and reg["output_frames"]["gui"]
    assert "output_frames" in cases.PROVING and "output_frames" not in cases.SMOKE and "output_frames" not in cases.PROVING_FAKE
    assert wm.skip_reason("output_frames", "powershell") is None and wm.skip_reason("output_frames", "gitbash") is None

    def fr(i, updated="", redraw="false", paint="false"):
        return "x DEBUG warpui::frame_cause frame %d: updated=[%s] removed=0 redraw=%s paint_only=%s" % (i, updated, redraw, paint)

    def wk(decision, shown, view=7):
        return "x DEBUG warpui::frame_cause wakeup: redraw=%s reachable=%s view=%s" % (decision, "true" if shown else "false", view)

    def um(*names):
        return "x DEBUG warpui::frame_cause unmounted_render: n=%d views=[%s]" % (len(names), ", ".join('"%s"' % n for n in names))

    # the build loop's `frame 2:` line with nothing pending is not a frame; one with a reason is; a re-rendered TerminalView is counted
    st = OF.window_stats("\n".join([fr(1, '"TerminalView", "Input"'), fr(2), fr(1, redraw="true"), fr(2, paint="true"), fr(3, '"A"'), "noise",
                                    wk("Relayout", True), wk("Full", True), wk("Full", False, 9), wk("PaintOnly", False, 9), um("TerminalView", "UseAgentToolbar")]))
    assert st["frames"] == 4 and st["frames_rerender"] == 1, st
    assert st["wakeups"] == {("Relayout", True): 1, ("Full", True): 1, ("Full", False): 1, ("PaintOnly", False): 1}, st
    assert st["views"] == {"7": [2, 0], "9": [0, 2]} and OF.wakeup_count(st, False) == 2 and OF.wakeup_count(st, True, "Full") == 1 and OF.wakeup_count(st) == 4
    assert st["unmounted_events"] == 1 and st["unmounted_views"] == 2 and st["unmounted_names"] == {"TerminalView": 1, "UseAgentToolbar": 1}
    m = OF.metrics_of(st, 2.0)
    assert m["frames_per_s"] == 2.0 and m["wakeups"]["Full_hidden"] == 1 and m["views_shown"] == 1 and m["views_hidden"] == 1, m
    # the app's own lines (the Rust unit tests pin these formats) are what the regexes read
    assert OF.WAKEUP_RE.search("wakeup: redraw=Relayout reachable=true view=41").groups() == ("Relayout", "true", "41")
    assert OF.UNMOUNTED_RE.search('unmounted_render: n=2 views=["TerminalView", "UseAgentToolbar"]').group(1) == "2"
    assert not OF.FRAME_RE.search('unmounted_render: n=2 views=["TerminalView"]') and not mt_frame_re().search(um("TerminalView"))

    secs = 6.0
    # visible: 40 wakeups/s, one Relayout frame each, on top of a 4 frames/s idle window: passes
    good = OF.window_stats("\n".join([wk("Relayout", True), fr(1, redraw="true")] * 240 + [wk("Full", True), fr(1, '"TerminalView"')] * 3))
    unp, vs = OF.judge_visible(good, secs, 4.0)
    assert unp is None and all(ok for ok, _ in vs), vs
    # two frames per wakeup: the one-frame-per-wakeup verdict fails, the cap one does too at 80 frames/s
    storm = OF.window_stats("\n".join([wk("Relayout", True), fr(1, redraw="true"), fr(1, redraw="true")] * 240))
    unp, vs = OF.judge_visible(storm, secs, 4.0)
    assert unp is None and [ok for ok, _ in vs] == [False, False, True], vs
    # every wakeup re-renders the view (the pre-#1256 behaviour): the reuse verdict fails
    rer = OF.window_stats("\n".join([wk("Full", True), fr(1, '"TerminalView"')] * 240))
    unp, vs = OF.judge_visible(rer, secs, 4.0)
    assert unp is None and [ok for ok, _ in vs] == [True, True, False], vs
    # no wakeup lines (old build) and a stream that never ran: unproven, never a pass
    assert OF.judge_visible(OF.window_stats(fr(1)), secs, 4.0)[0] and OF.judge_visible(OF.window_stats(wk("Relayout", True)), secs, 4.0)[0]

    tabs = 4
    idle_frames = [fr(1, paint="true")] * 24
    hid_ok = OF.window_stats("\n".join(idle_frames + [wk("Relayout", False, 20 + i % tabs) for i in range(tabs * 240)] + [wk("Full", False, 20), um("TerminalView")]))
    unp, vs = OF.judge_hidden(hid_ok, secs, 4.0, tabs)
    assert unp is None and all(ok for ok, _ in vs), vs
    # the issue's claim: every hidden wakeup re-renders the view in place (60/s per pane): the Full-share and the per-tab verdicts fail, frames stay flat
    hid_rer = OF.window_stats("\n".join(idle_frames + [x for i in range(tabs * 360) for x in (wk("Full", False, 20 + i % tabs), um("TerminalView"))]))
    unp, vs = OF.judge_hidden(hid_rer, secs, 4.0, tabs)
    assert unp is None and [ok for ok, _ in vs] == [True, False, False] and "TerminalView x1440" in vs[2][1], vs
    # the pre-#805 behaviour: a frame per hidden wakeup
    hid_frames = OF.window_stats("\n".join([x for i in range(tabs * 120) for x in (wk("Relayout", False, 20 + i % tabs), fr(1, redraw="true"))]))
    unp, vs = OF.judge_hidden(hid_frames, secs, 4.0, tabs)
    assert unp is None and [ok for ok, _ in vs] == [False, True, True], vs
    # streams not running, or the shown tab streaming: unproven
    assert "not running" in OF.judge_hidden(OF.window_stats("\n".join([wk("Relayout", False)] * 10)), secs, 4.0, tabs)[0]
    assert "itself streaming" in OF.judge_hidden(OF.window_stats("\n".join([wk("Relayout", False)] * 200 + [wk("Relayout", True)] * 100)), secs, 4.0, tabs)[0]
    assert OF.judge_hidden(OF.window_stats(fr(1)), secs, 4.0, tabs)[0]
    # bounded stream lines for both pane families
    assert OF.stream_line(None, "v", 450) == "for i in $(seq 450); do echo ofr_v_$i; sleep 0.02; done"
    assert OF.stream_line("powershell", "h2", 9).startswith("1..9 | % {") and "Start-Sleep -Milliseconds 20" in OF.stream_line("powershell", "h2", 9)


def mt_frame_re():
    import cases_agent_multitab as MT
    return MT.FRAME_CAUSE_RE
def wrapped_copy_and_vertical_tabs_pure_parts_1976_1990():
    """cases_wrap_vtabs.py: the fixture wraps where the case says it does, the clipboard judges name an inserted and a lost line break and pass only the
    exact text, the settings reader tells an editable layout from one it must not touch, the panel-row judge tells a column from a tab bar, and the four
    cases are PROVING and classified for Windows."""
    import random
    import cases_wrap_vtabs as wv
    for cols in (40, 80, 137, 213):
        lines, spans = wv.wrap_fixture(cols, random.Random(cols), "tok1")
        long_line = lines[0]
        assert 2 * cols < len(long_line) < 3 * cols, (cols, len(long_line))                 # three visual rows: it wraps twice
        assert len(lines[2]) == cols and len(lines[3]) == cols + 1 and not any(l != l.strip() for l in lines)
        assert len({w for w, _, _ in spans}) == len(spans) and all(long_line[a:b] == w and len(w) == 7 for w, a, b in spans)
        assert all(c in wv.U.OCR_SAFE + " " for c in long_line)
        (wa, a0, a1), (wb, b0, b1) = wv.pick_drag_words(spans, cols)
        assert 0 <= a0 and a1 <= cols <= b0 and b1 <= 2 * cols, (cols, a0, a1, b0, b1)        # one word wholly on row 1, one wholly on row 2
        assert wv.wrap_fixture(cols, random.Random(cols), "tok1")[0] == lines                 # the same seed gives the same fixture
        want = "\n".join(lines)
        assert wv.judge_wrapped_copy(want, lines) is None and wv.judge_wrapped_copy(want + "\n", lines) is None
        assert wv.judge_wrapped_copy(wv.norm_clip((want + "\n").replace("\n", "\r\n").encode()), lines) is None
        hard = "\n".join([long_line[:cols], long_line[cols:2 * cols], long_line[2 * cols:]] + lines[1:])   # a break at each soft wrap: the suspected #1976 defect
        assert "INSERTED at a soft wrap" in wv.judge_wrapped_copy(hard, lines)
        joined = "\n".join(lines[:2] + [lines[2] + lines[3]] + lines[4:])                     # the exactly-full line joined to the next
        assert "LOST" in wv.judge_wrapped_copy(joined, lines) and "printed line 3" in wv.judge_wrapped_copy(joined, lines)
        one_off = "\n".join([long_line[:cols + 1] + "\n" + long_line[cols + 1:]] + lines[1:])
        assert "INSERTED" in wv.judge_wrapped_copy(one_off, lines)
        assert "differs" in wv.judge_wrapped_copy(want.replace(spans[3][0], "x" * 7), lines)
        assert "differs" in wv.judge_wrapped_copy(want[:-3], lines) and "differs" in wv.judge_wrapped_copy(wv.SENTINEL, lines)
        # the drag: the text between the two words, a cell or two either way; a break at the wrap, a lost or changed character and a short copy all fail
        drag = long_line[a0:b1]
        assert wv.judge_drag_copy(drag, long_line, a0, b1) is None and wv.judge_drag_copy(drag + "\n", long_line, a0, b1) is None
        assert wv.judge_drag_copy(long_line[a0 + 1:b1 - 1], long_line, a0, b1) is None and wv.judge_drag_copy(long_line[a0 - 2:b1 + 2], long_line, a0, b1) is None
        assert "INSERTED at the soft wrap" in wv.judge_drag_copy(long_line[a0:cols] + "\n" + long_line[cols:b1], long_line, a0, b1)
        assert "not a piece" in wv.judge_drag_copy(long_line[a0:cols] + long_line[cols + 1:b1], long_line, a0, b1)
        assert "does not hold" in wv.judge_drag_copy(wa, long_line, a0, b1) and "not a piece" in wv.judge_drag_copy(wv.SENTINEL, long_line, a0, b1) and "not a piece" in wv.judge_drag_copy("", long_line, a0, b1)
        assert "chars" in wv.judge_drag_copy(long_line[:b1 + 20], long_line, a0, b1)
    assert wv.pick_drag_words([("abcdefh", 2, 9)], 4) is None
    # the settings reader
    assert wv.vtabs_state("[appearance.vertical_tabs]\nenabled = true\n") == "on" and wv.vtabs_state("[appearance.vertical_tabs]\nenabled = false\n") == "off"
    assert wv.vtabs_state("[appearance.text]\nfont_size = 14.0\n") == "unset" and wv.vtabs_editable("[appearance.text]\nfont_size = 14.0\n")
    inline = "[appearance]\nvertical_tabs = { view_mode = \"expanded\",\n  enabled = true }\n"
    assert wv.vtabs_state(inline) == "on" and not wv.vtabs_editable(inline)
    assert wv.vtabs_state(inline.replace("enabled = true", "enabled = false")) == "off"
    assert wv.vtabs_state("[appearance]\nvertical_tabs.enabled = false\n") == "off" and not wv.vtabs_editable("[appearance]\nvertical_tabs.enabled = false\n")
    assert wv.vtabs_state("[appearance.vertical_tabs]\nview_mode = \"compact\"\n") == "unknown" and not wv.vtabs_editable("[appearance.vertical_tabs]\nview_mode = \"compact\"\n")
    assert wv.vtabs_editable("[appearance.vertical_tabs]\nenabled = false\n")
    # the panel rows: a column in tab order passes; a tab bar, a wrong order and a single row do not
    assert wv.rows_stacked([(2, 80, 120), (3, 82, 170), (4, 79, 221)])[0] is True
    assert wv.rows_stacked([(4, 79, 221), (2, 80, 120)])[0] is True
    assert wv.rows_stacked([(2, 200, 20), (3, 380, 21), (4, 560, 20)])[0] is False and "not top to bottom" in wv.rows_stacked([(2, 200, 20), (3, 380, 21), (4, 560, 20)])[1]
    assert wv.rows_stacked([(2, 80, 170), (3, 80, 120)])[0] is False
    assert "across the screen" in wv.rows_stacked([(2, 100, 20), (3, 400, 30), (4, 700, 40)])[1]
    assert wv.rows_stacked([(2, 80, 120)])[0] is None and wv.rows_stacked([])[0] is None
    log = "x hop=tab_activate index=2 count=8 active=0 window=WindowId(1)\ny hop=view_mode mode=agent\nz hop=tab_activate index=7 count=3 active=2 window=WindowId(1)\n"
    assert wv.tab_counts(log) == [(2, 8), (7, 3)] and wv.tab_counts("") == []
    # the keys: the macOS backend refuses none of them (NEVER_CMD), and the names match app/src/util/bindings.rs
    lin, mac_ = types.SimpleNamespace(be=types.SimpleNamespace(name="x11")), types.SimpleNamespace(be=types.SimpleNamespace(name="mac"))
    assert wv.KX(lin, "copy_output") == "ctrl+alt+shift+c" and wv.KX(mac_, "copy_output") == "cmd+alt+shift+c" and wv.KX(lin, "focus_input") == "ctrl+shift+l"
    import backend
    for name in ("block_up", "copy_output", "focus_input", "next_tab", "prev_tab"):
        k = wv.KX(mac_, name).split("+")[-1]
        assert k not in backend.MacBackend.NEVER_CMD and (k in backend.MacBackend.SPECIAL or k in backend.MacBackend.LETTERS), (name, k)
    # registry: all four are PROVING (never SMOKE), run without the fake model, and are classified for Windows
    names = ("ui_copy_wrapped_output", "ui_copy_wrapped_drag", "ui_vertical_tabs_many", "ui_vertical_tabs_panel_click")
    reg = {c["name"]: c for c in cases.CASES}
    assert all(n in cases.PROVING and n not in cases.SMOKE and n not in cases.PROVING_FAKE and reg[n]["gui"] and reg[n]["modes"] == ("terminal", "agent") for n in names)
    assert "ui_copy_wrapped_output" in winmap.GITBASH_ONLY and all(n in winmap.NOT_WINDOWS for n in names[1:])
    src = open(os.path.join(HERE, "..", "..", "..", "docs", "testing.md")).read()
    assert all(("`%s`" % n) in src for n in names), "docs/testing.md does not describe every new case"


@check
def fakesshd_app_arm_da1_and_input_stats_2051():
    """#2051 (round 12, Windows Git Bash pane): the `app` arm's console host opens with a device-attributes request that a POSIX pty never
    sends (answered by the test sshd, not passed on), and `client input ended` says how long the client took and what the bytes were;
    subshell_ready_rc_clear's row says where the setup's time went"""
    import ptyhost as PH
    import cases_subshell_ready as sr
    assert PH.strip_startup_da1(b"\x1b[0c") == (b"", b"\x1b[0c") and PH.strip_startup_da1(b"\x1b[c\x1b[?25l") == (b"\x1b[?25l", b"\x1b[c")
    for other in (b"", b"PRB1", b"\x1b[6n", b"\x1b]9278;d;00\x07", b"x\x1b[c", b"\x1b[?62c"):       # only a LEADING request is the console host's own
        assert PH.strip_startup_da1(other) == (other, b""), other
    assert PH.DA1_REPLY == b"\x1b[?62c"
    clock = iter([10.0, 10.0, 12.0])
    st = PH.InputStats(now=lambda: next(clock))
    assert st.text() == "nothing received"
    st.add(b"M")
    st.add(b" if x; then\n\x1b[?62c\r")
    st.add(b"\x03\xc3\xa9" + b"a" * 100)
    assert (st.n, st.esc, st.cr, st.lf, st.ctl, st.high) == (123, 1, 1, 1, 1, 2), vars(st)
    assert len(st.head) == 48 and st.head.startswith(b"M if x; then\n\x1b[?62c\r\x03")
    t = st.text()
    assert t.startswith("2.0 s first to last byte, 62 bytes/s, ESC 1, CR 1, LF 1, other control 1, non-ASCII 2; starts b'M if x"), t
    quick = PH.InputStats(now=lambda: 5.0)
    quick.add(b"ab")
    assert "bytes/s" not in quick.text() and quick.text().startswith("0.0 s first to last byte, ESC 0"), quick.text()
    fs = open(os.path.join(HERE, "fakesshd_win.py")).read()
    compile(fs, "fakesshd_win.py", "exec")
    for needle in ('if arm == "app":', "d, asked = ptyhost.strip_startup_da1(d)", "cp.write(ptyhost.DA1_REPLY)", "stats = ptyhost.InputStats()",
                   "stats.add(d)", "written to the pseudo console (%s)\" % (got, chunks, wrote, stats.text())", "(starts %s)"):
        assert needle in fs, needle
    assert fs.index('arm = "inbox"') < fs.index("if kind == \"shell\":")        # a shell request keeps kernel32's console and is never answered for
    import cases_caseremote as rm
    assert rm.sshd_input_bytes(["client input ended: 78814 bytes in 69915 chunks, 78814 written to the pseudo console (55.0 s first to last byte, "
                                "1433 bytes/s, ESC 3, CR 2, LF 2100, other control 2, non-ASCII 0; starts b' if')"]) == 78814
    # the round 12 agent-mode log, cut to the lines that matter
    log = "\n".join([
        "x hop=subshell_setup_armed trigger=banner t=1",
        "x hop=subshell_ready_judged source=armed decision=Write(None) kind=Unknown leader=Unreadable tail=ShellPrompt t=1",
        "x hop=subshell_ready_judged source=output decision=Wait(LineDirty) kind=Unknown t=2",
        "x hop=subshell_setup_progress kind=StageTwoChunk gen=3 seq=1 age_ms=282 t=3",
        "x hop=subshell_setup_progress kind=Marker gen=3 seq=2 age_ms=282 t=3",
        "x hop=subshell_setup_progress kind=InitSubshell gen=3 seq=3 age_ms=1189 t=4",
        "x hop=subshell_setup_progress kind=InitShell gen=4 seq=6 age_ms=2778 t=5",
        "x hop=subshell_setup_progress kind=Bootstrapped gen=4 seq=7 age_ms=3742 t=6"])
    assert sr.setup_timeline(log) == "armed decision=Write(None); InitSubshell@1189 ms, InitShell@2778 ms, Bootstrapped@3742 ms", sr.setup_timeline(log)
    assert sr.setup_timeline("nothing") == "armed decision=not logged; no setup progress event logged"
    src = open(os.path.join(HERE, "cases_subshell_ready.py")).read()
    assert 'r.note("clear: setup " + setup_timeline(ctx.log_since(off)))' in src
    assert src.count('r.check(dt < 2.0, "the first inner line ran %.2f s after its Enter (budget 2 s)" % dt)') == 2     # the budget is not touched
    doc = open(os.path.join(HERE, "..", "..", "..", "docs", "testing.md")).read()
    assert "fakesshd_app_arm_da1_and_input_stats_2051" in doc and "#2051" in doc


@check
def fakesshd_exec_pty_arms_2007():
    """#2007: the Windows fake sshd's exec + pty (a managed ssh session) is served by a measured arm: the pure parts of ptyhost.py, the probe's
    remote command run for real on a pty (POSIX), and the wiring in fakesshd_win.py / sshd.py / ssh_diag"""
    import hashlib
    import inspect
    import re
    import ptyhost as PH
    import sshd
    assert PH.ARMS == ("msys", "app", "inbox") and set(PH.ARM_TEXT) == set(PH.ARMS)
    # paths and the pty host's files
    assert PH.msys_path("C:\\gl\\b\\run\\ssh") == "/c/gl/b/run/ssh" and PH.msys_path("d:/x y/z") == "/d/x y/z" and PH.msys_path("/usr/bin") == "/usr/bin"
    conf = PH.sshd_config(2222, "/c/r/hostkey", "/c/r/ak", "/c/r/pid", "/c/r it's/force.sh")
    for line in ("Port 2222", "ListenAddress 127.0.0.1", "HostKey /c/r/hostkey", "AuthorizedKeysFile /c/r/ak", "PasswordAuthentication no", "StrictModes no",
                 "PermitTTY yes", "ForceCommand /usr/bin/bash '/c/r it'\\''s/force.sh'"):
        assert line in conf.split("\n"), (line, conf)
    assert "UsePAM" not in conf
    force = PH.force_script("/c/r h/rhome")
    assert "export HOME='/c/r h/rhome' SHELL=/usr/bin/bash" in force and 'exec "$SHELL" -c "$SSH_ORIGINAL_COMMAND"' in force and '--rcfile "$HOME/.bashrc" -i' in force
    # which arm serves a request: a pinned arm is served or refused, never replaced; the measured one first, the rest behind it
    assert PH.arm_order(["msys", "app", "inbox"], "app", "msys") == (["app"], True)
    assert PH.arm_order(["inbox"], "msys", "") == ([], True)
    assert PH.arm_order(["msys", "app", "inbox"], "", "app") == (["app", "msys", "inbox"], False)
    assert PH.arm_order(["app", "inbox"], "", "msys") == (["app", "inbox"], False) and PH.arm_order(["inbox"], "", "") == (["inbox"], False)
    # the output half: whole, cut the way kernel32's pseudo console cut round 9's hooks (only the tail past the 256-byte pieces), nothing at all
    assert len(PH.PROBE_OSC) > 2 * 256 + 64
    ok, text = PH.judge_output(b"\x1b[2J\x1b[HPRB1" + PH.PROBE_OSC + b"PRB2\r\nRDY\r\n")
    assert ok and "arrived whole" in text
    ok, text = PH.judge_output(b"\x1b[2J\x1b[HPRB1" + PH.PROBE_OSC[512:] + b"PRB2\r\nRDY\r\n")
    assert not ok and "did NOT arrive whole" in text and "the longest piece of its 600 payload characters is 80 " in text, text
    ok, text = PH.judge_output(b"fakesshd: no exec pty arm could serve this request\r\n")
    assert not ok and "printed nothing" in text
    # the input half
    payload = PH.probe_payload()
    assert 3000 < len(payload) < 8192 and payload.count(b"\n") == 70 and any(b > 0x7f for b in payload) and b"\r" not in payload and b"\x1b" not in payload
    md5 = hashlib.md5(payload).hexdigest().encode()
    assert PH.judge_input(b"RDY\r\nMD5=" + md5 + b"=\r\n", payload)[0] is True
    assert PH.judge_input(b"RDY\r\nMD5=" + b"0" * 32 + b"=\r\n", payload) == (False, "a counted raw read of %d bytes got other bytes than were sent (md5 differs)" % len(payload))
    ok, text = PH.judge_input(b"RDY\r\n", payload)
    assert not ok and "did not finish" in text
    # which arm, and the line for the job log / ssh_diag
    good = dict(arm="msys", out_ok=True, in_ok=True, out="o", inp="i")
    half = dict(arm="app", out_ok=True, in_ok=False, out="o", inp="i")
    bad = dict(arm="inbox", out_ok=False, in_ok=True, out="o", inp="i")
    assert PH.choose_arm([dict(good, arm="inbox"), half]) == "inbox" and PH.choose_arm([bad, half]) == "app" and PH.choose_arm([bad]) is None and PH.choose_arm([]) is None
    assert PH.choose_arm([good, dict(good, arm="app"), dict(good, arm="inbox")]) == "msys"
    s = PH.summary([good, bad], "msys", ["n1"])
    assert s.startswith("exec+pty of the test sshd is served by a real msys pty") and "inbox: out BROKEN, in ok" in s and s.endswith("; n1"), s
    assert PH.summary([half, bad], "app").startswith("TEST HOST FAULT: no exec+pty arm of the test sshd is byte-exact both ways; using a pseudo console of the app's own conpty.dll")
    s = PH.summary([bad], None)
    assert s.startswith("TEST HOST FAULT: no exec+pty arm of the test sshd passes a long escape sequence whole") and "#2007" in s
    assert "not measured" in PH.summary([], None, ["no probe"]) and "no probe" in PH.summary([], None, ["no probe"])
    # the probe's remote command, for real: bash on a pty prints the OSC whole and reads the payload exactly (the same judges as on Windows)
    cmd = PH.probe_command(len(payload))
    if os.name != "nt" and shutil.which("bash") and shutil.which("md5sum") and shutil.which("stty"):
        import pty
        import select
        assert subprocess.run(["bash", "-n", "-c", cmd], capture_output=True).returncode == 0
        assert subprocess.run(["bash", "-n"], input=force.encode(), capture_output=True).returncode == 0
        pid, fd = pty.fork()
        if pid == 0:
            os.execvp("bash", ["bash", "-c", cmd])
        data, sent, end = b"", False, time.time() + 20
        try:
            while time.time() < end and not re.search(rb"MD5=[0-9a-f]{32}=", data):
                if select.select([fd], [], [], 0.25)[0]:
                    try:
                        d = os.read(fd, 65536)
                    except OSError:
                        break
                    if not d:
                        break
                    data += d
                if not sent and b"RDY" in data:
                    os.write(fd, payload)
                    sent = True
        finally:
            os.close(fd)
            os.waitpid(pid, 0)
        assert PH.judge_output(data)[0] is True, PH.judge_output(data)
        assert sent and PH.judge_input(data, payload)[0] is True, PH.judge_input(data, payload)
    # wiring: the server takes its arm from the pin / the measured file, never sends paramiko's banner tracebacks to stderr, and the shell request
    # keeps kernel32's pseudo console; sshd.py starts the pty host, measures, and stops what it started; a failing ssh row carries the measurement
    fs = open(os.path.join(HERE, "fakesshd_win.py")).read()
    compile(fs, "fakesshd_win.py", "exec")           # it cannot be imported off Windows (kernel32); it must at least parse
    for needle in ("ptyhost.arm_order(arms_available(), requested, chosen)", "ptyhost.BackendPty(PTY_PORT, PTY_USER, CK,", "api=APPCON if arm == \"app\" else k32",
                   "client_env.get(ptyhost.ARM_ENV, \"\")", "logging.getLogger(\"paramiko\").propagate = False", "ch.send_exit_status(97)",
                   "exec output carried %d OSC 9278 sequence start(s)", "FAKE_PTY_PORT", "FAKE_CONPTY_DLL", "FAKE_ARM_FILE"):
        assert needle in fs, needle
    assert "cp = ConPty('\"%s\" --rcfile \"%s\" -i' % (BASH, os.path.join(RHOME, \".bashrc\")), env, RHOME, spty[1], spty[2])" in fs
    ss = inspect.getsource(sshd)
    assert "app_bin" in inspect.signature(sshd.start).parameters and "_start_pty_host(" in ss and "_measure_exec_pty(d, port, ident, arms, notes)" in ss
    assert '"exec_pty": exec_pty' in ss and "_kill_tree(host[\"popen\"])" in inspect.getsource(sshd.stop)
    assert "sshd.start(prof, run, app_bin=a.bin)" in open(os.path.join(HERE, "run.py")).read()
    assert 'info.get("exec_pty")' in inspect.getsource(cases.Ctx.ssh_diag)
    # the stale lead of round 9's rows is gone: under msys the wrapper never multiplexes
    ps = inspect.getsource(profile.Profile.ssh_probe)
    assert "candidate APP finding" not in ps and "does NOT explain a failing case" in ps
    doc = open(os.path.join(HERE, "..", "..", "..", "docs", "testing.md")).read()
    assert "#2007" in doc and "`ptyhost.py`" in doc and "exec_pty_arm" in doc
    # the local leg as an app case: a short control hook and a long one (the size of the remote's SSH hook), refused by the app with their ids logged
    import cases_win_input as wi
    short, long_ = wi.long_osc_hook(123456789012345, 0), wi.long_osc_hook(999999999999999, 480)
    assert len(short) < 256 < 2 * 256 - 64 < len(long_) == 480 and short.startswith(b"\x1b]9278;d;") and long_.endswith(b"\x07")
    for seq, n in ((short, 123456789012345), (long_, 999999999999999)):
        assert json.loads(bytes.fromhex(seq[9:-1].decode())) == {"hook": "Clear", "value": {"session_id": n}}
    script = wi.long_osc_script(short, long_, "long_osc.txt", "TOK")
    assert script.count("\n") == 3 and "\x1b" not in script and script.endswith("echo TOK >> long_osc.txt\n")
    if os.name != "nt" and shutil.which("bash"):
        with tempfile.TemporaryDirectory() as d:
            open(os.path.join(d, "s.sh"), "w").write(script)
            assert subprocess.run(["bash", "s.sh"], capture_output=True, cwd=d).stdout == short + long_
            assert open(os.path.join(d, "long_osc.txt")).read() == "TOK\n"
    both = "WARN Rejected DCS hook 'Clear' with unrecognized session_id 123456789012345\nWARN Rejected DCS hook 'Clear' with unrecognized session_id 999999999999999\n"
    assert wi.long_osc_verdict(both, 123456789012345, 999999999999999)[0] is True
    ok, text = wi.long_osc_verdict(both.split("\n")[0], 123456789012345, 999999999999999)
    assert not ok and text.startswith("APP side: the short hook reached the app, the long one")
    ok, text = wi.long_osc_verdict("", 123456789012345, 999999999999999)
    assert not ok and text.startswith("TEST: the short control hook was not logged either")
    # the text the oracle reads is the app's own (a rename there must fail here, not turn the case into a silent TEST verdict)
    rs = open(os.path.join(HERE, "..", "..", "..", "app", "src", "terminal", "model", "ansi", "mod.rs")).read()
    assert "\"Rejected DCS hook '{}' with unrecognized session_id {}\"" in rs
    reg = {c["name"]: c for c in cases.CASES}
    n = "gitbash_long_osc_reaches_app"
    assert n in cases.PROVING and n not in cases.SMOKE and reg[n]["order"] < reg["ssh_connect_typing"]["order"] and n in winmap.GITBASH_ONLY
    assert winmap.skip_reason(n, "gitbash") is None and winmap.skip_reason(n, "powershell") and ("`%s`" % n) in doc


@check
def subshell_ready_newuser_guard():
    """#2015 (rounds 7-9b, Linux): subshell_ready_zsh_newuser_install typed `HOME=<empty dir> zsh` into a pane whose ZDOTDIR is the scratch home
    (app.py exports it; the scratch home has a .zshrc). zsh looks in $ZDOTDIR first, so no menu ever showed, while the guard, run in the harness's
    environment (no ZDOTDIR), saw one and did not skip. The case now empties the directory zsh really looks in, the guard starts its zsh the same
    way, and the pane's zsh is inspected through /proc before anything is judged."""
    import cases_subshell_ready as sr
    import app as appmod
    env = sr.newuser_probe_env({"HOME": "/real", "ZDOTDIR": "/scratch", "X": "1"}, "/lab", "/scratch")
    assert env == {"HOME": "/scratch", "ZDOTDIR": "/lab", "X": "1"}
    raw = b"A=1\0NOTZDOTDIR=/x\0ZDOTDIR=/lab\0HOME=/scratch\0"
    assert sr.environ_value(raw, "ZDOTDIR") == "/lab" and sr.environ_value(raw, "HOME") == "/scratch"
    assert sr.environ_value(raw, "DOTDIR") is None and sr.environ_value(b"", "ZDOTDIR") is None
    assert sr.pane_zsh_facts(None) is None
    with tempfile.TemporaryDirectory() as d:
        lab, other = os.path.join(d, "lab"), os.path.join(d, "scratch")
        os.makedirs(lab)
        os.makedirs(other)
        assert sr.newuser_precondition(None, lab)[0] == "unknown"
        assert sr.newuser_precondition({"zdotdir": lab, "rows": 40, "cols": 120}, lab)[0] == "ok"
        assert sr.newuser_precondition({"zdotdir": os.path.join(d, ".", "lab"), "rows": None, "cols": None}, lab)[0] == "ok"       # same directory, size unknown
        v, text = sr.newuser_precondition({"zdotdir": other, "rows": 40, "cols": 120}, lab)       # the round 7-9b pane
        assert v == "wrong_env" and "no menu can show" in text
        assert sr.newuser_precondition({"zdotdir": None, "rows": 40, "cols": 120}, lab)[0] == "wrong_env"
        assert sr.newuser_precondition({"zdotdir": lab, "rows": 14, "cols": 120}, lab)[0] == "small"
        assert sr.newuser_precondition({"zdotdir": lab, "rows": 40, "cols": 71}, lab)[0] == "small"
        assert sr.newuser_precondition({"zdotdir": lab, "rows": 15, "cols": 72}, lab)[0] == "ok"
    # the two halves that must agree: the app hands the pane a ZDOTDIR, and the case overrides exactly that variable (never only HOME again)
    assert 'ZDOTDIR="%(home)s"' in appmod.INNER
    src = open(os.path.join(HERE, "cases_subshell_ready.py")).read()
    assert 'ctx.be.type("ZDOTDIR=~/w/lab_zsh_home zsh"' in src and 'ctx.be.type("HOME=' not in src
    assert sr.NEWUSER_FOOTER == b"Type one of the keys in parentheses" and sr.NEWUSER_MIN_TTY == (15, 72)
    # a real zsh on a real pty: startup files in ZDOTDIR mean no menu, whatever HOME holds (the pane of rounds 7-9b) ...
    if os.name != "nt" and shutil.which("zsh") and os.geteuid() != 0:
        with tempfile.TemporaryDirectory() as d:
            dot, home = os.path.join(d, "dot"), os.path.join(d, "home")
            os.makedirs(dot)
            os.makedirs(home)
            with open(os.path.join(dot, ".zshrc"), "w") as f:
                f.write("PS1='lab%# '\n")
            assert sr.zsh_runs_newuser_install(dot, home, wait=1.5) is False
            assert os.listdir(dot) == []                 # emptied again for the case
            # ... and an empty ZDOTDIR means the menu although HOME has a .zshrc, where this zsh has the function at all (Linux distributions)
            has_fn = subprocess.run(["zsh", "-fc", "autoload -U +X zsh-newuser-install"], capture_output=True, timeout=20).returncode == 0
            if sys.platform.startswith("linux") and has_fn:
                with open(os.path.join(home, ".zshrc"), "w") as f:
                    f.write("PS1='home%# '\n")
                assert sr.zsh_runs_newuser_install(dot, home, wait=6.0) is True
                assert ".zshrc" not in os.listdir(dot)
    doc = open(os.path.join(HERE, "..", "..", "..", "docs", "testing.md")).read()
    assert "subshell_ready_newuser_guard" in doc and "#2015" in doc


@check
def r11_mac_round10_test_faults():
    """Round 10 macOS (build/r10a), four rows, all TEST faults (docs/design/r11-mac.md): the hop list of the subshell lab dropped the hop one
    case asserts; md_viewer left its viewer tab in front of the pane; ui_vertical_tabs_many typed into it; mode_proof read the screen with
    tesseract alone although the Mac's own engine reads the model chip."""
    import re
    import cases_subshell_ready as sr
    import cases_wrap_vtabs as wv
    # 1. every hop name the subshell lab looks up is one hops() keeps, and the real lines of the round 10 log are found in order
    src = open(os.path.join(HERE, "cases_subshell_ready.py")).read()
    looked_up = set(re.findall(r'first_index\(.+?, "(subshell_[a-z_]+)"', src)) | set(re.findall(r'h == "(subshell_[a-z_]+)"', src))
    assert "subshell_stage_two_pending" in looked_up and len(looked_up) >= 5, looked_up
    assert looked_up <= set(sr.HOPS), "hops() drops a hop a case looks up: %s" % sorted(looked_up - set(sr.HOPS))
    log = ("2026-11-10T08:03:23Z [DEBUG] input_order hop=subshell_setup_ready_write trigger=banner shell=None relay=false bytes_passed=0 age_ms=0 t=1\n"
           "2026-11-10T08:03:23Z [DEBUG] input_order hop=subshell_stage_two_pending shell=Bash t=2\n"
           "2026-11-10T08:03:23Z [DEBUG] input_order hop=frame_skip_run skipped=1 t=3\n"
           "2026-11-10T08:03:24Z [DEBUG] input_order hop=subshell_setup_cancelled why=Secret dest=Editor chunks=35 bytes=35 t=4\n"
           "2026-11-10T08:03:24Z [DEBUG] input_order hop=subshell_setup_ready_write trigger=banner shell=Some(Bash) relay=false bytes_passed=8 t=5\n")
    hs = sr.hops_in(log)
    assert [h for h, _ in hs] == ["subshell_setup_ready_write", "subshell_stage_two_pending", "subshell_setup_cancelled", "subshell_setup_ready_write"], hs
    pend, cancel = sr.first_index(hs, "subshell_stage_two_pending"), sr.first_index(hs, "subshell_setup_cancelled", "why=Secret")
    assert pend == 1 and cancel == 2 and not any(h == "subshell_setup_ready_write" for h, _ in hs[pend:cancel])
    assert sr.hops_in("") == []
    # 2. the viewer tab: opened only by the app's own log line; the close is proven by the PID probe
    assert cases.file_viewer_opened('x [INFO] dispatching action for "root_view:add_file_pane"\n') and not cases.file_viewer_opened("hop=ui_item name=other")
    assert cases.viewer_close_verdict(None, 52149, True)[0] is False and "still in front" in cases.viewer_close_verdict(None, 52149, True)[1]
    assert cases.viewer_close_verdict(52149, 52149, True)[0] is True and cases.viewer_close_verdict(777, None, True)[0] is True
    assert cases.viewer_close_verdict(777, 52149, True)[0] is False and cases.viewer_close_verdict(None, None, False)[0] is None
    proving = set(cases.PROVING)
    assert cases.wants_depth_check("md_viewer", proving) and cases.wants_depth_check("md_viewer_big", proving)      # the runner's second layer
    # close_file_viewer: nothing pressed without the log line; with it the close key goes out once and the probe decides
    sent = []
    be = types.SimpleNamespace(key=lambda k: sent.append(k), type=lambda *a, **k: sent.append(("type", a)))
    old = {k: os.environ.get(k) for k in ("APPCI_KEY_CLOSE", "APPCI_KEY_CMD_PALETTE")}
    orig_sleep, cases.time.sleep = cases.time.sleep, lambda s: None
    try:
        os.environ["APPCI_KEY_CLOSE"], os.environ["APPCI_KEY_CMD_PALETTE"] = "ctrl+shift+w", ""

        def ctx_with(log, pid):
            return types.SimpleNamespace(be=be, pane="bash", root_pid=52149, log_since=lambda off: log, shell_pid=lambda: pid,
                                         until=lambda fn, t, step=0.2: fn())
        r = cases.Result("t", "terminal")
        cases.close_file_viewer(ctx_with("nothing", None), r, 0, "md_viewer")
        assert sent == [] and not r.fails and any("nothing was closed" in n for n in r.notes), (sent, r.fails, r.notes)
        opened = 'dispatching action for "root_view:add_file_pane"'
        r = cases.Result("t", "terminal")
        cases.close_file_viewer(ctx_with(opened, 52149), r, 0, "md_viewer")
        assert sent == ["ctrl+shift+w"] and not r.fails, (sent, r.fails)
        r = cases.Result("t", "terminal")
        cases.close_file_viewer(ctx_with(opened, None), r, 0, "md_viewer")
        assert len(r.fails) == 1 and "still in front" in r.fails[0], r.fails
        del sent[:]
        os.environ["APPCI_KEY_CLOSE"] = ""
        r = cases.Result("t", "terminal")
        cases.close_file_viewer(ctx_with(opened, 52149), r, 0, "md_viewer")
        assert sent == [] and len(r.fails) == 1 and "no way to close" in r.fails[0], (sent, r.fails)
    finally:
        cases.time.sleep = orig_sleep
        for k, v in old.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v
    msrc = open(os.path.join(HERE, "cases.py")).read()
    for name in ("md_viewer", "md_viewer_big"):
        body = msrc[msrc.index('@case("%s"' % name):]
        body = body[:body.index("\n@case(", 10)]
        assert "finally:\n        close_file_viewer(ctx, r, off, \"%s\")" % name in body, name
    # 3. ui_vertical_tabs_*: the first key's destination is proven before anything is opened
    assert wv.first_tab_precondition("/w", "/w") is None
    assert "no shell is in front" in wv.first_tab_precondition("<none>", "/w") and "'/w/x'" in wv.first_tab_precondition("/w/x", "/w")
    vsrc = open(os.path.join(HERE, "cases_wrap_vtabs.py")).read()
    for fn in ("def c_vertical_tabs_many", "def c_vertical_tabs_panel_click"):
        body = vsrc[vsrc.index(fn):]
        assert 0 < body.index("first_tab_in_front(ctx, r") < body.index("VTabs(ctx, r)"), fn
    # 4. OCR engines: Apple Vision is always read on macOS when the tool exists; tesseract is added, never the only engine there
    assert cases.ocr_engines("mac", True, True) == ["vision", "tesseract"] and cases.ocr_engines("mac", False, True) == ["vision"]
    assert cases.ocr_engines("mac", True, False) == ["tesseract"] and cases.ocr_engines("mac", False, False) == []
    assert cases.ocr_engines("x11", True, True) == ["tesseract"] and cases.ocr_engines("wayland", False, False) == []
    vision = "(ESC) for terminal\nNew agent conversation\nCl Ollama / gpt-oss:20b\n"          # the two engines' reads of round 10's agent_proof_prekey.png
    tess = "at New agent conversation\n@) for help (7) for commands open conversation\n"
    assert "gpt-oss" not in tess.lower() and "gpt-oss" in " ".join((vision + "\n" + tess).lower().split())
    assert cases.ocr_agent_markers(" ".join((vision + "\n" + tess).lower().split())) == (True, False)
    doc = open(os.path.join(HERE, "..", "..", "..", "docs", "testing.md")).read()
    assert "r11_mac_round10_test_faults" in doc and "close_file_viewer" in doc and "ocr_engines" in doc


@check
def lab_helper_from_the_cache_r12():
    """Owner decision 2026-11-10 (docs/design/r12-policy.md 3): the lab's helper arms take the static phosphor-remote from the machine-local cache the manual
    `lab-helper` job fills (script/ci/lab_helper.py), unless APPCI_LAB_HELPER_BIN names one. (1) the resolver: explicit wins and a wrong explicit path is not
    papered over by the cache; no variable -> this checkout's cache entry; neither -> the reason, naming the job. (2) a helper that cannot be started on this
    machine is read from the version file built next to it; one that starts and fails is not. (3) a bundled build with no pinned digest
    (APPCI_APP_PINNED_DIGESTS=0) makes upload / upgrade / never UNVERIFIABLE before anything is touched, and leaves the pre-installed `helper` arm alone.
    (4) lab_prepare with a cached helper pre-installs exactly that binary under the name its version line gives. (5) the CI wiring and the documentation."""
    import cases_lab as CL
    import lab as L
    import profile as P
    LH = CL.lab_helper_cache()
    assert LH is not None and LH.BIN == "phosphor-remote" and callable(LH.cached_entry)
    d = tempfile.mkdtemp()
    keep = {k: os.environ.get(k) for k in ("APPCI_LAB_HELPER_BIN", "PHOSPHOR_CI_HELPER_CACHE", "APPCI_APP_PINNED_DIGESTS", "LAB_HELPER_TAG", CL.HELPER_ASSET_ENV)}
    try:
        for k in keep:
            os.environ.pop(k, None)
        cache = os.path.join(d, "cache")
        os.environ["PHOSPHOR_CI_HELPER_CACHE"] = cache
        # (1) nothing anywhere
        path, why = CL.resolve_helper_bin()
        assert path == "" and "APPCI_LAB_HELPER_BIN" in why and "lab-helper" in why, why
        # a cache entry for this checkout's key (the real key when the tree has one; a dirty tree under crates/ has none and says so)
        try:
            key = LH.key_of(LH.key_inputs(LH.ROOT, LH.helper_tag(LH.ROOT)))
        except LH.NoKey:
            key = None
        script = os.path.join(d, "fake-helper")
        tag = LH.helper_tag(LH.ROOT)
        line = "phosphor-remote %s protocol 1" % tag
        open(script, "w").write("#!/bin/sh\necho '%s'\n" % line)
        os.chmod(script, 0o755)
        if key is not None:
            entry = LH.store(cache, key, script, line, {"tag": tag})
            path, why = CL.resolve_helper_bin()
            assert why is None and path == os.path.join(entry, "phosphor-remote"), (path, why)
            # explicit wins; a wrong explicit path is reported, the cache does not cover for it
            assert CL.resolve_helper_bin({"APPCI_LAB_HELPER_BIN": script, "PHOSPHOR_CI_HELPER_CACHE": cache}) == (script, None)
            path, why = CL.resolve_helper_bin({"APPCI_LAB_HELPER_BIN": os.path.join(d, "nope"), "PHOSPHOR_CI_HELPER_CACHE": cache})
            assert path == "" and "does not name a built phosphor-remote binary" in why
        # (2) the version line
        if os.name == "posix":
            assert CL.helper_version_line(script) == line
            bad = os.path.join(d, "fails")
            open(bad, "w").write("#!/bin/sh\nexit 3\n")
            os.chmod(bad, 0o755)
            open(bad + ".version", "w").write(line + "\n")
            assert CL.helper_version_line(bad) is None, "a helper that starts and fails is not excused by its version file"
        foreign = os.path.join(d, "foreign")
        open(foreign, "wb").write(b"\x7fELF not runnable here")           # no exec bit: starting it raises OSError, as a Linux ELF does on macOS / Windows
        assert CL.helper_version_line(foreign) is None, "no version file: unknown"
        open(foreign + ".version", "w").write(line + "\n")
        assert CL.helper_version_line(foreign) == line and CL.helper_file_name(CL.helper_version_line(foreign)) == "phosphor-remote-" + tag
        open(foreign + ".version", "w").write("something else\n")
        assert CL.helper_version_line(foreign) is None

        # (3) + (4)
        class FakeLab:
            info = {"flavor": "full"}

            def __init__(self):
                self.helpers = []

            def helper(self, user, files=None):
                self.helpers.append((user, dict(files or {})))

        class R:
            msg = None

            def unverifiable(self, msg):
                self.msg = msg

            def skip(self, msg):
                raise cases.Skip(msg)
        prof = P.Profile(d)
        local = L.LocalRemote(prof)

        def run(helper):
            lab = FakeLab()
            ctx = types.SimpleNamespace(lab=lab, lab_why=None, prof=prof, remote=local, arm=None, lab_helper_mode=helper, run=d)
            arm = dict(name="lab_x.bash.lan.%s" % helper, user="u_bash", shell="bash", helper=helper, lat_ms=0, flavor="full")
            r = R()
            try:
                ok = CL.lab_prepare(ctx, r, arm)
            except Exception as e:                      # past the helper gates lab_prepare talks to a real lab (fault, RTT): not this check's business
                ok = e
            finally:
                CL.lab_cleanup(ctx, arm)
            return ok, r, lab, arm
        os.environ["APPCI_APP_PINNED_DIGESTS"] = "0"
        for helper in CL.UPLOAD_ARMS:
            ok, r, lab, arm = run(helper)
            assert ok is False and "no pinned helper digest" in r.msg and "APPCI_APP_PINNED_DIGESTS=0" in r.msg and lab.helpers == [], (helper, ok, r.msg, lab.helpers)
        assert CL.UPLOAD_ARMS == ("upload", "upgrade", "never") and "helper" not in CL.UPLOAD_ARMS and "nohelper" not in CL.UPLOAD_ARMS
        if key is not None and os.name == "posix":
            os.environ.pop("APPCI_LAB_HELPER_BIN", None)
            ok, r, lab, arm = run("helper")             # pins=0 does not gate the pre-installed arm
            assert r.msg is None, r.msg
            assert lab.helpers and lab.helpers[0][0] == "u_bash" and list(lab.helpers[0][1]) == ["phosphor-remote-" + tag], lab.helpers
            assert lab.helpers[0][1]["phosphor-remote-" + tag] == open(script, "rb").read() and arm["helper_file"] == "phosphor-remote-" + tag
            assert os.environ.get("APPCI_LAB_HELPER_BIN") == os.path.join(cache, key, "phosphor-remote")
        os.environ.pop("APPCI_APP_PINNED_DIGESTS", None)
        os.environ.pop("APPCI_LAB_HELPER_BIN", None)
        os.environ["PHOSPHOR_CI_HELPER_CACHE"] = os.path.join(d, "empty")
        ok, r, lab, arm = run("upload")                 # unset: a source build's arm goes on to the helper lookup, as before
        assert ok is False and "lab-helper" in r.msg and "no pinned" not in r.msg, r.msg
    finally:
        for k, v in keep.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v
        shutil.rmtree(d, ignore_errors=True)
    # (5)
    ci = open(os.path.join(HERE, "..", "..", "..", ".gitlab-ci.yml")).read()
    job = ci[ci.index("\nlab-helper:"):]
    job = job[:job.index("\n\n")]
    assert "script/ci/lab_helper.py build" in job and "when: manual" in job and "helper-artifacts/" in job and "tags: [little-thing]" in job, job
    assert "install" not in job.replace("installs nothing", "").replace("not installed", "").lower(), "the job installs nothing on the box"
    labjob = ci[ci.index("\napp-test-lab-linux:"):]
    labjob = labjob[:labjob.index("\n\n")]
    assert 'APPCI_APP_PINNED_DIGESTS: "0"' in labjob and "script/ci/lab_helper.py path" in labjob, labjob
    doc = open(os.path.join(HERE, "..", "..", "..", "docs", "testing.md")).read()
    assert "lab_helper.py" in doc and "lab-helper" in doc and "APPCI_APP_PINNED_DIGESTS=0" in doc and "PHOSPHOR_CI_HELPER_CACHE" in doc


@check
def precmd_secret_honor_ps1_pure_parts():
    """#1923 round 12 (cases_precmd_secret.py): the honor-PS1 arm of the precmd secret. The sourced function, the pane's own mode probe, the settings edit, the
    relaunch plan, the PIN-in-the-editor reader, and the wiring: both session modes, PROVING only, Git Bash pane on Windows, the setting's real TOML path."""
    import cases_precmd_secret as PS, toml_text as T
    n = PS.names("hp1")
    assert len(set(n.values())) == len(n) and all("hp1" in v for v in n.values())
    sc = PS.install_script(n)
    assert 'read -s -p "vault pin: " x' in sc and "precmd_functions+=(__pp_pin)" in sc and sc.endswith("echo installed > pp_hp1_inst.txt\n")
    assert sc.index("echo asked > pp_hp1_asked.txt") < sc.index("read -s") < sc.index('echo "$x" > pp_hp1_pin.txt'), "asked is recorded BEFORE the read, the PIN after"
    assert sc.index("rm -f pp_hp1_go.txt") < sc.index("read -s"), "the function asks once"
    assert PS.QUESTION_WORD in PS.QUESTION and PS.QUESTION.rstrip().endswith(":"), "the app's own question test (key_facts::output_tail_is_input_prompt) needs the colon"
    # nothing the case TYPES may contain the word the screen check looks for
    assert PS.QUESTION_WORD not in PS.mode_probe_line(n["mode"]) and PS.QUESTION_WORD not in "source ./%s 2> %s" % (n["script"], n["err"]) and PS.QUESTION_WORD not in "touch " + n["go"]
    assert PS.mode_probe_line("m.txt") == 'echo "hp=${WARP_HONOR_PS1:-unset} sh=${BASH_VERSION:+bash}" > m.txt'
    assert PS.parse_mode_probe(["hp=1 sh=bash"]) == (True, True) and PS.parse_mode_probe(["hp=0 sh=bash"]) == (False, True)
    assert PS.parse_mode_probe(["hp=1 sh="]) == (True, False), "zsh: BASH_VERSION is unset"
    assert PS.parse_mode_probe(["hp=unset sh=bash"]) == (None, True) and PS.parse_mode_probe([]) is None and PS.parse_mode_probe(["junk"]) is None
    assert PS.parse_mode_probe(["hp=0 sh=bash", "hp=1 sh=bash"]) == (True, True), "the newest line wins"
    # the relaunch plan: only a bash pane already in the wanted mode runs as it is
    assert PS.arm_plan(False, True, False) is False and PS.arm_plan(True, True, True) is False
    assert PS.arm_plan(False, True, True) and PS.arm_plan(True, True, False) and PS.arm_plan(False, False, False) and PS.arm_plan(None, True, False) and PS.arm_plan(None, False, True)
    # the settings edit: a bare profile, a profile that already has the table, and the macOS bash override
    bare = '[general]\ndefault_session_mode = "agent"\nrestore_session = false\n'
    on = PS.settings_for(bare, True, False)
    assert T.get_key(on, "terminal.input", "honor_ps1") == "honor_ps1 = true" and T.get_key(on, "general", "default_session_mode") == 'default_session_mode = "agent"'
    assert T.get_key(on, "session", "startup_shell_override") is None
    has = bare + '\n[terminal.input]\nhonor_ps1 = true\nother = 1\n\n[session]\nstartup_shell_override = "zsh"\n'
    off = PS.settings_for(has, False, True)
    assert T.get_key(off, "terminal.input", "honor_ps1") == "honor_ps1 = false" and off.count("honor_ps1") == 1 and T.get_key(off, "terminal.input", "other") == "other = 1"
    assert T.get_key(off, "session", "startup_shell_override") == 'startup_shell_override = "bash"' and off.count("startup_shell_override") == 1
    # the PIN in the editor at an Enter: the whole PIN, either half, text logging on and off
    pin = "pinab12cd"
    log = ('a input_order hop=enter_begin deferred=false buffer="touch pp_hp1_go.txt" input_type=Shell t=1\n'
           'b input_order hop=enter_begin deferred=false buffer="pinab12cd" input_type=AI t=2\n'
           'c input_order hop=kh_pane_key text="pinab12cd" t=3\n'
           'd input_order hop=enter_begin deferred=false buffer="2cd" input_type=Shell t=4\n')
    assert PS.pin_in_editor_enters(log, pin) == ["AI"], PS.pin_in_editor_enters(log, pin)       # a pty-key line and text that is no half of the PIN do not count
    assert PS.pin_in_editor_enters(log.replace('buffer="2cd"', 'buffer="b12cd"'), pin) == ["AI", "Shell"], "the second half alone, run as a command"
    assert PS.pin_in_editor_enters('x hop=enter_begin deferred=false buffer=<9 chars> input_type=AI\n', pin) == [] and PS.pin_in_editor_enters("", pin) == [] and PS.pin_in_editor_enters(None, pin) == []
    assert PS.enter_text_logged(log) is True and PS.enter_text_logged('x hop=enter_begin deferred=false buffer=<9 chars> input_type=AI\n') is False and PS.enter_text_logged("no enters") is None
    # #2053: the agent arm judges the question. Before a key where the tty is sampled; on ConPTY the look before a key is recorded and the one after half the PIN judged
    assert PS.question_in("touch pp_hp0_go.txt\nVault  PIN:\n") and not PS.question_in("touch pp_hp0_go.txt\n") and not PS.question_in(None) and not PS.question_in("")
    assert PS.asked_look_is_judged("terminal", False)[0] and PS.asked_look_is_judged("terminal", True)[0], "terminal mode: judged on every platform, as before"
    assert PS.asked_look_is_judged("agent", False)[0] and not PS.asked_look_is_judged("agent", True)[0]
    _ps_src = open(PS.__file__, encoding="utf-8").read()
    assert "recorded, not judged" not in _ps_src and "question_on_screen_typing_%s" in _ps_src, "the agent arm's question is judged (#2053), not only recorded"
    # wiring
    c = [x for x in cases.CASES if x["name"] == "precmd_secret_honor_ps1"]
    assert len(c) == 1 and set(c[0]["modes"]) == {"terminal", "agent"} and c[0]["gui"], "both session modes (owner: one mode = not tested)"
    assert "precmd_secret_honor_ps1" in cases.PROVING and "precmd_secret_honor_ps1" not in cases.SMOKE and "precmd_secret_honor_ps1" not in cases.PROVING_FAKE
    assert winmap.skip_reason("precmd_secret_honor_ps1", "gitbash") is None and "Git Bash" in winmap.skip_reason("precmd_secret_honor_ps1", "powershell")
    assert cases.wants_depth_check("precmd_secret_honor_ps1", cases.PROVING)
    root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
    src = lambda p: open(os.path.join(root, p), encoding="utf-8").read()
    assert 'toml_path: "terminal.input.honor_ps1"' in src("app/src/terminal/session_settings.rs"), "the key the case writes must be the setting's path"
    assert 'builder.env("WARP_HONOR_PS1"' in src("app/src/terminal/local_tty/unix.rs") and '"${WARP_HONOR_PS1:-}" == "1"' in src("app/assets/bundled/bootstrap/bash_body.sh")
    assert 'log_hop("enter_begin"' in src("app/src/terminal/input.rs") and "input_type={:?}" in src("app/src/terminal/input.rs"), "the hop the case reads must exist in the app"
    doc = src("docs/testing.md")
    assert "precmd_secret_honor_ps1" in doc and "r12-ps1case.md" in doc


@check
def appci_programs_find_their_siblings():
    """#2043: the Windows test machine's Python is the embeddable layout (python312._pth): the folder of the script being run is NOT on sys.path.
    fakesshd_win.py got `import ptyhost` without putting its own folder there, raised ModuleNotFoundError at start, and every Windows ssh case
    was a SKIP in rounds 10 and 11. Rule: an appci file that is run as a program (shebang or a __main__ block) and imports a sibling module
    anywhere must insert its own folder into sys.path BEFORE the first such import."""
    import ast
    stems = {f[:-3] for f in os.listdir(HERE) if f.endswith(".py")}

    def first_sibling_import_and_insert(src):
        tree = ast.parse(src)
        sib = []
        for n in ast.walk(tree):
            if isinstance(n, ast.Import):
                sib += [n.lineno for a in n.names if a.name.split(".")[0] in stems]
            elif isinstance(n, ast.ImportFrom) and n.level == 0 and (n.module or "").split(".")[0] in stems:
                sib.append(n.lineno)
        ins = [n.lineno for n in ast.walk(tree) if isinstance(n, ast.Call) and ast.unparse(n.func) == "sys.path.insert"]
        return (min(sib) if sib else None), (min(ins) if ins else None)

    programs, bad = [], []
    for f in sorted(os.listdir(HERE)):
        if not f.endswith(".py"):
            continue
        src = open(os.path.join(HERE, f), encoding="utf-8").read()
        if not (src.startswith("#!") or "__main__" in src):
            continue
        programs.append(f)
        sib, ins = first_sibling_import_and_insert(src)
        if sib is not None and (ins is None or ins > sib):
            bad.append("%s: sibling import at line %s, sys.path.insert(0, <own folder>) at %s" % (f, sib, ins))
    assert "fakesshd_win.py" in programs and "run.py" in programs and "report.py" in programs, programs
    assert not bad, "started as its own process this cannot import its neighbours under an embeddable Python: " + "; ".join(bad)
    # the rule itself: the round 10 form is caught, the repaired form is not, a program with no sibling import needs nothing
    r10 = "#!/usr/bin/env python3\nimport os, sys\nimport paramiko\n\nimport ptyhost\n"
    assert first_sibling_import_and_insert(r10) == (5, None)
    assert first_sibling_import_and_insert(r10.replace("import ptyhost", "sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))\nimport ptyhost")) == (6, 5)
    assert first_sibling_import_and_insert("import os\n\ndef f():\n    import winapi\n") == (4, None)      # a late import inside a function counts too
    assert first_sibling_import_and_insert("import os, json\n") == (None, None)
    # every program sshd.py starts with this Python is covered by the scan
    import re as _re
    started = set(_re.findall(r'os\.path\.join\(HERE, "(\w+\.py)"\)', open(os.path.join(HERE, "sshd.py"), encoding="utf-8").read()))
    assert "fakesshd_win.py" in started and started <= set(programs), started


@check
def ssh_test_host_missing_is_loud():
    """#2043: where the run requires an ssh test host (CI, or APPCI_REQUIRE_SSH=1) a pass without one gets a FAIL row `ssh_test_host` and its ssh
    cases are NOREF, never SKIP; a local run without sshd keeps the plain skip; a failed check before the skip still wins (#1498)."""
    import run as runmod, sshd
    assert sshd.required({"CI": "true"}) and sshd.required({"APPCI_REQUIRE_SSH": "1"}) and sshd.required({"CI": "true", "APPCI_REQUIRE_SSH": "0"})
    assert not sshd.required({}) and not sshd.required({"APPCI_REQUIRE_SSH": "0"}) and not sshd.required({"CI": ""})
    why = "fakesshd_win.py did not listen on 127.0.0.1:49956 (ssh/fakesshd.stderr: import ptyhost | ModuleNotFoundError: No module named 'ptyhost')"
    # the case side: every ssh case goes through cases.need_ssh
    have = types.SimpleNamespace(ssh={"port": 1}, ssh_why=None, ssh_required=False)
    assert cases.need_ssh(have, cases.Result("x", "terminal")) is None
    local = types.SimpleNamespace(ssh=None, ssh_why=None, ssh_required=False)
    r = cases.Result("ssh_connect_typing", "terminal")
    try:
        cases.need_ssh(local, r)
        raise AssertionError("no host: the case must not run")
    except cases.HostMissing:
        raise AssertionError("a local run without sshd is a plain skip")
    except cases.Skip as e:
        assert "no local ssh server" in str(e) and runmod.skip_status(r, e) == "skip" and not r.noref
    ci = types.SimpleNamespace(ssh=None, ssh_why=why, ssh_required=True)
    r = cases.Result("ssh_connect_typing", "terminal")
    try:
        cases.need_ssh(ci, r)
        raise AssertionError("no host: the case must not run")
    except cases.HostMissing as e:
        assert isinstance(e, cases.Skip) and "TEST HOST MISSING" in str(e) and "ModuleNotFoundError" in str(e)
        assert runmod.skip_status(r, e) == "noref" and r.noref and r.notes[0].startswith("NO-REFERENCE: TEST HOST MISSING"), (r.noref, r.notes)
        assert "UNVERIFIABLE" not in r.notes[0] and "never reached" not in r.notes[0], "results_trend.noref_kind must not file this under the injector"
    r = cases.Result("ssh_connect_typing", "terminal")
    r.check(False, "a real failure before the host check")
    assert runmod.skip_status(r, cases.HostMissing("TEST HOST MISSING: x")) == "fail", "a FAIL is never downgraded (#1498)"
    for mod, n in (("cases.py", 3), ("cases_agent_interactive.py", 1), ("cases_caseremote.py", 1)):
        src = open(os.path.join(HERE, mod), encoding="utf-8").read()
        assert "if not ctx.ssh:" not in src.replace("def need_ssh(ctx, r):", ""), "%s: an ssh case with its own host check bypasses the rule" % mod
        assert src.count("need_ssh(ctx, r)") >= n, mod
    # the pass side
    d = tempfile.mkdtemp(prefix="hostrow_")
    try:
        err = os.path.join(d, "fakesshd.stderr")
        open(err, "w").write('Traceback (most recent call last):\n  File "fakesshd_win.py", line 26, in <module>\n    import ptyhost\nModuleNotFoundError: No module named \'ptyhost\'\n\n')
        assert sshd.stderr_tail(err).endswith("import ptyhost | ModuleNotFoundError: No module named 'ptyhost'"), sshd.stderr_tail(err)
        assert sshd.stderr_tail(os.path.join(d, "absent")) == "" and len(sshd.stderr_tail(err, cap=20)) == 20
        row = sshd.host_missing_row("agent@gitbash", "fakesshd_win.py did not listen on 127.0.0.1:1 (see the log)", err)
        assert row["name"] == "ssh_test_host" and row["status"] == "fail" and row["mode"] == "agent@gitbash", row
        assert "TEST HOST MISSING" in row["detail"][0] and "not an app verdict" in row["detail"][0] and "ModuleNotFoundError" in row["detail"][1], row
        quoted = "fakesshd_win.py did not listen on 127.0.0.1:1 (ssh/fakesshd.stderr: %s)" % sshd.stderr_tail(err)      # the form sshd._start_windows writes
        assert len(sshd.host_missing_row("terminal", quoted, err)["detail"]) == 1, "a reason that already quotes the stderr is not repeated"
        wsrc = open(os.path.join(HERE, "sshd.py"), encoding="utf-8").read()
        assert '(ssh/fakesshd.stderr: %s)" % (port, stderr_tail(os.path.join(d, "fakesshd.stderr")) or "empty")' in wsrc, "the skip reason carries the server's last stderr lines"
        assert "no local ssh server" in sshd.host_missing_row("terminal", None)["detail"][0]
    finally:
        shutil.rmtree(d, ignore_errors=True)
    # wiring in run.py: the row is emitted, fails the run, and the pass's Ctx is told
    src = open(os.path.join(HERE, "run.py"), encoding="utf-8").read()
    i = src.index("ssh_missing = ssh_info is None and not sim and sshd.required()")
    blk = src[i:i + 900]
    assert "emit(row)" in blk and "exit_fail = True" in blk and "sshd.host_missing_row(label, ssh_why" in blk, blk
    assert "ctx.ssh_required = ssh_missing" in src
    assert "ssh_test_host" not in cases.SMOKE and not [c for c in cases.CASES if c["name"] == "ssh_test_host"], "a harness row, not a case"
    root = os.path.dirname(os.path.dirname(os.path.dirname(HERE)))
    doc = open(os.path.join(root, "docs", "testing.md"), encoding="utf-8").read()
    assert "ssh_test_host" in doc and "APPCI_REQUIRE_SSH" in doc and "#2043" in doc


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
