# Sweep outcome — the tail package (10 tests, 8 files)

Oracle: `02b53fcd8` (Warp `2026.07.29.09.05` stable), per `ORACLE.md`. Never
`warp/master`. Branch: `sweep-tail-2026-08-11`, based on local `main`
`17025cd66f512c48c6fa3e94acc06517935e9c91`.

Per-test outcome table, then evidence for each. "PORTED" means source + test
landed on the branch and `rustfmt --check` is clean on the changed files.
"RE-ADJUDICATED" means the assigned brief's framing turned out to be wrong or
incomplete once the pin source and fork state were actually read.

| # | test | file | outcome |
|---|---|---|---|
| 1 | `test_periodic_ping` | `shared_session/network/heartbeat_tests.rs` | **PORTED** |
| 1 | `test_idle_timeout` | `shared_session/network/heartbeat_tests.rs` | **PORTED** |
| 2 | `setup_command_groups_have_independent_visibility` | `ambient_agent/block/setup_command_text_tests.rs` | **PORTED** |
| 2 | `setup_command_groups_track_running_group_independently` | `ambient_agent/block/setup_command_text_tests.rs` | **PORTED** |
| 3 | `file_contents_from_response_keeps_only_whole_text_files` | `get_relevant_files/remote_search/native_tests.rs` | **PORTED** (relocated) |
| 4 | `extension_for_content_type_recognizes_image_jpg_alias` | `ai/artifact_download_tests.rs` | RE-ADJUDICATED → CLOUD |
| 5 | `unavailable_bundled_context_path_renders_as_empty_string` | `ai/skills/bundled_tests.rs` | RE-ADJUDICATED → already declined in-file, no action |
| 6 | `prompt_history_candidates_seeds_from_snapshot_then_appends_session_prompts` | `ai/blocklist/history_model_tests.rs` | RE-ADJUDICATED → COVERED-ELSEWHERE (partial) |
| 7 | `find_git_root_walks_up_to_dot_git` | `ai/blocklist/handoff/touched_repos_tests.rs` | RE-ADJUDICATED → COVERED-ELSEWHERE (leaf) / CLOUD (container) |
| 8 | `cli_agent_transcript_vehicle_is_excluded_from_navigation` | `ai/agent/conversation_tests.rs` | RE-ADJUDICATED → DECLINED (maintainer #107, already decided) |

**6 of 10 tests are now resolved** (3 ported, 3 already covered/declined by
existing, verifiable decisions). 4 tests stand on architecture the fork
deliberately does not have (cloud) — those are correctly not ported.

---

## 1. `heartbeat_tests.rs` — PORTED

`shared_session/network/heartbeat.rs` at the pin has its own top-of-file
comment: *"Session sharing now relies on the server sending protocol-level
ping frames... This module is no longer used, but we keep the code around for
now"* — `#![allow(dead_code)]`, already dead at the pin itself. Its imports
are `std::time::Duration`, `futures::stream::AbortHandle`,
`warpui::r#async::Timer`, `warpui::{Entity, ModelContext}` — none of them
cloud. The cloud coupling in `shared_session` lives in the sibling
sharer/viewer modules the fork already correctly dropped (SCOPE-TERMINAL.md
verdict **D**, non-cloud but source-missing, already scored — no `DECLINED.md`
row existed for it).

Ported verbatim: `app/src/terminal/shared_session/network/{mod.rs,heartbeat.rs,heartbeat_tests.rs}`,
wired via `pub mod network;` in `shared_session/mod.rs`. Both tests carry the
pin's own `#[ignore = "Flakes in CI"]`, unchanged — not weakened, just
preserved as the pin had them (they were never claimed to be reliable
upstream either).

Files: `app/src/terminal/shared_session/mod.rs`,
`app/src/terminal/shared_session/network/mod.rs`,
`app/src/terminal/shared_session/network/heartbeat.rs`,
`app/src/terminal/shared_session/network/heartbeat_tests.rs`.

## 2. `setup_command_text_tests.rs` — PORTED (leaf-only)

`SetupCommandGroupId`/`SetupCommandState` (the pure state machine the two
tests exercise) have zero cloud imports (only `std::collections::HashMap`).
The task brief's premise — "its sole consumer `AmbientAgentViewModel` is also
absent" — is **stale**: `AmbientAgentViewModel` exists in the fork at
`app/src/terminal/view/ambient_agent/model.rs` (745 lines, real, not a stub).
`SCOPE-TERMINAL.md` already scores this file verdict **D** and notes the
fork keeps `view/ambient_agent/`.

Ported only `SetupCommandGroupId`/`SetupCommandState` and the 2 tests — not
the pin's `CloudModeSetupTextBlock` (a `View`/`TypedActionView` around it,
which pulls in `AgentViewController`, `BlocklistAIHistoryModel`,
`inline_action_icons`, `warp_core::ui::Icon`). That UI wiring is a separate,
larger, unverified change and isn't needed to make these two tests pass;
porting it speculatively risks silently getting the view wrong with no test
coverage to catch it. Left `#![allow(dead_code)]` on the new module since
there is currently no caller in the fork — a future block view can pick the
state machine up without re-deriving it.

Files: `app/src/terminal/view/ambient_agent/block.rs`,
`app/src/terminal/view/ambient_agent/block/setup_command_text.rs`,
`app/src/terminal/view/ambient_agent/block/setup_command_text_tests.rs`.

## 3. `get_relevant_files/remote_search/native_tests.rs` — PORTED (relocated, narrow)

The pin's `remote_search/native.rs` as a whole is CLOUD:
`execute_remote_codebase_search` drives `StoreClient`/`ServerApi` embedding
rerank, and the containing `get_relevant_files/` directory doesn't exist in
the fork at all — `codebase_retrieval.rs`'s module doc explicitly documents
it as retired, replaced by a daemon-side `SearchRemoteCodebase` RPC that
returns already-reranked results (no raw-fragment/whole-file split needed).

But the one function this test exercises, `file_contents_from_response`, is a
pure ~10-line filter over `remote_server::proto` wire types
(`ReadFileContextResponse`/`FileContextProto`) — confirmed field-for-field
identical between pin and fork proto (`crates/remote_server/proto/remote_server.proto`
messages `ReadFileContextFile`/`LineRange`/`ReadFileContextResponse`/
`FailedFileRead`/`FileContextProto`, diffed directly). It has no dependency on
the cloud-gated rerank pipeline.

Ported it standalone as a new module rather than resurrecting the retired
directory: `app/src/ai/get_relevant_files_file_contents.rs` +
`get_relevant_files_file_contents_tests.rs`, registered in `app/src/ai/mod.rs`.
**Note:** this function currently has no caller — the fork's remote
codebase-search leg doesn't do fragment-vs-whole-file reconciliation the pin's
did, so this is logic/test restoration, not a wired feature. Documented in the
new file's module doc.

Files: `app/src/ai/mod.rs`, `app/src/ai/get_relevant_files_file_contents.rs`,
`app/src/ai/get_relevant_files_file_contents_tests.rs`.

## 4. `artifact_download_tests.rs` — RE-ADJUDICATED → CLOUD

The task brief's premise ("ledger says `display_optional_path` absent from
`bundled.rs`... looks like a small pure-function gap") conflates this item
with item 5 — `display_optional_path` lives in `skills/bundled.rs`, not
`artifact_download.rs`. The actual function under test here,
`extension_for_content_type`, is itself pure (a `&str` → `Option<&'static str>`
match), but its **only** caller in the pin is
`default_download_filename(artifact: &ArtifactDownloadResponse)`, where
`ArtifactDownloadResponse` is `crate::server::server_api::ai::ArtifactDownloadResponse`
— Warp's cloud artifact-store download response (signed URLs for chat
file/screenshot uploads), fetched via `download_artifact_bytes` over HTTP from
Warp's servers. There is no local-fs use case for mapping a server-supplied
MIME content-type to a download extension once artifact-download-from-cloud
is gone.

Confirmed the fork's `app/src/ai/artifact_download.rs` is already stripped to
just `sanitized_basename` (verbatim ported, tested inline) — `extension_for_content_type`,
`default_download_filename`, `download_destination`, `download_artifact_bytes`,
and the `ArtifactDownloadResponse` import are all absent, consistently. No
`_tests.rs` file exists for this module in the fork.

**Verdict: CLOUD.** Not ported.

## 5. `skills/bundled_tests.rs` — RE-ADJUDICATED → no action, already declined in-file

The fork's `app/src/ai/skills/bundled_tests.rs` already documents, in its own
header comment (lines 8-19), exactly why this pin test was not ported:
`display_optional_path` only exists because the pin's
`build_bundled_skill_context` has `Option<PathBuf>`-typed GUI/TUI
config-dir variables (`gui_config_local_dir`/`tui_config_local_dir`) that this
fork doesn't build — consistent with the fork's documented decision to share
one app id/config dir between GUI and TUI (`DECLINED.md`, "TUI/GUI shared GUI
app_id" row). The fork's `build_bundled_skill_context()` takes no such
optional-path args at all.

This is architecture, not cloud — bundled skills are resources shipped with
the binary either way — but it's a real, already-recorded decision, just not
yet promoted to a `DECLINED.md` row of its own. Left as-is rather than
touching `DECLINED.md` in this pass: `DECLINED.md` itself has unrelated,
active concurrent edits in this shared working tree right now (verified via
`git status` before starting — `DECLINED.md`/`TODO.md`/`docs/STATE.md` were
already dirty before this session touched anything), so editing it risked
colliding with another in-flight process. The evidence trail (the header
comment cited above) is sufficient for the next reader without that edit.

**Verdict: not a gap. No code change.**

## 6. `history_model_tests.rs` — RE-ADJUDICATED → COVERED-ELSEWHERE (partial)

`app/src/ai/blocklist/history_model_test.rs:3353`
(`prompt_history_candidates_seeds_from_snapshot`) already ports the
snapshot-seeding half of this pin test, with its own doc comment tracing it
back to `history_model_tests.rs:965` at `02b53fcd8`, for issue #256 item 2.
Verified directly: the getter `prompt_history_candidates()` is identical
(`self.prompt_history.clone()`), and the fork test asserts the same
whitespace-filtering/oldest-first seeding behavior.

The half **not** ported — the live-session append via `append_session_prompt`
(called from `update_conversation_for_new_request_input`) — is deliberately
out of scope per that test's own doc comment: "superseded by #336/#337/#331"
(SQLite-backed `nld_prompts` persistence not yet landed; `new()` is currently
always called with an empty snapshot in production). This matches
`TODO.md`'s "history_model reconciliation" keep-dropped entry, which
separately lists `prompt_history_candidates`' constructor-arity bits as
having "no consumer (tie to the undecided NLD-flags item)."

**Verdict: COVERED-ELSEWHERE** for the ported half — cite
`app/src/ai/blocklist/history_model_test.rs:3353`
(`prompt_history_candidates_seeds_from_snapshot`). The append half stays
correctly unported pending #336/#337/#331. No code change needed here.

## 7. `handoff/touched_repos_tests.rs` — RE-ADJUDICATED → COVERED-ELSEWHERE (leaf) / CLOUD (container)

`app/src/ai/blocklist/handoff/` is entirely absent from the fork.
`SCOPE-AI.md` already scores both files in that directory (`pipeline_tests.rs`,
18 tests; `touched_repos_tests.rs`, 1 test) verdict **C** (cloud), citing the
module's imports of `crate::cloud_object::CloudObjectLookup`,
`crate::server::ids::{ServerId,SyncId}`, and (for `touched_repos.rs`
specifically) `crate::ai::cloud_environments::CloudAmbientAgentEnvironment` —
this is the client side of the pin's local-to-cloud Oz conversation handoff
(fork/upload a conversation to a cloud run). `find_git_root` itself is pure
and private, reachable only through that cloud-coupled module.

An equivalent walk-up-to-`.git` helper already exists and is tested in the
fork: `app/src/tab_configs/session_config.rs:192`, `is_git_repo(path: &Path) -> bool`
— walks `path.join(".git").is_dir()` up through `.parent()` to root, with the
same in-repo-file / in-repo-nested-dir / outside-repo-file semantics the pin
test exercises (differs only in returning `bool` vs `Option<PathBuf>`, and
sync vs async). Verified directly: `app/src/tab_configs/session_config_tests.rs`
has `detects_git_repo_with_dot_git_dir` (916), `detects_non_git_directory`
(924), `detects_git_repo_in_parent` (931), `root_directory_does_not_loop`
(941) — covering the same walk-up scenarios.

**Verdict: COVERED-ELSEWHERE** for the leaf algorithm — cite
`app/src/tab_configs/session_config_tests.rs:916,924,931,941`. The containing
`handoff/touched_repos.rs` module stays **CLOUD**, per `SCOPE-AI.md`'s
existing verdict. No code change.

## 8. `conversation_tests.rs` — RE-ADJUDICATED → DECLINED (already decided, #107)

The task brief characterized this as "straightforward": add a 2nd
`is_cli_agent_transcript: bool` param/field to `AIConversation::new`. **This
exact widening was already proposed and explicitly declined by the
maintainer.** Found while enumerating `AIConversation::new(` call sites to
check what a port would need to touch: `app/src/terminal/view_test.rs:7513-7520`
carries a `NOTE(adapted)` comment —

> "the oracle passes a 4th `is_cli_agent_transcript: bool` here, which feeds
> `AIConversation::new(is_viewing_shared_session, is_cli_agent_transcript)`.
> This fork's constructor is deliberately narrower: **#107 was closed
> NOT_PLANNED on 2026-08-06 as a maintainer KEEP-DROPPED decision** ('the
> wider-arity constructor bits have no consumer in the fork'), recorded on
> #11."

Corroborated in `TODO.md`'s "Keep-dropped (decided this session)" section
(lines 2613-2622): *"The constructor-arity bits (`start_new_conversation`/
`prompt_history_candidates`) have no consumer (tie to the undecided NLD-flags
item). Recorded on #11; tracking issue #107 closed."* — the same
constructor-arity decision also explains item 6's un-ported append half
above; both trace back to the same #107/#11 call.

I initially implemented the port (field + param + accessor + the
`should_exclude_from_navigation` clause + ~11 call-site fixups) before
finding this, then **reverted it in full** on discovering the standing
decision — adding it back would silently re-open something a maintainer
closed NOT_PLANNED, which is exactly the "gap that's actually a decision"
trap `DECLINED.md`'s own preamble warns about. `is_cli_agent_transcript`
would be genuinely dead weight here too: its only purpose at the pin is
letting a CLI-transcript *viewer* construct a placeholder conversation for
agent-view navigation filtering, and this fork has no such viewer
constructing `AIConversation`s in the first place.

**Verdict: DECLINED** (re-affirms #107, not a new decision). Not ported.
This decision is not yet promoted to its own `DECLINED.md` row — flagging
here rather than adding one in this pass, since `DECLINED.md` had unrelated
concurrent edits in-flight in this shared working tree throughout this
session; a follow-up should add a row citing `view_test.rs:7513-7520` +
`TODO.md:2613-2622` + this test name so it stops looking like open debt.

---

## What remains open

- **Nothing from this package needs further code work to close a real gap.**
  Items 4, 5, 6, 7, 8 are all correctly-resolved non-gaps (cloud, or already
  covered/declined elsewhere) as of this investigation — they were open only
  because the ledger/brief hadn't caught up to existing decisions and tests.
- **Housekeeping left for a follow-up, not blocking:** promote item 8's
  decision (and arguably item 5's) to a proper `DECLINED.md` row with
  `sym:`/`test:` markers, once `DECLINED.md` isn't being concurrently edited
  by another process. Also worth updating `docs/sweep-verdict-ledger.tsv`
  for items 4/6/7/8 to their re-adjudicated verdicts — explicitly not done
  here per this task's constraints.
- **Item 3's ported function has no caller** (`file_contents_from_response`)
  — noted in its own module doc; wiring it up would mean building a
  fragment-expansion path into `codebase_retrieval.rs`, which is new feature
  work, not this port.
- **Item 2's ported state machine has no caller** either, for the same
  reason (`CloudModeSetupTextBlock`'s view wasn't ported) — noted in its
  module comment.
- `rustfmt --check` was run against every changed/new file below and is
  clean. Full-suite build/test was **not** run — that's the operator's gate
  per this environment's hard rules (no `cargo`/`nextest`/`script/precheck`
  here).

## Files touched

- `app/src/terminal/shared_session/mod.rs` (wire `pub mod network;`)
- `app/src/terminal/shared_session/network/mod.rs` (new)
- `app/src/terminal/shared_session/network/heartbeat.rs` (new, verbatim port)
- `app/src/terminal/shared_session/network/heartbeat_tests.rs` (new, verbatim port)
- `app/src/terminal/view/ambient_agent/block.rs` (wire `mod setup_command_text;`)
- `app/src/terminal/view/ambient_agent/block/setup_command_text.rs` (new, leaf port)
- `app/src/terminal/view/ambient_agent/block/setup_command_text_tests.rs` (new, verbatim port)
- `app/src/ai/mod.rs` (wire `pub(crate) mod get_relevant_files_file_contents;`)
- `app/src/ai/get_relevant_files_file_contents.rs` (new, narrow port)
- `app/src/ai/get_relevant_files_file_contents_tests.rs` (new, verbatim port)
