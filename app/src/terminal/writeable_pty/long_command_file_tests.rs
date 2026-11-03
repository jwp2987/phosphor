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
fn only_plain_single_line_commands_are_inserted_via_the_pipe() {
    assert!(can_insert_via_pipe("Get-ChildItem -Recurse"));
    assert!(can_insert_via_pipe("echo \u{e9}"));
    assert!(!can_insert_via_pipe(""));
    assert!(!can_insert_via_pipe("a\nb"));
    assert!(!can_insert_via_pipe("a\r\nb"));
    assert!(!can_insert_via_pipe("a\tb"));
    assert!(!can_insert_via_pipe("a\x1bb"));
    assert!(can_insert_via_pipe(&"a".repeat(MAX_TYPED_BYTES)));
    assert!(!can_insert_via_pipe(&"a".repeat(MAX_TYPED_BYTES + 1)));
}

#[test]
fn the_kill_switch_is_off_only_for_zero_under_either_name() {
    assert!(fast_insert_enabled_for(None, None));
    assert!(fast_insert_enabled_for(Some("1"), None));
    assert!(fast_insert_enabled_for(Some(""), Some("")));
    assert!(!fast_insert_enabled_for(Some("0"), None));
    assert!(!fast_insert_enabled_for(Some(" 0 "), None));
    // The old name keeps working, and either one is enough to turn it off.
    assert!(!fast_insert_enabled_for(None, Some("0")));
    assert!(!fast_insert_enabled_for(Some("1"), Some("0")));
    assert!(!fast_insert_enabled_for(Some("0"), Some("1")));
}
