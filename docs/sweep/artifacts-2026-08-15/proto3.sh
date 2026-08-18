#!/usr/bin/env bash
set -uo pipefail
W=/home/winters/git/phosphor/.claude/worktrees/agent-ab5f8ca829d9e7921
S=/tmp/claude-1000/-home-winters-git-phosphor/5769cc31-3963-4ecc-8c14-4bbdb07ec540/scratchpad
cd "$W"
PIN=42effe84055f891405b32914af333f14127ec381
LEDGER=docs/sweep-verdict-ledger.tsv

# pin: file<TAB>test  (script/state's three-attribute idiom)
git grep -A 3 -E '^\s*#\[(tokio::)?(async_std::)?test' "$PIN" -- "*.rs" 2>/dev/null \
  | grep -oE '^[0-9a-f]+:[^ ]+\.rs.fn [a-z0-9_]+' \
  | sed -E "s/^[0-9a-f]+://; s/\.rs.fn /.rs\t/" \
  | LC_ALL=C sort -u > "$S/pin.pairs"
echo "pin pairs: $(wc -l < "$S/pin.pairs")"
cut -f2 "$S/pin.pairs" | LC_ALL=C sort -u | wc -l

# same with the four-attribute SCOPE set, for the under-report note
git grep -A 3 -E '^\s*#\[[[:space:]]*(test|tokio::test|async_std::test|gpui::test|rstest|test_case)' "$PIN" -- "*.rs" 2>/dev/null \
  | grep -oE '^[0-9a-f]+:[^ ]+\.rs.fn [a-z0-9_]+' \
  | sed -E "s/^[0-9a-f]+://; s/\.rs.fn /.rs\t/" \
  | LC_ALL=C sort -u > "$S/pin.pairs4"
echo "pin pairs (4-attr): $(wc -l < "$S/pin.pairs4")"

awk -F'\t' 'NR>1{print $2"\t"$1}' "$LEDGER" | LC_ALL=C sort -u > "$S/led.pairs"
awk -F'\t' 'NR>1{print $2}' "$LEDGER" | LC_ALL=C sort -u > "$S/led.files"

# absent-from-fork pin pairs
LC_ALL=C join -t$'\t' -1 2 -2 1 -o 1.1,1.2 \
  <(LC_ALL=C sort -t$'\t' -k2,2 "$S/pin.pairs") "$S/fork.names" 2>/dev/null \
  | LC_ALL=C sort -u > "$S/present.pairs"
LC_ALL=C comm -23 "$S/pin.pairs" "$S/present.pairs" > "$S/absent.pairs"
echo "absent pairs (file,test): $(wc -l < "$S/absent.pairs")"
cut -f2 "$S/absent.pairs" | LC_ALL=C sort -u | wc -l

# absent pairs with no ledger row for that exact (file,test)
LC_ALL=C comm -23 "$S/absent.pairs" "$S/led.pairs" > "$S/gap.pairs"
echo "absent + no ledger row for that (file,test): $(wc -l < "$S/gap.pairs")"

# split by whether the file has any ledger rows
LC_ALL=C join -t$'\t' -1 1 -2 1 -o 1.1,1.2 "$S/gap.pairs" "$S/led.files" > "$S/gap.inledgerfiles"
echo "  ... in files that HAVE ledger rows: $(wc -l < "$S/gap.inledgerfiles") across $(cut -f1 "$S/gap.inledgerfiles" | sort -u | wc -l) files"
LC_ALL=C comm -23 "$S/gap.pairs" "$S/gap.inledgerfiles" > "$S/gap.noledgerfile"
echo "  ... in files with NO ledger rows:   $(wc -l < "$S/gap.noledgerfile") across $(cut -f1 "$S/gap.noledgerfile" | sort -u | wc -l) files"

echo "--- unique names in the in-ledger-file gap ---"
cut -f2 "$S/gap.inledgerfiles" | LC_ALL=C sort -u | wc -l
echo "--- per-file ---"
cut -f1 "$S/gap.inledgerfiles" | sort | uniq -c | sort -rn
