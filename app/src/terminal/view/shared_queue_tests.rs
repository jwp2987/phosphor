//! The pane's ONE input queue (#1353 steps 1-5 of the merge): the editor and the terminal view
//! share a single `PaneQueue`, a drain driven from the editor side hands a pty entry over to the
//! view (the foreign stop), and no key sits in a legacy holder at the same time as in the queue.
//!
//! The editor and the view always share the one queue (the two-instance wiring and its kill switch
//! are gone); these assert the ordering only a single queue can promise.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use warpui::{App, AppContext, ViewHandle};

use super::paste_queue::PtyQueueHost;
use super::{Event, TerminalView};
use crate::terminal::input_queue::{GateKind, GateSpec};
use crate::test_util::terminal::{add_window_with_terminal, initialize_app_for_terminal_view};

fn collect_pty_writes(
    app: &mut App,
    terminal: &ViewHandle<TerminalView>,
) -> Rc<RefCell<Vec<Vec<u8>>>> {
    let pty_writes: Rc<RefCell<Vec<Vec<u8>>>> = Rc::new(RefCell::new(Vec::new()));
    let writes = pty_writes.clone();
    app.update(|ctx| {
        ctx.subscribe_to_view(terminal, move |_, event, _| match event {
            Event::WriteBytesToPty { bytes } | Event::WriteHandoffUserBytesToPty { bytes } => {
                writes.borrow_mut().push(bytes.to_vec());
            }
            _ => {}
        });
    });
    pty_writes
}

fn flat(writes: &Rc<RefCell<Vec<Vec<u8>>>>) -> Vec<u8> {
    writes.borrow().iter().flatten().copied().collect()
}

/// X6, the "exactly one holder" proof: what the LEGACY holders of this pane hold right now (the
/// editor's replay list). On the
/// queue path every one of them must be empty while the queue holds the keys.
fn legacy_held(view: &TerminalView, ctx: &AppContext) -> usize {
    let replay = usize::from(
        view.input
            .as_ref(ctx)
            .editor()
            .as_ref(ctx)
            .paste_hold()
            .replay_pending(),
    );
    replay
}

#[test]
fn the_editor_and_the_view_share_one_queue() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let terminal = add_window_with_terminal(&mut app, None);
        terminal.read(&app, |view, ctx| {
            let editor_queue = view
                .input
                .as_ref(ctx)
                .editor()
                .as_ref(ctx)
                .paste_hold()
                .queue();
            assert!(
                view.paste_queue.queue.ptr_eq(&editor_queue),
                "one instance per pane"
            );
            // One counter: ids handed out on either side never repeat, and say who owns them.
            let editor_paste = view
                .input
                .as_ref(ctx)
                .editor()
                .as_ref(ctx)
                .paste_hold()
                .next_queue_paste_id();
            let pty_paste = view
                .paste_queue
                .queue
                .next_paste_id(crate::terminal::input_queue::PasteOwner::Pty);
            assert!(crate::terminal::input_queue::is_editor_paste_id(
                editor_paste
            ));
            assert!(!crate::terminal::input_queue::is_editor_paste_id(pty_paste));
            assert_eq!(legacy_held(view, ctx), 0);
        });
    })
}

/// Every gate holds every key: an Enter waiting for its verdict holds keys typed into the editor
/// AND keys typed for the pty, in arrival order (there is no editor/pty parity table any more).
#[test]
fn an_enter_gate_holds_editor_keys_and_pty_keys() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let terminal = add_window_with_terminal(&mut app, None);
        let pty_writes = collect_pty_writes(&mut app, &terminal);
        let input = terminal.read(&app, |view, _| view.input.clone());
        input.update(&mut app, |input, ctx| {
            input.open_submitted_enter_gate_awaiting_pty_for_test(ctx)
        });
        terminal.update(&mut app, |view, ctx| {
            let editor_queue = view
                .input
                .as_ref(ctx)
                .editor()
                .as_ref(ctx)
                .paste_hold()
                .queue();
            assert!(editor_queue.peek().holds_keys(), "the Enter holds keys");
            assert!(
                view.input
                    .as_ref(ctx)
                    .editor()
                    .as_ref(ctx)
                    .paste_hold()
                    .queue_is_busy()
            );
            // A pty key typed now is offered to the queue and waits behind the Enter.
            assert_eq!(
                view.offer_to_paste_queue(b"z", true, false, ctx),
                Some(true)
            );
            assert_eq!(legacy_held(view, ctx), 0);
        });
        assert!(
            flat(&pty_writes).is_empty(),
            "the pty key waits behind the Enter"
        );
    })
}

/// X1 (as far as the pieces that exist go) and the foreign-stop test: an Enter gate (editor side)
/// holds a line; a pty gate opens behind it and a pty key is typed behind both; the pty gate
/// closes first (nothing can move: the Enter still holds the head); then the Enter is released by
/// the INPUT, whose drain delivers what it can and STOPS at the pty key. The view continues the
/// drain on the queue's wake, and the pty key reaches the pty, after the line that preceded it.
/// With two instances the pty key is not behind the Enter at all (the old behaviour): it must
/// still arrive, exactly once.
#[test]
fn an_input_drain_hands_a_pty_key_to_the_view_and_nothing_overtakes() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let terminal = add_window_with_terminal(&mut app, None);
        let pty_writes = collect_pty_writes(&mut app, &terminal);
        let input = terminal.read(&app, |view, _| view.input.clone());
        // The Enter gate (barrier 1) with one line typed behind it (entry 1).
        input.update(&mut app, |input, ctx| {
            input.open_submitted_enter_gate_awaiting_pty_for_test(ctx)
        });
        terminal.update(&mut app, |view, ctx| {
            // A pty gate behind it (a terminal paste being read), and a pty key typed behind that.
            let queue = view.paste_queue.queue.clone();
            let mut host = PtyQueueHost::new(view, ctx);
            queue.lock().open_gate(
                &mut host,
                GateSpec::new(GateKind::PasteReading, 77).with_cap(None),
            );
            assert!(view.write_user_bytes_to_pty(b"a".to_vec(), ctx));
            assert!(
                pty_writes.borrow().is_empty(),
                "the pty key waits behind the pty gate"
            );
            assert_eq!(legacy_held(view, ctx), 0, "the queue is the only holder");
            // The pty gate closes. On the shared queue the Enter still holds the head.
            let queue = view.paste_queue.queue.clone();
            let mut host = PtyQueueHost::new(view, ctx);
            queue
                .lock()
                .on_gate_event(&mut host, GateKind::PasteReading, 77);
        });
        assert!(
            flat(&pty_writes).is_empty(),
            "the key is behind the Enter, in typed order"
        );
        // The Enter is released by the input: its drain meets the pty key.
        input.update(&mut app, |input, ctx| input.release_enter_gate(ctx));
        warpui::r#async::Timer::after(Duration::from_millis(100)).await;
        let written = flat(&pty_writes);
        assert_eq!(
            written.iter().filter(|byte| **byte == b'a').count(),
            1,
            "the pty key arrives exactly once: {written:?}"
        );
        if let Some(line) = written.iter().position(|byte| *byte == b'x') {
            let key = written.iter().position(|byte| *byte == b'a').unwrap();
            assert!(line < key, "the line typed first goes first: {written:?}");
        }
        terminal.read(&app, |view, ctx| {
            let queue = view.paste_queue.queue.peek();
            assert!(queue.is_idle(), "everything was delivered");
            assert!(!queue.pending_foreign(), "no stop is left waiting");
            assert_eq!(legacy_held(view, ctx), 0);
        });
    })
}

/// #1441: closing a pane that still holds input is not silent. The summary the view's `Drop` logs
/// is `None` for an idle pane and names counts (never content) for one that holds keys behind a
/// gate or a paste being read.
#[test]
fn a_pane_closed_with_held_input_has_a_drop_summary() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let terminal = add_window_with_terminal(&mut app, None);
        terminal.update(&mut app, |view, ctx| {
            assert_eq!(
                super::paste_queue::dropped_input_summary(
                    &view.paste_queue.queue.peek(),
                    view.paste_queue.payloads.len()
                ),
                None,
                "an idle pane drops nothing"
            );
            let queue = view.paste_queue.queue.clone();
            let mut host = PtyQueueHost::new(view, ctx);
            queue.lock().open_gate(
                &mut host,
                GateSpec::new(GateKind::PasteReading, 77).with_cap(None),
            );
            assert!(view.write_user_bytes_to_pty(b"secret".to_vec(), ctx));
            let line = super::paste_queue::dropped_input_summary(
                &view.paste_queue.queue.peek(),
                view.paste_queue.payloads.len(),
            )
            .expect("a held key and a paste being read are reported");
            assert!(line.contains("queued_entries=1"), "{line}");
            assert!(line.contains("queued_bytes=6"), "{line}");
            assert!(line.contains("paste_being_read=true"), "{line}");
            assert!(!line.contains("secret"), "counts only: {line}");
        });
    })
}
