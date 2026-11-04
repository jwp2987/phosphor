//! Compaction sidecar state — attached to `AIConversation`, decoupled from warp's
//! `api::Message` protocol.
//!
//! Because warp's `api::Message` comes from an external protobuf dependency
//! (`agent_proto`), fields like `is_summary` / `compacted` can't be added to
//! it; this sidecar indexes by message_id to attach this "compaction metadata" on the
//! conversation side instead.
//!
//! The serialization version number [`CompactionState::VERSION`] is bumped manually
//! as the schema evolves; an old conversation that fails to deserialize degrades to
//! `Default` (equivalent to "never compacted").

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

/// The source that triggered compaction. `Auto` is only auto-triggered by token-overflow; `Manual` is /compact /compact-and.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CompactionTrigger {
    Manual,
    Auto,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessageMarker {
    /// This assistant message is a summary, whose content is used to replace the
    /// preceding history when assembling a request.
    #[serde(default)]
    pub is_summary: bool,
    /// This user message is a compaction-trigger placeholder (opencode
    /// `parts.some(p => p.type === "compaction")`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compaction_trigger: Option<CompactionTrigger>,
    /// This ToolCallResult's output has been pruned, replaced with a placeholder
    /// during projection. Unix epoch ms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_output_compacted_at: Option<u64>,
    /// Marker for a synthesized user "Continue..." synthetic message on an
    /// auto-resume run (matches opencode `metadata.compaction_continue`).
    #[serde(default)]
    pub synthetic_continue: bool,
}

/// A completed compaction interval (matches an item returned by opencode's
/// `completedCompactions()`).
///
/// `user_msg_id` is the user message that triggered the summary (carrying a
/// compaction_trigger marker), `assistant_msg_id` is the synthesized summary
/// AgentOutput message. Both are treated as superseded in
/// [`CompactionState::hidden_message_ids`] and skipped during projection — but the
/// summary text itself is pulled out and substituted into the head region.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletedCompaction {
    pub user_msg_id: String,
    pub assistant_msg_id: String,
    /// The head-region message ids covered by this summary; all hidden when projecting a normal request.
    #[serde(default)]
    pub head_message_ids: Vec<String>,
    /// The tail's starting message id, used for split validation / debugging.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tail_start_id: Option<String>,
    /// The summary content (could also be taken directly from the assistant
    /// message, but caching it in state makes it convenient for build_prompt to get
    /// previous_summary).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_text: Option<String>,
    pub auto: bool,
    pub overflow: bool,
}

impl CompletedCompaction {
    /// Whether the anchor (`user_msg_id`) is one of the summarized messages. False only for
    /// compactions persisted before #1078, which anchored at the kept tail's user message.
    fn anchor_is_summarized(&self) -> bool {
        self.head_message_ids.is_empty() || self.head_message_ids.contains(&self.user_msg_id)
    }

    /// Whether this compaction can be shown in a request whose messages `message_exists`
    /// answers for: it has a non-blank summary and its insertion target is present. Only such
    /// a compaction hides its head or is shown.
    fn is_shown(&self, message_exists: &impl Fn(&str) -> bool) -> bool {
        self.summary().is_some() && message_exists(self.insertion_target())
    }

    /// The summary text, if it is not blank.
    fn summary(&self) -> Option<&str> {
        self.summary_text
            .as_deref()
            .filter(|text| !text.trim().is_empty())
    }

    /// The message the summary is shown before: the anchor, or for a legacy compaction (anchor
    /// outside its head) the kept tail's first message.
    fn insertion_target(&self) -> &str {
        if self.anchor_is_summarized() {
            &self.user_msg_id
        } else {
            self.tail_start_id.as_deref().unwrap_or(&self.user_msg_id)
        }
    }
}

/// A sidecar table persisted alongside `AIConversation`.
///
/// Default value = empty table = uncompacted state, completely non-invasive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionState {
    /// Schema version, bumped as it evolves.
    #[serde(default = "CompactionState::current_version")]
    pub version: u32,
    #[serde(default)]
    markers: HashMap<String, MessageMarker>,
    #[serde(default)]
    completed: Vec<CompletedCompaction>,
}

impl Default for CompactionState {
    fn default() -> Self {
        Self {
            version: Self::VERSION,
            markers: HashMap::new(),
            completed: Vec::new(),
        }
    }
}

impl CompactionState {
    pub const VERSION: u32 = 2;
    fn current_version() -> u32 {
        Self::VERSION
    }

    pub fn marker(&self, msg_id: &str) -> Option<&MessageMarker> {
        self.markers.get(msg_id)
    }

    /// Writes a marker (merged onto any existing marker, rather than overwriting the whole marker).
    pub fn upsert_marker(&mut self, msg_id: impl Into<String>, f: impl FnOnce(&mut MessageMarker)) {
        let entry = self.markers.entry(msg_id.into()).or_default();
        f(entry);
    }

    /// Marks a ToolCallResult's output as pruned.
    pub fn mark_tool_compacted(&mut self, msg_id: impl Into<String>, now_ms: u64) {
        self.upsert_marker(msg_id, |m| m.tool_output_compacted_at = Some(now_ms));
    }

    /// Pushes a completed compaction.
    pub fn push_completed(&mut self, c: CompletedCompaction) {
        // Marks both the user and assistant messages in sync (making them individually recognizable during projection).
        self.upsert_marker(c.user_msg_id.clone(), |m| {
            m.compaction_trigger = Some(if c.auto {
                CompactionTrigger::Auto
            } else {
                CompactionTrigger::Manual
            });
        });
        self.upsert_marker(c.assistant_msg_id.clone(), |m| m.is_summary = true);
        self.completed.push(c);
    }

    /// Marks a synthetic "Continue..." user message (synthesized on the auto+overflow path).
    pub fn mark_synthetic_continue(&mut self, msg_id: impl Into<String>) {
        self.upsert_marker(msg_id, |m| m.synthetic_continue = true);
    }

    /// Gets the last completed compaction (used as the incremental-summary anchor for [`super::prompt::build_prompt`]).
    ///
    /// The last compaction with non-blank summary text — the same one
    /// [`Self::summary_insertions`] shows, so the summary a request is built on and the one
    /// the summarizer is asked to update never disagree. (An early build stored `Some("")`
    /// for a blank reply.)
    pub fn previous_summary(&self) -> Option<&str> {
        self.completed
            .iter()
            .rev()
            .find_map(CompletedCompaction::summary)
    }

    pub fn completed(&self) -> &[CompletedCompaction] {
        &self.completed
    }

    /// True when the state carries neither a completed compaction nor any per-message marker
    /// (prune / synthetic-continue markers exist without a compaction), so there is nothing
    /// worth persisting.
    pub fn is_empty(&self) -> bool {
        self.completed.is_empty() && self.markers.is_empty()
    }

    /// The completed compaction whose summary is the assistant message `msg_id`, if that
    /// message is a committed summary.
    pub fn completed_for_summary(&self, msg_id: &str) -> Option<&CompletedCompaction> {
        self.completed
            .iter()
            .rev()
            .find(|c| c.assistant_msg_id == msg_id)
    }

    /// All message ids that should be skipped when assembling a request (matches
    /// opencode `hidden`): each completed compaction interval's head_message_ids +
    /// user_msg_id + assistant_msg_id.
    ///
    /// Note: this is only "the set of message ids meant to be hidden from history",
    /// and does **not** include the summary itself — the summary text is inserted
    /// as a synthesized message overriding the compaction trigger's user_msg_id
    /// position during request projection.
    ///
    /// `user_msg_id` is hidden only when it is part of what was summarized (in
    /// `head_message_ids`, or the head is empty). Compactions persisted before #1078 anchored
    /// at the latest user message, which sat in the kept tail; hiding it dropped the user's
    /// instruction for that turn, so for those the anchor stays visible and the summary is
    /// shown before the tail instead (see [`Self::summary_insertions`]).
    pub fn hidden_message_ids(&self) -> HashSet<String> {
        self.hidden_message_ids_where(|_| true)
    }

    /// [`Self::hidden_message_ids`] for a request whose messages `message_exists` answers for.
    /// Skips exactly the compactions [`Self::summary_insertions`] skips — no non-blank summary,
    /// or an insertion target that is not in the request — so a head is never hidden without
    /// a summary that could stand in for it.
    pub fn hidden_message_ids_where(
        &self,
        message_exists: impl Fn(&str) -> bool,
    ) -> HashSet<String> {
        let mut out = HashSet::new();
        for c in self
            .completed
            .iter()
            .filter(|c| c.is_shown(&message_exists))
        {
            out.extend(c.head_message_ids.iter().cloned());
            if c.anchor_is_summarized() {
                out.insert(c.user_msg_id.clone());
            }
            out.insert(c.assistant_msg_id.clone());
        }
        out
    }

    /// Where the summary is shown in a request: `(message id, summary text)` — the summary
    /// pair goes right before that message (which is itself hidden when it is the anchor).
    ///
    /// Only the latest summary is shown, at most one. Summaries are cumulative: every
    /// summarize request carries the last summary as `<previous-summary>`
    /// ([`Self::previous_summary`]) and is told to update it, so the latest one already holds
    /// what the earlier ones said. Showing every summary of a chain stacked them in front of
    /// the tail (each auto summary is followed by the continue message, which becomes the
    /// next anchor), and a rule based on head coverage still stacked them when a later head
    /// happened to be shorter. Earlier summaries stay hidden with their heads. A compaction
    /// without summary text supersedes nothing.
    ///
    /// A legacy compaction whose anchor is not in its own head (see
    /// [`Self::hidden_message_ids`]) is shown before its `tail_start_id` instead.
    ///
    /// `message_exists` filters out a compaction whose insertion target is no longer in the
    /// request's messages (a rewind removed it): its summary could not be shown, and picking
    /// it would suppress the previous one. `AIConversation::truncate_from_exchange` drops such
    /// compactions outright; this is the belt to that brace.
    pub fn summary_insertions(
        &self,
        message_exists: impl Fn(&str) -> bool,
    ) -> Vec<(String, String)> {
        self.completed
            .iter()
            .rev()
            .filter(|c| c.is_shown(&message_exists))
            .find_map(|c| Some((c.insertion_target().to_owned(), c.summary()?.to_owned())))
            .into_iter()
            .collect()
    }

    /// Drops every completed compaction whose summary message or insertion target no longer
    /// exists (a rewind removed it), together with its markers. Its head then becomes
    /// visible again: the summary that stood in for it is gone. Returns how many were dropped.
    pub fn retain_compactions_whose_messages_exist(
        &mut self,
        message_exists: impl Fn(&str) -> bool,
    ) -> usize {
        let (kept, dropped): (Vec<_>, Vec<_>) = std::mem::take(&mut self.completed)
            .into_iter()
            .partition(|c| {
                message_exists(&c.assistant_msg_id) && message_exists(c.insertion_target())
            });
        self.completed = kept;
        for c in &dropped {
            let still_anchored = self
                .completed
                .iter()
                .any(|k| k.user_msg_id == c.user_msg_id);
            if !still_anchored {
                if let Some(marker) = self.markers.get_mut(&c.user_msg_id) {
                    marker.compaction_trigger = None;
                }
            }
            if let Some(marker) = self.markers.get_mut(&c.assistant_msg_id) {
                marker.is_summary = false;
            }
        }
        dropped.len()
    }

    /// Debug / test entry point: checks whether a marker exists.
    #[cfg(test)]
    pub(crate) fn marker_count(&self) -> usize {
        self.markers.len()
    }
}

#[cfg(test)]
mod state_tests {
    use super::*;

    fn cc(uid: &str, aid: &str, auto: bool) -> CompletedCompaction {
        CompletedCompaction {
            user_msg_id: uid.to_string(),
            assistant_msg_id: aid.to_string(),
            head_message_ids: Vec::new(),
            tail_start_id: None,
            summary_text: Some(format!("summary-{aid}")),
            auto,
            overflow: false,
        }
    }

    #[test]
    fn push_completed_marks_both_messages() {
        let mut s = CompactionState::default();
        s.push_completed(cc("u1", "a1", true));
        assert!(s.marker("u1").unwrap().compaction_trigger == Some(CompactionTrigger::Auto));
        assert!(s.marker("a1").unwrap().is_summary);
    }

    #[test]
    fn previous_summary_returns_last() {
        let mut s = CompactionState::default();
        s.push_completed(cc("u1", "a1", false));
        s.push_completed(cc("u2", "a2", false));
        assert_eq!(s.previous_summary(), Some("summary-a2"));
    }

    #[test]
    fn hidden_message_ids_covers_all_completed() {
        let mut s = CompactionState::default();
        s.push_completed(cc("u1", "a1", false));
        s.push_completed(cc("u2", "a2", false));
        let h = s.hidden_message_ids();
        assert!(h.contains("u1"));
        assert!(h.contains("a1"));
        assert!(h.contains("u2"));
        assert!(h.contains("a2"));
        assert_eq!(h.len(), 4);
    }

    #[test]
    fn hidden_message_ids_includes_head_message_ids() {
        let mut s = CompactionState::default();
        let mut c = cc("u1", "a1", false);
        c.head_message_ids = vec!["h1".to_string(), "h2".to_string(), "u1".to_string()];
        s.push_completed(c);
        let h = s.hidden_message_ids();
        assert!(h.contains("h1"));
        assert!(h.contains("h2"));
        assert!(h.contains("u1"));
        assert!(h.contains("a1"));
        assert_eq!(h.len(), 4);
    }

    #[test]
    fn v1_completed_compaction_deserializes_to_empty_head_message_ids() {
        let json = r#"{
            "user_msg_id":"u1",
            "assistant_msg_id":"a1",
            "tail_start_id":null,
            "summary_text":"summary",
            "auto":false,
            "overflow":false
        }"#;
        let c: CompletedCompaction = serde_json::from_str(json).unwrap();
        assert!(c.head_message_ids.is_empty());
    }

    #[test]
    fn upsert_marker_merges() {
        let mut s = CompactionState::default();
        s.upsert_marker("m1", |m| m.is_summary = true);
        s.upsert_marker("m1", |m| m.synthetic_continue = true);
        let m = s.marker("m1").unwrap();
        assert!(m.is_summary);
        assert!(m.synthetic_continue);
        assert_eq!(s.marker_count(), 1);
    }

    #[test]
    fn default_serializable_roundtrip() {
        let s = CompactionState::default();
        let j = serde_json::to_string(&s).unwrap();
        let back: CompactionState = serde_json::from_str(&j).unwrap();
        assert_eq!(back.version, CompactionState::VERSION);
        assert!(back.completed.is_empty());
    }

    #[test]
    fn marker_only_state_is_not_empty_and_roundtrips() {
        let mut s = CompactionState::default();
        assert!(s.is_empty());
        s.mark_tool_compacted("t1", 42);
        assert!(s.completed().is_empty());
        assert!(!s.is_empty());
        let j = serde_json::to_string(&s).unwrap();
        let back: CompactionState = serde_json::from_str(&j).unwrap();
        assert_eq!(
            back.marker("t1").unwrap().tool_output_compacted_at,
            Some(42)
        );
    }

    #[test]
    fn state_json_without_markers_still_loads() {
        let back: CompactionState =
            serde_json::from_str(r#"{"version":2,"completed":[]}"#).unwrap();
        assert!(back.is_empty());
    }
}
