# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
param([string]$Out, [string]$Dir = 'C:\Users\@WINUSER@\winhooks\pm')
. (Join-Path $Dir 'cs.ps1'); Add-Type -TypeDefinition $src
[void][CPty]::SetDllDirectory('C:\Users\@WINUSER@\winhooks\cp')
$esc=[char]27
[CPty]::Start("powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -NoExit")
Start-Sleep 4
$res = @()
foreach ($mode in 'raw','raw2','w32') {
  [CPty]::Send(" Start-Sleep 10; 'DONE'`r"); Start-Sleep 1.5
  $m = [CPty]::Count()
  switch ($mode) {
    'raw' { [CPty]::Send([string][char]3) }
    'raw2' { [CPty]::Send([string][char]3); Start-Sleep -Milliseconds 300; [CPty]::Send([string][char]3) }
    'w32' { [CPty]::Send("$esc[17;29;0;1;8;1_$esc[67;46;3;1;8;1_$esc[67;46;3;0;8;1_$esc[17;29;0;0;0;1_") }
  }
  Start-Sleep 3
  $res += "$mode => " + ([CPty]::Concat($m) -replace "`e", '<ESC>')
  Start-Sleep 12
}
[IO.File]::WriteAllText($Out, ($res -join "`n")); [CPty]::Kill()
