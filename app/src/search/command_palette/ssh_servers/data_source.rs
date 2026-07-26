use fuzzy_match::{match_indices_case_insensitive, FuzzyMatchResult};
use itertools::Itertools;
use warpui::{AppContext, Entity};

use super::SshServerSearchItem;
use crate::search::command_palette::mixer::CommandPaletteItemAction;
use crate::search::data_source::{Query, QueryResult};
use crate::search::mixer::{DataSourceRunErrorWrapper, SyncDataSource};

use warp_ssh_manager::{NodeKind, SshRepository};

/// Upper bound. There are typically only a handful to a few dozen SSH servers, so this won't be hit.
const MAX_SSH_SERVERS_CONSIDERED: usize = 200;

#[derive(Default)]
pub struct SshServersDataSource;

impl SshServersDataSource {
    pub fn new() -> Self {
        Self
    }
}

impl Entity for SshServersDataSource {
    type Event = ();
}

impl SyncDataSource for SshServersDataSource {
    type Action = CommandPaletteItemAction;

    fn run_query(
        &self,
        query: &Query,
        _app: &AppContext,
    ) -> Result<Vec<QueryResult<Self::Action>>, DataSourceRunErrorWrapper> {
        // Uses its own with_conn (a separate write connection), so it doesn't
        // pollute PaneGroup's main write thread.
        // `DataSourceRunErrorWrapper` is a custom `Box<dyn DataSourceRunError>`
        // trait; wrapping into it is too costly here — on failure we just log
        // and return an empty result (SSH won't show in the palette, but
        // other sources are unaffected).
        let nodes = match warp_ssh_manager::with_conn(|c| Ok(SshRepository::list_nodes(c)?)) {
            Ok(n) => n,
            Err(e) => {
                log::warn!("command palette ssh: failed to load nodes: {e}");
                return Ok(Vec::new());
            }
        };

        // Only show server nodes. Fetch details for each node once, skipping
        // any that fail (folders have no details and come back as None).
        let server_nodes: Vec<_> = nodes
            .into_iter()
            .filter(|n| matches!(n.kind, NodeKind::Server))
            .take(MAX_SSH_SERVERS_CONSIDERED)
            .collect();

        let query_str = query.text.as_str();
        let results = server_nodes
            .into_iter()
            .filter_map(|node| {
                let server =
                    warp_ssh_manager::with_conn(|c| Ok(SshRepository::get_server(c, &node.id)?))
                        .ok()
                        .flatten()?;

                // Use name + " " + host as the search text, so a match on either name or host counts.
                let display_name = node.name.clone();
                let host_user = if server.username.is_empty() {
                    server.host.clone()
                } else {
                    format!("{}@{}", server.username, server.host)
                };
                let haystack = format!("{display_name} {host_user}");

                let match_result = if query_str.is_empty() {
                    Some(FuzzyMatchResult::no_match())
                } else {
                    match_indices_case_insensitive(&haystack, query_str)
                }?;

                let mut item = SshServerSearchItem::new(node, server, host_user, display_name);
                let mut mr = match_result;
                // Boost slightly, same as RepoDataSource, so ssh results stay competitive in the mixed panel.
                mr.score *= 4;
                item.match_result = mr;
                Some(item.into())
            })
            .collect_vec();

        Ok(results)
    }
}
