use super::*;

#[test]
fn short_commands_are_typed() {
    assert!(!needs_file(&"a".repeat(MAX_TYPED_BYTES)));
    assert!(needs_file(&"a".repeat(MAX_TYPED_BYTES + 1)));
}

#[test]
fn launcher_line_doubles_every_quote_powershell_accepts() {
    assert_eq!(
        launcher_line(r"C:\a b\x.ps1"),
        r". (Warp-Run-LongCommand 'C:\a b\x.ps1')"
    );
    assert_eq!(
        launcher_line("C:\\it's\\\u{2019}x.ps1"),
        ". (Warp-Run-LongCommand 'C:\\it''s\\\u{2019}\u{2019}x.ps1')"
    );
}

#[test]
fn a_long_command_becomes_a_short_line_and_a_file_with_the_full_text() {
    for len in [8_000, 100_000, 1_000_000] {
        let command = format!("echo '{}'", "a".repeat(len));
        let (path, line) = write_command_file(&command).expect("temp file");
        assert!(line.len() < 512, "launcher is {} bytes", line.len());
        assert!(!line.contains('\n') && !line.contains('\r'));
        let name = path.file_name().unwrap().to_str().unwrap().to_owned();
        assert!(name.starts_with(FILE_PREFIX) && name.ends_with(".ps1"));
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..3], &[0xEF, 0xBB, 0xBF]);
        assert_eq!(&bytes[3..], command.as_bytes());
        let kept = path.to_path_buf();
        drop(path);
        assert!(!kept.exists(), "the file is removed when dropped");
    }
}

#[test]
fn the_session_file_has_a_fixed_name_a_bom_and_is_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_session_command_file(dir.path(), 42, "Get-Date").expect("first write");
    assert_eq!(path, dir.path().join(".phosphor-cmd-42"));
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(&bytes[..3], &[0xEF, 0xBB, 0xBF]);
    assert_eq!(&bytes[3..], b"Get-Date");

    // A second command replaces the first completely (no tail of the longer one is left).
    let again = write_session_command_file(dir.path(), 42, "ls").expect("second write");
    assert_eq!(again, path);
    assert_eq!(&std::fs::read(&path).unwrap()[3..], b"ls");

    // Non-ASCII goes out as UTF-8.
    write_session_command_file(dir.path(), 42, "echo h\u{e9}llo \u{4e16}").unwrap();
    assert_eq!(
        std::str::from_utf8(&std::fs::read(&path).unwrap()[3..]).unwrap(),
        "echo h\u{e9}llo \u{4e16}"
    );

    // Another session has its own file.
    let other = write_session_command_file(dir.path(), 43, "pwd").unwrap();
    assert_ne!(other, path);
    assert_eq!(
        &std::fs::read(&path).unwrap()[3..],
        "echo h\u{e9}llo \u{4e16}".as_bytes()
    );

    // The handler deletes it; deleting is how this module's owner cleans up too.
    std::fs::remove_file(&path).unwrap();
    assert!(!path.exists());
}

#[test]
fn a_missing_directory_fails_instead_of_panicking() {
    let dir = tempfile::tempdir().unwrap();
    let gone = dir.path().join("gone");
    assert!(write_session_command_file(&gone, 1, "ls").is_none());
}

#[test]
fn only_plain_single_line_commands_are_inserted_from_the_file() {
    assert!(can_insert_from_file("Get-ChildItem -Recurse"));
    assert!(can_insert_from_file("echo \u{e9}"));
    assert!(!can_insert_from_file(""));
    assert!(!can_insert_from_file("a\nb"));
    assert!(!can_insert_from_file("a\r\nb"));
    assert!(!can_insert_from_file("a\tb"));
    assert!(!can_insert_from_file("a\x1bb"));
    assert!(can_insert_from_file(&"a".repeat(MAX_TYPED_BYTES)));
    assert!(!can_insert_from_file(&"a".repeat(MAX_TYPED_BYTES + 1)));
}

#[test]
fn the_kill_switch_is_off_only_for_zero() {
    assert!(file_insert_enabled_for(None));
    assert!(file_insert_enabled_for(Some("1")));
    assert!(file_insert_enabled_for(Some("")));
    assert!(!file_insert_enabled_for(Some("0")));
    assert!(!file_insert_enabled_for(Some(" 0 ")));
}
