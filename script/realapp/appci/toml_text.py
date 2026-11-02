# appci/toml_text.py: minimal TEXTUAL key edits for the owner's settings.toml, which uses multi-line inline tables that
# tomllib rejects and that must not be round-tripped through a TOML library (comments/layout/secret regexes stay byte-identical
# except for the edited keys). set_key/del_key address "table.sub" + key; values are written as TOML literals by the caller.
import re

HDR = re.compile(r"^\s*(\[\[?)\s*([^\]]+?)\s*\]\]?\s*(#.*)?$")


def _value_end(text, i):
    """index just past the value that starts at text[i]; string-aware bracket depth across lines"""
    depth, n, j = 0, len(text), i
    while j < n:
        c = text[j]
        if text.startswith('"""', j) or text.startswith("'''", j):
            q = text[j:j + 3]
            k = text.find(q, j + 3)
            j = (k + 3) if k >= 0 else n
            continue
        if c == '"':
            j += 1
            while j < n and text[j] != '"' and text[j] != "\n":
                j += 2 if text[j] == "\\" else 1
            j += 1
            continue
        if c == "'":
            k = text.find("'", j + 1)
            j = (k + 1) if k >= 0 else n
            continue
        if c == "#":
            while j < n and text[j] != "\n":
                j += 1
            continue
        if c in "[{":
            depth += 1
        elif c in "]}":
            depth -= 1
        elif c == "\n" and depth <= 0:
            return j
        j += 1
    return n


def _sections(text):
    """[(table_name, header_line_start, body_start, body_end)] for top-level (column 0) headers; '' = root"""
    out, pos, cur = [], 0, ("", 0, 0)
    lines = text.split("\n")
    offs, o = [], 0
    for l in lines:
        offs.append(o)
        o += len(l) + 1
    i = 0
    depth_skip = 0
    while i < len(lines):
        l = lines[i]
        m = HDR.match(l) if l[:1] == "[" else None
        if m:
            out.append((cur[0], cur[1], cur[2], offs[i]))
            cur = (m.group(2).replace(" ", "").replace('"', ""), offs[i], offs[i] + len(l) + 1)
        i += 1
    out.append((cur[0], cur[1], cur[2], len(text)))
    return out


def _find(text, table, key):
    for name, hs, bs, be in _sections(text):
        if name != table:
            continue
        m = re.compile(r"^(%s|\"%s\")[ \t]*=" % (re.escape(key), re.escape(key)), re.M).search(text, bs, be)
        if m:
            return m.start(), _value_end(text, m.end()), (hs, bs, be)
    return None


def set_key(text, table, key, literal):
    f = _find(text, table, key)
    line = "%s = %s" % (key, literal)
    if f:
        return text[:f[0]] + line + text[f[1]:]
    for name, hs, bs, be in _sections(text):
        if name == table and table:
            return text[:bs] + line + "\n" + text[bs:]
    if not table:
        return line + "\n" + text
    return text.rstrip("\n") + "\n\n[%s]\n%s\n" % (table, line)


def del_key(text, table, key):
    f = _find(text, table, key)
    if not f:
        return text
    e = f[1] + 1 if f[1] < len(text) else f[1]
    return text[:f[0]] + text[e:]


def get_key(text, table, key):
    f = _find(text, table, key)
    return None if not f else text[f[0]:f[1]]
