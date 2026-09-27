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
/// `mailto`) and every channel scheme `warp_core::channel::ChannelState::url_scheme`
/// can return (`warp`, `warppreview`, `warpdev`, `warplocal`, `warpintegration`,
/// `phosphor` for the OSS channel), plus `zap`, kept as a legacy compatibility
/// scheme alongside the other load-bearing `"zap"` surfaces the OSS rebrand left
/// in place (see `crates/warp_core/src/paths.rs`,
/// `app/src/util/file/external_editor/mac.rs`). `warposs` was never a real
/// channel scheme -- `Channel::Oss` has always mapped to `phosphor` -- so it was
/// dropped in favor of the value this list is actually supposed to track (#716).
///
/// **This cannot consume `app::uri::link_policy::is_openable_url_scheme`, the
/// equivalent policy for the desktop build.** `warpui` (and `warpui_core`,
/// which it wraps) sit *below* `warp_core` and `app` in the crate graph --
/// neither depends on `warp_core`, so neither can name `ChannelState` or
/// anything under `app::uri` -- and this function only exists in the `wasm`
/// build in the first place, where the browser itself is the "OS" being asked
/// to open a new tab. The list here is therefore a second, independently
/// maintained enumeration of the same channel schemes, not a call-out to the
/// shared one; keep the two in sync by hand when a channel scheme changes.
#[cfg(any(target_family = "wasm", test))]
pub(crate) fn safe_browser_open_url(url: &str) -> Option<String> {
    let parsed_url = url::Url::parse(url).ok()?;
    match parsed_url.scheme() {
        "http" | "https" | "mailto" | "warp" | "warppreview" | "warpdev" | "warplocal"
        | "phosphor" | "warpintegration" | "zap" => Some(parsed_url.to_string()),
        _ => None,
    }
}

#[cfg(test)]
#[path = "browser_tests.rs"]
mod tests;
