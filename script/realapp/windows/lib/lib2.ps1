# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
function KillApp { Get-Process phosphor-oss -ea 0 | ? { $_.Path -like '*r18ba*' } | % { & taskkill /PID $_.Id /T /F | Out-Null }; Start-Sleep 2 }
function FreshApp($rustlog="info,warp::input_order=debug",$wait=12) {
  KillApp
  Remove-Item "$env:LOCALAPPDATA\phosphor\Phosphor\data\warp.sqlite*" -ea 0
  $env:RUST_LOG=$rustlog; $env:WARP_INPUT_ORDER_LOG_TEXT="1"
  $global:AppP = Start-Process "C:\Users\@WINUSER@\r18ba\app\phosphor-oss.exe" -WorkingDirectory $env:USERPROFILE -PassThru
  Start-Sleep $wait; Focus
}
function SaveLog($name) { $log="$env:LOCALAPPDATA\phosphor\Phosphor\data\logs\phosphor.log"; try{ $t=[IO.File]::Open($log,'Open','Read','ReadWrite,Delete'); $s=(New-Object IO.StreamReader $t).ReadToEnd(); $t.Close(); Set-Content "$global:R\out\$name.log" $s }catch{ "savelog failed $_" } }
