use super::*;

#[test]
#[cfg(windows)]
fn bracketed_paste_command_execution_normalizes_crlf_to_lf_for_posix_shells_on_windows() {
    let command = "curl 'https://google.com' \\\r\n  -H 'accept: application/json'";

    let bytes = bytes_to_execute_command(command, ShellType::Bash, true);

    let mut expected = ShellType::Bash.kill_buffer_bytes().to_vec();
    expected.extend_from_slice(escape_sequences::BRACKETED_PASTE_START);
    expected.extend_from_slice(b"curl 'https://google.com' \\\n  -H 'accept: application/json'");
    expected.extend_from_slice(escape_sequences::BRACKETED_PASTE_END);
    expected.extend_from_slice(ShellType::Bash.execute_command_bytes());

    assert_eq!(bytes, expected);
    assert!(!bytes.contains(&b'\r'));
}

#[test]
#[cfg(not(windows))]
fn bracketed_paste_command_execution_preserves_crlf_for_posix_shells_off_windows() {
    let command = "curl 'https://google.com' \\\r\n  -H 'accept: application/json'";

    let bytes = bytes_to_execute_command(command, ShellType::Bash, true);

    let mut expected = ShellType::Bash.kill_buffer_bytes().to_vec();
    expected.extend_from_slice(escape_sequences::BRACKETED_PASTE_START);
    expected.extend_from_slice(b"curl 'https://google.com' \\\r\n  -H 'accept: application/json'");
    expected.extend_from_slice(escape_sequences::BRACKETED_PASTE_END);
    expected.extend_from_slice(ShellType::Bash.execute_command_bytes());

    assert_eq!(bytes, expected);
    assert!(bytes.contains(&b'\r'));
}

#[test]
fn unbracketed_paste_command_execution_preserves_lf_for_posix_shells() {
    let command = "printf 'hello'\nprintf 'world'";

    let bytes = bytes_to_execute_command(command, ShellType::Bash, false);

    let mut expected = ShellType::Bash.kill_buffer_bytes().to_vec();
    expected.extend_from_slice(b"printf 'hello'\nprintf 'world'");
    expected.extend_from_slice(ShellType::Bash.execute_command_bytes());

    assert_eq!(bytes, expected);
    assert!(!bytes.contains(&b'\r'));
}

#[test]
fn powershell_command_execution_normalizes_linefeeds_to_carriage_returns() {
    let command = "Write-Output 'hello'\r\nWrite-Output 'world'\nWrite-Output 'again'";

    let bytes = bytes_to_execute_command(command, ShellType::PowerShell, false);

    let mut expected = ShellType::PowerShell.kill_buffer_bytes().to_vec();
    expected.extend_from_slice(b"Write-Output 'hello'\rWrite-Output 'world'\rWrite-Output 'again'");
    expected.extend_from_slice(ShellType::PowerShell.execute_command_bytes());

    assert_eq!(bytes, expected);
    assert!(!bytes.contains(&b'\n'));
}

#[test]
fn split_kill_buffer_write_splits_powershell_off_from_the_rest() {
    let bytes = bytes_to_execute_command("Get-ChildItem", ShellType::PowerShell, false);

    let (kill_buffer, rest) =
        split_kill_buffer_write(&bytes, ShellType::PowerShell).expect("PowerShell should split");

    assert_eq!(kill_buffer, ShellType::PowerShell.kill_buffer_bytes());
    let mut expected_rest = b"Get-ChildItem".to_vec();
    expected_rest.extend_from_slice(ShellType::PowerShell.execute_command_bytes());
    assert_eq!(rest, expected_rest.as_slice());
}

#[test]
fn split_kill_buffer_write_does_not_split_the_other_three_shells() {
    for shell_type in [ShellType::Zsh, ShellType::Bash, ShellType::Fish] {
        let bytes = bytes_to_execute_command("echo hi", shell_type, false);
        assert!(
            split_kill_buffer_write(&bytes, shell_type).is_none(),
            "expected no split for {shell_type:?}, which uses a single unambiguous control byte"
        );
    }
}

#[test]
fn split_kill_buffer_write_handles_a_command_with_no_content_gracefully() {
    let kill_buffer_only = ShellType::PowerShell.kill_buffer_bytes().to_vec();
    assert!(split_kill_buffer_write(&kill_buffer_only, ShellType::PowerShell).is_none());
}

#[test]
fn split_kill_buffer_write_returns_none_when_the_kill_buffer_is_not_the_prefix() {
    let kill_buffer = ShellType::PowerShell.kill_buffer_bytes();
    let not_prefixed = b"Get-ChildItem (no leading chord)".to_vec();
    assert!(
        not_prefixed.len() > kill_buffer.len(),
        "fixture must be longer than the chord so the length guard alone wouldn't catch it"
    );
    assert!(!not_prefixed.starts_with(kill_buffer));
    assert!(split_kill_buffer_write(&not_prefixed, ShellType::PowerShell).is_none());
}

#[test]
fn split_kill_buffer_write_does_not_split_tmux_wrapped_bytes() {
    // Under tmux control mode `send_write_to_event_loop` rewraps the bytes with
    // `tmux::format_input` before the split runs, so the chord is no longer the prefix and the
    // write must go out whole -- splitting a tmux frame in two would corrupt it.
    let bytes = bytes_to_execute_command("Get-ChildItem", ShellType::PowerShell, false);
    let tmux_wrapped = crate::terminal::model::tmux::format_input(0, &bytes)
        .as_bytes()
        .to_owned();

    assert!(!tmux_wrapped.starts_with(ShellType::PowerShell.kill_buffer_bytes()));
    assert!(split_kill_buffer_write(&tmux_wrapped, ShellType::PowerShell).is_none());
}

/// nushell's line editor (reedline) takes a bracketed paste as one insertion, keeps its newlines
/// and leading whitespace, and submits on CR; Ctrl-P (bound to `edit: clear` by `nu.nu`) empties
/// the buffer first.
#[test]
fn nu_command_execution_clears_the_buffer_pastes_and_submits_with_cr() {
    let command = " if true {\n    echo multi\n}";

    let bytes = bytes_to_execute_command(command, ShellType::Nu, true);

    let mut expected = vec![escape_sequences::C0::DLE];
    expected.extend_from_slice(escape_sequences::BRACKETED_PASTE_START);
    expected.extend_from_slice(command.as_bytes());
    expected.extend_from_slice(escape_sequences::BRACKETED_PASTE_END);
    expected.push(b'\r');
    assert_eq!(bytes, expected);

    // Without bracketed paste the text is typed as it is, then submitted.
    let bytes = bytes_to_execute_command("ls | where size > 1mb", ShellType::Nu, false);
    let mut expected = vec![escape_sequences::C0::DLE];
    expected.extend_from_slice(b"ls | where size > 1mb");
    expected.push(b'\r');
    assert_eq!(bytes, expected);
}

#[test]
fn nu_command_execution_never_pastes_an_escape_byte() {
    // An ESC inside a paste could end the bracketed paste early.
    let bytes = bytes_to_execute_command("echo \u{1b}[201~ rm", ShellType::Nu, true);
    let start = escape_sequences::BRACKETED_PASTE_START.len() + 1;
    let end = bytes.len() - escape_sequences::BRACKETED_PASTE_END.len() - 1;
    assert!(!bytes[start..end].contains(&escape_sequences::C0::ESC));
}

#[test]
fn nu_kill_buffer_chord_is_a_single_byte_and_never_split_off() {
    let bytes = bytes_to_execute_command("ls", ShellType::Nu, false);
    assert!(split_kill_buffer_write(&bytes, ShellType::Nu).is_none());
    assert_eq!(
        ShellType::Nu.kill_buffer_bytes(),
        &[escape_sequences::C0::DLE]
    );
}
