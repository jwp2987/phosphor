use warpui::{SingletonEntity, async_assert, async_assert_eq, integration::AssertionCallback};

use crate::{
    ai::llms::LLMPreferences,
    integration_testing::view_getters::{input_view, single_input_view_for_tab},
    terminal::input::{
        InputSuggestionsMode,
        models::{ModelPickerChoice, query_model_picker_choices},
    },
};

pub fn assert_workflow_info_box_is_open(tab_idx: usize, pane_idx: usize) -> AssertionCallback {
    Box::new(move |app, window_id| {
        let input = input_view(app, window_id, tab_idx, pane_idx);
        input.read(app, |input, _ctx| {
            async_assert!(input.is_workflows_info_box_open())
        })
    })
}

pub fn input_editor_is_focused(tab_idx: usize) -> AssertionCallback {
    Box::new(move |app, window_id| {
        let input = single_input_view_for_tab(app, window_id, tab_idx);
        input.read(app, |input, ctx| {
            async_assert!(
                input.editor().is_focused(ctx),
                "Input editor should be focused"
            )
        })
    })
}

pub fn input_editor_is_not_focused(tab_idx: usize) -> AssertionCallback {
    Box::new(move |app, window_id| {
        let input = single_input_view_for_tab(app, window_id, tab_idx);
        input.read(app, |input, ctx| {
            async_assert!(
                !input.editor().is_focused(ctx),
                "Input editor should not be focused"
            )
        })
    })
}

pub fn input_contains_string(tab_idx: usize, string: String) -> AssertionCallback {
    Box::new(move |app, window_id| {
        let input = single_input_view_for_tab(app, window_id, tab_idx);
        input.read(app, |view, ctx| {
            async_assert_eq!(
                view.buffer_text(ctx),
                string,
                "Input should contain string {string}"
            )
        })
    })
}

pub fn input_is_empty(tab_idx: usize) -> AssertionCallback {
    Box::new(move |app, window_id| {
        let input = single_input_view_for_tab(app, window_id, tab_idx);
        input.read(app, |view, ctx| {
            async_assert!(view.buffer_text(ctx).is_empty(), "Input should be empty")
        })
    })
}

pub fn inline_model_selector_is_open(tab_idx: usize) -> AssertionCallback {
    Box::new(move |app, window_id| {
        let input = single_input_view_for_tab(app, window_id, tab_idx);
        input.read(app, |view, ctx| {
            async_assert_eq!(
                view.suggestions_mode_model().as_ref(ctx).mode(),
                &InputSuggestionsMode::ModelSelector,
                "Inline model selector should be open"
            )
        })
    })
}

pub fn tab_completions_menu_is_open(tab_idx: usize, is_opened: bool) -> AssertionCallback {
    Box::new(move |app, window_id| {
        let input = single_input_view_for_tab(app, window_id, tab_idx);
        input.read(app, |view, ctx| {
            let assertion = if is_opened {
                matches!(
                    view.suggestions_mode_model().as_ref(ctx).mode(),
                    InputSuggestionsMode::CompletionSuggestions { .. }
                )
            } else {
                matches!(
                    view.suggestions_mode_model().as_ref(ctx).mode(),
                    InputSuggestionsMode::Closed
                )
            };

            async_assert!(assertion)
        })
    })
}

pub fn latest_buffer_operations_are_empty(
    tab_idx: usize,
    should_be_empty: bool,
) -> AssertionCallback {
    Box::new(move |app, window_id| {
        let input = single_input_view_for_tab(app, window_id, tab_idx);
        input.read(app, |view, _ctx| {
            if should_be_empty {
                async_assert!(view.latest_buffer_operations().count() == 0)
            } else {
                async_assert!(view.latest_buffer_operations().count() > 0)
            }
        })
    })
}

#[derive(Clone)]
pub enum AutosuggestionState {
    /// The autosuggestion is inactive.
    Closed,
    /// The autosuggestion is active with _some_ text.
    Active,
    /// The autosuggestion is active and is specifically some text.
    ActiveWithText(String),
}

pub fn assert_autosuggestion_state(
    tab_idx: usize,
    state: AutosuggestionState,
) -> AssertionCallback {
    Box::new(move |app, window_id| {
        let input = single_input_view_for_tab(app, window_id, tab_idx);
        let state = state.clone();
        input.read(app, move |view, ctx| {
            let autosuggestion = view.editor().as_ref(ctx).current_autosuggestion_text();
            let assertion = match state {
                AutosuggestionState::Closed => autosuggestion.is_none(),
                AutosuggestionState::Active => autosuggestion.is_some(),
                AutosuggestionState::ActiveWithText(expected) => {
                    autosuggestion.is_some_and(|s| expected.as_str() == s)
                }
            };
            async_assert!(assertion)
        })
    })
}

/// Asserts that at least one *selectable* model in the inline model picker
/// matches `query_text`.
///
/// Guards inline-model-selector tests against passing vacuously. This fork
/// builds the picker's model list entirely from the user's configured BYOP
/// providers (`build_byop_models_by_feature`); the Warp-hosted catalog — "auto"
/// included — is not present. A fresh profile with no provider configured holds
/// exactly one entry, `placeholder_llm_info()`, which carries
/// `DisableReason::Unavailable`. Pressing enter over a list with no selectable
/// row emits no `SelectedModel` event at all, so a test that means to exercise
/// model *selection* silently exercises nothing.
pub fn selectable_model_matches(query_text: &'static str) -> AssertionCallback {
    Box::new(move |app, _window_id| {
        let selectable = app.read(|ctx| {
            let choices = LLMPreferences::as_ref(ctx)
                .get_base_llm_choices_for_agent_mode()
                .collect::<Vec<_>>();
            query_model_picker_choices(choices, query_text, ctx)
                .into_iter()
                .filter(ModelPickerChoice::is_selectable)
                .map(|choice| choice.llm.display_name.clone())
                .collect::<Vec<_>>()
        });
        async_assert!(
            !selectable.is_empty(),
            "no selectable model matches {query_text:?}, so pressing enter in the \
             inline model selector cannot accept one"
        )
    })
}
