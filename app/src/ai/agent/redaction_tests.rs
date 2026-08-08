//! Tests for `redact_secrets`, the function that strips secrets from agent input and action
//! results before they can leave the device (e.g. over BYOP). Not ported from the oracle pin
//! (`02b53fcd8`) — `app/src/ai/agent/redaction.rs` has no test file there either. This coverage
//! is new, written for issue #164.
//!
//! `redact_secrets` itself has no parameters beyond the string to redact; it always scans via
//! the process-global `SECRETS_REGEX` (populated by `set_user_and_enterprise_secret_regexes`).
//! That state is shared across tests, so every test here is `#[serial]` and sets the regexes it
//! needs at the top, following the pattern already used in
//! `app/src/ai/blocklist/block/secret_redaction_test.rs`.

use regex::Regex;
use serial_test::serial;

use crate::terminal::model::secrets;

use super::redact_secrets;

#[test]
#[serial]
fn redact_secrets_with_no_regexes_configured_leaves_input_untouched() {
    // No regexes configured at all: nothing should ever be redacted, no matter what the text
    // looks like.
    secrets::set_user_and_enterprise_secret_regexes(std::iter::empty(), std::iter::empty());

    let mut input = "please do not leak ghp_totallynotarealtoken or sk_live_fake".to_string();
    let original = input.clone();
    redact_secrets(&mut input);

    assert_eq!(input, original);
}

#[test]
#[serial]
fn redact_secrets_leaves_ordinary_text_untouched_when_nothing_matches() {
    // Regexes ARE configured, but the input contains nothing that matches them. This guards
    // against false-positive redaction of ordinary prose.
    secrets::set_user_and_enterprise_secret_regexes(
        [&Regex::new(r"SECRET-[0-9]{4}").expect("valid regex")],
        std::iter::empty(),
    );

    let mut input =
        "just a normal sentence about tokens and secrets, nothing here matches the pattern"
            .to_string();
    let original = input.clone();
    redact_secrets(&mut input);

    assert_eq!(input, original);
}

#[test]
#[serial]
fn redact_secrets_replaces_a_single_matched_secret_with_asterisks() {
    secrets::set_user_and_enterprise_secret_regexes(
        [&Regex::new(r"SECRET-[0-9]{4}").expect("valid regex")],
        std::iter::empty(),
    );

    let mut input = "token: SECRET-1234 please redact".to_string();
    redact_secrets(&mut input);

    // "SECRET-1234" is 11 bytes/chars; the replacement is the same length so the rest of the
    // string is untouched and unshifted.
    assert_eq!(input, format!("token: {} please redact", "*".repeat(11)));
}

#[test]
#[serial]
fn redact_secrets_redacts_every_occurrence_in_a_string_with_multiple_secrets() {
    secrets::set_user_and_enterprise_secret_regexes(
        [
            &Regex::new("SECRETA").expect("valid regex"),
            &Regex::new("SECRETB").expect("valid regex"),
        ],
        std::iter::empty(),
    );

    let mut input = "start SECRETA middle SECRETB end".to_string();
    redact_secrets(&mut input);

    assert_eq!(
        input,
        format!("start {} middle {} end", "*".repeat(7), "*".repeat(7))
    );
}

#[test]
#[serial]
fn redact_secrets_handles_a_secret_at_the_very_start_and_very_end_of_the_string() {
    secrets::set_user_and_enterprise_secret_regexes(
        [
            &Regex::new("HEAD").expect("valid regex"),
            &Regex::new("TAIL").expect("valid regex"),
        ],
        std::iter::empty(),
    );

    // No characters before the first secret or after the last one.
    let mut input = "HEAD middle TAIL".to_string();
    redact_secrets(&mut input);

    assert_eq!(input, format!("{} middle {}", "*".repeat(4), "*".repeat(4)));
}

#[test]
#[serial]
fn redact_secrets_redacts_adjacent_matches_that_touch_with_no_gap() {
    // Two patterns whose matches are directly back-to-back (the second starts exactly where the
    // first ends, no gap). `find_secrets_in_text` merges touching ranges
    // (`merge_sorted_ranges_with_levels`) before `redact_secrets` rewrites them, so this must
    // come out as one continuous redacted span, not two separately-computed (and potentially
    // mis-indexed) replacements.
    secrets::set_user_and_enterprise_secret_regexes(
        [
            &Regex::new("ab").expect("valid regex"),
            &Regex::new("cd").expect("valid regex"),
        ],
        std::iter::empty(),
    );

    let mut input = "xxabcdxx".to_string();
    redact_secrets(&mut input);

    assert_eq!(input, format!("xx{}xx", "*".repeat(4)));
}

#[test]
#[serial]
fn redact_secrets_redacts_multibyte_utf8_secret_without_panicking_on_char_boundaries() {
    // "秘密" (Chinese for "secret") is 2 chars / 6 UTF-8 bytes. Byte-range rewriting that isn't
    // char-boundary safe would panic on `replace_range`; `find_secrets_in_text`'s byte ranges
    // come from `char_indices`, so this must succeed and must not corrupt the surrounding ASCII
    // text.
    secrets::set_user_and_enterprise_secret_regexes(
        [&Regex::new("秘密").expect("valid regex")],
        std::iter::empty(),
    );

    let mut input = "foo 秘密 bar".to_string();
    redact_secrets(&mut input);

    // The replacement character is a single-byte '*', repeated once per byte of the match (6
    // bytes), so the redacted span is longer in character count than the original secret but the
    // ASCII text around it is untouched.
    assert_eq!(input, format!("foo {} bar", "*".repeat(6)));
}

#[test]
#[serial]
fn redact_secrets_redacts_a_multibyte_secret_at_the_very_start_of_the_string() {
    secrets::set_user_and_enterprise_secret_regexes(
        [&Regex::new("秘密").expect("valid regex")],
        std::iter::empty(),
    );

    let mut input = "秘密 bar".to_string();
    redact_secrets(&mut input);

    assert_eq!(input, format!("{} bar", "*".repeat(6)));
}
