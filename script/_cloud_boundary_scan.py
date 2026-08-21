#!/usr/bin/env python3
r"""Emit every import of crate::server / crate::cloud_object from outside them.

Output: one line per imported path, "<file> <fully-qualified-path>".

WHY THIS IS NOT A REGEX (or awk)
--------------------------------
Three successive versions of this check were defeated:

  1. `^\s*use crate::(server|cloud_object)::` saw only the plain form. It missed
     `pub use` re-exports and every `use crate::{...}` brace form, hiding 169
     live sites -- 62% of the real surface.
  2. An awk state machine that tracked `use crate::{` blocks caught those, but
     recorded only the OPENER line, so 154 allowlist entries collapsed to the
     opaque string `<file> use crate::{`. Adding a brand-new cloud import to an
     already-allowlisted block was therefore invisible: an adversarial probe
     inserted `server::telemetry::SecretBackdoor` into an existing block in
     persistence/sqlite.rs and the guard reported "ok". It also missed
     `pub(crate) use`, `use crate::server;` (no trailing `::`),
     `use crate::server as s;`, a bare `server` inside a brace list,
     `use crate :: server ::` with spaces, and `use super::super::server::`;
     and it false-positived on a `//` comment naming `server::` inside an
     unrelated block, and never reset its state on a `}` / `;` split across
     lines.

So: parse the `use` statements, expand the brace tree, and record the RESOLVED
PATHS. An allowlist of paths cannot be defeated by adding a path.
"""
import os, re, sys

TARGETS = ("server", "cloud_object")

def strip_comments(src: str) -> str:
    """Blank out comments, string literals and char literals, preserving offsets.

    Rust's literal forms all have to be handled, because getting any of them
    wrong BLANKS REAL CODE and hides imports rather than merely mis-parsing:

      * raw strings `r"..."`, `r#"..."#`, `br#"..."#` -- NO escape processing.
        An earlier version treated `\\` as an escape inside every quoted region,
        so `Path::new(&format!(r"{system_drive}\\"))`
        (app/src/terminal/available_shells.rs:785) swallowed its own terminator
        and blanked the rest of the file. Eight live files lost real `use`
        statements that way, and an import placed after such a literal was
        invisible to this guard -- verified by an adversarial probe.
      * char literals, including `'"'` (chat_stream.rs:293) and `'\\''`.
      * lifetimes `'a`, which must NOT be mistaken for a char literal.
    """
    out, i, n = [], 0, len(src)
    def ident(c): return c.isalnum() or c == '_'
    while i < n:
        c = src[i]
        # raw string: r"..." / r#"..."# / br##"..."##
        if c in 'rb':
            j = i
            if src[j] == 'b' and j + 1 < n and src[j+1] == 'r': j += 1
            if src[j] == 'r':
                k = j + 1
                hashes = 0
                while k < n and src[k] == '#': hashes += 1; k += 1
                if k < n and src[k] == '"' and (i == 0 or not ident(src[i-1])):
                    out.append(' ' * (k - i + 1)); i = k + 1
                    term = '"' + '#' * hashes
                    end = src.find(term, i)
                    if end == -1: end = n
                    for ch in src[i:end]: out.append('\n' if ch == '\n' else ' ')
                    out.append(' ' * len(term)); i = min(end + len(term), n)
                    continue
        if c == '"':                      # normal string: escapes apply
            out.append(' '); i += 1
            while i < n:
                if src[i] == '\\': out.append('  '); i += 2; continue
                if src[i] == '"': out.append(' '); i += 1; break
                out.append('\n' if src[i] == '\n' else ' '); i += 1
            continue
        if c == "'":                      # char literal vs lifetime
            is_char = (i + 1 < n and src[i+1] == '\\') or (i + 2 < n and src[i+2] == "'")
            if is_char:
                out.append(' '); i += 1
                while i < n:
                    if src[i] == '\\': out.append('  '); i += 2; continue
                    if src[i] == "'": out.append(' '); i += 1; break
                    out.append(' '); i += 1
                continue
            out.append(c); i += 1; continue
        if src.startswith("//", i):
            while i < n and src[i] != '\n': out.append(' '); i += 1
            continue
        if src.startswith("/*", i):
            depth = 1; out.append('  '); i += 2
            while i < n and depth:
                if src.startswith("/*", i): depth += 1; out.append('  '); i += 2; continue
                if src.startswith("*/", i): depth -= 1; out.append('  '); i += 2; continue
                out.append('\n' if src[i] == '\n' else ' '); i += 1
            continue
        out.append(c); i += 1
    return ''.join(out)

USE_RE = re.compile(r'\b(?:pub\s*(?:\([^)]*\)\s*)?)?use\s', re.S)

def use_statements(src: str):
    for m in USE_RE.finditer(src):
        # must start a statement: only whitespace/{/}/; before it on its line
        ls = src.rfind('\n', 0, m.start()) + 1
        prefix = src[ls:m.start()].lstrip('\ufeff').strip(' \t')
        # An attribute may precede `use` on the same line -- `#[cfg(test)] use ...`
        # was smuggled past the previous filter, which accepted only whitespace or
        # a brace/semicolon. Strip any leading attributes before judging.
        while prefix.startswith('#'):
            close = prefix.find(']')
            if close == -1: break
            prefix = prefix[close + 1:].strip(' \t')
        if prefix not in ('', '}', '{', ';'):
            continue
        i, depth = m.end(), 0
        while i < len(src):
            ch = src[i]
            if ch == '{': depth += 1
            elif ch == '}': depth -= 1
            elif ch == ';' and depth <= 0:
                yield src[m.end():i]
                break
            i += 1

def split_top(s: str):
    parts, depth, cur = [], 0, []
    for ch in s:
        if ch == '{': depth += 1
        elif ch == '}': depth -= 1
        if ch == ',' and depth == 0:
            parts.append(''.join(cur)); cur = []
        else:
            cur.append(ch)
    if ''.join(cur).strip(): parts.append(''.join(cur))
    return parts

def expand(prefix: str, body: str):
    """Yield fully-qualified paths from a use-tree body."""
    body = body.strip()
    if not body: return
    b = body.find('{')
    if b == -1:
        seg = re.sub(r'\s+', '', body.split(' as ')[0])
        yield (prefix + seg).strip(':') if prefix else seg
        return
    head = re.sub(r'\s+', '', body[:b])
    close = len(body) - 1
    depth = 0
    for idx, ch in enumerate(body[b:], start=b):
        if ch == '{': depth += 1
        elif ch == '}':
            depth -= 1
            if depth == 0: close = idx; break
    inner = body[b+1:close]
    for part in split_top(inner):
        yield from expand(prefix + head, part)

def main():
    hits = set()
    for root, dirs, files in os.walk('app/src'):
        dirs[:] = [d for d in dirs if os.path.join(root, d) not in
                   ('app/src/server', 'app/src/cloud_object')]
        if root.startswith('app/src/server') or root.startswith('app/src/cloud_object'):
            continue
        for fn in files:
            if not fn.endswith('.rs'): continue
            path = os.path.join(root, fn)
            try: src = strip_comments(open(path, encoding='utf-8-sig', errors='replace').read())
            except OSError: continue
            for stmt in use_statements(src):
                for p in expand('', stmt):
                    p = p.replace('crate::self::', 'crate::')
                    segs = p.split('::')
                    if len(segs) >= 2 and segs[0] == 'crate' and segs[1] in TARGETS:
                        hits.add(f"{path} {p}")
                    # `use super::super::server::X;` reaches the same modules
                    # without naming `crate`. Resolving it exactly needs the
                    # file's module depth; flagging any super-rooted path with a
                    # `server`/`cloud_object` segment is the conservative
                    # direction for a boundary guard -- a false positive is a
                    # line in the allowlist, a false negative is a silent breach.
                    elif segs[0] in ('super', 'self') and any(x in TARGETS for x in segs[1:]):
                        hits.add(f"{path} {p}")
    for h in sorted(hits): print(h)

main()
