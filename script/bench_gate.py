#!/usr/bin/env python3
"""Fail when a criterion benchmark's median exceeds its limit in script/bench_thresholds.json.

The `bench` CI job runs `cargo bench` (which only checks that the benches run and deliver),
then calls this on the criterion output directory. Usage:

    script/bench_gate.py CRITERION_DIR              # gate: exit 1 on a regression or a missing bench
    script/bench_gate.py CRITERION_DIR --suggest    # print measured medians and a suggested max_ns (10x)

CRITERION_DIR is the `criterion` directory (e.g. $PHOSPHOR_TARGET_ROOT/ci-bench/criterion or
target/criterion). Every threshold entry must have a result (a renamed or deleted bench fails the
gate instead of silently dropping its limit); results without a threshold are reported only.
Stdlib only; no cargo needed.
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
THRESHOLDS = os.path.join(HERE, "bench_thresholds.json")


def collect(root):
    """full_id -> median ns, from <root>/**/new/{benchmark,estimates}.json."""
    out = {}
    for dirpath, _dirs, files in os.walk(root):
        if os.path.basename(dirpath) != "new" or "estimates.json" not in files:
            continue
        try:
            with open(os.path.join(dirpath, "estimates.json")) as f:
                est = json.load(f)
            bid = None
            if "benchmark.json" in files:
                with open(os.path.join(dirpath, "benchmark.json")) as f:
                    bid = json.load(f).get("full_id")
            if not bid:
                bid = os.path.relpath(os.path.dirname(dirpath), root).replace(os.sep, "/")
            out[bid] = float(est["median"]["point_estimate"])
        except (OSError, KeyError, ValueError) as e:
            print(f"bench_gate: cannot read {dirpath}: {e}", file=sys.stderr)
    return out


def fmt(ns):
    for unit, div in (("s", 1e9), ("ms", 1e6), ("us", 1e3)):
        if ns >= div:
            return f"{ns / div:.2f} {unit}"
    return f"{ns:.1f} ns"


def main(argv):
    args = [a for a in argv[1:] if not a.startswith("--")]
    suggest = "--suggest" in argv
    if len(args) != 1:
        print(__doc__, file=sys.stderr)
        return 2
    results = collect(args[0])
    if not results:
        print(f"bench_gate: no criterion results under {args[0]}", file=sys.stderr)
        return 1
    with open(THRESHOLDS) as f:
        cfg = json.load(f)
    plat = "windows" if sys.platform == "win32" else "macos" if sys.platform == "darwin" else "linux"
    factor = float(cfg.get("platform_factor", {}).get(plat, 1.0))
    limits = {k: int(v * factor) for k, v in cfg["max_ns"].items()}
    if factor != 1.0:
        print(f"bench_gate: {plat} limits scaled x{factor:g} (platform_factor in bench_thresholds.json)")
    bad = 0
    for bid in sorted(set(results) | set(limits)):
        got, lim = results.get(bid), limits.get(bid)
        if got is None:
            print(f"MISSING  {bid}: limit {fmt(lim)} but no result (bench renamed or removed?)")
            bad += 1
        elif lim is None:
            print(f"no-limit {bid}: {fmt(got)}")
        elif suggest:
            print(f"{bid}: median {fmt(got)}, limit {fmt(lim)}, suggested max_ns {int(got * 10)}")
        elif got > lim:
            print(f"FAIL     {bid}: median {fmt(got)} > limit {fmt(lim)}")
            bad += 1
        else:
            print(f"ok       {bid}: median {fmt(got)} <= {fmt(lim)}")
    if bad and not suggest:
        print(f"bench_gate: {bad} regression(s)/missing; see docs/dev-tooling.md to update limits", file=sys.stderr)
    return 0 if suggest or not bad else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
