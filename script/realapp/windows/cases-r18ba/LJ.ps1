# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
Start-Transcript "$R\out\LJ_transcript.txt" -Force | Out-Null
. C:\Users\@WINUSER@\r18ba\lat.ps1
$o="$R\out\lat_r18ba_J.csv"; Remove-Item $o -ea 0
function SizeRun($tag,$w,$hh,$y,$n){ $h=PhoWin; [void][W]::MoveWindow($h,24,24,$w,$hh,$true); Start-Sleep 2; Focus; [M]::Click(24+200,24+$hh-60); Start-Sleep 1; LatInit 700 200; LatRun "r18ba|$tag-key" 0 $y "zqxjkvwpyz" $n $o "^a{BACKSPACE}" 20 }
function Static($tag){ $h=PhoWin; [void][W]::MoveWindow($h,24,24,1232,712,$true); Start-Sleep 2; Shot "J_static_$tag"; Start-Sleep 1; Shot "J_static_${tag}_b" }
$env:PHOSPHOR_WGPU_CLEAR_TO_BACKGROUND='0'
FreshApp "info,warp::input_order=debug" 15
Static 'clr0'
foreach($rep in 1..2){ SizeRun "clr0_big_r$rep" 1232 712 520 30; SizeRun "clr0_small_r$rep" 700 420 300 30 }
KillApp
Remove-Item Env:\PHOSPHOR_WGPU_CLEAR_TO_BACKGROUND
FreshApp "info,warp::input_order=debug" 15
Static 'default'
KillApp
$env:PHOSPHOR_RECT_FAST_PATH='0'
FreshApp "info,warp::input_order=debug" 15
Static 'fast0'
KillApp
Remove-Item Env:\PHOSPHOR_RECT_FAST_PATH
Stop-Transcript | Out-Null
