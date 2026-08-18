#!/usr/bin/env bash
set -uo pipefail
W=/home/winters/git/phosphor/.claude/worktrees/agent-ab5f8ca829d9e7921
S=/tmp/claude-1000/-home-winters-git-phosphor/5769cc31-3963-4ecc-8c14-4bbdb07ec540/scratchpad
cd "$W"
PIN=42effe84055f891405b32914af333f14127ec381
git grep -h -A 3 -E '^\s*#\[(tokio::)?(async_std::)?test' "$PIN" -- "*.rs" 2>/dev/null \
  | grep -oE '\bfn [a-z0-9_]+' | sed 's/fn //' | LC_ALL=C sort -u > "$S/pin.names"
git grep -h -A 3 -E '^\s*#\[(tokio::)?(async_std::)?test' HEAD -- "*.rs" 2>/dev/null \
  | grep -oE '\bfn [a-z0-9_]+' | sed 's/fn //' | LC_ALL=C sort -u > "$S/fork.names"
echo "pin names:  $(wc -l < "$S/pin.names")"
echo "fork names: $(wc -l < "$S/fork.names")"
LC_ALL=C comm -23 "$S/pin.names" "$S/fork.names" > "$S/absent.names"
echo "absent:     $(wc -l < "$S/absent.names")"
awk -F'\t' 'NR>1{print $1}' "$W/docs/sweep-verdict-ledger.tsv" | LC_ALL=C sort -u > "$S/ledger.names"
echo "ledger uniq names: $(wc -l < "$S/ledger.names")"
echo "ledger names present in fork: $(LC_ALL=C comm -12 "$S/ledger.names" "$S/fork.names" | wc -l)"
echo "ledger names not in pin:      $(LC_ALL=C comm -23 "$S/ledger.names" "$S/pin.names" | wc -l)"
echo "absent-and-unadjudicated:     $(LC_ALL=C comm -23 "$S/absent.names" "$S/ledger.names" | wc -l)"
