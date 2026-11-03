use std::cell::Cell;

use warpui::{App, SingletonEntity};

use super::*;
use crate::test_util::terminal::{add_window_with_terminal, initialize_app_for_terminal_view};

thread_local! {
    static SAVES: Cell<u32> = const { Cell::new(0) };
}

fn record_save(_: &mut AppContext) {
    SAVES.with(|saves| saves.set(saves.get() + 1));
}

fn saves() -> u32 {
    SAVES.with(Cell::get)
}

#[test]
fn quit_takes_the_snapshot_a_pending_save_owes() {
    // #1363: a layout change followed by an immediate quit must be saved by the quit, not by
    // the 250 ms timer the quit outruns.
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let _terminal = add_window_with_terminal(&mut app, None);
        app.update(|ctx| {
            ctx.add_singleton_model(|_| SaveAppSnapshotDebouncer::new());
            save_app(&(), ctx);
        });
        let before = saves();

        app.update(|ctx| flush_pending_app_snapshot_with(ctx, record_save));

        assert_eq!(
            saves(),
            before + 1,
            "the owed snapshot was not taken at quit"
        );
        app.read(|ctx| {
            assert!(SaveAppSnapshotDebouncer::as_ref(ctx).pending_save.is_none());
        });
    });
}

#[test]
fn quit_with_nothing_pending_takes_no_snapshot() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let _terminal = add_window_with_terminal(&mut app, None);
        app.update(|ctx| {
            ctx.add_singleton_model(|_| SaveAppSnapshotDebouncer::new());
        });
        let before = saves();

        app.update(|ctx| flush_pending_app_snapshot_with(ctx, record_save));

        assert_eq!(saves(), before);
    });
}

#[test]
fn quit_without_a_debouncer_is_a_noop() {
    // The remote-server daemon never initializes the workspace actions.
    App::test((), |mut app| async move {
        let before = saves();
        app.update(|ctx| flush_pending_app_snapshot_with(ctx, record_save));
        assert_eq!(saves(), before);
    });
}
