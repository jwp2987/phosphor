//! Writes the output of a just-finished SummarizeConversation stream back into
//! conversation.compaction_state — matching the state change + bus.publish(Compacted)
//! at the end of opencode's `compaction.ts processCompaction`.
//!
//! This module is independent from the controller, as a unit-testable helper (even
//! though the real call site is in controller.rs).

use warp_multi_agent_api as api;

use crate::ai::agent::conversation::AIConversation;

use super::algorithm::{prune_decisions, select, MessageRef};
use super::config::CompactionConfig;
use super::message_view::{build_tool_name_lookup, project};
use super::overflow::ModelLimit;
use super::state::CompletedCompaction;

/// Walks the conversation's root task backwards to find the last
/// `Message::AgentOutput` — that's the summary text the model just emitted.
///
/// `user_msg_id` (where the summary is shown) is the last user turn (`UserQuery` or
/// `InvokeSkill` — a conversation started with `/skill` has no `UserQuery` at all) *inside the
/// summarized head*; see `summary_anchor` (#1078).
///
/// Takes the last AgentOutput anywhere in the conversation. The controller uses
/// [`commit_summarization_for_stream`] instead, which only accepts the summary stream's own
/// output.
pub fn commit_summarization(
    conversation: &mut AIConversation,
    overflow: bool,
    cfg: &CompactionConfig,
) -> bool {
    commit_summarization_for_stream(conversation, overflow, cfg, |_| true)
}

/// [`commit_summarization`] restricted to the AgentOutput messages `is_summary_stream_message`
/// accepts (the summarize stream's own new messages).
///
/// Without the restriction, a summarize stream that produced no text (an empty response, or
/// a model that answered with a tool call, which is dropped on a summarize turn) committed the
/// conversation's previous AgentOutput — some earlier narration — as "the summary", hiding
/// the head behind text that does not summarize it.
pub fn commit_summarization_for_stream(
    conversation: &mut AIConversation,
    overflow: bool,
    cfg: &CompactionConfig,
    is_summary_stream_message: impl Fn(&str) -> bool,
) -> bool {
    // Consumed unconditionally: the note is about this stream only.
    let dropped_tool_calls =
        crate::ai::agent_providers::chat_stream::take_summary_stream_dropped_tool_calls(
            conversation.id(),
        );
    // Uses the conversation's existing linearized messages accessor — already merged in timestamp order across all tasks
    let mut all_msgs: Vec<&api::Message> = conversation.all_linearized_messages();
    all_msgs.sort_by_key(|m| {
        m.timestamp
            .as_ref()
            .map(|ts| (ts.seconds, ts.nanos))
            .unwrap_or((0, 0))
    });

    let last_agent_output: Option<(String, String)> = all_msgs.iter().rev().find_map(|m| {
        let inner = m.message.as_ref()?;
        match inner {
            api::message::Message::AgentOutput(a) if is_summary_stream_message(&m.id) => {
                Some((m.id.clone(), a.text.clone()))
            }
            _ => None,
        }
    });

    // Not a summary, so nothing to commit (and the recorded head, which belongs to this
    // request, is dropped so a later summarization can't pick it up):
    // - no AgentOutput from this stream at all;
    // - text that trims to nothing;
    // - text from a stream that answered with a tool call (dropped on a summarize turn): it is
    //   the narration that preceded the call ("Let me check the next file..."). Providers
    //   whose adapter ignores `tool_choice: none` produce exactly this, and committing it
    //   would hide the head — the latest tool results included — behind one sentence.
    // No length threshold beyond that: a genuine summary of a short history can be short,
    // and on the automatic tool-loop path a refused commit costs only an uncompacted
    // continue (the deferred summary resumes either way), while a wrong commit loses context.
    let summary =
        last_agent_output.filter(|(_, text)| !dropped_tool_calls && !text.trim().is_empty());
    let Some((assistant_id, summary_text)) = summary else {
        log::warn!(
            "[byop-compaction] commit: the summarize stream produced no summary (dropped_tool_calls={dropped_tool_calls}) — nothing to commit"
        );
        let _ = crate::ai::agent_providers::chat_stream::take_last_summarization_head(
            conversation.id(),
        );
        return false;
    };

    let assistant_id_str: &str = &assistant_id;
    let assistant_pos = all_msgs
        .iter()
        .position(|m| m.id.as_str() == assistant_id_str);
    // The head to hide is the head the summarizer was actually shown, recorded by
    // `build_chat_request` when it built this summarization request. Re-deriving it here does
    // not reproduce it, and the difference is not cosmetic: anything the re-derived head
    // covers that the request's head did not is hidden by `hidden_message_ids` from every
    // later request while being absent from the summary that replaced it — a message the user
    // sent that neither survives nor is represented. See
    // `chat_stream::LAST_SUMMARIZATION_HEAD` for the three reasons the two derivations
    // diverge (the summary output now sits in the final turn and moves the cut; `all_msgs`
    // here comes from `all_linearized_messages`, not the request's
    // `collect_linearized_task_messages`; and this list is timestamp-sorted where the request
    // was DFS-ordered).
    let recorded =
        crate::ai::agent_providers::chat_stream::take_last_summarization_head(conversation.id());
    let (head_message_ids, tail_start_id) = match recorded {
        Some(head) => (head.head_message_ids, head.tail_start_id),
        None => {
            // No recording for this conversation: a summarization interleaved from another
            // conversation overwrote the single slot, or this stream was not built by
            // `build_chat_request`. Re-derive, but only over the prefix that existed when the
            // request went out — everything from the summary output onwards is excluded,
            // which removes the one divergence that systematically *grows* the head and so
            // hides messages the summarizer never saw.
            log::warn!(
                "[byop-compaction] commit: no recorded summarization head for {:?} — \
                 re-deriving over the pre-summary prefix",
                conversation.id()
            );
            let prefix: Vec<&api::Message> = all_msgs[..assistant_pos.unwrap_or(all_msgs.len())]
                .iter()
                .copied()
                .collect();
            let tool_names = build_tool_name_lookup(prefix.iter().copied());
            let state_snapshot = conversation.compaction_state.clone();
            let views = project(&prefix, &state_snapshot, &tool_names);
            let select_result = select(&views, cfg, ModelLimit::FALLBACK, |slice| {
                slice.iter().map(MessageRef::estimate_size).sum()
            });
            let ids = prefix[..select_result.head_end]
                .iter()
                .map(|m| m.id.clone())
                .collect::<Vec<_>>();
            (ids, select_result.tail_start_id)
        }
    };
    let user_msg_id = summary_anchor(&all_msgs, &head_message_ids);
    let auto = overflow;
    let summary_len = summary_text.len();
    let completed = CompletedCompaction {
        user_msg_id: user_msg_id.clone(),
        assistant_msg_id: assistant_id.clone(),
        head_message_ids,
        tail_start_id,
        summary_text: Some(summary_text),
        auto,
        overflow,
    };
    log::info!(
        "[byop-compaction] commit: assistant_msg={} user_msg={} summary_len={} auto={} overflow={} head_count={} tail_start={:?}",
        assistant_id,
        user_msg_id,
        summary_len,
        auto,
        overflow,
        completed.head_message_ids.len(),
        completed.tail_start_id,
    );
    conversation.compaction_state.push_completed(completed);
    true
}

/// The message the summary is shown in place of (`CompletedCompaction::user_msg_id`): the
/// projection hides it and inserts the summary at its position, so it must be a message the
/// summary actually covers — one of `head_message_ids` — and the summary then sits where the
/// head was, before the kept tail (#1078).
///
/// It used to be the nearest user turn before the summary output, i.e. the *latest* user
/// message. The summarize request adds no user message of its own (opencode's does), so when
/// the kept tail started at or before that message it was hidden although it was never
/// summarized: the continuation lost the user's instruction for the current turn, and the
/// summary landed after the kept turns instead of before them.
///
/// The last user turn (`UserQuery` / `InvokeSkill`) in the head, else the head's last message
/// (a head without a user turn: a split inside one long turn). Only an empty head (nothing
/// summarized) falls back to a synthetic id, which hides nothing and is never inserted.
fn summary_anchor(all_msgs: &[&api::Message], head_message_ids: &[String]) -> String {
    let by_id: std::collections::HashMap<&str, &api::Message> =
        all_msgs.iter().map(|m| (m.id.as_str(), *m)).collect();
    head_message_ids
        .iter()
        .rev()
        .find(|id| {
            by_id.get(id.as_str()).is_some_and(|m| {
                matches!(
                    m.message,
                    Some(
                        api::message::Message::UserQuery(_) | api::message::Message::InvokeSkill(_)
                    )
                )
            })
        })
        .or_else(|| head_message_ids.last())
        .cloned()
        .unwrap_or_else(|| format!("compaction-trigger-{}", uuid::Uuid::new_v4()))
}

/// Automatically runs prune before every LLM request — matches opencode
/// `compaction.ts:297-341` 1:1.
///
/// Computes the decisions (which ToolCallResult outputs should be replaced with a
/// placeholder), then writes them to
/// `conversation.compaction_state.markers.tool_output_compacted_at`. The actual
/// replacement happens during `chat_stream::build_chat_request`'s projection (which
/// reads the marker).
///
/// A no-op when `cfg.prune == false`.
pub fn prune_now(conversation: &mut AIConversation, cfg: &CompactionConfig) -> usize {
    if !cfg.prune {
        return 0;
    }
    let all_msgs: Vec<&api::Message> = conversation.all_linearized_messages();
    if all_msgs.is_empty() {
        return 0;
    }
    let tool_names = build_tool_name_lookup(all_msgs.iter().copied());
    let state_snapshot = conversation.compaction_state.clone();
    let views = project(&all_msgs, &state_snapshot, &tool_names);
    // Uses a trait reference to avoid generic-inference ambiguity
    let views_ref: &[_] = &views;
    let decisions = prune_decisions::<super::message_view::WarpMessageView<'_>>(views_ref);
    if decisions.is_empty() {
        return 0;
    }
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let count = decisions.len();
    for (msg_id, _call_id) in decisions {
        // msg_id is the ToolCallResult's message id; mark_tool_compacted writes a timestamp onto the marker
        conversation
            .compaction_state
            .mark_tool_compacted(msg_id, now_ms);
    }
    log::info!("[byop-compaction] pruned {count} tool output(s)");
    count
}
