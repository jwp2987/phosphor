# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices; using System.Text;
public class W {
 [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
 [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
 [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
 [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
 [DllImport("user32.dll")] public static extern void mouse_event(uint f,uint dx,uint dy,uint d,UIntPtr e);
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
 [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h,int x,int y,int w,int hh,bool r);
 [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h,uint m,IntPtr w,IntPtr l);
 [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
 [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
 [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc p, IntPtr l);
 [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
 public delegate bool EnumProc(IntPtr h, IntPtr l);
 [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L,T,R,B; }
}
public class W2 { public static System.Collections.Generic.List<string> L = new System.Collections.Generic.List<string>();
 public static void Run(){ L.Clear(); W.EnumWindows(delegate(IntPtr h, IntPtr l){ if(W.IsWindowVisible(h)){ var sb=new StringBuilder(256); W.GetWindowText(h,sb,256); uint p; W.GetWindowThreadProcessId(h,out p); W.RECT r; W.GetWindowRect(h,out r); if(sb.Length>0) L.Add(string.Format("{0} pid={1} [{2},{3} {4}x{5}] '{6}'",h,p,r.L,r.T,r.R-r.L,r.B-r.T,sb)); } return true; }, IntPtr.Zero); } }
"@
function Wins { [W2]::Run(); [W2]::L }
function PhoWin { $p = Get-Process phosphor-oss -ea 0 | ? { $_.MainWindowHandle -ne 0 } | select -First 1; if($p){ $p.MainWindowHandle } else { [IntPtr]::Zero } }
function Rect { $h=PhoWin; $r=New-Object W+RECT; [void][W]::GetWindowRect($h,[ref]$r); "rect L=$($r.L) T=$($r.T) W=$($r.R-$r.L) H=$($r.B-$r.T) dpi=$([W]::GetDpiForWindow($h)) title='$((Get-Process phosphor-oss|select -first 1).MainWindowTitle)'" }
function Focus { $h=PhoWin; if($h -eq [IntPtr]::Zero){ throw "no window" }; [void][W]::ShowWindow($h,9); [void][W]::SetForegroundWindow($h); Start-Sleep -Milliseconds 300; if(-not (IsPho)){ (New-Object -ComObject WScript.Shell).AppActivate((Get-Process phosphor-oss|select -first 1).Id) | Out-Null; Start-Sleep -Milliseconds 300 } ; if(-not (IsPho)){ throw "phosphor not foreground" } }
function IsPho { $fg=[W]::GetForegroundWindow(); $p=0; [void][W]::GetWindowThreadProcessId($fg,[ref]$p); $tp=$(if($global:TP){$global:TP}else{'phosphor-oss'}); (Get-Process -Id $p -ea 0).ProcessName -eq $tp }
function Keys($s,$delay=0) { if(-not (IsPho)){ throw "not foreground - refusing to type" }; [System.Windows.Forms.SendKeys]::SendWait($s); if($delay){Start-Sleep -Milliseconds $delay} }
function Esc($t) { ($t -replace '([+^%~(){}\[\]])','{$1}') }
function Cmd($t,$wait=1500) { Keys ((Esc $t)+"{ENTER}"); Start-Sleep -Milliseconds $wait }
function Click($x,$y,$right=$false) { [void][W]::SetCursorPos($x,$y); Start-Sleep -Milliseconds 80; if($right){[W]::mouse_event(8,0,0,0,[UIntPtr]::Zero);[W]::mouse_event(16,0,0,0,[UIntPtr]::Zero)}else{[W]::mouse_event(2,0,0,0,[UIntPtr]::Zero);[W]::mouse_event(4,0,0,0,[UIntPtr]::Zero)}; Start-Sleep -Milliseconds 200 }
function Drag($x1,$y1,$x2,$y2) { [void][W]::SetCursorPos($x1,$y1); Start-Sleep -Milliseconds 100; [W]::mouse_event(2,0,0,0,[UIntPtr]::Zero); for($i=1;$i -le 10;$i++){ [void][W]::SetCursorPos([int]($x1+($x2-$x1)*$i/10),[int]($y1+($y2-$y1)*$i/10)); Start-Sleep -Milliseconds 40 }; [W]::mouse_event(4,0,0,0,[UIntPtr]::Zero); Start-Sleep -Milliseconds 200 }
function Shot($name="shot") { $b=[System.Windows.Forms.SystemInformation]::VirtualScreen; $bmp=New-Object System.Drawing.Bitmap $b.Width,$b.Height; $g=[System.Drawing.Graphics]::FromImage($bmp); $g.CopyFromScreen($b.Left,$b.Top,0,0,$bmp.Size); $bmp.Save("C:\Users\@WINUSER@\r18ba\shots\$name.png",[System.Drawing.Imaging.ImageFormat]::Png); $g.Dispose(); $bmp.Dispose() }
function Mem { Get-Process phosphor-oss,OpenConsole -ea 0 | % { "{0} {1} ws={2}MB" -f $_.Name,$_.Id,[math]::Round($_.WorkingSet64/1MB) } }
function Wheel([int]$x,[int]$y,[int]$n) { [void][W]::SetCursorPos($x,$y); Start-Sleep -Milliseconds 100; [W]::mouse_event(0x800,0,0,[BitConverter]::ToUInt32([BitConverter]::GetBytes([int]($n*120)),0),[UIntPtr]::Zero); Start-Sleep -Milliseconds 400 }
function QuitApp { $id=(Get-Process phosphor-oss -ea 0).Id; if($id){ & taskkill /PID $id | Out-Null; for($i=0;$i -lt 15 -and (Get-Process phosphor-oss -ea 0);$i++){ Start-Sleep 1 } }; "quit, alive=" + [bool](Get-Process phosphor-oss -ea 0) }
function Launch { & C:\Users\@WINUSER@\r18ba\start.ps1 | Out-Null; Start-Sleep 14 }
Add-Type @"
using System; using System.Runtime.InteropServices;
public class K { [DllImport("user32.dll")] public static extern void keybd_event(byte v, byte s, uint f, UIntPtr e);
public static void Down(byte v){ keybd_event(v,0,0,UIntPtr.Zero);} public static void Up(byte v){ keybd_event(v,0,2,UIntPtr.Zero);}
public static void Tap(byte v) { Down(v); System.Threading.Thread.Sleep(40); Up(v); }
public static void Combo(byte[] mods, byte v){ foreach(var m in mods) Down(m); System.Threading.Thread.Sleep(40); Tap(v); System.Threading.Thread.Sleep(40); for(int i=mods.Length-1;i>=0;i--) Up(mods[i]); } }
"@
function ShiftEnter { if(-not (IsPho)){ throw "not fg" }; [K]::Combo([byte[]]@(0x10),0x0D); Start-Sleep -Milliseconds 150 }
function CtrlShiftEnter { if(-not (IsPho)){ throw "not fg" }; [K]::Combo([byte[]]@(0x11,0x10),0x0D); Start-Sleep -Milliseconds 150 }
function CtrlEnter { if(-not (IsPho)){ throw "not fg" }; [K]::Combo([byte[]]@(0x11),0x0D); Start-Sleep -Milliseconds 150 }
function KC([byte[]]$mods,[byte]$v) { if(-not (IsPho)){ throw "not fg" }; [K]::Combo($mods,$v); Start-Sleep -Milliseconds 150 }
function Title { (Get-Process phosphor-oss | ? MainWindowHandle -ne 0 | % MainWindowTitle) -join ' | ' }
function Clip { Get-Clipboard -Raw }
