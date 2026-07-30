use std::cell::Cell;
use std::rc::Rc;

use super::*;
use crate::platform::WindowStyle;
use crate::{AddWindowOptions, App, Entity, TypedActionView};

/// A leaf element with a fixed 1x1 footprint and no visible output; only the
/// layout/paint plumbing matters for these tests, not what lands in the
/// buffer.
struct BlankElement {
    size: Option<TuiSize>,
}

impl TuiElement for BlankElement {
    fn layout(
        &mut self,
        constraint: TuiConstraint,
        _ctx: &mut TuiLayoutContext,
        _app: &AppContext,
    ) -> TuiSize {
        let size = constraint.clamp(TuiSize::new(1, 1));
        self.size = Some(size);
        size
    }

    fn render(
        &mut self,
        _origin: TuiScreenPosition,
        _surface: &mut TuiPaintSurface<'_>,
        _ctx: &mut TuiPaintContext,
    ) {
    }

    fn size(&self) -> Option<TuiSize> {
        self.size
    }
}

/// A view that counts every call to [`TuiView::render`], so tests can tell
/// whether the presenter re-rendered the view or reused a cached element.
struct CountingView {
    render_calls: Rc<Cell<usize>>,
}

impl Entity for CountingView {
    type Event = ();
}

impl TuiView for CountingView {
    fn ui_name() -> &'static str {
        "CountingView"
    }

    fn render(&self, _: &AppContext) -> Box<dyn TuiElement> {
        self.render_calls.set(self.render_calls.get() + 1);
        Box::new(BlankElement { size: None })
    }
}

impl TypedActionView for CountingView {
    type Action = ();
}

fn window_options() -> AddWindowOptions {
    AddWindowOptions {
        window_style: WindowStyle::NotStealFocus,
        ..Default::default()
    }
}

const AREA: TuiRect = TuiRect::new(0, 0, 10, 5);

#[test]
fn present_falls_back_to_a_direct_render_when_invalidate_was_never_called() {
    let render_calls = Rc::new(Cell::new(0));
    App::test((), |mut app| async move {
        let (window_id, root) = app.update(|ctx| {
            ctx.add_tui_window(window_options(), |_| CountingView {
                render_calls: render_calls.clone(),
            })
        });

        let mut presenter = TuiPresenter::new();
        app.update(|ctx| {
            presenter.present(ctx, &root, AREA);
        });

        assert_eq!(
            render_calls.get(),
            1,
            "present() without a prior invalidate() must render the root directly"
        );
        assert!(app.is_window_open(window_id));
    })
}

#[test]
fn present_reuses_the_view_invalidate_already_rendered() {
    let render_calls = Rc::new(Cell::new(0));
    App::test((), |mut app| async move {
        let (window_id, root) = app.update(|ctx| {
            ctx.add_tui_window(window_options(), |_| CountingView {
                render_calls: render_calls.clone(),
            })
        });
        let root_id = root.id();

        let mut presenter = TuiPresenter::new();
        let invalidation = WindowInvalidation {
            updated: [root_id].into_iter().collect(),
            ..Default::default()
        };
        app.read(|ctx| presenter.invalidate(&invalidation, ctx, window_id));
        assert_eq!(
            render_calls.get(),
            1,
            "invalidate() should have rendered the updated root exactly once"
        );

        app.update(|ctx| {
            presenter.present(ctx, &root, AREA);
        });
        assert_eq!(
            render_calls.get(),
            1,
            "present() must consume the element invalidate() already rendered, not render again"
        );
    })
}

#[test]
fn present_reuses_the_cached_root_when_invalidate_reports_no_changes() {
    let render_calls = Rc::new(Cell::new(0));
    App::test((), |mut app| async move {
        let (window_id, root) = app.update(|ctx| {
            ctx.add_tui_window(window_options(), |_| CountingView {
                render_calls: render_calls.clone(),
            })
        });
        let root_id = root.id();

        let mut presenter = TuiPresenter::new();

        // First frame: root is rendered and presented normally.
        let first_invalidation = WindowInvalidation {
            updated: [root_id].into_iter().collect(),
            ..Default::default()
        };
        app.read(|ctx| presenter.invalidate(&first_invalidation, ctx, window_id));
        app.update(|ctx| {
            presenter.present(ctx, &root, AREA);
        });
        assert_eq!(render_calls.get(), 1);

        // Second frame: invalidate() runs but reports nothing changed (e.g. a
        // paint-only repaint for an animation). present() must reuse the
        // cached `last_element` from the first frame rather than re-rendering
        // the view.
        let empty_invalidation = WindowInvalidation::default();
        app.read(|ctx| presenter.invalidate(&empty_invalidation, ctx, window_id));
        app.update(|ctx| {
            presenter.present(ctx, &root, AREA);
        });
        assert_eq!(
            render_calls.get(),
            1,
            "an unchanged root must be served from the cached last_element, not re-rendered"
        );
    })
}

#[test]
fn invalidate_drops_removed_views_from_rendered_views() {
    let render_calls = Rc::new(Cell::new(0));
    App::test((), |mut app| async move {
        let (window_id, root) = app.update(|ctx| {
            ctx.add_tui_window(window_options(), |_| CountingView {
                render_calls: render_calls.clone(),
            })
        });
        let root_id = root.id();

        let mut presenter = TuiPresenter::new();
        let updated = WindowInvalidation {
            updated: [root_id].into_iter().collect(),
            ..Default::default()
        };
        app.read(|ctx| presenter.invalidate(&updated, ctx, window_id));
        assert!(
            presenter.rendered_views.contains_key(&root_id),
            "invalidate() should have populated rendered_views for the updated view"
        );

        let removed = WindowInvalidation {
            removed: [root_id].into_iter().collect(),
            ..Default::default()
        };
        app.read(|ctx| presenter.invalidate(&removed, ctx, window_id));
        assert!(
            !presenter.rendered_views.contains_key(&root_id),
            "invalidate() must prune removed views out of rendered_views, not leave them cached"
        );
    })
}

#[test]
fn buffer_rect_for_covers_the_full_area_from_the_origin() {
    let area = TuiRect::new(3, 4, 10, 6);
    let rect = buffer_rect_for(area);
    assert_eq!(rect, TuiRect::new(0, 0, area.right(), area.bottom()));
}
