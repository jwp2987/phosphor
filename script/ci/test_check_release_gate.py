#!/usr/bin/env python3
# Offline fixtures for check_release_gate.py (fake GitLab API). Run: python3 script/ci/test_check_release_gate.py
# Wired into the `guards` job (.gitlab-ci.yml) and script/precheck.
import json, os, sys, unittest
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


if __name__ == "__main__":
    unittest.main()
