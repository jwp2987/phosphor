#!/bin/bash
# Hook script for Codex Stop event.
# Sends a structured Warp notification when Codex finishes a turn.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/should-use-structured.sh"

if ! should_use_structured; then
    exit 0
fi

source "$SCRIPT_DIR/build-payload.sh"

INPUT=$(cat)

RESPONSE=$(echo "$INPUT" | jq -r '.last_assistant_message // empty' 2>/dev/null)
RESPONSE=$(truncate_chars "$RESPONSE" 200)

TRANSCRIPT_PATH=$(echo "$INPUT" | jq -r '.transcript_path // empty' 2>/dev/null)

BODY=$(build_payload "$INPUT" "stop" \
    --arg response "$RESPONSE" \
    --arg transcript_path "$TRANSCRIPT_PATH")

"$SCRIPT_DIR/warp-notify.sh" "warp://cli-agent" "$BODY"
