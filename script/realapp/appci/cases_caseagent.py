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


def settled(ctx, tag, quiet=5, step=1.5, timeout=90):
    """True once the screen stops changing: at most 2 distinct frames (cursor blink) among the last `quiet` shots"""
    hs, t0 = [], time.time()
    while time.time() - t0 < timeout:
        h = shot_hash(ctx, tag)
        if h:
            hs.append(h)
        if len(hs) >= quiet and len(set(hs[-quiet:])) <= 2:
            return True
        time.sleep(step)
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
        # cancel. Two presses, by the app's design (terminal/view.rs handle_ctrl_c_input_event): with text in the input the first Ctrl-C only CLEARS the input
        # (cleared_buffer_len > 0 returns before any cancel); the second, on the empty input of an in-progress conversation, stops the stream. r18d2 Mac sent one
        # Ctrl-C and judged the stream still running because it had only cleared (or never cleared) the text.
        ctx.be.key("ctrl+c")
        time.sleep(1.2)
        ctx.shot("after_clear")
        still = ocr_has(ctx, "after_clear_ocr", typed)
        if still is True:
            r.check(False, "the first Ctrl-C did not clear the text typed while the reply streamed (%r still visible): the key did not reach the input" % typed)
        ctx.be.key("ctrl+c")
        time.sleep(2.5)
        ctx.shot("after_cancel")
        hs2 = []
        for _ in range(5):
            h = shot_hash(ctx, "post_cancel")
            if h:
                hs2.append(h)
            time.sleep(1.2)
        r.metric("distinct_frames_after_cancel", len(set(hs2)))
        r.check(len(set(hs2)) <= 2, "the screen kept changing %d s after the Ctrl-C that follows the input clear (%d distinct frames in 5): the stream did not stop" % (6, len(set(hs2))))
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
#   * "cat the error log and tell me what is wrong": request shape (#834/#965) -> agent in a conversation
AMBIGUOUS_EXPECT = {
    "find the bug in main.rs": ("shell", "shell"),
    "make a PR": (None, "agent"),
    "cat the error log and tell me what is wrong": (None, "agent"),
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
        dest = classify_dest(reached_shell, rows, enter_begin_types(ctx.log_since(off)))
        surface = enter["surface"] if enter else None
        if kind == "ambiguous":
            want = ambiguous_want(line, surface, enter["rule"] if enter else None)
        ok = want is None or dest == want
        results.append(dict(line=line, kind=kind, want=want, got=dest, surface=surface, rule=enter["rule"] if enter else None))
        if dest == "lost" or (dest == "unproven" and want != "shell"):
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
