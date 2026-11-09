//! The config wiring behind the mock BYOP provider, in its own module so the unit tests of the
//! agent fuzzer (`ai::blocklist::agent_fuzz`) can use it without the `integration_tests`
//! feature that gates `crate::integration_testing`. Re-exported from
//! `integration_testing::mock_provider`.

use settings::Setting;
use warpui::{AppContext, SingletonEntity};

use super::{AgentProviderSecrets, llm_id};
use crate::settings::{AISettings, AgentProvider, AgentProviderApiType, AgentProviderModel};

/// Provider id the mock registers under (stable, so scenarios can reference it).
pub const MOCK_PROVIDER_ID: &str = "mock-byop-provider";
/// A non-empty dummy key. The mock never validates it; it exists only so the BYOP
/// send path attaches an `Authorization: Bearer ...` header, matching a real keyed
/// provider. It is not a real secret.
pub const MOCK_API_KEY: &str = "mock-key-not-a-real-secret";

/// `wire_mock_provider_config` for a provider whose one model is `model_id`. The agent fuzzer
/// gives each case its own model id, because its stream seam (`fake_stream_seam`) is keyed by it.
pub fn wire_mock_provider_config_with_model(
    ctx: &mut AppContext,
    base_url: &str,
    model_id: &str,
) -> ai::LLMId {
    let provider = AgentProvider {
        id: MOCK_PROVIDER_ID.to_owned(),
        name: "Mock BYOP Provider".to_owned(),
        kind: Default::default(),
        api_type: AgentProviderApiType::OpenAi,
        base_url: base_url.to_owned(),
        models: vec![AgentProviderModel::from_id(model_id.to_owned())],
        extra_headers: Vec::new(),
        vertex_project: String::new(),
        vertex_location: String::new(),
        disabled: false,
        token_price: None,
    };

    AISettings::handle(ctx).update(ctx, |settings, ctx| {
        let _ = settings.agent_providers.set_value(vec![provider], ctx);
    });
    AgentProviderSecrets::handle(ctx).update(ctx, |secrets, ctx| {
        secrets.set(MOCK_PROVIDER_ID, MOCK_API_KEY.to_owned(), ctx);
    });

    llm_id::encode(MOCK_PROVIDER_ID, model_id)
}
