# appci/render_hops.py: parser for the render-latency step A hops (docs/design/render-latency.md). Metrics only, never a verdict.
#   macOS  hop=key_presented submit_to_presented_ms= key_to_presented_ms= sync=<0|1> wait_ms=   (crates/warpui/src/platform/mac/frame_latency.rs)
#   macOS  hop=key_frame_sched ms=                    request_redraw of a key-driven frame -> displayLayer entry
#   macOS  hop=frame_skip_run skipped= held_ms= key_frame_skipped=   (how often the #1838 gate skipped a KEY-driven frame)
#   Linux/Windows  hop=frame_drawn total_ms= acquire_ms= present_ms=   (present_ms = time inside surface_texture.present())
# A build before step A logs none of the new fields: every function returns an empty dict then, and callers say so (the empty result reads like "no samples").
import re, statistics

_NUM = r"(\d+(?:\.\d+)?)"
_T = re.compile(r" t=" + _NUM)
_KEY_PRESENTED = re.compile(r"hop=key_presented submit_to_presented_ms=" + _NUM + r" key_to_presented_ms=" + _NUM + r" sync=([01]) wait_ms=" + _NUM)
_KEY_SCHED = re.compile(r"hop=key_frame_sched ms=" + _NUM)
_SKIP_RUN = re.compile(r"hop=frame_skip_run skipped=(\d+) held_ms=" + _NUM + r"(?: key_frame_skipped=(\d+))?")
_FRAME_DRAWN = re.compile(r"hop=frame_drawn total_ms=" + _NUM + r" acquire_ms=" + _NUM + r"(?: present_ms=" + _NUM + r")?")


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


def render_hop_metrics(txt_or_lines, t0=None, t1=None):
    """-> {metric name: {n, p50, p95, max}} for the step-A hops in the window (absent names = no such line). Names:
    key_submit_to_presented_ms, key_to_presented_ms_hop, key_presented_wait_ms (macOS), key_frame_sched_ms (macOS), frame_present_ms,
    frame_acquire_ms (Linux/Windows). Plus the plain counts key_presented_sync_on / key_presented_sync_off, key_frames_skipped, skip_runs."""
    sub, k2p, wait, sync_on, sync_off, sched, kskip, runs, pres, acq = [], [], [], 0, 0, [], 0, 0, [], []
    for l in window_lines(txt_or_lines, t0, t1):
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
        m = _FRAME_DRAWN.search(l)
        if m:
            acq.append(float(m.group(2)))
            if m.group(3) is not None:
                pres.append(float(m.group(3)))
    out = {}
    for name, xs in (("key_submit_to_presented_ms", sub), ("key_to_presented_ms_hop", k2p), ("key_presented_wait_ms", wait),
                     ("key_frame_sched_ms", sched), ("frame_present_ms", pres), ("frame_acquire_ms", acq)):
        if xs:
            out[name] = _st(xs)
    if sub:
        out["key_presented_sync_on"], out["key_presented_sync_off"] = sync_on, sync_off
    if runs:
        out["key_frames_skipped"], out["skip_runs"] = kskip, runs
    return out


def record(r, txt_or_lines, t0=None, t1=None, prefix=""):
    """records render_hop_metrics as case metrics `<prefix><name>`; a note when a build logs none of them. -> the dict"""
    m = render_hop_metrics(txt_or_lines, t0, t1)
    for k, v in m.items():
        r.metric(prefix + k, v)
    if not m:
        r.note("render-latency hops: none in the window (a build before step A, or input-order logging off): no present/sched split recorded%s" % (" for " + prefix.rstrip("_") if prefix else ""))
    return m


# ---------------------------------------------------------------- Linux present-mode / frame-latency A/B (design option F, zero code)
# agent_latency_key relaunches the app per block with one of these environment variables (App.extra_env), like cases_ctrlc_ab: the variables are read once at
# start-up (crates/warpui/src/rendering/wgpu/resources.rs), so the arm cannot change inside a running app.
ARM_ENV = {"default": {}, "mailbox": {"PHOSPHOR_PRESENT_MODE": "mailbox"}, "lat1": {"PHOSPHOR_FRAME_LATENCY": "1"}}
ARMS = ("default", "mailbox", "lat1")


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
                present_ms=h.get("frame_present_ms"), acquire_ms=h.get("frame_acquire_ms"))


def arm_deltas(arms):
    """{arm: {raw_p50_delta, adjusted_p50_delta}} of each non-default arm against `default` (negative = the arm is faster); None where a side has no samples"""
    base = arms.get("default") or {}

    def d(a, b, key):
        x, y = (a.get(key) or {}).get("p50"), (b.get(key) or {}).get("p50")
        return round(x - y, 2) if x is not None and y is not None else None
    return {name: dict(raw_p50_delta=d(v, base, "streaming"), adjusted_p50_delta=d(v, base, "slot_adjusted")) for name, v in arms.items() if name != "default"}
