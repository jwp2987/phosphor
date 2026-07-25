//! Shell command/path completion popup state for the TUI (Tab-completion).
//!
//! Unlike the other inline menus (which treat the input buffer as a search
//! query), the completions menu operates on the token under the cursor in the
//! *existing* buffer: the session view fetches candidates from the shared
//! `warp_completer` engine and hands them here via [`TuiCompletionsMenuModel::show`],
//! and accepting a row replaces a byte span in the buffer rather than clearing
//! it. The replacement span (relative to the buffer text) is carried in the open
//! state so the session view can apply it. Mirrors the GUI's
//! `InputSuggestionsMode::CompletionSuggestions { replacement_start, .. }`.

use std::ops::Range;

use warpui_core::{AppContext, Entity, ModelContext, ModelHandle};

use crate::inline_menu::{
    MAX_INLINE_MENU_ROWS, TuiInlineMenuHeader, TuiInlineMenuListState, TuiInlineMenuRow,
    TuiInlineMenuRowStyle, TuiInlineMenuSnapshot, TuiInlineMenuStatus, result_row_capacity,
};
use crate::input_suggestions_mode::{TuiInputSuggestionsMode, TuiInputSuggestionsModeModel};

const MAX_VISIBLE_ROWS: usize = result_row_capacity(MAX_INLINE_MENU_ROWS, true, false);

#[derive(Debug, Clone)]
pub(crate) struct TuiCompletionRow {
    /// The text shown in the popup row.
    display: String,
    /// The text inserted into the buffer when this row is accepted.
    replacement: String,
    /// Optional trailing description (e.g. a flag's help text).
    description: Option<String>,
}

#[derive(Debug, Clone, Default)]
enum TuiCompletionsMenuState {
    #[default]
    Closed,
    Open {
        list: TuiInlineMenuListState<TuiCompletionRow>,
        /// Byte range in the buffer text that an accepted row replaces.
        replacement_span: Range<usize>,
    },
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct TuiCompletionsMenuEvent;

/// A completion accepted from the popup: the replacement text and the byte span
/// in the buffer it should overwrite.
#[derive(Debug, Clone)]
pub(crate) struct TuiAcceptedCompletion {
    pub(crate) replacement: String,
    pub(crate) span: Range<usize>,
}

pub(crate) struct TuiCompletionsMenuModel {
    suggestions_mode: ModelHandle<TuiInputSuggestionsModeModel>,
    state: TuiCompletionsMenuState,
}

impl TuiCompletionsMenuModel {
    pub(crate) fn new(suggestions_mode: ModelHandle<TuiInputSuggestionsModeModel>) -> Self {
        Self {
            suggestions_mode,
            state: TuiCompletionsMenuState::Closed,
        }
    }

    fn has_open_state(&self) -> bool {
        matches!(self.state, TuiCompletionsMenuState::Open { .. })
    }

    pub(crate) fn is_open(&self, ctx: &AppContext) -> bool {
        self.has_open_state()
            && self.suggestions_mode.as_ref(ctx).mode() == TuiInputSuggestionsMode::Completions
    }

    /// Populates the popup with freshly-fetched candidates and opens it. Rows
    /// are `(display, replacement, description)`. Returns `false` (and opens
    /// nothing) when there are no rows or another mode owns the input.
    pub(crate) fn show(
        &mut self,
        rows: Vec<(String, String, Option<String>)>,
        replacement_span: Range<usize>,
        ctx: &mut ModelContext<Self>,
    ) -> bool {
        if rows.is_empty() {
            self.dismiss(ctx);
            return false;
        }
        let did_open = self.suggestions_mode.update(ctx, |mode, ctx| {
            mode.try_open(TuiInputSuggestionsMode::Completions, ctx)
        });
        if !did_open {
            return false;
        }
        let mut list = TuiInlineMenuListState::default();
        list.replace_rows(
            rows.into_iter()
                .map(|(display, replacement, description)| TuiCompletionRow {
                    display,
                    replacement,
                    description,
                })
                .collect(),
            false,
            Some(0),
            MAX_VISIBLE_ROWS,
            |_| true,
        );
        self.state = TuiCompletionsMenuState::Open {
            list,
            replacement_span,
        };
        ctx.emit(TuiCompletionsMenuEvent);
        true
    }

    pub(crate) fn dismiss(&mut self, ctx: &mut ModelContext<Self>) {
        if !self.has_open_state() {
            return;
        }
        self.state = TuiCompletionsMenuState::Closed;
        self.suggestions_mode.update(ctx, |mode, ctx| {
            mode.close_if_active(TuiInputSuggestionsMode::Completions, ctx);
        });
        ctx.emit(TuiCompletionsMenuEvent);
    }

    pub(crate) fn select_previous(&mut self, ctx: &mut ModelContext<Self>) {
        let TuiCompletionsMenuState::Open { list, .. } = &mut self.state else {
            return;
        };
        list.select_previous(MAX_VISIBLE_ROWS, |_| true);
        ctx.emit(TuiCompletionsMenuEvent);
    }

    pub(crate) fn select_next(&mut self, ctx: &mut ModelContext<Self>) {
        let TuiCompletionsMenuState::Open { list, .. } = &mut self.state else {
            return;
        };
        list.select_next(MAX_VISIBLE_ROWS, |_| true);
        ctx.emit(TuiCompletionsMenuEvent);
    }

    pub(crate) fn accept_selected(&mut self, ctx: &mut ModelContext<Self>) -> Option<TuiAcceptedCompletion> {
        if !self.is_open(ctx) {
            return None;
        }
        let accepted = {
            let TuiCompletionsMenuState::Open {
                list,
                replacement_span,
            } = &self.state
            else {
                return None;
            };
            let row = list.selected_row()?;
            TuiAcceptedCompletion {
                replacement: row.replacement.clone(),
                span: replacement_span.clone(),
            }
        };
        self.dismiss(ctx);
        Some(accepted)
    }

    pub(crate) fn snapshot(&self, ctx: &AppContext) -> Option<TuiInlineMenuSnapshot> {
        if !self.is_open(ctx) {
            return None;
        }
        let TuiCompletionsMenuState::Open { list, .. } = &self.state else {
            return None;
        };
        Some(TuiInlineMenuSnapshot {
            header: Some(TuiInlineMenuHeader {
                title: Some("Completions".to_owned()),
                tabs: Vec::new(),
            }),
            rows: list
                .rows()
                .iter()
                .map(|row| TuiInlineMenuRow {
                    title: row.display.clone(),
                    description: row.description.clone(),
                    state_suffix: None,
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
                .then(|| TuiInlineMenuStatus::Empty("No completions".to_owned())),
        })
    }
}

/// Applies an accepted completion to `buffer_text`: replaces the byte range
/// `span` with `replacement`. Returns `None` when the span is stale relative to
/// the current buffer (out of bounds, inverted, or not on char boundaries),
/// which can happen if the buffer changed between fetch and accept.
pub(crate) fn apply_completion_replacement(
    buffer_text: &str,
    replacement: &str,
    span: &Range<usize>,
) -> Option<String> {
    if span.start > span.end
        || span.end > buffer_text.len()
        || !buffer_text.is_char_boundary(span.start)
        || !buffer_text.is_char_boundary(span.end)
    {
        return None;
    }
    let mut new_text =
        String::with_capacity(buffer_text.len() - (span.end - span.start) + replacement.len());
    new_text.push_str(&buffer_text[..span.start]);
    new_text.push_str(replacement);
    new_text.push_str(&buffer_text[span.end..]);
    Some(new_text)
}

impl Entity for TuiCompletionsMenuModel {
    type Event = TuiCompletionsMenuEvent;
}

#[cfg(test)]
#[path = "completions_menu_tests.rs"]
mod tests;
