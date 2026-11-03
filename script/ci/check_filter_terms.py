#!/usr/bin/env python3
# script/ci/check_filter_terms.py FILTER LOG: after a FILTERED nextest run (test-linux with TEST_FILTER), fail when a
# `test(/REGEX/)` term of the filter matched NO test that ran (#1504). A derived term that selects nothing (a changed file
# whose tests live under another module path) is a green that tested nothing for that file: false confidence.
# LOG is nextest's output (ANSI colour is stripped here). Every test that ran prints one status line
# "    PASS [   0.1s] (n/m) <binary-id> <test name>" (also FAIL, SIGABRT, TIMEOUT, LEAK, "TRY 2 FAIL", ...).
# Exit 0: every term matched at least one test (or the filter has no test(/.../) terms). Exit 1: a term matched none.
# Exit 2: usage, or the log shows no test at all (check_test_failures reports that run as failed already).
# Self-test: check_filter_terms.py --self-test
import re, sys

STATUS = re.compile(r"^\s*(?:TRY \d+ )?[A-Z]+(?:-[A-Z]+)? +\[ *[0-9.]+s\] +(?:\(\s*\d+/\d+\) +)?(\S+) +(\S+)")
TERM = re.compile(r"test\(/(.*?)/\)")


def ran_tests(text):
    text = re.sub(r"\x1b\[[0-9;]*[mGKHF]", "", text)
    return {m.group(2) for m in map(STATUS.match, text.splitlines()) if m}


def unmatched(filt, names):
    return [t for t in TERM.findall(filt) if not any(re.search(t, n) for n in names)]


def self_test():
    log = ("\x1b[32m        PASS\x1b[0m [   0.120s] (1/3) warp terminal::input_queue::tests::a\n"
           "  TRY 2 FAIL [   1.000s] (2/3) warp terminal::view::tests::b\n"
           "     SIGABRT [   0.500s] (3/3) warp editor::render::tests::c\n")
    names = ran_tests(log)
    ok = names == {"terminal::input_queue::tests::a", "terminal::view::tests::b", "editor::render::tests::c"}
    f = "(package(warp) & (test(/terminal::(input_queue|pane_key)/) | test(/blockgrid_thing::foo/) | test(/render/)))"
    ok = ok and unmatched(f, names) == ["blockgrid_thing::foo"] and unmatched("package(warp)", names) == []
    print("self-test: %s" % ("all passed" if ok else "FAILED"))
    return 0 if ok else 1


def main():
    if sys.argv[1:] == ["--self-test"]:
        return self_test()
    if len(sys.argv) != 3:
        sys.exit("usage: check_filter_terms.py FILTER NEXTEST_LOG")
    filt, log = sys.argv[1], sys.argv[2]
    if not TERM.search(filt):
        return 0
    names = ran_tests(open(log, errors="replace").read())
    if not names:
        print("check_filter_terms: no test status lines in %s; nothing to judge (the run itself failed?)" % log)
        return 2
    bad = unmatched(filt, names)
    for t in bad:
        print("ERROR check_filter_terms: test(/%s/) selected ZERO of the %d tests that ran: the change it was derived from was "
              "not tested. Fix the mapping in script/ci/ci_seq.py (INPUT_PATH_RULES) or pass --tests." % (t, len(names)))
    if not bad:
        print("check_filter_terms: all %d test(/.../) term(s) selected tests (%d ran)" % (len(TERM.findall(filt)), len(names)))
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
