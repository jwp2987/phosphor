//! Searchable TUI model picker state.

use warp::editor::{CodeEditorModel, CodeEditorModelEvent};
use warp::tui_export::{
    LLMId, LLMPreferences, LLMPreferencesEvent, query_model_picker_choices,
    tui_agent_provider_has_connected_key,
};
use warp_editor::model::CoreEditorModel;
use warpui_core::{AppContext, Entity, EntityId, ModelContext, ModelHandle, SingletonEntity};

use crate::inline_menu::{
    MAX_INLINE_MENU_ROWS, TuiInlineMenuHeader, TuiInlineMenuListState, TuiInlineMenuRow,
    TuiInlineMenuRowStyle, TuiInlineMenuSnapshot, TuiInlineMenuStatus, result_row_capacity,
};
use crate::input_suggestions_mode::{TuiInputSuggestionsMode, TuiInputSuggestionsModeModel};

const MAX_VISIBLE_ROWS: usize = result_row_capacity(MAX_INLINE_MENU_ROWS, true, false);

/// Row suffix shown for a BYOP model whose provider currently has a connected API key --
/// mirrors the GUI model picker's `Icon::Key` treatment (`terminal/input/models/data_source.rs`)
/// as plain text, since the ratatui surface has no icon glyphs. See
/// `tui_agent_provider_has_connected_key` for what "connected" means.
const KEY_CONNECTED_SUFFIX: &str = "(key connected)";

/// Row suffix marking the active execution profile's own base model -- the model this
/// surface falls back to once the picker's ad-hoc overrides are out of the way. Mirrors
/// the GUI picker's "default" badge. See `LLMPreferences::get_active_profile_base_model`.
const PROFILE_DEFAULT_SUFFIX: &str = "(default)";

/// Both badges on one row, in the pin's order (profile default first).
const PROFILE_DEFAULT_AND_KEY_CONNECTED_SUFFIX: &str = "(default) (key connected)";

#[derive(Debug, Clone)]
struct TuiModelMenuRow {
    id: LLMId,
    title: String,
    is_selectable: bool,
    /// Whether this model's provider currently has a usable, connected API key. Always `false`
    /// for non-BYOP entries (see `tui_agent_provider_has_connected_key`).
    key_connected: bool,
    /// Whether this row is the active execution profile's base model. Distinct from the
    /// *selected* row: the picker preselects the effective active model, which a
    /// per-surface override or the BYOP last-used model can move off the profile default.
    is_profile_default: bool,
}

#[derive(Debug, Clone, Default)]
enum TuiModelMenuState {
    #[default]
    Closed,
    Open {
        list: TuiInlineMenuListState<TuiModelMenuRow>,
    },
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct TuiModelMenuEvent;

pub(crate) struct TuiModelMenuModel {
    input_editor: ModelHandle<CodeEditorModel>,
    suggestions_mode: ModelHandle<TuiInputSuggestionsModeModel>,
    /// The owning session surface, so model resolution asks about *this* surface
    /// rather than the global default (see `refresh_rows`).
    terminal_view_id: EntityId,
    state: TuiModelMenuState,
}

impl TuiModelMenuModel {
    pub(crate) fn new(
        input_editor: ModelHandle<CodeEditorModel>,
        suggestions_mode: ModelHandle<TuiInputSuggestionsModeModel>,
        terminal_view_id: EntityId,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        ctx.subscribe_to_model(&input_editor, |model, event, ctx| {
            if model.is_open(ctx) && matches!(event, CodeEditorModelEvent::ContentChanged { .. }) {
                model.refresh_rows(ctx);
            }
        });
        ctx.subscribe_to_model(&LLMPreferences::handle(ctx), |model, event, ctx| {
            if model.is_open(ctx)
                && matches!(
                    event,
                    LLMPreferencesEvent::UpdatedAvailableLLMs
                        | LLMPreferencesEvent::UpdatedActiveAgentModeLLM
                )
            {
                model.refresh_rows(ctx);
            }
        });
        Self {
            input_editor,
            suggestions_mode,
            terminal_view_id,
            state: TuiModelMenuState::Closed,
        }
    }

    /// `rows` is `(id, is_selectable, key_connected)`; `key_connected` drives the
    /// "(key connected)" snapshot suffix without needing a live `AppContext` /
    /// `AgentProviderSecrets` singleton (see `refresh_rows` for the real computation).
    /// `is_profile_default` is always `false` here for the same reason -- resolving it
    /// needs live `LLMPreferences`, so the "(default)" badge is covered by
    /// `model_menu_labels_the_profile_default_model` against a real session instead.
    #[cfg(test)]
    pub(crate) fn new_for_test(
        input_editor: ModelHandle<CodeEditorModel>,
        suggestions_mode: ModelHandle<TuiInputSuggestionsModeModel>,
        rows: Vec<(LLMId, bool, bool)>,
        selected_index: usize,
    ) -> Self {
        let mut list = TuiInlineMenuListState::default();
        list.replace_rows(
            rows.into_iter()
                .map(|(id, is_selectable, key_connected)| TuiModelMenuRow {
                    title: id.to_string(),
                    id,
                    is_selectable,
                    key_connected,
                    is_profile_default: false,
                })
                .collect(),
            false,
            Some(selected_index),
            MAX_VISIBLE_ROWS,
            |row| row.is_selectable,
        );
        Self {
            input_editor,
            suggestions_mode,
            terminal_view_id: EntityId::new(),
            state: TuiModelMenuState::Open { list },
        }
    }

    fn has_open_state(&self) -> bool {
        matches!(self.state, TuiModelMenuState::Open { .. })
    }

    pub(crate) fn is_open(&self, ctx: &AppContext) -> bool {
        self.has_open_state()
            && self.suggestions_mode.as_ref(ctx).mode() == TuiInputSuggestionsMode::ModelSelector
    }

    pub(crate) fn open(&mut self, ctx: &mut ModelContext<Self>) {
        if self.has_open_state() {
            return;
        }
        let did_open = self.suggestions_mode.update(ctx, |mode, ctx| {
            mode.try_open(TuiInputSuggestionsMode::ModelSelector, ctx)
        });
        if !did_open {
            return;
        }
        self.input_editor
            .update(ctx, |editor, ctx| editor.clear_buffer(ctx));
        self.state = TuiModelMenuState::Open {
            list: TuiInlineMenuListState::default(),
        };
        self.refresh_rows(ctx);
    }

    pub(crate) fn dismiss(&mut self, ctx: &mut ModelContext<Self>) {
        if !self.is_open(ctx) {
            return;
        }
        self.state = TuiModelMenuState::Closed;
        self.suggestions_mode.update(ctx, |mode, ctx| {
            mode.close_if_active(TuiInputSuggestionsMode::ModelSelector, ctx);
        });
        self.input_editor
            .update(ctx, |editor, ctx| editor.clear_buffer(ctx));
        ctx.emit(TuiModelMenuEvent);
    }

    pub(crate) fn select_previous(&mut self, ctx: &mut ModelContext<Self>) {
        let TuiModelMenuState::Open { list } = &mut self.state else {
            return;
        };
        list.select_previous(MAX_VISIBLE_ROWS, |row| row.is_selectable);
        ctx.emit(TuiModelMenuEvent);
    }

    pub(crate) fn select_next(&mut self, ctx: &mut ModelContext<Self>) {
        let TuiModelMenuState::Open { list } = &mut self.state else {
            return;
        };
        list.select_next(MAX_VISIBLE_ROWS, |row| row.is_selectable);
        ctx.emit(TuiModelMenuEvent);
    }

    /// Selects the row at absolute snapshot index `index` (for mouse click).
    /// Returns `true` when the row was actually selected, `false` when the
    /// index is out of bounds, the menu is not open, or the row is not
    /// selectable.
    pub(crate) fn select_at_snapshot_index(
        &mut self,
        index: usize,
        ctx: &mut ModelContext<Self>,
    ) -> bool {
        let TuiModelMenuState::Open { list } = &mut self.state else {
            return false;
        };
        let selected = list.select_absolute(index, MAX_VISIBLE_ROWS, |row| row.is_selectable);
        ctx.emit(TuiModelMenuEvent);
        selected
    }

    /// Scrolls the viewport by `delta` rows without changing the selection.
    pub(crate) fn scroll_by_delta(&mut self, delta: isize, ctx: &mut ModelContext<Self>) {
        let TuiModelMenuState::Open { list } = &mut self.state else {
            return;
        };
        list.scroll_by(delta, MAX_VISIBLE_ROWS);
        ctx.emit(TuiModelMenuEvent);
    }

    pub(crate) fn accept_selected(&self, ctx: &AppContext) -> Option<LLMId> {
        if !self.is_open(ctx) {
            return None;
        }
        let TuiModelMenuState::Open { list } = &self.state else {
            return None;
        };
        list.selected_row().map(|row| row.id.clone())
    }

    pub(crate) fn snapshot(&self, ctx: &AppContext) -> Option<TuiInlineMenuSnapshot> {
        if !self.is_open(ctx) {
            return None;
        }
        let TuiModelMenuState::Open { list } = &self.state else {
            return None;
        };
        Some(TuiInlineMenuSnapshot {
            header: Some(TuiInlineMenuHeader {
                title: Some("Models".to_owned()),
                tabs: Vec::new(),
            }),
            rows: list.rows().iter().map(snapshot_row).collect(),
            selected_index: list.selected_index(),
            scroll_offset: list.scroll_offset(),
            scroll_anchor: list.scroll_anchor(),
            max_visible_rows: MAX_VISIBLE_ROWS,
            status: list
                .rows()
                .is_empty()
                .then(|| TuiInlineMenuStatus::Empty("No models found".to_owned())),
        })
    }

    fn refresh_rows(&mut self, ctx: &mut ModelContext<Self>) {
        if !self.is_open(ctx) {
            return;
        }
        let query = input_text(&self.input_editor, ctx);
        let terminal_view_id = self.terminal_view_id;
        let preferences = LLMPreferences::as_ref(ctx);
        let active_id = preferences
            .get_active_base_model(ctx, Some(terminal_view_id))
            .id
            .clone();
        // The profile's *own* base model, which the effective active model above can
        // sit off (a per-surface override or the BYOP last-used model wins over it).
        let profile_default_id = preferences
            .get_active_profile_base_model(ctx, Some(terminal_view_id))
            .id
            .clone();
        let choices = query_model_picker_choices(
            // Zap's `get_base_llm_choices_for_agent_mode` takes no `ctx` (warp/master's does).
            preferences.get_base_llm_choices_for_agent_mode(),
            &query,
            ctx,
        );
        let rows = choices
            .into_iter()
            .map(|choice| {
                let is_selectable = choice.is_selectable();
                let key_connected = tui_agent_provider_has_connected_key(ctx, &choice.llm.id);
                TuiModelMenuRow {
                    is_profile_default: choice.llm.id == profile_default_id,
                    id: choice.llm.id,
                    title: choice.llm.display_name,
                    is_selectable,
                    key_connected,
                }
            })
            .collect::<Vec<_>>();
        let preferred_index = preferred_selection_index(&rows, &active_id, query.trim().is_empty());
        let TuiModelMenuState::Open { list } = &mut self.state else {
            return;
        };
        list.replace_rows(rows, false, preferred_index, MAX_VISIBLE_ROWS, |row| {
            row.is_selectable
        });
        ctx.emit(TuiModelMenuEvent);
    }
}

/// Renders one picker row, folding both state badges into the single
/// `state_suffix` slot the inline-menu row model provides.
fn snapshot_row(row: &TuiModelMenuRow) -> TuiInlineMenuRow {
    let state_suffix = match (row.is_profile_default, row.key_connected) {
        (true, true) => Some(PROFILE_DEFAULT_AND_KEY_CONNECTED_SUFFIX.to_owned()),
        (true, false) => Some(PROFILE_DEFAULT_SUFFIX.to_owned()),
        (false, true) => Some(KEY_CONNECTED_SUFFIX.to_owned()),
        (false, false) => None,
    };
    TuiInlineMenuRow {
        title: row.title.clone(),
        prefix: None,
        description: (!row.is_selectable).then(|| "disabled".to_owned()),
        state_suffix,
        is_selectable: row.is_selectable,
        style: TuiInlineMenuRowStyle::Default,
    }
}

fn preferred_selection_index(
    rows: &[TuiModelMenuRow],
    active_id: &LLMId,
    query_is_empty: bool,
) -> Option<usize> {
    if query_is_empty {
        rows.iter()
            .position(|row| row.id == *active_id && row.is_selectable)
            .or_else(|| rows.iter().rposition(|row| row.is_selectable))
    } else {
        rows.iter().rposition(|row| row.is_selectable)
    }
}

fn input_text(editor: &ModelHandle<CodeEditorModel>, app: &AppContext) -> String {
    let model = editor.as_ref(app);
    let buffer = model.content().as_ref(app);
    if buffer.is_empty() {
        String::new()
    } else {
        buffer.text().into_string()
    }
}

impl Entity for TuiModelMenuModel {
    type Event = TuiModelMenuEvent;
}

#[cfg(test)]
#[path = "model_menu_tests.rs"]
mod tests;
