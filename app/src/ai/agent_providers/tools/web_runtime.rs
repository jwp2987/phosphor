//! Local execution logic for the BYOP `webfetch` and `websearch` tools.
//!
//! These two BYOP tools don't go through the protobuf executor (`warp_multi_agent_api` has no
//! corresponding variant); `chat_stream.rs::handle_byop_web_tool_intercept` calls this module
//! directly before `parse_incoming_tool_call`, synthesizing the result into a
//! `(ToolCall carrier, ToolCallResult)` message pair pushed back into the stream.
//!
//! ## Alignment with opencode
//!
//! - `webfetch` mirrors `packages/opencode/src/tool/webfetch.ts`:
//!   * UA defaults to Chrome; a 403 + `cf-mitigated: challenge` → falls back to the `Zap` UA
//!     and retries once
//!   * `Accept` header negotiated by q-priority based on the format parameter
//!   * Content-Length precheck + actual bytes-read double-check, 5 MB cap
//!   * timeout defaults to 30s, capped at 120s
//!   * image mimes auto-base64 into `output.attachments`
//! - `websearch` mirrors `packages/opencode/src/tool/{websearch,mcp-exa}.ts`:
//!   * defaults to the anonymous `https://mcp.exa.ai/mcp`; if the `EXA_API_KEY` env var is
//!     present it's appended to the querystring
//!   * 25s timeout
//!   * SSE response → `result.content[0].text`

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use reqwest::header::{ACCEPT, ACCEPT_LANGUAGE, CONTENT_LENGTH, CONTENT_TYPE, USER_AGENT};
use reqwest::redirect::Policy;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;
use std::time::Duration;

use super::exa;

// ---------------------------------------------------------------------------
// Constants (aligned with opencode webfetch.ts:8-10)
// ---------------------------------------------------------------------------

pub const MAX_RESPONSE_SIZE: usize = 5 * 1024 * 1024; // 5 MB
pub const DEFAULT_FETCH_TIMEOUT_SECS: u64 = 30;
pub const MAX_FETCH_TIMEOUT_SECS: u64 = 120;
pub const SEARCH_TIMEOUT_SECS: u64 = 25;

pub const CHROME_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36";
pub const FALLBACK_UA: &str = "Phosphor";

// ---------------------------------------------------------------------------
// webfetch
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum FetchFormat {
    #[default]
    Markdown,
    Text,
    Html,
}

impl FetchFormat {
    fn accept_header(&self) -> &'static str {
        match self {
            Self::Markdown => {
                "text/markdown;q=1.0, text/x-markdown;q=0.9, text/plain;q=0.8, \
                 text/html;q=0.7, */*;q=0.1"
            }
            Self::Text => "text/plain;q=1.0, text/markdown;q=0.9, text/html;q=0.8, */*;q=0.1",
            Self::Html => {
                "text/html;q=1.0, application/xhtml+xml;q=0.9, text/plain;q=0.8, \
                 text/markdown;q=0.7, */*;q=0.1"
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct FetchArgs {
    pub url: String,
    #[serde(default)]
    pub format: Option<FetchFormat>,
    /// Unit: seconds. `None` → 30s; capped at 120s, values above that are clamped.
    #[serde(default)]
    pub timeout: Option<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FetchAttachment {
    pub mime: String,
    /// `data:<mime>;base64,<...>` form (aligned with opencode).
    pub url: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FetchOutput {
    pub url: String,
    pub status: u16,
    pub content_type: String,
    pub format: String,
    pub output: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<FetchAttachment>,
}

/// Returns `true` if the IP falls within a private, loopback, link-local, or other range that
/// webfetch should not access.
fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_blocked_ipv4(v4),
        IpAddr::V6(v6) => {
            // Both the IPv4-mapped (::ffff:x.x.x.x) and the deprecated
            // IPv4-compatible (::x.x.x.x) forms embed an IPv4 address. `to_ipv4()`
            // covers both, so e.g. `::127.0.0.1` / `[::203.0.113.5]` are evaluated
            // with IPv4 rules and can't slip past the loopback/private/test blocks.
            if let Some(embedded_v4) = v6.to_ipv4() {
                return is_blocked_ipv4(embedded_v4);
            }
            v6.is_loopback()               // ::1
                || v6.is_unspecified()      // ::
                || v6.is_multicast()        // ff00::/8
                || is_ipv6_unique_local(v6) // fc00::/7
                || is_ipv6_link_local(v6)   // fe80::/10
                || is_ipv6_documentation(v6) // documentation example range 2001:db8::/32
        }
    }
}

fn is_blocked_ipv4(v4: Ipv4Addr) -> bool {
    let o = v4.octets();
    v4.is_loopback()          // 127.0.0.0/8
        || v4.is_private()    // 10/8, 172.16/12, 192.168/16
        || v4.is_link_local() // 169.254.0.0/16
        || v4.is_multicast()  // 224.0.0.0/4
        || o[0] == 0          // 0.0.0.0/8, "this host" range
        || v4.is_broadcast()  // 255.255.255.255
        || (Ipv4Addr::new(100, 64, 0, 0) <= v4 && v4 <= Ipv4Addr::new(100, 127, 255, 255))
            // CGNAT 100.64/10
        || (o[0] == 192 && o[1] == 0 && o[2] == 2)   // TEST-NET-1 192.0.2.0/24
        || (o[0] == 198 && o[1] == 51 && o[2] == 100) // TEST-NET-2 198.51.100.0/24
        || (o[0] == 203 && o[1] == 0 && o[2] == 113)  // TEST-NET-3 203.0.113.0/24
        || (o[0] == 198 && (o[1] & 0xfe) == 18)       // benchmarking range 198.18.0.0/15
        || o[0] >= 240 // reserved range 240.0.0.0/4
}

fn is_ipv6_unique_local(v6: Ipv6Addr) -> bool {
    (v6.segments()[0] & 0xfe00) == 0xfc00
}

fn is_ipv6_link_local(v6: Ipv6Addr) -> bool {
    (v6.segments()[0] & 0xffc0) == 0xfe80
}

fn is_ipv6_documentation(v6: Ipv6Addr) -> bool {
    v6.segments()[0] == 0x2001 && v6.segments()[1] == 0x0db8
}

/// Validates a URL's SSRF safety: rejects private/internal addresses given as IP literals.
///
/// Only performs a synchronous, I/O-free literal check. DNS results for hostnames are
/// filtered by `SsrfSafeResolver` at resolution time (enforced at connect time, with no
/// TOCTOU gap). An earlier version also did a blocking `to_socket_addrs` pre-resolution
/// here — it ran on the async runtime's threads (including inside the redirect policy
/// closure, which hyper calls on every hop), so slow DNS would stall the entire worker; and
/// `to_socket_addrs` isn't available on wasm32 anyway, so it never actually took effect there.
fn validate_url_not_internal(url_str: &str) -> Result<()> {
    let parsed = url::Url::parse(url_str).context("invalid URL")?;
    let host = parsed.host_str().context("URL has no host")?;

    // If host is already an IP literal, check it directly.
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_blocked_ip(ip) {
            bail!("URL targets a blocked IP address range");
        }
    }

    Ok(())
}

/// A DNS resolver that filters out disallowed internal IPs at resolution time, avoiding the
/// TOCTOU gap between pre-validation and connection.
///
/// Only available for non-WASM targets: reqwest's `dns` module and
/// `ClientBuilder::dns_resolver` aren't exposed to WebAssembly.
#[cfg(not(target_arch = "wasm32"))]
struct SsrfSafeResolver;

#[cfg(not(target_arch = "wasm32"))]
impl reqwest::dns::Resolve for SsrfSafeResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            use std::net::ToSocketAddrs;
            let lookup_host = host.clone();
            let addrs: Vec<std::net::SocketAddr> = tokio::task::spawn_blocking(
                move || -> std::io::Result<Vec<std::net::SocketAddr>> {
                    Ok((lookup_host.as_str(), 0)
                        .to_socket_addrs()?
                        .filter(|addr| !is_blocked_ip(addr.ip()))
                        .collect())
                },
            )
            .await
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;
            if addrs.is_empty() {
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!("DNS for '{host}' resolved to blocked IPs (SSRF protection)"),
                ))
                    as Box<dyn std::error::Error + Send + Sync>);
            }
            Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

/// Maximum redirect hops, matching reqwest's default.
const MAX_REDIRECT_HOPS: usize = 10;

/// Builds a reqwest client with SSRF protection:
/// - a custom DNS resolver blocks connections to internal IPs
/// - a custom redirect policy enforces HTTPS, validates each hop, and caps the total number of
///   hops (`Policy::custom` doesn't inherit reqwest's default hop limit)
pub fn build_ssrf_safe_client() -> Result<reqwest::Client> {
    let policy = Policy::custom(|attempt| {
        // `Policy::custom` doesn't inherit reqwest's default loop/max-hop protection, so it
        // must be enforced explicitly.
        if attempt.previous().len() >= MAX_REDIRECT_HOPS {
            return attempt.stop();
        }
        let url = attempt.url();
        // The redirect target must stay HTTPS, to avoid an HTTPS → HTTP downgrade.
        if url.scheme() != "https" {
            return attempt.stop();
        }
        // An extra layer of validation on top of the DNS resolver, to immediately reject
        // internal addresses given as IP literals.
        if validate_url_not_internal(url.as_str()).is_err() {
            attempt.stop()
        } else {
            attempt.follow()
        }
    });
    let builder = reqwest::Client::builder()
        .redirect(policy)
        .pool_idle_timeout(Duration::from_secs(30));
    // Only non-WASM targets get the SSRF-safe DNS resolver; WebAssembly doesn't expose
    // reqwest's DNS module.
    #[cfg(not(target_arch = "wasm32"))]
    let builder = builder.dns_resolver(Arc::new(SsrfSafeResolver));
    builder.build().context("build SSRF-safe reqwest client")
}

/// Entry point: runs a single webfetch, returning structured output (fed to the upstream LLM
/// by the caller via `serde_json::to_value`).
pub async fn run_webfetch(client: &reqwest::Client, args: FetchArgs) -> Result<FetchOutput> {
    if !args.url.starts_with("https://") {
        bail!("URL must use HTTPS");
    }
    validate_url_not_internal(&args.url)?;
    let format = args.format.clone().unwrap_or_default();
    let timeout_secs = args
        .timeout
        .unwrap_or(DEFAULT_FETCH_TIMEOUT_SECS)
        .min(MAX_FETCH_TIMEOUT_SECS);
    let timeout = Duration::from_secs(timeout_secs);

    let accept = format.accept_header();
    let resp = match send_fetch(client, &args.url, accept, CHROME_UA, timeout).await {
        Ok(r) => r,
        Err(e) => return Err(e),
    };

    // Cloudflare challenge: a first-round 403 + cf-mitigated: challenge with the Chrome UA →
    // switch UA and retry once.
    let resp = if resp.status() == StatusCode::FORBIDDEN
        && resp
            .headers()
            .get("cf-mitigated")
            .and_then(|v| v.to_str().ok())
            == Some("challenge")
    {
        log::info!("[webfetch] cloudflare challenge detected → retry with fallback UA");
        send_fetch(client, &args.url, accept, FALLBACK_UA, timeout).await?
    } else {
        resp
    };

    response_to_fetch_output(resp, &args.url, &format).await
}

/// Shared Response → FetchOutput conversion logic.
///
/// Called by both `run_webfetch` and test helper functions, to avoid duplicating status
/// checking, size limiting, image encoding, and JSON pretty-printing logic.
async fn response_to_fetch_output(
    resp: reqwest::Response,
    url: &str,
    format: &FetchFormat,
) -> Result<FetchOutput> {
    let status = resp.status();
    if !status.is_success() {
        bail!("HTTP {} fetching {url}", status.as_u16());
    }

    let content_type = resp
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let mime = content_type
        .split(';')
        .next()
        .map(|s| s.trim().to_ascii_lowercase())
        .unwrap_or_default();

    // Content-Length precheck
    if let Some(len_str) = resp
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
    {
        if let Ok(len) = len_str.parse::<usize>() {
            if len > MAX_RESPONSE_SIZE {
                bail!(
                    "Response too large (Content-Length {len} > {MAX_RESPONSE_SIZE} bytes limit)"
                );
            }
        }
    }

    let bytes = resp.bytes().await.context("read response body")?;
    if bytes.len() > MAX_RESPONSE_SIZE {
        bail!(
            "Response too large ({} bytes > {} bytes limit)",
            bytes.len(),
            MAX_RESPONSE_SIZE
        );
    }

    // Image → base64 attachment
    if is_image_mime(&mime) {
        let encoded = BASE64.encode(&bytes);
        let data_url = format!("data:{mime};base64,{encoded}");
        return Ok(FetchOutput {
            url: url.to_owned(),
            status: status.as_u16(),
            content_type,
            format: format!("{format:?}").to_ascii_lowercase(),
            output: "Image fetched successfully".to_owned(),
            attachments: vec![FetchAttachment {
                mime,
                url: data_url,
            }],
        });
    }

    let body_str = String::from_utf8_lossy(&bytes).into_owned();
    let is_html = mime == "text/html" || mime == "application/xhtml+xml";

    let output = match format {
        FetchFormat::Markdown if is_html => html_to_markdown(&body_str),
        FetchFormat::Text if is_html => extract_text_from_html(&body_str),
        FetchFormat::Html => body_str,
        // markdown / text requested but mime isn't html → pass through (already text-like)
        _ => body_str,
    };

    Ok(FetchOutput {
        url: url.to_owned(),
        status: status.as_u16(),
        content_type,
        format: format!("{format:?}").to_ascii_lowercase(),
        output: maybe_format_json(&output, &mime),
        attachments: vec![],
    })
}

async fn send_fetch(
    client: &reqwest::Client,
    url: &str,
    accept: &str,
    ua: &str,
    timeout: Duration,
) -> Result<reqwest::Response> {
    client
        .get(url)
        .header(USER_AGENT, ua)
        .header(ACCEPT, accept)
        .header(ACCEPT_LANGUAGE, "en-US,en;q=0.9")
        .timeout(timeout)
        .send()
        .await
        .with_context(|| format!("HTTP GET {url}"))
}

fn is_image_mime(mime: &str) -> bool {
    mime.starts_with("image/")
}

/// If mime is application/json and content is valid JSON, pretty-print it into a ```json```
/// code block (aligned with zed fetch_tool.rs's JSON handling).
fn maybe_format_json(content: &str, mime: &str) -> String {
    if mime != "application/json" {
        return content.to_owned();
    }
    match serde_json::from_str::<Value>(content) {
        Ok(v) => match serde_json::to_string_pretty(&v) {
            Ok(pretty) => format!("```json\n{pretty}\n```"),
            Err(_) => content.to_owned(),
        },
        Err(_) => content.to_owned(),
    }
}

fn html_to_markdown(html: &str) -> String {
    // htmd's default config already aligns with Turndown's common output style (atx
    // headings, fenced code blocks, etc.). Strip script / style / noscript / iframe content
    // beforehand (htmd's default behavior keeps the text inside these tags as plain text,
    // polluting the markdown output).
    let pre = strip_unsafe_blocks(html);
    match std::panic::catch_unwind(|| htmd::convert(&pre)) {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            log::warn!("[webfetch] htmd convert error: {e}, falling back to text extraction");
            naive_html_strip(&pre)
        }
        Err(_) => {
            log::warn!("[webfetch] htmd panicked, falling back to text extraction");
            naive_html_strip(&pre)
        }
    }
}

/// Deletes entire `<script>...</script>` / `<style>...</style>` / `<noscript>...</noscript>` /
/// `<iframe>...</iframe>` blocks, etc. (case-insensitive, attributes allowed).
///
/// Single-pass implementation: lowercases the whole document once and produces a single
/// String. An earlier version processed tags one at a time, lowercasing and rebuilding the
/// whole document on every pass (5 MB cap × 6 tags ≈ 12 large allocations).
fn strip_unsafe_blocks(html: &str) -> String {
    const STRIP_TAGS: &[&str] = &["script", "style", "noscript", "iframe", "object", "embed"];
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut cursor = 0;
    'scan: while cursor < html.len() {
        let Some(rel_lt) = lower[cursor..].find('<') else {
            break;
        };
        let lt = cursor + rel_lt;
        out.push_str(&html[cursor..lt]);
        cursor = lt;
        for tag in STRIP_TAGS {
            if lower[lt + 1..].starts_with(tag) {
                // Must be followed by `>` / whitespace / `/` (avoids mis-matching
                // <scriptlike>)
                let after = lt + 1 + tag.len();
                match lower.as_bytes().get(after) {
                    Some(b'>') | Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r')
                    | Some(b'/') => {}
                    _ => continue,
                }
                let close = format!("</{tag}>");
                match lower[after..].find(&close) {
                    Some(rel_close) => {
                        cursor = after + rel_close + close.len();
                    }
                    None => {
                        // Unclosed → discard the rest of the document
                        cursor = html.len();
                    }
                }
                continue 'scan;
            }
        }
        // Not a tag to strip: keep `<` as-is and keep scanning.
        out.push('<');
        cursor += 1;
    }
    out.push_str(&html[cursor..]);
    out
}

/// Minimal HTML→plain-text fallback: strips all tags with a simple scan. Only used when htmd
/// fails.
fn naive_html_strip(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// HTML → plain text: first convert to markdown with htmd, then strip markdown markers.
///
/// A simplified path that avoids pulling in an html5ever DOM traversal dependency
/// (`markup5ever_rcdom`). htmd already filters out invisible tags like script/style/noscript
/// internally, so the plain-text output is good enough for text mode.
fn extract_text_from_html(html: &str) -> String {
    let md = html_to_markdown(html);
    strip_markdown(&md)
}

fn strip_markdown(md: &str) -> String {
    let mut out = String::with_capacity(md.len());
    let mut last_blank = false;
    for raw_line in md.lines() {
        let mut line = raw_line.trim().to_owned();
        // Heading prefix # ## ###
        while line.starts_with('#') {
            line.remove(0);
        }
        let line = line.trim_start();
        // List / blockquote / horizontal-rule prefix
        let line = line.trim_start_matches(['-', '*', '>', '+']).trim_start();
        // ![alt](url) → delete the whole thing
        let line = strip_pattern(line, "![", ")");
        // [text](url) → keep text
        let line = unwrap_links(&line);
        // `code` / **bold** / *em* / _em_ — conservatively strip out ` * _
        let cleaned: String = line
            .chars()
            .filter(|c| !matches!(c, '`' | '*' | '_'))
            .collect();
        let trimmed = cleaned.trim();
        if trimmed.is_empty() {
            if !last_blank && !out.is_empty() {
                out.push('\n');
                last_blank = true;
            }
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(trimmed);
        last_blank = false;
    }
    out
}

fn strip_pattern(s: &str, start: &str, end: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find(start) {
        out.push_str(&rest[..i]);
        let after = &rest[i + start.len()..];
        match after.find(end) {
            Some(j) => rest = &after[j + end.len()..],
            None => {
                // Unclosed, keep the remainder
                rest = after;
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// `[text](url)` → `text`
///
/// Only slices and concatenates on str boundaries (everything `find` returns is a valid UTF-8
/// boundary). An earlier implementation did byte-by-byte `push(bytes[i] as char)`, which
/// converts multi-byte UTF-8 into Latin-1 code points one byte at a time, garbling all
/// non-ASCII text (CJK, etc.).
fn unwrap_links(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(open) = rest.find('[') {
        // Look for ]( then )
        if let Some(close_text) = rest[open + 1..].find("](") {
            let text = &rest[open + 1..open + 1 + close_text];
            let after_paren = &rest[open + 1 + close_text + 2..];
            if let Some(close_url) = after_paren.find(')') {
                out.push_str(&rest[..open]);
                out.push_str(text);
                rest = &after_paren[close_url + 1..];
                continue;
            }
        }
        // A `[` that isn't part of a link: keep as-is and keep scanning forward.
        out.push_str(&rest[..=open]);
        rest = &rest[open + 1..];
    }
    out.push_str(rest);
    out
}

// ---------------------------------------------------------------------------
// websearch
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct SearchToolArgs {
    pub query: String,
    #[serde(rename = "numResults", default)]
    pub num_results: Option<u32>,
    #[serde(default)]
    pub livecrawl: Option<String>,
    #[serde(rename = "type", default)]
    pub search_type: Option<String>,
    #[serde(rename = "contextMaxCharacters", default)]
    pub context_max_characters: Option<u32>,
}

impl SearchToolArgs {
    pub fn into_exa_args(self) -> exa::SearchArgs {
        let mut a = exa::SearchArgs::with_defaults(self.query);
        if let Some(n) = self.num_results {
            a.num_results = n;
        }
        if let Some(s) = self.livecrawl {
            a.livecrawl = s;
        }
        if let Some(t) = self.search_type {
            a.search_type = t;
        }
        if let Some(c) = self.context_max_characters {
            a.context_max_characters = Some(c);
        }
        a
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SearchOutput {
    pub query: String,
    /// The human-readable / LLM-optimized context string returned by Exa.
    pub results: String,
}

const EMPTY_FALLBACK: &str = "No search results found. Please try a different query.";

/// Entry point: runs a single Exa websearch.
///
/// `endpoint_override`: for tests; defaults to `exa::endpoint_url(api_key)`.
/// `api_key`: `None` → anonymous; `Some(...)` → appended to the querystring.
pub async fn run_websearch(
    client: &reqwest::Client,
    args: SearchToolArgs,
    api_key: Option<&str>,
    endpoint_override: Option<&str>,
) -> Result<SearchOutput> {
    let query = args.query.clone();
    let exa_args = args.into_exa_args();
    let body = exa::build_request_body(exa::SEARCH_TOOL_NAME, &exa_args);

    let url = endpoint_override
        .map(|s| s.to_owned())
        .unwrap_or_else(|| exa::endpoint_url(api_key));

    let resp = client
        .post(&url)
        .header(ACCEPT, "application/json, text/event-stream")
        .header(CONTENT_TYPE, "application/json")
        .timeout(Duration::from_secs(SEARCH_TIMEOUT_SECS))
        .json(&body)
        .send()
        .await
        .with_context(|| format!("Exa POST {url}"))?;

    let status = resp.status();
    if !status.is_success() {
        let body_text = resp.text().await.unwrap_or_default();
        bail!("Exa returned HTTP {} ({})", status.as_u16(), body_text);
    }
    let body_text = resp.text().await.context("read Exa SSE body")?;

    let parsed = exa::parse_sse_body(&body_text)?;
    let results = parsed.unwrap_or_else(|| EMPTY_FALLBACK.to_owned());
    Ok(SearchOutput { query, results })
}

/// Serializes webfetch / websearch structured results into a JSON Value (the string the
/// upstream LLM sees).
///
/// The tool_result of every BYOP local-intercept tool must carry the
/// `"_byop_intercepted":true` sentinel, otherwise the controller (`controller.rs:2693+`)
/// won't trigger auto-resume and the model will get stuck waiting for a result. See
/// `chat_stream::dispatch_byop_web_tool` and the controller's `needs_byop_local_resume` check.
pub fn fetch_output_to_json(out: &FetchOutput) -> Value {
    let mut v = serde_json::to_value(out).unwrap_or_else(|_| json!({"status": "serialize_error"}));
    if let Some(obj) = v.as_object_mut() {
        obj.insert("_byop_intercepted".to_owned(), Value::Bool(true));
    }
    v
}
pub fn search_output_to_json(out: &SearchOutput) -> Value {
    let mut v = serde_json::to_value(out).unwrap_or_else(|_| json!({"status": "serialize_error"}));
    if let Some(obj) = v.as_object_mut() {
        obj.insert("_byop_intercepted".to_owned(), Value::Bool(true));
    }
    v
}
/// Recognizes the `"invalid_arguments: "` / `"invalid arguments: "` prefix that
/// `chat_stream::dispatch_byop_web_tool` puts on the message it hands to [`error_to_json`]
/// specifically when `serde_json::from_str::<FetchArgs|SearchToolArgs>` failed to parse the
/// model's arguments — as opposed to a disabled-profile gate rejection or a genuine runtime
/// error (failed HTTP call, blocked SSRF target, etc). `dispatch_byop_web_tool` builds the
/// error via `anyhow::anyhow!(format!("invalid arguments: {e}"))`, so the prefix (and thus
/// this function) is the only signal available here: `error_to_json`'s signature is fixed by
/// its caller in `chat_stream.rs`, which this module does not own and cannot extend with an
/// extra "this was a parse failure" flag.
fn strip_invalid_arguments_prefix(message: &str) -> Option<&str> {
    message
        .strip_prefix("invalid_arguments:")
        .or_else(|| message.strip_prefix("invalid arguments:"))
        .map(str::trim)
}

/// Builds the same synthetic-rejection shape `todowrite::invalid_arguments_result_to_json`
/// uses, recognized by `convert_from::invalid_arguments_rejected_tool_call` so the UI renders
/// a real `RejectedToolCall` instead of silently dropping the message. See
/// `strip_invalid_arguments_prefix` for why this only fires on the parse-failure path and not
/// on a gate rejection or a genuine runtime error (those keep the original `{"status":"error"}`
/// shape, since `result_to_json`-style errors are answers from a call that ran, not answers to
/// a call whose arguments never parsed).
///
/// `received_args` isn't included: the raw argument string never reaches this function (only
/// the already-formatted `anyhow::Error` does), and the recognizer in `convert_from.rs` only
/// reads `error`/`tool`/`detail` — `received_args` is cosmetic parity with `todowrite`, not a
/// functional requirement.
pub fn error_to_json(tool: &str, e: &anyhow::Error) -> Value {
    let message = format!("{e:#}");
    if let Some(detail) = strip_invalid_arguments_prefix(&message) {
        return json!({
            "_byop_intercepted": true,
            "error": "invalid_arguments",
            "detail": detail,
            "tool": tool,
            "hint": "Arguments did not match the tool's expected format.",
        });
    }
    json!({
        "_byop_intercepted": true,
        "status": "error",
        "tool": tool,
        "message": message,
    })
}

#[cfg(test)]
#[path = "webfetch_tests.rs"]
mod webfetch_tests;
#[cfg(test)]
#[path = "websearch_tests.rs"]
mod websearch_tests;

#[cfg(test)]
mod ssrf_ip_tests {
    use super::*;
    use std::str::FromStr as _;

    fn blocked(s: &str) -> bool {
        is_blocked_ip(IpAddr::from_str(s).unwrap())
    }

    #[test]
    fn blocks_ipv4_embedded_in_ipv6_forms() {
        // IPv4-mapped and the deprecated IPv4-compatible forms must both be
        // evaluated with IPv4 rules so loopback/private/test can't be smuggled.
        assert!(blocked("::ffff:127.0.0.1"), "mapped loopback");
        assert!(blocked("::127.0.0.1"), "compatible loopback");
        assert!(blocked("::ffff:10.0.0.1"), "mapped private");
        assert!(blocked("::10.0.0.1"), "compatible private");
        assert!(blocked("::203.0.113.5"), "compatible TEST-NET-3");
    }

    #[test]
    fn allows_public_addresses() {
        assert!(!blocked("1.1.1.1"));
        assert!(!blocked("2606:4700:4700::1111"));
    }
}
