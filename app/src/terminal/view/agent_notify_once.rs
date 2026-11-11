//! One desktop notification per agent episode (#2140).
//!
//! The view is told about one change of a conversation by more than one event. A question (or a
//! permission request) that blocks the agent arrives as the conversation's status change
//! (`BlocklistAIHistoryEvent::UpdatedConversationStatus`) AND as the AI block's
//! `ActionBlockedOnUserConfirmation`; a finished turn arrives as the status change AND as the AI
//! block's `Finished`. Each of them asked for a desktop notification, so a person who had another
//! window in front got two identical notifications for one question (round 15, Wayland: two
//! `hop=desktop_notification` lines 56 microseconds apart). Upstream at the pin has the same three
//! call sites.
//!
//! An episode is what a person is told about once: a conversation that waits on one action, or one
//! turn that ended. The view asks [`NotifiedEpisodes::should_send`] before it notifies; an event
//! that names the episode already notified for is not sent again. A status that sends no
//! notification (in progress, cancelled, waiting, a transient error) ends the conversation's
//! episode, so whatever blocks or ends it next is told again even when it names the same action
//! or exchange.
//!
//! Kill switch: `PHOSPHOR_AGENT_NOTIFY_EVERY_EVENT=1` sends for every event, as before.

use std::collections::HashMap;
use std::hash::Hash;

use crate::ai::agent::conversation::ConversationStatus;

/// The statuses of a conversation a desktop notification is sent for: it waits on the user, or
/// its turn ended in success or in an error. Every other status ends the episode notified for
/// ([`NotifiedEpisodes::episode_ended`]).
pub(super) fn status_notifies(status: &ConversationStatus) -> bool {
    matches!(
        status,
        ConversationStatus::Success
            | ConversationStatus::Blocked { .. }
            | ConversationStatus::Error
    )
}

/// Environment variable that, set to `1`, restores one notification per event.
pub(super) const EVERY_EVENT_ENV: &str = "PHOSPHOR_AGENT_NOTIFY_EVERY_EVENT";

/// Whether the kill switch is on (read once per process).
pub(super) fn every_event() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| every_event_from(std::env::var(EVERY_EVENT_ENV).ok().as_deref()))
}

/// [`every_event`] with the variable's value passed in (tests).
pub(super) fn every_event_from(value: Option<&str>) -> bool {
    value.is_some_and(|value| value.trim() == "1")
}

/// What one notification is about, within one conversation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Episode {
    /// The name of the status that notifies (`blocked`, `success`, `error`): a fixed word.
    pub(super) state: &'static str,
    /// What tells this episode from the conversation's next one in the same state: the id of the
    /// action the agent waits on, or of the last exchange of the turn that ended. `None` when the
    /// conversation has neither (the status alone is the episode then).
    pub(super) marker: Option<String>,
}

/// The episode last notified for, per conversation (`C`: the conversation's id).
#[derive(Debug)]
pub(super) struct NotifiedEpisodes<C> {
    last: HashMap<C, Episode>,
}

impl<C> Default for NotifiedEpisodes<C> {
    fn default() -> Self {
        Self {
            last: HashMap::new(),
        }
    }
}

impl<C: Copy + Eq + Hash> NotifiedEpisodes<C> {
    /// Whether a notification for `episode` of `conversation` is to be sent now. `true` records
    /// it as sent; `false` means this very episode was notified for already. With `every_event`
    /// (the kill switch) every call is `true`.
    pub(super) fn should_send(
        &mut self,
        conversation: C,
        episode: Episode,
        every_event: bool,
    ) -> bool {
        if !every_event && self.last.get(&conversation) == Some(&episode) {
            return false;
        }
        self.last.insert(conversation, episode);
        true
    }

    /// The conversation reached a status that sends no notification: its episode is over.
    pub(super) fn episode_ended(&mut self, conversation: &C) {
        self.last.remove(conversation);
    }

    #[cfg(test)]
    pub(super) fn remembered(&self) -> usize {
        self.last.len()
    }
}

#[cfg(test)]
#[path = "agent_notify_once_tests.rs"]
mod tests;
