//! ECS / container credentials and EC2 IMDSv2, plus the JSON credential document shared with
//! `credential_process`.

use std::time::Duration;

use serde_json::Value;

use super::{Context, Creds, CredsError, http, parse_time};

const DEFAULT_IMDS_ENDPOINT: &str = "http://169.254.169.254";
const ECS_RELATIVE_HOST: &str = "http://169.254.170.2";
/// IMDS is link-local: if it is not there, fail fast rather than stall the chain.
const IMDS_TIMEOUT: Duration = Duration::from_secs(1);
const CONTAINER_TIMEOUT: Duration = Duration::from_secs(5);
const IMDS_TOKEN_TTL_SECS: &str = "21600";

/// Parses the credential JSON used by IMDS, ECS and `credential_process`:
/// `AccessKeyId`, `SecretAccessKey`, `Token` or `SessionToken`, optional `Expiration`.
/// A `Code` other than `Success` is an error document.
pub(crate) fn parse_json_credentials(body: &str) -> Result<Creds, CredsError> {
    let value: Value = serde_json::from_str(body)
        .map_err(|_| CredsError::Provider("the credential document is not valid JSON".into()))?;
    if let Some(code) = value.get("Code").and_then(Value::as_str)
        && code != "Success"
    {
        return Err(CredsError::Provider(format!(
            "the credential provider returned the error code {code}"
        )));
    }
    let text = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|s| !s.is_empty())
    };
    let access_key = text("AccessKeyId");
    let secret = text("SecretAccessKey");
    let (Some(access_key), Some(secret)) = (access_key, secret) else {
        return Err(CredsError::Provider(
            "the credential document is missing AccessKeyId or SecretAccessKey".into(),
        ));
    };
    let token = text("Token").or_else(|| text("SessionToken"));
    let expires_at = text("Expiration").and_then(|t| parse_time(&t));
    Ok(Creds::new(access_key, secret, token, expires_at))
}

/// ECS / container credentials from `AWS_CONTAINER_CREDENTIALS_RELATIVE_URI` or
/// `AWS_CONTAINER_CREDENTIALS_FULL_URI`. `NotLoaded` when neither is set.
pub(crate) async fn container(ctx: &Context) -> Result<Creds, CredsError> {
    let url = if let Some(relative) = ctx.env.get("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI") {
        format!("{ECS_RELATIVE_HOST}{relative}")
    } else if let Some(full) = ctx.env.get("AWS_CONTAINER_CREDENTIALS_FULL_URI") {
        full.to_string()
    } else {
        return Err(CredsError::NotLoaded(
            "no container credentials URI is set".into(),
        ));
    };

    let auth = if let Some(path) = ctx.env.get("AWS_CONTAINER_AUTHORIZATION_TOKEN_FILE") {
        Some(
            std::fs::read_to_string(path)
                .map_err(|_| {
                    CredsError::InvalidConfig(
                        "cannot read the container authorization token file".into(),
                    )
                })?
                .trim()
                .to_string(),
        )
    } else {
        ctx.env
            .get("AWS_CONTAINER_AUTHORIZATION_TOKEN")
            .map(str::to_string)
    };

    let mut request = ctx.http.get(url.as_str());
    if let Some(auth) = &auth {
        request = request.header("authorization", auth.as_str());
    }
    let response = http::send(
        request,
        "the container credentials endpoint",
        CONTAINER_TIMEOUT,
    )
    .await?;
    if !response.is_success() {
        return Err(CredsError::Provider(format!(
            "the container credentials endpoint answered HTTP {}",
            response.status
        )));
    }
    parse_json_credentials(&response.body)
}

/// EC2 instance-profile credentials over IMDSv2 (session token first; IMDSv1 only when the token
/// endpoint is refused with 403/404/405). `NotLoaded` when IMDS is disabled or unreachable.
pub(crate) async fn imds(ctx: &Context) -> Result<Creds, CredsError> {
    if ctx
        .env
        .get("AWS_EC2_METADATA_DISABLED")
        .is_some_and(|v| v.eq_ignore_ascii_case("true"))
    {
        return Err(CredsError::NotLoaded("IMDS is disabled".into()));
    }
    let base = ctx
        .env
        .get("AWS_EC2_METADATA_SERVICE_ENDPOINT")
        .unwrap_or(DEFAULT_IMDS_ENDPOINT)
        .trim_end_matches('/')
        .to_string();

    let token_url = format!("{base}/latest/api/token");
    let token_request = ctx
        .http
        .put(token_url.as_str())
        .header("x-aws-ec2-metadata-token-ttl-seconds", IMDS_TOKEN_TTL_SECS);
    let token = match http::send(token_request, "IMDS", IMDS_TIMEOUT).await {
        Ok(response) if response.is_success() => Some(response.body.trim().to_string()),
        Ok(response) if matches!(response.status, 403 | 404 | 405) => None,
        Ok(response) => {
            return Err(CredsError::NotLoaded(format!(
                "IMDS token request answered HTTP {}",
                response.status
            )));
        }
        // Unreachable or slow: not on EC2.
        Err(_) => return Err(CredsError::NotLoaded("IMDS is not reachable".into())),
    };

    let role_url = format!("{base}/latest/meta-data/iam/security-credentials/");
    let role_response = imds_get(ctx, &role_url, token.as_deref()).await?;
    let role = role_response
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .ok_or_else(|| CredsError::NotLoaded("IMDS lists no instance profile role".into()))?
        .to_string();

    let creds_url = format!("{role_url}{role}");
    let body = imds_get(ctx, &creds_url, token.as_deref()).await?;
    parse_json_credentials(&body)
}

async fn imds_get(ctx: &Context, url: &str, token: Option<&str>) -> Result<String, CredsError> {
    let mut request = ctx.http.get(url);
    if let Some(token) = token {
        request = request.header("x-aws-ec2-metadata-token", token);
    }
    let response = http::send(request, "IMDS", IMDS_TIMEOUT).await?;
    if response.status == 404 {
        return Err(CredsError::NotLoaded(
            "IMDS has no instance profile credentials".into(),
        ));
    }
    if !response.is_success() {
        return Err(CredsError::Provider(format!(
            "IMDS answered HTTP {}",
            response.status
        )));
    }
    Ok(response.body)
}

#[cfg(test)]
#[path = "imds_tests.rs"]
mod tests;
