#!/usr/bin/env bash
set -uo pipefail
W=/home/winters/git/phosphor/.claude/worktrees/agent-ab5f8ca829d9e7921
S=/tmp/claude-1000/-home-winters-git-phosphor/5769cc31-3963-4ecc-8c14-4bbdb07ec540/scratchpad
cd "$W"
PIN=42effe84055f891405b32914af333f14127ec381
OLD=02b53fcd8
LEDGER=docs/sweep-verdict-ledger.tsv

# ledger file list
awk -F'\t' 'NR>1{print $2}' "$LEDGER" | LC_ALL=C sort -u > "$S/ledger.files"
echo "ledger files: $(wc -l < "$S/ledger.files")"

# changed test-bearing files at the diff
git diff --name-status "$OLD" "$PIN" -- '*.rs' | awk -F'\t' '$1 !~ /^D/{print $2}' | LC_ALL=C sort -u > "$S/changed.files"
echo "changed .rs (non-deleted): $(wc -l < "$S/changed.files")"
LC_ALL=C comm -12 "$S/ledger.files" "$S/changed.files" > "$S/ledger.changed"
echo "ledger files that changed: $(wc -l < "$S/ledger.changed")"

extract() { # $1 = path ; prints test fn names at the pin
    local content
    content="$(git show "$PIN:$1" 2>/dev/null)" || return 0
    [[ -n "$content" ]] || return 0
    grep -A 3 -E '^\s*#\[(tokio::)?(async_std::)?test' <<<"$content" \
        | grep -oE '\bfn [a-z0-9_]+' | sed 's/fn //' | LC_ALL=C sort -u
}

run() { # $1 = file list, $2 = label
    local total=0 files=0
    : > "$S/detail.$2"
    while IFS= read -r f; do
        [[ -z "$f" ]] && continue
        extract "$f" > "$S/t.pin"
        [[ -s "$S/t.pin" ]] || continue
        awk -F'\t' -v f="$f" 'NR>1 && $2==f {print $1}' "$LEDGER" | LC_ALL=C sort -u > "$S/t.led"
        # pin tests, minus ledger rows for this file, minus names the fork already has
        LC_ALL=C comm -23 "$S/t.pin" "$S/t.led" > "$S/t.a"
        LC_ALL=C comm -23 "$S/t.a" "$S/fork.names" > "$S/t.gap"
        n=$(wc -l < "$S/t.gap")
        if (( n > 0 )); then
            files=$((files+1)); total=$((total+n))
            { printf '%s\t%d\t' "$f" "$n"; tr '\n' ' ' < "$S/t.gap"; echo; } >> "$S/detail.$2"
        fi
    done < "$1"
    echo "[$2] files=$files tests=$total"
}

run "$S/ledger.changed" changed
run "$S/ledger.files" allledger
echo "--- top 12 (all ledger files) ---"
sort -t$'\t' -k2,2nr "$S/detail.allledger" | head -12 | cut -c1-160
