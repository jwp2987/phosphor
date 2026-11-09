//! The default chain (order, explicit profile, environment) and an end-to-end check that resolved
//! credentials sign a Bedrock `Converse` request a stub can verify independently.

use fake_llm::aws_stub::{BedrockStub, HttpStub, SigV4Expectation, StubResponse, verify_sigv4};

use super::test_support::{Fixture, epoch, now, run, sts_json};
use super::*;

fn resolve_default(fx: &Fixture, profile: &str) -> Result<Creds, CredsError> {
    run(resolve(&fx.ctx(), profile))
}

const STATIC_DEFAULT: &str =
    "[default]\naws_access_key_id = AKPROFILE\naws_secret_access_key = sk-profile\n";

#[test]
fn environment_keys_come_first_and_carry_the_session_token() {
    let fx = Fixture::new()
        .set("AWS_ACCESS_KEY_ID", "AKENV")
        .set("AWS_SECRET_ACCESS_KEY", "sk-env")
        .set("AWS_SESSION_TOKEN", "tok-env");
    fx.credentials(STATIC_DEFAULT);
    let creds = resolve_default(&fx, "").expect("env");
    assert_eq!(creds.access_key_id, "AKENV");
    assert_eq!(creds.session_token.as_deref(), Some("tok-env"));
    // The legacy name for the session token is honoured too.
    let fx = fx
        .unset("AWS_SESSION_TOKEN")
        .set("AWS_SECURITY_TOKEN", "legacy-tok");
    assert_eq!(
        resolve_default(&fx, "")
            .expect("env")
            .session_token
            .as_deref(),
        Some("legacy-tok")
    );
}

#[test]
fn half_an_environment_pair_is_ignored_and_the_default_profile_is_used() {
    let fx = Fixture::new().set("AWS_ACCESS_KEY_ID", "AKONLY");
    fx.credentials(STATIC_DEFAULT);
    assert_eq!(
        resolve_default(&fx, "").expect("profile").access_key_id,
        "AKPROFILE"
    );
}

#[test]
fn an_explicit_profile_ignores_the_environment_and_other_profiles() {
    let fx = Fixture::new()
        .set("AWS_ACCESS_KEY_ID", "AKENV")
        .set("AWS_SECRET_ACCESS_KEY", "sk-env");
    fx.credentials(&format!(
        "{STATIC_DEFAULT}[named]\naws_access_key_id = AKNAMED\naws_secret_access_key = sk-named\n"
    ));
    assert_eq!(
        resolve_default(&fx, "named").expect("named").access_key_id,
        "AKNAMED"
    );
    assert_eq!(
        resolve_default(&fx, "  named \n")
            .expect("trimmed")
            .access_key_id,
        "AKNAMED"
    );
}

#[test]
fn an_explicit_profile_that_is_missing_does_not_fall_through_to_other_sources() {
    let fx = Fixture::new()
        .set("AWS_ACCESS_KEY_ID", "AKENV")
        .set("AWS_SECRET_ACCESS_KEY", "sk-env");
    assert_eq!(
        resolve_default(&fx, "missing").expect_err("missing").kind(),
        "profile_not_found"
    );
}

#[test]
fn aws_profile_selects_the_profile_and_a_missing_one_is_an_error() {
    let fx = Fixture::new().set("AWS_PROFILE", "other");
    fx.credentials(&format!(
        "{STATIC_DEFAULT}[other]\naws_access_key_id = AKOTHER\naws_secret_access_key = sk-other\n"
    ));
    assert_eq!(
        resolve_default(&fx, "").expect("AWS_PROFILE").access_key_id,
        "AKOTHER"
    );

    let fx = Fixture::new().set("AWS_PROFILE", "ghost");
    fx.credentials(STATIC_DEFAULT);
    assert_eq!(
        resolve_default(&fx, "").expect_err("ghost").kind(),
        "profile_not_found"
    );
}

#[test]
fn nothing_configured_ends_in_not_loaded_without_leaving_the_machine() {
    let fx = Fixture::new(); // IMDS disabled by the fixture
    assert_eq!(
        resolve_default(&fx, "").expect_err("nothing").kind(),
        "not_loaded"
    );
}

#[test]
fn the_profile_beats_web_identity_which_beats_container_and_imds() {
    let sts = HttpStub::start(|_| {
        StubResponse::json(
            200,
            sts_json("AssumeRoleWithWebIdentity", "ASIAWEB", "s", "t", epoch(600)),
        )
    });
    let ecs = HttpStub::start(|_| {
        StubResponse::json(
            200,
            r#"{"AccessKeyId":"ASIAECS","SecretAccessKey":"s","Token":"t","Expiration":"2030-01-01T00:00:00Z"}"#,
        )
    });
    let fx = Fixture::new();
    let token = fx.write("tok", "jwt");
    let fx = fx
        .set("AWS_ENDPOINT_URL_STS", &sts.url())
        .set("AWS_WEB_IDENTITY_TOKEN_FILE", &token.to_string_lossy())
        .set("AWS_ROLE_ARN", "arn:aws:iam::1:role/R")
        .set("AWS_CONTAINER_CREDENTIALS_FULL_URI", &ecs.url());

    // No profile: web identity wins over the container endpoint.
    assert_eq!(
        resolve_default(&fx, "")
            .expect("web identity")
            .access_key_id,
        "ASIAWEB"
    );
    assert!(ecs.requests().is_empty());

    // With a default profile, the profile wins over both.
    fx.credentials(STATIC_DEFAULT);
    assert_eq!(
        resolve_default(&fx, "").expect("profile").access_key_id,
        "AKPROFILE"
    );

    // Without web identity, the container endpoint answers.
    let fx = Fixture::new().set("AWS_CONTAINER_CREDENTIALS_FULL_URI", &ecs.url());
    assert_eq!(
        resolve_default(&fx, "").expect("container").access_key_id,
        "ASIAECS"
    );
}

#[test]
fn imds_is_the_last_resort() {
    let imds = HttpStub::start(|req| match (req.method.as_str(), req.path()) {
        ("PUT", _) => StubResponse::text(200, "tok"),
        ("GET", "/latest/meta-data/iam/security-credentials/") => StubResponse::text(200, "role"),
        _ => StubResponse::json(
            200,
            r#"{"Code":"Success","AccessKeyId":"ASIAIMDS","SecretAccessKey":"s","Token":"t","Expiration":"2030-01-01T00:00:00Z"}"#,
        ),
    });
    let fx = Fixture::new()
        .unset("AWS_EC2_METADATA_DISABLED")
        .set("AWS_EC2_METADATA_SERVICE_ENDPOINT", &imds.url());
    assert_eq!(
        resolve_default(&fx, "").expect("imds").access_key_id,
        "ASIAIMDS"
    );
}

#[test]
fn a_profile_error_stops_the_chain_instead_of_falling_through() {
    let fx = Fixture::new().set("AWS_CONTAINER_CREDENTIALS_FULL_URI", "http://127.0.0.1:9");
    fx.config("[default]\nsso_session = nope\nsso_account_id = 1\nsso_role_name = R\n");
    assert_eq!(
        resolve_default(&fx, "").expect_err("broken default").kind(),
        "invalid_config"
    );
}

#[test]
fn region_prefers_aws_region_then_default_region_then_the_profile() {
    let fx = Fixture::new();
    assert_eq!(default_region(&fx.env()), "us-east-1");
    fx.config("[default]\nregion = eu-north-1\n");
    assert_eq!(default_region(&fx.env()), "eu-north-1");
    let fx2 = Fixture::new().set("AWS_DEFAULT_REGION", "ap-northeast-1");
    assert_eq!(default_region(&fx2.env()), "ap-northeast-1");
    let fx3 = fx2.set("AWS_REGION", "sa-east-1");
    assert_eq!(default_region(&fx3.env()), "sa-east-1");
}

#[test]
fn the_oidc_managed_path_assumes_the_role_unsigned_with_the_task_identity_token() {
    let sts = HttpStub::start(|_| {
        StubResponse::json(
            200,
            sts_json(
                "AssumeRoleWithWebIdentity",
                "ASIAOIDC",
                "oidc-secret",
                "oidc-token",
                epoch(3600),
            ),
        )
    });
    let fx = Fixture::new()
        .set("AWS_ENDPOINT_URL_STS", &sts.url())
        .set("AWS_REGION", "us-west-2");
    let creds = run(assume_role_with_web_identity(
        &fx.ctx(),
        "arn:aws:iam::1:role/Oz",
        "Oz_Run_42",
        "task-identity-jwt",
    ))
    .expect("oidc");
    assert_eq!(creds.access_key_id, "ASIAOIDC");
    assert_eq!(creds.expires_at, Some(now() + Duration::from_secs(3600)));
    let req = &sts.requests()[0];
    assert_eq!(
        req.form_value("RoleSessionName").as_deref(),
        Some("Oz_Run_42")
    );
    assert_eq!(
        req.form_value("WebIdentityToken").as_deref(),
        Some("task-identity-jwt")
    );
    assert!(req.header("authorization").is_none());
}

#[test]
fn needs_legacy_login_follows_the_profile_and_its_role_source_chain() {
    let fx = Fixture::new();
    fx.config(
        "[profile lg]\nlogin_session = arn:aws:iam::1:user/me\n\
         [profile via]\nrole_arn = arn:r:x\nsource_profile = lg\n\
         [profile plain]\naws_access_key_id = A\naws_secret_access_key = B\n\
         [profile loop]\nrole_arn = arn:r:x\nsource_profile = loop\n",
    );
    let env = fx.env();
    assert!(needs_legacy_login(&env, "lg"));
    assert!(needs_legacy_login(&env, "via"));
    assert!(!needs_legacy_login(&env, "plain"));
    assert!(!needs_legacy_login(&env, "loop"));
    assert!(!needs_legacy_login(&env, "absent"));

    // The default chain: AWS_PROFILE picks the profile, environment keys short-circuit it.
    let fx = fx.set("AWS_PROFILE", "lg");
    assert!(needs_legacy_login(&fx.env(), ""));
    let fx = fx
        .set("AWS_ACCESS_KEY_ID", "AK")
        .set("AWS_SECRET_ACCESS_KEY", "SK");
    assert!(!needs_legacy_login(&fx.env(), ""));
}

#[test]
fn time_helpers_round_trip_and_accept_the_cli_utc_spelling() {
    let t = parse_time("2023-11-14T22:13:20Z").expect("rfc3339");
    assert_eq!(unix_secs(t), 1_700_000_000);
    assert_eq!(format_time(t), "2023-11-14T22:13:20Z");
    assert_eq!(parse_time("2023-11-14T22:13:20UTC"), Some(t));
    assert_eq!(parse_time("2023-11-14T23:13:20+01:00"), Some(t));
    assert_eq!(parse_time("garbage"), None);
}

#[test]
fn errors_and_credentials_never_print_secrets() {
    let creds = Creds::new("AKSECRETID", "sk-secret", Some("tok-secret".into()), None);
    let shown = format!("{creds:?}");
    assert!(
        !shown.contains("AKSECRETID")
            && !shown.contains("sk-secret")
            && !shown.contains("tok-secret")
    );
    let env = Fixture::new()
        .set("AWS_SECRET_ACCESS_KEY", "sk-in-env")
        .env();
    assert!(!format!("{env:?}").contains("sk-in-env"));
}

// --- Bedrock: resolved credentials sign a request a stub verifies independently -------------------

const MODEL_PATH: &str = "/model/anthropic.claude-sonnet-4-5-20250929-v1%3A0/converse";
const CONVERSE_BODY: &str = r#"{"messages":[{"role":"user","content":[{"text":"hello"}]}]}"#;

/// Signs a `Converse` POST with `creds` (service `bedrock`, the frozen clock) and sends it.
fn send_converse(ctx: &Context, base_url: &str, creds: &Creds, region: &str) -> u16 {
    let url = url::Url::parse(&format!("{base_url}{MODEL_PATH}")).expect("url");
    let host = sts::host_header(&url).expect("host");
    let signed = sigv4::sign(&sigv4::SigningInput {
        method: "POST",
        host: &host,
        path: url.path(),
        headers: &[("content-type", "application/json")],
        body: CONVERSE_BODY.as_bytes(),
        region,
        service: "bedrock",
        creds,
        time: ctx.now(),
    });
    let mut request = ctx
        .http
        .post(url.as_str())
        .header("content-type", "application/json")
        .header("x-amz-date", signed.amz_date.as_str())
        .header("authorization", signed.authorization.as_str());
    if let Some(token) = &signed.security_token {
        request = request.header("x-amz-security-token", token.as_str());
    }
    let response = run(http::send(
        request.body(CONVERSE_BODY),
        "bedrock stub",
        http::DEFAULT_TIMEOUT,
    ))
    .expect("sent");
    response.status
}

fn bedrock_expectation(
    key: &str,
    secret: &str,
    token: Option<&str>,
    region: &str,
) -> SigV4Expectation {
    SigV4Expectation {
        access_key: key.to_string(),
        secret_key: secret.to_string(),
        session_token: token.map(str::to_string),
        region: region.to_string(),
        service: "bedrock".to_string(),
        amz_date: Some(sigv4::amz_date(now())),
    }
}

#[test]
fn profile_credentials_with_a_session_token_sign_a_converse_request_the_stub_accepts() {
    let bedrock = BedrockStub::start(bedrock_expectation(
        "ASIAPROFILE",
        "sk-profile",
        Some("tok-profile"),
        "us-east-1",
    ));
    let fx = Fixture::new();
    fx.credentials("[default]\naws_access_key_id = ASIAPROFILE\naws_secret_access_key = sk-profile\naws_session_token = tok-profile\n");
    let ctx = fx.ctx();
    let creds = run(resolve(&ctx, "")).expect("profile creds");
    assert_eq!(
        send_converse(&ctx, &bedrock.url(), &creds, "us-east-1"),
        200
    );
    assert_eq!(bedrock.verdicts(), vec![Ok(())]);

    // The stub checked the scope, the token header and the content hash; prove it can say no.
    let wrong_region = send_converse(&ctx, &bedrock.url(), &creds, "eu-west-1");
    assert_eq!(wrong_region, 403);
    let wrong_key = Creds::new(
        "ASIAPROFILE",
        "another-secret",
        Some("tok-profile".into()),
        None,
    );
    assert_eq!(
        send_converse(&ctx, &bedrock.url(), &wrong_key, "us-east-1"),
        403
    );
    let no_token = Creds::new("ASIAPROFILE", "sk-profile", None, None);
    assert_eq!(
        send_converse(&ctx, &bedrock.url(), &no_token, "us-east-1"),
        403
    );
}

#[test]
fn assumed_role_credentials_sign_a_converse_request_end_to_end() {
    // STS verifies the AssumeRole signature, then hands back session credentials; Bedrock verifies a
    // request signed with those.
    let sts_expect = SigV4Expectation {
        access_key: "AKBASE".to_string(),
        secret_key: "sk-base".to_string(),
        session_token: None,
        region: "us-east-1".to_string(),
        service: "sts".to_string(),
        amz_date: Some(sigv4::amz_date(now())),
    };
    let sts = HttpStub::start(move |req| match verify_sigv4(req, &sts_expect) {
        Ok(()) => StubResponse::json(
            200,
            sts_json(
                "AssumeRole",
                "ASIAASSUMED",
                "sk-assumed",
                "tok-assumed",
                epoch(3600),
            ),
        ),
        Err(reason) => StubResponse::text(403, reason),
    });
    let bedrock = BedrockStub::start(bedrock_expectation(
        "ASIAASSUMED",
        "sk-assumed",
        Some("tok-assumed"),
        "us-east-1",
    ));

    let fx = Fixture::new().set("AWS_ENDPOINT_URL_STS", &sts.url());
    fx.credentials("[base]\naws_access_key_id = AKBASE\naws_secret_access_key = sk-base\n");
    fx.config("[profile bedrock]\nrole_arn = arn:aws:iam::123456789012:role/BedrockInvoke\nsource_profile = base\n");
    let ctx = fx.ctx();
    let creds = run(resolve(&ctx, "bedrock")).expect("assumed");
    assert_eq!(
        send_converse(&ctx, &bedrock.url(), &creds, "us-east-1"),
        200
    );
    assert_eq!(bedrock.verdicts(), vec![Ok(())]);
}
