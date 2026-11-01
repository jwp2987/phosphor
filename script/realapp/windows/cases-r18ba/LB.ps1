# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
Start-Transcript "$R\out\LB_transcript.txt" -Force | Out-Null
. C:\Users\@WINUSER@\r18ba\lat.ps1
LatInit 700 200
$L='r18ba'; $o="$R\out\lat_${L}_B.csv"; Remove-Item $o -ea 0
function EchoRuns($tag,$n,$x,$y){ for($k=0;$k -lt $n;$k++){ $res=[LT]::Burst($x,$y,'echo hi',10,$true); Add-Content $o "$L|$tag-echo-hi,$k,$res"; Start-Sleep -Milliseconds 2000 } }
function StampRuns($tag,$n,$x,$y,$mk){ $f="C:\Users\@WINUSER@\r18ba\out\start_B$tag.txt"; Remove-Item $f -ea 0
  for($k=0;$k -lt $n;$k++){ $res=[LT]::Burst($x,$y,(& $mk $f),10,$true); Add-Content $o "$L|$tag-start-stamp,$k,$res"; Start-Sleep -Milliseconds 2000 }
  if(Test-Path $f){ Add-Content $o ("$L|$tag-start-stamps," + ((Get-Content $f) -join ';')) } }
FreshApp "info,warp::input_order=debug" 60
[M]::Click(300,660); Start-Sleep 1
LatRun "$L|ps1-key" 0 520 "zqxjkvwpyz" 30 $o "^a{BACKSPACE}" 20
EchoRuns 'ps1' 12 0 520
StampRuns 'ps1' 12 0 520 { param($f) "[IO.File]::AppendAllText('$f',[string]([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())+[char]10)" }
Shot "B_ps1"
[M]::Click(313,42); Start-Sleep 1; [M]::Click(337,125); Start-Sleep 14; Focus; [M]::Click(300,660); Start-Sleep 1
Shot "B_gb0"
LatRun "$L|gb-key" 0 520 "zqxjkvwpyz" 40 $o "^a{BACKSPACE}" 20
EchoRuns 'gb' 15 0 520
StampRuns 'gb' 12 0 520 { param($f) 'echo $(date +%s%3N) >> /c/Users/@WINUSER@/r18ba/out/start_Bgb.txt' }
Shot "B_gb"
Keys "^+t"; Start-Sleep 8; Focus; [M]::Click(300,660); Start-Sleep 1
Shot "B_ps3_0"
LatRun "$L|ps3-key" 0 520 "zqxjkvwpyz" 40 $o "^a{BACKSPACE}" 20
Shot "B_ps3"
SaveLog "B_app"
"done " + (Get-Content $o).Count
Stop-Transcript | Out-Null
