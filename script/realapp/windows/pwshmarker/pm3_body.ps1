# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
param([string]$Mode = 'abort', [int]$N = 50, [string]$Out, [string]$Exe = 'powershell.exe', [string]$Dir = 'C:\Users\@WINUSER@\winhooks\pm')
$ErrorActionPreference = 'Stop'
. (Join-Path $Dir 'cs.ps1')
Add-Type -TypeDefinition $src
[void][CPty]::SetDllDirectory('C:\Users\@WINUSER@\winhooks\cp')
$esc = [char]27; $bel = [char]7; $tok = 424242
$l1 = (Get-Content (Join-Path $Dir 'l1.txt') -Raw).TrimEnd()
$l2 = (Get-Content (Join-Path $Dir 'l2.txt') -Raw).TrimEnd()
$wrap = Get-Content (Join-Path $Dir 'wrap.txt') -Raw
$wrap = $wrap.Trim().Replace('@TOK@', "$tok")
function Mk($k) { "[Console]::Out.Write(`"`$([char]27)]7771;$tok;$k`$([char]7)`")" }
if ($Mode -ne 'eol') {
  $suffix = ' # apikey'
  $b1 = $l1.Substring(0, $l1.Length - $suffix.Length)
  $line1 = "$b1; `$global:F1 = 1; $wrap$suffix"
  $line2 = "$l2; `$global:F2 = 1; `$global:_phMk = 1"
} else {
  $suffix = ' # apikey'
  $b1 = $l1.Substring(0, $l1.Length - $suffix.Length)
  $line1 = "$b1; `$global:F1 = 1; $(Mk 0)$suffix"
  $line2 = "$l2; `$global:F2 = 1; $(Mk 1)"
}
$line3 = ' Start-Sleep 1; $global:F3 = 1; [Console]::Out.Write(("RESULT:" + $global:F1 + $global:F2 + $global:F3 + ":" + $global:WARP_BOOTSTRAPPED + [Environment]::NewLine))'
$line4 = " Write-Output ('PRO' + 'BE')"
$held = " Write-Output ('HEL' + 'D')"
$res = @()
for ($i = 0; $i -lt $N; $i++) {
  [CPty]::Reset()
  $exeline = if ($Exe -like '*pwsh*') { "$Exe -NoLogo -NoProfile -NoExit -Command `"`$global:F1=0;`$global:F2=0;`$global:F3=0`"" } else { "$Exe -NoLogo -NoProfile -ExecutionPolicy Bypass -NoExit -Command `"`$global:F1=0;`$global:F2=0;`$global:F3=0`"" }
  [CPty]::Start($exeline)
  $d = [CPty]::Now() + 15000
  while ([CPty]::TimeOf(0, '> ') -lt 0 -and [CPty]::Now() -lt $d) { Start-Sleep -Milliseconds 20 }
  Start-Sleep -Milliseconds 1200
  $from = [CPty]::Count()
  $t0 = [CPty]::Now()
  $lines = @($line1, $line2, $line3)
  [CPty]::ClearArms()
  [CPty]::Arm("$esc]7771;$tok;0$bel", $lines[1] + "`r")
  [CPty]::Arm("$esc]7771;$tok;1$bel", $lines[2] + "`r")
  [CPty]::Send($lines[0]); [CPty]::Send("`r")
  $why = 'ok'
  $dl = [CPty]::Now() + 8000
  while ([CPty]::SentTimes.Count -lt 2 -and [CPty]::Now() -lt $dl) { Start-Sleep -Milliseconds 5 }
  Start-Sleep -Milliseconds 100   # line 3 is now running (Start-Sleep 30), like a script being sourced
  $mc = [CPty]::Count()
  if ($Mode -eq 'abort' -or $Mode -eq 'sim') { [CPty]::Arm("$esc]7771;$tok;1000000$bel", $held + "`r") }
  $tAbort = [CPty]::Now()
  if ($Mode -ne 'sim') { [CPty]::Send([string][char]3) }
  if ($Mode -eq 'abortnow') { [CPty]::Send($held + "`r") }
  if ($Mode -eq 'abort500') { Start-Sleep -Milliseconds 500; [CPty]::Send($held + "`r") }
  $dl = [CPty]::Now() + 6000
  while (-not ([CPty]::Concat($mc) -match 'HEL' + 'D') -and [CPty]::Now() -lt $dl) { Start-Sleep -Milliseconds 5 }
  $tHeld = [CPty]::TimeOf($mc, 'HEL' + 'D'); $tReady = [CPty]::TimeOf($mc, "$esc]7771;$tok;1000000$bel")
  $all = [CPty]::Concat($from)
  $crash = ($all -match 'Oops|something went wrong|ArgumentOutOfRange|SetCursorPosition')
  $held_ok = ($tHeld -gt 0)
  $ok = (-not $crash) -and $held_ok
  $res += [pscustomobject]@{ i = $i; ok = $ok; crash = $crash; held = $held_ok; ready = ('{0:N0}' -f ($tReady - $tAbort)); heldout = ('{0:N0}' -f ($tHeld - $tAbort)) }
  if (-not $ok) { [IO.File]::WriteAllText("$Out.fail$i.raw", $all) }
  [CPty]::Kill()
  Start-Sleep -Milliseconds 400
}
$bad = @($res | ? { -not $_.ok })
$lines = @("mode=$Mode N=$N ok=$($N - $bad.Count) bad=$($bad.Count) crashes=$(@($res | ? { $_.crash }).Count)")
$lines += ($res | % { "{0} ok={1} crash={2} held={3} ready_after_ctrlc_ms={4} held_out_ms={5}" -f $_.i, $_.ok, $_.crash, $_.held, $_.ready, $_.heldout })
$lines | Set-Content $Out
