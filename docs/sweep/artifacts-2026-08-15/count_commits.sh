#!/bin/bash
cd /home/winters/git/phosphor/.claude/worktrees/agent-a200a62845ff9c504
for p in app/src/workspace app/src/pane_group app/src/settings app/src/settings_view crates/warp_tui; do
  echo "$p: $(git log --format=%H 0dbd3d56..02b53fcd8 -- $p | wc -l)"
done
