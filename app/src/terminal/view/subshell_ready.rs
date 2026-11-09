//! The subshell setup's readiness judge (#1679, #1711 step 3): PURE rules, so every row of the
//! redesign is a truth-table test. The view (`SubshellSetupState` in `view.rs`) samples the pane at
//! an output wakeup, the hand-off's change event or a banner click (never on a timer), feeds the
//! sample here and acts on the decision. The Opus review of the redesign wins where the two
//! differ; the rule numbers below are its (R1-R10).
//!
//! The principle: a setup line is written only on a sample that proves WHO reads it (the shell's
//! own line editor; an editor-less shell at a prompt it drew; the first new prompt behind a relay)
//! and that NOTHING the user passed through is still unread (R1: the slave input queue is empty).
//! An Enter never writes. A secret read, a question, a changed block, the user driving the shell
//! and the 60 s ceiling end the attempt instead.

use std::time::Duration;

use crate::terminal::model::subshell_tty::{
    LeaderKind, SubshellTtyKind, SubshellTtySample, leader_has_line_editor, shell_type_of_leader,
};
use crate::terminal::shell::ShellType;

/// How long an armed setup waits for a sample that proves the shell ready before it is given up
/// (owner decision 2026-11-08, Q7: 60 s). A "click Phosphorize to try again" toast follows.
// give-up-net: 60 s ceiling for a setup waiting for the reader-ready proof: gives the setup up, keys return to the editor, a 'try again' toast follows
pub const PRE_READY_CEILING: Duration = Duration::from_secs(60);

/// What the end of the active block's output looks like.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tail {
    /// A finished line, or no visible text.
    None,
    /// Unterminated visible text that is not a prompt for input: the shape of a shell's prompt.
    ShellPrompt,
    /// Unterminated text that asks the user something (`output_tail_is_input_prompt`: `:`/`?`
    /// endings, `[y/N]`, a choice group, a secret word, the zsh-newuser-install footer).
    InputPrompt,
}

/// One look at the pane while the setup is armed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadinessSample {
    pub tty: SubshellTtySample,
    /// The block the trigger was armed in is still the active one.
    pub block_unchanged: bool,
    pub alt_screen: bool,
    /// #1562: keys delivered to the shell sit in its line (`setup_write_must_wait_for_the_line`).
    pub line_dirty: bool,
    pub tail: Tail,
    /// The cursor moved since the shape baseline (taken at arming, re-taken at a ReArm) and now
    /// sits in unterminated text: a prompt drawn SINCE then.
    pub prompt_moved_since_baseline: bool,
    /// A prompt drawn since the user's last line end (R1: required before any write that follows
    /// user bytes through a cooked shell or a relay; the line editor's own proof is the queue).
    pub prompt_moved_since_line_end: bool,
    /// Since the arming (a banner click resets it).
    pub elapsed: Duration,
    /// #1886: this pty has no subshell tty probe at all (ConPTY: no termios, no `/proc`), so
    /// `tty` is the all-unknown sample and the screen facts are the only evidence there is.
    pub no_tty_probe: bool,
}

/// What the user did since the setup was armed. Cumulative: a ReArm resets the shape baseline,
/// never this ledger (R1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UserLedger {
    /// Bytes passed through to the pty since arming.
    pub bytes_since_arm: usize,
    /// Line ends (Enter, ^U, ^C) since arming.
    pub lines_ended: u32,
    /// R4 hop binding: a shell leader was seen and then a relay took the foreground while the
    /// setup waited. Unpredicted: the rc file (or the user) ran a hop; nothing is written on that
    /// relay's shape.
    pub hop_changed_shell_to_relay: bool,
}

/// Why an attempt ended before or after its first line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelKind {
    /// The user ran a command of their own in the shell the setup waited for.
    UserDrove,
    /// No proof within [`PRE_READY_CEILING`].
    TooLong,
    /// The block changed: the shell the setup was for is gone.
    Gone,
    /// The ceiling passed with nothing but an unknown leader in front (vim, fzf, a REPL).
    NotAShell,
    /// A shell the setup cannot run in (R5's list), or a written line that was read as a plain
    /// command with no marker (R7).
    Unsupported,
}

/// Why the judge waits, for the log and the tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaitWhy {
    Secret,
    AltScreen,
    LineDirty,
    InputPrompt,
    /// R4: an unpredicted shell -> relay hop.
    Hop,
    /// No OS fact: the parent's group, another program, no probe.
    Unknown,
    /// The user's bytes are not proven consumed (R1).
    NotConsumed,
    /// A cooked shell with a line editor (bash/zsh/fish/nu/pwsh): a `read` builtin (R3).
    CookedReadBuiltin,
    /// An editor-less shell, cooked, with no new prompt drawn yet.
    CookedNoPrompt,
    /// A relay with the local tty canonical (R4: `docker exec -t` without `-i`).
    RelayCooked,
    /// A relay with no new prompt drawn yet.
    RelayNoPrompt,
}

/// The judge's answer for an armed setup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Write the first setup line now, for this shell type (`None`: the `sh` probe).
    Write(Option<ShellType>),
    Wait(WaitWhy),
    Cancel(CancelKind),
}

/// The readiness rules (the redesign's table with the review's corrections). `trigger` is the
/// shell type the trigger named (auto-warpify, the rc file, a static pwsh/nu command); `None` is
/// the probe path.
pub fn subshell_ready(
    sample: &ReadinessSample,
    user: &UserLedger,
    trigger: Option<ShellType>,
) -> Decision {
    if !sample.block_unchanged {
        return Decision::Cancel(CancelKind::Gone);
    }
    if sample.elapsed >= PRE_READY_CEILING {
        return Decision::Cancel(
            if sample.tty.kind == SubshellTtyKind::Unknown
                && !matches!(sample.tty.leader, LeaderKind::Shell | LeaderKind::Relay)
            {
                CancelKind::NotAShell
            } else {
                CancelKind::TooLong
            },
        );
    }
    // R5: a shell the probe cannot run in: given up at once, nothing written.
    if sample.tty.leader == LeaderKind::Unsupported {
        return Decision::Cancel(CancelKind::Unsupported);
    }
    if sample.alt_screen {
        return Decision::Wait(WaitWhy::AltScreen);
    }
    if sample.tty.kind == SubshellTtyKind::Secret {
        return Decision::Wait(WaitWhy::Secret);
    }
    if sample.line_dirty {
        return Decision::Wait(WaitWhy::LineDirty);
    }
    if sample.tail == Tail::InputPrompt {
        return Decision::Wait(WaitWhy::InputPrompt);
    }
    if user.hop_changed_shell_to_relay {
        return Decision::Wait(WaitWhy::Hop);
    }
    // #1886: no probe on this pty (Windows). The OS facts do not exist, so judge from the screen:
    // a shell-shaped tail drawn since the baseline, with nothing the user passed through left
    // unproven (no bytes, or a prompt drawn since the user's last line end). Only a leader that
    // is truly unreadable takes this branch; a probe that ran and found nothing still waits.
    if sample.no_tty_probe && sample.tty.leader == LeaderKind::Unreadable {
        let ready = sample.tail == Tail::ShellPrompt
            && sample.prompt_moved_since_baseline
            && (user.bytes_since_arm == 0 || sample.prompt_moved_since_line_end);
        return if ready {
            Decision::Write(write_shell_type(trigger, None))
        } else {
            Decision::Wait(WaitWhy::Unknown)
        };
    }
    let comm = sample.tty.leader_comm.as_deref().unwrap_or_default();
    let new_prompt = sample.tail == Tail::ShellPrompt && sample.prompt_moved_since_baseline;
    match sample.tty.kind {
        SubshellTtyKind::Secret => Decision::Wait(WaitWhy::Secret),
        SubshellTtyKind::Unknown => Decision::Wait(WaitWhy::Unknown),
        SubshellTtyKind::LineReader => {
            if consumed_by_line_editor(sample, user) {
                Decision::Write(write_shell_type(trigger, Some(comm)))
            } else {
                Decision::Wait(WaitWhy::NotConsumed)
            }
        }
        SubshellTtyKind::Cooked => {
            if leader_has_line_editor(comm) {
                // R3: bash/zsh/fish/nu/pwsh at a cooked tty is a `read` builtin (or the gap before
                // its editor): the editor's raw tty is the proof, never a cooked one.
                Decision::Wait(WaitWhy::CookedReadBuiltin)
            } else if !new_prompt && !idle_at_prompt_untouched(sample, user) {
                Decision::Wait(WaitWhy::CookedNoPrompt)
            } else if idle_at_prompt_untouched(sample, user) || consumed_behind_prompt(sample, user)
            {
                // An editor-less shell has no script of its own: the probe names it, and its
                // `InitSubshell` ends the setup as unsupported (#1710) or starts stage 2.
                Decision::Write(None)
            } else {
                Decision::Wait(WaitWhy::NotConsumed)
            }
        }
        SubshellTtyKind::Relay => {
            if sample.tty.canonical != Some(false) {
                Decision::Wait(WaitWhy::RelayCooked)
            } else if !new_prompt {
                Decision::Wait(WaitWhy::RelayNoPrompt)
            } else if consumed_behind_prompt(sample, user) {
                // The relay's comm says nothing about the shell behind it: the trigger's type.
                Decision::Write(trigger)
            } else {
                Decision::Wait(WaitWhy::NotConsumed)
            }
        }
    }
}

/// R1 for a shell whose line editor reads (raw tty): nothing passed through, or the slave input
/// queue is empty and the line holds nothing (`line_dirty` is false by the time this runs). The
/// editor redraws its line, not a new prompt, after a ^U, so no new prompt is asked for here.
fn consumed_by_line_editor(sample: &ReadinessSample, user: &UserLedger) -> bool {
    user.bytes_since_arm == 0 || sample.tty.queue_empty == Some(true)
}

/// Owner decision 2026-11-09 (#1847): an editor-less shell that is idle at its prompt with nothing
/// typed since arming is written to without a "moved" prompt (no event will ever move it). Only
/// these facts: a shell-shaped tail, the shell itself is the tty's reader (leader Shell), nothing
/// unread in the queue, and no user byte passed through (so R1 holds trivially).
fn idle_at_prompt_untouched(sample: &ReadinessSample, user: &UserLedger) -> bool {
    sample.tail == Tail::ShellPrompt
        && user.bytes_since_arm == 0
        && sample.tty.leader == LeaderKind::Shell
        && sample.tty.queue_empty == Some(true)
}

/// R1 for a cooked shell or a relay (owner Q8: "line ended + new prompt" counts only ALONGSIDE the
/// queue-empty proof): nothing passed through, or the queue is empty AND a prompt was drawn since
/// the user's last line end (the reader took the line and came back for the next).
fn consumed_behind_prompt(sample: &ReadinessSample, user: &UserLedger) -> bool {
    user.bytes_since_arm == 0
        || (sample.tty.queue_empty == Some(true) && sample.prompt_moved_since_line_end)
}

/// R5: the shell type the first line is written for. The trigger's, unless the leader's comm
/// names a different scripted shell (`exec zsh` in a `.bashrc`) or names none the trigger knows:
/// then the `sh` probe, which lets the shell name itself by the event.
fn write_shell_type(trigger: Option<ShellType>, leader_comm: Option<&str>) -> Option<ShellType> {
    let leader = leader_comm.and_then(shell_type_of_leader);
    match (trigger, leader) {
        (Some(t), Some(l)) if t == l => Some(t),
        (Some(_), _) => None,
        (None, _) => None,
    }
}

/// What a line end the user typed (Enter, ^U, ^C) does to an armed setup (R2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineEndVerdict {
    /// Re-take the shape baseline and keep waiting; the ledger is kept.
    ReArm,
    /// The user ran their own command in the shell the setup waited for.
    Cancel,
}

/// R2: a line end re-arms ONLY when it was empty (only the terminator, ^U or ^C), or when the
/// line answered something: the tty was reading a secret, a cooked shell was keeping up with its
/// reads (a `read` builtin: `cooked_reading`, the queue was empty at the last look), or the output
/// ended in a prompt for input. Any other non-empty line is the user driving the shell, except
/// one typed while a PROGRAM owns the tty (`program_owns_tty`: the leader at the last look was
/// neither a shell, a relay nor unsupported, e.g. vim's `:wq` + Enter): that line is the program's,
/// not the shell's, so the setup keeps waiting for the prompt that follows the program (Mac round
/// 7: the `:wq` Enter cancelled the attempt and the session was never Phosphorized).
pub fn on_user_line_ended(
    non_empty: bool,
    fact_at_enter: SubshellTtyKind,
    cooked_reading: bool,
    tail_at_enter: Tail,
    program_owns_tty: bool,
) -> LineEndVerdict {
    if !non_empty || program_owns_tty {
        return LineEndVerdict::ReArm;
    }
    match fact_at_enter {
        SubshellTtyKind::Secret => LineEndVerdict::ReArm,
        SubshellTtyKind::Cooked if cooked_reading => LineEndVerdict::ReArm,
        _ if tail_at_enter == Tail::InputPrompt => LineEndVerdict::ReArm,
        _ => LineEndVerdict::Cancel,
    }
}

/// What the user typed since the setup was armed, as the shell's LINE sees it (R2): a line is
/// empty only when nothing but its terminator was typed. Escape sequences, Tab, ^R, ^P, ^Y and
/// history recall all count as content (they change the line the shell runs). Bracketed paste is
/// understood across chunks: everything between `ESC [ 200 ~` and `ESC [ 201 ~` is content, and a
/// newline inside it does not end the line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LineTracker {
    content_bytes: usize,
    in_paste: bool,
    /// A partial `ESC [ 20x ~` at the end of the previous chunk.
    pending: Vec<u8>,
}

/// One line end the tracker saw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineEnd {
    pub non_empty: bool,
}

const PASTE_START: &[u8] = b"\x1b[200~";
const PASTE_END: &[u8] = b"\x1b[201~";

impl LineTracker {
    /// Feeds bytes passed through to the pty; returns every line end they completed, in order.
    pub fn note(&mut self, bytes: &[u8]) -> Vec<LineEnd> {
        let mut ends = Vec::new();
        let mut input = std::mem::take(&mut self.pending);
        input.extend_from_slice(bytes);
        let mut i = 0;
        while i < input.len() {
            let rest = &input[i..];
            if rest[0] == 0x1b {
                if rest.starts_with(PASTE_START) {
                    self.in_paste = true;
                    i += PASTE_START.len();
                    continue;
                }
                if rest.starts_with(PASTE_END) {
                    self.in_paste = false;
                    i += PASTE_END.len();
                    continue;
                }
                if rest.len() < PASTE_START.len()
                    && (PASTE_START.starts_with(rest) || PASTE_END.starts_with(rest))
                {
                    // A paste bracket split across chunks: judged with the next chunk.
                    self.pending = rest.to_vec();
                    break;
                }
            }
            let byte = rest[0];
            i += 1;
            if self.in_paste {
                self.content_bytes += 1;
                continue;
            }
            match byte {
                b'\r' | b'\n' => {
                    ends.push(LineEnd {
                        non_empty: self.content_bytes > 0,
                    });
                    self.content_bytes = 0;
                }
                // ^U kills the line, ^C abandons it: both leave an empty line behind.
                0x15 | 0x03 => {
                    ends.push(LineEnd { non_empty: false });
                    self.content_bytes = 0;
                }
                _ => self.content_bytes += 1,
            }
        }
        ends
    }

    /// Whether the line holds content the shell has not been told to end.
    pub fn line_has_content(&self) -> bool {
        self.content_bytes > 0 || self.in_paste
    }
}

/// One look at the pane while the setup's first line is out (Written).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WrittenSample {
    pub tty_kind: SubshellTtyKind,
    pub block_unchanged: bool,
    pub tail: Tail,
    /// The cursor moved since the first line was written and sits in unterminated text.
    pub prompt_moved_since_written: bool,
    /// A setup-line marker of THIS attempt's token (R7: token-checked, OSC 7771 is not registry
    /// validated), or the probe's `InitSubshell`, arrived since the line was written.
    pub progress_seen: bool,
    /// The tty echoed our line and the echo ended with a line feed: the shell took the line.
    pub echo_ended: bool,
    /// Prompt-shaped lines drawn after the echo ended, with no marker between.
    pub fresh_prompts_after_echo: u32,
    /// R1's queue proof at this sample.
    pub queue_empty: Option<bool>,
    /// The line went to a relay (R7: confirmed by a second prompt or the progress net, not by
    /// the queue, which is the relay's, not the remote shell's).
    pub relay: bool,
}

/// What a sample says about a written setup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WrittenVerdict {
    /// Nothing to do: progress, or nothing suspicious.
    Keep,
    /// E-SECRET: a secret read opened after our line went out. Cancel in THIS wakeup, hand the
    /// held keys back, re-arm.
    Secret,
    /// E-QUESTION: a line is outstanding with no marker and new output ends in a question. Cancel,
    /// hand back, go Idle (a click re-arms).
    Question,
    /// E-UNSUPPORTED, not yet confirmed: the echo ended and a fresh prompt followed with no
    /// marker. Local: the queue is not yet proven empty. Relay: one prompt only.
    UnsupportedSuspect,
    /// E-UNSUPPORTED confirmed: the shell read the line as a plain command (csh/tcsh, nu/pwsh as
    /// login shells, `noexec`, a raw zsh in newuser-install).
    Unsupported,
    /// E-GONE: the block changed.
    Gone,
}

/// R7 (stream-ordered), R8 and the redesign's Written rows.
pub fn judge_written(s: &WrittenSample) -> WrittenVerdict {
    // A changed block is "gone" only before the setup showed progress: the subshell's own
    // `InitShell` finishes the block the line was typed into and starts another, so after a marker
    // or `InitSubshell` the change is the bootstrap's (cancelling it truncated the script and
    // handed the user's typed line back, #1840). A subshell that really exits ends through the
    // block-completed path (`end_subshell_setup_if_block_gone`).
    if !s.block_unchanged && !s.progress_seen {
        return WrittenVerdict::Gone;
    }
    if s.tty_kind == SubshellTtyKind::Secret {
        return WrittenVerdict::Secret;
    }
    if s.progress_seen {
        return WrittenVerdict::Keep;
    }
    if s.tail == Tail::InputPrompt && s.prompt_moved_since_written {
        return WrittenVerdict::Question;
    }
    if !s.echo_ended || s.tail != Tail::ShellPrompt || s.fresh_prompts_after_echo == 0 {
        return WrittenVerdict::Keep;
    }
    let confirmed = if s.relay {
        s.fresh_prompts_after_echo >= 2
    } else {
        s.queue_empty == Some(true)
    };
    if confirmed {
        WrittenVerdict::Unsupported
    } else {
        WrittenVerdict::UnsupportedSuspect
    }
}

/// Where the keys held behind a cancelled setup go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeldKeysDest {
    /// Back into the editor as plain text: never auto-submitted, never added to history, with a
    /// count-only toast (owner Q9).
    Editor,
    /// To the pty, in order: the shell is at a fresh prompt and read the line (R8, Unsupported
    /// only).
    Pty,
}

/// Why a WRITTEN setup was cancelled, for [`held_keys_destination`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WrittenCancel {
    Secret,
    Question,
    /// The progress net or the ceiling.
    Net,
    Gone,
    Unsupported,
}

/// R8's destination table: Secret / Question / net / Gone -> the editor; Unsupported -> the pty in
/// order only when a fresh prompt is proven AND the line was consumed, else the editor.
pub fn held_keys_destination(
    why: WrittenCancel,
    fresh_prompt_proven: bool,
    line_consumed: bool,
) -> HeldKeysDest {
    match why {
        WrittenCancel::Unsupported if fresh_prompt_proven && line_consumed => HeldKeysDest::Pty,
        _ => HeldKeysDest::Editor,
    }
}

/// R9: whether a cancel flushes the slave input queue: only when the setup bytes PROVABLY sit
/// unread (the queue holds bytes and the tty echoed nothing of our line).
pub fn cancel_flushes_unread_setup(queue_empty: Option<bool>, echo_started: bool) -> bool {
    queue_empty == Some(false) && !echo_started
}

#[cfg(test)]
#[path = "subshell_ready_tests.rs"]
mod tests;
