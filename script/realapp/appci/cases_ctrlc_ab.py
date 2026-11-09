# appci/cases_ctrlc_ab.py: #1838 owner decision: the macOS blocked-frame gate (kill switch PHOSPHOR_MAC_SKIP_BLOCKED_FRAMES=0, default ON) is judged by an A/B in the
# SAME run: the Ctrl-C-under-flood measurement of ctrl_c_d (cases.ctrlc_samples, flood=True) once with the gate on (the default) and once with it off, in blocks
# that alternate (lat.block_schedule, like the Phosphor / Terminal.app A/B) so drift hits both arms equally.
#   ctrl_c_flood_ab   macOS only (SKIPs elsewhere, Windows in winmap.NOT_WINDOWS); PROVING (fix tier)
# WHY RELAUNCH, NOT TWO INSTANCES: frame_latency::gate_enabled() reads the variable ONCE (OnceLock), so the arm cannot change inside a running app. Two
# instances alternated per block would need two windows, two ctl dirs/pty recorders and a focus switch that MacApp (one instance per job, refused on purpose)
# and the guard do not support; one app relaunched per block, on the SAME profile, with the env var passed through `open --env` (MacApp.extra_env), changes
# exactly one thing. The cost is a relaunch per block (the first sample of each block is a discarded lead, as in every interleaved case).
# The ON arm is still judged against the budget / reference by ctrl_c_d (unchanged); this case reports the A/B as metrics and a note, and WARNs.
# Pure parts (log parsing, merge, verdict) are covered by selftest.py: ab_*.
import os, re, statistics, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases  # noqa: E402  (cases.py imports this module as its last line)
from cases import case, stats  # noqa: E402
import lat  # noqa: E402
import render_hops  # noqa: E402

GATE_ENV = "PHOSPHOR_MAC_SKIP_BLOCKED_FRAMES"
THROUGHPUT_WARN_PCT = 10.0          # ON arm's flood throughput this far below OFF = WARN
WATCHDOG_HELD_MS = 100.0            # frame_skip_run held_ms at or above this = the 100 ms watchdog fired
WAIT_MIN_MS = 2.0                   # drawable_wait counted from this length
FLOOD_GAP_S = 0.25                  # pty_read lines closer than this belong to one flood run
FLOOD_MIN_READS = 10                # a run needs this many reads to count as a flood
_T = re.compile(r" t=(\d+(?:\.\d+)?)")


def _t(line, t0):
    m = _T.search(line)
    if not m:
        return None
    t = float(m.group(1))
    return t if t / 1000.0 >= t0 else None


def ab_arm_log_metrics(lines, t0):
    """one block's raw app-log facts since wall time t0 -> dict of raw numbers (merge with ab_merge, summarise with ab_summary):
    skip_runs / held (hop=frame_skip_run skipped= held_ms=), waits (hop=drawable_wait wait_ms), k2k (each ctrl=true os_key_event -> the next hop=ctrl_c_keydown, ms),
    flood_bytes / flood_s (pty_read bytes= in runs of >= FLOOD_MIN_READS reads less than FLOOD_GAP_S apart; span first -> last read of each run)."""
    held, waits, k2k, reads = [], [], [], []
    ev = None
    for l in lines:
        t = _t(l, t0)
        if t is None:
            continue
        if "hop=frame_skip_run" in l:
            m = re.search(r"held_ms=(\d+(?:\.\d+)?)", l)
            if m:
                held.append(float(m.group(1)))
        elif "hop=drawable_wait" in l:
            m = re.search(r"wait_ms=(\d+(?:\.\d+)?)", l)
            if m:
                waits.append(float(m.group(1)))
        elif "hop=os_key_event" in l:
            ev = t if " ctrl=true" in l else ev
        elif "hop=ctrl_c_keydown" in l:
            if ev is not None:
                k2k.append(t - ev)
                ev = None
        elif " pty_read bytes=" in l:
            m = re.search(r"pty_read bytes=(\d+)", l)
            if m:
                reads.append((t / 1000.0, int(m.group(1))))
    fb, fs, run = 0, 0.0, []
    for rd in reads + [None]:
        if rd is not None and (not run or rd[0] - run[-1][0] <= FLOOD_GAP_S):
            run.append(rd)
            continue
        if len(run) >= FLOOD_MIN_READS:
            fb += sum(b for _, b in run[1:])        # the first read opens the span, so its bytes sit outside it
            fs += run[-1][0] - run[0][0]
        run = [rd] if rd is not None else []
    return dict(skip_runs=len(held), held=held, waits=waits, k2k=k2k, flood_bytes=fb, flood_s=fs)


def ab_merge(blocks):
    out = dict(skip_runs=0, held=[], waits=[], k2k=[], flood_bytes=0, flood_s=0.0)
    for b in blocks:
        out["skip_runs"] += b["skip_runs"]
        out["flood_bytes"] += b["flood_bytes"]
        out["flood_s"] += b["flood_s"]
        for k in ("held", "waits", "k2k"):
            out[k] += b[k]
    return out


def ab_summary(samples, raw):
    """one arm -> the metric dict: p50 / p95 of its Ctrl-C latency samples plus the app-log counts"""
    st = stats(samples)
    return dict(n=st["n"], p50=st["p50"], p95=st["p95"], max=st["max"],
                frame_skip_runs=raw["skip_runs"], held_ms_max=round(max(raw["held"]), 1) if raw["held"] else None,
                held_ge_100ms=sum(1 for h in raw["held"] if h >= WATCHDOG_HELD_MS),
                drawable_wait_ge_2ms=sum(1 for w in raw["waits"] if w >= WAIT_MIN_MS),
                os_key_event_to_ctrl_c_keydown_p50=round(statistics.median(raw["k2k"]), 2) if raw["k2k"] else None,
                flood_bytes_per_s=round(raw["flood_bytes"] / raw["flood_s"]) if raw["flood_s"] > 0 else None)


def ab_verdict(on, off):
    """(delta_p50 or None, [WARN texts]) from the two ab_summary dicts. Delta = ON minus OFF (negative = the gate helps)."""
    warns = []
    d = round(on["p50"] - off["p50"], 2) if on.get("p50") is not None and off.get("p50") is not None else None
    a, b = on.get("flood_bytes_per_s"), off.get("flood_bytes_per_s")
    if a is not None and b:
        pct = (b - a) * 100.0 / b
        if pct > THROUGHPUT_WARN_PCT:
            warns.append("gate-on flood throughput %d B/s is %.1f%% below gate-off %d B/s (limit %g%%)" % (a, pct, b, THROUGHPUT_WARN_PCT))
    if on.get("held_ge_100ms"):
        warns.append("the 100 ms watchdog fired %d time(s) with the gate on (frame_skip_run held_ms max %s)" % (on["held_ge_100ms"], on.get("held_ms_max")))
    return d, warns


def ab_schedule(n, warm):
    """the block order: lat.block_schedule's phosphor / reference sides are the gate-on / gate-off arms -> [(arm, count, discard)], ON first"""
    return [("on" if side == "phosphor" else "off", cnt, disc) for side, cnt, disc in lat.block_schedule(n, lat.interleave_block(), warm, lat.interleave_lead())]


def _arm_env(arm):
    return {GATE_ENV: "0"} if arm == "off" else {}


@case("ctrl_c_flood_ab", order=18, gui=True)
def c_ctrl_c_flood_ab(ctx, r):
    """#1838 A/B: Ctrl-C under a `yes` flood with the blocked-frame gate ON vs OFF, interleaved in blocks in this run (relaunch per block). Metrics
    ctrl_c_flood_gate_on / ctrl_c_flood_gate_off (p50, p95, counts, throughput), a delta note, WARN on a >10 % throughput loss or a watchdog hit.
    Fails only on harness-level problems (a relaunch that reached no prompt, too few delivered samples)."""
    import cases_robust
    if ctx.win or ctx.be.name != "mac":
        r.skip("the blocked-frame gate (#1838) exists only in the macOS renderer")
    if not hasattr(ctx.app, "extra_env"):
        r.skip("this app launcher cannot pass an extra environment to a relaunch")
    n, warm = ctx.count_lat(30), lat.warmup()
    samples, raws, oks, seq = {"on": [], "off": []}, {"on": [], "off": []}, [], 0
    hop_lines = {"on": [], "off": []}
    try:
        for arm, cnt, disc in ab_schedule(n, warm):
            ctx.app.extra_env = _arm_env(arm)
            _p, up = cases_robust.start_app(ctx, r, "ctrl_c_flood_ab gate %s" % arm)
            if not up:
                oks.append(False)
                break
            time.sleep(cases.LAT_SETTLE_S)
            ctx.reset()
            seq += 1
            t0 = time.time()
            got, ok = cases.ctrlc_samples(ctx, cnt, "ccab_%s_%d.txt" % (arm, seq), budget_s=max(120, cnt * 5), flood=True)
            oks.append(ok)
            samples[arm] += got[disc:]
            lines = ctx.app_log_lines()
            raws[arm].append(ab_arm_log_metrics(lines, t0))       # this launch's log: the next relaunch overwrites it
            hop_lines[arm] += render_hops.window_lines(lines, t0)
            ctx.reset()
    finally:
        ctx.app.extra_env = {}
        _p, back = cases_robust.start_app(ctx, r, "ctrl_c_flood_ab: relaunch with the default environment")
        ctx.reset()
    on, off = (ab_summary(samples[a], ab_merge(raws[a])) for a in ("on", "off"))
    r.metric("ctrl_c_flood_gate_on", on)
    r.metric("ctrl_c_flood_gate_off", off)
    for arm in ("on", "off"):
        render_hops.record(r, hop_lines[arm], prefix="ctrl_c_flood_gate_%s_" % arm)        # render-latency step A hops per arm (key_frames_skipped, key_presented); metrics only
    d, warns = ab_verdict(on, off)
    r.metric("ctrl_c_flood_gate_delta_p50_ms", d)
    r.note("gate on vs off: Δp50 = %s ms (on %s / off %s; p95 %s / %s; n %d / %d; blocks of %d, relaunch per block)" % (
        d if d is not None else "n/a", on["p50"], off["p50"], on["p95"], off["p95"], on["n"], off["n"], lat.interleave_block()))
    r.note("gate on: %d frame_skip_run, %d drawable_wait >= %g ms, key->ctrl_c_keydown p50 %s ms, flood %s B/s; gate off: %d, %d, %s ms, %s B/s" % (
        on["frame_skip_runs"], on["drawable_wait_ge_2ms"], WAIT_MIN_MS, on["os_key_event_to_ctrl_c_keydown_p50"], on["flood_bytes_per_s"],
        off["frame_skip_runs"], off["drawable_wait_ge_2ms"], off["os_key_event_to_ctrl_c_keydown_p50"], off["flood_bytes_per_s"]))
    for w in warns:
        r.note("WARN " + w)
    if off["frame_skip_runs"]:
        r.note("WARN gate off still logged %d frame_skip_run: the kill switch did not reach the app (extra_env not passed?)" % off["frame_skip_runs"])
    r.check(all(oks) and bool(oks), "Ctrl-C under flood A/B: a relaunch reached no prompt or fewer samples than asked stopped `yes` and ran the trap")
    r.check(len(samples["on"]) >= n and len(samples["off"]) >= n, "Ctrl-C under flood A/B: kept samples on %d / off %d, expected %d each" % (len(samples["on"]), len(samples["off"]), n))
    r.check(bool(back), "Ctrl-C under flood A/B: the app reached no prompt after the final relaunch with the default environment")
