# appci/winapi.py: the few Win32 calls the Windows backend, the Windows app launcher and the intl cases share (ctypes only; importing this
# module on another OS is harmless, calling anything in it is not). Handles are declared as c_void_p everywhere: on x64 the default
# c_int conversion truncates them.
import ctypes
import os
from ctypes import wintypes as wt

if os.name == "nt":
    user32 = ctypes.WinDLL("user32", use_last_error=True)
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    psapi = ctypes.WinDLL("psapi", use_last_error=True)

    user32.GetForegroundWindow.restype = ctypes.c_void_p
    user32.GetWindowThreadProcessId.argtypes = [ctypes.c_void_p, ctypes.POINTER(wt.DWORD)]
    user32.GetWindowThreadProcessId.restype = wt.DWORD
    user32.IsWindowVisible.argtypes = [ctypes.c_void_p]
    user32.IsIconic.argtypes = [ctypes.c_void_p]
    user32.GetWindow.argtypes = [ctypes.c_void_p, wt.UINT]
    user32.GetWindow.restype = ctypes.c_void_p
    user32.GetWindowRect.argtypes = [ctypes.c_void_p, ctypes.POINTER(wt.RECT)]
    user32.ShowWindow.argtypes = [ctypes.c_void_p, ctypes.c_int]
    user32.SetForegroundWindow.argtypes = [ctypes.c_void_p]
    user32.BringWindowToTop.argtypes = [ctypes.c_void_p]
    user32.AttachThreadInput.argtypes = [wt.DWORD, wt.DWORD, wt.BOOL]
    user32.GetWindowTextW.argtypes = [ctypes.c_void_p, wt.LPWSTR, ctypes.c_int]
    user32.GetClassNameW.argtypes = [ctypes.c_void_p, wt.LPWSTR, ctypes.c_int]
    user32.PostMessageW.argtypes = [ctypes.c_void_p, wt.UINT, ctypes.c_size_t, ctypes.c_ssize_t]
    kernel32.OpenProcess.argtypes = [wt.DWORD, wt.BOOL, wt.DWORD]
    kernel32.OpenProcess.restype = ctypes.c_void_p
    kernel32.CloseHandle.argtypes = [ctypes.c_void_p]
    kernel32.QueryFullProcessImageNameW.argtypes = [ctypes.c_void_p, wt.DWORD, wt.LPWSTR, ctypes.POINTER(wt.DWORD)]
    kernel32.GetProcessTimes.argtypes = [ctypes.c_void_p, ctypes.POINTER(wt.FILETIME), ctypes.POINTER(wt.FILETIME), ctypes.POINTER(wt.FILETIME), ctypes.POINTER(wt.FILETIME)]
    kernel32.GetCurrentThreadId.restype = wt.DWORD

    WNDENUMPROC = ctypes.WINFUNCTYPE(wt.BOOL, ctypes.c_void_p, ctypes.c_void_p)


# --- #1642: a Job Object with KILL_ON_JOB_CLOSE around the app under test. taskkill /T walks the parent chain, which conhost (parent: the
# app, but re-parented once it exits) and anything the app's shells detach escape; a job holds the whole tree whatever its parentage, and
# closing the handle (stop(), or the harness dying) kills every member, so none is left holding app.stdout. Every function takes the
# kernel32 object so the selftest can drive them on Linux with a mock.
JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE = 0x2000
JobObjectBasicProcessIdList = 3
JobObjectExtendedLimitInformation = 9
CREATE_SUSPENDED = 0x00000004
PROCESS_ALL_ACCESS_FOR_JOB = 0x1F0FFF


class _BasicLimit(ctypes.Structure):
    _fields_ = [("PerProcessUserTimeLimit", ctypes.c_int64), ("PerJobUserTimeLimit", ctypes.c_int64), ("LimitFlags", ctypes.c_uint32),
                ("MinimumWorkingSetSize", ctypes.c_size_t), ("MaximumWorkingSetSize", ctypes.c_size_t), ("ActiveProcessLimit", ctypes.c_uint32),
                ("Affinity", ctypes.c_size_t), ("PriorityClass", ctypes.c_uint32), ("SchedulingClass", ctypes.c_uint32)]


class _IoCounters(ctypes.Structure):
    _fields_ = [(n, ctypes.c_uint64) for n in ("ReadOps", "WriteOps", "OtherOps", "ReadBytes", "WriteBytes", "OtherBytes")]


class ExtendedLimit(ctypes.Structure):
    _fields_ = [("BasicLimitInformation", _BasicLimit), ("IoInfo", _IoCounters), ("ProcessMemoryLimit", ctypes.c_size_t),
                ("JobMemoryLimit", ctypes.c_size_t), ("PeakProcessMemoryUsed", ctypes.c_size_t), ("PeakJobMemoryUsed", ctypes.c_size_t)]


def job_create(k32=None):
    """a new anonymous job whose handle closing kills all its processes; returns the handle, or None when the job could not be made"""
    k32 = k32 or kernel32
    k32.CreateJobObjectW.restype = ctypes.c_void_p
    job = k32.CreateJobObjectW(None, None)
    if not job:
        return None
    info = ExtendedLimit()
    info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
    k32.SetInformationJobObject.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_void_p, wt.DWORD]
    if not k32.SetInformationJobObject(job, JobObjectExtendedLimitInformation, ctypes.byref(info), ctypes.sizeof(info)):
        k32.CloseHandle(job)
        return None
    return job


def job_assign(job, process_handle, k32=None):
    k32 = k32 or kernel32
    k32.AssignProcessToJobObject.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
    return bool(k32.AssignProcessToJobObject(job, process_handle))


def resume_process(process_handle, ntdll=None):
    """resume every thread of a CREATE_SUSPENDED process (Popen does not hand back the primary thread's handle)"""
    ntdll = ntdll or ctypes.WinDLL("ntdll")
    ntdll.NtResumeProcess.argtypes = [ctypes.c_void_p]
    return ntdll.NtResumeProcess(process_handle) == 0


def job_pids(job, k32=None, cap=512):
    """PIDs currently in the job (empty on any failure)"""
    k32 = k32 or kernel32
    class _Ids(ctypes.Structure):
        _fields_ = [("NumberOfAssignedProcesses", wt.DWORD), ("NumberOfProcessIdsInList", wt.DWORD), ("ProcessIdList", ctypes.c_size_t * cap)]
    buf = _Ids()
    k32.QueryInformationJobObject.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_void_p, wt.DWORD, ctypes.c_void_p]
    if not k32.QueryInformationJobObject(job, JobObjectBasicProcessIdList, ctypes.byref(buf), ctypes.sizeof(buf), None):
        return []
    return [int(buf.ProcessIdList[i]) for i in range(min(buf.NumberOfProcessIdsInList, cap))]


def job_close(job, k32=None):
    """closing the last handle kills every process still in the job"""
    k32 = k32 or kernel32
    k32.CloseHandle.argtypes = [ctypes.c_void_p]
    return bool(k32.CloseHandle(job))


def process_creation_filetime(pid, k32=None):
    """the creation time (FILETIME, 100 ns ticks) of a live pid, or None; after_job.ps1 uses it to tell a recorded process from a reused PID"""
    k32 = k32 or kernel32
    h = k32.OpenProcess(0x1000, False, pid)         # PROCESS_QUERY_LIMITED_INFORMATION
    if not h:
        return None
    try:
        c, e, kt, ut = wt.FILETIME(), wt.FILETIME(), wt.FILETIME(), wt.FILETIME()
        if not k32.GetProcessTimes(h, ctypes.byref(c), ctypes.byref(e), ctypes.byref(kt), ctypes.byref(ut)):
            return None
        return (c.dwHighDateTime << 32) | c.dwLowDateTime
    finally:
        k32.CloseHandle(h)


def foreground():
    """(hwnd, pid) of the foreground window; (None, 0) when there is none (locked desktop, UAC prompt)"""
    h = user32.GetForegroundWindow()
    pid = wt.DWORD(0)
    if h:
        user32.GetWindowThreadProcessId(h, ctypes.byref(pid))
    return h, pid.value


def window_pid(h):
    pid = wt.DWORD(0)
    user32.GetWindowThreadProcessId(h, ctypes.byref(pid))
    return pid.value


def proc_name(pid):
    """lower-case image file name of a process ('conhost.exe'), '' when it cannot be opened"""
    if not pid:
        return ""
    h = kernel32.OpenProcess(0x1000, False, pid)         # PROCESS_QUERY_LIMITED_INFORMATION
    if not h:
        return ""
    try:
        buf = ctypes.create_unicode_buffer(1024)
        n = wt.DWORD(1024)
        if kernel32.QueryFullProcessImageNameW(h, 0, buf, ctypes.byref(n)):
            return os.path.basename(buf.value).lower()
        return ""
    finally:
        kernel32.CloseHandle(h)


def top_windows(pid=None, visible=True):
    """[(hwnd, pid, (x, y, w, h), title)] of top-level windows (owned popups excluded), optionally of one process"""
    out = []

    def cb(h, _):
        if visible and not user32.IsWindowVisible(h):
            return True
        if user32.GetWindow(h, 4):                     # GW_OWNER: an owned popup/tool window, not a main window
            return True
        p = window_pid(h)
        if pid is not None and p != pid:
            return True
        r = wt.RECT()
        if not user32.GetWindowRect(h, ctypes.byref(r)):
            return True
        t = ctypes.create_unicode_buffer(256)
        user32.GetWindowTextW(h, t, 256)
        out.append((h, p, (r.left, r.top, r.right - r.left, r.bottom - r.top), t.value))
        return True
    user32.EnumWindows(WNDENUMPROC(cb), 0)
    return out


def main_window(pid):
    """the biggest visible top-level window of `pid`, or None"""
    ws = [w for w in top_windows(pid) if w[2][2] > 50 and w[2][3] > 50]
    return max(ws, key=lambda w: w[2][2] * w[2][3])[0] if ws else None


def focus_window(h):
    """bring `h` to the foreground WITHOUT sending a key: restore if minimised, attach to the foreground thread's input queue, SetForegroundWindow"""
    if user32.IsIconic(h):
        user32.ShowWindow(h, 9)                        # SW_RESTORE
    fg = user32.GetForegroundWindow()
    cur = kernel32.GetCurrentThreadId()
    t_fg = user32.GetWindowThreadProcessId(fg, None) if fg else 0
    t_tgt = user32.GetWindowThreadProcessId(h, None)
    attached = []
    for t in {t_fg, t_tgt}:
        if t and t != cur and user32.AttachThreadInput(cur, t, True):
            attached.append(t)
    try:
        user32.BringWindowToTop(h)
        user32.SetForegroundWindow(h)
    finally:
        for t in attached:
            user32.AttachThreadInput(cur, t, False)


class PMCEX(ctypes.Structure):
    """PROCESS_MEMORY_COUNTERS_EX: PMC plus PrivateUsage (private committed bytes: the heap and other non-shared memory, the figure the app's own
    memory_footprint reports on Windows)"""
    _fields_ = [("cb", wt.DWORD), ("PageFaultCount", wt.DWORD), ("PeakWorkingSetSize", ctypes.c_size_t), ("WorkingSetSize", ctypes.c_size_t),
                ("QuotaPeakPagedPoolUsage", ctypes.c_size_t), ("QuotaPagedPoolUsage", ctypes.c_size_t),
                ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t), ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
                ("PagefileUsage", ctypes.c_size_t), ("PeakPagefileUsage", ctypes.c_size_t), ("PrivateUsage", ctypes.c_size_t)]


def _memory_counters(h):
    """(working_set_kib, private_kib) of an open process handle; (0, 0) when GetProcessMemoryInfo fails"""
    pm = PMCEX()
    pm.cb = ctypes.sizeof(PMCEX)
    psapi.GetProcessMemoryInfo.argtypes = [ctypes.c_void_p, ctypes.POINTER(PMCEX), wt.DWORD]
    if not psapi.GetProcessMemoryInfo(h, ctypes.byref(pm), pm.cb):
        return 0, 0
    return pm.WorkingSetSize // 1024, pm.PrivateUsage // 1024


def process_times(pid):
    """(cpu_100ns, working_set_kib) of one process; raises OSError when it cannot be opened"""
    return process_times_private(pid)[:2]


def process_times_private(pid):
    """(cpu_100ns, working_set_kib, private_kib) of one process; raises OSError when it cannot be opened"""
    h = kernel32.OpenProcess(0x1000, False, pid)
    if not h:
        raise OSError("OpenProcess(%d) failed: %d" % (pid, ctypes.get_last_error()))
    try:
        c, e, k, u = wt.FILETIME(), wt.FILETIME(), wt.FILETIME(), wt.FILETIME()
        if not kernel32.GetProcessTimes(h, ctypes.byref(c), ctypes.byref(e), ctypes.byref(k), ctypes.byref(u)):
            raise OSError("GetProcessTimes failed: %d" % ctypes.get_last_error())
        cpu = ((k.dwHighDateTime << 32) | k.dwLowDateTime) + ((u.dwHighDateTime << 32) | u.dwLowDateTime)
        ws, private = _memory_counters(h)
        return cpu, ws, private
    finally:
        kernel32.CloseHandle(h)


def precise_time():
    """seconds since the epoch from GetSystemTimePreciseAsFileTime (sub-microsecond; time.time() on CPython 3.12/Windows ticks at ~15.6 ms)"""
    ft = wt.FILETIME()
    kernel32.GetSystemTimePreciseAsFileTime(ctypes.byref(ft))
    return (((ft.dwHighDateTime << 32) | ft.dwLowDateTime) - 116444736000000000) / 1e7


def profile_dir(environ=None):
    """The user's profile folder as the app under test resolves its home: Rust's dirs::home_dir() is SHGetKnownFolderPath(FOLDERID_Profile) on Windows
    and never reads HOME (the Git Bash pane's scratch HOME does not move it). cases_agent_mcp.py puts the app's global MCP config under it (#2065).
    Falls back to %USERPROFILE% (the same folder for an ordinary account) when the call is unavailable, which is also the answer off Windows (self-test)."""
    environ = os.environ if environ is None else environ
    if os.name == "nt":
        try:
            class GUID(ctypes.Structure):
                _fields_ = [("d1", wt.DWORD), ("d2", wt.WORD), ("d3", wt.WORD), ("d4", ctypes.c_ubyte * 8)]
            folder = GUID(0x5E6C858F, 0x0E22, 0x4760, (ctypes.c_ubyte * 8)(0x9A, 0xFE, 0xEA, 0x33, 0x17, 0xB6, 0x71, 0x73))      # FOLDERID_Profile
            shell32, ole32 = ctypes.WinDLL("shell32"), ctypes.WinDLL("ole32")
            shell32.SHGetKnownFolderPath.argtypes = [ctypes.POINTER(GUID), wt.DWORD, wt.HANDLE, ctypes.POINTER(ctypes.c_void_p)]
            shell32.SHGetKnownFolderPath.restype = ctypes.c_long
            ole32.CoTaskMemFree.argtypes = [ctypes.c_void_p]
            ole32.CoTaskMemFree.restype = None
            out = ctypes.c_void_p()
            if shell32.SHGetKnownFolderPath(ctypes.byref(folder), 0, None, ctypes.byref(out)) == 0 and out.value:
                try:
                    path = ctypes.wstring_at(out.value)
                finally:
                    ole32.CoTaskMemFree(out)
                if path:
                    return path
        except Exception:  # noqa: BLE001  (no shell32 / an odd session: the environment's answer below)
            pass
    return environ.get("USERPROFILE") or os.path.expanduser("~")


def git_bash():
    """path of Git for Windows' bash.exe (never System32\\bash.exe, which is WSL), or None"""
    cands = []
    for var in ("ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"):
        base = os.environ.get(var)
        if base:
            cands.append(os.path.join(base, "Git", "bin", "bash.exe") if var != "LOCALAPPDATA" else os.path.join(base, "Programs", "Git", "bin", "bash.exe"))
    for c in cands:
        if os.path.isfile(c):
            return c
    return None


def git_dirs():
    """[Git\\bin, Git\\usr\\bin] of the Git for Windows install (empty without one): less, vim, md5sum, seq, cat, ... live there"""
    b = git_bash()
    if not b:
        return []
    root = os.path.dirname(os.path.dirname(b))
    return [d for d in (os.path.join(root, "bin"), os.path.join(root, "usr", "bin")) if os.path.isdir(d)]


def ctrl_c_ignored():
    """#1570: whether THIS process ignores Ctrl-C (CONSOLE_IGNORE_CTRL_C in its process parameters: set by CREATE_NEW_PROCESS_GROUP or
    SetConsoleCtrlHandler(NULL, TRUE), and inherited by every child, so a reference terminal or an app this harness starts inherits it).
    True/False, or None when it cannot be read. Read from the PEB (no API returns it)."""
    try:
        ntdll = ctypes.WinDLL("ntdll")

        class PBI(ctypes.Structure):
            _fields_ = [("ExitStatus", ctypes.c_void_p), ("PebBaseAddress", ctypes.c_void_p), ("AffinityMask", ctypes.c_void_p),
                        ("BasePriority", ctypes.c_void_p), ("UniqueProcessId", ctypes.c_void_p), ("InheritedFromUniqueProcessId", ctypes.c_void_p)]
        kernel32.GetCurrentProcess.restype = ctypes.c_void_p
        ntdll.NtQueryInformationProcess.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_void_p, wt.ULONG, ctypes.c_void_p]
        pbi = PBI()
        if ntdll.NtQueryInformationProcess(kernel32.GetCurrentProcess(), 0, ctypes.byref(pbi), ctypes.sizeof(pbi), None) != 0 or not pbi.PebBaseAddress:
            return None
        wide = ctypes.sizeof(ctypes.c_void_p) == 8
        params = ctypes.c_void_p.from_address(pbi.PebBaseAddress + (0x20 if wide else 0x10)).value
        if not params:
            return None
        return bool(ctypes.c_uint32.from_address(params + (0x18 if wide else 0x14)).value & 1)
    except (OSError, AttributeError, ValueError, NameError):        # NameError: off Windows `kernel32` is not defined
        return None


def set_ctrl_c_ignored(ignored):
    """SetConsoleCtrlHandler(NULL, ignored): sets or clears this process's ignore-Ctrl-C flag (children started afterwards inherit it). -> success"""
    try:
        kernel32.SetConsoleCtrlHandler.argtypes = [ctypes.c_void_p, wt.BOOL]
        return bool(kernel32.SetConsoleCtrlHandler(None, bool(ignored)))
    except (OSError, AttributeError, NameError):
        return False


def pwsh7():
    """path of PowerShell 7's pwsh.exe (PATH, then Program Files\\PowerShell\\7), or None. Windows PowerShell 5.1 (powershell.exe) is never returned."""
    import shutil
    p = shutil.which("pwsh")
    if p:
        return p
    for var in ("ProgramFiles", "ProgramW6432"):
        base = os.environ.get(var)
        if base:
            c = os.path.join(base, "PowerShell", "7", "pwsh.exe")
            if os.path.isfile(c):
                return c
    return None


def cmd_exe():
    """path of cmd.exe (%ComSpec%, else System32\\cmd.exe), or None"""
    c = os.environ.get("ComSpec") or os.path.join(os.environ.get("SystemRoot", r"C:\\Windows"), "System32", "cmd.exe")
    return c if os.path.isfile(c) else None


def parse_wsl_list(raw):
    """pure: distribution names from the bytes `wsl.exe --list --quiet` prints (UTF-16LE, sometimes with NULs left after a lossy decode, one name per
    line; the default distribution is first). Lines that are not a plain name (the "no installed distributions" text, usage text) give []."""
    if not raw:
        return []
    if isinstance(raw, bytes):
        try:
            text = raw.decode("utf-16-le") if raw.count(b"\0") * 3 >= len(raw) else raw.decode("utf-8", "replace")
        except UnicodeDecodeError:
            text = raw.decode("utf-8", "replace")
    else:
        text = raw
    names = [l.strip().lstrip("\ufeff") for l in text.replace("\0", "").splitlines()]
    names = [n for n in names if n]
    import re
    return names if names and all(re.match(r"^[A-Za-z0-9][A-Za-z0-9._-]*$", n) for n in names) else []


def wsl_distro():
    """name of the default WSL distribution (the first `wsl.exe --list --quiet` prints), or None: no wsl.exe, no distribution, or the command failed.
    Nothing is installed or started by this call beyond wsl.exe's own listing."""
    import shutil, subprocess
    exe = shutil.which("wsl.exe") or os.path.join(os.environ.get("SystemRoot", r"C:\Windows"), "System32", "wsl.exe")
    if not os.path.isfile(exe):
        return None
    try:
        p = subprocess.run([exe, "--list", "--quiet"], capture_output=True, timeout=30)
    except (OSError, subprocess.SubprocessError):
        return None
    if p.returncode != 0:
        return None
    names = parse_wsl_list(p.stdout)
    return names[0] if names else None


def wsl_path(path):
    """pure: the /mnt/<drive>/... form of a Windows path, as a WSL shell sees the same file (C:\\a\\b or C:/a/b -> /mnt/c/a/b); other text unchanged"""
    p = path.replace("\\", "/")
    if len(p) >= 2 and p[1] == ":" and p[0].isalpha():
        return "/mnt/%s%s" % (p[0].lower(), p[2:] if p[2:].startswith("/") else "/" + p[2:])
    return p
