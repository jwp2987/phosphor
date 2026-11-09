use std::time::Duration;

use super::*;
use crate::terminal::model::block::{INTERACTIVE_PROMPT_NOTICE_EXPLICIT, interactive_prompt_shape};
use warp_terminal::model::Point;

fn candidate(text: &str) -> InteractivePromptCandidate {
    InteractivePromptCandidate {
        text: text.to_owned(),
        cursor: Point {
            row: 2,
            col: text.len(),
        },
        shape: interactive_prompt_shape(text).expect("prompt-shaped"),
    }
}

/// #1882: with no termios the watch only ever produces a notice, once, after the shape's quiet
/// window, driven by an observation (output or the timer it armed) -- never a hand-over.
#[test]
fn an_explicit_prompt_gets_a_notice_after_its_quiet_window_and_once() {
    let t0 = instant::Instant::now();
    let mut watch = PromptNoticeWatch::default();
    let block = BlockIndex::from(3);
    let first = watch.observe(block, t0, Some(candidate("Continue? [y/N]")));
    assert!(!first.show_notice);
    let (at, epoch) = first.arm.expect("the quiet timer is armed");
    assert_eq!(at, t0 + INTERACTIVE_PROMPT_NOTICE_EXPLICIT);

    // Output that leaves the same prompt moves nothing: the same timer stays armed.
    let again = watch.observe(
        block,
        t0 + Duration::from_secs(1),
        Some(candidate("Continue? [y/N]")),
    );
    assert_eq!(again, NoticeStep::default());

    assert!(watch.timer_fired(epoch));
    let fired = watch.observe(block, at, Some(candidate("Continue? [y/N]")));
    assert!(fired.show_notice);
    assert_eq!(fired.arm, None, "the notice is once per prompt");
    let later = watch.observe(
        block,
        at + Duration::from_secs(60),
        Some(candidate("Continue? [y/N]")),
    );
    assert_eq!(later, NoticeStep::default());
}

/// A prompt that changes restarts the clock; no candidate (the user holds the command, or the
/// line is output) forgets it; a stale timer is ignored.
#[test]
fn a_changed_prompt_restarts_the_clock_and_a_stale_timer_is_ignored() {
    let t0 = instant::Instant::now();
    let mut watch = PromptNoticeWatch::default();
    let block = BlockIndex::from(1);
    let (_, old_epoch) = watch
        .observe(block, t0, Some(candidate("Proceed? [y/N]")))
        .arm
        .unwrap();
    let moved = watch.observe(
        block,
        t0 + Duration::from_secs(2),
        Some(candidate("Really proceed? [y/N]")),
    );
    let (at, new_epoch) = moved.arm.expect("the deadline moved");
    assert_eq!(
        at,
        t0 + Duration::from_secs(2) + INTERACTIVE_PROMPT_NOTICE_EXPLICIT
    );
    assert!(!watch.timer_fired(old_epoch), "the old timer is void");
    assert!(watch.timer_fired(new_epoch));

    let none = watch.observe(block, at, None);
    assert!(!none.show_notice);
    assert_eq!(none.arm, None);
}

/// A new block starts over, so the same prompt text in the next command is noticed again.
#[test]
fn a_new_block_starts_over() {
    let t0 = instant::Instant::now();
    let mut watch = PromptNoticeWatch::default();
    let first = BlockIndex::from(1);
    let (at, _) = watch
        .observe(first, t0, Some(candidate("Press any key")))
        .arm
        .unwrap();
    assert!(
        watch
            .observe(first, at, Some(candidate("Press any key")))
            .show_notice
    );
    let second = BlockIndex::from(2);
    let step = watch.observe(second, at, Some(candidate("Press any key")));
    assert!(!step.show_notice);
    assert!(step.arm.is_some());
    assert_eq!(watch.block(), Some(second));
}

/// A bare trailing `:` is weaker evidence than a marker: a longer window, still only a notice.
#[test]
fn a_bare_trailing_colon_waits_longer_than_a_marker() {
    let t0 = instant::Instant::now();
    let mut watch = PromptNoticeWatch::default();
    let (at, _) = watch
        .observe(BlockIndex::from(1), t0, Some(candidate("Password:")))
        .arm
        .unwrap();
    assert!(at > t0 + INTERACTIVE_PROMPT_NOTICE_EXPLICIT);
}
