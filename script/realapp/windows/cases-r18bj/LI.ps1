# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
Start-Transcript "$R\out\LI_transcript.txt" -Force | Out-Null
. C:\Users\@WINUSER@\r18bj\tc.ps1
. C:\Users\@WINUSER@\r18bj\tc2.ps1
SetMode 'orig'
function Idle($exe,$label){
  KillApp; Get-Process phosphor-oss -ea 0 | ? { $_.Path -like '*Programs\Phosphor*' } | % { & taskkill /PID $_.Id /T /F | Out-Null }
  Remove-Item "$env:LOCALAPPDATA\phosphor\Phosphor\data\warp.sqlite*" -ea 0
  $p=Start-Process $exe -WorkingDirectory $env:USERPROFILE -PassThru
  Start-Sleep 15
  $res=@()
  foreach($i in 1..3){ $p.Refresh(); $c0=$p.TotalProcessorTime.TotalMilliseconds; Start-Sleep 20; $p.Refresh(); $res+=[math]::Round(($p.TotalProcessorTime.TotalMilliseconds-$c0)/200,2) }
  "$label idle CPU % of one core (3 x 20 s): $($res -join ', ')  ws=$([int]($p.WorkingSet64/1MB))MB"
  & taskkill /PID $p.Id /T /F | Out-Null; Start-Sleep 3
}
Idle "C:\Users\@WINUSER@\r18bj\app\phosphor-oss.exe" "r18bj terminal-mode"
Idle "C:\Users\@WINUSER@\AppData\Local\Programs\Phosphor\phosphor-oss.exe" "v0.1.15 terminal-mode"
"LI_DONE"
Stop-Transcript | Out-Null
