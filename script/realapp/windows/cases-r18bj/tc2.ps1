# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
# Rep2: 10 lines at $ms/key; check f.txt at +4 s (old wait) and +$waitMs (>=15 s) after last Enter -> separates late vs lost
function Rep2($label,$hwnd,$ms,$path='/c/Users/@WINUSER@/r18bj/f.txt',$waitMs=15000){
  Remove-Item "$global:R\f.txt" -ea 0
  if(-not (Fg $hwnd)){ return "$label ABORT-nofg" }
  $ok=$true
  for($n=1;$n -le 10;$n++){ if(-not [TC]::Line($hwnd,"echo L$n >> $path",$ms)){ $ok=$false; break } }
  $sw=[Diagnostics.Stopwatch]::StartNew()
  Start-Sleep -Milliseconds 4000
  $l4=@(); try{ if(Test-Path "$global:R\f.txt"){ $l4=@(Get-Content "$global:R\f.txt" -ea Stop) } }catch{}
  Start-Sleep -Milliseconds ($waitMs-4000)
  $lines=@(); if(Test-Path "$global:R\f.txt"){ $lines=@(Get-Content "$global:R\f.txt") }
  $exp=1..10 | % { "L$_" }
  $m4=@($exp | ? { $l4 -notcontains $_ }); $m15=@($exp | ? { $lines -notcontains $_ })
  $extra=$lines | ? { $exp -notcontains $_ }
  if(Test-Path "$global:R\f.txt"){ Move-Item "$global:R\f.txt" "$global:R\out\$label.txt" -Force }
  $cls=$(if($m15.Count -gt 0){'LOST'}elseif($m4.Count -gt 0){'LATE'}else{'OK'})
  "$label $cls fg_ok=$ok got4=$($l4.Count) got15=$($lines.Count)/10 missing4=[$($m4 -join ',')] missing15=[$($m15 -join ',')] garbled=[$($extra -join ';')] order=$($lines -join ',')"
}
function SetMode($m){
  $dst="$env:LOCALAPPDATA\phosphor\Phosphor\config\settings.toml"
  if($m -eq 'orig'){ Copy-Item "$global:R\bak\local\Phosphor\config\settings.toml" $dst -Force; return }
  $t=Get-Content "$global:R\settings_agent.toml" -Raw
  if($m -eq 'term'){ $t=$t -replace 'default_session_mode = "agent"','default_session_mode = "terminal"' -replace 'ai_auto_detection_enabled = true','ai_auto_detection_enabled = false' -replace 'nld_in_terminal_enabled = true','nld_in_terminal_enabled = false' }
  Set-Content -Encoding UTF8 $dst $t
}
function FreshApp2($rustlog,$wait=12){
  KillApp
  Remove-Item "$env:LOCALAPPDATA\phosphor\Phosphor\data\warp.sqlite*" -ea 0
  Remove-Item "$env:LOCALAPPDATA\phosphor\Phosphor\data\logs\*" -ea 0 -Force -Recurse -Confirm:$false
  $env:RUST_LOG=$rustlog; $env:WARP_INPUT_ORDER_LOG_TEXT="1"
  $global:AppP = Start-Process "C:\Users\@WINUSER@\r18bj\app\phosphor-oss.exe" -WorkingDirectory $env:USERPROFILE -PassThru
  Start-Sleep $wait; Focus
}
function Hwnd { (Get-Process phosphor-oss | ? { $_.Path -like '*r18bj*' -and $_.MainWindowHandle -ne 0 } | select -First 1).MainWindowHandle.ToInt64() }
