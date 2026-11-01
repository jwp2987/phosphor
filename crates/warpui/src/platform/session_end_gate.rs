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

/// The gate's state. Shell exits caused by a session end are attributed by *ordering*, not by
/// elapsed time: a shell whose process died while a session end was pending or under way (seen
/// on the exit watcher thread, at the instant the process died, via [`note_shell_exit`]) is
/// counted in `session_exits`; the UI thread consumes one count per `Exited` it handles. After a
/// cancel, only exits that were counted keep their pane, however late they are delivered, and an
/// ordinary exit (nothing counted) closes its pane at once. There is no OS signal that names the
/// killer: a console shell ended by CTRL_LOGOFF/SHUTDOWN_EVENT may exit 0xC000013A
/// (STATUS_CONTROL_C_EXIT) or with whatever its own handler returns, so the exit code is only
/// logged, not relied on.
#[derive(Debug, Default)]
pub(crate) struct SessionEndGate {
    /// When the latest `WM_QUERYENDSESSION` was answered, until a `WM_ENDSESSION` follows.
    /// Expiry after [`QUERY_PENDING_MAX`] is a give-up net for a `WM_ENDSESSION` that never
    /// arrives.
    query_pending_since: Option<Instant>,
    /// `WM_ENDSESSION(TRUE)` was seen: the process is going down.
    ending: bool,
    /// `WM_ENDSESSION(FALSE)` was seen and no newer query has started.
    cancelled: bool,
    /// Shell exits observed while a session end was pending/ending that no pane has consumed yet.
    session_exits: u32,
}

impl SessionEndGate {
    pub(crate) const fn new() -> Self {
        Self {
            query_pending_since: None,
            ending: false,
            cancelled: false,
            session_exits: 0,
        }
    }

    fn session_end_active(&self, now: Instant) -> bool {
        self.ending
            || self
                .query_pending_since
                .is_some_and(|since| now.saturating_duration_since(since) < QUERY_PENDING_MAX)
    }

    pub(crate) fn note_query(&mut self, now: Instant) {
        self.query_pending_since = Some(now);
        self.cancelled = false;
        // Counts left from an earlier, resolved session end must not swallow a later exit.
        self.session_exits = 0;
    }

    pub(crate) fn note_end_session(&mut self, session_ending: bool, _now: Instant) {
        self.query_pending_since = None;
        if session_ending {
            self.ending = true;
        } else {
            self.cancelled = true;
        }
    }

    /// A shell process died (called from the exit watcher at that instant). Counted only when a
    /// session end is pending or under way right now: those are the shells Windows killed.
    pub(crate) fn note_shell_exit(&mut self, now: Instant) {
        if self.session_end_active(now) {
            self.session_exits = self.session_exits.saturating_add(1);
        }
    }

    /// Whether a shell that just exited should keep its pane (showing the "shell terminated"
    /// banner) instead of closing it. True from `WM_QUERYENDSESSION` until `WM_ENDSESSION`
    /// resolves it, for the rest of an ending session (so the session snapshot still holds the
    /// tabs), and, after a cancel, for each exit that was counted while the session end was
    /// pending. Consumes one counted exit per call.
    pub(crate) fn should_keep_exited_pane(&mut self, now: Instant) -> bool {
        let counted = self.session_exits > 0;
        self.session_exits = self.session_exits.saturating_sub(1);
        self.session_end_active(now) || (self.cancelled && counted)
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
/// being queried, is under way, or was cancelled with this shell already dead (jwp2987/phosphor#1103). Always false on
/// platforms that never report a session end.
pub fn keep_exited_pane_open() -> bool {
    GATE.lock().should_keep_exited_pane(Instant::now())
}

/// Records that a shell process died, at the instant it died (the exit watcher thread on
/// Windows). `exit_code` is only logged. Whether the exit belongs to a session end is decided by
/// the order of events (see [`SessionEndGate`]), not by a time window.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn note_shell_exit(exit_code: Option<u32>) {
    log::debug!("session_end_gate: shell exited, code={exit_code:?}");
    GATE.lock().note_shell_exit(Instant::now());
}

#[cfg(test)]
#[path = "session_end_gate_tests.rs"]
mod tests;
