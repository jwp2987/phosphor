# appci/fake_model.py: the L2 fake model (docs/testing.md "Agent fuzzer and fake model"): a tiny Ollama-protocol server on 127.0.0.1, stdlib only.
# run.py starts it when APPCI_FAKE_MODEL=1 and no real provider is configured, and profile.py then writes the usual Ollama provider block pointing at it
# (id appci-ollama, so every provider-dependent case sees the same provider name). It is deterministic: the reply is a pure function of the request.
#   * a user turn "Please run exactly this shell command and nothing else: <cmd>" -> one run_shell_command tool call with that command (an approval card);
#   * "Please read exactly this file and nothing else: <path>" -> read_files; "Please create exactly this file: <path> with this content: <text>" and
#     "Please delete exactly this file and nothing else: <path>" -> apply_file_diffs (the remote file-tool lab cases, cases_lab_remote.py);
#   * "Please show this image: <url>" -> the text "Here it is: ![img](<url>)" (network_gate_notice: the app fetches the image through the network gate);
#   * the turn after a tool result -> the text "Done." (a finished conversation);
#   * any other user turn -> "OK <last word>" (prose only, no tool call).
# Endpoints: POST /api/chat (NDJSON stream), POST /api/show, GET /api/tags. The server counts requests (fake.requests) for the oracle.
# Fault injection (#1831, cases_agent_resilience.py): fake.inject(kind, times=1, match=None, **kw) arms a fault that the next `times` /api/chat requests
# (None = every one until clear_faults()) take instead of the normal answer; `match` = a substring of the request's last user text (None = any request).
#   status      kw code=429|500|.., retry_after=secs|None   -> that HTTP status with a JSON error body (and a Retry-After header)
#   disconnect  kw after=2 (lines)                           -> `after` NDJSON lines, then the connection is cut mid-body (reqwest: "connection closed")
#   delay       kw secs=30                                   -> nothing is sent for `secs` (the first token never comes); then the normal answer. A client
#                                                               that hangs up meanwhile is counted in fake.aborted (the proof that a cancel reached the wire)
#   drip        kw interval=0.5, words=60                    -> a long text reply, one 6-character line every `interval` s (a reply that visibly streams)
# fake.go_down() closes the listening socket (connection refused: "the model is down"); fake.come_up() rebinds the same port.
# Oracles the cases read: fake.requests, fake.fresh (the text of every request whose LAST message is a user turn, i.e. each prompt the app actually sent),
# fake.fired (fault kinds in the order they hit), fake.aborted (clients that hung up before the answer finished), FakeModel.ACTIVE (the running instance).
import json, re, select, socket, threading, time
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


def drip_text(words):
    """pure: the long reply a `drip` fault streams (distinct words, so a cut or a repeat is visible in the text)"""
    return " ".join("w%d" % i for i in range(int(words)))


def status_body(code):
    """pure: the JSON error body of a `status` fault"""
    return json.dumps({"error": "fake model fault: HTTP %d" % int(code)}).encode()


class FakeModel:
    ACTIVE = None                                   # the instance run.py started (cases reach the fake through it)

    def __init__(self, delay_s=0.0):
        self.requests, self.delay_s, self.log = 0, delay_s, []
        self.fresh, self.fired, self.aborted, self.faults = [], [], 0, []
        self._lock = threading.Lock()
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
                if msgs and msgs[-1].get("role") == "user":
                    owner.fresh.append(msgs[-1].get("content") or "")
                user_text = next((m.get("content") or "" for m in reversed(msgs) if m.get("role") == "user"), "")
                fault = owner._take(user_text)
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
                reply = reply_for(msgs)
                owner.log.append((time.time(), reply["tool"]["arguments"]["command"] if reply["tool"] else None))
                body = ("\n".join(stream_lines(req.get("model") or "fake-model", reply)) + "\n").encode()
                if reply.get("slow") or owner.delay_s:
                    time.sleep(3.0 if reply.get("slow") else owner.delay_s)
                self._send(200, body, "application/x-ndjson")

        self._handler = H
        self.httpd = ThreadingHTTPServer(("127.0.0.1", 0), H)
        self.port = self.httpd.server_address[1]
        self.url = "http://127.0.0.1:%d" % self.port
        self.thread = threading.Thread(target=self.httpd.serve_forever, daemon=True)
        self.up = True

    def inject(self, kind, times=1, match=None, **kw):
        assert kind in ("status", "disconnect", "delay", "drip"), kind
        with self._lock:
            self.faults.append({"kind": kind, "left": times, "match": match, "kw": kw})

    def clear_faults(self):
        with self._lock:
            self.faults = []

    def _take(self, user_text):
        with self._lock:
            for f in self.faults:
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

    def start(self):
        self.thread.start()
        FakeModel.ACTIVE = self
        return self

    def stop(self):
        if self.up:
            self.httpd.shutdown()
            self.httpd.server_close()
            self.up = False
        if FakeModel.ACTIVE is self:
            FakeModel.ACTIVE = None
