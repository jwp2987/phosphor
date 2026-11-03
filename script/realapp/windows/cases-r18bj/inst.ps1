# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
$r="C:\Users\@WINUSER@\r18bj"
$f="C:\Users\@WINUSER@\r18bj_PhosphorSetup.exe"
(Get-FileHash $f -Algorithm SHA256).Hash
$p = Start-Process $f -ArgumentList '/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/CURRENTUSER',"/DIR=$r\app","/LOG=$r\install.log" -Wait -PassThru
"installer exit $($p.ExitCode)"
Start-Sleep 3
Get-ChildItem "$r\app" | select -expand Name | select -first 20
(Get-Item "$r\app\phosphor-oss.exe").VersionInfo.ProductVersion
(Get-Item "$env:LOCALAPPDATA\Programs\Phosphor\phosphor-oss.exe").VersionInfo.ProductVersion
Get-ChildItem "$env:LOCALAPPDATA\Programs\Phosphor" -Recurse -File | % { "{0} {1}" -f (Get-FileHash $_.FullName -Algorithm MD5).Hash, $_.FullName } | sort | Set-Content "$r\md5_install_now.txt"
"install diffs: " + @(Compare-Object (Get-Content "$r\md5_install_orig.txt") (Get-Content "$r\md5_install_now.txt")).Count
reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall" /s /f Phosphor /d | select -first 20
reg query "HKCU\Software\Classes\phosphor\shell\open\command" /ve
([Environment]::GetEnvironmentVariable('Path','User') -eq (Get-Content "$r\path_user_orig.txt" -Raw).TrimEnd("`r","`n"))
