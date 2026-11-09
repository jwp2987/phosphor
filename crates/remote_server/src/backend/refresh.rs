//! When a shell-backed view re-reads its state.
//!
//! With the helper, the host pushes changes (a watcher on the host). In shell mode nothing pushes,
//! so the app re-reads at the moments the state can have changed: the prompt came back, the cwd
//! moved, the user asked, or the app itself just wrote. [`RefreshCoordinator`] is the pure state
//! machine behind that, one per (session, repo, family): at most ONE refresh in flight, and a
//! `dirty` flag that queues exactly one more when events arrive meanwhile. There is NO timer: no
//! debounce, no polling. The caller starts a refresh when told to and calls
//! [`RefreshCoordinator::finished`] when it ends.

use crate::shellcmd::predicates::{
    command_may_change_cwd, command_may_change_git_state, command_may_change_head,
};

/// Something that can have changed the host's state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteRefreshEvent {
    /// The prompt came back (precmd hook). `last_cmd` is the command that just finished, if known.
    Precmd {
        cwd: String,
        last_cmd: Option<String>,
    },
    /// The shell's cwd changed (also reported without a prompt, e.g. by an OSC).
    CwdChanged { cwd: String },
    /// The user pressed refresh.
    UserAction,
    /// The app just wrote to the host itself (a commit, a stage, a save).
    AfterOwnWrite,
}

/// How eagerly a family reacts to a prompt coming back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrecmdPolicy {
    /// Every prompt: the family sees edits made by any command (diff state, the file tree).
    EveryPrompt,
    /// Only after commands that can have moved HEAD or touched git (git status, GitHub info).
    GitRelevant,
}

/// What the caller must do about an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefreshDecision {
    /// Start a refresh now and call [`RefreshCoordinator::finished`] when it ends.
    Start,
    /// A refresh is already running; one more will be started when it finishes.
    Coalesced,
    /// Nothing could have changed.
    Skip,
}

#[derive(Debug)]
pub struct RefreshCoordinator {
    policy: PrecmdPolicy,
    in_flight: bool,
    dirty: bool,
    last_cwd: Option<String>,
}

impl RefreshCoordinator {
    pub fn new(policy: PrecmdPolicy) -> Self {
        Self {
            policy,
            in_flight: false,
            dirty: false,
            last_cwd: None,
        }
    }

    pub fn in_flight(&self) -> bool {
        self.in_flight
    }

    pub fn on_event(&mut self, event: &RemoteRefreshEvent) -> RefreshDecision {
        let wants = match event {
            RemoteRefreshEvent::Precmd { cwd, last_cmd } => {
                let cwd_moved = self.note_cwd(cwd);
                match self.policy {
                    PrecmdPolicy::EveryPrompt => true,
                    PrecmdPolicy::GitRelevant => {
                        cwd_moved
                            || last_cmd.as_deref().is_none_or(|cmd| {
                                command_may_change_git_state(cmd)
                                    || command_may_change_head(cmd)
                                    || command_may_change_cwd(cmd)
                            })
                    }
                }
            }
            RemoteRefreshEvent::CwdChanged { cwd } => self.note_cwd(cwd),
            RemoteRefreshEvent::UserAction | RemoteRefreshEvent::AfterOwnWrite => true,
        };
        if !wants {
            RefreshDecision::Skip
        } else if self.in_flight {
            self.dirty = true;
            RefreshDecision::Coalesced
        } else {
            self.in_flight = true;
            RefreshDecision::Start
        }
    }

    /// The running refresh ended. Returns true when another must start now (events arrived while
    /// it ran); the coordinator is then in flight again.
    pub fn finished(&mut self) -> bool {
        if self.dirty {
            self.dirty = false;
            self.in_flight = true;
            true
        } else {
            self.in_flight = false;
            false
        }
    }

    /// Records the cwd; true when it differs from the last one seen.
    fn note_cwd(&mut self, cwd: &str) -> bool {
        if self.last_cwd.as_deref() == Some(cwd) {
            false
        } else {
            self.last_cwd = Some(cwd.to_owned());
            true
        }
    }
}

#[cfg(test)]
#[path = "refresh_tests.rs"]
mod tests;
