use crate::presenter::PositionCache;
use pathfinder_geometry::rect::RectF;
use pathfinder_geometry::vector::Vector2F;

use std::{cell::Cell, rc::Rc};

use crate::{
    AfterLayoutContext, App, AppContext, Element, Entity, EventContext, LayoutContext,
    PaintContext, SizeConstraint, TypedActionView, elements::Point, event::DispatchedEvent,
    platform::WindowStyle,
};

#[test]
fn test_position_cache_caching() {
    let mut position_cache = PositionCache::new();
    position_cache.start();

    position_cache.cache_position_indefinitely(
        "position_1".to_string(),
        RectF::new(Vector2F::zero(), Vector2F::new(100.0, 100.0)),
    );
    position_cache.cache_position_for_one_frame(
        "position_2".to_string(),
        RectF::new(Vector2F::zero(), Vector2F::new(50.0, 50.0)),
    );

    position_cache.start();
    position_cache.cache_position_indefinitely(
        "position_1".to_string(),
        RectF::new(Vector2F::zero(), Vector2F::new(25.0, 25.0)),
    );
    position_cache.cache_position_indefinitely(
        "position_2".to_string(),
        RectF::new(Vector2F::zero(), Vector2F::new(10.0, 10.0)),
    );
    position_cache.cache_position_for_one_frame(
        "position_3".to_string(),
        RectF::new(Vector2F::zero(), Vector2F::new(5.0, 5.0)),
    );
    assert_eq!(position_cache.get_position("position_1"), None);

    position_cache.end();
    assert_eq!(
        position_cache.get_position("position_1"),
        Some(RectF::new(Vector2F::zero(), Vector2F::new(25.0, 25.0)))
    );
    assert_eq!(
        position_cache.get_position("position_2"),
        Some(RectF::new(Vector2F::zero(), Vector2F::new(10.0, 10.0)))
    );
    assert_eq!(
        position_cache.get_position("position_3"),
        Some(RectF::new(Vector2F::zero(), Vector2F::new(5.0, 5.0)))
    );

    position_cache.end();
    assert_eq!(
        position_cache.get_position("position_1"),
        Some(RectF::new(Vector2F::zero(), Vector2F::new(100.0, 100.0)))
    );
    assert_eq!(
        position_cache.get_position("position_2"),
        Some(RectF::new(Vector2F::zero(), Vector2F::new(50.0, 50.0)))
    );
    assert_eq!(
        position_cache.get_position("position_3"),
        Some(RectF::new(Vector2F::zero(), Vector2F::new(5.0, 5.0)))
    );

    position_cache.clear_single_frame_positions();
    assert_eq!(
        position_cache.get_position("position_1"),
        Some(RectF::new(Vector2F::zero(), Vector2F::new(100.0, 100.0)))
    );
    assert_eq!(
        position_cache.get_position("position_2"),
        Some(RectF::new(Vector2F::zero(), Vector2F::new(50.0, 50.0)))
    );
    assert_eq!(position_cache.get_position("position_3"), None);

    position_cache.clear_position("position_1");
    assert_eq!(position_cache.get_position("position_1"), None);
}

/// Shared layout/paint call counters for [`CountingElement`], read back by tests.
#[derive(Default)]
struct RepaintCounts {
    layout_calls: Cell<u32>,
    paint_calls: Cell<u32>,
}

/// A leaf element that records every `layout`/`paint` call it receives, so a test
/// can assert whether a given `Presenter::build_scene_skip_layout_if` call re-ran
/// layout or only paint. See issue #703.
struct CountingElement {
    counts: Rc<RepaintCounts>,
    size: Option<Vector2F>,
    origin: Option<Point>,
}

impl Element for CountingElement {
    fn layout(
        &mut self,
        constraint: SizeConstraint,
        _ctx: &mut LayoutContext,
        _app: &AppContext,
    ) -> Vector2F {
        self.counts
            .layout_calls
            .set(self.counts.layout_calls.get() + 1);
        let size = constraint.max;
        self.size = Some(size);
        size
    }

    fn after_layout(&mut self, _ctx: &mut AfterLayoutContext, _app: &AppContext) {}

    fn paint(&mut self, origin: Vector2F, ctx: &mut PaintContext, _app: &AppContext) {
        self.counts
            .paint_calls
            .set(self.counts.paint_calls.get() + 1);
        self.origin = Some(Point::from_vec2f(origin, ctx.scene.z_index()));
    }

    fn size(&self) -> Option<Vector2F> {
        self.size
    }

    fn origin(&self) -> Option<Point> {
        self.origin
    }

    fn dispatch_event(
        &mut self,
        _event: &DispatchedEvent,
        _ctx: &mut EventContext,
        _app: &AppContext,
    ) -> bool {
        false
    }
}

#[derive(Default)]
struct CountingRootView {
    counts: Rc<RepaintCounts>,
}

impl Entity for CountingRootView {
    type Event = ();
}

impl crate::core::View for CountingRootView {
    fn render(&self, _app: &AppContext) -> Box<dyn Element> {
        Box::new(CountingElement {
            counts: self.counts.clone(),
            size: None,
            origin: None,
        })
    }

    fn ui_name() -> &'static str {
        "CountingRootView"
    }
}

impl TypedActionView for CountingRootView {
    type Action = ();
}

/// A redraw whose only cause is a paint-only timer (what
/// `PaintContext::repaint_after_paint_only`/`repaint_at_paint_only` schedules —
/// e.g. a blinking cursor, with no view notified or removed) must not re-run
/// layout: `AppContext::build_scene` computes `skip_layout = true` and passes it to
/// `Presenter::build_scene_skip_layout_if` in that case. A real invalidation (a
/// view was notified) must still re-run layout. This is the mechanism behind the
/// fix for issue #703: skipping layout on an idle focused window's blink is what
/// makes it cheap.
///
/// This drives the real `AppContext::build_scene` path end-to-end (via the
/// test-only `simulate_render_frame`/`simulate_paint_only_redraw` helpers, which
/// mirror what a real frame and a real `repaint_after_paint_only` timer firing do),
/// rather than a hand-built `Presenter`, so it exercises the same wiring a real
/// blinking cursor goes through. Counts are compared as deltas across frames
/// rather than as absolute values, since a fresh window may render an initial
/// frame or two of its own before the test's first checkpoint.
#[test]
fn test_app_build_scene_skips_layout_only_for_paint_only_redraw() {
    App::test((), |mut app| async move {
        let app = &mut app;
        let (window_id, view) =
            app.add_window(WindowStyle::NotStealFocus, |_| CountingRootView::default());
        let root_view_id = app.root_view_id(window_id).unwrap();
        let counts = view.update(app, |view, _ctx| view.counts.clone());

        // Establish a checkpoint with an ordinary frame.
        app.update(|ctx| ctx.simulate_render_frame(window_id));
        let layout_calls_baseline = counts.layout_calls.get();
        let paint_calls_baseline = counts.paint_calls.get();

        // A paint-only redraw request (what firing a `repaint_after_paint_only`
        // timer sets) with nothing else invalidated: layout must be skipped, but
        // paint must still run (so time-based paint state, like a blink flag,
        // keeps updating).
        app.update(|ctx| {
            ctx.simulate_paint_only_redraw(window_id);
            ctx.simulate_render_frame(window_id);
        });
        assert_eq!(
            counts.layout_calls.get(),
            layout_calls_baseline,
            "a paint-only redraw must not re-run layout"
        );
        assert_eq!(counts.paint_calls.get(), paint_calls_baseline + 1);

        // A real invalidation (the view was notified) must still run layout, even
        // right after a paint-only frame. Using `simulate_view_updated` (rather
        // than `ctx.notify()`, which defers through the effects queue) keeps this
        // deterministic: it lands synchronously, in the same `app.update` call as
        // the frame it should affect.
        app.update(|ctx| {
            ctx.simulate_view_updated(window_id, root_view_id);
            ctx.simulate_render_frame(window_id);
        });
        assert_eq!(
            counts.layout_calls.get(),
            layout_calls_baseline + 1,
            "a real invalidation must still run layout"
        );
        assert_eq!(counts.paint_calls.get(), paint_calls_baseline + 2);
    })
}
