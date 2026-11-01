//! What a user key needs to know about the terminal model, without taking the model's lock
//! (#1403).
//!
//! The pty reader holds the model's `FairMutex` while it parses, so a key that locked it waited for
//! the parser before its bytes reached the pty. A key now reads two things instead:
//!
//! * [`KeyFacts`], a snapshot of the few model facts the key path decides on (alt screen, agent
//!   control, the active block's state), stored by the model in a [`KeyFactsCell`] (one atomic) at
//!   the end of every parse and wherever the main thread changes what it derives from. A key that
//!   finds the model free (`try_lock`) reads the exact state and refreshes the cell; one that finds
//!   the parser inside the lock reads the cell, which is at most the chunk being parsed behind.
//! * [`ModelInbox`], where the key path leaves what it used to write into the model under the lock:
//!   the user wrote this block its first input, a ^C, the typed-ahead bytes whose echo must be
//!   told from output. The model applies the commands in order before it parses the next bytes,
//!   so an echo is never parsed before the record of the key that caused it (the key posts, then
//!   writes the pty; the reader reads, then locks, then applies, then parses).
//!
//! Both are leaf primitives: a few atomic operations or a `Mutex` held for a `Vec` push, never
//! across I/O, a parse or any other lock.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use parking_lot::Mutex;

use super::early_output::TypeaheadMode;
use super::terminal_model::TerminalModel;

/// The facts of the terminal model the user-key path decides on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyFacts {
    /// A full-screen program owns the screen: its keys never reach the shell's line.
    pub alt_screen: bool,
    /// The agent is in control of the active block's command (#901, #963).
    pub agent_in_control: bool,
    /// The active block has already received user input (the first key marks it once). Whether a
    /// block is long running is decided when the mark is applied, under the model's lock: it
    /// depends on a latch an outside event sets, which a snapshot would miss.
    pub received_user_input: bool,
    /// The active block is a bootstrapped, started, unfinished command with no agent metadata: a
    /// running command the typeahead hand-off does not own (#1089).
    pub command_not_owned_by_handoff_possible: bool,
    /// The shell reports its line to this terminal's own requests and this is not a shared-session
    /// viewer (#1089).
    pub shell_reports_line: bool,
}

const ALT_SCREEN: u32 = 1 << 0;
const AGENT_IN_CONTROL: u32 = 1 << 1;
const RECEIVED_USER_INPUT: u32 = 1 << 2;
const COMMAND_NOT_OWNED: u32 = 1 << 3;
const SHELL_REPORTS_LINE: u32 = 1 << 4;

impl KeyFacts {
    /// Reads the facts from the model (the model's lock is held by the caller).
    pub fn from_model(model: &TerminalModel) -> Self {
        let active_block = model.block_list().active_block();
        let shell_reports_line = model.block_list().early_output().typeahead_mode()
            == TypeaheadMode::ShellReported
            && !model.shared_session_status().is_viewer();
        Self {
            alt_screen: model.is_alt_screen_active(),
            agent_in_control: active_block.is_agent_in_control(),
            received_user_input: active_block.has_received_user_input(),
            command_not_owned_by_handoff_possible: active_block.bootstrap_stage().is_done()
                && active_block.started()
                && !active_block.finished()
                && active_block.agent_interaction_metadata().is_none(),
            shell_reports_line,
        }
    }

    fn to_bits(self) -> u32 {
        [
            (self.alt_screen, ALT_SCREEN),
            (self.agent_in_control, AGENT_IN_CONTROL),
            (self.received_user_input, RECEIVED_USER_INPUT),
            (
                self.command_not_owned_by_handoff_possible,
                COMMAND_NOT_OWNED,
            ),
            (self.shell_reports_line, SHELL_REPORTS_LINE),
        ]
        .into_iter()
        .fold(0, |bits, (set, bit)| if set { bits | bit } else { bits })
    }

    fn from_bits(bits: u32) -> Self {
        Self {
            alt_screen: bits & ALT_SCREEN != 0,
            agent_in_control: bits & AGENT_IN_CONTROL != 0,
            received_user_input: bits & RECEIVED_USER_INPUT != 0,
            command_not_owned_by_handoff_possible: bits & COMMAND_NOT_OWNED != 0,
            shell_reports_line: bits & SHELL_REPORTS_LINE != 0,
        }
    }

    /// Whether user keys about to be written to the pty reach a running command whose leftover
    /// input the shell will report, while the typeahead hand-off does not own that command
    /// (#1089): it owns only commands submitted from the input editor, so keys for anything else
    /// (a queued command, an env-var collection, a command running when the editor took input
    /// over) were written uncounted, and the legacy report could miss them. Agent-requested
    /// commands keep the legacy path (their typeahead is deliberately not inserted), as do
    /// shared-session viewers (no local pty controller counts their writes) and shells that
    /// cannot report their line.
    pub fn keys_reach_a_command_the_handoff_does_not_own(&self) -> bool {
        self.shell_reports_line && self.command_not_owned_by_handoff_possible
    }
}

/// The latest [`KeyFacts`], shared between the model (which stores) and the views (which load).
#[derive(Clone, Debug, Default)]
pub struct KeyFactsCell(Arc<AtomicU32>);

impl KeyFactsCell {
    pub fn store(&self, facts: KeyFacts) {
        self.0.store(facts.to_bits(), Ordering::Release);
    }

    pub fn load(&self) -> KeyFacts {
        KeyFacts::from_bits(self.0.load(Ordering::Acquire))
    }

    /// The key path marked the active block as having received input: later keys of this block
    /// do not post the mark again, before the model has applied the first.
    pub fn note_received_user_input(&self) {
        self.0.fetch_or(RECEIVED_USER_INPUT, Ordering::AcqRel);
    }
}

/// What the key path leaves for the model to apply under its own lock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelCommand {
    /// The user wrote to the active block's command: its first input (when `first_input`), and a
    /// ^C (when `ctrl_c`, #1178).
    UserWrote { first_input: bool, ctrl_c: bool },
    /// Bytes the app wrote to the pty as the user's typed-ahead keys: their echo is recorded so it
    /// can be told apart from output (#1033), or forgotten when a full-screen program reads them.
    TypedAhead(Vec<u8>),
}

/// The inbox of [`ModelCommand`]s: written by the main thread without the model's lock, drained by
/// whoever next holds the model (the pty reader before it parses, or the main thread when it has
/// the model anyway).
#[derive(Clone, Debug, Default)]
pub struct ModelInbox {
    commands: Arc<Mutex<Vec<ModelCommand>>>,
    pending: Arc<AtomicBool>,
}

impl ModelInbox {
    /// Leaves `command` for the model. Never blocks on the model.
    pub fn post(&self, command: ModelCommand) {
        self.commands.lock().push(command);
        self.pending.store(true, Ordering::Release);
    }

    /// Whether anything waits (one atomic load: the parse path calls this before every slice).
    pub fn has_pending(&self) -> bool {
        self.pending.load(Ordering::Acquire)
    }

    /// Takes everything posted so far, in order.
    pub fn take(&self) -> Vec<ModelCommand> {
        if !self.has_pending() {
            return Vec::new();
        }
        let mut commands = self.commands.lock();
        self.pending.store(false, Ordering::Release);
        std::mem::take(&mut *commands)
    }
}

#[cfg(test)]
#[path = "key_facts_tests.rs"]
mod tests;
