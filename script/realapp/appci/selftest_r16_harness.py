# appci/selftest_r16_harness.py: offline self-tests of round 16's harness / CI decisions (owner 2026-11-11; docs/design/r10-r16-harness.md):
# sudo_su_real_shell passes only on an arm that ran and passed, the macOS close key is given only by a proof that passed in the same run, the lab jobs'
# variables (fake embeddings, the PROVING_LAB globs on macOS / Windows in the candidate tier), the ssh wrapper pinned on macOS, the never-played lab
# families' one fix-tier run (MOVE-AFTER-ONE-RUN), and the guards job naming the #2086 / #2089 / #2090 checks. (latency_key_log judged on the key -> pty
# write share is in selftest.py, next to the macOS check it extends.) Pure Python: no app, display, sudo or network.
# selftest.py appends CHECKS to its own list (two lines at its end), so `run.py --self-test` and `selftest.py` both run these.
# Every judge is fed a right input AND the wrong ones it exists to catch.
import importlib.util, inspect, os, re, sys, types

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
sys.path.insert(0, HERE)
import cases  # noqa: E402
import cases_coverage_a as cov  # noqa: E402
import cases_macptr as CM  # noqa: E402
import cases_robust as R  # noqa: E402
import lab as L  # noqa: E402

CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


def _read(*parts):
    return open(os.path.join(ROOT, *parts), encoding="utf-8").read()


def _ci_seq():
    """script/ci/ci_seq.py as a module (it imports no harness code)"""
    spec = importlib.util.spec_from_file_location("appci_r16_ci_seq", os.path.join(ROOT, "script", "ci", "ci_seq.py"))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def _job(yml, name):
    m = re.search(r"^%s:\n(?:[ \t].*\n|\n)*" % re.escape(name), yml, re.M)
    return m.group(0) if m else ""


@check
def r16_sudo_su_passes_only_on_an_arm_that_ran_and_passed():
    """owner 2026-11-11: NOT RUN notes are fine, but sudo_su_real_shell PASSES only if at least one arm actually ran and passed; no arm run is a SKIP
    whose text starts with the NEEDS: machine change (never a pass)."""
    needs = cov.PB.sudo_needs(ci_user="ci", platform="linux")
    assert needs.startswith("NEEDS: ")
    nr = ["sudo: `sudo -n true` asks for a password", "su: su asks for a password", "ksu: not installed"]
    # the pure rule
    v, text = cov.priv_arms_verdict([], [], nr, needs)
    assert v == "skip" and text.startswith("NEEDS: a sudoers entry") and "No privilege boundary could run here" in text and nr[0] in text and nr[1] in text, text
    assert cov.priv_arms_verdict(["sudo:root"], ["sudo:root"], nr[1:], needs) == ("pass", None)
    assert cov.priv_arms_verdict(["sudo:root", "su:nobody"], ["su:nobody"], nr[2:], needs) == ("pass", None), "one passing arm is enough for this rule (the failed arm has its own checks)"
    v, text = cov.priv_arms_verdict(["sudo:root", "su:nobody"], [], nr[2:], needs)
    assert v == "fail" and "sudo:root, su:nobody ran and every one of them failed" in text, text
    assert cov.priv_arms_verdict(["sudo:root"], ["su:nobody"], [], needs)[0] == "fail", "an arm that did not run cannot be the one that passed"

    # the case itself, with the machine and the pane replaced: which arms can run, and what the inner shell answers
    def play(sudo_ok, su_ok, inner, fail_in=()):
        """inner: {arm: bool} what nested_root_lines returns (the `id -u` proof); fail_in: arms whose body records a failed check"""
        ctx = types.SimpleNamespace(win=False, W="/w", count=lambda n: n)
        r = cases.Result("sudo_su_real_shell", "terminal")
        keep = (cov.PB.sudo_target, cov.PB.shared_dir, cov.nested_root_lines, cov.su_plan, cov.PB.sudo_needs)
        cleaned = []

        def body(ctx_, r_, label, cmd, n, uid, d=None):
            if label in fail_in:
                r_.check(False, "%s: a typed line was lost" % label)
            return inner.get(label, True)
        cov.PB.sudo_target = lambda: dict(ok=sudo_ok, user="root", uid=0, home="/root", root=True, flags="-n", explicit=False, why="`sudo -n true` asks for a password")
        cov.PB.shared_dir = lambda w, uid, label: ("/shared/" + label, lambda: cleaned.append(label))
        cov.nested_root_lines = body
        cov.su_plan = lambda *a: dict(run=su_ok, cmd="sudo -n su -s /bin/bash nobody", user="nobody", uid=65534, why="su asks for a password")
        cov.PB.sudo_needs = lambda *a, **k: needs
        try:
            cov.c_sudo_su(ctx, r)
            how = "fail" if r.fails else "pass"
        except cases.Skip as e:
            how, r.skipped = ("fail" if r.fails else "skip"), str(e)
        finally:
            cov.PB.sudo_target, cov.PB.shared_dir, cov.nested_root_lines, cov.su_plan, cov.PB.sudo_needs = keep
        return how, r, cleaned
    how, r, cleaned = play(True, True, {})
    assert how == "pass" and r.metrics["boundaries_run"] == ["sudo:root", "su:nobody"] == r.metrics["boundaries_passed"] and cleaned == ["sudo", "su"], (how, r.metrics)
    assert r.metrics["boundaries_not_run"] == ["ksu"] and [n for n in r.notes if n.startswith("NOT RUN: ksu")], "ksu is a NOT RUN note, and a pass all the same"
    how, r, _ = play(True, False, {})
    assert how == "pass" and r.metrics["boundaries_passed"] == ["sudo:root"] and r.metrics["boundaries_not_run"] == ["su", "ksu"], (how, r.metrics)
    # zero arms: SKIP, the NEEDS: text first, nothing passed
    how, r, cleaned = play(False, False, {})
    assert how == "skip" and r.skipped.startswith("NEEDS: ") and r.metrics["boundaries_run"] == [] == r.metrics["boundaries_passed"] and cleaned == [], (how, r.skipped)
    assert len([n for n in r.notes if n.startswith("NOT RUN: ")]) == 3
    # an arm that ran and failed: FAIL; with a second arm that passed the row is still a FAIL (its own checks), and the passed list says which
    how, r, _ = play(True, False, {}, fail_in=("sudo",))
    assert how == "fail" and r.metrics["boundaries_passed"] == [] and any("no privilege-boundary arm passed" in f for f in r.fails), r.fails
    how, r, _ = play(True, True, {}, fail_in=("sudo",))
    assert how == "fail" and r.metrics["boundaries_passed"] == ["su:nobody"] and not any("no privilege-boundary arm passed" in f for f in r.fails), r.fails
    # the inner shell never was the target user (nested_root_lines -> False): that arm did not pass even if no check were recorded
    how, r, _ = play(True, False, {"sudo": False})
    assert how == "fail" and r.metrics["boundaries_passed"] == [] and any("no privilege-boundary arm passed" in f for f in r.fails), r.fails
    doc = _read("docs", "testing.md")
    assert "boundaries_passed" in doc and "priv_arms_verdict" in doc


@check
def r16_macos_close_key_only_after_the_proof_passed():
    """owner 2026-11-11 (docs/design/r10-r15-w4mac.md): APPCI_KEY_CLOSE becomes cmd+w on macOS ONLY when mac_pointer_chord_proof passed earlier in the
    same run. The job names the key (APPCI_KEY_CLOSE_WHEN_PROVEN) and leaves APPCI_KEY_CLOSE empty; cases_robust.proven_close_key gives it and takes it back."""
    job = _read("script", "realapp", "mac", "ci", "app_test.sh")
    assert 'APPCI_KEY_CLOSE=""' in job and 'export APPCI_KEY_CLOSE_WHEN_PROVEN="${APPCI_KEY_CLOSE_WHEN_PROVEN-cmd+w}"' in job
    assert 'APPCI_KEY_CLOSE="cmd+w"' not in "\n".join(l for l in job.splitlines() if not l.lstrip().startswith("#")), "the job itself never sets the close key"
    assert R.CLOSE_WHEN_PROVEN_ENV == "APPCI_KEY_CLOSE_WHEN_PROVEN"
    keep = R.CLOSE_PANE
    try:
        def fresh(proven):
            R.CLOSE_PANE = ""
            return types.SimpleNamespace(app_chords_proven=proven), {"APPCI_KEY_CLOSE": "", "APPCI_KEY_CLOSE_WHEN_PROVEN": "cmd+w"}
        # the proof passed and Cmd-W closed a tab: the key is given, to the environment (cases.py reads it per call) and to CLOSE_PANE
        be, env = fresh(True)
        assert R.proven_close_key(be, True, env) == "cmd+w" == R.CLOSE_PANE == env["APPCI_KEY_CLOSE"] and be.close_key_from_proof is True
        # the wrong inputs: the case failed (another half), the chord half failed, the chord half never ran: no key
        for proven, passed in ((True, False), (False, True), (None, True), (False, False)):
            be, env = fresh(proven)
            assert R.proven_close_key(be, passed, env) == "" == R.CLOSE_PANE == env["APPCI_KEY_CLOSE"], (proven, passed)
        # the second session mode: its proof starts (nothing proven yet in this pass) and the earlier key is taken back; a failed proof leaves it off
        be, env = fresh(True)
        R.proven_close_key(be, True, env)
        assert R.proven_close_key(be, False, env) == "" == R.CLOSE_PANE == env["APPCI_KEY_CLOSE"] and be.close_key_from_proof is False
        be.app_chords_proven = False
        assert R.proven_close_key(be, False, env) == "" and env["APPCI_KEY_CLOSE"] == ""
        be.app_chords_proven = True
        assert R.proven_close_key(be, True, env) == "cmd+w", "and a second proof that passes gives it again"
        # without the job's variable (Linux, Windows, a hand run) nothing changes, whatever the backend says
        R.CLOSE_PANE = "ctrl+shift+w"
        be, env = types.SimpleNamespace(app_chords_proven=True), {}
        assert R.proven_close_key(be, True, env) == "ctrl+shift+w" == R.CLOSE_PANE and env == {} and not hasattr(be, "close_key_from_proof")
        assert R.proven_close_key(be, False, env) == "ctrl+shift+w" == R.CLOSE_PANE
        # a close key the job set itself is never replaced and never taken back
        R.CLOSE_PANE = "ctrl+shift+w"
        be, env = types.SimpleNamespace(app_chords_proven=True), {"APPCI_KEY_CLOSE": "ctrl+shift+w", "APPCI_KEY_CLOSE_WHEN_PROVEN": "cmd+w"}
        assert R.proven_close_key(be, True, env) == "ctrl+shift+w" and R.proven_close_key(be, False, env) == "ctrl+shift+w" and env["APPCI_KEY_CLOSE"] == "ctrl+shift+w"
    finally:
        R.CLOSE_PANE = keep
    # the proof case: takes the key back first, gives it as its last step and only from its own verdict
    src = inspect.getsource(CM.c_mac_pointer_chord_proof)
    first, last = src.index("R.proven_close_key(ctx.be, False)"), src.index("R.proven_close_key(ctx.be, not r.fails)")
    assert src.index("r.skip(MAC_ONLY)") < first < src.index("_pointer_half(ctx, r)") < src.index("_chord_half(ctx, r)") < src.index("ctx.app.alive()") < last
    assert "be.app_chords_proven = ok" in inspect.getsource(CM._chord_half)
    # the readers see a key given after import: no module holds its own copy of CLOSE_PANE
    for f in sorted(os.listdir(HERE)):
        if f.endswith(".py") and not f.startswith("selftest"):
            assert not re.search(r"^from cases_robust import .*\bCLOSE_PANE\b", open(os.path.join(HERE, f), encoding="utf-8").read(), re.M), "%s copies CLOSE_PANE at import" % f
    doc = _read("docs", "testing.md")
    assert "APPCI_KEY_CLOSE_WHEN_PROVEN" in doc and "proven_close_key" in doc


@check
def r16_never_played_lab_families_wait_one_run():
    """owner 2026-11-11: the never-played lab families stay in PROVING_LAB for ONE fix-tier run and move to the candidate tier after it. Not moved now:
    every named entry is still a PROVING_LAB entry; the marker the coordinator greps for is in cases.py and docs/testing.md and names them; and the
    candidate tier's lab matrix already plays every one of them, so the move loses no arm."""
    todo = cases.LAB_MOVE_AFTER_ONE_RUN
    assert len(todo) == 26 == len(set(todo))
    gone = [g for g in todo if g not in cases.PROVING_LAB]
    assert not gone, "moved out of PROVING_LAB while still listed in LAB_MOVE_AFTER_ONE_RUN (delete the tuple and this check with the move): %s" % gone
    src = _read("script", "realapp", "appci", "cases.py")
    assert len(re.findall(r"^# MOVE-AFTER-ONE-RUN ", src, re.M)) == 1
    block = src[src.index("# #2063 (round 15): the lab families that had never produced a row"):src.index("# MOVE-AFTER-ONE-RUN ")]
    assert re.findall(r'"(lab_[^"]+)"', block) == list(todo), "LAB_MOVE_AFTER_ONE_RUN is not exactly the #2063 block of PROVING_LAB lines"
    for keep in ("lab_agent_remote_*", "lab_fam_agent_*", "lab_enter_start.*.150.*", "lab_raw_key_arrival.*", "lab_freeze_and_wake.*"):
        assert keep in cases.PROVING_LAB and keep not in todo, keep
    doc = _read("docs", "testing.md")
    assert doc.count("MOVE-AFTER-ONE-RUN") >= 1
    sec = doc[doc.index("MOVE-AFTER-ONE-RUN"):]
    for g in todo:
        fam = g.split(".")[0].rstrip("*")
        assert fam in sec, "docs/testing.md (MOVE-AFTER-ONE-RUN) does not name %s" % fam
    # after the move the candidate tier still plays every arm these entries select in the fix tier
    CS = _ci_seq()
    arms = [c for c in cases.CASES if c.get("lab")]
    quick, cand = L.matrix_selection("quick"), L.matrix_selection(CS.LAB_CANDIDATE_MATRIX)
    for g in todo:
        exact = not any(ch in g for ch in "*?[")
        mine = [c for c in arms if L.name_matches(g, c["name"]) and (exact or L.arm_selected(c["arm"], quick))]
        assert mine, "%s selects no arm in the fix tier's lab job" % g
        lost = [c["name"] for c in mine if not L.arm_selected(c["arm"], cand)]
        assert not lost, "arms the candidate matrix would not play after the move: %s" % lost[:5]


@check
def r16_lab_jobs_and_guards_in_the_ci_file():
    """owner 2026-11-11: APPCI_FAKE_EMBEDDINGS=1 on the three lab jobs (lab_codebase_index_remote judges); the Mac and Windows lab jobs play only the
    PROVING_LAB globs in the candidate tier; the ssh wrapper pinned on app-test-macos; and the guards job names the #2086 and #2089 / #2090 checks."""
    yml = _read(".gitlab-ci.yml")
    CS = _ci_seq()
    for j in CS.LAB_APP.values():
        assert re.search(r'^    APPCI_FAKE_EMBEDDINGS: "1"', _job(yml, j), re.M), "%s does not set APPCI_FAKE_EMBEDDINGS=1" % j
    assert re.search(r'^    APPCI_FAKE_EMBEDDINGS: "0"', _job(yml, CS.REPLAY_JOB), re.M), "the replay extends the lab job and must switch the provider off again"
    for j in ("app-test-linux-wayland", "app-test-linux-x11", "app-test-macos", "app-test-windows"):
        assert "APPCI_FAKE_EMBEDDINGS" not in _job(yml, j), j
    # what the variable does, and what the case does without it (the row it was in every tier until now)
    run_src = _read("script", "realapp", "appci", "run.py")
    assert 'os.environ.get("APPCI_FAKE_EMBEDDINGS") == "1"' in run_src and 'os.environ["APPCI_FAKE_EMBEDDINGS_ACTIVE"] = "1"' in run_src
    import cases_lab_index
    assert 'os.environ.get("APPCI_FAKE_EMBEDDINGS_ACTIVE") != "1"' in inspect.getsource(cases_lab_index.c_codebase_index_remote)
    assert "lab_codebase_index_remote.*" in cases.PROVING_LAB and os.path.isfile(os.path.join(ROOT, "script", "fake_embeddings.py"))
    # the candidate tier, Mac and Windows: the PROVING_LAB globs, as the fix tier's Linux lab job; the release tier and Linux keep lab_*
    globs = CS.proving_lab_globs()
    assert globs == list(cases.PROVING_LAB)
    for p in (CS.MAC, CS.WIN):
        v = CS.lab_tier_vars(p, "candidate")
        assert v == CS.lab_fix_vars() and v["APPCI_CASES"] == ",".join(cases.PROVING_LAB) and v["APPCI_LAB_MATRIX"] == "quick" and v["APPCI_FAKE_MODEL"] == "1", v
        assert "APPCI_CASES" not in CS.lab_tier_vars(p, "release") and "APPCI_CASES" not in CS.lab_tier_vars(p, "candidate", ())
        assert 'APPCI_CASES: "lab_*"' in _job(yml, CS.LAB_APP[p]), "the job's own default stays lab_* (a hand play, the release tier)"
    assert "APPCI_CASES" not in CS.lab_tier_vars(CS.LINUX, "candidate") and CS.lab_tier_vars(CS.LINUX, "candidate")["APPCI_LAB_MATRIX"] == CS.LAB_CANDIDATE_MATRIX
    # fewer arms than the quick matrix of every family, and never the replay (run.py plays lab_replay* only when --cases names it)
    arms = [c for c in cases.CASES if c.get("lab") and not c["name"].startswith("lab_replay")]
    quick = L.matrix_selection("quick")
    exact = {g for g in globs if not any(ch in g for ch in "*?[")}
    every = [c["name"] for c in arms if L.arm_selected(c["arm"], quick)]
    proving = [c["name"] for c in arms if c["name"] in exact or (any(L.name_matches(g, c["name"]) for g in globs) and L.arm_selected(c["arm"], quick))]
    assert proving and not any(g.startswith("lab_replay") for g in globs)
    assert set(every) - set(proving), "PROVING_LAB now selects every quick-matrix arm: the Mac / Windows restriction changes nothing"
    # the ssh wrapper on macOS
    assert re.search(r'^  variables:\n(?:    .*\n)*?    APPCI_SSH_WRAPPER: "1"', _job(yml, "app-test-macos"), re.M)
    import cases_win_input as wi
    assert "app-test-macos" in wi.WRAPPER_PIN_NEEDS
    reg = {c["name"]: c for c in cases.CASES}
    for n in ("ssh_connect_typing_wrapper", "ssh_nested_typeahead_wrapper"):
        assert n in reg and n in cases.PROVING, n
    body = inspect.getsource(wi._wrapper_variant)
    assert "ctx.win and ctx.pane != \"gitbash\"" in body and "mac" not in body.split('"""')[2], "the wrapper cases refuse no macOS pane"
    # the guards job: the two checks by name, and ci_seq's own self-test
    guards = _job(yml, "guards")
    assert "python3 script/realapp/appci/selftest.py --require every_check_function_is_registered_2086,r15_proving_lab_is_one_list" in guards
    assert "python3 script/ci/ci_seq.py --self-test" in guards
    import selftest
    names = [f.__name__ for f in selftest.CHECKS]
    for n in ("every_check_function_is_registered_2086", "r15_proving_lab_is_one_list"):
        assert names.count(n) == 1, "%s is registered %d times" % (n, names.count(n))
    assert selftest.missing_required(["a", "b"], {"a", "b", "c"}) == [] and selftest.missing_required(["a", "gone"], {"a"}) == ["gone"]
    assert selftest.missing_required(["a"], set()) == ["a"], "a required check that FAILED (so is not among the passed) is missing too"
    main_src = inspect.getsource(selftest.main)
    assert "return 1 if failed or absent else 0" in main_src and "passed.add(f.__name__)" in main_src
    doc = _read("docs", "testing.md")
    for word in ("selftest_r16_harness.py", "--require", "APPCI_FAKE_EMBEDDINGS", "LAB_PLAT_PROVING_ONLY_TIERS", "key_pty_share_gate", "APPCI_KEY_PTY_SHARE_MAX_MS"):
        assert word in doc, "docs/testing.md does not mention %s" % word
