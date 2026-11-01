#!/bin/bash
# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
# usage: run.sh bodyfile [timeout]
D=$(dirname $(readlink -f $0)); V=${PHOSPHOR_WIN_SSH}
{ printf '\xEF\xBB\xBF'; printf '$global:R="C:\\Users\\@WINUSER@\\r18wk"\n. C:\\Users\\@WINUSER@\\r18wk\\lib.ps1\n. C:\\Users\\@WINUSER@\\r18wk\\mouse.ps1\n. C:\\Users\\@WINUSER@\\r18wk\\lib2.ps1\n'; cat "$1"; } > $D/_step.ps1
timeout 60 scp -q $D/_step.ps1 $V:r18wk/step.ps1 || exit 9
timeout $(( ${2:-120} + 60 )) ssh -o ServerAliveInterval=15 $V "powershell -NoProfile -ExecutionPolicy Bypass -File C:\\Users\\@WINUSER@\\indesk.ps1 C:\\Users\\@WINUSER@\\r18wk\\step.ps1 ${2:-120}" 2>&1 | head -c 30000 | tr -d '\r'
