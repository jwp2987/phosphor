use std::borrow::Cow;

use super::*;
use crate::terminal::SizeInfo;

fn host(name: &str) -> HostId {
    HostId::new(name.to_string())
}

fn session(name: &str) -> RemotePtySessionId {
    RemotePtySessionId::from(name.to_string())
}

// Breaks if: `outbound_rpc_for` stops carrying the bytes through, or starts
// interpreting them (trimming, re-encoding, splitting on newlines). Stdin is an
// opaque byte stream -- a paste, a Ctrl-C, a UTF-8 continuation byte arriving on
// its own -- and anything this layer "understands" it can also corrupt.
#[test]
fn input_becomes_a_verbatim_stdin_write() {
    let bytes = vec![0x03, b'l', b's', 0x0a, 0xf0];

    assert_eq!(
        outbound_rpc_for(Message::Input(Cow::Owned(bytes.clone()))),
        OutboundRpc::WriteStdin(bytes)
    );
}

// Rows and columns are both `u32` and adjacent in every signature they pass
// through (`resize_session(id, rows, cols)`, `ResizeSession { rows, cols }`), so
// a transposition compiles, type-checks, and produces a terminal whose geometry
// is silently wrong until someone runs a full-screen program. Asserted with
// deliberately unequal values, since equal ones would pass either way.
//
// Breaks if: `outbound_rpc_for` reads `size_info.columns` into `rows`, or
// `SizeInfo::new_without_font_metrics`'s own argument order is misread here.
#[test]
fn resize_keeps_rows_and_columns_the_right_way_round() {
    let size_info = SizeInfo::new_without_font_metrics(24, 80);

    assert_eq!(
        outbound_rpc_for(Message::Resize(size_info)),
        OutboundRpc::Resize { rows: 24, cols: 80 }
    );
}

// The contested decision, pinned so flipping it has to be deliberate.
//
// Note what this no longer rests on. The justification here used to be that
// `Message::Shutdown` arrives from `local_tty::TerminalManager::shutdown_event_loop`
// on app quit, so mapping it to a kill would destroy every remote session. That
// is false, and two refutation agents found it independently: that method sends
// on `local_tty`'s own `mio_channel`, neither remote manager has a `Drop`, and
// so no `Shutdown` reaches this transport at all today -- detach actually
// happens by channel close.
//
// The mapping is still right, for the reason requirement 4 gives, and is what a
// future `Shutdown` sender must get. But it is unexercised in production, which
// makes this test the only thing holding it.
//
// Breaks if: someone maps `Shutdown` onto `SignalSession`/`Kill`. The callers
// that want a kill have their own route now (`Message::Kill`, pinned by the
// test below), so there is no longer even a mis-served caller to justify it.
#[test]
fn shutdown_detaches_rather_than_killing_the_remote_session() {
    assert_eq!(outbound_rpc_for(Message::Shutdown), OutboundRpc::Detach);
}

// The sibling of the above, and the reason the two exist separately at all. A
// caller that has decided the shell must stop -- the autoupdate relaunch, a tab
// closed on a running command -- must reach the daemon's kill, not its detach,
// or the session comes back after the relaunch, which is the one thing the
// autoupdate path's own comment says must not happen.
//
// Breaks if: `Kill` is collapsed into `Shutdown` (the single-variant world this
// replaced), or routed to `Ignore` on the grounds that a daemon session always
// survives its client. The whole point is that this one does not.
#[test]
fn kill_ends_the_remote_session_rather_than_detaching_from_it() {
    assert_eq!(outbound_rpc_for(Message::Kill), OutboundRpc::Kill);
    assert_ne!(
        outbound_rpc_for(Message::Kill),
        outbound_rpc_for(Message::Shutdown),
        "the two teardown messages exist precisely to differ here"
    );
}

// Breaks if: `ChildExited` is forwarded as anything. It is a Windows-only device
// for telling a *local* event loop that its own child is gone; a daemon session
// learns about exits from `SessionExitedPush`, travelling the other way. Sending
// something here would mean inventing a message about a process this side never
// owned.
#[test]
fn child_exited_puts_nothing_on_the_wire() {
    assert_eq!(outbound_rpc_for(Message::ChildExited), OutboundRpc::Ignore);
}

// Both halves of the address are required, and each guards a different way of
// splicing one terminal's output into another.
//
// Breaks if: `is_for_session` compares only the session id (ids are
// client-minted, so two hosts can hold the same one, and one host's output would
// land in the other's terminal), or only the host (a host holds many sessions at
// once, so every session's output would land in every terminal on that host).
#[test]
fn output_is_addressed_by_host_and_session_together() {
    let this_host = host("build-box");
    let this_session = session("session-a");

    assert!(is_for_session(
        &this_host,
        &this_session,
        &this_host,
        &this_session
    ));
    assert!(
        !is_for_session(&host("other-box"), &this_session, &this_host, &this_session),
        "the same session id on a different host is a different session"
    );
    assert!(
        !is_for_session(&this_host, &session("session-b"), &this_host, &this_session),
        "a sibling session on the same host is a different session"
    );
    assert!(!is_for_session(
        &host("other-box"),
        &session("session-b"),
        &this_host,
        &this_session
    ));
}

// NOTE on what is deliberately NOT tested here: `ClientSlot`'s sharing across
// clones, and the send loop itself. Both need a live `RemoteServerClient`, which
// has no constructor outside `crates/remote_server`'s own duplex-backed test
// harness, and a test that asserts an empty slot stays empty would assert
// `None == None` -- coverage in name only, which is what `script/check_stub_coverage`
// exists to stop. The logic worth pinning was extracted into the two pure
// functions above precisely so it could be tested for real; what remains
// untested is the plumbing that carries their answers, and it is tested at the
// far end instead, in `server_model_tests.rs`.
