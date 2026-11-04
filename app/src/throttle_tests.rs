use std::time::Duration;

use futures_lite::{StreamExt, future};
use instant::Instant;

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Item {
    id: u32,
    urgent: bool,
}

impl Urgent for Item {
    fn is_urgent(&self) -> bool {
        self.urgent
    }
}

const FLOOR: Duration = Duration::from_millis(4);

#[test]
fn gate_never_bypasses_a_non_urgent_item() {
    let gate = BypassGate::new(FLOOR);
    assert!(!gate.may_bypass(Instant::now(), false));
}

#[test]
fn gate_bypasses_when_nothing_was_delivered_yet() {
    let gate = BypassGate::new(FLOOR);
    assert!(gate.may_bypass(Instant::now(), true));
}

#[test]
fn gate_holds_urgent_items_inside_the_floor_and_releases_them_after() {
    let start = Instant::now();
    let mut gate = BypassGate::new(FLOOR);
    gate.record_delivery(start);
    assert!(!gate.may_bypass(start + Duration::from_millis(1), true));
    assert!(!gate.may_bypass(start + Duration::from_micros(3999), true));
    assert!(gate.may_bypass(start + FLOOR, true));
    assert!(gate.may_bypass(start + Duration::from_millis(10), true));
}

#[test]
fn gate_floor_restarts_with_each_delivery() {
    let start = Instant::now();
    let mut gate = BypassGate::new(FLOOR);
    gate.record_delivery(start);
    let second = start + Duration::from_millis(5);
    assert!(gate.may_bypass(second, true));
    gate.record_delivery(second);
    assert!(!gate.may_bypass(second + Duration::from_millis(1), true));
}

#[test]
fn gate_delays_an_urgent_item_inside_the_floor_only_to_the_floor() {
    let start = Instant::now();
    let mut gate = BypassGate::new(FLOOR);
    assert_eq!(gate.bypass_delay(start, true), Some(Duration::ZERO));
    assert_eq!(gate.bypass_delay(start, false), None);

    gate.record_delivery(start);
    assert_eq!(
        gate.bypass_delay(start + Duration::from_millis(1), true),
        Some(Duration::from_millis(3))
    );
    assert_eq!(gate.bypass_delay(start + FLOOR, true), Some(Duration::ZERO));
    assert_eq!(
        gate.bypass_delay(start + Duration::from_millis(10), true),
        Some(Duration::ZERO)
    );
    assert_eq!(
        gate.bypass_delay(start + Duration::from_millis(1), false),
        None
    );
}

/// #1138: the second write of a keystroke's echo (vim sends the character, then the cursor
/// position) lands inside the floor. It must be delivered when the floor ends, not at the
/// (deliberately huge) period's trailing edge, which is what it used to wait for.
#[test]
fn urgent_item_inside_the_floor_is_delivered_at_the_floor_not_the_period() {
    let (tx, rx) = async_channel::unbounded();
    let floor = Duration::from_millis(50);
    let mut stream = Box::pin(throttle_with_bypass(Duration::from_secs(30), floor, rx));
    future::block_on(async {
        tx.send(Item {
            id: 1,
            urgent: true,
        })
        .await
        .unwrap();
        assert_eq!(stream.next().await.unwrap().id, 1);
        let delivered = Instant::now();

        tx.send(Item {
            id: 2,
            urgent: true,
        })
        .await
        .unwrap();
        assert_eq!(stream.next().await.unwrap().id, 2);
        let waited = delivered.elapsed();
        assert!(waited >= Duration::from_millis(40), "{waited:?}");
        assert!(waited < Duration::from_secs(5), "{waited:?}");
    });
}

/// An urgent item arriving mid-period is delivered long before the (deliberately huge) period
/// ends. With the plain trailing-edge throttle this would wait for the whole period.
#[test]
fn urgent_item_is_delivered_before_the_period_ends() {
    let (tx, rx) = async_channel::unbounded();
    let mut stream = Box::pin(throttle_with_bypass(
        Duration::from_secs(30),
        Duration::from_millis(1),
        rx,
    ));
    future::block_on(async {
        tx.send(Item {
            id: 1,
            urgent: false,
        })
        .await
        .unwrap();
        assert_eq!(stream.next().await.unwrap().id, 1);

        // Past the floor, inside the period.
        Timer::after(Duration::from_millis(10)).next().await;
        let sent = Instant::now();
        tx.send(Item {
            id: 2,
            urgent: true,
        })
        .await
        .unwrap();
        assert_eq!(stream.next().await.unwrap().id, 2);
        assert!(sent.elapsed() < Duration::from_secs(5));
    });
}

/// Non-urgent items keep the old behaviour: the second one waits for the timer.
#[test]
fn non_urgent_item_still_waits_for_the_period() {
    let (tx, rx) = async_channel::unbounded();
    let period = Duration::from_millis(150);
    let mut stream = Box::pin(throttle_with_bypass(period, Duration::from_millis(1), rx));
    future::block_on(async {
        tx.send(Item {
            id: 1,
            urgent: false,
        })
        .await
        .unwrap();
        assert_eq!(stream.next().await.unwrap().id, 1);
        let first = Instant::now();
        tx.send(Item {
            id: 2,
            urgent: false,
        })
        .await
        .unwrap();
        tx.send(Item {
            id: 3,
            urgent: false,
        })
        .await
        .unwrap();
        // Coalesced to the latest item, delivered at the trailing edge.
        assert_eq!(stream.next().await.unwrap().id, 3);
        assert!(first.elapsed() >= Duration::from_millis(100));
    });
}
