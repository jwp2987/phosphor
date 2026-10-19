//! The message-bar item for the one-click misroute recovery (#835), shared by the terminal and
//! agent-view message bars.

use warpui::elements::MouseStateHandle;

use super::{Message, MessageItem};
use crate::ai::blocklist::misroute_recovery::MisrouteOffer;
use crate::terminal::input::InputAction;

/// Builds the clickable message for `offer`. Clicking dispatches the matching [`InputAction`],
/// which `Input` handles by consuming the offer and re-submitting the remembered text once.
pub fn misroute_recovery_message(offer: &MisrouteOffer, mouse_state: MouseStateHandle) -> Message {
    let (label, action) = match offer {
        MisrouteOffer::SendToAgent { .. } => (
            crate::t!("terminal-message-send-to-agent-instead"),
            InputAction::SendLastSubmitToAgent,
        ),
        MisrouteOffer::RunAsShell { .. } => (
            crate::t!("terminal-message-run-as-shell-instead"),
            InputAction::RunLastSubmitAsShell,
        ),
    };
    Message::new(vec![MessageItem::clickable(
        vec![MessageItem::text(label)],
        move |ctx| ctx.dispatch_typed_action(action.clone()),
        mouse_state,
    )])
}
