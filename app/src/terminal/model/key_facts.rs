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

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use chrono::{DateTime, Local};
use warpui_core::sync_shim::Mutex;
use warpui_core::sync_shim::atomic::{AtomicBool, AtomicU32, Ordering};

use super::block::interactive_latch::InteractiveLatch;
use super::block::interactive_prompt_candidate;
use super::early_output::TypeaheadMode;
use super::grid::grid_handler::TermMode;
use super::session::SessionId;
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

/// The rest of what a user key reads from the terminal model (#1404), published the same way as
/// [`KeyFacts`] but too big for one atomic: the ssh connect / subshell bootstrap bookkeeping, the
/// agent-owned-pty rule and the active block's interactive latch. A key that finds the model free
/// reads the exact state; one that finds the parser inside the lock reads this copy, at most the
/// chunk being parsed behind. The cell is a leaf lock held for one clone or one store.
#[derive(Clone, Debug, Default)]
pub struct KeyModelSnapshot {
    /// The session is read only (a finished viewer, a transcript).
    pub read_only: bool,
    /// A shared-session executor: it writes to the pty even before its bootstrap script echoed.
    pub shared_session_executor: bool,
    /// The remote or subshell session the shell announced and that is not bootstrapped yet.
    pub pending_session_id: Option<SessionId>,
    /// The SSH hook was seen for the pending connect.
    pub has_pending_ssh_session: bool,
    /// The pending session is a subshell (`su -`, `sudo -i`).
    pub has_pending_subshell: bool,
    pub active_block_bootstrapped: bool,
    pub active_block_started: bool,
    pub active_and_long_running: bool,
    /// A hash of the active block's id: compared with the hash of an id the view remembers, so
    /// "is this still the block I noted" needs no clone of the id on every key.
    pub active_block_hash: u64,
    pub active_block_start: Option<DateTime<Local>>,
    /// The agent, not the user, owns the running command's pty (#1179).
    pub agent_owns_pty: bool,
    /// The active block's interactive latch (cheap to clone), for the per-key probe.
    pub interactive_latch: Option<InteractiveLatch>,
    /// The program set application cursor keys mode (DECCKM): arrows are `ESC O A`, not `ESC [ A`
    /// (#1440: `terminal_up`/`down`, Home/End no longer lock the model for it).
    pub app_cursor: bool,
    /// What `TerminalView::is_input_box_visible` reads from the model (#1440).
    pub input_box: InputBoxFacts,
}

/// The cheap model facts behind "is the input box visible", so the key path can answer it from
/// the published snapshot when the pty reader holds the model (#1440), and the render path can
/// read the same fields from the model it already holds. No grid scans here: this is built on
/// every frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputBoxFacts {
    pub read_only: bool,
    pub alt_screen_active: bool,
    pub shared_session_view_pending: bool,
    pub pending_session_id: Option<SessionId>,
    pub block_agent_in_control: bool,
    pub block_agent_tagged_in: bool,
    pub block_agent_monitoring: bool,
    pub block_agent_blocked: bool,
    pub block_active_and_long_running: bool,
    pub bootstrapped: bool,
    pub writing_or_executing_in_band_command: bool,
}

impl InputBoxFacts {
    pub fn from_model(model: &TerminalModel) -> Self {
        let active_block = model.block_list().active_block();
        Self {
            read_only: model.is_read_only(),
            alt_screen_active: model.is_alt_screen_active(),
            shared_session_view_pending: model.shared_session_status().is_view_pending(),
            pending_session_id: model.pending_session_id(),
            block_agent_in_control: active_block.is_agent_in_control(),
            block_agent_tagged_in: active_block.is_agent_tagged_in(),
            block_agent_monitoring: active_block.is_agent_monitoring(),
            block_agent_blocked: active_block.is_agent_blocked(),
            block_active_and_long_running: active_block.is_active_and_long_running(),
            bootstrapped: model.block_list().is_bootstrapped(),
            writing_or_executing_in_band_command: model
                .block_list()
                .is_writing_or_executing_in_band_command(),
        }
    }
}

/// The hash [`KeyModelSnapshot::active_block_hash`] holds for a block id.
pub fn block_id_hash(id: &super::BlockId) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut hasher);
    hasher.finish()
}

impl KeyModelSnapshot {
    /// Reads the snapshot from the model (the model's lock is held by the caller).
    pub fn from_model(model: &TerminalModel) -> Self {
        let active_block = model.block_list().active_block();
        let agent_owns_pty = !model.is_alt_screen_active()
            && active_block.is_agent_driving_command()
            && interactive_prompt_candidate(active_block.output_grid().grid_handler()).is_none();
        Self {
            read_only: model.is_read_only(),
            shared_session_executor: model.shared_session_status().is_executor(),
            pending_session_id: model.pending_session_id(),
            has_pending_ssh_session: model.has_pending_ssh_session(),
            has_pending_subshell: model.pending_subshell_session().is_some(),
            active_block_bootstrapped: model.is_active_block_bootstrapped(),
            active_block_started: active_block.started(),
            active_and_long_running: active_block.is_active_and_long_running(),
            active_block_hash: block_id_hash(model.active_block_id()),
            active_block_start: active_block.start_ts().copied(),
            agent_owns_pty,
            interactive_latch: Some(active_block.interactive_latch().clone()),
            app_cursor: model.is_term_mode_set(TermMode::APP_CURSOR),
            input_box: InputBoxFacts::from_model(model),
        }
    }

    /// `is_long_running` of the view: a running command in a session the user can write to.
    pub fn is_long_running(&self) -> bool {
        self.active_and_long_running && !self.read_only
    }
}

/// The latest [`KeyModelSnapshot`], shared between the model (which stores) and the views (which
/// load).
#[derive(Clone, Debug, Default)]
pub struct KeyModelCell(Arc<Mutex<KeyModelSnapshot>>);

impl KeyModelCell {
    pub fn store(&self, snapshot: KeyModelSnapshot) {
        *self.0.lock() = snapshot;
    }

    pub fn load(&self) -> KeyModelSnapshot {
        self.0.lock().clone()
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
    /// Text the user typed that may turn out to be typeahead: the early-output model matches it
    /// against the characters the pty echoes (`TerminalModel::push_user_input`).
    UserInput(String),
    /// Bytes the user typed, for a shared session this pane drives to relay to its viewers
    /// (`TerminalModel::send_write_to_pty_events_for_shared_session`).
    SharedSessionWrite(Vec<u8>),
}

/// The most commands the inbox keeps (#1442). A key leaves at most ~3 (typed-ahead record, user
/// input, shared-session write), the model drains the inbox before every parse slice and the main
/// thread drains it whenever the model is free, so it only grows while the pty reader sits inside
/// the model's lock. 180 s of the fastest key repeat (~60 Hz) is ~33,000 commands; the bound is
/// eight times that. Past it data commands are dropped (counted, logged, reported to the user by
/// the view through [`ModelInbox::take_overflow_report`]); `UserWrote` flags are merged instead,
/// never dropped, so a ^C or the first input is never lost.
pub const MAX_INBOX_COMMANDS: usize = 256 * 1024;

/// The inbox of [`ModelCommand`]s: written by the main thread without the model's lock, drained by
/// whoever next holds the model (the pty reader before it parses, or the main thread when it has
/// the model anyway).
#[derive(Clone, Debug)]
pub struct ModelInbox {
    commands: Arc<Mutex<Vec<ModelCommand>>>,
    pending: Arc<AtomicBool>,
    /// Commands refused for the cap since the view last asked ([`Self::take_overflow_report`]).
    dropped: Arc<AtomicU32>,
    max_commands: usize,
}

impl Default for ModelInbox {
    fn default() -> Self {
        Self::with_capacity(MAX_INBOX_COMMANDS)
    }
}

impl ModelInbox {
    /// An inbox that keeps at most `max_commands` (tests use a small one).
    pub fn with_capacity(max_commands: usize) -> Self {
        Self {
            commands: Arc::new(Mutex::new(Vec::new())),
            pending: Arc::new(AtomicBool::new(false)),
            dropped: Arc::new(AtomicU32::new(0)),
            max_commands,
        }
    }

    /// Leaves `command` for the model. Never blocks on the model. Returns false when the inbox
    /// was full and the command was dropped (#1442).
    pub fn post(&self, command: ModelCommand) -> bool {
        let mut commands = self.commands.lock();
        if commands.len() >= self.max_commands {
            // The flags are tiny and must not be lost: fold them into the newest `UserWrote`.
            if let ModelCommand::UserWrote {
                first_input,
                ctrl_c,
            } = &command
            {
                if let Some(ModelCommand::UserWrote {
                    first_input: kept_first,
                    ctrl_c: kept_ctrl_c,
                }) = commands
                    .iter_mut()
                    .rev()
                    .find(|kept| matches!(kept, ModelCommand::UserWrote { .. }))
                {
                    *kept_first |= *first_input;
                    *kept_ctrl_c |= *ctrl_c;
                    return true;
                }
                commands.push(command);
                self.pending.store(true, Ordering::Release);
                return true;
            }
            let total = self.dropped.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
            if total == 1 {
                log::warn!(
                    "model inbox full ({} commands waiting for the pty reader): dropping typed-ahead bookkeeping",
                    commands.len()
                );
            }
            return false;
        }
        commands.push(command);
        self.pending.store(true, Ordering::Release);
        true
    }

    /// How many commands were dropped for the cap since the last call (0: none), for the view to
    /// tell the user once per episode.
    pub fn take_overflow_report(&self) -> u32 {
        self.dropped.swap(0, Ordering::Relaxed)
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

#[cfg(all(test, not(any(shuttle, loom))))]
#[path = "key_facts_tests.rs"]
mod tests;

// Interleaving tests under `RUSTFLAGS="--cfg shuttle"` / `"--cfg loom"` (see
// `warpui_core::sync_shim`); a model build compiles these instead of the ordinary tests.
#[cfg(all(test, any(shuttle, loom)))]
#[path = "key_facts_model_tests.rs"]
mod model_tests;
