#!/usr/bin/env python3
# appci/report.py: turns results.jsonl (one JSON object per case, appended as cases finish) into
# results.json, junit.xml (GitLab test report) and summary.md. Exit 1 when any case failed or none was judged (#1508).
# usage: report.py RUN_DIR [--meta key=value ...]
import json, sys, os, time
from xml.sax.saxutils import escape, quoteattr

run = sys.argv[1]
meta = dict(a.split("=", 1) for a in sys.argv[2:] if "=" in a)
rows = []
p = os.path.join(run, "results.jsonl")
if os.path.exists(p):
    for l in open(p):
        l = l.strip()
        if l:
            try:
                rows.append(json.loads(l))
            except ValueError:
                pass
cnt = {"pass": 0, "fail": 0, "skip": 0, "noref": 0}   # noref = a latency case with no reference terminal: NOT a pass, not a failure (#1499)
for r in rows:
    cnt[r["status"]] = cnt.get(r["status"], 0) + 1
doc = {"meta": meta, "generated": time.strftime("%Y-%m-%dT%H:%M:%S%z"), "summary": cnt, "cases": rows}
json.dump(doc, open(os.path.join(run, "results.json"), "w"), indent=1)
x = ['<?xml version="1.0" encoding="UTF-8"?>',
     '<testsuite name=%s tests="%d" failures="%d" skipped="%d">' % (quoteattr("appci-" + meta.get("platform", "?")), len(rows), cnt["fail"], cnt["skip"] + cnt["noref"])]
for r in rows:
    x.append('<testcase classname=%s name=%s time="%.2f">' % (quoteattr(r.get("mode", "")), quoteattr(r["name"]), r.get("seconds", 0)))
    d = escape("\n".join(r.get("detail", [])))
    if r["status"] == "fail":
        x.append("<failure message=%s>%s</failure>" % (quoteattr((r.get("detail") or ["failed"])[0][:200]), d))
    elif r["status"] == "noref":
        x.append("<skipped message=%s/>" % quoteattr("NO-REFERENCE (not a pass): " + (r.get("detail") or [""])[0][:180]))
    elif r["status"] == "skip":
        x.append("<skipped message=%s/>" % quoteattr((r.get("detail") or [""])[0][:200]))
    x.append("</testcase>")
x.append("</testsuite>")
open(os.path.join(run, "junit.xml"), "w").write("\n".join(x))
m = ["# App test results (%s)" % ", ".join("%s=%s" % kv for kv in meta.items()), "", "pass %(pass)d, fail %(fail)d, skip %(skip)d, no-reference %(noref)d (not passes)" % cnt, "",
     "| mode | case | status | detail |", "|---|---|---|---|"]
for r in rows:
    m.append("| %s | %s | %s | %s |" % (r.get("mode", ""), r["name"], r["status"].upper(), "; ".join(r.get("detail", []))[:160].replace("|", "/")))
def f(x, nd=1):
    return "-" if x is None else ("%.*f" % (nd, x))


def cell(x):
    return "-" if not x or x.get("p50") is None else "%s / %s" % (f(x["p50"]), f(x.get("p95")))


def dcell(x):
    if not x or x.get("delta_ms") is None:
        return (x or {}).get("status", "-") if x and x.get("status") in ("pending", "missing") else "-"
    return "%s (%+.2f ms, %+.1f%%) %s" % (f(x.get("p50")), x["delta_ms"], x["delta_pct"], x["verdict"].upper())


lat_rows = []
for r in rows:
    for k, rec in ((r.get("metrics") or {}).get("lat") or {}).items():
        b = "-"
        if rec.get("budget_kind") == "overhead":
            b = "overhead <= %g ms" % rec["budget_ms"]
        elif rec.get("budget_kind") == "absolute":
            b = "<= %g ms over floor" % rec["budget_ms"]
        elif rec.get("budget_kind") == "slow-log":
            b = "<= 4 ms (warn 16)"
        ref = rec.get("reference")
        lat_rows.append("| %s | %s/%s | %s | %s%s | %s | %s | %s | %s | %s | %s |" % (
            r.get("mode", ""), r["name"], k, cell(rec.get("phosphor")) if rec.get("budget_kind") != "slow-log" else "worst %s ms" % f(rec["phosphor"].get("max"), 0),
            cell(ref), (" (%s)" % rec["reference_name"]) if ref and rec.get("reference_name") else "",
            f(rec.get("overhead_ms"), 2), b, dcell(rec.get("prev")), dcell(rec.get("baseline")), rec["verdict"].upper(),
            "n=%s" % (rec["phosphor"].get("n"))))
        if rec.get("remeasure"):
            lat_rows.append("| | re-measure | %s | | | | | | | |" % cell(rec["remeasure"]["phosphor"]))
if lat_rows:
    m += ["", "## Latency (%s): median / p95 ms, warm-up discarded" % meta.get("platform", "?"), "",
          "| mode | case/measure | Phosphor | reference | overhead ms | budget | previous build | baseline | verdict | n |",
          "|---|---|---|---|---|---|---|---|---|---|"] + lat_rows
kl = [(r.get("mode", ""), (r.get("metrics") or {}).get("key_loss")) for r in rows if (r.get("metrics") or {}).get("key_loss")]
if kl:
    m += ["", "## Key-loss control (#1414): lines whose first character never arrived / lines typed", "",
          "| mode | target | injection | lost-first | other-bad | missing | n |", "|---|---|---|---|---|---|---|"]
    for md, d in kl:
        for v in d.get("variants", []):
            m.append("| %s | %s | %s | %s | %s | %s | %s |" % (md, v["target"], v["inject"], v["lost_first"], v["other_bad"], v["missing"], v["n"]))
ec = [(r.get("mode", ""), (r.get("metrics") or {}).get("enter_control")) for r in rows if (r.get("metrics") or {}).get("enter_control")]
if ec:
    m += ["", "## Enter-loss control (#1482): reference-terminal lines missing after a fast burst with instant Enter", "",
          "| mode | reference | gap s | missing | n |", "|---|---|---|---|---|"]
    for md, d in ec:
        for g, v in d.get("runs", {}).items():
            m.append("| %s | %s | %s | %s | %s |" % (md, d.get("reference"), g, v["bad"], v["n"]))
open(os.path.join(run, "summary.md"), "w").write("\n".join(m) + "\n")
print("\n".join(m))
# #1508: a run that judged nothing is not a pass: zero rows, or nothing but skips, exits 1
empty = not rows or not (cnt["pass"] or cnt["noref"] or cnt["fail"])
if empty:
    print("RUN JUDGED NOTHING: %d rows, no pass/fail/no-reference row (a misspelled --cases, an all-skip selection or a crash before the first case)" % len(rows))
sys.exit(1 if (cnt["fail"] or empty) else 0)
