# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
$r="C:\Users\@WINUSER@\r18bj"
reg import "$r\uninstall_zap-oss_is1.reg" 2>&1
reg delete "HKCU\Software\Classes\phosphor" /f 2>&1 | out-null
reg import "$r\phosphor_handler.reg" 2>&1
[Environment]::SetEnvironmentVariable('Path',((Get-Content "$r\path_user_orig.txt" -Raw).TrimEnd("`r","`n")),'User')
$sh=New-Object -ComObject WScript.Shell
foreach($l in "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Phosphor.lnk","$env:USERPROFILE\Desktop\Phosphor.lnk"){ $s=$sh.CreateShortcut($l); $s.TargetPath="C:\Users\@WINUSER@\AppData\Local\Programs\Phosphor\phosphor-oss.exe"; $s.Arguments=""; $s.WorkingDirectory="C:\Users\@WINUSER@\AppData\Local\Programs\Phosphor"; $s.IconLocation="C:\Users\@WINUSER@\AppData\Local\Programs\Phosphor\icon.ico,0"; $s.Save(); $c=$sh.CreateShortcut($l); "$l -> $($c.TargetPath) args='$($c.Arguments)' wd='$($c.WorkingDirectory)' icon='$($c.IconLocation)'" }
"PATH == orig: " + ([Environment]::GetEnvironmentVariable('Path','User') -eq (Get-Content "$r\path_user_orig.txt" -Raw).TrimEnd("`r","`n"))
"--- uninstall key"
reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\zap-oss_is1" /v InstallLocation
reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\zap-oss_is1" /v DisplayVersion
reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\zap-oss_is1" /v UninstallString
"--- handler"
reg query "HKCU\Software\Classes\phosphor\shell\open\command" /ve
$a=(Get-Content "$r\phosphor_handler.reg") -join "`n"; $n=(reg export "HKCU\Software\Classes\phosphor" "$r\h_now.reg" /y) ; "handler reg identical to backup: " + ((Get-Content "$r\h_now.reg") -join "`n" -eq $a)
$a=(Get-Content "$r\uninstall_zap-oss_is1.reg") -join "`n"; reg export "HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\zap-oss_is1" "$r\u_now.reg" /y | out-null; "uninstall reg identical to backup: " + ((Get-Content "$r\u_now.reg") -join "`n" -eq $a)
"--- start menu"
Get-ChildItem "$env:APPDATA\Microsoft\Windows\Start Menu\Programs" -Filter *hosphor* -Recurse | % FullName
"--- v0.1.15"
(Get-Item "$env:LOCALAPPDATA\Programs\Phosphor\phosphor-oss.exe").VersionInfo.ProductVersion
Get-ChildItem "$env:LOCALAPPDATA\Programs\Phosphor" -Recurse -File | % { "{0} {1}" -f (Get-FileHash $_.FullName -Algorithm MD5).Hash, $_.FullName } | sort | Set-Content "$r\md5_install_now.txt"
"v0.1.15 install files orig=$((Get-Content "$r\md5_install_orig.txt").Count) now=$((Get-Content "$r\md5_install_now.txt").Count) diffs=$(@(Compare-Object (Get-Content "$r\md5_install_orig.txt") (Get-Content "$r\md5_install_now.txt")).Count)"
