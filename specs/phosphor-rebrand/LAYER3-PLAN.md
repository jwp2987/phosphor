# Layer 3 — storage / system identity rename (`dev.zap.Zap` → Phosphor)

Execution plan for **option B** in `SCOPE.md` (full deep rename + migration).
`SCOPE.md` scopes the milestone; this plans it. Read `SCOPE.md` layer 4 first —
crate names, module paths and `WARP_*` env vars are **explicitly out of scope**
and must not be touched.

**Status: IN PROGRESS, 2026-08-13.** Split across four parallel agent
branches, none merged to `main` yet:

| branch | owns |
|---|---|
| Rust identity/migration | `AppId::new` sites, `paths.rs` (incl. the Linux directory-name special case), `channel/mod.rs`/`channel/state.rs`, `app/build.rs`, `crates/remote_server` (incl. `remote_server_dir()`), the secure-storage migration itself, and every other `.rs` file touching the app id or derived paths |
| Linux packaging | `resources/linux/{debian,rpm,arch}/**`, `script/linux/**`, `app/channels/oss/*.desktop`, `app/channels/oss/icon/**` |
| Windows+macOS packaging | `script/windows/**`, `script/macos/**`, `*.iss`, `*.ps1`, `app/DockTilePlugin/**`, the macOS-bundle lines of `app/Cargo.toml` |
| Documentation + completeness inventory (this branch) | This file's status, `SCOPE.md`'s layer-3 corrections, and `specs/phosphor-rebrand/RENAME-INVENTORY.md` — an exhaustive checklist of every remaining old-identity occurrence, for verifying the other three branches' completeness once they merge |

**None of this has been compiled.** Builds (`cargo check`/`build`/`test`,
`script/precheck`) were prohibited for every agent in this round — the
maintainer was AFK and the build host OOM-reaps under concurrent cargo
invocations. Every plan, correction, and inventory entry below and in
`RENAME-INVENTORY.md` is a **static-analysis / grep-verified** claim, not a
build-verified one. The integration branch that merges all four must run
`script/precheck` and the full suite before anything here is trusted as
working, per `docs/FLEET-ROUND.md`'s batched-verification model. Do not treat
any statement in this file as describing a *working* tree — only a *planned
or landed-but-unverified* one.

## Why now, and the one number that decides it

Migration cost scales with the number of real installs. Today that is
approximately **one** — the maintainer's Linux laptop, carrying a ~14 MB
`warp.sqlite`, a `settings.toml`, and Secret Service entries under service
`dev.zap.Zap`. Every additional install multiplies the blast radius of a
migration bug. `SCOPE.md` is right that this is its own milestone and that you
**never do it twice**; it is also the cheapest it will ever be.

---

## 1. What actually derives from the app id

This is the finding that makes the milestone tractable: **on the Rust side,
almost everything flows from one value.**

`AppId` is set in two places, and read everywhere:

| site | value |
|---|---|
| `crates/warp_core/src/channel/state.rs:40` | `AppId::new("dev", "zap", "Zap")` — the default `ChannelConfig` |
| `app/src/bin/zap_oss.rs:29` | `AppId::new("dev", "zap", "Zap")` — the OSS channel's config |
| `crates/warp_core/src/channel/state.rs:275` `app_id_from_bundle()` | **macOS only** — overrides both of the above from the bundle's `CFBundleIdentifier` at runtime |

Everything below is *computed* from it and needs no separate edit:

- **All user data paths.** `paths.rs:254 project_dirs()` → `project_dirs_for_app_id()`
  → `directories::ProjectDirs::from(qualifier, organization, app_name)`. Root of
  `config_local_dir`, `data_dir`, `state_dir`, `cache_dir`, `themes_dir`,
  `secure_state_dir` and the Windows/macOS equivalents.
- **The secret-storage service name.** `ChannelState::data_domain()`
  (`state.rs:121`) is `app_id().to_string()` (plus a `WARP_DATA_PROFILE` suffix in
  debug builds). `app/src/lib.rs:1278` passes it straight into
  `warpui_extras::secure_storage::register*`, which becomes the Secret Service
  `service` attribute on Linux (`secure_storage/linux.rs:26`) and the macOS
  Keychain service name (`secure_storage/mac.rs:14`).

**Correction to `SCOPE.md`, layer 3.** It lists
`app/src/app_services/linux/mod.rs:163` as the "secret storage service name". It
is not — that is the **`org.freedesktop.Application` D-Bus well-known name** used
for single-instance activation. Both strings happen to read `dev.zap.Zap`, so
both change, but they are unrelated mechanisms with different failure modes:
getting the D-Bus name wrong breaks single-instance/activation, getting the
keyring service wrong orphans every stored API key. Fix that bullet in `SCOPE.md`
as part of this work.

## 2. What does NOT derive, and must be hand-edited

Packaging and OS-integration identifiers are independent strings.

**Cross-platform**
- `crates/warp_core/src/channel/mod.rs:45,70` — `Channel::Oss => "zap-oss"` (binary + CLI command name).

**Linux**
- `app/channels/oss/dev.zap.Zap.desktop` — the **filename**, plus `StartupWMClass=dev.zap.Zap`, `Icon=dev.zap.Zap`, `MimeType=x-scheme-handler/zap`. (`Name=` and `Exec=` are already Phosphor — layer 2 landed.)
- `script/linux/bundle:199,200,229` — `WARP_BIN`, `BINARY_NAME`, `BUNDLE_ID`.
- `resources/linux/{debian,rpm,arch}/**` — package names, `/opt/zap`, the `phosphor` symlink, postinst/spec file lists.
- `app/channels/oss/icon/**` — icon asset ids keyed to the desktop `Icon=` value.
- `app/src/autoupdate/linux.rs`, `app/src/settings_view/scripting_page.rs`.

**macOS**
- `app/Cargo.toml:879` `identifier = "dev.zap.Zap"`; `script/macos/bundle:315` `BUNDLE_ID`.
- `app/DockTilePlugin/Info.plist:6` — `dev.zap.ZapDockTilePlugin` (note the **derived** id: it is the app id with a suffix, not an independent name).

**Windows**
- `script/windows/bundle.ps1:113,114` `$WARP_BIN`/`$BINARY_NAME`; `:221` `$INNO_APP_ID = 'zap-oss'`.
- `script/windows/windows-installer.iss:246` `zap-oss.cmd`.
- AUMID `dev.zap.Zap` — `app/build.rs:382,393`, `crates/warpui/src/platform/windows/mod.rs`.

## 3. Decisions required before any code moves

1. **The app id triple — DECIDED 2026-08-13 (maintainer): `dev.phosphor.Phosphor`.**
   Keeps the existing `dev` qualifier, so nothing about the
   signing/notarization assumptions changes shape.
   → `AppId::new("dev", "phosphor", "Phosphor")`.
2. **Package/binary name — DECIDED 2026-08-13 (maintainer): `phosphor-oss`.**
   `Channel::Oss => "phosphor-oss"`. Keeping the `-oss` suffix preserves the
   channel distinction and, usefully, **avoids the symlink collision in §6**:
   `/usr/bin/phosphor` stays a symlink to the `phosphor-oss` binary exactly as it
   is a symlink to `zap-oss` today, so the deb/rpm file-ownership question is a
   package *rename* only, not a change in what kind of object lives at that path.
3. **Copy or move** the user's data (§4). *Still open — recommendation: copy.*
4. **Whether to keep a `zap://` scheme handler** alongside `phosphor://` for
   existing links. *Still open.*

## 4. The migration

**Placement.** Must run before anything reads config or secrets — i.e. before
`secure_storage::register*` at `app/src/lib.rs:1283-1289` and before settings
init. This is the earliest ordering constraint in the milestone and the one most
likely to be got wrong by inserting the call somewhere "reasonable" later.

**Guard.** A marker file in the **new** state dir (e.g. `identity-migrated-v1`).
Condition to run: marker absent **and** new config dir absent **and** old dir
present. Never keyed on "old dir exists" alone — that re-runs forever.

**Files.** Copy `config`, `data`, `state` and `cache` trees from the `zap`
locations to the `phosphor` ones.

**Recommendation: copy, do not move.** A move makes rollback impossible; a copy
leaves the old install intact and working if the user reverts the binary. The
cost is a transient ~14 MB duplicate and the risk of silent divergence if the
user runs both builds afterwards. Mitigate by writing a `MIGRATED-TO` breadcrumb
into the old directory. Revisit only if the data grows large enough that copying
is itself a hazard.

**Secrets are the sharp edge.** They cannot be moved with file operations —
Secret Service and Keychain entries are keyed by the `service` attribute, so
migrating means **read under the old service name, write under the new one**.
This is cheap only because `SecureStorage::new(service_name: &str)` is already
parameterized: construct a second, throwaway instance with `"dev.zap.Zap"`, read
each known key, write it through the live instance, and leave the old entries in
place (so a rollback still finds them).

That requires an **exhaustive list of secure-storage keys** — there is no
enumeration API. Known: `AgentProviderSecrets`
(`app/src/ai/agent_providers/secrets.rs:13`). `ApiKeyManager` has its own; grep
every `SECURE_STORAGE_KEY`-shaped constant and every literal passed to
`write_value`/`read_value` before writing the migration, and add a test that
fails when a new one appears unlisted. A key missed here is an API key silently
lost, with no error at the point of loss.

**Trap — do not touch the fallback encryption key.**
`secure_storage/linux.rs:108` derives the disk-fallback key from the literal
`"zap-local-secure-storage-fallback-key"`. On a host with no Secret Service
provider, secrets are AES-GCM blobs under `state_dir()` encrypted with it.
"Cleaning up the zap strings" by renaming it makes every existing fallback blob
undecryptable. Leave the literal alone; it is not user-visible. Add a comment
saying so, because it looks exactly like a string that should have been renamed.

**Trap — the Linux directory-name special case.** `paths.rs:273-277` maps
`"Zap"` → `zap` and `"Zap*"` → `zap-*` so the data dir matches the Linux package
name. A new app id falls through to `_ => application_name()` and would produce
`~/.config/Phosphor` (capital P). Add a `"Phosphor"` arm or generalize to
lowercasing. Missing this yields a working build with subtly wrong paths — the
failure is cosmetic on first run and permanent thereafter.

## 5. Phasing

Each phase is independently landable and independently revertible.

- **Phase 0 — decisions.** Resolve §3. Fix the `SCOPE.md` secret-storage bullet.
- **Phase 1 — migration machinery, still named `dev.zap.Zap`.** Land the
  key inventory, the migration function, the marker-file guard and their tests
  with the app id **unchanged**, so the migration is a verified no-op in
  production. This is the phase that makes the milestone safe: all the logic
  ships and is tested before anything moves.
- **Phase 2 — flip the app id.** Both `AppId::new` sites, plus the `paths.rs`
  Linux arm. At this point a dev build migrates real data — test with a copied
  `~/.config/zap` first, not the live one.
- **Phase 3 — packaging, one OS per PR.** Linux first (verifiable here), then
  macOS, then Windows. §2 is the checklist.
- **Phase 4 — verification.** §7.

## 6. Traps specific to packaging

- **macOS dual identity.** `app_id_from_bundle()` means a bundled build takes its
  id from `Info.plist` while `cargo run` takes the `state.rs` default. If those
  two disagree, dev and bundled builds silently use different data directories.
  They must be changed in the same commit.
- **Windows uninstall orphaning.** `AppId` in Inno Setup keys the registry
  Uninstall entry and upgrade tracking (`windows-installer.iss:41-47`, and
  `bundle.ps1:216-221` documents that it was already pinned once to avoid exactly
  this). Changing `$INNO_APP_ID` makes the new installer not see the old install:
  the user ends up with two entries and two `/opt`-equivalent trees.
- **deb/rpm will not upgrade across a package rename.** A renamed package needs
  `Conflicts:`/`Replaces:`/`Provides:` (deb) and `Obsoletes:`/`Provides:` (rpm),
  or `apt`/`dnf` treats it as an unrelated package and leaves both installed.
- **The `phosphor` symlink already exists.** Layer 2 shipped `/usr/bin/phosphor`
  as a symlink owned by the `zap` package (see the comment block in the desktop
  file). If the package renames to `phosphor` and installs a *binary* at that
  path, dpkg hits a file-ownership conflict during the upgrade unless the
  `Replaces:` is right. This is the most likely concrete breakage on the
  maintainer's own machine.
- **`WARP_DATA_PROFILE`** appends a suffix to the data domain in debug builds
  (`state.rs:110`). Migration must not fire for profiled instances, or it copies
  the default profile's data into a sandbox.

## 7. Verification

- **Automated:** `script/precheck`, then the full suite. Add tests for
  `project_dirs_for_app_id` (all three OS arms), the migration guard's
  run-once/skip conditions, and the secure-storage key inventory.
- **Manual, Linux (possible here):** build a package, install over the existing
  `zap` install on a machine with real data, confirm settings/history/API keys
  all survive, the launcher and icon still resolve, and `phosphor://` links open.
- **Manual, macOS/Windows: NOT possible on the current hardware.** The maintainer
  has a Linux laptop only. Phase 3's macOS and Windows halves ship
  **unverified against a real install** unless someone tests them. Say so in the
  PR rather than implying coverage — an untested Windows uninstall-entry change
  is exactly the kind of thing that is discovered by a user, once, permanently.

## 8. Rollback

Phases 1 and 3 revert cleanly. Phase 2 does not fully: once a user has run a
migrated build, the new directories exist and are authoritative. Reverting the
binary sends them back to the old `zap` directories, which — because the
migration **copies** — still hold their pre-migration state. Any work done after
migrating is stranded in the new tree, not lost. This is the entire reason for
preferring copy over move.
