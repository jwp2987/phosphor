# appci/selftest_r21_relaynote.py: offline self-tests of round 21, the two notes of a Phosphorize click that waits behind a relay (#2142, owner
# decision 2026-11-11; docs/design/r10-r21-relaynote.md): how subshell_ready_relay_shape reads a note (screen or the app's hop line), when it is
# FAIL / UNVERIFIABLE / not judged, and that the words and hop names it looks for are the ones the Rust source has. Pure Python: no app, no display.
# selftest.py appends CHECKS to its own list (two lines at its end), so `run.py --self-test` runs them.
# Every judge is fed a right input AND the wrong ones it exists to catch: an oracle that cannot fail is not a test.
import inspect, os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.join(HERE, "..", "..", "..")
sys.path.insert(0, HERE)

CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


def _src(*parts):
    return open(os.path.join(REPO, *parts), encoding="utf-8").read()


@check
def r21_relay_note_is_read_from_the_screen_or_the_hop_and_can_fail():
    """relay_note_state: the words on screen = read (OCR confusions tolerated); the hop alone = logged; neither with the readiness hops in the log =
    absent (the case FAILS); neither and no readiness hop at all = unreadable (UNVERIFIABLE)"""
    import cases  # noqa: F401
    import cases_subshell_ready as SR
    w = SR.RELAY_WAIT_WORDS
    screen = "root@box:~# Phosphor will set up this shell at the next empty prompt: press Enter on an empty line. Running a command first cancels"
    assert SR.relay_note_state(True, True, screen, w) == "read"
    assert SR.relay_note_state(True, False, screen, w) == "read", "the screen alone is proof"
    assert SR.relay_note_state(True, True, screen.replace("Enter on an empty", "Enler on an emply"), w) == "read", "two OCR confusions"
    assert SR.relay_note_state(True, True, None, w) == "logged", "no OCR engine: the hop names the note"
    assert SR.relay_note_state(True, True, "root@box:~# ", w) == "logged", "the screen read missed the toast"
    old = "root@box:~# Phosphor setup starts when the shell shows its prompt."
    assert SR.relay_note_state(True, False, old, w) == "absent", "the old toast is not the note"
    assert SR.relay_note_state(True, False, None, w) == "absent", "the log has the readiness hops and not the note's: the app did not show it"
    assert SR.relay_note_state(False, False, None, w) == "unreadable"
    assert SR.relay_note_state(False, False, "root@box:~# ", w) == "unreadable"
    c = SR.RELAY_CANCEL_WORDS
    assert SR.relay_note_state(True, False, "Phosphor setup cancelled: you ran a command before it started. To try again, click", c) == "read"
    assert SR.relay_note_state(True, False, screen, c) == "absent", "the wait note is not the cancel note"


@check
def r21_relay_note_is_judged_only_for_the_wait_it_is_shown_for():
    """relay_waits_for_prompt: the look AT THE ARMING decides (Wait(RelayNoPrompt)); a later look, another wait, a write or no look at all do not"""
    import cases  # noqa: F401
    import cases_subshell_ready as SR
    armed = "t hop=subshell_ready_judged source=armed decision=Wait(RelayNoPrompt) kind=Relay leader=Relay comm=\"sudo\" tail=ShellPrompt"
    assert SR.relay_waits_for_prompt(SR.hops_in("t hop=subshell_setup_armed trigger=banner\n" + armed + "\n"))
    for other in ("decision=Wait(LineDirty)", "decision=Wait(RelayCooked)", "decision=Write(None)", "decision=Wait(Unknown)"):
        assert not SR.relay_waits_for_prompt(SR.hops_in(armed.replace("decision=Wait(RelayNoPrompt)", other) + "\n")), other
    later = armed.replace("source=armed", "source=output")
    assert not SR.relay_waits_for_prompt(SR.hops_in(later + "\n")), "only the look at the arming"
    assert not SR.relay_waits_for_prompt(SR.hops_in(armed.replace("RelayNoPrompt", "Unknown") + "\n" + later + "\n")), "the FIRST look at the arming"
    assert not SR.relay_waits_for_prompt([])


@check
def r21_relay_note_hops_are_kept_and_found():
    """hops() keeps the two new hop names (a name missing from HOPS can never be found: round 10), and first_index tells the notes and their events
    apart"""
    import cases  # noqa: F401
    import cases_subshell_ready as SR
    assert "subshell_relay_note" in SR.HOPS and "subshell_setup_reoffered" in SR.HOPS
    log = ("t [DEBUG] input_order hop=subshell_setup_armed trigger=banner t=1\n"
           "t [DEBUG] input_order hop=subshell_relay_note note=armed_wait event=shown t=2\n"
           "t [DEBUG] input_order hop=subshell_relay_note note=armed_wait event=gone why=cancelled t=3\n"
           "t [DEBUG] input_order hop=subshell_setup_cancelled why=UserDrove trigger=banner bytes=5 lines=1 t=4\n"
           "t [DEBUG] input_order hop=subshell_relay_note note=cancelled event=shown reoffer=true t=5\n"
           "t [DEBUG] input_order hop=subshell_setup_reoffered offered=true t=6\n")
    hs = SR.hops_in(log)
    assert [h for h, _ in hs] == ["subshell_setup_armed", "subshell_relay_note", "subshell_relay_note", "subshell_setup_cancelled",
                                  "subshell_relay_note", "subshell_setup_reoffered"], hs
    assert SR.first_index(hs, "subshell_relay_note", "note=armed_wait event=shown") == 1
    assert SR.first_index(hs, "subshell_relay_note", "note=armed_wait event=gone why=cancelled") == 2
    assert SR.first_index(hs, "subshell_relay_note", "note=armed_wait event=gone why=written") is None
    assert SR.first_index(hs, "subshell_relay_note", "note=cancelled event=shown") == 4
    assert SR.first_index(hs, "subshell_setup_reoffered", "offered=true") == 5
    assert SR.first_index(SR.hops_in(log.replace("offered=true", "offered=false")), "subshell_setup_reoffered", "offered=true") is None


@check
def r21_relay_note_words_and_hops_are_the_apps():
    """the words the case looks for on screen and the hop fields it looks up are in the Rust source that shows the notes; the kill switch and the
    rule the notes leave alone are there too"""
    import cases  # noqa: F401
    import cases_subshell_ready as SR
    note = _src("app", "src", "terminal", "view", "subshell_relay_note.rs")
    view = _src("app", "src", "terminal", "view.rs")
    assert SR.RELAY_WAIT_WORDS in note and SR.RELAY_CANCEL_WORDS in note
    assert 'ARMED_WAIT_NOTE: &str = "armed_wait"' in note and 'CANCELLED_NOTE: &str = "cancelled"' in note
    assert '"PHOSPHOR_SUBSHELL_RELAY_NOTE"' in note
    assert "wait == Some(WaitWhy::RelayNoPrompt)" in note
    for field in ('log_hop("subshell_relay_note"', '"note={} event=shown"', '"note={} event=gone why={why}"', '"note={} event=shown reoffer={reoffer}"',
                  'log_hop("subshell_setup_reoffered"', 'format!("offered={same_block}")',
                  'dismiss_subshell_relay_wait_note("written", ctx)', 'dismiss_subshell_relay_wait_note("cancelled", ctx)'):
        assert field in view, "view.rs no longer has %s" % field
    rules = _src("app", "src", "terminal", "view", "subshell_ready.rs")
    assert "Decision::Wait(WaitWhy::RelayNoPrompt)" in rules and "_ => LineEndVerdict::Cancel," in rules, "the judge's rules are not the notes' to change"


@check
def r21_relay_case_reads_both_notes():
    """subshell_ready_relay_shape: the wait note is read BEFORE the Enter on an empty line (while it shows) and only for a look that waits for a
    prompt; its removal is required with the write; the cancel leg runs after the first leg left its shell, never on top of a failed first leg,
    types a command, requires the cancel, the cancel note, the button offered again and a new arming, and leaves its own shell"""
    import cases  # noqa: F401
    import cases_subshell_ready as SR
    body = inspect.getsource(SR.c_ready_relay_shape)
    assert body.index("relay_waits_for_prompt(hops(ctx, off))") < body.index('judge_relay_note(ctx, r, off, "armed_wait"') < body.index('ctx.be.key("Return")')
    assert "note=armed_wait event=gone why=written" in body
    assert body.index('"the pane\'s bash runs the next command"') < body.index("if r.fails:") < body.index("relay_cancel_leg(ctx, r, cmd)")
    leg = inspect.getsource(SR.relay_cancel_leg)
    order = ['ctx.be.key("ctrl+i")', 'ctx.be.type("true", 12, enter=True)', '"why=UserDrove"', 'judge_relay_note(ctx, r, off, "cancelled"',
             '"offered=true"', "off2 = ctx.log_size()", "hops(ctx, off2)", "ctx.exit_nested(", "leave_inner(ctx, r,"]
    at = [leg.index(x) for x in order]
    assert at == sorted(at), list(zip(order, at))
    assert "ctx.left_inner = False" in leg and "not_judged" in leg
    judge = inspect.getsource(SR.judge_relay_note)
    assert "r.unverifiable(" in judge and 'r.check(state == "read"' in judge and "ctx.ocr(tag)" in judge


@check
def r21_relaynote_documented():
    """docs/testing.md names what changed and this file"""
    doc = _src("docs", "testing.md")
    sec = doc[doc.index("## Round 21: the two notes of a Phosphorize click behind a relay"):]
    for word in ("#2142", "`subshell_relay_note note=armed_wait event=shown`", "`subshell_setup_reoffered offered=<bool>`", "`relay_note_armed_wait`", "`relay_note_cancelled`",
                 "PHOSPHOR_SUBSHELL_RELAY_NOTE", "selftest_r21_relaynote.py", "subshell_relay_note_view_tests.rs", "UNVERIFIABLE"):
        assert word in sec, "docs/testing.md (Round 21 relay notes) does not mention %s" % word
