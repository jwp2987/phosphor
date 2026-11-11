//! `hop=grid_resize` (round 14, app test `resize_reflow_output`).
//!
//! One trace line each time a terminal view gives its model a new number of rows or columns:
//! `input_order hop=grid_resize rows=<n> cols=<n> pane=<terminal view id> t=...`. The real-app
//! harness compares the latest line with the pty's own `stty size`: the two differ when the last
//! resize of a burst never reached the pty. Numbers and an id only, never text. Free when
//! input-order logging is off; no behaviour depends on it.

use warpui::EntityId;

/// The detail of the `hop=grid_resize` line. The harness parses `rows=` and `cols=`
/// (`script/realapp/appci/cases_window_use.py`, `GRID_HOP_RE`).
fn detail(rows: usize, cols: usize, pane: EntityId) -> String {
    format!("rows={rows} cols={cols} pane={pane}")
}

/// Logs the grid size a terminal view just applied to its model.
pub(super) fn log(rows: usize, cols: usize, pane: EntityId) {
    warpui::input_order::log_hop("grid_resize", || detail(rows, cols, pane));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detail_names_rows_then_cols_then_pane() {
        let d = detail(24, 80, EntityId::new());
        let rest = d
            .strip_prefix("rows=24 cols=80 pane=")
            .unwrap_or_else(|| panic!("unexpected shape: {d}"));
        assert!(!rest.is_empty() && !rest.contains(' '), "{d}");
    }
}
