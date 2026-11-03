# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
Start-Transcript "$R\out\LP_transcript.txt" -Force | Out-Null
. C:\Users\@WINUSER@\r18bj\lat.ps1
. C:\Users\@WINUSER@\r18bj\tc.ps1
. C:\Users\@WINUSER@\r18bj\tc2.ps1
. C:\Users\@WINUSER@\r18bj\tc3.ps1
LatInit 700 200
$L='bj'; $o="$R\out\lat_${L}_LP.csv"; Remove-Item $o -ea 0
SetMode 'orig'
FreshApp2 "info,warp::input_order=debug" 14
[M]::Click(300,660); Start-Sleep 1
Shot "LP_ps"
"=== phosphor PS key->paint"
LatRun "$L|ps-term-key" 0 520 "zqxjkvwpyz" 40 $o "^a{BACKSPACE}" 20
"=== echo hi"
for($k=0;$k -lt 30;$k++){ $res=[LT]::Burst(0,520,'echo hi',10,$true); Add-Content $o "$L|ps-term-echo-hi,$k,$res"; Start-Sleep -Milliseconds 2000 }
"=== start stamps"
$f="$R\out\start_LP_ps.txt"; Remove-Item $f -ea 0
for($k=0;$k -lt 32;$k++){
  $cmd="[IO.File]::AppendAllText('$f',[string]([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())+[char]10)"
  $res=[LT]::Burst(0,520,$cmd,10,$true); Add-Content $o "$L|ps-term-start-stamp,$k,$res"; Start-Sleep -Milliseconds 2000 }
Add-Content $o ("$L|ps-term-start-stamps," + ((Get-Content $f) -join ';'))
"=== Ctrl-C"
$f2="$R\out\ctrlc_ps.txt"; Remove-Item $f2 -ea 0
$cc=@()
for($k=0;$k -lt 8;$k++){
  Keys "Start-Sleep 30{ENTER}"; Start-Sleep -Milliseconds 2500
  $t0=[LT]::Now(); [K]::Combo([byte[]]@(0x11),0x43)
  $cmd="[IO.File]::AppendAllText('$f2',[string]([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())+[char]10)"
  Start-Sleep -Milliseconds 20
  Keys ((Esc $cmd)+"{ENTER}"); Start-Sleep -Milliseconds 2500
  $cc+=$t0 }
$st=@(Get-Content $f2 -ea 0)
"ctrlc: sent=$($cc.Count) stamps=$($st.Count) deltas(ms)=" + ((0..($st.Count-1) | % { [math]::Round([double]$st[$_]-$cc[$_]) }) -join ',')
Shot "LP_ctrlc"
"=== typing exact PS 25ms"
$h=Hwnd
$p="C:\Users\@WINUSER@\r18bj\f.txt"
foreach($rep in 1..8){ Rep3 "ps25_r$rep" $h 25 "Add-Content $p L{n}" 12000 }
foreach($rep in 1..4){ Rep3 "ps12_r$rep" $h 12 "Add-Content $p L{n}" 12000 }
SaveLog "LP_app"
KillApp
"LP_DONE"
Stop-Transcript | Out-Null
