# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
Start-Transcript "$R\out\LK_transcript.txt" -Force | Out-Null
. C:\Users\@WINUSER@\r18bj\tc.ps1
. C:\Users\@WINUSER@\r18bj\tc2.ps1
SetMode 'orig'
FreshApp2 "info" 14
$h=Hwnd
[M]::Click(700,660); Start-Sleep 2
Keys "echo alpha_beta gamma{ENTER}"; Start-Sleep 2
function DC($gap){
  Set-Clipboard "SENTINEL"
  [M]::Click(600,300); Start-Sleep -Milliseconds 600
  [void][W]::SetCursorPos(60,581); Start-Sleep -Milliseconds 300
  $sw=[Diagnostics.Stopwatch]::StartNew()
  [W]::mouse_event(2,0,0,0,[UIntPtr]::Zero); [W]::mouse_event(4,0,0,0,[UIntPtr]::Zero)
  while($sw.ElapsedMilliseconds -lt $gap){}
  [W]::mouse_event(2,0,0,0,[UIntPtr]::Zero); [W]::mouse_event(4,0,0,0,[UIntPtr]::Zero)
  Start-Sleep -Milliseconds 500
  [K]::Combo([byte[]]@(0x11,0x10),0x43); Start-Sleep -Milliseconds 500
  $c=(Get-Clipboard -Raw); "gap=${gap}ms -> clipboard=[$($c.Trim())]"
}
foreach($g in 150,300,380,430,470,520,700){ DC $g }
Shot "LK_last"
KillApp
"LK_DONE"
Stop-Transcript | Out-Null
