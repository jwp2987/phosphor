# appci/cases_agent_w2.py (round 14, docs/design/r14-test-gaps.md 7.1 wave 2, T11-T15; design note docs/design/r10-r14-w2a.md): five agent cases on the
# scripted fake model (fake_model.py; no real provider is ever used). All are in cases.PROVING and cases.PROVING_FAKE: they run in the fix tier's
# agent@fake pass (agent mode; the fake pass exists in agent mode only, run.py fake_pass_plan). Registered by one import line at the end of cases.py.
#   agent_model_switch         T11  two fake providers on two ports. A prompt goes to provider A; the model is switched BY KEYBOARD mid-conversation
#                                   (`/model` + Enter, the model's name, Enter); the next prompt must reach provider B only, carrying the earlier turn
#                                   exactly once; after the switch back the third prompt reaches A only with both earlier turns. Each provider has its own
#                                   key header (extra_headers): A's must never arrive at B, B's never at A. Oracles: both fakes' request logs (every
#                                   purpose, fake.seen), their headers, and the app's own `[byop] opening stream: model=` line.
#   agent_anthropic_wire       T12  the provider is api_type = "anthropic" pointing at the same fake (it answers /v1/messages in the Anthropic event-stream
#                                   format). First a text turn that must arrive ON THAT WIRE (the precondition of everything else), then the oracles of
#                                   agent_approval (a tool call with approval), agent_question_answer (a question card) and agent_fault_disconnect (a cut
#                                   stream), each required to have used the Anthropic wire only.
#   agent_autorun_permissions  T13  the safe read-only allow list (agents.warp_agent.permissions.safe_command_allowlist_enabled). OFF: `ls` shows a card (the
#                                   control: nothing auto-runs by itself). ON: `ls <dir>` runs with no card and the model gets the listing; `rm f`,
#                                   `ls d; rm f`, `ls $(rm f)` and `cat script | sh` each show a card and the file `rm` names still exists before Enter (and
#                                   the app wrote nothing to the pty, where it logs pty writes); the approved `rm` then removes its file (the card was the
#                                   only thing holding it). The command reaches the model through a prose prompt ("run scripted command KEY").
#   agent_conversation_resume  T14  restore_session on; one finished turn; quit (Linux) or kill (macOS, Windows); relaunch; the conversation is reopened by
#                                   keyboard (`/conversations` + Enter, its title, Enter); a follow-up. The follow-up request must carry the earlier prompt
#                                   and the earlier reply exactly once each; the earlier text is shown (OCR, or the app's `hop=conversation_restored` where
#                                   there is no OCR); the app sends nothing on its own after the relaunch.
#   agent_block_context        T15  a command prints a token line and a string matching a custom secret regex; the next prompt in the conversation carries
#                                   the block as attached context. Redaction OFF (the owner's setting): the request holds both lines unchanged and a copy of
#                                   the block holds the secret. Redaction ON (hidden display): the token line still arrives, the secret is in NO request of
#                                   any purpose, and a copy of the block does not hold it.
# Every case edits the PROFILE's settings.toml with the app stopped and puts the file back byte for byte at the end (Settings); nothing is typed into a
# busy conversation; typing is at 10-25 ms per key with Enter in the same call. A step whose evidence is missing is `unverifiable` (noref), never a pass.
# Platforms: Linux (Wayland; X11 runs them too, nothing in them is Wayland-only), macOS, Windows Git Bash pane (winmap.GITBASH_ONLY has the reason for
# the PowerShell pane).
import os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases  # noqa: E402  (cases.py imports this module as its last line)
import cases_caseagent as AG  # noqa: E402
import cases_agent_resilience as AR  # noqa: E402
import cases_agent_question as AQ  # noqa: E402
import cases_robust as R  # noqa: E402
import cases_ui as U  # noqa: E402
import cases_wrap_vtabs as WV  # noqa: E402
import fake_model as FMOD  # noqa: E402
import toml_text as T  # noqa: E402
from cases import case  # noqa: E402

STREAM = "[byop] opening stream"
MODEL_RE = re.compile(r"\[byop\] opening stream: model=(\S+)")
RESTORED_RE = re.compile(r"hop=conversation_restored conv=(\S+) ai_blocks=(\d+) startup=([01])")
FIRST_ID, FIRST_MODEL = "appci-ollama", "fake-model"            # the pass's own provider (profile.py)
SECOND_ID, SECOND_MODEL = "appci-second", "zebramodel"          # a name no fuzzy match confuses with the first
ANT_ID, ANT_MODEL = "appci-anthropic", "fake-anthropic"
PROVIDERS_TABLE = "agents.warp_agent.providers"
SHELL_KIND = "run_shell_command"
KEYRING_FAILED_RE = re.compile(r"Couldn't read the API key from the keyring|Could not read agent provider API keys from the keyring")
SECRET_PATTERN = "APPCISEC_[A-Z0-9]{16}"


# ---------------------------------------------------------------- pure parts (selftest_agent_w2.py)
def provider_toml(pid, name, api_type, url, model, key=None):
    """the inline TOML table of one provider; `key` = a sentinel sent as the X-Appci-Key header (the key store is not scriptable; a header travels the same way)"""
    extra = ', extra_headers = [ [ "X-Appci-Key", "%s" ] ]' % key if key else ""
    return '{ id = "%s", name = "%s", api_type = "%s", base_url = "%s", models = [ { name = "%s", id = "%s" } ]%s }' % (pid, name, api_type, url, model, model, extra)


def llm_id(pid, model):
    """the app's id of a configured model (agent_providers/llm_id.rs: byop:<provider id>:<model id>)"""
    return "byop:%s:%s" % (pid, model)


def drop_array_tables(text, table):
    """`text` without every `[[table]]` section and its sub-tables (a bare profile writes the provider list in that form; an inline `providers = [...]`
    beside it would define the key twice)"""
    return "".join(text[hs:be] for name, hs, _bs, be in T._sections(text) if not (name == table or name.startswith(table + ".")))


def set_providers(text, providers, active):
    """settings.toml text with the provider list replaced by `providers` (inline tables) and `active` (llm_id) as the selected model"""
    t = drop_array_tables(text, PROVIDERS_TABLE)
    t = T.set_key(t, "agents.warp_agent", "providers", "[ %s ]" % ", ".join(providers))
    return T.set_key(t, "agents.byop", "last_used_model_id", '"%s"' % active)


def stream_models(text):
    """the model id of every `[byop] opening stream: model=` line, in order"""
    return MODEL_RE.findall(text or "")


def _txt(m):
    c = m.get("content")
    return c if isinstance(c, str) else ("" if c is None else str(c))


def judge_route(want, got_a, got_b, leak_a, leak_b):
    """problems for ONE prompt. want = 'A' | 'B', the provider selected when it was sent; got_x = how many chat prompts carrying its token provider x
    received as the newest user turn; leak_x = the purposes of every other request to x that carried the token (title, prediction, ...)."""
    here, there, leak, other = (got_a, got_b, leak_b, "B") if want == "A" else (got_b, got_a, leak_a, "A")
    out = []
    if here != 1:
        out.append("the prompt reached the selected provider %s %d times (expected exactly once)" % (want, here))
    if there:
        out.append("the prompt reached provider %s, which was NOT selected, %d time(s)" % (other, there))
    if leak:
        out.append("the prompt's text reached provider %s, which was NOT selected, in %d other request(s): %s" % (other, len(leak), sorted(set(leak))))
    return out


def judge_history(messages, earlier):
    """problems when a request's messages do not carry each earlier turn EXACTLY once. earlier = [(token of the user's prompt, text of the reply)]:
    0 = the turn was dropped (a new conversation, a lost history); 2 = it was restored or appended twice."""
    out = []
    for utok, atext in earlier:
        nu = sum(1 for m in messages or [] if m.get("role") == "user" and utok in _txt(m))
        na = sum(1 for m in messages or [] if m.get("role") == "assistant" and atext in _txt(m))
        if nu != 1:
            out.append("the earlier prompt %s is in the request %d times (expected exactly once)" % (utok, nu))
        if na != 1:
            out.append("the earlier reply %r is in the request %d times (expected exactly once)" % (atext, na))
    return out


def key_hits(headers, sentinel):
    """how many request header sets carry `sentinel` in any header value"""
    return sum(1 for h in headers or [] if any(sentinel in str(v) for v in h.values()))


def judge_keys(a_headers, b_headers, key_a, key_b):
    """(problems, missing precondition or None). Provider A's key header must never arrive at B and B's never at A; the precondition is that each
    provider did receive its OWN key (else 'never elsewhere' proves nothing)."""
    out = []
    if key_hits(b_headers, key_a):
        out.append("provider A's key header arrived at provider B in %d request(s)" % key_hits(b_headers, key_a))
    if key_hits(a_headers, key_b):
        out.append("provider B's key header arrived at provider A in %d request(s)" % key_hits(a_headers, key_b))
    missing = None
    if not key_hits(a_headers, key_a):
        missing = "provider A never received its own key header"
    elif not key_hits(b_headers, key_b):
        missing = "provider B never received its own key header"
    return out, missing


def shell_actions(text):
    """[(id, status)] of the run_shell_command hop=agent_action lines, in order"""
    return [(aid, st) for aid, kind, st in AR.action_hops(text) if kind == SHELL_KIND]


def autorun_verdict(text):
    """what happened to the FIRST shell command action of a log excerpt:
    'none'            no such action yet
    'card'            it reached `blocked` and never ran: an approval card is up
    'ran_no_card'     it finished (`done`) and was never `blocked`: it ran by itself
    'ran_after_card'  it was `blocked` and then ran: approved
    'pending'         anything else (queued or running, not finished)"""
    acts = shell_actions(text)
    if not acts:
        return "none"
    first = acts[0][0]
    sts = [st for aid, st in acts if aid == first]
    blocked, ran, done = "blocked" in sts, "running" in sts or "done" in sts, "done" in sts
    if blocked and ran:
        return "ran_after_card"
    if blocked:
        return "card"
    if done:
        return "ran_no_card"
    return "pending"


def judge_arm(expect, verdict, victim_exists, pty_writes):
    """problems for one command of agent_autorun_permissions. expect = 'runs' (allowed: no card) | 'card'. verdict = autorun_verdict; victim_exists = the
    file the command would remove is still there (None when the command removes nothing); pty_writes = how many of the app's pty writes carried the
    command's token before Enter (None where the app logs no pty writes)."""
    out = []
    if expect == "runs":
        if verdict != "ran_no_card":
            out.append("the allowed command did not run by itself (%s)" % {"card": "an approval card is up", "none": "the model's command never became an action",
                                                                         "pending": "it never finished", "ran_after_card": "it was blocked first"}.get(verdict, verdict))
        return out
    if verdict in ("ran_no_card", "ran_after_card"):
        out.append("the command RAN without an approval (%s)" % verdict)
    elif verdict != "card":
        out.append("no approval card for the command (%s)" % verdict)
    if victim_exists is False:
        out.append("the file the command removes is GONE before any approval")
    if pty_writes:
        out.append("the app wrote the command to the pty %d time(s) before any approval" % pty_writes)
    return out


def judge_context(content, token_line, secret, redaction_on):
    """problems with the user message that carried the question. content = that message as the model received it (attached context + prompt)."""
    content = content or ""
    if "<attached_context>" not in content:
        return ["the request carries no <attached_context>: the block's output was not attached to the question"]
    if token_line not in content:
        return ["the attached context does not hold the block's output (the token line %r is missing)" % token_line]
    if redaction_on and secret in content:
        return ["redaction is ON and the request still holds the secret"]
    if not redaction_on and secret not in content:
        return ["redaction is OFF and the request does not hold the output unchanged (the secret-shaped string is missing or altered)"]
    return []


def judge_copy(got, token_line, secret, redaction_on, sentinel):
    """(problems, could not judge or None) for a copy of the block's output"""
    if got == sentinel:
        return [], "the copy-output key left the clipboard unchanged (the block was not selected)"
    if token_line not in (got or ""):
        return [], "the clipboard does not hold this block's output (another block was selected)"
    if redaction_on and secret in got:
        return ["redaction is ON (secrets hidden) and a copy of the block still holds the secret"], None
    if not redaction_on and secret not in got:
        return ["redaction is OFF and a copy of the block does not hold the output unchanged"], None
    return [], None


def restored_blocks(text, conv):
    """AI blocks the app says it put back for conversation `conv` (hop=conversation_restored), summed; None when the log has no such line at all"""
    rows = RESTORED_RE.findall(text or "")
    if not rows:
        return None
    return sum(int(n) for c, n, _s in rows if c == conv)


def last_conv(text):
    """the conversation id of the newest hop=turn line, or None"""
    st = AR.turn_states(text)
    return st[-1][0] if st else None


# ---------------------------------------------------------------- helpers
def need(ctx, r, name):
    R.need_relaunch_app(ctx, name)
    AR.need_fake(ctx, r)


class Settings:
    """The PROFILE's settings.toml, edited only while the app is stopped, and put back byte for byte by restore(). (Linux: the app reads this file;
    macOS / Windows: App.start() copies it to the app's own path when it changed.)"""

    def __init__(self, ctx, r, label):
        self.ctx, self.r, self.label, self.n = ctx, r, label, 0
        self.path = R.settings_file(ctx)
        with open(self.path, "rb") as f:
            self.saved = f.read()

    def relaunch(self, edit=None, hard=False, restored_min=1):
        """stop the app (its quit key on Linux, by pid on macOS / Windows; hard=True: kill -9 / taskkill), apply edit(text) -> text, start it again.
        True when it is up."""
        ctx, r = self.ctx, self.r
        self.n += 1
        tag = "%s_%d" % (self.label, self.n)
        if hard:
            if not R.hard_kill(ctx, r, tag):
                return False
        else:
            q = R.stop_for_restart(ctx, r, 20, tag)
            if not r.check(q["exited"], "%s: the app did not exit for the settings change (%r)" % (tag, q)):
                return False
            R.keep_log(ctx, tag)
        if edit:
            with open(self.path, encoding="utf-8", errors="replace") as f:
                text = f.read()
            out = edit(text)
            with open(self.path, "w", encoding="utf-8") as f:
                f.write(out)
        _n, up = R.start_app(ctx, r, tag, restored_min=restored_min)
        return up

    def restore(self):
        ctx, r = self.ctx, self.r
        try:
            if ctx.app.alive():
                q = R.stop_for_restart(ctx, r, 20, self.label + "_cleanup")
                if not q["exited"]:
                    r.note("%s: the cleanup stop did not exit; falling back to the harness stop" % self.label)
            R.keep_log(ctx, self.label + "_end")
            with open(self.path, "wb") as f:
                f.write(self.saved)
            R.start_app(ctx, r, self.label + "_fresh")
        except Exception as e:                                # never mask the case's own verdict
            r.check(False, "%s: could not put settings.toml back and leave a fresh app: %r" % (self.label, e))


def prompt_turn(ctx, r, text, label, timeout=60):
    """type one prompt and wait for its turn to end -> (log offset before it, final turn state or None when no model request was made)"""
    off = ctx.log_size()
    AG.type_fast(ctx, text)
    if not AG.wait_marker(ctx, off, STREAM, timeout=60):
        ctx.shot("%s_no_request" % label)
        r.check(False, "%s: no model request within 60 s of the prompt (not routed to the agent, or never sent)" % label)
        return off, None
    return off, AR.wait_turn(ctx, off, ("success", "error", "cancelled"), timeout)


def last_request_with(f, tok):
    """the messages of the newest chat request whose newest user turn (what the user typed) holds `tok`, or None"""
    for q in reversed(list(f.reqs)):
        users = [m for m in q["messages"] if m.get("role") == "user"]
        if users and tok in FMOD.prompt_text(_txt(users[-1])):
            return q["messages"]
    return None


def other_purposes(f, tok, since):
    """purposes of every non-chat request since `since` whose text holds tok"""
    return [x["purpose"] for x in f.seen_with(tok, since) if x["purpose"] != "chat"]


# ================================================================= T11 agent_model_switch
def switch_model(ctx, name, label):
    """keyboard only: `/model` + Enter opens the inline model selector (terminal/input/slash_commands: a typed /model), the model's name filters it,
    Enter selects the row"""
    AG.type_fast(ctx, "/model")
    time.sleep(1.2)
    ctx.shot("%s_menu" % label)
    AG.type_fast(ctx, name, enter=False)
    time.sleep(0.8)
    ctx.shot("%s_filtered" % label)
    ctx.be.key("Return")
    time.sleep(1.5)


@case("agent_model_switch", modes=("agent",), order=57, gui=True)
def c_agent_model_switch(ctx, r):
    """T11: a model switch by keyboard mid-conversation moves the next prompt to the other provider, with the earlier turns, and to that provider only;
    neither provider ever receives the other's key header."""
    need(ctx, r, "agent_model_switch")
    a = AR.fake()
    b = FMOD.FakeModel()
    b.log_path = None                                         # the run's request log is provider A's
    b.start(active=False)
    st = Settings(ctx, r, "msw")
    suffix = ctx.tok(4).lower()
    key_a, key_b = "appci-key-a-" + suffix, "appci-key-b-" + suffix
    try:
        a.clear_faults()
        provs = [provider_toml(FIRST_ID, "CI Ollama", "ollama", a.url, FIRST_MODEL, key_a), provider_toml(SECOND_ID, "CI Second", "ollama", b.url, SECOND_MODEL, key_b)]
        if not st.relaunch(lambda t: set_providers(t, provs, llm_id(FIRST_ID, FIRST_MODEL))):
            return
        AG.fresh_conversation(ctx, r)
        ha, hb = len(a.headers), len(b.headers)
        toks = ["MA" + ctx.tok(6).upper(), "MB" + ctx.tok(6).upper(), "MC" + ctx.tok(6).upper()]
        plan = (("A", FIRST_MODEL, None), ("B", SECOND_MODEL, SECOND_MODEL), ("A", FIRST_MODEL, FIRST_MODEL))
        earlier = []
        for i, (want, model, switch_to) in enumerate(plan):
            label = "msw_%d_%s" % (i + 1, want)
            if switch_to:
                switch_model(ctx, switch_to, label)
            sa, sb = len(a.seen), len(b.seen)
            off, state = prompt_turn(ctx, r, "Reply with the word %s" % toks[i], label)
            if state is None:
                return
            r.check(state == "success", "%s: the turn ended %r, expected `success`" % (label, state))
            time.sleep(2.5)                                    # a title or prediction request for this prompt goes out here
            ctx.shot(label)
            for p in judge_route(want, a.fresh_with(toks[i]), b.fresh_with(toks[i]), other_purposes(a, toks[i], sa), other_purposes(b, toks[i], sb)):
                r.check(False, "%s: %s" % (label, p))
            models = stream_models(ctx.log_since(off))
            r.check(models[-1:] == [model], "%s: the app's own request line names model %r, expected %r (`[byop] opening stream: model=`)" % (label, models[-1:], model))
            msgs = last_request_with(a if want == "A" else b, toks[i])
            if earlier and msgs is not None:
                for p in judge_history(msgs, earlier):
                    r.check(False, "%s (provider %s's request): %s" % (label, want, p))
            earlier.append((toks[i], "OK " + toks[i]))
            if i == 1 and b.fresh_with(toks[1]) == 0:
                r.note("the keyboard switch (`/model`, Enter, %r, Enter) did not move the conversation to provider B: see shots/*msw_2_B_menu.png and _filtered.png" % SECOND_MODEL)
        probs, missing = judge_keys(a.headers[ha:], b.headers[hb:], key_a, key_b)
        for p in probs:
            r.check(False, p)
        if missing:
            r.unverifiable("%s, so 'the other provider's key never arrives' is not judged (extra_headers not sent?)" % missing)
        r.metric("model_switch_requests", dict(a=len(a.headers) - ha, b=len(b.headers) - hb))
        for p in AR.judge_errors(ctx.log_since(0), limit=0):
            r.check(False, p)
    finally:
        st.restore()
        b.stop()
        AG.cleanup_tmp(ctx)


# ================================================================= T12 agent_anthropic_wire
def wire_step(ctx, r, f, label, body, min_chat):
    """run one borrowed oracle and require that its model traffic used the Anthropic wire only. A Skip raised inside (a precondition the borrowed
    case could not prove) is a FAILURE here: the model is scripted, so e.g. 'no approval card' means the app did not take the tool call off this wire."""
    w0, unk0 = dict(f.by_wire), len(f.unknown)
    try:
        body()
    except cases.Skip as e:
        r.check(False, "%s on the Anthropic wire could not be completed: %s" % (label, e))
    d_ant, d_oll = f.by_wire["anthropic"] - w0["anthropic"], f.by_wire["ollama"] - w0["ollama"]
    r.metric("anthropic_wire_%s_requests" % label, dict(anthropic=d_ant, ollama=d_oll))
    r.check(d_ant >= min_chat, "%s: only %d chat request(s) arrived on the Anthropic wire (at least %d expected)" % (label, d_ant, min_chat))
    r.check(d_oll == 0, "%s: %d chat request(s) arrived on the Ollama wire: the step did not test the Anthropic format" % (label, d_oll))
    r.check(len(f.unknown) == unk0, "%s: the fake model could not answer %d request(s): %s" % (label, len(f.unknown) - unk0, f.unknown[unk0:][:3]))


@case("agent_anthropic_wire", modes=("agent",), order=57, gui=True)
def c_agent_anthropic_wire(ctx, r):
    """T12: text, a tool call with approval, a question and a cut stream against a provider that speaks the Anthropic Messages stream"""
    need(ctx, r, "agent_anthropic_wire")
    f = AR.fake()
    st = Settings(ctx, r, "antw")
    try:
        f.clear_faults()
        provs = [provider_toml(FIRST_ID, "CI Ollama", "ollama", f.url, FIRST_MODEL), provider_toml(ANT_ID, "CI Anthropic", "anthropic", f.url, ANT_MODEL)]
        if not st.relaunch(lambda t: set_providers(t, provs, llm_id(ANT_ID, ANT_MODEL))):
            return
        AG.fresh_conversation(ctx, r)
        # 0. the precondition of every step below: a plain prompt arrives ON THE ANTHROPIC WIRE and its streamed text ends the turn in `success`
        tok = "AW" + ctx.tok(6).upper()
        s0, h0 = len(f.seen), len(f.headers)
        off, state = prompt_turn(ctx, r, "Reply with the word %s" % tok, "antw_text")
        if KEYRING_FAILED_RE.search(ctx.log_since(0)):
            r.unverifiable("this session's keyring cannot be read. A provider type that can carry a key (every type but Ollama) waits for the keyring before it "
                           "sends (agent_providers::must_wait_for_keys), so no Anthropic-format request can be made here. Missing capability: a readable keyring in the test session")
            return
        on_wire = f.seen_with(tok, s0, wire="anthropic", purpose="chat")
        r.check(not f.seen_with(tok, s0, wire="ollama"), "the prompt reached the fake model on the OLLAMA wire: the provider is not speaking the Anthropic format")
        if not r.check(len(on_wire) >= 1, "the prompt never arrived on the Anthropic wire (POST .../v1/messages): nothing below can be judged (turn state %r)" % state):
            return
        r.check(f.fresh_with(tok) == 1, "the prompt reached the model %d times (expected once)" % f.fresh_with(tok))
        r.check(state == "success", "the text turn ended %r, expected `success` (the Anthropic event stream was not read to its message_stop)" % state)
        r.check(any("anthropic-version" in h for h in f.headers[h0:]), "no request carried the anthropic-version header")
        r.check(stream_models(ctx.log_since(off))[-1:] == [ANT_MODEL], "the app's request line names model %r, expected %r" % (stream_models(ctx.log_since(off))[-1:], ANT_MODEL))
        seen = AG.ocr_has(ctx, "antw_text_ocr", "OK " + tok)
        if seen is None:
            r.note("no OCR on this runner: the streamed text is judged by the turn state only")
        else:
            r.check(seen, "the reply text 'OK %s' is not on screen (the text deltas were not shown)" % tok)
        # 1. a tool call with approval: agent_approval's oracles (card, focus Enter, approve once, exactly one run, follow-up, reject)
        wire_step(ctx, r, f, "approval", lambda: AG.c_agent_approval(ctx, r), 3)
        # 2. a question: agent_question_answer's oracles (the card is answerable, Enter answers, the model gets the answer, the next prompt works)
        wire_step(ctx, r, f, "question", lambda: AQ.run_variant(ctx, r, "ant_answer", None), 3)
        # 3. a cut stream: agent_fault_disconnect's oracles (error or one resume, nothing pending, no endless retry, the re-sent prompt works once)
        wire_step(ctx, r, f, "cut", lambda: AR.fault_body(ctx, r, "ant_fdisc", "disconnect", lambda fk, t: fk.inject("disconnect", times=1, match=t, after=3), 90,
                                                         r"stream chunk error|without a terminal End|connection closed|error decoding", "incomplete"), 2)
    finally:
        f.clear_faults()
        st.restore()
        AG.cleanup_tmp(ctx)


# ================================================================= T13 agent_autorun_permissions
def set_safe_allowlist(text, on):
    t = T.set_key(text, "agents.warp_agent.permissions", "safe_command_allowlist_enabled", "true" if on else "false")
    return T.set_key(t, "agents.warp_agent.permissions", "safe_command_allowlist", '[ "ls" ]')


def ask_scripted(ctx, r, key, command, label):
    """a fresh conversation, the prose prompt for the registered `command`, and the settled verdict of its action -> (log offset, verdict)"""
    FMOD.SCRIPTED_COMMANDS[key] = command
    AG.fresh_conversation(ctx, r)
    off = ctx.log_size()
    AG.type_fast(ctx, "Please run scripted command %s" % key)
    if not AG.wait_marker(ctx, off, STREAM, timeout=60):
        ctx.shot("%s_no_request" % label)
        r.check(False, "%s: no model request within 60 s of the prompt" % label)
        return off, None
    got = []

    def settled():
        got.append(autorun_verdict(ctx.log_since(off)))
        return len(got) >= 3 and got[-1] == got[-2] == got[-3] and got[-1] in ("card", "ran_no_card", "ran_after_card")
    ctx.until(settled, 30, step=0.5)
    time.sleep(1.0)
    ctx.shot(label)
    return off, autorun_verdict(ctx.log_since(off))


def reject_card(ctx, off):
    ctx.be.key("Return")            # focus the card (#1195/#1350)
    time.sleep(0.8)
    ctx.be.key("ctrl+c")            # reject it (or cancel the conversation): either way the command must not run
    ctx.ctrlc_ts.append(time.time())
    ctx.until(lambda: not AR.open_actions(ctx.log_since(off)), 8, step=0.25)
    time.sleep(1.0)


@case("agent_autorun_permissions", modes=("agent",), order=57, gui=True)
def c_agent_autorun_permissions(ctx, r):
    """T13: with the safe allow list on, `ls` runs with no card; `rm`, `ls; rm`, `ls $(rm)` and a pipe to `sh` each show a card and change nothing before Enter"""
    need(ctx, r, "agent_autorun_permissions")
    f = AR.fake()
    st = Settings(ctx, r, "arp")
    W = ctx.W
    tok = ctx.tok(5).lower().replace("_", "")
    d, inside = "arpdir%s" % tok, "arpseen%s" % tok
    victims = dict((k, "arpv%s%s.txt" % (k, tok)) for k in ("rm", "seq", "sub", "sh"))
    script = "arpscript%s.txt" % tok
    try:
        f.clear_faults()
        os.makedirs(os.path.join(W, d), exist_ok=True)
        for name in [os.path.join(d, inside), script] + list(victims.values()):
            with open(os.path.join(W, name), "w", newline="\n") as fh:
                fh.write("rm %s\n" % victims["sh"] if name == script else "x\n")
        exists = lambda k: os.path.exists(os.path.join(W, victims[k]))
        # control: with the allow list OFF the same `ls` asks. Without this, "ls ran with no card" could be the profile running everything by itself.
        if not st.relaunch(lambda t: set_safe_allowlist(t, False)):
            return
        off, v = ask_scripted(ctx, r, "ARPOFF" + tok, "ls %s" % d, "arp_off_ls")
        if v is None:
            return
        control_ok = r.check(v == "card", "control, allow list OFF: `ls` did not stop at an approval card (%s), so 'allowed runs with no card' below proves nothing about the allow list" % v)
        if v == "card":
            reject_card(ctx, off)
        if not st.relaunch(lambda t: set_safe_allowlist(t, True)):
            return
        # allowed: runs with no card, and the model receives the listing
        key = "ARPLS" + tok
        off, v = ask_scripted(ctx, r, key, "ls %s" % d, "arp_on_ls")
        if v is None:
            return
        for p in judge_arm("runs", v, None, None):
            r.check(False, "allow list ON, `ls`: %s" % p)
        if v == "ran_no_card":
            AR.wait_turn(ctx, off, ("success", "error", "cancelled"), 30)
            got = f.tool_results_for(key)
            r.check(any(inside in t for t in got), "allow list ON, `ls`: the model did not receive the listing (%d tool result(s), none names %s): the command did not really run" % (len(got), inside))
        elif v == "card":
            reject_card(ctx, off)
        if not control_ok:
            r.note("the OFF control failed, so the ON result for `ls` is not evidence about the allow list")
        # each of the others: a card, and nothing happened before Enter
        arms = (("rm", "rm %s" % victims["rm"]), ("seq", "ls %s; rm %s" % (d, victims["seq"])), ("sub", "ls $(rm %s)" % victims["sub"]), ("sh", "cat %s | sh" % script))
        for k, command in arms:
            label = "arp_%s" % k
            off, v = ask_scripted(ctx, r, "ARP%s%s" % (k.upper(), tok), command, label)
            if v is None:
                return
            log = ctx.log_since(off)
            writes = AG.approved_command_writes(log, victims[k] if k != "sh" else script) if AG.pty_write_hop_logged(log) else None
            for p in judge_arm("card", v, exists(k), writes):
                r.check(False, "`%s`: %s" % (command, p))
            if v != "card":
                continue
            time.sleep(2.0)                                # still held after a pause
            r.check(exists(k), "`%s`: the file is gone while the card is still waiting" % command)
            if k == "rm":
                # the positive control of the hold: approved, the very same command removes the file (the card was the only thing stopping it)
                ctx.be.key("Return")
                time.sleep(0.8)
                ctx.be.key("Return")
                gone = ctx.until(lambda: not exists(k), 30, step=0.25)
                r.check(gone, "`%s`: approved with Enter, the file is still there after 30 s (the command could never have run, so the hold above proves nothing)" % command)
                AR.wait_turn(ctx, off, ("success", "error", "cancelled"), 30)
            else:
                reject_card(ctx, off)
                r.check(exists(k), "`%s`: the file is gone after the card was rejected" % command)
        for p in AR.judge_errors(ctx.log_since(0), limit=0):
            r.check(False, p)
    finally:
        for key in [k for k in FMOD.SCRIPTED_COMMANDS if k.endswith(tok)]:
            FMOD.SCRIPTED_COMMANDS.pop(key, None)
        st.restore()
        AG.cleanup_tmp(ctx)


# ================================================================= T14 agent_conversation_resume
def reopen_conversation(ctx, title_query, label):
    """keyboard only: from the terminal view, `/conversations` + Enter opens the conversation list, the title filters it, Enter continues it in this
    pane -> True when the pane is in the agent view afterwards"""
    if ctx.log_view_mode() == "agent":
        ctx.reset()
        time.sleep(0.4)
        ctx.be.key("Escape")
        ctx.until(lambda: ctx.log_view_mode() == "terminal", 6)
        time.sleep(0.5)
    AG.type_fast(ctx, "/conversations")
    time.sleep(1.5)
    ctx.shot("%s_list" % label)
    AG.type_fast(ctx, title_query, enter=False)
    time.sleep(1.0)
    ctx.shot("%s_filtered" % label)
    ctx.be.key("Return")
    ok = ctx.until(lambda: ctx.log_view_mode() == "agent", 12)
    time.sleep(2.0)                                           # a stored conversation is loaded and its blocks are put back here
    return bool(ok)


@case("agent_conversation_resume", modes=("agent",), order=58, gui=True)
def c_agent_conversation_resume(ctx, r):
    """T14: a finished conversation, reopened by keyboard after a restart, continues with its earlier turns sent exactly once"""
    need(ctx, r, "agent_conversation_resume")
    f = AR.fake()
    st = Settings(ctx, r, "acr")
    hard = ctx.be.name == "mac" or ctx.win                    # the table: quit on Linux, kill on macOS and Windows
    try:
        f.clear_faults()
        if not st.relaunch(lambda t: T.set_key(t, "general", "restore_session", "true")):
            return
        AG.fresh_conversation(ctx, r)
        tag = "zq" + U.ocr_safe(ctx.tok(8))
        p1 = "RP" + ctx.tok(6).upper()
        titles0 = f.by_purpose.get("title", 0)
        off, state = prompt_turn(ctx, r, "Reply with the word %s for conversation tag %s" % (p1, tag), "acr_first")
        if state is None:
            return
        if not r.check(state == "success", "the first turn ended %r, expected `success`: there is no finished conversation to reopen" % state):
            return
        conv = last_conv(ctx.log_since(off))
        ctx.until(lambda: f.by_purpose.get("title", 0) > titles0, 10, step=0.5)
        titled = f.by_purpose.get("title", 0) > titles0
        r.metric("resume_title_requested", titled)
        time.sleep(3.0)                                       # the store has the finished turn
        ctx.shot("acr_before_stop")
        req0 = f.requests
        if not st.relaunch(None, hard=hard):
            return
        time.sleep(5.0)                                       # anything the app would send on its own happens here
        r.check(f.requests == req0, "the app sent %d model request(s) on its own after the relaunch" % (f.requests - req0))
        in_view = reopen_conversation(ctx, tag, "acr_reopen")
        log = ctx.log_since(0)
        if not r.check(in_view, "after `/conversations`, Enter, %r, Enter the pane is not in the agent view (hop=view_mode %s): the conversation was not reopened%s"
                       % (tag, ctx.log_view_mode(), "" if titled else " (the app asked for no title, so the list could not be filtered by it)")):
            ctx.be.key("Escape")
            return
        # the earlier text is shown: OCR where there is an engine, else the app's own record of the AI blocks it put back for THIS conversation
        seen = AG.ocr_has(ctx, "acr_reopened_ocr", p1)
        blocks = restored_blocks(log, conv) if conv else None
        r.metric("resume_restored_ai_blocks", blocks)
        if seen is not None:
            r.check(seen, "the earlier prompt %s is not on screen after the conversation was reopened (OCR)" % p1)
        elif blocks is not None:
            r.check(blocks >= 1, "the app put back %d AI blocks for conversation %s (hop=conversation_restored): the earlier turn is not shown" % (blocks, conv))
        else:
            r.unverifiable("no OCR engine and no hop=conversation_restored line in the app log (a build older than round 14): 'the earlier text is shown' is not judged")
        # the follow-up carries the earlier turn exactly once
        p2 = "RF" + ctx.tok(6).upper()
        off2, state2 = prompt_turn(ctx, r, "Reply with the word %s" % p2, "acr_followup")
        if state2 is None:
            return
        r.check(state2 == "success", "the follow-up ended %r, expected `success`" % state2)
        r.check(f.fresh_with(p2) == 1, "the follow-up reached the model %d times (expected once)" % f.fresh_with(p2))
        msgs = last_request_with(f, p2)
        if r.check(msgs is not None, "no request carries the follow-up as its newest user turn"):
            for p in judge_history(msgs, [(p1, "OK " + tag)]):
                r.check(False, "the follow-up request after the restart: %s" % p)
        conv2 = last_conv(ctx.log_since(off2))
        r.note("conversation id before the restart %s, of the follow-up %s (%s)" % (conv, conv2, "the same" if conv and conv == conv2 else "DIFFERENT: the request content above is the verdict"))
        ctx.shot("acr_followup")
        for p in AR.judge_errors(log, limit=0):
            if "unexpected ERROR" in p:
                r.check(False, "after the relaunch: %s" % p)
    finally:
        st.restore()
        AG.cleanup_tmp(ctx)


# ================================================================= T15 agent_block_context
def set_redaction(text, on):
    t = T.set_key(text, "privacy", "custom_secret_regex_list", '[ { pattern = "%s", name = "appci probe" } ]' % SECRET_PATTERN)
    t = T.set_key(t, "privacy.secret_redaction", "enabled", "true" if on else "false")
    return T.set_key(t, "privacy.secret_redaction", "secret_display_mode_setting", '"asterisks"')


def context_phase(ctx, r, f, on):
    """one redaction setting: a first turn, the command, a copy of its block, the question -> False when the phase could not run"""
    label = "abc_%s" % ("on" if on else "off")
    AG.fresh_conversation(ctx, r)
    _off, state = prompt_turn(ctx, r, "Reply with the word BF%s" % ctx.tok(5).upper(), label + "_first")
    if state != "success":
        r.check(state is None, "%s: the first turn ended %r, expected `success`" % (label, state))
        return False
    time.sleep(1.0)
    tok = ctx.tok(6).lower().replace("_", "")
    secret = "APPCISEC_" + "".join(ctx.rng.choice("ABCDEFGHJKLMNPQRSTUVWXYZ23456789") for _ in range(16))
    token_line = "bcline %s" % tok
    fname = "abc_%s.txt" % tok
    with open(os.path.join(ctx.W, fname), "w", newline="\n") as fh:
        fh.write("%s\nseckey %s tail\n" % (token_line, secret))
    mark = ctx.cmd_mark()
    ran = U.run_cmd(ctx, "cat %s" % fname)
    if not r.check(ran and ctx.cmds_since(mark)[-1:] == ["cat %s" % fname], "%s: `cat %s` did not reach the shell as typed (received %r): there is no block to ask about" % (label, fname, ctx.cmds_since(mark))):
        return False
    time.sleep(1.5)
    # a copy of the block's output, by keyboard (block-up selects the newest block)
    try:
        ctx.be.key(WV.KX(ctx, "block_up"))
        time.sleep(0.6)
        got = WV.clip_after(ctx, WV.KX(ctx, "copy_output"), label + "_copy")
    finally:
        WV.back_to_input(ctx, r)
    probs, cannot = judge_copy(got, token_line, secret, on, WV.SENTINEL)
    for p in probs:
        r.check(False, "%s: %s" % (label, p))
    if cannot:
        r.unverifiable("%s: %s: the copy is not judged (shots/*%s_copy.png)" % (label, cannot, label))
    time.sleep(0.5)
    # the question about that block
    q = "BQ" + ctx.tok(6).upper()
    s0 = len(f.seen)
    _off, state = prompt_turn(ctx, r, "What does the output above say? Reply with the word %s" % q, label + "_ask")
    if state is None:
        return False
    r.check(state == "success", "%s: the question's turn ended %r, expected `success`" % (label, state))
    time.sleep(2.5)                                           # title / prediction requests of this prompt go out here
    msgs = last_request_with(f, q)
    if r.check(msgs is not None, "%s: no request carries the question as its newest user turn" % label):
        content = _txt([m for m in msgs if m.get("role") == "user"][-1])
        for p in judge_context(content, token_line, secret, on):
            r.check(False, "%s: %s" % (label, p))
    if on:
        leaks = f.seen_with(secret, s0)
        r.check(not leaks, "%s: the secret left for the provider in %d request(s) of purpose %s" % (label, len(leaks), sorted({x["purpose"] for x in leaks})))
    ctx.shot(label)
    return True


@case("agent_block_context", modes=("agent",), order=58, gui=True)
def c_agent_block_context(ctx, r):
    """T15: a block's output as context of the next prompt: unchanged with redaction off, without the secret (request and copy) with redaction on"""
    need(ctx, r, "agent_block_context")
    f = AR.fake()
    st = Settings(ctx, r, "abc")
    try:
        f.clear_faults()
        for on in (False, True):
            if not st.relaunch(lambda t, on=on: set_redaction(t, on)):
                return
            if not context_phase(ctx, r, f, on):
                return
        for p in AR.judge_errors(ctx.log_since(0), limit=0):
            r.check(False, p)
    finally:
        st.restore()
        AG.cleanup_tmp(ctx)
