# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
Start-Transcript "$R\out\LF_transcript.txt" -Force | Out-Null
Add-Type @"
using System; using System.Runtime.InteropServices;
public class SM { [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h,uint m,IntPtr w,IntPtr l);
 [DllImport("user32.dll")] public static extern IntPtr SendMessageTimeout(IntPtr h,uint m,IntPtr w,IntPtr l,uint f,uint t,out IntPtr res); }
"@
FreshApp "info,warp::input_order=debug" 15
[M]::Click(300,660); Start-Sleep 1
Cmd 'echo session-end-test-1' 2500
Keys "^+t"; Start-Sleep 6; Focus; Cmd 'echo second-tab' 2500
Shot F_before
$h=PhoWin; $pidApp=(Get-Process phosphor-oss | ? { $_.Path -like '*r18ba*' }).Id
"window=$h pid=$pidApp"
$res=[IntPtr]::Zero
$sw=[Diagnostics.Stopwatch]::StartNew()
$ok=[SM]::SendMessageTimeout($h,0x11,[IntPtr]0,[IntPtr]0,0,10000,[ref]$res)
"QUERYENDSESSION(shutdown/restart) ok=$ok reply=$res in $($sw.ElapsedMilliseconds) ms"
Start-Sleep 2
$sw.Restart(); $ok=[SM]::SendMessageTimeout($h,0x16,[IntPtr]0,[IntPtr]0,0,10000,[ref]$res)   # ENDSESSION(FALSE) = cancelled
"ENDSESSION(FALSE) ok=$ok in $($sw.ElapsedMilliseconds) ms"
Start-Sleep 3
"after cancel: alive=" + [bool](Get-Process -Id $pidApp -ea 0) + " windows=" + @(Wins | ? { $_ -match "pid=$pidApp" }).Count
Shot F_after_cancel
SaveLog F_cancel
# now a real session end (emulated): WM_QUERYENDSESSION then WM_ENDSESSION(TRUE, logoff-less restart)
$ok=[SM]::SendMessageTimeout($h,0x11,[IntPtr]0,[IntPtr]0,0,10000,[ref]$res); "QUERY#2 reply=$res"
$sw.Restart(); $ok=[SM]::SendMessageTimeout($h,0x16,[IntPtr]1,[IntPtr]0,0,15000,[ref]$res)
"ENDSESSION(TRUE) ok=$ok returned in $($sw.ElapsedMilliseconds) ms"
for($i=0;$i -lt 30 -and (Get-Process -Id $pidApp -ea 0);$i++){ Start-Sleep -Milliseconds 200 }
"process gone after total $($sw.ElapsedMilliseconds) ms alive=" + [bool](Get-Process -Id $pidApp -ea 0)
Start-Sleep 1
SaveLog F_end
Get-Content "$R\out\F_end.log" | ? { $_ -match 'session-end|graceful|app_will_terminate|shutdown' } | select -last 20
Stop-Transcript | Out-Null
