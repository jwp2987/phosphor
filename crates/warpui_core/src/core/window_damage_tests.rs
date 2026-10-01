use pathfinder_geometry::vector::Vector2F;

use super::*;

fn rect(x: f32, y: f32, w: f32, h: f32) -> RectF {
    RectF::new(Vector2F::new(x, y), Vector2F::new(w, h))
}

/// `WindowDamage::union` must treat `Full` as absorbing: unioning it with
/// anything, in either order, yields `Full`. This is what makes a single
/// layout-affecting invalidation (a notified view, a resize, …) alongside a
/// paint-only blink repaint in the same frame always paint the whole window.
/// See issue #787.
#[test]
fn test_window_damage_union_full_dominates() {
    let region = WindowDamage::Region(rect(0., 0., 10., 10.));
    assert_eq!(WindowDamage::Full.union(region), WindowDamage::Full);
    assert_eq!(region.union(WindowDamage::Full), WindowDamage::Full);
    assert_eq!(
        WindowDamage::Full.union(WindowDamage::Full),
        WindowDamage::Full
    );
}

/// Two `Region`s union into the smallest rect covering both, never losing
/// either side's area — a blink in one split pane plus a blink in another
/// damages their combined bounding box, not just one of them.
#[test]
fn test_window_damage_union_of_two_regions_is_the_bounding_box() {
    let a = WindowDamage::Region(rect(0., 0., 10., 10.));
    let b = WindowDamage::Region(rect(5., 5., 10., 10.));
    let expected = WindowDamage::Region(rect(0., 0., 15., 15.));
    assert_eq!(a.union(b), expected);
    assert_eq!(b.union(a), expected, "union must be symmetric");
}

/// A window invalidation whose *only* reason to redraw is a paint-only
/// repaint that recorded a region reports `Region` damage, not `Full` — the
/// case the whole mechanism exists for: a lone blinking cursor.
#[test]
fn test_window_invalidation_damage_is_region_for_blink_only() {
    let mut invalidation = WindowInvalidation::default();
    invalidation.paint_only_redraw_requested = true;
    let cursor_rect = rect(100., 200., 2., 16.);
    invalidation.merge_paint_only_region(cursor_rect);

    assert_eq!(invalidation.damage(), WindowDamage::Region(cursor_rect));
}

/// `merge_paint_only_region` unions repeated calls rather than the later one
/// clobbering the earlier — two blinking cursors (e.g. split panes) in the
/// same window damage the union of both rects.
#[test]
fn test_merge_paint_only_region_unions_repeated_calls() {
    let mut invalidation = WindowInvalidation::default();
    invalidation.paint_only_redraw_requested = true;
    invalidation.merge_paint_only_region(rect(0., 0., 10., 10.));
    invalidation.merge_paint_only_region(rect(5., 5., 10., 10.));

    assert_eq!(
        invalidation.damage(),
        WindowDamage::Region(rect(0., 0., 15., 15.))
    );
}

/// Any layout-affecting reason to redraw — here, a notified view — forces
/// `Full` regardless of whatever paint-only region was also recorded for the
/// same frame: a frame with ANY non-blink invalidation must paint fully.
#[test]
fn test_window_invalidation_damage_is_full_when_a_view_was_updated() {
    let mut invalidation = WindowInvalidation::default();
    invalidation.paint_only_redraw_requested = true;
    invalidation.merge_paint_only_region(rect(0., 0., 10., 10.));
    invalidation.updated.insert(EntityId::new());

    assert_eq!(invalidation.damage(), WindowDamage::Full);
}

/// Same, for `redraw_requested` (a resize, a theme change, an asset finishing
/// loading, …) — it must never be masked by a paint-only region recorded in
/// the same frame.
#[test]
fn test_window_invalidation_damage_is_full_when_redraw_requested() {
    let mut invalidation = WindowInvalidation::default();
    invalidation.paint_only_redraw_requested = true;
    invalidation.merge_paint_only_region(rect(0., 0., 10., 10.));
    invalidation.redraw_requested = true;

    assert_eq!(invalidation.damage(), WindowDamage::Full);
}

/// A paint-only redraw that never recorded a region (the regionless
/// `repaint_after_paint_only`/`repaint_at_paint_only` path) must fall back to
/// `Full` rather than silently under-painting.
#[test]
fn test_window_invalidation_damage_falls_back_to_full_without_a_region() {
    let mut invalidation = WindowInvalidation::default();
    invalidation.paint_only_redraw_requested = true;

    assert_eq!(invalidation.damage(), WindowDamage::Full);
}

/// A fresh invalidation with nothing set at all is `Full` — the safe default,
/// e.g. for the very first frame of a window.
#[test]
fn test_window_invalidation_damage_defaults_to_full() {
    assert_eq!(WindowInvalidation::default().damage(), WindowDamage::Full);
}
