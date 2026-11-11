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

    # r15 wave 4 (#2060): the pointer and the window size
    user32.GetSystemMetrics.argtypes = [ctypes.c_int]
    user32.GetSystemMetrics.restype = ctypes.c_int
    user32.GetCursorPos.argtypes = [ctypes.POINTER(wt.POINT)]
    user32.ClientToScreen.argtypes = [ctypes.c_void_p, ctypes.POINTER(wt.POINT)]
    user32.GetClientRect.argtypes = [ctypes.c_void_p, ctypes.POINTER(wt.RECT)]
    user32.WindowFromPoint.argtypes = [wt.POINT]
    user32.WindowFromPoint.restype = ctypes.c_void_p
    user32.GetAncestor.argtypes = [ctypes.c_void_p, wt.UINT]
    user32.GetAncestor.restype = ctypes.c_void_p
    user32.IsZoomed.argtypes = [ctypes.c_void_p]
    user32.SetWindowPos.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int, wt.UINT]
    user32.SetWindowPos.restype = wt.BOOL
    user32.ShowWindowAsync.argtypes = [ctypes.c_void_p, ctypes.c_int]


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


# ---------------------------------------------------------------- r15 wave 4 (#2060; r14-test-gaps T31, T33): the pointer and the window size
# The pointer is SendInput mouse events at ABSOLUTE positions on the virtual desktop (never relative moves: pointer acceleration would bend them).
# Positions are screen pixels as the DPI-aware harness sees them (backend.WindowsBackend calls SetProcessDPIAware), which is what the app's find hook
# logs (device pixels of the window's client area) plus ClientToScreen of that window. Every move is read back with GetCursorPos: a pointer that is
# not where it was sent (a locked or disconnected desktop swallows SendInput and still reports the events as accepted) raises PointerError, so
# "the click did nothing" can never be read as the app ignoring a click. The structures have fixed-width fields, so their layout is the Win64 one
# on any 64-bit Python and the self-test checks it on Linux.
MOUSEEVENTF_MOVE, MOUSEEVENTF_WHEEL, MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_ABSOLUTE = 0x0001, 0x0800, 0x4000, 0x8000
# button numbers as xdotool and cases_ui.Mouse use them: 1 left, 2 middle, 3 right -> (down flag, up flag)
MOUSE_BUTTON = {1: (0x0002, 0x0004), 2: (0x0020, 0x0040), 3: (0x0008, 0x0010)}
WHEEL_DELTA = 120
SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN = 76, 77, 78, 79
SWP_NOSIZE, SWP_NOMOVE, SWP_NOZORDER, SWP_NOACTIVATE, SWP_ASYNCWINDOWPOS = 0x0001, 0x0002, 0x0004, 0x0010, 0x4000
SW_MAXIMIZE, SW_RESTORE = 3, 9


class MOUSEINPUT(ctypes.Structure):
    _fields_ = [("dx", ctypes.c_int32), ("dy", ctypes.c_int32), ("mouseData", ctypes.c_uint32), ("dwFlags", ctypes.c_uint32),
                ("time", ctypes.c_uint32), ("dwExtraInfo", ctypes.c_size_t)]


class KEYBDINPUT(ctypes.Structure):
    _fields_ = [("wVk", ctypes.c_uint16), ("wScan", ctypes.c_uint16), ("dwFlags", ctypes.c_uint32), ("time", ctypes.c_uint32),
                ("dwExtraInfo", ctypes.c_size_t)]


class _InputUnion(ctypes.Union):
    _fields_ = [("mi", MOUSEINPUT), ("ki", KEYBDINPUT)]


class INPUT(ctypes.Structure):
    _fields_ = [("type", ctypes.c_uint32), ("u", _InputUnion)]


class PointerError(RuntimeError):
    """the pointer could not be put where the case asked (outside the screen, SendInput refused, or the cursor is not there afterwards): a HARNESS
    or machine condition, never evidence about the app"""


def mouse_norm(x, y, vs):
    """screen pixel (x, y) -> the 0..65535 coordinates MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK takes, for the virtual screen vs = (left, top,
    width, height); None when the pixel is not on it. ceil(p * 65536 / size): the smallest value Windows maps back to exactly pixel p."""
    vx, vy, vw, vh = vs
    if vw < 1 or vh < 1 or not (vx <= x < vx + vw and vy <= y < vy + vh):
        return None
    return (min(65535, ((int(x) - vx) * 65536 + vw - 1) // vw), min(65535, ((int(y) - vy) * 65536 + vh - 1) // vh))


def mouse_move_event(x, y, vs):
    """the SendInput event (dx, dy, mouseData, flags) that puts the pointer on screen pixel (x, y), or None off the virtual screen"""
    n = mouse_norm(x, y, vs)
    return None if n is None else (n[0], n[1], 0, MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK)


def mouse_button_event(button, down):
    """the event that presses or releases button 1 (left), 2 (middle) or 3 (right) where the pointer is"""
    if button not in MOUSE_BUTTON:
        raise ValueError("no mouse button %r (1 left, 2 middle, 3 right)" % (button,))
    return (0, 0, 0, MOUSE_BUTTON[button][0 if down else 1])


def mouse_wheel_event(steps):
    """one event of `steps` wheel notches: positive = down (later lines), as cases_pointer.wheel; Windows' own sign is the opposite (positive = away
    from the user = up), so the delta is negated here and nowhere else"""
    return (0, 0, (-int(steps) * WHEEL_DELTA) & 0xFFFFFFFF, MOUSEEVENTF_WHEEL)


def _send_input_fn():
    """a PRIVATE SendInput function object (user32[...] makes a new one): backend.WindowsBackend declares user32.SendInput for its own keyboard INPUT
    class, and ctypes would refuse this module's array through that declaration"""
    f = user32["SendInput"]
    f.argtypes = [wt.UINT, ctypes.POINTER(INPUT), ctypes.c_int]
    f.restype = wt.UINT
    return f


def send_mouse(evs, send=None):
    """evs: [(dx, dy, mouseData, flags)] in ONE SendInput call -> how many events it accepted"""
    n = len(evs)
    arr = (INPUT * n)()
    for i, (dx, dy, data, fl) in enumerate(evs):
        arr[i].type = 0                                # INPUT_MOUSE
        arr[i].u.mi.dx, arr[i].u.mi.dy, arr[i].u.mi.mouseData, arr[i].u.mi.dwFlags = dx, dy, data, fl
    return int((send or _send_input_fn())(n, arr, ctypes.sizeof(INPUT)))


def virtual_screen(u32=None):
    """(left, top, width, height) of the virtual desktop (all monitors) in pixels"""
    u32 = u32 or user32
    return tuple(int(u32.GetSystemMetrics(i)) for i in (SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN))


def cursor_pos(u32=None):
    """(x, y) of the pointer, or None when it cannot be read (no input desktop: a locked screen)"""
    u32 = u32 or user32
    pt = wt.POINT()
    return (int(pt.x), int(pt.y)) if u32.GetCursorPos(ctypes.byref(pt)) else None


def client_rect_screen(h, u32=None):
    """(x, y, w, h) of a window's CLIENT area in screen pixels (the area the app draws: its find hook's origin), or None"""
    u32 = u32 or user32
    pt, rc = wt.POINT(0, 0), wt.RECT()
    if not h or not u32.ClientToScreen(h, ctypes.byref(pt)) or not u32.GetClientRect(h, ctypes.byref(rc)):
        return None
    return int(pt.x), int(pt.y), int(rc.right - rc.left), int(rc.bottom - rc.top)


def window_rect(h, u32=None):
    """(x, y, w, h) of a window's outer rectangle in screen pixels, or None"""
    u32 = u32 or user32
    r = wt.RECT()
    if not h or not u32.GetWindowRect(h, ctypes.byref(r)):
        return None
    return int(r.left), int(r.top), int(r.right - r.left), int(r.bottom - r.top)


def root_window_at(x, y, u32=None):
    """(hwnd, pid) of the top-level window that owns screen pixel (x, y); (None, 0) over nothing"""
    u32 = u32 or user32
    h = u32.WindowFromPoint(wt.POINT(int(x), int(y)))
    if not h:
        return None, 0
    root = u32.GetAncestor(h, 2) or h                  # GA_ROOT
    pid = wt.DWORD(0)
    u32.GetWindowThreadProcessId(root, ctypes.byref(pid))
    return root, int(pid.value)


def is_zoomed(h, u32=None):
    return bool((u32 or user32).IsZoomed(h))


def show_window(h, cmd, u32=None):
    """SW_RESTORE / SW_MAXIMIZE of a window, posted to its own thread (ShowWindowAsync: a busy app cannot hang the harness)"""
    return bool((u32 or user32).ShowWindowAsync(h, int(cmd)))


def set_window_rect(h, w, hgt, x=None, y=None, u32=None):
    """ask for a new outer size (and place when x, y are given) of a top-level window: SetWindowPos, posted to the window's own thread
    (SWP_ASYNCWINDOWPOS) so an app that is busy cannot hang the harness; no z-order change, no activation. -> True when the call was accepted
    (the caller reads the rectangle back: accepted is not applied)"""
    u32 = u32 or user32
    flags = SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS | (SWP_NOMOVE if x is None or y is None else 0)
    return bool(u32.SetWindowPos(h, None, int(x or 0), int(y or 0), int(w), int(hgt), flags))


def resize_verdict(before, asked, after):
    """None when a window that was `before` (w, h) and was asked for `asked` is `after` in a way that proves the request took: exactly the asked size,
    or a different size than before (Windows clamps a size to the window's minimum and to the desktop). Else what is wrong."""
    if after is None:
        return "the window's rectangle could not be read after SetWindowPos"
    if tuple(after) == tuple(asked):
        return None
    if tuple(before) == tuple(asked):
        return "the window was %dx%d, was asked to stay %dx%d and is now %dx%d" % (tuple(before) + tuple(asked) + tuple(after))
    if tuple(after) == tuple(before):
        return "the window is still %dx%d after SetWindowPos asked for %dx%d (a maximised or fixed-size window, or the request was dropped)" % (tuple(after) + tuple(asked))
    return None


class Pointer:
    """the mouse of the Windows backend. move() is one absolute SendInput move, read back with GetCursorPos (within `tol` px, waited for at most
    `settle` seconds: the input thread applies it a moment after SendInput returns); button() and wheel() act where the pointer is. The functions it
    calls are parameters so the self-test drives it on Linux."""

    def __init__(self, send=None, cursor=None, vscreen=None, sleep=None, tol=1, settle=1.0):
        import time as _t
        self._send = send or (lambda evs: send_mouse(evs))
        self._cursor = cursor or cursor_pos
        self._vscreen = vscreen or virtual_screen
        self._sleep = sleep or _t.sleep
        self.tol, self.settle = tol, settle
        self.held = set()                              # buttons this object pressed and has not released

    def _one(self, ev, what):
        got = self._send([ev])
        if got != 1:
            raise PointerError("SendInput refused the %s (accepted %d of 1 event: blocked by UIPI or an input hook)" % (what, got))

    def move(self, x, y):
        x, y = int(x), int(y)
        vs = self._vscreen()
        ev = mouse_move_event(x, y, vs)
        if ev is None:
            raise PointerError("(%d, %d) is not on the screen (virtual desktop %r): the window, or the text the app drew, is partly off the screen" % (x, y, tuple(vs)))
        self._one(ev, "pointer move to (%d, %d)" % (x, y))
        waited, pos = 0.0, self._cursor()
        while not self._at(pos, x, y) and waited < self.settle:
            self._sleep(0.01)
            waited += 0.01
            pos = self._cursor()
        if not self._at(pos, x, y):
            raise PointerError("the pointer was sent to (%d, %d) and is at %s: SendInput accepted the move and the desktop did not apply it. NEEDS: the test "
                               "runner on the unlocked, connected interactive console session of the Windows VM (a locked screen or a disconnected RDP "
                               "session has no input desktop)" % (x, y, "an unreadable position" if pos is None else "(%d, %d)" % pos))
        return pos

    def _at(self, pos, x, y):
        return pos is not None and abs(pos[0] - x) <= self.tol and abs(pos[1] - y) <= self.tol

    def button(self, button, down):
        self._one(mouse_button_event(button, down), "button %s %s" % (button, "press" if down else "release"))
        (self.held.add if down else self.held.discard)(button)

    def wheel(self, steps):
        self._one(mouse_wheel_event(steps), "wheel %+d" % steps)

    def release_all(self):
        """release every button this object still holds (a case that died between press and release); never raises"""
        for b in sorted(self.held):
            try:
                self.button(b, False)
            except Exception:
                self.held.discard(b)
