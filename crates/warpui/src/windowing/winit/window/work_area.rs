//! Pure rect math for keeping a new or restored window inside a monitor's work area (the monitor
//! minus the taskbar / docks). Everything here is unit-agnostic: callers pass logical rects for
//! pre-creation sizing and physical rects for post-creation correction, but never mix the two.
//!
//! Only Windows consumes this today (other window managers already constrain windows), but the
//! helpers are platform-neutral so they are tested everywhere. See #1045.
#![cfg_attr(not(windows), allow(dead_code))]

use pathfinder_geometry::rect::RectF;
use pathfinder_geometry::vector::{Vector2F, vec2f};

/// Gap, in logical pixels, kept between a freshly-sized default window and each edge of the work
/// area. Leaves room for non-client frame the inner size does not include.
pub(super) const WORK_AREA_MARGIN: f32 = 24.;

/// Converts a physical-pixel rect given as edges (left, top, right, bottom -- the shape of a Win32
/// `RECT`) into a logical rect for a monitor with the given scale factor.
pub(super) fn physical_edges_to_logical(
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    scale_factor: f64,
) -> RectF {
    let scale = if scale_factor.is_finite() && scale_factor > 0. {
        scale_factor as f32
    } else {
        1.
    };
    let origin = vec2f(left as f32, top as f32) / scale;
    let size = vec2f((right - left).max(0) as f32, (bottom - top).max(0) as f32) / scale;
    RectF::new(origin, size)
}

/// The size a window should open at: `desired` shrunk, per axis, so it fits within `work_area`
/// minus `margin` on every side. Never smaller than `min_size`, and never larger than `desired`.
pub(super) fn fit_size(
    desired: Vector2F,
    work_area: RectF,
    margin: f32,
    min_size: Vector2F,
) -> Vector2F {
    let available = work_area.size() - vec2f(2. * margin, 2. * margin);
    let limit = vec2f(
        available.x().max(min_size.x()),
        available.y().max(min_size.y()),
    );
    vec2f(desired.x().min(limit.x()), desired.y().min(limit.y()))
}

/// Rect of `size` centred in `work_area`. If `size` exceeds the work area on an axis, the window
/// is anchored to the work area's leading (left/top) edge so the caption stays reachable.
pub(super) fn center_in(size: Vector2F, work_area: RectF) -> RectF {
    let slack = work_area.size() - size;
    let origin = work_area.origin() + vec2f(slack.x().max(0.) / 2., slack.y().max(0.) / 2.);
    RectF::new(origin, size)
}

/// Clamps `rect` into `work_area`: the size is kept if it fits, otherwise shrunk to the work area
/// (but not below `min_size`); the origin is then moved the minimum distance needed to bring the
/// rect inside. If the rect still cannot fit (work area smaller than `min_size`) the leading edge
/// wins, so the top-left (caption) stays on-screen.
pub(super) fn clamp_into(rect: RectF, work_area: RectF, min_size: Vector2F) -> RectF {
    let size = fit_size(rect.size(), work_area, 0., min_size);
    let max_x = work_area.origin_x() + work_area.width() - size.x();
    let max_y = work_area.origin_y() + work_area.height() - size.y();
    let x = rect.origin_x().min(max_x).max(work_area.origin_x());
    let y = rect.origin_y().min(max_y).max(work_area.origin_y());
    RectF::new(vec2f(x, y), size)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn min_size() -> Vector2F {
        vec2f(480., 192.)
    }
    fn default_size() -> Vector2F {
        vec2f(1280., 800.)
    }

    fn rect(x: f32, y: f32, w: f32, h: f32) -> RectF {
        RectF::new(vec2f(x, y), vec2f(w, h))
    }

    #[test]
    fn default_fits_on_large_monitor_unchanged() {
        let work = rect(0., 0., 1920., 1040.);
        assert_eq!(
            fit_size(default_size(), work, WORK_AREA_MARGIN, min_size()),
            default_size()
        );
    }

    #[test]
    fn default_shrinks_on_1280x800_with_taskbar() {
        // 1280x800 display, 40px taskbar: work area is 1280x760 (the #1045 repro).
        let work = rect(0., 0., 1280., 760.);
        let size = fit_size(default_size(), work, WORK_AREA_MARGIN, min_size());
        assert_eq!(size, vec2f(1280. - 48., 760. - 48.));
        let placed = center_in(size, work);
        assert_eq!(placed.origin(), vec2f(24., 24.));
        // Entire window, including the right edge with the caption buttons, is on-screen.
        assert!(placed.max_x() <= work.max_x());
        assert!(placed.max_y() <= work.max_y());
        assert!(placed.origin_x() >= work.origin_x());
    }

    #[test]
    fn fit_size_never_below_minimum() {
        let work = rect(0., 0., 300., 100.);
        assert_eq!(
            fit_size(default_size(), work, WORK_AREA_MARGIN, min_size()),
            min_size()
        );
    }

    #[test]
    fn fit_size_never_grows_past_desired() {
        let work = rect(0., 0., 4000., 3000.);
        let small = vec2f(600., 400.);
        assert_eq!(fit_size(small, work, WORK_AREA_MARGIN, min_size()), small);
    }

    #[test]
    fn fit_size_shrinks_only_the_overflowing_axis() {
        let work = rect(0., 0., 2000., 500.);
        assert_eq!(
            fit_size(default_size(), work, 0., min_size()),
            vec2f(1280., 500.)
        );
    }

    #[test]
    fn center_in_offset_work_area() {
        // Work area starts below a top taskbar.
        let work = rect(0., 40., 1920., 1040.);
        let placed = center_in(vec2f(1280., 800.), work);
        assert_eq!(placed.origin(), vec2f(320., 160.));
    }

    #[test]
    fn center_in_negative_origin_secondary_monitor() {
        // Secondary monitor to the left of and above the primary.
        let work = rect(-1920., -200., 1920., 1040.);
        let placed = center_in(vec2f(1000., 600.), work);
        assert_eq!(placed.origin(), vec2f(-1920. + 460., -200. + 220.));
        assert!(placed.origin_x() >= work.origin_x() && placed.max_x() <= work.max_x());
    }

    #[test]
    fn center_in_oversized_anchors_to_leading_edge() {
        let work = rect(0., 0., 400., 300.);
        let placed = center_in(vec2f(800., 600.), work);
        assert_eq!(placed.origin(), vec2f(0., 0.));
    }

    #[test]
    fn clamp_keeps_fitting_rect_untouched() {
        let work = rect(0., 0., 1920., 1040.);
        let r = rect(100., 80., 1000., 700.);
        assert_eq!(clamp_into(r, work, min_size()), r);
    }

    #[test]
    fn clamp_moves_cascaded_rect_back_without_resizing() {
        // The #1045 symptom: 1280-wide window cascaded 156px right on a 1920 monitor still fits
        // after moving left; size must be preserved.
        let work = rect(0., 0., 1920., 1040.);
        let r = rect(900., 500., 1280., 800.);
        let c = clamp_into(r, work, min_size());
        assert_eq!(c.size(), r.size());
        assert_eq!(c.origin(), vec2f(640., 240.));
    }

    #[test]
    fn clamp_shrinks_rect_larger_than_work_area() {
        let work = rect(0., 0., 1280., 760.);
        let r = rect(156., 0., 1280., 800.);
        let c = clamp_into(r, work, min_size());
        assert_eq!(c, work);
    }

    #[test]
    fn clamp_negative_origin_secondary_monitor() {
        let work = rect(-1920., 0., 1920., 1040.);
        // Restored geometry was saved when the monitor sat elsewhere; now fully off to the right.
        let r = rect(500., 100., 800., 600.);
        let c = clamp_into(r, work, min_size());
        assert_eq!(c.origin(), vec2f(-800., 100.));
        assert_eq!(c.size(), r.size());
        // And one hanging off the left edge.
        let r = rect(-2500., -50., 800., 600.);
        let c = clamp_into(r, work, min_size());
        assert_eq!(c.origin(), vec2f(-1920., 0.));
    }

    #[test]
    fn clamp_pulls_caption_on_screen_when_above_work_area() {
        let work = rect(0., 40., 1920., 1000.);
        let r = rect(100., -30., 800., 600.);
        assert_eq!(clamp_into(r, work, min_size()).origin(), vec2f(100., 40.));
    }

    #[test]
    fn clamp_work_area_smaller_than_minimum_anchors_top_left() {
        let work = rect(10., 20., 300., 100.);
        let c = clamp_into(rect(200., 90., 800., 600.), work, min_size());
        assert_eq!(c.size(), min_size());
        assert_eq!(c.origin(), vec2f(10., 20.));
    }

    #[test]
    fn physical_to_logical_at_100_percent() {
        assert_eq!(
            physical_edges_to_logical(0, 0, 1280, 760, 1.0),
            rect(0., 0., 1280., 760.)
        );
    }

    #[test]
    fn physical_to_logical_at_150_percent() {
        // 1920x1080 physical at 150% is 1280x720 logical; taskbar 60 physical = 40 logical.
        let work = physical_edges_to_logical(0, 0, 1920, 1020, 1.5);
        assert_eq!(work, rect(0., 0., 1280., 680.));
        let size = fit_size(default_size(), work, WORK_AREA_MARGIN, min_size());
        assert_eq!(size, vec2f(1232., 632.));
    }

    #[test]
    fn physical_to_logical_negative_origin_150_percent() {
        let work = physical_edges_to_logical(-3000, -150, 0, 1050, 1.5);
        assert_eq!(work, rect(-2000., -100., 2000., 800.));
    }

    #[test]
    fn physical_to_logical_rejects_bad_scale() {
        assert_eq!(
            physical_edges_to_logical(0, 0, 100, 50, 0.0),
            rect(0., 0., 100., 50.)
        );
        assert_eq!(
            physical_edges_to_logical(0, 0, 100, 50, f64::NAN),
            rect(0., 0., 100., 50.)
        );
        // Inverted edges produce an empty, not negative, size.
        assert_eq!(
            physical_edges_to_logical(10, 10, 0, 0, 1.0).size(),
            vec2f(0., 0.)
        );
    }
}
