# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
#
# #1103 part 2 of 2, right after RS.ps1 in the same app (RS.ps1's header: back up first). Double-clicks "Restart shell" in the
# visible pane (tab 2). Set $BX/$BY to the button's centre read off RS_after_cancel.png (screen pixels) before running. Expected:
#   - exactly ONE restart for the double click: the log has "Restarting the exited shell of pane" once, and exactly one new shell
#     process appears under the test app (after = before + 1);
#   - the pane keeps its earlier output ('second-tab-1103' above a restored-session separator) and its input holds 'echo typed-1103',
#     which the restart did NOT run (checked in the log before any Enter; RS_after_restart.png shows it unsent);
#   - Enter then runs it in the new shell (RS_ran.png shows 'typed-1103' output), and a fresh command runs too.
# Tab 1 keeps its own banner and button until it is clicked: one click restarts one pane.
$BX=0; $BY=0
Start-Transcript "$R\out\RS2_transcript.txt" -Force | Out-Null
if($BX -eq 0 -or $BY -eq 0){ "set BX/BY from RS_after_cancel.png first"; Stop-Transcript | Out-Null; return }
$global:ShellImages = @('pwsh.exe','powershell.exe','cmd.exe','bash.exe','wsl.exe','zsh.exe','fish.exe','nu.exe')
function ShellsOf($appPid) { @(Get-CimInstance Win32_Process -Filter "ParentProcessId=$appPid" | ? { $global:ShellImages -contains $_.Name.ToLower() }) }
$app=$global:AppP
if(-not $app -or $app.HasExited){ $app=Get-Process phosphor-oss -ea 0 | ? { $_.Path -like '*r18ba*' -and $_.MainWindowHandle -ne 0 } | select -First 1 }
if(-not $app){ "ABORT: the test app is not running"; Stop-Transcript | Out-Null; return }
$pidApp=$app.Id
Focus
$before=(ShellsOf $pidApp).Count
[M]::Click($BX,$BY); Start-Sleep -Milliseconds 120; [M]::Click($BX,$BY)   # a double click must restart once
Start-Sleep 8
$after=(ShellsOf $pidApp).Count
"shells before click=$before after click=$after expected=$($before+1) " + $(if($after -eq $before+1){'PASS'}else{'FAIL'})
Shot RS_after_restart
SaveLog RS_restart_before_enter
$restarts=@(Get-Content "$R\out\RS_restart_before_enter.log" | ? { $_ -match 'Restarting the exited shell of pane' }).Count
"restarts: $restarts expected=1 " + $(if($restarts -eq 1){'PASS'}else{'FAIL'})
# The handed-back line must not have run: no command 'echo typed-1103' executed before the Enter below.
$ran=@(Get-Content "$R\out\RS_restart_before_enter.log" | ? { $_ -match 'typed-1103' -and $_ -match '(?i)execut|preexec|submit|write.*pty' }).Count
"typed-1103 executed before Enter: $ran expected=0 " + $(if($ran -eq 0){'PASS'}else{'FAIL'})
Focus
Keys "{ENTER}"; Start-Sleep 2
Cmd 'echo after-restart-1103' 2500
Shot RS_ran
SaveLog RS_restart
"RS2_DONE"
Stop-Transcript | Out-Null
