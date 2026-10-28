if ($PSEdition -eq 'Desktop' -or $IsWindows) {
    $EP = [Microsoft.PowerShell.ExecutionPolicy]
    if ((Microsoft.PowerShell.Security\Get-ExecutionPolicy -Scope MachinePolicy) -eq $EP::Restricted -or (Microsoft.PowerShell.Security\Get-ExecutionPolicy -Scope UserPolicy) -eq $EP::Restricted) {
        Microsoft.PowerShell.Utility\Write-Error 'ExecutionPolicy is Restricted. Unable to Phosphorize this PowerShell session.'
    } elseif ((Microsoft.PowerShell.Security\Get-ExecutionPolicy) -eq $EP::Restricted -and (Microsoft.PowerShell.Security\Get-ExecutionPolicy -Scope MachinePolicy) -eq $EP::Undefined -and (Microsoft.PowerShell.Security\Get-ExecutionPolicy -Scope UserPolicy) -eq $EP::Undefined) {
        $global:_warp_PSProcessExecPolicy = $(Microsoft.PowerShell.Security\Get-ExecutionPolicy -Scope Process)
        Microsoft.PowerShell.Security\Set-ExecutionPolicy -Scope Process -ExecutionPolicy RemoteSigned -Force
    }
}
$global:WARP_IS_SUBSHELL = 1
$global:_warpSessionId = [uint64]@@WARP_SESSION_ID@@
$_warpUser = [Environment]::UserName
$_warpHostname = [System.Net.Dns]::GetHostName()
$_warpMsg = Microsoft.PowerShell.Utility\ConvertTo-Json -Compress -InputObject @{ hook = 'InitShell'; value = @{ session_id = $_warpSessionId; shell = 'pwsh'; user = $_warpUser; hostname = $_warpHostname; is_subshell = $true } }
$_warpEncodedMsg = [BitConverter]::ToString([System.Text.Encoding]::UTF8.GetBytes($_warpMsg)).Replace('-', '')
Microsoft.PowerShell.Utility\Write-Host "$([char]0x1b)]9278;d;${_warpEncodedMsg}`a"
Microsoft.PowerShell.Utility\Remove-Variable _warpUser, _warpHostname, _warpMsg, _warpEncodedMsg -ErrorAction Ignore
