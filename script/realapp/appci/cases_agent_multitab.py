# appci/cases_agent_multitab.py (#1833): several agents at once across tabs. One case, `agent_multitab_streaming` (agent pass).
#   * tab 1 is the TYPING tab; tabs 2..N+1 (APPCI_MT_TABS, default 6) each start a long scripted reply from the fake model (fake_model.py "Please stream long
#     reply <TAG> chunks <N> gap <MS>": a chunk every gap ms for APPCI_MT_STREAM_S seconds, default 75), so every other tab is hidden while it streams
#     (the #805 situation: hidden-tab output must not cost the visible tab).
#   * The typing tab is exercised twice, WITHOUT streams (baseline) and WITH the streams running: (a) agent input: prose at 10-25 ms/key, instant Enter,
#     the exact text at Enter from `hop=enter_begin` (input_type AI) and main-thread `hop=ui_item` items; (b) terminal view: key -> first frame after the pty
#     echo (typing into a running `cat`, the pairs of latency_key_log) and a burst of append-commands whose effect file AND received command lines must be exact.
#     The latency verdict is the LOAD median against the BASELINE median of the same run (overhead <= APPCI_MT_BUDGET_MS, 2 ms, or twice the bootstrap SE of
#     the difference when that is larger: a 2 ms budget cannot be judged by a point estimate).
#   * Every stream must finish on the wire (fake.streams: all chunks written, the connection never dropped), and a follow-up typed in each streaming tab
#     ("Please follow up check <TAG>") makes the app send that tab's conversation back: the fake model checks it holds the tab's own chunks 1..N and END, once
#     each, and no marker of another tab, nor the typing tab's prose (cross-tab bleed / loss). No screenshots are needed for any verdict.
#   * #805 hops: frames per second (warpui::frame_cause lines) over an idle window with the streams running vs without: a metric and a note, not a verdict
#     (no fixed budget exists; the fix for #805 is judged by the key->frame numbers above).
# Linux and macOS (the tab keys follow the platform; the pty-echo frame log exists on both). Windows SKIPS: the PowerShell pane has no rc hook for the exact
# received-command proof and no verified pty-echo frame pairing. A precondition that cannot be proven (no fake model, no log, no hop=enter_begin) SKIPS.
import os, re, statistics, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases as C  # noqa: E402
import cases_caseagent as CA  # noqa: E402
import cases_robust as R  # noqa: E402
import cases_ui as U  # noqa: E402
import lat  # noqa: E402
from cases import case  # noqa: E402

FRAME_CAUSE_RE = re.compile(r"\bframe [123]: updated=\[")
BUDGET_MS = float(os.environ.get("APPCI_MT_BUDGET_MS", "2"))


# ---------------------------------------------------------------- pure helpers (unit-tested in selftest.py)
def compare_latency(base, load, budget_ms=BUDGET_MS):
    """(ok, message): the LOAD key->frame samples against the BASELINE samples (ms). Fails when the median grew by more than max(budget, 2 x the bootstrap
    standard error of the difference); fewer than 8 samples on either side cannot be judged (ok=None)."""
    if len(base) < 8 or len(load) < 8:
        return None, "not enough samples (%d baseline, %d under load) to judge" % (len(base), len(load))
    b, l = statistics.median(base), statistics.median(load)
    se = lat.median_diff_se(load, base) or 0.0
    limit = max(budget_ms, 2.0 * se)
    d = l - b
    return d <= limit, "median %.2f ms under load vs %.2f ms baseline: +%.2f ms (limit %.2f = max(budget %.1f, 2 x SE %.2f))" % (l, b, d, limit, budget_ms, se)


def streams_problems(streams, tags, total):
    """what is wrong with the wire side of the replies: a stream the fake never saw, one cut off by the app (broken), or not fully written"""
    out = []
    for t in tags:
        s = streams.get(t)
        if not s:
            out.append("%s: the model never received this tab's request" % t)
        elif s["broken"]:
            out.append("%s: the app dropped the connection after %d/%d chunks" % (t, s["sent"], s["total"]))
        elif not s["finished"] or s["sent"] != total:
            out.append("%s: only %d/%d chunks were written" % (t, s["sent"], total))
    return out


def bleed_problems(followups, tags, typed_tokens):
    """-> (problems, unproven): per tag the fake's history_check verdict; a typed-token of the typing tab appearing in a streaming tab's conversation is bleed.
    'empty' (the app sent no assistant text back) is not provable either way and goes to `unproven`."""
    bad, unproven = [], []
    for t in tags:
        f = followups.get(t)
        if not f:
            unproven.append("%s: no follow-up reached the model" % t)
            continue
        if f["verdict"] == "empty":
            unproven.append("%s: the app sent no assistant text back, so the reply content cannot be checked" % t)
        elif f["verdict"] != "ok":
            bad.append("%s: %s" % (t, f["verdict"]))
        for tok in typed_tokens:
            if tok in f.get("user_text", ""):
                bad.append("%s: text typed in the typing tab (%s) is in this tab's conversation" % (t, tok))
    return bad, unproven


def frames_per_s(text, secs):
    """frame_cause lines per second over a window of `secs` (the #805 measure: how many frames the app builds while only hidden tabs change)"""
    return len(FRAME_CAUSE_RE.findall(text)) / secs if secs > 0 else 0.0


# ---------------------------------------------------------------- phases in the typing tab
def agent_phase(ctx, r, fake, label, tokens, n=3):
    """prose typed into the agent input of the typing tab: exact at Enter, and the main-thread items while typing -> (logic item count, worst ms)"""
    off = ctx.log_size()
    for i in range(n):
        tok = ctx.tok(6).lower()
        tokens.append(tok)
        sent = "please check item %d of build %s" % (i + 1, tok)
        o2, nreq = ctx.log_size(), fake.requests
        CA.type_fast(ctx, sent)
        ctx.until(lambda: CA.enter_buffers(ctx.log_since(o2)), 6)
        err = CA.judge_agent_input(sent, CA.enter_buffers(ctx.log_since(o2)))
        r.check(err is None, "%s agent input #%d: %s" % (label, i + 1, err))
        ctx.until(lambda: fake.requests > nreq, 20)
        time.sleep(1.0)
    items = C.ui_items(ctx.log_since(off))
    logic = [ms for _, ms, k in items if k != "frame"]
    r.metric("%s_agent_ui_items" % label, dict(logic_count=len(logic), logic_max_ms=max(logic, default=0.0), frame_count=len(items) - len(logic)))
    return len(logic), max(logic, default=0.0)


def terminal_phase(ctx, r, label, n_keys):
    """terminal view of the typing tab: key -> frame samples (typing into a running cat) and an exact append-command burst -> samples (ms)"""
    ctx.be.key("Escape")
    time.sleep(0.8)
    ctx.be.type("cat > /dev/null", 10, enter=True)
    time.sleep(1.0)
    off = ctx.log_size()
    for _ in range(n_keys):
        ctx.be.type("m", 10)
        time.sleep(ctx.rng.uniform(0.08, 0.18))
    time.sleep(1.0)
    ds, cnt = C.key_frame_pairs(ctx.log_since(off))
    ctx.reset()                                       # Ctrl-C ends the cat (this tab streams nothing)
    time.sleep(0.5)
    r.metric("%s_key_to_frame_ms" % label, C.stats(ds))
    U.exact_burst(ctx, r, "mt_%s" % label, 12)
    return ds[lat.warmup():], cnt


def back_to_agent(ctx, r):
    ctx.be.type("/agent", 12, enter=True)
    if not ctx.until(lambda: ctx.log_view_mode() == "agent", 10):
        r.skip("the typing tab did not return to the agent view with /agent (hop=view_mode %s)" % ctx.log_view_mode())
    time.sleep(1.0)


@case("agent_multitab_streaming", modes=("agent",), order=97, gui=True)
def c_agent_multitab_streaming(ctx, r):
    """#1833: N tabs each streaming a long scripted reply (hidden while another tab is shown), typing in the shown tab (agent input AND terminal view)
    stays exact and within the key->frame budget of the same run's no-stream baseline, and every reply completes in its own tab with its own content."""
    import fake_model
    if os.environ.get("APPCI_FAKE_MODEL_ACTIVE") != "1" or fake_model.ACTIVE is None:
        r.skip("the fake model is not running (APPCI_FAKE_MODEL=1): long scripted replies need it")
    if ctx.win:
        r.skip("Windows: the PowerShell pane has no rc hook (no exact received-command proof) and no verified pty-echo frame pairing; Linux and macOS run this case")
    if not ctx.log_path or not os.path.exists(ctx.log_path):
        r.skip("no app log at %s: Enter buffers, frames and UI items cannot be read" % ctx.log_path)
    if ctx.log_view_mode() is None:
        r.skip("the app log has no hop=view_mode line (a build older than #1568)")
    fake = fake_model.ACTIVE
    ntabs = max(2, min(int(os.environ.get("APPCI_MT_TABS", "6")), 8))
    gap_ms = int(os.environ.get("APPCI_MT_GAP_MS", "100"))
    chunks = max(50, int(float(os.environ.get("APPCI_MT_STREAM_S", "75")) * 1000 / gap_ms))
    tags = ["S%d" % (k + 2) for k in range(ntabs)]
    n_keys = ctx.count_lat(40) + lat.warmup()
    typed_tokens = []
    CA.fresh_conversation(ctx, r)
    try:
        # ---- baseline: nothing streams
        off_idle = ctx.log_size()
        time.sleep(6)
        fps_idle = frames_per_s(ctx.log_since(off_idle), 6)
        a_base, a_base_max = agent_phase(ctx, r, fake, "base", typed_tokens)
        base_ds, _ = terminal_phase(ctx, r, "base", n_keys)
        back_to_agent(ctx, r)
        # ---- start the streams, one per new tab
        for k, tag in enumerate(tags):
            ctx.be.key(R.NEWTAB)
            time.sleep(1.2)
            if ctx.log_view_mode() != "agent":
                ctx.be.type("/agent", 12, enter=True)
                if not ctx.until(lambda: ctx.log_view_mode() == "agent", 10):
                    r.skip("tab %d did not open in agent view" % (k + 2))
                time.sleep(0.8)
            sent = "Please stream long reply %s chunks %d gap %d" % (tag, chunks, gap_ms)
            o2 = ctx.log_size()
            CA.type_fast(ctx, sent)
            ctx.until(lambda: CA.enter_buffers(ctx.log_since(o2)), 6)
            err = CA.judge_agent_input(sent, CA.enter_buffers(ctx.log_since(o2)))
            r.check(err is None, "stream tab %s prompt: %s" % (tag, err))
            if not ctx.until(lambda: tag in fake.streams, 30):
                r.check(False, "%s: the app sent no request for the long reply within 30 s" % tag)
        ctx.be.key(R.tab_key(ctx, 1))                       # back to the typing tab: every streaming tab is now hidden
        time.sleep(1.5)
        running = [t for t in tags if t in fake.streams and not fake.streams[t]["finished"]]
        r.check(len(running) >= max(1, ntabs // 2), "only %d of %d streams are still running when the typing starts (%s): lengthen APPCI_MT_STREAM_S" % (len(running), ntabs, running))
        off_hid = ctx.log_size()
        time.sleep(6)
        hid_txt = ctx.log_since(off_hid)
        fps_hidden = frames_per_s(hid_txt, 6)
        r.metric("frames_per_s", dict(idle_no_streams=round(fps_idle, 1), idle_hidden_streams=round(fps_hidden, 1), streams=len(running)))
        # #1979: the same window as wakeups per decision and in-place re-renders of hidden views (output_frames judges these for pty output; here
        # the hidden output is agent replies, so the numbers are reported only)
        import cases_output_frames as OF
        r.metric("hidden_streams_renders", OF.metrics_of(OF.window_stats(hid_txt), 6))
        r.note("#805 hop: the app built %.1f frames/s with %d hidden streaming tabs and an idle typing tab (%.1f frames/s with nothing streaming); informational, no fixed budget" % (fps_hidden, len(running), fps_idle))
        # ---- the same phases under load
        a_load, a_load_max = agent_phase(ctx, r, fake, "load", typed_tokens)
        allowed = 2
        r.check(a_load <= a_base + allowed, "agent input under %d streaming tabs: %d main-thread logic item(s) >= 4 ms (baseline %d, allowed +%d, worst %.1f ms)" % (len(running), a_load, a_base, allowed, a_load_max))
        load_ds, cnt = terminal_phase(ctx, r, "load", n_keys)
        if len(load_ds) < 0.5 * n_keys or len(base_ds) < 0.5 * n_keys:
            r.check(False, "only %d baseline / %d loaded key->frame pairs of ~%d keys (no keydown/frame lines: %s)" % (len(base_ds), len(load_ds), n_keys - lat.warmup(), cnt))
        else:
            ok, msg = compare_latency(base_ds, load_ds)
            r.metric("key_to_frame_load_vs_base_ms", dict(base=C.stats(base_ds), load=C.stats(load_ds)))
            if ok is None:
                r.no_reference(msg)
            else:
                r.check(ok, "terminal-view key->frame under %d hidden streaming tabs: %s" % (len(running), msg))
            p95b, p95l = lat.pct(base_ds, 0.95), lat.pct(load_ds, 0.95)
            r.check(p95l <= p95b + 16.0, "key->frame p95 %.1f ms under load vs %.1f ms baseline (+16 ms = one dropped frame allowed)" % (p95l, p95b))
        # ---- every reply completes, in its own tab, with its own content
        wait_s = chunks * gap_ms / 1000.0 + 90
        done = ctx.until(lambda: all(t in fake.streams and (fake.streams[t]["finished"] or fake.streams[t]["broken"]) for t in tags), wait_s, step=1.0)
        r.check(bool(done), "not every stream ended within %.0f s" % wait_s)
        for p in streams_problems(fake.streams, tags, chunks + 1):
            r.check(False, "stream " + p)
        for k, tag in enumerate(tags):
            ctx.be.key(R.tab_key(ctx, k + 2))
            time.sleep(1.2)
            CA.type_fast(ctx, "Please follow up check %s" % tag)
            ctx.until(lambda: tag in fake.followups, 30)
            time.sleep(0.8)
        ctx.be.key(R.tab_key(ctx, 1))
        for k, tag in enumerate(tags):
            n_for_tag = sum(1 for _, t in fake.user_turns if "stream long reply %s " % tag in t)
            r.check(n_for_tag == 1, "tab %s: %d requests carried its stream prompt (expected 1: a key typed elsewhere or a replayed Enter would add one)" % (tag, n_for_tag))
        bad, unproven = bleed_problems(fake.followups, tags, typed_tokens)
        for b in bad:
            r.check(False, "cross-tab: " + b)
        for u in unproven:
            r.no_reference("cross-tab content: " + u)
        r.check(ctx.app.alive(), "the app died during the multi-tab streaming case")
        r.check(not [l for l in ctx.log_since(off_idle).splitlines() if "panicked at" in l], "panic in the app log during the case")
    finally:
        # The case's own terminal phases pressed Escape (terminal_phase), so the typing tab may be left in the terminal view; the case owns that exit and re-enters the agent
        # view itself (same rule as agent_code_recycle, r18u8) instead of the runner reporting it as an app finding (Round 8: left_agent_mode_after_agent_multitab_streaming).
        try:
            time.sleep(0.8)
            if ctx.log_view_mode() == "terminal":
                ctx.be.type("/agent", 12, enter=True)
                ok = ctx.until(lambda: ctx.log_view_mode() == "agent", 10)
                r.note("the case's own Escape left the agent view; it re-entered with /agent (%s)" % ("agent view again" if ok else "NOT proven: the runner will relaunch"))
        except Exception:
            pass
        CA.cleanup_tmp(ctx)
