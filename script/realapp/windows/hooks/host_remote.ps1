# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
  if(!CreateProcessW(null,cmd,IntPtr.Zero,IntPtr.Zero,false,0x80000|0x400,IntPtr.Zero,null,ref si,out pi)) throw new Exception("CreateProcess "+Marshal.GetLastWin32Error());
  var t=new Thread(Drain); t.IsBackground=true; t.Start();
 }
 void Drain(){
  var b=new byte[65536]; int n;
  while(ReadFile(outR,b,b.Length,out n,IntPtr.Zero) && n>0){
   bytesOut+=n;
   for(int i=0;i+3<n;i++) if(b[i]==0x1b&&b[i+1]==(byte)'['&&b[i+2]==(byte)'6'&&b[i+3]==(byte)'n'){ dsr++; Send("\x1b[1;1R"); }
  }
 }
 public int Send(string s){ var b=Encoding.UTF8.GetBytes(s); int w; if(!WriteFile(inW,b,b.Length,out w,IntPtr.Zero)) throw new Exception("WriteFile "+Marshal.GetLastWin32Error()); if(w!=b.Length) throw new Exception("partial write "+w+"/"+b.Length); return w; 
 // 10 lines back to back; one WriteFile per key
 public void TypeLines(string[] lines,int ms){
  var sw=Stopwatch.StartNew(); long i=0;
  foreach(var l in lines){ var s=l+"\r"; foreach(char c in s){ double due=i*(double)ms; while(sw.Elapsed.TotalMilliseconds<due){} Send(c.ToString()); i++; } }
 }
 public void Close(){ try{ TerminateProcess(pi.hp,0);}catch{} try{ closeFn(hpc);}catch{} }
}
"@
[void][CP]::timeBeginPeriod(1)
$dir="C:\Users\@WINUSER@\winconpty"; $f="$dir\f_$Tag.txt"; $unix="/c/Users/@WINUSER@/winconpty/f_$Tag.txt"
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
  $res += "rep $r got=$($got.Count)/10 missing=[$miss]" + $(if($bad){" LOST order=$ord"}else{""})
}
$res
"RESULT tag=$Tag dll=$Dll ms=$Ms reps=$Reps lost_reps=$lost bytesOut=$($cp.bytesOut) dsr=$($cp.dsr)"
$cp.Close()
