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

use warp::tui_export::TuiCompletionCandidate;
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
    /// Whether accepting this row should append a trailing space. Only set
    /// when the row was fetched at the end of the buffer and does not name a
    /// directory -- a directory completion leaves the cursor ready to keep
    /// typing the next path segment, so it never gets the trailing space.
    append_space: bool,
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
pub struct TuiAcceptedCompletion {
    pub(crate) replacement: String,
    pub(crate) span: Range<usize>,
    /// Whether the caller should insert a trailing space after `replacement`.
    pub(crate) append_space: bool,
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

    /// Builds a model already open with `candidates`, bypassing `show`'s
    /// input-mode negotiation. For tests that need a populated popup without
    /// wiring up a full session view.
    // Unused until `terminal_session_view/completions.rs` lands (#390) and
    // exercises it from its own tests.
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn new_for_test(
        suggestions_mode: ModelHandle<TuiInputSuggestionsModeModel>,
        candidates: Vec<TuiCompletionCandidate>,
        replacement_span: Range<usize>,
        selected_index: usize,
    ) -> Self {
        let mut list = TuiInlineMenuListState::default();
        list.replace_rows(
            candidates
                .into_iter()
                .map(|candidate| TuiCompletionRow {
                    display: candidate.display,
                    replacement: candidate.replacement,
                    description: candidate.description,
                    append_space: false,
                })
                .collect(),
            false,
            Some(selected_index),
            MAX_VISIBLE_ROWS,
            |_| true,
        );
        Self {
            suggestions_mode,
            state: TuiCompletionsMenuState::Open {
                list,
                replacement_span,
            },
        }
    }

    fn has_open_state(&self) -> bool {
        matches!(self.state, TuiCompletionsMenuState::Open { .. })
    }

    pub(crate) fn is_open(&self, ctx: &AppContext) -> bool {
        self.has_open_state()
            && self.suggestions_mode.as_ref(ctx).mode() == TuiInputSuggestionsMode::Completions
    }

    /// Populates the popup with freshly-fetched candidates and opens it.
    /// `append_space_at_buffer_end` is true when the completion span reaches
    /// the end of the input buffer, so non-directory candidates get a
    /// trailing space on accept (matching shell tab-completion). Returns
    /// `false` (and opens nothing) when there are no candidates or another
    /// mode owns the input.
    pub(crate) fn show(
        &mut self,
        candidates: Vec<TuiCompletionCandidate>,
        replacement_span: Range<usize>,
        append_space_at_buffer_end: bool,
        ctx: &mut ModelContext<Self>,
    ) -> bool {
        if candidates.is_empty() {
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
            candidates
                .into_iter()
                .map(|candidate| TuiCompletionRow {
                    display: candidate.display,
                    replacement: candidate.replacement,
                    description: candidate.description,
                    append_space: append_space_at_buffer_end && !candidate.is_directory,
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

    /// Selects the row at absolute snapshot index `index` (for mouse click).
    /// Returns `true` when the row was actually selected, `false` when the
    /// index is out of bounds or the menu is not open.
    pub(crate) fn select_at_snapshot_index(
        &mut self,
        index: usize,
        ctx: &mut ModelContext<Self>,
    ) -> bool {
        let TuiCompletionsMenuState::Open { list, .. } = &mut self.state else {
            return false;
        };
        let selected = list.select_absolute(index, MAX_VISIBLE_ROWS, |_| true);
        ctx.emit(TuiCompletionsMenuEvent);
        selected
    }

    /// Scrolls the viewport by `delta` rows without changing the selection.
    pub(crate) fn scroll_by_delta(&mut self, delta: isize, ctx: &mut ModelContext<Self>) {
        let TuiCompletionsMenuState::Open { list, .. } = &mut self.state else {
            return;
        };
        list.scroll_by(delta, MAX_VISIBLE_ROWS);
        ctx.emit(TuiCompletionsMenuEvent);
    }

    pub(crate) fn accept_selected(
        &mut self,
        ctx: &mut ModelContext<Self>,
    ) -> Option<TuiAcceptedCompletion> {
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
                append_space: row.append_space,
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
                    prefix: None,
                    description: row.description.clone(),
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
    append_space: bool,
) -> Option<String> {
    if span.start > span.end
        || span.end > buffer_text.len()
        || !buffer_text.is_char_boundary(span.start)
        || !buffer_text.is_char_boundary(span.end)
    {
        return None;
    }
    let trailing_space = if append_space { " " } else { "" };
    let mut new_text = String::with_capacity(
        buffer_text.len() - (span.end - span.start) + replacement.len() + trailing_space.len(),
    );
    new_text.push_str(&buffer_text[..span.start]);
    new_text.push_str(replacement);
    new_text.push_str(trailing_space);
    new_text.push_str(&buffer_text[span.end..]);
    Some(new_text)
}

impl Entity for TuiCompletionsMenuModel {
    type Event = TuiCompletionsMenuEvent;
}

#[cfg(test)]
#[path = "completions_menu_tests.rs"]
mod tests;
