//! Tests for the Ctrl+Tab tab data source.
//!
//! The pin (`02b53fcd8`) ships `command_palette/tabs/` with no tests at all.
//! The MRU ordering is the part with real logic here — the data source is
//! handed tabs already in MRU order and must turn that order into scores the
//! mixer sorts by, without the query filter renumbering the survivors — so
//! that is what these cover. A future change that filtered before ranking, or
//! that inverted the score, would pass a "does it return results" test and
//! fail these.

use warpui::{App, EntityId, WindowId};

use super::DataSource;
use crate::search::SyncDataSource;
use crate::search::command_palette::mixer::CommandPaletteItemAction;
use crate::search::data_source::Query;
use crate::session_management::TabNavigationData;

fn tab(title: &str, subtitle: Option<&str>, tab_index: usize) -> TabNavigationData {
    TabNavigationData {
        pane_group_id: EntityId::new(),
        title: title.to_owned(),
        subtitle: subtitle.map(str::to_owned),
        window_id: WindowId::new(),
        tab_index,
        color: None,
    }
}

/// The tabs a workspace would hand over after activating tab 1, then 3, then 2:
/// MRU order is "second", "third", "first".
fn mru_tabs() -> Vec<TabNavigationData> {
    vec![
        tab("second", Some("~/projects/beta"), 2),
        tab("third", None, 3),
        tab("first", Some("~/projects/alpha"), 1),
    ]
}

fn accepted_pane_group_id(action: CommandPaletteItemAction) -> EntityId {
    match action {
        CommandPaletteItemAction::NavigateToTab { pane_group_id, .. } => pane_group_id,
        other => panic!("expected NavigateToTab, got {other:?}"),
    }
}

#[test]
fn empty_query_returns_every_tab_in_mru_order() {
    App::test((), |app| async move {
        let tabs = mru_tabs();
        let expected: Vec<EntityId> = tabs.iter().map(|t| t.pane_group_id).collect();

        let mut data_source = DataSource::new();
        data_source.set_tabs(tabs);

        let results = app.read(|ctx| data_source.run_query(&Query::from(""), ctx).unwrap());

        assert_eq!(results.len(), 3);
        let scores: Vec<_> = results.iter().map(|r| r.score()).collect();
        assert!(
            scores[0] > scores[1] && scores[1] > scores[2],
            "scores must descend with MRU rank so the mixer reproduces MRU order, got {scores:?}"
        );
        let ids: Vec<EntityId> = results
            .into_iter()
            .map(|r| accepted_pane_group_id(r.accept_result()))
            .collect();
        assert_eq!(ids, expected);
    })
}

#[test]
fn a_data_source_with_no_tabs_returns_nothing() {
    App::test((), |app| async move {
        let data_source = DataSource::new();
        let results = app.read(|ctx| data_source.run_query(&Query::from(""), ctx).unwrap());
        assert!(results.is_empty());
    })
}

#[test]
fn set_tabs_replaces_the_previous_snapshot() {
    App::test((), |app| async move {
        let mut data_source = DataSource::new();
        data_source.set_tabs(mru_tabs());
        data_source.set_tabs(vec![tab("only", None, 1)]);

        let results = app.read(|ctx| data_source.run_query(&Query::from(""), ctx).unwrap());

        assert_eq!(
            results.len(),
            1,
            "stale tabs must not survive a re-snapshot"
        );
    })
}

#[test]
fn query_matches_title_case_insensitively() {
    App::test((), |app| async move {
        let mut data_source = DataSource::new();
        data_source.set_tabs(mru_tabs());

        let results = app.read(|ctx| data_source.run_query(&Query::from("THIRD"), ctx).unwrap());

        assert_eq!(results.len(), 1);
    })
}

#[test]
fn query_matches_subtitle_as_well_as_title() {
    App::test((), |app| async move {
        let mut data_source = DataSource::new();
        data_source.set_tabs(mru_tabs());

        // "alpha" appears only in the third tab's subtitle.
        let results = app.read(|ctx| data_source.run_query(&Query::from("alpha"), ctx).unwrap());

        assert_eq!(results.len(), 1);
    })
}

#[test]
fn a_tab_with_no_subtitle_does_not_match_a_subtitle_query() {
    App::test((), |app| async move {
        let mut data_source = DataSource::new();
        data_source.set_tabs(vec![tab("plain", None, 1)]);

        let results = app.read(|ctx| {
            data_source
                .run_query(&Query::from("projects"), ctx)
                .unwrap()
        });

        assert!(results.is_empty());
    })
}

// This is the regression this file exists for. Filtering must not renumber the
// MRU ranks: the surviving tabs have to keep the relative order — and the
// relative *gap* — they had in the full list. Ranking after the filter would
// give the survivors ranks 0 and 1 and silently promote a stale tab.
#[test]
fn filtering_preserves_mru_rank_rather_than_renumbering_survivors() {
    App::test((), |app| async move {
        let mut data_source = DataSource::new();
        // MRU order is "match-recent" (rank 0), "noise" (rank 1),
        // "match-old" (rank 2). Only the two "match" tabs survive the query.
        data_source.set_tabs(vec![
            tab("match-recent", None, 1),
            tab("noise", None, 2),
            tab("match-old", None, 3),
        ]);

        let unfiltered = app.read(|ctx| data_source.run_query(&Query::from(""), ctx).unwrap());
        let filtered = app.read(|ctx| data_source.run_query(&Query::from("match"), ctx).unwrap());

        assert_eq!(filtered.len(), 2);
        assert_eq!(
            filtered[0].score(),
            unfiltered[0].score(),
            "the most-recent surviving tab keeps its original rank-0 score"
        );
        assert_eq!(
            filtered[1].score(),
            unfiltered[2].score(),
            "the older surviving tab keeps its original rank-2 score, not rank 1"
        );
        assert!(filtered[0].score() > filtered[1].score());
    })
}

#[test]
fn query_is_trimmed_before_matching() {
    App::test((), |app| async move {
        let mut data_source = DataSource::new();
        data_source.set_tabs(mru_tabs());

        let padded = app.read(|ctx| {
            data_source
                .run_query(&Query::from("  third  "), ctx)
                .unwrap()
        });

        assert_eq!(padded.len(), 1);
    })
}

#[test]
fn a_whitespace_only_query_is_treated_as_empty() {
    App::test((), |app| async move {
        let mut data_source = DataSource::new();
        data_source.set_tabs(mru_tabs());

        let results = app.read(|ctx| data_source.run_query(&Query::from("   "), ctx).unwrap());

        assert_eq!(results.len(), 3);
    })
}

#[test]
fn a_non_matching_query_returns_nothing() {
    App::test((), |app| async move {
        let mut data_source = DataSource::new();
        data_source.set_tabs(mru_tabs());

        let results = app.read(|ctx| {
            data_source
                .run_query(&Query::from("no-such-tab"), ctx)
                .unwrap()
        });

        assert!(results.is_empty());
    })
}
