# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
Start-Transcript "$R\out\LE_transcript.txt" -Force | Out-Null
. C:\Users\@WINUSER@\r18ba\lat.ps1
LatInit 700 200
$L='r18ba'; $o="$R\out\lat_${L}_E.csv"; Remove-Item $o -ea 0
function StampRuns($tag,$n,$x,$y,$mk,$file){ $f="C:\Users\@WINUSER@\r18ba\out\$file"; Remove-Item $f -ea 0
  for($k=0;$k -lt $n;$k++){ $res=[LT]::Burst($x,$y,(& $mk),10,$true); Add-Content $o "$L|$tag-start-stamp,$k,$res"; Start-Sleep -Milliseconds 2000 }
  if(Test-Path $f){ Add-Content $o ("$L|$tag-start-stamps," + ((Get-Content $f) -join ';')) } }
# plain Phosphor, only a bash tab opened first thing
FreshApp "info,warp::input_order=debug" 20
[M]::Click(313,42); Start-Sleep 1; [M]::Click(337,125); Start-Sleep 20; Focus; [M]::Click(300,660); Start-Sleep 20
Shot "E_gb0"
# builtin stamp: EPOCHREALTIME is microseconds with a dot
StampRuns 'gbB' 12 0 520 { 'echo $EPOCHREALTIME >> /c/Users/@WINUSER@/r18ba/out/start_Egb_b.txt' } 'start_Egb_b.txt'
StampRuns 'gbD' 12 0 520 { 'echo $(date +%s%3N) >> /c/Users/@WINUSER@/r18ba/out/start_Egb_d.txt' } 'start_Egb_d.txt'
StampRuns 'gbB2' 12 0 520 { 'echo $EPOCHREALTIME >> /c/Users/@WINUSER@/r18ba/out/start_Egb_b2.txt' } 'start_Egb_b2.txt'
Shot "E_gb"
SaveLog "E_app"
KillApp
# conhost bash control: builtin
$p=Start-Process "C:\Program Files\Git\bin\bash.exe" -ArgumentList '--login','-i' -PassThru
Start-Sleep 6; for($i=0;$i -lt 40;$i++){ $p.Refresh(); if($p.MainWindowHandle -ne 0){break}; Start-Sleep -Milliseconds 250 }
$h=$p.MainWindowHandle; [void][W]::ShowWindow($h,9); [void][W]::MoveWindow($h,0,500,900,260,$true); Start-Sleep 1; [void][W]::SetForegroundWindow($h); Start-Sleep 1
$global:TP='bash'
StampRuns 'congbB' 12 0 500 { 'echo $EPOCHREALTIME >> /c/Users/@WINUSER@/r18ba/out/start_Econgb_b.txt' } 'start_Econgb_b.txt'
Stop-Process -Id $p.Id -Force
"done " + (Get-Content $o).Count
Stop-Transcript | Out-Null
