# appci/prev_artifact.py: the PREVIOUS RELEASE of this project as a runnable app, on every platform, for the upgrade cases (r15 T44, #1546;
# docs/testing.md 2.25). prev_release.py did this for Linux only; this module adds macOS and Windows and the choice of the release.
#
# Where the build comes from, in order:
#   1. the cache directory of the release (APPCI_PREV_RELEASE_DIR, default ~/.cache/phosphor-ci/prev-release/<tag>/), filled by an earlier run;
#   2. a download from this project's GitLab generic package registry (where the release pipeline uploads every asset, script/ci/upload_package),
#      checked against the release's SHA256SUMS, then unpacked into the cache: Linux the AppImage (`--appimage-extract`), macOS the .dmg (mounted
#      read-only, the .app copied out), Windows the installer (run silently into the cache, copied, removed with its own uninstaller: wininstall.py);
#   3. the machine-local build an owner placed by hand (APPCI_PREV_BIN, on macOS with APPCI_PREV_BUNDLE; environment or ~/.config/phosphor-ci/ci.env).
#      It is used when 1 and 2 give nothing; its release is whatever was placed there.
# The download is skipped with APPCI_PREV_RELEASE_FETCH=0. Which release: APPCI_PREV_RELEASE_TAG, else the newest release of the project that is not
# the tag being built (asked from the releases API inside a CI job), else the pinned tag (prev_release.TAG).
# Nothing downloaded is run as anything but the app (or its own installer / uninstaller), and nothing is written outside the cache directory.
import json, os, platform, shutil, subprocess, sys, urllib.request

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import ci_env  # noqa: E402
import prev_release  # noqa: E402

PUBLIC_API = "https://gitlab.com/api/v4"
PROJECT_ID = "88085499"
PACKAGE = "phosphor"
# per platform: (what the asset's name ends with, words that must be in it when several match, how the report names it)
ASSET_RULES = {
    "linux": (".appimage", ("x86_64",), "the Linux AppImage (Phosphor-x86_64.AppImage)"),
    "macos": (".dmg", ("arm64", "aarch64"), "the macOS disk image (a .dmg for arm64)"),
    "windows": ("setup.exe", ("x86_64", "x64"), "the Windows installer (Phosphor-windows-x86_64-Setup.exe)"),
}


# ---------------------------------------------------------------- pure helpers (selftest `r15_prev_artifact_pure_parts`)
def platform_key(sys_platform=None):
    p = sys.platform if sys_platform is None else sys_platform
    return "windows" if p.startswith("win") else ("macos" if p == "darwin" else "linux")


def api_base(environ=None):
    """(api url, project id): the job's own when in CI, else the public project"""
    environ = os.environ if environ is None else environ
    return (environ.get("CI_API_V4_URL") or PUBLIC_API).rstrip("/"), environ.get("CI_PROJECT_ID") or PROJECT_ID


def package_url(tag, name, environ=None):
    api, pid = api_base(environ)
    return "%s/projects/%s/packages/generic/%s/%s/%s" % (api, pid, PACKAGE, tag, name)


def pick_asset(sums_text, plat):
    """the asset of platform `plat` among the names a release's SHA256SUMS lists, or None. One match is taken as it is; of several, the one whose
    name holds one of the platform's words (the architecture the test machines have)."""
    suffix, words, _what = ASSET_RULES[plat]
    names = []
    for line in (sums_text or "").splitlines():
        parts = line.split()
        if len(parts) >= 2 and len(parts[0]) == 64:
            names.append(parts[-1].lstrip("*"))
    hits = [n for n in names if n.lower().endswith(suffix)]
    if len(hits) > 1:
        pref = [n for n in hits if any(w in n.lower() for w in words)]
        hits = pref or hits
    return sorted(hits)[0] if hits else None


def pick_prev_tag(release_tags, current_tag, pinned):
    """the release to upgrade FROM: the newest release (the API lists newest first) that is not the tag being built; the pinned tag when the list
    is empty. A dev build (no current tag) upgrades from the newest release."""
    for t in release_tags or ():
        if t and t != current_tag:
            return t
    return pinned


def needs_text(plat, tag, why):
    """the NEEDS sentence of a run that has no previous release to upgrade from"""
    return ("NEEDS: %s of release %s as an asset of the generic package `%s/%s` of this project, listed in that release's SHA256SUMS (the release "
            "pipeline uploads both: script/ci/upload_package, release-publish), reachable from this machine; or a machine-local build named by "
            "APPCI_PREV_BIN%s in ~/.config/phosphor-ci/ci.env (#1546). %s" % (
                ASSET_RULES[plat][2], tag, PACKAGE, tag, " and APPCI_PREV_BUNDLE" if plat == "macos" else "", why))


def cached(plat, d, isfile=os.path.isfile, listdir=os.listdir):
    """(executable, bundle or None) of the release unpacked in cache directory `d`, or None"""
    if plat == "linux":
        b = prev_release.extracted_app(d)
        return (b, None) if b else None
    if plat == "windows":
        b = os.path.join(d, "app", "phosphor-oss.exe")
        return (b, None) if isfile(b) else None
    try:
        apps = sorted(n for n in listdir(d) if n.endswith(".app"))
    except OSError:
        return None
    for a in apps:
        mac = os.path.join(d, a, "Contents", "MacOS")
        try:
            bins = sorted(listdir(mac))
        except OSError:
            continue
        if bins:
            return os.path.join(mac, bins[0]), os.path.join(d, a)
    return None


# ---------------------------------------------------------------- network
def _open(url, environ=None, timeout=60):
    environ = os.environ if environ is None else environ
    rq = urllib.request.Request(url)
    if environ.get("CI_JOB_TOKEN") and environ.get("CI_API_V4_URL") and url.startswith(environ["CI_API_V4_URL"].rstrip("/")):
        rq.add_header("JOB-TOKEN", environ["CI_JOB_TOKEN"])      # the job's own token, only to the job's own GitLab
    return urllib.request.urlopen(rq, timeout=timeout)


def release_tags(environ=None):
    """tags of the project's releases, newest first; [] when the API cannot be asked"""
    api, pid = api_base(environ)
    try:
        with _open("%s/projects/%s/releases?per_page=20" % (api, pid), environ) as r:
            return [x.get("tag_name") for x in json.loads(r.read()) if x.get("tag_name")]
    except Exception:
        return []


def prev_tag(environ=None, tags_fn=release_tags):
    """-> (tag, how it was chosen)"""
    environ = os.environ if environ is None else environ
    t = environ.get("APPCI_PREV_RELEASE_TAG") or ci_env.get("APPCI_PREV_RELEASE_TAG")
    if t:
        return t, "APPCI_PREV_RELEASE_TAG"
    if environ.get("CI_API_V4_URL") and environ.get("APPCI_PREV_RELEASE_FETCH", "1") != "0":
        tags = tags_fn(environ)
        if tags:
            return pick_prev_tag(tags, environ.get("CI_COMMIT_TAG") or None, prev_release.TAG), "the newest release of the project"
    return prev_release.TAG, "the pinned tag (prev_release.TAG)"


def _download(url, dest, environ=None):
    part = dest + ".part"
    with _open(url, environ, timeout=600) as r, open(part, "wb") as f:
        shutil.copyfileobj(r, f, 1 << 20)
    os.replace(part, dest)


def fetch(plat, tag, d, environ=None, run_dir=None):
    """download the release's asset for `plat` into cache directory `d`, verify it and unpack it. Raises with the reason on any failure."""
    os.makedirs(d, exist_ok=True)
    with _open(package_url(tag, "SHA256SUMS", environ), environ) as r:
        sums = r.read().decode("utf-8", "replace")
    name = pick_asset(sums, plat)
    if not name:
        raise RuntimeError("SHA256SUMS of %s lists no asset ending in %s" % (tag, ASSET_RULES[plat][0]))
    want = prev_release.expected_sha(sums, name)
    path = os.path.join(d, name)
    if not (os.path.isfile(path) and prev_release.sha256_of(path) == want):
        _download(package_url(tag, name, environ), path, environ)
    got = prev_release.sha256_of(path)
    if got != want:
        os.remove(path)
        raise RuntimeError("sha256 mismatch for %s: %s != %s" % (name, got, want))
    UNPACK[plat](path, d, run_dir or d)


def _unpack_linux(path, d, _run_dir):
    os.chmod(path, 0o755)
    subprocess.run([path, "--appimage-extract"], cwd=d, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True, timeout=600)


def _unpack_macos(path, d, _run_dir):
    mnt = os.path.join(d, "mnt")
    os.makedirs(mnt, exist_ok=True)
    subprocess.run(["hdiutil", "attach", path, "-nobrowse", "-readonly", "-mountpoint", mnt, "-quiet"], check=True, timeout=300)
    try:
        apps = sorted(n for n in os.listdir(mnt) if n.endswith(".app"))
        if not apps:
            raise RuntimeError("the disk image holds no .app")
        dst = os.path.join(d, apps[0])
        shutil.rmtree(dst, ignore_errors=True)
        subprocess.run(["cp", "-R", os.path.join(mnt, apps[0]), dst], check=True, timeout=600)
    finally:
        subprocess.run(["hdiutil", "detach", mnt, "-quiet"], capture_output=True, timeout=120)
    subprocess.run(["xattr", "-dr", "com.apple.quarantine", dst], capture_output=True, timeout=120)


def _unpack_windows(path, d, run_dir):
    """the installer run silently into the cache, the installed directory copied, the install removed with its own uninstaller, and everything it
    wrote outside its directory put back (wininstall.Snapshot). The caller has stopped the app under test (the installer refuses otherwise)."""
    import wininstall
    snap = wininstall.Snapshot(os.path.join(run_dir, "prev_install_snapshot"))
    inst = os.path.join(d, "inst")
    try:
        exe, bad = wininstall.install(path, inst, os.path.join(d, "install.log"))
        if bad:
            raise RuntimeError(bad)
        wininstall.copy_install(inst, os.path.join(d, "app"))
        bad = wininstall.uninstall(inst, os.path.join(d, "uninstall.log"))
        if bad:
            raise RuntimeError("the previous release was copied, but its install could not be removed: %s" % bad)
    finally:
        left = snap.restore()
        if left:
            print("[prev_artifact] machine state not fully put back after the previous release's install: %s" % "; ".join(left), flush=True)


UNPACK = {"linux": _unpack_linux, "macos": _unpack_macos, "windows": _unpack_windows}


# ---------------------------------------------------------------- the entry point
def machine_local(plat, environ=None, getter=ci_env.get, isfile=os.path.isfile, isdir=os.path.isdir):
    """(executable, bundle or None, why) of the build an owner placed on this machine, or (None, None, why not)"""
    b, bundle = getter("APPCI_PREV_BIN"), getter("APPCI_PREV_BUNDLE")
    if not b:
        return None, None, "APPCI_PREV_BIN is not set (environment or ci.env)"
    if not isfile(b):
        return None, None, "APPCI_PREV_BIN names a file that does not exist"
    if plat == "macos" and not (bundle and isdir(bundle)):
        return None, None, "macOS needs APPCI_PREV_BUNDLE (the previous release's .app) next to APPCI_PREV_BIN"
    return b, (bundle if plat == "macos" else None), "the machine-local build (APPCI_PREV_BIN)"


def locate(plat=None, environ=None, fetcher=fetch, run_dir=None, allow_fetch=True, tag=None, local=machine_local):
    """-> dict(bin, bundle, tag, source) when a previous release can be run here, else dict(bin=None, needs=<the NEEDS sentence>, tag).
    allow_fetch=False looks at the cache and the machine-local build only (no download, no install): the caller asks again with True once the
    app under test is stopped."""
    environ = os.environ if environ is None else environ
    plat = plat or platform_key()
    how = "given"
    if not tag:
        tag, how = prev_tag(environ)
    d = prev_release.cache_dir(tag, environ)
    got = cached(plat, d)
    if got:
        return dict(bin=got[0], bundle=got[1], tag=tag, source="cache %s (%s)" % (d, how))
    why = "no cached build in %s" % d
    if environ.get("APPCI_PREV_RELEASE_FETCH", "1") == "0":
        why += "; APPCI_PREV_RELEASE_FETCH=0"
    elif plat == "linux" and platform.machine() not in ("x86_64", "AMD64"):
        why += "; the Linux release asset is x86_64 only and this machine is %s" % platform.machine()
    elif not allow_fetch:
        return dict(bin=None, bundle=None, tag=tag, source=None, fetchable=True, needs=needs_text(plat, tag, why + "; not fetched yet"))
    else:
        try:
            fetcher(plat, tag, d, environ, run_dir)
            got = cached(plat, d)
            if got:
                return dict(bin=got[0], bundle=got[1], tag=tag, source="downloaded %s (%s)" % (tag, how))
            why += "; the asset was fetched but did not unpack to a runnable app"
        except Exception as e:      # no network, a 404, a bad hash, a failed unpack: reported, the machine-local build is the fallback
            why += "; the download failed (%s: %s)" % (type(e).__name__, str(e)[:200])
    b, bundle, lwhy = local(plat, environ)
    if b:
        return dict(bin=b, bundle=bundle, tag="machine-local", source="%s; %s" % (lwhy, why))
    return dict(bin=None, bundle=None, tag=tag, source=None, needs=needs_text(plat, tag, "Tried: %s; %s." % (why, lwhy)))
