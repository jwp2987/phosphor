# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
# Rep3: like Rep2 but template with {n} and {p}; file written by command is C:\...\f.txt; uses Get-Content on UTF16-safe
function Rep3($label,$hwnd,$ms,$tmpl,$waitMs=15000,$enterGapMs=0){
  Remove-Item "$global:R\f.txt" -ea 0
  if(-not (Fg $hwnd)){ return "$label ABORT-nofg" }
  $ok=$true
  for($n=1;$n -le 10;$n++){ if(-not [TC]::Line($hwnd,($tmpl -replace '\{n\}',"$n"),$ms)){ $ok=$false; break } }
  Start-Sleep -Milliseconds 4000
  $l4=@(); try{ if(Test-Path "$global:R\f.txt"){ $l4=@(Get-Content "$global:R\f.txt" -ea Stop) } }catch{}
  Start-Sleep -Milliseconds ($waitMs-4000)
  $lines=@(); if(Test-Path "$global:R\f.txt"){ $lines=@(Get-Content "$global:R\f.txt") }
  $lines=@($lines | % { $_.Trim() })
  $exp=1..10 | % { "L$_" }
  $m4=@($exp | ? { $l4 -notcontains $_ }); $m15=@($exp | ? { $lines -notcontains $_ })
  $extra=$lines | ? { $exp -notcontains $_ }
  if(Test-Path "$global:R\f.txt"){ Move-Item "$global:R\f.txt" "$global:R\out\$label.txt" -Force }
  $cls=$(if($m15.Count -gt 0){'LOST'}elseif($m4.Count -gt 0){'LATE'}else{'OK'})
  "$label $cls fg_ok=$ok got4=$($l4.Count) got15=$($lines.Count)/10 missing15=[$($m15 -join ',')] garbled=[$($extra -join ';')] order=$($lines -join ',')"
}
