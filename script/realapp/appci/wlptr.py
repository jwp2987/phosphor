#!/usr/bin/env python3
"""wlptr.py: a real pointer for the headless sway session (#1789), with no install: zwlr_virtual_pointer_v1 spoken over the Wayland wire
by plain Python (sockets + struct; no fd passing is needed for this protocol).

Why a persistent holder: headless sway has NO input devices (WLR_LIBINPUT_NO_DEVICES=1), so its seat advertises no pointer capability and
clients never create a wl_pointer. `swaymsg seat - cursor ...` moves sway's own cursor, but no client is ever told about it: every
pointer-dependent appci case saw "the pointer events never reached the app". This is the same trap the virtual keyboard fell into (#1414,
wayland_session.sh keeps one wtype alive). `serve` creates ONE virtual pointer when the session starts (before the app, so the app binds
wl_pointer once) and keeps it for the whole run; the harness sends it one-line commands over a unix socket.

  wlptr.py serve --ctl SOCK        create the pointer, listen on SOCK (exit when the compositor goes away)
  wlptr.py cmd --ctl SOCK  abs X Y W H | btn CODE STATE | axis VALUE | ping
Commands (each ends with a wl_pointer.frame and a compositor round trip; the reply line is "ok" or "err <why>"):
  abs X Y W H    motion_absolute to (X, Y) in an output of W x H (layout pixels; sway's single headless output sits at 0,0)
  btn CODE STATE button (evdev code: 272 left, 273 right, 274 middle), STATE 1 pressed / 0 released
  axis VALUE     vertical scroll by VALUE (wl_fixed units are applied here; positive = down)
"""
import os
import socket
import struct
import sys
import time

BTN = {1: 0x110, 2: 0x112, 3: 0x111}     # X11-style button numbers (1 left, 2 middle, 3 right) -> evdev codes


def _pstr(s):
    b = s.encode() + b"\0"
    return struct.pack("<I", len(b)) + b + b"\0" * (-len(b) % 4)


def _rstr(buf, off):
    n = struct.unpack_from("<I", buf, off)[0]
    s = buf[off + 4:off + 4 + max(n - 1, 0)].decode(errors="replace")
    return s, off + 4 + n + (-n % 4)


def display_path(env=None):
    env = os.environ if env is None else env
    d = env.get("WAYLAND_DISPLAY") or "wayland-0"
    return d if os.path.isabs(d) else os.path.join(env.get("XDG_RUNTIME_DIR", ""), d)


class VirtualPointer:
    """one zwlr_virtual_pointer_v1 on the compositor at `path` (default: WAYLAND_DISPLAY in XDG_RUNTIME_DIR)"""

    def __init__(self, path=None):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(10)
        self.sock.connect(path or display_path())
        self.buf = b""
        self.nid = 1
        self.globals = {}
        reg = self.new_id()
        self.send(1, 1, struct.pack("<I", reg))                    # wl_display.get_registry
        self.roundtrip()
        seat = mgr = None
        for name, (iface, ver) in self.globals.items():
            if iface == "wl_seat" and seat is None:
                seat = self.bind(reg, name, iface, 1)
            elif iface == "zwlr_virtual_pointer_manager_v1":
                mgr = self.bind(reg, name, iface, min(ver, 1))
        if mgr is None:
            raise RuntimeError("the compositor does not offer zwlr_virtual_pointer_manager_v1 (globals: %s)" % sorted(v[0] for v in self.globals.values()))
        self.ptr = self.new_id()
        self.send(mgr, 0, struct.pack("<II", seat or 0, self.ptr))   # create_virtual_pointer(seat, id)
        self.roundtrip()

    def new_id(self):
        self.nid += 1
        return self.nid

    def bind(self, reg, name, iface, ver):
        i = self.new_id()
        self.send(reg, 0, struct.pack("<I", name) + _pstr(iface) + struct.pack("<II", ver, i))
        return i

    def send(self, obj, opcode, body=b""):
        self.sock.sendall(struct.pack("<II", obj, ((8 + len(body)) << 16) | opcode) + body)

    def _events(self):
        """parse buffered events: [(object, opcode, body)]"""
        out = []
        while len(self.buf) >= 8:
            obj, w = struct.unpack_from("<II", self.buf)
            size, op = w >> 16, w & 0xFFFF
            if size < 8 or len(self.buf) < size:
                break
            out.append((obj, op, self.buf[8:size]))
            self.buf = self.buf[size:]
        return out

    def roundtrip(self):
        cb = self.new_id()
        self.send(1, 0, struct.pack("<I", cb))                     # wl_display.sync
        while True:
            for obj, op, body in self._events():
                if obj == 1 and op == 0:                           # wl_display.error
                    oid, code = struct.unpack_from("<II", body)
                    raise RuntimeError("wayland error on object %d code %d: %s" % (oid, code, _rstr(body, 8)[0]))
                if obj == self.nid_registry() and op == 0:         # wl_registry.global
                    name = struct.unpack_from("<I", body)[0]
                    iface, off = _rstr(body, 4)
                    self.globals[name] = (iface, struct.unpack_from("<I", body, off)[0])
                if obj == cb and op == 0:
                    return
            d = self.sock.recv(65536)
            if not d:
                raise RuntimeError("the compositor closed the connection")
            self.buf += d

    def nid_registry(self):
        return 2

    @staticmethod
    def now():
        return int(time.monotonic() * 1000) & 0xFFFFFFFF

    def frame(self):
        self.send(self.ptr, 4)
        self.roundtrip()

    def motion_absolute(self, x, y, w, h):
        self.send(self.ptr, 1, struct.pack("<IIIII", self.now(), int(x), int(y), int(w), int(h)))
        self.frame()

    def button(self, code, state):
        self.send(self.ptr, 2, struct.pack("<III", self.now(), code, 1 if state else 0))
        self.frame()

    def axis(self, value):
        self.send(self.ptr, 5, struct.pack("<I", 0))                # axis_source: wheel
        self.send(self.ptr, 3, struct.pack("<IIi", self.now(), 0, int(round(value * 256))))
        self.frame()


def handle(vp, line):
    """run one command line on `vp`; the reply line"""
    w = line.split()
    try:
        if w[0] == "ping":
            return "ok"
        if w[0] == "abs" and len(w) == 5:
            vp.motion_absolute(*[float(v) for v in w[1:]])
        elif w[0] == "btn" and len(w) == 3:
            vp.button(int(w[1]), int(w[2]))
        elif w[0] == "axis" and len(w) == 2:
            vp.axis(float(w[1]))
        else:
            return "err bad command %r" % line
    except Exception as e:                                          # the harness reads this reply, never a silent drop
        return "err %s" % e
    return "ok"


def serve(ctl, path=None):
    vp = VirtualPointer(path)
    if os.path.exists(ctl):
        os.unlink(ctl)
    srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    srv.bind(ctl)
    srv.listen(4)
    print("wlptr: virtual pointer up, control socket %s" % ctl, flush=True)
    while True:
        c, _ = srv.accept()
        with c:
            c.settimeout(10)
            try:
                line = b""
                while not line.endswith(b"\n"):
                    d = c.recv(4096)
                    if not d:
                        break
                    line += d
                c.sendall((handle(vp, line.decode().strip()) + "\n").encode())
            except OSError:
                pass


def send_cmd(ctl, line, timeout=10):
    """one command to a running holder; the reply ('ok' / 'err ...'); raises OSError when there is no holder"""
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.settimeout(timeout)
    try:
        s.connect(ctl)
        s.sendall((line + "\n").encode())
        r = b""
        while not r.endswith(b"\n"):
            d = s.recv(4096)
            if not d:
                break
            r += d
        return r.decode().strip()
    finally:
        s.close()


def main(argv):
    if len(argv) >= 4 and argv[1] in ("serve", "cmd") and argv[2] == "--ctl":
        if argv[1] == "serve":
            try:
                serve(argv[3])
            except Exception as e:
                print("wlptr: %s" % e, file=sys.stderr, flush=True)
                return 1
            return 0
        r = send_cmd(argv[3], " ".join(argv[4:]))
        print(r)
        return 0 if r == "ok" else 1
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
