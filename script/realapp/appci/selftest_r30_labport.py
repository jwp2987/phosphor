# appci/selftest_r30_labport.py: offline self-tests of round 30's move of the Windows fake-sshd ssh cases into the lab (cases_lab_winport.py, winmap.LAB_MOVED;
# owner rule 2026-11-13: ssh / remote tests on all three platforms run against the real lab sshd). Pure Python, no lab, no app.
import os, sys, types

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


NEW_FAMILIES = ("lab_connect_typeahead", "lab_managed_connect", "lab_msys_ssh_switch", "lab_tab_no_helper", "lab_remote_session")


class _R:
    def __init__(self):
        self.fails = []

    def check(self, ok, msg):
        if not ok:
            self.fails.append(msg)
        return ok

    def metric(self, *a, **k):
        pass

    def note(self, *a, **k):
        pass

    def unverifiable(self, msg):
        raise AssertionError("unverifiable: " + msg)

    def skip(self, msg):
        raise AssertionError("skip: " + msg)


@check
def r30_new_lab_families_are_registered_declared_and_in_the_proving_lab_tier():
    import cases, case_settings, fnmatch
    names = [c["name"] for c in cases.CASES]
    for fam in NEW_FAMILIES:
        arms = [n for n in names if n.startswith(fam + ".")]
        assert arms, "family %s has no registered arm" % fam
        assert all(n.count(".") == 3 for n in arms), arms
        assert case_settings.declared(arms[0]), "%s.* has no declaration in case_settings.py" % fam
        assert any(fnmatch.fnmatchcase(arms[0], g) for g in cases.PROVING_LAB), "%s is in no PROVING_LAB glob (a lab family nobody plays)" % fam
    # the matrix the other families use: shell x latency x helper, and only the arms a family declares
    assert {n for n in names if n.startswith("lab_connect_typeahead.")} == {
        "lab_connect_typeahead.%s.%s.%s" % (s, l, h) for s in ("bash", "zsh") for l in ("lan", "150") for h in ("nohelper", "helper")}
    assert {n for n in names if n.startswith("lab_remote_session.")} == {"lab_remote_session.bash.lan.nohelper", "lab_remote_session.bash.150.nohelper"}
    assert {n for n in names if n.startswith("lab_msys_ssh_switch.")} == {"lab_msys_ssh_switch.bash.lan.nohelper"}


@check
def r30_windows_fake_sshd_cases_moved_to_the_lab():
    import cases, winmap
    names = {c["name"] for c in cases.CASES}
    labs = {n.split(".")[0] for n in names if n.startswith("lab_")}
    assert set(winmap.LAB_MOVED) <= names, sorted(set(winmap.LAB_MOVED) - names)
    for case, fam in winmap.LAB_MOVED.items():
        assert fam.split(" ")[0] in labs, "%s points at %s, which is not a registered lab family" % (case, fam)
    # the six that failed every round (r29a triage) and the ones found next to them
    for n in ("ssh_gitbash_typeahead_managed", "ssh_connect_typing_wrapper", "remote_session_legacy", "remote_tab_no_helper", "remote_tab_enter_no_helper"):
        assert n in winmap.LAB_MOVED, n
    # ssh_gitbash_failing_not_local_managed talks to a hanging LOCAL listener (no sshd at all): it stays in the app job
    assert "ssh_gitbash_failing_not_local_managed" not in winmap.LAB_MOVED and "ssh_failing_not_local" not in winmap.LAB_MOVED
    old = os.environ.pop(winmap.FAKESSHD_BY_HAND_ENV, None)
    try:
        for n, fam in winmap.LAB_MOVED.items():
            why = winmap.skip_reason(n, "gitbash")
            assert why and fam in why and "app-test-lab-windows" in why and winmap.FAKESSHD_BY_HAND_ENV in why, (n, why)
        # the PowerShell pane keeps its BOTH cases: no lab family runs there (lab_skip_reason), so skipping would lose them silently
        assert winmap.skip_reason("ssh_connect_typing", "powershell") is None and winmap.skip_reason("ssh_enter_then_immediate_command", "powershell") is None
        os.environ[winmap.FAKESSHD_BY_HAND_ENV] = "1"      # by hand: runnable as before
        assert all(winmap.lab_moved_reason(n, "gitbash") is None for n in winmap.LAB_MOVED)
        assert winmap.skip_reason("ssh_gitbash_managed", "gitbash") is None and winmap.skip_reason("remote_session_legacy", "gitbash") is None
    finally:
        os.environ.pop(winmap.FAKESSHD_BY_HAND_ENV, None)
        if old is not None:
            os.environ[winmap.FAKESSHD_BY_HAND_ENV] = old
    # the lab arms themselves run in the Git Bash pane only
    assert winmap.skip_reason("lab_connect_typeahead.bash.lan.nohelper", "gitbash") is None and winmap.skip_reason("lab_connect_typeahead.bash.lan.nohelper", "powershell")


@check
def r30_lab_runner_switches_the_msys_managed_ssh_on_in_the_git_bash_pane():
    import cases, cases_lab as CL, cases_caseremote as CR, lab as L, profile as P
    import tempfile
    typed, ran = [], []
    be = types.SimpleNamespace(type=lambda text, delay, enter=False: typed.append(text))

    class FakeLab:
        info = {"flavor": "full"}

        def lab_helper(self, *a):
            pass

        def helper(self, *a, **k):
            pass

        def clear_fault(self):
            pass

        def fault(self, **k):
            pass

        def measure_rtt(self, n=15):
            return [0.5] * n

        def clock_offset(self):
            return 0.0

    d = tempfile.mkdtemp()
    real_sleep, CR.time.sleep = CR.time.sleep, lambda s: None
    try:
        prof = P.Profile(d)
        local = L.LocalRemote(prof)
        arm = dict(name="lab_x.bash.lan.nohelper", user="u_bash", shell="bash", helper="nohelper", lat_ms=0, flavor="full")
        for win, pane, want in ((True, "gitbash", ["export PHOSPHOR_MSYS_MANAGED_SSH=1", "body", "unset PHOSPHOR_MSYS_MANAGED_SSH"]), (True, "powershell", ["body"]), (False, None, ["body"])):
            del typed[:]
            ctx = types.SimpleNamespace(lab=FakeLab(), lab_why=None, prof=prof, remote=local, arm=None, lab_helper_mode="nohelper", win=win, pane=pane, be=be)
            CL._runner(lambda c, rr, a: typed.append("body"), arm)(ctx, _R())
            assert typed == want, (win, pane, typed)
            assert ctx.remote is local and ctx.arm is None
    finally:
        CR.time.sleep = real_sleep


@check
def r30_wrapper_facts_and_judge_managed():
    import cases_lab_winport as WP
    win_log = "\n".join(["x hop=ssh_wrapper announced reclaimed=true", "x hop=ssh_wrapper sync stage=auth n=1", "x hop=ssh_wrapper sync stage=auth_done n=2",
                         "x hop=ssh_wrapper sync stage=managed n=3", "x hop=ssh_wrapper verdict managed=true", "x hop=ssh_wrapper typeahead_front adopted=true n=4"])
    f = WP.wrapper_facts(win_log)
    assert f["stages"] == ["auth", "auth_done", "managed"] and f["managed"] and f["verdict_true"] and f["announced"] and not f["ignored"] and f["front_adopted"] == 1 and not f["front_refused"], f
    r = _R()
    WP.judge_managed(r, f, "t", True, True)
    assert r.fails == [], r.fails
    # each failure the old fake-sshd cases caught is still caught
    bad = win_log.replace("stage=managed", "stage=plain").replace("verdict managed=true", "verdict managed=false") + "\nx hop=ssh_wrapper sync_ignored stage=x\nx hop=ssh_wrapper typeahead_front adopted=false"
    r = _R()
    WP.judge_managed(r, WP.wrapper_facts(bad), "t", True, True)
    joined = " | ".join(r.fails)
    assert "stages" in joined and "managed=true" in joined and "ignored" in joined and "front of the hold" in joined and "not managed" in joined, r.fails
    r = _R()
    WP.judge_managed(r, WP.wrapper_facts("nothing"), "t", False, True)       # Linux / macOS: managed hook required, announcement required when the job pins the wrapper
    assert any("not managed" in m for m in r.fails) and any("announcement" in m for m in r.fails), r.fails
    r = _R()
    WP.judge_managed(r, WP.wrapper_facts("Received ManagedSSHSessionStarting hook"), "t", False, False)
    assert r.fails == [], r.fails


@check
def r30_raw_program_line_and_expected_bytes():
    import cases_lab_winport as WP
    (text, rest), want = WP.raw_keys_expected("abc123")
    assert text == "ihello remote abc123" and rest == ":wq" and want == b"ihello remote abc123\x1b:wq\r" and len(want) == len(text) + 1 + len(rest) + 1
    line = WP.raw_program_line("rs_lab_raw.txt", len(want))
    assert "head -c %d" % len(want) in line and "stty raw -echo" in line and "stty sane" in line and "timeout --foreground 25" in line and "?1049h" in line and "?1049l" in line
    assert line.startswith("sh -c '") and line.endswith("'") and "!" not in line and "$HOME/rs_lab_raw.txt" in line
    assert line.count("'") == 2, "a single quote inside the quoted program would end it"


@check
def r30_raw_program_reads_the_tty_from_an_interactive_shell():
    """#2217: run the stand-in line in a REAL pty under an interactive bash, type the pager keys, and require the program to receive them (GNU timeout without
    --foreground made `head` a background process-group member: SIGTTIN, empty file, keys in the shell line). Skipped where there is no pty or GNU timeout."""
    import cases_lab_winport as WP, subprocess, tempfile, time, select
    try:
        import pty
        if "--foreground" not in subprocess.run(["timeout", "--help"], capture_output=True, text=True).stdout:
            return
    except Exception:
        return
    d = tempfile.mkdtemp()
    pid, fd = pty.fork()
    if pid == 0:
        os.environ["HOME"] = d
        os.execvp("bash", ["bash", "--norc", "--noprofile", "-i"])
    try:
        time.sleep(0.5)
        os.write(fd, (WP.raw_program_line("out.txt", 3, secs=6) + "\n").encode())
        time.sleep(1.0)
        os.write(fd, b" Gq")
        end = time.time() + 5
        got = b""
        while time.time() < end and not got:
            time.sleep(0.2)
            try:
                got = open(os.path.join(d, "out.txt"), "rb").read()
            except OSError:
                pass
        assert got == b" Gq", "the stand-in stored %r (keys did not reach it)" % got
    finally:
        try:
            os.write(fd, b"exit\n")
        except OSError:
            pass
        try:
            os.kill(pid, 9)
        except OSError:
            pass
        try:
            os.waitpid(pid, 0)
        except OSError:
            pass


@check
def r30_tab_lines_keep_only_the_cd_of_the_fixture():
    import cases_lab_winport as WP, cases_lab as CL
    recs = ["cd /work/u/tnh_a/phxabc_only/", "pwd >> /work/u/f", "cd /work/u/tnh_a/phxxyz", "cd /work/u"]
    assert WP.tab_lines(recs, "abc") == ["cd /work/u/tnh_a/phxabc_only/"]
    assert WP.tab_lines(recs, "xyz") == ["cd /work/u/tnh_a/phxxyz"]
    ok, _ = CL.tab_enter_received(WP.tab_lines(recs, "abc"), "/work/u/tnh_a/phxabc_only")
    assert ok
    ok, _ = CL.tab_enter_received(WP.tab_lines(recs, "xyz"), "/work/u/tnh_a/phxxyz_only")
    assert not ok, "the uncompleted prefix must fail the Tab + Enter judge"


@check
def r30_after_job_removes_the_lab_key_and_lab_stop_says_why():
    import lab as L, tempfile, json
    root = os.path.normpath(os.path.join(HERE, "..", "windows", "ci", "after_job.ps1"))
    assert '"labssh"' in open(root, encoding="utf-8").read(), "after_job.ps1 must remove run\\labssh (the lab lease's private key)"
    d = tempfile.mkdtemp()
    assert L.stop_from_json(d) is False and "no readable lab.json" in L.LAST_STOP_WHY
    json.dump({"mode": "fixed", "slot": "windows"}, open(os.path.join(d, "lab.json"), "w"))
    assert L.stop_from_json(d) is False and "no lease owner" in L.LAST_STOP_WHY
    json.dump({"mode": "fixed", "slot": "windows", "owner": "abc"}, open(os.path.join(d, "lab.json"), "w"))
    saved = {k: os.environ.pop(k, None) for k in ("SSHLAB_BOX", "APPCI_LAB_BOX", "SSHLAB_TOKEN")}
    try:
        orig = L.ci_env.get
        L.ci_env.get = lambda k, d=None: (os.environ.get(k) or d or "") if k not in ("SSHLAB_BOX", "APPCI_LAB_BOX", "SSHLAB_TOKEN") else os.environ.get(k, "")
        try:
            assert L.stop_from_json(d) is False and "SSHLAB_BOX" in L.LAST_STOP_WHY and "cannot release" in L.LAST_STOP_WHY, L.LAST_STOP_WHY
        finally:
            L.ci_env.get = orig
    finally:
        for k, v in saved.items():
            if v is not None:
                os.environ[k] = v
