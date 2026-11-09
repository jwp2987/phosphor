use super::user_facing_aws_credentials_error_message;
use crate::ai::aws_creds::CredsError;

#[test]
fn maps_credentials_not_loaded_to_user_message() {
    let expected = "AWS credentials were not found for the AWS profile `sandbox`. Log in with the AWS CLI or update your AWS credentials configuration, then refresh.";
    for err in [
        CredsError::NotLoaded("x".into()),
        CredsError::ProfileNotFound("x".into()),
    ] {
        assert_eq!(
            user_facing_aws_credentials_error_message(&err, "sandbox"),
            expected
        );
    }
}

#[test]
fn names_the_default_profile_when_none_is_set() {
    assert_eq!(
        user_facing_aws_credentials_error_message(&CredsError::NotLoaded("x".into()), "  "),
        "AWS credentials were not found for the default AWS profile. Log in with the AWS CLI or update your AWS credentials configuration, then refresh."
    );
}

#[test]
fn maps_invalid_configuration_to_user_message() {
    let message = user_facing_aws_credentials_error_message(
        &CredsError::InvalidConfig("bad config".into()),
        "readonly",
    );
    assert_eq!(
        message,
        "The AWS profile `readonly` is invalid or incomplete in your local AWS configuration. Update your AWS profile settings and credentials, then refresh."
    );
}

#[test]
fn maps_provider_timeout_to_user_message() {
    let message =
        user_facing_aws_credentials_error_message(&CredsError::Timeout("t".into()), "sandbox");
    assert_eq!(
        message,
        "Timed out while loading AWS credentials. Refresh and try again."
    );
}

#[test]
fn maps_provider_error_and_expired_session_to_user_message() {
    let expected = "Unable to load AWS credentials from your configured provider. Refresh your AWS login and try again.";
    for err in [
        CredsError::Provider("p".into()),
        CredsError::Expired("e".into()),
    ] {
        assert_eq!(
            user_facing_aws_credentials_error_message(&err, "sandbox"),
            expected
        );
    }
}

#[test]
fn maps_unhandled_error_to_user_message() {
    let message =
        user_facing_aws_credentials_error_message(&CredsError::Unhandled("u".into()), "sandbox");
    assert_eq!(
        message,
        "Unexpected error while loading AWS credentials. Refresh your AWS login and try again."
    );
}

#[test]
fn maps_mfa_to_a_specific_message() {
    let message =
        user_facing_aws_credentials_error_message(&CredsError::MfaRequired("m".into()), "mfa");
    assert!(message.starts_with("The AWS profile `mfa` requires an MFA code"));
}

#[test]
fn user_messages_never_echo_the_error_detail() {
    let message = user_facing_aws_credentials_error_message(
        &CredsError::Provider("SECRET-DETAIL".into()),
        "sandbox",
    );
    assert!(!message.contains("SECRET-DETAIL"));
}

/// The legacy SDK path keeps the pre-existing mapping from the SDK's error type.
#[cfg(feature = "aws-sdk-login")]
mod legacy {
    use std::time::Duration;

    use aws_credential_types::provider::error::CredentialsError;

    use super::*;
    use crate::ai::aws_creds::legacy_sdk::map_sdk_error;

    fn message(err: &CredentialsError) -> String {
        user_facing_aws_credentials_error_message(&map_sdk_error(err), "sandbox")
    }

    #[test]
    fn sdk_errors_keep_their_user_messages() {
        assert!(
            message(&CredentialsError::not_loaded_no_source())
                .starts_with("AWS credentials were not found")
        );
        assert!(
            message(&CredentialsError::invalid_configuration(
                std::io::Error::other("bad")
            ))
            .contains("is invalid or incomplete")
        );
        assert_eq!(
            message(&CredentialsError::provider_timed_out(Duration::from_secs(
                5
            ))),
            "Timed out while loading AWS credentials. Refresh and try again."
        );
        assert!(
            message(&CredentialsError::provider_error(std::io::Error::other(
                "p"
            )))
            .starts_with("Unable to load AWS credentials from your configured provider")
        );
        assert!(
            message(&CredentialsError::unhandled(std::io::Error::other("u")))
                .starts_with("Unexpected error")
        );
    }

    #[test]
    fn sdk_error_details_are_fixed_labels() {
        let err = map_sdk_error(&CredentialsError::provider_error(std::io::Error::other(
            "SECRET",
        )));
        assert!(!err.detail().contains("SECRET"));
    }
}
