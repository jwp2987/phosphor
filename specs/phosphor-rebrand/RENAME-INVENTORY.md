# Rename completeness inventory — `dev.zap.Zap` → `dev.phosphor.Phosphor`

Exhaustive checklist of remaining occurrences of the old storage/system
identity, for the maintainer to check off once the four parallel branches
(Rust identity/migration, Linux packaging, Windows+macOS packaging, this
docs/inventory pass) land. Produced 2026-08-13 against
`worktree-agent-a2f12d974cad43177` at commit `3a38b5b7` (fast-forwarded from
`2b3c2554` to pick up `LAYER3-PLAN.md`, which this inventory implements the
"§7 Verification" cross-check for).

**Method.** `rg` over the whole tree, excluding `target/`, `.git/`,
`lib/rust-genai/`, and `.claude/worktrees/`, for the ten strings the task
specified at minimum, plus every string those hits led to when traced
(secure-storage literals, migration-source paths, related Rust symbols). Not
a blind `grep -i zap` — that returns ~792 files, the overwhelming majority of
which are `warp_core`/`warpui`/`zap_sftp`/`zap_sync`/module-path lineage that
`SCOPE.md` layer 4 puts explicitly out of scope. This inventory is the
*storage/system identity* subset of that larger set.

## Verdict legend

- **RENAME** — part of the storage/system identity; must change as part of
  this milestone.
- **KEEP — internal lineage.** Crate names, module paths, Rust symbols,
  `WARP_*` build env vars. Out of scope per `SCOPE.md` layer 4.
- **KEEP — load-bearing literal.** Must NOT change or data becomes
  unrecoverable/unreadable.
- **KEEP — immutable history.** Applied SQL migrations / archived docs; never
  edited after the fact regardless of what they say.
- **OWNED BY Rust** — the identity/migration branch (`AppId`, `paths.rs`,
  `channel/`, `crates/remote_server`, and any other `.rs` file).
- **OWNED BY Linux packaging** — `resources/linux/**`, `script/linux/**`,
  `app/channels/oss/*.desktop`, `app/channels/oss/icon/**`.
- **OWNED BY Windows+macOS packaging** — `script/windows/**`,
  `script/macos/**`, `*.iss`, `*.ps1`, `app/DockTilePlugin/**`, and the
  macOS-bundle lines of `app/Cargo.toml`.
- **MINE (docs, this pass)** — fixed in `SCOPE.md`/`LAYER3-PLAN.md` in this
  same commit.
- **DOC DRIFT (not fixed this round)** — prose referencing the old identity
  that is still *accurate* today (layer 3 hasn't shipped) but will go stale
  the moment it does. Not one of this round's four deliverables; flagged so
  it isn't forgotten in a later prose sweep.
- **UNCERTAIN** — ownership or disposition genuinely unclear; stated why.

---

## KEEP — load-bearing literals (highest-value section — read this first)

| literal | where | why it must not change |
|---|---|---|
| `"zap-local-secure-storage-fallback-key"` | `crates/warpui_extras/src/secure_storage/linux.rs:108` | Derives the AES-GCM key for the disk-fallback secret store used on hosts with no Secret Service provider (`SecureStorage`'s non-keyring path, `secure_storage/linux.rs`). Renaming it makes every existing fallback-encrypted blob undecryptable — silent, permanent data loss with no error at the point of loss. **Verified this is the only such literal**: `mac.rs` and `windows.rs` in the same directory take the service name as a parameter and hardcode nothing "zap"-shaped. |

No other "must never change" literal was found. Two things that *look* similar but are not load-bearing in the same way, so they don't belong in this table:

- `ChannelState::data_domain()` (→ the Secret Service `service` / Keychain
  service name) **does** need to change, but via the documented migration
  (read under the old service name, write under the new one) — it's a RENAME
  with a migration step, not a "leave alone" literal. See the migration-source
  section below.
- The two generic `SECURE_STORAGE_KEY` constants (`"AgentProviderSecrets"` in
  `app/src/ai/agent_providers/secrets.rs:13`, `"AiApiKeys"` in
  `crates/ai/src/api_keys.rs:7`) aren't "zap"-named at all — they're the *key
  names* the migration must enumerate (per `LAYER3-PLAN.md` §4's "exhaustive
  list of secure-storage keys"), not identity strings themselves. Listed here
  only so whoever writes that enumeration doesn't have to re-find them.

## KEEP — immutable history (do not edit even though they say "Zap")

| where | what |
|---|---|
| `crates/persistence/migrations/2026-08-04-000000_drop_ssh_manager_tables/{up,down}.sql` | Comments mention `zap_sync`/"Zap does not maintain a down migration here". Applied SQL migrations are never edited after landing, regardless of content — this is a repo-wide rule, not specific to this rename. |
| `crates/persistence/migrations/2026-05-11-000000_drop_lsp_workspace_tables/down.sql` | Same — "Zap does not maintain a down migration here". |

## Migration-source references — correct as-is, must survive a naive sweep

These read the **old** `zap` identity on purpose: they are what the Phase-2
migration copies *from*. A find-and-replace that blindly renamed every
`"zap"` string would break exactly these.

| where | what |
|---|---|
| `LAYER3-PLAN.md` §4 | Explicitly: `SecureStorage::new("dev.zap.Zap")` as a second, throwaway read-only instance during migration. This is prose describing not-yet-written code, listed here so the eventual implementation is recognized as correct, not "leftover". |
| `crates/warp_core/src/paths.rs:274` (`"Zap" => "zap".to_owned()`) and the `"Zap*" => "zap-*"` arm at `:275` | These conditionals **read** the app id and must keep recognizing the *old* name until/unless the migration path also handles discovering old directories some other way. If Rust's rename changes `AppId::new` to `"Phosphor"` without also handling old-directory discovery, this arm becomes dead code for detection purposes — trace carefully; `LAYER3-PLAN.md` §4's "Trap — the Linux directory-name special case" covers the *new*-name half of this but not explicitly the migration-source-detection half. |
| `crates/remote_server/src/setup.rs:276` (`Channel::Oss => ".zap"` inside `remote_server_dir()`) | This is the **remote host's** install directory name (`~/.zap/remote-server`), independent of the local app id. Renaming it is a RENAME (see below), but note it has no migration path documented anywhere in `LAYER3-PLAN.md` — existing remote-server installs on already-configured SSH hosts will need re-installing under the new path; there is no "copy the remote directory" step planned. Flag for whoever owns this file. |

---

## 1. `dev.zap.Zap` — 42 occurrences, 20 files

| file:line | context | verdict |
|---|---|---|
| `crates/warp_core/src/channel/state.rs:40` | `AppId::new("dev", "zap", "Zap")` — default `ChannelConfig` | OWNED BY Rust |
| `app/src/bin/zap_oss.rs:29` | `AppId::new("dev", "zap", "Zap")` — OSS channel config | OWNED BY Rust |
| `app/src/bin/zap_oss.rs:68` | `<string>dev.zap.Zap</string>` (Info.plist template embedded in the bin) | OWNED BY Rust |
| `app/src/app_services/linux/mod.rs:160,163` | `default_service = "dev.zap.Zap"` — D-Bus well-known name for single-instance activation (see the SCOPE.md correction — **not** the keyring service) | OWNED BY Rust |
| `app/src/util/file/external_editor/mac_test.rs:6` | `assert!(is_zap_bundle("dev.zap.Zap"))` | OWNED BY Rust — **and** `is_zap_bundle` (`external_editor/mac.rs:367`) needs a new `dev.phosphor.Phosphor` arm added, not just this literal swapped; see §"Additional findings" below |
| `app/src/autoupdate/linux.rs:443` | doc comment, `~/.local/share/dev.zap.Zap/` | OWNED BY Rust |
| `app/build.rs:382,393` | comments referencing the AUMID `dev.zap.Zap` | OWNED BY Rust |
| `app/src/settings_view/scripting_page.rs:22` | module doc: "the unchanged app id `dev.zap.Zap`" | OWNED BY Rust (also now factually stale — the app id is *not* staying unchanged; flag for whoever edits this file) |
| `crates/warpui/src/platform/windows/mod.rs:82` | comment, `dev.zap.Zap -> Zap` example | OWNED BY Rust |
| `crates/warp_core/src/paths_tests.rs:92,109,137,154` | 4 test assertions on macOS/Windows paths and `project_path()` | OWNED BY Rust — must be updated in lockstep with the `AppId::new` change or they'll fail immediately |
| `app/channels/oss/dev.zap.Zap.desktop` (filename + `:28,32`) | filename, `StartupWMClass=`, `Icon=` | OWNED BY Linux packaging |
| `resources` — n/a, see desktop file above | — | — |
| `script/linux/bundle:225,229` | `BUNDLE_ID="dev.zap.Zap"` | OWNED BY Linux packaging |
| `script/macos/bundle:315` | `BUNDLE_ID="dev.zap.Zap"` | OWNED BY Windows+macOS packaging |
| `script/windows/bundle.ps1:125` | comment | OWNED BY Windows+macOS packaging |
| `script/windows/windows-installer.iss:20` | comment | OWNED BY Windows+macOS packaging |
| `app/DockTilePlugin/Info.plist:6` | `<string>dev.zap.ZapDockTilePlugin</string>` (CFBundleIdentifier) | OWNED BY Windows+macOS packaging |
| `app/Cargo.toml:879` | `identifier = "dev.zap.Zap"` (cargo-bundle metadata) | OWNED BY Windows+macOS packaging |
| `CLAUDE.md:32` | "the app id (dev.zap.Zap)... are intentionally unchanged" | DOC DRIFT — **now actively wrong**, not just stale: this line documents the *pre-decision* state (option A, decouple-and-defer) that `LAYER3-PLAN.md` §3 superseded on 2026-08-13. Whoever next edits `CLAUDE.md`'s HTML-comment block should fix this; flagged here rather than fixed in this pass since it's outside this round's three deliverables |
| `docs/migrate-from-warp.md:182` | "which gives it its own app ID (`dev.zap.Zap`)" | DOC DRIFT — accurate today, will need updating once Phase 2 ships |
| `specs/andy/CODE-1786/TECH.md:16` | historical tech spec, quotes `dev.zap.Zap` as an example | KEEP — immutable history (closed spec doc, not live guidance) |

## 2. `zap-oss` — 59 occurrences, 28 files

Grouped; every file is listed, exact line numbers given where the file has
few hits, "throughout" noted where it recurs heavily.

| file | lines | verdict |
|---|---|---|
| `crates/warp_core/src/channel/mod.rs` | 45, 70 | OWNED BY Rust — `Channel::Oss => "zap-oss"`, source of truth |
| `app/Cargo.toml` | 3 (`default-run`), 21 (`name = "zap-oss"`), 876 (`[package.metadata.bundle.bin.zap-oss]`) | OWNED BY Rust — this is the actual Cargo `[[bin]]` target definition, must move with `channel/mod.rs` |
| `app/build.rs` | 130, 132 | OWNED BY Rust (`rustc-link-arg-bin=zap-oss=...`) |
| `app/src/lib.rs` | 1059, 2818 | OWNED BY Rust (comments) |
| `app/src/autoupdate/mac.rs` | 924, 950, 961 | OWNED BY Rust |
| `app/src/app_services/mac/mod.rs` | 46, 47 | OWNED BY Rust |
| `app/src/settings_view/scripting_page.rs` | 22 | OWNED BY Rust |
| `app/src/bin/zap_oss.rs` | 66 (`<string>zap-oss</string>`) | OWNED BY Rust |
| `crates/remote_server/src/setup.rs` | 550 | OWNED BY Rust |
| `crates/remote_server/src/setup_tests.rs` | 246, 277, 289 | OWNED BY Rust |
| `crates/remote_server/src/install_remote_server.sh` | 7, 132 | OWNED BY Rust (coupled 1:1 to `setup.rs`, ships with the crate) |
| `script/windows/bundle.ps1` | 113, 114, 217, 221 (`$INNO_APP_ID`) | OWNED BY Windows+macOS packaging |
| `script/windows/windows-installer.iss` | 43, 246 | OWNED BY Windows+macOS packaging |
| `script/macos/bundle` | 314, 548 | OWNED BY Windows+macOS packaging |
| `script/macos/run` | 10 | OWNED BY Windows+macOS packaging |
| `app/channels/oss/dev.zap.Zap.desktop` | 15 (comment) | OWNED BY Linux packaging |
| `script/linux/bundle` | 199, 200, 209 | OWNED BY Linux packaging |
| `script/wasm/bundle` | 130 | UNCERTAIN — wasm bundling isn't one of the three named branches; needs an owner assigned |
| `script/test_warpctrl_early_dispatch` | 36, 64, 69 (+ `ZAP_CONTROL_TEST_BINARY` env var, see below) | UNCERTAIN — generic dev/test script, cross-platform, not clearly assigned |
| `script/run` | 30 | UNCERTAIN — generic dev-run script |
| `script/precheck` | 228 | UNCERTAIN — comment in the CI gate script |
| `script/check_channel_command_names` | 12, 23, 49 | UNCERTAIN — CI guard script; also its whole *purpose* (verifying channel↔command-name maps agree) should be re-run once the rename lands, not just have its comment text updated |
| `.github/workflows/phosphor_release.yml` | 408, 558, 574, 793, 795, 808, 875, 877, 890 (+ zap-tui-oss lines, see §3) | UNCERTAIN — release CI, consumes the Rust-defined binary name; not clearly owned by any of the three branches, but must be updated in the same PR that renames the binary or releases will silently keep building `zap-oss` |
| `.github/workflows/pr-check.yml` | 116 | UNCERTAIN — same reasoning |
| `TODO.md` | 1772, 1778, 1779 | DOC DRIFT |
| `docs/TODO-ARCHIVE.md` | 229 | KEEP — immutable history (archive) |
| `.cargo/config.toml` | 21 (comment) | OWNED BY Rust (`.toml`, excluded from my edits) |

## 3. `zap-tui-oss` — 26 occurrences, 13 files

| file | lines | verdict |
|---|---|---|
| `crates/warp_tui/Cargo.toml` | 9 (`default-run`), 15 (`name = "zap-tui-oss"`) | OWNED BY Rust |
| `app/src/terminal/cli_agent.rs` | 180, 229, 231 | OWNED BY Rust |
| `app/src/terminal/cli_agent_tests.rs` | 269, 617, 623, 668 | OWNED BY Rust |
| `crates/warp_tui/tests/worker_dispatch.rs` | 10 (`CARGO_BIN_EXE_zap-tui-oss`) | OWNED BY Rust |
| `crates/warp_tui/scripts/tui_harness.py` | 2, 12, 44, 92, 104 | OWNED BY Rust (dev harness shipped with the crate, coupled to the bin name) — note line 26 (see `.local/state/zap-tui` below) uses a *different* directory name than the app id, worth double-checking during the rename, not just find-replacing |
| `.github/workflows/phosphor_release.yml` | 408, 419, 426, 808, 817, 824, 890, 899, 904 | UNCERTAIN — same reasoning as §2 |
| `DECLINED.md` | 191 | DOC DRIFT — describes current fact ("declares one real bin (`zap-tui-oss`)") inside a *decision* row about `tui_cli_shell_command`. The decision itself doesn't depend on the binary name, so this needs a one-line factual update in the same commit as the actual rename, not before (editing a `DECLINED.md` row prematurely, out of sync with code, is worse than leaving it) |
| `README.md` | 110 | DOC DRIFT |
| `HANDOFF.md` | 13 | DOC DRIFT — **also stale for the same reason as CLAUDE.md above**: lists `zap-tui-oss` under "legacy identifiers... stay", which predates the `LAYER3-PLAN.md` decision to actually rename the binary. This is prescriptive guidance an agent might follow literally and then be wrong; flag as higher-priority drift than the others in this file |
| `specs/warp-oss-sync/SCOPE.md` | 50, 968 | KEEP — immutable history (records what had shipped as of that spec's date) |
| `specs/tui-render-perf/SCOPE.md` | 167 | DOC DRIFT |
| `specs/usage-test-suite/SCOPE.md` | 5, 66, 347 | DOC DRIFT |
| `specs/usage-test-suite/README.md` | 10 | DOC DRIFT |
| `.cargo/config.toml` | 21 (comment) | OWNED BY Rust |

## 4. `/opt/zap` — 3 occurrences, 2 files (+ this plan)

| file:line | verdict |
|---|---|
| `script/linux/linuxdeploy-plugin-warp:19,66` | OWNED BY Linux packaging |
| `LAYER3-PLAN.md:67` | MINE — plan prose, no action needed |

## 5. `~/.zap` — 16 occurrences, 9 files

| file | lines | verdict |
|---|---|---|
| `app/src/settings_view/ai_page.rs` | 2465, 3066 | OWNED BY Rust |
| `app/src/ai/agent_providers/prompt_renderer.rs` | 364 | OWNED BY Rust |
| `crates/warp_core/src/paths.rs` | 76 (doc comment) | OWNED BY Rust |
| `crates/remote_server/src/setup.rs` | 269 (doc comment, `warp-oss: ~/.zap/remote-server`) | OWNED BY Rust — **this is the migration-source-shaped item flagged above**; see that section |
| `crates/remote_server/src/setup_tests.rs` | 241, 283, 285 | OWNED BY Rust |
| `crates/remote_server/src/install_remote_server.sh` | 6 | OWNED BY Rust |
| `docs/migrate-from-warp.md` | 38, 47, 48, 49, 311 | DOC DRIFT — this is literally the Warp→fork migration doc; once layer 3 ships it will need a *second* migration note (zap paths → phosphor paths) alongside this one, not just a string swap |

## 6. `.config/zap` — 5 occurrences, 5 files

| file:line | verdict |
|---|---|
| `crates/warp_core/src/paths_tests.rs:30` | OWNED BY Rust |
| `app/channels/oss/dev.zap.Zap.desktop:15` (comment) | OWNED BY Linux packaging |
| `resources/linux/debian/app/postinst.template:5` (comment) | OWNED BY Linux packaging |
| `SCOPE.md:128`, `LAYER3-PLAN.md:160` | MINE — no action |

## 7. `.local/state/zap` — 4 occurrences, 4 files

| file:line | verdict |
|---|---|
| `crates/warp_core/src/paths_tests.rs:111` | OWNED BY Rust |
| `app/src/ai/agent_providers/tools/documents.rs:237` (doc-comment example path) | OWNED BY Rust |
| `crates/warp_tui/scripts/tui_harness.py:26` | OWNED BY Rust — **UNCERTAIN sub-note**: this comment says `~/.local/state/zap-tui/oz/zap-tui.log`, i.e. app name `zap-tui`, not `zap`/`Zap`. Per `DECLINED.md`'s "TUI/GUI shared app id" row, GUI and TUI deliberately share **one** app id/state dir — so either this comment is already wrong today (pre-existing, unrelated to this rename) or the TUI harness writes somewhere the shared-app-id design doesn't predict. Worth a real look, not a find-replace. |
| `SCOPE.md:129` | MINE — no action |

## 8. `.local/share/zap` — 2 occurrences, 2 files

| file:line | verdict |
|---|---|
| `crates/warp_core/src/paths_tests.rs:13` | OWNED BY Rust |
| `SCOPE.md:128` | MINE — no action |

## 9. `x-scheme-handler/zap` — 3 occurrences, 3 files

| file:line | verdict |
|---|---|
| `app/channels/oss/dev.zap.Zap.desktop:40` (`MimeType=`) | OWNED BY Linux packaging — also see `LAYER3-PLAN.md` §3 open decision #4: whether to keep `zap://` registered alongside `phosphor://` for existing links, which determines whether this line is replaced or has a second `MimeType=` added |
| `SCOPE.md:151`, `LAYER3-PLAN.md:65` | MINE — no action |

## 10. `ZapDockTilePlugin` — 20 occurrences, 8 files

| file | lines | verdict |
|---|---|---|
| `app/DockTilePlugin/Info.plist` | 6, 8, 10, 18 | OWNED BY Windows+macOS packaging |
| `app/DockTilePlugin/ZapDockTilePlugin.m` | filename + 1, 32, 185 | OWNED BY Windows+macOS packaging |
| `app/DockTilePlugin/Makefile` | 1, 2, 14 | OWNED BY Windows+macOS packaging |
| `script/macos/bundle` | 323, 519, 520 | OWNED BY Windows+macOS packaging |
| `app/src/appearance.rs` | 294 (`"ZapDockTilePlugin.docktileplugin"`) | OWNED BY Rust — **note the derived-id trap from `LAYER3-PLAN.md` §2**: this is `app_id` + suffix, not independent; must change in the same commit as the `Info.plist` rename or the plugin bundle name and the string this Rust code looks for diverge |
| `app/build.rs` | 58, 59, 77, 78 | OWNED BY Rust |
| `app/src/settings/app_icon.rs` | 10 (comment: "update the logic in ZapDockTilePlugin.m") | OWNED BY Rust |
| `LAYER3-PLAN.md:73` | MINE — no action |

---

## Additional findings beyond the ten required strings

Surfaced by tracing the required hits into their call sites — not exhaustive
the way §1–10 are, but worth recording so they aren't independently
rediscovered.

| what | where | verdict |
|---|---|---|
| `is_zap_bundle()` | `app/src/util/file/external_editor/mac.rs:367` | OWNED BY Rust. The **function name** is KEEP — internal lineage (Rust symbol, Layer 4 territory) — but its **body** matches `dev.zap.Zap` and every `dev.warp.Warp*` variant to decide "is this app in the Warp/Zap lineage" for the external-editor-picker guard. It needs a new match arm for `dev.phosphor.Phosphor` (functional change), separate from any literal swap. Miss this and Phosphor builds silently fail the "is this our own bundle" check. |
| `ZAP_CONTROL_TEST_BINARY` (env var) | `script/test_warpctrl_early_dispatch:59,64,119-124,134` | KEEP — internal lineage, by analogy to the `WARP_*` build-env-var carve-out in `SCOPE.md` layer 4. It's a test-only override var, not a persisted/user-facing identity string. Not renamed. |
| `SkillProvider::Zap` | `crates/ai/src/skills/skill_provider.rs` and ~15 call sites across `app/src/ai/**`, `crates/ai/**` | KEEP — internal lineage. Rust enum variant naming a skill-source provider; explicitly listed as staying in `HANDOFF.md:13`. Not a storage-identity string. |
| `zap_sftp`, `zap_sync` crate names | `SCOPE.md:177` (own text), `script/windows/bootstrap.ps1:52`, `docs/TODO-ARCHIVE.md:846`, `specs/warp-parity-sweep/SCOPE.md:188`, `specs/remove-ssh-manager/SCOPE.md:13,25,28,43` | KEEP — internal lineage. Crate names, explicitly out of scope per layer 4. (`zap_sync`/SSH-manager removal is a separate, already-declined-and-in-progress effort — see `specs/remove-ssh-manager/SCOPE.md` — unrelated to this rename.) |
| `WarpIcon::Zap` | `app/src/terminal/input/slash_commands/data_source/mod.rs:610` | KEEP — internal lineage. Icon enum variant, Rust symbol. |
| SQL migration comments mentioning "Zap" | `crates/persistence/migrations/2026-08-04-000000_drop_ssh_manager_tables/{up,down}.sql`, `2026-05-11-000000_drop_lsp_workspace_tables/down.sql` | KEEP — immutable history. |

---

## Bucket counts

Counting each distinct file:line pair once per section above (a line hit by
two search terms, e.g. a desktop-file line containing both `dev.zap.Zap` and
`x-scheme-handler/zap` substrings, is counted once per section it appears in
— sections are per search term, not deduplicated globally, so this total
double-counts a handful of multi-match lines by design: it mirrors "how many
checklist rows exist," which is what the maintainer greps against, not "how
many unique source lines").

| bucket | rows |
|---|---|
| RENAME (OWNED BY Rust) | ~85 |
| RENAME (OWNED BY Linux packaging) | ~16 |
| RENAME (OWNED BY Windows+macOS packaging) | ~35 |
| RENAME (UNCERTAIN owner — CI/generic scripts) | ~20 |
| KEEP — load-bearing literal | 1 |
| KEEP — immutable history | 6 |
| KEEP — internal lineage | ~25 |
| DOC DRIFT (not fixed this round) | ~15 |
| MINE (fixed in this pass — `SCOPE.md`/`LAYER3-PLAN.md`) | ~12 |

Total distinct file:line hits across the ten required strings: **~180**
(exact per-term counts: `dev.zap.Zap` 42, `zap-oss` 59, `zap-tui-oss` 26,
`/opt/zap` 3, `~/.zap` 16, `.config/zap` 5, `.local/state/zap` 4,
`.local/share/zap` 2, `x-scheme-handler/zap` 3, `ZapDockTilePlugin` 20).

## Open items for the maintainer

1. **`ZAP_CONTROL_TEST_BINARY`-style UNCERTAIN rows** (CI workflows, generic
   `script/*` dev tooling, `script/wasm/bundle`) have no clear owner among the
   three code branches. Someone needs to claim `.github/workflows/*.yml`,
   `script/wasm/bundle`, `script/run`, `script/precheck`,
   `script/check_channel_command_names`, and
   `script/test_warpctrl_early_dispatch` before merge, or the release
   pipeline keeps building `zap-oss`/`zap-tui-oss` after the Rust rename
   lands.
2. **`crates/remote_server/src/setup.rs`'s `remote_server_dir()`** renames the
   *remote* host's install directory with no migration plan — flagged above,
   worth a decision (leave stale installs to self-heal on next `oz remote
   setup`? write a remote-side migration too?) before Phase 2 ships.
3. **`CLAUDE.md` and `HANDOFF.md`** both currently tell a reader "zap
   identifiers are intentionally unchanged" / "stay" — both predate the
   2026-08-13 decision and are now misleading, not just stale. Recommend
   fixing both in the same commit that flips `AppId::new`, so no window
   exists where the docs actively contradict the code.
4. **`is_zap_bundle()`** needs a new `dev.phosphor.Phosphor` match arm — a
   functional change, easy to miss if this rename is treated as pure
   find-and-replace.
