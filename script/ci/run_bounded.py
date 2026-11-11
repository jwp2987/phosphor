#!/usr/bin/env python3
"""script/ci/run_bounded.py [--timeout-min N] [--grace S] -- CMD ARGS...   (#2148)

Run CMD with the job's stdio, then END THE PROCESS TREE IT STARTED when it returns (or when --timeout-min runs out), and exit with CMD's
status (124 on a timeout). Why: a leaky test child (nextest reports "LEAK") keeps the job's output pipe open after the tests are done, the
runner waits for that pipe, and the job sat for hours until it was cancelled. Only processes started by CMD are touched:
  * Windows: CMD is placed in a Job Object (KILL_ON_JOB_CLOSE); TerminateJobObject ends the whole tree, however the parent chain broke.
    If the job object cannot be used (already in a non-nestable job), `taskkill /T /F /PID <cmd pid>` of our own child is the fallback.
  * POSIX: CMD runs in its own session/process group; the group is signalled (TERM, then KILL after --grace seconds).
Every process ended after CMD returned is logged. `--self-test` runs offline (no network, no cargo).
"""
import os, subprocess, sys, time

LOG = "run_bounded:"


def _log(msg):
    print("%s %s" % (LOG, msg), flush=True)


class _Win:
    def __init__(self):
        import ctypes
        from ctypes import wintypes as wt
        self.ct, self.wt = ctypes, wt
        k = self.k = ctypes.WinDLL("kernel32", use_last_error=True)
        k.CreateJobObjectW.restype = wt.HANDLE
        k.AssignProcessToJobObject.argtypes = [wt.HANDLE, wt.HANDLE]
        k.TerminateJobObject.argtypes = [wt.HANDLE, wt.UINT]
        k.SetInformationJobObject.argtypes = [wt.HANDLE, ctypes.c_int, ctypes.c_void_p, wt.DWORD]

        class BASIC(ctypes.Structure):
            _fields_ = [("PerProcessUserTimeLimit", ctypes.c_int64), ("PerJobUserTimeLimit", ctypes.c_int64), ("LimitFlags", wt.DWORD),
                        ("MinimumWorkingSetSize", ctypes.c_size_t), ("MaximumWorkingSetSize", ctypes.c_size_t),
                        ("ActiveProcessLimit", wt.DWORD), ("Affinity", ctypes.c_size_t), ("PriorityClass", wt.DWORD), ("SchedulingClass", wt.DWORD)]

        class IO(ctypes.Structure):
            _fields_ = [(n, ctypes.c_uint64) for n in ("r", "w", "o", "rb", "wb", "ob")]

        class EXT(ctypes.Structure):
            _fields_ = [("Basic", BASIC), ("Io", IO), ("ProcessMemoryLimit", ctypes.c_size_t), ("JobMemoryLimit", ctypes.c_size_t),
                        ("PeakProcessMemoryUsed", ctypes.c_size_t), ("PeakJobMemoryUsed", ctypes.c_size_t)]
        self.job = k.CreateJobObjectW(None, None)
        info = EXT()
        info.Basic.LimitFlags = 0x2000          # JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        if not self.job or not k.SetInformationJobObject(self.job, 9, ctypes.byref(info), ctypes.sizeof(info)):
            raise OSError("job object: error %d" % ctypes.get_last_error())

    def adopt(self, proc):
        if not self.k.AssignProcessToJobObject(self.job, self.wt.HANDLE(int(proc._handle))):
            raise OSError("AssignProcessToJobObject: error %d" % self.ct.get_last_error())

    def kill_tree(self):
        self.k.TerminateJobObject(self.job, 1)


def _popen(cmd):
    if os.name == "nt":
        proc = subprocess.Popen(cmd)
        try:
            job = _Win()
            job.adopt(proc)
            return proc, job.kill_tree
        except Exception as e:      # noqa: BLE001 - fall back to our own child's tree, by PID
            _log("job object unavailable (%s): falling back to taskkill /T of pid %d" % (e, proc.pid))
            return proc, lambda: subprocess.call(["taskkill", "/PID", str(proc.pid), "/T", "/F"])
    proc = subprocess.Popen(cmd, start_new_session=True)
    return proc, lambda: None


def _group_members(pgid):
    out = subprocess.run(["ps", "-axo", "pid=,pgid=,command="], capture_output=True, text=True).stdout
    mem = []
    for ln in out.splitlines():
        f = ln.split(None, 2)
        if len(f) >= 2 and f[1] == str(pgid) and f[0] != str(os.getpid()):
            mem.append((int(f[0]), f[2][:140] if len(f) > 2 else ""))
    return mem


def _end_tree(proc, kill_tree, grace, why):
    if os.name == "nt":
        _log("%s: ending the process tree of pid %d (job object)" % (why, proc.pid))
        kill_tree()
        return
    import signal
    left = [(p, c) for p, c in _group_members(proc.pid) if p != proc.pid or proc.poll() is None]
    if not left:
        return
    for p, c in left:
        _log("%s: ending leftover pid %d (%s)" % (why, p, c))
    try:
        os.killpg(proc.pid, signal.SIGTERM)
    except OSError:
        pass
    end = time.time() + grace
    while time.time() < end and _group_members(proc.pid):
        time.sleep(0.2)
    for p, c in _group_members(proc.pid):
        _log("%s: pid %d needed SIGKILL" % (why, p))
    try:
        os.killpg(proc.pid, signal.SIGKILL)
    except OSError:
        pass


def run(cmd, timeout_s=None, grace=5.0):
    proc, kill_tree = _popen(cmd)
    rc = None
    try:
        rc = proc.wait(timeout=timeout_s)
        why = "command returned %d" % rc
    except subprocess.TimeoutExpired:
        rc = 124
        why = "TIMEOUT after %d s" % timeout_s
        _log(why + ": ending the command and its tree")
    except KeyboardInterrupt:
        rc = 130
        why = "interrupted"
    _end_tree(proc, kill_tree, grace, why)
    try:
        proc.wait(timeout=10)
    except subprocess.TimeoutExpired:
        pass
    return rc


def self_test():
    import tempfile, textwrap
    py = sys.executable
    d = tempfile.mkdtemp(prefix="rb-")
    leaker = os.path.join(d, "leaker.py")
    # parent exits 7 at once after starting a grandchild that keeps the inherited stdout open for 10 minutes
    open(leaker, "w").write(textwrap.dedent("""
        import subprocess, sys
        subprocess.Popen([sys.executable, "-c", "import time; time.sleep(600)"])
        sys.exit(7)
    """))
    # a reader on the pipe sees EOF only when EVERY writer is gone: this is what the runner does with the job's output
    reader = subprocess.Popen([py, __file__, "--"] + [py, leaker], stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    t0 = time.time()
    data = reader.stdout.read()      # blocks forever if the grandchild kept the pipe
    rc = reader.wait()
    dt = time.time() - t0
    assert rc == 7, "exit status of the command must pass through, got %r" % rc
    assert dt < 30, "the pipe stayed open %.0f s: the leaked child was not ended" % dt
    assert b"ending" in data or os.name == "nt", data
    # control (POSIX): without run_bounded the same leak DOES hold the pipe (proves the test can fail)
    if os.name != "nt":
        import select, signal
        ctl = subprocess.Popen([py, leaker], stdout=subprocess.PIPE, start_new_session=True)
        time.sleep(1.5)
        held = not select.select([ctl.stdout], [], [], 0.5)[0]
        os.killpg(ctl.pid, signal.SIGKILL)
        ctl.wait()
        assert held, "control: the leak should keep the pipe open"
    # timeout path: a command that sleeps past --timeout is ended and reports 124
    t0 = time.time()
    rc = run([py, "-c", "import time; time.sleep(600)"], timeout_s=2, grace=1)
    assert rc == 124 and time.time() - t0 < 30, rc
    print("run_bounded self-test ok (pipe closed after %.1f s)" % dt)
    return 0


def main(argv):
    if "--self-test" in argv:
        return self_test()
    timeout_s, grace, i = None, 5.0, 0
    while i < len(argv) and argv[i] != "--":
        if argv[i] == "--timeout-min":
            timeout_s = int(float(argv[i + 1]) * 60); i += 2
        elif argv[i] == "--grace":
            grace = float(argv[i + 1]); i += 2
        else:
            print("usage: run_bounded.py [--timeout-min N] [--grace S] -- CMD...", file=sys.stderr); return 2
    cmd = argv[i + 1:]
    if not cmd:
        print("usage: run_bounded.py [--timeout-min N] [--grace S] -- CMD...", file=sys.stderr); return 2
    return run(cmd, timeout_s, grace)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
