#!/bin/bash
# usage: refw1b-cmp.sh <forksha> <upsha>
cd /home/winters/git/phosphor/.worktrees/ref-wave1b
fork=$1; up=$2
norm() { grep -E '^\+' | sed -E 's/^\+//' | sed -E 's/[[:space:]]+/ /g; s/^ //; s/ $//' | grep -vE '^$' | grep -vE '^(\}|\{|\)|\);|\},|\}\)|\}\);|\},?\)?;?|//|#)$' | LC_ALL=C sort -u; }
git show "$up" --format='' | grep -v '^+++' | norm > /tmp/refw1b_up.txt
git show "$fork" --format='' | grep -v '^+++' | norm > /tmp/refw1b_fk.txt
LC_ALL=C comm -23 /tmp/refw1b_up.txt /tmp/refw1b_fk.txt > /tmp/refw1b_missing.txt
tot=$(wc -l < /tmp/refw1b_up.txt); miss=$(wc -l < /tmp/refw1b_missing.txt)
echo "PAIR $fork <- $up : upstream_added=$tot missing_from_fork_diff=$miss"
cat /tmp/refw1b_missing.txt
