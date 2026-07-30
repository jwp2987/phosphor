use super::{
    is_su_to_root, next_su_password_event, should_spawn_su_password_injector, SuPasswordEvent,
    PASSWORD_PROMPT_REGEX, SU_ROOT_CMD_REGEX,
};
use std::sync::Arc;
use zeroize::Zeroizing;

fn make_channel() -> (
    async_broadcast::Sender<Arc<Vec<u8>>>,
    async_broadcast::Receiver<Arc<Vec<u8>>>,
) {
    async_broadcast::broadcast(16)
}

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

/// Real flow: `su -` gets typed/echoed, then the genuine password prompt
/// follows right after — should fire.
#[test]
fn next_su_password_event_fires_on_genuine_prompt_after_su() {
    let (tx, mut rx) = make_channel();
    warpui::r#async::block_on(async {
        tx.broadcast(Arc::new(b"alice@host:~$ su -\r\n".to_vec()))
            .await
            .unwrap();
        tx.broadcast(Arc::new(b"Password: ".to_vec())).await.unwrap();
        drop(tx);

        let mut buf = Vec::new();
        let event = next_su_password_event(&mut rx, &mut buf).await;
        assert_eq!(event, SuPasswordEvent::PromptFired);
    });
}

/// Core regression test for finding #9: the old implementation matched the
/// su-command regex and the password-prompt regex independently against
/// the whole sliding-window buffer, so an `su root` line typed long before,
/// plus *unrelated* "Password:"-looking output later (e.g. `cat`ing a file
/// with a literal `Password:` line) — with nothing to do with an actual su
/// prompt — could co-occur in the window and pop the confirmation menu over
/// content that was never a real prompt. That must no longer happen once a
/// shell prompt has appeared between the two (i.e. the su invocation
/// already resolved without asking for a password).
#[test]
fn fake_password_prompt_unrelated_to_earlier_su_does_not_fire() {
    let bait: &[u8] = b"Password: \r\n";
    // Sanity: this bait text does match the raw password-prompt regex in
    // isolation — proving the state machine, not a non-matching pattern, is
    // what prevents the false trigger below.
    assert!(PASSWORD_PROMPT_REGEX.is_match(bait));

    let (tx, mut rx) = make_channel();
    warpui::r#async::block_on(async {
        // User runs `su -`, but it resolves immediately without asking for
        // a password (e.g. NOPASSWD-equivalent) — a shell prompt reappears.
        tx.broadcast(Arc::new(b"root@host:~$ su -\r\n".to_vec()))
            .await
            .unwrap();
        tx.broadcast(Arc::new(b"# ".to_vec())).await.unwrap();
        // Later, completely unrelated to that su invocation, the user cats
        // a file containing a line that happens to look like a password
        // prompt.
        tx.broadcast(Arc::new(b"cat leaked-notes.txt\r\n".to_vec()))
            .await
            .unwrap();
        tx.broadcast(Arc::new(bait.to_vec())).await.unwrap();
        drop(tx);

        let mut buf = Vec::new();
        // First cycle: su was seen, but it stood down (shell prompt showed
        // up before any password prompt) rather than firing on the later,
        // unrelated bait text.
        let first = next_su_password_event(&mut rx, &mut buf).await;
        assert_eq!(first, SuPasswordEvent::StoodDown);

        // Second cycle: nothing left to see but EOF (no *new* su
        // invocation preceded the bait text, so it's never considered).
        let second = next_su_password_event(&mut rx, &mut buf).await;
        assert_eq!(second, SuPasswordEvent::Eof);
    });
}

/// The password prompt may land in the same PTY chunk as the su/sudo echo
/// (a single terminal flush covering both) — must still fire.
#[test]
fn next_su_password_event_fires_when_prompt_in_same_chunk_as_su() {
    let (tx, mut rx) = make_channel();
    warpui::r#async::block_on(async {
        tx.broadcast(Arc::new(b"$ su -\r\nPassword: ".to_vec()))
            .await
            .unwrap();
        drop(tx);

        let mut buf = Vec::new();
        let event = next_su_password_event(&mut rx, &mut buf).await;
        assert_eq!(event, SuPasswordEvent::PromptFired);
    });
}

/// After standing down (or firing), the buffer must be cleared so a stale
/// su-root match can't silently combine with the next cycle's output.
#[test]
fn next_su_password_event_resets_buffer_between_cycles() {
    let (tx, mut rx) = make_channel();
    warpui::r#async::block_on(async {
        // Cycle 1: su root resolves instantly (stand down).
        tx.broadcast(Arc::new(b"$ su root\r\n# ".to_vec())).await.unwrap();
        drop(tx);

        let mut buf = Vec::new();
        assert_eq!(
            next_su_password_event(&mut rx, &mut buf).await,
            SuPasswordEvent::StoodDown
        );
        assert!(
            buf.is_empty(),
            "buffer must be cleared after standing down, got: {:?}",
            String::from_utf8_lossy(&buf)
        );
    });
}
