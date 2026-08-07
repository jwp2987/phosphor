//! Ported from the pinned oracle
//! (warp/master:app/src/code_review/git_repo_model/local_tests.rs @ 02b53fcd8).
//!
//! The pin's `LocalGitRepoStatusModel` lives in the fork as `GitRepoStatusModel`
//! (the fork flattened `git_repo_model/{mod,local,remote}.rs` into this single
//! `git_status_update.rs`, per `GitRepoModels`/`GitStatusUpdateModel` above) —
//! `should_refresh_metadata` and `parse_branch_tracking_counts` are unchanged
//! private associated functions on it, so these port with only the type rename.

use std::path::PathBuf;

use repo_metadata::{RepositoryUpdate, TargetFile};

use super::*;

#[test]
fn should_refresh_metadata_ignores_ignored_file_updates() {
    let mut ignored_update = RepositoryUpdate::default();
    ignored_update
        .modified
        .insert(TargetFile::new(PathBuf::from("/repo/ignored.log"), true));
    assert!(!GitRepoStatusModel::should_refresh_metadata(
        &ignored_update
    ));

    let mut tracked_update = RepositoryUpdate::default();
    tracked_update
        .modified
        .insert(TargetFile::new(PathBuf::from("/repo/src/main.rs"), false));
    assert!(GitRepoStatusModel::should_refresh_metadata(&tracked_update));

    // The fork's `RepositoryUpdate` has no `remote_ref_updated` field (the pin's
    // third case) — `commit_updated` is its equivalent "refresh immediately"
    // signal independent of the changed-file sets.
    let commit_update = RepositoryUpdate {
        commit_updated: true,
        ..Default::default()
    };
    assert!(GitRepoStatusModel::should_refresh_metadata(&commit_update));
}

#[test]
fn parse_branch_tracking_counts_accepts_git_rev_list_output() {
    assert_eq!(
        GitRepoStatusModel::parse_branch_tracking_counts("2\t3\n"),
        Some((2, 3, 0))
    );
    assert_eq!(
        GitRepoStatusModel::parse_branch_tracking_counts("10 0 4"),
        Some((10, 0, 4))
    );
    assert_eq!(
        GitRepoStatusModel::parse_branch_tracking_counts("error"),
        None
    );
}
