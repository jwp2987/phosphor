# appci/cases_agent_latency.py (#1828): agent-mode latency against the perf budgets, and the fast steer while a reply streams. Oracles are the app log hops and
# the scripted fake model's own request log (fake_model.FakeModel.ACTIVE.reqs, APPCI_FAKE_MODEL=1), never a screenshot alone.
#   agent_latency_key    key -> frame (hop=key_frame, the first frame built after a key-down; #1828 hook in crates/warpui_core/src/input_order.rs) in the agent input,
#                        idle and while a slow steady fake reply streams, against the SAME measure in the terminal-mode input editor of the same session
#                        (overhead budget APPCI_KEY_BUDGET_MS, 2 ms). Also the UI-work items (hop=ui_item >= 4 ms) while typing.
#   agent_latency_enter  Enter -> hop=enter_begin (the app's own share of Enter) for a prose line in the agent input, idle and while a reply streams, against a
#                        shell line in the terminal-mode editor (APPCI_ENTER_BUDGET_MS, 2 ms); Enter -> the request arriving at the fake model is a metric.
#   agent_steer_fast     #851 (by design): a 200+ character line with shell-looking text typed at 10-25 ms/key with instant Enter while a reply streams reaches
#                        the agent as a steer exactly once, byte for byte, three times in a row, and nothing reaches the pty or the shell.
# Platforms: Linux, macOS, Windows (all use the loopback fake model and the app log); each needs a build with the hop=key_frame hook (older builds: the key case
# reports unverifiable, never a pass). The steer path exists only in the agent view (there is no terminal-mode twin); the terminal-mode editor is the key and
# Enter cases' reference.
import os, re, statistics, time

import cases
from cases import case, plat, stats, ui_items
import cases_caseagent as AG
import lat
import render_hops

KEY_FRAME_RE = re.compile(r"input_order hop=key_frame layout=\S+ t=(\d+(?:\.\d+)?)")
KEYDOWN_ANY_RE = re.compile(r'input_order keydown key=("(?:[^"\\]|\\.)*"|\S+) ctrl=(true|false) alt=(true|false) shift=(true|false) cmd=(true|false) t=(\d+(?:\.\d+)?)')
ENTER_BEGIN_RE = re.compile(r"input_order hop=enter_begin\b.* t=(\d+(?:\.\d+)?)\s*$")
MODIFIERS = {"shift", "control", "ctrl", "alt", "option", "meta", "super", "command", "cmd", "capslock", "fn"}
ENTER_KEYS = {"enter", "return"}
STREAM_ALIVE_S = 11.0          # fake_model streams for STREAM_CHUNKS * STREAM_GAP_S = 12 s; a request older than this has (almost) finished


def _keydowns(txt):
    """[(key, plain, t_ms)] of every keydown line; plain = no ctrl/alt/cmd and not a bare modifier"""
    out = []
    for m in KEYDOWN_ANY_RE.finditer(txt):
        key, ctrl, alt, _sh, cmd, t = m.groups()
        k = key[1:-1] if len(key) >= 3 and key[0] == key[-1] == '"' else key
        out.append((k.lower(), not (ctrl == "true" or alt == "true" or cmd == "true" or k.lower() in MODIFIERS), float(t)))
    return out


def key_frame_deltas(txt, t_min=None, t_max=None):
    """pure: ms from every plain, non-Enter keydown to the first `hop=key_frame` line after it. Keys that pile up before one frame all pair with it. Keydowns outside
    [t_min, t_max] (epoch ms) are not sampled (used to keep only the keys typed while a reply streamed)."""
    events = []
    for l in txt.splitlines():
        m = KEYDOWN_ANY_RE.search(l)
        if m:
            key = m.group(1)
            k = (key[1:-1] if len(key) >= 3 and key[0] == key[-1] == '"' else key).lower()
            plain = not (m.group(2) == "true" or m.group(3) == "true" or m.group(5) == "true" or k in MODIFIERS or k in ENTER_KEYS)
            t = float(m.group(6))
            if plain and (t_min is None or t >= t_min) and (t_max is None or t <= t_max):
                events.append(("k", t))
            else:
                events.append(("x", t))         # a key that is not sampled still takes the frame it owes (its own frame is not the next key's)
            continue
        m = KEY_FRAME_RE.search(l)
        if m:
            events.append(("f", float(m.group(1))))
    out, pending = [], []
    for kind, t in events:
        if kind == "k":
            pending.append(t)
        elif kind == "x":
            pending = []        # an unsampled key-down re-arms the app's frame debt: earlier keys' frame is the one before it (they were paired already or are lost)
        else:
            out.extend(t - p for p in pending if t >= p)
            pending = []
    return out


FRAME_DRAWN_RE = re.compile(r"input_order hop=frame_drawn total_ms=(\d+(?:\.\d+)?) acquire_ms=\S+(?: present_ms=\S+)? t=(\d+(?:\.\d+)?)")


def paced_key_frame_deltas(txt, t_min, t_max):
    """pure: like key_frame_deltas, minus the compositor's frame pacing. While a reply streams the app builds a frame every refresh interval, and a key typed just
    after one is built waits for the next slot (previous frame start + the median frame spacing) on ANY app: that wait is not Phosphor's. -> ms from
    max(key-down, slot) to the key's frame, so the idle (no frame in flight) and streaming numbers mean the same thing. With too few frames to know the spacing
    (< 5 gaps) the pacing is unknown and the raw delta is returned."""
    frames = [(float(m.group(2)) - float(m.group(1)), float(m.group(2))) for m in FRAME_DRAWN_RE.finditer(txt)]
    ts = [t for _, t in frames if (t_min is None or t >= t_min) and (t_max is None or t <= t_max)]
    gaps = [b - a for a, b in zip(ts, ts[1:]) if b - a < 40.0]
    interval = statistics.median(gaps) if len(gaps) >= 5 else None
    raw = key_frame_deltas(txt, t_min, t_max)
    if interval is None:
        return raw
    out, last, kd = [], None, []
    for l in txt.splitlines():
        m = FRAME_DRAWN_RE.search(l)
        if m:
            last = float(m.group(2)) - float(m.group(1))        # start of the last frame built
            continue
        m = KEYDOWN_ANY_RE.search(l)
        if m:
            key = m.group(1)
            k = (key[1:-1] if len(key) >= 3 and key[0] == key[-1] == '"' else key).lower()
            plain = not (m.group(2) == "true" or m.group(3) == "true" or m.group(5) == "true" or k in MODIFIERS or k in ENTER_KEYS)
            t = float(m.group(6))
            if plain and (t_min is None or t >= t_min) and (t_max is None or t <= t_max):
                kd.append((t, last))
            else:
                kd = []
            continue
        m = KEY_FRAME_RE.search(l)
        if m:
            tf = float(m.group(1))
            for t, ls in kd:
                slot = max(t, ls + interval) if ls is not None else t
                if tf >= t:
                    out.append(tf - min(slot, tf))
            kd = []
    return out if len(out) == len(raw) else raw


CORE_ENTER_WINDOW_RE = re.compile(r"input_order hop=core_enter stage=handle_window_event\b.* t=(\d+(?:\.\d+)?)\s*$")
CORE_ENTER_KEYSTROKE_RE = re.compile(r"input_order hop=core_enter stage=dispatch_keystroke\b.* ctrl=(true|false) alt=(true|false) shift=(true|false) cmd=(true|false)")


def enter_shares(txt):
    """pure: ms from every plain Enter to the next `hop=enter_begin` line (the app's own share of Enter before any routing). The Enter's start is the window's
    dispatch (`hop=core_enter stage=handle_window_event`, every platform: a key an editor binding takes, which Enter is on Linux and Windows, logs NO
    `input_order keydown` line, round-6 finding), or the `keydown` line when a platform logs that first. Counted once either way."""
    out, pending, core_pending = [], None, False
    for l in txt.splitlines():
        m = CORE_ENTER_WINDOW_RE.search(l)
        if m:
            pending, core_pending = float(m.group(1)), True
            continue
        m = CORE_ENTER_KEYSTROKE_RE.search(l)
        if m:
            if m.group(1) == "true" or m.group(2) == "true" or m.group(4) == "true":
                pending = None          # a modified Enter (Ctrl-Enter, Alt-Enter) is not the plain submit
            continue
        m = KEYDOWN_ANY_RE.search(l)
        if m:
            k = (m.group(1)[1:-1] if len(m.group(1)) >= 3 and m.group(1)[0] == m.group(1)[-1] == '"' else m.group(1)).lower()
            if k in ENTER_KEYS:
                if core_pending:
                    core_pending = False        # the same physical Enter, already anchored at its window dispatch
                elif m.group(2) == "false" and m.group(3) == "false" and m.group(5) == "false":
                    pending = float(m.group(6))
            continue
        m = ENTER_BEGIN_RE.search(l)
        if m and pending is not None:
            out.append(float(m.group(1)) - pending)
            pending, core_pending = None, False
    return out


def clean(messages):
    """the request's messages as the prompt sees them: the app's <environment_context> user message (appended after the prompt) removed (fake_model.clean_messages)"""
    import fake_model
    return fake_model.clean_messages(messages)


def last_user(messages):
    us = [m.get("content") or "" for m in clean(messages) if m.get("role") == "user"]
    return us[-1] if us else None


def steer_verdict(reqs, steer):
    """pure: the fake model's requests made after the steer's Enter -> failure text or None. The steer must reach the model as ONE new user turn: exactly one
    request whose LAST user message is the steer, that message equal to the steer byte for byte, and the steer text occurring exactly once in that request."""
    mine = [q for q in reqs if last_user(q["messages"]) == steer]
    if not mine:
        near = [last_user(q["messages"]) for q in reqs if last_user(q["messages"]) is not None]
        has = [q for q in reqs if any(steer in (m.get("content") or "") for m in clean(q["messages"]))]
        if has:
            got = last_user(has[0]["messages"])
            k = next((i for i, (a, b) in enumerate(zip(got, steer)) if a != b), min(len(got), len(steer)))
            return "the steer reached the model but not as a user turn equal to what was typed (last user message %r; first difference at char %d)" % (got[:120], k)
        return "the model never received the steer (%d request(s) after the Enter; last user messages %r)" % (len(reqs), [x[:60] for x in near[-2:]])
    if len(mine) != 1:
        return "the steer reached the model as %d separate requests, expected exactly once" % len(mine)
    n = sum((m.get("content") or "").count(steer) for m in clean(mine[0]["messages"]))
    if n != 1:
        return "the steer text occurs %d times in the request that carried it, expected exactly once" % n
    return None


def steers_in_order(final_messages, steers):
    """pure: failure text or None. In the last request each steer is present exactly once and in the order they were typed (nothing lost, duplicated or reordered)"""
    texts = [m.get("content") or "" for m in clean(final_messages) if m.get("role") == "user"]
    pos = []
    for s in steers:
        idx = [i for i, t in enumerate(texts) if t == s]
        if len(idx) != 1:
            return "a steer appears %d times in the last request's user turns (expected once): %r" % (len(idx), s[:60])
        pos.append(idx[0])
    return None if pos == sorted(pos) else "the steers reached the model out of order: positions %r" % pos


def make_steer(tok, i):
    """a 200+ character steer with shell-looking text (a pipe, $HOME, &&, a redirect) that must go to the agent, never the shell"""
    s = ("steer %d %s: while you keep working, only note this: ls -la | grep x and echo $HOME && cat notes.txt > out_%s.txt are text for the plan, "
         "not commands to run; do not touch the rm -rf of any real directory, just acknowledge." % (i, tok, tok))
    return s + " pad" * max(0, (205 - len(s)) // 4 + 1)


def need_model(ctx, r):
    import fake_model
    if os.environ.get("APPCI_FAKE_MODEL_ACTIVE") != "1" or fake_model.FakeModel.ACTIVE is None:
        r.skip("the scripted fake model is not running (APPCI_FAKE_MODEL=1 starts fake_model.py as the agent provider); these cases need its request log and its slow stream")
    AG.need_provider(ctx, r)
    if ctx.log_view_mode() is None:
        r.skip("the app log has no hop=view_mode line (a build older than #1568): the view cannot be proven")
    return fake_model.FakeModel.ACTIVE


def wait_requests(ctx, fm, n_before, timeout=30):
    return ctx.until(lambda: len(fm.reqs) > n_before, timeout, step=0.02)


def stream_age_s(fm):
    return time.time() - fm.reqs[-1]["t"] if fm.reqs else 1e9


def start_stream(ctx, r, fm, tok):
    """a fresh conversation whose first prompt carries STREAMLONG: every reply of it streams for ~12 s. -> True once the request arrived"""
    AG.fresh_conversation(ctx, r)
    n0 = len(fm.reqs)
    AG.type_fast(ctx, "STREAMLONG keep talking %s" % tok)
    ok = wait_requests(ctx, fm, n0, 30)
    time.sleep(0.4)
    return ok


def to_terminal(ctx, r):
    ctx.reset()
    ctx.be.key("Escape")
    if not ctx.until(lambda: ctx.log_view_mode() == "terminal", 6):
        r.skip("Escape did not leave the agent view (hop=view_mode says %s): the terminal-mode reference cannot be measured" % ctx.log_view_mode())
    time.sleep(0.5)


def to_agent(ctx, r):
    chord = "cmd+Return" if ctx.be.name == "mac" else "ctrl+shift+Return"
    for _ in range(2):
        if ctx.log_view_mode() == "agent":
            break
        ctx.be.key(chord)
        ctx.until(lambda: ctx.log_view_mode() == "agent", 6)
    if ctx.log_view_mode() != "agent":
        r.check(False, "could not return to the agent view (hop=view_mode says %s): later cases would run in the wrong mode" % ctx.log_view_mode())
    time.sleep(0.5)


def type_keys(ctx, count, gap=(0.08, 0.18)):
    """`count` single 'm' presses with a human-ish gap, clearing the line every 10 keys (a growing line would change what a frame costs)"""
    for i in range(count):
        ctx.be.type("m", 10)
        time.sleep(ctx.rng.uniform(*gap))
        if i % 10 == 9:
            ctx.be.key("ctrl+u")
            time.sleep(0.1)


PACED = []          # slot-adjusted streaming deltas of the current case (metric only, #1843)


def key_batch(ctx, count, fm=None, gap=(0.08, 0.18)):
    """-> (deltas ms, log text of the window). With `fm`: only keys typed while that reply's stream was running count."""
    off = ctx.log_size()
    t_req = fm.reqs[-1]["t"] * 1000.0 if fm else None
    type_keys(ctx, count, gap)
    time.sleep(0.6)
    txt = ctx.log_since(off)
    lo, hi = (t_req + 300.0, t_req + STREAM_ALIVE_S * 1000.0) if fm else (None, None)
    # The VERDICT uses the raw key -> frame delta: a key that waits for the next streaming frame slot is a
    # wait the user sees (#1843; owner rule: never dismiss bad numbers). The slot-adjusted delta is kept as
    # a metric only (PACED below), to show how much of the overhead is that wait.
    if fm:
        PACED.extend(paced_key_frame_deltas(txt, lo, hi))
    return key_frame_deltas(txt, lo, hi), txt


def judge_ui_items(r, txts, n_keys, label):
    """the UI-work rule of latency_key_log: event/action main-thread items >= 4 ms while typing; more than max(2, 5 % of the keys) FAIL, frame items WARN"""
    items = [it for t in txts for it in ui_items(t)]
    logic = [ms for _, ms, k in items if k != "frame"]
    frames = [ms for _, ms, k in items if k == "frame"]
    allowed = max(2, int(0.05 * n_keys))
    r.metric("ui_items_%s" % label, dict(logic_count=len(logic), logic_max_ms=max(logic, default=0.0), frame_count=len(frames), frame_max_ms=max(frames, default=0.0), allowed=allowed))
    r.check(len(logic) <= allowed, "UI work (%s): %d event/action item(s) >= 4 ms during %d typed keys (allowed %d, worst %.1f ms)" % (label, len(logic), n_keys, allowed, max(logic, default=0.0)))
    if frames:
        r.note("UI work (%s): %d frame item(s) >= 4 ms (max %.1f ms); frame items include rasterisation: WARN on software GL, not a failure" % (label, len(frames), max(frames)))


def evaluate(ctx, r, name, ds, rs, budget, expected, notes):
    rec = lat.evaluate("%s/%s" % (ctx.label, name), ds, rs, budget, "overhead", plat(), ref_name="same app, terminal-mode input editor" if rs else None,
                       expected=expected, mode="sequential", notes=notes)
    r.metrics.setdefault("lat", {})[name.split("/", 1)[1]] = rec
    cases.lat_checks(r, rec, name)
    return rec


# ---------------------------------------------------------------- key -> frame in the agent input
@case("agent_latency_key", modes=("agent",), order=52, gui=True)
def c_agent_latency_key(ctx, r):
    """#1828: key -> first frame built in the AGENT input (a) idle and (b) while a slow steady fake reply streams, each judged as overhead over the terminal-mode
    input editor's key -> frame in the same session (budget APPCI_KEY_BUDGET_MS, 2 ms; lat.evaluate decides at 2 SE, thin evidence is never a pass)."""
    fm = need_model(ctx, r)
    n, warm = ctx.count_lat(60), lat.warmup()
    budget = float(os.environ.get("APPCI_KEY_BUDGET_MS", "2"))
    ctx.reset()
    AG.fresh_conversation(ctx, r)
    probe, _ = key_batch(ctx, 4)
    if not probe:
        r.skip("no `hop=key_frame` line followed the probe keys in the app log: a build without the #1828 hook, or input-order logging is off")
    try:
        # terminal-mode reference first half, then agent idle, agent streaming, reference second half (drift shows in lat.evaluate's block_drift)
        ref = []
        to_terminal(ctx, r)
        ds, txt_t1 = key_batch(ctx, (n + warm) // 2)
        ref += ds[warm:]
        to_agent(ctx, r)
        ctx.reset()
        AG.fresh_conversation(ctx, r)
        idle, txt_idle = key_batch(ctx, n + warm)
        idle = idle[warm:]
        stream, txts_stream, kept_streams = [], [], 0
        per, got = 30, 0
        PACED.clear()
        t_all = time.time()
        while got < n + warm and time.time() - t_all < 300:
            if not start_stream(ctx, r, fm, ctx.tok(5)):
                r.check(False, "no model request within 30 s of the STREAMLONG prompt: the streaming phase cannot run")
                break
            kept_streams += 1
            d, tx = key_batch(ctx, per, fm, gap=(0.05, 0.12))
            stream += d
            txts_stream.append(tx)
            got += len(d)
            ctx.reset()                     # Ctrl-C stops the running reply (#851)
            time.sleep(0.5)
        stream = stream[warm:]
        to_terminal(ctx, r)
        ds, txt_t2 = key_batch(ctx, (n + warm) - (n + warm) // 2)
        ref += ds[warm:]
    finally:
        ctx.reset()
        to_agent(ctx, r)
    r.metric("key_to_frame_agent_idle_ms", stats(idle))
    r.metric("key_to_frame_agent_streaming_ms", stats(stream))
    r.metric("key_to_frame_terminal_editor_ms", stats(ref))
    r.metric("stream_runs", kept_streams)
    r.metric("key_to_frame_agent_streaming_slot_adjusted_ms", stats(PACED[warm:]))      # metric only, not judged (#1843)
    r.check(len(idle) >= n * 0.8, "only %d/%d agent-idle key->frame samples" % (len(idle), n))
    r.check(len(stream) >= n * 0.5, "only %d/%d key->frame samples while a reply streamed (a stream ended before the keys, or the keys never reached the agent input)" % (len(stream), n))
    r.check(len(ref) >= n * 0.5, "only %d/%d terminal-editor reference samples" % (len(ref), n))
    note = ["reference = the same key->frame measure typed into the terminal-mode input editor of this session (before and after the agent phases); both are the app's own "
            "keydown -> first frame built, so the shared vsync wait cancels in the overhead"]
    if idle and ref:
        evaluate(ctx, r, "agent_latency_key/idle", idle, ref, budget, n, note)
    if stream and ref:
        evaluate(ctx, r, "agent_latency_key/streaming", stream, ref, budget, n, note + ["keys typed while the fake model's reply streamed (12 s slow steady stream, restarted per chunk of keys)"])
    if not (idle and ref):
        r.unverifiable("agent-idle or terminal-editor key->frame samples are missing: the overhead budget was NOT judged")
    judge_ui_items(r, [txt_idle], n + warm, "agent_idle")
    judge_ui_items(r, txts_stream, max(got, 1), "agent_streaming")
    # render-latency step A: the present / acquire split (Linux, Windows) and the macOS hops of the judged default run; metrics only
    render_hops.record(r, "\n".join(txts_stream), prefix="agent_streaming_")
    render_hops.record(r, txt_idle, prefix="agent_idle_")
    present_mode_ab(ctx, r, fm, n)


class ArmR:
    """the Result as the A/B arms see it: a failed check or skip is a WARN note, never a verdict (the default run above is the judged one)"""
    class Failed(Exception):
        pass

    def __init__(self, r):
        self.r = r

    def check(self, ok, msg=None, *a, **k):
        if not ok:
            self.r.note("WARN present-mode A/B: %s" % msg)
        return ok

    def skip(self, msg):
        raise ArmR.Failed(msg)

    def __getattr__(self, name):
        return getattr(self.r, name)


def stream_keys(ctx, r, fm, need, per=30):
    """keys typed while a fake reply streams until `need` samples -> (raw ms, slot-adjusted ms, log text); restarts the stream per `per` keys"""
    raw, paced, txts, t_all = [], [], [], time.time()
    PACED.clear()
    while len(raw) < need and time.time() - t_all < 240:
        if not start_stream(ctx, r, fm, ctx.tok(5)):
            break
        d, tx = key_batch(ctx, per, fm, gap=(0.05, 0.12))
        raw += d
        txts.append(tx)
        ctx.reset()
        time.sleep(0.5)
    return raw, list(PACED), "\n".join(txts)


def present_mode_ab(ctx, r, fm, n):
    """design option F (docs/design/render-latency.md), zero code: the streaming key -> frame of this case again with PHOSPHOR_PRESENT_MODE=mailbox and
    PHOSPHOR_FRAME_LATENCY=1, relaunching per block (render_hops.arm_schedule, like cases_ctrlc_ab) next to a relaunched default arm. Linux only (the launcher
    that takes App.extra_env; macOS and Windows have no such present-mode switch). METRICS and a note only: the judged run is the default one above."""
    if ctx.win or ctx.be.name == "mac" or not hasattr(ctx.app, "extra_env"):
        return
    if os.environ.get("APPCI_RENDER_AB", "1") == "0":
        r.note("present-mode A/B skipped (APPCI_RENDER_AB=0)")
        return
    from cases_robust import start_app
    arm_n, lead = ctx.count_lat(30), lat.interleave_lead()
    got = {a: dict(raw=[], paced=[], txt=[]) for a in render_hops.ARMS}
    quiet = ArmR(r)
    try:
        for arm, cnt, disc in render_hops.arm_schedule(render_hops.ARMS, arm_n, 2, lead):
            try:
                ctx.app.extra_env = render_hops.arm_env(arm)
                _p, up = start_app(ctx, quiet, "present-mode A/B arm %s" % arm)
                if not up:
                    continue
                time.sleep(cases.LAT_SETTLE_S)
                ctx.reset()
                AG.fresh_conversation(ctx, quiet)
                raw, paced, txt = stream_keys(ctx, quiet, fm, cnt)
                got[arm]["raw"] += raw[disc:]
                got[arm]["paced"] += paced[disc:]
                got[arm]["txt"].append(txt)
            except ArmR.Failed as e:
                r.note("WARN present-mode A/B: arm %s block skipped: %s" % (arm, str(e)[:160]))
    finally:
        ctx.app.extra_env = {}
        _p, back = start_app(ctx, r, "present-mode A/B: relaunch with the default environment")
        ctx.reset()
        if back:
            to_agent(ctx, r)
    arms = {a: render_hops.arm_summary(v["raw"], v["paced"], "\n".join(v["txt"])) for a, v in got.items()}
    for a, v in arms.items():
        r.metric("present_mode_ab_%s" % a, v)
    deltas = render_hops.arm_deltas(arms)
    r.metric("present_mode_ab_deltas", deltas)
    for a, d in deltas.items():
        r.note("present-mode A/B %s vs default (streaming key->frame p50, negative = faster): raw %s ms, slot-adjusted %s ms; present_ms p50 %s vs default %s (n %s / %s)" % (
            a, d["raw_p50_delta"], d["adjusted_p50_delta"], (arms[a]["present_ms"] or {}).get("p50"), (arms["default"]["present_ms"] or {}).get("p50"),
            (arms[a]["streaming"] or {}).get("n"), (arms["default"]["streaming"] or {}).get("n")))


# ---------------------------------------------------------------- Enter in the agent input
def enter_batch(ctx, r, count, line_fn, fm=None, streaming=False, settle=0.4):
    """type `count` lines with Enter in the same call. -> (shares ms Enter->enter_begin, Enter->request ms (agent), log text). With streaming, each line is typed
    while the previous line's reply is still streaming (a steer); the conversation is restarted every 20 lines so its context stays small."""
    shares, e2e, txts = [], [], []
    for i in range(count):
        if streaming and i % 20 == 0:
            if i:
                ctx.reset()
            if not start_stream(ctx, r, fm, ctx.tok(5)):
                break
        off = ctx.log_size()
        n0 = len(fm.reqs) if fm else 0
        ctx.be.type(line_fn(i), ctx.rng.randint(10, 25), enter=True)
        if fm:
            wait_requests(ctx, fm, n0, 20)
        else:
            time.sleep(0.5)
        time.sleep(settle)
        txt = ctx.log_since(off)
        txts.append(txt)
        sh = enter_shares(txt)
        shares += sh[-1:]
        if fm and len(fm.reqs) > n0 and sh:
            ents = [t for k, plain, t in _keydowns(txt) if k in ENTER_KEYS]
            if ents:
                e2e.append(fm.reqs[n0]["t"] * 1000.0 - ents[-1])
        if not streaming and not fm:
            time.sleep(0.3)
    return shares, e2e, txts


@case("agent_latency_enter", modes=("agent",), order=53, gui=True)
def c_agent_latency_enter(ctx, r):
    """#1828: the app's own share of Enter (Enter key-down -> hop=enter_begin) for a prose line typed into the agent input at 10-25 ms/key with instant Enter,
    (a) idle and (b) while a slow steady fake reply streams, judged as overhead over the same share for a shell line in the terminal-mode editor (budget
    APPCI_ENTER_BUDGET_MS, 2 ms). Enter -> the request arriving at the fake model is a metric (loopback; includes the app's request build)."""
    fm = need_model(ctx, r)
    n, warm = ctx.count_lat(60), lat.warmup()
    budget = float(os.environ.get("APPCI_ENTER_BUDGET_MS", "2"))
    ctx.reset()
    try:
        AG.fresh_conversation(ctx, r)
        # prose, never a command head: `ping <tok>` started with an installed command, so autodetect ran it in the shell
        # (owner rule: installed-head lines run in the shell) and the idle samples measured shell Enters, not agent ones (round 7)
        idle, e2e_idle, _ = enter_batch(ctx, r, n + warm, lambda i: "please note the word %s" % ctx.tok(6), fm)
        idle = idle[warm:]
        stream, e2e_stream, _ = enter_batch(ctx, r, n + warm, lambda i: "and also %s" % ctx.tok(6), fm, streaming=True)
        stream = stream[warm:]
        ctx.reset()
        to_terminal(ctx, r)
        ref, _, _ = enter_batch(ctx, r, n + warm, lambda i: "true # %s" % ctx.tok(5))
        ref = ref[warm:]
    finally:
        ctx.reset()
        to_agent(ctx, r)
    r.metric("enter_share_agent_idle_ms", stats(idle))
    r.metric("enter_share_agent_streaming_ms", stats(stream))
    r.metric("enter_share_terminal_editor_ms", stats(ref))
    r.metric("enter_to_request_idle_ms", stats(e2e_idle))
    r.metric("enter_to_request_streaming_ms", stats(e2e_stream))
    if not (idle or stream or ref):
        r.skip("no Enter keydown -> hop=enter_begin pair in the app log (input-order logging off, or the Enters never reached the editor)")
    r.check(len(idle) >= n * 0.8, "only %d/%d agent-idle Enter samples" % (len(idle), n))
    r.check(len(stream) >= n * 0.5, "only %d/%d Enter samples while a reply streamed" % (len(stream), n))
    r.check(len(ref) >= n * 0.8, "only %d/%d terminal-editor reference Enter samples" % (len(ref), n))
    note = ["share = Enter key-down -> `hop=enter_begin` (the app's own time before routing); reference = a shell line in the terminal-mode editor of the same session"]
    if idle and ref:
        evaluate(ctx, r, "agent_latency_enter/idle", idle, ref, budget, n, note)
    if stream and ref:
        evaluate(ctx, r, "agent_latency_enter/streaming", stream, ref, budget, n, note + ["Enter typed while the previous reply streamed (a steer)"])
    if not (idle and ref):
        r.unverifiable("agent-idle or terminal-editor Enter samples are missing: the overhead budget was NOT judged")
    for tag, xs in (("idle", e2e_idle), ("streaming", e2e_stream)):
        if xs:
            r.check(statistics.median(xs) <= 500, "Enter -> request at the model (%s) median %.0f ms > 500 ms sanity bound (loopback fake model)" % (tag, statistics.median(xs)))


# ---------------------------------------------------------------- the steer (#851)
@case("agent_steer_fast", modes=("agent",), order=54, gui=True)
def c_agent_steer_fast(ctx, r):
    """#851 (by design): while a reply streams, a 200+ character line with shell-looking text (`ls -la | grep x`, `$HOME`, `&&`, a redirect) typed at 10-25 ms/key
    with instant Enter goes to the AGENT as a steer. Oracles: the fake model's next request carries the steer as one user turn, byte for byte, exactly once
    (steer_verdict); after three steers in a row the last request holds all three once each in order (steers_in_order); the app log's hop=enter_begin buffer
    is the steer routed AI; no `input_order pty_write` line and no shell command from the first steer on."""
    fm = need_model(ctx, r)
    ctx.reset()
    AG.fresh_conversation(ctx, r)
    tok = ctx.tok(6)
    n0 = len(fm.reqs)
    AG.type_fast(ctx, "STREAMLONG keep talking %s" % tok)
    if not wait_requests(ctx, fm, n0, 30):
        r.check(False, "no model request within 30 s of the STREAMLONG prompt")
        return
    time.sleep(0.4)
    off_all, mark = ctx.log_size(), ctx.cmd_mark()
    steers, t_type = [], []
    for i in range(3):
        steer = make_steer(ctx.tok(5), i + 1)
        if stream_age_s(fm) > 4.0:
            r.unverifiable("steer %d: the reply had been streaming for %.1f s before the keys started (> 4 s), so the 200-character line would not finish typing inside its 12 s: precondition not provable" % (i + 1, stream_age_s(fm)))
            break
        k, off = len(fm.reqs), ctx.log_size()
        t0 = time.time()
        ctx.be.type(steer, ctx.rng.randint(10, 25), enter=True)
        t_enter = time.time()
        t_type.append(round(t_enter - t0, 2))
        age = t_enter - fm.reqs[k - 1]["t"]
        if age > STREAM_ALIVE_S:
            r.unverifiable("steer %d: the reply stream (%.1f s old) had ended before the Enter, so this steer was not typed while a reply ran" % (i + 1, age))
            break
        steers.append(steer)
        got = wait_requests(ctx, fm, k, 20)
        r.check(got, "steer %d: no model request within 20 s of the Enter (steer lost, or held in the input)" % (i + 1))
        time.sleep(0.6)
        err = steer_verdict(fm.reqs[k:], steer)
        if got:
            r.check(err is None, "steer %d: %s" % (i + 1, err))
        rows = AG.enter_buffers(ctx.log_since(off))
        e2 = AG.judge_agent_input(steer, rows)
        r.check(e2 is None, "steer %d: %s" % (i + 1, e2))
        sh = enter_shares(ctx.log_since(off))
        if sh:
            r.metric("steer_%d_enter_share_ms" % (i + 1), sh[-1])
        time.sleep(0.8)         # the next steer starts while the reply to this one is still streaming (it restarted at the request)
    r.metric("steer_type_seconds", t_type)
    r.metric("steer_lengths", [len(s) for s in steers])
    if len(steers) == 3 and fm.reqs:
        err = steers_in_order(fm.reqs[-1]["messages"], steers)
        r.check(err is None, "after three steers: %s" % err)
    for s in steers:
        r.check(len(s) >= 200, "the steer line is %d chars, the case needs 200+" % len(s))
    window = ctx.log_since(off_all)
    pw = [l for l in window.splitlines() if "input_order pty_write " in l]
    r.metric("steer_pty_writes", len(pw))
    r.check(not pw, "%d `input_order pty_write` line(s) during the steers, nothing may reach the pty: %s" % (len(pw), "; ".join(l.strip()[:90] for l in pw[:3])))
    ran = ctx.cmds_since(mark)
    r.check(not ran, "the shell received %d command(s) during the steers (shell-looking steer text must stay in the agent): %r" % (len(ran), ran[:3]))
    ctx.shot("agent_steer_fast")
    ctx.reset()
    r.check(ctx.log_view_mode() == "agent", "the view left agent mode during the case (hop=view_mode says %s)" % ctx.log_view_mode())
