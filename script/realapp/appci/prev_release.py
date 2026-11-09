# appci/prev_release.py: finds (or fetches) the PREVIOUS RELEASE build the upgrade case seeds a profile with (#1546, `upgrade_from_v0_1_17`).
# Order: APPCI_PREV_RELEASE_BIN (an executable you point at; the AppRun of an extracted AppImage, or phosphor itself) -> the cache directory
# (APPCI_PREV_RELEASE_DIR, default ~/.cache/phosphor-ci/prev-release/<tag>/) -> a download of the release's Linux AppImage from the GitLab generic
# package registry, checked against the release's SHA256SUMS, then extracted with `--appimage-extract` (no FUSE, no /tmp). The download is skipped with
# APPCI_PREV_RELEASE_FETCH=0 and is Linux x86_64 only (macOS ships a dmg and Windows an installer: point APPCI_PREV_RELEASE_BIN at an installed copy there).
# Returns (path or None, why): the reason is shown when the case falls back to the checked-in fixture. Nothing here ever runs the downloaded file as
# anything but the app, and nothing is written outside the cache directory.
import hashlib, os, platform, subprocess, sys, urllib.request

TAG = "v0.1.17"
API = "https://gitlab.com/api/v4/projects/88085499/packages/generic/phosphor/%s/%s"
APPIMAGE = "Phosphor-x86_64.AppImage"


def cache_dir(tag=TAG, environ=None):
    environ = os.environ if environ is None else environ
    base = environ.get("APPCI_PREV_RELEASE_DIR") or os.path.join(os.path.expanduser("~"), ".cache", "phosphor-ci", "prev-release")
    return os.path.join(base, tag)


def sha256_of(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def expected_sha(sums_text, name):
    """the hash `sha256sum` printed for `name` in a SHA256SUMS text; None when the file is not listed"""
    for line in sums_text.splitlines():
        parts = line.split()
        if len(parts) >= 2 and parts[-1].lstrip("*") == name:
            return parts[0].lower()
    return None


def _download(url, dest, timeout=600):
    with urllib.request.urlopen(url, timeout=timeout) as r, open(dest, "wb") as f:
        while True:
            b = r.read(1 << 20)
            if not b:
                break
            f.write(b)


def extracted_app(d):
    p = os.path.join(d, "squashfs-root", "AppRun")
    return p if os.path.isfile(p) and os.access(p, os.X_OK) else None


def locate(tag=TAG, environ=None, fetch=None):
    """-> (executable path or None, why). `fetch` overrides the download step (selftest)."""
    environ = os.environ if environ is None else environ
    env_bin = environ.get("APPCI_PREV_RELEASE_BIN", "")
    if env_bin:
        if os.path.isfile(env_bin) and os.access(env_bin, os.X_OK):
            return env_bin, "APPCI_PREV_RELEASE_BIN"
        return None, "APPCI_PREV_RELEASE_BIN=%s is not an executable file" % env_bin
    d = cache_dir(tag, environ)
    got = extracted_app(d)
    if got:
        return got, "cache %s" % d
    if environ.get("APPCI_PREV_RELEASE_FETCH", "1") == "0":
        return None, "no cached %s build in %s and APPCI_PREV_RELEASE_FETCH=0" % (tag, d)
    if not (sys.platform.startswith("linux") and platform.machine() in ("x86_64", "AMD64")):
        return None, "the automatic download is Linux x86_64 only; set APPCI_PREV_RELEASE_BIN to an installed %s build" % tag
    try:
        (fetch or _fetch_appimage)(tag, d)
    except Exception as e:      # no network, a 404, a bad hash, a failed extract: the fixture is the fallback, the reason is reported
        return None, "could not fetch the %s AppImage (%s: %s)" % (tag, type(e).__name__, e)
    got = extracted_app(d)
    return (got, "downloaded %s" % tag) if got else (None, "the %s AppImage was fetched but has no squashfs-root/AppRun" % tag)


def _fetch_appimage(tag, d):
    os.makedirs(d, exist_ok=True)
    img = os.path.join(d, APPIMAGE)
    sums = urllib.request.urlopen(API % (tag, "SHA256SUMS"), timeout=60).read().decode("utf-8", "replace")
    want = expected_sha(sums, APPIMAGE)
    if not want:
        raise RuntimeError("SHA256SUMS does not list %s" % APPIMAGE)
    if not (os.path.isfile(img) and sha256_of(img) == want):
        _download(API % (tag, APPIMAGE), img)
    got = sha256_of(img)
    if got != want:
        os.remove(img)
        raise RuntimeError("sha256 mismatch for %s: %s != %s" % (APPIMAGE, got, want))
    os.chmod(img, 0o755)
    # extract in the cache dir (the AppImage writes ./squashfs-root); TMPDIR stays whatever the caller set
    subprocess.run([img, "--appimage-extract"], cwd=d, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True, timeout=600)
