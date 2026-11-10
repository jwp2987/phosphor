# appci/cases_output_frames.py (#1979): what pty output costs the window, counted from the app's own log. One case, `output_frames`, both modes.
#
# The issue's two claims, each turned into a number the app logs (target warpui::frame_cause, debug; the launcher turns it on):
#   * "a visible busy tab drives ~1 frame per output line": frames built per second (`frame <n>: updated=[..] removed=.. redraw=.. paint_only=..`,
#     AppContext::build_scene_inner) against the delivered wakeups per second (`wakeup: redraw=<Full|Relayout|PaintOnly> reachable=<bool> view=<id>`,
#     TerminalView::handle_terminal_wakeup). The wakeup stream is throttled to 60/s per view (WAKEUP_THROTTLE_PERIOD), so the frame rate of one streaming
#     pane must stay at or under that cap, there must be no more than one frame per wakeup, and an output-only wakeup must reuse the element tree
#     (`redraw=Relayout`, wakeup_redraw.rs) instead of re-rendering the terminal view (`redraw=Full`).
#   * "hidden views are still re-rendered ~60/s per pane": with N tabs streaming while ANOTHER tab is shown, the wakeups of the hidden views
#     (`reachable=false`) must not be `redraw=Full` (a Full wakeup of a hidden view is one in-place re-render), the presenter's in-place re-renders
#     (`unmounted_render: n=<count> views=[..]`, Presenter::render_unmounted_updates) must stay rare, and the window must build no more frames than it
#     does idle (#805).
# Phases, all in one app run: (1) idle window, (2) the shown tab streams a line every ~20 ms, (3) APPCI_OF_TABS (4) more tabs stream while tab 1 is
# shown and idle. The streams are bounded (`seq`), so nothing is left running; the extra tabs stay open and idle (an idle tab costs nothing).
# No timing of keys and no screenshots are needed for a verdict. A build without the `wakeup:` line (older than #1979's measurement) cannot be judged:
# UNVERIFIABLE, never a pass. Thresholds are this case's own reading of the code's guarantees (see the judge functions), not an owner budget.
import os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases_robust as R  # noqa: E402
from cases import case  # noqa: E402

FRAME_RE = re.compile(r"\bframe (\d+): updated=\[([^\]]*)\] removed=\d+ redraw=(true|false) paint_only=(true|false)")
WAKEUP_RE = re.compile(r"\bwakeup: redraw=(Full|Relayout|PaintOnly) reachable=(true|false) view=(\S+)")
UNMOUNTED_RE = re.compile(r"\bunmounted_render: n=(\d+) views=\[([^\]]*)\]")
WAKEUP_CAP_PER_S = 60.0        # app/src/terminal/view.rs MAX_WAKEUPS_PER_SECOND
FULL_SHARE_MAX = 0.20          # share of output-only wakeups that may still re-render the view (block start/end, prompt, footer changes)
UNMOUNTED_PER_TAB_S_MAX = 5.0  # in-place re-renders of hidden views per hidden streaming tab per second (the issue's claim is ~60)


# ---------------------------------------------------------------- pure helpers (unit-tested in selftest.py)
def window_stats(text):
    """counts over one log window:
    frames            frames the app built: every `frame 1:` line, and a `frame 2:`/`frame 3:` line only when it carries a reason (the build loop logs the
                      line and then stops when nothing is pending, AppContext::build_scene_inner)
    frames_rerender   built frames whose updated=[..] names TerminalView (the terminal view was re-rendered for that frame)
    wakeups           {(decision, shown)} of the delivered terminal wakeups; shown = the view was in the window's element tree
    views             {view id: [shown wakeups, hidden wakeups]}
    unmounted_events  flushes that re-rendered hidden views in place; unmounted_views: views re-rendered that way; unmounted_names {type name: count}"""
    st = dict(frames=0, frames_rerender=0, wakeups={}, views={}, unmounted_events=0, unmounted_views=0, unmounted_names={})
    for line in text.splitlines():
        m = FRAME_RE.search(line)
        if m:
            if m.group(1) == "1" or m.group(2).strip() or m.group(3) == "true" or m.group(4) == "true":
                st["frames"] += 1
                if '"TerminalView"' in m.group(2):
                    st["frames_rerender"] += 1
            continue
        m = WAKEUP_RE.search(line)
        if m:
            shown = m.group(2) == "true"
            st["wakeups"][(m.group(1), shown)] = st["wakeups"].get((m.group(1), shown), 0) + 1
            v = st["views"].setdefault(m.group(3), [0, 0])
            v[0 if shown else 1] += 1
            continue
        m = UNMOUNTED_RE.search(line)
        if m:
            st["unmounted_events"] += 1
            st["unmounted_views"] += int(m.group(1))
            for name in re.findall(r'"([^"]*)"', m.group(2)):
                st["unmounted_names"][name] = st["unmounted_names"].get(name, 0) + 1
    return st


def wakeup_count(st, shown=None, decision=None):
    return sum(n for (d, s), n in st["wakeups"].items() if (shown is None or s == shown) and (decision is None or d == decision))


def metrics_of(st, secs):
    """the window as flat numbers for results.json"""
    return dict(frames_per_s=round(st["frames"] / secs, 1), frames_rerender_per_s=round(st["frames_rerender"] / secs, 1),
                wakeups_shown_per_s=round(wakeup_count(st, True) / secs, 1), wakeups_hidden_per_s=round(wakeup_count(st, False) / secs, 1),
                wakeups={"%s_%s" % (d, "shown" if s else "hidden"): n for (d, s), n in sorted(st["wakeups"].items())},
                views_shown=sum(1 for a, _ in st["views"].values() if a), views_hidden=sum(1 for _, b in st["views"].values() if b),
                unmounted_events=st["unmounted_events"], unmounted_views=st["unmounted_views"], unmounted_names=dict(sorted(st["unmounted_names"].items())))


def judge_visible(st, secs, idle_fps):
    """-> (unproven reason or None, [(ok, message)]) for the window in which the SHOWN tab streams.
    Unproven when the build logs no wakeup lines, or the stream did not run in the shown pane (fewer than 5 wakeups/s)."""
    shown = wakeup_count(st, True)
    if not st["wakeups"]:
        return "the app log has no 'wakeup:' line (a build older than the #1979 measurement, or warpui::frame_cause logging is off)", []
    if shown < 5 * secs:
        return "only %d wakeups of the shown view in %.0f s: the stream did not run in the shown pane" % (shown, secs), []
    fps, wps = st["frames"] / secs, shown / secs
    full = wakeup_count(st, True, "Full")
    out = [
        (fps <= WAKEUP_CAP_PER_S * 1.1 + idle_fps,
         "visible streaming tab: %.1f frames/s (limit %.0f = the %.0f/s wakeup throttle + 10%% + the idle window's %.1f frames/s)" % (
             fps, WAKEUP_CAP_PER_S * 1.1 + idle_fps, WAKEUP_CAP_PER_S, idle_fps)),
        (st["frames"] <= shown * 1.1 + (idle_fps + 2.0) * secs,
         "visible streaming tab: %.1f frames/s for %.1f wakeups/s (more than one frame per wakeup; allowed: wakeups + 10%% + the idle window's %.1f "
         "frames/s + 2/s)" % (fps, wps, idle_fps)),
        (full <= FULL_SHARE_MAX * shown,
         "visible streaming tab: %d of %d output wakeups re-rendered the terminal view (redraw=Full; allowed %.0f%%: an output-only wakeup reuses the "
         "element tree, wakeup_redraw.rs)" % (full, shown, FULL_SHARE_MAX * 100)),
    ]
    return None, out


def judge_hidden(st, secs, idle_fps, tabs):
    """-> (unproven reason or None, [(ok, message)]) for the window in which `tabs` HIDDEN tabs stream and the shown tab is idle.
    Unproven when no hidden view woke at least twice a second per tab (the streams were not running), or the shown tab was itself streaming."""
    hidden, shown = wakeup_count(st, False), wakeup_count(st, True)
    if not st["wakeups"]:
        return "the app log has no 'wakeup:' line (a build older than the #1979 measurement, or warpui::frame_cause logging is off)", []
    if hidden < 2 * secs * tabs:
        return "only %d wakeups of hidden views in %.0f s with %d tabs meant to stream: the hidden streams were not running" % (hidden, secs, tabs), []
    if shown > 2 * secs:
        return "%d wakeups of a SHOWN view in %.0f s: the shown tab was itself streaming, so this window does not measure hidden tabs" % (shown, secs), []
    fps = st["frames"] / secs
    full = wakeup_count(st, False, "Full")
    limit = idle_fps + max(2.0, 0.5 * idle_fps)
    per_tab = st["unmounted_views"] / secs / tabs
    names = ", ".join("%s x%d" % kv for kv in sorted(st["unmounted_names"].items(), key=lambda kv: -kv[1])[:6]) or "none"
    out = [
        (fps <= limit,
         "%d hidden streaming tabs: the window built %.1f frames/s (idle window: %.1f; limit %.1f): hidden-tab output must not cost frames (#805)" % (
             tabs, fps, idle_fps, limit)),
        (full <= FULL_SHARE_MAX * hidden,
         "%d hidden streaming tabs: %d of %d wakeups of hidden views re-rendered the view (redraw=Full; allowed %.0f%%)" % (
             tabs, full, hidden, FULL_SHARE_MAX * 100)),
        (per_tab <= UNMOUNTED_PER_TAB_S_MAX,
         "%d hidden streaming tabs: %.1f in-place re-renders of hidden views per tab per second (limit %.0f; views: %s)" % (
             tabs, per_tab, UNMOUNTED_PER_TAB_S_MAX, names)),
    ]
    return None, out


def stream_line(pane, tag, lines):
    """a command that prints `lines` lines, one every ~20 ms, and then ends by itself, for the pane's own shell (bash/zsh; Windows PowerShell)"""
    if pane == "powershell":
        return "1..%d | %% { 'ofr_%s_' + $_; Start-Sleep -Milliseconds 20 }" % (lines, tag)
    return "for i in $(seq %d); do echo ofr_%s_$i; sleep 0.02; done" % (lines, tag)


# ---------------------------------------------------------------- the case
def _wait_quiet(ctx, quiet_s, timeout):
    """True once the app logs no terminal wakeup for `quiet_s` (the bounded streams ended); False at `timeout`"""
    t0 = time.time()
    while time.time() - t0 < timeout:
        off = ctx.log_size()
        time.sleep(quiet_s)
        if not WAKEUP_RE.search(ctx.log_since(off)):
            return True
    return False


def _window(ctx, secs):
    off = ctx.log_size()
    time.sleep(secs)
    return window_stats(ctx.log_since(off))


@case("output_frames", order=89, gui=True)
def c_output_frames(ctx, r):
    """#1979: frames and view re-renders caused by pty output, from the app log: the shown tab streaming (frames <= the 60/s wakeup throttle, at most one
    frame per wakeup, output-only wakeups reuse the element tree) and N hidden tabs streaming (no extra frames, hidden views not re-rendered per wakeup)."""
    if not getattr(ctx, "log_path", None) or not os.path.exists(ctx.log_path):
        r.skip("no app log at %s: frames and wakeups are read from it" % getattr(ctx, "log_path", None))
    if ctx.pane == "cmd":
        r.skip("cmd.exe pane: the bounded stream line is written for PowerShell and bash/zsh")
    secs = float(os.environ.get("APPCI_OF_WINDOW_S", "6"))
    tabs = max(1, min(int(os.environ.get("APPCI_OF_TABS", "4")), 7))
    vis_lines = int(os.environ.get("APPCI_OF_VISIBLE_LINES", "450"))
    hid_lines = int(os.environ.get("APPCI_OF_HIDDEN_LINES", "1200"))
    ctx.reset()
    time.sleep(1.5)
    # ---- (1) idle
    idle = _window(ctx, secs)
    idle_fps = idle["frames"] / secs
    r.metric("idle", metrics_of(idle, secs))
    # ---- (2) the shown tab streams
    ctx.be.type(stream_line(ctx.pane, "v", vis_lines), 10, enter=True)
    time.sleep(1.5)
    vis = _window(ctx, secs)
    r.metric("visible_stream", metrics_of(vis, secs))
    ctx.shot("output_frames_visible")
    unproven, verdicts = judge_visible(vis, secs, idle_fps)
    if unproven:
        # no stream in the shown pane (or no wakeup lines at all): opening more tabs would prove nothing, and a line that did not reach the shell must
        # not be typed four more times
        r.unverifiable("visible streaming tab: " + unproven)
        ctx.reset()
        return
    for ok, msg in verdicts:
        r.check(ok, msg)
    if not _wait_quiet(ctx, 1.5, vis_lines * 0.06 + 30):
        r.note("the shown tab's stream was still producing wakeups after its expected end; it is stopped with Ctrl-C")
        ctx.reset()
        time.sleep(1.0)
    # ---- (3) N hidden tabs stream, tab 1 is shown and idle
    for k in range(tabs):
        ctx.be.key(R.NEWTAB)
        time.sleep(1.2)
        ctx.be.guard()
        ctx.be.type(stream_line(ctx.pane, "h%d" % (k + 2), hid_lines), 10, enter=True)
        time.sleep(0.3)
    ctx.be.key(R.tab_key(ctx, 1))
    time.sleep(1.5)
    hid = _window(ctx, secs)
    r.metric("hidden_streams", dict(metrics_of(hid, secs), tabs=tabs))
    ctx.shot("output_frames_hidden")
    unproven, verdicts = judge_hidden(hid, secs, idle_fps, tabs)
    if unproven:
        r.unverifiable("hidden streaming tabs: " + unproven)
    for ok, msg in verdicts:
        r.check(ok, msg)
    r.note("#1979: idle %.1f frames/s; shown tab streaming %.1f frames/s for %.1f wakeups/s (%d Full); %d hidden tabs streaming %.1f frames/s, "
           "%.1f hidden wakeups/s (%d Full), %d in-place re-renders of hidden views" % (
               idle_fps, vis["frames"] / secs, wakeup_count(vis, True) / secs, wakeup_count(vis, True, "Full"), tabs, hid["frames"] / secs,
               wakeup_count(hid, False) / secs, wakeup_count(hid, False, "Full"), hid["unmounted_views"]))
    if not _wait_quiet(ctx, 1.5, hid_lines * 0.06 + 45):
        r.note("a hidden tab was still streaming after its expected end (the streams are bounded; the tabs are left as they are)")
    r.check(ctx.app.alive(), "the app died during the output-frames case")
    ctx.reset()
