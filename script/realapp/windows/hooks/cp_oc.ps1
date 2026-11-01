# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
New-Item -ItemType Directory -Force C:\Users\@WINUSER@\winhooks\oc\x64 | Out-Null
Copy-Item C:\Users\@WINUSER@\r18wk\app\conpty.dll C:\Users\@WINUSER@\winhooks\oc\ -Force
Copy-Item C:\Users\@WINUSER@\r18wk\app\x64\OpenConsole.exe C:\Users\@WINUSER@\winhooks\oc\x64\ -Force
Get-ChildItem C:\Users\@WINUSER@\winhooks\oc -Recurse | select FullName,Length | out-string
