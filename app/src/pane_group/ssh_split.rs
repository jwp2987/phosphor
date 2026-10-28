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
//! Only the pure selection logic lives here; the pane wiring is in `PaneGroup::add_split_session`
//! and `TerminalView::set_cd_after_ssh_bootstrap`.
//!
//! Ported by content from TranscriptionFactory/warp `6f7be0287`, `5cf24a5de` and `3ef7cf928`,
//! with a stricter replay gate (see [`replayable_ssh_command`]).

use crate::terminal::model::session::SessionType;
use crate::terminal::model::session::command_executor::shell_quote_arg;
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
        None => legacy_ssh_spawning_command
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

#[cfg(test)]
#[path = "ssh_split_tests.rs"]
mod tests;
