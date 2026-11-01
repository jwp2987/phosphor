# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
$c = '"C:\Program Files\Git\bin\bash.exe" --noprofile --rcfile /c/Users/@WINUSER@/bashstall/rc_real0.sh -i'
& C:\Users\@WINUSER@\bashstall\host5.ps1 -Dll inbox -Reps 60 -Ms 25 -Tag real0 -Cmd $c *> C:\Users\@WINUSER@\bashstall\out_real0.txt
