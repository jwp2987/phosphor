# windows/ci/after_job.ps1: after_script of app-test-windows(-reuse). GitLab runs after_script also when a job is CANCELLED, when
# app_test.ps1's finally block (which runs restore.ps1) may never run because the job's process tree was killed.
# 1. Stops only processes this job recorded in appci-out\windows\run\pids ("<kind> <pid>" lines), and only when that PID's executable
#    path or command line lies under this job's appci-out dir (PID-reuse safe). Never by name. Never the GitLab runner.
# 3. (#1517) Removes %USERPROFILE%\WINPUSH.lock only when app_test.ps1 wrote it for THIS job (its text carries "pipeline=<id> job=<CI_JOB_ID> ");
#    a lock held by another job or by a hand-run agent is left alone. A cancelled job would otherwise block the next run for 30 min.
# 2. If this job made a backup (%USERPROFILE%\appci-bak\<CI_JOB_ID>\meta.json) and no restore has run for it yet (no restore.log),
#    runs restore.ps1 for that backup and prints its steps. Always exits 0: it must not mask the job's own result.
# -Check (#1507): parse + bind only; prints "after_job: check ok" and exits 0 without touching processes or backups. The
#    Windows parse gate (script/ci/parse_ps1.ps1) runs it under 5.1 before a job, because an after_script that fails to parse
#    fails silently (after_script results never fail the job).
param([string]$Out = "appci-out\windows", [switch]$Check)
$ErrorActionPreference = "Continue"
if ($Check) { Write-Output "after_job: check ok"; exit 0 }
$outFull = (Resolve-Path $Out -ErrorAction SilentlyContinue).Path
if ($outFull) {
  foreach ($f in @("$outFull\run\pids", "$outFull\pids")) {
    if (-not (Test-Path $f)) { continue }
    foreach ($l in Get-Content $f) {
      $w = $l -split '\s+'; if ($w.Count -lt 2 -or $w[1] -notmatch '^\d+$') { continue }
      $p = Get-CimInstance Win32_Process -Filter "ProcessId = $($w[1])" -ErrorAction SilentlyContinue
      if (-not $p) { Write-Output "after_job: recorded $($w[0]) pid $($w[1]) already gone"; continue }
      $where = "$($p.ExecutablePath) $($p.CommandLine)"
      if ($where -like "*$outFull*") {
        & taskkill.exe /PID $p.ProcessId /T /F 2>&1 | Out-Null
        Write-Output "after_job: stopped $($w[0]) pid $($p.ProcessId)"
      } else { Write-Output "after_job: recorded $($w[0]) pid $($w[1]) is alive but not ours (PID reuse): left alone" }
    }
  }
}
# audit: processes (other than this script) whose executable path or command line names this job's out dir, and the leftover scratch dirs
if ($outFull) {
  $left = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue | Where-Object { $_.ProcessId -ne $PID -and "$($_.ExecutablePath) $($_.CommandLine)" -like "*$outFull*" -and "$($_.CommandLine)" -notlike "*after_job.ps1*" })
  if ($left.Count) { Write-Output "after_job: leftover processes naming ${outFull}:"; foreach ($l in $left) { Write-Output ("after_job:   pid {0} {1}" -f $l.ProcessId, ("$($l.CommandLine)".Substring(0, [Math]::Min(160, "$($l.CommandLine)".Length)))) } }
  else { Write-Output "after_job: leftover processes naming ${outFull}: none" }
  foreach ($d in @("home", "ctl", "rctl", "refctl", "rhome")) {
    $dd = Join-Path $outFull "run\$d"
    if (Test-Path $dd) { Remove-Item $dd -Recurse -Force -ErrorAction SilentlyContinue; Write-Output ("after_job: scratch dir run\{0} {1}" -f $d, $(if (Test-Path $dd) { "COULD NOT BE REMOVED" } else { "removed" })) }
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
# lock last: after the restore, so the next job never sees a half-restored machine as free
if ($env:CI_JOB_ID) {
  $lock = Join-Path $env:USERPROFILE "WINPUSH.lock"
  if (Test-Path $lock) {
    $txt = [string](Get-Content $lock -Raw -ErrorAction SilentlyContinue)
    if ($txt -match ("(^|\s)job=" + [regex]::Escape($env:CI_JOB_ID) + "(\s|$)")) {
      Remove-Item $lock -Force -ErrorAction SilentlyContinue
      Write-Output "after_job: removed WINPUSH.lock (it was this job's)"
    } else { Write-Output "after_job: WINPUSH.lock left alone (not this job's: $($txt.Trim()))" }
  }
}
exit 0
