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
//! # Re-probing on open -- what is wired, and what is not
//!
//! `docs/design/moth-parliament.md` decides that this pane re-probes on open rather than
//! trusting a restored snapshot as fact, because every value it shows is a claim about another
//! machine and a restored claim is a claim about the past. This pane holds up its half of that:
//! it never restores or caches its own copy of host/group data, it reads
//! [`HostRegistryModel`] fresh on every render and subscribes to its `HostRegistryEvent` stream
//! so any observation recorded elsewhere repaints it immediately, with no need to reopen the
//! tab.
//!
//! What it cannot do from here is **cause** a probe. [`HostRegistryModel`] (`app/src/remote_server
//! /host_registry.rs`) only exposes `record_reached`/`record_install_state`, which *record* an
//! observation something else already made; there is no `request_probe`/`refresh` entry point
//! anywhere this pane is allowed to reach. The only code that talks to a real host is under
//! `app/src/remote_server/**` and `app/src/terminal/**` (e.g. `remote_server_controller.rs`),
//! which this change does not own and must not edit -- see this worktree's file-ownership split.
//! So on open this pane shows the registry's cached values, clearly labeled as last-seen (never
//! as current), rather than fabricating a probe call that does not exist.
//!
//! **What a human needs to wire up**: an entry point on [`HostRegistryModel`] (or the
//! `remote_server` manager) that actively re-probes every known host, and a call to it from
//! [`RemoteHostsView::new`] in place of the comment marked `TODO(remote-hosts-dashboard)` below.
//! Once that exists, the subscription already in place means no further UI change is needed for
//! the result to show up.
//!
//! # Never-reached vs. not-installed
//!
//! [`host_install_display`] is the mapping the rest of this module -- and its tests -- turn on:
//! it keeps [`HostInstallState::Unknown`] (nothing ever observed) visibly distinct from
//! [`HostInstallState::NotInstalled`] (a real, negative observation), the same false-safety trap
//! the footer bar's unknown-host colour exists to avoid. See the doc comment on
//! [`HostInstallDisplay`].

use chrono::{DateTime, Utc};
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
    /// Installed, as of the last successful handshake.
    Installed { version: String },
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
/// `Installed { version }` can carry an empty string: the wiring that records a completed
/// install currently has no way to read `InitializeResponse::server_version` back out, so it
/// records `Installed { version: String::new() }` rather than fabricating a number. Rendering
/// that blank as `"Installed (v)"` would be the same false-confidence trap this pane's brief
/// warns against for a never-reached host rendered as "not installed" -- a blank presented as
/// though it were a real value -- so an empty version gets its own label instead.
pub(crate) fn host_install_label(display: &HostInstallDisplay) -> String {
    match display {
        HostInstallDisplay::NeverReached => "Never reached".to_string(),
        HostInstallDisplay::NotInstalled => "Not installed".to_string(),
        HostInstallDisplay::Installed { version } if version.is_empty() => {
            "Installed (version unknown)".to_string()
        }
        HostInstallDisplay::Installed { version } => format!("Installed (v{version})"),
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
}

impl RemoteHostsView {
    pub fn new(ctx: &mut ViewContext<Self>) -> Self {
        let pane_configuration = ctx.add_model(|_ctx| PaneConfiguration::new(HEADER_TEXT));

        // Repaint whenever the registry changes -- a fresh observation recorded elsewhere
        // (including a probe the other agent working in this worktree is wiring up) shows up
        // here without needing the tab to be reopened. See the module docs.
        let registry = HostRegistryModel::handle(ctx);
        ctx.subscribe_to_model(&registry, |_, _, _, ctx| {
            ctx.notify();
        });

        // TODO(remote-hosts-dashboard): there is no reachable entry point to actively re-probe
        // every known host from here (see the module docs' "Re-probing on open" section) --
        // call it here once one exists. Until then this view only reads the registry's cached,
        // last-seen values.
        Self {
            pane_configuration,
            focus_handle: None,
            clipped_scroll_state: Default::default(),
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

        let mut hosts: Vec<&RemoteHostEntry> = registry.hosts().collect();
        hosts.sort_by(|a, b| a.target.cmp(&b.target));

        let mut groups: Vec<&HostGroup> = registry.groups().collect();
        groups.sort_by(|a, b| a.name.cmp(&b.name));

        let mut col = Flex::column().with_main_axis_size(MainAxisSize::Min);

        // Explicit, always-visible caveat: everything below is the registry's last-seen state,
        // not a live re-probe (see the module docs' "Re-probing on open" section). This is the
        // fallback the design doc calls for when the probe entry point is not reachable from
        // here -- distinct from the per-row "Never reached" label, which covers the case where
        // there is no observation at all rather than a stale one.
        col.add_child(self.render_section_label(
            "Showing the registry's last-seen state -- not a live check.",
            appearance,
        ));

        col.add_child(self.render_section_label("Hosts", appearance));
        if hosts.is_empty() {
            col.add_child(self.render_section_label("No hosts configured yet.", appearance));
        } else {
            for host in &hosts {
                col.add_child(self.render_host_row(host, appearance));
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
