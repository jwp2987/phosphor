//! The custom Agent Provider settings panel widget.
//!
//! UI shape:
//! - Sub-header (title on the left + a small `+ Add provider` button in the top-right corner) +
//!   brief description
//! - One card per provider, each card containing:
//!   . Three input boxes for `Name` / `Base URL` / `API Key` (edit-only, not auto-saved)
//!   . A model list area: header row `Display name | Model ID`, two input boxes per row + a
//!     `x` delete button
//!   . A bottom button row: `+ Add model` `Fetch from API` `Save` `Remove` (provider)
//!
//! **Save behavior**: clicking the "Save" button pushes the form state to `AISettings` and
//! `AgentProviderSecrets` in one shot. Blurring an input box / pressing Enter does not save --
//! this is to avoid the user being "implicitly committed" while still editing. Structural
//! operations that rebuild the page (adding/removing a model row, adding/removing a header row,
//! the API protocol chip, model capability chips) commit the current card's draft first, then
//! perform the original operation, so unsaved input isn't lost on rebuild.
//!
//! When the provider list's size or a given provider's model count changes,
//! `AISettingsPageView::rebuild_current_page` is triggered to rebuild the entire widget, so
//! that added/removed entries get their own EditorView handle.
//! Internally, `rebuild_current_page` reuses the old PageType's vertical scroll handle, so the
//! scroll position isn't reset.
//!
//! Provider metadata (name/base_url/models) goes through `settings.toml`; `api_key` goes
//! through the OS keychain (`AgentProviderSecrets`).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use settings::Setting;
use warpui::elements::{
    ChildView, Container, CornerRadius, CrossAxisAlignment, Expanded, Flex, MainAxisAlignment,
    MouseStateHandle, ParentElement, Radius, Text, Wrap,
};
use warpui::ui_components::{
    button::ButtonVariant,
    components::{Coords, UiComponent, UiComponentStyles},
};
use warpui::{AppContext, Element, SingletonEntity, ViewContext, ViewHandle};

use crate::ai::agent_providers::AgentProviderSecrets;
use crate::appearance::Appearance;
use crate::editor::{
    EditorView, Event as EditorEvent, SingleLineEditorOptions, TextColors, TextOptions,
};
use crate::settings::{AISettings, AgentProvider, AgentProviderApiType, AgentProviderModel};
use strum::IntoEnumIterator;

use super::ai_page::{AISettingsPageAction, AISettingsPageView, ModelCapabilityKind};
use super::settings_page::{
    build_sub_header, render_customer_type_badge, SettingsWidget, HEADER_PADDING,
};

const CARD_BUTTON_PADDING: f32 = 6.0;
const FIELD_LABEL_MARGIN_TOP: f32 = 6.0;
const FIELD_LABEL_MARGIN_BOTTOM: f32 = 2.0;
const MODEL_ROW_GAP: f32 = 6.0;

// ---------------------------------------------------------------------------
// Model-row expansion state (process-local, thread_local single-threaded-UI safe; not
// persisted)
// ---------------------------------------------------------------------------

std::thread_local! {
    /// {provider_id => Set<model_index>} the currently expanded model entries.
    /// Lost when the settings page is closed, similar to the AtomicBool behavior of
    /// `models_dev::chips_expanded()`.
    static EXPANDED_MODELS: RefCell<HashMap<String, HashSet<usize>>> = RefCell::new(HashMap::new());
}

pub(super) fn is_model_expanded(provider_id: &str, model_index: usize) -> bool {
    EXPANDED_MODELS.with(|m| {
        m.borrow()
            .get(provider_id)
            .is_some_and(|set| set.contains(&model_index))
    })
}

pub(super) fn toggle_model_expanded(provider_id: &str, model_index: usize) {
    EXPANDED_MODELS.with(|m| {
        let mut map = m.borrow_mut();
        let set = map.entry(provider_id.to_string()).or_default();
        if !set.insert(model_index) {
            set.remove(&model_index);
        }
    });
}

/// Clears the expansion record for a provider when it's deleted, avoiding index drift.
pub(super) fn clear_expanded_models_for_provider(provider_id: &str) {
    EXPANDED_MODELS.with(|m| {
        m.borrow_mut().remove(provider_id);
    });
}

/// Whether the "Disabled providers" section is expanded. Collapsed by default (`false`) so a
/// list with several disabled providers doesn't bury the active ones -- same
/// process-local/not-persisted treatment as `models_dev::chips_expanded()`.
static DISABLED_SECTION_EXPANDED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub(super) fn disabled_section_expanded() -> bool {
    DISABLED_SECTION_EXPANDED.load(std::sync::atomic::Ordering::Relaxed)
}

pub(super) fn toggle_disabled_section_expanded() {
    DISABLED_SECTION_EXPANDED.fetch_xor(true, std::sync::atomic::Ordering::Relaxed);
}

/// The editable view handles for a single model entry (name + id + context + output).
struct ModelRow {
    name_editor: ViewHandle<EditorView>,
    id_editor: ViewHandle<EditorView>,
    context_editor: ViewHandle<EditorView>,
    output_editor: ViewHandle<EditorView>,
    /// The remove button inside the detail panel.
    remove_button_state: MouseStateHandle,
    /// The quick-remove button to the right of the chevron at the end of the row.
    quick_remove_button_state: MouseStateHandle,
    /// The expand/collapse chevron at the end of the row.
    expand_button_state: MouseStateHandle,
    /// Mouse state for the tri-state image/pdf/audio chips inside the detail panel.
    image_chip_state: MouseStateHandle,
    pdf_chip_state: MouseStateHandle,
    audio_chip_state: MouseStateHandle,
    /// State for the two bool toggles inside the detail panel: reasoning / tool_call.
    reasoning_chip_state: MouseStateHandle,
    tool_call_chip_state: MouseStateHandle,
}

struct HeaderRow {
    key_editor: ViewHandle<EditorView>,
    val_editor: ViewHandle<EditorView>,
    remove_button_state: MouseStateHandle,
}

/// All editable view handles for a single provider row.
struct ProviderRow {
    name_editor: ViewHandle<EditorView>,
    base_url_editor: ViewHandle<EditorView>,
    api_key_editor: ViewHandle<EditorView>,
    /// Vertex AI only: GCP project id + location. Rendered in place of base_url when the
    /// selected api_type is Vertex.
    vertex_project_editor: ViewHandle<EditorView>,
    vertex_location_editor: ViewHandle<EditorView>,
    fetch_button_state: MouseStateHandle,
    sync_models_dev_button_state: MouseStateHandle,
    save_button_state: MouseStateHandle,
    remove_button_state: MouseStateHandle,
    disable_toggle_button_state: MouseStateHandle,
    add_model_button_state: MouseStateHandle,
    header_rows: Vec<HeaderRow>,
    add_header_button_state: MouseStateHandle,
    /// Mouse state for each of the 5 ApiType chips. The HashMap is keyed by chip display name.
    api_type_chip_states: RefCell<HashMap<AgentProviderApiType, MouseStateHandle>>,
    model_rows: Vec<ModelRow>,
}

type ModelDraftEditorHandles = (
    usize,
    ViewHandle<EditorView>,
    ViewHandle<EditorView>,
    ViewHandle<EditorView>,
    ViewHandle<EditorView>,
);

#[derive(Clone)]
struct ProviderDraftEditors {
    provider_id: String,
    name_editor: ViewHandle<EditorView>,
    base_url_editor: ViewHandle<EditorView>,
    api_key_editor: ViewHandle<EditorView>,
    vertex_project_editor: ViewHandle<EditorView>,
    vertex_location_editor: ViewHandle<EditorView>,
    header_editors: Vec<(ViewHandle<EditorView>, ViewHandle<EditorView>)>,
    model_editors: Vec<ModelDraftEditorHandles>,
}

impl ProviderDraftEditors {
    fn from_row(provider_id: String, row: &ProviderRow) -> Self {
        Self {
            provider_id,
            name_editor: row.name_editor.clone(),
            base_url_editor: row.base_url_editor.clone(),
            api_key_editor: row.api_key_editor.clone(),
            vertex_project_editor: row.vertex_project_editor.clone(),
            vertex_location_editor: row.vertex_location_editor.clone(),
            header_editors: row
                .header_rows
                .iter()
                .map(|h| (h.key_editor.clone(), h.val_editor.clone()))
                .collect(),
            model_editors: row
                .model_rows
                .iter()
                .enumerate()
                .map(|(idx, m)| {
                    (
                        idx,
                        m.name_editor.clone(),
                        m.id_editor.clone(),
                        m.context_editor.clone(),
                        m.output_editor.clone(),
                    )
                })
                .collect(),
        }
    }

    fn to_save_action(&self, app: &AppContext) -> AISettingsPageAction {
        self.to_save_action_with(
            app,
            |provider_id, name, base_url, api_key, vertex_project, vertex_location, headers, models| {
                AISettingsPageAction::SaveAgentProviderEdits {
                    provider_id,
                    name,
                    base_url,
                    api_key,
                    vertex_project,
                    vertex_location,
                    headers,
                    models,
                }
            },
        )
    }

    fn to_save_then_action(
        &self,
        app: &AppContext,
        action: AISettingsPageAction,
    ) -> AISettingsPageAction {
        self.to_save_action_with(
            app,
            |provider_id, name, base_url, api_key, vertex_project, vertex_location, headers, models| {
                AISettingsPageAction::SaveAgentProviderEditsThen {
                    provider_id,
                    name,
                    base_url,
                    api_key,
                    vertex_project,
                    vertex_location,
                    headers,
                    models,
                    action: Box::new(action),
                }
            },
        )
    }

    fn to_save_action_with(
        &self,
        app: &AppContext,
        build: impl FnOnce(
            String,
            String,
            String,
            String,
            String,
            String,
            Vec<(String, String)>,
            Vec<(usize, String, String, u32, u32)>,
        ) -> AISettingsPageAction,
    ) -> AISettingsPageAction {
        let name = self.name_editor.as_ref(app).buffer_text(app);
        let base_url = self.base_url_editor.as_ref(app).buffer_text(app);
        let api_key = self.api_key_editor.as_ref(app).buffer_text(app);
        let vertex_project = self.vertex_project_editor.as_ref(app).buffer_text(app);
        let vertex_location = self.vertex_location_editor.as_ref(app).buffer_text(app);
        let headers: Vec<(String, String)> = self
            .header_editors
            .iter()
            .map(|(k, v)| {
                (
                    k.as_ref(app).buffer_text(app),
                    v.as_ref(app).buffer_text(app),
                )
            })
            .collect();
        let models: Vec<(usize, String, String, u32, u32)> = self
            .model_editors
            .iter()
            .map(|(idx, name_e, id_e, ctx_e, out_e)| {
                let m_name = name_e.as_ref(app).buffer_text(app);
                let m_id = id_e.as_ref(app).buffer_text(app);
                let context_window = parse_token_count(&ctx_e.as_ref(app).buffer_text(app));
                let max_output_tokens = parse_token_count(&out_e.as_ref(app).buffer_text(app));
                (*idx, m_name, m_id, context_window, max_output_tokens)
            })
            .collect();

        build(
            self.provider_id.clone(),
            name,
            base_url,
            api_key,
            vertex_project,
            vertex_location,
            headers,
            models,
        )
    }
}

/// The custom Agent Provider settings widget.
pub(super) struct AgentProvidersWidget {
    add_button_state: MouseStateHandle,
    refresh_catalog_button_state: MouseStateHandle,
    expand_chips_button_state: MouseStateHandle,
    disabled_section_toggle_button_state: MouseStateHandle,
    /// The search box for the quick-add chip row.
    search_editor: ViewHandle<EditorView>,
    /// One button state per catalog provider id -- used by the chip row.
    quick_add_button_states: RefCell<HashMap<String, MouseStateHandle>>,
    rows: RefCell<HashMap<String, ProviderRow>>,
}

impl AgentProvidersWidget {
    pub(super) fn new(ctx: &mut ViewContext<AISettingsPageView>) -> Self {
        let providers = AISettings::as_ref(ctx).agent_providers.value().clone();
        let mut rows = HashMap::with_capacity(providers.len());
        for provider in &providers {
            let row = Self::build_row(provider, ctx);
            rows.insert(provider.id.clone(), row);
        }

        // Triggers a catalog load (disk cache + network if needed) as soon as the page is
        // entered.
        ctx.dispatch_typed_action_deferred(AISettingsPageAction::EnsureModelsDevLoaded);

        // ---- Search box ----
        let initial_query = crate::ai::agent_providers::models_dev::search_query();
        let search_editor = ctx.add_typed_action_view(move |ctx| {
            let appearance = Appearance::handle(ctx).as_ref(ctx);
            let options = single_line_editor_options(appearance, false);
            let mut editor = EditorView::single_line(options, ctx);
            editor.set_placeholder_text(
                crate::t!("settings-agent-providers-search-placeholder"),
                ctx,
            );
            if !initial_query.is_empty() {
                editor.set_buffer_text(&initial_query, ctx);
            }
            editor
        });
        ctx.subscribe_to_view(&search_editor, move |_, editor, event, ctx| {
            if matches!(event, EditorEvent::Edited(_)) {
                let buffer_text = editor.as_ref(ctx).buffer_text(ctx);
                ctx.dispatch_typed_action_deferred(AISettingsPageAction::SetModelsDevSearchQuery(
                    buffer_text,
                ));
            }
        });

        Self {
            add_button_state: MouseStateHandle::default(),
            refresh_catalog_button_state: MouseStateHandle::default(),
            expand_chips_button_state: MouseStateHandle::default(),
            disabled_section_toggle_button_state: MouseStateHandle::default(),
            search_editor,
            quick_add_button_states: RefCell::new(HashMap::new()),
            rows: RefCell::new(rows),
        }
    }

    /// Constructs the EditorView and subscriptions for a single model row.
    fn build_model_row(
        model: &AgentProviderModel,
        ctx: &mut ViewContext<AISettingsPageView>,
    ) -> ModelRow {
        // ---- name editor ----
        let initial_name = model.name.clone();
        let name_editor = ctx.add_typed_action_view(move |ctx| {
            let appearance = Appearance::handle(ctx).as_ref(ctx);
            let options = single_line_editor_options(appearance, false);
            let mut editor = EditorView::single_line(options, ctx);
            editor.set_placeholder_text(
                crate::t!("settings-agent-providers-model-name-placeholder"),
                ctx,
            );
            if !initial_name.is_empty() {
                editor.set_buffer_text(&initial_name, ctx);
            }
            editor
        });
        // Only responsible for collapsing the selection on blur; no longer saves implicitly --
        // saving goes through the "Save" button at the bottom.
        ctx.subscribe_to_view(&name_editor, move |_, editor, event, ctx| {
            collapse_selection_if_blurred(&editor, event, ctx);
        });

        // ---- id editor ----
        let initial_id = model.id.clone();
        let id_editor = ctx.add_typed_action_view(move |ctx| {
            let appearance = Appearance::handle(ctx).as_ref(ctx);
            let options = single_line_editor_options(appearance, false);
            let mut editor = EditorView::single_line(options, ctx);
            editor.set_placeholder_text(
                crate::t!("settings-agent-providers-model-id-placeholder"),
                ctx,
            );
            if !initial_id.is_empty() {
                editor.set_buffer_text(&initial_id, ctx);
            }
            editor
        });
        ctx.subscribe_to_view(&id_editor, move |_, editor, event, ctx| {
            collapse_selection_if_blurred(&editor, event, ctx);
        });

        // ---- context_window editor (numeric; empty = 0 = unspecified) ----
        let initial_context = if model.context_window == 0 {
            String::new()
        } else {
            model.context_window.to_string()
        };
        let context_editor = ctx.add_typed_action_view(move |ctx| {
            let appearance = Appearance::handle(ctx).as_ref(ctx);
            let options = single_line_editor_options(appearance, false);
            let mut editor = EditorView::single_line(options, ctx);
            editor.set_placeholder_text(
                crate::t!("settings-agent-providers-model-context-placeholder"),
                ctx,
            );
            if !initial_context.is_empty() {
                editor.set_buffer_text(&initial_context, ctx);
            }
            editor
        });
        ctx.subscribe_to_view(&context_editor, move |_, editor, event, ctx| {
            collapse_selection_if_blurred(&editor, event, ctx);
        });

        // ---- max_output_tokens editor ----
        let initial_output = if model.max_output_tokens == 0 {
            String::new()
        } else {
            model.max_output_tokens.to_string()
        };
        let output_editor = ctx.add_typed_action_view(move |ctx| {
            let appearance = Appearance::handle(ctx).as_ref(ctx);
            let options = single_line_editor_options(appearance, false);
            let mut editor = EditorView::single_line(options, ctx);
            editor.set_placeholder_text(
                crate::t!("settings-agent-providers-model-output-placeholder"),
                ctx,
            );
            if !initial_output.is_empty() {
                editor.set_buffer_text(&initial_output, ctx);
            }
            editor
        });
        ctx.subscribe_to_view(&output_editor, move |_, editor, event, ctx| {
            collapse_selection_if_blurred(&editor, event, ctx);
        });

        ModelRow {
            name_editor,
            id_editor,
            context_editor,
            output_editor,
            remove_button_state: MouseStateHandle::default(),
            quick_remove_button_state: MouseStateHandle::default(),
            expand_button_state: MouseStateHandle::default(),
            image_chip_state: MouseStateHandle::default(),
            pdf_chip_state: MouseStateHandle::default(),
            audio_chip_state: MouseStateHandle::default(),
            reasoning_chip_state: MouseStateHandle::default(),
            tool_call_chip_state: MouseStateHandle::default(),
        }
    }

    fn build_header_row(
        key: &str,
        value: &str,
        ctx: &mut ViewContext<AISettingsPageView>,
    ) -> HeaderRow {
        let initial_key = key.to_owned();
        let key_editor = ctx.add_typed_action_view(move |ctx| {
            let appearance = Appearance::handle(ctx).as_ref(ctx);
            let options = single_line_editor_options(appearance, false);
            let mut editor = EditorView::single_line(options, ctx);
            editor.set_placeholder_text("x-portkey-provider", ctx);
            if !initial_key.is_empty() {
                editor.set_buffer_text(&initial_key, ctx);
            }
            editor
        });

        let initial_value = value.to_owned();
        let val_editor = ctx.add_typed_action_view(move |ctx| {
            let appearance = Appearance::handle(ctx).as_ref(ctx);
            let options = single_line_editor_options(appearance, false);
            let mut editor = EditorView::single_line(options, ctx);
            editor.set_placeholder_text("openai", ctx);
            if !initial_value.is_empty() {
                editor.set_buffer_text(&initial_value, ctx);
            }
            editor
        });

        // Saving a header row likewise goes through the "Save" button at the bottom; this is
        // only responsible for collapsing the selection on blur.
        // (header_index / provider_id / val_editor are still read live inside build_row as
        // `HeaderRow`.)
        ctx.subscribe_to_view(&key_editor, move |_, editor, event, ctx| {
            collapse_selection_if_blurred(&editor, event, ctx);
        });

        ctx.subscribe_to_view(&val_editor, move |_, editor, event, ctx| {
            collapse_selection_if_blurred(&editor, event, ctx);
        });

        HeaderRow {
            key_editor,
            val_editor,
            remove_button_state: MouseStateHandle::default(),
        }
    }

    /// Constructs all view handles and button mouse states for a single provider.
    fn build_row(
        provider: &AgentProvider,
        ctx: &mut ViewContext<AISettingsPageView>,
    ) -> ProviderRow {
        let provider_id = provider.id.clone();

        // ---- Name editor ----
        let initial_name = provider.name.clone();
        let name_editor = ctx.add_typed_action_view(move |ctx| {
            let appearance = Appearance::handle(ctx).as_ref(ctx);
            let options = single_line_editor_options(appearance, false);
            let mut editor = EditorView::single_line(options, ctx);
            editor
                .set_placeholder_text(crate::t!("settings-agent-providers-name-placeholder"), ctx);
            if !initial_name.is_empty() {
                editor.set_buffer_text(&initial_name, ctx);
            }
            editor
        });
        // Only responsible for collapsing the selection on blur; saving goes through the "Save"
        // button at the bottom.
        ctx.subscribe_to_view(&name_editor, move |_, editor, event, ctx| {
            collapse_selection_if_blurred(&editor, event, ctx);
        });

        // ---- Base URL editor ----
        let initial_base_url = provider.base_url.clone();
        let base_url_editor = ctx.add_typed_action_view(move |ctx| {
            let appearance = Appearance::handle(ctx).as_ref(ctx);
            let options = single_line_editor_options(appearance, false);
            let mut editor = EditorView::single_line(options, ctx);
            editor.set_placeholder_text(
                crate::t!("settings-agent-providers-base-url-placeholder"),
                ctx,
            );
            if !initial_base_url.is_empty() {
                editor.set_buffer_text(&initial_base_url, ctx);
            }
            editor
        });
        ctx.subscribe_to_view(&base_url_editor, move |_, editor, event, ctx| {
            collapse_selection_if_blurred(&editor, event, ctx);
        });

        // ---- API Key editor (password mode) ----
        let initial_api_key = AgentProviderSecrets::as_ref(ctx)
            .get(&provider_id)
            .map(str::to_owned)
            .unwrap_or_default();
        let api_key_editor = ctx.add_typed_action_view(move |ctx| {
            let appearance = Appearance::handle(ctx).as_ref(ctx);
            let options = single_line_editor_options(appearance, true);
            let mut editor = EditorView::single_line(options, ctx);
            editor.set_placeholder_text(
                crate::t!("settings-agent-providers-api-key-placeholder"),
                ctx,
            );
            if !initial_api_key.is_empty() {
                editor.set_buffer_text(&initial_api_key, ctx);
            }
            editor
        });
        ctx.subscribe_to_view(&api_key_editor, move |_, editor, event, ctx| {
            collapse_selection_if_blurred(&editor, event, ctx);
        });

        // ---- Vertex project / location editors (only rendered when api_type == Vertex) ----
        let initial_vertex_project = provider.vertex_project.clone();
        let vertex_project_editor = ctx.add_typed_action_view(move |ctx| {
            let appearance = Appearance::handle(ctx).as_ref(ctx);
            let options = single_line_editor_options(appearance, false);
            let mut editor = EditorView::single_line(options, ctx);
            editor.set_placeholder_text(
                crate::t!("settings-agent-providers-vertex-project-placeholder"),
                ctx,
            );
            if !initial_vertex_project.is_empty() {
                editor.set_buffer_text(&initial_vertex_project, ctx);
            }
            editor
        });
        ctx.subscribe_to_view(&vertex_project_editor, move |_, editor, event, ctx| {
            collapse_selection_if_blurred(&editor, event, ctx);
        });

        let initial_vertex_location = provider.vertex_location.clone();
        let vertex_location_editor = ctx.add_typed_action_view(move |ctx| {
            let appearance = Appearance::handle(ctx).as_ref(ctx);
            let options = single_line_editor_options(appearance, false);
            let mut editor = EditorView::single_line(options, ctx);
            editor.set_placeholder_text(
                crate::t!("settings-agent-providers-vertex-location-placeholder"),
                ctx,
            );
            if !initial_vertex_location.is_empty() {
                editor.set_buffer_text(&initial_vertex_location, ctx);
            }
            editor
        });
        ctx.subscribe_to_view(&vertex_location_editor, move |_, editor, event, ctx| {
            collapse_selection_if_blurred(&editor, event, ctx);
        });

        // ---- Model rows ----
        let model_rows: Vec<ModelRow> = provider
            .models
            .iter()
            .map(|m| Self::build_model_row(m, ctx))
            .collect();

        let header_rows: Vec<HeaderRow> = provider
            .extra_headers
            .iter()
            .map(|(k, v)| Self::build_header_row(k, v, ctx))
            .collect();
        let add_header_button_state = MouseStateHandle::default();

        ProviderRow {
            name_editor,
            base_url_editor,
            api_key_editor,
            vertex_project_editor,
            vertex_location_editor,
            fetch_button_state: MouseStateHandle::default(),
            sync_models_dev_button_state: MouseStateHandle::default(),
            save_button_state: MouseStateHandle::default(),
            remove_button_state: MouseStateHandle::default(),
            disable_toggle_button_state: MouseStateHandle::default(),
            add_model_button_state: MouseStateHandle::default(),
            header_rows,
            add_header_button_state,
            api_type_chip_states: RefCell::new(HashMap::new()),
            model_rows,
        }
    }

    /// Renders the "API Type" row: 5 chips laid out horizontally, with the currently selected
    /// one highlighted.
    /// Clicking a chip dispatches `SetAgentProviderApiType`, and the backend fills in the
    /// default endpoint along the way.
    fn render_api_type_field(
        &self,
        provider: &AgentProvider,
        row: &ProviderRow,
        draft_editors: ProviderDraftEditors,
        label_color: warp_core::ui::theme::Fill,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let label_text = Container::new(
            Text::new(
                crate::t!("settings-agent-providers-field-api-type"),
                appearance.ui_font_family(),
                appearance.ui_font_size(),
            )
            .with_color(label_color.into())
            .finish(),
        )
        .with_margin_top(FIELD_LABEL_MARGIN_TOP)
        .with_margin_bottom(FIELD_LABEL_MARGIN_BOTTOM)
        .finish();

        let mut chip_row = Flex::row().with_cross_axis_alignment(CrossAxisAlignment::Center);
        {
            let mut states = row.api_type_chip_states.borrow_mut();
            for variant in AgentProviderApiType::iter() {
                let state = states.entry(variant).or_default().clone();
                let is_selected = provider.api_type == variant;
                let label = if is_selected {
                    format!("● {}", variant.display_name())
                } else {
                    variant.display_name().to_owned()
                };
                let chip = Self::render_card_button_preserving_draft(
                    label,
                    state,
                    draft_editors.clone(),
                    AISettingsPageAction::SetAgentProviderApiType {
                        provider_id: provider.id.clone(),
                        api_type: variant,
                    },
                    appearance,
                );
                chip_row = chip_row.with_child(Container::new(chip).with_margin_right(6.).finish());
            }
        }

        let hint_text = Container::new(
            Text::new(
                crate::t!(
                    "settings-agent-providers-api-type-hint",
                    url = provider.api_type.default_base_url()
                ),
                appearance.ui_font_family(),
                appearance.ui_font_size(),
            )
            .with_color(appearance.theme().disabled_ui_text_color().into())
            .soft_wrap(true)
            .finish(),
        )
        .with_margin_top(2.)
        .finish();

        Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_child(label_text)
            .with_child(chip_row.finish())
            .with_child(hint_text)
            .finish()
    }

    fn render_card_button(
        label: impl Into<String>,
        mouse_state: MouseStateHandle,
        action: AISettingsPageAction,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        appearance
            .ui_builder()
            .button(ButtonVariant::Secondary, mouse_state)
            .with_style(UiComponentStyles {
                font_size: Some(appearance.ui_font_body()),
                padding: Some(Coords::uniform(CARD_BUTTON_PADDING)),
                ..Default::default()
            })
            .with_centered_text_label(label.into())
            .build()
            .on_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(action.clone());
            })
            .finish()
    }

    fn render_card_button_preserving_draft(
        label: impl Into<String>,
        mouse_state: MouseStateHandle,
        draft_editors: ProviderDraftEditors,
        action: AISettingsPageAction,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        appearance
            .ui_builder()
            .button(ButtonVariant::Secondary, mouse_state)
            .with_style(UiComponentStyles {
                font_size: Some(appearance.ui_font_body()),
                padding: Some(Coords::uniform(CARD_BUTTON_PADDING)),
                ..Default::default()
            })
            .with_centered_text_label(label.into())
            .build()
            .on_click(move |ctx, app, _| {
                ctx.dispatch_typed_action(draft_editors.to_save_then_action(app, action.clone()));
            })
            .finish()
    }

    fn render_model_row(
        provider: &AgentProvider,
        index: usize,
        model: &AgentProviderModel,
        row: &ModelRow,
        draft_editors: ProviderDraftEditors,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let provider_id = provider.id.as_str();
        let is_expanded = is_model_expanded(provider_id, index);

        // chevron: expanded ▾ / collapsed ▸. Reuses render_card_button's visual style.
        let chevron_label = if is_expanded { "▾" } else { "▸" };
        let chevron_button = Self::render_card_button_preserving_draft(
            chevron_label,
            row.expand_button_state.clone(),
            draft_editors.clone(),
            AISettingsPageAction::ToggleAgentProviderModelExpanded {
                provider_id: provider.id.clone(),
                model_index: index,
            },
            appearance,
        );
        let quick_remove_button = Self::render_card_button_preserving_draft(
            "×",
            row.quick_remove_button_state.clone(),
            draft_editors.clone(),
            AISettingsPageAction::RemoveAgentProviderModel {
                provider_id: provider.id.clone(),
                model_index: index,
            },
            appearance,
        );
        let row_controls = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(
                Container::new(chevron_button)
                    .with_margin_right(MODEL_ROW_GAP)
                    .finish(),
            )
            .with_child(quick_remove_button)
            .finish();

        let cell = |flex: f32, view: &ViewHandle<EditorView>| -> Box<dyn Element> {
            Expanded::new(
                flex,
                Container::new(ChildView::new(view).finish())
                    .with_margin_right(MODEL_ROW_GAP)
                    .finish(),
            )
            .finish()
        };

        let header_row = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(cell(2., &row.name_editor))
            .with_child(cell(2., &row.id_editor))
            .with_child(cell(1., &row.context_editor))
            .with_child(cell(1., &row.output_editor))
            .with_child(row_controls)
            .finish();

        let mut col = Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_child(header_row);

        if is_expanded {
            col = col.with_child(Self::render_model_detail_panel(
                provider,
                index,
                model,
                row,
                draft_editors,
                appearance,
            ));
        }

        Container::new(col.finish())
            .with_margin_bottom(MODEL_ROW_GAP)
            .finish()
    }

    /// The expanded detail panel for a single model:
    /// - Modalities: tri-state chips for image / pdf / audio (Auto / On / Off)
    /// - Capabilities: two bool chips, reasoning / tool_call
    /// - A Remove button at the bottom
    fn render_model_detail_panel(
        provider: &AgentProvider,
        index: usize,
        model: &AgentProviderModel,
        row: &ModelRow,
        draft_editors: ProviderDraftEditors,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let label_color = theme.active_ui_text_color();

        // ---- Modalities section ----
        let modalities_label = Container::new(
            Text::new(
                "Modalities".to_string(),
                appearance.ui_font_family(),
                appearance.ui_font_size(),
            )
            .with_color(label_color.into())
            .finish(),
        )
        .with_margin_top(FIELD_LABEL_MARGIN_TOP)
        .with_margin_bottom(FIELD_LABEL_MARGIN_BOTTOM)
        .finish();

        let modality_chip = |label: &str,
                             slot: Option<bool>,
                             state: MouseStateHandle,
                             kind: ModelCapabilityKind|
         -> Box<dyn Element> {
            // Tri-state visuals: Auto = bare label / On = `● label` / Off = `○ label`.
            // Follows the existing `● {label}` selected style of the ApiType / ReasoningEffort
            // chips; Off uses a hollow circle ○ to contrast with the filled ●, and Auto has no
            // prefix (matching the unselected state).
            let chip_label = match slot {
                None => label.to_string(),
                Some(true) => format!("● {label}"),
                Some(false) => format!("○ {label}"),
            };
            Self::render_card_button_preserving_draft(
                chip_label,
                state,
                draft_editors.clone(),
                AISettingsPageAction::CycleAgentProviderModelCapability {
                    provider_id: provider.id.clone(),
                    model_index: index,
                    kind,
                },
                appearance,
            )
        };

        let modalities_row = Wrap::row()
            .with_spacing(6.)
            .with_run_spacing(4.)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(modality_chip(
                "Image",
                model.image,
                row.image_chip_state.clone(),
                ModelCapabilityKind::Image,
            ))
            .with_child(modality_chip(
                "PDF",
                model.pdf,
                row.pdf_chip_state.clone(),
                ModelCapabilityKind::Pdf,
            ))
            .with_child(modality_chip(
                "Audio",
                model.audio,
                row.audio_chip_state.clone(),
                ModelCapabilityKind::Audio,
            ))
            .finish();

        // ---- Capabilities section (reasoning / tool_call) ----
        let capabilities_label = Container::new(
            Text::new(
                "Capabilities".to_string(),
                appearance.ui_font_family(),
                appearance.ui_font_size(),
            )
            .with_color(label_color.into())
            .finish(),
        )
        .with_margin_top(FIELD_LABEL_MARGIN_TOP)
        .with_margin_bottom(FIELD_LABEL_MARGIN_BOTTOM)
        .finish();

        let bool_chip = |label: &str,
                         on: bool,
                         state: MouseStateHandle,
                         action: AISettingsPageAction|
         -> Box<dyn Element> {
            let chip_label = if on {
                format!("● {label}")
            } else {
                format!("○ {label}")
            };
            Self::render_card_button_preserving_draft(
                chip_label,
                state,
                draft_editors.clone(),
                action,
                appearance,
            )
        };

        let capabilities_row = Wrap::row()
            .with_spacing(6.)
            .with_run_spacing(4.)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(bool_chip(
                "Reasoning",
                model.reasoning,
                row.reasoning_chip_state.clone(),
                AISettingsPageAction::ToggleAgentProviderModelReasoning {
                    provider_id: provider.id.clone(),
                    model_index: index,
                },
            ))
            .with_child(bool_chip(
                "Tool Calling",
                model.tool_call,
                row.tool_call_chip_state.clone(),
                AISettingsPageAction::ToggleAgentProviderModelToolCall {
                    provider_id: provider.id.clone(),
                    model_index: index,
                },
            ))
            .finish();

        // ---- Remove button (only appears when expanded, to avoid accidental deletion while
        // collapsed) ----
        let remove_button = Self::render_card_button_preserving_draft(
            "Remove model",
            row.remove_button_state.clone(),
            draft_editors,
            AISettingsPageAction::RemoveAgentProviderModel {
                provider_id: provider.id.clone(),
                model_index: index,
            },
            appearance,
        );

        let remove_row = Container::new(
            Flex::row()
                .with_main_axis_alignment(MainAxisAlignment::End)
                .with_child(remove_button)
                .finish(),
        )
        .with_margin_top(FIELD_LABEL_MARGIN_TOP)
        .finish();

        // The overall detail panel uses a slight inset + border style to distinguish its
        // hierarchy level from the main row.
        Container::new(
            Flex::column()
                .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
                .with_child(modalities_label)
                .with_child(modalities_row)
                .with_child(capabilities_label)
                .with_child(capabilities_row)
                .with_child(remove_row)
                .finish(),
        )
        .with_margin_top(4.)
        .with_margin_left(12.)
        .with_margin_bottom(8.)
        .finish()
    }

    fn render_provider_card(
        &self,
        provider: &AgentProvider,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let is_any_ai_enabled = AISettings::as_ref(app).is_any_ai_enabled(app);
        let label_color = if is_any_ai_enabled && !provider.disabled {
            appearance.theme().active_ui_text_color()
        } else {
            appearance.theme().disabled_ui_text_color()
        };
        let detail_color = if is_any_ai_enabled && !provider.disabled {
            appearance.theme().foreground()
        } else {
            appearance.theme().disabled_ui_text_color()
        };

        let rows = self.rows.borrow();
        let row = match rows.get(&provider.id) {
            Some(row) => row,
            None => {
                return Container::new(
                    Text::new(
                        crate::t!(
                            "settings-agent-providers-row-missing",
                            id = provider.id.as_str()
                        ),
                        appearance.ui_font_family(),
                        appearance.ui_font_size(),
                    )
                    .with_color(detail_color.into())
                    .finish(),
                )
                .with_margin_bottom(8.)
                .finish();
            }
        };
        let draft_editors = ProviderDraftEditors::from_row(provider.id.clone(), row);

        let name_field = field_block(
            &crate::t!("settings-agent-providers-field-name"),
            ChildView::new(&row.name_editor).finish(),
            label_color,
            appearance,
        );
        let api_type_field = self.render_api_type_field(
            provider,
            row,
            draft_editors.clone(),
            label_color,
            appearance,
        );
        // Vertex is addressed by project + location rather than a raw base_url, and its "api key"
        // is an optional service-account email to impersonate (empty = gcloud ADC / active
        // account). Swap the endpoint + key fields accordingly.
        let is_vertex = provider.api_type.is_vertex();
        let endpoint_field = if is_vertex {
            Flex::column()
                .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
                .with_child(field_block(
                    &crate::t!("settings-agent-providers-field-vertex-project"),
                    ChildView::new(&row.vertex_project_editor).finish(),
                    label_color,
                    appearance,
                ))
                .with_child(field_block(
                    &crate::t!("settings-agent-providers-field-vertex-location"),
                    ChildView::new(&row.vertex_location_editor).finish(),
                    label_color,
                    appearance,
                ))
                .finish()
        } else {
            field_block(
                &crate::t!("settings-agent-providers-field-base-url"),
                ChildView::new(&row.base_url_editor).finish(),
                label_color,
                appearance,
            )
        };
        let api_key_label = if is_vertex {
            crate::t!("settings-agent-providers-field-vertex-service-account")
        } else {
            crate::t!("settings-agent-providers-field-api-key")
        };
        let api_key_field = field_block(
            &api_key_label,
            ChildView::new(&row.api_key_editor).finish(),
            label_color,
            appearance,
        );

        let headers_label = Container::new(
            Text::new(
                "Extra Headers".to_string(),
                appearance.ui_font_family(),
                appearance.ui_font_size(),
            )
            .with_color(label_color.into())
            .finish(),
        )
        .with_margin_top(FIELD_LABEL_MARGIN_TOP)
        .with_margin_bottom(FIELD_LABEL_MARGIN_BOTTOM)
        .finish();
        let mut headers_column = Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_child(headers_label);

        for (idx, h_row) in row.header_rows.iter().enumerate() {
            let remove_header_button = Self::render_card_button_preserving_draft(
                "×",
                h_row.remove_button_state.clone(),
                draft_editors.clone(),
                AISettingsPageAction::RemoveAgentProviderHeader {
                    provider_id: provider.id.clone(),
                    header_index: idx,
                },
                appearance,
            );
            let header_row = Flex::row()
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_child(
                    Expanded::new(
                        1.,
                        Container::new(ChildView::new(&h_row.key_editor).finish())
                            .with_margin_right(MODEL_ROW_GAP)
                            .finish(),
                    )
                    .finish(),
                )
                .with_child(
                    Expanded::new(
                        1.,
                        Container::new(ChildView::new(&h_row.val_editor).finish())
                            .with_margin_right(MODEL_ROW_GAP)
                            .finish(),
                    )
                    .finish(),
                )
                .with_child(remove_header_button)
                .finish();
            headers_column.add_child(
                Container::new(header_row)
                    .with_margin_bottom(MODEL_ROW_GAP)
                    .finish(),
            );
        }

        let add_header_button = Self::render_card_button_preserving_draft(
            "+ Add Header",
            row.add_header_button_state.clone(),
            draft_editors.clone(),
            AISettingsPageAction::AddAgentProviderHeader {
                provider_id: provider.id.clone(),
            },
            appearance,
        );
        headers_column.add_child(add_header_button);

        // ---- Model list section ----
        let models_label = Container::new(
            Text::new(
                crate::t!(
                    "settings-agent-providers-models-label",
                    count = provider.models.len()
                ),
                appearance.ui_font_family(),
                appearance.ui_font_size(),
            )
            .with_color(label_color.into())
            .finish(),
        )
        .with_margin_top(FIELD_LABEL_MARGIN_TOP)
        .with_margin_bottom(FIELD_LABEL_MARGIN_BOTTOM)
        .finish();

        let mut models_column = Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_child(models_label);

        if provider.models.is_empty() {
            let empty_hint = Container::new(
                Text::new(
                    crate::t!("settings-agent-providers-models-empty-hint"),
                    appearance.ui_font_family(),
                    appearance.ui_font_size(),
                )
                .with_color(appearance.theme().disabled_ui_text_color().into())
                .soft_wrap(true)
                .finish(),
            )
            .with_margin_bottom(MODEL_ROW_GAP)
            .finish();
            models_column.add_child(empty_hint);
        } else {
            // Header row: display name | model ID | context | output
            let dim = appearance.theme().disabled_ui_text_color();
            let header_cell = |flex: f32, label: &str| -> Box<dyn Element> {
                Expanded::new(
                    flex,
                    Container::new(
                        Text::new(
                            label.to_string(),
                            appearance.ui_font_family(),
                            appearance.ui_font_size(),
                        )
                        .with_color(dim.into())
                        .finish(),
                    )
                    .with_margin_right(MODEL_ROW_GAP)
                    .finish(),
                )
                .finish()
            };
            let header = Container::new(
                Flex::row()
                    .with_cross_axis_alignment(CrossAxisAlignment::Center)
                    .with_child(header_cell(
                        2.,
                        &crate::t!("settings-agent-providers-models-header-name"),
                    ))
                    .with_child(header_cell(
                        2.,
                        &crate::t!("settings-agent-providers-models-header-id"),
                    ))
                    .with_child(header_cell(
                        1.,
                        &crate::t!("settings-agent-providers-models-header-context"),
                    ))
                    .with_child(header_cell(
                        1.,
                        &crate::t!("settings-agent-providers-models-header-output"),
                    ))
                    // Placeholder, aligned with the expand/delete buttons below.
                    .with_child(
                        Flex::row()
                            .with_cross_axis_alignment(CrossAxisAlignment::Center)
                            .with_child(
                                Container::new(
                                    Text::new(
                                        "  ".to_string(),
                                        appearance.ui_font_family(),
                                        appearance.ui_font_size(),
                                    )
                                    .with_color(dim.into())
                                    .finish(),
                                )
                                .with_margin_right(MODEL_ROW_GAP)
                                .finish(),
                            )
                            .with_child(
                                Text::new(
                                    "  ".to_string(),
                                    appearance.ui_font_family(),
                                    appearance.ui_font_size(),
                                )
                                .with_color(dim.into())
                                .finish(),
                            )
                            .finish(),
                    )
                    .finish(),
            )
            .with_margin_bottom(2.)
            .finish();
            models_column.add_child(header);

            for (idx, m_row) in row.model_rows.iter().enumerate() {
                let model = match provider.models.get(idx) {
                    Some(m) => m,
                    // Edge case: settings were changed again during a rebuild gap, so
                    // model_rows and provider.models temporarily differ in length; skip to
                    // avoid a panic -- this self-corrects on the next frame.
                    None => continue,
                };
                models_column.add_child(Self::render_model_row(
                    provider,
                    idx,
                    model,
                    m_row,
                    draft_editors.clone(),
                    appearance,
                ));
            }
        }

        // ---- Bottom button row ----
        let add_model_button = Self::render_card_button_preserving_draft(
            crate::t!("settings-agent-providers-add-model"),
            row.add_model_button_state.clone(),
            draft_editors.clone(),
            AISettingsPageAction::AddAgentProviderModel {
                provider_id: provider.id.clone(),
            },
            appearance,
        );
        let fetch_button = Self::render_card_button_preserving_draft(
            crate::t!("settings-agent-providers-fetch-from-api"),
            row.fetch_button_state.clone(),
            draft_editors.clone(),
            AISettingsPageAction::FetchAgentProviderModels {
                provider_id: provider.id.clone(),
            },
            appearance,
        );
        let sync_models_dev_button = Self::render_card_button_preserving_draft(
            crate::t!("settings-agent-providers-sync-models-dev"),
            row.sync_models_dev_button_state.clone(),
            draft_editors.clone(),
            AISettingsPageAction::SyncProviderModelsFromModelsDev {
                provider_id: provider.id.clone(),
            },
            appearance,
        );
        let remove_button = Self::render_card_button(
            crate::t!("settings-agent-providers-remove"),
            row.remove_button_state.clone(),
            AISettingsPageAction::RemoveAgentProvider {
                provider_id: provider.id.clone(),
            },
            appearance,
        );
        // Hides the provider's models from the picker without touching its saved config or
        // API key -- the reversible alternative to Remove.
        let disable_toggle_label = if provider.disabled {
            crate::t!("settings-agent-providers-enable")
        } else {
            crate::t!("settings-agent-providers-disable")
        };
        let disable_toggle_button = Self::render_card_button(
            disable_toggle_label,
            row.disable_toggle_button_state.clone(),
            AISettingsPageAction::ToggleAgentProviderDisabled {
                provider_id: provider.id.clone(),
            },
            appearance,
        );

        // ---- Save button: reads all form buffers live inside the on_click closure.
        // The action can't be pre-built here (form values change with input), so the draft
        // editor handles travel with the closure, and SaveAgentProviderEdits is dispatched
        // together on click.
        let save_button = {
            let draft_editors = draft_editors.clone();

            appearance
                .ui_builder()
                .button(ButtonVariant::Accent, row.save_button_state.clone())
                .with_style(UiComponentStyles {
                    font_size: Some(appearance.ui_font_body()),
                    padding: Some(Coords::uniform(CARD_BUTTON_PADDING)),
                    ..Default::default()
                })
                .with_centered_text_label(crate::t!("settings-agent-providers-save"))
                .build()
                .on_click(move |ctx, app, _| {
                    ctx.dispatch_typed_action(draft_editors.to_save_action(app));
                })
                .finish()
        };

        let bottom_row = Flex::row()
            .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(
                Flex::row()
                    .with_cross_axis_alignment(CrossAxisAlignment::Center)
                    .with_child(
                        Container::new(add_model_button)
                            .with_margin_right(8.)
                            .finish(),
                    )
                    .with_child(Container::new(fetch_button).with_margin_right(8.).finish())
                    .with_child(sync_models_dev_button)
                    .finish(),
            )
            .with_child(
                Container::new(
                    Flex::row()
                        .with_cross_axis_alignment(CrossAxisAlignment::Center)
                        .with_child(Container::new(save_button).with_margin_right(8.).finish())
                        .with_child(
                            Container::new(disable_toggle_button)
                                .with_margin_right(8.)
                                .finish(),
                        )
                        .with_child(remove_button)
                        .finish(),
                )
                // Adds noticeable spacing from the primary action group on the left (add model /
                // fetch / sync), preventing the two groups from sticking together when
                // SpaceBetween runs out of card width.
                .with_margin_left(16.)
                .finish(),
            )
            .finish();

        // Uses the transparent detail_color to trigger it being read (avoiding an unused
        // warning); only relevant for potential coloring.
        let _ = detail_color;

        let mut card = Flex::column().with_cross_axis_alignment(CrossAxisAlignment::Stretch);
        if provider.disabled {
            card.add_child(
                Container::new(render_customer_type_badge(
                    appearance,
                    crate::t!("settings-agent-providers-disabled-badge").into(),
                ))
                .with_margin_bottom(6.)
                .finish(),
            );
        }

        Container::new(
            card
                .with_child(name_field)
                .with_child(api_type_field)
                .with_child(endpoint_field)
                .with_child(api_key_field)
                .with_child(
                    Container::new(headers_column.finish())
                        .with_margin_top(8.)
                        .finish(),
                )
                .with_child(
                    Container::new(models_column.finish())
                        .with_margin_top(8.)
                        .finish(),
                )
                .with_child(Container::new(bottom_row).with_margin_top(10.).finish())
                .finish(),
        )
        .with_background(appearance.theme().surface_1())
        .with_uniform_padding(12.)
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(6.)))
        .with_margin_bottom(8.)
        .finish()
    }
}

/// Parses user input into a token count. Tolerates `128k` / `128K` / `128 000` / `128,000` /
/// whitespace; on parse failure always returns 0 (meaning: unspecified).
fn parse_token_count(input: &str) -> u32 {
    let cleaned: String = input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',' && *c != '_')
        .collect();
    if cleaned.is_empty() {
        return 0;
    }
    let lower = cleaned.to_lowercase();
    let (num_part, multiplier): (&str, u64) = if let Some(stripped) = lower.strip_suffix('k') {
        (stripped, 1_000)
    } else if let Some(stripped) = lower.strip_suffix('m') {
        (stripped, 1_000_000)
    } else {
        (lower.as_str(), 1)
    };
    num_part
        .parse::<f64>()
        .ok()
        .map(|n| (n * multiplier as f64).round() as u64)
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(0)
}

/// Collapses the editor's selection to the end on blur.
///
/// Each input box is an independent `EditorView`, each maintaining its own selection range.
/// Selection-highlight rendering isn't affected by focus state (see
/// `app/src/editor/view/element.rs:1091`), so after a double-/triple-click or drag-select
/// followed by blur, the old selection stays in the buffer and is shown simultaneously with
/// other editors' selections, which looks like "multiple select states". Here, on Blurred, both
/// head/tail are collapsed to the end, visually releasing the selection.
fn collapse_selection_if_blurred(
    editor: &ViewHandle<EditorView>,
    event: &EditorEvent,
    ctx: &mut ViewContext<AISettingsPageView>,
) {
    if matches!(event, EditorEvent::Blurred) {
        editor.update(ctx, |editor, ctx| editor.move_to_buffer_end(ctx));
    }
}

fn single_line_editor_options(
    appearance: &Appearance,
    is_password: bool,
) -> SingleLineEditorOptions {
    SingleLineEditorOptions {
        is_password,
        clear_selections_on_blur: true,
        text: TextOptions {
            font_size_override: Some(appearance.ui_font_size()),
            font_family_override: Some(appearance.monospace_font_family()),
            text_colors_override: Some(TextColors {
                default_color: appearance.theme().active_ui_text_color(),
                disabled_color: appearance.theme().disabled_ui_text_color(),
                hint_color: appearance.theme().disabled_ui_text_color(),
            }),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn field_block(
    label: &str,
    editor_element: Box<dyn Element>,
    label_color: warp_core::ui::theme::Fill,
    appearance: &Appearance,
) -> Box<dyn Element> {
    let label_text = Container::new(
        Text::new(
            label.to_string(),
            appearance.ui_font_family(),
            appearance.ui_font_size(),
        )
        .with_color(label_color.into())
        .finish(),
    )
    .with_margin_top(FIELD_LABEL_MARGIN_TOP)
    .with_margin_bottom(FIELD_LABEL_MARGIN_BOTTOM)
    .finish();

    Flex::column()
        .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
        .with_child(label_text)
        .with_child(editor_element)
        .finish()
}

impl AgentProvidersWidget {
    /// Renders the "quick-add known providers from models.dev" section:
    /// - Title + "Refresh catalog" button
    /// - A row of chips (one per catalog provider id); clicking one creates a new local
    ///   provider with pre-filled models
    /// - Shows "Fetching..." while the catalog hasn't loaded yet
    fn render_models_dev_section(
        &self,
        appearance: &Appearance,
        _app: &AppContext,
    ) -> Box<dyn Element> {
        use crate::ai::agent_providers::models_dev;

        let label_color = appearance.theme().active_ui_text_color();
        let dim_color = appearance.theme().disabled_ui_text_color();

        let title = Text::new(
            crate::t!("settings-agent-providers-quick-add-title"),
            appearance.ui_font_family(),
            appearance.ui_font_size(),
        )
        .with_color(label_color.into())
        .finish();

        let refresh_button = Self::render_card_button(
            crate::t!("settings-agent-providers-refresh-catalog"),
            self.refresh_catalog_button_state.clone(),
            AISettingsPageAction::RefreshModelsDev,
            appearance,
        );

        let search_box = Container::new(ChildView::new(&self.search_editor).finish())
            .with_margin_left(8.)
            .with_margin_right(8.)
            .finish();

        let header_row = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(title)
            .with_child(Expanded::new(1., search_box).finish())
            .with_child(refresh_button)
            .finish();

        let mut body = Flex::column().with_cross_axis_alignment(CrossAxisAlignment::Stretch);
        body.add_child(header_row);

        // Shows the first N when collapsed (enough to fill about one row -- actual wrapping is
        // handled by the Wrap layout).
        const COLLAPSED_LIMIT: usize = 8;
        let expanded = models_dev::chips_expanded();

        match models_dev::cached() {
            None => {
                let catalog_text = if models_dev::last_fetch_failed() {
                    crate::t!("settings-agent-providers-catalog-load-failed")
                } else {
                    crate::t!("settings-agent-providers-loading-catalog")
                };
                body.add_child(
                    Container::new(
                        Text::new(
                            catalog_text,
                            appearance.ui_font_family(),
                            appearance.ui_font_size(),
                        )
                        .with_color(dim_color.into())
                        .finish(),
                    )
                    .with_margin_top(4.)
                    .finish(),
                );
            }
            Some(catalog) if catalog.is_empty() => {
                body.add_child(
                    Container::new(
                        Text::new(
                            crate::t!("settings-agent-providers-catalog-empty"),
                            appearance.ui_font_family(),
                            appearance.ui_font_size(),
                        )
                        .with_color(dim_color.into())
                        .finish(),
                    )
                    .with_margin_top(4.)
                    .finish(),
                );
            }
            Some(catalog) => {
                // Merge the Vertex-family entries into one "Google Vertex AI" chip before
                // filtering, so the row never shows two easily-confused vertex options.
                let catalog = models_dev::quick_add_catalog(&catalog);
                // Filters by the search query; an empty query -> all entries in order.
                let query = models_dev::search_query();
                let filtered = models_dev::filter_catalog(&catalog, &query);
                let total = filtered.len();
                let has_query = !query.trim().is_empty();
                // When search is active, always expand all matches without collapsing
                // (otherwise a result count <= the collapse limit wouldn't show them all).
                let visible_count = if expanded || has_query {
                    total
                } else {
                    COLLAPSED_LIMIT.min(total)
                };

                let mut wrap = Wrap::row()
                    .with_spacing(6.)
                    .with_run_spacing(6.)
                    .with_cross_axis_alignment(CrossAxisAlignment::Center);
                {
                    let mut states = self.quick_add_button_states.borrow_mut();
                    for (cat_id, cat_provider) in filtered.iter().take(visible_count) {
                        let label = if cat_provider.name.is_empty() {
                            cat_id.clone()
                        } else {
                            cat_provider.name.clone()
                        };
                        let state = states.entry(cat_id.clone()).or_default().clone();
                        let model_count = cat_provider.models.len();
                        let display_label = format!("+ {label} ({model_count})");
                        let chip = Self::render_card_button(
                            display_label,
                            state,
                            AISettingsPageAction::AddProviderFromModelsDev {
                                catalog_provider_id: cat_id.clone(),
                            },
                            appearance,
                        );
                        wrap = wrap.with_child(chip);
                    }
                }
                body.add_child(Container::new(wrap.finish()).with_margin_top(4.).finish());

                if has_query && total == 0 {
                    body.add_child(
                        Container::new(
                            Text::new(
                                crate::t!(
                                    "settings-agent-providers-no-match",
                                    query = query.as_str()
                                ),
                                appearance.ui_font_family(),
                                appearance.ui_font_size(),
                            )
                            .with_color(dim_color.into())
                            .finish(),
                        )
                        .with_margin_top(4.)
                        .finish(),
                    );
                }

                // Expand/collapse button (only shown when there's no search and the catalog
                // exceeds the collapse limit).
                if !has_query && total > COLLAPSED_LIMIT {
                    let toggle_label = if expanded {
                        crate::t!("settings-agent-providers-collapse")
                    } else {
                        let count: i64 = (total - COLLAPSED_LIMIT) as i64;
                        crate::t!("settings-agent-providers-expand-remaining", count = count)
                    };
                    let toggle_button = Self::render_card_button(
                        toggle_label,
                        self.expand_chips_button_state.clone(),
                        AISettingsPageAction::ToggleModelsDevChipsExpanded,
                        appearance,
                    );
                    body.add_child(
                        Container::new(
                            Flex::row()
                                .with_main_axis_alignment(MainAxisAlignment::Start)
                                .with_child(toggle_button)
                                .finish(),
                        )
                        .with_margin_top(6.)
                        .finish(),
                    );
                }
            }
        }

        Container::new(body.finish())
            .with_background(appearance.theme().surface_1())
            .with_uniform_padding(10.)
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(6.)))
            .with_margin_bottom(10.)
            .finish()
    }
}

impl SettingsWidget for AgentProvidersWidget {
    type View = AISettingsPageView;

    fn search_terms(&self) -> &str {
        "agent provider providers custom openai compatible deepseek glm moonshot dashscope qwen ollama base url api key models save 提供商 自定义 模型 保存"
    }

    fn render(
        &self,
        _view: &Self::View,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let is_any_ai_enabled = AISettings::as_ref(app).is_any_ai_enabled(app);
        let providers = AISettings::as_ref(app).agent_providers.value().clone();

        let title_node = build_sub_header(
            appearance,
            crate::t!("settings-agent-providers-title"),
            Some(if is_any_ai_enabled {
                appearance.theme().active_ui_text_color()
            } else {
                appearance.theme().disabled_ui_text_color()
            }),
        )
        .finish();

        let header_add_button = Self::render_card_button(
            crate::t!("settings-agent-providers-add-button"),
            self.add_button_state.clone(),
            AISettingsPageAction::AddAgentProvider,
            appearance,
        );

        let header = Container::new(
            Flex::row()
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_child(Expanded::new(1., title_node).finish())
                .with_child(header_add_button)
                .finish(),
        )
        .with_padding_bottom(HEADER_PADDING)
        .finish();

        let description_text = crate::t!("settings-agent-providers-description");
        let description = Container::new(
            Text::new(
                description_text,
                appearance.ui_font_family(),
                appearance.ui_font_size(),
            )
            .with_color(if is_any_ai_enabled {
                appearance.theme().foreground().into()
            } else {
                appearance.theme().disabled_ui_text_color().into()
            })
            .soft_wrap(true)
            .finish(),
        )
        .with_margin_bottom(12.)
        .finish();

        let mut column = Flex::column().with_child(header).with_child(description);

        // ---- Quick-add chip row from models.dev ----
        column.add_child(self.render_models_dev_section(appearance, app));

        if providers.is_empty() {
            let empty = Container::new(
                Text::new(
                    crate::t!("settings-agent-providers-empty"),
                    appearance.ui_font_family(),
                    appearance.ui_font_size(),
                )
                .with_color(appearance.theme().disabled_ui_text_color().into())
                .finish(),
            )
            .with_margin_bottom(12.)
            .finish();
            column.add_child(empty);
        } else {
            let (disabled_providers, active_providers): (Vec<_>, Vec<_>) =
                providers.iter().partition(|p| p.disabled);

            for provider in &active_providers {
                column.add_child(self.render_provider_card(provider, appearance, app));
            }

            // Disabled providers live in their own collapsed-by-default section so a list with
            // several turned off doesn't bury the active ones.
            if !disabled_providers.is_empty() {
                let section_expanded = disabled_section_expanded();
                let toggle_label = if section_expanded {
                    crate::t!(
                        "settings-agent-providers-disabled-section-collapse",
                        count = disabled_providers.len()
                    )
                } else {
                    crate::t!(
                        "settings-agent-providers-disabled-section-expand",
                        count = disabled_providers.len()
                    )
                };
                let toggle_button = Self::render_card_button(
                    toggle_label,
                    self.disabled_section_toggle_button_state.clone(),
                    AISettingsPageAction::ToggleDisabledProvidersExpanded,
                    appearance,
                );
                column.add_child(
                    Container::new(
                        Flex::row()
                            .with_main_axis_alignment(MainAxisAlignment::Start)
                            .with_child(toggle_button)
                            .finish(),
                    )
                    .with_margin_top(if active_providers.is_empty() { 0. } else { 4. })
                    .with_margin_bottom(8.)
                    .finish(),
                );
                if section_expanded {
                    for provider in &disabled_providers {
                        column.add_child(self.render_provider_card(provider, appearance, app));
                    }
                }
            }
        }

        Container::new(column.finish())
            .with_margin_bottom(HEADER_PADDING)
            .finish()
    }
}
