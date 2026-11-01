# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
Start-Transcript "$R\out\LT_transcript.txt" -Force | Out-Null
. C:\Users\@WINUSER@\r18bj\lat.ps1
. C:\Users\@WINUSER@\r18bj\tc.ps1
. C:\Users\@WINUSER@\r18bj\tc2.ps1
. C:\Users\@WINUSER@\r18bj\tc3.ps1
$p="C:\Users\@WINUSER@\r18bj\f.txt"
function ChkFile($label,$exp){
  Start-Sleep 3
  $lines=@(); if(Test-Path $p){ $lines=@(Get-Content $p | % { $_.Trim() }) }
  $miss=@($exp | ? { $lines -notcontains $_ }); $ex=@($lines | ? { $exp -notcontains $_ })
  if(Test-Path $p){ Move-Item $p "$R\out\$label.txt" -Force }
  "$label $(if($miss.Count -eq 0 -and $ex.Count -eq 0 -and $lines.Count -eq $exp.Count){'OK'}else{'FAIL'}) got=$($lines.Count)/$($exp.Count) missing=[$($miss -join ',')] extra=[$($ex -join ';')] order=$($lines -join ',')"
}
foreach($mode in 'term','agent'){
  SetMode $(if($mode -eq 'term'){'orig'}else{'agent'})
  FreshApp2 "info,warp::input_order=debug" 14
  $h=Hwnd; $iy=$(if($mode -eq 'term'){660}else{650})
  [M]::Click(700,$iy); Start-Sleep 2; Shot "LT_${mode}_start"
  "=== $mode typing 25 ms"
  foreach($rep in 1..6){ Rep3 "${mode}_ps25_r$rep" $h 25 "Add-Content $p L{n}" 12000 }
  "=== $mode typing 12 ms"
  foreach($rep in 1..4){ Rep3 "${mode}_ps12_r$rep" $h 12 "Add-Content $p L{n}" 12000 }
  Shot "LT_${mode}_after_typing"
  "=== $mode paste + instant Enter"
  Remove-Item $p -ea 0
  for($n=1;$n -le 12;$n++){
    Set-Clipboard "Add-Content $p PASTE$n"
    [void](Fg $h); [K]::Down(0x11); [K]::Down(0x56); [K]::Up(0x56); [K]::Up(0x11); [TC]::TapVk(0x0D)
    Start-Sleep -Milliseconds 700 }
  ChkFile "${mode}_paste" (1..12 | % { "PASTE$_" })
  "=== $mode Tab+Enter"
  Remove-Item $p -ea 0
  [void][TC]::Line($h,"cd C:\Users\@WINUSER@\r18bj",12); Start-Sleep 1
  for($n=1;$n -le 10;$n++){
    [void](Fg $h)
    foreach($c in 'c','d',' ','a','p'){ [TC]::Tap([char]$c); Start-Sleep -Milliseconds 12 }
    [TC]::TapVk(0x09); Start-Sleep -Milliseconds 15; [TC]::TapVk(0x0D); Start-Sleep -Milliseconds 700
    [void][TC]::Line($h,"Add-Content $p (Get-Location).Path",12); Start-Sleep -Milliseconds 700
    [void][TC]::Line($h,"cd ..",12); Start-Sleep -Milliseconds 500 }
  Start-Sleep 3
  $lines=@(); if(Test-Path $p){ $lines=@(Get-Content $p | % { $_.Trim() }) }
  $okc=@($lines | ? { $_ -eq 'C:\Users\@WINUSER@\r18bj\app' }).Count
  "${mode}_tabenter: $okc/10 landed in ...\r18bj\app; all=[$((($lines | select -Unique) -join ' | '))]"
  Shot "LT_${mode}_tab"
  if(Test-Path $p){ Move-Item $p "$R\out\${mode}_tabenter.txt" -Force }
  "=== $mode big paste 1.1 MB"
  $big='a'*1153433
  Set-Clipboard $big
  [void](Fg $h); [M]::Click(700,$iy)
  $pp=Get-Process phosphor-oss | ? { $_.Path -like '*r18bj*' } | select -First 1
  $sw=[Diagnostics.Stopwatch]::StartNew(); [K]::Down(0x11); [K]::Down(0x56); [K]::Up(0x56); [K]::Up(0x11)
  $resp=@(); foreach($i in 1..10){ Start-Sleep -Milliseconds 500; $pp.Refresh(); $resp+=$pp.Responding }
  "bigpaste responding samples (every 0.5 s x10): $($resp -join ',') elapsed=$($sw.ElapsedMilliseconds) ms ws=$([int]($pp.WorkingSet64/1MB))MB"
  Shot "LT_${mode}_bigpaste"
  [void][K]::Combo([byte[]]@(0x11),0x43); Start-Sleep 1; Keys "^a{BACKSPACE}"; Start-Sleep 2
  Set-Clipboard "x"
  Shot "LT_${mode}_bigpaste_after"
  SaveLog "LT_${mode}_app"
  KillApp
}
"LT_DONE"
Stop-Transcript | Out-Null
