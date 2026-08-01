//! Searchable TUI picker over a conversation's user-query exchanges, used by
//! `/fork-from` (fork the conversation at the chosen exchange) and `/rewind`
//! (roll the conversation back to the chosen exchange).
//!
//! Mirrors [`crate::profile_menu`]: the input buffer is a search query, and each
//! row is one past user query (via [`warp::tui_export::tui_list_conversation_exchanges`]).
//! The menu carries the action and conversation it was opened for so the session
//! view can route the accepted exchange to the right operation.

use warp::editor::{CodeEditorModel, CodeEditorModelEvent};
use warp::tui_export::{tui_list_conversation_exchanges, AIAgentExchangeId, AIConversationId};
use warp_editor::model::CoreEditorModel;
use warpui_core::{AppContext, Entity, ModelContext, ModelHandle};

use crate::inline_menu::{
    result_row_capacity, TuiInlineMenuHeader, TuiInlineMenuListState, TuiInlineMenuRow,
    TuiInlineMenuRowStyle, TuiInlineMenuSnapshot, TuiInlineMenuStatus, MAX_INLINE_MENU_ROWS,
};
use crate::input_suggestions_mode::{TuiInputSuggestionsMode, TuiInputSuggestionsModeModel};

const MAX_VISIBLE_ROWS: usize = result_row_capacity(MAX_INLINE_MENU_ROWS, true, false);

/// The operation the exchange picker was opened to perform.
///
/// `pub` (not `pub(crate)`) so it can appear in the `pub` [`crate::input`]
/// `TuiInputViewEvent::AcceptedExchange`; the `exchange_menu` module itself is
/// private, so this stays crate-internal in practice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiExchangeMenuAction {
    /// `/fork-from`: fork the conversation at the selected exchange.
    ForkFrom,
    /// `/rewind`: roll the conversation back to the selected exchange.
    Rewind,
}

impl TuiExchangeMenuAction {
    fn header_title(self) -> &'static str {
        match self {
            Self::ForkFrom => "Fork from",
            Self::Rewind => "Rewind to",
        }
    }
}

#[derive(Debug, Clone)]
struct TuiExchangeMenuRow {
    id: AIAgentExchangeId,
    title: String,
}

#[derive(Debug, Clone, Default)]
enum TuiExchangeMenuState {
    #[default]
    Closed,
    Open {
        list: TuiInlineMenuListState<TuiExchangeMenuRow>,
        conversation_id: AIConversationId,
        action: TuiExchangeMenuAction,
    },
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct TuiExchangeMenuEvent;

pub(crate) struct TuiExchangeMenuModel {
    input_editor: ModelHandle<CodeEditorModel>,
    suggestions_mode: ModelHandle<TuiInputSuggestionsModeModel>,
    state: TuiExchangeMenuState,
}

impl TuiExchangeMenuModel {
    pub(crate) fn new(
        input_editor: ModelHandle<CodeEditorModel>,
        suggestions_mode: ModelHandle<TuiInputSuggestionsModeModel>,
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
            state: TuiExchangeMenuState::Closed,
        }
    }

    fn has_open_state(&self) -> bool {
        matches!(self.state, TuiExchangeMenuState::Open { .. })
    }

    pub(crate) fn is_open(&self, ctx: &AppContext) -> bool {
        self.has_open_state()
            && self.suggestions_mode.as_ref(ctx).mode() == TuiInputSuggestionsMode::ExchangeMenu
    }

    pub(crate) fn open(
        &mut self,
        action: TuiExchangeMenuAction,
        conversation_id: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) {
        if self.has_open_state() {
            return;
        }
        let did_open = self.suggestions_mode.update(ctx, |mode, ctx| {
            mode.try_open(TuiInputSuggestionsMode::ExchangeMenu, ctx)
        });
        if !did_open {
            return;
        }
        self.input_editor
            .update(ctx, |editor, ctx| editor.clear_buffer(ctx));
        self.state = TuiExchangeMenuState::Open {
            list: TuiInlineMenuListState::default(),
            conversation_id,
            action,
        };
        self.refresh_rows(ctx);
    }

    pub(crate) fn dismiss(&mut self, ctx: &mut ModelContext<Self>) {
        if !self.is_open(ctx) {
            return;
        }
        self.state = TuiExchangeMenuState::Closed;
        self.suggestions_mode.update(ctx, |mode, ctx| {
            mode.close_if_active(TuiInputSuggestionsMode::ExchangeMenu, ctx);
        });
        self.input_editor
            .update(ctx, |editor, ctx| editor.clear_buffer(ctx));
        ctx.emit(TuiExchangeMenuEvent);
    }

    pub(crate) fn select_previous(&mut self, ctx: &mut ModelContext<Self>) {
        let TuiExchangeMenuState::Open { list, .. } = &mut self.state else {
            return;
        };
        list.select_previous(MAX_VISIBLE_ROWS, |_| true);
        ctx.emit(TuiExchangeMenuEvent);
    }

    pub(crate) fn select_next(&mut self, ctx: &mut ModelContext<Self>) {
        let TuiExchangeMenuState::Open { list, .. } = &mut self.state else {
            return;
        };
        list.select_next(MAX_VISIBLE_ROWS, |_| true);
        ctx.emit(TuiExchangeMenuEvent);
    }

    /// Selects the row at absolute snapshot index `index` (for mouse click).
    /// Returns `true` when the row was actually selected, `false` when the
    /// index is out of bounds or the menu is not open.
    pub(crate) fn select_at_snapshot_index(
        &mut self,
        index: usize,
        ctx: &mut ModelContext<Self>,
    ) -> bool {
        let TuiExchangeMenuState::Open { list, .. } = &mut self.state else {
            return false;
        };
        let selected = list.select_absolute(index, MAX_VISIBLE_ROWS, |_| true);
        ctx.emit(TuiExchangeMenuEvent);
        selected
    }

    /// Scrolls the viewport by `delta` rows without changing the selection.
    pub(crate) fn scroll_by_delta(&mut self, delta: isize, ctx: &mut ModelContext<Self>) {
        let TuiExchangeMenuState::Open { list, .. } = &mut self.state else {
            return;
        };
        list.scroll_by(delta, MAX_VISIBLE_ROWS);
        ctx.emit(TuiExchangeMenuEvent);
    }

    /// The selected exchange and the action the menu was opened for.
    pub(crate) fn accept_selected(
        &self,
        ctx: &AppContext,
    ) -> Option<(AIAgentExchangeId, TuiExchangeMenuAction)> {
        if !self.is_open(ctx) {
            return None;
        }
        let TuiExchangeMenuState::Open { list, action, .. } = &self.state else {
            return None;
        };
        list.selected_row().map(|row| (row.id, *action))
    }

    pub(crate) fn snapshot(&self, ctx: &AppContext) -> Option<TuiInlineMenuSnapshot> {
        if !self.is_open(ctx) {
            return None;
        }
        let TuiExchangeMenuState::Open { list, action, .. } = &self.state else {
            return None;
        };
        Some(TuiInlineMenuSnapshot {
            header: Some(TuiInlineMenuHeader {
                title: Some(action.header_title().to_owned()),
                tabs: Vec::new(),
            }),
            rows: list
                .rows()
                .iter()
                .map(|row| TuiInlineMenuRow {
                    title: row.title.clone(),
                    description: None,
                    state_suffix: None,
                    is_selectable: true,
                    style: TuiInlineMenuRowStyle::Default,
                })
                .collect(),
            selected_index: list.selected_index(),
            scroll_offset: list.scroll_offset(),
            scroll_anchor: list.scroll_anchor(),
            max_visible_rows: MAX_VISIBLE_ROWS,
            status: list
                .rows()
                .is_empty()
                .then(|| TuiInlineMenuStatus::Empty("No earlier messages found".to_owned())),
        })
    }

    fn refresh_rows(&mut self, ctx: &mut ModelContext<Self>) {
        if !self.is_open(ctx) {
            return;
        }
        let TuiExchangeMenuState::Open {
            conversation_id, ..
        } = &self.state
        else {
            return;
        };
        let conversation_id = *conversation_id;
        let query = input_text(&self.input_editor, ctx);
        let query_lower = query.trim().to_lowercase();
        let rows = tui_list_conversation_exchanges(ctx, conversation_id)
            .into_iter()
            .map(|entry| TuiExchangeMenuRow {
                id: entry.id,
                title: single_line_label(&entry.query_text),
            })
            .filter(|row| query_lower.is_empty() || row.title.to_lowercase().contains(&query_lower))
            .collect::<Vec<_>>();
        // Preselect the most recent exchange (the last row) when unfiltered, so
        // the default fork/rewind point is "just before now"; otherwise the first
        // match.
        let preferred_index = if query_lower.is_empty() {
            rows.len().checked_sub(1)
        } else {
            Some(0)
        };
        let TuiExchangeMenuState::Open { list, .. } = &mut self.state else {
            return;
        };
        list.replace_rows(rows, false, preferred_index, MAX_VISIBLE_ROWS, |_| true);
        ctx.emit(TuiExchangeMenuEvent);
    }
}

/// Collapses a possibly multi-line user query into a single trimmed line for the
/// picker row label.
fn single_line_label(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        "(empty message)".to_owned()
    } else {
        collapsed
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

impl Entity for TuiExchangeMenuModel {
    type Event = TuiExchangeMenuEvent;
}

#[cfg(test)]
#[path = "exchange_menu_tests.rs"]
mod tests;
