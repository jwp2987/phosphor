# appci/selftest_r31_2124.py: offline self-tests of #2124 (the agent_remote_drop where-probe runs only on a failed check) and #2206 (the pane state the app
# logged is carried into the gitbash managed-ssh restore verdict). Pure Python, no display.
import os, sys, inspect

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


@check
def r31_drop_where_probe_only_on_failure_2124():
    import cases_agent_remote as AR
    src = inspect.getsource(AR.c_agent_remote_drop) if hasattr(AR, "c_agent_remote_drop") else open(os.path.join(HERE, "cases_agent_remote.py")).read()
    assert '% (ctx.where_now(),))' not in src, "the where-probe fills the message even on a pass"
    assert 'ctx.where_now() if not is_local else "local"' in src
    assert 'ctx.where_now() == "local", 20' in src      # the check itself is kept


@check
def r31_pane_state_from_log_2206():
    import cases_caseremote as R
    a = "x hop=hand_route reason=unheld state=pty_owned awaiting_ready=false in_flight=0 evidence=None command_interactive=false enters_written=1 precmds_seen=2 bytes=\"e\""
    b = "x hop=hand_route reason=unheld state=pty_owned awaiting_ready=false in_flight=0 evidence=None command_interactive=true enters_written=3 precmds_seen=0 bytes=\"e\""
    assert R.pane_state_from_log(a + "\nother\n" + b + "\n") == "command_interactive=true enters_written=3 precmds_seen=0"
    assert R.pane_state_from_log(a) == "command_interactive=false enters_written=1 precmds_seen=2"
    assert R.pane_state_from_log("nothing here") == "no hand_route line"
    assert R.pane_state_now(object()) == "unknown"


@check
def r31_no_tcp_accept_verdict_carries_pane_state_2206():
    import cases
    class C: pass
    c = C()
    assert cases._pane_state_note(c) == ""
    c.pane_state_at_case_start = "command_interactive=true enters_written=3 precmds_seen=0"
    n = cases._pane_state_note(c)
    assert "at case start: command_interactive=true enters_written=3 precmds_seen=0" in n and "now: unknown" in n, n


@check
def r31_soak_rss_drops_1780():
    import cases_robust as CR
    r18u23 = [300, 340, 350] + [359] * 5 + [364] * 5 + [365] * 5 + [157] * 5 + [133] * 5 + [141] * 5
    d = CR.rss_drops(r18u23)
    assert d and d[0][1:] == (365, 157), d
    assert CR.rss_drops([300, 340, 350] + [369] * 5 + [378] * 5 + [383] * 5) == []
    assert CR.rss_drops([1, 2]) == []


@check
def r31_display_matrix_2078():
    import baseline, run
    # the meta of a run carries the display it used; a scaled or two-output run is not a baseline
    old = {k: os.environ.get(k) for k in ("APPCI_DISPLAY_SCALE", "APPCI_OUTPUTS")}
    try:
        os.environ["APPCI_DISPLAY_SCALE"], os.environ["APPCI_OUTPUTS"] = "1.5", "2"
        cmd = run.report_cmd("/x", type("A", (), dict(platform="linux", backend="b", app_display="d", seed=1))(), "n")
        assert "display_scale=1.5" in cmd and "outputs=2" in cmd and "scale=1" in cmd, cmd
        os.environ.pop("APPCI_DISPLAY_SCALE"); os.environ.pop("APPCI_OUTPUTS")
        cmd = run.report_cmd("/x", type("A", (), dict(platform="linux", backend="b", app_display="d", seed=1))(), "n")
        assert "display_scale=1" in cmd and "outputs=1" in cmd
    finally:
        for k, v in old.items():
            if v is None: os.environ.pop(k, None)
            else: os.environ[k] = v
    doc = lambda **m: {"meta": dict({"tier": "full", "scale": "1"}, **m), "summary": {"fail": 0}, "cases": []}
    assert not any("display_scale" in w for w in baseline.eligibility(doc()))
    assert any("display_scale=2" in w for w in baseline.eligibility(doc(display_scale="2", outputs="1")))
    assert any("outputs=2" in w for w in baseline.eligibility(doc(display_scale="1", outputs="2")))
    # the session script validates and applies both knobs, and refuses a scale it cannot set instead of running at 1
    sh = open(os.path.join(HERE, "..", "linux", "wayland_session.sh")).read()
    assert "scale ${DSCALE}" in sh and "Xft.dpi" in sh and "create_output" in sh and "exit 2" in sh and "APPCI_SECOND_OUTPUT_SCALE" in sh


@check
def r31_display_matrix_vars_pass_a_targeted_run_2078():
    sys.path.insert(0, os.path.join(HERE, "..", "..", "ci"))
    import ci_seq
    v = ci_seq.targeted_vars("linux-wayland", {"cases": ["scale_matrix"], "vars": {"APPCI_DISPLAY_SCALE": "1.5", "APPCI_SECOND_OUTPUT_SCALE": "2"}})
    assert v["APPCI_DISPLAY_SCALE"] == "1.5" and v["APPCI_SECOND_OUTPUT_SCALE"] == "2", v


@check
def r31_replay_jobs_on_mac_and_windows_2063():
    root = os.path.join(HERE, "..", "..", "..")
    yml = open(os.path.join(root, ".gitlab-ci.yml")).read()
    for job, ext, win in (("app-test-replay-macos", "app-test-lab-macos", False), ("app-test-replay-windows", "app-test-lab-windows", True)):
        i = yml.index("\n%s:\n" % job)
        blk = yml[i + 1:].split("\n\n", 1)[0]
        assert "extends: %s" % ext in blk and 'APPCI_CASES: "lab_replay*"' in blk and 'APPCI_FAKE_MODEL: "1"' in blk, blk
        assert ('APPCI_WIN_SHELLS: "gitbash"' in blk) == win, blk
    sys.path.insert(0, os.path.join(HERE, "..", "..", "ci"))
    import ci_seq
    assert ci_seq.APP_RESULT_DIR["app-test-replay-macos"] == "macos" and ci_seq.APP_RESULT_DIR["app-test-replay-windows"] == "windows"
    assert ci_seq.is_lab_job("app-test-replay-macos") and ci_seq.is_lab_job("app-test-replay-windows")


@check
def r31_phosphor_scheme_handler_1546():
    import cases, cases_install as CI
    assert CI.scheme_url("phosphor", "new_tab", "/a b/c") == "phosphor://action/new_tab?path=/a%20b/c"
    assert CI.scheme_url("warp", "x") == "warp://action/x"
    assert CI.tab_dir_line("bash", "t1", "/f") == 'echo "t1:$PWD" >> /f' and CI.tab_dir_line("pwsh", "t1", "C:/f").startswith("Add-Content C:/f ")
    assert CI.INVALID_URI_RE.search("WARN Custom URI is invalid: Received url with unexpected scheme: warp")
    assert CI.received_urls("received url scheme=phosphor host=action path=/new_tab") == [("phosphor", "action", "/new_tab")]
    names = [c["name"] for c in cases.CASES]
    assert names.count("phosphor_scheme_handler") == 1 and "phosphor_scheme_handler" in cases.PROVING


@check
def r31_wave5_targeted_file_is_valid_2076():
    import json, cases
    sys.path.insert(0, os.path.join(HERE, "..", "..", "ci"))
    import ci_seq
    d = json.load(open(os.path.join(HERE, "..", "..", "ci", "targeted", "wave5-never-played.json")))
    names = {c["name"] for c in cases.CASES}
    missing = [n for p in d["platforms"].values() for n in p["cases"] if n not in names]
    assert not missing, "not in the registry: %s" % missing
    assert set(ci_seq.targeted_spec(d)["apps"]) == {"linux-wayland", "macos", "windows"}
    assert not [n for p in d["platforms"].values() for n in p["cases"] if n in ("soak", "scale_matrix") or n.startswith("lab_")]
