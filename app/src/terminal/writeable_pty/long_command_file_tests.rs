use super::*;

#[test]
fn short_commands_are_typed() {
    assert!(!needs_launcher(&"a".repeat(MAX_TYPED_BYTES)));
    assert!(needs_launcher(&"a".repeat(MAX_TYPED_BYTES + 1)));
}

#[test]
fn the_launcher_line_carries_no_command_text_and_no_path() {
    assert_eq!(LAUNCHER_LINE, ". (Warp-Run-LongCommand)");
    assert!(!LAUNCHER_LINE.contains('\n') && !LAUNCHER_LINE.contains('\r'));
    assert!(!LAUNCHER_LINE.contains('\'') && !LAUNCHER_LINE.contains('\\'));
    // The bootstrap's history filter and the function name must stay in step with it.
    let bootstrap = include_str!("../../../assets/bundled/bootstrap/pwsh.ps1");
    assert!(bootstrap.contains("function global:Warp-Run-LongCommand"));
    assert!(bootstrap.contains("$trimmed -eq '. (Warp-Run-LongCommand)'"));
}

#[test]
fn the_bootstrap_never_touches_the_disk_for_a_long_command() {
    let bootstrap = include_str!("../../../assets/bundled/bootstrap/pwsh.ps1");
    let start = bootstrap
        .find("function global:Warp-Run-LongCommand")
        .unwrap();
    let end = bootstrap[start..].find("New-Module").unwrap();
    let function = &bootstrap[start..start + end];
    assert!(!function.contains("System.IO.File"));
    assert!(!function.contains("StreamReader"));
    assert!(function.contains("NamedPipeClientStream"));
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
