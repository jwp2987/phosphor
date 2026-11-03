# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
Start-Transcript "$R\out\S1_transcript.txt" -Force | Out-Null
. C:\Users\@WINUSER@\r18bj\tc.ps1
. C:\Users\@WINUSER@\r18bj\tc2.ps1
SetMode 'orig'
FreshApp2 "info,warp::input_order=debug,warp::terminal=debug" 14
[M]::Click(300,660); Start-Sleep 1
# Git Bash tab
[M]::Click(313,42); Start-Sleep 1; [M]::Click(337,125); Start-Sleep 14; Focus; [M]::Click(300,660); Start-Sleep 2
Keys "powershell{ENTER}"; Start-Sleep 2; Shot "S1_a"
Start-Sleep 3; Shot "S1_b"
Start-Sleep 6; Shot "S1_c"
SaveLog "S1_app"
"S1_DONE"
Stop-Transcript | Out-Null
