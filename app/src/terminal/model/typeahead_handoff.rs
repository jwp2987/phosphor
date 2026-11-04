//! Who owns the user's keystrokes around a command submitted from the input editor: the pty or
//! the editor (`FeatureFlag::TypeaheadHandoff`, #826/#827).
//!
//! From the moment such a command is written until the shell has been asked for its line buffer,
//! every key the user types is written to the pty, where the shell queues it as typeahead. The
//! request (the shell's input reporting sequence, `ESC i`) is written only once no Enter-terminated
//! line can still be queued in the tty, the shell is at its prompt and no routed user write is
//! still on its way to the pty. Writing it is the switch to the editor: no user byte is ever
//! written after a request, so the shell's report -- which also clears the shell's line -- covers
//! every byte the user typed.
//!
//! This module is pure. One [`PaneHandoff`] per terminal owns the [`TypeaheadHandoff`] behind its
//! own leaf lock (not the model's, #1403); the terminal view, the input editor and
//! `PtyController` feed it events and act on its decisions, and the model thread feeds it the
//! prompt facts it parses.
//!
//! # Prompt sequence numbers (#1353 phase 6)
//!
//! Every Precmd and Preexec the bootstrap hooks send carries `seq`, a per-shell counter that grows
//! by one for each hook of either kind. Once a shell has sent one (and `PHOSPHOR_PROMPT_SEQ` is not
//! `0`) [`TypeaheadHandoff::prompt_seq_mode`] is on and the decisions below are made from ordering
//! and sampled facts instead of elapsed time; a shell that never sends one (an older remote
//! bootstrap), the kill switch, or a session about which nothing can be sampled keeps the timing
//! path, which is left in place:
//!
//! | decision | timing path (kept) | prompt-sequence path |
//! |---|---|---|
//! | young Enter held for a prompt? | 600 ms window since the last activity, hold sized by 3x the p90 round trip (only where the platform can sample nothing, see below) | positive evidence only ([`YoungHoldEvidence`], #1614): the request is due (the counts say the prompt is back and nothing is queued, or the sample proves them stale), or a Precmd may be in flight (the line editor reads and the reader-side barrier says the stream is not applied yet); anything else is delivered to the pty as typed. Released by the report, the barrier's next publish, a Preexec, a program taking the tty, a command, a signal; give-up [`EVENT_HOLD_GIVE_UP`] hands back |
//! | line begun at an unnoticed prompt ("young line")? | began within the window | same facts |
//! | hand-back after Ctrl-C at the prompt | 250 ms settle | prompt cycle shells: the cycle's Precmd; zsh: the report of a request written behind the interrupt; one 2 s logged give-up |
//! | Enters consumed by a program | 250 ms quiet period | the finished command was interactive (latch, read on the model thread), the sampled state at decision time; one 5 s logged give-up |
//! | ConPTY request after a key | 120 ms quiet | `writes_in_flight == 0` and no key held in the view's queue (the queue boundary) |
//!
//! # Where held keys live (#1353 phase 5)
//!
//! This module keeps no bytes for the ConPTY prompt hold (a `LineEditorActive` gate, released by
//! the line editor's signal), for the young-Enter hold (a `PromptSeen` gate keyed by the hold's
//! id) nor for the keys behind a deferred Enter (a `HandoffReport` gate keyed by
//! [`TypeaheadHandoff::deferred_gate`]). A hold is only a marker here; every way it can end records a
//! [`YoungHoldEnd`] (or hands the id over synchronously for a signal) and the view turns that into
//! taking or dropping the gate's entries. A key the queue cannot take at that moment (a write made
//! while the queue is delivering something) is put back under its gate by the view when that
//! delivery returns.
//!
//! Still timing, because no signal exists: the young-Enter window and its round-trip learning where
//! the platform can say nothing about the shell (ConPTY, a session with no sample), the PS2
//! continued line (no Precmd, no Preexec) for a bootstrap that sends no line-editor markers, and
//! every give-up above. A shell the barrier is not installed for (fish writes its Precmd after raw
//! mode) keeps the 2 s event hold of before #1614 when its editor is sampled reading
//! ([`YoungHoldEvidence::PrecmdMaybeInFlight`], owner decision 2026-11-03).
//!
//! # The young-Enter hold engages on evidence, not on absence (#1614)
//!
//! The hold of #933 keeps an Enter (and the keys behind it) in the input queue until the shell's
//! report brings the line back into the editor, where the Enter is classified. Its release is the
//! request/report cycle, which needs the shell at its line editor with the counts agreeing that
//! no line is queued. Until #1614 the hold engaged on weaker grounds -- "the counts say the prompt
//! is back" or "the sample says a raw tty with the shell in front" -- and whenever those held while
//! the release did not (a `read x` that consumed an Enter the counts still queue, an inner shell
//! with no hooks, an ssh, a line typed into a running program) no event could end it and the 2 s
//! give-up handed the Enter back to the editor: the program never received it. The hold now
//! engages only when it can name the event that ends it ([`YoungHoldEvidence`]), every other Enter
//! goes to the pty as a plain terminal would deliver it, and an event that shows the hold's premise
//! gone (a Preexec, a program taking the tty, no request possible) writes the held keys as
//! typeahead ([`YoungHoldEnd::Typeahead`]). The timer is the net: it logs the evidence that failed.

use std::collections::VecDeque;
use std::mem;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use warpui_core::sync_shim::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use instant::Instant;

use crate::ai::agent::AIAgentActionId;
use crate::ai::agent::conversation::AIConversationId;
use crate::ai::blocklist::input_model::{ENTER_VERDICT_BUDGET, large_text_compute_allowance};

/// How long after a command was written (or a prompt or preexec seen) an Enter the user types
/// while the pty owns input may still be a line typed at a prompt the app has not noticed yet
/// (#933). The shell logs its precmd before the app has processed it, so a line typed and
/// Entered right after a command finished reaches the pty while the app still counts the command
/// as running; written as typeahead it would run as a shell command, unclassified. Such an Enter
/// is held until the prompt arrives, then routed like an Enter in the editor; if no prompt comes
/// within this window the command is running and the Enter is written as typeahead.
pub const YOUNG_ENTER_WINDOW: Duration = Duration::from_millis(600);

/// The shortest a held young Enter waits for the prompt. The window runs from the last activity
/// (command write, precmd, preexec), not from the Enter, so an Enter typed late in the window
/// waits only for what is left of it; this floor is the lag between the shell logging its precmd
/// and the app processing it, which is all the race in [`YOUNG_ENTER_WINDOW`] needs.
pub const YOUNG_ENTER_MIN_HOLD: Duration = Duration::from_millis(150);

/// The shortest a young Enter is held once round trips have been measured: the lag between the
/// shell logging its precmd and the app having processed it on this machine (a pty read, a parse
/// and a main-thread hop), with room for one slow frame.
pub const YOUNG_ENTER_FLOOR: Duration = Duration::from_millis(20);

/// How many round trips the hold's measured bound is the p90 of. A rolling window, so a stalled
/// machine's huge sample ages out instead of inflating the hold for the session.
const RTT_WINDOW: usize = 16;

/// The hold is this many measured round trips (p90). The late precmd of #933 was emitted before
/// the Enter was typed, so it is in flight for at most one trip (one way, plus the shell's own
/// work, which a request's round trip includes); two more trips absorb jitter.
const RTT_HOLD_FACTOR: u32 = 3;

/// What the platform can tell, right now, about whether the shell is at a prompt (#1353 phase 6).
/// A sampled fact, not a guess about elapsed time: each outcome is correct whenever it was
/// sampled, because a shell returning to the foreground and starting its line editor strictly
/// precedes the output of its next prompt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellPromptFact {
    /// The shell provably cannot be at a prompt: a plain program owns the foreground, or a program
    /// is reading a password.
    NotAtPrompt,
    /// The shell owns the foreground and its tty is raw: its line editor is reading a line, so the
    /// prompt's Precmd is already in the pty stream and will be parsed (an event).
    LineEditorReading,
    /// #1614: the shell owns the foreground and its tty is cooked (canonical): its line editor is
    /// not reading yet. Between a command's end and the line editor (the Precmd written,
    /// `PROMPT_COMMAND` / PS1 substitutions running), or a cooked builtin (`read x`). With a line
    /// unanswered and the stream not applied up to now, that Precmd may be in it.
    ShellCooked,
    /// Nothing is known (no probe, ConPTY), or another process group is in front (an inner shell,
    /// `ssh`, `sudo -i`): the caller keeps its timing path where nothing is sampled.
    Unknown,
}

/// What the shell's own line-editor markers say it is doing (#1390, #1391, #1392), applied on the
/// model thread in the order the markers arrive in the pty stream. The platform sample
/// ([`ShellPromptFact`]) is read ahead of the stream; this is the stream's own, ordered, account,
/// so it is never newer than the output the app has parsed. A bootstrap that sends no markers
/// leaves it [`Self::Unknown`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShellActivity {
    /// No marker seen this session (an older bootstrap, or nothing yet).
    #[default]
    Unknown,
    /// A Precmd was parsed and the line editor has not said it is reading yet.
    Prompt,
    /// The line editor said it is reading a primary-prompt line (`LineEditorReading`).
    Reading,
    /// The shell showed a continuation prompt (`ContinuationPrompt`): it consumed an Enter as part
    /// of a line that goes on.
    Continuation,
    /// A Preexec was parsed after the last of those: a command (or a builtin such as `read -k`)
    /// is running. A raw read in this state is not the line editor: it never sends the marker.
    Running,
}

/// The last shell hook the parser met (#1392), whatever the block lifecycle then did with it: a
/// Precmd the lifecycle ignores (a repeated or colliding one) still means the shell went back to
/// its prompt, so it must void a "command running" account that the ignored hook did not reset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParsedShellHook {
    /// A Precmd (with or without completion metadata).
    Precmd,
    /// A CommandFinished.
    CommandFinished,
    /// A Preexec.
    Preexec,
}

/// Asks the platform whether the shell provably cannot be at its prompt: a plain program such as
/// `cat` owns the foreground, or a program is reading a password (canonical tty, echo off), in
/// the top-level shell or a Phosphorized subshell. Unix only; where nothing is installed, or the
/// answer is unknown (over ssh the local tty says nothing about the remote shell), it is `false`.
#[derive(Clone, Default)]
pub struct ForegroundProbe {
    plain_program: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    prompt_fact: Option<Arc<dyn Fn() -> ShellPromptFact + Send + Sync>>,
    /// The reader-side barrier (#1392): whether everything the shell wrote before the call has
    /// been read from the pty and applied to the hook account (see
    /// [`super::pty_stream_barrier::PtyStreamBarrier`]). Asked only AFTER [`Self::prompt_fact`].
    stream_drained: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
}

impl ForegroundProbe {
    pub fn new(probe: Arc<dyn Fn() -> bool + Send + Sync>) -> Self {
        Self {
            plain_program: Some(probe),
            prompt_fact: None,
            stream_drained: None,
        }
    }

    /// Adds the richer sample (#1353 phase 6) that also says the line editor is reading.
    pub fn with_prompt_fact(
        mut self,
        probe: Arc<dyn Fn() -> ShellPromptFact + Send + Sync>,
    ) -> Self {
        self.prompt_fact = Some(probe);
        self
    }

    /// Adds the reader-side barrier (#1392); see [`Self::stream_drained`].
    pub fn with_stream_drained(mut self, probe: Arc<dyn Fn() -> bool + Send + Sync>) -> Self {
        self.stream_drained = Some(probe);
        self
    }

    fn foreground_is_plain_program(&self) -> bool {
        self.plain_program.as_ref().is_some_and(|probe| probe())
    }

    /// Whether the pty stream is provably applied up to now: `Some(true)` when nothing the shell
    /// wrote before this call is unread or unparsed, `Some(false)` when something may be (or the
    /// barrier cannot tell), `None` when no barrier is installed (ConPTY, tests, a shell such as
    /// fish that writes its Precmd after raw mode, #1392).
    fn stream_drained(&self) -> Option<bool> {
        self.stream_drained.as_ref().map(|probe| probe())
    }

    /// Whether the platform samples the shell's state at all ([`Self::prompt_fact`] installed). A
    /// sample of [`ShellPromptFact::Unknown`] from an installed probe is a fact (the shell's own
    /// group is not in front with a raw tty); without a probe it is ignorance (#1614).
    fn samples_shell(&self) -> bool {
        self.prompt_fact.is_some()
    }

    /// The sampled [`ShellPromptFact`]; without the richer probe, the plain-program answer alone.
    fn prompt_fact(&self) -> ShellPromptFact {
        if let Some(probe) = &self.prompt_fact {
            probe()
        } else if self.foreground_is_plain_program() {
            ShellPromptFact::NotAtPrompt
        } else {
            ShellPromptFact::Unknown
        }
    }
}

impl std::fmt::Debug for ForegroundProbe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            if self.plain_program.is_some()
                || self.prompt_fact.is_some()
                || self.stream_drained.is_some()
            {
                "ForegroundProbe(installed)"
            } else {
                "ForegroundProbe(none)"
            },
        )
    }
}

/// Environment variable that turns the prompt-sequence path off when set to `0` (#1353 phase 6):
/// the hand-off then decides the young Enter, the Ctrl-C hand-back, stale line counts and the
/// ConPTY request by the timing rules it used before -- with one exception kept on in both paths
/// (#1614): an Enter typed into a command the block latch says is interactive is delivered to it,
/// never held for a prompt that command's run cannot produce.
pub const PROMPT_SEQ_ENV: &str = "PHOSPHOR_PROMPT_SEQ";

/// Whether the prompt-sequence path is on by default: it is, unless the kill switch says `0`.
pub fn prompt_seq_env_enabled() -> bool {
    std::env::var(PROMPT_SEQ_ENV).map_or(true, |value| value.trim() != "0")
}

/// The give-up for a young Enter held by an event ([`TypeaheadHandoff::young_hold_wait`]): the
/// prompt's Precmd, or the report, normally releases it within milliseconds, so reaching this
/// means the signal is not coming. Logged by the view when it fires.
pub const EVENT_HOLD_GIVE_UP: Duration = Duration::from_secs(2);

/// Tags the ids of a deferred Enter's `HandoffReport` gate; a young hold's ids never carry it.
const DEFERRED_GATE_ID_BIT: u64 = 1 << 62;

/// The shell's last Precmd sequence number the app has parsed (#1353 phase 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PromptSeq {
    /// The shell instance (`WARP_SESSION_ID`) the counter belongs to; a different one is a
    /// different counter, not a regression.
    pub session: Option<u64>,
    pub seq: u64,
    /// Preexec hooks the shell sent between the previous Precmd and this one (the counter also
    /// counts them); `None` when the previous Precmd was another shell's or is unknown. Zero
    /// means this prompt followed a line that ran nothing (an empty Enter).
    pub preexecs_before: Option<u64>,
}

/// On a ConPTY session (#1162): how long the user must have stopped typing before the hand-off
/// asks the shell for a line that holds raw-typed keys. A request (`ESC i`) landing between two
/// keys of a line the user is still typing is written into the middle of that line's raw
/// writes, and ConPTY / MSYS readline lose the line. Well above the 10-25 ms between keys of
/// fast typing, below the stale-count reset (250 ms) that re-runs a waiting decision.
pub const CONPTY_MIDLINE_QUIET: Duration = Duration::from_millis(120);

/// On a ConPTY session (#1162, #1349): raw user keys typed between a prompt's precmd and the
/// shell's line editor becoming ready are held, and released by the line editor's own signal
/// (the bracketed-paste enable `ESC[?2004h` that readline, zle and fish emit as they start
/// reading a line), never by a timer. Git Bash's readline switches the tty mode, enables
/// bracketed paste and draws the prompt right after the prompt hook (measured: a key written
/// ~6 ms before that start-up output vanished below the app).
///
/// This is only the give-up safety net for a shell that was expected to signal and never did
/// (for example readline with `enable-bracketed-paste off`). A timer never writes to the pty
/// (#1584): keys with no Enter, at a prompt whose line holds nothing typed, go back to the editor
/// as shell-routed text; a segment with an Enter (or one that cannot go back in order) stays held
/// for the events, with a toast. A warning is logged either way. It is deliberately long: a signal
/// normally arrives within 10-60 ms, so reaching this means the signal is not coming.
///
/// It measures ONE wait, the precmd's (#1573): from the prompt's precmd (or from the hold's first
/// key, if that came later) to the line editor's signal. The enter window ([`CONPTY_ENTER_HOLD_ENV`])
/// shares the gate but not the clock: while the command its Enter started runs, nothing is
/// overdue, however long it takes (`sleep 3`, a build); the window ends by its events (the
/// command's prompt, the command becoming interactive, a continuation prompt, a signal key). See
/// [`TypeaheadHandoff::prompt_hold_cap_fired`].
///
/// What a give-up switches off (#1573, deciding #1349's "off for the session"): the hold stays off
/// for the rest of the session only when this session's shell has NEVER answered a precmd's wait
/// with a ready signal, i.e. it does not signal at all and every prompt would otherwise wait out
/// the cap (the documented #1349 reason). A shell that has signalled before missed one prompt (a
/// loaded machine, a slow prompt): that give-up is for the one prompt, and the next prompt is held
/// again, so the #1162 raw-write corruption does not come back for the rest of the session.
pub const CONPTY_PROMPT_HOLD_GIVE_UP: Duration = Duration::from_millis(1500);

/// #1573: when the prompt hold's cap fires after an event already ended both of its waits (the
/// release is in flight, e.g. deferred behind a busy queue), it is checked again after this; it
/// never gives up a hold whose release is under way.
const PROMPT_HOLD_RELEASE_IN_FLIGHT_RECHECK: Duration = Duration::from_millis(100);

/// Environment variable that turns the ConPTY prompt hold off when set to `0` (#1162).
pub const CONPTY_PROMPT_HOLD_ENV: &str = "PHOSPHOR_CONPTY_PROMPT_HOLD";

/// Whether the ConPTY prompt hold is on by default: it is, unless the kill switch says `0`.
pub fn conpty_prompt_hold_env_enabled() -> bool {
    std::env::var(CONPTY_PROMPT_HOLD_ENV).map_or(true, |value| value.trim() != "0")
}

/// Environment variable for the ENTER WINDOW of the ConPTY prompt hold (#1490, #1349): `1` turns it
/// on, `0` off; unset, it follows `PHOSPHOR_QUEUE_DESTINATION=table`, which is the
/// default mode (`legacy` and `shadow` leave the window off).
///
/// The prompt hold covers the keys typed between a prompt's precmd and its line editor becoming
/// ready. The enter window extends it BACKWARDS over the stretch before the precmd: from the moment
/// an Enter (a command, or a line the user typed) is written to a shell that was at its line
/// editor until the line editor is ready again. Keys typed in that stretch used to be written raw
/// to ConPTY, which loses or reorders them while the shell switches its tty between cooked and
/// raw mode around the command (#1490: `7echo ...`, `>echo ...`, `bash -c 'tap ...`). They are
/// held and released in typed order by the same signal that releases the prompt hold.
///
/// Typeahead for a program is still delivered: the window ends, releasing the held keys to the
/// running program in order, when the command becomes interactive (the model's latch event, see
/// [`TypeaheadHandoff::release_enter_window_for_program`]). It only opens when the shell was at its
/// line editor (a running program's own Enter never opens it).
pub const CONPTY_ENTER_HOLD_ENV: &str = "PHOSPHOR_CONPTY_ENTER_HOLD";

/// Whether the enter window is on: the environment says so, else the `table` destination mode.
pub fn conpty_enter_hold_enabled() -> bool {
    conpty_enter_hold_from(
        std::env::var(CONPTY_ENTER_HOLD_ENV).ok().as_deref(),
        crate::terminal::input_queue::destination_mode()
            == crate::terminal::input_queue::DestinationMode::Table,
    )
}

/// [`conpty_enter_hold_enabled`] on given inputs: `1` is on, any other value is off, unset follows
/// the `table` mode.
pub fn conpty_enter_hold_from(env: Option<&str>, table_mode: bool) -> bool {
    match env {
        Some(value) => value.trim() == "1",
        None => table_mode,
    }
}

/// Milliseconds on a process-wide monotonic clock, never 0 (0 means "never" in a stamp). Used only
/// for the mid-line guard's stamp that the view writes without taking the model lock.
fn stamp_now_ms() -> u64 {
    static BASE: OnceLock<Instant> = OnceLock::new();
    let base = BASE.get_or_init(Instant::now);
    (Instant::now().saturating_duration_since(*base).as_millis() as u64).saturating_add(1)
}

/// The shared stamp of the last user byte that the view queued for the shell's line while a
/// prompt hold held keys (#1353 phase 5). The view stores to it without the model lock; the
/// ConPTY mid-line guard ([`CONPTY_MIDLINE_QUIET`]) reads it with the hand-off's own stamp.
#[derive(Clone, Debug, Default)]
pub struct QueuedLineBytesStamp(Arc<AtomicU64>);

impl QueuedLineBytesStamp {
    /// Records "the user typed a byte for the shell's line just now".
    pub fn note_now(&self) {
        self.0.store(stamp_now_ms(), Ordering::Relaxed);
    }

    fn recent(&self, within: Duration) -> bool {
        let stamp = self.0.load(Ordering::Relaxed);
        stamp != 0 && stamp_now_ms().saturating_sub(stamp) < within.as_millis() as u64
    }
}

/// The longest a young line's deferred Enter waits for the verdict of its input-type detection
/// (#933). A verdict that has not come by then means the classifier is not going to answer; the
/// Enter then submits on the Enter-time rules without it (#1174), never kept in the editor.
///
/// The same local, generator-free classifier the editor Enter waits [`ENTER_VERDICT_BUDGET`] for
/// (#1158); the young line's parse only starts when the report lands (no debounce head start) and
/// the line is rare, so it gets a few budgets, not the seconds it had before #1158.
pub const YOUNG_DETECTION_CAP: Duration = ENTER_VERDICT_BUDGET.saturating_mul(5);

/// [`YOUNG_DETECTION_CAP`] for a buffer of `text_len` bytes (#1604): a buffer over the sync
/// routing cap is classified by a linear background job, which gets the same size-proportional
/// allowance an editor Enter grants it (`large_text_compute_allowance`).
pub fn young_detection_cap_for(text_len: usize) -> Duration {
    YOUNG_DETECTION_CAP + large_text_compute_allowance(text_len)
}

/// What the view does with the deferred Enter of a young line whose text must be classified
/// before it is submitted (#933). It is never submitted on an inherited or default input type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YoungSubmit {
    /// The verdict for the exact buffer text has landed: run the Enter.
    Submit,
    /// Keep waiting for the verdict.
    Wait,
    /// No classifier can run at all (no session): keep the text in the editor and let the user
    /// press Enter.
    KeepInEditor,
    /// A classifier exists but did not answer within [`YOUNG_DETECTION_CAP`] (#1174): drop the
    /// request and submit on the Enter-time rules, exactly as an Enter whose verdict is late
    /// does. An Enter is never swallowed.
    SubmitWithoutVerdict,
}

/// Decides [`YoungSubmit`]. `can_run` is whether a classification can run at all;
/// `verdict_pending` whether the verdict for the buffer has not landed yet.
pub fn young_submit_decision(
    can_run: bool,
    verdict_pending: bool,
    waited: Duration,
) -> YoungSubmit {
    young_submit_decision_for(can_run, verdict_pending, waited, 0)
}

/// [`young_submit_decision`] for a buffer of `text_len` bytes: the cap is
/// [`young_detection_cap_for`] (#1604).
pub fn young_submit_decision_for(
    can_run: bool,
    verdict_pending: bool,
    waited: Duration,
    text_len: usize,
) -> YoungSubmit {
    if !can_run {
        YoungSubmit::KeepInEditor
    } else if !verdict_pending {
        YoungSubmit::Submit
    } else if waited >= young_detection_cap_for(text_len) {
        YoungSubmit::SubmitWithoutVerdict
    } else {
        YoungSubmit::Wait
    }
}

/// Control bytes that signal or end input (^C, ^Z, ^\\, ^D). They are never held: they drop a
/// held Enter and are written at once.
pub fn is_signal_byte(byte: &u8) -> bool {
    matches!(byte, 0x03 | 0x04 | 0x1a | 0x1c)
}

/// Who owns the user's keystrokes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum HandoffState {
    /// The input editor owns input, and keys are routed by the rules that predate the hand-off.
    #[default]
    EditorOwned,
    /// The pty owns input: every key the user types is written to it.
    PtyOwned {
        /// Whether bytes were written since the last line terminator, i.e. whether the shell's
        /// current line may hold user input.
        wrote_user_bytes: bool,
        /// Line terminators written since this state was entered, including the one that ends
        /// the submitted command.
        enters_written: u32,
        /// Prompts (precmd hooks) seen since this state was entered.
        precmds_seen: u32,
    },
    /// The request was written. The editor owns input and the shell's report is outstanding.
    AwaitingReport,
    /// Enter was pressed while the report was outstanding. Keys typed since are held, in order,
    /// as entries behind a `HandoffReport` gate of the view's queue (the hand-off keeps no bytes,
    /// only the gate's id: [`TypeaheadHandoff::deferred_gate`]), and follow the deferred command
    /// to the pty once it has been written.
    HoldAfterEnter {
        /// Whether the deferred submit has run (after the report arrived).
        released: bool,
    },
}

/// Where a user write goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UserBytesRoute {
    /// Write it as before the hand-off existed.
    Write,
    /// Write it, and report it to `PtyController` as a hand-off user write, which is in flight
    /// until `PtyController` has processed it (see [`TypeaheadHandoff::on_user_write_flushed`]).
    WriteCounted,
    /// It was held; nothing is written now.
    Held,
}

/// What `PtyController` does when the shell's line editor becomes active.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActiveDecision {
    /// The hand-off does not own this prompt: request and drain as before the hand-off existed.
    Legacy,
    /// Drain queued writes without a request.
    Drain,
    /// Drain queued writes (the shell's line holds no user bytes) and, if no precmd or preexec
    /// arrives for a while, reset the counts and decide again: an Enter the counts consider
    /// queued may have been consumed by a program (`read`, a password prompt, a subshell's
    /// `exit`) and will never be answered by a prompt.
    DrainAndWait {
        /// The activity generation a stale-count reset is valid for.
        activity_generation: u64,
    },
    /// The editor took over without a request, because no user byte can be in the shell's line.
    /// Drain queued writes.
    Committed,
    /// Write the request ahead of the queued writes, then drain them. The editor owns input from
    /// now on.
    SendRequest,
    /// A request is not safe yet. Queued writes stay queued (they could kill the user's line);
    /// retry when the blocking condition clears, or reset stale counts after a quiet period.
    Wait {
        /// The activity generation a stale-count reset is valid for.
        activity_generation: u64,
    },
}

/// What the view does with a typeahead report from the shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportDisposition {
    /// Not a hand-off report: insert it as before the hand-off existed.
    InsertLegacy,
    /// Insert it at the front of the editor; the editor now owns input.
    Insert,
    /// As [`Self::Insert`], for a line the user began typing right after a command, before the app
    /// knew the shell was back at its prompt (#933): not typeahead for a running program, so an
    /// Enter on it must wait for a classification of the text.
    InsertYoung,
    /// Insert it, then run the Enter that was deferred while it was outstanding, as a shell
    /// command (shell typeahead).
    InsertThenSubmit,
    /// As [`Self::InsertThenSubmit`], for a line the user began typing right after a command,
    /// before the app knew the shell was back at its prompt (#933): the Enter is routed like any
    /// Enter in the editor.
    InsertThenSubmitYoung,
    /// Insert it, but keep the Enter deferred: the shell's line still holds user bytes this
    /// report did not cover (written behind the request, #1104). A further request is written
    /// behind them, and the Enter runs on the report that answers it.
    InsertKeepDeferred,
    /// Leave it stored and insert it when the editor next takes over, so it is not wiped with a
    /// submitted command's text.
    Stash,
}

/// What a Ctrl-C written while the pty owns input means for the hand-off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterruptOutcome {
    /// The pty did not own input.
    NotOwned,
    /// A command was running; it and any queued typeahead lines die, and the next prompt hands
    /// input back to the editor.
    CommandRunning,
    /// The shell was reading a line, which the interrupt discards. Some shells answer with a
    /// prompt cycle, so the caller waits briefly for one before handing input to the editor.
    AtPrompt,
}

/// Copyable summary of [`HandoffState`], so transitions can be decided without borrowing it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Owner {
    Editor,
    Pty {
        wrote_user_bytes: bool,
        enters_written: u32,
        precmds_seen: u32,
    },
    AwaitingReport,
    Hold {
        released: bool,
    },
}

/// The handle on one pane's [`TypeaheadHandoff`] (#1403). The state lives behind its own small
/// lock instead of inside the terminal model's, so a user key reaches it without waiting for the
/// pty reader, which holds the model's lock while it parses.
///
/// The lock is a leaf: held for one method call, never across I/O, a parse, a view update or any
/// other lock. The one order that exists is model lock first, hand-off second (the model thread
/// updates the hand-off while it parses); code that holds a hand-off guard must not take the
/// model's lock. Reads are recursive, so two shared guards alive in one statement cannot deadlock.
#[derive(Clone, Debug, Default)]
pub struct PaneHandoff(Arc<RwLock<TypeaheadHandoff>>);

impl From<TypeaheadHandoff> for PaneHandoff {
    fn from(handoff: TypeaheadHandoff) -> Self {
        Self(Arc::new(RwLock::new(handoff)))
    }
}

impl PaneHandoff {
    /// Shared access (recursive: nested reads on one thread are safe).
    pub fn read(&self) -> RwLockReadGuard<'_, TypeaheadHandoff> {
        self.0.read_recursive()
    }

    /// Shared access, or `None` while the model's parse holds the write lock. For the route facts
    /// (#1424), which never wait for another thread.
    pub fn try_read(&self) -> Option<RwLockReadGuard<'_, TypeaheadHandoff>> {
        self.0.try_read_recursive()
    }

    /// Exclusive access. Never call while a guard of this handle is alive on the same thread.
    pub fn write(&self) -> RwLockWriteGuard<'_, TypeaheadHandoff> {
        self.0.write()
    }
}

/// The hand-off state of one terminal. See the module documentation.
#[derive(Debug, Default)]
pub struct TypeaheadHandoff {
    state: HandoffState,
    /// User writes routed to the pty that `PtyController` has not processed yet.
    writes_in_flight: u32,
    /// Bumped on every precmd and preexec, so a stale-count reset armed before either is void.
    activity_generation: u64,
    /// The agent action (and its conversation) whose command `PtyController` holds behind the
    /// user's typeahead, if the held command is an agent's. Its block is not started yet, so
    /// this is the only place a cancel of that action, or a stop of that conversation, can find
    /// it.
    held_agent_command: Option<HeldAgentCommand>,
    /// When the editor last took input over from the pty.
    editor_took_over_at: Option<Instant>,
    /// A key applied within that window moved focus to the editor itself; the resulting focus
    /// event must not end the window.
    focus_moved_by_takeover_key: bool,
    /// Prompts the model has seen whose precmd event the terminal view has not handled yet.
    /// The view's handling of a prompt reinitializes the input editor (the finished block's
    /// `BlockCompleted`), wiping what the editor holds. Giving the editor input before that has
    /// run lets it take keys that the reinitialization then erases (#934), so no request is
    /// written, and the editor does not take over, until this is zero.
    precmds_pending_view: u32,
    /// Whether a terminal view is attached and handles each prompt's precmd event, so
    /// `precmds_pending_view` is worth counting. Without one (a controller driven on its own)
    /// nothing would ever clear it.
    tracks_view_precmds: bool,
    /// The last command written, prompt or preexec seen (see [`YOUNG_ENTER_WINDOW`]).
    last_activity_at: Option<Instant>,
    /// When the editor's last command was written, until its preexec arrives: that round trip
    /// (write, shell preexec hook, pty read, parse) is sampled like a request-to-report one
    /// (#1131), so the young hold is measured from the session's first command on, before any
    /// typeahead exists, instead of starting at `YOUNG_ENTER_MIN_HOLD`.
    command_written_at: Option<Instant>,
    /// Whether the pty's foreground process group is provably not a shell at a prompt. A line
    /// typed into such a program cannot be a #933 line typed at an unnoticed prompt.
    foreground_probe: ForegroundProbe,
    /// When each outstanding hand-off request was written, oldest first (reports come back in
    /// order, so the front is the one a report answers).
    rtt_pending: VecDeque<Instant>,
    /// The last [`RTT_WINDOW`] request-to-report round trips of this session.
    rtt_samples: VecDeque<Duration>,
    /// A young hold was just created, so the view must put the key that started it (the user's
    /// Enter, typed while the pty owns input and the command that was written may already have
    /// finished, #933) behind a `PromptSeen` gate keyed by the hold's id.
    young_hold_started: bool,
    /// Identifies the current hold; a give-up armed for an earlier one must not release it.
    young_hold_id: u64,
    /// A young hold exists and its keys are in the view's queue, undecided. Every way it can end
    /// records a [`YoungHoldEnd`] for the view to turn into taking or dropping the gate's
    /// entries ([`Self::take_young_hold_ends`]); the hold's give-up cap is the only timer.
    young_hold_queued: bool,
    /// Ends the view has not applied to the queue yet, oldest first.
    young_hold_ends: Vec<(u64, YoungHoldEnd)>,
    /// The id of a queued hold whose keys follow the command just written, between that decision
    /// and the view's [`Self::count_followed_young_hold`]. No request is decided meanwhile.
    young_hold_follow_pending: Option<u64>,
    /// The id of the `HandoffReport` gate that holds the keys typed behind a deferred Enter, while
    /// the state is [`HandoffState::HoldAfterEnter`]. The ids carry [`DEFERRED_GATE_ID_BIT`], so
    /// one number never names a young hold's gate as well.
    deferred_gate: Option<u64>,
    /// Counts the deferred gates handed out (the low bits of their ids); survives a reset.
    deferred_gate_seq: u64,
    /// A key reached the hand-off in `HoldAfterEnter` without having been queued behind the gate
    /// (the queue was idle or mid-delivery): the view puts it behind the gate
    /// ([`Self::take_deferred_key_started`]).
    deferred_key_started: bool,
    /// What the shell's markers say it is doing now (see [`ShellActivity`]).
    shell_activity: ShellActivity,
    /// The prompt sequence number of the last `LineEditorReading` applied, so a second marker for
    /// the same prompt changes nothing.
    reading_seq: Option<u64>,
    /// A line-editor marker has been seen this session: the bootstrap sends them, so the
    /// events (not the logged give-ups, which stay as the net) account for the shell's line
    /// editor.
    line_editor_markers_live: bool,
    /// The last shell hook parsed this session, applied or not (see [`ParsedShellHook`]).
    last_parsed_hook: Option<ParsedShellHook>,
    /// Whether this session's input goes through ConPTY (Windows). There a held run of keys is
    /// written back one chunk per key event, never as one write with an embedded Enter, and no
    /// request is written while the user is still typing a line (see [`CONPTY_MIDLINE_QUIET`]).
    conpty_input: bool,
    /// Whether the ConPTY prompt hold is on (see [`CONPTY_PROMPT_HOLD_GIVE_UP`]); it also needs
    /// `conpty_input`, so no other platform is ever affected.
    prompt_hold_enabled: bool,
    /// When the last precmd arrived, until the shell's line editor is active (or a command
    /// starts): keys typed in between are held.
    awaiting_line_editor_since: Option<Instant>,
    /// Whether the enter window is on (see [`CONPTY_ENTER_HOLD_ENV`]); it also needs the prompt
    /// hold to be armed, so no other platform is ever affected.
    enter_window_enabled: bool,
    /// When an Enter was last written to a shell that was at its line editor, until the shell's
    /// next precmd (which starts the wait above) or the command becoming interactive: keys typed
    /// in between are held (#1490).
    enter_window_since: Option<Instant>,
    /// F7 (#1595): the session's shell does not treat `^U` as a line kill (PowerShell's PSReadLine
    /// does not bind it by default), so a raw `^U` never makes the line clean in the hand-off's
    /// accounting. Default false: `^U` kills, as in a tty, readline, zle and fish.
    ctrl_u_not_line_kill: bool,
    /// Whether the session's shell is known to signal its line editor being ready (#1349): a
    /// readline, zle or fish prompt enables bracketed paste as it starts reading. A shell that
    /// is not known to (PowerShell 5.1, cmd, an unknown WSL shell) is never held for: with no
    /// signal to release it, the hold could only be a timer.
    prompt_hold_shell_signals: bool,
    /// A hold ran to the give-up cap with no signal and this session's shell never signalled: the
    /// shell does not signal after all, so the hold stays off for the rest of the session (#1349,
    /// narrowed by #1573: see [`CONPTY_PROMPT_HOLD_GIVE_UP`]).
    prompt_hold_gave_up: bool,
    /// A ready signal answered a precmd's wait in this session (#1573): the shell does signal, so a
    /// give-up is one missed prompt, not a reason to stop holding.
    line_editor_signal_seen: bool,
    /// When the live prompt hold's first key was held (#1573): its give-up cap was armed then.
    prompt_hold_started_at: Option<Instant>,
    /// When the shell last started a command (its preexec), to tell an enter window whose command
    /// is running from one whose Enter the shell never acted on (#1573).
    last_preexec_at: Option<Instant>,
    /// #1574: bytes the view delivered to the shell as a final delivery (the #1494 `stty sane`
    /// path) may be in the shell's line. They belong to that shell: no report is asked for while
    /// this is set (a report clears the shell's line, so it would move them into the editor, or
    /// lose them if the editor refused them). Cleared by anything that shows the shell's line no
    /// longer holds them (a commit to the editor, a command or a user Enter written).
    committed_delivery: bool,
    /// A prompt hold was just created, so the view must put the key that started it behind a
    /// `LineEditorActive` gate keyed by the hold's id.
    prompt_hold_started: bool,
    /// Identifies the current prompt hold; a give-up armed for an earlier one must not release it.
    prompt_hold_id: u64,
    /// A prompt hold exists and its keys are in the view's queue.
    /// Cleared by the view when it has released them ([`Self::finish_queued_prompt_hold`]) or by
    /// a signal that takes them ([`Self::take_queued_prompt_hold_for_signal`]).
    prompt_hold_queued: bool,
    /// Typing into a queued prompt hold, stamped by the view without the model lock.
    queued_line_bytes: QueuedLineBytesStamp,
    /// When the last precmd arrived, whether or not a hold follows (for the release hop).
    last_precmd_at: Option<Instant>,
    /// When the user last typed a byte that reached the shell's line.
    last_line_bytes_at: Option<Instant>,
    /// Whether the line the user is typing began within [`YOUNG_ENTER_WINDOW`] of the last
    /// activity, i.e. before the app could know the shell was at its prompt (#933).
    young_line: bool,
    /// #1614: the block latch (`block::interactive_latch`) said the running command is interactive:
    /// a program other than the shell owns the foreground, the tty is raw or reading a password,
    /// the shell itself is blocked in a builtin such as `read x`, or (ConPTY, where nothing can be
    /// asked) the command outlived the presentation debounce. A positive event that the keys typed
    /// now are that command's, not a line at a prompt: an Enter is delivered to it, and a young
    /// hold that was live when the event arrived is written as typeahead. Cleared by the command's
    /// Precmd and by the next command written. This is what replaced the #1609 `ssh` carve-out: an
    /// interactive `ssh`, `sudo -i`, inner `bash` or `vim` all latch, on every platform.
    command_interactive: bool,
    /// #1614: the command the editor wrote has not had its own Preexec parsed yet (set by the
    /// write, taken by the next Preexec, cleared by a Precmd). While it is pending, a Preexec is
    /// the written command starting, not a command the shell started after a hold opened; and a
    /// line editor sampled reading with the stream applied may still be the one that has not read
    /// the written command (the input direction is not the barrier's), so the counts are not
    /// proven stale by it.
    own_preexec_pending: bool,
    /// User bytes were written to the pty while the editor owned input and nothing adopted them
    /// (an agent-requested command, a block from before the bootstrap precmd), so the shell's line
    /// may hold text the next legacy report puts in front of the editor's (#1089).
    untracked_line_bytes: bool,
    /// Requests written whose reports have not arrived yet. Reports come back in the order the
    /// requests were written (the tty is FIFO).
    requests_outstanding: u32,
    /// How many more reports must arrive before one answers a request written after the
    /// untracked bytes; `None` while no such request was written.
    untracked_cover_after_reports: Option<u32>,
    /// Whether the Enter deferred behind the report ends such a line.
    deferred_young: bool,
    /// Identifies a deferred Enter that waits for a report covering untracked bytes (#1104). If
    /// that report never comes (a program ate the request), the view gives the Enter up when its
    /// timer for this id fires.
    uncovered_deferral_id: Option<u64>,
    /// A deferral was just armed, so the view must start its timer.
    uncovered_deferral_started: bool,
    uncovered_deferral_seq: u64,
    /// Identifies an Enter deferred behind the hand-off's own outstanding report (#1215). If
    /// that report never comes, the view gives the Enter up when its timer for this id fires,
    /// without running it.
    report_deferral_id: Option<u64>,
    /// A report deferral was just armed, so the view must start its timer.
    report_deferral_started: bool,
    report_deferral_seq: u64,
    /// #1353 phase 6: whether the prompt-sequence path may be used at all (the kill switch,
    /// `PHOSPHOR_PROMPT_SEQ`). It only takes effect once the shell has sent a sequence number.
    prompt_seq_enabled: bool,
    /// The last Precmd sequence number parsed, `None` until the shell sends one (an older
    /// bootstrap never does, and then every decision below keeps its timing path).
    prompt_seq: Option<PromptSeq>,
    /// A Ctrl-C reached a shell at its prompt that runs no prompt cycle after it: the hand-back to
    /// the editor waits for the shell's answer to a request written behind the interrupt, not for
    /// a timer.
    interrupt_barrier: bool,
    /// A Ctrl-C was written and the Precmd of the prompt cycle that follows it will hand input
    /// back; cleared by that Precmd. If it never comes, the view's logged give-up commits.
    interrupt_awaiting_cycle: bool,
    /// The command that just finished was interactive (a program was reading the keys typed into
    /// it), so every Enter typed meanwhile was consumed by it and no prompt will answer them.
    interactive_command_finished: bool,
    /// The evidence the live young hold stands on (#1614), `None` while no hold is live. It names
    /// the event that releases the hold and decides the cap ([`Self::young_hold_wait`]).
    young_hold_evidence: Option<YoungHoldEvidence>,
    /// #1615 measurement, no decision: the last Enter delivered to the pty while an unsampled
    /// process group (a warpified ssh's remote shell, an inner shell) was in front and a line was
    /// unanswered, followed through the hooks that come after it (see
    /// [`UnsampledEnterDelivery`]).
    unsampled_enter_delivery: Option<UnsampledEnterDelivery>,
}

/// #1615: an Enter the hand-off delivered to the pty although a line was unanswered, because the
/// shell in front is not the one the platform samples (`Unknown` from an installed probe: a
/// warpified ssh's remote shell, an inner shell). The remote #933 race -- a line typed and Entered
/// at a remote prompt whose Precmd is still on its way over the network -- looks exactly like an
/// Enter typed into the remote command that is still running, so it is delivered as plain ssh
/// would (owner decision 2026-11-03). Only logged: the Precmd that follows and the Preexec after it
/// say, by their order and timing, whether the delivered line then ran as a shell command at the
/// next prompt (the race, unclassified) or was the running command's.
#[derive(Clone, Copy, Debug)]
struct UnsampledEnterDelivery {
    /// When the Enter was delivered.
    at: Instant,
    /// When the first Precmd after it was parsed.
    precmd_after: Option<Instant>,
}

/// Why a young Enter is held (#1614): the positive evidence the hold stands on, each kind with the
/// event that releases it. A hold never engages without one. The give-up cap logs it, so a run in
/// which the cap fired names the evidence whose event did not come.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YoungHoldEvidence {
    /// (A) The request is due: the counts say the shell's prompt is back with no line queued (or
    /// the sample -- the shell's line editor reading with every Precmd applied -- proved them
    /// stale, and they were corrected on the spot), so the shell is at its line editor or about
    /// to be. The view asks the controller for the request decision at once; the request is
    /// written behind the typed bytes and the shell's report runs the Enter in the editor
    /// ([`YoungHoldEnd::SubmitOnReport`]). If the decision finds no request possible the hold is
    /// written as typeahead instead ([`YoungHoldEnd::Typeahead`]).
    RequestDue,
    /// (B) A Precmd may be in flight: the sample says the shell's line editor reads, the counts say
    /// a line is unanswered, and the reader-side barrier says the stream is not applied up to now
    /// (#933 proper, or just the echo of the typed keys). The barrier's next publish decides again
    /// ([`TypeaheadHandoff::on_stream_applied`]): into [`Self::RequestDue`] or a delivery.
    PrecmdInFlight,
    /// (B without a barrier) The sample says the shell's line editor reads and the counts say a
    /// line is unanswered, but no barrier is installed for this shell (fish writes its Precmd
    /// after raw mode, #1392), so whether that Precmd is still in the stream cannot be told. Held
    /// as before #1614 (owner decision 2026-11-03): released by the Precmd, then the request and
    /// the report; the 2 s hand-back cap is the net for counts that turn out stale.
    PrecmdMaybeInFlight,
    /// The platform can say nothing about the shell (ConPTY, a session with no sample): the
    /// elapsed-time window of [`YOUNG_ENTER_WINDOW`], as before #1614. The one remaining timing
    /// hold, kept as the documented special case; its release is the prompt's Precmd (then the
    /// request) within the window, or a Preexec.
    TimingWindow,
}

/// What [`TypeaheadHandoff::on_stream_applied`] decided for a hold waiting on the barrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamApplied {
    /// No hold waits on the barrier, or the stream is still not applied up to now.
    Nothing,
    /// The hold now stands on [`YoungHoldEvidence::RequestDue`]: the controller decides the request.
    RetryRequest,
    /// The hold ended as typeahead ([`YoungHoldEnd::Typeahead`] recorded): the view writes the keys.
    HoldEnded,
}

/// How a young hold whose keys are in the view's queue ended (#1353 phase 5b). The hand-off
/// decides, the view applies it to the gate's entries; no timer is involved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YoungHoldEnd {
    /// The held Enter and the keys behind it are void (a signal, a command that took the line, a
    /// state that ended the hold): the view discards the gate's entries.
    Dropped,
    /// The editor's command was written with the pty owning input: the held keys follow it as
    /// typeahead. The view writes them and reports them back
    /// ([`TypeaheadHandoff::count_followed_young_hold`]).
    FollowCommand,
    /// The shell's report arrived: the first held byte (the Enter) is run in the editor and the
    /// rest becomes the keys held behind it, under the `HandoffReport` gate `deferred_gate`
    /// (the view turns the hold's gate into that one in place).
    SubmitOnReport { deferred_gate: u64 },
    /// An event showed the held keys are typeahead for what owns the tty (#1614): a Preexec (the
    /// shell started a command), the block latch (a program reads the tty), the barrier (the
    /// stream is applied and nothing says the prompt is back), or a request decision that found
    /// no request possible. The view writes them to the pty in typed order and the hand-off counts
    /// them as the user bytes they are ([`TypeaheadHandoff::route_expired_young_chunks`]). An
    /// event-driven write, never a timer's.
    Typeahead,
}

/// What a queued young hold's give-up cap should do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YoungHoldExpiry {
    /// No prompt came: the keys are typeahead for the running command; the view writes them.
    Release,
    /// The request already went out: the report will bring the line back and run the Enter.
    Keep,
    /// The hold is not live (it ended; the view applies the recorded end).
    Gone,
}

/// What the give-up cap of a queued prompt hold decides when it fires (#1573); see
/// [`TypeaheadHandoff::prompt_hold_cap_fired`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptHoldCap {
    /// The hold is not live (its event released it, or the timer belongs to an earlier hold).
    Gone,
    /// Not overdue: the wait the cap measures has not run its course (the command the enter
    /// window's Enter started is still running, or the precmd that followed it came less than the
    /// cap ago). The view arms the cap again for this long; nothing is written.
    NotYet(Duration),
    /// The wait ran out with no event: the view releases the held keys, in typed order, through the
    /// counted release ([`TypeaheadHandoff::expire_queued_prompt_hold`]).
    GiveUp,
}

/// Identifies an agent's command held behind the user's typeahead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeldAgentCommand {
    pub action_id: AIAgentActionId,
    pub conversation_id: AIConversationId,
}

impl TypeaheadHandoff {
    pub fn state(&self) -> &HandoffState {
        &self.state
    }

    /// Marks the session's input as going through ConPTY (#1162).
    pub fn set_conpty_input(&mut self, conpty_input: bool) {
        self.conpty_input = conpty_input;
    }

    /// Declares whether `^U` kills the line in this session's shell (F7, #1595). `false` for
    /// PowerShell, whose PSReadLine does not bind it by default.
    pub fn set_ctrl_u_kills_line(&mut self, kills: bool) {
        self.ctrl_u_not_line_kill = !kills;
    }

    /// Turns the ConPTY prompt hold on or off (#1162). Only has an effect on a ConPTY session.
    pub fn set_conpty_prompt_hold(&mut self, enabled: bool) {
        self.prompt_hold_enabled = enabled;
    }

    /// Turns the enter window of the prompt hold on or off (#1490). Only has an effect on a
    /// ConPTY session whose prompt hold is armed.
    pub fn set_conpty_enter_window_hold(&mut self, enabled: bool) {
        self.enter_window_enabled = enabled;
    }

    /// Declares whether the session's shell signals its line editor being ready with a
    /// bracketed-paste enable (#1349). Without that capability no hold is ever started.
    pub fn set_conpty_prompt_hold_shell_signals(&mut self, signals: bool) {
        self.prompt_hold_shell_signals = signals;
    }

    /// Whether a young hold is live and its keys are in the view's queue.
    pub fn young_hold_queued(&self) -> bool {
        self.young_hold_queued
    }

    /// Ends recorded for the view to apply to the queue's gate, oldest first.
    pub fn take_young_hold_ends(&mut self) -> Vec<(u64, YoungHoldEnd)> {
        mem::take(&mut self.young_hold_ends)
    }

    /// Whether an end waits for the view.
    pub fn young_hold_end_pending(&self) -> bool {
        !self.young_hold_ends.is_empty()
    }

    /// The view could not apply `end` (its queue was busy): keep it for the next attempt, ahead of
    /// anything recorded since.
    pub fn restore_young_hold_end(&mut self, id: u64, end: YoungHoldEnd) {
        self.young_hold_ends.insert(0, (id, end));
    }

    /// Ends the young hold with the keys void. Returns whether there was one. The hold's keys
    /// are in the view's queue, which is told with [`YoungHoldEnd::Dropped`].
    fn drop_young_hold(&mut self) -> bool {
        let queued = mem::take(&mut self.young_hold_queued);
        if queued {
            self.young_hold_ends
                .push((self.young_hold_id, YoungHoldEnd::Dropped));
        }
        queued
    }

    /// The view is about to queue a key behind a live young hold: ends a hold the state has
    /// outgrown first (what the next routed key would have found), so the key is not swallowed by
    /// a stale gate. Returns whether an end now waits for the view.
    pub fn check_queued_young_hold(&mut self) -> bool {
        self.drop_stale_hold();
        self.young_hold_end_pending()
    }

    /// The stamp the view writes, without the model lock, for each key it queues behind a prompt
    /// hold, so the ConPTY mid-line guard sees keys that never came through here.
    pub fn queued_line_bytes_stamp(&self) -> QueuedLineBytesStamp {
        self.queued_line_bytes.clone()
    }

    /// Whether a queued prompt hold is live (its keys are in the view's queue).
    pub fn prompt_hold_queued(&self) -> bool {
        self.prompt_hold_queued
    }

    /// Whether a precmd arms the prompt hold: ConPTY, the kill switch is not set, the shell is
    /// known to signal, and no earlier hold of this session gave up waiting for a signal.
    fn prompt_hold_armed(&self) -> bool {
        self.conpty_input
            && self.prompt_hold_enabled
            && self.prompt_hold_shell_signals
            && !self.prompt_hold_gave_up
    }

    /// Whether the session is waiting for the line editor's signal after a precmd, and keys
    /// typed now are held. The wait ends at the signal, a command start, a ^C, or the give-up
    /// cap; elapsed time alone never releases it (the cap check here only covers the instant
    /// before the view's give-up timer runs).
    fn should_hold_for_prompt(&self, reaches_shell_line: bool, now: Instant) -> bool {
        self.prompt_hold_armed()
            && reaches_shell_line
            && matches!(self.state, HandoffState::PtyOwned { .. })
            && (self
                .awaiting_line_editor_since
                .is_some_and(|at| now.saturating_duration_since(at) < CONPTY_PROMPT_HOLD_GIVE_UP)
                || self.enter_window_open(now))
    }

    /// Whether the enter window is open (#1490): an Enter went to a shell that was at its line
    /// editor, and neither the command's prompt nor the command becoming interactive ended it.
    fn enter_window_open(&self, _now: Instant) -> bool {
        // #1573: no clock here. A command that runs longer than the give-up cap is not overdue;
        // keys typed into it are still held for the shell's next line editor, and the window ends
        // by its events only (precmd, interactive latch, continuation prompt, a signal key).
        self.enter_window_enabled && self.enter_window_since.is_some()
    }

    /// Whether an Enter written now submits a line at the shell's line editor, so the enter window
    /// opens: the window is on, the prompt hold is armed, no ready signal is awaited (a precmd's
    /// wait holds the keys itself), and the shell was last seen at a prompt or reading a line.
    fn enter_window_may_open(&self) -> bool {
        self.enter_window_enabled
            && self.prompt_hold_armed()
            && self.awaiting_line_editor_since.is_none()
            && matches!(
                self.shell_activity,
                ShellActivity::Prompt | ShellActivity::Reading
            )
    }

    /// The give-up cap the view arms a timer for when a hold starts.
    pub fn prompt_hold_give_up(&self) -> Duration {
        CONPTY_PROMPT_HOLD_GIVE_UP
    }

    /// Whether the line editor's ready signal is still awaited for the current prompt.
    #[cfg(test)]
    pub fn awaiting_line_editor_signal(&self) -> bool {
        self.awaiting_line_editor_since.is_some()
    }

    /// Takes the "a prompt hold was just created" flag, returning its id so the view queues the
    /// key that started it behind the hold's gate once.
    pub fn take_prompt_hold_started(&mut self) -> Option<u64> {
        mem::take(&mut self.prompt_hold_started).then_some(self.prompt_hold_id)
    }

    #[cfg(test)]
    pub(crate) fn has_prompt_hold(&self) -> bool {
        self.prompt_hold_queued
    }

    /// Routes one chunk that was held (for the prompt, #1162) as the write it was typed as,
    /// bypassing every hold, and returns whether the caller must write it. A chunk the current
    /// state absorbs (`HoldAfterEnter`) is not written: the caller puts it behind the deferred
    /// Enter's gate ([`Self::take_deferred_key_started`]). The view's queue releases a queued prompt
    /// hold chunk by chunk through this (#1353 phase 5).
    pub fn route_released_chunk(&mut self, chunk: &[u8]) -> bool {
        match self.route_user_bytes_unheld(chunk, true) {
            UserBytesRoute::WriteCounted => true,
            UserBytesRoute::Write => {
                // Not counted by the route, but the writer retires every chunk it writes.
                self.writes_in_flight = self.writes_in_flight.saturating_add(1);
                true
            }
            UserBytesRoute::Held => false,
        }
    }

    /// The line editor's ready signal arrived (#1349), observed in the pty output after the
    /// prompt's precmd, and the keys of the prompt hold are in the view's queue (#1353 phase 5):
    /// ends the wait and returns the hold's id when one is live, so the view releases that gate.
    /// The hold stays live ([`Self::prompt_hold_queued`]) until the view has accounted the
    /// released keys ([`Self::finish_queued_prompt_hold`]), and no request is decided meanwhile
    /// (see `on_line_editor_active`). A signal with no wait pending (a program enabling bracketed
    /// paste itself, or a second signal for the same prompt) releases nothing.
    pub fn release_queued_prompt_hold_on_signal(&mut self) -> Option<u64> {
        warpui::input_order::note_shell_signal(warpui::input_order::ShellSignal::LineEditorReady);
        if self.awaiting_line_editor_since.take().is_none() {
            return None;
        }
        // #1573: this shell signals; a later give-up is one missed prompt. #1584: a hold kept past
        // its cap may have switched the hold off for a shell thought not to signal; it does.
        self.line_editor_signal_seen = true;
        self.prompt_hold_gave_up = false;
        let waited_ms = self
            .last_precmd_at
            .map(|at| at.elapsed().as_secs_f64() * 1000.0);
        warpui::input_order::log_hop("prompt_hold_signal", || {
            format!(
                "since_precmd_ms={} queued={}",
                waited_ms.map_or_else(|| "none".to_string(), |ms| format!("{ms:.1}")),
                self.prompt_hold_queued
            )
        });
        self.prompt_hold_queued.then_some(self.prompt_hold_id)
    }

    /// The command the Enter submitted became interactive (the model's latch event: a full-screen
    /// program, a raw-mode reader, or the no-probe fallback): the keys typed since are typeahead
    /// for that program, not for the shell's line editor, so the enter window ends and the view
    /// releases them in order (#1490). Returns the hold's id when keys are held. Nothing happens
    /// when no window is open (the prompt's precmd already took over, or the window never opened).
    pub fn release_enter_window_for_program(&mut self) -> Option<u64> {
        self.enter_window_since.take()?;
        warpui::input_order::log_hop("enter_window_program", || {
            format!("queued={}", self.prompt_hold_queued)
        });
        self.prompt_hold_queued.then_some(self.prompt_hold_id)
    }

    /// The continuation prompt (PS2) of the line the enter window's Enter submitted was shown
    /// (#1573): the shell consumed that Enter and its line editor reads the next line, so the
    /// window ends by this event and the view releases the held keys in typed order (the
    /// controller's `PromptHoldReleased`). Returns the hold's id when keys are held; nothing
    /// happens when no window is open. (A window never opens at a continuation prompt, see
    /// `enter_window_may_open`, so this marker is the one the window's own Enter caused.)
    pub fn release_enter_window_for_continuation(&mut self) -> Option<u64> {
        self.enter_window_since.take()?;
        warpui::input_order::log_hop("enter_window_continuation", || {
            format!("queued={}", self.prompt_hold_queued)
        });
        self.prompt_hold_queued.then_some(self.prompt_hold_id)
    }

    /// The give-up cap of queued prompt hold `id` fired (#1573): whether it is a give-up at all.
    /// The cap is armed once, when the hold's first key is held, and the gate is shared by two
    /// waits with different meanings:
    ///
    /// * the precmd's wait for the line editor's signal: overdue once the cap has passed since the
    ///   precmd, or since the hold's first key if that came later (the timer was armed then);
    /// * the enter window: while the command its Enter started runs (its preexec was seen),
    ///   nothing is overdue -- the window ends by its events, and the precmd that follows starts
    ///   the wait above with a full cap. An Enter the shell answered with neither a preexec nor a
    ///   precmd within the cap (a continuation prompt with no marker, a lost Enter) is a give-up
    ///   for this window's keys;
    /// * neither wait pending while the hold is still queued: an event ended it and its release is
    ///   in flight. Not a give-up: checked again shortly.
    ///
    /// `NotYet` asks the view to arm the cap again; it writes nothing and runs nothing. `now` is
    /// given, never read.
    pub fn prompt_hold_cap_fired(&self, id: u64, now: Instant) -> PromptHoldCap {
        if id != self.prompt_hold_id || !self.prompt_hold_queued {
            return PromptHoldCap::Gone;
        }
        if let Some(precmd_at) = self.awaiting_line_editor_since {
            let started_at = self.prompt_hold_started_at.unwrap_or(precmd_at);
            if started_at >= precmd_at {
                // The hold began in this wait: its cap was armed with the full give-up then.
                return PromptHoldCap::GiveUp;
            }
            // The hold began in the enter window and the precmd came later: the wait gets its own
            // full cap from the precmd.
            let waited = now.saturating_duration_since(precmd_at);
            return match CONPTY_PROMPT_HOLD_GIVE_UP.checked_sub(waited) {
                Some(left) if !left.is_zero() => PromptHoldCap::NotYet(left),
                _ => PromptHoldCap::GiveUp,
            };
        }
        let Some(opened_at) = self.enter_window_since else {
            // Neither wait is pending but the hold is still queued: an event (the signal, the
            // interactive latch, a continuation prompt) ended it and its release is in flight
            // (possibly deferred behind a busy queue). Not a give-up: check again shortly. Bounded:
            // a hold whose waits ended with no release (a preexec that cleared the wait) is not
            // left holding keys forever; past twice the cap from its first key it is released.
            let stale = self.prompt_hold_started_at.is_some_and(|started| {
                now.saturating_duration_since(started) >= CONPTY_PROMPT_HOLD_GIVE_UP * 2
            });
            return if stale {
                PromptHoldCap::GiveUp
            } else {
                PromptHoldCap::NotYet(PROMPT_HOLD_RELEASE_IN_FLIGHT_RECHECK)
            };
        };
        if self.last_preexec_at.is_some_and(|at| at >= opened_at) {
            // The command is running: not overdue. The window's own events end it.
            return PromptHoldCap::NotYet(CONPTY_PROMPT_HOLD_GIVE_UP);
        }
        PromptHoldCap::GiveUp
    }

    /// The give-up of queued prompt hold `id` (#1349): [`Self::prompt_hold_cap_fired`] said
    /// [`PromptHoldCap::GiveUp`] and [`Self::prompt_hold_can_hand_back`] holds, so the view hands
    /// its keys back to the editor as shell-routed text (#1584: a timer never writes them to the
    /// pty). Returns whether that hold is live; `false` when it is gone already.
    ///
    /// #1573: the hold is switched off for the rest of the session only when the precmd's wait
    /// gave up and this session's shell has never signalled (it does not signal at all). Any other
    /// give-up is for this hold only: the next prompt is held again.
    pub fn expire_queued_prompt_hold(&mut self, id: u64) -> bool {
        if id != self.prompt_hold_id || !self.prompt_hold_queued {
            return false;
        }
        let precmd_wait = self.awaiting_line_editor_since.take().is_some();
        let enter_window = self.enter_window_since.take().is_some();
        let session_off = precmd_wait && !self.line_editor_signal_seen;
        if session_off {
            self.prompt_hold_gave_up = true;
            log::warn!(
                "ConPTY prompt hold: no line-editor ready signal within {} ms after the prompt, \
                 and this shell has never sent one; the held keys go back to the editor and the \
                 hold is off for this session",
                CONPTY_PROMPT_HOLD_GIVE_UP.as_millis()
            );
        } else if precmd_wait {
            log::warn!(
                "ConPTY prompt hold: no line-editor ready signal within {} ms after this prompt; \
                 the held keys go back to the editor (the hold stays on for the next prompt)",
                CONPTY_PROMPT_HOLD_GIVE_UP.as_millis()
            );
        } else if enter_window {
            log::warn!(
                "ConPTY enter window: the shell neither started a command nor prompted within {} \
                 ms of the Enter; releasing the held keys (the hold stays on)",
                CONPTY_PROMPT_HOLD_GIVE_UP.as_millis()
            );
        } else {
            // An event already ended both waits and no release followed within twice the cap (or a
            // deferred release replayed as a give-up). Not a missed line-editor signal.
            log::warn!(
                "ConPTY prompt hold: no wait pending but the keys were still held; releasing them"
            );
        }
        warpui::input_order::log_hop("prompt_hold_expired", || {
            format!(
                "precmd_wait={precmd_wait} enter_window={enter_window} session_off={session_off}"
            )
        });
        true
    }

    /// #1584: whether the give-up of queued prompt hold `id` may hand its keys back to the editor
    /// and keep them in order: the precmd's wait (the shell is at a prompt) and nothing the user
    /// typed is in the shell's line, so the editor takes input over at once
    /// ([`Self::try_commit_at_prompt`] succeeds once the hold is finished). Anywhere else (an enter
    /// window whose Enter the shell never acted on, a line that already holds typed keys) the
    /// shell's state is unknown and handing back would split the line, so the keys stay held for
    /// the events.
    pub fn prompt_hold_can_hand_back(&self, id: u64) -> bool {
        id == self.prompt_hold_id
            && self.prompt_hold_queued
            && self.awaiting_line_editor_since.is_some()
            && matches!(
                self.owner(),
                Owner::Pty {
                    wrote_user_bytes: false,
                    enters_written,
                    precmds_seen,
                } if precmds_seen >= enters_written
            )
    }

    /// #1584: whether queued prompt hold `id` waits in a precmd's wait of a shell that has never
    /// answered one with a ready signal: the signal is not coming, so a hold kept for it would
    /// be kept forever.
    pub fn prompt_hold_shell_never_signalled(&self, id: u64) -> bool {
        id == self.prompt_hold_id
            && self.prompt_hold_queued
            && self.awaiting_line_editor_since.is_some()
            && !self.line_editor_signal_seen
    }

    /// #1584: the give-up cap of queued prompt hold `id` ran out but its keys stay held (they hold
    /// an Enter, or cannot be handed back in order): no timer writes them. The waits are kept, so
    /// the events still release them. A shell that has never signalled is not waited for at the
    /// next prompts (#1349's session-wide off): keys typed after this hold is released go by the
    /// live path -- unless the signal that releases it shows the shell does signal after all.
    /// Returns whether the hold is live.
    pub fn keep_overdue_prompt_hold(&mut self, id: u64) -> bool {
        if id != self.prompt_hold_id || !self.prompt_hold_queued {
            return false;
        }
        if self.awaiting_line_editor_since.is_some()
            && !self.line_editor_signal_seen
            && !self.prompt_hold_gave_up
        {
            self.prompt_hold_gave_up = true;
            log::warn!(
                "ConPTY prompt hold: this shell has never sent a line-editor ready signal; later \
                 prompts are not held (the keys held now wait for an event)"
            );
        }
        true
    }

    /// Whether the prompt hold is off for the rest of the session after a give-up, for tests.
    #[cfg(test)]
    pub fn prompt_hold_off_for_session(&self) -> bool {
        self.prompt_hold_gave_up
    }

    /// A signal or EOF byte is about to be written while the view's queue holds keys for the
    /// prompt: ends the wait and hands the hold over (returns its id) so the view prepends the
    /// held keys to `bytes`, one write in typed order. The hold is over once this returns.
    pub fn take_queued_prompt_hold_for_signal(&mut self, bytes: &[u8]) -> Option<u64> {
        if !bytes.iter().any(is_signal_byte) {
            return None;
        }
        self.awaiting_line_editor_since = None;
        self.enter_window_since = None;
        mem::take(&mut self.prompt_hold_queued).then_some(self.prompt_hold_id)
    }

    /// The view released the keys of queued prompt hold `id` (all accounted through
    /// [`Self::route_released_chunk`]): the hold is over, a request may be decided again.
    pub fn finish_queued_prompt_hold(&mut self, id: u64) {
        if id == self.prompt_hold_id {
            self.prompt_hold_queued = false;
        }
    }

    /// Whether, on a ConPTY session, the user typed into the shell's line so recently that a
    /// request now would land in the middle of the line (#1162).
    fn typing_a_line_on_conpty(&self, now: Instant) -> bool {
        // #1387: with the prompt-sequence path the request is written at a queue boundary, not
        // after a guessed pause. Keys of the line that are still on their way are the hand-off's
        // own writes in flight and the keys the input queue holds; `on_line_editor_active` already
        // waits for both (`writes_in_flight`, `prompt_hold_queued`), so no key of this line can be
        // between the request and the pty.
        if self.prompt_seq_mode() {
            return false;
        }
        self.conpty_input
            && (self
                .last_line_bytes_at
                .is_some_and(|at| now.saturating_duration_since(at) < CONPTY_MIDLINE_QUIET)
                || self.queued_line_bytes.recent(CONPTY_MIDLINE_QUIET))
    }

    pub fn set_foreground_probe(&mut self, probe: ForegroundProbe) {
        self.foreground_probe = probe;
    }

    /// Enables (or, with the kill switch, disables) the prompt-sequence path (#1353 phase 6).
    pub fn set_prompt_seq_enabled(&mut self, enabled: bool) {
        self.prompt_seq_enabled = enabled;
    }

    /// Whether decisions use the shell's prompt sequence numbers and the sampled shell state
    /// instead of elapsed time: the kill switch is not set and this shell has sent a number.
    pub fn prompt_seq_mode(&self) -> bool {
        self.prompt_seq_enabled && self.prompt_seq.is_some()
    }

    /// The last Precmd sequence number parsed.
    #[cfg(test)]
    pub fn last_prompt_seq(&self) -> Option<PromptSeq> {
        self.prompt_seq
    }

    /// A Precmd carrying the shell's sequence number was parsed (model thread), after
    /// [`Self::on_precmd`]. A payload without one (an older bootstrap) changes nothing.
    pub fn on_precmd_seq(&mut self, session: Option<u64>, seq: Option<u64>) {
        let Some(seq) = seq else {
            return;
        };
        let preexecs_before = match self.prompt_seq {
            Some(prev) if prev.session == session && seq > prev.seq => Some(seq - prev.seq - 1),
            Some(_) | None => None,
        };
        self.prompt_seq = Some(PromptSeq {
            session,
            seq,
            preexecs_before,
        });
    }

    /// The shell's line editor said it is reading a line (model thread, in stream order after the
    /// prompt's Precmd). Returns whether it is news: a second marker for the same prompt
    /// (`seq`), or one older than the last Precmd's, changes nothing.
    pub fn on_line_editor_reading(&mut self, session: Option<u64>, seq: Option<u64>) -> bool {
        warpui::input_order::note_shell_signal(warpui::input_order::ShellSignal::LineEditorReady);
        self.line_editor_markers_live = true;
        let stale = match (seq, self.prompt_seq) {
            (Some(seq), Some(prompt)) => prompt.session == session && seq < prompt.seq,
            _ => false,
        };
        if stale || (self.shell_activity == ShellActivity::Reading && self.reading_seq == seq) {
            return false;
        }
        self.shell_activity = ShellActivity::Reading;
        self.reading_seq = seq;
        true
    }

    /// The shell showed a continuation prompt (PS2): it consumed the Enter that ended the line
    /// so far, which no Precmd and no Preexec answers (model thread, in stream order). The line
    /// counts are re-baselined on this event instead of waiting for the logged give-up of the
    /// stale counts: one unanswered Enter is answered. Returns whether the counts changed.
    pub fn on_continuation_prompt(&mut self) -> bool {
        self.line_editor_markers_live = true;
        self.shell_activity = ShellActivity::Continuation;
        if let HandoffState::PtyOwned {
            enters_written,
            precmds_seen,
            ..
        } = &mut self.state
            && *enters_written > *precmds_seen
        {
            *precmds_seen += 1;
            // A line the counts counted as unanswered was answered: a reset armed before this
            // (which would treat every line as consumed) is void.
            self.activity_generation = self.activity_generation.wrapping_add(1);
            return true;
        }
        false
    }

    /// What the shell's markers say it is doing now.
    pub fn shell_activity(&self) -> ShellActivity {
        self.shell_activity
    }

    /// Whether this session's shell sends line-editor markers (one was seen).
    pub fn line_editor_markers_live(&self) -> bool {
        self.line_editor_markers_live
    }

    /// The parser met a shell hook (model thread, in stream order, BEFORE the block lifecycle
    /// decides whether to apply it), see [`ParsedShellHook`].
    pub fn note_shell_hook_parsed(&mut self, hook: ParsedShellHook) {
        self.last_parsed_hook = Some(hook);
    }

    /// The command that just finished was interactive (a program read the keys typed into it)
    /// (model thread, before the Precmd that follows it).
    pub fn note_interactive_command_finished(&mut self) {
        self.interactive_command_finished = true;
    }

    /// Whether the line counts say lines are queued that no prompt has answered yet.
    pub fn counts_diverged(&self) -> bool {
        matches!(
            self.owner(),
            Owner::Pty { enters_written, precmds_seen, .. } if enters_written > precmds_seen
        )
    }

    /// The sampled state of the shell, for a decision made now (see [`ShellPromptFact`]).
    pub fn sample_prompt_fact(&self) -> ShellPromptFact {
        self.foreground_probe.prompt_fact()
    }

    /// Whether the hand-back after a Ctrl-C waits for the shell's answer to a request rather
    /// than a timer.
    pub fn interrupt_barrier_pending(&self) -> bool {
        self.interrupt_barrier
    }

    /// Takes the "the report of the request behind a Ctrl-C hands input back" flag; `true` when it
    /// was still set, i.e. the give-up of that wait fired with no report having come.
    pub fn take_interrupt_barrier(&mut self) -> bool {
        mem::take(&mut self.interrupt_barrier)
    }

    /// Takes the "a Precmd is awaited to hand input back after Ctrl-C" flag: `true` when it was
    /// still set, i.e. the give-up of that wait fired with no prompt cycle having come.
    pub fn take_interrupt_awaiting_cycle(&mut self) -> bool {
        mem::take(&mut self.interrupt_awaiting_cycle)
    }

    /// Whether a prompt cycle is awaited after Ctrl-C (see [`Self::take_interrupt_awaiting_cycle`]).
    pub fn interrupt_awaiting_cycle(&self) -> bool {
        self.interrupt_awaiting_cycle
    }

    /// The p90 of the measured request-to-report round trips, `None` before any.
    fn rtt_p90(&self) -> Option<Duration> {
        if self.rtt_samples.is_empty() {
            return None;
        }
        let mut sorted: Vec<Duration> = self.rtt_samples.iter().copied().collect();
        sorted.sort_unstable();
        let index = (sorted.len() * 9).div_ceil(10).saturating_sub(1);
        sorted.get(index).copied()
    }

    /// How long the view should wait for the prompt before releasing the hold just started. The
    /// race of #933 can only be a precmd already in flight, which lands within about one round
    /// trip, so with measurements it is [`RTT_HOLD_FACTOR`] times their p90, between
    /// [`YOUNG_ENTER_FLOOR`] and what is left of [`YOUNG_ENTER_WINDOW`] counted from the last
    /// activity. Before any round trip was measured it is that remainder, never less than
    /// [`YOUNG_ENTER_MIN_HOLD`]. A prompt or report arriving first releases the hold at once.
    pub fn young_hold_wait(&self, now: Instant) -> Duration {
        if matches!(
            self.young_hold_evidence,
            Some(
                YoungHoldEvidence::RequestDue
                    | YoungHoldEvidence::PrecmdInFlight
                    | YoungHoldEvidence::PrecmdMaybeInFlight
            )
        ) {
            return EVENT_HOLD_GIVE_UP;
        }
        let left = self.last_activity_at.map_or(Duration::ZERO, |at| {
            YOUNG_ENTER_WINDOW.saturating_sub(now.saturating_duration_since(at))
        });
        match self.rtt_p90() {
            Some(p90) => p90
                .saturating_mul(RTT_HOLD_FACTOR)
                .min(left)
                .clamp(YOUNG_ENTER_FLOOR, YOUNG_ENTER_WINDOW),
            None => left.clamp(YOUNG_ENTER_MIN_HOLD, YOUNG_ENTER_WINDOW),
        }
    }

    #[cfg(test)]
    pub(crate) fn record_rtt_for_test(&mut self, rtt: Duration) {
        self.record_rtt(rtt);
    }

    /// A new shell session (not a subshell) took the pty over, e.g. the remote shell of an `ssh`
    /// (#1131): its loop through the shell and the pipeline is unrelated to the previous
    /// session's, so the measured round trips are dropped and the next command measures afresh.
    /// A subshell (`sudo -i`, `ksu`) runs on the same machine and keeps them.
    pub fn on_new_session(&mut self) {
        self.rtt_samples.clear();
        self.rtt_pending.clear();
        self.command_written_at = None;
        // A new shell may signal where the last one did not (and must earn the per-prompt give-up
        // again, #1573).
        self.prompt_hold_gave_up = false;
        self.line_editor_signal_seen = false;
        self.enter_window_since = None;
        // ... and may predate the sequence numbers (an older remote bootstrap): it earns the
        // prompt-sequence path again with its first number.
        self.prompt_seq = None;
        self.interrupt_barrier = false;
        self.interrupt_awaiting_cycle = false;
        self.interactive_command_finished = false;
        // ... and may not send line-editor markers either.
        self.shell_activity = ShellActivity::Unknown;
        self.reading_seq = None;
        self.line_editor_markers_live = false;
        self.last_parsed_hook = None;
        // ... and the latch of the command that handed the pty over is the old session's.
        self.command_interactive = false;
        self.settle_unsampled_enter("new_session");
    }

    /// #1614: the block latch said the running command is interactive (main thread, from
    /// `CommandBecameInteractive`, for the block that is still the active executing one): a
    /// program, a blocked builtin or a login hop reads the tty. The keys typed into it are its.
    /// Decisive only where the platform samples nothing (ConPTY): the latch fires 50 ms into
    /// nearly any command and stays set until the command's Precmd is parsed, so after the command
    /// exits it is stale for the length of the pipeline while the sample is current (#933 is
    /// exactly that moment). Where there is a sample, the sample decides; here a young hold live
    /// now ends as typeahead only on an unsampled platform ([`YoungHoldEnd::Typeahead`]).
    pub fn note_command_interactive(&mut self) {
        self.command_interactive = true;
        if !self.foreground_probe.samples_shell() && self.end_young_hold_as_typeahead() {
            warpui::input_order::log_hop("young_hold_end", || {
                format!("why=command_interactive hold_id={}", self.young_hold_id)
            });
        }
    }

    /// Whether the block latch has said the running command is interactive (see
    /// [`Self::note_command_interactive`]).
    pub fn command_interactive(&self) -> bool {
        self.command_interactive
    }

    /// The evidence the live young hold stands on (`None` without a live hold), for the view's log
    /// when the cap fires and for the retry it asks for when the hold opens.
    pub fn young_hold_evidence(&self) -> Option<YoungHoldEvidence> {
        self.young_hold_queued
            .then_some(self.young_hold_evidence)
            .flatten()
    }

    /// Whether a live young hold waits on the stream barrier ([`YoungHoldEvidence::PrecmdInFlight`]),
    /// so the pty reader's publish has something to decide (a read, before the write lock).
    pub fn young_hold_waits_on_barrier(&self) -> bool {
        self.young_hold_evidence() == Some(YoungHoldEvidence::PrecmdInFlight)
    }

    /// Ends a live young hold whose keys are in the view's queue as typeahead for what owns the
    /// tty (#1614): only while the pty owns input (once the request is out the report runs the
    /// Enter). Returns whether a hold ended.
    fn end_young_hold_as_typeahead(&mut self) -> bool {
        if !self.young_hold_queued || !matches!(self.state, HandoffState::PtyOwned { .. }) {
            return false;
        }
        self.young_hold_queued = false;
        self.young_hold_evidence = None;
        self.young_hold_ends
            .push((self.young_hold_id, YoungHoldEnd::Typeahead));
        true
    }

    /// Records one measured round trip through the shell and the pty pipeline, in the rolling
    /// window the young hold is sized from.
    fn record_rtt(&mut self, rtt: Duration) {
        if self.rtt_samples.len() >= RTT_WINDOW {
            self.rtt_samples.pop_front();
        }
        self.rtt_samples.push_back(rtt);
    }

    /// Whether keys typed on the terminal go to the pty (or are held for it) rather than being
    /// routed by the pre-hand-off rules.
    pub fn routes_keys_to_pty(&self) -> bool {
        matches!(
            self.state,
            HandoffState::PtyOwned { .. } | HandoffState::HoldAfterEnter { .. }
        ) || self.holds_young_enter_behind_request()
    }

    /// The request is outstanding and the user's Enter is held behind it (#933): keys typed now
    /// must queue behind that Enter, not reach the editor ahead of it, so the terminal keeps
    /// routing them (into the hold).
    fn holds_young_enter_behind_request(&self) -> bool {
        self.state == HandoffState::AwaitingReport && self.young_hold_queued
    }

    /// Whether the terminal, rather than the input editor, should hold focus.
    pub fn wants_terminal_focus(&self) -> bool {
        self.routes_keys_to_pty()
    }

    pub fn is_pty_owned(&self) -> bool {
        matches!(self.state, HandoffState::PtyOwned { .. })
    }

    /// Whether the editor owns input, so stored typeahead can be inserted into it.
    pub fn editor_owns_input(&self) -> bool {
        matches!(
            self.state,
            HandoffState::EditorOwned | HandoffState::AwaitingReport
        )
    }

    /// Whether text the editor receives now will be followed by the shell's report in front of
    /// it (or never reaches the editor at all), i.e. the editor cannot know the quote or bracket
    /// context of what it is given (#949). The editor must insert typed symbols literally: an
    /// auto-inserted closer would pair with an opener that is in the report, not the editor.
    pub fn editor_text_precedes_report(&self) -> bool {
        // #1089: also while user bytes the hand-off did not adopt may be in the shell's line,
        // until a report requested behind them is in: the legacy report puts them in front of
        // the editor's text just the same (`echo "a b` + ` c" > x` became `echo "a b c" > x"`).
        !matches!(self.state, HandoffState::EditorOwned) || self.untracked_line_bytes
    }

    /// User bytes that reach the shell's line are about to be written while the editor owns
    /// input and the hand-off did not adopt them (#1089; see
    /// [`Self::adopt_untracked_typeahead`] for the keys it does adopt). Only meaningful for a
    /// shell that reports its line; a no-op unless the editor owns input. A request already
    /// outstanding does not cover them: they are behind it in the tty.
    pub fn note_untracked_line_bytes(&mut self) {
        if matches!(
            self.state,
            HandoffState::EditorOwned | HandoffState::AwaitingReport
        ) {
            self.untracked_line_bytes = true;
            self.untracked_cover_after_reports = None;
        }
    }

    /// #1574: the view delivered bytes to the shell's line as a FINAL delivery (the #1494 path:
    /// held ssh typeahead written to an unbootstrapped remote after `stty sane`). They belong to
    /// that shell, where the toast said they went: while the mark is set no report is asked for
    /// (`on_line_editor_active` answers `Drain`), so the shell keeps the line and the pty keeps
    /// input. The mark ends as soon as the line cannot hold them any more (a commit to the
    /// editor, a command written, a user Enter written).
    pub fn note_committed_delivery(&mut self) {
        self.committed_delivery = true;
        warpui::input_order::log_hop("handoff_committed_delivery", String::new);
    }

    /// Whether a committed delivery may still be in the shell's line, for tests.
    #[cfg(test)]
    pub fn committed_delivery_pending(&self) -> bool {
        self.committed_delivery
    }

    /// `PtyController` wrote a legacy request (the editor owns input). Its report covers every
    /// user byte written before it.
    pub fn on_legacy_request_written(&mut self) {
        self.on_request_written();
    }

    /// A request (legacy or the hand-off's own) is written: its report, once every report ahead
    /// of it is in, covers the untracked bytes written before it.
    fn on_request_written(&mut self) {
        warpui::input_order::note_shell_signal(warpui::input_order::ShellSignal::RequestDecided);
        if self.rtt_pending.len() >= RTT_WINDOW {
            self.rtt_pending.pop_front();
        }
        self.rtt_pending.push_back(Instant::now());
        self.requests_outstanding = self.requests_outstanding.saturating_add(1);
        if self.untracked_line_bytes && self.untracked_cover_after_reports.is_none() {
            self.untracked_cover_after_reports = Some(self.requests_outstanding);
        }
    }

    /// A report arrived: once it is the one that answers a request written after the untracked
    /// bytes, they are back in the editor.
    fn on_report_received(&mut self) {
        warpui::input_order::note_shell_signal(warpui::input_order::ShellSignal::ReportReceived);
        if let Some(written_at) = self.rtt_pending.pop_front() {
            self.record_rtt(written_at.elapsed());
        }
        self.requests_outstanding = self.requests_outstanding.saturating_sub(1);
        if let Some(remaining) = self.untracked_cover_after_reports.as_mut() {
            *remaining = remaining.saturating_sub(1);
            if *remaining == 0 {
                self.forget_untracked_line_bytes();
            }
        }
    }

    /// Whether a request written after the untracked bytes is outstanding, for tests.
    #[cfg(test)]
    pub(crate) fn untracked_bytes_requested(&self) -> bool {
        self.untracked_cover_after_reports.is_some()
    }

    /// The shell's line no longer holds untracked user bytes: they were reported, or a command's
    /// kill prefix ended the line. Outstanding requests are still in the tty and stay counted.
    fn forget_untracked_line_bytes(&mut self) {
        self.untracked_line_bytes = false;
        self.untracked_cover_after_reports = None;
    }

    /// Whether `PtyController` must hold its queued writes: the shell's line may hold user bytes
    /// that a queued command's kill prefix would destroy before they were reported.
    pub fn holds_queued_writes(&self) -> bool {
        matches!(
            self.state,
            HandoffState::PtyOwned {
                wrote_user_bytes: true,
                ..
            }
        ) || self.untracked_bytes_uncovered()
    }

    /// Whether the shell's line may hold typed text right now (#1562): keys were delivered to
    /// the pty and no Enter, ^C or ^U has ended the line since, or untracked user bytes sit in
    /// it. A setup write (ssh warpify, subshell bootstrap) must not land on such a line: it
    /// would be appended to it, or would have to kill it.
    pub fn shell_line_may_hold_typed_text(&self) -> bool {
        matches!(
            self.state,
            HandoffState::PtyOwned {
                wrote_user_bytes: true,
                ..
            }
        ) || self.untracked_bytes_uncovered()
    }

    /// Untracked user bytes (#1089) are in the shell's line and no request has been written
    /// behind them (#1104): a queued command's kill prefix would destroy them unreported.
    fn untracked_bytes_uncovered(&self) -> bool {
        self.untracked_line_bytes
            && self.untracked_cover_after_reports.is_none()
            && matches!(
                self.state,
                HandoffState::EditorOwned | HandoffState::AwaitingReport
            )
    }

    /// Whether the editor owns input and a report has left untracked bytes in the shell's line
    /// that no request covers yet, so the view should ask the controller to write one (#1104).
    pub fn drain_pending(&self) -> bool {
        self.state == HandoffState::EditorOwned && self.untracked_bytes_uncovered()
    }

    /// Whether a request must be written now, because untracked bytes sit in the shell's line
    /// behind every request written so far (#1104): the shell reports its line as of the request
    /// and clears it, so what was typed after it would stay there, to be killed by the next
    /// command's kill prefix or to prefix the next line. Records the request as written when it
    /// is due. `at_prompt` and `can_request` are as for [`Self::on_line_editor_active`].
    pub fn drain_request_due(&mut self, at_prompt: bool, can_request: bool) -> bool {
        let owner_allows = matches!(
            self.state,
            HandoffState::EditorOwned
                | HandoffState::HoldAfterEnter {
                    released: false,
                    ..
                }
        );
        if !(at_prompt
            && can_request
            && owner_allows
            && self.untracked_line_bytes
            && self.untracked_cover_after_reports.is_none())
        {
            return false;
        }
        self.on_request_written();
        true
    }

    /// The id of a deferral the view must arm a patience timer for (see
    /// [`Self::give_up_uncovered_deferral`]), once.
    pub fn take_uncovered_deferral_started(&mut self) -> Option<u64> {
        if mem::take(&mut self.uncovered_deferral_started) {
            self.uncovered_deferral_id
        } else {
            None
        }
    }

    fn arm_uncovered_deferral(&mut self) {
        self.uncovered_deferral_seq = self.uncovered_deferral_seq.wrapping_add(1);
        self.uncovered_deferral_id = Some(self.uncovered_deferral_seq);
        self.uncovered_deferral_started = true;
    }

    /// The id of a report deferral the view must arm a patience timer for (see
    /// [`Self::give_up_report_deferral`]), once.
    pub fn take_report_deferral_started(&mut self) -> Option<u64> {
        if mem::take(&mut self.report_deferral_started) {
            self.report_deferral_id
        } else {
            None
        }
    }

    fn arm_report_deferral(&mut self) {
        self.report_deferral_seq = self.report_deferral_seq.wrapping_add(1);
        self.report_deferral_id = Some(self.report_deferral_seq);
        self.report_deferral_started = true;
    }

    /// The patience timer of report deferral `id` ran out (#1215): the report the Enter waited
    /// for is not coming (a program ate the request, the shell hung). The Enter is given up
    /// WITHOUT running: the shell's line still holds the head of what the user typed, so the
    /// editor's text would run as a fragment. Input goes back to the editor; a late report is then
    /// inserted in front of it like any legacy report. Returns the id of the gate that holds the
    /// keys typed behind the Enter, for the view to take them out and put them in the editor;
    /// `None` when the deferral is over already.
    pub fn give_up_report_deferral(&mut self, id: u64) -> Option<u64> {
        if self.report_deferral_id != Some(id) {
            return None;
        }
        let HandoffState::HoldAfterEnter {
            released: false, ..
        } = &self.state
        else {
            return None;
        };
        self.report_deferral_id = None;
        self.uncovered_deferral_id = None;
        self.deferred_young = false;
        self.drop_young_hold();
        self.state = HandoffState::EditorOwned;
        self.deferred_gate.take()
    }

    /// The patience timer of deferral `id` ran out: the report covering the untracked bytes
    /// never came (a program ate the request), so the Enter runs as it did before #1104 rather
    /// than wait forever. Returns whether it was released; the caller then runs the Enter.
    pub fn give_up_uncovered_deferral(&mut self, id: u64) -> bool {
        if self.uncovered_deferral_id != Some(id)
            || !matches!(
                self.state,
                HandoffState::HoldAfterEnter {
                    released: false,
                    ..
                }
            )
        {
            return false;
        }
        self.uncovered_deferral_id = None;
        self.forget_untracked_line_bytes();
        self.deferred_young = false;
        self.mark_released();
        true
    }

    /// Whether the pty owns input and some Enter written to it is not answered by a prompt
    /// yet, i.e. the shell may still have user lines to run. A command that starts its own
    /// block when written must wait meanwhile, or the user lines' output would land in it.
    pub fn has_unanswered_lines(&self) -> bool {
        matches!(
            self.owner(),
            Owner::Pty {
                enters_written,
                precmds_seen,
                ..
            } if enters_written > precmds_seen
        )
    }

    pub fn held_agent_command(&self) -> Option<&HeldAgentCommand> {
        self.held_agent_command.as_ref()
    }

    pub fn held_command_action(&self) -> Option<&AIAgentActionId> {
        self.held_agent_command.as_ref().map(|held| &held.action_id)
    }

    pub fn set_held_agent_command(&mut self, held: Option<HeldAgentCommand>) {
        self.held_agent_command = held;
    }

    pub fn writes_in_flight(&self) -> u32 {
        self.writes_in_flight
    }

    fn owner(&self) -> Owner {
        match &self.state {
            HandoffState::EditorOwned => Owner::Editor,
            HandoffState::PtyOwned {
                wrote_user_bytes,
                enters_written,
                precmds_seen,
            } => Owner::Pty {
                wrote_user_bytes: *wrote_user_bytes,
                enters_written: *enters_written,
                precmds_seen: *precmds_seen,
            },
            HandoffState::AwaitingReport => Owner::AwaitingReport,
            HandoffState::HoldAfterEnter { released, .. } => Owner::Hold {
                released: *released,
            },
        }
    }

    /// A command from the input editor was accepted and written (or queued) to the pty. The keys
    /// held behind an Enter (a young hold, or a deferred Enter's `HandoffReport` gate) follow it
    /// as typeahead: the end is recorded for the view, which writes them behind the command and
    /// counts them ([`Self::count_followed_young_hold`]).
    pub fn on_command_written(&mut self) {
        self.forget_untracked_line_bytes();
        // #1574: the command's kill prefix and Enter end whatever line it lands on.
        self.committed_delivery = false;
        self.interrupt_barrier = false;
        self.interrupt_awaiting_cycle = false;
        // #1614: a new command runs; whether it is interactive is this block's latch to say, and
        // its own Preexec is the next one.
        self.command_interactive = false;
        self.own_preexec_pending = true;
        // #1615: the editor wrote the next command, so a delivered Enter's line is settled.
        self.settle_unsampled_enter("editor_command");
        // An Enter held behind the request follows the command as typeahead (#933): the view
        // writes the keys of the hold once it is told (below); in every state but `PtyOwned` the
        // hold is void.
        let queued_hold = mem::take(&mut self.young_hold_queued);
        self.young_hold_evidence = None;
        let mut queued_hold_follows = false;
        // Keys held behind a deferred Enter follow its command too.
        let mut deferred_follows = None;
        self.young_line = false;
        self.deferred_young = false;
        let (enters_before, precmds_seen) = match mem::take(&mut self.state) {
            HandoffState::HoldAfterEnter { .. } => {
                deferred_follows = self.deferred_gate.take();
                (0, 0)
            }
            // Lines the user already queued are still ahead of this command in the tty.
            HandoffState::PtyOwned {
                enters_written,
                precmds_seen,
                ..
            } => {
                queued_hold_follows = queued_hold;
                (enters_written, precmds_seen)
            }
            HandoffState::EditorOwned | HandoffState::AwaitingReport => (0, 0),
        };
        let now = Instant::now();
        if self.enter_window_may_open() {
            self.enter_window_since = Some(now);
        }
        self.last_activity_at = Some(now);
        self.command_written_at = Some(now);
        self.state = HandoffState::PtyOwned {
            wrote_user_bytes: false,
            enters_written: enters_before.saturating_add(1),
            precmds_seen,
        };
        if queued_hold {
            let id = self.young_hold_id;
            if queued_hold_follows {
                self.young_hold_follow_pending = Some(id);
                self.young_hold_ends.push((id, YoungHoldEnd::FollowCommand));
            } else {
                self.young_hold_ends.push((id, YoungHoldEnd::Dropped));
            }
        }
        if let Some(id) = deferred_follows {
            self.young_hold_follow_pending = Some(id);
            self.young_hold_ends.push((id, YoungHoldEnd::FollowCommand));
        }
    }

    /// The view wrote the keys of queued young hold `id` behind the command (it took them out of
    /// the gate for [`YoungHoldEnd::FollowCommand`]): they are counted as the lines and bytes
    /// they are, exactly as the buffered hold's keys were when the command was written, and as one
    /// write in flight (the writer retires it).
    pub fn count_followed_young_hold(&mut self, id: u64, bytes: &[u8]) {
        if self.young_hold_follow_pending == Some(id) {
            self.young_hold_follow_pending = None;
        }
        if bytes.is_empty() {
            return;
        }
        if let HandoffState::PtyOwned {
            wrote_user_bytes,
            enters_written,
            ..
        } = &mut self.state
        {
            let (enters, dirty) = scan_line_terminators(bytes, false, !self.ctrl_u_not_line_kill);
            *enters_written = enters_written.saturating_add(enters);
            *wrote_user_bytes = dirty;
            if enters > 0 {
                self.activity_generation = self.activity_generation.wrapping_add(1);
            }
        }
        self.writes_in_flight = self.writes_in_flight.saturating_add(1);
    }

    /// A command the hand-off does not own (a queued-prompt-panel command, an agent command, an
    /// in-band command) was written while the pty owns input. Its Enter is counted, so its
    /// precmd does not make lines the user queued ahead of it look answered. Its kill prefix and
    /// Enter end whatever line it lands on.
    pub fn on_uncounted_command_written(&mut self) {
        if self.enter_window_may_open() {
            self.enter_window_since = Some(Instant::now());
        }
        self.forget_untracked_line_bytes();
        // #1574: its kill prefix ends a line delivered bytes were in.
        self.committed_delivery = false;
        // Its kill prefix ends the line a held Enter belongs to.
        self.drop_young_hold();
        self.young_line = false;
        if let HandoffState::PtyOwned {
            wrote_user_bytes,
            enters_written,
            ..
        } = &mut self.state
        {
            *enters_written = enters_written.saturating_add(1);
            *wrote_user_bytes = false;
        }
    }

    /// A prompt was applied to the model; the view has yet to handle its precmd event (model
    /// thread). See [`Self::precmds_pending_view`](TypeaheadHandoff).
    pub fn note_precmd_pending_view(&mut self) {
        if !self.tracks_view_precmds {
            return;
        }
        self.precmds_pending_view = self.precmds_pending_view.saturating_add(1);
    }

    /// A terminal view is attached and will call [`Self::on_precmd_viewed`] for every prompt.
    pub fn track_view_precmds(&mut self) {
        self.tracks_view_precmds = true;
    }

    /// Prompts the model has seen whose precmd event the view has not handled yet (0 when no view
    /// tracks them).
    pub fn precmds_pending_view(&self) -> u32 {
        self.precmds_pending_view
    }

    /// The view handled a precmd event, after reinitializing the editor for the finished block
    /// (main thread).
    pub fn on_precmd_viewed(&mut self) {
        self.precmds_pending_view = self.precmds_pending_view.saturating_sub(1);
    }

    /// The shell emitted a precmd hook (model thread).
    pub fn on_precmd(&mut self) {
        self.shell_activity = ShellActivity::Prompt;
        self.reading_seq = None;
        self.activity_generation = self.activity_generation.wrapping_add(1);
        let now = Instant::now();
        self.last_activity_at = Some(now);
        self.last_precmd_at = Some(now);
        if self.prompt_hold_armed() {
            self.awaiting_line_editor_since = Some(now);
        }
        // The command's prompt: the wait above holds the keys from here (#1490).
        self.enter_window_since = None;
        self.follow_unsampled_enter_on_precmd(now);
        warpui::input_order::note_shell_signal(warpui::input_order::ShellSignal::Precmd);
        self.interrupt_awaiting_cycle = false;
        // #1614: the command that was interactive is over; the shell is back at its prompt, and a
        // Preexec it never sent (an empty line, a syntax error) is not pending any more.
        self.command_interactive = false;
        self.own_preexec_pending = false;
        let consumed_by_program = mem::take(&mut self.interactive_command_finished);
        let seq_mode = self.prompt_seq_mode();
        // #1552: shows which of the two blocked the correction below on a Git Bash session.
        warpui::input_order::log_hop("precmd_seen", || {
            let (enters, precmds) = match &self.state {
                HandoffState::PtyOwned {
                    enters_written,
                    precmds_seen,
                    ..
                } => (*enters_written, *precmds_seen),
                _ => (0, 0),
            };
            format!(
                "consumed={consumed_by_program} seq_mode={seq_mode} enters={enters} precmds={precmds}"
            )
        });
        if let HandoffState::PtyOwned {
            precmds_seen,
            enters_written,
            wrote_user_bytes,
            ..
        } = &mut self.state
        {
            *precmds_seen = precmds_seen.saturating_add(1);
            // #1589 liveness: delivered bytes that never landed (the remote exited, an unmanaged
            // remote sends no Precmd) are not in the outer shell's line once its prompt is back
            // with nothing written or queued: the mark clears on this event.
            if self.committed_delivery && !*wrote_user_bytes && *precmds_seen >= *enters_written {
                self.committed_delivery = false;
            }
            // #1353 phase 6 (PTY-3): the command that ended was reading the keys typed into it
            // (it became interactive), so the Enters written while it ran were consumed by it and
            // no prompt answers them. Answering them here is the event the 250 ms quiet period
            // used to stand in for. Decided by the previous command's own flag, so a stalled main
            // thread cannot change it.
            if consumed_by_program && seq_mode && *enters_written > *precmds_seen {
                *precmds_seen = *enters_written;
            }
        }
    }

    /// The shell emitted a preexec hook (model thread). `shell_started` is whether the shell
    /// started this command itself (a typeahead line), rather than the app writing it. Returns
    /// whether the pty took over input.
    pub fn on_preexec(&mut self, shell_started: bool) -> bool {
        warpui::input_order::note_shell_signal(warpui::input_order::ShellSignal::Preexec);
        self.shell_activity = ShellActivity::Running;
        self.activity_generation = self.activity_generation.wrapping_add(1);
        let now = Instant::now();
        // A prompt hold whose keys wait for this prompt's line editor: the shell started a command
        // instead (a line it already had), so the keys are typeahead for that command, held as in
        // an enter window and released by its events (the command becoming interactive, its
        // prompt, a continuation prompt, a signal key), never left with no wait at all.
        if self.prompt_hold_queued && self.awaiting_line_editor_since.is_some() {
            self.enter_window_since = Some(now);
        }
        // The shell took a line: its line editor is not about to start.
        self.awaiting_line_editor_since = None;
        self.last_activity_at = Some(now);
        // #1573: an enter window whose command started is not overdue while it runs.
        self.last_preexec_at = Some(now);
        self.follow_unsampled_enter_on_preexec(shell_started, now);
        // The editor's command reached the shell: one write-to-preexec loop measured. A line the
        // shell started itself ran ahead of that command, so its preexec is not the command's and
        // the command's own would include the line's run: the sample is dropped.
        if let Some(written_at) = self.command_written_at.take()
            && !shell_started
        {
            self.record_rtt(written_at.elapsed());
        }
        // #1614: the shell started a command after the hold opened, so no prompt answers the held
        // Enter before that command has run; the Enter and the keys behind it are its typeahead,
        // written now by this event (the documented closing event of the `PromptSeen` gate). Not
        // when this Preexec is the written command's own (a line typed and Entered right after the
        // submit, before its Preexec was parsed): that line is the next prompt's, held for it.
        let own_preexec = mem::take(&mut self.own_preexec_pending);
        if !own_preexec && self.end_young_hold_as_typeahead() {
            warpui::input_order::log_hop("young_hold_end", || {
                format!("why=preexec hold_id={}", self.young_hold_id)
            });
        }
        if shell_started && self.state == HandoffState::EditorOwned {
            self.state = HandoffState::PtyOwned {
                wrote_user_bytes: false,
                enters_written: 1,
                precmds_seen: 0,
            };
            return true;
        }
        false
    }

    /// The user is about to write keys to a running command the hand-off does not own (one the
    /// queued-command panel or an env-var collection wrote, or any command started while the
    /// editor owned input) (#1089). Whatever the command leaves unread is in the shell's line at
    /// the next prompt, so the hand-off takes it over exactly as it does a command submitted from
    /// the editor: the keys are counted, the request is written only behind every one of them,
    /// and the editor inserts what they typed literally until the report is in.
    ///
    /// Left to the legacy path, those keys were never counted. The legacy request could already
    /// be in the tty ahead of them (it goes out whenever the line editor is active), so the
    /// shell's report missed them and they stayed in its line, to prefix the next line run there
    /// (`)exit`) or be killed by the next command's kill prefix. Keys the editor took meanwhile
    /// were auto-paired, and the late report was placed in front of them with the cursor moved to
    /// the end: `$(uname -n)` became `$()uname -n)`.
    ///
    /// The command's own Enter is counted as unanswered, so its prompt (not an earlier one) is
    /// the one that hands input back. Returns whether ownership changed; it only does from
    /// [`HandoffState::EditorOwned`].
    pub fn adopt_untracked_typeahead(&mut self) -> bool {
        if self.state != HandoffState::EditorOwned {
            return false;
        }
        self.state = HandoffState::PtyOwned {
            wrote_user_bytes: false,
            enters_written: 1,
            precmds_seen: 0,
        };
        true
    }

    /// Hands input back to the editor without a request when nothing the user typed can be in
    /// the shell's line. Called on the main thread once a prompt was seen, after the editor has
    /// been reinitialized for the new block. Returns whether the editor took over.
    pub fn try_commit_at_prompt(&mut self) -> bool {
        // #1569: keys the view's queue holds for the shell's line are not counted yet (the hand-off
        // keeps no bytes; `route_released_chunk` counts them when the line editor's signal releases
        // them), so the line looks empty and the editor would take over -- and the keys typed next
        // would go to the editor, apart from and ahead of the held ones, which are then written raw
        // to the pty. The enter window (#1490) holds keys across this very precmd. The pty keeps
        // the line until the release has counted what it holds; the release asks for the decision
        // again (`RetryTypeaheadHandoffRequest`), which commits or requests with the true counts.
        if self.prompt_hold_queued {
            warpui::input_order::log_hop("commit_deferred_prompt_hold", String::new);
            return false;
        }
        // #1589: bytes delivered to the shell as final may still be deferred behind the queue's
        // gate (nothing counted yet); the line looks empty but is not. The pty keeps input until
        // they are written (`on_command_written` / the user's Enter clear the mark) or a prompt
        // proves the line empty (`on_precmd`).
        if self.committed_delivery {
            warpui::input_order::log_hop("commit_deferred_committed_delivery", String::new);
            return false;
        }
        match self.owner() {
            Owner::Pty {
                wrote_user_bytes: false,
                enters_written,
                precmds_seen,
            } if precmds_seen >= enters_written => {
                self.interrupt_barrier = false;
                self.interrupt_awaiting_cycle = false;
                self.state = HandoffState::EditorOwned;
                // #1574: nothing typed or delivered is in the shell's line any more.
                self.committed_delivery = false;
                true
            }
            Owner::Pty { .. } | Owner::Editor | Owner::AwaitingReport | Owner::Hold { .. } => false,
        }
    }

    /// The shell's line editor became active. This is the one place a request is decided, on
    /// the main thread and under the model lock, so it is serialized with every routing
    /// decision. `at_prompt` is whether the lifecycle phase is still at the prompt;
    /// `can_request` is whether the shell supports input reporting.
    pub fn on_line_editor_active(&mut self, at_prompt: bool, can_request: bool) -> ActiveDecision {
        match self.owner() {
            Owner::Editor => ActiveDecision::Legacy,
            Owner::Hold { released: false } if self.drain_request_due(at_prompt, can_request) => {
                ActiveDecision::SendRequest
            }
            Owner::AwaitingReport | Owner::Hold { .. } => ActiveDecision::Drain,
            Owner::Pty {
                wrote_user_bytes,
                enters_written,
                precmds_seen,
            } => {
                if self.young_hold_follow_pending.is_some() {
                    // #1353 phase 5b: the keys of a young hold follow the command just written and
                    // have not been counted yet (the view counts them as it writes them), so the
                    // shell's line looks shorter than it will be. Decide nothing until then.
                    return ActiveDecision::Wait {
                        activity_generation: self.activity_generation,
                    };
                }
                if self.prompt_hold_queued {
                    // #1353 phase 5: keys typed since the prompt wait in the view's queue and
                    // have not been counted yet, so the shell's line looks empty. Decide nothing
                    // until the view has released them (it asks for a retry then).
                    return ActiveDecision::Wait {
                        activity_generation: self.activity_generation,
                    };
                }
                let no_line_queued = precmds_seen >= enters_written;
                if no_line_queued && self.interrupt_awaiting_cycle {
                    // #1353 phase 6: a Ctrl-C was written and the prompt cycle that follows it has
                    // not been parsed yet. Its Precmd reinitializes the editor, so input is not
                    // handed over before it: that Precmd (an event) commits, with a logged give-up.
                    return ActiveDecision::Wait {
                        activity_generation: self.activity_generation,
                    };
                }
                if no_line_queued && self.precmds_pending_view > 0 {
                    // The editor has yet to be reinitialized for the finished block, which
                    // would erase whatever it took meanwhile. Keys keep going to the pty; the
                    // view's handling of the prompt decides again (#934).
                    return ActiveDecision::Wait {
                        activity_generation: self.activity_generation,
                    };
                }
                if self.committed_delivery && !wrote_user_bytes {
                    // #1589: the delivered bytes have not landed yet (deferred behind a gate), so
                    // the line looks empty but is not. The pty keeps input until they are written.
                    warpui::input_order::log_hop("handoff_request_skipped", || {
                        "reason=committed_delivery_pending".to_owned()
                    });
                    return ActiveDecision::Drain;
                }
                if !wrote_user_bytes && !self.interrupt_barrier {
                    if no_line_queued {
                        self.state = HandoffState::EditorOwned;
                        // #1574: the shell's line is empty: no delivered bytes are left in it.
                        self.committed_delivery = false;
                        ActiveDecision::Committed
                    } else if self.counts_provably_stale(enters_written, precmds_seen) {
                        // #1614: the sample says the shell's line editor reads and the barrier
                        // says every Precmd it wrote is applied, so the lines the counts still
                        // queue were consumed by the command that ended (a `read x`, a password,
                        // a program): stale NOW, a state fact, not after the 5 s give-up. The
                        // editor takes over as it would have with agreeing counts.
                        warpui::input_order::log_hop("handoff_stale_counts_reset", || {
                            format!(
                                "by=line_editor_reading_drained enters={enters_written} \
                                 precmds={precmds_seen}"
                            )
                        });
                        self.state = HandoffState::EditorOwned;
                        self.committed_delivery = false;
                        ActiveDecision::Committed
                    } else {
                        ActiveDecision::DrainAndWait {
                            activity_generation: self.activity_generation,
                        }
                    }
                } else if !can_request {
                    if self.young_hold_queued {
                        // #1614: nothing can ask the shell for its line, so no report will ever
                        // run the held Enter in the editor: the Enter and the keys behind it are
                        // typeahead for the shell's line, written now by this decision (the
                        // give-up cap only hands back, #1577, and would have lost the Enter).
                        if self.end_young_hold_as_typeahead() {
                            warpui::input_order::log_hop("young_hold_end", || {
                                format!("why=no_request_binding hold_id={}", self.young_hold_id)
                            });
                        }
                        return ActiveDecision::Drain;
                    }
                    self.interrupt_barrier = false;
                    self.state = HandoffState::EditorOwned;
                    ActiveDecision::Committed
                } else if self.committed_delivery {
                    // #1574: the line holds text delivered to the shell as final. A report would
                    // clear it from the shell; it stays there, and the pty keeps input.
                    warpui::input_order::log_hop("handoff_request_skipped", || {
                        "reason=committed_delivery".to_owned()
                    });
                    // #1614: a held Enter submits that line (as a typed Enter would, see
                    // `route_user_bytes_unheld`); no request will come for it.
                    if self.end_young_hold_as_typeahead() {
                        warpui::input_order::log_hop("young_hold_end", || {
                            format!("why=committed_delivery hold_id={}", self.young_hold_id)
                        });
                    }
                    ActiveDecision::Drain
                } else if self.typing_a_line_on_conpty(Instant::now()) {
                    // #1162: keys of a line are still arriving raw; the request waits for a
                    // pause (the quiet-period timer or the next flushed write decides again).
                    ActiveDecision::Wait {
                        activity_generation: self.activity_generation,
                    }
                } else if no_line_queued && at_prompt && self.writes_in_flight == 0 {
                    self.interrupt_barrier = false;
                    self.state = HandoffState::AwaitingReport;
                    // Its report covers every untracked byte written before it too (#1089).
                    self.on_request_written();
                    ActiveDecision::SendRequest
                } else {
                    ActiveDecision::Wait {
                        activity_generation: self.activity_generation,
                    }
                }
            }
        }
    }

    /// A routed user write. In [`HandoffState::PtyOwned`] it is counted, both as line content
    /// and as in flight; in [`HandoffState::HoldAfterEnter`] it is held.
    pub fn on_user_bytes(&mut self, bytes: &[u8]) -> UserBytesRoute {
        self.route_user_bytes(bytes, true)
    }

    /// A routed user write while a full-screen program (the alternate screen) runs. It reads the
    /// tty raw, so the bytes never reach the shell's line: they are in flight, but neither line
    /// content nor Enters for the shell (vim's `:wq\r` is answered by no prompt).
    pub fn on_user_bytes_to_full_screen_program(&mut self, bytes: &[u8]) -> UserBytesRoute {
        self.route_user_bytes(bytes, false)
    }

    fn route_user_bytes(&mut self, bytes: &[u8], reaches_shell_line: bool) -> UserBytesRoute {
        // A signal or EOF byte is never held: it drops a held Enter and goes out at once.
        if bytes.iter().any(is_signal_byte) {
            self.drop_young_hold();
            self.log_route_decision("signal_unheld", bytes);
            return self.route_user_bytes_unheld(bytes, reaches_shell_line);
        }
        self.drop_stale_hold();
        if self.young_hold_queued {
            // The view's queue holds the keys of a young hold and takes every key typed behind it
            // (its pre-key check ends a hold the state has outgrown first), so a key only gets
            // here if the queue and this state disagree (a write that bypassed the queue while
            // the gate was open). On disagreement the state is the one that says a hold is live,
            // so the key is held, never written ahead of the held Enter (#1422): it is reported
            // as the hold's first key again, and the view puts it behind the hold's gate (joining
            // the gate if it is open, opening it if it is not).
            log::warn!(
                "typeahead hand-off: a key reached the hand-off while a queued young hold was live; holding it"
            );
            self.young_hold_started = true;
            self.log_route_decision("held_young_hold_live", bytes);
            return UserBytesRoute::Held;
        }
        if self.prompt_hold_queued {
            // The same for a prompt hold: the key is held behind the hold's gate, never written
            // ahead of the keys the gate holds (#1422).
            log::warn!(
                "typeahead hand-off: a key reached the hand-off while a queued prompt hold was live; holding it"
            );
            self.prompt_hold_started = true;
            self.log_route_decision("held_prompt_hold_live", bytes);
            return UserBytesRoute::Held;
        }
        if self.should_hold_for_prompt(reaches_shell_line, Instant::now()) {
            // #1353 phase 5: the bytes are not stored here; the view puts this key at the front of
            // its queue behind a `LineEditorActive` gate (one entry per key event, in typed
            // order) and releases them by the line editor's signal.
            self.prompt_hold_queued = true;
            self.prompt_hold_id = self.prompt_hold_id.wrapping_add(1);
            self.prompt_hold_started = true;
            let now = Instant::now();
            self.prompt_hold_started_at = Some(now);
            self.last_line_bytes_at = Some(now);
            self.log_route_decision("held_prompt_hold_started", bytes);
            return UserBytesRoute::Held;
        }
        let (young_evidence, sampled_fact) =
            self.should_hold_young_enter_sampled(bytes, reaches_shell_line, Instant::now());
        if let Some(evidence) = young_evidence {
            self.young_hold_evidence = Some(evidence);
            self.young_hold_id = self.young_hold_id.wrapping_add(1);
            self.young_hold_started = true;
            // #1353 phase 5b: the bytes are not stored here; the view puts this key at the front
            // of its queue behind a `PromptSeen` gate and every key typed after it queues behind
            // that.
            self.young_hold_queued = true;
            // #1614: `RequestDue` with diverged counts means the sample proved them stale (the
            // shell's line editor reads with every Precmd applied): corrected here, so the
            // request decision the view asks for next finds no line queued.
            if evidence == YoungHoldEvidence::RequestDue {
                self.correct_stale_counts("young_enter_at_line_editor");
            }
            self.log_route_decision("held_young_enter_started", bytes);
            return UserBytesRoute::Held;
        }
        if self.enter_delivered_past_an_unsampled_group(bytes, reaches_shell_line, sampled_fact) {
            self.note_unsampled_enter_delivered();
        }
        self.log_route_decision("unheld", bytes);
        self.route_user_bytes_unheld(bytes, reaches_shell_line)
    }

    /// #1615: whether `bytes` are an Enter, typed with a line unanswered and user bytes in the
    /// shell's line, that is delivered only because the process group in front is not the sampled
    /// shell (`Unknown` from an installed probe, prompt-sequence path): the case a remote
    /// Precmd/Preexec-driven hold would decide. Read before the write is counted. `sampled_fact` is
    /// the fact the hold decision sampled for this Enter; the probe is never run again here, so the
    /// log names the fact the decision used.
    fn enter_delivered_past_an_unsampled_group(
        &self,
        bytes: &[u8],
        reaches_shell_line: bool,
        sampled_fact: Option<ShellPromptFact>,
    ) -> bool {
        let HandoffState::PtyOwned {
            wrote_user_bytes: true,
            enters_written,
            precmds_seen,
        } = self.state
        else {
            return false;
        };
        reaches_shell_line
            && matches!(bytes.first(), Some(b'\r' | b'\n'))
            && precmds_seen < enters_written
            && enters_written <= precmds_seen.saturating_add(1)
            && self.prompt_seq_mode()
            && self.foreground_probe.samples_shell()
            && sampled_fact == Some(ShellPromptFact::Unknown)
    }

    /// #1615: records and logs an Enter delivered past an unsampled group (see
    /// [`UnsampledEnterDelivery`]), with what the ordered stream said at that moment.
    fn note_unsampled_enter_delivered(&mut self) {
        let now = Instant::now();
        warpui::input_order::log_hop("unsampled_enter_delivered", || {
            format!(
                "shell_activity={:?} last_hook={:?} markers_live={} command_interactive={} \
                 since_activity_ms={} rtt_p90_ms={}",
                self.shell_activity,
                self.last_parsed_hook,
                self.line_editor_markers_live,
                self.command_interactive,
                self.last_activity_at.map_or_else(
                    || "none".to_string(),
                    |at| now.saturating_duration_since(at).as_millis().to_string()
                ),
                self.rtt_p90()
                    .map_or_else(|| "none".to_string(), |p90| p90.as_millis().to_string()),
            )
        });
        self.unsampled_enter_delivery = Some(UnsampledEnterDelivery {
            at: now,
            precmd_after: None,
        });
    }

    /// #1615: a Precmd was parsed; follows an Enter delivered past an unsampled group (model
    /// thread, from [`Self::on_precmd`]). The first Precmd after it is noted; a second one with no
    /// Preexec between means the delivered line never ran as a shell command.
    fn follow_unsampled_enter_on_precmd(&mut self, now: Instant) {
        let Some(delivery) = self.unsampled_enter_delivery.as_mut() else {
            return;
        };
        let delivered_ms = now.saturating_duration_since(delivery.at).as_millis();
        if delivery.precmd_after.is_none() {
            delivery.precmd_after = Some(now);
            warpui::input_order::log_hop("unsampled_enter_then_precmd", || {
                format!("delivered_to_precmd_ms={delivered_ms}")
            });
            return;
        }
        self.unsampled_enter_delivery = None;
        warpui::input_order::log_hop("unsampled_enter_settled", || {
            format!("outcome=no_command delivered_to_precmd_ms={delivered_ms}")
        });
    }

    /// #1615: a Preexec was parsed (model thread, from [`Self::on_preexec`]). After the Precmd
    /// that followed a delivered Enter, it is the next prompt's first command: with
    /// `shell_started` the shell read that line from the tty itself, i.e. the delivered line ran
    /// as a shell command, unclassified (the remote #933 race when `delivered_to_precmd_ms` is
    /// within a round trip; plain typeahead for the next prompt when it is long).
    fn follow_unsampled_enter_on_preexec(&mut self, shell_started: bool, now: Instant) {
        let Some(delivery) = self
            .unsampled_enter_delivery
            .take_if(|delivery| delivery.precmd_after.is_some())
        else {
            return;
        };
        let to_precmd_ms = delivery.precmd_after.map_or(0, |at| {
            at.saturating_duration_since(delivery.at).as_millis()
        });
        let to_preexec_ms = now.saturating_duration_since(delivery.at).as_millis();
        warpui::input_order::log_hop("unsampled_enter_settled", || {
            format!(
                "outcome={} delivered_to_precmd_ms={to_precmd_ms} \
                 delivered_to_preexec_ms={to_preexec_ms} rtt_p90_ms={}",
                if shell_started {
                    "ran_as_shell_command"
                } else {
                    "app_command"
                },
                self.rtt_p90()
                    .map_or_else(|| "none".to_string(), |p90| p90.as_millis().to_string()),
            )
        });
    }

    /// #1615: drops the follow of a delivered Enter, logging why (`editor_command`: the editor
    /// wrote the next command; `new_session`: a new shell took the pty over). Nothing when none
    /// is followed.
    fn settle_unsampled_enter(&mut self, outcome: &'static str) {
        let Some(delivery) = self.unsampled_enter_delivery.take() else {
            return;
        };
        let since_ms = Instant::now()
            .saturating_duration_since(delivery.at)
            .as_millis();
        warpui::input_order::log_hop("unsampled_enter_settled", || {
            format!(
                "outcome={outcome} precmd_seen={} delivered_ms_ago={since_ms}",
                delivery.precmd_after.is_some()
            )
        });
    }

    /// #1615: whether an Enter delivered past an unsampled group is being followed, for tests.
    #[cfg(test)]
    pub(crate) fn follows_unsampled_enter(&self) -> bool {
        self.unsampled_enter_delivery.is_some()
    }

    /// Logs why a user write was held or let through (#1490): the reason, the hand-off state and
    /// whether the shell's line-editor signal is still awaited for the current prompt. The bytes
    /// are shown only for synthetic test text (`WARP_INPUT_ORDER_LOG_TEXT=1`).
    fn log_route_decision(&self, reason: &str, bytes: &[u8]) {
        warpui::input_order::log_hop("hand_route", || {
            let state = match &self.state {
                HandoffState::PtyOwned { .. } => "pty_owned",
                HandoffState::HoldAfterEnter { .. } => "hold_after_enter",
                HandoffState::AwaitingReport => "awaiting_report",
                HandoffState::EditorOwned => "editor_owned",
            };
            let (wrote, enters, precmds) = match &self.state {
                HandoffState::PtyOwned {
                    wrote_user_bytes,
                    enters_written,
                    precmds_seen,
                } => (*wrote_user_bytes, *enters_written, *precmds_seen),
                _ => (false, 0, 0),
            };
            format!(
                "reason={reason} state={state} awaiting_ready={} in_flight={} evidence={:?} \
                 command_interactive={} enters_written={enters} precmds_seen={precmds} \
                 young_line={} wrote_user_bytes={wrote} prompt_seq_mode={} prompt_fact={:?} \
                 hold_id={} bytes={}",
                self.awaiting_line_editor_since.is_some(),
                self.writes_in_flight,
                self.young_hold_evidence,
                self.command_interactive,
                self.young_line,
                self.prompt_seq_mode(),
                self.sample_prompt_fact(),
                self.young_hold_id,
                warpui::input_order::describe_bytes_escaped(bytes)
            )
        });
    }

    /// What [`Self::on_user_bytes`] would answer for `bytes` right now, WITHOUT doing any of what it
    /// does (#1424): no hold is started or dropped, nothing is counted, no id moves. For the
    /// destination table's shadow mode and its tests; the pty reader thread advances this state,
    /// so a peek can differ from the commit that follows it, and the shadow counts the cases.
    /// `now` is given, never read.
    pub fn peek_user_bytes_route(
        &self,
        bytes: &[u8],
        reaches_shell_line: bool,
        now: Instant,
    ) -> UserBytesRoute {
        if bytes.iter().any(is_signal_byte) {
            return self.peek_user_bytes_unheld();
        }
        // `drop_stale_hold`: a queued young hold only lives in these two states.
        let state_keeps_hold = matches!(
            self.state,
            HandoffState::PtyOwned { .. } | HandoffState::AwaitingReport
        );
        if (self.young_hold_queued && state_keeps_hold)
            || self.prompt_hold_queued
            || self.should_hold_for_prompt(reaches_shell_line, now)
            || self
                .should_hold_young_enter(bytes, reaches_shell_line, now)
                .is_some()
        {
            return UserBytesRoute::Held;
        }
        self.peek_user_bytes_unheld()
    }

    /// The answer of `route_user_bytes_unheld`, without its bookkeeping.
    fn peek_user_bytes_unheld(&self) -> UserBytesRoute {
        match &self.state {
            HandoffState::PtyOwned { .. } => UserBytesRoute::WriteCounted,
            HandoffState::HoldAfterEnter { .. } if self.deferred_gate.is_some() => {
                UserBytesRoute::Held
            }
            HandoffState::HoldAfterEnter { .. }
            | HandoffState::EditorOwned
            | HandoffState::AwaitingReport => UserBytesRoute::Write,
        }
    }

    /// A hold only lives in `PtyOwned` and `AwaitingReport`; any other state has run, flushed
    /// or dropped it.
    fn drop_stale_hold(&mut self) {
        if !matches!(
            self.state,
            HandoffState::PtyOwned { .. } | HandoffState::AwaitingReport
        ) {
            self.drop_young_hold();
        }
    }

    /// Whether `bytes` are an Enter the user typed on a line that has content, while the only
    /// line the shell has not answered is the command that was just written, and the shell may be
    /// back at its prompt without the app having noticed (see [`YOUNG_ENTER_WINDOW`]). `Some`
    /// holds it, on the evidence named ([`YoungHoldEvidence`]).
    ///
    /// #1614: the hold engages on positive evidence only; see [`Self::young_evidence`]. The
    /// preconditions here are the state's: the pty owns input with user bytes in the shell's line,
    /// the key is an Enter for the shell's line, and at most one line is unanswered.
    fn should_hold_young_enter(
        &self,
        bytes: &[u8],
        reaches_shell_line: bool,
        now: Instant,
    ) -> Option<YoungHoldEvidence> {
        self.should_hold_young_enter_sampled(bytes, reaches_shell_line, now)
            .0
    }

    /// [`Self::should_hold_young_enter`], also returning the [`ShellPromptFact`] the decision
    /// sampled (`None` when it decided without sampling).
    fn should_hold_young_enter_sampled(
        &self,
        bytes: &[u8],
        reaches_shell_line: bool,
        now: Instant,
    ) -> (Option<YoungHoldEvidence>, Option<ShellPromptFact>) {
        let HandoffState::PtyOwned {
            wrote_user_bytes: true,
            enters_written,
            precmds_seen,
        } = self.state
        else {
            return (None, None);
        };
        // No line queued behind the one just written: the shell is running it, or is back at its
        // prompt (`precmds_seen` caught up) with the line still waiting for the request that
        // hands it to the editor.
        if !(reaches_shell_line
            && enters_written <= precmds_seen.saturating_add(1)
            && matches!(bytes.first(), Some(b'\r' | b'\n')))
        {
            return (None, None);
        }
        self.young_evidence_sampled(enters_written, precmds_seen, now)
    }

    /// The evidence a young hold would stand on right now (#1614), from the facts in the order
    /// they must be read (the platform sample first, the reader-side barrier last), or `None`: the
    /// Enter goes to the pty as a plain terminal would deliver it. Pure; also the re-evaluation
    /// the barrier's publish runs for a hold waiting on it ([`Self::on_stream_applied`]).
    ///
    /// * The block latch said the running command is interactive, or the sample says a plain
    ///   program or a password read owns the tty: the Enter is that program's. Delivered.
    /// * The sample says the shell's own line editor reads (`LineEditorReading`):
    ///   - the counts agree the prompt is back: the request is due
    ///     ([`YoungHoldEvidence::RequestDue`]), unless they agree only through a continuation
    ///     prompt inside a running command ([`Self::continuation_inside_running_command`]), which
    ///     the barrier decides as below;
    ///   - the counts queue a line but the barrier says the stream is applied: either the raw tty
    ///     is a running command's read (last hook a Preexec, no marker since, #1392: delivered) or
    ///     the counts are stale and the request is due;
    ///   - the barrier says something is unapplied: a Precmd may be in flight
    ///     ([`YoungHoldEvidence::PrecmdInFlight`]);
    ///   - no barrier for this shell (fish): the window, as before.
    /// * The sample says `Unknown`:
    ///   - the counts agree the prompt is back and the stream does not say a command started
    ///     since (#1390): the request is due (the line editor is about to be active);
    ///   - from an installed probe (the shell's group is not in front with a raw tty: an inner
    ///     shell, an ssh, a `sudo -i`, a cooked builtin): nothing says a prompt is coming.
    ///     Delivered;
    ///   - from no probe at all (ConPTY, no sample): the window, as before.
    /// * Without prompt sequence numbers (an older bootstrap, the kill switch): the window and the
    ///   plain-program check, as before.
    fn young_evidence(
        &self,
        enters_written: u32,
        precmds_seen: u32,
        now: Instant,
    ) -> Option<YoungHoldEvidence> {
        self.young_evidence_sampled(enters_written, precmds_seen, now)
            .0
    }

    /// [`Self::young_evidence`], also returning the [`ShellPromptFact`] it sampled (`None` when it
    /// decided before sampling), so a caller that logs about the same Enter (#1615) reads the very
    /// fact the decision used instead of running the probe a second time.
    fn young_evidence_sampled(
        &self,
        enters_written: u32,
        precmds_seen: u32,
        now: Instant,
    ) -> (Option<YoungHoldEvidence>, Option<ShellPromptFact>) {
        // A command the block latch calls interactive reads the keys typed into it (a program, a
        // blocked `read x`, an `ssh`, an inner shell): the Enter is its -- where nothing can be
        // sampled (ConPTY). With a sample, the sample decides: the latch outlives the command by
        // the length of the pipeline (set until its Precmd is parsed), which is #933's moment.
        if self.command_interactive && !self.foreground_probe.samples_shell() {
            return (None, None);
        }
        let in_window = || {
            self.young_line
                || self
                    .last_activity_at
                    .is_some_and(|at| now.saturating_duration_since(at) < YOUNG_ENTER_WINDOW)
        };
        let window = || in_window().then_some(YoungHoldEvidence::TimingWindow);
        if !self.prompt_seq_mode() {
            return (
                (in_window() && !self.foreground_probe.foreground_is_plain_program())
                    .then_some(YoungHoldEvidence::TimingWindow),
                None,
            );
        }
        let counts_say_prompt = precmds_seen >= enters_written;
        let fact = self.foreground_probe.prompt_fact();
        let evidence = match fact {
            // A plain program in the foreground means the shell is waiting on it, not at a
            // prompt: the Enter is for that program, not a line the app has yet to notice.
            ShellPromptFact::NotAtPrompt => None,
            // A continuation prompt the counts took as the answer to a line, shown after a
            // Preexec with no Precmd since (zle inside a running `vared`): the block runs, so no
            // request can be sent before it ends and "the request is due" is no evidence. The
            // barrier below decides, as for the running command's other reads.
            ShellPromptFact::LineEditorReading
                if counts_say_prompt && !self.continuation_inside_running_command() =>
            {
                Some(YoungHoldEvidence::RequestDue)
            }
            // The counts queue a line, the shell's own editor reads: the barrier, asked last and
            // once, tells "the Precmd that answers it is still in the stream" from "it never
            // comes".
            ShellPromptFact::LineEditorReading => match self.foreground_probe.stream_drained() {
                Some(false) => Some(YoungHoldEvidence::PrecmdInFlight),
                // #1392: the raw tty is a command's read (`read -k`, `read -n1`, zle inside
                // `vared`), not the line editor: the stream is applied up to a moment after the
                // sample (a shell writes its prompt's Precmd before its line editor makes the tty
                // raw, so that Precmd would have been applied) and the last hook parsed is a
                // Preexec, so a command runs and no prompt is coming for this Enter. It is
                // written to the pty, where that read takes it -- what the hold's give-up would
                // have done 2 s later. Every condition is an ordered fact; none is a time.
                Some(true) if self.raw_tty_is_a_running_commands_read() => None,
                // The written command's Preexec has not been parsed: the editor sampled reading
                // may be the one that has not read the command yet (the input direction is not
                // the barrier's), so nothing is proven stale; the window, released by the Precmd.
                Some(true) if self.own_preexec_pending => window(),
                Some(true) => Some(YoungHoldEvidence::RequestDue),
                // No barrier for this shell (fish): held for the prompt as before #1614, the
                // Precmd's request and report releasing it (owner decision 2026-11-03).
                None => Some(YoungHoldEvidence::PrecmdMaybeInFlight),
            },
            // #1614: the shell's own group is in front with a cooked tty: between its Precmd and
            // its line editor (`PROMPT_COMMAND`, PS1 substitutions), or a cooked builtin. The
            // counts say the prompt is back: the request is due once the editor is active.
            ShellPromptFact::ShellCooked
                if counts_say_prompt && !self.stream_says_command_running() =>
            {
                Some(YoungHoldEvidence::RequestDue)
            }
            // The written command's own Preexec has not been parsed: the cooked shell in front may
            // be the one about to read it (as for the line editor above); the window, released by
            // the command's Precmd.
            ShellPromptFact::ShellCooked if self.own_preexec_pending && !counts_say_prompt => {
                window()
            }
            // A line unanswered and the stream not applied up to now: its Precmd may be in it;
            // the barrier's next publish decides. Applied (or no barrier), nothing says a prompt
            // is coming: delivered, as to a cooked `read x`.
            ShellPromptFact::ShellCooked => match self.foreground_probe.stream_drained() {
                Some(false) if !counts_say_prompt => Some(YoungHoldEvidence::PrecmdInFlight),
                _ => None,
            },
            // #1390: with no sample and a shell that sends line-editor markers, the ordered
            // stream says what the shell is doing. A Preexec parsed after the last Precmd is
            // newer than the counts that said the prompt was back: a command runs, so the Enter
            // is for it.
            ShellPromptFact::Unknown if self.stream_says_command_running() => {
                if counts_say_prompt || self.foreground_probe.samples_shell() {
                    None
                } else {
                    window()
                }
            }
            ShellPromptFact::Unknown if counts_say_prompt => Some(YoungHoldEvidence::RequestDue),
            // An installed probe that sees another process group in front: a login hop (`ssh`,
            // `sudo -i`) or an inner shell. No prompt of the hooked local shell is in flight; a
            // plain terminal delivers. A warpified remote session's own Precmd/Preexec stream
            // could drive a hold here; accepted as delivered for now (owner decision 2026-11-03;
            // #1614 follow-up: remote Precmd/Preexec-driven hold). Until it is decided the
            // delivery is measured: `route_user_bytes` follows it through the next hooks
            // (`unsampled_enter_*` hops, #1615).
            ShellPromptFact::Unknown if self.foreground_probe.samples_shell() => None,
            ShellPromptFact::Unknown => window(),
        };
        (evidence, Some(fact))
    }

    /// Whether the shell's line editor shows a continuation prompt inside a running command: the
    /// last marker is a continuation prompt ([`ShellActivity::Continuation`]) and the last hook
    /// parsed is a Preexec with no Precmd since ([`Self::raw_tty_is_a_running_commands_read`]),
    /// so the block lifecycle is not at a prompt. Its re-baselining of the counts
    /// ([`Self::on_continuation_prompt`]) then says nothing about a prompt being back.
    fn continuation_inside_running_command(&self) -> bool {
        self.shell_activity == ShellActivity::Continuation
            && self.raw_tty_is_a_running_commands_read()
    }

    /// Whether this session's shell sends line-editor markers and the last thing its ordered
    /// stream said is that a command started ([`ShellActivity::Running`], a Preexec parsed after
    /// the last Precmd). Never true for a shell without markers (the window path stays).
    fn stream_says_command_running(&self) -> bool {
        self.line_editor_markers_live && self.shell_activity == ShellActivity::Running
    }

    /// Whether the first byte of a line typed now began before the app could know the shell was at
    /// its prompt (#933): `window` is the legacy answer, from elapsed time since the last activity.
    /// With the prompt-sequence path the sampled shell state and the line counts decide instead.
    fn first_byte_is_young(&self, window: bool) -> bool {
        if !self.prompt_seq_mode() {
            return window;
        }
        let Owner::Pty {
            enters_written,
            precmds_seen,
            ..
        } = self.owner()
        else {
            return window;
        };
        match self.foreground_probe.prompt_fact() {
            ShellPromptFact::NotAtPrompt => false,
            ShellPromptFact::LineEditorReading => true,
            // #1390: see `should_hold_young_enter`; a Preexec newer than the counts means the
            // line is typed into a running command, not at a prompt.
            ShellPromptFact::Unknown | ShellPromptFact::ShellCooked
                if self.stream_says_command_running() =>
            {
                precmds_seen < enters_written && window
            }
            ShellPromptFact::Unknown | ShellPromptFact::ShellCooked
                if precmds_seen >= enters_written =>
            {
                true
            }
            ShellPromptFact::Unknown | ShellPromptFact::ShellCooked => window,
        }
    }

    /// Makes the last command look older than [`YOUNG_ENTER_WINDOW`], for tests that model
    /// typeahead for a command that has been running a while.
    #[cfg(test)]
    pub(crate) fn age_last_activity_for_test(&mut self) {
        self.last_activity_at = None;
    }

    /// Whether the Enter deferred behind the report has not run yet: the keys typed since are
    /// held behind it. False once it ran, or was cancelled.
    pub fn deferred_submit_pending(&self) -> bool {
        matches!(
            self.state,
            HandoffState::HoldAfterEnter { released: true, .. }
        )
    }

    /// Whether an Enter pressed in the editor is deferred and the keys typed since are held
    /// behind it (#1016). False once the report arrived and the deferred submit runs: that
    /// submit is itself an Enter reaching the editor, and must not be held again.
    pub fn holds_keys_behind_deferred_enter(&self) -> bool {
        matches!(
            self.state,
            HandoffState::HoldAfterEnter {
                released: false,
                ..
            }
        )
    }

    /// Whether an Enter is held, for tests.
    #[cfg(test)]
    pub(crate) fn has_young_hold(&self) -> bool {
        self.young_hold_queued
    }

    /// Takes the "a hold was just created" flag, returning the hold's id so the view queues the
    /// key that started it behind the hold's gate once.
    pub fn take_young_hold_started(&mut self) -> Option<u64> {
        mem::take(&mut self.young_hold_started).then_some(self.young_hold_id)
    }

    /// A signal or EOF is about to be written while the keys of a young hold are in the view's
    /// queue: the hold ends here, synchronously, and the view takes the gate's entries out. The
    /// flag says whether they go out in front of `bytes` (a lone EOF: the tty keeps typeahead
    /// across it, so the held Enter must reach the program before it, #933) or are void (^C, ^Z
    /// and ^\\ flush the tty's typeahead). `None` when no queued hold is live.
    pub fn take_queued_young_hold_for_signal(&mut self, bytes: &[u8]) -> Option<(u64, bool)> {
        if !self.young_hold_queued || !bytes.iter().any(is_signal_byte) {
            return None;
        }
        let before_eof = bytes.contains(&0x04)
            && !bytes.iter().any(|b| matches!(b, 0x03 | 0x1a | 0x1c))
            && matches!(self.state, HandoffState::PtyOwned { .. });
        self.young_hold_queued = false;
        Some((self.young_hold_id, before_eof))
    }

    /// The give-up cap of the queued young hold `id` ran out (the window of
    /// [`Self::young_hold_wait`], or [`EVENT_HOLD_GIVE_UP`] when an event was to release it).
    /// [`YoungHoldExpiry::Release`] ends the hold: the caller writes its keys through
    /// [`Self::route_expired_young_chunks`].
    pub fn expire_queued_young_hold(&mut self, id: u64) -> YoungHoldExpiry {
        self.drop_stale_hold();
        if !self.young_hold_queued || id != self.young_hold_id {
            return YoungHoldExpiry::Gone;
        }
        if !matches!(self.state, HandoffState::PtyOwned { .. }) {
            return YoungHoldExpiry::Keep;
        }
        self.young_hold_queued = false;
        YoungHoldExpiry::Release
    }

    /// #1577: the give-up of queued young hold `id` handed its keys back to the editor instead of
    /// writing them: the hold is over, nothing is routed and no byte is counted as written. Clears
    /// the queued flag if it is still set for `id` (the expiry already did when it said `Release`).
    pub fn finish_young_hold_handed_back(&mut self, id: u64) {
        if id != self.young_hold_id {
            return;
        }
        self.young_hold_queued = false;
        self.young_hold_evidence = None;
        // #1614: the hold's premise failed (the net fired), so the line the keys were typed on is
        // not a line at an unnoticed prompt; left set, `young_line` made every later Enter on the
        // same shell line "in the window" and the net fired on each (28 per pass on macOS r18e4).
        self.young_line = false;
    }

    /// #1614: the pty reader applied another slice of the stream (model thread, right after the
    /// barrier's publish). A young hold waiting on the barrier ([`YoungHoldEvidence::PrecmdInFlight`])
    /// is decided again once the stream is applied up to now, with the facts as they are: the
    /// counts caught up (the slice held the Precmd) or the sample proves them stale, so the
    /// request is due; or nothing says the prompt is back, so the keys are typeahead for what
    /// owns the tty. The sample is read before the barrier, as at the Enter.
    pub fn on_stream_applied(&mut self) -> StreamApplied {
        if !self.young_hold_queued
            || self.young_hold_evidence != Some(YoungHoldEvidence::PrecmdInFlight)
        {
            return StreamApplied::Nothing;
        }
        let HandoffState::PtyOwned {
            enters_written,
            precmds_seen,
            ..
        } = self.state
        else {
            return StreamApplied::Nothing;
        };
        match self.young_evidence(enters_written, precmds_seen, Instant::now()) {
            // Still something unapplied: the next publish asks again.
            Some(YoungHoldEvidence::PrecmdInFlight) => StreamApplied::Nothing,
            Some(YoungHoldEvidence::RequestDue) => {
                self.young_hold_evidence = Some(YoungHoldEvidence::RequestDue);
                self.correct_stale_counts("stream_applied");
                warpui::input_order::log_hop("young_hold_reevaluated", || {
                    format!("outcome=request_due hold_id={}", self.young_hold_id)
                });
                StreamApplied::RetryRequest
            }
            // The written command's own Preexec is still to come (window), or the barrier is gone
            // from the probe (no barrier: the Precmd decides): the hold keeps waiting for that
            // Preexec (which does not end it) and the Precmd that makes the request due.
            Some(YoungHoldEvidence::TimingWindow | YoungHoldEvidence::PrecmdMaybeInFlight) => {
                StreamApplied::Nothing
            }
            None => {
                self.end_young_hold_as_typeahead();
                warpui::input_order::log_hop("young_hold_reevaluated", || {
                    format!("outcome=typeahead hold_id={}", self.young_hold_id)
                });
                StreamApplied::HoldEnded
            }
        }
    }

    /// #1614: the counts say a line is queued that the shell's state proves answered: they are
    /// corrected to agree (the same correction the Precmd of an interactive command and the 5 s
    /// give-up make), logged with the evidence that proved it.
    fn correct_stale_counts(&mut self, by: &'static str) {
        if let HandoffState::PtyOwned {
            enters_written,
            precmds_seen,
            ..
        } = &mut self.state
            && *precmds_seen < *enters_written
        {
            warpui::input_order::log_hop("handoff_stale_counts_reset", || {
                format!("by={by} enters={enters_written} precmds={precmds_seen}")
            });
            *precmds_seen = *enters_written;
        }
    }

    /// #1614: whether the counts queue a line the shell's sampled state proves answered: the
    /// shell's own group is in front with a raw tty (its line editor reads), the stream is applied
    /// up to a moment after that sample (so every Precmd the shell wrote is counted), and the raw
    /// tty is not a running command's read ([`Self::raw_tty_is_a_running_commands_read`]). The
    /// barrier is asked last, after the sample.
    fn counts_provably_stale(&self, enters_written: u32, precmds_seen: u32) -> bool {
        precmds_seen < enters_written
            && !self.own_preexec_pending
            && self.foreground_probe.prompt_fact() == ShellPromptFact::LineEditorReading
            && !self.raw_tty_is_a_running_commands_read()
            && self.foreground_probe.stream_drained() == Some(true)
    }

    /// Whether, with the stream applied up to now, the last thing the shell's hooks said is that
    /// a command started: the last hook parsed is a Preexec, and no Precmd since (it would have
    /// been applied). The block is running, so the raw tty is that command's read -- a builtin
    /// `read -k` (#1392), or zle inside `vared`, which sends the line-editor marker too; either
    /// way no request can be sent before the command ends (the lifecycle is not at a prompt), so
    /// a marker after the Preexec does not make the editor the reader of a prompt line.
    fn raw_tty_is_a_running_commands_read(&self) -> bool {
        self.last_parsed_hook == Some(ParsedShellHook::Preexec)
    }

    /// Routes the keys of an expired queued hold (one chunk per key event, in typed order) and
    /// returns the writes to make: one chunk per key event on a ConPTY session (#1162), one write
    /// elsewhere, exactly what the buffered hold's expiry returns.
    pub fn route_expired_young_chunks(&mut self, chunks: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
        let chunks = if self.conpty_input || chunks.len() <= 1 {
            chunks
        } else {
            vec![chunks.concat()]
        };
        for chunk in &chunks {
            self.route_user_bytes_unheld(chunk, true);
        }
        chunks
    }

    fn route_user_bytes_unheld(
        &mut self,
        bytes: &[u8],
        reaches_shell_line: bool,
    ) -> UserBytesRoute {
        // Whether this key begins a line typed before the app could know the shell was at its
        // prompt (#933); sampled before the state is borrowed.
        let opens_enter_window = reaches_shell_line
            && bytes.iter().any(|byte| matches!(byte, b'\r' | b'\n'))
            && matches!(self.state, HandoffState::PtyOwned { .. })
            && self.enter_window_may_open();
        let first_byte_young = reaches_shell_line
            && !matches!(bytes.first(), Some(b'\r' | b'\n'))
            && matches!(
                self.state,
                HandoffState::PtyOwned {
                    wrote_user_bytes: false,
                    ..
                }
            )
            && self.first_byte_is_young(self.last_activity_at.is_some_and(|at| {
                Instant::now().saturating_duration_since(at) < YOUNG_ENTER_WINDOW
            }));
        let ctrl_u_kills = !self.ctrl_u_not_line_kill;
        match &mut self.state {
            HandoffState::PtyOwned {
                wrote_user_bytes,
                enters_written,
                ..
            } => {
                let mut wrote_an_enter = false;
                if reaches_shell_line {
                    if first_byte_young {
                        self.young_line = true;
                    }
                    self.last_line_bytes_at = Some(Instant::now());
                    let (enters, dirty) =
                        scan_line_terminators(bytes, *wrote_user_bytes, ctrl_u_kills);
                    *enters_written = enters_written.saturating_add(enters);
                    *wrote_user_bytes = dirty;
                    wrote_an_enter = enters > 0;
                }
                // An Enter is activity too: a quiet-period reset armed before it must not
                // treat the line it ends as answered (a password typed after the timer was
                // armed could otherwise land in the editor).
                if wrote_an_enter {
                    self.activity_generation = self.activity_generation.wrapping_add(1);
                    // #1574: the Enter submits the line delivered bytes were in; it is not
                    // reported.
                    self.committed_delivery = false;
                    // #1490: an Enter at the shell's line editor opens the window in which the
                    // shell is not reading; keys typed now are held until it is again.
                    if opens_enter_window {
                        self.enter_window_since = Some(Instant::now());
                    }
                }
                self.writes_in_flight = self.writes_in_flight.saturating_add(1);
                UserBytesRoute::WriteCounted
            }
            // The keys typed behind a deferred Enter are entries behind its gate; the view's
            // queue takes every one of them (the gate is open from the moment the Enter is
            // deferred), so a key only gets here if the queue was idle or mid-delivery. The view
            // puts it behind the gate; it is never held here.
            HandoffState::HoldAfterEnter { .. } if self.deferred_gate.is_some() => {
                self.deferred_key_started = true;
                if reaches_shell_line {
                    self.last_line_bytes_at = Some(Instant::now());
                }
                UserBytesRoute::Held
            }
            HandoffState::HoldAfterEnter { .. } => {
                log::warn!("typeahead hand-off: a key reached a deferred Enter with no gate");
                UserBytesRoute::Write
            }
            // In `AwaitingReport` such a write is a key typed into a command drained behind the
            // outstanding request (a queued command written right after it): the tty holds the
            // request ahead of the key, so the report cannot miss anything typed before it, and
            // the key reaches that command, or the shell's line at that command's prompt, after
            // the report. It is not adopted from here: the editor may already hold keys typed
            // after the request, and a second report would land in front of them, out of typed
            // order. The report hands input to the editor (`on_report`), and from then on keys
            // typed into the running command are adopted (#1089). The keys of that window (the
            // report's latency) stay where they went and come back with the next prompt's legacy
            // report; until that report the editor inserts literally (the view notes them with
            // `note_untracked_line_bytes`).
            HandoffState::EditorOwned | HandoffState::AwaitingReport => UserBytesRoute::Write,
        }
    }

    /// `PtyController` processed a counted user write. Returns how many are still in flight.
    pub fn on_user_write_flushed(&mut self) -> u32 {
        self.writes_in_flight = self.writes_in_flight.saturating_sub(1);
        self.writes_in_flight
    }

    /// No precmd or preexec arrived for a while after a [`ActiveDecision::Wait`]: a line the
    /// counts still consider queued was consumed by something other than the shell (a program
    /// reading stdin, an empty or continued line). Trust the quiet and treat every line as
    /// answered. The in-flight count is left alone: those writes are real. Returns whether the
    /// counts were reset.
    pub fn reset_stale_counts(&mut self, activity_generation: u64) -> bool {
        if activity_generation != self.activity_generation {
            return false;
        }
        match &mut self.state {
            HandoffState::PtyOwned {
                enters_written,
                precmds_seen,
                ..
            } => {
                *precmds_seen = *enters_written;
                true
            }
            HandoffState::EditorOwned
            | HandoffState::AwaitingReport
            | HandoffState::HoldAfterEnter { .. } => false,
        }
    }

    /// Enter was pressed in the editor. Returns whether the submit must be deferred because a
    /// report is outstanding; keys typed from now on are held.
    pub fn defer_enter(&mut self) -> bool {
        // #1104: also while the editor owns input but a request is outstanding that does not
        // cover untracked bytes in the shell's line: submitting now would write the kill prefix
        // and the command behind that request, the line would run without its head, and the
        // report would land in the fresh editor as a fragment.
        let behind_uncovered_report = self.state == HandoffState::EditorOwned
            && self.untracked_line_bytes
            && self.requests_outstanding > 0;
        if self.state != HandoffState::AwaitingReport && !behind_uncovered_report {
            return false;
        }
        if behind_uncovered_report {
            self.arm_uncovered_deferral();
            self.report_deferral_id = None;
        } else {
            self.uncovered_deferral_id = None;
            self.arm_report_deferral();
        }
        self.deferred_young = self.young_line;
        self.deferred_gate = Some(self.next_gate_id());
        self.state = HandoffState::HoldAfterEnter { released: false };
        true
    }

    /// The next id of a deferred Enter's gate: a counter tagged with [`DEFERRED_GATE_ID_BIT`], so
    /// it never equals the id of a young hold's gate (both are recorded in one list of ends).
    fn next_gate_id(&mut self) -> u64 {
        self.deferred_gate_seq = self.deferred_gate_seq.wrapping_add(1) & !DEFERRED_GATE_ID_BIT;
        self.deferred_gate_seq | DEFERRED_GATE_ID_BIT
    }

    /// The id of the `HandoffReport` gate that holds the keys typed behind the deferred Enter,
    /// while one is deferred. The view opens it when the Enter is deferred and keys queue behind it
    /// (the hand-off keeps no bytes).
    pub fn deferred_gate(&self) -> Option<u64> {
        self.deferred_gate
    }

    /// Takes the "a key reached the hand-off while an Enter is deferred, unqueued" flag, returning
    /// the gate's id: the view puts that key behind the gate.
    pub fn take_deferred_key_started(&mut self) -> Option<u64> {
        mem::take(&mut self.deferred_key_started)
            .then_some(self.deferred_gate)
            .flatten()
    }

    /// The shell reported its line buffer.
    pub fn on_report(&mut self) -> ReportDisposition {
        self.interrupt_barrier = false;
        // #1089: a report for a request written before untracked bytes does not cover them.
        self.on_report_received();
        match self.owner() {
            Owner::Editor => ReportDisposition::InsertLegacy,
            Owner::AwaitingReport => {
                // #933: the user's Enter was held behind the request; run it now, in the editor,
                // with the keys typed after it held behind it.
                if mem::take(&mut self.young_hold_queued) {
                    // The keys are in the view's queue: the view turns the hold's gate into the
                    // deferred Enter's `HandoffReport` gate right after this returns (before
                    // anything else touches the held keys), dropping the Enter that runs now.
                    let deferred_gate = self.next_gate_id();
                    self.deferred_gate = Some(deferred_gate);
                    self.young_hold_ends.push((
                        self.young_hold_id,
                        YoungHoldEnd::SubmitOnReport { deferred_gate },
                    ));
                    self.state = HandoffState::HoldAfterEnter { released: true };
                    self.young_line = false;
                    return ReportDisposition::InsertThenSubmitYoung;
                }
                let young = mem::take(&mut self.young_line);
                self.state = HandoffState::EditorOwned;
                if young {
                    ReportDisposition::InsertYoung
                } else {
                    ReportDisposition::Insert
                }
            }
            Owner::Hold { released: false } if self.untracked_line_bytes => {
                // This report left bytes behind it in the shell's line (#1104): a request
                // written after them will report them. The Enter waits for that report.
                self.report_deferral_id = None;
                self.arm_uncovered_deferral();
                ReportDisposition::InsertKeepDeferred
            }
            Owner::Hold { released: false } => {
                self.uncovered_deferral_id = None;
                self.report_deferral_id = None;
                self.mark_released();
                if mem::take(&mut self.deferred_young) {
                    self.young_line = false;
                    ReportDisposition::InsertThenSubmitYoung
                } else {
                    ReportDisposition::InsertThenSubmit
                }
            }
            Owner::Hold { released: true } | Owner::Pty { .. } => ReportDisposition::Stash,
        }
    }

    /// The submit run after the report wrote no command: the keys held behind it are released.
    /// Returns the id of the gate that holds them (the view takes them out and sends them to the
    /// pty as typed if an Enter is among them, else back to the editor, #826 N1), or `None`
    /// unless that is the state (keys held, deferred submit already run).
    pub fn release_after_unsubmitted_enter(&mut self) -> Option<u64> {
        if self.owner() != (Owner::Hold { released: true }) {
            return None;
        }
        self.state = HandoffState::EditorOwned;
        self.deferred_gate.take()
    }

    /// The session ended: nothing can answer an outstanding request or run held keys.
    pub fn reset_on_exit(&mut self) {
        // A young hold whose keys are in the view's queue ends with the session: the view drops
        // them, and the ids keep growing so a later hold never reuses the gate's key. So do the
        // keys held behind a deferred Enter.
        self.drop_young_hold();
        if let Some(gate) = self.deferred_gate.take() {
            self.young_hold_ends.push((gate, YoungHoldEnd::Dropped));
        }
        *self = Self {
            tracks_view_precmds: self.tracks_view_precmds,
            young_hold_id: self.young_hold_id,
            deferred_gate_seq: self.deferred_gate_seq,
            young_hold_ends: mem::take(&mut self.young_hold_ends),
            queued_line_bytes: self.queued_line_bytes.clone(),
            ..Self::default()
        };
    }

    /// Ends the window opened by [`Self::note_editor_took_over`]: focus reached the editor, or
    /// the user clicked or selected on the terminal, so later keys there mean what they always
    /// did.
    pub fn end_editor_takeover_window(&mut self) {
        self.editor_took_over_at = None;
    }

    /// A key the window applied is about to move focus to the editor.
    pub fn note_focus_moved_by_takeover_key(&mut self) {
        self.focus_moved_by_takeover_key = true;
    }

    /// The editor gained focus: ends the window, unless a key the window applied moved focus
    /// there itself.
    pub fn on_editor_focused(&mut self) {
        if !mem::take(&mut self.focus_moved_by_takeover_key) {
            self.end_editor_takeover_window();
        }
    }

    /// Records that the editor just took input over (`now`), so a non-printable key that still
    /// reaches the terminal before focus follows can be applied to the editor.
    pub fn note_editor_took_over(&mut self, now: Instant) {
        self.editor_took_over_at = Some(now);
    }

    /// Whether the editor took input over and the window it opened is still open (#1439). The
    /// window ends on its events only: focus reaching the editor, a click or selection on the
    /// terminal, or the hand-off leaving the editor's ownership; how long focus takes (75-200 ms
    /// measured, longer on a busy main thread) decides nothing. `give_up` is a net for a focus
    /// event that never comes: past it the window is closed with a log line, so a key typed on
    /// the terminal much later keeps its usual meaning.
    pub fn editor_takeover_open(&mut self, give_up: Duration, now: Instant) -> bool {
        if !self.editor_owns_input() {
            return false;
        }
        let Some(at) = self.editor_took_over_at else {
            return false;
        };
        let age = now.saturating_duration_since(at);
        if age > give_up {
            log::warn!(
                "typeahead hand-off: the editor never gained focus within {age:?} of taking over; \
                 closing the takeover window"
            );
            self.editor_took_over_at = None;
            return false;
        }
        true
    }

    /// Whether the takeover window is open, read-only and clockless (#1424): the route facts
    /// cannot close the window (that is `editor_takeover_open`'s job, with its give-up net), so
    /// this reports "the editor owns input and took over at some point".
    pub fn editor_takeover_would_be_open(&self) -> bool {
        self.editor_owns_input() && self.editor_took_over_at.is_some()
    }

    /// A Ctrl-C was written while the pty owned input. The tty discards queued typeahead and the
    /// shell discards its line, so nothing the user typed survives it.
    pub fn on_interrupt(&mut self) -> InterruptOutcome {
        self.on_interrupt_for_shell(true)
    }

    /// As [`Self::on_interrupt`], knowing whether the session's shell runs a prompt cycle (a
    /// Precmd) after an interrupted line. bash, fish, PowerShell and nu do; zsh usually does not
    /// (#1353 phase 6).
    ///
    /// With the prompt-sequence path the hand-back is event driven instead of waiting a guessed
    /// time: after a cycle the Precmd hands input back (the view's prompt handling commits); with no
    /// cycle expected, a request is written behind the interrupt and the shell's report -- read in
    /// stream order after anything the interrupt made it emit -- hands it back.
    pub fn on_interrupt_for_shell(&mut self, prompt_cycle_expected: bool) -> InterruptOutcome {
        self.drop_young_hold();
        self.interrupt_barrier = false;
        self.interrupt_awaiting_cycle = false;
        match self.owner() {
            Owner::Pty {
                enters_written,
                precmds_seen,
                ..
            } => {
                let at_prompt = precmds_seen >= enters_written;
                // #1353 phase 6 (INP-4 / PTY-12): the hand-back after a Ctrl-C at the prompt is
                // not a guessed wait for a prompt cycle that some shells never run. A request is
                // written behind the interrupt and the shell's report, read in stream order
                // after whatever the interrupt made it emit, is the event that hands input back.
                let seq_mode = self.prompt_seq_mode();
                self.interrupt_barrier = at_prompt && seq_mode && !prompt_cycle_expected;
                self.interrupt_awaiting_cycle = seq_mode && !self.interrupt_barrier;
                self.state = HandoffState::PtyOwned {
                    wrote_user_bytes: false,
                    enters_written: if at_prompt {
                        precmds_seen
                    } else {
                        precmds_seen.saturating_add(1)
                    },
                    precmds_seen,
                };
                if at_prompt {
                    InterruptOutcome::AtPrompt
                } else {
                    InterruptOutcome::CommandRunning
                }
            }
            Owner::Editor | Owner::AwaitingReport | Owner::Hold { .. } => {
                InterruptOutcome::NotOwned
            }
        }
    }

    /// Ctrl-C while keys are held behind a deferred Enter cancels both: the deferred submit is
    /// dropped and the held keys with it. No command is running (the deferred one was never
    /// written), so nothing is written to the pty. Returns whether there was anything to cancel.
    pub fn cancel_deferred_enter(&mut self) -> bool {
        match self.owner() {
            // The report is still outstanding; the editor keeps waiting for it.
            Owner::Hold { released: false } => {
                self.uncovered_deferral_id = None;
                self.report_deferral_id = None;
                self.state = HandoffState::AwaitingReport;
                self.drop_deferred_gate();
                true
            }
            // The deferred submit already ran without writing a command.
            Owner::Hold { released: true } => {
                self.state = HandoffState::EditorOwned;
                self.drop_deferred_gate();
                true
            }
            // #933: an Enter held behind the request is cancelled with its keys.
            Owner::AwaitingReport => self.drop_young_hold(),
            Owner::Editor | Owner::Pty { .. } => false,
        }
    }

    /// The keys held behind a deferred Enter are void: the view drops the gate's entries
    /// ([`YoungHoldEnd::Dropped`]).
    fn drop_deferred_gate(&mut self) {
        if let Some(gate) = self.deferred_gate.take() {
            self.young_hold_ends.push((gate, YoungHoldEnd::Dropped));
        }
    }

    /// Whether Ctrl-D (EOT) should go to the pty: a command may be running or queued to read it,
    /// or the shell's line holds typed bytes. At the prompt with an empty line, EOT would exit the
    /// shell, so it is not written.
    pub fn ctrl_d_reaches_a_reader(&self) -> bool {
        match self.owner() {
            Owner::Pty {
                wrote_user_bytes,
                enters_written,
                precmds_seen,
            } => wrote_user_bytes || enters_written > precmds_seen,
            Owner::Hold { .. } => true,
            Owner::Editor | Owner::AwaitingReport => false,
        }
    }

    /// Whether the pty owns input at the prompt with user bytes in the shell's line, so only a
    /// request can hand input to the editor.
    #[cfg(test)]
    pub fn needs_request_at_prompt(&self) -> bool {
        matches!(
            self.owner(),
            Owner::Pty {
                wrote_user_bytes: true,
                enters_written,
                precmds_seen,
            } if precmds_seen >= enters_written
        )
    }

    fn mark_released(&mut self) {
        if let HandoffState::HoldAfterEnter { released, .. } = &mut self.state {
            *released = true;
        }
    }
}

/// Whether text released into the editor should force the next Enter to run as a shell command.
/// Empty text inserts nothing, so a flag set for it would stay armed and misroute a later
/// natural-language Enter to the shell.
pub fn forces_shell_submit(text: &str) -> bool {
    !text.is_empty()
}

/// The text of keys handed back to the editor, as the shell's line editor would have left it:
/// Backspace (DEL, ^H) erases a character, ^U the line, ^W a word; Enter (`\r`, `\n`) becomes
/// a newline and Tab stays, so a multi-line paste survives; well-formed escape sequences (CSI,
/// SS3, OSC, as sent by arrow and function keys) are dropped whole; any other control character
/// is dropped, and a lone ESC is dropped without taking the character after it.
pub fn text_for_editor(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        i += 1;
        match ch {
            // Backspace erases within the current line only, as the line editor would.
            '\u{7f}' | '\u{8}' => {
                if !out.ends_with('\n') {
                    out.pop();
                }
            }
            // ^U: back to the start of the line.
            '\u{15}' => {
                let line_start = out.rfind('\n').map_or(0, |index| index + 1);
                out.truncate(line_start);
            }
            // ^W: trailing whitespace, then the word before it.
            '\u{17}' => {
                while out.ends_with(|last: char| last == ' ' || last == '\t') {
                    out.pop();
                }
                while out.chars().last().is_some_and(|last| !last.is_whitespace()) {
                    out.pop();
                }
            }
            '\r' | '\n' => out.push('\n'),
            '\t' => out.push('\t'),
            '\u{1b}' => i = skip_escape_sequence(&chars, i),
            ch if ch.is_control() => {}
            ch => out.push(ch),
        }
    }
    out
}

/// Given `chars[start]` is the character after an ESC, returns the index after a well-formed
/// CSI (`ESC [` .. final byte), SS3 (`ESC O` + one character) or OSC (`ESC ]` .. BEL or ST)
/// sequence, or `start` itself when the ESC starts none of them.
fn skip_escape_sequence(chars: &[char], start: usize) -> usize {
    match chars.get(start) {
        Some('[') => {
            let mut index = start + 1;
            while let Some(ch) = chars.get(index) {
                index += 1;
                if ('@'..='~').contains(ch) {
                    return index;
                }
                if !(' '..='?').contains(ch) {
                    // Not a parameter or intermediate byte: not a CSI after all.
                    return start;
                }
            }
            start
        }
        Some('O') => match chars.get(start + 1) {
            Some(ch) if ('@'..='~').contains(ch) => start + 2,
            Some(_) | None => start,
        },
        Some(']') => {
            let mut index = start + 1;
            while let Some(ch) = chars.get(index) {
                index += 1;
                if *ch == '\u{7}' {
                    return index;
                }
                if *ch == '\u{1b}' && chars.get(index) == Some(&'\\') {
                    return index + 1;
                }
            }
            start
        }
        Some(_) | None => start,
    }
}

/// Counts line terminators in `bytes` and returns them with whether the line is still dirty
/// afterwards (bytes follow the last terminator), starting from `dirty`.
///
/// `ctrl_u_kills` is whether `^U` (0x15) kills the shell's line. It does in a tty line discipline,
/// readline, zle and fish; PSReadLine does not bind it by default, so on PowerShell a raw `^U`
/// leaves the line as dirty as before (F7, #1595).
fn scan_line_terminators(bytes: &[u8], mut dirty: bool, ctrl_u_kills: bool) -> (u32, bool) {
    let mut enters = 0u32;
    for byte in bytes {
        if matches!(byte, b'\r' | b'\n') {
            enters = enters.saturating_add(1);
            dirty = false;
        } else if *byte == 0x03 || (*byte == 0x15 && ctrl_u_kills) {
            // ^C and ^U end the line as the tty does (flush / kill): not an Enter, but the
            // line no longer holds typed text (#1562). ^U only where it is a kill (F7, #1595).
            dirty = false;
        } else {
            dirty = true;
        }
    }
    (enters, dirty)
}

#[cfg(all(test, not(any(shuttle, loom))))]
#[path = "typeahead_handoff_tests.rs"]
mod tests;

// Interleaving tests of the hand-off handle under `--cfg shuttle` / `--cfg loom`.
#[cfg(all(test, any(shuttle, loom)))]
#[path = "typeahead_handoff_model_tests.rs"]
mod model_tests;
