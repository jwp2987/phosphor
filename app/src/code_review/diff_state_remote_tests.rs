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
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use warp_core::HostId;
use warp_util::standardized_path::StandardizedPath;

use super::{InternalRemoteDiffState, RemoteDiffStateModel};
use crate::code::buffer_location::RemotePath;
use crate::code_review::diff_state::{
    DiffMetadata, DiffMetadataAgainstBase, DiffMode, DiffStats, DiffStateModelEvent,
};
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
