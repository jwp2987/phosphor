//! Regression coverage for issue #793: a click that lands inside a `SelectableArea` but does
//! not manage to start a text selection (e.g. nothing selectable is under the exact point) must
//! not silently fall through to whatever element is painted underneath -- for an agent response
//! embedded in the terminal's blocklist, that "underneath" element is the terminal's own grid
//! selection, which happily starts an unrelated selection at the same screen position. A fenced
//! code block never hits this because it embeds its own interactive editor view, which captures
//! its own clicks directly; only prose text depends on this `SelectableArea` at all.
use super::*;
use crate::{
    App, Entity, Presenter, TypedActionView, WindowInvalidation,
    elements::{DispatchEventResult, EventHandler, Rect},
    platform::WindowStyle,
};
use std::{cell::RefCell, collections::HashSet, rc::Rc};

/// A minimal scene: a `SelectableArea` (standing in for an agent response's local selection
/// area) wrapped by an outer click-recording `EventHandler` (standing in for whatever sits
/// beneath/beyond it in the propagation chain -- for a real agent response, the terminal's own
/// grid selection). The area's child is deliberately *not* a `SelectableElement`, so its own
/// `on_mouse_down` can never start a selection -- exactly the situation where a stale or missing
/// selectable descendant must not turn into a leaked click. Wrapping (rather than stacking
/// siblings) sidesteps this framework's z-index occlusion checks, which are orthogonal to the
/// propagation semantics this test cares about: the outer `EventHandler`'s own click handler
/// only runs when `SelectableArea::dispatch_event` returns `false` for the same event.
struct Scene {
    capture_clicks_within_bounds: bool,
    bottom_clicks: Rc<RefCell<usize>>,
    selection_handle: SelectionHandle,
}

impl Entity for Scene {
    type Event = ();
}

impl crate::core::View for Scene {
    fn ui_name() -> &'static str {
        "selectable_area_test_scene"
    }

    fn render(&self, _: &AppContext) -> Box<dyn Element> {
        // A bare `Rect` fills the incoming constraint (100x100, from the window built below)
        // on its own, so it doesn't need a `ConstrainedBox` wrapper to get a real size -- which
        // matters here because `ConstrainedBox::as_selectable_element` unconditionally returns
        // `Some(self)` (it delegates to its child the same way `Container`/`Flex` do), so wrapping
        // this `Rect` in one would have made `self.child.as_selectable_element()` succeed in
        // `SelectableArea::on_mouse_down` regardless of what's inside. `Rect` itself has no
        // `as_selectable_element` override (it uses the trait's default `None`), so it's the
        // only way to make `on_mouse_down`'s early `as_selectable_element` check -- not just
        // `expand_selection` further down -- actually fail, which is what this test needs.
        let non_selectable_child = Rect::new().finish();

        let mut area = SelectableArea::new(
            self.selection_handle.clone(),
            |_, _, _| {},
            non_selectable_child,
        );
        if self.capture_clicks_within_bounds {
            area = area.capture_clicks_within_bounds();
        }

        let bottom_clicks = self.bottom_clicks.clone();
        EventHandler::new(area.finish())
            .on_left_mouse_down(move |_, _, _| {
                *bottom_clicks.borrow_mut() += 1;
                DispatchEventResult::StopPropagation
            })
            .finish()
    }
}

impl TypedActionView for Scene {
    type Action = ();
}

/// Builds a one-window scene, dispatches a single left-mouse-down at its center, and asserts how
/// many times the element standing in for "the terminal beneath" recorded a click.
fn assert_center_click_reaches_element_beneath(
    capture_clicks_within_bounds: bool,
    expected_bottom_clicks: usize,
) {
    App::test((), |mut app| async move {
        let app = &mut app;
        let bottom_clicks = Rc::new(RefCell::new(0usize));
        let bottom_clicks_for_scene = bottom_clicks.clone();
        let selection_handle = SelectionHandle::default();

        let (window_id, _view) = app.add_window(WindowStyle::NotStealFocus, move |_| Scene {
            capture_clicks_within_bounds,
            bottom_clicks: bottom_clicks_for_scene,
            selection_handle,
        });

        let mut presenter = Presenter::new(window_id);
        let mut updated = HashSet::new();
        updated.insert(app.root_view_id(window_id).unwrap());
        let invalidation = WindowInvalidation {
            updated,
            ..Default::default()
        };

        app.update(move |ctx| {
            presenter.invalidate(invalidation, ctx);
            presenter.build_scene(vec2f(100., 100.), 1., None, ctx);
            let presenter = Rc::new(RefCell::new(presenter));

            ctx.simulate_window_event(
                Event::LeftMouseDown {
                    position: vec2f(50., 50.),
                    modifiers: Default::default(),
                    click_count: 1,
                    is_first_mouse: false,
                },
                window_id,
                presenter,
            );
        });

        assert_eq!(expected_bottom_clicks, *bottom_clicks.borrow());
    });
}

#[test]
fn click_with_nothing_selectable_leaks_to_element_beneath_by_default() {
    // Documents the #793 bug: a click over a SelectableArea with nothing selectable under it
    // currently falls through to the element beneath it.
    assert_center_click_reaches_element_beneath(false, 1);
}

#[test]
fn capture_clicks_within_bounds_stops_the_click_from_leaking_through() {
    // With capture_clicks_within_bounds set, a click inside the area's own bounds must never
    // reach the element painted beneath it.
    assert_center_click_reaches_element_beneath(true, 0);
}

/// A bare mouse-down (`is_selecting`, head == tail) or a head with no tail is not a selection,
/// so the answer output cap is not lifted by a plain click (#872).
#[test]
fn has_selection_ignores_bare_clicks_and_headless_tails() {
    let handle = SelectionHandle::default();
    assert!(!handle.has_selection());

    let point = SelectionBound::Relative(Vector2F::new(3., 4.));
    {
        let mut s = handle.selection.lock().unwrap();
        s.is_selecting = true;
        s.head = Some(point);
        s.tail = Some(point);
    }
    assert!(!handle.has_selection());

    {
        let mut s = handle.selection.lock().unwrap();
        s.is_selecting = false;
        s.tail = None;
    }
    assert!(!handle.has_selection());

    {
        let mut s = handle.selection.lock().unwrap();
        s.tail = Some(SelectionBound::Relative(Vector2F::new(30., 4.)));
    }
    assert!(handle.has_selection());
}

fn area_at(origin: Vector2F, size: Vector2F) -> SelectableArea {
    let mut area = SelectableArea::new(
        SelectionHandle::default(),
        |_, _, _| {},
        Rect::new().finish(),
    );
    area.origin = Some(Point::from_vec2f(origin, crate::scene::ZIndex::new(0)));
    area.size = Some(size);
    area
}

/// #1021: a drag begun a hair left of an agent bubble's first glyph landed in the frame
/// padding, outside every area, and started a terminal selection instead. A capturing area
/// with slop takes such a press and clamps its start point to the nearest in-bounds point.
#[test]
fn capture_slop_takes_presses_just_outside_the_bounds_and_clamps_them() {
    let area =
        area_at(vec2f(920.4, 590.), vec2f(300., 20.)).capture_clicks_within_bounds_with_slop(6.);
    // 0.4px left of the origin: outside the bounds, inside the slop.
    assert!(!area.contains_point_ignoring_clip(vec2f(920., 600.), 0.));
    assert!(area.contains_point_ignoring_clip(vec2f(920., 600.), area.capture_slop));
    assert_eq!(
        area.clamp_into_bounds(vec2f(920., 600.)),
        vec2f(920.4, 600.)
    );
    // Beyond the slop is genuinely outside.
    assert!(!area.contains_point_ignoring_clip(vec2f(900., 600.), area.capture_slop));
    // Inside is untouched.
    assert_eq!(
        area.clamp_into_bounds(vec2f(1000., 600.)),
        vec2f(1000., 600.)
    );
}

#[test]
fn plain_capture_has_no_slop() {
    let area = area_at(vec2f(920.4, 590.), vec2f(300., 20.)).capture_clicks_within_bounds();
    assert_eq!(area.capture_slop, 0.);
    assert!(!area.contains_point_ignoring_clip(vec2f(920., 600.), area.capture_slop));
}
