# appci/selftest_r31_2220.py: offline self-test of #2220: every ambiguous_want( call in the cases files passes installed=, and the PowerShell routing
# case asks the pane (Get-Command) instead of the harness Python's PATH. Pure Python.
import os, re, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


def _call_args(src, start):
    """the text of the parenthesised argument list that opens at src[start] == '('"""
    depth = 0
    for i in range(start, len(src)):
        depth += {"(": 1, ")": -1}.get(src[i], 0)
        if depth == 0:
            return src[start:i + 1]
    return src[start:]


def unguarded_calls(src):
    """[(line number, call text)] of every `ambiguous_want(` CALL in src (not its def) that lacks `installed=`"""
    bad = []
    for m in re.finditer(r"ambiguous_want\(", src):
        if src[:m.start()].rstrip().endswith("def"):
            continue
        args = _call_args(src, m.end() - 1)
        if "installed=" not in args:
            bad.append((src.count("\n", 0, m.start()) + 1, args))
    return bad


@check
def r31_2220_ambiguous_want_always_asks_installed():
    """#2220: a call without installed= defaults to True and expects shell for a head the pane may not have (make a PR -> agent in r30d)."""
    names = sorted(f for f in os.listdir(HERE) if f.startswith("cases") and f.endswith(".py"))
    assert "cases_r14_w1a.py" in names and "cases_caseagent.py" in names
    seen = 0
    for n in names:
        src = open(os.path.join(HERE, n), encoding="utf-8").read()
        seen += len(re.findall(r"ambiguous_want\(", src))
        bad = unguarded_calls(src)
        assert not bad, "%s: ambiguous_want( without installed= at line %s" % (n, bad[0][0])
    assert seen >= 3, "the scan found no calls: the pattern is stale"
    assert unguarded_calls("x = CA.ambiguous_want(l, s, r)\n") and not unguarded_calls("x = ambiguous_want(l, s, r, installed=f(l))\n")
    assert not unguarded_calls("def ambiguous_want(line, surface, rule, installed=True):\n")


@check
def r31_2220_powershell_case_asks_the_pane():
    import cases_r14_w1a as w
    import inspect
    body = inspect.getsource(w.c_routing_table_powershell)
    assert "ps_head_installed(" in body and "installed=bool(heads" in body
    probe = inspect.getsource(w.ps_head_installed)
    assert "Get-Command" in probe and "shutil" not in probe and "which(" not in probe
    doc = open(os.path.join(HERE, "..", "..", "..", "docs", "testing.md"), encoding="utf-8").read()
    assert "#2220" in doc
