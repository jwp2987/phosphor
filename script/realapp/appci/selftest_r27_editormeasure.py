# appci/selftest_r27_editormeasure.py: offline self-tests of the #2185 editor_memory_7mb measurement (heap prime, median open cost, per-cycle notes). No display.
import os, sys, types

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


def _s(rss):
    return {"rss": float(rss)}


@check
def r27_median_open_cost_is_independent_of_the_first_cycle():
    """r24a (cycle 1 into a 285 MB dirty heap: 97, then 202, 205) and r25a (129, 145, 147) judge on the median; limits unchanged: both FAIL the 100 MB target"""
    import cases
    for costs, want in (((97, 202, 205), 202.0), ((129, 145, 147), 145.0), ((130,), 130.0), ((100, 140), 120.0)):
        pairs = [(_s(300 + c), _s(300)) for c in costs]
        assert cases.median_open_cost(pairs) == {"rss": want}, (costs, cases.median_open_cost(pairs))
    # a macOS-style two-figure sample keeps both figures
    m = cases.median_open_cost([({"rss": 200.0, "phys_footprint": 150.0}, {"rss": 100.0, "phys_footprint": 100.0})])
    assert m == {"rss": 100.0, "phys_footprint": 50.0}, m


@check
def r27_verdict_still_fails_at_the_real_cold_cost_and_passes_below():
    import cases
    assert not cases.ACCEPTED_FOR_RELEASE, "no acceptance: the verdict is the real budget"
    for costs in ((129, 145, 147), (97, 202, 205)):
        r = cases.Result("t", "terminal")
        med = cases.median_open_cost([(_s(500 + c), _s(500)) for c in costs])
        assert not cases.mem_judge_known(r, med, {"rss": 0.0}, 100.0, "cost %.0f MB of %s", "editor_open_cost")
        assert len(r.fails) == 1, r.fails
    r = cases.Result("t", "terminal")
    assert cases.mem_judge_known(r, cases.median_open_cost([(_s(580), _s(500))] * 3), {"rss": 0.0}, 100.0, "cost %.0f MB of %s", "editor_open_cost")
    assert not r.fails


@check
def r27_cycle_note_records_base_after_open_and_app_breakdown():
    import cases
    n = cases.editor_cycle_note("txt_c2", _s(317), _s(446), [{"buffer_nodes_bytes": 53800800, "buffer_items": 230561}])
    for frag in ("txt_c2", "rss base 317 MB -> after-open 446 MB = +129 MB", "buffer_items=230561", "buffer_nodes_bytes=53800800"):
        assert frag in n, (frag, n)
    assert "app breakdown" not in cases.editor_cycle_note("t", _s(1), _s(2), [])


@check
def r27_prime_opens_closes_waits_for_the_trim_and_reports_the_post_trim_heap():
    import cases
    seq = iter([_s(608), _s(740), _s(330)])      # before, after open, after close trim
    calls = []
    saved = {n: getattr(cases, n) for n in ("settle_rss", "editor_open", "editor_close", "editor_mem_log", "editor_wait_dropped", "editor_wait_close_trim", "editor_opened")}
    try:
        cases.settle_rss = lambda ctx, **k: next(seq)
        cases.editor_open = lambda ctx, pal, path: calls.append("open")
        cases.editor_close = lambda ctx, r: (calls.append("close"), "key ctrl+shift+w")[1]
        cases.editor_mem_log = lambda ctx, off: {"open": [{"text_bytes": 1}], "dropped": [], "trim": []}
        cases.editor_wait_dropped = lambda ctx, off: calls.append("dropped") or [{}]
        cases.editor_wait_close_trim = lambda ctx, off: calls.append("trim") or [{"why": "close", "freed_estimate": 4096}]
        ctx = types.SimpleNamespace(log_path="x", log_size=lambda: 0, reset=lambda: None, be=types.SimpleNamespace(key=lambda k: None))
        p = cases.editor_prime(ctx, cases.Result("t", "terminal"), "ctrl+shift+p", "f")
    finally:
        for n, f in saved.items():
            setattr(cases, n, f)
    assert calls == ["open", "close", "dropped", "trim"], calls
    assert p["pre"] == _s(608) and p["opened"] == _s(740) and p["post_trim"] == _s(330) and p["close_trim_seen"] and p["freed_estimate"] == 4096, p


@check
def r27_case_wires_the_prime_median_and_bands():
    """the case source takes its base after the prime, judges the worst cycle, and (owner 2026-11-12) judges the open cost, the held-after-close and the cycle growth
    against history bands instead of the old 100 / 150 / 60 / 40 MB limits"""
    src = open(os.path.join(HERE, "cases.py"), encoding="utf-8").read()
    body = src[src.index('def c_editor_mem(ctx, r):'):src.index('@case("gitbash_subshell"')]
    assert body.index("editor_prime(") < body.index("base = settle_rss(ctx)") < body.index("worst_open_cost(cost_pairs)")
    assert "mem_judge_banded(r, ctx, worst," in body     # every cycle is judged (the worst), the median is only reported
    assert "mem_judge_banded(r, ctx, {k: after[k] - base[k]" in body and "_cycle_growth/%s" in body
    for frag in ("APPCI_EDITOR_MEM_TXT_MAX_MB", "APPCI_EDITOR_MEM_RS_MAX_MB", "APPCI_EDITOR_MEM_CLOSE_MAX_MB", "APPCI_EDITOR_MEM_CYCLE_GROWTH_MAX_MB"):
        assert frag not in body, frag


@check
def r27_worst_open_cost_judges_every_cycle():
    """the verdict is the worst cycle: one bad cycle out of three fails the row (never looser than the old cycle-1 check)"""
    import cases
    pairs = [({"rss": 580.0}, {"rss": 500.0}), ({"rss": 590.0}, {"rss": 500.0}), ({"rss": 640.0}, {"rss": 500.0})]
    assert cases.worst_open_cost(pairs) == {"rss": 140.0}
    assert cases.median_open_cost(pairs) == {"rss": 90.0}
