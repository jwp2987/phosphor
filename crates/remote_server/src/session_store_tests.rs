use super::*;

/// Spawn metadata for a test session. Values are arbitrary but distinct, so a
/// test asserting on metadata cannot pass by picking up a default.
fn test_metadata() -> SessionSpawnMetadata {
    SessionSpawnMetadata {
        cwd: "/home/test".to_string(),
        shell: Some("/bin/bash".to_string()),
        rows: 24,
        cols: 80,
    }
}

fn id(s: &str) -> RemotePtySessionId {
    RemotePtySessionId::from(s.to_string())
}

// Breaks if: `register` stops using `HashMap::entry(..).or_insert_with(..)`
// and instead unconditionally inserts a fresh `Session` (e.g.
// `self.sessions.insert(id, Session::new(...))`), which would wipe the
// buffered "hello" back to empty on the second `register` call.
#[test]
fn register_is_idempotent() {
    let mut store = SessionStore::new();
    let a = id("a");

    store.register(a.clone(), test_metadata());
    store.append_output(&a, b"hello").unwrap();
    store.register(a.clone(), test_metadata());

    let drained = store.drain_output(&a).unwrap();
    assert_eq!(drained.bytes, b"hello");
}

// Breaks if: `append` starts prepending instead of extending (e.g.
// `self.bytes.push_front` in a loop instead of `self.bytes.extend`),
// which would preserve the length but reverse the byte order.
#[test]
fn append_then_drain_returns_exact_bytes_in_order() {
    let mut store = SessionStore::new();
    let a = id("a");
    store.register(a.clone(), test_metadata());

    store.append_output(&a, b"hello, ").unwrap();
    store.append_output(&a, b"world").unwrap();

    let drained = store.drain_output(&a).unwrap();
    assert_eq!(drained.bytes, b"hello, world");
    assert_eq!(drained.dropped_bytes_total, 0);
}

// Breaks if: `append_output` resets `session.exit_status` (e.g. a stray
// `session.exit_status = None;` added to it, mirroring the exact bug this
// test exists to catch), or if the drop-oldest bound in `append` is
// removed so the buffer grows unbounded instead of staying at the
// configured bound.
#[test]
fn exit_status_survives_a_flood_of_output_after_exit() {
    let mut store = SessionStore::with_output_bound(16);
    let a = id("a");
    store.register(a.clone(), test_metadata());

    store.record_exit(&a, SessionExitStatus::exited(0)).unwrap();
    store.append_output(&a, &vec![b'x'; 1000]).unwrap();

    let summary = store.list().into_iter().find(|s| s.id == a).unwrap();
    assert_eq!(
        summary.state,
        SessionState::Exited(SessionExitStatus::exited(0))
    );
    assert_eq!(summary.buffered_bytes, 16);
}

// Draining clears the bytes but NOT the dropped count, which is cumulative for
// the life of the session. The count must survive a drain because draining is
// "remove for use", not "delivered": a caller that drains, builds a reattach
// payload and then fails to deliver it must not have destroyed the only record
// that a gap existed.
//
// Breaks if: `drain` resets `self.dropped_bytes = 0` (the second assertion
// would see 0), or if it stops clearing `self.bytes` (the third would still
// see "efgh").
#[test]
fn draining_clears_buffer_but_keeps_the_cumulative_dropped_count() {
    let mut store = SessionStore::with_output_bound(4);
    let a = id("a");
    store.register(a.clone(), test_metadata());
    store.append_output(&a, b"abcdefgh").unwrap(); // 8 bytes into a 4-byte bound

    let first = store.drain_output(&a).unwrap();
    assert_eq!(first.bytes, b"efgh");
    assert_eq!(first.dropped_bytes_total, 4);

    let second = store.drain_output(&a).unwrap();
    assert_eq!(second.bytes, Vec::<u8>::new(), "drain must clear the bytes");
    assert_eq!(
        second.dropped_bytes_total, 4,
        "the dropped count is cumulative and must survive a drain"
    );

    // And it keeps accumulating across periods rather than restarting.
    store.append_output(&a, b"ijklmnop").unwrap();
    let third = store.drain_output(&a).unwrap();
    assert_eq!(third.bytes, b"mnop");
    assert_eq!(
        third.dropped_bytes_total, 8,
        "4 from the first period + 4 from the second"
    );
}

// Breaks if: session lookup in `append_output` stops keying on `id` (e.g.
// `self.sessions.values_mut().next()` instead of
// `self.sessions.get_mut(id)`), which would route session A's flood into
// session B's buffer.
#[test]
fn bounds_are_per_session_not_per_store() {
    let mut store = SessionStore::with_output_bound(8);
    let noisy = id("noisy");
    let quiet = id("quiet");
    store.register(noisy.clone(), test_metadata());
    store.register(quiet.clone(), test_metadata());

    store.append_output(&noisy, &vec![b'n'; 1000]).unwrap();
    store.append_output(&quiet, b"ok").unwrap();

    let quiet_summary = store.list().into_iter().find(|s| s.id == quiet).unwrap();
    assert_eq!(quiet_summary.buffered_bytes, 2);
    assert_eq!(quiet_summary.dropped_bytes_total, 0);

    let noisy_summary = store.list().into_iter().find(|s| s.id == noisy).unwrap();
    assert_eq!(noisy_summary.buffered_bytes, 8);
    assert!(noisy_summary.dropped_bytes_total > 0);
}

// Breaks if: `with_output_bound` stops threading its argument through to
// `Session::new` (e.g. `Session::new(self.output_bound_bytes)` reverted
// to `Session::new(DEFAULT_OUTPUT_BUFFER_BYTES)`), which would make a
// configured-small-bound store behave like the 256 KiB default and never
// overflow the 100-byte append below.
#[test]
fn output_bound_is_configurable_at_construction() {
    let mut store = SessionStore::with_output_bound(8);
    let a = id("a");
    store.register(a.clone(), test_metadata());

    store.append_output(&a, &vec![b'z'; 100]).unwrap();

    let summary = store.list().into_iter().find(|s| s.id == a).unwrap();
    assert_eq!(summary.buffered_bytes, 8);
    assert_eq!(summary.dropped_bytes_total, 92);
}

// Breaks if: the `data.len() >= self.bound_bytes` branch's `keep_from`
// computation is flipped from `data.len() - self.bound_bytes` (keep the
// tail) to `0` (keep the head), which would retain the oldest bytes of
// the oversized append instead of the newest -- same length, wrong
// content, exactly the failure mode this test is written to catch.
#[test]
fn oversized_single_append_keeps_newest_tail_not_oldest_head() {
    let mut store = SessionStore::with_output_bound(4);
    let a = id("a");
    store.register(a.clone(), test_metadata());

    store.append_output(&a, b"abcdefgh").unwrap(); // 8 bytes, bound is 4

    let drained = store.drain_output(&a).unwrap();
    assert_eq!(drained.bytes, b"efgh");
    assert_eq!(drained.dropped_bytes_total, 4);
}

// Breaks if: the oversized-append branch stops counting the bytes that
// were already buffered before the flood (e.g. drops the
// `self.dropped_bytes += self.bytes.len() as u64;` line), which would
// undercount `dropped_bytes` by the 2 pre-existing bytes.
#[test]
fn oversized_append_counts_previously_buffered_bytes_as_dropped_too() {
    let mut store = SessionStore::with_output_bound(4);
    let a = id("a");
    store.register(a.clone(), test_metadata());
    store.append_output(&a, b"XY").unwrap(); // 2 bytes, under the bound

    store.append_output(&a, b"abcdefgh").unwrap(); // 8 bytes >= bound, supersedes "XY" too

    let drained = store.drain_output(&a).unwrap();
    assert_eq!(drained.bytes, b"efgh");
    assert_eq!(drained.dropped_bytes_total, 6); // 2 pre-existing + 4 dropped off the incoming 8
}

// Breaks if: `append_output` stops returning `Err` for an unregistered id
// (e.g. `ok_or(UnknownSession)?` replaced with silently ignoring the
// missing entry and returning `Ok(())`).
#[test]
fn appending_to_unknown_session_is_an_error() {
    let mut store = SessionStore::new();
    let ghost = id("ghost");
    assert_eq!(
        store.append_output(&ghost, b"x"),
        Err(UnknownSession(ghost.clone()))
    );
}

// Breaks if: `remove` stops reporting whether an entry actually existed
// (e.g. `self.sessions.remove(id); true` unconditionally), which would
// report `true` for the second, no-op removal below.
#[test]
fn removing_a_session_is_reported_and_is_final() {
    let mut store = SessionStore::new();
    let a = id("a");
    store.register(a.clone(), test_metadata());

    assert!(store.remove(&a));
    assert!(store.list().is_empty());
    assert!(!store.remove(&a));
}

// Breaks if: `list` swaps which ring-buffer accessor feeds which field
// (e.g. `buffered_bytes: session.output.dropped_bytes as usize` swapped
// with the `len()` call), which would report a running session as having
// buffered 0 bytes.
#[test]
fn list_reports_running_and_exited_sessions_with_byte_counts() {
    let mut store = SessionStore::new();
    let running = id("running");
    let exited = id("exited");
    store.register(running.clone(), test_metadata());
    store.register(exited.clone(), test_metadata());

    store.append_output(&running, b"abc").unwrap();
    store
        .record_exit(&exited, SessionExitStatus::exited(7))
        .unwrap();

    let summaries = store.list();
    assert_eq!(summaries.len(), 2);

    let running_summary = summaries.iter().find(|s| s.id == running).unwrap();
    assert_eq!(running_summary.state, SessionState::Running);
    assert_eq!(running_summary.buffered_bytes, 3);

    let exited_summary = summaries.iter().find(|s| s.id == exited).unwrap();
    assert_eq!(
        exited_summary.state,
        SessionState::Exited(SessionExitStatus::exited(7))
    );
    assert_eq!(exited_summary.buffered_bytes, 0);
}

// A session killed by a signal has NO exit code, and must not be reported as
// though it exited with one. `SignalSession` is one of the RPCs this store
// serves, so this is a state it has to represent rather than flatten into a
// sentinel -- the wire agrees, carrying `optional int32 exit_code`.
//
// Breaks if: `SessionExitStatus::code` goes back to a bare `i32`, or if
// `signalled()` invents a code (the `None` assertion fails).
#[test]
fn a_signalled_session_has_no_exit_code() {
    let mut store = SessionStore::with_output_bound(64);
    let a = id("a");
    store.register(a.clone(), test_metadata());
    store
        .record_exit(&a, SessionExitStatus::signalled())
        .unwrap();

    let summary = store.list().into_iter().next().unwrap();
    let SessionState::Exited(status) = summary.state else {
        panic!("a session with a recorded exit must report as exited");
    };
    assert_eq!(status.code, None, "a signalled session has no exit code");
    assert!(status.signal_killed);

    // And a normal exit stays distinguishable from it.
    let b = id("b");
    store.register(b.clone(), test_metadata());
    store.record_exit(&b, SessionExitStatus::exited(0)).unwrap();
    let b_summary = store.list().into_iter().find(|s| s.id == b).unwrap();
    assert_eq!(
        b_summary.state,
        SessionState::Exited(SessionExitStatus {
            code: Some(0),
            signal_killed: false
        }),
        "exit code 0 must not be conflated with a signalled exit"
    );
}

// `ListSessions` has to answer with `cwd`/`shell`/`rows`/`cols` -- the wire's
// `RemoteSessionSummary` requires them, and without retaining them the daemon
// could enumerate only ids and a reattaching client could not tell two sessions
// apart.
//
// Breaks if: `list` stops carrying `session.metadata`, or `register` drops the
// metadata it was given.
#[test]
fn list_reports_the_metadata_a_session_was_spawned_with() {
    let mut store = SessionStore::with_output_bound(64);
    let a = id("a");
    store.register(
        a.clone(),
        SessionSpawnMetadata {
            cwd: "/srv/build".to_string(),
            shell: Some("/bin/zsh".to_string()),
            rows: 40,
            cols: 120,
        },
    );

    let summary = store.list().into_iter().next().unwrap();
    assert_eq!(summary.metadata.cwd, "/srv/build");
    assert_eq!(summary.metadata.shell.as_deref(), Some("/bin/zsh"));
    assert_eq!((summary.metadata.rows, summary.metadata.cols), (40, 120));
}

// A reattaching client needs the size the pty has NOW, not the size it was
// spawned with, so a resize has to update what `ListSessions` reports.
//
// Breaks if: `record_resize` stops writing to `metadata`, or writes rows into
// cols (the assertion is on the ordered pair, so a transposition fails).
#[test]
fn a_resize_updates_the_reported_terminal_size() {
    let mut store = SessionStore::with_output_bound(64);
    let a = id("a");
    store.register(a.clone(), test_metadata()); // 24x80
    store.record_resize(&a, 50, 200).unwrap();

    let summary = store.list().into_iter().next().unwrap();
    assert_eq!(
        (summary.metadata.rows, summary.metadata.cols),
        (50, 200),
        "ListSessions must report the current size, not the spawn-time size"
    );
}

// Breaks if: `record_resize` stops returning `Err` for an unregistered id, or
// if `UnknownSession` stops carrying the id it was asked about.
#[test]
fn resizing_an_unknown_session_is_an_error_naming_the_id() {
    let mut store = SessionStore::with_output_bound(64);
    let ghost = id("ghost");

    assert_eq!(
        store.record_resize(&ghost, 10, 10),
        Err(UnknownSession(ghost.clone()))
    );
    assert!(
        UnknownSession(ghost.clone()).to_string().contains("ghost"),
        "the error must name the id, since it is logged away from the call site"
    );
}

// Registration is idempotent, and that must extend to metadata: a retried
// spawn after a lost response must not overwrite the live session's state with
// a second set of spawn arguments.
//
// Breaks if: `register` switches to an unconditional insert, or overwrites
// `metadata` on an existing entry.
#[test]
fn re_registering_does_not_overwrite_metadata_or_output() {
    let mut store = SessionStore::with_output_bound(64);
    let a = id("a");
    store.register(a.clone(), test_metadata()); // cwd /home/test, 24x80
    store.append_output(&a, b"first").unwrap();

    store.register(
        a.clone(),
        SessionSpawnMetadata {
            cwd: "/somewhere/else".to_string(),
            shell: None,
            rows: 1,
            cols: 1,
        },
    );

    let summary = store.list().into_iter().next().unwrap();
    assert_eq!(
        store.list().len(),
        1,
        "a retried spawn must not add a session"
    );
    assert_eq!(summary.metadata.cwd, "/home/test");
    assert_eq!((summary.metadata.rows, summary.metadata.cols), (24, 80));
    assert_eq!(
        store.drain_output(&a).unwrap().bytes,
        b"first",
        "a retried spawn must not discard buffered output"
    );
}
