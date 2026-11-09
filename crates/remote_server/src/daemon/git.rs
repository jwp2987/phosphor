//! Read-only git requests (`GetBranches`, `GetCommittedBranchFiles`) for the helper.
//!
//! Shells out to the host's `git`, exactly as the app's code-review model does, and builds the
//! same proto responses. No libgit2: it would pull a C toolchain into the musl cross builds.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, anyhow};

use crate::proto::{
    BranchInfo, FileChangeEntry, GetBranches, GetBranchesError, GetBranchesResponse,
    GetBranchesSuccess, GetCommittedBranchFilesError, GetCommittedBranchFilesRequest,
    GetCommittedBranchFilesResponse, GetCommittedBranchFilesSuccess, get_branches_response,
    get_committed_branch_files_response,
};

/// Server-side cap on the number of branches returned by `GetBranches`.
const MAX_BRANCH_COUNT_CAP: usize = 500;

/// Runs `git <args>` in `repo` and returns stdout. A non-zero exit is an error carrying stderr.
fn run_git(repo: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(std::process::Stdio::null())
        .output()
        .with_context(|| format!("could not run git {}", args.join(" ")))?;
    if !output.status.success() {
        return Err(anyhow!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub(super) fn canonical_repo_path(raw: &str) -> Result<PathBuf> {
    let path = crate::naming::expand_tilde(raw);
    std::fs::canonicalize(&path).map_err(|e| anyhow!("Invalid repo_path: {e}"))
}

/// The main branch by git-branchless style heuristics: origin's default, else the first of
/// common names that resolves, else the current branch.
fn detect_main_branch(repo: &Path) -> Result<String> {
    if let Ok(output) = run_git(repo, &["symbolic-ref", "refs/remotes/origin/HEAD"]) {
        if let Some(name) = output.trim().strip_prefix("refs/remotes/") {
            return Ok(name.to_string());
        }
    }
    for candidate in ["origin/main", "origin/master", "main", "master", "develop"] {
        if run_git(
            repo,
            &["rev-parse", "--verify", &format!("{candidate}^{{}}")],
        )
        .is_ok()
        {
            return Ok(candidate.to_string());
        }
    }
    run_git(repo, &["branch", "--show-current"])
}

/// Parses `git for-each-ref --format=%(refname:short)` output into `(name, is_main)` pairs.
/// The shared parser lives in `shellcmd::git` so the shell backend reads the same output.
pub(super) fn parse_branch_list(output: &str, main_branch: &str) -> Vec<(String, bool)> {
    crate::shellcmd::git::parse_branch_list(output, main_branch)
}

fn all_branches(
    repo: &Path,
    max_branch_count: Option<usize>,
    include_remotes: bool,
) -> Result<Vec<(String, bool)>> {
    let main_branch = detect_main_branch(repo).unwrap_or_else(|e| {
        log::warn!("Failed to detect main branch: {e}");
        "origin/main".to_string()
    });
    let count_arg = format!("--count={}", max_branch_count.unwrap_or(100));
    let mut args = vec![
        "for-each-ref",
        count_arg.as_str(),
        "--sort=-committerdate",
        "--format=%(refname:short)",
        "refs/heads",
    ];
    if include_remotes {
        args.push("refs/remotes");
    }
    let output = run_git(repo, &args)?;
    Ok(parse_branch_list(&output, &main_branch))
}

/// Parses `git diff --numstat` output into `(path, additions, deletions)`. Binary files render
/// as `-\t-\t<path>`; unparsable counts fall back to 0.
pub(super) fn parse_numstat(output: &str) -> Vec<(String, u64, u64)> {
    output
        .lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            (parts.len() >= 3).then(|| {
                (
                    parts[2].to_string(),
                    parts[0].parse().unwrap_or(0),
                    parts[1].parse().unwrap_or(0),
                )
            })
        })
        .collect()
}

fn committed_branch_files(repo: &Path) -> Result<Vec<(String, u64, u64)>> {
    let main_branch = detect_main_branch(repo)?;
    let merge_base = match run_git(repo, &["merge-base", "HEAD", main_branch.trim()]) {
        Ok(output) => output.trim().to_string(),
        Err(err) => {
            log::warn!("Could not determine merge base against branch {main_branch}: {err:#}");
            return Ok(Vec::new());
        }
    };
    let output = run_git(repo, &["diff", "--numstat", &merge_base, "HEAD"]).unwrap_or_default();
    Ok(parse_numstat(&output))
}

pub(super) fn branches_success_response(branches: Vec<(String, bool)>) -> GetBranchesResponse {
    GetBranchesResponse {
        result: Some(get_branches_response::Result::Success(GetBranchesSuccess {
            branches: branches
                .into_iter()
                .map(|(name, is_main)| BranchInfo { name, is_main })
                .collect(),
        })),
    }
}

pub(super) fn branches_error_response(message: String) -> GetBranchesResponse {
    GetBranchesResponse {
        result: Some(get_branches_response::Result::Error(GetBranchesError {
            message,
        })),
    }
}

pub(super) fn branches_result_to_response(
    result: Result<Vec<(String, bool)>>,
) -> GetBranchesResponse {
    match result {
        Ok(branches) => branches_success_response(branches),
        Err(e) => branches_error_response(format!("{e:#}")),
    }
}

pub(super) fn files_success_response(
    files: Vec<(String, u64, u64)>,
) -> GetCommittedBranchFilesResponse {
    GetCommittedBranchFilesResponse {
        result: Some(get_committed_branch_files_response::Result::Success(
            GetCommittedBranchFilesSuccess {
                files: files
                    .into_iter()
                    .map(|(path, additions, deletions)| FileChangeEntry {
                        path,
                        additions,
                        deletions,
                    })
                    .collect(),
            },
        )),
    }
}

pub(super) fn files_error_response(message: String) -> GetCommittedBranchFilesResponse {
    GetCommittedBranchFilesResponse {
        result: Some(get_committed_branch_files_response::Result::Error(
            GetCommittedBranchFilesError { message },
        )),
    }
}

pub(super) fn files_result_to_response(
    result: Result<Vec<(String, u64, u64)>>,
) -> GetCommittedBranchFilesResponse {
    match result {
        Ok(files) => files_success_response(files),
        Err(e) => files_error_response(format!("{e:#}")),
    }
}

/// Handles `GetBranches`. Blocking: the caller runs it on a worker thread.
pub(super) fn handle_get_branches(msg: GetBranches) -> GetBranchesResponse {
    let repo = match canonical_repo_path(&msg.repo_path) {
        Ok(repo) => repo,
        Err(e) => return branches_error_response(format!("{e}")),
    };
    let max = msg
        .max_branch_count
        .map(|c| (c as usize).min(MAX_BRANCH_COUNT_CAP));
    branches_result_to_response(all_branches(&repo, max, msg.include_remotes))
}

/// Handles `GetCommittedBranchFiles`. Blocking: the caller runs it on a worker thread.
pub(super) fn handle_get_committed_branch_files(
    msg: GetCommittedBranchFilesRequest,
) -> GetCommittedBranchFilesResponse {
    let repo = match canonical_repo_path(&msg.repo_path) {
        Ok(repo) => repo,
        Err(e) => return files_error_response(format!("{e}")),
    };
    files_result_to_response(committed_branch_files(&repo))
}

#[cfg(test)]
#[path = "git_tests.rs"]
mod tests;
