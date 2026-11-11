# appci/selftest_registered.py: the guard for #2086. selftest.py runs only the functions that carry `@check`; six check functions had lost that line
# in hand merges of the file's tail and never ran. This check fails when an asserting zero-argument function of a selftest*.py file is not registered.
# selftest.py appends CHECKS to its own list, so `run.py --self-test` and `selftest.py` both run it.
import ast, os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


# #2086: a zero-argument function with assertions in a self-test file is a check; one without `@check` never runs. Six were found on integ/r14
# (the decorator line lost in hand merges of the files' tails). A name here is a KNOWN unregistered check, with the issue that tracks it.
KNOWN_UNREGISTERED = {"selftest.py": {"lab_remote_edit_pieces": "#2086: fails when called (it expects lab_remote_edit arms for pwsh; the registry has none): the lab owner decides"}}


def unregistered_checks(path):
    """names of the top-level zero-argument functions of a self-test file that assert something and are not decorated with `check`"""
    tree = ast.parse(open(path, encoding="utf-8").read())
    out = []
    for n in tree.body:
        if not isinstance(n, ast.FunctionDef) or n.args.args or n.args.kwonlyargs or n.args.vararg or n.args.kwarg or n.name == "main":
            continue
        decorated = any(isinstance(d, ast.Name) and d.id == "check" for d in n.decorator_list)
        if not decorated and any(isinstance(x, ast.Assert) for x in ast.walk(n)):
            out.append(n.name)
    return out


@check
def every_check_function_is_registered_2086():
    """no self-test function is defined and silently not run: every asserting zero-argument function of the self-test files carries `@check`, except
    the ones listed in KNOWN_UNREGISTERED with their issue (and a listed one that has since been registered must leave the list)"""
    files = sorted(f for f in os.listdir(HERE) if f.startswith("selftest") and f.endswith(".py"))
    assert "selftest.py" in files and "selftest_registered.py" in files and "selftest_pointer.py" in files
    for f in files:
        known = KNOWN_UNREGISTERED.get(f, {})
        loose = unregistered_checks(os.path.join(HERE, f))
        new = [n for n in loose if n not in known]
        assert not new, "%s: defined but never run (no @check line; a merge of the file's tail may have dropped it): %s" % (f, new)
        gone = [n for n in known if n not in loose]
        assert not gone, "%s: %s is registered (or removed) now: take it out of KNOWN_UNREGISTERED" % (f, gone)
    # the guard itself: it sees a lost decorator and ignores helpers
    import tempfile
    d = tempfile.mkdtemp()
    p = os.path.join(d, "selftest_x.py")
    with open(p, "w") as fh:
        fh.write("def check(f):\n    return f\n\n@check\ndef a():\n    assert 1\n\ndef lost():\n    assert 1\n\ndef helper(x):\n    assert x\n\ndef no_asserts():\n    return 1\n")
    assert unregistered_checks(p) == ["lost"]
    os.remove(p)
    os.rmdir(d)
    import selftest
    names = {f.__name__ for f in selftest.CHECKS}
    for n in ("wrapped_copy_and_vertical_tabs_pure_parts_1976_1990", "r14_w1a_wave1_cases_pure_parts_and_registry", "r14_w1b_pure_parts", "r14_w2b_state_cases",
              "r14_window_use_cases_pure_parts"):
        assert n in names, n
