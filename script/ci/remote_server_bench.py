#!/usr/bin/env python3
# script/ci/remote_server_bench.py: size and request latency of the ssh remote-server binary (#1157 part 2: release-cli (opt-level "s", musl malloc)
# against release-remote (opt-level 3, mimalloc), so the next build-box run produces the numbers instead of an estimate).
#
#   remote_server_bench.py --bin before=path/to/phosphor-oss --bin after=path/to/phosphor-oss [--n 300] [--dir-entries 3000] --out remote-bench.json
#
# Per binary:
#   size            bytes of the binary, bytes of its gzip -9 (the tarball a host downloads), the same for any --tarball label=file given
#   startup_ms      `remote-server-proxy` spawn -> the InitializeResponse frame (this includes starting the daemon the proxy forks)
#   rtt_ms          p50 / p95 / p99 / max of N round trips of a small request (ResolvePath) over the proxy's stdio, one at a time
#   list_ms         the same for ListDirectory of a directory with --dir-entries files (many small allocations: the malloc-sensitive path)
#   burst_ms        N ListDirectory requests written back to back, wall time until the last response, divided by N (throughput)
# The proxy is the real wire path of every ssh session (length-prefixed protobuf, crates/remote_server/proto/remote_server.proto); the requests are
# hand-encoded (no protobuf library) and only the response's request_id is decoded. HOME is a scratch directory, so the daemon (forked by the proxy,
# in its own session) lives under it; its exact pid is read from the pid file it writes there and killed at the end. Nothing outside the scratch
# directory is touched. Linux/macOS only (a unix socket daemon). With two or more --bin the output has a `delta` table against the first.
import argparse, gzip, json, os, shutil, signal, struct, subprocess, sys, tempfile, time


# ---------------------------------------------------------------- minimal protobuf encoding (pure, unit-tested in appci/selftest.py)
def varint(n):
    out = bytearray()
    while True:
        b = n & 0x7F
        n >>= 7
        if n:
            out.append(b | 0x80)
        else:
            out.append(b)
            return bytes(out)


def field(num, payload):
    """a length-delimited field (string / bytes / embedded message)"""
    if isinstance(payload, str):
        payload = payload.encode()
    return varint((num << 3) | 2) + varint(len(payload)) + payload


def frame(msg):
    return struct.pack("<I", len(msg)) + msg


def msg_initialize(rid):
    """ClientMessage{request_id, session_scoped{initialize{}}}"""
    return field(1, rid) + field(3, field(1, b""))


def msg_resolve_path(rid, path):
    """ClientMessage{request_id, host_scoped{resolve_path(13){path}}}"""
    return field(1, rid) + field(2, field(13, field(1, path)))


def msg_list_directory(rid, path):
    """ClientMessage{request_id, host_scoped{list_directory(12){path}}}"""
    return field(1, rid) + field(2, field(12, field(1, path)))


def read_varint(b, i):
    n, shift = 0, 0
    while True:
        c = b[i]
        i += 1
        n |= (c & 0x7F) << shift
        if not c & 0x80:
            return n, i
        shift += 7


def request_id_of(b):
    """the top-level string field 1 (request_id) of a ServerMessage, '' when absent (a push); fields are skipped by wire type"""
    i = 0
    while i < len(b):
        tag, i = read_varint(b, i)
        num, wt = tag >> 3, tag & 7
        if wt == 2:
            ln, i = read_varint(b, i)
            if num == 1:
                return b[i:i + ln].decode("utf-8", "replace")
            i += ln
        elif wt == 0:
            _, i = read_varint(b, i)
        elif wt == 1:
            i += 8
        elif wt == 5:
            i += 4
        else:
            return ""
    return ""


def pct(xs, p):
    xs = sorted(xs)
    return xs[min(len(xs) - 1, int(p * len(xs)))] if xs else None


def summary(xs):
    return dict(n=len(xs), p50=round(pct(xs, 0.5), 3), p95=round(pct(xs, 0.95), 3), p99=round(pct(xs, 0.99), 3), max=round(max(xs), 3)) if xs else dict(n=0)


# ---------------------------------------------------------------- the session
class Session:
    def __init__(self, binary, home, key):
        env = dict(os.environ, HOME=home, XDG_RUNTIME_DIR=home, XDG_CONFIG_HOME=home + "/.config", XDG_DATA_HOME=home + "/.local/share", XDG_STATE_HOME=home + "/.local/state")
        self.home = home
        self.t_spawn = time.perf_counter()
        self.p = subprocess.Popen([binary, "remote-server-proxy", "--identity-key", key], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                  stderr=subprocess.DEVNULL, env=env, cwd=home)

    def send(self, msg):
        self.p.stdin.write(frame(msg))
        self.p.stdin.flush()

    def recv(self):
        h = self.p.stdout.read(4)
        if len(h) < 4:
            raise EOFError("the proxy closed its stdout")
        (n,) = struct.unpack("<I", h)
        buf = b""
        while len(buf) < n:
            c = self.p.stdout.read(n - len(buf))
            if not c:
                raise EOFError("short frame")
            buf += c
        return buf

    def wait_for(self, rid, timeout=30):
        t = time.time()
        while time.time() - t < timeout:
            if request_id_of(self.recv()) == rid:
                return time.perf_counter()
        raise TimeoutError("no response for %s within %ss" % (rid, timeout))

    def round_trip(self, msg, rid):
        t0 = time.perf_counter()
        self.send(msg)
        return (self.wait_for(rid) - t0) * 1000.0

    def close(self):
        try:
            self.p.stdin.close()
        except OSError:
            pass
        try:
            self.p.wait(5)
        except subprocess.TimeoutExpired:
            self.p.kill()
        # the daemon the proxy forked: its pid is in the pid file under the scratch HOME; kill exactly that pid
        for root, _d, files in os.walk(self.home):
            for f in files:
                if f.endswith(".pid"):
                    try:
                        os.kill(int(open(os.path.join(root, f)).read().strip()), signal.SIGTERM)
                    except (OSError, ValueError):
                        pass


def measure(binary, n, dir_entries):
    home = tempfile.mkdtemp(prefix="rsbench-")
    os.chmod(home, 0o700)
    big = os.path.join(home, "big")
    os.makedirs(big)
    for i in range(dir_entries):
        open(os.path.join(big, "file_%05d_%s.txt" % (i, "x" * (i % 17))), "w").close()
    key = "rsbench%d" % os.getpid()
    out = {}
    s = Session(binary, home, key)
    try:
        s.send(msg_initialize("init"))
        out["startup_ms"] = round((s.wait_for("init", 60) - s.t_spawn) * 1000.0, 1)
        for i in range(10):                                   # warm up the daemon's allocator and page cache
            s.round_trip(msg_resolve_path("w%d" % i, home), "w%d" % i)
        out["rtt_ms"] = summary([s.round_trip(msg_resolve_path("r%d" % i, home), "r%d" % i) for i in range(n)])
        out["list_ms"] = summary([s.round_trip(msg_list_directory("l%d" % i, big), "l%d" % i) for i in range(max(20, n // 5))])
        k = max(20, n // 5)
        t0 = time.perf_counter()
        for i in range(k):
            s.send(msg_list_directory("b%d" % i, big))
        for i in range(k):
            s.wait_for("b%d" % i)
        out["burst_ms_per_request"] = round((time.perf_counter() - t0) * 1000.0 / k, 3)
    finally:
        s.close()
        shutil.rmtree(home, ignore_errors=True)
    return out


def sizes(binary, tarballs):
    d = dict(binary_bytes=os.path.getsize(binary))
    raw = open(binary, "rb").read()
    d["gzip9_bytes"] = len(gzip.compress(raw, 9))
    for lab, path in tarballs.items():
        if os.path.isfile(path):
            d["tarball_bytes"] = os.path.getsize(path)
    return d


def delta(base, other):
    """other relative to base: size ratios and latency differences (ms) for the keys that exist in both"""
    d = {}
    for k in ("binary_bytes", "gzip9_bytes", "tarball_bytes"):
        if base.get("size", {}).get(k) and other.get("size", {}).get(k):
            d[k + "_ratio"] = round(other["size"][k] / base["size"][k], 3)
    for k in ("rtt_ms", "list_ms"):
        if base.get(k, {}).get("p50") is not None and other.get(k, {}).get("p50") is not None:
            for q in ("p50", "p95", "p99"):
                d["%s_%s_delta" % (k, q)] = round(other[k][q] - base[k][q], 3)
    for k in ("startup_ms", "burst_ms_per_request"):
        if k in base and k in other:
            d[k + "_delta"] = round(other[k] - base[k], 3)
    return d


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", action="append", required=True, metavar="LABEL=PATH", help="a remote-server binary (the `phosphor-oss` of the cli tarball); repeat for before/after")
    ap.add_argument("--tarball", action="append", default=[], metavar="LABEL=PATH", help="the tarball built from that binary (its size is reported as is)")
    ap.add_argument("--n", type=int, default=300)
    ap.add_argument("--dir-entries", type=int, default=3000)
    ap.add_argument("--out", default="")
    a = ap.parse_args(argv)
    bins = dict(x.split("=", 1) for x in a.bin)
    tars = dict(x.split("=", 1) for x in a.tarball)
    res = {}
    for lab, path in bins.items():
        r = dict(size=sizes(path, {lab: tars[lab]} if lab in tars else {}))
        try:
            r.update(measure(path, a.n, a.dir_entries))
        except Exception as e:       # a binary that cannot serve is a result, not a crash of the bench
            r["error"] = "%s: %s" % (type(e).__name__, e)
        res[lab] = r
    labs = list(res)
    if len(labs) > 1:
        res["delta"] = {l: delta(res[labs[0]], res[l]) for l in labs[1:]}
        res["delta_base"] = labs[0]
    txt = json.dumps(res, indent=2, sort_keys=True)
    print(txt)
    if a.out:
        open(a.out, "w").write(txt + "\n")
    return 1 if any("error" in v for k, v in res.items() if isinstance(v, dict) and k in bins) else 0


if __name__ == "__main__":
    sys.exit(main())
