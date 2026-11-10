use super::buffer_event_needs_render;

#[test]
fn a_shell_mode_keystroke_that_keeps_the_buffer_non_empty_does_not_rerender() {
    // Emptiness changing always renders, in either mode.
    assert!(buffer_event_needs_render(true, true, true));
    assert!(buffer_event_needs_render(true, false, true));
    // More text in a non-empty buffer: nothing the bar reads changed.
    assert!(!buffer_event_needs_render(false, true, true));
    assert!(!buffer_event_needs_render(false, false, true));
}

#[test]
fn the_kill_switch_restores_the_per_key_render_in_shell_mode() {
    assert!(buffer_event_needs_render(false, true, false));
    // Agent (AI) mode never re-rendered on a plain edit.
    assert!(!buffer_event_needs_render(false, false, false));
}

/// #1962: the slash-command model emits on every buffer change. For plain text its state is
/// `None` before and after, and the bar used to render again for each of those events.
#[test]
fn a_slash_event_that_changes_neither_the_state_nor_the_row_does_not_rerender() {
    use super::slash_event_needs_render;
    use crate::terminal::input::message_bar::row_event_needs_render;

    let row = Some("inline=false:hints");
    assert!(!slash_event_needs_render(false, || {
        row_event_needs_render(row, row, true)
    }));
}

#[test]
fn a_slash_event_rerenders_when_the_state_or_the_row_changed() {
    use super::slash_event_needs_render;
    use crate::terminal::input::message_bar::row_event_needs_render;
    use crate::terminal::input::slash_command_model::SlashCommandEntryState;

    // The state changed (`/` typed: nothing -> composing; one more character of the filter).
    let composing = |filter: &str| SlashCommandEntryState::Composing {
        filter: filter.to_owned(),
    };
    assert!(SlashCommandEntryState::None == SlashCommandEntryState::None);
    assert!(SlashCommandEntryState::None != composing(""));
    assert!(composing("pl") != composing("pla"));
    assert!(slash_event_needs_render(true, || {
        panic!("the row is not asked for when the state changed")
    }));
    // The state did not, but the row is no longer the one drawn (state the bar has no
    // notification for moved it: the shell finished bootstrapping, a plan appeared).
    assert!(slash_event_needs_render(false, || {
        row_event_needs_render(
            Some("inline=false:starting shell"),
            Some("inline=false:hints"),
            true,
        )
    }));
    // A row that cannot be compared (the misroute offer, a Figma chip) renders.
    assert!(slash_event_needs_render(false, || {
        row_event_needs_render(Some("inline=false:hints"), None, true)
    }));
    // Kill switch: every event renders.
    assert!(slash_event_needs_render(false, || {
        row_event_needs_render(Some("row"), Some("row"), false)
    }));
}
