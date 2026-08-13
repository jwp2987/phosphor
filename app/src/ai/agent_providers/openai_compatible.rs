//! Minimal subset of an OpenAI-compatible client: currently only used to fetch
//! the `/models` list.
//!
//! When multi-agent calls land in the second phase, this will expand to the full
//! Chat Completions + tool-calling stream.

use std::collections::HashMap;

use serde::Deserialize;

use http_client::Client;

/// A single model entry returned by the `/models` endpoint.
///
/// We only care about `id` (used by the Agent as the model name). The other
/// fields (`object`/`created`/`owned_by`) vary widely across providers, so they
/// are all ignored here.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct OpenAiCompatibleModel {
    pub id: String,
    /// The owner inferred from `owned_by`, used mainly for UI display; may be empty.
    #[serde(default)]
    pub owned_by: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<OpenAiCompatibleModel>,
}

/// Errors that can occur during a fetch.
#[derive(Debug, thiserror::Error)]
pub enum OpenAiCompatibleError {
    #[error("Invalid base URL: {0}")]
    InvalidBaseUrl(String),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("HTTP status {status}: {body}")]
    Status { status: u16, body: String },

    #[error("Failed to parse response: {0}")]
    Decode(String),

    #[error("Network/streaming request failed: {0}")]
    Stream(String),

    #[error("Request failed: {0}")]
    Other(String),

    #[error(
        "refusing to send the API key to {0} over plaintext HTTP — only https:// or a \
         loopback endpoint (localhost/127.0.0.1) may carry a key; use https, or point this \
         provider at a local runtime"
    )]
    InsecureEndpoint(String),
}

/// Normalizes a user-supplied base_url into an absolute URL, tolerating a
/// trailing `/`, a missing `/v1`, `/openai/v1`, and similar variations.
pub(crate) fn normalize_base_url(input: &str) -> Result<String, OpenAiCompatibleError> {
    let trimmed = input.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(OpenAiCompatibleError::InvalidBaseUrl(
            "base URL must not be empty".to_string(),
        ));
    }
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        return Err(OpenAiCompatibleError::InvalidBaseUrl(format!(
            "base URL must start with http:// or https://: {trimmed}"
        )));
    }
    Ok(trimmed.to_string())
}

/// Calls `${base_url}/models` and returns the list of model IDs (de-duplicated
/// and sorted alphabetically).
///
/// Auth: if `api_key` is non-empty it is sent as `Authorization: Bearer ...`.
/// Some local services (e.g. Ollama) allow unauthenticated access, so no header
/// is sent when the key is empty.
pub async fn fetch_openai_compatible_models(
    client: Client,
    base_url: &str,
    api_key: Option<&str>,
) -> Result<Vec<OpenAiCompatibleModel>, OpenAiCompatibleError> {
    let base = normalize_base_url(base_url)?;
    let url = format!("{base}/models");

    let mut req = client.get(&url);
    if let Some(key) = api_key.filter(|k| !k.trim().is_empty()) {
        // Defense-in-depth: never carry the API key as a plaintext `Authorization: Bearer`
        // header outside loopback. `https://` is fine (TLS); `http://localhost` /
        // `http://127.0.0.1` (Ollama et al.) never leaves the machine. Any other `http://`
        // host would leak the key to whoever can observe the wire, so refuse outright
        // rather than silently sending it or silently dropping it.
        if super::is_plaintext_bearer_risk(&base) {
            return Err(OpenAiCompatibleError::InsecureEndpoint(base));
        }
        req = req.bearer_auth(key);
    }

    let response = req.send().await?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(OpenAiCompatibleError::Status {
            status: status.as_u16(),
            body,
        });
    }

    let parsed: ModelsResponse = response
        .json()
        .await
        .map_err(|e| OpenAiCompatibleError::Decode(e.to_string()))?;

    let mut models = parsed.data;
    models.sort_by(|a, b| a.id.cmp(&b.id));
    models.dedup_by(|a, b| a.id == b.id);
    Ok(models)
}

// ---------------------------------------------------------------------------
// Ollama `/api/show` metadata (context window / max output tokens)
// ---------------------------------------------------------------------------
//
// The OpenAI-compatible `/models` endpoint above has no context-length field in its
// response shape, so it can never populate `context_window`. Ollama's *native*
// `/api/show` endpoint (`POST {root}/api/show`, body `{"model": "<id>"}`) carries that
// information instead, but it is per-model, so filling it in requires one extra request
// per model returned by `/models`.

/// Number of concurrent `/api/show` requests fired during a single "Fetch from API"
/// refresh. Bounded so a provider with a large model catalog doesn't send a burst of
/// simultaneous requests at what is usually a single local server process.
const OLLAMA_SHOW_CONCURRENCY: usize = 4;

/// Context-window sizing recovered from one model's `/api/show` response.
/// `None` in either field means "nothing usable was found" -- the caller must leave
/// the corresponding settings field alone rather than writing a guess.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OllamaContextInfo {
    pub context_window: Option<u32>,
    pub max_output_tokens: Option<u32>,
}

/// The subset of Ollama's `/api/show` response we care about. Both fields are treated as
/// optional/best-effort: an older Ollama version, a model with no `model_info`, or a
/// malformed response should degrade to "nothing usable" rather than fail the whole
/// fetch (see [`fetch_ollama_model_context`]).
#[derive(Debug, Default, Deserialize)]
struct OllamaShowResponse {
    /// Newline-separated Modelfile `PARAMETER` lines, e.g. `"num_ctx 4096\nstop ..."`.
    #[serde(default)]
    parameters: Option<String>,
    /// Architecture-specific metadata. Keys are prefixed by the model's architecture
    /// (`"llama.context_length"`, `"gptoss.context_length"`, ...), so the prefix cannot
    /// be hardcoded -- see [`parse_trained_context_length`].
    #[serde(default)]
    model_info: HashMap<String, serde_json::Value>,
}

/// Extracts a `PARAMETER`-style `key <value>` line from Ollama's newline-separated
/// `parameters` string (e.g. `num_ctx 4096`), returning the parsed integer value of the
/// first matching line.
fn parse_parameter_u32(parameters: &str, key: &str) -> Option<u32> {
    parameters.lines().find_map(|line| {
        let mut tokens = line.split_whitespace();
        if tokens.next() != Some(key) {
            return None;
        }
        tokens.next()?.parse::<u32>().ok()
    })
}

/// Finds the trained context length in `model_info` by *suffix* match on
/// `.context_length` rather than a hardcoded architecture prefix -- the prefix varies
/// per model family (`llama.context_length`, `gptoss.context_length`, `qwen2.context_length`,
/// ...) and there is no fixed enum of them.
fn parse_trained_context_length(model_info: &HashMap<String, serde_json::Value>) -> Option<u32> {
    model_info.iter().find_map(|(key, value)| {
        if !key.ends_with(".context_length") {
            return None;
        }
        value.as_u64().and_then(|n| u32::try_from(n).ok())
    })
}

impl OllamaContextInfo {
    fn from_show_response(resp: &OllamaShowResponse) -> Self {
        let num_ctx = resp
            .parameters
            .as_deref()
            .and_then(|p| parse_parameter_u32(p, "num_ctx"));
        let trained = parse_trained_context_length(&resp.model_info);

        // `num_ctx` wins when present: it is what the server actually allocates for this
        // model at serve time. `model_info.*.context_length` is only the length the model
        // was *trained* for -- the server may be configured to serve less. Writing the
        // trained length when the server is actually serving less is worse than writing
        // nothing: the UI would read healthy right up until the server silently truncates.
        let context_window = num_ctx.or(trained);

        let num_predict = resp
            .parameters
            .as_deref()
            .and_then(|p| parse_parameter_u32(p, "num_predict"));
        // Guard against the equal-to-context_window trap: downstream `overflow::usable()`
        // computes roughly `context - max_output`, so a max_output_tokens equal to
        // context_window yields zero usable tokens. If num_predict happens to coincide
        // with the resolved context window, drop it rather than write a value that would
        // silently zero out the whole budget.
        let max_output_tokens = match (num_predict, context_window) {
            (Some(p), Some(c)) if p == c => None,
            (p, _) => p,
        };

        Self {
            context_window,
            max_output_tokens,
        }
    }
}

/// Strips a trailing `/v1` (the OpenAI-compat mount point Ollama also serves) from a
/// normalized base URL, since `/api/show` is Ollama's native API and always lives at the
/// server root regardless of where the OpenAI-compat shim is mounted.
///
/// Not verified against a live server: this assumes `/v1` is the only compat suffix in
/// practice (matches `AgentProviderApiType::Ollama::default_base_url`, which has no
/// suffix at all), but a user-entered base_url with some other suffix would not be
/// stripped here.
fn ollama_root_url(base_url: &str) -> Result<String, OpenAiCompatibleError> {
    let base = normalize_base_url(base_url)?;
    Ok(base.strip_suffix("/v1").unwrap_or(&base).to_string())
}

/// Fetches `/api/show` for a single model and returns whatever context-window info was
/// recoverable. Errors (network, non-2xx, malformed body) are returned to the caller,
/// which is expected to treat a per-model failure as non-fatal (see
/// [`fetch_ollama_context_map`]).
///
/// Auth mirrors [`fetch_openai_compatible_models`]: sent as `Authorization: Bearer` only
/// when non-empty, and refused over plaintext HTTP to a non-loopback host.
async fn fetch_ollama_model_context(
    client: &Client,
    base_url: &str,
    api_key: Option<&str>,
    model_id: &str,
) -> Result<OllamaContextInfo, OpenAiCompatibleError> {
    let root = ollama_root_url(base_url)?;
    let url = format!("{root}/api/show");

    let mut req = client
        .post(&url)
        .json(&serde_json::json!({ "model": model_id }));
    if let Some(key) = api_key.filter(|k| !k.trim().is_empty()) {
        if super::is_plaintext_bearer_risk(&root) {
            return Err(OpenAiCompatibleError::InsecureEndpoint(root));
        }
        req = req.bearer_auth(key);
    }

    let response = req.send().await?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(OpenAiCompatibleError::Status {
            status: status.as_u16(),
            body,
        });
    }

    let parsed: OllamaShowResponse = response
        .json()
        .await
        .map_err(|e| OpenAiCompatibleError::Decode(e.to_string()))?;

    Ok(OllamaContextInfo::from_show_response(&parsed))
}

/// Fetches `/api/show` for each of `model_ids` (bounded concurrency, see
/// [`OLLAMA_SHOW_CONCURRENCY`]) and returns whatever context info was recoverable, keyed
/// by model id. A model whose request fails, times out, or decodes to nothing usable is
/// simply absent from the map -- one bad model must not lose context info for the rest
/// of the models being fetched.
pub async fn fetch_ollama_context_map(
    client: &Client,
    base_url: &str,
    api_key: Option<&str>,
    model_ids: Vec<String>,
) -> HashMap<String, OllamaContextInfo> {
    use futures::StreamExt;

    futures::stream::iter(model_ids)
        .map(move |id| async move {
            let info = fetch_ollama_model_context(client, base_url, api_key, &id)
                .await
                .ok();
            (id, info)
        })
        .buffer_unordered(OLLAMA_SHOW_CONCURRENCY)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .filter_map(|(id, info)| info.map(|info| (id, info)))
        .collect()
}

#[cfg(test)]
mod ollama_context_tests {
    use super::*;

    fn info_from_json(body: &str) -> OllamaContextInfo {
        let resp: OllamaShowResponse = serde_json::from_str(body).expect("valid fixture JSON");
        OllamaContextInfo::from_show_response(&resp)
    }

    #[test]
    fn num_ctx_wins_over_trained_context_length() {
        // The server is configured to serve a smaller window than the model was trained
        // for -- num_ctx (what it actually allocates) must be reported, not the larger
        // trained figure, or the UI would read healthy right up until the server
        // silently truncates.
        let info = info_from_json(
            r#"{
                "parameters": "num_ctx 4096\nstop \"<|endoftext|>\"",
                "model_info": { "llama.context_length": 131072 }
            }"#,
        );
        assert_eq!(info.context_window, Some(4096));
    }

    #[test]
    fn suffix_matched_context_length_used_when_no_num_ctx() {
        // No num_ctx override present -- falls back to model_info's *.context_length,
        // matched by suffix since the architecture prefix varies per model family.
        let info = info_from_json(
            r#"{
                "parameters": "stop \"<|endoftext|>\"",
                "model_info": { "gptoss.context_length": 131072 }
            }"#,
        );
        assert_eq!(info.context_window, Some(131072));
    }

    #[test]
    fn malformed_or_absent_response_yields_nothing_usable() {
        // No parameters, no model_info at all.
        let info = info_from_json("{}");
        assert_eq!(info.context_window, None);
        assert_eq!(info.max_output_tokens, None);

        // model_info present but no key ends in .context_length.
        let info = info_from_json(r#"{"model_info": {"llama.rope_freq_base": 500000}}"#);
        assert_eq!(info.context_window, None);
    }

    #[test]
    fn num_predict_becomes_max_output_tokens_when_distinct_from_context_window() {
        let info = info_from_json(
            r#"{
                "parameters": "num_ctx 8192\nnum_predict 2048",
                "model_info": {}
            }"#,
        );
        assert_eq!(info.context_window, Some(8192));
        assert_eq!(info.max_output_tokens, Some(2048));
    }

    #[test]
    fn num_predict_equal_to_context_window_is_dropped() {
        // Writing max_output_tokens == context_window would zero out
        // overflow::usable()'s budget downstream, so this must resolve to None instead
        // of propagating the coincidental match.
        let info = info_from_json(
            r#"{
                "parameters": "num_ctx 4096\nnum_predict 4096",
                "model_info": {}
            }"#,
        );
        assert_eq!(info.context_window, Some(4096));
        assert_eq!(info.max_output_tokens, None);
    }

    #[test]
    fn negative_num_predict_is_ignored() {
        // Ollama uses -1 ("unlimited") / -2 ("fill context") as sentinels; parse_parameter_u32
        // parses into u32 so these simply fail to parse and yield None, which is correct --
        // neither sentinel is a usable finite max-output value.
        let info = info_from_json(r#"{"parameters": "num_ctx 4096\nnum_predict -1"}"#);
        assert_eq!(info.max_output_tokens, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Caps a fetch attempt so a regression that starts making a real network call (instead
    /// of failing fast, either via our own guard or a loopback connection refusal) fails the
    /// test in a few seconds rather than hanging the suite.
    async fn fetch_bounded(
        client: Client,
        base_url: &str,
        api_key: Option<&str>,
    ) -> Result<Vec<OpenAiCompatibleModel>, OpenAiCompatibleError> {
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            fetch_openai_compatible_models(client, base_url, api_key),
        )
        .await
        .expect("fetch must not hang — either our guard or a loopback connection refusal should return promptly")
    }

    #[tokio::test]
    async fn refuses_bearer_over_plaintext_http_to_non_loopback_host() {
        let err = fetch_bounded(
            Client::new_for_test(),
            "http://example.com",
            Some("super-secret-key"),
        )
        .await
        .expect_err("must refuse to send the key over plaintext HTTP to a non-loopback host");

        assert!(
            matches!(err, OpenAiCompatibleError::InsecureEndpoint(_)),
            "expected InsecureEndpoint, got: {err}"
        );
        assert!(
            err.to_string().contains("example.com"),
            "error should name the offending endpoint: {err}"
        );
    }

    #[tokio::test]
    async fn refuses_bearer_over_plaintext_http_to_non_loopback_ip() {
        let err = fetch_bounded(
            Client::new_for_test(),
            "http://192.168.1.50:11434",
            Some("super-secret-key"),
        )
        .await
        .expect_err("a LAN IP is not loopback and must be refused");

        assert!(matches!(err, OpenAiCompatibleError::InsecureEndpoint(_)));
    }

    #[tokio::test]
    async fn allows_bearer_over_plaintext_http_to_loopback() {
        // Port 1 is a reserved port essentially never listening, so this fails fast with a
        // connection error -- what matters is that it is NOT our InsecureEndpoint guard,
        // proving the local-Ollama-with-a-key use case still gets past the check.
        let err = fetch_bounded(
            Client::new_for_test(),
            "http://127.0.0.1:1",
            Some("some-local-key"),
        )
        .await
        .expect_err("nothing should be listening on port 1");

        assert!(
            !matches!(err, OpenAiCompatibleError::InsecureEndpoint(_)),
            "loopback http:// with a key must not be treated as insecure: {err}"
        );
    }

    #[tokio::test]
    async fn allows_bearer_over_https_to_any_host() {
        let err = fetch_bounded(
            Client::new_for_test(),
            "https://127.0.0.1:1",
            Some("some-key"),
        )
        .await
        .expect_err("nothing should be listening on port 1");

        assert!(
            !matches!(err, OpenAiCompatibleError::InsecureEndpoint(_)),
            "https:// must never be flagged as insecure: {err}"
        );
    }

    #[tokio::test]
    async fn no_key_never_triggers_the_insecure_endpoint_guard() {
        // No key -> no Authorization header is attempted at all, so the guard must not
        // fire even for a plain http:// endpoint (matches the existing "unauthenticated
        // local service" tolerance -- absence of a key is never itself insecure). Uses a
        // loopback address so the test never makes a real network call either way.
        let err = fetch_bounded(Client::new_for_test(), "http://127.0.0.1:1", None)
            .await
            .expect_err("nothing should be listening on port 1");

        assert!(
            !matches!(err, OpenAiCompatibleError::InsecureEndpoint(_)),
            "no key configured should never trip the insecure-endpoint guard: {err}"
        );
    }
}
