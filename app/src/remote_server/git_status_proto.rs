//! Conversion between the git-status / GitHub-info proto types and the app
//! domain types consumed by [`RemoteGitRepoStatusModel`] and
//! [`RemoteGitHubRepoModel`].
//!
//! Rust types are canonical, proto types are the wire format. Git status
//! (branch + HEAD diff stats), GitHub PR info, and GitHub repository info are
//! kept separate so they can be pushed on independent cadences. The
//! `DiffStats` / `PrInfo` conversions are reused from [`super::diff_state_proto`]
//! rather than duplicated.
//!
//! Ported from `42effe840:app/src/remote_server/git_status_proto.rs`. Two
//! deviations, both noted at their sites: this fork spells the conversions as
//! free functions (the convention in `diff_state_proto`) instead of `From` /
//! `TryFrom` impls, and it carries the fork-original `rebased` flag across the
//! wire, which the pin's flat proto has no field for.
//!
//! [`RemoteGitRepoStatusModel`]: crate::code_review::git_status_update_remote::RemoteGitRepoStatusModel
//! [`RemoteGitHubRepoModel`]: crate::code_review::github_repo_model::RemoteGitHubRepoModel

use super::diff_state_proto::{diff_stats_to_proto, proto_to_diff_stats};
use super::proto;
use crate::code_review::git_status_update::GitStatusMetadata;
use crate::context_chips::display_chip::GitBranchTrackingStatus;
use crate::util::git::RepositoryInfo;

// ── RepositoryInfo (util/git.rs) ─────────────────────────────────

pub(crate) fn repository_info_to_proto(info: &RepositoryInfo) -> proto::RepositoryInfo {
    proto::RepositoryInfo {
        name: info.name.clone(),
        owner: info.owner.clone(),
        host: info.host.clone(),
    }
}

pub(crate) fn proto_to_repository_info(info: &proto::RepositoryInfo) -> RepositoryInfo {
    RepositoryInfo {
        name: info.name.clone(),
        owner: info.owner.clone(),
        host: info.host.clone(),
    }
}

// ── GitStatusMetadata (code_review/git_status_update.rs) ─────────

pub(crate) fn git_status_metadata_to_proto(
    metadata: &GitStatusMetadata,
) -> proto::GitStatusMetadata {
    proto::GitStatusMetadata {
        current_branch_name: metadata.current_branch_name.clone(),
        main_branch_name: metadata.main_branch_name.clone(),
        stats_against_head: Some(diff_stats_to_proto(&metadata.stats_against_head)),
        tracking_upstream: metadata.branch_tracking_status.upstream.clone(),
        tracking_ahead: metadata.branch_tracking_status.ahead,
        tracking_behind: metadata.branch_tracking_status.behind,
        tracking_counts_available: metadata.branch_tracking_status.counts_available,
        // Fork-original field (proto tag 8). The pin's `GitBranchTrackingStatus`
        // has no `rebased`, so its wire format drops the distinction between
        // "in sync" and "rebased onto the upstream". Sending it keeps the
        // remote chip identical to the local one.
        tracking_rebased: metadata.branch_tracking_status.rebased,
    }
}

/// Decode a pushed `GitStatusMetadata`.
///
/// Returns `Err` only for a structurally invalid message (a missing
/// `stats_against_head`), which a correct daemon never sends; the caller logs
/// and keeps the previous value rather than clearing the chip.
pub(crate) fn proto_to_git_status_metadata(
    metadata: &proto::GitStatusMetadata,
) -> Result<GitStatusMetadata, String> {
    let stats = metadata
        .stats_against_head
        .as_ref()
        .ok_or_else(|| "missing stats_against_head in GitStatusMetadata".to_string())?;
    Ok(GitStatusMetadata {
        current_branch_name: metadata.current_branch_name.clone(),
        main_branch_name: metadata.main_branch_name.clone(),
        stats_against_head: proto_to_diff_stats(stats),
        // Built as a struct literal rather than through the
        // `new` / `without_counts` / `rebased` constructors: those each derive
        // `counts_available` (and clear `rebased`) from their arguments, which
        // would discard one of the two flags the wire actually carries.
        branch_tracking_status: GitBranchTrackingStatus {
            branch: metadata.current_branch_name.clone(),
            upstream: metadata.tracking_upstream.clone(),
            ahead: metadata.tracking_ahead,
            behind: metadata.tracking_behind,
            counts_available: metadata.tracking_counts_available,
            rebased: metadata.tracking_rebased,
        },
    })
}

#[cfg(test)]
#[path = "git_status_proto_tests.rs"]
mod tests;
