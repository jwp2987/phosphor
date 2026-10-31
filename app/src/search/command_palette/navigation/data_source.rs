use crate::search::command_palette::mixer::CommandPaletteItemAction;
use crate::search::command_palette::navigation::search::{
    FuzzySessionSearcher, MatchedSession, SessionMatchResult, SessionSearcher,
};
use crate::search::command_palette::navigation::search_item::SearchItem;
use crate::search::data_source::{DataSourceSearchError, Query, QueryResult};
use crate::search::mixer::DataSourceRunErrorWrapper;
use crate::search::SyncDataSource;
use crate::session_management::{SessionNavigationData, SessionSource};
use crate::workspace::PaneViewLocator;
use warpui::{AppContext, Entity, ModelHandle};

/// Data source that produces possible running sessions a user could navigate to.
pub struct DataSource {
    searcher: Box<dyn SessionSearcher>,
    /// The open sessions as of the last [`Self::refresh_sessions`]. Building a
    /// [`SessionNavigationData`] renders the pane's prompt and locks its terminal model, so the
    /// palette snapshots the list when it opens (like `tabs::DataSource::set_tabs`) instead of
    /// rebuilding every pane's data on every keystroke.
    sessions: SessionSnapshot<SessionNavigationData>,
}

/// A snapshot of `T`s taken once and reused by every query until the next refresh. Until the
/// first refresh, queries fall back to reading the live items.
struct SessionSnapshot<T>(Option<Vec<T>>);

impl<T> SessionSnapshot<T> {
    fn refresh(&mut self, items: Vec<T>) {
        self.0 = Some(items);
    }

    /// Runs `f` on the snapshot, or on freshly built `live` items if there is no snapshot yet.
    fn with_items<R>(&self, live: impl FnOnce() -> Vec<T>, f: impl FnOnce(&[T]) -> R) -> R {
        match &self.0 {
            Some(items) => f(items),
            None => f(&live()),
        }
    }
}

impl DataSource {
    #[cfg(not(target_family = "wasm"))]
    pub fn new(active_session_handle: ModelHandle<SessionSource>) -> Self {
        if warp_core::features::FeatureFlag::UseTantivySearch.is_enabled() {
            Self::new_full_text(active_session_handle)
        } else {
            Self::new_fuzzy(active_session_handle)
        }
    }

    #[cfg(target_family = "wasm")]
    pub fn new(active_session_handle: ModelHandle<SessionSource>) -> Self {
        Self::new_fuzzy(active_session_handle)
    }

    #[cfg(not(target_family = "wasm"))]
    fn new_full_text(active_session_handle: ModelHandle<SessionSource>) -> Self {
        use crate::search::command_palette::navigation::search::FullTextSessionSearcher;
        let searcher = Box::new(FullTextSessionSearcher::new(active_session_handle));
        Self {
            searcher,
            sessions: SessionSnapshot(None),
        }
    }

    fn new_fuzzy(active_session_handle: ModelHandle<SessionSource>) -> Self {
        let searcher = Box::new(FuzzySessionSearcher {
            session_source_handle: active_session_handle,
        });
        Self {
            searcher,
            sessions: SessionSnapshot(None),
        }
    }

    /// Re-snapshots the open sessions. Called when the palette's [`SessionSource`] changes, i.e.
    /// when it opens.
    pub fn refresh_sessions(&mut self, app: &AppContext) {
        self.sessions
            .refresh(SessionNavigationData::all_sessions(app).collect());
    }
}

impl SyncDataSource for DataSource {
    type Action = CommandPaletteItemAction;

    fn run_query(
        &self,
        query: &Query,
        app: &AppContext,
    ) -> Result<Vec<QueryResult<Self::Action>>, DataSourceRunErrorWrapper> {
        let search_term = query.text.trim().to_lowercase();
        self.sessions
            .with_items(
                || SessionNavigationData::all_sessions(app).collect(),
                |sessions| self.searcher.search(&search_term, sessions, app),
            )
            .map_err(|err| {
                let search_error = DataSourceSearchError {
                    message: err.to_string(),
                };
                Box::new(search_error) as DataSourceRunErrorWrapper
            })
    }
}

impl DataSource {
    /// Returns a [`QueryResult`] for a workflow identified by `sync_id`. `None` if no result was
    /// found with the given ID.
    pub fn query_result(
        &self,
        pane_view_locator: PaneViewLocator,
        app: &AppContext,
    ) -> Option<QueryResult<CommandPaletteItemAction>> {
        let session = SessionNavigationData::all_sessions(app)
            .find(|session| session.pane_view_locator() == pane_view_locator)?;

        let matched_session = MatchedSession {
            session,
            match_result: SessionMatchResult::no_match(),
        };

        let active_session_id = self.searcher.active_session_id(app);

        Some(SearchItem::new(matched_session, active_session_id).into())
    }
}

impl Entity for DataSource {
    type Event = ();
}

#[cfg(test)]
#[path = "data_source_tests.rs"]
mod tests;
