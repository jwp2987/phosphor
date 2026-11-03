//! Context chips built into Zap

use chrono::Local;
use warp_util::path::user_friendly_path;

use crate::terminal::shell::ShellType;

use super::{
    context_chip::{GeneratorContext, ShellCommand, ShellCommandGenerator},
    ChipValue,
};

#[cfg(test)]
#[path = "builtins_tests.rs"]
mod tests;

/// Generator function for the current working directory.
pub fn working_directory(ctx: &GeneratorContext) -> Option<ChipValue> {
    let pwd = ctx.active_block_metadata.current_working_directory()?;
    let home_dir = ctx.active_session.and_then(|session| session.home_dir());
    Some(ChipValue::Text(
        user_friendly_path(pwd, home_dir).to_string(),
    ))
}

/// Generator function that always shows the username.
pub fn username(ctx: &GeneratorContext) -> Option<ChipValue> {
    ctx.active_session
        .map(|session| ChipValue::Text(session.user().to_owned()))
}

/// Generator function that always shows the host name.
pub fn hostname(ctx: &GeneratorContext) -> Option<ChipValue> {
    ctx.active_session
        .map(|session| ChipValue::Text(session.hostname().to_owned()))
}

/// Generator function that shows the current Python virtual environment.
pub fn virtual_environment(ctx: &GeneratorContext) -> Option<ChipValue> {
    ctx.current_environment
        .python_virtualenv()
        .cloned()
        .map(ChipValue::Text)
}

/// Generator function that shows the current Anaconda/conda environment.
pub fn conda_environment(ctx: &GeneratorContext) -> Option<ChipValue> {
    ctx.current_environment
        .conda_environment()
        .cloned()
        .map(ChipValue::Text)
}

/// Generator function that shows the current Node.js version.
pub fn node_version(ctx: &GeneratorContext) -> Option<ChipValue> {
    ctx.current_environment
        .node_version()
        .cloned()
        .map(ChipValue::Text)
}

/// Generator function that shows the current date.
pub fn date(_: &GeneratorContext) -> Option<ChipValue> {
    Some(ChipValue::Text(
        Local::now().format("%a %b %d %Y").to_string(),
    ))
}

/// Generator function that shows the current time in 12-hour format.
pub fn time12(_: &GeneratorContext) -> Option<ChipValue> {
    Some(ChipValue::Text(Local::now().format("%I:%M %P").to_string()))
}

/// Generator function that shows the current time in 24-hour format.
pub fn time24(_: &GeneratorContext) -> Option<ChipValue> {
    Some(ChipValue::Text(Local::now().format("%H:%M").to_string()))
}

/// Generator function for SSH session chip.
pub fn ssh_session(ctx: &GeneratorContext) -> Option<ChipValue> {
    let session = ctx.active_session?;
    if session.is_legacy_ssh_session()
        || matches!(
            session.session_type(),
            crate::terminal::model::session::SessionType::WarpifiedRemote { .. }
        )
    {
        let user = session.user();
        Some(ChipValue::Text(format!("{}@{}", user, session.hostname())))
    } else {
        None
    }
}

/// Generator function for Subshell session chip.
pub fn subshell(ctx: &GeneratorContext) -> Option<ChipValue> {
    let session = ctx.active_session?;
    let subshell_info = session.subshell_info().as_ref()?;

    let session_type = if let Some(env_var_collection_name) = &subshell_info.env_var_collection_name
    {
        env_var_collection_name.clone()
    } else {
        crate::terminal::warpify::subshell_display_name(&subshell_info.spawning_command)
            .unwrap_or("subshell")
            .to_string()
    };
    Some(ChipValue::Text(session_type))
}

/// Generator function that shows the current Git branch.
pub fn shell_git_branch() -> ShellCommandGenerator {
    // Note this command must stay in sync with how PrecmdValue::git_branch is generated in the
    // bootstrap scripts, at least until that is removed.
    const SH_COMMAND: &str = "GIT_OPTIONAL_LOCKS=0 git symbolic-ref --short HEAD 2> /dev/null || \
     GIT_OPTIONAL_LOCKS=0 git rev-parse --short HEAD 2> /dev/null";
    let pwsh_command = pwsh_git_branch_command();

    let command = ShellCommand::shell_specific([
        (ShellType::PowerShell, pwsh_command),
        (ShellType::Bash, SH_COMMAND.to_string()),
        (ShellType::Zsh, SH_COMMAND.to_string()),
        (ShellType::Fish, SH_COMMAND.to_string()),
    ]);

    ShellCommandGenerator::new(command, Some(vec!["git".to_owned()]))
}

pub fn shell_other_git_branches() -> ShellCommandGenerator {
    const SH_COMMAND: &str = "git --no-optional-locks branch --no-color --sort=-committerdate; \
        printf '\\036\\n'; \
        git --no-optional-locks worktree list --porcelain";
    let pwsh_command = pwsh_other_git_branches_command();

    let command = ShellCommand::shell_specific([
        (ShellType::PowerShell, pwsh_command),
        (ShellType::Bash, SH_COMMAND.to_string()),
        (ShellType::Zsh, SH_COMMAND.to_string()),
        (ShellType::Fish, SH_COMMAND.to_string()),
    ]);

    ShellCommandGenerator::new(command, Some(vec!["git".to_owned()]))
}

/// Generator function that shows the current Git branch plus its ahead/behind
/// (or rebased) tracking status against the configured upstream.
///
/// Used as a remote-session / shell fallback for the `GitBranchStatus` chip when
/// `GitRepoStatusModel` is unavailable. The output format matches
/// [`display_chip::GitBranchTrackingStatus::display_text`] so it can be parsed
/// back via `GitBranchTrackingStatus::from_display_text`.
pub fn shell_git_branch_status() -> ShellCommandGenerator {
    const SH_COMMAND: &str = "\
        sh -c 'branch=$(GIT_OPTIONAL_LOCKS=0 git symbolic-ref --short HEAD 2>/dev/null || \
        GIT_OPTIONAL_LOCKS=0 git rev-parse --short HEAD 2>/dev/null) || exit 1; \
        [ -n \"$branch\" ] || exit 1; \
        display_count() { if [ \"$1\" -gt 999 ]; then printf \"999+\"; else printf \"%s\" \"$1\"; fi; }; \
        if counts=$(GIT_OPTIONAL_LOCKS=0 git rev-list --left-right --cherry-mark --count HEAD...@{u} 2>/dev/null); then \
            set -- $counts; \
            ahead=${1:-0}; behind=${2:-0}; equivalent=${3:-0}; status=\"\"; \
            if [ \"$ahead\" -eq 0 ] && [ \"$behind\" -eq 0 ] && [ \"$equivalent\" -gt 0 ]; then \
                status=\"⇅\"; \
            else \
                if [ \"$ahead\" -gt 0 ]; then status=\"↑$(display_count \"$ahead\")\"; fi; \
                if [ \"$behind\" -gt 0 ]; then \
                    behind_status=\"↓$(display_count \"$behind\")\"; \
                    if [ -n \"$status\" ]; then status=\"$status $behind_status\"; else status=\"$behind_status\"; fi; \
                fi; \
            fi; \
            if [ -n \"$status\" ]; then printf \"%s • %s\\n\" \"$branch\" \"$status\"; else printf \"%s\\n\" \"$branch\"; fi; \
        else \
            printf \"%s\\n\" \"$branch\"; \
        fi'";
    let pwsh_command = safe_git_powershell(
        "$branch = git symbolic-ref --short HEAD 2>$null; \
        if ($LASTEXITCODE -ne 0 -or -not $branch) { \
            $branch = git rev-parse --short HEAD 2>$null; \
        } \
        if ($LASTEXITCODE -ne 0 -or -not $branch) { throw } \
        function Format-GitCount($count) { if ($count -gt 999) { '999+' } else { [string]$count } } \
        $counts = git rev-list --left-right --cherry-mark --count 'HEAD...@{u}' 2>$null; \
        if ($LASTEXITCODE -eq 0 -and $counts) { \
            $parts = $counts -split '\\s+'; \
            if ($parts.Length -ge 2) { \
                $ahead = [int]$parts[0]; \
                $behind = [int]$parts[1]; \
                $equivalent = if ($parts.Length -ge 3) { [int]$parts[2] } else { 0 }; \
                $status = @(); \
                if ($ahead -eq 0 -and $behind -eq 0 -and $equivalent -gt 0) { \
                    $status += '⇅'; \
                } else { \
                    if ($ahead -gt 0) { $status += \"↑$(Format-GitCount $ahead)\" } \
                    if ($behind -gt 0) { $status += \"↓$(Format-GitCount $behind)\" } \
                } \
                if ($status.Count -gt 0) { \"$branch • $($status -join ' ')\" } else { $branch } \
            } else { \
                $branch; \
            } \
        } else { \
            $branch; \
            $global:LASTEXITCODE = 0; \
        }",
    );

    let command = ShellCommand::shell_specific([
        (ShellType::PowerShell, pwsh_command),
        (ShellType::Bash, SH_COMMAND.to_string()),
        (ShellType::Zsh, SH_COMMAND.to_string()),
        (ShellType::Fish, SH_COMMAND.to_string()),
    ]);

    ShellCommandGenerator::new(command, Some(vec!["git".to_owned()]))
}

/// Generator function to get summary of git diff (num files changed and num lines changed).
///
/// Used as a remote-session fallback when GitRepoStatusModel is unavailable.
pub fn shell_git_line_changes() -> ShellCommandGenerator {
    const GIT_COMMAND: &str =
        "GIT_OPTIONAL_LOCKS=0 git -c diff.autoRefreshIndex=false diff --shortstat HEAD";

    let command = ShellCommand::shell_specific([
        (ShellType::Bash, GIT_COMMAND.to_string()),
        (ShellType::Zsh, GIT_COMMAND.to_string()),
        (ShellType::Fish, GIT_COMMAND.to_string()),
        (ShellType::PowerShell, pwsh_git_line_changes_command()),
    ]);

    ShellCommandGenerator::new(command, Some(vec!["git".to_owned()]))
}

/// The program and arguments of the git diff-stat chip command.
const GIT_LINE_CHANGES_ARGS: &[&str] = &[
    "-c",
    "diff.autoRefreshIndex=false",
    "diff",
    "--shortstat",
    "HEAD",
];

/// The program and arguments of the Kubernetes context chip command.
const KUBECTL_CURRENT_CONTEXT_COMMAND: &str = "kubectl config current-context";

fn pwsh_git_line_changes_command() -> String {
    safe_git_powershell(&format!("git {}", GIT_LINE_CHANGES_ARGS.join(" ")))
}

pub fn kubernetes_current_context() -> ShellCommandGenerator {
    ShellCommandGenerator::new(
        ShellCommand::portable(KUBECTL_CURRENT_CONTEXT_COMMAND),
        Some(vec!["kubectl".to_owned()]),
    )
}

fn pwsh_git_branch_command() -> String {
    safe_git_powershell(
        "git symbolic-ref --short HEAD  2>$null; \
            if ($? -eq $false) { \
                git rev-parse --short HEAD 2>$null; \
            }",
    )
}

fn pwsh_other_git_branches_command() -> String {
    safe_git_powershell(
        "git --no-optional-locks branch --no-color --sort=-committerdate; \
        [char]30; \
        git --no-optional-locks worktree list --porcelain",
    )
}

/// One external program with fixed arguments.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct DirectInvocation {
    pub program: &'static str,
    pub args: &'static [&'static str],
    /// Variables the shell command sets for the program (and puts back afterwards).
    pub env: &'static [(&'static str, &'static str)],
}

/// A chip command that is made of external programs only, so it can be run without a shell.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum DirectPlan {
    /// One program.
    Single(DirectInvocation),
    /// The first program, and only if it fails, the second: the result is the first success's
    /// output, else the second's.
    FirstSuccess(DirectInvocation, DirectInvocation),
    /// Both programs one after the other with a record separator line (`\x1e` and a newline)
    /// between their stdout; the result's status is the second's, as the shell's is the last
    /// command's.
    JoinedWithRecordSeparator(DirectInvocation, DirectInvocation),
}

const GIT_ENV: &[(&str, &str)] = &[("GIT_OPTIONAL_LOCKS", "0")];

/// The plain command the session runs for command-correction branch names. It is a single git
/// program, so it is run directly as well (#1334): on Windows it was the one per-prompt
/// `powershell.exe -EncodedCommand` that the chip commands' direct path did not cover.
pub(crate) const GIT_BRANCHES_FOR_CORRECTIONS_COMMAND: &str =
    "git --no-optional-locks branch --no-color";

/// The one listing of every git alias that the completer's alias lookups are answered from
/// (#1342). On Windows it was still a `powershell.exe -EncodedCommand` per repository and
/// config change; it is a single git program, so it is run directly as well. PowerShell passes
/// `'^alias.'` to git as `^alias.`, so the direct argument is the same text.
pub(crate) const GIT_ALIAS_LISTING_COMMAND: &str =
    "git --no-optional-locks config --get-regexp '^alias.'";

/// Recognises the PowerShell chip commands that run at prompts and are only external programs,
/// so the local executor can start those programs itself instead of a `pwsh -NoProfile` process
/// (#1230): a pwsh start costs 140-250 ms and ~0.25 s of CPU on Linux and several times that on
/// Windows, where bash costs 3 ms. Matches the exact text the chips above generate, nothing
/// else: any other command still goes through pwsh.
pub(crate) fn direct_powershell_invocation(command: &str) -> Option<DirectPlan> {
    if command == pwsh_git_line_changes_command() {
        return Some(DirectPlan::Single(DirectInvocation {
            program: "git",
            args: GIT_LINE_CHANGES_ARGS,
            env: GIT_ENV,
        }));
    }
    if command == GIT_BRANCHES_FOR_CORRECTIONS_COMMAND {
        return Some(DirectPlan::Single(DirectInvocation {
            program: "git",
            args: &["--no-optional-locks", "branch", "--no-color"],
            env: GIT_ENV,
        }));
    }
    if command == GIT_ALIAS_LISTING_COMMAND {
        return Some(DirectPlan::Single(DirectInvocation {
            program: "git",
            args: &["--no-optional-locks", "config", "--get-regexp", "^alias."],
            env: GIT_ENV,
        }));
    }
    if command == KUBECTL_CURRENT_CONTEXT_COMMAND {
        return Some(DirectPlan::Single(DirectInvocation {
            program: "kubectl",
            args: &["config", "current-context"],
            env: &[],
        }));
    }
    if command == pwsh_git_branch_command() {
        return Some(DirectPlan::FirstSuccess(
            DirectInvocation {
                program: "git",
                args: &["symbolic-ref", "--short", "HEAD"],
                env: GIT_ENV,
            },
            DirectInvocation {
                program: "git",
                args: &["rev-parse", "--short", "HEAD"],
                env: GIT_ENV,
            },
        ));
    }
    if command == pwsh_other_git_branches_command() {
        return Some(DirectPlan::JoinedWithRecordSeparator(
            DirectInvocation {
                program: "git",
                args: &[
                    "--no-optional-locks",
                    "branch",
                    "--no-color",
                    "--sort=-committerdate",
                ],
                env: GIT_ENV,
            },
            DirectInvocation {
                program: "git",
                args: &["--no-optional-locks", "worktree", "list", "--porcelain"],
                env: GIT_ENV,
            },
        ));
    }
    None
}

/// Generator function that shows the current svn "branch".
/// Since svn uses directories for different branches and tags,
/// we take the latest directory of the working copy as the branch/tag name.
pub fn svn_branch_context() -> ShellCommandGenerator {
    const SH_COMMAND: &str = "basename $(svn info --show-item wc-root)";
    const PWSH_COMMAND: &str = "svn info --show-item wc-root | Microsoft.PowerShell.Management\\Split-Path -Leaf";
    let command = ShellCommand::shell_specific([
        (ShellType::PowerShell, PWSH_COMMAND.to_string()),
        (ShellType::Bash, SH_COMMAND.to_string()),
        (ShellType::Zsh, SH_COMMAND.to_string()),
        (ShellType::Fish, SH_COMMAND.to_string()),
    ]);

    ShellCommandGenerator::new(command, Some(vec!["svn".to_owned()]))
}

/// Generator function that shows the number of uncommitted svn files/directories.
pub fn svn_dirty_items() -> ShellCommandGenerator {
    const SH_COMMAND: &str = "count=$(svn status | wc -l) \
        && (( $count > 0 )) && echo $(( $count ))";
    const FISH_COMMAND: &str = "set count (svn status | wc -l) \
        && test $count -gt 0 && string trim $count";
    const PWSH_COMMAND: &str = "svn status | Microsoft.PowerShell.Utility\\Measure-Object -line | \
        Microsoft.PowerShell.Core\\Where-Object {$_.Lines -gt 0 } | Microsoft.PowerShell.Core\\ForEach-Object { $_.Lines }";
    let command = ShellCommand::shell_specific([
        (ShellType::Bash, SH_COMMAND.to_string()),
        (ShellType::Zsh, SH_COMMAND.to_string()),
        (ShellType::Fish, FISH_COMMAND.to_string()),
        (ShellType::PowerShell, PWSH_COMMAND.to_string()),
    ]);
    ShellCommandGenerator::new(command, Some(vec!["svn".to_owned()]))
}

fn safe_git_powershell(cmd: &str) -> String {
    format!(
        "\
        $gitOptionalLocks = $env:GIT_OPTIONAL_LOCKS; \
        $env:GIT_OPTIONAL_LOCKS = 0; \
        try {{ \
            {cmd} \
        }} finally {{ \
            $success = $?; \
            $exitCode = $LASTEXITCODE; \
            $env:GIT_OPTIONAL_LOCKS = $gitOptionalLocks; \
            if ($exitCode -ne 0 -or -not $success) {{ \
                throw \
            }} \
        }}"
    )
}
