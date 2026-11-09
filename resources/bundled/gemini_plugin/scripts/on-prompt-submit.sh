#!/bin/bash
# Hook script for Gemini CLI BeforeAgent event (equivalent to Claude Code's UserPromptSubmit)
# Sends a structured Warp notification when the user submits a prompt,
# transitioning the session status from idle/blocked back to running.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/should-use-structured.sh"

if ! should_use_structured; then
    echo '{}'
    exit 0
fi

source "$SCRIPT_DIR/build-payload.sh"

# Read hook input from stdin
INPUT=$(cat)

# Extract the user's prompt
QUERY=$(echo "$INPUT" | jq -r '.prompt // empty' 2>/dev/null)
QUERY=$(truncate_chars "$QUERY" 200)

BODY=$(build_payload "$INPUT" "prompt_submit" \
    --arg query "$QUERY")

"$SCRIPT_DIR/warp-notify.sh" "warp://cli-agent" "$BODY"

# Output empty JSON so we don't interfere with the agent
echo '{}'
