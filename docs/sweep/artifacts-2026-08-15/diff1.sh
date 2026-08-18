#!/bin/bash
cd /home/winters/git/phosphor/.claude/worktrees/agent-a38ae80da838457b3 || exit 1
for f in mod.rs utils.rs event.rs keycode.rs clipboard.rs menus.rs notification.rs; do
echo "=== $f ==="
diff <(git show f60116d3eee^:crates/warpui/src/platform/mac/$f) crates/warpui/src/platform/mac/$f
echo
done
