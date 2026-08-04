use super::*;

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
