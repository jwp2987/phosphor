# Hook script for Codex PermissionRequest event.
# Sends a structured Warp notification when Codex needs permission to run a tool.

. "$PSScriptRoot\common.ps1"

if (-not (Test-ShouldUseStructured)) {
    exit 0
}

$inputJson = Read-HookInput
$inputObject = ConvertFrom-JsonSafe $inputJson
$toolName = [string](Get-JsonProperty $inputObject "tool_name" "unknown")
$toolInput = Get-JsonProperty $inputObject "tool_input" $null
if ($null -eq $toolInput) {
    $toolInput = [pscustomobject]@{}
}

$toolPreview = ""
$commandProperty = $toolInput.PSObject.Properties["command"]
$filePathProperty = $toolInput.PSObject.Properties["file_path"]
if ($null -ne $commandProperty -and $null -ne $commandProperty.Value) {
    $toolPreview = [string]$commandProperty.Value
} elseif ($null -ne $filePathProperty -and $null -ne $filePathProperty.Value) {
    $toolPreview = [string]$filePathProperty.Value
} else {
    $toolPreview = Get-TruncatedText ($toolInput | ConvertTo-Json -Compress -Depth 50) 80
}

$summary = "Wants to run $toolName"
if (-not [string]::IsNullOrEmpty($toolPreview)) {
    $summary = "${summary}: $(Get-TruncatedText $toolPreview 120)"
}

$body = New-WarpPayload $inputJson "permission_request" @{
    summary = $summary
    tool_name = $toolName
    tool_input = $toolInput
}

Send-WarpNotification "warp://cli-agent" $body
