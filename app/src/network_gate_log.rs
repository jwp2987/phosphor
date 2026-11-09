//! The in-memory log of network gate decisions (Settings > Network > "Network gate" list, and
//! the denial notice).
//!
//! The gate (`crates/network_gate`) reports every denial, every gate-off pass and every
//! allowed non-loopback request to a listener, from whatever thread made the request. This
//! singleton installs that listener, moves the events onto the UI thread through an unbounded
//! channel, and keeps the newest [`CAPACITY`] of them.
//!
//! Privacy: the log holds the HOST of each request so the user can press "Allow". It lives in
//! memory only. It is never serialised, never written to the app log (which only ever sees the
//! host hash), never put in a diagnostics bundle and never uploaded. Do not add a `Serialize`
//! impl or a log line that prints an entry's host.
//!
//! No timers: events arrive through the channel, and the time shown for each is stamped when
//! it is received.

use std::collections::VecDeque;

use chrono::{DateTime, Local};
use http_client::gate::{GateEvent, Outcome};
use warpui::{Entity, ModelContext, SingletonEntity};

/// How many decisions are kept; the oldest go first.
pub const CAPACITY: usize = 200;

/// One logged decision plus the time it reached the UI thread.
#[derive(Clone, Debug)]
pub struct LoggedGateEvent {
    pub event: GateEvent,
    pub at: DateTime<Local>,
}

/// The bounded list, separate from the model so it is testable without an app.
#[derive(Debug, Default)]
pub(crate) struct GateLogEntries {
    entries: VecDeque<LoggedGateEvent>,
}

impl GateLogEntries {
    pub(crate) fn push(&mut self, entry: LoggedGateEvent) {
        if self.entries.len() == CAPACITY {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }

    /// Oldest first.
    pub(crate) fn iter(&self) -> impl DoubleEndedIterator<Item = &LoggedGateEvent> {
        self.entries.iter()
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

#[derive(Clone, Debug)]
pub enum NetworkGateLogEvent {
    /// The list changed (an entry was added or the log was cleared).
    Changed,
    /// A request was refused. Emitted after `Changed`, for the denial notice.
    Denied(GateEvent),
}

pub struct NetworkGateLog {
    entries: GateLogEntries,
}

impl NetworkGateLog {
    pub fn new(ctx: &mut ModelContext<Self>) -> Self {
        let (tx, rx) = futures::channel::mpsc::unbounded::<GateEvent>();
        http_client::gate::set_event_listener(move |event| {
            // The receiver only goes away at shutdown; a lost event then is fine.
            let _ = tx.unbounded_send(event.clone());
        });
        let _ = ctx.spawn_stream_local(
            rx,
            |me: &mut Self, event, ctx| me.record(event, ctx),
            |_, _| {},
        );
        Self {
            entries: GateLogEntries::default(),
        }
    }

    fn record(&mut self, event: GateEvent, ctx: &mut ModelContext<Self>) {
        self.entries.push(LoggedGateEvent {
            event: event.clone(),
            at: Local::now(),
        });
        ctx.emit(NetworkGateLogEvent::Changed);
        if event.outcome == Outcome::Denied {
            ctx.emit(NetworkGateLogEvent::Denied(event));
        }
        ctx.notify();
    }

    /// Newest first, as the page shows them.
    pub fn entries_newest_first(&self) -> Vec<LoggedGateEvent> {
        self.entries.iter().rev().cloned().collect()
    }

    pub fn clear(&mut self, ctx: &mut ModelContext<Self>) {
        self.entries.clear();
        ctx.emit(NetworkGateLogEvent::Changed);
        ctx.notify();
    }
}

impl Entity for NetworkGateLog {
    type Event = NetworkGateLogEvent;
}

impl SingletonEntity for NetworkGateLog {}

#[cfg(test)]
#[path = "network_gate_log_tests.rs"]
mod tests;
