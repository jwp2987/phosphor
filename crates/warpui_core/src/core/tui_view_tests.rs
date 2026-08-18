//! Tests proving that a [`TuiView`] reuses the shared application core exactly
//! as a GUI `View` does: registration + focus/blur hook dispatch, handle
//! update/read, typed-action dispatch through the shared responder chain,
//! model subscription, drop/ref-count cleanup, and TUI rendering — all running
//! additively alongside the full GUI test suite under `--features tui`.

use super::*;
use crate::elements::tui::{
    TuiConstraint, TuiElement, TuiLayoutContext, TuiPaintContext, TuiPaintSurface,
    TuiScreenPosition, TuiSize,
};
use crate::platform::WindowStyle;

/// A GUI root view hosting TUI views: under the additive design, GUI and TUI
/// views coexist in the same window registry.
#[derive(Default)]
struct RootView {
    pings: usize,
}

impl Entity for RootView {
    type Event = ();
}

impl View for RootView {
    fn ui_name() -> &'static str {
        "RootView"
    }

    fn render(&self, _: &AppContext) -> Box<dyn crate::elements::Element> {
        crate::elements::Empty::new().finish()
    }
}

#[derive(Debug)]
struct RootPing;

impl TypedActionView for RootView {
    type Action = RootPing;

    fn handle_action(&mut self, _action: &RootPing, _ctx: &mut ViewContext<Self>) {
        self.pings += 1;
    }
}

/// A minimal `TuiView` carrying a counter and focus/blur hook recorders.
#[derive(Default)]
struct CounterView {
    count: usize,
    focus_events: usize,
    blur_events: usize,
}

impl Entity for CounterView {
    type Event = ();
}

impl TuiView for CounterView {
    fn ui_name() -> &'static str {
        "CounterView"
    }

    fn render(&self, _: &AppContext) -> Box<dyn TuiElement> {
        Box::new(TuiEmpty)
    }

    fn on_focus(&mut self, focus_ctx: &FocusContext, _ctx: &mut ViewContext<Self>) {
        if focus_ctx.is_self_focused() {
            self.focus_events += 1;
        }
    }

    fn on_blur(&mut self, blur_ctx: &BlurContext, _ctx: &mut ViewContext<Self>) {
        if blur_ctx.is_self_blurred() {
            self.blur_events += 1;
        }
    }
}

#[test]
fn test_add_focus_and_hook_dispatch() {
    App::test((), |mut app| async move {
        let (window_id, root) = app.add_window(WindowStyle::NotStealFocus, |_| RootView::default());

        let tui = app.update(|ctx| ctx.add_tui_view(window_id, |_| CounterView::default()));
        let name = app.read(|ctx| ctx.view_name(window_id, tui.id()).map(str::to_owned));
        assert_eq!(name.as_deref(), Some("CounterView"));

        // Focus the TUI view: the shared focus effect must dispatch its
        // on_focus hook through the unified ViewContext.
        tui.update(&mut app, |_, ctx| ctx.focus_self());
        assert_eq!(app.focused_view_id(window_id), Some(tui.id()));
        assert!(app.read(|ctx| tui.is_focused(ctx)));
        assert_eq!(tui.read(&app, |view, _| view.focus_events), 1);
        assert_eq!(tui.read(&app, |view, _| view.blur_events), 0);

        // Refocus the GUI root: the TUI view's on_blur hook fires.
        root.update(&mut app, |_, ctx| ctx.focus_self());
        assert_eq!(app.focused_view_id(window_id), Some(root.id()));
        assert_eq!(tui.read(&app, |view, _| view.blur_events), 1);
    });
}

#[test]
fn test_update_and_read_via_handle() {
    App::test((), |mut app| async move {
        let (window_id, _root) =
            app.add_window(WindowStyle::NotStealFocus, |_| RootView::default());

        let tui = app.update(|ctx| ctx.add_tui_view(window_id, |_| CounterView::default()));

        tui.update(&mut app, |view, _| view.count = 41);
        tui.update(&mut app, |view, _| view.count += 1);

        assert_eq!(tui.read(&app, |view, _| view.count), 42);
    });
}

#[test]
fn test_render_tui_view() {
    App::test((), |mut app| async move {
        let (window_id, root) = app.add_window(WindowStyle::NotStealFocus, |_| RootView::default());

        let tui = app.update(|ctx| ctx.add_tui_view(window_id, |_| CounterView::default()));

        // The TUI view renders through the TUI path and is rejected by the GUI path.
        assert!(app.read(|ctx| ctx.render_tui_view(window_id, tui.id()).is_ok()));
        assert!(app.read(|ctx| ctx.render_view(window_id, tui.id()).is_err()));

        // And vice versa for the GUI root.
        assert!(app.read(|ctx| ctx.render_view(window_id, root.id()).is_ok()));
        assert!(app.read(|ctx| ctx.render_tui_view(window_id, root.id()).is_err()));
    });
}

#[derive(Debug)]
struct Increment(usize);

#[derive(Default)]
struct ActionView {
    total: usize,
}

impl Entity for ActionView {
    type Event = ();
}

/// An empty element, useful as a placeholder render output.
#[derive(Default)]
pub struct TuiEmpty;

impl TuiElement for TuiEmpty {
    fn layout(
        &mut self,
        _constraint: TuiConstraint,
        _ctx: &mut TuiLayoutContext,
        _app: &AppContext,
    ) -> TuiSize {
        TuiSize::ZERO
    }

    fn render(
        &mut self,
        _origin: TuiScreenPosition,
        _surface: &mut TuiPaintSurface<'_>,
        _ctx: &mut TuiPaintContext,
    ) {
    }
}

impl TuiView for ActionView {
    fn ui_name() -> &'static str {
        "ActionView"
    }

    fn render(&self, _: &AppContext) -> Box<dyn TuiElement> {
        Box::new(TuiEmpty)
    }
}

impl TypedActionView for ActionView {
    type Action = Increment;

    fn handle_action(&mut self, action: &Increment, _ctx: &mut ViewContext<Self>) {
        self.total += action.0;
    }
}

#[test]
fn test_typed_action_dispatch_through_shared_responder_chain() {
    App::test((), |mut app| async move {
        let (window_id, root) = app.add_window(WindowStyle::NotStealFocus, |_| RootView::default());

        // Create the TUI view as a structural child of the GUI root, joining
        // the shared view_parents hierarchy.
        let tui = root.update(&mut app, |_, ctx| {
            ctx.add_typed_action_tui_view(|_| ActionView::default())
        });

        // Dispatch typed actions from the TUI leaf: the responder chain is
        // derived from the shared view hierarchy and the handler registered in
        // the shared typed_actions registry runs on the TUI view.
        app.update(|ctx| ctx.dispatch_typed_action_for_view(window_id, tui.id(), &Increment(5)));
        app.update(|ctx| ctx.dispatch_typed_action_for_view(window_id, tui.id(), &Increment(3)));
        assert_eq!(tui.read(&app, |view, _| view.total), 8);

        // An action only the GUI root handles, dispatched from the TUI leaf,
        // traverses the chain through the TUI view up to the GUI parent.
        app.update(|ctx| ctx.dispatch_typed_action_for_view(window_id, tui.id(), &RootPing));
        assert_eq!(root.read(&app, |view, _| view.pings), 1);
    });
}

struct CounterModel {
    value: usize,
}

impl Entity for CounterModel {
    type Event = usize;
}

#[derive(Default)]
struct SubscriberView {
    last_seen: usize,
    model: Option<ModelHandle<CounterModel>>,
}

impl Entity for SubscriberView {
    type Event = ();
}

impl TuiView for SubscriberView {
    fn ui_name() -> &'static str {
        "SubscriberView"
    }

    fn render(&self, _: &AppContext) -> Box<dyn TuiElement> {
        Box::new(TuiEmpty)
    }
}

#[test]
fn test_model_subscription_from_tui_view() {
    App::test((), |mut app| async move {
        let (window_id, _root) =
            app.add_window(WindowStyle::NotStealFocus, |_| RootView::default());

        let tui = app.update(|ctx| {
            ctx.add_tui_view(window_id, |vctx| {
                let model = vctx.add_model(|_| CounterModel { value: 0 });
                vctx.subscribe_to_model(&model, |view: &mut SubscriberView, _handle, event, _| {
                    view.last_seen = *event;
                });
                SubscriberView {
                    last_seen: 0,
                    model: Some(model),
                }
            })
        });

        let model = tui.read(&app, |view, _| view.model.clone().unwrap());

        app.update(|ctx| {
            ctx.update_model(&model, |model, mctx| {
                model.value = 99;
                mctx.emit(model.value);
            });
        });

        assert_eq!(tui.read(&app, |view, _| view.last_seen), 99);
    });
}

#[test]
fn test_drop_removes_tui_view() {
    App::test((), |mut app| async move {
        let (window_id, _root) =
            app.add_window(WindowStyle::NotStealFocus, |_| RootView::default());

        let tui = app.update(|ctx| ctx.add_tui_view(window_id, |_| CounterView::default()));
        let view_id = tui.id();
        assert!(app.read(|ctx| ctx.view_name(window_id, view_id).is_some()));

        // Dropping the last strong handle removes the TUI view through the
        // shared ref-count/remove_dropped_items path.
        drop(tui);
        app.update(|_| {});
        assert!(app.read(|ctx| ctx.view_name(window_id, view_id).is_none()));
    });
}

struct ObservedModel {
    count: usize,
}

impl Entity for ObservedModel {
    type Event = ();
}

#[derive(Default)]
struct ObserverTuiView {
    observed_counts: Vec<usize>,
}

impl Entity for ObserverTuiView {
    type Event = ();
}

impl TuiView for ObserverTuiView {
    fn ui_name() -> &'static str {
        "ObserverTuiView"
    }

    fn render(&self, _: &AppContext) -> Box<dyn TuiElement> {
        Box::new(TuiEmpty)
    }
}

/// Regression: `AppContext::notify_model_observers` used to look the observing
/// view up in `windows[..].views` only. A TUI view lives in `tui_views`, so the
/// lookup missed, the callback never fired, and — because the miss was reported
/// as "not alive" — the observation was dropped, permanently disconnecting the
/// view from the model. The two `notify()` rounds below pin both halves: the
/// callback runs, and it survives to run again.
#[test]
fn test_model_observation_from_tui_view() {
    App::test((), |mut app| async move {
        let (window_id, _root) =
            app.add_window(WindowStyle::NotStealFocus, |_| RootView::default());

        let model = app.add_model(|_| ObservedModel { count: 0 });
        let tui = app.update(|ctx| ctx.add_tui_view(window_id, |_| ObserverTuiView::default()));

        tui.update(&mut app, |_, ctx| {
            ctx.observe(&model, |view: &mut ObserverTuiView, observed, ctx| {
                view.observed_counts.push(observed.as_ref(ctx).count);
                // A real observer re-renders itself in response.
                ctx.notify();
            });
        });

        model.update(&mut app, |m, ctx| {
            m.count = 1;
            ctx.notify();
        });
        assert_eq!(
            tui.read(&app, |view, _| view.observed_counts.clone()),
            vec![1],
            "a TUI view's model observation must fire"
        );

        // The observation must still be registered: the pre-fix code dropped it
        // on the first notification, so this second round observed nothing.
        model.update(&mut app, |m, ctx| {
            m.count = 2;
            ctx.notify();
        });
        assert_eq!(
            tui.read(&app, |view, _| view.observed_counts.clone()),
            vec![1, 2],
            "a TUI view's model observation must survive the first notification"
        );

        // The view is still reachable in `tui_views` after being taken out and
        // put back around the callback.
        assert_eq!(
            app.read(|ctx| ctx.view_name(window_id, tui.id()).map(str::to_owned))
                .as_deref(),
            Some("ObserverTuiView"),
        );
    });
}

// ---------------------------------------------------------------------------
// Dual-registry regressions.
//
// The fork keeps TUI views in `Window::tui_views`, beside the GUI `Window::views`.
// Every `AppContext` path that walks "the window's views" has to walk both maps;
// the tests below pin the ones that used to walk only `views`. Each of them fails
// against the unfixed code.
// ---------------------------------------------------------------------------

use pathfinder_geometry::{rect::RectF, vector::Vector2F};
use std::cell::Cell;
use std::rc::Rc;

/// A do-nothing typed action, so a root view can satisfy the `TypedActionView`
/// bound on `add_window` / `add_tui_window` without caring about actions.
#[derive(Debug)]
#[allow(dead_code)]
struct NoopAction;

/// Hook invocations recorded out-of-band: `self_or_child_interacted_with` takes
/// `&self`, and `on_window_closed` fires while the view is being torn down, so
/// neither can be observed through a `ViewHandle` afterwards.
type HookLog = Rc<Cell<usize>>;

#[derive(Default)]
struct InteractionGuiView {
    interactions: HookLog,
}

impl Entity for InteractionGuiView {
    type Event = ();
}

impl View for InteractionGuiView {
    fn ui_name() -> &'static str {
        "InteractionGuiView"
    }

    fn render(&self, _: &AppContext) -> Box<dyn crate::elements::Element> {
        crate::elements::Empty::new().finish()
    }

    fn self_or_child_interacted_with(&self, _ctx: &mut ViewContext<Self>) {
        self.interactions.set(self.interactions.get() + 1);
    }
}

impl TypedActionView for InteractionGuiView {
    type Action = NoopAction;
}

#[derive(Default)]
struct InteractionTuiView {
    interactions: HookLog,
}

impl Entity for InteractionTuiView {
    type Event = ();
}

impl TuiView for InteractionTuiView {
    fn ui_name() -> &'static str {
        "InteractionTuiView"
    }

    fn render(&self, _: &AppContext) -> Box<dyn TuiElement> {
        Box::new(TuiEmpty)
    }

    fn self_or_child_interacted_with(&self, _ctx: &mut ViewContext<Self>) {
        self.interactions.set(self.interactions.get() + 1);
    }
}

/// Regression: `dispatch_self_or_child_interacted_with` looked every link of the
/// responder chain up in `views` only. `get_responder_chain` deliberately routes a
/// window without a GUI presenter through `view_ancestors`, so those chains are
/// made up entirely of `tui_views` ids — the hook was never delivered to a TUI
/// view at all, on any handled custom or typed action.
#[test]
fn test_self_or_child_interacted_with_reaches_tui_views() {
    App::test((), |mut app| async move {
        let gui_log: HookLog = Rc::default();
        let tui_log: HookLog = Rc::default();

        let (window_id, gui_root) = {
            let gui_log = gui_log.clone();
            app.add_window(WindowStyle::NotStealFocus, move |_| InteractionGuiView {
                interactions: gui_log,
            })
        };
        let gui_root_id = gui_root.id();

        let tui = {
            let tui_log = tui_log.clone();
            app.update(move |ctx| {
                let handle = ctx.add_tui_view(window_id, move |_| InteractionTuiView {
                    interactions: tui_log,
                });
                ctx.record_view_parent(window_id, handle.id(), gui_root_id);
                handle
            })
        };

        // Ancestor-first: [gui root, tui leaf].
        let responder_chain = app.read(|ctx| ctx.view_ancestors(window_id, tui.id()));
        assert_eq!(responder_chain, vec![gui_root_id, tui.id()]);

        app.update(|ctx| ctx.dispatch_self_or_child_interacted_with(window_id, &responder_chain));

        assert_eq!(
            gui_log.get(),
            1,
            "the GUI link of the chain must still be notified"
        );
        assert_eq!(
            tui_log.get(),
            1,
            "the TUI link of the chain must be notified too"
        );
    });
}

#[derive(Default)]
struct ClosingTuiView {
    closes: HookLog,
}

impl Entity for ClosingTuiView {
    type Event = ();
}

impl TuiView for ClosingTuiView {
    fn ui_name() -> &'static str {
        "ClosingTuiView"
    }

    fn render(&self, _: &AppContext) -> Box<dyn TuiElement> {
        Box::new(TuiEmpty)
    }

    fn on_window_closed(&mut self, _ctx: &mut ViewContext<Self>) {
        self.closes.set(self.closes.get() + 1);
    }
}

impl TypedActionView for ClosingTuiView {
    type Action = NoopAction;
}

/// Regression: `handle_window_closed` enumerated `window.views.keys()` only, so a
/// TUI view never received `on_window_closed` and never ran its teardown — and in
/// a TUI-only window, where `views` is empty, nothing was notified at all.
#[test]
fn test_window_close_notifies_tui_views() {
    App::test((), |mut app| async move {
        let closes: HookLog = Rc::default();

        let (window_id, _root) = {
            let closes = closes.clone();
            app.update(move |ctx| {
                ctx.add_tui_window(
                    AddWindowOptions {
                        window_style: WindowStyle::NotStealFocus,
                        ..Default::default()
                    },
                    move |_| ClosingTuiView { closes },
                )
            })
        };

        // Held for the rest of the test: dropping it would drop the closed
        // window (and the views inside it) mid-test.
        let _closed_window_data = app.update(|ctx| ctx.handle_window_closed(window_id));

        assert_eq!(
            closes.get(),
            1,
            "a TUI view must receive on_window_closed when its window closes"
        );
    });
}

#[derive(Default)]
struct PlainTuiView;

impl Entity for PlainTuiView {
    type Event = ();
}

impl TuiView for PlainTuiView {
    fn ui_name() -> &'static str {
        "PlainTuiView"
    }

    fn render(&self, _: &AppContext) -> Box<dyn TuiElement> {
        Box::new(TuiEmpty)
    }
}

impl TypedActionView for PlainTuiView {
    type Action = NoopAction;
}

/// Regression: `invalidate_all_views_for_window` collected `window.views.keys()`
/// only, so a forced invalidation never reached a TUI view — and reached nothing
/// at all in a TUI-only window.
#[test]
fn test_invalidate_all_views_for_window_covers_tui_views() {
    App::test((), |mut app| async move {
        let (window_id, root) = app.update(|ctx| {
            ctx.add_tui_window(
                AddWindowOptions {
                    window_style: WindowStyle::NotStealFocus,
                    ..Default::default()
                },
                |_| PlainTuiView,
            )
        });
        let child = app.update(|ctx| ctx.add_tui_view(window_id, |_| CounterView::default()));

        // Registering the views queued invalidations of their own; drain those so
        // the assertion below can only be satisfied by the explicit call.
        app.update(|ctx| {
            ctx.take_all_invalidations_for_window(window_id);
        });
        assert!(
            app.update(|ctx| ctx.take_all_invalidations_for_window(window_id))
                .updated
                .is_empty(),
            "precondition: registration invalidations must be drained"
        );

        app.update(|ctx| ctx.invalidate_all_views_for_window(window_id));

        let updated = app
            .update(|ctx| ctx.take_all_invalidations_for_window(window_id))
            .updated;
        assert!(
            updated.contains(&root.id()),
            "the TUI root view must be force-invalidated"
        );
        assert!(
            updated.contains(&child.id()),
            "a non-root TUI view must be force-invalidated"
        );
    });
}

#[derive(Default)]
struct A11yTuiView {
    content: Option<String>,
}

impl Entity for A11yTuiView {
    type Event = ();
}

impl TuiView for A11yTuiView {
    fn ui_name() -> &'static str {
        "A11yTuiView"
    }

    fn render(&self, _: &AppContext) -> Box<dyn TuiElement> {
        Box::new(TuiEmpty)
    }

    fn accessibility_data(&self, _ctx: &mut ViewContext<Self>) -> Option<AccessibilityData> {
        self.content
            .clone()
            .map(|content| AccessibilityData { content })
    }
}

impl TypedActionView for A11yTuiView {
    type Action = NoopAction;
}

#[derive(Default)]
struct A11yGuiView {
    content: Option<String>,
}

impl Entity for A11yGuiView {
    type Event = ();
}

impl View for A11yGuiView {
    fn ui_name() -> &'static str {
        "A11yGuiView"
    }

    fn render(&self, _: &AppContext) -> Box<dyn crate::elements::Element> {
        crate::elements::Empty::new().finish()
    }

    fn accessibility_data(&self, _ctx: &mut ViewContext<Self>) -> Option<AccessibilityData> {
        self.content
            .clone()
            .map(|content| AccessibilityData { content })
    }
}

/// Regression: `focused_view_accessibility_data` did `window.views.remove(&id)?`,
/// so the first TUI link in the responder chain aborted the entire walk — throwing
/// away the accessibility data of the GUI views further along it, not just the TUI
/// view's own.
#[test]
fn test_focused_view_accessibility_data_spans_both_registries() {
    App::test((), |mut app| async move {
        let (window_id, tui_root) = app.update(|ctx| {
            ctx.add_tui_window(
                AddWindowOptions {
                    window_style: WindowStyle::NotStealFocus,
                    ..Default::default()
                },
                |_| A11yTuiView::default(),
            )
        });
        let tui_root_id = tui_root.id();

        // A GUI view *below* the TUI root, so the chain is [tui_root, gui_leaf]
        // and the TUI link is walked first.
        let gui_leaf = app.update(move |ctx| {
            let handle = ctx.add_view(window_id, |_| A11yGuiView {
                content: Some("gui-leaf".to_owned()),
            });
            ctx.record_view_parent(window_id, handle.id(), tui_root_id);
            handle
        });
        gui_leaf.update(&mut app, |_, ctx| ctx.focus_self());
        assert_eq!(app.focused_view_id(window_id), Some(gui_leaf.id()));

        // The TUI root contributes nothing, so the walk must continue past it.
        assert_eq!(
            app.update(|ctx| ctx.focused_view_accessibility_data(window_id))
                .map(|data| data.content),
            Some("gui-leaf".to_owned()),
            "a TUI link in the chain must not abort the walk"
        );

        // And a TUI view's own data is used when it has some: it is the first link,
        // so it now wins over the GUI leaf below it.
        tui_root.update(&mut app, |view, _| {
            view.content = Some("tui-root".to_owned())
        });
        assert_eq!(
            app.update(|ctx| ctx.focused_view_accessibility_data(window_id))
                .map(|data| data.content),
            Some("tui-root".to_owned()),
            "a TUI view must be able to contribute accessibility data"
        );
    });
}

#[derive(Default)]
struct CursorTuiView {
    font_size: Option<f32>,
}

impl Entity for CursorTuiView {
    type Event = ();
}

impl TuiView for CursorTuiView {
    fn ui_name() -> &'static str {
        "CursorTuiView"
    }

    fn render(&self, _: &AppContext) -> Box<dyn TuiElement> {
        Box::new(TuiEmpty)
    }

    fn active_cursor_position(&self, _ctx: &ViewContext<Self>) -> Option<CursorInfo> {
        self.font_size.map(|font_size| CursorInfo {
            position: RectF::new(Vector2F::new(4.0, 8.0), Vector2F::new(1.0, font_size)),
            font_size,
        })
    }
}

impl TypedActionView for CursorTuiView {
    type Action = NoopAction;
}

/// Regression: `active_cursor_position` looked the focused view up in `views`
/// only, so a focused TUI view reported no cursor and the platform had nowhere to
/// anchor the IME.
#[test]
fn test_active_cursor_position_from_focused_tui_view() {
    App::test((), |mut app| async move {
        let (window_id, root) = app.update(|ctx| {
            ctx.add_tui_window(
                AddWindowOptions {
                    window_style: WindowStyle::NotStealFocus,
                    ..Default::default()
                },
                |_| CursorTuiView::default(),
            )
        });
        assert_eq!(app.focused_view_id(window_id), Some(root.id()));

        // A TUI view with no cursor still reports None.
        assert!(
            app.update(|ctx| ctx.active_cursor_position(window_id))
                .is_none()
        );

        root.update(&mut app, |view, _| view.font_size = Some(16.0));

        assert_eq!(
            app.update(|ctx| ctx.active_cursor_position(window_id))
                .map(|info| info.font_size),
            Some(16.0),
            "a focused TUI view must be able to report its cursor position"
        );
    });
}

/// Regression: `views_of_type`, `view_with_id` and `view_ids_for_window` read
/// `window.views` only, so every one of them was blind to TUI views — while
/// `view_name`, in the same impl block, already checked both maps.
#[test]
fn test_view_queries_span_both_registries() {
    App::test((), |mut app| async move {
        let (window_id, gui_root) =
            app.add_window(WindowStyle::NotStealFocus, |_| RootView::default());
        let tui = app.update(|ctx| ctx.add_tui_view(window_id, |_| CounterView::default()));

        let found = app
            .views_of_type::<CounterView>(window_id)
            .expect("the window exists");
        assert_eq!(
            found.iter().map(|handle| handle.id()).collect::<Vec<_>>(),
            vec![tui.id()],
            "views_of_type must find TUI views"
        );

        assert!(
            app.read(|ctx| ctx
                .view_with_id::<CounterView>(window_id, tui.id())
                .is_some()),
            "view_with_id must find a TUI view"
        );
        assert!(
            app.read(|ctx| ctx
                .view_with_id::<RootView>(window_id, gui_root.id())
                .is_some()),
            "view_with_id must still find a GUI view"
        );
        assert!(
            app.read(|ctx| ctx.view_with_id::<RootView>(window_id, tui.id()).is_none()),
            "view_with_id must still filter on the requested type"
        );

        let view_ids = app.read(|ctx| ctx.view_ids_for_window(window_id));
        assert!(view_ids.contains(&gui_root.id()));
        assert!(
            view_ids.contains(&tui.id()),
            "view_ids_for_window must report TUI views"
        );
    });
}
