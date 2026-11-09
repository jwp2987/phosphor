//! App-test "find hook" (#1789).
//!
//! When `PHOSPHOR_APPCI_FIND_FILE` names a file, the app reads a token list from it (one token per
//! line; the harness appends to it while the app runs). Whenever the app DRAWS text that contains
//! one of those tokens it logs
//! `appci_find token=T x0=.. y0=.. x1=.. y1=..` (window pixels, device pixels, top-left origin) at
//! info level on the log target `warpui::appci_find`. The harness reads that line instead of
//! locating the text by OCR.
//!
//! Cost and privacy rules:
//! - With the variable unset, [`active_tokens`] is one cached `OnceLock` read; callers do no string
//!   work. Layout stores nothing and painting walks an empty `Vec`.
//! - Only the listed tokens are ever logged (the token string and its box); no other screen text is
//!   read out, stored or logged.
//! - A token is logged once per position change (see [`Reporter`]).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use parking_lot::Mutex;

/// Environment variable naming the token file.
pub const ENV_FIND_FILE: &str = "PHOSPHOR_APPCI_FIND_FILE";
/// How often the token file is re-checked for new tokens.
const RECHECK: Duration = Duration::from_millis(200);
/// A position seen before is only re-logged this often (several on-screen copies of one token).
const RELOG: Duration = Duration::from_secs(1);
/// Longest token accepted.
const MAX_TOKEN_CHARS: usize = 200;

/// One token occurrence inside a laid-out line: its char range and the x extent along the line
/// (line-relative layout pixels).
#[derive(Debug, Clone, PartialEq)]
pub struct FindHit {
    pub token: String,
    /// Char index of the token's first char in the laid-out text.
    pub start: u32,
    /// Char index one past the token's last char.
    pub end: u32,
    pub x0: f32,
    pub x1: f32,
}

/// Char ranges `(token index, start, end)` where a token occurs in `text`. Non-overlapping per
/// token; ordered by token, then position.
pub fn find_token_ranges(text: &str, tokens: &[String]) -> Vec<(usize, usize, usize)> {
    let mut out = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        if token.is_empty() || token.len() > text.len() {
            continue;
        }
        let token_chars = token.chars().count();
        for (byte, _) in text.match_indices(token.as_str()) {
            let start = text[..byte].chars().count();
            out.push((i, start, start + token_chars));
        }
    }
    out
}

/// The x extent `(x0, x1)` covered by the glyphs for chars `start..end`, given every glyph of the
/// line as `(char index, x, width)`. `None` unless the first and the last char of the range both
/// have a glyph (a token split across lines, or hidden, is not reported).
pub fn glyph_extent(glyphs: &[(usize, f32, f32)], start: usize, end: usize) -> Option<(f32, f32)> {
    if end <= start {
        return None;
    }
    let mut has_first = false;
    let mut has_last = false;
    let mut x0 = f32::INFINITY;
    let mut x1 = f32::NEG_INFINITY;
    for &(index, x, width) in glyphs {
        if index >= start && index < end {
            has_first |= index == start;
            has_last |= index == end - 1;
            x0 = x0.min(x);
            x1 = x1.max(x + width);
        }
    }
    (has_first && has_last && x1 > x0).then_some((x0, x1))
}

/// All [`FindHit`]s for one laid-out line of `text`.
pub fn hits_for_line(text: &str, tokens: &[String], glyphs: &[(usize, f32, f32)]) -> Vec<FindHit> {
    find_token_ranges(text, tokens)
        .into_iter()
        .filter_map(|(i, start, end)| {
            let (x0, x1) = glyph_extent(glyphs, start, end)?;
            Some(FindHit {
                token: tokens[i].clone(),
                start: start as u32,
                end: end as u32,
                x0,
                x1,
            })
        })
        .collect()
}

/// Parse the token file: one token per line, blank lines and over-long lines ignored, duplicates
/// dropped.
pub fn parse_tokens(contents: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    contents
        .lines()
        .map(str::trim)
        .filter(|t| !t.is_empty() && t.chars().count() <= MAX_TOKEN_CHARS)
        .filter(|t| seen.insert(*t))
        .map(str::to_owned)
        .collect()
}

/// Decides whether a sighting is logged: once per position change per token. A position that was
/// logged before (another on-screen copy of the token scrolled away, so the older copy is the
/// visible one again) is logged again, but at most once per [`RELOG`].
#[derive(Default)]
pub struct Reporter {
    per_token: HashMap<String, TokenState>,
}

#[derive(Default)]
struct TokenState {
    last: Option<[i32; 4]>,
    seen: HashSet<[i32; 4]>,
    last_logged: Option<Instant>,
}

impl Reporter {
    pub fn should_log(&mut self, token: &str, rect: [i32; 4], now: Instant) -> bool {
        let st = self.per_token.entry(token.to_owned()).or_default();
        if st.last == Some(rect) {
            return false;
        }
        let first_time = st.seen.insert(rect);
        if !first_time
            && st
                .last_logged
                .is_some_and(|t| now.duration_since(t) < RELOG)
        {
            return false;
        }
        // Bound the memory of a token that moves a lot.
        if st.seen.len() > 256 {
            st.seen.clear();
            st.seen.insert(rect);
        }
        st.last = Some(rect);
        st.last_logged = Some(now);
        true
    }
}

struct TokenCache {
    tokens: Arc<Vec<String>>,
    checked: Option<Instant>,
    sig: Option<(u64, SystemTime)>,
}

static FIND_FILE: OnceLock<Option<PathBuf>> = OnceLock::new();
static CACHE: Mutex<Option<TokenCache>> = Mutex::new(None);
static REPORTER: Mutex<Option<Reporter>> = Mutex::new(None);

fn find_file() -> Option<&'static PathBuf> {
    FIND_FILE
        .get_or_init(|| {
            std::env::var_os(ENV_FIND_FILE)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        })
        .as_ref()
}

/// True when the hook is on (the variable is set). One cached read.
#[inline]
pub fn enabled() -> bool {
    find_file().is_some()
}

/// The tokens to look for, or `None` when the hook is off or the file lists none yet. The file is
/// re-read when its size or mtime changed, at most every 200 ms.
pub fn active_tokens() -> Option<Arc<Vec<String>>> {
    let path = find_file()?;
    let now = Instant::now();
    let mut guard = CACHE.lock();
    let cache = guard.get_or_insert_with(|| TokenCache {
        tokens: Arc::new(Vec::new()),
        checked: None,
        sig: None,
    });
    if cache
        .checked
        .is_none_or(|t| now.duration_since(t) >= RECHECK)
    {
        cache.checked = Some(now);
        if let Ok(meta) = std::fs::metadata(path) {
            let sig = (
                meta.len(),
                meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            );
            if cache.sig != Some(sig) {
                cache.sig = Some(sig);
                if let Ok(contents) = std::fs::read_to_string(path) {
                    cache.tokens = Arc::new(parse_tokens(&contents));
                }
            }
        }
    }
    (!cache.tokens.is_empty()).then(|| cache.tokens.clone())
}

/// The log line for a sighting; `rect` is `[x0, y0, x1, y1]` in logical pixels.
pub fn format_line(token: &str, rect: [f32; 4], scale: f32) -> String {
    format!(
        "appci_find token={token} x0={:.1} y0={:.1} x1={:.1} y1={:.1}",
        rect[0] * scale,
        rect[1] * scale,
        rect[2] * scale,
        rect[3] * scale
    )
}

/// Report that `token` was drawn at `rect` (`[x0, y0, x1, y1]`, logical window pixels). Logs once
/// per position change.
pub fn report(token: &str, rect: [f32; 4], scale: f32) {
    let key = [rect[0], rect[1], rect[2], rect[3]].map(|v| (v * scale).round() as i32);
    let log_it = REPORTER
        .lock()
        .get_or_insert_with(Reporter::default)
        .should_log(token, key, Instant::now());
    if log_it {
        log::info!(target: "warpui::appci_find", "{}", format_line(token, rect, scale));
    }
}

#[cfg(test)]
#[path = "appci_find_test.rs"]
mod tests;
