//! What a person is told when a click on Phosphorize waits behind a relay (#2142, owner decision
//! 2026-11-11). PURE: the texts, the two "is this note wanted" rules and the kill switch. The view
//! (`view.rs`) shows and removes the notes.
//!
//! Behind a relay (`sudo -s` with `use_pty`, ssh, docker, su) the readiness judge writes the
//! setup only at the first NEW prompt (`subshell_ready.rs`: `Wait(RelayNoPrompt)`; the relay hides
//! the inner shell's tty, so a prompt that was already on screen proves nothing), and a command
//! the user types before that cancels the attempt (R2, `UserDrove`). Both rules stay. Before this
//! note the click said "Phosphor setup starts when the shell shows its prompt." while the prompt
//! was on screen, and the cancel said nothing.
//!
//! Two notes, in the toast style every other setup notice uses, never a modal:
//! - [`ARMED_WAIT_TEXT`] when the click arms an attempt whose first look says
//!   `Wait(RelayNoPrompt)`. A state: it shows until the setup is written or the attempt ends.
//! - [`cancelled_text`] when the user's own command cancels such an attempt; the Phosphorize
//!   button is offered again so "try again" has something to act on.
//!
//! No timer of the input path is added, nothing is typed for the user, and nothing is read off
//! the screen: both notes follow the judge's own decisions.
//!
//! Kill switch: `PHOSPHOR_SUBSHELL_RELAY_NOTE=0` (also `off`, `false`, `no`): the click shows the
//! old toast, the cancel is silent and the button is not offered again.

use super::subshell_ready::WaitWhy;

/// Environment variable that switches both notes off.
pub(super) const RELAY_NOTE_ENV: &str = "PHOSPHOR_SUBSHELL_RELAY_NOTE";

/// The names the notes have in the `subshell_relay_note` hop line (`note=<name> event=...`).
pub(super) const ARMED_WAIT_NOTE: &str = "armed_wait";
pub(super) const CANCELLED_NOTE: &str = "cancelled";

/// The note of a click that waits behind a relay.
pub(super) const ARMED_WAIT_TEXT: &str = "Phosphor will set up this shell at the next empty \
     prompt: press Enter on an empty line. Running a command first cancels the setup.";

/// Whether the notes are on, from the switch's value.
pub(super) fn enabled_from_env(value: Option<&str>) -> bool {
    !matches!(
        value
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref(),
        Some("0" | "off" | "false" | "no")
    )
}

/// [`enabled_from_env`] for this process, read once.
pub(super) fn enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| enabled_from_env(std::env::var(RELAY_NOTE_ENV).ok().as_deref()))
}

/// The wait note is shown for a click on Phosphorize (`banner`) whose look said the setup waits
/// for a new prompt behind a relay. Every other wait keeps the old toast: a secret read, a
/// question, a program in front and a busy rc file are not ended by an Enter on an empty line.
pub(super) fn armed_wait_note_wanted(enabled: bool, banner: bool, wait: Option<WaitWhy>) -> bool {
    enabled && banner && wait == Some(WaitWhy::RelayNoPrompt)
}

/// The cancel note is shown when the user's own command cancelled an attempt that came from a
/// click on Phosphorize and waited behind a relay: the wait note was showing, or the tty fact at
/// the Enter was a relay (the click met another wait first, e.g. text already on the line).
/// Auto-setup and the rc-file trigger stay silent: the person asked for nothing there.
pub(super) fn cancelled_note_wanted(
    enabled: bool,
    banner: bool,
    wait_note_showing: bool,
    relay_at_enter: bool,
) -> bool {
    enabled && banner && (wait_note_showing || relay_at_enter)
}

/// The note of a cancel. `button_back`: the Phosphorize button is offered again (the command it
/// was offered for is known); `retry_key`: the key bound to it, as displayed. Without the button
/// the note names no way that does not exist.
pub(super) fn cancelled_text(button_back: bool, retry_key: Option<&str>) -> String {
    const CANCELLED: &str = "Phosphor setup cancelled: you ran a command before it started.";
    match (button_back, retry_key) {
        (true, Some(key)) => {
            format!("{CANCELLED} To try again, click Phosphorize subshell or press {key}.")
        }
        (true, None) => format!("{CANCELLED} To try again, click Phosphorize subshell."),
        (false, _) => format!("{CANCELLED} To try again, start the subshell again."),
    }
}

#[cfg(test)]
#[path = "subshell_relay_note_tests.rs"]
mod tests;
