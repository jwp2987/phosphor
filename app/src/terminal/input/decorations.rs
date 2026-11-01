//! Zap input editor logic related to decorating the input's text, such as
//! applying syntax highlighting and error underlining.

use std::{
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use pathfinder_color::ColorU;
use settings::Setting as _;
use string_offset::{ByteOffset, CharOffset};
use warp_core::features::FeatureFlag;
use warpui::{AppContext, SingletonEntity, ViewContext};

use crate::{
    ai::blocklist::input_model::{RoutingSessionFacts, shell_family},
    appearance::Appearance,
    completer::{EmptyCompletionContext, SessionContext},
    editor::{TextRun, TextStyleOperation},
    settings::InputSettings,
    themes::theme::{AnsiColorIdentifier, AnsiColors},
};

use super::Input;
use warp_completer::util::parse_current_commands_and_tokens_cancellable;

pub use warp_completer::{
    completer::SuggestionTypeName, util::parse_current_commands_and_tokens, ParsedTokenData,
    ParsedTokensSnapshot,
};

/// Input buffers with more chars than this get no background parse: no syntax
/// highlighting, error underlining or input-type detection (#868). Lowered from 1 MiB to
/// 150_000 (#1009), then to 4096 (#1174): the completer lexer/parser is roughly quadratic in
/// the token count, so a 5k-100k char single-line paste kept a core at 100% for over a minute
/// and parked the first Enter. Measured in round 18: 2k chars parsed in ~137 ms, 10k in
/// ~1-2 s, 100k in over 60 s, so the knee sits at 2-5k chars; 4096 is the conservative end
/// that still decorates every command anyone types by hand.
pub(super) const MAX_BACKGROUND_JOB_BUFFER_CHARS: usize = 4096;

/// The token cap that goes with [`MAX_BACKGROUND_JOB_BUFFER_CHARS`] (#1174): the parse cost
/// follows the token count, and a 4096-char buffer of one-char words holds 2048 tokens. A
/// buffer with more whitespace-separated words than this gets no background parse either.
pub(super) const MAX_BACKGROUND_JOB_BUFFER_TOKENS: usize = 1024;

/// Whether `text` has more whitespace-separated words than [`MAX_BACKGROUND_JOB_BUFFER_TOKENS`].
/// Stops counting one past the cap.
pub(super) fn text_exceeds_background_job_token_cap(text: &str) -> bool {
    text.split_whitespace()
        .take(MAX_BACKGROUND_JOB_BUFFER_TOKENS + 1)
        .count()
        > MAX_BACKGROUND_JOB_BUFFER_TOKENS
}

/// Options to enable/disable command decoration and/or AI input background tasks spawned on input
/// edits.
#[derive(Default, Clone, Copy)]
pub struct InputBackgroundJobOptions {
    command_decoration: bool,
    ai_input_detection: bool,
}

impl InputBackgroundJobOptions {
    pub fn with_command_decoration(mut self) -> Self {
        self.command_decoration = true;
        self
    }

    pub fn with_ai_input_detection(mut self) -> Self {
        self.ai_input_detection = true;
        self
    }

    /// Returns `true` if there are no input background jobs to run. Returns `false` if there is at
    /// least one job to run.
    fn no_jobs_to_run(self) -> bool {
        !self.command_decoration && !self.ai_input_detection
    }
}

// Characters that will make us ignore commands for error underlining - largely
// a temporary solution till our parser improves (to handle special edge cases).
// Note that some of these are redundant since we split on tokens such as "&"
// but including them to be defensive.
// TODO: Remove "," once we differentiate between brace expansion and grouped commands
// at the parsing level.
const INVALID_SYMBOLS_COMMAND_ERROR_UNDERLINING: [char; 22] = [
    '~', '`', '#', '$', '&', '*', '(', ')', '\\', '|', '[', ']', '{', '}', ';', '\'', '\"', '<',
    '>', '?', '!', ',',
];

enum CompletionSessionContext {
    Session(SessionContext),
    Empty(EmptyCompletionContext),
}

/// Returns boolean indicating whether we should attempt to red underline
/// the command or not (this is a stop-gap since our parser doesn't cover
/// all the edge cases for commands currently e.g. "!!"). We don't want
/// to incorrectly red underline a valid command. In other words,
/// we would rather miss red underlining an invalid command compared to
/// incorrectly red underlining a valid command.
fn valid_command_for_error_underline(command: &str) -> bool {
    command
        .chars()
        .all(|x| !INVALID_SYMBOLS_COMMAND_ERROR_UNDERLINING.contains(&x))
}

impl Input {
    fn completion_session_context_or_empty_context(
        &self,
        ctx: &AppContext,
    ) -> CompletionSessionContext {
        // Highlighting and input detection parse every token of the line on every edit; on a
        // non-local session `for_decoration` keeps that from running remote generators (#1130).
        self.completion_session_context(ctx)
            .map(|context| CompletionSessionContext::Session(context.for_decoration()))
            .unwrap_or_else(|| CompletionSessionContext::Empty(EmptyCompletionContext::new()))
    }

    /// Whether or not any decorations should be computed and applied to the
    /// input text.
    pub fn should_apply_decorations(&self, ctx: &ViewContext<Self>) -> bool {
        self.should_show_syntax_highlighting(ctx) || self.should_show_error_underlining(ctx)
    }

    /// Whether or not syntax highlighting should be computed and applied to the
    /// input text.
    fn should_show_syntax_highlighting(&self, ctx: &ViewContext<Self>) -> bool {
        *InputSettings::as_ref(ctx).syntax_highlighting.value()
    }

    /// Whether or not error underlining should be computed and applied to the
    /// input text.
    fn should_show_error_underlining(&self, ctx: &ViewContext<Self>) -> bool {
        *InputSettings::as_ref(ctx).error_underlining.value()
    }

    fn run_input_mode_detection(
        &self,
        completion_context: SessionContext,
        ctx: &mut ViewContext<Self>,
    ) {
        if let Some(parsed_token) = self.last_parsed_tokens.clone() {
            // Only classify a parse whose text still awaits a verdict (#833). A parse that
            // finished after the buffer moved on describes text that is no longer there, and
            // one that finished after its text was submitted would set the mode the *next*
            // command is typed into (the shell submit path leaves the command in the editor
            // until its block completes, so a buffer comparison alone cannot tell). Both used to
            // be hidden by the explicit-set window, which #833 narrowed to the buffer it was
            // made for. The newer buffer's own detection was requested by the edit that changed
            // it.
            if parsed_token.buffer_text != self.editor.as_ref(ctx).buffer_text(ctx)
                || !self
                    .ai_input_model
                    .as_ref(ctx)
                    .is_detection_requested_for(&parsed_token.buffer_text)
            {
                return;
            }
            let session_id = completion_context.session.id();
            let facts = RoutingSessionFacts {
                shell: shell_family(completion_context.session.shell().shell_type()),
            };
            self.ai_input_model.update(ctx, |ai_input_model, ctx| {
                ai_input_model.detect_and_set_input_type_with_facts(
                    parsed_token,
                    completion_context,
                    Some(session_id),
                    facts,
                    ctx,
                )
            })
        }
    }

    /// Applies background highlighting to slash command and skill command prefixes that should be
    /// syntax highlighted.
    fn apply_slash_command_prefix_highlighting(
        &mut self,
        buffer_text: &str,
        ctx: &mut ViewContext<Self>,
    ) -> bool {
        let highlighted_prefix_len = self
            .slash_command_model
            .as_ref(ctx)
            .state()
            .command_prefix_highlight_len(buffer_text);

        let Some(highlighted_prefix_len) = highlighted_prefix_len else {
            return false;
        };

        let theme = Appearance::as_ref(ctx).theme();
        let color = theme.ansi_fg_magenta();
        self.editor.update(ctx, |editor, ctx| {
            editor.update_buffer_styles(
                vec![CharOffset::from(0)..CharOffset::from(highlighted_prefix_len)],
                TextStyleOperation::default().set_syntax_color(color),
                ctx,
            )
        });
        true
    }

    /// Highlights registered @ context references so they get a recognizable style
    /// just like slash commands.
    fn apply_at_context_reference_highlighting(
        &mut self,
        buffer_text: &str,
        ctx: &mut ViewContext<Self>,
    ) -> bool {
        let references = self
            .ai_context_model
            .as_ref(ctx)
            .pending_at_context_attachments()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let mut ranges = Vec::new();

        for reference in references {
            let mut search_start = 0;
            while let Some(relative_start) = buffer_text[search_start..].find(&reference) {
                let start_byte = search_start + relative_start;
                let end_byte = start_byte + reference.len();
                let start_char = buffer_text[..start_byte].chars().count();
                let end_char = start_char + reference.chars().count();
                ranges.push(CharOffset::from(start_char)..CharOffset::from(end_char));
                search_start = end_byte;
            }
        }

        if ranges.is_empty() {
            return false;
        }

        let theme = Appearance::as_ref(ctx).theme();
        let color = theme.ansi_fg_magenta();
        self.editor.update(ctx, |editor, ctx| {
            editor.update_buffer_styles(
                ranges,
                TextStyleOperation::default().set_syntax_color(color),
                ctx,
            )
        });
        true
    }

    /// Stops the in-flight decoration/detection job, if any, and returns once the stale job
    /// has been told to stop. Aborting the future alone is not enough: the parse is CPU-bound
    /// and only notices an abort at a poll boundary, so the job also gets a cooperative
    /// cancel flag that its loops check between tokens (#1031).
    pub(super) fn cancel_decorations_job(&mut self) {
        if let Some(cancel) = self.decorations_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        if let Some(handle) = self.decorations_future_handle.take() {
            handle.abort_handle().abort();
        }
    }

    /// Whether the buffer is over [`MAX_BACKGROUND_JOB_BUFFER_CHARS`] chars or
    /// [`MAX_BACKGROUND_JOB_BUFFER_TOKENS`] words, so no background parse (and so no input-type
    /// verdict) will run for it. The length check reads the cached length, no copy; the text
    /// is only copied (at most the char cap) when it is long enough to hold that many words.
    pub(super) fn buffer_exceeds_background_job_cap(&self, ctx: &AppContext) -> bool {
        let len = self.editor.as_ref(ctx).buffer_size(ctx).as_usize();
        if len > MAX_BACKGROUND_JOB_BUFFER_CHARS {
            return true;
        }
        // N words need at least 2N-1 chars, so shorter buffers cannot be over the token cap.
        len >= 2 * MAX_BACKGROUND_JOB_BUFFER_TOKENS
            && text_exceeds_background_job_token_cap(&self.editor.as_ref(ctx).buffer_text(ctx))
    }

    /// Computes information about the currently-entered command in a background
    /// task and then uses it to decorate the input, specifically applying
    /// styles for syntax highlighting and error underlining.
    /// Includes a short-circuit that lets us clear formatting and return without parsing the input.
    pub fn run_input_background_jobs(
        &mut self,
        mode: InputBackgroundJobOptions,
        ctx: &mut ViewContext<Self>,
    ) {
        if mode.no_jobs_to_run() {
            return;
        }

        let mut mode = mode;

        // A buffer this large is a recalled history line or a paste, not a command anyone
        // reads token by token (#868: a 50 MB history line recalled with Up). Parsing it for
        // highlighting, error underlining and input-type detection copies and walks the
        // whole buffer on every edit and can start completion commands over it, so skip
        // the jobs and leave the text plain. The size check reads the buffer's cached
        // length, so an oversized buffer is never copied out here.
        if self.buffer_exceeds_background_job_cap(ctx) {
            self.cancel_decorations_job();
            // No verdict will ever come for this text: a request left pending would make
            // Enter wait for one and swallow the first Enter (#1036).
            self.ai_input_model
                .update(ctx, |model, _| model.abandon_pending_detection());
            // Clearing walks the whole buffer's styles, so do it once per oversized
            // stretch, not on every edit.
            if !self.decorations_cleared_for_oversized_buffer {
                self.clear_decorations_unchecked(ctx);
                self.decorations_cleared_for_oversized_buffer = true;
            }
            return;
        }
        self.decorations_cleared_for_oversized_buffer = false;

        // We don't show input command decorations in AI mode, but we keep slash command prefix highlighting.
        let buffer_text = self.editor.as_ref(ctx).buffer_text(ctx);
        if self.ai_input_model.as_ref(ctx).is_ai_input_enabled()
            || (FeatureFlag::AgentView.is_enabled()
                && self
                    .slash_command_model
                    .as_ref(ctx)
                    .state()
                    .is_detected_command_or_skill())
        {
            self.clear_decorations(ctx);
            self.apply_slash_command_prefix_highlighting(&buffer_text, ctx);
            self.apply_at_context_reference_highlighting(&buffer_text, ctx);
            mode.command_decoration = false;

            // Return early because there are no input background jobs to run.
            if mode.no_jobs_to_run() {
                // A job started for earlier text would only decorate text that is gone.
                self.cancel_decorations_job();
                return;
            }
        }

        match self.completion_session_context_or_empty_context(ctx) {
            CompletionSessionContext::Session(completion_context) => {
                // #1158: a parse that feeds input-type detection runs no generators, on any
                // session. Enter can be waiting on its verdict, and a generator is a process
                // (local) or a command on the shared ssh connection (remote) that takes
                // seconds for `systemctl`; the classifier needs only the command word, aliases,
                // history and syntax.
                let completion_context = if mode.ai_input_detection {
                    completion_context.for_classification()
                } else {
                    completion_context
                };
                let editor = self.editor.as_ref(ctx);
                let buffer_text = editor.buffer_text(ctx);

                if matches!(&self.last_parsed_tokens, Some(last_parsed_tokens) if buffer_text == last_parsed_tokens.buffer_text)
                {
                    // The cached parse already describes this text, so any in-flight job
                    // is for different (stale) text and must not overwrite it.
                    self.cancel_decorations_job();
                    if mode.ai_input_detection {
                        self.run_input_mode_detection(completion_context, ctx);
                    }

                    if mode.command_decoration {
                        self.apply_decorations(ctx);
                    }

                    return;
                }

                self.cancel_decorations_job();
                let cancel = Arc::new(AtomicBool::new(false));
                self.decorations_cancel = Some(cancel.clone());

                self.decorations_future_handle = Some(ctx.spawn_abortable(
                    async move {
                        // #1019: a program installed since the bootstrap scan is a command.
                        completion_context
                            .session
                            .discover_command_word_in_buffer(&buffer_text);
                        (
                            parse_current_commands_and_tokens_cancellable(
                                buffer_text,
                                &completion_context,
                                &cancel,
                            )
                            .await,
                            completion_context,
                        )
                    },
                    move |input, (parsed_tokens, completion_context), ctx| {
                        // A cancelled parse was superseded (or the buffer was cleared); its
                        // newer replacement owns the decorations.
                        let Some(parsed_tokens) = parsed_tokens else {
                            return;
                        };
                        input.last_parsed_tokens = Some(parsed_tokens);

                        if mode.ai_input_detection {
                            input.run_input_mode_detection(completion_context, ctx);
                        }

                        if mode.command_decoration {
                            input.apply_decorations(ctx);
                        }
                    },
                    move |_, _| {
                        // Superseding this job drops its future, which cancels exactly the in-band commands it
                        // issued (`InBandCommandExecutor::execute_command`'s `on_cancel`; the local executor kills
                        // its own process group). Do NOT call `cancel_active_commands()` here: it is session-wide
                        // and would also kill other jobs' in-flight Tab completions (#1352).
                    },
                ));
            }
            CompletionSessionContext::Empty(detection_ctx) => {
                if mode.ai_input_detection {
                    // No session context available (e.g., shared session viewer).
                    // Use a dedicated detection context that does not expose top-level commands.
                    let buffer_text = self.editor.as_ref(ctx).buffer_text(ctx);
                    let ai_input_model = self.ai_input_model.clone();
                    // No abort handle is kept for this job, but it still gets the cancel
                    // flag so the next edit stops it (#1031).
                    self.cancel_decorations_job();
                    let cancel = Arc::new(AtomicBool::new(false));
                    self.decorations_cancel = Some(cancel.clone());
                    ctx.spawn(
                        async move {
                            parse_current_commands_and_tokens_cancellable(
                                buffer_text,
                                &detection_ctx,
                                &cancel,
                            )
                            .await
                        },
                        move |input, parsed_tokens, ctx| {
                            let Some(parsed_tokens) = parsed_tokens else {
                                return;
                            };
                            // Stale or already-submitted parse: see `run_input_mode_detection`.
                            let buffer_text = input.editor.as_ref(ctx).buffer_text(ctx);
                            if parsed_tokens.buffer_text != buffer_text
                                || !ai_input_model
                                    .as_ref(ctx)
                                    .is_detection_requested_for(&parsed_tokens.buffer_text)
                            {
                                return;
                            }
                            ai_input_model.update(ctx, |model, ctx| {
                                model.detect_and_set_input_type(
                                    parsed_tokens,
                                    EmptyCompletionContext::new(),
                                    None,
                                    ctx,
                                );
                            });
                        },
                    );
                }
            }
        }
    }

    /// Applies error underlining and/or syntax highlighting as appropriate,
    /// using the result of the last parse operation.
    ///
    /// Every style edit relayouts the editor and requests a frame, so when the buffer already
    /// carries exactly the decorations this parse calls for (typing a character usually leaves
    /// most tokens' styling unchanged) nothing is touched (#1133). The comparison reads the
    /// styles actually present in the buffer rather than remembering what was applied last, so
    /// edits, undo and theme changes can never leave it comparing against stale state.
    fn apply_decorations(&mut self, ctx: &mut ViewContext<Self>) {
        let Some(parsed_tokens_snapshot) = &self.last_parsed_tokens else {
            return;
        };
        let buffer_text = self.editor.as_ref(ctx).buffer_text(ctx);
        if buffer_text != parsed_tokens_snapshot.buffer_text {
            // Our state is out-of-date (parsed_tokens no longer applies to the
            // updated state of the buffer) and this should be a no-op since
            // another async callback is likely handling or already handled
            // this (for a later state) i.e. race condition.
            return;
        }

        let theme = Appearance::as_ref(ctx).theme();
        let terminal_colors_normal = theme.terminal_colors().normal;

        let desired = DecorationSet::for_tokens(
            parsed_tokens_snapshot,
            self.should_show_syntax_highlighting(ctx),
            self.should_show_error_underlining(ctx),
            &terminal_colors_normal,
        );
        let current = DecorationSet::from_text_runs(self.editor.as_ref(ctx).text_style_runs(ctx));
        if desired == current {
            return;
        }

        // Clear all decorations before applying the updated ones. The comparison above already
        // read the buffer's styles, so skip the guard's second scan.
        self.clear_decorations_unchecked(ctx);
        self.apply_decoration_set(desired, ctx);
    }

    /// Whether the buffer carries any syntax color or error underline, i.e. whether
    /// [`Self::clear_decorations`] would change anything.
    pub(super) fn buffer_has_decorations(&self, ctx: &AppContext) -> bool {
        self.editor.as_ref(ctx).text_style_runs(ctx).any(|run| {
            let style = run.text_style();
            style.syntax_color.is_some() || style.error_underline_color.is_some()
        })
    }

    /// Removes decorations (error underlining, syntax highlighting, and background colors) from the input buffer.
    ///
    /// Every style edit relayouts the editor, resets its cursor blink and requests a frame even
    /// when it changes nothing, and an AI-mode buffer is cleared on every keystroke while it has
    /// no decorations at all. So a buffer that carries none is left alone: that clear was a
    /// second, visually identical frame ~10 ms after each key (#1298).
    pub(super) fn clear_decorations(&mut self, ctx: &mut ViewContext<Self>) {
        if self.buffer_has_decorations(ctx) {
            self.clear_decorations_unchecked(ctx);
        }
    }

    /// [`Self::clear_decorations`] without looking at the buffer's styles first, for callers that
    /// already know there is something to clear or that must not walk a huge buffer's runs.
    pub(super) fn clear_decorations_unchecked(&mut self, ctx: &mut ViewContext<Self>) {
        self.editor.update(ctx, |editor, ctx| {
            editor.update_buffer_styles(
                vec![CharOffset::from(0)..editor.buffer_size(ctx)],
                TextStyleOperation::default().clear_decorations(),
                ctx,
            )
        });
    }

    /// Applies the given decorations. This does not unset any existing decorations in the
    /// editor buffer.
    fn apply_decoration_set(&mut self, set: DecorationSet, ctx: &mut ViewContext<Self>) {
        if !set.error_underline.is_empty() {
            // Error underlines are all one color, so this is a single edit.
            let color = set.error_underline[0].1;
            let ranges: Vec<_> = set.error_underline.into_iter().map(|(r, _)| r).collect();
            self.editor.update(ctx, |editor, ctx| {
                editor.update_buffer_styles(
                    ranges,
                    TextStyleOperation::default().set_error_underline_color(color),
                    ctx,
                )
            });
        }

        // One edit per distinct syntax color.
        let mut by_color: Vec<(ColorU, Vec<Range<ByteOffset>>)> = vec![];
        for (range, color) in set.syntax {
            match by_color.iter_mut().find(|(c, _)| *c == color) {
                Some((_, ranges)) => ranges.push(range),
                None => by_color.push((color, vec![range])),
            }
        }
        for (color, ranges) in by_color {
            self.editor.update(ctx, |editor, ctx| {
                editor.update_buffer_styles(
                    ranges,
                    TextStyleOperation::default().set_syntax_color(color),
                    ctx,
                )
            });
        }
    }
}

/// The decorations (syntax colors and error underlines) of an input buffer, as sorted,
/// non-empty byte ranges with touching same-colored ranges merged. The same normal form is
/// built from a fresh parse and from the styles actually in the editor buffer, so two sets are
/// equal exactly when applying one would change nothing visible.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct DecorationSet {
    pub(super) syntax: Vec<(Range<ByteOffset>, ColorU)>,
    pub(super) error_underline: Vec<(Range<ByteOffset>, ColorU)>,
}

fn normalize_ranges(
    mut ranges: Vec<(Range<ByteOffset>, ColorU)>,
) -> Vec<(Range<ByteOffset>, ColorU)> {
    ranges.retain(|(r, _)| r.start < r.end);
    ranges.sort_by_key(|(r, _)| r.start);
    let mut merged: Vec<(Range<ByteOffset>, ColorU)> = Vec::with_capacity(ranges.len());
    for (range, color) in ranges {
        match merged.last_mut() {
            Some((last, last_color)) if last.end == range.start && *last_color == color => {
                last.end = range.end;
            }
            _ => merged.push((range, color)),
        }
    }
    merged
}

impl DecorationSet {
    pub(super) fn new(
        syntax: Vec<(Range<ByteOffset>, ColorU)>,
        error_underline: Vec<(Range<ByteOffset>, ColorU)>,
    ) -> Self {
        Self {
            syntax: normalize_ranges(syntax),
            error_underline: normalize_ranges(error_underline),
        }
    }

    /// The decorations a parse calls for, given which kinds are enabled and the theme colors.
    pub(super) fn for_tokens(
        snapshot: &ParsedTokensSnapshot,
        show_syntax: bool,
        show_error_underline: bool,
        terminal_colors_normal: &AnsiColors,
    ) -> Self {
        let mut syntax = vec![];
        let mut error_underline = vec![];
        for token_data in &snapshot.parsed_tokens {
            let range = ByteOffset::from(token_data.token.span.start())
                ..ByteOffset::from(token_data.token.span.end());
            if let Some(description) = &token_data.token_description {
                if show_syntax {
                    let color: AnsiColorIdentifier = description.suggestion_type.to_name().into();
                    syntax.push((range, color.to_ansi_color(terminal_colors_normal).into()));
                }
            } else if show_error_underline
                && token_data.token_index == 0
                && valid_command_for_error_underline(&token_data.token.item)
            {
                error_underline.push((
                    range,
                    AnsiColorIdentifier::Red
                        .to_ansi_color(terminal_colors_normal)
                        .into(),
                ));
            }
        }
        Self::new(syntax, error_underline)
    }

    /// The decorations currently present in the buffer, read from its style runs.
    pub(super) fn from_text_runs(runs: impl Iterator<Item = TextRun>) -> Self {
        let mut syntax = vec![];
        let mut error_underline = vec![];
        for run in runs {
            let style = run.text_style();
            if let Some(color) = style.syntax_color {
                syntax.push((run.byte_range().clone(), color));
            }
            if let Some(color) = style.error_underline_color {
                error_underline.push((run.byte_range().clone(), color));
            }
        }
        Self::new(syntax, error_underline)
    }
}

#[cfg(test)]
#[path = "decorations_tests.rs"]
mod tests;
