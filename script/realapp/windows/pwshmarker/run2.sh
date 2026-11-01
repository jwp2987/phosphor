#!/bin/bash
# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
# usage: run.sh mode N [exe]
cd ${PHOSPHOR_WORK}/pwshmarker
V=${PHOSPHOR_WIN_SSH}; D='C:\Users\@WINUSER@\winhooks\pm'
ssh $V "if not exist $D mkdir $D" >/dev/null
scp -q cs.ps1 l1.txt l2.txt wrap.txt pm2_body.ps1 $V:C:/Users/@WINUSER@/winhooks/pm/
ssh $V "powershell -NoProfile -ExecutionPolicy Bypass -File $D\\pm2_body.ps1 -Mode $1 -N $2 -Out $D\\res2_$1.txt ${3:+-Exe $3} > $D\\err2_$1.txt 2>&1"
ssh $V "type $D\\res2_$1.txt" > res2_$1.txt; head -1 res2_$1.txt; ssh $V "type $D\\err2_$1.txt" | head -5
