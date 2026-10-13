//! #868: ctrl-r over a large history must hand the mixer a bounded, correctly ordered
//! result set, and must not let one huge history line dominate matching.

use std::cell::Cell;
use std::sync::Arc;

use futures_lite::future::block_on;
use rand::{Rng, SeedableRng};

use super::super::history_display_text;
use super::*;
use crate::search::command_search::searcher::{AcceptedHistoryItem, CommandSearchItemAction};

/// What a full stable ascending sort by score followed by keeping the last `k` returns:
/// the ordering the mixer produced before #868, written out the slow way.
fn full_sort_then_keep_last(scores: &[i64], k: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..scores.len()).collect();
    order.sort_by_key(|&i| scores[i]); // stable: equal scores keep history order
    let skip = order.len().saturating_sub(k);
    order.split_off(skip)
}

fn top_k_positions(scores: &[i64], k: usize) -> Vec<usize> {
    let mut top = TopK::new(k);
    for (position, &score) in scores.iter().enumerate() {
        top.push((score, position), position);
        assert!(top.len() <= k, "TopK must never hold more than k items");
    }
    top.into_ascending_vec()
        .into_iter()
        .map(|(_, position)| position)
        .collect()
}

#[test]
fn top_k_equals_full_sort_then_truncate_for_random_inputs() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(868);
    for _ in 0..300 {
        let len = rng.gen_range(0..400);
        // A narrow score range makes ties the common case, which is where ordering bugs hide.
        let max_score = rng.gen_range(1..20);
        let scores: Vec<i64> = (0..len).map(|_| rng.gen_range(0..max_score)).collect();
        for k in [0, 1, 2, 7, 50, len, len + 3] {
            assert_eq!(
                top_k_positions(&scores, k),
                full_sort_then_keep_last(&scores, k),
                "k={k} scores={scores:?}"
            );
        }
    }
}

thread_local! {
    static COMPARISONS: Cell<u64> = const { Cell::new(0) };
}

/// A key whose every comparison is counted, to bound the work `TopK` does.
#[derive(PartialEq, Eq)]
struct CountedKey(i64, usize);

impl PartialOrd for CountedKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CountedKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        COMPARISONS.with(|c| c.set(c.get() + 1));
        (self.0, self.1).cmp(&(other.0, other.1))
    }
}

fn comparisons_to_keep_top_k(scores: impl Iterator<Item = i64>, k: usize) -> (u64, usize) {
    COMPARISONS.with(|c| c.set(0));
    let mut top = TopK::new(k);
    for (position, score) in scores.enumerate() {
        top.push(CountedKey(score, position), ());
    }
    let kept = top.len();
    (COMPARISONS.with(Cell::get), kept)
}

/// 1,000,000 synthetic entries: keeping the best `MAX_HISTORY_SEARCH_RESULTS` costs a
/// small constant number of comparisons per entry, nowhere near a full sort's
/// `n log2 n` (~20 per entry), and holds at most the cap.
#[test]
fn top_k_work_is_bounded_for_a_million_entries() {
    const N: usize = 1_000_000;
    let k = MAX_HISTORY_SEARCH_RESULTS;
    let full_sort_comparisons = (N as f64 * (N as f64).log2()) as u64;

    // Random scores: once the heap is full almost every entry costs one comparison.
    let mut rng = rand::rngs::StdRng::seed_from_u64(1);
    let (random_cmp, kept) = comparisons_to_keep_top_k((0..N).map(|_| rng.gen_range(0..200)), k);
    assert_eq!(kept, k);
    assert!(
        random_cmp < 3 * N as u64,
        "{random_cmp} comparisons for {N} random entries (full sort ~{full_sort_comparisons})"
    );

    // Worst case for the heap: every entry beats everything kept so far, which is what
    // the empty-query zero state looks like (equal scores, newest wins). Each entry is
    // then one comparison against the weakest plus a sift through a heap of height
    // log2(k) -- bounded by the cap, not by the history size.
    let log2_k = (k as f64).log2().ceil() as u64;
    let (ascending_cmp, kept) = comparisons_to_keep_top_k((0..N).map(|_| 0), k);
    assert_eq!(kept, k);
    assert!(
        ascending_cmp <= N as u64 * (2 * log2_k + 2),
        "{ascending_cmp} comparisons for {N} ascending entries"
    );
}

fn snapshot(commands: &[String], query: &str) -> HistorySnapshot {
    HistorySnapshot {
        commands: commands
            .iter()
            .map(|c| Arc::new(HistoryEntry::command_only(c.clone())))
            .collect(),
        query_text: query.to_owned(),
    }
}

fn accepted_command(result: &QueryResult<CommandSearchItemAction>) -> String {
    match result.accept_result() {
        CommandSearchItemAction::AcceptHistory(AcceptedHistoryItem { command, .. }) => command,
        other => panic!("history result accepted as {other:?}"),
    }
}

/// End to end through `fuzzy_match_history`: the capped output is exactly what the old
/// uncapped output became after the mixer's stable sort, restricted to the best
/// `MAX_HISTORY_SEARCH_RESULTS` -- same commands, same order.
#[test]
fn fuzzy_match_history_returns_the_best_matches_in_mixer_order() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(2);
    let words = [
        "git", "status", "cargo", "build", "ls", "-la", "grep", "rust", "u", "e",
    ];
    let commands: Vec<String> = (0..4 * MAX_HISTORY_SEARCH_RESULTS)
        .map(|i| {
            let n = rng.gen_range(1..5);
            let mut c = (0..n)
                .map(|_| words[rng.gen_range(0..words.len())])
                .collect::<Vec<_>>()
                .join(" ");
            c.push_str(&format!(" #{i}"));
            c
        })
        .collect();

    for query in ["u", "gst", "cargo b", "zzzz-no-match"] {
        let scores_and_positions: Vec<(i64, usize)> = commands
            .iter()
            .enumerate()
            .filter_map(|(i, c)| {
                fuzzy_match::match_indices_case_insensitive(c, query).map(|m| (m.score, i))
            })
            .collect();
        let scores: Vec<i64> = scores_and_positions.iter().map(|(s, _)| *s).collect();
        let expected: Vec<String> = full_sort_then_keep_last(&scores, MAX_HISTORY_SEARCH_RESULTS)
            .into_iter()
            .map(|i| commands[scores_and_positions[i].1].clone())
            .collect();

        let results = block_on(fuzzy_match_history(snapshot(&commands, query))).unwrap();
        assert!(results.len() <= MAX_HISTORY_SEARCH_RESULTS);
        let actual: Vec<String> = results.iter().map(accepted_command).collect();
        assert_eq!(actual, expected, "query {query:?}");
    }
}

/// History search did not dedupe before #868 and must not start: the history model owns
/// deduplication, and the mixer runs this source with `AllowDuplicates`.
#[test]
fn fuzzy_match_history_does_not_dedupe() {
    let commands = vec![
        "git push".to_owned(),
        "ls".to_owned(),
        "git push".to_owned(),
    ];
    let results = block_on(fuzzy_match_history(snapshot(&commands, "git"))).unwrap();
    let actual: Vec<String> = results.iter().map(accepted_command).collect();
    assert_eq!(actual, vec!["git push".to_owned(), "git push".to_owned()]);
}

/// A huge history line is matched and shown by a bounded prefix, but accepting it still
/// yields the whole command.
#[test]
fn long_entry_is_matched_on_a_prefix_and_accepted_whole() {
    let huge = format!("echo {}", "a".repeat(8 * 1024 * 1024));
    let commands = vec![huge.clone(), "echo short".to_owned()];

    let results = block_on(fuzzy_match_history(snapshot(&commands, "echo"))).unwrap();
    let accepted: Vec<String> = results.iter().map(accepted_command).collect();
    assert!(
        accepted.iter().any(|c| c == &huge),
        "the huge entry must be accepted with its full text"
    );

    // Every highlighted index falls inside the displayed prefix.
    let display = history_display_text(&huge);
    assert!(display.len() <= MAX_HISTORY_MATCH_BYTES + '…'.len_utf8());
    assert!(display.ends_with('…'));
    assert!(huge.starts_with(display.trim_end_matches('…')));
    assert_eq!(history_display_text("echo short"), "echo short");

    // The bound is real: text that only appears past the matched prefix does not match.
    let tail_only = format!("{}needle", "a".repeat(MAX_HISTORY_MATCH_BYTES));
    let results = block_on(fuzzy_match_history(snapshot(&[tail_only], "needle"))).unwrap();
    assert!(results.is_empty());
}

#[test]
fn history_match_target_is_a_char_boundary_prefix() {
    let short = "git status";
    assert_eq!(history_match_target(short), short);

    // Put a multi-byte char across the cap.
    let mut s = "a".repeat(MAX_HISTORY_MATCH_BYTES - 1);
    s.push('é');
    s.push_str("tail");
    let target = history_match_target(&s);
    assert_eq!(target.len(), MAX_HISTORY_MATCH_BYTES - 1);
    assert!(s.starts_with(target));
}
