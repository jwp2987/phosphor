#!/usr/bin/env python3
# labctl.py: the ONLY caller of podman for the sshlab (docs/testing.md 2.15). Run on the box (little-thing) by the sshlab-image CI job and, for the
# Mac/Windows runners, through the owner's forced-command ssh key (`labctl.py remote`, which accepts only start/stop/list/gc).
#   build                       build the `full` and `bare` images tagged by the sha256 of this tree (skip when present); keep 3 tags per flavor
#   start --run R --flavor F    gc first, then start one container; prints JSON {cid, ports, token, hostkeys, run, expires, image}
#   stop --id CID               remove that exact container (it must carry the sshlab label); never a pattern
#   gc                          remove containers whose expires label is past, by exact id
#   list                        JSON of lab containers
#   selftest                    per user: login, shell identity, RTT gate at 150 ms, MFA, onesess refusal; then assert no lab container is left
# Never put keys, hostnames or IPs here: the key is generated per run, the box address comes from the caller.
import argparse, hashlib, io, json, os, re, secrets, shlex, shutil, socket, subprocess, sys, tarfile, tempfile, time, urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
LABEL = "phosphor.sshlab"
NETWORK = "sshlab"
IMAGE_REPO = "localhost/phosphor-sshlab"
KEEP_TAGS = 3
MAX_TTL = 8 * 3600
MAX_CONTAINERS = 3
CAPS = ("NET_ADMIN", "SETUID", "SETGID", "CHOWN", "DAC_OVERRIDE", "FOWNER", "SYS_CHROOT", "AUDIT_WRITE", "KILL", "NET_BIND_SERVICE")
PORTS = (22, 2222, 7, 7000)
PIN_RE = {
    "DEBIAN_IMAGE": r"^[a-z0-9./_:-]+@sha256:[0-9a-f]{64}$", "DEBIAN_SNAPSHOT": r"^\d{8}T\d{6}Z$",
    "NU_SHA256": r"^[0-9a-f]{64}$", "PWSH_SHA256": r"^[0-9a-f]{64}$", "NU_URL": r"^https://\S+$", "PWSH_URL": r"^https://\S+$",
    "NU_VERSION": r"^\d+\.\d+\.\d+$", "PWSH_VERSION": r"^\d+\.\d+\.\d+$",
}


# ------------------------------------------------------------------ pure pieces (selftest)
def parse_versions(text):
    out = {}
    for line in text.splitlines():
        line = line.strip()
        if line and not line.startswith("#") and "=" in line:
            k, v = line.split("=", 1)
            out[k.strip()] = v.strip()
    return out


def check_pins(versions):
    """-> list of problems; empty = every pin is real. A PLACEHOLDER value, a missing key or a malformed digest/sha256/date refuses the build."""
    probs = []
    for k, rx in PIN_RE.items():
        v = versions.get(k)
        if v is None:
            probs.append("%s is missing" % k)
        elif "PLACEHOLDER" in v:
            probs.append("%s is still a PLACEHOLDER (%s)" % (k, v))
        elif not re.match(rx, v):
            probs.append("%s=%r does not look like a pin (%s)" % (k, v, rx))
    return probs


def tree_files(root):
    out = []
    for d, dirs, files in os.walk(root):
        dirs[:] = sorted(x for x in dirs if x != "__pycache__")
        for f in sorted(files):
            if f.endswith(".pyc") or (d == root and f == "labctl.py"):
                continue
            out.append(os.path.relpath(os.path.join(d, f), root).replace(os.sep, "/"))
    return sorted(out)


def tree_hash(root):
    """sha256 over (relative path, #! flag, LF-normalised bytes) of the sshlab tree except labctl.py (the image does not contain it)"""
    h = hashlib.sha256()
    for rel in tree_files(root):
        p = os.path.join(root, rel)
        data = open(p, "rb").read().replace(b"\r\n", b"\n")          # LF-normalised, and the exec bit comes from the content (a #! script), not the file mode:
        h.update(("%s\0%d\0" % (rel, 1 if data.startswith(b"#!") else 0)).encode())      # a Windows tar (0666, CRLF) must hash like the Linux tree
        h.update(data)
        h.update(b"\0")
    return h.hexdigest()


def image_tag(flavor, thash):
    return "%s:%s-%s" % (IMAGE_REPO, flavor, thash[:16])


def run_argv(tag, run, flavor, ttl, token, now, name=None):
    """the `podman run` argv for one lab container. No host path is mounted; ports are published on all interfaces with a random host port."""
    exp = int(now + ttl)
    a = ["podman", "run", "-d", "--rm", "--init", "--network", NETWORK, "--hostname", "sshlab.lab.test", "--cap-drop", "all"]
    for c in CAPS:
        a += ["--cap-add", c]
    a += ["--pids-limit", "1024", "--memory", "3g", 
          "--label", "%s=1" % LABEL, "--label", "%s.run=%s" % (LABEL, run), "--label", "%s.expires=%d" % (LABEL, exp), "--label", "%s.flavor=%s" % (LABEL, flavor),
          "-e", "SSHLAB_TOKEN", "-e", "SSHLAB_TTL_S=%d" % ttl, "--tmpfs", "/run:rw,mode=755", "--tmpfs", "/home:rw,exec,mode=755",
          "--tmpfs", "/work:rw,exec,mode=755", "--tmpfs", "/var/lib/appci:rw,mode=755"]
    if flavor == "bare":
        a += ["--read-only", "--tmpfs", "/tmp:rw,noexec,nosuid,mode=1777"]
    for p in PORTS:
        a += ["-p", "0.0.0.0::%d" % p]
    return a + [tag]


def parse_ports(text):
    """`podman port <cid>` lines '22/tcp -> 0.0.0.0:41234' -> {22: 41234}"""
    out = {}
    for line in text.splitlines():
        m = re.match(r"^(\d+)/tcp -> \S+:(\d+)\s*$", line.strip())
        if m:
            out[int(m.group(1))] = int(m.group(2))
    return out


def expired_ids(containers, now):
    """containers: [{'Id':..,'Labels':{..}}] -> exact ids whose expires label is in the past (a lab container without a parsable expiry counts as expired)"""
    out = []
    for c in containers:
        lab = c.get("Labels") or {}
        if lab.get(LABEL) != "1":
            continue
        try:
            if int(lab.get(LABEL + ".expires", "0")) <= now:
                out.append(c["Id"])
        except ValueError:
            out.append(c["Id"])
    return out


def prune_plan(images, keep=KEEP_TAGS):
    """images: [(created_epoch, tag, flavor)] -> tags to remove: all but the newest `keep` per flavor"""
    out = []
    for fl in {i[2] for i in images}:
        mine = sorted((i for i in images if i[2] == fl), reverse=True)
        out += [i[1] for i in mine[keep:]]
    return out


REMOTE_RE = {
    "start": re.compile(r"^start --run ([A-Za-z0-9._-]{1,64}) --flavor (full|bare) --ttl (\d{1,5})$"),
    "stop": re.compile(r"^stop --id ([0-9a-f]{12,64})$"),
    "list": re.compile(r"^list$"), "gc": re.compile(r"^gc$"),
}


def parse_remote_command(cmd):
    """the forced-command whitelist (SSH_ORIGINAL_COMMAND): -> (verb, args dict) or raises ValueError. Only start / stop / list / gc, strictly shaped."""
    cmd = (cmd or "").strip()
    verb = cmd.split(" ", 1)[0]
    rx = REMOTE_RE.get(verb)
    m = rx.match(cmd) if rx else None
    if not m:
        raise ValueError("labctl remote: refused %r (allowed: start --run R --flavor full|bare --ttl N | stop --id CID | list | gc)" % cmd[:80])
    if verb == "start":
        ttl = int(m.group(3))
        if not 60 <= ttl <= MAX_TTL:
            raise ValueError("ttl must be 60..%d" % MAX_TTL)
        return verb, dict(run=m.group(1), flavor=m.group(2), ttl=ttl)
    if verb == "stop":
        return verb, dict(id=m.group(1))
    return verb, {}


def safe_extract(data, dest):
    """extract a tar (bytes) into dest: regular files and dirs only, no absolute or .. names, size-capped. Returns the root of the sshlab tree."""
    total = 0
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:*") as tf:
        for m in tf.getmembers():
            n = os.path.normpath(m.name)
            if n.startswith("/") or n.startswith("..") or "/../" in n or not (m.isreg() or m.isdir()):
                raise ValueError("unsafe tar member %r" % m.name)
            total += m.size
            if total > 64 * 1024 * 1024:
                raise ValueError("tar too large")
        tf.extractall(dest)
    root = os.path.join(dest, "sshlab")
    if not os.path.isfile(os.path.join(root, "Containerfile")):
        raise ValueError("tar has no sshlab/Containerfile")
    return root


# ------------------------------------------------------------------ podman
def podman(*args, check=True, timeout=600, input=None, env=None):
    p = subprocess.run(["podman"] + list(args), capture_output=True, text=True, timeout=timeout, input=input, env=env)
    if check and p.returncode:
        raise SystemExit("podman %s failed (%d): %s" % (" ".join(args[:3]), p.returncode, (p.stderr or p.stdout).strip()[-600:]))
    return p


def image_exists(tag):
    return podman("image", "exists", tag, check=False).returncode == 0


def lab_containers():
    p = podman("ps", "-a", "--filter", "label=%s=1" % LABEL, "--format", "json", check=False)
    try:
        return json.loads(p.stdout or "[]") or []
    except ValueError:
        return []


def cmd_build(root, args):
    vers = parse_versions(open(os.path.join(root, "versions.env")).read())
    probs = check_pins(vers)
    if probs:
        print("labctl build refused: versions.env has unpinned values:\n  " + "\n  ".join(probs), file=sys.stderr)
        return 2
    th = tree_hash(root)
    built = []
    for flavor in ("full", "bare"):
        tag = image_tag(flavor, th)
        if image_exists(tag) and not args.force:
            print("image %s exists, skipped" % tag)
            continue
        ba = []
        for k in ("DEBIAN_IMAGE", "DEBIAN_SNAPSHOT", "NU_URL", "NU_SHA256", "PWSH_URL", "PWSH_SHA256"):
            ba += ["--build-arg", "%s=%s" % (k, vers[k])]
        podman("build", "--target", flavor, "-t", tag, "--label", LABEL + ".image=1", "--label", LABEL + ".flavor=" + flavor, *ba, "-f", os.path.join(root, "Containerfile"), root, timeout=3600)
        built.append(tag)
        print("built " + tag)
    prune_images()
    print(json.dumps(dict(hash=th, built=built)))
    return 0


def prune_images():
    p = podman("images", "--filter", "label=%s.image=1" % LABEL, "--format", "json", check=False)
    try:
        imgs = json.loads(p.stdout or "[]")
    except ValueError:
        return
    rows = []
    for i in imgs:
        for t in i.get("Names") or []:
            fl = "bare" if ":bare-" in t else "full"
            rows.append((i.get("Created", 0), t, fl))
    for t in prune_plan(rows):
        podman("rmi", t, check=False)


def health(port, token, timeout=5):
    req = urllib.request.Request("http://127.0.0.1:%d/health" % port, headers={"Authorization": "Bearer " + token})
    return json.loads(urllib.request.urlopen(req, timeout=timeout).read())


def cmd_gc():
    ids = expired_ids(lab_containers(), time.time())
    for cid in ids:
        podman("rm", "-f", "-t", "2", cid, check=False)
    return ids


def cmd_start(root, run, flavor, ttl):
    cmd_gc()
    if len(lab_containers()) >= MAX_CONTAINERS:
        raise SystemExit("%d lab containers already run (cap %d): stop one (labctl.py list / stop --id) or wait for its TTL" % (len(lab_containers()), MAX_CONTAINERS))
    tag = image_tag(flavor, tree_hash(root))
    if not image_exists(tag):
        raise SystemExit("image %s is not built: run the sshlab-image job (labctl.py build)" % tag)
    if podman("network", "exists", NETWORK, check=False).returncode:
        podman("network", "create", NETWORK)
    token = secrets.token_hex(24)
    cid = podman(*run_argv(tag, run, flavor, ttl, token, time.time())[1:], env=dict(os.environ, SSHLAB_TOKEN=token)).stdout.strip()      # the token travels in the environment, never on the (ps-visible) command line
    ports = parse_ports(podman("port", cid).stdout)
    deadline, h = time.time() + 90, None
    while time.time() < deadline:
        try:
            h = health(ports[7000], token)
            break
        except Exception:
            time.sleep(0.5)
    if not h:
        logs = podman("logs", "--tail", "40", cid, check=False).stdout
        podman("rm", "-f", "-t", "2", cid, check=False)
        raise SystemExit("labd did not become healthy within 90 s; log tail:\n" + logs)
    return dict(cid=cid, ports=ports, token=token, hostkeys=h.get("hostkeys"), tc=h.get("tc"), run=run, flavor=flavor, image=tag, expires=int(time.time() + ttl))


def cmd_stop(cid):
    if not re.match(r"^[0-9a-f]{12,64}$", cid or ""):
        raise SystemExit("stop --id needs 12-64 hex characters of the container id")
    for c in lab_containers():
        if c["Id"].startswith(cid):
            podman("rm", "-f", "-t", "2", c["Id"], check=False)
            return dict(stopped=c["Id"])
    return dict(stopped=None, note="no lab container with that id (already gone, or not a lab container: nothing was touched)")


def cmd_list():
    return [dict(id=c["Id"], labels={k: v for k, v in (c.get("Labels") or {}).items() if k.startswith(LABEL)}, state=c.get("State")) for c in lab_containers()]


# ------------------------------------------------------------------ remote (forced command)
def cmd_remote():
    try:
        verb, a = parse_remote_command(os.environ.get("SSH_ORIGINAL_COMMAND", ""))
    except ValueError as e:
        print(str(e), file=sys.stderr)
        return 126
    if verb == "start":
        data = sys.stdin.buffer.read(64 * 1024 * 1024)
        with tempfile.TemporaryDirectory(prefix="sshlab-remote-") as d:
            try:
                root = safe_extract(data, d)
            except (ValueError, tarfile.TarError) as e:
                print("labctl remote: bad tar: %s" % e, file=sys.stderr)
                return 126
            out = cmd_start(root, a["run"], a["flavor"], a["ttl"])
    elif verb == "stop":
        out = cmd_stop(a["id"])
    elif verb == "list":
        out = cmd_list()
    else:
        out = cmd_gc()
    print(json.dumps(out))
    return 0


# ------------------------------------------------------------------ selftest (CI, on the box)
def cmd_selftest(root):
    sys.path.insert(0, os.path.join(HERE, "..", "appci"))
    import lab as L
    tmp = tempfile.mkdtemp(prefix="sshlab-selftest-")
    key = os.path.join(tmp, "k")
    subprocess.run(["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", key], check=True)
    failures, info = [], None
    try:
        for flavor in ("full", "bare"):
            info = cmd_start(root, "selftest-" + flavor, flavor, 1800)
            lab = L.Lab.from_info(info, host="127.0.0.1", key=key, local=True)
            lease = lab.lease([open(key + ".pub").read().strip()])
            users = [u for u in lease["users"] if flavor == "full" or u["shell"] in ("bash", "rbash")]
            for u in users:
                name = u["user"]
                if name in ("p_password", "p_kbdint", "p_mfa", "p_slowauth") or u["profile"] in ("rcprompt", "slowrc", "nohome"):
                    continue          # exercised below or by the cases
                r = lab.ssh(name, "echo IDENT:$0; echo MARK", key=key)
                ok = "MARK" in r.stdout
                print("%-6s %-12s login %s" % (flavor, name, "ok" if ok else "FAIL: " + (r.stderr or r.stdout).strip()[:120]))
                if not ok and name not in ("p_nopty",):
                    failures.append("%s/%s: login failed" % (flavor, name))
            if flavor == "full":
                lab.fault(rtt_ms=150)
                v = L.rtt_gate_verdict(lab.measure_rtt(20), 150)
                print("RTT gate at 150 ms:", v)
                if v["verdict"] != "ok":
                    failures.append("RTT gate at 150 ms: %s" % v)
                lab.clear_fault()
                if not L.mfa_login_ok(lab, key):
                    failures.append("p_mfa: publickey + TOTP login failed")
                if not L.onesess_refuses(lab, key, tmp):
                    failures.append("p_onesess: a second session was not refused")
            lab.stop()
            info = None
    finally:
        if info:
            cmd_stop(info["cid"])
        left = [c for c in cmd_list() if c["labels"].get(LABEL + ".run", "").startswith("selftest-")]      # only our own containers, not the whole box
        shutil.rmtree(tmp, ignore_errors=True)
    if left:
        failures.append("lab containers left after selftest: %s" % [c["id"][:12] for c in left])
    print("sshlab selftest: %s" % ("FAILED: " + "; ".join(failures) if failures else "ok"))
    return 1 if failures else 0


def main(argv=None):
    ap = argparse.ArgumentParser(prog="labctl.py")
    sub = ap.add_subparsers(dest="verb", required=True)
    b = sub.add_parser("build"); b.add_argument("--force", action="store_true")
    s = sub.add_parser("start"); s.add_argument("--run", required=True); s.add_argument("--flavor", default="full", choices=("full", "bare")); s.add_argument("--ttl", type=int, default=3600)
    st = sub.add_parser("stop"); st.add_argument("--id", required=True)
    for v in ("gc", "list", "selftest", "remote"):
        sub.add_parser(v)
    a = ap.parse_args(argv)
    root = os.environ.get("SSHLAB_TREE") or HERE
    if a.verb == "remote":
        return cmd_remote()
    if a.verb == "build":
        return cmd_build(root, a)
    if a.verb == "start":
        if not 60 <= a.ttl <= MAX_TTL:
            raise SystemExit("ttl must be 60..%d" % MAX_TTL)
        print(json.dumps(cmd_start(root, a.run, a.flavor, a.ttl)))
    elif a.verb == "stop":
        print(json.dumps(cmd_stop(a.id)))
    elif a.verb == "gc":
        print(json.dumps(dict(removed=cmd_gc())))
    elif a.verb == "list":
        print(json.dumps(cmd_list()))
    elif a.verb == "selftest":
        return cmd_selftest(root)
    return 0


if __name__ == "__main__":
    sys.exit(main())
