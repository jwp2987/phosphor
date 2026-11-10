#!/usr/bin/env python3
"""stdin -> stdout: drops app log lines ([INFO]/[WARN]/[DEBUG]/[TRACE]) so a failing test's panic message survives
GitLab's 4 MB job-log cap (round 8: the paced agent-fuzz failure's panic text was cut off by svg/byop-diag spam).
The unfiltered output is kept as a job artifact (nextest-*.log); this only shortens what the job log shows.
Lines kept: everything that is not a timestamped app log line, plus every [ERROR] line. Drop counts print at the end."""
import re
import sys

LOG = re.compile(r"^(?:\S+Z \S+\s+)?\s*(?:\x1b\[[0-9;]*m)*\d\d:\d\d:\d\d\.\d+ (?:\x1b\[[0-9;]*m)*\[(INFO|WARN|DEBUG|TRACE)\]")
dropped = 0
for raw in sys.stdin.buffer:
    line = raw.decode("utf-8", "replace")
    if LOG.match(line):
        dropped += 1
        continue
    sys.stdout.write(line)
sys.stdout.write(f"[filter_test_log] dropped {dropped} INFO/WARN/DEBUG/TRACE app log lines (full log: the nextest-*.log artifact)\n")
