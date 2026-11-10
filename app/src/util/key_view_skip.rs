//! A typed character re-renders only the views whose output it changed (#1962, option A of
//! `docs/design/key-scene-build.md`).
//!
//! A notified view is rendered again and its fresh element tree is laid out cold, whether or not
//! it shows anything new. One typed character used to notify, besides the editor, the
//! autosuggestion keybinding hint, the input, the agent message bar, the terminal message bar,
//! the input button bar, the agent status bar and the queued-prompts panel. Each of those now
//! compares what it displays before it notifies; the comparison lives next to the view (search
//! for `key_view_skip::enabled`).
//!
//! Only notifications are skipped. No key is read, held or routed differently, no model changes
//! what it stores or emits, and every skipped notification is one whose render would have built
//! the same elements.
//!
//! Kill switch: `PHOSPHOR_KEY_VIEW_SKIP=0` restores every one of those notifications exactly. It
//! is read once, at the first use.

use std::sync::OnceLock;

/// `PHOSPHOR_KEY_VIEW_SKIP=0` turns the skips off; anything else (or unset) leaves them on.
pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        let enabled = enabled_from_env(std::env::var("PHOSPHOR_KEY_VIEW_SKIP").ok().as_deref());
        // Once per launch (at the first edit), so a run can tell which arm of an A/B an app log
        // belongs to: `input_order hop=key_view_skip enabled=<0|1>`.
        warpui::input_order::log_hop("key_view_skip", || format!("enabled={}", u8::from(enabled)));
        enabled
    })
}

pub(crate) fn enabled_from_env(value: Option<&str>) -> bool {
    value.is_none_or(|value| value.trim() != "0")
}

/// Whether a view has to be notified for a change that moved what it displays from `before` to
/// `after`. With the skips off every change notifies, as it did before #1962.
pub fn display_changed<T: PartialEq>(before: &T, after: &T, skip_enabled: bool) -> bool {
    !skip_enabled || before != after
}

#[cfg(test)]
#[path = "key_view_skip_tests.rs"]
mod tests;
