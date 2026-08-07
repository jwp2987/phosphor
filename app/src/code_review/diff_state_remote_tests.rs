//! Tests for the remote (SSH) diff-state backend's branch listing.
//!
//! The harness mirrors `warp/master:app/src/code_review/diff_state/remote_tests.rs`
//! (`new_for_test` builds the model without the `GetDiffState` subscription that
//! `new` issues), and the response test mirrors Warp's
//! `get_committed_branch_files_response_emits_domain_files`: feed a proto
//! response into the handler and assert the domain event it emits.

use std::sync::{Arc, Mutex};

use warp_core::HostId;
use warp_util::standardized_path::StandardizedPath;

use super::{InternalRemoteDiffState, RemoteDiffStateModel};
use crate::code::buffer_location::RemotePath;
use crate::code_review::diff_state::{DiffMode, DiffStateModelEvent};
use crate::remote_server::proto;

const TEST_REPO_PATH: &str = "/test/repo";

impl RemoteDiffStateModel {
    fn new_for_test(repo_path: &str) -> Self {
        Self {
            remote_path: RemotePath::new(
                HostId::new("test-host".to_string()),
                StandardizedPath::try_new(repo_path)
                    .expect("test repo path should be valid and absolute"),
            ),
            mode: DiffMode::Head,
            state: InternalRemoteDiffState::Loading,
            metadata: None,
        }
    }
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
