# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
#
# #1103 part 1 of 2 (run with ../lib/run.sh RS.ps1 180): a CANCELLED restart whose shells Windows already killed.
# Emulated as on the real restart screen: WM_QUERYENDSESSION is answered, then the shell processes under Phosphor are killed
# (Windows kills the console clients at query time), then WM_ENDSESSION(FALSE) cancels. Expected:
#   - the app and both tabs stay up (RS_after_cancel.png), each pane showing "Shell process exited" with a "Restart shell" button;
#   - the log has "Keeping the pane of an exited shell open: Windows session end" once per killed shell;
#   - tab 2's unsent line 'echo typed-1103' stays in its (disabled) input editor.
# Then read the button position off RS_after_cancel.png and run RS2.ps1 with it.
# Run it once with the owner's settings in agent mode and once in terminal mode (both modes, screenshot proof).
Start-Transcript "$R\out\RS_transcript.txt" -Force | Out-Null
Add-Type @"
using System; using System.Runtime.InteropServices;
public class SM1103 { [DllImport("user32.dll")] public static extern IntPtr SendMessageTimeout(IntPtr h,uint m,IntPtr w,IntPtr l,uint f,uint t,out IntPtr res); }
"@
FreshApp "info,warp::input_order=debug" 15
[M]::Click(300,660); Start-Sleep 1
Cmd 'echo before-restart-1103' 2500
Keys "^+t"; Start-Sleep 6; Focus
Cmd 'echo second-tab-1103' 2500
# an unsent line: it must survive the shell's death and come back in the restarted pane's editor, not be run
Keys (Esc 'echo typed-1103'); Start-Sleep 1
Shot RS_before
$h=PhoWin; $pidApp=$global:AppP.Id
"window=$h pid=$pidApp"
$shells = @(Get-CimInstance Win32_Process -Filter "ParentProcessId=$pidApp" | ? { $_.Name -notmatch '^(OpenConsole|conhost)\.exe$' })
"shells before: " + (($shells | % { "$($_.Name)/$($_.ProcessId)" }) -join ' ')
$res=[IntPtr]::Zero
$ok=[SM1103]::SendMessageTimeout($h,0x11,[IntPtr]0,[IntPtr]0,0,10000,[ref]$res)   # WM_QUERYENDSESSION
"QUERYENDSESSION ok=$ok reply=$res"
foreach($s in $shells){ Stop-Process -Id $s.ProcessId -Force -ea 0 }                 # what Windows does to console clients at query time
Start-Sleep 2
$ok=[SM1103]::SendMessageTimeout($h,0x16,[IntPtr]0,[IntPtr]0,0,10000,[ref]$res)   # WM_ENDSESSION(FALSE) = cancelled
"ENDSESSION(FALSE) ok=$ok"
Start-Sleep 3
"after cancel: alive=" + [bool](Get-Process -Id $pidApp -ea 0) + " windows=" + @(Wins | ? { $_ -match "pid=$pidApp" }).Count + " title='$(Title)'"
"shells after cancel: " + @(Get-CimInstance Win32_Process -Filter "ParentProcessId=$pidApp" | ? { $_.Name -notmatch '^(OpenConsole|conhost)\.exe$' }).Count
Shot RS_after_cancel
SaveLog RS_cancel
"kept panes: " + @(Get-Content "$R\out\RS_cancel.log" | ? { $_ -match 'Keeping the pane of an exited shell open: Windows session end' }).Count
"RS1_DONE"
Stop-Transcript | Out-Null
