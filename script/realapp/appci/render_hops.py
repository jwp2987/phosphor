# appci/render_hops.py: parser for the render-latency step A hops (docs/design/render-latency.md). Metrics only, never a verdict.
#   macOS  hop=key_presented submit_to_presented_ms= key_to_presented_ms= sync=<0|1> wait_ms=   (crates/warpui/src/platform/mac/frame_latency.rs)
#   macOS  hop=key_frame_sched ms=                    request_redraw of a key-driven frame -> displayLayer entry
#   macOS  hop=frame_skip_run skipped= held_ms= key_frame_skipped=   (how often the #1838 gate skipped a KEY-driven frame)
#   Linux/Windows  hop=frame_drawn total_ms= acquire_ms= present_ms=   (present_ms = time inside surface_texture.present())
#   key frames drawn at once (#1843 Linux/Wayland, #1848 macOS; docs/design/1843-1848-latency.md):
#   Linux  hop=key_frame_now wait_ms=        a key's frame drawn ahead of the compositor's frame callback (wait = acquire + present of that frame)
#   Linux  hop=key_frame_now_off wait_ms=    that path turned itself off for the window (two blocked frames in a row)
#   macOS  hop=key_frame_now drawn=<0|1> ms= a key's frame drawn at the end of its key press (drawn=0: the frame did not go out, the display pass draws it)
#   macOS  hop=echo_frame_now drawn=<0|1> ms= key_age_ms=   (#2039, docs/design/r14-echoframe.md) the first frame asked for after a key press (the echo of a
#          key typed into a running program) drawn when its request runs, not at the display pass (drawn=0: it did not go out, the display pass draws it)
#   per-key frame build split (docs/design/key-scene-build.md; crates/warpui_core/src/frame_split.rs), every platform, the first frame built after a key-down:
#   hop=notify_cause view= cause=   (#1995) which ctx.notify() asked for a re-render of Input; counted per key frame build into input_notify_causes
#   hop=key_frame_build passes= layout_passes= views= mounted= render_ms= layout_ms= after_layout_ms= paint_ms= hover_ms= total_ms= layout_top= paint_top=
#     views = views re-rendered, mounted = views in the tree, hover_ms = the synthetic mouse-move after the build, *_top = `ViewType:ms,...` (largest self time)
# A build before step A logs none of the new fields: every function returns an empty dict then, and callers say so (the empty result reads like "no samples").
import re, statistics

_NUM = r"(\d+(?:\.\d+)?)"
_T = re.compile(r" t=" + _NUM)
_KEY_PRESENTED = re.compile(r"hop=key_presented submit_to_presented_ms=" + _NUM + r" key_to_presented_ms=" + _NUM + r" sync=([01]) wait_ms=" + _NUM)
_KEY_SCHED = re.compile(r"hop=key_frame_sched ms=" + _NUM)
_SKIP_RUN = re.compile(r"hop=frame_skip_run skipped=(\d+) held_ms=" + _NUM + r"(?: key_frame_skipped=(\d+))?")
_FRAME_DRAWN = re.compile(r"hop=frame_drawn total_ms=" + _NUM + r" acquire_ms=" + _NUM + r"(?: present_ms=" + _NUM + r")?")
_KEY_FRAME_NOW = re.compile(r"hop=key_frame_now (?:wait_ms=" + _NUM + r"|drawn=([01]) ms=" + _NUM + r")")
_NOTIFY_CAUSE = re.compile(r"hop=notify_cause view=(\w+) cause=(\S+)")     # #1995: which call asked for a re-render of a view (Input: app/src/terminal/input.rs notify_cause!)
_KEY_FRAME_NOW_OFF = re.compile(r"hop=key_frame_now_off\b")
_ECHO_FRAME_NOW = re.compile(r"hop=echo_frame_now drawn=([01]) ms=" + _NUM + r" key_age_ms=" + _NUM)       # frame_latency.rs echo_frame_now_detail
_BUILD_STAGES = ("render", "layout", "after_layout", "paint", "hover", "total")
_KEY_FRAME_BUILD = re.compile(r"hop=key_frame_build passes=(\d+) layout_passes=(\d+) views=(\d+) mounted=(\d+) "
                              + " ".join(st + "_ms=" + _NUM for st in _BUILD_STAGES) + r" layout_top=(\S+) paint_top=(\S+)")
_TOP_VIEWS = 5      # view types named per key_frame_build_*_top_ms metric


def _st(xs):
    """{n, p50, p95, max} of a list of ms (the shape cases.stats gives; kept local so this module imports nothing from cases)"""
    xs = sorted(xs)
    return dict(n=len(xs), p50=round(statistics.median(xs), 2), p95=round(xs[min(len(xs) - 1, int(len(xs) * 0.95))], 2), max=round(xs[-1], 2))


def window_lines(txt_or_lines, t0=None, t1=None):
    """the lines inside [t0, t1] (wall seconds; the log's ` t=` is epoch ms); a line without ` t=` is kept only when no window is asked for"""
    lines = txt_or_lines.splitlines() if isinstance(txt_or_lines, str) else txt_or_lines
    out = []
    for l in lines:
        if t0 is None and t1 is None:
            out.append(l)
            continue
        m = _T.search(l)
        if not m:
            continue
        t = float(m.group(1)) / 1000.0
        if (t0 is None or t >= t0) and (t1 is None or t <= t1):
            out.append(l)
    return out


def _top_views(fields, frames):
    """['Terminal:0.31,Workspace:0.10', '-', ...] (one layout_top= / paint_top= field per key frame) -> {view type: mean ms per key frame}, the _TOP_VIEWS
    largest. Each field only names that frame's three largest, so a type's mean is a lower bound; {} when no frame timed its views"""
    tot = {}
    for f in fields:
        for part in ([] if f == "-" else f.split(",")):
            name, _, ms = part.rpartition(":")
            try:
                tot[name] = tot.get(name, 0.0) + float(ms)
            except ValueError:
                continue
    top = sorted(tot.items(), key=lambda kv: (-kv[1], kv[0]))[:_TOP_VIEWS]
    return {name: round(ms / frames, 3) for name, ms in top}


def render_hop_metrics(txt_or_lines, t0=None, t1=None):
    """-> {metric name: {n, p50, p95, max}} for the step-A hops in the window (absent names = no such line). Names:
    key_submit_to_presented_ms, key_to_presented_ms_hop, key_presented_wait_ms (macOS), key_frame_sched_ms (macOS), frame_present_ms,
    frame_acquire_ms (Linux/Windows). Plus the plain counts key_presented_sync_on / key_presented_sync_off, key_frames_skipped, skip_runs, and for the key frames drawn at once
    (#1843/#1848): key_frames_now (count), key_frames_now_drawn (macOS: those that went to the compositor), key_frames_now_off (Linux: the path switched itself
    off), key_frame_now_wait_ms (Linux) / key_frame_now_ms (macOS). And for the per-key build split (hop=key_frame_build): key_frame_build_<stage>_ms for
    render / layout / after_layout / paint / hover / total, key_frame_build_other_ms (total minus the five stages), key_frame_build_views (views re-rendered
    per key frame), key_frame_build_mounted, the counts key_frame_builds and key_frame_builds_multi_pass (a second layout + paint in the same frame), and
    key_frame_build_layout_top_ms / key_frame_build_paint_top_ms ({view type: mean self ms per key frame}). And for the echo frames drawn at once (#2039,
    macOS): echo_frames_now (attempts), echo_frames_now_drawn (those that went to the compositor), echo_frame_now_ms (the draw), echo_frame_key_age_ms
    (key press -> the draw began)."""
    sub, k2p, wait, sync_on, sync_off, sched, kskip, runs, pres, acq = [], [], [], 0, 0, [], 0, 0, [], []
    now_n, now_drawn, now_off, now_wait, now_ms = 0, 0, 0, [], []
    echo_n, echo_drawn, echo_ms, echo_age = 0, 0, [], []
    build = {st: [] for st in _BUILD_STAGES + ("other", "views", "mounted")}
    build_multi, build_ltop, build_ptop = 0, [], []
    pending, causes, cause_frames = [], {}, 0       # #1995: notify_cause lines since the previous key frame build
    for l in window_lines(txt_or_lines, t0, t1):
        m = _NOTIFY_CAUSE.search(l)
        if m:
            pending.append("%s:%s" % (m.group(1), m.group(2)))
            continue
        m = _KEY_PRESENTED.search(l)
        if m:
            sub.append(float(m.group(1)))
            k2p.append(float(m.group(2)))
            wait.append(float(m.group(4)))
            sync_on, sync_off = sync_on + (m.group(3) == "1"), sync_off + (m.group(3) == "0")
            continue
        m = _KEY_SCHED.search(l)
        if m:
            sched.append(float(m.group(1)))
            continue
        m = _SKIP_RUN.search(l)
        if m:
            runs += 1
            kskip += int(m.group(3) or 0)
            continue
        m = _KEY_FRAME_BUILD.search(l)
        if m:
            if pending:
                cause_frames += 1
                for c in pending:
                    causes[c] = causes.get(c, 0) + 1
                pending = []
            stages = [float(x) for x in m.groups()[4:10]]
            for st, v in zip(_BUILD_STAGES, stages):
                build[st].append(v)
            build["other"].append(max(0.0, stages[5] - sum(stages[:5])))
            build["views"].append(int(m.group(3)))
            build["mounted"].append(int(m.group(4)))
            build_multi += int(m.group(1)) > 1
            build_ltop.append(m.group(11))
            build_ptop.append(m.group(12))
            continue
        if _KEY_FRAME_NOW_OFF.search(l):
            now_off += 1
            continue
        m = _KEY_FRAME_NOW.search(l)
        if m:
            now_n += 1
            if m.group(1) is not None:
                now_wait.append(float(m.group(1)))
            else:
                now_drawn += m.group(2) == "1"
                now_ms.append(float(m.group(3)))
            continue
        m = _ECHO_FRAME_NOW.search(l)
        if m:
            echo_n += 1
            echo_drawn += m.group(1) == "1"
            echo_ms.append(float(m.group(2)))
            echo_age.append(float(m.group(3)))
            continue
        m = _FRAME_DRAWN.search(l)
        if m:
            acq.append(float(m.group(2)))
            if m.group(3) is not None:
                pres.append(float(m.group(3)))
    out = {}
    for name, xs in (("key_submit_to_presented_ms", sub), ("key_to_presented_ms_hop", k2p), ("key_presented_wait_ms", wait),
                     ("key_frame_sched_ms", sched), ("frame_present_ms", pres), ("frame_acquire_ms", acq),
                     ("key_frame_now_wait_ms", now_wait), ("key_frame_now_ms", now_ms),
                     ("echo_frame_now_ms", echo_ms), ("echo_frame_key_age_ms", echo_age)):
        if xs:
            out[name] = _st(xs)
    if build["total"]:
        n = len(build["total"])
        out["key_frame_builds"], out["key_frame_builds_multi_pass"] = n, build_multi
        for st in _BUILD_STAGES + ("other",):
            out["key_frame_build_%s_ms" % st] = _st(build[st])
        out["key_frame_build_views"], out["key_frame_build_mounted"] = _st(build["views"]), _st(build["mounted"])
        for name, fields in (("layout", build_ltop), ("paint", build_ptop)):
            top = _top_views(fields, n)
            if top:
                out["key_frame_build_%s_top_ms" % name] = top
    if causes:      # informational (#1995): notifies logged between key frame builds, per `view:cause`; every notify of a view counts, not only the ones that rendered
        out["input_notify_causes"], out["input_notify_frames"] = dict(sorted(causes.items(), key=lambda kv: (-kv[1], kv[0]))), cause_frames
    if sub:
        out["key_presented_sync_on"], out["key_presented_sync_off"] = sync_on, sync_off
    if runs:
        out["key_frames_skipped"], out["skip_runs"] = kskip, runs
    if now_n or now_off:
        out["key_frames_now"] = now_n
        if now_ms:
            out["key_frames_now_drawn"] = now_drawn
        if now_off:
            out["key_frames_now_off"] = now_off
    if echo_n:
        out["echo_frames_now"], out["echo_frames_now_drawn"] = echo_n, echo_drawn
    return out


def input_notify_note(m, top=4):
    """#1995: one note line from render_hop_metrics' input_notify_causes ({`View:cause`: count}, already sorted by count) and input_notify_frames, or None
    when the window logged no notify before a key frame build. Pure."""
    causes = m.get("input_notify_causes") or {}
    if not causes:
        return None
    return "#1995 notifies logged before %d of %s key frame build(s): %s%s" % (
        m.get("input_notify_frames", 0), m.get("key_frame_builds", "?"), ", ".join("%s x%d" % kv for kv in list(causes.items())[:top]),
        " (+%d more)" % (len(causes) - top) if len(causes) > top else "")


def record(r, txt_or_lines, t0=None, t1=None, prefix=""):
    """records render_hop_metrics as case metrics `<prefix><name>`; a note when a build logs none of them. -> the dict"""
    m = render_hop_metrics(txt_or_lines, t0, t1)
    for k, v in m.items():
        r.metric(prefix + k, v)
    if not m:
        r.note("render-latency hops: none in the window (a build before step A, or input-order logging off): no present/sched split recorded%s" % (" for " + prefix.rstrip("_") if prefix else ""))
    return m


# ---------------------------------------------------------------- Linux present-mode / frame-latency A/B (design option F, zero code)
# agent_latency_key relaunches the app per block with the environment variable of one of these arms (App.extra_env), like cases_ctrlc_ab: the variables are read once at
# start-up (crates/warpui/src/rendering/wgpu/resources.rs), so the arm cannot change inside a running app.
# keynow_off (#1843): the kill switch of the key frame drawn ahead of the frame callback (crates/warpui/src/windowing/winit/event_loop/key_frame.rs), so the
# default arm (path ON) and the old behaviour are measured in the same run, interleaved.
KEY_FRAME_NOW_ENV = "PHOSPHOR_KEY_FRAME_NOW"
ARM_ENV = {"default": {}, "keynow_off": {KEY_FRAME_NOW_ENV: "0"}}
ARMS = ("default", "keynow_off")       # the mailbox / frame-latency-1 arms were dropped (owner, round 8: raw -0.92 / +0.52 ms, slot-adjusted +0.1 both)


def arm_env(arm):
    """the environment a relaunch for `arm` adds (a copy)"""
    return dict(ARM_ENV[arm])


def arm_schedule(arms, n, blocks, lead):
    """-> [(arm, keys to type, keys to discard)]: `blocks` rounds, each visiting every arm once in order (A/B/C/A/B/C), so drift hits every arm alike.
    Each arm is kept for n samples in total (ceil(n / blocks) per block); the first `lead` samples of every block are discarded (app just relaunched)."""
    per = -(-n // max(blocks, 1))
    return [(a, per + lead, lead) for _ in range(max(blocks, 1)) for a in arms]


def arm_summary(raw, paced, txt):
    """one arm's metric dict: streaming key->frame (raw and slot-adjusted, ms) and the present / acquire split of its frames (None where not logged)"""
    h = render_hop_metrics(txt)
    return dict(streaming=_st(raw) if raw else None, slot_adjusted=_st(paced) if paced else None,
                present_ms=h.get("frame_present_ms"), acquire_ms=h.get("frame_acquire_ms"),
                key_frames_now=h.get("key_frames_now", 0), key_frames_now_off=h.get("key_frames_now_off", 0), key_frame_now_wait_ms=h.get("key_frame_now_wait_ms"))


def key_frame_now_warns(arms):
    """[WARN texts] about the #1843 A/B itself (pure): the kill-switch arm still drew key frames at once (the variable did not reach the app), the default arm
    drew none while it has samples (a build without the path, a compositor that is not Wayland, or a present mode that waits: then the two arms are the same
    measurement), or the path switched itself off (a frame blocked in acquire / present)."""
    out = []
    off, on = arms.get("keynow_off") or {}, arms.get("default") or {}
    if off.get("key_frames_now"):
        out.append("the keynow_off arm logged %d hop=key_frame_now: the kill switch %s=0 did not reach the app" % (off["key_frames_now"], KEY_FRAME_NOW_ENV))
    if on.get("streaming") and not on.get("key_frames_now"):
        out.append("the default arm logged no hop=key_frame_now: key frames were not drawn ahead of the frame callback (X11, a present mode that waits for vsync, "
                   "or a build before #1843), so default and keynow_off measure the same thing")
    if on.get("key_frames_now_off"):
        out.append("the default arm logged hop=key_frame_now_off %d time(s): a key frame blocked in acquire / present and the path switched itself off "
                   "(key_frame_now wait_ms p95 %s)" % (on["key_frames_now_off"], (on.get("key_frame_now_wait_ms") or {}).get("p95")))
    return out


def arm_deltas(arms):
    """{arm: {raw_p50_delta, adjusted_p50_delta}} of each non-default arm against `default` (negative = the arm is faster); None where a side has no samples"""
    base = arms.get("default") or {}

    def d(a, b, key):
        x, y = (a.get(key) or {}).get("p50"), (b.get(key) or {}).get("p50")
        return round(x - y, 2) if x is not None and y is not None else None
    return {name: dict(raw_p50_delta=d(v, base, "streaming"), adjusted_p50_delta=d(v, base, "slot_adjusted")) for name, v in arms.items() if name != "default"}
