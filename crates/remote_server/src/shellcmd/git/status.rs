//! The aggregate git status behind the prompt's git chip (branch, main branch, change counts
//! against HEAD, ahead/behind the upstream), computed over a [`GitRunner`] and produced as the
//! wire message `GitStatusMetadata`. Shared by the helper and the shell backend; the app's local
//! `LocalGitRepoStatusModel` computes the same values with the same commands.

use std::path::Path;

use anyhow::Result;

use super::ops::{detect_current_branch_display, detect_main_branch, read_branch_tracking};
use super::parse::{NUMSTAT_Z_HEAD_ARGS, STATUS_CHIP_ARGS, parse_status, sum_numstat_z};
use super::runner::{GitRunner, git_text};
use crate::proto;

/// Stats for the git chip: insertions and deletions of tracked files against HEAD (what
/// `git diff --shortstat HEAD` reports), and every changed path counted once as a file.
/// Untracked files count only as files, never by lines, and no untracked file is read.
pub async fn chip_stats_against_head(
    runner: &dyn GitRunner,
    repo: &Path,
) -> Result<proto::DiffStats> {
    // Untracked directories collapse to one entry: they only count as files.
    let status_output = git_text(runner, repo, &STATUS_CHIP_ARGS).await?;
    let files_changed = parse_status(&status_output)?.len();
    // No HEAD yet (or numstat fails): no tracked line changes to report.
    let (total_additions, total_deletions) =
        match git_text(runner, repo, &NUMSTAT_Z_HEAD_ARGS).await {
            Ok(output) => sum_numstat_z(&output),
            Err(_) => (0, 0),
        };
    Ok(proto::DiffStats {
        files_changed: files_changed as u64,
        total_additions: total_additions as u64,
        total_deletions: total_deletions as u64,
    })
}

/// The chip's metadata for `repo`. `cached_main_branch` skips the (several-process) detection of
/// the main branch when the caller still trusts an earlier answer.
pub async fn compute_git_status_metadata(
    runner: &dyn GitRunner,
    repo: &Path,
    cached_main_branch: Option<String>,
) -> Result<proto::GitStatusMetadata> {
    let main_branch_name = match cached_main_branch {
        Some(name) => name,
        None => detect_main_branch(runner, repo).await?,
    };
    // The display variant: a detached HEAD shows the short SHA instead of "HEAD".
    let current_branch_name = detect_current_branch_display(runner, repo).await?;
    let stats = chip_stats_against_head(runner, repo).await?;
    let (upstream, counts) = read_branch_tracking(runner, repo).await;
    let (tracking_ahead, tracking_behind, tracking_counts_available, tracking_rebased) =
        match (&upstream, counts) {
            (None, _) => (0, 0, false, false),
            (Some(_), None) => (0, 0, false, false),
            // Diverged only by cherry-equivalent commits: the branch was rebased.
            (Some(_), Some((0, 0, equivalent))) if equivalent > 0 => (0, 0, true, true),
            (Some(_), Some((ahead, behind, _))) => (ahead, behind, true, false),
        };
    Ok(proto::GitStatusMetadata {
        current_branch_name,
        main_branch_name,
        stats_against_head: Some(stats),
        tracking_upstream: upstream,
        tracking_ahead,
        tracking_behind,
        tracking_counts_available,
        tracking_rebased,
    })
}

/// The `GitStatusPush` for a repository, keyed by the path string the client sent.
pub fn git_status_push(
    wire_repo_path: &str,
    metadata: proto::GitStatusMetadata,
) -> proto::GitStatusPush {
    proto::GitStatusPush {
        repo_path: wire_repo_path.to_string(),
        metadata: Some(metadata),
    }
}

/// `GitHubPrInfoPush`; absent PR info is the "no PR" answer.
pub fn pr_info_push(
    wire_repo_path: &str,
    pr: Option<&super::types::PrInfo>,
) -> proto::GitHubPrInfoPush {
    proto::GitHubPrInfoPush {
        repo_path: wire_repo_path.to_string(),
        pr_info: pr.map(|pr| proto::PrInfo {
            number: pr.number,
            url: pr.url.clone(),
            state: pr.state.clone(),
            draft: pr.draft,
            base_branch: pr.base_branch.clone(),
        }),
    }
}

/// `GitHubRepositoryInfoPush`; absent info is the "no GitHub repository" answer.
pub fn repository_info_push(
    wire_repo_path: &str,
    info: Option<&super::types::RepositoryInfo>,
) -> proto::GitHubRepositoryInfoPush {
    proto::GitHubRepositoryInfoPush {
        repo_path: wire_repo_path.to_string(),
        repository_info: info.map(|info| proto::RepositoryInfo {
            name: info.name.clone(),
            owner: info.owner.clone(),
            host: info.host.clone(),
        }),
    }
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;
