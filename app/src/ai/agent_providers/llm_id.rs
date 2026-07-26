//! `LLMId` prefix encoding/decoding for BYOP (Bring Your Own Provider).
//!
//! Custom Agent provider models are distinguished in the `LLMId` string by the
//! `byop:` prefix, so the controller can decide at the request egress point whether
//! to route to the warp backend or the user's own OpenAI-compatible endpoint.
//!
//! Encoding format: `byop:<provider_id>:<model_id>`
//! - `provider_id` is `AgentProvider.id` (a UUID)
//! - `model_id` is `AgentProviderModel.id` (the value sent as the `model` field to
//!   the upstream API)
//!
//! Example: `byop:6f3b...:deepseek-chat`
//!
//! `provider_id` is a UUID and contains no colons, while `model_id` may contain
//! colons (some upstreams use `vendor:model`-style naming), so the split only
//! happens at the first colon.

use ai::LLMId;

pub const BYOP_PREFIX: &str = "byop:";

/// Encodes `(provider_id, model_id)` into a single `LLMId`.
pub fn encode(provider_id: &str, model_id: &str) -> LLMId {
    LLMId::from(format!("{BYOP_PREFIX}{provider_id}:{model_id}"))
}

/// Returns `(provider_id, model_id)` if `LLMId` is a BYOP encoding, otherwise `None`.
pub fn decode(id: &LLMId) -> Option<(String, String)> {
    let s = id.as_str().strip_prefix(BYOP_PREFIX)?;
    let (pid, mid) = s.split_once(':')?;
    if pid.is_empty() || mid.is_empty() {
        return None;
    }
    Some((pid.to_owned(), mid.to_owned()))
}

/// Whether this `LLMId` is a BYOP encoding (for callers to check quickly without
/// needing to split the fields).
pub fn is_byop(id: &LLMId) -> bool {
    id.as_str().starts_with(BYOP_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let id = encode("uuid-123", "deepseek-chat");
        assert_eq!(id.as_str(), "byop:uuid-123:deepseek-chat");
        assert_eq!(
            decode(&id),
            Some(("uuid-123".to_owned(), "deepseek-chat".to_owned()))
        );
    }

    #[test]
    fn model_id_with_colon_is_preserved() {
        // e.g. OpenRouter's "anthropic/claude-3-haiku" contains no colon, but some
        // gateways may use "vendor:model:variant". We only split at the first colon,
        // and treat the rest as the model_id as a whole.
        let id = encode("uuid-1", "vendor:model:v2");
        assert_eq!(
            decode(&id),
            Some(("uuid-1".to_owned(), "vendor:model:v2".to_owned()))
        );
    }

    #[test]
    fn non_byop_returns_none() {
        let id = LLMId::from("gpt-5.2");
        assert_eq!(decode(&id), None);
        assert!(!is_byop(&id));
    }

    #[test]
    fn missing_parts_returns_none() {
        assert_eq!(decode(&LLMId::from("byop:")), None);
        assert_eq!(decode(&LLMId::from("byop:uuid")), None); // no colon
        assert_eq!(decode(&LLMId::from("byop::model")), None); // empty provider_id
        assert_eq!(decode(&LLMId::from("byop:uuid:")), None); // empty model_id
    }
}
