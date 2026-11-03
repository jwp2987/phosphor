# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
param([string]$Boot)
$ErrorActionPreference='Continue'
$global:_warpSessionId='123456'
$global:WARP_BOOTSTRAPPED=1
function global:prompt { 'PS C:\x> ' }
$txt=(Get-Content -Raw $Boot).Replace('$null = Microsoft.PowerShell.Core\New-Module','$global:WM = Microsoft.PowerShell.Core\New-Module')
[void][scriptblock]::Create($txt)
$sb0=[scriptblock]::Create($txt); $null = & { . $sb0 } 2>&1
$m = $global:WM
function T($name,[scriptblock]$sb,$n=20){ [IO.File]::AppendAllText("/tmp/phosphor-realapp/prog.txt",$name+"`n");
  $null = & $m $sb
  $sw=[Diagnostics.Stopwatch]::new(); $t=@()
  for($i=0;$i -lt $n;$i++){ $sw.Restart(); $null = & $m $sb; $sw.Stop(); $t+=$sw.Elapsed.TotalMilliseconds }
  $t=$t|sort; $script:res += ("{0,-28} med {1,7:N2}  mean {2,7:N2} max {3,7:N2}" -f $name,$t[[int]($n/2)],($t|measure -Average).Average,$t[-1])
}
$o=[Console]::Out
$script:res=@("PS $($PSVersionTable.PSVersion)")
T 'empty-baseline' { 1 }
T 'Send-Json(Preexec)' { Warp-Send-JsonMessage @{hook='Preexec';value=@{session_id='1';command='echo hi'}} } 
T 'ResetGrid' { Warp-Send-ResetGridOSC }
T 'Preexec' { Warp-Preexec 'echo hi' }
T 'Precmd' { Warp-Precmd -status $true -code 0 }
T 'Render-Prompt' { Warp-Render-Prompt -status $true -code 0 -isGeneratorCommand $false }
T 'Decorate-Prompt' { Warp-Decorate-Prompt -basePrompt 'PS C:\x> ' }
T 'Set-ExtraPromptLineCount' { PSReadLine\Set-PSReadLineOption -ExtraPromptLineCount 0 }
T 'Prompt(total)' { Warp-Prompt }
T 'Idle-Maintenance' { Warp-Idle-Maintenance }
T 'PSRL-IfChanged' { Warp-Configure-PSReadLine-IfChanged }
T 'GitHeadInProcess' { Warp-GitHeadInProcess }
T 'Get-PSReadLineState' { Warp-Get-PSReadLineState }
T 'ConvertTo-Json' { ConvertTo-Json -InputObject @{hook='Precmd';value=@{a='1';b='2'}} -Compress }
T 'EncodeHex(300B)' { Warp-Encode-HexString ('x'*300) }

$script:res | Set-Content /tmp/phosphor-realapp/prof_local.txt
