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
use warpui_core::sync_shim::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use super::block::interactive_latch::InteractiveLatch;
use super::block::interactive_prompt_candidate;
use super::early_output::TypeaheadMode;
use super::escape_sequences::ModeProvider;
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
///
/// One `AtomicU64`: the low 32 bits are the fact bits, the high 32 bits the publication
/// generation (`facts_gen`, #1424) the facts belong to. They are read and written together, so a
/// reader never sees the bits of one publication under the gen of another. The same gen is stored
/// in the [`KeyModelSnapshot`] ([`KeyModelSnapshot::facts_gen`]); [`load_consistent`] retries
/// until the two cells agree. The cell only holds the low 32 bits of the model's `u64` gen, which
/// is enough to tell two publications apart (a reader compares for equality, never for order).
#[derive(Clone, Debug, Default)]
pub struct KeyFactsCell(Arc<AtomicU64>);

const GEN_SHIFT: u32 = 32;
const BITS_MASK: u64 = 0xFFFF_FFFF;

impl KeyFactsCell {
    /// Stores `facts` under the gen the cell already holds (a main-thread refresh that read the
    /// exact state: it must not claim a publication that did not happen, so no max-store and no
    /// bump).
    pub fn store(&self, facts: KeyFacts) {
        let bits = u64::from(facts.to_bits());
        let mut current = self.0.load(Ordering::Acquire);
        loop {
            let next = (current & !BITS_MASK) | bits;
            match self
                .0
                .compare_exchange(current, next, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => return,
                Err(seen) => current = seen,
            }
        }
    }

    /// Stores `facts` as the publication `facts_gen` (the model's publish).
    pub fn store_with_gen(&self, facts: KeyFacts, facts_gen: u64) {
        let word = ((facts_gen & BITS_MASK) << GEN_SHIFT) | u64::from(facts.to_bits());
        self.0.store(word, Ordering::Release);
    }

    pub fn load(&self) -> KeyFacts {
        self.load_with_gen().0
    }

    /// The facts and the low 32 bits of the gen they were published under, read together.
    pub fn load_with_gen(&self) -> (KeyFacts, u32) {
        let word = self.0.load(Ordering::Acquire);
        (
            KeyFacts::from_bits((word & BITS_MASK) as u32),
            (word >> GEN_SHIFT) as u32,
        )
    }

    /// The key path marked the active block as having received input: later keys of this block
    /// do not post the mark again, before the model has applied the first. Does not touch the gen.
    pub fn note_received_user_input(&self) {
        self.0
            .fetch_or(u64::from(RECEIVED_USER_INPUT), Ordering::AcqRel);
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
    /// The modes a key's escape sequence is built from (#1411): the pane key sink translates a key
    /// down into bytes from this copy, without the model's lock.
    pub key_modes: PublishedKeyModes,
    /// The active block is executing a command (`Block::is_executing`): the sink's rule for a key
    /// typed while an input method composes (#1411).
    pub active_block_executing: bool,
    /// What `TerminalView::is_input_box_visible` reads from the model (#1440).
    pub input_box: InputBoxFacts,
    /// The block list has a text selection (Ctrl-C copies it instead of interrupting, #1411).
    pub has_block_list_selection: bool,
    /// The alt screen has a text selection.
    pub has_alt_screen_selection: bool,
    /// The active block holds an agent-requested command that is on its way to / in the shell.
    pub agent_requested_command_in_flight: bool,
    /// The conversation a second Ctrl-C tears down: the user already took the command over with
    /// `Stop` (an earlier Ctrl-C).
    pub conversation_to_stop: Option<crate::ai::agent::conversation::AIConversationId>,
    /// The publication this snapshot belongs to (#1424): the model's `facts_gen` when it was
    /// built. The same value is stored in the [`KeyFactsCell`], and a reader that wants the two
    /// cells to describe one moment retries until they agree ([`load_consistent`]). A snapshot
    /// built by hand (a test) has gen 0.
    pub facts_gen: u64,
    /// `model.block_list().is_bootstrapped()`: the paste route's "the shell can take a paste" bit
    /// without the model's lock (`paste_routing`).
    pub is_bootstrapped: bool,
    /// `model.needs_bracketed_paste()`.
    pub needs_bracketed_paste: bool,
    /// The active block's output has had visible text at some point (sticky for the block: it
    /// never goes back to false while the block is active). Replaces the blocking
    /// `contents_to_string` lock of `ssh_connect_gate_drops_keys` (#1424).
    pub visible_text_seen: bool,
    /// The gen at which the active block's output last BECAME unterminated text (visible text
    /// with the cursor parked after it on its row: the shape of a prompt), `None` while it is not.
    /// The ssh password-prompt rule (H4) compares a key's stamp with this: a key stamped before
    /// it was typed blind. Exact for the last `OUTPUT_TAIL_ROWS` rows, like the view's own
    /// `active_output_ends_in_unterminated_text`.
    pub unterminated_since_gen: Option<u64>,
}

/// The terminal modes `KeystrokeWithDetails::to_escape_sequence` asks about (application cursor
/// keys and the kitty keyboard protocol's flags), copied from the model when it publishes (#1411).
/// A [`ModeProvider`] for the key path, so translating a key down never takes the model's lock.
/// It answers only the modes it holds; any other mode reads as unset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PublishedKeyModes {
    pub app_cursor: bool,
    /// `TermMode::KEYBOARD_PROTOCOL` (the composite the model answers for).
    pub keyboard_protocol: bool,
    pub disambiguate_escape: bool,
    pub report_event_types: bool,
    pub report_alternate_keys: bool,
    pub report_all_as_escape: bool,
    pub report_associated_text: bool,
}

impl PublishedKeyModes {
    pub fn from_model(model: &TerminalModel) -> Self {
        Self {
            app_cursor: model.is_term_mode_set(TermMode::APP_CURSOR),
            keyboard_protocol: model.is_term_mode_set(TermMode::KEYBOARD_PROTOCOL),
            disambiguate_escape: model.is_term_mode_set(TermMode::KEYBOARD_DISAMBIGUATE_ESCAPE),
            report_event_types: model.is_term_mode_set(TermMode::KEYBOARD_REPORT_EVENT_TYPES),
            report_alternate_keys: model.is_term_mode_set(TermMode::KEYBOARD_REPORT_ALTERNATE_KEYS),
            report_all_as_escape: model.is_term_mode_set(TermMode::KEYBOARD_REPORT_ALL_AS_ESCAPE),
            report_associated_text: model
                .is_term_mode_set(TermMode::KEYBOARD_REPORT_ASSOCIATED_TEXT),
        }
    }
}

impl ModeProvider for PublishedKeyModes {
    fn is_term_mode_set(&self, mode: TermMode) -> bool {
        let asked = mode.bits();
        let is = |known: TermMode| asked == known.bits();
        if is(TermMode::APP_CURSOR) {
            self.app_cursor
        } else if is(TermMode::KEYBOARD_PROTOCOL) {
            self.keyboard_protocol
        } else if is(TermMode::KEYBOARD_DISAMBIGUATE_ESCAPE) {
            self.disambiguate_escape
        } else if is(TermMode::KEYBOARD_REPORT_EVENT_TYPES) {
            self.report_event_types
        } else if is(TermMode::KEYBOARD_REPORT_ALTERNATE_KEYS) {
            self.report_alternate_keys
        } else if is(TermMode::KEYBOARD_REPORT_ALL_AS_ESCAPE) {
            self.report_all_as_escape
        } else if is(TermMode::KEYBOARD_REPORT_ASSOCIATED_TEXT) {
            self.report_associated_text
        } else {
            debug_assert!(false, "key mode {asked:#x} is not published");
            log::debug!("key mode {asked:#x} is not published; read as unset");
            false
        }
    }
}

/// The rows of output the unterminated-text test looks at (the view's `OUTPUT_TAIL_ROWS`).
pub const OUTPUT_TAIL_ROWS: usize = 16;

/// Whether `output` holds text a user could read (not just whitespace and control bytes).
pub fn output_has_visible_text(output: &str) -> bool {
    output
        .chars()
        .any(|c| !c.is_whitespace() && !c.is_control())
}

/// What the model remembers about the active block's output between publications, so the
/// snapshot's sticky bits cost nothing once they are set (see [`KeyModelSnapshot::visible_text_seen`]).
/// A leaf lock inside the model's: held for a few field writes plus the scans below, taken only
/// by whoever holds the model.
#[derive(Debug, Default)]
pub struct StickyOutput {
    block_hash: u64,
    visible_seen: bool,
    unterminated: bool,
    unterminated_since: u64,
}

impl StickyOutput {
    /// Folds in what the model shows now. `scan_visible` runs only while no visible text was
    /// seen in this block, `scan_unterminated` only once there was.
    fn observe(
        &mut self,
        block_hash: u64,
        facts_gen: u64,
        scan_visible: impl FnOnce() -> bool,
        scan_unterminated: impl FnOnce() -> bool,
    ) -> (bool, Option<u64>) {
        if self.block_hash != block_hash {
            *self = Self {
                block_hash,
                ..Self::default()
            };
        }
        if !self.visible_seen {
            self.visible_seen = scan_visible();
        }
        let unterminated_now = self.visible_seen && scan_unterminated();
        if unterminated_now {
            if !self.unterminated {
                self.unterminated = true;
                self.unterminated_since = facts_gen;
            }
        } else {
            self.unterminated = false;
        }
        (
            self.visible_seen,
            self.unterminated.then_some(self.unterminated_since),
        )
    }
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
        let active_block_hash = block_id_hash(model.active_block_id());
        let facts_gen = model.facts_gen();
        let (visible_text_seen, unterminated_since_gen) = model.sticky_output().lock().observe(
            active_block_hash,
            facts_gen,
            || output_has_visible_text(&active_block.output_grid().contents_to_string(false, None)),
            || {
                let grid = active_block.output_grid();
                grid.grid_handler().cursor_point().col > 0
                    && output_has_visible_text(
                        &grid.contents_to_string(false, Some(OUTPUT_TAIL_ROWS)),
                    )
            },
        );
        Self {
            read_only: model.is_read_only(),
            shared_session_executor: model.shared_session_status().is_executor(),
            pending_session_id: model.pending_session_id(),
            has_pending_ssh_session: model.has_pending_ssh_session(),
            has_pending_subshell: model.pending_subshell_session().is_some(),
            active_block_bootstrapped: model.is_active_block_bootstrapped(),
            active_block_started: active_block.started(),
            active_and_long_running: active_block.is_active_and_long_running(),
            active_block_hash,
            active_block_start: active_block.start_ts().copied(),
            agent_owns_pty,
            interactive_latch: Some(active_block.interactive_latch().clone()),
            app_cursor: model.is_term_mode_set(TermMode::APP_CURSOR),
            key_modes: PublishedKeyModes::from_model(model),
            active_block_executing: active_block.is_executing(),
            input_box: InputBoxFacts::from_model(model),
            has_block_list_selection: model.block_list().selection().is_some(),
            has_alt_screen_selection: model.alt_screen().selection().is_some(),
            agent_requested_command_in_flight: active_block
                .is_agent_requested_command_in_flight(),
            conversation_to_stop: active_block
                .long_running_control_state()
                .and_then(|state| {
                    state
                        .user_take_over_reason()
                        .is_some_and(crate::ai::blocklist::block::cli_controller::UserTakeOverReason::is_stop)
                        .then(|| active_block.ai_conversation_id())
                })
                .flatten(),
            facts_gen,
            is_bootstrapped: model.block_list().is_bootstrapped(),
            needs_bracketed_paste: model.needs_bracketed_paste_shared(),
            visible_text_seen,
            unterminated_since_gen,
        }
    }

    /// `is_long_running` of the view: a running command in a session the user can write to.
    pub fn is_long_running(&self) -> bool {
        self.active_and_long_running && !self.read_only
    }
}

/// The latest [`KeyModelSnapshot`], shared between the model (which stores) and the views (which
/// load).
///
/// Next to the snapshot (republished after every parse slice) it holds the key modes and the
/// alt-screen bit in one atomic word that the model stores the moment a mode changes, in the middle
/// of a slice (#1411): a key that finds the parser inside the lock overlays it on the stale copy
/// ([`Self::overlay_eager_modes`]), so it is encoded with the modes of this moment, not of the last
/// slice's end (a kitty pop and the prompt that follows arrive in one slice).
#[derive(Clone, Debug, Default)]
pub struct KeyModelCell {
    snapshot: Arc<Mutex<KeyModelSnapshot>>,
    eager_modes: Arc<AtomicU32>,
    /// Bit 0: the block list has a selection; bit 1: the alt screen has one. Stored by the main
    /// thread when the model reports a selection change, and with every stored snapshot.
    selections: Arc<AtomicU32>,
}

const EAGER_VALID: u32 = 1 << 8;
const EAGER_ALT_SCREEN: u32 = 1 << 7;

impl PublishedKeyModes {
    fn to_eager_bits(self) -> u32 {
        [
            self.app_cursor,
            self.keyboard_protocol,
            self.disambiguate_escape,
            self.report_event_types,
            self.report_alternate_keys,
            self.report_all_as_escape,
            self.report_associated_text,
        ]
        .into_iter()
        .enumerate()
        .fold(0, |bits, (i, set)| if set { bits | (1 << i) } else { bits })
    }

    fn from_eager_bits(bits: u32) -> Self {
        let bit = |i: u32| bits & (1 << i) != 0;
        Self {
            app_cursor: bit(0),
            keyboard_protocol: bit(1),
            disambiguate_escape: bit(2),
            report_event_types: bit(3),
            report_alternate_keys: bit(4),
            report_all_as_escape: bit(5),
            report_associated_text: bit(6),
        }
    }
}

impl KeyModelCell {
    pub fn store(&self, snapshot: KeyModelSnapshot) {
        self.store_eager_modes(snapshot.key_modes, snapshot.input_box.alt_screen_active);
        self.store_selections(
            snapshot.has_block_list_selection,
            snapshot.has_alt_screen_selection,
        );
        *self.snapshot.lock() = snapshot;
    }

    pub fn load(&self) -> KeyModelSnapshot {
        self.snapshot.lock().clone()
    }

    /// [`Self::load`] with the modes of this moment (see the type's doc).
    pub fn load_exact_modes(&self) -> KeyModelSnapshot {
        let mut snapshot = self.load();
        self.overlay_eager_modes(&mut snapshot);
        snapshot
    }

    /// Stores the key modes and the alt-screen bit as they are now. Called by the model with its
    /// lock held, wherever one of them changes.
    pub fn store_eager_modes(&self, modes: PublishedKeyModes, alt_screen: bool) {
        let word =
            modes.to_eager_bits() | EAGER_VALID | if alt_screen { EAGER_ALT_SCREEN } else { 0 };
        self.eager_modes.store(word, Ordering::Release);
    }

    /// Records whether the model has a text selection (the main thread, on a selection change).
    pub fn store_selections(&self, block_list: bool, alt_screen: bool) {
        self.selections.store(
            u32::from(block_list) | (u32::from(alt_screen) << 1),
            Ordering::Release,
        );
    }

    /// The key modes as last published (mid-slice or at a slice's end), `None` before the first.
    pub fn eager_modes(&self) -> Option<PublishedKeyModes> {
        let word = self.eager_modes.load(Ordering::Acquire);
        (word & EAGER_VALID != 0).then(|| PublishedKeyModes::from_eager_bits(word))
    }

    /// Replaces the snapshot's modes and alt-screen bit with the eagerly published ones (nothing
    /// before the first publication).
    pub fn overlay_eager_modes(&self, snapshot: &mut KeyModelSnapshot) {
        let word = self.eager_modes.load(Ordering::Acquire);
        if word & EAGER_VALID != 0 {
            snapshot.key_modes = PublishedKeyModes::from_eager_bits(word);
            snapshot.input_box.alt_screen_active = word & EAGER_ALT_SCREEN != 0;
        }
        let selections = self.selections.load(Ordering::Acquire);
        snapshot.has_block_list_selection = selections & 1 != 0;
        snapshot.has_alt_screen_selection = selections & 2 != 0;
    }

    /// The gen of the stored snapshot, without cloning it.
    pub fn facts_gen(&self) -> u64 {
        self.snapshot.lock().facts_gen
    }
}

/// How many times [`load_consistent`] re-reads before it gives up and reports the pair as torn.
pub const CONSISTENT_READ_ATTEMPTS: usize = 8;

/// One read of both published cells that describes ONE publication (#1424).
#[derive(Clone, Debug)]
pub struct ConsistentRead {
    pub facts: KeyFacts,
    pub snapshot: KeyModelSnapshot,
    /// The gen both cells agreed on (the snapshot's), or, when `consistent` is false, one before
    /// the snapshot's (the publisher stores the snapshot first, so the facts are the older half):
    /// a stamp taken from it can only be early (classed "before"), never late.
    pub facts_gen: u64,
    /// The two cells carried the same gen. False only if the publisher kept racing the reader for
    /// [`CONSISTENT_READ_ATTEMPTS`] reads; the caller treats the facts as one publication behind.
    pub consistent: bool,
    /// The facts were read from the exact model (it was free), not from the published cells.
    pub exact: bool,
}

/// Reads `facts` and `model` until their gens agree (seqlock style). The publisher stores the
/// snapshot first and the facts second, both under the same gen, so a reader that sees the same
/// gen in both has seen one publication whole; a mismatch means it caught the publisher between
/// the two stores (or a newer publication began) and it reads again. Never takes the model's
/// lock, never takes a maximum: the gen it reports is one that was published together.
pub fn load_consistent(facts: &KeyFactsCell, model: &KeyModelCell) -> ConsistentRead {
    load_consistent_with(facts, model, CONSISTENT_READ_ATTEMPTS)
}

/// [`load_consistent`] with the number of attempts given (the model-checker tests use few).
pub fn load_consistent_with(
    facts: &KeyFactsCell,
    model: &KeyModelCell,
    attempts: usize,
) -> ConsistentRead {
    let mut last = None;
    for _ in 0..attempts.max(1) {
        let (fact_bits, fact_gen) = facts.load_with_gen();
        let snapshot = model.load();
        let (_, fact_gen_after) = facts.load_with_gen();
        let snapshot_gen = snapshot.facts_gen;
        if fact_gen == fact_gen_after && (snapshot_gen & BITS_MASK) as u32 == fact_gen {
            return ConsistentRead {
                facts: fact_bits,
                facts_gen: snapshot_gen,
                snapshot,
                consistent: true,
                exact: false,
            };
        }
        last = Some((fact_bits, snapshot));
    }
    let (facts, snapshot) = last.expect("at least one attempt");
    ConsistentRead {
        facts,
        // The publisher stores the snapshot first, so a torn pair is the new snapshot with the
        // previous facts: report the previous gen.
        facts_gen: snapshot.facts_gen.saturating_sub(1),
        snapshot,
        consistent: false,
        exact: false,
    }
}

/// The gen the two cells agree on, for a key stamp (#1424): [`load_consistent`] without cloning
/// the snapshot. `(gen, true)` when both cells carried it; after [`CONSISTENT_READ_ATTEMPTS`]
/// torn reads `(snapshot gen - 1, false)`, the older half, so a stamp is early rather than late.
pub fn load_gen_consistent(facts: &KeyFactsCell, model: &KeyModelCell) -> (u64, bool) {
    load_gen_consistent_with(facts, model, CONSISTENT_READ_ATTEMPTS)
}

/// [`load_gen_consistent`] with the number of attempts given.
pub fn load_gen_consistent_with(
    facts: &KeyFactsCell,
    model: &KeyModelCell,
    attempts: usize,
) -> (u64, bool) {
    let mut snapshot_gen = 0;
    for _ in 0..attempts.max(1) {
        let (_, fact_gen) = facts.load_with_gen();
        snapshot_gen = model.facts_gen();
        let (_, fact_gen_after) = facts.load_with_gen();
        if fact_gen == fact_gen_after && (snapshot_gen & BITS_MASK) as u32 == fact_gen {
            return (snapshot_gen, true);
        }
    }
    (snapshot_gen.saturating_sub(1), false)
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
    /// The user wrote EOT to a command that is not the shell's own prompt (^D with no input box):
    /// the model ignores the bootstrapping messages that command's session would still send
    /// (`TerminalModel::ignore_bootstrapping_messages`, #1111). A flag, never dropped.
    IgnoreBootstrappingMessages,
    /// Ctrl-C with a selection on the alt screen clears it (`AltScreen::clear_selection`, #1411).
    ClearAltScreenSelection,
    /// Clears the block list's text selection (`BlockList::clear_selection`, #1411).
    ClearBlockListSelection,
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
            // A one-way flag: dropping it would let a dying session's bootstrap run (#1111).
            if matches!(
                command,
                ModelCommand::IgnoreBootstrappingMessages
                    | ModelCommand::ClearAltScreenSelection
                    | ModelCommand::ClearBlockListSelection
            ) {
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

// Interleaving models of the input pipeline (pane queue drain, ssh hand-over, hand-off holds, the
// stamp-then-decide gen order; #1512, #1531). The module name ends in `model_tests` so the
// `model_tests::` filter of the `test-shuttle-loom` job selects it.
#[cfg(all(test, any(shuttle, loom)))]
#[path = "pipeline_model_tests.rs"]
mod pipeline_model_tests;
