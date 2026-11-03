# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
$r="C:\Users\@WINUSER@\r18ba"
KillApp
foreach($id in 7260,19004,9708,14192,5148,13472){ $p=Get-Process -Id $id -ea 0; if($p){ "stopping my leftover $($p.Name) $id"; Stop-Process -Id $id -Force -ea 0 } }
Start-Sleep 2
Get-Process phosphor-oss,bash,mintty,conhost,powershell -ea 0 | select Id,Name,StartTime | ft | out-string
# data dirs
Remove-Item "$env:LOCALAPPDATA\phosphor" -Recurse -Force -ea 0; Copy-Item "$r\bak\local" "$env:LOCALAPPDATA\phosphor" -Recurse
Remove-Item "$env:APPDATA\phosphor" -Recurse -Force -ea 0; Copy-Item "$r\bak\roaming" "$env:APPDATA\phosphor" -Recurse
Get-ChildItem "$env:LOCALAPPDATA\phosphor","$env:APPDATA\phosphor" -Recurse -File -ea 0 | % { "{0} {1}" -f (Get-FileHash $_.FullName -Algorithm MD5).Hash, $_.FullName } | sort | Set-Content "$r\md5_final.txt"
$d=Compare-Object (Get-Content "$r\md5_orig.txt") (Get-Content "$r\md5_final.txt")
"md5 files orig=$((Get-Content "$r\md5_orig.txt").Count) final=$((Get-Content "$r\md5_final.txt").Count) diffs=$(@($d).Count)"
# registry / PATH / shortcuts
reg import "$r\uninstall_zap-oss_is1.reg" 2>&1
reg add "HKCU\Software\Classes\phosphor\shell\open\command" /ve /t REG_SZ /d "`"C:\Users\@WINUSER@\AppData\Local\Programs\Phosphor\phosphor-oss.exe`" `"%0`"" /f
[Environment]::SetEnvironmentVariable('Path',((Get-Content "$r\path_user_orig.txt" -Raw).TrimEnd("`r","`n")),'User')
$sh=New-Object -ComObject WScript.Shell
foreach($l in "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Phosphor.lnk","$env:USERPROFILE\Desktop\Phosphor.lnk"){ $s=$sh.CreateShortcut($l); $s.TargetPath="C:\Users\@WINUSER@\AppData\Local\Programs\Phosphor\phosphor-oss.exe"; $s.IconLocation="C:\Users\@WINUSER@\AppData\Local\Programs\Phosphor\icon.ico"; $s.Save(); "$l -> $($sh.CreateShortcut($l).TargetPath) wd='$($sh.CreateShortcut($l).WorkingDirectory)'" }
"PATH now == orig: " + ([Environment]::GetEnvironmentVariable('Path','User') -eq (Get-Content "$r\path_user_orig.txt" -Raw).TrimEnd("`r","`n"))
reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\zap-oss_is1" /v InstallLocation
reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\zap-oss_is1" /v DisplayVersion
(Get-Item "$env:LOCALAPPDATA\Programs\Phosphor\phosphor-oss.exe").VersionInfo.ProductVersion
Get-ChildItem "$env:LOCALAPPDATA\Programs\Phosphor" | measure | % { "v0.1.15 dir files: $($_.Count)" }
