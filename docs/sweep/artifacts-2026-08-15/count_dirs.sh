#!/bin/bash
cd /home/winters/git/phosphor/.claude/worktrees/agent-a4892929ca7a6687d || exit 1
for d in app/src/uri app/src/search app/src/util app/src/notebooks app/src/editor app/src/persistence app/src/ui_components app/src/tab_configs app/src/autoupdate crates/warp_cli crates/onboarding crates/languages crates/input_classifier crates/markdown_parser crates/vim crates/warp_files crates/local_control crates/http_client crates/asset_cache crates/handlebars crates/warp_logging crates/managed_secrets crates/watcher; do
  n=$(git log --oneline 0dbd3d56..02b53fcd8 -- "$d" | wc -l)
  echo "$n $d"
done | sort -rn
