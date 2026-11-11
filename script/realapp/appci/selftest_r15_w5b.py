# appci/selftest_r15_w5b.py: offline self-tests of round 15 wave 5 part B (r14-test-gaps T43, T44, T46): cases_install.py, prev_artifact.py,
# wininstall.py and the pane-shell passes. Pure Python: no app, display, network or Windows. selftest.py appends CHECKS to its own list.
import os, re, sys, tempfile, types

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import cases, winmap, profile  # noqa: E402
import cases_install as CI  # noqa: E402
import cases_r14_w1a as W  # noqa: E402
import cases_robust as R  # noqa: E402
import cases_shells as S  # noqa: E402
import prev_artifact as PA  # noqa: E402
import prev_release  # noqa: E402
import winapi  # noqa: E402
import wininstall as WI  # noqa: E402

CHECKS = []
NAMES = ("installer_smoke", "upgrade_from_previous_release", "startup_shell_pane")


def check(f):
    CHECKS.append(f)
    return f


def _skips(fn, *a, **k):
    try:
        fn(*a, **k)
    except cases.Skip as e:
        return str(e)
    return None


@check
def r15_cases_registered_proving_and_classified():
    """the three cases are registered, in PROVING (never SMOKE), terminal mode, and placed for Windows: the installer and the upgrade once, in the
    PowerShell pane; the tails of the shared lists are separate statements"""
    reg = {c["name"]: c for c in cases.CASES}
    for n in NAMES:
        assert n in reg and n in cases.PROVING and n not in cases.SMOKE, n
        assert reg[n]["modes"] == ("terminal",), n
        assert n not in cases.PROVING_FAKE, n
    for n in NAMES[:2]:
        assert winmap.skip_reason(n, "powershell") is None and "PowerShell pane" in winmap.skip_reason(n, "gitbash"), n
    src = open(os.path.join(HERE, "cases.py"), encoding="utf-8").read()
    assert src.rstrip().splitlines()[-1].startswith("import cases_install")
    assert "warp::uri=info" in __import__("app").WINDOWS_RUST_LOG and __import__("app").rust_log_enabled(__import__("app").WINDOWS_RUST_LOG, "warp::uri", "info")


@check
def r15_installer_pure_parts():
    """T43: artefact choice, the .deb layout verdicts, removal-script scan, the second-launch and received-URL verdicts, the kept-data verdict and its
    control, instance counting, and the Windows installer command lines"""
    assert CI.find_artifacts("linux", ["SHA256SUMS", "phosphor_0.1.17_amd64.deb", "Phosphor-x86_64.AppImage"]) == {"deb": "phosphor_0.1.17_amd64.deb", "appimage": "Phosphor-x86_64.AppImage"}
    assert CI.find_artifacts("macos", ["Phosphor-arm64.dmg", "SHA256SUMS"]) == {"dmg": "Phosphor-arm64.dmg"}
    assert CI.find_artifacts("windows", ["SHA256SUMS", "PhosphorSetup.exe"]) == {"setup": "PhosphorSetup.exe"}
    for plat in ("linux", "macos", "windows"):
        assert CI.find_artifacts(plat, ["SHA256SUMS"]) == {} and CI.artifact_needs(plat).startswith("NEEDS: "), plat
    assert CI.DIST_NEEDS.startswith("NEEDS: ") and "APPCI_DIST" in CI.DIST_NEEDS
    # the real desktop entry of the OSS channel: `phosphor %U`, an icon, the launcher made by the postinst
    desk = open(os.path.join(HERE, "..", "..", "..", "app", "channels", "oss", "dev.phosphor.Phosphor.desktop"), encoding="utf-8").read()
    e = CI.desktop_entry(desk)
    assert e["Exec"] == "phosphor %U" and e["Icon"] == "dev.phosphor.Phosphor", e
    assert CI.deb_layout_problems(e, False, True, True, True) == []
    assert CI.deb_layout_problems(e, True, False, True, True) == []
    dead = CI.deb_layout_problems(e, False, False, True, True)
    assert len(dead) == 1 and "dead launcher" in dead[0], dead
    assert any("icon" in p for p in CI.deb_layout_problems(e, True, False, False, True))
    assert any("no executable" in p for p in CI.deb_layout_problems(e, True, False, True, False))
    assert any("%U" in p for p in CI.deb_layout_problems(dict(e, Exec="phosphor"), True, False, True, True))
    assert any(".desktop" in p for p in CI.deb_layout_problems({}, True, True, True, True))
    assert CI.desktop_entry("[Other]\nExec=x\n[Desktop Entry]\n# Exec=no\nExec=phosphor %U\n") == {"Exec": "phosphor %U"}
    # a removal script may remove the launcher; it may not name a user's home
    assert CI.script_touches_user_data("#!/bin/sh\n# rm -rf $HOME/.config/phosphor\nrm -f /usr/bin/phosphor\n") == []
    assert CI.script_touches_user_data("rm -rf \"$HOME/.config/phosphor\"\n") and CI.script_touches_user_data("rm -rf /home/*/.local/share/phosphor\n")
    # what the running instance logged
    log = ("x [INFO] received url scheme=phosphor host=action path=/new_window\n"
           "y [INFO] received url scheme=file host=- path=/job/home/w/is_abs_k3.md\n")
    assert CI.received_urls(log) == [("phosphor", "action", "/new_window"), ("file", "-", "/job/home/w/is_abs_k3.md")]
    assert CI.joined(log) and not CI.joined(log.splitlines()[1]) and not CI.joined("received url scheme=phosphor host=action path=/new_tab")
    assert CI.file_received(log, "is_abs_k3.md") and not CI.file_received(log, "abs_k3.md") and not CI.file_received(log, "is_rel_k3.md")
    assert not CI.file_received("received url scheme=c host=- path=/job/is_abs_k3.md", "is_abs_k3.md"), "a Windows drive path taken as a URL scheme is not a file"
    ok = dict(rc=0, timed_out=False, timeout=30, err="", error=None)
    assert CI.second_launch_verdict(ok, "x") is None
    assert "exited with code 2" in CI.second_launch_verdict(dict(ok, rc=2, err="error: invalid value"), "x")
    assert "did not exit" in CI.second_launch_verdict(dict(ok, rc=None, timed_out=True), "x")
    assert "could not be started" in CI.second_launch_verdict(dict(ok, error="boom"), "x")
    # user data across the uninstall, and the control that there was data to lose
    before = {"r|phosphor/settings.toml": "a", "r|phosphor/warp.sqlite": "b", "r|phosphor/themes/t.yaml": "c"}
    assert CI.kept_verdict(before, dict(before)) == ([], [])
    assert CI.kept_verdict(before, {"r|phosphor/settings.toml": "a", "r|phosphor/warp.sqlite": "CHANGED"}) == (["r|phosphor/themes/t.yaml"], ["r|phosphor/warp.sqlite"])
    assert CI.kept_verdict(before, dict(before, **{"r|new": "n"})) == ([], []), "a file that appeared is not a loss"
    assert CI.user_data_present(before) and not CI.user_data_present({"r|phosphor/settings.toml": "a"}) and not CI.user_data_present({})
    rows = [("/root/opt/phosphor-oss", ["/root/usr/bin/phosphor"]), ("/root/opt/phosphor-oss", ["phosphor-oss", "terminal-server", "--x"]), ("/usr/bin/bash", ["bash"])]
    assert CI.instance_count(rows, "/root/opt/phosphor-oss") == 1
    assert CI.instance_count(rows + [("/root/opt/phosphor-oss", ["phosphor-oss", "file:///x"])], "/root/opt/phosphor-oss") == 2
    assert CI.plat_key(types.SimpleNamespace(win=True, be=None)) == "windows" and CI.plat_key(types.SimpleNamespace(win=False, be=types.SimpleNamespace(name="mac"))) == "macos"
    assert CI.plat_key(types.SimpleNamespace(win=False, be=types.SimpleNamespace(name="wayland"))) == "linux"
    # Windows installer
    assert WI.installer_argv("S.exe", r"C:\j\app", r"C:\j\i.log") == ["S.exe", "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/CURRENTUSER", r"/DIR=C:\j\app", r"/LOG=C:\j\i.log"]
    assert WI.uninstaller_argv("u.exe") == ["u.exe", "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"]
    assert WI.find_uninstaller(["phosphor-oss.exe", "unins000.exe", "unins000.dat", "unins001.exe", "uninstall-notes.exe"]) == "unins001.exe" and WI.find_uninstaller(["a.exe"]) is None
    assert WI.find_setup(["SHA256SUMS", "tool.exe", "Phosphor-windows-x86_64-Setup.exe"]) == "Phosphor-windows-x86_64-Setup.exe" and WI.find_setup(["x.txt"]) is None
    q = "\r\nHKEY_CURRENT_USER\\Software\\...\\zap-oss_is1\r\n    DisplayName    REG_SZ    Phosphor\r\n    InstallLocation    REG_SZ    C:\\job dir\\app\\\r\n"
    assert WI.install_location(q) == "C:\\job dir\\app" and WI.install_location("") is None
    assert WI.same_dir("C:\\Job Dir\\app\\", "c:/job dir/app") and not WI.same_dir("C:\\a", "C:\\b") and not WI.same_dir(None, "C:\\a")
    assert WI.UNINSTALL_KEY in WI.REG_KEYS and len(WI.shortcut_paths({"APPDATA": "A", "USERPROFILE": "U"})) == 2
    # the uninstall key and the mutex facts this rests on are the installer script's
    iss = open(os.path.join(HERE, "..", "..", "windows", "windows-installer.iss"), encoding="utf-8", errors="replace").read()
    assert "AppMutex=" in iss and "uninsdeletekey" in iss
    assert "InnoAppId" in open(os.path.join(HERE, "..", "..", "windows", "bundle.ps1"), encoding="utf-8", errors="replace").read()


@check
def r15_installer_smoke_skips_with_needs_and_swaps_the_guard():
    """T43: without the artefact directory the case SKIPS naming it (never a pass); swapping the app under test moves the frontmost-window guard
    with it and puts it back (the fault that would have refused every key on macOS and Windows in the upgrade cases)"""
    saved = os.environ.pop("APPCI_DIST", None)
    try:
        ctx = types.SimpleNamespace(app=types.SimpleNamespace(start=lambda: 1, stop=lambda: 1), win=False, be=types.SimpleNamespace(name="wayland"))
        why = _skips(CI.c_installer_smoke, ctx, cases.Result("installer_smoke", "terminal"))
        assert why and why.startswith("NEEDS: ") and "APPCI_DIST" in why, why
        d = tempfile.mkdtemp()
        os.environ["APPCI_DIST"] = d
        why = _skips(CI.c_installer_smoke, ctx, cases.Result("installer_smoke", "terminal"))
        assert why and why.startswith("NEEDS: ") and ".deb" in why, why
        assert "sim backend" in (_skips(CI.c_installer_smoke, types.SimpleNamespace(app=None), cases.Result("installer_smoke", "terminal")) or "")
    finally:
        os.environ.pop("APPCI_DIST", None)
        if saved is not None:
            os.environ["APPCI_DIST"] = saved
    stopped = []
    a = types.SimpleNamespace(pid=lambda: 11, stop=lambda: stopped.append("a"))
    b = types.SimpleNamespace(pid=lambda: 22, stop=lambda: stopped.append("b"))
    ctx = types.SimpleNamespace(app=a, be=types.SimpleNamespace(pid_fn=a.pid))
    with CI.swapped_app(ctx, b):
        assert ctx.app is b and ctx.be.pid_fn() == 22
    assert ctx.app is a and ctx.be.pid_fn() == 11 and stopped == ["b"]
    try:
        with CI.swapped_app(ctx, b):
            raise RuntimeError("case blew up")
    except RuntimeError:
        pass
    assert ctx.app is a and ctx.be.pid_fn() == 11, "the app under test must be back after an exception"
    x11 = types.SimpleNamespace(app=a, be=types.SimpleNamespace(pid_fn=None))          # a backend with no guard pid stays without one
    R.use_app(x11, b)
    assert x11.app is b and x11.be.pid_fn is None
    for f in ("cases_coverage_b.py", "cases_upgrade.py"):
        src = open(os.path.join(HERE, f), encoding="utf-8").read()
        assert not re.search(r"^\s*ctx\.app = ", src, re.M), "%s swaps ctx.app without the guard (use cases_robust.use_app)" % f
    # the wrappers hand the artefact directory to the harness
    root = os.path.join(HERE, "..")
    assert 'export APPCI_DIST=' in open(os.path.join(root, "linux", "wayland_session.sh"), encoding="utf-8").read()
    assert 'export APPCI_DIST=' in open(os.path.join(root, "mac", "ci", "app_test.sh"), encoding="utf-8").read()
    assert "$env:APPCI_DIST = $Dist" in open(os.path.join(root, "windows", "ci", "app_test.ps1"), encoding="utf-8", errors="replace").read()


@check
def r15_prev_artifact_pure_parts():
    """T44: which release, which asset, where it is fetched from, the cache layout per platform, the NEEDS sentence, and the order cache -> download ->
    machine-local build (no network: the fetcher is a stub)"""
    sums = ("%s  Phosphor-x86_64.AppImage\n%s  phosphor_0.1.17_amd64.deb\n%s *Phosphor-arm64.dmg\n%s  Phosphor-x86_64.dmg\n%s  Phosphor-windows-x86_64-Setup.exe\n"
            "%s  phosphor-cli-linux-x86_64.tar.gz\nnot a sum line\n" % tuple(c * 64 for c in "abcdef"))
    assert PA.pick_asset(sums, "linux") == "Phosphor-x86_64.AppImage"
    assert PA.pick_asset(sums, "macos") == "Phosphor-arm64.dmg"
    assert PA.pick_asset(sums, "windows") == "Phosphor-windows-x86_64-Setup.exe"
    assert PA.pick_asset("%s  only.dmg\n" % ("a" * 64), "macos") == "only.dmg" and PA.pick_asset("", "linux") is None and PA.pick_asset(sums.replace("Setup.exe", "Setup.zip"), "windows") is None
    assert prev_release.expected_sha(sums, "Phosphor-arm64.dmg") == "c" * 64
    assert PA.pick_prev_tag(["v0.1.18", "v0.1.17"], "v0.1.18", "v0.1.0") == "v0.1.17", "a tag build upgrades from the release before it"
    assert PA.pick_prev_tag(["v0.1.17", "v0.1.16"], None, "v0.1.0") == "v0.1.17", "a dev build upgrades from the newest release"
    assert PA.pick_prev_tag([], None, "v0.1.17") == "v0.1.17"
    assert PA.platform_key("win32") == "windows" and PA.platform_key("darwin") == "macos" and PA.platform_key("linux") == "linux"
    assert PA.package_url("v0.1.17", "SHA256SUMS", {}) == "https://gitlab.com/api/v4/projects/88085499/packages/generic/phosphor/v0.1.17/SHA256SUMS"
    assert PA.package_url("v1", "a", {"CI_API_V4_URL": "https://ci.example/api/v4/", "CI_PROJECT_ID": "7"}) == "https://ci.example/api/v4/projects/7/packages/generic/phosphor/v1/a"
    assert PA.prev_tag({"APPCI_PREV_RELEASE_TAG": "v9"}) == ("v9", "APPCI_PREV_RELEASE_TAG")
    env_ci = {"CI_API_V4_URL": "https://ci.example/api/v4", "CI_COMMIT_TAG": "v0.1.18"}
    saved = os.environ.pop("APPCI_PREV_RELEASE_TAG", None)
    real_get = PA.ci_env.get
    PA.ci_env.get = lambda k, default=None: None            # no machine-local ci.env in this test
    try:
        assert PA.prev_tag(env_ci, tags_fn=lambda e: ["v0.1.18", "v0.1.17"])[0] == "v0.1.17"
        assert PA.prev_tag(env_ci, tags_fn=lambda e: []) == (prev_release.TAG, "the pinned tag (prev_release.TAG)")
        assert PA.prev_tag({}, tags_fn=lambda e: ["v5"])[0] == prev_release.TAG, "outside CI the release list is not asked"
    finally:
        PA.ci_env.get = real_get
        if saved is not None:
            os.environ["APPCI_PREV_RELEASE_TAG"] = saved
    for plat in ("linux", "macos", "windows"):
        t = PA.needs_text(plat, "v0.1.17", "why.")
        assert t.startswith("NEEDS: ") and "v0.1.17" in t and "SHA256SUMS" in t and "APPCI_PREV_BIN" in t and ("APPCI_PREV_BUNDLE" in t) == (plat == "macos"), t
    tmp = tempfile.mkdtemp()
    # cache layouts
    assert PA.cached("linux", tmp) is None and PA.cached("macos", tmp) is None and PA.cached("windows", tmp) is None
    os.makedirs(os.path.join(tmp, "Phosphor.app", "Contents", "MacOS"))
    open(os.path.join(tmp, "Phosphor.app", "Contents", "MacOS", "phosphor-oss"), "w").close()
    assert PA.cached("macos", tmp) == (os.path.join(tmp, "Phosphor.app", "Contents", "MacOS", "phosphor-oss"), os.path.join(tmp, "Phosphor.app"))
    os.makedirs(os.path.join(tmp, "app"))
    open(os.path.join(tmp, "app", "phosphor-oss.exe"), "w").close()
    assert PA.cached("windows", tmp) == (os.path.join(tmp, "app", "phosphor-oss.exe"), None)
    # locate: cache -> download -> machine-local, each named in the result
    base = tempfile.mkdtemp()
    env = {"APPCI_PREV_RELEASE_DIR": base}
    none = lambda plat, environ=None: (None, None, "APPCI_PREV_BIN is not set (environment or ci.env)")      # noqa: E731
    calls = []

    def fake_fetch(plat, tag, d, environ, run_dir):
        calls.append((plat, tag))
        os.makedirs(os.path.join(d, "app"))
        open(os.path.join(d, "app", "phosphor-oss.exe"), "w").close()

    first = PA.locate("windows", env, fetcher=fake_fetch, allow_fetch=False, tag="v0.1.17", local=none)
    assert first["bin"] is None and first.get("fetchable") and first["needs"].startswith("NEEDS: ") and calls == [], first
    got = PA.locate("windows", env, fetcher=fake_fetch, tag="v0.1.17", local=none)
    assert got["bin"].endswith("phosphor-oss.exe") and got["tag"] == "v0.1.17" and got["source"].startswith("downloaded") and calls == [("windows", "v0.1.17")], got
    again = PA.locate("windows", env, fetcher=fake_fetch, allow_fetch=False, tag="v0.1.17", local=none)
    assert again["bin"] == got["bin"] and again["source"].startswith("cache") and len(calls) == 1, "a cached release is not fetched again"

    def broken(plat, tag, d, environ, run_dir):
        raise RuntimeError("HTTP Error 404: Not Found")
    miss = PA.locate("macos", env, fetcher=broken, tag="v0.1.17", local=none)
    assert miss["bin"] is None and miss["needs"].startswith("NEEDS: ") and "404" in miss["needs"] and ".dmg" in miss["needs"], miss
    local = PA.locate("macos", env, fetcher=broken, tag="v0.1.17", local=lambda plat, environ=None: ("/x/Phosphor.app/Contents/MacOS/p", "/x/Phosphor.app", "the machine-local build (APPCI_PREV_BIN)"))
    assert local["bin"] == "/x/Phosphor.app/Contents/MacOS/p" and local["bundle"] == "/x/Phosphor.app" and local["tag"] == "machine-local" and "404" in local["source"], local
    off = PA.locate("macos", dict(env, APPCI_PREV_RELEASE_FETCH="0"), fetcher=fake_fetch, tag="v0.1.17", local=none)
    assert off["bin"] is None and "APPCI_PREV_RELEASE_FETCH=0" in off["needs"] and len(calls) == 1, off
    # the machine-local build
    g = {"APPCI_PREV_BIN": "/b", "APPCI_PREV_BUNDLE": "/B.app"}.get
    assert PA.machine_local("linux", getter=g, isfile=lambda p: True)[:2] == ("/b", None)
    assert PA.machine_local("macos", getter=g, isfile=lambda p: True, isdir=lambda p: True)[:2] == ("/b", "/B.app")
    assert PA.machine_local("macos", getter=g, isfile=lambda p: True, isdir=lambda p: False)[0] is None
    assert PA.machine_local("linux", getter=g, isfile=lambda p: False)[0] is None and PA.machine_local("linux", getter={}.get)[0] is None
    # upgrade_over_previous reads the machine-local file too (it read os.environ only and skipped on every machine)
    src = open(os.path.join(HERE, "cases_coverage_b.py"), encoding="utf-8").read()
    assert '(ci_env.get(k) or "") for k in ("APPCI_PREV_FIXTURE", "APPCI_PREV_BIN", "APPCI_PREV_BUNDLE")' in src


@check
def r15_upgrade_case_arms_and_skip():
    """T44: which arms run where, and the SKIP names the release asset when no previous release can be had"""
    assert CI.table_arm_reason("linux", prev_release.TAG) is None
    for plat in ("macos", "windows"):
        t = CI.table_arm_reason(plat, prev_release.TAG)
        assert t.startswith("NOT RUN here") and "NEEDS:" in t and plat in t, t
    t = CI.table_arm_reason("linux", "v9.9.9")
    assert "NEEDS:" in t and "v9.9.9" in t and prev_release.TAG in t, t
    real = PA.locate
    PA.locate = lambda plat, **k: dict(bin=None, bundle=None, tag="v0.1.17", source=None, needs=PA.needs_text(plat, "v0.1.17", "Tried: nothing."))
    try:
        ctx = types.SimpleNamespace(app=types.SimpleNamespace(start=lambda: 1, stop=lambda: 1, alive=lambda: True), win=False, be=types.SimpleNamespace(name="wayland"))
        why = _skips(CI.c_upgrade_from_previous_release, ctx, cases.Result("upgrade_from_previous_release", "terminal"))
        assert why and why.startswith("NEEDS: ") and "AppImage" in why and "v0.1.17" in why, why
    finally:
        PA.locate = real
    assert CI.version_line(os.path.join(HERE, "no-such-binary")).startswith("could not be asked")


@check
def r15_pane_shell_passes():
    """T46: the pwsh7 / cmd / wsl passes: their case lists, what each needs on the machine, the app-fell-back verdict from the processes under the
    app, the WSL settings and paths, the cmd.exe history record, and the job variable that plays them"""
    assert winmap.VARIANT_FAMILY == {"pwsh7": "powershell", "cmd": "cmd", "wsl": "wsl"}
    reg = {c["name"] for c in cases.CASES}
    for v, names in winmap.VARIANT_CASES.items():
        assert set(names) <= reg and len(set(names)) == len(names), v
        for n in ("mode_proof", "startup_shell_pane", "history_up_recall", "routing_table_powershell"):
            assert n in names, (v, n)
        assert winmap.VARIANT_NEEDS[v].startswith("NEEDS: "), v
        assert v in winmap.VARIANT_PROCESS and v in S.IDENT, v
    assert "owner" in winmap.VARIANT_NEEDS["wsl"] and "owner" in winmap.VARIANT_NEEDS["pwsh7"] and "PowerShell 7" in winmap.VARIANT_NEEDS["pwsh7"]
    # the pane the app really started (#2084: cmd.exe is not a shell type it knows, so it starts its default shell)
    bad = winmap.variant_shell_problem("cmd", ["OpenConsole.exe", "powershell.exe", "conhost.exe"])
    assert bad and "powershell.exe" in bad and "fell back" in bad, bad
    assert winmap.variant_shell_problem("cmd", ["openconsole.exe", "cmd.exe"]) is None
    assert winmap.variant_shell_problem("cmd", []) is None, "an unreadable job object is no verdict"
    assert winmap.variant_shell_problem("pwsh7", ["pwsh.exe", "conhost.exe"]) is None and "pwsh.exe" in winmap.variant_shell_problem("pwsh7", ["powershell.exe"])
    assert winmap.variant_shell_problem("wsl", ["wsl.exe", "wslhost.exe"]) is None and "none of" in winmap.variant_shell_problem("wsl", ["conhost.exe"])
    # identity lines
    assert S.identity_ok("wsl", ["WSLOK_1"]) and not S.identity_ok("wsl", ["WSLOK_0"]) and not S.identity_ok("wsl", ["WSLOK_$(grep -ci microsoft /proc/version)"])
    assert "#2084" in S.identity_hint("cmd", ["CMDOK_%COMSPEC:~-7%"]) and "#2084" in S.identity_hint("cmd", ["C\0M\0D\0O\0K\0_\0%\0C\0O\0M\0S\0P\0E\0C\0"])
    assert S.identity_hint("cmd", []).strip().startswith("(nothing was written") and S.identity_hint("pwsh7", ["PS5_Desktop"]) == ""
    assert "terminal@wsl" in S.pane_pass_skip_reason(True) and S.pane_pass_skip_reason(False).startswith("Windows only")
    for ctx in (types.SimpleNamespace(variant=None, win=True), types.SimpleNamespace(variant=None, win=False)):
        assert _skips(S.c_startup_shell_pane, ctx, cases.Result("startup_shell_pane", "terminal")) == S.pane_pass_skip_reason(ctx.win)
    # WSL: the distribution list, the path the distribution sees, the settings value, no rc hook, no pid probe
    assert winapi.parse_wsl_list("Ubuntu\r\nDebian\r\n".encode("utf-16-le")) == ["Ubuntu", "Debian"]
    assert winapi.parse_wsl_list(b"Ubuntu-22.04\n") == ["Ubuntu-22.04"] and winapi.parse_wsl_list(b"") == []
    assert winapi.parse_wsl_list("Windows Subsystem for Linux has no installed distributions.\r\n".encode("utf-16-le")) == []
    assert winapi.wsl_path("C:\\job dir\\home\\w") == "/mnt/c/job dir/home/w" and winapi.wsl_path("D:/x") == "/mnt/d/x" and winapi.wsl_path("/already/posix") == "/already/posix"
    pr = profile.Profile(tempfile.mkdtemp(), pane="wsl")
    pr.win, pr.variant = True, "wsl"
    assert pr.local_prompts is False and pr.gitbash_toml() is None
    real = winapi.wsl_distro
    try:
        winapi.wsl_distro = lambda: "Ubuntu"
        assert pr.wsl_toml() == '{ wsl = "Ubuntu" }'
        winapi.wsl_distro = lambda: None
        try:
            pr.wsl_toml()
            raise AssertionError("no distribution must be an error, not an empty setting")
        except RuntimeError:
            pass
    finally:
        winapi.wsl_distro = real
    pr.variant = "cmd"
    assert pr.wsl_toml() is None
    assert cases.depth_probe_cmd("wsl", "t") is None and cases.depth_probe_cmd("cmd", "t") is None and cases.depth_probe_cmd("gitbash", "t")
    assert cases.Ctx.pane_shell(types.SimpleNamespace(pane="wsl")) == "bash"
    # T1 in a cmd.exe pane: its own syntax, Escape for the empty line, the console history as the record
    want, eff = W.hist_expected("cmd", "hu.txt", ["a1", "b2", "c3", "d4"])
    assert want == ["echo a1>>hu.txt", "echo b2>>hu.txt", "echo c3>>hu.txt", "echo b2>>hu.txt", "echo b2>>hu_e.txt", "echo d4>>hu.txt"], want
    assert eff == {"hu.txt": ["a1", "b2", "c3", "b2", "d4"], "hu_e.txt": ["b2"]}, eff
    plan = W.hist_edit_plan("cmd", "hu.txt", "b2")
    assert plan["line"][:-plan["backspaces"]] + plan["typed"] == plan["edited"], plan
    assert W.hist_clear_keys("cmd") == ["Up", "Escape"] and W.hist_clear_keys("bash") == ["Up", "Down"] and W.hist_clear_keys("pwsh") == ["Up", "Down"]
    assert W.hist_expected("bash", "hu.txt", ["a1", "b2", "c3", "d4"])[0][0] == "echo a1 >> hu.txt", "the bash form is unchanged"
    assert W.doskey_lines(["echo a>>f", "doskey /history>dk_x_1.txt", "", "echo b>>f  ", "DOSKEY /HISTORY>dk_x_2.txt"]) == ["echo a>>f", "echo b>>f"]
    typed = []

    class Be:
        def type(self, text, ms, enter=False):
            typed.append(text)
            f = text.split(">", 1)[1]
            with open(os.path.join(tmpw, f), "w") as fh:
                fh.write("\n".join(history + [text]) + "\n")
    tmpw = tempfile.mkdtemp()
    history = ["dir", "echo old>>x"]
    cx = types.SimpleNamespace(pane="cmd", W=tmpw, be=Be(), tok=lambda n: "tk", rm=lambda *a: None, HARNESS_MARKER_RE=cases.Ctx.HARNESS_MARKER_RE,
                               lines=lambda name: open(os.path.join(tmpw, name)).read().splitlines() if os.path.exists(os.path.join(tmpw, name)) else [],
                               until=lambda pred, t, step=0.1: pred())
    rec = W.Received(cx)
    assert rec.cmd and rec.mark == 2 and rec.recorded() and len(typed) == 1, (rec.mark, typed)
    assert rec.wait(3) == [] and len(typed) == 1, "waiting must not type a probe between the steps of a recall"
    history += ["echo a1>>hu.txt", "echo tok>>alive_x.txt"]
    assert rec.lines() == ["echo a1>>hu.txt"] and len(typed) == 2, rec.lines()
    # T5 says what the other panes lack
    assert "#2084" in W.routing_ps_skip_reason("cmd") and "NEEDS:" in W.routing_ps_skip_reason("cmd")
    assert "NEEDS:" in W.routing_ps_skip_reason("wsl") and "owner" in W.routing_ps_skip_reason("wsl")
    assert W.routing_ps_skip_reason("gitbash").startswith("runs in the PowerShell pane")
    # the job that plays the passes, and the run loop's use of the checks
    yml = open(os.path.join(HERE, "..", "..", "..", ".gitlab-ci.yml"), encoding="utf-8").read()
    m = re.search(r"^app-test-windows:\n(.*?)(?=^\S)", yml, re.S | re.M)
    v = re.search(r'^    APPCI_WIN_SHELLS: "([^"]+)"', m.group(1), re.M) if m else None
    assert v and v.group(1).split(",") == ["powershell", "gitbash", "pwsh7", "cmd", "wsl"], "app-test-windows no longer plays the pane-shell passes: startup_shell_pane would SKIP in every run again"
    src = open(os.path.join(HERE, "run.py"), encoding="utf-8").read()
    assert "variant_pane_problem(variant, app)" in src and "winmap.VARIANT_NEEDS[variant]" in src and "none of its cases is selected, not started" in src
