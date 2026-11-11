# windows/ci/reap.ps1: dot-source it, then `Reap-Leftovers -OutFull <abs path> -Tag <log prefix>` (#2150 and the same class as #2148).
# Ends every process (other than this one and its ancestors) whose executable path or command line names the job's own out dir, by exact PID
# (taskkill /PID <pid> /T /F), after logging it. A leftover child holding the job's output pipe keeps the runner's script step open until the
# job timeout. Never by name or pattern, never the runner (it is an ancestor of this script, and its command line does not name the out dir).
# Bounded: one process snapshot, one taskkill per leftover, no waits. Always returns; never throws.
function Reap-Leftovers([string]$OutFull, [string]$Tag = "reap") {
  if (-not $OutFull) { return }
  try {
    $all = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue)
    $parent = @{}; foreach ($q in $all) { $parent[[int]$q.ProcessId] = [int]$q.ParentProcessId }
    $anc = @{}; $c = [int]$PID; $n = 0
    while ($c -gt 0 -and -not $anc.ContainsKey($c) -and $n -lt 50) { $anc[$c] = $true; $c = if ($parent.ContainsKey($c)) { $parent[$c] } else { 0 }; $n++ }
    $pat = "*" + [Management.Automation.WildcardPattern]::Escape($OutFull) + "*"
    $left = @($all | Where-Object { -not $anc.ContainsKey([int]$_.ProcessId) -and ("$($_.ExecutablePath) $($_.CommandLine)" -like $pat) })
    if (-not $left.Count) { Write-Output "${Tag}: no leftover process names $OutFull"; return }
    foreach ($l in $left) {
      $cl = "$($l.CommandLine)"
      Write-Output ("{0}: leftover pid {1}: {2}" -f $Tag, $l.ProcessId, $cl.Substring(0, [Math]::Min(140, $cl.Length)))
      & taskkill.exe /PID $l.ProcessId /T /F 2>&1 | Out-Null
      $still = Get-Process -Id $l.ProcessId -ErrorAction SilentlyContinue
      Write-Output ("{0}: pid {1} {2}" -f $Tag, $l.ProcessId, $(if ($still) { "STILL RUNNING after taskkill" } else { "ended" }))
    }
  } catch { Write-Output "${Tag}: leftover scan failed: $($_.Exception.Message)" }
}
