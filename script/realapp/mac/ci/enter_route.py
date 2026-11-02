#!/usr/bin/env python3
"""enter_route.py OUT: where did every Return go? (#1482)

Joins ktap's view (OUT/ktap.log: each Return keyDown at the annotated session tap, with the process macOS targets) with Phosphor's
own trace (`input_order hop=mac_enter stage=app_send_event ... ts=<NSEvent timestamp, s>` in OUT/run/logs/*/phosphor*.log), matching
by the event's own timestamp. Writes OUT/run/logs/enter_route.txt and prints a short summary for the job log:
  - Returns targeted at Phosphor that Phosphor's NSApplication never saw  -> lost between macOS and the app's event queue
  - Returns targeted at another process (with its name)                  -> focus/routing: the key went elsewhere
  - Returns Phosphor saw that the tap never saw                          -> should be 0 (a sanity check on the join)
Never fails the job: a missing input is reported, exit 0."""
import glob, os, re, subprocess, sys

out = sys.argv[1]
run = os.path.join(out, "run")
dst = os.path.join(run, "logs", "enter_route.txt")
lines = []


def say(s):
    lines.append(s)


def pname(pid):
    try:
        return subprocess.run(["ps", "-o", "comm=", "-p", str(pid)], capture_output=True, text=True, timeout=5).stdout.strip() or "?"
    except Exception:
        return "?"


tap = []   # (ts_s, target_pid, wall)
try:
    for l in open(os.path.join(out, "ktap.log"), errors="replace"):
        m = re.match(r"E down keycode=\d+ ts_ns=(\d+) target_pid=(-?\d+) source_pid=-?\d+ wall=([\d.]+)", l)
        if m:
            tap.append((int(m.group(1)) / 1e9, int(m.group(2)), float(m.group(3))))
        elif l.startswith("W "):
            say("ktap warning: " + l.strip())
except OSError as e:
    say("no ktap.log (%s): cannot say where Returns went" % e)

app_ts = []
for f in sorted(glob.glob(os.path.join(run, "logs", "*", "phosphor*.log"))):
    for l in open(f, errors="replace"):
        if "hop=mac_enter stage=app_send_event" in l:
            m = re.search(r" ts=([\d.]+)", l)
            if m:
                app_ts.append(float(m.group(1)))

app_pids = set()
for f in (os.path.join(out, "pids"), os.path.join(run, "pids")):
    try:
        for l in open(f):
            w = l.split()
            if len(w) >= 2 and w[0] == "app" and w[1].isdigit():
                app_pids.add(int(w[1]))
    except OSError:
        pass

TOL = 0.0005   # 0.5 ms: the NSEvent timestamp is the CGEvent timestamp in seconds


def seen(ts):
    return any(abs(ts - a) <= TOL for a in app_ts)


targeted_lost, elsewhere = [], {}
for ts, pid, wall in tap:
    if pid in app_pids:
        if not seen(ts):
            targeted_lost.append((ts, pid, wall))
    else:
        elsewhere.setdefault(pid, []).append((ts, wall))
unmatched_app = [a for a in app_ts if not any(abs(a - t[0]) <= TOL for t in tap)]

say("Returns at the session tap: %d; Phosphor app_send_event lines: %d; Phosphor pids: %s" % (len(tap), len(app_ts), sorted(app_pids) or "unknown"))
say("targeted at Phosphor but never reached its NSApplication: %d" % len(targeted_lost))
for ts, pid, wall in targeted_lost[:50]:
    say("  LOST ts=%.6f wall=%.3f target_pid=%d" % (ts, wall, pid))
n_else = sum(len(v) for v in elsewhere.values())
say("targeted at another process: %d" % n_else)
for pid, evs in sorted(elsewhere.items(), key=lambda kv: -len(kv[1])):
    say("  pid %d (%s): %d, first wall=%.3f" % (pid, pname(pid), len(evs), evs[0][1]))
say("seen by Phosphor but not by the tap (join check, expect 0): %d" % len(unmatched_app))

os.makedirs(os.path.dirname(dst), exist_ok=True)
open(dst, "w").write("\n".join(lines) + "\n")
print("[enter_route] " + "\n[enter_route] ".join(lines[:8]))
