#!/usr/bin/env bash
set -uo pipefail
W=/home/winters/git/phosphor/.claude/worktrees/agent-ab5f8ca829d9e7921
S=/tmp/claude-1000/-home-winters-git-phosphor/5769cc31-3963-4ecc-8c14-4bbdb07ec540/scratchpad
cd "$W"
PIN=42effe84055f891405b32914af333f14127ec381
LEDGER=docs/sweep-verdict-ledger.tsv

pairs() { # $1 = attribute regex -> file<TAB>test pairs at the pin
  git grep -A 3 -E "$1" "$PIN" -- "*.rs" 2>/dev/null \
    | awk -v OFS='\t' '
        match($0, /\.rs[-:]/) {
            file = substr($0, 1, RSTART + 2)
            sub(/^[0-9a-f]+:/, "", file)
            rest = " " substr($0, RSTART + 4)
            if (match(rest, /[^A-Za-z0-9_]fn [a-z0-9_]+/))
                print file, substr(rest, RSTART + 4, RLENGTH - 4)
        }' \
    | LC_ALL=C sort -u
}

pairs '^\s*#\[(tokio::)?(async_std::)?test' > "$S/pin.pairs"
echo "3-attr pin pairs: $(wc -l < "$S/pin.pairs"), unique names: $(cut -f2 "$S/pin.pairs" | LC_ALL=C sort -u | wc -l)  (script/state says 10860)"

pairs '^\s*#\[[[:space:]]*(test|tokio::test|async_std::test|gpui::test|rstest|test_case)' > "$S/pin.pairs4"
echo "4-attr pin pairs: $(wc -l < "$S/pin.pairs4"), unique names: $(cut -f2 "$S/pin.pairs4" | LC_ALL=C sort -u | wc -l)"
echo "extra pairs from gpui/rstest/test_case: $(LC_ALL=C comm -13 "$S/pin.pairs" "$S/pin.pairs4" | wc -l)"

awk -F'\t' 'NR>1{print $2"\t"$1}' "$LEDGER" | LC_ALL=C sort -u > "$S/led.pairs"
awk -F'\t' 'NR>1{print $2}' "$LEDGER" | LC_ALL=C sort -u > "$S/led.files"

# drop pairs whose test NAME exists in the fork
LC_ALL=C join -t$'\t' -1 2 -2 1 -o 1.1,1.2 \
  <(LC_ALL=C sort -t$'\t' -k2,2 "$S/pin.pairs") "$S/fork.names" | LC_ALL=C sort -u > "$S/present.pairs"
LC_ALL=C comm -23 "$S/pin.pairs" "$S/present.pairs" > "$S/absent.pairs"
echo "absent pairs: $(wc -l < "$S/absent.pairs"), unique names: $(cut -f2 "$S/absent.pairs" | LC_ALL=C sort -u | wc -l)  (script/state says 2795)"

LC_ALL=C comm -23 "$S/absent.pairs" "$S/led.pairs" > "$S/gap.pairs"
echo "absent + no ledger row for that (file,test): $(wc -l < "$S/gap.pairs") pairs, $(cut -f2 "$S/gap.pairs" | LC_ALL=C sort -u | wc -l) names"
LC_ALL=C join -t$'\t' -1 1 -2 1 -o 1.1,1.2 "$S/gap.pairs" "$S/led.files" > "$S/gap.inledgerfiles"
echo "  in files WITH ledger rows: $(wc -l < "$S/gap.inledgerfiles") across $(cut -f1 "$S/gap.inledgerfiles" | sort -u | wc -l) files"
LC_ALL=C comm -23 "$S/gap.pairs" "$S/gap.inledgerfiles" > "$S/gap.noledgerfile"
echo "  in files with NO ledger rows: $(wc -l < "$S/gap.noledgerfile") across $(cut -f1 "$S/gap.noledgerfile" | sort -u | wc -l) files"
echo "--- per-file, ledger-covered files ---"
cut -f1 "$S/gap.inledgerfiles" | sort | uniq -c | sort -rn
