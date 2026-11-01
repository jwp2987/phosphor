# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
Start-Transcript "$R\out\LC_transcript.txt" -Force | Out-Null
. C:\Users\@WINUSER@\r18bj\lat.ps1
. C:\Users\@WINUSER@\r18bj\tc.ps1
. C:\Users\@WINUSER@\r18bj\tc2.ps1
LatInit 700 200
$L='bj'; $o="$R\out\lat_${L}_LC.csv"; Remove-Item $o -ea 0
KillApp
# classic console (conhost) running Windows PowerShell, same machine
$cp=Start-Process powershell.exe -ArgumentList '-NoLogo','-NoProfile' -PassThru -WindowStyle Normal
Start-Sleep 4
Add-Type -AssemblyName System.Windows.Forms
$cw=$null; foreach($i in 1..10){ [TC]::Wins(); $cw=[TC]::L | ? { $_ -match '\|ConsoleWindowClass\|' } | select -First 1; if($cw){break}; Start-Sleep 1 }
"conhost window: $cw"
$global:CH=[int64]($cw.Split('|')[0])
function IsPho { [TC]::GetForegroundWindow().ToInt64() -eq $global:CH }
[void][W]::MoveWindow([IntPtr]$global:CH,0,0,800,500,$true); Start-Sleep 1
[void](Fg $global:CH)
Keys "cls{ENTER}"; Start-Sleep 1; Shot "LC_console"
"=== conhost key->paint"
LatRun "$L|conhost-ps-key" 0 30 "zqxjkvwpyz" 40 $o "^a{BACKSPACE}" 20
"=== echo hi"
for($k=0;$k -lt 30;$k++){ Keys "cls{ENTER}"; Start-Sleep -Milliseconds 600; $res=[LT]::Burst(0,30,'echo hi',10,$true); Add-Content $o "$L|conhost-ps-echo-hi,$k,$res"; Start-Sleep -Milliseconds 1500 }
"=== start stamps"
$f="$R\out\start_LC_ps.txt"; Remove-Item $f -ea 0
for($k=0;$k -lt 32;$k++){
  Keys "cls{ENTER}"; Start-Sleep -Milliseconds 600
  $cmd="[IO.File]::AppendAllText('$f',[string]([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())+[char]10)"
  $res=[LT]::Burst(0,30,$cmd,10,$true); Add-Content $o "$L|conhost-ps-start-stamp,$k,$res"; Start-Sleep -Milliseconds 1500 }
Add-Content $o ("$L|conhost-ps-start-stamps," + ((Get-Content $f) -join ';'))
"=== Ctrl-C"
$f2="$R\out\ctrlc_conhost.txt"; Remove-Item $f2 -ea 0
$cc=@()
for($k=0;$k -lt 8;$k++){
  Keys "Start-Sleep 30{ENTER}"; Start-Sleep -Milliseconds 2500
  $t0=[LT]::Now(); [K]::Combo([byte[]]@(0x11),0x43)
  $cmd="[IO.File]::AppendAllText('$f2',[string]([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())+[char]10)"
  Start-Sleep -Milliseconds 20
  Keys ((Esc $cmd)+"{ENTER}"); Start-Sleep -Milliseconds 2500
  $cc+=$t0 }
$st=@(Get-Content $f2 -ea 0)
"ctrlc conhost: sent=$($cc.Count) stamps=$($st.Count) deltas(ms)=" + ((0..($st.Count-1) | % { [math]::Round([double]$st[$_]-$cc[$_]) }) -join ',')
Keys "exit{ENTER}"; Start-Sleep 2
if(Get-Process -Id $cp.Id -ea 0){ Stop-Process -Id $cp.Id -Force }
"LC_DONE"
Stop-Transcript | Out-Null
