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


def process_times(pid):
    """(cpu_100ns, working_set_kib) of one process; raises OSError when it cannot be opened"""
    h = kernel32.OpenProcess(0x1000, False, pid)
    if not h:
        raise OSError("OpenProcess(%d) failed: %d" % (pid, ctypes.get_last_error()))
    try:
        c, e, k, u = wt.FILETIME(), wt.FILETIME(), wt.FILETIME(), wt.FILETIME()
        if not kernel32.GetProcessTimes(h, ctypes.byref(c), ctypes.byref(e), ctypes.byref(k), ctypes.byref(u)):
            raise OSError("GetProcessTimes failed: %d" % ctypes.get_last_error())
        cpu = ((k.dwHighDateTime << 32) | k.dwLowDateTime) + ((u.dwHighDateTime << 32) | u.dwLowDateTime)

        class PMC(ctypes.Structure):
            _fields_ = [("cb", wt.DWORD), ("PageFaultCount", wt.DWORD), ("PeakWorkingSetSize", ctypes.c_size_t), ("WorkingSetSize", ctypes.c_size_t),
                        ("QuotaPeakPagedPoolUsage", ctypes.c_size_t), ("QuotaPagedPoolUsage", ctypes.c_size_t),
                        ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t), ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
                        ("PagefileUsage", ctypes.c_size_t), ("PeakPagefileUsage", ctypes.c_size_t)]
        pm = PMC()
        pm.cb = ctypes.sizeof(PMC)
        psapi.GetProcessMemoryInfo.argtypes = [ctypes.c_void_p, ctypes.POINTER(PMC), wt.DWORD]
        ok = psapi.GetProcessMemoryInfo(h, ctypes.byref(pm), pm.cb)
        return cpu, (pm.WorkingSetSize // 1024 if ok else 0)
    finally:
        kernel32.CloseHandle(h)


def precise_time():
    """seconds since the epoch from GetSystemTimePreciseAsFileTime (sub-microsecond; time.time() on CPython 3.12/Windows ticks at ~15.6 ms)"""
    ft = wt.FILETIME()
    kernel32.GetSystemTimePreciseAsFileTime(ctypes.byref(ft))
    return (((ft.dwHighDateTime << 32) | ft.dwLowDateTime) - 116444736000000000) / 1e7


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
