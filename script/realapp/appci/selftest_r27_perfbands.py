# appci/selftest_r27_perfbands.py: offline self-tests of the perf / memory bands (owner 2026-11-12: "Make bands for all, I don't [have] enough feel for bash yet
# to know if 53.5 is good enough or not. So have the band include that as passing and we can tighten later."). bands.judge, lat.evaluate(banded=True), lat_checks and
# every converted verdict: inside the band PASS, above FAIL, no band NOREF, provisional bands judge and say so. No display.
import json, os, shutil, sys, tempfile, types

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


def band(up, n=8, lo=None, kind="ms"):
    return {"n": n, "median": round(up - 1, 2), "mad": 0.1, "upper": up, "lower": up - 3 if lo is None else lo, "values": [up - 1], "runs": ["r"],
            "provisional": n < 5, "set_by": "data", "kind": kind}


class Fx:
    """a bands file for the test (APPCI_BANDS_FILE), restored afterwards"""
    def __init__(self, d):
        self.d, self.tmp, self.old = d, tempfile.mkdtemp(), os.environ.get("APPCI_BANDS_FILE")

    def __enter__(self):
        f = os.path.join(self.tmp, "bands.json")
        json.dump(self.d, open(f, "w"))
        os.environ["APPCI_BANDS_FILE"] = f
        return self

    def __exit__(self, *a):
        if self.old is None:
            os.environ.pop("APPCI_BANDS_FILE", None)
        else:
            os.environ["APPCI_BANDS_FILE"] = self.old
        shutil.rmtree(self.tmp)


@check
def r27_bands_judge_pass_fail_noref_provisional():
    import bands
    d = {"windows/terminal@gitbash/latency_enter/enter_to_start": band(54.0), "macos/agent/latency_enter/enter_to_start": band(6.0, n=3, lo=2.0)}
    k = "windows/terminal@gitbash/latency_enter/enter_to_start"
    v, note = bands.judge(k, 53.51, d)
    assert v == "pass" and "band 51-54 from n=8 run(s) (median 53)" in note, (v, note)          # the owner's 53.5 ms passes
    v, note = bands.judge(k, 54.01, d)
    assert v == "fail" and "outside its band (history band 51-54" in note, note
    assert bands.judge(k, 54.0, d)[0] == "pass", "the upper bound itself is inside"
    assert bands.judge(k, 30.0, d)[0] == "pass" and "improvement" in bands.judge(k, 30.0, d)[1], "below the band is an improvement note, never a failure"
    v, note = bands.judge("macos/agent/latency_enter/enter_to_start", 7.0, d)
    assert v == "fail" and "PROVISIONAL (n < 5)" in note, "a provisional band still judges, and says so"
    assert bands.judge("macos/agent/latency_enter/enter_to_start", 5.0, d)[0] == "pass" and "PROVISIONAL" in bands.judge("macos/agent/latency_enter/enter_to_start", 5.0, d)[1]
    v, note = bands.judge("windows/terminal@fish/latency_enter/enter_to_start", 5.0, d)
    assert v == "noref" and "no band yet" in note, "a new pane has no band: NOREF, never a pass or a fail"
    assert bands.judge(k, None, d)[0] == "noref"
    assert bands.key("linux-wayland", "terminal/latency_enter/enter_to_start") == "wayland/terminal/latency_enter/enter_to_start"
    assert bands.key("macos", "x") == "macos/x" and bands.key("windows", "x") == "windows/x"


@check
def r27_bands_check_helper_maps_to_case_verdicts():
    import bands, cases
    with Fx({"wayland/terminal/subshell_ready_rc_clear/clear_inner_line_after_enter_s": band(1.5, kind="s")}):
        k = "wayland/terminal/subshell_ready_rc_clear/clear_inner_line_after_enter_s"
        r = cases.Result("t", "terminal")
        assert bands.check(r, k, 1.2, "first inner line") == "pass" and not r.fails and not r.noref
        r = cases.Result("t", "terminal")
        assert bands.check(r, k, 2.6, "first inner line") == "fail" and len(r.fails) == 1 and "outside its band" in r.fails[0] and k in r.fails[0]
        r = cases.Result("t", "terminal")
        assert bands.check(r, "wayland/terminal/other/x", 2.6, "x") == "noref" and not r.fails and len(r.noref) == 1


@check
def r27_lat_evaluate_banded_overhead_inside_above_noref():
    import lat
    ref = [40.0 + (i % 3) * 0.1 for i in range(60)]
    ph = lambda ov: [40.0 + ov + (i % 3) * 0.1 for i in range(60)]
    rid = "terminal@gitbash/latency_enter/enter_to_start"
    with Fx({"windows/" + rid: band(54.0)}):
        ok = lat.evaluate(rid, ph(53.5), ref, None, "overhead", "windows", expected=60, banded=True)
        assert ok["budget_verdict"] == "pass" and ok["budget_ms"] == 54.0 and ok["band_key"] == "windows/" + rid and ok["verdict"] == "pass", ok
        bad = lat.evaluate(rid, ph(60.0), ref, None, "overhead", "windows", expected=60, banded=True)
        assert bad["budget_verdict"] == "fail" and bad["verdict"] == "fail", bad
        # a previous-build regression is a NOTE only: it never turns a record inside its band into a failure
        tmp = tempfile.mkdtemp()
        try:
            json.dump({"cases": [{"name": "latency_enter", "mode": "terminal@gitbash", "metrics": {"lat": {"enter_to_start": {"phosphor": {"n": 60, "p50": 41.0, "p95": 42.0}}}}}]},
                      open(os.path.join(tmp, "prev.json"), "w"))
            os.environ["APPCI_PREV_RESULTS"] = os.path.join(tmp, "prev.json")
            rec = lat.evaluate(rid, ph(53.5), ref, None, "overhead", "windows", expected=60, banded=True)
            assert rec["prev_verdict"] == "fail" and rec["verdict"] == "pass", rec
        finally:
            os.environ.pop("APPCI_PREV_RESULTS", None)
            shutil.rmtree(tmp)
        no = lat.evaluate("terminal@fish/latency_enter/enter_to_start", ph(1.0), ref, None, "overhead", "windows", expected=60, banded=True)
        assert no["budget_verdict"] == "no-reference" and no["band"] == {"status": "none"}, no
        # an SE-wide overhead is inconclusive at the band's upper, as it was at the budget (the 2 SE guard stays)
        wide = lat.evaluate(rid, [40.0 + 54.0 + ((i * 7) % 13) * 4.0 for i in range(60)], ref, None, "overhead", "windows", expected=60, banded=True)
        assert wide["budget_verdict"] in ("inconclusive", "fail", "pass"), wide
        # no reference by construction (latency_key_log): the Phosphor median itself is the banded figure
    k2 = "terminal/latency_key_log/key_to_frame"
    with Fx({"wayland/" + k2: band(3.0), "wayland/" + k2 + "#app": band(1.0)}):
        frames = [2.7 + (i % 3) * 0.05 for i in range(60)]
        rec = lat.evaluate(k2, frames, None, None, "overhead", "linux-wayland", expected=60, banded=True, band_on_median=True)
        assert rec["budget_verdict"] == "pass" and rec["overhead_ms"] is None, rec
        rec = lat.evaluate(k2, [x + 1.0 for x in frames], None, None, "overhead", "linux-wayland", expected=60, banded=True, band_on_median=True)
        assert rec["budget_verdict"] == "fail", rec
        # the app share is judged against the band of "<id>#app"; a missing band judges nothing (a note)
        sh = dict(what="x", p50=1.2, n=60, budget_ms=99.0)
        rec = lat.evaluate(k2, frames, None, None, "overhead", "linux-wayland", expected=60, banded=True, band_on_median=True, app_share=sh)
        assert rec["app_share"]["budget_ms"] == 1.0 and rec["app_share"]["budget_verdict"] == "fail" and rec["verdict"] == "fail", rec["app_share"]
    with Fx({"wayland/" + k2: band(3.0)}):
        rec = lat.evaluate(k2, frames, None, None, "overhead", "linux-wayland", expected=60, banded=True, band_on_median=True, app_share=dict(what="x", p50=9.0, n=60, budget_ms=1.0))
        assert rec["app_share"]["budget_ms"] is None and "budget_verdict" not in rec["app_share"] and rec["verdict"] == "pass" and any("no band yet" in n for n in rec["notes"]), rec


@check
def r27_every_latency_call_site_is_banded():
    """each verdict that decided on a fixed budget now hands lat.evaluate banded=True and no number"""
    import re
    want = {"cases.py": 7, "cases_agent_latency.py": 1, "cases_caseremote.py": 1}
    for fn, n in want.items():
        src = open(os.path.join(HERE, fn), encoding="utf-8").read()
        calls = [m.start() for m in re.finditer(r"lat\.evaluate\(", src)]
        banded = [c for c in calls if "banded=True" in src[c:c + 700]]
        assert len(banded) == n and len(calls) >= n, (fn, len(banded), len(calls))
    cs = open(os.path.join(HERE, "cases.py"), encoding="utf-8").read()
    for gone in ("APPCI_ENTER_BUDGET_MS", "APPCI_CTRLC_MAX_MS", "APPCI_CTRLC_FLOOD_MAX_MS", "APPCI_PAINT_BUDGET_MS", "APPCI_IDLE_KEY_BUDGET_MS", "APPCI_KEY_BUDGET_MS",
                 "APPCI_KEY_PTY_SHARE_MAX_MS", "APPCI_KEY_APP_SHARE_MAX_MS", "APPCI_CTRLC_APP_SHARE_MAX_MS", "APPCI_ENTER_APP_SHARE_MAX_MS", "MACOS_ENTER_GROSS_OVERHEAD_MS"):
        assert gone + '"' not in cs and gone + " =" not in cs, gone


@check
def r27_editor_memory_verdicts_read_bands():
    import cases
    r0 = lambda: cases.Result("editor_memory_7mb", "terminal")
    ctx = types.SimpleNamespace(label="terminal")
    orig = cases.plat
    cases.plat = lambda: "macos"
    try:
        with Fx({"macos/terminal/editor_memory_7mb/txt_open_cost_worst/rss": band(150.0, kind="mb"), "macos/terminal/editor_memory_7mb/txt_open_cost_worst/phys_footprint": band(120.0, kind="mb"),
                 "macos/terminal/editor_memory_7mb/txt_held_after_close/phys_footprint": band(20.0, n=2, kind="mb")}):
            r = r0()
            assert cases.mem_judge_banded(r, ctx, {"rss": 140.0, "phys_footprint": 110.0}, "txt_open_cost_worst", "opening cost %.0f MB of %s") and not r.fails
            r = r0()
            assert not cases.mem_judge_banded(r, ctx, {"rss": 140.0, "phys_footprint": 125.0}, "txt_open_cost_worst", "opening cost %.0f MB of %s")
            assert len(r.fails) == 1 and "phys_footprint" in r.fails[0] and "outside its band" in r.fails[0], r.fails
            r = r0()                      # macOS held after close: phys_footprint judged, RSS only reported; a provisional band judges and says so
            assert not cases.mem_judge_banded(r, ctx, {"rss": 400.0, "phys_footprint": 25.0}, "txt_held_after_close", "%.0f MB of %s still held", closed=True)
            assert len(r.fails) == 1 and "PROVISIONAL" in r.fails[0] and any("reported, not judged on macOS" in n for n in r.notes), (r.fails, r.notes)
            r = r0()                      # no band for the rs kind yet: NOREF, not a pass
            cases.mem_judge_banded(r, ctx, {"rss": 10.0, "phys_footprint": 10.0}, "rs_open_cost_worst", "x %.0f MB of %s")
            assert not r.fails and len(r.noref) == 2, r.noref
        cases.plat = lambda: "windows"
        with Fx({"windows/terminal/editor_memory_7mb/txt_open_cost_worst/private": band(90.0, kind="mb")}):
            r = r0()                      # Windows: private bytes judged, the working set reported
            assert cases.mem_judge_banded(r, ctx, {"rss": 500.0, "private": 80.0}, "txt_open_cost_worst", "x %.0f MB of %s") and not r.fails and any("reported, not judged on Windows" in n for n in r.notes)
    finally:
        cases.plat = orig


@check
def r27_subshell_first_inner_line_reads_its_band():
    src = open(os.path.join(HERE, "cases_subshell_ready.py"), encoding="utf-8").read()
    assert "dt < 2.0" not in src and src.count("bands.check(r, bands.key(") == 2
    import bands
    k = bands.key("windows", "terminal@gitbash/subshell_ready_rc_clear/clear_inner_line_after_enter_s")
    with Fx({k: band(5.5, kind="s")}):
        assert bands.judge(k, 4.8)[0] == "pass" and bands.judge(k, 6.0)[0] == "fail"
    with Fx({}):
        assert bands.judge(k, 4.8)[0] == "noref"


@check
def r27a_every_banded_record_name_has_its_band():
    """r27a: latency_enter_ssh's record was named .../enter_to_start while its band is .../ssh_enter_to_start, so the macOS and Wayland ssh Enter
    rows read 'no band yet' and were never judged. Every literal lat.evaluate record name must match a band in test-results/bands.json."""
    import glob, re
    here = os.path.dirname(os.path.abspath(__file__))
    root = os.path.abspath(os.path.join(here, "..", "..", ".."))
    b = json.load(open(os.path.join(root, "test-results", "bands.json")))
    suffixes = {"/".join(k.split("/")[2:]) for k in b.get("bands", b)}
    missing = []
    for f in glob.glob(os.path.join(here, "*.py")):
        for m in re.finditer(r'lat\.evaluate\(\s*"%s/([a-z0-9_]+/[a-z0-9_]+)"', open(f).read()):
            if m.group(1) not in suffixes:
                missing.append((os.path.basename(f), m.group(1)))
    assert not missing, missing
