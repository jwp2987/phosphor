# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
param([string]$Exe="C:\Users\@WINUSER@\r18ba\app\phosphor-oss.exe",[string]$Extra="")
$env:RUST_LOG="warn,warp::input_order=debug,warp::terminal::model_lock_trace=debug"
$env:WARP_INPUT_ORDER_LOG_TEXT="1"
Start-Process $Exe -WorkingDirectory $env:USERPROFILE
