//! BYOP local session compaction — replicates opencode
//! `packages/opencode/src/session/{compaction,overflow,summary}.ts` 1:1.
//!
//! Entry-point API:
//! - [`overflow::is_overflow`] — auto-trigger determination (based on LLM response usage)
//! - [`algorithm::select`] — splits into head (sent to the summarization LLM) + tail (kept as-is)
//! - [`algorithm::prune`] — only clears old tool output (doesn't delete messages)
//! - [`prompt::build_prompt`] — builds the summary request text
//!
//! Decoupled from warp's server-side protobuf `SummarizeConversation`; only takes effect on the BYOP path.
pub mod algorithm;
pub mod commit;
pub mod config;
pub mod message_view;
pub mod overflow;
pub mod prompt;
pub mod state;
pub mod token;

pub use config::CompactionConfig;
pub use overflow::{is_overflow, usable};

/// Byte-for-byte matches the constants at the top of opencode `compaction.ts` (lines 33-39, overflow.ts:6, util/token.ts:1).
pub mod consts {
    pub const PRUNE_MINIMUM: usize = 20_000;
    pub const PRUNE_PROTECT: usize = 40_000;
    pub const TOOL_OUTPUT_MAX_CHARS: usize = 2_000;
    pub const DEFAULT_TAIL_TURNS: usize = 2;
    pub const MIN_PRESERVE_RECENT_TOKENS: usize = 2_000;
    pub const MAX_PRESERVE_RECENT_TOKENS: usize = 8_000;
    pub const COMPACTION_BUFFER: usize = 20_000;
    pub const CHARS_PER_TOKEN: usize = 4;
    pub const PRUNE_PROTECTED_TOOLS: &[&str] = &["skill"];
}

#[cfg(test)]
mod tests;
