# appci/wininstall.py: the Windows installer (Inno Setup) run silently into a directory of the job, and its uninstaller, for the app cases that install
# a build themselves (r15: installer_smoke, T43; the previous release for upgrade_from_previous_release, T44). docs/testing.md 2.25.
#
# An install for the current user writes outside its directory: the uninstall entry, the Explorer context-menu keys, the app's own key, the Start-menu and
# Desktop shortcuts and (task addToPath) the user's PATH. Snapshot() records all of them before the first install and restore() puts them back after the
# last uninstall, so a case leaves the machine as it found it (windows/ci/app_test.ps1 restores the uninstall key, the handler, PATH and the shortcuts
# again at the end of the job; the context-menu keys and the app key are restored only here).
# The installer and the uninstaller refuse to run while a Phosphor of this channel is up (AppMutex): the caller stops the app under test first.
# Nothing here kills a process by name, and nothing is written outside the directories the caller names.
import os, shutil, subprocess, time

APP_EXE = "phosphor-oss.exe"
UNINSTALL_KEY = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\zap-oss_is1"      # script/windows/windows-installer.iss AppId (bundle.ps1: InnoAppId=zap-oss)
REG_KEYS = (
    UNINSTALL_KEY,
    r"HKCU\Software\Classes\phosphor",                                    # the phosphor:// handler
    r"HKCU\Software\Phosphor\Phosphor",                                   # [Registry] uninsdeletekey
    r"HKCU\Software\Classes\Directory\shell\PhosphorTab",                 # "Open Phosphor in new tab" (HKA = HKCU for a current-user install)
    r"HKCU\Software\Classes\Directory\Background\shell\PhosphorTab",
    r"HKCU\Software\Classes\Directory\shell\PhosphorWindow",
    r"HKCU\Software\Classes\Directory\Background\shell\PhosphorWindow",
)


def shortcut_paths(environ=None):
    """the two shortcuts the installer may write for the current user"""
    environ = os.environ if environ is None else environ
    out = []
    if environ.get("APPDATA"):
        out.append(os.path.join(environ["APPDATA"], "Microsoft", "Windows", "Start Menu", "Programs", "Phosphor.lnk"))
    if environ.get("USERPROFILE"):
        out.append(os.path.join(environ["USERPROFILE"], "Desktop", "Phosphor.lnk"))
    return out


# ---------------------------------------------------------------- pure helpers (selftest `r15_installer_pure_parts`)
def installer_argv(setup, dest, log):
    """the silent current-user install of windows/ci/app_test.ps1, into `dest`"""
    return [setup, "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/CURRENTUSER", "/DIR=%s" % dest, "/LOG=%s" % log]


def uninstaller_argv(unins, log=None):
    return [unins, "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"] + (["/LOG=%s" % log] if log else [])


def find_uninstaller(names):
    """pure: the uninstaller among the file names of an install directory (Inno Setup: unins000.exe, unins001.exe after a re-install): the highest, or None"""
    c = sorted(n for n in names if n.lower().startswith("unins") and n.lower().endswith(".exe") and n[5:-4].isdigit())
    return c[-1] if c else None


def find_setup(names):
    """pure: the installer among the file names of the build's dist directory: the one *.exe (a `*Setup*.exe` wins when there are several), or None"""
    exes = sorted(n for n in names if n.lower().endswith(".exe"))
    setups = [n for n in exes if "setup" in n.lower()]
    return (setups or exes or [None])[0]


def install_location(reg_query_text):
    """pure: the InstallLocation value in the text `reg query <uninstall key>` printed, without a trailing backslash; None when absent"""
    for line in (reg_query_text or "").splitlines():
        parts = line.split(None, 2)
        if len(parts) == 3 and parts[0].lower() == "installlocation" and parts[1].upper().startswith("REG_"):
            return parts[2].strip().rstrip("\\")
    return None


def same_dir(a, b):
    """pure: two Windows directory paths name the same directory (case, slashes and a trailing separator do not matter)"""
    n = lambda p: (p or "").replace("/", "\\").rstrip("\\").lower()      # noqa: E731
    return bool(a) and bool(b) and n(a) == n(b)


# ---------------------------------------------------------------- machine state around an install
def _reg(*args):
    try:
        return subprocess.run(["reg.exe"] + list(args), capture_output=True, text=True, errors="replace", timeout=60)
    except (OSError, subprocess.SubprocessError) as e:
        return subprocess.CompletedProcess(args, 1, "", repr(e))


def _user_path():
    """(value, type) of HKCU\\Environment\\Path, or (None, None) when the value does not exist"""
    import winreg
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, "Environment") as k:
            return winreg.QueryValueEx(k, "Path")
    except OSError:
        return None, None


class Snapshot:
    """What an install changes outside its own directory, recorded in `folder` (a directory of the job). restore() -> list of problems (empty = all
    back as recorded). Idempotent: a second restore() finds nothing to do."""

    def __init__(self, folder):
        self.folder = folder
        os.makedirs(folder, exist_ok=True)
        self.keys = {}
        for i, key in enumerate(REG_KEYS):
            f = os.path.join(folder, "key%d.reg" % i)
            self.keys[key] = f if _reg("export", key, f, "/y").returncode == 0 and os.path.isfile(f) else None
        self.links = {}
        for p in shortcut_paths():
            try:
                self.links[p] = open(p, "rb").read()
            except OSError:
                self.links[p] = None
        self.path = _user_path()

    def restore(self):
        bad = []
        for key, f in self.keys.items():
            present = _reg("query", key).returncode == 0
            if present and _reg("delete", key, "/f").returncode != 0:
                bad.append("could not delete %s before putting it back" % key)
                continue
            if f and _reg("import", f).returncode != 0:
                bad.append("could not import the saved %s" % key)
        for p, data in self.links.items():
            try:
                if data is None:
                    if os.path.exists(p):
                        os.remove(p)
                elif not os.path.isfile(p) or open(p, "rb").read() != data:
                    with open(p, "wb") as fh:
                        fh.write(data)
            except OSError as e:
                bad.append("shortcut %s: %r" % (os.path.basename(p), e))
        now = _user_path()
        if now != self.path:
            try:
                import winreg
                with winreg.OpenKey(winreg.HKEY_CURRENT_USER, "Environment", 0, winreg.KEY_SET_VALUE) as k:
                    if self.path[0] is None:
                        winreg.DeleteValue(k, "Path")
                    else:
                        winreg.SetValueEx(k, "Path", 0, self.path[1], self.path[0])
            except OSError as e:
                bad.append("the user's PATH could not be put back: %r" % (e,))
        return bad


# ---------------------------------------------------------------- install / uninstall
def install(setup, dest, log, timeout=900):
    """run the installer silently into `dest`. -> (exe path or None, problem or None)"""
    os.makedirs(os.path.dirname(dest), exist_ok=True)
    try:
        p = subprocess.run(installer_argv(setup, dest, log), capture_output=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        return None, "the installer did not finish within %d s (a dialog it could not show? see %s)" % (timeout, log)
    except OSError as e:
        return None, "the installer could not be started: %r" % (e,)
    exe = os.path.join(dest, APP_EXE)
    if p.returncode != 0 or not os.path.isfile(exe):
        return None, "installer exit code %s, %s %s (a running Phosphor of this channel makes a silent install stop: AppMutex; see %s)" % (
            p.returncode, APP_EXE, "exists" if os.path.isfile(exe) else "missing", log)
    return exe, None


def uninstall(dest, log=None, timeout=600):
    """run the install's own uninstaller silently and wait until the app exe is gone. -> problem or None"""
    try:
        names = os.listdir(dest)
    except OSError as e:
        return "the install directory cannot be listed: %r" % (e,)
    u = find_uninstaller(names)
    if not u:
        return "no unins*.exe in %s: this install cannot be removed by its own uninstaller" % dest
    try:
        p = subprocess.run(uninstaller_argv(os.path.join(dest, u), log), capture_output=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        return "the uninstaller did not finish within %d s" % timeout
    except OSError as e:
        return "the uninstaller could not be started: %r" % (e,)
    exe = os.path.join(dest, APP_EXE)
    t = time.time()
    while os.path.exists(exe) and time.time() - t < 60:      # the uninstaller's second stage runs from a temporary copy
        time.sleep(0.5)
    if os.path.exists(exe):
        return "uninstaller exit code %s, but %s is still there after 60 s" % (p.returncode, exe)
    if p.returncode != 0:
        return "uninstaller exit code %s" % p.returncode
    return None


def copy_install(src, dst):
    """a plain copy of an installed directory (the previous release kept for the upgrade case), without its uninstaller data"""
    if os.path.isdir(dst):
        shutil.rmtree(dst, ignore_errors=True)
    shutil.copytree(src, dst, ignore=shutil.ignore_patterns("unins*.exe", "unins*.dat", "unins*.msg"))
    return os.path.join(dst, APP_EXE)
