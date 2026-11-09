//! Replies and cancellable in-flight work for the helper's request handlers.
//!
//! The server core owns all state on one thread; work that can block (running a command,
//! searching a tree, git) runs on a worker thread that holds a [`Reply`] for its request and a
//! [`Cancel`] registered in [`Inflight`]. `Abort` flips the flag (and kills a spawned process),
//! after which the worker's result is dropped instead of sent.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Mutex};

use crate::proto::{ErrorCode, ErrorResponse, ServerMessage, server_message};

/// Sends responses for one request on the connection that made it.
#[derive(Clone)]
pub(super) struct Reply {
    tx: async_channel::Sender<ServerMessage>,
    request_id: String,
}

impl Reply {
    /// The id of the request this replies to (for `Abort` of a request that joined another's work).
    pub(super) fn request_id(&self) -> &str {
        &self.request_id
    }

    pub(super) fn new(tx: async_channel::Sender<ServerMessage>, request_id: String) -> Self {
        Self { tx, request_id }
    }

    /// Queues `message` for the connection's writer. A closed connection drops it silently.
    pub(super) fn send(&self, message: server_message::Message) {
        let _ = self.tx.try_send(ServerMessage {
            request_id: self.request_id.clone(),
            message: Some(message),
        });
    }

    pub(super) fn error(&self, code: ErrorCode, message: impl Into<String>) {
        self.send(server_message::Message::Error(ErrorResponse {
            code: code.into(),
            message: message.into(),
        }));
    }
}

/// Runs `work` on its own named thread (the git-derived views use one per recompute). A failure to
/// start the thread is logged and the work is dropped.
pub(super) fn spawn_worker(name: &'static str, work: impl FnOnce() + Send + 'static) -> bool {
    match std::thread::Builder::new().name(name.into()).spawn(work) {
        Ok(_) => true,
        Err(e) => {
            log::error!("could not start a worker thread: {e}");
            false
        }
    }
}

/// Cancellation state of one in-flight request.
#[derive(Default)]
pub(super) struct Cancel {
    cancelled: AtomicBool,
    /// Process group leader of a spawned child, 0 when none is running.
    pgid: AtomicI32,
}

impl Cancel {
    pub(super) fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    /// Records the child to kill on cancellation. If cancellation already happened the child is
    /// killed at once, so a request aborted before its process started does not leave one behind.
    pub(super) fn set_child_group(&self, pgid: i32) {
        self.pgid.store(pgid, Ordering::Release);
        if self.is_cancelled() {
            kill_group(pgid);
        }
    }

    pub(super) fn clear_child_group(&self) {
        self.pgid.store(0, Ordering::Release);
    }

    pub(super) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        let pgid = self.pgid.load(Ordering::Acquire);
        if pgid > 0 {
            kill_group(pgid);
        }
    }
}

fn kill_group(pgid: i32) {
    // SAFETY: kill(2) with a negative pid signals a process group; an invalid group is ESRCH.
    unsafe {
        libc::kill(-pgid, libc::SIGKILL);
    }
}

/// Registry of in-flight requests by request id, shared with the worker threads.
#[derive(Clone, Default)]
pub(super) struct Inflight {
    jobs: Arc<Mutex<HashMap<String, Arc<Cancel>>>>,
}

impl Inflight {
    pub(super) fn register(&self, request_id: &str) -> Arc<Cancel> {
        let cancel = Arc::new(Cancel::default());
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.insert(request_id.to_string(), cancel.clone());
        }
        cancel
    }

    pub(super) fn finish(&self, request_id: &str) {
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.remove(request_id);
        }
    }

    /// Cancels `request_id`; true when it was in flight.
    pub(super) fn abort(&self, request_id: &str) -> bool {
        let cancel = self
            .jobs
            .lock()
            .ok()
            .and_then(|mut jobs| jobs.remove(request_id));
        match cancel {
            Some(cancel) => {
                cancel.cancel();
                true
            }
            None => false,
        }
    }

    /// Runs `work` on a new thread. Its message is sent unless the request was aborted meanwhile.
    pub(super) fn spawn(
        &self,
        reply: Reply,
        work: impl FnOnce(&Cancel) -> server_message::Message + Send + 'static,
    ) {
        let request_id = reply.request_id.clone();
        let cancel = self.register(&request_id);
        let inflight = self.clone();
        let spawned = std::thread::Builder::new()
            .name("remote-job".into())
            .spawn(move || {
                let message = work(&cancel);
                inflight.finish(&request_id);
                if cancel.is_cancelled() {
                    log::info!("Request cancelled (request_id={request_id})");
                } else {
                    reply.send(message);
                }
            });
        if let Err(e) = spawned {
            log::error!("could not start a worker thread: {e}");
        }
    }
}

#[cfg(test)]
#[path = "jobs_tests.rs"]
mod tests;
