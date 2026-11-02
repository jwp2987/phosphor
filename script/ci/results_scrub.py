# script/ci/results_scrub.py: privacy rules for the test-results/ archive, shared by collect_results.py (scrub on the way in) and
# script/check_test_results (refuse on the way out). The owner's endpoints live only in machine-local config (ci.env, the realapp
# config, the environment), never in the repo; scrub() masks their VALUES, find_leaks() reports anything that still looks like one.
import os, re, sys

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
