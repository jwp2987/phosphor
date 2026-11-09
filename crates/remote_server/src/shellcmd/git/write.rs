//! The git WRITE requests (`GitCommitChain`, `GitPush`, `GitPull`, `GitCreatePr`, `DiscardFiles`,
//! `GitStage`) as one piece of logic over a [`GitRunner`], so the helper and the shell backend
//! run identical argv and give identical answers (wave H parity, package P7, family f).
//!
//! Each function takes the already-resolved repository directory (the helper canonicalizes it,
//! the shell backend passes it through) and returns the proto response, with the old full
//! server's contract (`server_model.rs` handlers, which these replace one for one):
//!
//! * the validation that needs no git runs first and fails as a plain error response;
//! * then the execution-time guard ([`guard_git_operation_in_progress`]): a merge, rebase,
//!   cherry-pick or lock file in progress refuses the write;
//! * then the operation; any failure is `{error:#}` in the response's error message.
//!
//! Credentials never prompt: that is the runner's job (see [`super::runner::git_ssh_command`]).

use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use futures::future::BoxFuture;

use super::ops::{
    apply_patch_cached, compute_unpushed_state, create_pr, discard_files,
    guard_git_operation_in_progress, run_commit_chain, run_pull, run_push, stage_paths,
};
use super::runner::GitRunner;
use super::types::{Commit, CommitChainMode, DiscardTarget, PrInfo};
use crate::proto;

/// How the caller tests for the in-progress sentinels in a git dir: the helper looks at its own
/// filesystem, the shell backend runs [`super::operation_probe_script`] on the host.
pub type OperationProbe<'a> =
    &'a (dyn Fn(PathBuf) -> BoxFuture<'static, Result<bool>> + Send + Sync);

/// The error text when [`guard_git_operation_in_progress`] refuses, verbatim from the old server.
pub const OPERATION_IN_PROGRESS: &str =
    "another git operation is in progress (merge, rebase, cherry-pick, or a lock file is present)";

/// What the stash a discard may create is called after.
pub const STASH_APP_NAME: &str = "Phosphor";

async fn guard(repo: &Path, probe: OperationProbe<'_>) -> Result<()> {
    if guard_git_operation_in_progress(repo, |git_dir| probe(git_dir)).await? {
        return Err(anyhow!(OPERATION_IN_PROGRESS));
    }
    Ok(())
}

fn commit_to_proto(commit: &Commit) -> proto::Commit {
    proto::Commit {
        hash: commit.hash.clone(),
        subject: commit.subject.clone(),
        files_changed: commit.files_changed as u64,
        additions: commit.additions as u64,
        deletions: commit.deletions as u64,
        files: Vec::new(),
    }
}

fn pr_info_to_proto(pr: &PrInfo) -> proto::PrInfo {
    proto::PrInfo {
        number: pr.number,
        url: pr.url.clone(),
        state: pr.state.clone(),
        draft: pr.draft,
        base_branch: pr.base_branch.clone(),
    }
}

fn delta(commits: &[Commit], upstream_ref: Option<String>) -> proto::GitOpDelta {
    proto::GitOpDelta {
        unpushed_commits: commits.iter().map(commit_to_proto).collect(),
        upstream_ref,
    }
}

fn op_error(e: &anyhow::Error) -> proto::GitOpError {
    proto::GitOpError {
        message: format!("{e:#}"),
    }
}

/// `GitCommitChain`: commit, then optionally push, then optionally create the PR. The PR content
/// flag is ignored (always `--fill`, #116).
pub async fn commit_chain(
    runner: &dyn GitRunner,
    probe: OperationProbe<'_>,
    repo: &Path,
    request: proto::GitCommitChainRequest,
) -> proto::GitCommitChainResponse {
    use proto::git_commit_chain_response::Result as R;
    let mode = match request.mode() {
        proto::GitCommitChainMode::CommitOnly => CommitChainMode::CommitOnly,
        proto::GitCommitChainMode::CommitAndPush => CommitChainMode::CommitAndPush,
        proto::GitCommitChainMode::CommitAndCreatePr => CommitChainMode::CommitAndCreatePr,
    };
    let result = async {
        guard(repo, probe).await?;
        run_commit_chain(
            runner,
            repo,
            mode,
            &request.message,
            request.include_unstaged,
            &request.branch,
        )
        .await
    }
    .await;
    proto::GitCommitChainResponse {
        result: Some(match result {
            Ok((commits, upstream_ref, pr_info)) => R::Success(proto::GitCommitChainSuccess {
                delta: Some(delta(&commits, upstream_ref)),
                pr_info: pr_info.as_ref().map(pr_info_to_proto),
            }),
            Err(e) => R::Error(op_error(&e)),
        }),
    }
}

/// `GitPush`: `git push --set-upstream origin <branch>`, then the refreshed delta.
pub async fn push(
    runner: &dyn GitRunner,
    probe: OperationProbe<'_>,
    repo: &Path,
    request: proto::GitPushRequest,
) -> proto::GitPushResponse {
    use proto::git_push_response::Result as R;
    let result = async {
        guard(repo, probe).await?;
        run_push(runner, repo, &request.branch).await?;
        anyhow::Ok(compute_unpushed_state(runner, repo).await)
    }
    .await;
    proto::GitPushResponse {
        result: Some(match result {
            Ok((commits, upstream_ref)) => R::Success(delta(&commits, upstream_ref)),
            Err(e) => R::Error(op_error(&e)),
        }),
    }
}

/// `GitPull`: fast-forward only, then the refreshed delta.
pub async fn pull(
    runner: &dyn GitRunner,
    probe: OperationProbe<'_>,
    repo: &Path,
    request: proto::GitPullRequest,
) -> proto::GitPullResponse {
    use proto::git_pull_response::Result as R;
    let result = async {
        guard(repo, probe).await?;
        run_pull(runner, repo, &request.branch).await?;
        anyhow::Ok(compute_unpushed_state(runner, repo).await)
    }
    .await;
    proto::GitPullResponse {
        result: Some(match result {
            Ok((commits, upstream_ref)) => R::Success(delta(&commits, upstream_ref)),
            Err(e) => R::Error(op_error(&e)),
        }),
    }
}

/// `GitCreatePr`: `gh pr create --fill` for the current (pushed) branch.
pub async fn create_pull_request(
    runner: &dyn GitRunner,
    probe: OperationProbe<'_>,
    repo: &Path,
    _request: proto::GitCreatePrRequest,
) -> proto::GitCreatePrResponse {
    use proto::git_create_pr_response::Result as R;
    let result = async {
        guard(repo, probe).await?;
        create_pr(runner, repo, None, None).await
    }
    .await;
    proto::GitCreatePrResponse {
        result: Some(match result {
            Ok(pr) => R::Success(pr_info_to_proto(&pr)),
            Err(e) => R::Error(op_error(&e)),
        }),
    }
}

/// Whether `path` is absolute under the encoding of the host it came from (unix `/...` or
/// Windows `C:\...`), whatever OS this code runs on.
pub fn is_absolute_on_its_host(path: &str) -> bool {
    let b = path.as_bytes();
    path.starts_with('/')
        || path.starts_with("\\\\")
        || (b.len() >= 3
            && b[0].is_ascii_alphabetic()
            && b[1] == b':'
            && matches!(b[2], b'/' | b'\\'))
}

/// Decodes the `files` of a discard request into [`DiscardTarget`]s relative to `repo`. A bad
/// entry is an error: discarding against a half-decoded path is destructive (#326).
fn decode_discard_targets(
    repo: &Path,
    files: &[proto::FileStatusInfo],
) -> Result<Vec<DiscardTarget>, String> {
    use proto::git_file_status::Status;
    let repo = repo.to_string_lossy();
    let repo_prefix = format!("{}/", repo.trim_end_matches(['/', '\\']));
    files
        .iter()
        .map(|info| {
            if !is_absolute_on_its_host(&info.path) {
                return Err(format!(
                    "FileStatusInfo path is not absolute: {}",
                    info.path
                ));
            }
            let status = info
                .status
                .as_ref()
                .and_then(|s| s.status.as_ref())
                .ok_or_else(|| "missing status in FileStatusInfo".to_string())?;
            let old_path = match status {
                Status::Renamed(r) => Some(&r.old_path),
                Status::Copied(c) => Some(&c.old_path),
                _ => None,
            };
            if let Some(old_path) = old_path {
                if !is_absolute_on_its_host(old_path) {
                    return Err(format!("old_path is not absolute: {old_path}"));
                }
            }
            let rel_path = info
                .path
                .strip_prefix(&repo_prefix)
                .unwrap_or(&info.path)
                .to_string();
            let renamed_from = match status {
                Status::Renamed(r) => Some(r.old_path.clone()),
                _ => None,
            };
            Ok(DiscardTarget {
                rel_path,
                renamed_from,
            })
        })
        .collect()
}

/// `DiscardFiles`. `remove_from_fs` deletes a working-tree file the git sequence left behind (a
/// file that is not in the target branch); see [`super::git_restore_and_clean`].
pub async fn discard(
    runner: &dyn GitRunner,
    probe: OperationProbe<'_>,
    repo: &Path,
    request: proto::DiscardFilesRequest,
    remove_from_fs: &(dyn Fn(&str) + Send + Sync),
) -> proto::DiscardFilesResponse {
    use proto::discard_files_response::Result as R;
    let error = |message: String| proto::DiscardFilesResponse {
        result: Some(R::Error(proto::DiscardFilesError { message })),
    };
    if request.files.is_empty() {
        return error("No files specified in DiscardFilesRequest".to_string());
    }
    let targets = match decode_discard_targets(repo, &request.files) {
        Ok(targets) => targets,
        Err(err) => {
            return error(format!("Invalid file entry in DiscardFilesRequest: {err}"));
        }
    };
    let branch = request.branch_name.unwrap_or_else(|| "HEAD".to_string());
    let result = async {
        guard(repo, probe).await?;
        discard_files(
            runner,
            repo,
            targets,
            request.should_stash,
            &branch,
            STASH_APP_NAME,
            remove_from_fs,
        )
        .await
    }
    .await;
    match result {
        Ok(()) => proto::DiscardFilesResponse {
            result: Some(R::Success(proto::DiscardFilesSuccess {})),
        },
        Err(e) => error(format!("{e:#}")),
    }
}

/// `GitStage`: exactly one of `paths` (repo-relative) and `patch` (applied to the index only).
pub async fn stage(
    runner: &dyn GitRunner,
    probe: OperationProbe<'_>,
    repo: &Path,
    request: proto::GitStageRequest,
) -> proto::GitStageResponse {
    use proto::git_stage_response::Result as R;
    let error = |message: String| proto::GitStageResponse {
        result: Some(R::Error(proto::GitStageError { message })),
    };
    enum Target {
        Paths(Vec<String>),
        Hunk(String),
    }
    // Both set is rejected rather than preferring one: guessing would stage something the client
    // did not ask for.
    let target = match (request.paths.is_empty(), request.patch) {
        (false, None) => Target::Paths(request.paths),
        (true, Some(patch)) => Target::Hunk(patch),
        (true, None) => return error("GitStageRequest set neither paths nor patch".to_string()),
        (false, Some(_)) => return error("GitStageRequest set both paths and patch".to_string()),
    };
    // `git add --` takes repo-relative paths; an absolute one would address the wrong tree.
    if let Target::Paths(paths) = &target {
        if let Some(bad) = paths.iter().find(|p| is_absolute_on_its_host(p)) {
            return error(format!(
                "GitStageRequest path must be repo-relative, got '{bad}'"
            ));
        }
    }
    let unstage = request.reverse;
    let result = async {
        guard(repo, probe).await?;
        match &target {
            Target::Paths(paths) => stage_paths(runner, repo, paths, unstage).await,
            Target::Hunk(patch) => {
                apply_patch_cached(runner, repo, "-", Some(patch.as_bytes()), unstage, true).await
            }
        }
    }
    .await;
    match result {
        Ok(_) => proto::GitStageResponse {
            result: Some(R::Success(proto::GitStageSuccess {})),
        },
        Err(e) => error(format!("{e:#}")),
    }
}

#[cfg(test)]
#[path = "write_tests.rs"]
mod tests;
