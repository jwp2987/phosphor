//! Vertex AI OAuth2 bearer-token minting for BYOP.
//!
//! Unlike every other BYOP provider type, Vertex does not use a static API key — each request
//! carries a short-lived GCP OAuth2 access token (~1h). We mint that token via the `gcloud` CLI,
//! which already handles the messy parts of GCP auth: Application Default Credentials, the active
//! logged-in account, and service-account impersonation. The token is cached in-process so we
//! don't spawn a subprocess on every conversation turn, and refreshed well before expiry.
//!
//! The single public entry point is [`access_token`]. It is called from the async
//! `ServiceTargetResolver` in `chat_stream::build_client_uncached` for `AgentProviderApiType::
//! Vertex`, so the token is fetched lazily per request while the underlying reqwest client
//! (connection pool) stays cached.
//!
//! Follow-up: for headless environments without the `gcloud` CLI, this is the natural place to
//! add a `gcp_auth`-based path that mints directly from a service-account JSON stored in
//! `AgentProviderSecrets`.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// gcloud access tokens are valid for roughly one hour. We refresh well before that to stay safe
/// against clock skew and long streaming turns, while still avoiding a `gcloud` subprocess on
/// every turn.
const TOKEN_TTL: Duration = Duration::from_secs(30 * 60);

/// Small cap on distinct credentials cached at once (a user rarely configures more than a couple
/// of Vertex providers).
const TOKEN_CACHE_CAP: usize = 8;

struct CachedToken {
    /// The credential key this token was minted for (empty = ADC / active account).
    credential: String,
    token: String,
    fetched_at: Instant,
}

static TOKEN_CACHE: Mutex<Vec<CachedToken>> = Mutex::new(Vec::new());

/// Returns a valid GCP OAuth2 access token for the given credential, minting a fresh one via
/// `gcloud` when the cache is empty or stale.
///
/// `credential` selects how the token is obtained:
/// - empty → the active `gcloud` account / Application Default Credentials.
/// - non-empty → treated as a service-account email to impersonate
///   (`--impersonate-service-account=<email>`); requires the active account to hold the
///   Service Account Token Creator role on that SA.
pub async fn access_token(credential: &str) -> Result<String, String> {
    let credential = credential.trim().to_string();

    if let Some(token) = cached_token(&credential) {
        return Ok(token);
    }

    let token = mint_via_gcloud(&credential).await?;
    store_token(&credential, &token);
    Ok(token)
}

fn cached_token(credential: &str) -> Option<String> {
    let cache = TOKEN_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cache
        .iter()
        .find(|c| c.credential == credential && c.fetched_at.elapsed() < TOKEN_TTL)
        .map(|c| c.token.clone())
}

fn store_token(credential: &str, token: &str) {
    let mut cache = TOKEN_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cache.retain(|c| c.credential != credential);
    cache.insert(
        0,
        CachedToken {
            credential: credential.to_owned(),
            token: token.to_owned(),
            fetched_at: Instant::now(),
        },
    );
    cache.truncate(TOKEN_CACHE_CAP);
}

async fn mint_via_gcloud(credential: &str) -> Result<String, String> {
    let mut cmd = command::r#async::Command::new("gcloud");
    cmd.arg("auth").arg("print-access-token");
    if !credential.is_empty() {
        cmd.arg(format!("--impersonate-service-account={credential}"));
    }

    let output = cmd.output().await.map_err(|e| {
        format!(
            "failed to run `gcloud auth print-access-token` — is the gcloud CLI installed and on \
             PATH? ({e})"
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "`gcloud auth print-access-token` failed ({}). Run `gcloud auth login` (or `gcloud \
             auth application-default login`) and confirm the project is set. Details: {}",
            output.status,
            stderr.trim()
        ));
    }

    let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if token.is_empty() {
        return Err("gcloud returned an empty access token".to_owned());
    }
    Ok(token)
}
