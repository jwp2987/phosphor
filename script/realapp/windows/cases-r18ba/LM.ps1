# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
Start-Transcript "$R\out\LM_transcript.txt" -Force | Out-Null
. C:\Users\@WINUSER@\r18ba\tc.ps1
$global:R="C:\Users\@WINUSER@\r18ba"
. C:\Users\@WINUSER@\r18ba\lat.ps1
LatInit 700 200
$L='r18ba'; $o="$R\out\lat_${L}_M.csv"; Remove-Item $o -ea 0
function StampRuns($tag,$n,$x,$y,$mk,$file){ $f="C:\Users\@WINUSER@\r18ba\out\$file"; Remove-Item $f -ea 0
  for($k=0;$k -lt $n;$k++){ $res=[LT]::Burst($x,$y,(& $mk),10,$true); Add-Content $o "$L|$tag-start-stamp,$k,$res"; Start-Sleep -Milliseconds 2000 }
  if(Test-Path $f){ Add-Content $o ("$L|$tag-start-stamps," + ((Get-Content $f) -join ';')) } }
FreshApp "info,warp::input_order=debug" 10
[M]::Click(300,660); Start-Sleep 1
# repro of A/B: PS stamps right after launch, then a bash tab
StampRuns 'ps' 12 0 520 { "[IO.File]::AppendAllText('C:\Users\@WINUSER@\r18ba\out\start_Mps.txt',[string]([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())+[char]10)" } 'start_Mps.txt'
[M]::Click(313,42); Start-Sleep 1; [M]::Click(337,125); Start-Sleep 14; Focus; [M]::Click(300,660); Start-Sleep 2
StampRuns 'gbD' 12 0 520 { 'echo $(date +%s%3N) >> /c/Users/@WINUSER@/r18ba/out/start_Mgb.txt' } 'start_Mgb.txt'
Shot M_gb
# typeahead #1162 in managed Git Bash (this is the bash tab)
$h=(Get-Process phosphor-oss | ? { $_.Path -like '*r18ba*' }).MainWindowHandle.ToInt64()
foreach($ms in 25,10){ foreach($rep in 1..4){ Rep "gb_${ms}ms_r$rep" $h $ms '/c/Users/@WINUSER@/r18ba/f.txt' 6000 } }
Stray
# PowerShell tab typeahead
Keys "^+t"; Start-Sleep 8; Focus; [M]::Click(300,660); Start-Sleep 1
Cmd 'cd C:\Users\@WINUSER@\r18ba' 2500
foreach($ms in 25,10){ foreach($rep in 1..4){ Rep "ps_${ms}ms_r$rep" $h $ms 'C:\Users\@WINUSER@\r18ba\f.txt' 6000 } }
Stray
Shot M_ps
SaveLog M_app
Stop-Transcript | Out-Null
