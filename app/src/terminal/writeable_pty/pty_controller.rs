use std::{borrow::Cow, collections::VecDeque, sync::Arc, time::Duration};

use async_channel::{Receiver, Sender};
use parking_lot::FairMutex;
use thiserror::Error;
use warpui::r#async::{Timer, block_on};
use warpui::{Entity, ModelContext, ModelHandle, SingletonEntity};

use crate::SessionSettings;
use crate::ai::agent::AIAgentPtyWriteMode;
use crate::features::FeatureFlag;
use crate::terminal::input::CommandExecutionSource;
use crate::terminal::model::block::BlockId;
use crate::terminal::model::completions::ShellCompletion;
use crate::terminal::model::early_output::TypeaheadMode;
use crate::terminal::model::key_facts::{ModelCommand, ModelInbox};
use crate::terminal::model::lifecycle::StartCommandOutcome;
#[cfg(not(target_family = "wasm"))]
use crate::terminal::model::session::BootstrapSessionType;
use crate::terminal::model::session::{
    ExecutorCommandEvent, InBandCommandCancelledEvent, Sessions,
};
use crate::terminal::model::tmux::commands::TmuxCommand;
use crate::terminal::model::typeahead_handoff::{
    ActiveDecision, HeldAgentCommand, PaneHandoff, PtyWriteCount, ShellPromptFact,
};
use crate::terminal::model_events::AnsiHandlerEvent;
use crate::terminal::view::LINEFEED_REGEX;
#[cfg(not(target_family = "wasm"))]
use crate::terminal::writeable_pty::bootstrap_file::{TempBootstrapFile, permanent_bootstrap_file};
use crate::terminal::writeable_pty::command_pipe;
#[cfg(not(target_family = "wasm"))]
use crate::terminal::writeable_pty::long_command_file;
use crate::terminal::writeable_pty::posix_long_command;
use crate::terminal::{
    SizeUpdate, TerminalModel, bootstrap,
    line_editor_status::{LineEditorStatus, LineEditorStatusEvent},
    model::{
        ansi::Handler,
        escape_sequences,
        session::{SessionId, SessionInfo},
    },
    model_events::{ModelEvent, ModelEventDispatcher},
    shell::ShellType,
};
use crate::view_components::DismissibleToast;
use crate::workspace::ToastStack;

use super::Message;
use super::write_queue::{AbortScope, Pacing, SetupLive};

/// Gap between the chunks of a bootstrap written into a subshell behind a user-space pty relay
/// (`sudo -i`, `su`, `script`, `ksu`, ...). #960: such a relay deadlocks when ~120KB is flooded
/// at it in one burst, and how fast it consumes its input is not observable from here (the echo
/// it produces is not byte-exact), so this is a time gap by necessity, not a way to decide
/// whether a chunk is written.
#[cfg(feature = "local_fs")]
// pacing: 25 ms gap between chunks of our own bootstrap into a user-space pty relay (#960); not a decision, never carries user keys
const RELAY_BOOTSTRAP_PACING: Duration = Duration::from_millis(25);
/// Gap between the chunks of a bootstrap written into a container subshell (`docker`/`podman
/// exec -it`): a double-pty proxy that drops data on large writes or bursts (#960 family).
/// Same reason as [`RELAY_BOOTSTRAP_PACING`]: the proxy's consumption is unobservable.
#[cfg(feature = "local_fs")]
// pacing: 50 ms gap between chunks of our own bootstrap into a container exec proxy (#960); not a decision, never carries user keys
const CONTAINER_BOOTSTRAP_PACING: Duration = Duration::from_millis(50);

/// Gap between the 1000-byte chunks of the ssh-warpify line-clear payload while the tty is
/// canonical (cooked, no newline in the line). Reason: P0 (#1384) measured that a canonical tty
/// accepts a write and then SILENTLY drops everything past its input queue (macOS: 1024 B; Linux
/// N_TTY: 4095 B), with no `EAGAIN`, no `POLLOUT` change and no readable counter in advance, so
/// there is no signal to wait on and the gap stays. A non-canonical tty (a bash/zsh line editor at
/// its prompt) gives a real `EAGAIN` instead, and then no gap is used (see
/// [`Pacing::GapIfCanonical`]).
// pacing: 10 ms gap between chunks of our own ssh-warpify payload on a cooked tty (#1384); not a decision, never carries user keys
const MAC_TTY_INPUT_QUEUE_PACING: Duration = Duration::from_millis(10);
/// Chunk size of the line-clear payload: under the macOS tty input queue (1024 B).
const MAC_TTY_INPUT_QUEUE_CHUNK: usize = 1000;

/// Whether the writer-owned job queue (#1384) is used: paced bootstrap writes, and the paste and
/// agent write classes (P4).
/// `PHOSPHOR_PTY_WRITE_QUEUE=0` turns it off and the timer-chained writes of before are used.
pub(crate) fn write_queue_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        let off = std::env::var("PHOSPHOR_PTY_WRITE_QUEUE").is_ok_and(|v| v.trim() == "0");
        if off {
            log::info!("PHOSPHOR_PTY_WRITE_QUEUE=0: bootstrap writes are paced by UI timers; paste and agent writes are not classed");
        }
        !off
    })
}

/// Whether the shell's `LineEditorReading` marker (#1390) ends the line editor's activation wait,
/// as the bracketed-paste enable does for readline/zle (#1208). PowerShell never enables
/// bracketed paste, so without this every PowerShell prompt waited the full 50 ms fallback
/// timer (`hop=line_editor_activation_fallback delay_ms=50` after every command in r18e4) before
/// the input report went out and keys typed since the prompt were handed over (#1401, #1402).
/// `PHOSPHOR_LINE_EDITOR_READING_ACTIVATES=0` (or off/false/no) keeps the timer.
fn line_editor_reading_activates() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        let enabled = line_editor_reading_activates_from(
            std::env::var("PHOSPHOR_LINE_EDITOR_READING_ACTIVATES")
                .ok()
                .as_deref(),
        );
        if !enabled {
            log::info!(
                "PHOSPHOR_LINE_EDITOR_READING_ACTIVATES is off: the line editor's reading marker \
                 does not end the activation wait; shells without bracketed paste wait the \
                 fallback"
            );
        }
        enabled
    })
}

/// [`line_editor_reading_activates`] for a given value of its kill switch: `0`, `off`, `false`
/// or `no` (any case) keeps the timer, as for `PHOSPHOR_CONPTY_CTRL_C_RESET`.
pub(crate) fn line_editor_reading_activates_from(value: Option<&str>) -> bool {
    !matches!(
        value
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref(),
        Some("0" | "off" | "false" | "no")
    )
}

/// Byte sequence to emulate the user pressing ENTER, used to execute a command in the shell.
const COMMAND_ENTER: &[u8] = &[escape_sequences::C0::CR, escape_sequences::C0::LF];
/// Used to let the shell know we are switching to the PS1 prompt via a bindkey \ep. This will
/// restore the PS1 from the saved PS1 value (we had unset the PS1 for Zap prompt).
const SWITCH_TO_PS1_ESCAPE_SEQUENCE: &[u8] = &[escape_sequences::C0::ESC, b'p'];
/// Used to let the shell know we are switching to the Zap prompt via a bindkey \ew. This will
/// unset the PS1 to ensure we don't have a double prompt (PS1 and Zap prompt).
const SWITCH_TO_WARP_PROMPT_ESCAPE_SEQUENCE: &[u8] = &[escape_sequences::C0::ESC, b'w'];

/// How long to wait for the shell's OSC reply to the `^Y` completions trigger before giving up.
///
/// The reply is emitted by the Phosphor shell hooks the instant they see the control code, so
/// this is a liveness bound, not a work budget: either the hooks are installed and it returns in
/// milliseconds, or they are not installed and it never returns at all. The latter is the common
/// case in an escalated or otherwise un-warpified shell (`su`, `sudo su -`, `ksu`), where `^Y` is
/// just readline's yank and nothing answers.
// give-up-net: only gives up -- closes the requester channel and marks the cancel frame owed; writes nothing
const NATIVE_COMPLETIONS_PROMPT_TIMEOUT: Duration = Duration::from_secs(2);

/// How long to wait for completion results once the prompt has been sent.
///
/// Unlike the prompt handshake this covers real work in the shell -- globbing a large directory,
/// say -- so it is far more generous. It does not gate PTY writes; it exists so the completions
/// request cannot leak its channel and hang the requester's future forever.
// give-up-net: a lost-event net only (the normal ends are CompletionsFinished, the cancel and the
// shell's Precmd/exit): drops the exchange and closes the requesters' channels; writes and releases nothing.
const NATIVE_COMPLETIONS_RESULTS_TIMEOUT: Duration = Duration::from_secs(15);

/// Shown once per pane when the shell never answers the `^Y` completions trigger.
///
/// Until this existed the timeout was recovered from silently: the watchdog dropped the wedged
/// state, PTY writes resumed, and a `log::warn!` went to the log file -- so from the user's side
/// Tab simply did nothing, every time, with no explanation anywhere they would look. The wording
/// hedges on the cause because the handshake can also go unanswered when the hooks are present
/// but wedged (a remote session that lost them, say); an un-phosphorized shell is only the
/// dominant case, not the only one.
const NATIVE_COMPLETIONS_UNAVAILABLE_MESSAGE: &str = "Tab completions aren't available in this \
     shell -- it has no Phosphor shell integration to answer them. This is usual for a shell \
     entered with su, sudo su - or ksu.";

/// How long the PTY write gate must stay shut with writes queued before
/// `track_pty_write_gate_stall` logs it. Chosen to be well above ordinary scheduling jitter
/// (`LineEditorStatusEvent::Active` firing a beat late) but well below the point where a user
/// would already suspect something is wrong, so the log line is there before they go looking.
const PTY_WRITE_GATE_STALL_LOG_THRESHOLD: Duration = Duration::from_secs(3);

/// #1353 phase 6 (PTY-3): the stale line counts are reset by EVENTS only -- the sampled shell
/// state at decision time (a program owns the terminal), the finished command having been
/// interactive (see `TypeaheadHandoff::note_interactive_command_finished`), a continuation
/// prompt, the shell's line editor reading again (the unix probe + stream barrier, or
/// PowerShell's `LineEditorReading` marker on ConPTY), and the prompt's Precmd.
///
/// Owner rule (2026-11-08, a timer never writes, routes, commits or re-asks): this is only the
/// one logged give-up (`handoff_stale_counts_kept`) for a wait no event can tell from "still
/// working through the queue". It changes no count and no state and asks for no decision; the pty
/// keeps input (a plain terminal) until the next event. The 250 ms reset of the non-seq path was
/// deleted: that path now waits for the same events.
// give-up-net: logs a stale-count wait only; no reset, no retry, no write, no routing
const TYPEAHEAD_HANDOFF_STALE_COUNT_GIVE_UP: Duration = Duration::from_secs(5);

/// #2182: how long the app's own in-band command (a generator, a prompt-chip probe) waits for the
/// user's held keys, or for the shell's line that holds typed text, before it is cancelled. The
/// holds end by their own events and caps; this only stops a missing event from stranding the
/// command's requester.
// give-up-net: cancels the app's own in-band command (its requester is told); no key is touched
const INBAND_WAIT_FOR_KEYS_GIVE_UP: Duration = Duration::from_secs(15);

/// Applied to the block a command starts, when it starts (e.g. its cloud workflow).
type AnnotateBlock = Box<dyn FnOnce(&mut TerminalModel) + Send>;

/// Why a queued [`PtyWrite::Command`] starts its block only when it is written.
enum CommandPark {
    /// It does not: its block was started when it was submitted, or it is the app's own in-band
    /// command (which says so by its id).
    None,
    /// An agent / queued-panel command held behind the user's unreported line
    /// (`hold_command_behind_unreported_user_line`): it waits on the hand-off's holds.
    BehindUserLine,
    /// #2182 (b): the user's own command, submitted while the app's in-band command occupied
    /// the active block. It is written at that command's prompt, ahead of every queued side call.
    BehindInBand(Box<ParkedUserCommand>),
}

/// #2182 (b): a user command parked behind the app's own in-band command, with what it needs to
/// start its block when it is written (and to go back to the front if that start is refused).
#[derive(Clone)]
struct ParkedUserCommand {
    command: String,
    shell_type: ShellType,
    source: CommandExecutionSource,
    /// Taken by the start that is accepted; kept across a refused one.
    annotate_block: Arc<parking_lot::Mutex<Option<AnnotateBlock>>>,
    /// What its `before_write_fn` found at the latest write; `None` until it has run.
    started: Arc<parking_lot::Mutex<Option<ParkedStart>>>,
    /// The editor's command on a shell that reports its line: the typeahead hand-off was told
    /// of it when it was PARKED (`TypeaheadHandoff::on_command_parked`), so the keys typed
    /// behind its Enter are the pty's and follow it (`handoff_command_pending`), and the request
    /// decision of the prompt it is written at is skipped, as for an editor command queued
    /// behind an inactive line editor. Only its block start is left for the write.
    expects_handoff: bool,
}

/// What the `before_write_fn` of a [`ParkedUserCommand`] found.
#[derive(Clone, Copy)]
struct ParkedStart {
    outcome: StartCommandOutcome,
    /// The typeahead hand-off engaged for the command.
    engaged: bool,
}

/// Represents a single call to write bytes to the PTY asynchronously.
enum PtyWrite {
    Command {
        command: String,
        shell_type: ShellType,
        /// The id if the command is an in-band command or `None` if the command is not an in-band
        /// command.
        in_band_command_id: Option<String>,
        /// If 'some', the given callback is called right before the bytes are written to the PTY.
        /// Its [`StartCommandOutcome`] gates whether the bytes are actually written: only
        /// [`StartCommandOutcome::Accepted`] proceeds to the PTY.
        before_write_fn: Option<Box<dyn FnOnce() -> StartCommandOutcome + Send + 'static>>,
        /// Why the command's block starts only when it is written, if it does. Explicit since
        /// #2182 (b): "not in-band, with a `before_write_fn`" used to mean one thing only.
        park: CommandPark,
    },
    Bytes {
        /// The bytes to be written.
        bytes: Cow<'static, [u8]>,
    },
    /// A paste-sized user write (#1255): written like `Bytes`, but a ^C cancels the part the
    /// event loop has not yet written to the pty.
    PasteBytes {
        /// The bytes to be written.
        bytes: Cow<'static, [u8]>,
    },
    AgentInput {
        /// The bytes to be written.
        bytes: Cow<'static, [u8]>,
        /// The `mode` for the agent's write.
        mode: AIAgentPtyWriteMode,
    },
    TmuxCommand(TmuxCommand),
    RunNativeShellCompletions(NativeShellCompletionsState),
}

// The in-band native-completions exchange (#1801), self-identifying by design.
//
// The exchange shares the tty with everything else written to it, and the shell and the app each
// have their own clock. Earlier versions held writes behind the `^Y` and had to guess when the
// shell was done; every guess lost keys somewhere (#1801 reviews). The design has two parts.
//
// The wire protocol. No step depends on the two clocks agreeing:
// 1. The app writes `^Y` only to a shell whose bootstrap advertised the protocol (the
//    `!phosphor_nc_frames_v1` shell option, see `Shell::supports_native_shell_completions`).
// 2. The shell's hook names the exchange with a fresh token: OSC `9280;P;<token>`. The app takes
//    a prompt only with a token it has not seen before in this pane; any other is cancelled.
// 3. The app ALWAYS ends the exchange with exactly one frame, as a single write:
//    `RS <token> : <%HH bytes of the text> US` (RS = 0x1e, US = 0x1f), or the cancel frame
//    `RS - : US`.
// 4. A frame is safe wherever it lands. The hook reads it byte by byte and accepts only the
//    frame's exact shape, by CONTENT only: `RS (token | -) ':' (%HH)* US`, the token being the
//    shell's own `${RANDOM}x${seq}x${RANDOM}`. There is no inter-byte timing: a frame may arrive
//    in pieces with any pauses. It consumes bytes while they still form a valid frame prefix;
//    the first byte that cannot continue one ends the read and everything read so far plus that
//    byte goes back to the line editor. A complete frame with the matching token is the answer;
//    one with another token, or the cancel, is consumed with nothing. At the shell's prompt (the
//    hook already gave up), RS is bound to a widget that reads the rest the same way. US itself
//    is not bound (it stays the line editor's undo). A frame never contains an EOT or a newline.
// 5. The hook and the widget consume nothing outside a valid frame prefix: the first byte must be
//    RS, and any byte that cannot continue the frame ends the read and everything read goes
//    back to the line editor, an Enter included (a terminal reply is dropped whole instead).
//    The only clock is the hook's overall read deadline, a give-up net: on expiry it hands back
//    what it read and exits, writing nothing and inventing no input.
//    bash below 4.0 (macOS /bin/bash 3.2, #1969) has no READLINE_LINE to edit: its hook hands
//    the same bytes back as keys, through a readline macro that runs right after the hook, in
//    front of anything still in the tty (`__warp_nc_finish` in bash_body.sh). Same order, same
//    rules; the app side does not differ.
//
// The app side. NOTHING ever sits in the write queue behind `^Y`:
// a. An exchange starts only when the write queue is empty, no command is executing, the line
//    editor is active and no other write path (the typeahead hand-off, paced setup writes, tmux
//    start-up) is engaged. Otherwise the Tab goes to the completer at once.
// b. While the exchange waits for its prompt, ANY write (a command, typed or pasted bytes, agent
//    bytes, ^C) first writes the cancel frame, which ends the hook safely wherever it is, then
//    goes out as usual, in order. The in-band answer is given up; nothing is held or reordered.
//    An Enter right after the Tab is held by the editor (the remote-native Enter wait) until the
//    answer lands, so it runs the completed line.
// c. The prompt arriving writes the answer frame (the hook consumes exactly the frame; later
//    bytes wait in the tty for the line editor). No prompt in time (2 s): the app GIVES UP. It
//    stops waiting, closes the requester's channel (no answer: the completer answers, and an
//    Enter held for the Tab is handed back with its line, never run) and marks the
//    cancel frame OWED; it writes NOTHING. The owed cancel goes in front of the next real write
//    (rule b), or when the shell's own late prompt arrives (a response to the shell's event);
//    either clears the debt, so there is never a second cancel. If neither ever happens, the
//    hook's own read deadline ends it. A timer is only a give-up net: it never writes and
//    never decides routing.
// e. A second Tab for the SAME text while an exchange is in flight (a fast Tab, Tab, Enter) is
//    attached to it: it gets the same answer (candidates, or an empty one for a cancel) and is
//    closed (no answer) if the exchange ends without one. No second `^Y` goes out. A different
//    text is refused (the completer answers).
// d. The shell leaving first (a Precmd, ExitShell or Exit while the prompt is awaited) just ends
//    the exchange: no frame (the shell that would read it is not the one asked).
//
// Two kinds of input reach the tty without going through `send_write_to_event_loop`, so the
// cancel-first rule does not cover them; each is safe on its own terms:
// - Terminal-query replies (DA, cursor position, colour reports), which the event loop queues
//   itself when the shell or a program asks. The shell's hook does not ask anything, so none is
//   due during an exchange; one that arrives anyway is an escape sequence, which breaks the
//   frame's shape, so the hook (or the RS widget) drops the whole reply and ends the exchange
//   with nothing, leaving the line clean.
// - A resize (`Message::Resize`, SIGWINCH to the shell). It writes no byte. The signal may cut
//   the hook's read short; the hook then just returns, and the frame lands at the prompt, where
//   the RS widget drops it. Only that Tab's in-band answer is lost.
//
// So a late prompt, a late answer or a timeout on either side can cost only that one Tab's
// in-band answer (the completer answers instead); never a key and never a command.

/// The byte that opens every answer frame (ASCII RS, `^^`).
pub const NATIVE_COMPLETIONS_FRAME_START: u8 = 0x1e;
/// The byte that closes every answer frame (ASCII US, `^_`).
pub const NATIVE_COMPLETIONS_FRAME_END: u8 = 0x1f;

/// The token of the cancel frame. Never generated by a shell (tokens are letters and digits).
const NATIVE_COMPLETIONS_CANCEL_TOKEN: &str = "-";

/// The most prompt tokens remembered per pane to refuse a replayed one (#1801 review M3).
const NATIVE_COMPLETIONS_SEEN_TOKENS: usize = 4096;

/// The answer frame for `token`'s exchange: `RS token : %HH... US`, one write.
pub fn native_completions_answer(token: &str, buffer_text: &str) -> Vec<u8> {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut bytes = Vec::with_capacity(buffer_text.len() * 3 + token.len() + 3);
    bytes.push(NATIVE_COMPLETIONS_FRAME_START);
    bytes.extend_from_slice(token.as_bytes());
    bytes.push(b':');
    for byte in buffer_text.bytes() {
        bytes.push(b'%');
        bytes.push(HEX[usize::from(byte >> 4)]);
        bytes.push(HEX[usize::from(byte & 0x0f)]);
    }
    bytes.push(NATIVE_COMPLETIONS_FRAME_END);
    bytes
}

/// The cancel frame: ends whatever exchange the shell is in, or is dropped at its prompt.
pub fn native_completions_cancel() -> Vec<u8> {
    native_completions_answer(NATIVE_COMPLETIONS_CANCEL_TOKEN, "")
}

/// How a native-completions request is recovered (#1801).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeCompletionsRequestOptions {
    /// The shell is asked FIRST (a remote session without the helper on an explicit Tab). A
    /// prompt that does not come is not "no shell integration" there (the session has it), so
    /// no such toast is shown.
    pub remote_first: bool,
}

enum NativeShellCompletionsState {
    /// `^Y` written; the next write of any kind first ends the exchange with the cancel frame.
    AwaitingPrompt {
        buffer_text: String,
        results_tx: async_channel::Sender<Vec<ShellCompletion>>,
        remote_first: bool,
    },
    /// The answer frame is written; the shell is completing. Writes are not held: the hook has
    /// consumed exactly its frame, and later bytes wait in the tty for the line editor.
    AwaitingResults {
        results_tx: async_channel::Sender<Vec<ShellCompletion>>,
    },
}

impl NativeShellCompletionsState {
    fn is_awaiting_prompt(&self) -> bool {
        matches!(self, Self::AwaitingPrompt { .. })
    }
}

/// The most input held while tmux control mode waits for its primary pane (#1442): the ssh hold's
/// 64 KiB. Control mode is ready within a round trip; this is only reached when it never is.
pub(crate) const MAX_TMUX_PENDING_BYTES: usize = 64 * 1024;

enum TmuxControlMode {
    /// Tmux control mode is started, but we don't have the primary pane yet.
    Pending { buffer: Vec<u8>, overflowed: bool },
    /// Tmux control mode is active.
    Active { primary_pane: u32 },
}

/// Controller for writes to the PTY.
///
/// This is responsible for coordinating writes to the PTY amongst input like user commands, non-command user
/// input, and in-band commands in conjunction with line editor status.
pub struct PtyController<T: EventLoopSender> {
    /// `Sender` for the main PTY event loop channel.
    event_loop_tx: T,
    terminal_model: Arc<FairMutex<TerminalModel>>,
    line_editor_status: ModelHandle<LineEditorStatus>,
    sessions: ModelHandle<Sessions>,
    model_event_dispatcher: ModelHandle<ModelEventDispatcher>,
    pending_writes: VecDeque<PtyWrite>,
    is_user_command_executing: bool,
    is_bracketed_paste_enabled: bool,
    /// If we're bootstrapping the shell by sourcing a file with the bootstrap
    /// script, this will hold the handle to the file.  Once bootstrapping is
    /// complete, it will be dropped to clean up the temporary file.
    #[cfg(not(target_family = "wasm"))]
    bootstrap_file: Option<TempBootstrapFile>,
    /// Whether this is a local PowerShell session, whose very long commands run from a temp file
    /// rather than being typed into PSReadLine (#1106). Set when the shell is initialised.
    #[cfg(not(target_family = "wasm"))]
    runs_long_commands_from_file: bool,
    /// The pipe serving the last such command to its launcher line (#1592); it must outlive the
    /// Preexec (which runs before the launcher reads it), so it is not `command_pipe`. Replaced by
    /// the next served command, closed on drop. In memory only: no command is written to disk.
    #[cfg(not(target_family = "wasm"))]
    long_command_pipe: Option<command_pipe::CommandPipe>,
    /// The sessions this controller's shells minted pipe names for; forgotten when it drops.
    #[cfg(not(target_family = "wasm"))]
    pipe_sessions: command_pipe::SessionPipeNames,
    /// The command being served on the session's named pipe for the PowerShell insert chord
    /// (#1581); in memory only, closed when dropped. Set exactly while `file_insert_pending` is.
    command_pipe: Option<command_pipe::CommandPipe>,
    /// The command whose text went to the shell on the pipe plus the insert chord (#1581), until
    /// its Preexec arrives (the shell ran it) or `CommandFileFailed` does (the shell inserted
    /// nothing and the command is typed instead). At most one: a second command is typed while it
    /// is set.
    file_insert_pending: Option<String>,
    /// The id of the active block (the prompt's) when the chord was written; a Precmd for that
    /// same block is not evidence the chord was lost (B2). Set and cleared with
    /// `file_insert_pending`.
    file_insert_block: Option<BlockId>,
    #[cfg(test)]
    fast_insert_off_for_test: bool,
    /// One-shot: the retype after `CommandFileFailed` must be typed, not sent as the chord again.
    force_typed_command: bool,
    tmux_control_mode: Option<TmuxControlMode>,
    in_flight_native_completions_state: Option<NativeShellCompletionsState>,
    /// Incremented every time a native-completions handshake starts or advances a phase, so a
    /// timer armed for one phase cannot clear the state belonging to a later one. Without it a
    /// slow-but-successful handshake would be torn down by its own predecessor's timeout.
    native_completions_generation: u64,
    /// Whether the user has already been told that this shell does not answer completion
    /// requests. A `PtyController` is per-pane, so this bounds the toast to once per pane for
    /// the life of the shell -- the un-phosphorized shell that causes this answers no Tab, ever,
    /// and a toast on every keystroke would be worse than the silence it replaces.
    has_reported_native_completions_unavailable: bool,
    /// Whether the last prompt-timeout cancel asked for the "no shell integration" toast; read by
    /// tests (#1801: never for a remote-first request).
    #[cfg_attr(not(test), allow(dead_code))]
    last_native_cancel_toasted: bool,
    /// The prompt timeout gave up on an exchange and wrote nothing: the cancel frame is owed. It
    /// goes out in front of the next real write, or when the shell's own late prompt arrives,
    /// and a Precmd/exit of the shell clears it. A give-up timer never writes.
    native_cancel_owed: bool,
    /// The text the in-flight exchange asks about, so a second Tab for the same text can attach.
    native_inflight_buffer: String,
    /// Requesters of later Tabs for the SAME text, attached to the in-flight exchange (a fast
    /// Tab, Tab, Enter): they get its answer (candidates, or an empty one for a cancel) and are
    /// dropped (no answer) whenever the exchange ends without one. No new exchange is started.
    native_attached_waiters: Vec<async_channel::Sender<Vec<ShellCompletion>>>,
    /// The prompt tokens taken in this pane (#1801 review M3).
    seen_native_prompt_tokens: std::collections::HashSet<String>,
    /// Delivers SIGINT to the pty's foreground process group (local ptys on Unix only); `None`
    /// where that is unavailable, in which case the ^C byte is written instead (#1001).
    foreground_sigint: Option<std::sync::Arc<dyn Fn() -> bool>>,
    /// When the current unbroken stretch of the PTY write gate being shut *with writes queued*
    /// began, if we are in one. Diagnostic-only bookkeeping for the still-unexplained field lockup
    /// in TODO.md ("The shell lockup"): the watchdog above only ever fires for the (currently
    /// unreachable, `FeatureFlag::NativeShellCompletions`-gated) native-completions handshake, but
    /// the *other* gate in `can_write_to_pty` -- the line editor being inactive -- has no such
    /// watchdog and is a live suspect. Cleared whenever the gate opens or the queue drains.
    pty_write_gate_blocked_since: Option<std::time::Instant>,
    /// Whether the current stretch tracked by `pty_write_gate_blocked_since` has already produced
    /// its one log line, so a long stall logs once instead of once per queued keystroke.
    has_logged_current_pty_write_stall: bool,
    /// Typeahead hand-off (`FeatureFlag::TypeaheadHandoff`): a command from the input editor is
    /// queued behind an inactive line editor. User bytes typed meanwhile collect in
    /// `handoff_bytes_after_command` and are written right after that command, so they become its
    /// typeahead instead of overtaking it into the shell's current line.
    handoff_command_pending: bool,
    handoff_bytes_after_command: Vec<u8>,
    /// The pane's typeahead hand-off, reached without the model's lock (#1403): a user key's
    /// write must not wait for the pty reader, which holds that lock while it parses.
    handoff: PaneHandoff,
    /// #1793: the hand-off's count of writes handed to the pty's event loop, counted here
    /// without the hand-off's lock (`send_message_to_event_loop`).
    pty_writes: PtyWriteCount,
    /// Where a key's write leaves its bookkeeping for the model (the typed-ahead record of its
    /// echo), applied before the model parses the next bytes.
    model_inbox: ModelInbox,
    /// Typeahead hand-off: the last request decision had to wait. Queued writes stay queued while
    /// the shell's line may hold user bytes, which a queued command's kill prefix would destroy.
    handoff_request_waiting: bool,
    /// #2187: the last request decision was parked because the shell's line editor was not yet
    /// proven to be reading while keys typed at this prompt sit in the shell's line (the keys went
    /// to the pty before the prompt was seen, e.g. right after a Ctrl-C). The shell's ready signal
    /// decides again ([`Self::activate_line_editor_on_ready_signal`]).
    handoff_parked_for_proof: bool,
    /// Typeahead hand-off: a command the hand-off does not own is queued behind the user's
    /// unreported line (`hold_command_behind_unreported_user_line`). At most one is held.
    handoff_held_command_pending: bool,
    /// #2182: the cancel channels of the in-band commands in `pending_writes`, by command id, so
    /// the give-up of their wait for the user's keys can tell the requester. Weak: they never
    /// keep a requester's channel open.
    in_band_cancels:
        std::collections::HashMap<String, async_channel::WeakSender<InBandCommandCancelledEvent>>,
    /// #2182: the front in-band command is waiting for the user's keys, and its give-up timer is
    /// armed for `in_band_wait_generation`.
    in_band_wait_armed: bool,
    in_band_wait_generation: u64,
    /// #2182 (r29 A2): `handoff_request_waiting` is set, and ONLY the in-band wait set it
    /// ([`Self::in_band_front_waits_for_keys`]). The shared flag also carries a parked decision
    /// (#2187), `ActiveDecision::Wait` and a held command; whenever one of those writes it
    /// ([`Self::set_handoff_request_waiting`]) this bit goes off, so the end of an in-band wait
    /// whose command was dropped (cancelled, cleared, given up) takes back exactly the waiting
    /// it set and never theirs.
    in_band_wait_set_waiting: bool,
    /// Typeahead hand-off: a command from the editor was written or queued while one was held,
    /// so `is_user_command_executing` belongs to it and dropping the held one must leave it.
    handoff_command_written_since_hold: bool,
    /// The chunks of a paced bootstrap write (container / relay subshell) that are not yet
    /// written, oldest first. Chunk k+1 is only written by the pacing step chained behind chunk
    /// k, so there is exactly one pending step and nothing to race.
    paced_write_queue: VecDeque<Vec<u8>>,
    /// Bumped to cancel the paced write in flight (a ^C, the pty shutting down, the shell
    /// exiting, a newer bootstrap): the step already scheduled sees the new value and writes
    /// nothing, so no chunk (nor the submit CR it ends in) is typed into whatever owns the pty
    /// afterwards.
    paced_write_generation: u64,
    /// Whether paced bootstrap writes go to the event loop as one `SetupChunked` job (#1384)
    /// instead of timer-chained `Input` messages. Decided once, from the sender (only the local
    /// unix event loop has the queue) and `PHOSPHOR_PTY_WRITE_QUEUE`.
    write_queue: bool,
    /// When the last lone ^C was sent to the event loop, until the line editor next announces it
    /// is ready (input-order log only: the ^C -> next prompt leg, #1401).
    ctrl_c_sent_at: Option<std::time::Instant>,
    /// The same ^C, kept until the NEXT command's Preexec (input-order log only, #1401): the
    /// ^C -> next command leg (line editor active, command written, Preexec) that the harness's
    /// "Ctrl-C then type the next command" measurement spans. Never decides anything.
    ctrl_c_next_command_at: Option<std::time::Instant>,
    /// A `SetupChunked` job was sent and no abort since, so a cancel has something to abort.
    setup_job_sent: bool,
    /// How many `SetupChunked` jobs the writer still holds (#1801 review M1): raised when one is
    /// sent, lowered by the writer when it drops the job (written, aborted, replaced, shut down).
    setup_jobs_live: Arc<std::sync::atomic::AtomicUsize>,
    /// Test-only: whether `shutdown_pty` was called (#980 tests; the real event loop is not
    /// running under test, so the sent `Message::Shutdown` is otherwise unobservable).
    #[cfg(test)]
    shutdown_requested: bool,
}

impl<T: EventLoopSender> PtyController<T> {
    pub fn new(
        event_loop_tx: T,
        model_event_dispatcher: ModelHandle<ModelEventDispatcher>,
        line_editor_status: ModelHandle<LineEditorStatus>,
        sessions: ModelHandle<Sessions>,
        executor_command_rx: Receiver<ExecutorCommandEvent>,
        terminal_model: Arc<FairMutex<TerminalModel>>,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        ctx.subscribe_to_model(&model_event_dispatcher, |me, event, ctx| {
            me.end_native_exchange_if_its_shell_left(event);
            match event {
            ModelEvent::Handler(AnsiHandlerEvent::UserCommandFinished) => {
                me.log_since_ctrl_c("command_finished_after_ctrl_c");
                me.is_user_command_executing = false;
            }
            ModelEvent::Handler(AnsiHandlerEvent::InitShell {
                pending_session_info,
            }) => {
                me.initialize_shell(pending_session_info.as_ref(), ctx);
            }
            ModelEvent::Handler(AnsiHandlerEvent::Bootstrapped {
                session_id,
                is_subshell,
            }) => {
                // #2218: the session is registered before this event is emitted
                // (`ModelEventDispatcher::complete_bootstrapped_session`).
                let is_ssh_or_remote = me
                    .sessions
                    .as_ref(ctx)
                    .get(*session_id)
                    .is_some_and(|session| session.is_subshell_or_ssh());
                me.shell_bootstrapped(*is_subshell, is_ssh_or_remote);
            }
            ModelEvent::Handler(AnsiHandlerEvent::SetBracketedPaste) => {
                me.set_bracketed_paste_enabled(true);
                if let Some(sent) = me.ctrl_c_sent_at.take() {
                    warpui::input_order::log_hop("line_editor_ready_after_ctrl_c", || {
                        format!(
                            "via=bracketed_paste since_ctrl_c_ms={:.1}",
                            sent.elapsed().as_secs_f64() * 1e3
                        )
                    });
                }
                // #1349: the line editor's ready signal releases the keys held since the
                // prompt's precmd (ConPTY), in typed order, ahead of any request. This is the
                // only thing that releases them besides a command start, a ^C or the give-up cap.
                // #1353 phase 5: with the hold in the view's input queue the keys are not here; the
                // view releases that gate (writing them, accounted, in typed order) and asks for the
                // decision again. The hand-off decides no request while the hold is live.
                let mut released_queued_hold = false;
                if FeatureFlag::TypeaheadHandoff.is_enabled() {
                    let queued_hold = {
                        let mut model = me.terminal_model.lock();
                        let mut handoff = model.typeahead_handoff_mut();
                        handoff.release_queued_prompt_hold_on_signal()
                    };
                    if let Some(hold_id) = queued_hold {
                        released_queued_hold = true;
                        ctx.emit(PtyControllerEvent::PromptHoldReleased { hold_id });
                    }
                }
                // #1208: readline and zle enable bracketed paste as they start reading a line, so
                // the shell is at its prompt now, not 50 ms after precmd.
                me.activate_line_editor_on_ready_signal(released_queued_hold, ctx);
            }
            ModelEvent::Handler(AnsiHandlerEvent::UnsetBracketedPaste) => {
                me.set_bracketed_paste_enabled(false);
            }
            ModelEvent::LineEditorReading { session_id, .. } => {
                // #1401: PowerShell never enables bracketed paste, so its "ready after ^C" leg
                // ends here (the last hop of the chain; the clock stops).
                if let Some(sent) = me.ctrl_c_sent_at.take() {
                    warpui::input_order::log_hop("line_editor_ready_after_ctrl_c", || {
                        format!(
                            "via=line_editor_reading since_ctrl_c_ms={:.1}",
                            sent.elapsed().as_secs_f64() * 1e3
                        )
                    });
                }
                me.on_line_editor_reading_marker(*session_id, ctx);
            }
            ModelEvent::ContinuationPrompt { .. } => {
                me.on_continuation_prompt_marker(ctx);
            }
            // #1614: the pty reader applied the stream a young hold waited on, and the hand-off
            // decided again: the hold either ended as typeahead (the view writes its keys) or now
            // waits for the request (decided here, if the line editor is active).
            ModelEvent::YoungHoldReevaluated => {
                if FeatureFlag::TypeaheadHandoff.is_enabled() {
                    me.announce_young_hold_end(ctx);
                    me.retry_typeahead_handoff_request(ctx);
                }
            }
            // The shell ran what the chord inserted (or anything else): nothing to retype (#1581).
            ModelEvent::Handler(AnsiHandlerEvent::Preexec) => {
                me.log_ctrl_c_to_next_command("preexec_after_ctrl_c", true);
                me.clear_file_insert();
                // #1614: a Preexec parsed after a young hold opened ended it as typeahead (on the
                // model thread); the view writes its keys now, on this event, not at the next key
                // or the cap.
                me.announce_young_hold_end(ctx);
            }
            ModelEvent::CommandFileFailed { session_id } => {
                let active = me
                    .model_event_dispatcher
                    .as_ref(ctx)
                    .active_session_id()
                    .map(|id| id.as_u64());
                if active != Some(session_id.as_u64()) {
                    log::warn!(
                        "CommandFileFailed for session {} while the active session is {:?}; retyping anyway",
                        session_id.as_u64(),
                        active
                    );
                }
                me.on_command_file_failed(ctx);
            }
            // The shell is back at a prompt and never ran the inserted command (no Preexec came):
            // nothing else will report it, so type it once (#1581).
            ModelEvent::Handler(AnsiHandlerEvent::Precmd) => {
                me.log_since_ctrl_c("precmd_after_ctrl_c");
                me.on_precmd_with_chord_outstanding(ctx);
                // #2182 (b) review m1: a parked user command whose start was refused at a write
                // (the line editor was active while the side call still held the block) is woken
                // by the side call's Precmd: `LineEditorStatus` emits nothing for a Precmd that
                // finds the editor already active, so nothing else would drain the queue.
                if me.has_parked_user_command() {
                    me.execute_next_queued_write(ctx);
                }
            }
            ModelEvent::Handler(AnsiHandlerEvent::StartTmuxControlMode) => {
                me.tmux_control_mode = Some(TmuxControlMode::Pending {
                    buffer: Default::default(),
                    overflowed: false,
                });
            }
            ModelEvent::Handler(AnsiHandlerEvent::RunTmuxCommand(command)) => {
                me.send_write_to_event_loop(PtyWrite::TmuxCommand(command.to_owned()), ctx);
            }
            ModelEvent::Handler(AnsiHandlerEvent::EndTmuxControlMode) => {
                me.tmux_control_mode = None;
            }
            ModelEvent::Exit { .. } => {
                // The shell is gone: the rest of a paced bootstrap has nowhere to go.
                me.cancel_paced_writes(ctx);
                if me.handoff_held_command_pending {
                    me.drop_held_handoff_command();
                }
                me.drop_parked_user_commands("shell_exit", false, ctx);
            }
            ModelEvent::HonorPS1OutOfSync => {
                // We force re-sync the PS1 state of Zap settings with the shell's environment variable, $WARP_HONOR_PS1, via
                // a bindkey (which triggers a shell function).
                let honor_ps1 = *SessionSettings::as_ref(ctx).honor_ps1;
                if honor_ps1 {
                    me.send_switch_to_ps1_bindkey(ctx);
                } else {
                    me.send_switch_to_warp_prompt_bindkey(ctx);
                }
            }
            ModelEvent::Handler(AnsiHandlerEvent::TmuxControlModeReady { primary_pane }) => {
                let previous_control_mode_state = me.tmux_control_mode.replace(TmuxControlMode::Active {
                        primary_pane: *primary_pane,
                    });
                if let Some(TmuxControlMode::Pending { buffer, .. }) = previous_control_mode_state {
                    me.send_write_to_event_loop(
                        PtyWrite::Bytes {
                            bytes: Cow::Owned(buffer),
                        },
                        ctx,
                    );
                }
            }
            ModelEvent::CompletionsFinished(data) => {
                // Check the variant *before* taking. `.take()` unconditionally empties the slot
                // and returns the old value; a late reply for a superseded request must not
                // destroy a newer, still-live request's state (TODO.md, "The shell lockup").
                if !matches!(
                    me.in_flight_native_completions_state,
                    Some(NativeShellCompletionsState::AwaitingResults { .. })
                ) {
                    log::warn!(
                        "Received CompletionsFinished event but wasn't awaiting results \
                         (generation {}); ignoring it without touching the in-flight state.",
                        me.native_completions_generation
                    );
                    return;
                }
                let Some(NativeShellCompletionsState::AwaitingResults { results_tx }) =
                    me.in_flight_native_completions_state.take()
                else {
                    unreachable!("checked above");
                };
                let _ = block_on(results_tx.send(data.clone()));
                for waiter in me.native_attached_waiters.drain(..) {
                    let _ = block_on(waiter.send(data.clone()));
                }
            }
            ModelEvent::SendCompletionsPrompt { token } => {
                // Same peek-before-take reasoning as `CompletionsFinished` above. With no live
                // `AwaitingPrompt` the app has already written this exchange's frame (the cancel,
                // at the prompt timeout), which the shell's read will find: write NOTHING more
                // (#770, and the framed exchange above).
                if !matches!(
                    me.in_flight_native_completions_state,
                    Some(NativeShellCompletionsState::AwaitingPrompt { .. })
                ) {
                    // A late prompt after the give-up: the cancel frame is owed, and this is
                    // the shell's own event, so it is written now (once) and the debt cleared.
                    if me.native_cancel_owed {
                        me.cancel_native_exchange_before_write(ctx);
                        me.execute_next_queued_write(ctx);
                        return;
                    }
                    log::warn!(
                        "Received SendCompletionsPrompt event but wasn't awaiting a prompt \
                         (generation {}); ignoring it without touching the in-flight state.",
                        me.native_completions_generation
                    );
                    return;
                }
                // A prompt that does not name its exchange, or names one already seen (a replay,
                // #1801 review M3), cannot be answered safely: cancel instead.
                let Some(token) = token.clone().filter(|token| me.note_native_prompt_token(token))
                else {
                    log::warn!("Native completions prompt without a fresh token; cancelling");
                    me.end_native_exchange_with_cancel(false, ctx);
                    return;
                };
                let Some(NativeShellCompletionsState::AwaitingPrompt {
                    buffer_text,
                    results_tx,
                    ..
                }) = me.in_flight_native_completions_state.take()
                else {
                    unreachable!("checked above");
                };
                me.in_flight_native_completions_state =
                    Some(NativeShellCompletionsState::AwaitingResults { results_tx });
                me.arm_native_completions_watchdog(NATIVE_COMPLETIONS_RESULTS_TIMEOUT, false, ctx);

                // The answer frame, one write. The state is already `AwaitingResults`, so this
                // write does not cancel the exchange (design point b).
                me.send_write_to_event_loop(
                    PtyWrite::Bytes {
                        bytes: native_completions_answer(&token, &buffer_text).into(),
                    },
                    ctx,
                );

                // Nothing is ever queued because of the exchange; this only keeps the queue moving.
                me.execute_next_queued_write(ctx);
            }
            _ => (),
        }
        });

        ctx.subscribe_to_model(&line_editor_status, |me, event, ctx| {
            if let LineEditorStatusEvent::GateOpened = event {
                // #1825: the prompt event opened the write gate; the request decision of a
                // session that signals waits for its signal (`Active` follows). Queued writes go
                // now: nothing waits for a signal that a user setting may have turned off.
                me.execute_next_queued_write(ctx);
                return;
            }
            if let LineEditorStatusEvent::Active = event {
                me.log_ctrl_c_to_next_command("line_editor_active_after_ctrl_c", false);
                if FeatureFlag::TypeaheadHandoff.is_enabled() {
                    me.decide_typeahead_handoff_request(false, ctx);
                    return;
                }
                let proven = me.line_editor_status.as_ref(ctx).is_proven_reading();
                let input_reporting_seq = me
                    .model_event_dispatcher
                    .as_ref(ctx)
                    .active_session_id()
                    .and_then(|id| me.sessions.as_ref(ctx).get(id))
                    .and_then(|session| session.shell().input_reporting_sequence())
                    .filter(|_| proven);
                if let Some(bytes) = input_reporting_seq {
                    me.pending_writes.push_front(PtyWrite::Bytes {
                        bytes: Cow::Owned(bytes.to_vec()),
                    });
                }
                me.execute_next_queued_write(ctx);
            }
        });

        let _ = ctx.spawn_stream_local(
            executor_command_rx,
            |me, event, ctx| match event {
                ExecutorCommandEvent::ExecuteCommand { command, cancel_tx } => {
                    me.queue_in_band_command(
                        command.command.as_str(),
                        command.shell_type,
                        command.command_id,
                        cancel_tx,
                        ctx,
                    );
                }
                ExecutorCommandEvent::CancelCommand { id } => {
                    me.cancel_in_band_command(id.as_str());
                }
                ExecutorCommandEvent::ExecuteTmuxCommand(command) => {
                    me.send_write_to_event_loop(PtyWrite::TmuxCommand(command), ctx);
                }
            },
            |_, _| (),
        );

        // #2182: the pane's queue wakes this controller when the user's held keys are released,
        // so an in-band command that waited for them goes out (event driven, nothing polls).
        {
            let (keys_released_tx, keys_released_rx) = async_channel::bounded::<()>(1);
            terminal_model
                .lock()
                .typeahead_handoff()
                .view_keys_cell()
                .set_waker(Some(keys_released_tx));
            let _ = ctx.spawn_stream_local(
                keys_released_rx,
                |me, _, ctx| me.execute_next_queued_write(ctx),
                |_, _| (),
            );
        }

        let write_queue = event_loop_tx.supports_write_queue() && write_queue_enabled();
        let (handoff, model_inbox, pty_writes) = {
            let model = terminal_model.lock();
            let pty_writes = model.typeahead_handoff().pty_write_count();
            (model.pane_handoff(), model.model_inbox(), pty_writes)
        };
        Self {
            event_loop_tx,
            terminal_model,
            line_editor_status,
            sessions,
            model_event_dispatcher,
            pending_writes: VecDeque::new(),
            is_user_command_executing: false,
            is_bracketed_paste_enabled: false,
            #[cfg(not(target_family = "wasm"))]
            bootstrap_file: None,
            #[cfg(not(target_family = "wasm"))]
            runs_long_commands_from_file: false,
            #[cfg(not(target_family = "wasm"))]
            long_command_pipe: None,
            pipe_sessions: Default::default(),
            command_pipe: None,
            file_insert_pending: None,
            file_insert_block: None,
            force_typed_command: false,
            #[cfg(test)]
            fast_insert_off_for_test: false,
            tmux_control_mode: None,
            in_flight_native_completions_state: None,
            native_completions_generation: 0,
            has_reported_native_completions_unavailable: false,
            last_native_cancel_toasted: false,
            native_cancel_owed: false,
            native_inflight_buffer: String::new(),
            native_attached_waiters: Vec::new(),
            seen_native_prompt_tokens: Default::default(),
            foreground_sigint: None,
            pty_write_gate_blocked_since: None,
            has_logged_current_pty_write_stall: false,
            handoff_command_pending: false,
            handoff_bytes_after_command: Vec::new(),
            handoff,
            pty_writes,
            model_inbox,
            handoff_request_waiting: false,
            handoff_parked_for_proof: false,
            handoff_held_command_pending: false,
            in_band_cancels: Default::default(),
            in_band_wait_armed: false,
            in_band_wait_generation: 0,
            in_band_wait_set_waiting: false,
            handoff_command_written_since_hold: false,
            paced_write_queue: VecDeque::new(),
            paced_write_generation: 0,
            write_queue,
            ctrl_c_sent_at: None,
            ctrl_c_next_command_at: None,
            setup_job_sent: false,
            setup_jobs_live: Default::default(),
            #[cfg(test)]
            shutdown_requested: false,
        }
    }

    /// Writes the shared `handoff_request_waiting` flag for one of its owners other than the
    /// in-band wait (a request decision, a parked decision, a held command, the pass that found
    /// nothing held). From here on the flag is theirs: the in-band wait's own mark goes off, so
    /// the end of that wait cannot clear a waiting it did not set (#2182, r29 A2).
    fn set_handoff_request_waiting(&mut self, waiting: bool) {
        self.handoff_request_waiting = waiting;
        self.in_band_wait_set_waiting = false;
    }

    /// The in-band wait is over without its command having been written (it was cancelled,
    /// cleared with the queue, or given up on): the give-up timer armed for it is void, and the
    /// waiting it set, if it alone set it, is taken back. A waiting another owner set since (the
    /// #2187 park, `ActiveDecision::Wait`, a held command) is left alone (#2182, r29 A2).
    fn end_in_band_wait(&mut self) {
        if self.in_band_wait_armed {
            self.in_band_wait_armed = false;
            self.in_band_wait_generation = self.in_band_wait_generation.wrapping_add(1);
        }
        if std::mem::take(&mut self.in_band_wait_set_waiting) {
            self.handoff_request_waiting = false;
        }
    }

    /// Whether the next queued write is an in-band command.
    fn front_is_in_band_command(&self) -> bool {
        matches!(
            self.pending_writes.front(),
            Some(PtyWrite::Command {
                in_band_command_id: Some(_),
                ..
            })
        )
    }

    /// Empties `pending_writes` and tells the requester of every in-band command that was in it
    /// that the command will not run, as the give-up of the in-band wait does (cf0aea183). The
    /// senders are taken BEFORE the commands are dropped: a queued command holds the only strong
    /// copy of its sender, so afterwards the requester would only see its channel close.
    fn clear_pending_writes_cancelling_in_band(&mut self) {
        let command_ids: Vec<String> = self
            .pending_writes
            .iter()
            .filter_map(|write| match write {
                PtyWrite::Command {
                    in_band_command_id: Some(command_id),
                    ..
                } => Some(command_id.clone()),
                _ => None,
            })
            .collect();
        let mut cancelled: Vec<(String, Sender<InBandCommandCancelledEvent>)> = Vec::new();
        for command_id in command_ids {
            if let Some(cancel_tx) = self
                .in_band_cancels
                .remove(&command_id)
                .and_then(|cancel| cancel.upgrade())
            {
                cancelled.push((command_id, cancel_tx));
            }
        }
        // #2182 (b): a user command parked behind a side call stays; it is not this caller's.
        self.pending_writes.retain(is_parked_user_command);
        self.end_in_band_wait();
        for (command_id, cancel_tx) in cancelled {
            if let Err(err) = block_on(cancel_tx.send(InBandCommandCancelledEvent { command_id })) {
                log::warn!("Pty Controller failed to cancel in band command: {err:?}");
            }
        }
    }

    /// Typeahead hand-off: decides, on the main thread and under the model lock, whether the
    /// shell may be asked for its line buffer now, and writes the request if so. `is_retry` is
    /// set when this re-runs a decision that had to wait, rather than answering a fresh
    /// activation of the shell's line editor.
    fn decide_typeahead_handoff_request(&mut self, is_retry: bool, ctx: &mut ModelContext<Self>) {
        // #1349: keys held since the prompt's precmd are released by the line editor's ready
        // signal (see the `SetBracketedPaste` handler), never by this activation: the fallback
        // timer behind it is not a signal.
        // The editor's command is still queued behind this activation: write it (and the keys
        // typed after it) first. The decision belongs to the prompt that follows it.
        // #2182 (b): the same for the editor's command parked behind the app's own in-band
        // command: it is written at this prompt, and the hand-off engages at that write.
        if self.handoff_command_pending || self.front_parked_command_expects_handoff() {
            self.execute_next_queued_write(ctx);
            return;
        }

        // #1798/#1672: the request (ESC i) is written only to a line editor the shell's own ready
        // signal proved to be reading at this prompt. The fallback that opens the write gate
        // without the signal (a shell still recovering from a ^C, a nested shell, a PS2) proves
        // nothing: a shell not reading echoes the request as `^[i`. Without the proof the
        // hand-off decides as for a shell with no request binding, and the keys go as typed.
        self.handoff_parked_for_proof = false;
        let editor_proven = self.line_editor_status.as_ref(ctx).is_proven_reading();
        if !editor_proven {
            warpui::input_order::log_hop("handoff_request_skipped", || {
                "reason=line_editor_not_proven".to_owned()
            });
        }
        let reporting_binding = self
            .model_event_dispatcher
            .as_ref(ctx)
            .active_session_id()
            .and_then(|id| self.sessions.as_ref(ctx).get(id))
            .and_then(|session| session.shell().input_reporting_sequence());
        let has_reporting_binding = reporting_binding.is_some();
        let input_reporting_seq = reporting_binding.filter(|_| editor_proven);
        // #2187: keys typed after a Ctrl-C, before the app saw the prompt, are in the shell's
        // line. Handing input to the editor now (nothing can be requested yet) strands them: the
        // editor's command write kills the line, and the first letters of the next command are
        // lost. The shell can report its line, so the decision waits for its ready signal (an
        // event, no timer) and the keys stay in the pty, in typed order, meanwhile.
        if !editor_proven
            && has_reporting_binding
            && self.handoff.read().typed_line_awaits_proven_editor()
        {
            warpui::input_order::log_hop("handoff_decision_parked", || {
                "reason=typed_line_awaits_proof".to_owned()
            });
            self.handoff_parked_for_proof = true;
            self.set_handoff_request_waiting(true);
            self.execute_next_queued_write(ctx);
            return;
        }
        // The one model read is the exact "at a prompt" fact (the decision depends on it, so it is
        // not a published copy); the hand-off is mutated through its own handle, after the model's
        // lock is released, so the decision never holds the model while it takes the hand-off.
        let at_prompt = self
            .terminal_model
            .lock()
            .is_at_prompt_for_typeahead_request();
        let decision = self
            .handoff
            .write()
            .on_line_editor_active(at_prompt, input_reporting_seq.is_some());

        warpui::input_order::log_hop("handoff_decision", || {
            format!("decision={decision:?} retry={is_retry}")
        });
        // #1614: a decision that found no request possible for a live young hold ended it as
        // typeahead; the view writes its keys.
        self.announce_young_hold_end(ctx);
        match decision {
            ActiveDecision::Legacy if !is_retry => {
                self.set_handoff_request_waiting(false);
                if let Some(bytes) = input_reporting_seq {
                    // #1089: its report covers user bytes the hand-off did not adopt.
                    self.handoff.write().on_legacy_request_written();
                    self.pending_writes.push_front(PtyWrite::Bytes {
                        bytes: Cow::Owned(bytes.to_vec()),
                    });
                }
                self.execute_next_queued_write(ctx);
            }
            ActiveDecision::Legacy | ActiveDecision::Drain => {
                // #1104: a report came back while user bytes written behind its request are
                // still in the shell's line. The shell reports its line as of the request and
                // clears it, so those bytes would stay there; one more request, behind them,
                // collects them. Queued commands go out behind it, not ahead of the bytes.
                let drain_request = if is_retry && decision == ActiveDecision::Legacy {
                    input_reporting_seq
                        .filter(|_| self.handoff.write().drain_request_due(at_prompt, true))
                } else {
                    None
                };
                if let Some(bytes) = drain_request {
                    self.pending_writes.push_front(PtyWrite::Bytes {
                        bytes: Cow::Owned(bytes.to_vec()),
                    });
                } else if self.handoff_held_command_pending
                    && self.handoff.read().holds_queued_writes()
                {
                    // A command held behind untracked user bytes stays held until a request
                    // behind them is written; its kill prefix would destroy them unreported.
                    self.set_handoff_request_waiting(true);
                    return;
                }
                self.set_handoff_request_waiting(false);
                self.execute_next_queued_write(ctx);
            }
            ActiveDecision::Committed => {
                self.set_handoff_request_waiting(false);
                self.execute_next_queued_write(ctx);
                ctx.emit(PtyControllerEvent::TypeaheadHandoffChanged);
            }
            ActiveDecision::SendRequest => {
                self.set_handoff_request_waiting(false);
                if let Some(bytes) = input_reporting_seq {
                    self.pending_writes.push_front(PtyWrite::Bytes {
                        bytes: Cow::Owned(bytes.to_vec()),
                    });
                }
                self.execute_next_queued_write(ctx);
                ctx.emit(PtyControllerEvent::TypeaheadHandoffChanged);
            }
            ActiveDecision::DrainAndWait {
                activity_generation,
            } => {
                self.set_handoff_request_waiting(false);
                self.execute_next_queued_write(ctx);
                self.arm_typeahead_handoff_stale_count_reset(activity_generation, ctx);
            }
            ActiveDecision::Wait {
                activity_generation,
            } => {
                self.set_handoff_request_waiting(true);
                self.arm_typeahead_handoff_stale_count_reset(activity_generation, ctx);
            }
        }
    }

    /// The shell said its line editor is reading a line (`LineEditorReading`, #1390): an explicit
    /// ready signal, which releases the keys held since the prompt's precmd (ConPTY, #1349) exactly
    /// as the bracketed-paste enable does. It works where that one never comes (readline with
    /// `enable-bracketed-paste off`), so the give-up cap of the hold is not reached there. The
    /// hand-off's own account of the marker (idempotent per prompt, in stream order) was applied
    /// on the model thread; releasing a hold twice, for the marker and for the enable, releases
    /// it once.
    fn on_line_editor_reading_marker(
        &mut self,
        session_id: SessionId,
        ctx: &mut ModelContext<Self>,
    ) {
        if !FeatureFlag::TypeaheadHandoff.is_enabled() {
            return;
        }
        let (queued_hold, reader_ended) = {
            let mut model = self.terminal_model.lock();
            let mut handoff = model.typeahead_handoff_mut();
            let reader_asked = handoff.prompt_reader_asks();
            let queued_hold = handoff.release_queued_prompt_hold_on_signal();
            let reader_ended = reader_asked && !handoff.prompt_reader_asks();
            (queued_hold, reader_ended)
        };
        let released_queued_hold = queued_hold.is_some();
        if let Some(hold_id) = queued_hold {
            ctx.emit(PtyControllerEvent::PromptHoldReleased { hold_id });
        }
        // #1401/#1402: PowerShell's marker is sent by its PSConsoleHostReadLine wrapper as the
        // last thing before PSReadLine reads, the same event the bracketed-paste enable is for
        // readline: it ends the activation wait instead of the 50 ms timer, under the same
        // conditions. On Windows PSReadLine reads raw console input records, so a request written
        // before it starts is neither echoed nor lost. On Unix pwsh the request can precede
        // ReadKey's tty setup, exactly as the 50 ms timer's request already could (the timer
        // never knew when PSReadLine started either). Only PowerShell: the other shells print the
        // marker with their prompt, where the tty may still be cooked (an early request would
        // echo), and they have the bracketed-paste signal.
        let is_powershell = self
            .sessions
            .as_ref(ctx)
            .get(session_id)
            .is_some_and(|session| session.shell().shell_type() == ShellType::PowerShell);
        // The marker is also the stale-count proof where nothing samples the shell (ConPTY),
        // under the same kill switch.
        self.terminal_model
            .lock()
            .typeahead_handoff_mut()
            .set_conpty_marker_proof(is_powershell && line_editor_reading_activates());
        if is_powershell && line_editor_reading_activates() {
            warpui::input_order::log_hop("line_editor_reading_activates", || {
                format!("released_queued_hold={released_queued_hold}")
            });
            self.activate_line_editor_on_ready_signal(released_queued_hold, ctx);
        } else if reader_ended && !released_queued_hold {
            // #2050: the marker ended a reader ahead of the line editor (a precmd function's
            // `read -s`) and nothing above decides for this shell. A session that also enables
            // bracketed paste decides at that signal (the retry does nothing while the decision
            // awaits it); one that does not (bash 3.2, `enable-bracketed-paste off`) has no later
            // event, so the hand-off decides now and the editor takes input back. The retry
            // writes a request only to a line editor proven to be reading.
            warpui::input_order::log_hop("prompt_reader_end_retry", String::new);
            self.retry_typeahead_handoff_request(ctx);
        }
    }

    /// The shell's line editor said it started reading a line (bracketed-paste enable, #1208, or
    /// the `LineEditorReading` marker, #1390): a pending activation is due now. Only while the
    /// hand-off can vouch that no typeahead line is queued ahead of a write: the line editor would
    /// run that line first and the write would land in the program it starts. With the keys still
    /// in the view's queue their Enters are not counted yet, so the line editor is not announced
    /// as reading until the view has released them (it asks for the decision again then).
    fn activate_line_editor_on_ready_signal(
        &mut self,
        released_queued_hold: bool,
        ctx: &mut ModelContext<Self>,
    ) {
        let was_active = self.line_editor_status.as_ref(ctx).is_line_editor_active();
        self.activate_line_editor_on_ready_signal_inner(released_queued_hold, ctx);
        // #2187: a decision parked for want of this proof (keys typed ahead sit in the shell's
        // line) is made now that the shell has signalled that its line editor reads.
        if self.handoff_parked_for_proof
            && FeatureFlag::TypeaheadHandoff.is_enabled()
            && self.line_editor_status.as_ref(ctx).is_proven_reading()
        {
            warpui::input_order::log_hop("handoff_parked_decision_retry", String::new);
            self.retry_typeahead_handoff_request(ctx);
        }
        // A Ctrl-C at a prompt on a non-seq session left the line editor active (no Precmd, so no
        // `Active` event): its ready signal, restarting the line, is the event that hands input
        // back -- the decision runs now, instead of the old 250 ms settle timer committing on a
        // guess. (`retry_` only decides while the line editor is active.)
        if was_active
            && FeatureFlag::TypeaheadHandoff.is_enabled()
            && self
                .terminal_model
                .lock()
                .typeahead_handoff_mut()
                .take_interrupt_settle_pending()
        {
            warpui::input_order::log_hop("interrupt_settle_signal", String::new);
            self.retry_typeahead_handoff_request(ctx);
        }
    }

    fn activate_line_editor_on_ready_signal_inner(
        &mut self,
        released_queued_hold: bool,
        ctx: &mut ModelContext<Self>,
    ) {
        if FeatureFlag::TypeaheadHandoff.is_enabled()
            && !released_queued_hold
            && !self
                .terminal_model
                .lock()
                .typeahead_handoff()
                .has_unanswered_lines_for_activation()
        {
            self.line_editor_status
                .update(ctx, |status, ctx| status.note_shell_reading_line(ctx));
        } else {
            // The signal cannot end the activation wait itself (#1349): fall back to the
            // default delay now that the keys held for it are released.
            self.line_editor_status
                .update(ctx, |status, ctx| status.expedite_pending_activation(ctx));
        }
    }

    /// The shell showed a continuation prompt (`ContinuationPrompt`, #1391): the hand-off
    /// re-baselined its line counts on this event (model thread, in stream order); a request
    /// decision that was waiting on counts that disagreed is decided again now, not after the
    /// give-up of the stale counts.
    fn on_continuation_prompt_marker(&mut self, ctx: &mut ModelContext<Self>) {
        if !FeatureFlag::TypeaheadHandoff.is_enabled() {
            return;
        }
        // #1573: the Enter of the ConPTY enter window ended a line the shell continues (PS2): its
        // line editor reads again, so the keys held since that Enter are released now, in typed
        // order, by this event (before, only the 1.5 s give-up released them, and it switched the
        // prompt hold off for the session).
        let held_for_continuation = self
            .terminal_model
            .lock()
            .typeahead_handoff_mut()
            .release_enter_window_for_continuation();
        if let Some(hold_id) = held_for_continuation {
            ctx.emit(PtyControllerEvent::PromptHoldReleased { hold_id });
        }
        self.retry_typeahead_handoff_request(ctx);
    }

    /// Re-runs a typeahead hand-off request decision that had to wait, if the shell's line
    /// editor is still active.
    pub fn retry_typeahead_handoff_request(&mut self, ctx: &mut ModelContext<Self>) {
        // A ready signal that could not open the gate when it came (keys released but not counted
        // yet, a line queued ahead) opens it on this event, not on a timer; the activation runs
        // the decision itself.
        if self.activate_remembered_ready_signal(ctx) {
            return;
        }
        let status = self.line_editor_status.as_ref(ctx);
        // The gate is open but this prompt's decision waits for the shell's signal: it decides
        // when the signal arrives (`Active`), not on this retry.
        if status.is_line_editor_active() && !status.decision_awaits_signal() {
            self.decide_typeahead_handoff_request(true, ctx);
        }
    }

    /// Starts the decision for a ready signal remembered by `expedite_pending_activation`, once the
    /// hand-off can vouch that no typeahead line is queued ahead of a write. Returns whether the
    /// line editor was activated (its `Active` event ran the decision).
    fn activate_remembered_ready_signal(&mut self, ctx: &mut ModelContext<Self>) -> bool {
        if !self
            .line_editor_status
            .as_ref(ctx)
            .signal_activation_pending()
        {
            return false;
        }
        if FeatureFlag::TypeaheadHandoff.is_enabled()
            && self
                .terminal_model
                .lock()
                .typeahead_handoff()
                .has_unanswered_lines_for_activation()
        {
            return false;
        }
        self.line_editor_status
            .update(ctx, |status, ctx| status.activate_pending_signal(ctx))
    }

    fn arm_typeahead_handoff_stale_count_reset(
        &mut self,
        activity_generation: u64,
        ctx: &mut ModelContext<Self>,
    ) {
        // #1353 phase 6: ask the pty now, at the moment the counts disagree with the prompt, who
        // owns the terminal. A program in the foreground (or reading a password) consumed the
        // lines the counts consider queued, so they are answered; no waiting for a quiet period.
        let (seq_mode, program_consumed_them) = {
            let mut model = self.terminal_model.lock();
            let handoff = model.typeahead_handoff_mut();
            let seq_mode = handoff.prompt_seq_mode();
            let program = seq_mode
                && handoff.counts_diverged()
                && handoff.sample_prompt_fact() == ShellPromptFact::NotAtPrompt;
            (seq_mode, program)
        };
        if program_consumed_them {
            let did_reset = self
                .terminal_model
                .lock()
                .typeahead_handoff_mut()
                .reset_stale_counts(activity_generation);
            warpui::input_order::log_hop("handoff_stale_counts_reset", || {
                format!("by=terminal_owner reset={did_reset}")
            });
            if did_reset {
                // The counts agree now, so this decision cannot arm another reset.
                self.retry_typeahead_handoff_request(ctx);
                return;
            }
        }
        let _ = seq_mode;
        // give-up-net: logs a stale-count wait only; no reset, no retry, no write, no routing
        ctx.spawn(
            async move {
                Timer::after(TYPEAHEAD_HANDOFF_STALE_COUNT_GIVE_UP).await;
            },
            move |me, _, _ctx| {
                // A logged give-up only: no reset, no retry, no toast, no write.
                let (still_waiting, activity, markers_live) = {
                    let model = me.terminal_model.lock();
                    let handoff = model.typeahead_handoff();
                    (
                        handoff.counts_diverged()
                            && handoff.activity_generation() == activity_generation,
                        handoff.shell_activity(),
                        handoff.line_editor_markers_live(),
                    )
                };
                if still_waiting {
                    log::warn!(
                        "typeahead hand-off: no prompt, command start or terminal-owner change \
                         within {} s for lines the counts consider queued; keeping them (the pty \
                         keeps input until the next event)",
                        TYPEAHEAD_HANDOFF_STALE_COUNT_GIVE_UP.as_secs()
                    );
                    warpui::input_order::log_hop("handoff_stale_counts_kept", || {
                        format!("shell_activity={activity:?} markers_live={markers_live}")
                    });
                }
            },
        );
    }

    /// Writes user bytes the typeahead hand-off routed to the pty and counted as in flight, then
    /// retires them from the in-flight count. Bytes typed behind a still-queued command from the
    /// input editor follow that command instead of overtaking it.
    pub fn write_handoff_user_bytes(
        &mut self,
        bytes: Cow<'static, [u8]>,
        ctx: &mut ModelContext<Self>,
    ) {
        self.write_handoff_user_bytes_inner(bytes, true, ctx);
    }

    /// As [`Self::write_handoff_user_bytes`]. `may_retry` is whether the write may re-run a
    /// waiting request decision; the decision that is releasing held keys passes `false`, as it
    /// is about to decide itself.
    fn write_handoff_user_bytes_inner(
        &mut self,
        bytes: Cow<'static, [u8]>,
        may_retry: bool,
        ctx: &mut ModelContext<Self>,
    ) {
        // An Enter voids any quiet-period reset armed before it, so decide again (which arms a
        // fresh one) rather than leave nobody to.
        let has_enter = bytes.iter().any(|byte| matches!(byte, b'\r' | b'\n'));
        if crate::terminal::model_lock_trace::key_path_uses_model_lock() {
            // #1033: the tty echoes these keys wherever the output is when they arrive; record
            // them so that echo can be told from output. A full-screen program reads them raw.
            let mut model = self.terminal_model.lock();
            if model.is_alt_screen_active() {
                model.block_list_mut().forget_typed_ahead_bytes();
            } else {
                model.block_list_mut().record_typed_ahead_bytes(&bytes);
            }
        } else {
            // The same record, left in the model's inbox instead of written under its lock (the
            // pty reader holds that lock while it parses, and this write waited for it, #1403).
            // The model applies it before it parses the next bytes, and the echo of these bytes
            // cannot be parsed before they are written, which happens below.
            self.model_inbox
                .post(ModelCommand::TypedAhead(bytes.to_vec()));
        }
        // #2182 (b) review m5: a ^C is never kept behind a parked command; it drops it (and
        // lets the keys typed behind it go first) and is written now.
        if bytes.as_ref() == [escape_sequences::C0::ETX] {
            self.drop_parked_user_commands_on_interrupt(ctx);
        }
        let deferred_behind_command = self.handoff_command_pending;
        let byte_count = bytes.len();
        if deferred_behind_command {
            self.handoff_bytes_after_command.extend_from_slice(&bytes);
        } else if byte_count >= PASTE_WRITE_MIN_BYTES {
            self.write_paste_bytes(bytes, ctx);
        } else {
            self.write_bytes(bytes, ctx);
        }
        let writes_in_flight = self.handoff.write().on_user_write_flushed();
        // #1162: where each typed-ahead write went and what was still in flight behind it.
        warpui::input_order::log_hop("handoff_user_write", || {
            format!(
                "bytes={byte_count} enter={has_enter} deferred_behind_command={deferred_behind_command} \
                 in_flight={writes_in_flight} request_waiting={}",
                self.handoff_request_waiting
            )
        });
        if may_retry && writes_in_flight == 0 && (self.handoff_request_waiting || has_enter) {
            self.retry_typeahead_handoff_request(ctx);
        }
    }

    fn flush_handoff_bytes_after_command(&mut self, ctx: &mut ModelContext<Self>) {
        self.handoff_command_pending = false;
        let bytes = std::mem::take(&mut self.handoff_bytes_after_command);
        if !bytes.is_empty() {
            self.write_bytes(bytes, ctx);
        }
    }

    /// Sends bindkey to notify shell process to switch to PS1 logic for prompt
    /// with the combined prompt/command grid (we restore the saved PS1 value).
    pub fn send_switch_to_ps1_bindkey(&mut self, ctx: &mut ModelContext<Self>) {
        self.pending_writes.push_back(PtyWrite::Bytes {
            bytes: SWITCH_TO_PS1_ESCAPE_SEQUENCE.into(),
        });
        self.execute_next_queued_write(ctx);

        let is_bash_shell = self
            .model_event_dispatcher
            .as_ref(ctx)
            .active_session_id()
            .and_then(|id| self.sessions.as_ref(ctx).get(id))
            .map(|session| session.shell().shell_type() == ShellType::Bash)
            .unwrap_or(false);
        if is_bash_shell {
            // We cannot repaint via shell command in bash, so we must execute an empty command to force refresh the prompt instantly
            // (avoid a 1 block delay since the current prompt has technically already been sent).
            self.pending_writes.push_back(PtyWrite::Bytes {
                bytes: COMMAND_ENTER.into(),
            });
            self.execute_next_queued_write(ctx);
        }
    }

    /// Sends bindkey to notify shell process to switch to Zap prompt logic for prompt
    /// with the combined prompt/command grid (we unset the PS1, but save the value for potential
    /// future restoration).
    pub fn send_switch_to_warp_prompt_bindkey(&mut self, ctx: &mut ModelContext<Self>) {
        self.pending_writes.push_back(PtyWrite::Bytes {
            bytes: SWITCH_TO_WARP_PROMPT_ESCAPE_SEQUENCE.into(),
        });
        self.execute_next_queued_write(ctx);

        let is_bash_shell = self
            .model_event_dispatcher
            .as_ref(ctx)
            .active_session_id()
            .and_then(|id| self.sessions.as_ref(ctx).get(id))
            .map(|session| session.shell().shell_type() == ShellType::Bash)
            .unwrap_or(false);
        if is_bash_shell {
            // We cannot repaint via shell command in bash, so we must execute an empty command to force refresh the prompt instantly
            // (avoid a 1 block delay since the current prompt has technically already been sent).
            self.pending_writes.push_back(PtyWrite::Bytes {
                bytes: COMMAND_ENTER.into(),
            });
            self.execute_next_queued_write(ctx);
        }
    }

    fn cancel_in_band_command(&mut self, command_id: &str) {
        self.in_band_cancels.remove(command_id);
        // A timer armed for a wait of the command just cancelled is void.
        if self.in_band_wait_armed {
            self.in_band_wait_armed = false;
            self.in_band_wait_generation = self.in_band_wait_generation.wrapping_add(1);
        }
        self.pending_writes.retain(|pty_write| {
            !matches!(pty_write, PtyWrite::Command {
                 in_band_command_id, ..
             } if in_band_command_id.as_deref() == Some(command_id))
        });
        // #2182 (r29 A2): with no in-band command left at the front nothing waits for the keys
        // any more, so the waiting that wait set goes with it (it used to stay set with nothing
        // to wait for). Another in-band command at the front keeps it: its own wait is the same
        // wait, and the next pass re-arms the timer.
        if !self.front_is_in_band_command() {
            self.end_in_band_wait();
        }
    }

    /// Queues an in-band command to be written to the PTY, either immediately, or when the line
    /// editor next becomes active.
    ///
    /// If a user command is currently executing, this short-circuits since the in-band command
    /// request is likely stale. However, we still need to signal that the command will not be
    /// executed so the executor knows to clear it.
    fn queue_in_band_command(
        &mut self,
        command: &str,
        shell_type: ShellType,
        command_id: String,
        cancel_tx: Sender<InBandCommandCancelledEvent>,
        ctx: &mut ModelContext<Self>,
    ) {
        if self.is_user_command_executing {
            // Send blocking should be okay b/c this is an unbound channel
            if let Err(err) = block_on(cancel_tx.send(InBandCommandCancelledEvent { command_id })) {
                log::warn!("Pty Controller failed to cancel in band command: {err:?}");
            }
            return;
        }

        let terminal_model = self.terminal_model.clone();
        let callback_command_id = command_id.clone();
        self.in_band_cancels
            .retain(|_, cancel| cancel.upgrade().is_some());
        self.in_band_cancels
            .insert(command_id.clone(), cancel_tx.downgrade());
        self.pending_writes.push_back(PtyWrite::Command {
            command: command.to_owned(),
            shell_type,
            in_band_command_id: Some(command_id),
            park: CommandPark::None,
            before_write_fn: Some(Box::new(move || {
                let mut terminal_model = terminal_model.lock();
                let outcome = terminal_model.start_in_band_command_execution();
                if !outcome.is_accepted() {
                    if let Err(err) = block_on(cancel_tx.send(InBandCommandCancelledEvent {
                        command_id: callback_command_id.clone(),
                    })) {
                        log::warn!(
                            "Pty Controller failed to cancel rejected in band command: {err:?}"
                        );
                    }
                }
                outcome
            })),
        });

        self.execute_next_queued_write(ctx);
    }

    /// Arms a watchdog for an in-flight native-completions exchange.
    ///
    /// For the prompt phase it ends an exchange whose prompt does not come (a slow link, or a
    /// shell whose hook is not there after all) with the cancel frame, so the shell's hook, if it
    /// is reading, ends safely. No write is ever held by the exchange (design point b in "The
    /// in-band native-completions exchange"), so this is about the Tab's answer, not the pane:
    /// an older design gated writes on `AwaitingPrompt` and an unanswered `^Y` locked the pane.
    ///
    /// Dropping `results_tx` (by dropping the state) is deliberate and load-bearing: the
    /// requester at `terminal/input.rs` awaits `results_rx.recv()` with no timeout of its own, so
    /// closing the channel is what lets that future resolve to `None` instead of leaking.
    fn arm_native_completions_watchdog(
        &mut self,
        timeout: Duration,
        expect_awaiting_prompt: bool,
        ctx: &mut ModelContext<Self>,
    ) {
        self.native_completions_generation = self.native_completions_generation.wrapping_add(1);
        let generation = self.native_completions_generation;
        // give-up-net: the prompt arm closes the requester's channel and owes the cancel frame; the
        // results arm drops the exchange. Neither writes or releases a key.
        ctx.spawn(
            async move {
                Timer::after(timeout).await;
            },
            move |me, _, ctx| {
                // A later phase (or a later request entirely) has superseded this timer.
                if me.native_completions_generation != generation {
                    return;
                }
                let still_in_expected_phase = me
                    .in_flight_native_completions_state
                    .as_ref()
                    .is_some_and(|state| state.is_awaiting_prompt() == expect_awaiting_prompt);
                if !still_in_expected_phase {
                    return;
                }
                if expect_awaiting_prompt {
                    // No prompt in time (a slow link, a VPN stall, or no hooks): GIVE UP. Stop
                    // waiting and write nothing; the cancel frame is owed and goes in front of
                    // the next real write (see "The in-band native-completions exchange").
                    log::warn!(
                        "Native shell completions prompt not seen after {timeout:?}; giving up \
                         (the Tab falls back to the completer, the cancel frame is owed)"
                    );
                    me.last_native_cancel_toasted = me.abandon_native_exchange(ctx);
                    return;
                }
                // Never log the buffer text -- it is whatever the user was typing.
                log::warn!(
                    "Native shell completions results timed out after {timeout:?}; abandoning \
                     the request."
                );
                // Nothing is ever queued behind the exchange, so there is nothing to release.
                me.in_flight_native_completions_state = None;
                me.native_attached_waiters.clear();
            },
        );
    }

    /// The prompt timeout: stops waiting for the prompt, WRITES NOTHING. Dropping the state
    /// closes the requester's channel (it falls back to the completer) and the cancel frame
    /// becomes owed. Returns whether the "no shell integration" toast was asked for (never for a
    /// remote-first request).
    fn abandon_native_exchange(&mut self, ctx: &mut ModelContext<Self>) -> bool {
        let Some(NativeShellCompletionsState::AwaitingPrompt { remote_first, .. }) =
            self.in_flight_native_completions_state.take()
        else {
            return false;
        };
        self.native_attached_waiters.clear();
        self.native_cancel_owed = true;
        if remote_first {
            return false;
        }
        self.report_native_completions_unavailable(ctx);
        true
    }

    /// Ends an exchange still waiting for its prompt: writes the cancel frame (the one frame this
    /// exchange gets) and drops the request (its requester falls back to the completer).
    ///
    /// `report` shows the "no shell integration" toast for a request that is not remote-first: a
    /// prompt that never came there is what an un-phosphorized shell looks like; a remote-first
    /// session has the hooks.
    ///
    /// Returns whether the toast was asked for (the decision; the toast itself needs a window).
    fn end_native_exchange_with_cancel(
        &mut self,
        report: bool,
        ctx: &mut ModelContext<Self>,
    ) -> bool {
        let Some(NativeShellCompletionsState::AwaitingPrompt {
            remote_first,
            results_tx,
            ..
        }) = self.in_flight_native_completions_state.take()
        else {
            return false;
        };
        // The cancel IS an answer (of nothing): the requester then runs an Enter held for it on
        // the typed line. A give-up (`abandon_native_exchange`) sends nothing: the channel just
        // closes, "no answer", and the held Enter is handed back instead.
        let _ = results_tx.try_send(Vec::new());
        for waiter in self.native_attached_waiters.drain(..) {
            let _ = waiter.try_send(Vec::new());
        }
        self.native_cancel_owed = false;
        self.send_write_to_event_loop(
            PtyWrite::Bytes {
                bytes: native_completions_cancel().into(),
            },
            ctx,
        );
        self.execute_next_queued_write(ctx);
        let toast = report && !remote_first;
        if toast {
            self.report_native_completions_unavailable(ctx);
        }
        toast
    }

    /// #1801: the shell an exchange asked is gone, or back at its prompt without answering (a
    /// ^C in its hook, an ssh that died, the pane's shell exiting): a Precmd, an ExitShell or
    /// an Exit while the exchange waits for its prompt OR for its results (the shell is back at
    /// its prompt, so results can no longer arrive; an event beats the 15 s net). The exchange
    /// just ends and its requesters see "no answer": no frame (the
    /// shell that would read it is not the one asked), and nothing was ever held behind its
    /// `^Y`, so there is nothing to release or hand back.
    fn end_native_exchange_if_its_shell_left(&mut self, event: &ModelEvent) {
        let shell_left = matches!(
            event,
            ModelEvent::Handler(AnsiHandlerEvent::Precmd)
                | ModelEvent::ExitShell { .. }
                | ModelEvent::Exit { .. }
        );
        if shell_left {
            self.native_cancel_owed = false;
        }
        if shell_left && self.in_flight_native_completions_state.is_some() {
            log::warn!("The shell asked for native completions left before answering; ending it");
            self.in_flight_native_completions_state = None;
            self.native_attached_waiters.clear();
        }
    }

    /// #2153: a ^C was written while an exchange waits for its RESULTS. The user abandoned the
    /// Tab (the shell's hook is interrupted and may never answer, and zsh may not run a Precmd
    /// for it), so the exchange must not stay in flight for the 15 s net and refuse the next
    /// Tab as "an exchange is already in flight". The exchange ends: its requesters see "no
    /// answer" (closed channels) and nothing is written. A reply that still arrives is ignored
    /// by the `CompletionsFinished` phase check, and it cannot be mistaken for a NEW exchange's
    /// answer: the shell is single-threaded, so the new exchange's prompt event (which moves it
    /// to awaiting-results) can only come after the old hook has returned, i.e. after any late
    /// reply, which finds the new exchange still waiting for its prompt and is ignored there
    /// (#770). An exchange still awaiting its prompt is not touched here: the write path
    /// cancels it with its frame (design point b).
    fn end_native_exchange_awaiting_results_on_interrupt(&mut self) {
        if !matches!(
            self.in_flight_native_completions_state,
            Some(NativeShellCompletionsState::AwaitingResults { .. })
        ) {
            return;
        }
        log::info!(
            "A ^C ends the native-completions exchange awaiting results (the Tab was abandoned)"
        );
        self.in_flight_native_completions_state = None;
        self.native_attached_waiters.clear();
    }

    /// #1801 (design point b): a write is about to go out while an exchange waits for its prompt.
    /// The cancel frame goes first: it ends the hook safely wherever it is (in its read, or at
    /// the prompt), and the write follows it in order. The Tab falls back to the completer.
    fn cancel_native_exchange_before_write(&mut self, ctx: &mut ModelContext<Self>) {
        if !self
            .in_flight_native_completions_state
            .as_ref()
            .is_some_and(NativeShellCompletionsState::is_awaiting_prompt)
            && !self.native_cancel_owed
        {
            return;
        }
        self.native_cancel_owed = false;
        log::info!(
            "A write during a native-completions exchange cancels it first (the Tab falls back \
             to the completer)"
        );
        // Taken first, so the cancel frame's own write below does not come back here.
        self.in_flight_native_completions_state = None;
        self.native_attached_waiters.clear();
        self.send_write_to_event_loop(
            PtyWrite::Bytes {
                bytes: native_completions_cancel().into(),
            },
            ctx,
        );
    }

    /// #1801 (design point a): whether an in-band exchange may start now. `Err` names why not;
    /// the Tab then goes to the completer at once.
    fn native_exchange_may_start(&self, ctx: &ModelContext<Self>) -> Result<(), &'static str> {
        if self.in_flight_native_completions_state.is_some() {
            return Err("an exchange is already in flight");
        }
        if self.native_cancel_owed {
            return Err("a given-up exchange still owes its cancel frame");
        }
        if !self.pending_writes.is_empty() {
            return Err("writes are queued");
        }
        if self.is_user_command_executing {
            return Err("a command is executing");
        }
        if !self.line_editor_status.as_ref(ctx).is_line_editor_active() {
            return Err("the line editor is not active");
        }
        if self.handoff_request_waiting
            || self.handoff_command_pending
            || self.handoff_held_command_pending
        {
            return Err("the typeahead hand-off is engaged");
        }
        // Setup writes in flight: the timer-chained chunks, or a `SetupChunked` job the writer
        // still holds (the write-queue path never fills `paced_write_queue`).
        if !self.paced_write_queue.is_empty()
            || self
                .setup_jobs_live
                .load(std::sync::atomic::Ordering::Acquire)
                > 0
        {
            return Err("setup writes are in flight");
        }
        if matches!(
            self.tmux_control_mode,
            Some(TmuxControlMode::Pending { .. })
        ) {
            return Err("tmux control mode is starting");
        }
        Ok(())
    }

    /// #1801 review M3: a prompt token is taken once per pane; a replayed or duplicated one is
    /// refused. Returns whether `token` is new (and remembers it).
    fn note_native_prompt_token(&mut self, token: &str) -> bool {
        if self.seen_native_prompt_tokens.contains(token) {
            return false;
        }
        if self.seen_native_prompt_tokens.len() >= NATIVE_COMPLETIONS_SEEN_TOKENS {
            self.seen_native_prompt_tokens.clear();
        }
        self.seen_native_prompt_tokens.insert(token.to_owned());
        true
    }

    /// Tells the user -- at most once per pane -- that this shell will not answer completion
    /// requests, so a dead Tab key has an explanation instead of looking like a hang.
    ///
    /// Reuses the workspace toast stack, the same path an OSC 9 notification from the shell
    /// takes on its way to the screen (`ModelEvent::PluggableNotification` ->
    /// `pane_group::Event::ShowToast`), reached here the way `MCPTemplatableManager` reaches it
    /// from a model: active window first, then the `ToastStack` singleton. Emitting
    /// `ModelEvent::PluggableNotification` directly was the other candidate and was rejected --
    /// every such event is fed to the CLI-agent listeners' `try_parse`
    /// (`terminal/cli_agent_sessions/listener/mod.rs`), so an internal diagnostic sent that way
    /// can mutate CLI-agent session state, and one with no title is silently swallowed whenever
    /// the pane already has an OSC 9 listener.
    ///
    /// `active_window()` is checked before anything else on purpose: it is `None` in headless
    /// and `App::test` apps, where the `ToastStack` singleton is never registered, so the guard
    /// both scopes the toast to a real window and keeps this recovery path away from a singleton
    /// lookup that has nothing to find there. The "already reported" flag is only set once a
    /// toast has actually been handed over, so a timeout that lands with no window does not
    /// consume the one notification the pane gets.
    fn report_native_completions_unavailable(&mut self, ctx: &mut ModelContext<Self>) {
        if self.has_reported_native_completions_unavailable {
            return;
        }
        let Some(window_id) = ctx.windows().active_window() else {
            return;
        };
        self.has_reported_native_completions_unavailable = true;
        ToastStack::handle(ctx).update(ctx, |toast_stack, ctx| {
            toast_stack.add_ephemeral_toast(
                DismissibleToast::default(NATIVE_COMPLETIONS_UNAVAILABLE_MESSAGE.to_owned()),
                window_id,
                ctx,
            );
        });
    }

    /// Returns whether we can currently write to the pty, or if we need to
    /// enqueue writes for later.
    fn can_write_to_pty(&self, ctx: &mut ModelContext<Self>) -> bool {
        self.line_editor_status.as_ref(ctx).is_line_editor_active()
    }

    /// Executes the next queued `PtyWrite`, if able.
    ///
    /// This is a no-op if the line editor is currently inactive; in the constructor of
    /// PtyController, a subscription is registered on `LineEditorStatus` which calls this function
    /// when the line editor becomes active.
    fn execute_next_queued_write(&mut self, ctx: &mut ModelContext<Self>) {
        if self.handoff_request_waiting {
            if self
                .terminal_model
                .lock()
                .typeahead_handoff()
                .holds_queued_writes()
            {
                return;
            }
            self.set_handoff_request_waiting(false);
        }
        if self.handoff_holds_front_command() {
            return;
        }
        if self.young_hold_holds_front_command() {
            return;
        }
        if self.in_band_front_waits_for_keys(ctx) {
            return;
        }
        if !self.can_write_to_pty(ctx) {
            self.track_pty_write_gate_stall(PTY_WRITE_GATE_STALL_LOG_THRESHOLD, ctx);
            return;
        }
        self.pty_write_gate_blocked_since = None;
        self.has_logged_current_pty_write_stall = false;

        if let Some(write) = self.pending_writes.pop_front() {
            let is_command = matches!(write, PtyWrite::Command { .. });
            let is_shell_command = matches!(
                write,
                PtyWrite::Command {
                    in_band_command_id: None,
                    ..
                }
            );
            let is_in_band_command = is_command && !is_shell_command;
            let is_held_handoff_command = is_held_handoff_command(&write);
            let parked_user_command = match &write {
                PtyWrite::Command {
                    park: CommandPark::BehindInBand(parked),
                    ..
                } => Some(ParkedUserCommand::clone(parked)),
                _ => None,
            };
            if let PtyWrite::Command {
                in_band_command_id: Some(id),
                ..
            } = &write
            {
                self.in_band_cancels.remove(id);
            }
            let did_write = self.send_write_to_event_loop(write, ctx);
            if let Some(parked) = parked_user_command
                && !self.settle_parked_user_command(parked, did_write, ctx)
            {
                // #2182 (b): its start was refused; it is back at the front and waits for the
                // next prompt. Draining on would only pop it again.
                return;
            }
            if is_held_handoff_command {
                if did_write {
                    self.handoff_held_command_pending = false;
                    self.terminal_model
                        .lock()
                        .typeahead_handoff_mut()
                        .set_held_agent_command(None);
                    // #2198: its `before_write_fn` told the hand-off; an end it recorded for a
                    // young hold is the view's to apply now.
                    self.announce_young_hold_end(ctx);
                } else {
                    log::warn!(
                        "A command held behind the user's typeahead could not start when it was \
                         released; dropping it."
                    );
                    self.forget_held_handoff_command();
                }
            }
            if did_write && is_shell_command && self.handoff_command_pending {
                self.flush_handoff_bytes_after_command(ctx);
            }
            // An in-band command's prompt is counted like any other, so its Enter must be too.
            if did_write && is_in_band_command && FeatureFlag::TypeaheadHandoff.is_enabled() {
                self.terminal_model
                    .lock()
                    .typeahead_handoff_mut()
                    .on_uncounted_command_written();
                self.announce_young_hold_end(ctx);
            }
            if !is_command || !did_write {
                self.execute_next_queued_write(ctx);
            }
        }
    }

    /// #2182: whether the next queued write is the app's own in-band command and must wait,
    /// because the user's keys are held in the view's queue, or the shell's line holds typed text
    /// or unreported bytes. Its kill prefix would wipe that line, and a write ahead of held keys
    /// would reorder them; the keys never move. It goes out after the request or drain that
    /// covers the bytes (`handoff_request_waiting` is set so the request is retried behind them),
    /// or when the view releases its keys (the pane's queue wakes this controller). A wait that
    /// outlasts [`INBAND_WAIT_FOR_KEYS_GIVE_UP`] cancels the command and writes nothing.
    fn in_band_front_waits_for_keys(&mut self, ctx: &mut ModelContext<Self>) -> bool {
        let front_is_in_band = FeatureFlag::TypeaheadHandoff.is_enabled()
            && matches!(
                self.pending_writes.front(),
                Some(PtyWrite::Command {
                    in_band_command_id: Some(_),
                    ..
                })
            );
        let waits = front_is_in_band && {
            let model = self.terminal_model.lock();
            let handoff = model.typeahead_handoff();
            handoff.inband_waits_for_keys()
                && (handoff.holds_queued_writes()
                    || handoff.view_holds_keys()
                    || handoff.shell_line_may_hold_typed_text())
        };
        if !waits {
            // Whatever ended the wait, a timer armed for it is void.
            if self.in_band_wait_armed {
                self.in_band_wait_armed = false;
                self.in_band_wait_generation = self.in_band_wait_generation.wrapping_add(1);
            }
            return false;
        }
        if !self.handoff_request_waiting {
            // Its own waiting, marked as such: only this wait's end takes it back (r29 A2).
            self.handoff_request_waiting = true;
            self.in_band_wait_set_waiting = true;
        }
        if !self.in_band_wait_armed {
            self.in_band_wait_armed = true;
            self.in_band_wait_generation = self.in_band_wait_generation.wrapping_add(1);
            let generation = self.in_band_wait_generation;
            warpui::input_order::log_hop("inband_waits_for_keys", || {
                format!("generation={generation}")
            });
            // give-up-net: cancels the app's own in-band command (its requester is told); no key is touched
            ctx.spawn(
                async move {
                    Timer::after(INBAND_WAIT_FOR_KEYS_GIVE_UP).await;
                },
                move |me, _, ctx| me.give_up_in_band_wait(generation, ctx),
            );
        }
        true
    }

    /// The give-up of [`Self::in_band_front_waits_for_keys`]: the front in-band command is
    /// cancelled (its requester is told) and not written; the user's keys are untouched.
    fn give_up_in_band_wait(&mut self, generation: u64, ctx: &mut ModelContext<Self>) {
        if !self.in_band_wait_armed || self.in_band_wait_generation != generation {
            return;
        }
        self.in_band_wait_armed = false;
        self.in_band_wait_generation = self.in_band_wait_generation.wrapping_add(1);
        let Some(PtyWrite::Command {
            in_band_command_id: Some(command_id),
            ..
        }) = self.pending_writes.front()
        else {
            return;
        };
        let command_id = command_id.clone();
        // Take the requester's sender BEFORE the command is dropped: the queued command holds
        // the only strong copy, so popping first made the upgrade fail and the requester only
        // saw its channel close (box test, r28).
        let cancel_tx = self
            .in_band_cancels
            .remove(&command_id)
            .and_then(|cancel| cancel.upgrade());
        self.pending_writes.pop_front();
        // #2182 (r29 A2): the wait is over; the waiting it set (and only that) goes with it. The
        // pass below sets it again if the next write is another in-band command that waits.
        self.end_in_band_wait();
        if let Some(cancel_tx) = cancel_tx {
            if let Err(err) = block_on(cancel_tx.send(InBandCommandCancelledEvent { command_id })) {
                log::warn!("Pty Controller failed to cancel in band command: {err:?}");
            }
        }
        warpui::input_order::log_hop("inband_wait_given_up", || {
            format!("generation={generation}")
        });
        log::warn!(
            "An in-band command waited too long for the user's held keys; cancelled it, the keys \
             are untouched."
        );
        self.execute_next_queued_write(ctx);
    }

    /// Diagnostics for the still-unexplained field lockup in TODO.md ("The shell lockup"): the
    /// reporter is on bash with no enabler for `FeatureFlag::NativeShellCompletions`, so the
    /// watchdog above cannot be what wedged their pane -- but `can_write_to_pty` has a second
    /// gate, the line editor being inactive, that has no watchdog and no diagnostics at all. This
    /// makes the next capture decisive: if the gate is shut for more than a few seconds while
    /// writes are queued, log once naming which gate is shut and how many writes are waiting,
    /// then stay quiet until the stretch ends (a fresh keystroke reopening the gate, or the queue
    /// draining) so a stuck pane cannot spam the log once per keystroke.
    ///
    /// Self-contained on purpose: it re-checks `can_write_to_pty` and re-derives everything else
    /// from current state, so it is safe to call from anywhere, including the one-shot timer
    /// below -- a stretch it was armed for that has since ended (or been superseded by a fresh
    /// one) just resolves to a no-op or a fresh, correctly-short `stalled_for`, never a stale log.
    ///
    /// `threshold` is a parameter (rather than always reading `PTY_WRITE_GATE_STALL_LOG_THRESHOLD`
    /// directly) purely so tests can drive it with a short duration instead of the real one, the
    /// same reason `arm_native_completions_watchdog` takes an explicit `timeout`; every production
    /// call site passes the real constant.
    ///
    /// Never logs write contents, only counts and booleans -- see #718 on log redaction.
    fn track_pty_write_gate_stall(&mut self, threshold: Duration, ctx: &mut ModelContext<Self>) {
        if self.can_write_to_pty(ctx) || self.pending_writes.is_empty() {
            self.pty_write_gate_blocked_since = None;
            self.has_logged_current_pty_write_stall = false;
            return;
        }

        let now = std::time::Instant::now();
        let is_new_stall = self.pty_write_gate_blocked_since.is_none();
        let blocked_since = *self.pty_write_gate_blocked_since.get_or_insert(now);

        if is_new_stall {
            // `execute_next_queued_write` only re-checks the gate when something drives it --
            // another queued write, or `LineEditorStatusEvent::Active`. If the user stops typing
            // entirely once the gate sticks (a very plausible reaction to a pane that stopped
            // responding), nothing would otherwise re-check until whatever input eventually comes
            // next, which may be long after the threshold or may never come at all if the pane
            // looks dead. Arm a one-shot timer, once per stall, so the log line fires on its own.
            ctx.spawn(
                async move {
                    // give-up-net: one-shot timer that only logs a stuck write gate
                    Timer::after(threshold).await;
                },
                move |me, _, ctx| {
                    me.track_pty_write_gate_stall(threshold, ctx);
                },
            );
        }

        if self.has_logged_current_pty_write_stall {
            return;
        }

        let stalled_for = now.saturating_duration_since(blocked_since);
        if stalled_for < threshold {
            return;
        }

        let line_editor_inactive = !self.line_editor_status.as_ref(ctx).is_line_editor_active();
        let awaiting_completions_prompt = self
            .in_flight_native_completions_state
            .as_ref()
            .is_some_and(|state| state.is_awaiting_prompt());
        log::warn!(
            "PTY write gate has been shut for {stalled_for:?} with {} write(s) queued \
             (line_editor_inactive={line_editor_inactive}, awaiting_completions_prompt={awaiting_completions_prompt}); \
             if the pane looks locked up, this is the capture to keep.",
            self.pending_writes.len()
        );
        self.has_logged_current_pty_write_stall = true;
    }

    /// Writes a set of bytes to the PTY to begin bootstrapping a shell.
    pub(super) fn initialize_shell(
        &mut self,
        pending_session_info: &SessionInfo,
        ctx: &mut ModelContext<Self>,
    ) {
        let shell_type = pending_session_info.shell.shell_type();

        #[cfg(not(target_family = "wasm"))]
        {
            self.runs_long_commands_from_file = shell_type == ShellType::PowerShell
                && pending_session_info.session_type == BootstrapSessionType::Local;
            if shell_type == ShellType::PowerShell {
                self.pipe_sessions
                    .track(pending_session_info.session_id.as_u64());
            }
        }

        // #2051: the remote command of this ssh session carried the bootstrap and the remote
        // shell runs it by itself, right after this `InitShell`: nothing is written. Every later
        // event (Bootstrapped, the first Precmd, the hand-over of the held keys) is the shell's
        // own and unchanged.
        if !bootstrap::app_writes_bootstrap(pending_session_info) {
            warpui::input_order::log_hop("bootstrap_in_command", || {
                format!("shell={shell_type:?} wrote=0")
            });
            return;
        }

        // #2019 follow-up: a fish or nu the wrapper's shim started waits for its bootstrap as a
        // counted read from the tty (byte count line, then the bytes); it writes them to a file
        // on the remote host and sources it.
        if bootstrap::staged_ssh_bootstrap_applies(pending_session_info) {
            let script = bootstrap::script_for_shell(shell_type, &crate::ASSETS);
            let payload = bootstrap::staged_ssh_bootstrap_payload(&script, shell_type);
            warpui::input_order::log_hop("bootstrap_staged", || {
                format!("shell={shell_type:?} bytes={}", payload.len())
            });
            self.write_bytes(payload, ctx);
            return;
        }

        #[cfg(feature = "local_fs")]
        if let Some(path) = permanent_bootstrap_file(shell_type, pending_session_info) {
            // If there is a permanent bootstrap file, source it directly. We
            // currently only do this for local PowerShell sessions on Windows.
            self.source_bootstrap_script(path, shell_type, ctx);
            return;
        }

        let bootstrap = bootstrap::script_for_shell(shell_type, &crate::ASSETS);
        self.write_bootstrap_script_to_shell(pending_session_info, ctx, shell_type, bootstrap);
    }

    /// Writes the bytes to to terminate and run the bootstrap script.
    ///
    /// Submits with CR on every platform, as the Enter key does (see
    /// `escape_sequences.rs`'s `"enter" => Some(vec![C0::CR])` key mapping). This used to write
    /// LF on unix, which only runs the line when the tty is cooked (or the line editor accepts
    /// it): PSReadLine 2.3.x reads LF as Ctrl+J and leaves the line unsubmitted, so `pwsh`'s
    /// rc-file bootstrap (the `. '<tmp>.ps1'` line this terminates) depended on being consumed
    /// before PSReadLine went raw. CR works in both modes. Same fix as #792. See #794.
    #[cfg(feature = "local_fs")]
    fn write_terminating_bootstrap_bytes(&mut self, ctx: &mut ModelContext<PtyController<T>>) {
        self.write_bytes(&[escape_sequences::C0::CR][..], ctx);
    }

    /// Writes `chunks` to the pty in order, the first now and each next one `gap` after the
    /// previous, as ONE cancellable sequence (a newer call, [`Self::cancel_paced_writes`]
    /// replaces / drops what is left).
    ///
    /// The gap is flow control for a user-space pty relay (`docker exec -it`, `sudo -i`, `script`)
    /// that deadlocks when flooded (#960): how fast the relay consumes the input is not
    /// observable from here (the echo it produces is not byte-exact and not guaranteed), so this
    /// is the one place the bootstrap stays paced by time. It is never what decides *whether* a
    /// chunk is written: a ^C, the pty shutting down, the shell exiting or a newer bootstrap
    /// bumps `paced_write_generation`, and nothing more is written (previously every chunk was a
    /// free-standing timer that typed itself into the live shell regardless).
    fn start_paced_writes(
        &mut self,
        chunks: Vec<Vec<u8>>,
        pacing: Pacing,
        ctx: &mut ModelContext<PtyController<T>>,
    ) {
        // #1801 (design point b): a setup write is a write like any other. A native-completions
        // exchange cannot be waiting for its prompt here in practice (setup follows a command,
        // which already cancelled it, and an exchange does not start while setup is in flight),
        // but the rule is kept here too rather than relied on.
        self.cancel_native_exchange_before_write(ctx);
        if self.write_queue && self.tmux_control_mode.is_none() {
            // One job in the event loop's own FIFO (#1384): the loop honours the gap as its poll
            // deadline, nothing a user types can land inside the script, and an abort replaces
            // the generation below.
            self.cancel_paced_writes(ctx);
            self.setup_job_sent = true;
            let live = SetupLive::new(self.setup_jobs_live.clone());
            self.send_message_to_event_loop(
                Message::SetupChunked {
                    chunks,
                    pacing,
                    live,
                },
                ctx,
            );
            return;
        }
        // The timer path cannot read the tty mode where the writer does, so it keeps the gap.
        let gap = match pacing {
            Pacing::Gap { dur, .. } | Pacing::GapIfCanonical { dur, .. } => dur,
            Pacing::None => Duration::ZERO,
        };
        self.cancel_paced_writes(ctx);
        let generation = self.paced_write_generation;
        self.paced_write_queue = chunks.into();
        self.write_next_paced_chunk(generation, gap, ctx);
    }

    /// One step of [`Self::start_paced_writes`]: write the next chunk and chain the one after it.
    fn write_next_paced_chunk(
        &mut self,
        generation: u64,
        gap: Duration,
        ctx: &mut ModelContext<PtyController<T>>,
    ) {
        if generation != self.paced_write_generation {
            return;
        }
        let Some(chunk) = self.paced_write_queue.pop_front() else {
            return;
        };
        self.write_bytes(chunk, ctx);
        // `write_bytes` of a lone ^C cancels, but a chunk is never one; re-check anyway.
        if generation != self.paced_write_generation || self.paced_write_queue.is_empty() {
            return;
        }
        // pacing: timer chain writing the next chunk of an app-built paced payload (bootstrap / warpify), never user keys
        ctx.spawn(Timer::after(gap), move |me, _, ctx| {
            me.write_next_paced_chunk(generation, gap, ctx)
        });
    }

    /// Writes the ssh-warpify one-line payload (#1384 P3): ctrl-k ctrl-u to clear the line, then
    /// `payload`, as ONE job in 1000-byte chunks. Nothing a user types lands inside it, a ^C / ^D
    /// or [`Self::cancel_paced_writes`] cuts it at a chunk boundary.
    ///
    /// Pacing: with the writer-owned queue the gap is used only while the tty is canonical (cooked
    /// tty drops input past its queue size silently, so there is no signal to wait on); a
    /// non-canonical tty (a line editor at its prompt) pushes back with `EAGAIN` / `POLLOUT` and
    /// loses nothing, so the chunks go back to back. The kill switch
    /// (`PHOSPHOR_PTY_WRITE_QUEUE=0`) and the remote / ConPTY pty keep the timer chain with the
    /// gap always on, as before.
    pub fn write_line_clear_setup(&mut self, payload: Vec<u8>, ctx: &mut ModelContext<Self>) {
        // Ctrl-u + ctrl-k clears everything before the cursor, then everything after the cursor.
        // Ctrl-c is dangerous because it could cancel an ongoing command. We add an arbitrary
        // space first so that the ctrl-u always clears at least one character, avoiding the
        // audible bell.
        let mut to_write = vec![
            b' ',
            escape_sequences::C0::VT,  // ctrl-k to clear forward
            escape_sequences::C0::NAK, // ctrl-u to clear backward
        ];
        to_write.extend_from_slice(&payload);
        let chunks: Vec<Vec<u8>> = to_write
            .chunks(MAC_TTY_INPUT_QUEUE_CHUNK)
            .map(<[u8]>::to_vec)
            .collect();
        self.start_paced_writes(
            chunks,
            Pacing::GapIfCanonical {
                dur: MAC_TTY_INPUT_QUEUE_PACING,
                why: "canonical tty silently drops input past its queue (macOS 1024 B) with no \
                      EAGAIN or POLLOUT (#1384 P0)",
            },
            ctx,
        );
    }

    /// Drops what is left of a paced bootstrap write (see [`Self::start_paced_writes`]).
    pub fn cancel_paced_writes(&mut self, ctx: &mut ModelContext<Self>) {
        self.paced_write_generation = self.paced_write_generation.wrapping_add(1);
        self.paced_write_queue.clear();
        if std::mem::take(&mut self.setup_job_sent) {
            self.send_message_to_event_loop(Message::Abort(AbortScope::Setup), ctx);
        }
    }

    /// The agent's command was cancelled: drops the agent input the writer has not yet written
    /// (#1384 P4), and nothing else -- typed keys, pastes, commands and replies stay queued.
    /// A no-op without the write queue (the agent input travels as ordinary input there).
    pub fn cancel_agent_writes(&mut self, ctx: &mut ModelContext<Self>) {
        if self.write_queue {
            self.send_message_to_event_loop(Message::Abort(AbortScope::Agent), ctx);
        }
    }

    /// Test-only: use the writer-owned job queue for paced writes, as the local unix pty does.
    #[cfg(test)]
    pub fn force_write_queue(&mut self, enabled: bool) {
        self.write_queue = enabled;
    }

    /// Test-only: how many chunks of a paced write are still unwritten.
    #[cfg(test)]
    pub fn paced_writes_remaining(&self) -> usize {
        self.paced_write_queue.len()
    }

    /// #2025: whether the session `session_info` was spawned from is an MSYS2 shell (Git Bash) or,
    /// since the owner decision of 2026-11-10, a Windows-native pane shell (PowerShell, cmd:
    /// `bootstrap::pane_shell_runs_windows_bash`), or a subshell started by plain `bash` whose own
    /// spawning chain leads to one without passing through a WSL session. A subshell's
    /// session has no launch data, so the pane's shell is found through the chain. Bounded like
    /// `Sessions::MAX_SPAWNING_CHAIN` (nothing prevents a cycle). `false` when the switch
    /// `PHOSPHOR_MSYS2_SUBSHELL_SOURCE` is off.
    #[cfg(feature = "local_fs")]
    fn spawned_from_msys2_through_plain_bash(
        &self,
        session_info: &SessionInfo,
        ctx: &ModelContext<PtyController<T>>,
    ) -> bool {
        static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        let enabled = *ENABLED.get_or_init(|| {
            bootstrap::msys2_subshell_source_enabled_from_env(
                std::env::var("PHOSPHOR_MSYS2_SUBSHELL_SOURCE")
                    .ok()
                    .as_deref(),
            )
        });
        if !enabled {
            return false;
        }
        let sessions = self.sessions.as_ref(ctx);
        let mut next = session_info.spawning_session_id;
        for _ in 0..8 {
            let Some(session) = next.and_then(|id| sessions.get(id)) else {
                return false;
            };
            if bootstrap::pane_shell_runs_windows_bash(
                session.launch_data(),
                session.is_wsl(),
                cfg!(windows),
            ) {
                return true;
            }
            // A plain bash on the way that says it is WSL: what it starts lives in WSL.
            let through_plain_bash = !session.is_wsl()
                && session
                    .subshell_info()
                    .as_ref()
                    .is_some_and(bootstrap::subshell_started_by_plain_bash);
            if !through_plain_bash {
                return false;
            }
            next = session.spawning_session_id();
        }
        false
    }

    #[cfg(feature = "local_fs")]
    fn write_bootstrap_script_to_shell(
        &mut self,
        pending_session_info: &SessionInfo,
        ctx: &mut ModelContext<PtyController<T>>,
        shell_type: ShellType,
        bootstrap: Cow<'static, [u8]>,
    ) {
        use super::bootstrap_file::create_bootstrap_file;
        use crate::terminal::ShellLaunchData;

        // #2025: a bash started by plain `bash` inside a Git Bash pane is sourced its bootstrap
        // like the pane's own shell; typed through its line editor it takes over 30 s on ConPTY.
        let msys2_bash_subshell = bootstrap::msys2_bash_subshell_sources_file(
            shell_type,
            pending_session_info,
            self.spawned_from_msys2_through_plain_bash(pending_session_info, ctx),
        );
        if msys2_bash_subshell {
            warpui::input_order::log_hop("subshell_bootstrap_sourced", || {
                "why=msys2_bash_subshell".to_owned()
            });
        }
        if msys2_bash_subshell
            || bootstrap::should_use_rc_file_bootstrap_method(shell_type, pending_session_info)
        {
            let wsl_distribution = match (
                &pending_session_info.launch_data,
                pending_session_info.wsl_name.as_ref(),
            ) {
                (_, Some(wsl_name)) => Some(wsl_name),
                (Some(ShellLaunchData::WSL { distro }), _) => Some(distro),
                (
                    Some(ShellLaunchData::Executable { .. })
                    | Some(ShellLaunchData::MSYS2 { .. })
                    | Some(ShellLaunchData::DockerSandbox { .. })
                    | None,
                    _,
                ) => None,
            };
            // If creating the temporary file fails for any reason, we fall
            // back to the existing bracketed paste logic. Using bracketed paste
            // reduces the amount of reformatting that Fish tries to do and so improves
            // bootstrap speed. We need to add an explicit leading space, since Fish
            // automatically trims the input when performing a bracketed paste.
            match create_bootstrap_file(&bootstrap, shell_type, wsl_distribution) {
                Some(file) => {
                    if let Some(path) = file.path_as_bytes() {
                        self.source_bootstrap_script(path, shell_type, ctx);
                    } else {
                        self.write_terminating_bootstrap_bytes(ctx);
                        log::error!("Could not convert bootstrap script file path to str");
                    }

                    self.bootstrap_file = Some(file);
                }
                _ => {
                    self.write_bytes(&b" "[..], ctx);
                    self.write_bytes(escape_sequences::BRACKETED_PASTE_START, ctx);
                    self.write_bytes(bootstrap, ctx);
                    self.write_bytes(escape_sequences::BRACKETED_PASTE_END, ctx);
                    self.write_terminating_bootstrap_bytes(ctx);
                }
            }
        } else if bootstrap::is_container_subshell(pending_session_info) {
            // Write in 4KB chunks with 50ms gaps between them, wrapped in bracketed paste and
            // submitted with CR -- the same wrapping the non-container fallback above uses
            // (~764-768). Without it, the raw multi-line script's embedded newlines are just
            // keystrokes to a raw-mode line editor (PSReadLine): the script accumulates into one
            // unsubmitted buffer instead of running. See #795.
            //
            // The chunking itself, and the 50ms delay between chunks, stay as they are: a
            // container subshell (`docker`/`podman exec -it`) sits behind a double-PTY proxy that
            // drops data on large writes or bursts, so the single unchunked write below silently
            // truncates the bootstrap and warpify never completes. Ported from the pin
            // (`4111d08f9:app/src/terminal/writeable_pty/pty_controller.rs:444-454`), which is
            // `is_container_subshell`'s second consumer — the first is
            // `bootstrap::should_use_rc_file_bootstrap_method`.
            //
            // The open marker goes on the front of the first chunk and the close marker plus the
            // submit CR go on the back of the last chunk, rather than out as their own writes, so
            // they ride the same paced cadence as the rest of the script and are never split
            // across a chunk boundary. They're only a handful of bytes, so this doesn't change the
            // chunking's purpose of keeping each write small.
            const CONTAINER_BOOTSTRAP_CHUNK_SIZE: usize = 4096;
            let bytes: Vec<u8> = bootstrap.into_owned();
            let mut chunks: Vec<Vec<u8>> = bytes
                .chunks(CONTAINER_BOOTSTRAP_CHUNK_SIZE)
                .map(<[u8]>::to_vec)
                .collect();
            if chunks.is_empty() {
                chunks.push(Vec::new());
            }
            if let Some(first) = chunks.first_mut() {
                first.splice(
                    0..0,
                    escape_sequences::BRACKETED_PASTE_START.iter().copied(),
                );
            }
            if let Some(last) = chunks.last_mut() {
                last.extend_from_slice(escape_sequences::BRACKETED_PASTE_END);
                last.push(escape_sequences::C0::CR);
            }
            self.start_paced_writes(
                chunks,
                Pacing::Gap {
                    dur: CONTAINER_BOOTSTRAP_PACING,
                    why: "docker/podman exec -it double-pty proxy drops bursts; its consumption is unobservable (#960)",
                },
                ctx,
            );
        } else if pending_session_info.subshell_info.is_some() {
            // #960: a subshell started through a user-space pty relay (`sudo -i`, `sudo su -`,
            // `script`, ...) deadlocks when ~120KB of script is flooded at it in one burst. The
            // relay blocks writing the input into the inner pty (its 4KB input queue is full)
            // while the inner shell, which echoes every byte it reads, blocks writing that echo
            // because the relay is not reading the output -- observed as the root bash stuck in
            // `n_tty_write`/`wait_woken` and `sudo` stuck in a 4097-byte `write`. Reproduced with a
            // blocking relay (`scratchpad/relay_repro.py`): an unpaced flood hangs forever, the same
            // bytes paced in 2KB writes finish in about a second. Pacing keeps the relay's pending
            // write below what the inner tty can queue. No bracketed paste here: the shell is a
            // line-editing bash/zsh that already receives this script raw.
            //
            // #1226: for a `sudo`/`su`/`ksu`/`doas` subshell of a local bash or zsh the body of the
            // script goes in as a counted raw read instead (see
            // `bootstrap::counted_subshell_bootstrap_chunks`): about 26 paced chunks, 650 ms of
            // pure pacing, become the few chunks before the body plus one write (746 -> 222 ms
            // from `InitShell` to `Bootstrapped` through a blocking pty relay).
            let chunks = match bootstrap::local_counted_bootstrap(
                &bootstrap,
                shell_type,
                pending_session_info,
            ) {
                Some((counted, prefix_len)) => {
                    bootstrap::counted_subshell_bootstrap_chunks(&counted, prefix_len)
                }
                None => bootstrap::subshell_bootstrap_chunks(&bootstrap),
            };
            self.start_paced_writes(
                chunks,
                Pacing::Gap {
                    dur: RELAY_BOOTSTRAP_PACING,
                    why: "user-space pty relay deadlocks on a flood; its consumption is unobservable (#960)",
                },
                ctx,
            );
        } else {
            // #1172: a bash over ssh that can take a counted raw read gets the body of the
            // bootstrap that way, not typed through its line editor.
            let bootstrap =
                bootstrap::bootstrap_for_session(bootstrap, shell_type, pending_session_info);
            // #1226: the same for a local bash or zsh tab (the bootstrap is read by one `head -c`
            // instead of typed through the line editor: 174 -> 128 ms for bash, 92 -> 59 ms for
            // zsh, InitShell to Bootstrapped). `PHOSPHOR_LOCAL_COUNTED_BOOTSTRAP=0` types it as
            // before.
            let bootstrap = match bootstrap::local_counted_bootstrap(
                &bootstrap,
                shell_type,
                pending_session_info,
            ) {
                Some((counted, _)) => Cow::Owned(counted),
                None => bootstrap,
            };
            self.write_bytes(bootstrap, ctx);
        }
    }

    #[cfg(feature = "local_fs")]
    /// Sources the bootstrap script at the given path. Assumes that the path
    /// contains a valid file.
    fn source_bootstrap_script(
        &mut self,
        path_to_script: Vec<u8>,
        shell_type: ShellType,
        ctx: &mut ModelContext<Self>,
    ) {
        use warp_util::path::ShellFamily;

        // TODO(CORE-2099): Figure out a more robust solution here. Fish users
        // can redefine these functions via fish functions. Ideally this won't
        // break if the user redefines the `source` or `.` built-in.
        match shell_type {
            ShellType::PowerShell => {
                let path_str = String::from_utf8_lossy(&path_to_script);
                let escaped = ShellFamily::PowerShell.escape(&path_str).into_owned();
                self.write_bytes(b" . ", ctx);
                self.write_bytes(escaped.into_bytes(), ctx);
            }
            _ => {
                self.write_bytes(b" source '", ctx);
                self.write_bytes(path_to_script, ctx);
                self.write_bytes(b"'", ctx);
            }
        }
        self.write_terminating_bootstrap_bytes(ctx);
    }

    #[cfg(not(feature = "local_fs"))]
    fn write_bootstrap_script_to_shell(
        &mut self,
        _pending_session_info: &SessionInfo,
        ctx: &mut ModelContext<PtyController<T>>,
        _shell_type: ShellType,
        bootstrap: Cow<'static, [u8]>,
    ) {
        self.write_bytes(bootstrap, ctx);
    }

    /// Handles the shell having finished bootstrapping.
    ///
    /// #2218: `is_user_command_executing` was set by the command that launched this shell. For a
    /// subshell it is cleared here; for an ssh / remote shell it used to stay set until the FIRST
    /// remote command finished (the remote's InitShell ends the launching `ssh host` block
    /// through `reinit_shell`, with no CommandFinished hook, and the remote's first Precmd
    /// finishes a bootstrap block, not a user one). Until then every
    /// in-band request was refused: the first Tab after a connect never asked the remote shell
    /// (`native_exchange_may_start`: "a command is executing") and went to the completer, whose
    /// side commands each pay the remote user's rc files. The remote shell that has bootstrapped
    /// is the one reading the pty, exactly as a subshell is, so the flag is cleared for it too.
    ///
    /// It is left set when a shell command is queued to be written (typed or held while the
    /// connection came up): that command set the flag for itself after the `ssh`, and it is
    /// still owed its run.
    fn shell_bootstrapped(&mut self, is_subshell: bool, is_ssh_or_remote: bool) {
        if is_subshell {
            self.is_user_command_executing = false;
        } else if is_ssh_or_remote && !self.has_queued_shell_command() {
            if self.is_user_command_executing {
                log::info!(
                    "The remote shell bootstrapped: the command that launched it no longer \
                     blocks in-band requests (#2218)"
                );
            }
            self.is_user_command_executing = false;
        }

        // Now that we have bootstrapped, we can be sure that the bootstrap
        // file is no longer needed.
        #[cfg(not(target_family = "wasm"))]
        self.bootstrap_file.take();
    }

    /// Whether a shell command (not an in-band one) is queued to be written at the next prompt.
    fn has_queued_shell_command(&self) -> bool {
        self.pending_writes.iter().any(|write| {
            matches!(
                write,
                PtyWrite::Command {
                    in_band_command_id: None,
                    ..
                }
            )
        })
    }

    /// Converts the given `command` into a byte array and writes its corresponding bytes to the PTY.
    ///
    /// If the line editor is active, the command is written immediately. Otherwise, the command is
    /// written when the line editor becomes active.
    ///
    /// This also clears pending_writes, since the priority is to execute the user's command.
    ///
    /// The exact sequence of corresponding bytes depends on the given `shell`. For example, if the
    /// shell supports bracketed paste, the command's bytes may be wrapped in bracketed paste byte
    /// sequences.
    pub fn write_command(
        &mut self,
        command: &str,
        shell_type: ShellType,
        source: CommandExecutionSource,
        ctx: &mut ModelContext<Self>,
    ) -> StartCommandOutcome {
        self.write_command_inner(command, shell_type, source, None, ctx)
    }

    /// [`Self::write_command`]. `annotate_block` is applied to the block the command starts, when
    /// that block starts: now for an accepted start, at the write for a command that is held or
    /// parked, and never for a refused one (it used to be applied before the start, to whichever
    /// block was active, a refused command's included).
    ///
    /// #2182 (b): a user's command refused only because the app's own in-band command occupies
    /// the active block is not dropped: it is parked at the front of `pending_writes` and
    /// written at that command's prompt ([`Self::park_user_command_behind_in_band`]).
    fn write_command_inner(
        &mut self,
        command: &str,
        shell_type: ShellType,
        source: CommandExecutionSource,
        annotate_block: Option<AnnotateBlock>,
        ctx: &mut ModelContext<Self>,
    ) -> StartCommandOutcome {
        if self.handoff_held_command_pending && !matches!(source, CommandExecutionSource::User) {
            // One held command at a time; a second would otherwise replace the first, whose
            // requester would wait for it forever.
            log::warn!("A command is already held behind the user's typeahead; coalescing.");
            return StartCommandOutcome::Coalesced;
        }
        let is_users_command = is_users_command_source(&source);
        if !is_users_command && self.has_parked_user_command() {
            // #2182 (b) review M3: the user's command is parked and owed the next prompt. A
            // command that is not the user's (an agent's, a shared session's) accepted in the
            // moment between the side call's end and that write would run ahead of it, so it is
            // coalesced, as behind a held command.
            warpui::input_order::log_hop("write_command_refused", || {
                format!(
                    "source={} outcome=Coalesced reason=user_command_parked",
                    command_source_name(&source)
                )
            });
            return StartCommandOutcome::Coalesced;
        }
        if is_users_command && self.has_parked_user_command() && self.tmux_control_mode.is_none() {
            // A command of the user's is already parked: this one runs after it, in the order
            // they were submitted, whatever the active block says by now. Decided before the
            // hold behind the user's line below: held there, it would set the request waiting
            // and keep the parked command queued behind a report that is not coming.
            warpui::input_order::log_hop("write_command", String::new);
            self.park_user_command_behind_in_band(
                command,
                shell_type,
                source,
                annotate_block,
                "behind_parked_command",
                ctx,
            );
            return StartCommandOutcome::Accepted;
        }
        if self.must_hold_command_behind_unreported_user_line(&source) {
            self.hold_command_behind_unreported_user_line(
                command,
                shell_type,
                source,
                annotate_block,
                ctx,
            );
            return StartCommandOutcome::Accepted;
        }
        warpui::input_order::log_hop("write_command", String::new);
        let is_editor_command = matches!(source, CommandExecutionSource::User);
        let engages_typeahead_handoff;
        {
            let mut model = self.terminal_model.lock();

            // Explicitly start the block now that the command is executed.
            let outcome = start_command_execution_for_source(&mut model, source.clone());
            if !outcome.is_accepted() {
                let in_band_block = model.block_list().active_block().is_in_band_command_block();
                drop(model);
                // #2182 (b): a refusal is never silent.
                warpui::input_order::log_hop("write_command_refused", || {
                    format!(
                        "source={} outcome={outcome:?} in_band_block={in_band_block}",
                        command_source_name(&source)
                    )
                });
                // Review m2: not under tmux control mode, where a write can be taken into
                // tmux's pending buffer before a parked command's block could be started; the
                // refusal is unchanged there (the line is given back to the editor).
                if in_band_block
                    && is_users_command
                    && outcome != StartCommandOutcome::IgnoredTerminated
                    && self.tmux_control_mode.is_none()
                {
                    // The app's own side call holds the block, not a command of the user's: the
                    // user's command waits for it to finish and is written first.
                    self.park_user_command_behind_in_band(
                        command,
                        shell_type,
                        source,
                        annotate_block,
                        "in_band_block",
                        ctx,
                    );
                    return StartCommandOutcome::Accepted;
                }
                if is_editor_command {
                    // #1353 phase 6 (INP-10): the editor's command was refused, so it is never
                    // written, no block starts and the hand-off will not engage for this Enter:
                    // the keys waiting behind it must not wait for either (the negative event).
                    // A command that IS written but is not the hand-off's (flag off, a shell that
                    // cannot report) is followed by its block starting, the positive event.
                    ctx.emit(PtyControllerEvent::TypeaheadHandoffNotEngaged);
                }
                return outcome;
            }
            if let Some(annotate_block) = annotate_block {
                annotate_block(&mut model);
            }
            engages_typeahead_handoff =
                note_command_block_started(&mut model, command, is_editor_command);
        }

        if self.handoff_held_command_pending {
            // A command from the editor goes first and the held command stays queued behind it,
            // to start at the prompt that follows; clearing the queue would drop it silently.
            self.pending_writes
                .retain(|write| is_held_handoff_command(write) || is_parked_user_command(write));
            self.handoff_command_written_since_hold = true;
        } else {
            // #2182 (b): a user command parked behind a side call is never cleared by another
            // command (an agent's, accepted in the moment between the side call's end and the
            // parked command's write); it is written at the next prompt.
            self.pending_writes.retain(is_parked_user_command);
        }
        // #2182 (r29 A2): an in-band command that waited for the user's keys went with the
        // queue: its wait is over, and the waiting it set (only that) with it.
        self.end_in_band_wait();
        self.is_user_command_executing = true;

        // Send the write to the PTY event loop.
        let write = PtyWrite::Command {
            command: command.to_owned(),
            shell_type,
            in_band_command_id: None,
            before_write_fn: None,
            park: CommandPark::None,
        };
        if self.can_write_to_pty(ctx) {
            // Cancel the async writer task and clear the async write queue.
            // Check if line editor is active
            self.send_write_to_event_loop(write, ctx);
            if self.handoff_command_pending {
                self.flush_handoff_bytes_after_command(ctx);
            }
        } else if self.handoff_held_command_pending {
            self.pending_writes.push_front(write);
            if engages_typeahead_handoff {
                self.handoff_command_pending = true;
            }
        } else {
            self.pending_writes.push_back(write);
            if engages_typeahead_handoff {
                self.handoff_command_pending = true;
            }
        }
        if engages_typeahead_handoff {
            self.set_handoff_request_waiting(false);
            ctx.emit(PtyControllerEvent::TypeaheadHandoffChanged);
        }
        self.announce_young_hold_end(ctx);
        StartCommandOutcome::Accepted
    }

    /// Tells the view that the hand-off ended a young hold whose keys are in its input queue
    /// (#1353 phase 5b), when it did. A no-op without the hand-off or without such a hold.
    fn announce_young_hold_end(&self, ctx: &mut ModelContext<Self>) {
        if !FeatureFlag::TypeaheadHandoff.is_enabled() {
            return;
        }
        let pending = self
            .terminal_model
            .lock()
            .typeahead_handoff()
            .young_hold_end_pending();
        if pending {
            ctx.emit(PtyControllerEvent::YoungHoldEnded);
        }
    }

    /// Drops every queued (not yet written) command, so a command parked until the line editor
    /// becomes active never reaches the shell. Used when the pane that queued it is closed
    /// (#848). Keystrokes and in-band commands stay queued.
    pub fn discard_pending_command_writes(&mut self) {
        // #2182 (b) review M2: not the user's own command parked behind a side call. This
        // discard is the agent's (its command was cancelled, `interrupt_agent_command`); the
        // user's command is dropped only by the shell's exit or a real Ctrl-C, with a log line.
        self.pending_writes.retain(|pty_write| {
            is_parked_user_command(pty_write)
                || !matches!(
                    pty_write,
                    PtyWrite::Command {
                        in_band_command_id: None,
                        ..
                    }
                )
        });
        // A command held behind the user's typeahead is parked like any other and goes with
        // them; forget it, or the next one would be coalesced behind a command that is gone.
        if self.handoff_held_command_pending {
            self.forget_held_handoff_command();
        }
    }

    /// Drops only the command held behind the user's typeahead (its agent action was cancelled
    /// or its conversation stopped), leaving every other queued write -- a command from the
    /// editor queued after it included.
    pub fn discard_held_handoff_command(&mut self) {
        if !self.handoff_held_command_pending {
            return;
        }
        self.pending_writes
            .retain(|write| !is_held_handoff_command(write));
        self.forget_held_handoff_command();
    }

    /// [`Self::write_command`], also applying `annotate_block` (e.g. recording the cloud
    /// workflow the command came from) to the block the command starts. For a command held
    /// behind the user's typeahead that block is started only when the command is written, so
    /// the annotation is applied then rather than to whichever block is active now.
    pub fn write_command_annotating_block(
        &mut self,
        command: &str,
        shell_type: ShellType,
        source: CommandExecutionSource,
        annotate_block: impl FnOnce(&mut TerminalModel) + Send + 'static,
        ctx: &mut ModelContext<Self>,
    ) -> StartCommandOutcome {
        self.write_command_inner(
            command,
            shell_type,
            source,
            Some(Box::new(annotate_block)),
            ctx,
        )
    }

    /// Test seam (#2182 b): queues the app's own in-band command, as an executor request does.
    /// For tests outside this module (the view's end-to-end case); nobody listens for a cancel.
    #[cfg(test)]
    pub(crate) fn queue_in_band_command_for_test(
        &mut self,
        command: &str,
        shell_type: ShellType,
        command_id: &str,
        ctx: &mut ModelContext<Self>,
    ) {
        let (cancel_tx, _cancel_rx) = async_channel::unbounded();
        self.queue_in_band_command(command, shell_type, command_id.to_owned(), cancel_tx, ctx);
    }

    /// #2182 (b): whether a command of the user's is parked behind the app's own in-band command.
    fn has_parked_user_command(&self) -> bool {
        self.pending_writes.iter().any(is_parked_user_command)
    }

    /// #2182 (b): whether the next queued write is a parked user command the typeahead hand-off
    /// will engage for.
    fn front_parked_command_expects_handoff(&self) -> bool {
        matches!(
            self.pending_writes.front(),
            Some(PtyWrite::Command {
                park: CommandPark::BehindInBand(parked),
                ..
            }) if parked.expects_handoff
        )
    }

    /// #2182 (b), owner decision 2026-11-14 ("wait"): the user's command was refused only
    /// because the app's own in-band command occupies the active block. It is neither dropped
    /// nor handed back to the editor, and the side call is not interrupted: the command is
    /// parked at the FRONT of `pending_writes` (behind a user command parked earlier, ahead of
    /// every side call still queued; nothing queued is cancelled) and written by the event that
    /// already drains the queue, the line editor becoming active at the side call's prompt. Its
    /// block starts at that write ([`Self::parked_user_command_write`]), so the side call's
    /// output never lands in it. No timer.
    ///
    /// `is_user_command_executing` is set, so no new side call is accepted meanwhile
    /// (`queue_in_band_command` cancels it to its requester). No
    /// `TypeaheadHandoffNotEngaged` is emitted: that event says nothing will take the command.
    fn park_user_command_behind_in_band(
        &mut self,
        command: &str,
        shell_type: ShellType,
        source: CommandExecutionSource,
        annotate_block: Option<AnnotateBlock>,
        reason: &'static str,
        ctx: &mut ModelContext<Self>,
    ) {
        let is_editor_command = matches!(source, CommandExecutionSource::User);
        let (can_report, side_call_prompt_pending) = {
            let model = self.terminal_model.lock();
            (
                model.block_list().early_output().typeahead_mode() == TypeaheadMode::ShellReported,
                model.block_list().active_block().is_in_band_command_block(),
            )
        };
        let expects_handoff =
            is_editor_command && FeatureFlag::TypeaheadHandoff.is_enabled() && can_report;
        if expects_handoff {
            // Review B1: the hand-off is told NOW, as `write_command` tells it for a command
            // queued behind an inactive line editor. Until the write the editor still holds the
            // submitted text; keys routed by the old facts landed in it behind that text
            // (`exit` then `ssh h` + Enter submitted `exitssh h`) and keys typed after the
            // write went to the pty (a reorder). From here the keys are the pty's, and
            // `write_handoff_user_bytes` keeps them behind the command (`handoff_command_pending`).
            self.handoff
                .write()
                .on_command_parked(side_call_prompt_pending);
        }
        let source_name = command_source_name(&source);
        let parked = ParkedUserCommand {
            command: command.to_owned(),
            shell_type,
            source,
            annotate_block: Arc::new(parking_lot::Mutex::new(annotate_block)),
            started: Arc::new(parking_lot::Mutex::new(None)),
            expects_handoff,
        };
        // Behind the last user command already parked, else first.
        let at = self
            .pending_writes
            .iter()
            .rposition(is_parked_user_command)
            .map_or(0, |last| last + 1);
        let write = self.parked_user_command_write(parked);
        self.pending_writes.insert(at, write);
        self.is_user_command_executing = true;
        // An in-band command that was waiting at the front for the user's keys is no longer the
        // front: its wait ends with the waiting it set, and is armed again when its turn comes.
        self.end_in_band_wait();
        let queued = self.pending_writes.len();
        warpui::input_order::log_hop("write_command_parked_behind_in_band", || {
            format!(
                "source={source_name} reason={reason} at={at} queued={queued} \
                 handoff={expects_handoff}"
            )
        });
        if expects_handoff {
            self.handoff_command_pending = true;
            self.set_handoff_request_waiting(false);
            ctx.emit(PtyControllerEvent::TypeaheadHandoffChanged);
        }
        // A young hold (or a deferred Enter's gate) the hand-off ended for this command: the
        // view writes its keys, which then wait behind the command like any typed now.
        self.announce_young_hold_end(ctx);
        // Its turn may have come already (parked behind an earlier one at a free prompt).
        self.execute_next_queued_write(ctx);
    }

    /// #2182 (b): a parked user command will not be written. Undoes what the park told the
    /// hand-off (`told_handoff`: its Enter is not awaited any more) and ends the wait of the
    /// keys typed behind it: they are written now (`write_keys`: a Ctrl-C, where the tty decides
    /// what an interrupt leaves of them, as it would have had they been written when typed) or
    /// dropped with the command (the shell is gone).
    fn forget_parked_handoff(
        &mut self,
        told_handoff: usize,
        write_keys: bool,
        ctx: &mut ModelContext<Self>,
    ) {
        if told_handoff > 0 {
            let mut handoff = self.handoff.write();
            for _ in 0..told_handoff {
                handoff.on_parked_command_dropped();
            }
        }
        if self.handoff_command_pending && !self.has_parked_user_command() {
            if write_keys {
                self.flush_handoff_bytes_after_command(ctx);
            } else {
                self.handoff_command_pending = false;
                self.handoff_bytes_after_command.clear();
            }
        }
    }

    /// The queued write of a parked user command. Its `before_write_fn` starts the command's
    /// block and does the hand-off bookkeeping of an accepted start
    /// ([`note_command_block_started`]) right before the bytes go, and records what it found
    /// for [`Self::settle_parked_user_command`].
    fn parked_user_command_write(&self, parked: ParkedUserCommand) -> PtyWrite {
        let terminal_model = self.terminal_model.clone();
        let for_start = parked.clone();
        PtyWrite::Command {
            command: parked.command.clone(),
            shell_type: parked.shell_type,
            in_band_command_id: None,
            before_write_fn: Some(Box::new(move || {
                let mut model = terminal_model.lock();
                let outcome =
                    start_command_execution_for_source(&mut model, for_start.source.clone());
                let mut engaged = false;
                if outcome.is_accepted() {
                    if let Some(annotate_block) = for_start.annotate_block.lock().take() {
                        annotate_block(&mut model);
                    }
                    if for_start.expects_handoff {
                        // The hand-off was told at the park (review B1): what is left is the
                        // block's own bookkeeping and what a write adds to the park.
                        model.publish_key_facts();
                        model.end_in_band_command_output(false);
                        model
                            .typeahead_handoff_mut()
                            .on_parked_command_written(&for_start.command);
                        model
                            .block_list_mut()
                            .early_output_mut()
                            .clear_typeahead_for_new_command();
                        engaged = true;
                    } else {
                        engaged = note_command_block_started(
                            &mut model,
                            &for_start.command,
                            matches!(for_start.source, CommandExecutionSource::User),
                        );
                    }
                }
                *for_start.started.lock() = Some(ParkedStart { outcome, engaged });
                outcome
            })),
            park: CommandPark::BehindInBand(Box::new(parked)),
        }
    }

    /// #2182 (b): the turn of a parked user command came (`did_write` is what its write
    /// returned). Returns `false` when its start was refused and it went back to the front of
    /// the queue to wait for the next prompt; the caller then stops draining.
    fn settle_parked_user_command(
        &mut self,
        parked: ParkedUserCommand,
        did_write: bool,
        ctx: &mut ModelContext<Self>,
    ) -> bool {
        let started = parked.started.lock().take();
        let is_editor_command = matches!(parked.source, CommandExecutionSource::User);
        let source_name = command_source_name(&parked.source);
        match started {
            Some(ParkedStart { outcome, engaged }) if outcome.is_accepted() => {
                warpui::input_order::log_hop("parked_user_command_written", || {
                    format!("source={source_name} engaged={engaged} did_write={did_write}")
                });
                if engaged {
                    // What `write_command` does for an engaged command at the submit.
                    self.set_handoff_request_waiting(false);
                    ctx.emit(PtyControllerEvent::TypeaheadHandoffChanged);
                }
                self.announce_young_hold_end(ctx);
                true
            }
            Some(ParkedStart {
                outcome: StartCommandOutcome::IgnoredTerminated,
                ..
            }) => {
                // The shell is gone: nothing can run the command.
                warpui::input_order::log_hop("parked_user_command_dropped", || {
                    format!("source={source_name} reason=terminated")
                });
                log::warn!(
                    "Dropped a submitted command parked behind an in-band command: the shell exited."
                );
                self.is_user_command_executing = self.has_parked_user_command();
                self.forget_parked_handoff(usize::from(parked.expects_handoff), false, ctx);
                if is_editor_command {
                    ctx.emit(PtyControllerEvent::TypeaheadHandoffNotEngaged);
                }
                true
            }
            Some(ParkedStart { outcome, .. }) => {
                // Still refused (another block was started under it): back to the front, for
                // the next prompt. Never dropped, never looped on.
                warpui::input_order::log_hop("write_command_refused", || {
                    format!("source={source_name} outcome={outcome:?} at_write=true reparked=true")
                });
                let write = self.parked_user_command_write(parked);
                self.pending_writes.push_front(write);
                false
            }
            None => {
                // The write ended before its `before_write_fn` ran (nothing to type, or tmux
                // control mode took or refused the bytes): no block was started for it here.
                warpui::input_order::log_hop("parked_user_command_not_started", || {
                    format!("source={source_name} did_write={did_write}")
                });
                log::warn!(
                    "A submitted command parked behind an in-band command was not started at its write."
                );
                self.is_user_command_executing = self.has_parked_user_command();
                // The keys typed behind it are written: tmux control mode took the command's
                // bytes (or refused them, and said so), and they belong behind those.
                self.forget_parked_handoff(usize::from(parked.expects_handoff), true, ctx);
                if is_editor_command {
                    ctx.emit(PtyControllerEvent::TypeaheadHandoffNotEngaged);
                }
                true
            }
        }
    }

    /// #2182 (b): a user command still parked will not run (`reason`): the shell exited, or a
    /// real Ctrl-C came (owner's #2209 Q3: a real ^C voids typed-ahead input, and a command
    /// submitted but not yet written is that). It goes with a log line, the hand-off forgets the
    /// Enter it counted for it, and the view is told nothing will take the command
    /// (`TypeaheadHandoffNotEngaged`, review m4: a deferred Enter's gate must not wait for its
    /// 10 s net). `write_keys` is [`Self::forget_parked_handoff`]'s. Returns how many went.
    fn drop_parked_user_commands(
        &mut self,
        reason: &'static str,
        write_keys: bool,
        ctx: &mut ModelContext<Self>,
    ) -> usize {
        let mut dropped = 0;
        let mut told_handoff = 0;
        self.pending_writes.retain(|write| match write {
            PtyWrite::Command {
                park: CommandPark::BehindInBand(parked),
                ..
            } => {
                dropped += 1;
                told_handoff += usize::from(parked.expects_handoff);
                false
            }
            _ => true,
        });
        if dropped == 0 {
            return 0;
        }
        self.is_user_command_executing = false;
        warpui::input_order::log_hop("parked_user_command_dropped", || {
            format!("reason={reason} commands={dropped} write_keys={write_keys}")
        });
        log::warn!(
            "Dropped {dropped} submitted command(s) parked behind an in-band command: {reason}."
        );
        self.forget_parked_handoff(told_handoff, write_keys, ctx);
        ctx.emit(PtyControllerEvent::TypeaheadHandoffNotEngaged);
        dropped
    }

    /// Review m5: a real Ctrl-C drops a user command that is still parked; the keys typed
    /// behind it are written ahead of the ^C, where the tty's interrupt handling decides what is
    /// left of them, exactly as if they had been written when they were typed.
    fn drop_parked_user_commands_on_interrupt(&mut self, ctx: &mut ModelContext<Self>) {
        if self.has_parked_user_command() {
            self.drop_parked_user_commands("ctrl_c", true, ctx);
        }
    }

    /// Typeahead hand-off: whether a command the hand-off does not own (a queued-prompt-panel or
    /// agent command) must wait, because the shell's line may hold user bytes its kill prefix
    /// would destroy before they were reported. Decided before the command's block is started.
    fn must_hold_command_behind_unreported_user_line(
        &self,
        source: &CommandExecutionSource,
    ) -> bool {
        if !FeatureFlag::TypeaheadHandoff.is_enabled() {
            return false;
        }
        let model = self.terminal_model.lock();
        let engages_typeahead_handoff = matches!(source, CommandExecutionSource::User)
            && model.block_list().early_output().typeahead_mode() == TypeaheadMode::ShellReported;
        // Only a command whose start would be accepted right now is held. Anything else (a block
        // already started or running) takes the normal path, so it is coalesced or rejected
        // exactly as without the hand-off.
        if engages_typeahead_handoff || !model.typeahead_handoff().holds_queued_writes() {
            return false;
        }
        if model.is_at_prompt_for_typeahead_request() {
            return true;
        }
        // #2198: a young Enter is held (typed right after a command finished, its prompt not
        // seen yet) with the keys behind it. "At the prompt" is exactly what is not known in
        // that moment, and a command accepted then was written over the held keys, which were
        // discarded. It is held like one at the prompt, until the hold has ended; only while no
        // block is started (a running command still rejects the start, as before).
        let young_hold_queued = model.typeahead_handoff().young_hold_queued();
        young_hold_queued && !model.block_list().active_block().started()
    }

    /// Queues a command behind the user's unreported line. Its block is started only when it is
    /// written -- after the request, which goes out ahead of it -- so the shell's prompt still
    /// reads as a prompt (the request needs one), and a command that never goes out leaves no
    /// started block behind. Its Enter is counted at that point, like any command the hand-off
    /// does not own.
    fn hold_command_behind_unreported_user_line(
        &mut self,
        command: &str,
        shell_type: ShellType,
        source: CommandExecutionSource,
        annotate_block: Option<Box<dyn FnOnce(&mut TerminalModel) + Send>>,
        ctx: &mut ModelContext<Self>,
    ) {
        let agent_metadata = match &source {
            CommandExecutionSource::AI { metadata } => Some(metadata),
            CommandExecutionSource::SharedSession { ai_metadata, .. } => ai_metadata.as_ref(),
            CommandExecutionSource::User
            | CommandExecutionSource::QueuedCommand
            | CommandExecutionSource::EnvVarCollection { .. } => None,
        };
        let held_agent_command = agent_metadata.and_then(|metadata| {
            metadata
                .requested_command_action_id()
                .map(|action_id| HeldAgentCommand {
                    action_id: action_id.clone(),
                    conversation_id: *metadata.conversation_id(),
                })
        });
        self.terminal_model
            .lock()
            .typeahead_handoff_mut()
            .set_held_agent_command(held_agent_command);
        self.handoff_command_written_since_hold = false;
        let terminal_model = self.terminal_model.clone();
        // The queued in-band commands go; their requesters are told (r29 A2), as the give-up of
        // the in-band wait tells its requester.
        self.clear_pending_writes_cancelling_in_band();
        self.is_user_command_executing = true;
        self.pending_writes.push_back(PtyWrite::Command {
            command: command.to_owned(),
            shell_type,
            in_band_command_id: None,
            park: CommandPark::BehindUserLine,
            before_write_fn: Some(Box::new(move || {
                let mut model = terminal_model.lock();
                let outcome = start_command_execution_for_source(&mut model, source);
                model.publish_key_facts();
                if outcome.is_accepted() {
                    if let Some(annotate_block) = annotate_block {
                        annotate_block(&mut model);
                    }
                    model.end_in_band_command_output(false);
                    model.typeahead_handoff_mut().on_uncounted_command_written();
                }
                outcome
            })),
        });
        self.handoff_held_command_pending = true;
        self.set_handoff_request_waiting(true);
        self.retry_typeahead_handoff_request(ctx);
    }

    /// Drops a command held behind the user's typeahead that can no longer go out (the shell
    /// exited). Its requester gets no result: commands carry no completion channel here.
    fn drop_held_handoff_command(&mut self) {
        self.discard_held_handoff_command();
        log::warn!("Dropped a command held behind the user's typeahead: the shell exited.");
    }

    /// Clears the bookkeeping of a held command that was dropped without being written.
    /// Records whether the shell has bracketed paste on, here and on the terminal model, so the
    /// input's multi-line confirmation decides on exactly the flag the write uses (#1046).
    fn set_bracketed_paste_enabled(&mut self, enabled: bool) {
        self.is_bracketed_paste_enabled = enabled;
        self.terminal_model.lock().set_pty_bracketed_paste(enabled);
    }

    fn forget_held_handoff_command(&mut self) {
        self.handoff_held_command_pending = false;
        if !self.handoff_command_written_since_hold {
            self.is_user_command_executing = false;
        }
        self.terminal_model
            .lock()
            .typeahead_handoff_mut()
            .set_held_agent_command(None);
    }

    /// #2198: whether the next queued write is an agent / queued-panel command held behind the
    /// user's line (the one kind of command that is not in-band and carries a `before_write_fn`)
    /// while a young Enter is held with the keys typed behind it. Written now it would land
    /// ahead of keys typed before it, its kill prefix would wipe the line the held Enter belongs
    /// to, and telling the hand-off of it used to end the hold by DISCARDING the keys. It waits
    /// instead, wherever the shell is (not only at the prompt), and nothing is dropped or
    /// reordered: the keys leave the hold by its own end and the command follows.
    ///
    /// Why waiting and not "end the hold as typeahead": the command would still be written
    /// first, so the user's Enter would run after the app's command and without the line the
    /// kill prefix removed. Only keeping the command back keeps the typed order.
    ///
    /// No new hold or timer: it is the queued write staying in `pending_writes`, as it does for
    /// every other reason. It is released by the events that end the hold: the view applies the
    /// end, which empties its queue (the pane's queue wakes this controller) and writes the keys
    /// through `write_handoff_user_bytes` (which decides again while `handoff_request_waiting`
    /// is set). The hold's own net (2 s / 10 s, in the view) bounds it.
    ///
    /// An in-band command waits for the same hold in [`Self::in_band_front_waits_for_keys`]
    /// (the hold is part of `TypeaheadHandoff::holds_queued_writes`), where its wait has its
    /// give-up. With that wait's kill switch (`PHOSPHOR_INBAND_WAIT_FOR_KEYS=0`) it is written
    /// over the hold as before #2182, and the keys then follow it in typed order instead of
    /// being discarded (`TypeaheadHandoff::on_uncounted_command_written`).
    fn young_hold_holds_front_command(&mut self) -> bool {
        let front_is_app_command = FeatureFlag::TypeaheadHandoff.is_enabled()
            && matches!(
                self.pending_writes.front(),
                Some(PtyWrite::Command {
                    in_band_command_id: None,
                    park: CommandPark::BehindUserLine,
                    ..
                })
            );
        if !front_is_app_command
            || !self
                .terminal_model
                .lock()
                .typeahead_handoff()
                .young_hold_queued()
        {
            return false;
        }
        warpui::input_order::log_hop("app_command_waits_for_young_hold", String::new);
        if !self.handoff_request_waiting {
            // The keys' own write decides again once the hold has ended.
            self.set_handoff_request_waiting(true);
        }
        true
    }

    /// Typeahead hand-off: whether the next queued write is a command that starts its own block
    /// when written (an in-band or held command) while the shell may still have user lines to
    /// run. It waits, or the output of those lines would land in its block.
    fn handoff_holds_front_command(&self) -> bool {
        // Not a parked user command (#2182 b): its own Enter is counted from the park on, so
        // it would wait for itself. The side call ahead of it keeps the line editor inactive.
        FeatureFlag::TypeaheadHandoff.is_enabled()
            && matches!(
                self.pending_writes.front(),
                Some(PtyWrite::Command {
                    before_write_fn: Some(_),
                    park: CommandPark::None | CommandPark::BehindUserLine,
                    ..
                })
            )
            && self
                .terminal_model
                .lock()
                .typeahead_handoff()
                .has_unanswered_lines()
    }

    /// Synchronously writes the EOT (End-of-Transmission) char to the PTY.
    pub fn write_end_of_transmission_char(&mut self, ctx: &mut ModelContext<Self>) {
        self.write_bytes(&[escape_sequences::C0::EOT][..], ctx);

        // Consider the active block to be "started" since a user performed an action that
        // results in bytes being written to the pty. This makes the output from ctrl-d during ssh
        // get written to the active block.
        // TODO: reconsider this behavior since the output was not the result of a command, and given the function name
        // is start_command_execution and no command was executed.
        {
            let mut model = self.terminal_model.lock();
            model.start_command_execution();
            model.publish_key_facts();
        }
    }

    /// Resizes the PTY's size (i.e. its notion of the number of columns and rows in the screen) via
    /// ioctl system call and updates the terminal model as appropriate.
    pub fn resize_pty(&self, size_update: SizeUpdate, ctx: &mut ModelContext<Self>) {
        // Send a message to the PTY event loop to resize the PTY.
        // We also need to resize when rows/cols changed without a pane size change
        // (e.g. ViewerSizeReported on the sharer side).
        if size_update.pane_size_changed()
            || size_update.is_refresh()
            || size_update.rows_or_columns_changed()
        {
            self.send_message_to_event_loop(Message::Resize(size_update.new_size), ctx);
        }
    }

    /// Writes agent input to the PTY.
    pub fn write_agent_bytes<B: Into<Cow<'static, [u8]>>>(
        &mut self,
        bytes: B,
        mode: &AIAgentPtyWriteMode,
        ctx: &mut ModelContext<Self>,
    ) {
        self.send_write_to_event_loop(
            PtyWrite::AgentInput {
                bytes: bytes.into(),
                mode: *mode,
            },
            ctx,
        );
    }

    /// Writes user input to the PTY.
    ///
    /// This should only be called for non-command input (e.g. input that should be passed through
    /// in a long-running command or in the alt screen, rather than from the input editor).
    pub fn write_bytes<B: Into<Cow<'static, [u8]>>>(
        &mut self,
        bytes: B,
        ctx: &mut ModelContext<Self>,
    ) {
        self.send_write_to_event_loop(
            PtyWrite::Bytes {
                bytes: bytes.into(),
            },
            ctx,
        );
    }

    /// Writes a paste-sized block of user bytes. The event loop streams it to the pty as the pty
    /// (and whatever is on the other end, such as ssh) accepts it, never dropping or splitting a
    /// byte, and a ^C cancels what is still unwritten (#1255).
    pub fn write_paste_bytes<B: Into<Cow<'static, [u8]>>>(
        &mut self,
        bytes: B,
        ctx: &mut ModelContext<Self>,
    ) {
        self.send_write_to_event_loop(
            PtyWrite::PasteBytes {
                bytes: bytes.into(),
            },
            ctx,
        );
    }

    /// ConPTY prompt hold (#1349): the session's shell signals its line editor being ready, so
    /// the hand-off's request decision of each prompt waits for that signal (the write gate does
    /// not, #1825); a prompt cycle that ends without one clears the wait.
    pub fn set_conpty_prompt_hold_signals(
        &mut self,
        wait: Option<std::time::Duration>,
        ctx: &mut ModelContext<Self>,
    ) {
        self.line_editor_status
            .update(ctx, |status, _| status.set_signal_wait(wait));
    }

    pub fn set_foreground_sigint(&mut self, f: std::sync::Arc<dyn Fn() -> bool>) {
        self.foreground_sigint = Some(f);
    }

    /// Interrupts the pty's foreground program with SIGINT rather than a ^C byte, which a raw
    /// tty would hand to the program as data (#1001). Falls back to the byte.
    pub fn sigint_foreground_or_write_etx(&mut self, ctx: &mut ModelContext<Self>) {
        // Whatever of a big paste is still waiting to be written must not outlive the interrupt,
        // nor the rest of a paced bootstrap.
        self.cancel_paced_writes(ctx);
        // #2182 (b) review m5: the interrupt also voids a user command still parked.
        self.drop_parked_user_commands_on_interrupt(ctx);
        self.send_message_to_event_loop(Message::DiscardQueuedInput, ctx);
        if self.foreground_sigint.as_ref().is_some_and(|f| f()) {
            return;
        }
        self.write_bytes(&[escape_sequences::C0::ETX][..], ctx);
    }

    /// Test-only: whether `shutdown_pty` has been called.
    #[cfg(test)]
    pub fn shutdown_requested(&self) -> bool {
        self.shutdown_requested
    }

    /// Shuts down the pty and event loop.
    pub fn shutdown_pty(&mut self, ctx: &mut ModelContext<Self>) {
        #[cfg(test)]
        {
            self.shutdown_requested = true;
        }
        self.cancel_paced_writes(ctx);
        self.send_message_to_event_loop(Message::Shutdown, ctx);
    }

    /// What to send to the shell for `command`: the line to type into the line editor, or, for a
    /// PowerShell command that can be inserted in one operation, the insert chord (#1581). A
    /// command too long for PSReadLine to render (#1106) becomes a short launcher line that reads
    /// it from the pipe (#1592). Nothing is ever written to disk; if the pipe is unavailable (or
    /// the kill switch is off) the command is typed.
    fn command_to_type(
        &mut self,
        command: String,
        shell_type: ShellType,
        is_user_command: bool,
        session_id: Option<u64>,
    ) -> TypedCommand {
        let force_typed = std::mem::take(&mut self.force_typed_command);
        #[cfg(not(target_family = "wasm"))]
        if shell_type == ShellType::PowerShell
            && self.runs_long_commands_from_file
            && is_user_command
            && !force_typed
            && self.file_insert_pending.is_none()
            && self.fast_insert_on()
            && long_command_file::can_insert_via_pipe(&command)
        {
            // The pipe exists before the chord is written, so the shell cannot connect early.
            if let Some(pipe) = self.serve_on_pipe(session_id, &command, false) {
                self.command_pipe = Some(pipe);
                // `qpc=`: the clock of PowerShell's hook timing marks (#1401, pwsh_timing.py).
                warpui::input_order::log_hop("pwsh_command_via_pipe", || {
                    format!(
                        "bytes={}{}",
                        command.len(),
                        warpui::input_order::qpc_suffix()
                    )
                });
                self.file_insert_pending = Some(command);
                {
                    let mut model = self.terminal_model.lock();
                    self.file_insert_block = Some(model.block_list().active_block().id().clone());
                    // #1793: the text reaches the shell on the pipe, not the tty, so its Preexec
                    // settles none of the requests written before it.
                    model.typeahead_handoff_mut().forget_written_command_text();
                }
                return TypedCommand::FileChord;
            }
        }
        // A command too long for PSReadLine to render (#1106) is served on the pipe and a short
        // launcher line is typed (#1592). With the kill switch off, or if the pipe cannot be made,
        // it is typed as is: never written to disk.
        #[cfg(not(target_family = "wasm"))]
        if shell_type == ShellType::PowerShell
            && self.runs_long_commands_from_file
            && is_user_command
            && !force_typed
            && self.fast_insert_on()
            && long_command_file::needs_launcher(&command)
        {
            if let Some(pipe) = self.serve_on_pipe(session_id, &command, true) {
                self.long_command_pipe = Some(pipe);
                let launcher = long_command_file::LAUNCHER_LINE.to_owned();
                // The shell echoes and reports the launcher; the block header, tab title and
                // suggestion context must show the command the user submitted (#1117).
                self.terminal_model
                    .lock()
                    .set_command_display_override(launcher.clone(), command);
                return TypedCommand::Line(launcher);
            }
        }
        let _ = (is_user_command, session_id, force_typed);
        let _ = shell_type;
        // A bash with no bracketed paste is typed the command key by key, which stalls for a
        // huge one (#1129); the shell decodes it from one short line instead. With bracketed paste
        // the same holds from a larger size, where readline's multi-line paste turns quadratic
        // (#1255).
        if shell_type == ShellType::Bash
            && posix_long_command::needs_wrapper_for(&command, self.is_bracketed_paste_enabled)
        {
            // Bracketed paste: one line. Typed keys: folded lines (#1129), the shell's line
            // editor redisplays the whole line per key.
            let line = if self.is_bracketed_paste_enabled {
                posix_long_command::wrapper_line(&command)
            } else {
                posix_long_command::wrapper_lines_for_typed_input(&command)
            };
            self.terminal_model
                .lock()
                .set_command_display_override(line.clone(), command);
            return TypedCommand::Line(line);
        }
        TypedCommand::Line(command)
    }

    /// The kill switch (`PHOSPHOR_PWSH_FAST_INSERT=0`); tests flip it per controller instead of
    /// through the process environment.
    #[cfg(not(target_family = "wasm"))]
    fn fast_insert_on(&self) -> bool {
        #[cfg(test)]
        if self.fast_insert_off_for_test {
            return false;
        }
        long_command_file::fast_insert_enabled()
    }

    /// Closes the previous pipe of the same slot (the name is a first-instance create, so it has
    /// to be free) and serves `command` on the session's pipe. The insert chord and the long
    /// command launcher have a slot each (the launcher's name has a `-long` suffix), so a long
    /// command can follow an outstanding chord without being typed raw. `None`: type it instead.
    #[cfg(not(target_family = "wasm"))]
    fn serve_on_pipe(
        &mut self,
        session_id: Option<u64>,
        command: &str,
        long: bool,
    ) -> Option<command_pipe::CommandPipe> {
        let mut name = session_id.and_then(command_pipe::lookup_pipe_name)?;
        if long {
            name.push_str("-long");
            self.long_command_pipe = None;
        } else {
            self.command_pipe = None;
        }
        command_pipe::CommandPipe::serve(&name, command)
    }

    /// The insert chord found no command file and inserted nothing (#1581): type the command the
    /// old way, once. The shell's buffer is untouched, and the retype starts with the kill-buffer
    /// chord like every typed command. Nothing is retyped when no chord is outstanding (a Preexec
    /// already answered it, or the report is stale).
    fn on_command_file_failed(&mut self, ctx: &mut ModelContext<Self>) {
        let Some(command) = self.file_insert_pending.take() else {
            warpui::input_order::log_hop("pwsh_command_file_failed_ignored", String::new);
            return;
        };
        self.file_insert_block = None;
        warpui::input_order::log_hop("pwsh_command_file_failed_retype", || {
            format!("bytes={}", command.len())
        });
        self.command_pipe = None;
        self.force_typed_command = true;
        self.send_write_to_event_loop(
            PtyWrite::Command {
                command,
                shell_type: ShellType::PowerShell,
                in_band_command_id: None,
                before_write_fn: None,
                park: CommandPark::None,
            },
            ctx,
        );
        self.force_typed_command = false;
    }

    /// A Precmd while a chord is outstanding means no Preexec came (it clears the chord): retype.
    fn on_precmd_with_chord_outstanding(&mut self, ctx: &mut ModelContext<Self>) {
        if self.file_insert_pending.is_none() {
            return;
        }
        let active = self
            .terminal_model
            .lock()
            .block_list()
            .active_block()
            .id()
            .clone();
        if self.file_insert_block.as_ref() == Some(&active) {
            // The Precmd of the prompt the chord was written at (queued before the write, or a
            // generator command's): not evidence the chord was lost. Retyping here could run the
            // command twice.
            warpui::input_order::log_hop("pwsh_precmd_same_prompt_chord_kept", String::new);
            return;
        }
        warpui::input_order::log_hop("pwsh_precmd_without_preexec", String::new);
        self.on_command_file_failed(ctx);
    }

    /// Forgets an outstanding insert chord (its command ran, or its write was refused before it
    /// reached the pty) and closes its pipe.
    fn clear_file_insert(&mut self) {
        self.file_insert_pending = None;
        self.file_insert_block = None;
        self.command_pipe = None;
    }

    /// Sends a message to the event loop thread requesting a PTY write for the given `bytes`.
    ///
    /// If the write corresponds to a command, this also calls
    /// [`LineEditorStatus::did_execute_command()`].
    fn send_write_to_event_loop(&mut self, write: PtyWrite, ctx: &mut ModelContext<Self>) -> bool {
        // #1801 (design point b): nothing is written behind a `^Y` still waiting for its prompt
        // without the cancel frame first.
        if !matches!(write, PtyWrite::RunNativeShellCompletions(_)) {
            self.cancel_native_exchange_before_write(ctx);
        }
        // Captured before the match below consumes `write`. Upstream threads this as a fifth
        // tuple element out of that match; here the tuple already carries a fork-only fifth
        // element (`raw_tmux_command`), and a sixth would push the binding past the line limit
        // and reindent the whole match. Same value, same arms, no reflow.
        let shell_type_for_split = match &write {
            PtyWrite::Command { shell_type, .. } => Some(*shell_type),
            PtyWrite::AgentInput { .. }
            | PtyWrite::Bytes { .. }
            | PtyWrite::PasteBytes { .. }
            | PtyWrite::TmuxCommand(_)
            | PtyWrite::RunNativeShellCompletions(_) => None,
        };

        // The submitted buffer of a user command (not an in-band one) may be cancelled by a ^C
        // while still unwritten (#1046); nothing else is.
        let is_user_command = matches!(
            &write,
            PtyWrite::Command {
                in_band_command_id: None,
                ..
            }
        );

        // A lone ^C is an interrupt: it cancels what is still queued for the pty (#1046).
        let is_interrupt = matches!(
            &write,
            PtyWrite::Bytes { bytes } if bytes.as_ref() == [escape_sequences::C0::ETX]
        );
        if is_interrupt {
            self.cancel_paced_writes(ctx);
            // #2182 (b) review m5: the ^C also voids a user command still parked.
            self.drop_parked_user_commands_on_interrupt(ctx);
        }

        // A paste is streamed by the event loop a tty-buffer at a time (the remote program or ssh
        // sets the pace; nothing is dropped or split). It is tagged cancellable so a ^C stops
        // what is still unwritten instead of queueing behind all of it (#1255).
        let is_paste = matches!(&write, PtyWrite::PasteBytes { .. });
        let is_agent_input = matches!(&write, PtyWrite::AgentInput { .. });

        let (bytes_to_write, is_for_command, on_write_fn, raw_tmux_command) = match write {
            PtyWrite::Command {
                command,
                shell_type,
                in_band_command_id,
                before_write_fn: on_write_fn,
                park: _,
            } => {
                let session_id = self
                    .model_event_dispatcher
                    .as_ref(ctx)
                    .active_session_id()
                    .map(|id| id.as_u64());
                let bytes = match self.command_to_type(
                    command,
                    shell_type,
                    in_band_command_id.is_none(),
                    session_id,
                ) {
                    TypedCommand::Line(line) => Cow::Owned(bytes_to_execute_command(
                        line.as_str(),
                        shell_type,
                        self.is_bracketed_paste_enabled,
                    )),
                    // Exactly the two chord bytes: no kill-buffer prefix (the handler reverts
                    // the line), no command text, no Enter.
                    #[cfg(not(target_family = "wasm"))]
                    TypedCommand::FileChord => Cow::Borrowed(long_command_file::INSERT_CHORD),
                    // Never produced on wasm (`command_to_type` has no pipe there).
                    #[cfg(target_family = "wasm")]
                    TypedCommand::FileChord => Cow::Borrowed(&b"\x1b3"[..]),
                };
                (bytes, true, on_write_fn, false)
            }
            PtyWrite::AgentInput { bytes, mode } => {
                let decorated_bytes =
                    mode.decorate_bytes(bytes.into_owned(), self.is_bracketed_paste_enabled);
                (decorated_bytes.into(), false, None, false)
            }
            PtyWrite::Bytes { bytes } | PtyWrite::PasteBytes { bytes } => {
                (bytes, false, None, false)
            }
            PtyWrite::TmuxCommand(command) => {
                let command = command.get_command_string();
                debug_assert!(
                    command.ends_with('\n'),
                    "Tmux commands must end in a newlines so they are executed"
                );
                debug_assert!(
                    self.tmux_control_mode.is_some(),
                    "Received tmux command outside of control mode."
                );
                (command.into_bytes().into(), false, None, true)
            }
            PtyWrite::RunNativeShellCompletions(state) => {
                if let NativeShellCompletionsState::AwaitingPrompt { buffer_text, .. } = &state {
                    self.native_inflight_buffer = buffer_text.clone();
                }
                self.native_attached_waiters.clear();
                self.in_flight_native_completions_state = Some(state);
                self.arm_native_completions_watchdog(NATIVE_COMPLETIONS_PROMPT_TIMEOUT, true, ctx);

                // Send a ^Y control code to trigger the right bindkey.  We
                // then wait for an OSC-based signal from the shell before we
                // send the text that needs to be completed.
                let bytes = vec![0x19_u8];
                (bytes.into(), false, None, false)
            }
        };

        // The terminal hangs if we send 0 bytes through.
        if bytes_to_write.is_empty() {
            return false;
        }

        let bytes_to_write = match &mut self.tmux_control_mode {
            None => bytes_to_write,
            Some(_) if raw_tmux_command => bytes_to_write,
            Some(TmuxControlMode::Pending { buffer, overflowed }) => {
                // #1442: held until the primary pane is known, which may never come; bounded
                // like the ssh hold. Past the cap the new bytes are dropped (never a prefix of a
                // line: whole writes only), logged once, and the user is told.
                if buffer.len() + bytes_to_write.len() > MAX_TMUX_PENDING_BYTES {
                    if !*overflowed {
                        *overflowed = true;
                        log::warn!(
                            "tmux control mode: pending input buffer full ({} bytes), dropping further input until the session is ready",
                            buffer.len()
                        );
                        ctx.emit(PtyControllerEvent::InputBufferOverflow);
                    }
                    return false;
                }
                buffer.extend_from_slice(&bytes_to_write);
                return false;
            }
            Some(TmuxControlMode::Active { primary_pane }) => {
                crate::terminal::model::tmux::format_input(*primary_pane, &bytes_to_write)
                    .as_bytes()
                    .to_owned()
                    .into()
            }
        };

        if let Some(on_write_fn) = on_write_fn {
            if !on_write_fn().is_accepted() {
                self.clear_file_insert();
                return false;
            }
        }

        if is_for_command {
            self.log_ctrl_c_to_next_command("command_written_after_ctrl_c", false);
            self.line_editor_status
                .update(ctx, |line_editor_status, ctx| {
                    line_editor_status.did_execute_command(ctx)
                });
        }

        if let Some(shell_type) = shell_type_for_split
            && let Some((kill_buffer, rest)) = split_kill_buffer_write(&bytes_to_write, shell_type)
        {
            self.send_message_to_event_loop(Message::Input(Cow::Owned(kill_buffer.to_vec())), ctx);
            let rest = Cow::Owned(rest.to_vec());
            self.send_message_to_event_loop(
                if is_user_command {
                    Message::CommandInput(rest)
                } else {
                    Message::Input(rest)
                },
                ctx,
            );
            return true;
        }

        if is_interrupt {
            warpui::input_order::log_hop("ctrl_c_to_event_loop", String::new);
            let now = std::time::Instant::now();
            self.ctrl_c_sent_at = Some(now);
            self.ctrl_c_next_command_at = Some(now);
            self.end_native_exchange_awaiting_results_on_interrupt();
            self.send_message_to_event_loop(Message::DiscardQueuedInput, ctx);
        }
        // On a pty with the write queue, a paste and an agent write are their own job classes
        // (#1384 P4), so the writer can abort them by scope; without it (ConPTY, the kill
        // switch) they travel as before.
        let message = if self.write_queue && is_paste {
            Message::PasteInput(bytes_to_write)
        } else if self.write_queue && is_agent_input {
            Message::AgentInput(bytes_to_write)
        } else if is_user_command || is_paste {
            Message::CommandInput(bytes_to_write)
        } else {
            Message::Input(bytes_to_write)
        };
        self.send_message_to_event_loop(message, ctx);
        true
    }

    /// Input-order log only (#1401): one hop of the ^C -> next prompt chain, with the time since
    /// the last lone ^C was handed to the event loop. Does not stop the clock (the chain ends at
    /// the line editor's ready signal) and decides nothing.
    fn log_since_ctrl_c(&self, what: &str) {
        if let Some(sent) = self.ctrl_c_sent_at {
            warpui::input_order::log_hop(what, || {
                format!("since_ctrl_c_ms={:.1}", sent.elapsed().as_secs_f64() * 1e3)
            });
        }
    }

    /// Input-order log only (#1401): one hop of the ^C -> next command chain (see
    /// `ctrl_c_next_command_at`); `end` stops the clock (the next command's Preexec).
    fn log_ctrl_c_to_next_command(&mut self, what: &str, end: bool) {
        let sent = if end {
            self.ctrl_c_next_command_at.take()
        } else {
            self.ctrl_c_next_command_at
        };
        if let Some(sent) = sent {
            warpui::input_order::log_hop(what, || {
                format!("since_ctrl_c_ms={:.1}", sent.elapsed().as_secs_f64() * 1e3)
            });
        }
    }

    /// Sends a message to the event loop. If the send fails with `SendError::Disconnected`, emits
    /// a `PtyDisconnected` event.
    fn send_message_to_event_loop(&self, message: Message, ctx: &mut ModelContext<Self>) {
        // #1793: every write to the pty is counted, so the ssh escape path can tell that nothing
        // reached the shell's line since its last report. #2186: and the last byte written is
        // kept (whether it was a newline), which is whether ssh is at its line start. This is the
        // one place every write the controller hands the pty passes through (user keys, commands,
        // pastes, agent and viewer writes, the hand-off's `ESC i`, init and shim payloads).
        match &message {
            Message::Input(bytes)
            | Message::CommandInput(bytes)
            | Message::PasteInput(bytes)
            | Message::AgentInput(bytes) => self.pty_writes.note_write_bytes(bytes),
            Message::SetupChunked { chunks, .. } => {
                self.pty_writes.note_write();
                if let Some(last) = chunks.iter().rev().find(|chunk| !chunk.is_empty()) {
                    self.pty_writes.note_tail(last);
                }
            }
            _ => {}
        }
        match self.event_loop_tx.send(message) {
            Err(EventLoopSendError::Disconnected) => {
                ctx.emit(PtyControllerEvent::PtyDisconnected);
            }
            Err(e) => {
                log::warn!("Unable to send event loop msg {e:?}");
            }
            _ => (),
        }
    }

    pub fn run_native_shell_completions(
        &mut self,
        buffer_text: String,
        options: NativeCompletionsRequestOptions,
        results_tx: async_channel::Sender<Vec<ShellCompletion>>,
        ctx: &mut ModelContext<Self>,
    ) {
        // Never start a new native-completions request while one is already in flight, in
        // *either* phase. `AwaitingResults` deliberately does not gate `can_write_to_pty` (typing
        // should not block on completions results for up to
        // `NATIVE_COMPLETIONS_RESULTS_TIMEOUT`), so without this check a newly queued request
        // could be dequeued and dispatched immediately behind an `AwaitingResults` request that
        // is still live -- e.g. from `CompletionsTrigger::AsYouType` firing again on the next
        // keystroke. The dispatch arm below unconditionally overwrites
        // `in_flight_native_completions_state`, so that would silently drop the current request's
        // `results_tx` and hand its eventual, now-mismatched `CompletionsFinished` reply to the
        // *new* request instead -- delivering one request's completions as if they were another
        // request's answer. Neither event carries a request id to tell them apart after the fact,
        // so the only structural fix is to make sure at most one request ever exists at a time.
        //
        // Dropping `results_tx` here (by simply not queuing anything) closes its channel
        // immediately, so the caller's `results_rx.recv().await.ok()` resolves to `None` right
        // away -- the same fallback shape an abandoned request already produces, and much better
        // than either hanging or being handed some other request's results.
        //
        // #1801 (design point a): the exchange also starts only when nothing else is queued or
        // engaged, and then at once: the `^Y` is never queued, so nothing can sit behind it.
        // A fast Tab, Tab, Enter: a second request for the SAME text while an exchange is in
        // flight is attached to it and gets its answer, instead of being refused as "no answer"
        // (which would hand a waiting Enter's line back). A different text, or a given-up
        // exchange that owes its cancel, is a real refusal.
        if self.in_flight_native_completions_state.is_some()
            && !self.native_cancel_owed
            && self.native_inflight_buffer == buffer_text
        {
            log::info!("Native completions request attached to the exchange in flight");
            self.native_attached_waiters.push(results_tx);
            return;
        }
        if let Err(why) = self.native_exchange_may_start(ctx) {
            log::info!("Native completions not started ({why}); the Tab uses the completer");
            return;
        }
        self.send_write_to_event_loop(
            PtyWrite::RunNativeShellCompletions(NativeShellCompletionsState::AwaitingPrompt {
                buffer_text,
                results_tx,
                remote_first: options.remote_first,
            }),
            ctx,
        );
    }
}

pub enum PtyControllerEvent {
    /// Emitted when the event loop thread has exited.
    PtyDisconnected,
    /// The typeahead hand-off changed who owns the user's keystrokes, so the surface should
    /// re-evaluate focus (and arm a report deadline after a request).
    TypeaheadHandoffChanged,
    /// The editor's command was refused, so it is never written and the hand-off will NOT engage
    /// for it (#1353 phase 6): the negative event keys waiting for the route to settle are
    /// released on.
    TypeaheadHandoffNotEngaged,
    /// The shell's line editor signalled it is ready while the view's input queue held keys for
    /// the prompt (#1353 phase 5, ConPTY): the view releases that gate (`hold_id` names it).
    PromptHoldReleased { hold_id: u64 },
    /// A command was written (or counted) while the view's input queue held the keys of a young
    /// hold, and the hand-off ended that hold (#1353 phase 5b): the view takes or drops the
    /// gate's entries.
    YoungHoldEnded,
    /// Input was dropped because a buffer on the way to the pty was full (#1442): the surface
    /// tells the user.
    InputBufferOverflow,
}

impl<T: EventLoopSender> Entity for PtyController<T> {
    type Event = PtyControllerEvent;
}

/// Whether `write` is a command held behind the user's typeahead: a shell command (not in-band)
/// whose block is started only when it is written.
fn is_held_handoff_command(write: &PtyWrite) -> bool {
    matches!(
        write,
        PtyWrite::Command {
            in_band_command_id: None,
            park: CommandPark::BehindUserLine,
            ..
        }
    )
}

/// #2182 (b): whether `write` is the user's command parked behind the app's own in-band command.
fn is_parked_user_command(write: &PtyWrite) -> bool {
    matches!(
        write,
        PtyWrite::Command {
            park: CommandPark::BehindInBand(_),
            ..
        }
    )
}

/// The sources whose command is the user's own (the editor's Enter, the queued-prompts panel, an
/// env-var collection the user ran): the ones #2182 (b) never drops behind a side call.
fn is_users_command_source(source: &CommandExecutionSource) -> bool {
    matches!(
        source,
        CommandExecutionSource::User
            | CommandExecutionSource::QueuedCommand
            | CommandExecutionSource::EnvVarCollection { .. }
    )
}

/// A source's name for the input-order log.
fn command_source_name(source: &CommandExecutionSource) -> &'static str {
    match source {
        CommandExecutionSource::AI { .. } => "ai",
        CommandExecutionSource::SharedSession { .. } => "shared_session",
        CommandExecutionSource::User => "user",
        CommandExecutionSource::QueuedCommand => "queued_command",
        CommandExecutionSource::EnvVarCollection { .. } => "env_var_collection",
    }
}

/// The bookkeeping of a command whose block has just started, done under the model's lock by
/// [`PtyController::write_command`] at the submit and by a parked user command's
/// `before_write_fn` at its write (#2182 b): one function, so the two cannot drift. Returns
/// whether the typeahead hand-off engaged for the command.
fn note_command_block_started(
    model: &mut TerminalModel,
    command: &str,
    is_editor_command: bool,
) -> bool {
    // The key path reads the active block's state from the published facts (#1403); a
    // command that starts here changes it.
    model.publish_key_facts();

    // Ensure that the `TerminalModel` doesn't interpret any of the PTY output from the
    // following commands as in-band command output. If the in-band command output is not
    // currently being received by the `TerminalModel`, this is a no-op.
    model.end_in_band_command_output(false);

    if !FeatureFlag::TypeaheadHandoff.is_enabled() {
        return false;
    }
    // A shell that cannot report its line keeps the pre-hand-off routing: nothing
    // could hand the keys left in its line back to the editor.
    let can_report =
        model.block_list().early_output().typeahead_mode() == TypeaheadMode::ShellReported;
    let engages_typeahead_handoff = is_editor_command && can_report;
    if engages_typeahead_handoff {
        // Keys typed behind the command's Enter (a held young Enter, a deferred
        // Enter's gate) follow it to the pty: the hand-off records the end and the
        // view writes them through `write_handoff_user_bytes`, which records their
        // echo (#1033) and keeps them behind a command that is still parked.
        {
            let mut handoff = model.typeahead_handoff_mut();
            handoff.on_command_written();
            // #1793: its own Preexec is told from a line queued ahead by its text.
            handoff.note_written_command_text(command);
        }
        model
            .block_list_mut()
            .early_output_mut()
            .clear_typeahead_for_new_command();
    } else {
        // Not the hand-off's command, but its prompt will still be counted.
        let mut handoff = model.typeahead_handoff_mut();
        if is_editor_command {
            // The user's own command: the keys held behind its Enter follow it.
            handoff.on_unengaged_user_command_written();
        }
        handoff.on_uncounted_command_written();
    }
    engages_typeahead_handoff
}

/// Starts the active block for a command from `source`, attaching the source's metadata.
fn start_command_execution_for_source(
    model: &mut TerminalModel,
    source: CommandExecutionSource,
) -> StartCommandOutcome {
    match source {
        CommandExecutionSource::AI { metadata } => {
            model.start_command_execution_with_ai_metadata(metadata)
        }
        CommandExecutionSource::SharedSession {
            participant_id,
            ai_metadata,
            ..
        } => model.start_command_execution_for_shared_session(participant_id, ai_metadata),
        CommandExecutionSource::User | CommandExecutionSource::QueuedCommand => {
            model.start_command_execution()
        }
        CommandExecutionSource::EnvVarCollection { metadata } => {
            model.start_command_execution_from_env_var_collection(metadata)
        }
    }
}

/// What `command_to_type` decided to send for a command.
#[cfg_attr(target_family = "wasm", allow(dead_code))]
enum TypedCommand {
    /// Type this line (with the shell's kill-buffer chord and Enter around it).
    Line(String),
    /// The command is in the session's command file: send only the insert chord (#1581).
    FileChord,
}

/// Splits `shell_type`'s kill-buffer chord off the front of `bytes` (the output of
/// `bytes_to_execute_command`, which prepends it), returning `Some((kill_buffer_bytes, rest))`, or
/// `None` when there is nothing to split.
///
/// Only PowerShell needs this. Its kill-buffer chord is an ESC-prefixed sequence that PSReadLine
/// can fail to disambiguate when it arrives in the same read as the command text, leaving the
/// command typed on top of the buffer; writing the chord separately avoids it. The other three
/// shells use a single unambiguous control byte. The prefix is validated rather than assumed: a
/// non-matching prefix returns `None` (write whole) so a caller that passes something else is
/// never mis-cut. That validation is also what disables the split under tmux control mode, where
/// `tmux::format_input` has already rewrapped the bytes so they no longer start with the chord.
fn split_kill_buffer_write(bytes: &[u8], shell_type: ShellType) -> Option<(&[u8], &[u8])> {
    if shell_type != ShellType::PowerShell {
        return None;
    }
    let kill_buffer = shell_type.kill_buffer_bytes();
    if !bytes.starts_with(kill_buffer) {
        return None;
    }
    let (kill_buffer_bytes, rest) = bytes.split_at(kill_buffer.len());
    if rest.is_empty() {
        return None;
    }
    Some((kill_buffer_bytes, rest))
}

/// Whether `command` is written to the pty as one bracketed paste (the shell then takes it as a
/// single block) rather than line by line. The one decision `bytes_to_execute_command` makes, and
/// the one the input's multi-line confirmation asks (#1046).
pub(crate) fn command_is_sent_as_bracketed_paste(
    is_bracketed_paste_enabled: bool,
    command: &str,
) -> bool {
    is_bracketed_paste_enabled && !command.is_empty()
}

/// Whether `command` reaches `shell_type` as ONE line however many lines it has: bash gets a
/// command over the wrapper size as a single base64 line that it decodes and sources
/// (`PtyController::command_to_type`, #1129, #1255; folded onto continuation lines inside one
/// open quote when the shell has no bracketed paste), so its lines are not run one by one even
/// without bracketed paste.
pub(crate) fn command_is_sent_as_one_wrapped_line(
    shell_type: ShellType,
    is_bracketed_paste_enabled: bool,
    command: &str,
) -> bool {
    shell_type == ShellType::Bash
        && posix_long_command::needs_wrapper_for(command, is_bracketed_paste_enabled)
}

/// User writes at least this large are pastes, not keystrokes: they go out as
/// [`PtyController::write_paste_bytes`], so a ^C cancels the part not yet written (#1255).
pub(crate) const PASTE_WRITE_MIN_BYTES: usize = 32 * 1024;

/// Returns the shell-dependent array of bytes to be written to the PTY to execute `command`.
fn bytes_to_execute_command(
    command: &str,
    shell_type: ShellType,
    is_bracketed_paste_enabled: bool,
) -> Vec<u8> {
    use warp_util::path::ShellFamily;

    let mut command_bytes = shell_type.kill_buffer_bytes().to_vec();
    let command = match ShellFamily::from(shell_type) {
        ShellFamily::Posix if cfg!(windows) => LINEFEED_REGEX.replace_all(command, "\n"),
        ShellFamily::PowerShell => LINEFEED_REGEX.replace_all(command, "\r"),
        _ => Cow::Borrowed(command),
    };

    // Only execute the command via bracketed paste if the command is not empty. Some ZSH
    // bracketed paste magic functions return errors if bracketed paste is used without text
    // in-between the bracketed paste escape sequences.
    if command_is_sent_as_bracketed_paste(is_bracketed_paste_enabled, &command) {
        match shell_type {
            ShellType::Fish => {
                // Fish strips leading (and trailing) whitespace from pasted commands (entered via
                // bracketed paste). To ensure that leading whitespace is preserved, first append
                // leading whitespace bytes and then surround the remaining command string with the
                // bracketed paste sequence. Conceptually, this would be like manually typing in
                // the whitespace into the fish line editor, and then pasting in the command.
                //
                // The leading whitespace is particularly meaningful in fish because it causes the
                // following command to be omitted from history (like the HISTIGNORESPACE option
                // in zsh).
                //
                // We don't care about preserving trailing whitespace; it would just take up
                // unnecessary space in the blocklist.
                let (leading_whitespace, rest_of_command) =
                    command.split_at(command.len() - command.trim_start().len());
                command_bytes.extend(leading_whitespace.as_bytes());
                command_bytes.extend(wrap_bytes_in_bracketed_paste(
                    rest_of_command
                        .replace(escape_sequences::C0::ESC as char, "")
                        .into_bytes(),
                ));
            }
            _ => command_bytes.extend(wrap_bytes_in_bracketed_paste(
                command
                    .replace(escape_sequences::C0::ESC as char, "")
                    .into_bytes(),
            )),
        }
    } else {
        let command_without_escapes = command.replace(escape_sequences::C0::ESC as char, "");
        command_bytes.extend(command_without_escapes.as_bytes());
    }
    command_bytes.extend(shell_type.execute_command_bytes().to_vec());
    command_bytes
}

/// Returns a vector containing the given `bytes` wrapped in bracketed paste start and end
/// sequences.
fn wrap_bytes_in_bracketed_paste(bytes: impl IntoIterator<Item = u8>) -> impl Iterator<Item = u8> {
    escape_sequences::BRACKETED_PASTE_START
        .iter()
        .copied()
        .chain(bytes)
        .chain(escape_sequences::BRACKETED_PASTE_END.iter().copied())
}

#[derive(Error, Debug)]
pub enum EventLoopSendError {
    #[error("Unable to send message: receiver is disconnected")]
    Disconnected,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Test seam (#2182 b): the event loop of a real `PtyController` wired to a test view. Records
/// the messages the controller sends, so a view test can read what would reach the pty.
#[cfg(test)]
#[derive(Clone, Default)]
pub(crate) struct RecordingEventLoop {
    messages: Arc<parking_lot::Mutex<Vec<Message>>>,
}

#[cfg(test)]
impl EventLoopSender for RecordingEventLoop {
    fn send(&self, message: Message) -> Result<(), EventLoopSendError> {
        self.messages.lock().push(message);
        Ok(())
    }
}

#[cfg(test)]
impl RecordingEventLoop {
    /// Everything written to the pty so far (commands and keys), as text.
    pub(crate) fn written_text(&self) -> String {
        let mut bytes = Vec::new();
        for message in self.messages.lock().iter() {
            if let Message::Input(input) | Message::CommandInput(input) = message {
                bytes.extend_from_slice(input);
            }
        }
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

pub trait EventLoopSender: 'static {
    fn send(&self, message: Message) -> Result<(), EventLoopSendError>;

    /// Whether the event loop behind this sender runs the typed write queue (`SetupChunked` and
    /// `Abort`, #1384). Only the local unix pty loop does.
    fn supports_write_queue(&self) -> bool {
        false
    }
}

#[cfg(test)]
#[path = "pty_controller_command_bytes_tests.rs"]
mod command_bytes_tests;
#[cfg(test)]
#[path = "pty_controller_lifecycle_tests.rs"]
mod lifecycle_tests;
#[cfg(test)]
#[path = "pty_controller_pwsh_insert_tests.rs"]
mod pwsh_insert_tests;
#[cfg(test)]
#[path = "pty_controller_tests.rs"]
mod tests;
