#!/bin/bash
SCRATCH=/tmp/claude-1000/-home-winters-git-phosphor/5769cc31-3963-4ecc-8c14-4bbdb07ec540/scratchpad
while IFS='|' read -r hash date subj; do
  echo "=== $hash $date $subj ==="
  git diff-tree --no-commit-id --name-status -r "$hash" -- crates/warpui_core
  echo
done < "$SCRATCH/commits_full.txt" > "$SCRATCH/all_names.txt"
wc -l "$SCRATCH/all_names.txt"
