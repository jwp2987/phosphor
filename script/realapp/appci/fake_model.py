# appci/fake_model.py: the L2 fake model (docs/testing.md "Agent fuzzer and fake model"): a tiny Ollama-protocol server on 127.0.0.1, stdlib only.
# run.py starts it when APPCI_FAKE_MODEL=1 and no real provider is configured, and profile.py then writes the usual Ollama provider block pointing at it
# (id appci-ollama, so every provider-dependent case sees the same provider name). It is deterministic: the reply is a pure function of the request.
#   * a user turn "Please run exactly this shell command and nothing else: <cmd>" -> one run_shell_command tool call with that command (an approval card);
#   * "Please read exactly this file and nothing else: <path>" -> read_files; "Please create exactly this file: <path> with this content: <text>" and
#     "Please delete exactly this file and nothing else: <path>" -> apply_file_diffs (the remote file-tool lab cases, cases_lab_remote.py);
#   * "Please show this image: <url>" -> the text "Here it is: ![img](<url>)" (network_gate_notice: the app fetches the image through the network gate);
#   * the turn after a tool result -> the text "Done." (a finished conversation);
#   * "Please stream long reply <TAG> chunks <N> gap <MS>" (#1833) -> N text chunks "[<TAG>:0001] ..." ... "[<TAG>:END]", one every MS ms on an open,
#     close-delimited stream (a reply that lasts N*MS ms). Each stream is recorded in fake.streams[TAG] (chunks sent, finished, broken);
#   * "Please follow up check <TAG>" -> the text "CHECKED <TAG> <verdict>", where history_check() read the conversation the app sent back: this tab's own
#     chunks all present, its END marker, and no marker of any other tag (cross-tab bleed). Recorded in fake.followups[TAG];
#   * "Please call mcp tool <TOOL> on <SERVER> [with <json>]" (#1834) -> a tool call to the function `mcp__<SERVER>__<TOOL>` if the app offered it in the
#     request's `tools`, else the text "NO-MCP-TOOL <SERVER>/<TOOL>" (the server did not connect). Every tool-role message is kept in fake.tool_results;
#   * any other user turn -> "OK <last word>" (prose only, no tool call).
# Endpoints: POST /api/chat (NDJSON stream), POST /api/show, GET /api/tags. The server counts requests (fake.requests) for the oracle.
import json, re, threading, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

READ_RE = re.compile(r"read exactly this file and nothing else:\s*(\S+)\s*$")
CREATE_RE = re.compile(r"create exactly this file:\s*(\S+)\s+with this content:\s*(.*?)\s*$", re.S)
DELETE_RE = re.compile(r"delete exactly this file and nothing else:\s*(\S+)\s*$")
IMG_RE = re.compile(r"show this image:\s*(\S+)\s*$")
ASK_RE = re.compile(r"run exactly this shell command and nothing else:\s*(.+?)\s*$", re.S)
STREAM_RE = re.compile(r"stream long reply (\w+) chunks (\d+) gap (\d+)")
FOLLOW_RE = re.compile(r"follow up check (\w+)")
MCP_RE = re.compile(r"call mcp tool (\S+) on (\S+)(?: with (\{.*\}))?\s*$", re.S)
MARK_RE = re.compile(r"\[(\w+):(\d{4}|END)\]")
ACTIVE = None          # the running FakeModel of this process (set by start()); the agent cases (cases_agent_*.py) read its records


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


def reply_for(messages, tools=None):
    """pure: the scripted reply to an Ollama /api/chat `messages` list -> {"text": str, "tool": None | {"name", "arguments"}}"""
    last = messages[-1] if messages else {}
    if last.get("role") == "tool":
        return {"text": "Done.", "tool": None}
    text = ""
    for m in reversed(messages):
        if m.get("role") == "user":
            text = m.get("content") or ""
            break
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
    if m:                                      # a markdown image: the app fetches it through the network gate (purpose remote-asset)
        return {"text": "Here it is: ![img](%s)" % m.group(1), "tool": None}
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
    if "SLOW" in text:
        return {"text": "slow reply", "tool": None, "slow": True}
    words = re.findall(r"\S+", text)
    return {"text": "OK %s" % (words[-1] if words else ""), "tool": None}


def stream_lines(model, reply, chunk=6):
    """pure: the NDJSON lines (str) of one streamed answer: the text in `chunk`-character pieces, the tool call (if any), then the done line"""
    out = []
    t = reply["text"]
    for piece in (reply.get("chunks") or [t[i:i + chunk] for i in range(0, len(t), chunk)]):
        out.append(json.dumps({"model": model, "message": {"role": "assistant", "content": piece}, "done": False}))
    if reply["tool"]:
        out.append(json.dumps({"model": model, "message": {"role": "assistant", "content": "", "tool_calls": [{"function": reply["tool"]}]}, "done": False}))
    out.append(json.dumps({"model": model, "message": {"role": "assistant", "content": ""}, "done": True, "done_reason": "stop"}))
    return out


class FakeModel:
    def __init__(self, delay_s=0.0):
        self.requests, self.delay_s, self.log = 0, delay_s, []
        self.streams, self.followups, self.tool_results, self.tools_seen, self.mcp_missing, self.user_turns = {}, {}, [], set(), [], []
        self.lock = threading.Lock()
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

            def do_GET(self):
                self._send(200, json.dumps({"models": [{"name": "fake-model", "model": "fake-model"}]}).encode())

            def do_POST(self):
                n = int(self.headers.get("Content-Length") or 0)
                try:
                    req = json.loads(self.rfile.read(n) or b"{}")
                except ValueError:
                    return self._send(400, b'{"error":"bad json"}')
                if self.path.endswith("/api/show"):
                    return self._send(200, json.dumps({"model_info": {"general.context_length": 32768}, "capabilities": ["completion", "tools"]}).encode())
                if not self.path.endswith("/api/chat"):
                    return self._send(404, b'{"error":"not found"}')
                owner.requests += 1
                msgs = req.get("messages") or []
                with owner.lock:
                    owner.tools_seen |= {(t.get("function") or {}).get("name") for t in req.get("tools") or []}
                    if msgs and msgs[-1].get("role") == "tool":
                        owner.tool_results.append((time.time(), msgs[-1].get("content") or ""))
                    elif msgs:
                        owner.user_turns.append((time.time(), msgs[-1].get("content") or ""))
                reply = reply_for(msgs, req.get("tools"))
                if reply.get("mcp_missing"):
                    owner.mcp_missing.append(reply["mcp_missing"])
                if reply.get("check"):
                    owner.followups[reply["check"]["tag"]] = dict(reply["check"], verdict=check_verdict(reply["check"]), t=time.time())
                tool = reply["tool"]
                owner.log.append((time.time(), tool["arguments"].get("command") if tool else None))
                if reply.get("chunks"):
                    return self._stream(req.get("model") or "fake-model", reply)
                body = ("\n".join(stream_lines(req.get("model") or "fake-model", reply)) + "\n").encode()
                if reply.get("slow") or owner.delay_s:
                    time.sleep(3.0 if reply.get("slow") else owner.delay_s)
                self._send(200, body, "application/x-ndjson")

        def _stream(self, model, reply):
            """a long reply on an open, close-delimited stream: one chunk per gap_ms; a client that goes away mid-reply marks the stream broken"""
            lines = stream_lines(model, reply)
            rec = owner.streams[reply["tag"]] = dict(total=len(reply["chunks"]), sent=0, finished=False, broken=False, t0=time.time(), t1=None)
            self.send_response(200)
            self.send_header("Content-Type", "application/x-ndjson")
            self.send_header("Connection", "close")
            self.end_headers()
            self.close_connection = True
            try:
                for i, l in enumerate(lines):
                    self.wfile.write((l + "\n").encode())
                    self.wfile.flush()
                    if i < len(reply["chunks"]):
                        rec["sent"] = i + 1
                        time.sleep(reply["gap_ms"] / 1000.0)
                rec["finished"] = True
            except (BrokenPipeError, ConnectionResetError, OSError):
                rec["broken"] = True
            rec["t1"] = time.time()

        H._stream = _stream
        self.httpd = ThreadingHTTPServer(("127.0.0.1", 0), H)
        self.url = "http://127.0.0.1:%d" % self.httpd.server_address[1]
        self.thread = threading.Thread(target=self.httpd.serve_forever, daemon=True)

    def start(self):
        global ACTIVE
        ACTIVE = self
        self.thread.start()
        return self

    def stop(self):
        self.httpd.shutdown()
        self.httpd.server_close()
