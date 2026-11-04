use std::ops::Range;

use super::{GridSnapshot, GridSource};
use crate::terminal::model::grid::Dimensions as _;
use crate::terminal::model::grid::grid_handler::GridHandler;
use crate::test_util::mock_blockgrid;

/// Every answer the renderer can ask for, from both sources, for rows `0..=total_rows`.
fn assert_snapshot_answers_like_the_grid(grid: &GridHandler, snapshot: &GridSnapshot) {
    let rows = snapshot.rows();
    assert_eq!(snapshot.columns(), grid.columns());
    assert_eq!(snapshot.visible_rows(), grid.visible_rows());
    assert_eq!(snapshot.total_rows(), grid.total_rows());
    assert_eq!(snapshot.history_size(), grid.history_size());
    assert_eq!(
        GridSource::cursor_point(snapshot),
        GridSource::cursor_point(grid)
    );
    assert_eq!(
        GridSource::cursor_render_point(snapshot),
        GridSource::cursor_render_point(grid)
    );
    assert_eq!(
        snapshot.is_cursor_on_wide_char(),
        grid.is_cursor_on_wide_char()
    );
    assert_eq!(
        GridSource::max_content_row(snapshot),
        GridSource::max_content_row(grid)
    );
    assert_eq!(
        GridSource::marked_text(snapshot),
        GridSource::marked_text(grid)
    );
    assert_eq!(
        GridSource::has_secrets(snapshot),
        GridSource::has_secrets(grid)
    );
    assert_eq!(
        GridSource::len_displayed(snapshot),
        GridSource::len_displayed(grid)
    );
    assert_eq!(
        GridSource::filter_matches(snapshot),
        GridSource::filter_matches(grid)
    );
    assert_eq!(
        GridSource::filter_has_context_lines(snapshot),
        GridSource::filter_has_context_lines(grid)
    );
    assert_eq!(
        GridSource::displayed_output_rows(snapshot).is_none(),
        GridSource::displayed_output_rows(grid).is_none()
    );
    assert_eq!(
        GridSource::get_image_ids_in_range(snapshot, rows.start, rows.end).len(),
        GridSource::get_image_ids_in_range(grid, rows.start, rows.end).len()
    );
    for index in rows {
        assert_eq!(
            GridSource::row(snapshot, index),
            GridSource::row(grid, index),
            "row {index}"
        );
    }
}

fn snapshot_of(grid: &GridHandler, rows: Range<usize>) -> GridSnapshot {
    GridSnapshot::capture(grid, rows).expect("a plain grid can be snapshotted")
}

#[test]
fn a_snapshot_answers_exactly_like_the_live_grid() {
    let blockgrid = mock_blockgrid("hello\r\nwide: 你好\r\n\r\nlast line");
    let grid = blockgrid.grid_handler();
    let snapshot = snapshot_of(grid, 0..grid.total_rows());
    assert_snapshot_answers_like_the_grid(grid, &snapshot);
    assert_eq!(snapshot.content_generation(), grid.content_generation());
}

#[test]
fn a_finished_grid_snapshots_from_flat_storage_the_same_way() {
    let mut blockgrid = mock_blockgrid("one\r\ntwo\r\nthree\r\nfour");
    blockgrid.finish();
    let grid = blockgrid.grid_handler();
    let snapshot = snapshot_of(grid, 0..grid.total_rows());
    assert_snapshot_answers_like_the_grid(grid, &snapshot);
    assert_eq!(
        snapshot.row(0).expect("first row")[0].c,
        grid.row(0).expect("first row")[0].c
    );
}

#[test]
fn rows_outside_the_captured_range_are_absent() {
    let blockgrid = mock_blockgrid("a\r\nb\r\nc\r\nd");
    let grid = blockgrid.grid_handler();
    let snapshot = snapshot_of(grid, 1..3);
    assert_eq!(snapshot.rows(), 1..3);
    assert!(GridSource::row(&snapshot, 0).is_none());
    assert_eq!(GridSource::row(&snapshot, 1), GridSource::row(grid, 1));
    assert_eq!(GridSource::row(&snapshot, 2), GridSource::row(grid, 2));
    assert!(GridSource::row(&snapshot, 3).is_none());
    // A range running past the grid records the missing rows as missing, like the grid does.
    let past_the_end = snapshot_of(grid, 0..grid.total_rows() + 2);
    assert!(GridSource::row(&past_the_end, grid.total_rows()).is_none());
    assert!(GridSource::row(grid, grid.total_rows()).is_none());
}

#[test]
fn a_grid_with_marked_text_is_not_snapshotted() {
    let mut blockgrid = mock_blockgrid("hello");
    blockgrid.grid_handler_mut().set_marked_text("ab", &(0..2));
    let grid = blockgrid.grid_handler();
    assert!(GridSnapshot::capture(grid, 0..grid.total_rows()).is_none());
}
