#!/usr/bin/env bash
# Block until the precheck run finishes, then print its verdict.
LOG=/tmp/claude-1000/-home-winters-git-phosphor/5769cc31-3963-4ecc-8c14-4bbdb07ec540/scratchpad/precheck.log
while pgrep -f "bash ./script/precheck" >/dev/null 2>&1; do
  sleep 20
done
sed 's/\x1b\[[0-9;]*m//g' "$LOG" | grep -E "^== |^  ok|^  FAIL|^  warn|^precheck|^EXIT"
