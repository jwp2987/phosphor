#!/usr/bin/env python3
"""script/ci/perf_bands.py: per-(platform, mode, pane, metric) performance and memory BANDS from the committed app-test history.

OWNER DECISION (2026-11-12): "Make bands for all, I don't [have] enough feel for bash yet to know if 53.5 is good enough or not. So have the band
include that as passing and we can tighten later."  Every perf / memory verdict of the app tests reads test-results/bands.json (appci/bands.py)
instead of a fixed budget.

Reads every committed run test-results/<date>_<kind>_<id>_<pipeline>/app/<platform>/results.json[l] (X11 skipped; lab-* only for lab_tab_remote_dir's tab_to_insert, keyed by the platform without the lab- prefix; results.json may carry a
UTF-8 BOM; .jsonl rows or {"cases": [...]}), takes one value per run for each banded metric, keeps the LAST 8 runs (after the "since" cut of
perf_bands_config.json) and writes test-results/bands.json:

    {metric_key: {n, median, mad, upper, lower, values, runs, provisional (n < 5), set_by ("data" | "owner"), kind (ms | mb | s)}}

    metric_key = <platform>/<mode incl. pane>/<case>/<metric>        e.g. windows/terminal@gitbash/latency_enter/enter_to_start
    upper      = max(median + 3 * MAD, median + floor, max(window))  (floor: latency ms 0.3, memory MB 15, seconds 0.3) so EVERY value of the
                 window passes
    lower      = min(window, median - 3 * MAD): an improvement note, never a failure

RATCHET: regenerating may only move an `upper` DOWN. If the data would raise it the old one is kept and
    WOULD LOOSEN <key> old -> new (owner OK needed: --allow-loosen KEY)
is printed, unless the widening is at most the kind's AUTO_LOOSEN limit (0.5 ms / 15 MB / 0.3 s): that applies by itself and prints AUTO-LOOSEN (owner
2026-11-12: noise-sized widening needs no OK; anything larger does). --allow-loosen KEY applies a larger one and records set_by "owner" and the date. Changing perf_bands_config.json "since" so that a band loosens
is a loosen as well: it is held back the same way.

    perf_bands.py                    regenerate test-results/bands.json
    perf_bands.py --check            exit 1 when bands.json is stale against the data (script/precheck)
    perf_bands.py --allow-loosen KEY [--allow-loosen KEY2 ...]
"""
import argparse
import glob
import json
import math
import os
import re
import statistics
import sys
import datetime
import fnmatch

REPO = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
RESULTS = os.path.join(REPO, "test-results")
BANDS = os.path.join(RESULTS, "bands.json")
CONFIG = os.path.join(os.path.dirname(os.path.abspath(__file__)), "perf_bands_config.json")
WINDOW = 8
PROVISIONAL_BELOW = 5
FLOOR = {"ms": 0.3, "mb": 15.0, "s": 0.3}
# owner 2026-11-12 ("I am fine with half a milli"): how far a band may widen by itself per regeneration; more needs --allow-loosen
AUTO_LOOSEN = {"ms": 0.5, "mb": 15.0, "s": 0.3}
SKIP_PLATFORMS = ("linux-x11", "x11")


# ---------------------------------------------------------------- reading the history

def read_rows(path):
    """rows of one results.json / results.jsonl (BOM tolerant); unreadable -> []"""
    try:
        if path.endswith(".jsonl"):
            out = []
            for line in open(path, encoding="utf-8-sig"):
                line = line.strip()
                if line:
                    try:
                        out.append(json.loads(line))
                    except ValueError:
                        pass
            return out
        doc = json.load(open(path, encoding="utf-8-sig"))
        return doc.get("cases", []) if isinstance(doc, dict) else []
    except (OSError, ValueError):
        return []


def platform_name(dirname):
    return "wayland" if dirname == "linux-wayland" else dirname


def run_order(name):
    """(date, pipeline id, name): the pipeline id is monotonic in time, the build tag is not (r18w9 ran before r10a)"""
    m = re.match(r"(\d{4}-\d{2}-\d{2})_.*?_(\d+)$", name)
    return (name[:10], int(m.group(2)) if m else 0, name)


def p50(v):
    return v.get("p50") if isinstance(v, dict) else None


def num(v):
    return round(float(v), 2) if isinstance(v, (int, float)) and not isinstance(v, bool) and math.isfinite(v) else None


def extract(row, plat=""):
    """one case row -> [(metric suffix, value, kind)]. The suffix is <case>/<metric>. Only figures a verdict is made on."""
    name, m = row.get("name"), row.get("metrics") or {}
    out = []

    def add(suffix, v, kind):
        v = num(v)
        if v is not None:
            out.append(("%s/%s" % (name, suffix), v, kind))

    for k, rec in (m.get("lat") or {}).items():
        if not isinstance(rec, dict) or rec.get("budget_kind") not in ("overhead", "absolute"):
            continue
        v = rec.get("overhead_ms")
        if v is None and rec.get("reference") is None and name == "latency_key_log":
            v = p50(rec.get("phosphor"))          # no same-method reference exists: the Phosphor-side median is the figure
        add(k, v, "ms")
        sh = rec.get("app_share") or {}
        if sh.get("p50") is not None and (sh.get("usable") or sh.get("gate_only") or sh.get("budget_ms") is not None):
            add(k + "#app", sh["p50"], "ms")
    # the app's own share of an Enter / the key -> pty write, from the case metrics of runs older than the latency record carrying it
    if name == "latency_enter":
        if not any(s == "latency_enter/enter_to_start#app" for s, _, _ in out):
            add("enter_to_start#app", m.get("app_enter_to_pty_write_ms"), "ms")
    if name == "latency_enter_ssh":
        if not any(s == "latency_enter_ssh/ssh_enter_to_start#app" for s, _, _ in out):
            add("ssh_enter_to_start#app", m.get("app_enter_to_pty_write_ms"), "ms")
    if name == "latency_key_log" and plat != "macos" and not any(s == "latency_key_log/key_to_frame#app" for s, _, _ in out):
        add("key_to_frame#app", p50(m.get("key_to_pty_write_ms")), "ms")      # older runs: the key -> pty write share (the Linux / Windows gate)
    # the one lab figure that is banded (owner 2026-11-13 'Turn it into a band'): lab_tab_remote_dir's Tab -> insert time, per shell / latency / helper arm
    if name and name.startswith("lab_tab_remote_dir."):
        add("tab_to_insert", m.get("tab_to_insert_ms"), "ms")
    # subshell: the first inner line after its Enter (seconds)
    for k, v in m.items():
        if k.endswith("_inner_line_after_enter_s"):
            add(k, v, "s")
    # editor memory: only runs measured the #2185 way (heap prime, every cycle judged, baseline from the post-trim heap)
    if name == "editor_memory_7mb" and "heap_prime" in m:
        for k, v in m.items():
            mt = re.match(r"^(txt|rs)_open_cost_worst_mb$", k)
            if mt and isinstance(v, dict):
                for fig, x in v.items():
                    add("%s_open_cost_worst/%s" % (mt.group(1), fig), x, "mb")
            mt = re.match(r"^(txt|rs)_rss_close(?:_(\w+?))?_delta_mb$", k)
            if mt:
                add("%s_held_after_close/%s" % (mt.group(1), mt.group(2) or "rss"), v, "mb")
            mt = re.match(r"^(txt|rs)_cycle_growth(?:_(\w+?))?_mb$", k)
            if mt:
                add("%s_cycle_growth/%s" % (mt.group(1), mt.group(2) or "rss"), v, "mb")
    return out


def history(results=RESULTS):
    """-> ({metric_key: [(run_name, value)] in run order, one per run}, {metric_key: kind}, [run names])"""
    series, kinds, runs = {}, {}, set()
    dirs = sorted((d for d in glob.glob(os.path.join(results, "20*")) if os.path.isdir(d)), key=lambda d: run_order(os.path.basename(d)))
    for d in dirs:
        run = os.path.basename(d)
        for pdir in sorted(glob.glob(os.path.join(d, "app", "*"))):
            plat = os.path.basename(pdir)
            lab = plat.startswith("lab")
            if plat in SKIP_PLATFORMS:
                continue
            f = os.path.join(pdir, "results.jsonl")
            if not os.path.exists(f):
                f = os.path.join(pdir, "results.json")
            rows = read_rows(f) if os.path.exists(f) else []
            seen = {}
            for row in rows:
                if "name" not in row:
                    continue
                mode = row.get("mode") or ""
                for suffix, v, kind in extract(row, platform_name(plat)):
                    if lab and not suffix.endswith("/tab_to_insert"):
                        continue          # labs are not banded, except the Tab -> insert time
                    seen["%s/%s/%s" % (platform_name(plat[4:] if lab else plat), mode, suffix)] = (v, kind)
            for key, (v, kind) in seen.items():
                series.setdefault(key, []).append((run, v))
                kinds[key] = kind
                runs.add(run)
    return series, kinds, sorted(runs, key=run_order)


# ---------------------------------------------------------------- the formula and the ratchet

def load_config(path=CONFIG):
    try:
        return json.load(open(path, encoding="utf-8-sig"))
    except (OSError, ValueError):
        return {}


def apply_since(key, pairs, config, all_runs):
    """drop the runs before the "since" run of the first config pattern matching `key` (fnmatch on the metric key)"""
    for pat, since in (config.get("since") or {}).items():
        if fnmatch.fnmatch(key, pat):
            hit = [r for r in all_runs if since in r]
            if not hit:
                raise SystemExit("perf_bands_config.json: since %r (for %s) matches no run directory" % (since, pat))
            cut = run_order(hit[0])
            return [(r, v) for r, v in pairs if run_order(r) >= cut]
    return pairs


def mad_of(xs):
    med = statistics.median(xs)
    return statistics.median([abs(x - med) for x in xs]) if len(xs) > 1 else 0.0


def compute_band(pairs, kind):
    window = pairs[-WINDOW:]
    xs = [v for _, v in window]
    med, mad = statistics.median(xs), mad_of(xs)
    upper = max(med + 3 * mad, med + FLOOR[kind], max(xs))
    lower = min(min(xs), med - 3 * mad)
    return {"n": len(xs), "median": round(med, 2), "mad": round(mad, 2), "upper": math.ceil(upper * 100 - 1e-9) / 100.0,
            "lower": math.floor(lower * 100 + 1e-9) / 100.0, "values": xs, "runs": [r for r, _ in window],
            "provisional": len(xs) < PROVISIONAL_BELOW, "set_by": "data", "kind": kind}


def generate(results=RESULTS, old=None, allow=(), config=None, today=None):
    """-> (bands dict, [WOULD LOOSEN lines]). The ratchet is applied against `old` (the committed bands)."""
    old = old or {}
    config = load_config() if config is None else config
    series, kinds, all_runs = history(results)
    bands, notes = {}, []
    for key in sorted(series):
        pairs = apply_since(key, series[key], config, all_runs)
        if not pairs:
            continue
        b = compute_band(pairs, kinds[key])
        o = old.get(key)
        if o and b["upper"] > o["upper"] + 1e-9 and max(b["values"] or [o["upper"]]) <= o["upper"] + 1e-9 and key not in allow:
            # r27a: when every value in the window is INSIDE the band, nothing widens it — a spread-driven formula (median + 3 MAD over a few, far-apart points)
            # would loosen the band when a run got FASTER (macOS ssh Enter -4.97 ms under a 5.17 ms band asked for 14.71). Keep the old upper.
            b["upper"] = o["upper"]
            b["set_by"] = o.get("set_by", "data")
            if "owner_date" in o:
                b["owner_date"] = o["owner_date"]
        elif o and b["upper"] > o["upper"] + 1e-9:
            if key in allow:
                b["set_by"], b["owner_date"] = "owner", today or datetime.date.today().isoformat()
            elif b["upper"] - o["upper"] <= AUTO_LOOSEN[kinds[key]] + 1e-9:
                # owner 2026-11-12: a band may widen on its own by up to its floor per regeneration (measurement noise); more needs the owner
                notes.append("AUTO-LOOSEN %s %s -> %s (within the %s %s auto-loosen limit)" % (key, o["upper"], b["upper"], AUTO_LOOSEN[kinds[key]], kinds[key]))
                b["set_by"] = "data"
            else:
                notes.append("WOULD LOOSEN %s %s -> %s (owner OK needed: --allow-loosen KEY)" % (key, o["upper"], b["upper"]))
                b["upper"] = o["upper"]
                b["set_by"] = o.get("set_by", "data")
                if "owner_date" in o:
                    b["owner_date"] = o["owner_date"]
        elif o and o.get("set_by") == "owner" and abs(b["upper"] - o["upper"]) < 1e-9 and "owner_date" in o:
            b["set_by"], b["owner_date"] = "owner", o["owner_date"]
        bands[key] = b
    for key, o in old.items():          # a key the data no longer yields keeps its band (ratchet: nothing loosens by omission)
        bands.setdefault(key, o)
    return bands, notes


def load_bands(path=BANDS):
    try:
        return json.load(open(path, encoding="utf-8-sig"))
    except (OSError, ValueError):
        return {}


def dump(bands):
    return json.dumps(bands, indent=1, sort_keys=True) + "\n"


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--check", action="store_true", help="exit 1 when bands.json is stale against the data")
    ap.add_argument("--allow-loosen", action="append", default=[], metavar="KEY", help="apply a loosening the owner approved (records set_by owner)")
    ap.add_argument("--results", default=RESULTS)
    ap.add_argument("--bands", default=BANDS)
    a = ap.parse_args(argv)
    old = load_bands(a.bands)
    bands, notes = generate(a.results, old, set(a.allow_loosen))
    for n in notes:
        print(n)
    for k in a.allow_loosen:
        if k not in bands:
            print("--allow-loosen %s: no such band" % k, file=sys.stderr)
            return 2
    text = dump(bands)
    if a.check:
        cur = open(a.bands, encoding="utf-8").read() if os.path.exists(a.bands) else ""
        if cur != text:
            print("STALE: %s differs from the committed history; run script/ci/perf_bands.py and commit it" % a.bands)
            return 1
        print("bands.json is current (%d bands)" % len(bands))
        return 0
    os.makedirs(os.path.dirname(a.bands), exist_ok=True)
    with open(a.bands, "w", encoding="utf-8") as f:
        f.write(text)
    print("wrote %s: %d bands (%d provisional)" % (a.bands, len(bands), sum(1 for b in bands.values() if b.get("provisional"))))
    return 0


if __name__ == "__main__":
    sys.exit(main())
