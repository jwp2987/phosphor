use super::search_item::SkillSearchItem;
use crate::ai::skills::SkillManager;
use crate::search::ai_context_menu::mixer::AIContextMenuSearchableAction;
use crate::search::data_source::{Query, QueryResult};
use crate::search::mixer::{DataSourceRunErrorWrapper, SyncDataSource};
use fuzzy_match::FuzzyMatchResult;
use warpui::{AppContext, Entity, SingletonEntity};

#[cfg(not(target_family = "wasm"))]
use crate::workspace::ActiveSession;

const MAX_RESULTS: usize = 50;

pub struct SkillsDataSource;

impl SkillsDataSource {
    pub fn new() -> Self {
        Self
    }
}

impl SyncDataSource for SkillsDataSource {
    type Action = AIContextMenuSearchableAction;

    fn run_query(
        &self,
        query: &Query,
        app: &AppContext,
    ) -> Result<Vec<QueryResult<Self::Action>>, DataSourceRunErrorWrapper> {
        let query_text = &query.text;

        // Resolve skills against the active window's *execution host*, not just this
        // machine: `ActiveSession::current_working_directory_location` reports `Remote`
        // for a connected SSH tab (same host-aware plumbing
        // `SessionContext::skill_path_origin` uses for the main agent-context flow), so
        // `SkillManager::get_skills_for_working_directory` resolves that host's skills
        // instead of always falling back to the local catalog. An SSH tab whose host
        // hasn't resolved yet (`is_unresolved_remote_session`) must not fall back to
        // local skills either — `get_skills_for_working_directory` treats a bare `None`
        // working directory as local, so that case is short-circuited to an empty list
        // here rather than by forwarding `None` through.
        let skills = {
            #[cfg(not(target_family = "wasm"))]
            {
                app.windows()
                    .state()
                    .active_window
                    .map_or_else(Vec::new, |window_id| {
                        let active_session = ActiveSession::as_ref(app);
                        if active_session.is_unresolved_remote_session(window_id) {
                            Vec::new()
                        } else {
                            let cwd = active_session.current_working_directory_location(window_id);
                            SkillManager::as_ref(app)
                                .get_skills_for_working_directory(cwd.as_ref(), app)
                        }
                    })
            }
            // wasm has no `ActiveSession`/window concept; preserve the previous
            // behavior of resolving skills with no known working directory.
            #[cfg(target_family = "wasm")]
            {
                SkillManager::as_ref(app).get_skills_for_working_directory(None, app)
            }
        };

        let mut results: Vec<QueryResult<Self::Action>> = if query_text.is_empty() {
            // Zero state: show all skills with a uniform high score.
            skills
                .into_iter()
                .map(|skill| {
                    QueryResult::from(SkillSearchItem {
                        name: skill.name,
                        description: skill.description,
                        provider: skill.provider,
                        icon_override: skill.icon_override,
                        match_result: FuzzyMatchResult {
                            score: 1000,
                            matched_indices: vec![],
                        },
                    })
                })
                .collect()
        } else {
            // Fuzzy match against skill name.
            skills
                .into_iter()
                .filter_map(|skill| {
                    let match_result =
                        fuzzy_match::match_indices_case_insensitive(&skill.name, query_text)?;
                    // Skip very weak matches once the user has typed more than one character.
                    if query_text.len() > 1 && match_result.score < 10 {
                        return None;
                    }
                    Some(QueryResult::from(SkillSearchItem {
                        name: skill.name,
                        description: skill.description,
                        provider: skill.provider,
                        icon_override: skill.icon_override,
                        match_result,
                    }))
                })
                .collect()
        };

        results.sort_by_key(|r| std::cmp::Reverse(r.score()));
        results.truncate(MAX_RESULTS);

        Ok(results)
    }
}

impl Entity for SkillsDataSource {
    type Event = ();
}

#[cfg(test)]
#[cfg(not(target_family = "wasm"))]
#[path = "data_source_tests.rs"]
mod tests;
