use std::collections::HashSet;

use warp_core::features::FeatureFlag;
use warpui::{AppContext, EntityId, SingletonEntity};

use crate::ai::blocklist::BlocklistAIHistoryModel;
use crate::ai::blocklist::InputConfig;
use crate::ai::blocklist::history_model::AIQueryHistory;
use crate::input_suggestions::HistoryInputSuggestion;
use crate::settings::AISettings;
use crate::suggestions::ignored_suggestions_model::{IgnoredSuggestionsModel, SuggestionType};
use crate::terminal::model::session::SessionId;

use super::History;

/// Controls which item types are included in up-arrow history results.
///
/// `pub` (not `pub(crate)`): the `warp_tui` crate constructs this directly
/// through the `app/src/tui_export/history.rs` adapter for issue #387's
/// combined prompt-and-command up-arrow menu, mirroring the pinned oracle
/// (`02b53fcd8`), where `UpArrowHistoryConfig` is also `pub` for the same
/// reason.
#[derive(Copy, Clone, Debug)]
pub struct UpArrowHistoryConfig {
    pub include_commands: bool,
    pub include_prompts: bool,
}

impl UpArrowHistoryConfig {
    /// Derives the config from the current input config.
    /// When the input is locked to a specific type, only that type is included.
    /// When unlocked (auto-detection), both types are included.
    pub fn for_input_config(input_config: &InputConfig) -> Self {
        if input_config.is_locked {
            Self {
                include_commands: input_config.is_shell(),
                include_prompts: input_config.is_ai(),
            }
        } else {
            Self {
                include_commands: true,
                include_prompts: true,
            }
        }
    }
}

fn sort_and_dedupe_suggestions<'a>(
    mut suggestions: Vec<HistoryInputSuggestion<'a>>,
    session_id: Option<SessionId>,
    all_live_session_ids: &HashSet<SessionId>,
) -> Vec<HistoryInputSuggestion<'a>> {
    suggestions.sort_by(|a, b| a.cmp(b, session_id, all_live_session_ids));

    // Deduplicate commands and AI queries separately: keep the latest occurrence for each type.
    // A flag per index rather than a `HashSet<usize>`: with a very large history this runs on
    // every Up press / keystroke, and the set was a second N-sized allocation (#874).
    let mut seen_commands: HashSet<&str> = HashSet::new();
    let mut seen_ai_queries: HashSet<&str> = HashSet::new();
    let mut keep = vec![true; suggestions.len()];
    for (idx, suggestion) in suggestions.iter().enumerate().rev() {
        let text = suggestion.normalized_text();
        if text.is_empty() {
            keep[idx] = false;
            continue;
        }
        let first_seen = if suggestion.is_ai_query() {
            seen_ai_queries.insert(text)
        } else {
            seen_commands.insert(text)
        };
        keep[idx] = first_seen;
    }

    let mut keep = keep.into_iter();
    suggestions.retain(|_| keep.next().unwrap_or(true));
    suggestions
}

impl History {
    pub(crate) fn up_arrow_suggestions_for_terminal_view<'a>(
        &'a self,
        terminal_view_id: EntityId,
        session_id: Option<SessionId>,
        config: UpArrowHistoryConfig,
        app: &'a AppContext,
    ) -> Vec<HistoryInputSuggestion<'a>> {
        self.up_arrow_suggestions_with_command_prefix(
            terminal_view_id,
            session_id,
            config,
            None,
            app,
        )
    }

    /// Like [`Self::up_arrow_suggestions_for_terminal_view`], but when `command_prefix` is
    /// given only commands whose trimmed text starts with it are considered. The prefix is
    /// applied before the sort and dedupe, so a typed prefix shrinks the O(N log N) work to
    /// the matching entries instead of running it over the whole history (#874). Because
    /// dedupe keys on the same trimmed text, the result equals filtering afterwards.
    /// AI queries are not filtered.
    pub(crate) fn up_arrow_suggestions_with_command_prefix<'a>(
        &'a self,
        terminal_view_id: EntityId,
        session_id: Option<SessionId>,
        config: UpArrowHistoryConfig,
        command_prefix: Option<&'a str>,
        app: &'a AppContext,
    ) -> Vec<HistoryInputSuggestion<'a>> {
        // Guarded, as the oracle guards them. These singletons are absent in
        // surfaces that build history without the full app graph, and the fork's
        // unguarded `handle()` turned that into a panic rather than the oracle's
        // fallback. `up_arrow_tests.rs` -- which had never been compiled -- fails
        // on exactly this.
        let ignored_suggestions = app
            .has_singleton_model::<IgnoredSuggestionsModel>()
            .then(|| IgnoredSuggestionsModel::handle(app).as_ref(app));

        let include_agent_commands = if app.has_singleton_model::<AISettings>() {
            *AISettings::handle(app)
                .as_ref(app)
                .include_agent_commands_in_history
        } else {
            true
        };

        // Lazy iteration over `visible_commands` instead of the `commands` Vec, which
        // materialised the whole history before any filter ran.
        let commands = session_id
            .and_then(|session_id| self.visible_commands(session_id))
            .into_iter()
            .flatten()
            .map(|entry| &**entry)
            .filter(move |entry| {
                command_prefix.is_none_or(|prefix| entry.command.trim().starts_with(prefix))
            })
            .filter(|entry| {
                ignored_suggestions.is_none_or(|ignored_suggestions| {
                    !ignored_suggestions.is_ignored(&entry.command, SuggestionType::ShellCommand)
                })
            })
            .filter(move |entry| include_agent_commands || !entry.is_agent_executed)
            .map(|entry| HistoryInputSuggestion::Command { entry });

        let should_include_prompts = config.include_prompts
            && if app.has_singleton_model::<AISettings>() {
                FeatureFlag::AgentMode.is_enabled()
                    && AISettings::handle(app).as_ref(app).is_any_ai_enabled(app)
            } else {
                true
            };
        let all_live_session_ids = self.all_live_session_ids();
        if !should_include_prompts {
            if !config.include_commands {
                return vec![];
            }
            return sort_and_dedupe_suggestions(
                commands.collect(),
                session_id,
                &all_live_session_ids,
            );
        }

        let ai_queries = BlocklistAIHistoryModel::handle(app)
            .as_ref(app)
            .all_ai_queries(Some(terminal_view_id))
            .filter(|query| {
                ignored_suggestions.is_none_or(|ignored_suggestions| {
                    !ignored_suggestions.is_ignored(&query.query_text, SuggestionType::AIQuery)
                })
            })
            .map(|entry| HistoryInputSuggestion::AIQuery { entry });

        let suggestions: Vec<HistoryInputSuggestion<'a>> =
            match (config.include_commands, config.include_prompts) {
                (true, true) => commands.chain(ai_queries).collect(),
                (true, false) => commands.collect(),
                (false, true) => ai_queries.collect(),
                (false, false) => vec![],
            };

        sort_and_dedupe_suggestions(suggestions, session_id, &all_live_session_ids)
    }
}

/// Returns the AI-query prompt history for a terminal view, most-recent first,
/// with ignored and empty prompts filtered out. Used by the `warp_tui`
/// prompt-history menu (which lists only prior prompts, not shell commands).
pub fn prompt_history_for_terminal_view(
    terminal_view_id: EntityId,
    app: &AppContext,
) -> Vec<AIQueryHistory> {
    let ignored_suggestions = app
        .has_singleton_model::<IgnoredSuggestionsModel>()
        .then(|| IgnoredSuggestionsModel::handle(app).as_ref(app));
    let suggestions = BlocklistAIHistoryModel::handle(app)
        .as_ref(app)
        .all_ai_queries(Some(terminal_view_id))
        .filter(|entry| !entry.query_text.trim().is_empty())
        .filter(|entry| {
            ignored_suggestions.is_none_or(|ignored_suggestions| {
                !ignored_suggestions.is_ignored(&entry.query_text, SuggestionType::AIQuery)
            })
        })
        .map(|entry| HistoryInputSuggestion::AIQuery { entry })
        .collect();
    let sorted = sort_and_dedupe_suggestions(suggestions, None, &HashSet::new());

    sorted
        .into_iter()
        .filter_map(|suggestion| match suggestion {
            HistoryInputSuggestion::AIQuery { entry } => Some(entry),
            HistoryInputSuggestion::Command { .. } => None,
        })
        .collect()
}

#[cfg(test)]
#[path = "up_arrow_tests.rs"]
mod tests;
