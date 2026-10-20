# Prevent history from being written to file, among other interactive features.
Microsoft.PowerShell.Core\Remove-Module -Name PSReadline -ErrorAction Ignore

$global:_warpOriginalPrompt = $function:global:prompt

if ($PSEdition -eq 'Desktop' -or $IsWindows) {
    $EP = [Microsoft.PowerShell.ExecutionPolicy]
    # MachinePolicy and UserPolicy scopes cannot be overridden. If either is Restricted, there's nothing we can do.
    if ((Microsoft.PowerShell.Security\Get-ExecutionPolicy -Scope MachinePolicy) -eq $EP::Restricted -or (Microsoft.PowerShell.Security\Get-ExecutionPolicy -Scope UserPolicy) -eq $EP::Restricted) {
        Microsoft.PowerShell.Utility\Write-Error 'ExecutionPolicy is Restricted. Unable to Phosphorize this PowerShell session.'
    } elseif ((Microsoft.PowerShell.Security\Get-ExecutionPolicy) -eq $EP::Restricted -and (Microsoft.PowerShell.Security\Get-ExecutionPolicy -Scope MachinePolicy) -eq $EP::Undefined -and (Microsoft.PowerShell.Security\Get-ExecutionPolicy -Scope UserPolicy) -eq $EP::Undefined) {
        $global:_warp_PSProcessExecPolicy = $(Microsoft.PowerShell.Security\Get-ExecutionPolicy -Scope Process)
        Microsoft.PowerShell.Security\Set-ExecutionPolicy -Scope Process -ExecutionPolicy RemoteSigned -Force
    }
}

# We must wait until pwsh attempts to show the first prompt before writing an OSC string.
# Trying to do so beforehand will prevent the "Write-Host" command from being submitted, as pwsh
# ignores submissions prior to the first prompt.
function prompt {
    # Reset the prompt back to the default to avoid infinite loops if sourcing the bootstrap script has an error.
    $function:global:prompt = $global:_warpOriginalPrompt
    $username = [Environment]::UserName
    $global:_warpSessionId = [uint64]@@WARP_SESSION_ID@@
    $msg = Microsoft.PowerShell.Utility\ConvertTo-Json -Compress -InputObject @{ hook = 'InitShell'; value = @{ session_id = $_warpSessionId; shell = 'pwsh'; user = $username; hostname = [System.Net.Dns]::GetHostName() } }
    $encodedMsg = [BitConverter]::ToString([System.Text.Encoding]::UTF8.GetBytes($msg)).Replace('-', '')
    $oscStart = "$([char]0x1b)]9278;"
    $oscEnd = "`a"
    $oscJsonMarker = 'd'
    $oscParameterSeparator = ';'
    Microsoft.PowerShell.Utility\Write-Host "${oscStart}${oscJsonMarker}${oscParameterSeparator}${encodedMsg}${oscEnd}"
    return $null
}
