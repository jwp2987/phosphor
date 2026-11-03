use super::{GridSnapshotCache, GridSnapshotKey};
use crate::terminal::block_list_element::GridType;
use crate::terminal::model::ansi;
use crate::terminal::model::grid::Dimensions as _;
use crate::terminal::model::terminal_model::BlockIndex;
use crate::test_util::mock_blockgrid;

fn key(index: usize) -> GridSnapshotKey {
    GridSnapshotKey {
        block_index: BlockIndex(index),
        grid: GridType::Output,
    }
}

/// A finished grid is captured once and the same snapshot is handed out frame after frame; a
/// grid that was written to is recaptured.
#[test]
fn cache_reuses_a_snapshot_until_the_grid_changes() {
    let mut blockgrid = mock_blockgrid("one\r\ntwo\r\nthree");
    let rows = 0..blockgrid.grid_handler().total_rows();
    let mut cache = GridSnapshotCache::default();

    let first = cache
        .get_or_capture(key(0), blockgrid.grid_handler(), rows.clone())
        .expect("plain grid");
    cache.begin_frame();
    let second = cache
        .get_or_capture(key(0), blockgrid.grid_handler(), rows.clone())
        .expect("plain grid");
    assert!(std::rc::Rc::ptr_eq(&first, &second));

    ansi::Handler::input(blockgrid.grid_handler_mut(), 'x');
    cache.begin_frame();
    let third = cache
        .get_or_capture(key(0), blockgrid.grid_handler(), rows.clone())
        .expect("plain grid");
    assert!(!std::rc::Rc::ptr_eq(&first, &third));
    assert_eq!(
        third.content_generation(),
        blockgrid.grid_handler().content_generation()
    );

    // A different visible range is a different snapshot.
    let partial = cache
        .get_or_capture(key(0), blockgrid.grid_handler(), 1..2)
        .expect("plain grid");
    assert_eq!(partial.rows(), 1..2);
}

/// Snapshots that no paint used since the last `begin_frame` are dropped.
#[test]
fn cache_forgets_snapshots_no_paint_used() {
    let blockgrid = mock_blockgrid("a\r\nb");
    let rows = 0..blockgrid.grid_handler().total_rows();
    let mut cache = GridSnapshotCache::default();
    cache.get_or_capture(key(0), blockgrid.grid_handler(), rows.clone());
    cache.get_or_capture(key(1), blockgrid.grid_handler(), rows.clone());
    assert_eq!(cache.len(), 2);

    cache.begin_frame();
    cache.get_or_capture(key(1), blockgrid.grid_handler(), rows);
    cache.begin_frame();
    assert_eq!(cache.len(), 1);
    cache.begin_frame();
    assert_eq!(cache.len(), 0);
}
