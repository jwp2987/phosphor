#!/bin/bash
cd /home/winters/git/phosphor/.claude/worktrees/agent-a200a62845ff9c504
git log --format='%H %s' 0dbd3d56..02b53fcd8 -- app/src/workspace/ app/src/pane_group/ app/src/settings/ app/src/settings_view/ crates/warp_tui/ > /tmp/claude-1000/-home-winters-git-phosphor/5769cc31-3963-4ecc-8c14-4bbdb07ec540/scratchpad/all_commits.txt
wc -l /tmp/claude-1000/-home-winters-git-phosphor/5769cc31-3963-4ecc-8c14-4bbdb07ec540/scratchpad/all_commits.txt
