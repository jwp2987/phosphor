#!/usr/bin/env python3
# appci/ts_reader.py: run INSIDE the terminal under test (`python3 ts_reader.py OUTFILE N`): puts the tty in raw
# mode and records the arrival time of each of N characters, so the key cadence the input tool really produced
# (10-25 ms/key) is measured at the pty and written to OUTFILE as "ord t" lines. Used by the cadence case.
import sys, time, tty, termios, os
out, n = sys.argv[1], int(sys.argv[2])
fd = sys.stdin.fileno(); old = termios.tcgetattr(fd)
rows = []
try:
    tty.setraw(fd)
    sys.stdout.write("READY\r\n"); sys.stdout.flush()
    while len(rows) < n:
        c = os.read(fd, 1)
        if not c:
            break
        rows.append((c[0], time.time()))
finally:
    termios.tcsetattr(fd, termios.TCSADRAIN, old)
    open(out, "w").write("".join("%d %.6f\n" % r for r in rows))
