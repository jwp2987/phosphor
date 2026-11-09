# appci/fake_model.py: the L2 fake model (docs/testing.md "Agent fuzzer and fake model"): a tiny Ollama-protocol server on 127.0.0.1, stdlib only.
# run.py starts it when APPCI_FAKE_MODEL=1 and no real provider is configured, and profile.py then writes the usual Ollama provider block pointing at it
# (id appci-ollama, so every provider-dependent case sees the same provider name). It is deterministic: the reply is a pure function of the request.
#   * a user turn "Please run exactly this shell command and nothing else: <cmd>" -> one run_shell_command tool call with that command (an approval card);
#   * "Please read exactly this file and nothing else: <path>" -> read_files; "Please create exactly this file: <path> with this content: <text>" and
#     "Please delete exactly this file and nothing else: <path>" -> apply_file_diffs (the remote file-tool lab cases, cases_lab_remote.py);
#   * "Please show this image: <url>" -> the text "Here it is: ![img](<url>)" (network_gate_notice: the app fetches the image through the network gate);
#   * "Please carry out these steps in order: <step> ;; <step> ..." (agent_remote_*, cases_agent_remote.py): one tool call per step, the step being the number of tool
#     results already answered since that user turn; steps: `read <path>`, `grep <query> <dir>`, `glob <pattern> <dir>`, `edit <path> :: <search> :: <replace>`,
#     `run <command>`, `codebase <query>`; after the last step -> "Done.". Every request is recorded (FakeModel.turns) so a case can check the tool results the model
#     RECEIVED (tool_results_for);
#   * the turn after a tool result -> the text "Done." (a finished conversation);
#   * any other user turn -> "OK <last word>" (prose only, no tool call).
# Endpoints: POST /api/chat (NDJSON stream), POST /api/show, GET /api/tags. The server counts requests (fake.requests) for the oracle.
import json, re, threading, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

READ_RE = re.compile(r"read exactly this file and nothing else:\s*(\S+)\s*$")
CREATE_RE = re.compile(r"create exactly this file:\s*(\S+)\s+with this content:\s*(.*?)\s*$", re.S)
DELETE_RE = re.compile(r"delete exactly this file and nothing else:\s*(\S+)\s*$")
IMG_RE = re.compile(r"show this image:\s*(\S+)\s*$")
PLAN_RE = re.compile(r"carry out these steps in order:\s*(.+?)\s*$", re.S)
ASK_RE = re.compile(r"run exactly this shell command and nothing else:\s*(.+?)\s*$", re.S)


ACTIVE = None          # the running FakeModel (set by start()); the harness and the cases live in one process


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
    """pure: (text of the last user turn, [content of the tool messages after it])"""
    text, results = "", []
    for m in reversed(messages or []):
        if m.get("role") == "user":
            text = m.get("content") or ""
            break
        if m.get("role") == "tool":
            results.append(m.get("content") or "")
    results.reverse()
    return text, results


def reply_for(messages):
    """pure: the scripted reply to an Ollama /api/chat `messages` list -> {"text": str, "tool": None | {"name", "arguments"}}"""
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
    text = ""
    for m in reversed(messages):
        if m.get("role") == "user":
            text = m.get("content") or ""
            break
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
    for i in range(0, len(t), chunk):
        out.append(json.dumps({"model": model, "message": {"role": "assistant", "content": t[i:i + chunk]}, "done": False}))
    if reply["tool"]:
        out.append(json.dumps({"model": model, "message": {"role": "assistant", "content": "", "tool_calls": [{"function": reply["tool"]}]}, "done": False}))
    out.append(json.dumps({"model": model, "message": {"role": "assistant", "content": ""}, "done": True, "done_reason": "stop"}))
    return out


class FakeModel:
    def __init__(self, delay_s=0.0):
        self.requests, self.delay_s, self.log, self.turns = 0, delay_s, [], []
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
                reply = reply_for(req.get("messages") or [])
                owner.log.append((time.time(), (reply["tool"]["arguments"].get("command") if reply["tool"] else None)))
                owner.turns.append((time.time(),) + turn_state(req.get("messages") or []))
                body = ("\n".join(stream_lines(req.get("model") or "fake-model", reply)) + "\n").encode()
                if reply.get("slow") or owner.delay_s:
                    time.sleep(3.0 if reply.get("slow") else owner.delay_s)
                self._send(200, body, "application/x-ndjson")

        self.httpd = ThreadingHTTPServer(("127.0.0.1", 0), H)
        self.url = "http://127.0.0.1:%d" % self.httpd.server_address[1]
        self.thread = threading.Thread(target=self.httpd.serve_forever, daemon=True)

    def tool_results_for(self, marker):
        """the tool-result contents the model received in the longest request whose user turn contains `marker` (a token unique to the case)"""
        best = []
        for _t, utext, results in list(self.turns):
            if marker in utext and len(results) >= len(best):
                best = results
        return best

    def start(self):
        global ACTIVE
        ACTIVE = self
        self.thread.start()
        return self

    def stop(self):
        self.httpd.shutdown()
        self.httpd.server_close()
