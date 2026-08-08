// Ported from the pinned oracle's `crates/ai/src/llm_provider_tests.rs`
// (`02b53fcd8`, release `2026.07.29.09.05` stable — see `ORACLE.md`). Both
// tests are pure self-consistency checks on the hand-written
// `API_KEY_PROVIDER_VALUE_NAME` / `supports_pasted_api_key` against the
// canonical `API_KEY_PROVIDERS` list, so they port unchanged. Issues #392 /
// #225.
use super::LLMProvider;

#[test]
fn api_key_provider_help_matches_the_canonical_provider_list() {
    assert_eq!(
        LLMProvider::API_KEY_PROVIDERS
            .into_iter()
            .map(|provider| provider.api_key_slug().unwrap())
            .collect::<Vec<_>>()
            .join("|"),
        LLMProvider::API_KEY_PROVIDER_VALUE_NAME
    );
}

#[test]
fn only_pasted_key_providers_report_pasted_key_support() {
    for provider in LLMProvider::API_KEY_PROVIDERS {
        assert_eq!(
            provider.supports_pasted_api_key(),
            matches!(
                provider,
                LLMProvider::OpenAI | LLMProvider::Anthropic | LLMProvider::Google
            )
        );
    }
}
