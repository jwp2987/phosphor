use std::sync::mpsc::sync_channel;

use super::{ModelEvent, send_model_event_nonblocking, try_send_or_defer};

#[test]
fn try_send_or_defer_returns_the_event_when_the_channel_is_full() {
    let (sender, receiver) = sync_channel::<ModelEvent>(1);
    assert!(
        try_send_or_defer(&sender, ModelEvent::Terminate)
            .expect("channel is connected")
            .is_none()
    );
    // The channel (capacity 1) is now full; the event must come back, not be dropped
    // and not block.
    let deferred = try_send_or_defer(&sender, ModelEvent::Terminate).expect("channel is connected");
    assert!(matches!(deferred, Some(ModelEvent::Terminate)));
    drop(receiver);
    assert!(try_send_or_defer(&sender, ModelEvent::Terminate).is_err());
}

#[test]
fn nonblocking_send_never_blocks_on_a_full_channel_and_never_drops() {
    let (sender, receiver) = sync_channel::<ModelEvent>(1);
    sender.try_send(ModelEvent::Terminate).expect("first fits");
    // The channel is full and nobody is receiving: a blocking `send` here would hang
    // this test forever (the old behaviour); the new call must return immediately.
    send_model_event_nonblocking(&sender, ModelEvent::Terminate).expect("deferred, not failed");
    // Draining the channel lets the deferred send through: both events arrive.
    let first = receiver.recv_timeout(std::time::Duration::from_secs(5));
    let second = receiver.recv_timeout(std::time::Duration::from_secs(5));
    assert!(matches!(first, Ok(ModelEvent::Terminate)));
    assert!(matches!(second, Ok(ModelEvent::Terminate)));
}
