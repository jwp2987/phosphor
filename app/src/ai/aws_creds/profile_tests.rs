use std::time::Duration;

use fake_llm::aws_stub::{HttpStub, SigV4Expectation, StubResponse, verify_sigv4};

use super::*;
use crate::ai::aws_creds::sigv4::amz_date;
use crate::ai::aws_creds::test_support::{Fixture, epoch, now, run, sts_json};

fn resolve(fx: &Fixture, profile: &str) -> Result<Creds, CredsError> {
    run(resolve_profile(&fx.ctx(), profile))
}

#[test]
fn static_keys_from_the_credentials_file_with_and_without_a_session_token() {
    let fx = Fixture::new();
    fx.credentials(
        "[default]\naws_access_key_id = AKDEFAULT\naws_secret_access_key = sk-default\n\
         [work]\naws_access_key_id = AKWORK\naws_secret_access_key = sk-work\naws_session_token = tok-work\n",
    );
    let default = resolve(&fx, "default").expect("default");
    assert_eq!(default.access_key_id, "AKDEFAULT");
    assert_eq!(default.session_token, None);
    assert_eq!(default.expires_at, None);
    let work = resolve(&fx, "work").expect("work");
    assert_eq!(work.access_key_id, "AKWORK");
    assert_eq!(work.session_token.as_deref(), Some("tok-work"));
}

#[test]
fn static_keys_in_the_config_file_work_and_the_credentials_file_wins() {
    let fx = Fixture::new();
    fx.config("[profile p]\naws_access_key_id = FROMCONFIG\naws_secret_access_key = cfg-secret\n");
    assert_eq!(
        resolve(&fx, "p").expect("config keys").access_key_id,
        "FROMCONFIG"
    );
    fx.credentials("[p]\naws_access_key_id = FROMCREDS\naws_secret_access_key = creds-secret\n");
    let creds = resolve(&fx, "p").expect("merged");
    assert_eq!(creds.access_key_id, "FROMCREDS");
    assert_eq!(creds.secret_access_key, "creds-secret");
}

#[test]
fn a_missing_profile_and_a_profile_without_credentials_are_distinct() {
    let fx = Fixture::new();
    fx.config("[profile empty]\nregion = us-west-2\n");
    assert_eq!(
        resolve(&fx, "nope").expect_err("missing").kind(),
        "profile_not_found"
    );
    assert_eq!(
        resolve(&fx, "empty").expect_err("no creds").kind(),
        "not_loaded"
    );
    // Half a key pair is not credentials.
    fx.credentials("[half]\naws_access_key_id = AK\n");
    assert_eq!(resolve(&fx, "half").expect_err("half").kind(), "not_loaded");
}

/// An STS stub that verifies SigV4 for whichever of `keys` (access key, secret, token) signed the
/// request, then answers with credentials named after the role it was asked to assume.
fn sts_stub(
    keys: Vec<(&'static str, &'static str, Option<&'static str>)>,
    region: &'static str,
) -> HttpStub {
    HttpStub::start(move |req| {
        let auth = req.header("authorization").unwrap_or("");
        let Some((access, secret, token)) = keys
            .iter()
            .find(|(ak, _, _)| auth.contains(&format!("Credential={ak}/")))
        else {
            return StubResponse::text(403, "unknown signer");
        };
        let expectation = SigV4Expectation {
            access_key: access.to_string(),
            secret_key: secret.to_string(),
            session_token: token.map(str::to_string),
            region: region.to_string(),
            service: "sts".to_string(),
            amz_date: Some(amz_date(now())),
        };
        if let Err(reason) = verify_sigv4(req, &expectation) {
            return StubResponse::text(403, reason);
        }
        let role = req.form_value("RoleArn").unwrap_or_default();
        let name = role.rsplit('/').next().unwrap_or("x").to_string();
        StubResponse::json(
            200,
            sts_json(
                "AssumeRole",
                &format!("ASIA{name}"),
                &format!("secret-{name}"),
                &format!("token-{name}"),
                epoch(3000),
            ),
        )
    })
}

#[test]
fn assume_role_over_a_source_profile_signs_with_the_base_keys() {
    let stub = sts_stub(vec![("AKBASE", "sk-base", None)], "us-west-2");
    let fx = Fixture::new().set("AWS_ENDPOINT_URL_STS", &stub.url());
    fx.credentials("[base]\naws_access_key_id = AKBASE\naws_secret_access_key = sk-base\n");
    fx.config(
        "[profile admin]\nrole_arn = arn:aws:iam::123456789012:role/Admin\nsource_profile = base\n\
         region = us-west-2\nexternal_id = ext-1\nrole_session_name = my-session\nduration_seconds = 1800\n",
    );
    let creds = resolve(&fx, "admin").expect("assumed");
    assert_eq!(creds.access_key_id, "ASIAAdmin");
    assert_eq!(creds.session_token.as_deref(), Some("token-Admin"));
    assert_eq!(creds.expires_at, Some(now() + Duration::from_secs(3000)));

    let req = &stub.requests()[0];
    assert_eq!(req.form_value("ExternalId").as_deref(), Some("ext-1"));
    assert_eq!(
        req.form_value("RoleSessionName").as_deref(),
        Some("my-session")
    );
    assert_eq!(req.form_value("DurationSeconds").as_deref(), Some("1800"));
}

#[test]
fn a_chain_of_two_roles_signs_each_hop_with_the_previous_hops_credentials() {
    // hop 1 (base keys) -> role One; hop 2 (One's session keys) -> role Two.
    let stub = sts_stub(
        vec![
            ("AKBASE", "sk-base", None),
            ("ASIAOne", "secret-One", Some("token-One")),
        ],
        "us-east-1",
    );
    let fx = Fixture::new().set("AWS_ENDPOINT_URL_STS", &stub.url());
    fx.credentials("[base]\naws_access_key_id = AKBASE\naws_secret_access_key = sk-base\n");
    fx.config(
        "[profile one]\nrole_arn = arn:aws:iam::1:role/One\nsource_profile = base\n\
         [profile two]\nrole_arn = arn:aws:iam::1:role/Two\nsource_profile = one\n",
    );
    let creds = resolve(&fx, "two").expect("two hops");
    assert_eq!(creds.access_key_id, "ASIATwo");
    assert_eq!(stub.requests().len(), 2);
}

#[test]
fn a_source_profile_loop_is_invalid_configuration() {
    let fx = Fixture::new().set("AWS_ENDPOINT_URL_STS", "http://127.0.0.1:9");
    fx.config(
        "[profile a]\nrole_arn = arn:r:a\nsource_profile = b\n[profile b]\nrole_arn = arn:r:b\nsource_profile = a\n\
         [profile self]\nrole_arn = arn:r:s\nsource_profile = self\n",
    );
    assert_eq!(
        resolve(&fx, "a").expect_err("loop").kind(),
        "invalid_config"
    );
    assert_eq!(
        resolve(&fx, "self").expect_err("self loop").kind(),
        "invalid_config"
    );
}

#[test]
fn a_failed_assume_role_is_a_provider_error_with_the_aws_code() {
    let stub = HttpStub::start(|_| {
        StubResponse::xml(
            403,
            "<ErrorResponse><Error><Code>AccessDenied</Code><Message>not allowed</Message></Error></ErrorResponse>",
        )
    });
    let fx = Fixture::new().set("AWS_ENDPOINT_URL_STS", &stub.url());
    fx.credentials("[base]\naws_access_key_id = AK\naws_secret_access_key = SK\n");
    fx.config("[profile r]\nrole_arn = arn:r:x\nsource_profile = base\n");
    let err = resolve(&fx, "r").expect_err("denied");
    assert_eq!(err.kind(), "provider");
    assert!(err.detail().contains("AccessDenied"));
}

#[test]
fn role_profiles_with_both_or_neither_source_are_invalid_and_mfa_is_reported() {
    let fx = Fixture::new();
    fx.config(
        "[profile both]\nrole_arn = arn:r:x\nsource_profile = b\ncredential_source = Environment\n\
         [profile neither]\nrole_arn = arn:r:x\n\
         [profile mfa]\nrole_arn = arn:r:x\nsource_profile = b\nmfa_serial = arn:aws:iam::1:mfa/me\n\
         [profile badsrc]\nrole_arn = arn:r:x\ncredential_source = Wat\n",
    );
    assert_eq!(
        resolve(&fx, "both").expect_err("both").kind(),
        "invalid_config"
    );
    assert_eq!(
        resolve(&fx, "neither").expect_err("neither").kind(),
        "invalid_config"
    );
    assert_eq!(resolve(&fx, "mfa").expect_err("mfa").kind(), "mfa_required");
    assert_eq!(
        resolve(&fx, "badsrc").expect_err("unknown source").kind(),
        "invalid_config"
    );
}

#[test]
fn credential_source_environment_uses_the_environment_keys() {
    let stub = sts_stub(vec![("AKENV", "sk-env", Some("tok-env"))], "us-east-1");
    let fx = Fixture::new()
        .set("AWS_ENDPOINT_URL_STS", &stub.url())
        .set("AWS_ACCESS_KEY_ID", "AKENV")
        .set("AWS_SECRET_ACCESS_KEY", "sk-env")
        .set("AWS_SESSION_TOKEN", "tok-env");
    fx.config(
        "[profile r]\nrole_arn = arn:aws:iam::1:role/EnvRole\ncredential_source = Environment\n",
    );
    assert_eq!(
        resolve(&fx, "r").expect("assumed").access_key_id,
        "ASIAEnvRole"
    );

    let bare = Fixture::new();
    bare.config("[profile r]\nrole_arn = arn:r:x\ncredential_source = Environment\n");
    assert_eq!(
        resolve(&bare, "r").expect_err("no env keys").kind(),
        "not_loaded"
    );
}

#[test]
fn a_role_profile_with_a_web_identity_token_file_assumes_unsigned() {
    let stub = HttpStub::start(|_| {
        StubResponse::json(
            200,
            sts_json("AssumeRoleWithWebIdentity", "ASIAWI", "s", "t", epoch(600)),
        )
    });
    let fx = Fixture::new().set("AWS_ENDPOINT_URL_STS", &stub.url());
    let token = fx.write("wi-token", "jwt-from-file\n");
    fx.config(&format!(
        "[profile wi]\nrole_arn = arn:aws:iam::1:role/Wi\nweb_identity_token_file = {}\n",
        token.display()
    ));
    assert_eq!(
        resolve(&fx, "wi").expect("web identity").access_key_id,
        "ASIAWI"
    );
    let req = &stub.requests()[0];
    assert_eq!(
        req.form_value("WebIdentityToken").as_deref(),
        Some("jwt-from-file")
    );
    assert!(req.header("authorization").is_none());
}

#[cfg(unix)]
#[test]
fn credential_process_profiles_run_the_command() {
    let fx = Fixture::new();
    fx.config(
        "[profile proc]\ncredential_process = printf '%s' '{\"Version\":1,\"AccessKeyId\":\"ASIAP\",\"SecretAccessKey\":\"ps\"}'\n",
    );
    assert_eq!(
        resolve(&fx, "proc").expect("process").access_key_id,
        "ASIAP"
    );
}

#[test]
fn static_keys_take_precedence_over_credential_process() {
    let fx = Fixture::new();
    fx.credentials(
        "[both]\naws_access_key_id = AKSTATIC\naws_secret_access_key = s\ncredential_process = definitely-not-run\n",
    );
    assert_eq!(
        resolve(&fx, "both").expect("static wins").access_key_id,
        "AKSTATIC"
    );
}
