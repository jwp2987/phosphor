# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
Start-Transcript "$R\out\LK_transcript.txt" -Force | Out-Null
FreshApp "info,warp::input_order=debug" 25
[M]::Click(300,660); Start-Sleep 1
$osc = [string][char]27 + "]9999;x" + [char]7
Cmd '$o=[string][char]27+"]9999;x"+[char]7; $a=1..20|%{ (Measure-Command{ Write-Host -NoNewline $o }).TotalMilliseconds }; $b=1..20|%{ (Measure-Command{ [Console]::Out.Write($o) }).TotalMilliseconds }; "WH med=" + ($a|sort)[10] + " max=" + ($a|measure -Max).Maximum + " n>5ms=" + @($a|?{$_ -gt 5}).Count + " | CO med=" + ($b|sort)[10] + " max=" + ($b|measure -Max).Maximum + " n>5ms=" + @($b|?{$_ -gt 5}).Count' 5000
Shot K_micro
Stop-Transcript | Out-Null
