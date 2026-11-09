//! A server core driven by hand with its real event channel and real filesystem watching, for the
//! tests of the families that push (diff state, git status, GitHub info).

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use crate::proto::{ClientMessage, ServerMessage, host_scoped_request, session_scoped_request};

use super::server::{ConnId, Event, GRACE_PERIOD, Server, next_conn_id};

pub(super) struct Live {
    pub server: Server,
    events: Receiver<Event>,
    rx: async_channel::Receiver<ServerMessage>,
    pub conn: ConnId,
}

impl Live {
    pub(super) fn new() -> Self {
        let (events_tx, events) = std::sync::mpsc::channel();
        let mut server = Server::new(GRACE_PERIOD).with_events(events_tx);
        let (tx, rx) = async_channel::unbounded();
        let conn = next_conn_id();
        server.handle_event(Event::Connected {
            conn,
            tx,
            chunking: Arc::new(AtomicBool::new(false)),
        });
        Self {
            server,
            events,
            rx,
            conn,
        }
    }

    pub(super) fn send(&mut self, msg: ClientMessage) {
        self.server.handle_message(self.conn, msg);
    }

    pub(super) fn send_session(&mut self, id: &str, m: session_scoped_request::Message) {
        self.send(ClientMessage::session_scoped(id.to_string(), m));
    }

    #[allow(dead_code)]
    pub(super) fn send_host(&mut self, id: &str, m: host_scoped_request::Message) {
        self.send(ClientMessage::host_scoped(id.to_string(), m));
    }

    pub(super) fn notify(&mut self, m: crate::proto::notification::Message) {
        self.send(ClientMessage::notification(m));
    }

    /// The next message for the connection, running the core's events while waiting. `None` after
    /// `within`.
    pub(super) fn next(&mut self, within: Duration) -> Option<ServerMessage> {
        let deadline = Instant::now() + within;
        loop {
            if let Ok(m) = self.rx.try_recv() {
                return Some(m);
            }
            let left = deadline.checked_duration_since(Instant::now())?;
            match self
                .events
                .recv_timeout(left.min(Duration::from_millis(50)))
            {
                Ok(event) => self.server.handle_event(event),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return None,
            }
        }
    }

    /// The next message, panicking after ten seconds.
    pub(super) fn expect(&mut self) -> ServerMessage {
        self.next(Duration::from_secs(10))
            .expect("timed out waiting for a message")
    }

    /// Skips messages until `pick` accepts one (a push can be preceded by others).
    pub(super) fn expect_where<T>(
        &mut self,
        mut pick: impl FnMut(&ServerMessage) -> Option<T>,
    ) -> T {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let left = deadline
                .checked_duration_since(Instant::now())
                .expect("timed out waiting for the expected message");
            let m = self
                .next(left)
                .expect("timed out waiting for the expected message");
            if let Some(t) = pick(&m) {
                return t;
            }
        }
    }

    /// Whether no message arrives within `quiet` (events keep being processed meanwhile).
    pub(super) fn stays_quiet(&mut self, quiet: Duration) -> bool {
        self.next(quiet).is_none()
    }
}
