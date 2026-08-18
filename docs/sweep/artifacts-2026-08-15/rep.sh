#!/usr/bin/env bash
# Run the mermaid test alone N times and print a pass/fail tally.
cd /home/winters/git/phosphor/.claude/worktrees/agent-a93a86435a60018dc || exit 1
N="${1:-5}"
pass=0; fail=0
for i in $(seq 1 "$N"); do
  if WARP_SHELL_PATH=/bin/zsh cargo nextest run -p integration --no-fail-fast -j 2 --retries 0 \
      -E 'test(test_backspace_inside_rendered_mermaid_block_is_atomic)' >/tmp/claude-1000/-home-winters-git-phosphor/5769cc31-3963-4ecc-8c14-4bbdb07ec540/scratchpad/rep_$i.log 2>&1; then
    pass=$((pass+1)); echo "run $i: PASS"
  else
    fail=$((fail+1)); echo "run $i: FAIL"
  fi
done
echo "TALLY at $(git rev-parse --short HEAD): pass=$pass fail=$fail"
