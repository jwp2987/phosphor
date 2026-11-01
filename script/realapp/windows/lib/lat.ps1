# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
Add-Type -TypeDefinition @"
using System; using System.Runtime.InteropServices; using System.Text; using System.Diagnostics; using System.Collections.Generic;
public class LT {
 [DllImport("user32.dll")] static extern IntPtr GetDC(IntPtr h);
 [DllImport("gdi32.dll")] static extern IntPtr CreateCompatibleDC(IntPtr dc);
 [DllImport("gdi32.dll")] static extern IntPtr SelectObject(IntPtr dc, IntPtr o);
 [DllImport("gdi32.dll")] static extern bool BitBlt(IntPtr d,int x,int y,int w,int h,IntPtr s,int sx,int sy,uint rop);
 [DllImport("gdi32.dll")] static extern IntPtr CreateDIBSection(IntPtr dc, ref BMI bmi, uint usage, out IntPtr bits, IntPtr sec, uint off);
 [DllImport("kernel32.dll")] static extern void GetSystemTimePreciseAsFileTime(out long t);
 [DllImport("user32.dll")] static extern void keybd_event(byte v, byte s, uint f, UIntPtr e);
 [DllImport("user32.dll")] static extern short VkKeyScan(char c);
 [DllImport("winmm.dll")] public static extern uint timeBeginPeriod(uint p);
 [StructLayout(LayoutKind.Sequential)] struct BMI { public int sz,w,h; public short pl,bpp; public int comp,img,xp,yp,cu,ci; }
 static IntPtr sdc, mdc, bits; static int W,H; 
 public static double Now(){ long t; GetSystemTimePreciseAsFileTime(out t); return (t - 116444736000000000L)/10000.0; } // epoch ms
 public static void Setup(int w,int h){ W=w;H=h; sdc=GetDC(IntPtr.Zero); mdc=CreateCompatibleDC(sdc); var b=new BMI(); b.sz=40;b.w=w;b.h=-h;b.pl=1;b.bpp=32; IntPtr bm=CreateDIBSection(mdc,ref b,0,out bits,IntPtr.Zero,0); SelectObject(mdc,bm); timeBeginPeriod(1); }
 static long[] lb; 
 static void Grab(int x,int y){ BitBlt(mdc,0,0,W,H,sdc,x,y,0x00CC0020); }
 static long Hash(){ if(lb==null||lb.Length!=W*H/2) lb=new long[W*H/2]; Marshal.Copy(bits,lb,0,lb.Length); ulong h=14695981039346656037UL; for(int i=0;i<lb.Length;i++){ h=(h^(ulong)(lb[i]&0x00FFFFFF00FFFFFFL))*1099511628211UL; } return (long)h; }
 public static double GrabCost(int x,int y,int n){ var sw=Stopwatch.StartNew(); for(int i=0;i<n;i++){ Grab(x,y); Hash(); } return sw.Elapsed.TotalMilliseconds/n; }
 public static void Tap(char c){ short s=VkKeyScan(c); byte vk=(byte)(s&0xff); bool sh=(s&0x100)!=0; if(sh) keybd_event(0x10,0,0,UIntPtr.Zero); keybd_event(vk,0,0,UIntPtr.Zero); keybd_event(vk,0,2,UIntPtr.Zero); if(sh) keybd_event(0x10,0,2,UIntPtr.Zero); }
 public static void TapVk(byte vk){ keybd_event(vk,0,0,UIntPtr.Zero); keybd_event(vk,0,2,UIntPtr.Zero); }
 // returns "tsend,tfirst,tlast,nnovel,grabs,nbase" (epoch ms). baseline = distinct frames seen in baseMs before the key (handles caret blink)
 public static string Sample(int x,int y,char c,int vk,int baseMs,int settleMs,int maxWaitMs){
   var bs=new HashSet<long>(); var sw0=Stopwatch.StartNew(); while(sw0.ElapsedMilliseconds<baseMs){ Grab(x,y); bs.Add(Hash()); }
   var seen=new HashSet<long>(); double ts=Now(); if(vk>0) TapVk((byte)vk); else Tap(c); double tf=-1,tl=-1; int nch=0,g=0;
   var sw=Stopwatch.StartNew();
   while(true){ Grab(x,y); double tn=Now(); long h=Hash(); g++;
     if(!bs.Contains(h) && seen.Add(h)){ if(tf<0)tf=tn; tl=tn; nch++; }
     long el=sw.ElapsedMilliseconds; if(tf<0){ if(el>maxWaitMs)break; } else if(tn-tl>settleMs) break; }
   return string.Format("{0:F2},{1:F2},{2:F2},{3},{4},{5}",ts,tf,tl,nch,g,bs.Count);
 }
 public static string Burst(int x,int y,string text,int intervalMs,bool enter){
   var bs=new HashSet<long>(); var sw0=Stopwatch.StartNew(); while(sw0.ElapsedMilliseconds<700){ Grab(x,y); bs.Add(Hash()); }
   var seen=new HashSet<long>(); int n=text.Length+(enter?1:0); double tfs=0,tls=0,tff=-1,tfl=-1,maxj=0,prevs=0; int nch=0;
   var sw=Stopwatch.StartNew(); double t0=Now(); double extra=0;
   for(int i=0;i<n;i++){ if(i<text.Length && text[i]=='\u00a7'){ extra+=300; }
     double due=i*(double)intervalMs+extra;
     while(true){ double el=sw.Elapsed.TotalMilliseconds; if(el>=due)break; if(due-el>20){ Grab(x,y); double tn=Now(); long h=Hash(); if(!bs.Contains(h)&&seen.Add(h)){ if(tff<0)tff=tn; tfl=tn; nch++; } } }
     double ts=Now(); if(i==0)tfs=ts; tls=ts; if(i>0){ double j=Math.Abs((ts-prevs)-intervalMs); if(j>maxj)maxj=j; } prevs=ts;
     if(i<text.Length){ char ch=text[i]; if(ch=='\u00a7'){} else if(ch=='\n') TapVk(0x0D); else if(ch=='\u2192') TapVk(0x27); else if(ch=='\t') TapVk(0x09); else Tap(ch); } else TapVk(0x0D);
   }
   var sw2=Stopwatch.StartNew(); while(sw2.ElapsedMilliseconds<400){ Grab(x,y); double tn=Now(); long h=Hash(); if(!bs.Contains(h)&&seen.Add(h)){ if(tff<0)tff=tn; tfl=tn; nch++; } }
   return string.Format("{0:F2},{1:F2},{2:F2},{3:F2},{4},{5:F1}",tfs,tls,tff,tfl,nch,maxj);
 }
}
"@
function LatInit($w,$h){ [LT]::Setup($w,$h) }
# run n samples typing chars from $chars (cycled), file out; clears via $clear scriptblock every $clearEvery
function LatRun($label,$x,$y,$chars,$n,$out,$clearKeys="^a{BACKSPACE}",$clearEvery=25,$gap=100,$settle=150,$base=500){
  $i=0
  for($k=0;$k -lt $n;$k++){
    if(-not (IsPho)){ "ABORT not foreground at $k"; break }
    $c=$chars[$k % $chars.Length]
    $r=[LT]::Sample($x,$y,$c,0,$base,$settle,1500)
    Add-Content $out ("$label,$k,$c,$r")
    Start-Sleep -Milliseconds $gap
    if($clearKeys -and (($k+1) % $clearEvery) -eq 0){ [System.Windows.Forms.SendKeys]::SendWait($clearKeys); Start-Sleep -Milliseconds 700 }
  }
}

function BurstCmds($label,$x,$y,$tmpl,$count,$interval,$out,$enterNow=$true,$gapMs=0){
  for($k=1;$k -le $count;$k++){
    if(-not (IsPho)){ "ABORT fg $k"; break }
    $t=$tmpl.Replace('@@',('{0:D2}' -f $k))
    $r=[LT]::Burst($x,$y,$t,$interval,$enterNow)
    Add-Content $out "$label,$k,$r"
    if($gapMs -gt 0){ Start-Sleep -Milliseconds $gapMs }
  }
}

# profiles a/b: $count commands typed at $interval ms/key, Enter pressed separately after a pause; tail = last key -> last paint
function ProfAB($label,$x,$y,$tmpl,$count,$interval,$out,$settle=4000){
  for($k=1;$k -le $count;$k++){
    if(-not (IsPho)){ "ABORT fg $k"; break }
    $t=$tmpl.Replace('@@',('{0:D2}' -f $k))
    $r=[LT]::Burst($x,$y,$t,$interval,$false)
    Add-Content $out "$label,$k,$r"
    Keys "{ENTER}"; Start-Sleep -Milliseconds $settle
  }
}
# profile c: all commands as ONE key stream, Enter between them with no pause
function ProfC($label,$x,$y,$tmpl,$count,$interval,$out){
  $t=(1..$count | % { $tmpl.Replace('@@',('{0:D2}' -f $_)) }) -join "`n"
  $r=[LT]::Burst($x,$y,$t,$interval,$true)
  Add-Content $out "$label,all,$r"
}

# generic stream: one text with \n=Enter, ->(U+2192)=Right, `t=Tab, §=300 ms pause; repeated $reps times, results appended
function Stream($label,$x,$y,$text,$interval,$out,$enterAtEnd=$false){
  $r=[LT]::Burst($x,$y,$text,$interval,$enterAtEnd)
  Add-Content $out "$label,$r"
}
function CmdList($tmpl,$count){ (1..$count | % { $tmpl.Replace('@@',('{0:D2}' -f $_)) }) -join "`n" }
