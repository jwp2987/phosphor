//! Inline menu backing the TUI `/api-keys` picker: lists every configured BYOP agent provider
//! (arbitrary user-defined endpoints, not a fixed catalog) alongside whether each currently has
//! an API key stored, and lets the user set/update or clear a key without leaving the TUI.
//!
//! Reuses `AgentProviderSecrets` -- via the `warp::tui_export::tui_*_agent_provider_*` helpers
//! -- the same secure-storage-backed store the GUI Settings AI page writes to
//! (`AISettingsPageAction::UpdateAgentProviderApiKey`), so a key set here is immediately visible
//! in Settings and vice versa; this menu never builds its own parallel key store.
//!
//! Mirrors upstream Warp's `/add-api-key` / `/clear-provider-api-key`
//! (`a9099599e`, `bf56c3c18`) in spirit -- open a picker, see connection state, add/clear a key
//! inline -- but implemented against this fork's arbitrary-provider BYOP model instead of
//! upstream's hardcoded ~4-provider list, and deliberately without upstream's
//! Warp-credit-fallback toggle or Grok-subscription OAuth connect flow: both are cloud-billing
//! concepts this fork, being BYOP-only, doesn't have.
//!
//! Structurally two sub-states share one [`TuiInputSuggestionsMode::ApiKeys`] slot:
//! - [`TuiApiKeysMenuState::List`]: one row per configured provider, each showing a
//!   "(key connected)" / "(no key)" suffix (mirroring the GUI model picker's `Icon::Key`
//!   convention in `terminal/input/models/data_source.rs`, as plain text since the ratatui
//!   surface has no icon glyphs), plus a second "Clear key" row under any provider that already
//!   has one -- the same two-rows-per-item shape `crate::mcp_menu` uses for its "Log out"
//!   action.
//! - [`TuiApiKeysMenuState::EnteringKey`]: selecting a provider's row switches into this state
//!   and repurposes the shared input buffer as free-text key entry (the same buffer every other
//!   menu here uses as a search filter) instead of a row list; Enter submits and returns to
//!   `List`, Esc cancels back to `List` without closing the menu entirely -- only a second Esc,
//!   from `List`, closes it. The typed key is not masked: the ratatui input line has no masking
//!   primitive today, and this is the same exposure a locally-typed shell command already has.

use warp::editor::{CodeEditorModel, CodeEditorModelEvent};
use warp::tui_export::{
    TuiApiKeyProvider, tui_clear_agent_provider_api_key, tui_list_agent_provider_keys,
    tui_set_agent_provider_api_key,
};
use warp_editor::model::CoreEditorModel;
use warpui_core::{AppContext, Entity, ModelContext, ModelHandle};

use crate::inline_menu::{
    MAX_INLINE_MENU_ROWS, TuiInlineMenuHeader, TuiInlineMenuListState, TuiInlineMenuRow,
    TuiInlineMenuRowStyle, TuiInlineMenuScrollAnchor, TuiInlineMenuSnapshot, TuiInlineMenuStatus,
    result_row_capacity,
};
use crate::input_suggestions_mode::{TuiInputSuggestionsMode, TuiInputSuggestionsModeModel};

const MAX_VISIBLE_ROWS: usize = result_row_capacity(MAX_INLINE_MENU_ROWS, true, false);

#[derive(Debug, Clone)]
enum TuiApiKeysRowAction {
    /// Selecting this row opens [`TuiApiKeysMenuState::EnteringKey`] for the provider, whether
    /// or not it already has a key (re-entering overwrites the stored key).
    Edit(String),
    /// Selecting this row clears the provider's stored key immediately, no confirmation step --
    /// mirrors `crate::mcp_menu`'s "Log out" row, which is likewise a single-Enter action.
    Clear(String),
}

#[derive(Debug, Clone)]
struct TuiApiKeysMenuRow {
    title: String,
    description: Option<String>,
    state_suffix: Option<String>,
    action: TuiApiKeysRowAction,
}

#[derive(Debug, Clone, Default)]
enum TuiApiKeysMenuState {
    #[default]
    Closed,
    List {
        list: TuiInlineMenuListState<TuiApiKeysMenuRow>,
    },
    EnteringKey {
        provider_id: String,
        provider_name: String,
    },
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct TuiApiKeysMenuEvent;

pub(crate) struct TuiApiKeysMenuModel {
    input_editor: ModelHandle<CodeEditorModel>,
    suggestions_mode: ModelHandle<TuiInputSuggestionsModeModel>,
    state: TuiApiKeysMenuState,
}

impl TuiApiKeysMenuModel {
    pub(crate) fn new(
        input_editor: ModelHandle<CodeEditorModel>,
        suggestions_mode: ModelHandle<TuiInputSuggestionsModeModel>,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        ctx.subscribe_to_model(&input_editor, |model, event, ctx| {
            // Only `List` treats the input buffer as a live filter query; `EnteringKey` leaves
            // it alone there -- the buffer *is* the key being typed, not a search term, and
            // re-filtering rows out from under an in-progress key entry would be actively wrong.
            if model.is_list_open(ctx) && matches!(event, CodeEditorModelEvent::ContentChanged { .. }) {
                model.refresh_rows(ctx);
            }
        });
        Self {
            input_editor,
            suggestions_mode,
            state: TuiApiKeysMenuState::Closed,
        }
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(
        input_editor: ModelHandle<CodeEditorModel>,
        suggestions_mode: ModelHandle<TuiInputSuggestionsModeModel>,
        rows: Vec<(&str, bool)>,
    ) -> Self {
        let mut list = TuiInlineMenuListState::default();
        let rows = rows
            .into_iter()
            .map(|(name, has_key)| test_provider_row(name, has_key))
            .collect::<Vec<_>>();
        let preferred_index = (!rows.is_empty()).then_some(0);
        list.replace_rows(rows, false, preferred_index, MAX_VISIBLE_ROWS, |_| true);
        Self {
            input_editor,
            suggestions_mode,
            state: TuiApiKeysMenuState::List { list },
        }
    }

    fn has_open_state(&self) -> bool {
        !matches!(self.state, TuiApiKeysMenuState::Closed)
    }

    pub(crate) fn is_open(&self, ctx: &AppContext) -> bool {
        self.has_open_state()
            && self.suggestions_mode.as_ref(ctx).mode() == TuiInputSuggestionsMode::ApiKeys
    }

    fn is_list_open(&self, ctx: &AppContext) -> bool {
        self.is_open(ctx) && matches!(self.state, TuiApiKeysMenuState::List { .. })
    }

    pub(crate) fn open(&mut self, ctx: &mut ModelContext<Self>) {
        if self.has_open_state() {
            return;
        }
        let did_open = self.suggestions_mode.update(ctx, |mode, ctx| {
            mode.try_open(TuiInputSuggestionsMode::ApiKeys, ctx)
        });
        if !did_open {
            return;
        }
        self.input_editor
            .update(ctx, |editor, ctx| editor.clear_buffer(ctx));
        self.state = TuiApiKeysMenuState::List {
            list: TuiInlineMenuListState::default(),
        };
        self.refresh_rows(ctx);
    }

    /// Closes the active sub-state one level at a time: cancels an in-progress key entry back
    /// to the list, or -- from the list -- closes the menu entirely. See the module docs.
    pub(crate) fn dismiss(&mut self, ctx: &mut ModelContext<Self>) {
        if !self.is_open(ctx) {
            return;
        }
        match &self.state {
            TuiApiKeysMenuState::EnteringKey { .. } => {
                self.state = TuiApiKeysMenuState::List {
                    list: TuiInlineMenuListState::default(),
                };
                self.input_editor
                    .update(ctx, |editor, ctx| editor.clear_buffer(ctx));
                self.refresh_rows(ctx);
            }
            TuiApiKeysMenuState::List { .. } => {
                self.state = TuiApiKeysMenuState::Closed;
                self.suggestions_mode.update(ctx, |mode, ctx| {
                    mode.close_if_active(TuiInputSuggestionsMode::ApiKeys, ctx);
                });
                self.input_editor
                    .update(ctx, |editor, ctx| editor.clear_buffer(ctx));
            }
            TuiApiKeysMenuState::Closed => {}
        }
        ctx.emit(TuiApiKeysMenuEvent);
    }

    pub(crate) fn select_previous(&mut self, ctx: &mut ModelContext<Self>) {
        let TuiApiKeysMenuState::List { list } = &mut self.state else {
            return;
        };
        list.select_previous(MAX_VISIBLE_ROWS, |_| true);
        ctx.emit(TuiApiKeysMenuEvent);
    }

    pub(crate) fn select_next(&mut self, ctx: &mut ModelContext<Self>) {
        let TuiApiKeysMenuState::List { list } = &mut self.state else {
            return;
        };
        list.select_next(MAX_VISIBLE_ROWS, |_| true);
        ctx.emit(TuiApiKeysMenuEvent);
    }

    /// Selects the row at absolute snapshot index `index` (for mouse click).
    /// Returns `true` when the row was actually selected, `false` when the
    /// index is out of bounds or the menu is not in the `List` sub-state.
    pub(crate) fn select_at_snapshot_index(
        &mut self,
        index: usize,
        ctx: &mut ModelContext<Self>,
    ) -> bool {
        let TuiApiKeysMenuState::List { list } = &mut self.state else {
            return false;
        };
        let selected = list.select_absolute(index, MAX_VISIBLE_ROWS, |_| true);
        ctx.emit(TuiApiKeysMenuEvent);
        selected
    }

    /// Scrolls the viewport by `delta` rows without changing the selection.
    pub(crate) fn scroll_by_delta(&mut self, delta: isize, ctx: &mut ModelContext<Self>) {
        let TuiApiKeysMenuState::List { list } = &mut self.state else {
            return;
        };
        list.scroll_by(delta, MAX_VISIBLE_ROWS);
        ctx.emit(TuiApiKeysMenuEvent);
    }

    /// Accepts the current sub-state's selection:
    /// - `List` + an "Edit" row: switches to `EnteringKey` for that provider (no persistence
    ///   yet -- that happens when the typed key is itself accepted).
    /// - `List` + a "Clear" row: clears the provider's key immediately via
    ///   [`tui_clear_agent_provider_api_key`] and stays in `List`.
    /// - `EnteringKey`: persists the typed buffer via [`tui_set_agent_provider_api_key`] (a
    ///   blank/whitespace-only entry is treated as "no change", not as "clear" -- clearing has
    ///   its own explicit row so it can't happen by accident) and returns to `List`.
    pub(crate) fn accept_selected(&mut self, ctx: &mut ModelContext<Self>) {
        match std::mem::take(&mut self.state) {
            TuiApiKeysMenuState::List { list } => {
                let Some(row) = list.selected_row().cloned() else {
                    self.state = TuiApiKeysMenuState::List { list };
                    return;
                };
                match row.action {
                    TuiApiKeysRowAction::Edit(provider_id) => {
                        self.state = TuiApiKeysMenuState::EnteringKey {
                            provider_id,
                            provider_name: row.title,
                        };
                        self.input_editor
                            .update(ctx, |editor, ctx| editor.clear_buffer(ctx));
                    }
                    TuiApiKeysRowAction::Clear(provider_id) => {
                        tui_clear_agent_provider_api_key(ctx, &provider_id);
                        self.state = TuiApiKeysMenuState::List { list };
                        self.refresh_rows(ctx);
                    }
                }
            }
            TuiApiKeysMenuState::EnteringKey { provider_id, .. } => {
                let key = input_text(&self.input_editor, ctx).trim().to_owned();
                if !key.is_empty() {
                    tui_set_agent_provider_api_key(ctx, &provider_id, key);
                }
                self.state = TuiApiKeysMenuState::List {
                    list: TuiInlineMenuListState::default(),
                };
                self.input_editor
                    .update(ctx, |editor, ctx| editor.clear_buffer(ctx));
                self.refresh_rows(ctx);
            }
            TuiApiKeysMenuState::Closed => {}
        }
        ctx.emit(TuiApiKeysMenuEvent);
    }

    pub(crate) fn snapshot(&self, ctx: &AppContext) -> Option<TuiInlineMenuSnapshot> {
        if !self.is_open(ctx) {
            return None;
        }
        match &self.state {
            TuiApiKeysMenuState::List { list } => Some(TuiInlineMenuSnapshot {
                header: Some(TuiInlineMenuHeader {
                    title: Some("API keys".to_owned()),
                    tabs: Vec::new(),
                }),
                rows: list
                    .rows()
                    .iter()
                    .map(|row| TuiInlineMenuRow {
                        title: row.title.clone(),
                        prefix: None,
                        description: row.description.clone(),
                        state_suffix: row.state_suffix.clone(),
                        is_selectable: true,
                        style: TuiInlineMenuRowStyle::Default,
                    })
                    .collect(),
                selected_index: list.selected_index(),
                scroll_offset: list.scroll_offset(),
                scroll_anchor: list.scroll_anchor(),
                max_visible_rows: MAX_VISIBLE_ROWS,
                status: list.rows().is_empty().then(|| {
                    TuiInlineMenuStatus::Empty(
                        "No providers configured yet -- add one in Settings > AI > Agent providers"
                            .to_owned(),
                    )
                }),
            }),
            TuiApiKeysMenuState::EnteringKey { provider_name, .. } => Some(TuiInlineMenuSnapshot {
                header: Some(TuiInlineMenuHeader {
                    title: Some(format!("API key · {provider_name}")),
                    tabs: Vec::new(),
                }),
                rows: Vec::new(),
                selected_index: None,
                scroll_offset: 0,
                scroll_anchor: TuiInlineMenuScrollAnchor::Selection,
                max_visible_rows: MAX_VISIBLE_ROWS,
                status: Some(TuiInlineMenuStatus::Empty(
                    "Type the API key and press Enter to save, Esc to cancel".to_owned(),
                )),
            }),
            TuiApiKeysMenuState::Closed => None,
        }
    }

    fn refresh_rows(&mut self, ctx: &mut ModelContext<Self>) {
        if !self.is_list_open(ctx) {
            return;
        }
        let query = input_text(&self.input_editor, ctx);
        let rows = build_provider_rows(tui_list_agent_provider_keys(ctx), &query);
        let preferred_index = (!rows.is_empty()).then_some(0);
        let TuiApiKeysMenuState::List { list } = &mut self.state else {
            return;
        };
        list.replace_rows(rows, false, preferred_index, MAX_VISIBLE_ROWS, |_| true);
        ctx.emit(TuiApiKeysMenuEvent);
    }
}

/// Builds the `List` sub-state's rows from the configured providers, filtered by `query`
/// (case-insensitive substring match against the provider's display name, matching the other
/// list menus' search behavior). Pure and `AppContext`-free so it's directly unit-testable; the
/// only caller-side state is which provider is connected, already baked into each
/// `TuiApiKeyProvider`.
///
/// Each keyed provider gets a second, immediately-following "Clear key" row -- the same
/// two-rows-per-item shape `crate::mcp_menu::refresh_rows` uses for its "Log out" action.
fn build_provider_rows(providers: Vec<TuiApiKeyProvider>, query: &str) -> Vec<TuiApiKeysMenuRow> {
    let query = query.trim().to_lowercase();
    let mut rows = Vec::new();
    for provider in providers {
        if !query.is_empty() && !provider.display_name.to_lowercase().contains(&query) {
            continue;
        }
        let state_suffix = Some(if provider.has_key {
            "(key connected)".to_owned()
        } else {
            "(no key)".to_owned()
        });
        rows.push(TuiApiKeysMenuRow {
            title: provider.display_name.clone(),
            description: Some(provider.api_type_label.to_owned()),
            state_suffix,
            action: TuiApiKeysRowAction::Edit(provider.provider_id.clone()),
        });
        if provider.has_key {
            rows.push(TuiApiKeysMenuRow {
                title: format!("Clear key · {}", provider.display_name),
                description: None,
                state_suffix: None,
                action: TuiApiKeysRowAction::Clear(provider.provider_id),
            });
        }
    }
    rows
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

#[cfg(test)]
fn test_provider_row(name: &str, has_key: bool) -> TuiApiKeysMenuRow {
    TuiApiKeysMenuRow {
        title: name.to_owned(),
        description: Some("OpenAI".to_owned()),
        state_suffix: Some(if has_key {
            "(key connected)".to_owned()
        } else {
            "(no key)".to_owned()
        }),
        action: TuiApiKeysRowAction::Edit(name.to_owned()),
    }
}

impl Entity for TuiApiKeysMenuModel {
    type Event = TuiApiKeysMenuEvent;
}

#[cfg(test)]
#[path = "api_keys_menu_tests.rs"]
mod tests;
