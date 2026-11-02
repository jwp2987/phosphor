#!/usr/bin/env python3
# script/ci/ci_seq.py: drive one GitLab pipeline for a build/* ref through a TEST TIER (docs/testing.md "Test tiers").
#
#   python3 script/ci/ci_seq.py build/<ref> [--tier fix|candidate|release] [--platforms linux,macos,windows]
#                               [--tests '<nextest filterset>'] [--base <git ref>] [--skip job ...] [--dry-run]
#   (legacy positional job names after the ref still mean --skip)
#
#   fix       (~1 h)   check-linux; test-linux with a nextest filter (--tests, or derived from the git diff);
#                      test-warpui-core / test-workspace-rest only when their crates changed; builds for --platforms
#                      (default: derived from the diff); app tests in SMOKE mode (APPCI_TIER=smoke). A platform whose
#                      app-affecting paths (APP_NONAFFECTING below, inverted) equal those of the newest earlier build/*
#                      pipeline with a SUCCESSFUL, unexpired build job REUSES that build (app-test-*-reuse, job id passed as
#                      APPCI_BUILD_JOB_ID) instead of building. --no-reuse forces builds. candidate/release ALWAYS build.
#   candidate (~3 h)   check-linux; all three test suites unfiltered; all three builds; the FULL app matrix everywhere.
#   release   (~8 h)   candidate + test-shuttle-loom, fuzz-smoke, bench, mutants, mutants-warpui-core.
#
# After the run it prints the collect+commit command, or with --collect runs script/ci/collect_results.py itself (test-results/ archive).
# Prereqs: glab authenticated (run 'glab api user' first). Never re-plays a job that already ran. Heavy jobs: ask the owner.
# Per-job variables (TEST_FILTER, APPCI_TIER) go through the play API's job_variables_attributes.
import argparse, json, os, re, subprocess, sys, threading, time

PROJECT = "88085499"
LINUX, MAC, WIN = "linux", "macos", "windows"
BUILD = {LINUX: "build-linux-x86_64", MAC: "build-macos-arm64", WIN: "build-windows-x86_64"}
APP = {LINUX: ["app-test-linux-wayland", "app-test-linux-x11"], MAC: ["app-test-macos"], WIN: ["app-test-windows"]}
APP_CORE_FILES = re.compile(r"^(app/|crates/|Cargo\.(toml|lock)|\.cargo/|script/ci/|\.gitlab-ci\.yml|\.config/)")
WIN_ONLY = re.compile(r"(^|/)(windows|win)(/|_|\.)|_windows\.rs$|^script/realapp/windows/|^script/windows/|\.ps1$|^wix/|^installer/windows")
MAC_ONLY = re.compile(r"(^|/)(macos|mac)(/|_|\.)|_macos\.rs$|^script/realapp/mac/|^script/mac/|^app/macos|\.plist$|\.entitlements$")
LINUX_ONLY = re.compile(r"(^|/)linux(/|_|\.)|_linux\.rs$|^script/realapp/linux/|^script/linux/|\.desktop$")
# Paths that never end up in the built app or in how a build job builds it. Everything NOT listed here is app-affecting, so a new
# top-level directory or script fails safe (rebuild). Also compared: the build-X job block and its templates in .gitlab-ci.yml.
APP_NONAFFECTING = ["docs", ".gitlab", ".gitlab-ci.yml", "specs", "fuzz", ".agents", ".zed", ".mcp.json", "issues.md", "stale-r13.pids",
                    "script/ci", "script/realapp", "script/precheck", "script/presubmit", "script/state", "script/keylat", "script/lint_powershell",
                    "script/_cloud_boundary_scan.py", "script/cloud_boundary_allowlist.txt", "script/known_test_failures.txt",
                    "script/large_deletion_allowlist.txt", "script/extract_sweep_ledger.py", "script/repro_subshell_relay_deadlock.py",
                    "script/test-fixtures", "deny.toml", "test-results", "script/agent-cargo", "script/agent-worktree", "script/setup-merge-drivers.sh"]
APP_NONAFFECTING_GLOBS = ["**/*.md", "script/check_*", "script/test_*"]
DOC_ONLY = re.compile(r"^(docs/|.*\.md$|TODO|issues\.md|DECLINED|ORACLE)")


def sh(*a, check=False):
    r = subprocess.run(list(a), capture_output=True, text=True)
    if check and r.returncode:
        sys.exit("%s failed: %s" % (" ".join(a), r.stderr.strip()))
    return r.stdout.strip()


def gitref(r):
    """A build/* name is usually only on the remote: resolve r, else origin/r. Fail loudly: an
    unresolvable ref used to give an empty diff, i.e. 'nothing changed, run no tests'."""
    for cand in (r, "origin/" + r):
        if subprocess.run(["git", "rev-parse", "--verify", "-q", cand + "^{commit}"], capture_output=True).returncode == 0:
            return cand
    sys.exit("ci_seq: cannot resolve git ref %r (nor origin/%s); run git fetch origin" % (r, r))


def changed_files(base, ref):
    return [l for l in sh("git", "diff", "--name-only", "%s..%s" % (base, gitref(ref)), check=True).splitlines() if l]


def resolve_base(ref, explicit):
    """merge-base of ref with the previous build/* ref (newest other build/* branch that is an ancestor), else origin/main."""
    ref = gitref(ref)
    if explicit:
        return sh("git", "merge-base", gitref(explicit), ref, check=True)
    for cand in sh("git", "for-each-ref", "--sort=-creatordate", "--format=%(refname:short)", "refs/remotes/*/build/*", "refs/heads/build/*").splitlines():
        if cand.endswith("/" + ref) or cand == ref:
            continue
        mb = sh("git", "merge-base", cand, ref)
        if mb and mb != sh("git", "rev-parse", ref):
            return mb
    return sh("git", "merge-base", "origin/main", ref) or "origin/main"


def crate_names():
    """crates/<dir> -> cargo package name (dir name and package name can differ)."""
    out = {}
    for d in sh("git", "ls-files", "crates/*/Cargo.toml", "lib/*/Cargo.toml").splitlines():
        m = re.search(r'^name\s*=\s*"([^"]+)"', open(d).read(), re.M) if _exists(d) else None
        if m:
            out[d.rsplit("/", 1)[0]] = m.group(1)
    return out


def _exists(p):
    import os
    return os.path.exists(p)


def derive(files):
    """-> (test-linux filterset or '', set of suites to run among {'linux','core','rest'}, reasons).
    Only test-linux (the ~50 min suite) is filtered; warpui-core (~2 min) and workspace-rest (~4 min) run whole, but only when touched."""
    names = crate_names()
    app_tests, linux_pk, suites, why = set(), set(), set(), []
    whole_warp = root_change = False
    for f in files:
        if DOC_ONLY.match(f):
            continue
        m = re.match(r"^((?:crates|lib)/[^/]+)/", f)
        if m:
            p = names.get(m.group(1), m.group(1).split("/")[1])
            if p == "warpui_core":
                suites.add("core"); suites.add("linux"); whole_warp = True
                why.append("%s -> warpui_core suite + whole warp (it is a dependency of the app)" % f)
            elif p == "warp_tui":
                suites.add("linux"); linux_pk.add(p); why.append("%s -> package(warp_tui)" % f)
            else:
                suites.add("rest"); why.append("%s -> package(%s): workspace-rest suite (dependents inside warp are NOT filtered in; pass --tests to add them)" % (f, p))
            continue
        if f.startswith("app/"):
            suites.add("linux")
            m = re.match(r"^app/src/(.+)$", f)
            segs = m.group(1).split("/") if m else []
            if segs and segs[0] in ("benches",) or f.startswith("app/benches/"):
                continue                                   # benches are not tests
            if not f.endswith(".rs"):
                whole_warp = True; why.append("%s -> whole warp package (non-Rust app file)" % f); continue
            segs = [re.sub(r"\.rs$", "", x) for x in segs]
            if "tests" in segs[:-1]:
                segs = segs[:segs.index("tests")]          # input_queue/tests/ops.rs -> input_queue
            if segs and segs[-1] in ("mod", "tests", "lib", "main"):
                segs = segs[:-1]
            segs = [re.sub(r"_(snapshot|model)$", "", re.sub(r"_tests?$", "", x)) for x in segs]
            if len(segs) >= 2 and segs[0] != "":
                name = "::".join(segs[-2:]) if len(segs) > 2 else "::".join(segs)
                app_tests.add(name); why.append("%s -> test(/%s/)" % (f, name))
            elif len(segs) == 1:
                app_tests.add(segs[0]); why.append("%s -> test(/%s/)" % (f, segs[0]))
            else:
                whole_warp = True; why.append("%s -> whole warp package" % f)
            continue
        if re.match(r"^(Cargo\.(toml|lock)|rust-toolchain|\.cargo/config|\.config/)", f):
            root_change = True; why.append("%s -> workspace-wide (nothing filtered)" % f)
    if root_change:
        return "", {"linux", "core", "rest"}, why
    parts = []
    if whole_warp:
        parts.append("package(warp) | package(warp_tui)")
    elif app_tests:
        parts.append("(package(warp) & (%s))" % " | ".join("test(/%s/)" % t for t in sorted(app_tests)))
    # dependents of changed non-app crates: run their own tests inside the warp suite too only by package name
    parts += ["package(%s)" % p for p in sorted(linux_pk) if not whole_warp]
    return " | ".join(parts), suites, why


def platforms_from(files):
    code = [f for f in files if not DOC_ONLY.match(f) and not re.match(r"^(script/ci/|\.gitlab-ci\.yml|script/(check|precheck|state|test_))", f)]
    if not code:
        return [LINUX]
    if all(WIN_ONLY.search(f) for f in code):
        return [WIN]
    if all(MAC_ONLY.search(f) for f in code):
        return [MAC]
    if all(LINUX_ONLY.search(f) for f in code):
        return [LINUX]
    if all(re.match(r"^(script|docs|\.gitlab)", f) and not APP_CORE_FILES.match(f) or WIN_ONLY.search(f) or MAC_ONLY.search(f) or LINUX_ONLY.search(f) for f in code):
        return sorted({p for f in code for p, rx in ((WIN, WIN_ONLY), (MAC, MAC_ONLY), (LINUX, LINUX_ONLY)) if rx.search(f)} or {LINUX})
    return [LINUX, MAC, WIN]


def app_pathspec():
    return ["--", "."] + [":(exclude)" + x for x in APP_NONAFFECTING] + [":(exclude,glob)" + x for x in APP_NONAFFECTING_GLOBS]


def ci_blocks(sha, build):
    """The text of build job `build` and the templates it extends in .gitlab-ci.yml at sha ('' if unreadable)."""
    txt = sh("git", "show", "%s:.gitlab-ci.yml" % sha)
    out = []
    for name in (build, ".shim_github_env", ".windows_job"):
        m = re.search(r"^%s:\n(?:[ \t].*\n|\n)*" % re.escape(name), txt, re.M)
        out.append(m.group(0) if m else "")
    return out


def app_diff(a, b, build):
    """-> list of app-affecting paths that differ between commits a and b (plus a '.gitlab-ci.yml:<job>' marker if the build job's
    definition changed). Empty = the build of a is the build of b."""
    r = subprocess.run(["git", "diff", "--name-only", a, b] + app_pathspec(), capture_output=True, text=True)
    if r.returncode:
        return ["<git diff failed: %s>" % r.stderr.strip()]
    ch = [l for l in r.stdout.splitlines() if l]
    if ci_blocks(a, build) != ci_blocks(b, build):
        ch.append(".gitlab-ci.yml:%s" % build)
    return ch


def find_reusable(plat, head, cur_pid):
    """Newest earlier build/* pipeline whose build job for plat SUCCEEDED, and an app-identical commit (its pipeline may still be running).
    -> (decision text, reuse dict or None)."""
    build, first_diff, seen = BUILD[plat], None, 0
    for p in api("projects/%s/pipelines?per_page=60&order_by=id&sort=desc" % PROJECT) or []:
        if p.get("id") == cur_pid or not str(p.get("ref", "")).startswith("build/"):
            continue
        jb = next((j for j in (api("projects/%s/pipelines/%d/jobs?per_page=100" % (PROJECT, p["id"])) or []) if j.get("name") == build), None)
        if not jb or jb.get("status") != "success":
            continue
        seen += 1
        # No artifact-expiry check: the reuse job takes the copy the build job cached on the test machine (the same machine);
        # GitLab artifacts are only its fallback. The pipeline need not be finished, only that build job.
        sha = p["sha"]
        if subprocess.run(["git", "cat-file", "-e", sha + "^{commit}"], capture_output=True).returncode:
            print("  %s: pipeline %s commit %s is not in the local clone; skipped" % (plat, p["id"], sha[:10])); continue
        ch = app_diff(sha, head, build)
        if not ch:
            return ("%s: reusing build job %d from pipeline %d (commit %s), app unchanged; the %s test machine should have it in its local build cache" % (plat, jb["id"], p["id"], sha[:10], plat),
                    {"job": jb["id"], "pipeline": p["id"], "sha": sha, "ref": p.get("ref", "")})
        if first_diff is None:
            first_diff = (p["id"], sha, ch)
        if seen >= 6:
            break
    if first_diff:
        pid, sha, ch = first_diff
        return ("%s: building, app changed in: %s (vs newest successful build, pipeline %d commit %s)" % (plat, ", ".join(ch[:8]) + (" ..." if len(ch) > 8 else ""), pid, sha[:10]), None)
    return ("%s: building, no earlier build/* pipeline has a successful, unexpired %s" % (plat, build), None)


def api(path, method="GET", variables=None):
    a = ["glab", "api", path] + (["-X", method] if method != "GET" else [])
    body = None
    if variables:   # play API: job_variables_attributes must arrive as a JSON array of {key,value}
        a += ["--header", "Content-Type: application/json", "--input", "-"]
        body = json.dumps({"job_variables_attributes": [{"key": k, "value": v} for k, v in variables.items()]})
    # GitLab rate limits (HTTP 429) and transient 5xx must never read as "no such job" (that ended a run early on 2026-11-02):
    # retry with backoff, and only give up (None) after ~5 minutes
    for attempt in range(10):
        r = subprocess.run(a, input=body, capture_output=True, text=True)
        transient = r.returncode != 0 and any(x in (r.stderr or "") for x in ("429", "502", "503", "504", "timeout", "Timeout"))
        if not transient:
            break
        time.sleep(min(60, 5 * (attempt + 1)))
    try:
        return json.loads(r.stdout or "null")
    except ValueError:
        return None


def main():
    ap = argparse.ArgumentParser(description="Drive a GitLab pipeline through a test tier; see the header of script/ci/ci_seq.py and docs/testing.md")
    ap.add_argument("ref")
    ap.add_argument("skip_pos", nargs="*", help="legacy: job names to skip")
    ap.add_argument("--tier", choices=["fix", "candidate", "release"], default="fix")
    ap.add_argument("--platforms", default="")
    ap.add_argument("--tests", default="")
    ap.add_argument("--base", default="")
    ap.add_argument("--skip", nargs="*", default=[])
    ap.add_argument("--reuse-job", action="append", default=[], metavar="PLATFORM=JOB_ID",
                    help="harness-only rounds: test this earlier build job of PLATFORM even though the app changed since (recorded as reused)")
    ap.add_argument("--no-reuse", action="store_true", help="fix tier: always build, never reuse an earlier app-identical build")
    ap.add_argument("--collect", action="store_true", help="at the end run script/ci/collect_results.py for this pipeline (writes test-results/; you commit it)")
    ap.add_argument("--dry-run", action="store_true", help="print the plan (filter, suites, platforms) and exit")
    a = ap.parse_args()
    ref, tier = a.ref, a.tier
    SKIP = set(a.skip_pos) | set(a.skip)

    sh("git", "fetch", "-q", "origin", check=True)
    base = resolve_base(ref, a.base)
    files = changed_files(base, ref) if tier == "fix" or a.dry_run else []
    filt, suites, plats = "", {"linux", "core", "rest"}, [LINUX, MAC, WIN]
    if tier == "fix":
        filt, suites, why = derive(files)
        plats = [p for p in a.platforms.split(",") if p] or platforms_from(files)
        if a.tests:
            filt = a.tests; suites = set(suites) | {"linux"}
        print("base %s..%s: %d files changed" % (base[:10], ref, len(files)))
        for w in why[:15]:
            print("  ", w)
        print("TEST_FILTER for test-linux (override with --tests):", filt or "(none: nothing in test-linux's scope changed)")
        print("suites:", sorted(suites) or "none", " platforms:", plats, flush=True)
        if "linux" in suites and not filt:
            print("note: test-linux requested but no filter derived; it would run the FULL ~50 min suite. Pass --tests to narrow it.", flush=True)
    elif a.platforms:
        plats = [p for p in a.platforms.split(",") if p]
    reuse = {}
    if tier == "fix":
        if a.no_reuse:
            print("reuse: disabled (--no-reuse); building every platform")
        else:
            head = sh("git", "rev-parse", gitref(ref) + "^{commit}", check=True)
            cur = (api("projects/%s/pipelines?ref=%s&per_page=1" % (PROJECT, ref.replace("/", "%2F"))) or [{}])[0].get("id")
            forced = dict(x.split("=", 1) for x in a.reuse_job)
            for p in plats:
                if p in forced:
                    jb = api("projects/%s/jobs/%s" % (PROJECT, forced[p])) or {}
                    if jb.get("name") != BUILD[p] or jb.get("status") != "success":
                        print("%s: --reuse-job %s is not a successful %s (got %r/%r)" % (p, forced[p], BUILD[p], jb.get("name"), jb.get("status"))); return 2
                    print("%s: FORCED reuse of build job %s (pipeline %s, commit %s): the app may differ from this ref; harness-only round" % (
                        p, jb["id"], jb["pipeline"]["id"], jb["commit"]["id"][:10]), flush=True)
                    reuse[p] = {"job": jb["id"], "pipeline": jb["pipeline"]["id"], "sha": jb["commit"]["id"], "ref": jb.get("ref", "")}
                    continue
                # a skipped build job still allows reuse: --skip build-* means "never build", not "never test"
                msg, r = find_reusable(p, head, cur)
                print(msg, flush=True)
                if r:
                    reuse[p] = r
    else:
        print("reuse: never in the %s tier; every platform builds" % tier)
    if a.dry_run:
        return 0
    fvars = {"TEST_FILTER": filt} if (tier == "fix" and filt) else None
    appvars = {"APPCI_TIER": "smoke" if tier == "fix" else "full"}

    pid = None
    while not pid:
        for p in api("projects/%s/pipelines?ref=%s" % (PROJECT, ref.replace("/", "%2F"))) or []:
            pid = p["id"]; break
        if not pid:
            time.sleep(20)
    t_start = time.time()
    timeline = {}

    def jobs():
        return {j["name"]: j for j in (api("projects/%s/pipelines/%d/jobs?per_page=100" % (PROJECT, pid)) or [])}

    def wait(name):
        while True:
            s = jobs().get(name, {}).get("status", "missing")
            if s in ("success", "failed", "canceled", "missing", "skipped"):
                j = jobs().get(name, {})
                timeline[name] = (s, j.get("duration"))
                return s
            time.sleep(60)

    def play(name, variables=None):
        j = jobs().get(name)
        if not j or j["status"] != "manual":
            return False                                   # already played/run or absent: never re-run
        api("projects/%s/jobs/%d/play" % (PROJECT, j["id"]), "POST", variables)
        return True

    print("pipeline", pid, "tier", tier, flush=True)
    if reuse and all(p in reuse for p in plats):
        # every platform tests an app-identical earlier build: that commit's compile check already passed, so do not queue
        # the app tests behind a check-linux that waits for the box
        s = "success"; print("check-linux: not waited for (no app change; every platform reuses a build)", flush=True)
    else:
        s = wait("check-linux"); print("check-linux", s, flush=True)
    if s != "success":
        summary(timeline, t_start); collect_hint(pid, a.collect); return 1

    # builds first (they are the long pole: macos ~21 min, windows ~38 min), tests alongside on the box
    builds = [BUILD[p] for p in plats if BUILD[p] not in SKIP and p not in reuse]
    reuse_started = []
    for p, r in reuse.items():          # no build to wait for: start these now (resource groups serialize them per machine)
        rv = dict(appvars, APPCI_BUILD_JOB_ID=str(r["job"]), APPCI_BUILD_PIPELINE_ID=str(r["pipeline"]), APPCI_BUILD_SHA=r["sha"], APPCI_BUILD_JOB_NAME=BUILD[p], APPCI_BUILD_REF=r.get("ref", ""))
        for n in APP[p]:
            if n in SKIP or n + "-reuse" not in jobs():
                continue
            if n == "app-test-linux-x11":
                print("%s: smoke tier skips the x11 pass; not played" % n); continue
            if play(n + "-reuse", rv):
                reuse_started.append(n + "-reuse")
    for n in builds:
        play(n)
    # Each platform's app tests start as soon as ITS build succeeds, not after the box test suites (those take ~1 h and
    # would hold the Mac and Windows tests back for no reason; app tests on other machines do not compete with the box).
    p_of = {BUILD[p]: p for p in plats}
    started, lock = [], threading.Lock()

    def after_build(b):
        if wait(b) != "success":
            return
        for n in [t for t in APP[p_of[b]] if t not in SKIP and t in jobs()]:
            if tier == "fix" and n == "app-test-linux-x11":
                continue
            if play(n, appvars):
                print("%s: %s succeeded, started %s" % (time.strftime("%H:%M"), b, n), flush=True)
                with lock:
                    started.append(n)
    watchers = [threading.Thread(target=after_build, args=(b,), daemon=True) for b in builds]
    for t in watchers:
        t.start()
    if tier == "fix":
        box = [(n, v) for n, k, v in (("test-linux", "linux", fvars), ("test-warpui-core", "core", None), ("test-workspace-rest", "rest", None)) if k in suites]
    else:
        box = [("test-linux", None), ("test-warpui-core", None), ("test-workspace-rest", None)]
    for n, v in box:
        if n in SKIP: continue
        play(n, v); print(n, wait(n), flush=True)
    if tier == "release":
        for n in ["test-shuttle-loom", "fuzz-smoke", "bench", "mutants", "mutants-warpui-core"]:
            if n not in SKIP: play(n)
    for t in watchers:
        t.join()
    for n in builds:
        print(n, timeline.get(n, ("?",))[0], flush=True)
    for n in started + reuse_started: print(n, wait(n), flush=True)
    if tier == "release":
        for n in ["test-shuttle-loom", "fuzz-smoke", "bench", "mutants", "mutants-warpui-core"]:
            if n not in SKIP and jobs().get(n, {}).get("status") not in (None, "manual"): print(n, wait(n), flush=True)
    summary(timeline, t_start)
    collect_hint(pid, a.collect)
    return 0 if all(v[0] == "success" for k, v in timeline.items() if not k.startswith(("app-test", "bench", "mutants", "fuzz"))) else 1


def collect_hint(pid, auto):
    """Every tier run ends by archiving its results (owner rule: every test result is kept in the repo). CI never pushes: the coordinator commits."""
    here = os.path.dirname(os.path.abspath(__file__))
    cmd = "python3 script/ci/collect_results.py %d && git add test-results && git commit -m 'test-results: pipeline %d'" % (pid, pid)
    if auto:
        print("\ncollecting results for pipeline %d ..." % pid, flush=True)
        subprocess.run([sys.executable, os.path.join(here, "collect_results.py"), str(pid)])
        print("now commit them:  git add test-results && git commit -m 'test-results: pipeline %d'" % pid, flush=True)
    else:
        print("\nto archive these results (the coordinator commits; CI never pushes):\n  " + cmd, flush=True)


def summary(timeline, t0):
    print("\n%-28s %-10s %s" % ("job", "status", "minutes"))
    for k, (s, d) in timeline.items():
        print("%-28s %-10s %s" % (k, s, "%.1f" % (d / 60) if d else "-"))
    print("wall clock: %.1f min" % ((time.time() - t0) / 60), flush=True)


if __name__ == "__main__":
    sys.exit(main())
