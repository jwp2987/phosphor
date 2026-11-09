#!/usr/bin/env python3
# labd.py: the sshlab container's daemon (docs/testing.md 2.15). Runs under an init (podman --init), supervises two sshds (:22 host key A, :2222 host
# key B), an echo probe on :7 and a stdlib HTTP control server on :7000 (bearer token from $SSHLAB_TOKEN). Exits after its TTL ($SSHLAB_TTL_S, default 8 h).
# Import-safe: selftest loads this file by path for its pure helpers (session_pids, safe_path, count_matches...); nothing runs unless __main__.
import base64, hashlib, http.server, json, os, pwd, re, secrets, shutil, signal, socket, socketserver, subprocess, sys, threading, time, urllib.parse

HERE = os.path.dirname(os.path.abspath(__file__))
for _d in ("/usr/local/lib/sshlab", os.path.join(HERE, "..", "lib", "sshlab")):
    if os.path.isdir(_d) and _d not in sys.path:
        sys.path.insert(0, _d)
import usersdb, netem  # noqa: E402

RUN = os.environ.get("SSHLAB_RUN_DIR", "/run/sshlab")
CTL_ROOT = "/var/lib/appci/ctl"
WORK_ROOT = "/work"
HOME_ROOT = "/home"
SSHD_LOG = {22: RUN + "/sshd_22.log", 2222: RUN + "/sshd_2222.log"}
VERSION = 1
LOCK = threading.RLock()
STATE = dict(started=time.time(), ttl=float(os.environ.get("SSHLAB_TTL_S", str(8 * 3600))), run=None, fault=None, tc_ok=None, users=[], secrets={},
             hostkeys={}, procs={}, uids=[], leased=False)


# ------------------------------------------------------------------ pure helpers
def safe_path(path, roots=(WORK_ROOT, HOME_ROOT, CTL_ROOT)):
    """normalized absolute path if it lies under one of `roots` (no `..` escape, no symlink escape when the path exists), else None"""
    if not path or "\0" in path:
        return None
    p = os.path.normpath(path)
    if not os.path.isabs(p):
        return None
    for r in roots:
        if p == r or p.startswith(r + "/"):
            real = os.path.realpath(p)
            if real == r or real.startswith(os.path.realpath(r) + "/"):
                return p
    return None


def session_pids(ps_text, user):
    """pids of the sshd session processes of `user` from `ps -eo pid=,args=` text: titles 'sshd-session: user [priv]' / 'sshd-session: user@pts/0' (or older 'sshd: ...')"""
    out = []
    pat = re.compile(r"^\s*(\d+)\s+(?:sshd-session|sshd): %s(?:@\S+| \[priv\])?\s*$" % re.escape(user))
    for line in ps_text.splitlines():
        m = pat.match(line)
        if m:
            out.append(int(m.group(1)))
    return out


def parse_ss(ss_text):
    """`ss -tnH state established` lines ('Recv-Q Send-Q Local:port Peer:port', no State column with a state filter) -> [(client_ip, client_port, server_port)].
    No pids: sshd is non-dumpable and the container has no SYS_PTRACE, so connections are matched to users through the sshd log (accepted_ports)."""
    out = []
    for line in ss_text.splitlines():
        f = line.split()
        if len(f) < 4 or not f[0].isdigit():
            continue
        try:
            lip, lport = f[2].rsplit(":", 1)
            rip, rport = f[3].rsplit(":", 1)
            out.append((rip.strip("[]"), int(rport), int(lport)))
        except ValueError:
            pass
    return out


def accepted_ports(sshd_log, user):
    """client (ip, port) pairs of 'Accepted <method> for <user> from <ip> port <n>' lines in the sshd log (LogLevel VERBOSE)"""
    pat = re.compile(r"Accepted \S+ for %s from (\S+) port (\d+)" % re.escape(user))
    return [(m.group(1), int(m.group(2))) for m in pat.finditer(sshd_log)]


def totp_file_text(secret_b32):
    return secret_b32 + '\n" TOTP_AUTH\n'


def rc_plan(row):
    """{relative path under the home: text} for a user's shell startup files. The hook and the scenario fragment are sourced from them. Pure."""
    u, sh, prof = row.user, row.shell, row.profile
    hook = "/etc/sshlab/hooks/hook.%s.%s" % (hook_shell_key(sh), u)
    scen = "/usr/local/lib/sshlab/scenarios/%s.sh" % prof
    bash_rc = '[ -r "%s" ] && . "%s"\n[ -r "%s" ] && . "%s"\n' % (hook, hook, scen, scen)
    plan = {}
    if sh in ("bash", "rbash"):
        plan[".bashrc"] = bash_rc
        plan[".bash_profile"] = ('exec zsh -l\n' if prof == "exec" else '') + '[ -f ~/.bashrc ] && . ~/.bashrc\n'
        if prof == "exec":
            hz = "/etc/sshlab/hooks/hook.zsh.%s" % u
            plan[".zshrc"] = '[ -r "%s" ] && . "%s"\n' % (hz, hz)
    elif sh == "zsh":
        plan[".zshrc"] = '. "%s"\n' % hook
    elif sh == "fish":
        plan[".config/fish/config.fish"] = 'source %s\n' % hook
    elif sh == "nu":
        plan[".config/nushell/env.nu"] = ""
        plan[".config/nushell/config.nu"] = 'source %s\n' % hook      # nu: `source` needs a parse-time constant path: the per-user hook path is one
    elif sh == "pwsh":
        plan[".config/powershell/Microsoft.PowerShell_profile.ps1"] = '. "%s"\n' % hook
    elif sh in ("dash", "ash"):
        plan[".profile"] = 'ENV="%s"; export ENV\n. "%s"\n' % (hook, hook)
    return plan


def hook_shell_key(shell):
    return "sh" if shell in ("dash", "ash") else shell


def count_matches(text, pattern):
    return len(re.findall(pattern, text))


# ------------------------------------------------------------------ system actions (container only)
def sh(argv, check=False, timeout=30, input=None, user=None, cwd=None, env=None):
    if user:
        argv = ["runuser", "-u", user, "--"] + list(argv)
    p = subprocess.run(argv, capture_output=True, text=True, timeout=timeout, input=input, cwd=cwd, env=env)
    if check and p.returncode:
        raise RuntimeError("%s failed (%d): %s" % (argv[:3], p.returncode, (p.stderr or p.stdout).strip()[:300]))
    return p


def uid_of(user):
    return pwd.getpwnam(user).pw_uid


def gen_hostkeys():
    for tag in ("A", "B"):
        d = "%s/hostkeys/%s" % (RUN, tag)
        os.makedirs(d, exist_ok=True)
        k = d + "/ssh_host_ed25519_key"
        if not os.path.exists(k):
            sh(["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", k], check=True)
        pub = open(k + ".pub").read().strip()
        fp = sh(["ssh-keygen", "-lf", k + ".pub"]).stdout.strip()
        STATE["hostkeys"][tag] = dict(public=pub, fingerprint=fp)


def sshd_config(port, tag, rows):
    return "\n".join([
        "Port %d" % port, "ListenAddress 0.0.0.0", "HostKey %s/hostkeys/%s/ssh_host_ed25519_key" % (RUN, tag), "PidFile %s/sshd_%d.pid" % (RUN, port),
        "LogLevel VERBOSE", "UsePAM yes", "PasswordAuthentication no", "KbdInteractiveAuthentication no", "PubkeyAuthentication yes",
        "AuthorizedKeysFile %s/authkeys/%%u" % RUN, "StrictModes no", "PermitTTY yes", "X11Forwarding no", "PrintMotd no", "PermitUserEnvironment no",
        "AcceptEnv LANG LC_*", "MaxSessions 10", "MaxAuthTries 6", "LoginGraceTime 30",
        "Subsystem sftp /usr/lib/openssh/sftp-server" if os.path.exists("/usr/lib/openssh/sftp-server") else "", ""]) + "\n" + usersdb.match_blocks(rows)


def start_sshds(rows):
    os.makedirs(RUN + "/authkeys", exist_ok=True)
    os.makedirs("/run/sshd", exist_ok=True)
    for port, tag in ((22, "A"), (2222, "B")):
        conf = "%s/sshd_%d.conf" % (RUN, port)
        open(conf, "w").write(sshd_config(port, tag, rows))
        sh(["/usr/sbin/sshd", "-t", "-f", conf], check=True)
        log = open(SSHD_LOG[port], "ab")
        STATE["procs"][port] = subprocess.Popen(["/usr/sbin/sshd", "-D", "-e", "-f", conf], stdout=log, stderr=log, start_new_session=True)


def supervise():
    rows = STATE["rows"]
    while True:
        time.sleep(2)
        if time.time() - STATE["started"] > STATE["ttl"]:
            print("labd: TTL reached, exiting", flush=True)
            os._exit(0)
        for port, p in list(STATE["procs"].items()):
            if p.poll() is not None:
                print("labd: sshd :%d exited rc=%s, restarting" % (port, p.returncode), flush=True)
                tag = "A" if port == 22 else "B"
                conf = "%s/sshd_%d.conf" % (RUN, port)
                STATE["procs"][port] = subprocess.Popen(["/usr/sbin/sshd", "-D", "-e", "-f", conf], stdout=open(SSHD_LOG[port], "ab"), stderr=subprocess.STDOUT, start_new_session=True)


def echo_server():
    class H(socketserver.BaseRequestHandler):
        def handle(self):
            try:
                self.request.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
                while True:
                    d = self.request.recv(4096)
                    if not d:
                        return
                    self.request.sendall(d)
            except OSError:
                pass

    class S(socketserver.ThreadingTCPServer):
        allow_reuse_address = True
        daemon_threads = True
    s = S(("0.0.0.0", 7), H)
    threading.Thread(target=s.serve_forever, daemon=True).start()


def apply_fault(spec):
    cmds = netem.apply_commands(spec)
    n_clear = len(netem.clear_commands())
    for i, c in enumerate(cmds):
        p = sh(c)
        if p.returncode and i >= n_clear:
            for cc in netem.clear_commands():
                sh(cc)
            raise RuntimeError("fault command failed: %s: %s" % (" ".join(c), (p.stderr or "").strip()[:200]))
    STATE["fault"] = None if netem.is_noop(spec) else netem.normalize(spec)
    return STATE["fault"]


def probe_tc():
    """startup probe: do tc + ifb work in this netns? (a tiny delay installed and removed). STATE['tc_ok'] = True | error text"""
    try:
        apply_fault({"rtt_ms": 2})
        apply_fault({})
        STATE["tc_ok"] = True
    except Exception as e:
        STATE["tc_ok"] = "tc/ifb unavailable: %s (fallback: the owner adds a modules-load.d file for sch_netem sch_prio cls_u32 sch_ingress act_mirred ifb; docs/testing.md 2.15)" % e


def block_outbound(rows):
    uids = []
    for r in rows:
        try:
            uids.append(uid_of(r.user))
        except KeyError:
            pass
    STATE["uids"] = uids
    p = sh(["nft", "-f", "-"], input=netem.nft_block_outbound(uids))
    return p.returncode == 0 or (p.stderr or "").strip()[:200]


def write_home(row, hooks, home_mode_override=None):
    u = row.user
    prof = usersdb.PROFILES[row.profile]
    home, work, ctl = "%s/%s" % (HOME_ROOT, u), "%s/%s" % (WORK_ROOT, u), "%s/%s" % (CTL_ROOT, u)
    for d in (home, work, ctl):
        shutil.rmtree(d, ignore_errors=True)
    uid = uid_of(u)
    gid = pwd.getpwnam(u).pw_gid
    for d in (work, ctl):
        os.makedirs(d)
        os.chown(d, uid, gid)
    for f in ("rec.log", "prompt.log", "fish_rec.log"):
        open("%s/%s" % (ctl, f), "w").close()
        os.chown("%s/%s" % (ctl, f), uid, gid)
    if prof["home"] != "none":
        os.makedirs(home)
        os.chown(home, uid, gid)
        os.chmod(home, 0o755)
        key = hook_shell_key(row.shell)
        os.makedirs("/run/sshlab/hooks", exist_ok=True)
        # the hook text is the harness's own rc template (profile.py is the single source); %(ctl)s is this user's log dir
        for sk, tmpl in hooks.items():
            txt = tmpl % {"ctl": ctl} if "%(ctl)s" in tmpl else tmpl
            txt = 'export APPCI_W="%s"\n' % work + txt if sk in ("bash", "rbash", "zsh", "sh") else txt
            open("/run/sshlab/hooks/hook.%s.%s" % (sk, u), "w").write(txt)
        for rel, text in rc_plan(row).items():
            p = os.path.join(home, rel)
            os.makedirs(os.path.dirname(p), exist_ok=True)
            open(p, "w").write(text)
        sh(["chown", "-R", "%d:%d" % (uid, gid), home])
        if prof["home"] == "ro":
            sh(["chown", "-R", "root:root", home]); sh(["chmod", "-R", "a-w", home])
        if prof.get("ro_phosphor"):
            d = home + "/.phosphor"
            os.makedirs(d, exist_ok=True)
            sh(["chown", "-R", "root:root", d]); sh(["chmod", "-R", "a-w", d])


def lease(req):
    """POST /lease {run, pubkeys: [..], hooks: {shell: template}}: reset every user's home, install the keys, set secrets"""
    with LOCK:
        pubkeys = [k for k in (req.get("pubkeys") or []) if isinstance(k, str) and k.startswith(("ssh-", "ecdsa-", "sk-"))]
        if not pubkeys:
            raise ValueError("lease needs at least one public key")
        STATE["run"] = req.get("run") or "?"
        pw, mfa = secrets.token_urlsafe(12), base64.b32encode(secrets.token_bytes(20)).decode().rstrip("=")
        os.makedirs(RUN + "/totp", exist_ok=True)
        for row in STATE["rows"]:
            write_home(row, req.get("hooks") or {})
            ak = RUN + "/authkeys/" + row.user
            if row.user in usersdb.PASSWORD_USERS:
                open(ak, "w").write("")        # password-only users carry no key
            else:
                open(ak, "w").write("\n".join(pubkeys) + "\n")
            os.chmod(ak, 0o644)
        for u in usersdb.PASSWORD_USERS:
            sh(["chpasswd"], input="%s:%s\n" % (u, pw), check=True)
        for u in usersdb.MFA_USERS:
            f = RUN + "/totp/" + u
            open(f, "w").write(totp_file_text(mfa)); os.chmod(f, 0o600)
        STATE["secrets"] = dict(password=pw, mfa_secret=mfa)
        STATE["leased"] = True
        return dict(run=STATE["run"], users=[dict(user=r.user, shell=r.shell, profile=r.profile, home=usersdb.PROFILES[r.profile]["home"]) for r in STATE["rows"]],
                    hostkeys=STATE["hostkeys"], password=pw, mfa_secret=mfa, password_users=list(usersdb.PASSWORD_USERS), mfa_users=list(usersdb.MFA_USERS))


DROPS = {"n": 0}


def drop(req):
    """POST /drop {user, kind: rst|blackhole|stall, seconds}: break the user's live ssh connection(s) in a chosen way"""
    user, kind, secs = req.get("user"), req.get("kind"), float(req.get("seconds") or 10)
    if user not in {r.user for r in STATE["rows"]} or kind not in ("rst", "blackhole", "stall"):
        raise ValueError("drop needs a known user and kind rst|blackhole|stall")
    pids = session_pids(sh(["ps", "-eo", "pid=,args="]).stdout, user)
    if not pids:
        return dict(dropped=0, note="no live session for %s" % user)
    if kind == "stall":
        for pid in pids:
            os.kill(pid, signal.SIGSTOP)
        threading.Timer(secs, lambda: [safe_kill(p, signal.SIGCONT) for p in pids]).start()
        return dict(dropped=len(pids), kind=kind, seconds=secs)
    log = "".join(open(SSHD_LOG[p], errors="replace").read() for p in SSHD_LOG if os.path.exists(SSHD_LOG[p]))
    mine = set(accepted_ports(log, user))
    conns = [c for c in parse_ss(sh(["ss", "-tnH", "state", "established"]).stdout) if c[2] in (22, 2222) and (c[0], c[1]) in mine]
    DROPS["n"] += 1
    table = "sshlab_drop_%d" % DROPS["n"]          # one table per drop: concurrent drops never flush each other's rules
    sh(["nft", "-f", "-"], input=netem.nft_drop_setup(table), check=True)
    rules = [r for c in conns for r in netem.nft_drop_connection(c[0], c[1], kind, c[2], table)]
    if rules:
        sh(["nft", "-f", "-"], input="\n".join(rules) + "\n", check=True)
    if kind == "rst":
        for pid in pids:
            safe_kill(pid, signal.SIGKILL)
        threading.Timer(30, lambda: sh(["nft", "delete", "table", "inet", table])).start()
    else:
        threading.Timer(secs, lambda: sh(["nft", "delete", "table", "inet", table])).start()
    return dict(dropped=len(conns), kind=kind, seconds=secs)


def safe_kill(pid, sig):
    try:
        os.kill(pid, sig)
    except OSError:
        pass


def known_user(user):
    if user not in {r.user for r in STATE["rows"]}:
        raise ValueError("unknown user %r" % (user,))
    return user


def put_tree(req):
    """PUT /tree {user, dir (under /work/<user>), files: {rel: base64}, git: bool, branch, branches}"""
    user = known_user(req.get("user"))
    base = safe_path("%s/%s/%s" % (WORK_ROOT, user, (req.get("dir") or "").lstrip("/")), roots=("%s/%s" % (WORK_ROOT, user),))
    if not base:
        raise ValueError("dir escapes /work/<user>")
    uid, gid = uid_of(user), pwd.getpwnam(user).pw_gid
    os.makedirs(base, exist_ok=True)
    for rel, b64 in (req.get("files") or {}).items():
        p = safe_path(os.path.join(base, rel), roots=(base,))
        if not p:
            raise ValueError("file path escapes the tree: %r" % rel)
        os.makedirs(os.path.dirname(p), exist_ok=True)
        open(p, "wb").write(base64.b64decode(b64))
    sh(["chown", "-R", "%d:%d" % (uid, gid), base])
    if req.get("git"):
        names = [req.get("branch") or "main"] + list(req.get("branches") or [])
        if not all(re.match(r"^[A-Za-z0-9._/-]{1,60}$", n) and not n.startswith("-") for n in names):
            raise ValueError("bad branch name")
        g = ["git", "-c", "user.name=lab", "-c", "user.email=lab@lab.test", "-C", base]
        sh(g + ["init", "-q", "-b", names[0]], user=user, check=True)
        sh(g + ["add", "-A"], user=user, check=True)
        sh(g + ["commit", "-q", "-m", "fixture"], user=user, check=True)
        for b in names[1:]:
            sh(g + ["branch", b], user=user, check=True)
    return dict(dir=base, files=len(req.get("files") or {}))


def helper(req, delete=False):
    """POST /helper {user, files: {rel: base64}} puts a helper install (already-installed state) into ~/.phosphor/remote-server; DELETE removes it"""
    user = req.get("user")
    d = "%s/%s/.phosphor/remote-server" % (HOME_ROOT, user)
    if safe_path(d) is None or user not in {r.user for r in STATE["rows"]}:
        raise ValueError("unknown user")
    shutil.rmtree(d, ignore_errors=True)
    if not delete:
        uid, gid = uid_of(user), pwd.getpwnam(user).pw_gid
        for rel, b64 in (req.get("files") or {}).items():
            p = safe_path(os.path.join(d, rel), roots=(d,))
            if not p:
                raise ValueError("path escapes: %r" % rel)
            os.makedirs(os.path.dirname(p), exist_ok=True)
            open(p, "wb").write(base64.b64decode(b64)); os.chmod(p, 0o755)
        sh(["chown", "-R", "%d:%d" % (uid, gid), "%s/%s/.phosphor" % (HOME_ROOT, user)])
    return dict(dir=d, installed=not delete)


COMPLETERS = {
    "bash": ["bash", "-c", r'''. /usr/share/bash-completion/bash_completion 2>/dev/null
line=$1; read -ra COMP_WORDS <<< "$line"; case "$line" in *" ") COMP_WORDS+=("");; esac
COMP_LINE=$line; COMP_POINT=${#line}; COMP_CWORD=$((${#COMP_WORDS[@]}-1)); cur=${COMP_WORDS[COMP_CWORD]}
cmd=${COMP_WORDS[0]}; _completion_loader "$cmd" 2>/dev/null
spec=$(complete -p "$cmd" 2>/dev/null); fn=$(sed -n 's/.* -F \([^ ]*\).*/\1/p' <<< "$spec")
if [ -n "$fn" ]; then "$fn" "$cmd" "$cur" "${COMP_WORDS[COMP_CWORD-1]}"; printf '%s\n' "${COMPREPLY[@]}"; else compgen -f -- "$cur"; fi''', "_"],
    "fish": ["fish", "-c", 'complete -C "$argv[1]"'],
    "pwsh": ["pwsh", "-NoProfile", "-Command", '$l = $env:LAB_LINE; (TabExpansion2 -inputScript $l -cursorColumn $l.Length).CompletionMatches | ForEach-Object { $_.CompletionText }'],
    "nu": ["nu", "--ide-complete"],
    "zsh": ["zsh", "-fc", r'''zmodload zsh/zpty; zpty z zsh -fi; zpty -w z "autoload -U compinit; compinit -u; bindkey '^I' complete-word; PS1='> '"; sleep 1
zpty -r z -t >/dev/null 2>&1; zpty -w -n z "$1"$'\t'; sleep 1; zpty -r z out; print -r -- "$out"; zpty -d z''', "_"],
}


ANSI_RE = re.compile(r"\x1b\[[0-9;?]*[ -/]*[@-~]|\x1b\][^\x07]*\x07|\r")


def split_candidates(key, stdout, line):
    """one candidate per entry. bash/pwsh/nu print one per line; fish adds a TAB description; zsh returns the raw zpty screen text: strip ANSI, split on
    whitespace, drop the prompt and the words of the typed line itself"""
    if key == "zsh":
        typed = set(line.split()) | {">"}
        return [t for t in ANSI_RE.sub(" ", stdout).split() if t not in typed]
    return [l.split("\t")[0].rstrip() for l in stdout.splitlines() if l.strip()]


def complete(req):
    """POST /complete {user, line}: the user's OWN shell's candidates for `line` (run as that user, in /work/<user>)"""
    user, line = known_user(req.get("user")), req.get("line", "")
    row = next((r for r in STATE["rows"] if r.user == user), None)
    if not row:
        raise ValueError("unknown user")
    key = "bash" if row.shell == "rbash" else row.shell
    argv = list(COMPLETERS.get(key) or [])
    if not argv:
        return dict(shell=row.shell, candidates=None, note="no completion driver for %s" % row.shell)
    argv = argv + ([line] if key not in ("nu", "pwsh") else [])
    if key == "nu":
        argv = ["nu", "--ide-complete", str(len(line)), "-c", line]
    env = dict(os.environ, LAB_LINE=line) if key == "pwsh" else None
    if env:
        argv = ["runuser", "-u", user, "--preserve-environment", "--"] + argv
        p = sh(argv, cwd="%s/%s" % (WORK_ROOT, user), timeout=20, env=env)
    else:
        p = sh(argv, user=user, cwd="%s/%s" % (WORK_ROOT, user), timeout=20)
    out = split_candidates(key, p.stdout, line)
    return dict(shell=row.shell, candidates=out, raw=p.stdout[-4000:], rc=p.returncode, stderr=p.stderr[-500:])


def read_from(path, since):
    try:
        with open(path, "rb") as f:
            f.seek(0, 2)
            size = f.tell()
            f.seek(min(since, size))
            return dict(size=size, data=f.read(1_000_000).decode("utf-8", "replace"))
    except FileNotFoundError:
        return dict(size=0, data="")


# ------------------------------------------------------------------ HTTP
class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.0"
    server_version = "labd"

    def log_message(self, *a):
        pass

    def _auth(self):
        tok = os.environ.get("SSHLAB_TOKEN", "")
        got = self.headers.get("Authorization", "")
        return bool(tok) and secrets.compare_digest(got, "Bearer " + tok)

    def _send(self, code, obj=None, raw=None):
        body = raw if raw is not None else json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/octet-stream" if raw is not None else "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _body(self):
        n = int(self.headers.get("Content-Length") or 0)
        return json.loads(self.rfile.read(n) or b"{}") if n else {}

    def _route(self, method):
        if not self._auth():
            return self._send(401, {"error": "bad token"})
        u = urllib.parse.urlparse(self.path)
        q = {k: v[0] for k, v in urllib.parse.parse_qs(u.query).items()}
        try:
            with LOCK if u.path in ("/lease", "/fault", "/drop") else threading.Lock():
                self._dispatch(method, u.path, q)
        except (ValueError, KeyError) as e:
            self._send(400, {"error": str(e)})
        except Exception as e:  # noqa: BLE001
            self._send(500, {"error": "%s: %s" % (type(e).__name__, e)})

    def _dispatch(self, method, path, q):
        if method == "GET" and path == "/health":
            return self._send(200, dict(ok=True, now=time.time(), version=VERSION, run=STATE["run"], leased=STATE["leased"], ttl_left=int(STATE["started"] + STATE["ttl"] - time.time()),
                                        tc=STATE["tc_ok"], fault=STATE["fault"], hostkeys=STATE["hostkeys"], uids=STATE["uids"],
                                        sshd={str(p): (pr.poll() is None) for p, pr in STATE["procs"].items()}))
        if method == "POST" and path == "/lease":
            return self._send(200, lease(self._body()))
        if method == "POST" and path == "/fault":
            return self._send(200, dict(fault=apply_fault(self._body())))
        if method == "DELETE" and path == "/fault":
            return self._send(200, dict(fault=apply_fault({})))
        if method == "POST" and path == "/drop":
            return self._send(200, drop(self._body()))
        if method == "GET" and path == "/file":
            p = safe_path(q.get("path", ""))
            if not p or not os.path.isfile(p):
                return self._send(404, {"error": "no such file under /work /home /var/lib/appci/ctl"})
            return self._send(200, raw=open(p, "rb").read(5_000_000))
        if method == "DELETE" and path == "/file":
            p = safe_path(q.get("path", ""))
            if not p or not os.path.isfile(p):
                return self._send(200, dict(removed=False))
            os.remove(p)
            return self._send(200, dict(removed=True))
        if method == "GET" and path == "/ctl":
            user, name = q.get("user", ""), os.path.basename(q.get("name", ""))
            p = safe_path("%s/%s/%s" % (CTL_ROOT, user, name))
            if not p:
                raise ValueError("bad ctl path")
            return self._send(200, read_from(p, int(q.get("since", 0))))
        if method == "PUT" and path == "/tree":
            return self._send(200, put_tree(self._body()))
        if method in ("POST", "DELETE") and path == "/helper":
            return self._send(200, helper(self._body(), delete=(method == "DELETE")))
        if method == "POST" and path == "/complete":
            return self._send(200, complete(self._body()))
        if method == "GET" and path == "/sshdlog":
            port = int(q.get("port", 22))
            if port not in SSHD_LOG:
                raise ValueError("port must be 22 or 2222")
            return self._send(200, read_from(SSHD_LOG[port], int(q.get("since", 0))))
        self._send(404, {"error": "no route %s %s" % (method, path)})

    def do_GET(self):
        self._route("GET")

    def do_POST(self):
        self._route("POST")

    def do_PUT(self):
        self._route("PUT")

    def do_DELETE(self):
        self._route("DELETE")


class Server(socketserver.ThreadingMixIn, http.server.HTTPServer):
    daemon_threads = True
    allow_reuse_address = True


def main():
    if not os.environ.get("SSHLAB_TOKEN"):
        sys.exit("labd: SSHLAB_TOKEN is not set")
    os.makedirs(RUN, exist_ok=True)
    STATE["rows"] = usersdb.load()
    gen_hostkeys()
    probe_tc()
    note = block_outbound(STATE["rows"])
    print("labd: outbound block: %s; tc probe: %s" % (note, STATE["tc_ok"]), flush=True)
    start_sshds(STATE["rows"])
    echo_server()
    signal.signal(signal.SIGTERM, lambda *_: os._exit(0))
    threading.Thread(target=supervise, daemon=True).start()
    print("labd: ready", flush=True)
    Server(("0.0.0.0", 7000), Handler).serve_forever()


if __name__ == "__main__":
    main()
