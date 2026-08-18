#!/usr/bin/env bash
# Non-vacuity check: run the COVERAGE GAP's exact three filters against the
# ledger AS IT STOOD AT THE OLD PIN (rows whose pin_commit is 02b53fcd8), and
# against the fork as it stood at the old pin's merge-base, to confirm the
# section would have reported the hundreds of tests issue #592 measured by hand
# -- i.e. that today's 17 is a small number because the manual pass was done,
# not because the filters are broken.
set -uo pipefail
W=/home/winters/git/phosphor/.claude/worktrees/agent-ab5f8ca829d9e7921
S=/tmp/claude-1000/-home-winters-git-phosphor/5769cc31-3963-4ecc-8c14-4bbdb07ec540/scratchpad
cd "$W"
PIN=42effe84055f891405b32914af333f14127ec381
LEDGER=docs/sweep-verdict-ledger.tsv

# pin pairs / fork names reused from proto4
awk -F'\t' -v OFS='\t' 'NR>1 && $7=="02b53fcd8" {print $2, $1}' "$LEDGER" | LC_ALL=C sort -u > "$S/led.pairs.old"
awk -F'\t' 'NR>1 && $7=="02b53fcd8" {print $2}' "$LEDGER" | LC_ALL=C sort -u > "$S/led.files.old"
echo "ledger rows as of the old pin: $(wc -l < "$S/led.pairs.old") across $(wc -l < "$S/led.files.old") files"

awk -F'\t' 'NR==FNR { have[$0]=1; next } !($2 in have)' "$S/fork.names" "$S/pin.pairs" > "$S/abs.old"
awk -F'\t' 'NR==FNR { row[$1 FS $2]=1; next } !(($1 FS $2) in row)' "$S/led.pairs.old" "$S/abs.old" > "$S/gap.old"
awk -F'\t' 'NR==FNR { swept[$0]=1; next } ($1 in swept)' "$S/led.files.old" "$S/gap.old" > "$S/gapcov.old"
echo "COVERAGE GAP with only the old pin's ledger rows: $(wc -l < "$S/gapcov.old") tests across $(cut -f1 "$S/gapcov.old" | sort -u | wc -l) files"
echo "top offenders:"
cut -f1 "$S/gapcov.old" | sort | uniq -c | sort -rn | head -8
