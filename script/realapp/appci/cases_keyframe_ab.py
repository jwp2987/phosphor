# appci/cases_keyframe_ab.py: #1848 (docs/design/1843-1848-latency.md): the macOS key frame drawn at the end of the key press (kill switch
# PHOSPHOR_MAC_KEY_FRAME_NOW=0, default ON) is judged by an A/B in the SAME run, as #1838's gate is (cases_ctrlc_ab): the key -> first changed pixel
# measurement of latency_paint (cases.paint_samples: kl3, ScreenCaptureKit at 240 Hz) once with the path on (the default) and once with it off, in blocks
# that alternate (cases_ctrlc_ab.ab_schedule = lat.block_schedule) so drift hits both arms equally.
#   latency_paint_ab   macOS only (SKIPs elsewhere, Windows in winmap.NOT_WINDOWS); PROVING (fix tier)
# WHY RELAUNCH: frame_latency::key_frame_now_enabled() reads the variable ONCE (OnceLock); see cases_ctrlc_ab.py for why one relaunched app, not two instances.
# The ON arm is still judged against Terminal.app and the 2 ms budget by latency_paint (unchanged, the raw overhead is the verdict); this case reports the A/B
# as metrics and a note, and WARNs when the A/B itself is not what it claims (the switch did not reach the app, the path never ran, its frames did not go out).
# Linux has the same A/B inside agent_latency_key (render_hops arm keynow_off, #1843). Pure parts are covered by selftest.py: keyframe_ab_*.
#   latency_echo_ab    #2039 (docs/design/r14-echoframe.md): the echo frame of a key typed into a running program drawn at once (kill switch
#                      PHOSPHOR_MAC_ECHO_FRAME_NOW=0, default ON), same relaunch-per-block A/B; keys typed into a running `cat` (the latency_key_log method),
#                      key -> frame from the app log per arm, and key -> first changed pixel (kl3) into the same `cat` where kl3 exists. macOS only; PROVING.
#                      Pure parts: selftest.py echoframe_ab_pure_parts_2039.
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


# ---------------------------------------------------------------- #2039: the echo frame drawn at once, on vs off
ECHO_FRAME_ENV = "PHOSPHOR_MAC_ECHO_FRAME_NOW"


def echo_arm_env(arm):
    return {ECHO_FRAME_ENV: "0"} if arm == "off" else {}


def echo_summary(frames, hops, paint=None):
    """one arm -> the metric dict: key -> frame built p50 / p95 (app log, cases.key_frame_pairs), what the app logged for its echo frames
    (render_hops.render_hop_metrics of the arm's lines) and, where kl3 measured it, key -> first changed pixel into the running program"""
    st, pt = stats(frames), stats(paint or [])
    return dict(n=st["n"], p50=st["p50"], p95=st["p95"], max=st["max"],
                echo_frames_now=hops.get("echo_frames_now", 0), echo_frames_now_drawn=hops.get("echo_frames_now_drawn", 0),
                echo_key_age_p50=(hops.get("echo_frame_key_age_ms") or {}).get("p50"),
                key_frame_sched_p50=(hops.get("key_frame_sched_ms") or {}).get("p50"), key_frame_sched_p95=(hops.get("key_frame_sched_ms") or {}).get("p95"),
                key_to_presented_p50=(hops.get("key_to_presented_ms_hop") or {}).get("p50"),
                submit_to_presented_p50=(hops.get("key_submit_to_presented_ms") or {}).get("p50"),
                key_frames_skipped=hops.get("key_frames_skipped", 0), paint_n=pt["n"], paint_p50=pt["p50"], paint_p95=pt["p95"])


def echo_verdict(on, off):
    """({frame, presented, paint}: ON minus OFF of the p50 in ms, None where a side has no figure; [WARN texts]) from two echo_summary dicts. Negative =
    drawing the echo frame at once helps. `frame` is key -> frame built (the app's clock), `presented` key -> on screen as the drawable's presented handler
    saw it, `paint` key -> first changed pixel (kl3). The WARNs are about the A/B itself, never about the latency."""
    def d(key):
        a, b = on.get(key), off.get(key)
        return round(a - b, 2) if a is not None and b is not None else None
    deltas = dict(frame=d("p50"), presented=d("key_to_presented_p50"), paint=d("paint_p50"))
    warns = []
    if off.get("echo_frames_now"):
        warns.append("the off arm logged %d hop=echo_frame_now: the kill switch %s=0 did not reach the app (extra_env not passed?)" % (
            off["echo_frames_now"], ECHO_FRAME_ENV))
    if on.get("n") and not on.get("echo_frames_now"):
        warns.append("the on arm logged no hop=echo_frame_now: no echo frame was drawn at once (a build before #2039, or every echo met a frame still in "
                     "flight), so both arms measure the same thing")
    elif on.get("echo_frames_now"):
        missed = on["echo_frames_now"] - on.get("echo_frames_now_drawn", 0)
        pct = missed * 100.0 / on["echo_frames_now"]
        if pct >= NOT_DRAWN_WARN_PCT:
            warns.append("%d of %d echo frames drawn at once did not go out (drawn=0: gate skip, app borrowed or no drawable; %.0f%%, limit %g%%): the display "
                         "pass drew them as before" % (missed, on["echo_frames_now"], pct, NOT_DRAWN_WARN_PCT))
    return deltas, warns


@case("latency_echo_ab", order=45, gui=True)
def c_latency_echo_ab(ctx, r):
    """#2039 A/B: a key typed into a running `cat` with the echo frame drawn at once ON vs OFF, interleaved in blocks in this run (relaunch per block).
    Per arm: key -> frame built (app log), the app's hops (echo_frame_now, key_frame_sched, key_presented) and, with kl3, key -> first changed pixel.
    Metrics echo_frame_now_on / echo_frame_now_off, the deltas, WARNs about the A/B itself. Fails only on harness-level problems (a relaunch that reached
    no prompt, too few key -> frame pairs)."""
    import cases_robust
    if ctx.win or ctx.be.name != "mac":
        r.skip("the echo frame drawn at once (#2039) exists only in the macOS renderer; other platforms have no display pass to wait for")
    if not hasattr(ctx.app, "extra_env"):
        r.skip("this app launcher cannot pass an extra environment to a relaunch")
    n, warm = ctx.count_lat(30), lat.warmup()
    kl3 = hasattr(ctx.be, "kl_start")
    frames, paint, hop_lines, oks = {"on": [], "off": []}, {"on": [], "off": []}, {"on": [], "off": []}, []
    back = None
    try:
        for arm, cnt, disc in cases_ctrlc_ab.ab_schedule(n, warm):
            ctx.app.extra_env = echo_arm_env(arm)
            _p, up = cases_robust.start_app(ctx, r, "latency_echo_ab echo frame %s" % arm)
            if not up:
                oks.append(False)
                break
            time.sleep(cases.LAT_SETTLE_S)
            ctx.reset()
            # the keys go straight to the pty of a running `cat` and the tty echoes them: the frame after that echo is the one #2039 moves
            ctx.be.type("cat > /dev/null", 10, enter=True)
            time.sleep(1.0)
            t0 = time.time()
            for _i in range(cnt):
                ctx.be.type("m", 10)
                time.sleep(ctx.rng.uniform(0.08, 0.18))
            time.sleep(1.0)
            t1 = time.time()
            lines = render_hops.window_lines(ctx.app_log_lines(), t0, t1)        # this launch's log: the next relaunch overwrites it
            got, _cnt = cases.key_frame_pairs("\n".join(lines))
            oks.append(True)
            frames[arm] += got[disc:]
            hop_lines[arm] += lines
            if kl3:                                                              # the same `cat` is still running: 'm' echoes, Delete erases it
                rect = ctx.be.window_rect()
                if rect and rect[2] >= 100:
                    ctx.be.guard()
                    paint[arm] += cases.paint_samples(ctx, rect, cnt)[disc:]
                else:
                    r.note("latency_echo_ab: no Phosphor window rect from System Events after the relaunch (arm %s): no kl3 samples for this block" % arm)
            ctx.reset()                                                          # Ctrl-C ends the cat
    finally:
        ctx.app.extra_env = {}
        _p, back = cases_robust.start_app(ctx, r, "latency_echo_ab: relaunch with the default environment")
        ctx.reset()
    hops = {a: render_hops.render_hop_metrics(hop_lines[a]) for a in ("on", "off")}
    on, off = (echo_summary(frames[a], hops[a], paint[a]) for a in ("on", "off"))
    r.metric("echo_frame_now_on", on)
    r.metric("echo_frame_now_off", off)
    for a in ("on", "off"):
        for k, v in hops[a].items():
            r.metric("echo_frame_now_%s_%s" % (a, k), v)
    d, warns = echo_verdict(on, off)
    r.metric("echo_frame_now_delta_p50_ms", d["frame"])
    r.metric("echo_frame_now_presented_delta_p50_ms", d["presented"])
    r.metric("echo_frame_now_paint_delta_p50_ms", d["paint"])
    r.note("echo frame drawn at once, on vs off: key->frame Δp50 = %s ms (on %s / off %s; p95 %s / %s; n %d / %d; blocks of %d, relaunch per block)" % (
        d["frame"] if d["frame"] is not None else "n/a", on["p50"], off["p50"], on["p95"], off["p95"], on["n"], off["n"], lat.interleave_block()))
    r.note("on screen: key->presented Δp50 = %s ms (on %s / off %s, the drawable's presented handler); key->paint Δp50 = %s ms (on %s / off %s; n %d / %d; kl3 "
           "resolution %.2f ms%s)" % (d["presented"] if d["presented"] is not None else "n/a", on["key_to_presented_p50"], off["key_to_presented_p50"],
                                      d["paint"] if d["paint"] is not None else "n/a", on["paint_p50"], off["paint_p50"], on["paint_n"], off["paint_n"],
                                      cases.KL3_QUANTUM_MS, "" if kl3 else "; kl3 not available on this backend"))
    r.note("on: %s echo frames drawn at once (%s went out, key age p50 %s ms), request->frame start p50 %s ms; off: %s (%s went out), request->frame start "
           "p50 %s ms" % (on["echo_frames_now"], on["echo_frames_now_drawn"], on["echo_key_age_p50"], on["key_frame_sched_p50"],
                          off["echo_frames_now"], off["echo_frames_now_drawn"], off["key_frame_sched_p50"]))
    for w in warns:
        r.note("WARN " + w)
    r.check(all(oks) and bool(oks), "echo-frame A/B: a relaunch reached no prompt")
    need = max(1, (n * 2) // 3)
    r.check(len(frames["on"]) >= need and len(frames["off"]) >= need,
            "echo-frame A/B: kept key->frame pairs on %d / off %d, expected at least %d each (input_order logging off, or the `cat` did not start)" % (
                len(frames["on"]), len(frames["off"]), need))
    r.check(bool(back), "echo-frame A/B: the app reached no prompt after the final relaunch with the default environment")
