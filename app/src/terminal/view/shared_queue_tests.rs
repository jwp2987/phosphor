//! The pane's ONE input queue (#1353 steps 1-5 of the merge): the editor and the terminal view
//! share a single `PaneQueue`, a drain driven from the editor side hands a pty entry over to the
//! view (the foreign stop), and no key sits in a legacy holder at the same time as in the queue.
//!
//! Every test runs on both wirings: shared (the default) and two instances
//! (`PHOSPHOR_INPUT_QUEUE_SHARED=0`, `PHOSPHOR_TEST_SHARED_QUEUE=0` for the suite), asserting the
//! observable behaviour that must be the same on both and the ordering that only the shared queue
//! can promise.

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
/// subshell buffers, the ssh typeahead, the pty paste sequencer, the editor's replay list). On the
/// queue path every one of them must be empty while the queue holds the keys.
fn legacy_held(view: &TerminalView, ctx: &AppContext) -> usize {
    let setup: usize = view.subshell_setup_held_bytes.iter().map(Vec::len).sum();
    let bootstrap: usize = view
        .subshell_bootstrap_in_flight
        .as_ref()
        .map_or(0, |in_flight| {
            in_flight.held_user_bytes.iter().map(Vec::len).sum()
        });
    let ssh = usize::from(!view.ssh_typeahead.is_empty());
    let sequencer = usize::from(view.paste_sequencer.has_pending());
    let replay = usize::from(
        view.input
            .as_ref(ctx)
            .editor()
            .as_ref(ctx)
            .paste_hold()
            .replay_pending(),
    );
    setup + bootstrap + ssh + sequencer + replay
}

#[test]
fn the_editor_and_the_view_share_one_queue_exactly_when_the_switch_is_on() {
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
            assert_eq!(
                view.paste_queue.queue.ptr_eq(&editor_queue),
                view.paste_queue.shared,
                "one instance per pane when shared, two when not"
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

/// Parity (the table of `GateKind::holds_editor_keys` / `holds_pty_keys`): an Enter waiting for
/// its verdict holds keys typed into the editor, never keys typed for the pty. On the shared queue
/// this is what keeps the merge from changing behaviour: a pty key still goes straight out.
#[test]
fn an_enter_gate_holds_editor_keys_but_not_pty_keys_on_either_wiring() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let terminal = add_window_with_terminal(&mut app, None);
        let input = terminal.read(&app, |view, _| view.input.clone());
        input.read(&app, |input, ctx| {
            input.editor().as_ref(ctx).paste_hold().set_uses_queue(true);
        });
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
            {
                let queue = editor_queue.peek();
                assert!(queue.holds_editor_keys(), "the Enter holds editor keys");
                assert!(!queue.holds_pty_keys(), "and never held a pty key");
            }
            assert!(
                view.input
                    .as_ref(ctx)
                    .editor()
                    .as_ref(ctx)
                    .paste_hold()
                    .queue_is_busy()
            );
            // A pty key typed now is not offered to the queue at all.
            assert_eq!(view.offer_to_paste_queue(b"z", true, false, ctx), None);
            assert_eq!(legacy_held(view, ctx), 0);
        });
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
        let shared = terminal.read(&app, |view, _| view.paste_queue.shared);
        input.read(&app, |input, ctx| {
            input.editor().as_ref(ctx).paste_hold().set_uses_queue(true);
        });
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
        if shared {
            assert!(
                flat(&pty_writes).is_empty(),
                "shared: the key is behind the Enter, in typed order"
            );
        }
        // The Enter is released by the input: its drain meets the pty key.
        input.update(&mut app, |input, ctx| input.release_enter_gate(ctx));
        warpui::r#async::Timer::after(Duration::from_millis(100)).await;
        let written = flat(&pty_writes);
        assert_eq!(
            written.iter().filter(|byte| **byte == b'a').count(),
            1,
            "the pty key arrives exactly once (shared={shared}): {written:?}"
        );
        if shared && let Some(line) = written.iter().position(|byte| *byte == b'x') {
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
