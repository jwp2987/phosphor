//! Aggregates credit usage across an orchestrator and its locally-loaded
//! descendants for the agent-mode footer "View details" rollup.
//!
//! Pure function — no I/O, no network. Walks [`BlocklistAIHistoryModel`]
//! using the shared [`descendant_conversation_ids_in_spawn_order`] helper,
//! sums each loaded conversation's `credits_spent`, and emits a per-agent
//! breakdown consumed by `ConversationUsageView`.
//!
//! Ported from the pin's `app/src/ai/blocklist/usage/rollup.rs` (`02b53fcd8`).
//! Two divergences from the pin, both because the underlying data doesn't
//! exist in this BYOP fork:
//! * The pin also tracks `platform_credits_spent` (Warp-hosted credit
//!   consumption) and adds it into each total. This fork's
//!   `ConversationUsageMetadata` has no such field — `credits_spent` is
//!   already the sole, fully-local total.
//! * The pin's avatar helpers live in `orchestration_pill_bar`; this fork
//!   already split the same pure rendering (no pill-bar state) out into
//!   `agent_view::avatar_disc`, which is what this module's caller
//!   (`conversation_usage_view.rs`) imports instead.

use crate::ai::agent::conversation::{AIConversation, AIConversationId};
use crate::ai::blocklist::BlocklistAIHistoryModel;
use crate::ai::blocklist::orchestration_topology::descendant_conversation_ids_in_spawn_order;

/// Avatar identity for a row in the per-agent breakdown.
///
/// The actual rendering still requires a theme (which the rollup, being a
/// pure function, cannot consult), so this enum only carries the structural
/// information needed to choose a renderer at render time. The child variant
/// reuses `agent_view::avatar_disc`'s deterministic per-name color +
/// uppercase initial.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentAvatar {
    /// The orchestrator itself.
    Orchestrator,
    /// A descendant agent.
    Child,
}

/// One row in the per-agent credit breakdown list.
#[derive(Debug, Clone, PartialEq)]
pub struct PerAgentCreditEntry {
    pub conversation_id: AIConversationId,
    pub display_name: String,
    pub avatar: AgentAvatar,
    pub credits_spent: f32,
}

/// Aggregated credit usage for an orchestrator and its locally-loaded
/// descendants.
#[derive(Debug, Clone, PartialEq)]
pub struct OrchestrationCreditRollup {
    /// Sum of `credits_spent` across the orchestrator and every
    /// locally-loaded descendant.
    pub total_credits: f32,
    /// One entry per agent that has spent > 0 credits, sorted by
    /// `credits_spent` descending. Ties are broken by spawn order (earlier
    /// spawn first; orchestrator always sorts before its descendants in a
    /// tie).
    pub per_agent: Vec<PerAgentCreditEntry>,
}

/// Computes the orchestration credit rollup for `parent_id`.
///
/// Returns `None` when:
/// * the orchestrator has no locally-loaded descendants, OR
/// * the orchestrator and every loaded descendant have spent zero credits.
///
/// Unloaded descendants (IDs in the topology index without a matching
/// `AIConversation` in `conversations_by_id`) are silently skipped.
///
/// No conversation is counted twice. That is not enforced here: it comes from
/// [`descendant_conversation_ids_in_spawn_order`], which dedups by id and
/// never yields `parent_id` itself, so a descendant reachable from two parents
/// contributes one summand and one row, and the orchestrator cannot be added
/// both by the block above and by the descendant loop. Any totals-only
/// re-implementation of this sum elsewhere would have to reproduce that, which
/// is why the pill calls [`orchestration_headline_credits`] instead of
/// summing on its own.
pub fn compute_orchestration_rollup(
    parent_id: AIConversationId,
    history: &BlocklistAIHistoryModel,
) -> Option<OrchestrationCreditRollup> {
    // Descendants in spawn order so ties break naturally. The orchestrator
    // is prepended at index 0 so it sorts before its descendants at equal
    // credit totals.
    let descendant_ids = descendant_conversation_ids_in_spawn_order(history, parent_id);
    if descendant_ids.is_empty() {
        return None;
    }

    let mut total_credits: f32 = 0.0;
    let mut entries: Vec<(usize, PerAgentCreditEntry)> = Vec::new();

    if let Some(orchestrator) = history.conversation(&parent_id) {
        let credits = orchestrator.credits_spent();
        total_credits += credits;
        if credits > 0.0 {
            entries.push((
                0,
                PerAgentCreditEntry {
                    conversation_id: parent_id,
                    display_name: orchestrator_display_name(orchestrator),
                    avatar: AgentAvatar::Orchestrator,
                    credits_spent: credits,
                },
            ));
        }
    }

    for (spawn_idx, descendant_id) in descendant_ids.iter().enumerate() {
        let Some(descendant) = history.conversation(descendant_id) else {
            // Silently skip unloaded descendants.
            continue;
        };
        let credits = descendant.credits_spent();
        total_credits += credits;
        if credits > 0.0 {
            entries.push((
                spawn_idx + 1,
                PerAgentCreditEntry {
                    conversation_id: *descendant_id,
                    display_name: child_display_name(descendant),
                    avatar: AgentAvatar::Child,
                    credits_spent: credits,
                },
            ));
        }
    }

    if entries.is_empty() {
        return None;
    }

    // Sort by credits descending; ties broken by spawn order ascending so
    // the earlier-spawned agent appears first.
    entries.sort_by(|a, b| {
        b.1.credits_spent
            .partial_cmp(&a.1.credits_spent)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });

    Some(OrchestrationCreditRollup {
        total_credits,
        per_agent: entries.into_iter().map(|(_, entry)| entry).collect(),
    })
}

/// The credit total that goes on screen for `conversation_id`, shared by the
/// collapsed usage pill (`block/view_impl/output.rs`) and the expanded usage
/// footer's "Credits spent (total)" headline
/// (`ConversationUsageView::headline_total_credits`).
///
/// `rollup` is the caller's already-computed [`compute_orchestration_rollup`]
/// result, threaded in rather than recomputed so the footer — which needs the
/// rollup for its drill-down rows anyway — walks the tree once.
///
/// Both surfaces must route through this one function, and both must read the
/// number **live** from `history`. The pill re-derives on every render; the
/// footer's `ConversationUsageInfo` is a snapshot frozen when the footer was
/// opened (`terminal/view.rs::handle_usage_footer_toggled`). Reading the
/// snapshot for the no-rollup fallback put two totals for one conversation on
/// screen at once as soon as it spent anything more with the footer open — the
/// same "headline disagrees with what's under it" defect the rollup headline
/// was introduced to fix, displaced one level down into the fallback limb.
///
/// Returns `None` only when `conversation_id` is not loaded and no rollup
/// applies; the caller then has nothing live to read and must fall back to
/// whatever it was constructed with.
pub fn orchestration_headline_credits(
    conversation_id: AIConversationId,
    history: &BlocklistAIHistoryModel,
    rollup: Option<&OrchestrationCreditRollup>,
) -> Option<f32> {
    match rollup {
        Some(rollup) => Some(rollup.total_credits),
        None => history
            .conversation(&conversation_id)
            .map(AIConversation::credits_spent),
    }
}

/// Display name for the orchestrator row. Prefers the explicitly assigned
/// `agent_name`, falls back to "Orchestrator" so the row is always
/// meaningful.
fn orchestrator_display_name(orchestrator: &AIConversation) -> String {
    orchestrator
        .agent_name()
        .filter(|n| !n.is_empty())
        .map(|n| n.to_string())
        .unwrap_or_else(|| "Orchestrator".to_string())
}

/// Display name for a child row. Mirrors the orchestration pill bar's
/// fallback (`"Agent"`) so the breakdown stays consistent with the pill
/// labels when an agent hasn't been named yet.
fn child_display_name(child: &AIConversation) -> String {
    child
        .agent_name()
        .filter(|n| !n.is_empty())
        .map(|n| n.to_string())
        .unwrap_or_else(|| "Agent".to_string())
}

#[cfg(test)]
#[path = "rollup_tests.rs"]
mod tests;
