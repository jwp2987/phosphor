# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
Start-Transcript "$R\out\B_immed_transcript.txt" -Force | Out-Null
. C:\Users\@WINUSER@\r18wk\lat.ps1
LatInit 700 200
$L='immed'; $o="$R\out\lat_${L}.csv"; Remove-Item $o -ea 0
$env:PHOSPHOR_GPU_TIMING="1"; $env:PHOSPHOR_PRESENT_MODE="immediate"
FreshApp "info,warp::input_order=debug,warpui::frame_cause=debug,warpui::gpu_done=debug" 14
Rect
[M]::Click(300,660); Start-Sleep 1
LatRun "$L|ps1-key" 0 520 "zqxjkvwpyz" 40 $o "^a{BACKSPACE}" 20
[M]::Click(313,42); Start-Sleep 1; [M]::Click(330,159); Start-Sleep 5; Focus
[M]::Click(300,660); Start-Sleep 1
LatRun "$L|agent1-key" 0 520 "zqxjkvwpyz" 40 $o "^a{BACKSPACE}" 20
Shot "B_immed_agent"
[M]::Click(177,42); Start-Sleep 2; [M]::Click(300,660); Start-Sleep 1
LatRun "$L|ps2-key" 0 520 "zqxjkvwpyz" 40 $o "^a{BACKSPACE}" 20
[M]::Click(385,42); Start-Sleep 2; [M]::Click(300,660); Start-Sleep 1
LatRun "$L|agent2-key" 0 520 "zqxjkvwpyz" 40 $o "^a{BACKSPACE}" 20
Shot "B_immed_end"
SaveLog "B_immed_app"
"done " + (Get-Content $o).Count
Stop-Transcript | Out-Null
