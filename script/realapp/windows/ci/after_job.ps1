# windows/ci/after_job.ps1: after_script of app-test-windows(-reuse). GitLab runs after_script also when a job is CANCELLED, when
# app_test.ps1's finally block (which runs restore.ps1) may never run because the job's process tree was killed.
# 1. Stops only processes this job recorded in appci-out\windows\run\pids ("<kind> <pid>" lines), and only when that PID's executable
#    path or command line lies under this job's appci-out dir (PID-reuse safe). Never by name. Never the GitLab runner.
# 2. If this job made a backup (%USERPROFILE%\appci-bak\<CI_JOB_ID>\meta.json) and no restore has run for it yet (no restore.log),
#    runs restore.ps1 for that backup and prints its steps. Always exits 0: it must not mask the job's own result.
param([string]$Out = "appci-out\windows")
$ErrorActionPreference = "Continue"
$outFull = (Resolve-Path $Out -ErrorAction SilentlyContinue).Path
if ($outFull) {
  foreach ($f in @("$outFull\run\pids", "$outFull\pids")) {
    if (-not (Test-Path $f)) { continue }
    foreach ($l in Get-Content $f) {
      $w = $l -split '\s+'; if ($w.Count -lt 2 -or $w[1] -notmatch '^\d+$') { continue }
      $p = Get-CimInstance Win32_Process -Filter "ProcessId = $($w[1])" -ErrorAction SilentlyContinue
      if (-not $p) { continue }
      $where = "$($p.ExecutablePath) $($p.CommandLine)"
      if ($where -like "*$outFull*") {
        & taskkill.exe /PID $p.ProcessId /T /F 2>&1 | Out-Null
        Write-Output "after_job: stopped $($w[0]) pid $($p.ProcessId)"
      }
    }
  }
}
if ($env:CI_JOB_ID) {
  $bak = Join-Path (Join-Path $env:USERPROFILE "appci-bak") $env:CI_JOB_ID
  if ((Test-Path "$bak\meta.json") -and -not (Test-Path "$bak\restore.log")) {
    Write-Output "after_job: the job ended before its restore ran; restoring from $bak"
    $rr = @(& (Join-Path $PSScriptRoot "restore.ps1") -Bak $bak)
    foreach ($r in $rr) { Write-Output ("after_job restore {0,-14} {1} {2}" -f $r.step, $(if ($r.ok) { "ok" } else { "FAILED" }), $r.detail) }
  } else {
    Write-Output "after_job: restore already done (or no backup for job $env:CI_JOB_ID)"
  }
}
exit 0
