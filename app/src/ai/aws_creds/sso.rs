//! IAM Identity Center (SSO): read the CLI's token cache, refresh it through the `ssooidc`
//! `CreateToken` refresh-token grant when it is close to expiring, then exchange the access token
//! for role credentials at the SSO portal.
//!
//! The cache file is `~/.aws/sso/cache/<sha1(session name | start url)>.json`, shared with the
//! AWS CLI. A refresh writes the new access token (and rotated refresh token) back into it, with
//! every other field preserved.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde_json::{Value, json};
use sha1::{Digest, Sha1};

use super::ini::{ParsedFiles, Props};
use super::{
    Context, Creds, CredsError, REFRESH_WINDOW, dns_suffix, format_time, http, parse_time,
    service_endpoint,
};

const EXPIRED_HINT: &str = "run `aws sso login`";

/// The pieces of a profile's SSO configuration.
struct SsoConfig {
    /// The cache key: the `sso_session` name, or the start URL for the legacy form.
    cache_key: String,
    region: String,
    account_id: String,
    role_name: String,
}

fn config(files: &ParsedFiles, props: &Props) -> Result<SsoConfig, CredsError> {
    let invalid = |what: &str| CredsError::InvalidConfig(format!("SSO profile is missing {what}"));
    let session_name = props.get("sso_session");
    let session = match session_name {
        Some(name) => Some(files.sso_sessions.get(name).ok_or_else(|| {
            CredsError::InvalidConfig("the profile's sso_session section does not exist".into())
        })?),
        None => None,
    };
    let from_session = |key: &str| session.and_then(|s| s.get(key));
    let start_url = props
        .get("sso_start_url")
        .or_else(|| from_session("sso_start_url"));
    let region = props
        .get("sso_region")
        .or_else(|| from_session("sso_region"))
        .ok_or_else(|| invalid("sso_region"))?;
    let cache_key = match session_name {
        Some(name) => name.clone(),
        None => start_url.ok_or_else(|| invalid("sso_start_url"))?.clone(),
    };
    Ok(SsoConfig {
        cache_key,
        region: region.clone(),
        account_id: props
            .get("sso_account_id")
            .ok_or_else(|| invalid("sso_account_id"))?
            .clone(),
        role_name: props
            .get("sso_role_name")
            .ok_or_else(|| invalid("sso_role_name"))?
            .clone(),
    })
}

pub(crate) fn cache_path(home: &Path, cache_key: &str) -> PathBuf {
    let digest = Sha1::digest(cache_key.trim().as_bytes());
    home.join(".aws")
        .join("sso")
        .join("cache")
        .join(format!("{}.json", hex::encode(digest)))
}

/// A cached SSO token: the JSON document plus the fields this module reads.
struct CachedToken {
    doc: Value,
    access_token: String,
    expires_at: SystemTime,
}

impl CachedToken {
    fn parse(text: &str) -> Option<Self> {
        let doc: Value = serde_json::from_str(text).ok()?;
        let access_token = doc.get("accessToken")?.as_str()?.to_string();
        let expires_at = parse_time(doc.get("expiresAt")?.as_str()?)?;
        Some(Self {
            doc,
            access_token,
            expires_at,
        })
    }

    fn field(&self, key: &str) -> Option<&str> {
        self.doc.get(key)?.as_str().filter(|s| !s.is_empty())
    }
}

pub(crate) async fn credentials(
    ctx: &Context,
    files: &ParsedFiles,
    props: &Props,
) -> Result<Creds, CredsError> {
    let cfg = config(files, props)?;
    let home = ctx
        .env
        .home_dir()
        .ok_or_else(|| CredsError::InvalidConfig("no home directory for the SSO cache".into()))?;
    let path = cache_path(&home, &cfg.cache_key);

    let expired =
        || CredsError::Expired(format!("SSO session expired or not found: {EXPIRED_HINT}"));
    let text = std::fs::read_to_string(&path).map_err(|_| expired())?;
    let mut token = CachedToken::parse(&text).ok_or_else(expired)?;

    let now = ctx.now();
    if token.expires_at <= now + REFRESH_WINDOW {
        match refresh(ctx, &cfg, &token, &path, now).await {
            Ok(refreshed) => token = refreshed,
            // A token that is still valid keeps working when the refresh fails.
            Err(_) if token.expires_at > now => {}
            Err(CredsError::Timeout(detail)) => return Err(CredsError::Timeout(detail)),
            Err(_) => return Err(expired()),
        }
    }

    role_credentials(ctx, &cfg, &token.access_token).await
}

/// `CreateToken` with `grant_type=refresh_token`; on success rewrites the cache file.
async fn refresh(
    ctx: &Context,
    cfg: &SsoConfig,
    token: &CachedToken,
    path: &Path,
    now: SystemTime,
) -> Result<CachedToken, CredsError> {
    let (Some(refresh_token), Some(client_id), Some(client_secret)) = (
        token.field("refreshToken"),
        token.field("clientId"),
        token.field("clientSecret"),
    ) else {
        return Err(CredsError::Expired(format!(
            "SSO token cannot be refreshed: {EXPIRED_HINT}"
        )));
    };
    // The region the client was registered in is the one that must serve the refresh.
    let region = token.field("region").unwrap_or(&cfg.region);
    let endpoint = service_endpoint(&ctx.env, "SSO_OIDC", || {
        format!("https://oidc.{region}.{}", dns_suffix(region))
    });
    let body = json!({
        "clientId": client_id,
        "clientSecret": client_secret,
        "grantType": "refresh_token",
        "refreshToken": refresh_token,
    })
    .to_string();
    let request = ctx
        .http
        .post(format!("{endpoint}/token"))
        .header("content-type", "application/json")
        .body(body);
    let response = http::send(request, "the SSO OIDC service", http::DEFAULT_TIMEOUT).await?;
    if !response.is_success() {
        // 400 `invalid_grant` / `expired_token` means the refresh token itself is spent.
        return Err(if response.status == 400 {
            CredsError::Expired(format!("SSO refresh token rejected: {EXPIRED_HINT}"))
        } else {
            CredsError::Provider(format!(
                "the SSO OIDC service answered HTTP {}",
                response.status
            ))
        });
    }
    let answer: Value = serde_json::from_str(&response.body)
        .map_err(|_| CredsError::Provider("the SSO OIDC answer is not valid JSON".into()))?;
    let access_token = answer
        .get("accessToken")
        .and_then(Value::as_str)
        .ok_or_else(|| CredsError::Provider("the SSO OIDC answer has no accessToken".into()))?
        .to_string();
    let expires_in = answer
        .get("expiresIn")
        .and_then(Value::as_u64)
        .unwrap_or(3600);
    let expires_at = now + Duration::from_secs(expires_in);

    let mut doc = token.doc.clone();
    doc["accessToken"] = Value::String(access_token.clone());
    doc["expiresAt"] = Value::String(format_time(expires_at));
    if let Some(new_refresh) = answer.get("refreshToken").and_then(Value::as_str) {
        doc["refreshToken"] = Value::String(new_refresh.to_string());
    }
    if write_cache(path, &doc).is_err() {
        // The CLI's cache is advisory for us; keep going with the in-memory token.
        log::warn!("Could not write the refreshed SSO token cache");
    }
    Ok(CachedToken {
        doc,
        access_token,
        expires_at,
    })
}

fn write_cache(path: &Path, doc: &Value) -> std::io::Result<()> {
    let text = serde_json::to_string(doc).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&tmp, path)
}

/// The portal's `GetRoleCredentials`.
async fn role_credentials(
    ctx: &Context,
    cfg: &SsoConfig,
    access_token: &str,
) -> Result<Creds, CredsError> {
    let endpoint = service_endpoint(&ctx.env, "SSO", || {
        format!(
            "https://portal.sso.{}.{}",
            cfg.region,
            dns_suffix(&cfg.region)
        )
    });
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("account_id", &cfg.account_id)
        .append_pair("role_name", &cfg.role_name)
        .finish();
    let request = ctx
        .http
        .get(format!("{endpoint}/federation/credentials?{query}"))
        .header("x-amz-sso_bearer_token", access_token);
    let response = http::send(request, "the SSO portal", http::DEFAULT_TIMEOUT).await?;
    if matches!(response.status, 401 | 403) {
        return Err(CredsError::Expired(format!(
            "the SSO portal rejected the access token: {EXPIRED_HINT}"
        )));
    }
    if !response.is_success() {
        return Err(CredsError::Provider(format!(
            "the SSO portal answered HTTP {}",
            response.status
        )));
    }
    let answer: Value = serde_json::from_str(&response.body)
        .map_err(|_| CredsError::Provider("the SSO portal answer is not valid JSON".into()))?;
    let role = answer.get("roleCredentials").ok_or_else(|| {
        CredsError::Provider("the SSO portal answer has no roleCredentials".into())
    })?;
    let text = |key: &str| role.get(key).and_then(Value::as_str).map(str::to_string);
    let (Some(access_key), Some(secret)) = (text("accessKeyId"), text("secretAccessKey")) else {
        return Err(CredsError::Provider(
            "the SSO role credentials are incomplete".into(),
        ));
    };
    let expires_at = role
        .get("expiration")
        .and_then(Value::as_i64)
        .filter(|ms| *ms >= 0)
        .map(|ms| SystemTime::UNIX_EPOCH + Duration::from_millis(ms as u64));
    Ok(Creds::new(
        access_key,
        secret,
        text("sessionToken"),
        expires_at,
    ))
}

#[cfg(test)]
#[path = "sso_tests.rs"]
mod tests;
