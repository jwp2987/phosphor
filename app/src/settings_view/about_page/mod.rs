use super::{
    settings_page::{
        render_body_item, MatchData, PageType, SettingsPageEvent, SettingsPageMeta,
        SettingsPageViewHandle, SettingsWidget,
    },
    LocalOnlyIconState, SettingsSection, ToggleState,
};
use crate::{
    appearance::Appearance,
    autoupdate::AutoupdateState,
    channel::ChannelState,
    report_if_error,
    settings::AutoupdateSettings,
    workspace::WorkspaceAction,
};
use settings::Setting as _;
use warp_core::{execution_mode::AppExecutionMode, settings::ToggleableSetting as _};
use warpui::ui_components::switch::SwitchStateHandle;
use warpui::{
    assets::asset_cache::AssetSource,
    elements::{
        Align, CacheOption, ConstrainedBox, Container, CrossAxisAlignment, Element, Flex, Image,
        MainAxisAlignment, MouseStateHandle, ParentElement, Wrap,
    },
    ui_components::components::UiComponent,
    AppContext, Entity, SingletonEntity, TypedActionView, View, ViewContext, ViewHandle,
};

// Parked, not deleted: this holds the "update status" row (check-for-update /
// downloading / ready-to-install / GitHub-fallback) so it doesn't rot silently
// in the middle of this file while `SHOW_AUTOUPDATE_UI` is `false`. See its
// module doc for why it's unused and what to check before flipping the flag.
mod autoupdate_ui;

#[derive(Debug, Clone)]
pub enum AboutPageAction {
    ToggleAutomaticUpdates,
    /// The user clicked the "Check for updates" button: proactively triggers a check
    /// (equivalent to RequestType::ManualCheck).
    CheckForUpdate,
    /// The user clicked the "Download from GitHub" link: opens the release page in the
    /// system's default browser.
    /// Only used on the error-fallback path (e.g. download failed / no available asset).
    OpenReleasePage(String),
    /// The user clicked the "Install now" link: dispatches to the workspace, triggering an
    /// install+restart flow fully equivalent to the menu's `ApplyUpdate`. See
    /// `autoupdate::apply_update` for the specific per-platform behavior.
    InstallUpdate,
    /// The user clicked the "Export logs" link: pops up the native save-file dialog; once the
    /// user picks a save location, the main log, MCP log, autoupdater log, and diagnostic
    /// summary are packaged into a zip and written directly to the user-specified path,
    /// reporting success / failure via a workspace toast when done.
    /// Implemented by `WorkspaceAction::ExportLogsToPath`.
    #[cfg(not(target_family = "wasm"))]
    ExportLogs,
}

pub struct AboutPageView {
    page: PageType<Self>,
}

/// Whether the autoupdate UI (update-status row + "Automatic updates" toggle) is
/// shown on the About page. Hidden for this fork: there is no Phosphor release
/// channel to update from (the upstream release URLs point at Warp/Zap), so those
/// controls would be misleading. Flip to `true` to restore them.
const SHOW_AUTOUPDATE_UI: bool = false;

impl AboutPageView {
    pub fn new(ctx: &mut ViewContext<AboutPageView>) -> Self {
        // Only subscribe to AutoupdateState when the autoupdate UI is actually
        // shown; otherwise its stage changes would needlessly re-render the page.
        if SHOW_AUTOUPDATE_UI {
            let autoupdate_handle = AutoupdateState::handle(ctx);
            ctx.observe(&autoupdate_handle, |_, _, ctx| {
                ctx.notify();
            });
        }

        AboutPageView {
            page: PageType::new_monolith(AboutPageWidget::default(), None, false),
        }
    }
}

impl Entity for AboutPageView {
    type Event = SettingsPageEvent;
}

impl TypedActionView for AboutPageView {
    type Action = AboutPageAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            AboutPageAction::ToggleAutomaticUpdates => {
                AutoupdateSettings::handle(ctx).update(ctx, |settings, ctx| {
                    report_if_error!(settings
                        .automatic_updates_enabled
                        .toggle_and_save_value(ctx));
                });
                ctx.notify();
            }
            AboutPageAction::CheckForUpdate => {
                AutoupdateState::handle(ctx).update(ctx, |state, ctx| {
                    state.manually_check_for_update(ctx);
                });
                ctx.notify();
            }
            AboutPageAction::OpenReleasePage(url) => {
                ctx.open_url(url);
            }
            AboutPageAction::InstallUpdate => {
                // Reuses WorkspaceAction::ApplyUpdate: it calls autoupdate::apply_update +
                // initiate_relaunch_for_update, and the platform layer decides the specific
                // install action inside relaunch() (mac OSS: open dmg / Win OSS: non-silent
                // install wizard / Linux: restart new binary).
                ctx.dispatch_typed_action(&WorkspaceAction::ApplyUpdate);
            }
            #[cfg(not(target_family = "wasm"))]
            AboutPageAction::ExportLogs => {
                // Triggers the workspace layer to pop up the save-file dialog; once the user
                // picks a save path, packaging completes and a toast reports the result.
                ctx.dispatch_typed_action(&WorkspaceAction::ExportLogsToPath);
            }
        }
    }
}

impl View for AboutPageView {
    fn ui_name() -> &'static str {
        "AboutPage"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        self.page.render(self, app)
    }
}

#[derive(Default)]
struct AboutPageWidget {
    copy_version_button_mouse_state: MouseStateHandle,
    automatic_updates_switch_state: SwitchStateHandle,
    update_action_link_mouse_state: MouseStateHandle,
    /// Hover / pressed state for the "Export logs" link.
    #[cfg(not(target_family = "wasm"))]
    export_logs_link_mouse_state: MouseStateHandle,
}

impl SettingsWidget for AboutPageWidget {
    type View = AboutPageView;

    fn search_terms(&self) -> &str {
        // Autoupdate terms omitted while SHOW_AUTOUPDATE_UI is false — searching
        // for them shouldn't surface a page that no longer has those controls.
        "about version copyright export logs"
    }

    fn render(
        &self,
        _view: &AboutPageView,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let ui_builder = appearance.ui_builder();

        // Phosphor brand badge; the name is rendered as separate text below, from the channel's
        // display name, no longer relying on an svg that includes the word "warp".
        let image_path = "bundled/jpg/phosphor-logo.jpeg";

        // GIT_RELEASE_TAG injected -> shows the tag; otherwise falls into Dev development mode
        let version = ChannelState::app_version().unwrap_or("Dev");

        let version_text = ui_builder
            .span(version.to_string())
            .with_soft_wrap()
            .build()
            .with_margin_top(16.)
            .finish();

        let copy_version_icon = appearance
            .ui_builder()
            .copy_button(16., self.copy_version_button_mouse_state.clone())
            .build()
            .on_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(WorkspaceAction::CopyVersion(version));
            })
            .finish();

        let version_row = Wrap::row()
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_children([
                version_text,
                Container::new(copy_version_icon)
                    .with_margin_top(16.)
                    .with_padding_left(6.)
                    .finish(),
            ]);

        let mut content = Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(
                ConstrainedBox::new(
                    Image::new(
                        AssetSource::Bundled { path: image_path },
                        CacheOption::BySize,
                    )
                    .finish(),
                )
                .with_max_height(100.)
                .with_max_width(350.)
                .finish(),
            )
            .with_child(
                ui_builder
                    .span(ChannelState::display_name())
                    .build()
                    .with_margin_top(12.)
                    .finish(),
            )
            .with_child(version_row.finish());

        // Update status area: shows whether a new version is currently available, and provides
        // a "Check for updates" or "Download from GitHub" link.
        // Only rendered in an execution mode that can enter the autoupdate flow (shares its
        // condition with the "Automatic updates" toggle below).
        if SHOW_AUTOUPDATE_UI && AppExecutionMode::as_ref(app).can_autoupdate() {
            content.add_child(
                Container::new(self.render_update_status(appearance, app))
                    .with_margin_top(16.)
                    .finish(),
            );
        }

        content.add_child(
            ui_builder
                .span(crate::t!("settings-about-copyright"))
                .build()
                .with_margin_top(16.)
                .finish(),
        );

        // "Export logs" link: platform-native export of a zip to share with support staff.
        // Skipped on WASM, which has no filesystem logs.
        #[cfg(not(target_family = "wasm"))]
        {
            let export_link = ui_builder
                .link(
                    crate::t!("settings-about-export-logs"),
                    None,
                    Some(Box::new(|ctx| {
                        ctx.dispatch_typed_action(AboutPageAction::ExportLogs);
                    })),
                    self.export_logs_link_mouse_state.clone(),
                )
                .soft_wrap(false)
                .build()
                .finish();

            // Uses a vertical Flex column to present both the link and the description text
            // (explaining why to export and what it contains).
            let export_section = Flex::column()
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_child(export_link)
                .with_child(
                    ui_builder
                        .span(crate::t!("settings-about-export-logs-description"))
                        .with_soft_wrap()
                        .build()
                        .with_margin_top(4.)
                        .finish(),
                )
                .finish();

            content.add_child(Container::new(export_section).with_margin_top(16.).finish());
        }

        if SHOW_AUTOUPDATE_UI && AppExecutionMode::as_ref(app).can_autoupdate() {
            content.add_child(
                Container::new(
                    ConstrainedBox::new(render_body_item::<AboutPageAction>(
                        crate::t!("settings-about-automatic-updates-label"),
                        None,
                        LocalOnlyIconState::Hidden,
                        ToggleState::Enabled,
                        appearance,
                        appearance
                            .ui_builder()
                            .switch(self.automatic_updates_switch_state.clone())
                            .check(
                                *AutoupdateSettings::as_ref(app)
                                    .automatic_updates_enabled
                                    .value(),
                            )
                            .build()
                            .on_click(move |ctx, _, _| {
                                ctx.dispatch_typed_action(AboutPageAction::ToggleAutomaticUpdates);
                            })
                            .finish(),
                        Some(crate::t!("settings-about-automatic-updates-description")),
                    ))
                    .with_max_width(520.)
                    .finish(),
                )
                .with_margin_top(24.)
                .finish(),
            );
        }

        Align::new(content.finish()).finish()
    }
}

impl SettingsPageMeta for AboutPageView {
    fn section() -> SettingsSection {
        SettingsSection::About
    }

    fn should_render(&self, _ctx: &AppContext) -> bool {
        true
    }

    fn update_filter(&mut self, query: &str, ctx: &mut ViewContext<Self>) -> MatchData {
        self.page.update_filter(query, ctx)
    }

    fn scroll_to_widget(&mut self, widget_id: &'static str) {
        self.page.scroll_to_widget(widget_id)
    }

    fn clear_highlighted_widget(&mut self) {
        self.page.clear_highlighted_widget();
    }
}

impl From<ViewHandle<AboutPageView>> for SettingsPageViewHandle {
    fn from(view_handle: ViewHandle<AboutPageView>) -> Self {
        SettingsPageViewHandle::About(view_handle)
    }
}
