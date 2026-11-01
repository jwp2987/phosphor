//! Whether a running command is "long running" / interactive (#1373), decided by latched events.
//!
//! `Block::is_active_and_long_running` used to be `command_start_time.elapsed() >= 50 ms`: a
//! wall-clock guess that flipped a block into interactive mode when the app was merely slow to
//! process `preexec` (a loaded main thread, a slow shell), and never flipped one that really was
//! interactive but fast. The ~150 consumers of that predicate all ask one question -- "does a
//! program, rather than the shell prompt, own the terminal?" -- so it is now answered by a latch
//! that events set and nothing resets for the life of the block:
//!
//! * the alternate screen was entered (`TerminalModel::enter_alt_screen`), immediately;
//! * the tty went non-canonical (raw mode) or canonical-without-echo (a password read), observed
//!   by the [`InteractiveProbe`] whenever the command produces output, immediately;
//! * the shell itself is blocked waiting (a builtin such as `read x` typed at the prompt: the
//!   foreground group is still the shell and it prints nothing, but its process is asleep), observed
//!   by the probe at the debounce, and on every output batch after it;
//! * a program other than the shell is the terminal's foreground process group, observed by the
//!   probe once the presentation debounce below has elapsed, and on every output batch after it.
//!
//! The one remaining timer is the *presentation debounce*, [`SETTLE_DELAY_MS`]: a command that
//! prints and exits in a few milliseconds (`git status`) must not flash the long-running
//! presentation (input editor hidden, padding collapsed) for one frame. It is anchored when the
//! block's `preexec` was processed (not when the user pressed Enter), it only decides WHEN the
//! probe is consulted, and it never sets the latch by itself -- except where there is no probe at
//! all (Windows/ConPTY exposes neither termios nor a foreground process group; no OS API exists to
//! ask), where it falls back to "still executing when the debounce elapsed" and says so in the log.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use super::BlockId;
use crate::terminal::event::Event;
use crate::terminal::event_listener::ChannelEventListener;

/// The presentation debounce, in milliseconds. See the module docs: pacing, not a decision.
pub const SETTLE_DELAY_MS: u64 = 50;

/// What the pty says about who owns the terminal right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalOwner {
    /// The shell itself (a builtin, a function, or between commands), cooked mode with echo.
    Shell,
    /// A program other than the shell is the foreground process group.
    Program,
    /// The shell itself is the foreground group, the tty is cooked, and the shell process is
    /// blocked in the kernel (asleep) rather than computing: a builtin waiting on the tty, e.g.
    /// `read x` typed at the prompt (#1373). Nothing else distinguishes it from a builtin that is
    /// merely slow, so the blocked state is the signal; a busy shell function stays `Shell`.
    ShellBlocked,
    /// The tty is in raw mode or reading a password: a program is definitely reading input.
    Input,
}

/// Reads the pty's current owner. Cheap (`tcgetpgrp` + `tcgetattr`); must not block.
pub type InteractiveProbe = Arc<dyn Fn() -> TerminalOwner + Send + Sync>;

/// Why the latch was set, for the log.
#[derive(Clone, Copy, Debug)]
pub enum LatchReason {
    AltScreen,
    InputMode,
    ForegroundProgram,
    /// The shell is blocked waiting (a builtin such as `read` reading the tty).
    ShellBlockedBuiltin,
    NoProbeFallback,
}

struct Inner {
    id: BlockId,
    latched: AtomicBool,
    /// Between `preexec` and `finish`.
    executing: AtomicBool,
    /// The presentation debounce has elapsed.
    settled: AtomicBool,
    probe: Mutex<Option<InteractiveProbe>>,
    event_proxy: ChannelEventListener,
}

/// Cheap to clone; shared with the debounce task.
#[derive(Clone)]
pub struct InteractiveLatch(Arc<Inner>);

impl std::fmt::Debug for InteractiveLatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InteractiveLatch")
            .field("latched", &self.is_latched())
            .field("executing", &self.0.executing.load(Ordering::Relaxed))
            .field("settled", &self.0.settled.load(Ordering::Relaxed))
            .finish()
    }
}

impl InteractiveLatch {
    pub fn new(id: BlockId, event_proxy: ChannelEventListener) -> Self {
        Self(Arc::new(Inner {
            id,
            latched: AtomicBool::new(false),
            executing: AtomicBool::new(false),
            settled: AtomicBool::new(false),
            probe: Mutex::new(None),
            event_proxy,
        }))
    }

    pub fn is_latched(&self) -> bool {
        self.0.latched.load(Ordering::Relaxed)
    }

    /// Test and fixture hook: forces the latch without sending an event.
    #[cfg(any(test, feature = "test-util"))]
    pub fn force(&self, latched: bool) {
        self.0.latched.store(latched, Ordering::Relaxed);
    }

    pub fn set_executing(&self, executing: bool) {
        self.0.executing.store(executing, Ordering::Relaxed);
    }

    pub fn set_probe(&self, probe: Option<InteractiveProbe>) {
        if let Ok(mut slot) = self.0.probe.lock() {
            *slot = probe;
        }
    }

    /// Sets the latch. Returns whether this call set it; only that call announces it.
    pub fn latch(&self, reason: LatchReason) -> bool {
        if self.0.latched.swap(true, Ordering::Relaxed) {
            return false;
        }
        log::info!(
            "block {:?} is interactive/long running: {reason:?}",
            self.0.id
        );
        self.0
            .event_proxy
            .send_terminal_event(Event::CommandBecameInteractive {
                block_id: self.0.id.clone(),
            });
        self.0.event_proxy.send_wakeup_event();
        true
    }

    fn probe(&self) -> Option<InteractiveProbe> {
        self.0.probe.lock().ok().and_then(|slot| slot.clone())
    }

    /// The command produced output (called on the pty thread after each batch while the block
    /// executes). Raw mode / a password read latch at once; a foreground program latches only
    /// after the debounce so a fast command never flashes the long-running presentation.
    pub fn on_output(&self) {
        if self.is_latched() || !self.0.executing.load(Ordering::Relaxed) {
            return;
        }
        let Some(probe) = self.probe() else {
            return;
        };
        match probe() {
            TerminalOwner::Input => {
                self.latch(LatchReason::InputMode);
            }
            TerminalOwner::Program if self.0.settled.load(Ordering::Relaxed) => {
                self.latch(LatchReason::ForegroundProgram);
            }
            TerminalOwner::ShellBlocked if self.0.settled.load(Ordering::Relaxed) => {
                self.latch(LatchReason::ShellBlockedBuiltin);
            }
            _ => {}
        }
    }

    /// The presentation debounce elapsed. Consults the probe about the terminal NOW: a command that
    /// already finished, or that never left the shell, does not latch however late this runs.
    pub fn on_settled(&self) {
        self.0.settled.store(true, Ordering::Relaxed);
        if self.is_latched() || !self.0.executing.load(Ordering::Relaxed) {
            return;
        }
        match self.probe() {
            Some(probe) => match probe() {
                TerminalOwner::Program => {
                    self.latch(LatchReason::ForegroundProgram);
                }
                TerminalOwner::Input => {
                    self.latch(LatchReason::InputMode);
                }
                TerminalOwner::ShellBlocked => {
                    self.latch(LatchReason::ShellBlockedBuiltin);
                }
                TerminalOwner::Shell => {}
            },
            None => {
                // No OS API to ask (ConPTY has no termios / foreground group). Executing past the
                // debounce is all that is known, so that is the signal; never reached on unix.
                self.latch(LatchReason::NoProbeFallback);
            }
        }
    }
}
