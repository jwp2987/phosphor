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
    assert_eq!(
        decoded_bytes.len() % 2,
        0,
        "UTF-16LE byte count must be even"
    );

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

fn fallback_source(unsupported_shell: Option<&str>) -> ShellStarterSource {
    ShellStarterSource::Fallback {
        unsupported_shell: unsupported_shell.map(str::to_owned),
        starter: DirectShellStarter::new_for_test(
            ShellType::Zsh,
            PathBuf::from("/bin/zsh"),
            Vec::new(),
        ),
    }
}

/// #882: the "doesn't currently support your default shell" banner is only
/// true when the default shell was resolved and is unsupported.
#[test]
fn unsupported_shell_banner_requires_a_resolved_unsupported_shell() {
    assert!(fallback_source(Some("/usr/bin/nu")).is_known_unsupported_default_shell());
}

/// #882: a fallback taken because the passwd lookup failed (under fd
/// exhaustion every lookup path fails with EMFILE) says nothing about the
/// user's shell, so it must not raise the banner.
#[test]
fn unresolved_default_shell_does_not_claim_it_is_unsupported() {
    assert!(!fallback_source(None).is_known_unsupported_default_shell());
}

#[test]
fn non_fallback_sources_never_claim_unsupported_shell() {
    let starter = || {
        DirectShellStarter::new_for_test(ShellType::Bash, PathBuf::from("/bin/bash"), Vec::new())
    };
    assert!(!ShellStarterSource::UserDefault(starter()).is_known_unsupported_default_shell());
    assert!(!ShellStarterSource::Environment(starter()).is_known_unsupported_default_shell());
}

/// Runs the exact argv `arguments_for_session_spawning_command` produces for fish through a
/// real fish, with a hostile `whoami`/`hostname`, and checks that InitShell is emitted and the
/// payload parses and round-trips (#898: one backslash level was lost between the asset and
/// fish's argv, `'\'` was an unterminated string, and no fish pane ever started; the older
/// `script/test_fish_bootstrap` sources the asset directly so it could not see this).
#[cfg(unix)]
fn assert_fish_init_shell_from_args(
    fish: &std::path::Path,
    args_for: impl FnOnce(&std::path::Path) -> Vec<OsString>,
) {
    use std::os::unix::fs::PermissionsExt;
    use std::process::{Command, Stdio};

    let dir = tempfile::tempdir().unwrap();
    let hostile = "a\\b\"c\td\u{1}e\\\\";
    for (name, value) in [
        ("whoami", format!("u{hostile}")),
        ("hostname", format!("h{hostile}")),
    ] {
        let path = dir.path().join(name);
        let body = format!(
            "#!/bin/sh\nprintf '%s\\n' '{}'\n",
            value.replace('\'', "'\\''")
        );
        std::fs::write(&path, body).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let args = args_for(fish);
    let path = format!(
        "{}:{}",
        dir.path().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let output = Command::new(fish)
        .args(&args)
        .env("PATH", path)
        .env("HOME", dir.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("Unexpected end of string") && !stderr.contains("Unsupported use"),
        "fish rejected the init command: {stderr}"
    );

    let stdout = output.stdout;
    let start = stdout
        .windows(3)
        .position(|w| w == b"\x1bP$")
        .unwrap_or_else(|| panic!("no DCS emitted; stderr: {stderr}"));
    let rest = &stdout[start + 4..];
    let end = rest
        .iter()
        .position(|b| *b == 0x9c)
        .expect("DCS terminator");
    let hex = std::str::from_utf8(&rest[..end]).unwrap();
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    let json: serde_json::Value =
        serde_json::from_slice(&bytes).expect("InitShell payload must be valid JSON");
    assert_eq!(json["hook"], "InitShell");
    assert_eq!(json["value"]["shell"], "fish");
    assert_eq!(json["value"]["user"], format!("u{hostile}"));
    assert_eq!(json["value"]["hostname"], format!("h{hostile}"));
}

#[cfg(unix)]
fn find_fish() -> Option<std::path::PathBuf> {
    let fish = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|d| d.join("fish"))
        .find(|p| p.is_file());
    if fish.is_none() {
        if std::env::var_os("CI").is_some() {
            panic!("fish must be installed in CI");
        }
        eprintln!("fish not installed; skipping");
    }
    fish
}

#[cfg(unix)]
#[test]
fn fish_init_command_from_real_launch_args_emits_init_shell() {
    let Some(fish) = find_fish() else { return };
    assert_fish_init_shell_from_args(&fish, |fish| {
        arguments_for_session_spawning_command(
            fish.to_str().unwrap(),
            ShellType::Fish,
            SessionId::from(4242u64),
        )
    });
}

/// #909: a tab-config pane with `shell = "fish"` resolves to an `AvailableShell` and goes
/// through `ShellStarter::init` -> `to_shell_starter_source` -> `ShellStarter::Direct`, not
/// straight through `arguments_for_session_spawning_command`. Run the args that path really
/// produces.
#[cfg(unix)]
#[test]
fn fish_tab_config_pane_shell_starter_args_emit_init_shell() {
    let Some(fish) = find_fish() else { return };
    assert_fish_init_shell_from_args(&fish, |fish| {
        let available =
            crate::terminal::available_shells::AvailableShell::try_from(fish.to_str().unwrap())
                .expect("fish resolves as a tab-config shell");
        let source = ShellStarter::init(available).expect("fish has a starter");
        let source = warpui::r#async::block_on(source.to_shell_starter_source())
            .expect("fish starter source");
        match ShellStarter::from(source) {
            ShellStarter::Direct(starter) => starter.args().clone(),
            _ => panic!("fish should start directly"),
        }
    });
}
