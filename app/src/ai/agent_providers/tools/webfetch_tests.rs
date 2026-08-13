//! Unit tests for `web_runtime::run_webfetch` (mockito, no outbound network).

use super::*;
use mockito::{Matcher, Server};

fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .build()
        .expect("reqwest client build")
}

fn args(url: &str) -> FetchArgs {
    FetchArgs {
        url: url.to_owned(),
        format: None,
        timeout: None,
    }
}

// ---------------------------------------------------------------------------
// URL validation (pure logic, no HTTP)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rejects_non_https_scheme() {
    let client = build_client();
    for bad in [
        "ftp://example.com",
        "file:///etc/passwd",
        "javascript:alert(1)",
        "http://example.com",
        "",
    ] {
        let err = run_webfetch(&client, args(bad)).await.unwrap_err();
        assert!(err.to_string().contains("HTTPS"), "bad={bad} err={err}");
    }
}

#[tokio::test]
async fn rejects_http_urls() {
    let client = build_client();
    let err = run_webfetch(&client, args("http://example.com"))
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("HTTPS"),
        "HTTP should be rejected: {err}"
    );
}

// ---------------------------------------------------------------------------
// Content-type branches — calls send_fetch directly, since mockito only serves HTTP
// ---------------------------------------------------------------------------

/// Helper: runs a webfetch-like flow against the mockito server, skipping the HTTPS check
/// (mockito only serves HTTP). Tests the content-processing pipeline without going through
/// URL scheme validation. Reuses `response_to_fetch_output` to avoid drifting from
/// production logic.
async fn run_webfetch_test(
    server_url: &str,
    path: &str,
    format: Option<FetchFormat>,
) -> Result<FetchOutput> {
    let client = build_client();
    let url = format!("{server_url}{path}");
    let fmt = format.unwrap_or_default();
    let accept = fmt.accept_header();
    let timeout = std::time::Duration::from_secs(DEFAULT_FETCH_TIMEOUT_SECS);

    let resp = send_fetch(&client, &url, accept, CHROME_UA, timeout).await?;
    response_to_fetch_output(resp, &url, &fmt).await
}

#[tokio::test]
async fn html_to_markdown() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("GET", "/page")
        .with_status(200)
        .with_header("content-type", "text/html; charset=utf-8")
        .with_body("<html><body><h1>Hello</h1><p>World</p></body></html>")
        .create_async()
        .await;

    let out = run_webfetch_test(&server.url(), "/page", None)
        .await
        .expect("ok");
    assert!(
        out.output.contains("Hello"),
        "missing Hello: {}",
        out.output
    );
    assert!(
        out.output.contains("World"),
        "missing World: {}",
        out.output
    );
    assert!(
        out.output.contains('#') || !out.output.contains("<h1>"),
        "should be markdown not HTML: {}",
        out.output
    );
    assert_eq!(out.format, "markdown");
    assert!(out.attachments.is_empty());
}

#[tokio::test]
async fn text_plain_passthrough() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("GET", "/text")
        .with_status(200)
        .with_header("content-type", "text/plain")
        .with_body("just some text")
        .create_async()
        .await;

    let out = run_webfetch_test(&server.url(), "/text", None)
        .await
        .expect("ok");
    assert_eq!(out.output, "just some text");
}

#[tokio::test]
async fn json_pretty_print() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("GET", "/api")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"a":1,"b":[2,3]}"#)
        .create_async()
        .await;

    let out = run_webfetch_test(&server.url(), "/api", None)
        .await
        .expect("ok");
    assert!(
        out.output.starts_with("```json\n"),
        "missing fence: {}",
        out.output
    );
    assert!(
        out.output.contains("\"a\": 1"),
        "not pretty: {}",
        out.output
    );
    assert!(out.output.ends_with("\n```"));
}

#[tokio::test]
async fn image_attachment_base64() {
    let mut server = Server::new_async().await;
    // 1x1 transparent PNG
    let png_bytes: Vec<u8> = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    let _m = server
        .mock("GET", "/img.png")
        .with_status(200)
        .with_header("content-type", "image/png")
        .with_body(png_bytes.clone())
        .create_async()
        .await;

    let out = run_webfetch_test(&server.url(), "/img.png", None)
        .await
        .expect("ok");
    assert_eq!(out.attachments.len(), 1);
    let att = &out.attachments[0];
    assert_eq!(att.mime, "image/png");
    assert!(att.url.starts_with("data:image/png;base64,"));
    let b64 = att.url.trim_start_matches("data:image/png;base64,");
    let decoded = BASE64.decode(b64).expect("decode");
    assert_eq!(decoded, png_bytes);
}

// ---------------------------------------------------------------------------
// format parameter
// ---------------------------------------------------------------------------

#[tokio::test]
async fn format_html_returns_raw() {
    let mut server = Server::new_async().await;
    let raw = "<html><body><h1>Raw</h1></body></html>";
    let _m = server
        .mock("GET", "/x")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(raw)
        .create_async()
        .await;

    let out = run_webfetch_test(&server.url(), "/x", Some(FetchFormat::Html))
        .await
        .expect("ok");
    assert_eq!(out.output, raw);
    assert_eq!(out.format, "html");
}

#[tokio::test]
async fn format_text_strips_html() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("GET", "/x")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body("<html><body><p>One</p><p>Two</p><script>alert(1)</script></body></html>")
        .create_async()
        .await;

    let out = run_webfetch_test(&server.url(), "/x", Some(FetchFormat::Text))
        .await
        .expect("ok");
    assert!(out.output.contains("One"));
    assert!(out.output.contains("Two"));
    assert!(
        !out.output.contains("alert(1)"),
        "script content should be stripped: {}",
        out.output
    );
    assert_eq!(out.format, "text");
}

#[tokio::test]
async fn default_format_is_markdown() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("GET", "/x")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body("<html><body><h2>x</h2></body></html>")
        .create_async()
        .await;
    let out = run_webfetch_test(&server.url(), "/x", None).await.unwrap();
    assert_eq!(out.format, "markdown");
}

#[tokio::test]
async fn accept_header_negotiation_for_markdown() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("GET", "/x")
        .match_header(
            "accept",
            Matcher::Regex(r"text/markdown\s*;\s*q=1\.0".into()),
        )
        .with_status(200)
        .with_header("content-type", "text/plain")
        .with_body("ok")
        .create_async()
        .await;

    let out = run_webfetch_test(&server.url(), "/x", None)
        .await
        .expect("ok");
    assert_eq!(out.output, "ok");
}

// ---------------------------------------------------------------------------
// Size / status
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rejects_oversized_content_length() {
    let big = vec![b'x'; MAX_RESPONSE_SIZE + 1024];
    let mut server = Server::new_async().await;
    let _m = server
        .mock("GET", "/big")
        .with_status(200)
        .with_header("content-type", "text/plain")
        .with_body(big)
        .create_async()
        .await;

    let err = run_webfetch_test(&server.url(), "/big", None)
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("too large"), "got: {msg}");
}

#[tokio::test]
async fn http_error_status_propagates() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("GET", "/404")
        .with_status(404)
        .create_async()
        .await;
    let err = run_webfetch_test(&server.url(), "/404", None)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("404"), "got: {err}");
}

// ---------------------------------------------------------------------------
// SSRF: is_blocked_ip coverage tests
// ---------------------------------------------------------------------------

#[test]
fn blocked_ip_ipv4_basics() {
    use std::net::IpAddr;
    for blocked in [
        "127.0.0.1",
        "10.0.0.1",
        "172.16.0.1",
        "192.168.1.1",
        "169.254.1.1",
        "0.0.0.0",
        "0.0.0.1",       // 0.0.0.0/8 "this host" range
        "0.255.255.255", // 0.0.0.0/8 upper bound
        "255.255.255.255",
        "100.64.0.1",      // CGNAT
        "192.0.2.1",       // TEST-NET-1
        "198.51.100.1",    // TEST-NET-2
        "203.0.113.1",     // TEST-NET-3
        "198.18.0.1",      // benchmarking range
        "224.0.0.1",       // multicast
        "239.255.255.255", // multicast upper bound
        "240.0.0.1",       // reserved range
    ] {
        let ip: IpAddr = blocked.parse().unwrap();
        assert!(is_blocked_ip(ip), "should block {blocked}");
    }
    // Public IPs must not be blocked by mistake.
    for allowed in ["8.8.8.8", "1.1.1.1", "93.184.216.34"] {
        let ip: IpAddr = allowed.parse().unwrap();
        assert!(!is_blocked_ip(ip), "should allow {allowed}");
    }
}

#[test]
fn blocked_ip_ipv4_mapped_ipv6() {
    use std::net::IpAddr;
    // ::ffff:127.0.0.1 is IPv4-mapped IPv6 and must be blocked as an IPv4 loopback.
    let mapped_loopback: IpAddr = "::ffff:127.0.0.1".parse().unwrap();
    assert!(
        is_blocked_ip(mapped_loopback),
        "::ffff:127.0.0.1 must be blocked"
    );

    let mapped_private: IpAddr = "::ffff:10.0.0.1".parse().unwrap();
    assert!(
        is_blocked_ip(mapped_private),
        "::ffff:10.0.0.1 must be blocked"
    );

    let mapped_link_local: IpAddr = "::ffff:169.254.1.1".parse().unwrap();
    assert!(
        is_blocked_ip(mapped_link_local),
        "::ffff:169.254.1.1 must be blocked"
    );

    // ::ffff:8.8.8.8 maps to a public IPv4 address and must not be blocked by mistake.
    let mapped_public: IpAddr = "::ffff:8.8.8.8".parse().unwrap();
    assert!(
        !is_blocked_ip(mapped_public),
        "::ffff:8.8.8.8 should be allowed"
    );
}

#[test]
fn blocked_ip_ipv6_ranges() {
    use std::net::IpAddr;
    for blocked in [
        "::1",         // loopback
        "::",          // unspecified
        "fc00::1",     // unique-local
        "fe80::1",     // link-local
        "ff00::1",     // multicast
        "2001:db8::1", // documentation example range
    ] {
        let ip: IpAddr = blocked.parse().unwrap();
        assert!(is_blocked_ip(ip), "should block {blocked}");
    }
    // Public IPv6 must not be blocked by mistake.
    let public: IpAddr = "2606:4700:4700::1111".parse().unwrap();
    assert!(!is_blocked_ip(public), "public IPv6 should be allowed");
}

// ---------------------------------------------------------------------------
// SSRF redirect protection
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ssrf_safe_client_builds_with_redirect_policy() {
    let client = build_ssrf_safe_client().expect("build client");
    // Verifies the client can be built successfully with the custom SSRF redirect policy and
    // DNS resolver.
    // TODO: once mockito supports redirects, add a real internal-IP redirect integration test.
    assert!(client.get("https://example.invalid").build().is_ok());
}

// ---------------------------------------------------------------------------
// FetchOutput serialization tests
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Real-endpoint smoke tests are skipped by default, to avoid CI or offline dev environments
// depending on outbound network. Set WARP_RUN_WEB_INTEGRATION=1 to verify against real
// endpoints manually.
// ---------------------------------------------------------------------------

fn skip_real() -> bool {
    std::env::var("WARP_RUN_WEB_INTEGRATION").is_err()
}

#[tokio::test]
async fn real_example_com_markdown() {
    if skip_real() {
        return;
    }
    let client = build_ssrf_safe_client().expect("build client");
    let out = run_webfetch(&client, args("https://example.com"))
        .await
        .expect("real example.com");
    assert!(
        out.output.to_lowercase().contains("example domain"),
        "got: {}",
        out.output
    );
}

#[tokio::test]
async fn real_httpbin_html_to_markdown() {
    if skip_real() {
        return;
    }
    let client = build_ssrf_safe_client().expect("build client");
    let out = run_webfetch(&client, args("https://httpbin.org/html"))
        .await
        .expect("real httpbin html");
    assert!(!out.output.trim().is_empty());
    assert_eq!(out.format, "markdown");
}

#[tokio::test]
async fn real_httpbin_json_pretty() {
    if skip_real() {
        return;
    }
    let client = build_ssrf_safe_client().expect("build client");
    let out = run_webfetch(&client, args("https://httpbin.org/json"))
        .await
        .expect("real httpbin json");
    assert!(out.output.contains("```json"), "got: {}", out.output);
}

#[tokio::test]
async fn real_httpbin_image_attachment() {
    if skip_real() {
        return;
    }
    let client = build_ssrf_safe_client().expect("build client");
    let out = run_webfetch(&client, args("https://httpbin.org/image/png"))
        .await
        .expect("real png");
    assert_eq!(out.attachments.len(), 1);
    assert_eq!(out.attachments[0].mime, "image/png");
}

#[tokio::test]
async fn real_httpbin_404_errors() {
    if skip_real() {
        return;
    }
    let client = build_ssrf_safe_client().expect("build client");
    let err = run_webfetch(&client, args("https://httpbin.org/status/404"))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("404"), "got: {err}");
}

// ---------------------------------------------------------------------------
// Description doc / opencode byte-level alignment regression
// ---------------------------------------------------------------------------

/// Locks webfetch.md to be byte-for-byte identical to opencode's
/// `packages/opencode/src/tool/webfetch.txt`. Keep both sides in sync when editing.
#[test]
fn webfetch_description_matches_opencode_verbatim() {
    use super::super::webfetch::WEBFETCH;
    let expected = "- Fetches content from a specified URL\n\
                    - Takes a URL and optional format as input\n\
                    - Fetches the URL content, converts to requested format (markdown by default)\n\
                    - Returns the content in the specified format\n\
                    - Use this tool when you need to retrieve and analyze web content\n\
                    \n\
                    Usage notes:\n\
                    \x20\x20- IMPORTANT: if another tool is present that offers better web fetching capabilities, is more targeted to the task, or has fewer restrictions, prefer using that tool instead of this one.\n\
                    \x20\x20- The URL must be a fully-formed valid URL\n\
                    \x20\x20- The URL must use HTTPS (http:// URLs are rejected)\n\
                    \x20\x20- Format options: \"markdown\" (default), \"text\", or \"html\"\n\
                    \x20\x20- This tool is read-only and does not modify any files\n\
                    \x20\x20- Results may be summarized if the content is very large\n";
    assert_eq!(WEBFETCH.description, expected);
}

#[test]
fn fetch_output_omits_empty_attachments_in_json() {
    let out = FetchOutput {
        url: "https://x".into(),
        status: 200,
        content_type: "text/plain".into(),
        format: "markdown".into(),
        output: "hi".into(),
        attachments: vec![],
    };
    let v = fetch_output_to_json(&out);
    assert!(
        v.get("attachments").is_none(),
        "empty attachments should be skipped: {v}"
    );
    assert_eq!(v["output"], "hi");
}

/// The `_byop_intercepted` sentinel must be present in every web tool result (including
/// errors), otherwise the controller (`controller.rs::needs_byop_local_resume`) won't trigger
/// auto-resume — the model gets stuck waiting for a result and the UI shows a silent failure.
#[test]
fn fetch_output_carries_byop_sentinel() {
    let out = FetchOutput {
        url: "https://x".into(),
        status: 200,
        content_type: "text/plain".into(),
        format: "markdown".into(),
        output: "hi".into(),
        attachments: vec![],
    };
    let v = fetch_output_to_json(&out);
    assert_eq!(v["_byop_intercepted"], true);

    let err = error_to_json("webfetch", &anyhow::anyhow!("boom"));
    assert_eq!(err["_byop_intercepted"], true);
    assert_eq!(err["status"], "error");
}

/// `chat_stream::dispatch_byop_web_tool` wraps a `serde_json::from_str::<FetchArgs>` /
/// `::<SearchToolArgs>` failure as `anyhow::anyhow!(format!("invalid arguments: {e}"))` (note:
/// space-separated, not the `invalid_arguments:` underscore form the codebase/get_relevant_files
/// dispatchers use) before handing it to `error_to_json` -- this is the exact shape a malformed
/// model tool call to webfetch/websearch produces. Previously that always came out as a plain
/// `{"status":"error",...}` payload, which `convert_from::invalid_arguments_rejected_tool_call`
/// doesn't recognize, so the UI rendered nothing at all: the user saw the agent silently do
/// nothing. This locks the fix: the synthesized error must carry the same
/// `error: "invalid_arguments"` marker `todowrite::invalid_arguments_result_to_json` uses, so
/// the UI shows a real rejected-tool-call message instead.
#[test]
fn malformed_fetch_args_error_is_recognizable_as_invalid_arguments() {
    let serde_err = serde_json::from_str::<FetchArgs>("{}").unwrap_err();
    let v = error_to_json(
        "webfetch",
        &anyhow::anyhow!(format!("invalid arguments: {serde_err}")),
    );
    assert_eq!(v["_byop_intercepted"], true);
    assert_eq!(v["error"], "invalid_arguments");
    assert_eq!(v["tool"], "webfetch");
    assert!(v.get("status").is_none(), "must not also carry the old status:error shape");
    assert!(
        v["detail"].as_str().unwrap().contains("url"),
        "detail should surface the underlying parse error, got {v:?}"
    );
}

/// Same as above, for `websearch`'s `SearchToolArgs`.
#[test]
fn malformed_search_args_error_is_recognizable_as_invalid_arguments() {
    let serde_err = serde_json::from_str::<SearchToolArgs>("{}").unwrap_err();
    let v = error_to_json(
        "websearch",
        &anyhow::anyhow!(format!("invalid arguments: {serde_err}")),
    );
    assert_eq!(v["_byop_intercepted"], true);
    assert_eq!(v["error"], "invalid_arguments");
    assert_eq!(v["tool"], "websearch");
    assert!(v.get("status").is_none(), "must not also carry the old status:error shape");
    assert!(
        v["detail"].as_str().unwrap().contains("query"),
        "detail should surface the underlying parse error, got {v:?}"
    );
}

/// A genuine runtime/gate error (as opposed to a parse failure) must keep the original
/// `{"status":"error"}` shape -- only the `invalid arguments:` / `invalid_arguments:`-prefixed
/// message is special-cased. This guards against the prefix sniff in `error_to_json`
/// over-firing.
#[test]
fn non_parse_error_keeps_status_error_shape() {
    let v = error_to_json(
        "webfetch",
        &anyhow::anyhow!("web_search_enabled is disabled for this profile"),
    );
    assert_eq!(v["_byop_intercepted"], true);
    assert_eq!(v["status"], "error");
    assert!(v.get("error").is_none());
}
