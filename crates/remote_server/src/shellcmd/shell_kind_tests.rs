use super::*;

#[test]
fn shells_are_classified_by_base_name() {
    for (name, kind) in [
        ("/bin/bash", ShellKind::Posix),
        ("zsh", ShellKind::Posix),
        ("-bash", ShellKind::Posix),
        ("/usr/bin/dash", ShellKind::Posix),
        ("/usr/local/bin/fish", ShellKind::Fish),
        ("/home/u/.cargo/bin/nu", ShellKind::Nu),
        ("pwsh", ShellKind::Pwsh),
        (
            "C:\\Program Files\\PowerShell\\7\\pwsh.exe",
            ShellKind::Pwsh,
        ),
    ] {
        assert_eq!(ShellKind::from_shell_name(name), Some(kind), "{name}");
    }
    assert_eq!(ShellKind::from_shell_name("tcsh"), None);
    assert_eq!(ShellKind::from_shell_name(""), None);
}

#[test]
fn posix_scripts_are_not_wrapped() {
    assert_eq!(
        ShellKind::Posix.wrap("echo hi; echo 'x'"),
        "echo hi; echo 'x'"
    );
}

#[test]
fn fish_gets_sh_c_with_fish_escapes() {
    assert_eq!(ShellKind::Fish.wrap("echo hi"), "sh -c 'echo hi'");
    // A quote and a backslash are the only characters fish treats specially in '...'.
    assert_eq!(
        ShellKind::Fish.wrap(r"printf '%s\n' it's"),
        r"sh -c 'printf \'%s\\n\' it\'s'"
    );
}

#[test]
fn nushell_gets_a_raw_string_that_cannot_be_closed_early() {
    assert_eq!(ShellKind::Nu.wrap("echo hi"), "^sh -c r#'echo hi'#");
    // The script contains `'#`, so one hash is not enough.
    assert_eq!(ShellKind::Nu.wrap("echo '#x'"), "^sh -c r##'echo '#x''##");
    assert_eq!(raw_string_hashes("a'##b"), 3);
    assert_eq!(raw_string_hashes("plain"), 1);
}

#[test]
fn powershell_gets_an_encoded_command() {
    // "a" in UTF-16LE is 61 00 -> base64 "YQA=".
    assert_eq!(
        ShellKind::Pwsh.wrap("a"),
        "pwsh -NoProfile -NonInteractive -EncodedCommand YQA="
    );
    assert!(ShellKind::Pwsh.is_powershell());
    assert!(!ShellKind::Fish.is_powershell());
}
