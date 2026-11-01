# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
$g="C:\Program Files\Git\cmd\git.exe"
$d="C:\Users\@WINUSER@\r18ba\repo"; Remove-Item $d -Recurse -Force -ea 0; New-Item -ItemType Directory $d | Out-Null; Set-Location $d
& $g init -q . ; & $g config user.email t@t; & $g config user.name t
1..300 | % { Set-Content "f$_.txt" "file $_" }
& $g add -A; & $g commit -qm init
1..40 | % { Add-Content "f$_.txt" "changed" }
"repo ok"; & $g status --short | measure | % Count
