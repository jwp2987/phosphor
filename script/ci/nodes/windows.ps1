# windows.ps1 -- rebuild the Windows CI node (GitLab tag `windows`, runner `phosphor-windows-vm`) on a fresh
# Windows 10 x64 VM. Read script/ci/nodes/README.md first.
#
# Windows PowerShell 5.1. DRY RUN BY DEFAULT: every step prints what it would do; -Apply executes. Idempotent.
# Two phases, because the runner user is a STANDARD user and installs go through a separate admin account:
#
#   [ELEVATED] phase, from an elevated PowerShell of the admin account (observed: `phosphor-admin`):
#     powershell -ExecutionPolicy Bypass -File script\ci\nodes\windows.ps1 -Phase Admin -RunnerUser Winters [-Apply]
#   [USER] phase, as the runner user itself (observed: `Winters`, standard user, the console desktop user), NOT elevated:
#     $env:GITLAB_RUNNER_TOKEN = "glrt-..."   # only for the registration step
#     powershell -ExecutionPolicy Bypass -File script\ci\nodes\windows.ps1 -Phase User [-Apply]
#
# EVERY PIN BELOW WAS OBSERVED ON THE VM ON 2026-11-04 (Windows 10 Pro 22H2 build 19045.6456, 8 logical CPUs,
# C: NTFS 149 GB) from a NON-elevated ssh login. What needs elevation to read was not observed and says TODO-VERIFY.
#
# Layout: the runner is NOT a service. It is gitlab-runner.exe started by the scheduled task
# "GitLab Runner (desktop session)" at the runner user's logon, with an interactive token, so it lives in the console
# desktop session and app-test-windows can open windows, send keys and take screenshots. AutoAdminLogon brings that
# session up after a reboot.

param(
  [ValidateSet("Admin", "User")] [string] $Phase = "User",
  [string] $RunnerUser = "Winters",
  [switch] $Apply
)
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

# ------------------------------------------------------------------ pins (observed 2026-11-04)
$GitlabRunnerVersion = "19.5.1"                          # C:\GitLab-Runner\gitlab-runner.exe --version (f411f264)
$RunnerDir           = "C:\GitLab-Runner"                # gitlab-runner.exe, config.toml, .runner_system_id
$BuildsDir           = "C:\gl\b"                         # config.toml builds_dir (short: MAX_PATH)
$CacheDir            = "C:\gl\c"                         # config.toml cache_dir
$RunnerName          = "phosphor-windows-vm"             # config.toml name / GitLab description; tag `windows`
$RunnerConcurrent    = 1
$TaskName            = "GitLab Runner (desktop session)"
$RustToolchain       = "1.99.1"                          # == rust-toolchain.toml (x86_64-pc-windows-msvc)
$RustupObserved      = "1.29.2"                          # rustup --version; installed toolchains: stable (default), 1.92.0, 1.99.1
$CargoBinstall       = "1.18.1"                          # == script/install_cargo_binstall
$CargoNextest        = "0.9.150"                         # ~/.cargo/.crates.toml (NOTE: the Linux box has 0.9.143)
$CargoAbout          = "0.8.4"                           # == build-windows-x86_64's own pin
$CargoMutants        = "27.1.0"
$ProtocVersion       = "25.1"                            # %LOCALAPPDATA%\protoc\bin\protoc.exe, user env PROTOC
$PythonVersion       = "3.12.10"                         # %LOCALAPPDATA%\phosphor-ci\python312 (embeddable layout: python312.zip + ._pth)
$PythonDir           = Join-Path $env:LOCALAPPDATA "phosphor-ci\python312"
# pip list of that Python (pip 26.2.2). paramiko + Pillow are what the harness imports; the rest are their deps.
$PythonPkgs          = @("paramiko==5.1.0", "pillow==13.0.0", "bcrypt==5.0.0", "cffi==2.1.1", "cryptography==50.0.2",
                         "invoke==3.0.3", "PyNaCl==1.6.2", "pycparser==3.12")
$ProcdumpDir         = "C:\Users\$RunnerUser\tools\procdump"   # procdump.exe / procdump64.exe / procdump64a.exe 12.02 + Procdump.zip
# winget package versions (DisplayVersion from the uninstall keys):
$WingetMachine = @(
  @{ Id = "Git.Git";                               Version = "2.56.0.3";   Why = "Git for Windows (git 2.56.0.windows.3): bash.exe runs run_suite.sh, Git Bash pane shell, its ssh.exe" },
  @{ Id = "Kitware.CMake";                         Version = "4.4.4";      Why = "native deps" },
  @{ Id = "StrawberryPerl.StrawberryPerl";         Version = "5.42.3001";  Why = "vendored OpenSSL build (perl 5.42.3)" },
  @{ Id = "JRSoftware.InnoSetup";                  Version = "6.7.3";      Why = "ISCC.exe at C:\Program Files (x86)\Inno Setup 6 (the path CI puts on PATH)" },
  @{ Id = "Microsoft.OpenSSH.Preview";             Version = "10.4.0.0";   Why = "OpenSSH server for admin ssh (C:\Program Files\OpenSSH); winget id TODO-VERIFY" }
)
$WingetUser = @(
  @{ Id = "jqlang.jq"; Version = "1.8.2"; Why = "script/windows/bootstrap.ps1 lists it; not used by CI jobs" }
)
# VS Build Tools 2022 17.14 (installationVersion 17.14.37806.7, channel VisualStudio.17.Release); the components observed:
$VsComponents = @(
  "Microsoft.VisualStudio.Workload.VCTools", "Microsoft.VisualStudio.Workload.MSBuildTools",
  "Microsoft.VisualStudio.Component.VC.Tools.x86.x64", "Microsoft.VisualStudio.Component.VC.CoreBuildTools",
  "Microsoft.VisualStudio.Component.VC.Redist.14.Latest", "Microsoft.VisualStudio.Component.VC.CMake.Project",
  "Microsoft.VisualStudio.Component.VC.ASAN", "Microsoft.VisualStudio.Component.Vcpkg",
  "Microsoft.VisualStudio.Component.Windows10SDK", "Microsoft.VisualStudio.Component.Windows11SDK.22621",
  "Microsoft.VisualStudio.Component.Windows11SDK.26100", "Microsoft.VisualStudio.Component.TestTools.BuildTools",
  "Microsoft.VisualStudio.Component.TextTemplating"
)

# -------------------------------------------------------------------------------- helpers
function Step($t) { Write-Output ""; Write-Output "== $t" }
function Note($t) { Write-Output "   $t" }
function Ok($t)   { Write-Output "   ok:   $t" }
function Die($t)  { Write-Output "   FAIL: $t"; exit 1 }
# Run "description" { scriptblock }: print; execute only with -Apply. A native command's exit code is checked.
function Run([string] $What, [scriptblock] $Do) {
  Write-Output "   + $What"
  if ($Apply) {
    $global:LASTEXITCODE = 0
    try { & $Do } catch { Die "'$What' failed: $($_.Exception.Message)" }
    if ($LASTEXITCODE) { Die "'$What' exited $LASTEXITCODE" }
  }
}
# Quiet { native probe }: run a probe whose stderr does not matter. Windows PowerShell 5.1 turns redirected native stderr
# into error records, which $ErrorActionPreference = Stop would make fatal.
function Quiet([scriptblock] $Probe) {
  $old = $ErrorActionPreference; $ErrorActionPreference = "Continue"
  try { & $Probe 2>$null } catch { $null = $_ } finally { $ErrorActionPreference = $old }
}
function IsElevated {
  $id = [Security.Principal.WindowsIdentity]::GetCurrent()
  return (New-Object Security.Principal.WindowsPrincipal($id)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}
function InstalledVersion([string] $NameLike) {
  $keys = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*", "HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*", "HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*"
  $hit = Get-ItemProperty $keys -ErrorAction SilentlyContinue | Where-Object { $_.DisplayName -like $NameLike } | Select-Object -First 1
  if ($hit) { return $hit.DisplayVersion } else { return $null }
}
$Repo = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..")).Path
$mode = "DRY RUN (no changes)"; if ($Apply) { $mode = "APPLY" }

Step "0. Preflight"
Note "phase: $Phase   runner user: $RunnerUser   mode: $mode"
$cv = Get-ItemProperty "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion"
Note "OS: $($cv.ProductName) $($cv.DisplayVersion) build $($cv.CurrentBuild).$($cv.UBR) (observed node: Windows 10 Pro 22H2 19045.6456)"
if (-not (Test-Path (Join-Path $Repo "rust-toolchain.toml"))) { Die "run from a checkout of the phosphor repo" }

# ================================================================== [ELEVATED] phase
if ($Phase -eq "Admin") {
  if (-not (IsElevated)) { Die "the Admin phase needs an elevated PowerShell (Run as administrator, admin account)" }
  if ($env:USERNAME -eq $RunnerUser) { Note "WARNING you are elevated AS the runner user; the observed VM keeps the runner user standard (non-admin)" }

  Step "A1. [ELEVATED] winget packages, machine scope"
  if (-not (Get-Command winget -ErrorAction SilentlyContinue)) { Die "winget (App Installer) is missing; install it from the Microsoft Store first" }
  foreach ($p in $WingetMachine) {
    Note "$($p.Id) $($p.Version): $($p.Why)"
    $id = $p.Id; $ver = $p.Version
    Run "winget install -e --id $id --version $ver --scope machine" { winget install -e --id $id --version $ver --scope machine --accept-package-agreements --accept-source-agreements --disable-interactivity }
  }
  Note "Git for Windows installer defaults are what the VM has (system gitconfig: core.autocrlf=true, core.longpaths=true,"
  Note "core.symlinks=false, core.fscache=true, credential.helper=manager). .gitattributes forces eol=lf for text anyway."
  Note "Inno Setup 6.7.3 is ALSO installed per-user on the observed VM (%LOCALAPPDATA%\Programs\Inno Setup 6); CI uses the machine copy."

  Step "A2. [ELEVATED] Visual Studio Build Tools 2022 (17.14) with the observed components"
  $vsw = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
  $haveVs = $false
  if (Test-Path $vsw) { $haveVs = [bool](& $vsw -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 Microsoft.VisualStudio.Component.Windows11SDK.22621 -property installationPath) }
  if ($haveVs) { Ok "VS Build Tools with VC.Tools.x86.x64 + Windows11SDK.22621 present" } else {
    $adds = ($VsComponents | ForEach-Object { "--add $_" }) -join " "
    $override = "--passive --wait --norestart $adds"
    Run "winget install Microsoft.VisualStudio.2022.BuildTools --override '$override'" { winget install -e --id Microsoft.VisualStudio.2022.BuildTools --accept-package-agreements --accept-source-agreements --override $override }
  }
  Note "Also on the VM: the standalone Windows SDK 10.0.26100.7705 (with Application Verifier, MSI tools, WACK) and SDK 10.0.22621.5040."
  Note "Whether CI needs the standalone 26100 SDK beyond the VS component: TODO-VERIFY (optional: winget install Microsoft.WindowsSDK.10.0.26100)."

  Step "A3. [ELEVATED] C:\gl and C:\GitLab-Runner, writable by $RunnerUser"
  foreach ($d in @($RunnerDir, "C:\gl", $BuildsDir, $CacheDir)) {
    if (Test-Path $d) { Ok $d } else { Run "mkdir $d" { New-Item -ItemType Directory -Force $d | Out-Null } }
  }
  Run "icacls C:\gl and $RunnerDir : grant ${RunnerUser}:(OI)(CI)M" {
    icacls "C:\gl" /grant "${RunnerUser}:(OI)(CI)M" | Out-Null
    icacls $RunnerDir /grant "${RunnerUser}:(OI)(CI)M" | Out-Null
  }
  Note "Exact observed ACLs on these dirs: TODO-VERIFY (not readable non-elevated); the runner user writes both today."

  Step "A4. [ELEVATED] Long paths"
  $lp = (Get-ItemProperty "HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem").LongPathsEnabled
  if ($lp -eq 1) { Ok "LongPathsEnabled=1" } else { Run "LongPathsEnabled=1" { Set-ItemProperty "HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem" LongPathsEnabled 1 -Type DWord } }

  Step "A5. [ELEVATED] AutoAdminLogon for $RunnerUser (brings the desktop session, and so the runner, up after a reboot)"
  $wl = "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon"
  $w = Get-ItemProperty $wl
  if ($w.AutoAdminLogon -eq "1" -and $w.DefaultUserName -eq $RunnerUser) { Ok "AutoAdminLogon=1 DefaultUserName=$RunnerUser" } else {
    Run "Winlogon AutoAdminLogon=1, DefaultUserName=$RunnerUser" { Set-ItemProperty $wl AutoAdminLogon "1"; Set-ItemProperty $wl DefaultUserName $RunnerUser }
  }
  Note "MANUAL: the password is NOT stored by this script. Observed: no DefaultPassword registry value, so it is most likely"
  Note "kept as an LSA secret by Sysinternals Autologon (TODO-VERIFY). Run Autologon64.exe (Sysinternals) as admin, enter"
  Note "$RunnerUser's password from your password manager. Never put the password in the repo, a script or Winlogon in clear."

  Step "A6. [ELEVATED] Power: never sleep on AC (a sleeping VM takes the runner offline)"
  Note "Observed (Balanced scheme): AC sleep after 0x708 = 30 min, display off after 0x258 = 10 min. Reported as fragile."
  Run "powercfg /change standby-timeout-ac 0; monitor-timeout-ac 0; hibernate-timeout-ac 0" {
    powercfg /change standby-timeout-ac 0; powercfg /change monitor-timeout-ac 0; powercfg /change hibernate-timeout-ac 0
  }

  Step "A7. [ELEVATED] Notes (not changed)"
  Note "Developer Mode: observed OFF (AllowDevelopmentWithoutDevLicense unset). Keep it off: without it the app falls back to"
  Note "  directory junctions (skill_dirs_publish.rs) and the unit suite is calibrated for that (docs/testing.md r18u16)."
  Note "Administrators group observed: Administrator, phosphor-admin. $RunnerUser stays a standard user."
  Note "No gitlab-runner Windows SERVICE (it would run in session 0, where app-test-windows cannot see the desktop)."
  Note "Windows Defender exclusions for C:\gl / .cargo: TODO-VERIFY (not readable non-elevated)."
  Note "Next: log in as $RunnerUser and run this script with -Phase User."
  exit 0
}

# ================================================================== [USER] phase (the runner user, not elevated)
if (IsElevated) { Note "WARNING this phase is meant for the standard runner user, not elevated; per-user state would land in the wrong profile" }
if ($env:USERNAME -ne $RunnerUser) { Note "WARNING running as $env:USERNAME, not $RunnerUser" }

Step "U1. Rust $RustToolchain (msvc) in %USERPROFILE%\.rustup / .cargo (observed rustup $RustupObserved; RUSTUP_HOME/CARGO_HOME unset)"
$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
$rustup = Join-Path $cargoBin "rustup.exe"
if (Test-Path $rustup) { Ok ((Quiet { & $rustup --version }) | Select-Object -First 1) } else {
  $init = Join-Path $env:TEMP "rustup-init.exe"
  Run "download rustup-init.exe; rustup-init -y --profile minimal --default-toolchain stable" {
    Invoke-WebRequest -UseBasicParsing -Uri "https://win.rustup.rs/x86_64" -OutFile $init
    & $init -y --profile minimal --default-toolchain stable
  }
}
$env:PATH = "$cargoBin;$env:PATH"
Run "rustup toolchain install $RustToolchain (rustfmt, clippy, rust-analyzer)" { & $rustup toolchain install $RustToolchain --profile minimal --component rustfmt,clippy,rust-analyzer }
Run "rustup toolchain install 1.92.0 (also present on the VM; not used by CI)" { & $rustup toolchain install 1.92.0 --profile minimal }

Step "U2. cargo tools in %USERPROFILE%\.cargo\bin"
$bash = "C:\Program Files\Git\bin\bash.exe"
if (-not (Test-Path $bash)) { Die "Git for Windows bash.exe is missing (Admin phase A1)" }
Run "script/install_cargo_binstall (cargo-binstall $CargoBinstall, SHA-pinned) via Git Bash" { & $bash (Join-Path $Repo "script/install_cargo_binstall") }
foreach ($spec in @("cargo-nextest@$CargoNextest", "cargo-about@$CargoAbout", "cargo-mutants@$CargoMutants")) {
  $s = $spec
  Run "cargo binstall --no-confirm --locked $s" { & (Join-Path $cargoBin "cargo.exe") binstall --no-confirm --locked $s }
}
Note "Also present, not needed by CI: wgslfmt (git tag 2025-06-28), diesel_cli 2.3.14, naga-cli 29.0.3."
Note "cargo-fuzz is not installed: fuzz-smoke-windows fails by design (cargo-fuzz is Unix-only)."

Step "U3. protoc $ProtocVersion in %LOCALAPPDATA%\protoc + user env PROTOC (as script/windows/bootstrap.ps1)"
$protocExe = Join-Path $env:LOCALAPPDATA "protoc\bin\protoc.exe"
if ((Test-Path $protocExe) -and ((& $protocExe --version) -eq "libprotoc $ProtocVersion")) { Ok "$protocExe libprotoc $ProtocVersion" } else {
  $zip = Join-Path $env:TEMP "protoc-$ProtocVersion-win64.zip"
  Run "download + expand protoc $ProtocVersion" {
    Invoke-WebRequest -UseBasicParsing -Uri "https://github.com/protocolbuffers/protobuf/releases/download/v$ProtocVersion/protoc-$ProtocVersion-win64.zip" -OutFile $zip
    Expand-Archive -Path $zip -DestinationPath (Join-Path $env:LOCALAPPDATA "protoc") -Force
    Remove-Item $zip
  }
}
if ([Environment]::GetEnvironmentVariable("PROTOC", "User") -eq $protocExe) { Ok "PROTOC (user) set" } else {
  Run "setx-equivalent: user env PROTOC=$protocExe" { [Environment]::SetEnvironmentVariable("PROTOC", $protocExe, "User") }
}

Step "U4. winget packages, user scope"
if (Get-Command winget -ErrorAction SilentlyContinue) {
  foreach ($p in $WingetUser) { $id = $p.Id; $ver = $p.Version; Note "$id $ver : $($p.Why)"; Run "winget install -e --id $id --version $ver --scope user" { winget install -e --id $id --version $ver --scope user --accept-package-agreements --accept-source-agreements --disable-interactivity } }
} else { Note "winget is not on PATH in this session (observed over ssh too); install jq $($WingetUser[0].Version) from the desktop if wanted" }

Step "U5. Private Python $PythonVersion at $PythonDir (NOT on PATH; the harness calls it by full path)"
$py = Join-Path $PythonDir "python.exe"
if (Test-Path $py) { Ok "$(Quiet { & $py --version })" } else {
  $zip = Join-Path $env:TEMP "python-$PythonVersion-embed-amd64.zip"
  Run "download the embeddable Python $PythonVersion, expand to $PythonDir, enable 'import site' in python312._pth, get-pip" {
    Invoke-WebRequest -UseBasicParsing -Uri "https://www.python.org/ftp/python/$PythonVersion/python-$PythonVersion-embed-amd64.zip" -OutFile $zip
    New-Item -ItemType Directory -Force $PythonDir | Out-Null
    Expand-Archive -Path $zip -DestinationPath $PythonDir -Force
    Remove-Item $zip
    # Observed python312._pth: "python312.zip", ".", "", "# Uncomment to run site.main() automatically", "import site"
    $pth = Join-Path $PythonDir "python312._pth"
    (Get-Content $pth) -replace '^#\s*import site', 'import site' | Set-Content -Encoding ASCII $pth
    $gp = Join-Path $env:TEMP "get-pip.py"
    Invoke-WebRequest -UseBasicParsing -Uri "https://bootstrap.pypa.io/get-pip.py" -OutFile $gp
    & $py $gp --no-warn-script-location
    Remove-Item $gp
  }
}
Note "Install method inferred from the layout (python312.zip + python312._pth with 'import site'): TODO-VERIFY"
$pkgs = $PythonPkgs
Run "python -m pip install pip==26.2.2 $($pkgs -join ' ')" {
  & $py -m pip install --disable-pip-version-check --no-warn-script-location "pip==26.2.2"
  & $py -m pip install --disable-pip-version-check --no-warn-script-location @pkgs
}

Step "U6. procdump (Sysinternals) at $ProcdumpDir (observed 12.02; for hang dumps, used by hand)"
if (Test-Path (Join-Path $ProcdumpDir "procdump64.exe")) { Ok "procdump64.exe $((Get-Item (Join-Path $ProcdumpDir 'procdump64.exe')).VersionInfo.FileVersion)" } else {
  Run "download Procdump.zip (Sysinternals serves only the latest; check it is 12.02 or note the new version)" {
    New-Item -ItemType Directory -Force $ProcdumpDir | Out-Null
    $z = Join-Path (Split-Path $ProcdumpDir) "Procdump.zip"
    Invoke-WebRequest -UseBasicParsing -Uri "https://download.sysinternals.com/files/Procdump.zip" -OutFile $z
    Expand-Archive -Path $z -DestinationPath $ProcdumpDir -Force
  }
}
Note "No CI job references procdump (grep of the repo, 2026-11-04); it is the hand-run diagnostics tool."

Step "U7. gitlab-runner $GitlabRunnerVersion at $RunnerDir\gitlab-runner.exe"
$exe = Join-Path $RunnerDir "gitlab-runner.exe"
$haveRunner = $false
if (Test-Path $exe) { $haveRunner = [bool]((Quiet { & $exe --version }) -match "Version:\s+$([regex]::Escape($GitlabRunnerVersion))$") }
if ($haveRunner) { Ok "$exe $GitlabRunnerVersion" } else {
  Run "download gitlab-runner-windows-amd64.exe v$GitlabRunnerVersion -> $exe" {
    Invoke-WebRequest -UseBasicParsing -Uri "https://gitlab-runner-downloads.s3.amazonaws.com/v$GitlabRunnerVersion/binaries/gitlab-runner-windows-amd64.exe" -OutFile $exe
  }
}

Step "U8. Register (shell executor, PowerShell 5.1, builds $BuildsDir, cache $CacheDir, concurrent $RunnerConcurrent)"
$cfg = Join-Path $RunnerDir "config.toml"
if ((Test-Path $cfg) -and (Select-String -Path $cfg -Pattern "name = `"$RunnerName`"" -Quiet)) { Ok "$cfg already has $RunnerName (not touched: it holds the token)" }
elseif (-not $env:GITLAB_RUNNER_TOKEN) { Note "GITLAB_RUNNER_TOKEN not set: skipping (create the runner in GitLab first, README.md)" }
else {
  $tok = $env:GITLAB_RUNNER_TOKEN
  Run "gitlab-runner register --non-interactive --executor shell --shell powershell ..." {
    & $exe register --non-interactive --config $cfg --url "https://gitlab.com" --token $tok --executor shell --shell powershell `
      --name $RunnerName --builds-dir $BuildsDir --cache-dir $CacheDir
  }
}
if (Test-Path $cfg) {
  if (Select-String -Path $cfg -Pattern "^concurrent = $RunnerConcurrent$" -Quiet) { Ok "concurrent = $RunnerConcurrent" }
  else { Run "set concurrent = $RunnerConcurrent" { (Get-Content $cfg) -replace '^concurrent = .*', "concurrent = $RunnerConcurrent" | Set-Content -Encoding ASCII $cfg } }
}
Note "Observed: $cfg is readable by $RunnerUser (it must be: the runner runs as $RunnerUser). Keep it out of backups that leave the VM."

Step "U9. Scheduled task '$TaskName' (exact observed definition; LogonType InteractiveToken = the console session)"
$userId = "$env:COMPUTERNAME\$RunnerUser"
$taskXml = @"
<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.3" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <URI>\$TaskName</URI>
  </RegistrationInfo>
  <Principals>
    <Principal id="Author">
      <UserId>$userId</UserId>
      <LogonType>InteractiveToken</LogonType>
    </Principal>
  </Principals>
  <Settings>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <RestartOnFailure>
      <Count>999</Count>
      <Interval>PT1M</Interval>
    </RestartOnFailure>
    <IdleSettings>
      <Duration>PT10M</Duration>
      <WaitTimeout>PT1H</WaitTimeout>
      <StopOnIdleEnd>true</StopOnIdleEnd>
      <RestartOnIdle>false</RestartOnIdle>
    </IdleSettings>
    <UseUnifiedSchedulingEngine>true</UseUnifiedSchedulingEngine>
  </Settings>
  <Triggers>
    <LogonTrigger>
      <UserId>$userId</UserId>
    </LogonTrigger>
  </Triggers>
  <Actions Context="Author">
    <Exec>
      <Command>$exe</Command>
      <Arguments>run --config "$cfg" --working-directory C:\gl</Arguments>
      <WorkingDirectory>$RunnerDir</WorkingDirectory>
    </Exec>
  </Actions>
</Task>
"@
Note "(observed UserId is the SID S-1-5-21-...-1001 of $RunnerUser; the account name resolves to the same principal)"
$exists = $false
Quiet { schtasks /query /tn $TaskName } | Out-Null
if ($LASTEXITCODE -eq 0) { $exists = $true }
if ($exists) { Ok "task exists (compare: schtasks /query /tn `"$TaskName`" /xml)" } else {
  $xmlFile = Join-Path $env:TEMP "gitlab-runner-task.xml"
  Run "schtasks /create /tn `"$TaskName`" /xml <task xml>" {
    [IO.File]::WriteAllText($xmlFile, $taskXml, [Text.Encoding]::Unicode)
    schtasks /create /tn $TaskName /xml $xmlFile /f
    Remove-Item $xmlFile
  }
}
Note "Start it now without logging off: schtasks /run /tn `"$TaskName`". If the runner hangs: Stop-Process it, then schtasks /run."

Step "U10. Console QuickEdit OFF for the runner console (a mouse click in QuickEdit 'Select' mode FREEZES the runner)"
Note "Observed 2026-11-04: HKCU\Console QuickEdit=1 and no per-window override for the runner: QuickEdit is ON (reported)."
$runnerConsoleKey = "HKCU:\Console\" + ($exe -replace '\\', '_')     # conhost keys per window title = the exe path; TODO-VERIFY
foreach ($k in @("HKCU:\Console", $runnerConsoleKey)) {
  $cur = $null
  if (Test-Path $k) { $cur = (Get-ItemProperty $k -ErrorAction SilentlyContinue).QuickEdit }
  if ($cur -eq 0) { Ok "$k QuickEdit=0" } else {
    $kk = $k
    Run "$kk QuickEdit=0" { if (-not (Test-Path $kk)) { New-Item -Path $kk -Force | Out-Null }; Set-ItemProperty $kk QuickEdit 0 -Type DWord }
  }
}
Note "Takes effect for consoles opened afterwards: restart the task (Stop-Process gitlab-runner; schtasks /run)."

Step "U11. Session hygiene for app-test-windows"
Note "Screen saver observed: ScreenSaveActive=1, no timeout, not secure; no lock-screen policy. Keep the session unlocked:"
Note "  Settings > Accounts > Sign-in options > 'Require sign-in' = Never; screen saver None. Values not changed here."
Note "Do not set APPCI_WIN_REF=wt unless Windows Terminal is installed (observed: not installed; reference = conhost)."

Step "U12. MANUAL: restore machine-local CI inputs from your backup (never in the repo or GitLab variables)"
Note "%USERPROFILE%\.config\phosphor-ci\ci.env               variable names: APPCI_OLLAMA_URL, APPCI_OLLAMA_MODEL"
Note "%USERPROFILE%\.config\phosphor-ci\owner-settings.toml  copy of the owner's settings.toml"
Note "Runtime dirs the harness creates itself (do not restore): %USERPROFILE%\appci-bak\<job>, WINPUSH.lock"
Note "Verify with CI: script/ci/nodes/README.md 'Verification' (diag-windows, guards-windows, check-windows,"
Note "build-windows-x86_64, test-warpui-core-windows, app-test-windows on a build/* push)."
Write-Output ""
Write-Output "Done ($mode)."
