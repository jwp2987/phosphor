# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
# usage inside step: SetMode agent|term
function SetMode($m){
  $dst="$env:LOCALAPPDATA\phosphor\Phosphor\config\settings.toml"
  $t=Get-Content "C:\Users\@WINUSER@\r18bj\settings_agent.toml" -Raw
  if($m -eq 'term'){ $t=$t -replace 'default_session_mode = "agent"','default_session_mode = "terminal"' -replace 'ai_auto_detection_enabled = true','ai_auto_detection_enabled = false' -replace 'nld_in_terminal_enabled = true','nld_in_terminal_enabled = false' }
  Set-Content -Encoding UTF8 $dst $t
}
