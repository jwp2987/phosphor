# Remove the SSH Manager feature

> **STATUS: PLANNING (2026-08-02).** Maintainer decision: remove the fork-original
> SSH Manager. It's redundant for the target user (Unix/Linux admins already manage
> `~/.ssh/config` + `ssh-agent`), and its GitHub-gist credential sync is a security
> liability rather than a convenience. This is **fork-original code — no Warp parity
> constraint**, so removal is a free call. Nothing removed yet.

## Rationale

- A GUI SSH connection manager re-stores hosts in its own SQLite DB — a second copy
  of `~/.ssh/config`, which is already the canonical source of truth.
- The **gist-based credential sync** (`zap_sync` → encrypted GitHub gist) creates an
  attack surface that plain `~/.ssh/config` + agent forwarding + a dotfiles repo does
  not. Downside (syncing SSH creds through a gist) outweighs the upside.
- Note: the config parser is **read-only** on `~/.ssh/config` (no writes), so removal
  does not risk the user's real ssh config — that file is untouched throughout.

## Scope boundary — what is NOT removed

- **SSH session wrapper** (`is_legacy_ssh` / `enable_legacy_ssh_wrapper`, `terminal/local_tty/*`)
  — makes `ssh` sessions work in the terminal. Independent of the manager. **KEEP.**
- **`~/.ssh/config`** — read-only source; never written. **Untouched.**

(Correction to an earlier assumption: `zap_sync` is NOT shared. Verified 2026-08-02 —
the only `SyncDataProvider` impl is `SshSyncProvider`, and every `SyncEngine::new()` in
`cloud_sync_page.rs` pairs with `SshSyncProvider::new()`. `cloud_sync.rs` only selects
the platform. So zap_sync exists solely to sync the SSH manager → it is removed too.)

## Blast radius (verified via grep, 2026-08-02)

`warp_ssh_manager` / `app/src/ssh_manager` consumers to remove or detach:

- `crates/warp_ssh_manager/` — whole crate (db, repository, secrets, ssh_command, `ssh_config_parser`, `sync_provider`, types + tests)
- `app/src/ssh_manager/` — whole dir (panel, server_view, candidates, onekey, `secret_injector`, `startup_command_injector`, `su_password_injector`, password/shell prompts, notifier + tests)
- `app/src/sftp_manager/` — the SFTP file browser (built on ssh_manager). **REMOVE** (maintainer, 2026-08-02) — see resolved decision 1
- `app/src/pane_group/pane/ssh_server_pane.rs` — the SSH server pane
- `app/src/search/command_palette/ssh_servers/{data_source,search_item}.rs` + the ssh-servers entry in `command_palette/mixer.rs`
- `app/src/settings/ssh.rs` — the manager-specific settings (`enable_ssh_auto_discovery`; the SSH-manager sync settings). Keep any pure session-wrapper settings.
- `app/src/settings_view/cloud_sync_page.rs` — **remove wholesale** (the entire page only ever drives `SshSyncProvider`)
- `app/src/settings/cloud_sync.rs` — the sync settings group (**GitHub/Gitee** platform selection) — remove
- `app/src/settings/cloud_sync_secrets.rs` — sync token/secret storage — remove
- `crates/zap_sync/` — whole crate (gist client, crypto, sync engine, **GitHub/Gitee `SyncPlatform`**) — dead once SSH sync is gone
- **i18n cleanup** — remove the now-dead sync strings (Gitee/GitHub sync labels) from `app/i18n/{en,ja,zh-CN}/warp.ftl`. (Deleting dead keys for a removed feature ≠ translating; the CLAUDE.local.md "leave zh-CN/ja as-is" rule is about not converting live translations, so pruning dead keys across all locales is fine.)

**Verified (2026-08-02):** all Gitee/GitHub *sync-platform* code lives inside this removal set — no sync-platform code elsewhere (the only outside hits were binary assets + the Cargo dep line). So this fully removes the GitHub/Gitee sync code with no dead remnants.
- `app/src/integration_testing/ssh_manager/` — integration tests
- `app/src/lib.rs` — module registration
- `Cargo.toml` + `app/Cargo.toml` — drop **both** `warp_ssh_manager` and `zap_sync` workspace deps

## Open decisions (maintainer)

1. ~~SFTP manager~~ **RESOLVED (maintainer, 2026-08-02): remove the SFTP browser too.** Same managed-SSH feature set; doesn't make sense to keep.
2. ~~Settings gist-sync~~ **RESOLVED (verified 2026-08-02): remove `zap_sync` + `cloud_sync` too.** They are not "settings sync" — the only `SyncDataProvider` is `SshSyncProvider`, so the whole cloud-sync feature is dead once the SSH manager is gone. Folded into the removal above.
3. **Existing user data** — SSH-manager SQLite tables + OS-keychain credentials from
   prior use. Do NOT auto-delete keychain entries without consent; drop the DB tables
   via migration. `~/.ssh/config` is the preserved source of truth.

## Approach / sequencing

Remove top-down so the build breaks in a controlled order; flock-verify each layer:

1. UI/entry points: `ssh_server_pane`, command-palette `ssh_servers` + mixer entry, panel open commands/keybindings.
2. Settings: manager-specific entries in `settings/ssh.rs`; SSH portion of `cloud_sync_page`.
3. `app/src/ssh_manager/` (+ `sftp_manager/` per decision 1) + `integration_testing/ssh_manager`.
4. `app/src/lib.rs` module registration.
5. `crates/warp_ssh_manager/` + drop the workspace/app `warp_ssh_manager` dep.
6. DB migration to drop the manager tables (per decision 3).
7. Full `cargo test -p warp --lib --features gui,tui` green; grep confirms zero `warp_ssh_manager` / `ssh_manager` references remain.

## Notes

- The **SSH-sync security fixes** (branch `fix/ssh-sync-security` — Argon2id KDF, AES-GCM
  payload auth, `--` option-injection guard, gist raw_url token gating) become moot for
  the SSH-manager path once removed. If settings-gist-sync stays (decision 2 = no), the
  crypto/gist_client fixes still matter for `zap_sync`.
- Removal deletes a body of fork-original tests (`ssh_command_tests`, `candidates_tests`,
  `ssh_config_parser` tests, panel/injector tests) — expected, not a coverage regression.

## Progress

Nothing removed yet. Decisions 1 (remove SFTP) and 2 (remove zap_sync/cloud_sync)
resolved; only decision 3 (existing user keychain data — don't auto-delete) to confirm.
The removal set is one self-contained fork-original cluster:
`warp_ssh_manager` + `zap_sync` crates, `app/src/{ssh_manager,sftp_manager}`,
`settings/cloud_sync` + `settings_view/cloud_sync_page`, the ssh-server pane +
command-palette source, and the manager settings in `settings/ssh.rs`. No Warp-parity
impact (all fork-original); SSH session wrapping stays.
