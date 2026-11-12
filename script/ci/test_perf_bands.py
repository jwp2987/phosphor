#!/usr/bin/env python3
"""Self-tests of script/ci/perf_bands.py: window of 8, formula, ratchet (down, refuse loosen, allow-loosen), provisional, BOM, since, --check."""
import json, os, shutil, sys, tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import perf_bands as pb


def lat_case(name, key, over, mode="terminal"):
    return {"name": name, "mode": mode, "metrics": {"lat": {key: {"budget_kind": "overhead", "overhead_ms": over, "phosphor": {"p50": 40.0}, "reference": {"p50": 38.0}}}}}


def make_run(root, run, vals, plat="wayland", bom=False, jsonl=False):
    d = os.path.join(root, run, "app", plat)
    os.makedirs(d)
    rows = [lat_case("latency_enter", "enter_to_start", v) for v in vals]
    if jsonl:
        open(os.path.join(d, "results.jsonl"), "w", encoding="utf-8-sig" if bom else "utf-8").write("\n".join(json.dumps(x) for x in rows) + "\n")
    else:
        open(os.path.join(d, "results.json"), "w", encoding="utf-8-sig" if bom else "utf-8").write(json.dumps({"cases": rows}))


KEY = "wayland/terminal/latency_enter/enter_to_start"
tests = []


def t(f):
    tests.append(f)
    return f


def fresh(runs, **kw):
    root = tempfile.mkdtemp()
    for i, v in enumerate(runs):
        make_run(root, "2026-11-%02d_build_r%02d_%d" % (i + 1, i, 1000 + i), [v], **kw)
    return root


@t
def window_of_eight_and_formula():
    root = fresh([9.0, 1, 2, 3, 4, 5, 6, 7, 8.0, 2.0])
    try:
        b, notes = pb.generate(root, {}, config={})
        x = b[KEY]
        assert x["n"] == 8 and x["values"] == [2, 3, 4, 5, 6, 7, 8.0, 2.0] and not x["provisional"] and x["set_by"] == "data", x
        assert x["median"] == 4.5 and x["mad"] == 2.0 and x["upper"] == 10.5 and x["lower"] == -1.5, x          # median + 3 MAD; every value inside
        assert max(x["values"]) <= x["upper"]
    finally:
        shutil.rmtree(root)


@t
def floor_and_max_keep_every_value_inside():
    root = fresh([53.51, 30.0, 30.0, 30.0, 30.0, 30.0, 30.0, 30.0][::-1])         # the owner's 53.5 ms is the newest value and must pass
    try:
        b, _ = pb.generate(root, {}, config={})
        assert b[KEY]["upper"] >= 30.0 and b[KEY]["values"][-1] == 53.51 and b[KEY]["upper"] >= 53.51, b[KEY]
        root2 = fresh([1.0, 1.0, 1.0])
        b2, _ = pb.generate(root2, {}, config={})
        assert b2[KEY]["upper"] == 1.3, b2[KEY]            # median + the 0.3 ms floor
        shutil.rmtree(root2)
    finally:
        shutil.rmtree(root)


@t
def provisional_below_five_runs():
    root = fresh([1.0, 2.0, 3.0, 4.0])
    try:
        assert pb.generate(root, {}, config={})[0][KEY]["provisional"] is True
        make_run(root, "2026-11-20_build_r20_2000", [3.0])
        assert pb.generate(root, {}, config={})[0][KEY]["provisional"] is False
    finally:
        shutil.rmtree(root)


@t
def ratchet_down_refuse_loosen_and_allow():
    root = fresh([5.0] * 8)
    try:
        old, _ = pb.generate(root, {}, config={})
        assert old[KEY]["upper"] == 5.3
        for i in range(8):                       # the history gets faster: the upper goes DOWN by itself
            make_run(root, "2026-12-%02d_build_f%02d_%d" % (i + 1, i, 3000 + i), [2.0])
        lower, notes = pb.generate(root, old, config={})
        assert lower[KEY]["upper"] == 2.3 and not notes, (lower[KEY], notes)
        make_run(root, "2027-01-01_build_slow_9000", [9.0])        # a slow run would raise the upper: refused
        for i in range(7):
            pass
        loose, notes = pb.generate(root, lower, config={})
        assert loose[KEY]["upper"] == 2.3 and notes and notes[0].startswith("WOULD LOOSEN " + KEY + " 2.3 -> 9.0 (owner OK needed: --allow-loosen KEY)"), (loose[KEY], notes)
        ok, notes = pb.generate(root, lower, allow={KEY}, config={}, today="2026-11-12")
        assert ok[KEY]["upper"] == 9.0 and ok[KEY]["set_by"] == "owner" and ok[KEY]["owner_date"] == "2026-11-12" and not notes, ok[KEY]
        again, _ = pb.generate(root, ok, config={})                  # idempotent, and an owner-set band keeps its stamp
        assert again[KEY] == ok[KEY], (again[KEY], ok[KEY])
    finally:
        shutil.rmtree(root)


@t
def small_widening_applies_by_itself_larger_needs_the_owner():
    """owner 2026-11-12: a band may widen by up to its floor (0.3 ms) on its own (AUTO-LOOSEN); more is held (WOULD LOOSEN)"""
    root = fresh([5.0] * 8)
    try:
        old, _ = pb.generate(root, {}, config={})
        assert old[KEY]["upper"] == 5.3
        make_run(root, "2027-01-01_build_noise_9001", [5.5])        # +0.2 over the upper: noise-sized
        small, notes = pb.generate(root, old, config={})
        assert small[KEY]["upper"] == 5.5 and notes and notes[0].startswith("AUTO-LOOSEN " + KEY + " 5.3 -> 5.5"), (small[KEY], notes)
        make_run(root, "2027-01-02_build_slow_9002", [6.0])          # +0.5 over the new upper: needs the owner
        held, notes = pb.generate(root, small, config={})
        assert held[KEY]["upper"] == 5.5 and notes and notes[0].startswith("WOULD LOOSEN " + KEY + " 5.5 -> 6.0"), (held[KEY], notes)
    finally:
        shutil.rmtree(root)


@t
def bom_jsonl_and_skips():
    root = tempfile.mkdtemp()
    try:
        make_run(root, "2026-11-01_build_a_1", [3.0], plat="windows", bom=True)
        make_run(root, "2026-11-02_build_b_2", [4.0], plat="windows", bom=True, jsonl=True)
        make_run(root, "2026-11-02_build_b_2", [4.0], plat="lab-wayland") if False else None
        os.makedirs(os.path.join(root, "2026-11-02_build_b_2", "app", "lab-wayland"))
        make_run(root, "2026-11-03_build_c_3", [7.0], plat="linux-x11")
        b, _ = pb.generate(root, {}, config={})
        assert list(b) == ["windows/terminal/latency_enter/enter_to_start"] and b["windows/terminal/latency_enter/enter_to_start"]["values"] == [3.0, 4.0], b
    finally:
        shutil.rmtree(root)


@t
def since_cuts_old_history_and_its_loosening_is_held_back():
    root = fresh([16.0, 16.0, 2.0, 2.0, 2.0, 2.0, 2.0])
    try:
        cfg = {"since": {"wayland/*/latency_enter/*": "build_r02"}}
        b, _ = pb.generate(root, {}, config=cfg)
        assert b[KEY]["values"] == [2.0] * 5 and b[KEY]["upper"] == 2.3, b[KEY]
        full, _ = pb.generate(root, {}, config={})
        assert full[KEY]["upper"] == 16.0
        held, notes = pb.generate(root, b, config={})                 # removing the cut would loosen: needs the owner
        assert held[KEY]["upper"] == 2.3 and notes and notes[0].startswith("WOULD LOOSEN"), notes
    finally:
        shutil.rmtree(root)


@t
def check_mode_detects_stale():
    root = fresh([3.0, 3.0, 3.0])
    f = os.path.join(root, "bands.json")
    try:
        assert pb.main(["--results", root, "--bands", f]) == 0
        assert pb.main(["--results", root, "--bands", f, "--check"]) == 0
        make_run(root, "2026-12-01_build_n_5000", [2.0])
        assert pb.main(["--results", root, "--bands", f, "--check"]) == 1
    finally:
        shutil.rmtree(root)


@t
def heap_prime_gates_editor_metrics():
    row = {"name": "editor_memory_7mb", "mode": "terminal", "metrics": {"txt_open_cost_worst_mb": {"rss": 120.0}, "txt_rss_close_delta_mb": 4.0, "txt_cycle_growth_mb": 2.0}}
    assert pb.extract(row) == [] , "a run measured before the heap prime (#2185) feeds no editor band"
    row["metrics"]["heap_prime"] = {"pre": {}}
    got = {s: v for s, v, _ in pb.extract(row)}
    assert got == {"editor_memory_7mb/txt_open_cost_worst/rss": 120.0, "editor_memory_7mb/txt_held_after_close/rss": 4.0, "editor_memory_7mb/txt_cycle_growth/rss": 2.0}, got


@t
def committed_bands_hold_the_owners_numbers():
    b = pb.load_bands()
    if not b:
        return
    k = "windows/terminal@gitbash/latency_enter/enter_to_start"
    assert k in b and b[k]["upper"] >= 53.51 and 53.51 in b[k]["values"], b.get(k)


if __name__ == "__main__":
    fails = 0
    for f in tests:
        try:
            f()
            print("ok   " + f.__name__)
        except Exception as e:  # noqa: BLE001
            fails += 1
            print("FAIL %s: %s: %s" % (f.__name__, type(e).__name__, e))
    print("test_perf_bands: %d tests, %d failed" % (len(tests), fails))
    sys.exit(1 if fails else 0)
