# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
Start-Transcript "$R\out\S2_transcript.txt" -Force | Out-Null
. C:\Users\@WINUSER@\r18bj\tc.ps1
. C:\Users\@WINUSER@\r18bj\tc2.ps1
SetMode 'orig'
$env:PHOSPHOR_BOOTSTRAP_TIMED="1"
FreshApp2 "info,warp::input_order=debug,warp::terminal=debug" 14
Remove-Item Env:PHOSPHOR_BOOTSTRAP_TIMED -ea 0
$h=Hwnd
[M]::Click(300,660); Start-Sleep 1
# Git Bash tab
[M]::Click(313,42); Start-Sleep 1; [M]::Click(337,125); Start-Sleep 14; Focus; [M]::Click(300,660); Start-Sleep 2
function CtrlI { [K]::Combo([byte[]]@(0x11),0x49) }
function CtrlC { [K]::Combo([byte[]]@(0x11),0x43) }
# A) powershell inside bash, Phosphorize, wait, type
Keys "powershell{ENTER}"; Start-Sleep 4
CtrlI; Start-Sleep 12; Shot "S2_A_after_setup"
Keys "echo done-A{ENTER}"; Start-Sleep 2; Shot "S2_A_cmd"
Keys "exit{ENTER}"; Start-Sleep 3; Shot "S2_A_exit"
# B) Ctrl-C during setup
Keys "powershell{ENTER}"; Start-Sleep 4
CtrlI; Start-Sleep -Milliseconds 400; CtrlC; Start-Sleep 8; Shot "S2_B_after_ctrlc"
Keys "echo done-B{ENTER}"; Start-Sleep 2; Shot "S2_B_cmd"
CtrlC; Start-Sleep 1; Keys "exit{ENTER}"; Start-Sleep 3; Shot "S2_B_exit"
# D) type during setup
Keys "powershell{ENTER}"; Start-Sleep 4
CtrlI; Start-Sleep -Milliseconds 100
[void][TC]::Line($h,"echo typed-during-setup-D",25); Start-Sleep 12; Shot "S2_D_after"
Keys "echo done-D{ENTER}"; Start-Sleep 2; Shot "S2_D_cmd"
Keys "exit{ENTER}"; Start-Sleep 3
# C) PowerShell tab: nested powershell
[M]::Click(177,42); Start-Sleep 2; [M]::Click(300,660); Start-Sleep 1
Keys "powershell{ENTER}"; Start-Sleep 4; Shot "S2_C_banner"
CtrlI; Start-Sleep 12; Shot "S2_C_after_setup"
Keys "echo done-C{ENTER}"; Start-Sleep 2; Shot "S2_C_cmd"
SaveLog "S2_app"
"S2_DONE"
Stop-Transcript | Out-Null
