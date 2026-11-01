# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
. C:\Users\@WINUSER@\r18bj\tc.ps1
. C:\Users\@WINUSER@\r18bj\tc2.ps1
SetMode 'term'
$env:PHOSPHOR_GPU_TIMING="1"
FreshApp2 "info,warp::input_order=debug,warpui::scene_stats=debug,warpui::frame_cause=debug" 10
Start-Sleep 3
SaveLog "T2"
KillApp
$t=Get-Content "$R\out\T2.log" -Raw
"lines: " + ($t -split "`n").Count
"frame_cause: " + ([regex]::Matches($t,'updated=')).Count
"fingerprint: " + ([regex]::Matches($t,'fingerprint')).Count
"gpu: " + ([regex]::Matches($t,'gpu_')).Count
($t -split "`n" | ? { $_ -match 'updated=|fingerprint|gpu' } | select -first 4) -join "`n"
