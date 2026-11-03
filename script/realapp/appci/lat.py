# appci/lat.py: latency measurement policy shared by the cases, report.py and baseline.py.
#   * a "latency record" is metrics["lat"][key] = {phosphor, reference, overhead_ms, budget_ms, budget_kind, prev, baseline, remeasure, verdict}
#   * budget verdicts (owner budgets, docs/testing.md "Latency budgets"): with a reference terminal measured the same way in the same
#     session the verdict is PASS/FAIL on OVERHEAD (Phosphor median - reference median); without one it is "no-reference" (never pass);
#     budget_kind "absolute" (Ctrl-C <= 20 ms over the harness floor, UI work) is judged on the number itself.
#   * regression verdicts compare the Phosphor median against the previous finished run (success or failed, #1513) and against baselines/<platform>.json:
#     FAIL when worse by > APPCI_REGRESS_PCT (5) % AND > APPCI_REGRESS_MS (0.5) ms; WARN when worse by 2..5 % AND > 0.5 ms.
#     A suspected regression is re-measured once; a failure is never dismissed (both measurements stay in the record), a clean
#     re-measure downgrades it to WARN, not pass.
import json, os, statistics

RANK = {"pass": 0, "no-reference": 1, "no-compare": 1, "inconclusive": 1, "pending": 1, "none": 0, "warn": 2, "fail": 3}


def pct(xs, p):
    xs = sorted(xs)
    return xs[min(len(xs) - 1, int(len(xs) * p))] if xs else None


def summarize(samples):
    xs = list(samples)
    if not xs:
        return {"n": 0, "p50": None, "p95": None, "max": None}
    return {"n": len(xs), "p50": round(statistics.median(xs), 2), "p95": round(pct(xs, 0.95), 2), "max": round(max(xs), 2)}


def median_diff_se(a, b, boot=400, seed=1):
    """standard error of median(a) - median(b), by bootstrap (deterministic: fixed seed). None when either side has < 2 samples.
    n=30 medians with a 12 ms p95-p50 spread have a ~2.3 ms SE on the difference (Fable report 2026-11-02): a 2 ms budget cannot be
    judged by a point estimate at that n."""
    import random
    a, b = list(a), list(b)
    if len(a) < 2 or len(b) < 2:
        return None
    rng = random.Random(seed)
    ds = []
    for _ in range(boot):
        ds.append(statistics.median(rng.choices(a, k=len(a))) - statistics.median(rng.choices(b, k=len(b))))
    return statistics.pstdev(ds)


def shell_base(s):
    """'/bin/zsh' -> 'zsh', 'powershell.exe' -> 'powershell', 'C:\\...\\bash.exe' -> 'bash'; None/'' -> None"""
    if not s:
        return None
    s = str(s).replace("\\", "/").rsplit("/", 1)[-1].lower()
    return (s[:-4] if s.endswith(".exe") else s).lstrip("-") or None


def overhead_verdict(overhead, se, budget, pane_shell=None, ref_shell=None, z=2.0):
    """-> 'no-compare' (pane and reference shells differ: the difference measures the shells), 'fail' (overhead - z*SE > budget: the
    budget is exceeded even at the low end), 'pass' (overhead + z*SE <= budget: within budget even at the high end), else 'inconclusive'
    (never a pass; the numbers are in the record). Without an SE (too few samples) the point estimate decides, as before."""
    ps, rs = shell_base(pane_shell), shell_base(ref_shell)
    if ps and rs and ps != rs:
        return "no-compare"
    if se is None:
        return "pass" if overhead <= budget else "fail"
    if overhead - z * se > budget:
        return "fail"
    if overhead + z * se <= budget:
        return "pass"
    return "inconclusive"


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


def evaluate(rid, phos, ref, budget_ms, kind, platform, remeasure=None, ref_name=None, floor=None, notes=None, pane_shell=None, ref_shell=None):
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
            se = median_diff_se(phos, ref)
            rec["overhead_se_ms"] = round(se, 2) if se is not None else None
            rec["pane_shell"], rec["reference_shell"] = pane_shell, ref_shell
            bv = overhead_verdict(rec["overhead_ms"], se, budget_ms, pane_shell, ref_shell)
            if bv == "inconclusive":
                rec["notes"].append("overhead %.2f +/- %.2f ms (SE of the median difference) vs budget %g ms: neither within nor over budget at 2 SE; more samples needed" % (rec["overhead_ms"], se, budget_ms))
            elif bv == "no-compare":
                rec["notes"].append("pane shell %s != reference shell %s: overhead %.2f ms is NOT a Phosphor figure; no verdict" % (pane_shell, ref_shell, rec["overhead_ms"]))
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
