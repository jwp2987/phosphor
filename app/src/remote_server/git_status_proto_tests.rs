//! Round-trip tests for the git-status / GitHub-info proto conversions.
//!
//! The `rebased` case is the one worth pinning: the pin's wire format has no
//! field for it, so a port that copied the pin verbatim would drop the fork's
//! rebased indicator on every remote session without any type error to catch
//! it. `tracking_rebased` (proto tag 8) exists to prevent that, and this test
//! is what keeps it wired.

use super::*;
use crate::code_review::diff_state::DiffStats;
use crate::code_review::git_status_update::GitStatusMetadata;
use crate::context_chips::display_chip::GitBranchTrackingStatus;
use crate::util::git::RepositoryInfo;

fn stats() -> DiffStats {
    DiffStats {
        files_changed: 3,
        total_additions: 12,
        total_deletions: 4,
    }
}

fn metadata(branch_tracking_status: GitBranchTrackingStatus) -> GitStatusMetadata {
    GitStatusMetadata {
        current_branch_name: "feature-a".to_string(),
        main_branch_name: "main".to_string(),
        stats_against_head: stats(),
        branch_tracking_status,
    }
}

fn assert_round_trips(original: &GitStatusMetadata) {
    let decoded = proto_to_git_status_metadata(&git_status_metadata_to_proto(original))
        .expect("metadata built by the encoder must decode");

    assert_eq!(decoded.current_branch_name, original.current_branch_name);
    assert_eq!(decoded.main_branch_name, original.main_branch_name);
    assert_eq!(
        decoded.stats_against_head.files_changed,
        original.stats_against_head.files_changed
    );
    assert_eq!(
        decoded.stats_against_head.total_additions,
        original.stats_against_head.total_additions
    );
    assert_eq!(
        decoded.stats_against_head.total_deletions,
        original.stats_against_head.total_deletions
    );
    assert_eq!(
        decoded.branch_tracking_status,
        original.branch_tracking_status
    );
}

#[test]
fn git_status_metadata_round_trips_with_ahead_behind_counts() {
    assert_round_trips(&metadata(GitBranchTrackingStatus::new(
        "feature-a".to_string(),
        Some("origin/feature-a".to_string()),
        3,
        1,
    )));
}

#[test]
fn git_status_metadata_round_trips_without_an_upstream() {
    assert_round_trips(&metadata(GitBranchTrackingStatus::new(
        "feature-a".to_string(),
        None,
        0,
        0,
    )));
}

#[test]
fn git_status_metadata_round_trips_without_counts() {
    assert_round_trips(&metadata(GitBranchTrackingStatus::without_counts(
        "feature-a".to_string(),
        Some("origin/feature-a".to_string()),
    )));
}

#[test]
fn git_status_metadata_preserves_the_rebased_indicator() {
    let original = metadata(GitBranchTrackingStatus::rebased(
        "feature-a".to_string(),
        "origin/feature-a".to_string(),
    ));
    assert!(original.branch_tracking_status.rebased);

    let encoded = git_status_metadata_to_proto(&original);
    assert!(
        encoded.tracking_rebased,
        "the fork-original rebased flag must reach the wire"
    );

    assert_round_trips(&original);
}

#[test]
fn git_status_metadata_rejects_a_push_with_no_stats() {
    let mut encoded = git_status_metadata_to_proto(&metadata(GitBranchTrackingStatus::new(
        "feature-a".to_string(),
        None,
        0,
        0,
    )));
    encoded.stats_against_head = None;

    assert!(proto_to_git_status_metadata(&encoded).is_err());
}

#[test]
fn repository_info_round_trips() {
    for original in [
        RepositoryInfo {
            name: "phosphor".to_string(),
            owner: Some("winters".to_string()),
            host: Some("github.com".to_string()),
        },
        RepositoryInfo {
            name: "phosphor".to_string(),
            owner: None,
            host: None,
        },
    ] {
        let decoded = proto_to_repository_info(&repository_info_to_proto(&original));
        assert_eq!(decoded, original);
    }
}
