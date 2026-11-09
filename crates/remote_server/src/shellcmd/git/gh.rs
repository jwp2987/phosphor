//! The GitHub CLI (`gh`) lookups behind the PR chip and the repository chip, shared by the app's
//! local backend, the helper and the shell backend: one argv, one JSON reading, one set of
//! "this just means there is no PR" error rules.
//!
//! Only compiled where `serde_json` is (the `client` and `daemon` features). Nothing here spawns
//! a process: the [`GitRunner`] is the only door out.

use std::path::Path;

use anyhow::{Result, anyhow};

use super::ops::detect_current_branch;
use super::parse::{
    GH_PR_VIEW_ARGS, GH_REPO_VIEW_ARGS, is_pr_lookup_not_applicable_error,
    is_repository_lookup_not_applicable_error,
};
use super::runner::{GitRunner, gh_text, git_text};
use super::types::{PrInfo, RepositoryInfo};

/// The host part of a URL such as `https://github.com/owner/repo`, lower-cased, without user info
/// or port. `None` when the string has no `scheme://host`.
pub fn host_from_url(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    if scheme.is_empty()
        || !scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority.rsplit('@').next().unwrap_or("");
    let host = if host_port.starts_with('[') {
        // An IPv6 literal keeps its brackets, as `Url::host_str` returns it.
        host_port
            .find(']')
            .map(|end| &host_port[..=end])
            .unwrap_or("")
    } else {
        host_port.split(':').next().unwrap_or("")
    };
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

/// Reads `gh repo view --json name,owner,url` output.
pub fn repository_info_from_gh_output(output: &str) -> Result<RepositoryInfo> {
    let parsed: serde_json::Value = serde_json::from_str(output.trim())
        .map_err(|e| anyhow!("Failed to parse gh output: {e}"))?;
    let name = parsed["name"]
        .as_str()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| anyhow!("Missing 'name' in gh output"))?
        .to_string();
    let owner = parsed["owner"]["login"]
        .as_str()
        .filter(|owner| !owner.is_empty())
        .ok_or_else(|| anyhow!("Missing 'owner.login' in gh output"))?
        .to_string();
    // The host is best-effort: parsed from the repo URL when present.
    let host = parsed["url"].as_str().and_then(host_from_url);
    Ok(RepositoryInfo {
        name,
        owner: Some(owner),
        host,
    })
}

/// Reads `gh pr view --json number,url,state,isDraft,baseRefName` output.
pub fn pr_info_from_gh_output(output: &str) -> Result<PrInfo> {
    let parsed: serde_json::Value = serde_json::from_str(output.trim())
        .map_err(|e| anyhow!("Failed to parse gh output: {e}"))?;
    let number = parsed["number"]
        .as_u64()
        .ok_or_else(|| anyhow!("Missing 'number' in gh output"))?;
    let url = parsed["url"]
        .as_str()
        .ok_or_else(|| anyhow!("Missing 'url' in gh output"))?
        .to_string();
    let state = parsed["state"]
        .as_str()
        .ok_or_else(|| anyhow!("Missing 'state' in gh output"))?
        .to_string();
    let draft = parsed["isDraft"]
        .as_bool()
        .ok_or_else(|| anyhow!("Missing 'isDraft' in gh output"))?;
    let base_branch = parsed["baseRefName"]
        .as_str()
        .ok_or_else(|| anyhow!("Missing 'baseRefName' in gh output"))?
        .to_string();
    Ok(PrInfo {
        number,
        url,
        state,
        draft,
        base_branch,
    })
}

/// Looks up the GitHub repository for `repo` via `gh repo view`. `Ok(None)` when the path is not
/// a work tree, or `gh` authoritatively reports there is no GitHub repository to resolve.
pub async fn get_repository_info(
    runner: &dyn GitRunner,
    repo: &Path,
) -> Result<Option<RepositoryInfo>> {
    if git_text(runner, repo, &["rev-parse", "--is-inside-work-tree"])
        .await
        .is_err()
    {
        return Ok(None);
    }
    match gh_text(runner, repo, &GH_REPO_VIEW_ARGS).await {
        Ok(stdout) => repository_info_from_gh_output(&stdout).map(Some),
        Err(e) => {
            if is_repository_lookup_not_applicable_error(&e.to_string()) {
                Ok(None)
            } else {
                Err(e)
            }
        }
    }
}

/// Looks up the PR for the current branch via `gh pr view`. `Ok(None)` when the context is not
/// eligible (not a work tree, detached HEAD, no GitHub remote) or the branch simply has no PR.
/// `Err` for real failures (auth, network, `gh` not installed).
pub async fn get_pr_for_branch(runner: &dyn GitRunner, repo: &Path) -> Result<Option<PrInfo>> {
    if git_text(runner, repo, &["rev-parse", "--is-inside-work-tree"])
        .await
        .is_err()
    {
        return Ok(None);
    }
    // A detached HEAD has no branch to look a PR up for, so skip `gh` entirely.
    if git_text(
        runner,
        repo,
        &["symbolic-ref", "--quiet", "--short", "HEAD"],
    )
    .await
    .is_err()
    {
        return Ok(None);
    }
    match gh_text(runner, repo, &GH_PR_VIEW_ARGS).await {
        Ok(stdout) => pr_info_from_gh_output(&stdout).map(Some),
        Err(e) => {
            if is_pr_lookup_not_applicable_error(&e.to_string()) {
                Ok(None)
            } else {
                Err(e)
            }
        }
    }
}

/// The branch a PR lookup would be for: `Some(name)` on a branch, `None` when detached or not a
/// repository. Used to tell whether the branch moved since the last lookup.
pub async fn branch_for_pr_lookup(runner: &dyn GitRunner, repo: &Path) -> Option<String> {
    detect_current_branch(runner, repo)
        .await
        .ok()
        .filter(|b| b != "HEAD" && !b.is_empty())
}

#[cfg(test)]
#[path = "gh_tests.rs"]
mod tests;
