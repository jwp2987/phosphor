//! The edge-driven ssh phase (#1424, design step 7; owner decision OD-6).
//!
//! Steps 1-6 DERIVE [`SshPhase`] at the moment a key is decided, from the legacy view fields,
//! after running the legacy refreshers (`refresh_ssh_connect_gate`) on the key
//! path. This machine replaces that derivation: it is fed EDGES, each by the event that is the
//! edge, and a key reads its phase without refreshing anything. It is pure (no view, no clock, no
//! lock), so every transition is a unit test.
//!
//! The edges, and who sends them (`view/ssh_edges.rs`):
//!
//! | edge | event |
//! |---|---|
//! | [`SshEdge::Submitted`] | the user submitted a command (an interactive `ssh ...` arms the window) |
//! | [`SshEdge::ConnectStart`] | the shell's preexec showed an ssh command running that nothing armed |
//! | [`SshEdge::TtyOwned`] | `CommandBecameInteractive` for the window's block: ssh owns the tty (#1611, #1612, #1621). Under the ssh wrapper (`PreInteractiveSSHSession` seen) the view defers it to the wrapper's `PlainSSHFallback` hook, or to the `SSH` hook of a login shell the remote command does not bootstrap (#2120, [`ssh_hook_announces_session`]), and never sends it for a managed session (#1641) |
//! | [`SshEdge::TtyReclaimed`] | `PreInteractiveSSHSession` handled after `TtyOwned` for the same window: the wrapper's master login owns the tty, so the hold resumes until the verdict (#1641) |
//! | [`SshEdge::Observed`] | the published facts at a model event or output wakeup: a remote session announced (InitShell) or gone (Bootstrapped / exit), the ssh hook, the setup dialog and the remote-setup state (`SessionsEvent`) |
//! | [`SshEdge::BlockCompleted`] | the ssh block ended (the remote shell's first precmd, or a failed connect) |
//! | [`SshEdge::BootstrapFailed`] | the bootstrap-failed timer of the pending session (#1494's path) |
//! | [`SshEdge::HoldStarted`] / [`SshEdge::HoldEnded`] | the first key held / the hold over (delivered, discarded, handed back) |
//! | [`SshEdge::Released`] | the window ended: by an event ([`SshRelease::FirstPrompt`], [`SshRelease::Failed`], [`SshRelease::OtherCommand`]) or by a logged GIVE-UP ([`SshRelease::is_give_up`]) |
//!
//! No edge reads the screen. The window used to open on "visible text was printed" and to
//! classify an unterminated line as a shell prompt or a credential prompt (#1561, #1575); four
//! rounds of that latch failed review (#1611, #1612, #1621) and the owner replaced it with the
//! plain terminal's rule: the keys held since the submit go to ssh once ssh OWNS THE TTY
//! ([`SshEdge::TtyOwned`], a `tcgetpgrp` / `tcgetattr` fact), and the remote side decides what
//! becomes of them, exactly as a tty queue would. The one guard at that release is a termios
//! read, not text: a tty that is canonical with echo off is a secret read, and the held keys are
//! discarded as the kernel's `TCSAFLUSH` would discard typeahead (`view.rs`,
//! `ssh_on_tty_owned`). See `DECLINED.md`.
//!
//! Focus changes are not an ssh edge: the phase does not depend on focus. They reach the queue
//! (`InputQueue::on_focus_changed`), which clears the line pin.
//!
//! The machine's phase is the one the facts carry and the key path does not refresh the gate. The
//! view's own gate fields (`ssh_connect_gate`, `ssh_typeahead_holding`) stay: the events write
//! them and the signal rows read them. (The legacy derivation of the phase and its `shadow`
//! comparison, `kh_ssh_phase_disagree`, went with #1353 step 8.)

use super::facts::SshPhase;

/// Whether a connect window is born with the tty already counted as ssh's. Unix has the
/// ownership event (`CommandBecameInteractive` from a `tcgetpgrp` / `tcgetattr` probe); ConPTY
/// has no foreground group and no termios, so the only "event" there would be the latch's 50 ms
/// settle timer, and a timer must never put keys into a pty (owner rule). A plain Windows console
/// gives the keys typed after the submit to the console input buffer, where `ssh.exe` reads them
/// in order: the window does the same by holding nothing before ownership there. The managed
/// holds (the announced session's bootstrap, the post-block wait, the first prompt) are events on
/// every platform and stay. The crate's own tests drive the ownership edge by hand on every
/// platform, so under `cfg(test)` a window is always born unowned.
pub const TTY_OWNED_AT_ARM: bool = cfg!(all(windows, not(test)));

/// #2120: whether the wrapper's remote command bootstraps a login shell of this name, i.e.
/// whether an `InitShell` can follow the `SSH` hook that names it. The remote command prints the
/// hook for every login shell (`remote_shell` is the last component of the remote `$SHELL`) and
/// then starts a bootstrapped shell for exactly these two names; for every other one (sh, dash,
/// ash, ksh, mksh, ...) it prints the MotD and runs `exec "$SHELL" -l` (#2126; `exec "$SHELL"`
/// after sourcing `/etc/profile` for a shell that refuses `-l`): a plain login from there on. Keep in step with the `case` at the end of the remote command in
/// `bootstrap/bash_body.sh`, `zsh_body.sh` and `fish.sh`.
pub fn ssh_remote_command_bootstraps(remote_shell: &str) -> bool {
    matches!(remote_shell, "bash" | "zsh")
}

/// #2120: the rule the model and the view both read, so the connect gate and the phase machine
/// cannot disagree: an `SSH` hook announces a managed session (one whose `InitShell`,
/// `Bootstrapped` and first prompt will come) only for a shell the remote command bootstraps.
/// For any other shell the hook is the event that says the session is a plain login: the model
/// keeps no pending session for it, and the view ends the wrapper's hold there
/// (`TerminalView::ssh_on_remote_plain_login`). `plain_rule_on` is the kill switch
/// (`switches::ssh_unbootstrapped_plain`); off, every hook announces a session, as before.
pub fn ssh_hook_announces_session_with(remote_shell: &str, plain_rule_on: bool) -> bool {
    !plain_rule_on || ssh_remote_command_bootstraps(remote_shell)
}

/// [`ssh_hook_announces_session_with`] under the process's kill switch.
pub fn ssh_hook_announces_session(remote_shell: &str) -> bool {
    ssh_hook_announces_session_with(remote_shell, super::switches::ssh_unbootstrapped_plain())
}

/// How an ssh connect window ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SshRelease {
    /// The remote shell showed its first prompt after its bootstrap: the event that hands over.
    FirstPrompt,
    /// The ssh block ended and nothing more is coming (a failed connect, a clean end of an
    /// unmanaged session): an event.
    Failed,
    /// A command that is not ssh started: the window is stale. An event.
    OtherCommand,
    /// GIVE-UP: the window passed its ceiling with none of the events above.
    GiveUpCeiling,
    /// GIVE-UP: the announced remote session finished its bootstrap and no first prompt came.
    GiveUpFirstPromptGrace,
    /// GIVE-UP: the ssh block ended cleanly, no remote session ever registered, and the grace ran
    /// out.
    GiveUpPostBlockGrace,
}

impl SshRelease {
    /// Whether this is a safety-net release (a timer), as opposed to the event that defines the
    /// end of the window. Every give-up is logged where it happens.
    pub fn is_give_up(self) -> bool {
        matches!(
            self,
            SshRelease::GiveUpCeiling
                | SshRelease::GiveUpFirstPromptGrace
                | SshRelease::GiveUpPostBlockGrace
        )
    }

    pub fn name(self) -> &'static str {
        match self {
            SshRelease::FirstPrompt => "first_prompt",
            SshRelease::Failed => "failed",
            SshRelease::OtherCommand => "other_command",
            SshRelease::GiveUpCeiling => "give_up_ceiling",
            SshRelease::GiveUpFirstPromptGrace => "give_up_first_prompt_grace",
            SshRelease::GiveUpPostBlockGrace => "give_up_post_block_grace",
        }
    }
}

/// What the published facts and the view's rich content say at an event. Idempotent: the machine
/// derives the edges (a session announced, a session gone) by comparing it with what it
/// remembers. Nothing here is read from the screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SshObservation {
    /// A remote or subshell session is announced and not bootstrapped (`pending_session_id`).
    pub pending_session: bool,
    /// The ssh hook was seen for the pending connect (`has_pending_ssh_session`). #2120: only
    /// the hook of a shell the remote command bootstraps is such a fact
    /// ([`ssh_hook_announces_session`]); the model keeps none for any other shell.
    pub hook_seen: bool,
    /// The remote-setup choice dialog is up for the pending session.
    pub dialog_up: bool,
    /// The pending session's remote-server setup is in flight (`SessionsEvent` state).
    pub setup_in_flight: bool,
    /// #1641: the managed-ssh hold's one pass-through. Under the ssh wrapper the ownership edge is
    /// deferred until the wrapper's verdict (the view sends [`SshEdge::TtyOwned`] only for a plain
    /// fallback), but while ssh owns the tty AND it is canonical with echo off right now (a password
    /// or passphrase read, a `tcgetattr` fact, never screen text) the keys go to ssh, as before.
    pub secret_pass_through: bool,
}

/// One edge of the ssh connect window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SshEdge {
    /// A command was submitted. `ssh` is whether it is an interactive ssh connect. Whatever the
    /// previous window held is over.
    Submitted { ssh: bool },
    /// Preexec showed an ssh command running that nothing armed (recognised at preexec only).
    ConnectStart,
    /// ssh owns the tty: `CommandBecameInteractive` for the window's block (a program other than
    /// the shell is the foreground group, or the tty is raw / reading a secret). The pre-ownership
    /// hold ends here; the view delivers or discards what it held at this edge.
    TtyOwned,
    /// #1641: the ssh wrapper announced itself after the ownership edge (the latch overtook the
    /// announcement): ssh's tty is its ControlMaster login, so the window holds again until the
    /// wrapper's verdict.
    TtyReclaimed,
    /// The facts at an event.
    Observed(SshObservation),
    /// The armed ssh block completed. `clean` is whether it exited 0.
    BlockCompleted { clean: bool },
    /// The pending session's bootstrap-failed timer fired: it no longer swallows keys.
    BootstrapFailed,
    /// The first key of the connect was held.
    HoldStarted,
    /// The hold is over.
    HoldEnded,
    /// The window ended.
    Released(SshRelease),
}

/// The edge-driven ssh state. `Default` is idle.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SshPhaseMachine {
    /// The connect window is open (the legacy `ssh_connect_gate.is_some()`).
    armed: bool,
    /// Keys are being held (the legacy `ssh_typeahead_holding`).
    holding: bool,
    /// A remote session is announced and not bootstrapped.
    pending: bool,
    /// A remote session was announced at some point in this window.
    saw_pending: bool,
    /// The remote is plausibly Phosphor-managed (a session was announced, or the ssh hook seen).
    managed: bool,
    /// ssh owns the tty (the [`SshEdge::TtyOwned`] edge; born true where there is no ownership
    /// event, [`TTY_OWNED_AT_ARM`]).
    tty_owned: bool,
    /// The ssh block ended cleanly on a managed remote; the bootstrap is expected to follow.
    block_done: bool,
    /// The ssh block ended and nothing more is coming.
    failed: bool,
    /// The announced session came up (or exited): the remote bootstrap is finishing.
    bootstrap_done: bool,
    /// The pending session's bootstrap-failed timer fired.
    bootstrap_failed: bool,
    dialog_up: bool,
    setup_in_flight: bool,
    /// The managed-ssh hold's secret-read pass-through ([`SshObservation::secret_pass_through`]).
    secret_pass_through: bool,
    /// How the last window ended (for the log and the tests).
    last_release: Option<SshRelease>,
}

impl SshPhaseMachine {
    /// Applies one edge.
    pub fn apply(&mut self, edge: SshEdge) {
        match edge {
            SshEdge::Submitted { ssh } => {
                // Whatever the previous window held is over (`reset_ssh_typeahead`), and a new
                // ssh command opens a new one.
                *self = Self {
                    armed: ssh,
                    tty_owned: TTY_OWNED_AT_ARM,
                    ..self.carried_observations()
                };
            }
            SshEdge::ConnectStart => {
                if !self.armed {
                    *self = Self {
                        armed: true,
                        tty_owned: TTY_OWNED_AT_ARM,
                        holding: self.holding,
                        ..self.carried_observations()
                    };
                }
            }
            SshEdge::TtyOwned => {
                if self.armed {
                    self.tty_owned = true;
                }
            }
            SshEdge::TtyReclaimed => {
                if self.armed {
                    self.tty_owned = false;
                }
            }
            SshEdge::Observed(observed) => {
                self.dialog_up = observed.dialog_up;
                self.setup_in_flight = observed.setup_in_flight;
                self.secret_pass_through = observed.secret_pass_through;
                if observed.hook_seen {
                    self.managed = true;
                }
                if self.armed {
                    if observed.pending_session {
                        // The remote shell announced itself (InitShell).
                        if !self.pending {
                            self.bootstrap_failed = false;
                        }
                        self.saw_pending = true;
                        self.managed = true;
                    } else if self.saw_pending {
                        // Bootstrapped, or the session exited: the remote bootstrap is
                        // finishing, and the first prompt after it hands over.
                        self.bootstrap_done = true;
                    }
                }
                self.pending = observed.pending_session;
            }
            SshEdge::BlockCompleted { clean } => {
                if self.armed {
                    if clean && (self.managed || self.saw_pending) {
                        self.block_done = true;
                    } else {
                        self.failed = true;
                    }
                }
            }
            SshEdge::BootstrapFailed => self.bootstrap_failed = true,
            SshEdge::HoldStarted => self.holding = true,
            SshEdge::HoldEnded => self.holding = false,
            SshEdge::Released(how) => {
                *self = Self {
                    holding: self.holding,
                    last_release: Some(how),
                    ..self.carried_observations()
                };
            }
        }
    }

    /// A fresh machine that keeps only what the facts say (the session, the dialog, the setup
    /// state): they are observations, not part of a window.
    fn carried_observations(&self) -> Self {
        Self {
            pending: self.pending,
            dialog_up: self.dialog_up,
            setup_in_flight: self.setup_in_flight,
            secret_pass_through: self.secret_pass_through,
            ..Self::default()
        }
    }

    /// Whether the machine has anything to do on an event: a window is open, keys are held, or a
    /// session is pending. Idle machines are not fed observations (the common case costs a
    /// branch).
    pub fn is_active(&self) -> bool {
        self.armed || self.holding || self.pending
    }

    pub fn armed(&self) -> bool {
        self.armed
    }

    /// ssh owns the tty of the open window.
    #[cfg(test)]
    pub fn tty_owned(&self) -> bool {
        self.armed && self.tty_owned
    }

    // Read by the tests and the log until step 8 reads it from the queue.
    #[allow(dead_code)]
    pub fn holding(&self) -> bool {
        self.holding
    }

    /// The remote bootstrap ended and the first prompt has not been seen.
    #[cfg(test)]
    pub fn bootstrap_done(&self) -> bool {
        self.bootstrap_done
    }

    #[allow(dead_code)]
    pub fn last_release(&self) -> Option<SshRelease> {
        self.last_release
    }

    /// The pre-session half of the gate: ssh does not own the tty yet, or the post-block
    /// bootstrap wait, or the stretch from the end of the remote bootstrap to its first prompt.
    /// Full-screen programs never hold, and neither does a secret read under the managed-ssh hold
    /// (#1641). (The legacy `SshConnectGate::drops_keys` and `ssh_wrapper_secret_pass_through`.)
    fn drops_keys(&self, alt_screen: bool) -> bool {
        self.armed
            && !alt_screen
            && !self.failed
            && !self.secret_pass_through
            && (self.block_done || !self.tty_owned || self.bootstrap_done)
    }

    /// Typing is held for the pending session's bootstrap (until its failure give-up).
    fn bootstrapping(&self) -> bool {
        self.armed && self.pending && !self.bootstrap_failed
    }

    /// Keys are dropped by the gate with nothing held yet: the setup is in flight, the remote
    /// bootstrap runs, or ssh does not own the tty yet.
    fn gate_drops(&self, alt_screen: bool) -> bool {
        (self.pending && self.setup_in_flight)
            || self.bootstrapping()
            || (!self.pending && self.drops_keys(alt_screen))
    }

    /// The phase a key is decided on. `alt_screen` is the model's fact at the decision (a
    /// full-screen program never holds).
    pub fn phase(&self, alt_screen: bool) -> SshPhase {
        if self.holding || (!self.dialog_up && self.gate_drops(alt_screen)) {
            return SshPhase::Holding;
        }
        SshPhase::Idle
    }
}

#[cfg(test)]
#[path = "ssh_phase_tests.rs"]
mod tests;
