# appci/app.py: starts / stops the Phosphor build under test on its own dbus session, tracking exact PIDs.
import os, signal, subprocess, time

INNER = r'''#!/bin/bash
unset GNOME_KEYRING_CONTROL SSH_AUTH_SOCK SSH_ASKPASS SSH_ASKPASS_REQUIRE
if [ "$APPCI_APP_DISPLAY" = x11 ]; then unset WAYLAND_DISPLAY; fi
if [ "$APPCI_APP_DISPLAY" = wayland ] && [ -z "$APPCI_KEEP_DISPLAY" ]; then unset DISPLAY; fi
export HOME="%(home)s" XDG_CONFIG_HOME="%(home)s/.config" XDG_STATE_HOME="%(home)s/.local/state" XDG_DATA_HOME="%(home)s/.local/share"
export SHELL=/bin/bash ZDOTDIR="%(home)s"
export RUST_LOG=warp::input_order=debug,warp::terminal::model_lock_trace=debug,warpui::frame_cause=debug,phosphor::routing=debug
export WARP_INPUT_ORDER_LOG_TEXT=1
mkdir -p "$XDG_STATE_HOME" "$XDG_DATA_HOME"
printf "" | gnome-keyring-daemon --unlock --components=secrets >/dev/null 2>&1 || echo "WARNING: gnome-keyring-daemon unavailable" >&2
echo $$ > "%(run)s/app.pid"
cd "%(home)s/w"
exec "%(bin)s" >> "%(run)s/app.stdout" 2>&1
'''


class App:
    def __init__(self, run, home, binary, display="wayland"):
        self.run, self.home, self.bin, self.display = run, home, binary, display
        self.proc = None

    def start(self):
        inner = os.path.join(self.run, "inner.sh")
        open(inner, "w").write(INNER % {"home": self.home, "run": self.run, "bin": self.bin})
        os.chmod(inner, 0o755)
        env = dict(os.environ, APPCI_APP_DISPLAY=self.display)
        try:
            os.remove(os.path.join(self.run, "app.pid"))
        except FileNotFoundError:
            pass
        # own session => own process group; it is the ONLY thing stop() ever signals
        self.proc = subprocess.Popen(["dbus-run-session", "--", inner], env=env, start_new_session=True,
                                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        with open(os.path.join(self.run, "pids"), "a") as f:
            f.write("app-launcher %d\n" % self.proc.pid)
        return self.proc.pid

    def pid(self):
        try:
            return int(open(os.path.join(self.run, "app.pid")).read().strip())
        except Exception:
            return None

    def alive(self):
        p = self.pid()
        if not p:
            return False
        try:
            return open("/proc/%d/stat" % p).read().split(")")[-1].split()[0] != "Z"
        except OSError:
            return False

    def stop(self):
        if not self.proc:
            return
        pg = self.proc.pid
        for sig, wait in ((signal.SIGTERM, 6), (signal.SIGKILL, 3)):
            try:
                os.killpg(pg, sig)
            except ProcessLookupError:
                break
            t = time.time()
            while time.time() - t < wait:
                try:
                    os.killpg(pg, 0)
                except ProcessLookupError:
                    break
                time.sleep(0.2)
            else:
                continue
            break
        try:
            self.proc.wait(timeout=2)
        except Exception:
            pass
        self.proc = None

    def cpu_ticks(self):
        p = self.pid()
        f = open("/proc/%d/stat" % p).read().rsplit(")", 1)[1].split()
        return int(f[11]) + int(f[12]), int(open("/proc/%d/statm" % p).read().split()[1]) * os.sysconf("SC_PAGE_SIZE") // 1024


class MacApp:
    """macOS: `open -n` the extracted .app (a proper GUI launch, so it can become frontmost) with a PRIVATE HOME
    (`open --env HOME=...`, the method the hand-run round-18 harness proved): the app derives every path from it, so the
    settings file is <home>/.phosphor/settings.toml (warp_core::paths::config_local_dir, Channel::Oss on macOS), the log is
    <home>/Library/Logs/phosphor.log, and the runner user's real ~/.phosphor, Library and Keychain-free state are never read
    or written. Nothing to back up or restore. Only the exact PID found from the bundle's executable path is ever signalled."""

    def __init__(self, run, home, app_bundle, binary, display=None):
        self.run, self.home, self.bundle, self.bin = run, home, app_bundle, binary
        self.settings = os.path.join(home, ".phosphor", "settings.toml")
        self._pid = None
        self.log_path = os.path.join(home, "Library", "Logs", "phosphor.log")

    def _install_settings(self):
        import shutil
        src = os.path.join(self.home, ".config/phosphor/settings.toml")     # written by profile.py (same schema on every OS)
        os.makedirs(os.path.dirname(self.settings), exist_ok=True)
        shutil.copy2(src, self.settings)
        os.chmod(self.settings, 0o600)

    def start(self):
        self._install_settings()
        os.makedirs(os.path.dirname(self.log_path), exist_ok=True)
        env_args = []
        for k, v in (("HOME", self.home), ("SHELL", "/bin/zsh"), ("ZDOTDIR", self.home), ("APPCI_W", self.home + "/w"),
                     ("PA_CTL", os.path.join(self.run, "ctl")),
                     ("RUST_LOG", "warp::input_order=debug,warpui::frame_cause=debug,phosphor::routing=debug"),
                     ("WARP_INPUT_ORDER_LOG_TEXT", "1")):
            env_args += ["--env", "%s=%s" % (k, v)]
        subprocess.run(["open", "-n"] + env_args + [self.bundle], check=True)
        self._pid = None
        t = time.time()
        while time.time() - t < 30 and not self.pid():
            time.sleep(0.5)
        with open(os.path.join(self.run, "pids"), "a") as f:
            f.write("app %s\n" % self._pid)
        self.focus()
        return self._pid

    def focus(self):
        p = self.pid()
        if p:       # System Events frontmost for that exact PID; sends no keys
            subprocess.run(["osascript", "-e", 'tell application "System Events" to set frontmost of (first process whose unix id is %d) to true' % p],
                           capture_output=True, timeout=30)
            time.sleep(1.0)

    def pid(self):
        out = subprocess.run(["ps", "-axo", "pid=,command="], capture_output=True, text=True).stdout
        for l in out.splitlines():
            p = l.strip().split(None, 1)
            if len(p) == 2 and p[1] == self.bin:        # argv0 is exactly the bundle executable, no extra args
                self._pid = int(p[0])
                return self._pid
        self._pid = None
        return None

    def alive(self):
        return self.pid() is not None

    def stop(self):
        p = self.pid()
        if p:
            for sig, wait in ((signal.SIGTERM, 6), (signal.SIGKILL, 3)):
                try:
                    os.kill(p, sig)
                except ProcessLookupError:
                    break
                t = time.time()
                while time.time() - t < wait and self.pid():
                    time.sleep(0.3)
                if not self.pid():
                    break

    def cpu_ticks(self):
        p = self.pid()
        out = subprocess.run(["ps", "-o", "cputime=,rss=", "-p", str(p)], capture_output=True, text=True).stdout.split()
        parts = out[0].replace(".", ":").split(":")
        secs = sum(float(x) * m for x, m in zip(reversed(parts), (0.01, 1, 60, 3600)))
        return int(secs * os.sysconf("SC_CLK_TCK")) if hasattr(os, "sysconf") and "SC_CLK_TCK" in os.sysconf_names else int(secs * 100), int(out[1])
