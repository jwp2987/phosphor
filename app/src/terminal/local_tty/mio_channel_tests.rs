use std::sync::mpsc::TryRecvError;

use super::channel;

/// `has_pending` is how the pty reader notices, between parse iterations, that a keystroke is
/// waiting for it. It must be true from the moment a message is sent until it is received, and
/// false otherwise.
#[test]
fn has_pending_tracks_sent_and_received_messages() {
    let (tx, rx) = channel::<u8>();
    assert!(!rx.has_pending());

    tx.send(1).unwrap();
    assert!(rx.has_pending());

    // A clone of the sender counts into the same queue.
    let tx2 = tx.clone();
    tx2.send(2).unwrap();
    assert_eq!(rx.try_recv(), Ok(1));
    assert!(rx.has_pending(), "one message is still queued");

    assert_eq!(rx.try_recv(), Ok(2));
    assert!(!rx.has_pending());
}

/// An empty `try_recv` must not take a message out of the count (it would go negative and read
/// as "pending" forever).
#[test]
fn an_empty_try_recv_leaves_nothing_pending() {
    let (tx, rx) = channel::<u8>();
    assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
    assert!(!rx.has_pending());

    tx.send(7).unwrap();
    assert_eq!(rx.try_recv(), Ok(7));
    assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
    assert!(!rx.has_pending());
}

/// A send that fails because the receiver is gone is not left counted.
#[test]
fn a_failed_send_is_not_counted() {
    let (tx, rx) = channel::<u8>();
    drop(rx);
    assert!(tx.send(1).is_err());
    // No receiver to ask, but a second failed send must still not panic or wrap the counter.
    assert!(tx.send(2).is_err());
}
