//! The AWS SDK credential chain, kept ONLY for `aws login` (`login_session`) profiles while the
//! `aws-sdk-login` cargo feature is on (it is on by default).
//!
//! [`super::login`] reimplements that flow without the SDK; this path exists so that nothing is
//! lost until the owner has verified the reimplementation against a real `aws login` profile.
//! With the feature off, `aws-config` and the `aws-sdk-*` crates leave the build entirely.
//!
//! The SDK's error `Display`/`Debug` output is deliberately not logged (it can carry provider
//! internals); only the mapped [`CredsError`] kind is.

use aws_credential_types::provider::ProvideCredentials;
use aws_credential_types::provider::error::CredentialsError;

use super::{Creds, CredsError};

/// Maps the SDK's error type onto the resolver's. The `String` details are fixed labels.
pub(crate) fn map_sdk_error(error: &CredentialsError) -> CredsError {
    match error {
        CredentialsError::CredentialsNotLoaded(_) => {
            CredsError::NotLoaded("the AWS SDK found no credentials".to_string())
        }
        CredentialsError::ProviderTimedOut(_) => {
            CredsError::Timeout("the AWS SDK credential provider timed out".to_string())
        }
        CredentialsError::InvalidConfiguration(_) => {
            CredsError::InvalidConfig("the AWS SDK rejected the profile configuration".to_string())
        }
        CredentialsError::ProviderError(_) => {
            CredsError::Provider("the AWS SDK credential provider failed".to_string())
        }
        _ => CredsError::Unhandled("the AWS SDK failed unexpectedly".to_string()),
    }
}

/// Loads credentials through `aws_config`'s default chain for `profile` (empty means the SDK
/// default: `AWS_PROFILE`, then `default`).
pub(crate) async fn load(profile: &str) -> Result<Creds, CredsError> {
    let region_provider = aws_config::meta::region::RegionProviderChain::default_provider();
    let loader =
        aws_config::defaults(aws_config::BehaviorVersion::latest()).region(region_provider);
    let loader = if profile.trim().is_empty() {
        loader
    } else {
        loader.profile_name(profile)
    };
    let config = loader.load().await;

    let provider = config.credentials_provider().ok_or_else(|| {
        CredsError::NotLoaded("the AWS SDK has no credentials provider".to_string())
    })?;
    let creds = provider
        .provide_credentials()
        .await
        .map_err(|e| map_sdk_error(&e))?;
    Ok(Creds::new(
        creds.access_key_id().to_string(),
        creds.secret_access_key().to_string(),
        creds.session_token().map(|s| s.to_string()),
        creds.expiry(),
    ))
}
