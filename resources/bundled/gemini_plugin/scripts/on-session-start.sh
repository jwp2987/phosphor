#!/bin/bash
# Hook script for Gemini CLI SessionStart event
# Shows welcome message, Warp detection status, and emits plugin version

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/should-use-structured.sh"

if ! should_use_structured; then
    exit 0
fi

if ! command -v jq &>/dev/null; then
    cat << 'EOF'
{
  "systemMessage": "🚨 Phosphor notifications require jq! Install it with your system package manager (e.g. brew install jq, apt install jq) 🚨"
}
EOF
    exit 0
fi
source "$SCRIPT_DIR/build-payload.sh"

# Read hook input from stdin
INPUT=$(cat)

# Read plugin version from gemini-extension.json
PLUGIN_VERSION=$(jq -r '.version // "unknown"' "$SCRIPT_DIR/../gemini-extension.json" 2>/dev/null)

# Emit structured notification with plugin version so Warp can track it
BODY=$(build_payload "$INPUT" "session_start" \
    --arg plugin_version "$PLUGIN_VERSION")
"$SCRIPT_DIR/warp-notify.sh" "warp://cli-agent" "$BODY"
