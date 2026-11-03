# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
Start-Transcript "$R\out\LR_transcript.txt" -Force | Out-Null
. C:\Users\@WINUSER@\r18bj\lat.ps1
. C:\Users\@WINUSER@\r18bj\tc.ps1
. C:\Users\@WINUSER@\r18bj\tc2.ps1
LatInit 700 40
$L='bj'; $o="$R\out\lat_${L}_LR.csv"; Remove-Item $o -ea 0
SetMode 'orig'
FreshApp2 "info,warp::input_order=debug" 14
$h=Hwnd
[M]::Click(700,660); Start-Sleep 2
# --- #1361: tab 2, WM_ENDSESSION(FALSE) then exit within 1 s
[K]::Combo([byte[]]@(0x11,0x10),0x54); Start-Sleep 4; Shot "LR_tab2"
"tabs before: $(Title)"
foreach($t in 1..3){
  [K]::Combo([byte[]]@(0x11,0x10),0x54); Start-Sleep 4; [M]::Click(700,660); Start-Sleep 1
  [void][W]::PostMessage([IntPtr]$h,0x11,[IntPtr]::Zero,[IntPtr]::Zero)   # WM_QUERYENDSESSION
  [void][W]::PostMessage([IntPtr]$h,0x16,[IntPtr]::Zero,[IntPtr]::Zero)   # WM_ENDSESSION(FALSE)
  Start-Sleep -Milliseconds 300
  Keys "exit{ENTER}"; Start-Sleep -Milliseconds 1500
  Shot "LR_endsession_$t"
  "endsession trial $t : app responding=$((Get-Process -Id (Get-Process phosphor-oss | ? { $_.Path -like '*r18bj*' } | select -First 1).Id).Responding)"
  Start-Sleep 4
}
SaveLog "LR_app1"
"LR1_DONE"
Stop-Transcript | Out-Null
