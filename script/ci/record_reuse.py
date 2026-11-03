#!/usr/bin/env python3
# script/ci/record_reuse.py: after_script of app-test-*-reuse jobs. Stamps which build was tested into every
# appci-out/*/run/results.json ("reused_build") and the top of summary.md. Best effort; never fails the job.
import glob, json, os, sys
try:
    meta = json.load(open("appci-out/reused_build.json"))["reused_build"]
except Exception as e:
    print("record_reuse: no reused_build.json (%s)" % e); sys.exit(0)
line = "**Tested a REUSED build**: job %s (%s) of pipeline %s, commit %s; this pipeline %s, commit %s (app-identical)." % (
    meta["job_id"], meta["job_name"], meta["pipeline_id"], meta["commit"], meta["this_pipeline"], meta["this_commit"])
sums = meta.get("dist_sha256") or {}
if sums:  # #1501: the sha256 of every file tested (dist/SHA256SUMS, verified by fetch_build.sh)
    line += " Files tested (sha256): " + ", ".join("%s %s" % (name, h[:16]) for name, h in sorted(sums.items())) + "."
for f in glob.glob("appci-out/*/run/results.json"):
    try:
        d = json.load(open(f))
        if isinstance(d, dict):
            d["reused_build"] = meta
            json.dump(d, open(f, "w"), indent=1)
    except Exception as e:
        print("record_reuse: %s: %s" % (f, e))
for f in glob.glob("appci-out/*/run/summary.md"):
    try:
        s = open(f).read(); open(f, "w").write(line + "\n\n" + s)
    except Exception as e:
        print("record_reuse: %s: %s" % (f, e))
print(line)
