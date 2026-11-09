# appci/fake_model.py: the L2 fake model (docs/testing.md "Agent fuzzer and fake model"): a tiny Ollama-protocol server on 127.0.0.1, stdlib only.
# run.py starts it when APPCI_FAKE_MODEL=1 and no real provider is configured, and profile.py then writes the usual Ollama provider block pointing at it
# (id appci-ollama, so every provider-dependent case sees the same provider name). It is deterministic: the reply is a pure function of the request.
#   * a user turn "Please run exactly this shell command and nothing else: <cmd>" -> one run_shell_command tool call with that command (an approval card);
#   * "Please read exactly this file and nothing else: <path>" -> read_files; "Please create exactly this file: <path> with this content: <text>" and
#     "Please delete exactly this file and nothing else: <path>" -> apply_file_diffs (the remote file-tool lab cases, cases_lab_remote.py);
#   * "Please show this image: <url>" -> the text "Here it is: ![img](<url>)" (network_gate_notice: the app fetches the image through the network gate);
#   * the turn after a tool result -> the text "Done." (a finished conversation);
#   * any other user turn -> "OK <last word>" (prose only, no tool call).
# Request log (#1829): when APPCI_FAKE_MODEL_LOG names a file, every /api/chat request appends one JSON line {t, n, last_role, tool, users} (request_record):
# what the model was SENT, i.e. the command output the app handed back (`tool`) and every user message (`users`), so a case can prove the model's next request
# carried the output of an interactive command and that no key typed into a program leaked into the conversation.
# Endpoints: POST /api/chat (NDJSON stream), POST /api/show, GET /api/tags. The server counts requests (fake.requests) for the oracle.
import json, os, re, threading, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

READ_RE = re.compile(r"read exactly this file and nothing else:\s*(\S+)\s*$")
CREATE_RE = re.compile(r"create exactly this file:\s*(\S+)\s+with this content:\s*(.*?)\s*$", re.S)
DELETE_RE = re.compile(r"delete exactly this file and nothing else:\s*(\S+)\s*$")
IMG_RE = re.compile(r"show this image:\s*(\S+)\s*$")
ASK_RE = re.compile(r"run exactly this shell command and nothing else:\s*(.+?)\s*$", re.S)


def reply_for(messages):
    """pure: the scripted reply to an Ollama /api/chat `messages` list -> {"text": str, "tool": None | {"name", "arguments"}}"""
    last = messages[-1] if messages else {}
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


def request_record(messages):
    """pure: what one /api/chat request carried, for the request log: the role of the last message, the content of the last tool message (None when the
    request has none) and every user message's content, in order"""
    msgs = messages or []
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
    for i in range(0, len(t), chunk):
        out.append(json.dumps({"model": model, "message": {"role": "assistant", "content": t[i:i + chunk]}, "done": False}))
    if reply["tool"]:
        out.append(json.dumps({"model": model, "message": {"role": "assistant", "content": "", "tool_calls": [{"function": reply["tool"]}]}, "done": False}))
    out.append(json.dumps({"model": model, "message": {"role": "assistant", "content": ""}, "done": True, "done_reason": "stop"}))
    return out


ACTIVE = None            # the running FakeModel of this process (cases_agent_netgate reads its request headers)


class FakeModel:
    def __init__(self, delay_s=0.0):
        self.requests, self.delay_s, self.log = 0, delay_s, []
        self.headers = []        # lower-cased header dict of every request, for the provider-key oracle (a provider's extra_headers sentinel must arrive here)
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

            def do_GET(self):
                self._send(200, json.dumps({"models": [{"name": "fake-model", "model": "fake-model"}]}).encode())

            def do_POST(self):
                owner.headers.append({k.lower(): v for k, v in self.headers.items()})
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
                if owner.log_path:
                    try:
                        rec = dict(request_record(req.get("messages") or []), t=time.time(), n=owner.requests)
                        with open(owner.log_path, "a") as f:
                            f.write(json.dumps(rec) + "\n")
                    except OSError:
                        pass
                reply = reply_for(req.get("messages") or [])
                owner.log.append((time.time(), reply["tool"]["arguments"]["command"] if reply["tool"] else None))
                body = ("\n".join(stream_lines(req.get("model") or "fake-model", reply)) + "\n").encode()
                if reply.get("slow") or owner.delay_s:
                    time.sleep(3.0 if reply.get("slow") else owner.delay_s)
                self._send(200, body, "application/x-ndjson")

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
