//! #1882: the take-over NOTICE for a waited agent command left on a prompt-shaped line, on a
//! platform with no termios (Windows / ConPTY).
//!
//! Unix feeds [`PromptDetector`] from termios and `/proc`; ConPTY has neither, so the answer to
//! "does the foreground read the tty" is always [`ForegroundTtyRead::Unknown`] and the detector can
//! only produce [`PromptDetection::MayBeWaiting`]: a notice ("This command may be waiting for
//! input -- Take over") after the shape's quiet window. Owner rule (#1796, #1811): a timer never
//! decides routing, so this NEVER hands the command over. The hand-over on Windows is event-driven
//! (the PowerShell bootstrap's `TtyRead` marker, #1882); this is the weaker fallback for programs
//! that print a prompt without announcing it (`ssh`, `gh auth login`, a `winget` Y/N question).
//!
//! The watch is a pure state machine: the view calls [`PromptNoticeWatch::observe`] when output
//! arrives and when the timer it armed fires, and acts on the returned [`NoticeStep`].

// Only the Windows view uses it (unix has termios and the manager's `PromptWatch`); the logic is
// platform-independent so its tests run everywhere.
#![cfg_attr(not(windows), allow(dead_code))]

use crate::terminal::model::block::{
    ForegroundTtyRead, InteractivePromptCandidate, PromptDetection, PromptDetector,
};
use crate::terminal::model::terminal_model::BlockIndex;

/// What the view does after one observation.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct NoticeStep {
    /// Show the "may be waiting for input -- Take over" notice now.
    pub show_notice: bool,
    /// Arm the single quiet timer for this instant; call [`PromptNoticeWatch::timer_fired`] with
    /// the epoch when it fires. `None`: keep the timer that is armed (or none is needed).
    pub arm: Option<(instant::Instant, u64)>,
}

#[derive(Debug, Default)]
pub(super) struct PromptNoticeWatch {
    detector: PromptDetector,
    /// The block the detector's observations belong to; a different block starts over.
    block: Option<BlockIndex>,
    /// The last observation had a candidate (so a `None` still has something to forget).
    observing: bool,
    /// Bumped to invalidate a pending quiet timer.
    epoch: u64,
    /// The instant the pending quiet timer is armed for.
    armed_for: Option<instant::Instant>,
}

impl PromptNoticeWatch {
    /// One observation of the active block's prompt-shaped line (`candidate`, `None` when the
    /// block is not an agent command awaited by a tool call, or the line is not prompt-shaped).
    pub(super) fn observe(
        &mut self,
        block: BlockIndex,
        now: instant::Instant,
        candidate: Option<InteractivePromptCandidate>,
    ) -> NoticeStep {
        if self.block != Some(block) {
            self.detector.reset();
            self.block = Some(block);
            self.epoch = self.epoch.wrapping_add(1);
            self.armed_for = None;
        }
        self.observing = candidate.is_some();
        // Cooked tty with echo on is the only shape an interactive prompt has; ConPTY cannot say
        // otherwise, and the read probe is unknown, so only a notice can come out.
        let detection =
            self.detector
                .observe(now, true, true, || candidate, || ForegroundTtyRead::Unknown);
        let mut step = NoticeStep {
            show_notice: detection == Some(PromptDetection::MayBeWaiting),
            arm: None,
        };
        let wake = self.detector.next_wake();
        if wake != self.armed_for {
            self.armed_for = wake;
            self.epoch = self.epoch.wrapping_add(1);
            step.arm = wake.map(|at| (at, self.epoch));
        }
        step
    }

    /// The quiet timer armed with `epoch` fired. Whether it is still the live one (a newer
    /// observation moved the deadline otherwise); the caller then observes again.
    pub(super) fn timer_fired(&mut self, epoch: u64) -> bool {
        if self.epoch != epoch {
            return false;
        }
        self.armed_for = None;
        true
    }

    /// Whether the last observation saw a prompt-shaped line: the caller may skip the cheap
    /// `None` observations while this is false.
    pub(super) fn is_observing(&self) -> bool {
        self.observing
    }

    /// The block it watches, for the caller's "same block?" check.
    pub(super) fn block(&self) -> Option<BlockIndex> {
        self.block
    }
}

#[cfg(test)]
#[path = "prompt_notice_tests.rs"]
mod tests;
