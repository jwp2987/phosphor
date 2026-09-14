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
//! **What this pane does not show, on purpose.** `SessionStore::list()` computes
//! buffered- and dropped-byte figures per session, but `handle_list_sessions`
//! does not put them on the wire: [`RemoteSessionSummary`] carries no such
//! field. Rendering a dropped-bytes column would mean inventing the number, so
//! there is none. Adding one is a protocol change, not a pane change.

use chrono::{DateTime, Utc};
use remote_server::proto::{RemoteSessionExit, RemoteSessionSummary};
use remote_server::setup::UnsupportedReason;
use warp_core::ui::appearance::Appearance;
use warpui::{
    AppContext, Element, Entity, ModelHandle, SingletonEntity, TypedActionView, View, ViewContext,
    ViewHandle,
    elements::{
        Align, ClippedScrollStateHandle, ClippedScrollable, ConstrainedBox, Container, Flex,
        MainAxisSize, ParentElement, ScrollbarWidth, Text,
    },
};

use crate::{
    app_state::LeafContents,
    pane_group::focus_state::PaneFocusHandle,
    remote_server::host_registry::{
        HostGroup, HostInstallState, HostRegistryModel, RemoteHostEntry,
    },
    remote_server::remote_sessions_model::{
        HostSessionsDisplay, RemoteSessionsModel, host_sessions_display,
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

/// One display line for a single session under its host.
///
/// Shows only fields [`RemoteSessionSummary`] actually carries: id, run state,
/// cwd, shell and size. See the module docs for why there is no dropped-bytes
/// column.
pub(crate) fn session_row_text(summary: &RemoteSessionSummary) -> String {
    format!(
        "        {id} \u{2014} {state} \u{2014} {cwd} \u{2014} {shell} \u{2014} {rows}x{cols}",
        id = summary.remote_session_id,
        state = session_run_label(&session_run_state(summary.exit.as_ref())),
        cwd = summary.cwd,
        // The daemon reports the shell it spawned; `None` means it recorded
        // none, which is not the same as a shell named "unknown".
        shell = summary.shell.as_deref().unwrap_or("shell not reported"),
        rows = summary.rows,
        cols = summary.cols,
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

impl TypedActionView for RemoteHostsView {
    type Action = ();

    fn handle_action(&mut self, _action: &(), _ctx: &mut ViewContext<Self>) {}
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
