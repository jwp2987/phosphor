# appci/selftest_r19_macallow.py: offline self-tests of the macOS Local Network auto-answer (owner decision 2026-11-11).
# Every ad-hoc-signed CI build re-asks "Allow "Phosphor" to find devices on local networks?" and the owner cannot click it
# remotely. With APPCI_MAC_AUTO_ALLOW_LOCAL_NETWORK=1 (the Mac runner's machine-local ci.env only) the harness answers exactly
# that question with Allow. Pure Python: no app, no display, no osascript. selftest.py appends CHECKS to its own list.
import os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import backend  # noqa: E402

CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


Q = ["Allow “Phosphor” to find devices on local networks?",
     "This will allow the app to discover, connect to, and collect data from devices on your networks."]
B = ["Don’t Allow", "Allow"]


@check
def r19_mac_local_network_prompt_is_recognised_exactly():
    """the judge accepts the exact Local Network question about Phosphor and nothing else: another app's question, another
    kind of prompt, other buttons, or an empty read are all refused (they stay NEEDS OWNER)"""
    judge = backend.MacBackend.is_phosphor_local_network_prompt
    assert judge(Q, B)
    assert judge(["Allow \"Phosphor\" to find devices on local networks?"], ["Don't Allow", "Allow"])
    assert not judge([Q[0].replace("Phosphor", "Terminal"), Q[1]], B), "another app's question"
    assert not judge(["“Phosphor” would like to send you notifications."], B), "another kind of prompt"
    assert not judge(["“Phosphor” wants to access your contacts."], ["Don’t Allow", "OK"]), "TCC prompt"
    assert not judge(Q, ["Allow"]), "not the two-button question"
    assert not judge(Q, ["Don’t Allow", "Allow", "Open System Settings"]), "a third button"
    assert not judge([""], [""]), "an empty read"


@check
def r19_mac_auto_allow_is_off_unless_the_machine_switch_is_on():
    """without APPCI_MAC_AUTO_ALLOW_LOCAL_NETWORK=1 the harness never clicks; with it, only for UserNotificationCenter"""
    import ci_env
    b = backend.MacBackend.__new__(backend.MacBackend)
    old = os.environ.pop(b.AUTO_ALLOW_ENV, None)
    real_file = ci_env._file
    ci_env._file = lambda: {}  # this machine's ci.env must not decide the test
    try:
        assert b.auto_allow_local_network("UserNotificationCenter") is False
        os.environ[b.AUTO_ALLOW_ENV] = "0"
        assert b.auto_allow_local_network("UserNotificationCenter") is False
        os.environ[b.AUTO_ALLOW_ENV] = "1"
        for owner in ("SecurityAgent", "CoreServicesUIAgent", "UniversalAccessAuthWarn"):
            assert b.auto_allow_local_network(owner) is False, owner
        # the machine-local file is how the Mac runner turns it on (not exported into the job's environment)
        os.environ.pop(b.AUTO_ALLOW_ENV, None)
        ci_env._file = lambda: {b.AUTO_ALLOW_ENV: "1"}
        assert ci_env.get(b.AUTO_ALLOW_ENV) == "1"
        assert b.auto_allow_local_network("SecurityAgent") is False
    finally:
        ci_env._file = real_file
        os.environ.pop(b.AUTO_ALLOW_ENV, None)
        if old is not None:
            os.environ[b.AUTO_ALLOW_ENV] = old
    src = open(os.path.join(HERE, "backend.py"), encoding="utf-8").read()
    assert "ci_env.get(self.AUTO_ALLOW_ENV)" in src, "read through ci_env (the Mac's ci.env is not exported)"
    assert 'click button "Allow" of window 1' in src and "is_phosphor_local_network_prompt(texts, buttons)" in src
    assert src.index("is_phosphor_local_network_prompt(texts, buttons)") < src.index('click button "Allow" of window 1'), \
        "the question is judged before anything is clicked"
