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

// ── command-completion refresh gating (#1334 follow-up) ─────────────────────

#[test]
fn empty_enter_with_quiet_watcher_runs_no_git() {
    let mut gate = RefreshGate::default();
    let t0 = Instant::now();
    gate.refresh_started(t0);
    let later = t0 + Duration::from_secs(10);
    assert_eq!(gate.decide("", true, later, false), RefreshDecision::Skip);
    assert_eq!(
        gate.decide("  \t", true, later, false),
        RefreshDecision::Skip
    );
    assert_eq!(
        gate.decide("ls -la", true, later, false),
        RefreshDecision::Skip
    );
}

#[test]
fn watcher_change_forces_refresh_then_clears() {
    let mut gate = RefreshGate::default();
    let t0 = Instant::now();
    gate.refresh_started(t0);
    gate.mark_dirty();
    let later = t0 + Duration::from_secs(10);
    assert_eq!(gate.decide("", true, later, false), RefreshDecision::Now);
    gate.refresh_started(later);
    assert_eq!(
        gate.decide("", true, later + Duration::from_secs(1), false),
        RefreshDecision::Skip
    );
}

#[test]
fn vcs_commands_and_untrusted_state_always_refresh() {
    let mut gate = RefreshGate::default();
    let t0 = Instant::now();
    gate.refresh_started(t0);
    let later = t0 + Duration::from_secs(10);
    assert_eq!(
        gate.decide("git switch b", true, later, false),
        RefreshDecision::Now
    );
    gate.refresh_started(later);
    let later = later + Duration::from_secs(10);
    // No metadata / dead watcher: cannot trust "nothing changed".
    assert_eq!(gate.decide("", false, later, false), RefreshDecision::Now);
    // Kill switch.
    assert_eq!(gate.decide("ls", true, later, true), RefreshDecision::Now);
}

#[test]
fn burst_collapses_to_one_refresh() {
    let mut gate = RefreshGate::default();
    let t0 = Instant::now();
    gate.refresh_started(t0);
    gate.mark_dirty();
    let t1 = t0 + Duration::from_millis(50);
    assert_eq!(
        gate.decide("make", true, t1, false),
        RefreshDecision::Defer(REFRESH_COALESCE_WINDOW - Duration::from_millis(50))
    );
    // Further completions while the deferred refresh is pending are absorbed.
    assert_eq!(
        gate.decide("git status", true, t1, false),
        RefreshDecision::Skip
    );
    gate.deferred_fired();
    gate.refresh_started(t0 + REFRESH_COALESCE_WINDOW);
    assert_eq!(
        gate.decide("git status", true, t0 + Duration::from_secs(5), false),
        RefreshDecision::Now
    );
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
