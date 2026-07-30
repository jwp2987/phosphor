//! Regression tests for host-key verification (`verify_host_key` /
//! `check_and_pin_host_key`) — see finding #1: the SFTP client used to
//! establish sessions without any host-key check at all, silently accepting
//! a MITM'd connection.
//!
//! `ssh2::Session::known_hosts()` works on a session that never did a TCP
//! handshake (it's purely a local libssh2 data structure), so these tests
//! exercise the real `KnownHosts` Match/Mismatch/NotFound decision paths
//! without needing a live SSH server.

use super::*;

fn fresh_known_hosts() -> ssh2::Session {
    ssh2::Session::new().expect("session::new should not require a live connection")
}

const KEY_A: &[u8] = b"fake-rsa-host-key-bytes-AAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const KEY_B: &[u8] = b"fake-rsa-host-key-bytes-BBBBBBBBBBBBBBBBBBBBBBBBBBBB";

#[test]
fn matching_host_key_is_accepted() {
    let session = fresh_known_hosts();
    let mut known_hosts = session.known_hosts().unwrap();
    known_hosts
        .add("example.test", KEY_A, "example.test", ssh2::KnownHostKeyFormat::SshRsa)
        .unwrap();

    let result = check_and_pin_host_key(
        &mut known_hosts,
        "example.test",
        22,
        KEY_A,
        HostKeyType::Rsa,
        None,
    );
    assert!(result.is_ok(), "matching host key should be accepted: {result:?}");
}

/// The core regression test for finding #1: a host key that no longer
/// matches what's on record (e.g. because a MITM presented a different key)
/// must be rejected, not silently accepted.
#[test]
fn mismatched_host_key_is_rejected() {
    let session = fresh_known_hosts();
    let mut known_hosts = session.known_hosts().unwrap();
    known_hosts
        .add("example.test", KEY_A, "example.test", ssh2::KnownHostKeyFormat::SshRsa)
        .unwrap();

    let result = check_and_pin_host_key(
        &mut known_hosts,
        "example.test",
        22,
        KEY_B,
        HostKeyType::Rsa,
        None,
    );
    match result {
        Err(SftpError::HostKeyMismatch(msg)) => {
            assert!(msg.contains("example.test"), "error should mention the host: {msg}");
        }
        other => panic!("expected HostKeyMismatch, got {other:?}"),
    }
}

/// An unknown (never-seen-before) host key must not be silently trusted
/// forever: it's accepted for *this* connection (trust-on-first-use, like
/// OpenSSH's default interactive prompt would after the user says yes), but
/// it must be durably pinned so that a *future* connection with a different
/// key for the same host hits the Mismatch branch instead of NotFound again.
#[test]
fn unknown_host_key_is_pinned_for_future_connections() {
    let dir = tempfile::tempdir().unwrap();
    let known_hosts_path = dir.path().join("known_hosts");
    assert!(!known_hosts_path.exists());

    // First connection to a never-seen host: accepted (TOFU) and persisted.
    let session = fresh_known_hosts();
    let mut known_hosts = session.known_hosts().unwrap();
    let result = check_and_pin_host_key(
        &mut known_hosts,
        "new-host.test",
        22,
        KEY_A,
        HostKeyType::Rsa,
        Some(&known_hosts_path),
    );
    assert!(result.is_ok(), "first-use host key should be accepted: {result:?}");
    assert!(
        known_hosts_path.exists(),
        "the accepted key must be persisted to known_hosts so a future mismatch is detectable"
    );

    // A brand new session (simulating a later connection) loads the
    // persisted file and must now reject a *different* key for that host.
    let session2 = fresh_known_hosts();
    let mut known_hosts2 = session2.known_hosts().unwrap();
    known_hosts2
        .read_file(&known_hosts_path, KnownHostFileKind::OpenSSH)
        .unwrap();
    let result2 = check_and_pin_host_key(
        &mut known_hosts2,
        "new-host.test",
        22,
        KEY_B,
        HostKeyType::Rsa,
        Some(&known_hosts_path),
    );
    assert!(
        matches!(result2, Err(SftpError::HostKeyMismatch(_))),
        "a changed key for a previously-pinned host must be rejected, got {result2:?}"
    );
}
