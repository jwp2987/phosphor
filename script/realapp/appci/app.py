# appci/app.py: starts / stops the Phosphor build under test on its own dbus session, tracking exact PIDs.
import os, signal, subprocess, time

INNER = r'''#!/bin/bash
unset GNOME_KEYRING_CONTROL SSH_AUTH_SOCK SSH_ASKPASS SSH_ASKPASS_REQUIRE
if [ "$APPCI_APP_DISPLAY" = x11 ]; then unset WAYLAND_DISPLAY; fi
if [ "$APPCI_APP_DISPLAY" = wayland ] && [ -z "$APPCI_KEEP_DISPLAY" ]; then unset DISPLAY; fi
export HOME="%(home)s" XDG_CONFIG_HOME="%(home)s/.config" XDG_STATE_HOME="%(home)s/.local/state" XDG_DATA_HOME="%(home)s/.local/share"
export SHELL=/bin/bash ZDOTDIR="%(home)s"
export RUST_LOG=warp::input_order=debug,warp::terminal::model_lock_trace=debug,warpui::frame_cause=debug,warpui::appci_find=info,phosphor::routing=debug,editor_memory=debug,warp::terminal::cli_agent_sessions::event=info,editor_disk=debug,remote_server::backend=debug
export WARP_INPUT_ORDER_LOG_TEXT=1
# #2040: the bytes of user keys on their way to the pty (`hop=pty_write_bytes class=UserKey`, `hop=hand_route bytes=`) need this second opt-in; without it
# the app logs their length. The harness types synthetic text only. Never set it where a person types: its name says why.
export WARP_INPUT_ORDER_LOG_KEY_BYTES_MAY_INCLUDE_PASSWORDS=1
# #1789 / #2069: the app's find hook. Until round 14 only the Windows launcher set these two (where there is no pointer), so on Linux, the only platform
# with pointer cases, the hook never logged a position: r12a, every hook-asking case, "app find hook gave no position ... located by OCR instead", and
# ui_right_click_paste has never got past "the probe was not found on screen by OCR". cases_ui.register_find appends tokens to this file.
export PHOSPHOR_APPCI_FIND_FILE="%(home)s/appci_find.txt"
# #2075 (T34): the app's grid snapshot hook (app/src/terminal/appci_grid.rs). cases_w4core.grid_request lists tokens in <dir>/request; the app writes
# the grid it paints to <dir>/<token>.grid when a listed token is on screen.
export PHOSPHOR_APPCI_GRID_DIR="%(home)s/appci_grid"
mkdir -p "%(home)s/appci_grid"
mkdir -p "$XDG_STATE_HOME" "$XDG_DATA_HOME"
if [ -z "$APPCI_NO_KEYRING" ]; then
  # A fresh HOME has no default collection and gnome-keyring cannot create one without a prompt, so the "unlocked" keyring was never usable
  # (round 6: "Failed to acquire default Secret Service collection: unknown error" on every launch). Seed an unencrypted login keyring: the
  # daemon then answers Unlock without a prompt.
  KR="$XDG_DATA_HOME/keyrings"
  if [ ! -e "$KR/default" ]; then
    mkdir -p "$KR" && chmod 700 "$KR"
    printf '[keyring]\ndisplay-name=Login\nctime=0\nmtime=0\nlock-on-idle=false\nlock-after=false\n' > "$KR/login.keyring"
    printf login > "$KR/default"
  fi
  printf "" | gnome-keyring-daemon --unlock --components=secrets >/dev/null 2>&1 || echo "WARNING: gnome-keyring-daemon unavailable" >&2
fi
# r15 (r14-test-gaps T37 / T39): a case's own session services (the recording notification service, an input-method daemon) start INSIDE this
# private bus, before the app. The case writes the lines and names the file in APPCI_SESSION_PRE (App.extra_env, one launch); unset for every other case.
if [ -n "$APPCI_SESSION_PRE" ] && [ -r "$APPCI_SESSION_PRE" ]; then . "$APPCI_SESSION_PRE"; fi
echo $$ > "%(run)s/app.pid"
cd "%(home)s/w"
exec %(wrap)s"%(bin)s" >> "%(run)s/app.stdout" 2>&1
'''


# The Windows launcher's RUST_LOG. Its bare `warn` base hides every plain `log::info!` line, so each module whose lines a case reads has its own directive
# (#1966: `hop=agent_action` comes from warp::ai::blocklist::action_model, which had none, so agent_question judged a card that was shown as "hung").
# selftest `windows_rust_log_enables_every_hop_module_1966` scans the Rust sources for plain-log `hop=` lines the cases read and fails on a module this
# string does not enable; a new such hop needs its module added here.
WINDOWS_RUST_LOG = ",".join([
    "warn", "winit=info",
    "warp::ai::agent_providers=info", "warp::ai::blocklist::controller=info",
    "warp::ai::blocklist::action_model=info", "warp::ai::blocklist::block::cli_controller=info", "warp::ai::blocklist::queued_query=info",
    "warp::code_review::diff_state_remote=info", "warp::code_review::git_status_update_remote=info", "warp::code_review::github_repo_model::remote=info",
    "warp::completer::pushed_listing=info", "warp::remote_server::remote_index_driver=info", "warp::settings_view::network_page=info",
    "remote_server::manager=info", "remote_server::backend::router=info",
    "warp::input_order=debug", "warp::terminal::model_lock_trace=debug", "warpui::frame_cause=debug", "warpui::appci_find=info",
    "phosphor::routing=debug", "editor_memory=debug",
    "warp::uri=info",          # r15 installer_smoke: `received url scheme=.. host=.. path=..` (uri::handle_incoming_uri), the proof a second launch was handed over
])

# Round 15 (#2076, #2087), kept as separate statements so sibling branches that add to the list above merge cleanly:
#   editor_disk=debug        the code editor's file-watcher trace (`editor_disk outcome=...`, editor_daily); also in the Linux and macOS launchers
#   warp::pane_group=info    'Displaying unsaved changes warning for pane' (editor_daily's close question; tab_lifecycle_keys reads the same line)
WINDOWS_RUST_LOG += ",editor_disk=debug,warp::pane_group=info"
# Round 19 (#2109): two plain Info lines the md_viewer cases read were hidden by the bare `warn` base, on Windows only:
#   warpui_core::core::app=info       'dispatching action for "root_view:add_file_pane"' (cases.FILE_PANE_OPENED): without it md_viewer_first_frame
#                                     failed ("the app did not open the file") and md_viewer / md_viewer_big never closed their viewer tab
#                                     ("nothing was closed"), which left it in front of the pane (pane_wedged_after_md_viewer_first_frame)
#   warp_editor::render::model=info   'rich text: first size ..' (cases_r17_misc.FIRST_SIZE_RE, #2028)
WINDOWS_RUST_LOG += ",warpui_core::core::app=info,warp_editor::render::model=info"
# macOS: the same trace, and the CLI-agent event line the two plugin cases read (the Linux launcher has had it since they were written)
MAC_RUST_LOG_MORE = ",editor_disk=debug,warp::terminal::cli_agent_sessions::event=info"


def rust_log_enabled(spec, module, level):
    """True when an env_logger-style RUST_LOG `spec` lets `module` log at `level` (error < warn < info < debug < trace): the longest directive naming a prefix
    of the module wins, a bare level is the default, no default means off"""
    order = ["error", "warn", "info", "debug", "trace"]
    best, best_len, default = None, -1, None
    for d in (x.strip() for x in spec.split(",") if x.strip()):
        if "=" not in d:
            if d in order:
                default = d
            continue
        name, lv = d.split("=", 1)
        if lv in order and (module == name or module.startswith(name + "::")) and len(name) > best_len:
            best, best_len = lv, len(name)
    lv = best or default
    return lv is not None and order.index(level) <= order.index(lv)


# #1532: environment variables that must reach the app under test even where the launcher does not inherit os.environ (macOS `open --env`).
# Linux and Windows pass the whole environment. APPCI_APP_ENV="K=V,K2=V2" (run.py) sets more of them for one run.
PASSTHROUGH_ENV = ("PHOSPHOR_QUEUE_DESTINATION", "PHOSPHOR_CONPTY_ENTER_HOLD", "PHOSPHOR_EDITOR_TRIM_AFTER_OPEN", "PHOSPHOR_HEAP_TRIM", "PHOSPHOR_HEAP_TRIM_ON_CLOSE")  # the last three: editor_memory_7mb trim A/B and the close-trim kill switch (#1607)


def passthrough_env(environ=None):
    environ = os.environ if environ is None else environ
    keys = list(PASSTHROUGH_ENV) + [kv.split("=", 1)[0] for kv in environ.get("APPCI_APP_ENV", "").split(",") if "=" in kv]
    return {k: environ[k] for k in keys if k in environ and environ[k] != ""}


# where the CI users keep shells (nu in ~/.local/bin on the box). On the app's own PATH, as on a real user's machine: added only inside the pane
# (`export PATH`), the app's classifier does not know the command, and agent view sends a bare `nu` to the agent as a prompt (r18cz Linux).
USER_BIN_DIRS = ("~/.local/bin", "/home/gitlab-runner/.local/bin", "/opt/homebrew/bin", "~/.cargo/bin")


def with_user_bin_dirs(path, isdir=os.path.isdir):
    """PATH plus every existing USER_BIN_DIRS entry it lacks, appended (the system's own directories keep precedence)"""
    have = path.split(os.pathsep) if path else []
    extra = [d for d in (os.path.expanduser(x) for x in USER_BIN_DIRS) if isdir(d) and d not in have]
    return os.pathsep.join(have + list(dict.fromkeys(extra)))


# #1805: how the app's private D-Bus session offers a Secret Service. "unlocked" (the default) starts gnome-keyring-daemon unlocked, as
# every case expects; the other two give the session NO keyring daemon, through a bus config with no standard service directories (so a
# gnome-keyring .service file on the box cannot be auto-started):
#   "absent"  nothing provides org.freedesktop.secrets: every call fails at once (ServiceUnknown);
#   "hang"    a .service file whose Exec never claims the name, so a call waits for the bus's service_start_timeout (120 s, the
#             window the waste audit saw block startup). Only the Secret Service is affected; other activatable services are also
#             missing in both variants (portals, notifications), which the app tolerates.
KEYRING_VARIANTS = ("unlocked", "absent", "hang")

DBUS_CONFIG = """<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <keep_umask/>
  <listen>unix:tmpdir=/tmp</listen>
  <auth>EXTERNAL</auth>
%(servicedir)s  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
  <limit name="service_start_timeout">120000</limit>
</busconfig>
"""

HANG_SERVICE = """[D-BUS Service]
Name=org.freedesktop.secrets
Exec=/bin/sleep 600
"""


def keyring_dbus_files(variant, run):
    """-> {path: text} the D-Bus session of `variant` needs under the run dir (empty for "unlocked": the default session)"""
    if variant not in KEYRING_VARIANTS:
        raise ValueError("unknown keyring variant %r (one of %s)" % (variant, ", ".join(KEYRING_VARIANTS)))
    if variant == "unlocked":
        return {}
    files, servicedir = {}, ""
    if variant == "hang":
        sdir = os.path.join(run, "dbus-services-hang")
        files[os.path.join(sdir, "org.freedesktop.secrets.service")] = HANG_SERVICE
        servicedir = "  <servicedir>%s</servicedir>\n" % sdir
    files[os.path.join(run, "dbus-session-%s.conf" % variant)] = DBUS_CONFIG % {"servicedir": servicedir}
    return files


def launcher_argv(variant, run, inner):
    """the dbus-run-session command line for `variant` (see keyring_dbus_files)"""
    if variant == "unlocked":
        return ["dbus-run-session", "--", inner]
    return ["dbus-run-session", "--config-file=%s" % os.path.join(run, "dbus-session-%s.conf" % variant), "--", inner]


class App:
    def __init__(self, run, home, binary, display="wayland"):
        self.run, self.home, self.bin, self.display = run, home, binary, display
        self.proc = None
        self.wrap = ""                 # words put before the binary in the launcher's final exec (zero_network_startup: strace), "" otherwise; ends in a space
        self.keyring = "unlocked"      # #1805: a case sets another KEYRING_VARIANTS entry before start() and puts it back after
        self.extra_env = {}            # {K: V} added to the NEXT launch's environment (a case sets it before a relaunch: the render-latency present-mode A/B)

    def start(self):
        inner = os.path.join(self.run, "inner.sh")
        open(inner, "w").write(INNER % {"home": self.home, "run": self.run, "bin": self.bin, "wrap": self.wrap})
        os.chmod(inner, 0o755)
        # r18u22 (Wayland agent shell_nu): the r18cz fix below lived only in WindowsApp.env (its `os.name != "nt"` branch never ran), so on
        # Linux nu stayed off the app's PATH and the agent pass sent `nu` to the agent: "nu never executed a probe line".
        env = dict(os.environ, APPCI_APP_DISPLAY=self.display, PATH=with_user_bin_dirs(os.environ.get("PATH", "")))
        env.pop("APPCI_NO_KEYRING", None)
        env.update(self.extra_env)
        for path, text in keyring_dbus_files(self.keyring, self.run).items():
            os.makedirs(os.path.dirname(path), exist_ok=True)
            open(path, "w").write(text)
        if self.keyring != "unlocked":
            env["APPCI_NO_KEYRING"] = "1"
        try:
            os.remove(os.path.join(self.run, "app.pid"))
        except FileNotFoundError:
            pass
        # own session => own process group; it is the ONLY thing stop() ever signals (the "hang" variant's sleep is in it too)
        self.proc = subprocess.Popen(launcher_argv(self.keyring, self.run, inner), env=env, start_new_session=True,
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


def find_hook_env(home):
    """{name: value} that arms the app's find hook (#1789) for a launch whose scratch home is `home`: the app logs `appci_find token=...` for the
    tokens cases_ui.register_find appends to that file. Empty with APPCI_NO_FIND_HOOK=1 (the kill switch cases_ui._hook_word also reads).
    #2082: the macOS launcher never set it (RUST_LOG enabled the target, the variable was missing), so a macOS app logged no position at all."""
    if os.environ.get("APPCI_NO_FIND_HOOK") == "1":
        return {}
    return {"PHOSPHOR_APPCI_FIND_FILE": os.path.join(home, "appci_find.txt")}


class MacApp:
    """macOS: `open -n` the extracted .app (a proper GUI launch, so it can become frontmost) with a PRIVATE HOME
    (`open --env HOME=...`, the method the hand-run round-18 harness proved): the app derives every path from it, so the
    settings file is <home>/.phosphor/settings.toml (warp_core::paths::config_local_dir, Channel::Oss on macOS), the log is
    <home>/Library/Logs/phosphor.log, and the runner user's real ~/.phosphor, Library and Keychain-free state are never read
    or written. Nothing to back up or restore. Only the exact PID found from the bundle's executable path is ever signalled."""

    def __init__(self, run, home, app_bundle, binary, display=None):
        self.run, self.home, self.bundle, self.bin = run, home, app_bundle, binary
        self.settings = os.path.join(home, ".phosphor", "settings.toml")
        self._installed_src = None       # the profile settings.toml bytes last copied over self.settings (see _install_settings)
        self._pid = None
        self.last_pid = None
        self.warn = None
        self.extra_env = {}              # {K: V} passed to the NEXT launch through `open --env` (a case sets it before a relaunch, #1838 A/B)
        self.log_path = os.path.join(home, "Library", "Logs", "phosphor.log")

    def _install_settings(self):
        """Copy the profile's settings.toml to the app's path when the PROFILE file changed since the last copy (or the app's file is missing).
        r18u22: this copied on EVERY start(), so a case that wrote the app's settings file and relaunched (cb_settings_migration: a legacy file,
        then a truncated one) had it replaced by the profile copy before the app read it. The Mac pass therefore never tested either file: it
        passed with truncated_file_unchanged False and legacy_entries_kept 0 while Linux, which reads the profile path directly, saw both.
        A case's own write now survives its relaunch, as it does on Linux; a new pass (profile.build rewrites the source) still installs."""
        import shutil
        src = os.path.join(self.home, ".config/phosphor/settings.toml")     # written by profile.py (same schema on every OS)
        data = open(src, "rb").read()
        if data == self._installed_src and os.path.isfile(self.settings):
            return
        os.makedirs(os.path.dirname(self.settings), exist_ok=True)
        shutil.copy2(src, self.settings)
        os.chmod(self.settings, 0o600)
        self._installed_src = data

    def start(self):
        self._install_settings()
        os.makedirs(os.path.dirname(self.log_path), exist_ok=True)
        env_args = []
        for k, v in (("HOME", self.home), ("SHELL", "/bin/zsh"), ("ZDOTDIR", self.home), ("APPCI_W", self.home + "/w"),
                     ("PA_CTL", os.path.join(self.run, "ctl")),
                     ("RUST_LOG", "warp::input_order=debug,warp::terminal::model_lock_trace=debug,warpui::frame_cause=debug,warpui::appci_find=info,phosphor::routing=debug,editor_memory=debug" + MAC_RUST_LOG_MORE),
                     ("WARP_INPUT_ORDER_LOG_TEXT", "1"),
                     ("WARP_INPUT_ORDER_LOG_KEY_BYTES_MAY_INCLUDE_PASSWORDS", "1")):      # #2040: user-key bytes in the log; synthetic text only
            env_args += ["--env", "%s=%s" % (k, v)]
        for k, v in find_hook_env(self.home).items():            # #2082: the find hook is armed on macOS too; #2061: the macOS pointer cases read it. The ONE place it is set (APPCI_NO_FIND_HOOK=1 leaves it unset)
            env_args += ["--env", "%s=%s" % (k, v)]
        for k, v in passthrough_env().items():
            env_args += ["--env", "%s=%s" % (k, v)]
        env_args += ["--env", "PHOSPHOR_APPCI_GRID_DIR=%s" % os.path.join(self.home, "appci_grid")]      # #2075 (T34): the grid snapshot hook, as in INNER
        for k, v in self.extra_env.items():
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
        self.extra_env = {}        # {K: V} added to the NEXT launch's environment (#1962 follow-up: a case sets it before a relaunch, like App/MacApp)
        self.proc = None
        self.job = None            # #1642: Win32 job handle (KILL_ON_JOB_CLOSE) around the app
        self.exit_code = None
        self.config = os.path.join(os.environ["LOCALAPPDATA"], "phosphor", "Phosphor", "config")
        # #1401: pwsh.ps1's hook timing marks go here while `<file>.on` exists (cases.py pwsh_hook_timing turns it on for a measurement only)
        self.hook_timing_path = os.path.join(self.home, "pwsh_hook_timing.txt")

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
        # r18u13 (#1629): winit too. Its keyboard-layout trace (WM_INPUTLANGCHANGE enter/exit, LAYOUT_CACHE takes, layout-build timing; lib/winit
        # CHANGES-PHOSPHOR.md) logs at info, and under the bare `warn` base the AltGr hang run carried none of it. winit logs nothing else through `log`.
        e["RUST_LOG"] = WINDOWS_RUST_LOG
        e["WARP_INPUT_ORDER_LOG_TEXT"] = "1"
        e["WARP_INPUT_ORDER_LOG_KEY_BYTES_MAY_INCLUDE_PASSWORDS"] = "1"      # #2040: `hop=pty_write_bytes class=UserKey bytes="..."` (line_evidence, the Ctrl-Break sequence, agent prose never typed into the pty); synthetic text only
        e["PA_CTL"] = fwd(self.ctl)
        e["APPCI_W"] = fwd(os.path.join(self.home, "w"))
        e["PHOSPHOR_PWSH_HOOK_TIMING"] = self.hook_timing_path
        # #1789: tokens the harness wants located; an app with the env-gated hook (warpui_core::appci_find) logs `appci_find token=.. x0= y0= x1= y1=` for each. An app
        # without the hook ignores both variables and the cases fall back to OCR (cases_ui.hook_find).
        e["PHOSPHOR_APPCI_FIND_FILE"] = fwd(os.path.join(self.home, "appci_find.txt"))
        e["PHOSPHOR_APPCI_GRID_DIR"] = fwd(os.path.join(self.home, "appci_grid"))      # #2075 (T34): the grid snapshot hook, as in INNER
        tail = os.pathsep.join(winapi.git_dirs())
        e["PATH"] = os.path.join(self.home, "bin") + os.pathsep + e.get("PATH", "") + (os.pathsep + tail if tail else "")
        if os.name != "nt":
            e["PATH"] = with_user_bin_dirs(e["PATH"])       # USER_BIN_DIRS (the Linux App does the same)
        if self.pane == "gitbash":
            e["HOME"] = self.home
            e["SHELL"] = "/usr/bin/bash"
            e["CHERE_INVOKING"] = "1"
        e.update(self.extra_env)        # last: an A/B arm's variable wins; _spawn hands this dict to CreateProcess, so the app (not a wrapper) reads it
        return e

    def start(self):
        self._install_settings()
        os.makedirs(os.path.join(self.home, "w"), exist_ok=True)
        out = open(os.path.join(self.run, "app.stdout"), "ab")          # python opens it non-inheritable; Popen hands the child only its std handles
        self.exit_code = None
        self.proc = self._spawn(out)
        out.close()                                                      # the child has its own duplicate
        with open(os.path.join(self.run, "pids"), "a") as f:
            f.write("app %d\n" % self.proc.pid)
        return self.proc.pid

    def _spawn(self, out):
        """#1642: the app starts suspended, joins a KILL_ON_JOB_CLOSE job, then runs, so no child (conhost, shells) can exist outside the job. Without a
        job (creation failed, or this process may not assign) it starts plain, as before, and teardown falls back to taskkill /T."""
        import winapi
        self.job = None
        job = winapi.job_create() if os.name == "nt" else None
        flags = getattr(subprocess, "CREATE_SUSPENDED", winapi.CREATE_SUSPENDED) if job else 0
        proc = subprocess.Popen([self.bin], cwd=os.path.join(self.home, "w"), env=self.env(), stdout=out, stderr=subprocess.STDOUT, creationflags=flags)
        if not job:
            print("WindowsApp: no job object; teardown uses taskkill /T only")
            return proc
        try:
            if winapi.job_assign(job, int(proc._handle)):
                self.job = job
            else:
                print("WindowsApp: AssignProcessToJobObject failed; teardown uses taskkill /T only")
                winapi.job_close(job)
        finally:
            if not winapi.resume_process(int(proc._handle)):
                proc.kill()
                raise RuntimeError("could not resume the suspended app (pid %d)" % proc.pid)
        return proc

    def _record_job_members(self):
        """append '<kind> <pid> <creation FILETIME>' for every process the job still holds, so after_job.ps1 can finish what teardown could not"""
        import winapi
        pids = winapi.job_pids(self.job)
        with open(os.path.join(self.run, "pids"), "a") as f:
            for pid in pids:
                ct = winapi.process_creation_filetime(pid)
                if ct is not None:
                    f.write("jobmember %d %d\n" % (pid, ct))
        return pids

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
            self.close_job()
            return
        if p.poll() is None:
            if self.job:
                try:
                    self._record_job_members()
                except Exception as e:
                    print("WindowsApp: job member listing failed: %s" % e)
            subprocess.run(["taskkill", "/PID", str(p.pid), "/T", "/F"], capture_output=True, timeout=30)
            try:
                p.wait(10)
            except Exception:
                pass
        self.close_job()

    def close_job(self):
        """closing the handle kills everything still in the job (the app's conhost and shells that taskkill /T missed)"""
        job, self.job = getattr(self, "job", None), None
        if job:
            import winapi
            winapi.job_close(job)

    def clk_tck(self):
        return 10_000_000          # FILETIME units (100 ns)

    def cpu_ticks(self):
        import winapi
        return winapi.process_times(self.proc.pid)

    def private_kib(self):
        """private committed bytes of the app process in KiB (PROCESS_MEMORY_COUNTERS_EX.PrivateUsage; #1340: the working set is trimmed by the OS
        and shared with other processes, so it is reported but not the figure the editor memory case judges)"""
        import winapi
        return winapi.process_times_private(self.proc.pid)[2]

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
