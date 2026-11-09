//! Pure parsers and argv builders for git output. No I/O.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Result, anyhow};

use super::types::*;

/// `git status` arguments for the full status read (code review): untracked files expanded,
/// branch headers included, NUL separated.
pub const STATUS_FULL_ARGS: [&str; 6] = [
    "--no-optional-locks",
    "status",
    "--untracked-files=all",
    "--branch",
    "--porcelain=2",
    "-z",
];

/// `git status` arguments for the chip's status read: untracked directories collapsed.
pub const STATUS_CHIP_ARGS: [&str; 5] = [
    "--no-optional-locks",
    "status",
    "--untracked-files=normal",
    "--porcelain=2",
    "-z",
];

/// `git diff --numstat -z HEAD` (the chip's tracked line counts).
pub const NUMSTAT_Z_HEAD_ARGS: [&str; 5] =
    ["--no-optional-locks", "diff", "--numstat", "-z", "HEAD"];

/// Parses `git status --porcelain=2 -z` output into per-path entries.
///
/// Also keeps the index (`X`) column; see [`FileStagedState`] (Zap #329). Header lines
/// (`# branch.*`) are skipped here; read them with [`parse_branch_headers`].
pub fn parse_status_entries(status_output: &str) -> Result<Vec<GitStatusEntry>> {
    if status_output.is_empty() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    let tokens: Vec<&str> = status_output.split('\0').collect();

    let mut i = 0;
    while i < tokens.len() {
        let token = tokens[i];

        if token.is_empty() {
            i += 1;
            continue;
        }

        // Skip header lines that start with '#'
        if token.starts_with("# ") {
            i += 1;
            continue;
        }

        let entry_kind = token.chars().next().unwrap_or('?');

        match entry_kind {
            '1' => {
                // Changed entry: 1 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <path>
                // Use splitn to preserve spaces in filenames (the path is the last field).
                let parts: Vec<&str> = token.splitn(9, ' ').collect();
                if parts.len() >= 9 {
                    let status_code = parts[1];
                    let path = parts[8];

                    let status = GitFileStatus::try_from(status_code).map_err(|e| {
                        anyhow!(
                            "Invalid status code '{}' for path '{}': {}",
                            status_code,
                            path,
                            e
                        )
                    })?;
                    files.push(GitStatusEntry {
                        path: PathBuf::from(path),
                        status,
                        staged: FileStagedState::from_status_code(status_code),
                    });
                } else {
                    log::warn!(
                        "Invalid format for changed entry: '{token}' - expected at least 9 parts, got {}",
                        parts.len()
                    );
                }
            }
            '2' => {
                // Renamed/copied entry: 2 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <X><score> <path><sep><origPath>
                // Use splitn to preserve spaces in filenames (the path is the last field).
                let parts: Vec<&str> = token.splitn(10, ' ').collect();
                if parts.len() >= 10 {
                    let status_code = parts[1];
                    let path = parts[9];

                    // Get the old path from the next token
                    let old_path = if i + 1 < tokens.len() {
                        tokens[i + 1].to_string()
                    } else {
                        log::warn!("Missing old path for renamed/copied entry: '{token}'");
                        String::new()
                    };

                    let status = if status_code.starts_with('R') {
                        GitFileStatus::Renamed { old_path }
                    } else if status_code.starts_with('C') {
                        GitFileStatus::Copied { old_path }
                    } else {
                        GitFileStatus::try_from(status_code).map_err(|e| {
                            anyhow!(
                                "Invalid status code '{}' for renamed/copied path '{}': {}",
                                status_code,
                                path,
                                e
                            )
                        })?
                    };

                    files.push(GitStatusEntry {
                        path: PathBuf::from(path),
                        status,
                        staged: FileStagedState::from_status_code(status_code),
                    });
                    i += 1; // Skip the old path token
                } else {
                    log::warn!(
                        "Invalid format for renamed/copied entry: '{token}' - expected at least 10 parts, got {}",
                        parts.len()
                    );
                }
            }
            'u' => {
                // Unmerged entry: u <xy> <sub> <m1> <m2> <m3> <mW> <h1> <h2> <h3> <path>
                // Use splitn to preserve spaces in filenames (the path is the last field).
                let parts: Vec<&str> = token.splitn(11, ' ').collect();
                if parts.len() >= 11 {
                    let path = parts[10];
                    // An unmerged path cannot be staged until the conflict
                    // is resolved, so it is never reported as staged.
                    files.push(GitStatusEntry {
                        path: PathBuf::from(path),
                        status: GitFileStatus::Conflicted,
                        staged: FileStagedState::Unstaged,
                    });
                } else {
                    log::warn!(
                        "Invalid format for unmerged entry: '{}' - expected at least 11 parts, got {}",
                        token,
                        parts.len()
                    );
                }
            }
            '?' => {
                // Untracked entry: ? <path>
                if token.len() > 2 {
                    let path = &token[2..]; // Skip "? "
                    files.push(GitStatusEntry {
                        path: PathBuf::from(path),
                        status: GitFileStatus::Untracked,
                        // Untracked means "no index entry at all".
                        staged: FileStagedState::Unstaged,
                    });
                } else {
                    log::warn!(
                        "Invalid format for untracked entry: '{token}' - expected path after '? '"
                    );
                }
            }
            '!' => {
                // Ignored entry: ! <path>
                // We skip ignored files
            }
            _ => {
                // Unknown entry type, skip
                log::debug!("Unknown git status entry type '{entry_kind}' in token: '{token}'");
            }
        }

        i += 1;
    }

    Ok(files)
}

/// Bare `(path, status)` view of [`parse_status_entries`].
pub fn parse_status(status_output: &str) -> Result<Vec<(PathBuf, GitFileStatus)>> {
    Ok(parse_status_entries(status_output)?
        .into_iter()
        .map(|entry| (entry.path, entry.status))
        .collect())
}

/// Parses `git diff --name-status -z` output (status<NUL>path, or status<NUL>old<NUL>new).
pub fn parse_name_status(diff_output: &str) -> Result<Vec<(PathBuf, GitFileStatus)>> {
    if diff_output.is_empty() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    let tokens: Vec<&str> = diff_output.split('\0').collect();

    let mut i = 0;
    while i < tokens.len() {
        let token = tokens[i].trim();
        if token.is_empty() {
            i += 1;
            continue;
        }

        // First token should be the status (M, A, D, R, C, etc.)
        let status_char = token.chars().next().unwrap_or('M');

        // Next token should be the file path
        if i + 1 >= tokens.len() {
            // No filename following the status, skip
            i += 1;
            continue;
        }

        let mut file_path = tokens[i + 1].trim();
        if file_path.is_empty() {
            i += 2;
            continue;
        }

        let status = match status_char {
            'A' => GitFileStatus::New,
            'M' => GitFileStatus::Modified,
            'D' => GitFileStatus::Deleted,
            'R' => {
                // For renamed files: R<score> <null> old_path <null> new_path <null>
                // We need to get the old path from the next token
                if i + 2 < tokens.len() {
                    let old_path = file_path;
                    file_path = tokens[i + 2].trim();
                    i += 1; // Skip the new path token (we'll increment again at the end)
                    GitFileStatus::Renamed {
                        old_path: old_path.to_string(),
                    }
                } else {
                    GitFileStatus::Modified
                }
            }
            'C' => {
                // For copied files: C<score> <null> new_path <null> old_path <null>
                // We need to get the old path from the next token
                if i + 2 < tokens.len() {
                    let old_path = tokens[i + 2].trim().to_string();
                    i += 1; // Skip the old path token (we'll increment again at the end)
                    GitFileStatus::Copied { old_path }
                } else {
                    GitFileStatus::Modified
                }
            }
            _ => GitFileStatus::Modified,
        };

        files.push((PathBuf::from(file_path), status));

        // Move to next status token (skip status + filename, or status + old_path + new_path for R/C)
        i += 2;
    }

    Ok(files)
}

/// Parses the `# branch.*` header lines of `git status --branch --porcelain=2` (NUL or newline
/// separated). Handles an unborn branch (`oid (initial)`), a detached HEAD (`head (detached)`),
/// no upstream, and a gone upstream (no `branch.ab` line).
pub fn parse_branch_headers(status_output: &str) -> BranchHeaders {
    let mut headers = BranchHeaders::default();
    for line in status_output.split(['\0', '\n']) {
        let Some(rest) = line.strip_prefix("# branch.") else {
            continue;
        };
        if let Some(oid) = rest.strip_prefix("oid ") {
            headers.oid = (oid != "(initial)").then(|| oid.to_string());
        } else if let Some(head) = rest.strip_prefix("head ") {
            headers.head = (head != "(detached)").then(|| head.to_string());
        } else if let Some(upstream) = rest.strip_prefix("upstream ") {
            headers.upstream = Some(upstream.to_string());
        } else if let Some(ab) = rest.strip_prefix("ab ") {
            let mut parts = ab.split_whitespace();
            let ahead = parts.next().and_then(|p| p.strip_prefix('+')?.parse().ok());
            let behind = parts.next().and_then(|p| p.strip_prefix('-')?.parse().ok());
            if let (Some(ahead), Some(behind)) = (ahead, behind) {
                headers.ahead_behind = Some((ahead, behind));
            }
        }
    }
    headers
}

/// Sums `(additions, deletions)` over `git diff --numstat -z` output.
/// Binary files (`-\t-`) count as 0. A rename/copy entry is
/// `<add>\t<del>\t\0<old>\0<new>\0`, so its two path tokens are skipped.
pub fn sum_numstat_z(output: &str) -> (usize, usize) {
    let mut tokens = output.split('\0');
    let (mut additions, mut deletions) = (0usize, 0usize);
    while let Some(token) = tokens.next() {
        let mut fields = token.splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        additions += added.parse().unwrap_or(0);
        deletions += removed.parse().unwrap_or(0);
        if path.is_empty() {
            tokens.next();
            tokens.next();
        }
    }
    (additions, deletions)
}

/// Parses `git diff --numstat` (newline form) into per-path counts. Binary files render as
/// `-\t-\t<path>`; unparsable counts fall back to 0.
pub fn parse_numstat_map(output: &str) -> HashMap<PathBuf, NumStat> {
    let mut map = HashMap::new();
    for line in output.lines() {
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 3 {
            map.insert(
                PathBuf::from(parts[2]),
                NumStat {
                    lines_added: parts[0].parse().unwrap_or(0),
                    lines_removed: parts[1].parse().unwrap_or(0),
                    is_binary_file: parts[0] == "-" && parts[1] == "-",
                },
            );
        }
    }
    map
}

/// Parses `git diff --numstat` (newline form) into `FileChangeEntry`s, in output order.
pub fn parse_numstat_entries(output: &str) -> Vec<FileChangeEntry> {
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
    entries
}

/// Parses `git diff-tree --numstat` output (`splitn(3, '\t')`, so a path may contain tabs).
pub fn parse_numstat_entries_splitn(output: &str) -> Vec<FileChangeEntry> {
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
    entries
}

/// Parses `git log --format=COMMIT:%H\t%s --numstat` output.
pub fn parse_commit_log(output: &str) -> Result<Vec<Commit>> {
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

/// `(files_changed, insertions, deletions)` from `git diff --shortstat` output, e.g.
/// ` 1 file changed, 2 insertions(+), 17 deletions(-)`. `None` for empty output.
pub fn parse_shortstat(raw_output: &str) -> Option<(u32, u32, u32)> {
    let line = raw_output.trim();
    if line.is_empty() {
        return None;
    }
    let (mut files_changed, mut lines_added, mut lines_removed) = (0, 0, 0);
    let words: Vec<&str> = line.split_whitespace().collect();
    for (i, word) in words.iter().enumerate() {
        if let Ok(num) = word.parse::<u32>() {
            if let Some(next_word) = words.get(i + 1) {
                if next_word.starts_with("file") {
                    files_changed = num;
                } else if next_word.starts_with("insertion") {
                    lines_added = num;
                } else if next_word.starts_with("deletion") {
                    lines_removed = num;
                }
            }
        }
    }
    Some((files_changed, lines_added, lines_removed))
}

/// `(ahead, behind, equivalent)` from `git rev-list --left-right --cherry-mark --count HEAD...@{u}`.
pub fn parse_tracking_counts(output: &str) -> Option<(u32, u32, u32)> {
    let mut parts = output.split_whitespace();
    let ahead = parts.next()?.parse().ok()?;
    let behind = parts.next()?.parse().ok()?;
    let equivalent = parts.next().map(str::parse).transpose().ok()?.unwrap_or(0);
    Some((ahead, behind, equivalent))
}

/// The first non-empty trimmed line of `git rev-parse --abbrev-ref --symbolic-full-name @{u}`.
pub fn parse_upstream_ref(output: &str) -> Option<String> {
    output
        .lines()
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
}

/// Parses `git for-each-ref --format=%(refname:short)` output into `(name, is_main)` pairs,
/// skipping HEAD pointers and detached states and removing duplicates, newest first.
pub fn parse_branch_list(output: &str, main_branch: &str) -> Vec<(String, bool)> {
    let mut seen = std::collections::HashSet::new();
    let mut branches = Vec::new();
    for branch in output.lines() {
        let branch = branch.trim();
        if branch.is_empty() || branch.contains("HEAD") || branch.starts_with('(') {
            continue;
        }
        let is_main = branch == main_branch || branch == main_branch.trim_start_matches("origin/");
        if seen.insert(branch.to_string()) {
            branches.push((branch.to_string(), is_main));
        }
    }
    branches
}

/// Whether `s` has the shape of an abbreviated or full commit SHA (7..=40 hex digits).
pub fn looks_like_commit_sha(s: &str) -> bool {
    (7..=40).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Returns a prefix of `s` at most `byte_cap` long that ends on a UTF-8 char boundary.
pub fn truncate_on_char_boundary(s: &str, byte_cap: usize) -> &str {
    if s.len() <= byte_cap {
        return s;
    }
    let mut cut = byte_cap;
    while cut > 0 && !s.is_char_boundary(cut) {
        cut -= 1;
    }
    &s[..cut]
}

/// Maximum number of bytes in a PR title passed to `gh pr create`.
pub const MAX_PR_TITLE_BYTES: usize = 200;

/// Trims an AI-generated PR title to a single line and caps its length.
pub fn sanitize_pr_title(raw: &str) -> String {
    let first_line = raw.lines().next().unwrap_or("").trim();
    truncate_on_char_boundary(first_line, MAX_PR_TITLE_BYTES).to_string()
}

/// Whether a `git stash push` / `git restore` error means "git cannot resolve HEAD yet".
pub fn is_unresolvable_head_error(message: &str) -> bool {
    message.contains("could not resolve") && message.contains("HEAD")
}

/// `gh pr view` failures that mean the branch simply has no pull request.
pub fn is_no_pr_for_branch_error(error_msg: &str) -> bool {
    let lower = error_msg.to_lowercase();
    lower.contains("no pull requests found for branch")
        || lower.contains("no open pull requests found for branch")
}

/// `gh pr view` failures that authoritatively mean a PR lookup does not apply here.
pub fn is_pr_lookup_not_applicable_error(error_msg: &str) -> bool {
    let lower = error_msg.to_lowercase();
    is_no_pr_for_branch_error(error_msg)
        || lower.contains(
            "none of the git remotes configured for this repository point to a known github host",
        )
        || lower.contains("no github remotes")
        || lower.contains("not a github repository")
        || lower.contains("could not determine base repo")
}

/// `gh repo view` failures that authoritatively mean there is no GitHub repository info.
pub fn is_repository_lookup_not_applicable_error(error_msg: &str) -> bool {
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

/// Sentinel paths (relative to the git dir) whose presence means the repo is mid-operation or
/// locked: merge / cherry-pick / revert / rebase, or another process holds the index lock.
pub const OPERATION_SENTINELS: [&str; 6] = [
    "MERGE_HEAD",
    "CHERRY_PICK_HEAD",
    "REVERT_HEAD",
    "rebase-merge",
    "rebase-apply",
    "index.lock",
];

/// A POSIX script that prints each present sentinel on its own line (empty output: none), for
/// hosts where the filesystem is only reachable through a shell. `git_dir` is the (already
/// resolved) git directory, quoted here.
pub fn operation_probe_script(git_dir: &str) -> String {
    let dir = crate::shellcmd::sh_quote(git_dir);
    let names = OPERATION_SENTINELS.join(" ");
    format!("for f in {names}; do [ -e {dir}/\"$f\" ] && printf '%s\\n' \"$f\"; done; true")
}

/// The PowerShell form of [`operation_probe_script`] (same output).
pub fn operation_probe_script_pwsh(git_dir: &str) -> String {
    let dir = format!("'{}'", git_dir.replace('\'', "''"));
    let names: Vec<String> = OPERATION_SENTINELS
        .iter()
        .map(|n| format!("'{n}'"))
        .collect();
    format!(
        "foreach ($f in @({})) {{ if (Test-Path -LiteralPath (Join-Path {dir} $f)) {{ $f }} }}",
        names.join(", ")
    )
}

/// Whether [`operation_probe_script`] output names any sentinel.
pub fn parse_operation_probe(output: &str) -> bool {
    output
        .lines()
        .any(|line| OPERATION_SENTINELS.contains(&line.trim()))
}

/// The `git apply` argv (after `apply`) for hunk staging. `source` is a patch file path, or `-`
/// for stdin; `recount` adds `--recount` (stdin patches reconstructed by the client). Never
/// passes `--unidiff-zero`: git's context check is what catches a stale patch.
pub fn apply_cached_args<'a>(reverse: bool, recount: bool, source: &'a str) -> Vec<&'a str> {
    let mut args = vec!["apply", "--cached"];
    if reverse {
        args.push("--reverse");
    }
    if recount {
        args.push("--recount");
    }
    args.push(source);
    args
}

/// The `git add -- <paths>` / `git restore --staged -- <paths>` argv.
pub fn stage_paths_args(relative_paths: &[String], unstage: bool) -> Vec<&str> {
    let mut args: Vec<&str> = if unstage {
        vec!["restore", "--staged", "--"]
    } else {
        vec!["add", "--"]
    };
    args.extend(relative_paths.iter().map(String::as_str));
    args
}

/// `git rm --cached -- <paths>`: the pre-first-commit un-stage.
pub fn rm_cached_args(relative_paths: &[String]) -> Vec<&str> {
    let mut args: Vec<&str> = vec!["rm", "--cached", "--"];
    args.extend(relative_paths.iter().map(String::as_str));
    args
}

/// `git stash push -u -m <msg> -- <paths>`.
pub fn stash_args<'a>(msg: &'a str, relative_paths: &'a [String]) -> Vec<&'a str> {
    let mut args = vec!["stash", "push", "-u", "-m", msg, "--"];
    args.extend(relative_paths.iter().map(String::as_str));
    args
}

/// The stash message: `<app>: stash <path>` or `<app>: stash N files`.
pub fn stash_message(app_name: &str, relative_paths: &[String]) -> String {
    if relative_paths.len() == 1 {
        format!("{app_name}: stash {}", relative_paths[0])
    } else {
        format!("{app_name}: stash {} files", relative_paths.len())
    }
}

/// The `gh pr create` argv. With both title and body: `--title`/`--body` (title sanitised);
/// otherwise `--fill`.
pub fn gh_pr_create_args(base: &str, title: Option<&str>, body: Option<&str>) -> Vec<String> {
    match (title, body) {
        (Some(t), Some(b)) => vec![
            "pr".into(),
            "create".into(),
            "--base".into(),
            base.into(),
            "--title".into(),
            sanitize_pr_title(t),
            "--body".into(),
            b.into(),
        ],
        _ => vec![
            "pr".into(),
            "create".into(),
            "--base".into(),
            base.into(),
            "--fill".into(),
        ],
    }
}

/// `gh pr view --json ...` fields the app reads.
pub const GH_PR_VIEW_ARGS: [&str; 4] = [
    "pr",
    "view",
    "--json",
    "number,url,state,isDraft,baseRefName",
];

/// `gh repo view --json ...` fields the app reads.
pub const GH_REPO_VIEW_ARGS: [&str; 4] = ["repo", "view", "--json", "name,owner,url"];

/// The PR number from the URL `gh pr create` prints (last path segment).
pub fn parse_pr_number_from_url(url: &str) -> Option<u64> {
    url.rsplit('/').next().and_then(|s| s.parse::<u64>().ok())
}
