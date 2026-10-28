//! Overflow determination — ported from opencode `packages/opencode/src/session/overflow.ts`
//! (below), with two divergences for small windows (#1090): the reserve on a combined window
//! never exceeds half of it, and an unknown `max_output` reserves a quarter
//! (`config::model_limit_from_parts`); and [`overflow_verdict`] refuses to compact a
//! conversation a summary could not shrink, short of the window itself.
//!
//! ```ts
//! const COMPACTION_BUFFER = 20_000
//!
//! export function usable(input: { cfg, model }) {
//!   const context = input.model.limit.context
//!   if (context === 0) return 0
//!   const reserved = input.cfg.compaction?.reserved
//!     ?? Math.min(COMPACTION_BUFFER, ProviderTransform.maxOutputTokens(input.model))
//!   return input.model.limit.input
//!     ? Math.max(0, input.model.limit.input - reserved)
//!     : Math.max(0, context - ProviderTransform.maxOutputTokens(input.model))
//! }
//!
//! export function isOverflow(input: { cfg, tokens, model }) {
//!   if (input.cfg.compaction?.auto === false) return false
//!   if (input.model.limit.context === 0) return false
//!   const count = input.tokens.total
//!     || input.tokens.input + input.tokens.output + input.tokens.cache.read + input.tokens.cache.write
//!   return count >= usable(input)
//! }
//! ```
use warp_multi_agent_api as api;

use super::consts::COMPACTION_BUFFER;
use super::CompactionConfig;

/// Model token limits — sourced from models.dev metadata or the BYOP provider config.
#[derive(Debug, Clone, Copy)]
pub struct ModelLimit {
    /// Overall context window
    pub context: usize,
    /// Separate input limit (some providers distinguish input/output). 0 means unknown, falling back to context - max_output.
    pub input: usize,
    /// Max output tokens for a single response
    pub max_output: usize,
}

impl ModelLimit {
    /// Conservative fallback when metadata can't be obtained (matches current mainstream Anthropic/OpenAI flagship models).
    pub const FALLBACK: ModelLimit = ModelLimit {
        context: 200_000,
        input: 180_000,
        max_output: 8_000,
    };
}

/// Cumulative token usage for the current conversation — fields match opencode `MessageV2.Assistant.tokens`.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokenCounts {
    /// The total given directly by the LLM (preferred)
    pub total: usize,
    pub input: usize,
    pub output: usize,
    pub cache_read: usize,
    pub cache_write: usize,
}

impl TokenCounts {
    /// Matches opencode: `tokens.total || input+output+cache.read+cache.write`
    pub fn count(&self) -> usize {
        if self.total > 0 {
            self.total
        } else {
            self.input + self.output + self.cache_read + self.cache_write
        }
    }
}

/// The most a summary is expected to take, for judging whether compacting can get a
/// conversation back under the threshold ([`overflow_verdict`]). The template has nine terse
/// sections; real summaries measured 100-500 tokens (round-17 logs: 344-1822 bytes), so 2k
/// is a 4x margin. Never more than half the output reserve ([`expected_summary_tokens`]):
/// the other half covers the summarize request's template (~300 tokens), so a summarize
/// request sent at the threshold fits the window down to a ~2.4k window.
pub const EXPECTED_SUMMARY_TOKENS: usize = 2_000;

/// [`EXPECTED_SUMMARY_TOKENS`], capped at half of `model`'s output reserve.
pub fn expected_summary_tokens(model: ModelLimit) -> usize {
    EXPECTED_SUMMARY_TOKENS.min(model.output_reserve() / 2)
}

impl ModelLimit {
    /// Tokens kept free for the reply on a combined window (no separate input limit):
    /// `max_output`, as `config::model_limit_from_parts` resolved it (a quarter of the window
    /// when unknown, a known value at most half), and never more than half the window here
    /// either, so a positive window always leaves a positive usable budget.
    pub fn output_reserve(&self) -> usize {
        self.max_output.min(self.context / 2)
    }
}

/// Number of usable tokens before the conversation counts as overflowing.
///
/// - Separate input limit: opencode's `input - (cfg.reserved ?? min(COMPACTION_BUFFER,
///   max_output))`, with the reserve capped at half the input limit so it can't reach 0.
/// - Combined window (every BYOP model): `context - ModelLimit::output_reserve` (#1090);
///   opencode's `context - max_output`, with the 20k default for an unknown max_output, hit 0
///   for windows of 20k or less.
pub fn usable(cfg: &CompactionConfig, model: ModelLimit) -> usize {
    if model.context == 0 {
        return 0;
    }
    if model.input > 0 {
        let reserved = cfg
            .reserved
            .unwrap_or_else(|| COMPACTION_BUFFER.min(model.max_output))
            .min(model.input / 2);
        model.input.saturating_sub(reserved)
    } else {
        model.context.saturating_sub(model.output_reserve())
    }
}

/// `count >= usable(...)` is an overflow. Always false when `cfg.auto == false`.
pub fn is_overflow(cfg: &CompactionConfig, tokens: TokenCounts, model: ModelLimit) -> bool {
    if !cfg.auto {
        return false;
    }
    if model.context == 0 {
        return false;
    }
    tokens.count() >= usable(cfg, model)
}

/// What the auto trigger makes of a finished turn's usage. See [`overflow_verdict`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowVerdict {
    /// Under the threshold.
    Under,
    /// Over the threshold, and summarizing the history can bring it back under.
    Over,
    /// Over the threshold and at the window itself ([`at_window`]), although a summary is
    /// not expected to get it back under the threshold: compact anyway, as the last resort
    /// before the provider rejects or truncates the next request. Only when there is
    /// something to summarize (more than a summary's worth).
    OverAtWindow,
    /// At the window, and the history is no larger than a summary: compacting would only
    /// swap it for a summary of the same size, then give up (the #1090 symptom). Typical of a
    /// window barely larger than the fixed prompt (8k with ~6k of system prompt and tools).
    /// Not compacted; the controller tells the user once per conversation that the window
    /// is too small.
    AtWindowCannotHelp,
    /// Over the threshold, but the history a summary could replace is too small for that:
    /// the usage is the fixed part of every request (system prompt, tool definitions, the
    /// environment) plus the tail a compaction keeps verbatim, which compaction does not
    /// touch. Compacting would only replace a short conversation with a summary of about the
    /// same size, and then fail the turn (#1090). The request still fits the window.
    OverButCompactionCannotHelp,
}

impl OverflowVerdict {
    /// Whether the auto trigger treats the turn as overflowing (`FinishedStream::Turn`'s
    /// `overflowing`): [`Self::Over`] and the last resort [`Self::OverAtWindow`]. A turn
    /// compaction can't help is left alone, and after an automatic compaction it counts as
    /// having come in under the threshold.
    pub fn compacts(self) -> bool {
        matches!(self, Self::Over | Self::OverAtWindow)
    }
}

/// [`is_overflow`], plus whether compacting can help.
///
/// `summarizable_tokens`: estimated size of what a summary would replace — the head the
/// summarize request would send, minus what earlier compactions already hide
/// ([`super::commit::summarizable_history_tokens`]). Only called once the usage is over the
/// threshold (it walks the whole history). After a compaction the request carries the rest
/// (fixed overhead and the kept tail) plus the summary; when that is still at or over the
/// threshold, compacting can't help — unless the usage already reached the window and there
/// is something to summarize ([`OverflowVerdict::OverAtWindow`], the last resort).
pub fn overflow_verdict(
    cfg: &CompactionConfig,
    tokens: TokenCounts,
    model: ModelLimit,
    summarizable_tokens: impl FnOnce() -> usize,
) -> OverflowVerdict {
    if !is_overflow(cfg, tokens, model) {
        return OverflowVerdict::Under;
    }
    let count = tokens.count();
    let summary = expected_summary_tokens(model);
    let replaced = summarizable_tokens().min(count);
    // A summary no smaller than what it replaces frees nothing ("never compact a tiny
    // conversation", not even at the window); otherwise it must get the request back under
    // the threshold, or the window must be full (the last resort).
    let worth_summarizing = replaced > summary;
    match (worth_summarizing, at_window(count, model)) {
        (true, _) if count - replaced + summary < usable(cfg, model) => OverflowVerdict::Over,
        (true, true) => OverflowVerdict::OverAtWindow,
        (false, true) => OverflowVerdict::AtWindowCannotHelp,
        (_, false) => OverflowVerdict::OverButCompactionCannotHelp,
    }
}

/// Whether `count` has (all but) reached `model`'s window: 98% of it or more. Not `>=
/// context`: providers rarely report that much — Anthropic and vLLM reject a request over
/// the window instead of answering it, and Ollama truncates the prompt to `num_ctx` so the
/// reported total stays just under — so only a margin makes the last resort reachable.
pub fn at_window(count: usize, model: ModelLimit) -> bool {
    count >= model.context - model.context / AT_WINDOW_MARGIN_DIVISOR
}

/// [`at_window`]'s margin: the last 1/50th (2%) of the window.
pub const AT_WINDOW_MARGIN_DIVISOR: usize = 50;

/// Whether the turn that just finished asked for a tool call (any `ToolCall` among the
/// messages `is_new` accepts), i.e. whether the agent loop is about to continue on its own.
///
/// Auto compaction must not run in that state. The controller learns of a finished stream
/// *before* it queues that stream's tool calls, so a summarize request sent at that moment
/// cancels the loop: the calls never run (the readiness pass answers them with a synthesized
/// cancellation result), the summary is generated, and then nothing continues the task.
/// Instead the compaction waits for that step's tool results and travels with them
/// (`TurnCompactionState::on_tool_results_ready`, #1059), so a loop that never pauses is
/// still compacted.
pub fn turn_has_tool_calls<'a>(
    messages: impl IntoIterator<Item = &'a api::Message>,
    is_new: impl Fn(&str) -> bool,
) -> bool {
    messages
        .into_iter()
        .any(|m| is_new(&m.id) && matches!(m.message, Some(api::message::Message::ToolCall(_))))
}
