//! Tests for the context-window usage circle icon mapping.
//!
//! Regression guard for the color semantics of the context-window circle:
//! the solid (white) marks represent the context *remaining*, not the amount
//! used. An empty conversation (0% used → 100% remaining) shows a full white
//! circle and it counts down to an all-grey circle as the window fills up
//! (100% used → 0% remaining).

use warp_core::ui::Icon;

use super::icon_for_context_window_usage;

#[test]
fn empty_conversation_shows_full_white_circle() {
    // 0% used == 100% remaining -> all-white circle.
    assert_eq!(
        icon_for_context_window_usage(0.0),
        Icon::ContextRemaining100
    );
}

#[test]
fn full_context_window_shows_all_grey_circle() {
    // 100% used == 0% remaining -> all-grey circle.
    assert_eq!(icon_for_context_window_usage(1.0), Icon::ContextRemaining0);
}

#[test]
fn icon_brightness_tracks_remaining_not_used() {
    // Lightly-used conversation: lots of context remaining -> mostly white.
    assert_eq!(icon_for_context_window_usage(0.1), Icon::ContextRemaining90);
    // Half used -> half white.
    assert_eq!(icon_for_context_window_usage(0.5), Icon::ContextRemaining50);
    // Heavily used (the original report's 88%): little remaining -> mostly grey.
    assert_eq!(
        icon_for_context_window_usage(0.88),
        Icon::ContextRemaining10
    );
}

#[test]
fn mapping_is_monotonic_more_usage_never_brightens_the_circle() {
    // As usage increases, the number of bright (remaining) marks must be
    // non-increasing — the circle only ever empties as context fills.
    let icon_rank = |usage: f32| match icon_for_context_window_usage(usage) {
        Icon::ContextRemaining0 => 0,
        Icon::ContextRemaining10 => 10,
        Icon::ContextRemaining20 => 20,
        Icon::ContextRemaining30 => 30,
        Icon::ContextRemaining40 => 40,
        Icon::ContextRemaining50 => 50,
        Icon::ContextRemaining60 => 60,
        Icon::ContextRemaining70 => 70,
        Icon::ContextRemaining80 => 80,
        Icon::ContextRemaining90 => 90,
        Icon::ContextRemaining100 => 100,
        other => panic!("unexpected icon: {other:?}"),
    };

    let mut usage = 0.0;
    let mut previous = icon_rank(usage);
    while usage <= 1.0 {
        let current = icon_rank(usage);
        assert!(
            current <= previous,
            "icon brightness increased as usage rose to {usage}: {previous} -> {current}"
        );
        previous = current;
        usage += 0.05;
    }
}

// ---------------------------------------------------------------------------
// Boundary pinning.
//
// The mapping is `remaining = 1.0 - usage` matched against ten inclusive
// `>=` thresholds, so an icon step changes at a usage value sitting 0.05 off
// a multiple of 0.1. The tests below pin every one of those ten steps, so an
// off-by-one edit to any threshold (or a `>=` silently becoming `>`) fails
// here rather than mislabelling how full a context window is.
// ---------------------------------------------------------------------------

/// One row per icon step: the usage value the step changes at, the icon just
/// below it, and the icon just above it.
const STEP_BOUNDARIES: &[(f32, Icon, Icon)] = &[
    (0.05, Icon::ContextRemaining100, Icon::ContextRemaining90),
    (0.15, Icon::ContextRemaining90, Icon::ContextRemaining80),
    (0.25, Icon::ContextRemaining80, Icon::ContextRemaining70),
    (0.35, Icon::ContextRemaining70, Icon::ContextRemaining60),
    (0.45, Icon::ContextRemaining60, Icon::ContextRemaining50),
    (0.55, Icon::ContextRemaining50, Icon::ContextRemaining40),
    (0.65, Icon::ContextRemaining40, Icon::ContextRemaining30),
    (0.75, Icon::ContextRemaining30, Icon::ContextRemaining20),
    (0.85, Icon::ContextRemaining20, Icon::ContextRemaining10),
    (0.95, Icon::ContextRemaining10, Icon::ContextRemaining0),
];

/// `0.001` is four orders of magnitude larger than the f32 rounding error at
/// these magnitudes, so each assertion is an exact statement about which side
/// of a threshold the value lands on — no epsilon fudge.
#[test]
fn every_icon_step_boundary_is_pinned_from_just_below_to_just_above() {
    for (boundary, below, above) in STEP_BOUNDARIES {
        assert_eq!(
            icon_for_context_window_usage(boundary - 0.001),
            *below,
            "usage just below the {boundary} boundary should keep {below:?}"
        );
        assert_eq!(
            icon_for_context_window_usage(boundary + 0.001),
            *above,
            "usage just above the {boundary} boundary should step down to {above:?}"
        );
    }
}

/// The thresholds are `>=` on the *remaining* fraction, so a usage value that
/// lands exactly on a boundary keeps the brighter (more-remaining) icon.
/// `0.85` is deliberately absent — see the test below it.
#[test]
fn usage_exactly_on_a_boundary_keeps_the_brighter_icon() {
    assert_eq!(
        icon_for_context_window_usage(0.05),
        Icon::ContextRemaining100
    );
    assert_eq!(icon_for_context_window_usage(0.15), Icon::ContextRemaining90);
    assert_eq!(icon_for_context_window_usage(0.25), Icon::ContextRemaining80);
    assert_eq!(icon_for_context_window_usage(0.35), Icon::ContextRemaining70);
    assert_eq!(icon_for_context_window_usage(0.45), Icon::ContextRemaining60);
    assert_eq!(icon_for_context_window_usage(0.55), Icon::ContextRemaining50);
    assert_eq!(icon_for_context_window_usage(0.65), Icon::ContextRemaining40);
    assert_eq!(icon_for_context_window_usage(0.75), Icon::ContextRemaining30);
    assert_eq!(icon_for_context_window_usage(0.95), Icon::ContextRemaining10);
}

/// The one boundary where f32 representation, not the threshold table,
/// decides the answer — recorded rather than hidden.
///
/// `0.85f32` is `0.85000002384…`, so `1.0 - 0.85f32` is `0.14999997615…`,
/// which is *below* the `>= 0.15` threshold; the circle shows 10% remaining
/// where exact arithmetic would show 20%. Every other exact boundary in the
/// table above rounds the other way and matches exact arithmetic. This is a
/// one-step cosmetic difference at a single tie point, it is deterministic
/// (IEEE-754 f32 is exactly reproducible), and it is identical in the pin
/// (`42effe840:app/src/ai/blocklist/usage/mod.rs` is byte-identical here),
/// so it is pinned as-is rather than "fixed" into a divergence.
#[test]
fn the_zero_point_eight_five_boundary_falls_to_the_lower_icon_in_f32() {
    assert_eq!(icon_for_context_window_usage(0.85), Icon::ContextRemaining10);
    // The neighbouring values behave as the table says, so this is the
    // representation of `0.85` itself and not a shifted threshold.
    assert_eq!(icon_for_context_window_usage(0.849), Icon::ContextRemaining20);
    assert_eq!(icon_for_context_window_usage(0.851), Icon::ContextRemaining10);
}

/// A context window can be reported as over-full (usage > 1.0) when the
/// transcript exceeds the model's window before it is trimmed. That must
/// clamp to "nothing remaining", never wrap around to a bright circle.
#[test]
fn over_full_context_window_clamps_to_zero_remaining() {
    assert_eq!(icon_for_context_window_usage(1.0), Icon::ContextRemaining0);
    assert_eq!(icon_for_context_window_usage(1.5), Icon::ContextRemaining0);
    assert_eq!(icon_for_context_window_usage(100.0), Icon::ContextRemaining0);
}

/// Negative usage is not a state the app should produce, but the function is
/// `pub` and takes a bare `f32`; it must clamp to the full circle rather than
/// fall through to the "context exhausted" icon.
#[test]
fn negative_usage_clamps_to_a_full_circle() {
    assert_eq!(
        icon_for_context_window_usage(-0.1),
        Icon::ContextRemaining100
    );
    assert_eq!(
        icon_for_context_window_usage(-100.0),
        Icon::ContextRemaining100
    );
}

/// Non-finite input reaches here if a caller ever divides by a zero-sized
/// context window (`0.0 / 0.0` is `NaN`, `x / 0.0` is `inf`). Nothing
/// upstream guarantees it cannot, so pin what happens: every `>=` comparison
/// against `NaN` is false, so `NaN` falls through the whole chain to the
/// final `else`, i.e. it renders as an exhausted context window.
///
/// Note for anyone changing this: `render_context_window_usage_icon` colours
/// the icon red on `usage >= 0.8`, and `NaN >= 0.8` is *false* — so a `NaN`
/// usage draws the "context full" glyph in the ordinary text colour, not the
/// red one. The two halves disagree about what `NaN` means. Neither is
/// reachable today; if a caller ever can produce `NaN`, fix it at the source
/// rather than making the two thresholds agree on a wrong answer.
#[test]
fn non_finite_usage_does_not_panic_and_reports_an_exhausted_window() {
    assert_eq!(
        icon_for_context_window_usage(f32::NAN),
        Icon::ContextRemaining0
    );
    assert_eq!(
        icon_for_context_window_usage(f32::INFINITY),
        Icon::ContextRemaining0
    );
    // `-inf` usage means "infinitely much remaining", which clamps full.
    assert_eq!(
        icon_for_context_window_usage(f32::NEG_INFINITY),
        Icon::ContextRemaining100
    );
}
