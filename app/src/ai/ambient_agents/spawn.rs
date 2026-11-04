//! Stream-based API for spawning and monitoring ambient agents.
#![cfg_attr(target_family = "wasm", expect(dead_code))]

use super::AmbientAgentTask;

/// Information about a session join link for an ambient agent task.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // persisted or derived enum
pub struct SessionJoinInfo {
    pub session_link: String,
}

impl SessionJoinInfo {
    #[allow(dead_code)] // used by sibling unused code or shared name
    pub fn from_task(task: &AmbientAgentTask) -> Option<Self> {
        // Prefer the server-provided session_link when available; it is a better signal
        // that a session-sharing link is ready to be shown to the user.
        if let Some(link) = task.session_link.as_ref().filter(|l| !l.is_empty()) {
            return Some(Self {
                session_link: link.to_string(),
            });
        }

        None
    }
}
