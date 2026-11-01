#!/bin/bash
# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
cd $HOME/git/phosphor
while true; do
 p=$(glab api "projects/88085499/pipelines?ref=build/r18wk" | python3 -c "import json,sys;print(json.load(sys.stdin)[0]['id'])")
 s=$(glab api projects/88085499/pipelines/$p/jobs | python3 -c "import json,sys;[print(j['id'],j['status']) for j in json.load(sys.stdin) if j['name']=='build-windows-x86_64']")
 case "$s" in *success*|*failed*|*canceled*) echo "$s"; break;; esac; sleep 60; done
