#!/bin/bash
cd /home/winters/git/phosphor/.worktrees/ref-wave1b
fork=$1; up=$2
git show "$up" --format='' | grep -E '^-' | grep -v '^---' | sed -E 's/^-//' | sed -E 's/^[[:space:]]+//; s/[[:space:]]+$//' \
  | grep -vE '^$' | grep -vE '^(\}|\{|\)|\);|\},|\}\)|\}\);|//|#|\]|\],|use .*;)$' | awk 'length($0)>25' | LC_ALL=C sort -u > /tmp/refw1b_updel.txt
git show "$fork" --format='' | grep -E '^-' | grep -v '^---' | sed -E 's/^-//' | sed -E 's/^[[:space:]]+//; s/[[:space:]]+$//' \
  | grep -vE '^$' | LC_ALL=C sort -u > /tmp/refw1b_fkdel.txt
LC_ALL=C comm -23 /tmp/refw1b_updel.txt /tmp/refw1b_fkdel.txt > /tmp/refw1b_notdeleted.txt
still=0; : > /tmp/refw1b_stillthere.txt
while IFS= read -r line; do
  if grep -rqF -- "$line" crates app lib script docker .agents .github 2>/dev/null; then echo "$line" >> /tmp/refw1b_stillthere.txt; still=$((still+1)); fi
done < /tmp/refw1b_notdeleted.txt
echo "DEL-CHECK $fork <- $up : upstream_removed=$(wc -l < /tmp/refw1b_updel.txt) not_removed_and_still_in_tree=$still"
cat /tmp/refw1b_stillthere.txt
