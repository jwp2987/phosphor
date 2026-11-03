# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
param([Parameter(Mandatory)][string]$Script,[string]$Name='r18ba-bg')
$runner = "$env:TEMP\$Name-run.cmd"
Set-Content -Encoding ascii $runner "@powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File `"$Script`""
schtasks /Delete /TN $Name /F 2>$null | Out-Null
schtasks /Create /TN $Name /TR $runner /SC ONCE /ST 23:59 /IT /F 2>&1 | Out-Null
schtasks /Run /TN $Name | Out-Null
