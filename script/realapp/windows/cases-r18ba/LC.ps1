# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
Start-Transcript "$R\out\LC_transcript.txt" -Force | Out-Null
FreshApp "info,warp::input_order=debug" 20
[M]::Click(300,660); Start-Sleep 1
Cmd 'cd C:\Users\@WINUSER@\r18ba\repo' 4000
Shot C_repo0
$pp = Start-Process powershell -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-WindowStyle','Hidden','-File','C:\Users\@WINUSER@\r18ba\poll.ps1','-Seconds','45' -PassThru
Start-Sleep 3
Focus; [M]::Click(300,660)
$t0=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds(); "enter-series start $t0"
for($i=0;$i -lt 10;$i++){ Keys "{ENTER}"; Start-Sleep -Milliseconds 2500; Add-Content "$R\out\poll_enters.txt" ([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()) }
Shot C_repo1
Start-Sleep 12
Wait-Process -Id $pp.Id -Timeout 30 -ea 0
Get-Content "$R\out\poll.txt"
"--- chips log tail"
Get-Content "$env:LOCALAPPDATA\phosphor\Phosphor\data\logs\phosphor.prompt_chips.log" -Tail 30
SaveLog C_app
Stop-Transcript | Out-Null
