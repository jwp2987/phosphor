# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
Start-Transcript "$R\out\LD_transcript.txt" -Force | Out-Null
. C:\Users\@WINUSER@\r18bj\lat.ps1
. C:\Users\@WINUSER@\r18bj\tc.ps1
. C:\Users\@WINUSER@\r18bj\tc2.ps1
LatInit 700 40
$L='bj'; $o="$R\out\lat_${L}_LD.csv"; Remove-Item $o -ea 0
SetMode 'orig'
FreshApp2 "info,warp::input_order=debug" 14
$h=Hwnd
[M]::Click(700,660); Start-Sleep 2
Keys "echo alpha_beta gamma{ENTER}"; Start-Sleep 2; Shot "LD_out"
"system DoubleClickTime ms: " + (Add-Type -MemberDefinition '[DllImport("user32.dll")] public static extern uint GetDoubleClickTime();' -Name DC -Namespace X -PassThru)::GetDoubleClickTime()
# ---- tab open/close responsiveness
$pp=Get-Process phosphor-oss | ? { $_.Path -like '*r18bj*' } | select -First 1
$sw=[Diagnostics.Stopwatch]::StartNew()
foreach($i in 1..10){ [K]::Combo([byte[]]@(0x11,0x10),0x54); Start-Sleep -Milliseconds 700 }
Start-Sleep 3; Shot "LD_10tabs"
$pp.Refresh(); "10 tabs opened (0.7 s apart): responding=$($pp.Responding) ws=$([int]($pp.WorkingSet64/1MB))MB"
# close latency: region = tab strip; hold ctrl+shift, tap W; first novel frame
for($i=1;$i -le 10;$i++){
  [M]::Click(700,660); Start-Sleep -Milliseconds 400
  [K]::Down(0x11); [K]::Down(0x10)
  $res=[LT]::Sample(0,26,'W',0,500,200,2500)
  [K]::Up(0x10); [K]::Up(0x11)
  Add-Content $o "$L|tab-close,$i,W,$res"
  Start-Sleep -Milliseconds 1200 }
Shot "LD_after_close"
$pp.Refresh(); "after closes: responding=$($pp.Responding) ws=$([int]($pp.WorkingSet64/1MB))MB"
SaveLog "LD_app"
"LD_DONE"
Stop-Transcript | Out-Null
