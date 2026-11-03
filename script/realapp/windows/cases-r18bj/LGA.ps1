# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
Start-Transcript "$R\out\LGA_transcript.txt" -Force | Out-Null
. C:\Users\@WINUSER@\r18bj\tc.ps1
. C:\Users\@WINUSER@\r18bj\tc2.ps1
SetMode "agent"
foreach($mode in 'on','off'){
  if($mode -eq 'off'){ $env:PHOSPHOR_CONPTY_PROMPT_HOLD="0" }
  FreshApp2 "info,warp::input_order=debug" 16
  Remove-Item Env:PHOSPHOR_CONPTY_PROMPT_HOLD -ea 0
  [M]::Click(253,76); Start-Sleep 1; [M]::Click(296,163); Start-Sleep 15; Focus; [M]::Click(700,650); Start-Sleep 3
  Shot "GA_${mode}_start"
  $h=Hwnd
  $n=$(if($mode -eq 'on'){24}else{12})
  foreach($rep in 1..$n){ Rep2 "agentbash_${mode}_25ms_r$rep" $h 25 '/c/Users/@WINUSER@/r18bj/f.txt' 15000 }
  Shot "GA_${mode}_end"
  SaveLog "GA_${mode}_app"
  KillApp
}
"LGA_DONE"
Stop-Transcript | Out-Null
