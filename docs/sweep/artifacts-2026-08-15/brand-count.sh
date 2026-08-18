#!/usr/bin/env bash
# Brand-term census. Run from the repo root.
for t in "Zap Agent" "Warp Agent" "Phosphor Agent"; do
  printf '%-18s %s\n' "$t" "$(git grep -c -F "$t" -- '*.rs' '*.ftl' '*.toml' '*.json' 2>/dev/null | awk -F: '{s+=$NF} END{print s+0}')"
done
for t in Zap Oz Warp; do
  printf '%-18s %s\n' "$t (word)" "$(git grep -cwE "$t" -- '*.rs' '*.ftl' '*.toml' '*.json' 2>/dev/null | awk -F: '{s+=$NF} END{print s+0}')"
done
