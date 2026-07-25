//! Searchable TUI agent-profile picker (the `/profile` menu).
//!
//! Mirrors [`crate::model_menu`]: the input buffer is a search query, and
//! accepting a row switches the active execution profile for this terminal
//! view (which also drops the pane LLM override so the profile's model can
//! apply — see [`warp::tui_export::tui_set_active_profile`]).

use warp::editor::{CodeEditorModel, CodeEditorModelEvent};
use warp::tui_export::ClientProfileId;
use warp::tui_export::tui_list_profiles;
use warp_editor::model::CoreEditorModel;
use warpui_core::{AppContext, Entity, EntityId, ModelContext, ModelHandle};

use crate::inline_menu::{
    MAX_INLINE_MENU_ROWS, TuiInlineMenuHeader, TuiInlineMenuListState, TuiInlineMenuRow,
    TuiInlineMenuRowStyle, TuiInlineMenuSnapshot, TuiInlineMenuStatus, result_row_capacity,
};
use crate::input_suggestions_mode::{TuiInputSuggestionsMode, TuiInputSuggestionsModeModel};

const MAX_VISIBLE_ROWS: usize = result_row_capacity(MAX_INLINE_MENU_ROWS, true, false);

#[derive(Debug, Clone)]
struct TuiProfileMenuRow {
    id: ClientProfileId,
    title: String,
    is_active: bool,
}

#[derive(Debug, Clone, Default)]
enum TuiProfileMenuState {
    #[default]
    Closed,
    Open {
        list: TuiInlineMenuListState<TuiProfileMenuRow>,
    },
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct TuiProfileMenuEvent;

pub(crate) struct TuiProfileMenuModel {
    input_editor: ModelHandle<CodeEditorModel>,
    suggestions_mode: ModelHandle<TuiInputSuggestionsModeModel>,
    terminal_view_id: EntityId,
    state: TuiProfileMenuState,
}

impl TuiProfileMenuModel {
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
        Self {
            input_editor,
            suggestions_mode,
            terminal_view_id,
            state: TuiProfileMenuState::Closed,
        }
    }

    fn has_open_state(&self) -> bool {
        matches!(self.state, TuiProfileMenuState::Open { .. })
    }

    pub(crate) fn is_open(&self, ctx: &AppContext) -> bool {
        self.has_open_state()
            && self.suggestions_mode.as_ref(ctx).mode() == TuiInputSuggestionsMode::ProfileSelector
    }

    pub(crate) fn open(&mut self, ctx: &mut ModelContext<Self>) {
        if self.has_open_state() {
            return;
        }
        let did_open = self.suggestions_mode.update(ctx, |mode, ctx| {
            mode.try_open(TuiInputSuggestionsMode::ProfileSelector, ctx)
        });
        if !did_open {
            return;
        }
        self.input_editor
            .update(ctx, |editor, ctx| editor.clear_buffer(ctx));
        self.state = TuiProfileMenuState::Open {
            list: TuiInlineMenuListState::default(),
        };
        self.refresh_rows(ctx);
    }

    pub(crate) fn dismiss(&mut self, ctx: &mut ModelContext<Self>) {
        if !self.is_open(ctx) {
            return;
        }
        self.state = TuiProfileMenuState::Closed;
        self.suggestions_mode.update(ctx, |mode, ctx| {
            mode.close_if_active(TuiInputSuggestionsMode::ProfileSelector, ctx);
        });
        self.input_editor
            .update(ctx, |editor, ctx| editor.clear_buffer(ctx));
        ctx.emit(TuiProfileMenuEvent);
    }

    pub(crate) fn select_previous(&mut self, ctx: &mut ModelContext<Self>) {
        let TuiProfileMenuState::Open { list } = &mut self.state else {
            return;
        };
        list.select_previous(MAX_VISIBLE_ROWS, |_| true);
        ctx.emit(TuiProfileMenuEvent);
    }

    pub(crate) fn select_next(&mut self, ctx: &mut ModelContext<Self>) {
        let TuiProfileMenuState::Open { list } = &mut self.state else {
            return;
        };
        list.select_next(MAX_VISIBLE_ROWS, |_| true);
        ctx.emit(TuiProfileMenuEvent);
    }

    pub(crate) fn accept_selected(&self, ctx: &AppContext) -> Option<ClientProfileId> {
        if !self.is_open(ctx) {
            return None;
        }
        let TuiProfileMenuState::Open { list } = &self.state else {
            return None;
        };
        list.selected_row().map(|row| row.id)
    }

    pub(crate) fn snapshot(&self, ctx: &AppContext) -> Option<TuiInlineMenuSnapshot> {
        if !self.is_open(ctx) {
            return None;
        }
        let TuiProfileMenuState::Open { list } = &self.state else {
            return None;
        };
        Some(TuiInlineMenuSnapshot {
            header: Some(TuiInlineMenuHeader {
                title: Some("Profiles".to_owned()),
                tabs: Vec::new(),
            }),
            rows: list
                .rows()
                .iter()
                .map(|row| TuiInlineMenuRow {
                    title: row.title.clone(),
                    description: None,
                    state_suffix: row.is_active.then(|| "active".to_owned()),
                    is_selectable: true,
                    style: TuiInlineMenuRowStyle::Default,
                })
                .collect(),
            selected_index: list.selected_index(),
            scroll_offset: list.scroll_offset(),
            max_visible_rows: MAX_VISIBLE_ROWS,
            status: list
                .rows()
                .is_empty()
                .then(|| TuiInlineMenuStatus::Empty("No profiles found".to_owned())),
        })
    }

    fn refresh_rows(&mut self, ctx: &mut ModelContext<Self>) {
        if !self.is_open(ctx) {
            return;
        }
        let query = input_text(&self.input_editor, ctx);
        let query_lower = query.trim().to_lowercase();
        let rows = tui_list_profiles(ctx, self.terminal_view_id)
            .into_iter()
            .filter(|entry| {
                query_lower.is_empty() || entry.display_name.to_lowercase().contains(&query_lower)
            })
            .map(|entry| TuiProfileMenuRow {
                id: entry.id,
                title: entry.display_name,
                is_active: entry.is_active,
            })
            .collect::<Vec<_>>();
        // Preselect the active profile when the query is empty; otherwise the
        // first (best) match.
        let preferred_index = if query_lower.is_empty() {
            rows.iter().position(|row| row.is_active).or(Some(0))
        } else {
            Some(0)
        };
        let TuiProfileMenuState::Open { list } = &mut self.state else {
            return;
        };
        list.replace_rows(rows, false, preferred_index, MAX_VISIBLE_ROWS, |_| true);
        ctx.emit(TuiProfileMenuEvent);
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

impl Entity for TuiProfileMenuModel {
    type Event = TuiProfileMenuEvent;
}

#[cfg(test)]
#[path = "profile_menu_tests.rs"]
mod tests;
