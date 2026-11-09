use std::io::{Read, Write};
use std::net::TcpStream;

use super::*;

fn raw_request(stub: &HttpStub, request: &str) -> String {
    let addr = stub.url().trim_start_matches("http://").to_string();
    let mut stream = TcpStream::connect(addr).expect("connect to the stub");
    stream.write_all(request.as_bytes()).expect("write");
    let mut out = String::new();
    stream.read_to_string(&mut out).expect("read");
    out
}

#[test]
fn the_stub_records_requests_and_answers_with_the_handler() {
    let stub =
        HttpStub::start(|req| StubResponse::json(200, format!("{{\"path\":\"{}\"}}", req.path())));
    let body = "a=1&b=two+words";
    let reply = raw_request(
        &stub,
        &format!(
            "POST /x/y HTTP/1.1\r\nHost: stub\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        ),
    );
    assert!(reply.starts_with("HTTP/1.1 200"));
    assert!(reply.ends_with("{\"path\":\"/x/y\"}"));
    let seen = stub.requests();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].method, "POST");
    assert_eq!(seen[0].form_value("b").as_deref(), Some("two words"));
    assert_eq!(seen[0].header("HOST"), Some("stub"));
}

// The AWS SigV4 suite's `get-vanilla` request, signed with the published key and frozen time.
fn suite_request(signature: &str, token: Option<&str>) -> StubRequest {
    let mut headers = vec![
        ("host".to_string(), "example.amazonaws.com".to_string()),
        ("x-amz-date".to_string(), "20150830T123600Z".to_string()),
    ];
    let mut signed = "host;x-amz-date".to_string();
    if let Some(token) = token {
        headers.push(("x-amz-security-token".to_string(), token.to_string()));
        signed.push_str(";x-amz-security-token");
    }
    headers.push((
        "authorization".to_string(),
        format!(
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/service/aws4_request, SignedHeaders={signed}, Signature={signature}"
        ),
    ));
    StubRequest {
        method: "GET".to_string(),
        target: "/".to_string(),
        headers,
        body: Vec::new(),
    }
}

fn suite_expectation(token: Option<&str>) -> SigV4Expectation {
    SigV4Expectation {
        access_key: "AKIDEXAMPLE".to_string(),
        secret_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".to_string(),
        session_token: token.map(str::to_string),
        region: "us-east-1".to_string(),
        service: "service".to_string(),
        amz_date: Some("20150830T123600Z".to_string()),
    }
}

#[test]
fn the_verifier_accepts_the_aws_suite_vector() {
    let request = suite_request(
        "5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31",
        None,
    );
    assert_eq!(verify_sigv4(&request, &suite_expectation(None)), Ok(()));
}

#[test]
fn the_verifier_accepts_the_aws_suite_session_token_vector() {
    let token = "6e86291e8372ff2a2260956d9b8aae1d763fbf315fa00fa31553b73ebf194267";
    let request = suite_request(
        "07ec1639c89043aa0e3e2de82b96708f198cceab042d4a97044c66dd9f74e7f8",
        Some(token),
    );
    assert_eq!(
        verify_sigv4(&request, &suite_expectation(Some(token))),
        Ok(())
    );
}

#[test]
fn the_verifier_rejects_a_wrong_signature_scope_key_and_time() {
    let good = "5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31";
    let bad_sig = suite_request(&good.replace('5', "6"), None);
    assert!(verify_sigv4(&bad_sig, &suite_expectation(None)).is_err());

    let mut wrong_region = suite_expectation(None);
    wrong_region.region = "eu-west-1".to_string();
    assert!(verify_sigv4(&suite_request(good, None), &wrong_region).is_err());

    let mut wrong_secret = suite_expectation(None);
    wrong_secret.secret_key = "not-the-secret".to_string();
    assert!(verify_sigv4(&suite_request(good, None), &wrong_secret).is_err());

    let mut wrong_time = suite_expectation(None);
    wrong_time.amz_date = Some("20150830T123601Z".to_string());
    assert!(verify_sigv4(&suite_request(good, None), &wrong_time).is_err());

    // A token that was not expected, and one that was expected but is absent.
    assert!(verify_sigv4(&suite_request(good, Some("t")), &suite_expectation(None)).is_err());
    assert!(verify_sigv4(&suite_request(good, None), &suite_expectation(Some("t"))).is_err());
}

#[test]
fn the_bedrock_stub_refuses_an_unsigned_converse_request() {
    let bedrock = BedrockStub::start(suite_expectation(None));
    let reply = raw_request(
        &bedrock.stub,
        "POST /model/some.model-v1:0/converse HTTP/1.1\r\nHost: stub\r\nContent-Length: 2\r\n\r\n{}",
    );
    assert!(reply.starts_with("HTTP/1.1 403"));
    assert_eq!(bedrock.verdicts().len(), 1);
    assert!(bedrock.verdicts()[0].is_err());
}
