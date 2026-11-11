# appci/selftest_agent_w2.py: offline self-tests of cases_agent_w2.py (round 14 wave 2, T11-T15) and of the fake model's Anthropic wire. Pure Python,
# loopback only. selftest.py appends CHECKS to its own list (two lines at its end), so `run.py --self-test` runs them.
# Every judge is fed a right input AND the wrong ones it exists to catch: an oracle that cannot fail is not a test.
import json, os, re, sys, urllib.error, urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

CHECKS = []
NAMES = ("agent_model_switch", "agent_anthropic_wire", "agent_autorun_permissions", "agent_conversation_resume", "agent_block_context")


def check(f):
    CHECKS.append(f)
    return f


@check
def w2_cases_registered_proving_fake_and_classified():
    """the five cases are registered agent-only, in PROVING and PROVING_FAKE (never SMOKE), run in the Git Bash pane and skip with a reason in PowerShell,
    and docs/testing.md names each with its cost estimate"""
    import cases, winmap
    import cases_agent_w2  # noqa: F401
    reg = {c["name"]: c for c in cases.CASES}
    for n in NAMES:
        assert n in reg and reg[n]["modes"] == ("agent",), n
        assert n in cases.PROVING and n in cases.PROVING_FAKE and n not in cases.SMOKE, n
        assert winmap.skip_reason(n, "gitbash") is None, n
        why = winmap.skip_reason(n, "powershell")
        assert why and "Git Bash pane" in why and len(why) > 60, (n, why)
    doc = open(os.path.join(HERE, "..", "..", "..", "docs", "testing.md"), encoding="utf-8").read()
    sec = doc[doc.index("## Round 14 wave 2: agent and state cases"):]
    for n in NAMES:
        assert ("`%s`" % n) in sec, "docs/testing.md has no row for %s" % n
    assert "min (estimate)" in sec


@check
def w2_fake_model_anthropic_wire_pure():
    """the Anthropic request is read into the shape every trigger uses, and the reply is a well-formed Anthropic event stream: the text as text
    deltas, a tool call as a tool_use block whose input_json_delta pieces join to the arguments, the right stop_reason, message_stop last"""
    import fake_model as FM
    assert FM.is_anthropic_path("/v1/messages") and FM.is_anthropic_path("/v1/messages/?x=1") and not FM.is_anthropic_path("/api/chat")
    env = "<environment_context>\n  <cwd>/w</cwd>\n</environment_context>"
    body = {"model": "fake-anthropic", "stream": True, "system": [{"type": "text", "text": "big agent prompt"}, {"type": "text", "text": "more"}],
            "tools": [{"name": "run_shell_command", "description": "d", "input_schema": {}}],
            "messages": [{"role": "user", "content": "Please run exactly this shell command and nothing else: echo T >> f.txt"},
                         {"role": "assistant", "content": [{"type": "text", "text": "ok "}, {"type": "tool_use", "id": "toolu_1", "name": "run_shell_command", "input": {"command": "echo T >> f.txt"}}]},
                         {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "toolu_1", "content": "exit 0"}, {"type": "text", "text": env}]}]}
    req = FM.anthropic_request(body)
    msgs = req["messages"]
    assert [m["role"] for m in msgs] == ["system", "user", "assistant", "tool", "user"], msgs
    assert msgs[0]["content"] == "big agent prompt\n\nmore" and msgs[3]["content"] == "exit 0" and msgs[2]["tool_calls"][0]["function"]["name"] == "run_shell_command"
    assert req["tools"] == [{"function": {"name": "run_shell_command"}}] and FM.classify_purpose("/api/chat", req) == "chat"
    assert FM.reply_for(FM.clean_messages(msgs)) == {"text": "Done.", "tool": None}, "a tool_result turn (with the environment block after it) is answered Done."
    first = FM.anthropic_request(dict(body, messages=body["messages"][:1]))
    reply = FM.reply_for(first["messages"])
    ev = FM.anthropic_events("m", reply)
    names = [n for n, _d in ev]
    assert names[0] == "message_start" and names[-2:] == ["message_delta", "message_stop"], names
    assert ev[1][1]["content_block"] == {"type": "tool_use", "id": "toolu_fake_0", "name": "run_shell_command", "input": {}}
    parts = [d["delta"]["partial_json"] for n, d in ev if n == "content_block_delta"]
    assert len(parts) == 2 and json.loads("".join(parts))["command"] == "echo T >> f.txt", parts
    assert ev[-2][1]["delta"]["stop_reason"] == "tool_use"
    tev = FM.anthropic_events("m", {"text": "hello world", "tool": None})
    assert "".join(d["delta"]["text"] for n, d in tev if n == "content_block_delta") == "hello world" and tev[-2][1]["delta"]["stop_reason"] == "end_turn"
    assert [n for n, _ in tev].count("content_block_start") == 1 and [n for n, _ in tev].count("content_block_stop") == 1
    wire = FM.sse(tev).decode()
    assert wire.startswith("event: message_start\ndata: {") and wire.endswith('event: message_stop\ndata: {"type": "message_stop"}\n\n')
    # two calls in one reply: two tool_use blocks with distinct ids and indexes
    two = FM.anthropic_events("m", FM.reply_for([{"role": "user", "content": "Please run a slow command then ask me a question Z8"}]), call_ids=["a", "b"])
    starts = [d for n, d in two if n == "content_block_start"]
    assert [s["content_block"]["id"] for s in starts] == ["a", "b"] and [s["index"] for s in starts] == [0, 1]
    one = json.loads(FM.anthropic_message_body("m", {"text": "Tagged X", "tool": None}))
    assert one["content"] == [{"type": "text", "text": "Tagged X"}] and one["stop_reason"] == "end_turn" and one["type"] == "message"
    assert json.loads(FM.anthropic_error_body(429))["error"]["type"] == "rate_limit_error"
    # the title tag, the scripted command trigger, and what a request carried
    title = [{"role": "system", "content": "You are a thread title generator."}, {"role": "user", "content": "Generate a title for this conversation:\n<user>Reply with the word P for conversation tag zqabc</user>"}]
    assert FM.purpose_reply("title", title)["text"] == "Tagged zqabc" and FM.purpose_reply("title", title[:1])["text"] == FM.PURPOSE_TEXT["title"]
    assert FM.purpose_reply("title")["text"] == FM.PURPOSE_TEXT["title"] and FM.purpose_reply("next_command", title)["text"] == "echo fake-next"
    FM.SCRIPTED_COMMANDS["SELFK1"] = "ls d; rm v"
    try:
        assert FM.reply_for([{"role": "user", "content": "Please run scripted command SELFK1"}])["tool"]["arguments"]["command"] == "ls d; rm v"
        assert FM.reply_for([{"role": "user", "content": "Please run scripted command NOPE"}]) == {"text": "NO-SCRIPTED-COMMAND NOPE", "tool": None}
        assert FM.reply_for([{"role": "user", "content": "Please run scripted command SELFK1"}, {"role": "tool", "content": "x"}])["text"] == "Done."
    finally:
        FM.SCRIPTED_COMMANDS.pop("SELFK1", None)
    assert "SECRETX" in FM.seen_text([{"role": "assistant", "content": "", "tool_calls": [{"function": {"arguments": {"c": "SECRETX"}}}]}])


@check
def w2_fake_model_anthropic_wire_served():
    """over HTTP: /v1/messages streams the Anthropic events, counts on the Anthropic wire only, takes the status / disconnect faults, refuses a fault
    with no Anthropic form loudly, answers a one-shot as one message; a second instance does not take over ACTIVE"""
    import fake_model as FM
    prev = FM.ACTIVE
    f = FM.FakeModel().start()
    g = FM.FakeModel().start(active=False)
    try:
        assert FM.ACTIVE is f and FM.FakeModel.ACTIVE is f and g.port != f.port

        def post(body, srv=f, path="/v1/messages"):
            rq = urllib.request.Request(srv.url + path, json.dumps(body).encode(), headers={"x-api-key": "", "anthropic-version": "2023-06-01"})
            try:
                with urllib.request.urlopen(rq, timeout=5) as resp:
                    return resp.headers.get("Content-Type"), resp.read().decode()
            except urllib.error.HTTPError as e:
                return "HTTP %d" % e.code, e.read().decode()
            except Exception as e:                            # a cut body
                return "CUT", type(e).__name__
        chat = lambda text: {"model": "m", "stream": True, "system": "agent", "tools": [{"name": "t"}], "messages": [{"role": "user", "content": text}]}
        ct, body = post(chat("Reply with the word AW1"))
        assert ct == "text/event-stream" and '"text": "OK AW1"' in body and body.rstrip().endswith('{"type": "message_stop"}'), (ct, body[-200:])
        assert f.by_wire == {"ollama": 0, "anthropic": 1} and f.requests == 1 and f.fresh_with("AW1") == 1
        assert len(f.seen_with("AW1", 0, wire="anthropic", purpose="chat")) == 1 and not f.seen_with("AW1", 0, wire="ollama")
        assert any("anthropic-version" in h for h in f.headers)
        ct, body = post({"model": "m", "stream": False, "system": "You are a thread title generator.", "messages": [{"role": "user", "content": "x conversation tag zq1"}]})
        assert json.loads(body)["content"][0]["text"] == "Tagged zq1" and f.requests == 1 and f.by_purpose["title"] == 1
        assert [x["purpose"] for x in f.seen_with("zq1")] == ["title"]
        f.inject("status", times=1, match="AW2", code=429, retry_after=1)
        ct, body = post(chat("Reply with the word AW2"))
        assert ct == "HTTP 429" and json.loads(body)["type"] == "error", (ct, body)
        f.inject("disconnect", times=1, match="AW3", after=2)
        ct, body = post(chat("Reply with the word AW3"))
        assert ct == "CUT", "a disconnect fault must cut the event stream, got %r" % (ct,)
        f.inject("ask_cut", times=1, match="AW4")
        ct, body = post(chat("Reply with the word AW4"))
        assert ct == "HTTP 501" and len(f.unknown) == 1 and "no Anthropic-wire form" in f.unknown[0]["system_head"], (ct, f.unknown)
        assert f.fired == ["status", "disconnect"], f.fired
        # the Ollama wire is unchanged and counted apart
        rq = urllib.request.Request(f.url + "/api/chat", json.dumps({"model": "m", "messages": [{"role": "user", "content": "Reply with the word OL1"}]}).encode())
        assert "OK OL1" in "".join(json.loads(l)["message"]["content"] for l in urllib.request.urlopen(rq, timeout=5).read().decode().splitlines())
        assert f.by_wire["ollama"] == 1 and g.requests == 0 and not g.seen
    finally:
        f.stop()
        g.stop()
        FM.ACTIVE = prev
        FM.FakeModel.ACTIVE = prev


@check
def w2_settings_edits():
    """the provider list replaces an inline list AND the array-of-tables form of a bare profile (never both forms in one file), and the per-case keys land"""
    import cases_agent_w2 as W
    import toml_text as T
    a = W.provider_toml("appci-ollama", "CI Ollama", "ollama", "http://127.0.0.1:1", "fake-model", "KEYA")
    b = W.provider_toml(W.ANT_ID, "CI Anthropic", "anthropic", "http://127.0.0.1:2", W.ANT_MODEL)
    assert 'api_type = "anthropic"' in b and "extra_headers" not in b and 'extra_headers = [ [ "X-Appci-Key", "KEYA" ] ]' in a
    assert W.llm_id(W.ANT_ID, W.ANT_MODEL) == "byop:appci-anthropic:fake-anthropic"
    bare = ('[general]\nrestore_session = false\n\n[[agents.warp_agent.providers]]\nid = "appci-ollama"\napi_type = "ollama"\nbase_url = "http://x"\n'
            'models = [{ name = "m", id = "m" }]\n\n[agents.byop]\nlast_used_model_id = "byop:appci-ollama:m"\n')
    out = W.set_providers(bare, [a, b], W.llm_id(W.ANT_ID, W.ANT_MODEL))
    assert "[[agents.warp_agent.providers]]" not in out and out.count("providers = [") == 1 and 'base_url = "http://x"' not in out, out
    assert T.get_key(out, "agents.byop", "last_used_model_id") == 'last_used_model_id = "byop:appci-anthropic:fake-anthropic"'
    assert T.get_key(out, "general", "restore_session") == "restore_session = false"
    inline = '[agents.warp_agent]\nproviders = [ { id = "old" } ]\nother = 1\n'
    out = W.set_providers(inline, [a], "byop:appci-ollama:fake-model")
    assert '{ id = "old" }' not in out and "other = 1" in out and out.count("providers = ") == 1
    assert W.drop_array_tables("[a]\nx = 1\n[[p]]\ny = 2\n[[p.models]]\nz = 3\n[b]\nw = 4\n", "p") == "[a]\nx = 1\n[b]\nw = 4\n"
    t = W.set_safe_allowlist("", True)
    assert T.get_key(t, "agents.warp_agent.permissions", "safe_command_allowlist_enabled").endswith("= true") and '[ "ls" ]' in t
    assert T.get_key(W.set_safe_allowlist(t, False), "agents.warp_agent.permissions", "safe_command_allowlist_enabled").endswith("= false")
    t = W.set_redaction("[privacy]\ncustom_secret_regex_list = [\n  { pattern = \"OWNER-[0-9]+\" },\n]\n", True)
    assert "OWNER-" not in t and W.SECRET_PATTERN in t and T.get_key(t, "privacy.secret_redaction", "enabled").endswith("= true") and '"asterisks"' in t
    assert re.fullmatch(W.SECRET_PATTERN, "APPCISEC_" + "A2" * 8)


@check
def w2_model_switch_judges():
    """T11: a prompt at the wrong provider, at both, twice, or leaked through a side request fails; history must hold each earlier turn exactly once;
    a key header at the other provider fails and a provider that never got its own key is 'not judged'"""
    import cases_agent_w2 as W
    assert W.judge_route("A", 1, 0, [], []) == [] and W.judge_route("B", 0, 1, [], []) == []
    assert W.judge_route("A", 1, 0, ["title"], []) == [], "a title request to the SELECTED provider is not a leak"
    assert "NOT selected" in W.judge_route("B", 1, 0, [], [])[1] and "0 times" in W.judge_route("B", 1, 0, [], [])[0]
    assert len(W.judge_route("B", 1, 1, [], [])) == 1 and len(W.judge_route("B", 0, 2, [], [])) == 1
    assert "title" in W.judge_route("B", 0, 1, ["title", "title"], [])[0] and W.judge_route("A", 1, 0, [], ["nld_predict"]) != []
    hist = [{"role": "system", "content": "s"}, {"role": "user", "content": "Reply with the word T1"}, {"role": "assistant", "content": "OK T1"},
            {"role": "user", "content": "Reply with the word T2"}]
    assert W.judge_history(hist, [("T1", "OK T1")]) == []
    assert len(W.judge_history(hist[:1] + hist[3:], [("T1", "OK T1")])) == 2, "a request without the earlier turn (a new conversation) fails"
    assert len(W.judge_history(hist[:3] + hist[1:], [("T1", "OK T1")])) == 2, "the earlier turn twice fails"
    assert len(W.judge_history(hist[:2] + hist[3:], [("T1", "OK T1")])) == 1, "the prompt without its reply fails"
    ka, kb = "key-a-1", "key-b-1"
    ha, hb = [{"x-appci-key": ka}, {"host": "h"}], [{"x-appci-key": kb}]
    assert W.judge_keys(ha, hb, ka, kb) == ([], None)
    assert W.judge_keys(ha, hb + [{"x-appci-key": ka}], ka, kb)[0] and W.judge_keys(ha + [{"authorization": "Bearer " + kb}], hb, ka, kb)[0]
    assert W.judge_keys([{"host": "h"}], hb, ka, kb) == ([], "provider A never received its own key header")
    assert W.judge_keys(ha, [], ka, kb)[1] == "provider B never received its own key header"
    assert W.stream_models("x [byop] opening stream: model=fake-model\ny [byop] opening stream: model=zebramodel\n") == ["fake-model", "zebramodel"]


@check
def w2_autorun_judges():
    """T13: the verdict of a shell action from the app log, and the arm judges: an allowed command that asks fails, a denied one that runs (with or
    without a card first), loses its file or reaches the pty fails"""
    import cases_agent_w2 as W
    h = lambda i, st, kind="run_shell_command": "hop=agent_action id=%s kind=%s status=%s\n" % (i, kind, st)
    assert W.autorun_verdict("") == "none" and W.autorun_verdict(h("q", "blocked", "ask_user_question")) == "none"
    assert W.autorun_verdict(h("a", "queued")) == "pending" and W.autorun_verdict(h("a", "queued") + h("a", "running")) == "pending"
    assert W.autorun_verdict(h("a", "queued") + h("a", "blocked")) == "card"
    assert W.autorun_verdict(h("a", "queued") + h("a", "running") + h("a", "done")) == "ran_no_card"
    assert W.autorun_verdict(h("a", "queued") + h("a", "blocked") + h("a", "running") + h("a", "done")) == "ran_after_card"
    assert W.autorun_verdict(h("a", "blocked") + h("a", "cancelled")) == "card", "a rejected card never ran"
    assert W.autorun_verdict(h("a", "blocked") + h("b", "running") + h("b", "done")) == "card", "the verdict is the FIRST action's"
    assert W.judge_arm("runs", "ran_no_card", None, None) == []
    for bad in ("card", "none", "pending", "ran_after_card"):
        assert W.judge_arm("runs", bad, None, None), bad
    assert W.judge_arm("card", "card", True, None) == [] and W.judge_arm("card", "card", True, 0) == []
    assert W.judge_arm("card", "ran_no_card", True, None) and W.judge_arm("card", "ran_after_card", True, None) and W.judge_arm("card", "none", True, None)
    assert "GONE" in W.judge_arm("card", "card", False, None)[0] and "pty" in W.judge_arm("card", "card", True, 1)[0]
    assert len(W.judge_arm("card", "ran_no_card", False, 2)) == 3


@check
def w2_resume_and_context_judges():
    """T14 / T15: the restored-block count per conversation; the question's context with redaction off and on; a copy of the block"""
    import cases_agent_w2 as W
    log = ("input_order hop=conversation_restored conv=c-1 ai_blocks=2 startup=1 t=5\ninput_order hop=conversation_restored conv=c-2 ai_blocks=4 startup=0 t=6\n"
           "input_order hop=turn conv=c-2 state=success t=7\n")
    assert W.restored_blocks(log, "c-1") == 2 and W.restored_blocks(log, "c-2") == 4 and W.restored_blocks(log, "c-9") == 0 and W.restored_blocks("no hop", "c-1") is None
    assert W.last_conv(log) == "c-2" and W.last_conv("") is None
    sec, line = "APPCISEC_ABCDEFGH23456789", "bcline tk1"
    att = "<attached_context>\ncommand: cat f\noutput:\n%s\nseckey %s tail\n</attached_context>\n\nWhat does the output above say?"
    off, on = att % (line, sec), att % (line, "*" * len(sec))
    assert W.judge_context(off, line, sec, False) == [] and W.judge_context(on, line, sec, True) == []
    assert "still holds the secret" in W.judge_context(off, line, sec, True)[0]
    assert "unchanged" in W.judge_context(on, line, sec, False)[0]
    assert "no <attached_context>" in W.judge_context("What does the output above say?", line, sec, True)[0], "no attachment must fail with redaction ON too (an empty request holds no secret)"
    assert "token line" in W.judge_context(att % ("other block", sec), line, sec, False)[0]
    S = "SENTINEL_NOT_COPIED"
    assert W.judge_copy("%s\nseckey %s tail" % (line, sec), line, sec, False, S) == ([], None)
    assert W.judge_copy("%s\nseckey **** tail" % line, line, sec, True, S) == ([], None)
    assert W.judge_copy("%s\nseckey %s tail" % (line, sec), line, sec, True, S)[0], "the secret on the clipboard with redaction ON fails"
    assert W.judge_copy("%s\nseckey **** tail" % line, line, sec, False, S)[0]
    assert W.judge_copy(S, line, sec, True, S) == ([], "the copy-output key left the clipboard unchanged (the block was not selected)")
    assert W.judge_copy("another block", line, sec, True, S)[1], "a copy of another block is 'not judged', never a pass of 'no secret'"
