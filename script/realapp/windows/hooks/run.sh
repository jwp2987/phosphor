#!/bin/bash
# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
scp -q prof.ps1 ${PHOSPHOR_WIN_SSH}:C:/Users/@WINUSER@/prof.ps1
scp -q "$1" ${PHOSPHOR_WIN_SSH}:C:/Users/@WINUSER@/winhooks_cur.ps1
ssh ${PHOSPHOR_WIN_SSH} 'powershell -NoProfile -ExecutionPolicy Bypass -File C:\Users\@WINUSER@\prof.ps1 C:\Users\@WINUSER@\winhooks_cur.ps1 > NUL 2>&1'
ssh ${PHOSPHOR_WIN_SSH} 'type C:\Users\@WINUSER@\prof_res.txt' | tee "$2"
