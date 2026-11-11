#!/usr/bin/env python3
# appci/osc52_reader.py: run INSIDE the terminal under test (`python3 osc52_reader.py OUTFILE SECONDS`), POSIX only (termios).
# Raw tty. Sends the OSC 52 clipboard READ query (ESC ] 52 ; c ; ? BEL) and then DSR 5 (ESC [ 5 n), and appends one "epoch hex" line to
# OUTFILE for every chunk of bytes the terminal sends back. DSR 5 is the control: a terminal always answers it (ESC [ 0 n), so a file with
# that answer and no OSC 52 reply means "the query was refused", not "replies never reach this program".
# Stops 1.5 s after the DSR answer (an OSC 52 reply is written when the clipboard read finishes, so it may come after it), or after SECONDS.
# Writes OUTFILE.done last. Used by the case osc52_write_read (cases_r14_state.py); judged by cases_r14_state.judge_reader.
import os, select, sys, termios, time, tty

QUERY = b"\x1b]52;c;?\x07"
DSR = b"\x1b[5n"
DSR_REPLY = b"\x1b[0n"
GRACE_S = 1.5


def main():
    out, secs = sys.argv[1], float(sys.argv[2])
    fd = sys.stdin.fileno()
    old = termios.tcgetattr(fd)
    f = open(out, "w", buffering=1)
    end = time.time() + secs
    seen = b""
    try:
        tty.setraw(fd)
        os.write(sys.stdout.fileno(), QUERY + DSR)
        while time.time() < end:
            if not select.select([fd], [], [], 0.1)[0]:
                continue
            c = os.read(fd, 4096)
            if not c:
                break
            f.write("%.6f %s\n" % (time.time(), c.hex()))
            f.flush()
            had = DSR_REPLY in seen
            seen += c
            if DSR_REPLY in seen and not had:
                end = min(end, time.time() + GRACE_S)
    finally:
        termios.tcsetattr(fd, termios.TCSADRAIN, old)
        f.close()
        open(out + ".done", "w").close()


if __name__ == "__main__":
    main()
