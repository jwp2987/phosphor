//! Token estimation — matches opencode `packages/opencode/src/util/token.ts`.
//!
//! ```ts
//! const CHARS_PER_TOKEN = 4
//! export function estimate(input: string) {
//!   return Math.max(0, Math.round((input || "").length / CHARS_PER_TOKEN))
//! }
//! ```
//!
//! Uses `chars().count()` instead of `len()`, so UTF-8 multibyte characters don't
//! wildly skew the estimate. In JS, opencode's `.length` is 1 for characters within
//! the BMP, matching chars().count() in most cases; for emoji beyond the BMP, JS
//! counts 2 (a UTF-16 surrogate pair) while Rust's chars().count() counts 1 — this
//! small discrepancy has no real impact on head/tail splitting.
use super::consts::CHARS_PER_TOKEN;

/// Equivalent to `Math.round(len / 4)`. Returns 0 for an empty string.
pub fn estimate(input: &str) -> usize {
    let n = input.chars().count();
    // Math.round is standard rounding in JS (as opposed to banker's
    // round-to-even); `(n + 2) / 4` here is equivalent to `round(n / 4)` for
    // positive integers.
    (n + CHARS_PER_TOKEN / 2) / CHARS_PER_TOKEN
}

/// Estimate after JSON serialization — matches opencode `compaction.ts:241`:
/// `Token.estimate(JSON.stringify(msgs))`
pub fn estimate_json<T: serde::Serialize>(value: &T) -> usize {
    serde_json::to_string(value)
        .map(|s| estimate(&s))
        .unwrap_or(0)
}
