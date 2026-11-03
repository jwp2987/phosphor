# script/ci/verify_artifacts.ps1 -Dist <dir> [-Sums <file>]: post-build verification for the Windows build/release jobs (#1515).
# FAILS: sums file missing/empty, a listed file absent or with a different SHA256, an artifact not covered by the sums.
# REPORTS ONLY: Get-AuthenticodeSignature status. The installer is UNSIGNED unless SIGN_TOOL_CMD is configured (not in CI);
# set PHOSPHOR_REQUIRE_SIGNED=1 to make an unsigned installer fail. See docs/release-signing.md.
param([Parameter(Mandatory)][string]$Dist, [string]$Sums)
$ErrorActionPreference = "Stop"
if (-not $Sums) { $Sums = Join-Path $Dist "SHA256SUMS" }
$script:fail = $false
function Bad($m) { Write-Host "verify_artifacts: ERROR $m"; $script:fail = $true }
if (-not (Test-Path $Dist)) { Bad "no such dir $Dist"; exit 1 }
if (-not (Test-Path $Sums) -or (Get-Item $Sums).Length -eq 0) { Bad "sums file $Sums is missing or empty" }
else {
  $listed = @{}
  foreach ($line in Get-Content $Sums) {
    if (-not $line.Trim()) { continue }
    $want, $name = $line.Trim() -split '\s+', 2; $name = $name.TrimStart('*').Trim(); $listed[$name] = 1
    $f = Join-Path $Dist $name
    if (-not (Test-Path $f)) { Bad "$Sums lists $name but $f does not exist"; continue }
    $got = (Get-FileHash $f -Algorithm SHA256).Hash.ToLower()
    if ($got -ne $want.ToLower()) { Bad "digest mismatch for ${name}: sums say $want, file is $got" }
  }
  if ($listed.Count -eq 0) { Bad "$Sums has no entries" }
  foreach ($f in Get-ChildItem $Dist -File) { if ($f.Name -ne (Split-Path $Sums -Leaf) -and -not $listed.ContainsKey($f.Name)) { Bad "$($f.Name) is in $Dist but not in $Sums" } }
  Write-Host "verify_artifacts: checksums: $($listed.Count) entries checked against $Dist"
}
foreach ($exe in Get-ChildItem $Dist -File -Filter *.exe) {
  $sig = Get-AuthenticodeSignature $exe.FullName
  if ($sig.Status -eq "Valid") { Write-Host "verify_artifacts: Authenticode $($exe.Name): Valid, signer $($sig.SignerCertificate.Subject)" }
  else {
    Write-Host "verify_artifacts: Authenticode $($exe.Name): $($sig.Status)  [UNSIGNED: SmartScreen will warn users. Expected until docs/release-signing.md is done]"
    if ($env:PHOSPHOR_REQUIRE_SIGNED -eq "1") { Bad "installer $($exe.Name) is not validly signed ($($sig.Status))" }
  }
}
if ($script:fail) { Write-Host "verify_artifacts: FAILED"; exit 1 }
Write-Host "verify_artifacts: OK"; exit 0
