//! Test-only mock BYOP provider.
//!
//! Stands up a local HTTP endpoint (via [`mockito`]) that returns a canned,
//! *streamed* OpenAI-compatible chat-completion response, plus a helper that
//! points the app's provider config ([`AISettings`] + [`AgentProviderSecrets`])
//! at that local endpoint with a dummy key. Together they let a usage scenario
//! drive a full agent round-trip **without any cloud service or a real API key**.
//!
//! ## BYOP-faithful, no cloud
//!
//! The mock impersonates the *user's own* provider: a local
//! `http://127.0.0.1:<port>` endpoint, exactly like pointing the app at a local
//! Ollama / vLLM / llama.cpp server. There is no cloud, no `api::impl`, and no
//! `ServerApi` involvement anywhere in this module — the app's real BYOP send
//! path (`crate::ai::agent_providers::chat_stream`, built on `genai`) resolves
//! the endpoint straight out of `AISettings` and talks HTTP to `localhost`.
//!
//! ## Wire shape
//!
//! [`AgentProviderApiType::OpenAi`] routes through genai's OpenAI adapter, which
//! `POST`s to `${endpoint}chat/completions` where `endpoint` is the provider's
//! `base_url` normalized to end in `/v1/`. So the mock is mounted on
//! `POST /v1/chat/completions` and the config helper stores a `base_url` of
//! `http://127.0.0.1:<port>/v1`. The response body is Server-Sent Events, the
//! same `data: {chunk}\n\n … data: [DONE]\n\n` stream a real OpenAI-compatible
//! provider emits, which genai decodes into `ChatStreamEvent::Chunk` events.

use mockito::{Mock, Server, ServerGuard};
use serde_json::json;
use settings::Setting;
use warpui::{AppContext, SingletonEntity};

use crate::ai::agent_providers::{llm_id, AgentProviderSecrets};
use crate::settings::{AISettings, AgentProvider, AgentProviderApiType, AgentProviderModel};

/// Provider id the mock registers under (stable, so scenarios can reference it).
pub const MOCK_PROVIDER_ID: &str = "mock-byop-provider";
/// Model id exposed by the mock provider.
pub const MOCK_MODEL_ID: &str = "mock-model";
/// A non-empty dummy key. The mock never validates it; it exists only so the BYOP
/// send path attaches an `Authorization: Bearer …` header, matching a real keyed
/// provider. It is not a real secret.
pub const MOCK_API_KEY: &str = "mock-key-not-a-real-secret";
/// The canned assistant reply the mock streams back by default.
pub const DEFAULT_CANNED_REPLY: &str = "Hello from the mock BYOP provider.";

/// Splits `text` into at most `parts` non-empty pieces so the mock streams the
/// reply as several incremental deltas (exercising real streaming) rather than a
/// single blob. Splits on char boundaries to stay UTF-8 safe.
fn split_into_deltas(text: &str, parts: usize) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let chars: Vec<char> = text.chars().collect();
    let parts = parts.clamp(1, chars.len());
    let chunk_len = chars.len().div_ceil(parts);
    chars
        .chunks(chunk_len)
        .map(|c| c.iter().collect::<String>())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Wraps a `delta` object into one OpenAI `chat.completion.chunk` SSE frame.
fn sse_chunk(delta: serde_json::Value, finish_reason: Option<&str>) -> String {
    let payload = json!({
        "id": "chatcmpl-mock",
        "object": "chat.completion.chunk",
        "created": 0,
        "model": MOCK_MODEL_ID,
        "choices": [{
            "index": 0,
            "delta": delta,
            "finish_reason": finish_reason,
        }],
    });
    // `serde_json::to_string` never fails for a plain object built from `json!`.
    format!("data: {}\n\n", serde_json::to_string(&payload).unwrap())
}

/// Builds an OpenAI-compatible **streamed** chat-completion body: a role-announce
/// chunk, `reply` spread across a few content deltas, a terminal
/// `finish_reason: "stop"` chunk, and the `[DONE]` sentinel — the exact wire shape
/// genai's OpenAI adapter (the BYOP send path) decodes into
/// `ChatStreamEvent::Chunk` events.
pub fn canned_chat_completion_sse(reply: &str) -> String {
    let mut body = String::new();
    body.push_str(&sse_chunk(json!({ "role": "assistant" }), None));
    for delta in split_into_deltas(reply, 3) {
        body.push_str(&sse_chunk(json!({ "content": delta }), None));
    }
    body.push_str(&sse_chunk(json!({}), Some("stop")));
    body.push_str("data: [DONE]\n\n");
    body
}

/// A running local mock BYOP provider.
///
/// Holds the [`mockito`] server (and the mounted [`Mock`], which must stay alive
/// for the route to remain served). Dropping this stops the server.
pub struct MockProvider {
    server: ServerGuard,
    // Kept alive so the mounted route is not torn down. Never read directly.
    _mock: Mock,
    reply: String,
}

impl MockProvider {
    /// Starts a local mock provider serving [`DEFAULT_CANNED_REPLY`].
    pub async fn start() -> Self {
        Self::start_with_reply(DEFAULT_CANNED_REPLY).await
    }

    /// Starts a local mock provider that streams `reply` as its canned answer.
    pub async fn start_with_reply(reply: &str) -> Self {
        let mut server = Server::new_async().await;
        let body = canned_chat_completion_sse(reply);
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_header("content-type", "text/event-stream")
            .with_body(body)
            // A single turn hits this once; a tool round-trip would hit it again.
            // Allow any count so neither case trips a strict expectation.
            .expect_at_least(0)
            .create_async()
            .await;
        Self {
            server,
            _mock: mock,
            reply: reply.to_owned(),
        }
    }

    /// The `base_url` to store in the provider config: `…/v1` so the endpoint
    /// normalizer yields `…/v1/` and genai joins `chat/completions` onto it.
    pub fn base_url(&self) -> String {
        format!("{}/v1", self.server.url())
    }

    /// The canned reply this mock streams (useful for assertions).
    pub fn canned_reply(&self) -> &str {
        &self.reply
    }

    /// Points the app's provider config at this mock and returns the BYOP
    /// [`ai::LLMId`] a scenario can select. See [`wire_mock_provider_config`].
    pub fn wire_into_config(&self, ctx: &mut AppContext) -> ai::LLMId {
        wire_mock_provider_config(ctx, &self.base_url())
    }
}

/// Registers a single OpenAI-api_type [`AgentProvider`] whose `base_url` points at
/// `base_url` (the local mock), stores [`MOCK_API_KEY`] in
/// [`AgentProviderSecrets`], and returns the encoded BYOP [`ai::LLMId`] for its
/// one model. This is the whole "point the app at the mock" step — after it,
/// `crate::ai::agent_providers::lookup_byop` resolves the model and the real send
/// path streams from `localhost`. No cloud, no real key.
pub fn wire_mock_provider_config(ctx: &mut AppContext, base_url: &str) -> ai::LLMId {
    let provider = AgentProvider {
        id: MOCK_PROVIDER_ID.to_owned(),
        name: "Mock BYOP Provider".to_owned(),
        kind: Default::default(),
        api_type: AgentProviderApiType::OpenAi,
        base_url: base_url.to_owned(),
        models: vec![AgentProviderModel::from_id(MOCK_MODEL_ID.to_owned())],
        extra_headers: Vec::new(),
        vertex_project: String::new(),
        vertex_location: String::new(),
        disabled: false,
    };

    AISettings::handle(ctx).update(ctx, |settings, ctx| {
        let _ = settings.agent_providers.set_value(vec![provider], ctx);
    });
    AgentProviderSecrets::handle(ctx).update(ctx, |secrets, ctx| {
        secrets.set(MOCK_PROVIDER_ID, MOCK_API_KEY.to_owned(), ctx);
    });

    llm_id::encode(MOCK_PROVIDER_ID, MOCK_MODEL_ID)
}

#[cfg(test)]
mod tests {
    use super::*;

    use futures::StreamExt;
    use genai::adapter::AdapterKind;
    use genai::chat::{ChatMessage, ChatRequest, ChatStreamEvent};
    use genai::resolver::{AuthData, Endpoint, ServiceTargetResolver};
    use genai::{ModelIden, ServiceTarget};
    use warpui::App;

    use crate::ai::agent_providers::{lookup_byop, AgentProviderSecrets};
    use crate::settings::AISettings;
    use crate::test_util::settings::initialize_settings_for_tests;

    /// Builds a genai client wired the same way the BYOP send path builds one
    /// (`chat_stream::build_client_uncached`): a `ServiceTargetResolver` that pins
    /// the OpenAI adapter, the local endpoint, and a static key. This is the exact
    /// consumer the mock's canned response is shaped for, so a successful stream
    /// here proves the mock speaks the BYOP wire protocol.
    fn genai_client_for(base_url: &str, api_key: &str) -> genai::Client {
        let endpoint_url = format!("{}/", base_url.trim_end_matches('/'));
        let key = api_key.to_owned();
        let resolver = ServiceTargetResolver::from_resolver_fn(
            move |service_target: ServiceTarget| -> Result<ServiceTarget, genai::resolver::Error> {
                let ServiceTarget { model, .. } = service_target;
                Ok(ServiceTarget {
                    endpoint: Endpoint::from_owned(endpoint_url.clone()),
                    auth: AuthData::from_single(key.clone()),
                    model: ModelIden::new(AdapterKind::OpenAI, model.model_name),
                })
            },
        );
        genai::Client::builder()
            .with_service_target_resolver(resolver)
            .build()
    }

    /// The mock starts, serves the canned response, and its SSE stream is decoded
    /// by genai (the real BYOP consumer) back into the exact reply text.
    #[tokio::test]
    async fn mock_streams_canned_reply_parseable_by_genai() {
        let mock = MockProvider::start().await;
        let client = genai_client_for(&mock.base_url(), MOCK_API_KEY);

        let request = ChatRequest::from_messages(vec![ChatMessage::user("hi")]);
        let response = client
            .exec_chat_stream(MOCK_MODEL_ID, request, None)
            .await
            .expect("mock stream should open");

        let mut stream = response.stream;
        let mut streamed = String::new();
        while let Some(event) = stream.next().await {
            if let ChatStreamEvent::Chunk(chunk) = event.expect("stream event should be Ok") {
                streamed.push_str(&chunk.content);
            }
        }

        assert_eq!(
            streamed,
            mock.canned_reply(),
            "genai should reassemble the streamed deltas into the canned reply"
        );
    }

    /// The config helper points `AISettings` + `AgentProviderSecrets` at a local
    /// base URL with a dummy key, and the real BYOP lookup then resolves the
    /// model — the seam a full round-trip scenario relies on.
    #[test]
    fn helper_wires_provider_config() {
        App::test((), |mut app| async move {
            initialize_settings_for_tests(&mut app);
            app.add_singleton_model(AgentProviderSecrets::new);

            let base_url = "http://127.0.0.1:65535/v1";
            let llm_id = app.update(|ctx| wire_mock_provider_config(ctx, base_url));

            app.read(|ctx| {
                // Provider config was written with the local base URL.
                let providers = AISettings::as_ref(ctx).agent_providers.value().clone();
                assert_eq!(providers.len(), 1, "exactly one mock provider configured");
                assert_eq!(providers[0].id, MOCK_PROVIDER_ID);
                assert_eq!(providers[0].base_url, base_url);
                assert_eq!(providers[0].api_type, AgentProviderApiType::OpenAi);

                // The dummy key was stored under the provider id.
                let stored_key = AgentProviderSecrets::as_ref(ctx).get(MOCK_PROVIDER_ID);
                assert_eq!(stored_key, Some(MOCK_API_KEY));

                // The real send-path lookup resolves the model + key, so a full
                // round-trip could proceed against the local endpoint.
                let (provider, api_key, model_id) = lookup_byop(ctx, &llm_id)
                    .expect("BYOP lookup should resolve the mock provider");
                assert_eq!(provider.id, MOCK_PROVIDER_ID);
                assert_eq!(provider.base_url, base_url);
                assert_eq!(model_id, MOCK_MODEL_ID);
                assert_eq!(api_key, MOCK_API_KEY);
            });
        });
    }

    #[test]
    fn canned_sse_has_stream_shape() {
        let body = canned_chat_completion_sse("abcdef");
        assert!(body.contains("chat.completion.chunk"));
        assert!(body.contains("\"finish_reason\":\"stop\""));
        assert!(body.trim_end().ends_with("data: [DONE]"));
        // Role announce + at least one content delta + stop chunk + [DONE].
        assert!(body.matches("data: ").count() >= 4);
    }
}
