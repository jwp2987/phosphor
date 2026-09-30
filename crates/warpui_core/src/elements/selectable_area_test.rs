//! Regression coverage for issue #793: a click that lands inside a `SelectableArea` but does
//! not manage to start a text selection (e.g. nothing selectable is under the exact point) must
//! not silently fall through to whatever element is painted underneath -- for an agent response
//! embedded in the terminal's blocklist, that "underneath" element is the terminal's own grid
//! selection, which happily starts an unrelated selection at the same screen position. A fenced
//! code block never hits this because it embeds its own interactive editor view, which captures
//! its own clicks directly; only prose text depends on this `SelectableArea` at all.
use super::*;
use crate::{
    elements::{ConstrainedBox, DispatchEventResult, EventHandler, Rect},
    platform::WindowStyle,
    App, Entity, Presenter, TypedActionView, WindowInvalidation,
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
        // Not a `SelectableElement`, so the area's own `on_mouse_down` can never succeed.
        let non_selectable_child = ConstrainedBox::new(Rect::new().finish())
            .with_width(100.)
            .with_height(100.)
            .finish();

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
