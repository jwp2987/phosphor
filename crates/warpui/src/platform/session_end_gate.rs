//! Platform-neutral pieces of the Windows session-end handling
//! (jwp2987/phosphor#773, #1075, #1103). Free of the `windows` crate so the decisions build and
//! are unit-tested on every platform; the Windows glue is
//! `windowing::winit::windows::session_end`.
//!
//! Two independent things live here:
//!
//! - [`pump_until`], the bounded wait that lets `WM_ENDSESSION(TRUE)` run the graceful shutdown
//!   before the window procedure returns (#1075).
//! - [`SessionEndGate`], which remembers that a session end is only being *queried* (or was
//!   just cancelled), so the shells Windows kills at query time do not close their panes and
//!   quit the app before the user has even chosen (#1103).

use std::time::Duration;

use instant::Instant;
use parking_lot::Mutex;

/// How a [`pump_until`] wait ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) enum PumpOutcome {
    Completed,
    TimedOut,
}

/// Runs `pump_once` (with the time left) until `is_done` or `deadline` has elapsed on `elapsed`.
/// `is_done` is checked before the deadline, so work that finished exactly at the deadline still
/// counts as completed, and `pump_once` always runs at least once when neither holds yet.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn pump_until(
    deadline: Duration,
    mut elapsed: impl FnMut() -> Duration,
    mut is_done: impl FnMut() -> bool,
    mut pump_once: impl FnMut(Duration),
) -> PumpOutcome {
    loop {
        if is_done() {
            return PumpOutcome::Completed;
        }
        let spent = elapsed();
        if spent >= deadline {
            return PumpOutcome::TimedOut;
        }
        pump_once(deadline - spent);
    }
}

/// How long after `WM_QUERYENDSESSION` with no `WM_ENDSESSION` the gate stops holding panes open.
/// Windows shows its "apps are preventing restart" screen until the user decides, which can take a
/// while, but not forever.
pub(crate) const QUERY_PENDING_MAX: Duration = Duration::from_secs(120);

/// How long after a cancelled session end (`WM_ENDSESSION(FALSE)`) a shell exit is still
/// attributed to it: the exits Windows caused are delivered to the UI thread asynchronously and
/// can land after the cancel.
pub(crate) const CANCEL_GRACE: Duration = Duration::from_secs(3);

#[derive(Debug, Default)]
pub(crate) struct SessionEndGate {
    /// When the latest `WM_QUERYENDSESSION` was answered, until a `WM_ENDSESSION` follows.
    query_pending_since: Option<Instant>,
    /// `WM_ENDSESSION(TRUE)` was seen: the process is going down.
    ending: bool,
    cancelled_at: Option<Instant>,
}

impl SessionEndGate {
    pub(crate) const fn new() -> Self {
        Self {
            query_pending_since: None,
            ending: false,
            cancelled_at: None,
        }
    }

    pub(crate) fn note_query(&mut self, now: Instant) {
        self.query_pending_since = Some(now);
        self.cancelled_at = None;
    }

    pub(crate) fn note_end_session(&mut self, session_ending: bool, now: Instant) {
        self.query_pending_since = None;
        if session_ending {
            self.ending = true;
        } else {
            self.cancelled_at = Some(now);
        }
    }

    /// Whether a shell that just exited should keep its pane (showing the "shell terminated"
    /// banner) instead of closing it. True from `WM_QUERYENDSESSION` until `WM_ENDSESSION`
    /// resolves it, for the rest of an ending session (so the session snapshot still holds the
    /// tabs), and briefly after a cancel.
    pub(crate) fn should_keep_exited_pane(&self, now: Instant) -> bool {
        self.ending
            || self
                .query_pending_since
                .is_some_and(|since| now.saturating_duration_since(since) < QUERY_PENDING_MAX)
            || self
                .cancelled_at
                .is_some_and(|at| now.saturating_duration_since(at) < CANCEL_GRACE)
    }
}

static GATE: Mutex<SessionEndGate> = Mutex::new(SessionEndGate::new());

/// Records a `WM_QUERYENDSESSION` (answered `TRUE`). UI thread.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn note_query() {
    GATE.lock().note_query(Instant::now());
}

/// Records a `WM_ENDSESSION`. UI thread.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn note_end_session(session_ending: bool) {
    GATE.lock().note_end_session(session_ending, Instant::now());
}

/// Whether a pane whose shell just exited should stay open because a Windows session end is
/// being queried, is under way, or was just cancelled (jwp2987/phosphor#1103). Always false on
/// platforms that never report a session end.
pub fn keep_exited_pane_open() -> bool {
    GATE.lock().should_keep_exited_pane(Instant::now())
}

#[cfg(test)]
#[path = "session_end_gate_tests.rs"]
mod tests;
