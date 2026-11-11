# appci/cases_agent_close_undo.py (round 18, #2103, owner decision 2026-11-11; design note docs/design/r18-retire.md): one agent case on the scripted
# fake model (fake_model.py; no real provider is ever used). In cases.PROVING and cases.PROVING_FAKE (owner rule 2026-11-09: a new case runs in the fix
# tier until the owner graduates it). Registered by one import line at the end of cases.py.
#   agent_close_undo_call_status   A new tab in the agent view. ONE reply with two calls: a command (approved by keyboard; it prints a token that is NOT
#                                  in its own command line) and a question. When the command is done and the question waits, the command's result is
#                                  finished and UNSENT. The close key closes the tab, the undo key brings it back, and a plain prompt is typed.
#                                  Oracles:
#                                    * the app's `hop=agent_close_discard ... results=N kept_status=1` (N >= 1): the close took the finished result
#                                      away and kept its call's status (kept_status=0 = the kill switch PHOSPHOR_CLOSE_KEEPS_CALL_STATUS=0 is set);
#                                    * #848, the owner's condition for #2103: the token the command printed is in NO tool-result message of any
#                                      request the fake model receives after the close (fake.reqs). Where else the token appears in those requests
#                                      (the app attaches the blocks run since the last prompt as context, #2045) is a note, not a verdict;
#                                    * the prompt after the undo reaches the model exactly once and its turn ends `success`;
#                                    * no `hop=agent_turn_stuck` line (#1967 / #2103: the restored reply's calls are not read as lost) and no
#                                      unexpected ERROR line from the prompt of the two calls to the end;
#                                    * the note (owner decision 2026-11-11, docs/design/r18-closenote.md): after the undo the restored reply says
#                                      "Results from before this tab was closed were not sent to the agent. Tell it what happened if it matters."
#                                      Read off the screenshot by OCR, with a control (the prompt the case typed, in the same conversation): the
#                                      control read and the note not = a failure; no OCR engine, or the control not read either = UNVERIFIABLE,
#                                      never a pass. Judged only when the close logged kept_status=1 (with the kill switch off there is no note,
#                                      and that run already fails on the discard line). And the note is SCREEN ONLY: its words are in no message
#                                      of any request the fake model receives after the close.
#                                  NOT judged by this case: what the restored cards LOOK like (the green check of the command that ran, the
#                                  cancelled question). No log line or OCR text tells a check mark from a cancel mark; the screenshot
#                                  `close_undo_restored` is kept for a person, and the unit test
#                                  `cards_of_a_restored_pane_show_what_the_discarded_calls_did` (output_tests.rs) pins what the cards decide on.
# Typing is at 10-25 ms per key with Enter in the same call. A step whose evidence is missing is `unverifiable`, never a pass.
# Platforms: Linux, macOS (needs the app-only close chord, R.close_key), Windows Git Bash pane (winmap.GITBASH_ONLY: the command is a POSIX printf).
import os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases  # noqa: E402  (cases.py imports this module as its last line)
import cases_caseagent as AG  # noqa: E402
import cases_agent_resilience as AR  # noqa: E402
import cases_agent_question as AQ  # noqa: E402
import cases_r14_w1a as W1  # noqa: E402
import cases_robust as R  # noqa: E402
import fake_model as FMOD  # noqa: E402
from cases import case  # noqa: E402

DISCARD_RE = re.compile(r"hop=agent_close_discard conversation=.*? results=(\d+) kept_status=([01])")     # controller.rs discard_finished_results_for_close
STUCK = "hop=agent_turn_stuck"                                                                              # controller.rs check_turn_invariant (ERROR)
SHELL_KIND = "run_shell_command"
# action_model.rs RESULTS_WITHHELD_BY_CLOSE_NOTE (pinned against the source by the self-test). The needle is the part no other text on screen or in a
# request has; OCR is matched with cases_caseagent.fuzzy_has (line wraps and single-glyph confusions tolerated).
NOTE = "Results from before this tab was closed were not sent to the agent. Tell it what happened if it matters."
NOTE_NEEDLE = "were not sent to the agent"
PROMPT_HEAD = "Please run scripted command"          # the control for the OCR: the case typed it into the same conversation, it is on the same screen
OUT_HEAD = "QX7"


# ---------------------------------------------------------------- pure parts (selftest.py agent_close_undo_pieces_2103)
def scripted_command(tok):
    """(command, what it prints). The printed text is in no part of the command line: a request that holds it holds the command's OUTPUT."""
    return "printf '%%s%%s\\n' %s %s" % (OUT_HEAD, tok), OUT_HEAD + tok


def discards(text):
    """[(results, kept_status)] of every hop=agent_close_discard line, in order"""
    return [(int(n), int(k)) for n, k in DISCARD_RE.findall(text or "")]


def judge_discard(found):
    """(problem or None, missing evidence or None) for the close of a pane whose reply had one finished, unsent result"""
    if not found:
        return None, ("the app logged no hop=agent_close_discard line at the close: no finished result was held when the tab closed (or this build "
                      "predates #2103), so nothing below shows what a close does with one")
    if any(k == 0 for _n, k in found):
        return "the close ERASED the finished result(s) (kept_status=0): PHOSPHOR_CLOSE_KEEPS_CALL_STATUS is off in this run", None
    return None, None


def _content(m):
    c = m.get("content")
    return c if isinstance(c, str) else ("" if c is None else str(c))


def tool_results_with(reqs, token):
    """how many tool-result messages of `reqs` (fake.reqs entries) hold `token`"""
    return sum(1 for q in reqs or [] for m in q.get("messages") or [] if m.get("role") == "tool" and token in _content(m))


def other_messages_with(reqs, token):
    """the roles of every other message of `reqs` that holds `token` (attached context of a user turn, ...)"""
    return sorted(set(str(m.get("role")) for q in reqs or [] for m in q.get("messages") or [] if m.get("role") != "tool" and token in _content(m)))


def command_ran_and_question_waits(text):
    """the state the case needs before the close: the command's action is done and the question card is answerable"""
    ran = any(kind == SHELL_KIND and st == "done" for _a, kind, st in AR.action_hops(text))
    return ran and AQ.verdict(text) == "answerable"


def judge_note(ocr_text):
    """(verdict, why) for the screen of the restored tab. verdict: "shown", "missing" (the screen was read and the note is not on it: a failure), or
    "unverifiable" (the screen text could not be read: never a pass)"""
    if ocr_text is None:
        return "unverifiable", "no OCR engine on this machine (tesseract-ocr; Apple Vision on macOS): the note on the restored reply was not read"
    flat = " ".join(ocr_text.split())
    if AG.fuzzy_has(flat, NOTE_NEEDLE):
        return "shown", ""
    if not AG.fuzzy_has(flat, PROMPT_HEAD):
        return "unverifiable", ("the OCR read neither the note nor the prompt this case typed (%r) off the restored tab's screenshot: the screen text "
                                "could not be read, the note is not judged" % PROMPT_HEAD)
    return "missing", ("the restored reply does not show the note %r: the OCR read the case's own prompt off the same screenshot but not the note "
                       "(#2103, owner decision 2026-11-11)" % NOTE)


def _strings(x):
    """every string in a JSON-like value, keys included"""
    if isinstance(x, str):
        yield x
    elif isinstance(x, dict):
        for k, v in x.items():
            yield str(k)
            yield from _strings(v)
    elif isinstance(x, (list, tuple)):
        for v in x:
            yield from _strings(v)


def requests_with_note(reqs):
    """how many of `reqs` (fake.reqs entries) hold the note's words anywhere: in any message of any role or any other field, line breaks or not"""
    return sum(1 for q in reqs or [] if any(NOTE_NEEDLE in " ".join(t.split()) for t in _strings(q)))


# ---------------------------------------------------------------- the case
@case("agent_close_undo_call_status", modes=("agent",), order=98, gui=True)
def c_agent_close_undo_call_status(ctx, r):
    """#2103: a tab closed with a finished, unsent tool result and brought back: the result is never sent, its call keeps its status, nothing reads as stuck"""
    close = R.close_key(ctx)
    if not close:
        r.skip(R.NO_CLOSE_WHY)
    undo = W1.reopen_key(ctx)
    AR.need_fake(ctx, r)
    f = AR.fake()
    ctx.reset()
    t = W1.Tabs(ctx, r)
    tok, qtag, tok2 = ctx.tok(6).upper(), "Q" + ctx.tok(5).upper(), "U" + ctx.tok(6).upper()
    key = "CU" + tok
    command, printed = scripted_command(tok)
    d = os.path.join(ctx.W, "cu_" + tok.lower())
    FMOD.SCRIPTED_COMMANDS[key] = command
    opened = False
    off = ctx.log_size()
    try:
        f.clear_faults()
        t.new_tab(d)
        opened = True
        if ctx.log_view_mode() is not None and ctx.log_view_mode() != "agent":
            r.unverifiable("the new tab is not in the agent view (hop=view_mode says %s): the case was not run" % ctx.log_view_mode())
            return
        off = ctx.log_size()
        AG.type_fast(ctx, "Please run scripted command %s then ask me a question %s" % (key, qtag))
        if not r.check(AG.wait_marker(ctx, off, "queue_actions", timeout=60), "no action was queued within 60 s of the prompt"):
            return
        time.sleep(1.2)
        ctx.be.key("Return")           # focus the command card
        time.sleep(0.8)
        ctx.be.key("Return")           # approve it: the command runs, the question waits behind it
        if not ctx.until(lambda: command_ran_and_question_waits(ctx.log_since(off)), 45, step=0.5):
            r.unverifiable("the command did not finish with the question waiting behind it within 45 s (actions %s, question verdict %s): there is no "
                           "finished, unsent result to close on" % (AR.action_hops(ctx.log_since(off))[-6:], AQ.verdict(ctx.log_since(off))))
            return
        time.sleep(0.8)
        ctx.shot("close_undo_before_close")
        reqs0 = len(f.reqs)
        if not r.check(tool_results_with(f.reqs, printed) == 0, "the command's output reached the model BEFORE the close: its result was not unsent, the case's "
                       "precondition does not hold"):
            return

        # ---- the close
        off_close = ctx.log_size()
        ctx.be.guard()
        ctx.be.key(close)
        time.sleep(1.2)
        if W1.CLOSE_WARNING_LINE in ctx.log_since(off_close):
            r.note("the close key asked first (a waiting agent turn): answered with Enter")
            ctx.be.key("Return")
            time.sleep(1.2)
        opened = False
        found = discards(ctx.log_since(off_close))
        bad, missing = judge_discard(found)
        if missing:
            r.unverifiable(missing)
        else:
            r.check(bad is None, bad or "")
        r.metric("close_discard_results", sum(n for n, _k in found))

        # ---- the undo
        ctx.be.guard()
        ctx.be.key(undo)
        time.sleep(1.5)
        back = t.where()
        opened = back == R.norm_path(d)
        if not r.check(opened, "after the undo key (%s) the pane that has the keyboard is in %r, expected the closed tab's directory %r: the tab did not "
                       "come back, the steps after the undo were not run" % (undo, back, R.norm_path(d))):
            return
        if found and all(k == 1 for _n, k in found):
            verdict, why = judge_note(ctx.ocr("close_undo_restored"))          # the screenshot is kept under this tag either way
            if verdict == "unverifiable":
                r.unverifiable(why)
            else:
                r.check(verdict == "shown", why)
            r.metric("close_note_shown", {"shown": 1, "missing": 0}.get(verdict, -1))
        else:
            ctx.shot("close_undo_restored")

        # ---- the next prompt in the restored conversation
        off2 = ctx.log_size()
        AG.type_fast(ctx, "Reply with the word %s" % tok2)
        r.check(AG.wait_marker(ctx, off2, "[byop] opening stream", timeout=60), "the prompt after the undo made no model request")
        state = AR.wait_turn(ctx, off2, ("success", "error", "cancelled"), 60)
        r.check(state == "success", "the prompt after the undo ended %r, expected `success`" % state)
        r.check(f.fresh_with(tok2) == 1, "the prompt after the undo reached the model %d times (expected once)" % f.fresh_with(tok2))
        later = list(f.reqs)[reqs0:]
        leaked = tool_results_with(later, printed)
        r.check(leaked == 0, "#848 / #2103: the output of the command that ran before the close is in %d tool-result message(s) of the %d request(s) sent after "
                "the close: a result the close discarded was sent to the model" % (leaked, len(later)))
        noted = requests_with_note(later)
        r.check(noted == 0, "the note of the restored reply is in %d of the %d request(s) sent after the close: it is for the screen only and must never "
                "reach the model" % (noted, len(later)))
        elsewhere = other_messages_with(later, printed)
        if elsewhere:
            r.note("the command's output is in %s message(s) of a request after the close (not a tool result: the app attaches the blocks run since the "
                   "last prompt as context, #2045)" % ", ".join(elsewhere))
        whole = ctx.log_since(off)
        r.check(STUCK not in whole, "the app logged %s: the turn watch read the restored reply's calls as lost (#2103)" % STUCK)
        for p in AR.judge_errors(whole, limit=4):
            r.check(False, p)
    finally:
        FMOD.SCRIPTED_COMMANDS.pop(key, None)
        f.clear_faults()
        if opened:
            R.close_tab_best_effort(ctx, r)
            time.sleep(1.0)
        AG.cleanup_tmp(ctx)
