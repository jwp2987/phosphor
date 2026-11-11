# appci/cases_r15_defects.py: round 15, the app cases for the defects the round 14 cases found. Design note: docs/design/r15-defects.md.
# Both are in cases.PROVING (owner rule 2026-11-09: a new case runs in the fix tier; graduation is the owner's).
#   scroll_keys_foreground        #2080. A program runs in the FOREGROUND and reads the terminal raw while a job prints ten lines a second.
#                                 Control: plain PageUp reaches the program (ESC [ 5 ~) and the app logs no scrollback key. Then Shift+PageUp three
#                                 times, Shift+Home, Shift+PageDown, Shift+End: the app logs `hop=scrollback_key key=...` for each and the program
#                                 receives NOTHING; where the pane has an OCR engine the view moved up and held its place while output arrived, and
#                                 Shift+End brought the newest lines back. One typed character then reaches the program. Second half: the same
#                                 program in the alternate screen receives Shift+PageUp and Shift+Home as bytes and the app logs no scrollback key.
#                                 Round 16 (owner decision 2026-11-11: the shifted keys also scroll at a prompt), first half of the case: at
#                                 the PROMPT, with an empty input, each of the four shifted keys logs its hop; with text in the input
#                                 Shift+Home selects it instead (no hop; the line typed over the selection is the one the shell receives).
#                                 Wayland / X11 / macOS / the Windows Git Bash pane (the PowerShell pane has no POSIX raw reader).
#   primary_absent_clipboard_kept #2081. On a display with NO primary selection: copy on select off, a drag over two words leaves CLIPBOARD holding
#                                 its sentinel; control with copy on select on: CLIPBOARD holds the selection (the drag selects, the setting is
#                                 live). On a display that has a primary selection the case SKIPs with the machine change it needs (the split is
#                                 judged there by primary_and_middle_click).
# Same rules as cases.py: typing at 10-25 ms per key with an instant Enter, a case that cannot run SKIPS with the capability that is missing, and
# a verdict that rests on evidence that is absent is NOREF, never a pass.
import os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases as C  # noqa: E402,F401  (cases.py imports this module as its last line, so everything below is already defined)
import cases_pointer as P  # noqa: E402
import cases_r14_w1a as W1  # noqa: E402
import cases_robust as R  # noqa: E402
import cases_ui as U  # noqa: E402
import cases_wrap_vtabs as WV  # noqa: E402
from cases import case  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
ISSUE_SCROLL_KEYS = 2080
ISSUE_PRIMARY_FALLBACK = 2081

# ---------------------------------------------------------------- #2080 scroll_keys_foreground
SCROLLBACK_HOP_RE = re.compile(r"hop=scrollback_key key=(page_up|page_down|top|bottom)\b")     # app/src/terminal/view/scrollback_keys.rs scrollback_key_hop_detail
SFG_LINES = 300
SFG_STREAM_LINES = 1500                # ten a second: longer than the case; the case stops the job itself
SFG_NEAR_BOTTOM = 15                   # a stream line this close to the newest one printed counts as "the bottom is on screen"


def scrollback_hops(text):
    """pure: the key of every `hop=scrollback_key` line of an app-log excerpt, in order"""
    return SCROLLBACK_HOP_RE.findall(text or "")


def nav_key(backend_name, name, shift=False):
    """pure: a navigation key as this backend's injector spells it (PageUp / PageDown are the X keysyms Prior / Next for wtype and xdotool)"""
    linux = backend_name in ("wayland", "x11")
    k = {"pageup": "Prior" if linux else "pageup", "pagedown": "Next" if linux else "pagedown", "home": "Home", "end": "End"}[name]
    return ("shift+" + k) if shift else k


def sfg_script(reader_argv, stream, seen, stop, out, secs=150):
    """pure: the bash script the case runs in the foreground. `$1` = main: a background job prints one line of `stream` every 100 ms (CR LF: the
    reader has the tty raw) to the pane and to `seen` until `stop` exists; `$1` = alt: no job, and the reader runs in the alternate screen. The
    reader (`reader_argv`, bytes_reader.py or its bash twin) logs every byte the pty hands it to `out`. `trap : INT`: the bash twin ends by SIGINT
    (its tty mode keeps ISIG), which reaches this script too; with a handler the script goes on, leaves the alternate screen and stops the job."""
    return ("#!/bin/bash\n"
            "trap : INT\n"
            "if [ \"$1\" = main ]; then\n"
            "  (sleep 1; while read -r l; do [ -e %(stop)s ] && break; printf '%%s\\r\\n' \"$l\"; echo \"$l\" >> %(seen)s; sleep 0.1; done < %(stream)s) &\n"
            "else\n"
            "  printf '\\033[?1049h'\n"
            "fi\n"
            "%(reader)s %(out)s %(secs)d\n"
            "[ \"$1\" = main ] || printf '\\033[?1049l'\n"
            ": > %(stop)s\n"
            "wait\n") % dict(stop=stop, seen=seen, stream=stream, reader=reader_argv, out=out, secs=secs)


def sfg_scroll_verdict(want, n, hops, got):
    """pure: failure text or None for a key that must scroll the scrollback: the app logged exactly `n` `hop=scrollback_key key=<want>` lines for it
    and the running program received no byte"""
    if got:
        return "the running program received %s: the key was written to the pty, not used to scroll" % got.hex()
    if hops != [want] * n:
        return "the app logged %r for it, expected %d x %r (`hop=scrollback_key`): the key did not reach the scrollback binding" % (hops, n, want)
    return None


def sfg_prompt_verdict(want, n, hops):
    """pure: failure text or None for a shifted key pressed at a PROMPT with an empty input: the app logged exactly `n` `hop=scrollback_key
    key=<want>` lines for it (round 16; before it the keys did nothing at a prompt and the app logged none)"""
    if hops != [want] * n:
        return "the app logged %r for it, expected %d x %r (`hop=scrollback_key`): the key did not scroll at the prompt" % (hops, n, want)
    return None


def sfg_select_verdict(hops, got, want):
    """pure: failure text or None for Shift+Home pressed in an input that HOLDS TEXT: it selects to the line start (the editor's key, as always),
    so the app logs no scrollback key and the text typed next REPLACES the selection: the shell receives exactly `want`"""
    if hops:
        return "the app scrolled the scrollback (%r) although the input held text: Shift+Home must select there" % hops
    if got != [want]:
        return "the shell received %r, expected exactly %r (the typed line replaces the selected one)" % (got[:2], want)
    return None


def sfg_program_verdict(prefix, hops, got, exact=None):
    """pure: failure text or None for a key that belongs to the running program: it received bytes starting with `prefix` (exactly `exact` when
    given) and the app logged no scrollback key"""
    if hops:
        return "the app scrolled the scrollback for it (%r) instead of leaving the key to the program" % hops
    if not got:
        return "the running program received nothing"
    if exact is not None and got != exact:
        return "the running program received %s, expected %s" % (got.hex(), exact.hex())
    if not got.startswith(prefix):
        return "the running program received %s, expected bytes starting with %s" % (got.hex(), prefix.hex())
    return None


def sfg_view_verdict(newest_seen, printed, want_bottom):
    """pure: ('pass' | 'fail' | 'noref', text) for where the view is, from the highest stream line number OCR read on screen (None: none read) and
    the number of lines the job has printed. want_bottom: the newest lines must be on screen; else they must NOT be (the view is scrolled up)."""
    at_bottom = newest_seen is not None and newest_seen >= printed - SFG_NEAR_BOTTOM
    if want_bottom:
        if newest_seen is None:
            return "noref", "OCR read none of the job's lines on a screen that should show the newest of %d" % printed
        return ("pass", "") if at_bottom else ("fail", "the newest job line on screen is %d of %d printed: the view is not at the bottom" % (newest_seen, printed))
    if at_bottom:
        return "fail", "the job's line %d of %d printed is on screen: the view is at the bottom, it did not scroll up (or it followed the output)" % (newest_seen, printed)
    return "pass", ""


def _sfg_press(ctx, rf, off, spec, n=1, want_bytes=0, want_hops=0):
    """press `spec` n times -> (bytes the reader logged for it, scrollback hops the app logged for it). Waits for what the caller expects (a give-up
    net), then a settle so a byte or a hop that should NOT come still has time to."""
    b0 = len(b"".join(U._reader_chunks(ctx, rf)))
    h0 = len(scrollback_hops(ctx.log_since(off)))
    ctx.be.keys([spec] * n, delay_ms=60)
    ctx.until(lambda: len(b"".join(U._reader_chunks(ctx, rf))) - b0 >= want_bytes and len(scrollback_hops(ctx.log_since(off))) - h0 >= want_hops, 4)
    time.sleep(0.6)
    return b"".join(U._reader_chunks(ctx, rf))[b0:], scrollback_hops(ctx.log_since(off))[h0:]


def _sfg_prompt_press(ctx, off, spec, n=1):
    """press `spec` n times at the prompt -> the scrollback hops the app logged for it (a give-up net, then a settle)"""
    h0 = len(scrollback_hops(ctx.log_since(off)))
    ctx.be.keys([spec] * n, delay_ms=60)
    ctx.until(lambda: len(scrollback_hops(ctx.log_since(off))) - h0 >= n, 4)
    time.sleep(0.5)
    return scrollback_hops(ctx.log_since(off))[h0:]


def _sfg_prompt_half(ctx, r, off, key, rows):
    """round 16: the shifted keys at a PROMPT. Empty input: all four scroll (one hop each). Input with text: Shift+Home selects, and the line typed
    over the selection is what the shell receives."""
    for label, name, want, n in (("Shift+PageUp x2", "pageup", "page_up", 2), ("Shift+Home", "home", "top", 1), ("Shift+PageDown", "pagedown", "page_down", 1),
                                 ("Shift+End", "end", "bottom", 1)):
        hops = _sfg_prompt_press(ctx, off, key(name, True), n)
        err = sfg_prompt_verdict(want, n, hops)
        rows.append(dict(key="prompt " + label, hops=hops))
        r.check(err is None, "%s at a prompt with an empty input: %s (#%d)" % (label, err, ISSUE_SCROLL_KEYS))
    ctx.shot("sfg_p_prompt_keys")
    # text in the input: Shift+Home is the editor's (select to the line start)
    tok = ctx.tok(5).lower()
    replaced, want = "echo sfgold%s" % tok, "echo sfgnew%s" % tok
    m = ctx.cmd_mark()
    W1.typed(ctx, replaced, enter=False)
    hops = _sfg_prompt_press(ctx, off, key("home", True), 0)
    W1.typed(ctx, want)
    ctx.until(lambda: len(ctx.cmds_since(m)) >= 1, 6)
    got = ctx.cmds_since(m)
    err = sfg_select_verdict(hops, got, want)
    rows.append(dict(key="prompt Shift+Home with text", hops=hops, got=got[:2]))
    r.check(err is None, "Shift+Home at a prompt with text in the input: %s" % err)
    ctx.reset()


def _sfg_start(ctx, r, arm, rf):
    """run the script's `arm` in the foreground and wait for the reader -> True when it listens"""
    ctx.rm(rf, rf + ".ready")
    W1.typed(ctx, "bash sfg_run.sh %s" % arm)
    if not ctx.until(lambda: os.path.exists(os.path.join(ctx.W, rf + ".ready")), 20):
        r.check(False, "[%s] the foreground reader never started (`bash sfg_run.sh %s`)" % (arm, arm))
        return False
    time.sleep(0.5)
    return True


def _sfg_end(ctx):
    ctx.be.key("ctrl+c")                           # ends the reader (byte 0x03; SIGINT for the bash twin)
    time.sleep(C.Ctx.EXIT_WINDOW_S + 0.4)          # past the agent view's exit-confirmation window before reset() sends its own Ctrl-C
    ctx.reset()


@case("scroll_keys_foreground", order=67, gui=True)
def c_scroll_keys_foreground(ctx, r):
    """#2080: at a prompt (round 16) and while a command runs in the foreground Shift+PageUp / Shift+PageDown / Shift+Home / Shift+End scroll the
    scrollback; under the command the plain PageUp is the program's; a full-screen program keeps the shifted keys too. At the prompt an input
    that holds text keeps Shift+Home (it selects). The program is a raw reader that logs every byte the pty hands it,
    with a job printing ten lines a second next to it. Verdicts: the reader's bytes (a scroll key gives none, a program key gives its sequence),
    the app's `hop=scrollback_key` lines (exactly one per scroll key, none for a program key), and where the pane has an OCR engine the view
    itself (scrolled up and held under new output; the newest lines back after Shift+End)."""
    if ctx.pane == "powershell":
        r.skip("the foreground program is a POSIX raw reader started from a bash script: runs in the Git Bash pane")
    W1.need_app_log(ctx, r, "the app's scrollback-key hop")
    if not ctx.win and not os.path.exists("/bin/bash") and not os.path.exists("/usr/bin/bash"):
        r.skip("NEEDS: bash on this machine (the foreground script is bash)")
    tag, stag = W1.skp_tags(ctx.rng)
    src, stream, seen, stop, rf = "sfg_lines.txt", "sfg_stream.txt", "sfg_seen.txt", "sfg_stop", "sfg_bytes.txt"
    ctx.rm(src, stream, seen, stop, rf, rf + ".ready", "sfg_run.sh")
    with open(os.path.join(ctx.W, src), "w", newline="\n") as f:
        f.write("".join("%s %d\n" % (W1.skp_word(tag, i), i) for i in range(1, SFG_LINES + 1)))
    with open(os.path.join(ctx.W, stream), "w", newline="\n") as f:
        f.write("".join("%s\n" % W1.skp_word(stag, i) for i in range(1, SFG_STREAM_LINES + 1)))
    reader = ('bash "%s"' % os.path.join(HERE, "bytes_reader.sh").replace("\\", "/")) if ctx.win else ('python3 "%s"' % os.path.join(HERE, "bytes_reader.py"))
    with open(os.path.join(ctx.W, "sfg_run.sh"), "w", newline="\n") as f:
        f.write(sfg_script(reader, stream, seen, stop, rf))
    printed = lambda: len(ctx.lines(seen))
    newest = lambda text: (W1.skp_visible(text, stag) or [None])[-1]
    ctx.reset()
    off = ctx.log_size()
    key = lambda name, shift=False: nav_key(ctx.be.name, name, shift)
    rows = []

    def scroll(label, name, want, n=1):
        got, hops = _sfg_press(ctx, rf, off, key(name, True), n, want_hops=n)
        err = sfg_scroll_verdict(want, n, hops, got)
        rows.append(dict(key=label, bytes=got.hex(), hops=hops))
        return r.check(err is None, "%s while a command runs in the foreground: %s (#%d)" % (label, err, ISSUE_SCROLL_KEYS))

    try:
        W1.typed(ctx, "cat %s" % src)
        time.sleep(1.5)
        _sfg_prompt_half(ctx, r, off, key, rows)
        if not _sfg_start(ctx, r, "main", rf):
            return
        if not r.check(bool(ctx.until(lambda: printed() >= 5, 15)), "the job printed %d line(s) within 15 s: there is no output to scroll back through" % printed()):
            return
        ocr0 = None if ctx.win else ctx.ocr("sfg_0_bottom")
        have_ocr = ocr0 is not None
        if not have_ocr:
            r.note("the view's position was not read from the screen (NEEDS: an OCR engine on this runner: tesseract-ocr on Linux and Windows, Apple "
                   "Vision on macOS); the scroll keys are judged by the app's hop and the program's bytes")
        else:
            v, why = sfg_view_verdict(newest(ocr0), printed(), True)
            if v == "noref":
                have_ocr = False
                r.note("control: %s (shots/*sfg_0_bottom.png); the view's position is not judged in this run" % why)
            else:
                r.check(v == "pass", "control, before any key: %s (shots/*sfg_0_bottom.png)" % why)
        # control: the plain key is the program's
        got, hops = _sfg_press(ctx, rf, off, key("pageup"), 1, want_bytes=4)
        err = sfg_program_verdict(b"\x1b[5", hops, got, exact=None if ctx.win else b"\x1b[5~")
        rows.append(dict(key="PageUp", bytes=got.hex(), hops=hops))
        if not r.check(err is None, "control, plain PageUp while a command runs in the foreground: %s" % err):
            return
        # Shift+PageUp three times: the view goes up and stays there while the job prints
        scroll("Shift+PageUp x3", "pageup", "page_up", 3)
        if have_ocr:
            t1, n1 = ctx.ocr("sfg_1_up"), printed()
            v, why = sfg_view_verdict(newest(t1), n1, False)
            r.check(v == "pass", "after Shift+PageUp three times: %s (shots/*sfg_1_up.png)" % why)
            time.sleep(3.0)
            t2, n2 = ctx.ocr("sfg_2_up_held"), printed()
            kept = W1.anchors_kept(t1, t2)
            r.metric("sfg_hold", dict(new_lines=n2 - n1, anchor_words=len(W1.anchor_words(t1)), kept=round(kept, 2), newest_on_screen=newest(t2)))
            if n2 - n1 < 10:
                r.check(False, "the job printed %d line(s) in 3 s, expected about 30: no output arrived, so holding the view proves nothing" % (n2 - n1))
            elif len(W1.anchor_words(t1)) < 5:
                r.unverifiable("after Shift+PageUp OCR read %d word(s) to compare (shots/*sfg_1_up.png): whether the view held is not judged" % len(W1.anchor_words(t1)))
            else:
                held = kept >= W1.SKP_KEPT_MIN and (newest(t2) is None or newest(t2) <= n1)
                r.check(held, "after Shift+PageUp, 3 s of new output: %d%% of the words on screen are still there, newest job line on screen %s (%d were "
                        "printed when the key was pressed): the view moved (shots/*sfg_1_up.png, *sfg_2_up_held.png)" % (int(kept * 100), newest(t2), n1))
        scroll("Shift+Home", "home", "top")
        scroll("Shift+PageDown", "pagedown", "page_down")
        scroll("Shift+End", "end", "bottom")
        if have_ocr:
            time.sleep(0.6)
            v, why = sfg_view_verdict(newest(ctx.ocr("sfg_3_end")), printed(), True)
            if v == "noref":
                r.unverifiable("after Shift+End: %s (shots/*sfg_3_end.png)" % why)
            else:
                r.check(v == "pass", "after Shift+End: %s (shots/*sfg_3_end.png)" % why)
        # a typed character is the program's, and it ends a scroll
        scroll("Shift+PageUp", "pageup", "page_up")
        b0 = len(b"".join(U._reader_chunks(ctx, rf)))
        W1.typed(ctx, "x", enter=False)
        ctx.until(lambda: len(b"".join(U._reader_chunks(ctx, rf))) > b0, 4)
        time.sleep(0.6)
        got = b"".join(U._reader_chunks(ctx, rf))[b0:]
        r.check(got == b"x", "one typed `x` after the scroll keys: the running program received %s, expected 78" % (got.hex() or "nothing"))
        if have_ocr:
            v, why = sfg_view_verdict(newest(ctx.ocr("sfg_4_after_key")), printed(), True)
            if v == "noref":
                r.unverifiable("after the typed key: %s (shots/*sfg_4_after_key.png)" % why)
            else:
                r.check(v == "pass", "after one typed key: %s (shots/*sfg_4_after_key.png)" % why)
        r.metric("sfg_keys", rows)
        R.touch(ctx, stop)
        _sfg_end(ctx)

        # a full-screen program keeps the shifted keys
        rows = []
        if not _sfg_start(ctx, r, "alt", rf):
            return
        for label, name, prefix, exact in (("Shift+PageUp", "pageup", b"\x1b[5", None), ("Shift+Home", "home", b"\x1b[", None if ctx.win else b"\x1b[1;2H")):
            got, hops = _sfg_press(ctx, rf, off, key(name, True), 1, want_bytes=len(exact) if exact else len(prefix) + 1)
            err = sfg_program_verdict(prefix, hops, got, exact=exact)
            rows.append(dict(key=label, bytes=got.hex(), hops=hops))
            r.check(err is None, "%s with a full-screen program in front (alternate screen): %s" % (label, err))
        r.metric("sfg_alt_keys", rows)
        ctx.shot("sfg_5_alt")
        _sfg_end(ctx)
    finally:
        R.touch(ctx, stop)
        WV.back_to_input(ctx, r)
        ctx.reset()
        ctx.rm(src, stream, seen, stop, rf, rf + ".ready", "sfg_run.sh")


# ---------------------------------------------------------------- #2081 primary_absent_clipboard_kept
NEEDS_NO_PRIMARY = ("NEEDS: a display WITHOUT a primary selection: a Wayland session whose compositor does not offer the primary-selection protocol "
                    "(zwp_primary_selection_device_manager_v1 and the wlr data-control primary selection), for example a second CI session under "
                    "`weston --no-config` or a wlroots compositor built without it. On this display PRIMARY works, so the defect of #2081 cannot "
                    "occur; the PRIMARY / CLIPBOARD split is judged here by primary_and_middle_click")


def no_primary_verdict(on, clip, sentinel, line, a0, b1):
    """pure: failure text or None for CLIPBOARD after a drag over line[a0:b1] on a display with no primary selection. copy on select off: CLIPBOARD
    still holds `sentinel` (#2081: the selection must not be written there in PRIMARY's place). On: it holds the selection (the control: the drag
    selected text and the setting is live)."""
    if not on:
        if clip == sentinel:
            return None
        return ("CLIPBOARD holds %r, it held %r: the selection was written to the clipboard although copy on select is off (the fallback for a "
                "display with no primary selection, #%d)" % (clip[:60], sentinel, ISSUE_PRIMARY_FALLBACK))
    if clip == sentinel:
        return "CLIPBOARD still holds its sentinel with copy on select on: the drag selected nothing or the setting was not applied live"
    return WV.judge_drag_copy(clip, line, a0, b1)


@case("primary_absent_clipboard_kept", order=72, gui=True)
def c_primary_absent_clipboard_kept(ctx, r):
    """#2081: on a display with no primary selection, a text selection with copy on select OFF leaves CLIPBOARD alone; with it ON the selection is
    there (control). SKIPs, naming the machine change, on a display whose primary selection works."""
    m = P.linux_pointer_or_skip(ctx, r, "primary_absent_clipboard_kept")
    U.need_tesseract(r)
    p = U.settings_path(ctx)
    if not os.path.isfile(p):
        r.skip("settings file %s not found" % p)
    raw = open(p, "rb").read()
    why = P.cos_profile_block(raw.decode("utf-8", errors="replace"))
    if why:
        r.skip(why)
    probe = "PRIMARY_PROBE_%s" % ctx.tok(5)
    P.primary_set(ctx, probe)
    time.sleep(0.3)
    if P.primary_get(ctx) == probe:
        r.skip(NEEDS_NO_PRIMARY)
    words = P.distinct_words(ctx.rng, 4)
    line, tok = " ".join(words), ctx.tok(5).lower()
    seen = P.print_file(ctx, r, "pa_%s.txt" % tok, line + "\n", words)
    if seen is None:
        return
    try:
        found = P.locate_many(ctx, r, "pa_find", words, seen)
        if not P.need_all(ctx, r, found, words, "pa_find"):
            return
        a0, b1 = P.word_spans(line)[words[1]][0], P.word_spans(line)[words[2]][1]
        for on in (True, False):                              # the control first: a drag that selects nothing would pass the `off` arm
            arm = "on" if on else "off"
            U.set_setting(ctx, "terminal", "copy_on_select", "true" if on else "false")
            time.sleep(2.0)                                   # the live settings reload (ui_settings_live)
            sent = "SENTINEL_CLIP_%s" % arm
            ctx.be.clip_set(sent)
            m.drag(found[words[1]][3] + 1, found[words[1]][2], found[words[2]][4] - 1, found[words[2]][2])
            time.sleep(0.8)
            clip = P.clip_now(ctx)
            ctx.shot("pa_drag_%s" % arm)
            err = no_primary_verdict(on, clip, sent, line, a0, b1)
            ok = r.check(err is None, "[no primary selection, copy_on_select %s] drag over %r..%r: %s" % (arm, words[1], words[2], err))
            WV.back_to_input(ctx, r)
            if on and not ok:
                r.note("the control arm failed: the copy_on_select off arm was not run (a drag that selects nothing would pass it)")
                break
    finally:
        with open(p, "wb") as f:
            f.write(raw)
        time.sleep(1.5)
        WV.back_to_input(ctx, r)
