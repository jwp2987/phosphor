use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::sync_channel;
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

fn numbered(n: usize) -> ModelEvent {
    ModelEvent::IncrementRetryCount(n.to_string())
}

fn number_of(event: ModelEvent) -> usize {
    match event {
        ModelEvent::IncrementRetryCount(n) => n.parse().expect("numbered event"),
        other => panic!("unexpected event {other:?}"),
    }
}

/// #1367: a write that found the channel full must not be overtaken by a later one that finds
/// room (the conversation writes are full replaces; the older landing last leaves stale state).
#[test]
fn a_later_event_never_overtakes_an_earlier_deferred_one() {
    let (sender, receiver) = sync_channel::<ModelEvent>(1);
    sender.try_send(numbered(0)).expect("first fits");
    send_model_event_nonblocking(&sender, numbered(1)).expect("deferred, not failed");
    // Room appears while event 1 is still waiting for the delivery thread.
    assert_eq!(
        number_of(receiver.recv_timeout(Duration::from_secs(5)).unwrap()),
        0
    );
    send_model_event_nonblocking(&sender, numbered(2)).expect("queued behind 1");
    send_model_event_nonblocking(&sender, numbered(3)).expect("queued behind 2");
    let mut order = Vec::new();
    for _ in 0..3 {
        order.push(number_of(
            receiver.recv_timeout(Duration::from_secs(5)).unwrap(),
        ));
    }
    assert_eq!(order, vec![1, 2, 3]);
    assert!(wait_for_deferred_sends(Duration::from_secs(5)));
}

/// #1367: many deferred events share one delivery thread.
#[test]
fn deferred_events_do_not_each_get_a_thread() {
    let (sender, receiver) = sync_channel::<ModelEvent>(1);
    sender.try_send(numbered(0)).expect("first fits");
    for n in 1..=20 {
        send_model_event_nonblocking(&sender, numbered(n)).expect("deferred, not failed");
    }
    // One count for the one delivery thread, however many events wait.
    assert!(
        outstanding_deferred_sends() <= 2,
        "one counted send per delivery thread"
    );
    for expected in 0..=20 {
        assert_eq!(
            number_of(receiver.recv_timeout(Duration::from_secs(5)).unwrap()),
            expected
        );
    }
    assert!(wait_for_deferred_sends(Duration::from_secs(5)));
}
