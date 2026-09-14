//! The remote-hosts dashboard pane: a read-only view over `HostRegistryModel`
//! (`docs/design/moth-parliament.md`, "The dashboard: hosts and groups need a surface, not a
//! settings page"). Shows every host and group the registry knows about -- install state, last
//! reached, OS and arch -- openable as its own tab. Follows the exact precedent
//! `settings_pane.rs` sets for a pane over app-level singleton state, rather than inventing a
//! new shape.
//!
//! # Scope
//!
//! Read and display only. No install/upgrade/remove actions: a host is a session target, a
//! group is a query target and never a session target (`docs/design/moth-parliament.md`, "Host
//! groups: a service is rarely one machine"), and partial-failure handling across a group's
//! install/upgrade is an open decision this pane does not attempt to resolve.
//!
//! ## The one exception: forgetting exited sessions
//!
//! [`ReapAffordance`] puts a control on this pane, which the rule above otherwise forbids.
//! It is admitted because it is not an action on a host. Installing, upgrading or removing
//! the remote server changes what a machine *is*; forgetting an exited session discards a
//! record -- an id, an exit status and a stale output buffer -- that the daemon is keeping
//! only so a client can read it, and that this pane is displaying at the moment the control
//! is clicked. The host is not touched, no process starts or stops, and nothing that was
//! running is affected. It clears the screen of something the user is already looking at.
//!
//! It exists because the alternative is worse: `ForgetSession` is the only thing that removes
//! a session record (`SessionStore::remove`'s only other caller is the spawn-failure
//! rollback), so without a caller every session a daemon has ever run stays in every listing
//! for the daemon's lifetime, and this pane renders an ever-growing wall of dead rows with no
//! way to clear them.
//!
//! **Exited sessions only, and the daemon agrees.** `handle_forget_session`
//! (`app/src/remote_server/server_model.rs`) refuses a running session -- "session {id} is
//! still running; signal it to exit before forgetting it" -- because forgetting one would
//! leave the daemon holding a pty and a child with no record that it owns them. So
//! [`reap_affordance`] counts only sessions the listing reports as exited, and the control
//! never names a running one. Offering a button the daemon will reject is worse than offering
//! none.
//!
//! **Why "forget all exited on this host" rather than one button per row.** Three reasons,
//! in order of weight:
//!
//! 1. A per-row control has to be absent or disabled on every running row, so the column
//!    becomes a mix of rows that offer an action and rows that do not, and a reader has to
//!    work out which rule is in play. One control per host cannot be pointed at a running
//!    session at all: the set it acts on is defined by [`exited_session_ids`], not by where
//!    the pointer happens to be.
//! 2. The user's intention is "clear the dead records", not "clear session 9f1c". Dead
//!    sessions accumulate as a class -- a host up for a week has dozens -- and one intention
//!    should not cost N clicks.
//! 3. The label states the count before the click ("Forget 3 exited sessions"), so the blast
//!    radius is visible, and it can only ever be sessions this pane is already showing as
//!    finished.
//!
//! The cost, stated plainly: forgetting an exited session also discards whatever output the
//! daemon still had buffered for it, for every session in the batch. Today nothing in `app`
//! can read that -- `RemoteServerClient::reattach_session` has no caller here, which was
//! checked rather than assumed -- so the batch destroys nothing a user could otherwise reach.
//! That is what makes the bulk affordance safe *now*. If reattach-to-an-exited-session lands
//! (`terminal/remote_server_tty/mod.rs` lists it as a later increment), an exited session's
//! buffer becomes readable and this trade should be revisited then, not assumed to still
//! hold.
//!
//! # Re-probing on open
//!
//! `docs/design/moth-parliament.md` decides that this pane re-probes on open rather than
//! trusting a restored snapshot as fact, because every value it shows is a claim about another
//! machine and a restored claim is a claim about the past. This pane holds up its half of that:
//! it never restores or caches its own copy of host/group data, it reads
//! [`HostRegistryModel`] fresh on every render and subscribes to its `HostRegistryEvent` stream
//! so any observation recorded elsewhere repaints it immediately, with no need to reopen the
//! tab.
//!
//! [`RemoteHostsView::new`] additionally calls
//! [`HostRegistryModel::refresh_from_live_connections`] once, synchronously, before returning --
//! see that method's doc comment (`app/src/remote_server/host_registry.rs`) for exactly what it
//! contacts (nothing: it only reads `RemoteServerManager`'s already-in-memory set of
//! currently-connected hosts) and what it deliberately does not (dial a fresh SSH connection to
//! a host that is not already connected, which would be worse than no refresh at all). Being
//! synchronous and free of I/O, this cannot block the pane opening.
//!
//! Because that refresh only ever touches hosts with a live connection right now, most rows
//! still show the registry's last cached observation rather than something freshly checked this
//! moment -- the on-screen caveat reflects that.
//!
//! # Never-reached vs. not-installed
//!
//! [`host_install_display`] is the mapping the rest of this module -- and its tests -- turn on:
//! it keeps [`HostInstallState::Unknown`] (nothing ever observed) visibly distinct from
//! [`HostInstallState::NotInstalled`] (a real, negative observation), the same false-safety trap
//! the footer bar's unknown-host colour exists to avoid. See the doc comment on
//! [`HostInstallDisplay`].
//!
//! # Sessions
//!
//! Each host row is followed by that host's daemon-owned pty sessions, read
//! through [`RemoteSessionsModel`] (`app/src/remote_server/remote_sessions_model.rs`),
//! which is this file's only new dependency and the first caller
//! `RemoteServerClient::list_sessions` has ever had in `app`. Three of this
//! pane's existing rules carry straight over, and the model's own docs say how
//! it keeps each:
//!
//! * **It never dials.** Only hosts that already have a live client are asked.
//!   A host without one reads "not connected" and no request is made -- never a
//!   spinner, which would imply a connection attempt that is not happening.
//! * **Nothing is cached across a disconnect.** A host's listing is dropped on
//!   `HostDisconnected`, because a list read over a dead connection is the same
//!   "claim about the past" the re-probing section above rejects.
//! * **Nothing is persisted.** The model is created by [`RemoteHostsView::new`]
//!   and dies with the tab.
//!
//! [`host_sessions_label`] keeps "not connected" (nothing was asked) separate
//! from "no sessions" (the host answered and is holding none), the same
//! distinction [`host_install_display`] draws for install state. And
//! [`session_run_state`] is where a session's `exit` field becomes
//! running/exited/signalled -- a signalled process has no exit code of its own,
//! so it never renders a fabricated `0`.
//!
//! [`session_buffer_label`] renders the buffer-pressure figures
//! [`RemoteSessionSummary`] now carries. `SessionStore::list()` always computed
//! them; until the protocol change that added fields 7 and 8 they were dropped
//! in `handle_list_sessions`, so a detached session could be losing output and
//! this pane read identically either way.
//!
//! **Which dropped figure, and why it is not the one the design doc names.**
//! The wire carries `dropped_bytes_total`, the session's lifetime eviction
//! count, and that is what this pane shows -- under the words "dropped in
//! total", never "dropped while detached". The "N KiB dropped while detached"
//! figure from `docs/design/moth-parliament.md` is a different number,
//! `dropped_bytes_since_ack`: the span between what a client has acknowledged
//! and the oldest byte still buffered, which resets when an acknowledgement
//! closes it. `SessionSummary` cannot produce it -- `SessionStore::list()` reads
//! no per-client watermark -- so `ListSessions` cannot report it, and only
//! `ReattachSessionSuccess` does. `PeekedOutput`'s field docs in
//! `session_store.rs` are explicit that the lifetime figure "is the wrong number
//! to show next to a specific reattach"; labelling it as a detached gap here
//! would be exactly that mistake, so the label says what the number is.
//!
//! A missing figure is not a zero. Both fields are `optional` on the wire, so a
//! daemon built before they existed reports neither, and
//! [`session_buffer_label`] says "not reported" rather than rendering the `0` a
//! bare proto3 scalar would have decoded that silence into.

use std::cell::RefCell;
use std::collections::HashMap;

use chrono::{DateTime, Utc};
use remote_server::proto::{RemoteSessionExit, RemoteSessionSummary};
use remote_server::setup::UnsupportedReason;
use warp_core::HostId;
use warp_core::ui::appearance::Appearance;
use warpui::{
    AppContext, Element, Entity, ModelHandle, SingletonEntity, TypedActionView, View, ViewContext,
    ViewHandle,
    elements::{
        Align, ClippedScrollStateHandle, ClippedScrollable, ConstrainedBox, Container, Flex,
        MainAxisSize, MouseStateHandle, ParentElement, ScrollbarWidth, Text,
    },
    ui_components::{button::ButtonVariant, components::UiComponent},
};

use crate::{
    app_state::LeafContents,
    pane_group::focus_state::PaneFocusHandle,
    remote_server::host_registry::{
        HostGroup, HostInstallState, HostRegistryModel, RemoteHostEntry,
    },
    remote_server::remote_sessions_model::{
        HostSessionsDisplay, ReapState, RemoteSessionsModel, exited_session_ids,
        host_sessions_display,
    },
    util::time_format::format_approx_duration_from_now_utc,
};

use super::{
    BackingView, DetachType, PaneConfiguration, PaneContent, PaneEvent, PaneGroup, PaneId,
    ShareableLink, ShareableLinkError,
    view::{self, PaneView},
};

/// Title used for both the tab/pane title and the pane header. Not localized -- matches
/// `ai/facts/view/mod.rs`'s `HEADER_TEXT` precedent, one of several existing pane titles that
/// are plain literals rather than routed through `crate::t!`.
const HEADER_TEXT: &str = "Remote Hosts";

/// How a host's install state should read to a person looking at the dashboard, collapsing
/// [`HostInstallState`]'s "unsupported reason" payload into a display-only reason string.
///
/// Keeps [`HostInstallState::Unknown`] -- nothing has ever been observed -- visibly distinct
/// from [`HostInstallState::NotInstalled`] -- a real, negative observation. Collapsing the two
/// would render a host nobody has ever probed as though it had been checked and found missing
/// the remote server: a confident lie in exactly the shape
/// `docs/design/moth-parliament.md` warns about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HostInstallDisplay {
    /// No observation exists at all: nothing has ever probed this host.
    NeverReached,
    /// A probe or install attempt found no remote server installed.
    NotInstalled,
    /// Installed. `None` when install has completed but no handshake has reported a real
    /// version yet -- see [`HostInstallState::Installed`]'s doc comment.
    Installed { version: Option<String> },
    /// A preinstall check classified this host as unable to run the prebuilt binary.
    Unsupported { reason_label: String },
}

/// Maps registry state to a [`HostInstallDisplay`]. Pure and free of `AppContext`, so the
/// never-reached/not-installed distinction is directly testable without a rendering harness --
/// see `remote_hosts_pane_tests.rs`.
pub(crate) fn host_install_display(state: &HostInstallState) -> HostInstallDisplay {
    match state {
        HostInstallState::Unknown => HostInstallDisplay::NeverReached,
        HostInstallState::NotInstalled => HostInstallDisplay::NotInstalled,
        HostInstallState::Installed { version } => HostInstallDisplay::Installed {
            version: version.clone(),
        },
        HostInstallState::Unsupported { reason } => HostInstallDisplay::Unsupported {
            reason_label: unsupported_reason_label(reason),
        },
    }
}

fn unsupported_reason_label(reason: &UnsupportedReason) -> String {
    match reason {
        UnsupportedReason::GlibcTooOld { detected, required } => {
            format!("glibc {detected} is older than the required {required}")
        }
        UnsupportedReason::NonGlibc { name } => format!("non-glibc libc ({name})"),
    }
}

/// One-line label for [`HostInstallDisplay`]. Never produces the same text for
/// `NeverReached` and `NotInstalled` -- that is the property under test.
///
/// `Installed { version: None }` is a real value: install can complete before any handshake
/// has reported a real `InitializeResponse::server_version` (see
/// `HostInstallState::Installed`'s doc comment). Rendering `None` as though it were a real
/// version would be the same false-confidence trap this pane's brief warns against for a
/// never-reached host rendered as "not installed" -- so `None` gets its own label instead of
/// formatting a version that was never observed.
pub(crate) fn host_install_label(display: &HostInstallDisplay) -> String {
    match display {
        HostInstallDisplay::NeverReached => "Never reached".to_string(),
        HostInstallDisplay::NotInstalled => "Not installed".to_string(),
        HostInstallDisplay::Installed { version: None } => {
            "Installed (version unknown)".to_string()
        }
        HostInstallDisplay::Installed {
            version: Some(version),
        } => format!("Installed (v{version})"),
        HostInstallDisplay::Unsupported { reason_label } => {
            format!("Unsupported ({reason_label})")
        }
    }
}

/// Label for when a host was last reached. `None` means no observation exists at all -- the
/// same "nothing has happened here yet" state [`host_install_display`] draws out for install
/// state, applied to the registry's other advisory field.
pub(crate) fn last_reached_label(last_reached_at: Option<DateTime<Utc>>) -> String {
    match last_reached_at {
        Some(ts) => format_approx_duration_from_now_utc(ts),
        None => "Never".to_string(),
    }
}

/// Label for a host's observed OS/arch, advisory like every other observed field (see
/// `host_registry.rs`'s module docs).
pub(crate) fn platform_label(os: Option<&str>, arch: Option<&str>) -> String {
    match (os, arch) {
        (Some(os), Some(arch)) => format!("{os} / {arch}"),
        (Some(os), None) => os.to_string(),
        (None, Some(arch)) => arch.to_string(),
        (None, None) => "Unknown".to_string(),
    }
}

/// One display line for a host row, combining every field the design doc asks this pane to
/// show: target, install state, last reached, and platform.
pub(crate) fn host_row_text(entry: &RemoteHostEntry) -> String {
    format!(
        "{target} \u{2014} {install} \u{2014} last reached: {last_reached} \u{2014} {platform}",
        target = entry.target,
        install = host_install_label(&host_install_display(&entry.install_state)),
        last_reached = last_reached_label(entry.last_reached_at),
        platform = platform_label(
            entry.os.as_ref().map(|os| os.as_str()),
            entry.arch.as_ref().map(|arch| arch.as_str()),
        ),
    )
}

/// How one session reported by `ListSessions` should read.
///
/// The whole point of `RemoteSessionSummary::exit` being an optional *message*
/// rather than a `bool` plus a code is that presence alone carries the
/// running/exited distinction, with no second field to disagree with it. This
/// enum keeps that: `Running` is the absence of `exit`, and everything else is
/// a shape of having exited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SessionRunState {
    /// No `exit` on the summary: the daemon is still holding a live pty.
    Running,
    /// Exited normally, with this code.
    Exited { code: i32 },
    /// Killed by a signal. Carries no code, because a signalled process has
    /// none -- see `SessionExitStatus::signalled` in
    /// `crates/remote_server/src/session_store.rs`, which exists precisely so
    /// nothing has to invent one.
    Signalled,
    /// Exited, but the summary says neither how. The daemon does not produce
    /// this today (`SessionExitStatus` is only ever built by `exited(code)` or
    /// `signalled()`, so one of the two is always set), but the wire admits it:
    /// both fields of `RemoteSessionExit` default to absent/false under proto3,
    /// so a peer that sends a bare `exit {}` lands here. It gets its own state
    /// rather than being folded into `Exited { code: 0 }`, which would report a
    /// successful exit that nothing observed.
    ExitedWithUnknownStatus,
}

/// Maps a summary's `exit` field to a [`SessionRunState`]. Pure and free of
/// `AppContext`, like [`host_install_display`] above, so the mapping is
/// testable without a daemon -- see `remote_hosts_pane_tests.rs`.
///
/// `signal_killed` wins over a code being present. The two together are
/// contradictory wire data (a signalled process has no code), and the signal is
/// the half that is load-bearing: reporting "exited (0)" for a killed session
/// is the fabrication this pane must not commit.
pub(crate) fn session_run_state(exit: Option<&RemoteSessionExit>) -> SessionRunState {
    let Some(exit) = exit else {
        return SessionRunState::Running;
    };
    if exit.signal_killed {
        return SessionRunState::Signalled;
    }
    match exit.exit_code {
        Some(code) => SessionRunState::Exited { code },
        None => SessionRunState::ExitedWithUnknownStatus,
    }
}

/// One-line label for [`SessionRunState`]. Never produces the same text for a
/// running session and an exited one -- that is the property under test.
pub(crate) fn session_run_label(state: &SessionRunState) -> String {
    match state {
        SessionRunState::Running => "running".to_string(),
        SessionRunState::Exited { code } => format!("exited ({code})"),
        SessionRunState::Signalled => "killed by signal".to_string(),
        SessionRunState::ExitedWithUnknownStatus => "exited (status unknown)".to_string(),
    }
}

/// Renders a byte count for a person.
///
/// Binary units, because every number it will ever be handed is measured
/// against a binary bound: `DEFAULT_OUTPUT_BUFFER_BYTES` is `256 * 1024` and
/// `session_store.rs` calls that "256 KiB". Rendering a 256 KiB buffer as
/// "262.1 KB" would invite the reader to compare it against a bound written in
/// the other base. (`settings_view/about_page/autoupdate_ui.rs` has a similar
/// helper in decimal-ish `KB`/`MB`; it is private, it rounds KB to whole units,
/// and it is describing download sizes rather than a ring buffer, so it is the
/// wrong one to reach for even if it were reachable.)
pub(crate) fn format_byte_count(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= MIB {
        format!("{:.1} MiB", b / MIB)
    } else if b >= KIB {
        format!("{:.1} KiB", b / KIB)
    } else {
        format!("{bytes} B")
    }
}

/// How much output the daemon is holding for one session, and how much it has
/// had to throw away.
///
/// Three rules, each with a way of being wrong that this exists to prevent:
///
/// * **The dropped figure is named as a lifetime total, never as a gap.** It is
///   `dropped_bytes_total`, and the "N KiB dropped while detached" number is
///   `dropped_bytes_since_ack`, which `ListSessions` does not carry and cannot
///   derive -- see the module docs. Any wording here that implies "since you
///   detached" would over-report, by exactly the whole history of the session.
/// * **`None` is not zero.** A daemon that predates these fields reports
///   neither, which is why they are `optional` on the wire; saying "0 B
///   dropped" for a daemon that never told us would be a measurement nobody
///   made.
/// * **`Some(0)` gets no clause at all.** A dropped-byte figure is only
///   interesting when it is non-zero, and "0 B dropped in total" on every
///   healthy session is noise that trains the reader to skip the field. The
///   absence of the clause is unambiguous precisely because the unknown case
///   above says something instead of nothing.
pub(crate) fn session_buffer_label(buffered: Option<u64>, dropped_total: Option<u64>) -> String {
    let buffered = match buffered {
        Some(bytes) => format!("buffered {}", format_byte_count(bytes)),
        None => "buffered not reported".to_string(),
    };
    match dropped_total {
        None => format!("{buffered}, dropped not reported"),
        Some(0) => buffered,
        Some(dropped) => format!(
            "{buffered}, {} dropped in total",
            format_byte_count(dropped)
        ),
    }
}

/// One display line for a single session under its host.
///
/// Shows the fields [`RemoteSessionSummary`] carries: id, run state, cwd, shell,
/// size, and the buffer-pressure figures via [`session_buffer_label`].
pub(crate) fn session_row_text(summary: &RemoteSessionSummary) -> String {
    format!(
        "        {id} \u{2014} {state} \u{2014} {cwd} \u{2014} {shell} \u{2014} {rows}x{cols} \u{2014} {buffer}",
        id = summary.remote_session_id,
        state = session_run_label(&session_run_state(summary.exit.as_ref())),
        cwd = summary.cwd,
        // The daemon reports the shell it spawned; `None` means it recorded
        // none, which is not the same as a shell named "unknown".
        shell = summary.shell.as_deref().unwrap_or("shell not reported"),
        rows = summary.rows,
        cols = summary.cols,
        buffer = session_buffer_label(summary.buffered_bytes, summary.dropped_bytes_total),
    )
}

/// The summary line that sits under a host row, saying what is known about its
/// sessions.
///
/// Deliberately free of any timestamp so it is deterministic under test; the
/// "checked N ago" suffix is appended at render time from
/// [`host_sessions_checked_at`].
///
/// "Not connected" and "No sessions" are different sentences because they are
/// different facts: the first means nothing was asked (this pane never dials to
/// find out), the second means the host answered and is holding none. Rendering
/// the first as the second would be the same confident lie
/// [`host_install_label`] avoids for a never-reached host.
pub(crate) fn host_sessions_label(display: &HostSessionsDisplay<'_>) -> String {
    match display {
        HostSessionsDisplay::NotConnected => "Sessions: not connected".to_string(),
        HostSessionsDisplay::NeverFetched => "Sessions: not checked yet".to_string(),
        HostSessionsDisplay::Loading => "Sessions: checking\u{2026}".to_string(),
        HostSessionsDisplay::Fetched { sessions, .. } if sessions.is_empty() => {
            "Sessions: none".to_string()
        }
        HostSessionsDisplay::Fetched { sessions, .. } if sessions.len() == 1 => {
            "Sessions: 1".to_string()
        }
        HostSessionsDisplay::Fetched { sessions, .. } => {
            format!("Sessions: {}", sessions.len())
        }
        HostSessionsDisplay::Failed { reason, .. } => {
            format!("Sessions: unavailable ({reason})")
        }
    }
}

/// When the listing behind `display` was read, if it was read at all. Drives
/// the "checked N ago" suffix; separated from [`host_sessions_label`] so that
/// function stays free of wall-clock time.
pub(crate) fn host_sessions_checked_at(display: &HostSessionsDisplay<'_>) -> Option<DateTime<Utc>> {
    match display {
        HostSessionsDisplay::NotConnected
        | HostSessionsDisplay::NeverFetched
        | HostSessionsDisplay::Loading => None,
        HostSessionsDisplay::Fetched { fetched_at, .. } => Some(*fetched_at),
        HostSessionsDisplay::Failed { failed_at, .. } => Some(*failed_at),
    }
}

/// Whether this pane should offer to forget a host's exited sessions, and what
/// the control should say.
///
/// See the module docs' "The one exception" for why this pane has a control at
/// all and why it is per host rather than per row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReapAffordance {
    /// No control. Either there is nothing this pane has been *told* is exited,
    /// or it has no listing to take that from.
    Hidden,
    /// Offer to forget `count` exited sessions -- the count is stated on the
    /// control so the blast radius is visible before the click.
    Offer { count: usize },
    /// A reap of `count` sessions is already running. The control is replaced,
    /// not merely disabled, so a second click cannot start an overlapping reap
    /// over the same ids.
    Working { count: usize },
}

/// Decides the reap control from what the pane already knows.
///
/// `NotConnected` wins over everything, the same precedence
/// `host_sessions_display` applies: a host with no live client has nothing that
/// could carry a `ForgetSession`, so offering to send one would be a control
/// that cannot work. `Loading`, `NeverFetched` and `Failed` are `Hidden` for a
/// plainer reason -- there is no listing, so there is no set of exited ids, so
/// there is no number to put on a button.
///
/// The count comes from [`exited_session_ids`], which is also what
/// `RemoteSessionsModel::forget_exited_sessions` sends. One function, so the
/// number on the control and the set on the wire cannot drift apart.
pub(crate) fn reap_affordance(
    display: &HostSessionsDisplay<'_>,
    reap: Option<&ReapState>,
) -> ReapAffordance {
    if matches!(display, HostSessionsDisplay::NotConnected) {
        return ReapAffordance::Hidden;
    }
    if let Some(ReapState::InFlight { count }) = reap {
        return ReapAffordance::Working { count: *count };
    }
    let HostSessionsDisplay::Fetched { sessions, .. } = display else {
        return ReapAffordance::Hidden;
    };
    match exited_session_ids(sessions).len() {
        0 => ReapAffordance::Hidden,
        count => ReapAffordance::Offer { count },
    }
}

/// The control's own text. Says the count, and says "exited", because those are
/// the two things that bound what clicking it will do.
pub(crate) fn reap_button_label(count: usize) -> String {
    if count == 1 {
        "Forget 1 exited session".to_string()
    } else {
        format!("Forget {count} exited sessions")
    }
}

/// What stands in for the control while a reap is running.
pub(crate) fn reap_working_label(count: usize) -> String {
    if count == 1 {
        "Forgetting 1 exited session\u{2026}".to_string()
    } else {
        format!("Forgetting {count} exited sessions\u{2026}")
    }
}

/// The line shown when the last reap did not forget everything it was asked to.
///
/// `None` for every other state, including a reap that succeeded: the refreshed
/// listing is the evidence for that one, and a second announcement of it could
/// only ever disagree with the rows.
///
/// A failure has to say something, because the sessions are still listed either
/// way -- silence would make a reap the daemon rejected look exactly like a
/// click that did nothing.
pub(crate) fn reap_error_label(reap: Option<&ReapState>) -> Option<String> {
    match reap {
        Some(ReapState::Failed { reason, .. }) => Some(format!("Forget failed: {reason}")),
        Some(ReapState::InFlight { .. }) | None => None,
    }
}

/// When the failure behind [`reap_error_label`] happened, if there is one.
/// Separated from the label for the same reason [`host_sessions_checked_at`] is
/// separated from [`host_sessions_label`]: the label stays free of wall-clock
/// time and so stays deterministic under test.
pub(crate) fn reap_failed_at(reap: Option<&ReapState>) -> Option<DateTime<Utc>> {
    match reap {
        Some(ReapState::Failed { failed_at, .. }) => Some(*failed_at),
        Some(ReapState::InFlight { .. }) | None => None,
    }
}

/// One display line for a group row: its name and its current membership. Never offers a
/// session affordance -- a group is a query target, not something `ssh` can open a shell on
/// (`docs/design/moth-parliament.md`, "Host groups").
pub(crate) fn group_row_text(group: &HostGroup) -> String {
    if group.members.is_empty() {
        format!("{} \u{2014} no members yet", group.name)
    } else {
        format!("{} \u{2014} {}", group.name, group.members.join(", "))
    }
}

/// The remote-hosts dashboard's own view. Holds no host/group state of its own -- every render
/// reads [`HostRegistryModel`] fresh, per the module docs' "re-probing on open" section.
pub struct RemoteHostsView {
    pane_configuration: ModelHandle<PaneConfiguration>,
    focus_handle: Option<PaneFocusHandle>,
    clipped_scroll_state: ClippedScrollStateHandle,
    /// Per-host pty session listings. Owned by this view rather than taken from
    /// a singleton, so it dies with the tab -- see the module docs' "Sessions"
    /// section and the model's own "Why this is a per-pane model".
    sessions: ModelHandle<RemoteSessionsModel>,
    /// Hover/press state for each host's "forget exited sessions" control.
    ///
    /// `RefCell` because `View::render` takes `&self` and the set of hosts is
    /// not known until it runs -- the same shape
    /// `ai/blocklist/agent_view/orchestration_pill_bar.rs` and
    /// `settings_view/privacy_page.rs` use for per-row controls over a list
    /// that is built at render time.
    ///
    /// This is hover state, not state about another machine: it holds nothing
    /// the pane would render as a claim, so keeping it across a disconnect
    /// breaks none of the rules in the module docs. Entries for hosts that have
    /// gone away are inert -- a `MouseStateHandle` nothing draws is never
    /// updated -- and the map dies with the tab like everything else here.
    reap_button_states: RefCell<HashMap<HostId, MouseStateHandle>>,
    /// Per-host mouse state for the "New session" button, kept for the same
    /// reason `reap_button_states` is: a button's hover state must survive the
    /// repaint that follows every model event.
    #[cfg(not(target_family = "wasm"))]
    new_session_button_states: RefCell<HashMap<HostId, MouseStateHandle>>,
}

impl RemoteHostsView {
    pub fn new(ctx: &mut ViewContext<Self>) -> Self {
        let pane_configuration = ctx.add_model(|_ctx| PaneConfiguration::new(HEADER_TEXT));

        // Repaint whenever the registry changes -- a fresh observation recorded elsewhere
        // shows up here without needing the tab to be reopened. See the module docs.
        let registry = HostRegistryModel::handle(ctx);
        ctx.subscribe_to_model(&registry, |_, _, _, ctx| {
            ctx.notify();
        });

        // Re-probe on open (module docs' "Re-probing on open" section). Synchronous and free
        // of any I/O -- see `refresh_from_live_connections`'s own doc comment for exactly what
        // it contacts -- so this cannot block the pane from opening.
        registry.update(ctx, |registry, ctx| {
            registry.refresh_from_live_connections(ctx);
        });

        // Daemon-owned pty sessions per host. Subscribed to for the same reason
        // the registry is: a listing that lands, fails, or is dropped repaints
        // this tab without it needing to be reopened.
        let sessions = ctx.add_model(RemoteSessionsModel::new);
        ctx.subscribe_to_model(&sessions, |_, _, _, ctx| {
            ctx.notify();
        });

        // Fetch on open -- for hosts that already have a live client, and only
        // those. `fetch_for_connected_hosts` reads `RemoteServerManager`'s
        // in-memory connected set and asks `client_for_host` for each, which is
        // a lookup over already-`Connected` sessions; a host with no client is
        // skipped without a request, exactly as `refresh_from_live_connections`
        // above skips a host it would have to dial. Each fetch that does happen
        // is an async RPC over a connection that already exists, so this returns
        // immediately and the pane opens showing "checking" rather than
        // blocking.
        sessions.update(ctx, |sessions, ctx| {
            sessions.fetch_for_connected_hosts(ctx);
        });

        Self {
            pane_configuration,
            focus_handle: None,
            clipped_scroll_state: Default::default(),
            sessions,
            reap_button_states: Default::default(),
            #[cfg(not(target_family = "wasm"))]
            new_session_button_states: Default::default(),
        }
    }

    pub fn pane_configuration(&self) -> ModelHandle<PaneConfiguration> {
        self.pane_configuration.clone()
    }

    pub fn focus(&mut self, ctx: &mut ViewContext<Self>) {
        ctx.focus_self();
    }

    fn render_host_row(
        &self,
        entry: &RemoteHostEntry,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let is_never_reached = matches!(
            host_install_display(&entry.install_state),
            HostInstallDisplay::NeverReached
        );
        // Never-reached hosts render in the de-emphasized color, same as any other
        // not-yet-meaningful state elsewhere in the app -- visibly distinct from a host that
        // was actually checked and found to be missing the remote server.
        let color = if is_never_reached {
            appearance.theme().disabled_ui_text_color()
        } else {
            appearance.theme().active_ui_text_color()
        };
        Text::new(
            host_row_text(entry),
            appearance.ui_font_family(),
            appearance.ui_font_body(),
        )
        .with_color(color.into())
        .finish()
    }

    /// The summary line under a host row: what is known about its sessions, plus
    /// how long ago that was read when there is something to have read.
    fn render_sessions_summary(
        &self,
        display: &HostSessionsDisplay<'_>,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let mut text = format!("    {}", host_sessions_label(display));
        if let Some(checked_at) = host_sessions_checked_at(display) {
            text.push_str(&format!(
                " (checked {})",
                format_approx_duration_from_now_utc(checked_at)
            ));
        }
        Text::new(text, appearance.ui_font_family(), appearance.ui_font_body())
            .with_color(appearance.theme().disabled_ui_text_color().into())
            .finish()
    }

    /// One session under its host. A session that has exited renders
    /// de-emphasized, the same device `render_host_row` uses for a
    /// never-reached host: the text already says which it is, and the colour
    /// stops a wall of finished sessions reading as live ones.
    fn render_session_row(
        &self,
        summary: &RemoteSessionSummary,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let is_running = matches!(
            session_run_state(summary.exit.as_ref()),
            SessionRunState::Running
        );
        let color = if is_running {
            appearance.theme().active_ui_text_color()
        } else {
            appearance.theme().disabled_ui_text_color()
        };
        Text::new(
            session_row_text(summary),
            appearance.ui_font_family(),
            appearance.ui_font_body(),
        )
        .with_color(color.into())
        .finish()
    }

    /// The "forget N exited sessions" control for one host.
    ///
    /// A "New session" button for a connected host: the entry point for item 6's
    /// session-creation path.
    ///
    /// Dispatches `WorkspaceAction::AddRemoteServerSessionTab` directly rather
    /// than hopping through [`RemoteHostsAction`], because nothing here handles
    /// it -- the `Workspace` does, and typed actions bubble to the view that
    /// owns them (`tab.rs` dispatches `WorkspaceAction` the same way from a
    /// non-workspace view). Adding a local variant that only re-dispatched would
    /// be a hop with no decision in it.
    ///
    /// Offered only for a host with a live client. That is the same condition
    /// `ConnectedRemotePtySession::for_spawned_session` enforces, so a host
    /// without one would produce a button whose only outcome is a logged
    /// failure.
    #[cfg(not(target_family = "wasm"))]
    fn render_new_session_button(
        &self,
        host_id: &HostId,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let mouse_state = self
            .new_session_button_states
            .borrow_mut()
            .entry(host_id.clone())
            .or_default()
            .clone();
        let host_id = host_id.clone();
        Container::new(
            ConstrainedBox::new(
                appearance
                    .ui_builder()
                    .button(ButtonVariant::Text, mouse_state)
                    .with_text_label("New session".to_string())
                    .build()
                    .on_click(move |ctx, _, _| {
                        ctx.dispatch_typed_action(
                            crate::workspace::WorkspaceAction::AddRemoteServerSessionTab(
                                host_id.clone(),
                            ),
                        );
                    })
                    .finish(),
            )
            .with_max_width(260.)
            .finish(),
        )
        .with_margin_left(16.)
        .finish()
    }

    /// Dispatches [`RemoteHostsAction::ForgetExitedSessions`] and nothing else:
    /// the set of ids is re-read from the model when the action is handled,
    /// rather than captured here, so a listing that changed between the paint
    /// and the click cannot make this forget something the button was not
    /// offering.
    fn render_reap_button(
        &self,
        host_id: &HostId,
        count: usize,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let mouse_state = self
            .reap_button_states
            .borrow_mut()
            .entry(host_id.clone())
            .or_default()
            .clone();
        let host_id = host_id.clone();
        Container::new(
            ConstrainedBox::new(
                appearance
                    .ui_builder()
                    .button(ButtonVariant::Text, mouse_state)
                    .with_text_label(reap_button_label(count))
                    .build()
                    .on_click(move |ctx, _, _| {
                        ctx.dispatch_typed_action(RemoteHostsAction::ForgetExitedSessions {
                            host_id: host_id.clone(),
                        });
                    })
                    .finish(),
            )
            .with_max_width(260.)
            .finish(),
        )
        .with_margin_left(16.)
        .finish()
    }

    /// A de-emphasized line under a host: the in-progress replacement for the
    /// reap control, or the reason the last reap did not finish.
    fn render_reap_note(&self, text: String, appearance: &Appearance) -> Box<dyn Element> {
        Text::new(
            format!("    {text}"),
            appearance.ui_font_family(),
            appearance.ui_font_body(),
        )
        .with_color(appearance.theme().disabled_ui_text_color().into())
        .finish()
    }

    fn render_group_row(&self, group: &HostGroup, appearance: &Appearance) -> Box<dyn Element> {
        Text::new(
            group_row_text(group),
            appearance.ui_font_family(),
            appearance.ui_font_body(),
        )
        .with_color(appearance.theme().active_ui_text_color().into())
        .finish()
    }

    fn render_section_label(&self, text: &str, appearance: &Appearance) -> Box<dyn Element> {
        Text::new(
            text.to_string(),
            appearance.ui_font_family(),
            appearance.ui_font_overline(),
        )
        .with_color(appearance.theme().disabled_ui_text_color().into())
        .finish()
    }
}

impl Entity for RemoteHostsView {
    type Event = PaneEvent;
}

impl View for RemoteHostsView {
    fn ui_name() -> &'static str {
        "RemoteHostsView"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let registry = HostRegistryModel::as_ref(app);
        let sessions = self.sessions.as_ref(app);
        // Read once per render, not per row. This is the manager's in-memory set
        // of hosts with a live connection right now -- the same read
        // `refresh_from_live_connections` makes, and equally free of I/O.
        let connected_host_ids = RemoteSessionsModel::connected_host_ids(app);

        let mut hosts: Vec<&RemoteHostEntry> = registry.hosts().collect();
        hosts.sort_by(|a, b| a.target.cmp(&b.target));

        let mut groups: Vec<&HostGroup> = registry.groups().collect();
        groups.sort_by(|a, b| a.name.cmp(&b.name));

        let mut col = Flex::column().with_main_axis_size(MainAxisSize::Min);

        // Explicit, always-visible caveat: opening this pane only refreshes hosts with a live
        // connection right now (see the module docs' "Re-probing on open" section) -- it does
        // not dial anything new, so most rows still reflect the registry's last-seen state, not
        // a check made this moment. Distinct from the per-row "Never reached" label, which
        // covers the case where there is no observation at all rather than a stale one.
        col.add_child(self.render_section_label(
            "Currently-connected hosts are refreshed live and are the only ones asked for \
             their sessions; others show the registry's last-seen state.",
            appearance,
        ));

        col.add_child(self.render_section_label("Hosts", appearance));
        if hosts.is_empty() {
            col.add_child(self.render_section_label("No hosts configured yet.", appearance));
        } else {
            for host in &hosts {
                col.add_child(self.render_host_row(host, appearance));

                // A host with no resolved `host_id` has never completed a
                // handshake, so it cannot be in the connected set and cannot be
                // keyed in the sessions model either: it reads "not connected",
                // which is the truth.
                let is_connected = host
                    .host_id
                    .as_ref()
                    .is_some_and(|host_id| connected_host_ids.contains(host_id));
                let fetch = host
                    .host_id
                    .as_ref()
                    .and_then(|host_id| sessions.fetch_state(host_id));
                let display = host_sessions_display(is_connected, fetch);
                col.add_child(self.render_sessions_summary(&display, appearance));
                if let HostSessionsDisplay::Fetched {
                    sessions: listed, ..
                } = &display
                {
                    for summary in listed.iter() {
                        col.add_child(self.render_session_row(summary, appearance));
                    }
                }

                // "New session", offered only for a host that is actually
                // connected. `HostSessionsDisplay::NotConnected` is the same
                // condition `ConnectedRemotePtySession::for_spawned_session`
                // enforces, so offering it otherwise would produce a button
                // whose only possible outcome is a logged failure.
                #[cfg(not(target_family = "wasm"))]
                if let Some(host_id) = host.host_id.as_ref() {
                    // `&display`, because `display` is read again by the reap
                    // section below. A unit-variant pattern would not move it,
                    // but borrowing says so at a glance instead of resting on
                    // that rule.
                    if !matches!(&display, HostSessionsDisplay::NotConnected) {
                        col.add_child(self.render_new_session_button(host_id, appearance));
                    }
                }

                // The reap control and its aftermath. A host with no resolved
                // `host_id` has nothing to key either on, and by the same
                // reasoning as above it reads as not connected -- so it gets
                // neither.
                let reap = host
                    .host_id
                    .as_ref()
                    .and_then(|host_id| sessions.reap_state(host_id));
                if let Some(host_id) = host.host_id.as_ref() {
                    match reap_affordance(&display, reap) {
                        ReapAffordance::Hidden => {}
                        ReapAffordance::Offer { count } => {
                            col.add_child(self.render_reap_button(host_id, count, appearance));
                        }
                        ReapAffordance::Working { count } => {
                            col.add_child(
                                self.render_reap_note(reap_working_label(count), appearance),
                            );
                        }
                    }
                }
                if let Some(mut text) = reap_error_label(reap) {
                    if let Some(failed_at) = reap_failed_at(reap) {
                        text.push_str(&format!(
                            " ({} ago)",
                            format_approx_duration_from_now_utc(failed_at)
                        ));
                    }
                    col.add_child(self.render_reap_note(text, appearance));
                }
            }
        }

        col.add_child(self.render_section_label("Groups", appearance));
        if groups.is_empty() {
            col.add_child(self.render_section_label("No groups yet.", appearance));
        } else {
            for group in &groups {
                col.add_child(self.render_group_row(group, appearance));
            }
        }

        ClippedScrollable::vertical(
            self.clipped_scroll_state.clone(),
            Align::new(
                Container::new(
                    ConstrainedBox::new(col.finish())
                        .with_max_width(720.)
                        .finish(),
                )
                .with_uniform_padding(16.)
                .finish(),
            )
            .top_center()
            .finish(),
            ScrollbarWidth::Auto,
            appearance.theme().nonactive_ui_detail().into(),
            appearance.theme().active_ui_detail().into(),
            warpui::elements::Fill::None,
        )
        .finish()
    }
}

/// The one action this pane can dispatch. See the module docs' "The one
/// exception" for why a read-and-display pane has one at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteHostsAction {
    /// Forget every session `host_id`'s daemon currently reports as exited.
    ///
    /// Carries the host, not the session ids. The ids are re-read from
    /// `RemoteSessionsModel` when this is handled, so a listing that was
    /// refreshed between the paint and the click decides what is forgotten --
    /// ids captured at paint time could name a session that has since been
    /// forgotten by someone else, or miss one that has since exited.
    ForgetExitedSessions { host_id: HostId },
}

impl TypedActionView for RemoteHostsView {
    type Action = RemoteHostsAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            RemoteHostsAction::ForgetExitedSessions { host_id } => {
                let host_id = host_id.clone();
                self.sessions.update(ctx, |sessions, ctx| {
                    sessions.forget_exited_sessions(host_id, ctx);
                });
            }
        }
    }
}

impl BackingView for RemoteHostsView {
    type PaneHeaderOverflowMenuAction = ();
    type CustomAction = ();
    type AssociatedData = ();

    fn handle_pane_header_overflow_menu_action(
        &mut self,
        _action: &Self::PaneHeaderOverflowMenuAction,
        _ctx: &mut ViewContext<Self>,
    ) {
        // Never called: `pane_header_overflow_menu_items` is not overridden, so it defaults to
        // an empty menu (see `BackingView`'s default) and nothing can select an item here.
        unimplemented!()
    }

    fn close(&mut self, ctx: &mut ViewContext<Self>) {
        ctx.emit(PaneEvent::Close);
    }

    fn focus_contents(&mut self, ctx: &mut ViewContext<Self>) {
        self.focus(ctx);
    }

    fn render_header_content(
        &self,
        _ctx: &view::HeaderRenderContext<'_>,
        _app: &AppContext,
    ) -> view::HeaderContent {
        view::HeaderContent::simple(HEADER_TEXT)
    }

    fn set_focus_handle(&mut self, focus_handle: PaneFocusHandle, _ctx: &mut ViewContext<Self>) {
        self.focus_handle = Some(focus_handle);
    }
}

/// The pane wrapper: owns the `PaneView<RemoteHostsView>` and its `PaneConfiguration`, exactly
/// the shape `settings_pane.rs`/`ai_fact_pane.rs` use. No per-window manager -- unlike Settings,
/// there is no "only one dashboard pane per window" requirement, so opening the dashboard again
/// simply adds another tab, the same as `AddConversationTab`.
pub struct RemoteHostsPane {
    view: ViewHandle<PaneView<RemoteHostsView>>,
    pane_configuration: ModelHandle<PaneConfiguration>,
}

impl RemoteHostsPane {
    pub fn new<V: View>(ctx: &mut ViewContext<V>) -> Self {
        let remote_hosts_view = ctx.add_typed_action_view(RemoteHostsView::new);
        let pane_configuration = remote_hosts_view.as_ref(ctx).pane_configuration();
        let pane_view = ctx.add_typed_action_view(|ctx| {
            let pane_id = PaneId::from_remote_hosts_pane_ctx(ctx);
            PaneView::new(
                pane_id,
                remote_hosts_view,
                (),
                pane_configuration.clone(),
                ctx,
            )
        });
        Self {
            view: pane_view,
            pane_configuration,
        }
    }

    fn remote_hosts_view(&self, ctx: &AppContext) -> ViewHandle<RemoteHostsView> {
        self.view.as_ref(ctx).child(ctx)
    }
}

impl PaneContent for RemoteHostsPane {
    fn id(&self) -> PaneId {
        PaneId::from_remote_hosts_pane_view(&self.view)
    }

    fn attach(
        &self,
        _group: &PaneGroup,
        focus_handle: PaneFocusHandle,
        ctx: &mut ViewContext<PaneGroup>,
    ) {
        self.view
            .update(ctx, |view, ctx| view.set_focus_handle(focus_handle, ctx));

        let pane_id = self.id();
        let child = self.remote_hosts_view(ctx);
        ctx.subscribe_to_view(&child, move |pane_group, _, event, ctx| {
            pane_group.handle_pane_event(pane_id, event, ctx);
        });
        ctx.subscribe_to_view(&self.view, move |group, _, event, ctx| {
            group.handle_pane_view_event(pane_id, event, ctx);
        });
    }

    fn detach(
        &self,
        _group: &PaneGroup,
        _detach_type: DetachType,
        ctx: &mut ViewContext<PaneGroup>,
    ) {
        let child = self.remote_hosts_view(ctx);
        ctx.unsubscribe_to_view(&child);
        ctx.unsubscribe_to_view(&self.view);
    }

    fn snapshot(&self, _app: &AppContext) -> LeafContents {
        LeafContents::RemoteHostsDashboard
    }

    fn has_application_focus(&self, ctx: &mut ViewContext<PaneGroup>) -> bool {
        self.view.is_self_or_child_focused(ctx)
    }

    fn focus(&self, ctx: &mut ViewContext<PaneGroup>) {
        self.remote_hosts_view(ctx)
            .update(ctx, |view, ctx| view.focus(ctx));
    }

    fn shareable_link(
        &self,
        _ctx: &mut ViewContext<PaneGroup>,
    ) -> Result<ShareableLink, ShareableLinkError> {
        Ok(ShareableLink::Base)
    }

    fn pane_configuration(&self) -> ModelHandle<PaneConfiguration> {
        self.pane_configuration.clone()
    }

    fn is_pane_being_dragged(&self, ctx: &AppContext) -> bool {
        self.view.as_ref(ctx).is_being_dragged()
    }
}

#[cfg(test)]
#[path = "remote_hosts_pane_tests.rs"]
mod tests;
