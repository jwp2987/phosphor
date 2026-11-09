# Hook script for Codex UserPromptSubmit event.
# Sends a structured Warp notification when the user submits a prompt.

. "$PSScriptRoot\common.ps1"

if (-not (Test-ShouldUseStructured)) {
    exit 0
}

$inputJson = Read-HookInput
$inputObject = ConvertFrom-JsonSafe $inputJson
$query = [string](Get-JsonProperty $inputObject "prompt" "")
$query = Get-TruncatedText $query 200

$body = New-WarpPayload $inputJson "prompt_submit" @{
    query = $query
}

Send-WarpNotification "warp://cli-agent" $body
