#!/usr/bin/env python3
"""A fake OpenAI-style embeddings server for the remote-index tests (Wave H parity, P9a).

    script/fake_embeddings.py [--port N] [--dims N] [--host 127.0.0.1]

POST /v1/embeddings  {"input": "text" | ["text", ...], "model": "...", "dimensions": N?}
answers the OpenAI shape, with a deterministic unit vector per input text (derived from
SHA-256 of the text, so equal text gives an equal vector on every run and machine).
GET /health answers "ok". Standard library only; it binds loopback by default and never opens
an outgoing connection. Port 0 picks a free port; the chosen port is the first line on stdout.
"""
import argparse
import hashlib
import json
import math
import struct
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

MAX_BODY = 32 * 1024 * 1024


def embed(text: str, dims: int) -> list:
    """Deterministic unit vector of `dims` floats for `text`."""
    raw = []
    counter = 0
    while len(raw) < dims:
        block = hashlib.sha256(f"{counter}\0{text}".encode("utf-8")).digest()
        for i in range(0, len(block), 4):
            (n,) = struct.unpack(">I", block[i : i + 4])
            raw.append(n / 2147483647.5 - 1.0)  # [-1, 1]
        counter += 1
    raw = raw[:dims]
    norm = math.sqrt(sum(x * x for x in raw)) or 1.0
    return [round(x / norm, 8) for x in raw]


def make_handler(default_dims: int, log_path=None):
    class Handler(BaseHTTPRequestHandler):
        def _send(self, code: int, body: bytes, ctype: str = "application/json"):
            self.send_response(code)
            self.send_header("Content-Type", ctype)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def do_GET(self):
            if self.path == "/health":
                self._send(200, b"ok", "text/plain")
            else:
                self._send(404, b'{"error":{"message":"not found"}}')

        def do_POST(self):
            if self.path.split("?")[0] not in ("/v1/embeddings", "/embeddings"):
                return self._send(404, b'{"error":{"message":"not found"}}')
            length = int(self.headers.get("Content-Length") or 0)
            if length > MAX_BODY:
                return self._send(413, b'{"error":{"message":"too large"}}')
            try:
                req = json.loads(self.rfile.read(length) or b"{}")
                inputs = req["input"]
                if isinstance(inputs, str):
                    inputs = [inputs]
                if not isinstance(inputs, list) or not all(isinstance(t, str) for t in inputs):
                    raise ValueError("input must be a string or a list of strings")
                dims = int(req.get("dimensions") or default_dims)
            except (KeyError, ValueError, TypeError) as exc:
                msg = json.dumps({"error": {"message": f"bad request: {exc}"}})
                return self._send(400, msg.encode())
            data = [
                {"object": "embedding", "index": i, "embedding": embed(t, dims)}
                for i, t in enumerate(inputs)
            ]
            if log_path:
                # One line per request, for the lab: how many texts, and a short hash of the
                # sentinel header (never the header itself) so a test can tell the app's key
                # reached this endpoint without the key ever being written down.
                sentinel = self.headers.get("X-Appci-Key") or ""
                digest = hashlib.sha256(sentinel.encode()).hexdigest()[:8] if sentinel else "none"
                with open(log_path, "a") as fh:
                    fh.write(f"embed inputs={len(inputs)} dims={dims} key_sha={digest}\n")
            tokens = sum(len(t.split()) for t in inputs)
            out = {
                "object": "list",
                "data": data,
                "model": req.get("model", "fake-embedding"),
                "usage": {"prompt_tokens": tokens, "total_tokens": tokens},
            }
            self._send(200, json.dumps(out).encode())

        def log_message(self, fmt, *args):  # quiet: the harness reads stdout
            pass

    return Handler


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--port", type=int, default=0)
    ap.add_argument("--dims", type=int, default=64)
    ap.add_argument("--log", default=None, help="append one line per embeddings request here")
    args = ap.parse_args()
    server = ThreadingHTTPServer((args.host, args.port), make_handler(args.dims, args.log))
    print(server.server_address[1], flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
