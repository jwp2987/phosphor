use crate::presenter::PositionCache;
use pathfinder_geometry::rect::RectF;
use pathfinder_geometry::vector::Vector2F;

use std::{cell::Cell, rc::Rc, time::Duration};

use crate::{
    AfterLayoutContext, App, AppContext, Element, Entity, EventContext, LayoutContext,
    PaintContext, SizeConstraint, TypedActionView,
    r#async::Timer,
    elements::{ParentElement, Point, Stack},
    event::DispatchedEvent,
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

/// How long [`PaintOnlyBlinker`] waits between paint-only repaint requests.
/// Short, so the tests below don't have to wait long for it to fire for real.
const PAINT_ONLY_DELAY: Duration = Duration::from_millis(20);

/// How long [`ConditionalLayoutRepainter`] waits between its (layout-required)
/// repaint requests, when its gate is open. Much longer than
/// [`PAINT_ONLY_DELAY`] so it never becomes the nearest deadline itself in the
/// tests below — the point is to observe what happens to the *paint-only*
/// timer once a layout-required request exists alongside it, not to also wait
/// for this one.
const LAYOUT_REQUIRED_DELAY: Duration = Duration::from_millis(2_000);

/// How long a test waits for a [`PAINT_ONLY_DELAY`] timer to fire for real.
/// Generously larger (25x) to avoid flakiness on a loaded machine, matching
/// the margin other tests in this crate use for real-timer waits (e.g.
/// `elements/hoverable_test.rs` waits 1s for a 500ms delay).
const WAIT_FOR_PAINT_ONLY_TIMER: Duration = Duration::from_millis(500);

/// An element that behaves like the editor's blinking cursor
/// (`update_blink_state`): every paint, it requests a paint-only repaint (see
/// issue #703) at a fixed delay, and never asks for layout on its own.
struct PaintOnlyBlinker {
    counts: Rc<RepaintCounts>,
    delay: Duration,
    size: Option<Vector2F>,
    origin: Option<Point>,
}

impl Element for PaintOnlyBlinker {
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
        ctx.repaint_after_paint_only(self.delay);
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

/// An element like a `LiveElement`-backed counter (`elements/live.rs`): while
/// `should_repaint` is set, every paint requests a plain (layout-required)
/// repaint at a fixed delay. The gate lets a test turn this on after the
/// window's first frame, to simulate a live counter starting up alongside an
/// already-blinking cursor.
struct ConditionalLayoutRepainter {
    counts: Rc<RepaintCounts>,
    should_repaint: Rc<Cell<bool>>,
    delay: Duration,
    size: Option<Vector2F>,
    origin: Option<Point>,
}

impl Element for ConditionalLayoutRepainter {
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
        if self.should_repaint.get() {
            ctx.repaint_after(self.delay);
        }
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

/// A root view with a [`PaintOnlyBlinker`] and a [`ConditionalLayoutRepainter`]
/// as siblings, so both get laid out and painted together every real frame —
/// mirroring a window with both a blinking cursor and (potentially) a live
/// counter.
#[derive(Default)]
struct TwoRepaintersRootView {
    paint_only_counts: Rc<RepaintCounts>,
    layout_repainter_counts: Rc<RepaintCounts>,
    should_repaint: Rc<Cell<bool>>,
}

impl Entity for TwoRepaintersRootView {
    type Event = ();
}

impl crate::core::View for TwoRepaintersRootView {
    fn render(&self, _app: &AppContext) -> Box<dyn Element> {
        let mut stack = Stack::new();
        stack.add_child(Box::new(PaintOnlyBlinker {
            counts: self.paint_only_counts.clone(),
            delay: PAINT_ONLY_DELAY,
            size: None,
            origin: None,
        }));
        stack.add_child(Box::new(ConditionalLayoutRepainter {
            counts: self.layout_repainter_counts.clone(),
            should_repaint: self.should_repaint.clone(),
            delay: LAYOUT_REQUIRED_DELAY,
            size: None,
            origin: None,
        }));
        stack.finish()
    }

    fn ui_name() -> &'static str {
        "TwoRepaintersRootView"
    }
}

impl TypedActionView for TwoRepaintersRootView {
    type Action = ();
}

/// A window whose only pending repaint is paint-only (the layout-required
/// repainter's gate stays closed throughout) must not run layout when that
/// timer actually fires for real — the negative control for the upgrade
/// tested below, and the scenario the whole optimization targets (an idle
/// focused window with a blinking cursor and nothing else asking for frames).
#[test]
fn test_pending_paint_only_timer_skips_layout_when_it_fires() {
    App::test((), |mut app| async move {
        let app = &mut app;
        let (window_id, view) = app.add_window(WindowStyle::NotStealFocus, |_| {
            TwoRepaintersRootView::default()
        });
        let (paint_only_counts, layout_repainter_counts) = view.update(app, |view, _ctx| {
            (
                view.paint_only_counts.clone(),
                view.layout_repainter_counts.clone(),
            )
        });

        // `should_repaint` stays false: only the paint-only blinker is actively
        // requesting a repaint, exactly like a lone blinking cursor.
        app.update(|ctx| ctx.simulate_render_frame(window_id));
        let layout_calls_baseline = layout_repainter_counts.layout_calls.get();
        let paint_calls_baseline = paint_only_counts.paint_calls.get();

        // Let the real timer armed by `repaint_after_paint_only` fire for real,
        // and drive a real frame through `update_windows`'s eager unit-test
        // rebuild (`AppContext::add_window`'s `on_window_invalidated` callback).
        Timer::after(WAIT_FOR_PAINT_ONLY_TIMER).await;

        assert_eq!(
            layout_repainter_counts.layout_calls.get(),
            layout_calls_baseline,
            "a window whose only pending repaint is paint-only must not run layout \
             when that timer fires"
        );
        assert!(
            paint_only_counts.paint_calls.get() > paint_calls_baseline,
            "paint must still run when the paint-only timer fires"
        );
    })
}

/// A pending paint-only timer (armed by [`PaintOnlyBlinker`], like a blinking
/// cursor's sticky deadline) must be upgraded to run a full layout if a
/// layout-required repaint request (from [`ConditionalLayoutRepainter`], like a
/// `LiveElement` counter) is later covered by its earlier-or-equal deadline —
/// otherwise the pending timer fires believing `layout_required=false` (the
/// value captured when it was spawned) and silently drops the newer, stronger
/// requirement, leaving the layout-required content stale. See the issue #703
/// review follow-up that reported this.
#[test]
fn test_pending_paint_only_timer_upgrades_when_a_layout_required_repaint_joins_it() {
    App::test((), |mut app| async move {
        let app = &mut app;
        let (window_id, view) = app.add_window(WindowStyle::NotStealFocus, |_| {
            TwoRepaintersRootView::default()
        });
        let (layout_repainter_counts, should_repaint) = view.update(app, |view, _ctx| {
            (
                view.layout_repainter_counts.clone(),
                view.should_repaint.clone(),
            )
        });

        // Frame 1: only the paint-only blinker is actively requesting a repaint
        // (the gate is closed). This arms a pending timer tagged
        // layout_required=false, exactly like a lone blinking cursor.
        app.update(|ctx| ctx.simulate_render_frame(window_id));
        let layout_calls_after_frame_1 = layout_repainter_counts.layout_calls.get();

        // Open the gate: the layout-required repainter now wants a repaint too,
        // like a `LiveElement` counter starting up in a window where a cursor is
        // already blinking. Rebuild once (standing in for whatever unrelated
        // event causes the next frame) so both elements paint in the same pass
        // and the merged, layout-required request reaches
        // `manage_delayed_repaint_timers` while the *original* paint-only timer
        // — armed back in frame 1, targeting well before this synchronous
        // rebuild — is still pending.
        should_repaint.set(true);
        app.update(|ctx| ctx.simulate_render_frame(window_id));
        let layout_calls_after_frame_2 = layout_repainter_counts.layout_calls.get();
        assert!(
            layout_calls_after_frame_2 > layout_calls_after_frame_1,
            "frame 2 is a full frame (simulate_render_frame always is, absent an \
             armed paint-only redraw) and should have run layout on its own"
        );

        // Wait for the *original* timer (from frame 1) to fire for real. Without
        // the upgrade in `manage_delayed_repaint_timers`, it would still believe
        // layout_required=false and this wake would skip layout, leaving the
        // layout-required repainter's content stale.
        Timer::after(WAIT_FOR_PAINT_ONLY_TIMER).await;
        assert!(
            layout_repainter_counts.layout_calls.get() > layout_calls_after_frame_2,
            "a pending paint-only timer must be upgraded to run a full layout once a \
             layout-required repaint request is covered by its (earlier) deadline"
        );
    })
}

/// An element like [`PaintOnlyBlinker`], but using the region-aware variant —
/// mirroring what the editor's cursor blink does since issue #787: every
/// paint, it requests a paint-only repaint scoped to a fixed rect, instead of
/// the whole window.
struct RegionBlinker {
    region: RectF,
    delay: Duration,
    size: Option<Vector2F>,
    origin: Option<Point>,
}

impl Element for RegionBlinker {
    fn layout(
        &mut self,
        constraint: SizeConstraint,
        _ctx: &mut LayoutContext,
        _app: &AppContext,
    ) -> Vector2F {
        let size = constraint.max;
        self.size = Some(size);
        size
    }

    fn after_layout(&mut self, _ctx: &mut AfterLayoutContext, _app: &AppContext) {}

    fn paint(&mut self, origin: Vector2F, ctx: &mut PaintContext, _app: &AppContext) {
        self.origin = Some(Point::from_vec2f(origin, ctx.scene.z_index()));
        ctx.repaint_after_paint_only_in_region(self.delay, self.region);
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

/// A root view with two [`RegionBlinker`]s as siblings, each damaging a
/// different (overlapping) rect — like two blinking cursors in split panes of
/// the same window.
struct TwoRegionBlinkersRootView;

impl Entity for TwoRegionBlinkersRootView {
    type Event = ();
}

impl crate::core::View for TwoRegionBlinkersRootView {
    fn render(&self, _app: &AppContext) -> Box<dyn Element> {
        let mut stack = Stack::new();
        stack.add_child(Box::new(RegionBlinker {
            region: RectF::new(Vector2F::new(0., 0.), Vector2F::new(10., 10.)),
            delay: PAINT_ONLY_DELAY,
            size: None,
            origin: None,
        }));
        stack.add_child(Box::new(RegionBlinker {
            region: RectF::new(Vector2F::new(5., 5.), Vector2F::new(10., 10.)),
            delay: PAINT_ONLY_DELAY,
            size: None,
            origin: None,
        }));
        stack.finish()
    }

    fn ui_name() -> &'static str {
        "TwoRegionBlinkersRootView"
    }
}

impl TypedActionView for TwoRegionBlinkersRootView {
    type Action = ();
}

/// The editor's blink repaint (after issue #787, `RichTextElement::paint`
/// schedules it via `repaint_after_paint_only_in_region` rather than the
/// regionless `repaint_after_paint_only`) must reach `Presenter::paint`'s own
/// return value as a `Region`, not get lost or widened to the whole window.
/// Two independent region requests (e.g. two blinking cursors in split panes)
/// must union into the bounding rect of both, exactly as `WindowDamage::union`
/// describes in its own pure unit tests (`core::window_damage_tests`) — this
/// test exercises the same semantics through the real `PaintContext`/`Element`
/// plumbing instead of calling `WindowDamage` directly.
#[test]
fn test_presenter_paint_accumulates_paint_only_region_across_elements() {
    App::test((), |mut app| async move {
        let app = &mut app;
        let (window_id, _view) =
            app.add_window(WindowStyle::NotStealFocus, |_| TwoRegionBlinkersRootView);

        // Establish layout via a real frame first: calling `Presenter::paint`
        // directly below assumes the view tree has already been rendered and
        // laid out (what `AppContext::build_scene` normally does before it).
        app.update(|ctx| ctx.simulate_render_frame(window_id));

        app.update(|ctx| {
            let presenter = ctx.presenter(window_id).unwrap();
            let (_, _, repaint_needs_layout, repaint_region, _) =
                presenter
                    .borrow_mut()
                    .paint(1., Vector2F::new(800., 600.), None, ctx);

            assert!(
                !repaint_needs_layout,
                "two paint-only-with-region requests must not require layout"
            );
            assert_eq!(
                repaint_region,
                Some(RectF::new(Vector2F::new(0., 0.), Vector2F::new(15., 15.))),
                "two blink regions must union into their bounding rect"
            );
        });
    })
}

/// A root view with a regionless [`PaintOnlyBlinker`] and a [`RegionBlinker`]
/// as siblings — like one editor whose blink couldn't determine a cursor rect
/// (shouldn't happen in practice, but the fallback must be safe) alongside one
/// that could.
#[derive(Default)]
struct MixedBlinkersRootView {
    paint_only_counts: Rc<RepaintCounts>,
}

impl Entity for MixedBlinkersRootView {
    type Event = ();
}

impl crate::core::View for MixedBlinkersRootView {
    fn render(&self, _app: &AppContext) -> Box<dyn Element> {
        let mut stack = Stack::new();
        stack.add_child(Box::new(PaintOnlyBlinker {
            counts: self.paint_only_counts.clone(),
            delay: PAINT_ONLY_DELAY,
            size: None,
            origin: None,
        }));
        stack.add_child(Box::new(RegionBlinker {
            region: RectF::new(Vector2F::new(0., 0.), Vector2F::new(10., 10.)),
            delay: PAINT_ONLY_DELAY,
            size: None,
            origin: None,
        }));
        stack.finish()
    }

    fn ui_name() -> &'static str {
        "MixedBlinkersRootView"
    }
}

impl TypedActionView for MixedBlinkersRootView {
    type Action = ();
}

/// A paint-only repaint that doesn't know its own damage rect (the regionless
/// `PaintContext::repaint_after_paint_only`) must poison the region for the
/// *whole* frame, even if another element in the same window did supply one —
/// "this known rect, plus some unknown other area" isn't representable as a
/// single rect, so the only safe outcome is `None` (which
/// `WindowInvalidation::damage` then treats as `Full`, never under-painting).
/// See `PaintContext::repaint_region_unknown`.
#[test]
fn test_presenter_paint_region_is_unknown_if_any_paint_only_request_omits_it() {
    App::test((), |mut app| async move {
        let app = &mut app;
        let (window_id, _view) = app.add_window(WindowStyle::NotStealFocus, |_| {
            MixedBlinkersRootView::default()
        });

        app.update(|ctx| ctx.simulate_render_frame(window_id));

        app.update(|ctx| {
            let presenter = ctx.presenter(window_id).unwrap();
            let (_, _, repaint_needs_layout, repaint_region, _) =
                presenter
                    .borrow_mut()
                    .paint(1., Vector2F::new(800., 600.), None, ctx);

            assert!(!repaint_needs_layout);
            assert_eq!(
                repaint_region, None,
                "a regionless paint-only request must poison the region for the whole frame"
            );
        });
    })
}
