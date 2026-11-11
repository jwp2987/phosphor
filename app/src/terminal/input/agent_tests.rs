use super::{AgentInputDisplayFacts, agent_input_edit_needs_render};
use crate::terminal::input::InputSuggestionsMode;

/// A prompt being typed in an agent view: AI input, non-empty buffer, no menu, nothing else shown.
fn typing() -> AgentInputDisplayFacts {
    AgentInputDisplayFacts {
        editor_is_empty: false,
        slash_command_detected: false,
        ai_input_enabled: true,
        input_type_is_ai: true,
        input_type_locked: false,
        suggestions_mode: std::mem::discriminant(&InputSuggestionsMode::Closed),
        shows_prompt_banner: false,
        prompt_chip_width_bits: 0,
        attachment_chips: 0,
        shows_workflow_info: false,
        voltron_open: false,
        pane_focused: true,
        active_session: true,
        shows_command_x_ray: false,
        shortcut_view_open: false,
        inline_agent_view: false,
        shows_queued_prompts_panel: false,
        pane_height_bits: 600.0_f32.to_bits(),
    }
}

/// [`typing`] with a prompt suggestion drawn as a chip 304 px wide on the editor's first line.
fn with_chip() -> AgentInputDisplayFacts {
    AgentInputDisplayFacts {
        shows_prompt_banner: true,
        prompt_chip_width_bits: 304.0_f32.to_bits(),
        ..typing()
    }
}

/// #2138: the chip gets what the typed text leaves free, so a character that takes room from it
/// renders the input again (rendered at 304 px, now 296 px), and one that does not (the chip is
/// at its half-width cap, or there is no chip) does not.
#[test]
fn a_character_that_narrows_the_prompt_suggestion_chip_rerenders_the_agent_input() {
    assert!(agent_input_edit_needs_render(
        Some(with_chip()),
        AgentInputDisplayFacts {
            prompt_chip_width_bits: 296.0_f32.to_bits(),
            ..with_chip()
        },
        true
    ));
    assert!(!agent_input_edit_needs_render(
        Some(with_chip()),
        with_chip(),
        true
    ));
}

#[test]
fn one_more_character_in_a_non_empty_buffer_does_not_rerender_the_agent_input() {
    assert!(!agent_input_edit_needs_render(
        Some(typing()),
        typing(),
        true
    ));
}

#[test]
fn an_edit_that_changes_a_fact_the_agent_input_is_built_from_rerenders_it() {
    let changes: Vec<(&str, AgentInputDisplayFacts)> = vec![
        (
            "the buffer became empty (the border color reads it)",
            AgentInputDisplayFacts {
                editor_is_empty: true,
                ..typing()
            },
        ),
        (
            "a slash command was detected",
            AgentInputDisplayFacts {
                slash_command_detected: true,
                ..typing()
            },
        ),
        (
            "the input type changed",
            AgentInputDisplayFacts {
                ai_input_enabled: false,
                input_type_is_ai: false,
                ..typing()
            },
        ),
        (
            "the input type was locked",
            AgentInputDisplayFacts {
                input_type_locked: true,
                ..typing()
            },
        ),
        (
            "a menu opened",
            AgentInputDisplayFacts {
                suggestions_mode: std::mem::discriminant(&InputSuggestionsMode::SlashCommands),
                ..typing()
            },
        ),
        (
            "the prompt suggestion banner appeared",
            AgentInputDisplayFacts {
                shows_prompt_banner: true,
                ..typing()
            },
        ),
        ("the prompt suggestion chip appeared (#2138)", with_chip()),
        (
            "the prompt suggestion chip got narrower (#2138)",
            AgentInputDisplayFacts {
                prompt_chip_width_bits: 296.0_f32.to_bits(),
                ..with_chip()
            },
        ),
        (
            "an attachment was added",
            AgentInputDisplayFacts {
                attachment_chips: 1,
                ..typing()
            },
        ),
        (
            "the workflow info box appeared",
            AgentInputDisplayFacts {
                shows_workflow_info: true,
                ..typing()
            },
        ),
        (
            "voltron opened",
            AgentInputDisplayFacts {
                voltron_open: true,
                ..typing()
            },
        ),
        (
            "the pane lost focus",
            AgentInputDisplayFacts {
                pane_focused: false,
                ..typing()
            },
        ),
        (
            "another session became the active one",
            AgentInputDisplayFacts {
                active_session: false,
                ..typing()
            },
        ),
        (
            "command x-ray opened",
            AgentInputDisplayFacts {
                shows_command_x_ray: true,
                ..typing()
            },
        ),
        (
            "the shortcuts view opened",
            AgentInputDisplayFacts {
                shortcut_view_open: true,
                ..typing()
            },
        ),
        (
            "the agent view became inline",
            AgentInputDisplayFacts {
                inline_agent_view: true,
                ..typing()
            },
        ),
        (
            "a prompt was queued (the panel is mounted)",
            AgentInputDisplayFacts {
                shows_queued_prompts_panel: true,
                ..typing()
            },
        ),
        (
            "the pane was resized",
            AgentInputDisplayFacts {
                pane_height_bits: 700.0_f32.to_bits(),
                ..typing()
            },
        ),
    ];
    for (what, now) in changes {
        assert!(
            agent_input_edit_needs_render(Some(typing()), now, true),
            "{what}"
        );
    }
}

#[test]
fn an_input_not_yet_rendered_as_the_agent_input_is_rendered() {
    // The last render showed the terminal input (or nothing yet).
    assert!(agent_input_edit_needs_render(None, typing(), true));
}

#[test]
fn with_the_kill_switch_every_edit_rerenders_the_agent_input() {
    assert!(agent_input_edit_needs_render(
        Some(typing()),
        typing(),
        false
    ));
}
