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


def median_se(a, boot=400, seed=1):
    """standard error of the median of ONE side's samples (bootstrap, fixed seed); None below 2 samples. Recorded per side next to n (#1514 follow-up)."""
    import random
    a = list(a)
    if len(a) < 2:
        return None
    rng = random.Random(seed)
    return statistics.pstdev([statistics.median(rng.choices(a, k=len(a))) for _ in range(boot)])


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


def block_drift(xs, ys=None, z=2.0):
    """Within-series drift (#1514; kept for interleaved runs too, where it shows drift the A/B/A/B blocks did NOT cancel): compare the median of the first half of a series with the
    median of the second half. -> (drift_ms, significant): significant when |drift| > z * SE of that difference. None when < 8 samples.
    Slow drift (thermal, GC, background load) between the Phosphor series and the later reference series shows up as a half-to-half move."""
    xs = list(xs or [])
    if len(xs) < 8:
        return None
    h = len(xs) // 2
    a, b = xs[:h], xs[h:]
    se = median_diff_se(b, a)
    d = statistics.median(b) - statistics.median(a)
    return round(d, 2), bool(se is not None and abs(d) > z * se)


def loss_rate_bound(k, n, conf=0.95):
    """one-sided Clopper-Pearson UPPER confidence bound on a loss rate: k lost of n trials -> p such that P(X <= k | n, p) = 1 - conf
    (bisection; deterministic). k=0 gives 1-(1-conf)^(1/n): 0/40 -> 7.2 % at 95 %, i.e. 40 clean trials cannot exclude a 1/40 rate. None for n < 1."""
    if n < 1:
        return None
    k = max(0, min(int(k), int(n)))
    if k >= n:
        return 1.0
    from math import comb

    def cdf(p):
        return sum(comb(n, i) * p ** i * (1 - p) ** (n - i) for i in range(k + 1))
    lo, hi = 0.0, 1.0
    for _ in range(80):
        mid = (lo + hi) / 2
        if cdf(mid) > 1 - conf:
            lo = mid
        else:
            hi = mid
    return hi


def trials_to_exclude(rate, conf=0.95):
    """clean (zero-loss) trials needed for the upper bound to fall below `rate`: ceil(ln(1-conf)/ln(1-rate)); 119 for 1/40 at 95 %"""
    from math import ceil, log
    return int(ceil(log(1 - conf) / log(1 - rate)))


def idle_schedule(gaps, n, block=10, warm=0, first="phosphor", alternate=True, two_sided=True):
    """#1160: the order of the first-key-after-idle samples. -> [(gap, side, count, discard)]: per gap, blocks of `block` samples alternate between
    'phosphor' and 'reference' (A/B/A/B by block; the side that starts alternates per gap when `alternate`, so neither always goes first), each side
    totalling exactly n kept samples; the first block of each side per gap also carries `warm` extra samples that are discarded. One-sided (no
    reference): only phosphor blocks."""
    other = {"phosphor": "reference", "reference": "phosphor"}
    out = []
    for gi, g in enumerate(gaps):
        start = first if not (alternate and gi % 2) else other[first]
        sides = [start, other[start]] if two_sided else ["phosphor"]
        left = {s: n for s in sides}
        seen = set()
        while any(v > 0 for v in left.values()):
            for s in sides:
                if left[s] <= 0:
                    continue
                c = min(block, left[s])
                left[s] -= c
                d = 0 if s in seen else warm
                seen.add(s)
                out.append((g, s, c + d, d))
    return out


def block_schedule(n, block=5, warm=0, lead=1, first="phosphor"):
    """#1514: the A/B/A/B order of an interleaved latency measurement. -> [(side, count, discard)]: blocks of `block` KEPT samples alternate
    strictly between 'phosphor' and 'reference' (never per sample: a focus switch is itself an activation), each side totalling exactly n kept
    samples. Every block is led by `lead` extra samples that are discarded (the first sample after a focus switch pays the activation), and
    the first block of each side also carries `warm` discarded warm-up samples. `count` is what to take (kept + discarded); the first `discard`
    of them are dropped."""
    out = []
    for _gap, side, count, disc in idle_schedule([0], n, block=block, warm=warm, first=first, alternate=False, two_sided=True):
        out.append((side, count + lead, disc + lead))
    return out


def interleave_block():
    return max(1, int(os.environ.get("APPCI_LAT_BLOCK", "5")))


def interleave_lead():
    return max(0, int(os.environ.get("APPCI_LAT_BLOCK_LEAD", "1")))


def interleave_enabled():
    return os.environ.get("APPCI_LAT_INTERLEAVE", "1") != "0"


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


def changed_since(prev_doc, rows):
    """#1513: case status changes against the previous build's results.json -> list of (kind, "mode/case", old, new), kind in
    NEW-FAILURE | FIXED | MISSING | NEW-CASE | CHANGED. Informational (no exit-code effect). A case present before and absent now is MISSING (a vanished
    case reads like a pass otherwise). No previous doc -> []."""
    if not prev_doc:
        return []
    key = lambda c: "%s/%s" % (c.get("mode", ""), c["name"])
    old = {key(c): c.get("status") for c in prev_doc.get("cases", []) if "name" in c}
    new = {key(c): c.get("status") for c in rows if "name" in c}
    out = []
    for k in sorted(set(old) | set(new)):
        o, n = old.get(k), new.get(k)
        if o is None:
            out.append(("NEW-CASE", k, None, n))
        elif n is None:
            out.append(("MISSING", k, o, None))
        elif o != n:
            out.append(("NEW-FAILURE" if n == "fail" else "FIXED" if o == "fail" else "CHANGED", k, o, n))
    return out


def load_prev():
    d = load_json(os.environ.get("APPCI_PREV_RESULTS", ""))
    return flat_lat(d) if d else {}


def evaluate(rid, phos, ref, budget_ms, kind, platform, remeasure=None, ref_name=None, floor=None, notes=None, pane_shell=None, ref_shell=None,
             expected=None, resolution_ms=None, mode="sequential", implausible_neg_ms=None):
    """phos/ref: sample lists (warm-up already dropped). remeasure: callable returning a fresh phos sample list, or None.
    expected: samples the case asked for (a PASS needs >= 90 % of them AND >= min_n, #1514); resolution_ms: the measurement's time quantum
    (a PASS is withheld when it exceeds budget/2). Both only ever turn a pass into 'inconclusive', never a fail into a pass.
    implausible_neg_ms: when set and the overhead is below -implausible_neg_ms (and beyond 2 SE), the reference is judged to be mis-measured (Phosphor
    cannot be that much FASTER than a bare terminal at the same job): verdict 'no-compare' (NOREF), never a PASS. Per-side n and the SE of each
    side's median are always recorded (rec['per_side']).
    Returns the latency record (see header)."""
    ps = summarize(phos)
    rec = {"measurement_mode": mode, "phosphor": ps, "reference": summarize(ref) if ref else None, "reference_name": ref_name if ref else None,
           "budget_ms": budget_ms, "budget_kind": kind, "floor_ms": floor, "notes": list(notes or [])}
    rec["per_side"] = {"phosphor": dict(n=ps["n"], se_median_ms=(lambda x: round(x, 2) if x is not None else None)(median_se(phos)))}
    if ref:
        rec["per_side"]["reference"] = dict(n=len(ref), se_median_ms=(lambda x: round(x, 2) if x is not None else None)(median_se(ref)))
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
            elif bv != "no-compare" and implausible_neg_ms and rec["overhead_ms"] < -implausible_neg_ms and (se is None or rec["overhead_ms"] + 2 * se < 0):
                bv = "no-compare"
                rec["implausible"] = True
                rec["notes"].append("IMPLAUSIBLE reference: Phosphor is %.2f ms FASTER than the reference (> %g ms, beyond 2 SE of %s): an ordering / focus / idle-wake artifact of the reference window, "
                                    "not a Phosphor figure; NOREF, no verdict (a PASS here would rest on a mis-measured reference)" % (-rec["overhead_ms"], implausible_neg_ms, se))
            elif bv == "no-compare":
                rec["notes"].append("pane shell %s != reference shell %s: overhead %.2f ms is NOT a Phosphor figure; no verdict" % (pane_shell, ref_shell, rec["overhead_ms"]))
        else:
            rec["overhead_ms"] = None
            bv = "no-reference"
    elif kind == "absolute":
        base = ps["p50"]
        rec["overhead_ms"] = round(base - (floor or 0.0), 2) if base is not None else None
        bv = "pass" if rec["overhead_ms"] is not None and rec["overhead_ms"] <= budget_ms else "fail"
    # #1514: thin evidence is never a pass. Fails stay fails.
    why = []
    if ps["n"] < min_n():
        why.append("n=%d < %d samples" % (ps["n"], min_n()))
    if expected and ps["n"] < 0.9 * expected:
        why.append("only %d of %d samples (< 90 %%)" % (ps["n"], expected))
    if resolution_ms and kind in ("overhead", "absolute") and resolution_ms > budget_ms / 2.0:
        why.append("resolution-limited: time quantum %.2f ms > half the %g ms budget" % (resolution_ms, budget_ms))
        rec["resolution_ms"] = round(resolution_ms, 2)
    if kind == "overhead" and ref and ps["n"] >= 8:
        for tag, series in (("Phosphor", phos), ("reference", ref)):
            dr = block_drift(series)
            if dr and dr[1] and abs(dr[0]) > budget_ms / 2.0:
                why.append("%s series drifted %+.2f ms between its halves (%s measurement)" % (tag, dr[0], "interleaved A/B/A/B" if mode == "interleaved" else "sequential A-then-B"))
            if dr:
                rec.setdefault("drift_ms", {})[tag.lower()] = dr[0]
    if why and bv == "pass":
        bv = "inconclusive"
        rec["insufficient"] = why
        rec["notes"].append("INCONCLUSIVE, not a pass: " + "; ".join(why))
    rec["budget_verdict"] = bv
    # regression against previous build and the committed baseline
    prev = load_prev().get(rid)
    bst, bmap = load_baseline(platform)
    base = bmap.get(rid)
    rec["prev"] = None
    rec["baseline"] = {"status": bst}
    pv, bsv = "none", ("pending" if bst == "pending" else "none")
    if prev and (prev.get("phosphor") or {}).get("p50") is not None and (prev["phosphor"].get("n") or 0) < min_n():
        # #1513: a previous run with too few samples (e.g. the app crashed mid-run) is not a representative reference
        rec["notes"].append("previous build ignored: its record has n=%s < %d samples" % (prev["phosphor"].get("n"), min_n()))
        rec["prev"] = {"ignored": "n=%s < %d" % (prev["phosphor"].get("n"), min_n())}
        prev = None
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
