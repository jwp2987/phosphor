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


# #1532: environment variables that must reach the app under test even where the launcher does not inherit os.environ (macOS `open --env`).
# Linux and Windows pass the whole environment. APPCI_APP_ENV="K=V,K2=V2" (run.py) sets more of them for one run.
PASSTHROUGH_ENV = ("PHOSPHOR_QUEUE_DESTINATION", "PHOSPHOR_CONPTY_ENTER_HOLD")


def passthrough_env(environ=None):
    environ = os.environ if environ is None else environ
    keys = list(PASSTHROUGH_ENV) + [kv.split("=", 1)[0] for kv in environ.get("APPCI_APP_ENV", "").split(",") if "=" in kv]
    return {k: environ[k] for k in keys if k in environ and environ[k] != ""}


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
        self.last_pid = None
        self.warn = None
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
        for k, v in passthrough_env().items():
            env_args += ["--env", "%s=%s" % (k, v)]
        # exactly ONE instance per pass: refuse to launch over a leftover of this job's bundle (it would share the window/dialog state)
        left = self._instances()
        if left:
            raise RuntimeError("an instance of this job's bundle is already running (pids %s): refusing a second launch" % [p for p, _ in left])
        subprocess.run(["open", "-n"] + env_args + [self.bundle], check=True)
        self._pid = None
        t = time.time()
        while time.time() - t < 30 and not self.pid():
            time.sleep(0.5)
        self.last_pid = self._pid
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

    def _instances(self):
        """[(pid, command)] of every process whose executable lives inside THIS job's private bundle path. Matching is by the
        exact bundle path (inside the job dir), never by bare process name: the owner's own Phosphor/Warp apps cannot match."""
        out = subprocess.run(["ps", "-axo", "pid=,command="], capture_output=True, text=True).stdout
        pre = self.bundle.rstrip("/") + "/Contents/MacOS/"
        res = []
        for l in out.splitlines():
            p = l.strip().split(None, 1)
            if len(p) == 2 and p[1].startswith(pre):
                res.append((int(p[0]), p[1]))
        return res

    def _verified(self, p):
        """p is alive (kill -0) and its command line is in our bundle (PID-reuse safe)"""
        try:
            os.kill(p, 0)
        except ProcessLookupError:
            return False
        except PermissionError:
            pass
        c = subprocess.run(["ps", "-o", "command=", "-p", str(p)], capture_output=True, text=True).stdout.strip()
        return c.startswith(self.bundle.rstrip("/") + "/Contents/MacOS/")

    def pid(self):
        """The PID of the one instance under test; re-resolved on demand. The last known PID is kept while verified (a transient
        ps miss cannot lose it); otherwise the lowest PID inside the bundle path (the `open` launch; helpers come later)."""
        if self._pid and self._verified(self._pid):
            return self._pid
        inst = self._instances()
        if inst:
            main = [p for p, c in inst if not self._is_helper(c)] or [p for p, _ in inst]
            self._pid = self.last_pid = min(main)
            if len(main) > 1:
                self.warn = "%d app instances inside %s: %s" % (len(main), self.bundle, main)
            return self._pid
        return None

    def instance_count(self):
        """App INSTANCES, not processes: Phosphor runs a `terminal-server` helper from the same executable, which is part of
        the one instance (first Mac CI reuse run counted it and refused to start). A second instance is a second process
        WITHOUT a helper role argument."""
        return len([p for p, c in self._instances() if not self._is_helper(c)])

    @staticmethod
    def _is_helper(command):
        parts = command.split()
        return len(parts) > 1 and parts[1] in ("terminal-server",)

    def alive(self):
        return self.pid() is not None

    def death(self):
        """None while the app is fine. 'APP DIED' is claimed ONLY when the last known PID is verified gone (kill -0) AND no process of
        this job's bundle exists; a PID we never resolved, or a lookup that failed while the app is up, is reported as a lookup failure."""
        if self.pid():
            return None
        if self.last_pid is None and self._pid is None:
            return "PID lookup failed: no process inside %s was ever resolved (launch problem, not a crash)" % self.bundle
        return "APP DIED: pid %s is gone (kill -0 fails) and no process of %s exists" % (self.last_pid or self._pid, self.bundle)

    def stop(self):
        for p in sorted({p for p, _ in self._instances()}):
            for sig, wait in ((signal.SIGTERM, 6), (signal.SIGKILL, 3)):
                try:
                    os.kill(p, sig)
                except ProcessLookupError:
                    break
                t = time.time()
                while time.time() - t < wait and self._verified(p):
                    time.sleep(0.3)
                if not self._verified(p):
                    break
        self._pid = None

    def cpu_ticks(self):
        p = self.pid()
        out = subprocess.run(["ps", "-o", "cputime=,rss=", "-p", str(p)], capture_output=True, text=True).stdout.split()
        parts = out[0].replace(".", ":").split(":")
        secs = sum(float(x) * m for x, m in zip(reversed(parts), (0.01, 1, 60, 3600)))
        return int(secs * os.sysconf("SC_CLK_TCK")) if hasattr(os, "sysconf") and "SC_CLK_TCK" in os.sysconf_names else int(secs * 100), int(out[1])


class WindowsApp:
    """Windows: the side-by-side TEST build (installed into <Out>\\app by windows/ci/app_test.ps1; its exe path is --bin) started directly
    from this process, so the PID is the one we started and the only one ever stopped (taskkill /PID <pid> /T /F, never by name). The app keeps
    its data under %LOCALAPPDATA%\\phosphor and %APPDATA%\\phosphor (known folders, NOT HOME-relative), which is why app_test.ps1 backs both up
    with md5 and restores them: prepare() wipes them once per (mode, pane) exactly like the PowerShell harness did, start() installs the
    profile's settings.toml at %LOCALAPPDATA%\\phosphor\\Phosphor\\config. The pane shell is PowerShell (the app default; no rc hook is possible
    because the app starts it with -NoProfile) or Git Bash (session.startup_shell_override; HOME = the scratch home, so ~/.bashrc logs rec.log)."""

    DATA = ("LOCALAPPDATA", "APPDATA")

    def __init__(self, run, home, binary, pane="powershell", ctl=None):
        self.run, self.home, self.bin, self.pane = run, home, binary, pane
        self.ctl = ctl or os.path.join(run, "ctl")
        self.proc = None
        self.exit_code = None
        self.config = os.path.join(os.environ["LOCALAPPDATA"], "phosphor", "Phosphor", "config")

    def prepare(self):
        import shutil
        for v in self.DATA:
            shutil.rmtree(os.path.join(os.environ[v], "phosphor"), ignore_errors=True)

    def _install_settings(self):
        import shutil
        src = os.path.join(self.home, ".config", "phosphor", "settings.toml")
        os.makedirs(self.config, exist_ok=True)
        shutil.copy2(src, os.path.join(self.config, "settings.toml"))

    def env(self):
        import winapi
        e = {k: v for k, v in os.environ.items() if not (k.startswith("CI_JOB_JWT") or k in ("CI_JOB_TOKEN", "CI_REGISTRY_PASSWORD", "CI_DEPLOY_PASSWORD"))}
        fwd = lambda p: p.replace("\\", "/")           # noqa: E731  (msys bash takes C:/... paths; backslashes inside variables survive unchanged)
        # r18d2: the agent cases read INFO markers ('[byop] opening stream', 'stream opened OK', '[byop-diag] queue_actions'); a bare `warn` base hid them, so
        # agent_stream_cancel / agent_approval reported "no model request" on a run whose screenshot showed the finished reply. Only those two modules go to info.
        e["RUST_LOG"] = "warn,warp::ai::agent_providers=info,warp::ai::blocklist::controller=info,warp::input_order=debug,warpui::frame_cause=debug,phosphor::routing=debug"
        e["WARP_INPUT_ORDER_LOG_TEXT"] = "1"
        e["PA_CTL"] = fwd(self.ctl)
        e["APPCI_W"] = fwd(os.path.join(self.home, "w"))
        tail = os.pathsep.join(winapi.git_dirs())
        e["PATH"] = os.path.join(self.home, "bin") + os.pathsep + e.get("PATH", "") + (os.pathsep + tail if tail else "")
        if os.name != "nt":
            # where the CI users keep shells (nu in ~/.local/bin): on the app's own PATH, as on a real user's machine. Added only inside the pane
            # (`export PATH`) the app's classifier does not see it, and agent view sends a bare `nu` to the agent as a prompt (r18cz Linux).
            have = e["PATH"].split(os.pathsep)
            extra = [d for d in (os.path.expanduser(x) for x in ("~/.local/bin", "/home/gitlab-runner/.local/bin", "/opt/homebrew/bin", "~/.cargo/bin"))
                     if os.path.isdir(d) and d not in have]
            if extra:
                e["PATH"] = e["PATH"] + os.pathsep + os.pathsep.join(dict.fromkeys(extra))
        if self.pane == "gitbash":
            e["HOME"] = self.home
            e["SHELL"] = "/usr/bin/bash"
            e["CHERE_INVOKING"] = "1"
        return e

    def start(self):
        self._install_settings()
        os.makedirs(os.path.join(self.home, "w"), exist_ok=True)
        out = open(os.path.join(self.run, "app.stdout"), "ab")
        self.exit_code = None
        self.proc = subprocess.Popen([self.bin], cwd=os.path.join(self.home, "w"), env=self.env(), stdout=out, stderr=subprocess.STDOUT)
        with open(os.path.join(self.run, "pids"), "a") as f:
            f.write("app %d\n" % self.proc.pid)
        return self.proc.pid

    def wait_window(self, timeout=60):
        import winapi
        t = time.time()
        while time.time() - t < timeout:
            if self.proc.poll() is not None:
                return False
            if winapi.main_window(self.proc.pid):
                return True
            time.sleep(0.5)
        return False

    def pid(self):
        return self.proc.pid if self.proc and self.proc.poll() is None else None

    def alive(self):
        return self.proc is not None and self.proc.poll() is None

    def death(self):
        if self.proc is None or self.proc.poll() is None:
            return None
        self.exit_code = self.proc.returncode
        return "APP DIED: pid %d exited with code %s (crash): see logs/app.stdout" % (self.proc.pid, self.proc.returncode)

    def stop(self):
        p, self.proc = self.proc, None
        if not p:
            return
        if p.poll() is None:
            subprocess.run(["taskkill", "/PID", str(p.pid), "/T", "/F"], capture_output=True, timeout=30)
            try:
                p.wait(10)
            except Exception:
                pass

    def clk_tck(self):
        return 10_000_000          # FILETIME units (100 ns)

    def cpu_ticks(self):
        import winapi
        return winapi.process_times(self.proc.pid)

    def find_log(self):
        """newest phosphor*.log under the app's data dirs (the location is not HOME-relative on Windows), or None"""
        best = None
        for v in self.DATA:
            base = os.path.join(os.environ[v], "phosphor")
            for d, _, fs in os.walk(base):
                for f in fs:
                    if f.lower().startswith("phosphor") and f.lower().endswith(".log"):
                        p = os.path.join(d, f)
                        if best is None or os.path.getmtime(p) > os.path.getmtime(best):
                            best = p
        return best
