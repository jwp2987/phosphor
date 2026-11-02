"""macOS reference terminal plumbing (#1485): the per-run .terminal window-settings document and Terminal.app lifecycle.

The reference window must close itself when its shell exits (shellExitAction = 0), and a Terminal.app that THIS run launched is quit at
teardown by its exact PID (SIGTERM, then SIGKILL). A Terminal.app that was already running (someone's windows) is never quit.
No Apple Events, no change to com.apple.Terminal's preferences by the harness itself, no pgrep/pkill: PIDs come from `ps -A -o pid=,comm=`."""
import atexit, os, plistlib, shlex, signal, subprocess, time

TERMINAL_PATH = "/System/Applications/Utilities/Terminal.app/Contents/MacOS/Terminal"
SHELL_EXIT_CLOSE = 0        # Terminal's shellExitAction: 0 = close the window, 1 = close only if the shell exited cleanly, 2 = keep it

_state = {"snapshotted": False, "was_running": False, "pid": None, "run": None}


def terminal_pids():
    """exact PIDs of Terminal.app (the comm column is the full executable path, matched exactly)"""
    try:
        out = subprocess.run(["ps", "-A", "-o", "pid=,comm="], capture_output=True, text=True, timeout=10).stdout
    except Exception:
        return []
    pids = []
    for line in out.splitlines():
        parts = line.strip().split(None, 1)
        if len(parts) == 2 and parts[0].isdigit() and parts[1].strip() == TERMINAL_PATH:
            pids.append(int(parts[0]))
    return pids


def snapshot(run):
    """call once at harness start, BEFORE any reference launch: was Terminal.app already running?"""
    if _state["snapshotted"]:
        return
    _state.update(snapshotted=True, was_running=bool(terminal_pids()), run=run)
    atexit.register(quit_if_ours)


def write_settings(path, name, cwd, home, ctl, cols=120, rows=30):
    """a Window Settings document: Terminal opens it (`open -a Terminal file.terminal`) as a window running CommandString under the
    login shell, and closes that window when the shell exits."""
    cmd = "cd %s && exec env HOME=%s ZDOTDIR=%s PA_CTL=%s zsh -i" % tuple(shlex.quote(x) for x in (cwd, home, home, ctl))
    doc = {"name": name, "type": "Window Settings", "ProfileCurrentVersion": 2.09, "CommandString": cmd,
           "RunCommandAsShell": True, "shellExitAction": SHELL_EXIT_CLOSE, "columnCount": cols, "rowCount": rows}
    with open(path, "wb") as f:
        plistlib.dump(doc, f, fmt=plistlib.FMT_XML)


def note_launched(run):
    """after the reference launch: if Terminal.app was not running at harness start, record its PID as `terminal <pid>` in <run>/pids
    (teardown.sh then handles a cancelled job). Never records a Terminal that was already running."""
    if _state["was_running"] or _state["pid"]:
        return
    pids = terminal_pids()
    if len(pids) == 1:
        _state["pid"] = pids[0]
        with open(os.path.join(run, "pids"), "a") as f:
            f.write("terminal %d\n" % pids[0])


def quit_if_ours():
    pid = _state["pid"]
    if _state["was_running"] or not pid:
        return
    _state["pid"] = None
    if pid not in terminal_pids():          # gone already, or the PID now belongs to something else
        return
    try:
        os.kill(pid, signal.SIGTERM)
    except OSError:
        return
    for _ in range(10):
        time.sleep(0.5)
        if pid not in terminal_pids():
            return
    try:
        os.kill(pid, signal.SIGKILL)
    except OSError:
        pass
