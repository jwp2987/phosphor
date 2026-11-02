# windows/ci/app_test.ps1: CI real-app test on the Windows VM (job app-test-windows). Windows PowerShell 5.1, standard user.
# Usage: powershell -NoProfile -ExecutionPolicy Bypass -File app_test.ps1 -Dist dist -Out appci-out\windows [-Modes terminal,agent]
#
# The GitLab Windows runner runs ON THE CONSOLE (interactive desktop session), so this script launches Phosphor and
# injects keys / takes screenshots directly: no scheduled-task / indesk.ps1 indirection. VM rules (docs/testing.md 3.4)
# are enforced here:
#   * ONE GUI driver at a time: C:\Users\<user>\WINPUSH.lock (younger than 2 h => wait up to 30 min, else fail), removed at the end;
#   * side-by-side install into -Out\app (the installed v0.1.15 stays and must report its version again afterwards);
#   * before installing: back up the app data dirs (md5 list), the uninstall key, user PATH, Start-menu/Desktop shortcuts and the
#     phosphor:// handler; the finally block restores all of them and fails the job if the md5 lists differ;
#   * only the PID this script started is ever stopped; the GitLab runner process is never touched;
#   * keys are only sent while Phosphor is the foreground window (IsPho), otherwise the case fails instead of typing elsewhere.
# Results: Out\run\results.jsonl, results.json, junit.xml, summary.md, shots\*.png. Cases are the PowerShell-shell subset of the
# docs/testing.md matrix (the Linux/Mac harness is Python + bash; no python is assumed on the VM). Agent mode runs the same cases
# with default_session_mode=agent (provider cases skip: no credentials in CI).
param([Parameter(Mandatory)][string]$Dist, [Parameter(Mandatory)][string]$Out, [string]$Modes = "terminal,agent")
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public class AW { [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
 [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
 [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c); }
"@
$Out = [IO.Path]::GetFullPath((Join-Path (Get-Location) $Out)); $Dist = [IO.Path]::GetFullPath((Join-Path (Get-Location) $Dist))
$Run = Join-Path $Out "run"; $Shots = Join-Path $Run "shots"; $Work = Join-Path $Out "w"; $Bak = Join-Path $Out "bak"; $App = Join-Path $Out "app"
New-Item -ItemType Directory -Force $Run, $Shots, $Work, $Bak | Out-Null
$ResF = Join-Path $Run "results.jsonl"; "" | Set-Content $ResF -Encoding ascii
$Lock = Join-Path $env:USERPROFILE "WINPUSH.lock"
$DataDirs = @("$env:LOCALAPPDATA\phosphor", "$env:APPDATA\phosphor")
$UninstKey = "HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\zap-oss_is1"
$HandlerKey = "HKCU\Software\Classes\phosphor"
$InstalledExe = "$env:LOCALAPPDATA\Programs\Phosphor\phosphor-oss.exe"
$script:Fail = 0
$script:AppProc = $null
$rng = New-Object Random 1

function Emit($name, $mode, $status, $detail, $secs = 0, $metrics = @{}, $shots = @()) {
  $o = [ordered]@{ name = $name; mode = $mode; status = $status; seconds = [math]::Round($secs, 1); detail = @($detail); metrics = $metrics; shots = @($shots) }
  ($o | ConvertTo-Json -Compress -Depth 5) | Add-Content $ResF -Encoding ascii
  "[{0}] {1,-24} {2} {3}" -f $mode, $name, $status.ToUpper(), (@($detail) -join "; ")
  if ($status -eq "fail") { $script:Fail++ }
}
function Md5List($dirs) { Get-ChildItem $dirs -Recurse -File -ErrorAction SilentlyContinue | ForEach-Object { "{0} {1}" -f (Get-FileHash $_.FullName -Algorithm MD5).Hash, $_.FullName } | Sort-Object }
function Shot($name) {
  $b = [Windows.Forms.SystemInformation]::VirtualScreen; $bmp = New-Object Drawing.Bitmap $b.Width, $b.Height
  $g = [Drawing.Graphics]::FromImage($bmp); $g.CopyFromScreen($b.Left, $b.Top, 0, 0, $bmp.Size)
  $p = Join-Path $Shots "$name.png"; $bmp.Save($p, [Drawing.Imaging.ImageFormat]::Png); $g.Dispose(); $bmp.Dispose(); "shots\$name.png"
}
function IsPho { $fg = [AW]::GetForegroundWindow(); $p = 0; [void][AW]::GetWindowThreadProcessId($fg, [ref]$p); ($script:AppProc -and $p -eq $script:AppProc.Id) }
function Focus {
  $script:AppProc.Refresh(); $h = $script:AppProc.MainWindowHandle
  if ($h -eq [IntPtr]::Zero) { throw "no Phosphor window" }
  [void][AW]::ShowWindow($h, 9); [void][AW]::SetForegroundWindow($h); Start-Sleep -Milliseconds 300
  if (-not (IsPho)) { (New-Object -ComObject WScript.Shell).AppActivate($script:AppProc.Id) | Out-Null; Start-Sleep -Milliseconds 300 }
  if (-not (IsPho)) { throw "Phosphor is not the foreground window" }
}
function SKEsc([string]$c) { if ("+^%~(){}[]".Contains($c)) { "{$c}" } else { $c } }
function Keys([string]$s) { if (-not (IsPho)) { throw "not foreground: refusing to type" }; [Windows.Forms.SendKeys]::SendWait($s) }
function TypeText([string]$t, [int]$lo = 10, [int]$hi = 25, [switch]$Enter) {
  foreach ($ch in $t.ToCharArray()) { Keys (SKEsc ([string]$ch)); Start-Sleep -Milliseconds $rng.Next($lo, $hi + 1) }
  if ($Enter) { Keys "{ENTER}" }
}
function Wf($path, $n, $timeout = 15) {
  $t = Get-Date
  while (((Get-Date) - $t).TotalSeconds -lt $timeout) { if ((Test-Path $path) -and @(Get-Content $path -ErrorAction SilentlyContinue).Count -ge $n) { break }; Start-Sleep -Milliseconds 100 }
  if (Test-Path $path) { @(Get-Content $path -ErrorAction SilentlyContinue) } else { @() }
}
function Tok { "T" + $rng.Next(100000, 999999) + "x" + $rng.Next(1000, 9999) }
function WriteSettings($mode) {
  $d = "$env:LOCALAPPDATA\phosphor\Phosphor\config"; New-Item -ItemType Directory -Force $d | Out-Null
  $m = if ($mode -eq "agent") { "agent" } else { "terminal" }
  $t = "[general]`r`ndefault_session_mode = `"$m`"`r`n`r`n[warpify.ssh]`r`nssh_extension_install_mode = `"never_install`"`r`n"
  if ($mode -eq "agent") { $t += "`r`n[agents.warp_agent.input]`r`nai_auto_detection_enabled = true`r`nnld_in_terminal_enabled = true`r`n" }
  if ($env:APPCI_OLLAMA_URL) { $t += "`r`n[[agents.warp_agent.providers]]`r`nid = `"appci-ollama`"`r`nname = `"CI Ollama`"`r`napi_type = `"ollama`"`r`nbase_url = `"$($env:APPCI_OLLAMA_URL)`"`r`nmodels = [{ name = `"gpt-oss:20b`", id = `"gpt-oss:20b`" }]`r`n" }
  Set-Content "$d\settings.toml" $t -Encoding ascii
}
function StartApp {
  $env:RUST_LOG = "warn,warp::input_order=debug"; $env:WARP_INPUT_ORDER_LOG_TEXT = "1"
  $script:AppProc = Start-Process "$App\phosphor-oss.exe" -WorkingDirectory $Work -PassThru
  $t = Get-Date; while (((Get-Date) - $t).TotalSeconds -lt 60) { $script:AppProc.Refresh(); if ($script:AppProc.MainWindowHandle -ne [IntPtr]::Zero) { break }; Start-Sleep -Milliseconds 500 }
  Start-Sleep 12; Focus; Keys "{ESC}"; Start-Sleep -Milliseconds 800
}
function StopApp { if ($script:AppProc -and -not $script:AppProc.HasExited) { & taskkill /PID $script:AppProc.Id /T /F | Out-Null; Start-Sleep 2 }; $script:AppProc = $null }

# ---- cases (each returns @(status, detail[], metrics)) ----
function C-Fast($mode) {
  $f = "$Work\r_ft.txt"; Remove-Item $f -ErrorAction SilentlyContinue; $exp = @()
  for ($i = 0; $i -lt 30; $i++) { $tok = Tok; $exp += $tok; TypeText "Add-Content r_ft.txt $tok" -Enter }
  $got = Wf $f 30 40
  if (($got -join "|") -eq ($exp -join "|")) { @("pass", @(), @{}) } else { @("fail", @("fast typing: $($got.Count)/30 lines, exact match false"), @{}) }
}
function C-PasteEnter($mode) {
  $f = "$Work\pe.txt"; Remove-Item $f -ErrorAction SilentlyContinue; $exp = @()
  for ($i = 0; $i -lt 6; $i++) { $tok = Tok; $pad = "w" * (10, 500, 5000)[$i % 3]; $exp += ($tok + $pad)
    Set-Clipboard -Value "Add-Content pe.txt $tok$pad"; Keys "^+v"; Keys "{ENTER}"; Start-Sleep -Milliseconds 900 }
  $got = Wf $f 6 20
  if (($got -join "|") -eq ($exp -join "|")) { @("pass", @(), @{}) } else { @("fail", @("paste + instant Enter: $($got.Count)/6 exact lines"), @{}) }
}
function C-CtrlC($mode) {
  $f = "$Work\cc.txt"; Remove-Item $f -ErrorAction SilentlyContinue; $lat = @()
  for ($i = 0; $i -lt 5; $i++) {
    TypeText "try { Start-Sleep 30 } finally { [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() | Add-Content cc.txt }" -Enter
    Start-Sleep -Milliseconds 1500; $t0 = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds(); Keys "^c"
    $g = Wf $f ($i + 1) 6; if ($g.Count -gt $i) { $lat += ([int64]$g[$i] - $t0) }; Start-Sleep -Milliseconds 500 }
  $med = if ($lat.Count) { ($lat | Sort-Object)[[int]($lat.Count / 2)] } else { $null }
  if ($lat.Count -eq 5) { @("pass", @(), @{ ctrl_c_ms_median = $med; note = "includes SendKeys cost" }) } else { @("fail", @("Ctrl-C delivered $($lat.Count)/5 times"), @{}) }
}
function C-EnterLatency($mode) {
  $ds = @()
  for ($i = 0; $i -lt 25; $i++) {
    $f = "$Work\lat_$i.txt"; Remove-Item $f -ErrorAction SilentlyContinue
    TypeText "[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() | Out-File lat_$i.txt -Encoding ascii" 10 25; Start-Sleep -Milliseconds 300
    $t0 = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds(); Keys "{ENTER}"
    $g = Wf $f 1 4; if ($g.Count) { $ds += ([int64]$g[0] - $t0) }; Start-Sleep -Milliseconds 200 }
  $s = $ds | Sort-Object; $m = if ($s) { $s[[int]($s.Count / 2)] } else { $null }
  $res = @{ enter_to_start_ms_median = $m; n = $ds.Count; note = "PowerShell cmdlet start + SendKeys; compare with the v0.1.16 baseline (368 ms Enter to last paint)" }
  if ($ds.Count -ge 20) { @("pass", @(), $res) } else { @("fail", @("only $($ds.Count)/25 Enter timings"), $res) }
}
function C-Idle($mode) {
  Start-Sleep 3; $p = $script:AppProc; $p.Refresh(); $c0 = $p.TotalProcessorTime.TotalSeconds; $w0 = Get-Date; Start-Sleep 20
  $p.Refresh(); $pct = ($p.TotalProcessorTime.TotalSeconds - $c0) / ((Get-Date) - $w0).TotalSeconds * 100
  $lim = if ($env:APPCI_IDLE_CPU_MAX) { [double]$env:APPCI_IDLE_CPU_MAX } else { 15 }
  $m = @{ idle_cpu_pct_of_one_core = [math]::Round($pct, 1); rss_mb = [math]::Round($p.WorkingSet64 / 1MB) }
  if ($pct -le $lim) { @("pass", @(), $m) } else { @("fail", @("idle CPU $([math]::Round($pct,1))% > $lim%"), $m) }
}
function C-Tabs($mode) {
  for ($i = 0; $i -lt 12; $i++) { Keys "^+t"; Start-Sleep -Milliseconds 700; TypeText "while (`$true) { 'streaming_$i'; Start-Sleep -Milliseconds 20 }" -Enter }
  $f = "$Work\tw.txt"; Remove-Item $f -ErrorAction SilentlyContinue; Keys "^+t"; Start-Sleep 1; $exp = @()
  for ($i = 0; $i -lt 10; $i++) { $tok = Tok; $exp += $tok; TypeText "Add-Content tw.txt $tok" -Enter }
  $got = Wf $f 10 40
  if ($script:AppProc.HasExited) { return @("fail", @("app died with 12 streaming tabs"), @{}) }
  if (($got -join "|") -eq ($exp -join "|")) { @("pass", @(), @{}) } else { @("fail", @("typing under 12 streaming tabs: $($got.Count)/10 exact"), @{}) }
}
$Cases = @(
  @{ n = "fast_typing"; f = ${function:C-Fast} }, @{ n = "paste_instant_enter"; f = ${function:C-PasteEnter} }, @{ n = "ctrl_c"; f = ${function:C-CtrlC} },
  @{ n = "latency_enter"; f = ${function:C-EnterLatency} }, @{ n = "idle_cpu"; f = ${function:C-Idle} }, @{ n = "tabs_streaming"; f = ${function:C-Tabs} })
# APPCI_TIER=smoke: the input-path subset (fix tier); empty/full: every case.
if ($env:APPCI_TIER -eq "smoke") { $Cases = @($Cases | Where-Object { @("fast_typing", "paste_instant_enter", "ctrl_c", "latency_enter") -contains $_.n }) }

# ---- lock ----
$t = Get-Date
while ((Test-Path $Lock) -and ((Get-Date) - (Get-Item $Lock).LastWriteTime).TotalHours -lt 2) {
  if (((Get-Date) - $t).TotalMinutes -gt 30) { Emit "preflight" "-" "fail" "WINPUSH.lock held by: $(Get-Content $Lock -Raw)"; exit 3 }
  Start-Sleep 30 }
"app-test-windows pipeline=$env:CI_PIPELINE_ID job=$env:CI_JOB_ID $(Get-Date -Format s)" | Set-Content $Lock

$restored = $false
try {
  # ---- backup ----
  $pre = Md5List $DataDirs; $pre | Set-Content "$Bak\md5_pre.txt"
  foreach ($d in $DataDirs) { if (Test-Path $d) { Copy-Item $d "$Bak\$(Split-Path $d -Parent | Split-Path -Leaf)_$(Split-Path $d -Leaf)" -Recurse -Force } }
  $hadUninst = $false; & reg export $UninstKey "$Bak\uninstall.reg" /y 2>$null | Out-Null; if ($LASTEXITCODE -eq 0) { $hadUninst = $true }
  $hadHandler = $false; & reg export $HandlerKey "$Bak\handler.reg" /y 2>$null | Out-Null; if ($LASTEXITCODE -eq 0) { $hadHandler = $true }
  $pathPre = [Environment]::GetEnvironmentVariable("Path", "User")
  $lnks = @("$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Phosphor.lnk", "$env:USERPROFILE\Desktop\Phosphor.lnk")
  $i = 0; foreach ($l in $lnks) { if (Test-Path $l) { Copy-Item $l "$Bak\lnk$i.lnk" -Force }; $i++ }
  $verPre = if (Test-Path $InstalledExe) { (Get-Item $InstalledExe).VersionInfo.ProductVersion } else { "none" }
  "installed before: $verPre"
  Get-Process phosphor-oss -ErrorAction SilentlyContinue | ForEach-Object { "note: Phosphor already running (pid $($_.Id), not ours; left alone)" }

  # ---- side-by-side install of the artifact ----
  $setup = Get-ChildItem $Dist -Filter *.exe | Select-Object -First 1
  if (-not $setup) { Emit "preflight" "-" "fail" "no installer .exe in $Dist (needs build-windows-x86_64 artifacts)"; throw "no installer" }
  $p = Start-Process $setup.FullName -ArgumentList "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/CURRENTUSER", "/DIR=$App", "/LOG=$Out\install.log" -Wait -PassThru
  if ($p.ExitCode -ne 0 -or -not (Test-Path "$App\phosphor-oss.exe")) { Emit "preflight" "-" "fail" "installer exit $($p.ExitCode), no $App\phosphor-oss.exe"; throw "install failed" }
  "installed test build: $((Get-Item "$App\phosphor-oss.exe").VersionInfo.ProductVersion) in $App"

  foreach ($mode in $Modes.Split(",")) {
    Remove-Item "$env:LOCALAPPDATA\phosphor" -Recurse -Force -ErrorAction SilentlyContinue; Remove-Item "$env:APPDATA\phosphor" -Recurse -Force -ErrorAction SilentlyContinue
    WriteSettings $mode
    try { StartApp } catch { Emit "launch" $mode "fail" $_.Exception.Message 0 @{} @(Shot "launch_fail_$mode"); StopApp; continue }
    $s0 = Shot "${mode}_proof"; Emit "mode_proof" $mode "pass" "screenshot only (no OCR on the VM): $s0" 0 @{} @($s0)
    foreach ($c in $Cases) {
      $t0 = Get-Date
      try { Focus; Keys "^c"; Start-Sleep -Milliseconds 300; Keys "^u"; $r = & $c.f $mode } catch { $r = @("fail", @("harness exception: $($_.Exception.Message)"), @{}) }
      $sh = Shot "${mode}_$($c.n)"
      if ($script:AppProc.HasExited) { $r[0] = "fail"; $r[1] = @($r[1]) + "APP DIED"; Emit $c.n $mode $r[0] $r[1] ((Get-Date) - $t0).TotalSeconds $r[2] @($sh); StartApp; continue }
      Emit $c.n $mode $r[0] $r[1] ((Get-Date) - $t0).TotalSeconds $r[2] @($sh)
    }
    Copy-Item "$env:LOCALAPPDATA\phosphor" "$Run\data_$mode" -Recurse -Force -ErrorAction SilentlyContinue
    StopApp
  }
}
catch { Emit "harness" "-" "fail" $_.Exception.Message }
finally {
  StopApp
  # ---- restore everything, then verify ----
  Remove-Item "$env:LOCALAPPDATA\phosphor" -Recurse -Force -ErrorAction SilentlyContinue; Remove-Item "$env:APPDATA\phosphor" -Recurse -Force -ErrorAction SilentlyContinue
  foreach ($d in $DataDirs) { $b = "$Bak\$(Split-Path $d -Parent | Split-Path -Leaf)_$(Split-Path $d -Leaf)"; if (Test-Path $b) { Copy-Item $b $d -Recurse -Force } }
  if ($hadUninst) { & reg import "$Bak\uninstall.reg" 2>&1 | Out-Null } else { & reg delete $UninstKey /f 2>&1 | Out-Null }
  if ($hadHandler) { & reg import "$Bak\handler.reg" 2>&1 | Out-Null } else { & reg delete $HandlerKey /f 2>&1 | Out-Null }
  [Environment]::SetEnvironmentVariable("Path", $pathPre, "User")
  $i = 0; foreach ($l in $lnks) { if (Test-Path "$Bak\lnk$i.lnk") { Copy-Item "$Bak\lnk$i.lnk" $l -Force } elseif (Test-Path $l) { Remove-Item $l -Force }; $i++ }
  $post = Md5List $DataDirs; $post | Set-Content "$Bak\md5_post.txt"
  $diff = @(Compare-Object $pre $post).Count
  $verPost = if (Test-Path $InstalledExe) { (Get-Item $InstalledExe).VersionInfo.ProductVersion } else { "none" }
  if ($diff -ne 0) { Emit "restore" "-" "fail" "app data md5 differs after restore ($diff entries)" }
  elseif ($verPost -ne $verPre) { Emit "restore" "-" "fail" "installed version changed $verPre -> $verPost" }
  elseif ([Environment]::GetEnvironmentVariable("Path", "User") -ne $pathPre) { Emit "restore" "-" "fail" "user PATH differs" }
  else { Emit "restore" "-" "pass" "data md5 identical, installed version $verPost, PATH/handler/uninstall/shortcuts restored" }
  Remove-Item $Lock -Force -ErrorAction SilentlyContinue
  # ---- report ----
  $rows = @(Get-Content $ResF | Where-Object { $_.Trim() } | ForEach-Object { $_ | ConvertFrom-Json })
  $cnt = @{ pass = 0; fail = 0; skip = 0 }; foreach ($r in $rows) { $cnt[$r.status]++ }
  @{ meta = @{ platform = "windows"; backend = "sendkeys"; commit = $env:CI_COMMIT_SHORT_SHA }; summary = $cnt; cases = $rows } | ConvertTo-Json -Depth 8 | Set-Content "$Run\results.json" -Encoding utf8
  $x = New-Object Text.StringBuilder; [void]$x.AppendLine('<?xml version="1.0" encoding="UTF-8"?>'); [void]$x.AppendLine("<testsuite name=`"appci-windows`" tests=`"$($rows.Count)`" failures=`"$($cnt.fail)`" skipped=`"$($cnt.skip)`">")
  foreach ($r in $rows) { $d = [Security.SecurityElement]::Escape((@($r.detail) -join "; ")); [void]$x.AppendLine("<testcase classname=`"$($r.mode)`" name=`"$($r.name)`" time=`"$($r.seconds)`">")
    if ($r.status -eq "fail") { [void]$x.AppendLine("<failure message=`"$d`"/>") } elseif ($r.status -eq "skip") { [void]$x.AppendLine("<skipped message=`"$d`"/>") }; [void]$x.AppendLine("</testcase>") }
  [void]$x.AppendLine("</testsuite>"); Set-Content "$Run\junit.xml" $x.ToString() -Encoding utf8
  $rows | ForEach-Object { "| {0} | {1} | {2} | {3} |" -f $_.mode, $_.name, $_.status.ToUpper(), (@($_.detail) -join "; ") } | Set-Content "$Run\summary.md" -Encoding utf8
  "pass=$($cnt.pass) fail=$($cnt.fail) skip=$($cnt.skip)"
}
if ($cnt.fail -gt 0) { exit 1 } else { exit 0 }
