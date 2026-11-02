# script/ci/cache_build.ps1: last step of a SUCCESSFUL build-windows-x86_64. Windows twin of cache_build.sh.
# Cache: %USERPROFILE%\.cache\phosphor-ci\builds\windows\<CI_JOB_ID>\ (dist files + meta.json); outside the build dir. Atomic; newest 3 kept.
$ErrorActionPreference = "Stop"
$id = $env:CI_JOB_ID
if (-not $id) { Write-Error "CI_JOB_ID is not set"; exit 1 }
if (-not (Test-Path dist\SHA256SUMS)) { Write-Error "cache_build: dist\SHA256SUMS missing; not caching"; exit 1 }
$root = Join-Path $env:USERPROFILE ".cache\phosphor-ci\builds\windows"
New-Item -ItemType Directory -Force $root | Out-Null
$tmp = Join-Path $root ".tmp-$id"
if (Test-Path $tmp) { Remove-Item -Recurse -Force $tmp }
New-Item -ItemType Directory $tmp | Out-Null
Copy-Item dist\* $tmp -Recurse -Force
@{ job_id = [int64]$id; pipeline_id = "$env:CI_PIPELINE_ID"; sha = "$env:CI_COMMIT_SHA"; ref = "$env:CI_COMMIT_REF_NAME"; created = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ") } | ConvertTo-Json | Set-Content (Join-Path $tmp "meta.json") -Encoding UTF8
$final = Join-Path $root $id
if (Test-Path $final) { Remove-Item -Recurse -Force $final }
Move-Item $tmp $final
if ($env:CI_PIPELINE_ID) { Set-Content (Join-Path $root "pipeline-$env:CI_PIPELINE_ID") -Value $id -Encoding ASCII }
Get-ChildItem $root -Directory | Where-Object { $_.Name -match '^\d+$' } | Sort-Object { [int64]$_.Name } -Descending | Select-Object -Skip 3 | Remove-Item -Recurse -Force
Get-ChildItem $root -Directory -Filter ".tmp-*" | Where-Object { $_.LastWriteTime -lt (Get-Date).AddHours(-2) } | Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
Get-ChildItem $root -File -Filter "pipeline-*" | Where-Object { -not (Test-Path (Join-Path $root (Get-Content $_.FullName -Raw).Trim())) } | Remove-Item -Force -ErrorAction SilentlyContinue
Write-Output "cache_build: cached dist\ of job $id at $final"
