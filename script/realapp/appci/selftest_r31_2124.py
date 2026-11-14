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
