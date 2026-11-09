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


def pick_ours(pids, pre_pid, started, t0, slack=1.0):
    """pure (#1826): which of `pids` (running Terminal.app PIDs) did THIS run launch? Never `pre_pid` (the Terminal that was running at harness start), and only one
    that STARTED at or after the harness did (`started` maps pid -> epoch s, lstart has 1 s resolution: `slack`). A single match is ours; zero or several are
    ambiguous and nothing is returned (we never guess which of two Terminals to quit)."""
    if t0 is None:
        return None
    late = [p for p in pids if p != pre_pid and (started.get(p) or 0) >= t0 - slack]
    return late[0] if len(late) == 1 else None


def _wait_gone(pid, tries=10):
    for _ in range(tries):
        time.sleep(0.5)
        if pid not in terminal_pids():
            return True
    return False


def _quit_pid(pid, why):
    """SIGTERM then SIGKILL one Terminal.app PID we own; True when it is gone afterwards"""
    if pid not in terminal_pids():          # gone already, or the PID now belongs to something else
        print("teardown: Terminal.app reference pid %s already gone (its windows closed with it)" % pid, flush=True)
        return True
    try:
        os.kill(pid, signal.SIGTERM)
    except OSError:
        print("teardown: Terminal.app reference pid %s could not be signalled" % pid, flush=True)
        return False
    if _wait_gone(pid):
        print("teardown: Terminal.app reference pid %s quit by SIGTERM (%s; all its reference windows closed)" % (pid, why), flush=True)
        return True
    try:
        os.kill(pid, signal.SIGKILL)
        print("teardown: Terminal.app reference pid %s needed SIGKILL (%s; windows closed with the process)" % (pid, why), flush=True)
    except OSError:
        pass
    return _wait_gone(pid, 6)


def _ours_pid():
    pid = _state["pid"]
    if not pid and _state["t0"] is not None and (not _state["was_running"] or _state["pre_pid"] not in terminal_pids()):
        # `open -a Terminal` returns before the process exists, so note_launched can miss it (r18ck left one running): any Terminal.app
        # that STARTED after this harness did was launched by this run's reference (lstart has 1 s resolution: allow it)
        pids = terminal_pids()
        pid = pick_ours(pids, _state["pre_pid"], {p: _started_epoch(p) for p in pids}, _state["t0"])
    return pid


def quit_launched(why="end of the reference block"):
    """#1826: at the end of EACH reference block quit the Terminal.app this run launched, by exact PID, so its window(s) (Terminal keeps the window of an exited shell
    open) do not pile up one per probe. A Terminal.app that was already running is never touched. Returns True when none of ours is left. The next reference launch
    starts a fresh Terminal.app (recorded again by note_launched)."""
    pid = _ours_pid()
    if not pid:
        return True
    _state["pid"] = None
    return _quit_pid(pid, why)


def left_windows():
    """#1826: PIDs of Terminal.app instances this run launched that are still running (their windows are still open); [] when none"""
    pid = _ours_pid()
    return [pid] if pid and pid in terminal_pids() else []


def quit_if_ours():
    pid = _ours_pid()
    if not pid:
        print("teardown: Terminal.app reference: none launched by this run (%s)" % ("a Terminal.app that was already running is left open" if _state["was_running"] else "no reference window was opened"), flush=True)
        return
    _state["pid"] = None
    _quit_pid(pid, "teardown")
    left = left_windows()
    print("teardown: Terminal.app reference windows left open by this run: %s" % ("NONE" if not left else "FAIL, pid %s still running" % left[0]), flush=True)
