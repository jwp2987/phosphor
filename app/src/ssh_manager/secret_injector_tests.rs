//! Regression tests for finding #6: the SSH password auto-injector must not
//! fire once login has already completed, even if later PTY output happens
//! to resemble a password prompt (e.g. `cat`ing a file with a literal
//! "Password:" line, or some unrelated interactive program printing text
//! that matches the pattern).
//!
//! `watch_for_prompt` is a plain async fn over an `InactiveReceiver`, so it
//! can be driven directly with `warpui::r#async::block_on` without needing
//! the full `ViewContext`/`TerminalView` machinery.
//!
//! ## A subtlety these tests must respect
//!
//! `InactiveReceiver::activate_cloned()` (called internally by
//! `watch_for_prompt` as its very first step) positions the new receiver at
//! the *current queue tail* — see its doc example in the `async-broadcast`
//! crate, which explicitly broadcasts *after* activating, not before. Two
//! consequences for a naive test:
//!
//! - Broadcasting all fixture chunks *before* calling `watch_for_prompt`
//!   would silently deliver nothing (the freshly-activated receiver starts
//!   reading from "now", past everything already queued) — the test would
//!   still pass for assertions expecting `false`, but for the wrong reason
//!   (nothing was ever observed, not "the login-completed guard worked").
//! - Separately, `Sender::broadcast()` defaults to `await_active: true`: if
//!   zero *active* receivers exist yet (only the not-yet-activated
//!   `InactiveReceiver`), the send future blocks — not errors — until an
//!   active receiver shows up. Awaiting all the sends to completion before
//!   ever calling `watch_for_prompt` therefore hangs forever.
//!
//! Both are solved the same way: broadcast the fixture chunks and run
//! `watch_for_prompt` *concurrently* (via `futures_lite::future::zip`)
//! instead of sequentially. `watch_for_prompt`'s synchronous
//! `activate_cloned()` prefix runs as soon as it's first polled, which both
//! unblocks the `await_active`-blocked sends (activation notifies waiting
//! senders) and puts the receiver's position at the right place to observe
//! everything broadcast from that point on.

use super::*;
use futures_lite::future::zip;

fn make_channel() -> (
    async_broadcast::Sender<Arc<Vec<u8>>>,
    InactiveReceiver<Arc<Vec<u8>>>,
) {
    let (tx, rx) = async_broadcast::broadcast(16);
    (tx, rx.deactivate())
}

#[test]
fn injects_on_genuine_password_prompt() {
    let (tx, rx) = make_channel();
    warpui::r#async::block_on(async {
        let send = async {
            tx.broadcast(Arc::new(b"user@host's password: ".to_vec()))
                .await
                .unwrap();
            drop(tx);
        };
        let (_, fired) = zip(send, watch_for_prompt(rx)).await;
        assert!(fired, "a real password prompt must trigger injection");
    });
}

#[test]
fn eof_without_any_prompt_returns_false() {
    let (tx, rx) = make_channel();
    drop(tx);
    warpui::r#async::block_on(async {
        assert!(!watch_for_prompt(rx).await);
    });
}

/// Core regression test for finding #6: once a shell prompt has been
/// observed (login already completed — e.g. passwordless key auth, so no
/// password prompt was ever shown), later text that merely *looks like* a
/// password prompt must NOT trigger injection into the now-authenticated
/// shell.
#[test]
fn fake_password_prompt_after_shell_prompt_does_not_trigger_injection() {
    // Sanity check: this exact bait text, in isolation, DOES match the
    // password-prompt regex — proving that what stops injection below is
    // the login-completed guard, not merely a non-matching pattern.
    let bait = b"Enter password: ";
    assert!(bytes_look_like_password_prompt(bait));

    let (tx, rx) = make_channel();
    warpui::r#async::block_on(async {
        let send = async {
            // Banner, then login completes silently (key auth) ...
            tx.broadcast(Arc::new(b"Last login: Mon Jan 1 2026 from 10.0.0.1\n".to_vec()))
                .await
                .unwrap();
            tx.broadcast(Arc::new(b"$ ".to_vec())).await.unwrap();
            // ... then something in the now-authenticated shell prints text
            // that looks like a password prompt.
            tx.broadcast(Arc::new(bait.to_vec())).await.unwrap();
            drop(tx);
        };

        let (_, fired) = zip(send, watch_for_prompt(rx)).await;
        assert!(
            !fired,
            "must not inject once a shell prompt shows login already completed"
        );
    });
}

/// Same shape, but the shell prompt and the bait both land in a single PTY
/// read chunk (e.g. a fast local `cat` can plausibly interleave this way
/// within one PTY read) — the shell-prompt check must win even within one
/// chunk, not just across separate chunks.
#[test]
fn fake_password_prompt_in_same_chunk_as_shell_prompt_does_not_trigger_injection() {
    let buf: &[u8] = b"$ cat notes.txt\nPassword:\n$ ";
    // Sanity: the password-prompt regex genuinely matches somewhere in this
    // buffer, and the shell-prompt regex genuinely matches its tail — the
    // fix's ordering (shell-prompt checked first) is what decides the
    // outcome here, not a pattern that never matched in the first place.
    assert!(bytes_look_like_password_prompt(buf));
    assert!(bytes_look_like_shell_prompt(buf));

    let (tx, rx) = make_channel();
    warpui::r#async::block_on(async {
        let send = async {
            tx.broadcast(Arc::new(buf.to_vec())).await.unwrap();
            drop(tx);
        };
        let (_, fired) = zip(send, watch_for_prompt(rx)).await;
        assert!(!fired);
    });
}

/// Direct proof that the concurrent-activation plumbing above actually
/// delivers messages (i.e. that a passing "must not fire" test isn't
/// silently vacuous because the receiver saw nothing at all): the same
/// send/zip pattern, with fixture text that should reach the injector and
/// be plainly visible before any prompt-matching logic even runs.
#[test]
fn zipped_send_is_actually_observed_by_the_receiver() {
    let (tx, rx) = make_channel();
    warpui::r#async::block_on(async {
        let mut active = rx.activate_cloned();
        let send = async {
            tx.broadcast(Arc::new(b"hello".to_vec())).await.unwrap();
        };
        let (_, received) = zip(send, active.recv()).await;
        assert_eq!(received.unwrap().as_slice(), b"hello");
    });
}
