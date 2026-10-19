//! Whether a BYOP model can be trusted with upstream Warp's "running command snapshot"
//! contract (#855).
//!
//! Upstream returns a `LongRunningCommandSnapshot` of the screen when an agent command
//! has not finished after a bounded wait, and expects the model to react (send `q` to a
//! pager, answer a prompt) with `write_to_long_running_shell_command`. Small models cannot
//! reliably follow that: they keep issuing new tool calls and wedge the pty. For those the
//! fork waits for completion instead. This module holds the pure, table-tested default for
//! "which side is this model on"; the per-model `handles_command_snapshots` setting
//! overrides it.

/// Model-id fragments that mark a small / fast tier even inside a large family
/// (`gpt-5-mini`, `claude-haiku`, `gemini-2.5-flash`).
const SMALL_TIER_MARKERS: &[&str] = &["mini", "nano", "flash", "haiku", "lite", "tiny"];

/// The model-id part of `id` after any `provider/` routing prefix (OpenRouter, Bedrock,
/// Vertex), lowercased.
fn bare_model_id(id: &str) -> String {
    id.trim()
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// `o1`, `o3`, `o3-pro`, `o4` ...: OpenAI's o-series reasoning models.
fn is_openai_o_series(id: &str) -> bool {
    let Some(rest) = id.strip_prefix('o') else {
        return false;
    };
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return false;
    }
    let after = &rest[digits.len()..];
    after.is_empty() || after.starts_with('-')
}

/// Whether `model_id` names a large hosted model that handles snapshots well: Claude
/// Opus / Sonnet, GPT-4 / 4.1 / 4o / 5 class, the o-series, Gemini Pro, and Grok that is not
/// `mini`. Everything else -- including "flash" / "mini" / "nano" / "haiku" tiers and models
/// this does not recognise -- is false.
pub fn model_id_handles_command_snapshots(model_id: &str) -> bool {
    let id = bare_model_id(model_id);
    // "gemini" itself contains "mini"; blank it out before looking for the small tier.
    let tier_probe = id.replace("gemini", "gem");
    if id.is_empty()
        || SMALL_TIER_MARKERS
            .iter()
            .any(|marker| tier_probe.contains(marker))
    {
        return false;
    }
    (id.contains("claude") && (id.contains("opus") || id.contains("sonnet")))
        || id.starts_with("gpt-4")
        || id.starts_with("gpt-5")
        || is_openai_o_series(&id)
        || (id.contains("gemini") && id.contains("pro"))
        || id.contains("grok")
}

/// The host part of `base_url`, without scheme, credentials, port or path.
fn host_of(base_url: &str) -> &str {
    let rest = base_url
        .trim()
        .split_once("://")
        .map_or(base_url.trim(), |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let authority = authority.rsplit('@').next().unwrap_or_default();
    if let Some(bracketed) = authority.strip_prefix('[') {
        return bracketed.split(']').next().unwrap_or_default();
    }
    authority.split(':').next().unwrap_or_default()
}

/// Whether `base_url` points at the user's own machine or LAN (local Ollama, llama.cpp,
/// vLLM...). Those serve small local models whatever the id says (`llama3:70b`, or a local
/// server that reports a `claude-sonnet` alias), so the model-id heuristic is not applied.
pub fn is_local_endpoint(base_url: &str) -> bool {
    let host = host_of(base_url).to_ascii_lowercase();
    if host.is_empty() {
        return false;
    }
    if host == "localhost"
        || host == "::1"
        || host == "0.0.0.0"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.ends_with(".lan")
        || host.ends_with(".internal")
    {
        return true;
    }
    let octets: Vec<u8> = host.split('.').filter_map(|p| p.parse().ok()).collect();
    if octets.len() == 4 && host.split('.').count() == 4 {
        return match octets[..] {
            [127, ..] | [10, ..] | [192, 168, ..] => true,
            [172, second, ..] => (16..=31).contains(&second),
            _ => false,
        };
    }
    false
}

/// The default when the user has not set `handles_command_snapshots`.
pub fn default_handles_command_snapshots(model_id: &str, base_url: &str, is_ollama: bool) -> bool {
    !is_ollama && !is_local_endpoint(base_url) && model_id_handles_command_snapshots(model_id)
}

#[cfg(test)]
#[path = "command_snapshots_tests.rs"]
mod tests;
