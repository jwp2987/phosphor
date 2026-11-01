# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
param([string]$Mode = 'rl', [int]$N = 50, [string]$Out, [string]$Exe = 'powershell.exe', [string]$Dir = 'C:\Users\@WINUSER@\winhooks\pm')
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
if ($Mode -eq 'rl') {
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
$line3 = ' $global:F3 = 1; [Console]::Out.Write(("RESULT:" + $global:F1 + $global:F2 + $global:F3 + ":" + $global:WARP_BOOTSTRAPPED + [Environment]::NewLine))'
$line4 = " Write-Output ('PRO' + 'BE')"
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
  $sentAt = @(); $markAt = @()
  [CPty]::Send($lines[0]); [CPty]::Send("`r"); $sentAt += [CPty]::Now() - $t0
  $why = 'ok'
  for ($k = 0; $k -lt 2; $k++) {
    $needle = "$esc]7771;$tok;$k$bel"
    $dl = [CPty]::Now() + 8000
    while ([CPty]::TimeOf($from, $needle) -lt 0 -and [CPty]::Now() -lt $dl) { Start-Sleep -Milliseconds 1 }
    $mt = [CPty]::TimeOf($from, $needle)
    if ($mt -lt 0) { $why = "nomarker$k"; break }
    $markAt += $mt - $t0
    # the next line goes out the instant the marker is seen (no extra wait)
    [CPty]::Send($lines[$k+1]); [CPty]::Send("`r"); $sentAt += [CPty]::Now() - $t0
  }
  $dl = [CPty]::Now() + 8000
  while ([CPty]::TimeOf($from, 'RESULT:') -lt 0 -and [CPty]::Now() -lt $dl) { Start-Sleep -Milliseconds 20 }
  Start-Sleep -Milliseconds 800
  [CPty]::Send($line4); [CPty]::Send("`r")
  $dl = [CPty]::Now() + 4000
  while (-not ([CPty]::Concat($from) -match "PROBE") -and [CPty]::Now() -lt $dl) { Start-Sleep -Milliseconds 20 }
  $all = [CPty]::Concat($from)
  $late = ('{0:N0}' -f ([CPty]::TimeOf($from, "$esc]7771;$tok;0$bel") - $t0))
  $crash = ($all -match 'Oops|something went wrong|ArgumentOutOfRange|SetCursorPosition')
  $result = if ($all -match 'RESULT:(\d+):') { $Matches[1] } else { 'none' }
  $probe = ($all -match "PROBE")
  $ok = (-not $crash) -and $result -eq '111' -and $probe -and $why -eq 'ok'
  $res += [pscustomobject]@{ i = $i; ok = $ok; crash = $crash; result = $result; probe = $probe; why = $why; late = $late; marks = ($markAt | % { '{0:N0}' -f $_ }) -join ','; sent = ($sentAt | % { '{0:N0}' -f $_ }) -join ',' }
  if (-not $ok) { [IO.File]::WriteAllText("$Out.fail$i.raw", $all) }
  [CPty]::Kill()
  Start-Sleep -Milliseconds 400
}
$bad = @($res | ? { -not $_.ok })
$lines = @("mode=$Mode exe=$Exe N=$N ok=$($N - $bad.Count) bad=$($bad.Count) crashes=$(@($res | ? { $_.crash }).Count)")
$lines += ($res | % { "{0} ok={1} crash={2} result={3} probe={4} why={5} late0={8} marks={6} sent={7}" -f $_.i, $_.ok, $_.crash, $_.result, $_.probe, $_.why, $_.marks, $_.sent, $_.late })
$lines | Set-Content $Out
