use super::*;

#[test]
fn pipe_names_have_the_documented_format() {
    let name = generate_pipe_name();
    let hex = name.strip_prefix(PIPE_NAME_PREFIX).expect("prefix");
    assert_eq!(hex.len(), 32, "128 bits as hex");
    assert!(
        hex.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
}

#[test]
fn pipe_names_are_random_and_not_derived_from_the_session_id() {
    let names: std::collections::HashSet<_> = (0..64).map(|_| generate_pipe_name()).collect();
    assert_eq!(names.len(), 64);

    let session = 8_200_001u64;
    let name = pipe_name_for_session(session);
    assert!(!name.contains(&session.to_string()));
    assert!(!name.contains(&format!("{session:x}")));
    // Stable for the session, different for another one.
    assert_eq!(pipe_name_for_session(session), name);
    assert_eq!(lookup_pipe_name(session).as_deref(), Some(name.as_str()));
    assert_ne!(pipe_name_for_session(session + 1), name);
    assert_eq!(lookup_pipe_name(8_200_999), None);
}

#[test]
fn the_init_script_gets_a_name_only_on_windows() {
    let value = init_script_value(8_200_010);
    if cfg!(windows) {
        assert!(value.starts_with(PIPE_NAME_PREFIX));
    } else {
        assert!(value.is_empty());
        assert_eq!(lookup_pipe_name(8_200_010), None);
    }
}

/// Runs only in the Windows CI job.
#[cfg(windows)]
#[test]
fn the_pipe_delivers_the_command_once_to_one_client_and_refuses_a_second() {
    use std::io::Read as _;

    let name = generate_pipe_name();
    let path = format!(r"\\.\pipe\{name}");
    let command = "Get-ChildItem \u{e9}\u{4e16}";
    let server = CommandPipe::serve(&name, command).expect("create the pipe");

    // The name is taken while the instance exists (first instance, max one).
    assert!(CommandPipe::serve(&name, "other").is_none());

    // The first client connects and reads exactly the command; there is no BOM.
    let mut first = std::fs::OpenOptions::new()
        .read(true)
        .open(&path)
        .expect("first client");
    // A second client is refused while the first holds the only instance.
    assert!(std::fs::OpenOptions::new().read(true).open(&path).is_err());

    let mut bytes = Vec::new();
    first.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, command.as_bytes());
    drop(first);
    drop(server);

    // Delivered and closed: nothing is left to connect to.
    assert!(std::fs::OpenOptions::new().read(true).open(&path).is_err());
}

/// Runs only in the Windows CI job.
#[cfg(windows)]
#[test]
fn dropping_an_unconnected_pipe_frees_the_name() {
    let name = generate_pipe_name();
    let server = CommandPipe::serve(&name, "x").expect("create");
    drop(server);
    let again = CommandPipe::serve(&name, "y").expect("the name is free again");
    drop(again);
}
