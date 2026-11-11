# appci/nettrace.py (r14-test-gaps T41, #2076): a per-process network trace for the platforms that have no strace.
# The operating system's own connection table is polled for a set of pids:
#   macOS    `lsof -nP -i -a -p <pids>`     (the runner user's own processes; no root)
#   Windows  `netstat -ano`                  (every connection with its owning pid)
# A row is an OUTBOUND fact when its remote address is neither loopback nor unspecified. What a poll cannot see, and the cases say so in
# their result: a connection that opens and closes between two polls, a UDP datagram sent without connect(), and DNS (the resolver service
# makes the query, not the traced process). Linux keeps `strace -f` (cases_netless.outbound_attempts), which sees all three.
# The parsers are pure (selftest_r15_w5a.py); control() proves on the machine that the tool lists a real connection of a real pid.
import os, re, socket, subprocess, sys, threading, time

POLL_S = 0.25
LOCAL_RX = re.compile(r"^(127\.|0\.0\.0\.0$|::1$|::$|\*$|::ffff:127\.|fe80:)", re.I)     # loopback, unspecified, link-local scope


def split_host_port(s):
    """'10.0.0.5:443' / '[::1]:22' / '[fe80::1%en0]:5353' / '*:*' -> (host, port or None)"""
    s = s.strip()
    if s.startswith("["):
        host, _, rest = s[1:].partition("]")
        port = rest[1:] if rest.startswith(":") else ""
    else:
        host, _, port = s.rpartition(":")
        if not host:
            host, port = s, ""
    host = host.split("%", 1)[0]
    return host, (int(port) if port.isdigit() else None)


def is_local(addr):
    return bool(LOCAL_RX.match(addr or ""))


# ---------------------------------------------------------------- Windows: netstat -ano
# "  TCP    10.0.2.15:49712        93.184.216.34:443      ESTABLISHED     5120"
# "  TCP    [::1]:49670            [::1]:49671            ESTABLISHED     900"
# "  UDP    0.0.0.0:5353           *:*                                    2040"      (no state column)
NETSTAT_ROW = re.compile(r"^\s*(TCP|UDP)\s+(\S+)\s+(\S+)\s+(?:(\S+)\s+)?(\d+)\s*$")


def parse_netstat(text):
    """-> [(pid, proto, remote host, remote port, state)] of every `netstat -ano` row that has a remote address. Listening sockets (remote
    0.0.0.0:0 / [::]:0 / *:*) have none and are left out; the state word is localised and never used for that."""
    out = []
    for line in text.splitlines():
        m = NETSTAT_ROW.match(line)
        if not m:
            continue
        proto, _local, remote, state, pid = m.groups()
        host, port = split_host_port(remote)
        if host in ("*", "0.0.0.0", "::") and not port:
            continue
        out.append((int(pid), proto, host, port or 0, state or ""))
    return out


# ---------------------------------------------------------------- macOS: lsof -nP -i -a -p <pids>
# "phosphor  1234 user   23u  IPv4 0x1a2b      0t0  TCP 192.168.1.5:52100->93.184.216.34:443 (ESTABLISHED)"
# "phosphor  1234 user   24u  IPv6 0x3c4d      0t0  UDP [fe80::1%en0]:55555->[fe80::2%en0]:5353"
# "phosphor  1234 user   25u  IPv4 0x5e6f      0t0  TCP *:8080 (LISTEN)"                                   (no remote: left out)
LSOF_ROW = re.compile(r"^\S.*?\s+(\d+)\s+\S+\s+\S+\s+IPv[46]\s+.*?\b(TCP|UDP)\s+(\S+?)->(\S+?)(?:\s+\((\w+)\))?\s*$")


def parse_lsof(text):
    """-> [(pid, proto, remote host, remote port, state)] of every `lsof -nP -i` row that names a remote end (`local->remote`)"""
    out = []
    for line in text.splitlines():
        m = LSOF_ROW.match(line)
        if not m:
            continue
        pid, proto, _local, remote, state = m.groups()
        host, port = split_host_port(remote)
        out.append((int(pid), proto, host, port or 0, state or ""))
    return out


def parse_ps_tree(text):
    """`ps -axo pid=,ppid=` -> {pid: ppid}"""
    tree = {}
    for line in text.splitlines():
        p = line.split()
        if len(p) >= 2 and p[0].isdigit() and p[1].isdigit():
            tree[int(p[0])] = int(p[1])
    return tree


def descendants(tree, roots):
    """roots plus every pid whose parent chain reaches one of them ({pid: ppid})"""
    want = set(int(r) for r in roots if r)
    grew = True
    while grew:
        grew = False
        for pid, ppid in tree.items():
            if ppid in want and pid not in want:
                want.add(pid)
                grew = True
    return want


def outbound(rows, allow=()):
    """rows of a sampler whose remote address is not loopback / unspecified / link-local and not in `allow`"""
    return [r for r in rows if not is_local(r[2]) and r[2] not in allow]


# ---------------------------------------------------------------- samplers (one call = one look at the table)
def run(argv, timeout=20):
    try:
        p = subprocess.run(argv, capture_output=True, timeout=timeout)
        return p.stdout.decode("utf-8", "replace")
    except Exception:
        return ""


def sample_mac(pids):
    """every connection row of `pids` (lsof exits 1 when it lists nothing: not an error)"""
    pids = sorted(int(p) for p in pids if p)
    if not pids:
        return []
    return parse_lsof(run(["lsof", "-nP", "-i", "-a", "-p", ",".join(str(p) for p in pids)]))


def sample_windows(pids):
    pids = set(int(p) for p in pids if p)
    if not pids:
        return []
    return [r for r in parse_netstat(run(["netstat", "-ano"])) if r[0] in pids]


def mac_pids(roots):
    return descendants(parse_ps_tree(run(["ps", "-axo", "pid=,ppid="])), roots)


def windows_tree(run_text=None):
    """{pid: ppid} from `wmic`-free PowerShell (one process per call is slow, so cases prefer the app's job object: winapi.job_pids)"""
    text = run_text if run_text is not None else run(["powershell", "-NoProfile", "-Command",
                                                      "Get-CimInstance Win32_Process | ForEach-Object { \"$($_.ProcessId) $($_.ParentProcessId)\" }"], timeout=60)
    return parse_ps_tree(text)


def platform_kind(ctx):
    """'mac' | 'windows' | None (Linux and the sim backend: no polled trace)"""
    if getattr(ctx, "win", False):
        return "windows"
    if getattr(getattr(ctx, "be", None), "name", "") == "mac":
        return "mac"
    return None


TOOL = {"mac": "lsof -nP -i -a -p <pid>", "windows": "netstat -ano"}


def app_pids(ctx):
    """the pids whose connections belong to the app under test: macOS, every process inside the job's bundle and their descendants (the pane
    shells hang off the bundle's terminal-server helper); Windows, the members of the app's job object, else the app pid and its descendants"""
    app = ctx.app
    if platform_kind(ctx) == "mac":
        roots = [p for p, _ in app._instances()] if hasattr(app, "_instances") else [app.pid()]
        return mac_pids(roots)
    job = getattr(app, "job", None)
    if job:
        try:
            import winapi
            got = set(winapi.job_pids(job))
            if got:
                return got
        except Exception:
            pass
    root = app.pid()
    return descendants(windows_tree(), [root]) if root else set()


class ConnPoller(threading.Thread):
    """polls `sampler(pids_fn())` every POLL_S and keeps every row it ever saw with the time it was first seen: seen = {(pid, proto, host,
    port): first time}. pids_fn is called on every tick (the app's process set changes: a relaunch, new shells) and may return an empty set."""

    def __init__(self, sampler, pids_fn, interval=POLL_S):
        super().__init__(daemon=True)
        self.sampler, self.pids_fn, self.interval = sampler, pids_fn, interval
        self.seen, self.ticks, self.errors = {}, 0, 0
        self._halt = threading.Event()

    def tick(self):
        try:
            rows = self.sampler(self.pids_fn())
        except Exception:
            self.errors += 1
            return
        now = time.time()
        self.ticks += 1
        for pid, proto, host, port, _state in rows:
            self.seen.setdefault((pid, proto, host, port), now)

    def run(self):
        while not self._halt.is_set():
            self.tick()
            self._halt.wait(self.interval)

    def stop(self):
        self._halt.set()
        self.join(timeout=30)
        self.tick()                       # one last look after the loop
        return self.seen

    def outbound_between(self, t0, t1, allow=()):
        """[(proto, host, port, first seen)] of non-local rows first seen in [t0, t1), oldest first"""
        rows = [(proto, host, port, t) for (_pid, proto, host, port), t in self.seen.items() if t0 <= t < t1 and not is_local(host) and host not in allow]
        return sorted(rows, key=lambda r: r[3])


def own_address():
    """this machine's address on its default route, or None when it has none (a UDP connect sends nothing; it only picks the source address)"""
    for target in ("192.0.2.1", "2001:db8::1"):
        fam = socket.AF_INET6 if ":" in target else socket.AF_INET
        try:
            with socket.socket(fam, socket.SOCK_DGRAM) as s:
                s.connect((target, 9))
                a = s.getsockname()[0]
                if a and not is_local(a):
                    return a
        except OSError:
            continue
    return None


CONTROL_CHILD = ("import socket,sys,time\n"
                 "s=socket.create_connection((sys.argv[1],int(sys.argv[2])),timeout=10)\n"
                 "sys.stdout.write('up\\n');sys.stdout.flush()\n"
                 "time.sleep(float(sys.argv[3]))\n")


def control(sampler, hold_s=4.0, addr=None):
    """Prove on THIS machine that `sampler` lists a real connection of a real pid: a child process connects to a listener on this machine's own
    non-loopback address and holds the connection; the sampler, asked about that pid, must list that address and port.
    -> (ok, why). ok is None when the machine has no non-loopback address (nothing to prove the tool with)."""
    addr = addr or own_address()
    if not addr:
        return None, "this machine has no non-loopback address, so no control connection can be made"
    fam = socket.AF_INET6 if ":" in addr else socket.AF_INET
    srv = socket.socket(fam, socket.SOCK_STREAM)
    try:
        srv.bind((addr, 0))
        srv.listen(1)
        port = srv.getsockname()[1]
        child = subprocess.Popen([sys.executable, "-c", CONTROL_CHILD, addr, str(port), str(hold_s)], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        try:
            srv.settimeout(10)
            try:
                conn, _ = srv.accept()
            except OSError as e:
                return False, "the control child could not connect to %s (%s)" % ("this machine's own address", e)
            child.stdout.readline()
            seen, t_end = [], time.time() + hold_s
            while time.time() < t_end and not seen:
                seen = [r for r in sampler({child.pid}) if r[0] == child.pid and r[2] == addr and r[3] == port]
                if not seen:
                    time.sleep(0.2)
            conn.close()
            return (True, "listed") if seen else (False, "the tool did not list the control child's connection to this machine's own address")
        finally:
            try:
                child.kill()
                child.wait(timeout=5)
            except Exception:
                pass
    except OSError as e:
        return False, "could not open the control listener (%s)" % e
    finally:
        srv.close()
