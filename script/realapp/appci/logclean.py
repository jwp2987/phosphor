"""pure (r27 loganchors): read the app log without the continuation lines of multi-line messages, and anchor hop patterns at a real log line.
Shared by cases.py, replay.py and injacct.py (no imports of the case modules)."""
import re


DIFF_CONT_PREFIXES = ("+", "-", " ", "\t", "@@", "diff --git", "index ", "\\ No newline")
LOG_LINE_START_RE = re.compile(r"^\d{4}-\d\d-\d\dT\S+ \[\w+\] ")       # a real app-log line: `2026-11-12T19:35:37Z [DEBUG] input_order hop=...`


def app_log_clean(text):
    """pure (r27 loganchors): the app-log text without the CONTINUATION lines of a multi-line message. The app once logged a whole git diff in one
    message ('Failed to get file diff for <path>' + the diff body, #2191) and the diff's '+...' lines were older harness log lines, so a hop
    pattern matched text that was only ECHOED inside a message. A diff body line starts with '+', '-', ' ', '@@', 'diff --git', 'index ' or a tab;
    a real log line starts with a digit (its timestamp). Every harness reader of the app log applies this before any hop= match."""
    if not text:
        return text or ""
    return "".join(l for l in text.splitlines(True) if not l.startswith(DIFF_CONT_PREFIXES))


def log_hop_rx(body, flags=0):
    """pure: re.compile of `body` anchored at the start of a real app-log line: `^<timestamp> [LEVEL] ` then `body` (which names the target text
    such as `input_order hop=view_mode ...`). A continuation line of a multi-line message ('+2026-...' of a diff body) never matches."""
    return re.compile(LOG_LINE_START_RE.pattern + r"(?:\S+ )?" + body, flags | re.M)
