"""macOS reference terminal lifecycle (#1485): a Terminal.app that THIS run launched is quit at the end by its exact PID (SIGTERM, then
SIGKILL), which closes every reference window (the account has no saved state). A Terminal.app that was already running (someone's
windows) is never quit. No Apple Events, no Terminal preference changes (no .terminal profile import), no pgrep/pkill: PIDs come from
`ps -A -o pid=,comm=`."""
import atexit, os, signal, subprocess, time

TERMINAL_PATH = "/System/Applications/Utilities/Terminal.app/Contents/MacOS/Terminal"

_state = {"snapshotted": False, "was_running": False, "pre_pid": None, "pid": None, "run": None, "t0": None}


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
    pre = terminal_pids()
    _state.update(snapshotted=True, was_running=bool(pre), pre_pid=pre[0] if pre else None, run=run, t0=time.time())
    atexit.register(quit_if_ours)


def left_open_note():
    if _state["was_running"]:
        return "reference windows left open: Terminal was already running (pid %s)" % _state["pre_pid"]
    return None


def note_launched(run):
    """after the reference launch: if Terminal.app was not running at harness start, record its PID as `terminal <pid>` in <run>/pids
    (teardown.sh then handles a cancelled job). Never records a Terminal that was already running."""
    if _state["pid"]:
        return
    pids = terminal_pids()
    if _state["was_running"]:
        if _state["pre_pid"] in pids:
            return
        # the Terminal that was running at harness start is gone (quit during the run): the one running now was launched by this run
        _state["was_running"] = False
    if len(pids) == 1:
        _state["pid"] = pids[0]
        with open(os.path.join(run, "pids"), "a") as f:
            f.write("terminal %d\n" % pids[0])


def _started_epoch(pid):
    """process start time (epoch s) from `ps -o lstart=`, or None"""
    try:
        out = subprocess.run(["ps", "-o", "lstart=", "-p", str(pid)], capture_output=True, text=True, timeout=5,
                             env=dict(os.environ, LC_ALL="C")).stdout.strip()
        return time.mktime(time.strptime(out, "%a %b %d %H:%M:%S %Y")) if out else None
    except Exception:
        return None


def quit_if_ours():
    pid = _state["pid"]
    if not pid and _state["t0"] is not None and (not _state["was_running"] or _state["pre_pid"] not in terminal_pids()):
        # `open -a Terminal` returns before the process exists, so note_launched can miss it (r18ck left one running): any Terminal.app
        # that STARTED after this harness did was launched by this run's reference (lstart has 1 s resolution: allow it)
        late = [p for p in terminal_pids() if (_started_epoch(p) or 0) >= _state["t0"] - 1 and p != _state["pre_pid"]]
        pid = late[0] if len(late) == 1 else None
    if not pid:
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
