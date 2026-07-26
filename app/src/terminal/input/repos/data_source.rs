//! Async data source for the inline repos menu.
//!
//! Historically, this pulled the list of "previously opened git repositories" from
//! `PersistedWorkspace`. Now that LSP + workspace history have been retired, that
//! candidate source no longer exists, so this data source only keeps the trait and
//! view wiring, always returning an empty result — meaning the menu can still be
//! invoked but will never have candidates. This avoids a large rewrite of the
//! upstream view / suggestions-mode wiring, so a data source can be reattached
//! later if "current pane group's live cwd" support is added.

use warpui::{AppContext, Entity};

use crate::search::data_source::{Query, QueryResult};
use crate::search::mixer::{AsyncDataSource, BoxFuture, DataSourceRunErrorWrapper};
use crate::terminal::input::repos::AcceptRepo;

pub struct RepoMenuDataSource;

impl RepoMenuDataSource {
    pub fn new() -> Self {
        Self
    }
}

impl AsyncDataSource for RepoMenuDataSource {
    type Action = AcceptRepo;

    fn run_query(
        &self,
        _query: &Query,
        _app: &AppContext,
    ) -> BoxFuture<'static, Result<Vec<QueryResult<Self::Action>>, DataSourceRunErrorWrapper>> {
        Box::pin(async move { Ok(Vec::new()) })
    }
}

impl Entity for RepoMenuDataSource {
    type Event = ();
}
