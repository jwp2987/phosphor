//! A tiny HTTP/1.1 stub for AWS-shaped endpoints, with a SigV4 verifier that shares no code with
//! the app's signer.
//!
//! * [`HttpStub`]: a thread-per-connection server on `127.0.0.1:0` that hands every request to a
//!   handler closure, records it, and answers with `Connection: close`. The credential resolver
//!   tests point `AWS_ENDPOINT_URL_STS` / `_SSO` / `_SSO_OIDC` / `_SIGNIN`, the ECS and IMDS
//!   variables, at it.
//! * [`verify_sigv4`]: recomputes the AWS Signature Version 4 of a received request from the
//!   secret key and checks the credential scope, the signed header list, the session token and the
//!   payload hash. It returns a secret-free reason on mismatch.
//! * [`BedrockStub`]: a Bedrock-compatible endpoint (`POST /model/{id}/converse`) that accepts a
//!   request only when its SigV4 signature verifies against a known key and a frozen time.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

/// One received request. Header names are lowercased.
#[derive(Debug, Clone)]
pub struct StubRequest {
    pub method: String,
    /// The request target as sent (path, plus `?query` if any).
    pub target: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl StubRequest {
    pub fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.as_str())
    }

    /// The path without the query string, as sent.
    pub fn path(&self) -> &str {
        self.target.split('?').next().unwrap_or("")
    }

    pub fn query(&self) -> &str {
        self.target.split_once('?').map(|(_, q)| q).unwrap_or("")
    }

    pub fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// The value of `key` in an `application/x-www-form-urlencoded` body or query string.
    pub fn form_value(&self, key: &str) -> Option<String> {
        let text = if self.body.is_empty() {
            self.query().to_string()
        } else {
            self.body_text()
        };
        text.split('&').find_map(|pair| {
            let (k, v) = pair.split_once('=')?;
            (percent_decode(k) == key).then(|| percent_decode(v))
        })
    }
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => match u8::from_str_radix(&input[i + 1..i + 3], 16) {
                Ok(b) => {
                    out.push(b);
                    i += 3;
                }
                Err(_) => {
                    out.push(b'%');
                    i += 1;
                }
            },
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// What a handler answers.
#[derive(Debug, Clone)]
pub struct StubResponse {
    pub status: u16,
    pub content_type: String,
    pub body: String,
    pub extra_headers: Vec<(String, String)>,
}

impl StubResponse {
    pub fn json(status: u16, body: impl Into<String>) -> Self {
        Self {
            status,
            content_type: "application/json".to_string(),
            body: body.into(),
            extra_headers: Vec::new(),
        }
    }

    pub fn xml(status: u16, body: impl Into<String>) -> Self {
        Self {
            status,
            content_type: "text/xml".to_string(),
            body: body.into(),
            extra_headers: Vec::new(),
        }
    }

    pub fn text(status: u16, body: impl Into<String>) -> Self {
        Self {
            status,
            content_type: "text/plain".to_string(),
            body: body.into(),
            extra_headers: Vec::new(),
        }
    }
}

type Handler = dyn Fn(&StubRequest) -> StubResponse + Send + Sync + 'static;

/// A running stub server. Dropping it stops the accept loop.
pub struct HttpStub {
    addr: SocketAddr,
    requests: Arc<Mutex<Vec<StubRequest>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl HttpStub {
    pub fn start<F>(handler: F) -> Self
    where
        F: Fn(&StubRequest) -> StubResponse + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind the stub listener");
        let addr = listener.local_addr().expect("stub local address");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let handler: Arc<Handler> = Arc::new(handler);

        let thread = {
            let requests = requests.clone();
            let stop = stop.clone();
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    let Ok(stream) = stream else { continue };
                    let handler = handler.clone();
                    let requests = requests.clone();
                    std::thread::spawn(move || serve(stream, &*handler, &requests));
                }
            })
        };
        Self {
            addr,
            requests,
            stop,
            thread: Some(thread),
        }
    }

    /// `http://127.0.0.1:<port>`, no trailing slash.
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Every request received so far, in arrival order.
    pub fn requests(&self) -> Vec<StubRequest> {
        self.requests.lock().expect("stub request log").clone()
    }
}

impl Drop for HttpStub {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Wake the blocking accept so the loop sees the flag.
        let _ = TcpStream::connect(self.addr);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve(mut stream: TcpStream, handler: &Handler, log: &Mutex<Vec<StubRequest>>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let Some(request) = read_request(&mut stream) else {
        return;
    };
    log.lock().expect("stub request log").push(request.clone());
    let response = handler(&request);
    let mut out = format!(
        "HTTP/1.1 {} Stub\r\ncontent-type: {}\r\ncontent-length: {}\r\nconnection: close\r\n",
        response.status,
        response.content_type,
        response.body.len()
    );
    for (name, value) in &response.extra_headers {
        out.push_str(&format!("{name}: {value}\r\n"));
    }
    out.push_str("\r\n");
    out.push_str(&response.body);
    let _ = stream.write_all(out.as_bytes());
    let _ = stream.flush();
}

fn read_request(stream: &mut TcpStream) -> Option<StubRequest> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        if let Some(pos) = find(&buf, b"\r\n\r\n") {
            break pos;
        }
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let method = first.next()?.to_string();
    let target = first.next()?.to_string();
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| {
            let (n, v) = l.split_once(':')?;
            Some((n.trim().to_ascii_lowercase(), v.trim().to_string()))
        })
        .collect();
    let length = headers
        .iter()
        .find(|(n, _)| n == "content-length")
        .and_then(|(_, v)| v.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = buf[header_end + 4..].to_vec();
    while body.len() < length {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(length);
    Some(StubRequest {
        method,
        target,
        headers,
        body,
    })
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

// ---------------------------------------------------------------------------------------------
// SigV4 verification
// ---------------------------------------------------------------------------------------------

/// What a verified request must have been signed with.
#[derive(Debug, Clone)]
pub struct SigV4Expectation {
    pub access_key: String,
    pub secret_key: String,
    pub session_token: Option<String>,
    pub region: String,
    pub service: String,
    /// When set, `x-amz-date` must equal this exactly (`YYYYMMDDTHHMMSSZ`): the frozen time.
    pub amz_date: Option<String>,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha256_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("hmac key");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn encode(input: &str) -> String {
    let mut out = String::new();
    for &b in input.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b'/') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Verifies `request` against `expect`. `Ok(())` means every check passed.
pub fn verify_sigv4(request: &StubRequest, expect: &SigV4Expectation) -> Result<(), String> {
    let auth = request
        .header("authorization")
        .ok_or("no Authorization header")?;
    let rest = auth
        .strip_prefix("AWS4-HMAC-SHA256 ")
        .ok_or("Authorization is not AWS4-HMAC-SHA256")?;
    let field = |name: &str| -> Option<&str> {
        rest.split(',')
            .map(str::trim)
            .find_map(|p| p.strip_prefix(&format!("{name}=")))
    };
    let credential = field("Credential").ok_or("no Credential")?;
    let signed_headers = field("SignedHeaders").ok_or("no SignedHeaders")?;
    let signature = field("Signature").ok_or("no Signature")?;

    let amz_date = request.header("x-amz-date").ok_or("no x-amz-date header")?;
    if let Some(frozen) = &expect.amz_date
        && amz_date != frozen
    {
        return Err(format!(
            "x-amz-date {amz_date} is not the frozen time {frozen}"
        ));
    }
    let date = amz_date.get(..8).ok_or("x-amz-date is too short")?;

    let expected_scope = format!("{date}/{}/{}/aws4_request", expect.region, expect.service);
    let expected_credential = format!("{}/{expected_scope}", expect.access_key);
    if credential != expected_credential {
        return Err(format!(
            "credential scope mismatch: got a scope of {:?}, want {expected_scope:?} (or the wrong access key)",
            credential.split_once('/').map(|(_, s)| s).unwrap_or("")
        ));
    }

    let names: Vec<&str> = signed_headers.split(';').collect();
    if !names.contains(&"host") || !names.contains(&"x-amz-date") {
        return Err("host and x-amz-date must be signed".to_string());
    }
    match &expect.session_token {
        Some(token) => {
            if request.header("x-amz-security-token") != Some(token.as_str()) {
                return Err("x-amz-security-token is missing or wrong".to_string());
            }
            if !names.contains(&"x-amz-security-token") {
                return Err("x-amz-security-token is not signed".to_string());
            }
        }
        None => {
            if request.header("x-amz-security-token").is_some() {
                return Err("unexpected x-amz-security-token".to_string());
            }
        }
    }

    let payload_hash = sha256_hex(&request.body);
    if let Some(declared) = request.header("x-amz-content-sha256")
        && declared != payload_hash
        && declared != "UNSIGNED-PAYLOAD"
    {
        return Err("x-amz-content-sha256 does not match the body".to_string());
    }

    let mut canonical_headers = String::new();
    for name in &names {
        let value = request
            .header(name)
            .ok_or_else(|| format!("signed header {name} is absent"))?;
        let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
        canonical_headers.push_str(&format!("{name}:{value}\n"));
    }
    if names.windows(2).any(|w| w[0] >= w[1]) {
        return Err("SignedHeaders is not sorted".to_string());
    }

    // Canonical URI: the path as received, URI-encoded once more (non-S3).
    let canonical_uri = encode(request.path());
    let mut query_pairs: Vec<&str> = request
        .query()
        .split('&')
        .filter(|p| !p.is_empty())
        .collect();
    query_pairs.sort();
    let canonical_query = query_pairs.join("&");

    let canonical_request = format!(
        "{}\n{canonical_uri}\n{canonical_query}\n{canonical_headers}\n{signed_headers}\n{payload_hash}",
        request.method
    );
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{expected_scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );
    let k_date = hmac_sha256(
        format!("AWS4{}", expect.secret_key).as_bytes(),
        date.as_bytes(),
    );
    let k_region = hmac_sha256(&k_date, expect.region.as_bytes());
    let k_service = hmac_sha256(&k_region, expect.service.as_bytes());
    let k_signing = hmac_sha256(&k_service, b"aws4_request");
    let expected = hex(&hmac_sha256(&k_signing, string_to_sign.as_bytes()));

    if expected != signature {
        return Err("signature does not match the recomputed SigV4 signature".to_string());
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Bedrock
// ---------------------------------------------------------------------------------------------

/// A Bedrock Runtime stub. `POST /model/{id}/converse` is accepted only when its SigV4 signature
/// (service `bedrock`) verifies; the verdict of every request is kept for the test to read.
pub struct BedrockStub {
    stub: HttpStub,
    verdicts: Arc<Mutex<Vec<Result<(), String>>>>,
}

impl BedrockStub {
    pub fn start(expect: SigV4Expectation) -> Self {
        let verdicts: Arc<Mutex<Vec<Result<(), String>>>> = Arc::new(Mutex::new(Vec::new()));
        let log = verdicts.clone();
        let stub = HttpStub::start(move |req| {
            let is_converse = req.method == "POST"
                && req.path().starts_with("/model/")
                && req.path().ends_with("/converse");
            if !is_converse {
                return StubResponse::json(404, r#"{"message":"unknown operation"}"#);
            }
            let verdict = verify_sigv4(req, &expect);
            let response = match &verdict {
                Ok(()) => StubResponse::json(
                    200,
                    r#"{"output":{"message":{"role":"assistant","content":[{"text":"signed ok"}]}},"stopReason":"end_turn","usage":{"inputTokens":1,"outputTokens":2,"totalTokens":3}}"#,
                ),
                Err(reason) => {
                    let mut r = StubResponse::json(
                        403,
                        r#"{"message":"The request signature we calculated does not match the signature you provided."}"#,
                    );
                    r.extra_headers.push((
                        "x-stub-reason".to_string(),
                        reason.replace(['\r', '\n'], " "),
                    ));
                    r
                }
            };
            log.lock().expect("verdict log").push(verdict);
            response
        });
        Self { stub, verdicts }
    }

    pub fn url(&self) -> String {
        self.stub.url()
    }

    pub fn requests(&self) -> Vec<StubRequest> {
        self.stub.requests()
    }

    /// One entry per `converse` request: `Ok` when the signature verified.
    pub fn verdicts(&self) -> Vec<Result<(), String>> {
        self.verdicts.lock().expect("verdict log").clone()
    }
}

#[cfg(test)]
#[path = "aws_stub_tests.rs"]
mod tests;
