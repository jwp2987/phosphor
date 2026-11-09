use fake_llm::aws_stub::{HttpStub, StubResponse};

use super::*;
use crate::ai::aws_creds::test_support::{Fixture, run};

fn creds_json(key: &str, secret: &str, token_field: &str, token: &str, expiry: &str) -> String {
    format!(
        "{{\"Code\":\"Success\",\"AccessKeyId\":\"{key}\",\"SecretAccessKey\":\"{secret}\",\
         \"{token_field}\":\"{token}\",\"Expiration\":\"{expiry}\"}}"
    )
}

#[test]
fn parses_imds_ecs_and_process_documents() {
    let creds = parse_json_credentials(&creds_json(
        "AK",
        "SK",
        "Token",
        "TOK",
        "2023-11-14T23:13:20Z",
    ))
    .expect("ok");
    assert_eq!(creds.access_key_id, "AK");
    assert_eq!(creds.session_token.as_deref(), Some("TOK"));
    assert!(creds.expires_at.is_some());

    // `SessionToken` (credential_process) and no expiry.
    let creds = parse_json_credentials(
        r#"{"Version":1,"AccessKeyId":"A","SecretAccessKey":"S","SessionToken":"T"}"#,
    )
    .expect("ok");
    assert_eq!(creds.session_token.as_deref(), Some("T"));
    assert_eq!(creds.expires_at, None);

    assert!(parse_json_credentials("not json").is_err());
    assert!(parse_json_credentials(r#"{"AccessKeyId":"A"}"#).is_err());
    let err = parse_json_credentials(r#"{"Code":"AssumeRoleUnauthorizedAccess","Message":"nope"}"#)
        .expect_err("error doc");
    assert!(err.detail().contains("AssumeRoleUnauthorizedAccess"));
}

#[test]
fn container_credentials_use_the_full_uri_and_the_authorization_token() {
    let stub = HttpStub::start(|req| {
        if req.header("authorization") == Some("Bearer ecs-token") {
            StubResponse::json(
                200,
                creds_json(
                    "ASIAECS",
                    "ecssecret",
                    "Token",
                    "ecstoken",
                    "2030-01-01T00:00:00Z",
                ),
            )
        } else {
            StubResponse::text(403, "no")
        }
    });
    let fx = Fixture::new()
        .set(
            "AWS_CONTAINER_CREDENTIALS_FULL_URI",
            &format!("{}/creds", stub.url()),
        )
        .set("AWS_CONTAINER_AUTHORIZATION_TOKEN", "Bearer ecs-token");
    let creds = run(container(&fx.ctx())).expect("ecs creds");
    assert_eq!(creds.access_key_id, "ASIAECS");
    assert_eq!(stub.requests()[0].path(), "/creds");
}

#[test]
fn container_authorization_token_file_is_read_and_trimmed() {
    let stub = HttpStub::start(|req| {
        if req.header("authorization") == Some("file-token") {
            StubResponse::json(
                200,
                creds_json("ASIAF", "s", "Token", "t", "2030-01-01T00:00:00Z"),
            )
        } else {
            StubResponse::text(403, "no")
        }
    });
    let fx = Fixture::new();
    let token_path = fx.write("ecs-token", "file-token\n");
    let fx = fx
        .set("AWS_CONTAINER_CREDENTIALS_FULL_URI", &stub.url())
        .set(
            "AWS_CONTAINER_AUTHORIZATION_TOKEN_FILE",
            &token_path.to_string_lossy(),
        )
        // The file wins over the literal variable.
        .set("AWS_CONTAINER_AUTHORIZATION_TOKEN", "ignored");
    assert_eq!(
        run(container(&fx.ctx())).expect("ok").access_key_id,
        "ASIAF"
    );
}

#[test]
fn container_without_uris_is_not_loaded_and_a_bad_status_is_a_provider_error() {
    let fx = Fixture::new();
    assert_eq!(
        run(container(&fx.ctx())).expect_err("unset").kind(),
        "not_loaded"
    );

    let stub = HttpStub::start(|_| StubResponse::text(500, "boom"));
    let fx = fx.set("AWS_CONTAINER_CREDENTIALS_FULL_URI", &stub.url());
    assert_eq!(
        run(container(&fx.ctx())).expect_err("500").kind(),
        "provider"
    );
}

fn imds_stub(require_token: bool) -> HttpStub {
    HttpStub::start(move |req| match (req.method.as_str(), req.path()) {
        ("PUT", "/latest/api/token") => {
            if req.header("x-aws-ec2-metadata-token-ttl-seconds").is_none() {
                return StubResponse::text(400, "missing ttl");
            }
            if require_token {
                StubResponse::text(200, "imds-session-token")
            } else {
                StubResponse::text(405, "no v2")
            }
        }
        ("GET", path) => {
            if require_token && req.header("x-aws-ec2-metadata-token") != Some("imds-session-token")
            {
                return StubResponse::text(401, "token required");
            }
            match path {
                "/latest/meta-data/iam/security-credentials/" => {
                    StubResponse::text(200, "my-role\n")
                }
                "/latest/meta-data/iam/security-credentials/my-role" => StubResponse::text(
                    200,
                    creds_json(
                        "ASIAIMDS",
                        "imdssecret",
                        "Token",
                        "imdstoken",
                        "2030-01-01T00:00:00Z",
                    ),
                ),
                _ => StubResponse::text(404, "nope"),
            }
        }
        _ => StubResponse::text(404, "nope"),
    })
}

#[test]
fn imdsv2_fetches_a_session_token_then_the_role_credentials() {
    let stub = imds_stub(true);
    let fx = Fixture::new()
        .unset("AWS_EC2_METADATA_DISABLED")
        .set("AWS_EC2_METADATA_SERVICE_ENDPOINT", &stub.url());
    let creds = run(imds(&fx.ctx())).expect("imds creds");
    assert_eq!(creds.access_key_id, "ASIAIMDS");
    assert_eq!(creds.session_token.as_deref(), Some("imdstoken"));
    let methods: Vec<_> = stub.requests().iter().map(|r| r.method.clone()).collect();
    assert_eq!(methods, ["PUT", "GET", "GET"]);
}

#[test]
fn imds_falls_back_to_v1_only_when_the_token_endpoint_is_refused() {
    let stub = imds_stub(false);
    let fx = Fixture::new()
        .unset("AWS_EC2_METADATA_DISABLED")
        .set("AWS_EC2_METADATA_SERVICE_ENDPOINT", &stub.url());
    assert_eq!(
        run(imds(&fx.ctx())).expect("v1 fallback").access_key_id,
        "ASIAIMDS"
    );
    assert!(
        stub.requests()[1]
            .header("x-aws-ec2-metadata-token")
            .is_none()
    );
}

#[test]
fn imds_disabled_or_unreachable_is_not_loaded() {
    let fx = Fixture::new().set("AWS_EC2_METADATA_DISABLED", "TRUE");
    assert_eq!(
        run(imds(&fx.ctx())).expect_err("disabled").kind(),
        "not_loaded"
    );

    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        l.local_addr().expect("addr").port()
    };
    let fx = Fixture::new().unset("AWS_EC2_METADATA_DISABLED").set(
        "AWS_EC2_METADATA_SERVICE_ENDPOINT",
        &format!("http://127.0.0.1:{port}"),
    );
    assert_eq!(
        run(imds(&fx.ctx())).expect_err("unreachable").kind(),
        "not_loaded"
    );
}

#[test]
fn imds_without_an_instance_profile_is_not_loaded() {
    let stub = HttpStub::start(|req| {
        if req.method == "PUT" {
            StubResponse::text(200, "t")
        } else {
            StubResponse::text(404, "no role")
        }
    });
    let fx = Fixture::new()
        .unset("AWS_EC2_METADATA_DISABLED")
        .set("AWS_EC2_METADATA_SERVICE_ENDPOINT", &stub.url());
    assert_eq!(
        run(imds(&fx.ctx())).expect_err("no role").kind(),
        "not_loaded"
    );
}
