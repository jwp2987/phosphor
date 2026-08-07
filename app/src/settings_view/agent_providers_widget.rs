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

use super::ai_page::{
    AISettingsPageAction, AISettingsPageView, ModelCapabilityKind, ModelEditFields,
    ProviderEditFields,
};
use super::settings_page::{
    build_sub_header, render_customer_type_badge, SettingsWidget, HEADER_PADDING,
};

const CARD_BUTTON_PADDING: f32 = 6.0;
const FIELD_LABEL_MARGIN_TOP: f32 = 6.0;
const FIELD_LABEL_MARGIN_BOTTOM: f32 = 2.0;
const MODEL_ROW_GAP: f32 = 6.0;
/// Below this many configured models, the search box + "Disable/Enable shown" bulk actions
/// stay hidden -- most providers have a handful of models and don't need curating down.
const MODEL_SEARCH_THRESHOLD: usize = 8;

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

/// Forces the "Disabled providers" section open. Used when adding a new empty provider,
/// since it lands in that section (no models yet) and would otherwise be invisible right
/// after clicking "+ Add provider".
pub(super) fn set_disabled_section_expanded(expanded: bool) {
    DISABLED_SECTION_EXPANDED.store(expanded, std::sync::atomic::Ordering::Relaxed);
}

/// Whether the "Hidden providers" section (models.dev catalog entries the user chose not to
/// see in the quick-add row) is expanded. Collapsed by default, same rationale as
/// `DISABLED_SECTION_EXPANDED`.
static HIDDEN_CATALOG_SECTION_EXPANDED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub(super) fn hidden_catalog_section_expanded() -> bool {
    HIDDEN_CATALOG_SECTION_EXPANDED.load(std::sync::atomic::Ordering::Relaxed)
}

pub(super) fn toggle_hidden_catalog_section_expanded() {
    HIDDEN_CATALOG_SECTION_EXPANDED.fetch_xor(true, std::sync::atomic::Ordering::Relaxed);
}

// ---------------------------------------------------------------------------
// gcloud login status (process-local, not persisted -- same treatment as the other ephemeral
// UI state above). Account-level rather than per-provider: any Vertex provider's card can
// trigger it, and the resulting status applies regardless of which card triggered it.
// ---------------------------------------------------------------------------

std::thread_local! {
    static GCLOUD_LOGIN_STATUS: RefCell<Option<String>> = const { RefCell::new(None) };
}

pub(super) fn gcloud_login_status() -> Option<String> {
    GCLOUD_LOGIN_STATUS.with(|s| s.borrow().clone())
}

pub(super) fn set_gcloud_login_status(status: Option<String>) {
    GCLOUD_LOGIN_STATUS.with(|s| *s.borrow_mut() = status);
}

// ---------------------------------------------------------------------------
// Per-provider model list search + disabled-models section state (process-local, not
// persisted -- same treatment as the model-row expansion state above)
// ---------------------------------------------------------------------------

std::thread_local! {
    /// {provider_id => current search text} for a provider's model list. Lets a provider
    /// with a large catalog (some have 200-300 models) be narrowed down instead of scrolling
    /// through every row, and scopes the "Disable/Enable shown" bulk actions.
    static MODEL_SEARCH_QUERIES: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
    /// {provider_id} whose "Disabled models" subsection is currently expanded. Collapsed by
    /// default, same rationale as `DISABLED_SECTION_EXPANDED`.
    static DISABLED_MODELS_EXPANDED: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

pub(super) fn model_search_query(provider_id: &str) -> String {
    MODEL_SEARCH_QUERIES.with(|m| m.borrow().get(provider_id).cloned().unwrap_or_default())
}

pub(super) fn set_model_search_query(provider_id: &str, query: String) {
    MODEL_SEARCH_QUERIES.with(|m| {
        let mut map = m.borrow_mut();
        if query.trim().is_empty() {
            map.remove(provider_id);
        } else {
            map.insert(provider_id.to_string(), query);
        }
    });
}

pub(super) fn disabled_models_expanded(provider_id: &str) -> bool {
    DISABLED_MODELS_EXPANDED.with(|s| s.borrow().contains(provider_id))
}

pub(super) fn toggle_disabled_models_expanded(provider_id: &str) {
    DISABLED_MODELS_EXPANDED.with(|s| {
        let mut set = s.borrow_mut();
        if !set.insert(provider_id.to_string()) {
            set.remove(provider_id);
        }
    });
}

/// Case-insensitive substring match against a model's name or id; an empty/blank query
/// matches everything. Shared by the model list's render-time filtering and the
/// "Disable/Enable shown" bulk actions, so what's visually shown and what gets bulk-toggled
/// can never disagree.
pub(super) fn model_matches_search(model: &AgentProviderModel, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    model.name.to_lowercase().contains(&query) || model.id.to_lowercase().contains(&query)
}

/// Clears the per-provider model search/expansion state when a provider is deleted, avoiding
/// stale entries piling up under an id that no longer exists.
pub(super) fn clear_model_search_state_for_provider(provider_id: &str) {
    MODEL_SEARCH_QUERIES.with(|m| {
        m.borrow_mut().remove(provider_id);
    });
    DISABLED_MODELS_EXPANDED.with(|s| {
        s.borrow_mut().remove(provider_id);
    });
}

/// The editable view handles for a single model entry (name + id + context + output + the two
/// `/cost` token rates).
struct ModelRow {
    name_editor: ViewHandle<EditorView>,
    id_editor: ViewHandle<EditorView>,
    context_editor: ViewHandle<EditorView>,
    output_editor: ViewHandle<EditorView>,
    /// USD per 1M input tokens, used by `/cost`. Empty = no rate configured, which `/cost`
    /// reports as such instead of billing at zero.
    input_price_editor: ViewHandle<EditorView>,
    /// USD per 1M output tokens, used by `/cost`.
    output_price_editor: ViewHandle<EditorView>,
    /// The remove button inside the detail panel.
    remove_button_state: MouseStateHandle,
    /// The quick-remove button to the right of the chevron at the end of the row.
    quick_remove_button_state: MouseStateHandle,
    /// The compact ●/○ enable/disable toggle, excludes this model from the picker without
    /// removing it from the list.
    disable_toggle_button_state: MouseStateHandle,
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
    gcloud_login_button_state: MouseStateHandle,
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
    /// Filters the models list below by name/id substring; also scopes the "Disable/Enable
    /// shown" bulk actions. Only meaningful once a provider has more than a couple of models.
    model_search_editor: ViewHandle<EditorView>,
    disable_shown_button_state: MouseStateHandle,
    enable_shown_button_state: MouseStateHandle,
    disabled_models_toggle_button_state: MouseStateHandle,
}

/// `(model_index, name, id, context_window, max_output_tokens, input_price, output_price)`.
type ModelDraftEditorHandles = (
    usize,
    ViewHandle<EditorView>,
    ViewHandle<EditorView>,
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
                        m.input_price_editor.clone(),
                        m.output_price_editor.clone(),
                    )
                })
                .collect(),
        }
    }

    fn to_save_action(&self, app: &AppContext) -> AISettingsPageAction {
        AISettingsPageAction::SaveAgentProviderEdits(self.collect_fields(app))
    }

    fn to_save_then_action(
        &self,
        app: &AppContext,
        action: AISettingsPageAction,
    ) -> AISettingsPageAction {
        AISettingsPageAction::SaveAgentProviderEditsThen(
            self.collect_fields(app),
            Box::new(action),
        )
    }

    /// Reads every draft editor's current buffer text into a [`ProviderEditFields`], shared by
    /// [`Self::to_save_action`] and [`Self::to_save_then_action`] so there's a single place that
    /// knows how to collect the form state.
    fn collect_fields(&self, app: &AppContext) -> ProviderEditFields {
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
        let models: Vec<ModelEditFields> = self
            .model_editors
            .iter()
            .map(|(idx, name_e, id_e, ctx_e, out_e, in_price_e, out_price_e)| {
                let m_name = name_e.as_ref(app).buffer_text(app);
                let m_id = id_e.as_ref(app).buffer_text(app);
                let context_window = parse_token_count(&ctx_e.as_ref(app).buffer_text(app));
                let max_output_tokens = parse_token_count(&out_e.as_ref(app).buffer_text(app));
                ModelEditFields {
                    model_index: *idx,
                    name: m_name,
                    id: m_id,
                    context_window,
                    max_output_tokens,
                    input_usd_per_million_tokens: parse_usd_rate(
                        &in_price_e.as_ref(app).buffer_text(app),
                    ),
                    output_usd_per_million_tokens: parse_usd_rate(
                        &out_price_e.as_ref(app).buffer_text(app),
                    ),
                }
            })
            .collect();

        ProviderEditFields {
            provider_id: self.provider_id.clone(),
            name,
            base_url,
            api_key,
            vertex_project,
            vertex_location,
            headers,
            models,
        }
    }
}

/// The custom Agent Provider settings widget.
pub(super) struct AgentProvidersWidget {
    add_button_state: MouseStateHandle,
    refresh_catalog_button_state: MouseStateHandle,
    expand_chips_button_state: MouseStateHandle,
    disabled_section_toggle_button_state: MouseStateHandle,
    hidden_catalog_section_toggle_button_state: MouseStateHandle,
    /// The search box for the quick-add chip row.
    search_editor: ViewHandle<EditorView>,
    /// One button state per catalog provider id -- used by the chip row.
    quick_add_button_states: RefCell<HashMap<String, MouseStateHandle>>,
    /// One hide/unhide button state per catalog provider id.
    quick_hide_button_states: RefCell<HashMap<String, MouseStateHandle>>,
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
            hidden_catalog_section_toggle_button_state: MouseStateHandle::default(),
            search_editor,
            quick_add_button_states: RefCell::new(HashMap::new()),
            quick_hide_button_states: RefCell::new(HashMap::new()),
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

        // ---- `/cost` token rates (USD per 1M tokens; empty = no rate configured) ----
        // Deliberately blank rather than pre-filled with a plausible default: `/cost` must be
        // able to tell "the user told me this model is free" from "nobody has told me what
        // this model costs", and only an empty field can mean the latter.
        let initial_input_price = model
            .token_price
            .map(|price| format_usd_rate(price.input_usd_per_million_tokens))
            .unwrap_or_default();
        let input_price_editor = ctx.add_typed_action_view(move |ctx| {
            let appearance = Appearance::handle(ctx).as_ref(ctx);
            let options = single_line_editor_options(appearance, false);
            let mut editor = EditorView::single_line(options, ctx);
            editor.set_placeholder_text(
                crate::t!("settings-agent-providers-model-input-price-placeholder"),
                ctx,
            );
            if !initial_input_price.is_empty() {
                editor.set_buffer_text(&initial_input_price, ctx);
            }
            editor
        });
        ctx.subscribe_to_view(&input_price_editor, move |_, editor, event, ctx| {
            collapse_selection_if_blurred(&editor, event, ctx);
        });

        let initial_output_price = model
            .token_price
            .map(|price| format_usd_rate(price.output_usd_per_million_tokens))
            .unwrap_or_default();
        let output_price_editor = ctx.add_typed_action_view(move |ctx| {
            let appearance = Appearance::handle(ctx).as_ref(ctx);
            let options = single_line_editor_options(appearance, false);
            let mut editor = EditorView::single_line(options, ctx);
            editor.set_placeholder_text(
                crate::t!("settings-agent-providers-model-output-price-placeholder"),
                ctx,
            );
            if !initial_output_price.is_empty() {
                editor.set_buffer_text(&initial_output_price, ctx);
            }
            editor
        });
        ctx.subscribe_to_view(&output_price_editor, move |_, editor, event, ctx| {
            collapse_selection_if_blurred(&editor, event, ctx);
        });

        ModelRow {
            name_editor,
            id_editor,
            context_editor,
            output_editor,
            input_price_editor,
            output_price_editor,
            remove_button_state: MouseStateHandle::default(),
            quick_remove_button_state: MouseStateHandle::default(),
            disable_toggle_button_state: MouseStateHandle::default(),
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

        // ---- Model list search box ----
        let initial_model_search = model_search_query(&provider_id);
        let model_search_editor = ctx.add_typed_action_view(move |ctx| {
            let appearance = Appearance::handle(ctx).as_ref(ctx);
            let options = single_line_editor_options(appearance, false);
            let mut editor = EditorView::single_line(options, ctx);
            editor.set_placeholder_text(
                crate::t!("settings-agent-providers-model-search-placeholder"),
                ctx,
            );
            if !initial_model_search.is_empty() {
                editor.set_buffer_text(&initial_model_search, ctx);
            }
            editor
        });
        {
            let provider_id = provider_id.clone();
            ctx.subscribe_to_view(&model_search_editor, move |_, editor, event, ctx| {
                if matches!(event, EditorEvent::Edited(_)) {
                    let buffer_text = editor.as_ref(ctx).buffer_text(ctx);
                    ctx.dispatch_typed_action_deferred(
                        AISettingsPageAction::SetAgentProviderModelSearchQuery {
                            provider_id: provider_id.clone(),
                            query: buffer_text,
                        },
                    );
                }
            });
        }

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
            gcloud_login_button_state: MouseStateHandle::default(),
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
            model_search_editor,
            disable_shown_button_state: MouseStateHandle::default(),
            enable_shown_button_state: MouseStateHandle::default(),
            disabled_models_toggle_button_state: MouseStateHandle::default(),
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
        // Compact enable/disable toggle: excludes this one model from the picker without
        // removing it from the list. Follows the same ●/○ filled/hollow convention as the
        // tri-state capability chips in the detail panel below.
        let disable_toggle_label = if model.disabled { "○" } else { "●" };
        let disable_toggle_button = Self::render_card_button_preserving_draft(
            disable_toggle_label,
            row.disable_toggle_button_state.clone(),
            draft_editors.clone(),
            AISettingsPageAction::ToggleAgentProviderModelDisabled {
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
            .with_child(
                Container::new(disable_toggle_button)
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
            .with_child(cell(1., &row.input_price_editor))
            .with_child(cell(1., &row.output_price_editor))
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
        let label_color = if is_any_ai_enabled && !provider.effectively_disabled() {
            appearance.theme().active_ui_text_color()
        } else {
            appearance.theme().disabled_ui_text_color()
        };
        let detail_color = if is_any_ai_enabled && !provider.effectively_disabled() {
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
        // Vertex's "api key" is a GCP OAuth2 bearer minted via `gcloud`, which requires an
        // active login and expires periodically (see `vertex_auth`). Surface a direct way to
        // (re)authenticate instead of making the user drop to a terminal.
        let gcloud_login_section = is_vertex.then(|| {
            let login_button = Self::render_card_button(
                crate::t!("settings-agent-providers-vertex-login"),
                row.gcloud_login_button_state.clone(),
                AISettingsPageAction::LaunchGcloudLogin,
                appearance,
            );
            let mut column = Flex::column()
                .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
                .with_child(login_button);
            if let Some(status) = gcloud_login_status() {
                column.add_child(
                    Container::new(
                        Text::new(status, appearance.ui_font_family(), appearance.ui_font_size())
                            .with_color(appearance.theme().disabled_ui_text_color().into())
                            .soft_wrap(true)
                            .finish(),
                    )
                    .with_margin_top(4.)
                    .finish(),
                );
            }
            Container::new(column.finish()).with_margin_top(6.).finish()
        });

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

        let show_model_search = provider.models.len() > MODEL_SEARCH_THRESHOLD;
        if show_model_search {
            models_column.add_child(
                Container::new(ChildView::new(&row.model_search_editor).finish())
                    .with_margin_bottom(6.)
                    .finish(),
            );
        }

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
                    .with_child(header_cell(
                        1.,
                        &crate::t!("settings-agent-providers-models-header-input-price"),
                    ))
                    .with_child(header_cell(
                        1.,
                        &crate::t!("settings-agent-providers-models-header-output-price"),
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

            // Indices (not a re-collected/re-indexed Vec) so RemoveAgentProviderModel /
            // ToggleAgentProviderModelExpanded / etc, which all address `provider.models` by
            // position, stay correct regardless of what's filtered out of view here.
            //
            // Only apply the stored query when the search box is actually shown: the query
            // persists in a process-lifetime thread_local keyed by provider id, so if the
            // model count drops back to/under the threshold (e.g. removing models one at a
            // time) the search box disappears -- if the stale query kept filtering with no
            // visible control left to clear it, models could vanish with no way to get them
            // back short of deleting and re-adding the whole provider.
            let search_query = if show_model_search {
                model_search_query(&provider.id)
            } else {
                String::new()
            };
            let matching_count = provider
                .models
                .iter()
                .filter(|m| model_matches_search(m, &search_query))
                .count();

            if show_model_search {
                let disable_shown_button = Self::render_card_button(
                    crate::t!(
                        "settings-agent-providers-disable-shown",
                        count = matching_count
                    ),
                    row.disable_shown_button_state.clone(),
                    AISettingsPageAction::BulkSetAgentProviderModelsDisabledForSearch {
                        provider_id: provider.id.clone(),
                        disabled: true,
                    },
                    appearance,
                );
                let enable_shown_button = Self::render_card_button(
                    crate::t!(
                        "settings-agent-providers-enable-shown",
                        count = matching_count
                    ),
                    row.enable_shown_button_state.clone(),
                    AISettingsPageAction::BulkSetAgentProviderModelsDisabledForSearch {
                        provider_id: provider.id.clone(),
                        disabled: false,
                    },
                    appearance,
                );
                models_column.add_child(
                    Container::new(
                        Flex::row()
                            .with_cross_axis_alignment(CrossAxisAlignment::Center)
                            .with_child(
                                Container::new(disable_shown_button)
                                    .with_margin_right(8.)
                                    .finish(),
                            )
                            .with_child(enable_shown_button)
                            .finish(),
                    )
                    .with_margin_bottom(6.)
                    .finish(),
                );
            }

            if matching_count == 0 {
                models_column.add_child(
                    Container::new(
                        Text::new(
                            crate::t!(
                                "settings-agent-providers-models-no-match",
                                query = search_query.as_str()
                            ),
                            appearance.ui_font_family(),
                            appearance.ui_font_size(),
                        )
                        .with_color(appearance.theme().disabled_ui_text_color().into())
                        .finish(),
                    )
                    .with_margin_bottom(MODEL_ROW_GAP)
                    .finish(),
                );
            } else {
                let mut disabled_matching = Vec::new();
                for (idx, m_row) in row.model_rows.iter().enumerate() {
                    let model = match provider.models.get(idx) {
                        Some(m) => m,
                        // Edge case: settings were changed again during a rebuild gap, so
                        // model_rows and provider.models temporarily differ in length; skip to
                        // avoid a panic -- this self-corrects on the next frame.
                        None => continue,
                    };
                    if !model_matches_search(model, &search_query) {
                        continue;
                    }
                    if model.disabled {
                        disabled_matching.push((idx, model, m_row));
                        continue;
                    }
                    models_column.add_child(Self::render_model_row(
                        provider,
                        idx,
                        model,
                        m_row,
                        draft_editors.clone(),
                        appearance,
                    ));
                }

                // Disabled models collapse into their own subsection, same rationale and
                // pattern as the top-level "Disabled providers" section: a curated-down
                // 200-300-model provider shouldn't still render every excluded row.
                if !disabled_matching.is_empty() {
                    let expanded = disabled_models_expanded(&provider.id);
                    let toggle_label = if expanded {
                        crate::t!(
                            "settings-agent-providers-disabled-models-collapse",
                            count = disabled_matching.len()
                        )
                    } else {
                        crate::t!(
                            "settings-agent-providers-disabled-models-expand",
                            count = disabled_matching.len()
                        )
                    };
                    let toggle_button = Self::render_card_button(
                        toggle_label,
                        row.disabled_models_toggle_button_state.clone(),
                        AISettingsPageAction::ToggleAgentProviderDisabledModelsExpanded {
                            provider_id: provider.id.clone(),
                        },
                        appearance,
                    );
                    models_column.add_child(
                        Container::new(
                            Flex::row()
                                .with_main_axis_alignment(MainAxisAlignment::Start)
                                .with_child(toggle_button)
                                .finish(),
                        )
                        .with_margin_top(4.)
                        .with_margin_bottom(4.)
                        .finish(),
                    );
                    if expanded {
                        for (idx, model, m_row) in disabled_matching {
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
                }
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
        // API key -- the reversible alternative to Remove. Always reflects the raw explicit
        // flag (not effectively_disabled()): even an empty, auto-hidden provider can be
        // explicitly disabled too, so it stays hidden after models are later added.
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
        if provider.effectively_disabled() {
            card.add_child(
                Container::new(render_customer_type_badge(
                    appearance,
                    crate::t!("settings-agent-providers-disabled-badge").into(),
                ))
                .with_margin_bottom(6.)
                .finish(),
            );
        }

        card.add_child(name_field);
        card.add_child(api_type_field);
        card.add_child(endpoint_field);
        card.add_child(api_key_field);
        if let Some(gcloud_login_section) = gcloud_login_section {
            card.add_child(gcloud_login_section);
        }

        Container::new(
            card
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

/// Parses a `/cost` token rate in USD per 1M tokens. Tolerates a leading `$`, thousands
/// separators and whitespace.
///
/// `None` means "no rate entered", which is a distinct answer from `Some(0.0)`: the former
/// makes `/cost` report token counts and say no rate is configured, the latter makes it report
/// `$0.0000` because the user said this endpoint is free. Unparseable and negative input is
/// treated as not-entered rather than silently coerced to a number.
fn parse_usd_rate(input: &str) -> Option<f64> {
    let cleaned: String = input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',' && *c != '_' && *c != '$')
        .collect();
    if cleaned.is_empty() {
        return None;
    }
    cleaned
        .parse::<f64>()
        .ok()
        .filter(|rate| rate.is_finite() && *rate >= 0.0)
}

/// Renders a stored rate back into the editor, trimming the trailing zeros `f64` printing adds
/// so a saved `3.0` reads as the `3` the user typed.
fn format_usd_rate(rate: f64) -> String {
    let mut text = format!("{rate:.4}");
    while text.contains('.') && (text.ends_with('0') || text.ends_with('.')) {
        text.pop();
    }
    text
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
        app: &AppContext,
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
                let has_query = !query.trim().is_empty();

                // Only a handful of "common" providers (the ones this app has a native
                // adapter for) show in the main row by default; everything else starts in
                // the collapsed "Hidden providers" section below, still reachable by
                // searching for it or one of its models. `overrides` flips that default in
                // either direction per id -- see `models_dev::effectively_visible`.
                let overrides: HashSet<String> = AISettings::as_ref(app)
                    .catalog_provider_visibility_overrides
                    .value()
                    .iter()
                    .cloned()
                    .collect();
                let (visible_matching, hidden_matching): (Vec<_>, Vec<_>) = filtered
                    .into_iter()
                    .partition(|(cat_id, _)| models_dev::effectively_visible(cat_id, &overrides));

                let total = visible_matching.len();
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
                    let mut add_states = self.quick_add_button_states.borrow_mut();
                    let mut hide_states = self.quick_hide_button_states.borrow_mut();
                    for (cat_id, cat_provider) in visible_matching.iter().take(visible_count) {
                        let label = if cat_provider.name.is_empty() {
                            cat_id.clone()
                        } else {
                            cat_provider.name.clone()
                        };
                        let add_state = add_states.entry(cat_id.clone()).or_default().clone();
                        let model_count = cat_provider.models.len();
                        let display_label = format!("+ {label} ({model_count})");
                        let chip = Self::render_card_button(
                            display_label,
                            add_state,
                            AISettingsPageAction::AddProviderFromModelsDev {
                                catalog_provider_id: cat_id.clone(),
                            },
                            appearance,
                        );
                        let hide_state = hide_states.entry(cat_id.clone()).or_default().clone();
                        let hide_button = Self::render_card_button(
                            "×",
                            hide_state,
                            AISettingsPageAction::ToggleCatalogProviderVisibilityOverride {
                                catalog_provider_id: cat_id.clone(),
                            },
                            appearance,
                        );
                        wrap = wrap.with_child(
                            Flex::row()
                                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                                .with_child(chip)
                                .with_child(Container::new(hide_button).with_margin_left(2.).finish())
                                .finish(),
                        );
                    }
                }
                body.add_child(Container::new(wrap.finish()).with_margin_top(4.).finish());

                if has_query && total == 0 && hidden_matching.is_empty() {
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

                // Hidden providers live in their own collapsed-by-default section, mirroring
                // the "Disabled providers" section for configured providers -- same
                // decluttering rationale, just for catalog entries never added at all.
                if !hidden_matching.is_empty() {
                    // While actively searching, auto-expand so a match hidden by default
                    // (e.g. an uncommon provider found by name or by one of its models)
                    // isn't mistaken for "no results" behind an unopened toggle -- mirrors
                    // the main chip row's `expanded || has_query` treatment above.
                    let hidden_section_expanded = has_query || hidden_catalog_section_expanded();
                    let hidden_toggle_label = if hidden_section_expanded {
                        crate::t!(
                            "settings-agent-providers-hidden-catalog-collapse",
                            count = hidden_matching.len()
                        )
                    } else {
                        crate::t!(
                            "settings-agent-providers-hidden-catalog-expand",
                            count = hidden_matching.len()
                        )
                    };
                    let hidden_toggle_button = Self::render_card_button(
                        hidden_toggle_label,
                        self.hidden_catalog_section_toggle_button_state.clone(),
                        AISettingsPageAction::ToggleHiddenCatalogSectionExpanded,
                        appearance,
                    );
                    body.add_child(
                        Container::new(
                            Flex::row()
                                .with_main_axis_alignment(MainAxisAlignment::Start)
                                .with_child(hidden_toggle_button)
                                .finish(),
                        )
                        .with_margin_top(6.)
                        .finish(),
                    );
                    if hidden_section_expanded {
                        let mut hide_states = self.quick_hide_button_states.borrow_mut();
                        let mut unhide_wrap = Wrap::row()
                            .with_spacing(6.)
                            .with_run_spacing(6.)
                            .with_cross_axis_alignment(CrossAxisAlignment::Center);
                        for (cat_id, cat_provider) in &hidden_matching {
                            let label = if cat_provider.name.is_empty() {
                                cat_id.clone()
                            } else {
                                cat_provider.name.clone()
                            };
                            let state = hide_states.entry(cat_id.clone()).or_default().clone();
                            let unhide_button = Self::render_card_button(
                                format!("↺ {label}"),
                                state,
                                AISettingsPageAction::ToggleCatalogProviderVisibilityOverride {
                                    catalog_provider_id: cat_id.clone(),
                                },
                                appearance,
                            );
                            unhide_wrap = unhide_wrap.with_child(unhide_button);
                        }
                        body.add_child(
                            Container::new(unhide_wrap.finish())
                                .with_margin_top(6.)
                                .finish(),
                        );
                    }
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
                providers.iter().partition(|p| p.effectively_disabled());

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

#[cfg(test)]
mod model_filter_tests {
    use super::*;

    fn model(name: &str, id: &str) -> AgentProviderModel {
        let mut m = AgentProviderModel::from_id(id.to_owned());
        m.name = name.to_owned();
        m
    }

    /// Fork-authored: the price fields are a BYOP addition, so there is no Warp test to port.
    /// The case worth pinning is the blank field — it must stay `None` all the way through,
    /// because a `Some(0.0)` here is what would make `/cost` claim an unpriced model is free.
    #[test]
    fn parse_usd_rate_distinguishes_blank_from_zero() {
        assert_eq!(parse_usd_rate(""), None);
        assert_eq!(parse_usd_rate("   "), None);
        assert_eq!(parse_usd_rate("0"), Some(0.0));
        assert_eq!(parse_usd_rate("0.00"), Some(0.0));
    }

    #[test]
    fn parse_usd_rate_accepts_the_shapes_a_price_list_is_copied_in() {
        assert_eq!(parse_usd_rate("3"), Some(3.0));
        assert_eq!(parse_usd_rate("3.00"), Some(3.0));
        assert_eq!(parse_usd_rate("$15.00"), Some(15.0));
        assert_eq!(parse_usd_rate(" 0.075 "), Some(0.075));
        assert_eq!(parse_usd_rate("1,250"), Some(1250.0));
    }

    #[test]
    fn parse_usd_rate_rejects_nonsense_rather_than_coercing_it() {
        assert_eq!(parse_usd_rate("abc"), None);
        assert_eq!(parse_usd_rate("-3"), None);
        assert_eq!(parse_usd_rate("3.0.0"), None);
    }

    #[test]
    fn format_usd_rate_round_trips_back_into_the_editor() {
        assert_eq!(format_usd_rate(3.0), "3");
        assert_eq!(format_usd_rate(0.3), "0.3");
        assert_eq!(format_usd_rate(0.075), "0.075");
        assert_eq!(format_usd_rate(15.5), "15.5");
        assert_eq!(parse_usd_rate(&format_usd_rate(0.075)), Some(0.075));
    }

    #[test]
    fn model_matches_search_is_empty_query_matches_everything() {
        assert!(model_matches_search(&model("GPT-5", "gpt-5"), ""));
        assert!(model_matches_search(&model("GPT-5", "gpt-5"), "   "));
    }

    #[test]
    fn model_matches_search_is_case_insensitive_on_name_or_id() {
        let m = model("DeepSeek V3 General", "deepseek-chat");
        assert!(model_matches_search(&m, "deepseek"));
        assert!(model_matches_search(&m, "DEEPSEEK"));
        assert!(model_matches_search(&m, "V3 General"));
        assert!(model_matches_search(&m, "chat"));
        assert!(!model_matches_search(&m, "claude"));
    }

    #[test]
    fn model_search_query_round_trips_and_clears_on_blank() {
        let provider_id = "test-provider-search-state";
        assert_eq!(model_search_query(provider_id), "");

        set_model_search_query(provider_id, "gpt".to_owned());
        assert_eq!(model_search_query(provider_id), "gpt");

        // Blank input clears the stored entry rather than storing whitespace.
        set_model_search_query(provider_id, "   ".to_owned());
        assert_eq!(model_search_query(provider_id), "");

        clear_model_search_state_for_provider(provider_id);
        assert_eq!(model_search_query(provider_id), "");
    }

    #[test]
    fn disabled_models_expanded_toggles_per_provider() {
        let provider_id = "test-provider-disabled-models-expand";
        assert!(!disabled_models_expanded(provider_id));

        toggle_disabled_models_expanded(provider_id);
        assert!(disabled_models_expanded(provider_id));

        toggle_disabled_models_expanded(provider_id);
        assert!(!disabled_models_expanded(provider_id));
    }
}
