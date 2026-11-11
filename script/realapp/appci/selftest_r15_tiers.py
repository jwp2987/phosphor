# appci/selftest_r15_tiers.py: offline self-tests of round 15's tier reach (#2056, #2057, #2058, #2063, #2064; docs/design/r15-tiers.md). Pure Python:
# no app, display or network. selftest.py appends CHECKS to its own list, so `run.py --self-test` and `selftest.py` both run these.
import importlib.util, os, shutil, subprocess, sys, tempfile, types

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
sys.path.insert(0, HERE)
import cases, winmap  # noqa: E402
import lab as L  # noqa: E402

CHECKS = []
# docs/design/r14-test-gaps.md 7.0 b: the daily-use slice of the full-tier cases
DAILY_SLICE = ("session_restore", "crash_kill9_restore", "ui_resize_typing", "ui_split_panes", "ui_ctrl_r", "ui_ghost_accept", "ui_tab_menu",
               "big_paste", "paste_multiline_prompt", "ssh_password_prompt", "remote_session_legacy", "alt_screen_programs", "agent_approval")


def check(f):
    CHECKS.append(f)
    return f


def ci_seq():
    """script/ci/ci_seq.py as a module (it imports no harness code; this file reads its tier constants so the two cannot drift)"""
    path = os.path.join(ROOT, "script", "ci", "ci_seq.py")
    spec = importlib.util.spec_from_file_location("appci_r15_ci_seq", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


@check
def r15_daily_slice_runs_in_the_fix_tier():
    """#2057: the 13 daily-use cases are registered, in PROVING (so the fix tier plays them on Linux / macOS and plans them on every Windows pane), not in
    SMOKE, not in the fake-model pass, each classified for Windows by name, and docs/testing.md names each."""
    reg = {c["name"]: c for c in cases.CASES}
    assert len(DAILY_SLICE) == 13 and len(set(DAILY_SLICE)) == 13
    sel = winmap.smoke_for("table", cases.SMOKE + cases.PROVING)
    doc = open(os.path.join(ROOT, "docs", "testing.md"), encoding="utf-8").read()
    for n in DAILY_SLICE:
        assert n in reg, "%s is not a registered case" % n
        assert n in cases.PROVING and n not in cases.SMOKE and n not in cases.PROVING_FAKE, n
        assert n in sel, "%s is not in the fix-tier selection" % n
        assert not reg[n].get("lab"), n
        assert n in winmap.BOTH or n in winmap.GITBASH_ONLY or n in winmap.NOT_WINDOWS, "%s has no winmap entry" % n
        for mode in ("terminal", "agent"):
            for pane in ("powershell", "gitbash"):
                names, _o = winmap.tier_plan("windows", "smoke", mode, pane, cases.SMOKE, cases.PROVING)
                assert n in names, (n, mode, pane)
        assert ("`%s`" % n) in doc, "docs/testing.md does not name %s" % n
    # the control: a case that is in neither list is NOT in the fix-tier selection (this is what the 13 were before round 15)
    outside = [n for n in reg if n not in cases.SMOKE and n not in cases.PROVING and not reg[n].get("lab")]
    assert outside and not set(outside) & set(sel), "a case outside SMOKE + PROVING is in the fix-tier selection"
    # the modes the fix tier will play them in are the registered ones (agent_approval is agent-only, the two coverage_a cases terminal-only)
    assert reg["agent_approval"]["modes"] == ("agent",) and reg["crash_kill9_restore"]["modes"] == ("terminal",) and reg["paste_multiline_prompt"]["modes"] == ("terminal",)


@check
def r15_candidate_lab_matrix_covers_every_family():
    """#2063: the candidate tier's Linux lab job (ci_seq.LAB_CANDIDATE_MATRIX, the job's own `lab_*`) selects at least one arm of EVERY lab family but the
    replay, the replay job's glob selects exactly the replay, and the quick matrix does not reach every family (the gap this closes)."""
    CS = ci_seq()
    arms = [c for c in cases.CASES if c.get("lab")]
    fams = {c["arm"]["case"] for c in arms}
    assert "replay" in fams and len(fams) > 30, sorted(fams)

    def selected(matrix, globs):
        """run.py's selection of lab arms for --cases `globs` under APPCI_LAB_MATRIX=`matrix` (the replay only when a pattern names it)"""
        sel = L.matrix_selection(matrix)
        return [c for c in arms if L.name_matches(globs, c["name"]) and L.arm_selected(c["arm"], sel) and (not c["name"].startswith("lab_replay") or "lab_replay" in globs)]
    src = open(os.path.join(HERE, "run.py"), encoding="utf-8").read()
    assert 'not c["name"].startswith("lab_replay") or "lab_replay" in a.cases' in src, "run.py no longer keeps the replay out of `lab_*`: this check's selection rule is stale"
    cand = selected(CS.LAB_CANDIDATE_MATRIX, "lab_*")
    got = {c["arm"]["case"] for c in cand}
    assert got == fams - {"replay"}, "families the candidate lab job does not play: %s; unexpected: %s" % (sorted(fams - {"replay"} - got), sorted(got - fams))
    assert {c["arm"]["helper"] for c in cand} == set(L.HELPERS), "a helper arm kind is missing from the candidate matrix"
    assert any(c["arm"]["fault"] for c in cand) and any(c["arm"]["scenario"] for c in cand)
    assert "APPCI_CASES" not in CS.lab_tier_vars(CS.LINUX), "the lab jobs keep their own `lab_*`"
    rep = selected("quick", "lab_replay*")              # app-test-replay-linux's own variables (.gitlab-ci.yml)
    assert rep and {c["arm"]["case"] for c in rep} == {"replay"}
    # the control: the quick matrix leaves families out, so the assertion above can fail
    quick = {c["arm"]["case"] for c in selected("quick", "lab_*")}
    assert fams - quick, "the quick matrix now reaches every family: LAB_CANDIDATE_MATRIX may be simplified"
    assert {"helper_install", "password_prompt"} <= fams - quick
    # macOS / Windows: the quick matrix, same case list; on Windows only the Git Bash pane runs them
    plat = selected(CS.LAB_PLAT_MATRIX, "lab_*")
    assert plat and "replay" not in {c["arm"]["case"] for c in plat}
    assert CS.lab_tier_vars(CS.MAC)["APPCI_LAB_MATRIX"] == CS.LAB_PLAT_MATRIX and CS.lab_tier_vars(CS.LINUX)["APPCI_LAB_MATRIX"] == CS.LAB_CANDIDATE_MATRIX
    assert winmap.skip_reason(plat[0]["name"], "powershell") and winmap.skip_reason("lab_chips.bash.lan.nohelper", "gitbash") is None
    # the fix tier's lab job now gets the union of both PROVING_LAB lines of cases.py (ci_seq read the first, Python keeps the second)
    globs = CS.proving_lab_globs()
    assert set(cases.PROVING_LAB) <= set(globs) and "lab_agent_nld_predict_ssh.*" in globs and "lab_raw_key_arrival.*" in globs, globs
    for g in globs:
        assert any(L.name_matches(g, c["name"]) for c in arms), "PROVING_LAB glob %s matches no lab arm" % g


@check
def r15_helper_arm_without_a_helper_file_skips_with_needs():
    """#2063 / #2062: a helper arm with no helper file SKIPs with `NEEDS:` naming the machine change and the job (it was a NOREF row); the pane's remote is put
    back; a file that IS there and is not a helper stays UNVERIFIABLE (a wrong explicit choice is not a missing capability); a nohelper arm is not touched."""
    import cases_lab as CL
    import profile as P
    why = "APPCI_LAB_HELPER_BIN is not set and the lab helper cache has none for this checkout: no cached helper for key abc"
    txt = CL.helper_missing_reason("upload", why)
    assert txt.startswith("upload arm: no helper file (") and why in txt and "NEEDS: " in txt and "#2062" in txt and "`lab-helper`" in txt, txt
    for word in ("zig", "cargo-zigbuild", "x86_64-unknown-linux-musl", "Linux box"):
        assert word in CL.HELPER_NEEDS, word

    class FakeLab:
        info = {"flavor": "full"}

        def helper(self, user, files=None):
            pass

        def clear_fault(self):
            pass

    class R:
        msg = None

        def unverifiable(self, msg):
            self.msg = msg

        def skip(self, msg):
            raise cases.Skip(msg)
    d = tempfile.mkdtemp()
    keep = {k: os.environ.get(k) for k in ("APPCI_LAB_HELPER_BIN", "PHOSPHOR_CI_HELPER_CACHE", "APPCI_APP_PINNED_DIGESTS")}
    try:
        for k in keep:
            os.environ.pop(k, None)
        os.environ["PHOSPHOR_CI_HELPER_CACHE"] = os.path.join(d, "empty-cache")
        prof = P.Profile(d)
        local = L.LocalRemote(prof)

        def run(helper):
            ctx = types.SimpleNamespace(lab=FakeLab(), lab_why=None, prof=prof, remote=local, arm=None, lab_helper_mode="nohelper", run=d)
            arm = dict(name="lab_x.bash.lan.%s" % helper, user="u_bash", shell="bash", helper=helper, lat_ms=0, flavor="full")
            ran, r = [], R()
            try:
                CL._runner(lambda c, rr, a: ran.append(1), arm)(ctx, r)
                out = None
            except cases.Skip as e:
                out = str(e)
            return out, r, ran, ctx
        # no cache entry, no variable: every helper arm kind skips with NEEDS, nothing runs, the swap is undone
        for helper in ("helper", "upload", "upgrade", "never"):
            out, r, ran, ctx = run(helper)
            assert out and "NEEDS: " in out and "#2062" in out and out.startswith(helper + " arm: no helper file"), (helper, out)
            assert not ran and r.msg is None and ctx.remote is local and prof.remote is None, helper
        # the Mac / Windows form: the variable names the artifact path and lab-helper left no artifact
        os.environ["APPCI_LAB_HELPER_BIN"] = os.path.join(d, "helper-artifacts", "phosphor-remote")
        out, r, ran, ctx = run("helper")
        assert out and "does not name a built phosphor-remote binary" in out and "NEEDS: " in out and not ran, out
        # a file that is there and is not a helper: UNVERIFIABLE as before, not a NEEDS skip
        bogus = os.path.join(d, "bogus")
        open(bogus, "w").write("not a helper\n")
        os.environ["APPCI_LAB_HELPER_BIN"] = bogus
        out, r, ran, ctx = run("helper")
        assert out is None and r.msg and "--version" in r.msg and "NEEDS" not in r.msg and not ran, (out, r.msg)
        assert ctx.remote is local and prof.remote is None
    finally:
        for k, v in keep.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v
        shutil.rmtree(d, ignore_errors=True)


@check
def r15_tier_drivers_self_tests_and_docs():
    """The tier plan (ci_seq.py) and the collector's new parsers (collect_results.py) have their own self-tests; they are pinned here so the guards job, which
    runs this file, runs them too. The docs name what changed."""
    if os.name == "posix":                              # both use git and POSIX paths; the Windows VM runs the appci self-test only for the harness
        for script in ("ci_seq.py", "collect_results.py"):
            p = subprocess.run([sys.executable, os.path.join(ROOT, "script", "ci", script), "--self-test"], capture_output=True, text=True, cwd=ROOT, stdin=subprocess.DEVNULL)
            assert p.returncode == 0, "%s --self-test failed:\n%s" % (script, "\n".join(l for l in p.stdout.splitlines() if not l.startswith("ok"))[-1500:])
    doc = open(os.path.join(ROOT, "docs", "testing.md"), encoding="utf-8").read()
    for needle in ("## Round 15: tier reach", "LAB_CANDIDATE_MATRIX", "app-test-replay-linux", "app-test-lab-macos", "app-test-lab-windows", "`NEEDS`", "#2062",
                   "parse_usage", "selftest_r15_tiers.py", "docs/design/r15-tiers.md"):
        assert needle in doc, "docs/testing.md does not mention %s" % needle
    yml = open(os.path.join(ROOT, ".gitlab-ci.yml"), encoding="utf-8").read()
    x11 = yml[yml.index("\napp-test-linux-x11:"):]
    x11 = x11[:x11.index("\n\n")]
    assert "exit 77" not in x11 and "--mode x11" in x11, "the x11 job must run in the fix tier (#2058)"
