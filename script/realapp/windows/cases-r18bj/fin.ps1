# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
$r="C:\Users\@WINUSER@\r18bj"
Get-Process phosphor-oss -ea 0 | ? { $_.Path -like '*r18bj*' } | % { "taskkill exact pid $($_.Id)"; & taskkill /PID $_.Id /T /F | Out-Null }
Start-Sleep 3
Get-Process phosphor-oss -ea 0 | % { "STILL RUNNING phosphor-oss $($_.Id) $($_.Path)" }
Remove-Item "$env:LOCALAPPDATA\phosphor" -Recurse -Force -ea 0; Copy-Item "$r\bak\local" "$env:LOCALAPPDATA\phosphor" -Recurse
if(Test-Path "$r\bak\roaming"){ Remove-Item "$env:APPDATA\phosphor" -Recurse -Force -ea 0; Copy-Item "$r\bak\roaming" "$env:APPDATA\phosphor" -Recurse }
Get-ChildItem "$env:LOCALAPPDATA\phosphor","$env:APPDATA\phosphor" -Recurse -File -ea 0 | % { "{0} {1}" -f (Get-FileHash $_.FullName -Algorithm MD5).Hash, $_.FullName } | sort | Set-Content "$r\md5_final.txt"
$d=Compare-Object (Get-Content "$r\md5_orig.txt") (Get-Content "$r\md5_final.txt")
"data md5 files orig=$((Get-Content "$r\md5_orig.txt").Count) final=$((Get-Content "$r\md5_final.txt").Count) diffs=$(@($d).Count)"
& "$r\restore_sys.ps1"
Copy-Item "$r\bak\Programs-Phosphor.lnk" "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Phosphor.lnk" -Force
Copy-Item "$r\bak\Desktop-Phosphor.lnk" "$env:USERPROFILE\Desktop\Phosphor.lnk" -Force
"shortcuts restored from byte backups"
"--- stray processes"
Get-Process phosphor-oss,bash,mintty,conhost,powershell,gitlab-runner -ea 0 | select Id,Name,StartTime | ft -auto | out-string -width 200
