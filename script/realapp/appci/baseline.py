#!/usr/bin/env python3
# appci/baseline.py: committed latency baselines (baselines/<platform>.json) and run-to-run deltas.
#   baseline.py check RESULTS.json              list why a run is not promotable (exit 1) or print 'eligible' (#1513)
#   baseline.py promote RESULTS.json PLATFORM [--force]  refuses an ineligible run unless --force.   write baselines/PLATFORM.json from a run (the coordinator does this from a run on the
#                                               build the owner judged good, then commits the file); PLATFORM = linux-wayland | linux-x11 | macos | windows
#   baseline.py compare A B                     print a delta table; A/B are results.json or baseline files (A = old, B = new)
import json, os, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lat  # noqa: E402


def load(path):
    d = json.load(open(path))
    if "cases" in d:
        return {k: {"p50": v["phosphor"]["p50"], "p95": v["phosphor"]["p95"], "n": v["phosphor"]["n"], "overhead_ms": v.get("overhead_ms")}
                for k, v in lat.flat_lat(d).items() if (v.get("phosphor") or {}).get("p50") is not None}
    return d.get("metrics", {})


def eligibility(doc):
    """#1513: why a run must NOT become a baseline -> list of reasons ([] = eligible). The accepted run is the first FULL-tier run per platform with
    no FAIL anywhere in the latency verdicts, every latency record at n >= lat.min_n(), no INCONCLUSIVE/NO-COMPARE/NO-REFERENCE budget verdict and no failed case.
    The owner still reviews the numbers (summary.md) before the coordinator promotes and commits."""
    why = []
    meta = doc.get("meta", {})
    if str(meta.get("tier", "unknown")).lower() not in ("full", "release"):
        why.append("tier=%s (only a run recorded as full-tier is promotable)" % meta.get("tier"))
    if str(meta.get("scale", "1")) not in ("1", "1.0"):
        why.append("scale=%s (a scaled run is not promotable)" % meta.get("scale"))
    cases = doc.get("cases", [])
    if (doc.get("summary") or {}).get("fail"):
        why.append("%d case(s) failed" % doc["summary"]["fail"])
    recs = lat.flat_lat(doc)
    for k, v in sorted(recs.items()):
        n = (v.get("phosphor") or {}).get("n", 0)
        if n < lat.min_n():
            why.append("%s: n=%s < %d" % (k, n, lat.min_n()))
        for f in ("verdict", "budget_verdict"):
            if v.get(f) in ("fail", "inconclusive", "no-compare", "no-reference"):
                why.append("%s: %s=%s" % (k, f, v[f].upper()))
    if not recs:
        why.append("no latency records")
    return why


def promote(res, platform, force=False):
    d = json.load(open(res))
    m = load(res)
    if not m:
        sys.exit("no latency records in %s" % res)
    why = eligibility(d)
    if why and not force:
        sys.exit("REFUSED: %s is not promotable (#1513):\n  %s\n(--force overrides; the owner has reviewed and accepted the numbers)" % (res, "\n  ".join(why[:20])))
    low = [k for k, v in m.items() if v["n"] < lat.min_n()]
    out = {"status": "ok", "platform": platform, "source": "promoted %s from %s commit %s" % (time.strftime("%Y-%m-%d"), os.path.basename(res), d.get("meta", {}).get("commit", "?")),
           "metrics": m}
    p = lat.baseline_path(platform)
    json.dump(out, open(p, "w"), indent=1, sort_keys=True)
    print("wrote %s (%d measurements)%s" % (p, len(m), ("; WARNING n < %d for: %s" % (lat.min_n(), ", ".join(low))) if low else ""))


def compare(a, b):
    A, B = load(a), load(b)
    print("| measurement | old median/p95 | new median/p95 | delta ms | delta % | verdict |\n|---|---|---|---|---|---|")
    for k in sorted(set(A) | set(B)):
        o, n = A.get(k), B.get(k)
        if o and n:
            v, dm, dp = lat.regress(n["p50"], o["p50"])
            print("| %s | %s / %s | %s / %s | %+.2f | %+.1f | %s |" % (k, o["p50"], o["p95"], n["p50"], n["p95"], dm, dp, v.upper()))
        else:
            print("| %s | %s | %s | - | - | %s |" % (k, "%s / %s" % (o["p50"], o["p95"]) if o else "-", "%s / %s" % (n["p50"], n["p95"]) if n else "-", "only in " + ("old" if o else "new")))


if __name__ == "__main__":
    if len(sys.argv) in (4, 5) and sys.argv[1] == "promote":
        promote(sys.argv[2], sys.argv[3], force="--force" in sys.argv[4:])
    elif len(sys.argv) == 3 and sys.argv[1] == "check":
        w = eligibility(json.load(open(sys.argv[2])))
        print("\n".join(w) if w else "eligible")
        sys.exit(1 if w else 0)
    elif len(sys.argv) == 4 and sys.argv[1] == "compare":
        compare(sys.argv[2], sys.argv[3])
    else:
        sys.exit(__doc__ if __doc__ else open(__file__).read().split("\n\n")[0])
