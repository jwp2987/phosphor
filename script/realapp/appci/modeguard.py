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


def unproven_text(want, notes):
    return "mode unproven: the pane's view mode before the case could not be established (case is %s). %s" % (want, "; ".join(notes) or "no log line, no screen read")


def screen_read(ctx, tag):
    """-> (mode, why). mode is 'agent' | 'terminal' | None from an OCR read of the screen through ctx.ocr(): the one OCR path of every backend (Apple Vision on
    macOS, Windows.Media.Ocr on Windows, tesseract elsewhere). why says what a None means: 'no OCR engine' (ctx.ocr gave None), 'no marker' (text was read
    and shows neither mode) or 'ocr failed: ...'. The markers are cases.ocr_agent_markers (the slash-less terminal hint is not an agent header)."""
    try:
        import cases
        txt = ctx.ocr(tag)
    except Exception as e:
        return None, "ocr failed: %r" % (e,)
    if txt is None:
        return None, "no OCR engine"
    try:
        low = " ".join(txt.lower().split())
        is_agent, hint = cases.ocr_agent_markers(low)
        if is_agent:
            return "agent", ""
        if hint or "new terminal session" in low:
            return "terminal", ""
        return None, "no marker"
    except Exception as e:
        return None, "ocr failed: %r" % (e,)


def screen_mode(ctx, tag):
    """'agent' | 'terminal' | None from an OCR read of the screen; None when the backend has no engine or the text shows no marker"""
    return screen_read(ctx, tag)[0]


def observe(ctx, tag):
    """-> (mode, source note). mode is 'agent' | 'terminal' | None (nothing could say). The app log (ctx.mode_now) is read first. With the log deciding, the
    screen is read as well: a disagreement is NOT hidden, it is returned as the note. With the log silent (a relaunched app whose new log has no
    hop=view_mode line yet, #2173) the screen decides through the same ctx.ocr() every backend uses, with cases.ocr_agent_markers (which also reads
    the markers mode_now's own text test misses); the note says it was the screen, or why even that could not say."""
    logged = ctx.mode_now(tag)
    note = None
    if logged in MODES:
        scr = screen_mode(ctx, tag + "_screen")
        if scr in MODES and scr != logged:
            note = "the app log says %s but the screen reads as %s (OCR); the log decides" % (logged, scr)
        return logged, note
    scr, why = screen_read(ctx, tag + "_screen")
    if scr in MODES:
        return scr, "no hop=view_mode line in the app log: the mode is read from the screen (OCR): %s" % scr
    return None, "no hop=view_mode line in the app log and the screen read gave no mode (%s)" % why


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
    A mode nothing can read (no log line, and the OCR read of the screen shows no mode) is ran_mode 'unknown' and the row is a FAIL "mode unproven" (#2173:
    the app now logs hop=view_mode for every pane whenever its log is opened or replaced, and the screen is read with the same OCR as every case, so
    an unknown mode is a harness or app fault, never a pass). exempt rows (mode_proof) record it instead."""
    notes, steps = [], []
    seen, note = observe(ctx, "mode_before_" + name)
    if note:
        notes.append(note)
    if exempt:
        return dict(ok=True, ran_mode=seen or "unknown", fail=None, notes=notes, recovered=steps)
    if seen is None:
        return dict(ok=False, ran_mode="unknown", fail=unproven_text(want, notes), notes=notes, recovered=steps)
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
