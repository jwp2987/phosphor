use super::{MIN_PANEL_WIDTH, MIN_REMAINING_WINDOW_SIZE, panel_width_bounds};

/// On an ample window, the panel's max width should give the rest of the
/// window at least `MIN_REMAINING_WINDOW_SIZE` and the min width should stay
/// pinned at the floor.
#[test]
fn bounds_reserve_remaining_window_size_on_a_wide_window() {
    let window_width = 1600.;
    let (min, max) = panel_width_bounds(window_width);

    assert_eq!(min, MIN_PANEL_WIDTH);
    assert_eq!(max, window_width - MIN_REMAINING_WINDOW_SIZE);
}

/// On a narrow window, where `window_width - MIN_REMAINING_WINDOW_SIZE` would
/// fall below the floor, the max must not drop below the min -- otherwise the
/// resizable element would be handed an inverted (max < min) range.
#[test]
fn max_never_drops_below_min_on_a_narrow_window() {
    let window_width = MIN_PANEL_WIDTH;
    let (min, max) = panel_width_bounds(window_width);

    assert_eq!(min, MIN_PANEL_WIDTH);
    assert_eq!(max, MIN_PANEL_WIDTH);
    assert!(max >= min);
}

/// The floor itself: regression guard for the #324 report that 300px wasted
/// real estate on a smaller display. This does not re-litigate what the right
/// number is, just that nobody silently raises it back toward 300 without
/// updating this test.
#[test]
fn min_panel_width_is_not_wider_than_the_detail_sidecar_floor() {
    // `DETAIL_SIDECAR_MIN_WIDTH` (240) in `workspace/view/vertical_tabs.rs` is
    // this codebase's existing convention for the narrowest a text-and-controls
    // side panel stays usable. Keep the AI assistant panel's floor at or below
    // it rather than drifting back toward the old 300px value.
    const DETAIL_SIDECAR_MIN_WIDTH: f32 = 240.;
    assert!(MIN_PANEL_WIDTH <= DETAIL_SIDECAR_MIN_WIDTH);
}
