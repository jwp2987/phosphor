# script/ci/results_scrub.py: privacy rules for the test-results/ archive, shared by collect_results.py (scrub on the way in) and
# script/check_test_results (refuse on the way out). The owner's endpoints live only in machine-local config (ci.env, the realapp
# config, the environment), never in the repo; scrub() masks their VALUES, find_leaks() reports anything that still looks like one.
import os, re, socket, sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "realapp", "appci"))
try:
    import ci_env
except Exception:                                   # the scrubber must work even if appci moves
    ci_env = None

IPV4 = re.compile(r"(?<![\w.])((?:25[0-5]|2[0-4]\d|1?\d?\d)(?:\.(?:25[0-5]|2[0-4]\d|1?\d?\d)){3})(?![\w.]*\w)")
SECRET = [("gitlab token", re.compile(r"glpat-[A-Za-z0-9_-]{16,}")), ("github token", re.compile(r"gh[pousr]_[A-Za-z0-9]{30,}")),
          ("api key", re.compile(r"\bsk-[A-Za-z0-9_-]{20,}")), ("aws key", re.compile(r"\bAKIA[0-9A-Z]{16}\b")),
          ("private key", re.compile(r"-----BEGIN [A-Z ]*PRIVATE KEY-----")), ("bearer token", re.compile(r"(?i)\bbearer\s+[A-Za-z0-9._~+/-]{24,}")),
          ("password assignment", re.compile(r"(?i)\b(?:password|passwd|secret|api[_-]?key|token)\s*[=:]\s*['\"]?[A-Za-z0-9/+_.-]{12,}")),
          ("url with credentials", re.compile(r"\w+://[^/\s:@]+:[^/\s@]+@"))]
HOME_RX = re.compile(r"(?i)(/home/|/Users/|[A-Z]:\\Users\\)[^/\\\s\"']+")
# `uname -a` and `id` output (#1516): kernel line carries the hostname, id carries the user name and every group.
UNAME_RX = re.compile(r"(?m)^(Linux|Darwin|FreeBSD) +\S+( +\d[^\n]*)")
ID_RX = re.compile(r"uid=\d+\([^)]*\) +gid=\d+\([^)]*\)(?: +groups=[^\n]*)?")
CONFIG_VARS = ["PHOSPHOR_BOX_SSH", "PHOSPHOR_BOX_HOST", "PHOSPHOR_MAC_SSH", "PHOSPHOR_WIN_SSH", "WINUSER", "APPCI_OLLAMA_URL"]
SAFE_IPS = ("127.", "0.0.0.0", "255.", "169.254.", "192.0.2.", "198.51.100.")   # loopback, mask, link-local, RFC 5737 docs (NOT 203.0.113.x: the owner's boxes live there)


def _local_values():
    """machine-local endpoint values: ci.env + environment + ~/.config/phosphor-realapp.sh exports"""
    vals = set()
    for k in CONFIG_VARS:
        v = os.environ.get(k) or (ci_env.get(k) if ci_env else None)
        if v:
            vals.add(v)
    try:
        for l in open(os.path.expanduser("~/.config/phosphor-realapp.sh"), errors="replace"):
            m = re.match(r"\s*export\s+(\w+)=(.*)", l)
            if m and m.group(1) in CONFIG_VARS:
                v = re.sub(r"\s+#.*$", "", m.group(2)).strip().strip('"').strip("'")
                if v and "$" not in v:
                    vals.add(v)
    except OSError:
        pass
    h = socket.gethostname().split(".")[0]                   # this machine's own host name (the account name is covered by HOME_RX / ID_RX)
    if len(h) >= 4 and h.lower() not in ("localhost", "runner"):
        vals.add(h)
    extra = set()
    for v in vals:                                  # user@host and bare host / host:port forms
        v2 = re.sub(r"^\w+://", "", v).rstrip("/")
        extra.add(v2)
        if "@" in v2:
            extra.add(v2.split("@", 1)[1])
    return sorted({x for x in vals | extra if len(x) >= 4}, key=len, reverse=True)


_VALS = None


def scrub(text):
    global _VALS
    if _VALS is None:
        _VALS = _local_values()
    for v in _VALS:
        text = text.replace(v, "<redacted-endpoint>")
    text = IPV4.sub(lambda m: m.group(1) if m.group(1).startswith(SAFE_IPS) else "<ip>", text)
    text = UNAME_RX.sub(lambda m: m.group(1) + " <host>" + m.group(2), text)
    text = ID_RX.sub("uid=<id> gid=<id> groups=<redacted>", text)
    text = HOME_RX.sub(lambda m: m.group(1) + "<user>", text)
    for name, rx in SECRET:
        text = rx.sub("<redacted-%s>" % name.replace(" ", "-"), text)
    return text


def find_leaks(text):
    """list of (kind, snippet) that must not be committed"""
    global _VALS
    if _VALS is None:
        _VALS = _local_values()
    out = [("local endpoint value", v[:3] + "...") for v in _VALS if v in text]
    out += [("ip address", m.group(1)) for m in IPV4.finditer(text) if not m.group(1).startswith(SAFE_IPS)]
    out += [(n, m.group(0)[:12] + "...") for n, rx in SECRET for m in rx.finditer(text)]
    return out


TEXT_SKIP = (".jpg", ".jpeg", ".png", ".gif", ".webp", ".zip", ".gz", ".bin", ".exe", ".dmg", ".deb", ".AppImage")


def scrub_tree(root, max_bytes=8 * 1024 * 1024):
    """scrub every text file under root IN PLACE (CI job artifacts, #1516: the committed archive was already scrubbed by
    collect_results, the 7-day job artifacts were not). Binary files (images, NUL bytes) and files over max_bytes are left alone;
    returns the number of files changed."""
    changed = 0
    for dp, _, names in os.walk(root):
        for n in names:
            p = os.path.join(dp, n)
            if n.endswith(TEXT_SKIP) or os.path.islink(p):
                continue
            try:
                if os.path.getsize(p) > max_bytes:
                    continue
                raw = open(p, "rb").read()
                if b"\0" in raw:
                    continue
                text = raw.decode("utf-8", errors="surrogateescape")
                new = scrub(text)
                if new != text:
                    open(p, "wb").write(new.encode("utf-8", errors="surrogateescape"))
                    changed += 1
            except OSError as e:
                print("results_scrub: %s: %s" % (p, e), file=sys.stderr)
    return changed


if __name__ == "__main__":
    # python3 script/ci/results_scrub.py --tree DIR...   (a missing DIR is not an error: the job may have died before creating it)
    if len(sys.argv) >= 3 and sys.argv[1] == "--tree":
        for d in sys.argv[2:]:
            if os.path.isdir(d):
                print("results_scrub: %s: %d file(s) scrubbed" % (d, scrub_tree(d)))
        sys.exit(0)
    sys.exit("usage: results_scrub.py --tree DIR...")
