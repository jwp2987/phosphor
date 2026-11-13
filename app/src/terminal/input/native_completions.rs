//! When Tab asks the shell itself for completions, in-band (#1801).
//!
//! A Phosphorized remote session without the remote helper (absent, declined, failed or lost)
//! is the owner's primary remote design. Its side commands go through the ControlMaster, a
//! second ssh channel per command that a slow or lossy link (a VPN) makes slow or breaks, and
//! the completer's path and generator completions depend on them. The remote shell, however,
//! is right there on the session's own connection and knows its completions exactly
//! (bash-completion on bash 3.2+, zsh compsys, fish `complete -C` since #2196; not nu, see
//! `ShellType::supports_native_shell_completions`). So an explicit Tab in such a session asks
//! the shell first, through the `^Y` handshake the bootstrap installs, and falls back to the
//! completer only when the shell offers nothing.
//!
//! Only an explicit Tab: the completions-while-typing menu runs per keystroke burst and must
//! never put a request on the session's PTY for each one.

use crate::terminal::input::CompletionsTrigger;
use crate::terminal::model::completions::ShellCompletion;
use std::sync::OnceLock;
use warp_completer::completer::{MatchStrategy, SuggestionResults};

/// How a completions run uses the shell's own (native) completions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeCompletionsUse {
    /// The shell is not asked.
    None,
    /// The shell is asked only when the completer has nothing (upstream's
    /// `NativeShellCompletions` flag, or the `ForceNativeShellCompletions` preference).
    Fallback,
    /// The shell is asked first; the completer runs only when the shell offers nothing (an
    /// explicit Tab in a remote session without the helper, #1801).
    Preferred,
}

/// The inputs of [`native_completions_use`], named so a call site cannot swap two booleans.
#[derive(Clone, Copy, Debug)]
pub(crate) struct NativeCompletionsInputs {
    pub trigger: CompletionsTrigger,
    /// The session's shell answers the `^Y` handshake (zsh, bash, fish).
    pub shell_supports_native: bool,
    /// The text to complete spans lines; never sent to the shell.
    pub multi_line: bool,
    /// The input is natural language for the agent, not a shell command.
    pub is_ai_input: bool,
    /// A Phosphorized remote session whose side commands do not go through the helper.
    pub remote_without_helper: bool,
    /// The upstream flag or the force preference is on.
    pub flag_or_forced: bool,
    /// The #1801 kill switch (`PHOSPHOR_REMOTE_NATIVE_COMPLETIONS=0` turns it off).
    pub remote_native_enabled: bool,
}

pub(crate) fn native_completions_use(inputs: NativeCompletionsInputs) -> NativeCompletionsUse {
    if !inputs.shell_supports_native || inputs.multi_line {
        return NativeCompletionsUse::None;
    }
    if inputs.remote_without_helper
        && inputs.remote_native_enabled
        && inputs.trigger == CompletionsTrigger::Keybinding
        && !inputs.is_ai_input
    {
        return NativeCompletionsUse::Preferred;
    }
    if inputs.flag_or_forced {
        return NativeCompletionsUse::Fallback;
    }
    NativeCompletionsUse::None
}

/// `PHOSPHOR_REMOTE_NATIVE_COMPLETIONS`: unset or anything but `0` keeps the #1801 path on.
pub(crate) fn remote_native_completions_enabled_from_env(value: Option<&str>) -> bool {
    value.map(str::trim) != Some("0")
}

/// The #1801 kill switch, read once per process.
pub(crate) fn remote_native_completions_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        remote_native_completions_enabled_from_env(
            std::env::var("PHOSPHOR_REMOTE_NATIVE_COMPLETIONS")
                .ok()
                .as_deref(),
        )
    })
}

/// The shell's completions as the menu's results. Every shell's hook writes each candidate as
/// the whole whitespace-delimited token it replaces (zsh with its ignored and hidden prefixes,
/// bash with the part of the token before its last word break, fish as `complete -C` gives
/// it), so the replacement span is the token before the cursor.
pub(crate) fn native_suggestion_results(
    results: Vec<ShellCompletion>,
    buffer_text: &str,
    cursor_position: usize,
) -> SuggestionResults {
    let suggestions = results.into_iter().map(Into::into).collect();
    let token_end = cursor_position.min(buffer_text.len());
    let token_start = buffer_text
        .get(0..token_end)
        // Find the last whitespace char before the token end; the token starts after it.
        .and_then(|before| {
            before
                .char_indices()
                .rev()
                .find(|(_, c)| c.is_whitespace())
                .map(|(pos, c)| pos + c.len_utf8())
        })
        // Otherwise, the start is the beginning of the buffer.
        .unwrap_or_default();
    SuggestionResults {
        replacement_span: (token_start, token_end).into(),
        suggestions,
        match_strategy: MatchStrategy::Fuzzy,
    }
}

#[cfg(test)]
#[path = "native_completions_tests.rs"]
mod tests;
