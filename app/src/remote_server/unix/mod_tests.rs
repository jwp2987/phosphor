// Regression tests for the daemon writer/reader loop's disconnect
// classification, added alongside the port of
// d9dee18e19e8c06e24b7b32a9619685e5dd3289c (#10681, "Fix daemon message too
// big error") and 363d1d6e929df5ff23431eb2d2cf1018cb0009e9 (#10727,
// "Downgrade remote server SSH disconnect errors"). Neither upstream commit
// added automated tests (both were verified manually against real Sentry
// events / local runs per their PR descriptions), so these are fork-original,
// covering the pure predicates `handle_daemon_connection` gates its log
// severity on. NOT COMPILED: verified by reading only.

use super::*;
use std::io;

#[test]
fn is_disconnect_io_error_true_for_broken_pipe() {
    let e = io::Error::from(io::ErrorKind::BrokenPipe);
    assert!(is_disconnect_io_error(&e));
}

#[test]
fn is_disconnect_io_error_true_for_connection_reset() {
    let e = io::Error::from(io::ErrorKind::ConnectionReset);
    assert!(is_disconnect_io_error(&e));
}

#[test]
fn is_disconnect_io_error_true_for_connection_aborted() {
    let e = io::Error::from(io::ErrorKind::ConnectionAborted);
    assert!(is_disconnect_io_error(&e));
}

#[test]
fn is_disconnect_io_error_false_for_other_kinds() {
    // A representative sample of IO errors that are NOT routine disconnects
    // and must keep logging at `error!` severity.
    for kind in [
        io::ErrorKind::PermissionDenied,
        io::ErrorKind::TimedOut,
        io::ErrorKind::InvalidData,
        io::ErrorKind::Other,
    ] {
        let e = io::Error::from(kind);
        assert!(
            !is_disconnect_io_error(&e),
            "expected {kind:?} to not be classified as a disconnect"
        );
    }
}

#[test]
fn is_disconnect_error_true_for_wrapped_disconnect_io_error() {
    let e = remote_server::protocol::ProtocolError::Io(io::Error::from(io::ErrorKind::BrokenPipe));
    assert!(is_disconnect_error(&e));
}

#[test]
fn is_disconnect_error_false_for_wrapped_non_disconnect_io_error() {
    let e = remote_server::protocol::ProtocolError::Io(io::Error::from(
        io::ErrorKind::PermissionDenied,
    ));
    assert!(!is_disconnect_error(&e));
}

#[test]
fn is_disconnect_error_false_for_non_io_variants() {
    // UnexpectedEof and MessageTooLarge are framing-level conditions, not
    // socket-level disconnects, and must not be downgraded to `warn!`.
    assert!(!is_disconnect_error(
        &remote_server::protocol::ProtocolError::UnexpectedEof
    ));
    assert!(!is_disconnect_error(
        &remote_server::protocol::ProtocolError::MessageTooLarge { size: 1, max: 0 }
    ));
}

#[test]
fn is_disconnect_protocol_error_matches_is_disconnect_error() {
    // `is_disconnect_protocol_error` is documented as a plain alias used at
    // the write-loop call site for readability; it must never diverge from
    // `is_disconnect_error`.
    let disconnect =
        remote_server::protocol::ProtocolError::Io(io::Error::from(io::ErrorKind::BrokenPipe));
    let not_disconnect = remote_server::protocol::ProtocolError::UnexpectedEof;

    assert_eq!(
        is_disconnect_protocol_error(&disconnect),
        is_disconnect_error(&disconnect)
    );
    assert_eq!(
        is_disconnect_protocol_error(&not_disconnect),
        is_disconnect_error(&not_disconnect)
    );
}
