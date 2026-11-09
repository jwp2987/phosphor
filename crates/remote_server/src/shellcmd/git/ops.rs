//! Git algorithms over a [`GitRunner`]. Each is the exact sequence of commands, fallbacks and
//! error handling the app ran locally before the move; only the process spawn is abstracted.

use std::future::Future;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};

use super::parse::*;
use super::runner::{GitRunner, gh_text, git_text, git_text_stdin};
use super::types::*;

/// Maximum number of characters of diff content sent to AI for commit message / PR text.
pub const MAX_DIFF_CHARS_FOR_AI: usize = 16_000;

/// Fetches the current git branch. In detached HEAD state this is the literal string "HEAD".
pub async fn detect_current_branch(runner: &dyn GitRunner, repo: &Path) -> Result<String> {
    log::debug!("[GIT OPERATION] git.rs detect_current_branch git rev-parse --abbrev-ref HEAD");
    let result = git_text(runner, repo, &["rev-parse", "--abbrev-ref", "HEAD"]).await;

    if result.is_err() {
        log::debug!("[GIT OPERATION] git.rs detect_current_branch git branch --show-current");
        git_text(runner, repo, &["branch", "--show-current"]).await
    } else {
        result
    }
    .map(|branch_name| branch_name.trim().to_owned())
}

/// Like [`detect_current_branch`], but a detached HEAD yields the short commit SHA.
pub async fn detect_current_branch_display(runner: &dyn GitRunner, repo: &Path) -> Result<String> {
    let branch = detect_current_branch(runner, repo).await?;
    if branch == "HEAD" {
        git_text(runner, repo, &["rev-parse", "--short", "HEAD"])
            .await
            .map(|sha| sha.trim().to_owned())
    } else {
        Ok(branch)
    }
}

/// Detects the main branch using git-branchless style heuristics.
pub async fn detect_main_branch(runner: &dyn GitRunner, repo: &Path) -> Result<String> {
    // First try to get the default branch from origin.
    log::debug!(
        "[GIT OPERATION] git.rs detect_main_branch git symbolic-ref refs/remotes/origin/HEAD"
    );
    if let Ok(output) = git_text(runner, repo, &["symbolic-ref", "refs/remotes/origin/HEAD"]).await
    {
        let branch_ref = output.trim();
        if let Some(branch_name) = branch_ref.strip_prefix("refs/remotes/") {
            return Ok(branch_name.to_string());
        }
    }

    // Fallback: try common main branch names in order of preference.
    for candidate in ["origin/main", "origin/master", "main", "master", "develop"] {
        log::debug!(
            "[GIT OPERATION] git.rs detect_main_branch git rev-parse --verify {candidate}^{{}}"
        );
        let result = git_text(
            runner,
            repo,
            &["rev-parse", "--verify", &format!("{candidate}^{{}}")],
        )
        .await;
        if result.is_ok() {
            return Ok(candidate.to_string());
        }
    }

    // Final fallback if all else fails.
    log::debug!("[GIT OPERATION] git.rs detect_main_branch git branch --show-current");
    git_text(runner, repo, &["branch", "--show-current"]).await
}

/// Returns the SHA where `HEAD` forked from any other ref.
pub async fn detect_fork_point(
    runner: &dyn GitRunner,
    repo: &Path,
    current_branch_name: Option<&str>,
) -> Result<Option<String>> {
    // Exclude `<current>` and `origin/<current>` so the branch isn't subtracted from itself.
    let current = current_branch_name
        .map(str::trim)
        .filter(|branch| !branch.is_empty() && *branch != "HEAD");

    let branch_exclude = current.map(|c| format!("--exclude={c}"));
    let remote_exclude = current.map(|c| format!("--exclude=origin/{c}"));

    let mut args: Vec<&str> = vec!["rev-list", "HEAD", "--not"];
    args.extend(branch_exclude.as_deref());
    args.push("--branches");
    args.extend(remote_exclude.as_deref());
    args.push("--remotes");

    let unique = match git_text(runner, repo, &args).await {
        Ok(out) => out,
        Err(e) => {
            log::debug!("detect_fork_point: rev-list failed: {e}");
            return Ok(None);
        }
    };

    // Last non-empty line = oldest unique commit; its parent = fork point.
    // No unique commits means HEAD is fully shared, so fork = HEAD.
    let target = match unique.lines().rfind(|l| !l.trim().is_empty()) {
        Some(sha) => format!("{}^", sha.trim()),
        None => "HEAD".to_string(),
    };
    Ok(git_text(runner, repo, &["rev-parse", &target])
        .await
        .ok()
        .map(|s| s.trim().to_string()))
}

/// Tracked per-file change entries, plus the untracked paths (when `include_unstaged`) so the
/// caller can count their lines in whatever way its filesystem allows (the app reads the file;
/// a shell backend uses `wc -l`). Untracked entries are NOT in the first list.
pub async fn file_change_entries_tracked(
    runner: &dyn GitRunner,
    repo: &Path,
    include_unstaged: bool,
) -> (Vec<FileChangeEntry>, Vec<String>) {
    let args: &[&str] = if include_unstaged {
        &["diff", "--numstat", "HEAD"]
    } else {
        &["diff", "--cached", "--numstat"]
    };
    let output = git_text(runner, repo, args).await.unwrap_or_default();
    let entries = parse_numstat_entries(&output);

    let mut untracked_paths = Vec::new();
    if include_unstaged {
        if let Ok(untracked) = git_text(
            runner,
            repo,
            &["ls-files", "--others", "--exclude-standard"],
        )
        .await
        {
            untracked_paths.extend(
                untracked
                    .lines()
                    .filter(|l| !l.is_empty())
                    .map(str::to_string),
            );
        }
    }
    (entries, untracked_paths)
}

/// Unpushed commits: `<upstream>..HEAD`, or `<fork_point>..HEAD` if no upstream.
pub async fn get_unpushed_commits(
    runner: &dyn GitRunner,
    repo: &Path,
    current_branch_name: Option<&str>,
    upstream_ref: Option<&str>,
) -> Result<Vec<Commit>> {
    let output = if let Some(upstream_ref) = upstream_ref.map(str::trim).filter(|s| !s.is_empty()) {
        let range = format!("{upstream_ref}..HEAD");
        git_text(
            runner,
            repo,
            &["log", &range, "--format=COMMIT:%H\t%s", "--numstat"],
        )
        .await?
    } else {
        // No upstream: fall back to the fork-point commit so we show exactly the commits
        // unique to this branch.
        let fork_point = detect_fork_point(runner, repo, current_branch_name)
            .await
            .ok()
            .flatten();

        let range = match fork_point {
            Some(sha) => format!("{sha}..HEAD"),
            None => "HEAD".to_string(),
        };

        git_text(
            runner,
            repo,
            &["log", &range, "--format=COMMIT:%H\t%s", "--numstat"],
        )
        .await
        .inspect_err(|e| log::warn!("Fallback unpushed-commits log failed: {e}"))
        .unwrap_or_default()
    };
    parse_commit_log(&output)
}

/// The branch's unpushed commits together with its upstream tracking ref. Failures read as
/// `(Vec::new(), None)`.
pub async fn compute_unpushed_state(
    runner: &dyn GitRunner,
    repo: &Path,
) -> (Vec<Commit>, Option<String>) {
    let current_branch = detect_current_branch(runner, repo).await.ok();
    let upstream_ref = git_text(
        runner,
        repo,
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    )
    .await
    .ok()
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty());
    let unpushed = get_unpushed_commits(
        runner,
        repo,
        current_branch.as_deref(),
        upstream_ref.as_deref(),
    )
    .await
    .unwrap_or_default();
    (unpushed, upstream_ref)
}

/// The upstream of the current branch and its ahead/behind/equivalent counts:
/// `(upstream, counts)`. `counts` is `None` when the upstream is missing or the count failed.
pub async fn read_branch_tracking(
    runner: &dyn GitRunner,
    repo: &Path,
) -> (Option<String>, Option<(u32, u32, u32)>) {
    let upstream = git_text(
        runner,
        repo,
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    )
    .await
    .ok()
    .and_then(|output| parse_upstream_ref(&output));

    if upstream.is_none() {
        return (None, None);
    }

    let counts = git_text(
        runner,
        repo,
        &[
            "rev-list",
            "--left-right",
            "--cherry-mark",
            "--count",
            "HEAD...@{u}",
        ],
    )
    .await
    .ok()
    .and_then(|output| parse_tracking_counts(&output));
    (upstream, counts)
}

/// The files changed in one commit, with per-file stats.
pub async fn get_commit_files(
    runner: &dyn GitRunner,
    repo: &Path,
    hash: &str,
) -> Result<Vec<FileChangeEntry>> {
    let output = git_text(
        runner,
        repo,
        &[
            "diff-tree",
            "--root",
            "--no-commit-id",
            "-r",
            "--numstat",
            hash,
        ],
    )
    .await?;
    Ok(parse_numstat_entries_splitn(&output))
}

/// The end ref for a PR-shaped diff: `origin/<current>` if it exists, else `HEAD`.
async fn pr_range(runner: &dyn GitRunner, repo: &Path) -> Result<String> {
    let base = detect_main_branch(runner, repo).await?;
    let base = base.trim();
    let current = detect_current_branch(runner, repo).await?;
    let remote_ref = format!("origin/{current}");

    let end_ref = if git_text(runner, repo, &["rev-parse", "--verify", &remote_ref])
        .await
        .is_ok()
    {
        remote_ref
    } else {
        "HEAD".to_string()
    };
    Ok(format!("{base}..{end_ref}"))
}

/// Per-file stats for what would land in a PR.
pub async fn get_branch_diff_entries(
    runner: &dyn GitRunner,
    repo: &Path,
) -> Result<Vec<FileChangeEntry>> {
    let range = pr_range(runner, repo).await?;
    let output = git_text(runner, repo, &["diff", "--numstat", &range]).await?;
    Ok(parse_numstat_entries(&output))
}

/// PR-ready diff, truncated for AI token limits.
pub async fn get_diff_for_pr(runner: &dyn GitRunner, repo: &Path) -> Result<String> {
    let range = pr_range(runner, repo).await?;
    let mut diff = git_text(runner, repo, &["diff", &range]).await?;
    if diff.len() > MAX_DIFF_CHARS_FOR_AI {
        diff = format!(
            "{}\n... (diff truncated)",
            truncate_on_char_boundary(&diff, MAX_DIFF_CHARS_FOR_AI)
        );
    }
    Ok(diff)
}

/// Commit subject lines on the current branch since the default branch.
pub async fn get_branch_commit_messages(
    runner: &dyn GitRunner,
    repo: &Path,
) -> Result<Vec<String>> {
    let base = detect_main_branch(runner, repo).await?;
    let base = base.trim();
    let range = format!("{base}..HEAD");
    let output = git_text(runner, repo, &["log", &range, "--format=%s"]).await?;
    Ok(output
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect())
}

/// Commits changes. If `include_unstaged`, stages all changes first via `git add -A`.
pub async fn run_commit(
    runner: &dyn GitRunner,
    repo: &Path,
    message: &str,
    include_unstaged: bool,
) -> Result<String> {
    if include_unstaged {
        git_text(runner, repo, &["add", "-A"]).await?;
    }
    // The message travels on stdin (`-F -`), never in an argv a process listing or an audit log
    // would show (#1864).
    git_text_stdin(
        runner,
        repo,
        &["commit", "-F", "-"],
        Some(message.as_bytes()),
    )
    .await
}

/// Pushes the given branch to origin, setting upstream tracking.
pub async fn run_push(runner: &dyn GitRunner, repo: &Path, branch: &str) -> Result<String> {
    git_text(runner, repo, &["push", "--set-upstream", "origin", branch]).await
}

/// Fast-forward-only pull of `branch` from origin. Never merges.
pub async fn run_pull(runner: &dyn GitRunner, repo: &Path, branch: &str) -> Result<String> {
    git_text(runner, repo, &["pull", "--ff-only", "origin", branch]).await
}

/// Creates a PR for the current branch (must already be pushed). `--fill` when title/body are
/// not both given. Always targets the detected default branch.
pub async fn create_pr(
    runner: &dyn GitRunner,
    repo: &Path,
    title: Option<&str>,
    body: Option<&str>,
) -> Result<PrInfo> {
    let base = detect_main_branch(runner, repo).await?;
    let base = base.trim();
    let base = base.strip_prefix("origin/").unwrap_or(base);
    let args = gh_pr_create_args(base, title, body);
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let stdout = gh_text(runner, repo, &arg_refs).await?;
    // `gh pr create` prints the PR URL on success.
    let url = stdout.trim().to_string();
    let number = parse_pr_number_from_url(&url)
        .ok_or_else(|| anyhow!("Could not parse PR number from URL: {url}"))?;
    Ok(PrInfo {
        number,
        url,
        state: "OPEN".to_string(),
        draft: false,
        base_branch: base.to_string(),
    })
}

/// Runs the commit chain: always commits, then optionally pushes, then optionally creates a PR
/// per `mode`, and returns the post-chain delta (refreshed unpushed commits + upstream ref) plus
/// any created PR. The delta is computed once after the whole chain settles.
///
/// Phosphor (BYOP) divergence from Warp: the create-PR stage always runs `gh pr create --fill`
/// (no cloud title/body autogeneration, #116).
pub async fn run_commit_chain(
    runner: &dyn GitRunner,
    repo: &Path,
    mode: CommitChainMode,
    message: &str,
    include_unstaged: bool,
    branch: &str,
) -> Result<(Vec<Commit>, Option<String>, Option<PrInfo>)> {
    run_commit(runner, repo, message, include_unstaged).await?;
    let pr_info = match mode {
        CommitChainMode::CommitOnly => None,
        CommitChainMode::CommitAndPush => {
            run_push(runner, repo, branch).await?;
            None
        }
        CommitChainMode::CommitAndCreatePr => {
            run_push(runner, repo, branch).await?;
            Some(create_pr(runner, repo, None, None).await?)
        }
    };
    let (commits, upstream_ref) = compute_unpushed_state(runner, repo).await;
    Ok((commits, upstream_ref, pr_info))
}

/// Whether the repo is mid-operation (merge / cherry-pick / revert / rebase) or locked. The
/// sentinels live under `<repo>/.git`; HOW they are tested is the caller's: `probe` receives the
/// git dir and reports whether any of [`OPERATION_SENTINELS`] exists in it (a local `exists()`,
/// the helper's `std::fs`, or [`operation_probe_script`] over the out-of-band executor). Shared
/// by the local pre-emptive guard and the execution-time check on the daemon / shell backend.
pub async fn guard_git_operation_in_progress<F, Fut>(repo: &Path, probe: F) -> Result<bool>
where
    F: FnOnce(PathBuf) -> Fut,
    Fut: Future<Output = Result<bool>>,
{
    probe(repo.join(".git")).await
}

/// Applies `source` (a patch file path, or `-` with `stdin`) to the index only. With `reverse`,
/// un-stages instead. `recount` adds `--recount` (for client-reconstructed hunks over stdin).
pub async fn apply_patch_cached(
    runner: &dyn GitRunner,
    repo: &Path,
    source: &str,
    stdin: Option<&[u8]>,
    reverse: bool,
    recount: bool,
) -> Result<String> {
    let args = apply_cached_args(reverse, recount, source);
    git_text_stdin(runner, repo, &args, stdin).await
}

/// Stages (or, with `unstage`, un-stages) whole files by repo-relative path. Before the first
/// commit `git restore --staged` fails, and `git rm --cached` is the equivalent there.
pub async fn stage_paths(
    runner: &dyn GitRunner,
    repo: &Path,
    relative_paths: &[String],
    unstage: bool,
) -> Result<String> {
    if relative_paths.is_empty() {
        return Err(anyhow!("No paths given to stage"));
    }

    let args = stage_paths_args(relative_paths, unstage);
    match git_text(runner, repo, &args).await {
        Ok(output) => Ok(output),
        Err(err) if unstage && is_unresolvable_head_error(&err.to_string()) => {
            git_text(runner, repo, &rm_cached_args(relative_paths)).await
        }
        Err(err) => Err(err),
    }
}

/// Stashes uncommitted changes (untracked included) for specific files. "No local changes to
/// save" is not an error.
pub async fn stash_uncommitted_changes(
    runner: &dyn GitRunner,
    repo: &Path,
    app_name: &str,
    relative_paths: &[String],
) -> Result<()> {
    let msg = stash_message(app_name, relative_paths);
    let stash_args = stash_args(&msg, relative_paths);

    log::debug!(
        "[GIT OPERATION] diff_state.rs stash_uncommitted_changes git {}",
        stash_args.join(" ")
    );
    match git_text(runner, repo, &stash_args).await {
        Ok(_) => Ok(()),
        Err(err) => {
            let err_msg = err.to_string();
            // If there are no local changes to stash, git stash fails; that is fine.
            if err_msg.contains("No local changes to save") {
                Ok(())
            } else {
                let context = if relative_paths.len() == 1 {
                    relative_paths[0].clone()
                } else {
                    format!("{} files", relative_paths.len())
                };
                Err(anyhow!(
                    "Failed to stash changes for {}: {}",
                    context,
                    err_msg
                ))
            }
        }
    }
}

/// Runs `git restore --staged --worktree --source=<branch>` and `git clean -fd` for files.
/// `remove_from_fs` is called with a repo-relative path after `git rm` for a file that
/// is not in `branch`; it deletes the working-tree file on whichever filesystem the caller has
/// (a no-op where `git rm -f` already did).
pub async fn git_restore_and_clean(
    runner: &dyn GitRunner,
    repo: &Path,
    relative_paths: &[String],
    branch: &str,
    remove_from_fs: &(dyn Fn(&str) + Send + Sync),
) -> Result<()> {
    let source_arg = format!("--source={branch}");
    let mut restore_args = vec![
        "restore",
        "--staged",
        "--worktree",
        source_arg.as_str(),
        "--",
    ];
    for path in relative_paths {
        restore_args.push(path.as_str());
    }

    log::debug!(
        "[GIT OPERATION] diff_state.rs git_restore_and_clean git {}",
        restore_args.join(" ")
    );
    let restore_res = git_text(runner, repo, &restore_args).await;

    match restore_res {
        Ok(_) => {
            // Clean untracked files for these specific paths.
            let mut clean_args = vec!["clean", "-fd"];
            for path in relative_paths {
                clean_args.push(path.as_str());
            }
            log::debug!(
                "[GIT OPERATION] diff_state.rs git_restore_and_clean git {}",
                clean_args.join(" ")
            );
            if let Err(err) = git_text(runner, repo, &clean_args).await {
                log::warn!("Failed to clean untracked files: {err}");
            }
            Ok(())
        }
        Err(err) => {
            let err_msg = err.to_string();
            if branch == "HEAD" && err_msg.contains("could not resolve HEAD") {
                let mut clean_args = vec!["clean", "-fd"];
                for path in relative_paths {
                    clean_args.push(path.as_str());
                }
                log::debug!(
                    "[GIT OPERATION] diff_state.rs git_restore_and_clean git {}",
                    clean_args.join(" ")
                );
                if let Err(err) = git_text(runner, repo, &clean_args).await {
                    log::warn!("Failed to clean untracked files: {err}");
                }
                Ok(())
            } else if err_msg.contains("did not match any file(s) known to git") {
                // If some files don't exist in the branch, we need to remove them. The batch
                // restore refused ALL of its paths because of those, so with more than one path
                // each is first restored on its own: a tracked file must be restored, never
                // removed along with the strangers it was batched with.
                for file_path in relative_paths {
                    if relative_paths.len() > 1 {
                        let single = [
                            "restore",
                            "--staged",
                            "--worktree",
                            source_arg.as_str(),
                            "--",
                            file_path.as_str(),
                        ];
                        if git_text(runner, repo, &single).await.is_ok() {
                            continue;
                        }
                    }
                    log::debug!(
                        "[GIT OPERATION] diff_state.rs git_restore_and_clean git rm -f -- {file_path}"
                    );
                    let rm_res =
                        git_text(runner, repo, &["rm", "-f", "--", file_path.as_str()]).await;

                    if let Err(rm_err) = rm_res {
                        let rm_err_msg = rm_err.to_string();
                        if rm_err_msg.contains("did not match any files") {
                            // The file was staged but isn't in the working directory, e.g. it
                            // was locally deleted.
                            log::debug!(
                                "[GIT OPERATION] diff_state.rs git_restore_and_clean git reset -- {file_path}"
                            );
                            if let Err(e) =
                                git_text(runner, repo, &["reset", "--", file_path.as_str()]).await
                            {
                                log::warn!("Failed to unstage file '{file_path}': {e}");
                            }
                        } else {
                            log::warn!("Failed to remove file '{file_path}': {rm_err_msg}");
                        }
                    }

                    remove_from_fs(file_path);
                }
                Ok(())
            } else {
                Err(err)
            }
        }
    }
}

/// Discards changes for files. Renames are handled first (remove the new path, restore the old
/// one), then everything else. See [`git_restore_and_clean`] for `remove_from_fs`.
pub async fn discard_files(
    runner: &dyn GitRunner,
    repo: &Path,
    targets: Vec<DiscardTarget>,
    should_stash: bool,
    branch: &str,
    app_name: &str,
    remove_from_fs: &(dyn Fn(&str) + Send + Sync),
) -> Result<()> {
    let (renamed, other): (Vec<_>, Vec<_>) = targets
        .into_iter()
        .partition(|target| target.renamed_from.is_some());

    // Handle renamed files specially.
    if !renamed.is_empty() {
        if branch == "HEAD" && should_stash {
            let renamed_paths: Vec<String> = renamed.iter().map(|t| t.rel_path.clone()).collect();
            stash_uncommitted_changes(runner, repo, app_name, &renamed_paths).await?;

            for target in &renamed {
                if let Some(old_path) = &target.renamed_from {
                    log::debug!(
                        "[GIT OPERATION] diff_state.rs discard_files_impl git restore --staged --worktree -- {old_path}"
                    );
                    let _ = git_text(
                        runner,
                        repo,
                        &["restore", "--staged", "--worktree", "--", old_path],
                    )
                    .await;
                }
            }
        } else {
            for target in renamed {
                if let Some(old_path) = &target.renamed_from {
                    let relative_new_path = &target.rel_path;

                    // Remove the new file.
                    log::debug!(
                        "[GIT OPERATION] diff_state.rs discard_files_impl git rm -f -- {relative_new_path}"
                    );
                    if let Err(e) =
                        git_text(runner, repo, &["rm", "-f", "--", relative_new_path]).await
                    {
                        log::warn!("Failed to remove renamed file '{relative_new_path}': {e}");
                    }

                    // Restore the old file from the branch using git checkout: the old path
                    // doesn't exist in the working directory yet, and git restore requires it.
                    log::debug!(
                        "[GIT OPERATION] diff_state.rs discard_files_impl git checkout {branch} -- {old_path}"
                    );
                    if let Err(e) =
                        git_text(runner, repo, &["checkout", branch, "--", old_path]).await
                    {
                        log::error!(
                            "Failed to restore old file '{old_path}' from branch '{branch}': {e}"
                        );
                    }
                }
            }
        }
    }

    // Handle other files normally.
    if !other.is_empty() {
        let relative_paths: Vec<String> = other.iter().map(|t| t.rel_path.clone()).collect();

        if branch == "HEAD" && should_stash {
            stash_uncommitted_changes(runner, repo, app_name, &relative_paths).await?;
        } else {
            git_restore_and_clean(runner, repo, &relative_paths, branch, remove_from_fs).await?;
        }
    }
    Ok(())
}

/// How many branches `GetBranches` returns when the caller names no count.
pub const DEFAULT_BRANCH_COUNT: usize = 100;
/// The most branches one `GetBranches` returns, whatever the caller asked (the helper's cap).
pub const MAX_BRANCH_COUNT_CAP: usize = 500;

/// The branch picker's list as `(name, is_main)` pairs, most recently committed first:
/// `git for-each-ref` over `refs/heads` (and `refs/remotes` with `include_remotes`), without HEAD
/// pointers, without duplicates. Same rules as the app's local listing, so the picker looks the
/// same on every backend. The main branch is detected ([`detect_main_branch`]).
pub async fn get_all_branches(
    runner: &dyn GitRunner,
    repo: &Path,
    max_branch_count: Option<usize>,
    include_remotes: bool,
) -> Result<Vec<(String, bool)>> {
    let main_branch = match detect_main_branch(runner, repo).await {
        Ok(branch) => branch,
        Err(err) => {
            log::warn!("Failed to detect main branch: {err}");
            "origin/main".to_string()
        }
    };
    let count = max_branch_count
        .unwrap_or(DEFAULT_BRANCH_COUNT)
        .min(MAX_BRANCH_COUNT_CAP);
    let count_arg = format!("--count={count}");
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
    let output = git_text(runner, repo, &args).await?;
    let mut branches = Vec::new();
    for branch in output.lines() {
        let branch = branch.trim();
        if branch.is_empty() || branch.contains("HEAD") || branch.starts_with('(') {
            continue;
        }
        let is_main = branch == main_branch || branch == main_branch.trim_start_matches("origin/");
        branches.push((branch.to_string(), is_main));
    }
    let mut seen = std::collections::HashSet::new();
    branches.retain(|(name, _)| seen.insert(name.clone()));
    Ok(branches)
}

/// The committed-only changed files of the current branch (`git diff --numstat <merge-base>
/// HEAD`) as `(path, additions, deletions)`; binary files and unparsable counts are 0. An
/// unknown merge base is an empty list, like the local listing.
pub async fn get_committed_branch_file_entries(
    runner: &dyn GitRunner,
    repo: &Path,
) -> Result<Vec<(String, u64, u64)>> {
    let main_branch = detect_main_branch(runner, repo).await?;
    let merge_base = match git_text(runner, repo, &["merge-base", "HEAD", main_branch.trim()]).await
    {
        Ok(output) => output.trim().to_string(),
        Err(err) => {
            log::warn!("Could not determine merge base against branch {main_branch}: {err:?}");
            return Ok(Vec::new());
        }
    };
    let output = git_text(runner, repo, &["diff", "--numstat", &merge_base, "HEAD"])
        .await
        .unwrap_or_default();
    let mut entries = Vec::new();
    for line in output.lines() {
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 3 {
            entries.push((
                parts[2].to_string(),
                parts[0].parse().unwrap_or(0),
                parts[1].parse().unwrap_or(0),
            ));
        }
    }
    Ok(entries)
}
