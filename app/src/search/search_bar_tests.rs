use super::*;
use crate::search::command_palette::mixer::CommandPaletteItemAction;

type State = SearchBarState<CommandPaletteItemAction>;

/// #1194: only a search bar that opted in skips the query behind its zero state, and only while
/// the buffer is empty and no filter is visible.
#[test]
fn query_is_redundant_only_for_an_opted_in_zero_state() {
    let plain = State::new(SearchResultOrdering::TopDown);
    assert!(!plain.query_is_redundant(true));

    let opted_in = State::new(SearchResultOrdering::TopDown).skip_query_in_zero_state();
    assert!(opted_in.query_is_redundant(true));
    assert!(!opted_in.query_is_redundant(false));

    let mut filtered = State::new(SearchResultOrdering::TopDown).skip_query_in_zero_state();
    filtered.query_filter = FilterState::Visible(QueryFilter::Files);
    assert!(!filtered.query_is_redundant(true));

    let runs_on_empty = State::new(SearchResultOrdering::TopDown)
        .skip_query_in_zero_state()
        .run_query_on_buffer_empty();
    assert!(!runs_on_empty.query_is_redundant(true));
}
