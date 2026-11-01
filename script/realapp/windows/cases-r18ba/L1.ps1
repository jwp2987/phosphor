# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
Focus; [M]::Click(300,660); Start-Sleep 1
. C:\Users\@WINUSER@\r18ba\lat.ps1
LatInit 700 200
$L='r18ba'; $o="$R\out\lat_${L}_ps_term.csv"; Remove-Item $o -ea 0
Shot "lat_ps_term"
LatRun "$L|ps-term-key" 0 520 "zqxjkvwpyz" 40 $o "^a{BACKSPACE}" 20
for($k=0;$k -lt 15;$k++){ $r=[LT]::Burst(0,520,'echo hi',10,$true); Add-Content $o "$L|ps-term-echo-hi,$k,$r"; Start-Sleep -Milliseconds 2000 }
$f="$R\out\start_ps_term.txt"; Remove-Item $f -ea 0
for($k=0;$k -lt 12;$k++){
  $cmd="[IO.File]::AppendAllText('$f',[string]([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())+[char]10)"
  $r=[LT]::Burst(0,520,$cmd,10,$true); Add-Content $o "$L|ps-term-start-stamp,$k,$r"; Start-Sleep -Milliseconds 2000 }
Add-Content $o ("$L|ps-term-start-stamps," + ((Get-Content $f) -join ';'))
Shot "lat_ps_term2"
"done " + (Get-Content $o).Count
