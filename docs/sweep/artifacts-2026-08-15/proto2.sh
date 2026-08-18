#!/usr/bin/env bash
set -uo pipefail
W=/home/winters/git/phosphor/.claude/worktrees/agent-ab5f8ca829d9e7921
S=/tmp/claude-1000/-home-winters-git-phosphor/5769cc31-3963-4ecc-8c14-4bbdb07ec540/scratchpad
cd "$W"
PIN=42effe84055f891405b32914af333f14127ec381
LEDGER=docs/sweep-verdict-ledger.tsv

# A. workspace-wide absent-and-unledgered names, split by whether their pin file
#    has ledger rows.  Build a name->file map at the pin.
git grep -h -A 3 -E '^\s*#\[(tokio::)?(async_std::)?test' "$PIN" -- "*.rs" 2>/dev/null > /dev/null
git grep -A 3 -E '^\s*#\[(tokio::)?(async_std::)?test' "$PIN" -- "*.rs" 2>/dev/null \
  | grep -oE '^[^:]+:[^:]*:?\s*(pub )?(async )?fn [a-z0-9_]+' > "$S/raw.map" || true
head -3 "$S/raw.map"
wc -l < "$S/raw.map"
