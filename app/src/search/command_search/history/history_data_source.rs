use futures_lite::future::yield_now;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::sync::Arc;
use warpui::{AppContext, SingletonEntity};

use crate::search::async_snapshot_data_source::AsyncSnapshotDataSource;
use crate::search::command_search::searcher::CommandSearchItemAction;
use crate::search::data_source::{Query, QueryResult};
use crate::search::mixer::{BoxFuture, DataSourceRunErrorWrapper};
use crate::settings::AISettings;
use crate::terminal;
use crate::terminal::model::session::SessionId;
use crate::terminal::HistoryEntry;

use super::{HistoryResultsCapped, HistoryResultsCappedNotice, HistorySearchItem};

pub(crate) struct HistorySnapshot {
    commands: Arc<[Arc<HistoryEntry>]>,
    query_text: String,
}

/// Creates an async data source for shell history commands.
#[cfg(test)]
pub fn history_data_source(
    commands: Vec<HistoryEntry>,
) -> AsyncSnapshotDataSource<HistorySnapshot, CommandSearchItemAction> {
    let commands: Arc<[Arc<HistoryEntry>]> = commands.into_iter().map(Arc::new).collect();
    history_data_source_from_shared(commands)
}

fn history_data_source_from_shared(
    commands: Arc<[Arc<HistoryEntry>]>,
) -> AsyncSnapshotDataSource<HistorySnapshot, CommandSearchItemAction> {
    AsyncSnapshotDataSource::new(
        move |query: &Query, _app: &AppContext| HistorySnapshot {
            // Historical commands are all stored as Arcs (with COW semantics and very infrequent writes),
            // so cloning the commands to pass them in to the async sort function is a negligible cost.
            commands: commands.clone(),
            query_text: query.text.clone(),
        },
        fuzzy_match_history,
    )
}

pub(crate) fn history_data_source_for_session(
    session_id: SessionId,
    history_model: &terminal::History,
    app: &AppContext,
) -> AsyncSnapshotDataSource<HistorySnapshot, CommandSearchItemAction> {
    let include_agent_commands = *AISettings::as_ref(app).include_agent_commands_in_history;
    let commands: Arc<[Arc<HistoryEntry>]> = history_model
        .commands_shared(session_id)
        .unwrap_or_default()
        .into_iter()
        .filter(|entry| include_agent_commands || !entry.is_agent_executed)
        .collect();
    history_data_source_from_shared(commands)
}

/// The most history matches one query hands to the mixer (#868).
///
/// Every match used to be returned. With a 1,000,000-line history one typed character
/// matched most of it, and the mixer then collected, sorted, wrapped in a renderer and
/// later dropped every one of those results on the main thread: 66-224 s of frozen UI
/// per keystroke. The command search panel shows about a dozen rows, so the best 1,000
/// are far more than anyone scrolls through; past that the query needs refining.
pub(crate) const MAX_HISTORY_SEARCH_RESULTS: usize = 1000;

/// The most history entries the empty query lists (#868): the newest ones.
///
/// The empty-query list is the history browser a filter atom (`history:`) opens, so it gets
/// a larger cap than a ranked query. Every entry ties on an empty query, so this costs a
/// copy of the history's tail and no matching at all.
pub(crate) const MAX_HISTORY_ZERO_STATE_RESULTS: usize = 10_000;

/// Only the first this-many bytes of a history entry are fuzzy-matched (#868).
///
/// The matcher's cost grows with the text length times the query length, so a single
/// 50 MB history line cost ~10 s per keystroke on its own. A match that starts past the
/// first 4 KiB of a command is not one a person is searching for. The full command is
/// still what gets accepted or executed; this bounds only the matching.
pub(crate) const MAX_HISTORY_MATCH_BYTES: usize = 4096;

/// The part of a history command that fuzzy matching sees (#868).
pub(crate) fn history_match_target(command: &str) -> &str {
    crate::util::truncation::prefix_on_char_boundary(command, MAX_HISTORY_MATCH_BYTES)
}

/// One candidate kept by [`TopK`]. Ordered by `key` alone; `item` is payload.
struct Ranked<K, T> {
    key: K,
    item: T,
}

impl<K: Ord, T> PartialEq for Ranked<K, T> {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl<K: Ord, T> Eq for Ranked<K, T> {}

impl<K: Ord, T> PartialOrd for Ranked<K, T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<K: Ord, T> Ord for Ranked<K, T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.key.cmp(&other.key)
    }
}

/// Keeps the `k` items with the greatest keys seen so far, in `O(k)` memory and
/// `O(log k)` work per offered item, instead of collecting everything and sorting it.
///
/// Keys must be unique for the result to be deterministic; history search makes them so
/// by pairing the score with the entry's position.
pub(crate) struct TopK<K: Ord, T> {
    k: usize,
    // A min-heap: the root is the weakest kept item, the one a better candidate evicts.
    heap: BinaryHeap<Reverse<Ranked<K, T>>>,
}

impl<K: Ord, T> TopK<K, T> {
    pub(crate) fn new(k: usize) -> Self {
        Self {
            k,
            heap: BinaryHeap::with_capacity(k.min(4096)),
        }
    }

    /// Offers one candidate. It is kept iff it ranks among the best `k` so far.
    pub(crate) fn push(&mut self, key: K, item: T) {
        if self.k == 0 {
            return;
        }
        if self.heap.len() < self.k {
            self.heap.push(Reverse(Ranked { key, item }));
            return;
        }
        if let Some(mut weakest) = self.heap.peek_mut() {
            if key > weakest.0.key {
                *weakest = Reverse(Ranked { key, item });
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.heap.len()
    }

    /// The kept items, weakest first: the same order, and the same items, as a full
    /// ascending sort by key followed by keeping the last `k`.
    pub(crate) fn into_ascending_vec(self) -> Vec<(K, T)> {
        // `into_sorted_vec` sorts ascending by `Reverse`, i.e. strongest first.
        let mut kept = self.heap.into_sorted_vec();
        kept.reverse();
        kept.into_iter()
            .map(|Reverse(ranked)| (ranked.key, ranked.item))
            .collect()
    }
}

pub(crate) fn fuzzy_match_history(
    snapshot: HistorySnapshot,
) -> BoxFuture<'static, Result<Vec<QueryResult<CommandSearchItemAction>>, DataSourceRunErrorWrapper>>
{
    Box::pin(async move {
        if snapshot.query_text.is_empty() {
            return Ok(newest_history_entries(&snapshot.commands));
        }

        // Keep only the best `MAX_HISTORY_SEARCH_RESULTS` while matching (#868), so the
        // mixer's main-thread sort, the view's renderers and the eventual drop are all
        // bounded by the cap rather than by the size of the history.
        //
        // The key is `(score, position)`. The mixer orders results with a stable ascending
        // sort on score, and the best result is the last one, so among equal scores the
        // newest entry (the highest position) ranks highest. Keeping the greatest
        // `(score, position)` pairs therefore keeps exactly the results that a full sort
        // would have put in the last `MAX_HISTORY_SEARCH_RESULTS` places, and emitting
        // them ascending gives them in that same order.
        //
        // History search does not dedupe (the mixer runs with `AllowDuplicates`, and the
        // history model already dedupes on load); neither does this.
        let mut top = TopK::new(MAX_HISTORY_SEARCH_RESULTS);
        let mut position = 0usize;
        let mut matched = 0usize;

        // History entries are cheap to match (bounded by `MAX_HISTORY_MATCH_BYTES`), so we
        // use a large chunk size to reduce yield overhead while still allowing cancellation
        // of stale queries.
        for chunk in snapshot.commands.chunks(512) {
            for entry in chunk {
                if let Some(match_result) = fuzzy_match::match_indices_case_insensitive(
                    history_match_target(entry.command.as_str()),
                    snapshot.query_text.as_str(),
                ) {
                    top.push((match_result.score, position), (entry, match_result));
                    matched += 1;
                }
                position += 1;
            }
            yield_now().await;
        }

        let kept = top.into_ascending_vec();
        let mut results = Vec::with_capacity(kept.len() + 1);
        if matched > kept.len() {
            results.push(capped_notice(HistoryResultsCapped::Best {
                shown: kept.len(),
                total: matched,
            }));
        }
        results.extend(kept.into_iter().map(|(_, (entry, match_result))| {
            HistorySearchItem {
                entry: entry.clone(),
                match_result,
            }
            .into()
        }));
        Ok(results)
    })
}

/// The empty-query list (#868): the newest `MAX_HISTORY_ZERO_STATE_RESULTS` entries, oldest
/// first, preceded by a notice row when older entries were left out.
///
/// The fuzzy matcher gives every entry `(score 0, no indices)` for an empty pattern, so a
/// full stable sort kept history order and the newest entries ended up last. Taking the
/// tail directly produces the same rows in the same order without matching anything.
fn newest_history_entries(
    commands: &[Arc<HistoryEntry>],
) -> Vec<QueryResult<CommandSearchItemAction>> {
    let start = commands
        .len()
        .saturating_sub(MAX_HISTORY_ZERO_STATE_RESULTS);
    let shown = &commands[start..];
    let mut results = Vec::with_capacity(shown.len() + 1);
    if start > 0 {
        results.push(capped_notice(HistoryResultsCapped::Newest {
            shown: shown.len(),
        }));
    }
    results.extend(shown.iter().map(|entry| {
        HistorySearchItem {
            entry: entry.clone(),
            match_result: fuzzy_match::FuzzyMatchResult::no_match(),
        }
        .into()
    }));
    results
}

fn capped_notice(capped: HistoryResultsCapped) -> QueryResult<CommandSearchItemAction> {
    HistoryResultsCappedNotice { capped }.into()
}

#[cfg(test)]
#[path = "history_data_source_tests.rs"]
mod tests;
