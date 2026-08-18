#!/bin/bash
cd /home/winters/git/phosphor/.claude/worktrees/agent-a75db409b6b4decd1 || exit 1
for h in "$@"; do
  echo "=== $h ==="
  git show --stat "$h" | sed -n '1,4p'
  git show --stat "$h" | tail -12
  echo
done
