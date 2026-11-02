# appci/ci_env.py: machine-local CI settings the repo must not carry (the owner's Ollama endpoint). Environment wins; else the
# per-machine file ~/.config/phosphor-ci/ci.env (KEY=VALUE lines, # comments; mode 600; Windows %USERPROFILE%\.config\phosphor-ci\ci.env).
# Keys: APPCI_OLLAMA_URL, APPCI_OLLAMA_MODEL (default gpt-oss:20b). The VALUE is never printed or written to any artifact.
import os, re

PATH = os.path.join(os.path.expanduser("~"), ".config", "phosphor-ci", "ci.env")


def _file():
    out = {}
    try:
        for l in open(PATH, errors="replace").read().splitlines():
            l = l.strip()
            if l and not l.startswith("#") and "=" in l:
                k, v = l.split("=", 1)
                out[k.strip()] = v.strip().strip('"').strip("'")
    except OSError:
        pass
    return out


def get(key, default=None):
    v = os.environ.get(key)
    if v:
        return v
    return _file().get(key) or default


def ollama():
    """(url, model) or (None, None); announces only WHETHER a provider is configured"""
    url = get("APPCI_OLLAMA_URL")
    if not url:
        print("ollama: not configured - agent provider cases skip", flush=True)
        return None, None
    print("ollama: configured from %s" % ("environment" if os.environ.get("APPCI_OLLAMA_URL") else "ci.env"), flush=True)
    return url, get("APPCI_OLLAMA_MODEL", "gpt-oss:20b")


def redact(text, url):
    """strip the endpoint (and its host:port) from text that is about to be copied into artifacts"""
    if not url:
        return text
    hp = re.sub(r"^\w+://", "", url).rstrip("/")
    for needle in sorted({url, url.rstrip("/"), hp}, key=len, reverse=True):
        if needle:
            text = text.replace(needle, "<ollama>")
    return text
