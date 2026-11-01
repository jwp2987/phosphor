# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
$r="C:\Users\@WINUSER@\r18ba"
$f="$r\PhosphorSetup.exe"
(Get-FileHash $f).Hash
Get-Process phosphor-oss -ea 0 | % { "running (not mine): $($_.Id)" }
$p = Start-Process $f -ArgumentList '/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/CURRENTUSER',"/DIR=$r\app","/LOG=$r\install.log" -Wait -PassThru
"installer exit $($p.ExitCode)"
Start-Sleep 3
Get-Process phosphor-oss -ea 0 | % { "proc $($_.Id) $($_.Path)" }
Get-ChildItem "$r\app" | select -expand Name
(Get-Item "$r\app\phosphor-oss.exe").VersionInfo.ProductVersion
(Get-Item "$env:LOCALAPPDATA\Programs\Phosphor\phosphor-oss.exe").VersionInfo.ProductVersion
[Environment]::GetEnvironmentVariable('Path','User')
