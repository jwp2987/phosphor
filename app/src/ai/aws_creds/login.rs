//! `aws login` (`login_session` profiles): the CLI caches console-login credentials under
//! `~/.aws/login/cache/<sha256(session arn)>.json` together with a refresh token and the P-256 key
//! that token is bound to. When the access credentials are close to expiring they are refreshed at
//! the sign-in service with a DPoP proof (RFC 9449): an ES256 JWT, signed by that key, naming the
//! HTTP method, the target URL, a unique id and the time.
//!
//! This reimplements what `aws-config`'s `credentials-login` feature does. Until the owner has
//! verified it against a real `aws login` profile the `aws-sdk-login` cargo feature (default on)
//! keeps routing those profiles through the SDK; see [`super::legacy_sdk`].

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use p256::SecretKey;
use p256::ecdsa::signature::Signer as _;
use p256::ecdsa::{Signature, SigningKey};
use p256::elliptic_curve::sec1::ToSec1Point as _;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::ini::Props;
use super::{
    Context, Creds, CredsError, REFRESH_WINDOW, format_time, http, parse_time, service_endpoint,
    unix_secs,
};

const EXPIRED_HINT: &str = "run `aws login`";

fn expired() -> CredsError {
    CredsError::Expired(format!(
        "the `aws login` session expired or was not found: {EXPIRED_HINT}"
    ))
}

pub(crate) fn cache_dir(ctx: &Context) -> Option<PathBuf> {
    match ctx.env.get("AWS_LOGIN_CACHE_DIRECTORY") {
        Some(dir) => Some(PathBuf::from(dir)),
        None => ctx
            .env
            .home_dir()
            .map(|h| h.join(".aws").join("login").join("cache")),
    }
}

pub(crate) fn cache_path(dir: &std::path::Path, login_session: &str) -> PathBuf {
    dir.join(format!(
        "{}.json",
        hex::encode(Sha256::digest(login_session.trim().as_bytes()))
    ))
}

/// The sign-in endpoint for a region.
fn signin_endpoint(ctx: &Context, region: &str) -> String {
    service_endpoint(&ctx.env, "SIGNIN", || {
        if region.starts_with("cn-") {
            format!("https://{region}.signin.amazonaws.cn")
        } else if region.starts_with("us-gov-") {
            format!("https://{region}.signin.amazonaws-us-gov.com")
        } else {
            format!("https://{region}.signin.aws.amazon.com")
        }
    })
}

/// Builds a DPoP proof JWT for a `POST` to `htu`, signed by the SEC1 PEM `key_pem`.
pub(crate) fn dpop_proof(
    key_pem: &str,
    htu: &str,
    iat: u64,
    jti: &str,
) -> Result<String, CredsError> {
    let invalid =
        || CredsError::InvalidConfig("the `aws login` DPoP key is not a P-256 key".into());
    let secret = SecretKey::from_sec1_pem(key_pem).map_err(|_| invalid())?;
    let point = secret.public_key().to_sec1_point(false);
    let (Some(x), Some(y)) = (point.x(), point.y()) else {
        return Err(invalid());
    };
    let x: &[u8] = x.as_ref();
    let y: &[u8] = y.as_ref();

    let header = json!({
        "typ": "dpop+jwt",
        "alg": "ES256",
        "jwk": {
            "kty": "EC",
            "x": URL_SAFE_NO_PAD.encode(x),
            "y": URL_SAFE_NO_PAD.encode(y),
            "crv": "P-256",
        },
    });
    let payload = json!({ "jti": jti, "htm": "POST", "htu": htu, "iat": iat });
    let message = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(header.to_string()),
        URL_SAFE_NO_PAD.encode(payload.to_string())
    );

    let signing_key = SigningKey::from(&secret);
    let signature: Signature = signing_key.sign(message.as_bytes());
    let signature = signature.to_bytes();
    let signature: &[u8] = signature.as_ref();
    Ok(format!("{message}.{}", URL_SAFE_NO_PAD.encode(signature)))
}

struct CachedLogin {
    doc: Value,
    expires_at: SystemTime,
}

impl CachedLogin {
    fn parse(text: &str) -> Option<Self> {
        let doc: Value = serde_json::from_str(text).ok()?;
        let access = doc.get("accessToken")?;
        // The access credentials must be complete for this to be a usable cache entry.
        for key in ["accessKeyId", "secretAccessKey", "sessionToken"] {
            access.get(key)?.as_str()?;
        }
        let expires_at = parse_time(access.get("expiresAt")?.as_str()?)?;
        Some(Self { doc, expires_at })
    }

    fn string(&self, key: &str) -> Option<&str> {
        self.doc.get(key)?.as_str().filter(|s| !s.is_empty())
    }

    fn creds(&self) -> Option<Creds> {
        let access = self.doc.get("accessToken")?;
        let text = |key: &str| access.get(key)?.as_str().map(str::to_string);
        Some(Creds::new(
            text("accessKeyId")?,
            text("secretAccessKey")?,
            text("sessionToken"),
            Some(self.expires_at),
        ))
    }
}

pub(crate) async fn credentials(ctx: &Context, props: &Props) -> Result<Creds, CredsError> {
    let session = props
        .get("login_session")
        .ok_or_else(|| CredsError::InvalidConfig("the profile has no login_session".into()))?;
    let dir = cache_dir(ctx)
        .ok_or_else(|| CredsError::InvalidConfig("no home directory for the login cache".into()))?;
    let path = cache_path(&dir, session);
    let text = std::fs::read_to_string(&path).map_err(|_| expired())?;
    let mut cached = CachedLogin::parse(&text).ok_or_else(|| {
        CredsError::InvalidConfig("the `aws login` cache entry is incomplete".into())
    })?;

    let now = ctx.now();
    if cached.expires_at <= now + REFRESH_WINDOW {
        let region = props
            .get("region")
            .cloned()
            .or_else(|| ctx.env.region())
            .unwrap_or_else(|| "us-east-1".to_string());
        match refresh(ctx, &cached, &region, &path, now).await {
            Ok(refreshed) => cached = refreshed,
            // Still-valid credentials keep working when the refresh fails.
            Err(_) if cached.expires_at > now => {}
            Err(CredsError::Timeout(detail)) => return Err(CredsError::Timeout(detail)),
            Err(_) => return Err(expired()),
        }
    }
    cached.creds().ok_or_else(|| {
        CredsError::InvalidConfig("the `aws login` cache entry is incomplete".into())
    })
}

async fn refresh(
    ctx: &Context,
    cached: &CachedLogin,
    region: &str,
    path: &std::path::Path,
    now: SystemTime,
) -> Result<CachedLogin, CredsError> {
    let (Some(refresh_token), Some(client_id), Some(dpop_key)) = (
        cached.string("refreshToken"),
        cached.string("clientId"),
        cached.string("dpopKey"),
    ) else {
        return Err(expired());
    };
    // The cache holds the PEM with real newlines (JSON-decoded); tolerate a literal `\n` too.
    let key_pem = dpop_key.replace("\\n", "\n");

    let url = format!("{}/v1/token", signin_endpoint(ctx, region));
    let proof = dpop_proof(
        &key_pem,
        &url,
        unix_secs(now),
        &uuid::Uuid::new_v4().to_string(),
    )?;
    let body = json!({
        "clientId": client_id,
        "grantType": "refresh_token",
        "refreshToken": refresh_token,
    })
    .to_string();
    let request = ctx
        .http
        .post(url.as_str())
        .header("content-type", "application/json")
        .header("dpop", proof.as_str())
        .body(body);
    let response = http::send(request, "the sign-in service", http::DEFAULT_TIMEOUT).await?;
    if !response.is_success() {
        return Err(if matches!(response.status, 400 | 401 | 403) {
            CredsError::Expired(format!(
                "the sign-in service rejected the refresh: {EXPIRED_HINT}"
            ))
        } else {
            CredsError::Provider(format!(
                "the sign-in service answered HTTP {}",
                response.status
            ))
        });
    }

    let answer: Value = serde_json::from_str(&response.body)
        .map_err(|_| CredsError::Provider("the sign-in answer is not valid JSON".into()))?;
    let access = answer
        .get("accessToken")
        .ok_or_else(|| CredsError::Provider("the sign-in answer has no accessToken".into()))?;
    let field = |key: &str| access.get(key).and_then(Value::as_str);
    let (Some(access_key), Some(secret), Some(session_token)) = (
        field("accessKeyId"),
        field("secretAccessKey"),
        field("sessionToken"),
    ) else {
        return Err(CredsError::Provider(
            "the sign-in answer has incomplete credentials".into(),
        ));
    };
    let new_refresh = answer
        .get("refreshToken")
        .and_then(Value::as_str)
        .ok_or_else(|| CredsError::Provider("the sign-in answer has no refreshToken".into()))?;
    let expires_in = answer
        .get("expiresIn")
        .and_then(Value::as_u64)
        .unwrap_or(900);
    let expires_at = now + Duration::from_secs(expires_in);

    let mut doc = cached.doc.clone();
    doc["accessToken"]["accessKeyId"] = Value::String(access_key.to_string());
    doc["accessToken"]["secretAccessKey"] = Value::String(secret.to_string());
    doc["accessToken"]["sessionToken"] = Value::String(session_token.to_string());
    doc["accessToken"]["expiresAt"] = Value::String(format_time(expires_at));
    doc["refreshToken"] = Value::String(new_refresh.to_string());
    if write_cache(path, &doc).is_err() {
        log::warn!("Could not write the refreshed `aws login` cache");
    }
    Ok(CachedLogin { doc, expires_at })
}

fn write_cache(path: &std::path::Path, doc: &Value) -> std::io::Result<()> {
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

#[cfg(test)]
#[path = "login_tests.rs"]
mod tests;
