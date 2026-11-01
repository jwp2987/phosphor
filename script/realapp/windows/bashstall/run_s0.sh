#!/bin/bash
# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
# usage: run.sh tag reps rcfile|--norc [ms]
tag=$1; reps=$2; rc=$3; ms=${4:-25}
V=${PHOSPHOR_WIN_SSH}
if [ "$rc" = "--norc" ]; then args='--noprofile --norc -i'; else
 scp -q /tmp/bs/$rc $V:C:/Users/@WINUSER@/bashstall/$rc
 args="--noprofile --rcfile /c/Users/@WINUSER@/bashstall/$rc -i"; fi
printf '%s\n' "\$c = '\"C:\\Program Files\\Git\\bin\\bash.exe\" $args'" \
 "& C:\\Users\\@WINUSER@\\bashstall\\host5.ps1 -Dll inbox -Settle 0 -Reps $reps -Ms $ms -Tag $tag -Cmd \$c *> C:\\Users\\@WINUSER@\\bashstall\\out_$tag.txt" > /tmp/bs/launch_$tag.ps1
scp -q /tmp/bs/launch_$tag.ps1 $V:C:/Users/@WINUSER@/bashstall/launch_$tag.ps1
ssh $V "powershell -NoProfile -ExecutionPolicy Bypass -File C:\\Users\\@WINUSER@\\bashstall\\launch_$tag.ps1"
ssh $V "type C:\\Users\\@WINUSER@\\bashstall\\out_$tag.txt" > /tmp/bs/out_$tag.txt
tail -1 /tmp/bs/out_$tag.txt
