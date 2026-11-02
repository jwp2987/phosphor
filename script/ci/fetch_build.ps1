# script/ci/fetch_build.ps1: Windows twin of fetch_build.sh (app-test-windows-reuse). Same inputs, same fail-loud rules.
# -ThisPipeline (plain app-test-windows): copy THIS pipeline's build from the local cache via the pipeline-<id> pointer that
# cache_build.ps1 leaves; no download (`needs` has artifacts: false); a missing copy fails loudly.
# -Record (after_script) stamps the reused build into appci-out\windows\run\results.json and summary.md.
param([switch]$Record, [switch]$ThisPipeline)
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
if ($Record) {
    try {
        $m = (Get-Content appci-out\reused_build.json -Raw | ConvertFrom-Json).reused_build
        $line = "**Tested a REUSED build**: job $($m.job_id) ($($m.job_name)) of pipeline $($m.pipeline_id), commit $($m.commit); this pipeline $($m.this_pipeline), commit $($m.this_commit) (app-identical)."
        # UTF-8 WITHOUT a BOM: Windows PowerShell 5.1's `Set-Content -Encoding UTF8` writes one, and json.load / text readers choke on it (r18cj: every reader
        # of the committed results.json needed utf-8-sig). [IO.File]::WriteAllText with UTF8Encoding($false) is the 5.1-safe way.
        $utf8 = New-Object System.Text.UTF8Encoding($false)
        foreach ($f in Get-ChildItem appci-out\*\run\results.json -ErrorAction SilentlyContinue) {
            $d = [IO.File]::ReadAllText($f.FullName, $utf8) | ConvertFrom-Json
            $d | Add-Member -NotePropertyName reused_build -NotePropertyValue $m -Force
            [IO.File]::WriteAllText($f.FullName, ($d | ConvertTo-Json -Depth 32), $utf8)
        }
        foreach ($f in Get-ChildItem appci-out\*\run\summary.md -ErrorAction SilentlyContinue) {
            $s = [IO.File]::ReadAllText($f.FullName, $utf8); [IO.File]::WriteAllText($f.FullName, ($line + "`n`n" + $s), $utf8)
        }
        Write-Output $line
    } catch { Write-Output "record_reuse: $_" }
    exit 0
}
function Die($msg) { Write-Error "ERROR fetch_build: $msg"; exit 2 }
# Identity (#1501): the cached copy's meta.json (written by cache_build.ps1) must say the job and commit that were asked for. Dies on a mismatch
# or a missing meta.json; reused_build.json is only ever written for a verified build.
function Test-Meta($dir, $wantJob, $wantSha, $wantPipeline) {
    $mp = Join-Path $dir "meta.json"
    if (-not (Test-Path $mp)) { Die "$mp is missing: a build without a recorded identity cannot be verified" }
    try { $m = Get-Content $mp -Raw | ConvertFrom-Json } catch { Die "$mp is unreadable: $_" }
    if ("$($m.job_id)" -ne "$wantJob") { Die "cached build $dir is job $($m.job_id), but job $wantJob was asked for" }
    if (-not $wantSha) { Die "no expected commit to compare with (APPCI_BUILD_SHA / CI_COMMIT_SHA empty)" }
    if ("$($m.sha)" -ne "$wantSha") { Die "cached build $dir was built from commit $($m.sha), but $wantSha was asked for" }
    if ($wantPipeline -and "$($m.pipeline_id)" -ne "$wantPipeline") { Die "cached build $dir belongs to pipeline $($m.pipeline_id), not $wantPipeline" }
    Write-Output "fetch_build: identity verified from ${mp}: job $($m.job_id), commit $("$($m.sha)".Substring(0, 10))"
}
function Copy-Checked($src) {
    if (Test-Path dist) { Remove-Item -Recurse -Force dist }
    New-Item -ItemType Directory dist | Out-Null
    Copy-Item "$src\*" dist -Recurse -Force
    Remove-Item dist\meta.json -ErrorAction SilentlyContinue
    foreach ($l in Get-Content dist\SHA256SUMS) {
        if (-not $l.Trim()) { continue }
        $h, $n = ($l -split '\s+', 2); $n = $n.TrimStart('*').Trim()
        if ((Get-FileHash (Join-Path dist $n) -Algorithm SHA256).Hash.ToLower() -ne $h.ToLower()) { Die "dist\SHA256SUMS check failed for $n" }
    }
}
if ($ThisPipeline) {
    $root = Join-Path $env:USERPROFILE ".cache\phosphor-ci\builds\windows"
    $ptr = Join-Path $root "pipeline-$env:CI_PIPELINE_ID"
    if (-not (Test-Path $ptr)) { Die "no local build for pipeline $env:CI_PIPELINE_ID ($ptr missing): the build job did not run on this machine or cache_build.ps1 failed. Re-run the build job." }
    $bid = (Get-Content $ptr -Raw).Trim(); $src = Join-Path $root $bid
    if (-not (Test-Path (Join-Path $src "SHA256SUMS"))) { Die "local build $src is missing or pruned; re-run the build job" }
    Write-Output "fetch_build: using this pipeline's build job $bid from this machine: $src (no download)"
    Test-Meta $src $bid $env:CI_COMMIT_SHA $env:CI_PIPELINE_ID
    Copy-Checked $src
    exit 0
}
$id = $env:APPCI_BUILD_JOB_ID
if (-not $id) { Die "APPCI_BUILD_JOB_ID is not set. This *-reuse job must be played with the build job id (script/ci/ci_seq.py does that); to test a fresh build play the plain app-test-* job instead." }
if ($id -notmatch '^\d+$') { Die "APPCI_BUILD_JOB_ID='$id' is not a job id" }
$local = Join-Path $env:USERPROFILE ".cache\phosphor-ci\builds\windows\$id"
function Write-Record($how) {
    New-Item -ItemType Directory -Force appci-out | Out-Null
    $rec = @{ reused_build = @{ job_id = [int64]$id; job_name = "$env:APPCI_BUILD_JOB_NAME"; pipeline_id = "$env:APPCI_BUILD_PIPELINE_ID"; commit = "$env:APPCI_BUILD_SHA"; this_pipeline = "$env:CI_PIPELINE_ID"; this_commit = "$env:CI_COMMIT_SHA"; fetched = $how } } | ConvertTo-Json
    [IO.File]::WriteAllText((Join-Path (Get-Location) "appci-out\reused_build.json"), $rec, (New-Object System.Text.UTF8Encoding($false)))      # no BOM (5.1's Set-Content -Encoding UTF8 adds one)
    Get-ChildItem dist | Format-Table Name, Length
    Write-Output "fetch_build: OK. Testing build job $id from pipeline $env:APPCI_BUILD_PIPELINE_ID (commit $env:APPCI_BUILD_SHA), NOT a build of this pipeline's commit $env:CI_COMMIT_SHA (app-identical by ci_seq.py)."
}
if (Test-Path (Join-Path $local "SHA256SUMS")) {
    Write-Output "fetch_build: using the build job $id already on this machine: $local (no download)"
    Test-Meta $local $id $env:APPCI_BUILD_SHA $null
    if (Test-Path dist) { Remove-Item -Recurse -Force dist }
    New-Item -ItemType Directory dist | Out-Null
    Copy-Item "$local\*" dist -Recurse -Force
    Remove-Item dist\meta.json -ErrorAction SilentlyContinue
    foreach ($l in Get-Content dist\SHA256SUMS) {
        if (-not $l.Trim()) { continue }
        $h, $n = ($l -split '\s+', 2); $n = $n.TrimStart('*').Trim()
        if ((Get-FileHash (Join-Path dist $n) -Algorithm SHA256).Hash.ToLower() -ne $h.ToLower()) { Die "dist\SHA256SUMS check failed for $n" }
    }
    Write-Record "local cache ($local)"
    exit 0
}
Write-Output "fetch_build: WARNING the local copy of build job $id is MISSING ($local; pruned, or built on another machine). Falling back to downloading its GitLab artifacts."
$url = "$env:CI_API_V4_URL/projects/$env:CI_PROJECT_ID/jobs/$id/artifacts"
$zip = Join-Path $env:TEMP "appci-build-$id.zip"
Write-Output "fetch_build: downloading artifacts of build job $id (pipeline $env:APPCI_BUILD_PIPELINE_ID, commit $env:APPCI_BUILD_SHA)"
function Fetch($hdr) {
    try { Invoke-WebRequest -Uri $url -Headers $hdr -OutFile $zip -UseBasicParsing; return 200 }
    catch { if ($_.Exception.Response) { return [int]$_.Exception.Response.StatusCode } else { Die "request failed: $($_.Exception.Message)" } }
}
$code = Fetch @{ "JOB-TOKEN" = $env:CI_JOB_TOKEN }
if ($code -ne 200 -and $env:APPCI_API_TOKEN) { $code = Fetch @{ "PRIVATE-TOKEN" = $env:APPCI_API_TOKEN } }
if ($code -eq 404) { Die "HTTP 404: job $id has no artifacts (expired after 3 days, or erased). Rebuild instead." }
if ($code -in 401, 403) { Die "HTTP ${code}: CI_JOB_TOKEN may not read job $id's artifacts (set a masked APPCI_API_TOKEN with read_api, or build instead)." }
if ($code -ne 200) { Die "HTTP $code from $url" }
if (Test-Path dist) { Remove-Item -Recurse -Force dist }
Expand-Archive -Path $zip -DestinationPath . -Force
Remove-Item $zip -Force
if (-not (Test-Path dist) -or -not (Get-ChildItem dist)) { Die "artifacts of job $id contain no dist/" }
# identity BEFORE anything is stamped: GitLab's own record of the job must match the commit and name that were asked for
if (-not $env:APPCI_BUILD_SHA) { Die "APPCI_BUILD_SHA is empty: nothing to verify the downloaded build against" }
$jj = $null; $why = ""
foreach ($h in @(@{ "JOB-TOKEN" = $env:CI_JOB_TOKEN }, @{ "PRIVATE-TOKEN" = $env:APPCI_API_TOKEN })) {
    if (-not ($h.Values | Select-Object -First 1)) { continue }
    try { $jj = Invoke-RestMethod -Uri "$env:CI_API_V4_URL/projects/$env:CI_PROJECT_ID/jobs/$id" -Headers $h -UseBasicParsing; break } catch { $why = "$_" }
}
if (-not $jj) { Die "cannot read job $id from the GitLab API to verify the downloaded build ($why); set a masked APPCI_API_TOKEN with read_api, or build instead" }
if ($jj.commit.id -ne $env:APPCI_BUILD_SHA) { Die "job $id was built from commit $($jj.commit.id), but $env:APPCI_BUILD_SHA was asked for" }
if ($env:APPCI_BUILD_JOB_NAME -and $jj.name -ne $env:APPCI_BUILD_JOB_NAME) { Die "job $id is '$($jj.name)', but '$env:APPCI_BUILD_JOB_NAME' was asked for" }
if ($jj.status -ne "success") { Die "job $id status is '$($jj.status)', not success" }
Write-Output "fetch_build: identity verified from the GitLab API: job $id, commit $($env:APPCI_BUILD_SHA.Substring(0, 10)), $($jj.name), success"
Write-Record "GitLab download (local copy missing)"
