//! One-click recovery when input auto-detection routes text to the wrong place (#835).
//!
//! [`MisrouteMemory`] remembers the last submitted text, where it went (shell or agent) and why
//! (autodetected, explicit or locked). From that it derives a [`MisrouteOffer`]:
//!
//! - text autodetected as Shell whose command then exited "command not found" can be sent to the
//!   agent instead;
//! - text autodetected as AI can be run as a shell command instead, while the agent turn is in
//!   progress or shortly after.
//!
//! Only autodetected routing ever produces an offer: the user made no choice, so the detector's
//! mistake is ours to repair. The memory is pure state with no view or model dependencies so the
//! state machine can be tested on its own.

use std::time::Duration;

use instant::Instant;
use warp_core::command::ExitCode;

use super::agent_view::AgentViewEntryOrigin;

/// How long an offer stays available: for "run as shell command instead", from the AI submit
/// (the offer is meant for the agent's response in progress or just after it); for "send to
/// agent instead", from the moment the command came back not found.
pub const OFFER_WINDOW: Duration = Duration::from_secs(120);

/// Whether an agent-view transition makes a remembered submit stale. The one transition that
/// does not is the entry the autodetected agent submit itself caused (it is recorded just before
/// that entry is announced); every other entry or exit -- slash commands included -- is a
/// different conversation context.
pub fn agent_view_entry_keeps_offer(origin: &AgentViewEntryOrigin) -> bool {
    matches!(
        origin,
        AgentViewEntryOrigin::Input {
            was_prompt_autodetected: true
        }
    )
}

/// Where a submit went.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SubmitRoute {
    Shell,
    Ai,
}

/// Why a submit went where it did.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RouteReason {
    /// Input autodetection chose the route.
    Autodetected,
    /// The user switched modes explicitly (toggle, `!`, slash command).
    Explicit,
    /// The input type was locked (including the one-shot recovery re-submits themselves, so a
    /// recovery is never offered again for its own result).
    Locked,
}

/// The recovery currently on offer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MisrouteOffer {
    /// Shell-routed text that was not a command: send it to the agent.
    SendToAgent { text: String },
    /// Agent-routed text that was a command: run it in the shell.
    RunAsShell { text: String },
}

#[derive(Clone, Debug)]
struct LastSubmit {
    text: String,
    route: SubmitRoute,
    reason: RouteReason,
    /// Exit code of the block the text produced, once it has finished. Shell route only.
    exit_code: Option<ExitCode>,
    /// When that block finished.
    finished_at: Option<Instant>,
    recorded_at: Instant,
}

/// The last submit and whether a recovery is still on offer for it.
#[derive(Clone, Debug, Default)]
pub struct MisrouteMemory {
    last: Option<LastSubmit>,
}

impl MisrouteMemory {
    /// Remembers a submit, replacing (and so dismissing) whatever came before. Blank text is not
    /// remembered.
    pub fn record_submit(
        &mut self,
        text: &str,
        route: SubmitRoute,
        reason: RouteReason,
        now: Instant,
    ) {
        let text = text.trim();
        self.last = (!text.is_empty()).then(|| LastSubmit {
            text: text.to_owned(),
            route,
            reason,
            exit_code: None,
            finished_at: None,
            recorded_at: now,
        });
    }

    /// Notes that a user command finished. Only the remembered shell submit it belongs to (same
    /// text, no exit code yet) is updated. Returns whether the offer may have changed.
    pub fn note_command_finished(
        &mut self,
        command: &str,
        exit_code: ExitCode,
        now: Instant,
    ) -> bool {
        match self.last.as_mut() {
            Some(last)
                if last.route == SubmitRoute::Shell
                    && last.exit_code.is_none()
                    && last.text == command.trim() =>
            {
                last.exit_code = Some(exit_code);
                last.finished_at = Some(now);
                true
            }
            _ => false,
        }
    }

    /// Notes an edit that left the buffer in the given state. Typing anything new dismisses the
    /// offer. Returns whether there was something to dismiss.
    pub fn note_buffer_edited(&mut self, buffer_is_empty: bool) -> bool {
        if buffer_is_empty {
            return false;
        }
        self.clear()
    }

    /// Forgets the last submit. Returns whether there was one.
    pub fn clear(&mut self) -> bool {
        self.last.take().is_some()
    }

    /// The recovery on offer at `now`, if any.
    pub fn offer(&self, now: Instant) -> Option<MisrouteOffer> {
        let last = self.last.as_ref()?;
        if last.reason != RouteReason::Autodetected {
            return None;
        }
        match last.route {
            SubmitRoute::Shell => {
                let finished_at = last.finished_at?;
                (last.exit_code?.was_command_not_found()
                    && now.saturating_duration_since(finished_at) < OFFER_WINDOW)
                    .then(|| MisrouteOffer::SendToAgent {
                        text: last.text.clone(),
                    })
            }
            // A multi-line paste must not be one click from running as a script.
            SubmitRoute::Ai => (!last.text.contains('\n')
                && now.saturating_duration_since(last.recorded_at) < OFFER_WINDOW)
                .then(|| MisrouteOffer::RunAsShell {
                    text: last.text.clone(),
                }),
        }
    }

    /// When the offer on offer at `now` expires, so the UI can re-render then.
    pub fn offer_expires_at(&self, now: Instant) -> Option<Instant> {
        self.offer(now)?;
        let last = self.last.as_ref()?;
        Some(last.finished_at.unwrap_or(last.recorded_at) + OFFER_WINDOW)
    }

    /// Consumes the offer: returns it (if one is on offer at `now`) and forgets the submit so the
    /// recovery fires once.
    pub fn take_offer(&mut self, now: Instant) -> Option<MisrouteOffer> {
        let offer = self.offer(now);
        if offer.is_some() {
            self.last = None;
        }
        offer
    }
}

#[cfg(test)]
#[path = "misroute_recovery_tests.rs"]
mod tests;
