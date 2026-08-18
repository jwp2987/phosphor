#!/bin/bash
cd /home/winters/git/phosphor/.claude/worktrees/agent-a38ae80da838457b3 || exit 1
for f in $(git diff --name-only 89ce4dfd0 port/objc2 -- '*.rs'); do
  if [ -f "$f" ]; then
    out=$(rustfmt --check --edition 2024 --config-path .rustfmt.toml "$f" 2>&1)
    if [ -n "$out" ]; then
      echo "=== $f ==="
    fi
  fi
done
echo "SCAN COMPLETE"
