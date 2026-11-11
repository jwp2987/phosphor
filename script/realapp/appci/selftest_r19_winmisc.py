# appci/selftest_r19_winmisc.py: offline self-tests of the round 19 Windows fixes after the round 14 candidate (docs/design/r19-winmisc.md, #2113 #2114).
# Pure Python: no app, no display. selftest.py appends CHECKS to its own list, so `run.py --self-test` runs them. The exit_nested / leave_nested proof
# of a Git Bash nested in the PowerShell pane is checked in selftest.py (audit2_...), the zero-network launch in selftest_r15_w5a.py.
# Every judge is fed a right input AND the wrong ones it exists to catch.
import inspect, os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.join(HERE, "..", "..", "..")
sys.path.insert(0, HERE)

CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


@check
def r19_lrc_case_waits_for_its_interrupted_command_to_be_logged():
    """#2114: agent_lrc_no_false_takeover's [y/N] command, interrupted in the cleanup, is logged by bash at its NEXT prompt; the case waits for that line
    so the next case's rec.log mark is not taken in front of it (cb_agent_pickers: 'a picker key reached the shell: [the [y/N] line]')"""
    import cases_caseagent as CA
    tok = "LRCYNT1_ABCDEF"
    line = "printf 'Continue? [y/N] '; read ans; echo %s >> lrc_yn.txt" % tok
    assert CA.lrc_command_pending(["echo a >> f"], tok, []) is True, "started, not finished, not logged: still to come"
    assert CA.lrc_command_pending(["echo a >> f", line], tok, []) is False, "logged: nothing to wait for"
    assert CA.lrc_command_pending(["echo a >> f"], tok, [tok]) is False, "it finished on its own (effect file): the shell logged it with that prompt"
    assert CA.lrc_command_pending(["echo a >> f"], None, []) is False, "part 2 never started"
    assert CA.lrc_command_pending([], tok, []) is False, "a pane with no rec.log (PowerShell) has no late line"

    class Prof:
        def __init__(self):
            self.cmds, self.calls = [(0, "echo a >> f")], 0

        def commands(self):
            self.calls += 1
            if self.calls == 3:
                self.cmds.append((1, line))
            return list(self.cmds)

    class Ctx:
        def __init__(self):
            self.prof = Prof()

        def lines(self, name):
            return []

        def until(self, pred, timeout, step=0.05):
            for _ in range(10):
                if pred():
                    return True
            return False
    c = Ctx()
    assert CA.lrc_wait_logged(c, tok) is True and any(tok in x for _t, x in c.prof.cmds), "it waited until the shell logged the line"
    assert CA.lrc_wait_logged(c, tok) is False, "already logged: no wait"
    src = inspect.getsource(CA.c_agent_lrc_no_false_takeover)
    assert src.index('ctx.be.key("ctrl+c")') < src.index("lrc_wait_logged(ctx, tok_y)"), "the wait follows the Ctrl-C that ends the command"


@check
def r19_previous_release_stage_cannot_fail_the_build_under_test():
    """#2114: upgrade_over_previous. A previous release that cannot be started or driven is UNVERIFIABLE (noref), named; once it ran, every check
    on the build under test is a plain check as before"""
    import cases_coverage_b as CB
    assert CB.prev_stage_unverifiable([]) is None
    why = CB.prev_stage_unverifiable(["uo_old: no window / shell after the relaunch within 90 s (alive True)"])
    assert why and "PREVIOUS release" in why and "NOT judged" in why and "uo_old: no window" in why and "NEEDS:" in why
    src = inspect.getsource(CB.c_upgrade_over_previous)
    old_stage, new_stage = src[src.index("R.use_app(ctx, old_app_like("):src.index("before = state_files(ctx)")], src[src.index("before = state_files(ctx)"):]
    assert 'R.start_app(ctx, old, "uo_old"' in old_stage and 'U.exact_burst(ctx, old, "uo_old", 3)' in old_stage and "r.unverifiable(why)" in old_stage
    assert 'R.start_app(ctx, r, "uo_new"' in new_stage and 'U.exact_burst(ctx, r, "uo_new", 4)' in new_stage, "the build under test is still judged by r"
    assert "old" not in [a.strip() for a in new_stage.split("(ctx,")[1:] if a.startswith(" old,")], "no check of the new build goes to the scratch result"


@check
def r19_first_key_after_idle_remeasures_a_suspected_regression():
    """#2114: lat.py re-measures a suspected regression once; the app-log path of latency_first_key_after_idle now hands it the re-measure. A clean
    second set is a WARN (never a pass), a second regressed set stays FAIL"""
    import cases, lat
    src = inspect.getsource(cases.c_lat_first_key_idle)
    assert "remeasure=again" in src and "idle_log_block(ctx, g, n + warm)[0][warm:]" in src
    regress = getattr(lat, "regress")
    v_fail, _dm, _dp = regress(4.04, 3.5)
    v_ok, _dm, _dp = regress(3.55, 3.5)
    assert v_fail == "fail" and v_ok in ("pass", "none"), (v_fail, v_ok)


@check
def r19_powershell_routing_table_has_the_expression_rows_and_docs_name_the_round():
    """#2113: the PowerShell routing table types a .NET static call and a range; both are pinned in the Rust rule's tests; docs/testing.md has the round"""
    import cases_r14_w1a as W
    for line in ("[math]::Sqrt(2)", "[IO.File]::Exists('log.txt')", "1..3"):
        assert line in W.PS_SHELL_LINES and (line, "shell", "shell") in W.ps_routing_table("agent", None), line
    rs = open(os.path.join(REPO, "crates", "input_classifier", "src", "util", "routing_tests.rs"), encoding="utf-8").read()
    assert "powershell_type_and_range_heads_are_shell_syntax_in_a_powershell_session" in rs and '"[math]::Sqrt(2)"' in rs and '"1..300000"' in rs
    rule = open(os.path.join(REPO, "crates", "input_classifier", "src", "util", "routing.rs"), encoding="utf-8").read()
    assert rule.index("is_powershell_expression_head(&first.text)") < rule.index("if first.quoted {"), "decided before the quoted-first-word return"
    doc = open(os.path.join(REPO, "docs", "testing.md"), encoding="utf-8").read()
    assert "Round 19: Windows" in doc and "selftest_r19_winmisc.py" in doc and "#2113" in doc and "#2114" in doc and "RUNNER_AFTER_SCRIPT_TIMEOUT" in doc
