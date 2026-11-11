# appci/selftest_r22_labcases.py: offline self-tests of round 22, the lab cases (#2160 #2163 #2164 #2165): the plain-session gate on every lab case that judges a
# managed-session feature, the self-match-proof daemon count, the helper-takeover wait and per-channel evidence, the chips OCR read. Pure Python: no app, no lab.
# selftest.py appends CHECKS to its own list (two lines at its end), so `run.py --self-test` runs them.
import inspect, os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


def _src(name):
    return open(os.path.join(HERE, name), encoding="utf-8").read()


def _body(fname, func):
    """the source text of one top-level def in a file (up to the next top-level statement)"""
    import re
    t = _src(fname)
    m = re.search(r"^def %s\(.*?(?=^\S)" % func, t, re.S | re.M)
    assert m, (fname, func)
    return m.group(0)


@check
def r22_every_managed_session_lab_case_applies_the_plain_session_gate():
    """#2160: each lab case that needs a managed session calls plain_gate (which reads the connect's own app log); the list is the one in cases_lab"""
    import cases  # noqa: F401
    import cases_lab as CL
    want = {"c_chips", "c_no_helper_chip", "c_tab_remote_dir", "c_lab_drop", "c_nu_agent_pager_env", "fam_body", "c_codebase_index_remote"}
    assert set(CL.MANAGED_SESSION_CASES) == want, set(CL.MANAGED_SESSION_CASES) ^ want
    for func, fname in sorted(CL.MANAGED_SESSION_CASES.items()):
        body = _body(fname, func)
        assert "plain_gate(" in body or "plain_session_feature_limit(" in body, "%s does not apply the plain-session gate" % func


@check
def r22_gate_limits_fish_and_nu_only_and_reads_the_log():
    import cases  # noqa: F401
    import cases_lab as CL
    plain = "x\n" + CL.PLAIN_SESSION_HOOK + "\nhop=ssh_wrapper verdict managed=false\n"
    for sh in ("fish", "nu"):
        why = CL.plain_session_feature_limit(sh, plain, "f")
        assert why and "#2019" in why and "DECLINED.md" in why, sh
        assert CL.plain_session_feature_limit(sh, plain + "hop=ssh_wrapper verdict managed=true\n", "f") is None, "a managed session is judged"
        assert CL.plain_session_feature_limit(sh, "", "f") is None, "no evidence in the log: judged"
    for sh in ("bash", "zsh"):
        assert CL.plain_session_feature_limit(sh, plain, "f") is None, "bash/zsh stay fully judged"


class _R:
    def __init__(self):
        self.rows = []

    def metric(self, *a):
        self.rows.append(("metric",) + a)

    def unverifiable(self, m):
        self.rows.append(("unverifiable", m))

    def note(self, m):
        self.rows.append(("note", m))


@check
def r22_plain_gate_marks_unverifiable_and_leaves_the_session():
    import cases  # noqa: F401
    import cases_lab as CL
    left, r = [], _R()

    class Ctx:
        def __init__(self, log):
            self.log = log

        def log_since(self, off):
            return self.log

    orig = CL.finish_remote
    CL.finish_remote = lambda ctx, rr: left.append(1)
    try:
        assert CL.plain_gate(Ctx(CL.PLAIN_SESSION_HOOK), r, {"shell": "nu"}, 0, "f") is True
        assert left and any(x[0] == "unverifiable" for x in r.rows)
        left.clear()
        r2 = _R()
        assert CL.plain_gate(Ctx(CL.PLAIN_SESSION_HOOK), r2, {"shell": "bash"}, 0, "f") is False
        assert not left and not r2.rows
    finally:
        CL.finish_remote = orig


@check
def r22_daemon_pattern_does_not_match_its_own_wrapper():
    """#2163: the pattern, as an ERE, matches a daemon's command line and not the wrapper `bash -c` that carries the pattern text"""
    import re
    import cases  # noqa: F401
    import cases_lab_helper as H
    pat = re.compile(H.DAEMON_PATTERN)
    assert pat.search("/home/u/.phosphor/bin/phosphor-remote-linux-x86_64 --socket x daemon")
    wrapper = "bash -c pgrep -u $(id -u) -f '%s'; true" % H.DAEMON_PATTERN
    assert not pat.search(wrapper), "the wrapper's own command line must not count"
    assert H.parse_pids("123\n45\nx\n") == [45, 123] and H.parse_pids("") == []


@check
def r22_daemon_cases_kill_first_and_compare_pids():
    import cases  # noqa: F401
    t = _src("cases_lab_helper.py")
    for f in ("c_reconnect", "c_helper_killed", "c_idle_exit"):
        assert "kill_daemons(" in _body("cases_lab_helper.py", f), f + " must kill leftover daemons before it connects"
    rc = _body("cases_lab_helper.py", "c_reconnect")
    assert "pids0 == pids1" in rc and "len(pids1) == 1" in rc, "exactly 1 AND the same daemon pid"
    assert rc.index("kill_daemons(") < rc.index("connect_or_fail(")


@check
def r22_helper_takeover_state_reads_the_app_log():
    """#2164"""
    import cases  # noqa: F401
    import cases_lab as CL
    swap = "Remote-server client connected after session init; command executor swapped from the ControlMaster\n"
    assert CL.helper_takeover_state("") == (False, 0)
    assert CL.helper_takeover_state(swap) == (True, 0)
    assert CL.helper_takeover_state(swap + "hop=remote_index_sync start reason=X\n") == (True, 1), "a sync started and not finished"
    assert CL.helper_takeover_state(swap + "hop=remote_index_sync start reason=X\nhop=remote_index_sync done files=1 fragments=1 embedded=1 already=0 stale=0\n") == (True, 0)
    assert CL.helper_takeover_state("hop=remote_index_sync start x\nhop=remote_index_sync skipped reason=no_provider\n") == (False, 0)
    note = CL.helper_takeover_note(swap + "hop=backend_route family=git_read backend=shell reason=r\n")
    assert "shell_routes_after_swap=1" in note


@check
def r22_takeover_wait_is_event_driven_and_never_widens_limits():
    import cases  # noqa: F401
    import cases_lab as CL
    r = _R()
    swap = "command executor swapped from the ControlMaster\n"

    class Ctx:
        def __init__(self, logs):
            self.logs, self.i = logs, 0

        def log_since(self, off):
            v = self.logs[min(self.i, len(self.logs) - 1)]
            return v

        def until(self, fn, t, step=0.1):
            for _ in range(5):
                self.i += 1
                if fn():
                    return True
            return False
    assert CL.wait_helper_takeover(Ctx([""]), r, {"helper": "nohelper"}, 0) is True, "no-helper arms do not wait"
    assert CL.wait_helper_takeover(Ctx([""]), r, {"helper": "helper"}, 0, quiet=0) is False, "no swap: not settled, noted, case goes on"
    assert CL.wait_helper_takeover(Ctx(["", swap]), r, {"helper": "helper"}, 0, quiet=0) is True
    assert CL.wait_helper_takeover(Ctx([swap + "hop=remote_index_sync start r\n"]), r, {"helper": "helper"}, 0, quiet=0) is False, "a pending sync"
    assert any(x[0] == "note" and "NOT settled" in x[1] for x in r.rows)
    for fn in ("c_chips", "c_path_detect"):
        assert "wait_helper_takeover(" in _body("cases_lab.py", fn), fn
    assert "wait_helper_takeover(" in _body("cases_lab_helper.py", "fam_body")


@check
def r22_channel_evidence_has_per_channel_timestamps():
    import cases  # noqa: F401
    import cases_lab as CL
    data = "Starting session: command for u from 1\nClose session\nStarting session: shell on pts/0 for u\nStarting session: command for u\n"
    assert CL.channel_timeline(data) == ["command", "shell", "command"]
    assert CL.watch_timeline([(10.0, 0, 0), (10.5, 1, 2), (11.0, 3, 5)], 10.0) == [(0.5, 2), (1.0, 5), (1.0, 5)]
    for fn in ("c_chips", "c_path_detect"):
        assert "ChannelWatch(" in _body("cases_lab.py", fn) and ".stop(r)" in _body("cases_lab.py", fn), fn
    assert "ChannelWatch(" in _body("cases_lab_helper.py", "fam_body")


@check
def r22_chips_read_by_find_token_and_the_chip_value_line():
    """#2165: the branch is found with cases_ui.find_token (one glyph of tolerance), the app's update_chip_value line is required, and the OCR text and shot
    carry the arm in their names"""
    import cases  # noqa: F401
    import cases_lab as CL
    import cases_ui as U
    b = _body("cases_lab.py", "c_chips")
    assert "U.find_token(" in b and "chip_value_logged(" in b and "ocr_evidence(" in b and "arm_tag(arm)" in b
    br = "brabcdef123456"
    assert CL.chip_value_logged("[chips] update_chip_value ShellGitBranch -> Some(Text(\"%s\"))" % br, br)
    assert not CL.chip_value_logged("[chips] update_chip_value ShellGitBranch -> Some(Text(\"main\"))", br)
    assert not CL.chip_value_logged("[chips] update_chip_value ShellCwd -> %s" % br, br), "another chip"
    ws = [("brabcdef12345G", 1, 1, 0, 1, 0, 1)]
    assert U.token_hits(ws, br), "one misread glyph is tolerated by find_token"
    assert not U.token_hits([("brabcdXX123456", 1, 1, 0, 1, 0, 1)], br), "two are not"
    assert CL.arm_tag({"shell": "zsh", "lat": "150", "helper": "helper"}) == "zsh.150.helper"
    assert "ocr_evidence(" in _body("cases_lab.py", "c_no_helper_chip")
