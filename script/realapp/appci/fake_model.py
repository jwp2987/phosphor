# appci/fake_model.py: the L2 fake model (docs/testing.md "Agent fuzzer and fake model"): a tiny Ollama-protocol server on 127.0.0.1, stdlib only.
# run.py starts it when APPCI_FAKE_MODEL=1 and no real provider is configured, and profile.py then writes the usual Ollama provider block pointing at it
# (id appci-ollama, so every provider-dependent case sees the same provider name). It is deterministic: the reply is a pure function of the request.
#   * a user turn "Please run exactly this shell command and nothing else: <cmd>" -> one run_shell_command tool call with that command (an approval card);
#   * "Please read exactly this file and nothing else: <path>" -> read_files; "Please create exactly this file: <path> with this content: <text>" and
#     "Please delete exactly this file and nothing else: <path>" -> apply_file_diffs (the remote file-tool lab cases, cases_lab_remote.py);
#   * "Please show this image: <url>" -> the text "Here it is: ![img](<url>)" (network_gate_notice: the app fetches the image through the network gate);
#   * the turn after a tool result -> the text "Done." (a finished conversation);
#   * a conversation whose user turns contain STREAMLONG (#1828) -> a SLOW STEADY stream: STREAM_CHUNKS pieces STREAM_GAP_S apart (about 12 s), sent as
#     they are produced (no Content-Length), so a reply is visibly running while a case types. Sticky: every later turn of that conversation streams
#     too, so a steer (#851) typed while the reply runs is itself answered by a running stream. fake.reqs holds every /api/chat request body.
#   * any other user turn -> "OK <last word>" (prose only, no tool call).
# Endpoints: POST /api/chat (NDJSON stream), POST /api/show, GET /api/tags. The server counts requests (fake.requests) for the oracle.
import json, re, threading, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

READ_RE = re.compile(r"read exactly this file and nothing else:\s*(\S+)\s*$")
CREATE_RE = re.compile(r"create exactly this file:\s*(\S+)\s+with this content:\s*(.*?)\s*$", re.S)
DELETE_RE = re.compile(r"delete exactly this file and nothing else:\s*(\S+)\s*$")
IMG_RE = re.compile(r"show this image:\s*(\S+)\s*$")
ASK_RE = re.compile(r"run exactly this shell command and nothing else:\s*(.+?)\s*$", re.S)


STREAM_CHUNKS, STREAM_GAP_S = 240, 0.05


def stream_text():
    """pure: the text of the slow steady stream, 't0000 t0001 ...' cut into 6-character pieces by stream_lines"""
    return "".join("t%04d " % i for i in range(STREAM_CHUNKS))


def user_texts(messages):
    """pure: the content of every user message of a /api/chat request, in order"""
    return [m.get("content") or "" for m in messages or [] if m.get("role") == "user"]


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
    if any("STREAMLONG" in t for t in user_texts(messages)):
        return {"text": stream_text(), "tool": None, "stream": True}
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
    ACTIVE = None                                   # the instance run.py started (cases reach the fake through it)

    def __init__(self, delay_s=0.0):
        self.requests, self.delay_s, self.log = 0, delay_s, []
        self.reqs = []          # #1828: one {"t": epoch s, "messages": [...]} per /api/chat request, in arrival order (the steer cases' oracle)
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
                owner.reqs.append({"t": time.time(), "messages": req.get("messages") or []})
                reply = reply_for(req.get("messages") or [])
                owner.log.append((time.time(), reply["tool"]["arguments"]["command"] if reply["tool"] else None))
                if reply.get("stream"):
                    # paced, not Content-Length'd: an HTTP/1.0 close-delimited body, each NDJSON line flushed as it is produced
                    self.send_response(200)
                    self.send_header("Content-Type", "application/x-ndjson")
                    self.end_headers()
                    try:
                        for ln in stream_lines(req.get("model") or "fake-model", reply):
                            self.wfile.write((ln + "\n").encode())
                            self.wfile.flush()
                            time.sleep(STREAM_GAP_S)
                    except (BrokenPipeError, ConnectionResetError):
                        pass            # the app cancelled the stream (a steer or Ctrl-C): not an error here
                    return
                body = ("\n".join(stream_lines(req.get("model") or "fake-model", reply)) + "\n").encode()
                if reply.get("slow") or owner.delay_s:
                    time.sleep(3.0 if reply.get("slow") else owner.delay_s)
                self._send(200, body, "application/x-ndjson")

        self.httpd = ThreadingHTTPServer(("127.0.0.1", 0), H)
        self.url = "http://127.0.0.1:%d" % self.httpd.server_address[1]
        self.thread = threading.Thread(target=self.httpd.serve_forever, daemon=True)

    def start(self):
        FakeModel.ACTIVE = self
        self.thread.start()
        return self

    def stop(self):
        self.httpd.shutdown()
        self.httpd.server_close()
