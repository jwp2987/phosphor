//! Shared orchestration tab-bar presentation for terminal sessions.
//!
//! Semantic topology, selection, and paging intent live in
//! [`crate::orchestration_model`]; this module translates that state into the
//! generic [`crate::tab_bar`] configuration and the session footers.
//!
//! With `FeatureFlag::MultiLevelOrchestration` enabled the snapshot carries one
//! drill-down level (breadcrumbs, anchor, direct children with subtree rollup
//! badges), and this module additionally attaches the narrow-width degradation
//! ladder as width-bounded presentation variants.
//!
//! Adaptations from the pin:
//!
//! - **Cloud-run orchestration is absent.** The pin pairs this with a
//!   `render_cloud_orchestration_tab_footer` for `TuiCloudRunView`; that view
//!   does not exist here (`DECLINED.md` #290), so only the local footers are
//!   defined. `StartAgentExecutionMode` correspondingly has no `Remote`
//!   variant in this fork.
//! - **The snapshot data shapes are declared in this module, not the model.**
//!   The pin declares [`TuiOrchestrationChild`] / [`TuiOrchestrationSnapshot`]
//!   / [`TuiOrchestrationBreadcrumb`] in `orchestration_model.rs`. They were
//!   defined here while the model was still unported, and are left in place
//!   rather than moved: relocating them is churn across every importer with
//!   no behavioral effect.
//! - `conversation_status_glyph`/`conversation_status_glyph_style` are
//!   copied from the pin's `agent_message.rs` rather than importing it: that
//!   file also renders full agent chat messages (a separate, larger,
//!   unported feature), and pulling in the whole module for two pure
//!   functions would be a much bigger and unrelated change than a tab bar.

#![allow(dead_code)]
// Staged port: this module came across from the pinned oracle (see `8c6d3a4c
// feat(tui): stage warp_tui crate ... (phase 0)` and the `port(tui)` commits) with
// the upstream API surface intact, but only the paths the TUI actually drives are
// wired up yet. The unused items here are upstream's, not ours.
//
// Kept rather than pruned because this fork re-pins against upstream roughly
// weekly (`ORACLE.md`); deleting upstream's helpers would turn each one into a
// re-pin conflict for no gain. Drop this attribute once the module is fully wired
// and check what is genuinely dead then.

use std::collections::HashMap;

use warp::tui_export::{AIConversationId, ConversationStatus, LoadedSubtreeRollup};
use warpui::SingletonEntity;
use warpui_core::elements::tui::{TuiElement, TuiStyle, TuiText};
use warpui_core::keymap::macros::*;
use warpui_core::keymap::{ContextPredicate, EditableBinding, FixedBinding};
use warpui_core::{Action, AppContext};

use crate::keybindings::TUI_BINDING_GROUP;
use crate::orchestrated_agent_identity_styling::{AgentIdentity, assign_agent_identity_indices};
use crate::orchestration_model::TuiOrchestrationModel;
use crate::tab_bar::{
    TuiTab, TuiTabBarConfig, TuiTabBarNarrowVariant, TuiTabBarNavigationDirection,
    TuiTabBarSecondaryEdge, TuiTabBarView,
};
use crate::tui_builder::TuiUiBuilder;

pub(crate) const ORCHESTRATION_TAB_BAR_FOCUSED_FLAG: &str = "TuiOrchestrationTabBarFocused";
const ORCHESTRATION_TAB_LABEL_MAX_COLUMNS: u16 = 20;
/// Marker rendered before a breadcrumb chip's label.
const BREADCRUMB_MARKER: &str = "‹";
/// Marker leading a group child's subtree rollup badge.
const ROLLUP_BADGE_MARKER: &str = "▸";

/// One navigable child conversation in an orchestration snapshot.
///
/// **Divergence from the pin:** upstream declares this (and
/// [`TuiOrchestrationSnapshot`], [`TuiOrchestrationBreadcrumb`]) in
/// `orchestration_model.rs`. They were defined here when the model had not
/// been ported yet, and are deliberately left in place: moving them now is
/// pure churn across every importer, and the model that builds them is
/// `crate::orchestration_model` either way.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TuiOrchestrationChild {
    pub(crate) conversation_id: AIConversationId,
    pub(crate) label: String,
    pub(crate) spawn_index: usize,
    pub(crate) status: ConversationStatus,
    /// Rolled-up loaded subtree beneath this child. `Some` only for a
    /// **group** child (at least one loaded descendant) while the
    /// multi-level presentation is enabled.
    pub(crate) subtree_rollup: Option<LoadedSubtreeRollup>,
}

/// One selectable ancestor chip shown while the bar is drilled below the root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TuiOrchestrationBreadcrumb {
    pub(crate) conversation_id: AIConversationId,
    pub(crate) label: String,
}

/// Live semantic state for the orchestration tab bar.
///
/// With `FeatureFlag::MultiLevelOrchestration` enabled the snapshot holds one
/// **level** of the tree — the drill-down anchor plus its direct children —
/// mirroring the GUI pill bar's `drill_down_anchor_id` rule. With the flag
/// disabled it keeps the historical flat projection: the anchor is the tree
/// root and `children` lists every navigable descendant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TuiOrchestrationSnapshot {
    pub(crate) root_conversation_id: AIConversationId,
    /// The conversation whose level the bar renders. Equals the root at
    /// orchestration depth 1 and whenever multi-level is disabled.
    pub(crate) anchor_conversation_id: AIConversationId,
    /// Main-tab label: the anchor's agent name, or `ORCHESTRATOR_TAB_LABEL`
    /// when the anchor is the tree root.
    pub(crate) anchor_label: String,
    /// Aggregated status of the anchor's subtree, shown as the main tab's
    /// leading glyph. `None` while multi-level is disabled.
    pub(crate) anchor_status: Option<ConversationStatus>,
    /// Whether the anchor maps to a retained, focusable session. A leaf's
    /// parent can be loaded but sessionless (e.g. a restored child whose
    /// session did not come back); its tab still frames the level but cannot
    /// be selected.
    pub(crate) anchor_navigable: bool,
    /// Ancestor chips rendered before the anchor: the tree root, plus the
    /// anchor's direct parent when the anchor sits 2+ levels below the root
    /// and maps to a retained session.
    pub(crate) breadcrumbs: Vec<TuiOrchestrationBreadcrumb>,
    pub(crate) selected_conversation_id: AIConversationId,
    pub(crate) children: Vec<TuiOrchestrationChild>,
    /// Stable child ID used to resolve the page start at the current width.
    pub(crate) page_anchor: Option<AIConversationId>,
    /// Whether the tab bar may override the anchor to reveal the selection.
    pub(crate) reveal_selected: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum TuiOrchestrationTabNavigationAction {
    /// `←`: the previous tab in the rendered row (breadcrumbs, anchor, level
    /// children), wrapping across the row's ends.
    Previous,
    /// `→`: the next tab in the rendered row.
    Next,
    /// `Shift+Tab`: the previous conversation in the whole orchestration
    /// tree (root plus every navigable descendant in pill order).
    TreePrevious,
    /// `Tab`: the next conversation in the whole orchestration tree.
    TreeNext,
    FirstChild,
    LastChild,
}

impl TuiOrchestrationTabNavigationAction {
    pub(crate) fn target(self, tab_bar: &TuiTabBarView, ctx: &AppContext) -> Option<String> {
        match self {
            Self::Previous => tab_bar.navigation_target(TuiTabBarNavigationDirection::Previous),
            Self::Next => tab_bar.navigation_target(TuiTabBarNavigationDirection::Next),
            Self::TreePrevious => {
                tree_navigation_target(tab_bar, TuiTabBarNavigationDirection::Previous, ctx)
            }
            Self::TreeNext => {
                tree_navigation_target(tab_bar, TuiTabBarNavigationDirection::Next, ctx)
            }
            Self::FirstChild => tab_bar.secondary_edge_target(TuiTabBarSecondaryEdge::First),
            Self::LastChild => tab_bar.secondary_edge_target(TuiTabBarSecondaryEdge::Last),
        }
    }
}

/// Resolves the adjacent conversation in the tree-wide keyboard-cycling
/// order. With the bar's flat flag-off projection the row and the tree
/// coincide, so this matches the historical `Tab` behavior exactly.
fn tree_navigation_target(
    tab_bar: &TuiTabBarView,
    direction: TuiTabBarNavigationDirection,
    ctx: &AppContext,
) -> Option<String> {
    if !ctx.has_singleton_model::<TuiOrchestrationModel>() {
        return None;
    }
    let selected = AIConversationId::try_from(tab_bar.selected_key()?.to_owned()).ok()?;
    TuiOrchestrationModel::as_ref(ctx)
        .adjacent_tree_conversation(selected, direction, ctx)
        .map(|conversation_id| conversation_id.to_string())
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
            "Select the previous orchestration tab in the row",
            navigation_action(TuiOrchestrationTabNavigationAction::Previous),
        )
        .with_context_predicate(tab_context.clone())
        .with_group(TUI_BINDING_GROUP)
        .with_key_binding("left"),
        EditableBinding::new(
            "tui:orchestration_tabs:next",
            "Select the next orchestration tab in the row",
            navigation_action(TuiOrchestrationTabNavigationAction::Next),
        )
        .with_context_predicate(tab_context.clone())
        .with_group(TUI_BINDING_GROUP)
        .with_key_binding("right"),
        // Deliberate split of the old alias pair (upstream 683d40782, per the
        // approved spec): `←`/`→` stay row-scoped, `Tab`/`Shift+Tab` become
        // tree-scoped. Do not re-unify.
        EditableBinding::new(
            "tui:orchestration_tabs:tree_previous",
            "Select the previous agent in the orchestration tree",
            navigation_action(TuiOrchestrationTabNavigationAction::TreePrevious),
        )
        .with_context_predicate(tab_context.clone())
        .with_group(TUI_BINDING_GROUP)
        .with_key_binding("shift-tab"),
        EditableBinding::new(
            "tui:orchestration_tabs:tree_next",
            "Select the next agent in the orchestration tree",
            navigation_action(TuiOrchestrationTabNavigationAction::TreeNext),
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

/// How a group child's `▸N` rollup badge renders at one ladder tier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BadgeDisplay {
    /// Marker plus loaded-descendant count (`▸3`).
    Full,
    /// Marker alone (`▸`).
    MarkerOnly,
    /// No badge.
    Hidden,
}

/// One presentation rung of the bar: label caps, chrome, and badge form.
struct LevelPresentation {
    leading: &'static str,
    /// Breadcrumb label cap; `None` collapses chips to their `‹` marker.
    breadcrumb_label_columns: Option<u16>,
    /// Anchor label cap; `None` collapses the anchor to its status glyph.
    anchor_label_columns: Option<u16>,
    child_label_columns: u16,
    badge: BadgeDisplay,
}

/// Full-width presentation (spec tier T0).
const BASE_PRESENTATION: LevelPresentation = LevelPresentation {
    leading: "   Agents:   ",
    breadcrumb_label_columns: Some(12),
    anchor_label_columns: Some(ORCHESTRATION_TAB_LABEL_MAX_COLUMNS),
    child_label_columns: ORCHESTRATION_TAB_LABEL_MAX_COLUMNS,
    badge: BadgeDisplay::Full,
};

/// The `Agents:` leading collapsed to two cells of padding (tier T2+).
const COLLAPSED_LEADING: &str = "  ";

/// The narrow-width degradation ladder (spec rules 47-50): each rung applies
/// strictly below its width, shedding chrome before content. The boundaries
/// are tunable defaults; the drop order is normative.
const NARROW_TIERS: [(u16, LevelPresentation); 5] = [
    (
        96,
        LevelPresentation {
            leading: "   Agents:   ",
            breadcrumb_label_columns: Some(8),
            anchor_label_columns: Some(ORCHESTRATION_TAB_LABEL_MAX_COLUMNS),
            child_label_columns: 16,
            badge: BadgeDisplay::Full,
        },
    ),
    (
        84,
        LevelPresentation {
            leading: COLLAPSED_LEADING,
            breadcrumb_label_columns: Some(8),
            anchor_label_columns: Some(ORCHESTRATION_TAB_LABEL_MAX_COLUMNS),
            child_label_columns: 16,
            badge: BadgeDisplay::Full,
        },
    ),
    (
        72,
        LevelPresentation {
            leading: COLLAPSED_LEADING,
            breadcrumb_label_columns: None,
            anchor_label_columns: Some(8),
            child_label_columns: 16,
            badge: BadgeDisplay::Full,
        },
    ),
    (
        64,
        LevelPresentation {
            leading: COLLAPSED_LEADING,
            breadcrumb_label_columns: None,
            anchor_label_columns: None,
            child_label_columns: 12,
            badge: BadgeDisplay::MarkerOnly,
        },
    ),
    (
        56,
        LevelPresentation {
            leading: COLLAPSED_LEADING,
            breadcrumb_label_columns: None,
            anchor_label_columns: None,
            child_label_columns: 8,
            badge: BadgeDisplay::Hidden,
        },
    ),
];

pub(crate) fn orchestration_tab_bar_config(
    snapshot: &TuiOrchestrationSnapshot,
    focused: bool,
    builder: &TuiUiBuilder,
) -> TuiTabBarConfig {
    let mut config = level_tab_bar_config(snapshot, focused, builder, &BASE_PRESENTATION);
    // The degradation ladder is part of the multi-level presentation; the
    // flag-off flat projection keeps its historical narrow behavior.
    if snapshot.anchor_status.is_some() {
        config.narrow_variants = NARROW_TIERS
            .iter()
            .map(
                |(max_width_exclusive, presentation)| TuiTabBarNarrowVariant {
                    max_width_exclusive: *max_width_exclusive,
                    config: level_tab_bar_config(snapshot, focused, builder, presentation),
                },
            )
            .collect();
    }
    config
}

/// Builds one width tier's complete tab-bar configuration for a level.
fn level_tab_bar_config(
    snapshot: &TuiOrchestrationSnapshot,
    focused: bool,
    builder: &TuiUiBuilder,
    presentation: &LevelPresentation,
) -> TuiTabBarConfig {
    let styles = builder.orchestration_tab_bar_styles();
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
            let mut tab = TuiTab::new(child.conversation_id.to_string(), child.label.clone())
                .with_leading_text(icon_glyph, icon_style);
            if let (Some(rollup), Some(badge)) = (
                child.subtree_rollup.as_ref(),
                rollup_badge_text(child.subtree_rollup.as_ref(), presentation.badge),
            ) {
                tab = tab.with_trailing_text(badge, rollup_badge_style(&rollup.status, builder));
            }
            tab
        })
        .collect();

    let mut config = TuiTabBarConfig::new(tabs);
    config.leading = Some(presentation.leading.to_owned());
    config.breadcrumb_tabs = snapshot
        .breadcrumbs
        .iter()
        .map(|breadcrumb| {
            let label = match presentation.breadcrumb_label_columns {
                Some(_) => breadcrumb.label.clone(),
                None => String::new(),
            };
            let mut tab = TuiTab::new(breadcrumb.conversation_id.to_string(), label)
                .with_leading_text(BREADCRUMB_MARKER, styles.chrome);
            if let Some(columns) = presentation.breadcrumb_label_columns {
                tab = tab.with_max_label_columns(columns);
            }
            tab
        })
        .collect();
    let anchor_label = match presentation.anchor_label_columns {
        // Glyph-only anchors only occur on ladder tiers, where the anchor
        // always carries a status glyph.
        None if snapshot.anchor_status.is_some() => String::new(),
        _ => snapshot.anchor_label.clone(),
    };
    let mut main_tab = TuiTab::new(snapshot.anchor_conversation_id.to_string(), anchor_label)
        .with_selectable(snapshot.anchor_navigable);
    if let Some(status) = &snapshot.anchor_status {
        main_tab = main_tab.with_leading_text(
            conversation_status_glyph(status),
            conversation_status_glyph_style(status, builder),
        );
    }
    if let Some(columns) = presentation.anchor_label_columns {
        main_tab = main_tab.with_max_label_columns(columns);
    }
    config.main_tab = Some(main_tab);
    config.selected_key = Some(snapshot.selected_conversation_id.to_string());
    config.focused = focused;
    config.page_anchor = snapshot.page_anchor.map(|id| id.to_string());
    config.reveal_selected = snapshot.reveal_selected;
    config.maximum_label_columns = Some(presentation.child_label_columns);
    config.secondary_gap_columns = 3;
    config.styles = styles;
    config
}

/// The badge text for a group child at one tier, or `None` for leaves and
/// badge-shedding tiers.
fn rollup_badge_text(rollup: Option<&LoadedSubtreeRollup>, badge: BadgeDisplay) -> Option<String> {
    let rollup = rollup?;
    match badge {
        BadgeDisplay::Full => Some(format!("{ROLLUP_BADGE_MARKER}{}", rollup.descendant_count)),
        BadgeDisplay::MarkerOnly => Some(ROLLUP_BADGE_MARKER.to_owned()),
        BadgeDisplay::Hidden => None,
    }
}

/// The `▸N` badge color for a subtree's aggregated status, per design
/// review: **yellow** while any descendant is working or stuck (running,
/// recovering, waiting for events — alive and resumable per QUALITY-780 — or
/// blocked), **red** when the settled subtree contains a failure, and
/// **neutral_7** when everything settled without one (success or cancelled).
fn rollup_badge_style(status: &ConversationStatus, builder: &TuiUiBuilder) -> TuiStyle {
    match status {
        ConversationStatus::InProgress
        | ConversationStatus::TransientError
        | ConversationStatus::WaitingForEvents
        | ConversationStatus::Blocked { .. } => builder.attention_glyph_style(),
        ConversationStatus::Error => builder.error_text_style(),
        ConversationStatus::Success | ConversationStatus::Cancelled => {
            builder.neutral_7_text_style()
        }
    }
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

/// Footer shown when a **child** tab is selected in the local orchestration
/// tab bar. Extends the standard navigation hint with a kill shortcut, naming
/// the blast radius when the child has loaded descendants.
pub(crate) fn render_orchestration_child_selected_tab_footer(
    builder: &TuiUiBuilder,
    nested_descendants: usize,
) -> Box<dyn TuiElement> {
    let primary = builder.primary_text_style();
    let muted = builder.muted_text_style();
    TuiText::from_spans([
        ("Tab or ← →".to_string(), primary),
        (" to navigate  ".to_string(), muted),
        ("Shift + ← →".to_string(), primary),
        (" to go to start/end  ".to_string(), muted),
        ("↓".to_string(), primary),
        (" to send a message  ".to_string(), muted),
        ("Ctrl+C ".to_string(), primary),
        (kill_hint_text(nested_descendants), muted),
    ])
    .truncate()
    .finish()
}

/// Kill-hint copy naming the subtree blast radius for group children.
fn kill_hint_text(nested_descendants: usize) -> String {
    if nested_descendants > 0 {
        format!("to kill sub-agent +{nested_descendants} nested")
    } else {
        "to kill sub-agent".to_string()
    }
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

#[cfg(test)]
#[path = "orchestration_tab_bar_tests.rs"]
mod tests;
