# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
Remove-Item C:\Users\@WINUSER@\winhooks -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item C:\Users\@WINUSER@\prof.ps1,C:\Users\@WINUSER@\prof_*.txt,C:\Users\@WINUSER@\winhooks_*.ps1,C:\Users\@WINUSER@\dbg.ps1,C:\Users\@WINUSER@\st_wh.ps1,C:\Users\@WINUSER@\ls_wh.ps1,C:\Users\@WINUSER@\ls2_wh.ps1,C:\Users\@WINUSER@\rd_wh.ps1 -ErrorAction SilentlyContinue
Get-Process OpenConsole -ErrorAction SilentlyContinue | % { "OpenConsole $($_.Id) $($_.StartTime)" }
