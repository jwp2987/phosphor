# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
Start-Transcript "$R\out\LD_transcript.txt" -Force | Out-Null
KillApp
. C:\Users\@WINUSER@\r18ba\lat.ps1
LatInit 700 120
$L='r18ba'; $o="$R\out\lat_${L}_D.csv"; Remove-Item $o -ea 0
function PlaceCon($p){ for($i=0;$i -lt 40;$i++){ $p.Refresh(); if($p.MainWindowHandle -ne 0){break}; Start-Sleep -Milliseconds 250 }
  $h=$p.MainWindowHandle; [void][W]::ShowWindow($h,9); [void][W]::MoveWindow($h,0,500,900,260,$true); Start-Sleep -Milliseconds 500; [void][W]::SetForegroundWindow($h); Start-Sleep 1
  $fg=[W]::GetForegroundWindow(); $pid2=0; [void][W]::GetWindowThreadProcessId($fg,[ref]$pid2); "fg=$fg pid=$pid2 name=$((Get-Process -Id $pid2 -ea 0).ProcessName) h=$h" }
function EchoRuns($tag,$n,$x,$y,$clear){ for($k=0;$k -lt $n;$k++){ $res=[LT]::Burst($x,$y,'echo hi',10,$true); Add-Content $o "$L|$tag-echo-hi,$k,$res"; Start-Sleep -Milliseconds 1500; [System.Windows.Forms.SendKeys]::SendWait($clear); Start-Sleep 1 } }
function StampRuns($tag,$n,$x,$y,$mk,$clear){ $f="C:\Users\@WINUSER@\r18ba\out\start_D$tag.txt"; Remove-Item $f -ea 0
  for($k=0;$k -lt $n;$k++){ $res=[LT]::Burst($x,$y,(& $mk $f),10,$true); Add-Content $o "$L|$tag-start-stamp,$k,$res"; Start-Sleep -Milliseconds 2000; [System.Windows.Forms.SendKeys]::SendWait($clear); Start-Sleep -Milliseconds 700 }
  if(Test-Path $f){ Add-Content $o ("$L|$tag-start-stamps," + ((Get-Content $f) -join ';')) } }
# ---- conhost PowerShell 5.1
$p=Start-Process powershell.exe -ArgumentList '-NoLogo','-NoProfile' -PassThru
"launched conhost ps $($p.Id)"; Start-Sleep 4
PlaceCon $p; $global:TP='powershell'
Shot D_conps0
$X=0;$Y=500
LatRun "$L|conps-key" $X $Y "zqxjkvwpyz" 40 $o "{ESC}" 20
EchoRuns 'conps' 15 $X $Y "cls{ENTER}"
StampRuns 'conps' 12 $X $Y { param($f) "[IO.File]::AppendAllText('$f',[string]([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())+[char]10)" } "cls{ENTER}"
Shot D_conps
Stop-Process -Id $p.Id -Force
# ---- conhost Git Bash
$p=Start-Process "C:\Program Files\Git\bin\bash.exe" -ArgumentList '--login','-i' -PassThru
"launched conhost bash $($p.Id)"; Start-Sleep 6
PlaceCon $p; $global:TP='bash'
Shot D_congb0
LatRun "$L|congb-key" $X $Y "zqxjkvwpyz" 40 $o "^u" 20
EchoRuns 'congb' 15 $X $Y "^l"
StampRuns 'congb' 12 $X $Y { param($f) 'echo $(date +%s%3N) >> /c/Users/@WINUSER@/r18ba/out/start_Dcongb.txt' } "^l"
Shot D_congb
Stop-Process -Id $p.Id -Force
Get-Process bash,mintty,conhost -ea 0 | select Id,Name | ft | out-string
"done " + (Get-Content $o).Count
Stop-Transcript | Out-Null
