use crate::ai::byop_readiness::BlockedByopReadinessError;
use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use warp_core::errors::{AnyhowErrorExt, ErrorExt};
use warp_core::register_error;

const WARP_ERROR_CODE_HEADER: &str = "X-Zap-Error-Code";
const WARP_ERROR_CODE_OUT_OF_CREDITS: &str = "OUT_OF_CREDITS";

#[derive(thiserror::Error, Debug, Serialize, Deserialize)]
#[error("{error}")]
pub struct ClientError {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_url: Option<String>,
}

#[derive(thiserror::Error, Debug)]
pub enum DeserializationError {
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Transport(reqwest::Error),
}

/// Longest provider-requested `Retry-After` wait we will honour for a single wait. The bound is
/// per wait, not per recovery: the agent driver re-arms its deadline on every transient error,
/// so a series of capped waits is not cut short by `AUTO_RESUME_TIMEOUT` (120s) as long as each
/// one stays well under it.
pub(crate) const MAX_RETRY_AFTER: Duration = Duration::from_secs(60);

/// Parses a `Retry-After` header value (RFC 9110 §10.2.3): either delta-seconds or an
/// HTTP-date. Returns `None` for absent-equivalent input -- malformed, zero/negative, or a
/// date that is not in the future relative to `now` -- so callers fall back to their own
/// backoff. Larger values are clamped to [`MAX_RETRY_AFTER`].
pub(crate) fn parse_retry_after(
    value: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<Duration> {
    let value = value.trim();
    let delay = if let Ok(secs) = value.parse::<u64>() {
        Duration::from_secs(secs)
    } else if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) {
        // All digits but too large for u64: a huge delay, not a malformed one. Clamp it.
        MAX_RETRY_AFTER
    } else if value.starts_with(|c: char| c.is_ascii_digit() || c == '-' || c == '+') {
        // Numeric-looking but not delta-seconds (negative, fractional, junk).
        return None;
    } else {
        let date = chrono::DateTime::parse_from_rfc2822(value).ok()?;
        (date.with_timezone(&chrono::Utc) - now).to_std().ok()?
    };
    if delay.is_zero() {
        return None;
    }
    Some(delay.min(MAX_RETRY_AFTER))
}

#[derive(thiserror::Error, Debug)]
pub enum AIApiError {
    #[error("Request failed due to lack of AI quota.")]
    QuotaLimit,

    #[error("Phosphor is currently overloaded. Please try again later.")]
    ServerOverloaded,

    #[error("Internal error occurred at transport layer.")]
    Transport(#[source] reqwest::Error),

    #[error("Failed to deserialize API response.")]
    Deserialization(#[source] DeserializationError),

    #[error("No context found on context search.")]
    NoContextFound,

    /// A non-success HTTP status from the provider, its message, and the provider's parsed
    /// `Retry-After` delay (already clamped to [`MAX_RETRY_AFTER`]) when it sent a usable one.
    #[error("Failed with status code {0}: {1}")]
    ErrorStatus(http::StatusCode, String, Option<Duration>),

    #[error(transparent)]
    Other(#[from] anyhow::Error),

    #[error("Got error when streaming {stream_type}: {source:#}")]
    Stream {
        stream_type: &'static str,
        #[source]
        source: anyhow::Error,
    },
}

impl From<http_client::ResponseError> for AIApiError {
    fn from(err: http_client::ResponseError) -> Self {
        Self::from_response_error(err.source, &err.headers)
    }
}

impl From<reqwest::Error> for AIApiError {
    fn from(err: reqwest::Error) -> Self {
        Self::from_transport_error(err)
    }
}

impl From<serde_json::Error> for AIApiError {
    fn from(err: serde_json::Error) -> Self {
        AIApiError::Deserialization(err.into())
    }
}

impl AIApiError {
    fn from_response_error(err: reqwest::Error, headers: &::http::HeaderMap) -> Self {
        if err.status() == Some(http::StatusCode::TOO_MANY_REQUESTS) {
            return Self::error_for_429(headers);
        }

        Self::from_transport_error(err)
    }

    fn from_transport_error(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            return AIApiError::Transport(err);
        }
        if err.is_decode() {
            #[cfg(not(target_family = "wasm"))]
            {
                use std::error::Error as _;
                let mut source = err.source();
                while let Some(underlying) = source {
                    if underlying.is::<hyper::Error>() {
                        return AIApiError::Transport(err);
                    }

                    source = underlying.source();
                }
            }

            return AIApiError::Deserialization(DeserializationError::Transport(err));
        }

        AIApiError::Transport(err)
    }

    fn error_for_429(headers: &::http::HeaderMap) -> Self {
        if headers
            .get(WARP_ERROR_CODE_HEADER)
            .and_then(|v| v.to_str().ok())
            == Some(WARP_ERROR_CODE_OUT_OF_CREDITS)
        {
            AIApiError::QuotaLimit
        } else {
            AIApiError::ServerOverloaded
        }
    }

    pub(crate) async fn from_stream_error(
        stream_type: &'static str,
        err: reqwest_eventsource::Error,
    ) -> Self {
        match err {
            reqwest_eventsource::Error::InvalidStatusCode(
                http::StatusCode::TOO_MANY_REQUESTS,
                ref res,
            ) => Self::error_for_429(res.headers()),
            reqwest_eventsource::Error::InvalidStatusCode(status, res) => {
                let retry_after = res
                    .headers()
                    .get(::http::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| parse_retry_after(v, chrono::Utc::now()));
                Self::ErrorStatus(
                    status,
                    res.text()
                        .await
                        .unwrap_or_else(|e| format!("(no response body: {e:#})")),
                    retry_after,
                )
            }
            reqwest_eventsource::Error::Transport(err) => Self::from_transport_error(err),
            err => AIApiError::Stream {
                stream_type,
                #[cfg(target_family = "wasm")]
                source: anyhow!("{err:#?}"),
                #[cfg(not(target_family = "wasm"))]
                source: anyhow!(err),
            },
        }
    }

    /// The provider's requested wait before the next attempt (`Retry-After`), if it sent a
    /// usable one. Already clamped to [`MAX_RETRY_AFTER`].
    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            AIApiError::ErrorStatus(_, _, retry_after) => *retry_after,
            _ => None,
        }
    }

    pub fn is_retryable(&self) -> bool {
        fn is_retryable_status(status: http::StatusCode) -> bool {
            !status.is_client_error()
                || status == http::StatusCode::REQUEST_TIMEOUT
                || status == http::StatusCode::TOO_MANY_REQUESTS
        }

        match self {
            AIApiError::ErrorStatus(status, _, _) => is_retryable_status(*status),
            AIApiError::Transport(e) => {
                if let Some(status) = e.status() {
                    return is_retryable_status(status);
                }
                true
            }
            AIApiError::QuotaLimit
            | AIApiError::ServerOverloaded
            | AIApiError::Deserialization(_)
            | AIApiError::NoContextFound
            | AIApiError::Stream { .. } => true,
            AIApiError::Other(error) => error.downcast_ref::<BlockedByopReadinessError>().is_none(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::byop_readiness::{BlockedByopReadinessError, ReadinessCategory};

    #[test]
    fn byop_blocked_readiness_error_is_not_retryable() {
        let error = AIApiError::Other(
            BlockedByopReadinessError::new(ReadinessCategory::MissingResultWithoutRepairSource)
                .into(),
        );

        assert!(!error.is_retryable());
    }

    fn now() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc2822("Wed, 21 Oct 2015 07:28:00 GMT")
            .unwrap()
            .with_timezone(&chrono::Utc)
    }

    #[test]
    fn retry_after_delta_seconds() {
        assert_eq!(parse_retry_after("7", now()), Some(Duration::from_secs(7)));
        assert_eq!(
            parse_retry_after(" 7 ", now()),
            Some(Duration::from_secs(7))
        );
    }

    #[test]
    fn retry_after_http_date() {
        assert_eq!(
            parse_retry_after("Wed, 21 Oct 2015 07:28:30 GMT", now()),
            Some(Duration::from_secs(30))
        );
    }

    #[test]
    fn retry_after_http_date_in_the_past_is_ignored() {
        assert_eq!(
            parse_retry_after("Wed, 21 Oct 2015 07:27:00 GMT", now()),
            None
        );
        assert_eq!(
            parse_retry_after("Wed, 21 Oct 2015 07:28:00 GMT", now()),
            None
        );
    }

    #[test]
    fn retry_after_malformed_is_ignored() {
        for v in ["", "soon", "1.5", "12abc", "Tomorrow, 99 Foo", "-"] {
            assert_eq!(parse_retry_after(v, now()), None, "{v:?}");
        }
    }

    #[test]
    fn retry_after_zero_and_negative_are_ignored() {
        assert_eq!(parse_retry_after("0", now()), None);
        assert_eq!(parse_retry_after("-5", now()), None);
    }

    #[test]
    fn retry_after_over_cap_is_clamped() {
        assert_eq!(parse_retry_after("3600", now()), Some(MAX_RETRY_AFTER));
        // Overflows u64 but is still a (huge) delay-seconds value: clamp, do not drop.
        assert_eq!(
            parse_retry_after("99999999999999999999", now()),
            Some(MAX_RETRY_AFTER)
        );
        assert_eq!(
            parse_retry_after("Thu, 22 Oct 2015 07:28:00 GMT", now()),
            Some(MAX_RETRY_AFTER)
        );
    }

    #[test]
    fn error_status_exposes_retry_after() {
        let with = AIApiError::ErrorStatus(
            http::StatusCode::TOO_MANY_REQUESTS,
            "slow down".into(),
            Some(Duration::from_secs(9)),
        );
        assert_eq!(with.retry_after(), Some(Duration::from_secs(9)));
        assert!(with.is_retryable());
        let without =
            AIApiError::ErrorStatus(http::StatusCode::TOO_MANY_REQUESTS, String::new(), None);
        assert_eq!(without.retry_after(), None);
    }
}

impl ErrorExt for AIApiError {
    fn is_actionable(&self) -> bool {
        match self {
            AIApiError::Deserialization(_) => true,
            AIApiError::Transport(error) => error.is_actionable(),
            AIApiError::Other(error) => error.is_actionable(),
            AIApiError::Stream { source, .. } => source.is_actionable(),
            AIApiError::ErrorStatus(_, _, _) => self.is_retryable(),
            AIApiError::QuotaLimit | AIApiError::ServerOverloaded | AIApiError::NoContextFound => {
                false
            }
        }
    }
}

register_error!(AIApiError);
