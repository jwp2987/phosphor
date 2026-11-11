# appci/cases_macptr.py: round 15 wave 4, macOS harness (#2061, docs/design/r14-test-gaps.md T32; design note docs/design/r10-r15-w4mac.md).
#   mac_pointer_chord_proof   macOS only. The proof that the Mac harness's own new tools work on this machine, BEFORE the cases that rest on them
#                             (order 2, right after mode_proof), so a failure of one of those cases can be read as the app's or the harness's:
#       tools     mptr, appkey and ocrw compiled (mac/ci/app_test.sh); a missing one FAILS here and SKIPs the cases that need it
#       scale     pixels per point from a real screenshot (Retina: 2)
#       pointer   the cursor is where two different moves put it (the window server accepted the events); a click on the menu bar, which is
#                 not the app's window, is REFUSED by the tool (the control: the safety fence is live); the app's find hook and Apple Vision
#                 agree on where a printed word is (the window-origin assumption the hook's positions rest on); a double click on that word
#                 and the copy key put exactly it on the clipboard (the click reached the app)
#       chords    Cmd-H is refused, and Cmd-W is refused while the reference terminal is the target (controls, nothing is sent); Cmd-W closes a
#                 tab this case opened (the pane that answers is the first tab again). The result is kept on the backend
#                 (app_chords_proven): after a FAILED proof no later case sends an app chord again in this run (cases_robust.mac_chords_why_not).
#       close key the job names the macOS close key in APPCI_KEY_CLOSE_WHEN_PROVEN; it becomes APPCI_KEY_CLOSE for the cases after this one only
#                 when this case PASSED (cases_robust.proven_close_key; owner 2026-11-11), and a later failed proof takes it back.
# Cmd-Q is not sent here (it needs a relaunch): quit_24_tabs and agent_restart_quit_* are its cases.
import os, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases as C  # noqa: E402  (cases.py imports this module near its end, so everything below is already defined)
import cases_r14_w1a as W1  # noqa: E402
import cases_robust as R  # noqa: E402
import cases_ui as U  # noqa: E402
import macptr as MP  # noqa: E402
from cases import case  # noqa: E402

HOOK_OCR_SLACK_PX = 24         # the find hook and Vision on the same word: centres within this many screenshot pixels (Vision's word boxes are loose)
MAC_ONLY = ("macOS only: this case proves the macOS harness's own pointer tool (mac/tools/mptr) and app-only chords (mac/tools/appkey); Linux and "
            "Windows inject through other tools that have their own proofs")


def hook_ocr_verdict(hook, ocr, slack=HOOK_OCR_SLACK_PX):
    """pure: None when the two positions of one word (cases_ui.words() rows) agree, else the sentence. Either one missing is not a verdict (None)."""
    if not hook or not ocr:
        return None
    dx, dy = hook[1] - ocr[1], hook[2] - ocr[2]
    if abs(dx) <= slack and abs(dy) <= slack:
        return None
    return ("the app's find hook puts the word at (%d, %d) and Apple Vision reads it at (%d, %d), %d / %d px apart: the hook's positions are not "
            "screen positions on this machine (the window's corner is not the corner of what the app draws, or the display scale differs from the "
            "app's); set APPCI_NO_FIND_HOOK=1 to place the pointer by OCR until this is corrected" % (hook[1], hook[2], ocr[1], ocr[2], dx, dy))


def menu_bar_point(display):
    """pure: a point (points) in the middle of the menu bar of the main display, where no application window can be"""
    return display[0] + display[2] / 2.0, display[1] + 6.0


def _pointer_half(ctx, r):
    m = U.mac_mouse(ctx)
    if not r.check(m is not None, MP.missing_tool_reason(ctx.be, "mptr")):
        return None
    r.metric("display_scale", m.scale if m.scale else -1)
    if not r.check(bool(m.scale), "the display scale could not be measured: %s" % m.scale_how):
        return None
    r.note("display scale %s (%s)" % (m.scale, m.scale_how))
    rect = ctx.be.window_rect()
    if not r.check(bool(rect), "the app window's rectangle could not be read (winrect rect <pid>)"):
        return None
    cx, cy = (rect[0] + rect[2] / 2.0) * m.scale, (rect[1] + rect[3] / 2.0) * m.scale
    seen = []
    for dx in (0, 60):                                         # two different places: a cursor that never moves cannot be in both
        m.move(cx + dx, cy)
        seen.append((m.last, m.delivered()))
    r.metric("cursor_readback", [[list(w), list(l) if l else None] for (w, l), _ok in seen])
    if not r.check(all(ok for _l, ok in seen), "the cursor is not where the pointer tool put it (wanted / read back, points: %r): the events were not "
                                                 "accepted (Accessibility for the mptr binary?)" % [l for l, _ok in seen]):
        return None
    # control: the tool's own fence. The menu bar is not the app's window, so a click there must be refused and must not happen.
    disp = ctx.be.ptr_info().get("display")
    if r.check(bool(disp), "`mptr info` gave no display line"):
        bx, by = menu_bar_point(disp)
        refused = None
        try:
            ctx.be.ptr(["k:%.1f:%.1f:1:1" % (bx, by)])
        except RuntimeError as e:
            refused = str(e)
        r.check(bool(refused) and "refused" in refused, "a click on the menu bar at (%.0f, %.0f) points was NOT refused by mptr (%r): the fence that "
                                                         "keeps clicks inside the app's own windows is not live" % (bx, by, refused))
    return m


def _click_half(ctx, r, m):
    tok = "mpcp" + U.ocr_safe(ctx.tok(6).lower().replace("_", ""))
    U.register_find(ctx, tok)
    seen = ctx.log_size()
    U.run_cmd(ctx, "printf 'mpcp%%s\\n' %s" % tok[4:])         # the command line does not hold the token it prints
    time.sleep(0.8)
    hook = U._hook_word(ctx, tok, seen)
    ws, _p = U.words(ctx, "mpcp_find") if U.have_word_ocr(ctx) else ([], None)
    hits = U.token_hits(ws, tok)
    ocr = hits[-1] if hits else None
    r.metric("word_at", dict(hook=list(hook[1:3]) if hook else None, ocr=list(ocr[1:3]) if ocr else None))
    if not hook:
        U.hook_missing_note(r, tok)
    if not ocr:
        r.note("Apple Vision (ocrw) did not read %r on the screenshot (shots/*mpcp_find.png): the hook's position has no second opinion" % tok)
    bad = hook_ocr_verdict(hook, ocr)
    r.check(bad is None, bad)
    w = (hook if bad is None else None) or ocr
    if not r.check(bool(w), "the printed word %r was located neither by the app's find hook nor by OCR: the click half was not run" % tok):
        return
    got = U.copy_check(ctx, r, "mpcp", lambda: m.click(w[1], w[2], 1, 2), tok)
    r.check(got.strip() == tok, "double click on %r at %r + the copy key: the clipboard holds %r, expected exactly the word (the sentinel = no "
                                "selection: the click did not reach the app, or missed the word)" % (tok, tuple(w[1:3]), got[:60]))
    ctx.be.key("Escape")
    time.sleep(0.3)


def _chord_half(ctx, r):
    be = ctx.be
    # controls: nothing is sent by either
    for spec in ("cmd+h", "cmd+shift+q", "ctrl+cmd+f"):
        try:
            be.key(spec)
            r.check(False, "the backend SENT the forbidden chord %s" % spec)
        except ValueError:
            pass
    was = be.guard_override
    be.guard_override = "Terminal"
    try:
        be.app_chord("cmd+w")
        r.check(False, "Cmd-W was sent while the reference terminal was the injection target")
    except RuntimeError as e:
        r.check("reference terminal" in str(e), "Cmd-W with the reference terminal as target was refused for another reason: %s" % e)
    finally:
        be.guard_override = was
    bad = R.mac_chords_why_not(ctx)
    if not r.check(bad is None, "the app-only chords cannot be sent in this run: %s" % bad):
        be.app_chords_proven = False
        return
    r.metric("chord_via", be.CHORD_VIA)
    t = W1.Tabs(ctx, r)
    home = R.norm_path(ctx.W)
    if not r.check(t.where() == home, "before the chord proof the pane that has the keyboard is not the first tab: nothing was opened or closed"):
        return
    d = os.path.join(ctx.W, "mpc_" + ctx.tok(5).lower())
    t.new_tab(d)
    if not r.check(t.where() == R.norm_path(d), "the new tab's shell is not in its directory (cwd probe): Cmd-W was NOT sent"):
        return
    ok = False
    try:
        be.key("cmd+w")
        time.sleep(1.2)
        back = t.where()
        ok = back == home
        r.check(ok, "Cmd-W (appkey, via %s) on a tab with an idle shell: the pane that has the keyboard is in %r, expected the first tab %r: the chord did "
                    "not close the tab (if no `hop=os_key_event` line follows it in the app log the event never reached the app: try "
                    "APPCI_MAC_CHORD_VIA=hid)" % (be.CHORD_VIA, back, home))
    except RuntimeError as e:
        r.check(False, "Cmd-W was not sent: %s" % e)
    finally:
        be.app_chords_proven = ok
        if not ok and t.where() != home:                        # leave one tab: the throw-away tab's shell ends on Ctrl-D
            R.close_tab_best_effort(ctx, r)
            time.sleep(1.0)
    r.check(t.where() == home, "at the end the pane that has the keyboard is not the first tab (cwd probe)")


@case("mac_pointer_chord_proof", order=2, gui=True)
def c_mac_pointer_chord_proof(ctx, r):
    """#2061 (T32): the macOS pointer tool and the app-only chords work on this machine. See the header of cases_macptr.py for the three halves and
    their controls. Runs before every case that uses them; a failed chord half switches the chord path off for the rest of the run."""
    if not U.mac(ctx):
        r.skip(MAC_ONLY)
    R.proven_close_key(ctx.be, False)          # nothing is proven yet in this pass: a close key the other mode's proof gave is taken back first
    for t in ("mptr", "appkey", "ocrw"):
        r.check(bool(MP.tool(ctx.be, t)), MP.missing_tool_reason(ctx.be, t))
    ctx.reset()
    m = _pointer_half(ctx, r)
    if m is not None:
        _click_half(ctx, r, m)
    ctx.reset()
    _chord_half(ctx, r)
    C.effect_run(ctx, r, "mpcp_end", 3, 0.0)
    r.check(ctx.app.alive(), "the app is not alive after the proof")
    # owner 2026-11-11: the close key (APPCI_KEY_CLOSE_WHEN_PROVEN, set by mac/ci/app_test.sh) is given to the rest of the run only by a proof that
    # PASSED, here, as its last step. A proof that failed, skipped or raised never reaches a grant, so every later case keeps its no-close-key path.
    key = R.proven_close_key(ctx.be, not r.fails)
    r.metric("close_key_after_proof", key)
    if os.environ.get(R.CLOSE_WHEN_PROVEN_ENV):
        r.note("close key for the cases after this one: %s" % (("%s (APPCI_KEY_CLOSE, given by this proof)" % key) if key else
                                                                 "none (the proof did not pass: APPCI_KEY_CLOSE stays empty)"))
