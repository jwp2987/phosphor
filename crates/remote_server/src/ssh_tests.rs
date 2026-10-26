use super::*;

#[test]
fn removes_leftover_control_socket() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("1234567890");
    std::fs::write(&socket, b"").unwrap();
    remove_stale_control_socket(&socket);
    assert!(!socket.exists(), "stale ControlPath socket must be removed");
}

#[test]
fn missing_control_socket_is_not_an_error() {
    let dir = tempfile::tempdir().unwrap();
    // Must not panic when a graceful master already unlinked it.
    remove_stale_control_socket(&dir.path().join("absent"));
}
