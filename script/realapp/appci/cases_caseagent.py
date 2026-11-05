# appci/cases_caseagent.py (#1463): agent mode end to end with the CI Ollama provider, and the autodetect routing table.
# Registered with the same @case decorator as cases.py (imported at the end of cases.py). Evidence, in order of strength:
#   * the shell's own rec.log (what the shell RECEIVED, from the scratch rc), never "something ran";
#   * the app's debug log: 'phosphor::routing' enter-stage decision lines (route, rule, surface, head word, byte count) and the byop
#     request markers ('[byop] opening stream', 'stream opened OK', '[byop-diag] queue_actions' = an action card is pending);
#   * screenshots (+ OCR when an engine exists; without one the OCR sub-checks are notes, not passes).
# Typing is at the owner's stress rate (10-25 ms/key, Enter in the same call). Shell commands are never typed into a busy agent
# conversation (that steers the agent): every agent-bound text here is prose, and shell-bound lines are typed only when idle.
import hashlib, os, re, shutil, time

import cases
from cases import case

ROUTE_RE = re.compile(r'enter: (\w+) by (.*?) \((\w+), head "(.*?)", (\d+) bytes\)')


def need_provider(ctx, r):
    if not ctx.ollama:
        r.skip("no provider: configure APPCI_OLLAMA_URL (env or the machine-local ~/.config/phosphor-ci/ci.env) to run agent cases that need a model")
    if not ctx.log_path or not os.path.exists(ctx.log_path):
        r.skip("no app log at %s: the request markers cannot be read" % ctx.log_path)


def fresh_conversation(ctx, r):
    """r18u8 (Mac agent_approval): the agent pass is ONE conversation, so the model saw the earlier cases' prompts (approval_card_pause's `echo APPROVAL_TOKEN >> ap.txt`,
    the routing prose) and proposed the STALE command instead of the one the case asked for. A case that needs the model to do exactly what it is told starts a new
    conversation: Escape (leaves the agent view for the terminal session, shown as `ESC for terminal`), then `/agent` (the terminal hint `new /agent conversation`).
    The result is proven by the app's hop=view_mode line; if the new conversation cannot be proven the case SKIPS (it is never run on the stale one)."""
    if not ctx.log_path:
        return
    ctx.reset()
    time.sleep(0.4)
    ctx.be.key("Escape")
    if not ctx.until(lambda: ctx.log_view_mode() == "terminal", 6):
        r.skip("fresh conversation: Escape did not leave the agent view (hop=view_mode still %s), so a new conversation could not be started" % ctx.log_view_mode())
    time.sleep(0.5)
    ctx.be.type("/agent", 12, enter=True)
    if not ctx.until(lambda: ctx.log_view_mode() == "agent", 10):
        r.skip("fresh conversation: `/agent` did not open a new agent conversation within 10 s (hop=view_mode says %s)" % ctx.log_view_mode())
    time.sleep(1.0)
    r.note("a fresh agent conversation was started (Escape, /agent) so earlier cases' prompts are not in the model's context")


def type_fast(ctx, text, enter=True):
    ctx.be.type(text, ctx.rng.randint(10, 25), enter=enter)


def count_in_log(ctx, off, needle):
    return ctx.log_since(off).count(needle)


def shot_hash(ctx, tag):
    d = os.path.join(ctx.prof.shots, "_tmp")
    os.makedirs(d, exist_ok=True)
    p = os.path.join(d, "%s.png" % tag)
    try:
        ctx.be.shot(p)
        return hashlib.md5(open(p, "rb").read()).hexdigest()
    except Exception:
        return None


CARET_DIFF = 0.0008      # fraction of pixels: a blinking caret is ~0.00002 of a screen, streaming text moves orders of magnitude more


def frame_paths(ctx, tag, n, step):
    """n screenshots `step` s apart, each kept under its own name so consecutive ones can be compared; returns the paths (a failed shot is skipped)"""
    d = os.path.join(ctx.prof.shots, "_tmp")
    os.makedirs(d, exist_ok=True)
    out = []
    for i in range(n):
        p = os.path.join(d, "%s_%d.png" % (tag, i))
        try:
            ctx.be.shot(p)
            out.append(p)
        except Exception:
            pass
        if i < n - 1:
            time.sleep(step)
    return out


CTRL_C_ROUTE_RE = re.compile(r"hop=ctrl_c_route route=(\w+)")
CANCEL_ROUTES = ("cancel_conversation", "stop_conversation_and_command")


def ctrl_c_routes(text):
    """the `hop=ctrl_c_route route=<name>` names in an app-log excerpt, in order ([] on a build without the hop)"""
    return CTRL_C_ROUTE_RE.findall(text)


def cancel_route_error(routes):
    """routes = per Ctrl-C press, the routes the app logged. Owner rule (#851): while an agent reply runs, Ctrl-C stops the reply. So the first press
    that logged a route must have cancelled the conversation, and no press may have sent the ^C to the pty. None = fine, or nothing logged (old build)."""
    logged = [(i + 1, rs) for i, rs in enumerate(routes) if rs]
    if not logged:
        return None
    for press, rs in logged:
        if "pty" in rs:
            return "Ctrl-C #%d while the reply streamed went to the pty (app route %s), not to the conversation (#851: Ctrl-C stops the reply)" % (press, ",".join(rs))
    press, rs = logged[0]
    if not any(x in CANCEL_ROUTES for x in rs):
        return "Ctrl-C #%d while the reply streamed did not cancel the conversation (app route %s; #851: Ctrl-C stops the reply)" % (press, ",".join(rs))
    return None


def frames_judged_by_pixels():
    import cases_ui as U
    return U.have_pillow()


def U_PILLOW_MSG():
    import cases_ui as U
    return U.PILLOW_MSG


def frames_settled(paths):
    """(settled, distinct): is the screen still? r18u8 (Windows H18): a blinking caret produced 3 distinct md5s in 5 frames and the old `<= 2 distinct` rule called a
    stopped stream 'still changing'. With Pillow the frames are compared by changed-pixel fraction (a caret is ~0.00002, a streaming reply far above CARET_DIFF);
    without it the rule is unchanged (<= 2 distinct hashes), never looser."""
    import cases_ui as U
    hs = []
    for p in paths:
        try:
            hs.append(hashlib.md5(open(p, "rb").read()).hexdigest())
        except OSError:
            pass
    distinct = len(set(hs))
    if U.have_pillow() and len(paths) >= 2:
        diffs = [U.img_diff(a, b) for a, b in zip(paths, paths[1:])]
        if all(d is not None for d in diffs):
            return max(diffs) <= CARET_DIFF, distinct
    return distinct <= 2, distinct


def settled(ctx, tag, quiet=5, step=1.5, timeout=90):
    """True once the screen stops changing (frames_settled over the last `quiet` shots: a blinking caret does not count as change)"""
    t0 = time.time()
    while time.time() - t0 < timeout:
        ok, _ = frames_settled(frame_paths(ctx, tag, quiet, step))
        if ok:
            return True
    return False


def cleanup_tmp(ctx):
    shutil.rmtree(os.path.join(ctx.prof.shots, "_tmp"), ignore_errors=True)


def wait_marker(ctx, off, needle, n=1, timeout=60):
    return ctx.until(lambda: count_in_log(ctx, off, needle) >= n, timeout, step=0.25)


def _squash(text):
    return "".join(c for c in text.lower() if c.isalnum())


def _edit_distance(a, b):
    prev = list(range(len(b) + 1))
    for i, ca in enumerate(a, 1):
        cur = [i]
        for j, cb in enumerate(b, 1):
            cur.append(min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (ca != cb)))
        prev = cur
    return prev[-1]


def fuzzy_has(text, needle, max_ratio=0.25):
    """needle in an OCR text, tolerating OCR confusions (r18d2 Wayland: the input showed 'keepmet115_qm09d8', tesseract read 'keepmet115_qmoods'; the
    exact substring test failed on a screen that was right). Punctuation and case are dropped, then a window of the needle's length (+-2) must lie
    within max_ratio edit distance. Short needles must match exactly."""
    n = _squash(needle)
    if not n:
        return False
    t = _squash(text)
    if n in t:
        return True
    if len(n) < 10:
        return False
    allow = int(len(n) * max_ratio)
    for w in range(len(n) - 2, len(n) + 3):
        for i in range(0, max(0, len(t) - w) + 1):
            if _edit_distance(n, t[i:i + w]) <= allow:
                return True
    return False


def ocr_has(ctx, tag, needle):
    t = ctx.ocr(tag)
    if t is None:
        return None
    return fuzzy_has(" ".join(t.split()), needle)


def setting_bool(ctx, key):
    """a boolean from the profile's settings.toml (only the boolean is read out; the file is never printed). None if absent."""
    for sub in (".config/phosphor/settings.toml", ".phosphor/settings.toml"):
        try:
            t = open(os.path.join(ctx.prof.home, sub), errors="replace").read()
        except OSError:
            continue
        m = re.search(r"^\s*%s\s*=\s*(true|false)\b" % re.escape(key), t, re.M)
        if m:
            return m.group(1) == "true"
    return None


# ---------------------------------------------------------------- agent: stream, type-while-streaming, cancel
@case("agent_stream_cancel", modes=("agent",), order=46, gui=True)
def c_agent_stream_cancel(ctx, r):
    """A real prompt streams a reply; keys typed while it streams land in the input (no steering = no new request, nothing
    reaches the shell); Ctrl-C (terminal:cancel_command) stops the stream (the screen stops changing). First-token latency is a metric."""
    need_provider(ctx, r)
    fresh_conversation(ctx, r)
    try:
        off = ctx.log_size()
        mark = ctx.cmd_mark()
        t0 = time.time()
        type_fast(ctx, "Write a long, detailed essay of at least 800 words about the history of the bicycle, with a heading per century.")
        if not wait_marker(ctx, off, "[byop] opening stream", timeout=60):
            ctx.shot("no_request")
            r.check(False, "no model request ('[byop] opening stream') within 60 s of the prompt's Enter; prompt not routed to the agent or provider unreachable")
            return
        r.metric("request_start_s", round(time.time() - t0, 2))
        if wait_marker(ctx, off, "stream opened OK", timeout=90):
            r.metric("stream_opened_s", round(time.time() - t0, 2))      # HTTP accepted; the first token follows (model speed varies: metric, not verdict)
        elif "stream chunk error" in ctx.log_since(off):
            r.skip("provider error (see the app log '[byop] stream chunk error'): the CI Ollama did not answer; not an app verdict")
        hs, t1 = [], time.time()
        while time.time() - t1 < 90 and len(set(hs)) < 3:
            h = shot_hash(ctx, "streaming")
            if h:
                hs.append(h)
            time.sleep(1.0)
        if len(set(hs)) < 3:
            r.skip("the reply never visibly streamed within 90 s (%d distinct frames); model/provider too slow to test cancel" % len(set(hs)))
        r.metric("first_visible_change_s", round(time.time() - t0, 2))
        # keys typed while the reply streams must land in the input
        tok = ctx.tok(6).lower()
        typed = "keepme" + tok
        before_req = count_in_log(ctx, off, "[byop] opening stream")
        type_fast(ctx, typed, enter=False)
        time.sleep(1.0)
        ctx.shot("typed_while_streaming")
        seen = ocr_has(ctx, "typed_ocr", typed)
        if seen is None:
            r.note("no OCR engine: typed-while-streaming landing is screenshot-only (shots/agent_typed_while_streaming.png)")
        else:
            r.check(seen, "text typed at 10-25 ms/key while the reply streamed is not visible in the input (%r)" % typed)
        r.check(count_in_log(ctx, off, "[byop] opening stream") == before_req, "typing during the reply started a new model request (steering)")
        r.check(not [c for c in ctx.cmds_since(mark) if typed in c], "text typed during the reply reached the shell")
        # cancel. r18u8 (Mac): the screenshot after the FIRST Ctrl-C shows the stream already stopped and the typed text still in the input, so while a reply streams the
        # first Ctrl-C CANCELS the stream and the second clears the input; the old comment ("first only clears") had the order backwards. The case does not
        # assume an order: after each press it measures both facts (stream stopped = the screen settles; typed text visible by OCR), allows at most 2 presses,
        # and requires the end state: stream stopped AND typed text gone, nothing at the shell.
        def screen_settled(tag):
            return frames_settled(frame_paths(ctx, tag, 5, 1.2))
        # r18u13: the app logs where each ^C went (`hop=ctrl_c_route route=<name>`, view.rs ctrl_c_to_active_block); a build without the hop logs none.
        stopped, nframes, cleared, routes = False, -1, None, []
        for press in (1, 2):
            press_off = ctx.log_size()
            ctx.be.key("ctrl+c")
            time.sleep(1.5)
            ctx.shot("after_ctrl_c_%d" % press)
            cleared = ocr_has(ctx, "after_ctrl_c_%d_ocr" % press, typed)
            stopped, nframes = screen_settled("post_cancel_%d" % press)
            routes.append(ctrl_c_routes(ctx.log_since(press_off)))
            r.note("Ctrl-C #%d: stream %s (%d distinct frames in 5), typed text %s, app route %s" % (
                press, "stopped" if stopped else "still changing", nframes, "not checkable (no OCR)" if cleared is None else ("still in the input" if cleared else "gone"),
                ",".join(routes[-1]) or "not logged"))
            if stopped and cleared is not True:
                break
        r.metric("distinct_frames_after_cancel", nframes)
        r.metric("ctrl_c_routes", routes)
        err = cancel_route_error(routes)
        if err:
            r.check(False, err)
        if not stopped and frames_judged_by_pixels():
            r.check(False, "the screen kept changing after two Ctrl-C presses (%d distinct frames in 5): the stream did not stop" % nframes)
        elif not stopped:
            # r18u13 (Mac): without Pillow the frames are compared by md5, and a full-screen capture has more than the caret blinking (the menu-bar capture
            # indicator): 3 distinct hashes on a stopped stream (the app log showed the 120 fps stream redraw ending at the first ^C). md5 can prove a still
            # screen (<= 2 hashes), never a moving one: no verdict.
            r.unverifiable("stream stop not judged: %d distinct md5 frames in 5 and no Pillow to measure how much changed (%s)" % (nframes, U_PILLOW_MSG()))
        if cleared is True:
            r.check(False, "the typed text (%r) is still in the input after two Ctrl-C presses: the second press did not clear it" % typed)
        r.check(not [c for c in ctx.cmds_since(mark) if "bicycle" in c or typed in c], "agent text or the typed text reached the shell")
    finally:
        cleanup_tmp(ctx)


# ---------------------------------------------------------------- agent: approval card, approve once / reject once, follow-up
def ask_command(ctx, r, off, token, fname, label):
    ctx.rm(fname)
    type_fast(ctx, "Please run exactly this shell command and nothing else: echo %s >> %s" % (token, fname))
    if not wait_marker(ctx, off, "queue_actions", timeout=120):
        ctx.shot("%s_no_card" % label)
        r.skip("%s: no approval card within 120 s (the model did not propose the command, or auto-approve ran it)" % label)
    time.sleep(1.5)
    ctx.shot("%s_card" % label)


@case("agent_approval", modes=("agent",), order=47, gui=True)
def c_agent_approval(ctx, r):
    """Approve once: the command appears in the shell's rec.log exactly once and writes its effect file exactly once; the first
    Enter on the empty input only focuses the card (#1195/#1350); nothing ran before the approval. Then a follow-up question in the
    SAME conversation opens a new model request. Reject once: the rejected command never reaches the shell."""
    need_provider(ctx, r)
    fresh_conversation(ctx, r)
    try:
        off = ctx.log_size()
        mark = ctx.cmd_mark()
        tok_a, tok_r = "APPROVEME" + ctx.tok(6).upper(), "REJECTME" + ctx.tok(6).upper()
        ask_command(ctx, r, off, tok_a, "ap_ok.txt", "approve")
        r.check(not ctx.lines("ap_ok.txt") and not [c for c in ctx.cmds_since(mark) if tok_a in c], "the command ran before it was approved")
        # focus the card with Enter on the empty input, then approve with a second Enter
        ctx.be.key("Return")
        time.sleep(1.0)
        ctx.shot("approve_focused")
        r.check(not [c for c in ctx.cmds_since(mark) if tok_a in c], "the first Enter on the empty input approved the card (it must only focus it, #1195/#1350)")
        ctx.be.key("Return")
        ran = ctx.until(lambda: ctx.lines("ap_ok.txt"), 30)
        time.sleep(3)           # a duplicate would land here
        got = ctx.lines("ap_ok.txt")
        shell_cmds = [c for c in ctx.cmds_since(mark) if tok_a in c]
        ctx.shot("approve_done")
        r.check(bool(ran) and got == [tok_a], "approved command effect file: expected [%r], got %r" % (tok_a, got))
        r.check(len(shell_cmds) == 1, "approved command appears %d times in the shell's rec.log (expected exactly once): %r" % (len(shell_cmds), shell_cmds[:3]))
        # follow-up in the same conversation (wait for the agent to finish reporting first: never type into a busy conversation)
        wait_marker(ctx, off, "[byop] opening stream", n=2, timeout=60)
        if not settled(ctx, "idle1", timeout=120):
            r.note("the conversation did not go idle within 120 s after the approved command; follow-up skipped")
        else:
            tok_f = ctx.tok(6).upper()
            reqs = count_in_log(ctx, off, "[byop] opening stream")
            t0 = time.time()
            type_fast(ctx, "Reply with just the word DONE%s and nothing else." % tok_f)
            ok = wait_marker(ctx, off, "[byop] opening stream", n=reqs + 1, timeout=60)
            r.check(ok, "the follow-up question in the same conversation did not open a model request within 60 s")
            if ok:
                r.metric("followup_request_s", round(time.time() - t0, 2))
                settled(ctx, "idle2", timeout=120)
                seen = ocr_has(ctx, "followup", "DONE" + tok_f)
                r.note("follow-up reply %s on screen (OCR; model wording varies, not a verdict)" % ("visible" if seen else "not visible" if seen is False else "unchecked (no OCR)"))
            r.check(not [c for c in ctx.cmds_since(mark) if "Reply with" in c or "DONE" + tok_f in c], "follow-up text reached the shell")
            settled(ctx, "idle3", timeout=60)
        # reject once
        off2 = ctx.log_size()
        mark2 = ctx.cmd_mark()
        ask_command(ctx, r, off2, tok_r, "ap_rej.txt", "reject")
        ctx.be.key("Return")            # focus the card
        time.sleep(1.0)
        ctx.be.key("ctrl+c")            # Reject (card focused), else cancels the conversation: either way it must not run
        time.sleep(4)
        ctx.shot("reject_done")
        r.check(not ctx.lines("ap_rej.txt"), "the rejected command's effect file exists: %r" % ctx.lines("ap_rej.txt"))
        r.check(not [c for c in ctx.cmds_since(mark2) if tok_r in c], "the rejected command reached the shell's rec.log")
    finally:
        cleanup_tmp(ctx)
        ctx.be.key("ctrl+c")


# ---------------------------------------------------------------- autodetect routing table
SHELL_LINES = ["ls -la", "git status", "grep foo foo.txt", "echo hello world", "pwd", "cat foo.txt", "wc -l foo.txt", "uname -a", "ls the", "date",
               # #1560 (owner): clear shell structure beats an unknown head; the shell says "command not found" itself
               "print -r -- $EPOCHREALTIME > lat_0", "notacmd_x $HOME/x", "xyz_nocmd > out_nc.txt", "foo_nocmd --bar | grep x"]
PROSE_LINES = ["what does this error mean", "how do I undo my last git commit", "explain why the build is failing", "why is my disk full",
               "can you summarize the changes in this repo", "please walk me through the failing test",
               # r18e4: "what is 5 > 3" is NOT prose here: it carries a `>` (shell syntax, #1560 / #1183: `what` is an installed head on macOS, so the line is
               # command evidence and runs in the shell). A prose twin without shell syntax stays in the table; the `>` line is judged as AMBIGUOUS below.
               "is five bigger than three", "can you show me how to use --verbose?", "explain $PATH to me"]
# installed head + English arguments: the documented rule (owner decision #964, #1183) decides by SURFACE, read from the routing log:
# plain terminal (no conversation open) = Shell, inside an agent conversation with no command evidence = Agent
AMBIGUOUS_LINES = ["find the bug in main.rs", "make a PR", "cat the error log and tell me what is wrong",
                   "what is 5 > 3"]
# (plain terminal, inside an agent conversation); None = not pinned for that surface (the logged rule below still has to agree). r18d2: the #1183
# evidence rule judges the LINE, not the conversation's history, and the table's own prose lines open a conversation in the terminal pane too, so the
# expectation is per line:
#   * "find the bug in main.rs": `main.rs` is a data-shaped word (interior dot, util.rs is_data_shaped_word) = command evidence, so it runs in the
#     shell on both surfaces (routing_corpus.rs: Case::cmd "#964 / #906"); the log says evidence-in-conversation (#1183) when it does
#   * "make a PR": `make` + words that are not common make targets = no evidence in a conversation (no-evidence-in-conversation) -> agent
#   * "cat the error log and tell me what is wrong": request shape (#834/#965: `tell me what` is a request addressed to someone) -> agent on
#     BOTH surfaces. r18u19: the plain-terminal half was unpinned, so the surface fallback below said shell; until then the line had only ever
#     been typed inside a conversation (r18u8/r18u13 logged AgentConversation). The routing corpus pins the plain terminal as Agent too
#     (routing_corpus.rs: `sudo rm the old logs for me` "#1291 (plain terminal unchanged)", `find the process that is eating my memory`
#     #834, and this exact line): the owner's installed-head rule (#964) covers English-looking arguments, not a sentence addressed to someone.
AMBIGUOUS_EXPECT = {
    "find the bug in main.rs": ("shell", "shell"),
    "make a PR": (None, "agent"),
    "cat the error log and tell me what is wrong": ("agent", "agent"),
}


def ambiguous_want(line, surface, rule):
    """Where an installed-head English line must go, from the line, the surface the app reports and the rule it logged. None = no verdict
    (no routing line was logged)."""
    pinned = AMBIGUOUS_EXPECT.get(line)
    if pinned and surface:
        want = pinned[1] if surface == "AgentConversation" else pinned[0]
        if want:
            return want
    if rule and rule.startswith(("evidence-in-conversation", "shell-structure-unknown-head")):
        return "shell"
    if rule and rule.startswith("no-evidence-in-conversation"):
        return "agent"
    return ("agent" if surface == "AgentConversation" else "shell") if surface else None


ENTER_BEGIN_RE = re.compile(r'hop=enter_begin\b.*?\binput_type=(\w+)')


def enter_begin_types(text):
    """input_type of every `hop=enter_begin` line in an app-log excerpt, in order ('Shell' | 'AI' | ...)"""
    return [m.group(1) for m in ENTER_BEGIN_RE.finditer(text)]


# #1593 (F5/A5): an Enter whose shell-or-chat verdict missed its budget is not submitted by the clock; the line stays in the editor
# with "Couldn't tell shell from chat in time; press Enter again" (`enter_verdict_give_up ... verdict_cap` /
# `young_verdict_cap`, or the Enter gate's `kh_enter_gate_give_up by=verdict_cap`). Correct per the owner's rule; counted so the
# agent-mode round can see whether the toast is frequent (then the budget, not the rule, is wrong).
VERDICT_GIVE_UP_RE = cases.VERDICT_GIVE_UP_RE      # one definition, in cases.py (ssh_nested_typeahead uses it too)


def verdict_give_ups(text):
    """How many verdict give-ups (#1593 A5 toast) an app-log excerpt shows. One Enter can log both the editor's
    `enter_verdict_give_up` and its gate's `kh_enter_gate_give_up`; each line counts, so use it as 'any' per typed line."""
    return len(VERDICT_GIVE_UP_RE.findall(text))


def classify_dest(reached_shell, route_rows, begin_types):
    """Where a typed line went, PROVEN from the app log (#1511). 'shell' = the shell's rec.log has the line; 'BOTH' = it did and the
    routing decision also says Agent; 'agent' = the shell received nothing AND the app logged the Enter as routed to the agent (the
    `enter: Agent` decision, or `enter_begin ... input_type=AI`); 'lost' = neither the shell nor an AI-routed Enter has it (a swallowed
    line); 'unproven' = the shell has nothing and the app log has no routing row at all (no log: nothing can be said)."""
    route = route_rows[-1]["route"] if route_rows else None
    if reached_shell:
        return "BOTH" if route == "Agent" else "shell"
    if route == "Agent" or (begin_types and begin_types[-1] == "AI"):
        return "agent"
    if route is None and not begin_types:
        return "unproven"
    return "lost"


def routing_table_rows(ctx, off):
    return [dict(route=m.group(1), rule=m.group(2), surface=m.group(3), head=m.group(4), n=int(m.group(5))) for m in ROUTE_RE.finditer(ctx.log_since(off))]


@case("routing_table", order=48, gui=True)
def c_routing_table(ctx, r):
    """Typed lines (10-25 ms/key, instant Enter) and where they went: shell (rec.log has the exact line) or agent (the enter-stage
    routing decision says Agent and the shell received nothing). Installed commands -> shell; clear natural language -> agent when
    autodetect applies (agent mode always; terminal mode per nld_in_terminal_enabled); installed-head English by the documented rule
    keyed on the surface the log reports. A shell command reaching the agent, or prose reaching the shell, fails."""
    open(os.path.join(ctx.W, "foo.txt"), "w").write("foo\nbar\n")
    if not ctx.log_path or not os.path.exists(ctx.log_path):
        r.note("no app log: destination inferred from the shell's rec.log alone (agent = the shell received nothing)")
    nld = setting_bool(ctx, "nld_in_terminal_enabled")
    auto = setting_bool(ctx, "ai_auto_detection_enabled")
    r.metric("settings_nld_in_terminal", nld)
    r.metric("settings_ai_auto_detection", auto)
    prose_to_agent = True if ctx.mode == "agent" else (nld is not False)       # absent key = the shipped default (on, manual 5.x)
    table = [(l, "shell", "shell") for l in SHELL_LINES] + [(l, "prose", "agent" if prose_to_agent else "shell") for l in PROSE_LINES] + [(l, "ambiguous", None) for l in AMBIGUOUS_LINES]
    results = []
    for line, kind, want in table:
        ctx.reset()
        off, mark = ctx.log_size(), ctx.cmd_mark()
        type_fast(ctx, line)
        got_cmd = ctx.until(lambda: [c for c in ctx.cmds_since(mark) if c == line], 3.0 if want != "agent" else 1.2)
        if not got_cmd:
            time.sleep(0.6)
        rows = routing_table_rows(ctx, off)
        enter = rows[-1] if rows else None
        reached_shell = any(c == line for c in ctx.cmds_since(mark))
        if enter and enter["route"] == "Shell" and not reached_shell:
            ctx.until(lambda: any(c == line for c in ctx.cmds_since(mark)), 3)
            reached_shell = any(c == line for c in ctx.cmds_since(mark))
        excerpt = ctx.log_since(off)
        dest = classify_dest(reached_shell, rows, enter_begin_types(excerpt))
        gave_up = verdict_give_ups(excerpt) > 0                 # #1593: the A5 toast fired for this line
        surface = enter["surface"] if enter else None
        if kind == "ambiguous":
            want = ambiguous_want(line, surface, enter["rule"] if enter else None)
        ok = want is None or dest == want
        results.append(dict(line=line, kind=kind, want=want, got=dest, surface=surface, rule=enter["rule"] if enter else None, verdict_give_up=gave_up))
        if gave_up and dest in ("lost", "unproven"):
            r.check(False, "%r (%s) was not sent: its shell-or-chat verdict missed the budget and the line stayed in the editor with the "
                    "'press Enter again' toast (#1593 A5; dest %s, surface %s)" % (line, kind, dest, surface))
        elif dest == "lost" or (dest == "unproven" and want != "shell"):
            r.check(False, "%r (%s) was LOST or unproven: the shell received nothing and the app log shows no agent-routed Enter (dest %s, surface %s)" % (line, kind, dest, surface))
        elif want is None:
            r.note("ambiguous %r: went to %s (no routing log line, so no rule to check)" % (line, dest))
        elif not ok:
            r.check(False, "%r (%s) expected %s, went to %s (surface %s, rule %s); shell received %r" % (line, kind, want, dest, surface, enter["rule"] if enter else "n/a", [c for c in ctx.cmds_since(mark)][:2]))
        if dest == "agent":
            ctx.be.key("ctrl+c")            # do not leave a model request running
            time.sleep(0.4)
    ctx.shot("routing_table_end")
    r.metric("routing_rows", results)
    r.metric("misroutes", len([x for x in results if x["want"] and x["want"] != x["got"]]))
    r.metric("verdict_give_ups", len([x for x in results if x["verdict_give_up"]]))     # #1593: lines kept by the A5 toast
    ctx.reset()


# ---------------------------------------------------------------- agent input exactness and mid-session mode switches (#1510)
# The app logs `hop=enter_begin deferred=.. buffer=<Debug string> input_type=Shell|AI` for every Enter (the text only because the harness
# sets WARP_INPUT_ORDER_LOG_TEXT=1; without it `buffer=<N chars>`), so what the agent input HELD at Enter is machine-readable from the log:
# byte-exact comparison with what was typed, no OCR. `hop=view_mode mode=agent|terminal` (Ctx.log_view_mode) names the view.
ENTER_BUFFER_RE = re.compile(r'hop=enter_begin\b[^\n]*?\bbuffer=("(?:[^"\\]|\\.)*"|<\d+ chars>) input_type=(\w+)')
_RUST_ESC = {"n": "\n", "r": "\r", "t": "\t", "0": "\0", "\\": "\\", '"': '"', "'": "'"}


def unescape_rust_debug(s):
    """Inverse of Rust's `{:?}` for a str: "..." with \\n \\r \\t \\0 \\\\ \\" \\' and \\u{hex}. Anything else is left as written."""
    if len(s) >= 2 and s[0] == '"' and s[-1] == '"':
        s = s[1:-1]
    out, i = [], 0
    while i < len(s):
        c = s[i]
        if c != "\\" or i + 1 >= len(s):
            out.append(c); i += 1; continue
        n = s[i + 1]
        if n == "u" and s[i + 2:i + 3] == "{" and "}" in s[i + 3:]:
            j = s.index("}", i + 3)
            try:
                out.append(chr(int(s[i + 3:j], 16))); i = j + 1; continue
            except ValueError:
                pass
        out.append(_RUST_ESC.get(n, "\\" + n)); i += 2
    return "".join(out)


def enter_buffers(text):
    """[(buffer, input_type)] for every `hop=enter_begin` line in an app-log excerpt. buffer is the exact text, or an int (its char count)
    when the app was not told to log text."""
    rows = []
    for m in ENTER_BUFFER_RE.finditer(text):
        b = m.group(1)
        rows.append((int(re.match(r"<(\d+) chars>", b).group(1)) if b.startswith("<") else unescape_rust_debug(b), m.group(2)))
    return rows


def judge_agent_input(sent, rows):
    """Failure text or None. The LAST Enter in `rows` must be the sent text, byte for byte, routed to the agent (input_type AI)."""
    if not rows:
        return "the app logged no hop=enter_begin line for the Enter (no log line to judge from)"
    buf, typ = rows[-1]
    if typ != "AI":
        return "the Enter was routed as %s, not AI (the agent view did not take the prose line)" % typ
    if isinstance(buf, int):
        return None if buf == len(sent) else "the agent input held %d chars at Enter, %d were typed" % (buf, len(sent))
    if buf != sent:
        k = next((i for i, (a, b) in enumerate(zip(buf, sent)) if a != b), min(len(buf), len(sent)))
        return "agent input differs from what was typed at char %d: typed %r, app held %r" % (k, sent, buf)
    return None


def wait_view_mode(ctx, want, timeout=6):
    ctx.until(lambda: ctx.log_view_mode() == want, timeout)
    return ctx.log_view_mode()


def submit_prose(ctx, r, tag, sent, why):
    """type `sent` (agent-bound prose) at the owner's rate, Enter in the same call; judge the Enter from the app log; stop the request."""
    off, mark = ctx.log_size(), ctx.cmd_mark()
    type_fast(ctx, sent)
    ctx.until(lambda: enter_buffers(ctx.log_since(off)), 6)
    rows = enter_buffers(ctx.log_since(off))
    err = judge_agent_input(sent, rows)
    r.check(err is None, "%s: %s" % (why, err))
    r.check(not [c for c in ctx.cmds_since(mark) if sent[:20] in c], "%s: the prose line reached the SHELL: %r" % (why, ctx.cmds_since(mark)[:2]))
    ctx.shot(tag)
    ctx.be.key("ctrl+c")                 # one Ctrl-C: stops a started request (two inside 1 s would leave the agent view)
    ctx.ctrlc_ts.append(time.time())
    time.sleep(0.6)
    return rows


@case("agent_input_exact", modes=("agent",), order=47, gui=True)
def c_agent_input_exact(ctx, r):
    """Prose typed into the AGENT view at 10-25 ms/key arrives in the agent input byte for byte (quotes, apostrophe, punctuation, a nonce),
    proven from the app's own `hop=enter_begin buffer=.. input_type=AI` line, not OCR; the shell must receive nothing. Needs no model."""
    if not ctx.log_path or not os.path.exists(ctx.log_path):
        r.skip("no app log at %s: the agent input at Enter cannot be read" % ctx.log_path)
    if ctx.log_view_mode() is None:
        r.skip("the app log has no hop=view_mode line (a build older than #1568): the view cannot be proven")
    r.check(ctx.log_view_mode() == "agent", "agent view expected but the log says hop=view_mode mode=%s" % ctx.log_view_mode())
    ctx.reset()
    for i, sent in enumerate(["please explain why \"build %s\" isn't passing, it's odd (really)?" % ctx.tok(6),
                              "what does error code %d mean when the linker says: cannot find -lfoo_%s" % (ctx.rng.randint(100, 999), ctx.tok(4))]):
        submit_prose(ctx, r, "agent_input_exact_%d" % i, sent, "prose #%d" % (i + 1))
    r.check(ctx.log_view_mode() == "agent", "the view left agent mode during the case (hop=view_mode says %s)" % ctx.log_view_mode())
    ctx.reset()


@case("agent_mode_switch", order=49, gui=True)
def c_agent_mode_switch(ctx, r):
    """Switch agent <-> terminal inside one session and check the ROUTING after each switch, from the app log: Escape leaves the agent view
    (hop=view_mode mode=terminal), the new-conversation chord (Ctrl+Shift+Enter, Cmd+Enter on macOS) enters it again. In the terminal view a
    shell line reaches the shell exactly; in the agent view a prose line reaches the agent input exactly and not the shell. The case ends in
    the mode it started in, and fails if it cannot get back (later cases would otherwise silently run in the wrong view)."""
    if not ctx.log_path or not os.path.exists(ctx.log_path):
        r.skip("no app log at %s: the view mode cannot be read" % ctx.log_path)
    start = ctx.log_view_mode()
    if start is None:
        r.skip("the app log has no hop=view_mode line (a build older than #1568): the view cannot be proven")
    chord = "cmd+Return" if ctx.be.name == "mac" else "ctrl+shift+Return"
    to_agent = lambda: ctx.be.key(chord)
    to_term = lambda: ctx.be.key("Escape")

    def in_terminal(step):
        ctx.reset()
        off, mark = ctx.log_size(), ctx.cmd_mark()
        line = "echo sw_%s_%s" % (step, ctx.tok(6))
        type_fast(ctx, line)
        got = ctx.until(lambda: [c for c in ctx.cmds_since(mark) if c == line], 5)
        rows = enter_buffers(ctx.log_since(off))
        r.check(bool(got), "%s: the shell never received %r (shell got %r)" % (step, line, ctx.cmds_since(mark)[:2]))
        r.check(not rows or rows[-1][1] == "Shell", "%s: in the terminal view the Enter was routed %s, not Shell" % (step, rows[-1][1] if rows else None))

    def in_agent(step):
        ctx.reset()
        submit_prose(ctx, r, "agent_mode_switch_" + step, "please explain why the %s test run is failing, it's odd" % ctx.tok(6), step)

    seq = [("terminal", to_term, in_terminal), ("agent", to_agent, in_agent)] if start == "agent" else [("agent", to_agent, in_agent), ("terminal", to_term, in_terminal)]
    seq = seq * 2          # there and back, then there and back again
    trail = [start]
    try:
        for i, (want, go, probe) in enumerate(seq):
            go()
            got = wait_view_mode(ctx, want)
            trail.append(got)
            if not r.check(got == want, "switch %d: expected hop=view_mode mode=%s after the key, the app log says %s" % (i + 1, want, got)):
                break
            ctx.shot("agent_mode_switch_%d_%s" % (i + 1, want))
            probe("switch%d_%s" % (i + 1, want))
    finally:
        # always try to leave the session as found
        for _ in range(2):
            if ctx.log_view_mode() == start:
                break
            (to_agent if start == "agent" else to_term)()
            wait_view_mode(ctx, start)
        ctx.reset()
    r.metric("view_mode_trail", trail)
    r.check(ctx.log_view_mode() == start, "could not return to the %s view the case started in (log says %s): later cases would run in the wrong mode" % (start, ctx.log_view_mode()))


# ---------------------------------------------------------------- #1267 recycled code-block editors: height / scroll stability, copy
RECYCLE_RE = re.compile(r"editor_memory agent_code_(recycled|rebuilt) block=(\S+) (?:dropped|rebuilt)=(\d+)")


def recycle_events(text):
    """[(kind 'recycled'|'rebuilt', block id string, count)] of the app's debug lines (app/src/ai/blocklist/block.rs recycle_code_editors /
    restore_code_editors, target editor_memory)"""
    return [(m.group(1), m.group(2), int(m.group(3))) for m in RECYCLE_RE.finditer(text)]


def snippet_lines(tok, n):
    return ["echo RCY%s_%02d" % (tok, i) for i in range(1, n + 1)]


def visible_snippets(ctx, tag, tok):
    """the snippet indices whose token OCR reads on screen (None without an OCR engine)"""
    t = ctx.ocr(tag)
    if t is None:
        return None
    sq = _squash(t)
    return sorted({int(m.group(1)) for m in re.finditer(r"rcy%s(\d\d)" % tok.lower(), sq)})


@case("agent_code_recycle", order=50, gui=True)
def c_agent_code_recycle(ctx, r):
    """#1267, both modes: one answer with APPCI_RECYCLE_SNIPPETS (12) fenced code blocks, each one line `echo RCY<tok>_NN`, then
    APPCI_RECYCLE_FILLERS (9) one-word answers push it more than 6 rich-content entries away, so its code-block editors are recycled (the app's
    'editor_memory agent_code_recycled' line). Block navigation (APPCI_KEY_BLOCK_UP: ctrl+up, mac cmd+up = SelectBlockAbove) goes back to it until
    it is rebuilt ('agent_code_rebuilt'). Checked: the recycle and the rebuild happen; the snippets visible on screen (OCR) and the screenshot do
    not change across the rebuild (height / scroll stable; the hash is a metric, the OCR set the verdict); copying the selected answer
    (APPCI_KEY_COPY: ctrl+shift+c, mac cmd+c) gives every snippet line exactly. Needs the CI model: the answer comes from it (there is no fake-model
    path in the app, as for md_viewer_ai_answer), so a model that does not produce the snippets is a SKIP. The per-snippet Run / Copy buttons need a
    mouse the harness does not drive: they are covered by the unit tests' plain-path reasoning only (block_tests.rs code_editor_recycling)."""
    need_provider(ctx, r)
    mac = ctx.be.name == "mac"
    up = os.environ.get("APPCI_KEY_BLOCK_UP", "cmd+up" if mac else "ctrl+up")
    copy = os.environ.get("APPCI_KEY_COPY", "cmd+c" if mac else "ctrl+shift+c")
    n = int(os.environ.get("APPCI_RECYCLE_SNIPPETS", "12"))
    fillers = int(os.environ.get("APPCI_RECYCLE_FILLERS", "9"))
    tok = ctx.tok(4).upper()
    want = snippet_lines(tok, n)
    mark = ctx.cmd_mark()
    off = ctx.log_size()
    try:
        prompt = ("Reply with exactly %d fenced bash code blocks and no other text. Code block number N contains only the single line "
                  "echo RCY%s_NN where NN is N written with two digits, from 01 to %02d." % (n, tok, n))
        type_fast(ctx, prompt)
        if not wait_marker(ctx, off, "[byop] opening stream", timeout=60):
            r.skip("the snippet prompt opened no model request in 60 s (routed elsewhere in %s mode, or no provider)" % ctx.mode)
        if not settled(ctx, "rcy_answer", timeout=180):
            r.skip("the snippet answer did not finish within 180 s")
        first = visible_snippets(ctx, "rcy_answer_ocr", tok)
        r.metric("snippets_visible_after_answer", first)
        if first is not None and not first:
            r.skip("the model's answer shows none of the RCY%s_NN snippets (model nondeterminism): nothing to recycle" % tok)
        for i in range(fillers):
            reqs = count_in_log(ctx, off, "[byop] opening stream")
            type_fast(ctx, "Reply with just the word FILL%s%d and nothing else." % (tok, i))
            if not wait_marker(ctx, off, "[byop] opening stream", n=reqs + 1, timeout=60):
                r.skip("filler %d opened no model request in 60 s" % i)
            settled(ctx, "rcy_fill", timeout=120)
        ev = recycle_events(ctx.log_since(off))
        recycled = [e for e in ev if e[0] == "recycled"]
        r.metric("recycle_events_after_fillers", ev)
        if not recycled:
            r.check(False, "no 'editor_memory agent_code_recycled' line after %d later answers: the snippet answer's editors were never recycled "
                           "(a build before #1267, PHOSPHOR_AGENT_CODE_EDITOR_RECYCLE=0, or the answer is not > 6 rich-content entries away)" % fillers)
            return
        block = recycled[0][1]
        r.metric("recycled_editors", recycled[0][2])
        # back to the snippet answer, one block at a time, until it is rebuilt
        off2 = ctx.log_size()
        rebuilt = False
        vis_first = None      # the snippets on the first frame the answer is back on screen: still the RECYCLED plain rendering (review)
        for _ in range(fillers + 6):
            ctx.be.key(up)
            time.sleep(0.6)
            if vis_first is None:
                v = visible_snippets(ctx, "rcy_nav_ocr", tok)
                if v:
                    vis_first = v
            if any(e[0] == "rebuilt" and e[1] == block for e in recycle_events(ctx.log_since(off2))):
                rebuilt = True
                break
        if not rebuilt:
            r.check(False, "block navigation (%s) back to the snippet answer never rebuilt its editors (no 'agent_code_rebuilt block=%s')" % (up, block))
            return
        # stability across the rebuild: what is on screen right after it and once everything settled must be the same
        time.sleep(0.4)
        h0 = shot_hash(ctx, "rcy_rebuilt_now")
        vis0 = visible_snippets(ctx, "rcy_rebuilt_now_ocr", tok)
        settled(ctx, "rcy_rebuilt_settle", timeout=30)
        h1 = shot_hash(ctx, "rcy_rebuilt_settled")
        vis1 = visible_snippets(ctx, "rcy_rebuilt_settled_ocr", tok)
        ctx.shot("rcy_rebuilt")
        r.metric("rebuild_shot_stable", h0 == h1)
        r.metric("snippets_visible_rebuilt", [vis0, vis1])
        r.metric("snippets_visible_first_on_screen", vis_first)
        if vis_first is not None and vis1 is not None:
            # the plain -> editor swap (the height change #1267 risks) happens before vis0 is taken; compare across it
            r.check(vis_first == vis1, "the visible snippets moved between the answer first coming on screen and after its rebuild settled "
                                       "(the recycled plain rendering and the rebuilt editor differ in height): %s -> %s" % (vis_first, vis1))
        if vis0 is None:
            r.note("no OCR engine: rebuild stability is the screenshot hash only (%s)" % ("stable" if h0 == h1 else "changed"))
            r.check(h0 == h1, "the screen changed after the rebuild of the recycled code blocks settled (height or scroll moved)")
        else:
            r.check(vis0 == vis1, "the visible snippets changed across the rebuild (height or scroll moved): %s -> %s" % (vis0, vis1))
            if h0 != h1:
                r.note("screenshot hash changed across the rebuild while the visible snippets did not (syntax colouring / cursor): %s" % vis1)
        # copy the selected answer: every snippet line exactly
        ctx.be.clip_set("")
        ctx.be.key(copy)
        time.sleep(0.8)
        clip = ctx.be.clip_get()
        clip = clip.decode(errors="replace") if isinstance(clip, bytes) else (clip or "")
        lines = [l.strip() for l in clip.replace("\r\n", "\n").split("\n")]
        missing = [w for w in want if w not in lines]
        r.metric("copy_missing", missing)
        if len(missing) == len(want):
            r.unverifiable("the copy key (%s) put none of the snippets on the clipboard: the snippet answer was not the selected block, or the model "
                           "wrote other snippets (clipboard %d bytes); copy across the rebuild is NOT verified" % (copy, len(clip)))
        else:
            r.check(not missing, "copying the rebuilt answer lost or changed snippet lines: missing %s" % missing)
        r.check(not [c for c in ctx.cmds_since(mark) if "RCY" + tok in c], "snippet text reached the shell")
    finally:
        try:
            ctx.be.key("escape")
        except Exception:
            pass
        # r18u8 (Mac + Windows left_agent_mode_after_agent_code_recycle): THIS case's own Escape (it clears the block selection) leaves an empty agent view, which the
        # runner then reported as an app finding. The case owns that exit, so it re-enters the agent view itself (`/agent`, proven by hop=view_mode) and says so.
        try:
            if ctx.mode == "agent":
                time.sleep(0.8)
                if ctx.log_view_mode() == "terminal":
                    ctx.be.type("/agent", 12, enter=True)
                    ok = ctx.until(lambda: ctx.log_view_mode() == "agent", 10)
                    r.note("the case's own Escape left the agent view; it re-entered with /agent (%s)" % ("agent view again" if ok else "NOT proven: the runner will relaunch"))
        except Exception:
            pass
        cleanup_tmp(ctx)
