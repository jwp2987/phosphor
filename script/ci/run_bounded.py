#!/usr/bin/env python3
"""script/ci/run_bounded.py [--timeout-min N] [--grace S] -- CMD ARGS...   (#2148)

Run CMD with the job's stdio, then END THE PROCESS TREE IT STARTED when it returns (or when --timeout-min runs out), and exit with CMD's
status (124 on a timeout). Why: a leaky test child (nextest reports "LEAK") keeps the job's output pipe open after the tests are done, the
runner waits for that pipe, and the job sat for hours until it was cancelled. Only processes started by CMD are touched:
  * Windows: CMD is placed in a Job Object (KILL_ON_JOB_CLOSE); TerminateJobObject ends the whole tree, however the parent chain broke.
    If the job object cannot be used (already in a non-nestable job), `taskkill /T /F /PID <cmd pid>` of our own child is the fallback.
  * POSIX: CMD runs in its own session/process group; the group is signalled (TERM, then KILL after --grace seconds).
Every process ended after CMD returned is logged. `--self-test` runs offline (no network, no cargo).

  --done-regex RE --done-grace S   (#2148, r22c): CMD's output is read through a pipe and copied to our stdout. When a line matches RE (the
    suite's end-of-run line, "[agent-cargo] RESULT"), S seconds later every process in CMD's tree that is NOT a shell/coreutils tool of the
    script itself (bash, tee, sed, grep, ...) and not CMD is ended. Why: the r22c hang was NOT "CMD returned and something stayed": CMD
    (check_test_failures) never returned, because its `nextest | tee` pipeline waits for tee, tee waits for EOF, and a git.exe started by a test
    held the write end. Nothing "returned", so ending the tree on return never ran. Ending the leaker lets the pipeline finish normally and
    the script's own verdict and exit status survive.
"""
import os, re, subprocess, sys, threading, time

# Processes of the suite script's own pipeline. Never ended by the done-regex sweep (ending tee would lose the log).
KEEP = {"bash", "sh", "tee", "sed", "grep", "egrep", "awk", "gawk", "sort", "comm", "cat", "tr", "wc", "mktemp", "rm", "mv", "cp", "head",
        "tail", "cut", "uniq", "dirname", "basename", "date", "env", "cygpath", "ls", "printf", "echo", "test"}


def pick_victims(procs, root_pid, me, keep=KEEP):
    """procs: [(pid, image_path_or_command)]. Returns the (pid, name) to end: everything but CMD itself, us, and the script's own tools."""
    out = []
    for pid, img in procs:
        if pid in (root_pid, me):
            continue
        name = os.path.basename(img.strip().split(" ")[0] if "\\" not in img and "/" in img and " " in img.strip() else img.strip())
        name = name.replace("\\", "/").rsplit("/", 1)[-1].lower()
        if name.endswith(".exe"):
            name = name[:-4]
        if name in keep:
            continue
        out.append((pid, name))
    return out

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

    def resume(self, proc):
        # CREATE_SUSPENDED child: nothing has run, so every descendant is born inside the job (no window where a child escapes it).
        nt = self.ct.WinDLL("ntdll")
        nt.NtResumeProcess.argtypes = [self.wt.HANDLE]
        nt.NtResumeProcess(self.wt.HANDLE(int(proc._handle)))

    def members(self):
        ct, wt = self.ct, self.wt
        n = 4096

        class LIST(ct.Structure):
            _fields_ = [("Assigned", wt.DWORD), ("InList", wt.DWORD), ("Ids", ct.c_size_t * n)]
        info = LIST()
        self.k.QueryInformationJobObject.argtypes = [wt.HANDLE, ct.c_int, ct.c_void_p, wt.DWORD, ct.c_void_p]
        if not self.k.QueryInformationJobObject(self.job, 3, ct.byref(info), ct.sizeof(info), None):
            return []
        k = self.k
        k.OpenProcess.restype = wt.HANDLE
        res = []
        for i in range(min(info.InList, n)):
            pid = int(info.Ids[i])
            h = k.OpenProcess(0x1000, False, pid)      # PROCESS_QUERY_LIMITED_INFORMATION
            img = ""
            if h:
                buf = ct.create_unicode_buffer(1024)
                sz = wt.DWORD(1024)
                if k.QueryFullProcessImageNameW(wt.HANDLE(h), 0, buf, ct.byref(sz)):
                    img = buf.value
                k.CloseHandle(wt.HANDLE(h))
            res.append((pid, img or "?"))
        return res

    def kill_tree(self):
        self.k.TerminateJobObject(self.job, 1)


def _popen(cmd):
    """Returns (proc, kill_tree, members). members() -> [(pid, image)] of the tree. stdout+stderr go through a pipe we copy (see --done-regex)."""
    kw = dict(stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    if os.name == "nt":
        try:
            job = _Win()
            proc = subprocess.Popen(cmd, creationflags=0x4, **kw)    # CREATE_SUSPENDED; no CREATE_BREAKAWAY_FROM_JOB
        except Exception as e:      # noqa: BLE001
            _log("job object unavailable (%s): plain start, taskkill /T fallback" % e)
            proc = subprocess.Popen(cmd, **kw)
            return proc, (lambda: subprocess.call(["taskkill", "/PID", str(proc.pid), "/T", "/F"])), (lambda: [])
        try:
            job.adopt(proc)
            job.resume(proc)
            return proc, job.kill_tree, job.members
        except Exception as e:      # noqa: BLE001 - fall back to our own child's tree, by PID
            _log("job object unavailable (%s): falling back to taskkill /T of pid %d" % (e, proc.pid))
            try:
                job.resume(proc)
            except Exception:       # noqa: BLE001
                pass
            return proc, (lambda: subprocess.call(["taskkill", "/PID", str(proc.pid), "/T", "/F"])), (lambda: [])
    proc = subprocess.Popen(cmd, start_new_session=True, **kw)
    return proc, (lambda: None), (lambda: _group_members(proc.pid))


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


def _kill_pid(pid):
    if os.name == "nt":
        subprocess.call(["taskkill", "/PID", str(pid), "/T", "/F"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    else:
        import signal
        try:
            os.kill(pid, signal.SIGKILL)
        except OSError:
            pass


def _sweep_dir(dirpath):
    """Windows, last resort after the job is closed: end (by PID, logged) any process whose command line names this build dir."""
    if os.name != "nt":
        return
    me = os.getpid()
    # every process: pid, parent pid, does its command line name the build dir, name. The job's OWN chain (this script's ancestors: the bash
    # running run_suite.sh, the job's PowerShell, the runner's step script) also names the build dir and must never be ended (review 2026-11-12).
    ps = ("Get-CimInstance Win32_Process | ForEach-Object { '{0} {1} {2} {3}' -f $_.ProcessId, $_.ParentProcessId, "
          "[int]([bool]($_.CommandLine -and $_.CommandLine.Contains('%s'))), $_.Name }" % dirpath.replace("'", "''"))
    try:
        out = subprocess.run(["powershell", "-NoProfile", "-Command", ps], capture_output=True, text=True, timeout=60).stdout
    except Exception as e:      # noqa: BLE001
        _log("dir sweep skipped: %s" % e)
        return
    for pid, name in sweep_victims(out, me):
        _log("dir sweep: ending leftover pid %d (%s)" % (pid, name))
        _kill_pid(pid)


def sweep_victims(table, me):
    """pure: from 'pid ppid names_dir name' lines, the (pid, name) to end: names the build dir, and is neither `me` nor any ancestor of `me`."""
    parent, rows = {}, []
    for ln in table.splitlines():
        f = ln.split(None, 3)
        if len(f) >= 3 and f[0].isdigit() and f[1].isdigit():
            parent[int(f[0])] = int(f[1])
            rows.append((int(f[0]), f[2] == "1", f[3] if len(f) > 3 else ""))
    spare, p = set(), me
    while p and p not in spare:
        spare.add(p)
        p = parent.get(p)
    return [(pid, name) for pid, names_dir, name in rows if names_dir and pid not in spare]


def run(cmd, timeout_s=None, grace=5.0, done_re=None, done_grace=90.0):
    proc, kill_tree, members = _popen(cmd)
    done = threading.Event()
    rx = re.compile(done_re.encode()) if done_re else None

    def pump():
        out = sys.stdout.buffer
        for line in iter(proc.stdout.readline, b""):
            try:
                out.write(line)
                out.flush()
            except Exception:       # noqa: BLE001
                pass
            if rx and not done.is_set() and rx.search(line):
                done.set()
    t = threading.Thread(target=pump, daemon=True)
    t.start()
    start = time.time()
    done_at = None
    swept = False
    rc = None
    why = None
    while True:
        rc = proc.poll()
        if rc is not None:
            why = "command returned %d" % rc
            break
        now = time.time()
        if timeout_s is not None and now - start > timeout_s:
            rc = 124
            why = "TIMEOUT after %d s" % timeout_s
            _log(why + ": ending the command and its tree")
            break
        if done.is_set() and done_at is None:
            done_at = now
        if done_at is not None and not swept and now - done_at > done_grace:
            swept = True
            victims = pick_victims(members(), proc.pid, os.getpid())
            _log("end-of-run line seen %d s ago and the command has not returned: ending %d leftover process(es) holding its pipe" %
                 (done_grace, len(victims)))
            for pid, name in victims:
                _log("ending leftover pid %d (%s)" % (pid, name))
                _kill_pid(pid)
        if swept and now - done_at > done_grace + 60:
            rc = 125
            why = "command still did not return 60 s after the sweep"
            _log(why + ": ending its whole tree")
            break
        time.sleep(0.2)
    _end_tree(proc, kill_tree, grace, why)
    try:
        proc.wait(timeout=10)
    except subprocess.TimeoutExpired:
        pass
    t.join(timeout=10)
    _sweep_dir(os.getcwd())
    return rc


def self_test():
    import tempfile, textwrap
    # the dir sweep never ends this script or its ancestors (the job's own shell chain names the build dir too)
    tbl = "\n".join(["100 1 1 powershell.exe", "200 100 1 bash.exe", "300 200 0 python.exe", "400 300 1 git.exe", "500 1 1 git.exe", "600 1 0 msedge.exe"])
    assert sweep_victims(tbl, 300) == [(400, "git.exe"), (500, "git.exe")], sweep_victims(tbl, 300)
    assert sweep_victims("1 0 1 x\n2 1 1 y", 2) == [], "a chain up to the root is spared"
    print("ok   dir sweep spares run_bounded's own ancestors (the job's shell chain)")
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
    # pick_victims is pure: msys tools of the script stay, test children go (both path styles)
    v = pick_victims([(10, "C:\\Git\\usr\\bin\\tee.exe"), (11, "C:\\Git\\mingw64\\bin\\git.exe"), (12, "/usr/bin/bash"), (13, "/x/leak"), (14, "C:\\Git\\usr\\bin\\bash.exe")], 12, 99)
    assert sorted(v) == [(11, "git"), (13, "leak")], v
    # the r22c shape: CMD never returns because its `| tee`-like pipeline waits on a leaker that holds the pipe AFTER the end-of-run line
    import shutil
    if os.name != "nt" and shutil.which("bash") and shutil.which("cat"):
        script = "{ ( %s -c 'import time; time.sleep(600)' & ) ; echo '[agent-cargo] RESULT ok'; } | cat; echo after-pipeline; exit 5" % py
        t0 = time.time()
        rc = run(["bash", "-c", script], timeout_s=60, grace=1, done_re=r"\[agent-cargo\] RESULT", done_grace=1)
        assert rc == 5, "the script's own exit status must survive the sweep, got %r" % rc
        assert time.time() - t0 < 30, "the pipeline stayed blocked on the leaker"
    # structure of the Windows path (cannot run here): suspended start, kill-on-close, no breakaway, members via the job
    src = open(__file__).read()
    for needle in ("creationflags=0x4", "0x2000", "NtResumeProcess", "QueryInformationJobObject", "TerminateJobObject"):
        assert needle in src, needle
    print("run_bounded self-test ok (pipe closed after %.1f s)" % dt)
    return 0


def main(argv):
    if "--self-test" in argv:
        return self_test()
    timeout_s, grace, i, done_re, done_grace = None, 5.0, 0, None, 90.0
    while i < len(argv) and argv[i] != "--":
        if argv[i] == "--timeout-min":
            timeout_s = int(float(argv[i + 1]) * 60); i += 2
        elif argv[i] == "--grace":
            grace = float(argv[i + 1]); i += 2
        elif argv[i] == "--done-regex":
            done_re = argv[i + 1]; i += 2
        elif argv[i] == "--done-grace":
            done_grace = float(argv[i + 1]); i += 2
        else:
            print("usage: run_bounded.py [--timeout-min N] [--grace S] -- CMD...", file=sys.stderr); return 2
    cmd = argv[i + 1:]
    if not cmd:
        print("usage: run_bounded.py [--timeout-min N] [--grace S] -- CMD...", file=sys.stderr); return 2
    return run(cmd, timeout_s, grace, done_re, done_grace)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
