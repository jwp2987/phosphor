//! Capture buffer for the BYOP wire inspector.
//!
//! A process-wide ring buffer that records the *new* outbound messages sent to
//! the upstream model and the inbound messages that come back, so the floating
//! inspector window can filter by category, search, and diff the structured
//! context between exchanges.
//!
//! ## Design constraints (all from the feature requirements)
//!
//! - **Capture while armed**: when [`set_enabled`] is off, `capture_*` returns
//!   early — zero overhead. Capture is armed the first time the inspector opens
//!   and persists across the window closing/reopening (the window steals focus,
//!   so the user must close it to send a message, then reopen to view). The
//!   buffer is only wiped by the explicit "Clear" action.
//! - **Deltas only**: outbound records hold just the messages that are new this
//!   turn relative to the previous one, never the full history that ZAP re-sends
//!   every turn (the caller slices the delta out — see `chat_stream`).
//! - **Context window required**: if the model/provider has no context window
//!   defined, capture does nothing (see [`should_capture`]). Context usage is the
//!   core signal of this tool; if it can't be computed there is nothing to show.
//! - **Prompt body is not diffed**: the structured tools / skills / env snapshot
//!   lives in [`ContextSnapshot`] for the UI to highlight changes; the system
//!   prompt body is only shown as a payload.
//!
//! ## Decoupling from the UI
//!
//! The outbound/inbound capture points in `chat_stream` are free functions with
//! no `AppContext` (same constraint as `prompt_renderer`'s override slot), so the
//! state here is a process-wide static. The UI polls [`generation`] to notice
//! changes instead of relying on cross-layer events.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::RwLock;

use chrono::{DateTime, Local};

/// Ring buffer capacity. Older records are dropped once it is exceeded.
const CAP: usize = 200;

/// Direction of a record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Client -> model.
    Out,
    /// Model -> client.
    In,
}

/// Record category. The UI filter groups by this; each kind has a fixed direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The user / tool messages that are new this turn (outbound).
    UserDelta,
    /// Model response (text + tool calls).
    Response,
    /// Conversation title generation call.
    TitleGen,
    /// JSON one-shot call.
    Oneshot,
    /// Conversation compaction call.
    Compaction,
}

impl Kind {
    /// Stable slug used by the filter logic (the UI label goes through i18n; this
    /// is the stable identifier the filter keys on).
    pub fn slug(self) -> &'static str {
        match self {
            Kind::UserDelta => "user-delta",
            Kind::Response => "response",
            Kind::TitleGen => "title",
            Kind::Oneshot => "oneshot",
            Kind::Compaction => "compaction",
        }
    }

    /// Every category the filter UI iterates over.
    pub const ALL: [Kind; 5] = [
        Kind::UserDelta,
        Kind::Response,
        Kind::TitleGen,
        Kind::Oneshot,
        Kind::Compaction,
    ];
}

/// Structured context snapshot the UI diffs to highlight "what changed vs. the
/// previous record".
///
/// Holds the comparable structured fields plus the raw system prompt. The
/// structured fields are line-diffed; the system prompt is NOT char-diffed (per
/// the requirements) — the UI just shows it in full when it differs from the
/// previous turn, so you can confirm the actual prompt text going out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContextSnapshot {
    /// Tool names exposed to the model this turn (already gated).
    pub tools: Vec<String>,
    /// Skill names listed in the system prompt this turn.
    pub skills: Vec<String>,
    /// Environment key/values (cwd / shell / os / git, ...), sorted so the UI can
    /// diff line by line.
    pub env: Vec<(String, String)>,
    /// The full system prompt sent this turn (from `chat_req.system` or, under
    /// Anthropic shaping, the system-role messages). Shown when it changes.
    pub system: Option<String>,
}

/// A record payload: pretty-printed JSON when it can be serialized, otherwise a
/// one-line flag explaining why there is no JSON.
#[derive(Debug, Clone)]
pub enum Payload {
    /// Already pretty-printed JSON text.
    Json(String),
    /// Cannot / should not be JSON-encoded; carries the reason (e.g. "binary
    /// image blob").
    Flagged(String),
}

/// Token / context usage. Only inbound (Response) records carry this.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TokenUsage {
    pub prompt: i32,
    pub completion: i32,
    /// Real KV-cache occupancy reported by some Ollama-compatible servers, when
    /// present. Preferred over `prompt + completion` for `pct`, because those
    /// servers under-report `prompt` (newly-evaluated tokens only).
    pub active_kv_tokens: Option<i32>,
    pub context_window: u32,
    /// used / context_window as a percentage (0-100), where `used` is
    /// `active_kv_tokens` if present else `prompt + completion`.
    pub pct: f32,
}

impl TokenUsage {
    /// Derive the usage percentage. Prefers `active_kv_tokens` (true occupancy)
    /// over the raw prompt+completion sum; divides by the configured window.
    /// pct is 0 when the window is 0.
    pub fn new(
        prompt: i32,
        completion: i32,
        active_kv_tokens: Option<i32>,
        context_window: u32,
    ) -> Self {
        let used = match active_kv_tokens {
            Some(kv) if kv > 0 => kv,
            _ => prompt.max(0) + completion.max(0),
        } as f32;
        let pct = if context_window == 0 {
            0.0
        } else {
            used / context_window as f32 * 100.0
        };
        Self {
            prompt,
            completion,
            active_kv_tokens,
            context_window,
            pct,
        }
    }
}

/// One captured wire record.
#[derive(Debug, Clone)]
pub struct WireEntry {
    /// Monotonic sequence number; also the stable UI key.
    pub seq: u64,
    pub at: DateTime<Local>,
    pub direction: Direction,
    pub kind: Kind,
    pub model_id: String,
    /// Endpoint / adapter label (e.g. "Ollama @ http://host/v1").
    pub adapter: String,
    pub payload: Payload,
    /// Structured context snapshot (for diffing); outbound records only.
    pub context: Option<ContextSnapshot>,
    /// Token / context usage; inbound (Response) records only.
    pub usage: Option<TokenUsage>,
}

static ENABLED: AtomicBool = AtomicBool::new(false);
static SEQ: AtomicU64 = AtomicU64::new(0);
static BUF: RwLock<VecDeque<WireEntry>> = RwLock::new(VecDeque::new());

/// Turn capture on/off. Does NOT touch the buffer: capture persists across the
/// inspector window being closed and reopened (the window steals focus, so the
/// user must close it to send a message, then reopen to view). Wiping the buffer
/// is an explicit action via [`clear`] / the "Clear" button.
pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

/// Whether capture is currently on.
pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Whether this exchange should be captured: capture must be enabled AND the
/// provider/model must have a **defined context window**.
///
/// With no context window the whole inspector does nothing — context usage is its
/// core signal, and without it there is nothing meaningful to show. Both capture
/// sites (outbound and inbound) go through this gate first.
pub fn should_capture(context_window: Option<u32>) -> bool {
    is_enabled() && matches!(context_window, Some(w) if w > 0)
}

/// Change token the UI polls: bumped on every recorded entry. When it changes,
/// redraw.
pub fn generation() -> u64 {
    SEQ.load(Ordering::Relaxed)
}

/// Clear the buffer (leaves the enabled state untouched).
pub fn clear() {
    if let Ok(mut buf) = BUF.write() {
        buf.clear();
    }
}

/// A snapshot of the current buffer (shallow copy) for the UI to render. Oldest
/// first, newest last.
pub fn snapshot() -> Vec<WireEntry> {
    BUF.read()
        .map(|buf| buf.iter().cloned().collect())
        .unwrap_or_default()
}

/// Internal: assign the seq, enqueue, and enforce the CAP limit.
fn record(mut entry: WireEntry) {
    entry.seq = SEQ.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut buf) = BUF.write() {
        if buf.len() == CAP {
            buf.pop_front();
        }
        buf.push_back(entry);
    }
}

/// Capture an outbound record (the user/tool messages new this turn, or a
/// title/oneshot/compaction request).
///
/// Callers must pass the [`should_capture`] gate first. `context` is the
/// structured snapshot the UI diffs.
pub fn capture_out(
    kind: Kind,
    model_id: impl Into<String>,
    adapter: impl Into<String>,
    payload: Payload,
    context: Option<ContextSnapshot>,
) {
    if !is_enabled() {
        return;
    }
    record(WireEntry {
        seq: 0,
        at: Local::now(),
        direction: Direction::Out,
        kind,
        model_id: model_id.into(),
        adapter: adapter.into(),
        payload,
        context,
        usage: None,
    });
}

/// Capture an inbound record (model response + token/context usage).
pub fn capture_in(
    kind: Kind,
    model_id: impl Into<String>,
    adapter: impl Into<String>,
    payload: Payload,
    usage: Option<TokenUsage>,
) {
    if !is_enabled() {
        return;
    }
    record(WireEntry {
        seq: 0,
        at: Local::now(),
        direction: Direction::In,
        kind,
        model_id: model_id.into(),
        adapter: adapter.into(),
        payload,
        context: None,
        usage,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    /// Every test touches global statics, so they must run serially and reset
    /// state first.
    fn reset() {
        set_enabled(false);
        clear();
        SEQ.store(0, Ordering::Relaxed);
    }

    fn sample_out() {
        capture_out(
            Kind::UserDelta,
            "qwen2.5-coder",
            "Ollama @ local",
            Payload::Json("{}".into()),
            Some(ContextSnapshot::default()),
        );
    }

    #[test]
    #[serial]
    fn disabled_captures_nothing() {
        reset();
        // Not enabled -> both outbound and inbound are dropped.
        sample_out();
        capture_in(Kind::Response, "m", "a", Payload::Flagged("x".into()), None);
        assert!(snapshot().is_empty());
    }

    #[test]
    #[serial]
    fn reenabling_preserves_buffer() {
        // Capture must survive the window closing (disable) and reopening (enable)
        // so a message sent while closed is still visible on reopen — enabling
        // must NOT wipe the buffer.
        reset();
        set_enabled(true);
        sample_out(); // captured while "open"
        set_enabled(false); // window closed; capture still armed elsewhere in flow
        set_enabled(true); // reopened -> must not clear
        sample_out();
        assert_eq!(snapshot().len(), 2, "reopening must preserve prior entries");
        clear();
        assert!(snapshot().is_empty(), "explicit clear still empties the buffer");
    }

    #[test]
    #[serial]
    fn should_capture_requires_enabled_and_context_window() {
        reset();
        set_enabled(true);
        assert!(should_capture(Some(8192)));
        assert!(!should_capture(Some(0)), "a 0 window counts as undefined");
        assert!(!should_capture(None), "no context window -> do not capture");
        set_enabled(false);
        assert!(!should_capture(Some(8192)), "disabled -> never capture");
    }

    #[test]
    #[serial]
    fn ring_buffer_caps_and_keeps_newest() {
        reset();
        set_enabled(true);
        for _ in 0..(CAP + 25) {
            sample_out();
        }
        let snap = snapshot();
        assert_eq!(snap.len(), CAP, "never exceeds the cap");
        // Oldest were dropped: the first remaining seq is past the dropped ones.
        assert!(snap.first().unwrap().seq >= 25);
        // Newest is last; seq is monotonic.
        assert!(snap.last().unwrap().seq > snap.first().unwrap().seq);
    }

    #[test]
    #[serial]
    fn generation_advances_per_record() {
        reset();
        set_enabled(true);
        let g0 = generation();
        sample_out();
        let g1 = generation();
        assert!(g1 > g0, "generation advances after recording one entry");
    }

    #[test]
    #[serial]
    fn direction_and_usage_are_set_per_side() {
        reset();
        set_enabled(true);
        sample_out();
        capture_in(
            Kind::Response,
            "m",
            "a",
            Payload::Json("{}".into()),
            Some(TokenUsage::new(1000, 200, None, 8192)),
        );
        let snap = snapshot();
        assert_eq!(snap[0].direction, Direction::Out);
        assert!(snap[0].context.is_some());
        assert!(snap[0].usage.is_none());
        assert_eq!(snap[1].direction, Direction::In);
        assert!(snap[1].usage.is_some());
        assert!(snap[1].context.is_none());
    }

    #[test]
    fn token_usage_pct_math() {
        // Pure function, no globals, no serial needed.
        let u = TokenUsage::new(3000, 1000, None, 8000);
        assert!((u.pct - 50.0).abs() < 1e-3);
        let z = TokenUsage::new(100, 100, None, 0);
        assert_eq!(z.pct, 0.0, "a 0 window must not divide by zero");
        // active_kv_tokens wins over the under-reported prompt+completion sum.
        let kv = TokenUsage::new(214, 96, Some(10578), 30000);
        assert!((kv.pct - 35.26).abs() < 0.1, "uses active_kv/window: {}", kv.pct);
    }

    #[test]
    fn kind_all_covers_every_variant_slug() {
        // Regression guard: a new Kind that forgets ALL / slug.
        let mut slugs: Vec<&str> = Kind::ALL.iter().map(|k| k.slug()).collect();
        slugs.sort();
        slugs.dedup();
        assert_eq!(slugs.len(), Kind::ALL.len(), "slugs are unique and cover ALL");
    }
}
