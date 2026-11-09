use base64::Engine as _;
use fake_llm::aws_stub::{HttpStub, StubRequest, StubResponse};
use p256::ecdsa::signature::Verifier as _;
use p256::ecdsa::{Signature, SigningKey};
use serde_json::{Value, json};

use super::*;
use crate::ai::aws_creds::test_support::{Fixture, epoch, iso, run};

// A throwaway P-256 key (the public test vector from the AWS SDK's own tests); never a real login.
const TEST_KEY: &str = "-----BEGIN EC PRIVATE KEY-----\nMHcCAQEEIBMB/RwQERsVoqWRQG4zK8CnaAa5dfrpbm+9tFdBh3z4oAoGCCqGSM49\nAwEHoUQDQgAEWb1VLi1EA2hJaTz4yYuxSELvY+1GAfL+8rUTCAdiFid87Bf6GY+s\n2+1RpqDv0RpZiDIMCrZrsAh+RK9S3QCaGA==\n-----END EC PRIVATE KEY-----\n";
const SESSION_ARN: &str = "arn:aws:iam::0123456789012:user/Admin";

fn decode(part: &str) -> Vec<u8> {
    URL_SAFE_NO_PAD.decode(part).expect("base64url")
}

/// Splits a proof, checks the ES256 signature against the key's public half, returns (header, payload).
fn check_proof(proof: &str) -> (Value, Value) {
    let parts: Vec<&str> = proof.split('.').collect();
    assert_eq!(parts.len(), 3, "a JWT has three parts");
    let secret = SecretKey::from_sec1_pem(TEST_KEY).expect("test key");
    let verifying = SigningKey::from(&secret).verifying_key().clone();
    let signature = Signature::from_slice(&decode(parts[2])).expect("64-byte r||s signature");
    verifying
        .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature)
        .expect("the proof verifies under the key's public half");
    (
        serde_json::from_slice(&decode(parts[0])).expect("header json"),
        serde_json::from_slice(&decode(parts[1])).expect("payload json"),
    )
}

#[test]
fn the_dpop_proof_has_the_rfc_9449_shape_and_a_valid_signature() {
    let proof = dpop_proof(
        TEST_KEY,
        "https://us-east-1.signin.aws.amazon.com/v1/token",
        1_651_516_560,
        "jti-1",
    )
    .expect("proof");
    let (header, payload) = check_proof(&proof);
    assert_eq!(header["typ"], "dpop+jwt");
    assert_eq!(header["alg"], "ES256");
    assert_eq!(header["jwk"]["kty"], "EC");
    assert_eq!(header["jwk"]["crv"], "P-256");
    // The coordinates of the test key (same vector the SDK asserts).
    assert_eq!(
        header["jwk"]["x"],
        "Wb1VLi1EA2hJaTz4yYuxSELvY-1GAfL-8rUTCAdiFic"
    );
    assert_eq!(
        header["jwk"]["y"],
        "fOwX-hmPrNvtUaag79EaWYgyDAq2a7AIfkSvUt0Amhg"
    );
    assert_eq!(payload["htm"], "POST");
    assert_eq!(
        payload["htu"],
        "https://us-east-1.signin.aws.amazon.com/v1/token"
    );
    assert_eq!(payload["iat"], 1_651_516_560u64);
    assert_eq!(payload["jti"], "jti-1");
}

#[test]
fn an_invalid_dpop_key_is_invalid_configuration() {
    assert_eq!(
        dpop_proof("not a key", "https://x", 1, "j")
            .expect_err("bad")
            .kind(),
        "invalid_config"
    );
}

#[test]
fn the_cache_file_is_named_by_the_sha256_of_the_trimmed_session_arn() {
    let dir = std::path::Path::new("/c");
    let expected = std::path::PathBuf::from(
        "/c/36db1d138ff460920374e4c3d8e01f53f9f73537e89c88d639f68393df0e2726.json",
    );
    assert_eq!(cache_path(dir, SESSION_ARN), expected);
    assert_eq!(cache_path(dir, &format!("  {SESSION_ARN}  ")), expected);
}

fn login_doc(access_key: &str, expires: &str) -> Value {
    json!({
        "accessToken": {
            "accessKeyId": access_key, "secretAccessKey": "login-secret", "sessionToken": "login-token",
            "accountId": "012345678901", "expiresAt": expires
        },
        "tokenType": "aws_sigv4",
        "refreshToken": "login-refresh",
        "idToken": "id-token",
        "clientId": "arn:aws:signin:::devtools/same-device",
        "dpopKey": TEST_KEY,
    })
}

fn setup(doc: &Value, stub_url: Option<&str>) -> (Fixture, std::path::PathBuf) {
    let mut fx = Fixture::new();
    if let Some(url) = stub_url {
        fx = fx.set("AWS_ENDPOINT_URL_SIGNIN", url);
    }
    fx.config(&format!(
        "[profile lg]\nlogin_session = {SESSION_ARN}\nregion = us-east-2\n"
    ));
    let dir = fx.home().join(".aws/login/cache");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = cache_path(&dir, SESSION_ARN);
    std::fs::write(&path, doc.to_string()).expect("write cache");
    (fx, path)
}

fn resolve_lg(fx: &Fixture) -> Result<Creds, CredsError> {
    run(crate::ai::aws_creds::resolve(&fx.ctx(), "lg"))
}

#[test]
fn a_valid_cached_login_is_used_as_is_without_a_network_call() {
    let (fx, _) = setup(&login_doc("ASIALOGIN", &iso(3600)), None);
    let creds = resolve_lg(&fx).expect("cached login creds");
    assert_eq!(creds.access_key_id, "ASIALOGIN");
    assert_eq!(creds.secret_access_key, "login-secret");
    assert_eq!(creds.session_token.as_deref(), Some("login-token"));
}

fn refresh_stub(status: u16) -> HttpStub {
    HttpStub::start(move |req: &StubRequest| {
        if status != 200 {
            return StubResponse::json(status, "{\"error\":\"nope\"}");
        }
        if req.path() != "/v1/token" || req.header("dpop").is_none() {
            return StubResponse::json(400, "{}");
        }
        StubResponse::json(
            200,
            json!({
                "accessToken": {"accessKeyId": "ASIAREFRESHED", "secretAccessKey": "new-secret", "sessionToken": "new-token"},
                "refreshToken": "rotated",
                "expiresIn": 900,
                "tokenType": "aws_sigv4"
            })
            .to_string(),
        )
    })
}

#[test]
fn an_expiring_login_is_refreshed_with_a_dpop_proof_and_the_cache_is_rewritten() {
    let stub = refresh_stub(200);
    let (fx, path) = setup(&login_doc("ASIAOLD", &iso(60)), Some(&stub.url()));
    let creds = resolve_lg(&fx).expect("refreshed");
    assert_eq!(creds.access_key_id, "ASIAREFRESHED");
    assert_eq!(
        creds.expires_at,
        Some(crate::ai::aws_creds::test_support::now() + Duration::from_secs(900))
    );

    let req = &stub.requests()[0];
    assert_eq!(req.method, "POST");
    let body: Value = serde_json::from_str(&req.body_text()).expect("json body");
    assert_eq!(body["grantType"], "refresh_token");
    assert_eq!(body["refreshToken"], "login-refresh");
    assert_eq!(body["clientId"], "arn:aws:signin:::devtools/same-device");

    // The proof names this very request: POST, the full URL, the frozen time.
    let (_, payload) = check_proof(req.header("dpop").expect("dpop header"));
    assert_eq!(payload["htm"], "POST");
    assert_eq!(payload["htu"], format!("{}/v1/token", stub.url()));
    assert_eq!(payload["iat"], epoch(0));

    let cached: Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("read")).expect("json");
    assert_eq!(cached["accessToken"]["accessKeyId"], "ASIAREFRESHED");
    assert_eq!(cached["accessToken"]["accountId"], "012345678901");
    assert_eq!(cached["accessToken"]["expiresAt"], iso(900));
    assert_eq!(cached["refreshToken"], "rotated");
    assert_eq!(cached["dpopKey"], TEST_KEY);
    assert_eq!(cached["idToken"], "id-token");
}

#[test]
fn a_rejected_refresh_keeps_a_still_valid_login_and_expires_an_expired_one() {
    let stub = refresh_stub(401);
    let (fx, _) = setup(&login_doc("ASIASTILL", &iso(120)), Some(&stub.url()));
    assert_eq!(
        resolve_lg(&fx).expect("still valid").access_key_id,
        "ASIASTILL"
    );

    let (fx, _) = setup(&login_doc("ASIADEAD", &iso(-10)), Some(&stub.url()));
    let err = resolve_lg(&fx).expect_err("expired and unrefreshable");
    assert_eq!(err.kind(), "expired");
    assert!(err.detail().contains("aws login"));
}

#[test]
fn a_missing_cache_entry_asks_for_aws_login() {
    let fx = Fixture::new();
    fx.config(&format!("[profile lg]\nlogin_session = {SESSION_ARN}\n"));
    assert_eq!(resolve_lg(&fx).expect_err("no cache").kind(), "expired");
}

#[test]
fn the_signin_endpoint_follows_the_partition() {
    let fx = Fixture::new();
    let ctx = fx.ctx();
    assert_eq!(
        signin_endpoint(&ctx, "us-east-1"),
        "https://us-east-1.signin.aws.amazon.com"
    );
    assert_eq!(
        signin_endpoint(&ctx, "cn-north-1"),
        "https://cn-north-1.signin.amazonaws.cn"
    );
    assert_eq!(
        signin_endpoint(&ctx, "us-gov-west-1"),
        "https://us-gov-west-1.signin.amazonaws-us-gov.com"
    );
}
