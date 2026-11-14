# appci/cases_install.py: round 15, wave 5 part B of docs/design/r14-test-gaps.md 7.1 (#2076, #1546). Design note: docs/design/r15-w5b.md;
# docs/testing.md 2.25. Both cases are in cases.PROVING (terminal mode; on Windows once, in the PowerShell pane).
#   installer_smoke                 T43. The artefact the build made is INSTALLED into a directory of the job (Linux: the .deb unpacked with its
#                                   launcher, else the AppImage extracted; macOS: the .app copied out of the .dmg into a scratch Applications folder;
#                                   Windows: the installer run silently), launched to a prompt, a second launch joins the first instance (it exits 0
#                                   and the first instance opens the window it asked for), `phosphor <path>` opens the file (absolute and relative;
#                                   control: with PHOSPHOR_CLI_PATH_ARGS=0 the same line opens nothing), and the uninstall leaves the user's data
#                                   byte for byte. The build under test is restarted at the end.
#   upgrade_from_previous_release   T44. The previous release (prev_artifact.py: cache, a download from this project's package registry, or the
#                                   machine-local build) is run on the profile and the new build upgrades it: upgrade_over_previous' checks on every
#                                   platform, and on Linux the settings table of upgrade_from_v0_1_17 while the previous release is v0.1.17.
# A case that cannot run says what the machine or the job lacks ("NEEDS: ..."), it never passes.
import contextlib, os, re, shutil, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases as C  # noqa: E402  (cases.py imports this module as its last line)
import cases_coverage_b as CB  # noqa: E402
import cases_robust as R  # noqa: E402
import cases_ui as UI  # noqa: E402
import cases_upgrade as CU  # noqa: E402
import prev_artifact as PA  # noqa: E402
import prev_release  # noqa: E402
import wininstall  # noqa: E402
from cases import case  # noqa: E402

DIST_NEEDS = ("NEEDS: the directory of build artefacts in APPCI_DIST (the app-test wrappers export it: linux/wayland_session.sh --dist, mac/ci/app_test.sh, "
              "windows/ci/app_test.ps1 -Dist). A run started with --bin alone has no installer to test")
KILL_SWITCH = "PHOSPHOR_CLI_PATH_ARGS"                                   # #2083 (crates/warp_cli/src/lib.rs CLI_PATH_ARGS_ENV)
RECEIVED_URL_RE = re.compile(r"received url scheme=(\S+) host=(\S+) path=(\S+)")      # app/src/uri/mod.rs handle_incoming_uri (release channels log these three fields)
USER_DATA_WORDS = ("$HOME", "${HOME}", "~/", "/home/", "/root/", ".config/phosphor", ".local/share/phosphor", ".local/state/phosphor", "getent passwd")
HELPER_ROLES = ("terminal-server",)                                      # the app runs helpers from its own executable (app.MacApp._is_helper)


# ---------------------------------------------------------------- pure helpers (selftest `r15_installer_pure_parts`)
def plat_key(ctx):
    return "windows" if ctx.win else ("macos" if ctx.be.name == "mac" else "linux")


def find_artifacts(plat, names):
    """what the build's dist directory holds for this platform: {kind: file name}. Linux: deb and / or appimage; macOS: dmg; Windows: setup."""
    low = sorted(names)
    out = {}
    if plat == "linux":
        for kind, suffix in (("deb", ".deb"), ("appimage", ".appimage")):
            hit = [n for n in low if n.lower().endswith(suffix)]
            if hit:
                out[kind] = hit[0]
    elif plat == "macos":
        hit = [n for n in low if n.lower().endswith(".dmg")]
        if hit:
            out["dmg"] = hit[0]
    else:
        s = wininstall.find_setup(low)
        if s:
            out["setup"] = s
    return out


def artifact_needs(plat):
    what = {"linux": "a .deb or an .AppImage (build-linux-x86_64 makes both)", "macos": "a .dmg (build-macos-arm64)",
            "windows": "the installer .exe (build-windows-x86_64)"}[plat]
    return "NEEDS: %s in the APPCI_DIST directory of this job; the build this run tests did not produce one there" % what


def desktop_entry(text):
    """the [Desktop Entry] keys of a .desktop file -> dict (comments and other groups left out)"""
    out, on = {}, False
    for line in (text or "").splitlines():
        s = line.strip()
        if s.startswith("["):
            on = s == "[Desktop Entry]"
        elif on and s and not s.startswith("#") and "=" in s:
            k, v = s.split("=", 1)
            out[k.strip()] = v.strip()
    return out


def deb_layout_problems(entry, launcher_shipped, launcher_in_postinst, icon_found, binary_found):
    """what is wrong with an unpacked .deb as an INSTALL: the desktop entry must start a command the package provides (shipped under usr/bin or made
    by its postinst), take a file or URL argument, and name an icon the package ships; the app binary must be there. -> list of sentences."""
    bad = []
    if not binary_found:
        bad.append("the package holds no executable phosphor-oss")
    if not entry:
        return bad + ["the package ships no .desktop file under usr/share/applications: no launcher in the desktop's menu"]
    words = entry.get("Exec", "").split()
    if not words:
        bad.append("the .desktop file has no Exec line")
    else:
        if not (launcher_shipped or launcher_in_postinst):
            bad.append("the .desktop file starts `%s`, which the package neither ships under usr/bin nor creates in its postinst: a dead launcher" % words[0])
        if not any(w in ("%U", "%u", "%F", "%f") for w in words[1:]):
            bad.append("the .desktop Exec line takes no file or URL (%%U / %%F): a file opened from the desktop would start the app without it (Exec=%s)" % entry.get("Exec"))
    if entry.get("Icon") and not icon_found:
        bad.append("the .desktop file names the icon %r, which the package does not ship" % entry.get("Icon"))
    return bad


def script_touches_user_data(text):
    """the lines of a package removal script (prerm / postrm) that name a user's home or the app's per-user directories: an uninstall must not"""
    out = []
    for line in (text or "").splitlines():
        s = line.strip()
        if s and not s.startswith("#") and any(w in s for w in USER_DATA_WORDS):
            out.append(s[:160])
    return out


def received_urls(log):
    """[(scheme, host, path)] of every URL the running instance logged as received"""
    return RECEIVED_URL_RE.findall(log or "")


def joined(log):
    """the running instance was asked for a new window by a second launch (app_services: a launch with no URL forwards <scheme>://action/new_window)"""
    return any(s != "file" and h == "action" and p == "/new_window" for s, h, p in received_urls(log))


def file_received(log, name):
    """the running instance received a file URL whose last path component is `name` (plain characters only, so no percent-encoding is in play)"""
    return any(s == "file" and p.endswith("/" + name) for s, _h, p in received_urls(log))


def second_launch_verdict(res, what):
    """a second launch must hand over and EXIT 0 by itself. -> failure sentence or None. res = launch_again()'s dict."""
    if res.get("error"):
        return "%s: the second launch could not be started: %s" % (what, res["error"])
    if res["timed_out"]:
        return ("%s: the second launch did not exit within %d s (killed): it did not hand over to the running instance, or it became a second app on the "
                "same profile" % (what, res["timeout"]))
    if res["rc"] != 0:
        return "%s: the second launch exited with code %s, expected 0%s" % (what, res["rc"], (": " + res["err"]) if res.get("err") else "")
    return None


def kept_verdict(before, after):
    """user data across an uninstall: -> (files gone, files changed). before / after = cases_upgrade.tree_hashes maps."""
    return sorted(k for k in before if k not in after), sorted(k for k in before if k in after and after[k] != before[k])


def user_data_present(hashes):
    """is there user data to lose at all? A settings file and a database: without both, 'the uninstall kept everything' proves nothing"""
    keys = [k.lower() for k in hashes]
    return any(k.endswith("settings.toml") for k in keys) and any(k.endswith((".sqlite", ".db", ".sqlite3")) for k in keys)


def instance_count(cmdlines, exe):
    """app INSTANCES among process command lines [(exe path, argv list)]: processes of executable `exe` that are not one of the app's own helpers"""
    return len([1 for e, argv in cmdlines if e == exe and not (len(argv) > 1 and argv[1] in HELPER_ROLES)])


# ---------------------------------------------------------------- shared runtime helpers
@contextlib.contextmanager
def swapped_app(ctx, other):
    """ctx.app is `other` inside the block (the frontmost-window guard follows it), the app under test again after it. `other` is stopped on the way out."""
    mine = ctx.app
    R.use_app(ctx, other)
    try:
        yield other
    finally:
        try:
            other.stop()
        except Exception as e:
            print("[appci] could not stop the swapped-in app: %r" % (e,), flush=True)
        R.use_app(ctx, mine)


def _proc_env(pid):
    env = {}
    for kv in open("/proc/%d/environ" % pid, "rb").read().split(b"\0"):
        if b"=" in kv:
            k, v = kv.split(b"=", 1)
            env[k.decode(errors="replace")] = v.decode(errors="replace")
    return env


def launch_again(ctx, args, extra_env=None, timeout=30):
    """Start the running app's own executable a second time, as a user's second launch would (same session, same profile), from the case directory.
    -> dict(rc, timed_out, timeout, err, error). Linux: the first instance's environment (its D-Bus session) without a display, so a launch that does
    not hand over cannot open a second window. macOS: the scratch HOME and the user's own temporary directory (where the app keeps its socket when
    the state path is too long). Windows: the launcher's environment. A launch that does not exit is killed by its exact pid."""
    res = dict(rc=None, timed_out=False, timeout=timeout, err="", error=None)
    try:
        if ctx.win:
            env = dict(ctx.app.env())
        elif ctx.be.name == "mac":
            env = dict(os.environ, HOME=ctx.app.home, ZDOTDIR=ctx.app.home)
            t = subprocess.run(["getconf", "DARWIN_USER_TEMP_DIR"], capture_output=True, text=True, timeout=10).stdout.strip()
            if t:
                env["TMPDIR"] = t
        else:
            env = _proc_env(ctx.app.pid())
            for k in ("DISPLAY", "WAYLAND_DISPLAY"):
                env.pop(k, None)
            if not env.get("DBUS_SESSION_BUS_ADDRESS"):
                res["error"] = "the running app has no DBUS_SESSION_BUS_ADDRESS in its environment"
                return res
        env.update(extra_env or {})
        kw = {} if ctx.win else {"start_new_session": True}
        p = subprocess.Popen([ctx.app.bin] + list(args), env=env, cwd=ctx.W, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, **kw)
    except Exception as e:
        res["error"] = repr(e)
        return res
    with open(os.path.join(ctx.run, "pids"), "a") as f:
        f.write("second-launch %d\n" % p.pid)
    try:
        _out, err = p.communicate(timeout=timeout)
        res["rc"], res["err"] = p.returncode, (err or b"").decode(errors="replace").strip().replace("\n", " | ")[:300]
    except subprocess.TimeoutExpired:
        res["timed_out"] = True
        if ctx.win:
            subprocess.run(["taskkill", "/PID", str(p.pid), "/T", "/F"], capture_output=True, timeout=30)
        else:
            try:
                os.killpg(p.pid, 9)
            except OSError:
                pass
        try:
            p.communicate(timeout=10)
        except Exception:
            pass
    return res


def linux_instances(real):
    """app instances of the executable `real` (the /proc/<pid>/exe of the running app) on this machine, from /proc"""
    rows = []
    for d in os.listdir("/proc"):
        if d.isdigit():
            try:
                exe = os.readlink("/proc/%s/exe" % d)
                argv = [a.decode(errors="replace") for a in open("/proc/%s/cmdline" % d, "rb").read().split(b"\0") if a]
            except OSError:
                continue
            rows.append((exe, argv))
    return instance_count(rows, real)


APP_DATA_SUBDIRS = (".config/phosphor", ".phosphor") + tuple(CB.STATE_SUBDIRS)


def app_data_roots(ctx):
    """the directories that hold the APP's per-user data (settings, databases, themes, keybindings): on Windows the known folders, elsewhere the
    app's directories under the scratch home. Not the whole home: a shell that exits late still writes its own history file there."""
    if ctx.win:
        return R.data_roots(ctx)
    return [p for p in (os.path.join(ctx.prof.home, *sub.split("/")) for sub in APP_DATA_SUBDIRS) if os.path.isdir(p)]


class Installed:
    """an installed copy: the executable a user starts, the macOS bundle (else None), and how to remove it again (-> problem or None)"""

    def __init__(self, binary, bundle, root, remove, how):
        self.bin, self.bundle, self.root, self.remove, self.how = binary, bundle, root, remove, how


def _rm_tree(path):
    def go():
        shutil.rmtree(path, ignore_errors=True)
        return "could not remove %s" % path if os.path.exists(path) else None
    return go


# ---------------------------------------------------------------- the three installs
def install_linux(ctx, r, dist, arts, sandbox):
    root = os.path.join(sandbox, "root")
    if arts.get("deb") and shutil.which("dpkg-deb"):
        deb = os.path.join(dist, arts["deb"])
        ctl = os.path.join(sandbox, "DEBIAN")
        for argv in (["dpkg-deb", "-x", deb, root], ["dpkg-deb", "-e", deb, ctl]):
            p = subprocess.run(argv, capture_output=True, text=True, timeout=600)
            if not r.check(p.returncode == 0, "%s failed (exit %s): %s" % (" ".join(argv[:2]), p.returncode, p.stderr.strip()[:200])):
                return None
        binary = None
        for base, _d, files in os.walk(root):
            if "phosphor-oss" in files and os.access(os.path.join(base, "phosphor-oss"), os.X_OK) and not os.path.islink(os.path.join(base, "phosphor-oss")):
                binary = os.path.join(base, "phosphor-oss")
                break
        apps = os.path.join(root, "usr", "share", "applications")
        desk = sorted(n for n in (os.listdir(apps) if os.path.isdir(apps) else []) if n.endswith(".desktop"))
        entry = desktop_entry(open(os.path.join(apps, desk[0]), errors="replace").read()) if desk else {}

        def script(name):
            try:
                return open(os.path.join(ctl, name), errors="replace").read()
            except OSError:
                return ""
        cmd = (entry.get("Exec", "").split() or ["phosphor"])[0]
        launcher = os.path.join(root, "usr", "bin", os.path.basename(cmd))
        shipped = os.path.lexists(launcher)
        in_postinst = ("/usr/bin/" + os.path.basename(cmd)) in script("postinst")
        icon = entry.get("Icon", "")
        icon_found = any(os.path.splitext(f)[0] == icon for base in ("icons", "pixmaps") for _b, _d, fs in os.walk(os.path.join(root, "usr", "share", base)) for f in fs)
        for bad in deb_layout_problems(entry, shipped, in_postinst, icon_found, bool(binary)):
            r.check(False, "the .deb as installed: " + bad)
        for name in ("prerm", "postrm"):
            hits = script_touches_user_data(script(name))
            r.check(not hits, "the package's %s script names a user's home or the app's per-user directories (an uninstall must leave user data alone): %s" % (name, hits[:3]))
        if not binary:
            return None
        if not shipped and in_postinst:        # what the postinst does on a real system: the user-facing command is a symlink to the binary
            os.makedirs(os.path.dirname(launcher), exist_ok=True)
            os.symlink(binary, launcher)
        start = launcher if os.path.lexists(launcher) else binary
        r.metric("installed_from", ".deb (dpkg-deb -x into the job directory; launched through %s)" % os.path.relpath(start, root))
        return Installed(start, None, root, _rm_tree(root), "deb")
    if arts.get("appimage"):
        os.makedirs(root, exist_ok=True)
        img = os.path.join(root, "Phosphor.AppImage")
        shutil.copyfile(os.path.join(dist, arts["appimage"]), img)
        os.chmod(img, 0o755)
        p = subprocess.run([img, "--appimage-extract"], cwd=root, capture_output=True, text=True, timeout=600)
        run = os.path.join(root, "squashfs-root", "AppRun")
        if not r.check(p.returncode == 0 and os.access(run, os.X_OK), "the AppImage did not extract to squashfs-root/AppRun (exit %s): %s" % (p.returncode, p.stderr.strip()[:200])):
            return None
        r.metric("installed_from", "AppImage (extracted into the job directory; launched through AppRun)")
        return Installed(run, None, root, _rm_tree(root), "appimage")
    r.skip("NEEDS: dpkg-deb on this runner to unpack the .deb (the only artefact in APPCI_DIST); the harness installs nothing")


def install_macos(ctx, r, dist, arts, sandbox):
    mnt, apps_dir = os.path.join(sandbox, "mnt"), os.path.join(sandbox, "Applications")
    os.makedirs(mnt, exist_ok=True)
    os.makedirs(apps_dir, exist_ok=True)
    p = subprocess.run(["hdiutil", "attach", os.path.join(dist, arts["dmg"]), "-nobrowse", "-readonly", "-mountpoint", mnt, "-quiet"], capture_output=True, text=True, timeout=300)
    if not r.check(p.returncode == 0, "the disk image could not be mounted (exit %s): %s" % (p.returncode, p.stderr.strip()[:200])):
        return None
    try:
        apps = sorted(n for n in os.listdir(mnt) if n.endswith(".app"))
        r.metric("dmg_has_applications_link", os.path.islink(os.path.join(mnt, "Applications")))
        if not r.check(len(apps) == 1, "the disk image holds %d .app bundles, expected exactly one: %s" % (len(apps), apps)):
            return None
        dst = os.path.join(apps_dir, apps[0])
        shutil.rmtree(dst, ignore_errors=True)
        c = subprocess.run(["cp", "-R", os.path.join(mnt, apps[0]), dst], capture_output=True, text=True, timeout=600)
        if not r.check(c.returncode == 0, "the .app could not be copied out of the disk image: %s" % c.stderr.strip()[:200]):
            return None
    finally:
        subprocess.run(["hdiutil", "detach", mnt, "-quiet"], capture_output=True, timeout=120)
    subprocess.run(["xattr", "-dr", "com.apple.quarantine", dst], capture_output=True, timeout=120)
    mac = os.path.join(dst, "Contents", "MacOS")
    bins = sorted(os.listdir(mac)) if os.path.isdir(mac) else []
    if not r.check(bool(bins), "the copied .app has no executable under Contents/MacOS"):
        return None
    r.metric("installed_from", ".dmg (the .app copied into a scratch Applications folder of the job)")
    return Installed(os.path.join(mac, bins[0]), dst, dst, _rm_tree(dst), "dmg")


def install_windows(ctx, r, dist, arts, sandbox):
    dest = os.path.join(sandbox, "app")
    exe, bad = wininstall.install(os.path.join(dist, arts["setup"]), dest, os.path.join(sandbox, "install.log"))
    if not r.check(bad is None, "silent install: %s" % bad):
        return None
    names = os.listdir(dest)
    r.check(wininstall.find_uninstaller(names) is not None, "the install directory holds no unins*.exe: the app cannot be removed from Settings > Apps")
    r.check(os.path.isdir(os.path.join(dest, "resources")), "the install directory holds no resources folder")
    q = wininstall._reg("query", wininstall.UNINSTALL_KEY)
    loc = wininstall.install_location(q.stdout)
    r.check(q.returncode == 0 and wininstall.same_dir(loc, dest), "the uninstall entry of the user does not point at this install (InstallLocation %r, installed into %r)" % (loc, dest))
    r.metric("installed_from", "the installer, run silently for the current user into the job directory")

    def remove():
        return wininstall.uninstall(dest, os.path.join(sandbox, "uninstall.log"))
    return Installed(exe, None, dest, remove, "setup")


INSTALL = {"linux": install_linux, "macos": install_macos, "windows": install_windows}


# ---------------------------------------------------------------- T43 installer_smoke
@case("installer_smoke", modes=("terminal",), order=93, gui=True)
def c_installer_smoke(ctx, r):
    """T43: install the build's artefact into a directory of the job and use it as a user does. (1) the install has what a user needs to start and
    remove it (Linux .deb: desktop entry, launcher, icon, removal scripts that leave user data alone; Windows: uninstaller and uninstall entry);
    (2) the installed copy reaches a prompt and takes exact typing; (3) a second launch exits 0 and the first instance opens the window it asked
    for, one instance stays; (4) `phosphor <absolute path>` and `phosphor <relative path>` reach the running instance as that file (#2083), and the
    control with PHOSPHOR_CLI_PATH_ARGS=0 opens nothing; (5) after the uninstall the installed program is gone and the user's data (settings,
    database, everything in the profile) is unchanged byte for byte; (6) the build under test starts again on that data."""
    R.need_relaunch_app(ctx, "installer_smoke")
    plat = plat_key(ctx)
    dist = os.environ.get("APPCI_DIST", "")
    if not dist or not os.path.isdir(dist):
        r.skip(DIST_NEEDS)
    arts = find_artifacts(plat, os.listdir(dist))
    if not arts:
        r.skip(artifact_needs(plat))
    if plat == "linux" and not hasattr(ctx.app, "proc"):
        r.skip("needs the Linux App launcher (not the sim backend)")
    sandbox = os.path.join(ctx.run, "install_smoke")
    shutil.rmtree(sandbox, ignore_errors=True)
    os.makedirs(sandbox, exist_ok=True)
    new_app = ctx.app
    snap = None
    try:
        ctx.snapshot_log()
        new_app.stop()                                    # one instance per profile; the Windows installer refuses to run beside a running Phosphor
        if plat == "windows":
            snap = wininstall.Snapshot(os.path.join(sandbox, "machine_state"))
        inst = INSTALL[plat](ctx, r, dist, arts, sandbox)
        if inst is None:
            return
        with swapped_app(ctx, CB.old_app_like(new_app, inst.bin, inst.bundle)):
            # ---- (2) launch to a prompt
            _n, up = R.start_app(ctx, r, "is_first", restored_min=1)
            if not r.check(up, "the installed copy (%s) did not reach a shell prompt" % inst.how):
                return
            if plat == "macos" and hasattr(ctx.be, "system_dialog"):
                d = ctx.be.system_dialog(ocr=True)        # a system prompt for the new bundle path would take every key
                if d:
                    raise ctx.be.needs_owner(d)
            time.sleep(2)
            ctx.be.key("Escape")
            exe = None
            if plat == "linux":
                exe = os.readlink("/proc/%d/exe" % ctx.app.pid())
                r.check(exe.startswith(os.path.realpath(inst.root) + os.sep), "the app that reached the prompt is %s, not the installed copy under %s" % (exe, inst.root))
            UI.exact_burst(ctx, r, "is_first", 3)
            ctx.shot("installer_smoke_first")
            # ---- (3) a second launch joins the first instance
            off, p0 = ctx.log_size(), ctx.prompts()
            res = launch_again(ctx, [])
            bad = second_launch_verdict(res, "second launch")
            r.check(bad is None, bad or "")
            if bad is None:
                got = ctx.until(lambda: joined(ctx.log_since(off)), 15, step=0.3)
                r.check(bool(got), "the second launch exited 0 but the running instance logged no request for a new window (`received url ... host=action "
                                   "path=/new_window`): the launch was not handed over. URLs it received: %r" % (received_urls(ctx.log_since(off))[:3],))
                if ctx.prof.local_prompts:
                    r.check(bool(ctx.until(lambda: ctx.prompts() > p0, 25, step=0.5)), "the first instance opened no new window for the second launch: no new shell prompt within 25 s")
            n_inst = linux_instances(exe) if plat == "linux" else (ctx.app.instance_count() if plat == "macos" else (1 if ctx.app.alive() else 0))
            r.metric("instances_after_second_launch", n_inst)
            r.check(n_inst == 1, "%d app instances after the second launch, expected exactly 1" % n_inst)
            r.check(ctx.app.alive(), "the first instance is gone after the second launch")
            ctx.shot("installer_smoke_second_launch")
            # ---- (4) `phosphor <path>`
            tok = ctx.tok(6).lower()
            names = {"abs": "is_abs_%s.md" % tok, "rel": "is_rel_%s.md" % tok, "ctl": "is_ctl_%s.md" % tok}
            for n in names.values():
                with open(os.path.join(ctx.W, n), "w") as f:
                    f.write("# installer smoke\n\n%s\n" % n)
            for kind, arg in (("abs", os.path.join(ctx.W, names["abs"])), ("rel", names["rel"])):
                off = ctx.log_size()
                res = launch_again(ctx, [arg])
                what = "`phosphor <%s path>`" % ("absolute" if kind == "abs" else "relative")
                bad = second_launch_verdict(res, what)
                r.check(bad is None, (bad or "") + " (#2083: a bare path used to be refused as 'relative URL without a base')")
                if bad is None:
                    got = ctx.until(lambda: file_received(ctx.log_since(off), names[kind]), 15, step=0.3)
                    r.check(bool(got), "%s exited 0 but the running instance did not log the file as received: %r" % (what, received_urls(ctx.log_since(off))[:3]))
                    # which pane shows it (the Markdown viewer or the editor) follows the user's settings and is md_viewer's and editor_edit_save's
                    # subject; recorded here, not judged
                    r.metric("viewer_tab_logged_%s" % kind, bool(ctx.until(lambda: C.file_viewer_opened(ctx.log_since(off)), 4, step=0.3)))
            ctx.shot("installer_smoke_path_open")
            # control: the same line with the kill switch must open nothing, so the checks above can tell "opened" from "not opened"
            off = ctx.log_size()
            res = launch_again(ctx, [names["ctl"]], extra_env={KILL_SWITCH: "0"})
            time.sleep(2.0)
            opened = file_received(ctx.log_since(off), names["ctl"])
            r.check(not opened and not res["timed_out"] and res["rc"] not in (0, None),
                    "control (%s=0): the relative path was expected to be refused (non-zero exit, nothing received), got exit %s, received=%s: the path "
                    "checks above cannot tell an opened file from an unopened one" % (KILL_SWITCH, res["rc"], opened))
            r.check(ctx.app.alive(), "the installed copy died during the path checks")
            R.keep_log(ctx, "installer_smoke")
        # ---- (5) uninstall keeps user data (the installed copy was stopped by swapped_app)
        time.sleep(1.5)
        before = {}
        for root in app_data_roots(ctx):
            before.update({root + "|" + k: v for k, v in CU.tree_hashes(root).items()})
        r.metric("user_data_files", len(before))
        r.check(user_data_present(before), "the profile holds no settings file and database before the uninstall (%d files): 'kept' would prove nothing" % len(before))
        bad = inst.remove()
        r.check(bad is None, "uninstall: %s" % bad)
        r.check(not os.path.exists(inst.bin), "the installed program is still there after the uninstall: %s" % inst.bin)
        after = {}
        for root in app_data_roots(ctx):
            after.update({root + "|" + k: v for k, v in CU.tree_hashes(root).items()})
        gone, changed = kept_verdict(before, after)
        r.check(not gone, "the uninstall removed %d file(s) of the user's data: %s" % (len(gone), [g.split("|", 1)[1] for g in gone[:5]]))
        r.check(not changed, "the uninstall changed %d file(s) of the user's data: %s" % (len(changed), [g.split("|", 1)[1] for g in changed[:5]]))
    finally:
        R.use_app(ctx, new_app)
        if snap is not None:
            left = snap.restore()
            r.check(not left, "the machine state the installer changed was not all put back: %s" % "; ".join(left))
        try:
            # ---- (6) the build under test, again, on the same data
            _n, up = R.start_app(ctx, r, "is_end", restored_min=1)
            if r.check(up, "the build under test did not come back to a prompt after the installer smoke"):
                time.sleep(2)
                ctx.be.key("Escape")
                UI.exact_burst(ctx, r, "is_end", 2)
        except C.Skip:
            raise
        except Exception as e:                            # never mask the case's own verdict
            r.check(False, "could not leave the build under test running: %r" % (e,))


# ---------------------------------------------------------------- #1546: the phosphor:// handler, end to end
INVALID_URI_RE = re.compile(r"Custom URI is invalid|unexpected action|unexpected scheme")


def scheme_url(scheme, action, path=None):
    """pure: the URL a desktop environment hands to the registered handler (`Exec=phosphor %U`, CFBundleURLSchemes, the Windows `phosphor` protocol key):
    <scheme>://action/<action>[?path=<percent-encoded path>]"""
    import urllib.parse
    u = "%s://action/%s" % (scheme, action)
    return u + ("?path=" + urllib.parse.quote(path, safe="/:") if path else "")


def tab_dir_line(shell, tok, abs_file):
    """pure: the line that appends `<tok>:<the shell's current directory>` to abs_file, for append_cmd's shells"""
    if shell == "pwsh":
        return 'Add-Content %s "%s:$((Get-Location).Path)"' % (abs_file, tok)
    return 'echo "%s:$PWD" >> %s' % (tok, abs_file)


@case("phosphor_scheme_handler", modes=("terminal",), order=94, gui=True)
def c_phosphor_scheme_handler(ctx, r):
    """#1546, the handler half of 'phosphor:// end to end': what the operating system runs for a clicked `phosphor://` link is the app's own executable
    with the URL as its argument (Linux `.desktop` Exec=... %U, macOS CFBundleURLSchemes through the single-instance manager, the Windows protocol key).
    That line is run here against the RUNNING build, as the system would. (1) `phosphor://action/new_tab?path=<dir>`: the second launch exits 0, the
    running instance logs the URL (scheme, host, path only) and opens a tab whose shell is in <dir>. (2) The same URL with an unknown action and (3) with
    another scheme (`warp://`) open nothing and the app logs why; the app stays alive and keeps taking keys. The registration itself (that the OS
    knows to run it) is installer_smoke's concern on the installed copy: Linux `.desktop` MimeType, the Windows key (wininstall.py), the macOS plist."""
    if ctx.app is None or not hasattr(ctx.app, "bin"):
        r.skip("needs the real app (not the sim backend)")
    ctx.reset()
    tok = ctx.tok(6).lower()
    dname = "scheme_dir_%s" % tok
    ddir = os.path.join(ctx.W, dname)
    os.makedirs(ddir, exist_ok=True)
    out = "scheme_cwd_%s.txt" % tok
    abs_out = R.fwd(os.path.join(ctx.W, out))
    # (1) a valid new_tab URL
    off, p0 = ctx.log_size(), ctx.prompts()
    res = launch_again(ctx, [scheme_url("phosphor", "new_tab", ddir)])
    bad = second_launch_verdict(res, "`phosphor phosphor://action/new_tab?path=...`")
    r.check(bad is None, bad or "")
    if bad is None:
        got = ctx.until(lambda: [u for u in received_urls(ctx.log_since(off)) if u[0] == "phosphor" and u[1] == "action" and u[2] == "/new_tab"], 15, step=0.3)
        r.check(bool(got), "the second launch exited 0 but the running instance logged no `received url scheme=phosphor host=action path=/new_tab`: the URL was "
                           "not handed over. URLs it received: %r" % (received_urls(ctx.log_since(off))[:3],))
        if got and ctx.prof.local_prompts:
            r.check(bool(ctx.until(lambda: ctx.prompts() > p0, 25, step=0.5)), "the URL was received but no new tab's shell showed a prompt within 25 s")
        if got:
            time.sleep(1.0)
            ctx.be.guard()
            ctx.be.type(tab_dir_line(ctx.pane_shell(), tok, abs_out), 12, enter=True)
            lines = ctx.until(lambda: [l for l in ctx.lines(out) if l.startswith(tok + ":")], 12)
            if r.check(bool(lines), "the new tab took the typed line but it never reached %s: the tab the URL opened did not get the keys (or its shell never started)" % out):
                where = lines[0].split(":", 1)[1].strip().replace("\\", "/").rstrip("/")
                r.check(where.endswith("/" + dname), "the tab the URL opened is in %r, expected the directory named by the URL (…/%s)" % (where, dname))
            ctx.shot("phosphor_scheme_new_tab")
    # (2) an unknown action, (3) another scheme: nothing opens, the app says why and stays up
    for label, url in (("an unknown action", scheme_url("phosphor", "no_such_action_%s" % tok)), ("another scheme", scheme_url("warp", "new_tab", ddir))):
        off, p0 = ctx.log_size(), ctx.prompts()
        res = launch_again(ctx, [url])
        time.sleep(2.0)
        if res.get("error") or res["timed_out"]:
            r.check(False, "%s: the second launch %s" % (label, res.get("error") or "did not exit within %d s" % res["timeout"]))
            continue
        log = ctx.log_since(off)
        # the exit code is not judged: a launcher may refuse a foreign scheme itself (non-zero, nothing handed over) or hand it over and let the app refuse it
        handed = bool(received_urls(log)) or bool(INVALID_URI_RE.search(log))
        r.note("%s: exit %s, handed over: %s, refused with a log line: %s" % (label, res["rc"], bool(received_urls(log)), bool(INVALID_URI_RE.search(log))))
        if ctx.prof.local_prompts:
            r.check(ctx.prompts() == p0, "%s opened a tab (%d new prompt(s)): an invalid URL must open nothing" % (label, ctx.prompts() - p0))
        if label == "an unknown action" and handed:
            r.check(bool(INVALID_URI_RE.search(log)), "%s was handed to the app but it logged no 'Custom URI is invalid' line" % label)
        r.check(ctx.app.alive(), "the app died after %s" % label)
    ctx.be.guard()
    UI.exact_burst(ctx, r, "scheme_end", 2)
    R.keep_log(ctx, "phosphor_scheme_handler")


# ---------------------------------------------------------------- T44 upgrade_from_previous_release
PREV_ENV = ("APPCI_PREV_BIN", "APPCI_PREV_BUNDLE", "APPCI_PREV_RELEASE_BIN", "APPCI_PREV_FIXTURE")
TABLE_NEEDS = ("NOT RUN here: the settings table of upgrade_table.py (upgrade_from_v0_1_17: every seeded key, the keyring items, launch 2 idempotent). %s")


def table_arm_reason(plat, tag, fixture_tag=prev_release.TAG):
    """pure: None when the v0.1.17-shaped settings table can be judged in this run, else why not"""
    if plat != "linux":
        return TABLE_NEEDS % ("NEEDS: the seed paths and the keyring helper of cases_upgrade.py ported to %s (the fixture and the table are platform-neutral); "
                              "the data-directory checks of upgrade_over_previous ran" % plat)
    if tag != fixture_tag:
        return TABLE_NEEDS % ("The checked-in fixture is the profile %s writes and the previous release here is %s. NEEDS: a fixture and table reviewed for "
                              "%s (fixtures/upgrade_v0_1_17, upgrade_table.py)" % (fixture_tag, tag, tag))
    return None


def version_line(binary):
    """what `<binary> --version` prints (first line), or a note when it cannot be asked"""
    try:
        p = subprocess.run([binary, "--version"], capture_output=True, text=True, errors="replace", timeout=60)
        return (p.stdout or p.stderr).strip().splitlines()[0][:80] if (p.stdout or p.stderr).strip() else "no output (exit %s)" % p.returncode
    except Exception as e:
        return "could not be asked: %s" % type(e).__name__


@case("upgrade_from_previous_release", modes=("terminal",), order=91, gui=True)
def c_upgrade_from_previous_release(ctx, r):
    """T44 (#1546): the build under test upgrades a profile the PREVIOUS RELEASE wrote. The previous release is found by prev_artifact.locate (the
    cache, a download from this project's package registry verified against the release's SHA256SUMS, or the machine-local build); without one
    the case SKIPS naming the release asset that has to exist. Arm `data` (every platform) is upgrade_over_previous: the old build writes its own
    state, the new build reaches a prompt on it, takes exact typing, logs no panic / migration / database ERROR, keeps every database file and never
    has fewer applied migrations. Arm `table` (Linux, while the previous release is v0.1.17) is upgrade_from_v0_1_17: every row of upgrade_table.py
    holds and a second launch changes nothing. An arm that cannot run is named in a note; the case skips only when no arm ran."""
    R.need_relaunch_app(ctx, "upgrade_from_previous_release")
    plat = plat_key(ctx)
    got = PA.locate(plat, allow_fetch=False)
    stopped = False
    if not got["bin"] and got.get("fetchable"):
        if plat == "windows":                             # the previous release's installer refuses to run beside a running Phosphor
            ctx.snapshot_log()
            ctx.app.stop()
            stopped = True
        got = PA.locate(plat, run_dir=ctx.run, tag=got["tag"])
    if not got["bin"]:
        if stopped:
            R.start_app(ctx, r, "ufp_back", restored_min=1)
        r.skip(got["needs"])
    r.metric("previous_release", got["tag"])
    r.metric("previous_release_source", got["source"])
    r.metric("previous_release_version", version_line(got["bin"]))
    saved = {k: os.environ.get(k) for k in PREV_ENV}
    skipped, ran = [], []
    try:
        os.environ["APPCI_PREV_BIN"] = os.environ["APPCI_PREV_RELEASE_BIN"] = got["bin"]
        os.environ["APPCI_PREV_BUNDLE"] = got["bundle"] or ""
        os.environ.pop("APPCI_PREV_FIXTURE", None)        # this case is about the real previous build, never a packed state
        arms = [("data", CB.c_upgrade_over_previous)]
        why_not = table_arm_reason(plat, got["tag"])
        if why_not is None:
            arms.append(("table", CU.c_upgrade_from_v0_1_17))
        else:
            r.note(why_not)
        for name, fn in arms:
            try:
                fn(ctx, r)
                ran.append(name)
            except C.Skip as e:
                skipped.append("%s: %s" % (name, e))
                r.note("arm %s did not run: %s" % (name, e))
    finally:
        for k, v in saved.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v
        if not ctx.app.alive():
            try:
                R.start_app(ctx, r, "ufp_end", restored_min=1)
            except Exception as e:
                r.check(False, "could not leave the build under test running: %r" % (e,))
    r.metric("upgrade_arms_run", ran)
    if not ran:
        r.skip("no arm of the upgrade ran: %s" % " ;; ".join(skipped))
