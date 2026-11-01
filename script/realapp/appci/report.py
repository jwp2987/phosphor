#!/usr/bin/env python3
# appci/report.py: turns results.jsonl (one JSON object per case, appended as cases finish) into
# results.json, junit.xml (GitLab test report) and summary.md. Exit 1 when any case failed.
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
cnt = {"pass": 0, "fail": 0, "skip": 0}
for r in rows:
    cnt[r["status"]] = cnt.get(r["status"], 0) + 1
doc = {"meta": meta, "generated": time.strftime("%Y-%m-%dT%H:%M:%S%z"), "summary": cnt, "cases": rows}
json.dump(doc, open(os.path.join(run, "results.json"), "w"), indent=1)
x = ['<?xml version="1.0" encoding="UTF-8"?>',
     '<testsuite name=%s tests="%d" failures="%d" skipped="%d">' % (quoteattr("appci-" + meta.get("platform", "?")), len(rows), cnt["fail"], cnt["skip"])]
for r in rows:
    x.append('<testcase classname=%s name=%s time="%.2f">' % (quoteattr(r.get("mode", "")), quoteattr(r["name"]), r.get("seconds", 0)))
    d = escape("\n".join(r.get("detail", [])))
    if r["status"] == "fail":
        x.append("<failure message=%s>%s</failure>" % (quoteattr((r.get("detail") or ["failed"])[0][:200]), d))
    elif r["status"] == "skip":
        x.append("<skipped message=%s/>" % quoteattr((r.get("detail") or [""])[0][:200]))
    x.append("</testcase>")
x.append("</testsuite>")
open(os.path.join(run, "junit.xml"), "w").write("\n".join(x))
m = ["# App test results (%s)" % ", ".join("%s=%s" % kv for kv in meta.items()), "", "pass %(pass)d, fail %(fail)d, skip %(skip)d" % cnt, "",
     "| mode | case | status | detail |", "|---|---|---|---|"]
for r in rows:
    m.append("| %s | %s | %s | %s |" % (r.get("mode", ""), r["name"], r["status"].upper(), "; ".join(r.get("detail", []))[:160].replace("|", "/")))
open(os.path.join(run, "summary.md"), "w").write("\n".join(m) + "\n")
print("\n".join(m))
sys.exit(1 if cnt["fail"] else 0)
