# Layer-3 rename — merge checklist

Four branches were produced in one parallel round on 2026-08-13/14, each in its
own worktree, **none of them compiled** (builds were prohibited: the maintainer
was away and this host OOM-reaps under cargo). This file is the cross-branch
contract: the things that are only correct if *several* branches land together.

Read `LAYER3-PLAN.md` for the design and `RENAME-INVENTORY.md` (docs branch) for
the exhaustive occurrence list. This file is only the interdependencies.

| branch | owns | commit |
|---|---|---|
| `worktree-agent-aa4358013d3436219` | Rust identity + migration | (two commits) |
| `worktree-agent-a94f7ebd6c989f458` | Linux packaging | `67e97e17` |
| `worktree-agent-a734023a2efa3819b` | Windows + macOS packaging | `752e4177` |
| `worktree-agent-a2f12d974cad43177` | docs + inventory | `851c2c5b` |

**Branch bases differ.** Two branched from `3a38b5b7`, two from `2b3c2554`.
Rebase before merging, and re-read anything whose line numbers were quoted from
a stale base.

## Blocking — CI breaks the moment the binary rename lands

`.github/workflows/phosphor_release.yml` hardcodes the compiled binary's
filename. **Nobody owned this file in the round**, and it is not optional: once
`Channel::Oss` and `app/Cargo.toml`'s `[[bin]] name` become `phosphor-oss`,
these steps `cp` a path that no longer exists and the release job fails.

- lines **793, 795** — Linux CLI tarball: `cp "…executable_path" zap-oss` then `tar … zap-oss`
- lines **875, 877** — macOS CLI tarball: same shape
- lines **558, 574** — RPM / Arch steps referencing `target/release-lto/zap-oss`

Fix all of these to `phosphor-oss` **in the same merge** as the Rust binary
rename, not before (doing it early breaks CI in the opposite direction).

Note the tarball's *internal* filename is what
`remote_server::setup::binary_name()` looks for on the remote host. A mismatch
is currently survivable — `install_remote_server.sh`'s fallback search accepts
`phosphor-oss`, `zap-oss`, `warp-oss` and `oz*` (commit `248a7610`) — but
survivable is not correct; make the tarball name match.

## Blocking — cross-branch consistency

1. **`app/Cargo.toml` binary naming ↔ both packaging branches.**
   `[[bin]] name`, its `path`, `default-run`, and
   `[package.metadata.bundle.bin.zap-oss]` must all become `phosphor-oss`.
   These are literally the `cargo build --bin <name>` argument used by
   `script/linux/bundle` and `script/windows/bundle.ps1`; without them both
   packaging scripts fail outright.
2. **`app/Cargo.toml` `identifier` ↔ `script/macos/bundle` `BUNDLE_ID`.** Both
   must read `dev.phosphor.Phosphor`. `app_id_from_bundle()` overrides the Rust
   default from `CFBundleIdentifier` at runtime, so if these disagree a bundled
   build and `cargo run` silently use *different data directories*.
3. **`Channel::Oss` command name ↔ `script/check_channel_command_names`.** The
   guard checks the Rust name against the packaging literals
   (`phosphor-oss.cmd`, `bin/phosphor-oss`).
4. **Arch package name ↔ `app/src/autoupdate/linux.rs:343`.** That line lists
   the AUR package names probed for autoupdate detection
   (`["zap", "zap-bin", "zap-git"]`). The Linux branch renamed the package to
   `phosphor`; if this list is not renamed with it, autoupdate detection stops
   matching, silently.
5. **DockTilePlugin artifact name.** If the Rust branch's change to
   `app/DockTilePlugin/Info.plist` renames the produced bundle, the three
   `ZapDockTilePlugin.docktileplugin` paths in `script/macos/bundle` (~323, 519,
   520) must change with it. Left unresolved in the round.

## Deliberately NOT renamed — do not "finish the job"

Each of these is load-bearing under its old name. Renaming any of them causes
silent data loss or silent breakage, with no error at the point of failure.

- **`"zap-local-secure-storage-fallback-key"`**
  (`crates/warpui_extras/src/secure_storage/linux.rs:108`) — derives the AES key
  for disk-fallback secrets on hosts with no Secret Service. Renaming it makes
  every existing fallback blob undecryptable.
- **`Local\Zap{Channel}_SingleInstance`**
  (`app/src/app_services/windows/single_instance_manager.rs:60`) — must match
  `windows-installer.iss:37`'s `AppMutexName`, and keeping it stable is what
  lets a *new* installer detect an *old* running app during the upgrade that
  matters most.
- **Inno `AppId` (`zap-oss`)** — an installer-internal upgrade-tracking token,
  independent of the runtime app id. Changing it makes the new installer fail to
  recognise the existing install: two Add/Remove entries, two install trees.
- **`Channel::Oss => ".zap"` in `crates/remote_server/src/setup.rs:276`** — the
  install dir on the *remote* host. The migration runs locally only; there is no
  mechanism to migrate a directory on every host the user has ever SSHed into.
  Renaming orphans the daemon and its `bundled_resources` tree on all of them.
  **Open: wants a maintainer decision, not an agent's.**
- **Applied SQL migrations** mentioning "Zap" in comments — never edit an
  applied migration.
- **Crate names, module paths, Rust symbols, `WARP_*` env vars** — `SCOPE.md`
  layer 4 puts these explicitly out of scope.
- **Old identifiers in migration-source and recognition paths** must survive:
  the `"Zap" => "zap"` arm in `paths.rs`, and `is_zap_bundle()` in
  `external_editor/mac.rs:367`, which needs the new id added as an extra match
  arm rather than a literal swap.

## Open decisions still unresolved

- `zap://` scheme: Linux registers **both** `phosphor` and `zap`;
  `WARP_SCHEME_NAME="zap"` in `script/macos/bundle` was left untouched. Decide
  and make them consistent.
- Whether the TUI binary `zap-tui-oss` renames. It was **not** in scope for any
  branch. The published artifact is already `phosphor-tui`, so the internal name
  is cosmetic — but `crates/warp_tui/Cargo.toml` and six workflow references
  move together if it does.
- Ownership of CI workflows and generic scripts (`script/run`, `script/precheck`,
  `script/wasm/bundle`, `script/check_channel_command_names`) — no branch
  claimed them.

## Known-stale documentation

`CLAUDE.md:32` and `HANDOFF.md:13` still assert the zap identifiers are
intentionally unchanged. That predates the 2026-08-13 decision and is now
actively wrong — fix on merge.

## Before you trust any of this

Nothing here has been compiled, packaged, or installed. macOS and Windows cannot
be exercised on this hardware at all, and the PowerShell and Inno scripts were
not syntax-checked (no interpreter available). The first build is the real gate.
