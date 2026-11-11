#!/usr/bin/env python3
"""notify_recorder.py: a recording org.freedesktop.Notifications service for the app's PRIVATE D-Bus session (r14-test-gaps T37, H7), with no
install: the D-Bus wire protocol spoken by plain Python (a unix socket + struct; EXTERNAL auth), like wlptr.py speaks Wayland.

Why: the app sends a desktop notification through the session bus (notify-rust: one `Notify` method call). The launcher's private bus
(app.py, dbus-run-session) has no notification daemon, so every notification of every run so far failed with ServiceUnknown and nothing
looked. This service owns the name BEFORE the app starts, answers `Notify` like a daemon and writes one JSON line per notification.

  notify_recorder.py serve --log FILE --ready FILE      own org.freedesktop.Notifications on $DBUS_SESSION_BUS_ADDRESS until the bus goes away
  notify_recorder.py send  --address ADDR SUMMARY BODY  one Notify call (the harness's control: proves the recording path end to end)

--ready FILE gets one JSON line once the name is owned: {"ok": true, "address": ..., "name": ..., "pid": ...} or {"ok": false, "why": ...}
(another daemon already owns the name: the recorder asked with REPLACE_EXISTING | DO_NOT_QUEUE and says who kept it).
Log lines: {"t": epoch, "id": n, "app": .., "replaces": n, "summary": .., "body": .., "actions": [..], "timeout": ms, "sender": ..}
Two seconds after each notification the service emits NotificationClosed(id, 1 = expired), as a daemon does when the bubble goes away
(notify-rust keeps one blocked thread per notification until then).
The message text is the user's command line: the log stays in the job's run directory and is scrubbed with the other artifacts.
"""
import json
import os
import socket
import struct
import sys
import time

NAME = "org.freedesktop.Notifications"
PATH = "/org/freedesktop/Notifications"
BUS = "org.freedesktop.DBus"
METHOD_CALL, METHOD_RETURN, ERROR, SIGNAL = 1, 2, 3, 4
# header field codes
F_PATH, F_IFACE, F_MEMBER, F_ERROR, F_REPLY, F_DEST, F_SENDER, F_SIG = 1, 2, 3, 4, 5, 6, 7, 8
FIELD_SIG = {F_PATH: "o", F_IFACE: "s", F_MEMBER: "s", F_ERROR: "s", F_REPLY: "u", F_DEST: "s", F_SENDER: "s", F_SIG: "g"}
CLOSE_AFTER_S = 2.0
ALIGN = {"y": 1, "b": 4, "n": 2, "q": 2, "i": 4, "u": 4, "x": 8, "t": 8, "d": 8, "s": 4, "o": 4, "g": 1, "a": 4, "(": 8, "{": 8, "v": 1, "h": 4}
FIXED = {"y": "B", "b": "I", "n": "h", "q": "H", "i": "i", "u": "I", "x": "q", "t": "Q", "d": "d", "h": "I"}


# ---------------------------------------------------------------- signatures
def split_sig(sig):
    """the complete types of a signature, in order: 'susa{sv}i' -> ['s', 'u', 's', 'a{sv}', 'i']"""
    out, i = [], 0
    while i < len(sig):
        j = _type_end(sig, i)
        out.append(sig[i:j])
        i = j
    return out


def _type_end(sig, i):
    c = sig[i]
    if c == "a":
        return _type_end(sig, i + 1)
    if c in "({":
        close, depth, j = (")" if c == "(" else "}"), 0, i
        while j < len(sig):
            if sig[j] == c:
                depth += 1
            elif sig[j] == close:
                depth -= 1
                if depth == 0:
                    return j + 1
            j += 1
        raise ValueError("unbalanced %r in signature %r" % (c, sig))
    if c not in ALIGN:
        raise ValueError("unknown type %r in signature %r" % (c, sig))
    return i + 1


# ---------------------------------------------------------------- marshalling (little endian only: we write 'l', and refuse a big-endian peer)
def _pad(buf, n):
    buf.extend(b"\0" * (-len(buf) % n))


def marshal(buf, sig, value):
    """append `value` of the single complete type `sig` to the bytearray `buf` (alignment is relative to buf's start)"""
    c = sig[0]
    _pad(buf, ALIGN[c])
    if c in FIXED:
        buf.extend(struct.pack("<" + FIXED[c], int(value) if c != "d" else float(value)))
    elif c in "so":
        b = value.encode("utf-8")
        buf.extend(struct.pack("<I", len(b)) + b + b"\0")
    elif c == "g":
        b = value.encode("ascii")
        buf.extend(bytes([len(b)]) + b + b"\0")
    elif c == "a":
        elem = sig[1:]
        at = len(buf)
        buf.extend(b"\0\0\0\0")
        _pad(buf, ALIGN[elem[0]])
        start = len(buf)
        items = value.items() if isinstance(value, dict) else value
        for it in items:
            marshal(buf, elem, it)
        struct.pack_into("<I", buf, at, len(buf) - start)
    elif c in "({":
        for s, v in zip(split_sig(sig[1:-1]), value):
            marshal(buf, s, v)
    elif c == "v":
        vsig, v = value
        marshal(buf, "g", vsig)
        marshal(buf, vsig, v)
    else:
        raise ValueError("cannot marshal %r" % sig)


def unmarshal(data, off, sig):
    """-> (value, new offset) for the single complete type `sig` at `off` of `data` (alignment relative to data's start)"""
    c = sig[0]
    off += -off % ALIGN[c]
    if c in FIXED:
        f = "<" + FIXED[c]
        return struct.unpack_from(f, data, off)[0], off + struct.calcsize(f)
    if c in "so":
        n = struct.unpack_from("<I", data, off)[0]
        return bytes(data[off + 4:off + 4 + n]).decode("utf-8", "replace"), off + 4 + n + 1
    if c == "g":
        n = data[off]
        return bytes(data[off + 1:off + 1 + n]).decode("ascii", "replace"), off + 1 + n + 1
    if c == "a":
        elem = sig[1:]
        n = struct.unpack_from("<I", data, off)[0]
        off += 4
        off += -off % ALIGN[elem[0]]
        end, out = off + n, []
        while off < end:
            v, off = unmarshal(data, off, elem)
            out.append(v)
        return out, end
    if c in "({":
        out = []
        for s in split_sig(sig[1:-1]):
            v, off = unmarshal(data, off, s)
            out.append(v)
        return tuple(out), off
    if c == "v":
        vsig, off = unmarshal(data, off, "g")
        v, off = unmarshal(data, off, vsig)
        return (vsig, v), off
    raise ValueError("cannot unmarshal %r" % sig)


def build_message(mtype, serial, fields, sig="", args=(), flags=0):
    """one D-Bus message: fields = {field code: value}; args are marshalled with `sig`"""
    body = bytearray()
    for s, v in zip(split_sig(sig), args):
        marshal(body, s, v)
    f = dict(fields)
    if sig:
        f[F_SIG] = sig
    head = bytearray(struct.pack("<cBBBII", b"l", mtype, flags, 1, len(body), serial))
    marshal(head, "a(yv)", [(code, (FIELD_SIG[code], val)) for code, val in sorted(f.items())])
    _pad(head, 8)
    return bytes(head) + bytes(body)


def message_size(buf):
    """total size of the message at the start of `buf`, or None while its fixed header is incomplete"""
    if len(buf) < 16:
        return None
    if buf[0:1] != b"l":
        raise ValueError("big-endian D-Bus peer is not supported")
    body_len, _serial, fields_len = struct.unpack_from("<III", buf, 4)
    head = 16 + fields_len
    return head + (-head % 8) + body_len


def parse_message(data):
    """-> dict(type, flags, serial, fields {code: value}, args [...]) of one whole message"""
    _e, mtype, flags, _v, body_len, serial = struct.unpack_from("<cBBBII", data, 0)
    raw, off = unmarshal(data, 12, "a(yv)")
    fields = {code: val for code, (_s, val) in raw}
    off += -off % 8
    body, args = data[off:off + body_len], []
    o = 0
    for s in split_sig(fields.get(F_SIG, "")):
        v, o = unmarshal(body, o, s)
        args.append(v)
    return dict(type=mtype, flags=flags, serial=serial, fields=fields, args=args)


# ---------------------------------------------------------------- the service's behaviour (pure: no socket)
class Service:
    """what the recorder answers. handle(msg) -> [message bytes to send]; every Notify is appended to self.records"""

    def __init__(self, now=time.time):
        self.records, self.serial, self.next_id, self.now = [], 100, 1, now
        self.to_close = []          # [(due time, notification id)]

    def _serial(self):
        self.serial += 1
        return self.serial

    def _reply(self, msg, sig="", args=()):
        f = {F_REPLY: msg["serial"]}
        if msg["fields"].get(F_SENDER):
            f[F_DEST] = msg["fields"][F_SENDER]
        return build_message(METHOD_RETURN, self._serial(), f, sig, args)

    def _error(self, msg, name, text):
        f = {F_REPLY: msg["serial"], F_ERROR: name}
        if msg["fields"].get(F_SENDER):
            f[F_DEST] = msg["fields"][F_SENDER]
        return build_message(ERROR, self._serial(), f, "s", (text,))

    def handle(self, msg):
        if msg["type"] != METHOD_CALL:
            return []
        fl, member, iface = msg["fields"], msg["fields"].get(F_MEMBER), msg["fields"].get(F_IFACE)
        no_reply = bool(msg["flags"] & 1)
        out = []
        if member == "Notify" and iface in (NAME, None) and len(msg["args"]) >= 8:
            app, replaces, _icon, summary, body, actions, _hints, timeout = msg["args"][:8]
            nid = replaces or self.next_id
            if not replaces:
                self.next_id += 1
            self.records.append(dict(t=round(self.now(), 3), id=nid, app=app, replaces=replaces, summary=summary, body=body, actions=list(actions),
                                     timeout=timeout, sender=fl.get(F_SENDER)))
            self.to_close.append((self.now() + CLOSE_AFTER_S, nid))
            out.append(self._reply(msg, "u", (nid,)))
        elif member == "GetCapabilities":
            out.append(self._reply(msg, "as", (["body"],)))
        elif member == "GetServerInformation":
            out.append(self._reply(msg, "ssss", ("appci-notify-recorder", "phosphor-appci", "1", "1.2")))
        elif member == "CloseNotification":
            out.append(self._reply(msg))
        elif member == "Ping" and iface == "org.freedesktop.DBus.Peer":
            out.append(self._reply(msg))
        elif member == "Introspect":
            out.append(self._reply(msg, "s", ("<node/>",)))
        else:
            out.append(self._error(msg, "org.freedesktop.DBus.Error.UnknownMethod", "appci recorder: no method %s.%s" % (iface, member)))
        return [] if no_reply else out

    def due_signals(self):
        """NotificationClosed(id, 1) for every notification whose two seconds are over"""
        now, out, keep = self.now(), [], []
        for due, nid in self.to_close:
            if due <= now:
                out.append(build_message(SIGNAL, self._serial(), {F_PATH: PATH, F_IFACE: NAME, F_MEMBER: "NotificationClosed"}, "uu", (nid, 1)))
            else:
                keep.append((due, nid))
        self.to_close = keep
        return out


# ---------------------------------------------------------------- the bus connection
def parse_address(addr):
    """the first unix transport of a D-Bus address -> the socket address for socket.connect (bytes with a leading NUL for an abstract socket)"""
    for part in (addr or "").split(";"):
        if not part.startswith("unix:"):
            continue
        kv = dict(p.split("=", 1) for p in part[5:].split(",") if "=" in p)
        if "path" in kv:
            return kv["path"]
        if "abstract" in kv:
            return b"\0" + kv["abstract"].encode()
    raise ValueError("no unix transport in D-Bus address %r" % addr)


class Bus:
    def __init__(self, address=None, timeout=10):
        self.address = address or os.environ.get("DBUS_SESSION_BUS_ADDRESS", "")
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(timeout)
        self.sock.connect(parse_address(self.address))
        self.buf, self.serial, self.pending = b"", 0, []
        self.sock.sendall(b"\0AUTH EXTERNAL " + str(os.getuid()).encode().hex().encode() + b"\r\n")
        line = b""
        while not line.endswith(b"\r\n"):
            c = self.sock.recv(1)
            if not c:
                raise ConnectionError("the bus closed the connection during auth")
            line += c
        if not line.startswith(b"OK"):
            raise ConnectionError("D-Bus auth refused: %r" % line[:80])
        self.sock.sendall(b"BEGIN\r\n")
        self.unique = self.call(BUS, "/org/freedesktop/DBus", BUS, "Hello")[0]

    def send(self, data):
        self.sock.sendall(data)

    def read(self, timeout=None):
        """the next whole message (parsed), or None on timeout; raises ConnectionError when the bus is gone"""
        self.sock.settimeout(timeout)
        while True:
            n = message_size(self.buf)
            if n is not None and len(self.buf) >= n:
                data, self.buf = self.buf[:n], self.buf[n:]
                return parse_message(data)
            try:
                chunk = self.sock.recv(65536)
            except socket.timeout:
                return None
            if not chunk:
                raise ConnectionError("the bus closed the connection")
            self.buf += chunk

    def call(self, dest, path, iface, member, sig="", args=(), timeout=10):
        """a method call; -> the reply's args. Messages that arrive meanwhile are kept in self.pending"""
        self.serial += 1
        mine = self.serial
        self.send(build_message(METHOD_CALL, mine, {F_PATH: path, F_IFACE: iface, F_MEMBER: member, F_DEST: dest}, sig, args))
        end = time.time() + timeout
        while time.time() < end:
            m = self.read(max(0.05, end - time.time()))
            if m is None:
                continue
            if m["fields"].get(F_REPLY) == mine and m["type"] in (METHOD_RETURN, ERROR):
                if m["type"] == ERROR:
                    raise RuntimeError("%s: %s" % (m["fields"].get(F_ERROR), m["args"][0] if m["args"] else ""))
                return m["args"]
            self.pending.append(m)
        raise TimeoutError("no reply to %s.%s within %s s" % (iface, member, timeout))


def serve(log, ready, address=None):
    def say(d):
        with open(ready + ".tmp", "w", encoding="utf-8") as f:
            f.write(json.dumps(d) + "\n")
        os.replace(ready + ".tmp", ready)
    try:
        bus = Bus(address)
        # REPLACE_EXISTING (2) | DO_NOT_QUEUE (4): take the name from an activated daemon that allows it, never wait behind one
        rc = bus.call(BUS, "/org/freedesktop/DBus", BUS, "RequestName", "su", (NAME, 2 | 4))[0]
        if rc not in (1, 4):
            owner = "?"
            try:
                owner = bus.call(BUS, "/org/freedesktop/DBus", BUS, "GetNameOwner", "s", (NAME,))[0]
            except Exception:
                pass
            say(dict(ok=False, why="%s is owned by another connection (%s) that does not allow replacement (RequestName returned %d)" % (NAME, owner, rc)))
            return 1
    except Exception as e:
        say(dict(ok=False, why="%s: %s" % (type(e).__name__, e)))
        return 1
    svc = Service()
    open(log, "a").close()
    say(dict(ok=True, address=bus.address, name=bus.unique, pid=os.getpid()))
    backlog, bus.pending = bus.pending, []
    try:
        while True:
            msg = backlog.pop(0) if backlog else bus.read(0.25)
            if msg is not None:
                n0 = len(svc.records)
                for out in svc.handle(msg):
                    bus.send(out)
                if len(svc.records) > n0:
                    with open(log, "a", encoding="utf-8") as f:
                        for rec in svc.records[n0:]:
                            f.write(json.dumps(rec, ensure_ascii=False) + "\n")
            for out in svc.due_signals():
                bus.send(out)
    except (ConnectionError, OSError):
        return 0


def send(address, summary, body, app="appci-control"):
    """one Notify call on the bus at `address` -> the notification id the service returned"""
    bus = Bus(address)
    try:
        return bus.call(NAME, PATH, NAME, "Notify", "susssasa{sv}i", (app, 0, "", summary, body, [], {}, -1))[0]
    finally:
        bus.sock.close()


def read_log(path):
    """the notifications recorded in `path` (missing file: none); a torn last line is ignored"""
    out = []
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            for line in f:
                try:
                    out.append(json.loads(line))
                except ValueError:
                    pass
    except OSError:
        pass
    return out


def main(argv):
    def opt(name, default=None):
        return argv[argv.index(name) + 1] if name in argv else default
    if len(argv) >= 2 and argv[1] == "serve" and opt("--log") and opt("--ready"):
        return serve(opt("--log"), opt("--ready"), opt("--address"))
    if len(argv) >= 2 and argv[1] == "send" and opt("--address") and len(argv) >= 6:
        print(send(opt("--address"), argv[-2], argv[-1]))
        return 0
    sys.stderr.write(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
