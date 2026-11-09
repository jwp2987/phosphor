use std::time::Duration;

use crate::proto::{ErrorCode, server_message};

use super::*;

fn pair() -> (Reply, async_channel::Receiver<ServerMessage>) {
    let (tx, rx) = async_channel::unbounded();
    (Reply::new(tx, "req-1".to_string()), rx)
}

#[test]
fn a_finished_job_replies_with_its_request_id_and_unregisters() {
    let (reply, rx) = pair();
    let inflight = Inflight::default();
    inflight.spawn(reply, |_| {
        server_message::Message::Error(crate::proto::ErrorResponse {
            code: ErrorCode::Internal.into(),
            message: "done".into(),
        })
    });
    let msg = async_io::block_on(async {
        use crate::rt::FutureExt as _;
        rx.recv().with_timeout(Duration::from_secs(10)).await
    })
    .expect("timed out")
    .expect("channel closed");
    assert_eq!(msg.request_id, "req-1");
    assert!(
        !inflight.abort("req-1"),
        "a finished job is no longer in flight"
    );
}

#[test]
fn an_aborted_job_sends_nothing() {
    let (reply, rx) = pair();
    let inflight = Inflight::default();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
    inflight.spawn(reply, move |_| {
        started_tx.send(()).unwrap();
        go_rx.recv().unwrap();
        server_message::Message::Error(crate::proto::ErrorResponse::default())
    });
    started_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(inflight.abort("req-1"));
    go_tx.send(()).unwrap();
    // Give the worker a moment to finish; nothing may arrive.
    std::thread::sleep(Duration::from_millis(200));
    assert!(rx.try_recv().is_err());
}

#[test]
fn cancelling_before_the_child_is_recorded_still_kills_it() {
    let cancel = Cancel::default();
    cancel.cancel();
    assert!(cancel.is_cancelled());
    // A pgid that cannot exist: the kill is a harmless ESRCH, but the path must not panic.
    cancel.set_child_group(i32::MAX);
}
