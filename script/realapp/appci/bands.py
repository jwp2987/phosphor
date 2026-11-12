# appci/bands.py: the perf / memory BAND every performance verdict reads (owner decision 2026-11-12: "Make bands for all, I don't [have] enough feel for
# bash yet to know if 53.5 is good enough or not. So have the band include that as passing and we can tighten later.").
# test-results/bands.json is written by script/ci/perf_bands.py from the committed run history (docs/testing.md "Perf and memory bands").
#   key   = <platform>/<mode incl. pane>/<case>/<metric>      (platform: wayland | macos | windows; "linux-wayland" is "wayland")
#   judge(key, value) -> (verdict, note)    verdict "pass" | "fail" | "noref"
#     above `upper` -> fail "outside its band (history ...)"; inside (below `lower` is only an improvement note) -> pass;
#     no band for the key (a new metric / pane) -> noref "no band yet": never a fail, never a pass.
#     A provisional band (fewer than 5 runs) still judges; its note says so.
# Env: APPCI_BANDS_FILE points at another bands file (a local experiment, a self-test fixture).
import json
import os

_REPO = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", ".."))
DEFAULT_FILE = os.path.join(_REPO, "test-results", "bands.json")
_cache = {}


def norm_platform(p):
    p = (p or "").strip()
    if p.startswith("macos"):
        return "macos"
    if p.startswith("windows"):
        return "windows"
    return "wayland" if p in ("linux-wayland", "wayland", "linux") else p


def key(platform, rid):
    """band key for a latency id rid = "<mode incl. pane>/<case>/<metric>" (what lat.evaluate is given)"""
    return "%s/%s" % (norm_platform(platform), rid)


def load(path=None):
    path = path or os.environ.get("APPCI_BANDS_FILE") or DEFAULT_FILE
    try:
        st = os.stat(path)
        sig = (st.st_mtime_ns, st.st_size)
    except OSError:
        return {}
    c = _cache.get(path)
    if c and c[0] == sig:
        return c[1]
    try:
        with open(path, encoding="utf-8-sig") as f:
            d = json.load(f)
    except (OSError, ValueError):
        d = {}
    _cache[path] = (sig, d)
    return d


def lookup(k, bands=None):
    b = (load() if bands is None else bands).get(k)
    return b if isinstance(b, dict) and b.get("upper") is not None else None


def describe(b):
    return "band %g-%g from n=%d run(s) (median %g)%s" % (b["lower"], b["upper"], b["n"], b["median"], ", PROVISIONAL (n < 5)" if b.get("provisional") else "")


def judge(k, value, bands=None):
    """-> (verdict, note). value None -> noref (nothing measured)."""
    b = lookup(k, bands)
    if b is None:
        return "noref", "no band yet for %s (a new metric or pane: NOREF, not a pass)" % k
    if value is None:
        return "noref", "no value to judge against the %s" % describe(b)
    if value > b["upper"]:
        return "fail", "%g is outside its band (history %s; above %g)" % (value, describe(b), b["upper"])
    note = describe(b)
    if value < b["lower"]:
        note += "; %g is below the band (an improvement)" % value
    return "pass", note


def check(r, k, value, what):
    """the case-level verdict for a banded figure: r.check on pass/fail, r.no_reference with no band, a note otherwise. Returns the verdict."""
    v, note = judge(k, value)
    if v == "fail":
        r.check(False, "%s: %s [%s]" % (what, note, k))
    elif v == "noref":
        r.no_reference("%s: %s [%s]" % (what, note, k))
    else:
        r.note("%s: %s [%s]" % (what, note, k))
    return v
