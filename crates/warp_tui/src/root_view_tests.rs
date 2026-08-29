use warp::tui_export::register_tui_session_view_test_singletons;
use warpui::platform::WindowStyle;
use warpui::{AddWindowOptions, SingletonEntity as _, UpdateModel};
use warpui_core::{App, TuiView as _, WindowId};

use super::RootTuiView;
use crate::session_registry::{TuiSessions, TuiSessionsEvent};
use crate::test_fixtures::{add_test_semantic_selection, add_test_terminal_session};

fn add_root(app: &mut App) -> (WindowId, warpui_core::ViewHandle<RootTuiView>) {
    app.update(|ctx| {
        ctx.add_tui_window(
            AddWindowOptions {
                window_style: WindowStyle::NotStealFocus,
                ..Default::default()
            },
            |_| RootTuiView::new(),
        )
    })
}

#[test]
fn root_projects_only_the_focused_retained_session_view() {
    App::test((), |mut app| async move {
        register_tui_session_view_test_singletons(&mut app);
        app.update(|ctx| add_test_semantic_selection(ctx));
        app.update(crate::autoupdate::TuiAutoupdater::register);
        let (window_id, root) = add_root(&mut app);
        let sessions = app.add_singleton_model(|_| TuiSessions::new_for_test());
        root.update(&mut app, |_, ctx| {
            ctx.subscribe_to_model(&sessions, |_, _, event, ctx| match event {
                TuiSessionsEvent::SessionRemoved(_) => ctx.notify(),
                TuiSessionsEvent::FocusChanged(_) => ctx.notify(),
            });
        });
        app.read(|ctx| {
            assert!(root.as_ref(ctx).child_view_ids(ctx).is_empty());
        });

        let (first, first_manager) = add_test_terminal_session(&mut app, window_id);
        let first_view_id = first.id();
        let first_id = app.update(|ctx| {
            TuiSessions::register_session(&sessions, first, first_manager, true, ctx)
        });
        app.read(|ctx| {
            assert!(root.as_ref(ctx).child_view_ids(ctx).is_empty());
        });
        root.update(&mut app, |root, ctx| root.show_terminal(ctx));
        app.read(|ctx| {
            assert_eq!(root.as_ref(ctx).child_view_ids(ctx), vec![first_view_id]);
            assert!(ctx.check_view_or_child_focused(window_id, &first_view_id));
        });
        let focused_window_view = app.read(|ctx| ctx.focused_view_id(window_id));
        let (second, second_manager) = add_test_terminal_session(&mut app, window_id);
        let second_view_id = second.id();

        let second_id = app.update(|ctx| {
            TuiSessions::register_session(&sessions, second, second_manager, false, ctx)
        });
        app.read(|ctx| {
            assert_eq!(root.as_ref(ctx).child_view_ids(ctx), vec![first_view_id]);
            assert_eq!(ctx.focused_view_id(window_id), focused_window_view);
        });

        app.update_model(&sessions, |sessions, ctx| {
            sessions.focus_session(second_id, ctx);
        });
        app.read(|ctx| {
            assert_eq!(root.as_ref(ctx).child_view_ids(ctx), vec![second_view_id]);
            assert!(ctx.check_view_or_child_focused(window_id, &second_view_id));
            assert_ne!(ctx.focused_view_id(window_id), focused_window_view);
        });
        app.update_model(&sessions, |sessions, ctx| {
            sessions.focus_session(first_id, ctx);
        });
        app.read(|ctx| {
            assert_eq!(root.as_ref(ctx).child_view_ids(ctx), vec![first_view_id]);
            assert!(ctx.check_view_or_child_focused(window_id, &first_view_id));
            assert_eq!(ctx.focused_view_id(window_id), focused_window_view);
        });

        root.update(&mut app, |root, ctx| root.show_auth(ctx));
        app.read(|ctx| {
            assert!(root.as_ref(ctx).child_view_ids(ctx).is_empty());
            assert!(ctx.check_view_or_child_focused(window_id, &root.id()));
        });
    });
}

/// Ported from the pin's `terminal_root_focus_delegates_to_the_selected_session`
/// (upstream 4111d08f9), unchanged.
///
/// Deleting `RootTuiView::on_focus` leaves framework focus parked on the root
/// after `focus_self`, so the final assertion fails.
#[test]
fn terminal_root_focus_delegates_to_the_selected_session() {
    App::test((), |mut app| async move {
        register_tui_session_view_test_singletons(&mut app);
        app.update(|ctx| add_test_semantic_selection(ctx));
        app.update(crate::autoupdate::TuiAutoupdater::register);
        let (window_id, root) = add_root(&mut app);
        let sessions = app.add_singleton_model(|_| TuiSessions::new_for_test());
        let (foreground, foreground_manager) = add_test_terminal_session(&mut app, window_id);
        let foreground_id = app.update(|ctx| {
            TuiSessions::register_session(
                &sessions,
                foreground.clone(),
                foreground_manager,
                true,
                ctx,
            )
        });
        let (background, background_manager) = add_test_terminal_session(&mut app, window_id);
        app.update(|ctx| {
            TuiSessions::register_session(
                &sessions,
                background.clone(),
                background_manager,
                false,
                ctx,
            );
        });
        root.update(&mut app, |root, ctx| root.show_terminal(ctx));

        background.update(&mut app, |background, ctx| background.activate(ctx));
        assert!(app.read(|ctx| { ctx.check_view_or_child_focused(window_id, &background.id()) }));
        assert_eq!(
            app.read(|ctx| TuiSessions::as_ref(ctx).focused_session_id()),
            Some(foreground_id)
        );

        root.update(&mut app, |_, ctx| ctx.focus_self());

        assert!(app.read(|ctx| { ctx.check_view_or_child_focused(window_id, &foreground.id()) }));
    });
}
