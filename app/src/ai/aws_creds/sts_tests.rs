use fake_llm::aws_stub::{HttpStub, SigV4Expectation, StubResponse, verify_sigv4};

use super::*;
use crate::ai::aws_creds::sigv4::amz_date;
use crate::ai::aws_creds::test_support::{Fixture, epoch, iso, now, run, sts_json, sts_xml};

fn fixture_for(stub: &HttpStub) -> Fixture {
    Fixture::new().set("AWS_ENDPOINT_URL_STS", &stub.url())
}

#[test]
fn web_identity_is_unsigned_and_sends_the_documented_form_fields() {
    let stub = HttpStub::start(|_| {
        StubResponse::json(
            200,
            sts_json(
                "AssumeRoleWithWebIdentity",
                "ASIAWEB",
                "websecret",
                "webtoken",
                epoch(3600),
            ),
        )
    });
    let fx = fixture_for(&stub);
    let creds = run(assume_role_with_web_identity(
        &fx.ctx(),
        "us-west-2",
        "arn:aws:iam::123456789012:role/Oidc",
        "Oz_Run_abc",
        "jwt-goes-here",
        Some(1800),
    ))
    .expect("web identity succeeds");

    assert_eq!(creds.access_key_id, "ASIAWEB");
    assert_eq!(creds.secret_access_key, "websecret");
    assert_eq!(creds.session_token.as_deref(), Some("webtoken"));
    assert_eq!(creds.expires_at, Some(now() + Duration::from_secs(3600)));

    let seen = stub.requests();
    assert_eq!(seen.len(), 1);
    let req = &seen[0];
    assert_eq!(req.method, "POST");
    assert_eq!(req.path(), "/");
    assert_eq!(
        req.form_value("Action").as_deref(),
        Some("AssumeRoleWithWebIdentity")
    );
    assert_eq!(req.form_value("Version").as_deref(), Some("2011-06-15"));
    assert_eq!(
        req.form_value("RoleArn").as_deref(),
        Some("arn:aws:iam::123456789012:role/Oidc")
    );
    assert_eq!(
        req.form_value("RoleSessionName").as_deref(),
        Some("Oz_Run_abc")
    );
    assert_eq!(
        req.form_value("WebIdentityToken").as_deref(),
        Some("jwt-goes-here")
    );
    assert_eq!(req.form_value("DurationSeconds").as_deref(), Some("1800"));
    assert_eq!(req.header("accept"), Some("application/json"));
    assert!(
        req.header("authorization").is_none(),
        "the web identity call must be unsigned"
    );
}

#[test]
fn web_identity_accepts_the_xml_fallback() {
    let stub = HttpStub::start(|_| {
        StubResponse::xml(
            200,
            sts_xml(
                "AssumeRoleWithWebIdentity",
                "ASIAXML",
                "xmlsecret&amp;more",
                "xmltoken",
                &iso(900),
            ),
        )
    });
    let fx = fixture_for(&stub);
    let creds = run(assume_role_with_web_identity(
        &fx.ctx(),
        "us-east-1",
        "arn:r",
        "s",
        "t",
        None,
    ))
    .expect("xml answer parses");
    assert_eq!(creds.access_key_id, "ASIAXML");
    assert_eq!(creds.secret_access_key, "xmlsecret&more");
    assert_eq!(creds.expires_at, Some(now() + Duration::from_secs(900)));
}

#[test]
fn web_identity_errors_carry_the_code_and_message_but_not_the_body() {
    let stub = HttpStub::start(|_| {
        StubResponse::xml(
            403,
            "<ErrorResponse><Error><Type>Sender</Type><Code>AccessDenied</Code>\
             <Message>Not authorized to perform sts:AssumeRoleWithWebIdentity</Message></Error>\
             <RequestId>do-not-leak-me</RequestId></ErrorResponse>",
        )
    });
    let fx = fixture_for(&stub);
    let err = run(assume_role_with_web_identity(
        &fx.ctx(),
        "us-east-1",
        "arn:r",
        "s",
        "t",
        None,
    ))
    .expect_err("403 fails");
    assert_eq!(err.kind(), "provider");
    assert!(err.detail().contains("AccessDenied"));
    assert!(
        err.detail()
            .contains("Not authorized to perform sts:AssumeRoleWithWebIdentity")
    );
    assert!(!err.detail().contains("do-not-leak-me"));
}

#[test]
fn expired_token_errors_are_expired_and_bare_5xx_names_the_status() {
    let expired = HttpStub::start(|_| {
        StubResponse::json(
            400,
            "{\"Error\":{\"Code\":\"ExpiredTokenException\",\"Message\":\"gone\"}}",
        )
    });
    let err = run(assume_role_with_web_identity(
        &fixture_for(&expired).ctx(),
        "us-east-1",
        "r",
        "s",
        "t",
        None,
    ))
    .expect_err("fails");
    assert_eq!(err.kind(), "expired");

    let down = HttpStub::start(|_| StubResponse::text(503, "upstream said secret-body"));
    let err = run(assume_role_with_web_identity(
        &fixture_for(&down).ctx(),
        "us-east-1",
        "r",
        "s",
        "t",
        None,
    ))
    .expect_err("fails");
    assert_eq!(err.kind(), "provider");
    assert!(err.detail().contains("HTTP 503"));
    assert!(!err.detail().contains("secret-body"));
}

#[test]
fn an_unreachable_endpoint_is_a_provider_error() {
    // Bind then drop to get a port nothing listens on.
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        l.local_addr().expect("addr").port()
    };
    let fx = Fixture::new().set("AWS_ENDPOINT_URL_STS", &format!("http://127.0.0.1:{port}"));
    let err = run(assume_role_with_web_identity(
        &fx.ctx(),
        "us-east-1",
        "r",
        "s",
        "t",
        None,
    ))
    .expect_err("connection refused");
    assert!(matches!(
        err,
        CredsError::Provider(_) | CredsError::Timeout(_)
    ));
}

fn base_creds() -> Creds {
    Creds::new(
        "AKIABASE",
        "base-secret-key",
        Some("base-session".to_string()),
        None,
    )
}

#[test]
fn assume_role_is_sigv4_signed_with_the_base_credentials() {
    let expectation = SigV4Expectation {
        access_key: "AKIABASE".to_string(),
        secret_key: "base-secret-key".to_string(),
        session_token: Some("base-session".to_string()),
        region: "eu-central-1".to_string(),
        service: "sts".to_string(),
        amz_date: Some(amz_date(now())),
    };
    let stub = HttpStub::start(move |req| match verify_sigv4(req, &expectation) {
        Ok(()) => StubResponse::json(
            200,
            sts_json(
                "AssumeRole",
                "ASIAROLE",
                "rolesecret",
                "roletoken",
                epoch(3000),
            ),
        ),
        Err(reason) => StubResponse::text(403, reason),
    });
    let fx = fixture_for(&stub);
    let creds = run(assume_role(
        &fx.ctx(),
        &base_creds(),
        &AssumeRoleRequest {
            role_arn: "arn:aws:iam::123456789012:role/Admin",
            session_name: "my session",
            external_id: Some("ext/id 1"),
            duration_secs: Some(3000),
            region: "eu-central-1",
        },
    ))
    .expect("the stub verified the signature");
    assert_eq!(creds.access_key_id, "ASIAROLE");
    assert_eq!(creds.session_token.as_deref(), Some("roletoken"));

    let req = &stub.requests()[0];
    assert_eq!(req.form_value("Action").as_deref(), Some("AssumeRole"));
    assert_eq!(
        req.form_value("RoleArn").as_deref(),
        Some("arn:aws:iam::123456789012:role/Admin")
    );
    assert_eq!(
        req.form_value("RoleSessionName").as_deref(),
        Some("my session")
    );
    assert_eq!(req.form_value("ExternalId").as_deref(), Some("ext/id 1"));
    assert_eq!(req.form_value("DurationSeconds").as_deref(), Some("3000"));
}

#[test]
fn assume_role_with_the_wrong_base_secret_is_rejected_by_the_verifier() {
    let expectation = SigV4Expectation {
        access_key: "AKIABASE".to_string(),
        secret_key: "a-different-secret".to_string(),
        session_token: None,
        region: "us-east-1".to_string(),
        service: "sts".to_string(),
        amz_date: None,
    };
    let stub = HttpStub::start(move |req| match verify_sigv4(req, &expectation) {
        Ok(()) => StubResponse::json(200, "{}"),
        Err(_) => StubResponse::xml(
            403,
            "<ErrorResponse><Error><Code>SignatureDoesNotMatch</Code><Message>bad</Message></Error></ErrorResponse>",
        ),
    });
    let err = run(assume_role(
        &fixture_for(&stub).ctx(),
        &Creds::new("AKIABASE", "base-secret-key", None, None),
        &AssumeRoleRequest {
            role_arn: "arn:r",
            session_name: "s",
            external_id: None,
            duration_secs: None,
            region: "us-east-1",
        },
    ))
    .expect_err("signature rejected");
    assert!(err.detail().contains("SignatureDoesNotMatch"));
}

#[test]
fn web_identity_from_env_reads_the_token_file_and_skips_when_unset() {
    let stub = HttpStub::start(|_| {
        StubResponse::json(
            200,
            sts_json("AssumeRoleWithWebIdentity", "ASIAENV", "s", "t", epoch(600)),
        )
    });
    let fx = fixture_for(&stub);
    assert_eq!(run(web_identity_from_env(&fx.ctx())), Ok(None));

    let token_path = fx.write("token", "  file-jwt\n");
    let fx = fx
        .set("AWS_WEB_IDENTITY_TOKEN_FILE", &token_path.to_string_lossy())
        .set("AWS_ROLE_ARN", "arn:aws:iam::1:role/Env")
        .set("AWS_ROLE_SESSION_NAME", "from-env")
        .set("AWS_REGION", "ap-southeast-2");
    let creds = run(web_identity_from_env(&fx.ctx()))
        .expect("ok")
        .expect("some");
    assert_eq!(creds.access_key_id, "ASIAENV");
    let req = &stub.requests()[0];
    assert_eq!(
        req.form_value("WebIdentityToken").as_deref(),
        Some("file-jwt")
    );
    assert_eq!(
        req.form_value("RoleSessionName").as_deref(),
        Some("from-env")
    );
}

#[test]
fn an_empty_or_missing_token_file_is_invalid_configuration() {
    let fx = Fixture::new().set("AWS_ENDPOINT_URL_STS", "http://127.0.0.1:9");
    let empty = fx.write("empty", " \n");
    let err = run(web_identity_from_file(
        &fx.ctx(),
        "us-east-1",
        "arn:r",
        "s",
        &empty.to_string_lossy(),
    ))
    .expect_err("empty");
    assert_eq!(err.kind(), "invalid_config");
    let err = run(web_identity_from_file(
        &fx.ctx(),
        "us-east-1",
        "arn:r",
        "s",
        "/no/such/file",
    ))
    .expect_err("missing");
    assert_eq!(err.kind(), "invalid_config");
}

#[test]
fn xml_tag_decodes_entities_and_host_header_keeps_non_default_ports() {
    assert_eq!(
        xml_tag("<A>x &lt;y&gt; &quot;z&quot; &apos;w&apos; &amp;</A>", "A").as_deref(),
        Some("x <y> \"z\" 'w' &")
    );
    assert_eq!(xml_tag("<A>1</A>", "B"), None);
    assert_eq!(
        host_header(&Url::parse("http://127.0.0.1:8080/").unwrap()).as_deref(),
        Some("127.0.0.1:8080")
    );
    assert_eq!(
        host_header(&Url::parse("https://sts.us-east-1.amazonaws.com/").unwrap()).as_deref(),
        Some("sts.us-east-1.amazonaws.com")
    );
}

#[test]
fn the_endpoint_follows_region_partition_and_overrides() {
    let fx = Fixture::new();
    assert_eq!(
        sts_endpoint(&fx.ctx(), "us-west-2"),
        "https://sts.us-west-2.amazonaws.com"
    );
    assert_eq!(
        sts_endpoint(&fx.ctx(), "cn-north-1"),
        "https://sts.cn-north-1.amazonaws.com.cn"
    );
    let fx = fx.set("AWS_ENDPOINT_URL", "http://global.example/");
    assert_eq!(
        sts_endpoint(&fx.ctx(), "us-west-2"),
        "http://global.example"
    );
    let fx = fx.set("AWS_ENDPOINT_URL_STS", "http://sts.example");
    assert_eq!(sts_endpoint(&fx.ctx(), "us-west-2"), "http://sts.example");
}
