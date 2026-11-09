#!/bin/bash
# Hook script for Gemini CLI AfterAgent event (equivalent to Claude Code's Stop)
# Sends a structured Warp notification when Gemini completes a task

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/should-use-structured.sh"

if ! should_use_structured; then
    exit 0
fi

source "$SCRIPT_DIR/build-payload.sh"

# Read hook input from stdin
INPUT=$(cat)

# Skip if a stop hook is already active (prevents double-notification on retries)
STOP_HOOK_ACTIVE=$(echo "$INPUT" | jq -r '.stop_hook_active // false' 2>/dev/null)
if [ "$STOP_HOOK_ACTIVE" = "true" ]; then
    exit 0
fi

# Gemini's AfterAgent provides prompt and prompt_response directly — no transcript parsing needed.
QUERY=$(echo "$INPUT" | jq -r '.prompt // empty' 2>/dev/null)
RESPONSE=$(echo "$INPUT" | jq -r '.prompt_response // empty' 2>/dev/null)
TRANSCRIPT_PATH=$(echo "$INPUT" | jq -r '.transcript_path // empty' 2>/dev/null)

# Truncate for notification display
QUERY=$(truncate_chars "$QUERY" 200)
RESPONSE=$(truncate_chars "$RESPONSE" 200)

BODY=$(build_payload "$INPUT" "stop" \
    --arg query "$QUERY" \
    --arg response "$RESPONSE" \
    --arg transcript_path "$TRANSCRIPT_PATH")

"$SCRIPT_DIR/warp-notify.sh" "warp://cli-agent" "$BODY"

# Output empty JSON so we don't interfere with the agent
echo '{}'
