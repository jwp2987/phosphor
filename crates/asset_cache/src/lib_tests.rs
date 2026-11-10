use super::*;

/// Drive the fetch closure of an [`AssetSource::Async`] to completion.
fn fetch_bytes(source: &AssetSource) -> Result<Bytes> {
    match source {
        AssetSource::Async { fetch, .. } => futures::executor::block_on(fetch()),
        other => panic!("expected an Async source, got {other:?}"),
    }
}

#[test]
fn data_uri_source_decodes_base64_payload() {
    let source = data_uri_source("data:image/png;base64,iVBORw0KGgo=")
        .expect("base64 data URI should produce a source");
    let bytes = fetch_bytes(&source).expect("payload should decode");
    assert_eq!(
        bytes.as_ref(),
        BASE64_STANDARD.decode("iVBORw0KGgo=").unwrap().as_slice()
    );
}

#[test]
fn data_uri_source_strips_embedded_whitespace() {
    // base64 payloads saved in notebooks frequently contain newlines.
    let source =
        data_uri_source("data:image/png;base64,iVBO\nRw0K Ggo=").expect("should produce a source");
    let bytes = fetch_bytes(&source).expect("payload should decode after stripping whitespace");
    assert_eq!(
        bytes.as_ref(),
        BASE64_STANDARD.decode("iVBORw0KGgo=").unwrap().as_slice()
    );
}

#[test]
fn data_uri_source_rejects_non_base64_data_uris() {
    assert!(data_uri_source("https://example.com/a.png").is_none());
    assert!(data_uri_source("/abs/path.png").is_none());
    assert!(data_uri_source("relative/path.png").is_none());
    // A `data:` URI without the `;base64` marker is not a renderable asset.
    assert!(data_uri_source("data:text/plain,hello").is_none());
    // A `data:` URI without a comma separator is malformed.
    assert!(data_uri_source("data:image/png;base64").is_none());
}

#[test]
fn data_uri_source_invalid_base64_fails_on_fetch() {
    // Detection succeeds on the prefix/marker, but decoding the bad payload
    // surfaces as a fetch error (FailedToLoad) rather than a panic.
    let source =
        data_uri_source("data:image/png;base64,not valid base64!").expect("detected as data URI");
    assert!(fetch_bytes(&source).is_err());
}

#[test]
fn data_uri_source_rejects_oversized_payload() {
    // An untrusted, oversized payload must be rejected before it is cloned or
    // decoded, so it never produces an asset source.
    let huge = "A".repeat(MAX_DATA_URI_PAYLOAD_BYTES + 1);
    let source = format!("data:image/png;base64,{huge}");
    assert!(data_uri_source(&source).is_none());
}

#[test]
fn data_uri_exceeds_limit_flags_only_oversized_base64_payloads() {
    let huge = "A".repeat(MAX_DATA_URI_PAYLOAD_BYTES + 1);
    assert!(data_uri_exceeds_limit(&format!(
        "data:image/png;base64,{huge}"
    )));

    // In-limit payloads, non-base64 `data:` URIs, and non-`data:` sources are
    // not flagged as oversized.
    assert!(!data_uri_exceeds_limit(
        "data:image/png;base64,iVBORw0KGgo="
    ));
    assert!(!data_uri_exceeds_limit(&format!("data:text/plain,{huge}")));
    assert!(!data_uri_exceeds_limit("https://example.com/a.png"));
    assert!(!data_uri_exceeds_limit("relative/path.png"));
}

/// The asset cache logs a URL source with `{:?}` and a failed fetch with its error. Neither
/// may name the remote image's host, port or path: the app log carries a host hash at most.
#[test]
fn url_source_debug_and_gate_denial_never_show_the_url() {
    // `.invalid` never resolves and is on no allowlist: the gate refuses it before any
    // connection or DNS lookup, so this test makes no network request.
    let url = "http://asset-gate-test.invalid:8099/private/a_tok123.png";
    for source in [
        url_source(url),
        url_source_with_persistence(url, Path::new("asset-cache-test-dir-that-is-never-created")),
    ] {
        let shown = format!("{source:?}");
        for needle in ["asset-gate-test", "invalid", "8099", "private", "a_tok123"] {
            assert!(
                !shown.contains(needle),
                "Debug of a URL asset source shows {needle:?}: {shown}"
            );
        }

        let err = fetch_bytes(&source).expect_err("an unlisted host must be refused by the gate");
        let logged = format!("{err:#}");
        assert!(
            logged.contains("network gate denied"),
            "expected the gate's denial, got: {logged}"
        );
        for needle in ["asset-gate-test", "8099", "private", "a_tok123"] {
            assert!(
                !logged.contains(needle),
                "the fetch error shows {needle:?}: {logged}"
            );
        }
    }
}
