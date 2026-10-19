//! The message-bar item for the one-click misroute recovery (#835), shared by the terminal and
//! agent-view message bars.

use warpui::elements::MouseStateHandle;

use super::{Message, MessageItem};
use crate::ai::blocklist::misroute_recovery::MisrouteOffer;
use crate::terminal::input::InputAction;

/// Builds the clickable message for `offer`. Clicking dispatches the matching [`InputAction`],
/// which `Input` handles by consuming the offer and re-submitting the remembered text once.
pub fn misroute_recovery_message(offer: &MisrouteOffer, mouse_state: MouseStateHandle) -> Message {
    let (label, text, action) = match offer {
        MisrouteOffer::SendToAgent { text } => (
            crate::t!("terminal-message-send-to-agent-instead"),
            text,
            InputAction::SendLastSubmitToAgent,
        ),
        MisrouteOffer::RunAsShell { text } => (
            crate::t!("terminal-message-run-as-shell-instead"),
            text,
            InputAction::RunLastSubmitAsShell,
        ),
    };
    Message::new(vec![MessageItem::clickable(
        vec![
            MessageItem::text(label),
            MessageItem::text(format!(" ({})", preview(text))),
        ],
        move |ctx| ctx.dispatch_typed_action(action.clone()),
        mouse_state,
    )])
}

/// Maximum characters of the submitted text shown in the label.
const PREVIEW_MAX_CHARS: usize = 40;

/// A one-line preview of `text`, ellipsized past [`PREVIEW_MAX_CHARS`] characters.
fn preview(text: &str) -> String {
    let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= PREVIEW_MAX_CHARS {
        return one_line;
    }
    let mut out: String = one_line.chars().take(PREVIEW_MAX_CHARS - 1).collect();
    out.push('\u{2026}');
    out
}

#[cfg(test)]
mod tests {
    use super::{PREVIEW_MAX_CHARS, preview};

    #[test]
    fn short_text_is_shown_whole() {
        assert_eq!(preview("git status"), "git status");
    }

    #[test]
    fn long_text_is_ellipsized_to_the_limit() {
        let p = preview(&"x".repeat(100));
        assert_eq!(p.chars().count(), PREVIEW_MAX_CHARS);
        assert!(p.ends_with('\u{2026}'));
    }
}
