use super::*;
use futures_util::StreamExt as _;
use std::time::Instant;
use warpui::App;

const PERIOD: Duration = Duration::from_millis(200);

#[test]
fn test_leading_debounce_emits_first_item_immediately() {
    App::test((), |_app| async move {
        let (tx, rx) = async_channel::unbounded();
        let mut stream = Box::pin(debounce_leading(PERIOD, rx));

        let start = Instant::now();
        tx.send(1).await.unwrap();
        assert_eq!(stream.next().await, Some(1));
        // The trailing-edge `debounce` would not have emitted before `PERIOD` had elapsed.
        assert!(start.elapsed() < PERIOD / 2, "{:?}", start.elapsed());
    });
}

#[test]
fn test_leading_debounce_coalesces_burst_into_one_trailing_item_with_final_value() {
    App::test((), |_app| async move {
        let (tx, rx) = async_channel::unbounded();
        let mut stream = Box::pin(debounce_leading(PERIOD, rx));

        let start = Instant::now();
        tx.send(1).await.unwrap();
        assert_eq!(stream.next().await, Some(1));

        // Inside the window: coalesced, and the final value (4) must win.
        tx.send(2).await.unwrap();
        tx.send(3).await.unwrap();
        tx.send(4).await.unwrap();
        assert_eq!(stream.next().await, Some(4));
        // The trailing item waits for the window to close.
        assert!(start.elapsed() >= PERIOD - Duration::from_millis(20));

        // Nothing else is pending: closing the channel ends the stream without a duplicate.
        drop(tx);
        assert_eq!(stream.next().await, None);
    });
}

#[test]
fn test_leading_debounce_emits_immediately_again_after_quiet_period() {
    App::test((), |_app| async move {
        let (tx, rx) = async_channel::unbounded();
        let mut stream = Box::pin(debounce_leading(PERIOD, rx));

        tx.send(1).await.unwrap();
        assert_eq!(stream.next().await, Some(1));

        Timer::after(PERIOD * 2).await;

        let start = Instant::now();
        tx.send(2).await.unwrap();
        assert_eq!(stream.next().await, Some(2));
        assert!(start.elapsed() < PERIOD / 2, "{:?}", start.elapsed());
    });
}

#[test]
fn test_leading_debounce_flushes_pending_item_when_stream_closes() {
    App::test((), |_app| async move {
        let (tx, rx) = async_channel::unbounded();
        let mut stream = Box::pin(debounce_leading(PERIOD, rx));

        tx.send(1).await.unwrap();
        assert_eq!(stream.next().await, Some(1));
        tx.send(2).await.unwrap();
        drop(tx);
        assert_eq!(stream.next().await, Some(2));
        assert_eq!(stream.next().await, None);
    });
}
