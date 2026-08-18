use std::collections::HashSet;
use std::path::Path;

use anyhow::{anyhow, Result};

#[cfg(test)]
#[path = "git_tests.rs"]
mod tests;

/// Runs a git command and returns the output as a string.
/// Thin wrapper over [`run_git_command_with_env`] with no `PATH` override.
#[cfg(feature = "local_fs")]
pub async fn run_git_command(repo_path: &Path, args: &[&str]) -> Result<String> {
    run_git_command_with_env(repo_path, args, None).await
}

/// Like [`run_git_command`] but sets `PATH` on the child when `path_env` is
/// `Some`. Used by callers whose hooks need user-installed binaries (e.g.
/// the LFS `pre-push` hook → `git-lfs`). See `specs/APP-4188/TECH.md`.
#[cfg(feature = "local_fs")]
pub async fn run_git_command_with_env(
    repo_path: &Path,
    args: &[&str],
    path_env: Option<&str>,
) -> Result<String> {
    use command::Stdio;

    log::debug!(
        "[GIT OPERATION] git.rs run_git_command git {}",
        args.join(" ")
    );
    let mut git_args = vec!["-c", "diff.autoRefreshIndex=false"];
    git_args.extend_from_slice(args);
    let env = git_child_env(path_env);

    let mut cmd = git_command(repo_path, &git_args, &env);
    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let output = cmd
        .output()
        .await
        .map_err(|e| anyhow!("Failed to execute git command: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Handle git diff specific behavior:
    // - Exit code 0: no differences
    // - Exit code 1: differences found (this is normal for diff commands)
    // - Exit code > 1: actual error
    if output.status.success() || (output.status.code() == Some(1) && !stdout.is_empty()) {
        Ok(stdout)
    } else {
        Err(anyhow!("Git command failed: {}, {}", stderr, stdout))
    }
}

/// Environment applied to every git subprocess this module spawns.
///
/// - `GIT_OPTIONAL_LOCKS=0` stops Phosphor's own background git calls from
///   taking the index lock and fighting the user's terminal git.
/// - `GIT_TERMINAL_PROMPT=0` makes a git command that needs credentials fail
///   fast instead of blocking on a prompt. This is a correctness fix, not
///   hygiene: the code-review git dialogs run these commands with no tty they
///   can service, and `GitDialogAction::Cancel` early-returns while the dialog
///   is loading, so a blocked prompt is an unrecoverable spinner rather than a
///   slow operation. Git converts the suppressed prompt into
///   `could not read Username for '<url>': terminal prompts disabled`, which
///   `code_review::git_dialog::user_facing_git_error` maps onto the normal
///   authentication toast.
///
///   This disables only *terminal* prompting. `GIT_ASKPASS` / `core.askPass`
///   GUI helpers and configured credential helpers still run, so users on a
///   credential manager (the common macOS and Windows setup) are unaffected;
///   only the case that would otherwise hang changes behaviour.
///
/// `PATH` is appended only when the caller supplies one — see
/// [`run_git_command_with_env`] for why that matters to LFS hooks.
#[cfg(feature = "local_fs")]
fn git_child_env(path_env: Option<&str>) -> Vec<(&'static str, &str)> {
    let mut env = vec![("GIT_OPTIONAL_LOCKS", "0"), ("GIT_TERMINAL_PROMPT", "0")];
    if let Some(path_env) = path_env {
        env.push(("PATH", path_env));
    }
    env
}

/// Builds the command that runs `git` with `args` in `repo_path`, with `env` set on the child.
///
/// A WSL session's working directory is a `\\wsl$\<distro>\...` UNC path on a Windows host, and
/// the Windows `git.exe` mishandles those: it reports "dubious ownership", produces bogus diffs,
/// and can hang. Such a path is instead routed to the distribution's own git via `wsl.exe`.
///
/// Ported from upstream `d46473504` ("Route Warp-internal git through wsl.exe for WSL UNC working
/// directories", #13793). Upstream this lives in `crates/warp_util/src/git.rs`; this fork has no
/// such module and keeps the shared git-subprocess helpers here instead.
#[cfg(feature = "local_fs")]
fn git_command(repo_path: &Path, args: &[&str], env: &[(&str, &str)]) -> command::r#async::Command {
    use command::r#async::Command;

    // Gated with `cfg!` rather than `#[cfg]` so the translation stays compiled and unit-tested on
    // every platform.
    let translated = if cfg!(windows) {
        translate_for_wsl_unc_cwd(args, repo_path, env)
    } else {
        None
    };

    if let Some(translated) = translated {
        let mut cmd = Command::new("wsl.exe");
        cmd.args(&translated.args);
        // The working directory is deliberately left unset: `--cd` supplies it inside the
        // distribution, which keeps `wsl.exe` itself off the UNC path.
        // A caller-supplied `PATH` rides through the argument vector instead; see `build_wslenv`.
        for (key, value) in env.iter().filter(|(key, _)| !is_path_env_key(key)) {
            cmd.env(key, value);
        }
        // Left unset when empty so the child keeps inheriting the parent's `WSLENV`.
        if !translated.wslenv.is_empty() {
            cmd.env("WSLENV", &translated.wslenv);
        }
        return cmd;
    }

    let mut cmd = Command::new("git");
    cmd.args(args).current_dir(repo_path);
    for (key, value) in env {
        cmd.env(key, value);
    }
    cmd
}

/// A `git` command rewritten to run inside a WSL distribution via `wsl.exe`.
#[cfg(feature = "local_fs")]
#[derive(Debug, PartialEq, Eq)]
struct WslGitCommand {
    args: Vec<String>,
    /// The `WSLENV` value propagating the explicitly-set environment variables into the
    /// distribution; empty when there is nothing to propagate.
    wslenv: String,
}

/// Rewrites a `git` invocation whose working directory is a WSL UNC path into the equivalent
/// `wsl.exe` invocation, carrying `env` across as `WSLENV` entries except for `PATH`, which
/// becomes an argv element (`--exec /usr/bin/env PATH=<value> git ...`). Returns `None` when
/// `repo_path` is not a WSL UNC path.
#[cfg(feature = "local_fs")]
fn translate_for_wsl_unc_cwd(
    args: &[&str],
    repo_path: &Path,
    env: &[(&str, &str)],
) -> Option<WslGitCommand> {
    let unc = warp_util::path::parse_wsl_unc_path(repo_path)?;

    let mut translated_args = vec![
        "--distribution".to_string(),
        unc.distro.clone(),
        "--cd".to_string(),
        unc.linux_path,
        "--exec".to_string(),
    ];
    match env.iter().find(|(key, _)| is_path_env_key(key)) {
        // A caller-supplied `PATH` already names the directory `git` lives in, so no login shell
        // is needed to resolve it.
        Some((_, path_value)) => {
            translated_args.push("/usr/bin/env".to_string());
            translated_args.push(format!("PATH={path_value}"));
            translated_args.push("git".to_string());
        }
        // Otherwise a login shell is needed: `wsl.exe --exec` searches only a minimal default
        // `PATH` (`/usr/bin`, `/bin`, ...), which misses distributions that put `git` elsewhere —
        // NixOS exposes it only under `/etc/profiles`. Arguments ride along as positional
        // parameters so no shell quoting is involved.
        None => {
            translated_args.push("/bin/sh".to_string());
            translated_args.push("-lc".to_string());
            translated_args.push(r#"exec git "$@""#.to_string());
            translated_args.push("git".to_string());
        }
    }
    translated_args.extend(args.iter().map(|arg| translate_arg(arg, &unc.distro)));

    Some(WslGitCommand {
        args: translated_args,
        wslenv: build_wslenv(env),
    })
}

/// Converts an argument that is a UNC path for `distro` into its Linux path. Every other argument
/// is passed through unchanged.
#[cfg(feature = "local_fs")]
fn translate_arg(arg: &str, distro: &str) -> String {
    match warp_util::path::parse_wsl_unc_path(Path::new(arg)) {
        Some(parsed) if parsed.distro.eq_ignore_ascii_case(distro) => parsed.linux_path,
        _ => arg.to_string(),
    }
}

/// Builds the `WSLENV` value advertising the keys of `env` to the distribution, using the `/u`
/// suffix that shares a variable when invoking WSL from Windows. Empty when there is nothing to
/// propagate.
///
/// `PATH` is deliberately excluded: Windows applies a non-disableable Windows-to-WSL `PATH`
/// conversion, and a `PATH` that is already in Linux form fails that conversion and gets
/// truncated. It travels as an argv element instead.
#[cfg(feature = "local_fs")]
fn build_wslenv(env: &[(&str, &str)]) -> String {
    env.iter()
        .map(|(key, _)| key)
        .filter(|key| !is_path_env_key(key))
        .map(|key| format!("{key}/u"))
        .collect::<Vec<_>>()
        .join(":")
}

/// True when `key` names the `PATH` environment variable, compared case-insensitively.
#[cfg(feature = "local_fs")]
fn is_path_env_key(key: &str) -> bool {
    key.eq_ignore_ascii_case("PATH")
}

#[cfg(not(feature = "local_fs"))]
pub async fn run_git_command(_repo_path: &Path, _args: &[&str]) -> Result<String> {
    Err(anyhow!("Not supported on wasm"))
}

#[cfg(not(feature = "local_fs"))]
pub async fn run_git_command_with_env(
    _repo_path: &Path,
    _args: &[&str],
    _path_env: Option<&str>,
) -> Result<String> {
    Err(anyhow!("Not supported on wasm"))
}

/// Returns the set of local branch names for the repo at `repo_path`.
/// Uses a synchronous subprocess call — suitable for call sites in
/// synchronous view handlers where the result is needed immediately.
/// Returns an empty set on any failure (not a git repo, git not found, etc.).
#[cfg(feature = "local_fs")]
pub fn list_local_branches_sync(repo_path: &Path) -> HashSet<String> {
    let output = command::blocking::Command::new("git")
        .args(["branch", "--list", "--format=%(refname:short)"])
        .current_dir(repo_path)
        .stdout(command::Stdio::piped())
        .stderr(command::Stdio::null())
        .output();

    match output {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|line| line.trim().to_string())
            .filter(|line| !line.is_empty())
            .collect(),
        _ => HashSet::new(),
    }
}

#[cfg(not(feature = "local_fs"))]
pub fn list_local_branches_sync(_repo_path: &Path) -> HashSet<String> {
    HashSet::new()
}

/// Fetches the current git branch.
#[cfg(not(feature = "local_fs"))]
pub async fn detect_current_branch(_repo_path: &Path) -> Result<String> {
    Err(anyhow!("Not supported without local_fs"))
}

/// Fetches the current git branch.
/// In detached HEAD state this returns the literal string "HEAD".
#[cfg(feature = "local_fs")]
pub async fn detect_current_branch(repo_path: &Path) -> Result<String> {
    log::debug!("[GIT OPERATION] git.rs detect_current_branch git rev-parse --abbrev-ref HEAD");
    let result = run_git_command(repo_path, &["rev-parse", "--abbrev-ref", "HEAD"]).await;

    if result.is_err() {
        log::debug!("[GIT OPERATION] git.rs detect_current_branch git branch --show-current");
        run_git_command(repo_path, &["branch", "--show-current"]).await
    } else {
        result
    }
    .map(|branch_name| branch_name.trim().to_owned())
}

/// Like [`detect_current_branch`], but in detached HEAD state returns the short
/// commit SHA instead of the literal "HEAD".
/// (Matches the shell command `git symbolic-ref --short HEAD || git rev-parse --short HEAD`.)
#[cfg(feature = "local_fs")]
pub async fn detect_current_branch_display(repo_path: &Path) -> Result<String> {
    let branch = detect_current_branch(repo_path).await?;
    if branch == "HEAD" {
        run_git_command(repo_path, &["rev-parse", "--short", "HEAD"])
            .await
            .map(|sha| sha.trim().to_owned())
    } else {
        Ok(branch)
    }
}

#[cfg(not(feature = "local_fs"))]
pub async fn detect_current_branch_display(_repo_path: &Path) -> Result<String> {
    Err(anyhow!("Not supported without local_fs"))
}

/// Detects the main branch using git-branchless style heuristics.
#[cfg(not(feature = "local_fs"))]
pub async fn detect_main_branch(_repo_path: &Path) -> Result<String> {
    Err(anyhow!("Not supported without local_fs"))
}

/// Detects the main branch using git-branchless style heuristics.
#[cfg(feature = "local_fs")]
pub async fn detect_main_branch(repo_path: &Path) -> Result<String> {
    // First try to get the default branch from origin
    log::debug!(
        "[GIT OPERATION] git.rs detect_main_branch git symbolic-ref refs/remotes/origin/HEAD"
    );
    match run_git_command(repo_path, &["symbolic-ref", "refs/remotes/origin/HEAD"]).await {
        Ok(output) => {
            let branch_ref = output.trim();
            if let Some(branch_name) = branch_ref.strip_prefix("refs/remotes/") {
                return Ok(branch_name.to_string());
            }
        }
        Err(_) => {
            // If remote fetch fails, fall back to candidates.
        }
    }

    // Fallback: try common main branch names in order of preference.
    let candidates = ["origin/main", "origin/master", "main", "master", "develop"];

    for candidate in candidates {
        log::debug!(
            "[GIT OPERATION] git.rs detect_main_branch git rev-parse --verify {candidate}^{{}}"
        );
        let result = run_git_command(
            repo_path,
            &["rev-parse", "--verify", &format!("{candidate}^{{}}")],
        )
        .await;

        if result.is_ok() {
            return Ok(candidate.to_string());
        }
    }

    // Final fallback if all else fails.
    log::debug!("[GIT OPERATION] git.rs detect_main_branch git branch --show-current");
    run_git_command(repo_path, &["branch", "--show-current"]).await
}

/// Returns the SHA where `HEAD` forked from any other ref. Use
/// `<fork>..HEAD` for "commits unique to this branch".
#[cfg(not(feature = "local_fs"))]
pub async fn detect_fork_point(
    _repo_path: &Path,
    _current_branch_name: Option<&str>,
) -> Result<Option<String>> {
    Err(anyhow!("Not supported without local_fs"))
}

/// See the no-`local_fs` stub above for documentation.
#[cfg(feature = "local_fs")]
pub async fn detect_fork_point(
    repo_path: &Path,
    current_branch_name: Option<&str>,
) -> Result<Option<String>> {
    // Exclude `<current>` and `origin/<current>` so the branch isn't
    // subtracted from itself.
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

    let unique = match run_git_command(repo_path, &args).await {
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
    Ok(run_git_command(repo_path, &["rev-parse", &target])
        .await
        .ok()
        .map(|s| s.trim().to_string()))
}

/// Git summary for a repo: current branch + uncommitted diff stats.
#[derive(Debug, Clone)]
#[cfg(feature = "local_fs")]
pub struct RepoGitSummary {
    pub branch: String,
    pub lines_added: u32,
    pub lines_removed: u32,
}

/// Runs git commands in `repo_root` to get current branch + diff stats.
/// Returns None if not a git repo or git is unavailable.
#[cfg(feature = "local_fs")]
pub async fn get_repo_git_summary(repo_root: &Path) -> Option<RepoGitSummary> {
    use crate::context_chips::display_chip::GitLineChanges;

    let branch = {
        log::debug!("[GIT OPERATION] git.rs get_repo_git_summary git symbolic-ref --short HEAD");
        let result = run_git_command(repo_root, &["symbolic-ref", "--short", "HEAD"]).await;
        match result {
            Ok(output) => Some(output.trim().to_string()),
            Err(_) => {
                // Fallback to rev-parse for detached HEAD
                log::debug!(
                    "[GIT OPERATION] git.rs get_repo_git_summary git rev-parse --short HEAD"
                );
                run_git_command(repo_root, &["rev-parse", "--short", "HEAD"])
                    .await
                    .ok()
                    .map(|o| o.trim().to_string())
            }
        }
    };

    // Tracked file changes (git diff --shortstat HEAD doesn't include untracked files).
    log::debug!("[GIT OPERATION] git.rs get_repo_git_summary git diff --shortstat HEAD");
    let stats = run_git_command(repo_root, &["diff", "--shortstat", "HEAD"])
        .await
        .ok()
        .and_then(|o| GitLineChanges::parse_from_git_output(&o));

    let mut lines_added = stats.as_ref().map_or(0, |s| s.lines_added);
    let lines_removed = stats.as_ref().map_or(0, |s| s.lines_removed);

    // Also count lines in untracked files to match what the git diff chip shows.
    log::debug!(
        "[GIT OPERATION] git.rs get_repo_git_summary git ls-files --others --exclude-standard"
    );
    if let Ok(untracked_output) =
        run_git_command(repo_root, &["ls-files", "--others", "--exclude-standard"]).await
    {
        for file_name in untracked_output.lines() {
            if file_name.is_empty() {
                continue;
            }
            lines_added += count_lines_if_text_file(&repo_root.join(file_name));
        }
    }

    let branch = branch?;
    Some(RepoGitSummary {
        branch,
        lines_added,
        lines_removed,
    })
}

/// Short summary of a commit: hash and subject line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub hash: String,
    pub subject: String,
    pub files_changed: usize,
    pub additions: usize,
    pub deletions: usize,
}

/// A single changed file with per-file addition/deletion counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChangeEntry {
    pub path: String,
    pub additions: usize,
    pub deletions: usize,
}

/// Returns per-file change entries. When `include_unstaged` is true, returns all
/// uncommitted changes (staged + unstaged + untracked) vs HEAD; otherwise only staged changes.
#[cfg(feature = "local_fs")]
pub async fn get_file_change_entries(
    repo_path: &Path,
    include_unstaged: bool,
) -> Result<Vec<FileChangeEntry>> {
    let args: &[&str] = if include_unstaged {
        &["diff", "--numstat", "HEAD"]
    } else {
        &["diff", "--cached", "--numstat"]
    };
    let output = run_git_command(repo_path, args).await.unwrap_or_default();
    let mut entries = Vec::new();
    for line in output.lines() {
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 3 {
            entries.push(FileChangeEntry {
                path: parts[2].to_string(),
                additions: parts[0].parse().unwrap_or(0),
                deletions: parts[1].parse().unwrap_or(0),
            });
        }
    }

    // Also include untracked files when showing all changes.
    if include_unstaged {
        if let Ok(untracked) =
            run_git_command(repo_path, &["ls-files", "--others", "--exclude-standard"]).await
        {
            for file_name in untracked.lines() {
                if file_name.is_empty() {
                    continue;
                }
                let additions = count_lines_if_text_file(&repo_path.join(file_name)) as usize;
                entries.push(FileChangeEntry {
                    path: file_name.to_string(),
                    additions,
                    deletions: 0,
                });
            }
        }
    }

    Ok(entries)
}

#[cfg(not(feature = "local_fs"))]
pub async fn get_file_change_entries(
    _repo_path: &Path,
    _include_unstaged: bool,
) -> Result<Vec<FileChangeEntry>> {
    Err(anyhow!("Not supported on wasm"))
}

/// Unpushed commits: `<upstream>..HEAD`, or `<fork_point>..HEAD` if no upstream.
#[cfg(feature = "local_fs")]
pub async fn get_unpushed_commits(
    repo_path: &Path,
    current_branch_name: Option<&str>,
    upstream_ref: Option<&str>,
) -> Result<Vec<Commit>> {
    let output = if let Some(upstream_ref) = upstream_ref.map(str::trim).filter(|s| !s.is_empty()) {
        let range = format!("{upstream_ref}..HEAD");
        run_git_command(
            repo_path,
            &["log", &range, "--format=COMMIT:%H\t%s", "--numstat"],
        )
        .await?
    } else {
        // No upstream — fall back to the fork-point commit so we show
        // exactly the commits unique to this branch
        let fork_point = detect_fork_point(repo_path, current_branch_name)
            .await
            .ok()
            .flatten();

        let range = match fork_point {
            Some(sha) => format!("{sha}..HEAD"),
            None => "HEAD".to_string(),
        };

        run_git_command(
            repo_path,
            &["log", &range, "--format=COMMIT:%H\t%s", "--numstat"],
        )
        .await
        .inspect_err(|e| log::warn!("Fallback unpushed-commits log failed: {e}"))
        .unwrap_or_default()
    };
    parse_commit_log(&output)
}

#[cfg(feature = "local_fs")]
fn parse_commit_log(output: &str) -> Result<Vec<Commit>> {
    let mut commits = Vec::new();
    let mut current: Option<Commit> = None;

    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("COMMIT:") {
            if let Some(commit) = current.take() {
                commits.push(commit);
            }
            let parts: Vec<&str> = rest.splitn(2, '\t').collect();
            if parts.len() == 2 {
                current = Some(Commit {
                    hash: parts[0].to_string(),
                    subject: parts[1].to_string(),
                    files_changed: 0,
                    additions: 0,
                    deletions: 0,
                });
            }
        } else if !line.is_empty() {
            // numstat line: "<additions>\t<deletions>\t<path>"
            if let Some(ref mut commit) = current {
                let parts: Vec<&str> = line.splitn(3, '\t').collect();
                if parts.len() == 3 {
                    commit.additions += parts[0].parse::<usize>().unwrap_or(0);
                    commit.deletions += parts[1].parse::<usize>().unwrap_or(0);
                    commit.files_changed += 1;
                }
            }
        }
    }

    if let Some(commit) = current {
        commits.push(commit);
    }

    Ok(commits)
}

#[cfg(not(feature = "local_fs"))]
pub async fn get_unpushed_commits(
    _repo_path: &Path,
    _current_branch_name: Option<&str>,
    _upstream_ref: Option<&str>,
) -> Result<Vec<Commit>> {
    Err(anyhow!("Not supported on wasm"))
}

/// Returns the list of files changed in a specific commit, with per-file stats.
#[cfg(feature = "local_fs")]
pub async fn get_commit_files(repo_path: &Path, hash: &str) -> Result<Vec<FileChangeEntry>> {
    let output = run_git_command(
        repo_path,
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

    let mut entries = Vec::new();
    for line in output.lines() {
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.splitn(3, '\t').collect();
        if parts.len() == 3 {
            entries.push(FileChangeEntry {
                path: parts[2].to_string(),
                additions: parts[0].parse().unwrap_or(0),
                deletions: parts[1].parse().unwrap_or(0),
            });
        }
    }

    Ok(entries)
}

#[cfg(not(feature = "local_fs"))]
pub async fn get_commit_files(_repo_path: &Path, _hash: &str) -> Result<Vec<FileChangeEntry>> {
    Err(anyhow!("Not supported on wasm"))
}

/// Maximum number of characters of diff content to send to AI for commit
/// message / PR title / PR description generation.
#[cfg(feature = "local_fs")]
const MAX_DIFF_CHARS_FOR_AI: usize = 16_000;

/// Per-file cap for untracked-file content we synthesise into the diff sent
/// to AI. Keeps any one new file from dominating the budget.
#[cfg(feature = "local_fs")]
const MAX_UNTRACKED_FILE_BYTES: usize = 4_000;

/// Number of leading bytes examined when classifying an untracked file as
/// binary, mirroring the heuristic in `count_lines_if_text_file`.
#[cfg(feature = "local_fs")]
const BINARY_CHECK_BYTES: usize = 1_024;

/// Maximum number of bytes in a PR title passed to `gh pr create`. GitHub's
/// hard limit is 256; we cap short of that to leave headroom for an
/// ellipsis marker. Measured in bytes because it's fed to
/// [`truncate_on_char_boundary`], which slices on byte offsets.
#[cfg(feature = "local_fs")]
const MAX_PR_TITLE_BYTES: usize = 200;

/// Returns a prefix of `s` whose length is at most `byte_cap` and which ends
/// on a UTF-8 char boundary. Plain `&s[..byte_cap]` panics when the cut
/// point lands inside a multi-byte code point, which is reachable in diffs
/// and source files containing non-ASCII text.
#[cfg(feature = "local_fs")]
fn truncate_on_char_boundary(s: &str, byte_cap: usize) -> &str {
    if s.len() <= byte_cap {
        return s;
    }
    let mut cut = byte_cap;
    while cut > 0 && !s.is_char_boundary(cut) {
        cut -= 1;
    }
    &s[..cut]
}

/// Returns the diff for commit message generation, truncated to avoid token
/// limits. When `include_unstaged` is true, diffs against HEAD (all
/// uncommitted changes) and also appends untracked files as synthetic diff
/// hunks so the LLM has full context even when the commit consists entirely
/// of new files. When `include_unstaged` is false, diffs only staged changes.
#[cfg(feature = "local_fs")]
pub async fn get_diff_for_commit_message(
    repo_path: &Path,
    include_unstaged: bool,
) -> Result<String> {
    let mut diff = if !include_unstaged {
        run_git_command(repo_path, &["diff", "--cached"]).await?
    } else if run_git_command(repo_path, &["rev-parse", "--verify", "HEAD"])
        .await
        .is_ok()
    {
        run_git_command(repo_path, &["diff", "HEAD"]).await?
    } else {
        // No HEAD before the first commit. Include staged changes plus
        // unstaged edits to staged files; untracked files are added below.
        let mut diff = run_git_command(repo_path, &["diff", "--cached"]).await?;
        diff.push_str(&run_git_command(repo_path, &["diff"]).await?);
        diff
    };

    // `git diff HEAD` only shows changes to already-tracked files. New files that
    // haven't been staged yet are invisible to it, so we synthesise diff hunks for
    // them here — mirroring the logic in `get_file_change_entries`.
    if include_unstaged {
        if let Ok(untracked) = run_git_command(
            repo_path,
            &["ls-files", "--others", "--exclude-standard", "-z"],
        )
        .await
        {
            // `-z` separates paths with NUL bytes and disables C-style
            // quoting, so paths containing spaces or non-ASCII characters
            // round-trip intact.
            // Cap the read to cover both the binary-check window and the
            // synthesised-hunk budget.
            let read_cap = BINARY_CHECK_BYTES.max(MAX_UNTRACKED_FILE_BYTES);
            for file_name_bytes in untracked.as_bytes().split(|b| *b == 0) {
                if file_name_bytes.is_empty() {
                    continue;
                }
                let Ok(file_name) = std::str::from_utf8(file_name_bytes) else {
                    continue;
                };
                let file_path = repo_path.join(file_name);
                // Async + bounded so a large untracked file doesn't block
                // the executor or balloon memory.
                let Ok(file) = tokio::fs::File::open(&file_path).await else {
                    continue;
                };
                let mut bytes = Vec::with_capacity(read_cap);
                use tokio::io::AsyncReadExt as _;
                if file
                    .take(read_cap as u64)
                    .read_to_end(&mut bytes)
                    .await
                    .is_err()
                {
                    continue;
                }
                let check_len = bytes.len().min(BINARY_CHECK_BYTES);
                if warp_util::file_type::is_buffer_binary(&bytes[..check_len]) {
                    continue;
                }
                let Ok(content) = std::str::from_utf8(&bytes) else {
                    continue;
                };
                let content = truncate_on_char_boundary(content, MAX_UNTRACKED_FILE_BYTES);
                let line_count = content.lines().count();
                diff.push_str(&format!(
                    "diff --git a/{file_name} b/{file_name}\nnew file mode 100644\n\
                     --- /dev/null\n+++ b/{file_name}\n@@ -0,0 +1,{line_count} @@\n"
                ));
                for line in content.lines() {
                    diff.push('+');
                    diff.push_str(line);
                    diff.push('\n');
                }
            }
        }
    }

    if diff.len() <= MAX_DIFF_CHARS_FOR_AI {
        Ok(diff)
    } else {
        Ok(format!(
            "{}\n... (diff truncated)",
            truncate_on_char_boundary(&diff, MAX_DIFF_CHARS_FOR_AI)
        ))
    }
}

#[cfg(not(feature = "local_fs"))]
pub async fn get_diff_for_commit_message(
    _repo_path: &Path,
    _include_unstaged: bool,
) -> Result<String> {
    Err(anyhow!("Not supported on wasm"))
}

/// Commits changes. If `include_unstaged` is true, stages all changes first via `git add -A`.
/// `path_env` is forwarded so commit hooks can find tools on the user's `PATH`.
#[cfg(feature = "local_fs")]
pub async fn run_commit(
    repo_path: &Path,
    message: &str,
    include_unstaged: bool,
    path_env: Option<&str>,
) -> Result<String> {
    if include_unstaged {
        run_git_command_with_env(repo_path, &["add", "-A"], path_env).await?;
    }
    run_git_command_with_env(repo_path, &["commit", "-m", message], path_env).await
}

#[cfg(not(feature = "local_fs"))]
pub async fn run_commit(
    _repo_path: &Path,
    _message: &str,
    _include_unstaged: bool,
    _path_env: Option<&str>,
) -> Result<String> {
    Err(anyhow!("Not supported on wasm"))
}

/// Per-file stats for what would land in a PR: default branch vs
/// `origin/<current>` (or HEAD when unpushed).
#[cfg(feature = "local_fs")]
pub async fn get_branch_diff_entries(repo_path: &Path) -> Result<Vec<FileChangeEntry>> {
    let base = detect_main_branch(repo_path).await?;
    let base = base.trim();
    let current = detect_current_branch(repo_path).await?;
    let remote_ref = format!("origin/{current}");

    // Use the remote ref if it exists, otherwise fall back to HEAD.
    let end_ref = if run_git_command(repo_path, &["rev-parse", "--verify", &remote_ref])
        .await
        .is_ok()
    {
        remote_ref
    } else {
        "HEAD".to_string()
    };

    let range = format!("{base}..{end_ref}");
    let output = run_git_command(repo_path, &["diff", "--numstat", &range]).await?;
    let mut entries = Vec::new();
    for line in output.lines() {
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 3 {
            entries.push(FileChangeEntry {
                path: parts[2].to_string(),
                additions: parts[0].parse().unwrap_or(0),
                deletions: parts[1].parse().unwrap_or(0),
            });
        }
    }
    Ok(entries)
}

#[cfg(not(feature = "local_fs"))]
pub async fn get_branch_diff_entries(_repo_path: &Path) -> Result<Vec<FileChangeEntry>> {
    Err(anyhow!("Not supported on wasm"))
}

/// Pushes the given branch to origin, setting upstream tracking if not already configured.
/// `path_env` is forwarded so the LFS `pre-push` hook can find `git-lfs`.
#[cfg(feature = "local_fs")]
pub async fn run_push(repo_path: &Path, branch: &str, path_env: Option<&str>) -> Result<String> {
    run_git_command_with_env(
        repo_path,
        &["push", "--set-upstream", "origin", branch],
        path_env,
    )
    .await
}

#[cfg(not(feature = "local_fs"))]
pub async fn run_push(_repo_path: &Path, _branch: &str, _path_env: Option<&str>) -> Result<String> {
    Err(anyhow!("Not supported on wasm"))
}

/// Fast-forward-only pull of `branch` from origin. Never merges: a
/// non-fast-forward (diverged history) fails with git's own error rather than
/// creating a merge commit or leaving conflict markers, so this never needs a
/// conflict-resolution UX (Stage 1 of git-pull parity; merging pull is a
/// separate, later item). `path_env` is forwarded so a post-merge hook (e.g.
/// LFS) can find `git-lfs` on the user's `PATH`, mirroring [`run_push`].
#[cfg(feature = "local_fs")]
pub async fn run_pull(repo_path: &Path, branch: &str, path_env: Option<&str>) -> Result<String> {
    run_git_command_with_env(
        repo_path,
        &["pull", "--ff-only", "origin", branch],
        path_env,
    )
    .await
}

#[cfg(not(feature = "local_fs"))]
pub async fn run_pull(_repo_path: &Path, _branch: &str, _path_env: Option<&str>) -> Result<String> {
    Err(anyhow!("Not supported on wasm"))
}

// Branch create/switch is deliberately NOT provided as an async primitive here.
// `run_create_branch` / `run_switch_branch` existed with zero callers and were
// removed 2026-08-18 (maintainer decision). They were fork-original -- the pin
// has neither -- and they were LOCAL-ONLY (`local_fs`, `&Path`), whereas the
// shipping path emits a shell command through the context chip
// (`PromptChipShellCommand::{GitCheckout, GitCreateAndCheckoutBranch}` ->
// `app/src/terminal/input.rs`), which therefore also works on remote/SSH
// sessions and shows the user the command and its output. Routing the chip
// through local primitives would have broken branch switching over SSH.
// Reinstate only alongside a dialog-based Git panel that has a remote story
// (Zap #329); git history holds the original bodies.

/// Applies `patch` to the index only, leaving the working tree untouched —
/// the primitive behind hunk-level staging. With `reverse`, un-stages instead.
///
/// The patch goes through a temp file because [`run_git_command_with_env`]
/// gives the child no stdin; `git apply` is happy to read from a path.
///
/// `--cached` is what makes this hunk-level: whole-file staging elsewhere in
/// this codebase uses `git restore --staged --worktree`, which cannot express
/// "part of this file". `--unidiff-zero` is deliberately NOT passed — it
/// disables git's context checking, which is the only thing that catches a
/// patch reconstructed against a stale diff.
#[cfg(feature = "local_fs")]
pub async fn run_apply_patch_cached(
    repo_path: &Path,
    patch: &str,
    reverse: bool,
) -> Result<String> {
    use std::io::Write as _;

    let mut file = tempfile::NamedTempFile::new()
        .map_err(|e| anyhow!("Failed to create temp file for patch: {e}"))?;
    file.write_all(patch.as_bytes())
        .map_err(|e| anyhow!("Failed to write patch to temp file: {e}"))?;
    file.flush()
        .map_err(|e| anyhow!("Failed to flush patch temp file: {e}"))?;

    let patch_path = file.path().to_string_lossy().to_string();
    let mut args: Vec<&str> = vec!["apply", "--cached"];
    if reverse {
        args.push("--reverse");
    }
    args.push(&patch_path);

    run_git_command(repo_path, &args).await
}

#[cfg(not(feature = "local_fs"))]
pub async fn run_apply_patch_cached(
    _repo_path: &Path,
    _patch: &str,
    _reverse: bool,
) -> Result<String> {
    Err(anyhow!("Not supported on wasm"))
}

/// Stages (or, with `unstage`, un-stages) whole files by repo-relative path —
/// the primitive behind per-file staging (Zap #329).
///
/// Paths are repo-relative and passed after `--` so a path that looks like an
/// option or a rev (`-x`, `HEAD`) is still treated as a path.
///
/// Staging is `git add`, which since git 2.0 also stages deletions, so a
/// deleted file needs no separate `git rm` branch. Un-staging is
/// `git restore --staged`, which reverts the index entry to HEAD *without*
/// touching the working tree — deliberately unlike the Discard Files path in
/// `code_review::diff_state`, which passes `--staged --worktree` and therefore
/// destroys the edit. The two differ by exactly that one flag, which is why
/// this lives here rather than being folded into the discard helpers.
///
/// Before the first commit there is no HEAD to restore an index entry from and
/// `git restore --staged` fails; `git rm --cached` is the equivalent there, and
/// it is only reached on that failure so a genuine error is still surfaced.
#[cfg(feature = "local_fs")]
pub async fn run_stage_paths(
    repo_path: &Path,
    relative_paths: &[String],
    unstage: bool,
) -> Result<String> {
    if relative_paths.is_empty() {
        return Err(anyhow!("No paths given to stage"));
    }

    let mut args: Vec<&str> = if unstage {
        vec!["restore", "--staged", "--"]
    } else {
        vec!["add", "--"]
    };
    for path in relative_paths {
        args.push(path.as_str());
    }

    match run_git_command(repo_path, &args).await {
        Ok(output) => Ok(output),
        Err(err) if unstage && err.to_string().contains("could not resolve HEAD") => {
            // Pre-first-commit repo: the index has no HEAD version to restore
            // from, so removing the entry outright is what "unstage" means.
            let mut rm_args: Vec<&str> = vec!["rm", "--cached", "--"];
            for path in relative_paths {
                rm_args.push(path.as_str());
            }
            run_git_command(repo_path, &rm_args).await
        }
        Err(err) => Err(err),
    }
}

#[cfg(not(feature = "local_fs"))]
pub async fn run_stage_paths(
    _repo_path: &Path,
    _relative_paths: &[String],
    _unstage: bool,
) -> Result<String> {
    Err(anyhow!("Not supported on wasm"))
}

/// What to run after the commit succeeds. Shared vocabulary for the commit
/// chain, mirroring Warp's `CommitChainMode` (Warp keeps it on
/// `code_review::diff_state`; the fork keeps it next to the git primitives it
/// composes). Maps to/from the wire enum `proto::GitCommitChainMode` at the
/// remote-server daemon boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitChainMode {
    CommitOnly,
    CommitAndPush,
    CommitAndCreatePr,
}

/// Runs the commit chain — always commits, then optionally pushes, then
/// optionally creates a PR per `mode` — and returns the post-chain delta
/// (refreshed unpushed commits + upstream ref) plus any created PR. The delta
/// is computed once after the whole chain settles.
///
/// Deterministic, backend-agnostic composition of the single-command
/// primitives above; the local code-review dialog and the remote-server daemon
/// both drive the same sequence, so local and remote behave identically.
///
/// Phosphor (BYOP) divergence from Warp: the create-PR stage always runs
/// `gh pr create --fill` — the fork drops Warp's cloud AIClient, so there is no
/// title/body autogeneration here (see #116). The `autogenerate_*` request
/// flags are accepted on the wire for protocol parity but currently fall back
/// to `--fill` because the daemon has no BYOP provider reachable.
pub async fn run_commit_chain(
    repo_path: &Path,
    mode: CommitChainMode,
    message: &str,
    include_unstaged: bool,
    branch: &str,
    path_env: Option<&str>,
) -> Result<(Vec<Commit>, Option<String>, Option<PrInfo>)> {
    run_commit(repo_path, message, include_unstaged, path_env).await?;
    let pr_info = match mode {
        CommitChainMode::CommitOnly => None,
        CommitChainMode::CommitAndPush => {
            run_push(repo_path, branch, path_env).await?;
            None
        }
        CommitChainMode::CommitAndCreatePr => {
            run_push(repo_path, branch, path_env).await?;
            Some(create_pr(repo_path, None, None, path_env).await?)
        }
    };
    let (commits, upstream_ref) = compute_unpushed_state(repo_path).await;
    Ok((commits, upstream_ref, pr_info))
}

/// Computes the branch's unpushed commits together with its upstream
/// tracking ref, so callers that need both (metadata refresh, the remote
/// git-operation delta returned to the client) don't repeat the work.
/// Returns `(Vec::new(), None)` on failure rather than erroring, since the
/// caller treats "no upstream" and "detection failed" the same way.
/// Ported verbatim from Warp (`warp/master:app/src/util/git.rs`).
#[cfg(feature = "local_fs")]
pub async fn compute_unpushed_state(repo_path: &Path) -> (Vec<Commit>, Option<String>) {
    let current_branch = detect_current_branch(repo_path).await.ok();
    let upstream_ref = run_git_command(
        repo_path,
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    )
    .await
    .ok()
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty());
    let unpushed = get_unpushed_commits(
        repo_path,
        current_branch.as_deref(),
        upstream_ref.as_deref(),
    )
    .await
    .unwrap_or_default();
    (unpushed, upstream_ref)
}

#[cfg(not(feature = "local_fs"))]
pub async fn compute_unpushed_state(_repo_path: &Path) -> (Vec<Commit>, Option<String>) {
    (Vec::new(), None)
}

/// Returns `true` if the repository is mid-operation (merge / cherry-pick /
/// revert / rebase) or another process holds the index lock, detected by
/// probing the sentinel files git writes under `.git/`. Code-review git
/// mutations are blocked in these states because they would behave
/// surprisingly (e.g. a commit would complete an in-progress merge) or fail.
/// Shared by the local pre-emptive guard (`is_git_operation_blocked`) and the
/// daemon-side execution-time check.
/// Ported verbatim from Warp (`warp/master:app/src/util/git.rs`).
#[cfg(feature = "local_fs")]
pub fn git_operation_in_progress(repo_path: &Path) -> bool {
    let git_dir = repo_path.join(".git");
    git_dir.join("MERGE_HEAD").exists()
        || git_dir.join("CHERRY_PICK_HEAD").exists()
        || git_dir.join("REVERT_HEAD").exists()
        || git_dir.join("rebase-merge").exists()
        || git_dir.join("rebase-apply").exists()
        || git_dir.join("index.lock").exists()
}

// ── gh CLI helpers ───────────────────────────────────────────────────────────

/// PR information returned by `gh pr view`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrInfo {
    pub number: u64,
    pub url: String,
    pub state: String,
    pub draft: bool,
    pub base_branch: String,
}

/// Repository information returned by `gh repo view`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryInfo {
    pub name: String,
    pub owner: Option<String>,
    /// The repository host (e.g. "github.com"), parsed from the repo URL.
    pub host: Option<String>,
}

#[cfg(feature = "local_fs")]
fn repository_info_from_gh_output(output: &str) -> Result<RepositoryInfo> {
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
    let host = parsed["url"]
        .as_str()
        .and_then(|u| url::Url::parse(u).ok())
        .and_then(|u| u.host_str().map(|host| host.to_string()));
    Ok(RepositoryInfo {
        name,
        owner: Some(owner),
        host,
    })
}

/// Looks up the GitHub repository for `repo_path` via `gh repo view`.
/// Returns `Ok(None)` when the path is not a work tree, or when `gh`
/// authoritatively reports that there is no GitHub repository to resolve.
#[cfg(feature = "local_fs")]
pub async fn get_repository_info(
    repo_path: &Path,
    path_env: Option<&str>,
) -> Result<Option<RepositoryInfo>> {
    if run_git_command(repo_path, &["rev-parse", "--is-inside-work-tree"])
        .await
        .is_err()
    {
        return Ok(None);
    }

    match run_gh_command(
        repo_path,
        &["repo", "view", "--json", "name,owner,url"],
        path_env,
    )
    .await
    {
        Ok(stdout) => repository_info_from_gh_output(&stdout).map(Some),
        Err(e) => {
            let msg = e.to_string();
            if is_repository_lookup_not_applicable_error(&msg) {
                Ok(None)
            } else {
                Err(e)
            }
        }
    }
}

#[cfg(not(feature = "local_fs"))]
pub async fn get_repository_info(
    _repo_path: &Path,
    _path_env: Option<&str>,
) -> Result<Option<RepositoryInfo>> {
    Err(anyhow!("Not supported without local_fs"))
}

/// Runs a `gh` CLI command and returns stdout on success. `path_env`, when
/// `Some`, is set as the child's `PATH` so a Homebrew-installed `gh` is
/// findable from macOS GUI launches (launchd's minimal `PATH` excludes it).
#[cfg(feature = "local_fs")]
async fn run_gh_command(repo_path: &Path, args: &[&str], path_env: Option<&str>) -> Result<String> {
    use command::r#async::Command;
    use command::Stdio;

    log::debug!(
        "[GIT OPERATION] git.rs run_gh_command gh {}",
        args.join(" ")
    );

    let mut cmd = Command::new("gh");
    cmd.args(args)
        .current_dir(repo_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .kill_on_drop(true);
    if let Some(path_env) = path_env {
        cmd.env("PATH", path_env);
    }
    let output = cmd
        .output()
        .await
        .map_err(|e| anyhow!("Failed to execute gh command: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok(stdout)
    } else {
        Err(anyhow!("gh command failed: {stderr}"))
    }
}

/// Looks up the PR for the current branch via `gh pr view`.
/// Returns `Ok(None)` when the repo context is not eligible for a PR lookup or
/// there is simply no PR for this branch. Returns `Err` for real failures
/// (auth, network, gh not installed).
#[cfg(feature = "local_fs")]
pub async fn get_pr_for_branch(repo_path: &Path, path_env: Option<&str>) -> Result<Option<PrInfo>> {
    if run_git_command(repo_path, &["rev-parse", "--is-inside-work-tree"])
        .await
        .is_err()
    {
        return Ok(None);
    }

    // A detached HEAD has no branch to look a PR up for, so skip `gh` entirely.
    if run_git_command(repo_path, &["symbolic-ref", "--quiet", "--short", "HEAD"])
        .await
        .is_err()
    {
        return Ok(None);
    }

    match run_gh_command(
        repo_path,
        &[
            "pr",
            "view",
            "--json",
            "number,url,state,isDraft,baseRefName",
        ],
        path_env,
    )
    .await
    {
        Ok(stdout) => {
            let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
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
            Ok(Some(PrInfo {
                number,
                url,
                state,
                draft,
                base_branch,
            }))
        }
        Err(e) => {
            let msg = e.to_string();
            if is_pr_lookup_not_applicable_error(&msg) {
                Ok(None)
            } else {
                Err(e)
            }
        }
    }
}

#[cfg(not(feature = "local_fs"))]
pub async fn get_pr_for_branch(
    _repo_path: &Path,
    _path_env: Option<&str>,
) -> Result<Option<PrInfo>> {
    Err(anyhow!("Not supported on wasm"))
}

/// Classifies `gh pr view` failures that mean the current branch simply has no
/// pull request, rather than a real lookup failure.
#[cfg(feature = "local_fs")]
fn is_no_pr_for_branch_error(error_msg: &str) -> bool {
    let lower = error_msg.to_lowercase();
    lower.contains("no pull requests found for branch")
        || lower.contains("no open pull requests found for branch")
}

/// Classifies `gh pr view` failures that authoritatively mean a PR lookup does
/// not apply to this repository, rather than a transient fetch failure.
#[cfg(feature = "local_fs")]
fn is_pr_lookup_not_applicable_error(error_msg: &str) -> bool {
    let lower = error_msg.to_lowercase();
    is_no_pr_for_branch_error(error_msg)
        || lower.contains(
            "none of the git remotes configured for this repository point to a known github host",
        )
        || lower.contains("no github remotes")
        || lower.contains("not a github repository")
        || lower.contains("could not determine base repo")
}

/// Classifies `gh repo view` failures that authoritatively mean the current
/// repository has no GitHub repository info, rather than a transient fetch
/// failure.
#[cfg(feature = "local_fs")]
fn is_repository_lookup_not_applicable_error(error_msg: &str) -> bool {
    let lower = error_msg.to_lowercase();
    lower.contains(
        "none of the git remotes configured for this repository point to a known github host",
    ) || lower.contains("no github remotes")
        || lower.contains("not a github repository")
        || lower.contains("could not determine base repo")
}

/// Heuristic check for `gh` CLI authentication errors in an error message.
pub fn is_gh_auth_error(error_msg: &str) -> bool {
    let lower = error_msg.to_lowercase();
    lower.contains("not logged in")
        || lower.contains("authentication required")
        || lower.contains("gh auth login")
}

/// Heuristic check for errors caused by `gh` not being executable from `PATH`.
pub fn is_gh_missing_error(error_msg: &str) -> bool {
    let lower = error_msg.to_lowercase();
    lower.contains("failed to execute gh command")
        && (lower.contains("no such file or directory")
            || lower.contains("not found")
            || lower.contains("cannot find")
            || lower.contains("could not find"))
}

/// PR-ready diff (default branch vs `origin/<current>` or HEAD),
/// truncated for AI token limits.
#[cfg(feature = "local_fs")]
pub async fn get_diff_for_pr(repo_path: &Path) -> Result<String> {
    let base = detect_main_branch(repo_path).await?;
    let base = base.trim();
    let current = detect_current_branch(repo_path).await?;
    let remote_ref = format!("origin/{current}");

    let end_ref = if run_git_command(repo_path, &["rev-parse", "--verify", &remote_ref])
        .await
        .is_ok()
    {
        remote_ref
    } else {
        "HEAD".to_string()
    };

    let range = format!("{base}..{end_ref}");
    let mut diff = run_git_command(repo_path, &["diff", &range]).await?;
    if diff.len() > MAX_DIFF_CHARS_FOR_AI {
        diff = format!(
            "{}\n... (diff truncated)",
            truncate_on_char_boundary(&diff, MAX_DIFF_CHARS_FOR_AI)
        );
    }
    Ok(diff)
}

#[cfg(not(feature = "local_fs"))]
pub async fn get_diff_for_pr(_repo_path: &Path) -> Result<String> {
    Err(anyhow!("Not supported on wasm"))
}

/// Commit subject lines on the current branch since the default branch.
#[cfg(feature = "local_fs")]
pub async fn get_branch_commit_messages(repo_path: &Path) -> Result<Vec<String>> {
    let base = detect_main_branch(repo_path).await?;
    let base = base.trim();
    let range = format!("{base}..HEAD");
    let output = run_git_command(repo_path, &["log", &range, "--format=%s"]).await?;
    Ok(output
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect())
}

#[cfg(not(feature = "local_fs"))]
pub async fn get_branch_commit_messages(_repo_path: &Path) -> Result<Vec<String>> {
    Err(anyhow!("Not supported on wasm"))
}

/// Creates a PR for the current branch (must already be pushed). Falls back
/// to `--fill` when title/body are `None`. Always targets the detected
/// default branch.
#[cfg(feature = "local_fs")]
pub async fn create_pr(
    repo_path: &Path,
    title: Option<&str>,
    body: Option<&str>,
    path_env: Option<&str>,
) -> Result<PrInfo> {
    let base = detect_main_branch(repo_path).await?;
    let base = base.trim();
    let base = base.strip_prefix("origin/").unwrap_or(base);
    let sanitized_title;
    let args: Vec<&str> = match (title, body) {
        (Some(t), Some(b)) => {
            sanitized_title = sanitize_pr_title(t);
            vec![
                "pr",
                "create",
                "--base",
                base,
                "--title",
                &sanitized_title,
                "--body",
                b,
            ]
        }
        _ => vec!["pr", "create", "--base", base, "--fill"],
    };
    let stdout = run_gh_command(repo_path, &args, path_env).await?;
    // `gh pr create` prints the PR URL on success.
    let url = stdout.trim().to_string();
    // Extract PR number from the URL (e.g. https://github.com/owner/repo/pull/123)
    let number = url
        .rsplit('/')
        .next()
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or_else(|| anyhow!("Could not parse PR number from URL: {url}"))?;
    Ok(PrInfo {
        number,
        url,
        state: "OPEN".to_string(),
        draft: false,
        base_branch: base.to_string(),
    })
}

/// Trims an AI-generated PR title to a single line and caps its length.
#[cfg(feature = "local_fs")]
fn sanitize_pr_title(raw: &str) -> String {
    let first_line = raw.lines().next().unwrap_or("").trim();
    truncate_on_char_boundary(first_line, MAX_PR_TITLE_BYTES).to_string()
}

#[cfg(not(feature = "local_fs"))]
pub async fn create_pr(
    _repo_path: &Path,
    _title: Option<&str>,
    _body: Option<&str>,
    _path_env: Option<&str>,
) -> Result<PrInfo> {
    Err(anyhow!("Not supported on wasm"))
}

/// Counts newlines in a file, returning 0 for binary or oversized files.
#[cfg(feature = "local_fs")]
fn count_lines_if_text_file(path: &Path) -> u32 {
    const MAX_FILE_SIZE: u64 = 20_000_000;
    const BINARY_CHECK_SIZE: usize = 1024;

    let Ok(metadata) = std::fs::metadata(path) else {
        return 0;
    };
    if metadata.len() > MAX_FILE_SIZE || !metadata.is_file() {
        return 0;
    }
    let Ok(content) = std::fs::read(path) else {
        return 0;
    };
    let check_len = content.len().min(BINARY_CHECK_SIZE);
    if warp_util::file_type::is_buffer_binary(&content[..check_len]) {
        return 0;
    }
    bytecount::count(&content, b'\n') as u32
}
