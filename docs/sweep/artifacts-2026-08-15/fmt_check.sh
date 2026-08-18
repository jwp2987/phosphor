#!/bin/bash
cd /home/winters/git/phosphor/.claude/worktrees/agent-a4892929ca7a6687d || exit 1
for f in app/src/tab_configs/session_config.rs app/src/tab_configs/session_config_tests.rs app/src/tab_configs/tab_config.rs app/src/tab_configs/tab_config_tests.rs app/src/user_config/mod_test.rs; do
  echo "== $f =="
  rustfmt --check --config-path .rustfmt.toml "$f" 2>&1 | grep -A8 "^Diff in.*$(basename "$f")"
done
