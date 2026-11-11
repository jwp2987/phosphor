# appci/fake_model.py: the L2 fake model (docs/testing.md "Agent fuzzer and fake model"): a tiny Ollama-protocol server on 127.0.0.1, stdlib only.
# run.py starts it when APPCI_FAKE_MODEL=1 and no real provider is configured, and profile.py then writes the usual Ollama provider block pointing at it
# (id appci-ollama, so every provider-dependent case sees the same provider name). It is deterministic: the reply is a pure function of the request.
#   * a user turn "Please run exactly this shell command and nothing else: <cmd>" -> one run_shell_command tool call with that command (an approval card);
#   * "Please read exactly this file and nothing else: <path>" -> read_files; "Please create exactly this file: <path> with this content: <text>" and
#     "Please delete exactly this file and nothing else: <path>" -> apply_file_diffs (the remote file-tool lab cases, cases_lab_remote.py);
#   * "Please edit exactly this file: <path> with these edits: <search>=><replace> ;; <search>=><replace>" -> apply_file_diffs with one op "edit" per pair, in order
#     (a multi-hunk diff; a literal backslash-n in a pair is a newline; cases_agent_file_edits.py, #1835);
#   * "Please show this image: <url>" -> the text "Here it is:\n\n![img](<url>)", the image on its OWN line because the markdown parser only makes an image block from a line that is nothing else. The app does NOT fetch an http(s) image in agent output (is_supported_blocklist_image_source rejects http/https on purpose; the alt text "img" is drawn), so this turn makes no request: agent_gate_deny_allow step 0 asserts exactly that. The gate cases make their request through the markdown viewer instead (cases_agent_netgate.show_image, docs/design/r10-wl-gate.md);
#   * "Please carry out these steps in order: <step> ;; <step> ..." (agent_remote_*, cases_agent_remote.py): one tool call per step, the step being the number of tool
#     results already answered since that user turn; steps: `read <path>`, `grep <query> <dir>`, `glob <pattern> <dir>`, `edit <path> :: <search> :: <replace>`,
#     `run <command>`, `codebase <query>`; after the last step -> "Done.". Every request is recorded (FakeModel.turns) so a case can check the tool results the model
#     RECEIVED (tool_results_for);
#   * the turn after a tool result -> the text "Done." (a finished conversation);
#   * "Please stream long reply <TAG> chunks <N> gap <MS>" (#1833) -> N text chunks "[<TAG>:0001] ..." ... "[<TAG>:END]", one every MS ms on an open,
#     close-delimited stream (a reply that lasts N*MS ms). Each stream is recorded in fake.streams[TAG] (chunks sent, finished, broken);
#   * "Please follow up check <TAG>" -> the text "CHECKED <TAG> <verdict>", where history_check() read the conversation the app sent back: this tab's own
#     chunks all present, its END marker, and no marker of any other tag (cross-tab bleed). Recorded in fake.followups[TAG];
#   * "Please call mcp tool <TOOL> on <SERVER> [with <json>]" (#1834) -> a tool call to the function `mcp__<SERVER>__<TOOL>` if the app offered it in the
#     request's `tools`, else the text "NO-MCP-TOOL <SERVER>/<TOOL>" (the server did not connect). Every tool-role message is kept in fake.tool_results;
#     fake.tools_seen = every tool name ever offered, fake.user_turns = (t, last message content) of every request that did not end in a tool result;
#   * a conversation whose user turns contain STREAMLONG (#1828) -> a SLOW STEADY stream: STREAM_CHUNKS pieces STREAM_GAP_S apart (about 12 s), sent as
#     they are produced (no Content-Length), so a reply is visibly running while a case types. Sticky: every later turn of that conversation streams
#     too, so a steer (#851) typed while the reply runs is itself answered by a running stream. fake.reqs holds every /api/chat request body;
#   * "Please ask me a question <TAG>" (#1851) -> one ask_user_question call (one multiple-choice question, options "alpha" / "beta", TAG in the question text);
#     after the answer the turn after the tool result is "Done." like any other;
#   * "Please run a slow command then ask me a question <TAG>" (#1858) -> ONE reply with two calls: run_shell_command `sleep 8`, then ask_user_question;
#   * "Please run scripted command <KEY>" (round 14, agent_autorun_permissions) -> one run_shell_command call with SCRIPTED_COMMANDS[KEY], a command the
#     case registered beforehand. The prompt the case TYPES is plain prose: a command with `;`, `|` or `$(...)` typed into the input could be routed to
#     the shell by its syntax and run there, which would read as "ran with no card". An unknown key -> the text "NO-SCRIPTED-COMMAND <KEY>";
#   * any other user turn -> "OK <last word>" (prose only, no tool call).
# Trigger order in reply_for (no trigger shadows another): plan, tool-result "Done.", stream long reply, follow up check, call mcp tool, show this image,
# run a slow command then ask me a question, ask me a question, run scripted command, run / read / create / delete / edit exactly, STREAMLONG (sticky), SLOW, fallback.
# Every trigger and every fault `match` reads the text the USER typed (prompt_text): never the app's <environment_context> message, and never the
# <attached_context> block the app puts in front of the prompt (the blocks run since the last query, with their output; #2045).
# Fault injection (#1831, cases_agent_resilience.py): fake.inject(kind, times=1, match=None, **kw) arms a fault that the next `times` /api/chat requests
# (None = every one until clear_faults()) take instead of the normal answer; `match` = a substring of the request's last user text (None = any request).
#   status      kw code=429|500|.., retry_after=secs|None   -> that HTTP status with a JSON error body (and a Retry-After header)
#   disconnect  kw after=2 (lines)                           -> `after` NDJSON lines, then the connection is cut mid-body (reqwest: "connection closed")
#   delay       kw secs=30                                   -> nothing is sent for `secs` (the first token never comes); then the normal answer. A client
#                                                               that hangs up meanwhile is counted in fake.aborted (the proof that a cancel reached the wire)
#   drip        kw interval=0.5, words=60                    -> a long text reply, one 6-character line every `interval` s (a reply that visibly streams)
#   ask_cut     kw after_call=True                           -> (#1851) an ask_user_question call line, then the connection is cut (a stream cut right after a complete call)
#   ask_nodone  kw secs=60                                   -> (#1851) the call line, then nothing (no `done` line) for `secs`, then the close: a provider that never finishes
#   ask_trunc   -                                            -> (#1851) the call with truncated JSON arguments (a string cut mid-object), then `done`
#   ask_empty   -                                            -> (#1851) the call with `{}` arguments (no questions), then `done`
#   ask_slow    kw secs=3                                    -> (#1851) a text line, `secs` of silence on the open stream, then the call line and `done`
# Faults are applied after the request is recorded and before the reply is built or streamed.
# fake.go_down() closes the listening socket (connection refused: "the model is down"); fake.come_up() rebinds the same port.
# Oracles the cases read: fake.requests, fake.fresh (the text of every request whose LAST message is a user turn, i.e. each prompt the app actually sent),
# fake.fired (fault kinds in the order they hit), fake.aborted (clients that hung up before the answer finished), fake.turns, fake.headers.
# ACTIVE: the running instance, as the module global fake_model.ACTIVE and as FakeModel.ACTIVE (the same object; cleared by stop()).
# Request log (#1829): when APPCI_FAKE_MODEL_LOG names a file, every /api/chat request appends one JSON line {t, n, last_role, tool, users} (request_record):
# what the model was SENT, i.e. the command output the app handed back (`tool`) and every user message (`users`), so a case can prove the model's next request
# carried the output of an interactive command and that no key typed into a program leaked into the conversation.
# Endpoints: POST /api/chat (NDJSON stream), POST /api/show, GET /api/tags. The server counts requests (fake.requests) for the oracle.
# Anthropic wire (round 14, agent_anthropic_wire): the same server also answers the Anthropic Messages API. A POST whose path ends in /messages (the app
# posts <base_url>/v1/messages for a provider with api_type = "anthropic") is read as an Anthropic request (anthropic_to_messages: `system`, text /
# tool_use / tool_result content blocks -> the role/content list every trigger reads), answered by the SAME reply_for, and written back as the Anthropic
# event stream (anthropic_events: message_start, content_block_start / _delta / _stop for the text and for each tool_use with its input as
# input_json_delta pieces, message_delta with the stop_reason, message_stop), or as one message object when the request says "stream": false.
# Faults on that wire: status (an Anthropic error object), disconnect (kw after = text deltas sent before the cut), delay. Any other fault kind has
# no Anthropic form here: the request is answered 501 and listed in fake.unknown, never answered as if no fault were armed.
# fake.seen holds one {t, wire, purpose, text} per POST of EITHER wire and EVERY purpose (text = every message's content, the system prompt and
# tool-call arguments included): seen_with(token) is the oracle for "this text reached this provider through any request at all".
# A title request whose user text holds "conversation tag <TAG>" is answered "Tagged <TAG>" (agent_conversation_resume finds its conversation by it).
# A second instance (agent_model_switch: two providers on two ports) is started with start(active=False), which leaves ACTIVE alone.
import json, os, re, select, socket, sys, threading, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

READ_RE = re.compile(r"read exactly this file and nothing else:\s*(\S+)\s*$")
CREATE_RE = re.compile(r"create exactly this file:\s*(\S+)\s+with this content:\s*(.*?)\s*$", re.S)
DELETE_RE = re.compile(r"delete exactly this file and nothing else:\s*(\S+)\s*$")
EDIT_RE = re.compile(r"edit exactly this file:\s*(\S+)\s+with these edits:\s*(.*?)\s*$", re.S)
IMG_RE = re.compile(r"show this image:\s*(\S+)\s*$")
PLAN_RE = re.compile(r"carry out these steps in order:\s*(.+?)\s*$", re.S)
ASK_RE = re.compile(r"run exactly this shell command and nothing else:\s*(.+?)\s*$", re.S)
STREAM_RE = re.compile(r"stream long reply (\w+) chunks (\d+) gap (\d+)")
FOLLOW_RE = re.compile(r"follow up check (\w+)")
MCP_RE = re.compile(r"call mcp tool (\S+) on (\S+)(?: with (\{.*\}))?\s*$", re.S)
ASKCMD_RE = re.compile(r"run a slow command then ask me a question(?:\s+(\w+))?")      # #1858: [run_shell_command sleep, ask_user_question] in ONE reply
ASKQ_RE = re.compile(r"ask me a question(?:\s+(\w+))?")
SCRIPT_RE = re.compile(r"run scripted command (\w+)")
SCRIPTED_COMMANDS = {}       # KEY -> shell command, registered by a case before it types "Please run scripted command KEY"
MARK_RE = re.compile(r"\[(\w+):(\d{4}|END)\]")
STREAM_CHUNKS, STREAM_GAP_S = 240, 0.05


ACTIVE = None          # the running FakeModel (set by start()); the harness and the cases live in one process; cases_agent_netgate reads its request headers


def stream_chunks(tag, n):
    """pure: the n text chunks of a long scripted reply, then the END marker. Every chunk names its tag and its number, so a reply that landed in the
    wrong tab, lost a chunk or repeated one can be told from the text alone."""
    out = []
    for i in range(1, n + 1):
        out.append("[%s:%04d] lorem ipsum dolor sit amet.%s" % (tag, i, "\n\n" if i % 8 == 0 else " "))
    out.append("[%s:END]" % tag)
    return out


def history_check(messages, tag):
    """pure: what the app sent back for a follow-up in tab `tag`. -> {tag, chunks_expected, chunks_seen, end, foreign, assistant_chars, dup, user_text}
    chunks_seen counts DISTINCT numbered markers of the own tag in assistant messages; foreign lists markers of other tags anywhere in the conversation."""
    messages = clean_messages(messages)
    asst = "\n".join((m.get("content") or "") for m in messages if m.get("role") == "assistant")
    allt = "\n".join((m.get("content") or "") for m in messages if m.get("role") in ("assistant", "user", "system", "tool"))
    exp = 0
    for m in messages:
        mm = STREAM_RE.search(m.get("content") or "") if m.get("role") == "user" else None
        if mm and mm.group(1) == tag:
            exp = int(mm.group(2))
    own = [n for t, n in MARK_RE.findall(asst) if t == tag]
    nums = [n for n in own if n != "END"]
    return {"tag": tag, "chunks_expected": exp, "chunks_seen": len(set(nums)), "end": "END" in own, "dup": len(nums) - len(set(nums)),
            "foreign": sorted({t for t, _ in MARK_RE.findall(allt) if t != tag}), "assistant_chars": len(asst),
            "user_text": "\n".join((m.get("content") or "") for m in messages if m.get("role") == "user")}


def check_verdict(c):
    """pure: 'ok' | 'empty' (the app sent no assistant text back: nothing to judge) | a ';'-joined list of what is wrong"""
    if not c["assistant_chars"]:
        return "empty"
    bad = []
    if c["chunks_expected"] and c["chunks_seen"] != c["chunks_expected"]:
        bad.append("chunks %d/%d" % (c["chunks_seen"], c["chunks_expected"]))
    if not c["end"]:
        bad.append("no END")
    if c["dup"]:
        bad.append("dup %d" % c["dup"])
    if c["foreign"]:
        bad.append("foreign " + ",".join(c["foreign"]))
    return ";".join(bad) or "ok"


def mcp_function(tools, server, tool):
    """pure: the offered function name for MCP `tool` of `server` (mcp__<server>__<tool>, the app's naming), or None"""
    for t in tools or []:
        n = (t.get("function") or {}).get("name") or ""
        if n.startswith("mcp__%s__" % server) and n.endswith("__%s" % tool):
            return n
    return None


def stream_text():
    """pure: the text of the slow steady stream, 't0000 t0001 ...' cut into 6-character pieces by stream_lines"""
    return "".join("t%04d " % i for i in range(STREAM_CHUNKS))


ENV_RE = re.compile(r"<environment_context>.*?</environment_context>\s*", re.S)      # user_context.rs render_environment_context: its own user message after the prompt


# #2045: the app puts what the user attached in FRONT of the prompt, in the same user message (chat_stream.rs: "{prefix}\n\n{query}"; user_context.rs
# collect_user_attachments / render_referenced_attachments, each one `<attached_context>\n ... </attached_context>`). The blocks run since the last query are
# attached by the app itself (auto_attached="true"), with their command and OUTPUT. The text inside is XML-escaped (xml_text: & < >), so the first
# literal `</attached_context>` is the block's real end.
ATT_RE = re.compile(r"\A(?:<attached_context>\n.*?</attached_context>(?:\n\n|\Z))+", re.S)


def prompt_text(content):
    """pure: a user message's text without the attached context the app put in front of it: what the USER typed. Triggers and fault matches read this,
    never the attachment: round 11 lab, a nu arm's prompt had been typed into the remote shell, whose error message quotes the line, and the next arm's
    request carried that block in front of its own prompt; PLAN_RE (tried first) matched the EARLIER case's "carry out these steps in order: ..." there
    and the model answered an edit request with another case's plan (lab_fam_agent_edit_conflict.bash, lab_agent_remote_drop.bash). clean_messages
    keeps the attachment: the request log, fake.fresh and history_check must still show everything the model was sent (#1829)."""
    return ATT_RE.sub("", content) if isinstance(content, str) and content.startswith("<attached_context>") else (content or "")


def clean_messages(messages):
    """pure: the request's messages as the PROMPT sees them: the app's <environment_context> block is removed (a user message that holds nothing else is dropped;
    a block inside a longer message is cut out). Every trigger / token match goes through this, never through the raw list."""
    out = []
    for m in messages or []:
        if m.get("role") == "user" and isinstance(m.get("content"), str) and "<environment_context>" in m["content"]:
            rest = ENV_RE.sub("", m["content"]).strip()
            if not rest:
                continue
            m = dict(m, content=rest)
        out.append(m)
    return out


def user_prompt(messages):
    """pure: the text of the last user message that is not the environment-context message ('' when none), without its attached context (prompt_text)"""
    for m in reversed(clean_messages(messages)):
        if m.get("role") == "user":
            return prompt_text(m.get("content"))
    return ""


# Every request the app can make through the Ollama-protocol provider this fake stands in for (round 6; app/src/ai/agent_providers/):
#   purpose            endpoint     recognised by                                              answer (deterministic)
#   chat               /api/chat    the request offers tools (the agent turn), or has no       the scripted reply (triggers, faults, plans)
#                                   system message (a harness-direct request)
#   title              /api/chat    system prompts/tasks/title_system.md "thread title generator"          "Fake conversation title"
#   commit_message     /api/chat    tasks/commit_message_system.md "git commit message generator"          subject + body
#   explain_command    /api/chat    tasks/explain_command_system.md "You explain shell commands"           RISK: SAFE / RISKS / EXPLANATION
#   compaction         /api/chat    tasks/compaction_system.j2 "anchored context summarization"           the ## Goal ... sections
#   next_command       /api/chat    active_ai/next_command_system.j2 "predict the most likely next shell command"   "echo fake-next"
#   nld_predict        /api/chat    active_ai/nld_predict_system.j2 "complete a partial natural-language query"    "fake completion"
#   prompt_suggestions /api/chat    active_ai/prompt_suggestions_system.j2 "Predict the single most useful follow-up"   {"kind":"none"}
#   relevant_files     /api/chat    active_ai/relevant_files_system.j2 "return ONLY the subset of paths"      {"paths":[]}
#   workflow_metadata  /api/chat    active_ai/workflow_metadata_system.j2 "reusable, parameterized workflow"  the JSON object (no arguments)
#   show / tags        /api/show, /api/tags (model metadata / model list)                                       fixed JSON
#   UNKNOWN_PURPOSE    anything else: /api/generate, /api/embed, /api/embeddings, or a /api/chat with a system message and no tools that matches none of the
#                      above. NOT answered as chat: a 501 with {"error": "UNKNOWN_PURPOSE ..."}, counted in fake.by_purpose and listed in fake.unknown
# The app uses no embeddings and no /api/generate on this path (grepped: none). Only `chat` takes case triggers and faults by default; inject(..., purpose=P)
# aims a fault at another purpose. `requests` counts chat requests only; every purpose is counted in fake.by_purpose.
PURPOSE_MARKERS = (
    ("title", "thread title generator"),
    ("commit_message", "git commit message generator"),
    ("explain_command", "You explain shell commands"),
    ("compaction", "anchored context summarization"),
    ("next_command", "predict the most likely next shell command"),
    ("nld_predict", "complete a partial natural-language query"),
    ("prompt_suggestions", "Predict the single most useful follow-up"),
    ("relevant_files", "return ONLY the subset of paths most relevant"),
    ("workflow_metadata", "reusable, parameterized workflow"),
)
PURPOSE_TEXT = {
    "title": "Fake conversation title",
    "commit_message": "Fix the fake thing\n\nDeterministic fake commit message.",
    "explain_command": "RISK: SAFE\nRISKS:\n- None\nEXPLANATION:\n- `fake` does nothing.",
    "compaction": "## Goal\n- fake goal\n\n## Instructions\n- none\n\n## Discoveries\n- none\n\n## Accomplished\n- 🔲 nothing\n\n## Relevant Files\n- none\n\n## Open Questions / Pending\n- none",
    "next_command": "echo fake-next",
    "nld_predict": "fake completion",
    "prompt_suggestions": '{"kind":"none"}',
    "relevant_files": '{"paths":[]}',
    "workflow_metadata": json.dumps({"title": "Fake workflow", "description": "A deterministic fake workflow.", "command": "echo fake", "arguments": []}),
}


def classify_purpose(path, req):
    """pure: the purpose of one request (see the table above). `path` is the URL path, `req` the decoded JSON body."""
    p = (path or "").split("?")[0]
    if p.endswith("/api/show"):
        return "show"
    if p.endswith("/api/tags"):
        return "tags"
    if not p.endswith("/api/chat"):
        return "UNKNOWN_PURPOSE"
    msgs = (req or {}).get("messages") or []
    sysm = "\n".join((m.get("content") or "") for m in msgs if m.get("role") == "system" and isinstance(m.get("content"), str))
    for name, marker in PURPOSE_MARKERS:
        if marker in sysm:
            return name
    if (req or {}).get("tools") or not sysm:
        return "chat"
    return "UNKNOWN_PURPOSE"


TITLE_TAG_RE = re.compile(r"conversation tag (\w+)")


def purpose_reply(purpose, messages=None):
    """pure: the scripted answer of a non-chat purpose, shaped like reply_for's. A title request whose user text names a "conversation tag <TAG>"
    gets the title "Tagged <TAG>", so a case can find its own conversation in the app's conversation list; every other title is the fixed one."""
    if purpose == "title" and messages:
        m = TITLE_TAG_RE.search("\n".join(c for c in user_texts(messages) if isinstance(c, str)))
        if m:
            return {"text": "Tagged %s" % m.group(1), "tool": None}
    return {"text": PURPOSE_TEXT[purpose], "tool": None}


def is_title_request(messages):
    """pure: the app's conversation-title request"""
    return classify_purpose("/api/chat", {"messages": messages}) == "title"


def one_shot_body(model, reply):
    """pure: the single JSON object of a NON-streaming /api/chat answer (request "stream": false: titles, suggestions, commit messages)"""
    return json.dumps({"model": model, "message": {"role": "assistant", "content": reply["text"]}, "done": True, "done_reason": "stop"})


def user_texts(messages):
    """pure: the content of every user message of a /api/chat request, in order (environment context removed)"""
    return [m.get("content") or "" for m in clean_messages(messages) if m.get("role") == "user"]


def drip_text(words):
    """pure: the long reply a `drip` fault streams (distinct words, so a cut or a repeat is visible in the text)"""
    return " ".join("w%d" % i for i in range(int(words)))


def status_body(code):
    """pure: the JSON error body of a `status` fault"""
    return json.dumps({"error": "fake model fault: HTTP %d" % int(code)}).encode()


def parse_plan(text):
    """pure: the steps of a "carry out these steps in order: a ;; b" turn -> [(kind, args...)], or None when `text` is not a plan; an unparseable step is ("bad", step)"""
    m = PLAN_RE.search(text or "")
    if not m:
        return None
    out = []
    for raw in re.split(r"\s;;\s", m.group(1)):
        raw = raw.strip()
        kind, _, rest = raw.partition(" ")
        rest = rest.strip()
        if kind in ("read", "codebase", "run") and rest:
            out.append((kind, rest))
        elif kind in ("grep", "glob") and len(rest.split()) == 2:
            out.append((kind,) + tuple(rest.split()))
        elif kind == "edit" and len(rest.split(" :: ")) == 3:
            out.append((kind,) + tuple(x.strip() for x in rest.split(" :: ")))
        else:
            out.append(("bad", raw))
    return out


def ask_question_call(tag, args=None):
    """pure: the ask_user_question tool call of a "Please ask me a question <TAG>" turn (args: a replacement `arguments`, e.g. a truncated string or {})"""
    if args is None:
        args = {"questions": [{"question": "Which one do you want? %s" % (tag or "Q"), "options": ["alpha", "beta"], "recommended_index": 0, "multi_select": False, "supports_other": False}]}
    return {"name": "ask_user_question", "arguments": args}


def ask_fault_lines(model, kind):
    """pure: ([NDJSON lines before the fault's own behaviour], [lines after]) for the ask_* faults: text line, call line (truncated / empty / whole), done line"""
    def line(msg, done=False, **kw):
        d = {"model": model, "message": dict({"role": "assistant", "content": ""}, **msg), "done": done}
        d.update(kw)
        return json.dumps(d)
    text = line({"content": "Let me ask you something. "})
    done = line({}, True, done_reason="stop")
    if kind == "ask_trunc":
        call = line({"tool_calls": [{"function": ask_question_call("T", '{"questions":[{"question":"Which one do')}]})
    elif kind == "ask_empty":
        call = line({"tool_calls": [{"function": ask_question_call("E", {})}]})
    else:
        call = line({"tool_calls": [{"function": ask_question_call("F")}]})
    return text, call, done


def plan_call(step):
    """pure: the tool call of one parsed plan step"""
    k = step[0]
    if k == "read":
        return {"name": "read_files", "arguments": {"files": [{"path": step[1]}]}}
    if k == "grep":
        return {"name": "grep", "arguments": {"queries": [step[1]], "path": step[2]}}
    if k == "glob":
        return {"name": "file_glob", "arguments": {"patterns": [step[1]], "search_dir": step[2]}}
    if k == "edit":
        return {"name": "apply_file_diffs", "arguments": {"summary": "Edit " + step[1], "operations": [{"op": "edit", "file_path": step[1], "search": step[2], "replace": step[3]}]}}
    if k == "run":
        return {"name": "run_shell_command", "arguments": {"command": step[1], "is_read_only": False, "uses_pager": False, "is_risky": False}}
    if k == "codebase":
        return {"name": "search_codebase", "arguments": {"query": step[1]}}
    return None


def turn_state(messages):
    """pure: (text of the last user turn without its attached context (prompt_text), [content of the tool messages after it])"""
    text, results = "", []
    for m in reversed(clean_messages(messages)):
        if m.get("role") == "user":
            text = prompt_text(m.get("content"))
            break
        if m.get("role") == "tool":
            results.append(m.get("content") or "")
    results.reverse()
    return text, results


def reply_for(messages, tools=None):
    """pure: the scripted reply to an Ollama /api/chat `messages` list -> {"text": str, "tool": None | {"name", "arguments"}}"""
    messages = clean_messages(messages)
    last = messages[-1] if messages else {}
    utext, results = turn_state(messages)
    plan = parse_plan(utext)
    if plan is not None:
        if len(results) >= len(plan):
            return {"text": "Done.", "tool": None}
        call = plan_call(plan[len(results)])
        if call is None:
            return {"text": "bad step: %s" % plan[len(results)][1], "tool": None}
        return {"text": "", "tool": call}
    if last.get("role") == "tool":
        return {"text": "Done.", "tool": None}
    text = user_prompt(messages)
    m = STREAM_RE.search(text)
    if m:
        return {"text": "", "tool": None, "chunks": stream_chunks(m.group(1), int(m.group(2))), "gap_ms": int(m.group(3)), "tag": m.group(1)}
    m = FOLLOW_RE.search(text)
    if m:
        c = history_check(messages, m.group(1))
        return {"text": "CHECKED %s %s" % (m.group(1), check_verdict(c)), "tool": None, "check": c}
    m = MCP_RE.search(text)
    if m:
        fn = mcp_function(tools, m.group(2), m.group(1))
        if fn is None:
            return {"text": "NO-MCP-TOOL %s/%s" % (m.group(2), m.group(1)), "tool": None, "mcp_missing": (m.group(2), m.group(1))}
        try:
            args = json.loads(m.group(3)) if m.group(3) else {}
        except ValueError:
            args = {}
        return {"text": "", "tool": {"name": fn, "arguments": args}, "mcp": (m.group(2), m.group(1))}
    m = IMG_RE.search(text)
    if m:                                      # a markdown image: agent output never loads an http(s) image, so the app must request nothing for it (agent_gate_deny_allow step 0)
        return {"text": "Here it is:\n\n![img](%s)" % m.group(1), "tool": None}
    m = ASKCMD_RE.search(text)
    if m:                                      # #1858: the ask trails a running command; the card must not say "Preparing question..." meanwhile
        return {"text": "", "tool": {"name": "run_shell_command", "arguments": {"command": "sleep 8", "is_read_only": False, "uses_pager": False, "is_risky": False}},
                "tools_after": [ask_question_call(m.group(1))], "ask_question": True}
    m = ASKQ_RE.search(text)
    if m:
        return {"text": "", "tool": ask_question_call(m.group(1)), "ask_question": True}
    m = SCRIPT_RE.search(text)
    if m:
        cmd = SCRIPTED_COMMANDS.get(m.group(1))
        if cmd is None:
            return {"text": "NO-SCRIPTED-COMMAND %s" % m.group(1), "tool": None}
        return {"text": "", "tool": {"name": "run_shell_command", "arguments": {"command": cmd, "is_read_only": False, "uses_pager": False, "is_risky": False}}}
    m = ASK_RE.search(text)
    if m:
        return {"text": "", "tool": {"name": "run_shell_command", "arguments": {"command": m.group(1), "is_read_only": False, "uses_pager": False, "is_risky": False}}}
    m = READ_RE.search(text)
    if m:
        return {"text": "", "tool": {"name": "read_files", "arguments": {"files": [{"path": m.group(1)}]}}}
    m = CREATE_RE.search(text)
    if m:
        return {"text": "", "tool": {"name": "apply_file_diffs", "arguments": {"summary": "Create " + m.group(1), "operations": [{"op": "create", "file_path": m.group(1), "content": m.group(2)}]}}}
    m = DELETE_RE.search(text)
    if m:
        return {"text": "", "tool": {"name": "apply_file_diffs", "arguments": {"summary": "Delete " + m.group(1), "operations": [{"op": "delete", "file_path": m.group(1)}]}}}
    m = EDIT_RE.search(text)
    if m:
        ops = []
        for pair in m.group(2).split(" ;; "):
            old, _, new = pair.partition("=>")
            ops.append({"op": "edit", "file_path": m.group(1), "search": old.replace("\\n", "\n"), "replace": new.replace("\\n", "\n")})
        return {"text": "", "tool": {"name": "apply_file_diffs", "arguments": {"summary": "Edit " + m.group(1), "operations": ops}}}
    if any("STREAMLONG" in prompt_text(t) for t in user_texts(messages)):
        return {"text": stream_text(), "tool": None, "stream": True}
    if "SLOW" in text:
        return {"text": "slow reply", "tool": None, "slow": True}
    words = re.findall(r"\S+", text)
    return {"text": "OK %s" % (words[-1] if words else ""), "tool": None}


def request_record(messages):
    """pure: what one /api/chat request carried, for the request log: the role of the last message, the content of the last tool message (None when the
    request has none) and every user message's content, in order"""
    msgs = clean_messages(messages)
    tool = None
    for m in reversed(msgs):
        if m.get("role") == "tool":
            tool = m.get("content") if isinstance(m.get("content"), str) else json.dumps(m.get("content"))
            break
    return {"last_role": (msgs[-1].get("role") if msgs else None), "tool": tool,
            "users": [m.get("content") or "" for m in msgs if m.get("role") == "user"]}


def read_request_log(path, skip=0):
    """the request-log rows (dicts) of `path` after the first `skip` rows; [] when the file is missing"""
    try:
        rows = [json.loads(l) for l in open(path, errors="replace").read().splitlines() if l.strip()]
    except (OSError, ValueError):
        return []
    return rows[skip:]


def stream_lines(model, reply, chunk=6):
    """pure: the NDJSON lines (str) of one streamed answer: the text in `chunk`-character pieces, the tool call (if any), then the done line"""
    out = []
    t = reply["text"]
    for piece in (reply.get("chunks") or [t[i:i + chunk] for i in range(0, len(t), chunk)]):
        out.append(json.dumps({"model": model, "message": {"role": "assistant", "content": piece}, "done": False}))
    if reply["tool"]:
        calls = [{"function": reply["tool"]}] + [{"function": t} for t in reply.get("tools_after") or []]
        out.append(json.dumps({"model": model, "message": {"role": "assistant", "content": "", "tool_calls": calls}, "done": False}))
    out.append(json.dumps({"model": model, "message": {"role": "assistant", "content": ""}, "done": True, "done_reason": "stop"}))
    return out


# ---------------------------------------------------------------- the Anthropic Messages wire (pure parts)
def is_anthropic_path(path):
    """pure: is this URL path the Anthropic Messages endpoint (<base>/v1/messages; the app's genai adapter posts `{base_url}messages`)"""
    return (path or "").split("?")[0].rstrip("/").endswith("/messages")


def _ant_text(content):
    """pure: the text of an Anthropic `content` value: a string, or the text blocks of a block list joined; anything else as JSON"""
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        return "\n".join((b.get("text") or "") if isinstance(b, dict) and b.get("type") == "text" else json.dumps(b) for b in content)
    return "" if content is None else json.dumps(content)


def anthropic_to_messages(req):
    """pure: the messages of an Anthropic Messages request body as the role/content list reply_for and every oracle read: `system` (a string or text
    blocks) -> one system message; a user turn's text blocks -> one user message (joined by a blank line, as the app joins attachment and prompt);
    each tool_result block -> its own tool message, in order, BEFORE the text of the same turn (Anthropic puts the results first); an assistant turn's
    text -> its content and its tool_use blocks -> tool_calls."""
    out = []
    sysm = (req or {}).get("system")
    stext = sysm if isinstance(sysm, str) else "\n\n".join((b.get("text") or "") for b in (sysm or []) if isinstance(b, dict))
    if stext:
        out.append({"role": "system", "content": stext})
    for m in (req or {}).get("messages") or []:
        role, c = m.get("role"), m.get("content")
        if isinstance(c, str):
            out.append({"role": role, "content": c})
            continue
        texts, calls = [], []
        for b in c or []:
            if not isinstance(b, dict):
                continue
            if b.get("type") == "text":
                texts.append(b.get("text") or "")
            elif b.get("type") == "tool_result":
                out.append({"role": "tool", "content": _ant_text(b.get("content")), "tool_use_id": b.get("tool_use_id")})
            elif b.get("type") == "tool_use":
                calls.append({"id": b.get("id"), "function": {"name": b.get("name"), "arguments": b.get("input")}})
        if role == "assistant":
            msg = {"role": "assistant", "content": "".join(texts)}
            if calls:
                msg["tool_calls"] = calls
            out.append(msg)
        elif texts:
            out.append({"role": role or "user", "content": "\n\n".join(texts)})
    return out


def anthropic_request(req):
    """pure: an Anthropic request body as the Ollama-shaped request the handler works on ({model, messages, tools, stream})"""
    return {"model": (req or {}).get("model"), "messages": anthropic_to_messages(req), "stream": (req or {}).get("stream"),
            "tools": [{"function": {"name": t.get("name")}} for t in (req or {}).get("tools") or [] if isinstance(t, dict)]}


def _reply_calls(reply):
    return ([reply["tool"]] if reply.get("tool") else []) + list(reply.get("tools_after") or [])


def anthropic_events(model, reply, chunk=6, call_ids=None):
    """pure: [(event name, data dict)] of one streamed Anthropic answer: message_start; the text as one text block (content_block_start, one
    text_delta per `chunk` characters or per reply chunk, content_block_stop); each tool call as one tool_use block whose input arrives as TWO
    input_json_delta pieces (the client must join them); message_delta with stop_reason tool_use / end_turn; message_stop.
    call_ids: the id of each tool_use block (the server passes unique ones; default toolu_fake_<n>)."""
    ev = [("message_start", {"type": "message_start", "message": {"id": "msg_fake", "type": "message", "role": "assistant", "model": model, "content": [],
                                                                  "stop_reason": None, "stop_sequence": None, "usage": {"input_tokens": 1, "output_tokens": 0}}})]
    i = 0
    t = reply.get("text") or ""
    pieces = reply.get("chunks") or [t[k:k + chunk] for k in range(0, len(t), chunk)]
    if pieces:
        ev.append(("content_block_start", {"type": "content_block_start", "index": i, "content_block": {"type": "text", "text": ""}}))
        for p in pieces:
            ev.append(("content_block_delta", {"type": "content_block_delta", "index": i, "delta": {"type": "text_delta", "text": p}}))
        ev.append(("content_block_stop", {"type": "content_block_stop", "index": i}))
        i += 1
    calls = _reply_calls(reply)
    for n, c in enumerate(calls):
        js = c["arguments"] if isinstance(c["arguments"], str) else json.dumps(c["arguments"])
        cid = call_ids[n] if call_ids else "toolu_fake_%d" % n
        ev.append(("content_block_start", {"type": "content_block_start", "index": i, "content_block": {"type": "tool_use", "id": cid, "name": c["name"], "input": {}}}))
        for part in (js[:len(js) // 2], js[len(js) // 2:]):
            if part:
                ev.append(("content_block_delta", {"type": "content_block_delta", "index": i, "delta": {"type": "input_json_delta", "partial_json": part}}))
        ev.append(("content_block_stop", {"type": "content_block_stop", "index": i}))
        i += 1
    ev.append(("message_delta", {"type": "message_delta", "delta": {"stop_reason": "tool_use" if calls else "end_turn", "stop_sequence": None}, "usage": {"output_tokens": 1}}))
    ev.append(("message_stop", {"type": "message_stop"}))
    return ev


def sse(events):
    """pure: the text/event-stream bytes of [(event, data)]"""
    return "".join("event: %s\ndata: %s\n\n" % (name, json.dumps(data)) for name, data in events).encode()


def anthropic_message_body(model, reply, call_ids=None):
    """pure: the single JSON object of a NON-streaming Anthropic answer ("stream": false: titles and the other one-shot purposes)"""
    content = [{"type": "text", "text": reply.get("text") or ""}] if (reply.get("text") or not _reply_calls(reply)) else []
    for n, c in enumerate(_reply_calls(reply)):
        content.append({"type": "tool_use", "id": call_ids[n] if call_ids else "toolu_fake_%d" % n, "name": c["name"],
                        "input": c["arguments"] if isinstance(c["arguments"], dict) else {}})
    return json.dumps({"id": "msg_fake", "type": "message", "role": "assistant", "model": model, "content": content,
                       "stop_reason": "tool_use" if _reply_calls(reply) else "end_turn", "stop_sequence": None, "usage": {"input_tokens": 1, "output_tokens": 1}})


def anthropic_error_body(code):
    """pure: the JSON error object of a `status` fault on the Anthropic wire"""
    kind = {429: "rate_limit_error", 401: "authentication_error", 529: "overloaded_error"}.get(int(code), "api_error")
    return json.dumps({"type": "error", "error": {"type": kind, "message": "fake model fault: HTTP %d" % int(code)}}).encode()


ANTHROPIC_FAULTS = ("status", "disconnect", "delay")        # the fault kinds that have an Anthropic-wire form


def seen_text(messages):
    """pure: everything a request's messages carry as one text: each message's content (a non-string as JSON) and its tool-call arguments. The oracle
    for "did this token reach this provider in ANY request" (fake.seen / seen_with)."""
    out = []
    for m in messages or []:
        c = m.get("content")
        out.append(c if isinstance(c, str) else json.dumps(c))
        if m.get("tool_calls"):
            out.append(json.dumps(m["tool_calls"]))
    return "\n".join(out)


class FakeModel:
    ACTIVE = None                                   # the instance run.py started (same object as the module global ACTIVE)

    def __init__(self, delay_s=0.0):
        self.requests, self.delay_s, self.log, self.turns = 0, delay_s, [], []
        self.reqs = []          # #1828: one {"t": epoch s, "messages": [...]} per /api/chat request, in arrival order (the steer cases' oracle)
        self.fresh, self.fired, self.aborted, self.faults = [], [], 0, []        # #1831
        self.by_purpose, self.unknown = {}, []     # per-purpose request counts; UNKNOWN_PURPOSE records ({"path", "model", "system_head"})
        self.title_requests = 0     # = by_purpose["title"]
        self._lock = threading.Lock()
        self.streams, self.followups, self.tool_results, self.tools_seen, self.mcp_missing, self.user_turns = {}, {}, [], set(), [], []   # #1833/#1834
        self.lock = threading.Lock()
        self.headers = []        # lower-cased header dict of every request, for the provider-key oracle (a provider's extra_headers sentinel must arrive here)
        self.seen = []           # one {t, wire, purpose, text} per POST of either wire and every purpose (seen_text): see seen_with
        self.call_seq = 0        # tool_use ids of the Anthropic wire are unique per reply
        self.by_wire = {"ollama": 0, "anthropic": 0}      # CHAT requests per wire: agent_anthropic_wire proves its prompts came on the Anthropic one
        self.log_path = os.environ.get("APPCI_FAKE_MODEL_LOG") or None
        owner = self

        class H(BaseHTTPRequestHandler):
            def log_message(self, *a):
                pass

            def _send(self, code, body, ctype="application/json"):
                self.send_response(code)
                self.send_header("Content-Type", ctype)
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def _wait_or_hangup(self, secs):
                """sleep `secs`, polling the socket: False (and fake.aborted += 1) when the client hung up meanwhile"""
                end = time.time() + secs
                while time.time() < end:
                    try:
                        rd, _, _ = select.select([self.connection], [], [], min(0.1, max(0.0, end - time.time())))
                        if rd and self.connection.recv(1, socket.MSG_PEEK) == b"":
                            owner.aborted += 1
                            return False
                    except OSError:
                        owner.aborted += 1
                        return False
                return True

            def do_GET(self):
                self._send(200, json.dumps({"models": [{"name": "fake-model", "model": "fake-model"}]}).encode())

            def do_POST(self):
                owner.headers.append({k.lower(): v for k, v in self.headers.items()})
                n = int(self.headers.get("Content-Length") or 0)
                try:
                    req = json.loads(self.rfile.read(n) or b"{}")
                except ValueError:
                    return self._send(400, b'{"error":"bad json"}')
                wire = "anthropic" if is_anthropic_path(self.path) else "ollama"
                if wire == "anthropic":           # the same triggers, faults and oracles: the request is read into the shape they all use
                    req = anthropic_request(req)
                purpose = classify_purpose("/api/chat" if wire == "anthropic" else self.path, req)
                with owner.lock:
                    owner.by_purpose[purpose] = owner.by_purpose.get(purpose, 0) + 1
                    owner.seen.append({"t": time.time(), "wire": wire, "purpose": purpose, "text": seen_text(req.get("messages"))})
                    if purpose == "chat":
                        owner.by_wire[wire] += 1
                if purpose == "show":
                    return self._send(200, json.dumps({"model_info": {"general.context_length": 32768}, "capabilities": ["completion", "tools"]}).encode())
                if purpose == "UNKNOWN_PURPOSE":
                    sysm = next((m.get("content") or "" for m in req.get("messages") or [] if m.get("role") == "system"), "")
                    owner.unknown.append({"path": self.path, "model": req.get("model"), "system_head": str(sysm)[:80]})
                    sys.stderr.write("fake_model: UNKNOWN_PURPOSE %s model=%s system=%r\n" % (self.path, req.get("model"), str(sysm)[:80]))
                    return self._send(501, json.dumps({"error": "UNKNOWN_PURPOSE: the fake model does not answer %s" % self.path}).encode())
                title = purpose == "title"
                if purpose == "chat":
                    owner.requests += 1
                else:
                    owner.title_requests = owner.by_purpose.get("title", 0)
                if owner.log_path and purpose == "chat":
                    try:
                        rec = dict(request_record(req.get("messages") or []), t=time.time(), n=owner.requests)
                        with open(owner.log_path, "a") as f:
                            f.write(json.dumps(rec) + "\n")
                    except OSError:
                        pass
                msgs = clean_messages(req.get("messages") or [])
                if purpose == "chat":
                    owner.turns.append((time.time(),) + turn_state(msgs))
                    owner.reqs.append({"t": time.time(), "messages": msgs})
                    if msgs and msgs[-1].get("role") == "user":
                        owner.fresh.append(msgs[-1].get("content") or "")
                with owner.lock:
                    owner.tools_seen |= {(t.get("function") or {}).get("name") for t in req.get("tools") or []}
                    if purpose != "chat":
                        pass
                    elif msgs and msgs[-1].get("role") == "tool":
                        owner.tool_results.append((time.time(), msgs[-1].get("content") or ""))
                    elif msgs:
                        owner.user_turns.append((time.time(), msgs[-1].get("content") or ""))
                fault = owner._take(user_prompt(msgs), purpose)
                if wire == "anthropic":
                    return self._anthropic(req, purpose, fault, msgs)
                if purpose != "chat" and not fault:
                    reply = purpose_reply(purpose, msgs)
                    if req.get("stream") is False:
                        return self._send(200, one_shot_body(req.get("model") or "fake-model", reply).encode())
                    return self._send(200, ("\n".join(stream_lines(req.get("model") or "fake-model", reply)) + "\n").encode(), "application/x-ndjson")
                if fault:
                    owner.fired.append(fault["kind"])
                    kw = fault["kw"]
                    if fault["kind"] == "status":
                        self.send_response(int(kw.get("code", 500)))
                        if kw.get("retry_after") is not None:
                            self.send_header("Retry-After", str(kw["retry_after"]))
                        body = status_body(kw.get("code", 500))
                        self.send_header("Content-Type", "application/json")
                        self.send_header("Content-Length", str(len(body)))
                        self.end_headers()
                        self.wfile.write(body)
                        return
                    if fault["kind"] == "disconnect":
                        lines = stream_lines(req.get("model") or "fake-model", {"text": drip_text(40), "tool": None})
                        self.send_response(200)
                        self.send_header("Content-Type", "application/x-ndjson")
                        self.send_header("Content-Length", str(100000))        # promises more than is sent: the cut is a protocol error, not a clean end
                        self.end_headers()
                        for ln in lines[:int(kw.get("after", 2))]:
                            self.wfile.write((ln + "\n").encode())
                        self.wfile.flush()
                        self.close_connection = True
                        try:
                            self.connection.shutdown(socket.SHUT_RDWR)
                        except OSError:
                            pass
                        return
                    if fault["kind"].startswith("ask_"):                      # #1851: the ask_user_question call under stream faults
                        text_l, call_l, done_l = ask_fault_lines(req.get("model") or "fake-model", fault["kind"])
                        kind = fault["kind"]
                        self.send_response(200)
                        self.send_header("Content-Type", "application/x-ndjson")
                        if kind == "ask_cut":
                            self.send_header("Content-Length", str(100000))     # promises more than is sent: the cut is a protocol error, not a clean end
                        self.end_headers()                                      # else HTTP/1.0: the body ends when the connection closes
                        try:
                            self.wfile.write((text_l + "\n").encode())
                            self.wfile.flush()
                            if kind == "ask_slow" and not self._wait_or_hangup(float(kw.get("secs", 3))):
                                return
                            self.wfile.write((call_l + "\n").encode())
                            self.wfile.flush()
                            if kind == "ask_cut":
                                self.close_connection = True
                                try:
                                    self.connection.shutdown(socket.SHUT_RDWR)
                                except OSError:
                                    pass
                                return
                            if kind == "ask_nodone":
                                self._wait_or_hangup(float(kw.get("secs", 60)))
                                return
                            self.wfile.write((done_l + "\n").encode())
                            self.wfile.flush()
                        except OSError:
                            owner.aborted += 1
                        return
                    if fault["kind"] == "delay":
                        if not self._wait_or_hangup(float(kw.get("secs", 30))):
                            return
                    if fault["kind"] == "drip":
                        lines = stream_lines(req.get("model") or "fake-model", {"text": drip_text(kw.get("words", 60)), "tool": None})
                        self.send_response(200)
                        self.send_header("Content-Type", "application/x-ndjson")
                        self.end_headers()                                      # HTTP/1.0: the body ends when the connection closes
                        try:
                            for ln in lines:
                                self.wfile.write((ln + "\n").encode())
                                self.wfile.flush()
                                if not self._wait_or_hangup(float(kw.get("interval", 0.5))):
                                    return
                        except OSError:
                            owner.aborted += 1
                        return
                if purpose != "chat":                                    # a non-chat purpose whose fault was a `delay`: then the normal purpose answer
                    reply = purpose_reply(purpose, msgs)
                    if req.get("stream") is False:
                        return self._send(200, one_shot_body(req.get("model") or "fake-model", reply).encode())
                    return self._send(200, ("\n".join(stream_lines(req.get("model") or "fake-model", reply)) + "\n").encode(), "application/x-ndjson")
                reply = reply_for(msgs, req.get("tools"))
                if reply.get("mcp_missing"):
                    owner.mcp_missing.append(reply["mcp_missing"])
                if reply.get("check"):
                    owner.followups[reply["check"]["tag"]] = dict(reply["check"], verdict=check_verdict(reply["check"]), t=time.time())
                owner.log.append((time.time(), (reply["tool"]["arguments"].get("command") if reply["tool"] else None)))
                if reply.get("chunks") or reply.get("stream"):
                    return self._stream(req.get("model") or "fake-model", reply)
                body = ("\n".join(stream_lines(req.get("model") or "fake-model", reply)) + "\n").encode()
                if reply.get("slow") or owner.delay_s:
                    time.sleep(3.0 if reply.get("slow") else owner.delay_s)
                self._send(200, body, "application/x-ndjson")

        def _stream(self, model, reply):
            """a long reply on an open, close-delimited stream, each NDJSON line flushed as it is produced: one text piece per gap (a #1833 reply: gap_ms,
            recorded in fake.streams[tag]; a STREAMLONG reply: STREAM_GAP_S). A client that goes away mid-reply (a steer, Ctrl-C) is not an error here."""
            lines = stream_lines(model, reply)
            n_text = len(reply["chunks"]) if reply.get("chunks") else -(-len(reply["text"]) // 6)
            gap = reply["gap_ms"] / 1000.0 if reply.get("chunks") else STREAM_GAP_S
            rec = dict(total=n_text, sent=0, finished=False, broken=False, t0=time.time(), t1=None)
            if reply.get("tag"):
                owner.streams[reply["tag"]] = rec
            self.send_response(200)
            self.send_header("Content-Type", "application/x-ndjson")
            self.send_header("Connection", "close")
            self.end_headers()
            self.close_connection = True
            try:
                for i, l in enumerate(lines):
                    self.wfile.write((l + "\n").encode())
                    self.wfile.flush()
                    if i < n_text:
                        rec["sent"] = i + 1
                        time.sleep(gap)
                rec["finished"] = True
            except (BrokenPipeError, ConnectionResetError, OSError):
                rec["broken"] = True
            rec["t1"] = time.time()

        def _anthropic(self, req, purpose, fault, msgs):
            """the answer on the Anthropic wire (the request is already recorded): a fault that has an Anthropic form, else the same scripted reply as
            the Ollama wire, as an event stream or (\"stream\": false) one message object"""
            model = req.get("model") or "fake-model"
            if fault:
                kind, kw = fault["kind"], fault["kw"]
                if kind not in ANTHROPIC_FAULTS:
                    owner.unknown.append({"path": self.path, "model": model, "system_head": "fault %s has no Anthropic-wire form" % kind})
                    sys.stderr.write("fake_model: fault %s has no Anthropic-wire form (%s)\n" % (kind, self.path))
                    return self._send(501, json.dumps({"type": "error", "error": {"type": "api_error", "message": "fake model: fault %s has no Anthropic-wire form" % kind}}).encode())
                owner.fired.append(kind)
                if kind == "status":
                    body = anthropic_error_body(kw.get("code", 500))
                    self.send_response(int(kw.get("code", 500)))
                    if kw.get("retry_after") is not None:
                        self.send_header("Retry-After", str(kw["retry_after"]))
                    self.send_header("Content-Type", "application/json")
                    self.send_header("Content-Length", str(len(body)))
                    self.end_headers()
                    self.wfile.write(body)
                    return
                if kind == "disconnect":                                  # message_start, the text block's start and `after` text deltas, then the cut
                    events = anthropic_events(model, {"text": drip_text(40), "tool": None})
                    self.send_response(200)
                    self.send_header("Content-Type", "text/event-stream")
                    self.send_header("Content-Length", str(100000))        # promises more than is sent: the cut is a protocol error, not a clean end
                    self.end_headers()
                    self.wfile.write(sse(events[:2 + int(kw.get("after", 2))]))
                    self.wfile.flush()
                    self.close_connection = True
                    try:
                        self.connection.shutdown(socket.SHUT_RDWR)
                    except OSError:
                        pass
                    return
                if kind == "delay" and not self._wait_or_hangup(float(kw.get("secs", 30))):
                    return
            reply = reply_for(msgs, req.get("tools")) if purpose == "chat" else purpose_reply(purpose, msgs)
            if purpose == "chat":
                if reply.get("mcp_missing"):
                    owner.mcp_missing.append(reply["mcp_missing"])
                if reply.get("check"):
                    owner.followups[reply["check"]["tag"]] = dict(reply["check"], verdict=check_verdict(reply["check"]), t=time.time())
                owner.log.append((time.time(), (reply["tool"]["arguments"].get("command") if reply["tool"] and isinstance(reply["tool"]["arguments"], dict) else None)))
            with owner.lock:
                owner.call_seq += 1
                base = owner.call_seq
            ids = ["toolu_fake_%d_%d" % (base, n) for n in range(len(_reply_calls(reply)))]       # unique per reply, as a real provider's are
            if req.get("stream") is False:
                return self._send(200, anthropic_message_body(model, reply, ids).encode())
            events = anthropic_events(model, reply, call_ids=ids)
            if not (reply.get("chunks") or reply.get("stream")):
                if reply.get("slow") or owner.delay_s:
                    time.sleep(3.0 if reply.get("slow") else owner.delay_s)
                return self._send(200, sse(events), "text/event-stream")
            # a long reply on an open, close-delimited stream, one text delta per gap (as _stream does for the Ollama wire)
            gap = reply["gap_ms"] / 1000.0 if reply.get("chunks") else STREAM_GAP_S
            rec = dict(total=sum(1 for n, d in events if n == "content_block_delta" and d["delta"]["type"] == "text_delta"), sent=0, finished=False, broken=False, t0=time.time(), t1=None)
            if reply.get("tag"):
                owner.streams[reply["tag"]] = rec
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Connection", "close")
            self.end_headers()
            self.close_connection = True
            try:
                for name, data in events:
                    self.wfile.write(sse([(name, data)]))
                    self.wfile.flush()
                    if name == "content_block_delta" and data["delta"]["type"] == "text_delta":
                        rec["sent"] += 1
                        time.sleep(gap)
                rec["finished"] = True
            except (BrokenPipeError, ConnectionResetError, OSError):
                rec["broken"] = True
            rec["t1"] = time.time()

        H._anthropic = _anthropic
        H._stream = _stream
        self._handler = H
        self.httpd = ThreadingHTTPServer(("127.0.0.1", 0), H)
        self.port = self.httpd.server_address[1]
        self.url = "http://127.0.0.1:%d" % self.port
        self.thread = threading.Thread(target=self.httpd.serve_forever, daemon=True)
        self.up = True

    def inject(self, kind, times=1, match=None, purpose="chat", **kw):
        assert kind in ("status", "disconnect", "delay", "drip", "ask_cut", "ask_nodone", "ask_trunc", "ask_empty", "ask_slow"), kind
        with self._lock:
            self.faults.append({"kind": kind, "left": times, "match": match, "purpose": purpose, "kw": kw})

    def clear_faults(self):
        with self._lock:
            self.faults = []

    def _take(self, user_text, purpose="chat"):
        with self._lock:
            for f in self.faults:
                if f.get("purpose", "chat") != purpose:
                    continue
                if f["match"] is not None and f["match"] not in user_text:
                    continue
                if f["left"] is not None:
                    f["left"] -= 1
                    if f["left"] <= 0:
                        self.faults.remove(f)
                return f
        return None

    def fresh_with(self, token):
        """how many prompts the app actually sent (a request whose last message is a user turn) that contain `token`"""
        return sum(1 for t in self.fresh if token in t)

    def seen_with(self, token, since=0, wire=None, purpose=None):
        """the fake.seen rows from index `since` on whose text holds `token` (optionally of one wire / purpose): every request of any purpose that
        carried it. `since` = len(fake.seen) taken before the step."""
        return [x for x in list(self.seen)[since:] if token in x["text"] and (wire is None or x["wire"] == wire) and (purpose is None or x["purpose"] == purpose)]

    def go_down(self):
        """connection refused: the listening socket is closed (a running answer is not touched)"""
        if self.up:
            self.httpd.shutdown()
            self.httpd.server_close()
            self.up = False

    def come_up(self):
        if not self.up:
            self.httpd = ThreadingHTTPServer(("127.0.0.1", self.port), self._handler)
            self.thread = threading.Thread(target=self.httpd.serve_forever, daemon=True)
            self.thread.start()
            self.up = True

    def tool_results_for(self, marker):
        """the tool-result contents the model received in the longest request whose user turn contains `marker` (a token unique to the case)"""
        best = []
        for _t, utext, results in list(self.turns):
            if marker in utext and len(results) >= len(best):
                best = results
        return best

    def start(self, active=True):
        """serve. active=False: a SECOND provider beside the run's own (agent_model_switch): ACTIVE keeps naming the run's fake model"""
        global ACTIVE
        if active:
            ACTIVE = self
            FakeModel.ACTIVE = self
        self.thread.start()
        return self

    def stop(self):
        global ACTIVE
        if self.up:
            self.httpd.shutdown()
            self.httpd.server_close()
            self.up = False
        if FakeModel.ACTIVE is self:
            FakeModel.ACTIVE = None
        if ACTIVE is self:
            ACTIVE = None
