# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
param(
 [string]$Dll='inbox',          # 'inbox' or path to conpty.dll
 [string]$Cmd='"C:\Program Files\Git\bin\bash.exe" --noprofile --norc -i',
 [int]$Reps=24, [int]$Ms=25, [int]$WaitMs=4000, [int]$StartWait=3000,
 [string]$Tag='x', [string]$Pre='',   # Pre = text typed (with \r) before reps, e.g. init script
 [int]$Chunk=0                         # 0 = per-key WriteFile; N = coalesce keys of N ms windows (not used)
)
$ErrorActionPreference='Stop'
Add-Type -TypeDefinition @"
using System; using System.Runtime.InteropServices; using System.Text; using System.Threading; using System.IO; using System.Diagnostics; using Microsoft.Win32.SafeHandles;
public class CP {
 [StructLayout(LayoutKind.Sequential)] public struct COORD { public short X, Y; }
 [StructLayout(LayoutKind.Sequential)] public struct STARTUPINFO { public int cb; public string r, d, t; public int x,y,xs,ys,xc,yc,fa,fl; public short sw, cb2; public IntPtr r2, hi, ho, he; }
 [StructLayout(LayoutKind.Sequential)] public struct STARTUPINFOEX { public STARTUPINFO si; public IntPtr attr; }
 [StructLayout(LayoutKind.Sequential)] public struct PI { public IntPtr hp, ht; public int pid, tid; }
 [DllImport("kernel32.dll", SetLastError=true)] public static extern bool CreatePipe(out IntPtr r, out IntPtr w, IntPtr sa, int sz);
 [DllImport("kernel32.dll", SetLastError=true)] public static extern bool WriteFile(IntPtr h, byte[] b, int n, out int w, IntPtr o);
 [DllImport("kernel32.dll", SetLastError=true)] public static extern bool ReadFile(IntPtr h, byte[] b, int n, out int r, IntPtr o);
 [DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr h);
 [DllImport("kernel32.dll", SetLastError=true)] public static extern bool InitializeProcThreadAttributeList(IntPtr l, int c, int f, ref IntPtr sz);
 [DllImport("kernel32.dll", SetLastError=true)] public static extern bool UpdateProcThreadAttribute(IntPtr l, uint f, IntPtr a, IntPtr v, IntPtr sz, IntPtr p, IntPtr r);
 [DllImport("kernel32.dll", SetLastError=true, CharSet=CharSet.Unicode)] public static extern bool CreateProcessW(string app, string cmd, IntPtr pa, IntPtr ta, bool inh, uint fl, IntPtr env, string dir, ref STARTUPINFOEX si, out PI pi);
 [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] public static extern IntPtr LoadLibraryW(string p);
 [DllImport("kernel32.dll")] public static extern IntPtr GetProcAddress(IntPtr m, string n);
 [DllImport("kernel32.dll")] public static extern bool TerminateProcess(IntPtr h, uint c);
 [DllImport("winmm.dll")] public static extern uint timeBeginPeriod(uint p);
 delegate int CreateFn(COORD sz, IntPtr i, IntPtr o, uint f, out IntPtr h);
 delegate void CloseFn(IntPtr h);
 public IntPtr hpc, inW, outR; public PI pi; CloseFn closeFn; public long bytesOut; public MemoryStream cap=new MemoryStream(); public int dsr; public StringBuilder tail=new StringBuilder();
 public void Start(string dll, string cmd){
  IntPtr inR, outW; CreatePipe(out inR, out inW, IntPtr.Zero, 0); CreatePipe(out outR, out outW, IntPtr.Zero, 0);
  IntPtr mod = dll=="inbox" ? LoadLibraryW("kernel32.dll") : LoadLibraryW(dll);
  if(mod==IntPtr.Zero) throw new Exception("loadlib "+dll+" "+Marshal.GetLastWin32Error());
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
 void Drain(){
  var b=new byte[65536]; int n;
  while(ReadFile(outR,b,b.Length,out n,IntPtr.Zero) && n>0){
   bytesOut+=n; lock(cap){ cap.Write(b,0,n); }
   for(int i=0;i+3<n;i++) if(b[i]==0x1b&&b[i+1]==(byte)'['&&b[i+2]==(byte)'6'&&b[i+3]==(byte)'n'){ dsr++; Send("\x1b[1;1R"); }
  }
 }
 public int Send(string s){ var b=Encoding.UTF8.GetBytes(s); int w; if(!WriteFile(inW,b,b.Length,out w,IntPtr.Zero)) throw new Exception("WriteFile "+Marshal.GetLastWin32Error()); if(w!=b.Length) throw new Exception("partial write "+w+"/"+b.Length); return w; }
 // 10 lines back to back; one WriteFile per key
 public void TypeLines(string[] lines,int ms){
  var sw=Stopwatch.StartNew(); long i=0;
  foreach(var l in lines){ var s=l+"\r"; foreach(char c in s){ double due=i*(double)ms; while(sw.Elapsed.TotalMilliseconds<due){} Send(c.ToString()); i++; } }
 }
 public void Close(){ try{ TerminateProcess(pi.hp,0);}catch{} try{ closeFn(hpc);}catch{} }
}
"@
[void][CP]::timeBeginPeriod(1)
$dir="C:\Users\@WINUSER@\bashstall"; $f="$dir\f_$Tag.txt"; $unix="/c/Users/@WINUSER@/bashstall/f_$Tag.txt"
Remove-Item $f -ea 0
$cp=New-Object CP
$cp.Start($Dll,$Cmd)
Start-Sleep -Milliseconds $StartWait
if($Pre){ foreach($l in ($Pre -split "`n")){ [void]$cp.Send($l+"`r"); Start-Sleep -Milliseconds 300 }; Start-Sleep -Milliseconds 1500 }
$lost=0; $res=@()
for($r=1;$r -le $Reps;$r++){
  Remove-Item $f -ea 0
  $lines=1..10 | % { "echo L$_ >> $unix" }
  $cp.TypeLines($lines,$Ms)
  Start-Sleep -Milliseconds $WaitMs
  $got=@(); if(Test-Path $f){ $got=@(Get-Content $f) }
  $miss=(1..10 | ? { $got -notcontains "L$_" }) -join ','
  $ord=($got -join ',')
  $bad = ($miss -ne '') -or ($ord -ne ((1..10|%{"L$_"}) -join ','))
  if($bad){ $lost++ }
  $line = "$(Get-Date -Format HH:mm:ss.fff) rep $r got=$($got.Count)/10 missing=[$miss]" + $(if($bad){" LOST order=$ord"}else{""}); $res += $line; Write-Output $line
}


"RESULT tag=$Tag dll=$Dll ms=$Ms reps=$Reps lost_reps=$lost bytesOut=$($cp.bytesOut) dsr=$($cp.dsr)"
$cp.Close()
