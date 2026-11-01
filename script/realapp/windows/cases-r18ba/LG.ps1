# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
Start-Transcript "$R\out\LG_transcript.txt" -Force | Out-Null
Add-Type -AssemblyName System.Windows.Forms
FreshApp "info,warp::input_order=debug,warp::terminal::model_lock_trace=debug" 15
[M]::Click(300,660); Start-Sleep 1
$h=PhoWin
# --- smoke: cmd inside PS tab
Cmd 'cmd /c echo cmd-c-ok' 2000
Cmd 'cmd' 2500; Cmd 'echo hi-from-cmd' 1500; Cmd 'exit' 1500
Shot G_cmd
# --- scrollback for resize / find
Cmd '1..20000 | % { "line $_ " + ("x" * 90) }' 12000
Shot G_scroll
# --- (2) resize stalls with 20000 lines of scrollback
$r0=New-Object W+RECT; [void][W]::GetWindowRect($h,[ref]$r0)
$sizes=@(@(1232,712),@(900,600),@(700,500),@(1100,700),@(1232,712),@(800,650),@(1000,712),@(650,420),@(1232,712),@(950,640),@(1232,712),@(700,700),@(1232,712))
$durs=@()
foreach($s in $sizes){ $sw=[Diagnostics.Stopwatch]::StartNew(); [void][W]::MoveWindow($h,24,24,$s[0],$s[1],$true); $d=$sw.Elapsed.TotalMilliseconds; $durs+=$d; "resize -> $($s[0])x$($s[1]) MoveWindow took $([math]::Round($d,1)) ms"; Start-Sleep -Milliseconds 700 }
"resize MoveWindow ms: max=$([math]::Round(($durs|measure -Max).Maximum,1)) avg=$([math]::Round(($durs|measure -Average).Average,1))"
# rapid drag-like resize: 60 steps with no pause
$durs2=@(); for($i=0;$i -lt 60;$i++){ $w=1000+[int](230*[math]::Sin($i/4.0)); $sw=[Diagnostics.Stopwatch]::StartNew(); [void][W]::MoveWindow($h,24,24,$w,712,$true); $durs2+=$sw.Elapsed.TotalMilliseconds }
"rapid 60-step resize MoveWindow ms: max=$([math]::Round(($durs2|measure -Max).Maximum,1)) avg=$([math]::Round(($durs2|measure -Average).Average,1)) >100ms: $(@($durs2|?{$_ -gt 100}).Count)"
[void][W]::MoveWindow($h,24,24,1232,712,$true); Start-Sleep 2
Shot G_resized
# responsiveness right after resize: key->paint
. C:\Users\@WINUSER@\r18ba\lat.ps1; LatInit 700 200
Focus; [M]::Click(300,660); Start-Sleep 1
$o="$R\out\lat_r18ba_G.csv"; Remove-Item $o -ea 0
LatRun "r18ba|ps-afterresize-key" 0 520 "zqxjkvwpyz" 20 $o "^a{BACKSPACE}" 20
# --- (3) copy then paste immediately (#1307)
Set-Clipboard "OLDTEXT-STALE"
Keys "NEWTEXT123"; Start-Sleep -Milliseconds 400
Keys "^a"; Start-Sleep -Milliseconds 200
Keys "^+c^+v"
Start-Sleep -Milliseconds 800
Shot G_copypaste
"clipboard after copy+paste: " + (Get-Clipboard -Raw)
Keys "^a{BACKSPACE}"; Start-Sleep -Milliseconds 300
# --- (4) paste while another process holds the clipboard (#1305/#1306)
Set-Clipboard "HELDPASTE"
$hp = Start-Process powershell -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-WindowStyle','Hidden','-File','C:\Users\@WINUSER@\r18ba\hold.ps1','-Seconds','5' -PassThru -RedirectStandardOutput "$R\out\hold_out.txt" -WindowStyle Hidden
Start-Sleep -Milliseconds 1500
Focus
$t0=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
Keys "^+v"
$t1=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds(); "paste key sent at +$($t1-$t0) ms (SendWait returned)"
LatRun "r18ba|ps-held-clip-key" 0 520 "abcdef" 6 $o "" 0 100 150 300
$t2=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds(); "6 key samples done at +$($t2-$t0) ms"
Shot G_held_during
Wait-Process -Id $hp.Id -Timeout 12 -ea 0
Start-Sleep 2
Shot G_held_after
"hold out: " + ((Get-Content "$R\out\hold_out.txt") -join ' | ')
Keys "^a{BACKSPACE}"; Start-Sleep -Milliseconds 300
# --- right-click menu with text clipboard
Set-Clipboard "rc-text"
$sw=[Diagnostics.Stopwatch]::StartNew(); Click 600 300 $true; Start-Sleep -Milliseconds 600; Shot G_rclick; Keys "{ESC}"
# --- find
Start-Sleep -Milliseconds 500
Keys "^+f"; Start-Sleep -Milliseconds 800; Keys "line 199"; Start-Sleep -Milliseconds 1500; Shot G_find; Keys "{ESC}"
SaveLog G_app
Get-Process phosphor-oss | ? { $_.Path -like '*r18ba*' } | % { "app ws=$([int]($_.WorkingSet64/1MB))MB threads=$($_.Threads.Count)" }
Stop-Transcript | Out-Null
