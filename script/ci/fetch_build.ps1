# script/ci/fetch_build.ps1: Windows twin of fetch_build.sh (app-test-windows-reuse). Same inputs, same fail-loud rules.
# -Record (after_script) stamps the reused build into appci-out\windows\run\results.json and summary.md.
param([switch]$Record)
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
if ($Record) {
    try {
        $m = (Get-Content appci-out\reused_build.json -Raw | ConvertFrom-Json).reused_build
        $line = "**Tested a REUSED build**: job $($m.job_id) ($($m.job_name)) of pipeline $($m.pipeline_id), commit $($m.commit); this pipeline $($m.this_pipeline), commit $($m.this_commit) (app-identical)."
        foreach ($f in Get-ChildItem appci-out\*\run\results.json -ErrorAction SilentlyContinue) {
            $d = Get-Content $f.FullName -Raw | ConvertFrom-Json
            $d | Add-Member -NotePropertyName reused_build -NotePropertyValue $m -Force
            $d | ConvertTo-Json -Depth 32 | Set-Content $f.FullName -Encoding UTF8
        }
        foreach ($f in Get-ChildItem appci-out\*\run\summary.md -ErrorAction SilentlyContinue) {
            $s = Get-Content $f.FullName -Raw; Set-Content $f.FullName -Value ($line + "`n`n" + $s) -Encoding UTF8
        }
        Write-Output $line
    } catch { Write-Output "record_reuse: $_" }
    exit 0
}
function Die($msg) { Write-Error "ERROR fetch_build: $msg"; exit 2 }
$id = $env:APPCI_BUILD_JOB_ID
if (-not $id) { Die "APPCI_BUILD_JOB_ID is not set. This *-reuse job must be played with the build job id (script/ci/ci_seq.py does that); to test a fresh build play the plain app-test-* job instead." }
if ($id -notmatch '^\d+$') { Die "APPCI_BUILD_JOB_ID='$id' is not a job id" }
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
New-Item -ItemType Directory -Force appci-out | Out-Null
@{ reused_build = @{ job_id = [int64]$id; job_name = "$env:APPCI_BUILD_JOB_NAME"; pipeline_id = "$env:APPCI_BUILD_PIPELINE_ID"; commit = "$env:APPCI_BUILD_SHA"; this_pipeline = "$env:CI_PIPELINE_ID"; this_commit = "$env:CI_COMMIT_SHA" } } | ConvertTo-Json | Set-Content appci-out\reused_build.json -Encoding UTF8
Get-ChildItem dist | Format-Table Name, Length
Write-Output "fetch_build: OK. Testing build job $id from pipeline $env:APPCI_BUILD_PIPELINE_ID (commit $env:APPCI_BUILD_SHA), NOT a build of this pipeline's commit $env:CI_COMMIT_SHA (app-identical by ci_seq.py)."
