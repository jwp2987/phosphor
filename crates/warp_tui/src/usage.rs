//! Reusable context-window usage display for the TUI footer.
//!
//! BYOP replacement for Warp's cloud credits/cost usage entry. Warp's
//! `ConversationUsageTotals` (credits_spent / cost_in_cents) are both
//! server-computed and structurally zero in BYOP — there is no cloud credit
//! accounting, and providers return token counts rather than a dollar cost —
//! so the footer shows the one usage number BYOP actually has: the selected
//! conversation's context-window occupancy
//! ([`AIConversation::context_window_usage`], a 0.0–1.0 fraction Zap already
//! derives from provider token counts). The entry is informational: unlike
//! Warp's credits⇄cost entry it is not clickable and carries no persisted
//! display-mode setting.

use warpui_core::AppContext;
use warpui_core::elements::tui::{TuiElement, TuiText};

use crate::tui_builder::TuiUiBuilder;

/// Formats a context-window occupancy `fraction` (0.0–1.0) as a whole-percent
/// footer label, e.g. `0.183` → `"18% context"`.
pub(crate) fn format_context_usage(fraction: f32) -> String {
    let pct = (fraction.clamp(0.0, 1.0) * 100.0).round() as u32;
    format!("{pct}% context")
}

/// Renders the footer's context-window usage entry, dim like the rest of the
/// footer metadata. Informational only — unlike Warp's credits/cost entry it
/// is not clickable and carries no persisted display-mode setting.
pub(crate) fn render_context_usage_entry(fraction: f32, app: &AppContext) -> Box<dyn TuiElement> {
    let builder = TuiUiBuilder::from_app(app);
    TuiText::new(format_context_usage(fraction))
        .with_style(builder.muted_text_style())
        .truncate()
        .finish()
}

#[cfg(test)]
#[path = "usage_tests.rs"]
mod tests;
