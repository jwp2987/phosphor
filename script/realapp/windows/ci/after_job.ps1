# windows/ci/after_job.ps1: after_script of app-test-windows(-reuse). GitLab runs after_script also when a job is CANCELLED, when
# app_test.ps1's finally block (which runs restore.ps1) may never run because the job's process tree was killed.
# 1. Stops only processes this job recorded in appci-out\windows\run\pids ("<kind> <pid>" lines), and only when that PID's executable
#    path or command line lies under this job's appci-out dir (PID-reuse safe). Never by name. Never the GitLab runner.
#    (#1642) The app runs inside a Job Object with KILL_ON_JOB_CLOSE (appci/app.py WindowsApp), so conhost trees die with the harness; the
#    "jobmember <pid> <creation FILETIME>" lines it records are this script's fallback for anything that still holds app.stdout.
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
$sw = [Diagnostics.Stopwatch]::StartNew()
$outFull = (Resolve-Path $Out -ErrorAction SilentlyContinue).Path
if ($outFull) {
  # #2123: ONE process snapshot for all recorded PIDs. This loop used to ask WMI once per recorded line; round 14's 7 h run had recorded 1067 of them
  # (1061 long gone), which took the whole 5 minutes GitLab gives an after_script, so the scrub and the slim behind it never ran and the failing run
  # kept no logs or screenshots. A PID missing from the snapshot is gone (counted, one line in all); one that is in it is asked for again, by itself,
  # right before it is judged, so what is killed is decided on the process as it is then, exactly as before. Each recorded line is judged once.
  $snap = @{}
  foreach ($q in @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue)) { $snap["$($q.ProcessId)"] = $true }
  if (-not $snap.Count) { Write-Output "after_job: the process snapshot is empty (WMI did not answer): asking for each recorded pid by itself" }
  $seen = @{}; $gone = 0; $recorded = 0
  foreach ($f in @("$outFull\run\pids", "$outFull\pids")) {
    if (-not (Test-Path $f)) { continue }
    foreach ($l in Get-Content $f) {
      $w = $l -split '\s+'; if ($w.Count -lt 2 -or $w[1] -notmatch '^\d+$') { continue }
      $key = ($w[0..([Math]::Min(2, $w.Count - 1))] -join ' '); if ($seen[$key]) { continue }; $seen[$key] = $true; $recorded++
      if ($snap.Count -and -not $snap[$w[1].TrimStart('0')]) { $gone++; continue }
      $p = Get-CimInstance Win32_Process -Filter "ProcessId = $($w[1])" -ErrorAction SilentlyContinue
      if (-not $p) { $gone++; continue }
      $where = "$($p.ExecutablePath) $($p.CommandLine)"
      # #1642: "jobmember <pid> <creation FILETIME>" lines are written by WindowsApp.stop from the app's Job Object (conhost, shells: command lines
      # that never name the out dir). The creation time, not the path, proves the PID is still the process the job held. Only PIDs recorded here.
      $isMember = $false
      if ($w[0] -eq "jobmember" -and $w.Count -ge 3 -and $w[2] -match '^\d+$' -and $p.CreationDate) { $isMember = ($p.CreationDate.ToFileTimeUtc() -eq [int64]$w[2]) }
      if ($isMember -or $where -like "*$outFull*") {
        & taskkill.exe /PID $p.ProcessId /T /F 2>&1 | Out-Null
        Write-Output "after_job: stopped $($w[0]) pid $($p.ProcessId)"
      } else { Write-Output "after_job: recorded $($w[0]) pid $($w[1]) is alive but not ours (PID reuse): left alone" }
    }
  }
  Write-Output ("after_job: {0} recorded process(es), {1} already gone ({2:N1} s)" -f $recorded, $gone, $sw.Elapsed.TotalSeconds)
}
# fake ssh lab (docs/testing.md 2.15): stop the container named in run\lab.json by its exact id (the forced-command key stays in the job environment)
if ($outFull -and (Test-Path "$outFull\run\lab.json")) {
  $labpy = Join-Path $env:LOCALAPPDATA "phosphor-ci\python312\python.exe"
  if (Test-Path $labpy) { & $labpy (Join-Path $PSScriptRoot "..\..\appci\lab.py") stop "$outFull\run" 2>&1 | ForEach-Object { Write-Output "after_job: $_" } }
  else { Write-Output "after_job: run\lab.json exists but the VM python is missing: the lab container ends at its TTL" }
}
# leftover processes naming this job's out dir are ended; then the leftover scratch dirs
if ($outFull) {
  # #2150: leftovers are ENDED by exact PID (reap.ps1), not only listed; one that keeps the job's output pipe open stalls the runner step
  . (Join-Path $PSScriptRoot "reap.ps1")
  Reap-Leftovers -OutFull $outFull -Tag "after_job"
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
Write-Output ("after_job: done in {0:N1} s" -f $sw.Elapsed.TotalSeconds)
exit 0
