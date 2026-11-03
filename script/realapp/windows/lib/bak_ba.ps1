# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
$r="C:\Users\@WINUSER@\r18ba"
New-Item -ItemType Directory -Force "$r","$r\out","$r\shots","$r\bak" | Out-Null
foreach($f in 'lib.ps1','mouse.ps1','lat.ps1','tc.ps1'){ (Get-Content "C:\Users\@WINUSER@\r18av\$f" -Raw) -replace 'r18av','r18ba' | Set-Content -Encoding UTF8 "$r\$f" }
Get-Process phosphor-oss -ea 0 | % { "running: $($_.Id) $($_.Path)" }
if(-not (Test-Path "$r\bak\local")){ Copy-Item "$env:LOCALAPPDATA\phosphor" "$r\bak\local" -Recurse; "backed up local" }
if((Test-Path "$env:APPDATA\phosphor") -and -not (Test-Path "$r\bak\roaming")){ Copy-Item "$env:APPDATA\phosphor" "$r\bak\roaming" -Recurse; "backed up roaming" }
Get-ChildItem "$env:LOCALAPPDATA\phosphor","$env:APPDATA\phosphor" -Recurse -File -ea 0 | % { "{0} {1}" -f (Get-FileHash $_.FullName -Algorithm MD5).Hash, $_.FullName } | sort | Set-Content "$r\md5_orig.txt"
(Get-Content "$r\md5_orig.txt").Count
"matches r18av final: diffs=" + @(Compare-Object (Get-Content "C:\Users\@WINUSER@\r18av\md5_orig.txt") (Get-Content "$r\md5_orig.txt")).Count
[Environment]::GetEnvironmentVariable('Path','User') | Set-Content "$r\path_user_orig.txt"
"PATH same as r18av orig: " + ((Get-Content "$r\path_user_orig.txt" -Raw) -eq (Get-Content C:\Users\@WINUSER@\r18av\path_user_orig.txt -Raw))
reg export "HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\zap-oss_is1" "$r\uninstall_zap-oss_is1.reg" /y
reg export "HKCU\Software\Classes\phosphor" "$r\phosphor_handler.reg" /y
foreach($l in "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Phosphor.lnk","$env:USERPROFILE\Desktop\Phosphor.lnk"){ if(Test-Path $l){ Copy-Item $l "$r\bak\$(Split-Path $l -Parent | Split-Path -Leaf)-Phosphor.lnk"; $s=(New-Object -ComObject WScript.Shell).CreateShortcut($l); "$l -> $($s.TargetPath) args='$($s.Arguments)' wd='$($s.WorkingDirectory)' icon='$($s.IconLocation)'" } else { "$l MISSING" } }
Get-ChildItem "$env:APPDATA\Microsoft\Windows\Start Menu\Programs" -Filter *hosphor* -Recurse | % FullName
(Get-Item "$env:LOCALAPPDATA\Programs\Phosphor\phosphor-oss.exe").VersionInfo.ProductVersion
Get-ChildItem "$env:LOCALAPPDATA\Programs\Phosphor" -Recurse -File | % { "{0} {1}" -f (Get-FileHash $_.FullName -Algorithm MD5).Hash, $_.FullName } | sort | Set-Content "$r\md5_install_orig.txt"
"install diff vs r18av: " + @(Compare-Object (Get-Content C:\Users\@WINUSER@\r18av\md5_install_orig.txt) (Get-Content "$r\md5_install_orig.txt")).Count
reg query "HKCU\Software\Classes\phosphor\shell\open\command" /ve
Get-Content "$env:LOCALAPPDATA\phosphor\Phosphor\config\settings.toml"
Get-Process | ? Name -match 'phosphor|bash|mintty|conhost|runner' | select Name,Id | ft | out-string
