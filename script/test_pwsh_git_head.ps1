# Asserts that pwsh.ps1's in-process Warp-GitHeadInProcess (#1163) reports the same
# branch/head as the `git symbolic-ref` / `rev-parse` calls Warp-Precmd used to make,
# and times both. Usage: pwsh -NoProfile -File script/test_pwsh_git_head.ps1
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$src = Join-Path $root 'app/assets/bundled/bootstrap/pwsh.ps1'
$ast = [System.Management.Automation.Language.Parser]::ParseFile($src, [ref]$null, [ref]$null)
$fn = $ast.FindAll({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq 'Warp-GitHeadInProcess' }, $true) | Select-Object -First 1
Invoke-Expression $fn.Extent.Text

# What the old Warp-Precmd computed, via real git.
function Old-Head {
    $b = '' | & git symbolic-ref --short HEAD 2>$null
    if ($null -ne $b) { return @($b, $b) }
    $h = '' | & git rev-parse --short HEAD 2>$null
    if ($null -ne $h) { return @('', $h) }
    return @('', '')
}
# What the new code computes (git only when the helper declines).
function New-Head {
    $i = Warp-GitHeadInProcess
    if ($null -ne $i -and $i.NotRepo) { return @('', '') }
    if ($null -ne $i -and $null -ne $i.Branch) { return @($i.Branch, $i.Head) }
    return $null
}

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("gh1163-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory $tmp | Out-Null
$fail = 0
try {
    $repo = Join-Path $tmp 'repo'
    git init -q -b main $repo
    Push-Location $repo
    git -c user.email=a@b -c user.name=t commit -q --allow-empty -m one
    git branch feature/x-y
    New-Item -ItemType Directory sub/deep -Force | Out-Null
    git worktree add -q ../wt feature/x-y
    git -c user.email=a@b -c user.name=t commit -q --allow-empty -m two
    $sha = git rev-parse HEAD
    git worktree add -q --detach ../wtd HEAD
    Pop-Location
    $nonrepo = Join-Path $tmp 'plain'
    New-Item -ItemType Directory $nonrepo | Out-Null
    $unborn = Join-Path $tmp 'unborn'
    git init -q -b fresh $unborn
    $bare = Join-Path $tmp 'bare.git'
    git init -q --bare -b main $bare

    $cases = @(
        @('repo root', $repo, $true), @('repo subdir', (Join-Path $repo 'sub/deep'), $true),
        @('worktree', (Join-Path $tmp 'wt'), $true), @('non-repo', $nonrepo, $true),
        @('unborn', $unborn, $true), @('detached worktree', (Join-Path $tmp 'wtd'), $false),
        @('bare', $bare, $false))
    foreach ($c in $cases) {
        Push-Location $c[1]
        $old = Old-Head
        $new = New-Head
        if ($null -eq $new) {
            if ($c[2]) { Write-Host "FAIL $($c[0]): fell back to git unexpectedly"; $fail++ }
            else { Write-Host "ok   $($c[0]): declines (git fallback), old=$($old -join '|')" }
        } elseif ($old[0] -ne $new[0] -or $old[1] -ne $new[1]) {
            Write-Host "FAIL $($c[0]): old=$($old -join '|') new=$($new -join '|')"; $fail++
        } else { Write-Host "ok   $($c[0]): $($new -join '|')" }
        if ($c[2]) {
            $t_old = (Measure-Command { 1..100 | ForEach-Object { Old-Head | Out-Null } }).TotalMilliseconds / 100
            $t_new = (Measure-Command { 1..100 | ForEach-Object { New-Head | Out-Null } }).TotalMilliseconds / 100
            Write-Host ("       old {0:N2} ms  new {1:N2} ms" -f $t_old, $t_new)
        }
        Pop-Location
    }
    # Env overrides must make the helper decline.
    Push-Location $repo
    $env:GIT_DIR = (Join-Path $repo '.git')
    if ($null -ne (Warp-GitHeadInProcess)) { Write-Host 'FAIL GIT_DIR should decline'; $fail++ }
    Remove-Item env:GIT_DIR
    Pop-Location
} finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
if ($fail) { Write-Host "$fail failure(s)"; exit 1 }
Write-Host 'all ok'
