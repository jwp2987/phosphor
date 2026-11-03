# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
Start-Transcript "$R\out\LI_transcript.txt" -Force | Out-Null
Add-Type @"
using System; using System.Runtime.InteropServices; using System.Diagnostics; using System.Threading;
public class TR2 {
 [DllImport("ntdll.dll")] public static extern int NtQueryTimerResolution(out uint min, out uint max, out uint cur);
 public static string Res(){ uint a,b,c; NtQueryTimerResolution(out a,out b,out c); return string.Format("min={0} max={1} cur={2} (100ns units; cur/10000 = {3} ms)",a,b,c,c/10000.0); }
 public static string Sleep1(){ var sw=Stopwatch.StartNew(); double t=0; double mx=0; for(int i=0;i<40;i++){ double a=sw.Elapsed.TotalMilliseconds; Thread.Sleep(1); double d=sw.Elapsed.TotalMilliseconds-a; t+=d; if(d>mx)mx=d;} return string.Format("Sleep(1) avg={0:F2} max={1:F2}",t/40,mx); }
}
"@
KillApp; Start-Sleep 3
"app NOT running : " + [TR2]::Res() + " | " + [TR2]::Sleep1()
FreshApp "info,warp::input_order=debug" 15
"app running     : " + [TR2]::Res() + " | " + [TR2]::Sleep1()
"app running (2) : " + [TR2]::Res()
. C:\Users\@WINUSER@\r18ba\lat.ps1
$h=PhoWin
$o="$R\out\lat_r18ba_I.csv"; Remove-Item $o -ea 0
function SizeRun($tag,$w,$hh,$x,$y,$n){ [void][W]::MoveWindow($h,24,24,$w,$hh,$true); Start-Sleep 2; Focus; [M]::Click(24+200,24+$hh-60); Start-Sleep 1; LatInit 700 130; LatRun "r18ba|$tag" $x $y "zqxjkvwpyz" $n $o "^a{BACKSPACE}" 20 }
foreach($rep in 1..2){
  SizeRun "big1232x712_r$rep" 1232 712 0 520 30
  SizeRun "small700x420_r$rep" 700 420 0 330 30
}
Shot I_small
[void][W]::MoveWindow($h,24,24,1232,712,$true)
SaveLog I_app
Stop-Transcript | Out-Null
