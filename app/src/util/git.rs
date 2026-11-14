use std::collections::HashSet;
use std::path::Path;

use anyhow::{Result, anyhow};

// The pure git logic (argv builders, parsers, the algorithms over a `GitRunner`) lives in
// `remote_server::shellcmd::git` so the helper and the shell backend run the same commands.
// This module keeps the app's local wrappers around it (P5).
#[cfg(feature = "local_fs")]
use remote_server::shellcmd::git::{self as shared, GitOutput, GitRunner, git_text};
pub use remote_server::shellcmd::git::{
    Commit, CommitChainMode, FileChangeEntry, PrInfo, RepositoryInfo, is_gh_auth_error,
    is_gh_missing_error, looks_like_commit_sha,
};
// Referenced by this module's tests only.
#[cfg(all(test, feature = "local_fs"))]
use remote_server::shellcmd::git::{
    is_no_pr_for_branch_error, is_repository_lookup_not_applicable_error,
};

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
    git_text(&LocalGitRunner::new(path_env), repo_path, args).await
}

/// The app's local [`GitRunner`]: spawns `git` on this machine (through `wsl.exe` for a WSL UNC
/// working directory), with the shared base arguments and environment. Exit-status policy and
/// error text come from `remote_server::shellcmd::git::git_text`, identical to what
/// `run_git_command_with_env` always produced.
#[cfg(feature = "local_fs")]
#[derive(Clone, Debug, Default)]
pub struct LocalGitRunner {
    path_env: Option<String>,
}

#[cfg(feature = "local_fs")]
impl LocalGitRunner {
    /// `path_env` sets `PATH` on the child (see [`run_git_command_with_env`]).
    pub fn new(path_env: Option<&str>) -> Self {
        Self {
            path_env: path_env.map(str::to_owned),
        }
    }
}

#[cfg(feature = "local_fs")]
impl GitRunner for LocalGitRunner {
    fn run<'a>(
        &'a self,
        args: &'a [&'a str],
        stdin: Option<&'a [u8]>,
        cwd: &'a Path,
    ) -> futures::future::BoxFuture<'a, Result<GitOutput>> {
        Box::pin(async move {
            use command::Stdio;
            use futures_lite::io::AsyncWriteExt as _;

            log::debug!(
                "[GIT OPERATION] git.rs run_git_command git {}",
                args.join(" ")
            );
            let mut git_args: Vec<&str> = shared::GIT_BASE_ARGS.to_vec();
            git_args.extend_from_slice(args);
            let env = git_child_env(self.path_env.as_deref());

            let mut cmd = git_command(cwd, &git_args, &env);
            cmd.stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true);
            let output = match stdin {
                None => cmd.output().await,
                Some(bytes) => {
                    cmd.stdin(Stdio::piped());
                    match cmd.spawn() {
                        Ok(mut child) => {
                            // `close` (not a bare drop): on Windows async-process feeds the
                            // child through a buffering thread, and a drop without close can
                            // lose bytes not yet handed over; git then reads an empty commit
                            // message or a truncated patch (#2226).
                            let fed = match child.stdin.take() {
                                Some(mut pipe) => match pipe.write_all(bytes).await {
                                    Ok(()) => pipe.close().await,
                                    Err(e) => Err(e),
                                },
                                None => Ok(()),
                            };
                            let output = child.output().await;
                            match (fed, output) {
                                (Err(e), Ok(out))
                                    if e.kind() != std::io::ErrorKind::BrokenPipe
                                        || out.status.success() =>
                                {
                                    Err(e)
                                }
                                (_, output) => output,
                            }
                        }
                        Err(e) => Err(e),
                    }
                }
            }
            .map_err(|e| anyhow!("Failed to execute git command: {}", e))?;

            Ok(GitOutput {
                stdout: output.stdout,
                stderr: output.stderr,
                exit_code: output.status.code(),
            })
        })
    }

    fn run_gh<'a>(
        &'a self,
        args: &'a [&'a str],
        cwd: &'a Path,
    ) -> futures::future::BoxFuture<'a, Result<GitOutput>> {
        Box::pin(async move {
            use command::Stdio;
            use command::r#async::Command;

            log::debug!(
                "[GIT OPERATION] git.rs run_gh_command gh {}",
                args.join(" ")
            );

            let mut cmd = Command::new("gh");
            cmd.args(args)
                .current_dir(cwd)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .env("HOMEBREW_NO_AUTO_UPDATE", "1")
                .env("GH_PROMPT_DISABLED", "1")
                .kill_on_drop(true);
            if let Some(path_env) = &self.path_env {
                cmd.env("PATH", path_env);
            }
            let output = cmd
                .output()
                .await
                .map_err(|e| anyhow!("Failed to execute gh command: {e}"))?;
            Ok(GitOutput {
                stdout: output.stdout,
                stderr: output.stderr,
                exit_code: output.status.code(),
            })
        })
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
        Ok(out) if out.status.success() => parse_branch_list(&String::from_utf8_lossy(&out.stdout)),
        _ => HashSet::new(),
    }
}

/// Parses `git branch --list --format=%(refname:short)` output.
pub(crate) fn parse_branch_list(stdout: &str) -> HashSet<String> {
    stdout
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect()
}

/// Non-blocking variant of [`list_local_branches_sync`] (#1313). Empty set on any failure.
#[cfg(feature = "local_fs")]
pub async fn list_local_branches(repo_path: &Path) -> HashSet<String> {
    match run_git_command(
        repo_path,
        &["branch", "--list", "--format=%(refname:short)"],
    )
    .await
    {
        Ok(out) => parse_branch_list(&out),
        Err(_) => HashSet::new(),
    }
}

#[cfg(not(feature = "local_fs"))]
pub async fn list_local_branches(_repo_path: &Path) -> HashSet<String> {
    HashSet::new()
}

/// Per-repo cache of local branch names, filled off the UI thread. UI-thread code reads it with
/// [`cached_local_branches`] and never runs git; a refresh result is only stored if no newer
/// refresh was started for the same repo (generation check).
#[derive(Default)]
struct BranchCache {
    branches: std::collections::HashMap<std::path::PathBuf, HashSet<String>>,
    /// Names handed out for new worktree branches whose `git worktree add -b` may not have run
    /// yet, so a stale or cold list cannot offer the same name twice. Dropped once a listing
    /// contains them.
    reserved: std::collections::HashMap<std::path::PathBuf, HashSet<String>>,
    generation: std::collections::HashMap<std::path::PathBuf, u64>,
}

static BRANCH_CACHE: std::sync::LazyLock<std::sync::Mutex<BranchCache>> =
    std::sync::LazyLock::new(Default::default);

pub(crate) fn cached_local_branches(repo_path: &Path) -> Option<HashSet<String>> {
    let cache = BRANCH_CACHE.lock().ok()?;
    let listed = cache.branches.get(repo_path);
    let reserved = cache.reserved.get(repo_path);
    match (listed, reserved) {
        (None, None) => None,
        (l, r) => Some(
            l.into_iter()
                .chain(r)
                .flat_map(|set| set.iter().cloned())
                .collect(),
        ),
    }
}

/// Remembers `name` as taken for `repo_path` until a branch listing contains it.
pub(crate) fn reserve_branch_name(repo_path: &Path, name: &str) {
    const MAX_RESERVED: usize = 64;
    let Ok(mut cache) = BRANCH_CACHE.lock() else {
        return;
    };
    let set = cache.reserved.entry(repo_path.to_path_buf()).or_default();
    if set.len() >= MAX_RESERVED {
        set.clear();
    }
    set.insert(name.to_owned());
}

/// Bumped whenever a repository watcher reports that `HEAD` or a local branch ref changed.
/// Branch-list caches record it before listing and are invalid once it moves, so they follow the
/// event instead of trusting a file mtime (coarse mtimes and same-mtime swaps go stale).
static REFS_EPOCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub(crate) fn note_refs_changed() {
    REFS_EPOCH.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
}

pub(crate) fn refs_epoch() -> u64 {
    REFS_EPOCH.load(std::sync::atomic::Ordering::SeqCst)
}

/// Starts a refresh for `repo_path`, returning its generation.
pub(crate) fn begin_branch_refresh(repo_path: &Path) -> u64 {
    let Ok(mut cache) = BRANCH_CACHE.lock() else {
        return 0;
    };
    let slot = cache.generation.entry(repo_path.to_path_buf()).or_insert(0);
    *slot += 1;
    *slot
}

/// Stores `branches` unless a newer refresh was started. Returns whether it was stored.
pub(crate) fn finish_branch_refresh(
    repo_path: &Path,
    generation: u64,
    branches: HashSet<String>,
) -> bool {
    let Ok(mut cache) = BRANCH_CACHE.lock() else {
        return false;
    };
    if cache.generation.get(repo_path) != Some(&generation) {
        return false;
    }
    if let Some(reserved) = cache.reserved.get_mut(repo_path) {
        reserved.retain(|name| !branches.contains(name));
    }
    evict_branch_cache_if_full(&mut cache, repo_path);
    cache.branches.insert(repo_path.to_path_buf(), branches);
    true
}

/// Most repos whose branch list is kept; one repo is evicted to make room for a new one.
const MAX_CACHED_REPOS: usize = 64;

/// Keeps the per-repo maps bounded: repos ever opened in this process would otherwise
/// accumulate forever (#1382). An evicted repo just re-lists on its next refresh.
fn evict_branch_cache_if_full(cache: &mut BranchCache, keep: &Path) {
    if cache.branches.contains_key(keep) || cache.branches.len() < MAX_CACHED_REPOS {
        return;
    }
    if let Some(victim) = cache.branches.keys().find(|k| k.as_path() != keep).cloned() {
        cache.branches.remove(&victim);
        cache.generation.remove(&victim);
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
    shared::detect_current_branch(&LocalGitRunner::default(), repo_path).await
}

/// Like [`detect_current_branch`], but in detached HEAD state returns the short
/// commit SHA instead of the literal "HEAD".
/// (Matches the shell command `git symbolic-ref --short HEAD || git rev-parse --short HEAD`.)
#[cfg(feature = "local_fs")]
pub async fn detect_current_branch_display(repo_path: &Path) -> Result<String> {
    shared::detect_current_branch_display(&LocalGitRunner::default(), repo_path).await
}

/// Whether a shell command probably moves HEAD (switch/checkout/worktree/rebase/
/// merge/pull/reset/stash/cherry-pick/bisect), so a branch read right after it must
/// not trust a cached value. Over-approximate by design. The pure predicate lives in
/// `remote_server::shellcmd::predicates` so the remote refresh shares it.
pub fn command_may_change_head(command: &str) -> bool {
    remote_server::shellcmd::predicates::command_may_change_head(command)
}

/// Whether a shell command probably changes the shell's working directory (`cd`,
/// `pushd`, `popd`). Over-approximate by design; see [`command_may_change_head`].
pub fn command_may_change_cwd(command: &str) -> bool {
    remote_server::shellcmd::predicates::command_may_change_cwd(command)
}

/// Reads the current branch for the repo containing `dir` straight from `.git/HEAD`,
/// without spawning git. Walks up from `dir`; resolves a `.git` *file* (linked
/// worktrees and submodules: `gitdir: <path>`). Returns the branch name, or the
/// short (7 char) SHA when HEAD is detached. Local filesystems only: callers must
/// not use this for remote/SSH sessions.
pub fn read_head_branch_sync(dir: &Path) -> Option<String> {
    for ancestor in dir.ancestors() {
        let dot_git = ancestor.join(".git");
        let git_dir = if dot_git.is_dir() {
            dot_git
        } else if dot_git.is_file() {
            let content = std::fs::read_to_string(&dot_git).ok()?;
            let target = content.trim().strip_prefix("gitdir:")?.trim();
            let target = Path::new(target);
            if target.is_absolute() {
                target.to_path_buf()
            } else {
                ancestor.join(target)
            }
        } else {
            continue;
        };
        let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
        return parse_head_file(&head);
    }
    None
}

/// Parses the contents of a `HEAD` file into a branch name or short SHA.
fn parse_head_file(head: &str) -> Option<String> {
    let head = head.trim();
    if let Some(reference) = head.strip_prefix("ref:") {
        let reference = reference.trim();
        let name = reference.strip_prefix("refs/heads/").unwrap_or(reference);
        return (!name.is_empty()).then(|| name.to_owned());
    }
    looks_like_commit_sha(head).then(|| head.chars().take(7).collect())
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
    shared::detect_main_branch(&LocalGitRunner::default(), repo_path).await
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
    shared::detect_fork_point(&LocalGitRunner::default(), repo_path, current_branch_name).await
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

/// Returns per-file change entries. When `include_unstaged` is true, returns all
/// uncommitted changes (staged + unstaged + untracked) vs HEAD; otherwise only staged changes.
#[cfg(feature = "local_fs")]
pub async fn get_file_change_entries(
    repo_path: &Path,
    include_unstaged: bool,
) -> Result<Vec<FileChangeEntry>> {
    let (mut entries, untracked) = shared::file_change_entries_tracked(
        &LocalGitRunner::default(),
        repo_path,
        include_unstaged,
    )
    .await;

    // Also include untracked files when showing all changes (their lines are counted from the
    // local file; a remote host counts them its own way).
    for file_name in untracked {
        let additions = count_lines_if_text_file(&repo_path.join(&file_name)) as usize;
        entries.push(FileChangeEntry {
            path: file_name,
            additions,
            deletions: 0,
        });
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
    shared::get_unpushed_commits(
        &LocalGitRunner::default(),
        repo_path,
        current_branch_name,
        upstream_ref,
    )
    .await
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
    shared::get_commit_files(&LocalGitRunner::default(), repo_path, hash).await
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
                let content = shared::truncate_on_char_boundary(content, MAX_UNTRACKED_FILE_BYTES);
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
            shared::truncate_on_char_boundary(&diff, MAX_DIFF_CHARS_FOR_AI)
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
    shared::run_commit(
        &LocalGitRunner::new(path_env),
        repo_path,
        message,
        include_unstaged,
    )
    .await
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
    shared::get_branch_diff_entries(&LocalGitRunner::default(), repo_path).await
}

#[cfg(not(feature = "local_fs"))]
pub async fn get_branch_diff_entries(_repo_path: &Path) -> Result<Vec<FileChangeEntry>> {
    Err(anyhow!("Not supported on wasm"))
}

/// Pushes the given branch to origin, setting upstream tracking if not already configured.
/// `path_env` is forwarded so the LFS `pre-push` hook can find `git-lfs`.
#[cfg(feature = "local_fs")]
pub async fn run_push(repo_path: &Path, branch: &str, path_env: Option<&str>) -> Result<String> {
    shared::run_push(&LocalGitRunner::new(path_env), repo_path, branch).await
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
    shared::run_pull(&LocalGitRunner::new(path_env), repo_path, branch).await
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
    let args = shared::apply_cached_args(reverse, false, &patch_path);

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
///
/// Git reports the missing HEAD as `fatal: could not resolve 'HEAD'` — the ref
/// name is quoted, and the quoting style has changed between git releases, so
/// the fallback matches the two stable halves of the message instead of one
/// fixed string.

#[cfg(feature = "local_fs")]
pub async fn run_stage_paths(
    repo_path: &Path,
    relative_paths: &[String],
    unstage: bool,
) -> Result<String> {
    shared::stage_paths(
        &LocalGitRunner::default(),
        repo_path,
        relative_paths,
        unstage,
    )
    .await
}

#[cfg(not(feature = "local_fs"))]
pub async fn run_stage_paths(
    _repo_path: &Path,
    _relative_paths: &[String],
    _unstage: bool,
) -> Result<String> {
    Err(anyhow!("Not supported on wasm"))
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
    shared_run_commit_chain(repo_path, mode, message, include_unstaged, branch, path_env).await
}

async fn shared_run_commit_chain(
    repo_path: &Path,
    mode: CommitChainMode,
    message: &str,
    include_unstaged: bool,
    branch: &str,
    path_env: Option<&str>,
) -> Result<(Vec<Commit>, Option<String>, Option<PrInfo>)> {
    #[cfg(feature = "local_fs")]
    {
        shared::run_commit_chain(
            &LocalGitRunner::new(path_env),
            repo_path,
            mode,
            message,
            include_unstaged,
            branch,
        )
        .await
    }
    #[cfg(not(feature = "local_fs"))]
    {
        let _ = (repo_path, mode, message, include_unstaged, branch, path_env);
        Err(anyhow!("Not supported on wasm"))
    }
}

/// Computes the branch's unpushed commits together with its upstream
/// tracking ref, so callers that need both (metadata refresh, the remote
/// git-operation delta returned to the client) don't repeat the work.
/// Returns `(Vec::new(), None)` on failure rather than erroring, since the
/// caller treats "no upstream" and "detection failed" the same way.
/// Ported verbatim from Warp (`warp/master:app/src/util/git.rs`).
#[cfg(feature = "local_fs")]
pub async fn compute_unpushed_state(repo_path: &Path) -> (Vec<Commit>, Option<String>) {
    shared::compute_unpushed_state(&LocalGitRunner::default(), repo_path).await
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
    // The sentinel list and the `<repo>/.git` anchor are shared with the helper and the shell
    // backend (`shared::guard_git_operation_in_progress`); only the existence test is local.
    let git_dir = repo_path.join(".git");
    shared::OPERATION_SENTINELS
        .iter()
        .any(|sentinel| git_dir.join(sentinel).exists())
}

// ── gh CLI helpers ───────────────────────────────────────────────────────────

#[cfg(feature = "local_fs")]
fn repository_info_from_gh_output(output: &str) -> Result<RepositoryInfo> {
    // The reading lives in `remote_server::shellcmd::git::gh`, shared with the helper and the
    // shell backend.
    shared::repository_info_from_gh_output(output)
}

/// Looks up the GitHub repository for `repo_path` via `gh repo view`.
/// Returns `Ok(None)` when the path is not a work tree, or when `gh`
/// authoritatively reports that there is no GitHub repository to resolve.
#[cfg(feature = "local_fs")]
pub async fn get_repository_info(
    repo_path: &Path,
    path_env: Option<&str>,
) -> Result<Option<RepositoryInfo>> {
    shared::get_repository_info(&LocalGitRunner::new(path_env), repo_path).await
}

#[cfg(not(feature = "local_fs"))]
pub async fn get_repository_info(
    _repo_path: &Path,
    _path_env: Option<&str>,
) -> Result<Option<RepositoryInfo>> {
    Err(anyhow!("Not supported without local_fs"))
}

/// Looks up the PR for the current branch via `gh pr view`.
/// Returns `Ok(None)` when the repo context is not eligible for a PR lookup or
/// there is simply no PR for this branch. Returns `Err` for real failures
/// (auth, network, gh not installed). `path_env`, when `Some`, is set as the
/// child's `PATH` so a Homebrew-installed `gh` is findable from macOS GUI
/// launches (launchd's minimal `PATH` excludes it).
#[cfg(feature = "local_fs")]
pub async fn get_pr_for_branch(repo_path: &Path, path_env: Option<&str>) -> Result<Option<PrInfo>> {
    shared::get_pr_for_branch(&LocalGitRunner::new(path_env), repo_path).await
}

#[cfg(not(feature = "local_fs"))]
pub async fn get_pr_for_branch(
    _repo_path: &Path,
    _path_env: Option<&str>,
) -> Result<Option<PrInfo>> {
    Err(anyhow!("Not supported on wasm"))
}

/// PR-ready diff (default branch vs `origin/<current>` or HEAD),
/// truncated for AI token limits.
#[cfg(feature = "local_fs")]
pub async fn get_diff_for_pr(repo_path: &Path) -> Result<String> {
    shared::get_diff_for_pr(&LocalGitRunner::default(), repo_path).await
}

#[cfg(not(feature = "local_fs"))]
pub async fn get_diff_for_pr(_repo_path: &Path) -> Result<String> {
    Err(anyhow!("Not supported on wasm"))
}

/// Commit subject lines on the current branch since the default branch.
#[cfg(feature = "local_fs")]
pub async fn get_branch_commit_messages(repo_path: &Path) -> Result<Vec<String>> {
    shared::get_branch_commit_messages(&LocalGitRunner::default(), repo_path).await
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
    shared::create_pr(&LocalGitRunner::new(path_env), repo_path, title, body).await
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
