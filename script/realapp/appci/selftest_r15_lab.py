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
    assignment, one `+=` line per later entry, and ci_seq reads exactly what Python computes."""
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
    assert ns["fold_tuple_assignments"]('X = ("a",)\nX = ("b",)\nX += ("c",)\n', "X") == ["b", "c"]


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
    gives the recorded `NEEDS: ...` line as the helper arm's reason (the row is UNVERIFIABLE, never a pass, and says which machine change is missing)."""
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
            try:
                ok = CL.lab_prepare(ctx, r, arm)
            finally:
                CL.lab_cleanup(ctx, arm)
            assert ok is False and r.msg.startswith("helper arm: ") and "NEEDS: zig installed" in r.msg and lab.helpers == [], (ok, r.msg, lab.helpers)
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
