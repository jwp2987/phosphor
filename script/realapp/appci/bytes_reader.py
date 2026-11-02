#!/usr/bin/env python3
# appci/bytes_reader.py: run INSIDE the terminal under test (`python3 bytes_reader.py OUTFILE SECONDS`): raw tty, appends one
# "epoch hex" line to OUTFILE for every chunk of bytes that reaches the pty, flushed at once, so a case can assert that an IME's
# marked text / a dead key leaked NOTHING before the commit and that the committed bytes are exactly the expected UTF-8.
# Creates OUTFILE.ready when listening. Exits on byte 0x03 (Ctrl-C, ISIG is off in raw mode) or after SECONDS.
import os, select, sys, termios, time, tty
out, secs = sys.argv[1], float(sys.argv[2])
fd = sys.stdin.fileno(); old = termios.tcgetattr(fd)
f = open(out, "w", buffering=1)
end = time.time() + secs
try:
    tty.setraw(fd)
    open(out + ".ready", "w").close()
    while time.time() < end:
        if not select.select([fd], [], [], 0.2)[0]:
            continue
        c = os.read(fd, 4096)
        if not c:
            break
        f.write("%.6f %s\n" % (time.time(), c.hex()))
        f.flush()
        if b"\x03" in c:
            break
finally:
    termios.tcsetattr(fd, termios.TCSADRAIN, old)
    f.close()
