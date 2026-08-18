#!/bin/bash
SCRATCH=/tmp/claude-1000/-home-winters-git-phosphor/5769cc31-3963-4ecc-8c14-4bbdb07ec540/scratchpad
while IFS='|' read -r hash date subj; do
  echo "=== $hash $date $subj ==="
  git show --stat "$hash" -- crates/warpui_core | tail -n +6
  echo
done < "$SCRATCH/commits_full.txt" > "$SCRATCH/all_stats.txt"
wc -l "$SCRATCH/all_stats.txt"
