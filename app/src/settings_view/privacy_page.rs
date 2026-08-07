//! The Privacy settings page.
//!
//! Ported from Warp (`app/src/settings_view/privacy_page.rs`). Before this port the fork
//! defined and read `SafeModeEnabled`, `SecretDisplayModeSetting`, `HideSecretsInBlockList`,
//! `IsCrashReportingEnabled` and `IsTelemetryEnabled` at runtime but shipped no GUI control
//! for any of them, because the page had never been ported (see issue #2).
//!
//! What was deliberately **not** ported, and why:
//!
//! * `CloudConversationStorageWidget` — backed by `IsCloudConversationStorageEnabled`, a
//!   setting this fork removed together with cloud conversation storage. There is nothing
//!   local for the toggle to control.
//! * `NetworkLogWidget` — the network-log console (`pane_group/pane/network_log_pane.rs`,
//!   `server/network_log_view.rs`, `warp_server_client::network_logging`) was removed from
//!   this fork, so the "View network logging" link has no destination.
//! * `DataManagementWidget` — opens Warp's hosted account-deletion page via
//!   `AuthManager::open_url_maybe_with_anonymous_token`. Both the page and that auth manager
//!   are part of the Warp cloud account, which this fork does not have.
//! * The "Read more about Warp's use of data" link inside the app-analytics widget — it
//!   points at `docs.warp.dev`, a Warp-corporate document that does not describe this fork.
//!
//! Behavioral adaptations, all of which keep the user-visible behavior Warp intends:
//!
//! * `CrashReportsWidget::should_render` is gated on `FeatureFlag::CrashReporting` instead of
//!   `ChannelState::is_crash_reporting_available()`. Warp's gate means "this build cannot ship
//!   crash reports, so the toggle is a no-op". In this fork `is_crash_reporting_available()` is
//!   hard-coded `false` (no crash reports are ever *uploaded*), but the setting is emphatically
//!   not a no-op: `crash_reporting::init` reads it and subscribes to it, and turning it on
//!   installs the panic hook in `init_local_crash_reporting` that writes a full backtrace to
//!   the local log. Applying Warp's gate verbatim would hide a control that still does
//!   something, which is the exact defect this page is being ported to fix.
//! * Hard-coded 12px font sizes are replaced with `appearance.ui_font_body()` /
//!   `ui_font_overline()`, matching this fork's conversion of `settings_page::CONTENT_FONT_SIZE`
//!   to scalable UI font sizes.
//! * User-visible strings go through `crate::t!`; Warp hard-codes English.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

use pathfinder_geometry::vector::vec2f;
use regex::Regex;
use settings::Setting as _;
use warp_core::features::FeatureFlag;
use warp_core::ui::theme::color::internal_colors;
use warp_core::ui::theme::WarpTheme;
use warpui::r#async::{SpawnedFutureHandle, Timer};
use warpui::elements::{
    Align, ChildAnchor, ChildView, ConstrainedBox, Container, CornerRadius, CrossAxisAlignment,
    Empty, Expanded, Flex, Hoverable, MainAxisAlignment, MainAxisSize, MouseStateHandle,
    OffsetPositioning, ParentAnchor, ParentElement, ParentOffsetBounds, Radius, Rect, Shrinkable,
    Stack, Text,
};
use warpui::fonts::Weight;
use warpui::keymap::ContextPredicate;
use warpui::platform::Cursor;
use warpui::ui_components::button::{ButtonVariant, TextAndIcon, TextAndIconAlignment};
use warpui::ui_components::components::{Coords, UiComponent, UiComponentStyles};
use warpui::ui_components::switch::{SwitchStateHandle, TooltipConfig};
use warpui::{
    Action, AppContext, Element, Entity, ModelHandle, SingletonEntity, TypedActionView, UpdateModel,
    View, ViewContext, ViewHandle,
};

use super::privacy::{AddRegexModal, AddRegexModalEvent, AddRegexModalViewState};
use super::settings_page::{
    render_body_item, render_sub_header, LocalOnlyIconState, MatchData, PageType, SettingsPageMeta,
    SettingsPageViewHandle, SettingsWidget, ToggleState, HEADER_PADDING,
    TOGGLE_BUTTON_RIGHT_PADDING,
};
use super::{flags, SettingsAction, SettingsSection, ToggleSettingActionPair};
use crate::appearance::Appearance;
use crate::channel::ChannelState;
use crate::modal::{Modal, ModalEvent, ModalViewState};
use crate::report_if_error;
use crate::send_telemetry_from_ctx;
use crate::server::telemetry::TelemetryEvent;
use crate::settings::{CustomSecretRegex, PrivacySettings, RegexDisplayInfo};
use crate::settings_view::render_body_item_label;
use crate::terminal::safe_mode_settings::{
    get_effective_secret_display_mode, SafeModeEnabled, SafeModeSettings, SecretDisplayMode,
    SecretDisplayModeSetting,
};
use crate::ui_components::buttons::icon_button;
use crate::ui_components::icons::Icon;
use crate::util::links::PRIVACY_POLICY_URL;
use crate::view_components::{Dropdown, DropdownItem};
use crate::workspaces::user_workspaces::UserWorkspaces;
use crate::workspaces::workspace::{CustomerType, UgcCollectionEnablementSetting};

pub struct PrivacyPageView {
    page: PageType<Self>,
    local_only_icon_tooltip_states: RefCell<HashMap<String, MouseStateHandle>>,
    /// This needs to mirror the length of PrivacySettings::user_secret_regex_list.
    added_user_secret_regex_list_button_handles: Vec<MouseStateHandle>,
    /// Set of indices for regex items that are pending removal
    pending_regex_removals: HashSet<usize>,
    /// Handle to the current debounce timer
    pending_timer: Option<SpawnedFutureHandle>,
    /// Modal state
    add_regex_modal_state: AddRegexModalViewState,
    /// Active tab for secret redaction settings
    active_secret_redaction_tab: SecretRedactionTab,
    /// Dropdown for selecting secret redaction display mode
    secret_redaction_display_dropdown: ViewHandle<Dropdown<PrivacyPageAction>>,
}

#[derive(Clone, Copy)]
pub enum PrivacyPageViewEvent {
    ShowAddRegexModal,
    HideAddRegexModal,
}

impl PrivacyPageView {
    const BATCH_TIMEOUT_MS: u64 = 700;

    pub fn new(ctx: &mut ViewContext<PrivacyPageView>) -> Self {
        let privacy_settings_handle = PrivacySettings::handle(ctx);
        ctx.observe(&privacy_settings_handle, |_, _, ctx| {
            // It is possible that PrivacySettings are updated without an interaction in this view,
            // so notify the view if the model is updated.
            ctx.notify();
        });
        ctx.observe(&privacy_settings_handle, Self::update_button_states);
        ctx.subscribe_to_model(&privacy_settings_handle, |me, model, _, ctx| {
            me.update_button_states(model, ctx);
            ctx.notify();
        });
        ctx.subscribe_to_model(&SafeModeSettings::handle(ctx), |me, _, _, ctx| {
            me.update_secret_display_dropdown(ctx);
            ctx.notify();
        });

        let add_regex_body = ctx.add_typed_action_view(AddRegexModal::new);
        ctx.subscribe_to_view(&add_regex_body, |me, _, event, ctx| {
            me.handle_add_regex_modal_event(event, ctx);
        });

        let add_regex_modal_view = ctx.add_typed_action_view(|ctx| {
            Modal::new(
                Some(crate::t!("settings-privacy-add-regex-modal-title")),
                add_regex_body,
                ctx,
            )
            .with_modal_style(UiComponentStyles {
                width: Some(600.),
                height: Some(400.),
                ..Default::default()
            })
            .with_header_style(UiComponentStyles {
                padding: Some(Coords {
                    top: 24.,
                    bottom: 0.,
                    left: 24.,
                    right: 24.,
                }),
                font_size: Some(16.),
                font_weight: Some(Weight::Bold),
                ..Default::default()
            })
            .with_body_style(UiComponentStyles {
                padding: Some(Coords {
                    top: 0.,
                    bottom: 24.,
                    left: 24.,
                    right: 24.,
                }),
                ..Default::default()
            })
            .with_background_opacity(100)
            .with_dismiss_on_click()
        });
        ctx.subscribe_to_view(&add_regex_modal_view, |me, _, event, ctx| {
            me.handle_modal_event(event, ctx);
        });

        let secret_display_dropdown = ctx.add_typed_action_view(|ctx| {
            let mut dropdown = Dropdown::new(ctx);
            dropdown.set_items(
                SecretDisplayMode::all_modes()
                    .iter()
                    .map(|mode| {
                        DropdownItem::new(
                            mode.display_name(),
                            PrivacyPageAction::SetSecretDisplayMode(*mode),
                        )
                    })
                    .collect(),
                ctx,
            );
            dropdown
        });

        let mut privacy_page_view = Self {
            page: Self::build_page(),
            local_only_icon_tooltip_states: Default::default(),
            added_user_secret_regex_list_button_handles: Default::default(),
            pending_regex_removals: Default::default(),
            pending_timer: None,
            add_regex_modal_state: AddRegexModalViewState::new(ModalViewState::new(
                add_regex_modal_view,
            )),
            active_secret_redaction_tab: SecretRedactionTab::Personal,
            secret_redaction_display_dropdown: secret_display_dropdown,
        };

        privacy_page_view.update_button_states(privacy_settings_handle, ctx);
        privacy_page_view.update_secret_display_dropdown(ctx);
        privacy_page_view
    }

    fn build_page() -> PageType<Self> {
        // `CloudConversationStorageWidget` / `NetworkLogWidget` / `DataManagementWidget` are
        // deliberately absent -- see the module doc for why.
        let widgets: Vec<Box<dyn SettingsWidget<View = Self>>> = vec![
            Box::new(SecretRedactionWidget::default()),
            Box::new(AppAnalyticsWidget::default()),
            Box::new(CrashReportsWidget::default()),
            Box::new(PrivacyPolicyWidget::default()),
        ];
        PageType::new_uncategorized(widgets, Some(crate::t_static!("settings-privacy-page-title")))
    }

    fn update_button_states(
        &mut self,
        privacy_settings_handle: ModelHandle<PrivacySettings>,
        ctx: &mut ViewContext<Self>,
    ) {
        let privacy_settings = privacy_settings_handle.as_ref(ctx);
        self.added_user_secret_regex_list_button_handles = privacy_settings
            .user_secret_regex_list
            .iter()
            .map(|_| Default::default())
            .collect();
    }

    fn toggle_safe_mode(&mut self, ctx: &mut ViewContext<Self>) {
        let safe_mode_settings = SafeModeSettings::handle(ctx);
        let new_value = { !*safe_mode_settings.as_ref(ctx).safe_mode_enabled.value() };

        send_telemetry_from_ctx!(
            TelemetryEvent::ToggleSecretRedaction { enabled: new_value },
            ctx
        );

        ctx.update_model(&safe_mode_settings, move |safe_mode_settings, ctx| {
            report_if_error!(safe_mode_settings.safe_mode_enabled.set_value(new_value, ctx));
        });
        ctx.notify();
    }

    fn toggle_hide_secrets_in_block_list(&mut self, ctx: &mut ViewContext<Self>) {
        let safe_mode_settings = SafeModeSettings::handle(ctx);
        let new_value = {
            !*safe_mode_settings
                .as_ref(ctx)
                .hide_secrets_in_block_list
                .value()
        };

        ctx.update_model(&safe_mode_settings, move |safe_mode_settings, ctx| {
            report_if_error!(safe_mode_settings
                .hide_secrets_in_block_list
                .set_value(new_value, ctx));
        });
        ctx.notify();
    }

    fn set_secret_display_mode(&mut self, mode: SecretDisplayMode, ctx: &mut ViewContext<Self>) {
        let safe_mode_settings = SafeModeSettings::handle(ctx);

        ctx.update_model(&safe_mode_settings, move |safe_mode_settings, ctx| {
            report_if_error!(safe_mode_settings.secret_display_mode.set_value(mode, ctx));
        });
        ctx.notify();
    }

    fn toggle_telemetry(&mut self, ctx: &mut ViewContext<Self>) {
        let privacy_settings_handle = PrivacySettings::handle(ctx);
        let old_value = privacy_settings_handle.as_ref(ctx).is_telemetry_enabled;
        ctx.update_model(&privacy_settings_handle, |privacy_settings, ctx| {
            privacy_settings.set_is_telemetry_enabled(!old_value, ctx);
        });
        ctx.notify();
    }

    fn toggle_crash_reporting(&mut self, ctx: &mut ViewContext<Self>) {
        let privacy_settings_handle = PrivacySettings::handle(ctx);
        let old_value = privacy_settings_handle
            .as_ref(ctx)
            .is_crash_reporting_enabled;
        ctx.update_model(&privacy_settings_handle, |privacy_settings, ctx| {
            privacy_settings.set_is_crash_reporting_enabled(!old_value, ctx);
        });
        ctx.notify();
    }

    fn queue_regex_removal(&mut self, idx: usize, ctx: &mut ViewContext<Self>) {
        // Check if this removal is already pending
        if self.pending_regex_removals.contains(&idx) {
            return;
        }

        if let Some(timer) = self.pending_timer.take() {
            timer.abort();
        }

        // Add to pending set
        self.pending_regex_removals.insert(idx);
        ctx.notify();

        // Start a new timer only if we don't have one
        if self.pending_timer.is_none() {
            let handle = ctx.spawn(
                async move {
                    Timer::after(Duration::from_millis(Self::BATCH_TIMEOUT_MS)).await;
                },
                |me, _, ctx| {
                    // Only process if we still have pending removals and a timer
                    // (they might have been processed by an add operation)
                    if !me.pending_regex_removals.is_empty() && me.pending_timer.is_some() {
                        me.pending_timer = None;
                        me.process_pending_removals(ctx);
                    }
                },
            );
            self.pending_timer = Some(handle);
        }
    }

    fn update_secret_display_dropdown(&mut self, ctx: &mut ViewContext<Self>) {
        let safe_mode_settings = SafeModeSettings::as_ref(ctx);

        let current_mode = get_effective_secret_display_mode(safe_mode_settings);
        self.secret_redaction_display_dropdown
            .update(ctx, |dropdown, ctx| {
                dropdown.set_selected_by_action(
                    PrivacyPageAction::SetSecretDisplayMode(current_mode),
                    ctx,
                );
            });
    }

    fn process_pending_removals(&mut self, ctx: &mut ViewContext<Self>) {
        let mut indices: Vec<_> = self.pending_regex_removals.iter().copied().collect();
        if indices.is_empty() {
            return;
        }
        // Sort in reverse order to remove from highest index first
        indices.sort_unstable_by(|a, b| b.cmp(a));

        let privacy_settings_handle = PrivacySettings::handle(ctx);
        for idx in indices {
            privacy_settings_handle.update(ctx, |privacy_settings, ctx| {
                privacy_settings.remove_user_secret_regex(&idx, ctx);
            });
        }

        self.pending_regex_removals.clear();
        ctx.notify();
    }

    fn show_add_regex_modal(&mut self, ctx: &mut ViewContext<Self>) {
        self.add_regex_modal_state.open(ctx);
        ctx.emit(PrivacyPageViewEvent::ShowAddRegexModal);
    }

    fn hide_add_regex_modal(&mut self, ctx: &mut ViewContext<Self>) {
        self.add_regex_modal_state.close(ctx);
        ctx.emit(PrivacyPageViewEvent::HideAddRegexModal);
    }

    fn handle_modal_event(&mut self, event: &ModalEvent, ctx: &mut ViewContext<Self>) {
        match event {
            ModalEvent::Close => {
                self.hide_add_regex_modal(ctx);
            }
        }
    }

    fn handle_add_regex_modal_event(
        &mut self,
        event: &AddRegexModalEvent,
        ctx: &mut ViewContext<Self>,
    ) {
        match event {
            AddRegexModalEvent::Close => {
                self.hide_add_regex_modal(ctx);
            }
            AddRegexModalEvent::Submit { name, pattern } => {
                self.add_custom_regex(name.clone(), pattern.clone(), ctx);
                self.hide_add_regex_modal(ctx);
            }
        }
    }

    fn add_custom_regex(&mut self, name: String, pattern: String, ctx: &mut ViewContext<Self>) {
        // First process any pending removals
        if !self.pending_regex_removals.is_empty() {
            self.process_pending_removals(ctx);
        }

        let privacy_settings_handle = PrivacySettings::handle(ctx);
        ctx.update_model(
            &privacy_settings_handle,
            |privacy_settings, ctx| match Regex::new(&pattern) {
                Ok(regex) => {
                    let mut new_user_secret_regex_list =
                        privacy_settings.user_secret_regex_list.to_vec();
                    new_user_secret_regex_list.push(CustomSecretRegex {
                        pattern: regex,
                        name: if name.trim().is_empty() {
                            None
                        } else {
                            Some(name.trim().to_string())
                        },
                    });

                    if privacy_settings
                        .user_secret_regex_list
                        .set_value(new_user_secret_regex_list, ctx)
                        .is_err()
                    {
                        log::error!("Failed to add custom regex to secret regex list");
                    }
                    ctx.notify();
                }
                Err(error) => {
                    log::error!("Invalid regex pattern {pattern}: {error}");
                }
            },
        );
    }

    pub fn get_modal_content(&self) -> Option<Box<dyn Element>> {
        if self.add_regex_modal_state.is_open() {
            Some(self.add_regex_modal_state.render())
        } else {
            None
        }
    }
}

impl View for PrivacyPageView {
    fn ui_name() -> &'static str {
        "PrivacyPageView"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        self.page.render(self, app)
    }
}

impl Entity for PrivacyPageView {
    type Event = PrivacyPageViewEvent;
}

#[derive(Clone, Debug, PartialEq)]
pub enum PrivacyPageAction {
    ToggleSafeMode,
    /// Toggles the legacy `HideSecretsInBlockList` setting. As in Warp, no widget on this page
    /// renders a control for it: it is superseded by `SetSecretDisplayMode` and only survives
    /// so that `get_effective_secret_display_mode` can migrate users who set it before the
    /// display-mode dropdown existed.
    ToggleHideSecretsInBlockList,
    SetSecretDisplayMode(SecretDisplayMode),
    ToggleTelemetry,
    ToggleCrashReporting,
    RemoveCustomRegex(usize),
    AddAllRecommendedRegexes,
    ShowAddRegexModal,
    AddRecommendedRegex(usize),
    SwitchSecretRedactionTab(SecretRedactionTab),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SecretRedactionTab {
    Personal,
    Enterprise,
}

impl TypedActionView for PrivacyPageView {
    type Action = PrivacyPageAction;

    fn handle_action(&mut self, action: &PrivacyPageAction, ctx: &mut ViewContext<Self>) {
        match action {
            PrivacyPageAction::AddRecommendedRegex(idx) => {
                // First process any pending removals
                if !self.pending_regex_removals.is_empty() {
                    self.process_pending_removals(ctx);
                }

                let privacy_settings_handle = PrivacySettings::handle(ctx);
                ctx.update_model(&privacy_settings_handle, |privacy_settings, ctx| {
                    let workspaces = UserWorkspaces::as_ref(ctx);
                    let enterprise_regex_list =
                        workspaces.get_enterprise_secret_redaction_regex_list();
                    let current_patterns: Vec<&str> = enterprise_regex_list
                        .iter()
                        .map(|s| s.pattern.as_str())
                        .chain(
                            privacy_settings
                                .user_secret_regex_list
                                .iter()
                                .map(|r| r.pattern().as_str()),
                        )
                        .collect();

                    let recommended_regexes: Vec<_> =
                        crate::terminal::model::secrets::regexes::DEFAULT_REGEXES_WITH_NAMES
                            .iter()
                            .filter(|r| !current_patterns.contains(&r.pattern))
                            .collect();

                    if let Some(regex) = recommended_regexes.get(*idx)
                        && let Ok(pattern) = Regex::new(regex.pattern)
                    {
                        let mut new_user_secret_regex_list =
                            privacy_settings.user_secret_regex_list.to_vec();
                        new_user_secret_regex_list.push(CustomSecretRegex {
                            pattern,
                            name: Some(regex.name.to_string()),
                        });

                        if privacy_settings
                            .user_secret_regex_list
                            .set_value(new_user_secret_regex_list, ctx)
                            .is_err()
                        {
                            log::error!(
                                "Failed to add recommended regex to custom secret regex list"
                            );
                        }
                        ctx.notify();
                    }
                });
            }
            PrivacyPageAction::ToggleSafeMode => self.toggle_safe_mode(ctx),
            PrivacyPageAction::ToggleHideSecretsInBlockList => {
                self.toggle_hide_secrets_in_block_list(ctx)
            }
            PrivacyPageAction::SetSecretDisplayMode(mode) => {
                self.set_secret_display_mode(*mode, ctx)
            }
            PrivacyPageAction::ToggleTelemetry => self.toggle_telemetry(ctx),
            PrivacyPageAction::ToggleCrashReporting => self.toggle_crash_reporting(ctx),
            PrivacyPageAction::RemoveCustomRegex(idx) => {
                self.queue_regex_removal(*idx, ctx);
            }
            PrivacyPageAction::AddAllRecommendedRegexes => {
                // First process any pending removals
                if !self.pending_regex_removals.is_empty() {
                    self.process_pending_removals(ctx);
                }

                let privacy_settings_handle = PrivacySettings::handle(ctx);
                ctx.update_model(&privacy_settings_handle, |privacy_settings, ctx| {
                    privacy_settings.add_all_recommended_regex(ctx);
                });
            }
            PrivacyPageAction::ShowAddRegexModal => {
                self.show_add_regex_modal(ctx);
            }
            PrivacyPageAction::SwitchSecretRedactionTab(tab) => {
                self.active_secret_redaction_tab = *tab;
                ctx.notify();
            }
        }
    }
}

impl SettingsPageMeta for PrivacyPageView {
    fn section() -> SettingsSection {
        SettingsSection::Privacy
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

impl From<ViewHandle<PrivacyPageView>> for SettingsPageViewHandle {
    fn from(view_handle: ViewHandle<PrivacyPageView>) -> Self {
        SettingsPageViewHandle::Privacy(view_handle)
    }
}

#[derive(Default)]
struct SecretRedactionWidget {
    switch_state: SwitchStateHandle,
    add_regex_button_mouse_state: MouseStateHandle,
    add_recommended_button_mouse_states: RefCell<Vec<MouseStateHandle>>,
    add_all_button_mouse_state: MouseStateHandle,
    personal_tab_mouse_state: MouseStateHandle,
    enterprise_tab_mouse_state: MouseStateHandle,
}

impl SecretRedactionWidget {
    /// Ensures there's enough mouse states for the recommended regexes to be added.
    fn ensure_recommended_regex_mouse_states(&self, count: usize) {
        while self.add_recommended_button_mouse_states.borrow().len() < count {
            self.add_recommended_button_mouse_states
                .borrow_mut()
                .push(Default::default());
        }
    }

    fn render_tab(
        &self,
        label: String,
        count: usize,
        tab_type: SecretRedactionTab,
        is_active: bool,
        mouse_state: MouseStateHandle,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let font_size = appearance.ui_font_body();
        let (background_fill, text_color, count_color) = if is_active {
            (
                Some(theme.surface_overlay_1()),
                theme.active_ui_text_color().into(),
                theme.sub_text_color(theme.surface_2()).into(),
            )
        } else {
            (
                None,
                theme.sub_text_color(theme.background()).into(),
                theme
                    .sub_text_color(theme.background())
                    .with_opacity(56)
                    .into(),
            )
        };

        let hover_background = if !is_active {
            Some(appearance.theme().surface_overlay_2())
        } else {
            None
        };

        Hoverable::new(mouse_state, move |mouse_state| {
            let is_hovered = mouse_state.is_hovered();

            let tab_content = Flex::row()
                .with_child(
                    Text::new_inline(label.clone(), appearance.ui_font_family(), font_size)
                        .with_color(text_color)
                        .finish(),
                )
                .with_child(
                    Container::new(
                        Text::new_inline(
                            format!(" {count}"),
                            appearance.ui_font_family(),
                            font_size,
                        )
                        .with_color(count_color)
                        .finish(),
                    )
                    .finish(),
                )
                .finish();

            let mut container = Container::new(tab_content)
                .with_vertical_padding(9.)
                .with_horizontal_padding(12.)
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(6.)));

            // Apply background based on hover state
            if is_hovered && !is_active {
                if let Some(hover_bg) = hover_background {
                    container = container.with_background(hover_bg);
                }
            } else if let Some(bg) = background_fill {
                container = container.with_background(bg);
            }

            container.finish()
        })
        .on_click(move |ctx, _, _| {
            ctx.dispatch_typed_action(PrivacyPageAction::SwitchSecretRedactionTab(tab_type));
        })
        .with_cursor(Cursor::PointingHand)
        .finish()
    }

    /// Renders the tab bar for switching between Personal and Enterprise views
    fn render_tab_bar(
        &self,
        appearance: &Appearance,
        privacy_settings: &PrivacySettings,
        active_tab: SecretRedactionTab,
        view: &PrivacyPageView,
        app: &AppContext,
    ) -> Box<dyn Element> {
        if !privacy_settings.is_enterprise_secret_redaction_enabled() {
            return Empty::new().finish();
        }

        let workspaces = UserWorkspaces::as_ref(app);
        let enterprise_regex_list = workspaces.get_enterprise_secret_redaction_regex_list();
        let enterprise_count = enterprise_regex_list.len();

        // Count personal regexes excluding pending removals
        let personal_count = privacy_settings
            .user_secret_regex_list
            .iter()
            .enumerate()
            .filter(|(i, _)| !view.pending_regex_removals.contains(i))
            .count();

        let personal_tab = self.render_tab(
            crate::t!("settings-privacy-tab-personal"),
            personal_count,
            SecretRedactionTab::Personal,
            active_tab == SecretRedactionTab::Personal,
            self.personal_tab_mouse_state.clone(),
            appearance,
        );

        let is_enterprise_tab_active = active_tab == SecretRedactionTab::Enterprise;

        let enterprise_tab = self.render_tab(
            crate::t!("settings-privacy-tab-enterprise"),
            enterprise_count,
            SecretRedactionTab::Enterprise,
            is_enterprise_tab_active,
            self.enterprise_tab_mouse_state.clone(),
            appearance,
        );

        let mut row = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(personal_tab)
            .with_child(enterprise_tab);

        if is_enterprise_tab_active {
            row.add_child(Shrinkable::new(1., Empty::new().finish()).finish());
            row.add_child(self.render_info(
                crate::t!("settings-privacy-enterprise-immutable"),
                appearance,
            ));
        }

        Container::new(row.finish())
            .with_margin_bottom(16.)
            .finish()
    }

    /// Renders a section title with consistent styling
    fn render_section_title(&self, title: String, appearance: &Appearance) -> Box<dyn Element> {
        Text::new_inline(title, appearance.ui_font_family(), appearance.ui_font_body())
            .with_color(appearance.theme().active_ui_text_color().into())
            .finish()
    }

    /// Renders a description paragraph with consistent styling
    fn render_description(
        &self,
        text: String,
        appearance: &Appearance,
        margin_bottom: f32,
    ) -> Box<dyn Element> {
        let description_text_color = description_text_color(appearance.theme()).into_solid();
        appearance
            .ui_builder()
            .paragraph(text)
            .with_style(UiComponentStyles {
                font_color: Some(description_text_color),
                margin: Some(
                    Coords::default()
                        .top(styles::DESCRIPTION_LINE_MARGIN_BOTTOM)
                        .bottom(margin_bottom),
                ),
                ..Default::default()
            })
            .build()
            .finish()
    }

    /// Renders a regex item with consistent container styling
    fn render_regex_item(
        &self,
        content: Box<dyn Element>,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let background = appearance.theme().surface_overlay_1();
        Container::new(content)
            .with_background(background)
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.)))
            .with_uniform_padding(8.)
            .with_margin_bottom(4.)
            .finish()
    }

    fn horizontal_divider(&self, appearance: &Appearance) -> Box<dyn Element> {
        Container::new(
            ConstrainedBox::new(
                Rect::new()
                    .with_background(appearance.theme().outline())
                    .finish(),
            )
            .with_height(1.)
            .finish(),
        )
        .with_vertical_margin(24.)
        .finish()
    }

    /// Renders regex content using the RegexDisplayInfo trait (supports both user and enterprise
    /// regexes)
    fn render_regex_content<T: RegexDisplayInfo>(
        &self,
        regex_info: &T,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let regex_color = internal_colors::fg_overlay_6(appearance.theme());
        let font_size = appearance.ui_font_body();

        if let Some(name) = regex_info.name() {
            Flex::column()
                .with_child(
                    Text::new_inline(name.to_string(), appearance.ui_font_family(), font_size)
                        .with_color(appearance.theme().active_ui_text_color().into())
                        .finish(),
                )
                .with_child(
                    Text::new_inline(
                        regex_info.pattern().to_string(),
                        appearance.ui_font_family(),
                        font_size,
                    )
                    .with_color(regex_color.into())
                    .finish(),
                )
                .finish()
        } else {
            Text::new_inline(
                regex_info.pattern().to_string(),
                appearance.ui_font_family(),
                font_size,
            )
            .with_color(regex_color.into())
            .finish()
        }
    }

    /// Renders the enterprise tab content (regexes with title support)
    fn render_enterprise_content(
        &self,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let workspaces = UserWorkspaces::as_ref(app);
        let enterprise_regex_list = workspaces.get_enterprise_secret_redaction_regex_list();
        let ui_builder = appearance.ui_builder();
        let description_text_color = description_text_color(appearance.theme()).into_solid();

        if enterprise_regex_list.is_empty() {
            return ui_builder
                .paragraph(crate::t!("settings-privacy-enterprise-empty"))
                .with_style(UiComponentStyles {
                    font_color: Some(description_text_color),
                    ..Default::default()
                })
                .build()
                .finish();
        }

        let mut column = Flex::column();

        for enterprise_regex in enterprise_regex_list {
            let content = self.render_regex_content(&enterprise_regex, appearance);
            let item = self.render_regex_item(
                Flex::row()
                    .with_cross_axis_alignment(CrossAxisAlignment::Center)
                    .with_child(Expanded::new(1., content).finish())
                    .finish(),
                appearance,
            );
            column.add_child(item);
        }

        column.finish()
    }

    /// Renders the personal tab content (user regexes + recommended regexes)
    fn render_personal_content(
        &self,
        view: &PrivacyPageView,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let privacy_settings = PrivacySettings::as_ref(app);
        let ui_builder = appearance.ui_builder();
        let workspaces = UserWorkspaces::as_ref(app);

        let mut column = Flex::column();

        for (i, regex) in privacy_settings.user_secret_regex_list.iter().enumerate() {
            if view.pending_regex_removals.contains(&i) {
                continue;
            }

            let text_content = self.render_regex_content(regex, appearance);

            let item = self.render_regex_item(
                Flex::row()
                    .with_cross_axis_alignment(CrossAxisAlignment::Center)
                    .with_child(Expanded::new(1., text_content).finish())
                    .with_child(
                        ui_builder
                            .close_button(
                                20., // diameter
                                view.added_user_secret_regex_list_button_handles[i].clone(),
                            )
                            .build()
                            .on_click(move |ctx, _, _| {
                                ctx.dispatch_typed_action(PrivacyPageAction::RemoveCustomRegex(i));
                            })
                            .finish(),
                    )
                    .finish(),
                appearance,
            );

            column.add_child(item);
        }

        // Get a list of regexes that are recommended but not currently in use
        let enterprise_regex_list_with_titles =
            workspaces.get_enterprise_secret_redaction_regex_list();
        let current_patterns: Vec<&str> = enterprise_regex_list_with_titles
            .iter()
            .map(|r| r.pattern.as_str())
            .chain(
                privacy_settings
                    .user_secret_regex_list
                    .iter()
                    .map(|r| r.pattern().as_str()),
            )
            .collect();

        let recommended_regexes: Vec<_> =
            crate::terminal::model::secrets::regexes::DEFAULT_REGEXES_WITH_NAMES
                .iter()
                .filter(|r| !current_patterns.contains(&r.pattern))
                .collect();

        if !recommended_regexes.is_empty() {
            column.add_child(self.horizontal_divider(appearance));

            // Add the "Recommended" header with "Add all" button
            column.add_child(
                Container::new(
                    Flex::row()
                        .with_main_axis_size(MainAxisSize::Max)
                        .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
                        .with_cross_axis_alignment(CrossAxisAlignment::Center)
                        .with_child(self.render_section_title(
                            crate::t!("settings-privacy-recommended"),
                            appearance,
                        ))
                        .with_child(
                            Container::new(
                                ui_builder
                                    .button(
                                        ButtonVariant::Secondary,
                                        self.add_all_button_mouse_state.clone(),
                                    )
                                    .with_text_and_icon_label(Self::add_button(
                                        crate::t!("settings-privacy-add-all"),
                                        appearance,
                                    ))
                                    .with_style(Self::add_button_style())
                                    .build()
                                    .on_click(move |ctx, _, _| {
                                        ctx.dispatch_typed_action(
                                            PrivacyPageAction::AddAllRecommendedRegexes,
                                        );
                                    })
                                    .finish(),
                            )
                            .with_margin_bottom(8.)
                            .finish(),
                        )
                        .finish(),
                )
                .finish(),
            );

            self.ensure_recommended_regex_mouse_states(recommended_regexes.len());
            let recommended_button_states = self.add_recommended_button_mouse_states.borrow();

            for (i, regex) in recommended_regexes.iter().enumerate() {
                let text_content = self.render_regex_content(regex, appearance);

                let item = self.render_regex_item(
                    Flex::row()
                        .with_cross_axis_alignment(CrossAxisAlignment::Center)
                        .with_child(Expanded::new(1., text_content).finish())
                        .with_child(
                            icon_button(
                                appearance,
                                Icon::Plus,
                                false,
                                recommended_button_states[i].clone(),
                            )
                            .build()
                            .on_click(move |ctx, _, _| {
                                ctx.dispatch_typed_action(PrivacyPageAction::AddRecommendedRegex(
                                    i,
                                ));
                            })
                            .finish(),
                        )
                        .finish(),
                    appearance,
                );

                column.add_child(item);
            }
        }

        column.finish()
    }

    fn render_info(&self, text: String, appearance: &Appearance) -> Box<dyn Element> {
        let info_icon = Container::new(
            ConstrainedBox::new(
                Icon::Info
                    .to_warpui_icon(
                        appearance
                            .theme()
                            .hint_text_color(appearance.theme().background()),
                    )
                    .finish(),
            )
            .with_width(appearance.ui_font_size() * 1.2)
            .with_height(appearance.ui_font_size() * 1.2)
            .finish(),
        )
        .with_padding_right(4.)
        .finish();

        Flex::row()
            .with_child(info_icon)
            .with_child(
                appearance
                    .ui_builder()
                    .span(text)
                    .with_style(UiComponentStyles {
                        font_color: Some(
                            appearance
                                .theme()
                                .hint_text_color(appearance.theme().background())
                                .into_solid(),
                        ),
                        font_size: Some(appearance.ui_font_body()),
                        ..Default::default()
                    })
                    .build()
                    .finish(),
            )
            .finish()
    }

    fn add_button(text: impl Into<Cow<'static, str>>, appearance: &Appearance) -> TextAndIcon {
        TextAndIcon::new(
            TextAndIconAlignment::IconFirst,
            text,
            Icon::Plus.to_warpui_icon(appearance.theme().active_ui_text_color()),
            MainAxisSize::Min,
            MainAxisAlignment::SpaceBetween,
            vec2f(16., 16.),
        )
        .with_inner_padding(3.)
    }

    fn add_button_style() -> UiComponentStyles {
        UiComponentStyles {
            padding: Some(Coords {
                // There's some offset issue with the button component
                left: 8.,
                right: 12.,
                top: 6.,
                bottom: 6.,
            }),
            margin: Some(Coords {
                left: 8.,
                right: 0.,
                top: 0.,
                bottom: 0.,
            }),
            ..Default::default()
        }
    }
}

impl SettingsWidget for SecretRedactionWidget {
    type View = PrivacyPageView;

    fn search_terms(&self) -> &str {
        "secret redaction safe mode hide"
    }

    fn render(
        &self,
        view: &Self::View,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let safe_mode_settings = SafeModeSettings::as_ref(app);
        let privacy_settings = PrivacySettings::as_ref(app);
        let description_text_color = description_text_color(appearance.theme()).into_solid();
        let ui_builder = appearance.ui_builder();
        let is_enterprise_enabled = privacy_settings.is_enterprise_secret_redaction_enabled();

        let local_only_icon_state = LocalOnlyIconState::for_setting(
            SafeModeEnabled::storage_key(),
            SafeModeEnabled::sync_to_cloud(),
            &mut view.local_only_icon_tooltip_states.borrow_mut(),
            app,
        );

        let secret_redaction_title_row = Container::new(
            Flex::row()
                .with_child(
                    Shrinkable::new(
                        1.0,
                        render_sub_header(
                            appearance,
                            crate::t!("settings-privacy-secret-redaction-title"),
                            Some(local_only_icon_state),
                        ),
                    )
                    .finish(),
                )
                .with_child(
                    Container::new({
                        if is_enterprise_enabled {
                            self.render_info(
                                crate::t!("settings-privacy-secret-redaction-enabled-by-org"),
                                appearance,
                            )
                        } else {
                            ui_builder
                                .switch(self.switch_state.clone())
                                .check(*safe_mode_settings.safe_mode_enabled.value())
                                .build()
                                .on_click(move |ctx, _, _| {
                                    ctx.dispatch_typed_action(PrivacyPageAction::ToggleSafeMode)
                                })
                                .finish()
                        }
                    })
                    .with_padding_right(TOGGLE_BUTTON_RIGHT_PADDING)
                    .finish(),
                )
                .with_cross_axis_alignment(CrossAxisAlignment::Start)
                .finish(),
        )
        .with_padding_bottom(HEADER_PADDING)
        .finish();

        let mut column = Flex::column()
            .with_child(secret_redaction_title_row)
            .with_child(
                ui_builder
                    .paragraph(crate::t!("settings-privacy-secret-redaction-description"))
                    .with_style(UiComponentStyles {
                        font_color: Some(description_text_color),
                        // One size up from the body font, as in Warp.
                        font_size: Some(appearance.ui_font_body() + 1.),
                        margin: Some(
                            Coords::default()
                                .top(-24.)
                                .bottom(styles::DESCRIPTION_LINE_MARGIN_BOTTOM),
                        ),
                        ..Default::default()
                    })
                    .build()
                    .finish(),
            );

        if *safe_mode_settings.safe_mode_enabled {
            // Add the secret display mode dropdown
            let local_only_icon_state = LocalOnlyIconState::for_setting(
                SecretDisplayModeSetting::storage_key(),
                SecretDisplayModeSetting::sync_to_cloud(),
                &mut view.local_only_icon_tooltip_states.borrow_mut(),
                app,
            );

            // Create the label with local-only icon if needed
            let label_with_icon = super::settings_page::render_dropdown_item_label(
                crate::t!("settings-privacy-secret-display-mode-label"),
                None,
                local_only_icon_state,
                None,
                appearance,
            );

            // Create left column with label and description
            let left_content = Flex::column()
                .with_child(label_with_icon)
                .with_child(
                    Container::new(
                        ui_builder
                            .paragraph(crate::t!(
                                "settings-privacy-secret-display-mode-description"
                            ))
                            .with_style(UiComponentStyles {
                                font_color: Some(description_text_color),
                                margin: Some(Coords::default().top(4.).bottom(0.)),
                                ..Default::default()
                            })
                            .build()
                            .finish(),
                    )
                    .finish(),
                )
                .finish();

            // Create the horizontal row with left content and dropdown
            let dropdown_row = Flex::row()
                .with_cross_axis_alignment(CrossAxisAlignment::Start)
                .with_child(
                    Shrinkable::new(
                        1.0,
                        Container::new(left_content)
                            // Space between left content and dropdown
                            .with_padding_right(16.)
                            .finish(),
                    )
                    .finish(),
                )
                .with_child(ChildView::new(&view.secret_redaction_display_dropdown).finish())
                .finish();

            column.add_child(
                Container::new(dropdown_row)
                    .with_margin_top(8.)
                    .with_margin_bottom(styles::DESCRIPTION_MARGIN_BOTTOM)
                    .finish(),
            );

            // User regexes section
            column.add_child(
                Flex::row()
                    .with_cross_axis_alignment(CrossAxisAlignment::Start)
                    .with_child(
                        Expanded::new(
                            1.,
                            Flex::column()
                                .with_child(self.render_section_title(
                                    crate::t!("settings-privacy-custom-regex-title"),
                                    appearance,
                                ))
                                .with_child(self.render_description(
                                    crate::t!("settings-privacy-custom-regex-description"),
                                    appearance,
                                    if privacy_settings.user_secret_regex_list.iter().count() > 0 {
                                        10.
                                    } else {
                                        0.
                                    },
                                ))
                                .finish(),
                        )
                        .finish(),
                    )
                    .with_child(
                        ui_builder
                            .button(
                                ButtonVariant::Secondary,
                                self.add_regex_button_mouse_state.clone(),
                            )
                            .with_text_and_icon_label(Self::add_button(
                                crate::t!("settings-privacy-add-regex"),
                                appearance,
                            ))
                            .with_style(Self::add_button_style())
                            .build()
                            .on_click(move |ctx, _, _| {
                                ctx.dispatch_typed_action(PrivacyPageAction::ShowAddRegexModal);
                            })
                            .finish(),
                    )
                    .finish(),
            );

            let workspaces = UserWorkspaces::as_ref(app);
            let enterprise_regex_list = workspaces.get_enterprise_secret_redaction_regex_list();

            if is_enterprise_enabled && !enterprise_regex_list.is_empty() {
                column.add_child(self.render_tab_bar(
                    appearance,
                    privacy_settings,
                    view.active_secret_redaction_tab,
                    view,
                    app,
                ));
            }

            let tab_content = if is_enterprise_enabled && !enterprise_regex_list.is_empty() {
                match view.active_secret_redaction_tab {
                    SecretRedactionTab::Personal => {
                        self.render_personal_content(view, appearance, app)
                    }
                    SecretRedactionTab::Enterprise => {
                        self.render_enterprise_content(appearance, app)
                    }
                }
            } else {
                self.render_personal_content(view, appearance, app)
            };

            column.add_child(tab_content);
            column.add_child(self.horizontal_divider(appearance));
        }

        Container::new(column.finish()).finish()
    }
}

#[derive(Default)]
struct AppAnalyticsWidget {
    switch_state: SwitchStateHandle,
    zdr_badge_mouse_state: MouseStateHandle,
}

impl AppAnalyticsWidget {
    fn render_zero_data_retention_badge(&self, appearance: &Appearance) -> Box<dyn Element> {
        let ui_builder = appearance.ui_builder();

        Hoverable::new(self.zdr_badge_mouse_state.clone(), move |mouse_state| {
            let is_hovered = mouse_state.is_hovered();
            let theme = appearance.theme();

            let background_color = appearance.theme().accent();

            let badge = Container::new(
                Text::new_inline(
                    "ZDR",
                    appearance.ui_font_family(),
                    appearance.ui_font_overline(),
                )
                .with_color(theme.active_ui_text_color().into())
                .finish(),
            )
            .with_background(background_color)
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(3.)))
            .with_uniform_padding(3.)
            .with_margin_left(8.)
            .finish();

            let mut stack = Stack::new().with_child(badge);
            if is_hovered {
                let tooltip = ui_builder.tool_tip(crate::t!("settings-privacy-zdr-tooltip"));
                stack.add_positioned_child(
                    tooltip.build().finish(),
                    OffsetPositioning::offset_from_parent(
                        vec2f(0., -3.),
                        ParentOffsetBounds::Unbounded,
                        ParentAnchor::TopLeft,
                        ChildAnchor::BottomLeft,
                    ),
                );
            }
            stack.finish()
        })
        .with_cursor(Cursor::PointingHand)
        .finish()
    }

    fn should_show_zdr_badge(&self, app: &AppContext) -> bool {
        let setting = UserWorkspaces::as_ref(app).get_ugc_collection_enablement_setting();
        matches!(setting, UgcCollectionEnablementSetting::Disable)
    }
}

impl SettingsWidget for AppAnalyticsWidget {
    type View = PrivacyPageView;

    fn search_terms(&self) -> &str {
        "telemetry usage analytics data collection"
    }

    fn should_render(&self, app: &AppContext) -> bool {
        // Builds without a telemetry config cannot ship telemetry, so the toggle would be a
        // no-op. Hide it in that case. NOTE: this fork hard-codes
        // `is_telemetry_available()` to `false` and `send_telemetry_from_ctx!` is a no-op, so
        // this widget currently never renders. The widget is ported anyway so the control comes
        // back automatically if a telemetry channel is ever wired up -- unlike crash reporting,
        // `IsTelemetryEnabled` genuinely has no live consumer here.
        if !ChannelState::is_telemetry_available() {
            return false;
        }
        let privacy_settings = PrivacySettings::as_ref(app);
        !privacy_settings.is_telemetry_force_enabled()
    }

    fn render(
        &self,
        _view: &Self::View,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let privacy_settings = PrivacySettings::as_ref(app);
        let ui_builder = appearance.ui_builder();
        let description_text_color = description_text_color(appearance.theme()).into_solid();

        let is_enterprise = UserWorkspaces::as_ref(app)
            .current_workspace()
            .is_some_and(|w| w.billing_metadata.customer_type == CustomerType::Enterprise);
        // Keep the old description for enterprise users because we do not collect block
        // input/output for them.
        let description = if is_enterprise {
            crate::t!("settings-privacy-telemetry-description-enterprise")
        } else {
            crate::t!("settings-privacy-telemetry-description")
        };

        let org_setting = UserWorkspaces::handle(app)
            .as_ref(app)
            .get_ugc_collection_enablement_setting();

        let (is_toggleable, is_checked) = match org_setting {
            UgcCollectionEnablementSetting::Enable => (false, true),
            UgcCollectionEnablementSetting::Disable => (false, false),
            UgcCollectionEnablementSetting::RespectUserSetting => {
                (true, privacy_settings.is_telemetry_enabled)
            }
        };

        let zdr_label_component = if self.should_show_zdr_badge(app) {
            Flex::row()
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_child(render_body_item_label::<PrivacyPageAction>(
                    crate::t!("settings-privacy-telemetry-title"),
                    None,
                    None,
                    LocalOnlyIconState::Hidden,
                    is_toggleable.into(),
                    appearance,
                ))
                .with_child(self.render_zero_data_retention_badge(appearance))
                .finish()
        } else {
            render_body_item_label::<PrivacyPageAction>(
                crate::t!("settings-privacy-telemetry-title"),
                None,
                None,
                LocalOnlyIconState::Hidden,
                is_toggleable.into(),
                appearance,
            )
        };

        let switch = ui_builder
            .switch(self.switch_state.clone())
            .check(is_checked);
        let switch = if is_toggleable {
            switch
                .build()
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(PrivacyPageAction::ToggleTelemetry)
                })
                .finish()
        } else {
            switch
                .with_tooltip(TooltipConfig {
                    text: crate::t!("settings-privacy-managed-by-org"),
                    styles: ui_builder.default_tool_tip_styles(),
                })
                .disable()
                .build()
                .finish()
        };

        let mut column = Flex::column();
        column.add_child(super::settings_page::build_toggle_element(
            zdr_label_component,
            switch,
            appearance,
            None,
        ));
        // Warp follows the description with a "Read more about Warp's use of data" link to
        // docs.warp.dev. Dropped here: that document describes Warp Inc's data practices, not
        // this fork's, so linking it would be misleading.
        column.add_child(
            ui_builder
                .paragraph(description)
                .with_style(UiComponentStyles {
                    font_color: Some(description_text_color),
                    margin: Some(
                        Coords::default()
                            .top(styles::DESCRIPTION_NEGATIVE_MARGIN_OFFSET)
                            .bottom(styles::DESCRIPTION_MARGIN_BOTTOM),
                    ),
                    ..Default::default()
                })
                .build()
                .finish(),
        );

        column.finish()
    }
}

#[derive(Default)]
struct CrashReportsWidget {
    switch_state: SwitchStateHandle,
}

impl SettingsWidget for CrashReportsWidget {
    type View = PrivacyPageView;

    fn search_terms(&self) -> &str {
        "telemetry crash reports stability data collection"
    }

    fn should_render(&self, app: &AppContext) -> bool {
        // Warp gates this on `ChannelState::is_crash_reporting_available()`, meaning "this build
        // cannot ship crash reports, so the toggle is a no-op". In this fork nothing is uploaded
        // (that function is hard-coded `false`), but the setting is still live: it gates the
        // panic hook installed by `crash_reporting::init_local_crash_reporting`, which writes a
        // full backtrace to the local log. Gate on the feature flag that actually decides
        // whether `crash_reporting::init` runs, so the toggle appears exactly when it does
        // something.
        if !FeatureFlag::CrashReporting.is_enabled() {
            return false;
        }
        let privacy_settings = PrivacySettings::as_ref(app);
        !privacy_settings.is_telemetry_force_enabled()
    }

    fn render(
        &self,
        _view: &Self::View,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let ui_builder = appearance.ui_builder();
        let privacy_settings = PrivacySettings::as_ref(app);
        Flex::column()
            .with_child(render_body_item::<PrivacyPageAction>(
                crate::t!("settings-privacy-crash-reports-title"),
                None,
                // Crash report state is never synced anywhere, so no local-only icon.
                LocalOnlyIconState::Hidden,
                ToggleState::Enabled,
                appearance,
                ui_builder
                    .switch(self.switch_state.clone())
                    .check(privacy_settings.is_crash_reporting_enabled)
                    .build()
                    .on_click(move |ctx, _, _| {
                        ctx.dispatch_typed_action(PrivacyPageAction::ToggleCrashReporting)
                    })
                    .finish(),
                None,
            ))
            .with_child(
                ui_builder
                    .paragraph(crate::t!("settings-privacy-crash-reports-description"))
                    .with_style(UiComponentStyles {
                        font_color: Some(
                            appearance
                                .theme()
                                .sub_text_color(appearance.theme().surface_2())
                                .into_solid(),
                        ),
                        margin: Some(
                            Coords::default()
                                .top(styles::DESCRIPTION_NEGATIVE_MARGIN_OFFSET)
                                .bottom(styles::DESCRIPTION_MARGIN_BOTTOM),
                        ),
                        ..Default::default()
                    })
                    .build()
                    .finish(),
            )
            .finish()
    }
}

#[derive(Default)]
struct PrivacyPolicyWidget {
    link_mouse_state: MouseStateHandle,
}

impl SettingsWidget for PrivacyPolicyWidget {
    type View = PrivacyPageView;

    fn search_terms(&self) -> &str {
        "privacy policy terms"
    }

    fn should_render(&self, _app: &AppContext) -> bool {
        // `PRIVACY_POLICY_URL` is an empty placeholder in this fork (see
        // `app/src/util/links.rs`) until the project publishes its own policy. Rendering the
        // section anyway would show a link that opens nothing, so hide it until the URL is set.
        !PRIVACY_POLICY_URL.is_empty()
    }

    fn render(
        &self,
        _view: &Self::View,
        appearance: &Appearance,
        _app: &AppContext,
    ) -> Box<dyn Element> {
        Flex::column()
            .with_child(render_body_item::<PrivacyPageAction>(
                crate::t!("settings-privacy-policy-title"),
                None,
                // Not rendering a setting, so no need to show local only icon state.
                LocalOnlyIconState::Hidden,
                ToggleState::Enabled,
                appearance,
                Empty::new().finish(),
                None,
            ))
            .with_child(
                Align::new(
                    appearance
                        .ui_builder()
                        .link(
                            crate::t!("settings-privacy-policy-link"),
                            Some(PRIVACY_POLICY_URL.into()),
                            None,
                            self.link_mouse_state.clone(),
                        )
                        .soft_wrap(false)
                        .build()
                        .with_margin_bottom(styles::DESCRIPTION_MARGIN_BOTTOM)
                        .finish(),
                )
                .left()
                .finish(),
            )
            .finish()
    }
}

pub fn init_actions_from_parent_view<T: Action + Clone>(
    app: &mut AppContext,
    context: &ContextPredicate,
    builder: fn(SettingsAction) -> T,
) {
    // The "cloud AI conversation storage" pair Warp registers here is dropped along with
    // `IsCloudConversationStorageEnabled`.
    let toggle_binding_pairs = vec![
        ToggleSettingActionPair::new(
            "app analytics",
            builder(SettingsAction::PrivacyPageToggle(
                PrivacyPageAction::ToggleTelemetry,
            )),
            context,
            flags::TELEMETRY_FLAG,
        ),
        ToggleSettingActionPair::new(
            "crash reporting",
            builder(SettingsAction::PrivacyPageToggle(
                PrivacyPageAction::ToggleCrashReporting,
            )),
            context,
            flags::CRASH_REPORTING_FLAG,
        ),
        ToggleSettingActionPair::new(
            "secret redaction",
            builder(SettingsAction::PrivacyPageToggle(
                PrivacyPageAction::ToggleSafeMode,
            )),
            context,
            flags::SAFE_MODE_FLAG,
        ),
    ];

    ToggleSettingActionPair::add_toggle_setting_action_pairs_as_bindings(toggle_binding_pairs, app);
}

mod styles {
    // Apply a negative margin to the description text so it appears closer to the main
    // settings option text.
    pub const DESCRIPTION_NEGATIVE_MARGIN_OFFSET: f32 = -8.;

    /// The space between a description and the next toggle.
    pub const DESCRIPTION_MARGIN_BOTTOM: f32 = 12.;

    /// The space between two description lines which are describing the same toggle.
    pub const DESCRIPTION_LINE_MARGIN_BOTTOM: f32 = 6.;
}

fn description_text_color(theme: &WarpTheme) -> warp_core::ui::theme::Fill {
    theme.sub_text_color(theme.surface_2())
}
