//! Tests for the render-dirty flag that gates the terminal view's per-wakeup frame (#812).

use super::ChannelEventListener;
use crate::terminal::event::Event as TerminalEvent;

fn listener_with_wakeups() -> (ChannelEventListener, async_channel::Receiver<()>) {
    let (wakeups_tx, wakeups_rx) = async_channel::unbounded();
    let listener = ChannelEventListener::builder_for_test()
        .with_wakeups_tx(wakeups_tx)
        .build();
    (listener, wakeups_rx)
}

#[test]
fn starts_dirty_and_take_clears() {
    let listener = ChannelEventListener::new_for_test();
    assert!(
        listener.is_render_dirty(),
        "the first wakeup must always draw"
    );
    assert!(listener.take_render_dirty());
    assert!(!listener.is_render_dirty());
    assert!(
        !listener.take_render_dirty(),
        "a second take sees the cleared flag"
    );
}

#[test]
fn send_wakeup_event_marks_dirty_and_queues_a_wakeup() {
    let (listener, wakeups_rx) = listener_with_wakeups();
    assert!(listener.take_render_dirty());

    listener.send_wakeup_event();

    assert!(listener.is_render_dirty());
    assert!(
        wakeups_rx.try_recv().is_ok(),
        "the wakeup is still delivered"
    );
}

#[test]
fn byte_processing_wakeup_queues_a_wakeup_without_marking_dirty() {
    let (listener, wakeups_rx) = listener_with_wakeups();
    assert!(listener.take_render_dirty());

    listener.send_byte_processing_wakeup_event();

    assert!(
        !listener.is_render_dirty(),
        "the handler, not the pty read, decides whether a frame is needed"
    );
    assert!(
        wakeups_rx.try_recv().is_ok(),
        "the wakeup is still delivered"
    );
}

#[test]
fn terminal_events_mark_dirty() {
    let listener = ChannelEventListener::new_for_test();
    assert!(listener.take_render_dirty());

    listener.send_terminal_event(TerminalEvent::Typeahead);

    assert!(listener.is_render_dirty());
}

#[test]
fn mark_is_shared_between_clones() {
    let listener = ChannelEventListener::new_for_test();
    let clone = listener.clone();
    assert!(listener.take_render_dirty());
    assert!(!clone.is_render_dirty());

    clone.mark_render_dirty();

    assert!(listener.is_render_dirty());
    assert!(listener.take_render_dirty());
    assert!(!clone.is_render_dirty());
}
