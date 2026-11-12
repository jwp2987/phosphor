//! #2172: an agent request the provider fails the same way, over and over, is not resent.
//!
//! The recovery budget (`RecoveryBudget`) bounds the retries of ONE request chain, and the
//! controller starts a fresh chain for several follow-ups. A provider that fails the very same
//! request identically every time (a proxy that cannot map the model's tool call and returns a
//! failed stream for the turn) was therefore asked again and again with the same messages, every
//! few seconds, while the turn looked hung.
//!
//! This is the part of the answer the controller can decide EXACTLY from facts, with no clock:
//! a request is identified by [`request_fingerprint`] (the ids of every message it carries) and
//! a failure by the error text. [`MAX_IDENTICAL_FAILURES`] failures of the same request with the
//! same error in a row and the automatic resend is refused: the turn ends with the provider's
//! message on screen, and the user's next message is a different request, which is sent.

use std::hash::{Hash, Hasher};

/// How many times in a row the same request may fail with the same error before an automatic
/// resend of it is refused.
pub const MAX_IDENTICAL_FAILURES: u8 = 2;

/// A stable identity for a request: the ids of all messages it carries, in order. Two requests
/// with the same fingerprint carry the same conversation.
pub fn request_fingerprint<'a>(message_ids: impl IntoIterator<Item = &'a str>) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for id in message_ids {
        id.hash(&mut hasher);
    }
    hasher.finish()
}

/// The run of identical failures seen for one conversation.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FailureStreak {
    last: Option<(u64, String)>,
    count: u8,
}

impl FailureStreak {
    /// A request with `fingerprint` just failed with `error`.
    pub fn record(&mut self, fingerprint: u64, error: &str) {
        match &self.last {
            Some((last_fingerprint, last_error))
                if *last_fingerprint == fingerprint && last_error == error =>
            {
                self.count = self.count.saturating_add(1);
            }
            _ => {
                self.last = Some((fingerprint, error.to_owned()));
                self.count = 1;
            }
        }
    }

    /// The error to show when an automatic resend of the request `fingerprint` is refused:
    /// that request already failed [`MAX_IDENTICAL_FAILURES`] times in a row with this error.
    pub fn refusal(&self, fingerprint: u64) -> Option<&str> {
        match &self.last {
            Some((last_fingerprint, error))
                if *last_fingerprint == fingerprint && self.count >= MAX_IDENTICAL_FAILURES =>
            {
                Some(error.as_str())
            }
            _ => None,
        }
    }

    /// How many identical failures are on record.
    pub fn count(&self) -> u8 {
        self.count
    }
}

/// The text the turn ends with when a resend is refused. Names the provider's message as it
/// came; the tool the provider named is part of that message.
pub fn refusal_reason(error: &str, failures: u8) -> String {
    format!(
        "the provider failed this same request {failures} times in a row with the same \
         error, so it is not sent again. Provider message: {error}. Send a message to \
         retry."
    )
}

#[cfg(test)]
#[path = "repeated_failure_tests.rs"]
mod tests;
