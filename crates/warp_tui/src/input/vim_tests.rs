//! Regression tests for the free functions in [`super`] (the TUI prompt's
//! [`VimHandler`](vim::vim::VimHandler) implementation).
//!
//! These target the pure helpers directly rather than driving a full
//! [`TuiInputView`](super::TuiInputView) through the app harness (see
//! `view_tests.rs` for that heavier style): `bounded_repeated_text` and
//! `vim_operand_motion_type` have no dependency on `ViewContext`/`App`, so a
//! plain `#[test]` exercises the exact logic `replace_text` and `paste` call
//! without needing to spin up a window, model singletons, etc.
use vim::vim::{CharacterMotion, Direction, MotionType, VimMotion, VimOperand};

use super::{MAX_VIM_PASTE_BYTES, bounded_repeated_text, vim_operand_motion_type};

/// Regression test for the vim replace-mode DoS: a count-prefixed replace
/// (`999999999r`) or continuous `R`-mode used to build
/// `text.repeat(repeat_count as usize)` directly, with no cap on
/// `repeat_count: u32`. That could attempt a multi-gigabyte allocation and
/// crash or hang the process. `bounded_repeated_text` — reused from the vim
/// paste path — must clamp the result to `MAX_VIM_PASTE_BYTES` regardless of
/// how large the requested count is.
#[test]
fn bounded_repeated_text_caps_huge_counts_without_allocating_unbounded_memory() {
    let result = bounded_repeated_text("x", u32::MAX);
    assert!(
        result.len() <= MAX_VIM_PASTE_BYTES,
        "expected result capped at {MAX_VIM_PASTE_BYTES} bytes, got {}",
        result.len()
    );
    assert_eq!(result.len(), MAX_VIM_PASTE_BYTES);
    assert!(result.bytes().all(|b| b == b'x'));
}

/// A large count paired with a multi-byte replacement string must also stay
/// within the byte cap (the cap is on total output bytes, not repeat count).
#[test]
fn bounded_repeated_text_caps_by_total_bytes_not_repeat_count() {
    let text = "abcd";
    let result = bounded_repeated_text(text, u32::MAX);
    assert!(
        result.len() <= MAX_VIM_PASTE_BYTES,
        "expected result capped at {MAX_VIM_PASTE_BYTES} bytes, got {}",
        result.len()
    );
    // The cap rounds down to a whole number of repeats of `text`.
    assert_eq!(result.len() % text.len(), 0);
}

/// Counts small enough to stay under the cap are unaffected (no false
/// truncation of legitimate, small `r`/`R` replacements).
#[test]
fn bounded_repeated_text_leaves_small_counts_unbounded() {
    assert_eq!(bounded_repeated_text("ab", 3), "ababab");
    assert_eq!(bounded_repeated_text("x", 0), "");
}

/// An empty replacement text must not panic (guards the `text.len().max(1)`
/// divide-by-zero avoidance in the bound computation).
#[test]
fn bounded_repeated_text_handles_empty_text() {
    assert_eq!(bounded_repeated_text("", u32::MAX), "");
    assert_eq!(bounded_repeated_text("", 0), "");
}

#[test]
fn vim_operand_motion_type_matches_operand_shape() {
    assert_eq!(
        vim_operand_motion_type(&VimOperand::Line),
        MotionType::Linewise
    );

    let charwise_motion = VimOperand::Motion {
        motion_type: MotionType::Charwise,
        motion: VimMotion::Character(CharacterMotion::Right),
    };
    assert_eq!(
        vim_operand_motion_type(&charwise_motion),
        MotionType::Charwise
    );

    let linewise_motion = VimOperand::Motion {
        motion_type: MotionType::Linewise,
        motion: VimMotion::Paragraph(Direction::Forward),
    };
    assert_eq!(
        vim_operand_motion_type(&linewise_motion),
        MotionType::Linewise
    );
}
