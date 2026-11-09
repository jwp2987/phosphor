//! AWS credential resolution for Amazon Bedrock, without the AWS SDK.
//!
//! The app does not sign Bedrock requests: it resolves credentials here and forwards them
//! (`ai::api_keys::ApiKeys.aws_credentials`). This module replaces the `aws-config` default
//! provider chain with a small resolver over the app's own HTTP client (so proxies and enterprise
//! CAs apply), covering every source the SDK chain did:
//!
//! * environment (`AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` / `AWS_SESSION_TOKEN`),
//! * shared `~/.aws/credentials` + `~/.aws/config` profiles (static keys, session tokens),
//! * IAM Identity Center / SSO (`ssooidc` refresh of the CLI's cache, portal `GetRoleCredentials`),
//! * AssumeRole (`role_arn` with `source_profile` or `credential_source`, SigV4-signed STS),
//! * web identity (`web_identity_token_file`, `AWS_WEB_IDENTITY_TOKEN_FILE`, unsigned STS),
//! * `credential_process`,
//! * ECS container credentials and EC2 IMDSv2,
//! * `aws login` (`login_session`, DPoP-bound refresh; see [`login`]).
//!
//! Chain order matches `aws-config`: an explicit profile resolves that profile only; otherwise
//! environment, then the profile named by `AWS_PROFILE` (or `default`), then web identity from the
//! environment, then ECS container credentials, then IMDS.
//!
//! Service endpoints honour `AWS_ENDPOINT_URL_<SERVICE>` / `AWS_ENDPOINT_URL` (also how the tests
//! point the resolver at local stubs).
//!
//! # Secrets
//!
//! Nothing here logs tokens, keys or response bodies. [`CredsError`] details are built from file
//! names, profile names, HTTP status codes and AWS error *codes* only; [`Creds`] has a redacting
//! `Debug`.

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ai::api_keys::AwsCredentials;

mod http;
mod imds;
mod ini;
mod login;
mod process;
mod profile;
mod sigv4;
mod sso;
mod sts;
#[cfg(test)]
mod test_support;

#[cfg(feature = "aws-sdk-login")]
pub(crate) mod legacy_sdk;

pub(crate) use ini::ParsedFiles;

/// Resolved credentials. `Debug` never prints the key material.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Creds {
    pub(crate) access_key_id: String,
    pub(crate) secret_access_key: String,
    pub(crate) session_token: Option<String>,
    pub(crate) expires_at: Option<SystemTime>,
}

impl fmt::Debug for Creds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Creds")
            .field("access_key_id", &"<redacted>")
            .field("secret_access_key", &"<redacted>")
            .field(
                "session_token",
                &self.session_token.as_ref().map(|_| "<redacted>"),
            )
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

impl Creds {
    pub(crate) fn new(
        access_key_id: impl Into<String>,
        secret_access_key: impl Into<String>,
        session_token: Option<String>,
        expires_at: Option<SystemTime>,
    ) -> Self {
        Self {
            access_key_id: access_key_id.into(),
            secret_access_key: secret_access_key.into(),
            session_token,
            expires_at,
        }
    }

    pub(crate) fn into_api(self) -> AwsCredentials {
        AwsCredentials::new(
            self.access_key_id,
            self.secret_access_key,
            self.session_token,
            self.expires_at,
        )
    }
}

/// Why credentials could not be resolved. The `String` is a short, secret-free detail for logs and
/// tests; the user-facing text is chosen by variant (`aws_credentials.rs`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CredsError {
    /// The requested profile does not exist.
    ProfileNotFound(String),
    /// This source has no credentials (the chain moves on to the next source).
    NotLoaded(String),
    /// A session exists but is expired and cannot be refreshed (e.g. run `aws sso login`).
    Expired(String),
    /// The profile or environment is malformed or incomplete.
    InvalidConfig(String),
    /// A provider took too long.
    Timeout(String),
    /// A provider answered with an error or could not be reached.
    Provider(String),
    /// The profile requires an MFA code, which cannot be prompted for here.
    MfaRequired(String),
    /// Anything else.
    Unhandled(String),
}

impl CredsError {
    /// A stable, secret-free label for logging.
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::ProfileNotFound(_) => "profile_not_found",
            Self::NotLoaded(_) => "not_loaded",
            Self::Expired(_) => "expired",
            Self::InvalidConfig(_) => "invalid_config",
            Self::Timeout(_) => "timeout",
            Self::Provider(_) => "provider",
            Self::MfaRequired(_) => "mfa_required",
            Self::Unhandled(_) => "unhandled",
        }
    }

    pub(crate) fn detail(&self) -> &str {
        match self {
            Self::ProfileNotFound(d)
            | Self::NotLoaded(d)
            | Self::Expired(d)
            | Self::InvalidConfig(d)
            | Self::Timeout(d)
            | Self::Provider(d)
            | Self::MfaRequired(d)
            | Self::Unhandled(d) => d,
        }
    }

    /// True when the chain should try its next source rather than fail.
    fn is_not_found(&self) -> bool {
        matches!(self, Self::ProfileNotFound(_) | Self::NotLoaded(_))
    }
}

impl fmt::Display for CredsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind(), self.detail())
    }
}

impl std::error::Error for CredsError {}

/// A snapshot of the process environment (or a test fixture). Never `Debug`-prints its values: the
/// environment can hold `AWS_SECRET_ACCESS_KEY`.
#[derive(Clone, Default)]
pub(crate) struct Env {
    vars: HashMap<String, String>,
    /// Fall back to the OS home directory when `HOME` / `USERPROFILE` are absent. Off for test
    /// fixtures so a test can never read the developer's real `~/.aws`.
    fallback_home: bool,
}

impl fmt::Debug for Env {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Env")
            .field("vars", &self.vars.len())
            .finish()
    }
}

impl Env {
    pub(crate) fn from_process() -> Self {
        let vars = std::env::vars_os()
            .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
            .collect();
        Self {
            vars,
            fallback_home: true,
        }
    }

    #[cfg(test)]
    pub(crate) fn from_pairs(pairs: &[(&str, &str)]) -> Self {
        Self {
            vars: pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            fallback_home: false,
        }
    }

    /// The variable's trimmed value; unset and empty are the same.
    pub(crate) fn get(&self, key: &str) -> Option<&str> {
        self.vars
            .get(key)
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
    }

    pub(crate) fn home_dir(&self) -> Option<PathBuf> {
        self.get("HOME")
            .or_else(|| self.get("USERPROFILE"))
            .map(PathBuf::from)
            .or_else(|| self.fallback_home.then(dirs::home_dir).flatten())
    }

    /// `AWS_REGION`, then `AWS_DEFAULT_REGION`.
    pub(crate) fn region(&self) -> Option<String> {
        self.get("AWS_REGION")
            .or_else(|| self.get("AWS_DEFAULT_REGION"))
            .map(str::to_string)
    }
}

/// Everything a resolution needs: the environment, the HTTP client, and (for tests) a frozen clock.
pub(crate) struct Context {
    pub(crate) env: Env,
    pub(crate) http: Arc<http_client::Client>,
    now_override: Option<SystemTime>,
}

impl Context {
    pub(crate) fn new() -> Self {
        Self {
            env: Env::from_process(),
            http: Arc::new(http_client::Client::new()),
            now_override: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test(env: Env, now: Option<SystemTime>) -> Self {
        Self {
            env,
            http: Arc::new(http_client::Client::new_for_test()),
            now_override: now,
        }
    }

    pub(crate) fn now(&self) -> SystemTime {
        self.now_override.unwrap_or_else(SystemTime::now)
    }
}

/// Refresh a cached session this long before it expires.
pub(crate) const REFRESH_WINDOW: Duration = Duration::from_secs(5 * 60);

/// The DNS suffix for a region's partition.
pub(crate) fn dns_suffix(region: &str) -> &'static str {
    if region.starts_with("cn-") {
        "amazonaws.com.cn"
    } else {
        "amazonaws.com"
    }
}

/// The service endpoint: `AWS_ENDPOINT_URL_<SERVICE>`, then `AWS_ENDPOINT_URL`, then `default()`.
/// No trailing slash.
pub(crate) fn service_endpoint(
    env: &Env,
    service_env: &str,
    default: impl FnOnce() -> String,
) -> String {
    let url = env
        .get(&format!("AWS_ENDPOINT_URL_{service_env}"))
        .or_else(|| env.get("AWS_ENDPOINT_URL"))
        .map(str::to_string)
        .unwrap_or_else(default);
    url.trim_end_matches('/').to_string()
}

/// RFC 3339 (also the CLI's older `...UTC` spelling) to a `SystemTime`.
pub(crate) fn parse_time(text: &str) -> Option<SystemTime> {
    let text = text.trim();
    let normalized = text
        .strip_suffix("UTC")
        .map(|t| format!("{t}Z"))
        .unwrap_or_else(|| text.to_string());
    chrono::DateTime::parse_from_rfc3339(&normalized)
        .ok()
        .map(SystemTime::from)
}

pub(crate) fn format_time(time: SystemTime) -> String {
    chrono::DateTime::<chrono::Utc>::from(time)
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string()
}

pub(crate) fn unix_secs(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Credentials from `AWS_ACCESS_KEY_ID` + `AWS_SECRET_ACCESS_KEY` (+ `AWS_SESSION_TOKEN`, or the
/// legacy `AWS_SECURITY_TOKEN`). One half set is "not configured", as in the SDK.
pub(crate) fn env_credentials(env: &Env) -> Option<Creds> {
    let key = env.get("AWS_ACCESS_KEY_ID")?;
    let secret = env.get("AWS_SECRET_ACCESS_KEY")?;
    let token = env
        .get("AWS_SESSION_TOKEN")
        .or_else(|| env.get("AWS_SECURITY_TOKEN"))
        .map(str::to_string);
    Some(Creds::new(key, secret, token, None))
}

/// Resolves credentials.
///
/// * A non-empty `profile` resolves that profile only (no environment, no IMDS).
/// * An empty one runs the default chain: environment, `AWS_PROFILE` / `default`, web identity from
///   the environment, ECS container credentials, IMDS.
pub(crate) async fn resolve(ctx: &Context, profile: &str) -> Result<Creds, CredsError> {
    let explicit = profile.trim();
    if !explicit.is_empty() {
        return profile::resolve_profile(ctx, explicit).await;
    }

    if let Some(creds) = env_credentials(&ctx.env) {
        return Ok(creds);
    }

    let (profile_name, profile_is_explicit) = match ctx.env.get("AWS_PROFILE") {
        Some(name) => (name, true),
        None => ("default", false),
    };
    match profile::resolve_profile(ctx, profile_name).await {
        Ok(creds) => return Ok(creds),
        // An absent `default` profile just means "keep looking"; a named one that is absent is a
        // configuration mistake worth reporting.
        Err(err)
            if err.is_not_found()
                && !(profile_is_explicit && matches!(err, CredsError::ProfileNotFound(_))) => {}
        Err(err) => return Err(err),
    }

    if let Some(creds) = sts::web_identity_from_env(ctx).await? {
        return Ok(creds);
    }
    match imds::container(ctx).await {
        Ok(creds) => return Ok(creds),
        Err(err) if err.is_not_found() => {}
        Err(err) => return Err(err),
    }
    match imds::imds(ctx).await {
        Ok(creds) => Ok(creds),
        Err(err) if err.is_not_found() => Err(CredsError::NotLoaded(
            "no credential source produced credentials".to_string(),
        )),
        Err(err) => Err(err),
    }
}

/// The OIDC-managed path (Oz task identity): unsigned STS `AssumeRoleWithWebIdentity` in the
/// default region (`AWS_REGION` / `AWS_DEFAULT_REGION`, then the default profile's `region`, then
/// `us-east-1`).
pub(crate) async fn assume_role_with_web_identity(
    ctx: &Context,
    role_arn: &str,
    session_name: &str,
    web_identity_token: &str,
) -> Result<Creds, CredsError> {
    let region = default_region(&ctx.env);
    sts::assume_role_with_web_identity(
        ctx,
        &region,
        role_arn,
        session_name,
        web_identity_token,
        None,
    )
    .await
}

fn default_region(env: &Env) -> String {
    env.region()
        .or_else(|| {
            let files = ParsedFiles::load(env);
            let name = env.get("AWS_PROFILE").unwrap_or("default");
            files.profiles.get(name)?.get("region").cloned()
        })
        .unwrap_or_else(|| "us-east-1".to_string())
}

/// Whether `profile` (or the default chain's profile) needs the legacy SDK because it, or a
/// profile it assumes a role from, is an `aws login` (`login_session`) profile. Only meaningful
/// while the `aws-sdk-login` feature keeps that path.
#[cfg(any(feature = "aws-sdk-login", test))]
pub(crate) fn needs_legacy_login(env: &Env, profile: &str) -> bool {
    let explicit = profile.trim();
    let name = if !explicit.is_empty() {
        explicit.to_string()
    } else if env_credentials(env).is_some() {
        return false;
    } else {
        env.get("AWS_PROFILE").unwrap_or("default").to_string()
    };
    let files = ParsedFiles::load(env);
    let mut seen: Vec<String> = Vec::new();
    let mut current = name;
    loop {
        if seen.contains(&current) {
            return false;
        }
        let Some(props) = files.profiles.get(&current) else {
            return false;
        };
        seen.push(current);
        if props.contains_key("login_session") {
            return true;
        }
        match props.get("source_profile") {
            Some(next) if props.contains_key("role_arn") => current = next.clone(),
            _ => return false,
        }
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
