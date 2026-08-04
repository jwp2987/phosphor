pub fn escape_html_attribute(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

/// Validates a URL against a browser-safe scheme allowlist before it is handed
/// to the platform browser-open path, returning the re-serialized URL when the
/// scheme is permitted and `None` otherwise.
///
/// The allowlist keeps the universally safe web schemes (`http`, `https`,
/// `mailto`) and the app's own deeplink schemes. The fork registers the `zap`
/// scheme for its OSS channel (see `ChannelState::url_scheme`), while the
/// upstream `warp*` schemes are retained so that Warp-origin deeplinks continue
/// to route into the app unchanged (parity with upstream).
#[cfg(any(target_family = "wasm", test))]
pub(crate) fn safe_browser_open_url(url: &str) -> Option<String> {
    let parsed_url = url::Url::parse(url).ok()?;
    match parsed_url.scheme() {
        "http" | "https" | "mailto" | "warp" | "warppreview" | "warpdev" | "warplocal"
        | "warposs" | "warpintegration" | "zap" => Some(parsed_url.to_string()),
        _ => None,
    }
}

#[cfg(test)]
#[path = "browser_tests.rs"]
mod tests;
