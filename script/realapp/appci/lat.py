# appci/lat.py: latency measurement policy shared by the cases, report.py and baseline.py.
#   * a "latency record" is metrics["lat"][key] = {phosphor, reference, overhead_ms, budget_ms, budget_kind, prev, baseline, remeasure, verdict}
#   * budget verdicts (owner budgets, docs/testing.md "Latency budgets"): with a reference terminal measured the same way in the same
#     session the verdict is PASS/FAIL on OVERHEAD (Phosphor median - reference median); without one it is "no-reference" (never pass);
#     budget_kind "absolute" (Ctrl-C <= 20 ms over the harness floor, UI work) is judged on the number itself.
#   * regression verdicts compare the Phosphor median against the previous successful run and against baselines/<platform>.json:
#     FAIL when worse by > APPCI_REGRESS_PCT (5) % AND > APPCI_REGRESS_MS (0.5) ms; WARN when worse by 2..5 % AND > 0.5 ms.
#     A suspected regression is re-measured once; a failure is never dismissed (both measurements stay in the record), a clean
#     re-measure downgrades it to WARN, not pass.
import json, os, statistics

RANK = {"pass": 0, "no-reference": 1, "pending": 1, "none": 0, "warn": 2, "fail": 3}


def pct(xs, p):
    xs = sorted(xs)
    return xs[min(len(xs) - 1, int(len(xs) * p))] if xs else None


def summarize(samples):
    xs = list(samples)
    if not xs:
        return {"n": 0, "p50": None, "p95": None, "max": None}
    return {"n": len(xs), "p50": round(statistics.median(xs), 2), "p95": round(pct(xs, 0.95), 2), "max": round(max(xs), 2)}


def warmup():
    return int(os.environ.get("APPCI_WARMUP", "3"))


def min_n():
    return int(os.environ.get("APPCI_MIN_SAMPLES", "30"))


def thresholds():
    return float(os.environ.get("APPCI_REGRESS_PCT", "5")), float(os.environ.get("APPCI_REGRESS_MS", "0.5")), float(os.environ.get("APPCI_WARN_PCT", "2"))


def regress(cur, old):
    """cur/old medians in ms -> (verdict, delta_ms, delta_pct)"""
    if cur is None or old is None:
        return "none", None, None
    d = cur - old
    p = d / old * 100 if old else 0.0
    fail_pct, ms, warn_pct = thresholds()
    if d > ms and p > fail_pct:
        return "fail", round(d, 2), round(p, 1)
    if d > ms and p > warn_pct:
        return "warn", round(d, 2), round(p, 1)
    return "pass", round(d, 2), round(p, 1)


def worst(*vs):
    return max(vs, key=lambda v: RANK.get(v, 0))


def load_json(path):
    try:
        return json.load(open(path))
    except Exception:
        return None


def flat_lat(doc):
    """results.json document -> {"mode/case/key": latency record}"""
    out = {}
    for c in (doc or {}).get("cases", []):
        for k, rec in ((c.get("metrics") or {}).get("lat") or {}).items():
            out["%s/%s/%s" % (c.get("mode", ""), c["name"], k)] = rec
    return out


def baseline_path(platform):
    return os.path.join(os.path.dirname(os.path.abspath(__file__)), "baselines", platform + ".json")


def load_baseline(platform):
    """-> (status, {id: {p50,p95,n}}); status 'pending' (seeded, not filled) or 'ok' or 'missing'"""
    d = load_json(baseline_path(platform))
    if not d:
        return "missing", {}
    if d.get("status") != "ok":
        return "pending", {}
    return "ok", d.get("metrics", {})


def load_prev():
    d = load_json(os.environ.get("APPCI_PREV_RESULTS", ""))
    return flat_lat(d) if d else {}


def evaluate(rid, phos, ref, budget_ms, kind, platform, remeasure=None, ref_name=None, floor=None, notes=None):
    """phos/ref: sample lists (warm-up already dropped). remeasure: callable returning a fresh phos sample list, or None.
    Returns the latency record (see header)."""
    ps = summarize(phos)
    rec = {"phosphor": ps, "reference": summarize(ref) if ref else None, "reference_name": ref_name if ref else None,
           "budget_ms": budget_ms, "budget_kind": kind, "floor_ms": floor, "notes": list(notes or [])}
    if ps["n"] < min_n():
        rec["notes"].append("n=%d < %d samples (smoke/scaled run): numbers are indicative only" % (ps["n"], min_n()))
    bv = "pass"
    if kind == "overhead":
        if ref and ps["p50"] is not None:
            rec["overhead_ms"] = round(ps["p50"] - statistics.median(ref), 2)
            bv = "pass" if rec["overhead_ms"] <= budget_ms else "fail"
        else:
            rec["overhead_ms"] = None
            bv = "no-reference"
    elif kind == "absolute":
        base = ps["p50"]
        rec["overhead_ms"] = round(base - (floor or 0.0), 2) if base is not None else None
        bv = "pass" if rec["overhead_ms"] is not None and rec["overhead_ms"] <= budget_ms else "fail"
    rec["budget_verdict"] = bv
    # regression against previous build and the committed baseline
    prev = load_prev().get(rid)
    bst, bmap = load_baseline(platform)
    base = bmap.get(rid)
    rec["prev"] = None
    rec["baseline"] = {"status": bst}
    pv, bsv = "none", ("pending" if bst == "pending" else "none")
    if prev and (prev.get("phosphor") or {}).get("p50") is not None:
        pv, dm, dp = regress(ps["p50"], prev["phosphor"]["p50"])
        rec["prev"] = {"p50": prev["phosphor"]["p50"], "p95": prev["phosphor"].get("p95"), "delta_ms": dm, "delta_pct": dp, "verdict": pv}
    if base and base.get("p50") is not None:
        bsv, dm, dp = regress(ps["p50"], base["p50"])
        rec["baseline"] = {"status": "ok", "p50": base["p50"], "p95": base.get("p95"), "delta_ms": dm, "delta_pct": dp, "verdict": bsv}
    elif bst == "ok":
        rec["baseline"] = {"status": "ok", "verdict": "none"}
    if remeasure and "fail" in (pv, bsv):
        again = summarize(remeasure())
        rec["remeasure"] = {"phosphor": again}
        for tag, src in (("prev", prev), ("baseline", base)):
            ov = (src or {}).get("phosphor", src or {}) if tag == "prev" else (src or {})
            old = (ov or {}).get("p50")
            v2, dm, dp = regress(again["p50"], old)
            rec["remeasure"][tag] = {"verdict": v2, "delta_ms": dm, "delta_pct": dp}
        # still regressed on the second measurement -> stays FAIL; clean second measurement -> WARN (never pass, never dropped)
        if pv == "fail":
            pv = "fail" if rec["remeasure"]["prev"]["verdict"] in ("fail", "warn") else "warn"
        if bsv == "fail":
            bsv = "fail" if rec["remeasure"]["baseline"]["verdict"] in ("fail", "warn") else "warn"
        if rec["prev"]:
            rec["prev"]["verdict"] = pv
        if rec["baseline"].get("p50") is not None:
            rec["baseline"]["verdict"] = bsv
    rec["prev_verdict"], rec["baseline_verdict"] = pv, bsv
    v = worst(bv, pv, bsv if bsv != "pending" else "pass")
    rec["verdict"] = v
    return rec
