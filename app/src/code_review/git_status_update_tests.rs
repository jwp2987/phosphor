//! Ported from the pinned oracle
//! (warp/master:app/src/code_review/git_repo_model/local_tests.rs @ 02b53fcd8).
//!
//! The pin's `LocalGitRepoStatusModel` keeps its name in the fork; the fork
//! flattens `git_repo_model/{mod,local}.rs` into this single
//! `git_status_update.rs` (with the remote backend in the sibling
//! `git_status_update_remote.rs`, per `GitRepoModels`/`GitStatusUpdateModel`
//! above). `should_refresh_metadata` and `parse_branch_tracking_counts` are
//! unchanged private associated functions on the local backend, so these port
//! verbatim.

use std::path::PathBuf;

use repo_metadata::{RepositoryUpdate, TargetFile};

use super::*;

#[test]
fn should_refresh_metadata_ignores_ignored_file_updates() {
    let mut ignored_update = RepositoryUpdate::default();
    ignored_update
        .modified
        .insert(TargetFile::new(PathBuf::from("/repo/ignored.log"), true));
    assert!(!LocalGitRepoStatusModel::should_refresh_metadata(
        &ignored_update
    ));

    let mut tracked_update = RepositoryUpdate::default();
    tracked_update
        .modified
        .insert(TargetFile::new(PathBuf::from("/repo/src/main.rs"), false));
    assert!(LocalGitRepoStatusModel::should_refresh_metadata(
        &tracked_update
    ));

    let remote_ref_update = RepositoryUpdate {
        remote_ref_updated: true,
        ..Default::default()
    };
    assert!(LocalGitRepoStatusModel::should_refresh_metadata(
        &remote_ref_update
    ));
}

#[test]
fn parse_branch_tracking_counts_accepts_git_rev_list_output() {
    assert_eq!(
        LocalGitRepoStatusModel::parse_branch_tracking_counts("2\t3\n"),
        Some((2, 3, 0))
    );
    assert_eq!(
        LocalGitRepoStatusModel::parse_branch_tracking_counts("10 0 4"),
        Some((10, 0, 4))
    );
    assert_eq!(
        LocalGitRepoStatusModel::parse_branch_tracking_counts("error"),
        None
    );
}

/// #892: HEAD / branch-ref changes skip the 5s metadata throttle; index churn and
/// ordinary file edits stay throttled.
#[test]
fn head_changes_bypass_throttle_but_file_churn_does_not() {
    let head_update = RepositoryUpdate {
        commit_updated: true,
        ..Default::default()
    };
    assert!(LocalGitRepoStatusModel::is_head_change(&head_update));

    let mut edit = RepositoryUpdate::default();
    edit.modified
        .insert(TargetFile::new(PathBuf::from("/repo/src/main.rs"), false));
    assert!(!LocalGitRepoStatusModel::is_head_change(&edit));

    let lock = RepositoryUpdate {
        index_lock_detected: true,
        ..Default::default()
    };
    assert!(!LocalGitRepoStatusModel::is_head_change(&lock));
    assert!(!LocalGitRepoStatusModel::is_head_change(
        &RepositoryUpdate::default()
    ));
}

fn sample_metadata(branch: &str, additions: usize) -> GitStatusMetadata {
    GitStatusMetadata {
        current_branch_name: branch.to_owned(),
        main_branch_name: "main".to_owned(),
        stats_against_head: DiffStats {
            files_changed: 1,
            total_additions: additions,
            total_deletions: 0,
        },
        branch_tracking_status: GitBranchTrackingStatus::new(
            branch.to_owned(),
            Some(format!("origin/{branch}")),
            0,
            0,
        ),
    }
}

#[test]
fn metadata_changed_is_emitted_for_the_first_result_even_if_unchanged() {
    // The first event is what `deferred_code_review_open` waits for, so it must
    // fire even when the first load yields None (failed load) or equal data.
    assert!(should_emit_metadata_changed(&None, &None, false));
    let m = Some(sample_metadata("main", 1));
    assert!(should_emit_metadata_changed(&m, &m.clone(), false));
}

#[test]
fn metadata_changed_is_suppressed_when_identical() {
    let m = Some(sample_metadata("main", 1));
    assert!(!should_emit_metadata_changed(&m, &m.clone(), true));
    assert!(!should_emit_metadata_changed(&None, &None, true));
}

#[test]
fn metadata_changed_is_emitted_on_any_field_change() {
    let old = Some(sample_metadata("main", 1));
    assert!(should_emit_metadata_changed(
        &old,
        &Some(sample_metadata("main", 2)),
        true
    ));
    assert!(should_emit_metadata_changed(
        &old,
        &Some(sample_metadata("dev", 1)),
        true
    ));
    assert!(should_emit_metadata_changed(&None, &old, true));
    assert!(should_emit_metadata_changed(&old, &None, true));
}

#[cfg(feature = "local_fs")]
#[test]
fn main_branch_cache_is_reused_only_while_fresh() {
    use std::time::{Duration, Instant};

    let start = Instant::now();
    let cache = Some(("origin/main".to_owned(), start));
    // No cache: detect.
    assert_eq!(super::fresh_cached_main_branch(&None, start), None);
    // Inside the TTL the six-process detection is skipped (#1334).
    assert_eq!(
        super::fresh_cached_main_branch(&cache, start + Duration::from_secs(5)),
        Some("origin/main".to_owned())
    );
    // Past the TTL it is detected again.
    assert_eq!(
        super::fresh_cached_main_branch(&cache, start + Duration::from_secs(31)),
        None
    );
}

// ── command-completion refresh gating (#1334 follow-up, #1371) ───────────────

#[test]
fn only_empty_enter_with_quiet_watcher_skips() {
    let gate = RefreshGate::default();
    assert_eq!(gate.decide("", true, false), RefreshDecision::Skip);
    assert_eq!(gate.decide("  \t", true, false), RefreshDecision::Skip);
}

#[test]
fn plain_command_refreshes_even_before_watcher_reports() {
    // `echo x >> tracked`: the block completes before the watcher flushes, so `dirty` is still
    // false. The old gate skipped here and the chip lagged the edit.
    let gate = RefreshGate::default();
    assert_eq!(
        gate.decide("echo x >> tracked", true, false),
        RefreshDecision::Refresh
    );
    assert_eq!(gate.decide("ls -la", true, false), RefreshDecision::Refresh);
}

#[test]
fn dirty_untrusted_vcs_and_kill_switch_refresh() {
    let mut gate = RefreshGate::default();
    gate.mark_dirty();
    assert_eq!(gate.decide("", true, false), RefreshDecision::Refresh);
    let gate = RefreshGate::default();
    assert_eq!(gate.decide("", false, false), RefreshDecision::Refresh);
    assert_eq!(gate.decide("", true, true), RefreshDecision::Refresh);
    assert_eq!(
        gate.decide("git switch b", true, false),
        RefreshDecision::Refresh
    );
}

#[test]
fn request_during_flight_reruns_once_after_it() {
    let mut gate = RefreshGate::default();
    assert!(gate.request_refresh(), "first request starts a refresh");
    // Edits arrive while it runs: coalesced into one pending re-run, never dropped.
    assert!(!gate.request_refresh());
    assert!(!gate.request_refresh());
    assert!(gate.refresh_finished(), "pending request must re-run");
    assert!(gate.request_refresh());
    assert!(!gate.refresh_finished(), "nothing pending, no re-run");
    assert!(gate.request_refresh(), "idle gate starts immediately");
}

#[test]
fn starting_a_refresh_clears_dirty() {
    let mut gate = RefreshGate::default();
    gate.mark_dirty();
    assert!(gate.request_refresh());
    gate.refresh_finished();
    assert_eq!(gate.decide("", true, false), RefreshDecision::Skip);
}

#[test]
fn vcs_command_detection() {
    assert!(command_may_change_git_state("git commit -m x"));
    assert!(command_may_change_git_state("cd a && git pull"));
    assert!(command_may_change_git_state("git.exe checkout b"));
    assert!(command_may_change_git_state("gh pr checkout 3"));
    assert!(!command_may_change_git_state("cargo build"));
    assert!(!command_may_change_git_state("echo digit"));
    assert!(!command_may_change_git_state(""));
}
