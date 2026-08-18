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

use std::sync::Arc;

use warp::tui_export::{Session, TuiCompletionCandidate, tui_completion_session_context};
use warp_completer::completer::{
    CompleterOptions, EngineFileType, ExplicitTabCompletion, SuggestionResults, suggestions,
};
use warp_core::SessionId;
use warpui_core::r#async::SpawnedFutureHandle;
use warpui_core::{AppContext, ViewContext};

use super::TuiTerminalSessionView;
use crate::completions_menu::TuiAcceptedCompletion;
use crate::inline_menu::active_inline_menu;
use crate::input::view::TuiCompletionInputSnapshot;
use crate::input_suggestions_mode::TuiInputSuggestionsMode;

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
    /// Warms the sources the completer reads from once a session's shell has
    /// bootstrapped, so the first Tab press does not pay for a cold engine.
    ///
    /// **Partial vs. the oracle.** The pin also warms shell *functions* and
    /// *builtins* here (`Session::load_all_function_names` /
    /// `Session::load_all_builtins`, on background-executor tasks). Neither
    /// loader — nor the deferred-name-set machinery they sit on
    /// (`additional_function_names`, `additional_builtin_names`,
    /// `load_deferred_name_set`, `ShellType::shell_command_to_get_all_functions`
    /// / `_builtins`) — exists in this fork's `Session`. That absence predates
    /// this port and is not part of the ported commit; function and builtin
    /// names still come from the bootstrap snapshot, they are simply never
    /// refreshed in-band afterwards.
    pub(super) fn warm_shell_completion_sources(
        &self,
        session: Arc<Session>,
        ctx: &mut ViewContext<Self>,
    ) {
        ctx.spawn(
            async move { session.load_external_commands().await },
            |_, _, _| {},
        );
    }

    /// Defensive retry for a session whose bootstrap warm-up never ran (or
    /// raced the subscription that triggers it).
    pub(super) fn ensure_external_commands_are_warming(&self, ctx: &mut ViewContext<Self>) {
        let Some(session) = self.active_session.as_ref(ctx).session(ctx) else {
            return;
        };
        if session.has_attempted_to_load_external_commands() {
            return;
        }

        ctx.spawn(
            async move { session.load_external_commands().await },
            |_, _, _| {},
        );
    }

    /// Tab-completion entry point. When the completions popup is already
    /// open, Tab cycles to the next candidate; when some *other* inline menu
    /// is open it owns Tab and the press is swallowed; otherwise Tab fetches
    /// candidates for the token under the cursor from the shared completer
    /// engine and opens the popup.
    pub(super) fn request_shell_completion(&mut self, ctx: &mut ViewContext<Self>) {
        let mode = self.suggestions_mode.as_ref(ctx).mode();
        let other_inline_menu_is_open = mode != TuiInputSuggestionsMode::Completions
            && active_inline_menu(&self.inline_menus, mode, ctx).is_some();
        match shell_completion_tab_action(
            self.completions_menu.as_ref(ctx).is_open(ctx),
            other_inline_menu_is_open,
        ) {
            ShellCompletionTabAction::CycleCandidates => {
                self.completions_menu
                    .update(ctx, |menu, ctx| menu.select_next(ctx));
                ctx.notify();
                return;
            }
            ShellCompletionTabAction::ConsumedByOpenInlineMenu => return,
            ShellCompletionTabAction::RequestCandidates => {}
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
        // `get` (rather than indexing) also guards the out-of-bounds and
        // non-UTF8-boundary replacement spans that the old, TUI-local
        // `should_insert_common_prefix` used to reject explicitly.
        let Some(query) = request
            .input
            .buffer_text
            .get(results.replacement_span.start()..results.replacement_span.end())
        else {
            return;
        };
        let Some(session) = self.active_session.as_ref(ctx).session(ctx) else {
            return;
        };
        let path_separators = session.path_separators();
        let decision = results.explicit_tab_completion(query, path_separators.all);
        let (suggestions, replacement_span, menu_input) = match decision {
            ExplicitTabCompletion::NoAction => return,
            ExplicitTabCompletion::InsertSingle {
                suggestion,
                replacement_span,
            } => {
                let acceptance = TuiAcceptedCompletion {
                    replacement: suggestion.suggestion.replacement.to_string(),
                    span: replacement_span.start()..replacement_span.end(),
                    append_space: request.input.cursor_byte_offset
                        == request.input.buffer_text.len()
                        && suggestion.suggestion.file_type != Some(EngineFileType::Directory),
                };
                self.input_view.update(ctx, |input, ctx| {
                    input.apply_shell_completion(acceptance, ctx)
                });
                return;
            }
            ExplicitTabCompletion::InsertCommonPrefixAndOpen {
                common_prefix,
                suggestions,
                replacement_span,
            } => {
                let acceptance = TuiAcceptedCompletion {
                    replacement: common_prefix,
                    span: replacement_span.start()..replacement_span.end(),
                    append_space: false,
                };
                let did_apply = self.input_view.update(ctx, |input, ctx| {
                    input.apply_shell_completion(acceptance, ctx)
                });
                let menu_input = did_apply
                    .then(|| self.input_view.as_ref(ctx).completion_snapshot(ctx))
                    .flatten()
                    .unwrap_or_else(|| request.input.clone());
                (suggestions, replacement_span, menu_input)
            }
            ExplicitTabCompletion::Open {
                suggestions,
                replacement_span,
            } => (suggestions, replacement_span, request.input.clone()),
        };
        let menu_replacement_range = replacement_span.start()..menu_input.cursor_byte_offset;
        let append_space_at_buffer_end =
            menu_input.cursor_byte_offset == menu_input.buffer_text.len();
        self.completion_request.menu_snapshot = Some(menu_input);
        let candidates = suggestions
            .into_iter()
            .map(|prepared| TuiCompletionCandidate {
                display: prepared.suggestion.display.to_string(),
                replacement: prepared.suggestion.replacement.to_string(),
                description: prepared.suggestion.description.clone(),
                is_directory: prepared.suggestion.file_type == Some(EngineFileType::Directory),
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

/// What a Tab press does, given what is already on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShellCompletionTabAction {
    /// Nothing owns Tab: fetch candidates for the token under the cursor.
    RequestCandidates,
    /// The completions popup is already open: advance to the next candidate.
    CycleCandidates,
    /// A *different* inline menu (slash commands, conversations, MCP, the
    /// model picker, ...) is open, so Tab belongs to it and is swallowed.
    ConsumedByOpenInlineMenu,
}

/// Resolves Tab's owner. Extracted from `request_shell_completion` so the
/// precedence is testable without a live shell session.
///
/// The third arm is the one that used to be missing: the entry point checked
/// only the completions popup, so Tab pressed over any other open inline menu
/// started a completion request behind that menu. `completion_request_is_current`
/// then discarded the results (its `has_active_inline_menu` dimension), which
/// hid the defect -- but the request had already aborted the in-flight one,
/// bumped the generation, and spawned a completer task with a shell round-trip
/// on it. Refusing at the entry point is what the pin does, one layer up, in
/// `TuiInputView::handle_inline_menu_action`.
fn shell_completion_tab_action(
    completions_menu_is_open: bool,
    other_inline_menu_is_open: bool,
) -> ShellCompletionTabAction {
    if completions_menu_is_open {
        ShellCompletionTabAction::CycleCandidates
    } else if other_inline_menu_is_open {
        ShellCompletionTabAction::ConsumedByOpenInlineMenu
    } else {
        ShellCompletionTabAction::RequestCandidates
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

#[cfg(test)]
#[path = "completions_tests.rs"]
mod tests;
