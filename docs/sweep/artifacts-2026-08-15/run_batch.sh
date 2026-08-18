#!/bin/bash
SCRATCH=/tmp/claude-1000/-home-winters-git-phosphor/5769cc31-3963-4ecc-8c14-4bbdb07ec540/scratchpad
while IFS= read -r hash; do
  bash "$SCRATCH/check_commit.sh" "$hash"
done < "$SCRATCH/batch_hashes.txt"
