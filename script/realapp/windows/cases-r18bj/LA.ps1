# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
Start-Transcript "$R\out\LA_transcript.txt" -Force | Out-Null
. C:\Users\@WINUSER@\r18bj\lat.ps1
. C:\Users\@WINUSER@\r18bj\tc.ps1
. C:\Users\@WINUSER@\r18bj\tc2.ps1
LatInit 700 200
$L='bj'; $o="$R\out\lat_${L}_LA.csv"; Remove-Item $o -ea 0
$env:PHOSPHOR_GPU_TIMING="1"
foreach($mode in 'agent','term'){
  SetMode $mode
  FreshApp2 "info,warp::input_order=debug,warpui::scene_stats=debug,warpui::frame_cause=debug" 14
  [M]::Click(700,650); Start-Sleep 2
  Shot "LA_${mode}_start"
  $pp=Get-Process phosphor-oss | ? { $_.Path -like '*r18bj*' } | select -First 1
  # idle (blink) window, no input
  $c0=$pp.TotalProcessorTime.TotalMilliseconds; $t0=Get-Date; Start-Sleep 20; $pp.Refresh(); $c1=$pp.TotalProcessorTime.TotalMilliseconds
  "$mode idle: wall 20 s cpu_ms=$([math]::Round($c1-$c0)) => $([math]::Round(($c1-$c0)/200,2)) % of one core; idle window UTC $($t0.ToUniversalTime().ToString('HH:mm:ss'))-$($t0.AddSeconds(20).ToUniversalTime().ToString('HH:mm:ss'))"
  "$mode idle mem ws=$([int]($pp.WorkingSet64/1MB))MB"
  LatRun "$L|$mode-key" 274 560 "zqxjkvwpyz" 40 $o "^a{BACKSPACE}" 20
  Shot "LA_${mode}_typed"
  # Enter -> command start from the main input
  $f="$R\out\start_LA_$mode.txt"; Remove-Item $f -ea 0
  for($k=0;$k -lt 24;$k++){
    $cmd="[IO.File]::AppendAllText('$f',[string]([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())+[char]10)"
    $res=[LT]::Burst(274,560,$cmd,10,$true); Add-Content $o "$L|$mode-start-stamp,$k,$res"; Start-Sleep -Milliseconds 2500 }
  Add-Content $o ("$L|$mode-start-stamps," + ((Get-Content $f -ea 0) -join ';'))
  Shot "LA_${mode}_end"
  SaveLog "LA_${mode}_app"
  KillApp
}
"LA_DONE"
Stop-Transcript | Out-Null
