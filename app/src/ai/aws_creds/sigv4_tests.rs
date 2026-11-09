use std::time::{Duration, UNIX_EPOCH};

use super::*;

// 2015-08-30T12:36:00Z, the AWS SigV4 test suite's frozen time.
fn suite_time() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_440_938_160)
}

fn suite_creds(token: Option<&str>) -> Creds {
    Creds::new(
        "AKIDEXAMPLE",
        "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
        token.map(str::to_string),
        None,
    )
}

#[test]
fn matches_the_aws_suite_get_vanilla_vector() {
    let creds = suite_creds(None);
    let signed = sign(&SigningInput {
        method: "GET",
        host: "example.amazonaws.com",
        path: "/",
        headers: &[],
        body: b"",
        region: "us-east-1",
        service: "service",
        creds: &creds,
        time: suite_time(),
    });
    assert_eq!(signed.amz_date, "20150830T123600Z");
    assert_eq!(
        signed.authorization,
        "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/service/aws4_request, \
         SignedHeaders=host;x-amz-date, \
         Signature=5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31"
    );
    assert_eq!(signed.security_token, None);
}

#[test]
fn matches_the_aws_suite_session_token_vector() {
    let creds = suite_creds(Some(
        "6e86291e8372ff2a2260956d9b8aae1d763fbf315fa00fa31553b73ebf194267",
    ));
    let signed = sign(&SigningInput {
        method: "GET",
        host: "example.amazonaws.com",
        path: "/",
        headers: &[],
        body: b"",
        region: "us-east-1",
        service: "service",
        creds: &creds,
        time: suite_time(),
    });
    assert_eq!(
        signed.authorization,
        "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/service/aws4_request, \
         SignedHeaders=host;x-amz-date;x-amz-security-token, \
         Signature=07ec1639c89043aa0e3e2de82b96708f198cceab042d4a97044c66dd9f74e7f8"
    );
    assert_eq!(
        signed.security_token.as_deref(),
        Some("6e86291e8372ff2a2260956d9b8aae1d763fbf315fa00fa31553b73ebf194267")
    );
}

#[test]
fn matches_the_aws_suite_post_form_vector() {
    // post-x-www-form-urlencoded: the suite signs content-length, content-type and
    // x-amz-content-sha256 as well.
    let creds = suite_creds(None);
    let signed = sign(&SigningInput {
        method: "POST",
        host: "example.amazonaws.com",
        path: "/",
        headers: &[
            ("content-length", "13"),
            ("Content-Type", "application/x-www-form-urlencoded"),
            (
                "x-amz-content-sha256",
                "9095672bbd1f56dfc5b65f3e153adc8731a4a654192329106275f4c7b24d0b6e",
            ),
        ],
        body: b"Param1=value1",
        region: "us-east-1",
        service: "service",
        creds: &creds,
        time: suite_time(),
    });
    assert_eq!(
        signed.authorization,
        "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/service/aws4_request, \
         SignedHeaders=content-length;content-type;host;x-amz-content-sha256;x-amz-date, \
         Signature=d3875051da38690788ef43de4db0d8f280229d82040bfac253562e56c3f20e0b"
    );
}

#[test]
fn path_is_encoded_a_second_time_for_the_canonical_request() {
    assert_eq!(
        uri_encode("/model/a:b/converse", false),
        "/model/a%3Ab/converse"
    );
    // A path already sent as `a%3Ab` canonicalises to `a%253Ab`.
    assert_eq!(
        uri_encode("/model/a%3Ab/converse", false),
        "/model/a%253Ab/converse"
    );
    assert_eq!(uri_encode("a b/c", true), "a%20b%2Fc");
    assert_eq!(uri_encode("-_.~AZaz09", true), "-_.~AZaz09");
}

#[test]
fn creds_debug_never_prints_key_material() {
    let creds = suite_creds(Some("tok-secret"));
    let shown = format!("{creds:?}");
    assert!(!shown.contains("AKIDEXAMPLE"));
    assert!(!shown.contains("wJalr"));
    assert!(!shown.contains("tok-secret"));
}
