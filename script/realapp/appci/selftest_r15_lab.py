# appci/selftest_r15_lab.py: offline self-tests of the round 15 lab work (#2062 the helper artifact, #2063 the lab families and their drivers, #2005 the
# keys kl3 posts). Pure Python: no app, lab, display or network. selftest.py appends CHECKS to its own list, so `run.py --self-test` runs these.
import json, os, re, shutil, sys, tempfile, types

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, "..", "..", ".."))
sys.path.insert(0, HERE)
import cases  # noqa: E402
import backend, injacct  # noqa: E402
import lab as L  # noqa: E402
import cases_lab as CL  # noqa: E402
import cases_lab_buffers as CLB  # noqa: E402
import cases_lab_diff as CLD  # noqa: E402
import cases_lab_git as CLG  # noqa: E402

CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


def _read(*parts):
    return open(os.path.join(ROOT, *parts), encoding="utf-8").read()


def _job(ci, name):
    """the text of one top-level job of .gitlab-ci.yml (to the next blank line)"""
    j = ci[ci.index("\n%s:\n" % name) + 1:]
    return j[:j.index("\n\n")]


@check
def r15_proving_lab_is_one_list():
    """Round 14 left TWO `PROVING_LAB = (...)` statements in cases.py (two branches each rewrote the tuple, the merge kept both). Python used the second,
    ci_seq.py read the first: the harness's list had no lab_agent_nld_predict_ssh and the fix tier's lab job was never given lab_raw_key_arrival. Now: one
    assignment, one `+=` line per later entry, and ci_seq reads exactly what Python computes from them. (Should a second plain assignment ever come back,
    ci_seq's reader returns the union of both, never only one: the first assertion below is what refuses it here.)"""
    src = _read("script", "realapp", "appci", "cases.py")
    assert len(re.findall(r"^PROVING_LAB = ", src, re.M)) == 1, "cases.py must hold ONE `PROVING_LAB = ` statement; append with `PROVING_LAB += (...)` lines"
    assert len(cases.PROVING_LAB) == len(set(cases.PROVING_LAB)), sorted(g for g in cases.PROVING_LAB if cases.PROVING_LAB.count(g) > 1)
    assert {"lab_raw_key_arrival.*", "lab_agent_nld_predict_ssh.*", "lab_agent_remote_*", "lab_ctrl_c_no_esc_i.*"} <= set(cases.PROVING_LAB)
    import importlib.util
    spec = importlib.util.spec_from_file_location("r15_ci_seq_reader", os.path.join(ROOT, "script", "ci", "ci_seq.py"))
    seq_src = open(spec.origin, encoding="utf-8").read()
    # ci_seq.py imports glab helpers at load; only its pure reader is wanted here
    ns = {}
    start = seq_src.index("def fold_tuple_assignments(")
    exec(seq_src[start:seq_src.index("\n\n\n", start)], ns)
    assert ns["fold_tuple_assignments"](src, "PROVING_LAB") == list(cases.PROVING_LAB), "ci_seq.py and the harness disagree about PROVING_LAB"
    assert ns["fold_tuple_assignments"]('X = ("b",)\nX += ("c",)\n', "X") == ["b", "c"]
    # merged with the round 15 tier branch's reader: this expected ["b", "c"] (Python's value). The one reader now keeps a doubled assignment's entries
    # (the union), because the doubled PROVING_LAB of round 14 cost a case its fix-tier runs; cases.py itself may hold only one (asserted above)
    assert ns["fold_tuple_assignments"]('X = ("a",)\nX = ("b",)\nX += ("c",)\n', "X") == ["a", "b", "c"]


@check
def r15_every_lab_family_is_in_a_tier():
    """#2063: 18 lab families were registered and never played, because nothing said a family must be in a tier. Now every lab family (the name before the
    first dot) has at least one arm the fix tier's lab job runs (PROVING_LAB under the quick matrix, or an arm named exactly), or it is in
    cases.LAB_NOT_IN_FIX_TIER with the reason. Never both, and no reason for a family that does not exist."""
    sel = L.matrix_selection("quick")
    exact = {g for g in cases.PROVING_LAB if not any(ch in g for ch in "*?[")}
    fams, played = {}, set()
    for c in cases.CASES:
        if not c.get("lab"):
            continue
        fam = c["name"].split(".")[0]
        fams.setdefault(fam, []).append(c["name"])
        if c["name"] in exact or (any(L.name_matches(g, c["name"]) for g in cases.PROVING_LAB) and L.arm_selected(c["arm"], sel)):
            if not c["name"].startswith("lab_replay"):          # run.py never sweeps the replay in by a glob
                played.add(fam)
    names = {c["name"] for c in cases.CASES}
    assert exact <= names, "PROVING_LAB names arms that are not registered: %s" % sorted(exact - names)
    excused = set(cases.LAB_NOT_IN_FIX_TIER)
    assert excused <= set(fams), "LAB_NOT_IN_FIX_TIER names families that are not registered: %s" % sorted(excused - set(fams))
    assert not (excused & played), "in the fix tier AND excused from it: %s" % sorted(excused & played)
    missing = sorted(set(fams) - played - excused)
    assert not missing, "lab families no tier plays (add a `PROVING_LAB += (...)` line in cases.py, or a reason in LAB_NOT_IN_FIX_TIER): %s" % missing
    for fam, why in cases.LAB_NOT_IN_FIX_TIER.items():
        assert len(why) > 20, fam
    # the families #2063 names (docs/design/r14-test-gaps.md 4.5, last row) are played, with a no-helper arm each where the family has one
    for fam in ("lab_chips", "lab_path_detect", "lab_tab_remote_dir", "lab_file_tree_remote", "lab_diff_panel_remote", "lab_git_write_remote", "lab_codebase_index_remote",
                "lab_skills_remote", "lab_read_files_remote", "lab_remote_edit", "lab_password_prompt", "lab_ssh_drop_reconnect", "lab_fam_fs_basic", "lab_fam_git_read",
                "lab_fam_run_command", "lab_gh_chip_remote", "lab_diff_apply_remote", "lab_no_helper_chip", "lab_nu_agent_pager_env", "lab_raw_key_arrival",
                "lab_nohelper_tab_hands_back", "lab_nohelper_unavailable", "lab_nohelper_listing_single_flight"):
        assert fam in played, fam
        arms = [c for c in cases.CASES if c["name"].split(".")[0] == fam and c["arm"]["helper"] == "nohelper"
                and (c["name"] in exact or (any(L.name_matches(g, c["name"]) for g in cases.PROVING_LAB) and L.arm_selected(c["arm"], sel)))]
        assert arms, "%s has no no-helper arm in the fix tier" % fam
    # scenario users and network faults are outside the quick matrix: they are played because they are named exactly
    assert {"lab_password_prompt.p_password.lan.nohelper", "lab_password_prompt.p_kbdint.lan.nohelper", "lab_ssh_drop_reconnect.bash.lan.nohelper.rst"} <= exact
    assert "lab_replay" in cases.LAB_NOT_IN_FIX_TIER and "app-test-replay-linux" in cases.LAB_NOT_IN_FIX_TIER["lab_replay"]
    doc = _read("docs", "testing.md")
    assert "LAB_NOT_IN_FIX_TIER" in doc and "r15_every_lab_family_is_in_a_tier" in doc


@check
def r15_lab_helper_from_the_artifact():
    """#2062: a lab job on a machine with no helper cache (the Mac, the Windows VM) takes the `lab-helper` job's artifact, ./helper-artifacts/. The resolver
    uses it only as THIS checkout's helper, prefers an explicit APPCI_LAB_HELPER_BIN, falls back to the cache, and when the box could not build the helper it
    gives the recorded `NEEDS: ...` line as the helper arm's reason (the row is a SKIP since #2063, never a pass, and says which machine change is missing)."""
    LH = CL.lab_helper_cache()
    d = tempfile.mkdtemp()
    try:
        art, cache = os.path.join(d, "helper-artifacts"), os.path.join(d, "cache")
        env = {"APPCI_LAB_HELPER_ARTIFACTS": art, "PHOSPHOR_CI_HELPER_CACHE": cache, "CI_PIPELINE_ID": "4242"}
        # nothing anywhere: the job is named; a NEEDS line appears only when something recorded one (or zig is absent where the build looks, on Linux)
        path, why = CL.resolve_helper_bin(dict(env, HOME=d, PATH=""))
        assert path == "" and "lab-helper" in why and "helper-artifacts" in why, why
        # the box could not build: the artifact holds the NEEDS record
        LH.write_needs(art, ["zig", "cargo-zigbuild"], "k", "2026-11-11T00:00:00Z")
        path, why = CL.resolve_helper_bin(env)
        assert path == "" and "NEEDS: zig, cargo-zigbuild installed" in why and "owner: machine change" in why and "could not build" in why, why
        assert why.count("NEEDS:") == 1, why
        shutil.rmtree(art)
        # the cache root holds the record (the Linux lab job, same machine)
        LH.write_needs(cache, ["zig"], "k")
        path, why = CL.resolve_helper_bin(env)
        assert path == "" and "NEEDS: zig installed" in why and why.count("NEEDS:") == 1, why
        LH.clear_needs(cache)
        # a real entry, exported as the job exports it
        try:
            key = LH.key_of(LH.key_inputs(LH.ROOT, LH.helper_tag(LH.ROOT)))
        except LH.NoKey:
            key = None
        tag = LH.helper_tag(LH.ROOT)
        line = "phosphor-remote %s protocol 1" % tag
        fake = os.path.join(d, "fake")
        open(fake, "wb").write(b"\x7fELF\x02\x01" + bytes(12) + (62).to_bytes(2, "little") + b" not a real helper")
        entry = LH.store(os.path.join(d, "elsewhere"), key or "nokey", fake, line, {"tag": tag})
        LH.export(entry, art, {"CI_PIPELINE_ID": "4242", "CI_JOB_ID": "9"})
        path, why = CL.resolve_helper_bin(env)
        assert why is None and path == os.path.join(art, "phosphor-remote"), (path, why)
        assert CL.helper_version_line(path) == line, "a helper this machine cannot start is read from the .version file the artifact carries"
        assert CL.helper_arch(path) == "x86_64"
        # explicit still wins; another pipeline's artifact with no key match is refused
        assert CL.resolve_helper_bin(dict(env, APPCI_LAB_HELPER_BIN=fake)) == (fake, None)
        meta = json.load(open(os.path.join(art, "meta.json")))
        meta["key"] = "0" * 24
        json.dump(meta, open(os.path.join(art, "meta.json"), "w"))
        path, why = CL.resolve_helper_bin(dict(env, CI_PIPELINE_ID="1"))
        if key is not None:
            assert path == "" and "another commit's helper" in why, (path, why)
        else:
            assert path == "" and "cannot be tied to this checkout" in why, (path, why)

        # lab_prepare: the helper arm stops with that reason, before anything is put on the lab host
        class FakeLab:
            info = {"flavor": "full"}
            helpers = []

            def helper(self, user, files=None):
                self.helpers.append(user)

        class R:
            msg = None

            def unverifiable(self, msg):
                self.msg = msg

            def skip(self, msg):
                raise cases.Skip(msg)
        import profile as P
        shutil.rmtree(art)
        LH.write_needs(art, ["zig"], "k")
        keep = {k: os.environ.get(k) for k in ("APPCI_LAB_HELPER_BIN", "APPCI_LAB_HELPER_ARTIFACTS", "PHOSPHOR_CI_HELPER_CACHE", "APPCI_APP_PINNED_DIGESTS")}
        try:
            os.environ.pop("APPCI_LAB_HELPER_BIN", None)
            os.environ.pop("APPCI_APP_PINNED_DIGESTS", None)
            os.environ["APPCI_LAB_HELPER_ARTIFACTS"], os.environ["PHOSPHOR_CI_HELPER_CACHE"] = art, cache
            prof = P.Profile(d)
            lab = FakeLab()
            ctx = types.SimpleNamespace(lab=lab, lab_why=None, prof=prof, remote=L.LocalRemote(prof), arm=None, lab_helper_mode="nohelper", run=d)
            arm = dict(name="lab_x.bash.lan.helper", user="u_bash", shell="bash", helper="helper", lat_ms=0, flavor="full")
            r = R()
            # merged with the round 15 tier branch (#2063): a helper arm with no helper file is a SKIP that names the machine change (cases_lab.
            # helper_missing_reason), no longer an UNVERIFIABLE row. This asserted `ok is False` and the text on r.msg; the same text (the arm, the
            # recorded NEEDS line of the artifact) is now asserted on the Skip, and nothing may have been marked unverifiable or put on the lab host.
            ok, skipped = None, None
            try:
                ok = CL.lab_prepare(ctx, r, arm)
            except cases.Skip as e:
                skipped = str(e)
            finally:
                CL.lab_cleanup(ctx, arm)
            assert ok is None and skipped and skipped.startswith("helper arm: ") and "NEEDS: zig installed" in skipped and "#2062" in skipped, (ok, skipped)
            assert r.msg is None and lab.helpers == [], (r.msg, lab.helpers)
        finally:
            for k, v in keep.items():
                if v is None:
                    os.environ.pop(k, None)
                else:
                    os.environ[k] = v
    finally:
        shutil.rmtree(d, ignore_errors=True)
    # the CI wiring
    ci = _read(".gitlab-ci.yml")
    helper = _job(ci, "lab-helper")
    assert "when: always" in helper[helper.index("artifacts:"):] and "helper-artifacts/" in helper, "lab-helper must upload helper-artifacts/ when it fails too (NEEDS.txt)"
    for name, build in (("app-test-lab-macos", "build-macos-arm64"), ("app-test-lab-windows", "build-windows-x86_64")):
        job = _job(ci, name)
        assert re.search(r"needs:\n    - job: %s\n      artifacts: false\n    - job: lab-helper\n      artifacts: true\n      optional: true\n" % build, job), job
        assert 'APPCI_APP_PINNED_DIGESTS: "0"' in job and "APPCI_LAB: fixed" in job, job
    linux = _job(ci, "app-test-lab-linux")
    assert "job: lab-helper" not in linux, "the Linux lab job reads the box's cache; a `needs` on the manual lab-helper job would make it unplayable until that job is played"
    assert "lab_helper.py artifact" in linux and "lab_helper.py path" in linux
    doc = _read("docs", "testing.md")
    assert "helper-artifacts/NEEDS.txt" in doc and "APPCI_LAB_HELPER_ARTIFACTS" in doc and "needs: lab-helper" in doc


@check
def r15_lab_panel_drivers_pure_parts():
    """#2063: the three lab families that ended "no keyboard path / no pointer click" now have drivers. Their pure parts, each fed the wrong result it must
    not accept, and the app text they rest on (palette entries, button words, the two app log lines), read from the sources."""
    # the app's words
    ftl = _read("app", "i18n", "en", "warp.ftl")
    assert "\nkeybinding-desc-workspace-toggle-right-panel = %s\n" % CL.PALETTE_CODE_REVIEW in ftl
    assert "\nkeybinding-desc-workspace-toggle-project-explorer = %s\n" % CL.PALETTE_PROJECT_EXPLORER in ftl
    assert "\ncommon-commit = %s\n" % CLG.COMMIT_WORD in ftl and "\ncommon-confirm = %s\n" % CLG.CONFIRM_WORD in ftl
    rp = _read("app", "src", "workspace", "view", "right_panel.rs")
    assert 'format!("hop=right_panel open={open} repo_context={repo_context}")' in rp, "right_panel_trace_line changed: cases_lab.HOOK_RIGHT_PANEL reads it"
    view = _read("app", "src", "workspace", "view.rs")
    assert "right_panel::right_panel_trace_line(" in view and '"Opening remote file: host={host} path={path} target={target:?}"' in view
    tree = _read("app", "src", "code", "file_tree", "view.rs")
    assert "(selected_item.index + 1).min(root_dir.items.len().saturating_sub(1))" in tree, "the file tree's Down no longer stops at the last row: lab_remote_edit relies on it"
    # the right panel's trace line
    log = "x hop=right_panel open=true repo_context=false\ny hop=right_panel open=false repo_context=true\nhop=right_panel open=maybe repo_context=true\n"
    assert CL.right_panel_states(log) == [True, False] and CL.right_panel_states("hop=diff_state event=snapshot") == []
    # no snapshot: a FAIL only when the app said the panel is open
    assert CLD.panel_no_snapshot_verdict("open")[0] == "fail" and "hop=right_panel open=true" in CLD.panel_no_snapshot_verdict("open")[1]
    for opened in ("unknown", None):
        v, msg = CLD.panel_no_snapshot_verdict(opened)
        assert v == "noref" and "NEEDS:" in msg, (opened, v, msg)

    # open_code_review: an already-open panel is toggled closed by the first Enter and opened by the second; no trace line is 'unknown'
    class Be:
        name = "wayland"

        def __init__(self, answers):
            self.answers, self.keys, self.typed = list(answers), [], []

        def guard(self):
            pass

        def key(self, spec, **_kw):
            self.keys.append(spec)

        def type(self, text, *_a, **_kw):
            self.typed.append(text)

    def ctx_for(answers):
        be = Be(answers)
        state = {"log": "", "n": 0}

        def log_size():
            return len(state["log"])

        def log_since(off):
            return state["log"][off:]

        def until(pred, timeout=1, step=0.1):
            # the app "answers" one palette run per poll: the next scripted line is appended once per Return seen
            while state["n"] < be.keys.count("Return") and be.answers:
                state["log"] += be.answers.pop(0)
                state["n"] += 1
            return pred()
        return types.SimpleNamespace(be=be, win=False, log_size=log_size, log_since=log_since, until=until, shot=lambda tag: None), be
    real_sleep, CL.time.sleep = CL.time.sleep, lambda _s: None
    try:
        ctx, be = ctx_for(["hop=right_panel open=true repo_context=false\n"])
        assert CL.open_code_review(ctx, 0, "t") == "open" and be.typed == [CL.PALETTE_CODE_REVIEW]
        ctx, be = ctx_for(["hop=right_panel open=false repo_context=false\n", "hop=right_panel open=true repo_context=false\n"])
        assert CL.open_code_review(ctx, 0, "t") == "open" and be.typed == [CL.PALETTE_CODE_REVIEW] * 2, be.typed
        ctx, be = ctx_for([""])
        assert CL.open_code_review(ctx, 0, "t") == "unknown" and len(be.typed) == 1
        ctx, be = ctx_for(["hop=right_panel open=false repo_context=false\n", "hop=right_panel open=false repo_context=false\n"])
        assert CL.open_code_review(ctx, 0, "t") == "unknown"
    finally:
        CL.time.sleep = real_sleep
    # the held foreground: one sh word list every login shell passes on unchanged; a path with shell syntax in it is refused, not typed
    assert CL.hold_line("/work/u_bash/go_ab12") == "sh -c 'until [ -e /work/u_bash/go_ab12 ]; do sleep 0.2; done'"
    assert not any(ch in CL.hold_line("/work/u_nu/go_x") for ch in "!$\\\"`")
    for bad in ("/work/a b", "/work/a;rm", "/work/$(x)", "/work/a'b"):
        try:
            CL.hold_line(bad)
            raise RuntimeError("hold_line accepted %r" % bad)
        except AssertionError:
            pass
    # a fish / nu login shell: the decided plain session, read from the log of the connect; every other shell is judged
    plain = "x Received PlainSSHFallback hook\n"
    for shell in ("fish", "nu"):
        why = CL.plain_session_feature_limit(shell, plain, "the code-review panel of a remote repository")
        assert why and "#2019" in why and "DECLINED.md" in why and "the code-review panel" in why, why
        assert CL.plain_session_feature_limit(shell, plain + "Received ManagedSSHSessionStarting hook\n", "f") is None
        assert CL.plain_session_feature_limit(shell, "", "f") is None
    for shell in ("bash", "zsh", "dash", "ash", "pwsh"):
        assert CL.plain_session_feature_limit(shell, plain, "f") is None, shell
    # lab_remote_edit: the app's own line is the proof that the file opened; another file is not it
    opened = "Opening remote file: host=h1 path=/work/u_bash/edit_ab/zz_ab.txt target=CodeEditor(SplitPane)\n"
    assert CLB.opened_remote(opened, "zz_ab.txt") and not CLB.opened_remote(opened, "ab.txt") and not CLB.opened_remote(opened, "zz_cd.txt")
    assert not CLB.opened_remote("Opening file: /tmp/zz_ab.txt\n", "zz_ab.txt")
    # ... and the conflict: the host's change must survive the save
    assert CLB.conflict_verdict("E1_staged x\noutside_x", "outside_x", "E2_") is None, "the save was refused: fine"
    assert CLB.conflict_verdict("E2_E1_staged x\noutside_x", "outside_x", "E2_") is None, "the editor took the change in first: fine"
    assert "REPLACED" in CLB.conflict_verdict("E2_E1_staged x", "outside_x", "E2_")
    assert "neither" in CLB.conflict_verdict("", "outside_x", "E2_")
    assert CLB.expected_backend({"helper": "nohelper"}) == "shell" and CLB.expected_backend({"helper": "upload"}) == "helper"
    assert CLB.routes("hop=backend_route family=buffers backend=shell reason=caps\nhop=backend_route family=git_write backend=helper reason=caps\n") == [("shell", "caps")]
    # lab_git_write_remote, the app layer: every way it can end
    V = CLG.app_commit_verdict
    assert V("driven", "", ["shell"], "shell", True)[0] == "pass"
    assert V("driven", "", ["helper"], "shell", True)[0] == "fail" and "helper backend" in V("driven", "", ["helper"], "shell", True)[1]
    assert V("nopointer", "mac", ["helper", "shell"], "helper", False)[0] == "fail", "a call on the wrong backend fails however the driver fared"
    assert V("driven", "", ["shell"], "shell", False)[0] == "fail", "the app tried and the commit is not on the host"
    assert V("driven", "", [], "shell", True)[0] == "fail", "committed with no route line: the route is not logged"
    for how, detail, word in (("nopointer", "mac", "NEEDS: pointer synthesis"), ("nopanel", "", "NEEDS: an app build"), ("nobutton", "Confirm", "'Confirm'"), ("driven", "", "not judged")):
        v, msg = V(how, detail, [], "shell", False)
        assert v == "noref" and word in msg, (how, v, msg)
    assert [m.group(1) for m in CLG.HOOK_REMOTE_OP.finditer("hop=backend_route family=git_write backend=shell reason=caps\nhop=backend_route family=git_read backend=helper x\n")] == ["shell"]
    # drive_commit without a pointer backend decides nothing and clicks nothing
    for name, win in (("mac", False), ("windows", True)):
        be = Be([])
        be.name = name
        assert CLG.drive_commit(types.SimpleNamespace(be=be, win=win), None, {"lat_ms": 0}, "m", 0)[0] == "nopointer" and not be.keys and not be.typed
    # the three cases are still registered on both arms, terminal mode, and the stale claims are gone from the sources
    reg = {c["name"]: c for c in cases.CASES}
    for n in ("lab_diff_panel_remote.bash.lan.nohelper", "lab_diff_panel_remote.bash.lan.upload", "lab_remote_edit.zsh.lan.nohelper", "lab_git_write_remote.bash.lan.nohelper"):
        assert n in reg and reg[n]["modes"] == ("terminal",), n
    for f in ("cases_lab_diff.py", "cases_lab_buffers.py", "cases_lab_git.py"):
        src = open(os.path.join(HERE, f), encoding="utf-8").read()
        assert "no keyboard path" not in src and "no pointer click on the file tree" not in src, f
    doc = _read("docs", "testing.md")
    for word in ("hop=right_panel", CL.PALETTE_CODE_REVIEW, CL.PALETTE_PROJECT_EXPLORER, "lab_remote_edit", "panel_no_snapshot_verdict", "app_commit_verdict"):
        assert word in doc, word


@check
def r15_kl3_keys_are_recorded():
    """#2005, what was left of it in round 12 (macOS only: latency_paint, latency_paint_ab, latency_key_views_ab, both modes, "app logged 39 / 78 more than
    were injected"): kl3 posts its own keys and no injector record was kept for them. kl() now records each as a call, so the accounting counts them as
    sent; a kl3 key aimed at the reference terminal is a `ref` call (not the app's), and a kl3 answer without a post time records an errored call."""
    assert backend.kl_keys("K 400 m") == (1, [], 0) and backend.kl_keys("K 400 DEL") == (0, ["KEY51"], 0) and backend.kl_keys("K 400 CMDP") == (0, ["KEY35"], 1)
    assert backend.kl_keys("K 400 AT") == (1, [], 0) and backend.kl_keys("R 400") == (0, ["KEY36"], 0)
    assert backend.kl_keys("T 400 ls -la") == (6, ["KEY36"], 0) and backend.kl_keys("W echo hi") == (7, [], 0)
    assert backend.kl_keys("C 200") == (0, [], 0) and backend.kl_keys("Q") == (0, [], 0) and backend.kl_keys("") == (0, [], 0)
    assert backend.kl_posted("K post0=1731200000.123 post1=1731200000.124 n=2 ch=1.0:5") and backend.kl_posted("W done")
    assert not backend.kl_posted("ERR no display") and not backend.kl_posted("BADCHAR") and not backend.kl_posted("")
    swift = _read("script", "realapp", "mac", "tools", "kl3.swift")
    for name, code in (("TAB", 48), ("CMDP", 35), ("CMDK", 40), ("DEL", 51), ("ESC", 53)):
        assert 'if s == "%s"' % name in swift and backend.KL_NAMED[name] == "KEY%d" % code, name
    d = tempfile.mkdtemp()
    try:
        mb = backend.MacBackend()
        mb.calls, mb.inject_log, mb.guard_override = [], os.path.join(d, "inject.log"), None
        t = 500.0
        for i in range(6):                              # latency_paint's pattern: 'm' and Delete in turn
            key = "m" if i % 2 == 0 else "DEL"
            mb._record_kl("K 400 %s" % key, "K post0=%.3f post1=%.3f n=1 ch=%.1f:9" % (t + i, t + i + 0.001, (t + i) * 1000 + 6), t + i - 0.35, t + i + 0.4)
        mb._record_kl("C 200", "C n=0 ch=", t + 7, t + 7.2)                                # posts nothing: no record
        assert len(mb.calls) == 6 and [c["chars"] for c in mb.calls] == [1, 0, 1, 0, 1, 0] and all(c["tool"] == "kl3" and not c["ref"] for c in mb.calls)
        assert mb.calls[1]["special"] == ["KEY51@501.000"] and mb.calls[1]["post_wall"] == [501.0]
        lines = open(mb.inject_log).read().splitlines()
        assert len(lines) == 6 and "tool=kl3" in lines[0] and "chars=1" in lines[0] and "KEY51@" in lines[1], lines

        def osk(t):
            return "input_order hop=os_key_event age_ms=0.400 enter=false ctrl=false t=%.3f\n" % (t * 1000)

        def kd(t, seq):
            return "input_order keydown key=<1 chars> ctrl=false alt=false shift=false cmd=false t=%.3f seq=%d\n" % (t * 1000, seq)
        log = os.path.join(d, "app.log")
        open(log, "w").write("".join(osk(t + i + 0.002) + kd(t + i + 0.003, i + 1) for i in range(6)))
        txt, j = injacct.account(mb.calls, log, "latency_paint", t_end=t + 8, t_floor=t - 1)
        assert (j["sent"], j["seen"], j["lost"], j["extra"]) == (6, 6, 0, 0) and "more than were injected" not in txt, (j, txt)
        # before: the same app log against no record of kl3's keys is the round 12 note (one htype2 key so the case has a window at all)
        one = [dict(n=0, wall0=t - 0.5, wall1=t + 6, chars=0, special=[], enters_posted=0, enters_want=0, enter_wall=[], err=None, out=True, ref=False, cmd_chords=0)]
        txt0, j0 = injacct.account(one, log, "latency_paint", t_end=t + 8, t_floor=t - 1)
        assert j0["extra"] == 6 and "app logged 6 more than were injected" in txt0, (j0, txt0)
        # a key the app never saw is still a loss: the record does not hide it
        open(log, "w").write("".join(osk(t + i + 0.002) + kd(t + i + 0.003, i + 1) for i in range(5)))
        _txt, j1 = injacct.account(mb.calls, log, "latency_paint", t_end=t + 8, t_floor=t - 1)
        assert (j1["sent"], j1["lost"]) == (6, 1), j1
        # aimed at the reference terminal: recorded, and not expected of the app
        mb.calls, mb.guard_override = [], {"pids": [1]}
        mb._record_kl("K 400 m", "K post0=600.000 post1=600.001 n=1 ch=", 599.6, 600.4)
        assert mb.calls[0]["ref"] is True and injacct.join(mb.calls, []) is None
        # kl3 answered without a post time: an errored call, which counts as nothing sent
        mb.calls, mb.guard_override = [], None
        mb._record_kl("K 400 m", "ERR capture stopped", 700.0, 700.1)
        assert mb.calls[0]["err"] and mb.calls[0]["chars"] == 0 and injacct.join(mb.calls, [])["sent"] == 0
    finally:
        shutil.rmtree(d, ignore_errors=True)
