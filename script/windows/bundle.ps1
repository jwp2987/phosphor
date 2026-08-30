#!/usr/bin/env powershell
#
# Bundle the application for release.

Param (
    # Build dev bundles by default.
    [Switch]$DEBUG_BUILD = $False,

    [Alias('check-only')]
    [Switch]$CHECK_ONLY,

    [ValidateSet('local', 'dev', 'preview', 'stable', 'oss')]
    [String]$CHANNEL = 'dev',

    [Alias('release-tag')]
    [String]$RELEASE_TAG = '',
    [String]$FEATURES = 'release_bundle,crash_reporting,gui',

    # Builds only the Zap binary, skips the installer.
    [Switch]$SKIP_BUILD_INSTALLER = $False,
    # Builds only the installer, skips the Zap binary. Use this if the Zap
    # binary has already been built.
    [Switch]$SKIP_BUILD_BINARY = $False,

    [ValidateSet('x64', 'arm64')]
    [String]$ARCH = '',

    # A signtool command for Inno Setup to sign the setup engine and uninstaller.
    # Uses $f as the file placeholder, e.g.:
    #   'signtool.exe sign /fd SHA256 ... $f'
    # When empty, the installer is built without signing.
    [Alias('sign-tool-cmd')]
    [String]$SIGN_TOOL_CMD = ''
)

if ($RELEASE_TAG) {
    $env:GIT_RELEASE_TAG = $RELEASE_TAG
}

# Use provided ARCH parameter if set, otherwise detect from system
if (-not $ARCH) {
    if ($env:PROCESSOR_ARCHITECTURE -eq 'AMD64') {
        $ARCH = 'x64'
    } elseif ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') {
        $ARCH = 'arm64'
    } else {
        throw "Unsupported processor architecture: $env:PROCESSOR_ARCHITECTURE"
    }
}

if ($ARCH -eq 'arm64') {
    $FILE_ENDING = 'Setup-arm64'
    $PLATFORM_TARGET = 'aarch64-pc-windows-msvc'
} else {
    # If x64, then we just use the filename "WarpSetup.exe" for example
    $FILE_ENDING = 'Setup'
    $PLATFORM_TARGET = 'x86_64-pc-windows-msvc'
}

$ErrorActionPreference = 'Stop'

$WORKSPACE_ROOT_DIR = $(Get-Location).Path
$CARGO_TARGET_DIR = $WORKSPACE_ROOT_DIR + '\target'
$WINDOWS_INSTALLER_DIR = $WORKSPACE_ROOT_DIR + '\script\windows'

if ($DEBUG_BUILD) {
    $CARGO_PROFILE = 'dev'
} elseif (("$CHANNEL" -eq 'local') -or ("$CHANNEL" -eq 'dev')) {
    # For dev bundles, we want to enable debug assertions to
    # catch violations that would otherwise silently pass in
    # a normal release build (e.g. in stable).
    $CARGO_PROFILE = 'rltoda'
} else {
    $CARGO_PROFILE = 'rlto'
}

if ($CARGO_PROFILE -eq 'dev') {
    $CARGO_TARGET_OUTPUT_DIR = "$CARGO_TARGET_DIR" + '\' + $PLATFORM_TARGET + '\debug'
} else {
    $CARGO_TARGET_OUTPUT_DIR = "$CARGO_TARGET_DIR" + '\' + $PLATFORM_TARGET + '\' + "$CARGO_PROFILE"
}
$BUNDLE_ID = "dev.warp.$app_name"

# Update parameters based on the target release channel.
#
# APP_NAME here must match the value used in Rust as the
# application name; see app/src/channel.rs.
#
# WARP_BIN is the name of the binary produced by cargo;
# BINARY_NAME is the desired name of the binary in the final package.
if ("$CHANNEL" -eq 'local') {
    $WARP_BIN = 'warp'
    $BINARY_NAME = 'warp.exe'
    $APP_NAME = 'WarpLocal'
    $FEATURES = "$FEATURES,nld_improvements"
} elseif ("$CHANNEL" -eq 'dev') {
    $WARP_BIN = 'dev'
    $BINARY_NAME = 'dev.exe'
    $APP_NAME = 'WarpDev'
    $FEATURES = "$FEATURES,agent_mode_debug,nld_improvements"
} elseif ("$CHANNEL" -eq 'preview') {
    $WARP_BIN = 'preview'
    $BINARY_NAME = 'preview.exe'
    $APP_NAME = 'WarpPreview'
    $FEATURES = "$FEATURES,preview_channel,nld_improvements"
} elseif ("$CHANNEL" -eq 'stable') {
    $WARP_BIN = 'stable'
    $BINARY_NAME = 'warp.exe'
    $APP_NAME = 'Phosphor'
    # TODO(vorporeal): Remove this once we get tests passing with this default enabled.
    $FEATURES = "$FEATURES,nld_improvements"
} elseif ("$CHANNEL" -eq 'oss') {
    $WARP_BIN = 'phosphor-oss'
    $BINARY_NAME = 'phosphor-oss.exe'
    $APP_NAME = 'Phosphor'
    # The OSS channel uses local crash reporting; it doesn't enable the release default feature set.
    # `autoupdate` is DELIBERATELY ABSENT from this list. Do not "restore parity"
    # by adding it back (#630).
    #
    # No platform ships the GUI autoupdater: script/linux/bundle's oss branch
    # never had it, and `160cfca59` turned it on for macOS and Windows only --
    # that is the commit this line reverses, so mac/Windows now match Linux.
    #
    # The cargo feature is the ONLY enable path. `FeatureFlag::Autoupdate` is
    # deliberately not in `RELEASE_FLAGS` (see the comment at
    # crates/warp_features/src/lib.rs:896); the flag is set solely by
    # `#[cfg(feature = "autoupdate")] FeatureFlag::Autoupdate` in `enabled_features()`
    # (app/src/lib.rs:2927-2928). Dropping it here is therefore sufficient and
    # complete -- there is nothing to also switch off in Rust.
    #
    # Kept for whoever re-enables it: with the feature on, autoupdate polls the
    # GitHub Releases API for jwp2987/phosphor -- the repo is REPO_OWNER/REPO_NAME in
    # app/src/autoupdate/github.rs:13-14, not a URL configured here. The installer is
    # downloaded to a tempfile (tempfile::Builder in
    # app/src/autoupdate/windows.rs:72-75), NOT to ~/Downloads, and Inno Setup IS
    # invoked: windows.rs:347-357 execs the downloaded installer with Inno switches
    # (/SP- /NORESTART /LOG /update=1 /NOCLOSEAPPLICATIONS /DIR=...). On the Oss channel
    # it deliberately omits /SILENT, so the user sees the standard install UI and can
    # cancel -- that is the difference from the official channels, not "no Inno Setup".
    #
    #
    # Scope: this is the GUI app. `phosphor-tui` ships a SEPARATE background updater
    # (`crates/warp_tui/src/autoupdate.rs`), keyed off `general.autoupdate_enabled`
    # (default true) rather than this cargo feature or `FeatureFlag::Autoupdate`. It
    # is inert for a different reason -- eligibility needs a managed
    # `versions/<version>/` install layout the shipped tarball never creates -- and is
    # declined separately in DECLINED.md. Do not conflate the two.
    # `gui` and `nld_improvements` are unrelated and load-bearing; keep them.
    $FEATURES = 'release_bundle,gui,nld_improvements'
}

$BINARY_PATH = "$CARGO_TARGET_OUTPUT_DIR\$BINARY_NAME"
# AUMID (Windows AppUserModel ID) — must exactly match what the process side's
# `ChannelState::app_id()` generates, otherwise Windows ToastNotificationManager
# silently swallows the toast when the Start Menu shortcut / process AUMID
# mismatch. OSS (Phosphor) is `dev.phosphor.Phosphor` (formerly `dev.zap.Zap`)
# in `app/src/bin/zap_oss.rs`; other official channels are `dev.warp.<Name>`.
if ("$CHANNEL" -eq 'oss') {
    $AUMID = "dev.phosphor.$APP_NAME"
} else {
    $AUMID = "dev.warp.$APP_NAME"
}
$BUNDLE_ID = $AUMID
$INSTALLER_OUTPUT_DIR = "$WINDOWS_INSTALLER_DIR\Output"
$INSTALLER_NAME = "$($APP_NAME)$($FILE_ENDING)"
$INSTALLER_PATH = "$($INSTALLER_OUTPUT_DIR)\$($INSTALLER_NAME).exe"
$PDB_PATH = "$CARGO_TARGET_OUTPUT_DIR\$WARP_BIN.pdb"

# The CARGO_FULL_PROFILE environment variable is read by the `cargo` build
# script (`app/build.rs`) to determine where to place `conpty.dll`.
if ($DEBUG_BUILD) {
    $env:CARGO_FULL_PROFILE = 'debug'
} else {
    $env:CARGO_FULL_PROFILE = $CARGO_PROFILE
}

# If we only want to check that compilation will succeed, perform the checks
# then exit.  We use this script to invoke `cargo check` to ensure that we are
# using the same feature flags and profile that we would be using in production.
if ($CHECK_ONLY) {
    cargo check -p warp --profile "$CARGO_PROFILE" --bin "$WARP_BIN" --features "$FEATURES" --target $PLATFORM_TARGET
    if (-Not $?) {
        Write-Error "Failed to verify Zap $WARP_BIN compilation with profile $CARGO_PROFILE"
        exit 1
    }
    exit 0
}

if (-Not $SKIP_BUILD_BINARY) {
    Write-Output "Building Zap for channel $CHANNEL and bundle id $BUNDLE_ID"
    $env:CARGO_BIN_NAME = $CHANNEL
    $env:WARP_APP_NAME = $APP_NAME
    cargo build -p warp --profile "$CARGO_PROFILE" --bin "$WARP_BIN" --features "$FEATURES" --target $PLATFORM_TARGET
    if (-Not $?) {
        Write-Error "Failed to build Zap $WARP_BIN binary with profile $CARGO_PROFILE"
        exit 1
    }

    # If we desire an executable name different from the cargo bin, rename it.
    if ("$WARP_BIN.exe" -ne $BINARY_NAME) {
        $binarySource = "$CARGO_TARGET_OUTPUT_DIR\$WARP_BIN.exe"
        Write-Output "Renaming executable $WARP_BIN.exe to $BINARY_NAME"
        Move-Item -Path "$binarySource" -Destination "$BINARY_PATH" -Force
    }
}

if ($SKIP_BUILD_INSTALLER) {
    # If this is being run within a GitHub action, set an output variable with the
    # location of the binary so it can be referenced by subsequent actions.
    if ($env:GITHUB_ACTIONS -eq 'true') {
        Write-Output '::echo::on'
        "target_profile_dir=$CARGO_TARGET_OUTPUT_DIR" >> "$env:GITHUB_OUTPUT"
        "binary_path=$BINARY_PATH" >> "$env:GITHUB_OUTPUT"
        Write-Output '::echo::off'
    }
    exit 0
}

Write-Output "Built for $ARCH with executable at $BINARY_PATH"

# Prepare bundled resources
$BUNDLED_RESOURCES_DIR = "$CARGO_TARGET_OUTPUT_DIR\resources"
Write-Output "Preparing bundled resources..."
# Only forward --target to the schema generator when the build target is
# runnable on the host; otherwise `cargo run` would try to execute a
# cross-compiled binary (e.g. aarch64-pc-windows-msvc on an x64 runner)
# and fail.
if ($env:PROCESSOR_ARCHITECTURE -eq 'AMD64') {
    $HOST_TARGET = 'x86_64-pc-windows-msvc'
} elseif ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') {
    $HOST_TARGET = 'aarch64-pc-windows-msvc'
} else {
    $HOST_TARGET = ''
}
if ($PLATFORM_TARGET -eq $HOST_TARGET) {
    $SCHEMA_CARGO_TARGET = $PLATFORM_TARGET
} else {
    $SCHEMA_CARGO_TARGET = ''
}
& "$WINDOWS_INSTALLER_DIR\prepare_bundled_resources.ps1" -DestinationDir "$BUNDLED_RESOURCES_DIR" -Channel "$CHANNEL" -CargoProfile "$CARGO_PROFILE" -CargoFeatures "$FEATURES" -CargoTarget "$SCHEMA_CARGO_TARGET"
if (-Not $?) {
    Write-Error "Failed to prepare bundled resources"
    exit 1
}

Write-Output 'Building Zap installer'
# Inno Setup's `AppId` determines the registry Uninstall entry and the
# upgrade-tracking key. Fixed to `zap-oss` under OSS, to avoid staying on the
# default `warp-terminal-oss`.
#
# DELIBERATELY NOT renamed to `phosphor-oss` as part of the dev.zap.Zap ->
# dev.phosphor.Phosphor identity rename. This value is an installer-internal
# upgrade-tracking token, independent of the app's runtime AppId triple (which
# already flips to dev.phosphor.Phosphor via the Rust-side change regardless
# of what this string is). Changing it makes the new installer fail to
# recognize an existing `zap-oss` install as the same product: the user ends
# up with two Add/Remove Programs entries and two install trees on disk
# (see LAYER3-PLAN.md §6, "Windows uninstall orphaning"). Keeping it fixed is
# exactly why it was pinned to a literal in the first place rather than
# templated from the channel/app name. Revisit only with a deliberate,
# tested migration plan for existing OSS installs.
#
# Other channels use the default `warp-terminal-{ReleaseChannel}` from the
# .iss file.
if ("$CHANNEL" -eq 'oss') {
    $INNO_APP_ID = 'zap-oss'
} else {
    $INNO_APP_ID = "warp-terminal-$CHANNEL"
}
$ISCC_ARGS = @(
    "$WINDOWS_INSTALLER_DIR\windows-installer.iss",
    "/DReleaseChannel=$CHANNEL",
    "/DMyAppExeName=$BINARY_NAME",
    "/DTargetProfileDir=$CARGO_TARGET_OUTPUT_DIR",
    "/DMyAppName=$APP_NAME",
    "/DMyAppVersion=$env:GIT_RELEASE_TAG",
    "/DArch=$ARCH",
    "/DOutputName=$INSTALLER_NAME",
    "/DAppUserModelId=$AUMID",
    "/DInnoAppId=$INNO_APP_ID"
)
# Also accept the sign tool command via env var
if (-not $SIGN_TOOL_CMD -and $env:SIGN_TOOL_CMD) {
    $SIGN_TOOL_CMD = $env:SIGN_TOOL_CMD
}
if ($SIGN_TOOL_CMD) {
    $ISCC_ARGS += '/DSIGN_TOOL=1'
    $ISCC_ARGS += "/Scodesign=$SIGN_TOOL_CMD"
}
& ISCC @ISCC_ARGS
if (-Not $?) {
    Write-Error "Failed to build $APP_NAME installer"
    exit 1
}

# If this is being run within a GitHub action, set an output variable with the
# location of the installer so it can be referenced by subsequent actions.
if ($env:GITHUB_ACTIONS -eq 'true') {
    Write-Output '::echo::on'
    $INSTALLER_PATH = $INSTALLER_PATH -replace '\\', '/'
    "installer_path=$INSTALLER_PATH" >> "$env:GITHUB_OUTPUT"
    "pdb_file_path=$PDB_PATH" >> "$env:GITHUB_OUTPUT"
    Write-Output '::echo::off'
}
