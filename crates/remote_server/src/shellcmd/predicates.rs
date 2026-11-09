//! Pure predicates on a command line that the refresh logic uses to decide whether a command
//! could have moved HEAD, the working directory or git state. They live here (no I/O, no warpui)
//! so the app's local refresh and the remote refresh coordinator share one definition;
//! `app/src/util/git.rs` delegates to them.

/// Whether a shell command probably moves HEAD (switch/checkout/worktree/rebase/
/// merge/pull/reset/stash/cherry-pick/bisect), so a branch read right after it must
/// not trust a cached value. Handles `git -C dir ...`, `git -c k=v ...` and
/// `&&`/`||`/`;`/`|`-joined commands. Deliberately over-approximate: a false
/// positive costs one cheap file read.
pub fn command_may_change_head(command: &str) -> bool {
    const HEAD_CHANGING: &[&str] = &[
        "switch",
        "checkout",
        "worktree",
        "rebase",
        "merge",
        "pull",
        "reset",
        "stash",
        "cherry-pick",
        "bisect",
    ];
    let spaced = command.replace(['&', '|', ';', '\n', '(', ')'], " \u{0} ");
    let mut tokens = spaced.split_whitespace().peekable();
    while let Some(tok) = tokens.next() {
        if tok != "git" {
            continue;
        }
        // Skip global options to reach the subcommand.
        while let Some(&next) = tokens.peek() {
            if next == "-C" || next == "-c" || next == "--git-dir" || next == "--work-tree" {
                tokens.next();
                tokens.next();
            } else if next.starts_with('-') {
                tokens.next();
            } else {
                break;
            }
        }
        if let Some(&sub) = tokens.peek()
            && HEAD_CHANGING.contains(&sub)
        {
            return true;
        }
    }
    false
}

/// Whether a shell command probably changes the shell's working directory (`cd`,
/// `pushd`, `popd`), so the cwd/branch the agent is told about after it must not
/// trust a value cached before it ran. Over-approximate by design: a false positive
/// costs one cheap file read.
pub fn command_may_change_cwd(command: &str) -> bool {
    command
        .replace(['&', '|', ';', '\n', '(', ')', '{', '}'], " ")
        .split_whitespace()
        .any(|tok| matches!(tok, "cd" | "pushd" | "popd"))
}

/// Whether the command names a VCS tool, i.e. could change git state in a way that must not wait
/// for (or depend on) a file watcher. Plain file edits and build output are the watcher's job.
pub fn command_may_change_git_state(command: &str) -> bool {
    const VCS: &[&str] = &[
        "git", "gh", "gt", "hub", "jj", "hg", "sl", "svn", "stg", "lazygit", "pijul",
    ];
    command
        .split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_' || c == '.'))
        .any(|tok| {
            let tok = tok.trim_end_matches(".exe");
            VCS.contains(&tok) || tok.starts_with("git-")
        })
}

#[cfg(test)]
#[path = "predicates_tests.rs"]
mod tests;
