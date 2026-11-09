#!/bin/bash
# Hook script for Gemini CLI Notification event
# Handles both idle/input-needed notifications and ToolPermission (approval) notifications.
# (In Claude Code these are separate hooks: Notification + PermissionRequest.
#  In Gemini CLI they are both sub-types of the Notification event.)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/should-use-structured.sh"

if ! should_use_structured; then
    exit 0
fi

source "$SCRIPT_DIR/build-payload.sh"

# Read hook input from stdin
INPUT=$(cat)

NOTIF_TYPE=$(echo "$INPUT" | jq -r '.notification_type // "unknown"' 2>/dev/null)

case "$NOTIF_TYPE" in
    ToolPermission)
        # Permission request — Gemini puts tool info in .details
        TOOL_NAME=$(echo "$INPUT" | jq -r '.details.rootCommand // .details.toolDisplayName // .details.toolName // .details.title // "unknown"' 2>/dev/null)
        TOOL_INPUT=$(echo "$INPUT" | jq -c '.details // {}' 2>/dev/null)
        [ -z "$TOOL_INPUT" ] && TOOL_INPUT='{}'

        # Build a human-readable summary
        TOOL_PREVIEW=$(echo "$INPUT" | jq -r '(.details | if .command then .command elif .filePath then .filePath elif .toolName then .toolName else (.title // "") end)' 2>/dev/null)
        SUMMARY="Wants to run $TOOL_NAME"
        if [ -n "$TOOL_PREVIEW" ] && [ "$TOOL_PREVIEW" != "$TOOL_NAME" ]; then
            TOOL_PREVIEW=$(truncate_chars "$TOOL_PREVIEW" 120)
            SUMMARY="$SUMMARY: $TOOL_PREVIEW"
        fi

        BODY=$(build_payload "$INPUT" "permission_request" \
            --arg summary "$SUMMARY" \
            --arg tool_name "$TOOL_NAME" \
            --argjson tool_input "$TOOL_INPUT")

        "$SCRIPT_DIR/warp-notify.sh" "warp://cli-agent" "$BODY"
        ;;
    *)
        # Generic notification (idle_prompt, etc.)
        MSG=$(echo "$INPUT" | jq -r '.message // "Input needed"' 2>/dev/null)
        [ -z "$MSG" ] && MSG="Input needed"

        BODY=$(build_payload "$INPUT" "$NOTIF_TYPE" \
            --arg summary "$MSG")

        "$SCRIPT_DIR/warp-notify.sh" "warp://cli-agent" "$BODY"
        ;;
esac
