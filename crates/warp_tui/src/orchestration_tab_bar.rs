//! Shared orchestration tab-bar presentation for terminal sessions.
//!
//! Ported from the pin's `orchestration_tab_bar.rs`. Two adaptations from the
//! pin, both because this fork has no cloud-run orchestration:
//!
//! - The pin imports [`TuiOrchestrationSnapshot`] from a `orchestration_model`
//!   module: a singleton (`TuiOrchestrationModel`) that both builds this
//!   snapshot from live conversation state *and* dispatches child-agent
//!   creation, including a `StartAgentExecutionMode::Remote` branch that
//!   spawns agents on Warp's servers via `ServerApiProvider`/`ai_client`.
//!   That singleton has not been ported here — only the plain data shape
//!   ([`TuiOrchestrationChild`], [`TuiOrchestrationSnapshot`]) it hands to
//!   this module is defined below. Building a local-only snapshot builder
//!   (mirroring the already-ported, cloud-free `app::ai::blocklist::
//!   orchestration_topology` helpers used by the GUI pill bar) and wiring a
//!   snapshot source into [`crate::terminal_session_view`] is future work,
//!   not included in this change.
//! - `conversation_status_glyph`/`conversation_status_glyph_style` are
//!   copied from the pin's `agent_message.rs` rather than importing it: that
//!   file also renders full agent chat messages (a separate, larger,
//!   unported feature), and pulling in the whole module for two pure
//!   functions would be a much bigger and unrelated change than a tab bar.
use std::collections::HashMap;

use warp::tui_export::{AIConversationId, ConversationStatus};
use warpui_core::elements::tui::{TuiElement, TuiStyle, TuiText};
use warpui_core::keymap::macros::*;
use warpui_core::keymap::{ContextPredicate, EditableBinding, FixedBinding};
use warpui_core::{Action, AppContext};

use crate::keybindings::TUI_BINDING_GROUP;
use crate::orchestrated_agent_identity_styling::{AgentIdentity, assign_agent_identity_indices};
use crate::tab_bar::{
    TuiTab, TuiTabBarConfig, TuiTabBarNavigationDirection, TuiTabBarSecondaryEdge, TuiTabBarView,
};
use crate::tui_builder::TuiUiBuilder;

pub(crate) const ORCHESTRATION_TAB_BAR_FOCUSED_FLAG: &str = "TuiOrchestrationTabBarFocused";
const ORCHESTRATION_TAB_LABEL_MAX_COLUMNS: u16 = 20;

/// One navigable child conversation in an orchestration snapshot. Mirrors
/// the pin's `orchestration_model::TuiOrchestrationChild` — see the module
/// doc comment above for why the model that builds this is not ported yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TuiOrchestrationChild {
    pub(crate) conversation_id: AIConversationId,
    pub(crate) label: String,
    pub(crate) spawn_index: usize,
    pub(crate) status: ConversationStatus,
}

/// Live semantic state for the orchestration tab bar. Mirrors the pin's
/// `orchestration_model::TuiOrchestrationSnapshot` — see the module doc
/// comment above for why the model that builds this is not ported yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TuiOrchestrationSnapshot {
    pub(crate) root_conversation_id: AIConversationId,
    pub(crate) selected_conversation_id: AIConversationId,
    pub(crate) children: Vec<TuiOrchestrationChild>,
    /// Stable child ID used to resolve the page start at the current width.
    pub(crate) page_anchor: Option<AIConversationId>,
    /// Whether the tab bar may override the anchor to reveal the selection.
    pub(crate) reveal_selected: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum TuiOrchestrationTabNavigationAction {
    Previous,
    Next,
    FirstChild,
    LastChild,
}

impl TuiOrchestrationTabNavigationAction {
    pub(crate) fn target(self, tab_bar: &TuiTabBarView) -> Option<String> {
        match self {
            Self::Previous => tab_bar.navigation_target(TuiTabBarNavigationDirection::Previous),
            Self::Next => tab_bar.navigation_target(TuiTabBarNavigationDirection::Next),
            Self::FirstChild => tab_bar.secondary_edge_target(TuiTabBarSecondaryEdge::First),
            Self::LastChild => tab_bar.secondary_edge_target(TuiTabBarSecondaryEdge::Last),
        }
    }
}

pub(crate) fn register_orchestration_surface_bindings<A>(
    app: &mut AppContext,
    surface_context: ContextPredicate,
    interrupt_action: A,
    navigation_action: impl Fn(TuiOrchestrationTabNavigationAction) -> A,
) where
    A: Action,
{
    app.register_fixed_bindings([FixedBinding::new(
        "ctrl-c",
        interrupt_action,
        surface_context.clone(),
    )
    .with_group(TUI_BINDING_GROUP)]);

    let tab_context = surface_context & id!(ORCHESTRATION_TAB_BAR_FOCUSED_FLAG);
    app.register_editable_bindings([
        EditableBinding::new(
            "tui:orchestration_tabs:previous",
            "Select the previous orchestration tab",
            navigation_action(TuiOrchestrationTabNavigationAction::Previous),
        )
        .with_context_predicate(tab_context.clone())
        .with_group(TUI_BINDING_GROUP)
        .with_key_binding("left"),
        EditableBinding::new(
            "tui:orchestration_tabs:previous",
            "Select the previous orchestration tab",
            navigation_action(TuiOrchestrationTabNavigationAction::Previous),
        )
        .with_context_predicate(tab_context.clone())
        .with_group(TUI_BINDING_GROUP)
        .with_key_binding("shift-tab"),
        EditableBinding::new(
            "tui:orchestration_tabs:next",
            "Select the next orchestration tab",
            navigation_action(TuiOrchestrationTabNavigationAction::Next),
        )
        .with_context_predicate(tab_context.clone())
        .with_group(TUI_BINDING_GROUP)
        .with_key_binding("right"),
        EditableBinding::new(
            "tui:orchestration_tabs:next",
            "Select the next orchestration tab",
            navigation_action(TuiOrchestrationTabNavigationAction::Next),
        )
        .with_context_predicate(tab_context.clone())
        .with_group(TUI_BINDING_GROUP)
        .with_key_binding("tab"),
        EditableBinding::new(
            "tui:orchestration_tabs:first_child",
            "Select the first child agent",
            navigation_action(TuiOrchestrationTabNavigationAction::FirstChild),
        )
        .with_context_predicate(tab_context.clone())
        .with_group(TUI_BINDING_GROUP)
        .with_key_binding("shift-left"),
        EditableBinding::new(
            "tui:orchestration_tabs:last_child",
            "Select the last child agent",
            navigation_action(TuiOrchestrationTabNavigationAction::LastChild),
        )
        .with_context_predicate(tab_context)
        .with_group(TUI_BINDING_GROUP)
        .with_key_binding("shift-right"),
    ]);
}

pub(crate) fn orchestration_tab_bar_config(
    snapshot: &TuiOrchestrationSnapshot,
    focused: bool,
    builder: &TuiUiBuilder,
) -> TuiTabBarConfig {
    let palette = builder.agent_identity_palette();
    let mut children_in_spawn_order = snapshot.children.iter().collect::<Vec<_>>();
    children_in_spawn_order.sort_by_key(|child| child.spawn_index);
    let identity_indices = assign_agent_identity_indices(
        children_in_spawn_order
            .iter()
            .map(|child| child.label.as_str()),
        palette.len(),
    );
    let identity_by_conversation = children_in_spawn_order
        .into_iter()
        .map(|child| child.conversation_id)
        .zip(identity_indices)
        .collect::<HashMap<AIConversationId, usize>>();
    let tabs = snapshot
        .children
        .iter()
        .map(|child| {
            let identity = palette
                .get(
                    identity_by_conversation
                        .get(&child.conversation_id)
                        .copied()
                        .unwrap_or_default(),
                )
                .or_else(|| palette.first())
                .cloned()
                .unwrap_or_default();
            let (icon_glyph, icon_style) =
                orchestration_tab_icon(&child.status, &identity, builder);
            TuiTab::new(child.conversation_id.to_string(), child.label.clone())
                .with_leading_text(icon_glyph, icon_style)
        })
        .collect();
    let mut config = TuiTabBarConfig::new(tabs);
    config.leading = Some("   Agents:   ".to_owned());
    config.main_tab = Some(TuiTab::new(
        snapshot.root_conversation_id.to_string(),
        "orchestrator",
    ));
    config.selected_key = Some(snapshot.selected_conversation_id.to_string());
    config.focused = focused;
    config.page_anchor = snapshot.page_anchor.map(|id| id.to_string());
    config.reveal_selected = snapshot.reveal_selected;
    config.maximum_label_columns = Some(ORCHESTRATION_TAB_LABEL_MAX_COLUMNS);
    config.secondary_gap_columns = 3;
    config.styles = builder.orchestration_tab_bar_styles();
    config
}

pub(crate) fn render_orchestration_tab_footer(builder: &TuiUiBuilder) -> Box<dyn TuiElement> {
    let primary = builder.primary_text_style();
    let muted = builder.muted_text_style();
    TuiText::from_spans([
        ("Tab or ← →".to_string(), primary),
        (" to navigate  ".to_string(), muted),
        ("Shift + ← →".to_string(), primary),
        (" to go to start/end  ".to_string(), muted),
        ("↓".to_string(), primary),
        (" to send a message".to_string(), muted),
    ])
    .truncate()
    .finish()
}

pub(crate) fn orchestration_tab_icon(
    status: &ConversationStatus,
    identity: &AgentIdentity,
    builder: &TuiUiBuilder,
) -> (&'static str, TuiStyle) {
    match status {
        ConversationStatus::InProgress
        | ConversationStatus::TransientError
        | ConversationStatus::WaitingForEvents
        | ConversationStatus::Blocked { .. } => (
            conversation_status_glyph(status),
            conversation_status_glyph_style(status, builder),
        ),
        ConversationStatus::Success | ConversationStatus::Error | ConversationStatus::Cancelled => {
            (identity.glyph, identity.style)
        }
    }
}

/// Compact glyph for a conversation's lifecycle status. Copied from the
/// pin's `agent_message.rs` — see the module doc comment above.
fn conversation_status_glyph(status: &ConversationStatus) -> &'static str {
    match status {
        ConversationStatus::InProgress
        | ConversationStatus::TransientError
        | ConversationStatus::WaitingForEvents => "●",
        ConversationStatus::Success => "✓",
        ConversationStatus::Error => "×",
        ConversationStatus::Cancelled | ConversationStatus::Blocked { .. } => "■",
    }
}

/// Semantic theme style for a conversation's lifecycle glyph. Copied from
/// the pin's `agent_message.rs` — see the module doc comment above.
fn conversation_status_glyph_style(
    status: &ConversationStatus,
    builder: &TuiUiBuilder,
) -> TuiStyle {
    match status {
        ConversationStatus::InProgress
        | ConversationStatus::TransientError
        | ConversationStatus::WaitingForEvents
        | ConversationStatus::Blocked { .. } => builder.attention_glyph_style(),
        ConversationStatus::Success => builder.success_glyph_style(),
        ConversationStatus::Error => builder.error_text_style(),
        ConversationStatus::Cancelled => builder.muted_text_style(),
    }
}
