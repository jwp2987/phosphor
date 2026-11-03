# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
param(
 [string]$Script='C:\Users\@WINUSER@\winhooks\pwsh.ps1',   # '' = no bootstrap (control)
 [string]$Dll='inbox', [int]$Reps=30, [int]$KeyMs=10, [string]$Tag='x', [switch]$Instr,
 [string]$Exe='powershell.exe -NoLogo', [switch]$App
)
$ErrorActionPreference='Stop'
Add-Type -TypeDefinition @"
using System; using System.Runtime.InteropServices; using System.Text; using System.Threading; using System.Diagnostics; using System.Collections.Generic;
public class CP2 { public static bool Script_NoKill=false;
 [StructLayout(LayoutKind.Sequential)] public struct COORD { public short X, Y; }
 [StructLayout(LayoutKind.Sequential)] public struct STARTUPINFO { public int cb; public string r, d, t; public int x,y,xs,ys,xc,yc,fa,fl; public short sw, cb2; public IntPtr r2, hi, ho, he; }
 [StructLayout(LayoutKind.Sequential)] public struct STARTUPINFOEX { public STARTUPINFO si; public IntPtr attr; }
 [StructLayout(LayoutKind.Sequential)] public struct PI { public IntPtr hp, ht; public int pid, tid; }
 [DllImport("kernel32.dll", SetLastError=true)] public static extern bool CreatePipe(out IntPtr r, out IntPtr w, IntPtr sa, int sz);
 [DllImport("kernel32.dll", SetLastError=true)] public static extern bool WriteFile(IntPtr h, byte[] b, int n, out int w, IntPtr o);
 [DllImport("kernel32.dll", SetLastError=true)] public static extern bool ReadFile(IntPtr h, byte[] b, int n, out int r, IntPtr o);
 [DllImport("kernel32.dll", SetLastError=true)] public static extern bool InitializeProcThreadAttributeList(IntPtr l, int c, int f, ref IntPtr sz);
 [DllImport("kernel32.dll", SetLastError=true)] public static extern bool UpdateProcThreadAttribute(IntPtr l, uint f, IntPtr a, IntPtr v, IntPtr sz, IntPtr p, IntPtr r);
 [DllImport("kernel32.dll", SetLastError=true, CharSet=CharSet.Unicode)] public static extern bool CreateProcessW(string app, string cmd, IntPtr pa, IntPtr ta, bool inh, uint fl, IntPtr env, string dir, ref STARTUPINFOEX si, out PI pi);
 [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] public static extern IntPtr LoadLibraryW(string p);
 [DllImport("kernel32.dll")] public static extern IntPtr GetProcAddress(IntPtr m, string n);
 [DllImport("kernel32.dll")] public static extern bool TerminateProcess(IntPtr h, uint c);
 [DllImport("kernel32.dll")] static extern void GetSystemTimePreciseAsFileTime(out long t);
 [DllImport("winmm.dll")] public static extern uint timeBeginPeriod(uint p);
 public static double Now(){ long t; GetSystemTimePreciseAsFileTime(out t); return (t - 116444736000000000L)/10000.0; }
 delegate int CreateFn(COORD sz, IntPtr i, IntPtr o, uint f, out IntPtr h);
 delegate void CloseFn(IntPtr h);
 public IntPtr hpc, inW, outR; public PI pi; CloseFn closeFn;
 public List<double> ts=new List<double>(); public List<int> lens=new List<int>(); public object lk=new object(); public string lastText="";
 public void Start(string dll, string cmd){
  IntPtr inR, outW; CreatePipe(out inR, out inW, IntPtr.Zero, 0); CreatePipe(out outR, out outW, IntPtr.Zero, 0);
  IntPtr mod = dll=="inbox" ? LoadLibraryW("kernel32.dll") : LoadLibraryW(dll); if(mod==IntPtr.Zero) throw new Exception("loadlib "+dll+" err "+Marshal.GetLastWin32Error());
  var cf=(CreateFn)Marshal.GetDelegateForFunctionPointer(GetProcAddress(mod,"CreatePseudoConsole"),typeof(CreateFn));
  closeFn=(CloseFn)Marshal.GetDelegateForFunctionPointer(GetProcAddress(mod,"ClosePseudoConsole"),typeof(CloseFn));
  var sz=new COORD{X=120,Y=30}; int hr=cf(sz,inR,outW,0,out hpc); if(hr!=0) throw new Exception("CreatePseudoConsole hr="+hr.ToString("x"));
  var si=new STARTUPINFOEX(); si.si.cb=Marshal.SizeOf(typeof(STARTUPINFOEX)); si.si.fl=0x100;
  IntPtr asz=IntPtr.Zero; InitializeProcThreadAttributeList(IntPtr.Zero,1,0,ref asz); si.attr=Marshal.AllocHGlobal(asz);
  InitializeProcThreadAttributeList(si.attr,1,0,ref asz);
  if(!UpdateProcThreadAttribute(si.attr,0,(IntPtr)0x20016,hpc,(IntPtr)IntPtr.Size,IntPtr.Zero,IntPtr.Zero)) throw new Exception("upta");
  if(!CreateProcessW(null,cmd,IntPtr.Zero,IntPtr.Zero,false,0x80000|0x400,IntPtr.Zero,null,ref si,out pi)) throw new Exception("CreateProcess "+Marshal.GetLastWin32Error());
  var t=new Thread(Drain); t.IsBackground=true; t.Start();
 }
 void Drain(){ var b=new byte[65536]; int n;
  while(ReadFile(outR,b,b.Length,out n,IntPtr.Zero) && n>0){ double t=Now();
   lock(lk){ ts.Add(t); lens.Add(n); }
   for(int i=0;i+3<n;i++) if(b[i]==0x1b&&b[i+1]==(byte)'['&&b[i+2]==(byte)'6'&&b[i+3]==(byte)'n'){ Send("\x1b[1;1R"); } } }
 public void Send(string s){ var b=Encoding.UTF8.GetBytes(s); int w; if(!WriteFile(inW,b,b.Length,out w,IntPtr.Zero)) throw new Exception("WriteFile"); }
 // type text key by key at ms interval, Enter last; returns time Enter was written
 public double TypeApp(string text){ if(!Script_NoKill) Send("\u001b"+"2"); double te=Now(); Send(text+"\r"); return te; }
 public double Type(string text,int ms){ var sw=Stopwatch.StartNew(); int i=0; double te=0;
   foreach(char c in text+"\r"){ double due=i*(double)ms; while(sw.Elapsed.TotalMilliseconds<due){} if(c=='\r') te=Now(); Send(c.ToString()); i++; } return te; }
 // last output arrival within window after t0 that is followed by >=settle ms of quiet; also first
 public string After(double t0,int settle,int maxWait){ double first=-1,last=-1; int bytes=0; var sw=Stopwatch.StartNew();
   while(sw.ElapsedMilliseconds<maxWait){ Thread.Sleep(5); lock(lk){ for(int k=ts.Count-1;k>=0;k--){ if(ts[k]<t0)break; if(last<ts[k]) last=ts[k]; } }
     if(last>0 && Now()-last>settle) break; }
   lock(lk){ for(int k=0;k<ts.Count;k++) if(ts[k]>=t0){ if(first<0) first=ts[k]; bytes+=lens[k]; } }
   return string.Format("{0:F1},{1:F1}",first>0?first-t0:-1,last>0?last-t0:-1); }
 public void Close(){ try{ TerminateProcess(pi.hp,0);}catch{} try{ closeFn(hpc);}catch{} }
}
"@
[void][CP2]::timeBeginPeriod(1); if(-not $Script){ [CP2]::Script_NoKill=$true }
$dir='C:\Users\@WINUSER@\winhooks\out'; New-Item -ItemType Directory -Force $dir | Out-Null
$stamp="$dir\stamps_$Tag.txt"; Remove-Item $stamp -ea 0
$cp=New-Object CP2; $cp.Start($Dll,$Exe)
Start-Sleep -Milliseconds 3500
function Line($t,$wait=600){ [void]$cp.Type($t,1); Start-Sleep -Milliseconds $wait }
if($Script){
  Line '$global:_warpSessionId=[uint64]7' 800
  Line 'Microsoft.PowerShell.Core\Remove-Module -Name PSReadline -ErrorAction Ignore' 800
  Line ". '$Script'" 5000
}
if($Instr){ Line ". 'C:\Users\@WINUSER@\winhooks\instr.ps1'" 1500; Line ('$global:_hkOut=''' + $stamp + '''') 500 }
Line ('function stm { [IO.File]::AppendAllText(''' + $stamp + ''', [string](([DateTime]::UtcNow.Ticks - 621355968000000000)/10000.0) + "`n") }') 800
Line 'echo warm' 800
$res=@()
for($k=0;$k -lt $Reps;$k++){
  $te=$(if($App){$cp.TypeApp('echo hi')}else{$cp.Type('echo hi',$KeyMs)}); $r=$cp.After($te,300,3000); $res+=("echo,$k,$r"); Start-Sleep -Milliseconds 700
}
$cs=@(); $cn=$(if($Instr){'s'}else{'stm'})
for($k=0;$k -lt $Reps;$k++){
  $te=$(if($App){$cp.TypeApp($cn)}else{$cp.Type($cn,$KeyMs)}); $r=$cp.After($te,300,3000); Start-Sleep -Milliseconds 400
  $cs+=$te
}
if($Instr){ Copy-Item $stamp "$dir\raw_$Tag.txt" -Force; $cs | Set-Content "$dir\host_$Tag.txt"; $st=@(Get-Content $stamp | ? { $_ -match '^[0-9.]+ ' } | % {[double]($_ -split ' ')[4]}) } else { $st=@(Get-Content $stamp | ? { $_ -match '^[0-9.]+$' } | % {[double]$_}) }
function Med($a){ $s=@($a|sort); $s[[int]($s.Count/2)] }
$e=@($res | % { [double]($_ -split ',')[3] }); $f=@($res | % { [double]($_ -split ',')[2] })
$d=@(); for($k=0;$k -lt [Math]::Min($cs.Count,$st.Count);$k++){ $d += ($st[$k]-$cs[$k]) }
"TAG $Tag script=[$Script] dll=$Dll"
"echo hi enter->first out ms: med {0:N1} ; enter->last out ms: med {1:N1} p90 {2:N1} min {3:N1} max {4:N1} n={5}" -f (Med $f),(Med $e),(@($e|sort)[[int]($e.Count*0.9)]),(@($e|sort)[0]),(@($e|sort)[-1]),$e.Count
"enter->command start (stm stamp) ms: med {0:N1} p90 {1:N1} min {2:N1} max {3:N1} n={4}" -f (Med $d),(@($d|sort)[[int]($d.Count*0.9)]),(@($d|sort)[0]),(@($d|sort)[-1]),$d.Count
$res | Set-Content "$dir\res_$Tag.txt"; ($d -join ',') | Add-Content "$dir\res_$Tag.txt"
$cp.Close()
