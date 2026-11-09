#!/usr/bin/env python3
# script/ci/check_release_gate.py: the RELEASE GATE (#1495). Runs as the `release-gate` job of a `v*` tag pipeline before anything is built or
# uploaded, and exits non-zero unless the commit being tagged already has a green RELEASE-TIER run.
#
#   check_release_gate.py                      in a tag pipeline (CI_COMMIT_TAG, CI_COMMIT_SHA, CI_PROJECT_ID; reads with CI_JOB_TOKEN / APPCI_API_TOKEN)
#   check_release_gate.py --sha SHA [--pipeline ID ...] [--project ID]   read-only DRY RUN from a workstation through `glab api` (script/ci/glapi.py).
#                                              Without --pipeline every pipeline of SHA counts; with it only those pipelines.
#
# Why a check and not "run the tests in the tag pipeline": the app tests drive the real app on the box, the Mac and the Windows VM and need a
# build of their own pipeline (`build-*`). The tag pipeline builds different binaries (`release-*`, with the CLI digests compiled in), is created
# the instant a Maintainer pushes the tag, and has no tier driver. KNOWN GAP (documented, not redesigned): the app tests therefore exercise
# `build-*` binaries, not the `release-*` ones. So the app-test evidence is recorded on the SAME COMMIT by an earlier `build/*` pipeline
# (`ci_seq.py --tier release`, docs/testing.md "Release gate") and this job refuses the tag when it is missing, red, stale (artifacts
# expire after 7 days) or from a smaller tier.
#
# What it requires, all on the tagged sha (any pipeline of that sha; a job that succeeded in any of them counts):
#   * BASE_JOBS (guards, check-linux, licenses, advisories): success;
#   * every job of the RELEASE tier that the tier gates on, taken from script/ci/ci_seq.py (release_gate_jobs(): the box unit / integration /
#     usage suites, input-timers-strict, test-shuttle-loom, fuzz-smoke, and the Mac / Windows unit and heavy suites; bench and mutants are
#     informational there and here): success;
#   * every app-test job of ci_seq.APP: success, and its results.json (one per pass: Wayland also has the XWayland pass) says tier=release,
#     sha=<tagged sha>, 0 failed cases and at least one pass. The verdict is the appci one (report.py / ci_seq.judged_nothing): a NOREF case
#     (no reference terminal; not a pass, not a failure) and a KNOWN note (an ACCEPTED_FOR_RELEASE shortfall, recorded on a PASS row) never fail
#     a run, so they do not fail the gate either.
# Missing read access, expired artifacts and smoke/candidate-tier runs are FAILURES, never passes.
import json, os, sys, urllib.error, urllib.request

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import ci_seq  # noqa: E402  (the release tier's own job lists: the single source of truth)

RELEASE_TIER = ci_seq.app_tier_name("release")
BASE_JOBS = ["guards", "check-linux", "licenses", "advisories"]
# app job -> results.json paths that must all be green (the Linux Wayland job also runs an XWayland pass).
APP_RESULT_PATHS = {
    "app-test-linux-wayland": ["appci-out/wayland/run/results.json", "appci-out/xwayland/run/results.json"],
    "app-test-linux-x11": ["appci-out/x11/run/results.json"],
    "app-test-macos": ["appci-out/macos/run/results.json"],
    "app-test-windows": ["appci-out/windows/run/results.json"],
}


def yml_jobs(path=None):
    """Top-level job names of .gitlab-ci.yml (ci_seq likewise plays only jobs the pipeline has: input-timers-strict has no -macos / -windows twin)."""
    import re
    path = path or os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", ".gitlab-ci.yml")
    return set(re.findall(r"^([A-Za-z0-9_][A-Za-z0-9_.-]*):", open(path).read(), re.M))


def required_jobs(defined=None):
    """-> (plain jobs that must have succeeded, app jobs). Derived from ci_seq's release tier, never a hand list; a job the pipeline does not
    define (`defined`, default: .gitlab-ci.yml; matrix jobs `name: [N]` by their base name) cannot be required."""
    defined = yml_jobs() if defined is None else defined
    plain = list(BASE_JOBS)
    for names in ci_seq.release_gate_jobs().values():
        plain += [n for n in names if n not in plain and n.split(":")[0] in defined]
    apps = [a for names in ci_seq.APP.values() for a in names]
    return plain, apps


class CiApi:
    """The tag pipeline's own reads: CI_JOB_TOKEN first, then the masked APPCI_API_TOKEN (read_api)."""
    def __init__(self):
        self.base = os.environ.get("CI_API_V4_URL", "")
        self.tokens = [("JOB-TOKEN", os.environ.get("CI_JOB_TOKEN", "")), ("PRIVATE-TOKEN", os.environ.get("APPCI_API_TOKEN", ""))]

    def get(self, path, raw=False):
        last = "no token available"
        for hdr, tok in self.tokens:
            if not tok:
                continue
            try:
                data = urllib.request.urlopen(urllib.request.Request(self.base + path, headers={hdr: tok}), timeout=60).read()
                return data if raw else json.loads(data)
            except urllib.error.HTTPError as e:
                last = "HTTP %s with %s" % (e.code, hdr)
            except Exception as e:  # network error: fail closed like everything else here
                last = str(e)
        raise RuntimeError("cannot read %s (%s); set a masked APPCI_API_TOKEN with read_api" % (path, last))


class GlabApi:
    """Workstation dry run: `glab api` reads only (GET), through glapi's rate-limit handling."""
    def get(self, path, raw=False):
        import glapi
        st, out = glapi.call(path)
        if st != 200:
            raise RuntimeError("HTTP %s for %s" % (st, path))
        return out if raw else json.loads(out)


def judge_results(res, sha, label):
    """Problems (list of str) with one results.json document; empty = a green release-tier run of `sha`. The pass/fail rule is the appci one:
    fail > 0 or a run that judged nothing fails (report.py exits 1); NOREF (summary.noref) and KNOWN (a note on a pass row) do not."""
    meta, cnt = res.get("meta", {}), res.get("summary", {})
    out = []
    if meta.get("tier") != RELEASE_TIER:
        out.append("%s: tier is %r, not %r" % (label, meta.get("tier"), RELEASE_TIER))
    if meta.get("sha") != sha:
        out.append("%s: ran on commit %r, not %s" % (label, meta.get("sha"), sha))
    if cnt.get("fail", 0):
        out.append("%s: %d failed case(s)" % (label, cnt["fail"]))
    if ci_seq.judged_nothing(res):
        out.append("%s: the run judged nothing (no pass/fail/no-reference row)" % label)
    elif not cnt.get("pass", 0):
        out.append("%s: no passing case at all" % label)
    return out


def info_counts(res):
    cnt = res.get("summary", {})
    known = sum(1 for r in res.get("cases", []) if "KNOWN:" in json.dumps(r))
    return "pass %s, noref %s, KNOWN %s" % (cnt.get("pass", 0), cnt.get("noref", 0), known)


def collect_jobs(api, proj, sha, pipelines):
    ids = [str(p) for p in pipelines] or [str(p["id"]) for p in api.get("/projects/%s/pipelines?sha=%s&per_page=100" % (proj, sha))]
    jobs = []  # (pipeline id, job)
    for pid in ids:
        page = 1
        while True:
            got = api.get("/projects/%s/pipelines/%s/jobs?per_page=100&page=%d&include_retried=true" % (proj, pid, page))
            jobs += [(pid, j) for j in got]
            if len(got) < 100:
                break
            page += 1
    return ids, jobs


def check(api, proj, sha, pipelines=()):
    """-> (ok lines, problem lines)."""
    ids, jobs = collect_jobs(api, proj, sha, pipelines)
    plain, apps = required_jobs()
    problems, ok = [], []
    if not ids:
        return ok, ["no pipeline found for %s" % sha[:10]]
    for name in plain:
        hit = [(pid, j) for pid, j in jobs if j["name"] == name and j["status"] == "success"]
        if hit:
            ok.append("%s: success (pipeline %s job %s)" % (name, hit[0][0], hit[0][1]["id"]))
        else:
            seen = sorted({j["status"] for _, j in jobs if j["name"] == name})
            problems.append("%s: no successful run on %s (%s)" % (name, sha[:10], "seen: " + ", ".join(seen) if seen else "job never ran"))
    for name in apps:
        cands = [(pid, j) for pid, j in jobs if j["name"] == name and j["status"] == "success"]
        if not cands:
            seen = sorted({j["status"] for _, j in jobs if j["name"] == name})
            problems.append("%s: no successful run on %s (%s; the -reuse variant does not count)" % (name, sha[:10], "seen: " + ", ".join(seen) if seen else "job never ran"))
            continue
        why = []
        for pid, j in sorted(cands, key=lambda c: -int(c[1]["id"])):
            bad, notes = [], []
            for path in APP_RESULT_PATHS[name]:
                label = "%s job %s %s" % (name, j["id"], path)
                try:
                    res = json.loads(api.get("/projects/%s/jobs/%s/artifacts/%s" % (proj, j["id"], path), raw=True))
                except Exception as e:
                    bad.append("%s: results.json unreadable (%s; artifacts expire after 7 days)" % (label, e))
                    continue
                bad += judge_results(res, sha, label)
                notes.append("%s %s" % (path.split("/")[1], info_counts(res)))
            if not bad:
                ok.append("%s: job %s (pipeline %s) green, tier %s [%s]" % (name, j["id"], pid, RELEASE_TIER, "; ".join(notes)))
                break
            why += bad
        else:
            problems += why
    return ok, problems


def run(api, proj, sha, pipelines, label):
    try:
        ok, problems = check(api, proj, sha, pipelines)
    except Exception as e:
        print("ERROR release gate could not query GitLab: %s" % e, file=sys.stderr)
        return 1
    for line in ok:
        print("ok    " + line)
    for line in problems:
        print("FAIL  " + line, file=sys.stderr)
    if problems:
        print("\nRELEASE GATE: REFUSING %s (%s), %d problem(s). Run the release tier on this exact commit first:\n"
              "  python3 script/ci/ci_seq.py build/<name> --tier release   (docs/testing.md, 'Release gate')\n"
              "then push the tag again." % (label, sha[:10], len(problems)), file=sys.stderr)
        return 1
    print("RELEASE GATE: PASS %s (%s) has a green release-tier run" % (label, sha[:10]))
    return 0


def main(argv):
    import argparse
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[1] if __doc__ else "")
    ap.add_argument("--sha", help="dry run: the commit to judge (reads through glab, read-only)")
    ap.add_argument("--pipeline", action="append", default=[], help="dry run: only these pipeline ids (default: every pipeline of --sha)")
    ap.add_argument("--project", default=ci_seq.PROJECT)
    a = ap.parse_args(argv)
    if a.sha:
        pipes = a.pipeline
        return run(GlabApi(), a.project, a.sha, pipes, "dry run")
    sha, proj = os.environ.get("CI_COMMIT_SHA", ""), os.environ.get("CI_PROJECT_ID", "")
    if not (sha and proj and os.environ.get("CI_COMMIT_TAG")):
        print("ERROR check_release_gate runs only in a tag pipeline (CI_COMMIT_TAG, CI_COMMIT_SHA, CI_PROJECT_ID), or with --sha for a dry run", file=sys.stderr)
        return 2
    return run(CiApi(), proj, sha, [], "tag " + os.environ["CI_COMMIT_TAG"])


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
