use rmcp::transport::auth::OAuthTokenResponse;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;

use super::*;

/// Sends a bare-bones HTTP GET to a loopback redirect URI, as a browser would when following
/// the OAuth provider's redirect, and returns the raw HTTP response text.
async fn send_loopback_callback(redirect_uri: &str, query: &str) -> String {
    let url = Url::parse(redirect_uri).expect("redirect URI should parse");
    let address = format!(
        "{}:{}",
        url.host_str().expect("redirect should have a host"),
        url.port().expect("redirect should have a port")
    );
    let mut stream = TcpStream::connect(address)
        .await
        .expect("loopback callback should connect");
    let target = format!("{}?{query}", url.path());
    stream
        .write_all(
            format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .await
        .expect("callback request should write");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .await
        .expect("callback response should read");
    response
}

/// The loopback listener must bind to `127.0.0.1` only (never `0.0.0.0`), and must accept a
/// callback whose `state` matches what the caller is expecting.
#[tokio::test]
async fn loopback_callback_accepts_matching_state() {
    let receiver = LoopbackOAuthReceiver::bind()
        .await
        .expect("loopback receiver should bind");
    let redirect_uri = receiver.redirect_uri().to_string();
    assert!(
        redirect_uri.starts_with("http://127.0.0.1:"),
        "loopback redirect URI must be loopback-only, got {redirect_uri}"
    );
    let callback = tokio::spawn(async move {
        send_loopback_callback(&redirect_uri, "code=test-code&state=test-state").await
    });

    let result = receiver
        .receive("test-state")
        .await
        .expect("matching callback should succeed");
    match result {
        CallbackResult::Success { code, csrf_token } => {
            assert_eq!(code, "test-code");
            assert_eq!(csrf_token, "test-state");
        }
        CallbackResult::Error { error } => panic!("unexpected callback error: {error:?}"),
    }
    assert!(callback
        .await
        .expect("callback task should join")
        .contains("200 OK"));
}

/// CSRF protection: a callback whose `state` does not match the one generated for this
/// authorization attempt must be rejected, not treated as a successful login.
#[tokio::test]
async fn loopback_callback_rejects_mismatched_state() {
    let receiver = LoopbackOAuthReceiver::bind()
        .await
        .expect("loopback receiver should bind");
    let redirect_uri = receiver.redirect_uri().to_string();
    let callback = tokio::spawn(async move {
        send_loopback_callback(&redirect_uri, "code=test-code&state=wrong-state").await
    });

    let error = receiver
        .receive("expected-state")
        .await
        .expect_err("mismatched callback should fail");
    assert!(error.to_string().contains("state did not match"));
    assert!(callback
        .await
        .expect("callback task should join")
        .contains("400 Bad Request"));
}

/// Builds a minimal `OAuthTokenResponse` for tests, optionally with a refresh token.
fn make_test_token_response(refresh_token: Option<&str>) -> OAuthTokenResponse {
    let mut json = serde_json::json!({
        "access_token": "test_access_token",
        "token_type": "bearer",
        "expires_in": 3600,
    });
    if let Some(rt) = refresh_token {
        json["refresh_token"] = serde_json::Value::String(rt.to_string());
    }
    serde_json::from_value(json).expect("OAuthTokenResponse deserialization")
}

fn make_test_store(
    client_secret: Option<String>,
) -> (
    PersistingCredentialStore,
    async_channel::Receiver<PersistedCredentials>,
) {
    let (tx, rx) = async_channel::unbounded();
    let store = PersistingCredentialStore {
        inner: InMemoryCredentialStore::new(),
        client_secret,
        persist_tx: tx,
    };
    (store, rx)
}

/// Backward compatibility: credentials persisted by older Zap/Warp versions do
/// not have the `token_received_at` field. Deserializing them must succeed
/// and default to `None` so the next refresh can populate it. Failing this
/// test would mean every existing user loses their MCP OAuth tokens on
/// upgrade.
#[test]
fn persisted_credentials_deserializes_legacy_format_without_received_at() {
    // Legacy format: no `token_received_at` field.
    let legacy_json = r#"{
        "client_id": "client-abc",
        "client_secret": null,
        "token_response": {
            "access_token": "old_access",
            "token_type": "bearer",
            "expires_in": 3600,
            "refresh_token": "old_refresh"
        }
    }"#;

    let parsed: PersistedCredentials =
        serde_json::from_str(legacy_json).expect("legacy format must deserialize");

    assert_eq!(parsed.credentials.client_id, "client-abc");
    assert_eq!(parsed.credentials.token_received_at, None);
}

/// When rmcp persists refreshed credentials via `CredentialStore::save`, the
/// `token_received_at` must be forwarded into the channel so the persisted
/// (secure-storage) representation can stamp it. Without this, a restart
/// would lose the timestamp and rmcp's pre-emptive refresh check would be
/// permanently disabled for the cached session.
#[tokio::test]
async fn save_forwards_token_received_at_to_persist_channel() {
    let (store, rx) = make_test_store(Some("client_secret_xyz".to_string()));

    let credentials = StoredCredentials {
        client_id: "client-id".to_string(),
        token_response: Some(make_test_token_response(Some("refresh-1"))),
        granted_scopes: Vec::new(),
        token_received_at: Some(1_700_000_500),
    };

    store.save(credentials).await.expect("save succeeds");

    let persisted = rx.try_recv().expect("persist channel received credentials");
    assert_eq!(persisted.credentials.token_received_at, Some(1_700_000_500));
    assert_eq!(persisted.credentials.client_id, "client-id");
    assert_eq!(
        persisted.client_secret.as_deref(),
        Some("client_secret_xyz")
    );
}

/// Defensive: if rmcp ever calls `save` without a `token_received_at`
/// (e.g., during initial credential set-up before refresh), we must
/// propagate `None` rather than silently substituting a value.
#[tokio::test]
async fn save_forwards_none_when_received_at_is_none() {
    let (store, rx) = make_test_store(None);

    let credentials = StoredCredentials {
        client_id: "c".to_string(),
        token_response: Some(make_test_token_response(None)),
        granted_scopes: Vec::new(),
        token_received_at: None,
    };

    store.save(credentials).await.expect("save succeeds");

    let persisted = rx.try_recv().expect("persist channel received credentials");
    assert_eq!(persisted.credentials.token_received_at, None);
}

/// `save` only forwards a credentials snapshot to the persist channel when
/// `token_response` is `Some`. This guards the existing branch from regression.
#[tokio::test]
async fn save_skips_persist_when_token_response_absent() {
    let (store, rx) = make_test_store(None);

    let credentials = StoredCredentials {
        client_id: "c".to_string(),
        token_response: None,
        granted_scopes: Vec::new(),
        token_received_at: Some(1_700_000_500),
    };

    store.save(credentials).await.expect("save succeeds");

    assert!(
        rx.try_recv().is_err(),
        "no PersistedCredentials should be sent when token_response is absent"
    );
}

/// The carry-forward of refresh tokens (when the OAuth server omits one
/// from a refresh response) must not interfere with `token_received_at`
/// propagation. Tests both behaviors in one save: the new credentials get
/// the prior refresh token AND the new `token_received_at`.
#[tokio::test]
async fn save_carries_forward_refresh_token_and_preserves_received_at() {
    let (store, rx) = make_test_store(None);

    // Seed the inner store with prior credentials that have a refresh token.
    store
        .inner
        .save(StoredCredentials {
            client_id: "c".to_string(),
            token_response: Some(make_test_token_response(Some("prior-refresh-token"))),
            granted_scopes: Vec::new(),
            token_received_at: Some(1_699_000_000),
        })
        .await
        .expect("seed succeeds");

    // Now save NEW credentials that omit a refresh token, simulating a
    // refresh response from a server that does not rotate refresh tokens.
    let new_credentials = StoredCredentials {
        client_id: "c".to_string(),
        token_response: Some(make_test_token_response(None)),
        granted_scopes: Vec::new(),
        token_received_at: Some(1_700_000_500),
    };

    store.save(new_credentials).await.expect("save succeeds");

    let persisted = rx.try_recv().expect("persist channel received credentials");
    assert_eq!(
        persisted.credentials.token_received_at,
        Some(1_700_000_500),
        "newer received_at preserved"
    );

    let refresh_token = persisted
        .credentials
        .token_response
        .and_then(|tr| tr.refresh_token().cloned());
    assert_eq!(
        refresh_token.map(|rt| rt.secret().to_string()),
        Some("prior-refresh-token".to_string()),
        "prior refresh token carried forward"
    );
}
