# appci/cases_keyframe_ab.py: #1848 (docs/design/1843-1848-latency.md): the macOS key frame drawn at the end of the key press (kill switch
# PHOSPHOR_MAC_KEY_FRAME_NOW=0, default ON) is judged by an A/B in the SAME run, as #1838's gate is (cases_ctrlc_ab): the key -> first changed pixel
# measurement of latency_paint (cases.paint_samples: kl3, ScreenCaptureKit at 240 Hz) once with the path on (the default) and once with it off, in blocks
# that alternate (cases_ctrlc_ab.ab_schedule = lat.block_schedule) so drift hits both arms equally.
#   latency_paint_ab   macOS only (SKIPs elsewhere, Windows in winmap.NOT_WINDOWS); PROVING (fix tier)
# WHY RELAUNCH: frame_latency::key_frame_now_enabled() reads the variable ONCE (OnceLock); see cases_ctrlc_ab.py for why one relaunched app, not two instances.
# The ON arm is still judged against Terminal.app and the 2 ms budget by latency_paint (unchanged, the raw overhead is the verdict); this case reports the A/B
# as metrics and a note, and WARNs when the A/B itself is not what it claims (the switch did not reach the app, the path never ran, its frames did not go out).
# Linux has the same A/B inside agent_latency_key (render_hops arm keynow_off, #1843). Pure parts are covered by selftest.py: keyframe_ab_*.
import os, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases  # noqa: E402  (cases.py imports this module as one of its last lines)
from cases import case, stats  # noqa: E402
import cases_ctrlc_ab  # noqa: E402
import lat  # noqa: E402
import render_hops  # noqa: E402

KEY_FRAME_ENV = "PHOSPHOR_MAC_KEY_FRAME_NOW"
NOT_DRAWN_WARN_PCT = 50.0           # ON arm: this share of key frames that did not go out at once (drawn=0) = WARN (the path is mostly not taken)


def arm_env(arm):
    return {KEY_FRAME_ENV: "0"} if arm == "off" else {}


def ab_summary(samples, hops):
    """one arm -> the metric dict: key -> paint p50 / p95 (kl3) plus what the app logged for its key frames (render_hops.render_hop_metrics of the arm's lines)"""
    st = stats(samples)
    return dict(n=st["n"], p50=st["p50"], p95=st["p95"], max=st["max"],
                key_frames_now=hops.get("key_frames_now", 0), key_frames_now_drawn=hops.get("key_frames_now_drawn", 0),
                key_frame_sched_p50=(hops.get("key_frame_sched_ms") or {}).get("p50"), key_frame_sched_p95=(hops.get("key_frame_sched_ms") or {}).get("p95"),
                key_to_presented_p50=(hops.get("key_to_presented_ms_hop") or {}).get("p50"),
                submit_to_presented_p50=(hops.get("key_submit_to_presented_ms") or {}).get("p50"),
                key_frames_skipped=hops.get("key_frames_skipped", 0))


def ab_verdict(on, off):
    """(delta_p50 or None, [WARN texts]) from the two ab_summary dicts. Delta = ON minus OFF, key -> paint ms (negative = drawing the key frame at once helps).
    The WARNs are about the A/B, never about the latency (latency_paint judges that)."""
    warns = []
    d = round(on["p50"] - off["p50"], 2) if on.get("p50") is not None and off.get("p50") is not None else None
    if off.get("key_frames_now"):
        warns.append("the off arm logged %d hop=key_frame_now: the kill switch %s=0 did not reach the app (extra_env not passed?)" % (off["key_frames_now"], KEY_FRAME_ENV))
    if on.get("n") and not on.get("key_frames_now"):
        warns.append("the on arm logged no hop=key_frame_now: no key frame was drawn at the end of a key press (a build before #1848, or every key met a frame "
                     "still in flight), so both arms measure the same thing")
    elif on.get("key_frames_now"):
        missed = on["key_frames_now"] - on.get("key_frames_now_drawn", 0)
        pct = missed * 100.0 / on["key_frames_now"]
        if pct >= NOT_DRAWN_WARN_PCT:
            warns.append("%d of %d key frames drawn at once did not go out (drawn=0: gate skip, app borrowed or no drawable; %.0f%%, limit %g%%): the display pass "
                         "drew them as before" % (missed, on["key_frames_now"], pct, NOT_DRAWN_WARN_PCT))
    return d, warns


@case("latency_paint_ab", order=44, gui=True)
def c_latency_paint_ab(ctx, r):
    """#1848 A/B: key -> first changed pixel (kl3) with the key frame drawn at the end of the key press ON vs OFF, interleaved in blocks in this run (relaunch
    per block). Metrics key_frame_now_on / key_frame_now_off (p50, p95, the app's own hops per arm), a delta note, WARNs about the A/B itself.
    Fails only on harness-level problems (a relaunch that reached no prompt, too few samples from kl3)."""
    import cases_robust
    if ctx.win or ctx.be.name != "mac" or not hasattr(ctx.be, "kl_start"):
        r.skip("the key frame drawn at the end of the key press (#1848) exists only in the macOS renderer, and kl3 (ScreenCaptureKit) is macOS only; "
               "Linux measures its own (#1843) inside agent_latency_key (arm keynow_off)")
    if not hasattr(ctx.app, "extra_env"):
        r.skip("this app launcher cannot pass an extra environment to a relaunch")
    n, warm = ctx.count_lat(30), lat.warmup()
    samples, hop_lines, oks = {"on": [], "off": []}, {"on": [], "off": []}, []
    back = None
    try:
        for arm, cnt, disc in cases_ctrlc_ab.ab_schedule(n, warm):
            ctx.app.extra_env = arm_env(arm)
            _p, up = cases_robust.start_app(ctx, r, "latency_paint_ab key frame %s" % arm)
            if not up:
                oks.append(False)
                break
            time.sleep(cases.LAT_SETTLE_S)
            ctx.reset()
            rect = ctx.be.window_rect()
            if not rect or rect[2] < 100:
                oks.append(False)
                r.note("latency_paint_ab: no Phosphor window rect from System Events after the relaunch (arm %s)" % arm)
                break
            ctx.be.guard()
            t0 = time.time()
            got = cases.paint_samples(ctx, rect, cnt)
            oks.append(True)
            samples[arm] += got[disc:]
            hop_lines[arm] += render_hops.window_lines(ctx.app_log_lines(), t0)       # this launch's log: the next relaunch overwrites it
            ctx.reset()
    finally:
        ctx.app.extra_env = {}
        _p, back = cases_robust.start_app(ctx, r, "latency_paint_ab: relaunch with the default environment")
        ctx.reset()
    hops = {a: render_hops.render_hop_metrics(hop_lines[a]) for a in ("on", "off")}
    on, off = (ab_summary(samples[a], hops[a]) for a in ("on", "off"))
    r.metric("key_frame_now_on", on)
    r.metric("key_frame_now_off", off)
    for a in ("on", "off"):
        for k, v in hops[a].items():
            r.metric("key_frame_now_%s_%s" % (a, k), v)
    d, warns = ab_verdict(on, off)
    r.metric("key_frame_now_delta_p50_ms", d)
    r.note("key frame at the end of the key press, on vs off: key->paint Δp50 = %s ms (on %s / off %s; p95 %s / %s; n %d / %d; blocks of %d, relaunch per block; kl3 "
           "resolution %.2f ms)" % (d if d is not None else "n/a", on["p50"], off["p50"], on["p95"], off["p95"], on["n"], off["n"], lat.interleave_block(),
                                    cases.KL3_QUANTUM_MS))
    r.note("on: %s key frames drawn at once (%s went out), request->frame start p50 %s ms, key->presented p50 %s ms; off: %s, request->frame start p50 %s ms, "
           "key->presented p50 %s ms" % (on["key_frames_now"], on["key_frames_now_drawn"], on["key_frame_sched_p50"], on["key_to_presented_p50"],
                                         off["key_frames_now"], off["key_frame_sched_p50"], off["key_to_presented_p50"]))
    for w in warns:
        r.note("WARN " + w)
    r.check(all(oks) and bool(oks), "key-frame A/B: a relaunch reached no prompt or no window rect")
    need = max(1, (n * 2) // 3)
    r.check(len(samples["on"]) >= need and len(samples["off"]) >= need,
            "key-frame A/B: kept key->paint samples on %d / off %d, expected at least %d each (kl3 capture region or Screen Recording permission wrong)" % (
                len(samples["on"]), len(samples["off"]), need))
    r.check(bool(back), "key-frame A/B: the app reached no prompt after the final relaunch with the default environment")
