# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
$r="C:\Users\@WINUSER@\r18wk"
Get-Process phosphor-oss -ea 0 | % { "running: $($_.Id) $($_.Path)" }
if(-not (Test-Path "$r\bak\local")){ Copy-Item "$env:LOCALAPPDATA\phosphor" "$r\bak\local" -Recurse; "backed up local" }
if((Test-Path "$env:APPDATA\phosphor") -and -not (Test-Path "$r\bak\roaming")){ Copy-Item "$env:APPDATA\phosphor" "$r\bak\roaming" -Recurse; "backed up roaming" }
Get-ChildItem "$env:LOCALAPPDATA\phosphor","$env:APPDATA\phosphor" -Recurse -File -ea 0 | % { "{0} {1}" -f (Get-FileHash $_.FullName -Algorithm MD5).Hash, $_.FullName } | sort | Set-Content "$r\md5_now.txt"
"data md5 vs r18ba orig: diffs=" + @(Compare-Object (Get-Content "$r\md5_orig.txt") (Get-Content "$r\md5_now.txt")).Count
Get-ChildItem "$env:LOCALAPPDATA\Programs\Phosphor" -Recurse -File | % { "{0} {1}" -f (Get-FileHash $_.FullName -Algorithm MD5).Hash, $_.FullName } | sort | Set-Content "$r\md5_install_now.txt"
"install md5 diffs=" + @(Compare-Object (Get-Content "$r\md5_install_orig.txt") (Get-Content "$r\md5_install_now.txt")).Count
[Environment]::GetEnvironmentVariable('Path','User') | Set-Content "$r\path_now.txt"
"path same: " + ((Get-Content "$r\path_now.txt" -Raw) -eq (Get-Content "$r\path_user_orig.txt" -Raw))
(Get-Item "$env:LOCALAPPDATA\Programs\Phosphor\phosphor-oss.exe").VersionInfo.ProductVersion
Get-Content "$env:LOCALAPPDATA\phosphor\Phosphor\config\settings.toml"
