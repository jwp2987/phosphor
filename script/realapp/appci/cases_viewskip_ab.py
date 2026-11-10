# appci/cases_viewskip_ab.py: #1962 (docs/design/key-scene-build.md, option A): a typed character re-renders only the views it changed (kill switch
# PHOSPHOR_KEY_VIEW_SKIP=0, default ON). Judged like #1838 / #1848: both arms in the SAME run, interleaved in blocks (cases_ctrlc_ab.ab_schedule =
# lat.block_schedule), one relaunch per block because the app reads the variable once.
#   latency_key_views_ab   Linux, macOS and Windows, terminal and agent mode; PROVING (fix tier). Windows: WindowsApp.extra_env is merged into the environment of the process the harness spawns.
# What is compared is the app's own hop=key_frame_build line (#1961) of every key frame: `views=` (views re-rendered), render_ms, layout_ms, total_ms, for two
# typing patterns per block:
#   first  the latency_paint pattern: a character into an EMPTY input, then Delete, a pause between keys. Every key changes empty <-> non-empty, the case in
#          which round 8 saw six views re-rendered. On macOS the keys are posted by kl3 (cases.paint_samples), which also gives key -> first changed pixel.
#   burst  a word typed at 10-25 ms/key into a non-empty input (two views in terminal mode, four in agent mode in round 8).
# The latency VERDICTS stay where they are (latency_paint, agent_latency_key, default arm); no budget is read or changed here. This case fails on a harness
# problem (a relaunch without a prompt, too few key frames) and on the one thing the change must never do: re-render MORE views with the skips on than off.
# Everything else is metrics, a note per pattern, and WARNs about the A/B itself. Pure parts are covered by selftest.py: viewskip_ab_pure_parts_1962.
import os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases  # noqa: E402  (cases.py imports this module as one of its last lines)
from cases import case, stats  # noqa: E402
import cases_ctrlc_ab  # noqa: E402
import lat  # noqa: E402
import render_hops  # noqa: E402

KEY_VIEW_SKIP_ENV = "PHOSPHOR_KEY_VIEW_SKIP"
BURST_KEYS = 10                     # characters per burst line (the line is cleared after it, so a frame's cost does not grow with the line)
PARTS = ("first", "burst")
_SKIP_FLAG = re.compile(r"hop=key_view_skip enabled=([01])\b")
_BUILD = "hop=key_frame_build "


def arm_env(arm):
    return {KEY_VIEW_SKIP_ENV: "0"} if arm == "off" else {}


def skip_flags(lines):
    """the values of `hop=key_view_skip enabled=<0|1>` in a launch's log (the app logs it once, at its first edit) -> a sorted list, [] when it logged none"""
    return sorted({m.group(1) for l in lines for m in [_SKIP_FLAG.search(l)] if m})


def drop_lead_builds(lines, k):
    """the lines without the first k hop=key_frame_build lines (the lead keys of a block: the app has just been relaunched)"""
    out, left = [], max(0, k)
    for l in lines:
        if left and _BUILD in l:
            left -= 1
            continue
        out.append(l)
    return out


def part_summary(hops):
    """render_hops.render_hop_metrics of one arm's lines for one pattern -> what this case compares (None where the build logged no such line)"""
    return dict(key_frames=hops.get("key_frame_builds", 0), views=hops.get("key_frame_build_views"), render_ms=hops.get("key_frame_build_render_ms"),
                layout_ms=hops.get("key_frame_build_layout_ms"), total_ms=hops.get("key_frame_build_total_ms"))


def ab_summary(first_hops, burst_hops, paint, flags):
    """one arm -> its metric dict: the two patterns, key -> first changed pixel (macOS, kl3) and the switch values the app logged"""
    st = stats(paint)
    return dict(first=part_summary(first_hops), burst=part_summary(burst_hops), key_to_paint=st if st["n"] else None, skip_flags=list(flags))


def _p50(arm, part, name):
    return ((arm.get(part) or {}).get(name) or {}).get("p50")


def _delta(a, b):
    return round(a - b, 2) if a is not None and b is not None else None


def ab_verdict(on, off):
    """(deltas, [WARN texts], [FAIL texts]) from the two ab_summary dicts. Deltas are ON minus OFF at the median (negative = the skips help): views, render_ms
    and total_ms per pattern, and key -> paint. A FAIL is the skips re-rendering MORE views per key than no skips; the WARNs are about the A/B itself."""
    deltas, warns, fails = {}, [], []
    for part in PARTS:
        deltas[part] = {name: _delta(_p50(on, part, name), _p50(off, part, name)) for name in ("views", "render_ms", "total_ms")}
    deltas["key_to_paint_p50"] = _delta((on.get("key_to_paint") or {}).get("p50"), (off.get("key_to_paint") or {}).get("p50"))
    if "1" in off.get("skip_flags", []):
        warns.append("the off arm logged hop=key_view_skip enabled=1: the kill switch %s=0 did not reach the app (extra_env not passed?)" % KEY_VIEW_SKIP_ENV)
    if "0" in on.get("skip_flags", []):
        warns.append("the on arm logged hop=key_view_skip enabled=0: %s=0 is set in the default environment, so both arms run without the skips" % KEY_VIEW_SKIP_ENV)
    if not on.get("skip_flags") and not off.get("skip_flags"):
        warns.append("neither arm logged hop=key_view_skip: a build before #1962 (or input-order logging off), so both arms measure the same thing")
    for part in PARTS:
        d = deltas[part]["views"]
        if d is None:
            continue
        if d > 0:
            fails.append("%s keys: %s views re-rendered per key with the skips ON against %s with them OFF; the skips must never re-render more" % (
                part, _p50(on, part, "views"), _p50(off, part, "views")))
        elif d == 0:
            warns.append("%s keys: the same %s views re-rendered per key in both arms; the skips change nothing for this pattern in this mode" % (
                part, _p50(on, part, "views")))
    return deltas, warns, fails


def type_first(ctx, n):
    """latency_paint's pattern without kl3: n keys, a character into the empty input and Delete in turn, a pause between keys"""
    for i in range(n):
        if i % 2 == 0:
            ctx.be.type("m", 10)
        else:
            ctx.be.key("BackSpace")
        time.sleep(ctx.rng.uniform(0.08, 0.18))


def type_burst(ctx, n):
    """about n keys typed at 10-25 ms/key into a non-empty input, in lines of BURST_KEYS characters (the line is cleared after each)"""
    for _ in range(max(1, -(-n // BURST_KEYS))):
        ctx.be.type("m" * BURST_KEYS, ctx.rng.randint(10, 25))
        time.sleep(0.3)
        ctx.be.key("ctrl+u")
        time.sleep(0.2)


@case("latency_key_views_ab", order=45, gui=True)
def c_latency_key_views_ab(ctx, r):
    """#1962 A/B: views re-rendered per key frame (hop=key_frame_build views= / render_ms / total_ms) with the per-view skips ON (default) vs OFF
    (PHOSPHOR_KEY_VIEW_SKIP=0), interleaved in blocks in this run (relaunch per block), for the first-character pattern of latency_paint and for a typing
    burst; on macOS also key -> first changed pixel (kl3). Metrics key_view_skip_on / key_view_skip_off and key_view_skip_deltas, a note per pattern.
    Fails on a harness problem and when the skips re-render MORE views than no skips; never judges a latency."""
    import cases_robust
    if not hasattr(ctx.app, "extra_env"):
        r.skip("this app launcher cannot pass an extra environment to a relaunch, and the switch is read once at start-up")
    if not ctx.log_path:
        r.skip("no app log (set APPCI_APP_LOG): the comparison is the app's own hop=key_frame_build lines")
    kl3 = ctx.be.name == "mac" and hasattr(ctx.be, "kl_start")
    n, warm = ctx.count_lat(30), lat.warmup()
    lines = {a: {p: [] for p in PARTS} for a in ("on", "off")}
    paint, flags, oks = {"on": [], "off": []}, {"on": set(), "off": set()}, []
    back = None
    try:
        for arm, cnt, disc in cases_ctrlc_ab.ab_schedule(n, warm):
            ctx.app.extra_env = arm_env(arm)
            _p, up = cases_robust.start_app(ctx, r, "latency_key_views_ab skips %s" % arm)
            if not up:
                oks.append(False)
                break
            time.sleep(cases.LAT_SETTLE_S)
            ctx.reset()
            ctx.be.guard()
            off0 = ctx.log_size()
            if kl3:
                rect = ctx.be.window_rect()
                if not rect or rect[2] < 100:
                    oks.append(False)
                    r.note("latency_key_views_ab: no Phosphor window rect from System Events after the relaunch (arm %s)" % arm)
                    break
                paint[arm] += cases.paint_samples(ctx, rect, cnt)[disc:]
            else:
                type_first(ctx, cnt)
            time.sleep(0.5)
            lines[arm]["first"] += drop_lead_builds(ctx.log_since(off0).splitlines(), disc)
            ctx.reset()
            off1 = ctx.log_size()
            type_burst(ctx, cnt)
            time.sleep(0.5)
            lines[arm]["burst"] += drop_lead_builds(ctx.log_since(off1).splitlines(), disc)
            flags[arm] |= set(skip_flags(ctx.app_log_lines()))       # this launch's log: the next relaunch overwrites it
            oks.append(True)
            ctx.reset()
    finally:
        ctx.app.extra_env = {}
        _p, back = cases_robust.start_app(ctx, r, "latency_key_views_ab: relaunch with the default environment")
        ctx.reset()
    arms = {a: ab_summary(render_hops.render_hop_metrics(lines[a]["first"]), render_hops.render_hop_metrics(lines[a]["burst"]), paint[a], sorted(flags[a]))
            for a in ("on", "off")}
    on, off = arms["on"], arms["off"]
    r.metric("key_view_skip_on", on)
    r.metric("key_view_skip_off", off)
    deltas, warns, fails = ab_verdict(on, off)
    r.metric("key_view_skip_deltas", deltas)
    for part in PARTS:
        d = deltas[part]
        r.note("#1962 views re-rendered per key, skips on vs off, %s keys (%s mode): views p50 %s / %s (Δ %s), render_ms p50 %s / %s (Δ %s), frame build total_ms "
               "p50 %s / %s (Δ %s); key frames %s / %s" % (
                   part, ctx.mode, _p50(on, part, "views"), _p50(off, part, "views"), d["views"], _p50(on, part, "render_ms"), _p50(off, part, "render_ms"),
                   d["render_ms"], _p50(on, part, "total_ms"), _p50(off, part, "total_ms"), d["total_ms"], on[part]["key_frames"], off[part]["key_frames"]))
    if kl3:
        r.note("#1962 key -> first changed pixel (kl3, first keys), skips on vs off: p50 %s / %s ms (Δ %s; kl3 resolution %.2f ms; the verdict on key -> paint "
               "is latency_paint's)" % ((on["key_to_paint"] or {}).get("p50"), (off["key_to_paint"] or {}).get("p50"), deltas["key_to_paint_p50"],
                                        cases.KL3_QUANTUM_MS))
    for w in warns:
        r.note("WARN " + w)
    r.check(all(oks) and bool(oks), "key view skip A/B: a relaunch reached no prompt or no window rect")
    r.check(bool(back), "key view skip A/B: the app reached no prompt after the final relaunch with the default environment")
    if not any(arms[a][p]["key_frames"] for a in arms for p in PARTS):
        r.unverifiable("no hop=key_frame_build line in either arm (a build before #1961, or input-order logging off): nothing to compare")
        return
    need = max(1, n // 3)
    for a in ("on", "off"):
        for p in PARTS:
            r.check(arms[a][p]["key_frames"] >= need, "key view skip A/B: %d key frames logged for the %s keys of the %s arm, expected at least %d" % (
                arms[a][p]["key_frames"], p, a, need))
    for f in fails:
        r.check(False, "key view skip A/B: " + f)
