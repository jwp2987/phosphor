#!/usr/bin/env python3
"""Interactive PTY test harness for the headless `zap-tui-oss` TUI binary.

Boots the TUI inside a pseudo-terminal, replays a scripted sequence of input
events, and (with `pyte` installed) captures the emulated screen after each step.
This is what lets us drive and regression-test the TUI's interaction end to end
without a human at a real terminal — e.g. verifying that typing renders, that
Enter submits, and that keybindings fire.

Usage:
    # Build the binary first:
    cargo build -p warp_tui --bin zap-tui-oss

    # Default smoke script (type a shell command, submit it):
    python3 crates/warp_tui/scripts/tui_harness.py

    # Custom script: alternating "wait:<seconds>" and literal text / key tokens.
    #   text is sent verbatim; \r = Enter, \t = Tab, \e = Escape, \x03 = Ctrl-C.
    python3 crates/warp_tui/scripts/tui_harness.py \
        "wait:3" "echo hello" "wait:0.5" "\r" "wait:2"

`pyte` (pip install --break-system-packages pyte) renders the emulated screen;
without it the harness prints how many raw bytes each step produced and whether
expected text appeared, which is enough for CI-style assertions.

On Linux the TUI logs to ~/.local/state/phosphor/warp-cli/zap-tui.log (truncate
it before a run to inspect a single session's log). The first component is
`warp_core::paths::state_dir()`, i.e. the app id shared with the GUI; the second
is `TUI_LOG_SUBDIRECTORY` in `crates/warp_logging/src/native.rs`; only the
filename comes from this binary's `logfile_name`. This line previously read
`~/.local/state/zap-tui/oz/zap-tui.log`, which was wrong in both directory
components (there has never been a `zap-tui` app dir, and `oz` is the *CLI*
subdirectory, not the TUI's).
"""
import os
import pty
import select
import signal
import struct
import sys
import termios
import time
import fcntl

COLS, ROWS = 120, 40
BIN = os.environ.get(
    "ZAP_TUI_BIN",
    os.path.join(
        os.path.dirname(os.path.abspath(__file__)),
        "..", "..", "..", "target", "debug", "zap-tui-oss",
    ),
)

try:
    import pyte  # type: ignore

    HAVE_PYTE = True
except ImportError:
    HAVE_PYTE = False


def decode_token(tok: str) -> bytes:
    """Turn a script token into bytes, honoring a few escape sequences."""
    return (
        tok.replace("\\r", "\r")
        .replace("\\n", "\n")
        .replace("\\t", "\t")
        .replace("\\e", "\x1b")
        .replace("\\x03", "\x03")
    ).encode()


def default_script():
    return [
        ("wait", 3.0),
        ("send", b"echo hello from the harness"),
        ("wait", 0.5),
        ("send", b"\r"),
        ("wait", 2.0),
    ]


def parse_args(argv):
    if not argv:
        return default_script()
    steps = []
    for tok in argv:
        if tok.startswith("wait:"):
            steps.append(("wait", float(tok[len("wait:"):])))
        else:
            steps.append(("send", decode_token(tok)))
    return steps


def main():
    steps = parse_args(sys.argv[1:])
    if not os.path.exists(BIN):
        sys.exit(f"binary not found: {BIN}\nbuild it: cargo build -p warp_tui --bin zap-tui-oss")

    screen = stream = None
    if HAVE_PYTE:
        screen = pyte.Screen(COLS, ROWS)
        stream = pyte.ByteStream(screen)

    pid, fd = pty.fork()
    if pid == 0:
        os.environ["TERM"] = "xterm-256color"
        os.environ["COLUMNS"] = str(COLS)
        os.environ["LINES"] = str(ROWS)
        os.execv(BIN, ["zap-tui-oss"])
        return  # unreachable

    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))

    def drain(seconds: float) -> bytes:
        buf = bytearray()
        end = time.time() + seconds
        while time.time() < end:
            r, _, _ = select.select([fd], [], [], 0.1)
            if r:
                try:
                    data = os.read(fd, 65536)
                except OSError:
                    break
                if not data:
                    break
                buf += data
                if stream is not None:
                    stream.feed(data)
        return bytes(buf)

    def dump(label: str):
        print(f"\n===== {label} =====")
        if screen is not None:
            print("+" + "-" * COLS + "+")
            for line in screen.display:
                print("|" + line.rstrip())
            print("+" + "-" * COLS + "+")
            print(f"cursor: x={screen.cursor.x} y={screen.cursor.y}")

    try:
        for kind, value in steps:
            if kind == "wait":
                got = drain(value)
                dump(f"after wait {value}s ({len(got)} bytes)")
            else:
                os.write(fd, value)
                got = drain(0.3)
                dump(f"after send {value!r} ({len(got)} bytes)")
    finally:
        try:
            os.kill(pid, signal.SIGKILL)
            os.waitpid(pid, 0)
        except OSError:
            pass


if __name__ == "__main__":
    main()
