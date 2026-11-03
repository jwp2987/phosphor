# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
#
# #1103 part 2 of 2, right after RS.ps1 in the same app: click "Restart shell" in the visible pane (tab 2).
# Set $BX/$BY to the button's centre read off RS_after_cancel.png (screen pixels) before running. Expected:
#   - the log has "Restarting the exited shell of pane" once, and a new shell process appears under Phosphor;
#   - the pane keeps its earlier output ('second-tab-1103' above a restored-session separator) and its input holds 'echo typed-1103',
#     which was NOT run by the restart (RS_after_restart.png shows it unsent);
#   - Enter then runs it in the new shell (RS_ran.png shows 'typed-1103' output), and a fresh command runs too.
# Tab 1 keeps its own banner and button until it is clicked: one click restarts one pane.
$BX=0; $BY=0
Start-Transcript "$R\out\RS2_transcript.txt" -Force | Out-Null
if($BX -eq 0 -or $BY -eq 0){ "set BX/BY from RS_after_cancel.png first"; Stop-Transcript | Out-Null; return }
Focus
$pidApp=(Get-Process phosphor-oss | ? { $_.Path -like '*r18ba*' } | select -First 1).Id
$before=@(Get-CimInstance Win32_Process -Filter "ParentProcessId=$pidApp" | ? { $_.Name -notmatch '^(OpenConsole|conhost)\.exe$' }).Count
[M]::Click($BX,$BY); Start-Sleep 8
$after=@(Get-CimInstance Win32_Process -Filter "ParentProcessId=$pidApp" | ? { $_.Name -notmatch '^(OpenConsole|conhost)\.exe$' }).Count
"shells before click=$before after click=$after"
Shot RS_after_restart
Focus
Keys "{ENTER}"; Start-Sleep 2
Cmd 'echo after-restart-1103' 2500
Shot RS_ran
SaveLog RS_restart
"restarts: " + @(Get-Content "$R\out\RS_restart.log" | ? { $_ -match 'Restarting the exited shell of pane' }).Count
"RS2_DONE"
Stop-Transcript | Out-Null
