# 1. Installation and first run

Phosphor is a desktop terminal emulator you download as an ordinary installer:
a `.dmg` on macOS, an `.exe` installer on Windows, an AppImage or a distro
package on Linux. There is no account to create, nothing to sign into, and no
setup wizard — the first launch drops you straight into a window with a shell.
This section covers which builds exist, how to install them, how the release
channels and the updater work, and what you actually see the first time you run
it.

---

## Which platforms have builds

The release pipeline (`.github/workflows/phosphor_release.yml`) builds exactly
four targets:

| target | runner | what it produces |
|---|---|---|
| macOS arm64 (Apple Silicon) | `macos-26` | `Phosphor-arm64.dmg` |
| macOS x86_64 (Intel) | `macos-26-intel` | `Phosphor-intel.dmg` |
| Windows x64 | `windows-latest` | `PhosphorSetup.exe` |
| Linux x86_64 | `ubuntu-22.04` | AppImage, `.deb`, `.rpm`, Arch package |

Plus command-line artifacts on the same targets (see the asset table below).

### What upstream builds that Phosphor does not

- **No web build.** Warp ships a browser client; Phosphor's pipeline drops it.
- **No universal ("fat") macOS binaries.** macOS gets one single-architecture
  DMG and one single-architecture CLI tarball per architecture, so pick the
  right one for your Mac. Apple Silicon → `arm64`; Intel → `intel`.
- **No Windows arm64.** Only x64 is built. On an arm64 Windows machine you
  would be relying on x64 emulation, which is not tested here.
- **No Linux aarch64 — of anything.** Neither the GUI nor the CLI is published
  for `linux-aarch64`, and that gap is enforced rather than papered over: the
  remote-server installer has no pinned checksum for that platform and
  therefore refuses to install onto it rather than installing something
  unverified.

### Every build is unsigned. This matters on first launch.

Phosphor does not have an Apple Developer ID certificate or a Windows
code-signing certificate, and the pipeline deliberately does not pass signing
credentials (`--read-passwords-from-env` on macOS, `SIGN_TOOL_CMD` on Windows).
Practically:

**macOS.** The `.app` inside the DMG is *ad-hoc* signed (`--selfsign`, falling
back to the `-` identity). Ad-hoc signing is what gives the bundle a cdhash,
which macOS requires before it will register Phosphor in
**System Settings → Notifications** — without it, desktop notifications fail
silently. But ad-hoc signing is not notarization, so Gatekeeper still blocks the
first launch with its "unidentified developer" dialog (the exact wording varies
by macOS version). Two ways past it:

```bash
# Option A — after dragging Phosphor.app into /Applications:
xattr -rd com.apple.quarantine /Applications/Phosphor.app
```

Option B: right-click (or Control-click) `Phosphor.app` in Finder, choose
**Open**, then confirm **Open** in the dialog. macOS remembers the exception.

**Windows.** The installer and the binary are both unsigned, so SmartScreen
interposes a "publisher unknown" warning on the installer. Expand **More info**
and choose **Run anyway**. Third-party antivirus is also more likely to
quarantine an unsigned installer than a signed one; if the download vanishes,
check its quarantine list before re-downloading.

**Linux.** No signing expectation exists in the first place. The `.deb`/`.rpm`
are not served from a signed apt/dnf repository — you install the downloaded
file directly.

---

## How do I install it?

Download from **https://github.com/jwp2987/phosphor/releases**.

### macOS

1. Download `Phosphor-arm64.dmg` (Apple Silicon) or `Phosphor-intel.dmg`
   (Intel).
2. Open the DMG and drag **Phosphor** into **Applications**.
3. Clear the quarantine flag or right-click → Open (see above).
4. Launch Phosphor from Applications or Spotlight.

### Windows

1. Download `PhosphorSetup.exe`.
2. Run it, click through the SmartScreen warning, and follow the Inno Setup
   installer.
3. Launch **Phosphor** from the Start menu.

### Linux

Four package formats are published. Pick one:

**AppImage** — works anywhere, no install step:

```bash
chmod +x Phosphor-x86_64.AppImage
./Phosphor-x86_64.AppImage
```

**Debian / Ubuntu:**

```bash
sudo apt install ./phosphor_<version>_amd64.deb
```

**Fedora / RHEL** (needs RPM 4.13+, i.e. Fedora 26+ / RHEL 8+ — the spec uses
boolean dependency syntax):

```bash
sudo dnf install ./phosphor-<version>-<release>.x86_64.rpm
```

**Arch:**

```bash
sudo pacman -U phosphor-<version>-<release>-x86_64.pkg.tar.zst
```

After installing a package, the launch command is **`phosphor`**, and a
**Phosphor** entry appears in your application menu. Note the deliberate split:
the *installed binary* is `phosphor-oss`, and `/usr/bin/phosphor` is always a
symlink (or, on Arch, a wrapper script) pointing at it. Type `phosphor`.

The desktop entry also registers Phosphor as the handler for `phosphor://`
URLs, and keeps `zap://` registered so links made before the rename still open.

> **glibc floor.** The Linux GUI is built on Ubuntu 22.04 (glibc 2.35)
> specifically so it runs on older distros — roughly Debian 11 / Ubuntu 20.04
> and newer. The *CLI* tarball is a static musl build with no glibc dependency
> at all, so it runs on anything, including Alpine.

### Which Linux packages are guaranteed?

Only the **AppImage** and the **`.deb`** are load-bearing: if either fails to
build, the whole release fails and nothing is published. The **`.rpm`** and the
**Arch package** are best-effort (`continue-on-error: true` on both steps) — a
toolchain failure in either one is allowed through so it cannot block the
release. If a release page is missing an `.rpm` or a `.pkg.tar.zst`, that is
why; use the AppImage.

### Building your own

`script/bundle` is the same entry point CI uses; it dispatches to
`script/macos/bundle`, `script/linux/bundle`, or `script/windows/bundle.ps1`
based on your OS. The Linux form:

```bash
GIT_RELEASE_TAG=v0.0.0-local script/bundle --channel oss --arch x86_64 --packages appimage,deb
```

Useful flags: `--channel <oss|dev|…>`, `--arch <x86_64|aarch64>`,
`--packages <appimage,deb,rpm,arch,none>`, `--artifact <app|cli>`,
`--check-only` (runs `cargo check` with the production feature set and stops),
`--skip-build` (package an already-built binary), `--release-tag <tag>`.

---

## Release assets, in full

A completed release attaches all of these:

| asset | platform | what it is |
|---|---|---|
| `Phosphor-arm64.dmg` | macOS Apple Silicon | GUI app |
| `Phosphor-intel.dmg` | macOS Intel | GUI app |
| `PhosphorSetup.exe` | Windows x64 | GUI app installer (Inno Setup) |
| `Phosphor-x86_64.AppImage` | Linux x86_64 | GUI app, portable |
| `phosphor_<ver>_amd64.deb` | Linux x86_64 | GUI app, Debian package |
| `phosphor-<ver>-<rel>.x86_64.rpm` | Linux x86_64 | GUI app, RPM (best-effort) |
| `phosphor-<ver>-<rel>-x86_64.pkg.tar.zst` | Linux x86_64 | GUI app, Arch (best-effort) |
| `phosphor-cli-linux-x86_64.tar.gz` | Linux x86_64 | CLI / remote-server (static musl) |
| `phosphor-cli-macos-aarch64.tar.gz` | macOS arm64 | CLI / remote-server |
| `phosphor-cli-macos-x86_64.tar.gz` | macOS Intel | CLI / remote-server |
| `phosphor-tui-linux-x86_64.tar.gz` | Linux x86_64 | terminal-UI front-end |
| `phosphor-tui-macos-aarch64.tar.gz` | macOS arm64 | terminal-UI front-end |
| `phosphor-tui-macos-x86_64.tar.gz` | macOS Intel | terminal-UI front-end |
| `phosphor-tui-windows-x64.zip` | Windows x64 | terminal-UI front-end |

The CLI tarballs are not just a convenience download: the GUI client is
compiled with the SHA-256 of every published CLI tarball baked in, and the
SSH remote-server installer verifies the remote download against those pinned
digests before executing anything. That is why a release either has all the CLI
tarballs or has no release page at all.

---

## Release channels

`.github/workflows/release_configurations.json` defines exactly **two**
channels, and they differ only in how the GitHub Release is labelled — not in
what gets compiled. Every build is the `oss` application channel regardless.

| channel | prerelease? | release name | when you get it |
|---|---|---|---|
| `oss` | no | `Phosphor <tag>` | tags that do **not** contain `beta` |
| `beta` | yes | `Phosphor Beta <tag>` | tags containing `beta` (case-insensitive) |

The selection is mechanical: on a tag push, if the tag name matches `beta`
case-insensitively (e.g. `v2026.08.29.1-beta`) the release is flagged as a
**prerelease** and is *not* marked "Latest"; anything else is a normal release
and takes "Latest". A manually dispatched run picks the channel from a dropdown
and can never claim "Latest", whatever you choose.

**What to expect from each, as a user:**

- **`oss` (stable).** The normal download. It appears at
  `/releases/latest`, so it is what the GitHub UI offers by default and what
  the built-in updater sees.
- **`beta` (prerelease).** Its own release body says it plainly: *"pre-release,
  not fully vetted."* You have to go find it on the releases page — GitHub does
  not surface prereleases as "Latest". **The updater will never move you onto a
  beta**, because the OSS updater reads only the `/releases/latest` endpoint.
  If you install a beta, you stay on it until you manually download something
  newer.

> Phosphor's own README describes the whole project as being in beta: settings,
> storage layout and provider plumbing have changed between releases before.
> Read the release notes before upgrading.

---

## Autoupdate

Two entirely separate updaters exist, and they cover different things.

### The GUI updater (`AutoupdateSettings`)

It polls `https://api.github.com/repos/jwp2987/phosphor/releases/latest` — the
same GitHub Release the pipeline publishes — every **10 minutes** while the app
is running, plus a check each time the app is reactivated from the background.
When a newer version exists it downloads the installer for your platform into a
local cache, verifies its SHA-256 against the digest GitHub published for that
asset, and then stops. **It never replaces the running application by itself.**

- **macOS:** the verified `.dmg` is re-hashed a second time at install time and
  then handed to Finder via `/usr/bin/open` after Phosphor exits. You drag the
  new app into `/Applications` yourself. The download lives under
  `~/Library/Application Support/dev.phosphor.Phosphor/autoupdate/<update-id>/`
  — not `~/Downloads`.
- **Windows:** the downloaded installer is executed with Inno Setup switches
  `/SP- /NORESTART /LOG /update=1 /NOCLOSEAPPLICATIONS /DIR=…`. `/SILENT` is
  deliberately omitted on this channel, so you see the normal install UI and
  can cancel.
- **Linux: there is no GUI updater at all.** The `autoupdate` Cargo feature is
  compiled into the shipped macOS and Windows bundles, but *not* into the Linux
  bundle. On Linux, update by downloading the new AppImage or package. (Check
  what you are running under Settings → About, or `phosphor --version`.)

Verification is fail-closed by design: if a release publishes an asset with no
usable `sha256` digest, or the bytes do not match, the update is **refused**
and Phosphor raises an error banner with an **"Update Phosphor manually"**
button rather than silently going quiet. Be honest about what that buys you,
though: the digest arrives over the same TLS connection as the download URL, so
it catches corruption and a misbehaving CDN edge — it is not supply-chain
integrity, because nothing signs these artifacts with a key held outside
GitHub.

**Where the controls are — and where they aren't.** The About page's
"Automatic updates" switch and update-status row are **hidden in this build**
(`SHOW_AUTOUPDATE_UI` is compiled to `false`), and the "Check for updates" menu
entries are hidden too, because the `oss` channel ships no `autoupdate_config`.
What remains reachable are two command-palette commands, on macOS and Windows:

- **Check for updates** (`workspace:check_for_updates`)
- **Install update and relaunch** (`workspace:update_and_relaunch`)

Neither has a default key binding; open the command palette and type the name.

### The TUI updater (`TuiAutoupdateSettings`)

`phosphor-tui` has its own background updater, modelled on how CLI tools like
Claude Code self-update: a versioned install tree with a `current` symlink,
refreshed in the background and picked up on the next launch.

**On Phosphor it never runs.** It only activates for a "managed install" — a
binary living inside a `versions/<version>/` directory laid down by an install
script — and Phosphor publishes the TUI as a plain tarball with no such script.
Even if the layout existed, the version lookup explicitly refuses the `oss`
channel: *"no TUI release artifacts exist for the oss channel."* Update the TUI
by downloading the new tarball. The setting below is documented because it
exists and is honoured; it currently has nothing to gate.

### Every autoupdate option

Both settings live in `settings.toml`:

| platform | path |
|---|---|
| macOS | `~/.phosphor/settings.toml` |
| Linux | `~/.config/phosphor/settings.toml` |
| Windows | under `%LOCALAPPDATA%\phosphor\Phosphor\config\` |

| setting | TOML path | type | default | what it does | where else it is set |
|---|---|---|---|---|---|
| `automatic_updates_enabled` | `updates.automatic_updates_enabled` | bool | `true` | Whether Phosphor automatically checks for and downloads updates in the background. Consulted for the 10-minute poll and the daily/on-activate check; a *manual* check ignores it. | Nowhere in the UI — the About-page toggle is compiled out. Edit `settings.toml`. |
| `autoupdate_enabled` (TUI) | `general.autoupdate_enabled` | bool | `true` | Whether `phosphor-tui` installs updates in the background. Read **once at TUI startup**, so a change takes effect on the next launch. | Edit `settings.toml`. |

Neither setting is ever synced anywhere (`SyncToCloud::Never`); there is nowhere
to sync it to.

### How do I turn updates off?

**GUI (macOS/Windows):** add to `settings.toml`

```toml
[updates]
automatic_updates_enabled = false
```

Background polling and downloading stop. The two command-palette commands still
work if you want to check by hand. On Linux there is nothing to turn off.

**TUI:** either

```toml
[general]
autoupdate_enabled = false
```

or, for one launch only, set the environment variable:

```bash
WARP_TUI_DISABLE_AUTOUPDATE=1 phosphor-tui
```

Any value works; only its presence is checked.

---

## First run: what you actually see

**There is no onboarding.** No slides, no wizard, no sign-in, no "choose your
plan". Phosphor opens a window and you are in.

Concretely, first launch gives you a window with a single **New tab** pane
showing the Phosphor mark and a small palette with two entries:

- **Terminal session** — opens a shell in a new tab.
- **Add repository** (`cmd-shift-N` on macOS, `alt-n` on Linux/Windows) — opens
  a folder picker; choosing a directory opens a session rooted there.

Settings is reachable from the same palette. Pick **Terminal session** and you
have a working prompt — your normal login shell, in your normal working
directory.

That is the whole first run. If you are coming from Warp and are waiting for a
sign-in step: it does not exist and is not being skipped. The `oss` build has no
authentication path at all — the internal "is logged in" check is hardcoded to
`true` and the local user is created already marked as onboarded, so every
account-gated branch, including the onboarding deck, is unreachable by
construction.

### The onboarding slide deck, for the record

The slide code is still in the tree (`crates/onboarding/`) and, if you force it
open in a debug build (`root_view:enter_onboarding_state`, which refuses in
release builds), the sequence is:

**Intro → Intention → Customize → Agent → Third-party → Theme picker**

The **Agent** slide is skipped if you pick the *Terminal* intention on the
Intention slide. Notably, the pinned upstream sequence also contains
`AiSetup`, `AiAccess` and `PostAuthOffer` steps — the account-first / paid-tier
redesign, including the "offer" slide. Those steps do not exist here at all.

You are not missing anything by not seeing this deck: everything it configures
(agent autonomy, the tools panel, vertical tabs, theme) is in Settings.

### Getting AI working

Phosphor is bring-your-own-provider. Nothing about installation configures a
model — no key ships with the build and none is fetched. Until you add a
provider key in Settings, the terminal works fully and the agent does not. See
the AI/providers section of this manual.

### Coming from Zap or OpenWarp?

The storage identity changed on 2026-08-14 with **no automatic migration**.
A pre-rename build stored everything under a `zap` identity; this one uses
`phosphor`. On first launch you start fresh: no settings, no history, no saved
API keys. Nothing is deleted — the old directories are left in place. See
`docs/migrate-from-warp.md` and the README's migration table for the hand-copy
procedure. API keys cannot be copied that way, because OS keychain entries are
keyed by service name; re-enter those.

---

## Checking your version

**In the app:** Settings → **About**. The page shows the Phosphor mark, the
channel display name (`Phosphor`), and the version, with a copy button beside
it. The same page carries the licence summary, the AGPL §13 source-code link,
and **Export logs…**, which bundles recent app logs (plus MCP and update logs
when present) and a diagnostic summary into a zip — that is what to attach to a
bug report.

**On the command line:**

```bash
phosphor --version      # Linux, via the /usr/bin/phosphor symlink
phosphor-oss --version  # the real binary name on every platform
```

On Windows the packaged executable is built as a GUI subsystem app, so it has
no console to print to — use Settings → About there instead.

### What the version string actually is

The version is **the git release tag the build was cut from**, injected as
`GIT_RELEASE_TAG` at compile time — for example `v2026.08.29.1-beta`. It is
*not* a semantic version, and it is not derived from a version field in the
source. If `GIT_RELEASE_TAG` was not set at build time (any local `cargo build`
or `cargo run`), the About page shows **`Dev`** and `--version` prints
**`<unknown>`**.

**A wrinkle worth knowing.** The Cargo package version is currently `0.1.2`
(`app/Cargo.toml`), and release *tags* use a dated scheme
(`v2026.08.29.1-beta`). These are two different numbering systems and neither
is derived from the other. Nothing a user sees reports `0.1.2` — the About page
and `--version` both show the tag. Quote the tag when reporting a bug; it is
what identifies the build.

---

## Not available in Phosphor

Things a Warp user will go looking for during installation and first run, and
will not find:

| what | why |
|---|---|
| **Sign-in / login / "Continue with Google"** | There is no cloud backend to authenticate against. The sign-in UI was physically removed; the internal auth state is a local placeholder that always reports "logged in". |
| **Account-first onboarding** | Declined — `DECLINED.md`, "Account-first onboarding, billing, paid tiers" (#11). `account_class`, `is_paid`, `has_team` and the upgrade flows have no BYOP equivalent. |
| **Paid tiers, credits, billing, the post-auth "offer" slide** | Same row (#11). The onboarding steps that presented them (`AiAccess`, `PostAuthOffer`) do not exist in this tree. |
| **Teams / organisations / an org+email row in the status menu** | Declined (#389): with no account there is no truthful value to render, so the fields were removed outright rather than left blank. |
| **AI that works out of the box** | Deliberate: keys are yours. Add a provider in Settings after installing. |
| **A signed / notarized macOS build; a signed Windows installer** | No certificates. Expect Gatekeeper and SmartScreen prompts (see above). |
| **A signed apt/dnf repository** | Packages are downloaded and installed by hand from the release page; there is no repo to add. |
| **Web build, universal macOS binary, Windows arm64, Linux aarch64** | Not built by this pipeline. |
| **The `InitProject` first-run wizard** | Declined — `DECLINED.md` (#11 and the `b0b1faef9` row). Its one durable local capability is superseded by the `/init` prompt, which works in both GUI and TUI. |

<!-- SOURCES
Platforms / pipeline shape:
- .github/workflows/phosphor_release.yml:1-31 (header: only macOS arm64 + macOS Intel + Windows x64 + Linux x86_64; Web/CLI-universal/Win-arm64 dropped; unsigned; single-arch DMG + CLI tarball per arch)
- .github/workflows/phosphor_release.yml:250-258 (macOS matrix: aarch64/macos-26, x86_64/macos-26-intel; dmg_name_suffix arm64/intel)
- .github/workflows/phosphor_release.yml:370 (release_windows runs-on: windows-latest)
- .github/workflows/phosphor_release.yml:438-450 (release_linux runs-on: ubuntu-22.04; glibc 2.35 rationale, Debian 11 / Ubuntu 20.04+)
- .github/workflows/phosphor_release.yml:698 (build_cli_linux runs-on: ubuntu-22.04)
- .github/workflows/phosphor_release.yml:842-850 (build_cli_macos matrix)
- .github/workflows/phosphor_release.yml:277-290, 508-524 (CLI digest pinning; "No linux-aarch64 CLI is published ... fails closed")
- .github/workflows/phosphor_release.yml:762-772 (static musl CLI: runs on any Linux x86_64 incl. Alpine/CentOS 7)

Unsigned / Gatekeeper / SmartScreen:
- .github/workflows/phosphor_release.yml:9-11 (Unsigned: no --read-passwords-from-env / no SIGN_TOOL_CMD)
- .github/workflows/phosphor_release.yml:332-352 (--selfsign ad-hoc; cdhash needed for UNUserNotificationCenter; "Users will still hit Gatekeeper's 'unidentified developer' dialog"; `xattr -rd com.apple.quarantine` workaround, issue #51)
- .github/workflows/phosphor_release.yml:376-383 ("Build binary (unsigned)" / "Bundle app (unsigned installer)")

Artifact names:
- script/macos/bundle:417-421 (DMG_NAME / FINAL_DMG_NAME = "$WARP_APP_NAME-$DMG_NAME_SUFFIX.dmg")
- script/macos/bundle:338,348 (WARP_APP_NAME="Phosphor" on oss)
- script/windows/bundle.ps1:52-57 (FILE_ENDING Setup / Setup-arm64), :113-114 (APP_NAME='Phosphor', BINARY_NAME='phosphor-oss.exe'), :141-142 (INSTALLER_NAME = "$APP_NAME$FILE_ENDING"; INSTALLER_PATH .exe)
- script/linux/bundle:198-201 (oss: WARP_BIN/BINARY_NAME=phosphor-oss, APP_NAME=Phosphor), :240 (APPIMAGE_NAME="$APP_NAME-$BUILD_ARCH.AppImage")
- script/linux/bundle_deb:19-27 (oss: PACKAGE_NAME=phosphor / phosphor-cli, OPT_DIR=/opt/phosphor), :59-63 (ARCH=amd64 on x86_64), :122 (${PACKAGE_NAME}_${VERSION}_${ARCH}.deb)
- script/linux/bundle_rpm:22-27, :103 (RPM_NAME="$PACKAGE_NAME-$VERSION-$RELEASE.$ARCH.rpm"), :100 (rpmbuild)
- script/linux/bundle_arch:22, :113 (${PACKAGE_NAME}-${PKGVER}-${RELEASE}-${ARCH}.pkg.tar.zst)
- .github/workflows/phosphor_release.yml:659-666 (RPM: "install with RPM 4.13+ on Fedora 26+ / RHEL 8+ (the spec uses boolean dep syntax)")
- .github/workflows/phosphor_release.yml:667-676 (rename zap_*/zap-* -> phosphor_*; already phosphor for oss, so a no-op leftover)
- .github/workflows/phosphor_release.yml:405-436 (Windows TUI zip phosphor-tui-windows-x64.zip)
- .github/workflows/phosphor_release.yml:786-830 (Linux CLI + TUI tarball names)
- .github/workflows/phosphor_release.yml:880-914 (macOS CLI + TUI tarball names)
- .github/workflows/phosphor_release.yml:640-657, 662-664 (rpm + arch steps both continue-on-error: true)
- .github/workflows/phosphor_release.yml:916-972 (publish_release needs all build jobs; files: dist/*)

Linux launch command / desktop entry:
- app/channels/oss/dev.phosphor.Phosphor.desktop (Name=Phosphor, Exec=phosphor %U, MimeType phosphor:// + zap://, and the comment explaining /usr/bin/phosphor is always a symlink to phosphor-oss)
- script/linux/bundle_appimage:68-72 (AppImage rewrites Exec= to the binary)

script/bundle:
- script/bundle:1-33 (OS dispatch)
- script/linux/bundle:38-121 (--debug/--check-only/--skip-build/--channel/--release-tag/--packages/--artifact/--arch/--target)
- script/linux/bundle:330-351 (appimage/deb/rpm/arch dispatch)

Channels:
- .github/workflows/release_configurations.json (exactly two channels: oss is_prerelease false / "Phosphor"; beta is_prerelease true / "Phosphor Beta" + "pre-release, not fully vetted")
- .github/workflows/phosphor_release.yml:94-110 (tag matching [Bb][Ee][Tt][Aa] -> beta; else oss; dispatch uses the input)
- .github/workflows/phosphor_release.yml:36-50 (workflow_dispatch release_channel input; app always builds as oss Channel)
- .github/workflows/phosphor_release.yml:951-970 (prerelease + make_latest logic; only a tag-triggered non-prerelease run may take Latest; "/releases/latest, the only endpoint the OSS updater reads")
- README.md:14-27 (project described as beta; read release notes before upgrading)

GUI autoupdate:
- app/src/settings/autoupdate.rs:3-13 (AutomaticUpdatesEnabled: bool, default true, SupportedPlatforms::DESKTOP, SyncToCloud::Never, private false, toml_path "updates.automatic_updates_enabled", description)
- app/src/autoupdate/github.rs:14-15 (REPO_OWNER jwp2987 / REPO_NAME phosphor), :102-104 (GET /repos/{owner}/{repo}/releases/latest)
- app/src/autoupdate/mod.rs:270-290 (register: gated on FeatureFlag::Autoupdate + can_autoupdate; poll loop + on-activate DailyCheck)
- app/src/autoupdate/mod.rs:304-326 (ManualCheck bypasses the setting; Poll/DailyCheck consult automatic_updates_enabled)
- app/src/autoupdate/mod.rs:334 (AUTOUPDATE_POLL = 10 min)
- app/src/autoupdate/mod.rs:113-190 (verify_oss_asset_sha256 doc: fail-closed, UpdateBlocked -> UnableToUpdateToNewVersion banner with "Update Phosphor manually"; explicit statement that this is not supply-chain integrity)
- app/src/autoupdate/mod.rs:1053-1066 (Channel::Oss short-circuits fetch_version to the GitHub release)
- crates/warp_core/src/execution_mode.rs:76-78 (can_autoupdate == is_app)
- crates/warp_features/src/lib.rs / app/src/lib.rs:2926-2928 (FeatureFlag::Autoupdate only under #[cfg(feature = "autoupdate")])
- app/Cargo.toml:480-661 ("autoupdate" absent from `default`; "autoupdate_ui_revamp" at :830 is a different feature and IS in default)
- script/macos/bundle:345-358 (oss: FEATURES="release_bundle,extern_plist,autoupdate"; DMG downloaded to cache_dir/autoupdate/<update_id>/, NOT ~/Downloads; `open <dmg>` after exit, user drags to /Applications)
- script/windows/bundle.ps1:117-125 (oss: FEATURES includes autoupdate; installer to a tempfile; Inno invoked non-/SILENT so the user sees the wizard and can cancel)
- script/linux/bundle:198-203, 219-222 (oss: FEATURES="release_bundle" then ",gui,nld_improvements" — no autoupdate anywhere in this script)
- app/src/autoupdate/windows.rs:449-466 (Oss Inno switches /SP- /NORESTART /LOG /update=1 /NOCLOSEAPPLICATIONS /DIR=, no /SILENT)
- app/src/autoupdate/mac.rs:425-455, 483-497 (oss_open_installer: re-hash then `exec /usr/bin/open $dmg` after this pid exits), :1206-1224 (dmg_name hardcodes Phosphor-arm64.dmg / Phosphor-intel.dmg on Oss)
- app/src/autoupdate/mac.rs:224 + crates/warp_core/src/paths.rs:266-279 (download dir = cache_dir()/autoupdate; on macOS cache_dir() is project_dirs.data_dir())
- DECLINED.md:180 (the corrected row: the Cargo feature is the real gate; shipped macOS/Windows bundles DO enable autoupdate; the release workflow does publish the feed)

Autoupdate UI hidden:
- app/src/settings_view/about_page/mod.rs:61-66 (SHOW_AUTOUPDATE_UI = false), :160-163 (autoupdate search terms omitted), :233-243 and :344-370 (both UI blocks gated on it)
- app/src/bin/phosphor_oss.rs:39 (autoupdate_config: None) + crates/warp_core/src/channel/state.rs:229-237 (show_autoupdate_menu_items() -> unwrap_or_default() == false)
- app/src/workspace/view.rs:6907 and :8692-8695 (menu items gated on show_autoupdate_menu_items)
- app/src/resource_center/main_page.rs:533 (version row gated the same way)
- app/src/workspace/mod.rs:1169-1188 (workspace:update_and_relaunch / workspace:check_for_updates registered under FeatureFlag::Autoupdate only, no default keybinding; ContextFlag::PromptForVersionUpdates)
- crates/warp_core/src/context_flag.rs:35-37 (all ContextFlags default to enabled)
- app/i18n/en/warp.ftl:1972-1973 ("Install update and relaunch", "Check for updates"), :232 ("Update Phosphor manually"), :645-646 (Automatic updates label/description)

TUI autoupdate:
- app/src/settings/tui_autoupdate.rs:8-21 (TuiAutoupdateEnabled: bool, default true, SyncToCloud::Never, toml_path "general.autoupdate_enabled", description)
- crates/warp_tui/src/autoupdate.rs:1-27 (versioned install layout; managed installs only; opt-out via setting or env var)
- crates/warp_tui/src/autoupdate.rs:49 (DISABLE_ENV_VAR = "WARP_TUI_DISABLE_AUTOUPDATE"), :281-308 (determine(): env var, then setting, then version tag, then platform, then InstallLayout::detect)
- crates/warp_tui/src/autoupdate.rs:105-110 (detect() returns None for cargo-run / flat installs)
- crates/warp_tui/src/autoupdate.rs:698-702 (tui_version_for_channel bails: "no TUI release artifacts exist for the {channel} channel" for Local | Oss | Integration)

Settings file location:
- app/src/settings/mod.rs:653 (config_local_dir().join("settings.toml"))
- crates/warp_core/src/paths.rs:113-126 (macOS oss config dir ".phosphor"), :146-158 (config_local_dir), :298-341 (Linux app name "phosphor"; ProjectDirs::from(qualifier, org, app) for the rest)
- app/src/settings/init.rs:539-570 (SettingsFile flag -> TOML-backed public prefs) + app/Cargo.toml default list contains "settings_file"

First run / no onboarding:
- app/src/auth/mod.rs:294-296 (is_logged_in() -> true), :298-301 (is_anonymous_or_logged_out() -> false), :204-221 (User::test(), is_onboarded: true, "Zap uses this user on every code path"), :456-459 (reset_local_defaults)
- app/src/root_view.rs:1478-1515 (is_logged_in branch -> AuthOnboardingState::Terminal; the pre-login onboarding branch is unreachable)
- app/src/root_view.rs:1786-1803 (debug_enter_onboarding_state; refuses unless ChannelState::enable_debug_features()), :387 ("root_view:enter_onboarding_state")
- crates/warp_core/src/channel/state.rs:84-86 (enable_debug_features = debug_assertions || Local/Dev channel)
- app/src/workspace/view.rs:6990-7010 (should_trigger_get_started_onboarding: false once is_onboarded), :3883-3900 (falls through to add_welcome_tab when WelcomeTab is enabled)
- app/Cargo.toml (default features include "welcome_tab", "agent_onboarding", "open_warp_new_settings_modes", "get_started_tab")
- app/src/pane_group/pane/welcome_view.rs:34-53 (workspace:new_tab "Terminal session"; welcome_view:open_project "Add repository" with cmd-shift-N / alt-n), :86-89 (workspace:show_settings also shown), :135-170 (create_terminal_session / open_project folder picker)
- app/i18n/en/warp.ftl:2246-2247 ("Terminal session", "Add repository")
- app/src/workspace/one_time_modal_model.rs:34-60, 262-289 (launch modal only triggers off AuthManagerEvent::AuthComplete) + app/src/auth/mod.rs:788-795, 818-822 (AuthComplete only emitted by create_anonymous_user / set_user_onboarded)

Onboarding slide sequence:
- crates/onboarding/src/model.rs:120-129 (OnboardingStep: Intro, Intention, Customize, Agent, ThirdParty, Project, ThemePicker)
- crates/onboarding/src/model.rs:624-664 (next(): with ZapNewSettingsModes on -> Intro -> Intention -> Customize -> {Agent | ThirdParty} -> ThirdParty -> ThemePicker)
- app/src/lib.rs:3271-3272 (ZapNewSettingsModes under feature "open_warp_new_settings_modes", which is in `default`)
- crates/onboarding/src/model.rs:1-14 (the pin's AiSetup / AiAccess / PostAuthOffer steps, AiAccessChoice::Subscription and the AccountFirstOnboarding gate are DECLINED per DECLINED.md #11; none of these types exist in this crate)

Version:
- crates/warp_core/src/channel/state.rs:213-227 (app_version() from GIT_RELEASE_TAG via option_env!)
- crates/warp_cli/src/lib.rs:221-223 (command.version(version_string())), :523-530 (version_string() -> app_version().unwrap_or("<unknown>"))
- app/src/settings_view/about_page/mod.rs:184 (ChannelState::app_version().unwrap_or("Dev")), :190-210 (version text + copy button)
- app/Cargo.toml:8 (version = "0.1.2"); git tag list shows dated tags e.g. v2026.08.29.1-beta
- app/src/bin/phosphor_oss.rs:3 (windows_subsystem = "windows" under feature "release_bundle"), :30-31 (app_id dev.phosphor.Phosphor, display_name "Phosphor")
- app/i18n/en/warp.ftl:640-644, 659-662 (About page copyright / licence / source-code link / Export logs strings)
- app/src/settings_view/about_page/mod.rs:144 (SOURCE_CODE_URL github.com/jwp2987/phosphor)

Not available:
- DECLINED.md:85 (Account-first onboarding, billing, paid tiers — #11)
- DECLINED.md:228 (status-menu org/email fields dropped — #389)
- DECLINED.md:214, :222 (InitProject wizard declined; /init supersedes it)
- README.md:44-58, :162-183 (BYOP, no mandatory cloud, "Not included, on purpose")
- README.md:189-232 (storage identity rename 2026-08-14, no migration, hand-copy procedure, keychain caveat)
-->
