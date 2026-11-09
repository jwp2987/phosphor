# Hook script for Codex PostToolUse event.
# Sends a structured Warp notification after a tool call completes.

. "$PSScriptRoot\common.ps1"

if (-not (Test-ShouldUseStructured)) {
    exit 0
}

$inputJson = Read-HookInput
$inputObject = ConvertFrom-JsonSafe $inputJson
$toolName = [string](Get-JsonProperty $inputObject "tool_name" "")

$body = New-WarpPayload $inputJson "tool_complete" @{
    tool_name = $toolName
}

Send-WarpNotification "warp://cli-agent" $body
