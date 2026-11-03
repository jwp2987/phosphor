# script/ci/parse_ps1.ps1: the Windows PowerShell 5.1 parse gate (#1507). Windows PowerShell 5.1, standard user, changes nothing.
#
# script/check_pwsh_parse parses the same files with pwsh 7 on Linux/macOS (script/precheck), but the VM runs 5.1, and an
# after_script that does not parse fails SILENTLY: GitLab never fails a job on its after_script, and `if ($LASTEXITCODE)`
# after a script that failed to parse is a no-op. So this runs the VM's own parser:
#   1. every *.ps1 under app\assets\bundled, script\realapp\windows, script\windows and script\ci (top level), with the
#      same placeholder substitution as check_pwsh_parse (@@TOKEN@@ -> 0, @TOKEN@ -> PLACEHOLDER) in a temp copy;
#   2. the after_script helpers with -Check (parse + parameter binding, no side effects): after_job.ps1 and restore.ps1.
# Exit 1 on any failure or when nothing was found (stale paths); 0 otherwise. Used by the guards-windows job and as the
# first before_script step of app-test-windows(-reuse).
param([string]$Root = "")
$ErrorActionPreference = "Stop"
if (-not $Root) { $Root = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path }
$tmp = Join-Path ([IO.Path]::GetTempPath()) ("parse_ps1_" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force $tmp | Out-Null
$fail = 0; $n = 0
try {
  $files = @()
  foreach ($d in @("app\assets\bundled", "script\realapp\windows", "script\windows")) {
    $p = Join-Path $Root $d
    if (Test-Path $p) { $files += @(Get-ChildItem $p -Recurse -File -Filter *.ps1) }
  }
  $ci = Join-Path $Root "script\ci"
  if (Test-Path $ci) { $files += @(Get-ChildItem $ci -File -Filter *.ps1) }
  foreach ($f in ($files | Sort-Object FullName)) {
    $n++
    $text = [IO.File]::ReadAllText($f.FullName)
    $text = [regex]::Replace($text, '@@[A-Z0-9_]+@@', '0')
    $text = [regex]::Replace($text, '@[A-Z][A-Z0-9_]+@', 'PLACEHOLDER')
    $copy = Join-Path $tmp ("copy-$n-" + $f.Name)
    [IO.File]::WriteAllText($copy, $text, (New-Object System.Text.UTF8Encoding($true)))
    $tokens = $null; $errors = $null
    [void][System.Management.Automation.Language.Parser]::ParseFile($copy, [ref]$tokens, [ref]$errors)
    if ($errors -and $errors.Count) {
      $fail++
      Write-Output "FAIL $($f.FullName)"
      foreach ($e in $errors) { Write-Output "  line $($e.Extent.StartLineNumber): $($e.Message)" }
    }
  }
  if ($n -eq 0) { Write-Output "parse_ps1: FAILED (found no scripts to parse under $Root; the paths are stale)"; exit 1 }

  $ciDir = Join-Path $Root "script\realapp\windows\ci"
  try {
    $o = @(& (Join-Path $ciDir "after_job.ps1") -Check)
    if (-not ($o -contains "after_job: check ok")) { $fail++; Write-Output "FAIL after_job.ps1 -Check printed: $($o -join ' | ')" }
  } catch { $fail++; Write-Output "FAIL after_job.ps1 -Check threw: $($_.Exception.Message)" }
  try {
    $r = @(& (Join-Path $ciDir "restore.ps1") -Check)
    if ($r.Count -ne 1 -or $r[0].step -ne "check" -or -not $r[0].ok) { $fail++; Write-Output "FAIL restore.ps1 -Check returned $($r.Count) row(s)" }
  } catch { $fail++; Write-Output "FAIL restore.ps1 -Check threw: $($_.Exception.Message)" }
} finally {
  Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
if ($fail) { Write-Output "parse_ps1: FAILED ($fail problem(s); PowerShell $($PSVersionTable.PSVersion))"; exit 1 }
Write-Output "parse_ps1: all $n PowerShell scripts parse under PowerShell $($PSVersionTable.PSVersion); after_job.ps1/restore.ps1 -Check ok"
exit 0
