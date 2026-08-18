#!/bin/bash
# For each commit hash on stdin, print hash + count of in-scope files touched + total files touched.
cd /home/winters/git/phosphor/.claude/worktrees/agent-a75db409b6b4decd1 || exit 1
while read -r h; do
  [ -z "$h" ] && continue
  files=$(git show --format='' --name-only "$h" 2>/dev/null)
  total=$(echo "$files" | grep -c .)
  inscope=$(echo "$files" | grep -cE '^app/src/ai/(agent/|agent_providers/|agent_sdk/|agent_events/|byop_compaction/|byop_readiness/|agent_conversations_model)')
  echo "$h $inscope $total"
done
