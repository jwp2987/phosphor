#!/usr/bin/env python3
# script/ci/collect_results.py: archive one GitLab pipeline's test results under test-results/ (owner rule 2026-11-02: "every test
# result is kept in the repo for regression review"). The coordinator commits the output; CI never pushes.
#
#   python3 script/ci/collect_results.py <pipeline_id | build/ref | ref> [--out test-results] [--tier fix|candidate|release|targeted] [--cases-file FILE]
#   python3 script/ci/collect_results.py --self-test        (the pure parts: usage-suite parsing, tier inference; no network)
#
# Writes <out>/<YYYY-MM-DD>_<ref-sanitized>_<pipeline>/ with manifest.json, unit/<job>.json, app/<platform>/..., README.md, and
# rebuilds <out>/INDEX.md. Idempotent: re-running for a pipeline replaces that run's directory only (a still-running pipeline can be
# collected again once more of it has finished). Every README and INDEX.md is a pure function of the manifests, so they are
# regenerated for all runs each time. Needs glab authenticated ('glab api user'). Privacy: results_scrub.py; size: <= 5 MB per run
# script/check_test_results fails at 10 MB. Screenshots are never copied (public repo).
import time
import argparse, glob, io, json, os, re, shutil, subprocess, sys, zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "..", "realapp", "appci"))
import results_scrub
import lat_evidence                            # the latency cases keep their input_order / frame hop lines
import glapi                                    # every GitLab call: honours Retry-After / RateLimit-*
try:
    import lat as appci_lat                       # the regression rule shared with the harness (5 % and 0.5 ms)
except Exception:
    appci_lat = None

PROJECT = "88085499"
WARN_BYTES = 5 * 1024 * 1024
# Every box suite also runs on the Mac and Windows runners (<suite>-macos / <suite>-windows; test-macos / test-windows is the lib suite):
# archived exactly like the box job (unit/<job>.json), judged per platform in the history.
PLATS = ("macos", "windows")
UNIT_JOBS = ("test-linux", "test-warpui-core", "test-workspace-rest", "integration-linux", "test-shuttle-loom") + \
    tuple("%s-%s" % (n, p) for p in PLATS for n in ("test", "test-warpui-core", "test-workspace-rest", "test-shuttle-loom"))
NUMBERS_JOBS = ("bench", "fuzz-smoke", "mutants", "mutants-warpui-core") + tuple("%s-%s" % (n, p) for p in PLATS for n in ("bench", "fuzz-smoke", "mutants", "mutants-warpui-core"))
GUARD_JOBS = ("guards", "advisories", "licenses")
# round 15 (#2064): the usage / acceptance suite (script/usage-test). It prints one NDJSON object per scenario and a final {"summary": ...} on stdout
# (crates/usage_suite/src/report.rs); the job had a row in the job table and no result. Archived as unit/<job>.json in the shape of a nextest suite.
USAGE_JOBS = ("usage-test-linux", "usage-test-macos")
# round 15 (#2063): the two lab set-up jobs keep their ERROR lines (a lab-helper that lacks its build tools says which, #2062)
SETUP_JOBS = ("lab-helper", "sshlab-image")
RELEASE_JOBS = ("test-shuttle-loom", "fuzz-smoke", "bench", "mutants", "mutants-warpui-core") + tuple("%s-%s" % (n, p) for p in PLATS for n in ("test-shuttle-loom", "fuzz-smoke", "bench", "mutants", "mutants-warpui-core"))
ANSI = re.compile(r"\x1b\[[0-9;?]*[A-Za-z]")
PREFIX = re.compile(r"^\d{4}-\d\d-\d\dT[\d:.]+Z \d\d[OE]\+? ?")
FIN = ("success", "failed")


def api(path, raw=False):
    """GitLab answer for path (parsed JSON, or bytes with raw). glapi honours Retry-After / RateLimit-* and retries outages for up to
    30 min (r18cz: one dropped connection lost a whole round's collection); a 404 or other 4xx raises, as before."""
    try:
        st, out = glapi.call(path)
    except glapi.GitLabUnavailable as e:
        raise RuntimeError(str(e))
    if st != 200:
        raise RuntimeError("glab api %s failed: HTTP %s %s" % (path, st, out.decode(errors="replace").strip()[:300]))
    return out if raw else json.loads(out or b"null")


def trace(job_id):
    t = api("projects/%s/jobs/%s/trace" % (PROJECT, job_id), raw=True).decode(errors="replace")
    return [PREFIX.sub("", ANSI.sub("", l)).rstrip() for l in t.splitlines()]


def dump_scrubbed(obj, path):
    """unit/*.json carry trace text (bench/fuzz-smoke: cargo's 'Running on <host>...'): scrubbed like every other file (r18e4: the box host name reached the guards job)"""
    open(path, "w").write(results_scrub.scrub(json.dumps(obj, indent=1)))


def sanitize(ref):
    return re.sub(r"[^A-Za-z0-9._-]+", "_", ref).strip("_")


def resolve(arg):
    if arg.isdigit():
        return api("projects/%s/pipelines/%s" % (PROJECT, arg))
    ps = api("projects/%s/pipelines?ref=%s&per_page=1" % (PROJECT, arg.replace("/", "%2F")))
    if not ps:
        sys.exit("collect_results: no pipeline for ref %r" % arg)
    return api("projects/%s/pipelines/%s" % (PROJECT, ps[0]["id"]))


def all_jobs(pid):
    out, page = [], 1
    while True:
        js = api("projects/%s/pipelines/%s/jobs?per_page=100&page=%d" % (PROJECT, pid, page))
        out += js
        if len(js) < 100:
            return out
        page += 1


# ---- unit (nextest) traces ----------------------------------------------------------------------------------------------------
SUMMARY = re.compile(r"Summary \[\s*([\d.]+)s\]\s+(\d+) tests? run:\s*(.*)")
# #2122: an integration scenario whose own predicate said no exits 0, which nextest counts as passed. The job's last step (script/ci/integ_evidence.py)
# prints one line per such scenario; they are taken out of "passed" here and listed with what they need (a skip that is not listed reads as coverage).
SELF_SKIP = re.compile(r"^\s*INTEG-SKIP\s+(\S+)\s+NEEDS:\s*(.*)$")
STATUS = re.compile(r"^\s*(FAIL|TIMEOUT|SIGSEGV|SIGABRT|SIGKILL|SIGBUS|SIGILL|ABORT|LEAK|EXEC-FAIL|FLAKY)\s+(?:\d+/\d+\s+)?\[\s*([\d.]+)s\]\s+(?:\(\s*\d+/\d+\)\s+)?(\S+)\s+(\S.*)$")


def parse_unit(lines):
    d = {"summary_line": None, "duration_s": None, "run": None, "passed": 0, "failed": 0, "timed_out": 0, "skipped": 0, "slow": 0, "flaky": 0,
         "failures": [], "regressions": [], "notes": []}
    seen = {}
    self_skips = {}
    for l in lines:
        m = SELF_SKIP.match(l)
        if m:
            self_skips.setdefault(m.group(1), m.group(2).strip()[:300] or "no reason given")
            continue
        m = SUMMARY.search(l)
        if m:
            d["summary_line"], d["duration_s"], d["run"] = l.strip(), float(m.group(1)), int(m.group(2))
            rest = m.group(3)
            for n, k in re.findall(r"(\d+)\s+(passed|failed|timed out|skipped)", rest):
                d[{"passed": "passed", "failed": "failed", "timed out": "timed_out", "skipped": "skipped"}[k]] = int(n)
            for n, k in re.findall(r"(\d+)\s+(slow|flaky)", rest):
                d[k] = int(n)
            continue
        m = STATUS.match(l)
        if m and m.group(1) != "FLAKY":
            kind, secs, crate, test = m.group(1), float(m.group(2)), m.group(3), m.group(4).strip()
            key = (kind, crate, test)
            if key not in seen:
                seen[key] = {"kind": kind, "crate": crate, "test": test, "seconds": secs}
            continue
        if l.startswith("  + ") and "::" in l:
            d["regressions"].append(l[4:].strip())
        elif re.search(r"known failure|known_test_failures|REGRESSION|tests: ok|STALE|now passes", l) and len(d["notes"]) < 12:
            d["notes"].append(l.strip()[:200])
    d["failures"] = sorted(seen.values(), key=lambda x: (x["crate"], x["test"]))
    d["regressions"] = sorted(set(d["regressions"]))
    if self_skips:
        d["skips"] = [{"surface": "integration", "scenario": n, "reason": r} for n, r in sorted(self_skips.items())]
        if d["run"] is not None:                             # nextest's own line stays in summary_line; the counts say what ran
            n = min(len(self_skips), d["passed"])
            d["passed"], d["run"], d["skipped"] = d["passed"] - n, d["run"] - n, d["skipped"] + n
            d["notes"].append("%d scenario(s) skipped themselves (exit 0, counted as passed by nextest): counted here as skipped, each listed with what it needs" % n)
    if d["run"] is None:                                     # compile error / early abort: keep the cause
        d["errors"] = [l.strip()[:200] for l in lines if re.match(r"\s*(error(\[E\d+\])?:|ERROR)", l)][:8]
    return d


def parse_usage(lines):
    """usage-test-* trace -> the parse_unit shape. Rows: {"surface","scenario","status": pass|fail|skip,"duration_ms","reason","failure_detail",...};
    last line {"summary": {"total","passed","failed","skipped","surfaces"}}. A failed scenario becomes a failure named `usage-<surface> <scenario>`
    (so NEW / FIXED work like for a unit test); EVERY skip is kept with its reason (a skip that is not listed reads as coverage). No summary line (the
    suite died first): the rows seen so far are counted and a note says so; no row at all: run stays None and the trace's error lines are kept."""
    d = {"summary_line": None, "duration_s": None, "run": None, "passed": 0, "failed": 0, "timed_out": 0, "skipped": 0, "slow": 0, "flaky": 0,
         "failures": [], "regressions": [], "notes": [], "skips": [], "surfaces": {}}
    rows, summ = {}, None
    for l in lines:
        t = l.strip()
        if not (t.startswith("{") and t.endswith("}")):
            continue
        try:
            o = json.loads(t)
        except ValueError:
            continue
        if not isinstance(o, dict):
            continue
        if isinstance(o.get("summary"), dict):
            summ, d["summary_line"] = o["summary"], t[:300]
        elif isinstance(o.get("scenario"), str) and o.get("status") in ("pass", "fail", "skip"):
            rows[(str(o.get("surface", "")), o["scenario"])] = o
    cnt = {"pass": 0, "fail": 0, "skip": 0}
    for (surface, name), o in sorted(rows.items()):
        cnt[o["status"]] += 1
        d["surfaces"][surface] = d["surfaces"].get(surface, 0) + 1
        secs = round((o.get("duration_ms") or 0) / 1000.0, 3)
        if o["status"] == "fail":
            d["failures"].append({"kind": "FAIL", "crate": "usage-" + surface, "test": name, "seconds": secs, "detail": str(o.get("failure_detail") or "")[:600]})
        elif o["status"] == "skip":
            d["skips"].append({"surface": surface, "scenario": name, "reason": str(o.get("reason") or "no reason given")[:300]})
    if rows or summ:
        d["passed"], d["failed"], d["skipped"] = cnt["pass"], cnt["fail"], cnt["skip"]
        d["run"] = cnt["pass"] + cnt["fail"]                 # like nextest's "N tests run": skipped scenarios did not run
        d["duration_s"] = round(sum((o.get("duration_ms") or 0) for o in rows.values()) / 1000.0, 1)
    if summ is None and rows:
        d["notes"].append("no summary line in the trace: the suite did not finish; %d scenario rows counted" % len(rows))
    elif summ is not None and (summ.get("passed"), summ.get("failed"), summ.get("skipped")) != (cnt["pass"], cnt["fail"], cnt["skip"]):
        d["notes"].append("the suite's summary (%s passed, %s failed, %s skipped) differs from its rows (%d, %d, %d): the rows are what is recorded (trace cut?)" % (
            summ.get("passed"), summ.get("failed"), summ.get("skipped"), cnt["pass"], cnt["fail"], cnt["skip"]))
    if d["run"] is None:
        d["errors"] = [l.strip()[:200] for l in lines if re.match(r"\s*(error(\[E\d+\])?:|ERROR)", l)][:8]
    return d


TARGETED_NOTE = ("TARGETED run (ci_seq.py --tier targeted): only the cases and suites of its cases file were played. A pass here proves those cases on "
                 "those platforms and nothing else; a job with the note `not run` was not part of it.")


def tier_from_app_meta(app):
    """the tier the app runs recorded themselves (results.json meta.tier = APPCI_TIER: smoke | full | release) -> fix | candidate | release | None"""
    # the ordinary app-test rows only: a lab or replay job played by hand has no APPCI_TIER and would read as "full"
    seen = {str((i.get("meta") or {}).get("tier") or "") for k, i in (app or {}).items() if not k.startswith(("lab-", "replay-"))}
    for mark, tier in (("release", "release"), ("full", "candidate"), ("smoke", "fix")):
        if mark in seen:
            return tier
    return None


def infer_tier(status_of):
    """Tier from which jobs ran, when neither --tier nor an app run says. status_of: {job name: status}. Round 15: a long x11 run no longer means
    "candidate" (the fix tier plays x11 too, #2058); the integration suite does (candidate and release only, #1502)."""
    ran = lambda n: status_of.get(n) not in (None, "manual", "created", "skipped", "waiting_for_resource", "pending")
    if any(ran(n) for n in RELEASE_JOBS):
        return "release"
    if all(ran(n) for n in ("test-linux", "test-warpui-core", "test-workspace-rest")) and any(ran(n) for n in status_of if n.startswith("integration-linux")):
        return "candidate"
    return "fix"


def parse_check(lines):
    crates, kinds, errors = {}, {}, 0
    for l in lines:
        m = re.match(r"warning: `([^`]+)` \((.*?)\) generated (\d+) warnings?", l)
        if m:
            k = "%s (%s)" % (m.group(1), m.group(2))
            crates[k] = max(crates.get(k, 0), int(m.group(3)))
            continue
        m = re.match(r"warning: (.+)", l)
        if m and "spurious network" not in l and "generated" not in l:
            kind = re.sub(r"[`'\"].*", "", m.group(1)).strip(" :") or m.group(1)[:40]
            kinds[kind[:60]] = kinds.get(kind[:60], 0) + 1
        if re.match(r"error(\[E\d+\])?:", l) and "aborting due to" not in l and "could not compile" not in l:
            errors += 1
    return {"warning_count": sum(crates.values()), "warnings_by_crate": crates, "warning_lines_by_kind": dict(sorted(kinds.items(), key=lambda x: -x[1])),
            "errors": errors}


NUM_LINES = re.compile(r"time:\s+\[|Summary|test result|mutants? (tested|caught|missed|unviable)|caught|missed|unviable|fuzz|execs?/s|cov:|DONE|iterations|Running|FAILED|passed|failed|panick", re.I)


def parse_numbers(lines):
    out = [l.strip()[:200] for l in lines if NUM_LINES.search(l) and not l.startswith(("Downloading", "Compiling", "Fetching"))]
    return {"lines": out[:60], "truncated": max(0, len(out) - 60)}


def parse_guards(lines, status):
    bad = [l.strip()[:200] for l in lines if re.match(r"\s*(ERROR|FAIL|error:)", l)]
    return {"failing_lines": bad[:12] if status != "success" else []}


# ---- app-test artifacts -------------------------------------------------------------------------------------------------------

def lat_summary(metrics):
    out = {}
    for key, r in (metrics.get("lat") or {}).items():
        ph = r.get("phosphor") or {}
        out[key] = {"p50": ph.get("p50"), "p95": ph.get("p95"), "n": ph.get("n"), "overhead_ms": r.get("overhead_ms"),
                    "budget_ms": r.get("budget_ms"), "budget_kind": r.get("budget_kind"), "verdict": r.get("verdict")}
    return out


CASE_LINE = re.compile(r"^\[(terminal|agent)\]\s+(\S+)\s+(PASS|FAIL|SKIP|NOREF)\s*(.*)$")


def cases_from_trace(job, plat, names, base):
    cases, cnt = [], {"pass": 0, "fail": 0, "skip": 0, "noref": 0}
    for l in trace(job["id"]):
        m = CASE_LINE.match(l)
        if m:
            st = m.group(3).lower()
            cnt[st] += 1
            shots = [n[len(base + "run/"):] for n in names if n.startswith(base + "run/shots/") and os.path.basename(n).startswith("%s_%s" % (m.group(1), m.group(2)))]
            cases.append({"mode": m.group(1), "name": m.group(2), "status": st, "seconds": None, "detail": [m.group(4)[:300]], "metrics": {}, "shots": shots})
    return {"meta": {"platform": plat, "source": "job trace (no results.json artifact)"}, "summary": cnt, "cases": cases}


def collect_app(job, dest_root, notes):
    """returns manifest app entries (dict platform -> info); files are written under dest_root/<platform>/"""
    out = {}
    if not any(a.get("filename", "").endswith(".zip") for a in job.get("artifacts", [])):
        return out
    z = zipfile.ZipFile(io.BytesIO(api("projects/%s/jobs/%s/artifacts" % (PROJECT, job["id"]), raw=True)))
    names = [n for n in z.namelist() if not n.startswith("/") and ".." not in n.split("/")]
    plats = sorted({n.split("/")[1] for n in names if n.startswith("appci-out/") and n.count("/") >= 2})
    for plat in plats:
        key = ("lab-" if job["name"].startswith("app-test-lab-") else "replay-" if job["name"].startswith("app-test-replay-") else "") + plat      # lab / replay jobs get their own key: they must not replace the ordinary app-test row
        pd = os.path.join(dest_root, key)
        os.makedirs(pd, exist_ok=True)
        base = "appci-out/%s/" % plat
        info = {"job": job["name"], "job_id": job["id"], "status": job["status"], "reuse": job["name"].endswith("-reuse"), "cases": [], "files": []}
        res = {}
        if base + "run/results.json" in names:
            res = json.loads(z.read(base + "run/results.json"))
        info["meta"] = res.get("meta", {})
        info["summary"] = res.get("summary", {})
        fail_shots = []
        if not res:                                  # e.g. Windows when the job dies before report.py: rebuild the table from the job trace
            res = cases_from_trace(job, plat, names, base)
            info["summary"] = res["summary"]
            info["from_trace"] = True
        for c in res.get("cases", []):
            m = c.get("metrics") or {}
            info["cases"].append({"mode": c.get("mode", ""), "name": c["name"], "status": c["status"], "seconds": c.get("seconds"),
                                  "detail": [x[:300] for x in c.get("detail", [])][:6], "lat": lat_summary(m)})
            if c["status"] == "fail":
                fail_shots += [(c, s) for s in c.get("shots", [])]
        # text artifacts: results.json / summary.md / junit.xml, and any other small data file that is not a log or a screenshot
        keep = []
        for n in names:
            if not n.startswith(base) or n.endswith("/"):
                continue
            rel = n[len(base):]
            if rel.startswith(("run/logs/", "run/shots/")) or rel in ("session.log", "results.jsonl") or rel.endswith(".log"):   # logs are never copied wholesale
                continue
            # screenshots/video never enter the repo, whatever the run dir is called (the Mac job uses run_slim/)
            if "/shots/" in "/" + rel or rel.lower().endswith((".jpg", ".jpeg", ".png", ".gif", ".webp", ".mp4", ".webm", ".mov")):
                continue
            keep.append((n, rel))
        # latency evidence (owner 2026-11-09): the per-case app log of a latency case, reduced to its hop lines (lat_evidence.py), scrubbed below
        lat_logs = [(n, n[len(base):]) for n in names if n.startswith(base) and lat_evidence.case_of(n[len(base):])]
        keep += [("appci-out/reused_build.json", "reused_build.json")] if "appci-out/reused_build.json" in names and plat == plats[0] else []
        # The platform budget (so three platforms stay well under check_test_results' 10 MB per run) is shared between the logs by
        # lat_evidence.fair_caps (#2035): spent in name order, ctrl_c_flood_ab's flood left latency_enter 320 bytes and the next mode nothing.
        lat_logs = sorted(lat_logs)
        caps = lat_evidence.fair_caps([lat_evidence.matched_bytes(z.read(n).decode(errors="replace")) for n, _ in lat_logs])
        for (n, rel), cap in zip(lat_logs, caps):
            if cap <= 0:
                continue                                           # no hop line in this log
            kept, hits, nkept = lat_evidence.evidence(z.read(n).decode(errors="replace"), cap)
            if not kept:
                continue
            if nkept < hits:
                notes.append("%s/%s: latency evidence capped at %d KB (%d of %d hop lines kept; the platform budget of %d KB is shared between %d logs)" % (
                    plat, rel, cap // 1024, nkept, hits, lat_evidence.PLATFORM_CAP // 1024, len(lat_logs)))
            text = results_scrub.scrub(kept)
            try:
                import ci_env
                text = ci_env.redact(text, ci_env.get("APPCI_OLLAMA_URL"))
            except Exception:
                pass
            tp = os.path.join(pd, (rel.replace("run/", "", 1) if rel.startswith("run/") else rel)[:-len(".log")] + ".lat.txt")
            os.makedirs(os.path.dirname(tp), exist_ok=True)
            open(tp, "w").write(text)
            info["files"].append(os.path.relpath(tp, dest_root))
        for n, rel in keep:
            raw = z.read(n)
            if len(raw) > 1024 * 1024:
                notes.append("%s/%s: %d KB skipped (over 1 MB)" % (plat, rel, len(raw) // 1024))
                continue
            text = results_scrub.scrub(raw.decode(errors="replace"))
            if rel == "reused_build.json":
                try:
                    info["reused_build"] = json.loads(text)
                except ValueError:
                    pass
            tp = os.path.join(pd, rel.replace("run/", "", 1) if rel.startswith("run/") else rel)
            os.makedirs(os.path.dirname(tp), exist_ok=True)
            if os.path.basename(tp) == "results.json":                       # detail strings can name paths/hosts: scrubbed above
                pass
            open(tp, "w").write(text)
            info["files"].append(os.path.relpath(tp, dest_root))
        # Screenshots are NEVER stored in the repo (owner 2026-11-02: no screenshots in a public repo).
        # Failing cases keep a pointer to the CI job artifact that holds them (7-day expiry).
        info["screenshots_in_ci_artifacts"] = [s for _, s in fail_shots]
        out[key] = info
    return out


def dir_size(p):
    return sum(os.path.getsize(os.path.join(r, f)) for r, _, fs in os.walk(p) for f in fs)


# ---- rendering (pure functions of manifests) ------------------------------------------------------------------------------------
def unit_totals(m):
    t = {"passed": 0, "failed": 0, "skipped": 0, "run": 0, "jobs": 0}
    for j in m.get("unit", {}).values():
        if j.get("run") is not None:
            t["jobs"] += 1
            t["passed"] += j.get("passed", 0)
            t["failed"] += j.get("failed", 0) + j.get("timed_out", 0)
            t["skipped"] += j.get("skipped", 0)
            t["run"] += j["run"]
    return t


def unit_failure_set(m):
    return {"%s %s" % (f["crate"], f["test"]) for j in m.get("unit", {}).values() for f in j.get("failures", [])}


def app_cases(m):
    return {(p, c["mode"], c["name"]): c for p, i in m.get("app", {}).items() for c in i["cases"]}


def app_totals(m):
    out = {}
    for p, i in m.get("app", {}).items():
        s = {"pass": 0, "fail": 0, "skip": 0, "noref": 0}
        for c in i["cases"]:
            s[c["status"]] = s.get(c["status"], 0) + 1
        out[p] = s
    return out


def lat_regress(cur, old):
    if appci_lat:
        v, d, p = appci_lat.regress(cur, old)
        return v, d, p
    if cur is None or old is None:
        return "none", None, None
    d = cur - old
    p = d / old * 100 if old else 0.0
    return ("fail" if d > 0.5 and p > 5 else "pass"), round(d, 2), round(p, 1)


def last_with(hist, test):
    for m in reversed(hist):
        if test(m):
            return m
    return None


def diff(hist, cur):
    """NEW / FIXED failures and latency regressions of cur versus the most recent EARLIER run that has the same data: per unit job and
    per app platform, not simply the previous run (a Mac-only reuse run must not 'fix' every Linux failure). hist is oldest-first."""
    d = {"new_unit": [], "fixed_unit": [], "new_app": [], "fixed_app": [], "latency": [], "baselines": []}
    fl = lambda j: {"%s %s" % (f["crate"], f["test"]) for f in j.get("failures", [])}
    for n, j in sorted(cur.get("unit", {}).items()):
        if j.get("run") is None:
            continue
        pm = last_with(hist, lambda m: m.get("unit", {}).get(n, {}).get("run") is not None)
        if not pm:
            continue
        pj = pm["unit"][n]
        d["baselines"].append("%s: %s (%s)" % (n, pm["ref"], pm["pipeline"]))
        d["new_unit"] += sorted("%s [%s]" % (x, n) for x in fl(j) - fl(pj))
        if j["run"] >= pj["run"]:                       # a narrower fix-tier filter cannot prove a failure fixed
            d["fixed_unit"] += sorted("%s [%s]" % (x, n) for x in fl(pj) - fl(j))
        elif fl(pj) - fl(j):
            d["baselines"].append("%s ran fewer tests (%d vs %d): missing failures not counted as fixed" % (n, j["run"], pj["run"]))
    for plat, info in sorted(cur.get("app", {}).items()):
        # a targeted run holds only the cases it was given: as a baseline it would make every other case "not in previous run"
        pm = last_with(hist, lambda m: plat in m.get("app", {}) and m["app"][plat]["cases"] and m.get("tier") != "targeted")
        if not pm or not info["cases"]:
            continue
        d["baselines"].append("app %s: %s (%s)" % (plat, pm["ref"], pm["pipeline"]))
        old = {(c["mode"], c["name"]): c for c in pm["app"][plat]["cases"]}
        for c in info["cases"]:
            k = (c["mode"], c["name"])
            o = old.get(k)
            lab = "%s/%s/%s" % (plat, k[0], k[1])
            if c["status"] == "fail" and (o is None or o["status"] != "fail"):
                d["new_app"].append(lab + ("" if o else " (case not in previous run)"))
            if c["status"] == "pass" and o and o["status"] == "fail":
                d["fixed_app"].append(lab)
            for lk, lr in c["lat"].items():
                ol = (o or {}).get("lat", {}).get(lk)
                if ol and lr.get("p50") is not None and ol.get("p50") is not None:
                    v, dm, dp = lat_regress(lr["p50"], ol["p50"])
                    if dm is not None and dm > 0.5 and (dp or 0) > 5:
                        d["latency"].append("%s %s: %.2f -> %.2f ms (+%.1f%%)" % (lab, lk, ol["p50"], lr["p50"], dp))
    return d


def fmt_app(m):
    t = app_totals(m)
    return "; ".join("%s %d/%d/%d%s" % (p, s["pass"], s["fail"], s["skip"], (" +%d no-ref" % s["noref"]) if s.get("noref") else "") for p, s in sorted(t.items())) or "-"


HEAD = ("key_to_frame", "enter", "ctrl_c")


def proving_names():
    """cases.PROVING, read from script/realapp/appci/cases.py as a literal (this collector imports no harness code)"""
    import ast
    try:
        src = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "realapp", "appci", "cases.py")).read()
        for node in ast.parse(src).body:
            if isinstance(node, ast.Assign) and any(getattr(t, "id", "") == "PROVING" for t in node.targets):
                return set(ast.literal_eval(node.value))
    except (OSError, SyntaxError, ValueError):
        pass
    return set()


def render_proving(m):
    """owner rule 2026-11-09: the PROVING cases with their status per platform and mode. Graduation is the OWNER's decision, never made by code."""
    pv = proving_names()
    rows = [(p, c) for p, i in sorted(m.get("app", {}).items()) for c in i["cases"] if c["name"] in pv]
    if not rows:
        return []
    L = ["", "## PROVING cases (owner decides graduation)", "", "| case | platform | mode | status |", "|---|---|---|---|"]
    L += ["| %s | %s | %s | %s |" % (c["name"], p, c["mode"], c["status"].upper()) for p, c in sorted(rows, key=lambda x: (x[1]["name"], x[0], x[1]["mode"]))]
    names = {c["name"] for _, c in rows}
    ok = sorted(n for n in names if all(c["status"] == "pass" for _, c in rows if c["name"] == n))
    L += ["", "passed everywhere it ran (%d, candidates for the owner's decision): %s" % (len(ok), ", ".join(ok) or "none"),
          "not passed somewhere (fail / skip / no-reference): %s" % (", ".join(sorted(names - set(ok))) or "none")]
    return L


def fmt_lat(m):
    """headline latency medians vs budget per platform, e.g. wayland key 3.1 (ovh 0.9/2 pass)"""
    parts = []
    for p, i in sorted(m.get("app", {}).items()):
        seen = []
        for c in i["cases"]:
            for lk, r in c["lat"].items():
                if r.get("p50") is not None and any(h in lk for h in HEAD) and len(seen) < 3:
                    b = "" if r.get("budget_ms") is None else (" ovh %s/%s %s" % (r.get("overhead_ms") if r.get("overhead_ms") is not None else "-", r["budget_ms"], r.get("verdict")))
                    seen.append("%s:%s %.1f ms%s" % (c["mode"][:1], lk.replace("_ms", ""), r["p50"], b))
        if seen:
            parts.append("%s %s" % (p, ", ".join(seen)))
    return "<br>".join(parts) or "-"


def soak_verdict(m):
    """one entry per platform with a soak row: that platform's worst status (fail, then noref, pass, skip). Round 15 (T49): the fix tier soaks macOS
    and Windows while Linux skips, and the first row found used to be the whole answer, so a Linux skip would have hidden the two that ran."""
    rank = {"fail": 3, "noref": 2, "pass": 1, "skip": 0}
    out = []
    for i in m.get("app", {}).values():
        st = [c["status"] for c in i["cases"] if "soak" in c["name"]]
        if st:
            out.append("%s (%s)" % (max(st, key=lambda x: rank.get(x, 0)), i["meta"].get("platform", "")))
    return ", ".join(out) or "n/a"


def compile_state(m):
    c = m.get("compile", {})
    if not c:
        return "-"
    return "%s, %d warnings" % ("ok" if c["status"] == "success" else "FAIL", c.get("warning_count", 0))


def render_diff(d):
    lines = []
    for title, key in (("NEW failures (unit)", "new_unit"), ("FIXED failures (unit)", "fixed_unit"), ("NEW failures (app)", "new_app"),
                       ("FIXED failures (app)", "fixed_app"), ("Latency regressions > 5%", "latency")):
        lines.append("- %s: %s" % (title, "none" if not d[key] else ""))
        lines += ["  - `%s`" % x for x in d[key][:40]]
        if len(d[key]) > 40:
            lines.append("  - ... and %d more" % (len(d[key]) - 40))
    lines += ["- compared against: " + "; ".join(d["baselines"])] if d["baselines"] else ["- nothing comparable in earlier runs"]
    return lines


def render_readme(m, hist):
    L = ["# %s  %s  (pipeline %s)" % (m["ref"], m["commit"][:9], m["pipeline"]), "",
         "- date: %s, tier: %s, pipeline status: %s%s" % (m.get("date", "?"), m.get("tier", "?"), m.get("pipeline_status", "unknown"), " (collected before it finished)" if m.get("pipeline_status") in ("running", "pending", "created") else ""),
         "- compile (check-linux): %s" % compile_state(m)]
    if m.get("tier") == "targeted":
        L.append("- **%s**" % TARGETED_NOTE)
        for k, e in sorted(((m.get("targeted") or {}).get("platforms") or {}).items()):
            names = e if isinstance(e, list) else (e or {}).get("cases") or []
            if names:
                L.append("  - asked for on %s: %s" % (k, ", ".join("`%s`" % n for n in names)))
    u = unit_totals(m)
    L.append("- unit: %d passed, %d failed, %d skipped of %d run (%d suites)" % (u["passed"], u["failed"], u["skipped"], u["run"], u["jobs"]))
    L.append("- app (pass/fail/skip): %s; soak: %s" % (fmt_app(m), soak_verdict(m)))
    L += ["", "## Jobs", "", "| job | status | minutes | runner | note |", "|---|---|---|---|---|"]
    for n, j in sorted(m["jobs"].items()):
        L.append("| %s | %s | %s | %s | %s |" % (n, j["status"], "-" if not j.get("duration") else "%.1f" % (j["duration"] / 60), j.get("runner") or "-", j.get("note", "")))
    for n, j in sorted(m.get("unit", {}).items()):
        if j.get("failures") or j.get("errors"):
            L += ["", "## %s failures" % n, ""]
            L += ["- `%s %s` %s (%.1fs)" % (f["crate"], f["test"], f["kind"], f["seconds"]) for f in j.get("failures", [])]
            L += ["- error: " + e for e in j.get("errors", [])]
        if j.get("skips"):                                    # round 15 (#2064): the usage suite's skipped scenarios, each with its reason
            L += ["", "## %s skipped scenarios (%d; not coverage)" % (n, len(j["skips"])), ""]
            L += ["- `%s %s`: %s" % (x["surface"], x["scenario"], x["reason"].replace("|", "/")) for x in j["skips"]]
    for n, j in sorted(m.get("setup", {}).items()):          # round 15 (#2063): lab-helper / sshlab-image when red, with what they said
        if j.get("failing_lines"):
            L += ["", "## %s (%s)" % (n, j.get("status")), ""] + ["- " + x for x in j["failing_lines"]]
    for p, i in sorted(m.get("app", {}).items()):
        L += ["", "## app %s (%s%s)" % (p, i["job"], ", REUSED build" if i["reuse"] else ""), ""]
        if i.get("reused_build"):
            L.append("reuse: `%s`" % json.dumps(i["reused_build"])[:300])
        L += ["", "| mode | case | status | detail |", "|---|---|---|---|"]
        for c in i["cases"]:
            L.append("| %s | %s | %s | %s |" % (c["mode"], c["name"], c["status"].upper(), "; ".join(c["detail"])[:140].replace("|", "/")))
    L += render_proving(m)
    for n in m.get("notes", []):
        L.append("- note: " + n)
    L += ["", "## vs previous run", ""]
    L += render_diff(diff(hist, m)) if hist else ["first run in the archive"]
    return "\n".join(L) + "\n"


def load_runs(out):
    runs = []
    for p in glob.glob(os.path.join(out, "*", "manifest.json")):
        try:
            m = json.load(open(p))
            m["_dir"] = os.path.dirname(p)
            # manifests from the older ssh-run archive have another shape (no jobs list); the
            # pipeline history covers only runs this collector wrote
            if "jobs" in m:
                runs.append(m)
        except ValueError:
            pass
    # older manifests (before created_at was recorded) sort by their date, then pipeline id
    return sorted(runs, key=lambda m: (m.get("created_at") or m.get("date") or "", m.get("pipeline") or 0))


def rebuild(out):
    runs = load_runs(out)
    rows = ["| date | ref | commit | tier | compile | unit pass/fail/skip | app pass/fail/skip | key latency (p50 vs budget) | soak |", "|---|---|---|---|---|---|---|---|---|"]
    sections = []
    for i, m in enumerate(runs):
        hist = runs[:i]
        prev = hist[-1] if hist else None
        open(os.path.join(m["_dir"], "README.md"), "w").write(render_readme(m, hist))
        u = unit_totals(m)
        rows.append("| %s | [%s](%s/README.md) | %s | %s | %s | %s | %s | %s | %s |" % (m["date"], m["ref"], os.path.basename(m["_dir"]), m["commit"][:9], m["tier"], compile_state(m),
                    "%d/%d/%d" % (u["passed"], u["failed"], u["skipped"]) if u["jobs"] else "-", fmt_app(m), fmt_lat(m), soak_verdict(m)))
        sections.append(["### %s (pipeline %s) vs earlier runs" % (m["ref"], m["pipeline"]), ""] +
                        (render_diff(diff(hist, m)) if prev else ["first run in the archive"]) + [""])
    text = ["# Test results index", "", "One row per collected pipeline, oldest first; generated by `script/ci/collect_results.py` (never edit). Layout and rules: `docs/testing.md` \"Test results history\".", ""]
    text += rows + ["", "## vs previous run", "", "NEW / FIXED are by test or case name; latency regressions use the appci rule (> 5 % and > 0.5 ms on the Phosphor median).", ""]
    for s in reversed(sections):
        text += s
    open(os.path.join(out, "INDEX.md"), "w").write("\n".join(text))


# ---- main ---------------------------------------------------------------------------------------------------------------------
def main():
    ap = argparse.ArgumentParser(description="archive one GitLab pipeline's test results under test-results/")
    ap.add_argument("pipeline", nargs="?", help="pipeline id, or a build/<ref> (its newest pipeline)")
    ap.add_argument("--out", default="test-results")
    ap.add_argument("--tier", choices=["fix", "candidate", "release", "targeted"], help="the tier that was played (ci_seq.py passes it); else the app runs' own meta.tier, else inferred")
    ap.add_argument("--cases-file", default="", help="a targeted run's cases file (ci_seq.py --tier targeted passes it): kept in the manifest as `targeted`, so the record says what was asked for")
    ap.add_argument("--self-test", action="store_true", help="check the pure parts (usage-suite parsing, tier inference) and exit")
    a = ap.parse_args()
    if a.self_test:
        return self_test()
    if not a.pipeline:
        ap.error("pipeline is required")
    pl = resolve(a.pipeline)
    pid = pl["id"]
    jobs = all_jobs(pid)
    by = {}
    for j in jobs:
        by.setdefault(j["name"], j)
    tier = a.tier or infer_tier({n: j["status"] for n, j in by.items()})       # refined below from the app runs' own meta.tier when --tier is not given
    date = pl["created_at"][:10]
    d = os.path.join(a.out, "%s_%s_%s" % (date, sanitize(pl["ref"]), pid))
    if os.path.isdir(d):
        shutil.rmtree(d)                                  # idempotent: this run's directory only
    os.makedirs(os.path.join(d, "unit"))
    man = {"pipeline": pid, "web_url": pl.get("web_url"), "ref": pl["ref"], "commit": pl["sha"], "created_at": pl["created_at"], "date": date, "tier": tier,
           "pipeline_status": pl["status"], "jobs": {}, "unit": {}, "app": {}, "notes": []}
    if tier == "targeted":
        man["notes"].append(TARGETED_NOTE)
        if a.cases_file:
            try:
                import ci_seq
                doc = ci_seq.load_targeted(a.cases_file)
                man["targeted"] = {k: doc[k] for k in ("platforms", "suites", "filters", "note") if k in doc}
            except (OSError, ValueError) as e:
                man["notes"].append("the targeted cases file could not be read (%s): what was asked for is not in this record" % str(e)[:160])
    for j in jobs:
        n = j["name"]
        ent = {"status": j["status"], "duration": j.get("duration"), "id": j["id"], "runner": (j.get("runner") or {}).get("description")}
        if j["status"] in ("manual", "created", "waiting_for_resource", "pending"):
            ent["note"] = "not run"
        man["jobs"][n] = ent
    for n, j in by.items():
        if j["status"] not in FIN and not (j["status"] == "canceled" and n.startswith(("app-test", "test-"))):
            continue
        base = re.sub(r": \[\d+\]$", "", n)
        try:
            if base in UNIT_JOBS or n.startswith("integration-linux"):
                u = parse_unit(trace(j["id"]))
                u.update(job=n, status=j["status"], duration=j.get("duration"))
                dump_scrubbed(u, os.path.join(d, "unit", sanitize(n) + ".json"))
                man["unit"][n] = {k: u[k] for k in u if k not in ("notes",)} if False else u
            elif n == "check-linux":
                c = parse_check(trace(j["id"]))
                c.update(status=j["status"], duration=j.get("duration"))
                man["compile"] = c
                dump_scrubbed(c, os.path.join(d, "unit", "check-linux.json"))
            elif base in NUMBERS_JOBS:
                x = parse_numbers(trace(j["id"]))
                x.update(job=n, status=j["status"], duration=j.get("duration"))
                dump_scrubbed(x, os.path.join(d, "unit", sanitize(n) + ".json"))
            elif n in USAGE_JOBS:
                u = parse_usage(trace(j["id"]))
                u.update(job=n, status=j["status"], duration=j.get("duration"))
                dump_scrubbed(u, os.path.join(d, "unit", sanitize(n) + ".json"))
                man["unit"][n] = u
            elif n in SETUP_JOBS:
                g = parse_guards(trace(j["id"]), j["status"])
                g.update(job=n, status=j["status"], duration=j.get("duration"))
                dump_scrubbed(g, os.path.join(d, "unit", sanitize(n) + ".json"))
                man.setdefault("setup", {})[n] = g
            elif n in GUARD_JOBS or n in ("check-macos", "check-windows"):
                g = parse_guards(trace(j["id"]), j["status"])
                g.update(job=n, status=j["status"], duration=j.get("duration"))
                dump_scrubbed(g, os.path.join(d, "unit", sanitize(n) + ".json"))
            elif n.startswith("app-test-"):
                got = collect_app(j, os.path.join(d, "app"), man["notes"])
                if not got:
                    man["jobs"][n]["note"] = "no results artifact (job %s)" % j["status"]
                man["app"].update(got)
        except Exception as e:                                # one broken job must not lose the rest of the run
            man["jobs"][n]["note"] = "collect error: %s" % str(e)[:120]
            man["notes"].append("%s: %s" % (n, str(e)[:200]))
    if not a.tier:
        man["tier"] = tier_from_app_meta(man["app"]) or tier
    size = dir_size(d)
    if size > WARN_BYTES:
        man["notes"].append("run dir is %.1f MB (> 5 MB soft limit)" % (size / 1e6))
        print("WARNING: %s is %.1f MB" % (d, size / 1e6), file=sys.stderr)
    man = json.loads(results_scrub.scrub(json.dumps(man)))
    json.dump(man, open(os.path.join(d, "manifest.json"), "w"), indent=1)
    rebuild(a.out)
    print("%s: %.2f MB" % (d, dir_size(d) / 1e6))
    if dir_size(d) > WARN_BYTES:
        print("WARNING: still over 5 MB", file=sys.stderr)
    leaks = [x for f in glob.glob(d + "/**/*", recursive=True) if os.path.isfile(f) and not f.endswith(".jpg") for x in results_scrub.find_leaks(open(f, errors="replace").read())]
    if leaks:
        print("WARNING: possible leaks remain: %s" % leaks[:5], file=sys.stderr)
        return 2
    return 0


def self_test():
    """`collect_results.py --self-test`: the pure parts added in round 15 (#2064). Exit 0 = all pass."""
    rows = []
    usage = [
        "$ ./script/usage-test --surface both",
        '{"surface":"gui","scenario":"typeahead_basic","status":"pass","duration_ms":1200,"tags":["core"]}',
        '{"surface":"gui","scenario":"ssh_login","status":"fail","duration_ms":30500,"tags":[],"retries":1,"failure_detail":"assertion failed: prompt seen"}',
        '{"surface":"tui","scenario":"usage_tui_open","status":"skip","tags":["needs-byop"],"reason":"needs --include-byop"}',
        '{"surface":"tui","scenario":"usage_tui_type","status":"pass","duration_ms":300,"tags":[]}',
        "not json {", '{"other": 1}',
        '{"summary":{"total":4,"passed":2,"failed":1,"skipped":1,"surfaces":{"gui":2,"tui":2}}}',
    ]
    u = parse_usage(usage)
    rows.append(((u["run"], u["passed"], u["failed"], u["skipped"]) == (3, 2, 1, 1) and u["surfaces"] == {"gui": 2, "tui": 2} and not u["notes"], "usage suite: rows counted, summary agrees"))
    rows.append((u["failures"] == [{"kind": "FAIL", "crate": "usage-gui", "test": "ssh_login", "seconds": 30.5, "detail": "assertion failed: prompt seen"}], "a failed scenario is a named failure (NEW / FIXED work on it)"))
    rows.append((u["skips"] == [{"surface": "tui", "scenario": "usage_tui_open", "reason": "needs --include-byop"}], "every skipped scenario is kept with its reason"))
    integ = parse_unit([
        "        PASS [   1.664s] (12/86) integration::integration shell_integration_tests::test_legacy_ssh_into_bash",
        "  TRY 3 FAIL [  22.493s] (30/86) integration::integration shell_integration_tests::test_ssh_with_shell_override",
        "     Summary [ 159.871s] 86 tests run: 84 passed, 2 failed, 232 skipped",
        "INTEG-SKIP test_legacy_ssh_into_bash NEEDS: FeatureFlag::SSHTmuxWrapper off",
        "INTEG-SKIP test_ssh_into_ash NEEDS: the local sshd fixture with remote shell `ash`: the remote shell `ash` is not installed",
        "INTEG-SKIP test_ssh_into_ash NEEDS: said twice",
    ])
    rows.append(((integ["run"], integ["passed"], integ["failed"], integ["skipped"]) == (84, 82, 2, 234) and [x["scenario"] for x in integ["skips"]] == ["test_legacy_ssh_into_bash", "test_ssh_into_ash"]
                 and integ["skips"][1]["reason"].startswith("the local sshd fixture") and "86 tests run: 84 passed" in integ["summary_line"],
                 "#2122: an integration scenario that skipped itself is taken out of passed and listed with what it needs; nextest's own line is kept"))
    md = render_readme({"ref": "build/x", "commit": "0" * 40, "pipeline": 1, "jobs": {}, "unit": {"integration-linux: [1]": integ}, "app": {}}, [])
    rows.append(("## integration-linux: [1] skipped scenarios (2; not coverage)" in md and "`integration test_legacy_ssh_into_bash`: FeatureFlag::SSHTmuxWrapper off" in md,
                 "#2122: the README lists the integration scenarios that did not run"))
    rows.append(("skips" not in parse_unit(["     Summary [ 1.0s] 3 tests run: 3 passed, 0 skipped"]), "a suite with no self-skip has no skips entry"))
    cut = parse_usage(usage[:3])
    rows.append((cut["run"] == 2 and cut["failed"] == 1 and any("did not finish" in n for n in cut["notes"]), "no summary line: the rows seen are counted and the note says the suite did not finish"))
    lied = parse_usage(usage[:2] + ['{"summary":{"total":4,"passed":4,"failed":0,"skipped":0}}'])
    rows.append((lied["passed"] == 1 and lied["failed"] == 0 and any("differs from its rows" in n for n in lied["notes"]), "a summary that disagrees with the rows is noted; the rows are recorded"))
    dead = parse_usage(["error: could not compile `usage_suite`", "ERROR xvfb-run is missing on this runner"])
    rows.append((dead["run"] is None and len(dead["errors"]) == 2, "no row at all: not a result (run None), the error lines are kept"))
    m = {"unit": {"usage-test-linux": u}}
    rows.append((unit_totals(m) == {"passed": 2, "failed": 1, "skipped": 1, "run": 3, "jobs": 1} and unit_failure_set(m) == {"usage-gui ssh_login"}, "the usage suite counts in the unit totals and the failure set"))
    md = render_readme({"ref": "build/x", "commit": "0" * 40, "pipeline": 1, "jobs": {}, "unit": {"usage-test-linux": u},
                        "setup": {"lab-helper": {"status": "failed", "failing_lines": ["lab_helper: ERROR zig is missing for this user"]}}, "app": {}}, [])
    rows.append(("## usage-test-linux failures" in md and "`usage-gui ssh_login` FAIL" in md and "## usage-test-linux skipped scenarios (1; not coverage)" in md
                 and "`tui usage_tui_open`: needs --include-byop" in md and "## lab-helper (failed)" in md and "zig is missing" in md, "the README shows the failure, the skip with its reason, and a red lab-helper's own words"))
    units = {"test-linux": "success", "test-warpui-core": "success", "test-workspace-rest": "failed"}
    rows.append((infer_tier(dict(units, **{"app-test-linux-x11": "success"})) == "fix", "all three unit suites and a long x11 run is still the fix tier (#2058: the fix tier plays x11)"))
    rows.append((infer_tier(dict(units, **{"integration-linux: [2]": "failed"})) == "candidate" and infer_tier(dict(units, **{"integration-linux: [1]": "manual"})) == "fix", "the integration suite having run is what marks a candidate run"))
    rows.append((infer_tier(dict(units, **{"mutants-macos": "success"})) == "release" and infer_tier({}) == "fix", "a release-only suite marks a release run; nothing ran = fix"))
    rows.append(([tier_from_app_meta({"wayland": {"meta": {"tier": t}}}) for t in ("smoke", "full", "release", "")] == ["fix", "candidate", "release", None]
                 and tier_from_app_meta({"a": {"meta": {"tier": "smoke"}}, "b": {"meta": {"tier": "full"}}}) == "candidate" and tier_from_app_meta({}) is None,
                 "the app runs' own meta.tier names the tier"))
    rows.append((tier_from_app_meta({"wayland": {"meta": {"tier": "targeted"}}}) is None, "a targeted app run does not name fix / candidate / release (ci_seq passes --tier targeted)"))
    tc = lambda st: [{"mode": "terminal", "name": "a", "status": st, "lat": {}}]
    th = [{"tier": "candidate", "ref": "c", "pipeline": 1, "app": {"wayland": {"cases": tc("fail") + [{"mode": "terminal", "name": "b", "status": "pass", "lat": {}}]}}},
          {"tier": "targeted", "ref": "t", "pipeline": 2, "app": {"wayland": {"cases": tc("pass")}}}]
    td = diff(th, {"tier": "candidate", "app": {"wayland": {"cases": tc("pass") + [{"mode": "terminal", "name": "b", "status": "fail", "lat": {}}]}}})
    rows.append((td["fixed_app"] == ["wayland/terminal/a"] and td["new_app"] == ["wayland/terminal/b"], "a targeted run is never the baseline of a later run's app diff (it holds only its own cases)"))
    rows.append((diff(th[:1], {"tier": "targeted", "app": {"wayland": {"cases": tc("pass")}}})["fixed_app"] == ["wayland/terminal/a"], "a targeted run is compared with the last full run"))
    rows.append((all(x in render_readme({"ref": "r", "commit": "c" * 9, "pipeline": 1, "tier": "targeted", "jobs": {}, "app": {}, "unit": {},
                                                                 "targeted": {"platforms": {"macos": ["a"]}}}, []) for x in ("TARGETED run", "asked for on macos: `a`")), "a targeted run's README says so, with what was asked for"))
    rows.append((tier_from_app_meta({"lab-wayland": {"meta": {"tier": "full"}}, "wayland": {"meta": {"tier": "smoke"}}}) == "fix" and tier_from_app_meta({"replay-wayland": {"meta": {"tier": "full"}}}) is None,
                 "a lab or replay row does not name the tier"))
    rows.append((all(n in USAGE_JOBS for n in ("usage-test-linux", "usage-test-macos")) and "lab-helper" in SETUP_JOBS, "the job lists"))
    bad = 0
    for ok, what in rows:
        bad += 0 if ok else 1
        print("%s %s" % ("ok  " if ok else "FAIL", what))
    print("self-test: %s" % ("FAILED (%d)" % bad if bad else "all passed"))
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
