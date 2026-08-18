#!/usr/bin/env bash
# Block up to ~9 minutes waiting for precheck to exit. Prints DONE or STILL_RUNNING.
LOG=/tmp/claude-1000/-home-winters-git-phosphor/5769cc31-3963-4ecc-8c14-4bbdb07ec540/scratchpad/precheck2.log
deadline=$((SECONDS + 540))
while (( SECONDS < deadline )); do
  if ! pgrep -f "bash ./script/precheck" >/dev/null 2>&1; then
    echo "DONE"
    sed 's/\x1b\[[0-9;]*m//g' "$LOG" | grep -E "^== |^  ok|^  FAIL|^  warn|^precheck"
    exit 0
  fi
  sleep 15
done
echo "STILL_RUNNING"
sed 's/\x1b\[[0-9;]*m//g' "$LOG" | grep -E "^== |^  ok|^  FAIL|^  warn" | tail -4
tail -2 "$LOG"
