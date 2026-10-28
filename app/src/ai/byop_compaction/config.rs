//! Compaction config — matches opencode's `Config.compaction`:
//!
//! ```ts
//! compaction: {
//!   auto?: boolean,                  // default: true
//!   prune?: boolean,                 // default: true
//!   tail_turns?: NonNegativeInt,     // default: 2
//!   preserve_recent_tokens?: NonNegativeInt,
//!   reserved?: NonNegativeInt,
//! }
//! ```
//!
//! On warp's side this lives in settings/ai.rs's BYOPCompactionSettings, and gets
//! converted into this struct after deserialization.
use serde::{Deserialize, Serialize};

use super::consts::COMPACTION_BUFFER;
use super::overflow::ModelLimit;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionConfig {
    /// Auto overflow-trigger toggle. Defaults to true.
    pub auto: bool,
    /// Tool output prune toggle. Defaults to true.
    pub prune: bool,
    /// How many recent user turns to keep as the tail. Defaults to 2.
    pub tail_turns: usize,
    /// Force-overrides `preserve_recent_budget` (tokens). None uses opencode's formula.
    pub preserve_recent_tokens: Option<usize>,
    /// Force-overrides the reserved buffer (tokens) in `usable()` when the model has a separate
    /// input limit. None takes min(20_000, max_output).
    pub reserved: Option<usize>,
    /// A model reference dedicated to summarization (optional). If set, it's used; otherwise falls back to the conversation's current model.
    pub compaction_model: Option<CompactionModelRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompactionModelRef {
    pub provider_id: String,
    pub model_id: String,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            auto: true,
            prune: true,
            tail_turns: super::consts::DEFAULT_TAIL_TURNS,
            preserve_recent_tokens: None,
            reserved: None,
            compaction_model: None,
        }
    }
}

impl CompactionConfig {
    /// Computes the actual preserve_recent_budget — matches opencode
    /// `compaction.ts:134-139`:
    /// `cfg.preserve_recent_tokens ?? min(MAX, max(MIN, floor(usable * 0.25)))`,
    /// and never more than half of `usable` (#1090): opencode's 2k floor alone would keep a
    /// tail bigger than a tiny window's whole budget. Changes nothing from `usable >= 4_000`.
    pub fn preserve_recent_budget(&self, usable_tokens: usize) -> usize {
        use super::consts::{MAX_PRESERVE_RECENT_TOKENS, MIN_PRESERVE_RECENT_TOKENS};
        self.preserve_recent_tokens.unwrap_or_else(|| {
            MAX_PRESERVE_RECENT_TOKENS
                .min(MIN_PRESERVE_RECENT_TOKENS.max(usable_tokens / 4))
                .min(usable_tokens / 2)
        })
    }

    /// Token limits for the model the agent would send to right now.
    ///
    /// `overflow::usable` was a 1:1 port of opencode, where `model.limit` always arrives from
    /// models.dev metadata. Warp has no such source at the auto-overflow call site, so it
    /// passed [`ModelLimit::FALLBACK`] — a hardcoded 200k/180k/8k — for *every* model, and the
    /// trigger sat at `180_000 - 8_000 = 172_000` tokens no matter what was configured.
    /// Observed 2026-08-14 on `claude-opus-5` over Vertex: `auto overflow detected:
    /// tokens=172157 usable=172000` in the same minute as `context usage: window=1000000 used=
    /// 156646 → 15.7%`. A 1M window compacted at 15.7% — about 83% of the paid-for context
    /// discarded, and `chat_stream.rs`'s claim that the configured window "is the budget ZAP
    /// compacts against" was false.
    ///
    /// The configured window (`AgentProviderModel::context_window`) is the same number the `%`
    /// meter and `/usage` already report, so this makes the trigger agree with what the user
    /// is shown.
    pub fn model_limit(app: &warpui::AppContext) -> super::overflow::ModelLimit {
        match crate::ai::usage_cost::active_byop_model(app) {
            Some((_, model)) => {
                model_limit_from_parts(model.context_window, model.max_output_tokens)
            }
            None => super::overflow::ModelLimit::FALLBACK,
        }
    }

    /// Deserializes from `AISettings` (matches opencode `Config.compaction.*`).
    ///
    /// Field mapping:
    /// - `byop_compaction_auto` → `auto`
    /// - `byop_compaction_prune` → `prune`
    /// - `byop_compaction_tail_turns` → `tail_turns` (0 is kept as-is, meaning tail splitting is disabled)
    /// - `byop_compaction_preserve_recent_tokens` → `preserve_recent_tokens` (0 → None, uses the formula)
    /// - `byop_compaction_reserved` → `reserved` (0 → None, uses min(20_000, max_output))
    /// - `byop_compaction_model_provider_id` + `byop_compaction_model_id` → `compaction_model`
    ///   (either being empty → None, falls back to the conversation's current model)
    pub fn from_settings(app: &warpui::AppContext) -> Self {
        use crate::settings::AISettings;
        use warpui::SingletonEntity as _;
        let s = AISettings::as_ref(app);
        let provider_id = s.byop_compaction_model_provider_id.to_string();
        let model_id = s.byop_compaction_model_id.to_string();
        let compaction_model = if !provider_id.is_empty() && !model_id.is_empty() {
            Some(CompactionModelRef {
                provider_id,
                model_id,
            })
        } else {
            None
        };
        let preserve_raw: u32 = *s.byop_compaction_preserve_recent_tokens;
        let reserved_raw: u32 = *s.byop_compaction_reserved;
        Self {
            auto: *s.byop_compaction_auto,
            prune: *s.byop_compaction_prune,
            tail_turns: *s.byop_compaction_tail_turns as usize,
            preserve_recent_tokens: if preserve_raw == 0 {
                None
            } else {
                Some(preserve_raw as usize)
            },
            reserved: if reserved_raw == 0 {
                None
            } else {
                Some(reserved_raw as usize)
            },
            compaction_model,
        }
    }
}

/// Pure mapping half of [`CompactionConfig::model_limit`], split out to be testable without
/// an `AppContext`.
///
/// Two zero cases, both meaning "unspecified" in `AgentProviderModel`, and they need opposite
/// treatment:
///
/// - `context_window == 0` → [`ModelLimit::FALLBACK`]. A zero context short-circuits both
///   `usable` and `is_overflow` to 0/false (`overflow.rs:71`, `overflow.rs:89`), so auto
///   compaction would never fire at all. Compacting on the old conservative guess beats
///   never compacting and letting the provider reject the turn.
/// - `max_output_tokens == 0` → [`COMPACTION_BUFFER`], capped at a quarter of the window
///   ([`UNKNOWN_OUTPUT_RESERVE_DIVISOR`]). With no separate input limit, `usable` takes the
///   `context - max_output` branch, which ignores `cfg.reserved` entirely — so a literal 0
///   leaves no headroom and the trigger only fires once the prompt already fills the whole
///   window, too late to fit a response. 20k matches the cap opencode puts on `reserved`; the
///   quarter keeps a small window from being left with nothing (#1090: every window of 20k or
///   less had `usable == 0`).
/// - a known `max_output_tokens` → as configured, through [`clamp_max_output_tokens`], which
///   only touches a value over half the window (a misconfiguration).
///
/// `input` stays 0 because `AgentProviderModel` has no separate input limit to report; that
/// selects the `context - max_output` branch, which is the correct reading of a single
/// combined window.
pub fn model_limit_from_parts(context_window: u32, max_output_tokens: u32) -> ModelLimit {
    if context_window == 0 {
        return ModelLimit::FALLBACK;
    }
    let context = context_window as usize;
    ModelLimit {
        context,
        input: 0,
        max_output: if max_output_tokens == 0 {
            COMPACTION_BUFFER.min(context / UNKNOWN_OUTPUT_RESERVE_DIVISOR)
        } else {
            clamp_max_output_tokens(context_window, max_output_tokens) as usize
        },
    }
}

/// The share of the window reserved for the reply when the model's `max_output_tokens` is
/// unknown: a quarter (#1090). It keeps three quarters of a small window for the prompt and
/// still leaves room for a real reply: 1k tokens (~750 words) at 4k, 2k at 8k, 4k at 16k, 8k
/// at 32k. From an 80k window up the 20k default is the smaller, so large models keep
/// opencode's numbers. Nothing is sent as `max_tokens` in this case.
pub const UNKNOWN_OUTPUT_RESERVE_DIVISOR: usize = 4;

/// The `max_output_tokens` a model is held to, for the compaction reserve *and* the
/// `max_tokens` its requests send (`chat_stream::generate_byop_output`), so the two always
/// agree: the configured value, but never more than half the window (#1090).
///
/// The overflow threshold is `window - max_output`, and a request is accepted only while
/// `prompt + max_tokens <= window` (Anthropic and vLLM reject it otherwise). A configured value
/// up to half the window is used exactly, which covers every catalog entry seen (Claude
/// 200k/64k, o3 200k/100k, DeepSeek 128k/64k). A larger one — up to the window itself, which
/// a hand-written config or a server default can produce — would leave the prompt almost
/// nothing (every reply compacts, then gives up), so it is clamped to half: prompt up to
/// half the window plus a `max_tokens` of half still fits. `0` (unspecified) is returned as
/// is, as is any value when the window is unknown.
pub fn clamp_max_output_tokens(context_window: u32, max_output_tokens: u32) -> u32 {
    if context_window == 0 || max_output_tokens == 0 {
        return max_output_tokens;
    }
    max_output_tokens.min(context_window / 2)
}
