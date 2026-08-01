//! Plain-data option lists rendered by the TUI's option-selector and
//! ask-user-question surfaces.
//!
//! Extracted (verbatim, minus the orchestration builders) from upstream Warp's
//! `ai/orchestration/snapshots.rs`. Zap dropped the orchestration module, but
//! these types carry no orchestration/GUI coupling — they are the plain data
//! model shared between a question/option producer and the `warp_tui`
//! option renderer, re-exported via `tui_export`.

use warp_cli::agent::Harness;

/// One selectable row in an option snapshot. Carries no GUI types.
#[derive(Debug, Clone, PartialEq)]
pub struct OptionRow {
    pub id: String,
    pub label: String,
    /// Harness identifier for rows representing harnesses; each frontend
    /// maps it to its own icon (GUI `Icon`) or glyph/color (TUI).
    pub harness: Option<Harness>,
    pub badge: Option<OptionBadge>,
    pub disabled_reason: Option<String>,
}

impl OptionRow {
    /// Creates an enabled row with no badge or harness.
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            harness: None,
            badge: None,
            disabled_reason: None,
        }
    }
}

/// Secondary marker rendered next to a row's label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionBadge {
    Default,
    Recent,
    Connected,
    /// The option the agent recommends, surfaced next to its label.
    Recommended,
}

/// Load state of the catalog backing a snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptionSourceStatus {
    Ready,
    Loading,
    Failed { message: String },
    Empty { message: String },
}

/// Trailing affordance below the option list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptionFooter {
    /// Free-form text entry (e.g. custom host slug).
    CustomText { label: String },
    /// "New API key…" affordance. The GUI renders it for harnesses that
    /// support managed secrets; the TUI intentionally omits resource creation.
    CreateNewAuthSecret,
}

/// A complete option list for one configuration field.
#[derive(Debug, Clone, PartialEq)]
pub struct OptionSnapshot {
    pub rows: Vec<OptionRow>,
    pub selected_id: Option<String>,
    pub status: OptionSourceStatus,
    pub footer: Option<OptionFooter>,
}

impl OptionSnapshot {
    /// A `Ready` snapshot with no footer.
    pub fn ready(rows: Vec<OptionRow>, selected_id: Option<String>) -> Self {
        Self {
            rows,
            selected_id,
            status: OptionSourceStatus::Ready,
            footer: None,
        }
    }
}
