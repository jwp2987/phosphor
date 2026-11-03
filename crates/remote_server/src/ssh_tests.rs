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

/// #1083: every `ssh -O` Phosphor issues carries the guards that make a fallback login
/// impossible, ignores the user's config, and names only the placeholder destination.
#[test]
fn mux_control_args_can_never_fall_back_to_a_login() {
    let socket = Path::new("/home/u/.ssh/123");
    for op in ["exit", "check", "stop"] {
        let args = mux_control_args(op, socket);
        let joined = args.join(" ");
        // No user or system config (ControlMaster yes / ProxyJump / Host * cannot apply).
        assert_eq!(&args[..2], ["-F", "/dev/null"], "{joined}");
        for option in [
            "ControlMaster=no",
            "ControlPath=/home/u/.ssh/123",
            "ProxyCommand=false",
            "BatchMode=yes",
            "ConnectTimeout=5",
            "ClearAllForwardings=yes",
        ] {
            let at = args
                .iter()
                .position(|a| a == option)
                .unwrap_or_else(|| panic!("{op}: missing {option}: {joined}"));
            assert_eq!(
                args[at - 1],
                "-o",
                "{op}: {option} must be an -o value: {joined}"
            );
        }
        assert!(args.iter().any(|a| a == "-n"), "{joined}");
        let o = args.iter().position(|a| a == "-O").unwrap();
        assert_eq!(args[o + 1], op);
        // The destination is last and is the fixed placeholder; nothing after it (a remote
        // command), and no other destination anywhere.
        assert_eq!(args.last().unwrap(), MUX_CONTROL_DESTINATION);
        assert!(!args.iter().any(|a| a.contains('@')), "{joined}");
        // Exactly one ProxyCommand and it is first-wins on the command line (no -J allowed).
        assert_eq!(
            args.iter()
                .filter(|a| a.starts_with("ProxyCommand="))
                .count(),
            1
        );
        assert!(!args.iter().any(|a| a == "-J"), "{joined}");
    }
}

/// #1083: `stop_control_master` actually runs the guarded command (not a hand-rolled one).
#[test]
fn stop_control_master_uses_the_guarded_args() {
    let source = include_str!("ssh.rs");
    let body = &source[source.find("async fn stop_control_master_inner").unwrap()..];
    let body = &body[..body.find("match result").unwrap()];
    assert!(
        body.contains("mux_control_args(\"exit\", socket_path)")
            && body.contains("Stdio::null()")
            && body.contains(".env(\"SHELL\", \"/bin/sh\")"),
        "{body}"
    );
    assert!(!body.contains("ssh_args("), "{body}");
}

/// #1100: the app's background mux clients (`run_ssh_command`, `run_ssh_script`, the
/// remote-server proxy, `RemoteCommandExecutor`) must carry the same fallback guards as
/// `mux_control_args`, so a dead master fails fast instead of opening a real connection.
#[test]
fn ssh_args_can_never_fall_back_to_a_login() {
    let args = ssh_args(Path::new("/home/u/.ssh/123"));
    let joined = args.join(" ");
    assert_eq!(&args[..2], ["-F", "/dev/null"], "{joined}");
    for option in [
        "ControlMaster=no",
        "ControlPath=/home/u/.ssh/123",
        "ProxyCommand=false",
        "BatchMode=yes",
        "ConnectTimeout=5",
        "ClearAllForwardings=yes",
    ] {
        let at = args
            .iter()
            .position(|a| a == option)
            .unwrap_or_else(|| panic!("missing {option}: {joined}"));
        assert_eq!(args[at - 1], "-o", "{option} must be an -o value: {joined}");
    }
    assert_eq!(args.last().unwrap(), "placeholder@placeholder");
    assert!(!args.iter().any(|a| a == "-J"), "{joined}");
}

/// #1100: a socket path with whitespace is quoted (ssh splits `-o` values on whitespace) and
/// `%` is escaped (ssh expands `%` tokens in ControlPath).
#[test]
fn control_path_option_quotes_whitespace_and_escapes_percent() {
    assert_eq!(
        control_path_option(Path::new("/home/a b/.ssh/1")),
        "ControlPath=\"/home/a b/.ssh/1\""
    );
    assert_eq!(
        control_path_option(Path::new("/home/u/.ssh/1")),
        "ControlPath=/home/u/.ssh/1"
    );
    assert_eq!(
        control_path_option(Path::new("/home/u%h/.ssh/1")),
        "ControlPath=/home/u%%h/.ssh/1"
    );
    assert!(
        mux_control_args("check", Path::new("/h/a b/s"))
            .contains(&"ControlPath=\"/h/a b/s\"".to_string())
    );
}

/// #1100: every spawn of ssh/scp in this module goes through a guarded arg list.
#[test]
fn every_ssh_spawn_is_guarded() {
    let source = include_str!("ssh.rs");
    let code = &source[..source.find("#[cfg(test)]").unwrap()];
    assert_eq!(code.matches(".args(ssh_args(socket_path))").count(), 2);
    let scp = &code[code.find("Command::new(\"scp\")").unwrap()..];
    assert!(
        scp.contains("\"/dev/null\"") && scp.contains("ProxyCommand=false"),
        "scp unguarded"
    );
}
