#!/usr/bin/env python3
# script/ci/ci_seq.py: drive one GitLab pipeline for a build/* ref through a TEST TIER (docs/testing.md "Test tiers").
#
#   python3 script/ci/ci_seq.py build/<ref> [--tier fix|candidate|release] [--platforms linux,macos,windows]
#                               [--tests '<nextest filterset>'] [--base <git ref>] [--skip job ...] [--dry-run]
#   (legacy positional job names after the ref still mean --skip)
#
#   fix       (~1 h)   check-linux; test-linux with a nextest filter (--tests, or derived from the git diff);
#                      test-warpui-core only when its crate changed, test-workspace-rest on every code change (#1496); builds for --platforms
#                      (default: derived from the diff); app tests in SMOKE mode (APPCI_TIER=smoke). A platform whose
#                      app-affecting paths (APP_NONAFFECTING below, inverted) equal those of the newest earlier build/*
#                      pipeline with a SUCCESSFUL, unexpired build job REUSES that build (app-test-*-reuse, job id passed as
#                      APPCI_BUILD_JOB_ID) instead of building. --no-reuse forces builds. candidate/release ALWAYS build.
#   candidate (~6 h)   check-linux; all three box test suites unfiltered; all three builds; the FULL app matrix everywhere; and on the
#                      Mac and Windows runners the same three unit suites (test-<plat>, test-warpui-core-<plat>,
#                      test-workspace-rest-<plat>), played AFTER that platform's app tests (each runner is one machine).
#   release   (~16 h)  candidate + on the box test-shuttle-loom, fuzz-smoke, bench, mutants, mutants-warpui-core, and on EACH of
#                      macOS and Windows the same heavy set (-macos / -windows suffix), after that platform's unit suites. Every
#                      suite runs on Linux, macOS and Windows ("the tests need to be the same on all boxes"); the one exception
#                      is fuzz-smoke-windows (cargo-fuzz is Unix-only): it is never played, it is reported "unsupported" (neutral).
#                      Times are estimates: the Mac/Windows suites have never run; the first release run records them.
#
# After the run it prints the collect+commit command, or with --collect runs script/ci/collect_results.py itself (test-results/ archive).
# Prereqs: glab authenticated (run 'glab api user' first). Never re-plays a job that already ran. Heavy jobs: ask the owner.
# Per-job variables (TEST_FILTER, APPCI_TIER) go through the play API's job_variables_attributes.
import argparse, json, os, re, subprocess, sys, threading, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import glapi                                    # every GitLab call: honours Retry-After / RateLimit-* (GitLab.com rate limits)

PROJECT = "88085499"
LINUX, MAC, WIN = "linux", "macos", "windows"
BUILD = {LINUX: "build-linux-x86_64", MAC: "build-macos-arm64", WIN: "build-windows-x86_64"}
APP = {LINUX: ["app-test-linux-wayland", "app-test-linux-x11"], MAC: ["app-test-macos"], WIN: ["app-test-windows"]}
UNIT_SUFFIX = ["test", "test-warpui-core", "test-workspace-rest"]
HEAVY_SUFFIX = ["test-shuttle-loom", "fuzz-smoke", "bench", "mutants", "mutants-warpui-core"]
PLAT_SUFFIX = {MAC: "macos", WIN: "windows"}
# suites that cannot exist on a platform, with the reason: reported, never played (a job that is built to fail is not a test)
UNSUPPORTED = {"fuzz-smoke-windows": "cargo-fuzz works only on Unix-like systems (no Windows/MSVC)"}


def plat_suites(p, heavy):
    """The unit (and, in the release tier, heavy) suite job names of the Mac/Windows platform p: test-macos, test-warpui-core-macos, ..."""
    sfx = PLAT_SUFFIX[p]
    # the plain unit suite is test-macos / test-windows; the others are <suite>-<platform>
    names = ["test-" + sfx] + ["%s-%s" % (n, sfx) for n in UNIT_SUFFIX[1:]]
    if heavy:
        names += ["%s-%s" % (n, sfx) for n in HEAVY_SUFFIX if n != "test-shuttle-loom"] + ["test-shuttle-loom-" + sfx]
    return names


APP_CORE_FILES = re.compile(r"^(app/|crates/|Cargo\.(toml|lock)|\.cargo/|script/ci/|\.gitlab-ci\.yml|\.config/)")
WIN_ONLY = re.compile(r"(^|/)(windows|win)(/|_|\.)|_windows\.rs$|^script/realapp/windows/|^script/windows/|\.ps1$|^wix/|^installer/windows")
MAC_ONLY = re.compile(r"(^|/)(macos|mac)(/|_|\.)|_macos\.rs$|^script/realapp/mac/|^script/mac/|^app/macos|\.plist$|\.entitlements$")
LINUX_ONLY = re.compile(r"(^|/)linux(/|_|\.)|_linux\.rs$|^script/realapp/linux/|^script/linux/|\.desktop$")
# Paths that never end up in the built app or in how a build job builds it. Everything NOT listed here is app-affecting, so a new
# top-level directory or script fails safe (rebuild). Also compared: the build-X job block and its templates in .gitlab-ci.yml.
APP_NONAFFECTING = ["docs", ".gitlab", ".gitlab-ci.yml", "specs", "fuzz", ".agents", ".zed", ".mcp.json", "issues.md", "stale-r13.pids",
                    "script/ci", "script/realapp", "script/precheck", "script/presubmit", "script/state", "script/keylat", "script/lint_powershell",
                    "script/_cloud_boundary_scan.py", "script/cloud_boundary_allowlist.txt", "script/known_test_failures.txt", "script/known_test_failures_macos.txt", "script/known_test_failures_windows.txt",
                    "script/large_deletion_allowlist.txt", "script/extract_sweep_ledger.py", "script/repro_subshell_relay_deadlock.py",
                    "script/test-fixtures", "deny.toml", "test-results", "script/agent-cargo", "script/agent-worktree", "script/setup-merge-drivers.sh"]
APP_NONAFFECTING_GLOBS = ["**/*.md", "script/check_*", "script/test_*"]
DOC_ONLY = re.compile(r"^(docs/|.*\.md$|issues\.md$)")   # #1527: no bare TODO/DECLINED/ORACLE prefixes


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


# Per-path overrides for the input path (#1504). The generic mapping below turns app/src/a/b.rs into test(/a::b/), which selects ZERO
# tests for files whose tests live elsewhere (input_queue/destination.rs and gates.rs are tested from input_queue::tests and
# input_queue::table::tests; view.rs's hand-over tests are in input_queue::, pane_key::, model::typeahead_handoff). Any path matching a
# rule here selects the whole INPUT_PATH set plus the rule's extra module alternatives, as `terminal::(...)` test-name regexes.
INPUT_PATH = ["input_queue", "pane_key", "view", "model::key_facts", "model::typeahead_handoff"]
INPUT_PATH_RULES = [
    (re.compile(r"^app/src/terminal/input_queue/"), []),
    (re.compile(r"^app/src/terminal/view(\.rs|/|_)"), ["writeable_pty", "local_tty"]),
    (re.compile(r"^app/src/terminal/model/typeahead"), []),
    (re.compile(r"^app/src/terminal/local_tty/"), ["local_tty", "writeable_pty"]),
    (re.compile(r"^app/src/terminal/writeable_pty/"), ["writeable_pty", "local_tty"]),
    (re.compile(r"^app/src/terminal/pane_key"), []),
    (re.compile(r"^app/src/terminal/(model/)?bootstrap"), ["bootstrap", "model::bootstrap"]),
]


def input_path_test(f):
    """-> the `terminal::(a|b|...)` regex for a changed path under the input path, or None."""
    for rx, extra in INPUT_PATH_RULES:
        if rx.search(f):
            return "terminal::(%s)" % "|".join(INPUT_PATH + [x for x in extra if x not in INPUT_PATH])
    return None


def app_file_terms(f):
    """app/<path> -> (set of test-name regexes, whole_warp, reasons). The mapping derive() applies to a changed app file."""
    if not f.startswith("app/"):
        return set(), False, []
    m = re.match(r"^app/src/(.+)$", f)
    segs = m.group(1).split("/") if m else []
    if segs and segs[0] in ("benches",) or f.startswith("app/benches/"):
        return set(), False, []                            # benches are not tests
    if not f.endswith(".rs"):
        return set(), True, ["%s -> whole warp package (non-Rust app file)" % f]
    ip = input_path_test(f)
    if ip:
        return {ip}, False, ["%s -> test(/%s/) (input-path override)" % (f, ip)]
    segs = [re.sub(r"\.rs$", "", x) for x in segs]
    if "tests" in segs[:-1]:
        segs = segs[:segs.index("tests")]          # input_queue/tests/ops.rs -> input_queue
    if segs and segs[-1] in ("mod", "tests", "lib", "main"):
        segs = segs[:-1]
    segs = [re.sub(r"_(snapshot|model)$", "", re.sub(r"_tests?$", "", x)) for x in segs]
    if len(segs) >= 2 and segs[0] != "":
        name = "::".join(segs[-2:]) if len(segs) > 2 else "::".join(segs)
        return {name}, False, ["%s -> test(/%s/)" % (f, name)]
    if len(segs) == 1:
        return {segs[0]}, False, ["%s -> test(/%s/)" % (f, segs[0])]
    return set(), True, ["%s -> whole warp package" % f]


# A changed library crate pulls in the app modules that USE it (#1504): app/src files naming `<crate>::` are mapped like changed app files.
# More users than this and the whole warp package runs (a crate that much of the app uses is not a narrow change).
DEPENDENT_FILES_CAP = 40


def app_dependents(pkg):
    """-> app/src/*.rs paths that name the crate `pkg` (`use pkg::`, `pkg::x`), via git grep on the working tree; None if grep failed."""
    ident = pkg.replace("-", "_")
    r = subprocess.run(["git", "grep", "-l", "-E", r"(^|[^A-Za-z0-9_:])%s::" % re.escape(ident), "--", "app/src/*.rs", "app/src/**/*.rs"],
                       capture_output=True, text=True)
    if r.returncode not in (0, 1):
        return None
    return sorted({l for l in r.stdout.splitlines() if l})


def derive(files):
    """-> (test-linux filterset or '', set of suites to run among {'linux','core','rest'}, reasons).
    Only test-linux (the ~50 min suite) is filtered; warpui-core (~2 min) and workspace-rest (~4 min) run whole, but only when touched."""
    names = crate_names()
    app_tests, linux_pk, suites, why = set(), set(), set(), []
    dependents_done = set()
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
                suites.add("rest"); why.append("%s -> package(%s): workspace-rest suite" % (f, p))
                if p in dependents_done:
                    continue
                dependents_done.add(p)
                users = app_dependents(p)
                if users is None or len(users) > DEPENDENT_FILES_CAP:
                    suites.add("linux"); whole_warp = True
                    why.append("%s -> whole warp: %s" % (f, "git grep for its users failed" if users is None else "%d app files use %s" % (len(users), p)))
                elif users:
                    suites.add("linux")
                    for u in users:
                        terms, whole, _ = app_file_terms(u)
                        app_tests |= terms; whole_warp = whole_warp or whole
                    why.append("%s -> %d app file(s) using %s mapped like changed app files" % (f, len(users), p))
            continue
        if f.startswith("app/"):
            suites.add("linux")
            terms, whole, w = app_file_terms(f)
            app_tests |= terms; whole_warp = whole_warp or whole; why += w
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


def self_test():
    """Table-driven checks of derive(): `python3 script/ci/ci_seq.py --self-test`. Exit 0 = all pass."""
    t = "app/src/terminal/"
    base = "terminal::(input_queue|pane_key|view|model::key_facts|model::typeahead_handoff"
    cases = [
        # (changed files, substrings the filter must contain, suites that must be selected)
        ([t + "input_queue/destination.rs"], [base + ")"], {"linux"}),
        ([t + "input_queue/gates.rs"], [base + ")"], {"linux"}),
        ([t + "input_queue/tests/ops.rs"], [base + ")"], {"linux"}),
        ([t + "view.rs"], [base + "|writeable_pty|local_tty)"], {"linux"}),
        ([t + "view/dest_table.rs"], [base + "|writeable_pty|local_tty)"], {"linux"}),
        ([t + "view_test.rs"], [base + "|writeable_pty|local_tty)"], {"linux"}),
        ([t + "model/typeahead_handoff.rs"], [base + ")"], {"linux"}),
        ([t + "local_tty/event_loop.rs"], ["local_tty", "input_queue"], {"linux"}),
        ([t + "pane_key.rs"], [base + ")"], {"linux"}),
        ([t + "bootstrap.rs"], ["bootstrap", "input_queue"], {"linux"}),
        ([t + "model/bootstrap.rs"], ["model::bootstrap"], {"linux"}),
        ([t + "blockgrid_thing/foo.rs"], ["test(/"], {"linux"}),
        (["docs/testing.md"], [], set()),
        (["Cargo.lock"], [], {"linux", "core", "rest"}),
        # #1504: a crate change pulls in the app modules that use it (few users: their modules; many: the whole warp package)
        (["crates/sum_tree/src/lib.rs"], ["(package(warp) & (test(/"], {"linux", "rest"}),
        (["crates/warp_core/src/lib.rs"], ["package(warp) | package(warp_tui)"], {"linux", "rest"}),
    ]
    bad = 0
    for files, must, suites in cases:
        filt, got, _ = derive(files)
        ok = all(m in filt for m in must) and suites <= set(got) and (bool(filt) or "Cargo.lock" in files[0] or not must)
        if not ok:
            bad += 1
        print("%s %s -> %r %s" % ("ok  " if ok else "FAIL", files, filt, sorted(got)))
    # the old behaviour this replaces: nothing may select a module path that has no tests
    f, _, _ = derive([t + "input_queue/destination.rs"])
    if "input_queue::destination" in f:
        bad += 1; print("FAIL destination.rs still maps to the empty input_queue::destination")
    # #1517: play() response checking and the per-job running deadline
    for resp, want_err in [({"id": 1, "status": "pending"}, False), ({"id": 1, "status": "created"}, False), ({"message": "400 Bad request"}, True),
                           ({"error": "403 Forbidden"}, True), (None, True), ([], True), ({"id": 1, "status": "manual"}, True)]:
        ok = bool(play_error(resp)) == want_err
        bad += 0 if ok else 1
        print("%s play_error(%r) -> %r" % ("ok  " if ok else "FAIL", resp, play_error(resp)))
    yml = "build-x:\n  stage: b\n  timeout: 90m\nother:\n  timeout: 3h\n"
    for nm, want in [("build-x", 5400), ("other", 10800), ("absent", 18000)]:
        got = job_timeout_s(nm, yml)
        bad += 0 if got == want else 1
        print("%s job_timeout_s(%s) = %d" % ("ok  " if got == want else "FAIL", nm, got))
    # #1527: a .gitlab-ci.yml change to a build job builds that platform (build_changed injects what ci_build_platforms would find)
    for files, changed, want in (
        ([".gitlab-ci.yml"], {WIN}, [WIN]),
        ([".gitlab-ci.yml"], set(), [LINUX]),
        ([".gitlab-ci.yml", "docs/testing.md"], {MAC, WIN}, [MAC, WIN]),
        ([".gitlab-ci.yml", "app/src/terminal/view.rs"], {WIN}, [LINUX, MAC, WIN]),
        ([".gitlab-ci.yml", "script/windows/bundle.ps1"], {LINUX}, [LINUX, WIN]),
        (["script/ci/ci_seq.py"], {WIN}, [LINUX]),          # no yml change: build_changed is never consulted
    ):
        got = platforms_from(files, build_changed=changed)
        if got != want:
            bad += 1
        print("%s platforms_from(%s, build job changed %s) -> %s" % ("ok  " if got == want else "FAIL", files, sorted(changed), got))
    print("self-test: %s" % ("FAILED (%d)" % bad if bad else "all passed"))
    return 1 if bad else 0


def derive_env(before, head, suite):
    """For the main-push CI jobs: print shell assignments `RUN_SUITE=0|1` and `TEST_FILTER=...` for the diff before..head.
    Any failure to compute the diff selects the full suite (RUN_SUITE=1, no filter): never silently run less."""
    r = subprocess.run(["git", "diff", "--name-only", "%s..%s" % (before, head)], capture_output=True, text=True)
    if r.returncode or not before.strip("0"):
        sys.stderr.write("derive-env: cannot diff %s..%s (%s); running the full suite\n" % (before[:10], head[:10], r.stderr.strip()))
        print("RUN_SUITE=1\nTEST_FILTER=")
        return 0
    filt, suites, why = derive([l for l in r.stdout.splitlines() if l])
    for w in why[:15]:
        sys.stderr.write("derive-env: %s\n" % w)
    run = suite in suites
    import shlex
    print("RUN_SUITE=%d\nTEST_FILTER=%s" % (1 if run else 0, shlex.quote(filt) if suite == "linux" else ""))
    return 0


def ci_build_platforms(base, ref):
    """#1527: the platforms whose BUILD job definition (the job block plus the template it extends) differs in .gitlab-ci.yml between
    base and ref. A yml edit that changes, say, build-windows-x86_64's env must build Windows, not only the linux default."""
    out = set()
    for p, job in BUILD.items():
        a, b = ci_blocks(base, job), ci_blocks(ref, job)
        keep = (0, 2) if p == WIN else (0, 1)          # ci_blocks -> [job, .shim_github_env, .windows_job]
        if [a[i] for i in keep] != [b[i] for i in keep]:
            out.add(p)
    return out


def platforms_from(files, base=None, ref=None, build_changed=None):
    """-> the platforms to build for a fix-tier change. script/ci/ and .gitlab-ci.yml are not app code, but when base/ref are given a
    .gitlab-ci.yml change to a platform's build job adds that platform (#1527); when nothing else needs a build, exactly those platforms
    are built. build_changed: injected ci_build_platforms result (self-test)."""
    plats = platforms_from_files(files)
    if ".gitlab-ci.yml" not in files or not (build_changed is not None or (base and ref)):
        return plats
    extra = build_changed if build_changed is not None else ci_build_platforms(base, ref)
    if not extra:
        return plats
    if not app_code(files):
        return sorted(extra)
    return sorted(set(plats) | extra)


def app_code(files):
    return [f for f in files if not DOC_ONLY.match(f) and not re.match(r"^(script/ci/|\.gitlab-ci\.yml|script/(check|precheck|state|test_))", f)]


def platforms_from_files(files):
    code = app_code(files)
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
    """-> GitLab's parsed JSON answer (an error body for a 4xx), or None when GitLab gave no answer within glapi's deadline.
    Rate limits (429) and transient 5xx must never read as "no such job" (that ended a run early on 2026-11-02): glapi waits as
    long as Retry-After says and paces before the window runs out."""
    body = hdrs = None
    if variables:   # play API: job_variables_attributes must arrive as a JSON array of {key,value}
        hdrs = ["Content-Type: application/json"]
        body = json.dumps({"job_variables_attributes": [{"key": k, "value": v} for k, v in variables.items()]})
    try:
        _, out = glapi.call(path, method, body=body, headers=hdrs or ())
    except glapi.GitLabUnavailable as e:
        print("%s: %s" % (time.strftime("%H:%M"), e), flush=True)
        return None
    try:
        return json.loads(out or b"null")
    except ValueError:
        return None


def api_raw(path):
    """-> (HTTP status, body text) for a non-JSON resource (job trace, artifact file); (None, "") when GitLab gave no answer."""
    try:
        st, out = glapi.call(path)
    except glapi.GitLabUnavailable as e:
        print("%s: %s" % (time.strftime("%H:%M"), e), flush=True)
        return None, ""
    return st, out.decode(errors="replace")


def play_error(resp):
    """None when a play API response means the job was started; else why not. glab api prints GitLab's JSON error body
    ({"message": ...}) on a refused call, and None/non-dict means no usable answer at all."""
    if not isinstance(resp, dict):
        return "no usable response from the GitLab API (%r)" % (resp,)
    if resp.get("message") or resp.get("error"):
        return str(resp.get("message") or resp.get("error"))
    if resp.get("status") in (None, "manual", "failed", "canceled"):
        return "job did not leave 'manual' (status %r)" % resp.get("status")
    return None


def job_timeout_s(name, yml=None):
    """The job's `timeout:` from .gitlab-ci.yml in seconds (the first one in the job's block); 5 h if not found."""
    try:
        txt = yml if yml is not None else open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", ".gitlab-ci.yml")).read()
        m = re.search(r"^%s:\s*\n(?:(?!\S).*\n)*?\s+timeout:\s*(\d+)\s*([mhs])" % re.escape(name), txt, re.M)
        if m:
            return int(m.group(1)) * {"s": 1, "m": 60, "h": 3600}[m.group(2)]
    except OSError:
        pass
    return 5 * 3600


def main():
    ap = argparse.ArgumentParser(description="Drive a GitLab pipeline through a test tier; see the header of script/ci/ci_seq.py and docs/testing.md")
    ap.add_argument("ref", nargs="?", default="")
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
    ap.add_argument("--self-test", action="store_true", help="run the table-driven checks of the TEST_FILTER derivation and exit")
    ap.add_argument("--files", default="", help="with --dry-run: a comma-separated FAKE changed-file list instead of the git diff (prints the filter)")
    ap.add_argument("--derive-env", nargs=3, metavar=("BEFORE", "HEAD", "SUITE"), help="CI use: print RUN_SUITE/TEST_FILTER shell assignments for BEFORE..HEAD (SUITE = linux|core|rest)")
    ap.add_argument("--pending-timeout", type=float, default=20, metavar="MIN",
                    help="a job still 'pending' (no runner picked it up) after this many minutes is reported RUNNER-OFFLINE and the run continues (default 20)")
    ap.add_argument("--running-factor", type=float, default=2.0, metavar="X",
                    help="a job still 'running' after X times its own .gitlab-ci.yml timeout is reported RUNNING-DEADLINE and the run continues (default 2)")
    ap.add_argument("--dry-run", action="store_true", help="print the plan (filter, suites, platforms) and exit")
    a = ap.parse_args()
    if a.self_test:
        return self_test()
    if a.derive_env:
        return derive_env(*a.derive_env)
    if a.files and a.dry_run:                      # fake file list: no git, no pipeline
        files = [f for f in a.files.split(",") if f]
        filt, suites, why = derive(files)
        for w in why:
            print("  ", w)
        print("TEST_FILTER for test-linux:", filt or "(none: nothing in test-linux's scope changed)")
        print("suites:", sorted(suites) or "none", " platforms:", platforms_from(files))
        return 0
    if not a.ref:
        ap.error("ref is required")
    ref, tier = a.ref, a.tier
    SKIP = set(a.skip_pos) | set(a.skip)
    PENDING_LIMIT[0] = a.pending_timeout * 60
    RUNNING_FACTOR[0] = a.running_factor

    sh("git", "fetch", "-q", "origin", check=True)
    base = resolve_base(ref, a.base)
    files = changed_files(base, ref) if tier == "fix" or a.dry_run else []
    filt, suites, plats = "", {"linux", "core", "rest"}, [LINUX, MAC, WIN]
    if tier == "fix":
        filt, suites, why = derive(files)
        plats = [p for p in a.platforms.split(",") if p] or platforms_from(files, base, gitref(ref))
        if a.tests:
            filt = a.tests; suites = set(suites) | {"linux"}
        # #1496: test-workspace-rest (~4 min, the 40 other packages) runs in EVERY fix tier with a code change, not only when a
        # crate changed: a red suite nobody plays is a red suite nobody owns.
        if any(not DOC_ONLY.match(f) for f in files):
            suites = set(suites) | {"rest"}
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
        # r18cz: a network outage made every running job read "missing" and the run ended while the pipeline ran on. An unanswered
        # API call is UNKNOWN, never "no such job": keep asking until GitLab answers.
        while True:
            js = api("projects/%s/pipelines/%d/jobs?per_page=100" % (PROJECT, pid))
            if isinstance(js, list):
                return {j["name"]: j for j in js}
            print("%s: GitLab API unreachable; retrying" % time.strftime("%H:%M"), flush=True)
            time.sleep(60)

    def wait(name):
        pending_since = None
        running_since = None
        while True:
            s = jobs().get(name, {}).get("status", "missing")
            if s in ("success", "failed", "canceled", "missing", "skipped"):
                j = jobs().get(name, {})
                timeline[name] = (s, j.get("duration"))
                return s
            # #1517: 'pending' = queued for a runner. A Mac/Windows runner that is offline leaves it pending forever; report
            # it and let the other jobs go on. (waiting_for_resource, a resource_group queue behind another job, is not pending.)
            if s == "pending":
                pending_since = pending_since or time.time()
                if time.time() - pending_since > PENDING_LIMIT[0]:
                    print("%s: RUNNER-OFFLINE (pending with no runner for %.0f min); continuing without it" % (name, (time.time() - pending_since) / 60), flush=True)
                    timeline[name] = (RUNNER_OFFLINE, None)
                    return RUNNER_OFFLINE
            else:
                pending_since = None
            # #1517: a job that stays 'running' far past its own `timeout:` (a wedged runner, a lost connection GitLab never noticed)
            # must not block the run forever either: give up after RUNNING_FACTOR x the job's yaml timeout (--running-factor)
            if s == "running":
                running_since = running_since or time.time()
                lim = job_timeout_s(name) * RUNNING_FACTOR[0]
                if time.time() - running_since > lim:
                    print("%s: RUNNING-DEADLINE (still running after %.0f min = %.1fx its %.0f min timeout); continuing without it" % (
                        name, (time.time() - running_since) / 60, RUNNING_FACTOR[0], job_timeout_s(name) / 60), flush=True)
                    timeline[name] = (RUNNING_DEADLINE, None)
                    return RUNNING_DEADLINE
            else:
                running_since = None
            time.sleep(60)

    def play(name, variables=None):
        j = jobs().get(name)
        if not j or j["status"] != "manual":
            return False                                   # already played/run or absent: never re-run
        resp = api("projects/%s/jobs/%d/play" % (PROJECT, j["id"]), "POST", variables)
        err = play_error(resp)
        if err:      # #1517: a refused play used to read as started and the run then waited on a job that never ran
            sys.stderr.write("ERROR: play %s (job %s) failed: %s\n" % (name, j["id"], err))
            timeline[name] = ("play-failed", None)
            summary(timeline, t_start); collect_hint(pid, a.collect)
            sys.exit(2)
        return True

    print("pipeline", pid, "tier", tier, flush=True)
    # r18e4: the Mac and Windows builds have `needs: []` and run on their own machines; holding them behind a check-linux that
    # waits for the busy box left those machines idle for hours. Start them now (a compile error fails them on their own).
    for p in (MAC, WIN):
        if p in plats and p not in reuse and BUILD[p] not in SKIP and play(BUILD[p]):
            print("%s: started %s without waiting for check-linux (own machine)" % (time.strftime("%H:%M"), BUILD[p]), flush=True)
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
                print("%s: smoke tier skips the x11 pass; not played" % n)
                timeline[n + "-reuse"] = (TIER_SKIP, None); continue
            if play(n + "-reuse", rv):
                reuse_started.append(n + "-reuse")
    for n in builds:
        play(n)
    # Each platform's tests start as soon as ITS build succeeds, not after the box test suites (those take ~1 h and
    # would hold the Mac and Windows tests back for no reason; app tests on other machines do not compete with the box).
    # Owner 2026-11-05: on each machine the UNIT suites run first, then the app tests, then (release tier) the heavy suites,
    # one at a time, so a logic regression shows in minutes and compile load never overlaps the latency cases.
    p_of = {BUILD[p]: p for p in plats}
    started, lock = [], threading.Lock()

    def run_suites(names):
        """Play the suite jobs one after the other on their machine; a suite that cannot exist there is reported, not played."""
        for n in names:
            if n in SKIP or n not in jobs():
                continue
            if n in UNSUPPORTED:
                print("%s: not played, unsupported on this platform: %s" % (n, UNSUPPORTED[n]), flush=True)
                timeline[n] = (PLATFORM_UNSUPPORTED, None); continue
            if play(n):
                print("%s: started %s" % (time.strftime("%H:%M"), n), flush=True)
            if jobs().get(n, {}).get("status") not in (None, "manual"):
                print(n, wait(n), flush=True)

    def after_build(b):
        bs = wait(b)
        if bs != "success":
            for n in [t for t in APP[p_of[b]] if t not in SKIP and t in jobs()]:
                timeline[n] = ("not run (%s %s)" % (b, bs), None)   # a build that failed leaves its app tests unrun: that is a failure, not a pass
            return
        if tier != "fix" and p_of[b] in PLAT_SUFFIX:
            run_suites(plat_suites(p_of[b], False))     # unit suites first, on the same machine, before its app tests
        for n in [t for t in APP[p_of[b]] if t not in SKIP and t in jobs()]:
            if tier == "fix" and n == "app-test-linux-x11":
                timeline[n] = (TIER_SKIP, None)
                continue
            if play(n, appvars):
                print("%s: %s succeeded, started %s" % (time.strftime("%H:%M"), b, n), flush=True)
                with lock:
                    started.append(n)
    watchers = [threading.Thread(target=after_build, args=(b,), daemon=True) for b in builds]
    for t in watchers:
        t.start()
    watch_of = {b: t for b, t in zip(builds, watchers)}

    def plat_unit_suites(p):
        """candidate/release: after this platform's unit suites and app tests (played by after_build), its heavy suites
        (release tier only), one after the other on the same machine. A build that failed leaves its unit suites unplayed."""
        if BUILD[p] in watch_of:
            watch_of[BUILD[p]].join()                       # units ran and the app tests were played (or the build failed) after this
        for n in APP[p]:
            with lock:
                mine = n in started
            if mine:
                print(n, wait(n), flush=True)               # do not overlap the app tests on the same machine
        units = plat_suites(p, False)
        if BUILD[p] not in watch_of:
            run_suites(units)                               # no build of ours to wait for (reused): units still run
        run_suites([n for n in plat_suites(p, tier == "release") if n not in units])
    suite_threads = []
    if tier != "fix":
        suite_threads = [threading.Thread(target=plat_unit_suites, args=(p,), daemon=True) for p in plats if p in PLAT_SUFFIX]
        for t in suite_threads:
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
    for t in suite_threads:
        t.join()                                            # the Mac/Windows suites are counted in the verdict like every other job
    for n in builds:
        print(n, timeline.get(n, ("?",))[0], flush=True)
    for n in started + reuse_started: print(n, wait(n), flush=True)
    if tier == "release":
        for n in ["test-shuttle-loom", "fuzz-smoke", "bench", "mutants", "mutants-warpui-core"]:
            if n not in SKIP and jobs().get(n, {}).get("status") not in (None, "manual"): print(n, wait(n), flush=True)
    network_jobs(jobs, wait, timeline)
    check_app_results(jobs, timeline)
    summary(timeline, t_start)
    collect_hint(pid, a.collect)
    return 0 if all(verdict(k, v[0]) not in ("FAIL", "OFFLINE") for k, v in timeline.items()) else 1


def network_jobs(jobs, wait, timeline):
    """#1518: licenses/advisories run automatically in every pipeline and need the network. Report them in the timeline; a failure that is a
    fetch outage (and shows no finding) is NETWORK-FAIL, reported separately and not as a code failure; a genuine finding stays FAIL."""
    for n in NETWORK_JOBS:
        j = jobs().get(n)
        if not j or j["status"] in ("manual", "skipped"):
            continue
        s = wait(n)
        if s == "failed":
            j = jobs().get(n, {})
            timeline[n] = (classify_network_job(j["id"]) if j.get("id") else s, j.get("duration"))
        print(n, timeline.get(n, (s,))[0], flush=True)


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


TIER_SKIP = "skipped by tier"
PLATFORM_UNSUPPORTED = "unsupported on platform"
RUNNER_OFFLINE = "RUNNER-OFFLINE"
RUNNING_DEADLINE = "RUNNING-DEADLINE"
RUNNING_FACTOR = [2.0]             # set from --running-factor
NETWORK_FAIL = "NETWORK-FAIL"
PENDING_LIMIT = [20 * 60]          # seconds; set from --pending-timeout
NETWORK_JOBS = ("licenses", "advisories")   # need the network (advisory DB / licence sources / cargo-binstall); automatic, not allow_failure
# What a cargo-deny/binstall FETCH outage looks like. A real finding prints `error[vulnerability]` / `error[unmaintained]` / `error[unsound]`
# / `error[rejected]` / `error[banned]` etc.: if any of those is in the trace the job is a genuine FAIL whatever else the trace says.
NETWORK_RX = re.compile(r"failed to (fetch|clone)|could not resolve host|couldn.t resolve|error sending request|connection (reset|refused|timed out)|"
                        r"timed out|temporary failure in name resolution|unable to (access|update)|network is unreachable|SSL|502|503|504|"
                        r"failed to download|binstall.*(fail|error)|RPC failed|early EOF", re.I)
FINDING_RX = re.compile(r"error\[[a-z-]+\]|^error: .*(vulnerabilit|advisor|licen[cs]e|banned|unmaintained|unsound)", re.I | re.M)


def classify_network_job(job_id):
    """-> NETWORK_FAIL when a failed licenses/advisories job failed to FETCH (and shows no finding), else 'failed'."""
    _, trace = api_raw("projects/%s/jobs/%d/trace" % (PROJECT, job_id))
    if FINDING_RX.search(trace):
        return "failed"
    return NETWORK_FAIL if NETWORK_RX.search(trace) else "failed"


APP_RESULT_DIR = {"app-test-linux-wayland": "wayland", "app-test-linux-x11": "x11", "app-test-macos": "macos", "app-test-windows": "windows"}


def judged_nothing(doc):
    """#1508: True when an app run's results.json judged no case: no rows, or nothing but skips (the app jobs are allow_failure, so a green
    job with an empty run must still be a FAIL here). doc = parsed results.json."""
    s = (doc or {}).get("summary") or {}
    return not (s.get("pass") or s.get("fail") or s.get("noref"))


def check_app_results(jobs, timeline):
    """Second line of defence behind run.py's own non-zero exit: a successful app job whose results.json judged nothing becomes a failure."""
    for n, d in APP_RESULT_DIR.items():
        j = jobs().get(n)
        if not j or timeline.get(n, ("",))[0] != "success" or not j.get("id"):
            continue
        st, text = api_raw("projects/%s/jobs/%d/artifacts/appci-out/%s/run/results.json" % (PROJECT, j["id"], d))
        try:
            doc = json.loads(text) if st == 200 else None
        except ValueError:
            doc = None
        if doc is None:
            print("%s: results.json unreadable (%s, HTTP %s): cannot confirm the run judged anything" % (n, d, st), flush=True)
            continue
        if judged_nothing(doc):
            timeline[n] = ("success but its run judged NOTHING", timeline[n][1])
            print("%s: green job, but results.json has no pass/fail row: FAIL (#1508)" % n, flush=True)


def verdict(name, status):
    """Per-job verdict. Every job in the timeline counts, app tests included (they are allow_failure in GitLab, so the
    pipeline is green regardless; this verdict is the one that is not). Only an explicit tier skip is neutral; bench,
    mutants and fuzz are informational (not gating) and keep their status. Anything else that is not success fails."""
    if status in (TIER_SKIP, PLATFORM_UNSUPPORTED):
        return "SKIPPED"
    if status == "success":
        return "PASS"
    if status == RUNNER_OFFLINE:
        return "OFFLINE"
    if status == RUNNING_DEADLINE:
        return "TIMEOUT"
    if status == NETWORK_FAIL:
        return "NETWORK"
    if name.startswith(("bench", "mutants", "fuzz")):
        return "INFO"
    return "FAIL"


def summary(timeline, t0):
    print("\n%-28s %-18s %-8s %s" % ("job", "status", "verdict", "minutes"))
    for k, (s, d) in timeline.items():
        print("%-28s %-18s %-8s %s" % (k, s, verdict(k, s), "%.1f" % (d / 60) if d else "-"))
    bad = [k for k, (s, _) in timeline.items() if verdict(k, s) in ("FAIL", "OFFLINE")]
    net = [k for k, (s, _) in timeline.items() if verdict(k, s) == "NETWORK"]
    off = [k for k, (s, _) in timeline.items() if verdict(k, s) == "OFFLINE"]
    print("overall: %s" % ("FAIL (%s)" % ", ".join(bad) if bad else "PASS"))
    if off:
        print("RUNNER-OFFLINE (not a code failure; the runner never picked the job up): %s" % ", ".join(off))
    if net:
        print("NETWORK-DEPENDENT jobs that could not fetch (NOT a code failure and NOT a pass: re-run them, nothing was checked): %s" % ", ".join(net))
    print("wall clock: %.1f min" % ((time.time() - t0) / 60), flush=True)


if __name__ == "__main__":
    sys.exit(main())
