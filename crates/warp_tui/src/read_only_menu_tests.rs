//! Tests ported from the pinned Warp oracle (`02b53fcd8`). Two oracle tests —
//! `selection_stops_at_trailing_whitespace` and
//! `double_click_selects_complete_styled_text` — are not ported: they assert
//! on `TuiViewportedList::with_trimmed_selection_line_ends` and
//! `TuiSelectable::with_semantic_selection_by_style`, neither of which exists
//! in this fork's `warpui_core` yet (see the doc comment on
//! [`super::TuiReadOnlyMenu::render`]).
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use warp::tui_export::Appearance;
use warpui::event::ModifiersState;
use warpui::{App, EntityId, EntityIdMap};
use warpui_core::elements::tui::{
    TuiBuffer, TuiConstraint, TuiContainer, TuiElement, TuiEvent, TuiEventContext, TuiFlex,
    TuiLayoutContext, TuiPaintContext, TuiPaintSurface, TuiPoint, TuiRect, TuiScreenPosition,
    TuiSelectionHandle, TuiSize,
};

use super::{
    TuiReadOnlyMenu, TuiReadOnlyMenuRow, TuiReadOnlyMenuSection, TuiReadOnlyMenuText,
    TuiReadOnlyMenuVisualRow,
};
use crate::tui_builder::TuiUiBuilder;

fn render(app: &App, element: &mut dyn TuiElement, size: TuiSize) -> TuiBuffer {
    render_with_constraint(app, element, TuiConstraint::tight(size))
}

fn render_with_constraint(
    app: &App,
    element: &mut dyn TuiElement,
    constraint: TuiConstraint,
) -> TuiBuffer {
    app.read(|ctx| {
        let mut rendered_views = EntityIdMap::default();
        let mut layout_ctx = TuiLayoutContext {
            rendered_views: &mut rendered_views,
        };
        let size = element.layout(constraint, &mut layout_ctx, ctx);
        let mut buffer = TuiBuffer::empty(TuiRect::new(0, 0, size.width, size.height));
        let mut paint_ctx = TuiPaintContext::new(&mut rendered_views);
        {
            let mut surface = TuiPaintSurface::new(&mut buffer);
            element.render(TuiScreenPosition::new(0, 0), &mut surface, &mut paint_ctx);
        }
        buffer
    })
}

fn dispatch_mouse(app: &App, element: &mut dyn TuiElement, size: TuiSize, event: TuiEvent) -> bool {
    app.read(|ctx| {
        let mut rendered_views = EntityIdMap::default();
        let mut layout_ctx = TuiLayoutContext {
            rendered_views: &mut rendered_views,
        };
        element.layout(TuiConstraint::tight(size), &mut layout_ctx, ctx);
        let mut buffer = TuiBuffer::empty(TuiRect::new(0, 0, size.width, size.height));
        let mut paint_ctx = TuiPaintContext::new(&mut rendered_views);
        {
            let mut surface = TuiPaintSurface::new(&mut buffer);
            element.render(TuiScreenPosition::new(0, 0), &mut surface, &mut paint_ctx);
        }
        let scene = Rc::new(paint_ctx.scene.clone());
        drop(paint_ctx);
        let mut event_ctx = TuiEventContext::new(scene, &mut rendered_views);
        event_ctx.set_origin_view(Some(EntityId::new()));
        element.dispatch_event(&event, &mut event_ctx, ctx)
    })
}

fn left_down(x: u16, y: u16) -> TuiEvent {
    TuiEvent::LeftMouseDown {
        position: TuiPoint::new(x, y),
        modifiers: ModifiersState::default(),
        click_count: 1,
        is_first_mouse: false,
    }
}

fn left_drag(x: u16, y: u16) -> TuiEvent {
    TuiEvent::LeftMouseDragged {
        position: TuiPoint::new(x, y),
        modifiers: ModifiersState::default(),
    }
}

fn left_up(x: u16, y: u16) -> TuiEvent {
    TuiEvent::LeftMouseUp {
        position: TuiPoint::new(x, y),
        modifiers: ModifiersState::default(),
    }
}

#[test]
fn visual_rows_own_the_full_width_background() {
    App::test((), |app| async move {
        app.add_singleton_model(|_| Appearance::mock());
        let (mut element, background) = app.read(|ctx| {
            let builder = TuiUiBuilder::from_app(ctx);
            let row = TuiReadOnlyMenuRow::new([TuiReadOnlyMenuText::new([(
                "Version".to_owned(),
                builder.primary_text_style(),
            )])]);
            (
                TuiReadOnlyMenuVisualRow::Content(row).render(builder.read_only_menu_background()),
                builder.read_only_menu_background(),
            )
        });

        let buffer = render_with_constraint(
            &app,
            element.as_mut(),
            TuiConstraint::loose(TuiSize::new(40, 1)),
        );

        assert_eq!(buffer.area.width, 40);
        assert_eq!(buffer[(0, 0)].style().bg, Some(background));
        assert_eq!(buffer[(39, 0)].style().bg, Some(background));
    });
}

#[test]
fn background_fills_available_width_under_loose_constraints() {
    App::test((), |app| async move {
        app.add_singleton_model(|_| Appearance::mock());
        let (mut element, background) = app.read(|ctx| {
            let builder = TuiUiBuilder::from_app(ctx);
            let row = TuiReadOnlyMenuRow::new([TuiReadOnlyMenuText::new([(
                "Version".to_owned(),
                builder.primary_text_style(),
            )])]);
            (
                TuiReadOnlyMenu::new(vec![TuiReadOnlyMenuSection::new("Status", vec![row])])
                    .render(
                        TuiSelectionHandle::default(),
                        &builder,
                        |_, _| {},
                        |_, _, _| {},
                    ),
                builder.read_only_menu_background(),
            )
        });

        let buffer = render_with_constraint(
            &app,
            element.as_mut(),
            TuiConstraint::loose(TuiSize::new(40, 2)),
        );

        assert_eq!(buffer.area.width, 40);
        for row in 0..buffer.area.height {
            assert_eq!(buffer[(0, row)].style().bg, Some(background));
            assert_eq!(buffer[(39, row)].style().bg, Some(background));
        }
    });
}

#[test]
fn background_fills_available_width_through_session_style_wrapper() {
    App::test((), |app| async move {
        app.add_singleton_model(|_| Appearance::mock());
        let (mut element, background) = app.read(|ctx| {
            let builder = TuiUiBuilder::from_app(ctx);
            let row = TuiReadOnlyMenuRow::new([TuiReadOnlyMenuText::new([(
                "Version".to_owned(),
                builder.primary_text_style(),
            )])]);
            let menu = TuiReadOnlyMenu::new(vec![TuiReadOnlyMenuSection::new("Status", vec![row])])
                .render(
                    TuiSelectionHandle::default(),
                    &builder,
                    |_, _| {},
                    |_, _, _| {},
                );
            (
                TuiFlex::column()
                    .child(TuiContainer::new(menu).with_padding_top(1).finish())
                    .finish(),
                builder.read_only_menu_background(),
            )
        });

        let buffer = render_with_constraint(
            &app,
            element.as_mut(),
            TuiConstraint::loose(TuiSize::new(40, 3)),
        );

        assert_eq!(buffer.area.width, 40);
        for row in 1..buffer.area.height {
            assert_eq!(buffer[(0, row)].style().bg, Some(background));
            assert_eq!(buffer[(39, row)].style().bg, Some(background));
        }
    });
}

#[test]
fn selection_spans_section_titles_and_rows() {
    App::test((), |app| async move {
        app.add_singleton_model(|_| Appearance::mock());
        let starts = Rc::new(Cell::new(0));
        let copies = Rc::new(RefCell::new(Vec::new()));
        let starts_for_callback = starts.clone();
        let copies_for_callback = copies.clone();
        let mut element = app.read(|ctx| {
            let builder = TuiUiBuilder::from_app(ctx);
            let row = TuiReadOnlyMenuRow::new([TuiReadOnlyMenuText::new([(
                "Version".to_owned(),
                builder.primary_text_style(),
            )])]);
            TuiReadOnlyMenu::new(vec![TuiReadOnlyMenuSection::new("Status", vec![row])]).render(
                TuiSelectionHandle::default(),
                &builder,
                move |_, _| starts_for_callback.set(starts_for_callback.get() + 1),
                move |text, _, _| copies_for_callback.borrow_mut().push(text),
            )
        });
        let size = TuiSize::new(40, 2);

        let _ = render(&app, element.as_mut(), size);
        assert!(dispatch_mouse(
            &app,
            element.as_mut(),
            size,
            left_down(1, 0)
        ));
        assert!(dispatch_mouse(
            &app,
            element.as_mut(),
            size,
            left_drag(7, 1)
        ));
        assert!(dispatch_mouse(&app, element.as_mut(), size, left_up(7, 1)));

        assert_eq!(starts.get(), 1);
        assert_eq!(copies.borrow().as_slice(), ["Status\nVersion"]);
    });
}
