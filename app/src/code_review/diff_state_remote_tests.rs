//! Tests for [`RemoteDiffStateModel`] — state folding and branch listing.
//!
//! Harness shape (`new_for_test` + metadata builders + `warpui::App::test`) is
//! ported from Warp's `warp/master:app/src/code_review/diff_state/remote_tests.rs`,
//! which the fork had never taken. Constructing the model directly rather than
//! through `new` deliberately skips the `GetDiffState` subscription RPC, so the
//! state-folding logic can be exercised without a connected host. The response
//! test mirrors Warp's `get_committed_branch_files_response_emits_domain_files`:
//! feed a proto response into the handler and assert the domain event it emits.

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use remote_server::manager::RemoteServerManagerEvent;
use warp_core::{HostId, SessionId};
use warp_util::standardized_path::StandardizedPath;

use super::{InternalRemoteDiffState, RemoteDiffStateModel};
use crate::code::buffer_location::RemotePath;
use crate::code_review::diff_size_limits::DiffSize;
use crate::code_review::diff_state::{
    DiffMetadata, DiffMetadataAgainstBase, DiffMode, DiffState, DiffStateModelEvent, DiffStats,
    FileDiff, GitDiffData, GitFileStatus,
};
use crate::remote_server::diff_state_proto::snapshot_from_parts;
use crate::remote_server::proto;
use crate::util::git::Commit;

const TEST_REPO_PATH: &str = "/test/repo";

impl RemoteDiffStateModel {
    fn new_for_test_with(
        repo_path: &str,
        mode: DiffMode,
        state: InternalRemoteDiffState,
        metadata: Option<DiffMetadata>,
    ) -> Self {
        Self {
            remote_path: RemotePath::new(
                HostId::new("test-host".to_string()),
                StandardizedPath::try_new(repo_path)
                    .expect("test repo path should be valid and absolute"),
            ),
            mode,
            state,
            metadata,
        }
    }

    /// Branch-listing tests: default mode/state, path under test.
    fn new_for_test(repo_path: &str) -> Self {
        Self::new_for_test_with(
            repo_path,
            DiffMode::Head,
            InternalRemoteDiffState::Loading,
            None,
        )
    }

    /// State-folding tests: fixed path, caller-chosen mode/state/metadata.
    fn new_for_test_folding(
        mode: DiffMode,
        state: InternalRemoteDiffState,
        metadata: Option<DiffMetadata>,
    ) -> Self {
        Self::new_for_test_with(TEST_REPO_PATH, mode, state, metadata)
    }
}

fn commit(subject: &str, hash: &str) -> Commit {
    Commit {
        hash: hash.to_string(),
        subject: subject.to_string(),
        files_changed: 0,
        additions: 0,
        deletions: 0,
    }
}

/// Metadata for a branch that is `unpushed.len()` commits ahead of
/// `upstream_ref`, i.e. the state the UI shows before a push.
fn metadata_with(unpushed: Vec<Commit>, upstream_ref: Option<&str>) -> DiffMetadata {
    DiffMetadata {
        main_branch_name: "main".to_string(),
        current_branch_name: "feature".to_string(),
        against_head: DiffMetadataAgainstBase {
            aggregate_stats: DiffStats::default(),
        },
        against_base_branch: None,
        has_head_commit: true,
        unpushed_commits: unpushed,
        upstream_ref: upstream_ref.map(str::to_string),
        pr_info: None,
    }
}

#[test]
fn apply_git_op_delta_replaces_unpushed_commits_and_upstream() {
    warpui::App::test((), |mut app| async move {
        let handle = app.add_model(|_ctx| {
            RemoteDiffStateModel::new_for_test_folding(
                DiffMode::Head,
                InternalRemoteDiffState::Loading,
                Some(metadata_with(
                    vec![commit("local change", "aaa1111")],
                    Some("origin/feature"),
                )),
            )
        });

        // The daemon reports the post-push state: nothing left unpushed.
        handle.update(&mut app, |model, ctx| {
            model.apply_git_op_delta(Vec::new(), Some("origin/feature".to_string()), ctx)
        });

        let (unpushed, upstream) = handle.read(&app, |model, _| {
            (
                model.unpushed_commits().to_vec(),
                model.upstream_ref().map(str::to_string),
            )
        });
        assert!(
            unpushed.is_empty(),
            "a successful push must clear the unpushed set, got {unpushed:?}"
        );
        assert_eq!(upstream.as_deref(), Some("origin/feature"));
    });
}

#[test]
fn apply_git_op_delta_records_a_newly_created_upstream() {
    warpui::App::test((), |mut app| async move {
        // Branch has never been published: no upstream, one local commit.
        let handle = app.add_model(|_ctx| {
            RemoteDiffStateModel::new_for_test_folding(
                DiffMode::Head,
                InternalRemoteDiffState::Loading,
                Some(metadata_with(vec![commit("first", "bbb2222")], None)),
            )
        });

        // `git push --set-upstream` creates the tracking ref; the delta carries it.
        handle.update(&mut app, |model, ctx| {
            model.apply_git_op_delta(Vec::new(), Some("origin/feature".to_string()), ctx)
        });

        let upstream = handle.read(&app, |model, _| model.upstream_ref().map(str::to_string));
        assert_eq!(
            upstream.as_deref(),
            Some("origin/feature"),
            "publishing a branch must record the newly created upstream ref"
        );
    });
}

#[test]
fn apply_git_op_delta_keeps_commits_still_unpushed_after_a_commit_only_chain() {
    warpui::App::test((), |mut app| async move {
        let handle = app.add_model(|_ctx| {
            RemoteDiffStateModel::new_for_test_folding(
                DiffMode::Head,
                InternalRemoteDiffState::Loading,
                Some(metadata_with(
                    vec![commit("older", "ccc3333")],
                    Some("origin/feature"),
                )),
            )
        });

        // A commit-only chain adds to the unpushed set rather than clearing it.
        handle.update(&mut app, |model, ctx| {
            model.apply_git_op_delta(
                vec![commit("newer", "ddd4444"), commit("older", "ccc3333")],
                Some("origin/feature".to_string()),
                ctx,
            )
        });

        let subjects: Vec<String> = handle.read(&app, |model, _| {
            model
                .unpushed_commits()
                .iter()
                .map(|c| c.subject.clone())
                .collect()
        });
        assert_eq!(subjects, vec!["newer".to_string(), "older".to_string()]);
    });
}

#[test]
fn apply_git_op_delta_emits_metadata_changed() {
    warpui::App::test((), |mut app| async move {
        let handle = app.add_model(|_ctx| {
            RemoteDiffStateModel::new_for_test_folding(
                DiffMode::Head,
                InternalRemoteDiffState::Loading,
                Some(metadata_with(
                    vec![commit("local change", "aaa1111")],
                    Some("origin/feature"),
                )),
            )
        });

        // The code-review UI repaints off this event; without it the view keeps
        // rendering the pre-operation unpushed count even though the model moved.
        // Subscriptions live on a context rather than on `App`, so the model
        // subscribes to itself from inside an update (the pattern used by
        // `settings::ai_tests`).
        let saw_metadata_changed = Rc::new(Cell::new(false));
        let observed = saw_metadata_changed.clone();
        let subscribed_handle = handle.clone();
        handle.update(&mut app, move |_, ctx| {
            ctx.subscribe_to_model(
                &subscribed_handle,
                move |_, event: &DiffStateModelEvent, _| {
                    if matches!(event, DiffStateModelEvent::DiffMetadataChanged(_)) {
                        observed.set(true);
                    }
                },
            );
        });

        handle.update(&mut app, |model, ctx| {
            model.apply_git_op_delta(Vec::new(), Some("origin/feature".to_string()), ctx)
        });

        assert!(
            saw_metadata_changed.get(),
            "applying a git-op delta must emit DiffMetadataChanged so the view repaints"
        );
    });
}

#[test]
fn apply_git_op_delta_before_any_snapshot_seeds_metadata() {
    warpui::App::test((), |mut app| async move {
        // No metadata yet: the initial GetDiffState snapshot has not landed.
        // Warp seeds default metadata (`metadata.get_or_insert_with`) so the
        // delta is never dropped; the fork must do the same.
        let handle = app.add_model(|_ctx| {
            RemoteDiffStateModel::new_for_test_folding(
                DiffMode::Head,
                InternalRemoteDiffState::Loading,
                None,
            )
        });

        handle.update(&mut app, |model, ctx| {
            model.apply_git_op_delta(
                vec![commit("orphan", "eee5555")],
                Some("origin/feature".to_string()),
                ctx,
            )
        });

        let (subjects, upstream) = handle.read(&app, |model, _| {
            (
                model
                    .unpushed_commits()
                    .iter()
                    .map(|c| c.subject.clone())
                    .collect::<Vec<_>>(),
                model.upstream_ref().map(str::to_string),
            )
        });
        assert_eq!(subjects, vec!["orphan".to_string()]);
        assert_eq!(upstream.as_deref(), Some("origin/feature"));
    });
}

/// Collects every `BranchesReceived` payload emitted by `handle`.
fn subscribe_to_branches(
    app: &mut warpui::App,
    handle: &warpui::ModelHandle<RemoteDiffStateModel>,
) -> Arc<Mutex<Vec<Vec<(String, bool)>>>> {
    let received = Arc::new(Mutex::new(Vec::new()));
    let received_for_subscription = received.clone();
    app.update(|ctx| {
        ctx.subscribe_to_model(handle, move |_, event, _| {
            if let DiffStateModelEvent::BranchesReceived(branches) = event {
                received_for_subscription
                    .lock()
                    .expect("branches mutex should not be poisoned")
                    .push(branches.clone());
            }
        });
    });
    received
}

fn success_response(branches: &[(&str, bool)]) -> proto::GetBranchesResponse {
    proto::GetBranchesResponse {
        result: Some(proto::get_branches_response::Result::Success(
            proto::GetBranchesSuccess {
                branches: branches
                    .iter()
                    .map(|(name, is_main)| proto::BranchInfo {
                        name: (*name).to_string(),
                        is_main: *is_main,
                    })
                    .collect(),
            },
        )),
    }
}

#[test]
fn get_branches_request_targets_the_remote_repo_path() {
    let model = RemoteDiffStateModel::new_for_test(TEST_REPO_PATH);
    let request = model.get_branches_request();

    // The request must carry the *remote* path — the whole point of routing
    // through the RPC instead of running git on the client.
    assert_eq!(request.repo_path, TEST_REPO_PATH);
    // Same parameters the local backend uses: server default count, local
    // branches only.
    assert_eq!(request.max_branch_count, None);
    assert!(!request.include_remotes);
}

#[test]
fn get_branches_response_emits_domain_branches() {
    warpui::App::test((), |mut app| async move {
        let handle = app.add_model(|_ctx| RemoteDiffStateModel::new_for_test(TEST_REPO_PATH));
        let received = subscribe_to_branches(&mut app, &handle);

        // Success: proto entries are converted to `(name, is_main)` pairs,
        // preserving the daemon's ordering and main-branch flag.
        let response = success_response(&[("main", true), ("feature/one", false)]);
        handle.update(&mut app, |model, ctx| {
            model.handle_get_branches_response(&response, ctx);
        });

        // Error: an empty list is emitted so the dropdown falls back to its
        // defaults rather than keeping stale branches.
        let error = proto::GetBranchesResponse {
            result: Some(proto::get_branches_response::Result::Error(
                proto::GetBranchesError {
                    message: "boom".to_string(),
                },
            )),
        };
        handle.update(&mut app, |model, ctx| {
            model.handle_get_branches_response(&error, ctx);
        });

        // A response with no `result` set is treated like an error.
        let empty = proto::GetBranchesResponse { result: None };
        handle.update(&mut app, |model, ctx| {
            model.handle_get_branches_response(&empty, ctx);
        });

        let received = received
            .lock()
            .expect("branches mutex should not be poisoned");
        assert_eq!(received.len(), 3);
        assert_eq!(
            received[0],
            vec![
                ("main".to_string(), true),
                ("feature/one".to_string(), false),
            ]
        );
        assert!(received[1].is_empty());
        assert!(received[2].is_empty());
    });
}

#[test]
fn fetch_branches_never_reads_the_local_filesystem() {
    warpui::App::test((), |mut app| async move {
        app.add_singleton_model(remote_server::manager::RemoteServerManager::new);

        // Point the remote model at a path that *does* exist on the client and
        // is a git repository. If the remote backend fell back to the local
        // branch lookup (the bug this covers), it would happily list this
        // repo's branches. With no connected session for the host it must
        // instead emit nothing at all.
        let local_repo = std::env::current_dir()
            .expect("current dir should be readable")
            .to_string_lossy()
            .into_owned();
        let handle = app.add_model(|_ctx| RemoteDiffStateModel::new_for_test(&local_repo));
        let received = subscribe_to_branches(&mut app, &handle);

        handle.update(&mut app, |model, ctx| model.fetch_branches(ctx));

        let received = received
            .lock()
            .expect("branches mutex should not be poisoned");
        assert!(
            received.is_empty(),
            "remote backend must not fall back to the client's filesystem"
        );
    });
}

// ── Ported from the pinned oracle
// (warp/master:app/src/code_review/diff_state/remote_tests.rs @ 02b53fcd8) ──
//
// The pin's `RemoteDiffStateModel::apply_snapshot`/`apply_metadata_update`
// take already-decoded domain values directly. The fork's equivalents
// (`apply_snapshot`, `apply_metadata`) instead take the raw
// `proto::DiffStateSnapshot` pushed by the daemon and decode it internally,
// so these tests build snapshots with `snapshot_from_parts` (the same
// `pub(crate)` builder the daemon itself uses) rather than calling
// `apply_snapshot` with bare domain values. The fork's `DiffState` also has
// no `NotInRepository`-without-diffs-becomes-error case (`Loaded` always
// carries its `GitDiffData`, so the "loaded but empty" invalid state the pin
// guards against is unrepresentable) and no `Disconnected` variant (a
// disconnected model reports `Loading`, preserving stale data) — both are
// called out at their adapted test below rather than ported literally.

fn empty_metadata(branch: &str) -> DiffMetadata {
    DiffMetadata {
        main_branch_name: "main".to_string(),
        current_branch_name: branch.to_string(),
        against_head: DiffMetadataAgainstBase::default(),
        against_base_branch: None,
        has_head_commit: true,
        unpushed_commits: vec![],
        upstream_ref: None,
        pr_info: None,
    }
}

fn test_metadata(branch: &str) -> DiffMetadata {
    DiffMetadata {
        main_branch_name: "main".to_string(),
        current_branch_name: branch.to_string(),
        against_head: DiffMetadataAgainstBase {
            aggregate_stats: DiffStats {
                files_changed: 1,
                total_additions: 5,
                total_deletions: 2,
            },
        },
        against_base_branch: None,
        has_head_commit: true,
        unpushed_commits: vec![commit("test commit", "abc123")],
        upstream_ref: Some("origin/feature".to_string()),
        pr_info: None,
    }
}

fn simple_file_diff(path: &str) -> FileDiff {
    FileDiff {
        file_path: PathBuf::from(path),
        status: GitFileStatus::Modified,
        hunks: Arc::new(vec![]),
        is_binary: false,
        is_autogenerated: false,
        max_line_number: 10,
        has_hidden_bidi_chars: false,
        size: DiffSize::Normal,
    }
}

#[test]
fn apply_snapshot_loaded_with_diffs() {
    warpui::App::test((), |mut app| async move {
        let handle = app.add_model(|_ctx| {
            RemoteDiffStateModel::new_for_test_folding(
                DiffMode::Head,
                InternalRemoteDiffState::Loading,
                None,
            )
        });
        let diff_data = GitDiffData {
            files: vec![simple_file_diff("src/main.rs")],
            total_additions: 1,
            total_deletions: 0,
            files_changed: 1,
        };
        let snapshot = snapshot_from_parts(
            TEST_REPO_PATH.to_string(),
            &DiffMode::Head,
            Some(empty_metadata("feature")),
            Some(diff_data),
        );
        handle.update(&mut app, |m, ctx| m.apply_snapshot(&snapshot, ctx));
        handle.read(&app, |m, _| {
            assert!(matches!(m.get(), DiffState::Loaded(_)));
        });
    });
}

#[test]
fn apply_snapshot_not_in_repository() {
    warpui::App::test((), |mut app| async move {
        let handle = app.add_model(|_ctx| {
            RemoteDiffStateModel::new_for_test_folding(
                DiffMode::Head,
                InternalRemoteDiffState::Loading,
                None,
            )
        });
        // `snapshot_from_parts` builds a `NotInRepository` snapshot when there
        // is no metadata to report.
        let snapshot = snapshot_from_parts(TEST_REPO_PATH.to_string(), &DiffMode::Head, None, None);
        handle.update(&mut app, |m, ctx| m.apply_snapshot(&snapshot, ctx));
        assert!(matches!(
            handle.read(&app, |m, _| m.get()),
            DiffState::NotInRepository
        ));
    });
}

#[test]
fn apply_snapshot_error_stores_message() {
    warpui::App::test((), |mut app| async move {
        let handle = app.add_model(|_ctx| {
            RemoteDiffStateModel::new_for_test_folding(
                DiffMode::Head,
                InternalRemoteDiffState::Loading,
                None,
            )
        });
        // `snapshot_from_parts` reports `Error("Failed to compute diff")` when
        // metadata is present but the diff computation itself failed
        // (`diff_data: None`).
        let snapshot = snapshot_from_parts(
            TEST_REPO_PATH.to_string(),
            &DiffMode::Head,
            Some(empty_metadata("feature")),
            None,
        );
        handle.update(&mut app, |m, ctx| m.apply_snapshot(&snapshot, ctx));
        assert!(matches!(
            handle.read(&app, |m, _| m.get()),
            DiffState::Error(ref msg) if msg == "Failed to compute diff"
        ));
    });
}

#[test]
fn apply_snapshot_preserves_repo_relative_file_paths() {
    warpui::App::test((), |mut app| async move {
        let handle = app.add_model(|_ctx| {
            RemoteDiffStateModel::new_for_test_folding(
                DiffMode::Head,
                InternalRemoteDiffState::Loading,
                None,
            )
        });
        let diff_data = GitDiffData {
            files: vec![simple_file_diff("src/main.rs")],
            total_additions: 1,
            total_deletions: 0,
            files_changed: 1,
        };
        let snapshot = snapshot_from_parts(
            TEST_REPO_PATH.to_string(),
            &DiffMode::Head,
            Some(empty_metadata("feature")),
            Some(diff_data),
        );
        handle.update(&mut app, |m, ctx| m.apply_snapshot(&snapshot, ctx));
        handle.read(&app, |m, _| {
            let InternalRemoteDiffState::Loaded(diffs) = &m.state else {
                panic!("state should be Loaded");
            };
            assert_eq!(diffs.files.len(), 1);
            assert_eq!(diffs.files[0].file_path, PathBuf::from("src/main.rs"));
        });
    });
}

#[test]
fn apply_snapshot_emits_event_with_repo_relative_paths() {
    // Subscribers to NewDiffsComputed should see repo-relative paths so they
    // can index into the loaded state by the same key.
    warpui::App::test((), |mut app| async move {
        let handle = app.add_model(|_ctx| {
            RemoteDiffStateModel::new_for_test_folding(
                DiffMode::Head,
                InternalRemoteDiffState::Loading,
                None,
            )
        });
        let emitted_paths = Arc::new(Mutex::new(Vec::new()));
        {
            let emitted_paths = emitted_paths.clone();
            app.update(|ctx| {
                ctx.subscribe_to_model(&handle, move |_, event, _| {
                    if let DiffStateModelEvent::NewDiffsComputed(Some(diffs)) = event {
                        emitted_paths
                            .lock()
                            .expect("emitted paths mutex should not be poisoned")
                            .extend(
                                diffs
                                    .files
                                    .iter()
                                    .map(|file| file.file_diff.file_path.clone()),
                            );
                    }
                });
            });
        }

        let diff_data = GitDiffData {
            files: vec![simple_file_diff("src/main.rs")],
            total_additions: 1,
            total_deletions: 0,
            files_changed: 1,
        };
        let snapshot = snapshot_from_parts(
            TEST_REPO_PATH.to_string(),
            &DiffMode::Head,
            Some(empty_metadata("feature")),
            Some(diff_data),
        );
        handle.update(&mut app, |m, ctx| m.apply_snapshot(&snapshot, ctx));

        assert_eq!(
            emitted_paths
                .lock()
                .expect("emitted paths mutex should not be poisoned")
                .as_slice(),
            &[PathBuf::from("src/main.rs")]
        );
    });
}

#[test]
fn apply_metadata_first_time_sets_branch() {
    warpui::App::test((), |mut app| async move {
        let handle = app.add_model(|_ctx| {
            RemoteDiffStateModel::new_for_test_folding(
                DiffMode::Head,
                InternalRemoteDiffState::Loading,
                None,
            )
        });
        let meta = empty_metadata("feature");
        handle.update(&mut app, |m, ctx| m.apply_metadata(meta, ctx));
        assert_eq!(
            handle
                .read(&app, |m, _| m.get_current_branch_name())
                .as_deref(),
            Some("feature")
        );
    });
}

#[test]
fn apply_metadata_branch_change_updates_branch() {
    warpui::App::test((), |mut app| async move {
        let handle = app.add_model(|_ctx| {
            RemoteDiffStateModel::new_for_test_folding(
                DiffMode::Head,
                InternalRemoteDiffState::Loading,
                Some(test_metadata("feature-a")),
            )
        });
        let meta = empty_metadata("feature-b");
        handle.update(&mut app, |m, ctx| m.apply_metadata(meta, ctx));
        assert_eq!(
            handle
                .read(&app, |m, _| m.get_current_branch_name())
                .as_deref(),
            Some("feature-b")
        );
    });
}

#[test]
fn read_api_with_metadata() {
    let m = RemoteDiffStateModel::new_for_test_folding(
        DiffMode::Head,
        InternalRemoteDiffState::Loading,
        Some(test_metadata("feature")),
    );
    assert_eq!(m.get_main_branch_name().as_deref(), Some("main"));
    assert_eq!(m.get_current_branch_name().as_deref(), Some("feature"));
    assert!(!m.is_on_main_branch());
    assert_eq!(m.unpushed_commits().len(), 1);
    assert_eq!(m.upstream_ref(), Some("origin/feature"));
    assert!(m.upstream_differs_from_main());
    assert!(m.has_head());
    assert_eq!(
        m.get_uncommitted_stats()
            .expect("uncommitted stats should be present")
            .total_additions,
        5
    );
}

#[test]
fn read_api_defaults_without_metadata() {
    let m = RemoteDiffStateModel::new_for_test_folding(
        DiffMode::Head,
        InternalRemoteDiffState::Loading,
        None,
    );
    assert_eq!(m.get_main_branch_name(), None);
    assert_eq!(m.get_current_branch_name(), None);
    assert!(!m.is_on_main_branch());
    assert!(m.unpushed_commits().is_empty());
    assert!(m.upstream_ref().is_none());
    assert!(!m.upstream_differs_from_main());
    assert!(!m.has_head());
    assert!(m.get_uncommitted_stats().is_none());
}

#[test]
fn is_on_main_branch_true_when_matching() {
    let mut meta = test_metadata("main");
    meta.current_branch_name = "main".into();
    let m = RemoteDiffStateModel::new_for_test_folding(
        DiffMode::Head,
        InternalRemoteDiffState::Loading,
        Some(meta),
    );
    assert!(m.is_on_main_branch());
}

#[test]
fn get_returns_each_state_variant() {
    // The fork's public `DiffState` has no `Disconnected` variant — a
    // disconnected remote model preserves its last-known diffs and reports
    // `Loading` rather than surfacing the transport state (see `get()` in
    // diff_state_remote.rs) — so that sub-case is adapted to assert the
    // `Loading` mapping instead of a `Disconnected` variant that doesn't
    // exist.
    assert!(matches!(
        RemoteDiffStateModel::new_for_test_folding(
            DiffMode::Head,
            InternalRemoteDiffState::Loading,
            None
        )
        .get(),
        DiffState::Loading
    ));
    assert!(matches!(
        RemoteDiffStateModel::new_for_test_folding(
            DiffMode::Head,
            InternalRemoteDiffState::NotInRepository,
            None
        )
        .get(),
        DiffState::NotInRepository
    ));
    assert!(matches!(
        RemoteDiffStateModel::new_for_test_folding(
            DiffMode::Head,
            InternalRemoteDiffState::Error("x".into()),
            None
        )
        .get(),
        DiffState::Error(_)
    ));
    assert!(matches!(
        RemoteDiffStateModel::new_for_test_folding(
            DiffMode::Head,
            InternalRemoteDiffState::Disconnected,
            None
        )
        .get(),
        DiffState::Loading
    ));
}

#[test]
fn diff_mode_preserved() {
    let m = RemoteDiffStateModel::new_for_test_folding(
        DiffMode::OtherBranch("develop".into()),
        InternalRemoteDiffState::Loading,
        None,
    );
    assert_eq!(m.diff_mode(), DiffMode::OtherBranch("develop".into()));
}

#[test]
fn empty_branch_names_become_none() {
    let mut meta = test_metadata("");
    meta.main_branch_name = String::new();
    meta.current_branch_name = String::new();
    let m = RemoteDiffStateModel::new_for_test_folding(
        DiffMode::Head,
        InternalRemoteDiffState::Loading,
        Some(meta),
    );
    assert_eq!(m.get_main_branch_name(), None);
    assert_eq!(m.get_current_branch_name(), None);
}

#[test]
fn host_disconnected_for_matching_host_transitions_to_disconnected() {
    // Adapted from the pin's `mark_disconnected_transitions_state_and_emits_
    // connection_lost`: the fork has no dedicated `mark_disconnected` method
    // and no `ConnectionLost` event (`DiffStateModelEvent` carries no such
    // variant) — `handle_manager_event` sets the internal state inline with
    // no signal to subscribers. See the filed feature-gap issue for the
    // missing reconnect-banner signal; this test covers the state
    // transition that does happen.
    warpui::App::test((), |mut app| async move {
        let handle = app.add_model(|_ctx| {
            RemoteDiffStateModel::new_for_test_folding(
                DiffMode::Head,
                InternalRemoteDiffState::Loading,
                None,
            )
        });

        let event = RemoteServerManagerEvent::HostDisconnected {
            host_id: HostId::new("test-host".to_string()),
        };
        handle.update(&mut app, |m, ctx| m.handle_manager_event(&event, ctx));

        handle.read(&app, |m, _| {
            assert!(matches!(m.state, InternalRemoteDiffState::Disconnected));
        });
    });
}

#[test]
fn host_disconnected_for_other_host_is_ignored() {
    warpui::App::test((), |mut app| async move {
        let handle = app.add_model(|_ctx| {
            RemoteDiffStateModel::new_for_test_folding(
                DiffMode::Head,
                InternalRemoteDiffState::Loading,
                None,
            )
        });
        let event = RemoteServerManagerEvent::HostDisconnected {
            host_id: HostId::new("other-host".to_string()),
        };
        handle.update(&mut app, |m, ctx| m.handle_manager_event(&event, ctx));

        handle.read(&app, |m, _| {
            assert!(matches!(m.state, InternalRemoteDiffState::Loading));
        });
    });
}

#[test]
fn session_disconnected_is_ignored_by_session_agnostic_model() {
    // Per-session lifecycle events are not the model's concern; the manager
    // picks a connected client at RPC dispatch time and only host-level
    // connect/disconnect drive state transitions. `handle_manager_event`'s
    // catch-all arm silently ignores everything else, including this event.
    warpui::App::test((), |mut app| async move {
        let handle = app.add_model(|_ctx| {
            RemoteDiffStateModel::new_for_test_folding(
                DiffMode::Head,
                InternalRemoteDiffState::Loading,
                None,
            )
        });
        let event = RemoteServerManagerEvent::SessionDisconnected {
            session_id: SessionId::default(),
            host_id: HostId::new("test-host".to_string()),
            exit_status: None,
        };
        handle.update(&mut app, |m, ctx| m.handle_manager_event(&event, ctx));

        handle.read(&app, |m, _| {
            assert!(matches!(m.state, InternalRemoteDiffState::Loading));
        });
    });
}
