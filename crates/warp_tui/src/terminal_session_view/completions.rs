//! Asynchronous shell-command completion coordination for the TUI composer.
//!
//! Ported from the pin (`02b53fcd8`) for #390, with two deliberate
//! deviations from the pin, both explained where they matter below:
//!
//! - Not gated to shell mode. This fork's Tab-completion binding
//!   (`TRIGGER_COMPLETIONS_BINDING_NAME`) is available in both shell and
//!   agent-composer input, not shell-only as in the pin -- see
//!   `TuiInputView::completion_snapshot`'s doc comment.
//! - No per-session environment variables are threaded into the completer
//!   (`suggestions()`'s `session_env_vars` parameter is always `None` here).
//!   This fork's `TuiTerminalSessionView` has no handle to the `Sessions`
//!   registry `Session::get_env_vars_for_session` needs; wiring one in is a
//!   separate, larger change than porting this file's completion richness.

use warp::tui_export::{
    TuiCompletionCandidate, longest_common_prefix, tui_completion_session_context,
};
use warp_completer::completer::{
    CompleterOptions, EngineFileType, Match, SuggestionResults, suggestions,
};
use warp_core::SessionId;
use warpui_core::r#async::SpawnedFutureHandle;
use warpui_core::{AppContext, ViewContext};

use super::TuiTerminalSessionView;
use crate::completions_menu::TuiAcceptedCompletion;
use crate::inline_menu::active_inline_menu;
use crate::input::view::TuiCompletionInputSnapshot;

#[derive(Default)]
pub(super) struct CompletionRequestState {
    future: Option<SpawnedFutureHandle>,
    generation: u64,
    menu_snapshot: Option<TuiCompletionInputSnapshot>,
}

#[derive(Clone, Debug)]
struct CompletionRequestSnapshot {
    input: TuiCompletionInputSnapshot,
    session_id: SessionId,
    current_working_directory: String,
    generation: u64,
}

impl TuiTerminalSessionView {
    /// Tab-completion entry point. When the completions popup is already
    /// open, Tab cycles to the next candidate; otherwise it fetches
    /// candidates for the token under the cursor from the shared completer
    /// engine and opens the popup.
    pub(super) fn request_shell_completion(&mut self, ctx: &mut ViewContext<Self>) {
        if self.completions_menu.as_ref(ctx).is_open(ctx) {
            self.completions_menu
                .update(ctx, |menu, ctx| menu.select_next(ctx));
            ctx.notify();
            return;
        }
        let Some(input) = self.input_view.as_ref(ctx).completion_snapshot(ctx) else {
            return;
        };
        let Some(current_working_directory) = self.current_working_directory(ctx) else {
            return;
        };
        let Some(completion_context) = tui_completion_session_context(
            self.active_session.as_ref(ctx),
            current_working_directory.clone(),
            ctx,
        ) else {
            return;
        };
        let session_id = completion_context.session.id();
        self.abort_shell_completion(ctx);
        self.completion_request.generation = self.completion_request.generation.wrapping_add(1);
        let generation = self.completion_request.generation;
        let request = CompletionRequestSnapshot {
            input,
            session_id,
            current_working_directory,
            generation,
        };
        let line = request.input.buffer_text[..request.input.cursor_byte_offset].to_owned();
        let cursor_byte_offset = request.input.cursor_byte_offset;
        let completion_session = completion_context.session.clone();
        self.completion_request.future = Some(ctx.spawn_abortable(
            async move {
                let results = suggestions(
                    &line,
                    cursor_byte_offset,
                    None,
                    CompleterOptions::default(),
                    &completion_context,
                )
                .await;
                (request, results)
            },
            |view, (request, results), ctx| {
                view.handle_shell_completion_results(request, results, ctx);
            },
            move |_, _| completion_session.cancel_active_commands(),
        ));
    }

    pub(super) fn abort_shell_completion(&mut self, ctx: &mut ViewContext<Self>) {
        if let Some(future) = self.completion_request.future.take() {
            future.abort();
        }
        self.completion_request.generation = self.completion_request.generation.wrapping_add(1);
        self.completion_request.menu_snapshot = None;
        self.completions_menu
            .update(ctx, |menu, ctx| menu.dismiss(ctx));
    }

    /// Called on every input-editor selection/content change and input-mode
    /// switch: re-checks whether an open completions popup's snapshot still
    /// matches the live buffer, aborting the request/closing the popup when
    /// it has gone stale (the user kept typing while a fetch was in flight,
    /// or moved off the token the popup was completing).
    pub(super) fn handle_completion_editor_changed(&mut self, ctx: &mut ViewContext<Self>) {
        let current_snapshot = self.input_view.as_ref(ctx).completion_snapshot(ctx);
        let preserves_open_menu = self.completions_menu.as_ref(ctx).is_open(ctx)
            && current_snapshot.as_ref() == self.completion_request.menu_snapshot.as_ref();
        if !preserves_open_menu {
            self.abort_shell_completion(ctx);
        }
    }

    fn handle_shell_completion_results(
        &mut self,
        request: CompletionRequestSnapshot,
        results: Option<SuggestionResults>,
        ctx: &mut ViewContext<Self>,
    ) {
        if self.completion_request.generation == request.generation {
            self.completion_request.future = None;
        }
        if !self.completion_result_is_applicable(&request, ctx) {
            return;
        }
        let Some(results) = results.filter(|results| !results.suggestions.is_empty()) else {
            return;
        };
        let replacement_range = results.replacement_span.start()..results.replacement_span.end();
        let append_space_at_buffer_end =
            request.input.cursor_byte_offset == request.input.buffer_text.len();

        if let Some(suggestion) = results.single_prefix_suggestion() {
            let acceptance = TuiAcceptedCompletion {
                replacement: suggestion.replacement().to_owned(),
                span: replacement_range,
                append_space: append_space_at_buffer_end
                    && suggestion.suggestion.file_type != Some(EngineFileType::Directory),
            };
            self.input_view.update(ctx, |input, ctx| {
                input.apply_shell_completion(acceptance, ctx)
            });
            return;
        }

        let common_prefix = longest_common_prefix(
            results
                .suggestions
                .iter()
                .filter(|suggestion| {
                    matches!(
                        suggestion.match_type,
                        Match::Prefix {
                            is_case_sensitive: true
                        } | Match::Exact {
                            is_case_sensitive: true
                        }
                    )
                })
                .map(|suggestion| suggestion.replacement()),
        )
        .map(str::to_owned);
        let menu_input = common_prefix
            .filter(|prefix| {
                should_insert_common_prefix(
                    prefix,
                    &request.input,
                    results.replacement_span.start(),
                    results.replacement_span.distance(),
                )
            })
            .and_then(|prefix| {
                let acceptance = TuiAcceptedCompletion {
                    replacement: prefix,
                    span: replacement_range.clone(),
                    append_space: false,
                };
                let did_apply = self.input_view.update(ctx, |input, ctx| {
                    input.apply_shell_completion(acceptance, ctx)
                });
                did_apply.then(|| self.input_view.as_ref(ctx).completion_snapshot(ctx))?
            })
            .unwrap_or_else(|| request.input.clone());
        let menu_replacement_range =
            results.replacement_span.start()..menu_input.cursor_byte_offset;
        let append_space_at_buffer_end =
            menu_input.cursor_byte_offset == menu_input.buffer_text.len();
        self.completion_request.menu_snapshot = Some(menu_input);
        let candidates = results
            .suggestions
            .into_iter()
            .map(|matched| TuiCompletionCandidate {
                display: matched.display().to_owned(),
                replacement: matched.replacement().to_owned(),
                description: matched.description(),
                is_directory: matched.suggestion.file_type == Some(EngineFileType::Directory),
            })
            .collect::<Vec<_>>();
        self.completions_menu.update(ctx, |menu, ctx| {
            menu.show(
                candidates,
                menu_replacement_range,
                append_space_at_buffer_end,
                ctx,
            );
        });
    }

    fn completion_result_is_applicable(
        &self,
        request: &CompletionRequestSnapshot,
        ctx: &AppContext,
    ) -> bool {
        let current_input = self.input_view.as_ref(ctx).completion_snapshot(ctx);
        let current_session_id = self
            .active_session
            .as_ref(ctx)
            .session(ctx)
            .map(|session| session.id());
        let current_working_directory = self.current_working_directory(ctx);
        let has_active_inline_menu = active_inline_menu(
            &self.inline_menus,
            self.suggestions_mode.as_ref(ctx).mode(),
            ctx,
        )
        .is_some();
        completion_request_is_current(
            request,
            self.completion_request.generation,
            current_input.as_ref(),
            current_session_id,
            current_working_directory.as_deref(),
            has_active_inline_menu,
        )
    }
}

fn completion_request_is_current(
    request: &CompletionRequestSnapshot,
    current_generation: u64,
    current_input: Option<&TuiCompletionInputSnapshot>,
    current_session_id: Option<SessionId>,
    current_working_directory: Option<&str>,
    has_active_inline_menu: bool,
) -> bool {
    current_generation == request.generation
        && current_input == Some(&request.input)
        && current_session_id == Some(request.session_id)
        && current_working_directory == Some(request.current_working_directory.as_str())
        && !has_active_inline_menu
}

fn should_insert_common_prefix(
    common_prefix: &str,
    input: &TuiCompletionInputSnapshot,
    replacement_start: usize,
    replacement_distance: usize,
) -> bool {
    let Some(current_word) = input
        .buffer_text
        .get(replacement_start..input.cursor_byte_offset)
    else {
        return false;
    };
    common_prefix.len() > replacement_distance && common_prefix.starts_with(current_word)
}

#[cfg(test)]
#[path = "completions_tests.rs"]
mod tests;
