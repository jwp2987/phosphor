//! AWS Signature Version 4 for header-signed requests (no query string, no S3 quirks): what STS
//! `AssumeRole` needs, and what a Bedrock `Converse` call needs (tests sign one against the
//! `fake_llm` Bedrock stub, which recomputes the signature independently).
//!
//! Canonicalisation follows the AWS SigV4 spec for non-S3 services: the path is URI-encoded once
//! more on top of how it is sent, headers are lowercased, trimmed and sorted, and the payload hash
//! is the lowercase hex SHA-256 of the body.

use std::fmt::Write as _;
use std::time::SystemTime;

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use super::Creds;

type HmacSha256 = Hmac<Sha256>;

/// One request to sign.
pub(crate) struct SigningInput<'a> {
    pub(crate) method: &'a str,
    /// `host[:port]` exactly as it will appear in the `Host` header.
    pub(crate) host: &'a str,
    /// The request path exactly as sent on the wire (already percent-encoded once).
    pub(crate) path: &'a str,
    /// Extra headers to sign, besides host, `x-amz-date` and `x-amz-security-token` (which are
    /// added here). Names may be any case.
    pub(crate) headers: &'a [(&'a str, &'a str)],
    pub(crate) body: &'a [u8],
    pub(crate) region: &'a str,
    pub(crate) service: &'a str,
    pub(crate) creds: &'a Creds,
    pub(crate) time: SystemTime,
}

/// The headers a caller must set on the request.
pub(crate) struct Signed {
    /// `X-Amz-Date` value.
    pub(crate) amz_date: String,
    /// `Authorization` value.
    pub(crate) authorization: String,
    /// `X-Amz-Security-Token` value, when the credentials have a session token.
    pub(crate) security_token: Option<String>,
}

pub(crate) fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

fn hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
    // HMAC accepts keys of any length, so this cannot fail.
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// RFC 3986 unreserved characters pass through; everything else is `%XX` (uppercase).
pub(crate) fn uri_encode(input: &str, encode_slash: bool) -> String {
    let mut out = String::with_capacity(input.len());
    for &b in input.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b'/' if !encode_slash => out.push('/'),
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

fn canonical_path(path: &str) -> String {
    let path = if path.is_empty() { "/" } else { path };
    uri_encode(path, false)
}

fn collapse_spaces(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn amz_date(time: SystemTime) -> String {
    chrono::DateTime::<chrono::Utc>::from(time)
        .format("%Y%m%dT%H%M%SZ")
        .to_string()
}

pub(crate) fn sign(input: &SigningInput<'_>) -> Signed {
    let amz_date = amz_date(input.time);
    let date = &amz_date[..8];

    let mut headers: Vec<(String, String)> = vec![
        ("host".to_string(), collapse_spaces(input.host)),
        ("x-amz-date".to_string(), amz_date.clone()),
    ];
    if let Some(token) = &input.creds.session_token {
        headers.push(("x-amz-security-token".to_string(), token.clone()));
    }
    for (name, value) in input.headers {
        headers.push((name.to_ascii_lowercase(), collapse_spaces(value)));
    }
    headers.sort();

    let canonical_headers: String = headers.iter().map(|(n, v)| format!("{n}:{v}\n")).collect();
    let signed_headers = headers
        .iter()
        .map(|(n, _)| n.as_str())
        .collect::<Vec<_>>()
        .join(";");

    let canonical_request = format!(
        "{}\n{}\n\n{}\n{}\n{}",
        input.method,
        canonical_path(input.path),
        canonical_headers,
        signed_headers,
        sha256_hex(input.body),
    );

    let scope = format!("{date}/{}/{}/aws4_request", input.region, input.service);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );

    let k_date = hmac(
        format!("AWS4{}", input.creds.secret_access_key).as_bytes(),
        date.as_bytes(),
    );
    let k_region = hmac(&k_date, input.region.as_bytes());
    let k_service = hmac(&k_region, input.service.as_bytes());
    let k_signing = hmac(&k_service, b"aws4_request");
    let signature = hex::encode(hmac(&k_signing, string_to_sign.as_bytes()));

    Signed {
        authorization: format!(
            "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
            input.creds.access_key_id
        ),
        amz_date,
        security_token: input.creds.session_token.clone(),
    }
}

#[cfg(test)]
#[path = "sigv4_tests.rs"]
mod tests;
