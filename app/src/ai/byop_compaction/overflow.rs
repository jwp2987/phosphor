//! Overflow determination — ported 1:1 from opencode `packages/opencode/src/session/overflow.ts`.
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

/// Number of usable tokens — `cfg.reserved ?? min(COMPACTION_BUFFER, max_output)` used as a buffer.
pub fn usable(cfg: &CompactionConfig, model: ModelLimit) -> usize {
    if model.context == 0 {
        return 0;
    }
    let reserved = cfg
        .reserved
        .unwrap_or_else(|| COMPACTION_BUFFER.min(model.max_output));
    if model.input > 0 {
        model.input.saturating_sub(reserved)
    } else {
        model.context.saturating_sub(model.max_output)
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
