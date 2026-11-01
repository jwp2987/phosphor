# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
param([string]$Out, [string]$Dir = 'C:\Users\@WINUSER@\winhooks\pm')
. (Join-Path $Dir 'cs.ps1'); Add-Type -TypeDefinition $src
[void][CPty]::SetDllDirectory('C:\Users\@WINUSER@\winhooks\cp')
[CPty]::Start("cmd.exe")
Start-Sleep 3
[CPty]::Send("ping -n 20 127.0.0.1`r"); Start-Sleep 2
$m = [CPty]::Count(); [CPty]::Send([string][char]3); Start-Sleep 2
[IO.File]::WriteAllText($Out, ([CPty]::Concat($m) -replace "`e", '<ESC>')); [CPty]::Kill()
