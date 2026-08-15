//! Typed classification of a CLI-agent `stop_failure` event's `error_type`.
//!
//! `error_type` rides the CLI-agent OSC 777 protocol (see
//! [`crate::cli_agent_protocol`]) as a free-form string, and it stays free-form
//! on the wire: producers already in the field spell whatever they like there
//! (`"rate_limit"` from Claude Code's `StopFailure` hook, `"error"` and
//! `"cancelled"` from this fork's own TUI publisher). Nothing in this module
//! changes a single byte of that wire format.
//!
//! What it does change is *where the spelling is written down*. Before, the
//! literal lived twice and independently — once in the producer
//! (`crates/warp_tui/src/cli_agent_osc_event_publisher.rs`) and once in
//! whichever consumer wanted to react to it — which is exactly the coupling
//! that makes "just match the string in the GUI" a bad idea: the GUI would be
//! pinned to one producer's spelling with nothing keeping the two in step.
//! Here the spelling is defined once, in the crate both sides already depend
//! on, and both sides refer to the *variant*.
//!
//! ## Why cancellation specifically
//!
//! A cancelled turn is not a failed turn. The user pressed Ctrl-C; nothing went
//! wrong. This fork's [`ConversationStatus`] has a distinct `Cancelled` state
//! (a neutral gray stop) precisely so a cancel does not have to borrow the red
//! error triangle, and a consumer that collapses every `stop_failure` into
//! `Error` throws that distinction away after the producer took the trouble to
//! send it. Refs #596.
//!
//! Everything that is *not* recognised here stays a failure. `Other` is the
//! safe default and the overwhelmingly common case: an unrecognised
//! classification means "some failure we have no opinion about", never "treat
//! it as benign".
//!
//! [`ConversationStatus`]: <app/src/ai/agent/conversation.rs>

/// The wire value a producer emits when a turn ended because the user
/// cancelled it. This is the spelling the pinned oracle emits at
/// `42effe840` (`crates/warp_tui/src/cli_agent_osc_event_publisher.rs`), kept
/// verbatim: the wire vocabulary is upstream's, and this fork does not invent
/// tokens in a namespace it does not own.
pub const CANCELLED: &str = "cancelled";

/// The US spelling, accepted on input only. Never emitted.
///
/// Producers of this protocol are third-party plugins written by people who did
/// not read this file, and one letter should not decide whether the user sees a
/// red error triangle for their own Ctrl-C. Accepting both costs one
/// comparison; being strict costs a wrong-looking status chip that nobody can
/// diagnose from the outside.
const CANCELLED_ALTERNATE_SPELLING: &str = "canceled";

/// A `stop_failure` payload's `error_type`, classified.
///
/// Borrows the wire string rather than owning it: this is consulted on status
/// transitions that feed the GUI status chip, and the common answer
/// ([`Self::Other`]) should not allocate to say "no opinion".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CLIAgentErrorType<'a> {
    /// The turn ended because the user cancelled it, not because it failed.
    Cancelled,
    /// Any other producer classification, passed through verbatim. Consumers
    /// must treat this as an unqualified failure — see the module docs.
    Other(&'a str),
}

impl<'a> CLIAgentErrorType<'a> {
    /// Classifies the raw `error_type` string from a `stop_failure` payload.
    ///
    /// Comparison is trimmed and ASCII-case-insensitive; see
    /// [`CANCELLED_ALTERNATE_SPELLING`] for why the match is deliberately not
    /// byte-exact. The *emitted* spelling is always exactly [`CANCELLED`].
    pub fn from_wire(value: &'a str) -> Self {
        let normalized = value.trim();
        if normalized.eq_ignore_ascii_case(CANCELLED)
            || normalized.eq_ignore_ascii_case(CANCELLED_ALTERNATE_SPELLING)
        {
            Self::Cancelled
        } else {
            Self::Other(value)
        }
    }

    /// The value to put on the wire for this classification.
    ///
    /// Round-trips [`Self::Other`] verbatim, and normalises [`Self::Cancelled`]
    /// to the canonical spelling regardless of which one was parsed, so a
    /// relay never launders a variant spelling onward as if it were canonical.
    pub fn as_wire_str(&self) -> &'a str {
        match self {
            Self::Cancelled => CANCELLED,
            Self::Other(value) => value,
        }
    }

    /// Whether this classification means "the user stopped it", as opposed to
    /// "it broke".
    pub fn is_cancellation(&self) -> bool {
        matches!(self, Self::Cancelled)
    }
}

#[cfg(test)]
#[path = "cli_agent_error_type_tests.rs"]
mod tests;
