# appci/modeguard.py: the view-mode guard that runs before EVERY case of both modes (#2170).
# Round 15 Windows: an earlier case left the pane in an agent conversation and every later [terminal] case ran there and PASSED. run.py only
# re-checked the mode after agent-mode cases. Now, before each case: read the pane's actual view mode (the app's own `hop=view_mode` log line, the
# screen by OCR where the backend has an engine), put the pane back when it differs (Escape out of agent view; the agent-entry chord; a relaunch
# as the last resort), re-check, and if it still differs the case does NOT run and its row is FAIL "wrong mode: ran in X, case is Y".
# Every row records `ran_mode`, what was observed. Pure parts and the driver take a duck-typed ctx so selftest_r23_modeguard.py needs no app.
import time

MODES = ("terminal", "agent")


def wrong_mode_text(seen, want):
    return "wrong mode: ran in %s, case is %s" % (seen, want)


def screen_mode(ctx, tag):
    """'agent' | 'terminal' | None from an OCR read of the screen; None when the backend has no engine (the log alone then decides)"""
    try:
        import cases
        txt = ctx.ocr(tag)
        if txt is None:
            return None
        low = " ".join(txt.lower().split())
        is_agent, hint = cases.ocr_agent_markers(low)
        if is_agent:
            return "agent"
        return "terminal" if hint or "new terminal session" in low else None
    except Exception:
        return None


def observe(ctx, tag):
    """-> (mode, source note). mode is 'agent' | 'terminal' | None (nothing could say). The primary source is ctx.mode_now (app log first, OCR when the
    log is silent). With the log deciding, the screen is read as well: a disagreement is NOT hidden, it is returned as the note."""
    logged = ctx.mode_now(tag)
    note = None
    if logged in MODES:
        scr = screen_mode(ctx, tag + "_screen")
        if scr in MODES and scr != logged:
            note = "the app log says %s but the screen reads as %s (OCR); the log decides" % (logged, scr)
    return logged, note


def escape_to_terminal(ctx):
    ctx.be.key("Escape")
    ctx.until(lambda: ctx.log_view_mode() == "terminal", 6)
    time.sleep(0.5)


def enter_agent(ctx):
    chord = "cmd+Return" if ctx.be.name == "mac" else "ctrl+shift+Return"
    for _ in range(2):
        if ctx.log_view_mode() == "agent":
            break
        ctx.be.key(chord)
        ctx.until(lambda: ctx.log_view_mode() == "agent", 6)
    time.sleep(0.5)


def plan(want):
    """pure: the recovery steps, in order, for a pane that is not in `want`"""
    return ["escape", "escape", "relaunch"] if want == "terminal" else ["enter_agent", "enter_agent", "relaunch"]


def ensure(ctx, want, name, relaunch=None, exempt=False):
    """Establish the pane's mode before case `name`, recovering when it differs.
    -> dict(ok, ran_mode, fail (text or None), notes [str], recovered [step names]).
    exempt (mode_proof, which asserts the mode itself and sends its own Escape): observed and recorded, never recovered or failed.
    A mode nothing can read (no log line, no OCR engine) is recorded as 'unknown' with a note: it is not a mismatch, and it is not a pass of the mode."""
    notes, steps = [], []
    seen, note = observe(ctx, "mode_before_" + name)
    if note:
        notes.append(note)
    if exempt:
        return dict(ok=True, ran_mode=seen or "unknown", fail=None, notes=notes, recovered=steps)
    if seen is None:
        notes.append("mode before the case could not be established (no hop=view_mode line in the app log and no OCR engine): ran_mode unknown")
        return dict(ok=True, ran_mode="unknown", fail=None, notes=notes, recovered=steps)
    first = seen
    for step in plan(want):
        if seen == want:
            break
        if step == "relaunch" and relaunch is None:
            continue
        steps.append(step)
        try:
            if step == "escape":
                escape_to_terminal(ctx)
            elif step == "enter_agent":
                enter_agent(ctx)
            else:
                relaunch()
        except Exception as e:
            notes.append("recovery step %s raised %r" % (step, e))
        seen, note = observe(ctx, "mode_after_%s_%s" % (step, name))
        if note:
            notes.append(note)
        if seen is None:
            break
    if seen == want:
        if steps:
            notes.append("mode guard: the pane was in %s before this %s case; put back with %s" % (first, want, " + ".join(steps)))
        return dict(ok=True, ran_mode=seen, fail=None, notes=notes, recovered=steps)
    shown = seen or first
    return dict(ok=False, ran_mode=shown, fail=wrong_mode_text(shown, want) + " (recovery tried: %s)" % (", ".join(steps) or "none"), notes=notes, recovered=steps)
