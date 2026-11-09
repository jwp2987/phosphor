# appci/lab.py: client of the fake ssh lab (script/realapp/sshlab, docs/testing.md 2.15) plus its pure pieces (matrix expansion, TOTP, the RTT gate,
# the paired-latency verdict, sshd-log channel counts, the remote-read adapters). No keys, hostnames or IPs live in the repo: the box address comes
# from APPCI_LAB_BOX / SSHLAB_BOX (remote mode), the forced-command key from APPCI_LAB_KEY, and every lab key is generated per run.
#   APPCI_LAB=off|local|remote   default: local when podman exists (the box), else off (the lab cases SKIP with the reason)
#   APPCI_LAB_MATRIX=quick|full|<spec>   which arms of a lab case run (see matrix_selection)
import base64, fnmatch, hashlib, hmac, io, itertools, json, os, re, shutil, socket, statistics, struct, subprocess, sys, tarfile, tempfile, time, urllib.parse, urllib.request, urllib.error

HERE = os.path.dirname(os.path.abspath(__file__))
SSHLAB_DIR = os.path.normpath(os.path.join(HERE, "..", "sshlab"))
USERS_PY = os.path.join(SSHLAB_DIR, "rootfs", "usr", "local", "lib", "sshlab", "usersdb.py")
ALL_SHELLS = ("bash", "zsh", "fish", "nu", "pwsh", "dash", "ash", "rbash")
LATS = {"lan": 0, "150": 150, "300": 300}          # RTT label -> ms
HELPERS = ("nohelper", "helper")                    # nohelper is the default and always the first arm
RTT_TOL_MS = 15.0
NET_FAULTS = ("rst", "blackhole", "stall10")


# ------------------------------------------------------------------ matrix
def lat_label(ms):
    return "lan" if not ms else str(int(ms))


def matrix_selection(value=None):
    """APPCI_LAB_MATRIX -> dict(shells, lats, helpers, faults, scenarios). quick / full / 'shells=a,b;lat=lan,150;helper=nohelper;faults=1;scenarios=0'.
    shells/lats/helpers None = every value the case declares. Unknown keys or values raise ValueError (a typo must not select nothing)."""
    v = (value if value is not None else os.environ.get("APPCI_LAB_MATRIX", "quick")).strip() or "quick"
    if v == "quick":
        return dict(shells={"bash", "zsh", "fish"}, lats={"lan", "150"}, helpers={"nohelper"}, faults=False, scenarios=False)
    if v == "full":
        return dict(shells=None, lats=None, helpers=None, faults=True, scenarios=True)
    sel = dict(shells=None, lats=None, helpers={"nohelper"}, faults=False, scenarios=False)
    for part in v.split(";"):
        if not part.strip():
            continue
        if "=" not in part:
            raise ValueError("bad APPCI_LAB_MATRIX part %r" % part)
        k, val = [x.strip() for x in part.split("=", 1)]
        items = {x.strip() for x in val.split(",") if x.strip()}
        if k == "shells":
            bad = items - set(ALL_SHELLS)
            sel["shells"] = items
        elif k == "lat":
            bad = items - set(LATS)
            sel["lats"] = items
        elif k == "helper":
            bad = items - set(HELPERS)
            sel["helpers"] = items
        elif k in ("faults", "scenarios"):
            bad = set()
            sel[k] = val in ("1", "true", "yes")
        else:
            raise ValueError("unknown APPCI_LAB_MATRIX key %r" % k)
        if bad:
            raise ValueError("unknown value(s) %s for %s" % (sorted(bad), k))
    return sel


def expand_arms(case, shells=("bash",), lats=("lan",), helpers=("nohelper",), faults=(), scenarios=()):
    """every arm a lab case DECLARES (the full matrix): list of dicts {name, case, shell, user, lat_ms, lat, helper, fault, scenario}.
    Names: lab_<case>.<shell|scenario user>.<lat>.<helper>[.<fault>]. Arms are ordered helper-major (so the app relaunches only when the helper mode
    changes), nohelper first; a fault arm runs at LAN and at 150 only."""
    out = []
    hs = sorted(helpers, key=lambda h: HELPERS.index(h))
    for h in hs:
        for who, is_scen in [(s, False) for s in shells] + [(s, True) for s in scenarios]:
            for lat in lats:
                for fault in (None,) + tuple(faults):
                    if fault and (is_scen or lat not in ("lan", "150") or who not in ("bash", "zsh")):
                        continue
                    user = who if is_scen else "u_" + who
                    out.append(dict(name="lab_%s.%s.%s.%s%s" % (case, who, lat, h, "." + fault if fault else ""), case=case, shell="bash" if is_scen else who,
                                    user=user, lat_ms=LATS[lat], lat=lat, helper=h, fault=fault, scenario=is_scen))
    return out


def arm_selected(arm, sel):
    """does the matrix selection `sel` (matrix_selection) want this arm?"""
    if arm["fault"] and not sel["faults"]:
        return False
    if arm["scenario"] and not sel["scenarios"]:
        return False
    if sel["shells"] is not None and not arm["scenario"] and arm["shell"] not in sel["shells"]:
        return False
    if sel["lats"] is not None and arm["lat"] not in sel["lats"]:
        return False
    if sel["helpers"] is not None and arm["helper"] not in sel["helpers"]:
        return False
    return True


def name_matches(patterns, name):
    """--cases accepts exact names and fnmatch globs, comma separated"""
    return any(fnmatch.fnmatchcase(name, p) for p in patterns.split(",") if p)


# ------------------------------------------------------------------ verdict maths
def totp(secret_b32, t=None, step=30, digits=6):
    """RFC 6238 TOTP (HMAC-SHA1), the code pam_google_authenticator expects for the secret labd leased"""
    s = secret_b32.upper()
    key = base64.b32decode(s + "=" * (-len(s) % 8))
    counter = int((time.time() if t is None else t) // step)
    h = hmac.new(key, struct.pack(">Q", counter), hashlib.sha1).digest()
    o = h[-1] & 0x0F
    return str((struct.unpack(">I", h[o:o + 4])[0] & 0x7FFFFFFF) % (10 ** digits)).zfill(digits)


def rtt_gate_verdict(samples_ms, target_ms, tol=RTT_TOL_MS, min_n=5):
    """the per-arm gate: the median measured echo RTT must be target +- tol (LAN: <= tol). Anything else is 'mismatch' = the arm is UNVERIFIABLE (never a pass);
    too few samples is 'no-data'."""
    xs = [x for x in samples_ms if x is not None]
    if len(xs) < min_n:
        return dict(verdict="no-data", median=None, target=target_ms, n=len(xs))
    med = statistics.median(xs)
    ok = med <= tol if not target_ms else abs(med - target_ms) <= tol
    return dict(verdict="ok" if ok else "mismatch", median=round(med, 1), target=target_ms, n=len(xs), tol=tol)


def paired_verdict(app, plain, floor_ms=15.0, pct=0.10, min_n=10):
    """owner decision 2026-11-08: median(app) <= median(plain ssh) + max(15 ms, 10 %) at 150/300 ms (the +-1 ms budget is for LAN key->echo only).
    Samples are interleaved by the caller. -> dict(verdict pass|fail|no-data, app, plain, allowed, over_ms)"""
    a, p = [x for x in app if x is not None], [x for x in plain if x is not None]
    if len(a) < min_n or len(p) < min_n:
        return dict(verdict="no-data", n_app=len(a), n_plain=len(p))
    ma, mp = statistics.median(a), statistics.median(p)
    allowed = mp + max(floor_ms, pct * mp)
    return dict(verdict="pass" if ma <= allowed else "fail", app=round(ma, 1), plain=round(mp, 1), allowed=round(allowed, 1), over_ms=round(ma - allowed, 1))


def count_starts(sshd_log_text, kinds=("command", "shell", "subsystem")):
    """sshd (LogLevel VERBOSE) 'Starting session: command for u from ip port N id K' lines -> {kind: count}; the exec channels the app opened"""
    out = {k: 0 for k in kinds}
    for m in re.finditer(r"Starting session: (\w+)", sshd_log_text):
        if m.group(1) in out:
            out[m.group(1)] += 1
    return out


def clock_offset(samples):
    """samples: [(t_send, remote_now, t_recv)] from /health -> remote_clock - local_clock, using the smallest round trip (NTP-style midpoint)"""
    best = min(samples, key=lambda s: s[2] - s[0])
    return best[1] - (best[0] + best[2]) / 2.0


def known_hosts_line(public, alias="sshlab"):
    f = public.split()
    return "%s %s %s\n" % (alias, f[0], f[1])


def ssh_config_text(host, port22, port2222, users, key, known_hosts, password_users=(), alias="sshlab"):
    """`Host lab-<user>` blocks for the harness's ssh_config (forward slashes: also valid for the Windows clients), plus lab-changedkey (port 2222, host key B)"""
    key, known_hosts = key.replace("\\", "/"), known_hosts.replace("\\", "/")
    common = ("  HostName %s\n  IdentityFile %s\n  IdentitiesOnly yes\n  UserKnownHostsFile %s\n  HostKeyAlias %s\n  StrictHostKeyChecking yes\n  LogLevel ERROR\n"
              % (host, key, known_hosts, alias))
    out = []
    for u in users:
        interactive = u in password_users or u == "p_mfa"
        out.append("Host lab-%s\n  Port %d\n  User %s\n%s  BatchMode %s\n%s\n" % (u, port22, u, common, "no" if interactive else "yes",
                                                                                   "  NumberOfPasswordPrompts 1\n" if interactive else ""))
    out.append("Host lab-changedkey\n  Port %d\n  User u_bash\n%s  BatchMode yes\n" % (port2222, common))
    return "".join(out)


# ------------------------------------------------------------------ remote-read adapters (ctx.remote)
class LocalRemote:
    """today's behaviour: the 'remote' is this machine, its files are prof.rhome / prof.rctl"""

    def __init__(self, prof):
        self.prof = prof

    def lines(self, name):
        for attempt in range(5):
            try:
                return open(os.path.join(self.prof.rhome, name), errors="replace").read().splitlines()
            except FileNotFoundError:
                return []
            except PermissionError:       # Windows: the shell still has the file open for writing
                time.sleep(0.05 * (attempt + 1))
        return []

    def rm(self, *names):
        for n in names:
            try:
                os.remove(os.path.join(self.prof.rhome, n))
            except FileNotFoundError:
                pass


class LabRemote:
    """the lab user's side over HTTP: effect files live in /work/<user>; a relative name that is not there is looked up in the home (`echo x >> ~/f`);
    the shell logs are the ctl dir's rec.log / prompt.log (fish: fish_rec.log)"""

    def __init__(self, lab, user, shell=None):
        self.lab, self.user = lab, user
        self.shell = shell or ("fish" if user == "u_fish" else "bash")

    def _read(self, p):
        try:
            return self.lab.file(p).decode("utf-8", "replace").splitlines()
        except LabError:
            return None

    def lines(self, name):
        if name.startswith("/"):
            return self._read(name) or []
        w = self._read("/work/%s/%s" % (self.user, name))
        return w if w is not None else (self._read("/home/%s/%s" % (self.user, name)) or [])

    def rm(self, *names):
        for n in names:
            for base in ("/work/%s/" % self.user, "/home/%s/" % self.user):
                self.lab.rm_file(n if n.startswith("/") else base + n)

    def read(self, which):
        """rec.log / prompt.log / fish_rec.log of the user (Profile.read(remote=True) goes through here)"""
        return self.lab.ctl(self.user, which)["data"].splitlines()

    def commands(self):
        """[(epoch on the REMOTE clock, line)] the shell received, in order (fish: whole-second epoch)"""
        out = []
        for l in self.read("fish_rec.log" if self.shell == "fish" else "rec.log"):
            p = l.split("\t", 2)
            if len(p) == 3 and p[0] == "C":
                try:
                    out.append((float(p[1]), p[2]))
                except ValueError:
                    out.append((0.0, p[2]))
        return out

    def prompts(self):
        return len([l for l in self.read("prompt.log") if l.startswith("P")])


# ------------------------------------------------------------------ the client
class LabError(Exception):
    pass


def _host_of(box):
    return box.rsplit("@", 1)[-1]


def lab_mode():
    """-> (mode, reason): APPCI_LAB=off|local|remote; unset = local when podman is on PATH, else off"""
    v = os.environ.get("APPCI_LAB", "").strip().lower()
    if v in ("off", "0", "no"):
        return "off", "APPCI_LAB=off"
    if v == "remote":
        if not box_addr():
            return "off", "APPCI_LAB=remote but neither APPCI_LAB_BOX nor SSHLAB_BOX is set"
        if not os.environ.get("APPCI_LAB_KEY"):
            return "off", "APPCI_LAB=remote but APPCI_LAB_KEY (the forced-command key) is not set"
        return "remote", ""
    if v in ("", "local"):
        if shutil.which("podman") and os.name != "nt":
            return "local", ""
        return "off", "no podman on this machine; set APPCI_LAB=remote with APPCI_LAB_BOX and APPCI_LAB_KEY to use the box's lab"
    return "off", "unknown APPCI_LAB=%r" % v


def box_addr():
    return os.environ.get("APPCI_LAB_BOX") or os.environ.get("SSHLAB_BOX") or ""


def sshlab_tar():
    """the client's sshlab tree as a tar (the remote verb hashes it to pick the image tag)"""
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w") as tf:
        tf.add(SSHLAB_DIR, arcname="sshlab", filter=lambda ti: None if (ti.name.endswith(".pyc") or "__pycache__" in ti.name) else ti)
    return buf.getvalue()


class Lab:
    def __init__(self, info, host, mode, key_for_box=None):
        self.info, self.host, self.mode, self.box_key = info, host, mode, key_for_box
        self.cid, self.token = info["cid"], info["token"]
        self.ports = {int(k): v for k, v in info["ports"].items()}
        self.key = None             # the lab user key (private path), set by lease()
        self.known_hosts = None
        self.leased = None
        self.home_cfg = None
        self.stopped = False
        self.offset = None

    @classmethod
    def from_info(cls, info, host, key=None, local=True):
        lab = cls(info, host, "local" if local else "remote")
        lab.key = key
        return lab

    # -- lifecycle
    @classmethod
    def start(cls, run, flavor="full", ttl=3 * 3600):
        """-> (Lab, None) or (None, reason). local: runs labctl.py here; remote: ssh to the box with the forced-command key, the sshlab tree as a tar on stdin"""
        mode, why = lab_mode()
        if mode == "off":
            return None, why
        runid = re.sub(r"[^A-Za-z0-9._-]", "-", os.path.basename(os.path.normpath(run)))[:48] or "run"
        try:
            if mode == "local":
                ctl = os.environ.get("APPCI_LAB_CTL") or os.path.join(SSHLAB_DIR, "labctl.py")
                p = subprocess.run([sys.executable, ctl, "start", "--run", runid, "--flavor", flavor, "--ttl", str(ttl)], capture_output=True, text=True, timeout=300)
                host = "127.0.0.1"
            else:
                box = box_addr()
                p = subprocess.run(["ssh", "-i", os.environ["APPCI_LAB_KEY"], "-o", "BatchMode=yes", "-o", "IdentitiesOnly=yes", "-o", "StrictHostKeyChecking=accept-new",
                                    "-o", "UserKnownHostsFile=" + os.path.join(run, "lab_box_known_hosts"), "-o", "ConnectTimeout=20", box,
                                    "start --run %s --flavor %s --ttl %d" % (runid, flavor, ttl)], input=sshlab_tar(), capture_output=True, timeout=300)
                p.stdout, p.stderr = p.stdout.decode(errors="replace"), p.stderr.decode(errors="replace")
                host = _host_of(box)
            if p.returncode:
                return None, "labctl start failed (%d): %s" % (p.returncode, (p.stderr or p.stdout).strip()[-400:])
            info = json.loads(p.stdout.strip().splitlines()[-1])
        except (OSError, ValueError, subprocess.TimeoutExpired) as e:
            return None, "the lab could not be started: %s" % e
        lab = cls(info, host, mode)
        lab.run = run
        try:
            json.dump({k: info[k] for k in ("cid", "run", "flavor", "image", "expires")} | {"mode": mode}, open(os.path.join(run, "lab.json"), "w"))     # no token, no address
        except OSError:
            pass
        return lab, None

    def stop(self):
        if self.stopped:
            return
        self.stopped = True
        try:
            stop_container(self.cid, self.mode, self.box_key)
        except Exception as e:  # noqa: BLE001
            print("lab: stop failed: %r (labctl gc removes it at its TTL)" % (e,), flush=True)

    # -- HTTP
    def _req(self, method, path, body=None, timeout=30, raw=False):
        data = json.dumps(body).encode() if body is not None else None
        r = urllib.request.Request("http://%s:%d%s" % (self.host, self.ports[7000], path), data=data, method=method,
                                   headers={"Authorization": "Bearer " + self.token, "Content-Type": "application/json"})
        try:
            resp = urllib.request.urlopen(r, timeout=timeout).read()
        except urllib.error.HTTPError as e:
            raise LabError("%s %s -> %d %s" % (method, path, e.code, e.read()[:200].decode(errors="replace")))
        except (urllib.error.URLError, OSError) as e:
            raise LabError("%s %s failed: %s" % (method, path, e))
        return resp if raw else json.loads(resp or b"{}")

    def health(self):
        return self._req("GET", "/health")

    def lease(self, pubkeys=None, hooks=None):
        """generate the run's lab user key (unless given), push keys + the shell hook templates, remember the secrets"""
        if pubkeys is None:
            self.make_key()
            pubkeys = [open(self.key + ".pub").read().strip()]
        if hooks is None:
            import profile
            hooks = profile.lab_hooks()
        self.leased = self._req("POST", "/lease", dict(run=os.path.basename(getattr(self, "run", "")), pubkeys=pubkeys, hooks=hooks), timeout=120)
        return self.leased

    def make_key(self):
        d = os.path.join(getattr(self, "run", tempfile.gettempdir()), "labssh")
        os.makedirs(d, exist_ok=True)
        self.key = os.path.join(d, "labkey")
        if not os.path.exists(self.key):
            subprocess.run(["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", self.key], check=True, stdin=subprocess.DEVNULL)
        if os.name == "nt":
            user = os.environ.get("USERNAME", "runner")
            for f in (self.key,):
                subprocess.run(["icacls", f, "/inheritance:r", "/grant:r", "%s:F" % user], capture_output=True)
        return self.key

    def fault(self, rtt_ms=0, jitter_ms=0, loss_pct=0, rate_kbit=0):
        return self._req("POST", "/fault", dict(rtt_ms=rtt_ms, jitter_ms=jitter_ms, loss_pct=loss_pct, rate_kbit=rate_kbit))

    def clear_fault(self):
        return self._req("DELETE", "/fault")

    def drop(self, user, kind, seconds=10):
        return self._req("POST", "/drop", dict(user=user, kind=kind, seconds=seconds))

    def file(self, path):
        return self._req("GET", "/file?path=" + urllib.parse.quote(path), raw=True)

    def rm_file(self, path):
        try:
            self._req("DELETE", "/file?path=" + urllib.parse.quote(path))
        except LabError:
            pass

    def ctl(self, user, name, since=0):
        return self._req("GET", "/ctl?user=%s&name=%s&since=%d" % (user, name, since))

    def tree(self, user, files, dir="", git=False, branches=()):
        return self._req("PUT", "/tree", dict(user=user, dir=dir, files={k: base64.b64encode(v if isinstance(v, bytes) else v.encode()).decode() for k, v in files.items()},
                                              git=git, branches=list(branches)))

    def helper(self, user, files=None):
        if files is None:
            return self._req("DELETE", "/helper", dict(user=user))
        return self._req("POST", "/helper", dict(user=user, files={k: base64.b64encode(v).decode() for k, v in files.items()}))

    def complete(self, user, line):
        return self._req("POST", "/complete", dict(user=user, line=line))

    def sshd_since(self, since=0, port=22):
        return self._req("GET", "/sshdlog?since=%d&port=%d" % (since, port))

    def clock_offset(self, n=5):
        samples = []
        for _ in range(n):
            t0 = time.time()
            h = self.health()
            samples.append((t0, h.get("now", t0), time.time()))
        self.offset = clock_offset(samples)
        return self.offset

    # -- ssh
    def write_ssh(self, home):
        """write <home>/.ssh/lab_known_hosts and append the Host lab-* blocks to <home>/.ssh/config (after prof.build wrote its own). Returns the config text."""
        if not self.leased:
            raise LabError("lease first")
        d = os.path.join(home, ".ssh")
        os.makedirs(d, exist_ok=True)
        kh = os.path.join(d, "lab_known_hosts")
        open(kh, "w").write(known_hosts_line(self.leased["hostkeys"]["A"]["public"]))
        text = ssh_config_text(self.host, self.ports[22], self.ports[2222], [u["user"] for u in self.leased["users"]], self.key, kh, self.leased.get("password_users", ()))
        with open(os.path.join(d, "config"), "a", newline="\n") as f:
            f.write("\n" + text)
        self.home_cfg = os.path.join(d, "config")
        if os.name == "nt":
            subprocess.run(["icacls", self.home_cfg, "/inheritance:r", "/grant:r", "%s:F" % os.environ.get("USERNAME", "runner")], capture_output=True)
        return text

    def ssh(self, user, cmd, key=None, timeout=30):
        """a one-shot ssh as the lab user (BatchMode, no tty); used by selftest and helpers"""
        kh = os.path.join(tempfile.gettempdir(), "sshlab_kh_%d" % self.ports[22])
        open(kh, "w").write(known_hosts_line(self.leased["hostkeys"]["A"]["public"]) if self.leased else "")
        argv = ["ssh", "-i", key or self.key, "-p", str(self.ports[22]), "-o", "IdentitiesOnly=yes", "-o", "BatchMode=yes", "-o", "UserKnownHostsFile=" + kh,
                "-o", "HostKeyAlias=sshlab", "-o", "StrictHostKeyChecking=yes", "-o", "LogLevel=ERROR", "-o", "ConnectTimeout=15", "%s@%s" % (user, self.host), cmd]
        return subprocess.run(argv, capture_output=True, text=True, timeout=timeout, stdin=subprocess.DEVNULL)

    def measure_rtt(self, n=20, port=7):
        """echo-probe RTTs (ms) to the container's port 7 through the published port: the per-arm gate's evidence. The first sample (connect) is dropped."""
        out = []
        try:
            s = socket.create_connection((self.host, self.ports[port]), 5)
            s.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
            s.settimeout(5)
            for i in range(n + 1):
                t0 = time.perf_counter()
                s.sendall(b"x")
                s.recv(1)
                if i:
                    out.append((time.perf_counter() - t0) * 1000.0)
                time.sleep(0.02)
            s.close()
        except OSError:
            pass
        return out

    def diag(self):
        """tails for ssh_diag(): /health and the last sshd log lines"""
        out = []
        try:
            h = self.health()
            out.append("lab /health: sshd=%s fault=%s tc=%s ttl_left=%s" % (h.get("sshd"), h.get("fault"), h.get("tc"), h.get("ttl_left")))
        except LabError as e:
            out.append("lab /health failed: %s" % e)
        try:
            d = self.sshd_since(0)
            out.append("lab sshd.log tail: " + " | ".join([l for l in d["data"].splitlines() if l.strip()][-8:]))
        except LabError as e:
            out.append("lab /sshdlog failed: %s" % e)
        return " ;; ".join(out)


def stop_container(cid, mode, box_key=None):
    if mode == "local":
        ctl = os.environ.get("APPCI_LAB_CTL") or os.path.join(SSHLAB_DIR, "labctl.py")
        subprocess.run([sys.executable, ctl, "stop", "--id", cid], capture_output=True, timeout=60)
    else:
        subprocess.run(["ssh", "-i", os.environ["APPCI_LAB_KEY"], "-o", "BatchMode=yes", "-o", "IdentitiesOnly=yes", "-o", "StrictHostKeyChecking=accept-new",
                        "-o", "UserKnownHostsFile=" + os.path.join(tempfile.gettempdir(), "lab_box_known_hosts"), box_addr(), "stop --id " + cid],
                       capture_output=True, timeout=60, stdin=subprocess.DEVNULL)


def stop_from_json(run):
    """teardown.sh / the run.py finally: stop the container named by <run>/lab.json (exact id; no token needed). True when something was asked to stop."""
    try:
        j = json.load(open(os.path.join(run, "lab.json")))
    except (OSError, ValueError):
        return False
    stop_container(j["cid"], j.get("mode", "local"))
    return True


# ------------------------------------------------------------------ selftest helpers (labctl selftest)
def mfa_login_ok(lab, key):
    """publickey + keyboard-interactive TOTP login as p_mfa (SSH_ASKPASS supplies the code)"""
    d = tempfile.mkdtemp(prefix="sshlab-askpass-")
    try:
        ap = os.path.join(d, "ask.sh")
        open(ap, "w").write("#!/bin/sh\necho %s\n" % totp(lab.leased["mfa_secret"]))
        os.chmod(ap, 0o700)
        kh = os.path.join(d, "kh")
        open(kh, "w").write(known_hosts_line(lab.leased["hostkeys"]["A"]["public"]))
        env = dict(os.environ, SSH_ASKPASS=ap, SSH_ASKPASS_REQUIRE="force", DISPLAY=os.environ.get("DISPLAY", "none"))
        p = subprocess.run(["ssh", "-i", key, "-p", str(lab.ports[22]), "-o", "IdentitiesOnly=yes", "-o", "UserKnownHostsFile=" + kh, "-o", "HostKeyAlias=sshlab",
                            "-o", "NumberOfPasswordPrompts=1", "-o", "LogLevel=ERROR", "p_mfa@" + lab.host, "echo MFA_OK"], env=env, capture_output=True, text=True,
                           timeout=60, stdin=subprocess.DEVNULL)
        return "MFA_OK" in p.stdout
    finally:
        shutil.rmtree(d, ignore_errors=True)


def onesess_refuses(lab, key, tmp):
    """p_onesess has MaxSessions 1: through one ControlMaster connection the second session must be refused"""
    cp = os.path.join(tmp, "cm_onesess")
    kh = os.path.join(tmp, "kh_onesess")
    open(kh, "w").write(known_hosts_line(lab.leased["hostkeys"]["A"]["public"]))
    base = ["ssh", "-i", key, "-p", str(lab.ports[22]), "-o", "IdentitiesOnly=yes", "-o", "UserKnownHostsFile=" + kh, "-o", "HostKeyAlias=sshlab", "-o", "BatchMode=yes",
            "-o", "LogLevel=ERROR", "-o", "ControlPath=" + cp]
    m = subprocess.Popen(base + ["-o", "ControlMaster=yes", "p_onesess@" + lab.host, "sleep 20"], stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        for _ in range(50):
            if os.path.exists(cp):
                break
            time.sleep(0.2)
        p = subprocess.run(base + ["-o", "ControlMaster=no", "p_onesess@" + lab.host, "echo SECOND"], capture_output=True, text=True, timeout=30, stdin=subprocess.DEVNULL)
        return "SECOND" not in p.stdout
    finally:
        m.kill()


# ------------------------------------------------------------------ plain ssh control (POSIX): the paired baseline
class PlainSsh:
    """`ssh -tt -o ControlMaster=no -F <cfg> lab-<user>` in a pty, driven like a person: per-key echo times and Enter -> record times. Nothing of Phosphor
    is involved, so it measures the path + the remote shell only. POSIX only (None from open() elsewhere: the paired verdict is then UNVERIFIABLE)."""

    def __init__(self, cfg, user, env=None):
        self.cfg, self.user, self.fd, self.pid, self.buf = cfg, user, None, None, bytearray()
        self.env = env or {}

    def open(self, timeout=30):
        if os.name == "nt" or not shutil.which("ssh"):
            return False
        import pty, fcntl, termios
        pid, fd = pty.fork()
        if pid == 0:
            try:
                os.execvpe("ssh", ["ssh", "-tt", "-o", "ControlMaster=no", "-o", "ControlPath=none", "-F", self.cfg, "lab-" + self.user], dict(os.environ, TERM="xterm-256color", **self.env))
            finally:
                os._exit(127)
        self.pid, self.fd = pid, fd
        fcntl.fcntl(fd, fcntl.F_SETFL, fcntl.fcntl(fd, fcntl.F_GETFL) | os.O_NONBLOCK)
        end = time.perf_counter() + timeout
        while time.perf_counter() < end:          # the first prompt is "whatever the shell prints, then quiet for 0.7 s" (bash $, zsh %, fish >, nu, pwsh)
            n = len(self.buf)
            self._pump(0.7)
            if len(self.buf) == n and n:
                return True
        return False

    def _pump(self, dur):
        import select
        end = time.perf_counter() + dur
        while True:
            left = end - time.perf_counter()
            if left <= 0:
                return
            r, _, _ = select.select([self.fd], [], [], min(left, 0.01))
            if r:
                try:
                    d = os.read(self.fd, 65536)
                except (BlockingIOError, OSError):
                    d = b""
                if d:
                    self.buf += d

    def wait_for(self, needle, timeout, start=0):
        """seconds (perf_counter timestamp) when `needle` first appears in the output after offset `start`, else None"""
        end = time.perf_counter() + timeout
        while time.perf_counter() < end:
            if needle in self.buf[start:]:
                return time.perf_counter()
            self._pump(0.005)
        return None

    def key_echo(self, ch=b"q", timeout=5):
        """send one key, return the ms until it is echoed back (the shell's own echo through the path)"""
        start = len(self.buf)
        t0 = time.perf_counter()
        os.write(self.fd, ch)
        t1 = self.wait_for(ch, timeout, start)
        os.write(self.fd, b"\x15")      # Ctrl-U: clear the line
        self._pump(0.05)
        return None if t1 is None else (t1 - t0) * 1000.0

    def run_line(self, text, gap_ms=(10, 25), rng=None):
        """type `text` at 10-25 ms/key and Enter at once; returns the wall time (time.time()) of the Enter write: the caller reads the record time from the remote rec.log"""
        import random
        rng = rng or random.Random(1)
        for ch in text.encode():
            os.write(self.fd, bytes([ch]))
            self._pump(rng.uniform(*gap_ms) / 1000.0)
        t = time.time()
        os.write(self.fd, b"\r")
        return t

    def close(self):
        import signal
        try:
            os.kill(self.pid, signal.SIGKILL)
            os.waitpid(self.pid, 0)
        except OSError:
            pass
        try:
            os.close(self.fd)
        except OSError:
            pass


if __name__ == "__main__":
    # lab.py stop <run dir>: teardown.sh / after_job.ps1 stop the container named by <run dir>/lab.json (exact id)
    if len(sys.argv) == 3 and sys.argv[1] == "stop":
        print("lab stop: %s" % ("asked to stop the container in lab.json" if stop_from_json(sys.argv[2]) else "no lab.json in %s (nothing to stop)" % sys.argv[2]))
    else:
        sys.exit("usage: lab.py stop <run dir>")
