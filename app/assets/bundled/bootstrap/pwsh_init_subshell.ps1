$global:WARP_IS_SUBSHELL = 1
$global:_warpSessionId = [uint64]@@WARP_SESSION_ID@@
$_warpUser = [Environment]::UserName
$_warpHostname = [System.Net.Dns]::GetHostName()
$_warpMsg = Microsoft.PowerShell.Utility\ConvertTo-Json -Compress -InputObject @{ hook = 'InitShell'; value = @{ session_id = $_warpSessionId; shell = 'pwsh'; user = $_warpUser; hostname = $_warpHostname; is_subshell = $true } }
$_warpEncodedMsg = [BitConverter]::ToString([System.Text.Encoding]::UTF8.GetBytes($_warpMsg)).Replace('-', '')
Microsoft.PowerShell.Utility\Write-Host "$([char]0x1b)]9278;d;${_warpEncodedMsg}`a"
Microsoft.PowerShell.Utility\Remove-Variable _warpUser, _warpHostname, _warpMsg, _warpEncodedMsg -ErrorAction Ignore
