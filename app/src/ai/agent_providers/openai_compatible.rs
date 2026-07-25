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
