# appci/selftest_r26_wintests.py: offline self-tests of the r24a Windows test-fault fixes. Pure Python, no display.
import os, re, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


def _case(name):
    import cases
    import cases_r14_w1a, cases_caseagent, cases_shells, cases_subshell_ready  # noqa: F401  (register their cases)
    return next(c for c in cases.CASES if c["name"] == name)


@check
def r26_routing_tables_declare_enters_agent():
    """the routing tables send prose to the agent on purpose, so the post-case 'entered agent view' check must not fail them"""
    for n in ("routing_table_powershell", "routing_table"):
        assert _case(n)["enters_agent"] is True, n


@check
def r26_md_diagram_fits_the_viewer():
    """the mermaid fixture is a left-to-right flowchart (3 nodes in one row), not a tall top-down one that falls below the 565 px viewer; every node label is still checked"""
    import cases
    assert "graph LR\n    A[AlphaNodeStart] --> B[BetaNodeMiddle]" in cases.MD_SMALL
    assert "graph TD" not in cases.MD_SMALL
    assert cases.MD_SMALL_DIAGRAM == ("alphanodestart", "betanodemiddle", "gammanodeend")


@check
def r26_console_host_only_job_is_no_verdict():
    """r24a pwsh7: only openconsole.exe listed under the app is no verdict (the identity probe judges); a wrong shell still fails"""
    import winmap
    assert winmap.variant_shell_problem("pwsh7", ["OpenConsole.exe"]) is None
    assert winmap.variant_shell_problem("pwsh7", ["openconsole.exe", "conhost.exe"]) is None
    assert winmap.variant_shell_problem("pwsh7", ["pwsh.exe", "openconsole.exe"]) is None
    assert winmap.variant_shell_problem("cmd", ["openconsole.exe", "powershell.exe"]) is not None      # wrong shell: still a fail
    assert "pwsh.exe" in winmap.variant_shell_problem("pwsh7", ["openconsole.exe", "cmd.exe"])         # a real non-host process, no pwsh: still a fail
    assert "none of" in winmap.variant_shell_problem("wsl", ["conhost.exe"]), "the wsl pass stays strict"


@check
def r26_pwsh7_identity_probe_logs_path():
    """startup_shell_pane keeps failing a wrong identity and records the resolved pwsh path"""
    src = open(os.path.join(HERE, "cases_shells.py")).read()
    assert "identity_ok(variant, got)" in src and "MainModule.FileName" in src and "pwsh7 pane shell resolved to" in src
    import cases_shells
    assert cases_shells.identity_ok("pwsh7", ["PS7_Core"]) and not cases_shells.identity_ok("pwsh7", ["PS5_Desktop"])


@check
def r26_rc_clear_names_2025_and_keeps_budget():
    src = open(os.path.join(HERE, "cases_subshell_ready.py")).read()
    assert "r.check(dt < 2.0," in src, "the 2 s budget stays"
    import cases_subshell_ready as S
    assert "#2025" in S.WIN_BOOTSTRAP_NOTE and "4760" in S.WIN_BOOTSTRAP_NOTE and "unchanged" in S.WIN_BOOTSTRAP_NOTE


@check
def r26_completions_case_records_setup_evidence():
    src = open(os.path.join(HERE, "cases_ui.py")).read()
    assert "hop=suggestions_mode" in src and "fixture dir" in src
