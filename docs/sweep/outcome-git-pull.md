# Outcome: `git pull --ff-only` (Stage 1)

Task: implement fast-forward-only `git pull`, mirroring the existing `git
push` implementation layer-by-layer. Merging/conflicting pull (Stage 2+) is
explicitly out of scope — a fast-forward can't conflict, which is the whole
point of doing this stage first.

Status: **code + tests written, unverified** — the build freeze means none of
this has been compiled or run. Everything below is "should work by
construction, mirroring an already-working sibling path," not "confirmed
working."

## What was built, per layer

### Local git — `app/src/util/git.rs`
Added `run_pull(repo_path, branch, path_env)`, mirroring `run_push` exactly:
runs `git pull --ff-only origin <branch>` via the existing
`run_git_command_with_env`, forwarding `path_env` for the same reason push
does (a post-merge hook, e.g. LFS, needs the user's `PATH`). `--ff-only` is
the load-bearing flag — git refuses non-fast-forward pulls outright rather
than merging, so there is no merge-conflict state this needs to detect or
recover from.

Tests added to `app/src/util/git_tests.rs`:
- `run_pull_fast_forwards_to_new_upstream_commit` — a second clone pushes to
  the shared bare origin, `run_pull` succeeds, the new file and commit land
  locally.
- `run_pull_fails_without_merging_on_diverged_history` — local and origin
  diverge independently; `run_pull` returns `Err`, and `HEAD` plus the
  working tree are asserted unchanged (no merge commit, no partial pull).

There is no pre-existing `run_push`-specific unit test in this file to mirror
directly (push is only exercised indirectly through `run_commit_chain` and
`add_bare_origin`), so these two tests instead mirror that file's existing
bare-origin test style, plus add a `clone_repo` helper to simulate "someone
else pushed."

### Proto — `crates/remote_server/proto/remote_server.proto`
Added `GitPullRequest` / `GitPullResponse`, reusing `GitOpDelta`/`GitOpError`
exactly like `GitPushRequest`/`GitPushResponse` (no conflict payload needed,
per the Stage-1 scope). Wired into the two oneofs at the next free slots:
`HostScopedRequest.git_pull = 22`, `ServerMessage.git_pull_response = 37`.
`prost`'s field-name → variant-name mapping was checked against `git_push` →
`GitPush` first (per the task's naming-convention note); `git_pull` follows
the identical pattern, so `host_scoped_request::Message::GitPull` and
`server_message::Message::GitPullResponse` are what the generated code will
produce (unverified — codegen only runs at `cargo build`, which the build
freeze blocks).

### Daemon — `app/src/remote_server/server_model.rs`
Added `handle_git_pull`, byte-for-byte structural mirror of `handle_git_push`
including the `guard_git_operation_in_progress` lock, dispatched from the
same `HostScoped` match arm block as `GitPush`.

### Client — `crates/remote_server/src/client/mod.rs`
Added `RemoteServerClient::git_pull`, mirroring `git_push`. Tests added to
`crates/remote_server/src/client_tests.rs`: `git_pull_round_trip` (success,
mirrors the existing `git_push_round_trip`) and `git_pull_round_trip_error`
(diverged-history error, mirrors `git_create_pr_round_trip_error`'s error
pattern — there's no push-specific error round-trip test to mirror, since a
push has no equivalent "refuses because it would need to merge" case).

### UI — `app/src/code_review/git_dialog/pull.rs` (new file)
New `GitDialog` mode, structurally mirroring `push.rs`'s local/remote split
at `start_confirm_remote`. Simpler than push's body: no expandable commit
list, because unlike push (which already knows the exact local commits about
to be sent) a pull's incoming commits aren't known until the fetch/merge
actually happens — the dialog just confirms "pull `<branch>`" and reports the
resulting toast.

Wired through the same generic infrastructure every other `GitDialog` mode
uses: `GitDialogKind::Pull` → `CodeReviewView::open_git_dialog` →
`GitDialog::new_for_pull` → `GitDialogMode::Pull(PullState)`. Entry point:
`CodeReviewAction::OpenPullDialog`, exposed as a "Pull" item in the git
operations dropdown (`git_operations_menu_items`), disabled when the branch
has no upstream. Added `common-pull = Pull` to `app/i18n/en/warp.ftl` (no
zh-CN/ja translations added — per `CLAUDE.local.md`, those are intentional
translations left alone; a new English key with no counterpart yet is
expected to fall back like any other untranslated key).

**Known gap, called out explicitly rather than silently left**: the chevron
dropdown itself is hidden entirely in three of the five
`PrimaryGitActionMode` states (`CreatePr`, `ViewPr`, `Publish` —
`update_git_operations_ui` clears the adjoined side and never shows a
chevron in those modes). Pull was only added to the two modes where the
chevron already renders (`Commit`, `Push`). Extending Pull's reachability
into the other three states means changing when the chevron itself shows,
which is unrelated pre-existing UI behavior outside this task's scope
("mirror push", not "redesign the primary-button state machine"). In those
three states the repo has nothing to commit or push, so pulling is still
reachable via the terminal in the meantime.

## Buffer/working-tree invalidation (the step called out as most likely to be forgotten)

Push never touches the working tree; pull does, and the task flagged this
explicitly. Investigated before writing any code, documented in
`pull.rs`'s module doc comment:

**Local**: `GitDialogEvent::Completed` (emitted by `pull::start_confirm` on
both success and failure, same as every other mode) is handled by
`CodeReviewView::open_git_dialog`'s subscription, which calls
`refresh_after_git_operation`. That already calls
`load_diffs_for_active_repo`, which does a full diff/content-cache reload for
every file — a superset of feeding individual changed files to
`FileInvalidationTask`. This path is generic across every `GitDialog` mode;
wiring Pull through it (rather than a bespoke "run `run_pull`, show a toast"
path bypassing `GitDialogEvent`) is what makes this work, and is exactly the
trap the task's wording anticipated an agent could fall into.

**Remote (SSH daemon)**: the daemon's per-repo `DiffStateWatch`
(`app/src/remote_server/diff_state_tracker.rs`) is a real filesystem watcher
already relied on today to catch working-tree changes from *any* source —
`discard`, or literally running `git pull` by hand in a terminal on the
daemon host — and push `DiffStateFileDelta`/`DiffStateSnapshot` to
subscribers. It doesn't care who or what changed the files, so a
daemon-executed `run_pull` is picked up the same way with zero new code on
the daemon side. `refresh_after_git_operation` still runs on the client
afterward regardless (same as the local case), so the UI doesn't depend on
watcher timing.

Net result: no bespoke file-invalidation code was needed in either
`handle_git_pull` or `pull.rs`'s RPC handling — but that's a conclusion
reached by reading `diff_state_tracker.rs` and `code_review_view.rs`'s
existing watcher plumbing, not an assumption. Both are cited by path/comment
at the point in `pull.rs` where this could have been silently skipped.

## Verification status

- **Not run**: `cargo build`/`check`/`test`, `nextest`, `rustc` — build
  freeze forbids this for the whole task. No test in this change has been
  executed; "written, unverified" for all of them.
- **Run, passed**: `script/check_cloud_boundary` (271 allowlisted import
  sites, no new cloud imports), `script/check_stub_coverage` (no test
  targets a gutted stub).
- **`rustfmt --check`**: run on every touched file. This local `rustfmt`
  build disagrees with the repo's committed import ordering (case-sensitive
  vs. the committed case-insensitive-ish convention) and reflows long import
  lists differently — confirmed pre-existing by diffing untouched files
  (`push.rs`, `pr.rs`, `commit.rs`, and large unrelated stretches of
  `code_review_view.rs` all show the same class of diff without any edits
  from this task). Cross-checked every reported diff location against this
  change's actual line ranges (`git diff -U0`): no reported diff overlaps a
  line this task touched, and where an import line I *did* edit shows up in
  the noise, it's because I matched the already-committed ordering
  convention (verified against `push.rs`'s untouched import block), not
  because of a real formatting defect. New file `pull.rs` was written
  matching `push.rs`'s import-ordering convention directly rather than
  trusting local `rustfmt`'s suggestion.

## Unfinished / explicitly out of scope

- Stage 2 (merging/conflicting pull) — not started, per the task's explicit
  scope boundary.
- Pull's dropdown entry is unreachable in the `CreatePr`/`ViewPr`/`Publish`
  primary-action states (see UI section above).
- No zh-CN/ja translation added for the new `common-pull` key.
- Nothing in this change has been compiled, run, or otherwise verified beyond
  reading and the two shell-only guard scripts above.
