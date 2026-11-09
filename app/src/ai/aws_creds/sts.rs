//! STS over the Query API: unsigned `AssumeRoleWithWebIdentity` (the web identity token is the
//! credential) and SigV4-signed `AssumeRole`.
//!
//! Requests ask for JSON (`Accept: application/json`); the parser also reads the classic XML
//! response, in case an endpoint ignores the header.

use std::time::{Duration, UNIX_EPOCH};

use serde_json::Value;
use url::Url;

use super::{
    Context, Creds, CredsError, dns_suffix, http, parse_time, service_endpoint, sigv4, unix_secs,
};

const STS_VERSION: &str = "2011-06-15";
/// STS messages are shown to the user (the OIDC path always did); cap them.
const MAX_MESSAGE_CHARS: usize = 300;

pub(crate) fn sts_endpoint(ctx: &Context, region: &str) -> String {
    service_endpoint(&ctx.env, "STS", || {
        format!("https://sts.{region}.{}", dns_suffix(region))
    })
}

pub(crate) fn default_session_name(ctx: &Context) -> String {
    format!("phosphor-{}", unix_secs(ctx.now()))
}

/// `AWS_WEB_IDENTITY_TOKEN_FILE` + `AWS_ROLE_ARN` (+ `AWS_ROLE_SESSION_NAME`). `Ok(None)` when the
/// variables are not set.
pub(crate) async fn web_identity_from_env(ctx: &Context) -> Result<Option<Creds>, CredsError> {
    let (Some(token_file), Some(role_arn)) = (
        ctx.env.get("AWS_WEB_IDENTITY_TOKEN_FILE"),
        ctx.env.get("AWS_ROLE_ARN"),
    ) else {
        return Ok(None);
    };
    let session_name = ctx
        .env
        .get("AWS_ROLE_SESSION_NAME")
        .map(str::to_string)
        .unwrap_or_else(|| default_session_name(ctx));
    let region = ctx.env.region().unwrap_or_else(|| "us-east-1".to_string());
    web_identity_from_file(ctx, &region, role_arn, &session_name, token_file)
        .await
        .map(Some)
}

pub(crate) async fn web_identity_from_file(
    ctx: &Context,
    region: &str,
    role_arn: &str,
    session_name: &str,
    token_file: &str,
) -> Result<Creds, CredsError> {
    let token = std::fs::read_to_string(token_file)
        .map_err(|_| CredsError::InvalidConfig("cannot read the web identity token file".into()))?;
    let token = token.trim();
    if token.is_empty() {
        return Err(CredsError::InvalidConfig(
            "the web identity token file is empty".into(),
        ));
    }
    assume_role_with_web_identity(ctx, region, role_arn, session_name, token, None).await
}

/// Unsigned `AssumeRoleWithWebIdentity`. Also the OidcManaged (Oz task identity) path.
pub(crate) async fn assume_role_with_web_identity(
    ctx: &Context,
    region: &str,
    role_arn: &str,
    session_name: &str,
    web_identity_token: &str,
    duration_secs: Option<u64>,
) -> Result<Creds, CredsError> {
    let endpoint = sts_endpoint(ctx, region);
    let duration = duration_secs.map(|d| d.to_string());
    let mut form: Vec<(&str, &str)> = vec![
        ("Action", "AssumeRoleWithWebIdentity"),
        ("Version", STS_VERSION),
        ("RoleArn", role_arn),
        ("RoleSessionName", session_name),
        ("WebIdentityToken", web_identity_token),
    ];
    if let Some(duration) = &duration {
        form.push(("DurationSeconds", duration));
    }
    let request = ctx
        .http
        .post(endpoint.as_str())
        .header("accept", "application/json")
        .form(&form);
    let response = http::send(request, "STS", http::DEFAULT_TIMEOUT).await?;
    credentials_from_response(&response, "AssumeRoleWithWebIdentity")
}

/// One `AssumeRole` call.
pub(crate) struct AssumeRoleRequest<'a> {
    pub(crate) role_arn: &'a str,
    pub(crate) session_name: &'a str,
    pub(crate) external_id: Option<&'a str>,
    pub(crate) duration_secs: Option<u64>,
    pub(crate) region: &'a str,
}

/// SigV4-signed `AssumeRole` using `base` as the caller identity.
pub(crate) async fn assume_role(
    ctx: &Context,
    base: &Creds,
    req: &AssumeRoleRequest<'_>,
) -> Result<Creds, CredsError> {
    let endpoint = sts_endpoint(ctx, req.region);
    let url = Url::parse(&endpoint)
        .map_err(|_| CredsError::InvalidConfig("the STS endpoint is not a valid URL".into()))?;
    let host = host_header(&url)
        .ok_or_else(|| CredsError::InvalidConfig("the STS endpoint has no host".into()))?;

    // The serializer is not Send; build the body in a block so it is gone before the await.
    let body = {
        let mut form = url::form_urlencoded::Serializer::new(String::new());
        form.append_pair("Action", "AssumeRole")
            .append_pair("Version", STS_VERSION)
            .append_pair("RoleArn", req.role_arn)
            .append_pair("RoleSessionName", req.session_name);
        if let Some(duration) = req.duration_secs {
            form.append_pair("DurationSeconds", &duration.to_string());
        }
        if let Some(external_id) = req.external_id {
            form.append_pair("ExternalId", external_id);
        }
        form.finish()
    };

    let content_type = "application/x-www-form-urlencoded; charset=utf-8";
    let signed = sigv4::sign(&sigv4::SigningInput {
        method: "POST",
        host: &host,
        path: url.path(),
        headers: &[("content-type", content_type)],
        body: body.as_bytes(),
        region: req.region,
        service: "sts",
        creds: base,
        time: ctx.now(),
    });

    let mut request = ctx
        .http
        .post(url.as_str())
        .header("content-type", content_type)
        .header("accept", "application/json")
        .header("x-amz-date", signed.amz_date.as_str())
        .header("authorization", signed.authorization.as_str());
    if let Some(token) = &signed.security_token {
        request = request.header("x-amz-security-token", token.as_str());
    }
    let request = request.body(body);
    let response = http::send(request, "STS", http::DEFAULT_TIMEOUT).await?;
    credentials_from_response(&response, "AssumeRole")
}

/// The `Host` header value for `url`: the host, plus the port when it is not the scheme default.
pub(crate) fn host_header(url: &Url) -> Option<String> {
    let host = url.host_str()?;
    Some(match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    })
}

fn credentials_from_response(
    response: &http::HttpResponse,
    action: &str,
) -> Result<Creds, CredsError> {
    if !response.is_success() {
        return Err(sts_error(response, action));
    }
    parse_credentials(&response.body).ok_or_else(|| {
        CredsError::Provider(format!("STS {action} response did not include credentials"))
    })
}

/// Reads the `Credentials` block of an STS response (JSON or XML).
fn parse_credentials(body: &str) -> Option<Creds> {
    if body.trim_start().starts_with('{') {
        let value: Value = serde_json::from_str(body).ok()?;
        let credentials = find_key(&value, "Credentials")?;
        let text = |key: &str| credentials.get(key)?.as_str().map(str::to_string);
        let expires_at = match credentials.get("Expiration") {
            Some(Value::Number(n)) => n
                .as_f64()
                .filter(|secs| *secs >= 0.0)
                .map(|secs| UNIX_EPOCH + Duration::from_secs_f64(secs)),
            Some(Value::String(s)) => parse_time(s),
            _ => None,
        };
        return Some(Creds::new(
            text("AccessKeyId")?,
            text("SecretAccessKey")?,
            text("SessionToken"),
            expires_at,
        ));
    }
    let start = body.find("<Credentials>")?;
    let end = body[start..].find("</Credentials>")? + start;
    let block = &body[start..end];
    Some(Creds::new(
        xml_tag(block, "AccessKeyId")?,
        xml_tag(block, "SecretAccessKey")?,
        xml_tag(block, "SessionToken"),
        xml_tag(block, "Expiration").and_then(|t| parse_time(&t)),
    ))
}

fn find_key<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Object(map) => map
            .get(key)
            .or_else(|| map.values().find_map(|v| find_key(v, key))),
        _ => None,
    }
}

/// The text of the first `<tag>...</tag>` in `xml`, with the five predefined entities decoded.
pub(crate) fn xml_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(
        xml[start..end]
            .trim()
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&amp;", "&"),
    )
}

/// Builds the error for a non-2xx STS answer from its error *code* and message. The body itself is
/// never kept or logged.
fn sts_error(response: &http::HttpResponse, action: &str) -> CredsError {
    let (code, message) = if response.body.trim_start().starts_with('{') {
        let value: Value = serde_json::from_str(&response.body).unwrap_or(Value::Null);
        let text = |keys: &[&str]| {
            keys.iter()
                .find_map(|k| find_key(&value, k)?.as_str().map(str::to_string))
        };
        (
            text(&["Code", "code", "__type"]).unwrap_or_default(),
            text(&["Message", "message"]).unwrap_or_default(),
        )
    } else {
        (
            xml_tag(&response.body, "Code").unwrap_or_default(),
            xml_tag(&response.body, "Message").unwrap_or_default(),
        )
    };
    // `__type` looks like `com.amazon.coral#ExpiredTokenException`.
    let code = code.rsplit('#').next().unwrap_or_default().to_string();
    let message: String = message.chars().take(MAX_MESSAGE_CHARS).collect();
    let detail = match (code.is_empty(), message.is_empty()) {
        (true, _) => format!("STS {action} failed with HTTP {}", response.status),
        (false, true) => format!("STS {action} failed: {code}"),
        (false, false) => format!("STS {action} failed: {code}: {message}"),
    };
    if code.starts_with("ExpiredToken") {
        CredsError::Expired(detail)
    } else {
        CredsError::Provider(detail)
    }
}

#[cfg(test)]
#[path = "sts_tests.rs"]
mod tests;
