#!/usr/bin/env python3
"""Reproduces #960: flooding a typed bootstrap at a shell behind a blocking pty relay deadlocks.

A user-space relay (sudo -i) that does a blocking write of input into the inner pty, and only
reads the inner shell's echo between writes, hangs once the inner tty input queue (4095 bytes)
is full: the shell blocks writing its echo, the relay blocks writing input. Pacing the same bytes
in small writes completes.

    script/repro_subshell_relay_deadlock.py 4097 0       # unpaced flood: exits 2 (deadlock)
    script/repro_subshell_relay_deadlock.py 2048 0.025   # what the app does: exits 0

Args: chunk-size-bytes delay-seconds. Needs bash. Uses bash_body.sh as the payload.
"""
import os
import pty
import select
import signal
import sys
import time

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
body = open(os.path.join(root, "app/assets/bundled/bootstrap/bash_body.sh"), "rb").read()
payload = b"builtin read -r -d '' X << 'EOM' || :\n" + body + b"\nEOM\necho DONE_MARK\n"
chunk, delay = int(sys.argv[1]), float(sys.argv[2])

pid, fd = pty.fork()
if pid == 0:
    os.execvp("bash", ["bash", "--norc", "--noprofile", "-i"])

def hung(*_):
    print("DEADLOCK: relay blocked writing input while the shell blocks writing echo", flush=True)
    os.kill(pid, signal.SIGKILL)
    os._exit(2)

signal.signal(signal.SIGALRM, hung)
signal.alarm(20)
got, pos, nxt, start = b"", 0, 0.0, time.time()
while True:
    if select.select([fd], [], [], 0)[0]:
        try:
            got += os.read(fd, 65536)
        except OSError:
            break
        if b"DONE_MARK" in got.replace(b"echo DONE_MARK", b""):
            print("ok: bootstrap consumed in %.2fs" % (time.time() - start))
            os.kill(pid, signal.SIGKILL)
            sys.exit(0)
    if pos < len(payload) and time.time() >= nxt:
        pos += os.write(fd, payload[pos:pos + chunk])  # blocking, like the relay
        nxt = time.time() + delay
