# windows/ci/restore.ps1: finish (or redo) the restore that app_test.ps1 does after an app-test-windows job. Windows PowerShell 5.1, standard user.
# Usage by hand:  powershell -NoProfile -ExecutionPolicy Bypass -File restore.ps1 [-Bak %USERPROFILE%\appci-bak\<job id>]   (default: newest backup)
# app_test.ps1 dot-calls this in its finally block. Every step runs on its own (own try/catch, ErrorActionPreference=Continue, native
# commands judged by $LASTEXITCODE, never by stderr) and its outcome is printed + appended to <Bak>\restore.log. The backup is made by
# app_test.ps1 in %USERPROFILE%\appci-bak\<CI_JOB_ID> (outside the job dir so a later git clean cannot delete it; newest 3 are kept):
# meta.json, md5_pre.txt, data_local\, data_roaming\, uninstall.reg, handler.reg, path_pre.txt, lnk0.lnk, lnk1.lnk.
# Returns an array of objects { step; ok; detail }; ok=$false rows are failures. Never stops the GitLab runner or any process.
param([string]$Bak = "")
$ErrorActionPreference = "Continue"
if (-not $Bak) { $d = Get-ChildItem (Join-Path $env:USERPROFILE "appci-bak") -Directory -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 1; if ($d) { $Bak = $d.FullName } }
if (-not $Bak -or -not (Test-Path "$Bak\meta.json")) { [pscustomobject]@{ step = "backup"; ok = $false; detail = "no backup with meta.json found (Bak='$Bak')" }; return }
$meta = Get-Content "$Bak\meta.json" -Raw | ConvertFrom-Json
$log = Join-Path $Bak "restore.log"
function Step($name, [scriptblock]$body) {
  $ok = $true; $detail = "ok"
  try { $r = & $body; if ($r -is [string] -and $r) { $detail = $r } elseif ($r -is [array]) { $detail = ($r -join "; ") } }
  catch { $ok = $false; $detail = $_.Exception.Message }
  if ($ok -and $detail -like "FAIL:*") { $ok = $false }
  $o = [pscustomobject]@{ step = $name; ok = $ok; detail = $detail }
  "{0} {1,-12} {2} {3}" -f (Get-Date -Format s), $name, $(if ($ok) { "ok" } else { "FAILED" }), $detail | Add-Content $log
  $o
}
function Reg($regArgs) { $null = & reg.exe @regArgs 2>&1; $LASTEXITCODE }   # exit code only: stderr text from reg.exe is not an error
function RegHas($k) { (Reg @("query", $k)) -eq 0 }
function Md5List($dirs) { Get-ChildItem $dirs -Recurse -File -ErrorAction SilentlyContinue | ForEach-Object { "{0} {1}" -f (Get-FileHash $_.FullName -Algorithm MD5).Hash, $_.FullName } | Sort-Object }

foreach ($x in @(@("local", $meta.dirs[0]), @("roaming", $meta.dirs[1]))) {
  $nm = $x[0]; $dir = $x[1]
  Step "data-$nm" {
    $src = "$Bak\data_$nm"
    if (Test-Path $dir) { Remove-Item $dir -Recurse -Force -ErrorAction Continue }
    if (Test-Path $src) { Copy-Item $src $dir -Recurse -Force -ErrorAction Stop; "restored $dir" }
    elseif (-not $meta."had_$nm") { "absent before, removed" } else { "FAIL: backup $src missing but dir existed before" }
  }
}
Step "uninstall-key" {
  if ($meta.hadUninst) { $c = Reg @("import", "$Bak\uninstall.reg"); if ($c) { "FAIL: reg import exit $c" } else { "imported" } }
  elseif (RegHas $meta.uninstKey) { $c = Reg @("delete", $meta.uninstKey, "/f"); if ($c) { "FAIL: reg delete exit $c" } else { "deleted (not there before)" } } else { "absent, as before" } }
Step "handler-key" {
  if ($meta.hadHandler) { $c = Reg @("import", "$Bak\handler.reg"); if ($c) { "FAIL: reg import exit $c" } else { "imported" } }
  elseif (RegHas $meta.handlerKey) { $c = Reg @("delete", $meta.handlerKey, "/f"); if ($c) { "FAIL: reg delete exit $c" } else { "deleted (not there before)" } } else { "absent, as before" } }
Step "user-path" {
  $pre = if (Test-Path "$Bak\path_pre.txt") { [IO.File]::ReadAllText("$Bak\path_pre.txt") } else { throw "path_pre.txt missing" }
  $now = [Environment]::GetEnvironmentVariable("Path", "User")
  if ($now -ceq $pre) { "unchanged" } else { [Environment]::SetEnvironmentVariable("Path", $pre, "User"); if ([Environment]::GetEnvironmentVariable("Path", "User") -ceq $pre) { "restored" } else { "FAIL: PATH still differs after set" } } }
$i = 0
foreach ($l in $meta.lnks) {
  $n = $i; $lnk = $l
  Step "shortcut$n" { if (Test-Path "$Bak\lnk$n.lnk") { Copy-Item "$Bak\lnk$n.lnk" $lnk -Force -ErrorAction Stop; "restored" } elseif (Test-Path $lnk) { Remove-Item $lnk -Force -ErrorAction Stop; "removed (not there before)" } else { "absent, as before" } }
  $i++
}
Step "keyboard-layout" {
  # cases_caseintl.ps1 switches only the Phosphor window to a German layout (WM_INPUTLANGCHANGEREQUEST) and writes kbd_pre.json first; this step proves the
  # user's language list and HKCU\Keyboard Layout\Preload are what they were, and repairs them when a layout load persisted
  $f = "$Bak\kbd_pre.json"
  if (-not (Test-Path $f)) { "no keyboard snapshot (international-input cases did not run)" } else {
    $pre = Get-Content $f -Raw | ConvertFrom-Json; $fix = @()
    $cur = @{}; try { $k = Get-ItemProperty "HKCU:\Keyboard Layout\Preload" -ErrorAction Stop; foreach ($p in $k.PSObject.Properties) { if ($p.Name -notlike "PS*") { $cur[$p.Name] = [string]$p.Value } } } catch {}
    $want = @{}; if ($pre.preload) { foreach ($p in $pre.preload.PSObject.Properties) { $want[$p.Name] = [string]$p.Value } }
    $same = ($cur.Count -eq $want.Count); foreach ($n in $want.Keys) { if ($cur[$n] -ne $want[$n]) { $same = $false } }
    if (-not $same) {
      foreach ($n in @($cur.Keys)) { Remove-ItemProperty "HKCU:\Keyboard Layout\Preload" -Name $n -ErrorAction SilentlyContinue }
      foreach ($n in $want.Keys) { Set-ItemProperty "HKCU:\Keyboard Layout\Preload" -Name $n -Value $want[$n] }
      $fix += "Preload restored"
    }
    try {
      $tags = @($pre.langs | ForEach-Object { $_.tag }); $nowTags = @(Get-WinUserLanguageList | ForEach-Object { $_.LanguageTag })
      if ($tags.Count -and (($tags -join ",") -ne ($nowTags -join ","))) { Set-WinUserLanguageList -LanguageList $tags -Force; $fix += "language list restored ($($nowTags -join ',') -> $($tags -join ','))" }
    } catch { $fix += "FAIL: language list check threw $($_.Exception.Message)" }
    if ($fix -match "^FAIL") { $fix -join "; " } elseif ($fix.Count) { "repaired: " + ($fix -join "; ") } else { "language list and Preload identical to the snapshot" }
  } }
Step "verify-data" {
  $pre = @(Get-Content "$Bak\md5_pre.txt" -ErrorAction Stop | Where-Object { $_ }); $post = @(Md5List $meta.dirs); $post | Set-Content "$Bak\md5_post.txt"
  $d = @(Compare-Object $pre $post).Count; if ($d) { "FAIL: app data md5 differs after restore ($d entries)" } else { "data md5 identical" } }
Step "verify-version" {
  $v = if (Test-Path $meta.installedExe) { (Get-Item $meta.installedExe).VersionInfo.ProductVersion } else { "none" }
  if ($v -ne $meta.verPre) { "FAIL: installed version changed $($meta.verPre) -> $v" } else { "installed version $v" } }
