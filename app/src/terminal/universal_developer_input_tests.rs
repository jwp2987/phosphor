use super::{ButtonBarDisplay, empty_state_change_needs_bar_render};

fn display() -> ButtonBarDisplay {
    ButtonBarDisplay {
        is_viewer: false,
        shows_model_selector: true,
        shows_prompt_alert: false,
        compact_threshold_bits: 420.0_f32.to_bits(),
    }
}

#[test]
fn the_first_typed_character_does_not_rerender_the_bar_itself() {
    // Empty -> non-empty changes only the segmented control's label and colors (its own
    // notification); the bar would be built from exactly what it was built from last.
    assert!(!empty_state_change_needs_bar_render(
        Some(display()),
        Some(display()),
        true
    ));
}

#[test]
fn a_bar_that_would_be_built_differently_is_rerendered() {
    for changed in [
        ButtonBarDisplay {
            is_viewer: true,
            ..display()
        },
        ButtonBarDisplay {
            shows_model_selector: false,
            ..display()
        },
        ButtonBarDisplay {
            shows_prompt_alert: true,
            ..display()
        },
        ButtonBarDisplay {
            compact_threshold_bits: 500.0_f32.to_bits(),
            ..display()
        },
    ] {
        assert!(
            empty_state_change_needs_bar_render(Some(display()), Some(changed), true),
            "{changed:?}"
        );
    }
}

#[test]
fn an_unknown_side_is_rerendered() {
    // Never rendered yet.
    assert!(empty_state_change_needs_bar_render(
        None,
        Some(display()),
        true
    ));
    // The terminal model was busy, so what the bar would be built from now is not known.
    assert!(empty_state_change_needs_bar_render(
        Some(display()),
        None,
        true
    ));
}

#[test]
fn with_the_kill_switch_every_empty_state_change_rerenders_the_bar() {
    assert!(empty_state_change_needs_bar_render(
        Some(display()),
        Some(display()),
        false
    ));
}
