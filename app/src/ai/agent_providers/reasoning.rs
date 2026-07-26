//! Heuristic detection of model reasoning (chain-of-thought) capability.
//!
//! Background: genai 0.6's adapters do **not** gate models by capability internally ——
//! as long as `ChatOptions::reasoning_effort` is non-empty, the thinking parameter gets
//! injected regardless. For **models that don't support reasoning** (claude-3-5-haiku /
//! gpt-4o / gemini-1.5-pro) this causes the upstream API to return a 400 directly, so the
//! client side has to make this determination itself.
//!
//! The detection strategy follows opencode's `provider/transform.ts::variants()` approach
//! of "hardcoded table + substring matching": a BYOP user's model id is an arbitrary
//! string, so we can't rely on registry metadata and can only match naming conventions.
//!
//! References:
//! - genai 0.6 anthropic adapter's SUPPORT_EFFORT_MODELS / SUPPORT_ADAPTTIVE_THINK_MODELS
//! - opencode v5's anthropicAdaptiveEfforts / OPENAI_EFFORTS lists
//! - Each provider's official docs listing their thinking-mode models

use crate::settings::{AgentProviderApiType, ReasoningEffortSetting};
use std::collections::HashSet;
use std::sync::{OnceLock, RwLock};

/// Returns the list of reasoning effort tiers actually available for the given
/// (api_type, model_id).
///
/// Empty list → the picker is hidden entirely (reasoning unsupported, or the client
/// can't reliably inject it).
/// First item → the recommended default tier for this model (the initial value the
/// first time the picker appears).
/// Last item is always [`ReasoningEffortSetting::Off`], meaning "explicitly turn off
/// thinking" (for models that support effort tiers this sends the `none` tier; for
/// budget-style models the thinking field is omitted entirely).
///
/// Design follows opencode's `provider/transform.ts::variants()` —— each vendor's tiers
/// are hardcoded, not sourced from models.dev. models.dev only gives a boolean for
/// "does it support reasoning"; the actual tiers are built into the client.
pub fn model_reasoning_variants(
    api_type: AgentProviderApiType,
    model_id: &str,
) -> Vec<ReasoningEffortSetting> {
    use ReasoningEffortSetting as R;
    let id = strip_effort_suffix(&model_id.to_ascii_lowercase()).to_string();

    match api_type {
        AgentProviderApiType::Anthropic => {
            if is_opus_4_7_or_higher(&id) {
                // Opus 4.7+: adaptive thinking + xhigh + max (already supported by genai)
                return vec![R::High, R::Low, R::Medium, R::XHigh, R::Max, R::Off];
            }
            if id.contains("claude-opus-4-6") || id.contains("claude-sonnet-4-6") {
                // 4.6 line: adaptive thinking + max
                return vec![R::High, R::Low, R::Medium, R::Max, R::Off];
            }
            if is_anthropic_reasoning_model(&id) {
                // 4.5 / 3.7-sonnet etc. — legacy budget, no max
                return vec![R::High, R::Low, R::Medium, R::Off];
            }
            vec![]
        }
        AgentProviderApiType::OpenAi | AgentProviderApiType::OpenAiResp => {
            if id.contains("gpt-5") || id.contains("codex") {
                // GPT-5 / codex: both minimal and xhigh are available
                return vec![R::Medium, R::Minimal, R::Low, R::High, R::XHigh, R::Off];
            }
            if is_openai_reasoning_model(&id) {
                // o-series: only low/medium/high
                return vec![R::Medium, R::Low, R::High, R::Off];
            }
            vec![]
        }
        AgentProviderApiType::Gemini => {
            if is_gemini_reasoning_model(&id) {
                // genai 0.6 uniformly sends a thinkingBudget number; 2.5/3.x don't
                // distinguish tiers
                return vec![R::Medium, R::Low, R::High, R::Off];
            }
            vec![]
        }
        // DeepSeek thinking-mode models (deepseek-reasoner / v4 / thinking / r1).
        // Zap's local fork (`lib/rust-genai`) relaxed the injection condition in
        // adapter_shared.rs so the `reasoning_effort` top-level field gets sent per
        // DeepSeek's thinking_mode docs.
        //
        // Ollama backend model ids are arbitrary, so we conservatively leave this empty.
        AgentProviderApiType::DeepSeek => {
            if is_deepseek_thinking_model(&id) {
                // DeepSeek's official docs only expose two thinking depths, high / max
                // (low/medium/xhigh are accepted as aliases by the server deserializer
                // at best, so the picker doesn't expose redundant entries).
                // The Off tier turns thinking off: our local genai fork supports
                // ChatOptions::extra_body, and chat_stream sends
                // `extra_body = {"thinking": {"type": "disabled"}}` merged at the top
                // level for DeepSeek+Off.
                vec![R::High, R::Max, R::Off]
            } else {
                vec![]
            }
        }
        AgentProviderApiType::Ollama => vec![],
    }
}

/// The recommended default tier for this model (the initial value the first time the
/// picker appears); `None` means the model doesn't support reasoning.
pub fn default_reasoning_for(
    api_type: AgentProviderApiType,
    model_id: &str,
) -> Option<ReasoningEffortSetting> {
    model_reasoning_variants(api_type, model_id)
        .first()
        .copied()
}

/// Opus 4.7 and above (`claude-opus-4-7` / `claude-opus-5-0` ...).
/// Semantically matches the genai anthropic adapter's `is_opus_4_7_or_higher` regex.
fn is_opus_4_7_or_higher(model_name: &str) -> bool {
    static RE: OnceLock<Option<regex::Regex>> = OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"claude-opus-(\d+)-(\d+)").ok());
    let Some(re) = re.as_ref() else {
        return false;
    };
    let Some(caps) = re.captures(model_name) else {
        return false;
    };
    let major = caps.get(1).and_then(|m| m.as_str().parse::<u32>().ok());
    let minor = caps.get(2).and_then(|m| m.as_str().parse::<u32>().ok());
    matches!((major, minor), (Some(major), Some(minor)) if (major, minor) >= (4, 7))
}

/// Determines whether the given (api_type, model_name) combination supports reasoning
/// (chain-of-thought).
///
/// Only when this returns `true` do we inject `reasoning_effort` into genai; otherwise
/// we send a plain chat request as-is, to avoid injecting a thinking parameter into
/// older models (e.g. claude-3-5-haiku / gpt-4o) that upstream would reject.
///
/// Naming conventions per vendor's model id style (all matched lowercase, by substring):
/// - **Anthropic**: `claude-opus-4` / `claude-sonnet-4` / `claude-haiku-4` /
///   `claude-3-7-sonnet` (where extended thinking starts) and newer
/// - **OpenAI / OpenAIResp**: `o1` / `o3` / `o4` series, `gpt-5`, `codex`
/// - **Gemini**: `gemini-2.5*` / `gemini-3*` (thinking starts at 2.5, all of 3.x)
/// - **DeepSeek**: `deepseek-reasoner` / `deepseek-r1` / `deepseek-v4*` /
///   `deepseek-thinking` (official has two tiers: high / max go through the
///   `reasoning_effort` top-level field; Off goes through
///   `extra_body.thinking.type=disabled` to turn thinking off)
/// - **Ollama**: goes through the OpenAI-compatible path, backend model id is not
///   controllable, so we **conservatively return `false`**
///   (if a user really is running a thinking model, they can explicitly set the tier
///   in Settings; we can relax this later)
pub fn model_supports_reasoning(api_type: AgentProviderApiType, model_id: &str) -> bool {
    !model_reasoning_variants(api_type, model_id).is_empty()
}

fn strip_effort_suffix(id: &str) -> &str {
    if let Some((prefix, last)) = id.rsplit_once('-') {
        if matches!(
            last,
            "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max" | "zero"
        ) {
            return prefix;
        }
    }
    id
}

fn is_anthropic_reasoning_model(id: &str) -> bool {
    // claude-3-7-sonnet is where extended thinking starts (released 2025-02).
    if id.contains("claude-3-7-sonnet") {
        return true;
    }
    // claude-opus-4* / claude-sonnet-4* / claude-haiku-4* are all supported.
    // Also handles the `4.5` / `4-5` / `4_5` dot-style variants.
    let four_series = ["claude-opus-4", "claude-sonnet-4", "claude-haiku-4"];
    if four_series.iter().any(|prefix| id.contains(prefix)) {
        return true;
    }
    false
}

fn is_openai_reasoning_model(id: &str) -> bool {
    // o-series reasoning models (o1 / o1-mini / o1-pro / o3 / o3-mini / o4 / o4-mini).
    // Note: `o1-mini` is excluded in opencode's azure case, but OpenAI's official API
    // accepts reasoning_effort for it, so we keep it here to match upstream OpenAI
    // behavior.
    let o_series_prefixes = ["o1", "o3", "o4"];
    for prefix in o_series_prefixes {
        if id == prefix
            || id.starts_with(&format!("{prefix}-"))
            || id.starts_with(&format!("{prefix}_"))
        {
            return true;
        }
    }
    // GPT-5 series (all support reasoning) + codex variants (gpt-5-codex / codex-* /
    // o*-codex etc.).
    if id.contains("gpt-5") || id.contains("codex") {
        return true;
    }
    false
}

fn is_deepseek_thinking_model(id: &str) -> bool {
    // DeepSeek thinking-mode model naming convention: reasoner / r1 / v4* / *-thinking.
    // The `deepseek-v4` substring also covers later variants like `deepseek-v4-flash`.
    id.contains("deepseek-reasoner")
        || id.contains("deepseek-v4")
        || id.contains("deepseek-thinking")
        || id.contains("deepseek-r1")
}

fn is_gemini_reasoning_model(id: &str) -> bool {
    // Thinking mode starts at gemini-2.5-* (flash-thinking-exp / pro / pro-thinking).
    // All of gemini-3.* (opencode distinguishes 3 / 3.1 at the levels layer).
    if id.contains("gemini-2.5") || id.contains("gemini-3") {
        return true;
    }
    // Legacy thinking exp channel (2.0 flash-thinking-exp counts too).
    if id.contains("thinking") {
        return true;
    }
    false
}

/// Mirrors opencode's `model.capabilities.interleaved.field` (`provider/provider.ts:1182-1187`,
/// `provider/transform.ts:217-249`): some thinking-mode models require historical
/// reasoning to be attached back onto the assistant message under a specific field name.
///
/// opencode's two valid values are `"reasoning_content"` and `"reasoning_details"`:
/// - `reasoning_content`: the top-level string field used by most Chinese OpenAI-compatible
///   thinking models (DeepSeek/Kimi/MiMo/Qwen3/GLM-thinking/MiniMax/Hunyuan/Ernie/Doubao ...).
/// - `reasoning_details`: the array form used by aggregator providers like OpenRouter;
///   the genai 0.6 OpenAI adapter doesn't support this yet (it can only hoist the
///   top-level `reasoning_content` string) — kept as an enum placeholder, and falls back
///   to serializing as `ReasoningContent` when matched (which covers most compatible
///   endpoints).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReasoningInterleavedField {
    /// Top-level `reasoning_content` string field.
    ReasoningContent,
    /// Top-level `reasoning_details` array field (reserved; the current serialization
    /// path falls back).
    ReasoningDetails,
}

/// Substring match table of model_ids for Chinese / third-party OpenAI-compatible
/// thinking models.
///
/// Modeled after opencode's `models.dev` `capabilities.interleaved` data field —— each
/// thinking model explicitly declares its field in the catalog, and the client looks up
/// the model to decide the echo shape. warp has no external catalog, so the table is
/// hardcoded here; it can be made configurable/overridable later.
///
/// Rule: **a match is a lowercase model_id substring containing the needle**. Order
/// doesn't matter (short and long strings don't shadow each other; the first match
/// wins). Maintaining this only requires adding a row to the table, no control-flow
/// changes.
const INTERLEAVED_RULES: &[(&str, ReasoningInterleavedField)] = {
    use ReasoningInterleavedField::ReasoningContent as RC;
    &[
        // DeepSeek's whole thinking line (users often configure the official
        // OpenAI-compatible endpoint as the OpenAi api_type)
        ("deepseek-reasoner", RC),
        ("deepseek-v4", RC),
        ("deepseek-r1", RC),
        ("deepseek-thinking", RC),
        // Moonshot Kimi series
        ("kimi", RC),
        ("moonshot", RC),
        // Xiaomi MiMo (source of the reported issue: `mimo-v2.5-pro`)
        ("mimo", RC),
        // Alibaba Qwen thinking / QwQ (DashScope OpenAI-compatible endpoint +
        // enable_thinking)
        ("qwen3", RC),
        ("qwq", RC),
        // Zhipu GLM thinking (z.ai / Zhipu open platform)
        ("zai-glm", RC),
        ("glm-4.5-thinking", RC),
        ("glm-4.6-thinking", RC),
        ("glm-4.7", RC),
        // MiniMax M1 thinking (uses the reasoning_content field)
        ("minimax-m1", RC),
        // MiniMax M3: reasoning is delivered inline in content wrapped in <think> tags;
        // the multi-turn echo format (RC vs <think>-in-content) is still unconfirmed,
        // so no RC entry is added for it yet.
        // The display fix is handled by the model_uses_think_tags_in_content
        // whitelist plus streaming extraction.
        // Tencent Hunyuan T1 thinking
        ("hunyuan-t1", RC),
        // Baidu Ernie X1 / thinking
        ("ernie-x1", RC),
        ("ernie-thinking", RC),
        // StepFun Step thinking
        ("step-r-mini", RC),
        ("step-thinking", RC),
        // ByteDance Doubao thinking
        ("doubao-thinking", RC),
        ("doubao-1-5-thinking", RC),
        // 01.AI Yi thinking
        ("yi-thinking", RC),
    ]
};

/// Whitelist of OpenAI-compatible thinking models that deliver reasoning inline in
/// `/delta/content` wrapped in `<think>...</think>` tags (rather than as a separate
/// `/delta/reasoning_content` field).
///
/// For models matching this table, the chat_stream streaming layer extracts `<think>`
/// tags from Chunk events, routing the tagged content to the reasoning channel to be
/// displayed as a grayed-out thinking block.
/// Models that don't match keep their original text-output behavior, to avoid
/// accidentally swallowing normal output that happens to contain a literal `<think>`.
const THINK_TAG_IN_CONTENT_MODELS: &[&str] = &[
    // MiniMax M3: reasoning is delivered via <think> tags in content.
    "minimax-m3",
];

/// Returns whether the given model delivers reasoning via `<think>` tags in content
/// (rather than via the reasoning_content field).
///
/// The chat_stream streaming layer uses this function to decide whether to extract
/// `<think>` tags from Chunk events.
pub fn model_uses_think_tags_in_content(model_id: &str) -> bool {
    let id = model_id.to_ascii_lowercase();
    THINK_TAG_IN_CONTENT_MODELS
        .iter()
        .any(|&needle| id.contains(needle))
}

/// Runtime latch set: records which (api_type, model_id) pairs have sent a
/// `ReasoningChunk` during some stream —— i.e. a precise heuristic signal that "this
/// endpoint's server recognizes the reasoning_content field".
///
/// This is the key difference from opencode: opencode statically declares
/// `capabilities.interleaved` via an external `models.dev` catalog; warp has no
/// catalog, so it uses stream probing instead —— an endpoint that has ever sent a
/// reasoning chunk must recognize reasoning_content, and **strict providers like
/// Cerebras / Groq / OpenRouter / Together AI / SambaNova** that never send that chunk
/// will simply never get latched, automatically avoiding the kind of spurious 400 seen
/// in zerx-lab/warp #25.
///
/// The signal is only kept in memory across a stream/turn and is cleared on process
/// restart (it gets re-latched the next time a reasoning chunk is seen). It's only
/// meaningful for the OpenAi / OpenAiResp api_type —— the whole DeepSeek adapter echoes
/// by default; Anthropic / Gemini each go through thinking blocks / thought signatures
/// respectively, so even if a stream emits a reasoning chunk they don't need this
/// top-level `reasoning_content` field.
static REASONING_ECHO_LATCH: OnceLock<RwLock<HashSet<(AgentProviderApiType, String)>>> =
    OnceLock::new();

fn latch_set() -> &'static RwLock<HashSet<(AgentProviderApiType, String)>> {
    REASONING_ECHO_LATCH.get_or_init(|| RwLock::new(HashSet::new()))
}

/// Called when a `ReasoningChunk` is received in a stream; marks (api_type, lowercased
/// model_id) as "needs to echo back reasoning_content". On the next
/// [`model_reasoning_interleaved`] / [`model_requires_reasoning_echo`] query, this takes
/// priority and returns `Some(ReasoningContent)` / `true`, regardless of whether it's
/// present in the static [`INTERLEAVED_RULES`] table.
///
/// Only actually writes for the OpenAi / OpenAiResp api_type (other api_types already
/// have a native reasoning channel, so latching would have no benefit and would just
/// pollute the set); other paths return early.
pub fn note_reasoning_seen(api_type: AgentProviderApiType, model_id: &str) {
    if !matches!(
        api_type,
        AgentProviderApiType::OpenAi | AgentProviderApiType::OpenAiResp
    ) {
        return;
    }
    let key = (api_type, model_id.to_ascii_lowercase());
    if let Ok(s) = latch_set().read() {
        if s.contains(&key) {
            return;
        }
    }
    if let Ok(mut s) = latch_set().write() {
        s.insert(key);
    }
}

fn latch_contains(api_type: AgentProviderApiType, model_id_lower: &str) -> bool {
    latch_set()
        .read()
        .map(|s| s.contains(&(api_type, model_id_lower.to_string())))
        .unwrap_or(false)
}

/// For tests only: clears the latch. Production code should not call this.
#[cfg(test)]
fn reset_reasoning_latch() {
    if let Ok(mut s) = latch_set().write() {
        s.clear();
    }
}

/// Looks up which reasoning interleaved field the model should use; `None` means this
/// endpoint should not echo back `reasoning_content` —— even if the stream received real
/// reasoning, it's discarded on replay, to avoid a 400 `wrong_api_format` rejection from
/// **strict-schema providers like Cerebras / Groq / OpenRouter / Together AI / SambaNova /
/// official OpenAI**.
///
/// Mirrors the semantics of opencode's `provider/transform.ts:217-249`
/// `capabilities.interleaved`, enhanced into a two-stage decision (precision first,
/// then recall as a fallback):
///
/// 1. **Runtime latch** (precise): this (api_type, model_id) has sent a
///    `ReasoningChunk` in a past stream → this endpoint's server must recognize the
///    reasoning_content field → returns `Some(ReasoningContent)`. This covers any
///    Chinese / third-party thinking model outside the [`INTERLEAVED_RULES`] table,
///    with no whitelist maintenance needed.
/// 2. **Static hint** (cold start): if the latch doesn't match, falls back to the
///    [`INTERLEAVED_RULES`] substring table and api_type defaults:
///    - **DeepSeek api_type**: the whole adapter is DeepSeek-specific, so all models
///      echo (matches opencode's default of
///      `apiID.includes("deepseek") → { field: "reasoning_content" }`)
///    - **OpenAI / OpenAiResp**: uses the substring table, covering mainstream
///      Chinese thinking models
///    - **Anthropic / Gemini / Ollama**: `None` (Anthropic uses thinking blocks,
///      Gemini uses thought signatures, Ollama uses native reasoning; none of them
///      need this echo)
pub fn model_reasoning_interleaved(
    api_type: AgentProviderApiType,
    model_id: &str,
) -> Option<ReasoningInterleavedField> {
    use AgentProviderApiType as T;
    let id = model_id.to_ascii_lowercase();
    // (1) Runtime latch —— echo is locked in if the last stream sent a reasoning chunk
    if matches!(api_type, T::OpenAi | T::OpenAiResp) && latch_contains(api_type, &id) {
        return Some(ReasoningInterleavedField::ReasoningContent);
    }
    // (2) Static hint —— fallback for cold start / first turn (not streamed yet)
    match api_type {
        T::DeepSeek => Some(ReasoningInterleavedField::ReasoningContent),
        T::OpenAi | T::OpenAiResp => INTERLEAVED_RULES
            .iter()
            .find(|(needle, _)| id.contains(needle))
            .map(|(_, f)| *f),
        T::Anthropic | T::Gemini | T::Ollama => None,
    }
}

/// Determines whether the given (api_type, model_id) needs to echo back the
/// `reasoning_content` field on every assistant message (including as an empty-string
/// placeholder). Equivalent to [`model_reasoning_interleaved`] `.is_some()`; the old
/// name is kept for compatibility with existing call sites.
///
/// Background: newer-generation thinking-mode models like `deepseek-v4-flash` /
/// `mimo-v2.5-pro` tightened server-side validation from "an assistant message
/// containing only tool_calls must carry reasoning_content" to "in thinking mode,
/// every assistant message must carry reasoning_content, or it's a 400 error:
/// `The reasoning_content in the thinking mode must be passed back to the API`".
/// The genai 0.6 serialization layer (`adapter_shared.rs:368-373`) only echoes an
/// existing `ContentPart::ReasoningContent` and **does not auto-fill a missing one**,
/// so the client layer must forcibly attach a placeholder field (an empty string is
/// fine —— genai inserts it as-is, and the server only validates that the field is
/// present).
pub fn model_requires_reasoning_echo(api_type: AgentProviderApiType, model_id: &str) -> bool {
    model_reasoning_interleaved(api_type, model_id).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anthropic_supported() {
        let t = AgentProviderApiType::Anthropic;
        assert!(model_supports_reasoning(t, "claude-opus-4-5"));
        assert!(model_supports_reasoning(t, "claude-sonnet-4-6"));
        assert!(model_supports_reasoning(t, "claude-opus-4-7"));
        assert!(model_supports_reasoning(t, "claude-3-7-sonnet-20250219"));
        // suffix should not affect the determination
        assert!(model_supports_reasoning(t, "claude-sonnet-4-5-high"));
        assert!(model_supports_reasoning(t, "claude-opus-4-7-max"));
    }

    #[test]
    fn anthropic_unsupported() {
        let t = AgentProviderApiType::Anthropic;
        assert!(!model_supports_reasoning(t, "claude-3-5-haiku-20241022"));
        assert!(!model_supports_reasoning(t, "claude-3-5-sonnet-20241022"));
        assert!(!model_supports_reasoning(t, "claude-3-opus-20240229"));
        assert!(!model_supports_reasoning(t, "claude-2.1"));
    }

    #[test]
    fn openai_supported() {
        let t = AgentProviderApiType::OpenAi;
        assert!(model_supports_reasoning(t, "o1"));
        assert!(model_supports_reasoning(t, "o1-mini"));
        assert!(model_supports_reasoning(t, "o3-mini"));
        assert!(model_supports_reasoning(t, "o4-mini"));
        assert!(model_supports_reasoning(t, "gpt-5"));
        assert!(model_supports_reasoning(t, "gpt-5-codex"));
        assert!(model_supports_reasoning(t, "gpt-5-codex-high"));
    }

    #[test]
    fn openai_unsupported() {
        let t = AgentProviderApiType::OpenAi;
        assert!(!model_supports_reasoning(t, "gpt-4o"));
        assert!(!model_supports_reasoning(t, "gpt-4-turbo"));
        assert!(!model_supports_reasoning(t, "gpt-3.5-turbo"));
    }

    #[test]
    fn gemini_supported() {
        let t = AgentProviderApiType::Gemini;
        assert!(model_supports_reasoning(t, "gemini-2.5-pro"));
        assert!(model_supports_reasoning(t, "gemini-2.5-flash"));
        assert!(model_supports_reasoning(t, "gemini-3-pro"));
        assert!(model_supports_reasoning(t, "gemini-2.0-flash-thinking-exp"));
    }

    #[test]
    fn gemini_unsupported() {
        let t = AgentProviderApiType::Gemini;
        assert!(!model_supports_reasoning(t, "gemini-1.5-pro"));
        assert!(!model_supports_reasoning(t, "gemini-1.5-flash"));
        assert!(!model_supports_reasoning(t, "gemini-2.0-flash"));
    }

    #[test]
    fn deepseek_thinking_models_supported() {
        let t = AgentProviderApiType::DeepSeek;
        assert!(model_supports_reasoning(t, "deepseek-reasoner"));
        assert!(model_supports_reasoning(t, "deepseek-v4"));
        assert!(model_supports_reasoning(t, "deepseek-v4-flash"));
        assert!(model_supports_reasoning(t, "deepseek-thinking"));
        assert!(model_supports_reasoning(t, "deepseek-r1"));
        // plain chat models don't have thinking
        assert!(!model_supports_reasoning(t, "deepseek-chat"));
        assert!(!model_supports_reasoning(t, "deepseek-coder"));
    }

    #[test]
    fn ollama_always_false() {
        assert!(!model_supports_reasoning(
            AgentProviderApiType::Ollama,
            "qwq-32b"
        ));
    }

    #[test]
    fn requires_reasoning_echo_deepseek() {
        // DeepSeek api_type always echoes, regardless of model
        assert!(model_requires_reasoning_echo(
            AgentProviderApiType::DeepSeek,
            "deepseek-v4-flash"
        ));
        assert!(model_requires_reasoning_echo(
            AgentProviderApiType::DeepSeek,
            "deepseek-chat"
        ));
        assert!(model_requires_reasoning_echo(
            AgentProviderApiType::DeepSeek,
            "deepseek-reasoner"
        ));
    }

    #[test]
    fn requires_reasoning_echo_kimi_via_openai() {
        let t = AgentProviderApiType::OpenAi;
        assert!(model_requires_reasoning_echo(t, "kimi-k2-thinking"));
        assert!(model_requires_reasoning_echo(t, "moonshot-v1-32k"));
        assert!(model_requires_reasoning_echo(
            AgentProviderApiType::OpenAiResp,
            "Kimi-Latest"
        ));
        // plain OpenAI models don't echo
        assert!(!model_requires_reasoning_echo(t, "gpt-5"));
        assert!(!model_requires_reasoning_echo(t, "o3-mini"));
    }

    #[test]
    fn requires_reasoning_echo_deepseek_via_openai() {
        // DeepSeek's official endpoint is OpenAI-compatible; users often configure it
        // as an OpenAI api_type BYOP provider. Thinking models must echo back
        // `reasoning_content`, or it's a 400.
        let t = AgentProviderApiType::OpenAi;
        assert!(model_requires_reasoning_echo(t, "deepseek-v4-flash"));
        assert!(model_requires_reasoning_echo(t, "deepseek-v4"));
        assert!(model_requires_reasoning_echo(t, "deepseek-reasoner"));
        assert!(model_requires_reasoning_echo(t, "deepseek-r1"));
        assert!(model_requires_reasoning_echo(t, "deepseek-thinking"));
        // case-insensitive
        assert!(model_requires_reasoning_echo(t, "DeepSeek-V4-Flash"));
        // OpenAiResp shares the same behavior
        assert!(model_requires_reasoning_echo(
            AgentProviderApiType::OpenAiResp,
            "deepseek-r1"
        ));
        // Non-thinking DeepSeek models (deepseek-chat / deepseek-coder) don't go
        // through thinking-mode validation over the OpenAI-compatible path, so no
        // echo is needed
        assert!(!model_requires_reasoning_echo(t, "deepseek-chat"));
        assert!(!model_requires_reasoning_echo(t, "deepseek-coder"));
    }

    #[test]
    fn opus_4_7_variants_have_xhigh_and_max() {
        let v =
            model_reasoning_variants(AgentProviderApiType::Anthropic, "claude-opus-4-7-20260101");
        assert!(v.contains(&ReasoningEffortSetting::XHigh));
        assert!(v.contains(&ReasoningEffortSetting::Max));
        assert_eq!(v.first().copied(), Some(ReasoningEffortSetting::High));
        assert_eq!(v.last().copied(), Some(ReasoningEffortSetting::Off));
    }

    #[test]
    fn opus_5_0_variants_treated_as_4_7_plus() {
        let v = model_reasoning_variants(AgentProviderApiType::Anthropic, "claude-opus-5-0");
        assert!(v.contains(&ReasoningEffortSetting::XHigh));
        assert!(v.contains(&ReasoningEffortSetting::Max));
    }

    #[test]
    fn sonnet_4_6_variants_have_max_no_xhigh() {
        let v = model_reasoning_variants(AgentProviderApiType::Anthropic, "claude-sonnet-4-6");
        assert!(v.contains(&ReasoningEffortSetting::Max));
        assert!(!v.contains(&ReasoningEffortSetting::XHigh));
    }

    #[test]
    fn sonnet_4_5_variants_legacy_no_max_no_xhigh() {
        let v = model_reasoning_variants(AgentProviderApiType::Anthropic, "claude-sonnet-4-5");
        assert!(!v.contains(&ReasoningEffortSetting::Max));
        assert!(!v.contains(&ReasoningEffortSetting::XHigh));
        assert!(v.contains(&ReasoningEffortSetting::High));
    }

    #[test]
    fn claude_3_5_haiku_variants_empty() {
        let v =
            model_reasoning_variants(AgentProviderApiType::Anthropic, "claude-3-5-haiku-20241022");
        assert!(v.is_empty());
    }

    #[test]
    fn gpt_5_variants_have_minimal_and_xhigh() {
        let v = model_reasoning_variants(AgentProviderApiType::OpenAi, "gpt-5");
        assert!(v.contains(&ReasoningEffortSetting::Minimal));
        assert!(v.contains(&ReasoningEffortSetting::XHigh));
        assert_eq!(v.first().copied(), Some(ReasoningEffortSetting::Medium));
    }

    #[test]
    fn o3_variants_no_minimal_no_xhigh() {
        let v = model_reasoning_variants(AgentProviderApiType::OpenAi, "o3-mini");
        assert!(!v.contains(&ReasoningEffortSetting::Minimal));
        assert!(!v.contains(&ReasoningEffortSetting::XHigh));
        assert!(v.contains(&ReasoningEffortSetting::High));
    }

    #[test]
    fn gpt_4o_variants_empty() {
        let v = model_reasoning_variants(AgentProviderApiType::OpenAi, "gpt-4o");
        assert!(v.is_empty());
    }

    #[test]
    fn gemini_2_5_variants_three_levels() {
        let v = model_reasoning_variants(AgentProviderApiType::Gemini, "gemini-2.5-pro");
        assert_eq!(v.len(), 4); // Medium, Low, High, Off
        assert!(v.contains(&ReasoningEffortSetting::Off));
    }

    #[test]
    fn gemini_1_5_variants_empty() {
        let v = model_reasoning_variants(AgentProviderApiType::Gemini, "gemini-1.5-pro");
        assert!(v.is_empty());
    }

    #[test]
    fn deepseek_thinking_variants_two_levels_plus_off() {
        let v = model_reasoning_variants(AgentProviderApiType::DeepSeek, "deepseek-reasoner");
        // DeepSeek official: only high / max plus Off
        assert_eq!(v.len(), 3);
        assert_eq!(v[0], ReasoningEffortSetting::High);
        assert_eq!(v[1], ReasoningEffortSetting::Max);
        assert_eq!(v[2], ReasoningEffortSetting::Off);
        // should not expose redundant aliases
        assert!(!v.contains(&ReasoningEffortSetting::Medium));
        assert!(!v.contains(&ReasoningEffortSetting::Low));
        assert!(!v.contains(&ReasoningEffortSetting::XHigh));
    }

    #[test]
    fn deepseek_chat_variants_empty() {
        assert!(
            model_reasoning_variants(AgentProviderApiType::DeepSeek, "deepseek-chat").is_empty()
        );
    }

    #[test]
    fn ollama_variants_empty() {
        assert!(model_reasoning_variants(AgentProviderApiType::Ollama, "qwq-32b").is_empty());
    }

    #[test]
    fn default_reasoning_for_consistency() {
        // default should equal the first item in the variants list
        assert_eq!(
            default_reasoning_for(AgentProviderApiType::Anthropic, "claude-opus-4-7"),
            Some(ReasoningEffortSetting::High)
        );
        assert_eq!(
            default_reasoning_for(AgentProviderApiType::OpenAi, "gpt-5"),
            Some(ReasoningEffortSetting::Medium)
        );
        assert_eq!(
            default_reasoning_for(AgentProviderApiType::OpenAi, "gpt-4o"),
            None
        );
    }

    #[test]
    fn supports_reasoning_consistent_with_variants() {
        // single source of truth: supports == !variants.is_empty()
        for (t, m) in [
            (AgentProviderApiType::Anthropic, "claude-opus-4-7"),
            (AgentProviderApiType::Anthropic, "claude-3-5-haiku"),
            (AgentProviderApiType::OpenAi, "gpt-5"),
            (AgentProviderApiType::OpenAi, "gpt-4o"),
            (AgentProviderApiType::Gemini, "gemini-2.5-pro"),
            (AgentProviderApiType::Gemini, "gemini-1.5-pro"),
            (AgentProviderApiType::DeepSeek, "deepseek-reasoner"),
        ] {
            assert_eq!(
                model_supports_reasoning(t, m),
                !model_reasoning_variants(t, m).is_empty(),
                "{t:?}/{m}"
            );
        }
    }

    #[test]
    fn requires_reasoning_echo_domestic_thinking_models() {
        // Chinese OpenAI-compatible thinking models must echo `reasoning_content`,
        // or the server returns 400 `The reasoning_content in the thinking mode must
        // be passed back`.
        // Test hits this under the OpenAi api_type (the most common BYOP configuration).
        let t = AgentProviderApiType::OpenAi;
        // Xiaomi MiMo (the model that triggered this issue)
        assert!(model_requires_reasoning_echo(t, "mimo-v2.5-pro"));
        assert!(model_requires_reasoning_echo(t, "mimo-vl-7b"));
        // Alibaba Qwen3 thinking / QwQ
        assert!(model_requires_reasoning_echo(
            t,
            "qwen3-235b-a22b-thinking-2507"
        ));
        assert!(model_requires_reasoning_echo(t, "qwq-32b-preview"));
        // Zhipu GLM thinking
        assert!(model_requires_reasoning_echo(t, "zai-glm-4.7"));
        assert!(model_requires_reasoning_echo(t, "glm-4.6-thinking"));
        assert!(model_requires_reasoning_echo(t, "glm-4.5-thinking"));
        // MiniMax / Hunyuan / Ernie / Step / Doubao / Yi
        assert!(model_requires_reasoning_echo(t, "minimax-m1-80k"));
        assert!(model_requires_reasoning_echo(t, "hunyuan-t1-latest"));
        assert!(model_requires_reasoning_echo(t, "ernie-x1-turbo-32k"));
        assert!(model_requires_reasoning_echo(t, "step-r-mini"));
        assert!(model_requires_reasoning_echo(t, "doubao-1-5-thinking-pro"));
        assert!(model_requires_reasoning_echo(t, "yi-thinking-v1"));
        // OpenAiResp shares the same behavior
        let r = AgentProviderApiType::OpenAiResp;
        assert!(model_requires_reasoning_echo(r, "MiMo-V2.5-Pro"));
        assert!(model_requires_reasoning_echo(r, "Qwen3-Coder-Thinking"));
    }

    #[test]
    fn reasoning_interleaved_field_for_domestic_models() {
        // model_reasoning_interleaved must return ReasoningContent (currently every
        // INTERLEAVED_RULES entry is ReasoningContent; ReasoningDetails is a reserved
        // enum placeholder).
        let t = AgentProviderApiType::OpenAi;
        assert_eq!(
            model_reasoning_interleaved(t, "mimo-v2.5-pro"),
            Some(ReasoningInterleavedField::ReasoningContent)
        );
        assert_eq!(
            model_reasoning_interleaved(t, "deepseek-v4-flash"),
            Some(ReasoningInterleavedField::ReasoningContent)
        );
        // DeepSeek api_type returns ReasoningContent for all models (including
        // non-thinking chat / coder) — the adapter is DeepSeek-specific, matching
        // opencode's default of `apiID.includes("deepseek") →
        // { field: "reasoning_content" }`.
        let d = AgentProviderApiType::DeepSeek;
        assert_eq!(
            model_reasoning_interleaved(d, "deepseek-chat"),
            Some(ReasoningInterleavedField::ReasoningContent)
        );
        // undeclared models / non-OpenAI-family → None
        assert_eq!(model_reasoning_interleaved(t, "gpt-5"), None);
        assert_eq!(model_reasoning_interleaved(t, "gpt-4o"), None);
        assert_eq!(
            model_reasoning_interleaved(AgentProviderApiType::Anthropic, "claude-opus-4-7"),
            None
        );
        assert_eq!(
            model_reasoning_interleaved(AgentProviderApiType::Gemini, "gemini-2.5-pro"),
            None
        );
        assert_eq!(
            model_reasoning_interleaved(AgentProviderApiType::Ollama, "qwq-32b"),
            None
        );
    }

    #[test]
    fn requires_reasoning_echo_strict_providers_excluded() {
        // Official OpenAI / Anthropic / Gemini / plain OpenAI models → don't attach
        // reasoning_content, to avoid a 400 `wrong_api_format` from strict OpenAI
        // providers like Cerebras / Groq / OpenRouter (zerx-lab/warp #25).
        let t = AgentProviderApiType::OpenAi;
        assert!(!model_requires_reasoning_echo(t, "gpt-5"));
        assert!(!model_requires_reasoning_echo(t, "gpt-4o"));
        assert!(!model_requires_reasoning_echo(t, "o3-mini"));
        // arbitrary BYOP models whose names contain no known thinking substring and
        // aren't the DeepSeek api_type
        assert!(!model_requires_reasoning_echo(t, "llama-3.3-70b-instruct"));
        assert!(!model_requires_reasoning_echo(t, "mistral-large-2407"));
    }

    #[test]
    fn runtime_latch_overrides_static_table() {
        // Any Chinese/third-party thinking model not in INTERLEAVED_RULES should
        // start auto-echoing from the next turn onward once the stream has sent a
        // reasoning chunk.
        // Uses a deliberately "nonexistent" model id to verify the latch actually
        // works.
        let t = AgentProviderApiType::OpenAi;
        let exotic = "totally-new-thinking-model-2099";
        reset_reasoning_latch();
        assert!(
            !model_requires_reasoning_echo(t, exotic),
            "a model outside the whitelist should not echo before being latched"
        );
        note_reasoning_seen(t, exotic);
        assert!(
            model_requires_reasoning_echo(t, exotic),
            "must echo after being latched"
        );
        assert_eq!(
            model_reasoning_interleaved(t, exotic),
            Some(ReasoningInterleavedField::ReasoningContent)
        );
        // case-insensitive
        assert!(model_requires_reasoning_echo(
            t,
            "Totally-New-Thinking-Model-2099"
        ));
        // OpenAiResp and OpenAi are independent keys —— but the same endpoint class
        // should each latch on its own
        let r = AgentProviderApiType::OpenAiResp;
        assert!(
            !model_requires_reasoning_echo(r, exotic),
            "the other api_type should not be affected"
        );
        note_reasoning_seen(r, exotic);
        assert!(model_requires_reasoning_echo(r, exotic));
        reset_reasoning_latch();
    }

    #[test]
    fn runtime_latch_never_writes_for_strict_api_types() {
        // Anthropic / Gemini / Ollama each go through their native reasoning
        // channel, so even if someone mistakenly calls note_reasoning_seen it must
        // not pollute the latch (otherwise sharing a model_id across api_types could
        // cause a false hit on the OpenAi path —— we use an (api_type, id) composite
        // key which already isolates this, but as extra insurance semantically these
        // api_types should never enter the latch).
        reset_reasoning_latch();
        for at in [
            AgentProviderApiType::Anthropic,
            AgentProviderApiType::Gemini,
            AgentProviderApiType::Ollama,
            AgentProviderApiType::DeepSeek,
        ] {
            note_reasoning_seen(at, "some-model");
        }
        // no OpenAi/OpenAiResp query should be hit by this noise
        assert!(!model_requires_reasoning_echo(
            AgentProviderApiType::OpenAi,
            "some-model"
        ));
        assert!(!model_requires_reasoning_echo(
            AgentProviderApiType::OpenAiResp,
            "some-model"
        ));
        reset_reasoning_latch();
    }

    #[test]
    fn requires_reasoning_echo_others_false() {
        assert!(!model_requires_reasoning_echo(
            AgentProviderApiType::Anthropic,
            "claude-opus-4-7"
        ));
        assert!(!model_requires_reasoning_echo(
            AgentProviderApiType::Gemini,
            "gemini-2.5-pro"
        ));
        assert!(!model_requires_reasoning_echo(
            AgentProviderApiType::Ollama,
            "qwq-32b"
        ));
    }

    #[test]
    fn think_tag_in_content_models() {
        // MiniMax M3 matches
        assert!(model_uses_think_tags_in_content("minimax-m3"));
        assert!(model_uses_think_tags_in_content("MiniMax-M3-80k"));
        assert!(model_uses_think_tags_in_content("MINIMAX-M3"));
        // MiniMax M1 does not match (uses the reasoning_content field)
        assert!(!model_uses_think_tags_in_content("minimax-m1"));
        // other thinking models do not match (each uses the reasoning_content field)
        assert!(!model_uses_think_tags_in_content("deepseek-r1"));
        assert!(!model_uses_think_tags_in_content("gpt-5"));
        assert!(!model_uses_think_tags_in_content("qwen3-235b"));
        assert!(!model_uses_think_tags_in_content("kimi-k2-thinking"));
        // plain non-thinking models do not match
        assert!(!model_uses_think_tags_in_content("gpt-4o"));
        assert!(!model_uses_think_tags_in_content("claude-opus-4-7"));
    }
}
