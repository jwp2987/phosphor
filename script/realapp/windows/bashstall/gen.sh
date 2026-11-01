#!/bin/bash
# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
# usage: gen.sh bootstrapdir > rc.sh
d=$1
sed -e 's/@@WARP_SESSION_ID@@/1/' -e 's/@@USING_CON_PTY_BOOLEAN@@/true/' -e '/^command -p stty raw$/d' $d/bash_init_shell.sh
echo
while IFS= read -r line; do
  if [[ $line == "#include bundled/bootstrap/bash_body.sh" ]]; then cat $d/bash_body.sh; else printf '%s\n' "$line"; fi
done < $d/bash.sh
