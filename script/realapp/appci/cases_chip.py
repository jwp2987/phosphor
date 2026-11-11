# appci/cases_chip.py: round 21, #2138 (owner decision 2026-11-11; docs/design/r21-chip.md). Registered with the same @case decorator as cases.py
# (one import line at its end). Pure parts are self-tested in selftest_r21_chip.py.
#
# A model's prompt suggestion arrives seconds after the command it is about. It used to be a banner the input grew for: the block list moved up by
# its height (78 px in the round 15 macOS run) and a click meant for the output landed on the banner and started an agent conversation. The rule: no
# visible output row moves when the suggestion appears or goes. The app now draws it as a chip at the right end of the editor's first line, a row the
# input already has (kill switch PHOSPHOR_PROMPT_SUGGESTION_CHIP=0: the banner again), and logs
#   input_order hop=prompt_chip shown=<0|1> hidden=<0|1> place=<chip|banner> drawn=<0|1> width=<px>      (Input::set_prompt_suggestions_banner_state)
#   input_order hop=prompt_chip_accept source=<Keybinding|Button>                                         (the suggestion was taken)
#
#   suggestion_chip_keeps_output         both modes, the pass's configured model. Whether a suggestion comes is the model's choice: none within the
#                                        wait is UNVERIFIABLE with that reason, never a pass. Terminal mode: Escape drops the chip (keyboard dismissal).
#   agent_suggestion_chip_keeps_output   the scripted model (PROVING_FAKE, agent mode): fake_model.py answers the prompt-suggestions request of the
#                                        block that names `suggestiontag_<TAG>` with "Explain chip <TAG>", 3 s late, so the chip comes on demand. Its
#                                        text is read from the screen (OCR) and a second chip is taken with the accept key.
#
# One body (play):
#   1. `cat` prints a file whose last line holds a word registered with the find hook (#1789): the app logs `appci_find token=.. x0= y0= x1= y1=`
#      where it draws the word, and again whenever that place changes.
#   2. wait for `hop=prompt_chip shown=1 hidden=0`. Every sighting of the word logged BEFORE that line is "before the suggestion"; every one after
#      it is "with the suggestion" (log order, no clock). The newest sighting of the same run of text (same left edge, right edge, height) must be
#      at the same place: a different y is the defect. No new sighting = the place did not change.
#   3. scripted model: the chip's text ("Explain chip <TAG>") is read from a screenshot. Read: the chip is on screen. Not read, no OCR engine, or
#      the app said it drew no chip (no room on the line): UNVERIFIABLE with what was read of the printed row (the control word of the same shot).
#   4. terminal mode: Escape drops the suggestion (`shown=0`); the same judgement of step 2 for the chip going. In the agent view Escape leaves the
#      view, a layout change of its own, so the going half is judged by the Rust view test only (a note says so).
#   5. control: one more command is printed. A new block moves the older rows in a pane whose input is at the bottom, and the hook must log the word
#      at another y. Without that line "no new sighting" proves nothing about this run: UNVERIFIABLE. A failed judgement of step 2 or 4 stands.
#   6. scripted model: the file is printed again (Enter dropped the first suggestion), the second chip is awaited and taken with the accept key
#      (Alt+Shift+Enter; Ctrl+Enter on macOS). `hop=prompt_chip_accept source=Keybinding` after the key: the key works. No such line: FAIL.
# No pointer. Positions and the accept come from the app's own log; only step 3 reads the screen.
import os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases  # noqa: E402,F401  (cases.py imports this module near its end, so everything below is already defined)
import cases_ui as U  # noqa: E402
import cases_pointer as P  # noqa: E402
import cases_caseagent as CAG  # noqa: E402
from cases import case  # noqa: E402

# ================================================================ pure parts (selftest_r21_chip.py)
CHIP_RE = re.compile(r"hop=prompt_chip shown=([01]) hidden=([01]) place=(\w+) drawn=([01]) width=(\d+)")
ACCEPT_RE = re.compile(r"hop=prompt_chip_accept source=(\w+)")
DEFAULT_WAIT_S = 45.0          # the round 15 run saw the suggestion 15 s after the command (the CI machine's model)
SCRIPTED_WAIT_S = 20.0         # the scripted model answers 3 s late
ACCEPT_WAIT_S = 5.0
SLACK_PX = 0.5                 # the hook logs to a tenth of a pixel; the same place is the same number


def chip_events(text):
    """pure: [(offset in text, shown, hidden, place, drawn, width px)] for every `hop=prompt_chip` line, oldest first"""
    return [(m.start(), m.group(1) == "1", m.group(2) == "1", m.group(3), m.group(4) == "1", int(m.group(5))) for m in CHIP_RE.finditer(text or "")]


def first_shown(events, after=-1):
    """pure: the first event after offset `after` of a suggestion that is set and not hidden, or None"""
    return next((e for e in events if e[0] > after and e[1] and not e[2]), None)


def first_gone(events, after):
    """pure: the first event after offset `after` that says no suggestion is set, or None"""
    return next((e for e in events if e[0] > after and not e[1]), None)


def accept_sources(text):
    """pure: the `source=` word of every `hop=prompt_chip_accept` line, oldest first"""
    return [m.group(1) for m in ACCEPT_RE.finditer(text or "")]


def find_rows(text, token):
    """pure: [(x0, y0, x1, y1)] in window pixels for every `appci_find` line of `token`, oldest first"""
    return [tuple(float(m.group(i)) for i in (2, 3, 4, 5)) for m in U.FIND_RE.finditer(text or "") if m.group(1) == token]


def same_run(a, b, slack=1.0):
    """pure: two sightings are the same run of text (same left edge, right edge and height): moved, or not, with its block. Another shape is
    another copy of the word (a tab title, a quote) and is never compared (cases_pointer.current_row, #2137)."""
    return abs(a[0] - b[0]) <= slack and abs(a[2] - b[2]) <= slack and abs((a[3] - a[1]) - (b[3] - b[1])) <= slack


def place_after(row, later):
    """pure: where the run of text first seen at `row` is drawn after the sightings `later` (oldest first): the newest of the same run, else `row`"""
    same = [h for h in later if same_run(row, h)]
    return same[-1] if same else row


def moved(a, b, slack=SLACK_PX):
    """pure: the vertical move from sighting a to sighting b in pixels, or 0.0 when it is the same place"""
    dy = b[1] - a[1]
    return dy if abs(dy) > slack else 0.0


def judge(main_text, ctl_text, token, mode, waited_s):
    """pure: -> dict(status="pass" | "fail" | "unverifiable", why=str, notes=[str], metrics={}).
    `main_text`: the app log from before the `cat` until just before the control command; `ctl_text`: the log after the control command was typed.
    fail: the printed row is drawn at another y after the suggestion appeared, or (terminal mode) after it went.
    unverifiable: no sighting before the suggestion, no suggestion within the wait, or the control saw no move."""
    out = dict(status="pass", why="", notes=[], metrics={})
    events = chip_events(main_text)
    shown = first_shown(events)
    cut = shown[0] if shown else len(main_text or "")
    before = find_rows((main_text or "")[:cut], token)
    if not before:
        out.update(status="unverifiable", why="the find hook logged no position of the printed row before the suggestion (an app without the hook, or the row "
                                               "was never drawn): there is nothing to compare")
        return out
    base = before[-1]
    out["metrics"]["row_before"] = list(base)
    if shown is None:
        out.update(status="unverifiable", why="no prompt suggestion was shown within %.0f s of the command (no `hop=prompt_chip shown=1 hidden=0` line; %d "
                                               "prompt_chip line(s) in all): the model offered no suggestion, so nothing moved and nothing was judged"
                                               % (waited_s, len(events)))
        return out
    place = shown[3]
    out["metrics"].update(place=place, drawn=shown[4], width_px=shown[5])
    gone = first_gone(events, shown[0])
    end_show = gone[0] if gone else len(main_text)
    showing = place_after(base, find_rows(main_text[shown[0]:end_show], token))
    out["metrics"]["row_showing"] = list(showing)
    fails = []
    dy = moved(base, showing)
    if dy:
        fails.append("the printed row moved %+.1f px (y %.1f -> %.1f) when the prompt suggestion appeared (place=%s): a click aimed at the output lands "
                     "elsewhere (#2138)" % (dy, base[1], showing[1], place))
    if mode == "terminal":
        if gone is None:
            out["notes"].append("Escape did not drop the suggestion (no `hop=prompt_chip shown=0` line after it): the suggestion going is not judged in this run")
        else:
            after = place_after(showing, find_rows(main_text[gone[0]:], token))
            out["metrics"]["row_gone"] = list(after)
            dy = moved(showing, after)
            if dy:
                fails.append("the printed row moved %+.1f px (y %.1f -> %.1f) when the prompt suggestion went (Escape; place=%s)" % (dy, showing[1], after[1], place))
    else:
        out["notes"].append("the suggestion going is not judged in the agent view: Escape leaves the view there, a layout change of its own (the Rust view test "
                            "a_late_prompt_suggestion_is_a_chip_that_moves_neither_the_output_nor_the_input covers it)")
    if fails:
        out.update(status="fail", why="; ".join(fails))
        return out
    last = place_after(base, find_rows(main_text[shown[0]:], token))
    ctl = [h for h in find_rows(ctl_text, token) if same_run(last, h) and moved(last, h)]
    out["metrics"]["control_moves"] = len(ctl)
    if not ctl:
        out.update(status="unverifiable", why="control: after one more command was printed below it, the find hook logged no new place for the row, so 'no new "
                                               "place when the suggestion appeared' proves nothing in this run (the hook stopped logging, the row left the "
                                               "screen, or older rows do not move in this layout)")
    return out


def chip_label(tag):
    """pure: the text the scripted model's suggestion is drawn with (fake_model.suggestion_reply_text)"""
    return "Explain chip %s" % tag


def judge_text(shown, flat, label, control):
    """pure: is the chip's text on screen? -> ("pass" | "unverifiable", why).
    `shown`: the chip_events row of the suggestion; `flat`: the OCR text of the screenshot taken while it showed (None: no OCR engine);
    `control`: a word of the printed output on the same screen. Never a fail: a text OCR did not read is not proof that it is not drawn; the
    reason says whether the same read found the control word, which is what tells a blind OCR from a missing chip."""
    if shown[3] != "chip":
        return "unverifiable", "the app drew the suggestion as a %s, not as the chip (place=%s: the kill switch PHOSPHOR_PROMPT_SUGGESTION_CHIP is off?): the chip's text was not looked for" % (shown[3], shown[3])
    if not shown[4]:
        return "unverifiable", "the app drew no chip for the suggestion (drawn=0 width=%d: no room on the editor's first line in this window), so there is no text to read" % shown[5]
    if flat is None:
        return "unverifiable", "no OCR engine on this machine: the chip's text %r was not read from the screen (the app logged drawn=1 width=%d)" % (label, shown[5])
    if CAG.fuzzy_has(flat, label):
        return "pass", ""
    saw = CAG.fuzzy_has(flat, control)
    return "unverifiable", ("OCR did not read the chip's text %r on the screen (the app logged drawn=1 width=%d); the same read %s the printed row's word %r%s"
                            % (label, shown[5], "found" if saw else "did not find", control,
                               ": the screen was readable, look at the chip_shown screenshot" if saw else ": OCR cannot read this screen"))


def judge_accept(text_after_key, key):
    """pure: did the accept key take the suggestion? -> ("pass" | "fail", why). `text_after_key`: the app log from the key on."""
    got = accept_sources(text_after_key)
    if "Keybinding" in got:
        return "pass", ""
    return "fail", ("the accept key (%s) did not take the prompt suggestion: no `hop=prompt_chip_accept source=Keybinding` line after it (accept lines: %r). "
                    "The chip was on screen and the key is its only keyboard action (#2138)" % (key, got))


def accept_key(backend_name):
    """pure: the default chord of terminal:accept_prompt_suggestions with the agent view feature on (app/src/terminal/view/init.rs)"""
    return "ctrl+Return" if backend_name == "mac" else "alt+shift+Return"


def worst(*statuses):
    """pure: fail beats unverifiable beats pass"""
    return "fail" if "fail" in statuses else "unverifiable" if "unverifiable" in statuses else "pass"


def fixture_text(tag, token):
    """pure: what the case prints. The last line is the row that is measured: the lowest output row, the one the banner used to take the place of.
    The lines above it are the kind of output a model offers a follow-up for (an error that names a file, a URL); the scripted model answers the
    `suggestiontag_<TAG>` word, which is also in the file's name and so in the command line."""
    return ("error[E0432]: unresolved import in src/%s.rs:140\n"
            "see https://192.0.2.7/%s for the build log\n"
            "suggestiontag_%s\n"
            "row %s\n" % (token, tag, tag, token))


# ================================================================ the cases
def wait_s(scripted):
    try:
        return float(os.environ.get("APPCI_CHIP_WAIT_S", "") or (SCRIPTED_WAIT_S if scripted else DEFAULT_WAIT_S))
    except ValueError:
        return SCRIPTED_WAIT_S if scripted else DEFAULT_WAIT_S


def play(ctx, r, scripted, fake=None):
    tag = U.ocr_safe(re.sub(r"[^A-Za-z0-9]", "", ctx.tok(8)).lower() or "tag")
    token = P.safe_tok(ctx, "bn")
    fname = "suggestiontag_%s.txt" % tag
    asked0 = fake.by_purpose.get("prompt_suggestions", 0) if fake is not None else 0
    seen = P.print_file(ctx, r, fname, fixture_text(tag, token), (token,))
    if seen is None:
        return
    ctx.until(lambda: find_rows(ctx.log_since(seen), token), 5, step=0.2)          # give-up net: judge() says so when the hook logged nothing
    limit = wait_s(scripted)
    t0 = time.time()
    up = ctx.until(lambda: first_shown(chip_events(ctx.log_since(seen))) is not None, limit, step=0.25)
    waited = time.time() - t0
    text_verdict = None
    if up:
        time.sleep(1.0)                    # the frame that draws the chip, and the hook's line if the row moved in it
        shown = first_shown(chip_events(ctx.log_since(seen)))
        if scripted:
            raw = ctx.ocr("chip_shown")    # takes the screenshot too
            if raw is None:
                ctx.shot("chip_shown")
            text_verdict = judge_text(shown, None if raw is None else " ".join(raw.split()), chip_label(tag), token)
        else:
            ctx.shot("chip_shown")
            r.note("the chip's text is not read from the screen with the pass's own model (its wording is the model's); agent_suggestion_chip_keeps_output "
                   "reads the scripted model's")
        if ctx.mode == "terminal":
            off = len(ctx.log_since(seen))
            ctx.be.key("escape")
            ctx.until(lambda: first_gone(chip_events(ctx.log_since(seen)), off - 1) is not None, 5, step=0.1)
            time.sleep(1.0)
            ctx.shot("chip_gone")
    else:
        ctx.shot("chip_none")
    ctl_off = ctx.log_size()
    main_text = ctx.log_since(seen)
    ctl_text = ""
    if up:
        ran = U.run_cmd(ctx, "echo bnctl_%s" % tag)
        r.check(ran or not ctx.prof.commands(), "the control command `echo bnctl_%s` did not reach the shell" % tag)
        ctx.until(lambda: find_rows(ctx.log_since(ctl_off), token), 5, step=0.2)
        time.sleep(0.5)
        ctl_text = ctx.log_since(ctl_off)
    v = judge(main_text, ctl_text, token, ctx.mode, waited)
    r.metric("chip", dict(v["metrics"], waited_s=round(waited, 1), scripted=bool(scripted)))
    for n in v["notes"]:
        r.note(n)
    whys = [v["why"]] if v["status"] != "pass" else []
    statuses = [v["status"]]
    if text_verdict is not None:
        r.metric("chip_text", text_verdict[0])
        statuses.append(text_verdict[0])
        if text_verdict[0] != "pass":
            whys.append(text_verdict[1])
    if scripted and up:
        accept = play_accept(ctx, r, fname, tag, token, limit)
        r.metric("chip_accept", accept[0])
        statuses.append(accept[0])
        if accept[0] != "pass":
            whys.append(accept[1])
    if fake is not None:
        asked = fake.by_purpose.get("prompt_suggestions", 0) - asked0
        r.metric("prompt_suggestion_requests", asked)
        if not up:
            whys.append("the scripted model received %d prompt-suggestions request(s) since the command; it answers the one whose last block names suggestiontag_%s" % (asked, tag))
    status = worst(*statuses)
    if status == "fail":
        r.check(False, "; ".join(whys))
    elif status == "unverifiable":
        r.unverifiable("; ".join(whys))
    r.check(ctx.app.alive(), "the app died while the prompt suggestion was shown")
    ctx.reset()


def play_accept(ctx, r, fname, tag, token, limit):
    """step 6: print the file again, wait for the second chip, press the accept key -> ("pass" | "fail" | "unverifiable", why)"""
    off = ctx.log_size()
    mark = ctx.cmd_mark()
    ran = U.run_cmd(ctx, "cat %s" % fname)
    if ctx.prof.commands() and not (ran and ctx.cmds_since(mark)[-1:] == ["cat %s" % fname]):
        return "unverifiable", "the second `cat %s` did not reach the shell as typed (received %r): the accept key was not pressed" % (fname, ctx.cmds_since(mark))
    if not ctx.until(lambda: first_shown(chip_events(ctx.log_since(off))) is not None, limit, step=0.25):
        ctx.shot("chip_second_none")
        return "unverifiable", ("no second prompt suggestion within %.0f s of printing the file again (no `hop=prompt_chip shown=1 hidden=0` line): there was "
                                "nothing for the accept key to take" % limit)
    time.sleep(0.5)
    key = accept_key(ctx.be.name)
    key_off = ctx.log_size()
    ctx.be.key(key)
    ctx.until(lambda: accept_sources(ctx.log_since(key_off)), ACCEPT_WAIT_S, step=0.1)
    time.sleep(0.5)
    ctx.shot("chip_accepted")
    return judge_accept(ctx.log_since(key_off), key)


@case("suggestion_chip_keeps_output", order=73, gui=True)
def c_suggestion_chip_keeps_output(ctx, r):
    """#2138. A command prints output; when the model's prompt suggestion arrives (a chip on the editor's first line) the lowest output row is drawn
    where it was, and (terminal mode) again when Escape drops the suggestion. Positions from the find hook, the suggestion from `hop=prompt_chip`.
    No suggestion within the wait (APPCI_CHIP_WAIT_S, 45 s): UNVERIFIABLE."""
    play(ctx, r, scripted=False)


@case("agent_suggestion_chip_keeps_output", modes=("agent",), order=73, gui=True)
def c_agent_suggestion_chip_keeps_output(ctx, r):
    """#2138, scripted model: the same judgement with a suggestion on demand (fake_model.suggestion_tag answers the block that names the tag), the
    chip's text read from the screen, and a second chip taken with the accept key."""
    import fake_model as FM
    fake = FM.ACTIVE
    if fake is None:
        r.skip("NEEDS: the scripted fake model as this pass's provider (the fix tier's agent@fake pass, or APPCI_FAKE_MODEL=1): fake_model.ACTIVE is unset")
    play(ctx, r, scripted=True, fake=fake)
