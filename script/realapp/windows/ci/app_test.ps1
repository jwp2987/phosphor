# windows/ci/app_test.ps1: CI real-app test on the Windows VM (jobs app-test-windows / app-test-windows-reuse). Windows PowerShell 5.1, standard user.
# Usage: powershell -NoProfile -ExecutionPolicy Bypass -File app_test.ps1 -Dist dist -Out appci-out\windows [-Modes terminal,agent]
#
# THIN VM WRAPPER (#1488). The cases, the key injection (SendInput, ctypes), the screenshots, the ssh test host and the report are the SAME Python
# harness every platform runs (script/realapp/appci/run.py, `--backend windows`), executed by the VM's private Python
# (%LOCALAPPDATA%\phosphor-ci\python312\python.exe, 3.12 + paramiko + Pillow; it is not on PATH). This script keeps only what is specific to
# touching the shared VM safely (docs/testing.md 3.4):
#   * ONE GUI driver at a time: C:\Users\<user>\WINPUSH.lock (younger than 2 h => wait up to 30 min, else fail), removed at the end;
#   * side-by-side install into -Out\app (the installed v0.1.15 stays and must report its version again afterwards); Python gets the exe path;
#   * before installing: back up the app data dirs (md5 list), the uninstall key, user PATH, Start-menu/Desktop shortcuts and the
#     phosphor:// handler; the finally block runs restore.ps1 (unchanged) and fails the job if anything differs;
#   * only the PID the Python harness started is ever stopped (it uses taskkill /PID <pid> /T); the GitLab runner process is never touched;
#   * keys are only sent while Phosphor is the foreground window (the backend's guard), otherwise the case fails instead of typing elsewhere.
# APPCI_WIN_BAK=<backup dir> tells the Python harness where to write kbd_pre.json (the keyboard snapshot restore.ps1 verifies).
# Env that selects the run is unchanged: APPCI_TIER (smoke = the shared fix-tier list), APPCI_WIN_SHELLS (default "powershell,gitbash").
# Results: Out\run\results.jsonl, results.json, junit.xml, summary.md, shots\*.png, logs\ (written by the Python harness, plus the `restore` row below).
param([Parameter(Mandatory)][string]$Dist, [Parameter(Mandatory)][string]$Out, [string]$Modes = "terminal,agent")
$ErrorActionPreference = "Stop"
$Out = [IO.Path]::GetFullPath((Join-Path (Get-Location) $Out)); $Dist = [IO.Path]::GetFullPath((Join-Path (Get-Location) $Dist))
$Run = Join-Path $Out "run"; $Shots = Join-Path $Run "shots"; $Work = Join-Path $Out "w"; $App = Join-Path $Out "app"
# The backup lives OUTSIDE the job dir (a later job's git clean must not delete it); newest 3 are kept. restore.ps1 can finish a restore from it by hand.
$BakRoot = Join-Path $env:USERPROFILE "appci-bak"; $BakId = if ($env:CI_JOB_ID) { $env:CI_JOB_ID } else { Get-Date -Format "yyyyMMdd-HHmmss" }; $Bak = Join-Path $BakRoot $BakId
New-Item -ItemType Directory -Force $Run, $Shots, $Work, $Bak | Out-Null
$ResF = Join-Path $Run "results.jsonl"; "" | Set-Content $ResF -Encoding ascii
$Lock = Join-Path $env:USERPROFILE "WINPUSH.lock"
$DataDirs = @("$env:LOCALAPPDATA\phosphor", "$env:APPDATA\phosphor")
$UninstKey = "HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\zap-oss_is1"
$HandlerKey = "HKCU\Software\Classes\phosphor"
$InstalledExe = "$env:LOCALAPPDATA\Programs\Phosphor\phosphor-oss.exe"
$Py = Join-Path $env:LOCALAPPDATA "phosphor-ci\python312\python.exe"
$AppciDir = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\..\appci"))
$script:Fail = 0
$pyrc = -1

function Emit($name, $mode, $status, $detail, $secs = 0, $metrics = @{}, $shots = @()) {
  $o = [ordered]@{ name = $name; mode = $mode; status = $status; seconds = [math]::Round($secs, 1); detail = @($detail); metrics = $metrics; shots = @($shots) }
  ($o | ConvertTo-Json -Compress -Depth 10) | Add-Content $ResF -Encoding ascii
  "[{0}] {1,-24} {2} {3}" -f $mode, $name, $status.ToUpper(), (@($detail) -join "; ")
  if ($status -eq "fail") { $script:Fail++ }
}
function Md5List($dirs) { Get-ChildItem $dirs -Recurse -File -ErrorAction SilentlyContinue | ForEach-Object { "{0} {1}" -f (Get-FileHash $_.FullName -Algorithm MD5).Hash, $_.FullName } | Sort-Object }
function Reg-Code($regArgs) { $ErrorActionPreference = "Continue"; $null = & reg.exe @regArgs 2>&1; $LASTEXITCODE }  # judged by exit code, never by stderr

# ---- lock ----
$t = Get-Date
while ((Test-Path $Lock) -and ((Get-Date) - (Get-Item $Lock).LastWriteTime).TotalHours -lt 2) {
  if (((Get-Date) - $t).TotalMinutes -gt 30) { Emit "preflight" "-" "fail" "WINPUSH.lock held by: $(Get-Content $Lock -Raw)"; exit 3 }
  Start-Sleep 30 }
"app-test-windows pipeline=$env:CI_PIPELINE_ID job=$env:CI_JOB_ID $(Get-Date -Format s)" | Set-Content $Lock

$verPre = "none"
try {
  # ---- backup (outside the job dir; see restore.ps1) ----
  Get-ChildItem $BakRoot -Directory -ErrorAction SilentlyContinue | Where-Object { $_.FullName -ne $Bak } | Sort-Object LastWriteTime -Descending | Select-Object -Skip 2 | ForEach-Object { Remove-Item $_.FullName -Recurse -Force -ErrorAction SilentlyContinue }
  $pre = Md5List $DataDirs; $pre | Set-Content "$Bak\md5_pre.txt"
  $had = @{}; $k = 0; foreach ($d in $DataDirs) { $nm = @("local", "roaming")[$k]; $had[$nm] = [bool](Test-Path $d); if ($had[$nm]) { Copy-Item $d "$Bak\data_$nm" -Recurse -Force }; $k++ }
  $ec = Reg-Code @("export", $UninstKey, "$Bak\uninstall.reg", "/y"); $hadUninst = ($ec -eq 0)
  $ec = Reg-Code @("export", $HandlerKey, "$Bak\handler.reg", "/y"); $hadHandler = ($ec -eq 0)
  $pathPre = [Environment]::GetEnvironmentVariable("Path", "User"); [IO.File]::WriteAllText("$Bak\path_pre.txt", [string]$pathPre)
  $lnks = @("$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Phosphor.lnk", "$env:USERPROFILE\Desktop\Phosphor.lnk")
  $i = 0; foreach ($l in $lnks) { if (Test-Path $l) { Copy-Item $l "$Bak\lnk$i.lnk" -Force }; $i++ }
  $verPre = if (Test-Path $InstalledExe) { (Get-Item $InstalledExe).VersionInfo.ProductVersion } else { "none" }
  @{ dirs = $DataDirs; had_local = $had["local"]; had_roaming = $had["roaming"]; hadUninst = $hadUninst; hadHandler = $hadHandler; uninstKey = $UninstKey; handlerKey = $HandlerKey; lnks = $lnks; installedExe = $InstalledExe; verPre = $verPre; job = $env:CI_JOB_ID } | ConvertTo-Json | Set-Content "$Bak\meta.json" -Encoding ascii
  "backup: $Bak"
  "installed before: $verPre"
  Get-Process phosphor-oss -ErrorAction SilentlyContinue | ForEach-Object { "note: Phosphor already running (pid $($_.Id), not ours; left alone)" }

  # ---- side-by-side install of the artifact ----
  $setup = Get-ChildItem $Dist -Filter *.exe | Select-Object -First 1
  if (-not $setup) { Emit "preflight" "-" "fail" "no installer .exe in $Dist (needs build-windows-x86_64 artifacts)"; throw "no installer" }
  $p = Start-Process $setup.FullName -ArgumentList "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/CURRENTUSER", "/DIR=$App", "/LOG=$Out\install.log" -Wait -PassThru
  if ($p.ExitCode -ne 0 -or -not (Test-Path "$App\phosphor-oss.exe")) { Emit "preflight" "-" "fail" "installer exit $($p.ExitCode), no $App\phosphor-oss.exe"; throw "install failed" }
  "installed test build: $((Get-Item "$App\phosphor-oss.exe").VersionInfo.ProductVersion) in $App"

  # ---- the shared Python harness ----
  if (-not (Test-Path $Py)) { Emit "preflight" "-" "fail" "no private Python at $Py (3.12 with paramiko + Pillow is part of the VM setup, docs/testing.md 3.4)"; throw "no python" }
  $ErrorActionPreference = "Continue"
  $null = & $Py -c "import sys, paramiko, PIL.ImageGrab; print('python', sys.version.split()[0], 'paramiko', paramiko.__version__, 'Pillow', PIL.__version__)" 2>&1 | ForEach-Object { "$_" } | Tee-Object -Variable pyver
  $pyok = $LASTEXITCODE
  $ErrorActionPreference = "Stop"
  $pyver
  if ($pyok -ne 0) { Emit "preflight" "-" "fail" "the private Python cannot import paramiko and Pillow: $($pyver -join ' ')"; throw "python modules" }
  $env:APPCI_WIN_BAK = $Bak
  $env:PYTHONUNBUFFERED = "1"
  $env:PYTHONUTF8 = "1"   # r18e4: open()/print default to cp1252 here; a U+FFFD in a case log crashed save_case_log and its crash handler, ending the run after the PowerShell pane
  $ErrorActionPreference = "Continue"       # native stderr text is output, not an error; the exit code is what counts
  & $Py (Join-Path $AppciDir "run.py") --run $Run --bin "$App\phosphor-oss.exe" --backend windows --platform windows --modes $Modes
  $pyrc = $LASTEXITCODE
  $ErrorActionPreference = "Stop"
  "run.py exit code: $pyrc"
}
catch { Emit "harness" "-" "fail" $_.Exception.Message }
finally {
  $ErrorActionPreference = "Continue"   # every step below is independent: one failure must not skip the rest
  try {
    $rr = @(& (Join-Path $PSScriptRoot "restore.ps1") -Bak $Bak)
    foreach ($r in $rr) { "restore step {0}: {1} {2}" -f $r.step, $(if ($r.ok) { "ok" } else { "FAILED" }), $r.detail }
    $bad = @($rr | Where-Object { -not $_.ok })
    if (-not $rr.Count) { Emit "restore" "-" "fail" "restore.ps1 produced no steps" }
    elseif ($bad.Count) { Emit "restore" "-" "fail" ("restore failed: " + (($bad | ForEach-Object { "$($_.step): $($_.detail)" }) -join "; ") + " - finish by hand with script\realapp\windows\ci\restore.ps1 -Bak $Bak") }
    else { Emit "restore" "-" "pass" "all $($rr.Count) restore steps ok (data md5 identical, version $verPre, PATH/handler/uninstall/shortcuts restored)" }
  } catch { Emit "restore" "-" "fail" "restore crashed: $($_.Exception.Message) - finish by hand with restore.ps1 -Bak $Bak" }
  try { Remove-Item $Lock -Force -ErrorAction SilentlyContinue } catch {}
}
# ---- report: results.json / junit.xml / summary.md again, now including the wrapper's rows (restore, preflight) ----
$rrc = 1
if (Test-Path $Py) {
  # --backend/--platform are passed although report_meta.json carries them: run.py's parser used to REQUIRE --backend, died with a usage error here, and the
  # restore row never reached results.json or summary.md (r18cj; the restore itself had passed). run.py no longer requires it with --report-only.
  if (Test-Path (Join-Path $Run "report_meta.json")) { & $Py (Join-Path $AppciDir "run.py") --run $Run --report-only --backend windows --platform windows; $rrc = $LASTEXITCODE }
  else { & $Py (Join-Path $AppciDir "report.py") $Run "platform=windows" "backend=windows" "commit=$env:CI_COMMIT_SHORT_SHA"; $rrc = $LASTEXITCODE }
} else { "no Python: no report could be generated" }
if ($rrc -ne 0 -or $script:Fail -gt 0 -or ($pyrc -ne 0)) { exit 1 } else { exit 0 }
