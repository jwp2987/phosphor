//! Banner shown when the remote-server binary check, installation, or connection fails on the remote host.
//! We fall back to the existing Warpification behavior and display this banner so the user knows why advanced features are unavailable.

use warp_core::ui::theme::AnsiColorIdentifier;
use warp_core::ui::theme::color::internal_colors;
use warpui::{
    AppContext, Element, Entity, SingletonEntity, TypedActionView, View, ViewContext,
    elements::{
        ConstrainedBox, Container, CornerRadius, CrossAxisAlignment, Flex, Hoverable,
        MainAxisAlignment, MainAxisSize, MouseStateHandle, ParentElement, Radius, Shrinkable, Text,
    },
    platform::Cursor,
};

use remote_server::setup::UnsupportedReason;

use crate::{
    Appearance,
    terminal::model::session::{LegacySshFallbackReason, SessionId},
    ui_components::icons::Icon,
};

const BANNER_BODY: &str = "While advanced features like file browsing and code review are currently \
    disabled, the rest of your Phosphorized experience is fully available.";

#[derive(Clone, Debug)]
pub enum SshRemoteServerFailedBannerAction {
    Dismiss,
}

#[derive(Clone, Debug)]
pub enum SshRemoteServerFailedBannerEvent {
    Dismissed,
}

#[derive(Clone, Copy, Debug)]
pub enum SshRemoteServerFailureKind {
    BinaryCheck,
    BinaryInstall,
    Launch,
    /// The preinstall check classified this host as incompatible with the
    /// prebuilt remote-server binary (old glibc, non-glibc libc, ...) and the
    /// controller deliberately fell back to the legacy ControlMaster-backed
    /// SSH flow -- nothing failed, the host was correctly declined. Distinct
    /// from the other three kinds (which ARE failures): until this variant
    /// existed, nothing told the user this session fell back to the legacy
    /// path at all, let alone why (TODO.md "Remote-session setup degrades
    /// silently", item 1).
    Unsupported,
    /// The command executor for this legacy SSH session was constructed
    /// without ever reaching a connected remote-server client, and without
    /// an explicit `Failed`/`Unsupported` state recorded for it either --
    /// the feature flag may be off, or the connection may simply not have
    /// completed yet. This is the "default fully-silent case" of TODO.md
    /// "Remote-session setup degrades silently", item 1: previously nothing
    /// at all distinguished this session from a fully phosphorized one.
    Legacy,
    /// The install was refused because this build has no pinned SHA-256 for
    /// the remote-server tarball (`install_remote_server.sh` exit 4 -- every
    /// local/unofficial build). Fail-closed is correct here; what was wrong
    /// was the banner, which reported it as `BinaryInstall` ("could not be
    /// written or executed"), a claim nothing in this path ever tested
    /// (#1086). The real reason was only in the red detail box.
    UnverifiedBuild,
    /// The install was refused because the download could not be verified
    /// against the pinned SHA-256: the remote host has no digest tool
    /// (exit 5), or the bytes did not match (exit 6). Like `UnverifiedBuild`
    /// it is not a write/execute failure, so it must not say it is (#1086).
    VerificationRefused,
}

impl SshRemoteServerFailureKind {
    /// The banner kind for a failed install, from the install error text.
    ///
    /// The install result reaches the view only as a `String`, so the
    /// integrity refusals are recognised by the headlines
    /// `ssh_transport::route_install_failure` prefixes onto them -- shared
    /// constants in `remote_server::setup`, so the two cannot drift apart
    /// silently. Anything else stays `BinaryInstall` (#1086).
    pub fn for_install_error(error: &str) -> Self {
        use remote_server::setup::{
            INSTALL_REFUSED_DIGEST_MISMATCH, INSTALL_REFUSED_NO_DIGEST_TOOL,
            INSTALL_REFUSED_NO_PINNED_DIGEST,
        };
        let error = error.trim_start();
        if error.starts_with(INSTALL_REFUSED_NO_PINNED_DIGEST) {
            Self::UnverifiedBuild
        } else if error.starts_with(INSTALL_REFUSED_NO_DIGEST_TOOL)
            || error.starts_with(INSTALL_REFUSED_DIGEST_MISMATCH)
        {
            Self::VerificationRefused
        } else {
            Self::BinaryInstall
        }
    }

    fn title(self) -> String {
        match self {
            Self::BinaryCheck => "SSH extension couldn't be verified".to_string(),
            Self::BinaryInstall => "SSH extension couldn't be installed".to_string(),
            Self::Launch => "SSH extension couldn't be started".to_string(),
            Self::Unsupported => crate::t!("terminal-ssh-remote-server-unsupported-title"),
            Self::Legacy => crate::t!("terminal-ssh-remote-server-legacy-title"),
            Self::UnverifiedBuild => {
                crate::t!("terminal-ssh-remote-server-unverified-build-title")
            }
            Self::VerificationRefused => {
                crate::t!("terminal-ssh-remote-server-verification-refused-title")
            }
        }
    }

    fn description(self) -> String {
        match self {
            Self::BinaryCheck => {
                "The SSH extension binary could not be verified on the remote host.".to_string()
            }
            Self::BinaryInstall => {
                "The binary could not be written or executed on the remote host.".to_string()
            }
            Self::Launch => {
                "The SSH extension could not be started on the remote host.".to_string()
            }
            Self::Unsupported => crate::t!("terminal-ssh-remote-server-unsupported-description"),
            Self::Legacy => crate::t!("terminal-ssh-remote-server-legacy-description"),
            Self::UnverifiedBuild => {
                crate::t!("terminal-ssh-remote-server-unverified-build-description")
            }
            Self::VerificationRefused => {
                crate::t!("terminal-ssh-remote-server-verification-refused-description")
            }
        }
    }
}

/// Explains, in one line, why the preinstall check declined this host for the
/// remote-server extension. Shown as the [`SshRemoteServerFailedBanner`]'s
/// detail line for [`SshRemoteServerFailureKind::Unsupported`].
///
/// Pure and unit-tested on its own (see `ssh_remote_server_failed_banner_tests.rs`)
/// so the mapping from [`UnsupportedReason`] to user-facing text can be
/// checked without constructing any view.
pub fn describe_unsupported_reason(reason: &UnsupportedReason) -> String {
    match reason {
        UnsupportedReason::GlibcTooOld { detected, required } => crate::t!(
            "terminal-ssh-remote-server-unsupported-glibc-detail",
            detected = detected.to_string(),
            required = required.to_string()
        ),
        UnsupportedReason::NonGlibc { name } => crate::t!(
            "terminal-ssh-remote-server-unsupported-non-glibc-detail",
            name = name.clone()
        ),
    }
}

/// Explains, in one line, why this legacy SSH session never reached a
/// connected remote-server client. Shown as the
/// [`SshRemoteServerFailedBanner`]'s detail line for
/// [`SshRemoteServerFailureKind::Legacy`].
///
/// Pure and unit-tested on its own (see `ssh_remote_server_failed_banner_tests.rs`)
/// so the mapping from [`LegacySshFallbackReason`] to user-facing text can be
/// checked without constructing any view.
pub fn describe_legacy_fallback_reason(reason: &LegacySshFallbackReason) -> String {
    match reason {
        LegacySshFallbackReason::FeatureDisabled => {
            crate::t!("terminal-ssh-remote-server-legacy-feature-disabled-detail")
        }
        LegacySshFallbackReason::NoConnectedClient => {
            crate::t!("terminal-ssh-remote-server-legacy-no-client-detail")
        }
    }
}

/// Whether a legacy-SSH fallback deserves a banner at all (#916). Declining
/// the install ("Continue without installing", or the `NeverInstall` mode) is
/// a legitimate choice and surfaces as `NoConnectedClient`; showing a red
/// error for it was a regression against v0.1.12. Only the feature-disabled
/// case, which the user did not choose, keeps a (neutral) note.
pub fn legacy_fallback_shows_banner(reason: &LegacySshFallbackReason) -> bool {
    match reason {
        LegacySshFallbackReason::FeatureDisabled => true,
        LegacySshFallbackReason::NoConnectedClient => false,
    }
}

pub struct SshRemoteServerFailedBanner {
    session_id: SessionId,
    kind: SshRemoteServerFailureKind,
    error: String,
    close_mouse_state: MouseStateHandle,
}

impl SshRemoteServerFailedBanner {
    pub fn new(session_id: SessionId, kind: SshRemoteServerFailureKind, error: String) -> Self {
        Self {
            session_id,
            kind,
            error,
            close_mouse_state: MouseStateHandle::default(),
        }
    }

    pub fn session_id(&self) -> SessionId {
        self.session_id
    }
}

impl Entity for SshRemoteServerFailedBanner {
    type Event = SshRemoteServerFailedBannerEvent;
}

impl View for SshRemoteServerFailedBanner {
    fn ui_name() -> &'static str {
        "SshRemoteServerFailedBanner"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let theme = appearance.theme();
        let fg_color = theme.foreground().into_solid();
        let muted_color = internal_colors::neutral_5(theme);
        let font_size = appearance.monospace_font_size();
        let small_font_size = font_size - 2.;

        // Alert-circle icon
        let icon = Container::new(
            ConstrainedBox::new(Icon::AlertCircle.to_warpui_icon(fg_color.into()).finish())
                .with_width(16.)
                .with_height(16.)
                .finish(),
        )
        .with_margin_right(8.)
        .finish();

        let title = Text::new(self.kind.title(), appearance.ui_font_family(), font_size)
            .with_color(fg_color)
            .finish();

        let body_text = format!("{} {BANNER_BODY}", self.kind.description());
        let body = Text::new(body_text, appearance.ui_font_family(), small_font_size)
            .soft_wrap(true)
            .with_color(muted_color)
            .finish();

        // Error detail line wrapped in a red-tinted background container.
        let trimmed_error = self.error.trim();
        let error_element = if trimmed_error.is_empty() {
            None
        } else {
            let ansi_red = AnsiColorIdentifier::Red.to_ansi_color(&theme.terminal_colors().normal);
            let error_bg = theme.ansi_overlay_1(ansi_red);
            let error_text_color = theme.ansi_fg_red();

            let error_text = Text::new(
                format!("ERROR: {trimmed_error}"),
                appearance.ui_font_family(),
                small_font_size,
            )
            .soft_wrap(true)
            .with_color(error_text_color)
            .finish();

            let error_container = Container::new(error_text)
                .with_background(error_bg)
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.)))
                .with_uniform_padding(12.)
                .finish();

            Some(
                Container::new(error_container)
                    .with_margin_top(8.)
                    .with_margin_left(24.)
                    .finish(),
            )
        };

        // Close (X) button
        let close_icon_color = muted_color;
        let close = Hoverable::new(self.close_mouse_state.clone(), move |_| {
            ConstrainedBox::new(Icon::X.to_warpui_icon(close_icon_color.into()).finish())
                .with_width(16.)
                .with_height(16.)
                .finish()
        })
        .with_cursor(Cursor::PointingHand)
        .on_click(|ctx, _, _| {
            ctx.dispatch_typed_action(SshRemoteServerFailedBannerAction::Dismiss);
        })
        .finish();

        let close_container = Container::new(close).with_uniform_padding(4.).finish();

        // Header row: [icon + title] ... [close]
        let header_row = Flex::row()
            .with_main_axis_size(MainAxisSize::Max)
            .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(
                Shrinkable::new(
                    1.,
                    Flex::row()
                        .with_cross_axis_alignment(CrossAxisAlignment::Center)
                        .with_child(icon)
                        .with_child(Shrinkable::new(1., title).finish())
                        .finish(),
                )
                .finish(),
            )
            .with_child(close_container)
            .finish();

        // Body text indented past the icon to align with the title
        let body_container = Container::new(body)
            .with_margin_top(2.)
            .with_margin_left(24.)
            .finish();

        let mut content = Flex::column()
            .with_main_axis_size(MainAxisSize::Min)
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_child(header_row)
            .with_child(body_container);

        if let Some(error) = error_element {
            content = content.with_child(error);
        }

        let content = content.finish();

        Container::new(content)
            .with_background(internal_colors::fg_overlay_1(theme))
            .with_uniform_padding(12.)
            .finish()
    }
}

impl TypedActionView for SshRemoteServerFailedBanner {
    type Action = SshRemoteServerFailedBannerAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            SshRemoteServerFailedBannerAction::Dismiss => {
                ctx.emit(SshRemoteServerFailedBannerEvent::Dismissed);
            }
        }
    }
}

#[cfg(test)]
#[path = "ssh_remote_server_failed_banner_tests.rs"]
mod tests;
