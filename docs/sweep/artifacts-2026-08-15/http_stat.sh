#!/bin/bash
cd /home/winters/git/phosphor/.claude/worktrees/agent-a4892929ca7a6687d || exit 1
for c in b491ddaf2 cf3ad092f 1a3bdee4a 086150b87 4aea06734 252afbd62 0b0318a32 3f53f3c62 1148ae3e8 57e8e3e9b; do
  echo "== $c =="
  git show --stat $c -- crates/http_client | head -8
done
