//! Unit tests for the `Client` request builders.

use super::*;
use std::sync::Once;

static INSTALL_CRYPTO_PROVIDER: Once = Once::new();

/// A client with no TLS roots and no proxy, so building it is cheap and nothing can be sent.
fn test_client() -> Client {
    INSTALL_CRYPTO_PROVIDER.call_once(|| {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
    Client::from_client_builder(reqwest::ClientBuilder::new().tls_certs_only([]).no_proxy())
        .expect("should build test client")
}

/// Every header the Warp/Zap ancestry used to stamp onto outbound requests (#1062).
const REMOVED_IDENTIFYING_HEADERS: &[&str] = &[
    "X-Zap-Client-ID",
    "X-Zap-Client-Version",
    "X-Zap-OS-Category",
    "X-Zap-OS-Name",
    "X-Zap-OS-Version",
    "X-Zap-OS-Linux-Kernel-Version",
];

fn assert_no_identifying_headers(request: &Request, what: &str) {
    let headers = request.wrapped.headers();
    for name in REMOVED_IDENTIFYING_HEADERS {
        assert!(
            !headers.contains_key(*name),
            "{what} carried {name}: {:?}",
            headers.get(*name)
        );
    }
    // Catch any future re-introduction under a different name in the same family.
    for name in headers.keys() {
        let lower = name.as_str().to_ascii_lowercase();
        assert!(
            !lower.starts_with("x-zap-") && !lower.starts_with("x-warp-"),
            "{what} carried a client-identifying header: {name}"
        );
    }
}

/// Requests to third-party hosts (a BYOP provider, the models.dev catalog, a release
/// download) carry no client-ID / version / OS headers, whichever verb built them.
/// On the old code `OperatingSystemInfo::get()` succeeds in a test process, so at least
/// `X-Zap-OS-Category` and `X-Zap-OS-Name` were attached and this fails.
#[test]
fn third_party_requests_carry_no_client_identifying_headers() {
    let client = test_client();
    let urls = [
        "https://api.openai.com/v1/embeddings",
        "https://models.dev/api.json",
        "https://gitlab.com/api/v4/projects/88085499/releases",
        "http://localhost:11434/api/tags",
    ];
    for url in urls {
        let built = [
            ("GET", client.get(url).build()),
            ("POST", client.post(url).build()),
            ("PUT", client.put(url).build()),
            ("PATCH", client.patch(url).build()),
            ("DELETE", client.delete(url).build()),
        ];
        for (verb, request) in built {
            let request = request.expect("request should build");
            assert_no_identifying_headers(&request, &format!("{verb} {url}"));
        }
    }
}

/// Caller-supplied headers still go out untouched; only the injected ones are gone.
#[test]
fn caller_headers_are_preserved() {
    let client = test_client();
    let request = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", "k")
        .header(http::header::USER_AGENT, "Phosphor-Test")
        .build()
        .expect("request should build");
    let headers = request.wrapped.headers();
    assert_eq!(headers.get("x-api-key").unwrap(), "k");
    assert_eq!(
        headers.get(http::header::USER_AGENT).unwrap(),
        "Phosphor-Test"
    );
    assert_no_identifying_headers(&request, "POST with caller headers");
}
