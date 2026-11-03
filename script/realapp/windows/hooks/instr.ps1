# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
# stage-split instrumentation; dot-source inside a Phosphor PowerShell tab
$global:_hk = @{ enter=0; ret=0; pre0=0; pre1=0; p0=0; pc1=0; p1=0; cend=0 }
function global:_hkNow { ([DateTime]::UtcNow.Ticks - 621355968000000000) / 10000.0 }
$global:_hkOut = 'C:\Users\@WINUSER@\winhooks\out\stamps.txt'
PSReadLine\Set-PSReadLineKeyHandler -Chord Enter -ScriptBlock { $global:_hk.enter = _hkNow; [Microsoft.PowerShell.PSConsoleReadLine]::AcceptLine() }
$m = (Get-Command Get-EpochTime).Module
& $m {
  $o = $script:oldPSConsoleHostReadLine
  $script:oldPSConsoleHostReadLine = { $l = & $o; $global:_hk.ret = _hkNow; $l }.GetNewClosure()
  $global:_hkPre = ${function:Warp-Preexec}
  $global:_hkPrecmd = ${function:Warp-Precmd}
  Set-Item function:Warp-Preexec { param([string]$command) $global:_hk.pre0 = _hkNow; & $global:_hkPre $command; $global:_hk.pre1 = _hkNow }
  Set-Item function:Warp-Precmd { param([bool]$status,[int]$code) $global:_hk.p0 = _hkNow; & $global:_hkPrecmd -status $status -code $code; $global:_hk.pc1 = _hkNow }
}
$global:_hkPrompt = $function:global:prompt
$function:global:prompt = { $r = & $global:_hkPrompt; $global:_hk.p1 = _hkNow; $r }
function global:s { $global:_hk.cmd = _hkNow
  $h=$global:_hk
  [IO.File]::AppendAllText($global:_hkOut, ('{0:F1} {1:F1} {2:F1} {3:F1} {4:F1} {5:F1} {6:F1} {7:F1}' -f $h.enter,$h.ret,$h.pre0,$h.pre1,$h.cmd,$h.p0,$h.pc1,$h.p1) + "`n") }
function global:mb { param([int]$n=30)
  $r=@{}
  $o = [Console]::Out
  foreach($t in @(@('WH',{Microsoft.PowerShell.Utility\Write-Host -NoNewline ([string][char]27+']9279'+[char]7)}),@('CO',{[Console]::Out.Write(([string][char]27+']9279'+[char]7))}),@('TITLE',{$HOST.UI.RawUI.WindowTitle='x'+(Get-Random)}),@('CURSOR',{$null=$HOST.UI.RawUI.CursorPosition}),@('PSRLOPT',{PSReadLine\Set-PSReadLineOption -ExtraPromptLineCount 0}))) {
    $ts = 1..$n | % { $sw=[Diagnostics.Stopwatch]::StartNew(); & $t[1]; $sw.Elapsed.TotalMilliseconds } | sort
    $r[$t[0]] = '{0} med {1:N2} p90 {2:N2} max {3:N2} over5={4}' -f $t[0],$ts[[int]($n/2)],$ts[[int]($n*0.9)],$ts[-1],@($ts|?{$_ -gt 5}).Count
  }
  Add-Content $global:_hkOut ("MB " + ($r.Values -join ' | '))
}
