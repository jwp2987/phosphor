# TODO — Phosphor: Warp parity ledger (#11) + code-review debt

Reconciled 2026-08-04; **#11 section re-verified against code on `main` 2026-08-06.**
**Last updated 2026-08-06 late — `main` = `8c1841a94`, 25 PRs merged, 0 open.**
`[x]` items in issue #11 = "keep/restore" (maintainer wants them in the fork). This
file is the live tracker: **mark an item `- [x]` the moment it's verified done.**

> **`main` is currently RED by maintainer decision.** PRs #140 and #181 were merged
> knowingly carrying failing tests (#171 and the `warpui` suite) so all work would be
> consolidated on one branch, with the fixes to follow. Do not treat a red suite on
> `main` as a new regression without first checking it against #171. See
> [Red on main](#red-on-main--2026-08-06) below.

## Rules (apply to every item — same as the whole project)
- **`warp/master` is the behavioral oracle.** Port faithfully; adapt only for
  BYOP/local (no cloud) — never silently simplify away Warp behavior (AGENTS §5.10).
- **Tests-first, never defer.** Port Warp's oracle tests with each feature; a red
  test gets fixed now, never parked (AGENTS §5.6). Never weaken an assertion to go green.
- **Run all cargo through the governor:** `script/agent-cargo <agent-name> <cargo-args>`.
  It bounds how many compiles run at once and gives each agent its own target dir.
  Never invoke cargo bare while another agent is running (AGENTS §5.8).
- **English only** (code, comments, tests, docs). Exception: `app/i18n/zh-CN|ja/*.ftl`.
- **Central verification:** the owner re-runs the suite before marking done — don't
  trust an agent's self-report.
- **No CI builds as a discovery loop.** Local `cargo`/user's `script/run --release`
  is the verification; a release build happens once at the end to confirm.

---

## What's in this file

Two separate concerns, kept distinguishable:

- **[Part 1 — Warp parity restore ledger (#11)](#part-1--warp-parity-restore-ledger-11)** —
  the issue #11 keep/restore ledger plus the other outstanding parity work.
- **[Part 2 — Code-review debt](#part-2--code-review-debt)** — actionable findings from the
  code reviews and the security/performance audit, grouped by review.

Part 2 was consolidated in from a separate lowercase `todo.md` on 2026-08-06: two tracked
files differing only by case collide on case-insensitive filesystems (macOS/Windows), so
only this `TODO.md` remains. Nothing was dropped — items since verified as landed on `main`
are marked `- [x]` with the evidence inline rather than deleted.

---

# Part 1 — Warp parity restore ledger (#11)

## #11 status — verified against code on `main` 2026-08-06

Of issue #11's **56 checked `[x]` items: 44 built + 2 resolved 2026-08-06 eve
(pending-edit-batch core merged via #105; history_model remainder keep-dropped
per #107) = 46. 10 remain**, of which 3 are decisions/holds (global-skills,
skill-remote-path, local_control app-side) and **7 are buildable — now in-flight
on the overnight agent fleet**. (The 13 unchecked `[ ]` items are all
keep-dropped/cloud — OTEL, VoiceInputLifecycle, semantic-search, RunAgents
orchestration, computer-use recording, cloud-mode-v2, product-analytics
telemetry, `IsCloudConversationStorageEnabled`, etc. — not work, by decision.)

### Merged this session
- [x] **Pending-edit-batch conflict-discard** — CORE MERGED (PR #105, `Fixes #101`):
  `PendingEditBatch` 200 ms debounce + push-conflict-discard + save-flush; 3 oracle
  tests green in isolation. Deferred sub-part `BufferConflictDetected` server→client
  push tracked as **#102** (blocks `handle_buffer_conflict_detected` + its 4th test);
  now being built by the fleet. Assessment: `specs/pending-edit-batch/ASSESS.md`.

### Requires macOS / Windows — cannot be built or verified on this host
Not deferred for lack of intent: this box is Linux and these cannot be compiled or
exercised here at all. They need a macOS or Windows machine (or CI) to progress.
**Do not mark any of these done from a Linux build.**

- [ ] **WSLENV passthrough vars** *(Windows)* — `wsl_env_allowlist` absent (0 hits).
  Compile-only port plus a flag.
- [ ] **Launch-at-login** *(macOS + Windows)* — `app/src/login_item/` does not exist.
- [ ] **Edition-2024 release verification** *(macOS)* — the **code work is done and on
  `main`** (commit `48bc21cb9`, PR #53). Only a macOS release build remains unverified.
- [ ] **pwsh `-EncodedCommand` at 2 call sites** *(Windows)* — the fix is ported to
  `local_command_executor.rs:55` and `msys2_command_executor.rs:67`, matching the
  already-verified `shell.rs` site (commit `5365c62a`). Needs a Windows run to confirm.

### Won't do — decided 2026-08-06
- [x] **AI global skills** (global arm) — **WON'T DO (maintainer, 2026-08-06).**
  Unchecked from #11. No `global_skills`/`filter_skills_by_spec` in
  `app/src/ai/skills/`; the bundled arm is done and the remote arm is cloud (dropped).
  The single remaining non-cloud function has **no consumer** and would require a
  `LocalOrRemotePath` type migration to land. Not worth the migration for dead code.

### Not started — true gaps
- [ ] **Skill remote-path** — now **#205**. Promoted out of this ledger after finding a
  real correctness bug rather than a missing feature: `get_provider_for_path` **and**
  `get_scope_for_path` both resolve `home_skills_path` against the *client's* home, so
  a remote skill under a same-named home dir is silently misclassified as local.
  Latent only because #170 means no remote path reaches them yet — **fix with or
  before #170.** Note this ledger previously claimed `get_scope_for_path` was migrated
  by #59; it was not (still `&Path`).

### Keep-dropped (decided this session)
- [x] **history_model reconciliation** — non-cloud parts DONE (optimistic rename /
  event-sequence / child-index cleanup + `TransientError` recovery). Remaining
  `WaitForEvents`/orchestration part is **KEEP-DROPPED (maintainer 2026-08-06)**: it
  is cloud orchestration (only a Warp-server tool call triggers it; RunAgents /
  OrchestrationEventStreamer are the dropped cloud surface). The BYOP recovery
  equivalent (`recovery_pending`→`TransientError`) already covers the local case, so
  `WaitingForEvents` never firing is correct, not a bug. The constructor-arity bits
  (`start_new_conversation`/`prompt_history_candidates`) have no consumer (tie to the
  undecided NLD-flags item). Recorded on #11; tracking issue #107 closed.

### Core landed — sub-part / wiring still outstanding
- [x] **`remote_server_controller` connection-label helpers** — DONE. This entry was
  **false**: `connection_label_for_session_info` is called in production at
  `remote_server_controller.rs:290` and `:526`, not only from its own tests.
  Re-verified against `main` `8c1841a94` on 2026-08-06.
- [ ] **`local_control` / `warpctrl` app-side** — now **#200**. crate `crates/local_control` exists;
  `app/src/local_control/` is absent. Blocked on `FeatureFlag::{WarpControlCli,
  AgentManagementView}` + a missing Agent-Management view subsystem.
- [ ] **Pinned-tabs / tab-groups remaining GUI surfaces** — tracked as **#146**. storage (migrations +
  schema), the live model (`Workspace::tab_groups`, `TabData::{group_id, pinned}`),
  the `PinTab`/`UngroupTabs`/… actions, snapshot round-trip, keybindings, the
  per-tab Pin/Unpin + tab-group context-menu entries, the multi-tab right-click
  menu, shift/cmd-click multi-selection and the "Move to group" submenu sidecar
  all landed. Still to port from `warp/master`: the vertical-tabs group-header
  row, the tab-group right-click menu (which hangs off that header), the inline
  group-rename editor, and group-aware drag-and-drop reordering.
- [ ] **repo_metadata standing-queries wiring** — now **#201**. `standing_queries.rs` on main;
  the app skill-watcher wiring that drives it is the follow-up.
- [ ] **Log-rotation deferred wiring** — now **#202**. machinery built (`simple_logger` + `warp_logging`
  rotation). **This entry was partly false and is corrected:** `register_with_rotation`
  **is** called at the MCP logger site (`app/src/ai/mcp/templatable_manager/native.rs:789`,
  with `logs::mcp_log_rotation_config()`), re-verified against `main` `8c1841a94`.
  Remaining real work: `frontend` / `max_file_size_bytes` are still not threaded
  into `LogConfig`.
- [x] **code_review over SSH — git write-ops** — DONE, merged 2026-08-06 (PR #125,
  issue #116). Commit / push / create-PR RPCs over SSH, plus a
  `git_operation_in_progress` guard on all three mutating handlers. Verified
  109/109 on `code_review` before merge. **Remaining sub-part:** AI commit-message
  autogen is still local-only and calls `generate_for_local_repo` with no
  `is_remote()` check — see #126.

### Done — 44 of 56 (present on `main`)
Verified by spot-check (all present): `is_jupyter_notebook_file`, `sorted_cd_directories`,
`LLMContextWindow`, `safe_browser_open_url`, `remote_matches_to_global`,
`GitBranchTrackingStatus`, `seal_with_context`, `SshRemoteServerSupport`,
`soft_wrapped_row_bounds`. The remainder are the earlier ~24 keeps (theme
syncability, relative line numbers, mermaid config, OSC-52/OSC-8, hyperlink
registry, tmux DCS, link-punct strip, CJK boundary, box-drawing, block-lifecycle,
code-symbol source, `approve` keyword, `sync::Condition`, `file_uri_drive_path`,
NLD flags, CDPATH, SettingSurfaces, browser allowlist, content-version assets,
image fallback, `TuiStack`, soft-wrap Home/End, tab-drag collapse, oversized
data-URI, editable bindings, autoupdate per-channel, `external_control_master`,
async find, banner-immune PATH capture, terminal-background reprobe, managed-secrets
BYO, `ModelEventDispatcher` SSH gate, URI deep-links) plus the PRs #58–97 items
(queued-prompts panel, remote/SSH global search, diff-state-over-SSH read path,
skill-scope `Home`, WSL program translation, Windows PATHEXT, `nld_heuristic_v2`,
mermaid fallback, focus-URL env, `standing_queries`, pinned-tabs storage).
(Sampled, not each of 44 exhaustively grepped.)

---

## Other outstanding (non-#11)
- [x] ⭐ **SMOKE TESTS** — on merged main (2026-08-05, after the 6 diff-state PRs #59-#64):
  `./script/usage-test --surface both` = **12 pass / 0 fail / 7 skip** (skips are
  needs-real-shell / needs-byop / needs-desktop — environmental), EXIT 0. Full warp lib
  sanity: `cargo test -p warp --lib` = **3910 pass / 0 fail / 33 ignored**. App boots and
  behaves with all parity + diff-state-over-SSH changes in.
- [ ] **Edition-2024 cross-platform build — macOS release verification only.** The code work
  is DONE and on `main`: the mac/wasm/windows `unsafe`-syntax fixes from branch
  `fix/edition-2024-native-targets` are merged (commit `48bc21cb9`, via PR #53) — verified
  2026-08-06 with `git merge-base --is-ancestor 48bc21cb9 main`, and the remote branch has
  been deleted. **All that remains is a local macOS `script/run --release` run**, which
  cannot be done on this Linux host — it needs a Mac (no CI-discovery builds). That run may
  still surface further latent mac-only errors.
- [ ] **#4 warp_tui suite** — plain `cargo test -p warp_tui --lib` STILL DEADLOCKS at
  `tui_generic_tool_call_view::accepting_new_conversation_suggestion_completes_the_executor`
  (reconfirmed 2026-08-04). The #4 fix may only hold under nextest. Re-investigate;
  do NOT force-green. Also its listed real bugs (diff ghost-blocks, transcript-clear).
  UPDATE 2026-08-05: `cargo nextest run -p warp_tui --no-fail-fast` = **579 pass / 18 fail**
  (589 → 597 total). PRE-EXISTING (confirmed: `git log dc885a802..HEAD -- crates/warp_tui`
  is EMPTY — this session's diff-state PRs #61-65 never touched warp_tui). Failures span
  input::view, root_view, session_registry, terminal_session_view, transcript_view,
  tui_diff_storage (×4), tui_file_edits_view, tui_permission_prompt (×2),
  tui_shell_command_view (×4). Spot-check: `input::view::move_up_through_empty_line_positions_cursor`
  PASSES in isolation (nextest parallel-ordering artifact — shared global-state pollution,
  same class as the i18n-isolation item below); `session_registry::focus_drives_events`
  FAILS in isolation (real). So the 18 are a MIX of real bugs + isolation-order artifacts.
  Dedicated #4 effort — not a diff-state regression. (usage-suite's warp_tui-nextest subset
  is green, which is why smoke passed.)
- [ ] **#2 sweep** — the 2 missing GUI auto-resume oracle tests
  (`completed_user_controlled_lrc_{resumes_when_not_suppressed,skips_resume_when_suppressed}`)
  are now PORTED to `terminal/view_test.rs` (2/0; the resumes case needed a
  `GlobalResourceHandlesProvider` mock for the subagent-sidecar persist path; the fork's
  teardown method is `set_user_control_with_stop_reason`, Warp's is `set_user_control_for_teardown`).
  Broader 379-module sweep still ongoing. (Anchor Stop/auto-resume regression already code-fixed.)
- [ ] **#5 deferred low-sev** — 5 latent items, all still present; low priority.
- [x] **warp-suite i18n test-isolation** (found 2026-08-04) — the 3 deterministically-red
  tests (`drive::export::test_export_untitled_notebook`, `search::…::test_directory_search_support`,
  `workspace::…::terminal_primary_line_falls_back_to_new_session`) were the localized-`t!()`
  case: `App::test` never globally inits i18n, so the key only resolved when an earlier test
  triggered init. FIXED per-test and **LANDED ON `main` via PR #103** (commit `3150a17b9`) —
  verified 2026-08-06 with `git merge-base --is-ancestor 3150a17b9 main`; issue #98 is closed.
  All 3 green in isolation, no assertions changed. NOTE: the same class likely
  still affects #4's `slash_commands` tests; a test-binary-global i18n init would close those too.
- [ ] get_relevant_files: live end-to-end smoke against a real BYOP provider (unit + lib green).
- [x] **Vertex provider bugs** — DONE, on `main`. Empty-project silent-drop (`#99`) +
  8-field payload struct (`#100`), fixed on `fix/vertex-provider-bugs` (commit `a08b52777`)
  and **merged via PR #104** — verified 2026-08-06 with
  `git merge-base --is-ancestor a08b52777 main`. `AgentProvider::validation_error()` and
  `ProviderEditFields` are present on `main`; issues #99/#100 are closed. Nothing to review
  or merge.

## Issue reconciliation status
- **#37** SSH ControlMaster guard — DONE (verify → close). Refinement `external_control_master` still open (above).
- **#4** — NOT done (deadlock reproduces; see above).
- **#98/#99/#100** — MERGED to `main` (PRs #103 and #104) and all three issues are closed. **#101** pending-edit-batch core also merged (PR #105) and closed; **#102** filed (deferred BufferConflictDetected push) and still OPEN.
- **#2/#5/#11** — tracking issues; stay open. #11 items tracked here.

### Closed 2026-08-06 late
#129 (mermaid flake) · #131 (MCP redaction gate) · #135 (PR lookup) · #137 (empty
branch dropdown over SSH) · #138 (watch filter) · #143 (Privacy page) · #145 (editor
parity) · #152 (`/usage` + `/cost`) · #156 (`PrInfo` fields) · #157 (gh-auth) ·
#185 (WSL paths) · #196 (WCAG chip labels).

### Deliberately left open — partially resolved, remainder is real
- **#126** — BYOP commit-message gen: local path shipped (PR #130); the remote path
  was deferred pending #125. #125 has now landed and the wiring still is not done —
  `maybe_start_commit_message_autogen` calls `generate_for_local_repo` with **no
  `is_remote()` check**, so on an SSH repo it runs `git` against a path that does not
  exist locally and silently produces no draft.
- **#136** — `read_files`: local half fixed (PR #159); remote half open. The stated
  blocker is **gone** — PR #192's proto re-pin supplies the `failed_reads` field that
  `AnyFilesSuccess` previously lacked, so it is now implementable.
- **#142** — `api_keys`: all portable tests ported (PR #189). Real remainder is a
  serde round-trip test for `AgentProviderSecrets`, which lives in `app/`. Note the
  issue's own premise was wrong — Warp has 71 tests not 82, and
  `custom_model_providers_*` is superseded by `AgentProviderSecrets`, not missing.
- **#146** — pinned tabs: core + deferred UI merged (PRs #132, #172); see the
  remaining GUI surfaces item above.

### New issues filed 2026-08-06 late
#183, #184 (`warp_cli` gaps) · #188 (3 more local-model-on-remote-path sites) ·
#191 (`.rustfmt.toml` pins edition 2018 while all 64 crates are 2024) · #194 (BYOP
token accounting was dead, which disabled auto-compaction) · #196 (closed).

---

## Red on main — 2026-08-06

`main` carries knowingly-failing tests. This was a maintainer decision to consolidate
all work onto one branch and fix afterwards, taken in full knowledge of AGENTS §5.6.
**It is a debt to pay down, not a new policy.**

- [ ] **#171 — 9 ported Warp terminal tests fail** (came in with PR #140). Two are
  security-relevant and should be fixed first:
  - **OSC 1337 parser panic on untrusted PTY output** — `ansi/mod.rs:1073` indexes
    `params[1]` unguarded; `warp/master` guards it. Any process writing to the
    terminal can crash it.
  - **Unquoted `cat {history_file}`** — `session.rs:1384`.
  - Remainder: LRC misclassification, focus reporting, copy/ETX, wrapped-path
    truncation, scrollback assert, Droid.
- [ ] **`warpui` / `warpui_core` suite** (came in with PR #181). Also contains ~82
  lines of `#[cfg(macos)]` code that **cannot be compiled on Linux** and needs macOS
  CI to verify at all.
- [ ] **Establish the real baseline.** Before this, `main` was 4005/0/33 at
  `44bf4daa6`. Re-measure and record the number here so the next session can tell an
  accepted failure from a new regression.

**Method note:** the "4025" figure that circulated was PR #132's *branch* number, not
`main`'s, and it made a clean proto re-pin look like it had lost 22 tests. Always
state which commit a test count belongs to.

---

# Part 2 — Code-review debt

Actionable items from the code reviews run on 2026-07-26 (and later). Grouped by review.
Each item notes `file:line`, the problem, and the suggested fix.

Consolidated here from the former lowercase `todo.md` on 2026-08-06. Every item was carried
over. Items re-verified against `main` during the consolidation and found already landed were
flipped to `- [x]` with the evidence inline — none were deleted. Note that several `file:line`
references below predate later refactors (e.g. `app/src/settings_view/about_page.rs` is now the
`about_page/` module); the original paths are kept as written so the findings stay traceable.

## warp_tui test suite health (found 2026-07-29, commits `5b2d600f`/`eaabdc36`)

Discovered while verifying the #328 fix + TUI allow/reject keybindings.
Confirmed via `git stash` that both issues below reproduce identically on
clean HEAD — pre-existing, unrelated to either of those changes. Not fixed
here to keep those changes scoped; `cargo build`/`cargo check` (the actual
release gates) are unaffected either way.

- [x] **`cargo test -p warp_tui --lib` deadlocks partway through a full serial run**
  — RESOLVED on `main` (PR #124, commit `87d06d179`, "fix(warp_tui): stop cargo test
  --lib deadlock"; verified 2026-08-06 with
  `git merge-base --is-ancestor 87d06d179 main`), which reworked
  `tui_generic_tool_call_view_tests.rs` and `test_fixtures.rs`.
  Hung at `tui_generic_tool_call_view::tests::accepting_new_conversation_suggestion_completes_the_executor`
  — 39 threads, all blocked in `futex_do_wait`, zero CPU progress for 20+
  minutes. Reproduced twice. Scoping the test filter away from this module
  avoided it (used for verification of the allow/reject work), so the full
  crate suite may simply have never been run to completion before.
  **NOTE:** only the *deadlock* is closed out here. The remaining `warp_tui`
  suite work — the 18 `nextest` failures — is tracked in Part 1 under
  **`#4 warp_tui suite`**, whose text predates PR #124.

- [x] **3 tests in `terminal_session_view_tests.rs` fail even run alone/serially**
  — RESOLVED on `main` (PR #73, commit `b7c6012ce`, "test(tui): drive footer/zero-state
  trio into AI input mode"; verified 2026-08-06 with
  `git merge-base --is-ancestor b7c6012ce main`), which made the implicit setup explicit
  per-test exactly as the fix below proposed. All three tests still exist in
  `crates/warp_tui/src/terminal_session_view_tests.rs`.
  `agent_hint_tracks_transcript_emptiness_without_input_invalidation`,
  `footer_conversations_callout_no_longer_renders`,
  `footer_model_label_is_a_bounded_click_target` — all failed with a
  default/empty-looking footer ("shell mode", "No custom provider
  configured" not found) even filtered down to just this one test module
  with `--test-threads=1`, meaning they depended on setup that normally
  happened as a side effect of some other test module running first in the
  full suite — not truly hermetic.
  **Fix (applied):** find whatever global/singleton setup they implicitly depend on
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

Parallel audit (6 agents) of the fork's own code (fork-specific additions + newer work;
boundary = Warp merge-base `c325d146`). Upstream Warp treated as trusted/out of
scope. Ranked most-actionable first. No CRITICAL/HIGH security issues; **crash
sweep found zero reachable panics** (BYOP/AI stack is well-hardened). Duplicates
across agents have been merged.

> **Scope note (2026-08-06):** Track 3 (merge `fad390189`, on `main`) deleted
> `app/src/ssh_manager/`, `crates/warp_ssh_manager`, `crates/zap_sync` and
> `crates/zap_sftp` outright. Every finding below that names one of those paths is
> therefore moot as live code — the fixes are recorded for history, and the one
> remaining follow-up they carried (decoupling the cloud-sync DEK from the PAT) has
> no code left to apply to.

### Security

- [x] **[MED] SSH-sync payload integrity → RCE-on-connect** — FIXED
  — `crates/warp_ssh_manager/src/sync_provider.rs` *(crate since removed by Track 3)*
  Now seals the entire `SshSyncData` in a single AES-GCM envelope (`seal_payload`
  / `unseal_payload`, v2 format), so every field — `host`, `key_path`,
  `startup_command`, `notes`, node structure — is covered by the GCM auth tag.
  Tampered payloads fail authentication and are rejected; legacy v1
  (unauthenticated) payloads are refused with a "re-upload to upgrade" message.
  Tests: `seal_roundtrip_*`, `tampered_sealed_payload_is_rejected`,
  `legacy_unauthenticated_payload_is_rejected`.
  *(original location note: sync_provider.rs:174,332)*
  On download, only the encrypted secret fields were authenticated; `host`,
  `key_path`, and `startup_command` came from the gist JSON integrity-unprotected,
  and `startup_command` was written verbatim to the PTY on connect. A tampered gist
  (writable with a `gist`-scoped or leaked token that can't read the encrypted
  blob) → command execution on connect, or connection/key redirect.
  **Fix:** authenticate the whole payload (HMAC/sign all fields, or wrap the entire
  JSON in the AES-GCM envelope), and confirm changes pulled from sync on apply.

- [x] **[MED] SSH destination argument injection (leading-dash host → local RCE)** — FIXED
  — `crates/warp_ssh_manager/src/ssh_command.rs` *(crate since removed by Track 3)*
  Added a `--` option terminator before the destination in all three argv paths
  (`build_ssh_args`, `test_key_auth`, `build_password_auth_cmd_args`) via a shared
  `push_destination` helper, so a `-o…` host/username can't be parsed as an ssh
  option. Regression tests: `build_ssh_args_guards_leading_dash_host`,
  `password_auth_args_guard_leading_dash_host`.
  *(original location note: ssh_command.rs:50-55)*
  (also `test_key_auth:118`, password path `:307-309`, PTY `build_ssh_command_line:59-65`)
  The `host` / `user@host` target was appended as the final `ssh` argv with no `--`
  separator. A host beginning with `-` (e.g. `-oProxyCommand=touch /tmp/pwned`)
  is parsed as an option → local command execution before any connection.
  `shell_escape` does NOT neutralize a leading-dash flag. Self-inflicted today
  (own config), but reachable if a host was ever imported/synced from `~/.ssh/config`
  or a shared profile.
  **Fix:** insert a literal `--` before the target in all four paths; reject
  host/username values starting with `-`.

- [x] **[MED] Cloud-sync key is unsalted, token-coupled, not a real KDF** — PARTIALLY FIXED,
  remainder now MOOT (crate removed by Track 3)
  — `crates/zap_sync/src/crypto.rs` *(crate since removed by Track 3)*
  Replaced `SHA256(SHA256(token))` with **Argon2id** over a random 16-byte
  per-message salt (embedded in the blob as `salt || nonce || ciphertext`). This
  closed the "not a real KDF / unsalted / brute-forceable low-entropy token"
  weakness; API unchanged so all callers were untouched. It remained **token-derived**
  (not decoupled from gist access) — full decoupling would have needed an independent
  user passphrase (larger UX change), left as a follow-up. That follow-up is now moot:
  there is no `zap_sync` crate on `main`.
  The AES-256-GCM key was `SHA256(SHA256(PAT))` — derived from the same GitHub/Gitee
  token that also fetched the ciphertext gist, with no salt/work factor/domain
  separation. Token compromise yielded both ciphertext and key; low-entropy
  (self-hosted/Gitee/custom) tokens became brute-forceable against the public gist.
  **Fix:** derive the DEK from an independent user passphrase (or a random per-user
  key kept only in the OS keychain, never uploaded) via Argon2id + stored random
  salt. **Availability footgun:** rotating the PAT silently made all synced data
  undecryptable — document it.

- [x] **[LOW] `http://` provider base_url sends the API key as cleartext Bearer**
  — RESOLVED on `main` (PR #114, commit `74e365635`; verified 2026-08-06 with
  `git merge-base --is-ancestor 74e365635 main`)
  — `app/src/ai/agent_providers/openai_compatible.rs:61` (and `chat_stream.rs`
  `normalize_endpoint_url:3344`)
  `http://` was permitted and `Authorization: Bearer <key>` was attached anyway.
  Intended for local Ollama, but a plaintext/MITM'd provider leaked the key.
  **Fix (applied):** `is_loopback_host` / the cleartext-risk check in
  `app/src/ai/agent_providers/mod.rs` now gate the bearer to `https://` or a
  loopback `http://` endpoint; `chat_stream.rs` strips the key and warns otherwise.
  The gate matches on the literal host (`localhost`, `127.0.0.0/8`, `::1`) rather
  than on DNS resolution, since a name that merely resolves to loopback today can
  be repointed tomorrow.

- [x] **[LOW] Unbounded response/stream reads (DoS)** — ACCEPTED RISK (stock upstream, unfixed upstream; documented in SECURITY.md)
  — `lib/rust-genai/src/webc/web_client.rs:113,128`, `models_dev.rs:254`
  (`res.text()`/`bytes()` with no cap) and `web_stream.rs:~168` (SSE
  `partial_message` grows unbounded if the delimiter never arrives).
  A malicious/compromised provider endpoint can OOM the client. (gzip is off, so
  not a decompression bomb — just raw size.)
  **Fix:** size-limited streamed reads; cap the SSE buffer and error past a limit.

- [x] **[LOW] SSH sync uploads structural fields to the gist in plaintext** — RESOLVED (by the v2 seal)
  — `crates/warp_ssh_manager/src/sync_provider.rs` *(crate since removed by Track 3)*
  Mooted by the payload-integrity fix: the whole `SshSyncData` (host, username,
  port, startup_command, notes, key_path, node tree) went inside the v2 AES-GCM
  seal, so nothing structural was on the wire in plaintext anymore.

- [x] **[LOW] Bearer token forwarded to `raw_url` taken from response JSON** — FIXED
  — `crates/zap_sync/src/gist_client.rs` *(crate since removed by Track 3)*
  The truncated-gist path only attached the `Authorization` header when
  `raw_url_is_trusted(platform, raw_url)` — HTTPS + a per-platform content-host
  allowlist (`gist.githubusercontent.com` etc. for GitHub, `*.gitee.com` for
  Gitee). A tampered `raw_url` was fetched without credentials, so the token
  couldn't be exfiltrated. Tests: `raw_url_trusted_*`, `raw_url_rejected_*`.

- [x] **[LOW] Decrypted secrets held in non-zeroized `String`** — FIXED
  — `crates/warp_ssh_manager/src/sync_provider.rs` *(crate since removed by Track 3)*
  `PendingSecret.value` became `Zeroizing<String>` and both per-field decrypts were
  wrapped in `Zeroizing::new(...)`, so decrypted passwords/passphrases were zeroed
  on drop after being written to the keychain — consistent with
  `WrittenSecret.prior_value`.

- [x] **[LOW] SSRF IPv4-compatible IPv6 gap** — FIXED (to_ipv4 covers ::a.b.c.d); WASM DNS-filter gap noted (cloud target only)
  — `app/src/ai/agent_providers/tools/web_runtime.rs:110-155`
  `is_blocked_ip` handled `::ffff:a.b.c.d` but not the deprecated `::a.b.c.d`
  form; `SsrfSafeResolver` is `cfg(not(wasm32))` so the WASM build only checks IP
  literals. Marginal on desktop; noted for completeness.
  **Fix:** also reject embedded-IPv4 IPv6 / `v6.to_ipv4()`; document the WASM gap.

- [x] **[LOW] Defense-in-depth: unvalidated inputs to sensitive sinks**
  — RESOLVED on `main`, all three sub-parts:
  - `vertex_auth.rs:89` — gcloud `--impersonate-service-account` SA email was only
    checked non-empty (argv-safe, no injection, but wanted an email format check).
    **Done:** `is_plausible_service_account_email` now gates the flag (PR #114,
    commit `74e365635`), with unit tests.
  - `app/src/ssh_manager/su_password_injector.rs` + `secret_injector.rs:107` — raw
    secret + `\n` written to PTY, so an embedded newline injected trailing bytes as
    commands. **Moot:** the whole `app/src/ssh_manager/` directory was deleted by
    Track 3 (merge `fad390189`, on `main`); there are no injectors left in the tree.
  - prompt custom-file loader `prompt_renderer.rs:278` — blocked `..`/absolute but
    followed symlinks out of the dir. **Done:** `canonicalize` + `starts_with`
    containment on both loader paths (PR #114), with a regression test.

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
  Same trigger; cloned every plan/shell/generic action every chunk.
  **Analysis:** *Shell* is safe to skip-when-unchanged — `update_action` is a pure
  function of `(action, output_streaming)` and shell action payloads are small
  (just the command string; live output is reactive from `terminal_model`), so the
  payoff is small. *Plan* (`CreateDocuments`/`EditDocuments`, the larger payloads)
  is NOT safe to skip: `sync_action` → `sync_documents` re-resolves per-document
  state from `action_model`, which changes independently of the action. A correct
  plan fix needs to fold that model-derived state into the change key.
  **Recommend:** do plan properly with a running-TUI check + a streaming snapshot
  test; shell-only is low value.

- [ ] **[HIGH] Full-document rebuild on every layout pass, not viewport-gated** — now **#203**. — NEEDS REFACTOR (deferred)
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
  secrets through it (BYOP API keys, proxy password) on
  headless-Linux/WSL/no-Secret-Service boxes, amplifying blast radius. Escalate
  upstream or override in the fork. (The cloud-sync PAT and SSH passwords that
  originally widened this blast radius are gone with Track 3.)
- genai logs full response bodies at `tracing::trace` (no secrets/`Authorization`).
- LLM file tools (`tools/files.rs`, `edit.rs`) add no extra sandboxing beyond
  upstream's executor + block-UI approval.
- **Crash sweep: 0 findings.** BYOP/AI stack uses checked slicing, `saturating_sub`,
  `.get()`, `from_utf8_lossy`, `to_ascii_lowercase`, division-by-zero guards
  throughout; one `crates/editor` diff is itself a panic fix.

---

## About page + Phosphor theme (commits `41a77348`, `472a339b`)

- [x] **Search terms advertise now-hidden autoupdate controls** — FIXED (trimmed)
  — `app/src/settings_view/about_page.rs:138` (now `about_page/mod.rs:152`)
  `search_terms` still listed "automatic updates auto update check for updates
  new version", but `SHOW_AUTOUPDATE_UI = false` hides those controls. Settings
  search for "automatic updates" led to the About page with no such control.
  **Fix:** trim the autoupdate vocabulary from `search_terms` while the UI is
  hidden.

- [ ] **JPEG logo: opaque background + baked-in text, illegible at ~100px** — now **#204**.
  — `app/src/settings_view/about_page.rs:187` (now `about_page/mod.rs:167`,
  `bundled/jpg/phosphor-logo.jpeg` — re-confirmed present on `main` 2026-08-06)
  The 1024×1024 badge is downscaled to ~100px (its "PHOSPHOR TERMLNK / CRT
  TERMINAL" lettering becomes noise), and being an opaque JPEG it renders as a
  dark box on a light-themed About page.
  **Fix:** use a transparent icon-only PNG/SVG mark for the About header; keep
  the full badge for README/marketing.

- [x] **Autoupdate observer now gated** — FIXED (subscribe only when SHOW_AUTOUPDATE_UI)
  — `app/src/settings_view/about_page.rs:61` (now `about_page/mod.rs:72`)
  `new()` still subscribed to `AutoupdateState` (`ctx.observe(... ctx.notify())`)
  and all autoupdate `handle_action` arms remained. While disabled, any autoupdate
  stage change re-rendered the About page for no visible effect; the controller
  half was left half-wired.
  **Fix:** gate the subscription alongside the render (ideally derive the flag
  from real release-channel availability).

- [x] **~200 lines reachable only via the const-false branch** — RESOLVED via the
  "extract so it's clearly parked" option
  — `app/src/settings_view/about_page.rs:303`
  `render_update_status` + `UpdateAction` + `format_bytes` +
  `format_download_progress` were only reachable through
  `SHOW_AUTOUPDATE_UI` (compile-time `false`). Deliberate/reversible, but the
  dead branch would bit-rot and was untested while disabled.
  **Fix (applied):** they now live in their own module,
  `app/src/settings_view/about_page/autoupdate_ui.rs`, whose header documents it as
  "**Parked, not wired up.**" and states that re-enabling means flipping
  `SHOW_AUTOUPDATE_UI` back to `true`; `autoupdate_ui_tests.rs` covers `format_bytes`
  and `format_download_progress` so the parked code no longer rots untested.

- [x] **Amber theme duplicated in Rust const + yaml, hand-synced** — RESOLVED via the
  "add a test asserting the two stay in sync" option
  — `themes/phosphor_amber.yaml:24`
  Phosphor Amber is defined twice — the bundled Rust `AnsiColors` const (the
  actual default) and this copy-in yaml — with no shared source. The change that
  raised this had to edit identical blue/cyan values in both, and nothing prevented
  future drift.
  **Fix (applied):** `app/src/themes/default_themes_tests.rs` now has
  `phosphor_amber_yaml_matches_builtin_theme` (and a green counterpart) asserting the
  YAML round-trips to exactly the built-in `WarpTheme`, with a failure message telling
  you to re-sync. The two are still hand-synced duplicates by design — the test is the
  guard rail.

---

## Vertex AI provider (merge `fae32e14`)

- [x] **Empty project builds a malformed URL + silent picker drop** — RESOLVED on `main`
  (issue `#99`, commit `a08b52777`, PR #104; verified 2026-08-06 with
  `git merge-base --is-ancestor a08b52777 main`)
  — `app/src/settings/ai.rs:924`
  There was no save-time validation, so a Vertex provider could be saved with an empty
  project. `build_byop_llm_infos` (`mod.rs:83`) then silently skipped it (models
  never appeared, no feedback), and `vertex_endpoint_url("", "global")` yielded
  `.../projects//locations/global/` if any path resolved it.
  **Fix (applied):** `AgentProvider::validation_error()` rejects a Vertex provider with
  an empty `vertex_project` at save time and `save_agent_provider_edits` surfaces it as
  an error toast.

- [x] **Vertex location not case-normalized** — FIXED (vertex_endpoint_url lowercases location)
  — `app/src/settings/ai.rs:927`
  The `location == "global"` check was case-sensitive and the raw location was
  interpolated into the hostname, so "Global" → `Global-aiplatform...` and
  "US-EAST5" → `US-EAST5-aiplatform...` — both invalid hosts.
  **Fix:** `location.to_ascii_lowercase()` before the global check and host
  interpolation.

- [x] **Cold-start token mint has no in-flight coalescing** — FIXED (MINT_LOCK single-flight)
  — `app/src/ai/agent_providers/vertex_auth.rs:47`
  On a cold cache, concurrent first requests (main stream + title gen +
  active-AI) each missed and spawned their own `gcloud auth print-access-token`
  subprocess.
  **Fix:** single-flight the mint per credential (per-credential async lock or
  in-flight map) so only one `gcloud` runs.

- [x] **8-field positional provider-edit payload duplicated ~4×** — RESOLVED on `main`
  (issue `#100`, commit `a08b52777`, PR #104; verified 2026-08-06 with
  `git merge-base --is-ancestor a08b52777 main`)
  — `app/src/settings_view/ai_page.rs:2425`
  `SaveAgentProviderEdits` / `SaveAgentProviderEditsThen` / the
  `to_save_action_with` closure type / `save_agent_provider_edits` all carried the
  same 8 positional fields, kept in lockstep by hand (needing
  `#[allow(clippy::too_many_arguments)]`). A mismatched order silently swapped
  values.
  **Fix (applied):** collapsed into a single `ProviderEditFields` struct passed by
  value (now in `app/src/settings_view/ai_page.rs` +
  `app/src/settings_view/agent_providers_widget.rs`).

- [x] **Vertex family routing duplicated** — FIXED (shared vertex_model_family())
  — `app/src/ai/agent_providers/reasoning.rs:100`
  (and `app/src/ai/agent_providers/attachment_caps.rs:225`)
  The `contains("claude") ? Anthropic : Gemini` dispatch was implemented verbatim
  in both; a change to the heuristic had to touch both or the surfaces disagreed.
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
  Only residual: symlink-follow — since fixed, see the defense-in-depth item above.
