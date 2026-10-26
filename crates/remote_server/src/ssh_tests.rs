use super::*;

#[cfg(unix)]
#[test]
fn removes_leftover_control_socket() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("1234567890");
    // A SIGKILLed master leaves its socket inode behind after the listener is gone.
    drop(std::os::unix::net::UnixListener::bind(&socket).unwrap());
    assert!(socket.exists());
    remove_stale_control_socket(&socket);
    assert!(!socket.exists(), "stale ControlPath socket must be removed");
}

#[cfg(unix)]
#[test]
fn regular_file_is_not_removed() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("precious");
    std::fs::write(&file, b"data").unwrap();
    remove_stale_control_socket(&file);
    assert_eq!(std::fs::read(&file).unwrap(), b"data");
}

#[cfg(unix)]
#[test]
fn symlink_is_not_followed_or_removed() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target.sock");
    let _listener = std::os::unix::net::UnixListener::bind(&target).unwrap();
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    remove_stale_control_socket(&link);
    assert!(link.symlink_metadata().is_ok(), "symlink must remain");
    assert!(target.exists(), "symlink target must remain");
}

#[test]
fn missing_control_socket_is_not_an_error() {
    let dir = tempfile::tempdir().unwrap();
    // Must not panic when a graceful master already unlinked it.
    remove_stale_control_socket(&dir.path().join("absent"));
}
