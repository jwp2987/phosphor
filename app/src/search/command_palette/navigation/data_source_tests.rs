use super::*;
use std::cell::Cell;

/// #1151 F3: once the snapshot is taken, queries read it and never rebuild the live list.
/// Before the fix every keystroke rebuilt every pane's session data.
#[test]
fn queries_after_refresh_do_not_rebuild_live_sessions() {
    let live_builds = Cell::new(0);
    let live = || {
        live_builds.set(live_builds.get() + 1);
        vec![1, 2, 3]
    };

    let mut snapshot = SessionSnapshot::<i32>(None);

    // No snapshot yet: falls back to the live items on every query.
    assert_eq!(snapshot.with_items(live, |items| items.to_vec()), [1, 2, 3]);
    assert_eq!(live_builds.get(), 1);

    snapshot.refresh(vec![7, 8]);
    for _ in 0..10 {
        assert_eq!(snapshot.with_items(live, |items| items.to_vec()), [7, 8]);
    }
    assert_eq!(live_builds.get(), 1, "refreshed queries must not rebuild");

    // A later refresh (palette reopened) replaces the snapshot.
    snapshot.refresh(vec![9]);
    assert_eq!(snapshot.with_items(live, |items| items.to_vec()), [9]);
}
