# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
$sh=New-Object -ComObject WScript.Shell
foreach($d in "$env:APPDATA\Microsoft\Windows\Start Menu\Programs","$env:USERPROFILE\Desktop","$env:PUBLIC\Desktop","$env:ProgramData\Microsoft\Windows\Start Menu\Programs"){
  Get-ChildItem $d -Recurse -Filter *.lnk -ea 0 | ? { $_.Name -match 'hosphor|zap' -or $_.LastWriteTime -gt (Get-Date).AddHours(-3) } | % { "$($_.FullName) -> $($sh.CreateShortcut($_.FullName).TargetPath)  [$($_.LastWriteTime)]" } }
Get-Process phosphor-oss,bash,mintty,conhost,powershell -ea 0 | select Id,Name,StartTime | ft | out-string
"HKCU Run:"; Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -ea 0 | fl | out-string
Get-ChildItem 'HKCU:\Software\Classes' -ea 0 | ? { $_.Name -match 'hosphor|zap' } | select -expand Name
reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\zap-oss_is1" /v InstallLocation
