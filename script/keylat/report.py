#!/usr/bin/env python3 -I
"""Summarise keystroke->photon latency runs from any of the three platform tools.

usage: report.py [--label NAME] FILE...

Input formats, detected per line (a file may mix runs; all samples are pooled):
  Linux  keylat.c    "K idx char realtime_ns" key lines + "P realtime_ns hash" change lines.
                     Latency = first P at or after each K. A key with no later P is counted
                     as `nopaint`, never dropped silently.
  macOS  keylat.swift a line "raw 12.3 14.1 ..." (milliseconds).
  Windows lat.ps1    CSV rows "label,idx,char,tsend,tfirst,tlast,..." (epoch ms);
                     latency = tfirst - tsend, rows with tfirst < 0 are `nopaint`.
  plain  a line "ms 12.3 14.1 ..." for anything else.

Prints N, p50, p90, p99, max (never a mean alone; see README.md).
Keep the whole report next to the reference terminal's report for the same script.
"""
import bisect
import statistics
import sys


def pct(sorted_values, p):
    if not sorted_values:
        return float("nan")
    k = (len(sorted_values) - 1) * p / 100.0
    lo = int(k)
    hi = min(lo + 1, len(sorted_values) - 1)
    return sorted_values[lo] + (sorted_values[hi] - sorted_values[lo]) * (k - lo)


def load(path):
    latencies, nopaint = [], 0
    keys, changes = [], []
    for line in open(path, errors="replace"):
        f = line.split()
        if not f:
            continue
        if f[0] == "K" and len(f) >= 4:
            keys.append(int(f[3]))
        elif f[0] == "P" and len(f) >= 2:
            changes.append(int(f[1]))
        elif f[0] in ("raw", "ms"):
            latencies += [float(x) for x in f[1:]]
        elif "," in line and f[0].count(",") >= 5:
            row = line.strip().split(",")
            try:
                ts, tf = float(row[3]), float(row[4])
            except ValueError:
                continue
            if tf < 0:
                nopaint += 1
            else:
                latencies.append(tf - ts)
    changes.sort()
    for t in keys:
        j = bisect.bisect_right(changes, t)
        if j < len(changes):
            latencies.append((changes[j] - t) / 1e6)
        else:
            nopaint += 1
    return latencies, nopaint


def main(argv):
    label = ""
    if argv[:1] == ["--label"]:
        label, argv = argv[1], argv[2:]
    if not argv:
        print(__doc__)
        return 2
    all_lat, all_np = [], 0
    for path in argv:
        lat, np = load(path)
        all_lat += lat
        all_np += np
    v = sorted(all_lat)
    n = len(v)
    note = "" if n >= 200 else "  (N < 200: below the protocol minimum)"
    print(
        f"{label or 'run'}: N={n} nopaint={all_np} p50={pct(v, 50):.1f} p90={pct(v, 90):.1f} "
        f"p99={pct(v, 99):.1f} max={(v[-1] if v else float('nan')):.1f} ms "
        f"(stdev {statistics.pstdev(v) if n > 1 else float('nan'):.1f}){note}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
