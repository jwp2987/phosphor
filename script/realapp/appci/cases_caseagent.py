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
        if not shell_cmds and bash_rec_hook(ctx):
            # r18u23 (Windows Git Bash, the first run that got an approval card there): the app keeps the agent's command wrapper out of the shell's
            # history BY DESIGN (#904: HISTIGNORE on HISTORY_MARKER, agent_shell_wrap.rs), and the bash rc hook logs rec.log from `history 1`, so a bash
            # pane's rec.log can never show an approved command (the zsh hook can: it logs from preexec, r18u13). The same "reached the shell exactly
            # once" is read from the app's own pty writes of the wrapper instead; the effect file above still has to hold the token exactly once.
            ok_once, how, detail = approved_once_verdict(ctx.log_since(off), tok_a, got)
            r.note("approved command: a bash pane's rec.log cannot show an agent command (#904 keeps the wrapper out of history; the rc hook reads "
                   "`history 1`): counted by %s instead" % how)
            r.check(ok_once, "approved command did not reach the shell exactly once: %s (a bash pane's rec.log cannot show it, #904)" % detail)
        else:
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
        if bash_rec_hook(ctx) and pty_write_hop_logged(ctx.log_since(off2)):  # the hop is Windows-only; elsewhere the effect-file check above stands
            r.check(approved_command_writes(ctx.log_since(off2), tok_r) == 0, "the rejected command was written to the shell by the app (hop=pty_write_bytes "
                    "class=UserCommand carrying %s)" % tok_r)
    finally:
        cleanup_tmp(ctx)
        ctx.be.key("ctrl+c")


def bash_rec_hook(ctx):
    """True when the pane's rc hook is the bash one (profile.BASHRC, rec.log from `history 1`): the Windows Git Bash pane, and the Linux app (SHELL=/bin/bash).
    The macOS app runs zsh (profile.ZSHRC, logged from preexec)."""
    if getattr(ctx, "win", False):
        return getattr(ctx, "pane", None) == "gitbash"
    return getattr(getattr(ctx, "be", None), "name", "") != "mac"


USER_COMMAND_WRITE_RE = re.compile(r'hop=pty_write_bytes class=UserCommand\b[^\n]*?\bbytes="([^\n]*)"')


def approved_command_writes(log_text, tok):
    """how many times `tok` occurs in the app's pty writes of a command (`hop=pty_write_bytes class=UserCommand ... bytes="..."`, the agent's wrapper
    among them); needs WARP_INPUT_ORDER_LOG_TEXT=1 (the harness sets it on every backend).
    #2069: counted over the writes JOINED in log order, not per line. A ConPTY pipe may take part of a buffer (the app logs `pty_write_partial` and one
    pty_write_bytes line per accepted piece), so a wrapper of several hundred bytes can have the token split across two lines; per line that read as 0
    writes although the command ran once (r18u28 Git Bash: effect file exact, 0 counted). The bytes are escape_ascii text, byte by byte, so joining the
    pieces is the escaped text of the joined bytes. A command written twice still counts 2."""
    return "".join(m.group(1) for m in USER_COMMAND_WRITE_RE.finditer(log_text or "")).count(tok)


ANY_CLASS_WRITE_RE = re.compile(r'hop=pty_write_bytes class=(\w+)\b[^\n]*?\bbytes=("[^\n]*"|<[^>\n]*>)')


def token_write_classes(log_text, tok):
    """{class: lines} of every pty write line of ANY class whose shown bytes carry `tok`, plus '<not shown>' for writes logged by size only: what a
    failed count reports, so the next run says whether the command went out under another class, in pieces, or with its text not logged"""
    out = {}
    for m in ANY_CLASS_WRITE_RE.finditer(log_text or ""):
        if tok in m.group(2):
            out[m.group(1)] = out.get(m.group(1), 0) + 1
        elif m.group(2).startswith("<"):
            out["<not shown>"] = out.get("<not shown>", 0) + 1
    return out


def pty_write_hop_logged(log_text):
    """True when the app log carries any `hop=pty_write_bytes` line. The hop is Windows-only (event_loop.rs: `if cfg!(windows)`), so a Linux/Mac log never has it"""
    return "hop=pty_write_bytes" in (log_text or "")


def approved_once_verdict(log_text, tok, effect_lines):
    """(ok, how, detail): did the approved command reach the shell exactly once, for a pane whose rec.log cannot show it (#904). Windows logs the app's pty
    writes (hop=pty_write_bytes), so those are counted. Linux has no such hop (0 for the whole case, even though the command ran), so the side-effect file
    (`echo tok >> file`, one line per run) is the count: exactly [tok] = once; no line or two lines fail."""
    if pty_write_hop_logged(log_text):
        n = approved_command_writes(log_text, tok)
        return n == 1, "the app's pty writes (hop=pty_write_bytes class=UserCommand)", "%d pty writes carrying %s (writes of any class carrying it: %s)" % (
            n, tok, token_write_classes(log_text, tok) or "none")
    got = list(effect_lines or [])
    return got == [tok], "the command's effect file (this platform logs no pty_write_bytes hop)", "effect file lines %r, expected [%r]" % (got, tok)


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
#   * "make a PR": `make` + words that are not common make targets = no evidence in a conversation (no-evidence-in-conversation) -> agent. On the
#     plain terminal it runs in the shell when `make` is a program on this machine (#1981, owner 2026-11-10: an installed first word runs in the shell
#     in terminal mode; routing_corpus.rs Case::cmd "#831 / #964"). Until r14 this half was unpinned and either outcome passed; the committed
#     results show why nobody noticed: on macOS and Windows the terminal-mode pass typed the line with surface AgentConversation every time (the
#     table's own prose lines had opened a conversation), and on Wayland the row had no surface at all (see `seen` in the case)
#   * "cat the error log and tell me what is wrong": request shape (#834/#965: `tell me what` is a request addressed to someone) -> agent on
#     BOTH surfaces. r18u19: the plain-terminal half was unpinned, so the surface fallback below said shell; until then the line had only ever
#     been typed inside a conversation (r18u8/r18u13 logged AgentConversation). The routing corpus pins the plain terminal as Agent too
#     (routing_corpus.rs: `sudo rm the old logs for me` "#1291 (plain terminal unchanged)", `find the process that is eating my memory`
#     #834, and this exact line): the owner's installed-head rule (#964) covers English-looking arguments, not a sentence addressed to someone.
AMBIGUOUS_EXPECT = {
    "find the bug in main.rs": ("shell", "shell"),
    "make a PR": ("shell", "agent"),
    "cat the error log and tell me what is wrong": ("agent", "agent"),
}


def ambiguous_want(line, surface, rule, installed=True):
    """Where an installed-head English line must go, from the line, the surface the app reports and the rule it logged. None = no verdict
    (no routing line was logged, or the line is pinned to the shell on the plain terminal and its first word is NOT a program on this machine:
    `installed` False; the pin is the installed-head rule, which then does not apply)."""
    pinned = AMBIGUOUS_EXPECT.get(line)
    if pinned and surface:
        want = pinned[1] if surface == "AgentConversation" else pinned[0]
        if want == "shell" and surface != "AgentConversation" and not installed:
            return None
        if want:
            return want
    if rule and rule.startswith(("evidence-in-conversation", "shell-structure-unknown-head")):
        return "shell"
    if rule and rule.startswith("no-evidence-in-conversation"):
        return "agent"
    return ("agent" if surface == "AgentConversation" else "shell") if surface else None


def head_installed(line, which=shutil.which):
    """whether the line's first word is a program on this machine (the app under test runs here, with this PATH unless a case changed it)"""
    return bool(which(line.split()[0]))


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


def wrote_to_shell(excerpt, line):
    """#2205: True when the app's own pty writes (`hop=pty_write_bytes class=UserCommand`, Windows) carry `line`, the proof that the app put the
    line to the shell when the shell's rec.log is dark. The bytes are escape_ascii text (a quote is `\\"`), so both spellings are tried."""
    if not line:
        return False
    esc = line.replace("\\", "\\\\").replace('"', '\\"')
    return any(line in m.group(1) or esc in m.group(1) for m in USER_COMMAND_WRITE_RE.finditer(excerpt or ""))


def classify_dest(reached_shell, route_rows, begin_types, wrote_shell=False):
    """Where a typed line went, PROVEN from the app log (#1511). 'shell' = the shell's rec.log has the line; 'BOTH' = it did and the
    routing decision also says Agent; 'agent' = the shell received nothing AND the app logged the Enter as routed to the agent (the
    `enter: Agent` decision, or `enter_begin ... input_type=AI`); 'lost' = neither the shell nor an AI-routed Enter has it (a swallowed
    line); 'unproven' = the shell has nothing and the app log has no routing row at all (no log: nothing can be said).
    #2205: when rec.log is dark the app's decision line (`enter: Shell by ...`) is what says where the line was meant to go, and the app's
    UserCommand pty write of the line (`wrote_shell`) is the proof it ran in the shell. `enter_begin input_type` is the mode BEFORE the decision
    and is only the fallback when no decision line exists. A Shell decision with no write is 'lost'."""
    route = route_rows[-1]["route"] if route_rows else None
    if reached_shell:
        return "BOTH" if route == "Agent" else "shell"
    if route == "Agent":
        return "agent"
    if wrote_shell:
        return "shell"
    if route == "Shell":
        return "lost"
    if begin_types and begin_types[-1] == "AI":
        return "agent"
    if route is None and not begin_types:
        return "unproven"
    return "lost"


def recorder_prompts(ctx):
    """prompts the pane's recorder counted so far (None when the pane keeps none)"""
    try:
        return ctx.prompts() if ctx.prof.local_prompts else None
    except Exception:
        return None


def recorder_dark(cmds_in_case, prompts_in_case):
    """#2205: True when the shell recorder (rec.log) logged no command for a whole case although prompts occurred. The harness's recorder, not the
    app, is what failed then: a row with no app-side proof is reported UNVERIFIABLE ('HARNESS: recorder dark', status noref, never a pass), not LOST."""
    return not cmds_in_case and bool(prompts_in_case and prompts_in_case > 0)


def routing_table_rows(ctx, off):
    return [dict(route=m.group(1), rule=m.group(2), surface=m.group(3), head=m.group(4), n=int(m.group(5))) for m in ROUTE_RE.finditer(ctx.log_since(off))]


@case("routing_table", order=48, gui=True, enters_agent=True)
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
    case_mark, prompts0, unproven_msgs = ctx.cmd_mark(), recorder_prompts(ctx), []
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
        wrote = wrote_to_shell(excerpt, line)
        dest = classify_dest(reached_shell, rows, enter_begin_types(excerpt), wrote)
        gave_up = verdict_give_ups(excerpt) > 0                 # #1593: the A5 toast fired for this line
        # surface and rule: the enter-stage row, else the line's own keystroke-stage row. A line whose keystroke verdict had settled logs no `enter:`
        # row at all (every Wayland routing_table row up to r12a has surface None for that reason), and an ambiguous line then had no verdict
        any_rows = typo_rows(excerpt, line)
        seen = enter or (any_rows[-1] if any_rows else None)
        surface = seen["surface"] if seen else None
        rule = seen["rule"] if seen else None
        if kind == "ambiguous":
            want = ambiguous_want(line, surface, rule, installed=head_installed(line))
        ok = want is None or dest == want
        results.append(dict(line=line, kind=kind, want=want, got=dest, surface=surface, rule=rule, verdict_give_up=gave_up))
        if gave_up and dest in ("lost", "unproven"):
            r.check(False, "%r (%s) was not sent: its shell-or-chat verdict missed the budget and the line stayed in the editor with the "
                    "'press Enter again' toast (#1593 A5; dest %s, surface %s)" % (line, kind, dest, surface))
        elif dest == "lost" or (dest == "unproven" and want != "shell"):
            # #2205: whether this is the app's loss or the recorder's darkness is known only for the whole case (below)
            unproven_msgs.append("%r (%s) was LOST or unproven: the shell received nothing and the app log shows no agent-routed Enter and no pty write of the "
                                 "line (dest %s, surface %s)" % (line, kind, dest, surface))
        elif want is None:
            r.note("ambiguous %r: went to %s, no verdict (%s)" % (line, dest, "no routing log line, so no rule to check" if not surface else
                                                                 "surface %s, rule %s, first word a program here: %s" % (surface, rule, head_installed(line))))
        elif not ok:
            r.check(False, "%r (%s) expected %s, went to %s (surface %s, rule %s); shell received %r" % (line, kind, want, dest, surface, rule or "n/a", [c for c in ctx.cmds_since(mark)][:2]))
        if dest == "agent":
            ctx.be.key("ctrl+c")            # do not leave a model request running
            time.sleep(0.4)
    ctx.shot("routing_table_end")
    dark = recorder_dark(len(ctx.cmds_since(case_mark)), recorder_prompts(ctx) - prompts0 if prompts0 is not None else None)
    r.metric("recorder_dark", dark)
    for m in unproven_msgs:
        if dark:
            r.unverifiable("HARNESS: recorder dark: rec.log has no line for this whole case although prompts occurred, and the app log has no proof either: " + m)
        else:
            r.check(False, m)
    r.metric("routing_rows", results)
    r.metric("misroutes", len([x for x in results if x["want"] and x["want"] != x["got"]]))
    r.metric("verdict_give_ups", len([x for x in results if x["verdict_give_up"]]))     # #1593: lines kept by the A5 toast
    ctx.reset()


# ---------------------------------------------------------------- #1980: a mistyped command typed in an agent conversation
# In a conversation an unknown first word one edit from a command, on a line shaped like that command's, runs in the shell (which prints "command not
# found") instead of going to the model: rule `mistyped-command-in-conversation (#1980)` (routing.rs reads_as_mistyped_command). The first two are a
# subcommand CLI plus its own subcommand; the third is near a curated command word with no English argument. The prose twins keep going to the agent.
TYPO_LINES = ["gti status", "dokcer ps", "gerp -rn foo foo.txt"]
TYPO_PROSE_LINES = ["gti is broken again"]
TYPO_RULE = "mistyped-command-in-conversation"
# keystroke- or enter-stage routing rows (ROUTE_RE reads the enter stage only; a line whose keystroke verdict had already settled logs no enter row)
ANY_ROUTE_RE = re.compile(r'(keystroke|enter): (\w+) by (.*?) \((\w+), head "(.*?)", (\d+) bytes\)')


def typo_rows(text, line):
    """the routing rows (either stage) an app-log excerpt has for `line`: same first word and byte length, in order"""
    head = line.split()[0]
    return [dict(stage=m.group(1), route=m.group(2), rule=m.group(3), surface=m.group(4)) for m in ANY_ROUTE_RE.finditer(text or "")
            if m.group(5) == head and int(m.group(6)) == len(line.encode())]


def typo_verdict(kind, dest, surface, rule):
    """(ok, note) for one typed line, or None when it was not typed in a conversation (the rule is for that surface only; the plain terminal leaves
    an unknown head to the classifier). kind 'typo' must reach the shell and nothing else; 'prose' must go to the agent. A typo that reached the shell
    by another rule (its head is installed on this machine, e.g. the `gti` joke program) passes with a note: the case cannot prove #1980 with it."""
    if surface != "AgentConversation":
        return None
    if kind == "prose":
        return dest == "agent", None
    if dest != "shell":
        return False, None
    if rule and not rule.startswith(TYPO_RULE):
        return True, "reached the shell by %s, not by the #1980 rule (is the word a command on this machine?)" % rule
    return True, None


@case("routing_typo_head", modes=("agent",), order=49, gui=True)
def c_routing_typo_head(ctx, r):
    """#1980. Agent mode: mistyped commands typed in the conversation (10-25 ms/key, instant Enter) must reach the shell with the exact line in rec.log
    (the shell then says "command not found"), by the rule the app logs; a sentence with the same mistyped first word must go to the agent and never
    reach the shell. A line typed while no conversation is the surface has no verdict (noted); with none typed in a conversation the case skips."""
    if not ctx.log_path or not os.path.exists(ctx.log_path):
        r.skip("no app log at %s: the routing surface and rule cannot be read" % ctx.log_path)
    open(os.path.join(ctx.W, "foo.txt"), "w").write("foo\nbar\n")
    results, judged = [], 0
    for line, kind in [(l, "typo") for l in TYPO_LINES] + [(l, "prose") for l in TYPO_PROSE_LINES]:
        ctx.reset()
        off, mark = ctx.log_size(), ctx.cmd_mark()
        type_fast(ctx, line)
        if not ctx.until(lambda: [c for c in ctx.cmds_since(mark) if c == line], 3.0 if kind == "typo" else 1.2):
            time.sleep(0.6)
        excerpt = ctx.log_since(off)
        rows = typo_rows(excerpt, line)
        last = rows[-1] if rows else None
        reached_shell = any(c == line for c in ctx.cmds_since(mark))
        dest = classify_dest(reached_shell, [x for x in rows if x["stage"] == "enter"] or rows, enter_begin_types(excerpt), wrote_to_shell(excerpt, line))
        surface = last["surface"] if last else None
        rule = last["rule"] if last else None
        verdict = typo_verdict(kind, dest, surface, rule)
        results.append(dict(line=line, kind=kind, got=dest, surface=surface, rule=rule, stage=last["stage"] if last else None))
        if verdict is None:
            r.note("%r: no verdict, surface %s (went to %s by %s)" % (line, surface, dest, rule))
        else:
            judged += 1
            ok, why = verdict
            r.check(ok, "%r (%s) went to %s by %s (surface %s); the shell received %r" % (line, kind, dest, rule, surface, [c for c in ctx.cmds_since(mark)][:2]))
            if why:
                r.note("%r %s" % (line, why))
        if dest == "agent":
            ctx.be.key("ctrl+c")            # do not leave a model request running
            time.sleep(0.4)
    ctx.shot("routing_typo_head_end")
    r.metric("typo_rows", results)
    ctx.reset()
    if not judged:
        r.skip("no line was typed with an agent conversation as the routing surface (rows: %s)" % [(x["line"], x["surface"]) for x in results])


# ---------------------------------------------------------------- #1981: an installed first word runs in the shell on the PLAIN terminal
# Owner decision 2026-11-10 (confirming #964): in terminal mode a line whose first word is a command the session knows runs in the shell, however
# English its arguments read (`make a PR`); rule `installed-head (#906/#964)` (routing.rs). routing_table could not prove it: its terminal-mode pass
# types these lines after its own prose lines, which open a conversation in the pane, so the surface was AgentConversation and the #1183 evidence rule
# (correctly) sent `make a PR` to the agent. This case leaves the agent view first and judges only lines the app itself logged as PlainTerminal.
PLAIN_INSTALLED_LINES = ["make a PR", "make a README", "find the bug in main.rs", "find large files"]
PLAIN_INSTALLED_RULE = "installed-head"


def plain_installed_verdict(dest, surface, rule, installed):
    """(ok, note) for one typed line, or None = no verdict: the line was not typed on the plain terminal (the app's own routing row says another surface,
    or there is no row), or its first word is no program on this machine (the installed-head rule does not apply). With a verdict the line must have
    reached the shell and nothing else; reaching it by another rule passes with a note naming the rule."""
    if surface != "PlainTerminal" or not installed:
        return None
    if dest != "shell":
        return False, None
    if rule and not rule.startswith(PLAIN_INSTALLED_RULE):
        return True, "reached the shell by %s, not by the installed-head rule" % rule
    return True, None


@case("routing_plain_installed_head", order=49, gui=True)
def c_routing_plain_installed_head(ctx, r):
    """#1981. Both modes: with the pane in the TERMINAL view (Escape leaves the agent view; proven by hop=view_mode mode=terminal), lines whose first
    word is an installed command but whose arguments read as English (`make a PR`) are typed at 10-25 ms/key with an instant Enter and must reach the
    shell with the exact line in rec.log, never the agent. Judged only when the app's routing row for the line says PlainTerminal and the first word is
    a program on this machine; with no line judged the case is UNVERIFIABLE. Ends in the view it started in."""
    if not ctx.log_path or not os.path.exists(ctx.log_path):
        r.skip("no app log at %s: the routing surface and rule cannot be read" % ctx.log_path)
    start = ctx.log_view_mode()
    if start is None:
        r.skip("the app log has no hop=view_mode line (a build older than #1568): the view cannot be proven")
    chord = "cmd+Return" if ctx.be.name == "mac" else "ctrl+shift+Return"

    def to_terminal():
        """True once the log says the pane shows the terminal view"""
        if ctx.log_view_mode() == "terminal":
            return True
        ctx.reset()
        ctx.be.key("Escape")
        return wait_view_mode(ctx, "terminal") == "terminal"

    results, judged = [], 0
    try:
        for line in PLAIN_INSTALLED_LINES:
            if not to_terminal():
                r.note("%r: not typed, Escape did not leave the agent view (hop=view_mode says %s)" % (line, ctx.log_view_mode()))
                results.append(dict(line=line, got=None, surface=None, rule=None, stage=None, installed=head_installed(line)))
                continue
            ctx.reset()
            off, mark = ctx.log_size(), ctx.cmd_mark()
            type_fast(ctx, line)
            if not ctx.until(lambda: [c for c in ctx.cmds_since(mark) if c == line], 3.0):
                time.sleep(0.6)
            excerpt = ctx.log_since(off)
            rows = typo_rows(excerpt, line)
            last = rows[-1] if rows else None
            if last and last["route"] == "Shell" and not any(c == line for c in ctx.cmds_since(mark)):
                ctx.until(lambda: any(c == line for c in ctx.cmds_since(mark)), 3)          # the shell logs a command with its next prompt
            reached_shell = any(c == line for c in ctx.cmds_since(mark))
            excerpt = ctx.log_since(off)
            dest = classify_dest(reached_shell, [x for x in rows if x["stage"] == "enter"] or rows, enter_begin_types(excerpt), wrote_to_shell(excerpt, line))
            surface, rule, installed = (last["surface"] if last else None), (last["rule"] if last else None), head_installed(line)
            verdict = plain_installed_verdict(dest, surface, rule, installed)
            results.append(dict(line=line, got=dest, surface=surface, rule=rule, stage=last["stage"] if last else None, installed=installed))
            if verdict is None:
                r.note("%r: no verdict (surface %s, first word a program here: %s; went to %s by %s)" % (line, surface, installed, dest, rule))
            else:
                judged += 1
                ok, why = verdict
                r.check(ok, "%r typed on the plain terminal went to %s by %s; an installed first word must run in the shell (#1981). The shell received %r" % (
                    line, dest, rule, [c for c in ctx.cmds_since(mark)][:2]))
                if why:
                    r.note("%r %s" % (line, why))
            if dest == "agent":
                ctx.be.key("ctrl+c")            # do not leave a model request running
                time.sleep(0.4)
        ctx.shot("routing_plain_installed_head_end")
    finally:
        # always try to leave the session as found (agent_mode_switch does the same)
        for _ in range(2):
            if ctx.log_view_mode() == start:
                break
            ctx.reset()
            ctx.be.key(chord if start == "agent" else "Escape")
            wait_view_mode(ctx, start)
        ctx.reset()
    r.metric("plain_installed_rows", results)
    r.check(ctx.log_view_mode() == start, "could not return to the %s view the case started in (log says %s): later cases would run in the wrong mode" % (start, ctx.log_view_mode()))
    if not judged:
        r.unverifiable("no line was typed with the plain terminal as the routing surface and an installed first word, so the installed-head rule was not "
                       "judged (rows: %s)" % [(x["line"], x["surface"], x["installed"]) for x in results])


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


PLAN_RE = re.compile(r"editor_memory agent_code_plan (rich=\d+ visible=\[[^\]]*\] plan=\[[^\]]*\](?: \(\+\d+ more\))?)")


def recycle_plans(text):
    """#1787: the app's `agent_code_plan` debug lines (app/src/terminal/view.rs ai_code_recycle_plan_summary): one per CHANGED visible set, saying how
    many rich-content entries there were, which positions were visible and each AI block's action (R recycle, B rebuild, K keep). No line at all
    means the visible set never changed; a line with no R means the snippet answer stayed within 6 entries of a visible one."""
    return PLAN_RE.findall(text)


def recycle_events(text):
    """[(kind 'recycled'|'rebuilt', block id string, count)] of the app's debug lines (app/src/ai/blocklist/block.rs recycle_code_editors /
    restore_code_editors, target editor_memory)"""
    return [(m.group(1), m.group(2), int(m.group(3))) for m in RECYCLE_RE.finditer(text)]


def snippet_lines(tok, n):
    return ["echo RCY%s_%02d" % (tok, i) for i in range(1, n + 1)]


RECYCLE_NEEDS_FAKE = ("NEEDS: the scripted fake model as this pass's provider (the fix tier's agent@fake pass, or a run started with APPCI_FAKE_MODEL=1). "
                      "agent_code_recycle is never judged on a real model's wording (#2071): its answer is fake_model.snippet_reply, the same text in every run")


def recycle_fake(environ=None, active=None):
    """#2071: the running scripted model when this pass's provider is the fake (APPCI_FAKE_MODEL_ACTIVE=1 and a FakeModel is up), else None.
    `environ` / `active` are for the self-test; a case passes neither."""
    import fake_model
    environ = os.environ if environ is None else environ
    active = fake_model.FakeModel.ACTIVE if active is None else active
    return active if environ.get("APPCI_FAKE_MODEL_ACTIVE") == "1" and active else None


RECYCLE_TOKEN_LETTERS = "ABCDEFHKMNPRTUVWXY"      # no I / L / O / Q / S / Z / G / J: letters OCR confuses with digits or each other


def recycle_token(rng, n=4):
    """pure given the rng: the token of one run's snippets, LETTERS only. Until round 15 it was ctx.tok(4).upper() (`T12_ABCD`): visible_snippets() looks
    for `rcy<tok><nn>` in the screen text with everything but letters and digits removed, where the token's own underscore can never be, so with an OCR
    engine the case read "none of the snippets is on screen" on every run and SKIPPED as "model nondeterminism" whatever the model had written (#2071)."""
    return "".join(rng.choice(RECYCLE_TOKEN_LETTERS) for _ in range(n))


def recycle_filler_prompt(tok, i):
    """pure: one filler turn. Plain prose that ends in its own tag: the fake model answers `OK FILL<tok><i>` (its fallback: the last word), one short line"""
    return "Please answer this filler turn FILL%s%d" % (tok, i)


def visible_snippets(ctx, tag, tok):
    """the snippet indices whose token OCR reads on screen (None without an OCR engine)"""
    t = ctx.ocr(tag)
    if t is None:
        return None
    sq = _squash(t)
    return sorted({int(m.group(1)) for m in re.finditer(r"rcy%s(\d\d)" % tok.lower(), sq)})


@case("agent_code_recycle", modes=("agent",), order=50, gui=True)
def c_agent_code_recycle(ctx, r):
    """#1267, driven by the scripted fake model (#2071): one answer with APPCI_RECYCLE_SNIPPETS (12) fenced code blocks, each one line
    `echo RCY<tok>_NN` (fake_model.snippet_reply: the same text in every run), then APPCI_RECYCLE_FILLERS (9) one-line answers push it more than 6
    rich-content entries away, so its code-block editors are recycled (the app's 'editor_memory agent_code_recycled' line). Block navigation
    (APPCI_KEY_BLOCK_UP: ctrl+up, mac cmd+up = SelectBlockAbove) goes back to it until it is rebuilt ('agent_code_rebuilt'). Checked: the model was
    sent the prompt (fake.fresh) and the answer's snippets are on screen (OCR) before anything else is judged; the recycle and the rebuild happen;
    the snippets visible on screen and the screenshot do not change across the rebuild (height / scroll stable; the hash is a metric, the OCR set the
    verdict); copying the selected answer (APPCI_KEY_COPY: ctrl+shift+c, mac cmd+c) gives every snippet line exactly.
    Until round 15 the answer came from the CI model and the case SKIPPED, was NOREF or failed on every platform on that model's wording (F10 of
    docs/design/r14-test-gaps.md); it now SKIPS with NEEDS: when the pass's provider is not the fake, and nothing in it depends on wording.
    Agent mode only: the scripted model has an agent pass only (run.py's agent@fake), and every PROVING_FAKE case is agent-only (selftest); the
    recycling itself is the block list's and has no mode. The per-snippet Run / Copy buttons need a mouse the harness does not drive here: they are
    covered by the unit tests' plain-path reasoning only (block_tests.rs code_editor_recycling)."""
    import fake_model
    fake = recycle_fake()
    if fake is None:
        r.skip(RECYCLE_NEEDS_FAKE)
    need_provider(ctx, r)
    mac = ctx.be.name == "mac"
    up = os.environ.get("APPCI_KEY_BLOCK_UP", "cmd+up" if mac else "ctrl+up")
    copy = os.environ.get("APPCI_KEY_COPY", "cmd+c" if mac else "ctrl+shift+c")
    n = int(os.environ.get("APPCI_RECYCLE_SNIPPETS", "12"))
    fillers = int(os.environ.get("APPCI_RECYCLE_FILLERS", "9"))
    n = min(n, fake_model.SNIP_MAX)
    tok = recycle_token(ctx.rng)
    want = snippet_lines(tok, n)
    mark = ctx.cmd_mark()
    off = ctx.log_size()
    try:
        prompt = fake_model.snippet_prompt(tok, n)
        sent0 = fake.fresh_with(prompt)
        type_fast(ctx, prompt)
        # The answer is scripted, so nothing below is the model's to get wrong: a prompt that never reached the model, or an answer that is not on
        # screen, is a finding about the app or the harness, never a SKIP.
        asked = ctx.until(lambda: fake.fresh_with(prompt) > sent0, 60, step=0.25)
        r.metric("snippet_prompt_requests", fake.fresh_with(prompt) - sent0)
        if not r.check(bool(asked), "the snippet prompt %r reached the scripted model 0 times in 60 s (the agent input did not send it: routed to the shell, "
                                    "or the view is not the agent view; app log has %d '[byop] opening stream' line(s) since the prompt)" % (
                                        prompt, count_in_log(ctx, off, "[byop] opening stream"))):
            return
        if not r.check(settled(ctx, "rcy_answer", timeout=180), "the scripted snippet answer (%d short code blocks, sent in one reply) was still changing the screen after 180 s" % n):
            return
        first = visible_snippets(ctx, "rcy_answer_ocr", tok)
        r.metric("snippets_visible_after_answer", first)
        if first is not None and not r.check(bool(first), "the scripted answer was sent (%d code blocks `echo RCY%s_NN`) and none of them is on screen (OCR read no "
                                                         "rcy%s<nn>): the answer was not rendered, or the view is not showing its end" % (n, tok, tok.lower())):
            return
        # r18u22 (Windows agent pass, PowerShell pane): 9 one-word answers were not enough. The app recycles a block more than 6 rich-content entries from the
        # NEAREST VISIBLE one (view.rs AI_CODE_RECYCLE_DISTANCE), and in that agent view several short filler answers were on screen at once, so the snippet
        # answer stayed within 6 of a visible one: correctly kept. The very next layout that changed the visible set (the case's own Escape, 11 s later) logged
        # `agent_code_recycled ... dropped=14`. How many fillers it takes depends on how many answers fit on the screen, so after the first `fillers` the case
        # adds more, one at a time, until the recycle line comes (cap APPCI_RECYCLE_MAX_FILLERS, default 24); the count it needed is a metric.
        max_fillers = max(fillers, int(os.environ.get("APPCI_RECYCLE_MAX_FILLERS", "24")))
        used = 0
        while used < max_fillers:
            if used >= fillers and any(e[0] == "recycled" for e in recycle_events(ctx.log_since(off))):
                break
            filler = recycle_filler_prompt(tok, used)
            type_fast(ctx, filler)
            if not r.check(bool(ctx.until(lambda: fake.fresh_with(filler) >= 1, 60, step=0.25)),
                           "filler %d (%r) reached the scripted model 0 times in 60 s: the conversation stopped taking prompts after the snippet answer" % (used, filler)):
                return
            settled(ctx, "rcy_fill", timeout=120)
            used += 1
        r.metric("fillers_used", used)
        ev = recycle_events(ctx.log_since(off))
        recycled = [e for e in ev if e[0] == "recycled"]
        r.metric("recycle_events_after_fillers", ev)
        if not recycled:
            plans = recycle_plans(ctx.log_since(off))
            r.metric("recycle_plan_lines", len(plans))
            still = visible_snippets(ctx, "rcy_norecycle_ocr", tok)
            r.metric("snippets_visible_without_recycle", still)
            r.check(False, "no 'editor_memory agent_code_recycled' line after %d later answers: the snippet answer's editors were never recycled "
                           "(a build before #1267, PHOSPHOR_AGENT_CODE_EDITOR_RECYCLE=0, or the answer is never > 6 rich-content entries from a visible one). "
                           "#1787 evidence: %s; snippets still on screen (OCR): %s" % (
                               used,
                               ("%d agent_code_plan line(s), last %r (no R = every AI block within 6 entries of a visible one; a snippet answer still on screen is "
                                "visible, so correctly kept)" % (len(plans), plans[-1])) if plans
                               else "NO agent_code_plan line: the visible rich-content set never changed after the answer (nothing triggers a recycle) or the build lacks the line",
                               still))
            return
        block = recycled[0][1]
        r.metric("recycled_editors", recycled[0][2])
        # back to the snippet answer, one block at a time, until it is rebuilt
        off2 = ctx.log_size()
        rebuilt = False
        vis_first = None      # the snippets on the first frame the answer is back on screen: still the RECYCLED plain rendering (review)
        for _ in range(used + 6):
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
            r.unverifiable("the copy key (%s) put none of the snippets on the clipboard: the snippet answer was not the selected block when the key was sent "
                           "(clipboard %d bytes; the answer's text is scripted, so this is the selection, not the wording); copy across the rebuild is NOT "
                           "verified" % (copy, len(clip)))
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


# ---------------------------------------------------------------- #1796: a slow, silent agent command is not "needs your input"
LRC_HANDED_TO_USER = "handing the command to the user"          # interaction_mode.rs PromptDetector: an interactive prompt was detected
LRC_KEPT_WITH_AGENT = "leaving the command with the agent"     # #1796: a trailing `?`/`:` sat quiet, the OS says nothing reads the tty (or cannot tell)
LRC_NOTICE = "showing a take-over notice"                       # #1811: silence with an unknown probe shows the notice, never a hand-over
LRC_HANDOFF_ERROR = "Failed to enter inline agent view for LRC handoff"
LRC_AWAITED_RE = re.compile(r"agent_command_wait awaited=(true|false)\b")   # shell_command.rs: is this tool call one the hand-over watches


def lrc_awaited(log_text):
    """True / False from the LAST `agent_command_wait awaited=` line in `log_text` (the command just run), None when there is none (an older build)"""
    found = LRC_AWAITED_RE.findall(log_text or "")
    return (found[-1] == "true") if found else None


def lrc_takeover_verdict(log_text):
    """(handed_to_user, kept_with_agent, handoff_errors) from the app log of one #1796 sub-case"""
    t = log_text or ""
    return LRC_HANDED_TO_USER in t, LRC_KEPT_WITH_AGENT in t, t.count(LRC_HANDOFF_ERROR)


def lrc_run_command(ctx, r, off, command, label):
    """ask the agent to run `command` and WAIT for it (the take-over detector only watches a tool call blocked until the command completes);
    approve the card when one comes (Enter focuses it, a second Enter approves, as in agent_approval). False when no card and nothing ran."""
    type_fast(ctx, "Please run exactly this shell command, nothing else, and wait for it to finish before you reply: %s" % command)
    if wait_marker(ctx, off, "queue_actions", timeout=120):
        time.sleep(1.5)
        ctx.shot("%s_card" % label)
        ctx.be.key("Return")
        time.sleep(1.0)
        ctx.be.key("Return")
        return True
    return False


def lrc_cmds(shell, tok):
    """pure: (part 1, part 2) shell lines of agent_lrc_no_false_takeover for the pane's shell ('pwsh' = Windows PowerShell pane, else POSIX). Part 1 prints a bare
    `Waiting:` and sits silent 20 s; part 2 prints `Continue? [y/N] ` and reads a line. Neither announces itself to the app, which is the point."""
    if shell == "pwsh":
        return ("Write-Host -NoNewline 'Waiting:'; Start-Sleep 20; Add-Content lrc_wait.txt %s" % tok,
                "Write-Host -NoNewline 'Continue? [y/N] '; $ans = [Console]::ReadLine(); Add-Content lrc_yn.txt %s" % tok)
    return ("printf 'Waiting:'; sleep 20; echo %s >> lrc_wait.txt" % tok,
            "printf 'Continue? [y/N] '; read ans; echo %s >> lrc_yn.txt" % tok)


@case("agent_lrc_no_false_takeover", modes=("agent",), order=51, gui=True)
def c_agent_lrc_no_false_takeover(ctx, r):
    """#1796 (owner report 2026-11-06): an agent command that is merely slow and silent after printing a line ending in `:` was handed to the user
    as "Command needs your input", and the hand-back logged 'Failed to enter inline agent view for LRC handoff: Already in agent mode' as an ERROR.
    Part 1: `printf 'Waiting:'; sleep 20; echo <tok> >> lrc_wait.txt` must run to its end with the agent: no 'handing the command to the user' line
    in the app log; the effect file gets the token. On Linux the app logs that it saw the quiet `Waiting:` line and kept the command because the
    foreground was not reading the tty (a note: it only logs when the model's tool call waited for completion).
    Part 2: `printf 'Continue? [y/N] '; read ans; echo <tok> >> lrc_yn.txt` is handed over when the OS proves the command reads the tty (Linux; #1811, no
    quiet floor); where it cannot tell, only the take-over notice is logged. The 'handing the command to the user' line comes; the case answers `y` + Enter and the effect file gets the token. A model that does not wait for the
    command (its tool call returns a snapshot) never arms the detector: the app logs `agent_command_wait awaited=false` for it, and then a missing
    hand-over is a NOTE even when the command is still at its prompt after 60 s. A missing hand-over with `awaited=true` (or no such line) and the command
    still stuck after 60 s FAILS (the 30-minute hang). After a confirmed hand-over the typed `y` + Enter must reach the command (its effect file).
    Across the whole case: no 'Failed to enter inline agent view for LRC handoff' line (the agent view is full-screen in this pass). Needs the CI model.
    Windows (#1882): runs in the PowerShell and Git Bash panes with the pane's own commands (lrc_cmds). ConPTY has no tty-read probe, so part 2 gets the take-over
    NOTICE (the shared detector's `showing a take-over notice` line) and stays with the agent; the hand-over on Windows is the Read-Host marker (agent_interactive_password)."""
    need_provider(ctx, r)
    fresh_conversation(ctx, r)
    tok_y = None
    try:
        # ---- part 1: a slow, silent command after a trailing colon stays with the agent
        tok_w = "LRCWAIT" + ctx.tok(6).upper()
        ctx.rm("lrc_wait.txt")
        off = ctx.log_size()
        t0 = time.time()
        if not lrc_run_command(ctx, r, off, lrc_cmds(ctx.pane_shell(), tok_w)[0], "lrc_wait"):
            if not ctx.until(lambda: ctx.lines("lrc_wait.txt"), 5):
                ctx.shot("lrc_wait_no_card")
                r.skip("no approval card within 120 s and the command did not run (the model did not propose it)")
        time.sleep(12)
        ctx.shot("lrc_wait_running")
        done = ctx.until(lambda: ctx.lines("lrc_wait.txt"), 60, step=0.5)
        r.metric("lrc_wait_s", round(time.time() - t0, 1))
        ctx.shot("lrc_wait_done")
        handed, kept, errs = lrc_takeover_verdict(ctx.log_since(off))
        r.check(bool(done) and ctx.lines("lrc_wait.txt") == [tok_w], "the silent command did not finish: lrc_wait.txt is %r" % ctx.lines("lrc_wait.txt"))
        r.check(not handed, "the silent `printf 'Waiting:'; sleep 20` was handed to the user as needing input (#1796): the log says '%s'" % LRC_HANDED_TO_USER)
        r.check(errs == 0, "the LRC hand-back logged '%s' %d time(s)" % (LRC_HANDOFF_ERROR, errs))
        r.note("detector saw the quiet `Waiting:` line and kept the command with the agent: %s (agent_command_wait awaited=%s)" % (
            "yes" if kept else "not logged (the model's tool call may not have waited for completion)", lrc_awaited(ctx.log_since(off))))
        wait_marker(ctx, off, "[byop] opening stream", n=2, timeout=60)
        settled(ctx, "lrc_idle1", timeout=120)

        # ---- part 2: an explicit [y/N] prompt is still handed over
        tok_y = "LRCYN" + ctx.tok(6).upper()
        ctx.rm("lrc_yn.txt")
        off2 = ctx.log_size()
        if not lrc_run_command(ctx, r, off2, lrc_cmds(ctx.pane_shell(), tok_y)[1], "lrc_yn"):
            r.note("part 2: no approval card within 120 s (the model did not propose the [y/N] command); the explicit hand-over is NOT verified")
            return
        handed = ctx.until(lambda: lrc_takeover_verdict(ctx.log_since(off2))[0], 60, step=0.5)
        ctx.shot("lrc_yn_prompt")
        if handed:
            time.sleep(1.0)
            ctx.be.type("y", ctx.rng.randint(10, 25), enter=True)
            answered = ctx.until(lambda: ctx.lines("lrc_yn.txt"), 20, step=0.5)
            ctx.shot("lrc_yn_answered")
            r.check(ctx.lines("lrc_yn.txt") == [tok_y], "part 2: the [y/N] prompt was handed to the user, but the typed `y` + Enter did not reach the command "
                    "(lrc_yn.txt is %r after 20 s)" % (ctx.lines("lrc_yn.txt") if answered else []))
        elif LRC_NOTICE in ctx.log_since(off2):
            # #1811 (owner decision 2026-11-09): where the OS cannot say the command reads the tty (no probe, e.g. macOS) an explicit
            # marker only gets the take-over notice after 3 s; it is never handed over on silence.
            r.note("part 2: the OS cannot tell whether the command reads the tty; the take-over notice was logged and the command stayed with the agent (no hand-over on a timer)")
        elif ctx.lines("lrc_yn.txt"):
            r.note("part 2: no hand-over, but the command finished: the model answered the prompt itself (its tool call did not wait), so the hand-over was not exercised")
        elif lrc_awaited(ctx.log_since(off2)) is False:
            r.note("part 2: no hand-over and the command is still at its prompt after 60 s, but the app logged `agent_command_wait awaited=false`: the model's "
                   "tool call did not wait for completion, so the hand-over detector was never armed; the explicit hand-over is NOT verified")
        else:
            r.check(False, "part 2: `printf 'Continue? [y/N] '; read` sat at its prompt for 60 s and was neither handed to the user (a proven tty read must be, #1811) nor given the take-over notice ("
                           "agent_command_wait awaited=%s)" % lrc_awaited(ctx.log_since(off2)))
        _, _, errs2 = lrc_takeover_verdict(ctx.log_since(off2))
        r.check(errs2 == 0, "part 2: the LRC hand-back logged '%s' %d time(s)" % (LRC_HANDOFF_ERROR, errs2))
    finally:
        cleanup_tmp(ctx)
        ctx.be.key("ctrl+c")
        lrc_wait_logged(ctx, tok_y)


def lrc_command_pending(commands, tok, effect_lines):
    """pure: is the [y/N] command of this case (it carries `tok`) still to be logged by the shell? True while it was started (tok given), did not
    finish on its own (its effect file is empty) and no rec.log line carries the token yet. A pane without rec.log (commands == []) has nothing to wait for."""
    return bool(tok) and bool(commands) and not effect_lines and not any(tok in c for c in commands)


def lrc_wait_logged(ctx, tok, timeout=10.0):
    """#2114 (round 14, Windows agent@gitbash): part 2 can end with `printf 'Continue? [y/N] '; read ans; ...` still at its prompt (the model's tool call
    did not wait, so nothing handed it over). The Ctrl-C above ends it, and bash logs an interrupted command at its NEXT prompt, which came 0.7 s later,
    after the next case (cb_agent_pickers) had taken its rec.log mark: "a picker key reached the shell: [the [y/N] line]". A case leaves the pane as it
    found it: wait (bounded; the give-up net is the old behaviour) until the shell has logged the line. Returns True when it waited for one."""
    try:
        pending = lambda: lrc_command_pending([c for _t, c in ctx.prof.commands()], tok, ctx.lines("lrc_yn.txt"))       # noqa: E731
        if not pending():
            return False
        ctx.until(lambda: not pending(), timeout, step=0.2)
        return True
    except Exception:          # cleanup never masks the case's verdict
        return False
