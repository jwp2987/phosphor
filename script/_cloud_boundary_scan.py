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
    out, i, n = [], 0, len(src)
    while i < n:
        c = src[i]
        if c == '"':                      # skip string literal
            out.append(c); i += 1
            while i < n:
                if src[i] == '\\': out.append('  '); i += 2; continue
                if src[i] == '"': out.append('"'); i += 1; break
                out.append(' ' if src[i] != '\n' else '\n'); i += 1
            continue
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
        if src[ls:m.start()].strip(' \t') not in ('', '}', '{', ';'):
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
            try: src = strip_comments(open(path, encoding='utf-8', errors='replace').read())
            except OSError: continue
            for stmt in use_statements(src):
                for p in expand('', stmt):
                    p = p.replace('crate::self::', 'crate::')
                    segs = p.split('::')
                    if len(segs) >= 2 and segs[0] == 'crate' and segs[1] in TARGETS:
                        hits.add(f"{path} {p}")
    for h in sorted(hits): print(h)

main()
