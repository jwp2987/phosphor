# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
. C:\Users\@WINUSER@\r18bj\tc.ps1
$global:R="C:\Users\@WINUSER@\r18bj"
Get-Content C:\Users\@WINUSER@\r18bj\settings_agent.toml -Raw | Out-Null
$t=Get-Content "$global:R\settings_agent.toml" -Raw
Set-Content -Encoding UTF8 "$env:LOCALAPPDATA\phosphor\Phosphor\config\settings.toml" $t
FreshApp "info" 14
Wins | ? { $_ -match 'phosphor|Phosphor' }
Shot "t0_agent"
Rect
KillApp
