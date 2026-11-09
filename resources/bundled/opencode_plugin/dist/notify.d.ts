/**
 * Send a notification via OSC 777 escape sequence.
 * Only emits when the terminal declares cli-agent protocol support,
 * avoiding garbled output in other terminals (and working over SSH).
 *
 * On Unix we write to /dev/tty so the sequence bypasses any
 * stdout redirection or terminal-multiplexer capture.
 * On Windows /dev/tty doesn't exist, so we fall back to
 * process.stdout which works because OpenCode plugins run
 * in-process (sharing the same ConPTY-connected stdout).
 */
declare function warpNotify(title: string, body: string): void;
export { warpNotify };
