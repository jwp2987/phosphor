//! The Code settings page: after Zap retired the full LSP stack and persisted workspace
//! history, this page has only a handful of local toggles left, related to the editor and
//! code review.
//!
//! Historically this page also hosted the LSP management subpage; that has been retired, so
//! `Code` is no longer an umbrella in the sidebar (there's no second subpage to hang off it)
//! and has become a single-layer Page. What the page renders is exactly this set of toggles.
//!
//! Codebase indexing is back, minus the pin's per-repository index list: the two
//! `code.indexing.*` settings and a read-only readout of the embedding provider they depend
//! on live at the bottom of the page. See the `-- Codebase indexing --` section below.

#[cfg(feature = "local_fs")]
use super::features::external_editor::ExternalEditorView;
use super::{
    settings_page::{
        render_body_item, MatchData, PageType, SettingsPageMeta, SettingsPageViewHandle,
        SettingsWidget,
    },
    LocalOnlyIconState, SettingsAction, SettingsSection, ToggleState,
};
use crate::{
    ai::agent_providers::embeddings,
    appearance::Appearance,
    send_telemetry_from_ctx,
    settings::{AISettings, CodeSettings},
    terminal::general_settings::GeneralSettings,
    workspace::tab_settings::TabSettings,
    workspaces::user_workspaces::UserWorkspaces,
    TelemetryEvent,
};
use ai::project_context::model::{ProjectContextModel, ProjectContextModelEvent};

use settings::Setting as _;
use std::path::PathBuf;
use warp_core::{features::FeatureFlag, report_if_error, settings::ToggleableSetting as _};
use warpui::{
    elements::{ChildView, Container, Element, Empty, Flex, ParentElement},
    keymap::ContextPredicate,
    ui_components::{
        components::{UiComponent, UiComponentStyles},
        switch::SwitchStateHandle,
    },
    Action, AppContext, Entity, SingletonEntity, TypedActionView, View, ViewContext, ViewHandle,
};

pub struct CodeSettingsPageView {
    page: PageType<Self>,
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

        let (page, external_editor_view) = Self::build_page(ctx);

        Self {
            page,
            #[cfg(feature = "local_fs")]
            external_editor_view,
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
                Box::new(CodebaseContextToggleWidget::default()),
                Box::new(AutoIndexingToggleWidget::default()),
                Box::new(CodebaseEmbeddingModelWidget),
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

    /// There is no ExternalEditorView in wasm builds; only the non-external-editor widgets are
    /// rendered. The codebase-indexing widgets are listed here too, but hide themselves --
    /// their settings are desktop-only (see `codebase_indexing_settings_supported`).
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
                    Box::new(CodebaseContextToggleWidget::default()),
                    Box::new(AutoIndexingToggleWidget::default()),
                    Box::new(CodebaseEmbeddingModelWidget),
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
    OpenProjectRules { rule_paths: Vec<PathBuf> },
    ToggleCodeReviewPanel,
    ToggleShowCodeReviewDiffStats,
    ToggleAutoOpenCodeReviewPane,
    ToggleProjectExplorer,
    ToggleGlobalSearch,
    ToggleShowHiddenFiles,
    ToggleCodebaseContext,
    ToggleAutoIndexing,
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
            CodeSettingsPageAction::ToggleCodebaseContext => {
                CodeSettings::handle(ctx).update(ctx, |settings, ctx| {
                    report_if_error!(settings.codebase_context_enabled.toggle_and_save_value(ctx));
                });
                ctx.notify();
            }
            CodeSettingsPageAction::ToggleAutoIndexing => {
                CodeSettings::handle(ctx).update(ctx, |settings, ctx| {
                    report_if_error!(settings.auto_indexing_enabled.toggle_and_save_value(ctx));
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

// -- Codebase indexing --------------------------------------------------------
//
// Restored from the pin (`02b53fcd8:app/src/settings_view/code_page.rs`,
// `CodebaseIndexingCategorizedWidget`) down to the two toggles it owns. The pin's
// per-repository index list, its "index limit reached" banner and its team-admin override
// are not ported: the first two belong to a manager this fork wires up elsewhere, and the
// third arrived from Warp's server. What replaces the admin override is the embedding
// readout below, because the fork's equivalent question -- "will this actually index
// anything?" -- is answered locally, by whether a provider serves an embedding model.

/// Whether the codebase-indexing controls can do anything on this build.
///
/// Mirrors `crate::ai::codebase_auto_indexing::codebase_indexing_enabled`: without
/// `FullSourceCodeEmbedding` nothing indexes whatever the settings say, and both settings are
/// desktop-only, so on other platforms the rows would write values nothing reads.
fn codebase_indexing_settings_supported(app: &AppContext) -> bool {
    FeatureFlag::FullSourceCodeEmbedding.is_enabled()
        && CodeSettings::as_ref(app)
            .codebase_context_enabled
            .is_supported_on_current_platform()
}

/// A provider's display name, falling back to its id.
///
/// A provider is usable as soon as it lists a model, which can happen before the user has
/// named it; an empty label in the readout would be worse than the raw id.
fn provider_display_name(name: &str, id: &str) -> String {
    let name = name.trim();
    if name.is_empty() {
        id.to_string()
    } else {
        name.to_string()
    }
}

#[derive(Default)]
struct CodebaseContextToggleWidget {
    switch_state: SwitchStateHandle,
}

impl SettingsWidget for CodebaseContextToggleWidget {
    type View = CodeSettingsPageView;

    fn search_terms(&self) -> &str {
        "codebase context indexing index repository embeddings agent code search"
    }

    fn should_render(&self, app: &AppContext) -> bool {
        codebase_indexing_settings_supported(app)
    }

    fn render(
        &self,
        _view: &Self::View,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        // The switch shows `is_codebase_context_enabled`, i.e. the value the agent actually
        // reads, not the raw setting: it is ANDed with the global AI toggle, so with AI off
        // the setting is inert. The pin disabled the control in that case for the same
        // reason, and a switch the user can flip with no effect is worse than a dead one.
        let ai_enabled = AISettings::as_ref(app).is_any_ai_enabled(app);
        let enabled = UserWorkspaces::as_ref(app).is_codebase_context_enabled(app);

        render_body_item::<CodeSettingsPageAction>(
            crate::t!("settings-code-codebase-context"),
            None,
            LocalOnlyIconState::Hidden,
            ToggleState::from(ai_enabled),
            appearance,
            appearance
                .ui_builder()
                .switch(self.switch_state.clone())
                .check(enabled)
                .with_disabled(!ai_enabled)
                .build()
                .on_click(move |ctx, _, _| {
                    if !ai_enabled {
                        return;
                    }
                    ctx.dispatch_typed_action(CodeSettingsPageAction::ToggleCodebaseContext);
                })
                .finish(),
            Some(crate::t!("settings-code-codebase-context-desc")),
        )
    }
}

#[derive(Default)]
struct AutoIndexingToggleWidget {
    switch_state: SwitchStateHandle,
}

impl SettingsWidget for AutoIndexingToggleWidget {
    type View = CodeSettingsPageView;

    fn search_terms(&self) -> &str {
        "auto indexing automatic index new folders repositories codebase context"
    }

    /// Hidden rather than greyed when codebase context is off, which is what the pin does:
    /// it renders this row only inside `global_ai_enabled && codebase_context_enabled`.
    fn should_render(&self, app: &AppContext) -> bool {
        codebase_indexing_settings_supported(app)
            && UserWorkspaces::as_ref(app).is_codebase_context_enabled(app)
    }

    fn render(
        &self,
        _view: &Self::View,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let code_settings = CodeSettings::as_ref(app);

        render_body_item::<CodeSettingsPageAction>(
            crate::t!("settings-code-auto-indexing"),
            None,
            LocalOnlyIconState::Hidden,
            ToggleState::Enabled,
            appearance,
            appearance
                .ui_builder()
                .switch(self.switch_state.clone())
                .check(*code_settings.auto_indexing_enabled)
                .build()
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(CodeSettingsPageAction::ToggleAutoIndexing);
                })
                .finish(),
            Some(crate::t!("settings-code-auto-indexing-desc")),
        )
    }
}

/// Reports which embedding model and provider the codebase index resolves to.
///
/// Read-only on purpose. There is no "embedding model" setting to expose: a model is chosen
/// by listing its id on a provider under Settings > AI, and
/// `embeddings::resolve_configured_embedding_model` takes the first entry of
/// `SUPPORTED_EMBEDDING_MODELS` that a usable provider serves. Adding a persisted preference
/// here would be a second source of truth that could name a model no provider serves, which
/// is exactly the split `app/src/ai/agent_providers/embeddings.rs` was written to avoid.
///
/// What it must not do is stay quiet: an unconfigured index fails with
/// `Error::NoEmbeddingProvider` rather than silently defaulting, so the unconfigured state is
/// spelled out here and points at the page that fixes it.
struct CodebaseEmbeddingModelWidget;

impl SettingsWidget for CodebaseEmbeddingModelWidget {
    type View = CodeSettingsPageView;

    fn search_terms(&self) -> &str {
        "embedding model provider codebase index vectors voyage openai"
    }

    fn should_render(&self, app: &AppContext) -> bool {
        codebase_indexing_settings_supported(app)
    }

    fn render(
        &self,
        _view: &Self::View,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let resolved = embeddings::resolve_configured_embedding_model(app).and_then(|config| {
            embeddings::resolve_embedding_provider(app, config).map(|provider| (config, provider))
        });

        let (value_text, value_color) = match &resolved {
            Some((config, provider)) => (
                crate::t!(
                    "settings-code-embedding-model-value",
                    model = config.model_id(),
                    provider = provider_display_name(&provider.name, &provider.id)
                ),
                appearance.theme().active_ui_text_color(),
            ),
            None => (
                crate::t!("settings-code-embedding-model-none"),
                appearance.theme().disabled_ui_text_color(),
            ),
        };

        let description = if resolved.is_some() {
            crate::t!("settings-code-embedding-model-desc")
        } else {
            crate::t!("settings-code-embedding-model-none-desc")
        };

        let mut column = Flex::column();
        column.add_child(render_body_item::<CodeSettingsPageAction>(
            crate::t!("settings-code-embedding-model"),
            None,
            LocalOnlyIconState::Hidden,
            ToggleState::from(AISettings::as_ref(app).is_any_ai_enabled(app)),
            appearance,
            appearance
                .ui_builder()
                .span(value_text)
                .with_style(UiComponentStyles {
                    font_color: Some(value_color.into()),
                    font_size: Some(appearance.ui_font_body()),
                    ..Default::default()
                })
                .build()
                .finish(),
            Some(description),
        ));

        // The candidate list, in preference order, so "add one of these to a provider" is an
        // instruction the user can follow without reading the source.
        let mut candidates = Flex::column();
        for config in embeddings::SUPPORTED_EMBEDDING_MODELS {
            let provider = embeddings::resolve_embedding_provider(app, *config);
            let (line, color) = match &provider {
                Some(provider) => (
                    crate::t!(
                        "settings-code-embedding-candidate-available",
                        model = config.model_id(),
                        provider = provider_display_name(&provider.name, &provider.id)
                    ),
                    appearance.theme().active_ui_text_color(),
                ),
                None => (
                    crate::t!(
                        "settings-code-embedding-candidate-unavailable",
                        model = config.model_id()
                    ),
                    appearance.theme().disabled_ui_text_color(),
                ),
            };
            candidates.add_child(
                Container::new(
                    appearance
                        .ui_builder()
                        .span(line)
                        .with_style(UiComponentStyles {
                            font_color: Some(color.into()),
                            font_size: Some(appearance.ui_font_footnote()),
                            ..Default::default()
                        })
                        .build()
                        .finish(),
                )
                .with_margin_bottom(2.)
                .finish(),
            );
        }
        column.add_child(
            Container::new(candidates.finish())
                .with_margin_left(8.)
                .with_margin_bottom(12.)
                .finish(),
        );

        column.finish()
    }
}

#[cfg(test)]
#[path = "code_page_tests.rs"]
mod tests;
