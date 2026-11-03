#!/usr/bin/env python3
# script/ci/collect_results.py: archive one GitLab pipeline's test results under test-results/ (owner rule 2026-11-02: "every test
# result is kept in the repo for regression review"). The coordinator commits the output; CI never pushes.
#
#   python3 script/ci/collect_results.py <pipeline_id | build/ref | ref> [--out test-results] [--tier fix|candidate|release]
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
import glapi                                    # every GitLab call: honours Retry-After / RateLimit-*
try:
    import lat as appci_lat                       # the regression rule shared with the harness (5 % and 0.5 ms)
except Exception:
    appci_lat = None

PROJECT = "88085499"
WARN_BYTES = 5 * 1024 * 1024
UNIT_JOBS = ("test-linux", "test-warpui-core", "test-workspace-rest", "integration-linux", "test-shuttle-loom")
NUMBERS_JOBS = ("bench", "fuzz-smoke", "mutants", "mutants-warpui-core")
GUARD_JOBS = ("guards", "advisories", "licenses")
RELEASE_JOBS = ("test-shuttle-loom", "fuzz-smoke", "bench", "mutants", "mutants-warpui-core")
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
STATUS = re.compile(r"^\s*(FAIL|TIMEOUT|SIGSEGV|SIGABRT|SIGKILL|SIGBUS|SIGILL|ABORT|LEAK|EXEC-FAIL|FLAKY)\s+(?:\d+/\d+\s+)?\[\s*([\d.]+)s\]\s+(?:\(\s*\d+/\d+\)\s+)?(\S+)\s+(\S.*)$")


def parse_unit(lines):
    d = {"summary_line": None, "duration_s": None, "run": None, "passed": 0, "failed": 0, "timed_out": 0, "skipped": 0, "slow": 0, "flaky": 0,
         "failures": [], "regressions": [], "notes": []}
    seen = {}
    for l in lines:
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
    if d["run"] is None:                                     # compile error / early abort: keep the cause
        d["errors"] = [l.strip()[:200] for l in lines if re.match(r"\s*(error(\[E\d+\])?:|ERROR)", l)][:8]
    return d


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
        pd = os.path.join(dest_root, plat)
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
            keep.append((n, rel))
        keep += [("appci-out/reused_build.json", "reused_build.json")] if "appci-out/reused_build.json" in names and plat == plats[0] else []
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
        out[plat] = info
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
        pm = last_with(hist, lambda m: plat in m.get("app", {}) and m["app"][plat]["cases"])
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
    for i in m.get("app", {}).values():
        for c in i["cases"]:
            if "soak" in c["name"]:
                return "%s (%s)" % (c["status"], i["meta"].get("platform", ""))
    return "n/a"


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
         "- date: %s, tier: %s, pipeline status: %s%s" % (m["date"], m["tier"], m["pipeline_status"], " (collected before it finished)" if m["pipeline_status"] in ("running", "pending", "created") else ""),
         "- compile (check-linux): %s" % compile_state(m)]
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
    for p, i in sorted(m.get("app", {}).items()):
        L += ["", "## app %s (%s%s)" % (p, i["job"], ", REUSED build" if i["reuse"] else ""), ""]
        if i.get("reused_build"):
            L.append("reuse: `%s`" % json.dumps(i["reused_build"])[:300])
        L += ["", "| mode | case | status | detail |", "|---|---|---|---|"]
        for c in i["cases"]:
            L.append("| %s | %s | %s | %s |" % (c["mode"], c["name"], c["status"].upper(), "; ".join(c["detail"])[:140].replace("|", "/")))
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
            runs.append(m)
        except ValueError:
            pass
    return sorted(runs, key=lambda m: (m["created_at"], m["pipeline"]))


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
    ap.add_argument("pipeline", help="pipeline id, or a build/<ref> (its newest pipeline)")
    ap.add_argument("--out", default="test-results")
    ap.add_argument("--tier", choices=["fix", "candidate", "release"], help="override the inferred tier")
    a = ap.parse_args()
    pl = resolve(a.pipeline)
    pid = pl["id"]
    jobs = all_jobs(pid)
    by = {}
    for j in jobs:
        by.setdefault(j["name"], j)
    ran = lambda n: n in by and by[n]["status"] not in ("manual", "created", "skipped", "waiting_for_resource", "pending")
    tier = a.tier or ("release" if any(ran(n) for n in RELEASE_JOBS) else "candidate" if all(ran(n) and by[n]["status"] != "skipped" for n in ("test-linux", "test-warpui-core", "test-workspace-rest")) and ran("app-test-linux-x11") and by["app-test-linux-x11"].get("duration", 0) > 120 else "fix")
    date = pl["created_at"][:10]
    d = os.path.join(a.out, "%s_%s_%s" % (date, sanitize(pl["ref"]), pid))
    if os.path.isdir(d):
        shutil.rmtree(d)                                  # idempotent: this run's directory only
    os.makedirs(os.path.join(d, "unit"))
    man = {"pipeline": pid, "web_url": pl.get("web_url"), "ref": pl["ref"], "commit": pl["sha"], "created_at": pl["created_at"], "date": date, "tier": tier,
           "pipeline_status": pl["status"], "jobs": {}, "unit": {}, "app": {}, "notes": []}
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


if __name__ == "__main__":
    sys.exit(main())
