use async_channel::Sender;

use crate::terminal::{
    model::{
        session::command_executor::{InBandCommand, InBandCommandCancelledEvent},
        tmux::commands::TmuxCommand,
    },
    shell::ShellType,
};

/// Set of events sent by command executors.
pub enum ExecutorCommandEvent {
    /// The command should be executed.
    ExecuteCommand {
        command: InBandCommand,
        /// A Sender that can be used to signal that the command has been cancelled.
        /// Lets us unblock the command in the executor.
        cancel_tx: Sender<InBandCommandCancelledEvent>,
    },
    ExecuteTmuxCommand(TmuxCommand),
    /// The command identified by `id` should be cancelled.
    CancelCommand {
        id: String,
    },
}

pub fn shell_escape_single_quotes(command: &str, shell_type: ShellType) -> String {
    match shell_type {
        ShellType::Fish => {
            // Inside fish single quotes both `\\` and `\'` are escapes, so backslashes must be
            // doubled before quotes are escaped. Escaping only the quote turned a trailing `\`
            // into `\'`, which swallowed the closing quote, and collapsed `\\` to `\` (#867).
            command.replace('\\', r"\\").replace('\'', r"\'")
        }
        ShellType::PowerShell => {
            // In powershell we escape single quotes using two single quotes ''
            command.replace('\'', "''")
        }
        // A nushell single-quoted string is literal and cannot contain a single quote at all, so
        // there is nothing to escape WITH. Nothing builds a single-quoted nushell word from
        // this: nushell values go through `shell_quote_arg` / `nu_raw_string`, and the commands
        // Phosphor runs for itself are written for `ShellType::executor_syntax` (bash).
        ShellType::Nu => command.to_owned(),
        _ => {
            // For Bash and Zsh, replace each single quote with a '"'"' sequence.
            // The first single quote completes the single quoted string to the left,
            // the next three characters: "'" evaluate to a literal single quote in
            // bash/zsh, and then the final single quote starts a new single-quoted
            // string to the right. Effectively, this concatenates the left
            // single-quoted string, a literal single quote char, and the right
            // single-quoted string.
            command.replace('\'', r#"'"'"'"#)
        }
    }
}

/// Quotes a single shell argument so it is passed as data instead of being
/// interpreted as shell syntax.
///
/// Use this for complete interpolated arguments in generated command strings,
/// not for fragments that intentionally contain operators, pipes, or flags.
///
/// A value with a control character is not single-quoted: quoting keeps it one word, but the
/// character would still be typed into the PTY raw, where the line editor acts on it (a CR
/// submits the line). Such a value is spelled with the shell's escape syntax instead (#867).
pub fn shell_quote_arg(value: &str, shell_type: ShellType) -> String {
    if value
        .chars()
        .any(warp_util::path::needs_control_char_quoting)
    {
        return warp_util::path::ShellDialect::from(shell_type)
            .escape(value)
            .into_owned();
    }
    if shell_type == ShellType::Nu {
        // A single quote cannot be written inside a nushell single-quoted string; the
        // double-quoted form escapes it (and anything else) and is always one word.
        return if value.contains('\'') {
            warp_util::path::nu_quoted_string(value)
        } else {
            format!("'{value}'")
        };
    }
    format!("'{}'", shell_escape_single_quotes(value, shell_type))
}

/// A nushell raw string literal for `value`: `r#'...'#`, with enough `#`s that no `'#` inside
/// `value` can end it early. Used to pass arbitrary text (a command) as one argument.
pub fn nu_raw_string(value: &str) -> String {
    let longest_hash_run = value
        .split(|c| c != '#')
        .map(str::len)
        .max()
        .unwrap_or_default();
    let hashes = "#".repeat(longest_hash_run + 1);
    format!("r{hashes}'{value}'{hashes}")
}

/// Single-quotes a word for a POSIX `sh` script that is itself passed as one argument, e.g.
/// `sh -c {shell_quote_arg(script)}`. Control characters stay raw here on purpose: `sh` (dash)
/// has no `$'...'`, and the outer [`shell_quote_arg`] escapes the whole script for the
/// session's shell, so nothing raw reaches the PTY. Never use this for a word typed directly.
pub fn sh_script_quote_arg(value: &str) -> String {
    format!("'{}'", shell_escape_single_quotes(value, ShellType::Bash))
}

#[cfg(test)]
#[path = "shared_tests.rs"]
mod tests;
