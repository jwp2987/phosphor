use std::sync::{Arc, Mutex};

use fake_llm::aws_stub::{HttpStub, StubResponse};
use serde_json::{Value, json};

use super::*;
use crate::ai::aws_creds::test_support::{Fixture, epoch, iso, now, run};

const CONFIG: &str = "[profile sso]\nsso_session = my-sso\nsso_account_id = 111122223333\nsso_role_name = Dev\nregion = us-east-1\n\n[sso-session my-sso]\nsso_start_url = https://corp.awsapps.com/start\nsso_region = us-east-2\n";

fn cache_doc(access: &str, expires: &str, refresh: Option<&str>) -> Value {
    let mut doc = json!({
        "startUrl": "https://corp.awsapps.com/start",
        "region": "us-east-2",
        "accessToken": access,
        "expiresAt": expires,
        "clientId": "client-id",
        "clientSecret": "client-secret",
        "registrationExpiresAt": "2030-01-01T00:00:00Z",
    });
    if let Some(refresh) = refresh {
        doc["refreshToken"] = json!(refresh);
    }
    doc
}

fn write_cache(fx: &Fixture, key: &str, doc: &Value) -> std::path::PathBuf {
    let path = cache_path(fx.home(), key);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(&path, doc.to_string()).expect("write cache");
    path
}

/// Portal + OIDC on one stub. `portal_token` is the bearer the portal accepts.
struct Sso {
    stub: HttpStub,
    portal_seen: Arc<Mutex<Vec<String>>>,
}

fn sso_stub(portal_token: &'static str, oidc_status: u16) -> Sso {
    let portal_seen = Arc::new(Mutex::new(Vec::new()));
    let seen = portal_seen.clone();
    let stub = HttpStub::start(move |req| match req.path() {
        "/token" => {
            if oidc_status != 200 {
                return StubResponse::json(oidc_status, "{\"error\":\"invalid_grant\"}");
            }
            StubResponse::json(
                200,
                json!({"accessToken": "new-access", "expiresIn": 28800, "refreshToken": "rotated-refresh", "tokenType": "Bearer"})
                    .to_string(),
            )
        }
        "/federation/credentials" => {
            let bearer = req
                .header("x-amz-sso_bearer_token")
                .unwrap_or("")
                .to_string();
            seen.lock().expect("log").push(bearer.clone());
            if bearer != portal_token {
                return StubResponse::json(
                    401,
                    "{\"message\":\"Session token not found or invalid\"}",
                );
            }
            StubResponse::json(
                200,
                json!({"roleCredentials": {
                    "accessKeyId": "ASIASSO", "secretAccessKey": "ssosecret", "sessionToken": "ssotoken",
                    "expiration": (epoch(3600) * 1000)
                }})
                .to_string(),
            )
        }
        _ => StubResponse::text(404, "?"),
    });
    Sso { stub, portal_seen }
}

fn fixture(sso: &Sso) -> Fixture {
    let fx = Fixture::new()
        .set("AWS_ENDPOINT_URL_SSO", &sso.stub.url())
        .set("AWS_ENDPOINT_URL_SSO_OIDC", &sso.stub.url());
    fx.config(CONFIG);
    fx
}

fn resolve_sso(fx: &Fixture) -> Result<Creds, CredsError> {
    run(crate::ai::aws_creds::resolve(&fx.ctx(), "sso"))
}

#[test]
fn cache_file_is_named_by_the_sha1_of_the_session_name_or_start_url() {
    let home = std::path::Path::new("/h");
    assert_eq!(
        cache_path(home, "my-sso"),
        std::path::PathBuf::from("/h/.aws/sso/cache/0ad374308c5a4e22f723adf10145eafad7c4031c.json")
    );
    assert_eq!(
        cache_path(home, "https://corp.awsapps.com/start"),
        std::path::PathBuf::from("/h/.aws/sso/cache/a51746d4aa793ae07b25e90c3a5db96fc7832bd7.json")
    );
}

#[test]
fn a_valid_cached_token_is_exchanged_at_the_portal_without_refreshing() {
    let sso = sso_stub("cached-access", 200);
    let fx = fixture(&sso);
    write_cache(
        &fx,
        "my-sso",
        &cache_doc("cached-access", &iso(3600), Some("r")),
    );
    let creds = resolve_sso(&fx).expect("sso creds");
    assert_eq!(creds.access_key_id, "ASIASSO");
    assert_eq!(creds.session_token.as_deref(), Some("ssotoken"));
    assert_eq!(creds.expires_at, Some(now() + Duration::from_secs(3600)));

    let seen = sso.stub.requests();
    assert_eq!(seen.len(), 1, "no OIDC call for a token with time left");
    assert_eq!(seen[0].method, "GET");
    assert!(seen[0].target.contains("account_id=111122223333"));
    assert!(seen[0].target.contains("role_name=Dev"));
}

#[test]
fn the_legacy_start_url_form_uses_the_start_url_cache_key() {
    let sso = sso_stub("legacy-access", 200);
    let fx = fixture(&sso);
    fx.config("[profile sso]\nsso_start_url = https://corp.awsapps.com/start\nsso_region = us-east-2\nsso_account_id = 111122223333\nsso_role_name = Dev\n");
    write_cache(
        &fx,
        "https://corp.awsapps.com/start",
        &cache_doc("legacy-access", &iso(3600), None),
    );
    assert_eq!(
        resolve_sso(&fx).expect("legacy sso").access_key_id,
        "ASIASSO"
    );
}

#[test]
fn an_expired_token_is_refreshed_and_the_cache_is_rewritten_with_other_fields_kept() {
    let sso = sso_stub("new-access", 200);
    let fx = fixture(&sso);
    let path = write_cache(
        &fx,
        "my-sso",
        &cache_doc("old-access", &iso(-60), Some("old-refresh")),
    );
    let creds = resolve_sso(&fx).expect("refresh then portal");
    assert_eq!(creds.access_key_id, "ASIASSO");
    // The portal saw the refreshed token, not the stale one.
    assert_eq!(
        *sso.portal_seen.lock().expect("log"),
        vec!["new-access".to_string()]
    );

    let oidc = &sso.stub.requests()[0];
    assert_eq!(oidc.method, "POST");
    assert_eq!(oidc.path(), "/token");
    let body: Value = serde_json::from_str(&oidc.body_text()).expect("json body");
    assert_eq!(body["grantType"], "refresh_token");
    assert_eq!(body["refreshToken"], "old-refresh");
    assert_eq!(body["clientId"], "client-id");
    assert_eq!(body["clientSecret"], "client-secret");

    let cached: Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("read")).expect("json");
    assert_eq!(cached["accessToken"], "new-access");
    assert_eq!(cached["refreshToken"], "rotated-refresh");
    assert_eq!(cached["expiresAt"], iso(28800));
    assert_eq!(cached["clientId"], "client-id");
    assert_eq!(cached["startUrl"], "https://corp.awsapps.com/start");
    assert_eq!(cached["registrationExpiresAt"], "2030-01-01T00:00:00Z");
}

#[test]
fn an_expired_token_with_no_refresh_token_asks_for_aws_sso_login() {
    let sso = sso_stub("x", 200);
    let fx = fixture(&sso);
    write_cache(&fx, "my-sso", &cache_doc("old", &iso(-60), None));
    let err = resolve_sso(&fx).expect_err("expired");
    assert_eq!(err.kind(), "expired");
    assert!(err.detail().contains("aws sso login"));
    assert!(
        sso.stub.requests().is_empty(),
        "nothing to refresh with, nothing to call"
    );
}

#[test]
fn a_rejected_refresh_of_an_expired_token_is_expired() {
    let sso = sso_stub("x", 400);
    let fx = fixture(&sso);
    write_cache(&fx, "my-sso", &cache_doc("old", &iso(-60), Some("spent")));
    assert_eq!(
        resolve_sso(&fx).expect_err("spent refresh").kind(),
        "expired"
    );
}

#[test]
fn a_failed_refresh_of_a_token_that_still_has_time_falls_back_to_it() {
    let sso = sso_stub("soon-access", 500);
    let fx = fixture(&sso);
    // Inside the 5-minute refresh window but not yet expired.
    write_cache(
        &fx,
        "my-sso",
        &cache_doc("soon-access", &iso(120), Some("r")),
    );
    assert_eq!(
        resolve_sso(&fx).expect("still valid").access_key_id,
        "ASIASSO"
    );
}

#[test]
fn a_missing_cache_and_a_portal_401_both_ask_for_aws_sso_login() {
    let sso = sso_stub("the-right-token", 200);
    let fx = fixture(&sso);
    assert_eq!(resolve_sso(&fx).expect_err("no cache").kind(), "expired");

    write_cache(&fx, "my-sso", &cache_doc("revoked", &iso(3600), None));
    let err = resolve_sso(&fx).expect_err("portal 401");
    assert_eq!(err.kind(), "expired");
    assert!(err.detail().contains("aws sso login"));
}

#[test]
fn incomplete_sso_configuration_is_invalid_not_a_crash() {
    let fx = Fixture::new();
    fx.config("[profile sso]\nsso_session = nope\nsso_account_id = 1\nsso_role_name = R\n");
    assert_eq!(
        resolve_sso(&fx).expect_err("no session section").kind(),
        "invalid_config"
    );

    fx.config(
        "[profile sso]\nsso_start_url = https://x\nsso_region = us-east-1\nsso_role_name = R\n",
    );
    assert_eq!(
        resolve_sso(&fx).expect_err("no account").kind(),
        "invalid_config"
    );
}
