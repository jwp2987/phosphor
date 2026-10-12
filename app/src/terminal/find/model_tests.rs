//! Tests for `TerminalFindModel`'s results-revision tracking (#815): a rerun
//! that reproduces the previous results must not announce itself, while any
//! change to the match set or the focused match must.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use parking_lot::FairMutex;
use warpui::{App, ModelHandle};

use super::{FindOptions, TerminalFindModel};
use crate::terminal::model::TerminalModel;
use crate::test_util::settings::initialize_settings_for_tests;
use crate::view_components::find::{FindDirection, FindEvent, FindModel};

/// Records the name of every `FindEvent` the model emits from now on.
fn record_find_events(
    app: &mut App,
    model: &ModelHandle<TerminalFindModel>,
) -> Rc<RefCell<Vec<&'static str>>> {
    let events: Rc<RefCell<Vec<&'static str>>> = Default::default();
    let recorded = events.clone();
    app.update(|ctx| {
        ctx.subscribe_to_model(model, move |_, event, _| {
            recorded.borrow_mut().push(match event {
                FindEvent::RanFind => "RanFind",
                FindEvent::UpdatedFocusedMatch => "UpdatedFocusedMatch",
            });
        });
    });
    events
}

fn results_revision(app: &mut App, model: &ModelHandle<TerminalFindModel>) -> Option<u64> {
    model.update(app, |model, _ctx| model.results_revision())
}

#[test]
fn alt_screen_rerun_bumps_revision_only_when_results_change() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        let mut mock_terminal_model = TerminalModel::mock(None, None);
        mock_terminal_model.set_altscreen_active();
        mock_terminal_model.process_bytes("foo\r\nbar foo\r\nfoo");
        let terminal_model = Arc::new(FairMutex::new(mock_terminal_model));

        let find_model = app.add_model(|ctx| TerminalFindModel::new(terminal_model.clone(), ctx));
        let events = record_find_events(&mut app, &find_model);

        // A fresh run is always a change.
        let before_run = results_revision(&mut app, &find_model);
        find_model.update(&mut app, |model, ctx| {
            model.run_find(
                FindOptions {
                    query: Some("foo".to_owned().into()),
                    ..Default::default()
                },
                ctx,
            );
        });
        let after_run = results_revision(&mut app, &find_model);
        assert_ne!(before_run, after_run, "a new run changes the results");
        assert_eq!(
            find_model.update(&mut app, |model, _ctx| model.match_count()),
            3
        );
        assert_eq!(events.borrow().as_slice(), &["RanFind"][..]);
        events.borrow_mut().clear();

        // Rerunning on an unchanged screen -- what every wakeup does while the
        // find bar is open -- reproduces the results: no bump, no event.
        find_model.update(&mut app, |model, ctx| model.rerun_find_on_active_grid(ctx));
        assert_eq!(
            results_revision(&mut app, &find_model),
            after_run,
            "identical results must not count as a change"
        );
        assert!(
            events.borrow().is_empty(),
            "no RanFind for an unchanged rerun, got {:?}",
            events.borrow()
        );

        // Output that adds a match changes the count: bump and RanFind.
        terminal_model.lock().process_bytes("\r\nfoo");
        find_model.update(&mut app, |model, ctx| model.rerun_find_on_active_grid(ctx));
        let after_change = results_revision(&mut app, &find_model);
        assert_ne!(
            after_change, after_run,
            "a different match count is a change"
        );
        assert_eq!(
            find_model.update(&mut app, |model, _ctx| model.match_count()),
            4
        );
        assert_eq!(events.borrow().as_slice(), &["RanFind"][..]);
        events.borrow_mut().clear();

        // Moving the focus is a change too, announced as UpdatedFocusedMatch.
        let focused_before = find_model.update(&mut app, |model, _ctx| model.focused_match_index());
        find_model.update(&mut app, |model, ctx| {
            model.focus_next_find_match(FindDirection::Up, ctx)
        });
        assert_ne!(
            find_model.update(&mut app, |model, _ctx| model.focused_match_index()),
            focused_before
        );
        assert_ne!(
            results_revision(&mut app, &find_model),
            after_change,
            "a focused-match change is a change"
        );
        assert_eq!(events.borrow().as_slice(), &["UpdatedFocusedMatch"][..]);
    });
}
