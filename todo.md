# Code Review TODO

Actionable items from the code reviews run on 2026-07-26. Grouped by review.
Each item notes `file:line`, the problem, and the suggested fix.

---

## warp_tui test suite health (found 2026-07-29, commits `5b2d600f`/`eaabdc36`)

Discovered while verifying the #328 fix + TUI allow/reject keybindings.
Confirmed via `git stash` that both issues below reproduce identically on
clean HEAD — pre-existing, unrelated to either of those changes. Not fixed
here to keep those changes scoped; `cargo build`/`cargo check` (the actual
release gates) are unaffected either way.

- [ ] **`cargo test -p warp_tui --lib` deadlocks partway through a full serial run**
  Hangs at `tui_generic_tool_call_view::tests::accepting_new_conversation_suggestion_completes_the_executor`
  — 39 threads, all blocked in `futex_do_wait`, zero CPU progress for 20+
  minutes. Reproduced twice. Scoping the test filter away from this module
  avoids it (used for verification of the allow/reject work), so the full
  crate suite may simply have never been run to completion before.
  **Fix:** bisect which test(s) before it in run order leave shared state
  that this test deadlocks on (or whether it deadlocks in isolation too),
  likely a mutex/channel never released in the executor-completion test
  harness (`add_test_action_model`/`queue_tui_permission_action` or
  similar shared fixture).

- [ ] **3 tests in `terminal_session_view_tests.rs` fail even run alone/serially**
  `agent_hint_tracks_transcript_emptiness_without_input_invalidation`,
  `footer_conversations_callout_no_longer_renders`,
  `footer_model_label_is_a_bounded_click_target` — all fail with a
  default/empty-looking footer ("shell mode", "No custom provider
  configured" not found) even filtered down to just this one test module
  with `--test-threads=1`, meaning they depend on setup that normally
  happens as a side effect of some other test module running first in the
  full suite — not truly hermetic.
  **Fix:** find whatever global/singleton setup they implicitly depend on
  and make it explicit per-test (matching the pattern already used to fix
  `warp`'s own historical test-isolation issues — see settings.toml
  hermetic-path fix, ssh-onekey singleton, etc.).

---

## Follow-up code-review fixes (2026-07-29, commit `fddc193a`)

Dev machine is Linux; nothing below has been run against a real Windows
`pwsh.exe`. Verified only via `cargo check`/`cargo test` (static + unit-level).

- [ ] **NEEDS WINDOWS VERIFICATION: pwsh `-EncodedCommand` at 2 more call sites**
  — `app/src/terminal/model/session/command_executor/local_command_executor.rs:55`,
  `app/src/terminal/model/session/command_executor/msys2_command_executor.rs:67`
  Ported the same fix as the interactive-session-launch site (`shell.rs`,
  commit `5365c62a`) to `LocalCommandExecutor`'s generator/login-shell command
  path and `MSYS2CommandExecutor`'s Windows-native-shell path, both of which
  built `pwsh ... -c <command>` as a plain string — open to the same PS 7.6
  `-Command` quoting-parser crash on any command containing a `"`. Shared the
  encode logic into `util::encode_pwsh_command`.
  Regression tests (`encode_pwsh_command_round_trips_without_trailing_nul` in
  `util/mod.rs`, plus the existing `shell_tests.rs` one) only check the
  base64/UTF-16LE encoding itself round-trips correctly — they don't spawn a
  real `pwsh.exe` and confirm it accepts the argv or that a generator command
  containing a quote actually executes.
  **To verify:** on a Windows box with PowerShell 7.6, run a generator/BYOP
  local command whose text contains a `"` (e.g. a quoted path) through both
  executors and confirm it executes instead of erroring; also sanity-check a
  plain no-quotes command still works end-to-end (stdout/exit code correct).

---

## Security / performance audit — non-Warp code (2026-07-26)

Parallel audit (6 agents) of the fork's own code (Zap additions + newer work;
boundary = Warp merge-base `c325d146`). Upstream Warp treated as trusted/out of
scope. Ranked most-actionable first. No CRITICAL/HIGH security issues; **crash
sweep found zero reachable panics** (BYOP/AI stack is well-hardened). Duplicates
across agents have been merged.

### Security

- [x] **[MED] SSH-sync payload integrity → RCE-on-connect** — FIXED
  — `crates/warp_ssh_manager/src/sync_provider.rs`
  Now seals the entire `SshSyncData` in a single AES-GCM envelope (`seal_payload`
  / `unseal_payload`, v2 format), so every field — `host`, `key_path`,
  `startup_command`, `notes`, node structure — is covered by the GCM auth tag.
  Tampered payloads fail authentication and are rejected; legacy v1
  (unauthenticated) payloads are refused with a "re-upload to upgrade" message.
  Tests: `seal_roundtrip_*`, `tampered_sealed_payload_is_rejected`,
  `legacy_unauthenticated_payload_is_rejected`.
  *(original location note: sync_provider.rs:174,332)*
  On download, only the encrypted secret fields are authenticated; `host`,
  `key_path`, and `startup_command` come from the gist JSON integrity-unprotected,
  and `startup_command` is written verbatim to the PTY on connect. A tampered gist
  (writable with a `gist`-scoped or leaked token that can't read the encrypted
  blob) → command execution on connect, or connection/key redirect.
  **Fix:** authenticate the whole payload (HMAC/sign all fields, or wrap the entire
  JSON in the AES-GCM envelope), and confirm changes pulled from sync on apply.

- [x] **[MED] SSH destination argument injection (leading-dash host → local RCE)** — FIXED
  — `crates/warp_ssh_manager/src/ssh_command.rs`
  Added a `--` option terminator before the destination in all three argv paths
  (`build_ssh_args`, `test_key_auth`, `build_password_auth_cmd_args`) via a shared
  `push_destination` helper, so a `-o…` host/username can't be parsed as an ssh
  option. Regression tests: `build_ssh_args_guards_leading_dash_host`,
  `password_auth_args_guard_leading_dash_host`.
  *(original location note: ssh_command.rs:50-55)*
  (also `test_key_auth:118`, password path `:307-309`, PTY `build_ssh_command_line:59-65`)
  The `host` / `user@host` target is appended as the final `ssh` argv with no `--`
  separator. A host beginning with `-` (e.g. `-oProxyCommand=touch /tmp/pwned`)
  is parsed as an option → local command execution before any connection.
  `shell_escape` does NOT neutralize a leading-dash flag. Self-inflicted today
  (own config), but reachable if a host is ever imported/synced from `~/.ssh/config`
  or a shared profile.
  **Fix:** insert a literal `--` before the target in all four paths; reject
  host/username values starting with `-`.

- [x] **[MED] Cloud-sync key is unsalted, token-coupled, not a real KDF** — PARTIALLY FIXED
  — `crates/zap_sync/src/crypto.rs`
  Replaced `SHA256(SHA256(token))` with **Argon2id** over a random 16-byte
  per-message salt (embedded in the blob as `salt || nonce || ciphertext`). This
  closes the "not a real KDF / unsalted / brute-forceable low-entropy token"
  weakness; API unchanged so all callers are untouched. **Still token-derived**
  (not decoupled from gist access) — full decoupling would need an independent
  user passphrase (larger UX change), left as a follow-up.
  The AES-256-GCM key = `SHA256(SHA256(PAT))` — derived from the same GitHub/Gitee
  token that also fetches the ciphertext gist, with no salt/work factor/domain
  separation. Token compromise yields both ciphertext and key; low-entropy
  (self-hosted/Gitee/custom) tokens become brute-forceable against the public gist.
  **Fix:** derive the DEK from an independent user passphrase (or a random per-user
  key kept only in the OS keychain, never uploaded) via Argon2id + stored random
  salt. **Availability footgun:** rotating the PAT silently makes all synced data
  undecryptable — document it.

- [ ] **[LOW] `http://` provider base_url sends the API key as cleartext Bearer**
  — `app/src/ai/agent_providers/openai_compatible.rs:61` (and `chat_stream.rs`
  `normalize_endpoint_url:3344`)
  `http://` is permitted and `Authorization: Bearer <key>` is attached anyway.
  Intended for local Ollama, but a plaintext/MITM'd provider leaks the key.
  **Fix:** only allow `http://` when the host resolves to loopback, or warn when a
  key would go over plaintext.

- [x] **[LOW] Unbounded response/stream reads (DoS)** — ACCEPTED RISK (stock upstream, unfixed upstream; documented in SECURITY.md)
  — `lib/rust-genai/src/webc/web_client.rs:113,128`, `models_dev.rs:254`
  (`res.text()`/`bytes()` with no cap) and `web_stream.rs:~168` (SSE
  `partial_message` grows unbounded if the delimiter never arrives).
  A malicious/compromised provider endpoint can OOM the client. (gzip is off, so
  not a decompression bomb — just raw size.)
  **Fix:** size-limited streamed reads; cap the SSE buffer and error past a limit.

- [x] **[LOW] SSH sync uploads structural fields to the gist in plaintext** — RESOLVED (by the v2 seal)
  — `crates/warp_ssh_manager/src/sync_provider.rs`
  Mooted by the payload-integrity fix: the whole `SshSyncData` (host, username,
  port, startup_command, notes, key_path, node tree) is now inside the v2 AES-GCM
  seal, so nothing structural is on the wire in plaintext anymore.

- [x] **[LOW] Bearer token forwarded to `raw_url` taken from response JSON** — FIXED
  — `crates/zap_sync/src/gist_client.rs`
  The truncated-gist path now only attaches the `Authorization` header when
  `raw_url_is_trusted(platform, raw_url)` — HTTPS + a per-platform content-host
  allowlist (`gist.githubusercontent.com` etc. for GitHub, `*.gitee.com` for
  Gitee). A tampered `raw_url` is fetched without credentials, so the token can't
  be exfiltrated. Tests: `raw_url_trusted_*`, `raw_url_rejected_*`.

- [x] **[LOW] Decrypted secrets held in non-zeroized `String`** — FIXED
  — `crates/warp_ssh_manager/src/sync_provider.rs`
  `PendingSecret.value` is now `Zeroizing<String>` and both per-field decrypts are
  wrapped in `Zeroizing::new(...)`, so decrypted passwords/passphrases are zeroed
  on drop after being written to the keychain — consistent with
  `WrittenSecret.prior_value`.

- [x] **[LOW] SSRF IPv4-compatible IPv6 gap** — FIXED (to_ipv4 covers ::a.b.c.d); WASM DNS-filter gap noted (cloud target only)
  — `app/src/ai/agent_providers/tools/web_runtime.rs:110-155`
  `is_blocked_ip` handles `::ffff:a.b.c.d` but not the deprecated `::a.b.c.d`
  form; `SsrfSafeResolver` is `cfg(not(wasm32))` so the WASM build only checks IP
  literals. Marginal on desktop; noted for completeness.
  **Fix:** also reject embedded-IPv4 IPv6 / `v6.to_ipv4()`; document the WASM gap.

- [ ] **[LOW] Defense-in-depth: unvalidated inputs to sensitive sinks**
  — `vertex_auth.rs:89` (gcloud `--impersonate-service-account` SA email only
  checked non-empty — argv-safe, no injection, but add an email format check);
  `app/src/ssh_manager/su_password_injector.rs` + `secret_injector.rs:107` (raw
  secret + `\n` written to PTY — an embedded newline injects trailing bytes as
  commands; strip/reject control chars); prompt custom-file loader
  `prompt_renderer.rs:278` (blocks `..`/absolute but follows symlinks out of the
  dir — `canonicalize` + `starts_with` check).

### Performance (new TUI rendering — all HIGH, same trigger: per-streamed-chunk / per-frame)

- [x] **[HIGH] `sync_code_block_views` reclones every code block each streamed chunk** — FIXED
  — `crates/warp_tui/src/agent_block.rs`
  The reconciler now compares the borrowed `&str` against the retained view's
  content (`TuiCodeBlockView::matches`) and only clones new/changed sections (in
  practice just the streaming block). `sync()` already no-ops on an equal payload,
  so this elides only redundant allocation. Verified: builds; code_block (8) +
  agent_block (51) tests pass.

- [x] **[HIGH] `sync_action_views` re-clones actions each chunk** — FIXED (matches-skip for shell+plan; plan re-resolves presentation to catch model state). Commit e77659f7
  — `crates/warp_tui/src/agent_block.rs:498-541`
  Same trigger; clones every plan/shell/generic action every chunk.
  **Analysis:** *Shell* is safe to skip-when-unchanged — `update_action` is a pure
  function of `(action, output_streaming)` and shell action payloads are small
  (just the command string; live output is reactive from `terminal_model`), so the
  payoff is small. *Plan* (`CreateDocuments`/`EditDocuments`, the larger payloads)
  is NOT safe to skip: `sync_action` → `sync_documents` re-resolves per-document
  state from `action_model`, which changes independently of the action. A correct
  plan fix needs to fold that model-derived state into the change key.
  **Recommend:** do plan properly with a running-TUI check + a streaming snapshot
  test; shell-only is low value.

- [ ] **[HIGH] Full-document rebuild on every layout pass, not viewport-gated** — NEEDS REFACTOR (deferred)
  — `crates/warp_tui/src/editor_element.rs:351-401` (`build`) +
  `crates/editor/src/render/model/char_cell_display.rs:257-334` (`display_rows`)
  `layout()` unconditionally rebuilds: `text.chars().collect()` + a full-buffer
  `display_lattice` walk even when `with_viewport_rows` is set; any animated
  element (shimmer, ~10 Hz) re-layouts the whole retained tree.
  **Analysis:** `build()` can't be memoized wholesale — it has essential
  per-layout side effects (`try_layout_pending_edits`, scroll clamp/follow_cursor,
  `set_terminal_width`); skipping it breaks editing/scroll. The real fix is to
  separate the pure projection from the side effects and/or make `display_lattice`
  viewport-windowed in shared `crates/editor` code — an intricate change that
  **must** be verified in a running TUI. Deferred to a focused, harness-backed
  session rather than shipped blind.

### INFO / noted (not action items)

- Linux `secure_storage` fallback uses a hardcoded embedded key
  (`secure_storage/linux.rs:95-113`) → fallback files are effectively plaintext.
  This is **upstream Warp** code, but the fork now routes far more sensitive
  secrets through it (cloud-sync PAT, SSH passwords, BYOP API keys, proxy password)
  on headless-Linux/WSL/no-Secret-Service boxes, amplifying blast radius. Escalate
  upstream or override in the fork.
- genai logs full response bodies at `tracing::trace` (no secrets/`Authorization`).
- LLM file tools (`tools/files.rs`, `edit.rs`) add no extra sandboxing beyond
  upstream's executor + block-UI approval.
- **Crash sweep: 0 findings.** BYOP/AI stack uses checked slicing, `saturating_sub`,
  `.get()`, `from_utf8_lossy`, `to_ascii_lowercase`, division-by-zero guards
  throughout; one `crates/editor` diff is itself a panic fix.

---

## About page + Phosphor theme (commits `41a77348`, `472a339b`)

- [x] **Search terms advertise now-hidden autoupdate controls** — FIXED (trimmed)
  — `app/src/settings_view/about_page.rs:138`
  `search_terms` still lists "automatic updates auto update check for updates
  new version", but `SHOW_AUTOUPDATE_UI = false` hides those controls. Settings
  search for "automatic updates" leads to the About page with no such control.
  **Fix:** trim the autoupdate vocabulary from `search_terms` while the UI is
  hidden.

- [ ] **JPEG logo: opaque background + baked-in text, illegible at ~100px**
  — `app/src/settings_view/about_page.rs:187`
  The 1024×1024 badge is downscaled to ~100px (its "PHOSPHOR TERMLNK / CRT
  TERMINAL" lettering becomes noise), and being an opaque JPEG it renders as a
  dark box on a light-themed About page.
  **Fix:** use a transparent icon-only PNG/SVG mark for the About header; keep
  the full badge for README/marketing.

- [x] **Autoupdate observer now gated** — FIXED (subscribe only when SHOW_AUTOUPDATE_UI)
  — `app/src/settings_view/about_page.rs:61`
  `new()` still subscribes to `AutoupdateState` (`ctx.observe(... ctx.notify())`)
  and all autoupdate `handle_action` arms remain. While disabled, any autoupdate
  stage change re-renders the About page for no visible effect; the controller
  half is left half-wired.
  **Fix:** gate the subscription alongside the render (ideally derive the flag
  from real release-channel availability).

- [ ] **~200 lines reachable only via the const-false branch**
  — `app/src/settings_view/about_page.rs:303`
  `render_update_status` + `UpdateAction` + `format_bytes` +
  `format_download_progress` are only reachable through
  `SHOW_AUTOUPDATE_UI` (compile-time `false`). Deliberate/reversible, but the
  dead branch will bit-rot (still references the old `zerx-lab` release URL) and
  is untested while disabled.
  **Fix:** accept as documented, or extract behind a cfg/feature so it's clearly
  parked.

- [ ] **Amber theme duplicated in Rust const + yaml, hand-synced**
  — `themes/phosphor_amber.yaml:24`
  Phosphor Amber is defined twice — the bundled Rust `AnsiColors` const (the
  actual default) and this copy-in yaml — with no shared source. This change had
  to edit identical blue/cyan values in both; nothing prevents future drift.
  **Fix:** generate the yaml from the Rust const (or vice versa), or add a test
  asserting the two stay in sync.

---

## Vertex AI provider (merge `fae32e14`)

- [ ] **Empty project builds a malformed URL + silent picker drop**
  — `app/src/settings/ai.rs:924`
  No save-time validation, so a Vertex provider can be saved with an empty
  project. `build_byop_llm_infos` (`mod.rs:83`) then silently skips it (models
  never appear, no feedback), and `vertex_endpoint_url("", "global")` yields
  `.../projects//locations/global/` if any path resolves it.
  **Fix:** validate project non-empty at save time and surface the requirement
  in the UI.

- [x] **Vertex location not case-normalized** — FIXED (vertex_endpoint_url lowercases location)
  — `app/src/settings/ai.rs:927`
  The `location == "global"` check is case-sensitive and the raw location is
  interpolated into the hostname, so "Global" → `Global-aiplatform...` and
  "US-EAST5" → `US-EAST5-aiplatform...` — both invalid hosts.
  **Fix:** `location.to_ascii_lowercase()` before the global check and host
  interpolation.

- [x] **Cold-start token mint has no in-flight coalescing** — FIXED (MINT_LOCK single-flight)
  — `app/src/ai/agent_providers/vertex_auth.rs:47`
  On a cold cache, concurrent first requests (main stream + title gen +
  active-AI) each miss and spawn their own `gcloud auth print-access-token`
  subprocess.
  **Fix:** single-flight the mint per credential (per-credential async lock or
  in-flight map) so only one `gcloud` runs.

- [ ] **8-field positional provider-edit payload duplicated ~4×**
  — `app/src/settings_view/ai_page.rs:2425`
  `SaveAgentProviderEdits` / `SaveAgentProviderEditsThen` / the
  `to_save_action_with` closure type / `save_agent_provider_edits` all carry the
  same 8 positional fields, kept in lockstep by hand (now needs
  `#[allow(clippy::too_many_arguments)]`). A mismatched order silently swaps
  values.
  **Fix:** collapse into a single `ProviderEditFields` struct passed by value.

- [x] **Vertex family routing duplicated** — FIXED (shared vertex_model_family())
  — `app/src/ai/agent_providers/reasoning.rs:100`
  (and `app/src/ai/agent_providers/attachment_caps.rs:225`)
  The `contains("claude") ? Anthropic : Gemini` dispatch is implemented verbatim
  in both; a change to the heuristic must touch both or the surfaces disagree.
  **Fix:** extract `fn vertex_model_family(model_id: &str) -> AgentProviderApiType`
  and call it from both.

---

## warp-oss-sync / TUI port (range `ab207e20..7accb626`)

Scale: ~150 commits, 20k+ lines across 207 shared files (plus the isolated
`warp_tui` crate + test churn). Too large for a faithful inline line-by-line
pass — run `/code-review ultra josh/warp-oss-sync` for full coverage.

A **focused GUI-regression review of the two biggest GUI-facing keystones** was
done inline and both came back **clean**:

- [x] **View→Entity relaxation + `tui_views` routing** (`core/view/context.rs`,
  `core/view/handle.rs`) — GUI-safe. All `T: View` → `T: Entity` changes are
  widenings (`View: Entity`), method bodies unchanged; the `tui_views` fallback
  in `WeakViewHandle::upgrade` (and view/try_view/update_view) is
  `#[cfg(feature = "tui")]`-gated, so GUI builds behave identically. The change
  also fixes a latent bug where weak handles to TUI views failed to upgrade.
- [x] **`TerminalManager<S>` genericization** (`terminal/local_tty/terminal_manager.rs`)
  — structurally sound. GUI wiring stays in a concrete `impl
  TerminalManager<TerminalView>`; the generic `impl<S>` path is additive; GUI
  downcast site (`pane_group/mod.rs:2314`) is consistent. Full line-by-line of
  the 1079-line body extraction was not done (defer to ultra); the green test
  suite covers terminal behavior.

Reviewed 2026-07-26 (the three previously-unreviewed files) — all CLEAN:
- [x] `crates/warpui_core/src/core/app.rs` — GUI-safe. Same shape as the cleared
  keystones: View→Entity widenings, `tui_views` fallbacks all `#[cfg(feature =
  "tui")]`-gated (compiled out of GUI builds; the GUI `views` map is always
  checked first with unchanged behavior), and the `&mut dyn Any` downcast
  refactor is consistent throughout. No regression.
- [x] `crates/editor/src/render/model/mod.rs` — char-cell render model, no
  reachable panic: `opportunities` is sized `count+1` (never empty), row/char
  indexing relies on sentinel invariants that are `debug_assert`-checked and
  maintained by `rebuild`, byte-offset math is `.min()`/`.get()`/`saturating_sub`
  clamped. Internal-invariant-guarded, not untrusted-triggerable.
- [x] `app/src/ai/agent_providers/prompt_renderer.rs` — no SSTI: templates are
  pre-registered by name; LLM/user values flow in only as context DATA
  (`Value::from_serialize`), never compiled as templates. minijinja is sandboxed
  (no eval/shell/fs from templates), no `render_str`, no command exec.
  `custom_prompt_raw` blocks absolute/`..` paths (input is user config, not LLM).
  Only residual: symlink-follow (already tracked as a LOW above).
