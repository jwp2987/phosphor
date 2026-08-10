//! The Code settings page: a flat list of local toggles for the editor and code review,
//! plus the language-server (LSP) management section.
//!
//! Zap once retired the LSP subpage wholesale (`efcaa42b8`). LSP is back, so the subpage's
//! content is back with it -- but not its shape: `Code` is still a single-layer Page with no
//! subpages hanging off it in the sidebar, so what the pin renders as a "Codebase Indexing"
//! subpage of per-workspace rows is rendered here as one more section in the flat list. The
//! controls themselves (per-server enable/disable, install status, restart, remove) and the
//! actions they dispatch are the pin's.

#[cfg(feature = "local_fs")]
use super::features::external_editor::ExternalEditorView;
use super::{
    settings_page::{
        render_body_item, render_sub_header_with_description, MatchData, PageType,
        SettingsPageMeta, SettingsPageViewHandle, SettingsWidget,
    },
    LocalOnlyIconState, SettingsAction, SettingsSection, ToggleState,
};
use crate::{
    ai::persisted_workspace::{
        EnablementState, LspRepoStatus, PersistedWorkspace, PersistedWorkspaceEvent,
    },
    appearance::Appearance,
    code::lsp_telemetry::{LspControlActionType, LspEnablementSource, LspTelemetryEvent},
    send_telemetry_from_ctx,
    settings::CodeSettings,
    terminal::general_settings::GeneralSettings,
    ui_components::{
        avatar::{Avatar, AvatarContent, StatusElementTypes},
        buttons::icon_button,
        icons::Icon,
    },
    workspace::tab_settings::TabSettings,
    TelemetryEvent,
};
use ai::project_context::model::{ProjectContextModel, ProjectContextModelEvent};
use ai::workspace::WorkspaceMetadata;

use lsp::supported_servers::LSPServerType;
use lsp::{LspManagerModel, LspManagerModelEvent, LspServerModel, LspState};
use pathfinder_color::ColorU;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use warp_core::{
    features::FeatureFlag,
    report_if_error,
    settings::ToggleableSetting as _,
    ui::theme::{AnsiColorIdentifier, WarpTheme},
};
use warp_util::path::user_friendly_path;
use warpui::{
    elements::{
        ChildView, Container, CornerRadius, CrossAxisAlignment, Element, Empty, Expanded, Fill,
        Flex, MainAxisAlignment, MainAxisSize, MouseStateHandle, ParentElement, Radius, Shrinkable,
    },
    fonts::Weight,
    keymap::ContextPredicate,
    platform::Cursor,
    ui_components::{
        button::ButtonVariant,
        components::{UiComponent, UiComponentStyles},
        switch::SwitchStateHandle,
    },
    Action, AppContext, Entity, ModelHandle, SingletonEntity, TypedActionView, View, ViewContext,
    ViewHandle,
};

/// Vertical rhythm between the language-server section's stacked pieces. Both
/// constants are the pin's.
const MAIN_SECTION_MARGIN: f32 = 12.;
const SUB_SECTION_MARGIN: f32 = 8.;
const LSP_STATUS_INDICATOR_SIZE: f32 = 8.;

/// Mouse/switch state for one language-server row. One entry per rendered row,
/// flattened across workspaces in render order (see [`CodeSettingsPageView::lsp_row_mouse_states`]).
#[derive(Clone, Default)]
struct LspServerRowMouseStates {
    restart: MouseStateHandle,
    toggle: SwitchStateHandle,
    install: MouseStateHandle,
}

pub struct CodeSettingsPageView {
    page: PageType<Self>,
    /// Mouse states for LSP server row buttons.
    ///
    /// Each workspace can have 0..n language servers, so the count does not match
    /// workspaces 1:1. The states are flattened into a single Vec, indexed by
    /// walking workspaces and their servers in render order -- which is why both
    /// orders are sorted deterministically in `render_language_servers` rather
    /// than taken straight off a `HashMap` iterator.
    lsp_row_mouse_states: Vec<LspServerRowMouseStates>,
    /// Tracks installation status for suggested LSP servers so the UI can decide
    /// whether to show "Available for download" vs "Installed" and whether the
    /// "+" button should trigger install or just enable.
    suggested_server_statuses: HashMap<(PathBuf, LSPServerType), LspRepoStatus>,
    #[cfg(feature = "local_fs")]
    external_editor_view: Option<ViewHandle<ExternalEditorView>>,
}

impl CodeSettingsPageView {
    pub fn new(ctx: &mut ViewContext<CodeSettingsPageView>) -> Self {
        // Subscribes to ProjectContextModel: re-render when project rules change, keeping any
        // subcomponent that depends on the rule set up to date.
        ctx.subscribe_to_model(&ProjectContextModel::handle(ctx), |_me, _, event, ctx| {
            if matches!(event, ProjectContextModelEvent::KnownRulesChanged(_)) {
                ctx.notify();
            }
        });

        // Both singleton checks are load-bearing, not defensive padding.
        // `LspManagerModel` is registered by `lsp::init` on the client app's
        // `workspace::init` path only, and `PersistedWorkspace` by `app/src/lib.rs`;
        // `handle()` panics on an unregistered singleton. Page-level tests (and any
        // future harness) build this view without either. Same hazard class as the
        // guard already documented on `GlobalBufferModel::new`.
        let lsp_server_count = if ctx.has_singleton_model::<PersistedWorkspace>() {
            PersistedWorkspace::as_ref(ctx).total_lsp_server_count(true)
        } else {
            0
        };

        if ctx.has_singleton_model::<LspManagerModel>() {
            // Real-time status updates: a server starting, stopping or being removed
            // changes both the status dot and the number of rows.
            ctx.subscribe_to_model(
                &LspManagerModel::handle(ctx),
                |me, _, event, ctx| match event {
                    LspManagerModelEvent::ServerStarted(_)
                    | LspManagerModelEvent::ServerStopped(_)
                    | LspManagerModelEvent::ServerRemoved { .. } => {
                        me.resize_lsp_row_mouse_states(ctx);
                        ctx.notify();
                    }
                },
            );
        }

        if ctx.has_singleton_model::<PersistedWorkspace>() {
            // Suggested-server detection and installation status. Detection itself is
            // not kicked off here -- `PersistedWorkspace::new()` already starts it at
            // startup and emits `AvailableServersDetected` per workspace.
            ctx.subscribe_to_model(
                &PersistedWorkspace::handle(ctx),
                move |me, _model, event, ctx| match event {
                    PersistedWorkspaceEvent::AvailableServersDetected {
                        workspace_path,
                        servers,
                    } => {
                        for &server_type in servers {
                            #[cfg(feature = "local_fs")]
                            let status = PersistedWorkspace::handle(ctx).update(ctx, |model, ctx| {
                                model.detect_lsp_workspace_status(
                                    workspace_path.clone(),
                                    server_type,
                                    ctx,
                                )
                            });
                            #[cfg(not(feature = "local_fs"))]
                            let status = LspRepoStatus::CheckingForInstallation;
                            me.suggested_server_statuses
                                .insert((workspace_path.clone(), server_type), status);
                        }
                        me.resize_lsp_row_mouse_states(ctx);
                        ctx.notify();
                    }
                    PersistedWorkspaceEvent::InstallStatusUpdate {
                        server_type,
                        status,
                    } => {
                        let new_status = LspRepoStatus::from_installation_status(status, *server_type);
                        for ((_, st), repo_status) in &mut me.suggested_server_statuses {
                            if *st == *server_type {
                                *repo_status = new_status.clone();
                            }
                        }
                        ctx.notify();
                    }
                    PersistedWorkspaceEvent::InstallationSucceeded
                    | PersistedWorkspaceEvent::InstallationFailed
                    | PersistedWorkspaceEvent::WorkspaceAdded { .. } => {
                        ctx.notify();
                    }
                },
            );
        }

        let (page, external_editor_view) = Self::build_page(ctx);

        Self {
            page,
            lsp_row_mouse_states: (0..lsp_server_count).map(|_| Default::default()).collect(),
            suggested_server_statuses: HashMap::new(),
            #[cfg(feature = "local_fs")]
            external_editor_view,
        }
    }

    /// Keeps one mouse-state entry per rendered language-server row.
    ///
    /// Preserves existing entries (`resize_with`) so a re-render triggered by an
    /// unrelated server does not drop the hover/press state of a row the user is
    /// currently interacting with.
    fn resize_lsp_row_mouse_states(&mut self, ctx: &AppContext) {
        if !ctx.has_singleton_model::<PersistedWorkspace>() {
            return;
        }
        let new_count = PersistedWorkspace::as_ref(ctx).total_lsp_server_count(true);
        if self.lsp_row_mouse_states.len() != new_count {
            self.lsp_row_mouse_states
                .resize_with(new_count, Default::default);
        }
    }

    /// Constructs the page widgets. Code is now a single page (no subpages, no category
    /// headers) — the editor and code-review toggles are laid out flat.
    #[cfg(feature = "local_fs")]
    fn build_page(
        ctx: &mut ViewContext<Self>,
    ) -> (PageType<Self>, Option<ViewHandle<ExternalEditorView>>) {
        let (widgets, external_editor_view) = if FeatureFlag::ZapNewSettingsModes.is_enabled()
        {
            let editor_view = ctx.add_typed_action_view(ExternalEditorView::new);
            let widgets: Vec<Box<dyn SettingsWidget<View = Self>>> = vec![
                Box::new(ExternalEditorCodeWidget),
                Box::new(AutoOpenCodeReviewPaneCodeWidget::default()),
                Box::new(CodeReviewPanelToggleWidget::default()),
                Box::new(CodeReviewDiffStatsToggleWidget::default()),
                Box::new(ProjectExplorerToggleWidget::default()),
                Box::new(GlobalSearchToggleWidget::default()),
                Box::new(ShowHiddenFilesToggleWidget::default()),
                Box::new(FormatOnSaveToggleWidget::default()),
                Box::new(LanguageServersWidget),
            ];
            (widgets, Some(editor_view))
        } else {
            // Legacy view: in the old settings mode, the Code page renders nothing (the
            // original CodePageWidget only rendered an LSP-era header with no real content, so
            // this just returns an empty page).
            (vec![], None)
        };
        (
            PageType::new_uncategorized(widgets, None),
            external_editor_view,
        )
    }

    /// There is no ExternalEditorView in wasm builds; only the 4 non-external-editor toggles
    /// are rendered.
    #[cfg(not(feature = "local_fs"))]
    fn build_page(
        _ctx: &mut ViewContext<Self>,
    ) -> (PageType<Self>, Option<ViewHandle<ExternalEditorView>>) {
        let widgets: Vec<Box<dyn SettingsWidget<View = Self>>> =
            if FeatureFlag::ZapNewSettingsModes.is_enabled() {
                vec![
                    Box::new(AutoOpenCodeReviewPaneCodeWidget::default()),
                    Box::new(CodeReviewPanelToggleWidget::default()),
                    Box::new(CodeReviewDiffStatsToggleWidget::default()),
                    Box::new(ProjectExplorerToggleWidget::default()),
                    Box::new(GlobalSearchToggleWidget::default()),
                    Box::new(ShowHiddenFilesToggleWidget::default()),
                    Box::new(FormatOnSaveToggleWidget::default()),
                    Box::new(LanguageServersWidget),
                ]
            } else {
                vec![]
            };
        (PageType::new_uncategorized(widgets, None), None)
    }
}

impl Entity for CodeSettingsPageView {
    type Event = CodeSettingsPageEvent;
}

impl View for CodeSettingsPageView {
    fn ui_name() -> &'static str {
        "CodePage"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        self.page.render(self, app)
    }
}

#[derive(Debug, Clone)]
pub enum CodeSettingsPageEvent {
    OpenProjectRules { rule_paths: Vec<PathBuf> },
}

#[derive(Debug, Clone)]
pub enum CodeSettingsPageAction {
    OpenProjectRules {
        rule_paths: Vec<PathBuf>,
    },
    ToggleCodeReviewPanel,
    ToggleShowCodeReviewDiffStats,
    ToggleAutoOpenCodeReviewPane,
    ToggleProjectExplorer,
    ToggleGlobalSearch,
    ToggleShowHiddenFiles,
    ToggleFormatOnSave,
    /// Toggle an LSP server on/off for a workspace.
    ToggleLspServer {
        workspace_path: PathBuf,
        server_type: LSPServerType,
        currently_enabled: bool,
    },
    RestartLspServer {
        server: ModelHandle<LspServerModel>,
    },
    /// Install (if needed) and enable a suggested LSP server.
    InstallAndEnableLspServer {
        workspace_path: PathBuf,
        server_type: LSPServerType,
    },
    /// Enable a suggested LSP server that is already installed.
    EnableSuggestedLspServer {
        workspace_path: PathBuf,
        server_type: LSPServerType,
    },
}

impl TypedActionView for CodeSettingsPageView {
    type Action = CodeSettingsPageAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            CodeSettingsPageAction::OpenProjectRules { rule_paths } => {
                ctx.emit(CodeSettingsPageEvent::OpenProjectRules {
                    rule_paths: rule_paths.clone(),
                });
            }
            CodeSettingsPageAction::ToggleCodeReviewPanel => {
                TabSettings::handle(ctx).update(ctx, |settings, ctx| {
                    report_if_error!(settings.show_code_review_button.toggle_and_save_value(ctx));
                });
                ctx.notify();
            }
            CodeSettingsPageAction::ToggleShowCodeReviewDiffStats => {
                TabSettings::handle(ctx).update(ctx, |settings, ctx| {
                    report_if_error!(settings
                        .show_code_review_diff_stats
                        .toggle_and_save_value(ctx));
                });
                ctx.notify();
            }
            CodeSettingsPageAction::ToggleProjectExplorer => {
                CodeSettings::handle(ctx).update(ctx, |settings, ctx| {
                    report_if_error!(settings.show_project_explorer.toggle_and_save_value(ctx));
                });
                ctx.notify();
            }
            CodeSettingsPageAction::ToggleGlobalSearch => {
                CodeSettings::handle(ctx).update(ctx, |settings, ctx| {
                    report_if_error!(settings.show_global_search.toggle_and_save_value(ctx));
                });
                ctx.notify();
            }
            CodeSettingsPageAction::ToggleShowHiddenFiles => {
                CodeSettings::handle(ctx).update(ctx, |settings, ctx| {
                    report_if_error!(settings.show_hidden_files.toggle_and_save_value(ctx));
                });
                ctx.notify();
            }
            CodeSettingsPageAction::ToggleFormatOnSave => {
                CodeSettings::handle(ctx).update(ctx, |settings, ctx| {
                    report_if_error!(settings.format_on_save.toggle_and_save_value(ctx));
                });
                ctx.notify();
            }
            CodeSettingsPageAction::ToggleLspServer {
                workspace_path,
                server_type,
                currently_enabled,
            } => {
                if *currently_enabled {
                    // Toggling OFF: stop the running server and persist the disable.
                    // `remove_server` stops with `manually_stopped = true`, which is
                    // what keeps the shutdown manager from auto-restarting it.
                    send_telemetry_from_ctx!(
                        LspTelemetryEvent::ServerRemoved {
                            server_type: server_type.binary_name().to_string(),
                            source: LspEnablementSource::Settings,
                        },
                        ctx
                    );
                    if ctx.has_singleton_model::<LspManagerModel>() {
                        LspManagerModel::handle(ctx).update(ctx, |manager, ctx| {
                            manager.remove_server(workspace_path, *server_type, ctx);
                        });
                    }
                    PersistedWorkspace::handle(ctx).update(ctx, |workspace, _| {
                        workspace.disable_lsp_server_for_path(workspace_path, *server_type);
                    });
                } else {
                    // Toggling ON: persist the enable, then spawn.
                    send_telemetry_from_ctx!(
                        LspTelemetryEvent::ServerEnabled {
                            server_type: server_type.binary_name().to_string(),
                            source: LspEnablementSource::Settings,
                            needed_install: false,
                        },
                        ctx
                    );
                    let workspace_path = workspace_path.clone();
                    let server_type = *server_type;
                    PersistedWorkspace::handle(ctx).update(ctx, |workspace, _ctx| {
                        workspace.enable_lsp_server_for_path(&workspace_path, server_type);
                        #[cfg(feature = "local_fs")]
                        workspace.execute_lsp_task(
                            crate::ai::persisted_workspace::LspTask::Spawn {
                                file_path: workspace_path,
                            },
                            _ctx,
                        );
                    });
                }
                ctx.notify();
            }
            CodeSettingsPageAction::RestartLspServer { server } => {
                let server_name = server.as_ref(ctx).server_name();
                send_telemetry_from_ctx!(
                    LspTelemetryEvent::ControlAction {
                        action: LspControlActionType::Restart,
                        server_type: Some(server_name),
                    },
                    ctx
                );
                server.update(ctx, |server, ctx| {
                    server.restart(ctx);
                });
            }
            CodeSettingsPageAction::InstallAndEnableLspServer {
                workspace_path,
                server_type,
            } => {
                send_telemetry_from_ctx!(
                    LspTelemetryEvent::ServerEnabled {
                        server_type: server_type.binary_name().to_string(),
                        source: LspEnablementSource::Settings,
                        needed_install: true,
                    },
                    ctx
                );
                #[cfg(feature = "local_fs")]
                {
                    let workspace_path = workspace_path.clone();
                    let server_type = *server_type;
                    PersistedWorkspace::handle(ctx).update(ctx, |workspace, _ctx| {
                        workspace.execute_lsp_task(
                            crate::ai::persisted_workspace::LspTask::Install {
                                file_path: workspace_path.clone(),
                                repo_root: workspace_path,
                                server_type,
                            },
                            _ctx,
                        );
                    });
                }
                #[cfg(not(feature = "local_fs"))]
                let _ = workspace_path;
                ctx.notify();
            }
            CodeSettingsPageAction::EnableSuggestedLspServer {
                workspace_path,
                server_type,
            } => {
                send_telemetry_from_ctx!(
                    LspTelemetryEvent::ServerEnabled {
                        server_type: server_type.binary_name().to_string(),
                        source: LspEnablementSource::Settings,
                        needed_install: false,
                    },
                    ctx
                );
                let workspace_path = workspace_path.clone();
                let server_type = *server_type;
                PersistedWorkspace::handle(ctx).update(ctx, |workspace, _ctx| {
                    workspace.enable_lsp_server_for_path(&workspace_path, server_type);
                    #[cfg(feature = "local_fs")]
                    workspace.execute_lsp_task(
                        crate::ai::persisted_workspace::LspTask::Spawn {
                            file_path: workspace_path,
                        },
                        _ctx,
                    );
                });
                ctx.notify();
            }
            CodeSettingsPageAction::ToggleAutoOpenCodeReviewPane => {
                GeneralSettings::handle(ctx).update(ctx, |settings, ctx| {
                    report_if_error!(settings
                        .auto_open_code_review_pane_on_first_agent_change
                        .toggle_and_save_value(ctx));
                });
                send_telemetry_from_ctx!(
                    TelemetryEvent::FeaturesPageAction {
                        action: "ToggleAutoOpenCodeReviewPane".to_string(),
                        value: format!(
                            "{}",
                            *GeneralSettings::as_ref(ctx)
                                .auto_open_code_review_pane_on_first_agent_change
                        )
                    },
                    ctx
                );
                ctx.notify();
            }
        }
    }
}

pub fn init_actions_from_parent_view<T: Action + Clone>(
    _app: &mut AppContext,
    _context: &ContextPredicate,
    _builder: fn(SettingsAction) -> T,
) {
}

#[cfg(feature = "local_fs")]
struct ExternalEditorCodeWidget;

#[cfg(feature = "local_fs")]
impl SettingsWidget for ExternalEditorCodeWidget {
    type View = CodeSettingsPageView;

    fn search_terms(&self) -> &str {
        "code editor open files markdown AI conversations layout pane tab"
    }

    fn render(
        &self,
        view: &Self::View,
        _appearance: &Appearance,
        _app: &AppContext,
    ) -> Box<dyn Element> {
        if let Some(editor_view) = &view.external_editor_view {
            ChildView::new(editor_view).finish()
        } else {
            Empty::new().finish()
        }
    }
}

#[derive(Default)]
struct AutoOpenCodeReviewPaneCodeWidget {
    switch_state: SwitchStateHandle,
}

impl SettingsWidget for AutoOpenCodeReviewPaneCodeWidget {
    type View = CodeSettingsPageView;

    fn search_terms(&self) -> &str {
        "oz auto open code review pane panel agent mode change first time accepted diff view conversation"
    }

    fn render(
        &self,
        _view: &Self::View,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let general_settings = GeneralSettings::as_ref(app);
        render_body_item::<CodeSettingsPageAction>(
            crate::t!("settings-code-auto-open-review-panel"),
            None,
            LocalOnlyIconState::Hidden,
            ToggleState::Enabled,
            appearance,
            appearance
                .ui_builder()
                .switch(self.switch_state.clone())
                .check(*general_settings.auto_open_code_review_pane_on_first_agent_change)
                .build()
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(CodeSettingsPageAction::ToggleAutoOpenCodeReviewPane);
                })
                .finish(),
            Some(crate::t!("settings-code-auto-open-review-panel-desc")),
        )
    }
}

impl SettingsPageMeta for CodeSettingsPageView {
    fn section() -> SettingsSection {
        SettingsSection::Code
    }

    fn update_filter(&mut self, query: &str, ctx: &mut ViewContext<Self>) -> MatchData {
        self.page.update_filter(query, ctx)
    }

    fn should_render(&self, _ctx: &AppContext) -> bool {
        FeatureFlag::ZapNewSettingsModes.is_enabled()
    }

    fn on_page_selected(&mut self, _: bool, _ctx: &mut ViewContext<Self>) {}

    fn scroll_to_widget(&mut self, widget_id: &'static str) {
        self.page.scroll_to_widget(widget_id)
    }

    fn clear_highlighted_widget(&mut self) {
        self.page.clear_highlighted_widget();
    }
}

impl From<ViewHandle<CodeSettingsPageView>> for SettingsPageViewHandle {
    fn from(view_handle: ViewHandle<CodeSettingsPageView>) -> Self {
        SettingsPageViewHandle::Code(view_handle)
    }
}

#[derive(Default)]
struct CodeReviewPanelToggleWidget {
    switch_state: SwitchStateHandle,
}

impl SettingsWidget for CodeReviewPanelToggleWidget {
    type View = CodeSettingsPageView;

    fn search_terms(&self) -> &str {
        "code review panel right side diff git"
    }

    fn render(
        &self,
        _view: &Self::View,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let tab_settings = TabSettings::as_ref(app);

        render_body_item::<CodeSettingsPageAction>(
            crate::t!("settings-code-show-code-review-button"),
            None,
            LocalOnlyIconState::Hidden,
            ToggleState::Enabled,
            appearance,
            appearance
                .ui_builder()
                .switch(self.switch_state.clone())
                .check(*tab_settings.show_code_review_button)
                .build()
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(CodeSettingsPageAction::ToggleCodeReviewPanel);
                })
                .finish(),
            Some(crate::t!("settings-code-show-code-review-button-desc")),
        )
    }
}

#[derive(Default)]
struct CodeReviewDiffStatsToggleWidget {
    switch_state: SwitchStateHandle,
}

impl SettingsWidget for CodeReviewDiffStatsToggleWidget {
    type View = CodeSettingsPageView;

    fn search_terms(&self) -> &str {
        "code review diff stats lines added removed counts"
    }

    fn render(
        &self,
        _view: &Self::View,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let tab_settings = TabSettings::as_ref(app);

        render_body_item::<CodeSettingsPageAction>(
            crate::t!("settings-code-show-diff-stats"),
            None,
            LocalOnlyIconState::Hidden,
            ToggleState::Enabled,
            appearance,
            appearance
                .ui_builder()
                .switch(self.switch_state.clone())
                .check(*tab_settings.show_code_review_diff_stats)
                .build()
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(
                        CodeSettingsPageAction::ToggleShowCodeReviewDiffStats,
                    );
                })
                .finish(),
            Some(crate::t!("settings-code-show-diff-stats-desc")),
        )
    }
}

#[derive(Default)]
struct ProjectExplorerToggleWidget {
    switch_state: SwitchStateHandle,
}

impl SettingsWidget for ProjectExplorerToggleWidget {
    type View = CodeSettingsPageView;

    fn search_terms(&self) -> &str {
        "project explorer file tree left panel tools"
    }

    fn render(
        &self,
        _view: &Self::View,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let code_settings = CodeSettings::as_ref(app);

        render_body_item::<CodeSettingsPageAction>(
            crate::t!("settings-code-project-explorer"),
            None,
            LocalOnlyIconState::Hidden,
            ToggleState::Enabled,
            appearance,
            appearance
                .ui_builder()
                .switch(self.switch_state.clone())
                .check(*code_settings.show_project_explorer)
                .build()
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(CodeSettingsPageAction::ToggleProjectExplorer);
                })
                .finish(),
            Some(crate::t!("settings-code-project-explorer-desc")),
        )
    }
}

#[derive(Default)]
struct GlobalSearchToggleWidget {
    switch_state: SwitchStateHandle,
}

impl SettingsWidget for GlobalSearchToggleWidget {
    type View = CodeSettingsPageView;

    fn search_terms(&self) -> &str {
        "global search file search left panel tools"
    }

    fn render(
        &self,
        _view: &Self::View,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let code_settings = CodeSettings::as_ref(app);

        render_body_item::<CodeSettingsPageAction>(
            crate::t!("settings-code-global-search"),
            None,
            LocalOnlyIconState::Hidden,
            ToggleState::Enabled,
            appearance,
            appearance
                .ui_builder()
                .switch(self.switch_state.clone())
                .check(*code_settings.show_global_search)
                .build()
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(CodeSettingsPageAction::ToggleGlobalSearch);
                })
                .finish(),
            Some(crate::t!("settings-code-global-search-desc")),
        )
    }
}

// Ported from the pin (`02b53fcd8:app/src/settings_view/code_page.rs`); issue #498. Mirrors
// `ProjectExplorerToggleWidget` / `GlobalSearchToggleWidget` above -- see #340 for the
// underlying setting and dotfile-filtering logic this exposes a control for.
#[derive(Default)]
struct ShowHiddenFilesToggleWidget {
    switch_state: SwitchStateHandle,
}

impl SettingsWidget for ShowHiddenFilesToggleWidget {
    type View = CodeSettingsPageView;

    fn search_terms(&self) -> &str {
        "show hidden files dotfiles project explorer file tree"
    }

    fn render(
        &self,
        _view: &Self::View,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let code_settings = CodeSettings::as_ref(app);

        render_body_item::<CodeSettingsPageAction>(
            crate::t!("settings-code-show-hidden-files"),
            None,
            LocalOnlyIconState::Hidden,
            ToggleState::Enabled,
            appearance,
            appearance
                .ui_builder()
                .switch(self.switch_state.clone())
                .check(*code_settings.show_hidden_files)
                .build()
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(CodeSettingsPageAction::ToggleShowHiddenFiles);
                })
                .finish(),
            Some(crate::t!("settings-code-show-hidden-files-desc")),
        )
    }
}

// Restored with LSP: format-on-save is an LSP request, so the setting went out with
// `efcaa42b8` and came back with `app/src/settings/code.rs`. Its toggle did not come
// back with it -- until now the restored setting had no user-reachable control, which
// is the "restored but never wired" defect this track exists to close.
#[derive(Default)]
struct FormatOnSaveToggleWidget {
    switch_state: SwitchStateHandle,
}

impl SettingsWidget for FormatOnSaveToggleWidget {
    type View = CodeSettingsPageView;

    fn search_terms(&self) -> &str {
        "format on save lsp language server formatting autoformat"
    }

    fn render(
        &self,
        _view: &Self::View,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let code_settings = CodeSettings::as_ref(app);

        render_body_item::<CodeSettingsPageAction>(
            crate::t!("settings-code-format-on-save"),
            None,
            LocalOnlyIconState::Hidden,
            ToggleState::Enabled,
            appearance,
            appearance
                .ui_builder()
                .switch(self.switch_state.clone())
                .check(*code_settings.format_on_save)
                .build()
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(CodeSettingsPageAction::ToggleFormatOnSave);
                })
                .finish(),
            Some(crate::t!("settings-code-format-on-save-desc")),
        )
    }
}

// The language-server management section, restored from the pin
// (`02b53fcd8:app/src/settings_view/code_page.rs`) -- LSP step 6b.
//
// WHAT MOVED, AND WHY
// -------------------
// At the pin these rows live inside the "Initialized / indexed folders" list of the
// Codebase Indexing subpage: one card per workspace, holding an indexing subsection and
// an LSP subsection. This fork's Code page is a single flat list with no subpages, so
// the same cards are rendered by their own widget rather than nested under an indexing
// section that does not exist here. The rows, their controls, and the actions they
// dispatch are the pin's; only their host changed.
//
// Two deliberate differences from the pin, recorded rather than left silent:
//   * No "View logs" button. The pin routes it as `CodeSettingsPageEvent::OpenLspLogs`
//     -> `SettingsViewEvent` -> `WorkspaceView::open_lsp_logs`, which is three files
//     outside this one. Log access already exists on the code footer
//     (`app/src/code/footer.rs`), the pin's other entry point for it.
//   * Workspaces and rows are rendered in a sorted order. The pin walks a `HashMap`
//     directly, so its row order is whatever the hasher produced that frame -- which
//     also re-assigns the flattened mouse-state indices from frame to frame.
/// The language-server management section: one card per workspace with known servers,
/// each card holding one row per server.
struct LanguageServersWidget;

impl SettingsWidget for LanguageServersWidget {
    type View = CodeSettingsPageView;

    fn search_terms(&self) -> &str {
        "lsp language server servers rust-analyzer gopls pyright typescript install enable disable restart stop remove diagnostics hover goto definition"
    }

    fn render(
        &self,
        view: &Self::View,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        view.render_language_servers(appearance, app)
    }
}

impl CodeSettingsPageView {
    /// Renders the whole language-server section.
    ///
    /// Returns an empty element when `PersistedWorkspace` is not registered: `as_ref`
    /// on an unregistered singleton panics, and this page is constructible in
    /// harnesses that register neither it nor `LspManagerModel`.
    fn render_language_servers(
        &self,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        if !app.has_singleton_model::<PersistedWorkspace>() {
            return Empty::new().finish();
        }

        let mut content = Flex::column().with_spacing(SUB_SECTION_MARGIN);
        content.add_child(render_sub_header_with_description(
            appearance,
            crate::t!("settings-code-language-servers"),
            crate::t!("settings-code-language-servers-desc"),
        ));

        let persisted = PersistedWorkspace::as_ref(app);
        let lsp_manager = if app.has_singleton_model::<LspManagerModel>() {
            Some(LspManagerModel::as_ref(app))
        } else {
            None
        };

        let workspaces: Vec<WorkspaceMetadata> = persisted.workspaces().collect();

        // One flattened mouse-state entry per rendered row, consumed in render order.
        let mut mouse_index = 0usize;
        let mut rendered_any = false;

        for workspace in &workspaces {
            let mut all_servers: Vec<(LSPServerType, EnablementState)> = persisted
                .all_lsp_servers(&workspace.path, true)
                .map(|servers| servers.collect())
                .unwrap_or_default();
            if all_servers.is_empty() {
                continue;
            }
            // `all_lsp_servers` iterates a `HashMap`; sort so row order -- and with it
            // the mouse-state index each row is handed -- is stable frame to frame.
            all_servers.sort_by_key(|(server_type, _)| server_type.binary_name());

            let row_mouse_states: Vec<LspServerRowMouseStates> = all_servers
                .iter()
                .map(|_| {
                    let state = self
                        .lsp_row_mouse_states
                        .get(mouse_index)
                        .cloned()
                        .unwrap_or_default();
                    mouse_index += 1;
                    state
                })
                .collect();

            rendered_any = true;
            content.add_child(self.render_workspace_card(
                &workspace.path,
                &all_servers,
                lsp_manager,
                row_mouse_states,
                appearance,
                app,
            ));
        }

        if !rendered_any {
            content.add_child(
                Container::new(
                    appearance
                        .ui_builder()
                        .paragraph(crate::t!("settings-code-language-servers-empty"))
                        .build()
                        .finish(),
                )
                .with_margin_bottom(MAIN_SECTION_MARGIN)
                .finish(),
            );
        }

        content.finish()
    }

    /// One workspace's card: the repo path, then a row per language server.
    fn render_workspace_card(
        &self,
        workspace_path: &Path,
        all_servers: &[(LSPServerType, EnablementState)],
        lsp_manager: Option<&LspManagerModel>,
        row_mouse_states: Vec<LspServerRowMouseStates>,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let mut card = Flex::column().with_spacing(SUB_SECTION_MARGIN);

        let home_dir = dirs::home_dir().and_then(|home| home.to_str().map(|s| s.to_owned()));
        let label = user_friendly_path(
            workspace_path.to_string_lossy().as_ref(),
            home_dir.as_deref(),
        )
        .to_string();

        let path_label = Shrinkable::new(
            1.,
            appearance
                .ui_builder()
                .span(label)
                .with_style(UiComponentStyles {
                    font_family_id: Some(appearance.monospace_font_family()),
                    font_size: Some(appearance.ui_font_size()),
                    font_weight: Some(Weight::Bold),
                    font_color: Some(theme.active_ui_text_color().into()),
                    ..Default::default()
                })
                .build()
                .finish(),
        )
        .finish();

        card.add_child(
            Flex::row()
                .with_main_axis_size(MainAxisSize::Max)
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_child(Expanded::new(1., path_label).finish())
                .finish(),
        );

        // A server only has a model once it has been enabled and spawned; a disabled
        // or suggested one has none, which is what the row renders as "Not running".
        let server_models =
            lsp_manager.and_then(|manager| manager.servers_for_workspace(workspace_path));

        for ((server_type, enablement_state), mouse_states) in
            all_servers.iter().zip(row_mouse_states)
        {
            if *enablement_state == EnablementState::Suggested {
                let repo_status = self
                    .suggested_server_statuses
                    .get(&(workspace_path.to_path_buf(), *server_type))
                    .cloned();
                card.add_child(self.render_suggested_lsp_server_row(
                    workspace_path,
                    *server_type,
                    repo_status,
                    mouse_states,
                    appearance,
                ));
            } else {
                let server_model = server_models.and_then(|servers| {
                    servers
                        .iter()
                        .find(|server| server.as_ref(app).server_type() == *server_type)
                });
                card.add_child(self.render_lsp_server_row(
                    workspace_path,
                    *server_type,
                    server_model,
                    *enablement_state == EnablementState::Yes,
                    mouse_states,
                    appearance,
                    app,
                ));
            }
        }

        Container::new(card.finish())
            .with_uniform_padding(MAIN_SECTION_MARGIN)
            .with_background(theme.surface_1())
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.)))
            .with_margin_bottom(MAIN_SECTION_MARGIN)
            .finish()
    }

    /// Renders a suggested LSP server row with its "+" install/enable button.
    fn render_suggested_lsp_server_row(
        &self,
        workspace_path: &Path,
        server_type: LSPServerType,
        repo_status: Option<LspRepoStatus>,
        mouse_states: LspServerRowMouseStates,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let ui_builder = appearance.ui_builder();

        let mut row = Flex::row()
            .with_main_axis_size(MainAxisSize::Max)
            .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

        let mut left_content = Flex::row().with_cross_axis_alignment(CrossAxisAlignment::Center);

        // Language badge with no status dot: a suggested server has no process behind it.
        left_content.add_child(
            Container::new(
                Self::lsp_server_badge(server_type, appearance)
                    .build()
                    .finish(),
            )
            .with_margin_right(8.)
            .finish(),
        );

        let mut name_desc_column = Flex::column().with_spacing(4.);
        name_desc_column.add_child(
            ui_builder
                .span(server_type.binary_name())
                .with_style(UiComponentStyles {
                    font_size: Some(12.0),
                    font_color: Some(theme.active_ui_text_color().into()),
                    ..Default::default()
                })
                .build()
                .finish(),
        );

        let (description, is_installing) = match &repo_status {
            Some(LspRepoStatus::DisabledAndInstalled { .. }) => {
                (crate::t!("settings-code-lsp-installed"), false)
            }
            Some(LspRepoStatus::Installing { .. }) => {
                (crate::t!("settings-code-lsp-installing"), true)
            }
            Some(LspRepoStatus::CheckingForInstallation) => {
                (crate::t!("settings-code-lsp-checking"), true)
            }
            _ => (crate::t!("settings-code-lsp-available-download"), false),
        };

        name_desc_column.add_child(
            ui_builder
                .label(description)
                .with_style(UiComponentStyles {
                    font_color: Some(theme.disabled_ui_text_color().into()),
                    font_size: Some(12.),
                    ..Default::default()
                })
                .build()
                .finish(),
        );

        left_content.add_child(name_desc_column.finish());
        row.add_child(left_content.finish());

        // While an install is in flight there is nothing to press.
        if !is_installing {
            let workspace_path = workspace_path.to_path_buf();
            // No status at all means detection has not answered yet. Treat that as
            // "needs install", so the button never enables a binary that is missing.
            let needs_install = matches!(
                &repo_status,
                None | Some(LspRepoStatus::DisabledAndNotInstalled { .. })
            );
            row.add_child(
                icon_button(appearance, Icon::Plus, false, mouse_states.install)
                    .with_style(UiComponentStyles {
                        border_width: Some(1.),
                        border_color: Some(theme.surface_3().into()),
                        ..Default::default()
                    })
                    .build()
                    .with_cursor(Cursor::PointingHand)
                    .on_click(move |ctx, _, _| {
                        if needs_install {
                            ctx.dispatch_typed_action(
                                CodeSettingsPageAction::InstallAndEnableLspServer {
                                    workspace_path: workspace_path.clone(),
                                    server_type,
                                },
                            );
                        } else {
                            ctx.dispatch_typed_action(
                                CodeSettingsPageAction::EnableSuggestedLspServer {
                                    workspace_path: workspace_path.clone(),
                                    server_type,
                                },
                            );
                        }
                    })
                    .finish(),
            );
        }

        Container::new(row.finish())
            .with_uniform_padding(12.)
            .with_background(theme.surface_2())
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.)))
            .finish()
    }

    /// Renders a known (enabled or disabled) LSP server row: badge, status, the
    /// enable/disable switch, and a restart button when the server has failed.
    #[allow(clippy::too_many_arguments)]
    fn render_lsp_server_row(
        &self,
        workspace_path: &Path,
        server_type: LSPServerType,
        server_model: Option<&ModelHandle<LspServerModel>>,
        is_enabled: bool,
        mouse_states: LspServerRowMouseStates,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let ui_builder = appearance.ui_builder();

        let mut row = Flex::row()
            .with_main_axis_size(MainAxisSize::Max)
            .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

        let mut left_content = Flex::row().with_cross_axis_alignment(CrossAxisAlignment::Center);

        let (status_color, status_text) = Self::lsp_status_info(server_model, app, theme);
        let is_failed = server_model
            .is_some_and(|model| matches!(model.as_ref(app).state(), LspState::Failed { .. }));

        let badge = Self::lsp_server_badge(server_type, appearance)
            .with_status_element_with_offset(
                StatusElementTypes::Circle,
                UiComponentStyles {
                    width: Some(LSP_STATUS_INDICATOR_SIZE),
                    height: Some(LSP_STATUS_INDICATOR_SIZE),
                    border_radius: Some(CornerRadius::with_all(Radius::Percentage(50.))),
                    background: Some(Fill::Solid(status_color)),
                    ..Default::default()
                },
                -5.,
                5.,
            );

        left_content.add_child(
            Container::new(badge.build().finish())
                .with_margin_right(8.)
                .finish(),
        );

        let mut name_status_column = Flex::column().with_spacing(4.);
        name_status_column.add_child(
            ui_builder
                .span(server_type.binary_name())
                .with_style(UiComponentStyles {
                    font_size: Some(12.0),
                    font_color: Some(theme.active_ui_text_color().into()),
                    ..Default::default()
                })
                .build()
                .finish(),
        );

        // A failed server carries its failure in the status text colour, not only in
        // the dot -- the dot alone is 8 pixels of red on a page full of grey.
        let status_text_color = if is_failed {
            Some(status_color)
        } else {
            Some(theme.disabled_ui_text_color().into())
        };

        name_status_column.add_child(
            ui_builder
                .label(status_text)
                .with_style(UiComponentStyles {
                    font_color: status_text_color,
                    font_size: Some(12.),
                    ..Default::default()
                })
                .build()
                .finish(),
        );

        left_content.add_child(name_status_column.finish());
        row.add_child(left_content.finish());

        let mut right_content = Flex::row()
            .with_spacing(8.)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

        if is_failed && let Some(server_handle) = server_model.cloned() {
            right_content.add_child(
                ui_builder
                    .button(ButtonVariant::Secondary, mouse_states.restart)
                    .with_style(UiComponentStyles {
                        font_size: Some(12.),
                        ..Default::default()
                    })
                    .with_hovered_styles(UiComponentStyles {
                        background: Some(theme.surface_3().into()),
                        ..Default::default()
                    })
                    .with_text_label(crate::t!("settings-code-lsp-restart-server"))
                    .build()
                    .with_cursor(Cursor::PointingHand)
                    .on_click(move |ctx, _, _| {
                        ctx.dispatch_typed_action(CodeSettingsPageAction::RestartLspServer {
                            server: server_handle.clone(),
                        });
                    })
                    .finish(),
            );
        }

        // The switch is also the stop/remove control: turning it off removes the
        // server from the manager (which stops it as manually-stopped, so the
        // shutdown manager does not restart it) and persists the disable, so it does
        // not come back on the next buffer load.
        let workspace_path = workspace_path.to_path_buf();
        right_content.add_child(
            ui_builder
                .switch(mouse_states.toggle)
                .check(is_enabled)
                .build()
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(CodeSettingsPageAction::ToggleLspServer {
                        workspace_path: workspace_path.clone(),
                        server_type,
                        currently_enabled: is_enabled,
                    });
                })
                .finish(),
        );

        row.add_child(right_content.finish());

        Container::new(row.finish())
            .with_uniform_padding(12.)
            .with_background(theme.surface_2())
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.)))
            .finish()
    }

    /// The round language badge shared by both row kinds.
    fn lsp_server_badge(server_type: LSPServerType, appearance: &Appearance) -> Avatar {
        let theme = appearance.theme();
        Avatar::new(
            AvatarContent::DisplayName(server_type.binary_name().to_string()),
            UiComponentStyles {
                width: Some(36.),
                height: Some(36.),
                border_radius: Some(CornerRadius::with_all(Radius::Percentage(50.))),
                font_family_id: Some(appearance.ui_font_family()),
                font_weight: Some(Weight::Bold),
                background: Some(theme.surface_3().into()),
                font_size: Some(16.),
                font_color: Some(theme.active_ui_text_color().into()),
                ..Default::default()
            },
        )
    }

    /// Status colour and label for a server row.
    ///
    /// `Available` with work outstanding reports as "Busy", not "Available" -- the
    /// distinction is the pin's, and it is what tells a user that hover or
    /// goto-definition is slow rather than broken.
    fn lsp_status_info(
        server_model: Option<&ModelHandle<LspServerModel>>,
        app: &AppContext,
        theme: &WarpTheme,
    ) -> (ColorU, String) {
        match server_model {
            Some(model) => {
                let server = model.as_ref(app);
                match server.state() {
                    LspState::Available { .. } if !server.has_pending_tasks() => (
                        AnsiColorIdentifier::Green
                            .to_ansi_color(&theme.terminal_colors().normal)
                            .into(),
                        crate::t!("settings-code-lsp-status-available"),
                    ),
                    LspState::Starting | LspState::Available { .. } => (
                        AnsiColorIdentifier::Yellow
                            .to_ansi_color(&theme.terminal_colors().normal)
                            .into(),
                        crate::t!("settings-code-lsp-status-busy"),
                    ),
                    LspState::Failed { .. } => (
                        AnsiColorIdentifier::Red
                            .to_ansi_color(&theme.terminal_colors().normal)
                            .into(),
                        crate::t!("settings-code-lsp-status-failed"),
                    ),
                    LspState::Stopped { .. } | LspState::Stopping { .. } => (
                        theme.disabled_ui_text_color().into_solid(),
                        crate::t!("settings-code-lsp-status-stopped"),
                    ),
                }
            }
            None => (
                theme.disabled_ui_text_color().into_solid(),
                crate::t!("settings-code-lsp-status-not-running"),
            ),
        }
    }
}

#[cfg(test)]
#[path = "code_page_tests.rs"]
mod tests;
