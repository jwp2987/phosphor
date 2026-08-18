#!/bin/bash
# usage: refw1b-tree.sh <upsha> [pathfilter...]
cd /home/winters/git/phosphor/.worktrees/ref-wave1b
up=$1; shift
git show "$up" --format='' -- "$@" | grep -E '^\+' | grep -v '^+++' | sed -E 's/^\+//' \
 | sed -E 's/^[[:space:]]+//; s/[[:space:]]+$//' | grep -vE '^$' \
 | grep -vE '^(\}|\{|\)|\);|\},|\}\)|\}\);|\},?\)?;?|//|#|\]|\],|use .*;)$' \
 | LC_ALL=C sort -u > /tmp/refw1b_uplines.txt
total=$(wc -l < /tmp/refw1b_uplines.txt)
missing=0
: > /tmp/refw1b_treemiss.txt
while IFS= read -r line; do
  if ! grep -rqF -- "$line" crates app lib script docker .github 2>/dev/null; then
    echo "$line" >> /tmp/refw1b_treemiss.txt; missing=$((missing+1))
  fi
done < /tmp/refw1b_uplines.txt
echo "TREE-CHECK $up: upstream_added=$total absent_from_fork_tree=$missing"
cat /tmp/refw1b_treemiss.txt
