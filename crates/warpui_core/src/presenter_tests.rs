use crate::presenter::{ChildView, PositionCache};
use pathfinder_geometry::rect::RectF;
use pathfinder_geometry::vector::Vector2F;

use std::{cell::Cell, rc::Rc, time::Duration};

use crate::{
    AfterLayoutContext, App, AppContext, Element, Entity, EventContext, LayoutContext,
    PaintContext, SizeConstraint, TypedActionView, ViewContext, ViewHandle,
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

/// #841/#842: a frame that finds the window zero-sized (unmapped/minimized)
/// still runs `Presenter::invalidate`, which swaps the notified views' elements for
/// fresh, never-laid-out ones, and consumes the invalidation. The next frame must
/// therefore run a full layout even when its only pending reason is a paint-only
/// timer; otherwise it paints (and later dispatches events to) unlaid elements.
#[test]
fn test_zero_size_frame_owes_a_full_layout() {
    App::test((), |mut app| async move {
        let app = &mut app;
        let (window_id, view) =
            app.add_window(WindowStyle::NotStealFocus, |_| CountingRootView::default());
        let root_view_id = app.root_view_id(window_id).unwrap();
        let counts = view.update(app, |view, _ctx| view.counts.clone());

        app.update(|ctx| ctx.simulate_render_frame(window_id));
        let layout_calls_baseline = counts.layout_calls.get();

        // Window unmapped: a view is notified, but the frame bails out at zero size.
        crate::platform::test::set_forced_window_size(Some(Vector2F::zero()));
        app.update(|ctx| {
            ctx.simulate_view_updated(window_id, root_view_id);
            ctx.simulate_render_frame(window_id);
        });
        crate::platform::test::set_forced_window_size(None);
        assert_eq!(counts.layout_calls.get(), layout_calls_baseline);

        // Window mapped again; the only pending reason for a frame is a paint-only one.
        app.update(|ctx| {
            ctx.simulate_paint_only_redraw(window_id);
            ctx.simulate_render_frame(window_id);
        });
        assert_eq!(
            counts.layout_calls.get(),
            layout_calls_baseline + 1,
            "the first frame after a zero-size frame must re-run layout"
        );
    })
}

/// #842: a window whose focused view is alive but not in the element tree (a
/// dismissed context menu) cannot take keyboard input. When the window becomes
/// active again, focus must fall back to the root view; a focused view that is
/// mounted must be left alone.
#[test]
fn test_refocus_root_when_focused_view_is_unmounted() {
    App::test((), |mut app| async move {
        let app = &mut app;
        let (window_id, _root) =
            app.add_window(WindowStyle::NotStealFocus, |_| CountingRootView::default());
        let root_view_id = app.root_view_id(window_id).unwrap();
        app.update(|ctx| ctx.simulate_render_frame(window_id));

        // Focused root: nothing to recover.
        app.update(|ctx| ctx.refocus_root_if_focus_unreachable(window_id));
        assert_eq!(app.focused_view_id(window_id), Some(root_view_id));

        // A live view that no parent mounts takes focus.
        let orphan = app.add_view(window_id, |_| CountingChildView::default());
        orphan.update(app, |_, ctx| ctx.focus_self());
        app.update(|ctx| ctx.simulate_render_frame(window_id));
        assert_eq!(app.focused_view_id(window_id), Some(orphan.id()));

        app.update(|ctx| ctx.refocus_root_if_focus_unreachable(window_id));
        assert_eq!(
            app.focused_view_id(window_id),
            Some(root_view_id),
            "focus on an unmounted view must fall back to the root view"
        );
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

/// A child view that counts its own `render` calls on top of its element's
/// layout/paint counts. Stands in for a `TerminalView` in a tab. See issue #805.
#[derive(Default)]
struct CountingChildView {
    counts: Rc<RepaintCounts>,
    render_calls: Rc<Cell<u32>>,
}

impl Entity for CountingChildView {
    type Event = ();
}

impl crate::core::View for CountingChildView {
    fn render(&self, _app: &AppContext) -> Box<dyn Element> {
        self.render_calls.set(self.render_calls.get() + 1);
        Box::new(CountingElement {
            counts: self.counts.clone(),
            size: None,
            origin: None,
        })
    }

    fn ui_name() -> &'static str {
        "CountingChildView"
    }
}

impl TypedActionView for CountingChildView {
    type Action = ();
}

/// An element that lays out (and paints) its child only while `lay_out_child`
/// is set, the way a block list only lays out the items in its viewport. The
/// child is always *rendered* into the tree; it is just not visited by layout.
/// See issue #805.
struct GatedChild {
    child: Box<dyn Element>,
    lay_out_child: Rc<Cell<bool>>,
    size: Option<Vector2F>,
    origin: Option<Point>,
}

impl Element for GatedChild {
    fn layout(
        &mut self,
        constraint: SizeConstraint,
        ctx: &mut LayoutContext,
        app: &AppContext,
    ) -> Vector2F {
        let size = if self.lay_out_child.get() {
            self.child.layout(constraint, ctx, app)
        } else {
            constraint.max
        };
        self.size = Some(size);
        size
    }

    fn after_layout(&mut self, ctx: &mut AfterLayoutContext, app: &AppContext) {
        if self.lay_out_child.get() {
            self.child.after_layout(ctx, app);
        }
    }

    fn paint(&mut self, origin: Vector2F, ctx: &mut PaintContext, app: &AppContext) {
        self.origin = Some(Point::from_vec2f(origin, ctx.scene.z_index()));
        if self.lay_out_child.get() {
            self.child.paint(origin, ctx, app);
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

/// The view between the root and the child — a tab's pane group, which the
/// workspace mounts only while that tab is active. It lays the child out only
/// while `lay_out_child` is set (the way a block list lays out only the items
/// in view). See issue #805.
struct TabView {
    child: ViewHandle<CountingChildView>,
    lay_out_child: Rc<Cell<bool>>,
}

impl Entity for TabView {
    type Event = ();
}

impl crate::core::View for TabView {
    fn render(&self, _app: &AppContext) -> Box<dyn Element> {
        Box::new(GatedChild {
            child: ChildView::new(&self.child).finish(),
            lay_out_child: self.lay_out_child.clone(),
            size: None,
            origin: None,
        })
    }

    fn ui_name() -> &'static str {
        "TabView"
    }
}

impl TypedActionView for TabView {
    type Action = ();
}

/// A root that mounts its tab only while `show_child` is set (the way the
/// workspace mounts only the active tab's pane group); the tab in turn holds
/// the child, so hiding the tab hides the child's parent too, as in production
/// (workspace → pane group → pane → terminal). Its own element counts layout
/// calls, which is how the tests below count frames. See issue #805.
struct SwitchingRootView {
    counts: Rc<RepaintCounts>,
    tab: ViewHandle<TabView>,
    child: ViewHandle<CountingChildView>,
    show_child: bool,
    lay_out_child: Rc<Cell<bool>>,
}

impl SwitchingRootView {
    fn new(ctx: &mut ViewContext<Self>) -> Self {
        let child = ctx.add_view(|_| CountingChildView::default());
        let lay_out_child = Rc::new(Cell::new(true));
        let tab = ctx.add_view(|_| TabView {
            child: child.clone(),
            lay_out_child: lay_out_child.clone(),
        });
        Self {
            counts: Rc::default(),
            tab,
            child,
            show_child: true,
            lay_out_child,
        }
    }
}

impl Entity for SwitchingRootView {
    type Event = ();
}

impl crate::core::View for SwitchingRootView {
    fn render(&self, _app: &AppContext) -> Box<dyn Element> {
        let mut stack = Stack::new();
        stack.add_child(Box::new(CountingElement {
            counts: self.counts.clone(),
            size: None,
            origin: None,
        }));
        if self.show_child {
            stack.add_child(ChildView::new(&self.tab).finish());
        }
        stack.finish()
    }

    fn ui_name() -> &'static str {
        "SwitchingRootView"
    }
}

impl TypedActionView for SwitchingRootView {
    type Action = ();
}

/// Notifying a view that is not in the window's element tree, and whose parent
/// is not either (its parent stopped mounting it — a terminal in a hidden tab
/// receiving pty output), must not build a frame: nothing on screen can change.
/// The view is still re-rendered as the notification is flushed, so that when
/// the parent mounts it again the one frame that takes lays out its current
/// state, with no further render. A mounted view's notification must still
/// build a frame as before. This drives the real `ctx.notify()` → effect flush
/// → `update_windows` → eager unit-test `build_scene` path, which is where the
/// gate lives. See issue #805.
#[test]
fn test_notifying_an_unmounted_view_does_not_build_a_frame() {
    App::test((), |mut app| async move {
        let app = &mut app;
        let (window_id, root) = app.add_window(WindowStyle::NotStealFocus, SwitchingRootView::new);
        let (frames, child) =
            root.update(app, |root, _ctx| (root.counts.clone(), root.child.clone()));
        let child_renders = child.update(app, |child, _ctx| child.render_calls.clone());
        let presenter = app.presenter(window_id).unwrap();
        app.update(|ctx| ctx.simulate_render_frame(window_id));
        assert!(presenter.borrow().is_view_mounted(child.id()));

        // Mounted: a notification is a frame and a render, exactly as before.
        let frames_before = frames.layout_calls.get();
        let renders_before = child_renders.get();
        child.update(app, |_, ctx| ctx.notify());
        assert_eq!(
            frames.layout_calls.get(),
            frames_before + 1,
            "notifying a mounted view must still build a frame"
        );
        assert_eq!(child_renders.get(), renders_before + 1);

        // Hide the tab: the next layout visits neither it nor the child.
        let tab = root.update(app, |root, _| root.tab.clone());
        root.update(app, |root, ctx| {
            root.show_child = false;
            ctx.notify();
        });
        assert!(!presenter.borrow().is_view_mounted(tab.id()));
        assert!(!presenter.borrow().is_view_mounted(child.id()));

        // Unmounted: each notification re-renders the view but builds no frame,
        // and leaves no pending invalidation behind for the platform to act on.
        let frames_before = frames.layout_calls.get();
        let renders_before = child_renders.get();
        child.update(app, |_, ctx| ctx.notify());
        child.update(app, |_, ctx| ctx.notify());
        assert_eq!(
            frames.layout_calls.get(),
            frames_before,
            "notifying a view that is not in the element tree must not build a frame"
        );
        assert_eq!(
            child_renders.get(),
            renders_before + 2,
            "an unmounted view is still re-rendered when notified"
        );
        assert!(!app.has_window_invalidations(window_id));

        // Show it again: one frame, laid out from the element the last
        // notification rendered — no render of its own.
        let child_layouts_before = child.update(app, |child, _| child.counts.layout_calls.get());
        root.update(app, |root, ctx| {
            root.show_child = true;
            ctx.notify();
        });
        assert_eq!(frames.layout_calls.get(), frames_before + 1);
        assert_eq!(child_renders.get(), renders_before + 2);
        assert_eq!(
            child.update(app, |child, _| child.counts.layout_calls.get()),
            child_layouts_before + 1,
            "the re-shown child is laid out by the frame that mounts it"
        );
        assert!(presenter.borrow().is_view_mounted(child.id()));

        // And it is an ordinary mounted view again.
        let frames_before = frames.layout_calls.get();
        child.update(app, |_, ctx| ctx.notify());
        assert_eq!(frames.layout_calls.get(), frames_before + 1);
    })
}

/// A view that its parent renders but does not lay out (a block-list item
/// outside the viewport) is not in the tree either — but its *parent* is, and
/// the parent's layout may depend on the child's state (the block list
/// re-measures off-screen rich content inside its own layout). Such a
/// notification must still build a frame. See issue #805.
#[test]
fn test_notifying_an_unmounted_child_of_a_mounted_parent_still_builds_a_frame() {
    App::test((), |mut app| async move {
        let app = &mut app;
        let (window_id, root) = app.add_window(WindowStyle::NotStealFocus, SwitchingRootView::new);
        let (frames, child, lay_out_child) = root.update(app, |root, _ctx| {
            (
                root.counts.clone(),
                root.child.clone(),
                root.lay_out_child.clone(),
            )
        });
        let presenter = app.presenter(window_id).unwrap();
        app.update(|ctx| ctx.simulate_render_frame(window_id));

        // Keep rendering the child but stop laying it out; its parent, the tab,
        // stays mounted.
        let tab = root.update(app, |root, _| root.tab.clone());
        lay_out_child.set(false);
        tab.update(app, |_, ctx| ctx.notify());
        assert!(!presenter.borrow().is_view_mounted(child.id()));
        assert!(presenter.borrow().is_view_mounted(tab.id()));

        let frames_before = frames.layout_calls.get();
        child.update(app, |_, ctx| ctx.notify());
        assert_eq!(
            frames.layout_calls.get(),
            frames_before + 1,
            "a child of a mounted parent must still get its frame when notified"
        );
    })
}

/// A view the app creates is new: its creation is an invalidation and builds a
/// frame as before, even though nothing mounts it yet (a parent may do so in
/// the same frame). Once it has been rendered, though, further notifications
/// while it stays unmounted re-render it without a frame; and when a parent
/// finally mounts it, that parent's re-render is the frame, laying out the
/// element the view already has. See issue #805.
#[test]
fn test_a_new_view_builds_a_frame_once_then_none_until_mounted() {
    App::test((), |mut app| async move {
        let app = &mut app;
        let (window_id, root) = app.add_window(WindowStyle::NotStealFocus, SwitchingRootView::new);
        let frames = root.update(app, |root, _ctx| root.counts.clone());
        app.update(|ctx| ctx.simulate_render_frame(window_id));
        let presenter = app.presenter(window_id).unwrap();

        // Created, not mounted by anything: rendered once, one frame.
        let frames_before = frames.layout_calls.get();
        let new_child = app.add_view(window_id, |_| CountingChildView::default());
        let renders = new_child.update(app, |child, _ctx| child.render_calls.clone());
        assert_eq!(renders.get(), 1);
        assert_eq!(
            frames.layout_calls.get(),
            frames_before + 1,
            "a new view's creation is still a frame"
        );
        assert!(!presenter.borrow().is_view_mounted(new_child.id()));

        // Notified again while still unmounted: re-rendered, no frame.
        let frames_before = frames.layout_calls.get();
        new_child.update(app, |_, ctx| ctx.notify());
        assert_eq!(renders.get(), 2, "an unmounted view is still re-rendered");
        assert_eq!(
            frames.layout_calls.get(),
            frames_before,
            "an unmounted view that was rendered before must not build a frame"
        );
        assert!(!app.has_window_invalidations(window_id));

        // Mounted by the tab: the tab's re-render is the frame; the new view is
        // laid out from the element it already has.
        let tab = root.update(app, |root, _| root.tab.clone());
        tab.update(app, |tab, ctx| {
            tab.child = new_child.clone();
            ctx.notify();
        });
        let layouts = new_child.update(app, |child, _ctx| child.counts.layout_calls.get());
        assert_eq!(frames.layout_calls.get(), frames_before + 1);
        assert_eq!(renders.get(), 2, "mounting an already-rendered view needs no render");
        assert_eq!(layouts, 1, "and the frame that mounts it lays it out");
        assert!(presenter.borrow().is_view_mounted(new_child.id()));
    })
}
