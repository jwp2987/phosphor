#!/usr/bin/env python3
# appci/hostile_out.py (#2072, T8 hostile_output_recovers): the hostile terminal output of the case, as data.
#   seeded_bytes(seed, n)   the "binary file" a user cats by mistake: n pseudo-random bytes, the SAME bytes for the same seed on every machine
#                           and Python (a failure can be replayed; the case records the sha256). The harness writes it to a file, the pane `cat`s it.
#   script_chunks(tok)      what a badly behaved program leaves behind: window-title spam, a 1 MB OSC 8 hyperlink, mouse reporting switched on and
#                           never off, the alternate screen entered and never left.
# Run INSIDE the terminal under test as `python3 hostile_out.py script TOKEN`: writes the chunks to stdout and exits without restoring anything.
# No sibling imports (selftest appci_programs_find_their_siblings).
import sys

TITLES = 2000
LINK_BYTES = 1_000_000
ESC = b"\x1b"
MOUSE_ON = ESC + b"[?1000h" + ESC + b"[?1002h" + ESC + b"[?1006h"
ALT_ON = ESC + b"[?1049h"
# what the case types afterwards as CLEANUP only (after its verdicts): the modes above switched off, so a build that did not recover does not
# also break the cases that follow
RESTORE_PRINTF = r"printf '\033[?1049l\033[?1006l\033[?1002l\033[?1000l'"


def seeded_bytes(seed, n):
    """n bytes from a 64-bit linear congruential generator (Knuth's MMIX constants), high byte of each state: no dependence on the `random` module's
    version, so the file is identical everywhere"""
    out = bytearray(n)
    s = seed & 0xFFFFFFFFFFFFFFFF
    for i in range(n):
        s = (s * 6364136223846793005 + 1442695040888963407) & 0xFFFFFFFFFFFFFFFF
        out[i] = s >> 56
    return bytes(out)


def script_chunks(tok, titles=TITLES, link_bytes=LINK_BYTES):
    """the hostile script's output as a list of byte chunks, in order: titles, the long link, mouse reporting on, alt screen on"""
    t = tok.encode()
    chunks = []
    for i in range(titles):
        # OSC 0 ended by BEL and OSC 2 ended by ST, alternating
        if i % 2:
            chunks.append(ESC + b"]2;" + t + b"-title-%d" % i + ESC + b"\\")
        else:
            chunks.append(ESC + b"]0;" + t + b"-title-%d" % i + b"\x07")
    chunks.append(ESC + b"]8;;http://appci.invalid/" + b"a" * link_bytes + ESC + b"\\" + b"link-" + t + ESC + b"]8;;" + ESC + b"\\\r\n")
    chunks.append(MOUSE_ON)
    chunks.append(ALT_ON + ESC + b"[2J" + ESC + b"[H" + b"hostile alt screen " + t + b"\r\n")
    return chunks


def main(argv):
    if len(argv) != 3 or argv[1] != "script":
        sys.stderr.write("usage: hostile_out.py script TOKEN\n")
        return 2
    out = sys.stdout.buffer
    for c in script_chunks(argv[2]):
        out.write(c)
    out.flush()
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
