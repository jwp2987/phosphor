# appci/cases_agent_question.py (#1851): the ask_user_question card must never hang on "Preparing question...". Real app, scripted fake model (fake_model.py,
# APPCI_FAKE_MODEL=1; no real provider is used). Trigger: the user turn "Please ask me a question <TAG>" -> one ask_user_question call (fake_model.ask_question_call).
# Faults (fake_model.inject, kinds ask_*) shape the stream around that call.
#   agent_question_answer      the call arrives whole: the card becomes answerable (hop=agent_action blocked), Enter answers it, the model gets the answer
#                              (a tool message in fake.tool_results), the turn ends in `success`, the input takes the next prompt
#   agent_question_cut         the connection is cut right after the complete call
#   agent_question_nodone      the call, then no `done` line for 20 s, then the close (a provider that never finishes; the stream ends without a terminal End)
#   agent_question_trunc       the call's arguments are cut mid-JSON
#   agent_question_empty       the call's arguments are `{}` (no question)
#   agent_question_slow        a text line, 6 s of silence on the open stream, then the call
#   agent_question_ctrl_c      Ctrl-C while the call sits unfinished (nodone fault): the turn is `cancelled` within 5 s and the client hangs up
#   agent_question_after_command  (#1858) one reply [run_shell_command sleep 8, ask_user_question]: while the command runs the screen must not say
#                              "Preparing question..." (the running command's text shows); then the command ends and the card is answerable
#   agent_question_steer_at_end  (#1861) a new prompt typed right after the stream opens (the reply ends while the steer cancels it): the steer prompt
#                              reaches the model once and the turn is never left in progress with the ask open
#   agent_question_steer       a new prompt typed while the call sits unfinished (#851 steer): the new prompt reaches the model, the turn ends, nothing stays pending
# Pass rule for every fault case: the conversation reaches an END the user can see, one of
#   (a) an answerable card (hop=agent_action kind=ask_user_question status=blocked): answered with Enter, the turn then ends `success`;
#   (b) a terminal turn state (hop=turn error / cancelled / success) with no ask_user_question action left open, within a bound;
# and in both: the input then takes a plain prompt that reaches the model exactly once and ends `success` (the turn was really freed), no agent action stays
# pending, and the app log has no ERROR line outside the [byop] failure allowance (cases_agent_resilience.judge_errors). Never `in_progress` forever: the case
# fails with the last state it saw. Event-driven app side (no timer decides); the idle give-up (PHOSPHOR_BYOP_STREAM_IDLE_TIMEOUT_SECS) is only a net.
# Platforms: wherever the fake model runs (host, 127.0.0.1): Linux, macOS, Windows.
import os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases  # noqa: E402
import cases_caseagent as AG  # noqa: E402
import cases_agent_resilience as AR  # noqa: E402
from cases import case  # noqa: E402

ASK_KIND = "ask_user_question"
BOUND_S = 150            # longest a variant may take to END: the nodone close (20 s) + resume + a possible idle give-up on a slow runner


# ---------------------------------------------------------------- pure parts (selftest rows)
def early_success(states):
    """pure (#2029): the turn read `success` and then went on with nobody acting: a `success` that is not the last turn state of an excerpt in which the
    user did nothing after the prompt. Before the fix a tool call the app rejected read success, in_progress (the automatic follow-up), success."""
    return "success" in list(states)[:-1]


def ask_actions(text):
    """[(id, status)] of the ask_user_question hop=agent_action lines, in order"""
    return [(aid, st) for aid, kind, st in AR.action_hops(text) if kind == ASK_KIND]


def ask_blocked_ids(text):
    """ids of ask_user_question actions that reached `blocked` (a card the user could answer)"""
    out = []
    for aid, st in ask_actions(text):
        if st == "blocked" and aid not in out:
            out.append(aid)
    return out


def ask_open_ids(text):
    """ids of ask_user_question actions whose last logged status is not terminal (done / failed / cancelled)"""
    last = {}
    for aid, st in ask_actions(text):
        last[aid] = st
    return sorted(a for a, s in last.items() if s not in ("done", "failed", "cancelled"))


def verdict(text):
    """'answerable' | 'ended' | 'hung' for the log excerpt since the prompt: answerable = a card reached blocked and is still open; ended = the turn is
    terminal and no ask action is open; hung = anything else (still in progress, or terminal with an ask action left open)"""
    st = AR.last_turn_state(text)
    open_ids = ask_open_ids(text)
    blocked = [a for a in ask_blocked_ids(text) if a in open_ids]
    if blocked:
        return "answerable"
    if st in ("error", "cancelled", "success") and not open_ids:
        return "ended"
    return "hung"


def settled_verdict(samples):
    """pure: the verdict a run of polled verdict() samples (oldest first) has SETTLED on, or None while it has not: the last two samples agree and say
    'answerable' or 'ended'. Round 10 (Linux Wayland, agent_question_empty; #2027): for a tool call the app rejects, the log reads `hop=turn state=success`,
    3 ms later `state=in_progress` (the automatic follow-up request with the error result), then `success` again. One poll read the first `success` as
    'ended', the case read the log AGAIN for the verdict it judges and got 'hung', and the row said "neither answerable nor ended after 150 s" 5 s into
    the case. A single sample is never the end; two polls apart is."""
    if len(samples) >= 2 and samples[-1] == samples[-2] and samples[-1] in ("answerable", "ended"):
        return samples[-1]
    return None


# ---------------------------------------------------------------- helpers
def ready(ctx, r):
    AR.need_fake(ctx, r)
    AG.fresh_conversation(ctx, r)


def answer_card(ctx, r, off, tag):
    """Enter (selects / confirms) up to three times until the ask action is done; True when it finished"""
    for _ in range(3):
        ctx.be.key("Return")
        if ctx.until(lambda: any(st == "done" for _a, st in ask_actions(ctx.log_since(off))), 6, step=0.25):
            ctx.shot("question_%s_answered" % tag)
            return True
    ctx.shot("question_%s_answer_failed" % tag)
    return False


def next_prompt_works(ctx, r, f, off_all, tag):
    """the input takes a plain prompt after the variant: it reaches the model exactly once and the turn ends `success`"""
    off, req0 = ctx.log_size(), f.requests
    tok = "U" + ctx.tok(6).upper()
    AG.type_fast(ctx, "Reply with the word %s" % tok)
    r.check(AG.wait_marker(ctx, off, "[byop] opening stream", timeout=60), "%s: the prompt after the question made no model request (the input is stuck)" % tag)
    state = AR.wait_turn(ctx, off, ("success", "error", "cancelled"), 60)
    r.check(state == "success", "%s: the prompt after the question ended %r, expected `success`" % (tag, state))
    r.check(f.fresh_with(tok) == 1, "%s: the next prompt reached the model %d times (expected once)" % (tag, f.fresh_with(tok)))
    r.check(f.requests - req0 == 1, "%s: the next prompt made %d request(s), expected 1" % (tag, f.requests - req0))
    r.check(not ask_open_ids(ctx.log_since(off_all)), "%s: an ask_user_question action is still open: %s" % (tag, ask_open_ids(ctx.log_since(off_all))))
    for p in AR.judge_errors(ctx.log_since(off_all), limit=4):
        r.check(False, "%s: %s" % (tag, p))


def run_variant(ctx, r, tag, fault, **kw):
    """arm `fault` (None: no fault), ask the question, and require the card to be answerable or the turn to have ended; then the next prompt works"""
    ready(ctx, r)
    f = AR.fake()
    try:
        f.clear_faults()
        tok = "Q" + ctx.tok(6).upper()
        if fault:
            f.inject(fault, times=1, match=tok, **kw)
        off, req0 = ctx.log_size(), f.requests
        t0 = time.time()
        AG.type_fast(ctx, "Please ask me a question %s" % tok)
        if not AG.wait_marker(ctx, off, "[byop] opening stream", timeout=60):
            r.check(False, "%s: no model request within 60 s of the prompt" % tag)
            return
        got = []

        def settled():
            got.append(verdict(ctx.log_since(off)))
            return settled_verdict(got)
        # the verdict judged below is the one the polls settled on (two polls 0.5 s apart that agree), never a fresh read of a log that is still moving
        v = ctx.until(settled, BOUND_S, step=0.5) or "hung"
        waited = time.time() - t0
        r.metric("question_%s_seconds_to_end" % tag, round(waited, 1))
        ctx.shot("question_%s_state" % tag)
        log = ctx.log_since(off)
        # #2029: nothing was typed or pressed since the prompt, so a `success` that the turn then left by itself was reported too early
        states = [s for _c, s in AR.turn_states(log)]
        r.check(not early_success(states), "%s: the turn read `success` and then went on by itself (turn states since the prompt: %r): it was reported finished "
                "before its automatic follow-up was decided (#2029)" % (tag, states))
        r.check(v in ("answerable", "ended"),
                "%s: the turn is neither answerable nor ended %.0f s after the prompt (bound %d s; last samples %r, last turn state %r, ask actions %r): the card hangs on 'Preparing question...'"
                % (tag, waited, BOUND_S, got[-3:], AR.last_turn_state(log), ask_actions(log)))
        if v == "answerable":
            r.check(answer_card(ctx, r, off, tag), "%s: Enter did not answer the card (no ask_user_question action reached done)" % tag)
            state = AR.wait_turn(ctx, off, ("success", "error", "cancelled"), 60)
            r.check(state == "success", "%s: after the answer the turn ended %r, expected `success`" % (tag, state))
            r.check(any("alpha" in t or "ok" in t.lower() for _t, t in f.tool_results[-1:]), "%s: the model's next request carried no answer to the question" % tag)
        elif v == "hung":
            ctx.be.key("ctrl+c")
        next_prompt_works(ctx, r, f, off, tag)
        r.check(f.requests - req0 >= 2, "%s: the model got only %d request(s) for the whole variant" % (tag, f.requests - req0))
    finally:
        f.clear_faults()
        AG.cleanup_tmp(ctx)
        ctx.be.key("ctrl+c")


# ---------------------------------------------------------------- cases
@case("agent_question_answer", modes=("agent",), order=53, gui=True)
def c_agent_question_answer(ctx, r):
    """a whole ask_user_question call: the card becomes answerable, Enter answers it, the model gets the answer, the turn ends `success`, the input works"""
    run_variant(ctx, r, "answer", None)


@case("agent_question_cut", modes=("agent",), order=53, gui=True)
def c_agent_question_cut(ctx, r):
    """the connection is cut right after the complete call: an answerable card (a resume asks again) or an ended turn with a visible error; never a hang"""
    run_variant(ctx, r, "cut", "ask_cut")


@case("agent_question_nodone", modes=("agent",), order=53, gui=True)
def c_agent_question_nodone(ctx, r):
    """the call, then no `done` for 20 s, then the close without a terminal End: the turn ends (card or error), never a hang"""
    run_variant(ctx, r, "nodone", "ask_nodone", secs=20)


@case("agent_question_trunc", modes=("agent",), order=53, gui=True)
def c_agent_question_trunc(ctx, r):
    """the call's arguments are cut mid-JSON: a rejected-call card and the model's retry, or an ended turn; never a hang"""
    run_variant(ctx, r, "trunc", "ask_trunc")


@case("agent_question_empty", modes=("agent",), order=53, gui=True)
def c_agent_question_empty(ctx, r):
    """the call carries `{}` (no question): an error result goes back to the model (#1797), the turn ends; never a hang"""
    run_variant(ctx, r, "empty", "ask_empty")


@case("agent_question_slow", modes=("agent",), order=53, gui=True)
def c_agent_question_slow(ctx, r):
    """a text line, 6 s of silence on the open stream, then the call: while the call is not there the turn is simply in progress; then the card is answerable"""
    run_variant(ctx, r, "slow", "ask_slow", secs=6)


@case("agent_question_ctrl_c", modes=("agent",), order=53, gui=True)
def c_agent_question_ctrl_c(ctx, r):
    """Ctrl-C while the question call sits unfinished (the stream is held open after the call): `cancelled` within 5 s, the client hangs up, the input works"""
    ready(ctx, r)
    f = AR.fake()
    try:
        f.clear_faults()
        tok = "C" + ctx.tok(6).upper()
        f.inject("ask_nodone", times=1, match=tok, secs=60)
        off, ab0 = ctx.log_size(), f.aborted
        AG.type_fast(ctx, "Please ask me a question %s" % tok)
        r.check(AG.wait_marker(ctx, off, "[byop] opening stream", timeout=60), "no model request within 60 s of the prompt")
        r.check(ctx.until(lambda: bool(f.fired) and f.fired[-1] == "ask_nodone", 10), "the fault never reached the fake model")
        time.sleep(3.0)
        ctx.shot("question_ctrl_c_waiting")
        t0 = time.time()
        ctx.be.key("ctrl+c")
        ctx.ctrlc_ts.append(time.time())
        state = AR.wait_turn(ctx, off, ("cancelled", "error", "success"), 8)
        took = round(time.time() - t0, 2)
        r.metric("question_ctrl_c_to_state_s", took)
        r.check(state == "cancelled", "Ctrl-C during the preparing state left the turn %r, expected `cancelled`" % state)
        r.check(took <= 5.0, "the cancel took %.1f s (bound 5 s)" % took)
        r.check(ctx.until(lambda: f.aborted > ab0, 5), "the app did not hang up the connection within 5 s of the cancel")
        ctx.shot("question_ctrl_c_cancelled")
        next_prompt_works(ctx, r, f, off, "ctrl_c")
    finally:
        f.clear_faults()
        AG.cleanup_tmp(ctx)
        ctx.be.key("ctrl+c")


@case("agent_question_steer", modes=("agent",), order=53, gui=True)
def c_agent_question_steer(ctx, r):
    """a new prompt typed while the question call sits unfinished (#851 steer): it reaches the model once, the turn ends, nothing stays pending"""
    ready(ctx, r)
    f = AR.fake()
    try:
        f.clear_faults()
        tok = "S" + ctx.tok(6).upper()
        f.inject("ask_nodone", times=1, match=tok, secs=60)
        off = ctx.log_size()
        AG.type_fast(ctx, "Please ask me a question %s" % tok)
        r.check(AG.wait_marker(ctx, off, "[byop] opening stream", timeout=60), "no model request within 60 s of the prompt")
        r.check(ctx.until(lambda: bool(f.fired) and f.fired[-1] == "ask_nodone", 10), "the fault never reached the fake model")
        time.sleep(3.0)
        tok2 = "T" + ctx.tok(6).upper()
        req0 = f.requests
        AG.type_fast(ctx, "Reply with the word %s" % tok2)
        r.check(ctx.until(lambda: f.fresh_with(tok2) >= 1, 30, step=0.25), "the steer prompt never reached the model")
        state = AR.wait_turn(ctx, off, ("success", "error", "cancelled"), 60)
        ctx.shot("question_steer_end")
        r.check(state in ("success", "cancelled"), "after the steer the turn is %r, expected success (or cancelled)" % state)
        r.check(f.fresh_with(tok2) == 1, "the steer prompt reached the model %d times" % f.fresh_with(tok2))
        r.check(not ask_open_ids(ctx.log_since(off)), "an ask_user_question action is still open after the steer: %s" % ask_open_ids(ctx.log_since(off)))
        next_prompt_works(ctx, r, f, off, "steer")
        r.check(f.requests - req0 >= 2, "the model got %d request(s) after the steer" % (f.requests - req0))
    finally:
        f.clear_faults()
        AG.cleanup_tmp(ctx)
        ctx.be.key("ctrl+c")


@case("agent_question_after_command", modes=("agent",), order=53, gui=True)
def c_agent_question_after_command(ctx, r):
    """#1858: [slow command, ask] in one reply: no 'Preparing question...' on screen while the command runs; then the card is answerable and answers"""
    ready(ctx, r)
    f = AR.fake()
    try:
        f.clear_faults()
        tok = "K" + ctx.tok(6).upper()
        off = ctx.log_size()
        AG.type_fast(ctx, "Please run a slow command then ask me a question %s" % tok)
        r.check(AG.wait_marker(ctx, off, "queue_actions", timeout=60), "no action was queued within 60 s of the prompt")
        time.sleep(1.2)
        ctx.be.key("Return")           # focus the command card
        time.sleep(0.8)
        ctx.be.key("Return")           # approve it: the command runs for ~8 s with the ask queued behind it
        time.sleep(2.5)
        ctx.shot("question_after_command_running")
        seen = AG.ocr_has(ctx, "question_after_command_ocr", "Preparing question")
        if seen is None:
            r.note("no OCR on this runner: the loading text was not read from the screen")
        else:
            r.check(not seen, "'Preparing question...' is on screen while the earlier command is still running (the wait is the command's)")
        ok = ctx.until(lambda: "answerable" == verdict(ctx.log_since(off)), 60, step=0.5)
        r.check(ok, "after the command ended the ask never became answerable (verdict %s)" % verdict(ctx.log_since(off)))
        if ok:
            r.check(answer_card(ctx, r, off, "after_command"), "Enter did not answer the card")
            state = AR.wait_turn(ctx, off, ("success", "error", "cancelled"), 60)
            r.check(state == "success", "after the answer the turn ended %r, expected `success`" % state)
        next_prompt_works(ctx, r, f, off, "after_command")
    finally:
        f.clear_faults()
        AG.cleanup_tmp(ctx)
        ctx.be.key("ctrl+c")


@case("agent_question_steer_at_end", modes=("agent",), order=53, gui=True)
def c_agent_question_steer_at_end(ctx, r):
    """#1861: a steer typed right as the question reply ends cancels a finished stream and discards its ask: the steer prompt is answered, nothing stays open"""
    ready(ctx, r)
    f = AR.fake()
    try:
        f.clear_faults()
        tok = "E" + ctx.tok(6).upper()
        off = ctx.log_size()
        AG.type_fast(ctx, "Please ask me a question %s" % tok)
        r.check(AG.wait_marker(ctx, off, "[byop] opening stream", timeout=60), "no model request within 60 s of the prompt")
        tok2 = "T" + ctx.tok(6).upper()
        AG.type_fast(ctx, "Reply with the word %s" % tok2)
        # Two legitimate races (the fake model's reply is instant, so the whole ask is usually in before the steer is typed):
        #  (a) the steer won: it cancelled the finished stream, the ask was discarded, the steer prompt reaches the model by itself;
        #  (b) the ask card came first: the typed prompt is QUEUED behind the pending question (hop=enter_queued origin=pending_approval, Round 8 Linux), so it can only reach
        #      the model after the card is answered. Requiring the steer at the model before answering (the old first check) failed (b) although the app did the right thing.
        ctx.until(lambda: f.fresh_with(tok2) >= 1 or verdict(ctx.log_since(off)) == "answerable", 30, step=0.25)
        ctx.until(lambda: verdict(ctx.log_since(off)) in ("answerable", "ended"), BOUND_S, step=0.5)
        v = verdict(ctx.log_since(off))
        ctx.shot("question_steer_at_end_state")
        r.check(v in ("answerable", "ended"), "the turn is neither answerable nor ended (verdict %s, last turn state %r)" % (v, AR.last_turn_state(ctx.log_since(off))))
        if v == "answerable":
            r.check(answer_card(ctx, r, off, "steer_at_end"), "Enter did not answer the card")
        r.check(ctx.until(lambda: f.fresh_with(tok2) >= 1, 30, step=0.25), "the steer prompt never reached the model (after the card was %s)" % ("answered" if v == "answerable" else "discarded"))
        AR.wait_turn(ctx, off, ("success", "error", "cancelled"), 60)
        r.check(f.fresh_with(tok2) == 1, "the steer prompt reached the model %d times" % f.fresh_with(tok2))
        next_prompt_works(ctx, r, f, off, "steer_at_end")
    finally:
        f.clear_faults()
        AG.cleanup_tmp(ctx)
        ctx.be.key("ctrl+c")
