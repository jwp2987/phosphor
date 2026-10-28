//! Splitting a pane from an SSH session onto the same host (#1069).
//!
//! A default split of a pane whose active session is an SSH session (the ssh wrapper's managed
//! session, or a tmux-warpified one) replays the `ssh ...` command the user typed in the new
//! pane's local shell, then `cd`s to the original pane's remote working directory once the new
//! remote session has bootstrapped.
//!
//! The replayed command goes through the shell's ssh wrapper like any typed `ssh`, so the new
//! pane gets its own Phosphor-owned ControlMaster (`$SSH_SOCKET_DIR/<new session id>`) whose
//! lifetime is tied to the new pane alone. It deliberately does NOT ride the original pane's
//! master: the wrapper and `TerminalView::shutdown_for_discard` end that master with
//! `ssh -O exit`, which would kill the split's session when the original pane closes. A user
//! ControlMaster attached via `warpify.ssh.reuse_existing_control_master` is found by the
//! wrapper again for the new pane, which then needs no new authentication, and Phosphor still
//! never tears it down.
//!
//! The replay runs in the base pane's own local shell kind and, when it is known, in the local
//! directory the original `ssh` was typed in, so relative paths, globs and quoting mean what they
//! meant the first time. When that directory cannot be recovered, a command whose meaning
//! depends on it is not replayed at all ([`is_cwd_sensitive`]).
//!
//! Opt-in: `warpify.ssh.split_inherits_ssh` defaults to off.
//!
//! Only the pure selection logic lives here; the pane wiring is in `PaneGroup::ssh_split_plan` /
//! `PaneGroup::add_split_session`, and the post-bootstrap follow-up in [`SplitFollowUp`], which
//! `TerminalView` drives.
//!
//! Ported by content from TranscriptionFactory/warp `6f7be0287`, `5cf24a5de` and `3ef7cf928`,
//! with a stricter replay gate (see [`replayable_ssh_command`]).

use std::path::PathBuf;

use crate::terminal::available_shells::AvailableShell;
use crate::terminal::model::session::command_executor::shell_quote_arg;
use crate::terminal::model::session::{SessionId, SessionType};
use crate::terminal::model::terminal_model::SubshellInitializationInfo;
use crate::terminal::shell::ShellType;
use crate::terminal::ssh::util::parse_interactive_ssh_command;

/// Characters that make a command line more than a plain word list: command separators,
/// background, pipes, redirections, subshells, parameter / command substitution, and bash
/// history expansion. A command containing any of them is never replayed, because replaying it
/// could re-run side effects or expand to something other than what the user ran.
const UNSAFE_REPLAY_CHARS: &[char] = &[';', '&', '|', '<', '>', '(', ')', '$', '`', '!'];

/// Returns the ssh command typed to start a remote session, if it parsed as an interactive ssh.
///
/// A tmux-warpified session records the command in `subshell_info`; a session started by the
/// ssh wrapper has no `subshell_info`, so its command is kept in
/// `SessionInfo::legacy_ssh_spawning_command` and parsed here.
pub fn resolve_ssh_spawning_command<'a>(
    subshell_info: Option<&'a SubshellInitializationInfo>,
    legacy_ssh_spawning_command: Option<&'a str>,
) -> Option<&'a str> {
    match subshell_info {
        Some(info) => info
            .ssh_connection_info
            .is_some()
            .then_some(info.spawning_command.as_str()),
        // The block's command text can carry surrounding whitespace (a leading space, a
        // trailing tab); the parser expects the bare command.
        None => legacy_ssh_spawning_command
            .map(str::trim)
            .filter(|command| parse_interactive_ssh_command(command).is_some()),
    }
}

/// Returns `command` (trimmed) if it is safe to type again, verbatim, into a fresh local shell
/// of the same kind: a literal `ssh` invocation of an interactive session with a destination,
/// and nothing else.
///
/// The command is replayed verbatim rather than re-quoted from parsed tokens so that every
/// option the user gave (`-p`, `-J`, `-i ~/key`, `-o 'ProxyCommand=...'`) means exactly what it
/// meant the first time. That is only sound because anything that could make the line do more
/// than run ssh is rejected: shell operators, substitutions and control characters (a raw CR or
/// LF would submit early). The text is the user's own command as recorded by the local shell,
/// not anything the remote host sent.
pub fn replayable_ssh_command(command: &str) -> Option<String> {
    let command = command.trim();
    if command.chars().any(char::is_control) || command.contains(UNSAFE_REPLAY_CHARS) {
        return None;
    }
    // A literal `ssh`: not `command ssh` (which bypasses the wrapper), not `sshpass ... ssh`,
    // `gcloud compute ssh`, `FOO=1 ssh` or an alias.
    if command.split_whitespace().next() != Some("ssh") {
        return None;
    }
    let parsed = parse_interactive_ssh_command(command)?;
    parsed.host.as_ref()?;
    Some(command.to_string())
}

/// Returns the command to run in a new pane split from an SSH session so that the new pane
/// lands on the same remote host, or `None` if the split should stay a plain local shell.
///
/// Only an SSH session started directly from a local session qualifies: a nested hop (ssh from
/// inside a remote session) has no local command that reaches it.
pub fn inherited_ssh_command(
    session_type: &SessionType,
    ssh_spawning_command: Option<&str>,
    spawning_session_type: Option<&SessionType>,
) -> Option<String> {
    if !matches!(session_type, SessionType::WarpifiedRemote { .. }) {
        return None;
    }
    if !matches!(spawning_session_type, Some(SessionType::Local)) {
        return None;
    }
    replayable_ssh_command(ssh_spawning_command?)
}

/// How a split from an SSH pane re-creates that session in the new pane.
#[derive(Debug)]
pub struct SshSplitPlan {
    /// The ssh command to replay, verbatim (see [`replayable_ssh_command`]).
    pub command: String,
    /// The local directory the ssh was typed in, when it is known and still exists. `None`
    /// only for a command that is not [`is_cwd_sensitive`].
    pub local_cwd: Option<PathBuf>,
    /// The base pane's local shell, so the replay runs in the same kind of shell.
    pub shell: Option<AvailableShell>,
    /// The remote directory to return to once the new session has bootstrapped.
    pub remote_cd: Option<PendingRemoteCd>,
}

/// Working directory of the original pane's remote session, to restore in a pane that
/// inherited its ssh connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingRemoteCd {
    /// `user@hostname` of the remote session the path was read from.
    pub remote_host: String,
    /// Raw remote path. It is never a local path.
    pub path: String,
}

/// Returns the `cd` command to run once the inherited ssh session has bootstrapped. Only fires
/// if the bootstrapped session is the host the path came from and the input is untouched. The
/// path is remote-supplied, so it is always quoted for the remote shell.
pub fn remote_cd_command(
    pending: &PendingRemoteCd,
    session_host: &str,
    shell_type: ShellType,
    input_is_empty: bool,
) -> Option<String> {
    (input_is_empty && !pending.path.is_empty() && pending.remote_host == session_host)
        .then(|| format!("cd {}", shell_quote_arg(&pending.path, shell_type)))
}

/// `-o` keys whose value is a local file path (resolved against ssh's working directory when
/// relative) or a local command (run from it). Compared case-insensitively, as ssh does.
const CWD_SENSITIVE_OPTION_KEYS: &[&str] = &[
    "identityfile",
    "certificatefile",
    "userknownhostsfile",
    "globalknownhostsfile",
    "revokedhostkeys",
    "controlpath",
    "identityagent",
    "include",
    "securitykeyprovider",
    "pkcs11provider",
    "xauthlocation",
    "proxycommand",
    "localcommand",
    "knownhostscommand",
];

/// Option letters whose argument is a local file path.
const PATH_ARG_FLAGS: &[char] = &['i', 'F', 'E'];

/// Option letters that take an argument (see `is_interactive_ssh_session` in the bootstrap
/// scripts); the argument is skipped.
const OTHER_ARG_FLAGS: &[char] = &[
    'B', 'b', 'c', 'D', 'e', 'I', 'J', 'L', 'l', 'm', 'O', 'P', 'p', 'R', 'S', 'W', 'w',
];

/// Whether `value` (an ssh path argument) depends on the working directory: anything that is not
/// absolute, `~`-relative or an ssh `%` token expansion. `none` and `SSH_AUTH_SOCK` are keywords.
fn is_relative_path_arg(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty()
        || value.eq_ignore_ascii_case("none")
        || value == "SSH_AUTH_SOCK"
        || value.starts_with(['/', '~', '%'])
        || value.starts_with("\\\\")
    {
        return false;
    }
    let bytes = value.as_bytes();
    let is_windows_absolute = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/');
    !is_windows_absolute
}

/// Whether an `-o` argument (`Key=value` or `Key value`) depends on the working directory.
fn is_cwd_sensitive_option(option: &str) -> bool {
    let option = option.trim();
    let split_at = option.find(['=', ' ', '\t']).unwrap_or(option.len());
    let (key, value) = option.split_at(split_at);
    let value = value.trim_start_matches(['=', ' ', '\t']);
    if !CWD_SENSITIVE_OPTION_KEYS
        .iter()
        .any(|sensitive| key.eq_ignore_ascii_case(sensitive))
    {
        return false;
    }
    // A command is run by ssh from its working directory: never assume it is independent of it.
    let is_command = key.to_ascii_lowercase().ends_with("command");
    is_command || is_relative_path_arg(value)
}

/// Whether the meaning of `command` (an ssh command line that passed
/// [`replayable_ssh_command`]) depends on the local directory it is run in: a glob, a relative
/// `-i`/`-F`/`-E` path, or a relative path / local command in a file- or command-valued `-o`.
/// Conservative: anything unparseable counts as sensitive.
pub fn is_cwd_sensitive(command: &str) -> bool {
    if command.contains(['*', '?', '[']) {
        return true;
    }
    let Ok(tokens) = shell_words::split(command) else {
        return true;
    };
    let mut tokens = tokens.iter().skip(1);
    while let Some(token) = tokens.next() {
        let Some(flags) = token.strip_prefix('-') else {
            continue;
        };
        if flags.starts_with('-') {
            continue;
        }
        for (index, flag) in flags.char_indices() {
            let takes_path = PATH_ARG_FLAGS.contains(&flag);
            let takes_option = flag == 'o';
            if !takes_path && !takes_option && !OTHER_ARG_FLAGS.contains(&flag) {
                continue;
            }
            // The argument is glued to the flag (`-i./key`) or is the next token.
            let glued = &flags[index + flag.len_utf8()..];
            let argument = if glued.is_empty() {
                tokens.next().map(String::as_str)
            } else {
                Some(glued)
            };
            let Some(argument) = argument else {
                return true;
            };
            if (takes_path && is_relative_path_arg(argument))
                || (takes_option && is_cwd_sensitive_option(argument))
            {
                return true;
            }
            break;
        }
    }
    false
}

/// What happens after a split replayed an ssh command, as a small state machine the new pane's
/// `TerminalView` drives (#1069):
///
/// 1. [`Self::arm`] when the split is created.
/// 2. [`Self::on_ssh_session_bootstrapped`] when an ssh session bootstraps in the pane: the
///    one-shot agent-view entry is reported, and the remote `cd` is prepared, keyed to that
///    session, only if it is the host the directory came from.
/// 3. [`Self::take_cd_for_bootstrap_precmd`] at that session's bootstrap precmd (its bootstrap
///    block completing): the `cd` is
///    released only while that same session is active and the input is untouched.
///
/// [`Self::on_user_block_completed`] disarms everything still waiting for a bootstrap: a user
/// block completing before the ssh session bootstrapped means the replayed ssh failed or exited
/// (it is the pane's first user command), so a later, unrelated ssh to the same host must not
/// inherit the stale `cd` or agent-view entry.
#[derive(Debug, Default)]
pub struct SplitFollowUp {
    pending_cd: Option<PendingRemoteCd>,
    enter_agent_view: bool,
    ready_cd: Option<(SessionId, String)>,
}

impl SplitFollowUp {
    pub fn arm(&mut self, pending_cd: Option<PendingRemoteCd>, enter_agent_view: bool) {
        self.pending_cd = pending_cd;
        self.enter_agent_view = enter_agent_view;
        self.ready_cd = None;
    }

    /// An ssh session bootstrapped. Returns whether to enter agent view now (one-shot).
    pub fn on_ssh_session_bootstrapped(
        &mut self,
        session_id: SessionId,
        session_host: &str,
        shell_type: ShellType,
    ) -> bool {
        if let Some(pending) = self.pending_cd.take() {
            self.ready_cd = remote_cd_command(&pending, session_host, shell_type, true)
                .map(|cd| (session_id, cd));
        }
        std::mem::take(&mut self.enter_agent_view)
    }

    /// The bootstrap precmd of the session that is now active. Returns the `cd` to run now, if
    /// it was prepared for this very session and the user has not started typing. One-shot:
    /// whatever was prepared is dropped either way.
    pub fn take_cd_for_bootstrap_precmd(
        &mut self,
        active_session_id: Option<SessionId>,
        input_is_empty: bool,
    ) -> Option<String> {
        let (session_id, cd) = self.ready_cd.take()?;
        (input_is_empty && active_session_id == Some(session_id)).then_some(cd)
    }

    /// Whether anything is still waiting: a `cd`, or the agent-view entry.
    pub fn is_armed(&self) -> bool {
        self.pending_cd.is_some() || self.enter_agent_view || self.ready_cd.is_some()
    }

    /// Whether a remote `cd` is prepared and waiting for its session's bootstrap precmd.
    pub fn has_ready_cd(&self) -> bool {
        self.ready_cd.is_some()
    }

    /// A user block completed in the pane.
    pub fn on_user_block_completed(&mut self) {
        self.pending_cd = None;
        self.enter_agent_view = false;
    }
}

#[cfg(test)]
#[path = "ssh_split_tests.rs"]
mod tests;
