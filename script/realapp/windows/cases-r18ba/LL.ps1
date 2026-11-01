# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
Start-Transcript "$R\out\LL_transcript.txt" -Force | Out-Null
FreshApp "info,warp::input_order=debug" 15
[M]::Click(300,660); Start-Sleep 1
Keys (Esc 'echo @(1,2,3).Count'); Start-Sleep -Milliseconds 1200; Shot L_typed
Keys "{ENTER}"; Start-Sleep 3; Shot L_enter1
Keys "{ENTER}"; Start-Sleep 3; Shot L_enter2
Keys "^a{BACKSPACE}"; Start-Sleep -Milliseconds 500
Keys (Esc 'echo hello @world'); Start-Sleep -Milliseconds 1200; Shot L_typed2
Keys "{ENTER}"; Start-Sleep 3; Shot L_enter3
SaveLog L_app
Stop-Transcript | Out-Null
