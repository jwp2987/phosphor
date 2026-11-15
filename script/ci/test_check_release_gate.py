#!/usr/bin/env python3
# Offline fixtures for check_release_gate.py (fake GitLab API). Run: python3 script/ci/test_check_release_gate.py
# Wired into the `guards` job (.gitlab-ci.yml) and script/precheck.
import json, os, pathlib, sys, unittest
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import check_release_gate as g

SHA = "a" * 40
PLAIN, APPS = g.required_jobs()


def good(sha=SHA, tier="release", **summary):
    cnt = {"pass": 5, "fail": 0, "skip": 1, "noref": 0}
    cnt.update(summary)
    return {"meta": {"tier": tier, "sha": sha}, "summary": cnt, "cases": []}


class Fake:
    """jobs: [(name, status)] in pipeline 7; results: artifact path -> results.json doc"""
    def __init__(self, jobs, results):
        self.jobs, self.results = jobs, results

    def get(self, path, raw=False):
        if "/pipelines?sha=" in path:
            return [{"id": 7}]
        if "/pipelines/7/jobs" in path:
            return [{"id": 100 + i, "name": n, "status": s} for i, (n, s) in enumerate(self.jobs)]
        if "/artifacts/" in path:
            p = path.split("/artifacts/", 1)[1]
            if p not in self.results:
                raise RuntimeError("404")
            return json.dumps(self.results[p]).encode()
        raise AssertionError(path)


ALL_JOBS = [(n, "success") for n in PLAIN + APPS]
ALL_RES = {p: good() for ps in g.APP_RESULT_PATHS.values() for p in ps}


class Gate(unittest.TestCase):
    def problems(self, jobs=ALL_JOBS, results=ALL_RES):
        return g.check(Fake(jobs, results), "1", SHA)[1]

    def test_pass(self):
        self.assertEqual(self.problems(), [])

    def test_failed_job(self):
        p = self.problems([(n, "failed" if n == "fuzz-smoke" else s) for n, s in ALL_JOBS])
        self.assertEqual(len(p), 1)
        self.assertIn("fuzz-smoke", p[0])

    def test_failed_guard(self):
        self.assertTrue(self.problems([(n, "failed" if n == "guards" else s) for n, s in ALL_JOBS]))

    def test_job_list_comes_from_the_release_tier(self):
        for n in ("test-shuttle-loom", "fuzz-smoke", "input-timers-strict", "licenses", "advisories", "check-linux", "test-macos", "test-windows"):
            self.assertIn(n, PLAIN)
        self.assertNotIn("fuzz-smoke-windows", PLAIN)      # unsupported on Windows
        self.assertFalse([n for n in PLAIN if n.startswith(("bench", "mutants"))])
        self.assertEqual(set(APPS), {"app-test-linux-wayland", "app-test-macos", "app-test-windows"})   # owner 2026-11-12: no X11 in the gate

    def test_wrong_sha(self):
        r = dict(ALL_RES, **{"appci-out/wayland/run/results.json": good(sha="b" * 40)})
        self.assertTrue(any("not " + SHA in x for x in self.problems(results=r)))
        r = dict(ALL_RES, **{"appci-out/wayland/run/results.json": {"meta": {"tier": "release"}, "summary": {"pass": 3}}})   # a pre-gate results.json: no sha
        self.assertTrue(self.problems(results=r))

    def test_wrong_tier(self):
        for tier in ("full", "smoke", None):
            r = dict(ALL_RES, **{"appci-out/macos/run/results.json": good(tier=tier)})
            self.assertTrue(any("not 'release'" in x for x in self.problems(results=r)), tier)

    def test_failed_case(self):
        self.assertTrue(self.problems(results=dict(ALL_RES, **{"appci-out/windows/run/results.json": good(fail=1)})))

    def test_known_accepted(self):
        # an ACCEPTED_FOR_RELEASE shortfall is a note on a PASS row (cases.Result.known): the run is green and so is the gate
        doc = good()
        doc["cases"] = [{"name": "editor_open_cost", "status": "pass", "detail": [], "notes": ["KNOWN: cost 201 MB [ACCEPTED_FOR_RELEASE #1607 v0.1.17: x]"]}]
        self.assertEqual(self.problems(results=dict(ALL_RES, **{"appci-out/macos/run/results.json": doc})), [])
        self.assertIn("KNOWN 1", g.info_counts(doc))

    def test_noref_accepted(self):
        # NOREF is not a pass and not a failure (appci verdict: report.py exits 0); one pass is still required
        self.assertEqual(self.problems(results=dict(ALL_RES, **{"appci-out/windows/run/results.json": good(noref=3)})), [])
        self.assertTrue(self.problems(results=dict(ALL_RES, **{"appci-out/windows/run/results.json": good(noref=3, **{"pass": 0})})))

    def test_judged_nothing(self):
        self.assertTrue(self.problems(results=dict(ALL_RES, **{"appci-out/macos/run/results.json": good(**{"pass": 0, "noref": 0})})))

    def test_missing_platform(self):
        p = self.problems([j for j in ALL_JOBS if j[0] != "app-test-macos"])
        self.assertTrue(any(x.startswith("app-test-macos") for x in p))

    def test_reuse_variant_does_not_count(self):
        self.assertTrue(self.problems([(n + ("-reuse" if n == "app-test-windows" else ""), s) for n, s in ALL_JOBS]))

    def test_expired_artifact(self):
        self.assertTrue(self.problems(results={k: v for k, v in ALL_RES.items() if "xwayland" not in k}))

    def test_no_pipeline(self):
        class Empty(Fake):
            def get(self, path, raw=False):
                return []
        self.assertTrue(g.check(Empty([], {}), "1", SHA)[1])


def issue(iid, *labels, state="opened"):
    return {"iid": iid, "title": "t%d" % iid, "state": state, "labels": list(labels)}


class OpenIssues(unittest.TestCase):
    """owner decision 2026-11-12: no open issue labelled with the release may still be fixed-untested"""
    def test_label_from_tag(self):
        self.assertEqual(g.release_label("v0.2.0"), "v0.2.0")
        self.assertEqual(g.release_label("0.2.0"), "v0.2.0")

    def test_none_passes(self):
        ok, bad = g.judge_issues([], "v0.2.0")
        self.assertEqual(bad, [])
        self.assertTrue(ok)
        self.assertEqual(g.judge_issues([issue(5, "v0.2.0", "verified")], "v0.2.0")[1], [])

    def test_one_fails_naming_it(self):
        _, bad = g.judge_issues([issue(7, "v0.2.0"), issue(1234, "v0.2.0", "fixed-untested")], "v0.2.0")
        self.assertEqual(len(bad), 1)
        self.assertIn("#1234", bad[0])
        self.assertIn("fixed-untested", bad[0])

    def test_lists_every_one(self):
        _, bad = g.judge_issues([issue(1, "v0.2.0", "fixed-untested"), issue(2, "v0.2.0", "fixed-untested")], "v0.2.0")
        self.assertEqual(len(bad), 2)

    def test_other_labels_and_closed_ignored(self):
        iss = [issue(1, "v0.1.18", "fixed-untested"), issue(2, "fixed-untested"), issue(3, "v0.2.0", "fixed-untested", state="closed")]
        self.assertEqual(g.judge_issues(iss, "v0.2.0")[1], [])
        self.assertEqual(g.judge_issues([{"iid": 4, "title": "x", "labels": [{"name": "v0.2.0"}, {"name": "fixed-untested"}]}], "v0.2.0")[1][0][:10], "issue #4 i")

    def test_parse_paginated_file(self):
        self.assertEqual(len(g.parse_issue_list('[{"iid":1}][{"iid":2}]')), 2)
        self.assertEqual(g.parse_issue_list('[]'), [])
        with self.assertRaises(ValueError):
            g.parse_issue_list("")
        with self.assertRaises(ValueError):
            g.parse_issue_list('{"message":"401"}')

    def run_gate(self, issues, issues_file=None):
        class WithIssues(Fake):
            def get(self, path, raw=False):
                if "/issues?" in path:
                    if isinstance(issues, Exception):
                        raise issues
                    return issues
                return Fake.get(self, path, raw)
        import contextlib, io
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            rc = g.run(WithIssues(ALL_JOBS, ALL_RES), "1", SHA, [], "tag v0.2.0", "v0.2.0", issues_file)
        return rc, err.getvalue()

    def test_gate_run(self):
        self.assertEqual(self.run_gate([])[0], 0)
        rc, err = self.run_gate([issue(99, "v0.2.0", "fixed-untested")])
        self.assertEqual(rc, 1)
        self.assertIn("#99", err)
        self.assertEqual(self.run_gate([issue(99, "v0.1.18", "fixed-untested")])[0], 0)
        self.assertEqual(self.run_gate(RuntimeError("no token"))[0], 1)      # an unreadable list fails closed

    def test_gate_run_from_file(self):
        import tempfile
        with tempfile.TemporaryDirectory(dir=os.path.dirname(os.path.abspath(__file__))) as d:
            f = os.path.join(d, "i.json")
            pathlib.Path(f).write_text(json.dumps([issue(42, "v0.2.0", "fixed-untested")]))
            rc, err = self.run_gate(RuntimeError("must not be queried"), f)
            self.assertEqual(rc, 1)
            self.assertIn("#42", err)
            pathlib.Path(f).write_text("[]")
            self.assertEqual(self.run_gate(RuntimeError("must not be queried"), f)[0], 0)
            pathlib.Path(f).write_text("not json")
            self.assertEqual(self.run_gate([], f)[0], 1)


class Beta(unittest.TestCase):
    """Owner decision 2026-11-14: a tag with "beta" in its name needs guards + the nine unit suites + the three builds, nothing heavier."""
    BETA_OK = [(n, "success") for n in g.beta_required_jobs()]

    def problems(self, jobs):
        return g.check(Fake(jobs, {}), "1", SHA, (), True)[1]

    def test_is_beta(self):
        for t in ("v0.2.0-beta.1", "v0.2.0-BETA.2", "v0.2.0-Beta"):
            self.assertTrue(g.is_beta(t), t)
        for t in ("v0.2.0", "v0.2.0-rc.1", "", None):
            self.assertFalse(g.is_beta(t), t)

    def test_required_list_is_real_jobs(self):
        self.assertEqual(len(g.BETA_UNIT_JOBS), 9)
        self.assertEqual(len(g.BETA_BUILD_JOBS), 3)
        self.assertFalse(set(g.beta_required_jobs()) - g.yml_jobs(), "every beta job is defined in .gitlab-ci.yml")

    def test_passes_on_units_and_builds_without_app_tests_or_heavy_tier(self):
        self.assertEqual(self.problems(self.BETA_OK), [])
        self.assertNotIn("fuzz-smoke", g.beta_required_jobs())       # heavy tier not required
        self.assertTrue(set(APPS) - set(g.beta_required_jobs()))

    def test_fails_on_any_missing_or_red_unit_or_build(self):
        for name in g.BETA_UNIT_JOBS + g.BETA_BUILD_JOBS + g.BASE_JOBS:
            p = self.problems([(n, "failed" if n == name else s) for n, s in self.BETA_OK])
            self.assertEqual(len(p), 1, name)
            self.assertIn(name, p[0])
            p = self.problems([(n, s) for n, s in self.BETA_OK if n != name])
            self.assertEqual(len(p), 1, name)
            self.assertIn("never ran", p[0])

    def run_gate(self, tag, jobs, issues, results=None):
        class W(Fake):
            def get(self, path, raw=False):
                if "/issues?" in path:
                    if isinstance(issues, Exception):
                        raise issues
                    return issues
                return Fake.get(self, path, raw)
        import contextlib, io
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            rc = g.run(W(jobs, results or {}), "1", SHA, [], "tag " + tag, g.release_label(tag))
        return rc, out.getvalue(), err.getvalue()

    def test_beta_passes_with_fixed_untested_open_and_lists_them(self):
        rc, out, err = self.run_gate("v0.2.0-beta.1", self.BETA_OK, [issue(99, "v0.2.0-beta.1", "fixed-untested")])
        self.assertEqual((rc, err), (0, ""))
        self.assertIn("#99", out)
        self.assertIn("BETA", out)
        self.assertEqual(self.run_gate("v0.2.0-beta.1", self.BETA_OK, RuntimeError("no token"))[0], 0)

    def test_beta_fails_when_a_suite_or_build_is_red(self):
        rc, _, err = self.run_gate("v0.2.0-beta.1", [(n, "failed" if n == "build-windows-x86_64" else s) for n, s in self.BETA_OK], [])
        self.assertEqual(rc, 1)
        self.assertIn("build-windows-x86_64", err)
        rc, _, err = self.run_gate("v0.2.0-BETA.1", [(n, s) for n, s in self.BETA_OK if n != "test-macos"], [])
        self.assertEqual(rc, 1)

    def test_non_beta_still_needs_the_full_tier_and_zero_fixed_untested(self):
        rc, _, err = self.run_gate("v0.2.0", self.BETA_OK, [])
        self.assertEqual(rc, 1)                                # units + builds alone are not enough
        self.assertIn("fuzz-smoke", err)
        self.assertIn("app-test-macos", err)
        rc, _, err = self.run_gate("v0.2.0", ALL_JOBS, [issue(7, "v0.2.0", "fixed-untested")], ALL_RES)
        self.assertEqual(rc, 1)
        self.assertIn("#7", err)
        self.assertEqual(self.run_gate("v0.2.0", ALL_JOBS, [], ALL_RES)[0], 0)
        self.assertEqual(self.run_gate("v0.2.0-rc.1", ALL_JOBS, [], ALL_RES)[0], 0)
        self.assertEqual(self.run_gate("v0.2.0-rc.1", self.BETA_OK, [])[0], 1)

    def test_known_issues_markdown(self):
        md = g.known_issues_markdown([issue(5, "v0.2.0-beta.1", "fixed-untested"), issue(3, "v0.2.0-beta.1", "fixed-untested"),
                                      issue(4, "v0.2.0-beta.1"), issue(6, "v0.2.0-beta.1", "fixed-untested", state="closed")], "v0.2.0-beta.1")
        self.assertIn("- #3 ", md)
        self.assertIn("- #5 ", md)
        self.assertNotIn("#4", md)
        self.assertNotIn("#6", md)
        self.assertLess(md.index("#3"), md.index("#5"))
        self.assertIn("No open", g.known_issues_markdown([], "v0.2.0-beta.1"))


if __name__ == "__main__":
    unittest.main()
