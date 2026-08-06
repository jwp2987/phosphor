//! The "update status" row (check-for-update / downloading / ready-to-install
//! / GitHub-fallback link) for the About page.
//!
//! **Parked, not wired up.** [`super::SHOW_AUTOUPDATE_UI`] is `false` for this
//! fork — there is no release channel to check against yet — so
//! [`AboutPageWidget::render_update_status`](super::AboutPageWidget::render_update_status)
//! is never called. The code is kept here (rather than deleted) because it's
//! a real, working implementation that upstream Warp exercises; re-enabling
//! is meant to be "flip `SHOW_AUTOUPDATE_UI` back to `true`", not
//! "reimplement this from scratch".
//!
//! Before flipping the flag back on, re-verify:
//! - The GitHub-fallback URL below, which currently points at this repo
//!   (`jwp2987/phosphor`) as a placeholder — confirm it's still the right
//!   releases page once real release automation exists.
//! - That `crate::autoupdate` / `github::cached_release` are still wired to a
//!   real release feed for this fork (they were last exercised against
//!   Warp's own channel infrastructure).
//!
//! This code has no dedicated test coverage beyond [`format_bytes`] /
//! [`format_download_progress`] below (see `autoupdate_ui_tests.rs`) — the
//! UI-building half isn't unit-testable in isolation from the rest of
//! [`super::AboutPageWidget`].

use crate::appearance::Appearance;
use crate::autoupdate::{self, github, AutoupdateStage};
use warpui::elements::{
    Container, CrossAxisAlignment, Element, Flex, MainAxisAlignment, ParentElement,
};
use warpui::ui_components::components::UiComponent;
use warpui::{AppContext, SingletonEntity};

use super::{AboutPageAction, AboutPageWidget};

impl AboutPageWidget {
    /// Renders the "update status" row: status text + action link (check for updates /
    /// progress display / install now / GitHub fallback).
    pub(super) fn render_update_status(
        &self,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let ui_builder = appearance.ui_builder();

        // The current stage determines the copy and action:
        // - NoUpdateAvailable / unknown error: already up to date + "Check for updates"
        // - CheckingForUpdate: checking... (no action)
        // - DownloadingUpdate: downloading X% (X MB / Y MB) (no action)
        // - UpdateReady / UpdatedPendingRestart: ready to install + "Install now" button
        // - UnableTo*: automatic install failed + "Download from GitHub" fallback link
        let stage = autoupdate::get_update_state(app);
        let progress = autoupdate::AutoupdateState::as_ref(app).download_progress().cloned();

        let (status_text, action) = match &stage {
            AutoupdateStage::CheckingForUpdate => (
                crate::t!("settings-about-update-checking"),
                UpdateAction::None,
            ),
            AutoupdateStage::DownloadingUpdate => {
                // Shared across all three platforms: gets the downloaded bytes from
                // AutoupdateState.download_progress and formats them as
                // "X.X MB / Y.Y MB (P%)"; shows only the downloaded bytes if the total size is
                // unknown.
                let new_version = stage
                    .available_new_version()
                    .map(|v| v.version.as_str())
                    .unwrap_or("");
                let text = match &progress {
                    Some(p) => {
                        // i18n_embed_fl::fl! requires the argument to be a reference with a
                        // lifetime, so bind the progress string to a let first rather than
                        // inlining a temporary expression.
                        let progress_str = format_download_progress(p);
                        crate::t!(
                            "settings-about-update-downloading",
                            version = new_version,
                            progress = progress_str.as_str()
                        )
                    }
                    None => crate::t!(
                        "settings-about-update-downloading-init",
                        version = new_version
                    ),
                };
                (text, UpdateAction::None)
            }
            AutoupdateStage::NoUpdateAvailable => (
                crate::t!("settings-about-update-up-to-date"),
                UpdateAction::Check,
            ),
            AutoupdateStage::UpdateReady { new_version, .. }
            | AutoupdateStage::UpdatedPendingRestart { new_version } => {
                let text = crate::t!(
                    "settings-about-update-ready",
                    version = new_version.version.as_str()
                );
                (text, UpdateAction::Install)
            }
            stage if stage.available_new_version().is_some() => {
                // UnableToUpdateToNewVersion / UnableToLaunchNewVersion / Updating (leftover):
                // automatic install errored out or was interrupted -> give the user a manual
                // download fallback.
                let new_version = stage.available_new_version().unwrap();
                let text = crate::t!(
                    "settings-about-update-available",
                    version = new_version.version.as_str()
                );
                // Placeholder fallback: see the module doc for why this needs
                // re-checking before SHOW_AUTOUPDATE_UI is ever flipped back on.
                let url = github::cached_release()
                    .map(|r| r.html_url)
                    .unwrap_or_else(|| {
                        "https://github.com/jwp2987/phosphor/releases/latest".to_owned()
                    });
                (text, UpdateAction::OpenReleasePage(url))
            }
            // Fallback (theoretically unreachable): any remaining stage is treated as "already
            // up to date".
            _ => (
                crate::t!("settings-about-update-up-to-date"),
                UpdateAction::Check,
            ),
        };

        let mut row = Flex::row()
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(ui_builder.span(status_text).build().finish());

        match action {
            UpdateAction::None => {}
            UpdateAction::Check => {
                row.add_child(
                    Container::new(
                        ui_builder
                            .link(
                                crate::t!("settings-about-update-check-now"),
                                None,
                                Some(Box::new(|ctx| {
                                    ctx.dispatch_typed_action(AboutPageAction::CheckForUpdate);
                                })),
                                self.update_action_link_mouse_state.clone(),
                            )
                            .soft_wrap(false)
                            .build()
                            .finish(),
                    )
                    .with_padding_left(8.)
                    .finish(),
                );
            }
            UpdateAction::OpenReleasePage(url) => {
                let url_clone = url.clone();
                row.add_child(
                    Container::new(
                        ui_builder
                            .link(
                                crate::t!("settings-about-update-open-release"),
                                None,
                                Some(Box::new(move |ctx| {
                                    ctx.dispatch_typed_action(AboutPageAction::OpenReleasePage(
                                        url_clone.clone(),
                                    ));
                                })),
                                self.update_action_link_mouse_state.clone(),
                            )
                            .soft_wrap(false)
                            .build()
                            .finish(),
                    )
                    .with_padding_left(8.)
                    .finish(),
                );
            }
            UpdateAction::Install => {
                row.add_child(
                    Container::new(
                        ui_builder
                            .link(
                                crate::t!("settings-about-update-install-now"),
                                None,
                                Some(Box::new(|ctx| {
                                    ctx.dispatch_typed_action(AboutPageAction::InstallUpdate);
                                })),
                                self.update_action_link_mouse_state.clone(),
                            )
                            .soft_wrap(false)
                            .build()
                            .finish(),
                    )
                    .with_padding_left(8.)
                    .finish(),
                );
            }
        }

        // Install hint: only shown in the UpdateReady/UpdatedPendingRestart state (Install
        // action), letting the user know ahead of clicking what happens next (open dmg /
        // launch install wizard / restart AppImage).
        if matches!(
            autoupdate::get_update_state(app),
            AutoupdateStage::UpdateReady { .. } | AutoupdateStage::UpdatedPendingRestart { .. }
        ) {
            // t! is a macro and must be passed a literal, not a variable, so the specific key is
            // picked per cfg branch.
            #[cfg(target_os = "macos")]
            let hint = crate::t!("settings-about-update-install-hint-macos");
            #[cfg(windows)]
            let hint = crate::t!("settings-about-update-install-hint-windows");
            #[cfg(all(not(target_os = "macos"), not(windows)))]
            let hint = crate::t!("settings-about-update-install-hint-linux");

            return Flex::column()
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_child(row.finish())
                .with_child(
                    ui_builder
                        .span(hint)
                        .with_soft_wrap()
                        .build()
                        .with_margin_top(4.)
                        .finish(),
                )
                .finish();
        }

        row.finish()
    }
}

/// Formats a byte count as "X.X MB" / "X KB", used for download-progress copy.
fn format_bytes(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

/// Renders a DownloadProgress as "1.2 MB / 3.4 MB (35%)"; shows only the downloaded amount when
/// the total is unknown.
fn format_download_progress(p: &autoupdate::DownloadProgress) -> String {
    let downloaded = format_bytes(p.downloaded);
    match p.total {
        Some(total) if total > 0 => {
            let pct = ((p.downloaded as f64 / total as f64) * 100.0).clamp(0.0, 100.0);
            format!("{} / {} ({:.0}%)", downloaded, format_bytes(total), pct)
        }
        _ => downloaded,
    }
}

/// The action to show in the update status area: none / check for updates / open GitHub
/// release / install now.
enum UpdateAction {
    None,
    Check,
    OpenReleasePage(String),
    Install,
}

#[cfg(test)]
#[path = "autoupdate_ui_tests.rs"]
mod tests;
