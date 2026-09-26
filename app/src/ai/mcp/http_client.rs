use std::collections::HashMap;

use reqwest::header::HeaderMap;

type ReqwestHttpTransport = rmcp::transport::StreamableHttpClientTransport<reqwest::Client>;

/// Builds a `HeaderMap` from a `HashMap<String, String>` of user-provided headers.
///
/// Invalid header names or values are skipped.
fn build_header_map(headers: &HashMap<String, String>) -> HeaderMap {
    headers.try_into().unwrap_or_default()
}

/// Builds a reqwest client with custom headers for MCP HTTP/SSE connections.
///
/// Redirects are not followed. reqwest strips only its own list of sensitive
/// headers (`Authorization`, `Cookie`, ...) on a cross-origin redirect, so
/// user-configured headers such as `X-API-Key` would be re-sent to whatever
/// host the server redirects to. rmcp 2.1 closed this for its own default
/// client (CVE-2026-64684) the same way; this client bypasses that default.
#[allow(clippy::result_large_err)]
pub fn build_client_with_headers(
    headers: &HashMap<String, String>,
) -> Result<reqwest::Client, rmcp::RmcpError> {
    let header_map = build_header_map(headers);

    reqwest::Client::builder()
        .default_headers(header_map)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| {
            rmcp::RmcpError::transport_creation::<ReqwestHttpTransport>(format!(
                "Failed to build client with headers: {e}",
            ))
        })
}

#[cfg(test)]
#[path = "http_client_tests.rs"]
mod tests;
