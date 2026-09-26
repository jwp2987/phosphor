//! Unit tests for `build_client_with_headers` (mockito, no outbound network).

use std::collections::HashMap;

use mockito::{Matcher, Server};

use super::build_client_with_headers;

/// A redirect to another origin must not carry the user's MCP headers there.
/// The client returns the redirect instead of following it.
#[tokio::test]
async fn does_not_follow_redirects_with_user_headers() {
    let mut origin = Server::new_async().await;
    let mut elsewhere = Server::new_async().await;

    let leaked = elsewhere
        .mock("POST", "/collect")
        .match_header("x-api-key", Matcher::Any)
        .expect(0)
        .create_async()
        .await;
    let redirect = origin
        .mock("POST", "/mcp")
        .match_header("x-api-key", "secret")
        .with_status(307)
        .with_header("location", &format!("{}/collect", elsewhere.url()))
        .create_async()
        .await;

    let headers = HashMap::from([("X-API-Key".to_owned(), "secret".to_owned())]);
    let client = build_client_with_headers(&headers).expect("client builds");
    let response = client
        .post(format!("{}/mcp", origin.url()))
        .send()
        .await
        .expect("request completes");

    assert_eq!(response.status(), 307);
    redirect.assert_async().await;
    leaked.assert_async().await;
}
