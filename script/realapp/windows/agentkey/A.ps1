# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
Start-Transcript "$R\out\A_transcript.txt" -Force | Out-Null
. C:\Users\@WINUSER@\r18wk\lat.ps1
LatInit 700 200
$L='wk'; $o="$R\out\lat_${L}_A.csv"; Remove-Item $o -ea 0
FreshApp "info,warp::input_order=debug,warpui::frame_cause=debug" 14
Rect
[M]::Click(300,660); Start-Sleep 1
Shot "A_ps0"
LatRun "$L|ps-key" 0 520 "zqxjkvwpyz" 40 $o "^a{BACKSPACE}" 20
[M]::Click(313,42); Start-Sleep 1; [M]::Click(330,159); Start-Sleep 5; Focus; Shot "A_agent_tab0"
[M]::Click(300,660); Start-Sleep 1
LatRun "$L|agent-key" 0 520 "zqxjkvwpyz" 40 $o "^a{BACKSPACE}" 20
Shot "A_agent_tab"
SaveLog "A_app"
"done " + (Get-Content $o).Count
Stop-Transcript | Out-Null
