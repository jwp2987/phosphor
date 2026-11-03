# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
param([string]$Boot, [int]$N = 30, [string]$Out, [string]$Cmd = 'echo hi')
$src = @'
using System; using System.Collections.Generic; using System.Diagnostics; using System.IO; using System.Runtime.InteropServices; using System.Text; using System.Threading; using Microsoft.Win32.SafeHandles;
public class CPty {
  [StructLayout(LayoutKind.Sequential)] struct COORD { public short X, Y; }
  [StructLayout(LayoutKind.Sequential)] struct STARTUPINFO { public int cb; public IntPtr lpReserved, lpDesktop, lpTitle; public int dwX, dwY, dwXSize, dwYSize, dwXCountChars, dwYCountChars, dwFillAttribute, dwFlags; public short wShowWindow, cbReserved2; public IntPtr lpReserved2, hStdInput, hStdOutput, hStdError; }
  [StructLayout(LayoutKind.Sequential)] struct STARTUPINFOEX { public STARTUPINFO si; public IntPtr lpAttributeList; }
  [StructLayout(LayoutKind.Sequential)] struct PROCESS_INFORMATION { public IntPtr hProcess, hThread; public int dwProcessId, dwThreadId; }
  [DllImport("kernel32.dll")] static extern bool CreatePipe(out IntPtr r, out IntPtr w, IntPtr a, int sz);
  [DllImport("kernel32.dll", CharSet=CharSet.Unicode)] public static extern bool SetDllDirectory(string d);
  [DllImport("conpty.dll")] static extern int CreatePseudoConsole(COORD size, IntPtr hIn, IntPtr hOut, uint flags, out IntPtr hPC);
  [DllImport("conpty.dll")] static extern void ClosePseudoConsole(IntPtr hPC);
  [DllImport("kernel32.dll")] static extern bool InitializeProcThreadAttributeList(IntPtr l, int n, int f, ref IntPtr sz);
  [DllImport("kernel32.dll")] static extern bool UpdateProcThreadAttribute(IntPtr l, uint f, IntPtr attr, IntPtr v, IntPtr sz, IntPtr p, IntPtr r);
  [DllImport("kernel32.dll", SetLastError=true, CharSet=CharSet.Unicode)] static extern bool CreateProcess(string app, string cmd, IntPtr pa, IntPtr ta, bool inh, uint flags, IntPtr env, string cwd, ref STARTUPINFOEX si, out PROCESS_INFORMATION pi);
  [DllImport("kernel32.dll")] static extern bool TerminateProcess(IntPtr h, uint c);
  public static List<KeyValuePair<double,string>> Log = new List<KeyValuePair<double,string>>();
  static Stopwatch sw = Stopwatch.StartNew();
  static FileStream inS, outS; static IntPtr hPC; static PROCESS_INFORMATION pi;
  public static double Now() { return sw.Elapsed.TotalMilliseconds; }
  public static void Start(string cmdline) {
    IntPtr inR, inW, outR, outW;
    CreatePipe(out inR, out inW, IntPtr.Zero, 0); CreatePipe(out outR, out outW, IntPtr.Zero, 0);
    COORD sz; sz.X = 120; sz.Y = 30;
    int hr = CreatePseudoConsole(sz, inR, outW, 0, out hPC); if (hr != 0) throw new Exception("CreatePseudoConsole " + hr);
    IntPtr n = IntPtr.Zero; InitializeProcThreadAttributeList(IntPtr.Zero, 1, 0, ref n);
    STARTUPINFOEX si = new STARTUPINFOEX(); si.si.cb = Marshal.SizeOf(typeof(STARTUPINFOEX)); si.si.dwFlags = 0x100; si.lpAttributeList = Marshal.AllocHGlobal(n);
    InitializeProcThreadAttributeList(si.lpAttributeList, 1, 0, ref n);
    UpdateProcThreadAttribute(si.lpAttributeList, 0, (IntPtr)0x20016, hPC, (IntPtr)IntPtr.Size, IntPtr.Zero, IntPtr.Zero);
    if (!CreateProcess(null, cmdline, IntPtr.Zero, IntPtr.Zero, false, 0x80400, IntPtr.Zero, null, ref si, out pi)) throw new Exception("CreateProcess " + Marshal.GetLastWin32Error());
    inS = new FileStream(new SafeFileHandle(inW, true), FileAccess.Write, 1, false);
    outS = new FileStream(new SafeFileHandle(outR, true), FileAccess.Read, 1, false);
    Thread t = new Thread(delegate() { byte[] b = new byte[65536]; while (true) { int k; try { k = outS.Read(b, 0, b.Length); } catch { return; } if (k <= 0) return; double ts = sw.Elapsed.TotalMilliseconds; string s = Encoding.UTF8.GetString(b, 0, k); lock (Log) { Log.Add(new KeyValuePair<double,string>(ts, s)); } if (s.Contains("\u001b[6n")) { Send("\u001b[1;1R"); } } });
    t.IsBackground = true; t.Start();
  }
  public static void Send(string s) { byte[] b = Encoding.UTF8.GetBytes(s); inS.Write(b, 0, b.Length); inS.Flush(); }
  public static int Count() { lock (Log) { return Log.Count; } }
  public static string Concat(int from) { lock (Log) { var sb = new StringBuilder(); for (int i = from; i < Log.Count; i++) sb.Append(Log[i].Value); return sb.ToString(); } }
  // time of first chunk at/after index 'from' whose cumulative text since 'from' contains needle; -1 if none
  public static double TimeOf(int from, string needle) { lock (Log) { var sb = new StringBuilder(); for (int i = from; i < Log.Count; i++) { sb.Append(Log[i].Value); if (sb.ToString().Contains(needle)) return Log[i].Key; } } return -1; }
  public static double TimeFirst(int from) { lock (Log) { if (from < Log.Count) return Log[from].Key; } return -1; }
  public static void Kill() { TerminateProcess(pi.hProcess, 0); }
}
'@
Add-Type -TypeDefinition $src
[void][CPty]::SetDllDirectory('C:\Users\@WINUSER@\winhooks\cp')
$esc=[char]27; $bel=[char]7
function HexOf($s){ [BitConverter]::ToString([Text.Encoding]::UTF8.GetBytes($s)).Replace('-','') }
$hPre = HexOf '"hook":"Preexec"'; $hCF = HexOf '"hook":"CommandFinished"'; $hPc = HexOf '"hook":"Precmd"'
$pb = "$esc]133;B$bel"
$boot = $Boot
$init = if ($Boot -like "*triv*") { ". $boot" } else { "`$global:_warpSessionId=1; . $boot" }
$cmdline = "powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -NoExit -Command `"$init`""
[CPty]::Start($cmdline)
Start-Sleep 3
[CPty]::Send("`r"); Start-Sleep 3
[IO.File]::WriteAllText($Out + '.dbg', ((Get-Process | ? { $_.Name -match 'OpenConsole|conhost|powershell' } | % { $_.Name + ' ' + $_.Id + ' ' + $_.StartTime }) -join ';') + ("count=" + [CPty]::Count() + " now=" + [CPty]::Now() + " " + [CPty]::Concat(0)))
# wait for first prompt end marker
$deadline = [CPty]::Now() + 20000
while ([CPty]::TimeOf(0, $pb) -lt 0 -and [CPty]::Now() -lt $deadline) { Start-Sleep -Milliseconds 50 }
Start-Sleep -Milliseconds 1500
$rows = @()
for ($i = 0; $i -lt $N + 3; $i++) {
  Start-Sleep -Milliseconds 700
  $from = [CPty]::Count(); $t0 = [CPty]::Now()
  [CPty]::Send($Cmd + "`r")
  $d = [CPty]::Now() + 5000
  while ([CPty]::TimeOf($from, $pb) -lt 0 -and [CPty]::Now() -lt $d) { Start-Sleep -Milliseconds 2 }
  Start-Sleep -Milliseconds 120
  $r = [ordered]@{ first = [CPty]::TimeFirst($from) - $t0; preexec = [CPty]::TimeOf($from,$hPre) - $t0; cmdfin = [CPty]::TimeOf($from,$hCF) - $t0; precmd = [CPty]::TimeOf($from,$hPc) - $t0; out = [CPty]::TimeOf($from,"`r`nhi`r`n") - $t0; promptB = [CPty]::TimeOf($from,$pb) - $t0 }
  if ($i -ge 3) { $rows += [pscustomobject]$r }
}
function Med($a){ $s = $a | Sort-Object; $s[[int]($s.Count/2)] }
$lines = @("N=$N cmd='$Cmd' boot=$boot")
foreach ($k in 'first','preexec','cmdfin','precmd','out','promptB') { $v = $rows | % { $_.$k }; $lines += ("{0,-8} med {1,7:N1}  min {2,7:N1}  max {3,7:N1}" -f $k, (Med $v), ($v|measure -Minimum).Minimum, ($v|measure -Maximum).Maximum) }
$lines | Set-Content $Out
[IO.File]::WriteAllText($Out + '.raw', [CPty]::Concat(0))
[CPty]::Kill()
