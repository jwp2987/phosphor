use super::*;

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

    store.register(a.clone());
    store.append_output(&a, b"hello").unwrap();
    store.register(a.clone());

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
    store.register(a.clone());

    store.append_output(&a, b"hello, ").unwrap();
    store.append_output(&a, b"world").unwrap();

    let drained = store.drain_output(&a).unwrap();
    assert_eq!(drained.bytes, b"hello, world");
    assert_eq!(drained.dropped_bytes, 0);
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
    store.register(a.clone());

    store
        .record_exit(&a, SessionExitStatus { code: 0 })
        .unwrap();
    store.append_output(&a, &vec![b'x'; 1000]).unwrap();

    let summary = store.list().into_iter().find(|s| s.id == a).unwrap();
    assert_eq!(
        summary.state,
        SessionState::Exited(SessionExitStatus { code: 0 })
    );
    assert_eq!(summary.buffered_bytes, 16);
}

// Breaks if: `drain` stops resetting `self.dropped_bytes = 0`, so a
// second drain would report the same dropped count as the first instead
// of zero.
#[test]
fn draining_clears_buffer_and_resets_dropped_count() {
    let mut store = SessionStore::with_output_bound(4);
    let a = id("a");
    store.register(a.clone());
    store.append_output(&a, b"abcdefgh").unwrap(); // 8 bytes into a 4-byte bound

    let first = store.drain_output(&a).unwrap();
    assert_eq!(first.bytes, b"efgh");
    assert_eq!(first.dropped_bytes, 4);

    let second = store.drain_output(&a).unwrap();
    assert_eq!(second.bytes, Vec::<u8>::new());
    assert_eq!(second.dropped_bytes, 0);
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
    store.register(noisy.clone());
    store.register(quiet.clone());

    store.append_output(&noisy, &vec![b'n'; 1000]).unwrap();
    store.append_output(&quiet, b"ok").unwrap();

    let quiet_summary = store.list().into_iter().find(|s| s.id == quiet).unwrap();
    assert_eq!(quiet_summary.buffered_bytes, 2);
    assert_eq!(quiet_summary.dropped_bytes, 0);

    let noisy_summary = store.list().into_iter().find(|s| s.id == noisy).unwrap();
    assert_eq!(noisy_summary.buffered_bytes, 8);
    assert!(noisy_summary.dropped_bytes > 0);
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
    store.register(a.clone());

    store.append_output(&a, &vec![b'z'; 100]).unwrap();

    let summary = store.list().into_iter().find(|s| s.id == a).unwrap();
    assert_eq!(summary.buffered_bytes, 8);
    assert_eq!(summary.dropped_bytes, 92);
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
    store.register(a.clone());

    store.append_output(&a, b"abcdefgh").unwrap(); // 8 bytes, bound is 4

    let drained = store.drain_output(&a).unwrap();
    assert_eq!(drained.bytes, b"efgh");
    assert_eq!(drained.dropped_bytes, 4);
}

// Breaks if: the oversized-append branch stops counting the bytes that
// were already buffered before the flood (e.g. drops the
// `self.dropped_bytes += self.bytes.len() as u64;` line), which would
// undercount `dropped_bytes` by the 2 pre-existing bytes.
#[test]
fn oversized_append_counts_previously_buffered_bytes_as_dropped_too() {
    let mut store = SessionStore::with_output_bound(4);
    let a = id("a");
    store.register(a.clone());
    store.append_output(&a, b"XY").unwrap(); // 2 bytes, under the bound

    store.append_output(&a, b"abcdefgh").unwrap(); // 8 bytes >= bound, supersedes "XY" too

    let drained = store.drain_output(&a).unwrap();
    assert_eq!(drained.bytes, b"efgh");
    assert_eq!(drained.dropped_bytes, 6); // 2 pre-existing + 4 dropped off the incoming 8
}

// Breaks if: `append_output` stops returning `Err` for an unregistered id
// (e.g. `ok_or(UnknownSession)?` replaced with silently ignoring the
// missing entry and returning `Ok(())`).
#[test]
fn appending_to_unknown_session_is_an_error() {
    let mut store = SessionStore::new();
    let ghost = id("ghost");
    assert_eq!(store.append_output(&ghost, b"x"), Err(UnknownSession));
}

// Breaks if: `remove` stops reporting whether an entry actually existed
// (e.g. `self.sessions.remove(id); true` unconditionally), which would
// report `true` for the second, no-op removal below.
#[test]
fn removing_a_session_is_reported_and_is_final() {
    let mut store = SessionStore::new();
    let a = id("a");
    store.register(a.clone());

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
    store.register(running.clone());
    store.register(exited.clone());

    store.append_output(&running, b"abc").unwrap();
    store
        .record_exit(&exited, SessionExitStatus { code: 7 })
        .unwrap();

    let summaries = store.list();
    assert_eq!(summaries.len(), 2);

    let running_summary = summaries.iter().find(|s| s.id == running).unwrap();
    assert_eq!(running_summary.state, SessionState::Running);
    assert_eq!(running_summary.buffered_bytes, 3);

    let exited_summary = summaries.iter().find(|s| s.id == exited).unwrap();
    assert_eq!(
        exited_summary.state,
        SessionState::Exited(SessionExitStatus { code: 7 })
    );
    assert_eq!(exited_summary.buffered_bytes, 0);
}
