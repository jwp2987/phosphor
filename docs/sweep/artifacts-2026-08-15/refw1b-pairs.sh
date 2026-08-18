#!/bin/bash
cd /home/winters/git/phosphor/.worktrees/ref-wave1b
for b in C D E F; do
  base=$(git merge-base repin-shard-$b main)
  for c in $(git rev-list --reverse $base..repin-shard-$b); do
    body=$(git log -1 --format='%b' $c)
    up=""
    for tok in $(echo "$body" | grep -oE '\b[0-9a-f]{9}\b' | head -20); do
      if git cat-file -t "$tok" >/dev/null 2>&1 && [ "$(git cat-file -t $tok)" = commit ]; then
        # must be an ancestor of the new pin and not of repin
        if git merge-base --is-ancestor "$tok" 42effe840 2>/dev/null; then up=$tok; break; fi
      fi
    done
    printf "%s %s %s\n" "$b" "$(git rev-parse --short=9 $c)" "${up:-NONE}"
  done
done
