use super::*;

#[test]
fn test_program_invalid_bash() {
    // This test assumes there is no bash binary at /some/weird/path/bash.
    let shell_path = "/some/weird/path/bash".to_owned();
    assert!(supported_shell_path_and_type(&shell_path).is_none());
}

#[test]
fn test_program_invalid_zsh() {
    // This test assumes there is no bash zsh at /some/weird/path/bash.
    let shell_path = "/some/weird/path/zsh".to_owned();
    assert!(supported_shell_path_and_type(&shell_path).is_none());
}

#[test]
fn test_program_unknown_shell() {
    let shell_path = "/some/weird/path/wtfsh".to_owned();
    assert!(supported_shell_path_and_type(&shell_path).is_none());
}

#[test]
fn test_powershell_encoded_command_has_no_trailing_nul() {
    let session_id = crate::terminal::bootstrap::generate_session_id();
    let args = arguments_for_session_spawning_command("pwsh", ShellType::PowerShell, session_id);
    let encoded_index = args
        .iter()
        .position(|a| a == "-EncodedCommand")
        .expect("PowerShell args should include -EncodedCommand")
        + 1;
    let encoded = args[encoded_index]
        .to_str()
        .expect("encoded blob should be valid UTF-8 (it's base64)");

    use base64::Engine as _;
    let decoded_bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .expect("should be valid base64");
    assert_eq!(decoded_bytes.len() % 2, 0, "UTF-16LE byte count must be even");

    let decoded_utf16: Vec<u16> = decoded_bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    // -EncodedCommand decodes into a length-prefixed managed string, not a C string --
    // a NUL terminator would become a literal trailing character in the parsed script,
    // not get stripped. Regression test for that bug.
    assert!(
        !decoded_utf16.ends_with(&[0]),
        "decoded script must not have a trailing NUL code unit"
    );

    let decoded_script = String::from_utf16(&decoded_utf16).expect("should be valid UTF-16");
    let expected_script =
        init_shell_script_for_shell(ShellType::PowerShell, &crate::ASSETS, session_id);
    assert_eq!(decoded_script, expected_script);
}

#[test]
fn test_trim_wsl_err_from_output() {
    assert_eq!(
        take_until_utf16_crlf(b"/bin/bash\n".to_vec()),
        b"/bin/bash\n".to_vec()
    );
    assert_eq!(
        take_until_utf16_crlf(b"/bin/bash\n\r\0\n\0W\0A\0R\0N\0I\0N\0G\0".to_vec()),
        b"/bin/bash\n".to_vec()
    );
}
