use std::collections::HashMap;
use std::sync::mpsc::sync_channel;
use std::sync::Arc;
use std::time::Duration;

use super::{
    ModelEvent, outstanding_deferred_sends, send_model_event_nonblocking,
    send_snapshot_nonblocking, try_send_or_defer, wait_for_deferred_sends,
};
use crate::app_state::AppState;

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

/// #1251: a send parked on a deferred-send thread is counted until it reaches the channel, so
/// the quit path can wait for it instead of stopping the writer underneath it.
#[test]
fn deferred_sends_are_counted_until_they_reach_the_channel() {
    let (sender, receiver) = sync_channel::<ModelEvent>(1);
    sender.try_send(ModelEvent::Terminate).expect("first fits");
    send_model_event_nonblocking(&sender, ModelEvent::Terminate).expect("deferred, not failed");
    assert!(outstanding_deferred_sends() >= 1);
    assert!(
        !wait_for_deferred_sends(Duration::from_millis(50)),
        "nothing is receiving, so the deferred send cannot have landed"
    );
    assert!(matches!(
        receiver.recv_timeout(Duration::from_secs(5)),
        Ok(ModelEvent::Terminate)
    ));
    assert!(matches!(
        receiver.recv_timeout(Duration::from_secs(5)),
        Ok(ModelEvent::Terminate)
    ));
    assert!(wait_for_deferred_sends(Duration::from_secs(5)));
}

fn snapshot_numbered(active_window_index: usize) -> AppState {
    AppState {
        windows: Vec::new(),
        active_window_index: Some(active_window_index),
        block_lists: Arc::new(HashMap::new()),
        running_mcp_servers: Vec::new(),
    }
}

/// #1251: a snapshot that finds the channel full is not skipped; the newest one waits for room
/// and is delivered, and an older one never follows it.
#[test]
fn a_snapshot_that_finds_the_channel_full_is_delivered_newest_only() {
    let (sender, receiver) = sync_channel::<ModelEvent>(1);
    sender.try_send(ModelEvent::Terminate).expect("first fits");
    send_snapshot_nonblocking(&sender, snapshot_numbered(1));
    send_snapshot_nonblocking(&sender, snapshot_numbered(2));
    assert!(matches!(
        receiver.recv_timeout(Duration::from_secs(5)),
        Ok(ModelEvent::Terminate)
    ));
    match receiver.recv_timeout(Duration::from_secs(5)) {
        Ok(ModelEvent::Snapshot(state)) => assert_eq!(state.active_window_index, Some(2)),
        other => panic!("the newest snapshot must be delivered, got {other:?}"),
    }
    assert!(
        receiver.recv_timeout(Duration::from_millis(300)).is_err(),
        "the older snapshot must not follow the newer one"
    );
    assert!(wait_for_deferred_sends(Duration::from_secs(5)));
}
