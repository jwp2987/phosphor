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

    let peeked = store.peek_output(&a).unwrap();
    assert_eq!(peeked.bytes, b"hello");
}

// Breaks if: `append` starts prepending instead of extending (e.g.
// `self.bytes.push_front` in a loop instead of `self.bytes.extend`),
// which would preserve the length but reverse the byte order.
#[test]
fn append_then_peek_returns_exact_bytes_in_order() {
    let mut store = SessionStore::new();
    let a = id("a");
    store.register(a.clone(), test_metadata());

    store.append_output(&a, b"hello, ").unwrap();
    store.append_output(&a, b"world").unwrap();

    let peeked = store.peek_output(&a).unwrap();
    assert_eq!(peeked.bytes, b"hello, world");
    assert_eq!(peeked.dropped_bytes_total, 0);
    assert_eq!(peeked.dropped_bytes_since_ack, 0);
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

// `peek_output` must not consume anything: calling it twice in a row, with
// nothing else happening in between, must return identical results both
// times.
//
// Breaks if: `OutputRingBuffer::peek` is changed back to draining (e.g.
// `self.bytes.drain(..).collect()` instead of `self.bytes.iter().copied()`),
// which would make the second call see an empty buffer.
#[test]
fn peek_output_is_non_destructive_repeated_peeks_return_the_same_bytes() {
    let mut store = SessionStore::new();
    let a = id("a");
    store.register(a.clone(), test_metadata());
    store.append_output(&a, b"hello").unwrap();

    let first = store.peek_output(&a).unwrap();
    let second = store.peek_output(&a).unwrap();

    assert_eq!(first, second);
    assert_eq!(first.bytes, b"hello");
}

// The entire point of two-phase delivery: a caller that peeks, and then
// never acknowledges (the send failed, or it crashed before flush), must
// not have lost the output. A later peek -- possibly after more output has
// arrived -- must still see everything.
//
// Breaks if: `peek_output` (or `OutputRingBuffer::peek`) discards what it
// read, e.g. by calling `self.bytes.drain(..)` instead of reading through
// an iterator.
#[test]
fn bytes_survive_a_peek_that_is_never_acknowledged() {
    let mut store = SessionStore::new();
    let a = id("a");
    store.register(a.clone(), test_metadata());
    store.append_output(&a, b"hello").unwrap();

    let first = store.peek_output(&a).unwrap();
    assert_eq!(first.bytes, b"hello");

    // No acknowledge_output call here -- simulating a failed delivery.
    store.append_output(&a, b" world").unwrap();

    let second = store.peek_output(&a).unwrap();
    assert_eq!(
        second.bytes, b"hello world",
        "output peeked but never acknowledged must not be lost"
    );
}

// Acknowledging must discard exactly the delivered prefix, from the oldest
// end, and leave the rest -- not clear the whole buffer and not discard
// from the wrong end.
//
// Breaks if: `OutputRingBuffer::acknowledge` drains from the back instead of
// the front (e.g. `self.bytes.drain(self.bytes.len() - discard..)`), which
// would leave "abc" instead of "defgh"; or if it clears the whole buffer
// regardless of the acknowledged offset.
#[test]
fn acknowledge_discards_exactly_the_delivered_prefix_and_leaves_the_rest() {
    let mut store = SessionStore::with_output_bound(64);
    let a = id("a");
    store.register(a.clone(), test_metadata());
    store.append_output(&a, b"abcdefgh").unwrap();

    store.acknowledge_output(&a, 3).unwrap();

    let peeked = store.peek_output(&a).unwrap();
    assert_eq!(peeked.bytes, b"defgh");

    store.append_output(&a, b"ij").unwrap();
    let peeked = store.peek_output(&a).unwrap();
    assert_eq!(peeked.bytes, b"defghij");
}

// The lifetime dropped-byte total must never go down, but the
// "since acknowledged" figure must reset to 0 the moment a delivery is
// acknowledged, then start climbing again as more is dropped after that.
//
// Breaks if: `acknowledge` resets `self.dropped_bytes` instead of
// `self.acknowledged_dropped_bytes` (the lifetime total would wrongly drop
// to 0), or if `peek`'s `dropped_bytes_since_ack` is computed some way other
// than `dropped_bytes - acknowledged_dropped_bytes` that fails to reset.
#[test]
fn dropped_bytes_since_ack_resets_on_acknowledgement_but_total_does_not() {
    let mut store = SessionStore::with_output_bound(4);
    let a = id("a");
    store.register(a.clone(), test_metadata());
    store.append_output(&a, b"abcdefgh").unwrap(); // 8 into a 4-byte bound: drops 4

    let before_ack = store.peek_output(&a).unwrap();
    assert_eq!(before_ack.bytes, b"efgh");
    assert_eq!(before_ack.dropped_bytes_total, 4);
    assert_eq!(before_ack.dropped_bytes_since_ack, 4);

    // "efgh" spans stream offsets 4..8, so delivering all of it acknowledges
    // through offset 8 -- not 4, which is where those bytes START.
    store
        .acknowledge_output(&a, before_ack.next_offset)
        .unwrap();

    let after_ack = store.peek_output(&a).unwrap();
    assert_eq!(after_ack.bytes, Vec::<u8>::new());
    assert_eq!(
        after_ack.dropped_bytes_total, 4,
        "the lifetime total must survive an acknowledgement"
    );
    assert_eq!(
        after_ack.dropped_bytes_since_ack, 0,
        "the since-acknowledged figure must reset"
    );

    store.append_output(&a, b"ijklmnop").unwrap(); // another 8 into 4: drops 4 more

    let later = store.peek_output(&a).unwrap();
    assert_eq!(later.bytes, b"mnop");
    assert_eq!(
        later.dropped_bytes_total, 8,
        "4 from before the ack + 4 from after"
    );
    assert_eq!(
        later.dropped_bytes_since_ack, 4,
        "only the post-ack drop should count now"
    );
}

// A `delivered_bytes` larger than what remains buffered must be clamped, not
// treated as an error and not allowed to corrupt the watermark: it must
// discard everything present (no more, since there is no more) and still
// advance `acknowledged_dropped_bytes` correctly, so later arithmetic stays
// consistent instead of drifting or underflowing.
//
// Breaks if: `acknowledge` computes `discard` as `delivered_bytes` without
// the `.min(self.bytes.len())` clamp (this would panic in
// `VecDeque::drain` on an out-of-range end), or if it skips updating the
// watermark when the acknowledged offset overshoots what is buffered.
#[test]
fn acknowledging_more_than_buffered_clamps_without_panicking_or_corrupting_watermark() {
    let mut store = SessionStore::with_output_bound(4);
    let a = id("a");
    store.register(a.clone(), test_metadata());
    store.append_output(&a, b"abcdefgh").unwrap(); // drops 4, buffers "efgh"

    store.acknowledge_output(&a, 100).unwrap(); // far more than the 4 buffered

    let peeked = store.peek_output(&a).unwrap();
    assert_eq!(
        peeked.bytes,
        Vec::<u8>::new(),
        "an over-large ack discards everything present, no more"
    );
    assert_eq!(peeked.dropped_bytes_total, 4);
    assert_eq!(peeked.dropped_bytes_since_ack, 0);

    // Watermark arithmetic must still be correct afterward: append within
    // the bound (no new drop), then append enough to force a real drop, and
    // check the since-ack figure reflects only the new drop.
    store.append_output(&a, b"xyz").unwrap(); // 3 bytes, no overflow
    let mid = store.peek_output(&a).unwrap();
    assert_eq!(mid.bytes, b"xyz");
    assert_eq!(mid.dropped_bytes_total, 4);
    assert_eq!(mid.dropped_bytes_since_ack, 0);

    store.append_output(&a, b"12345").unwrap(); // 3 + 5 = 8 into bound 4: drops 4 more
    let later = store.peek_output(&a).unwrap();
    assert_eq!(
        later.dropped_bytes_total, 8,
        "the clamp must not have corrupted the lifetime total"
    );
    assert_eq!(
        later.dropped_bytes_since_ack, 4,
        "the watermark set by the clamped ack must still be the correct base"
    );
}

// Acknowledging zero bytes must not discard anything -- it is a real,
// meaningful call (e.g. "I delivered a gap marker with no bytes behind it"),
// not an error, and not equivalent to acknowledging everything.
//
// Breaks if: `acknowledge` treats `delivered_bytes == 0` as "acknowledge
// everything" (e.g. `if through_offset == 0 { self.bytes.clear() }`), which
// would empty the buffer instead of leaving it untouched.
#[test]
fn acknowledging_offset_zero_discards_nothing_but_still_advances_the_watermark() {
    let mut store = SessionStore::with_output_bound(4);
    let a = id("a");
    store.register(a.clone(), test_metadata());
    store.append_output(&a, b"abcdefgh").unwrap(); // drops 4, buffers "efgh"

    store.acknowledge_output(&a, 0).unwrap();

    let peeked = store.peek_output(&a).unwrap();
    assert_eq!(
        peeked.bytes, b"efgh",
        "acknowledging through offset 0 must not discard buffered output"
    );
    assert_eq!(peeked.dropped_bytes_total, 4);
    assert_eq!(
        peeked.dropped_bytes_since_ack, 0,
        "a 0-byte delivery still retires the gap it reported"
    );
}

// Acknowledging a session with nothing buffered must be a harmless no-op on
// the bytes (there is nothing to discard), not a panic and not an error.
//
// Breaks if: `acknowledge` indexes into the buffer without bounds-checking
// (e.g. `self.bytes.drain(..delivered_bytes)` with no `.min(len())`), which
// would panic when the acknowledged offset is past an empty buffer.
#[test]
fn acknowledging_a_session_with_nothing_buffered_is_harmless() {
    let mut store = SessionStore::new();
    let a = id("a");
    store.register(a.clone(), test_metadata());

    store.acknowledge_output(&a, 5).unwrap();

    let peeked = store.peek_output(&a).unwrap();
    assert_eq!(peeked.bytes, Vec::<u8>::new());
    assert_eq!(peeked.dropped_bytes_total, 0);
    assert_eq!(peeked.dropped_bytes_since_ack, 0);
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

    let peeked = store.peek_output(&a).unwrap();
    assert_eq!(peeked.bytes, b"efgh");
    assert_eq!(peeked.dropped_bytes_total, 4);
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

    let peeked = store.peek_output(&a).unwrap();
    assert_eq!(peeked.bytes, b"efgh");
    assert_eq!(peeked.dropped_bytes_total, 6); // 2 pre-existing + 4 dropped off the incoming 8
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

// Breaks if: `peek_output` stops returning `Err` for an unregistered id
// (e.g. it returns a default-empty `PeekedOutput` instead of propagating
// `UnknownSession`).
#[test]
fn peeking_an_unknown_session_is_an_error() {
    let store = SessionStore::new();
    let ghost = id("ghost");
    assert_eq!(
        store.peek_output(&ghost),
        Err(UnknownSession(ghost.clone()))
    );
}

// Breaks if: `acknowledge_output` stops returning `Err` for an unregistered
// id (e.g. it silently no-ops instead of propagating `UnknownSession`).
#[test]
fn acknowledging_an_unknown_session_is_an_error() {
    let mut store = SessionStore::new();
    let ghost = id("ghost");
    assert_eq!(
        store.acknowledge_output(&ghost, 5),
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
        store.peek_output(&a).unwrap().bytes,
        b"first",
        "a retried spawn must not discard buffered output"
    );
}

// THE reason acknowledgement is addressed by offset and not by a count of
// delivered bytes.
//
// A peek and its acknowledgement are two separate calls. If output arrives in
// between and overflows the bound, bytes leave the OLDEST end -- so the byte at
// buffer index 0 is no longer the byte that was at index 0 when the caller
// peeked. "Discard the first N" would then discard N bytes starting from the
// wrong byte, silently destroying output that was never delivered: exactly the
// loss this two-phase design exists to prevent, reintroduced by the
// acknowledgement itself.
//
// Breaks if: `acknowledge` goes back to `delivered_bytes.min(len)` and discards
// a prefix by count. With that implementation this test discards "bcd" and
// leaves "e", losing the undelivered "d".
#[test]
fn an_append_between_peek_and_acknowledge_does_not_discard_undelivered_bytes() {
    let mut store = SessionStore::with_output_bound(4);
    let a = id("a");
    store.register(a.clone(), test_metadata());
    store.append_output(&a, b"abc").unwrap();

    // The caller peeks "abc" (offsets 0..3) and begins delivering it.
    let peeked = store.peek_output(&a).unwrap();
    assert_eq!(peeked.bytes, b"abc");
    assert_eq!((peeked.first_offset, peeked.next_offset), (0, 3));

    // Meanwhile the pty emits more, overflowing the 4-byte bound and evicting
    // "a" from the front. The buffer is now "bcde" -- index 0 is "b", not "a".
    store.append_output(&a, b"de").unwrap();

    // The delivery of "abc" completes, acknowledging through offset 3.
    store.acknowledge_output(&a, peeked.next_offset).unwrap();

    let after = store.peek_output(&a).unwrap();
    assert_eq!(
        after.bytes, b"de",
        "only the delivered bytes may be discarded; \"d\" and \"e\" were never delivered"
    );
    assert_eq!(
        after.dropped_bytes_total, 1,
        "one byte (\"a\") was evicted by the bound, and that is the only loss"
    );
}

// A stale acknowledgement -- one whose bytes have ALREADY been evicted by the
// bound -- must discard nothing rather than wrap around and take live bytes.
//
// Breaks if: the `saturating_sub(self.first_retained_offset())` in
// `acknowledge` becomes a plain subtraction (underflow panics in debug, wraps
// to an enormous value in release, which then clamps to the buffer length and
// discards the entire live buffer).
#[test]
fn an_acknowledgement_whose_bytes_were_already_evicted_discards_nothing() {
    let mut store = SessionStore::with_output_bound(4);
    let a = id("a");
    store.register(a.clone(), test_metadata());
    store.append_output(&a, b"ab").unwrap();

    let peeked = store.peek_output(&a).unwrap(); // offsets 0..2
    assert_eq!(peeked.next_offset, 2);

    // Everything peeked is evicted before the acknowledgement lands.
    store.append_output(&a, b"cdefgh").unwrap(); // total 8, bound 4 -> buffers "efgh"
    assert_eq!(store.peek_output(&a).unwrap().bytes, b"efgh");

    store.acknowledge_output(&a, peeked.next_offset).unwrap();

    assert_eq!(
        store.peek_output(&a).unwrap().bytes,
        b"efgh",
        "a stale acknowledgement must not discard bytes that outlived it"
    );
}
