//! Minimal subset of an OpenAI-compatible client: currently only used to fetch
//! the `/models` list.
//!
//! When multi-agent calls land in the second phase, this will expand to the full
//! Chat Completions + tool-calling stream.

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
