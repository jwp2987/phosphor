//! Profile resolution: turns one named profile of `~/.aws/config` + `~/.aws/credentials` into
//! credentials, following `source_profile` chains for AssumeRole.
//!
//! Per profile, in order:
//!
//! 1. `role_arn` with `web_identity_token_file`: web identity.
//! 2. `role_arn` with `source_profile` or `credential_source`: SigV4 `AssumeRole` over the base
//!    credentials (`mfa_serial` is reported as [`CredsError::MfaRequired`]).
//! 3. `sso_session` / `sso_start_url`: IAM Identity Center.
//! 4. `login_session`: `aws login`.
//! 5. `aws_access_key_id` + `aws_secret_access_key` (+ `aws_session_token`): static credentials.
//! 6. `credential_process`.

use futures::future::BoxFuture;

use super::ini::{ParsedFiles, Props};
use super::{Context, Creds, CredsError, env_credentials, imds, login, process, sso, sts};

pub(crate) async fn resolve_profile(ctx: &Context, name: &str) -> Result<Creds, CredsError> {
    let files = ParsedFiles::load(&ctx.env);
    resolve_in(ctx, &files, name, Vec::new()).await
}

pub(crate) async fn resolve_in(
    ctx: &Context,
    files: &ParsedFiles,
    name: &str,
    chain: Vec<String>,
) -> Result<Creds, CredsError> {
    resolve_named(ctx, files, name.to_string(), chain).await
}

fn resolve_named<'a>(
    ctx: &'a Context,
    files: &'a ParsedFiles,
    name: String,
    mut chain: Vec<String>,
) -> BoxFuture<'a, Result<Creds, CredsError>> {
    Box::pin(async move {
        if chain.contains(&name) {
            return Err(CredsError::InvalidConfig(
                "source_profile forms a loop".to_string(),
            ));
        }
        let props = files.profiles.get(&name).ok_or_else(|| {
            CredsError::ProfileNotFound(format!("profile `{name}` is not defined"))
        })?;
        chain.push(name.clone());

        if let Some(role_arn) = props.get("role_arn") {
            return resolve_role(ctx, files, &name, props, role_arn, chain).await;
        }
        if props.contains_key("sso_session") || props.contains_key("sso_start_url") {
            return sso::credentials(ctx, files, props).await;
        }
        if props.contains_key("login_session") {
            return login::credentials(ctx, props).await;
        }
        if let Some(creds) = static_credentials(props) {
            return Ok(creds);
        }
        if let Some(command) = props.get("credential_process") {
            return process::credentials(ctx, command).await;
        }
        Err(CredsError::NotLoaded(format!(
            "profile `{name}` has no credentials"
        )))
    })
}

fn static_credentials(props: &Props) -> Option<Creds> {
    let key = props.get("aws_access_key_id").filter(|v| !v.is_empty())?;
    let secret = props
        .get("aws_secret_access_key")
        .filter(|v| !v.is_empty())?;
    let token = props
        .get("aws_session_token")
        .or_else(|| props.get("aws_security_token"))
        .filter(|v| !v.is_empty())
        .cloned();
    Some(Creds::new(key.clone(), secret.clone(), token, None))
}

async fn resolve_role(
    ctx: &Context,
    files: &ParsedFiles,
    name: &str,
    props: &Props,
    role_arn: &str,
    chain: Vec<String>,
) -> Result<Creds, CredsError> {
    let region = props
        .get("region")
        .cloned()
        .or_else(|| ctx.env.region())
        .unwrap_or_else(|| "us-east-1".to_string());
    let session_name = props
        .get("role_session_name")
        .cloned()
        .unwrap_or_else(|| sts::default_session_name(ctx));
    let duration_secs = props
        .get("duration_seconds")
        .map(|d| {
            d.parse::<u64>().map_err(|_| {
                CredsError::InvalidConfig("duration_seconds is not a number".to_string())
            })
        })
        .transpose()?;

    if let Some(token_file) = props.get("web_identity_token_file") {
        let token = std::fs::read_to_string(token_file).map_err(|_| {
            CredsError::InvalidConfig("cannot read the web identity token file".to_string())
        })?;
        let token = token.trim();
        if token.is_empty() {
            return Err(CredsError::InvalidConfig(
                "the web identity token file is empty".to_string(),
            ));
        }
        return sts::assume_role_with_web_identity(
            ctx,
            &region,
            role_arn,
            &session_name,
            token,
            duration_secs,
        )
        .await;
    }

    if props.contains_key("mfa_serial") {
        return Err(CredsError::MfaRequired(format!(
            "profile `{name}` requires an MFA code"
        )));
    }

    let base = match (props.get("source_profile"), props.get("credential_source")) {
        (Some(_), Some(_)) => {
            return Err(CredsError::InvalidConfig(
                "a profile cannot set both source_profile and credential_source".to_string(),
            ));
        }
        (Some(source), None) => resolve_named(ctx, files, source.clone(), chain).await?,
        (None, Some(source)) => credential_source(ctx, source).await?,
        // A role profile that carries its own static keys assumes the role with them.
        (None, None) => static_credentials(props).ok_or_else(|| {
            CredsError::InvalidConfig(
                "role_arn needs a source_profile or credential_source".to_string(),
            )
        })?,
    };

    sts::assume_role(
        ctx,
        &base,
        &sts::AssumeRoleRequest {
            role_arn,
            session_name: &session_name,
            external_id: props.get("external_id").map(String::as_str),
            duration_secs,
            region: &region,
        },
    )
    .await
}

async fn credential_source(ctx: &Context, source: &str) -> Result<Creds, CredsError> {
    match source {
        "Environment" => env_credentials(&ctx.env).ok_or_else(|| {
            CredsError::NotLoaded("credential_source Environment has no credentials".to_string())
        }),
        "Ec2InstanceMetadata" => imds::imds(ctx).await,
        "EcsContainer" => imds::container(ctx).await,
        _ => Err(CredsError::InvalidConfig(
            "unknown credential_source".to_string(),
        )),
    }
}

#[cfg(test)]
#[path = "profile_tests.rs"]
mod tests;
