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
    ActiveDecision, HeldAgentCommand, PaneHandoff, ShellPromptFact,
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
use super::write_queue::{AbortScope, Pacing};

/// Gap between the chunks of a bootstrap written into a subshell behind a user-space pty relay
/// (`sudo -i`, `su`, `script`, `ksu`, ...). #960: such a relay deadlocks when ~120KB is flooded
/// at it in one burst, and how fast it consumes its input is not observable from here (the echo
/// it produces is not byte-exact), so this is a time gap by necessity, not a way to decide
/// whether a chunk is written.
#[cfg(feature = "local_fs")]
const RELAY_BOOTSTRAP_PACING: Duration = Duration::from_millis(25);
/// Gap between the chunks of a bootstrap written into a container subshell (`docker`/`podman
/// exec -it`): a double-pty proxy that drops data on large writes or bursts (#960 family).
/// Same reason as [`RELAY_BOOTSTRAP_PACING`]: the proxy's consumption is unobservable.
#[cfg(feature = "local_fs")]
const CONTAINER_BOOTSTRAP_PACING: Duration = Duration::from_millis(50);

/// Gap between the 1000-byte chunks of the ssh-warpify line-clear payload while the tty is
/// canonical (cooked, no newline in the line). Reason: P0 (#1384) measured that a canonical tty
/// accepts a write and then SILENTLY drops everything past its input queue (macOS: 1024 B; Linux
/// N_TTY: 4095 B), with no `EAGAIN`, no `POLLOUT` change and no readable counter in advance, so
/// there is no signal to wait on and the gap stays. A non-canonical tty (a bash/zsh line editor at
/// its prompt) gives a real `EAGAIN` instead, and then no gap is used (see
/// [`Pacing::GapIfCanonical`]).
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
const NATIVE_COMPLETIONS_PROMPT_TIMEOUT: Duration = Duration::from_secs(2);

/// How long to wait for completion results once the prompt has been sent.
///
/// Unlike the prompt handshake this covers real work in the shell -- globbing a large directory,
/// say -- so it is far more generous. It does not gate PTY writes; it exists so the completions
/// request cannot leak its channel and hang the requester's future forever.
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

/// How long a typeahead hand-off request may wait on lines the counts consider queued, with no
/// precmd or preexec arriving, before those lines are taken as consumed by something other than
/// the shell (a program reading stdin, an empty or continued line) and the counts are reset.
const TYPEAHEAD_HANDOFF_STALE_COUNT_RESET: Duration = Duration::from_millis(250);

/// #1353 phase 6 (PTY-3): with the prompt-sequence path the stale line counts are reset by events
/// -- the sampled shell state at decision time (a program owns the terminal), the finished
/// command having been interactive (see `TypeaheadHandoff::note_interactive_command_finished`),
/// and the prompt's Precmd -- so this is only the one logged give-up for a continued line (PS2) or
/// a shell that reports nothing, which no event can tell from "still working through the queue".
const TYPEAHEAD_HANDOFF_STALE_COUNT_GIVE_UP: Duration = Duration::from_secs(5);

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

enum NativeShellCompletionsState {
    AwaitingPrompt {
        buffer_text: String,
        results_tx: async_channel::Sender<Vec<ShellCompletion>>,
    },
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
    /// Where a key's write leaves its bookkeeping for the model (the typed-ahead record of its
    /// echo), applied before the model parses the next bytes.
    model_inbox: ModelInbox,
    /// Typeahead hand-off: the last request decision had to wait. Queued writes stay queued while
    /// the shell's line may hold user bytes, which a queued command's kill prefix would destroy.
    handoff_request_waiting: bool,
    /// Typeahead hand-off: a command the hand-off does not own is queued behind the user's
    /// unreported line (`hold_command_behind_unreported_user_line`). At most one is held.
    handoff_held_command_pending: bool,
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
        ctx.subscribe_to_model(&model_event_dispatcher, |me, event, ctx| match event {
            ModelEvent::Handler(AnsiHandlerEvent::UserCommandFinished) => {
                me.log_since_ctrl_c("command_finished_after_ctrl_c");
                me.is_user_command_executing = false;
            }
            ModelEvent::Handler(AnsiHandlerEvent::InitShell {
                pending_session_info,
            }) => {
                me.initialize_shell(pending_session_info.as_ref(), ctx);
            }
            ModelEvent::Handler(AnsiHandlerEvent::Bootstrapped { is_subshell, .. }) => {
                me.shell_bootstrapped(*is_subshell);
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
                // and returns the old value; matching on that returned value in a `let-else`
                // still leaves the slot empty even when the match fails. If this event is a late
                // reply for a request that has since been superseded (its watchdog fired and a
                // newer request is now in flight, in whichever phase), taking first would destroy
                // that newer, still-live request's state -- e.g. an `AwaitingPrompt` for request B
                // -- and neither the shell nor the app would ever finish B's handshake, leaving
                // zsh stuck inside `read -d $'\4'` forever. See the entry in TODO.md ("The shell
                // lockup") for the full trace. So: peek, and only take when it actually matches.
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
            }
            ModelEvent::SendCompletionsPrompt => {
                // Same peek-before-take reasoning as `CompletionsFinished` above.
                //
                // If this event has no matching live state, we do *not* write anything to the
                // PTY -- not even a bare terminator. An earlier version of this fix tried to
                // answer a late reply for an already-abandoned request with a bare EOT, on the
                // theory that the shell must still be blocked in `read -d $'\4'` waiting for one.
                // That assumption is unsafe: the shell's own `read` in
                // `warp_read_completion_buffer` (zsh_body.sh) can return for reasons the app
                // cannot observe -- most plausibly the user hitting Ctrl-C after a Tab that
                // visibly did nothing, which is exactly what the 2s prompt watchdog produces.
                // SIGINT aborts that `read`, and the shell is back at a live, empty prompt. A
                // blind EOT sent after that point is Ctrl-D typed at an interactive prompt, which
                // exits the shell outright unless IGNOREEOF happens to be set. Recovering the
                // shell from a stuck handshake now belongs entirely to the shell side (a bounded
                // `read -t` in `warp_read_completion_buffer`, per TODO.md and phosphor#770); the
                // app's only safe move on the app side is to never type anything into a prompt it
                // cannot see the state of.
                if !matches!(
                    me.in_flight_native_completions_state,
                    Some(NativeShellCompletionsState::AwaitingPrompt { .. })
                ) {
                    log::warn!(
                        "Received SendCompletionsPrompt event but wasn't awaiting a prompt \
                         (generation {}); ignoring it without touching the in-flight state.",
                        me.native_completions_generation
                    );
                    return;
                }
                let Some(NativeShellCompletionsState::AwaitingPrompt {
                    buffer_text,
                    results_tx,
                }) = me.in_flight_native_completions_state.take()
                else {
                    unreachable!("checked above");
                };
                me.in_flight_native_completions_state =
                    Some(NativeShellCompletionsState::AwaitingResults { results_tx });
                me.arm_native_completions_watchdog(NATIVE_COMPLETIONS_RESULTS_TIMEOUT, false, ctx);

                let mut bytes = buffer_text.into_bytes();
                // We use the EOT character to signal the end of the prompt.
                bytes.push(escape_sequences::C0::EOT);

                // We send the write directly to the event loop without
                // queueing, as we currently have exclusive control over pty
                // writes.
                me.send_write_to_event_loop(
                    PtyWrite::Bytes {
                        bytes: bytes.into(),
                    },
                    ctx,
                );

                // Now that we've provided the prompt, we can start executing
                // other queued writes.
                me.execute_next_queued_write(ctx);
            }
            _ => (),
        });

        ctx.subscribe_to_model(&line_editor_status, |me, event, ctx| {
            if let LineEditorStatusEvent::Active = event {
                me.log_ctrl_c_to_next_command("line_editor_active_after_ctrl_c", false);
                if FeatureFlag::TypeaheadHandoff.is_enabled() {
                    me.decide_typeahead_handoff_request(false, ctx);
                    return;
                }
                let input_reporting_seq = me
                    .model_event_dispatcher
                    .as_ref(ctx)
                    .active_session_id()
                    .and_then(|id| me.sessions.as_ref(ctx).get(id))
                    .and_then(|session| session.shell().input_reporting_sequence());
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

        let write_queue = event_loop_tx.supports_write_queue() && write_queue_enabled();
        let (handoff, model_inbox) = {
            let model = terminal_model.lock();
            (model.pane_handoff(), model.model_inbox())
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
            foreground_sigint: None,
            pty_write_gate_blocked_since: None,
            has_logged_current_pty_write_stall: false,
            handoff_command_pending: false,
            handoff_bytes_after_command: Vec::new(),
            handoff,
            model_inbox,
            handoff_request_waiting: false,
            handoff_held_command_pending: false,
            handoff_command_written_since_hold: false,
            paced_write_queue: VecDeque::new(),
            paced_write_generation: 0,
            write_queue,
            ctrl_c_sent_at: None,
            ctrl_c_next_command_at: None,
            setup_job_sent: false,
            #[cfg(test)]
            shutdown_requested: false,
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
        if self.handoff_command_pending {
            self.execute_next_queued_write(ctx);
            return;
        }

        let input_reporting_seq = self
            .model_event_dispatcher
            .as_ref(ctx)
            .active_session_id()
            .and_then(|id| self.sessions.as_ref(ctx).get(id))
            .and_then(|session| session.shell().input_reporting_sequence());
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
                self.handoff_request_waiting = false;
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
                    self.handoff_request_waiting = true;
                    return;
                }
                self.handoff_request_waiting = false;
                self.execute_next_queued_write(ctx);
            }
            ActiveDecision::Committed => {
                self.handoff_request_waiting = false;
                self.execute_next_queued_write(ctx);
                ctx.emit(PtyControllerEvent::TypeaheadHandoffChanged);
            }
            ActiveDecision::SendRequest => {
                self.handoff_request_waiting = false;
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
                self.handoff_request_waiting = false;
                self.execute_next_queued_write(ctx);
                self.arm_typeahead_handoff_stale_count_reset(activity_generation, ctx);
            }
            ActiveDecision::Wait {
                activity_generation,
            } => {
                self.handoff_request_waiting = true;
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
        let queued_hold = self
            .terminal_model
            .lock()
            .typeahead_handoff_mut()
            .release_queued_prompt_hold_on_signal();
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
        if is_powershell && line_editor_reading_activates() {
            warpui::input_order::log_hop("line_editor_reading_activates", || {
                format!("released_queued_hold={released_queued_hold}")
            });
            self.activate_line_editor_on_ready_signal(released_queued_hold, ctx);
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
        if FeatureFlag::TypeaheadHandoff.is_enabled()
            && !released_queued_hold
            && !self
                .terminal_model
                .lock()
                .typeahead_handoff()
                .has_unanswered_lines()
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
        if self.line_editor_status.as_ref(ctx).is_line_editor_active() {
            self.decide_typeahead_handoff_request(true, ctx);
        }
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
        let wait = if seq_mode {
            TYPEAHEAD_HANDOFF_STALE_COUNT_GIVE_UP
        } else {
            TYPEAHEAD_HANDOFF_STALE_COUNT_RESET
        };
        ctx.spawn(
            async move {
                Timer::after(wait).await;
            },
            move |me, _, ctx| {
                let (did_reset, diverged, young_hold_live, activity, markers_live) = {
                    let mut model = me.terminal_model.lock();
                    let mut handoff = model.typeahead_handoff_mut();
                    let diverged = handoff.counts_diverged();
                    let young_hold_live = handoff.young_hold_queued();
                    let activity = handoff.shell_activity();
                    let markers_live = handoff.line_editor_markers_live();
                    (
                        handoff.reset_stale_counts(activity_generation),
                        diverged,
                        young_hold_live,
                        activity,
                        markers_live,
                    )
                };
                if seq_mode && did_reset && diverged {
                    // The give-up of the wait fired: nothing said the queued lines were answered
                    // or consumed. Counts and the wait only, never bytes. The shell's last
                    // line-editor marker says which case it was (#1391): `Reading` with live
                    // markers is a line continued inside the editor with no continuation event
                    // (PowerShell on ConPTY), `Unknown` a bootstrap that sends no markers.
                    log::warn!(
                        "typeahead hand-off: no prompt, command start or terminal-owner change \
                         within {} s for lines the counts consider queued; treating them as consumed",
                        TYPEAHEAD_HANDOFF_STALE_COUNT_GIVE_UP.as_secs()
                    );
                    warpui::input_order::log_hop("handoff_stale_counts_reset", || {
                        format!(
                            "by=give_up shell_activity={activity:?} markers_live={markers_live}"
                        )
                    });
                }
                // Decide again whether or not a request was waiting: after a `DrainAndWait` the
                // reset is what hands input back to the editor.
                // (In the prompt-sequence path other events retry a wait that is not about the counts.)
                if did_reset && (diverged || !seq_mode) {
                    if young_hold_live {
                        // #1614: not now (a timer must not end the hold as typeahead: the
                        // decision's no-binding and committed-delivery branches would); decide on
                        // the next flushed write, or when the hold's own end asks for it.
                        me.handoff_request_waiting = true;
                    } else {
                        me.retry_typeahead_handoff_request(ctx);
                    }
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
        self.pending_writes.retain(|pty_write| {
            !matches!(pty_write, PtyWrite::Command {
                 in_band_command_id, ..
             } if in_band_command_id.as_deref() == Some(command_id))
        });
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
        self.pending_writes.push_back(PtyWrite::Command {
            command: command.to_owned(),
            shell_type,
            in_band_command_id: Some(command_id),
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

    /// Arms a watchdog that abandons an in-flight native-completions handshake if the shell
    /// never answers.
    ///
    /// This exists because `AwaitingPrompt` gates `can_write_to_pty`, and nothing else could
    /// ever clear it. The handshake is: send `^Y`, wait for the shell hooks to reply with an OSC
    /// sequence (`ModelEvent::SendCompletionsPrompt`), then send the text to complete. If that
    /// reply never comes -- which is exactly what happens in a shell that was never warpified,
    /// such as one entered via `su`, `sudo su -` or `ksu`, where `^Y` is just readline's yank --
    /// then `in_flight_native_completions_state` stays `AwaitingPrompt` forever.
    ///
    /// The consequence was a hard, permanent lockup of the pane, not a missing popup:
    /// `execute_next_queued_write` returns early whenever `can_write_to_pty` is false, so every
    /// subsequent keystroke and command queued into `pending_writes` and none of them were ever
    /// written. Recovery was impossible from the UI -- the only thing that re-drives the drain
    /// is `LineEditorStatusEvent::Active`, and that calls straight back into
    /// `execute_next_queued_write`, which re-checks the same gate and returns early again. A new
    /// prompt therefore did not help; the pane was dead until it was closed.
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
                // Never log the buffer text -- it is whatever the user was typing.
                log::warn!(
                    "Native shell completions timed out after {timeout:?} (awaiting_prompt={expect_awaiting_prompt}); \
                     abandoning the request so PTY writes can resume."
                );
                me.in_flight_native_completions_state = None;
                // Only the prompt phase gates writes, but draining unconditionally is harmless
                // and keeps the recovery path identical for both.
                me.execute_next_queued_write(ctx);
                // Only the prompt phase is reported. That phase timing out means the shell never
                // acknowledged the trigger at all, which is what an un-phosphorized shell looks
                // like; a results-phase timeout means the hooks did answer and then the work
                // outran its budget, for which "no shell integration" would simply be untrue.
                if expect_awaiting_prompt {
                    me.report_native_completions_unavailable(ctx);
                }
            },
        );
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
            // If we're in the middle of a native completions request, we should not send any more
            // writes to the shell until we've sent the string to complete.
            && !self.in_flight_native_completions_state.as_ref().is_some_and(|state| state.is_awaiting_prompt())
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
            self.handoff_request_waiting = false;
        }
        if self.handoff_holds_front_command() {
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
            let did_write = self.send_write_to_event_loop(write, ctx);
            if is_held_handoff_command {
                if did_write {
                    self.handoff_held_command_pending = false;
                    self.terminal_model
                        .lock()
                        .typeahead_handoff_mut()
                        .set_held_agent_command(None);
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
        if self.write_queue && self.tmux_control_mode.is_none() {
            // One job in the event loop's own FIFO (#1384): the loop honours the gap as its poll
            // deadline, nothing a user types can land inside the script, and an abort replaces
            // the generation below.
            self.cancel_paced_writes(ctx);
            self.setup_job_sent = true;
            self.send_message_to_event_loop(Message::SetupChunked { chunks, pacing }, ctx);
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

        if bootstrap::should_use_rc_file_bootstrap_method(shell_type, pending_session_info) {
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
    fn shell_bootstrapped(&mut self, is_subshell: bool) {
        if is_subshell {
            self.is_user_command_executing = false;
        }

        // Now that we have bootstrapped, we can be sure that the bootstrap
        // file is no longer needed.
        #[cfg(not(target_family = "wasm"))]
        self.bootstrap_file.take();
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
        if self.handoff_held_command_pending && !matches!(source, CommandExecutionSource::User) {
            // One held command at a time; a second would otherwise replace the first, whose
            // requester would wait for it forever.
            log::warn!("A command is already held behind the user's typeahead; coalescing.");
            return StartCommandOutcome::Coalesced;
        }
        if self.must_hold_command_behind_unreported_user_line(&source) {
            self.hold_command_behind_unreported_user_line(command, shell_type, source, None, ctx);
            return StartCommandOutcome::Accepted;
        }
        warpui::input_order::log_hop("write_command", String::new);
        let is_editor_command = matches!(source, CommandExecutionSource::User);
        let mut engages_typeahead_handoff = false;
        {
            let mut model = self.terminal_model.lock();

            // Explicitly start the block now that the command is executed.
            let outcome = start_command_execution_for_source(&mut model, source);
            if !outcome.is_accepted() {
                if is_editor_command {
                    // #1353 phase 6 (INP-10): the editor's command was refused, so it is never
                    // written, no block starts and the hand-off will not engage for this Enter:
                    // the keys waiting behind it must not wait for either (the negative event).
                    // A command that IS written but is not the hand-off's (flag off, a shell that
                    // cannot report) is followed by its block starting, the positive event.
                    drop(model);
                    ctx.emit(PtyControllerEvent::TypeaheadHandoffNotEngaged);
                }
                return outcome;
            }

            // The key path reads the active block's state from the published facts (#1403); a
            // command that starts here changes it.
            model.publish_key_facts();

            // Ensure that the `TerminalModel` doesn't interpret any of the PTY output from the
            // following commands as in-band command output. If the in-band command output is not
            // currently being received by the `TerminalModel`, this is a no-op.
            model.end_in_band_command_output(false);

            if FeatureFlag::TypeaheadHandoff.is_enabled() {
                // A shell that cannot report its line keeps the pre-hand-off routing: nothing
                // could hand the keys left in its line back to the editor.
                let can_report = model.block_list().early_output().typeahead_mode()
                    == TypeaheadMode::ShellReported;
                engages_typeahead_handoff = is_editor_command && can_report;
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
                    model.typeahead_handoff_mut().on_uncounted_command_written();
                }
            }
        }

        if self.handoff_held_command_pending {
            // A command from the editor goes first and the held command stays queued behind it,
            // to start at the prompt that follows; clearing the queue would drop it silently.
            self.pending_writes.retain(is_held_handoff_command);
            self.handoff_command_written_since_hold = true;
        } else {
            self.pending_writes.clear();
        }
        self.is_user_command_executing = true;

        // Send the write to the PTY event loop.
        let write = PtyWrite::Command {
            command: command.to_owned(),
            shell_type,
            in_band_command_id: None,
            before_write_fn: None,
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
            self.handoff_request_waiting = false;
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
        self.pending_writes.retain(|pty_write| {
            !matches!(
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
        if !self.handoff_held_command_pending
            && self.must_hold_command_behind_unreported_user_line(&source)
        {
            self.hold_command_behind_unreported_user_line(
                command,
                shell_type,
                source,
                Some(Box::new(annotate_block)),
                ctx,
            );
            return StartCommandOutcome::Accepted;
        }
        annotate_block(&mut self.terminal_model.lock());
        self.write_command(command, shell_type, source, ctx)
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
        !engages_typeahead_handoff
            && model.typeahead_handoff().holds_queued_writes()
            && model.is_at_prompt_for_typeahead_request()
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
        self.pending_writes.clear();
        self.is_user_command_executing = true;
        self.pending_writes.push_back(PtyWrite::Command {
            command: command.to_owned(),
            shell_type,
            in_band_command_id: None,
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
        self.handoff_request_waiting = true;
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

    /// Typeahead hand-off: whether the next queued write is a command that starts its own block
    /// when written (an in-band or held command) while the shell may still have user lines to
    /// run. It waits, or the output of those lines would land in its block.
    fn handoff_holds_front_command(&self) -> bool {
        FeatureFlag::TypeaheadHandoff.is_enabled()
            && matches!(
                self.pending_writes.front(),
                Some(PtyWrite::Command {
                    before_write_fn: Some(_),
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
    /// the activation fallback waits for that signal for the (long) give-up cap instead of
    /// firing 50 ms after the precmd and releasing held keys ahead of it.
    #[allow(dead_code)] // used only by tests
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
            let line = posix_long_command::wrapper_line(&command);
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
        if self.in_flight_native_completions_state.is_some() {
            return;
        }

        // Make sure we only have a single pending native shell completions
        // request at a time by dropping any existing ones from the queue.
        self.pending_writes
            .retain(|write| !matches!(write, PtyWrite::RunNativeShellCompletions(_)));

        self.pending_writes
            .push_back(PtyWrite::RunNativeShellCompletions(
                NativeShellCompletionsState::AwaitingPrompt {
                    buffer_text,
                    results_tx,
                },
            ));
        self.execute_next_queued_write(ctx);
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
            before_write_fn: Some(_),
            ..
        }
    )
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
/// (`PtyController::command_to_type`, #1129, #1255), so its lines are not run one by one even
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
