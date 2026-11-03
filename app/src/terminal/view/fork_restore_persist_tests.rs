use super::*;

fn id(n: u32) -> BlockId {
    BlockId::from(format!("block-{n}"))
}

#[test]
fn pending_block_waits_for_the_install_however_many_polls_pass() {
    let mut persist = ForkRestorePersist::default();
    // The old code gave up after 200 polls (10 s). No count of polls, and so no length of
    // parse, may drop a pending block.
    for _ in 0..10_000 {
        assert!(persist.poll([(id(1), true)]).is_empty());
        assert!(persist.is_waiting());
    }
    // The install event: the block is persisted, exactly once.
    assert_eq!(persist.poll([(id(1), false)]), vec![id(1)]);
    assert!(!persist.is_waiting());
    assert!(persist.poll([(id(1), false)]).is_empty());
}

#[test]
fn ready_blocks_persist_immediately_and_pending_ones_later() {
    let mut persist = ForkRestorePersist::default();
    assert_eq!(
        persist.poll([(id(1), false), (id(2), true), (id(3), false)]),
        vec![id(1), id(3)]
    );
    assert!(persist.is_waiting());
    assert_eq!(
        persist.poll([(id(1), false), (id(2), false), (id(3), false)]),
        vec![id(2)]
    );
    assert!(!persist.is_waiting());
}

#[test]
fn only_the_current_watchdog_gives_up() {
    let mut persist = ForkRestorePersist::default();
    persist.poll([(id(1), true)]);
    let stale = persist.generation();
    // Progress (an install, another poll) makes the older watchdog stale.
    persist.poll([(id(1), true)]);
    assert!(!persist.give_up(stale));
    assert!(persist.is_waiting());
    let current = persist.generation();
    assert!(persist.give_up(current));
    assert!(!persist.is_waiting());
}

#[test]
fn watchdog_after_completion_is_a_noop() {
    let mut persist = ForkRestorePersist::default();
    persist.poll([(id(1), true)]);
    let armed = persist.generation();
    persist.poll([(id(1), false)]);
    assert!(!persist.give_up(armed));
}

/// #1265: a block whose deferred restore failed is reported once, so the user is told the fork
/// lost it instead of the block vanishing silently; a new fork reports it again.
#[test]
fn failed_blocks_are_reported_once_per_fork() {
    let mut persist = ForkRestorePersist::default();
    assert!(!persist.note_failed([]));
    assert!(persist.note_failed([id(1)]));
    assert!(!persist.note_failed([id(1)]));
    assert!(persist.note_failed([id(1), id(2)]));
    assert!(!persist.note_failed([id(2), id(1)]));
    persist.reset();
    assert!(persist.note_failed([id(1)]));
}
