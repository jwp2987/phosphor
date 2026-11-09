//! A thin layer over the app's `http_client::Client` that returns the status and body text and maps
//! transport failures to [`CredsError`] without echoing URLs or bodies.

use std::time::Duration;

use super::CredsError;

pub(crate) const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// A completed HTTP exchange. `body` is secret-bearing: callers parse it and drop it, and it is
/// never logged.
pub(crate) struct HttpResponse {
    pub(crate) status: u16,
    pub(crate) body: String,
}

impl HttpResponse {
    pub(crate) fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// Sends `request` and reads the whole body. `what` names the call in errors ("SSO portal").
pub(crate) async fn send(
    request: http_client::RequestBuilder<'_>,
    what: &str,
    timeout: Duration,
) -> Result<HttpResponse, CredsError> {
    let response = request
        .timeout(timeout)
        .send()
        .await
        .map_err(|e| transport_error(&e, what))?;
    let status = response.status().as_u16();
    let body = response
        .text()
        .await
        .map_err(|e| transport_error(&e, what))?;
    Ok(HttpResponse { status, body })
}

fn transport_error(error: &reqwest::Error, what: &str) -> CredsError {
    if error.is_timeout() {
        CredsError::Timeout(format!("{what} timed out"))
    } else if error.is_connect() {
        CredsError::Provider(format!("{what} could not be reached"))
    } else {
        CredsError::Provider(format!("{what} request failed"))
    }
}
