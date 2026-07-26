use super::{
    is_su_to_root, should_spawn_su_password_injector, PASSWORD_PROMPT_REGEX, SU_ROOT_CMD_REGEX,
};
use zeroize::Zeroizing;

fn pw_matches(input: &str) -> bool {
    PASSWORD_PROMPT_REGEX.is_match(input.as_bytes())
}

fn su_matches(input: &str) -> bool {
    SU_ROOT_CMD_REGEX.is_match(input.as_bytes())
}

#[test]
fn password_prompt_matches_typical_forms() {
    // Half-width colon
    assert!(pw_matches("Password:"));
    assert!(pw_matches("Password: "));
    assert!(pw_matches("[sudo] password for alice: "));
    assert!(pw_matches("user@host's password: "));
    // Full-width colon (Chinese input method) — fixture strings are
    // intentional CJK, kept verbatim: they exercise the real-world Chinese
    // `su` password prompt (see PASSWORD_PROMPT_REGEX).
    assert!(pw_matches("密码:"));
    assert!(pw_matches("密码："));
    // Kylin V10's colon-less special case
    assert!(pw_matches("输入密码"));
    assert!(pw_matches("输入密码 "));
    // passphrase
    assert!(pw_matches("Enter passphrase for key '/home/u/.ssh/id_rsa': "));
}

#[test]
fn password_prompt_rejects_false_positives() {
    // These all contain 'password' / '密码' but are not real prompts, so they must not false-positive
    assert!(!pw_matches("Your password has expired"));
    assert!(!pw_matches("Bad password, try again"));
    assert!(!pw_matches("password changed successfully"));
    assert!(!pw_matches("New password for root"));
    assert!(!pw_matches("Welcome! Please change your password soon.\n"));
    assert!(!pw_matches("Last login: Mon Jan 1 password rotated yesterday\n"));
    // Same for Chinese — fixture kept verbatim (real-world non-prompt Chinese text)
    assert!(!pw_matches("您的密码已过期"));
}

#[test]
fn su_root_matches_common_variants() {
    // Most basic
    assert!(su_matches("su"));
    assert!(su_matches("su\n"));
    // Shorthand form without a username (defaults to root)
    assert!(su_matches("su -"));
    assert!(su_matches("su -l"));
    assert!(su_matches("su --login"));
    // Explicit root
    assert!(su_matches("su root"));
    assert!(su_matches("su - root"));
    assert!(su_matches("su -l root"));
    assert!(su_matches("su --login root"));
    // sudo su (\bsu still matches)
    assert!(su_matches("sudo su"));
}

#[test]
fn su_to_other_user_does_not_match() {
    // Switching to a non-root user should not trigger
    assert!(!su_matches("su lg"));
    assert!(!su_matches("su - lg"));
    assert!(!su_matches("su -l lg"));
    assert!(!su_matches("su --login lg"));
    assert!(!su_matches("su admin"));
}

#[test]
fn su_in_middle_of_other_command_does_not_match() {
    // su not at the end of a line should not trigger
    assert!(!su_matches("susan"));
    assert!(!su_matches("issue"));
    // For a command like "grep su file", the line end is neither su nor the su root pattern
    assert!(!su_matches("grep su /etc/passwd"));
}

#[test]
fn is_su_to_root_detects_in_buffer() {
    let buf = b"user@host:~$ su root\r\nPassword: ";
    assert!(is_su_to_root(buf));

    let buf = b"user@host:~$ su lg\r\nPassword: ";
    assert!(!is_su_to_root(buf));
}

#[test]
fn full_pipeline_su_root_with_password_prompt() {
    // Simulates a full PTY sequence: user types `su -`, and a password prompt appears after the echo
    let buf = b"alice@kylin:~$ su -\r\n\xe5\xaf\x86\xe7\xa0\x81\xef\xbc\x9a";
    assert!(PASSWORD_PROMPT_REGEX.is_match(buf));
    assert!(is_su_to_root(buf));
}

#[test]
fn should_spawn_su_password_injector_requires_non_empty_root_password() {
    assert!(!should_spawn_su_password_injector(None));

    let empty_password = Zeroizing::new(String::new());
    assert!(!should_spawn_su_password_injector(Some(&empty_password)));

    let password = Zeroizing::new("root-password".to_string());
    assert!(should_spawn_su_password_injector(Some(&password)));
}
