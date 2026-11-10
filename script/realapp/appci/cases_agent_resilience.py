# appci/cases_agent_resilience.py (#1831, #1832): the agent under model failures and under restarts, in the real app, against the scripted fake model
# (fake_model.py, APPCI_FAKE_MODEL=1; no real provider is ever used). Registered like cases_caseagent.py (imported by the last line of cases.py's block).
#
# #1831 model failures mid-reply. The fake model injects the fault (fake_model.FakeModel.inject / go_down); the app is the real one.
#   agent_fault_429          HTTP 429 with Retry-After on every try: the retry schedule runs out, a clear error ends the turn; a re-sent prompt succeeds
#   agent_fault_500          HTTP 500 on every try (2 s + 4 s + 8 s schedule): same
#   agent_fault_disconnect   the connection is cut mid-stream after some text: no retry (output was already seen), a clear "incomplete" error; re-send works
#   agent_fault_down         the fake model is DOWN (connection refused): clear unreachable error; after it is back up a re-sent prompt works
#   agent_fault_slow_cancel  nothing arrives for 30 s (first token delay): Ctrl-C ends the turn within a bound AND the client hangs up (fake.aborted),
#                            then a new prompt works and no request is made on its own afterwards
#   agent_fault_queued_rows  two /queue rows behind a failing reply: each row is sent exactly once or is still held (stored/in the input), never lost, never twice
# Oracles (never a screenshot alone): hop=turn (conversation state) ends in `error` / `cancelled` / `success`, not `in_progress`; no hop=agent_action left open;
# fake.requests / fake.fresh / fake.aborted; the ERROR lines of the app log against a small allowance (the fuzzer's own: oracle.rs error_allowance, 2 per
# failing kind; only `[byop] open stream failed | stream chunk error | stream ended without a terminal End` are expected); OCR of the block is a note on
# top when an engine exists.
#
# #1832 restart mid-conversation. restore_session is on. One COMPLETED turn first (the conversation to restore), then the scenario, then a clean quit
# (Linux) or `kill -9` (every platform) with:
#   stream   a reply is streaming (fake `drip`)
#   queued   two `/queue` rows wait behind the streaming reply
#   blocked  an approval card is blocked on a command and a follow-up is queued behind it (a locked PendingApprovalFollowUp row)
# Relaunch (cases_robust relaunch helpers): the conversation is in warp.sqlite (agent_conversations / ai_queries hold the completed turn), the queued rows are
# in queued_prompts (or, if the app popped them back to the input, on the screen) and are NOT sent (no new request carries them, fake counts requests), no
# hop=agent_action running / hop=turn in_progress appears with no user input, and the blocked command never ran (no effect file, no rec.log line).
#   agent_restart_{quit,kill}_{stream,queued,blocked}  (the quit variants are Linux only: elsewhere the harness has no quit key, cases_robust.stop_for_restart)
#
# Overlap with the in-process agent fuzzer (app/src/ai/blocklist/agent_fuzz, docs/testing.md "Agent fuzzer"): it replaces the provider SEAM (events, not HTTP) and
# checks state invariants I1-I6 (incl. chaos kinds status / disconnect / stall / delay / never_end and the ERROR allowance). It never touches the wire, never
# restarts the app, never reads the sqlite store. These cases cover exactly that: the real HTTP client (Retry-After, connection refused, a socket cut),
# the real UI error block and cancel key, and the quit / kill -9 + relaunch persistence.
# Platforms: the fault cases run wherever the fake model runs (host side, 127.0.0.1): Linux, macOS, Windows. Restart quit variants: Linux only; kill variants:
# Linux, macOS (SIGKILL), Windows (taskkill /F /T).
import os, re, sqlite3, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases  # noqa: E402
import cases_caseagent as AG  # noqa: E402
import cases_robust as R  # noqa: E402
import cases_coverage_a as CA  # noqa: E402
import fake_model as FMOD  # noqa: E402
import replay as RP  # noqa: E402
from cases import case  # noqa: E402

TURN_RE = re.compile(r"hop=turn conv=(\S+) state=(\w+)")
ACTION_RE = re.compile(r"hop=agent_action id=(\S+) kind=(\S+) status=(\w+)")
ROW_RE = re.compile(r"hop=queued_row id=(\S+) state=(\w+)")
BYOP_ERR_ALLOW_RE = r"\[byop\] (open stream failed|stream chunk error|stream ended without a terminal End)"
BYOP_ERR_LIMIT = 2                                      # oracle.rs error_allowance: status / disconnect / stall = 2 per failing request chain
GENERIC_ERROR_WORDS = ("error", "failed", "retr", "provider", "rate limit", "server", "unreachable", "incomplete", "connect", "closed", "truncated")


# ---------------------------------------------------------------- pure parts (selftest rows in selftest.py)
def turn_states(text):
    """[(conversation id, state)] of every hop=turn line, in order"""
    return TURN_RE.findall(text or "")


def last_turn_state(text):
    st = turn_states(text)
    return st[-1][1] if st else None


def action_hops(text):
    return ACTION_RE.findall(text or "")


def open_actions(text):
    """ids of agent actions whose last logged status is not terminal (done / failed / cancelled): 'something left pending'"""
    last = {}
    for aid, _kind, status in action_hops(text):
        last[aid] = status
    return sorted(a for a, s in last.items() if s not in ("done", "failed", "cancelled"))


def running_actions(text):
    return [a for a, _k, s in action_hops(text) if s == "running"]


def row_hops(text):
    return ROW_RE.findall(text or "")


def byop_errors(text):
    """(expected, unexpected): ERROR lines of an app-log excerpt split by the [byop] failure allowlist (RP.error_lines does the level filtering)"""
    allowed_re = re.compile(BYOP_ERR_ALLOW_RE, re.I)
    lines = RP.error_lines(text)
    return [l for l in lines if allowed_re.search(l)], [l for l in lines if not allowed_re.search(l)]


# macOS test session (ssh, no login): there is no default keychain, so EVERY launch (the first one too) logs these two ERROR lines when it reads its settings; the app is
# fine (Round 8: `A default keychain could not be found`, 5 secure-storage lines per launch, the first launch included). Dropped on macOS only, so a Linux/Windows ERROR still counts.
ENV_ERR_RE = re.compile(r"Failed to read (?:local control mode|API keys) from secure storage")


def judge_errors(text, limit=BYOP_ERR_LIMIT, platform=None):
    """failure strings: any ERROR outside the [byop] allowlist, or more than `limit` expected ones"""
    if (platform or sys.platform) == "darwin":
        text = "\n".join(l for l in (text or "").split("\n") if not ENV_ERR_RE.search(l))
    exp, unexp = byop_errors(text)
    out = ["unexpected ERROR line: %s" % l for l in unexp[:5]]
    if len(exp) > limit:
        out.append("%d expected-kind ERROR lines, allowance %d: %s" % (len(exp), limit, exp[:4]))
    return out


def judge_rows(tokens, fresh, hops, stored, on_screen=()):
    """The accounting of queued rows. tokens = the text of each row queued; fresh = every prompt the model got (fake.fresh); hops = [(row id, state)] from
    the app log; stored = the texts held by queued_prompts; on_screen = tokens seen in the input / panel. Problems (empty = fine):
      * a token sent more than once (a row sent twice, or sent and also still held);
      * a token neither sent nor held (lost);
      * the number of `fired` hops differing from the number of sent tokens (the app says it sent a row that the model never got, or the reverse)."""
    out, sent = [], 0
    ids = []
    for rid, state in hops:
        if state in ("queued", "queued_locked") and rid not in ids:
            ids.append(rid)
    for t in tokens:
        n = sum(1 for p in fresh if t in p)
        held = any(t in s for s in stored) or t in on_screen
        if n > 1:
            out.append("%r was sent to the model %d times" % (t, n))
        if n >= 1 and held:
            out.append("%r was sent to the model AND is still held as a queued row" % t)
        if n == 0 and not held:
            out.append("%r was lost: never sent and not held in the queue or the input" % t)
        sent += 1 if n else 0
    fired = sum(1 for rid, s in hops if s == "fired")
    if fired != sent:
        out.append("the app logged %d row(s) fired but the model received %d of the queued prompts" % (fired, sent))
    if len(ids) != len(tokens):
        out.append("the app logged %d queued row(s) for %d queued prompt(s)" % (len(ids), len(tokens)))
    return out


def sqlite_ro(path):
    return sqlite3.connect("file:%s?mode=ro" % path.replace("\\", "/"), uri=True, timeout=5)


def read_stored(paths):
    """(queued prompt texts, conversation blobs) from every warp.sqlite given. Missing tables / unreadable files read as empty (the caller reports)"""
    rows, blobs = [], []
    for p in paths:
        try:
            con = sqlite_ro(p)
        except sqlite3.Error:
            continue
        try:
            for q, dst in (("SELECT text FROM queued_prompts ORDER BY conversation_id, position", rows),
                           ("SELECT input FROM ai_queries", blobs),
                           ("SELECT conversation_data FROM agent_conversations", blobs),
                           ("SELECT CAST(task AS TEXT) FROM agent_tasks", blobs)):
                try:
                    dst.extend(str(x[0]) for x in con.execute(q).fetchall())
                except sqlite3.Error:
                    pass
        finally:
            con.close()
    return rows, blobs


# ---------------------------------------------------------------- helpers
def need_fake(ctx, r):
    if os.environ.get("APPCI_FAKE_MODEL_ACTIVE") != "1" or FMOD.FakeModel.ACTIVE is None:
        r.skip("the fake model is not running (APPCI_FAKE_MODEL=1 starts fake_model.py as the agent provider; these cases inject faults into it and never use a real provider)")
    AG.need_provider(ctx, r)
    if ctx.log_view_mode() is not None and ctx.log_view_mode() != "agent":
        r.skip("the pane is not in the agent view (hop=view_mode says %s)" % ctx.log_view_mode())


def fake():
    return FMOD.FakeModel.ACTIVE


def ask(ctx, text):
    AG.type_fast(ctx, text)


def implicit_in_progress(state, extra=()):
    """True for in_progress, for a state in `extra`, and for None: a new conversation starts InProgress and logs no hop=turn until it changes"""
    return state is None or state == "in_progress" or state in extra


def wait_turn(ctx, off, want, timeout):
    """wait until the last hop=turn state since `off` is in `want`; returns the last state seen"""
    ctx.until(lambda: last_turn_state(ctx.log_since(off)) in want, timeout, step=0.25)
    return last_turn_state(ctx.log_since(off))


def block_text_check(ctx, r, tag):
    """a note on top of the log oracles: the block shows SOME error wording (OCR, when an engine exists)"""
    t = ctx.ocr(tag)
    if t is None:
        r.note("no OCR engine: the error block's wording is checked from the app log only (shots/agent_%s.png)" % tag)
        return
    low = " ".join(t.lower().split())
    r.check(any(w in low for w in GENERIC_ERROR_WORDS), "the block shows no error wording after the failure (OCR saw %r)" % low[:160])


def settle_quiet(ctx, f, secs=8):
    """True when the fake model got no request for `secs`: nothing retries on its own after the turn ended"""
    n = f.requests
    time.sleep(secs)
    return f.requests == n


def fault_body(ctx, r, name, kind, arm, off_wait, expect_log, word_hint, min_requests=2):
    """shared body of the failing-reply cases. arm(f, tok) injects the fault; the prompt carries tok; the turn must end in `error`."""
    need_fake(ctx, r)
    f = fake()
    AG.fresh_conversation(ctx, r)
    try:
        f.clear_faults()
        tok = "F" + ctx.tok(6).upper()
        arm(f, tok)
        off, req0, fired0 = ctx.log_size(), f.requests, len(f.fired)
        t0 = time.time()
        ask(ctx, "Reply with the word %s" % tok)
        if not AG.wait_marker(ctx, off, "[byop] opening stream", timeout=60):
            r.check(False, "no model request within 60 s of the prompt (prompt not routed to the agent)")
            return
        state = wait_turn(ctx, off, ("error", "cancelled", "success"), off_wait)
        r.metric("%s_turn_s" % name, round(time.time() - t0, 1))
        ctx.shot("%s_failed" % name)
        # A cut stream after output is surfaced (the chunk-error line) and, once actions/text arrived, the app RESUMES the conversation once (recovery_action; pinned by
        # exhausted_in_stream_recovery_is_terminal_but_a_mid_stream_drop_still_resumes): the turn then ends `success` from the resumed request, never from the cut one.
        resumed = kind == "disconnect" and "recovery=resume" in ctx.log_since(off)
        r.check(state == "error" or (resumed and state == "success"), "the turn ended in %r (hop=turn), expected `error` after the %s fault" % (state, kind))
        hit = len(f.fired) - fired0
        r.metric("%s_fault_hits" % name, hit)
        r.metric("%s_requests" % name, f.requests - req0)
        if kind != "down":
            r.check(hit >= 1, "the fault never fired (the model got no matching request)")
        if min_requests > 1 and kind in ("status",):
            r.check(f.requests - req0 >= min_requests, "no retry before the error: the model got %d request(s) (retry-before-first-byte, stream_recovery.rs)" % (f.requests - req0))
        if kind == "disconnect":
            r.check(f.requests - req0 == (2 if resumed else 1), "a stream cut AFTER output was sent was re-sent %d requests (at most ONE resume, never a verbatim retry)" % (f.requests - req0))
        log = ctx.log_since(off)
        r.check(re.search(expect_log, log, re.I) is not None, "the app log has no failure line matching %r (the error was not surfaced)" % expect_log)
        for p in judge_errors(log):
            r.check(False, p)
        block_text_check(ctx, r, "%s_ocr" % name)
        r.check(not open_actions(log), "agent action(s) left pending after the failure: %s" % open_actions(log))
        r.check(settle_quiet(ctx, f, 8), "the app sent another request on its own after the turn ended in error (an endless retry)")
        r.check(last_turn_state(ctx.log_since(off)) == state, "the turn state changed on its own after it ended: %r (was %r)" % (last_turn_state(ctx.log_since(off)), state))
        # retry: the fault is gone, the same prompt is sent again and the conversation answers
        f.clear_faults()
        f.come_up()
        off2, req1 = ctx.log_size(), f.requests
        tok2 = "R" + ctx.tok(6).upper()
        ask(ctx, "Reply with the word %s" % tok2)
        r.check(AG.wait_marker(ctx, off2, "[byop] opening stream", timeout=60), "the re-sent prompt made no model request")
        s2 = wait_turn(ctx, off2, ("success", "error", "cancelled"), 60)
        r.check(s2 == "success", "after the retry the turn ended in %r, expected `success`" % s2)
        r.check(f.fresh_with(tok2) == 1, "the retry prompt reached the model %d times (expected once)" % f.fresh_with(tok2))
        r.check(f.requests - req1 == 1, "the retry made %d request(s), expected 1" % (f.requests - req1))
        r.check(not judge_errors(ctx.log_since(off2)), "ERROR line(s) during the successful retry: %s" % judge_errors(ctx.log_since(off2)))
        ctx.shot("%s_retried" % name)
        r.check(not open_actions(ctx.log_since(off)), "agent action(s) left pending at the end")
    finally:
        f.clear_faults()
        f.come_up()
        AG.cleanup_tmp(ctx)
        ctx.be.key("ctrl+c")


@case("agent_fault_429", modes=("agent",), order=52, gui=True)
def c_fault_429(ctx, r):
    """HTTP 429 + Retry-After on every try: the app retries before the first byte (stream_recovery.rs, Retry-After replaces the wait), then ends the turn with a
    clear error (hop=turn error, `[byop] open stream failed` naming 429); nothing is left pending, no endless retry; a re-sent prompt succeeds exactly once."""
    fault_body(ctx, r, "f429", "status", lambda f, tok: f.inject("status", times=None, match=tok, code=429, retry_after=1), 120, r"open stream failed.*429|429.*open stream failed|429", "rate")


@case("agent_fault_500", modes=("agent",), order=52, gui=True)
def c_fault_500(ctx, r):
    """HTTP 500 on every try: 2 s + 4 s + 8 s retries, then a clear error naming the server status; the retry of the prompt succeeds exactly once."""
    fault_body(ctx, r, "f500", "status", lambda f, tok: f.inject("status", times=None, match=tok, code=500), 150, r"open stream failed.*500|500.*open stream failed|\b500\b", "server")


@case("agent_fault_disconnect", modes=("agent",), order=52, gui=True)
def c_fault_disconnect(ctx, r):
    """The connection is cut after some text was streamed: NO retry (a re-send would repeat output), the turn ends in `error` with the incomplete / closed
    wording, the text already shown stays, nothing is pending; a re-sent prompt succeeds."""
    fault_body(ctx, r, "fdisc", "disconnect", lambda f, tok: f.inject("disconnect", times=1, match=tok, after=3), 90, r"stream chunk error|without a terminal End|connection closed|error decoding", "incomplete")


@case("agent_fault_down", modes=("agent",), order=52, gui=True)
def c_fault_down(ctx, r):
    """The fake model is down (connection refused) when the prompt is sent: the app retries (transport failure before the first byte), then ends the turn
    with a clear unreachable error; nothing is pending; with the model back up the re-sent prompt succeeds."""
    def arm(f, tok):
        f.go_down()
    fault_body(ctx, r, "fdown", "down", arm, 150, r"open stream failed|stream chunk error|error sending request|Connection refused|unreachable", "unreachable", min_requests=0)


@case("agent_fault_slow_cancel", modes=("agent",), order=52, gui=True)
def c_fault_slow_cancel(ctx, r):
    """The model sends nothing for 30 s: the turn is in progress with no text; Ctrl-C ends it (hop=turn cancelled) within 5 s AND the app hangs up the
    connection (fake.aborted: the cancel reached the wire, not only the UI); no request follows on its own; a new prompt works."""
    need_fake(ctx, r)
    f = fake()
    AG.fresh_conversation(ctx, r)
    try:
        f.clear_faults()
        tok = "S" + ctx.tok(6).upper()
        f.inject("delay", times=1, match=tok, secs=30)
        off, ab0, req0 = ctx.log_size(), f.aborted, f.requests
        ask(ctx, "Reply with the word %s" % tok)
        if not AG.wait_marker(ctx, off, "[byop] opening stream", timeout=60):
            r.check(False, "no model request within 60 s of the prompt")
            return
        r.check(ctx.until(lambda: f.fired and f.fired[-1] == "delay", 10), "the delayed request never reached the fake model")
        time.sleep(3.0)
        # hop=turn logs only on a CHANGE, and a fresh conversation is created already InProgress, so its first turn logs no line: no line since `off` means still in progress.
        r.check(last_turn_state(ctx.log_since(off)) in (None, "in_progress"), "during the first-token delay the turn is %r, expected in_progress (or no change from the new conversation's initial in_progress)" % last_turn_state(ctx.log_since(off)))
        ctx.shot("slow_waiting")
        t0 = time.time()
        coff = ctx.log_size()
        ctx.be.key("ctrl+c")
        ctx.ctrlc_ts.append(time.time())
        state = wait_turn(ctx, off, ("cancelled", "error", "success"), 8)
        took = round(time.time() - t0, 2)
        r.metric("cancel_to_state_s", took)
        r.check(state == "cancelled", "Ctrl-C during the delay left the turn %r, expected `cancelled` (hop=turn)" % state)
        r.check(took <= 5.0, "the cancel took %.1f s (bound 5 s)" % took)
        routes = AG.ctrl_c_routes(ctx.log_since(coff))
        err = AG.cancel_route_error([routes])
        if err:
            r.check(False, err)
        r.check(ctx.until(lambda: f.aborted > ab0, 5), "the app did not hang up the connection within 5 s of the cancel (fake.aborted unchanged): the request is still open")
        ctx.shot("slow_cancelled")
        r.check(settle_quiet(ctx, f, 6) and f.requests == req0 + 1, "a request was made after the cancel without any input (requests %d -> %d)" % (req0 + 1, f.requests))
        r.check(not open_actions(ctx.log_since(off)), "agent action(s) left pending after the cancel")
        for p in judge_errors(ctx.log_since(off)):
            r.check(False, p)
        # a new prompt on the same conversation works
        off2 = ctx.log_size()
        tok2 = "N" + ctx.tok(6).upper()
        ask(ctx, "Reply with the word %s" % tok2)
        r.check(AG.wait_marker(ctx, off2, "[byop] opening stream", timeout=60), "the prompt after the cancel made no model request")
        s2 = wait_turn(ctx, off2, ("success", "error", "cancelled"), 60)
        r.check(s2 == "success" and f.fresh_with(tok2) == 1, "after the cancel the next prompt ended %r with %d model request(s) carrying it" % (s2, f.fresh_with(tok2)))
    finally:
        f.clear_faults()
        AG.cleanup_tmp(ctx)
        ctx.be.key("ctrl+c")


def queue_rows(ctx, r, f, tokens, off):
    """type `/queue <token>` for each token; True once the app logged a queued_row hop for each (the preconditions: the rows exist)"""
    for t in tokens:
        ask(ctx, "/queue please answer with the word %s" % t)
        time.sleep(0.7)
    if not ctx.until(lambda: len({i for i, s in row_hops(ctx.log_since(off)) if s in ("queued", "queued_locked")}) >= len(tokens), 12):
        ctx.shot("queue_rows_missing")
        r.skip("the %d /queue row(s) did not appear (no hop=queued_row queued within 12 s): the queue precondition cannot be proven, so nothing is judged" % len(tokens))


@case("agent_fault_queued_rows", modes=("agent",), order=52, gui=True)
def c_fault_queued_rows(ctx, r):
    """Two /queue rows wait behind a reply that fails (HTTP 500 on every try for the first prompt only). Whatever the app decides to do with them (send in
    order, hold them locked/suspended, or pop them back into the input), each row is accounted for exactly once: sent exactly once XOR still held, never lost,
    never sent twice, and the app's `fired` hops equal the prompts the model actually got. Nothing is left pending."""
    need_fake(ctx, r)
    f = fake()
    AG.fresh_conversation(ctx, r)
    try:
        f.clear_faults()
        tok = "Q" + ctx.tok(6).upper()
        rows = ["QA" + ctx.tok(6).upper(), "QB" + ctx.tok(6).upper()]
        f.inject("status", times=None, match=tok, code=500)
        off = ctx.log_size()
        ask(ctx, "Reply with the word %s" % tok)
        if not AG.wait_marker(ctx, off, "[byop] opening stream", timeout=60):
            r.check(False, "no model request within 60 s of the prompt")
            return
        if not implicit_in_progress(last_turn_state(ctx.log_since(off))):
            r.skip("the first prompt is already %r when the rows are typed: /queue sends at once when no reply is running (no hop=queued_row), so there is nothing to queue behind "
                   "(the 500 fault did not hold the reply open: a fake model that does not see the prompt text, round 6)" % last_turn_state(ctx.log_since(off)))
        queue_rows(ctx, r, f, rows, off)                 # during the 14 s retry schedule the first prompt is still in progress
        r.check(implicit_in_progress(last_turn_state(ctx.log_since(off)), ("error",)), "unexpected turn state while queueing: %r" % last_turn_state(ctx.log_since(off)))
        state = wait_turn(ctx, off, ("error", "success", "cancelled"), 150)
        r.check(state == "error", "the failing prompt ended %r, expected `error`" % state)
        time.sleep(10)                                   # rows the app chooses to send go out inside this window
        f.clear_faults()
        time.sleep(4)
        ctx.shot("queued_after_failure")
        log = ctx.log_since(off)
        stored, _ = read_stored(CA.find_db(ctx))
        on_screen = tuple(t for t in rows if AG.ocr_has(ctx, "queued_ocr_%s" % t, t))
        probs = judge_rows(rows, f.fresh, row_hops(log), stored, on_screen)
        for p in probs:
            r.check(False, p)
        r.metric("queued_rows_hops", row_hops(log))
        r.metric("queued_rows_sent", [t for t in rows if f.fresh_with(t)])
        r.metric("queued_rows_stored", [t for t in rows if any(t in s for s in stored)])
        r.check(not [l for l in judge_errors(log)], "ERROR lines: %s" % judge_errors(log))
        r.check(not open_actions(log), "agent action(s) left pending")
        r.check(last_turn_state(log) in ("error", "success"), "conversation not idle at the end: %r" % last_turn_state(log))
    finally:
        f.clear_faults()
        AG.cleanup_tmp(ctx)
        ctx.be.key("ctrl+c")


# ================================================================= #1832: restart mid-conversation
SCENARIOS = ("stream", "queued", "blocked")


def restart_body(ctx, r, how, scenario):
    R.need_relaunch_app(ctx, "agent_restart_%s_%s" % (how, scenario))
    if how == "quit":
        R.need_linux_app(ctx, "a clean quit through the app's own quit key (the harness has no usable quit key on macOS / Windows)")
    need_fake(ctx, r)
    f = fake()
    dbs_why = None
    rows = []
    label = "ar_%s_%s" % (how, scenario)
    try:
        f.clear_faults()
        q = R.stop_for_restart(ctx, r, 20, label + "_setup")
        r.check(q["exited"], "%s: the app did not exit for the restore setup (%r)" % (label, q))
        if not q["exited"]:
            return
        R.set_restore(ctx, True)
        _, up = R.start_app(ctx, r, label + "_first")
        if not up:
            return
        AG.fresh_conversation(ctx, r)
        # 1. one completed turn: the conversation that must come back
        prime = "P" + ctx.tok(6).upper()
        off = ctx.log_size()
        ask(ctx, "Reply with the word %s" % prime)
        AG.wait_marker(ctx, off, "[byop] opening stream", timeout=60)
        if wait_turn(ctx, off, ("success", "error", "cancelled"), 60) != "success":
            r.check(False, "%s: the priming turn did not complete (hop=turn %r)" % (label, last_turn_state(ctx.log_since(off))))
            return
        time.sleep(2.0)
        # 2. the scenario
        rows, eff_tok, eff_name = [], None, "ar_blk_%s.txt" % ctx.tok(5).lower()
        ctx.rm(eff_name)
        if scenario in ("stream", "queued"):
            tok = "D" + ctx.tok(6).upper()
            f.inject("drip", times=1, match=tok, interval=0.5, words=120)
            off = ctx.log_size()
            ask(ctx, "Reply with the word %s" % tok)
            if not AG.wait_marker(ctx, off, "stream opened OK", timeout=60):
                r.check(False, "%s: the streaming reply never opened" % label)
                return
            time.sleep(3.0)
            r.check(last_turn_state(ctx.log_since(off)) == "in_progress", "%s: not mid-reply before the stop (hop=turn %r)" % (label, last_turn_state(ctx.log_since(off))))
            if scenario == "queued":
                rows = ["QA" + ctx.tok(6).upper(), "QB" + ctx.tok(6).upper()]
                queue_rows(ctx, r, f, rows, off)
        else:
            eff_tok = "BLK" + ctx.tok(6).upper()
            off = ctx.log_size()
            ask(ctx, "Please run exactly this shell command and nothing else: echo %s >> %s" % (eff_tok, R.fwd(os.path.join(ctx.W, eff_name))))
            if not AG.wait_marker(ctx, off, "queue_actions", timeout=60):
                ctx.shot(label + "_no_card")
                r.skip("%s: no approval card within 60 s (the precondition 'blocked on a command' cannot be proven)" % label)
            time.sleep(1.5)
            r.check(last_turn_state(ctx.log_since(off)) == "blocked", "%s: the conversation is %r with the card up, expected blocked" % (label, last_turn_state(ctx.log_since(off))))
            rows = ["FU" + ctx.tok(6).upper()]
            ask(ctx, "follow up question %s" % rows[0])
            if not ctx.until(lambda: any(s in ("queued", "queued_locked") for _i, s in row_hops(ctx.log_since(off))), 10):
                ctx.shot(label + "_no_row")
                r.skip("%s: typing under the blocked card made no queued row (hop=queued_row): the queue precondition cannot be proven" % label)
        time.sleep(2.5)                                           # the sqlite writer has stored the rows
        sent_before = {t: f.fresh_with(t) for t in rows}
        r.check(all(n == 0 for n in sent_before.values()), "%s: a queued row was sent before the stop: %r (nothing to test)" % (label, sent_before))
        pre_log = ctx.log_since(off)
        r.check(not (eff_tok and ctx.lines(eff_name)), "%s: the blocked command ran before the stop" % label)
        ctx.shot(label + "_before_stop")
        req_before, mark = f.requests, ctx.cmd_mark()
        # 3. the stop
        if how == "quit":
            q = R.stop_for_restart(ctx, r, float(os.environ.get("APPCI_QUIT_BOUND_S", "10")), label)
            r.check(q["exited"], "%s: the app was still alive after the quit key (%r)" % (label, q))
            if not q["exited"]:
                return
            R.keep_log(ctx, label + "_quit")
        else:
            if not R.hard_kill(ctx, r, label + "_kill"):
                return
        f.clear_faults()
        # 4. what is on disk
        dbs = CA.find_db(ctx)
        if not dbs:
            dbs_why = "no warp.sqlite under %s" % R.data_roots(ctx)
        stored, blobs = read_stored(dbs)
        r.metric("%s_stored_rows" % label, len(stored))
        # 5. relaunch on the same profile
        newp, up = R.start_app(ctx, r, label + "_relaunch", restored_min=1)
        if not up:
            return
        ctx.shot(label + "_relaunched")
        time.sleep(6.0)                                           # anything the app would send or run on its own happens here
        rel = ctx.log_since(0)                                    # the relaunch overwrote the log
        stored2, blobs2 = read_stored(CA.find_db(ctx))
        # conversation restored
        if dbs_why:
            r.check(False, "%s: %s: nothing was persisted to restore" % (label, dbs_why))
        else:
            r.check(any(prime in b for b in blobs2), "%s: the completed turn %s is not in the store after the relaunch (agent_conversations / ai_queries): the conversation was not restored" % (label, prime))
        seen = AG.ocr_has(ctx, label + "_ocr", prime)
        r.note("%s: the completed turn is %s on the relaunched screen (OCR; the store check above is the verdict)" % (label, "visible" if seen else "not visible" if seen is False else "unchecked (no OCR)"))
        # queued rows: held, not sent
        if rows:
            on_screen = tuple(t for t in rows if AG.ocr_has(ctx, "%s_row_%s" % (label, t), t))
            for t in rows:
                held = any(t in s for s in stored2) or t in on_screen
                r.check(held, "%s: queued row %r was lost across the %s (not in queued_prompts, not on the screen)" % (label, t, how))
                r.check(f.fresh_with(t) == 0, "%s: queued row %r was sent to the model %d time(s) after the restart (suspended rows must wait for the user)" % (label, t, f.fresh_with(t)))
            r.check(len([s for s in stored2 if any(t in s for t in rows)]) <= len(rows), "%s: queued rows duplicated in the store: %r" % (label, [s for s in stored2 if any(t in s for t in rows)]))
            popped = [i for i, s in row_hops(pre_log) if s == "popped"]
            if popped:
                r.note("%s: %d row(s) were popped back to the input before the stop (a cancel restores them, #690)" % (label, len(popped)))
        # nothing runs on its own
        r.check(f.requests == req_before, "%s: the app sent %d model request(s) on its own after the restart" % (label, f.requests - req_before))
        r.check(not running_actions(rel), "%s: a phantom running action after the relaunch: hop=agent_action running for %s" % (label, running_actions(rel)))
        r.check("in_progress" not in [s for _c, s in turn_states(rel)], "%s: a conversation is in progress after the relaunch with no input (hop=turn)" % label)
        r.check(not open_actions(rel), "%s: agent action(s) left open after the relaunch: %s" % (label, open_actions(rel)))
        if eff_tok:
            r.check(not ctx.lines(eff_name), "%s: the blocked command RAN on the relaunch: %r" % (label, ctx.lines(eff_name)))
            r.check(not [c for c in ctx.cmds_since(mark) if eff_tok in c], "%s: the blocked command reached the shell on the relaunch" % label)
            r.check(not R_command_writes(rel, eff_tok), "%s: the app wrote the blocked command to the pty on the relaunch" % label)
        for p in judge_errors(rel, limit=0):
            if "unexpected ERROR" in p:
                r.check(False, "%s (relaunch): %s" % (label, p))
        # the restored app is usable: a new prompt works
        tok2 = "A" + ctx.tok(6).upper()
        AG.fresh_conversation(ctx, r)
        off2 = ctx.log_size()
        ask(ctx, "Reply with the word %s" % tok2)
        r.check(AG.wait_marker(ctx, off2, "[byop] opening stream", timeout=60), "%s: a new prompt after the relaunch made no model request" % label)
        s2 = wait_turn(ctx, off2, ("success", "error", "cancelled"), 60)
        r.check(s2 == "success" and f.fresh_with(tok2) == 1, "%s: a new prompt after the relaunch ended %r with %d request(s) carrying it" % (label, s2, f.fresh_with(tok2)))
    finally:
        f.clear_faults()
        R.fresh_app(ctx, r, label)


def R_command_writes(log_text, tok):
    return AG.approved_command_writes(log_text, tok)


def _register(how, scenario):
    name = "agent_restart_%s_%s" % (how, scenario)

    def body(ctx, r):
        restart_body(ctx, r, how, scenario)
    body.__doc__ = ("%s while %s, restore_session on; relaunch on the same profile: the conversation (a completed turn) is restored, queued rows are held and not sent, "
                    "no phantom running action, the blocked command never ran (#1832)." % (
                        "A clean quit" if how == "quit" else "kill -9", {"stream": "a reply streams", "queued": "two /queue rows wait behind a streaming reply",
                                                                          "blocked": "an approval card is blocked on a command with a follow-up queued"}[scenario]))
    case(name, modes=("agent",), order=53 + (1 if how == "kill" else 0), gui=True)(body)


for _how in ("quit", "kill"):
    for _sc in SCENARIOS:
        _register(_how, _sc)
