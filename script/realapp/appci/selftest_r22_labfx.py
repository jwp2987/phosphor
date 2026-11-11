# appci/selftest_r22_labfx.py: offline self-tests of round 22 lab fixes #2157 #2158 #2159. Pure Python.
import glob, os, re, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


@check
def r22_no_git_tree_without_dir():
    """#2157: every lab.tree(..., git=True) names its own dir, so each fixture is its own repo"""
    n = 0
    for p in sorted(glob.glob(os.path.join(HERE, "cases_*.py"))):
        for i, line in enumerate(open(p, encoding="utf-8"), 1):
            if "lab.tree(" in line and "git=True" in line:
                n += 1
                assert "dir=" in line, "%s:%d lab.tree(git=True) without dir=" % (os.path.basename(p), i)
    assert n >= 8, n   # the scan itself finds the calls
    # a call without dir= would be caught
    assert "dir=" not in 'lab.tree(u, {"x": ""}, git=True)'


@check
def r22_git_apply_command_formats():
    """#2158: the git apply command formats with both flag values and keeps its literal %s"""
    src = open(os.path.join(HERE, "cases_lab_git.py"), encoding="utf-8").read()
    m = re.search(r'^\s*apply = (.+)$', src, re.M)
    assert m, "apply line not found"
    GIT, base64, PATCH = "git", __import__("base64"), "p\n"
    apply = eval(m.group(1))
    for flag in ("", "--reverse "):
        cmd = apply.replace("{flag}", flag)
        assert "apply --cached --recount %s-" % flag in cmd and cmd.startswith("printf %s "), cmd
    assert not re.search(r'apply % ""|apply % "--reverse', src), "apply is still %-formatted"


@check
def r22_codebase_step_names_get_relevant_files():
    import fake_model as FM
    call = FM.plan_call(("codebase", "where is x"))
    assert call["name"] == "get_relevant_files" and call["arguments"] == {"query": "where is x"}, call
    assert "search_codebase" not in open(os.path.join(HERE, "fake_model.py"), encoding="utf-8").read().split("def plan_call")[1].split("def turn_state")[0]
