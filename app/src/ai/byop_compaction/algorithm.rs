//! Core compaction algorithm — ported 1:1 from opencode `compaction.ts:141-341`
//! (turns / select / splitTurn / prune).
//!
//! Decoupled from warp's concrete message types: exposed externally via the
//! [`MessageRef`] trait abstraction; see `super::message_view` for the real
//! implementation.
use std::hash::Hash;

use super::consts::{PRUNE_MINIMUM, PRUNE_PROTECT, PRUNE_PROTECTED_TOOLS};
use super::overflow::{usable, ModelLimit};
use super::CompactionConfig;

/// A message's role — used for turn detection and select.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
    Tool,
}

/// Metadata for a single tool output (needed for prune decisions).
#[derive(Debug, Clone)]
pub struct ToolOutputRef<CallId> {
    pub call_id: CallId,
    pub tool_name: String,
    /// Estimated token count (matches opencode `Token.estimate(part.state.output)`).
    pub output_size: usize,
    pub completed: bool,
    /// Already marked `compacted` by prune; break out when encountered while continuing to iterate.
    pub already_compacted: bool,
}

/// An abstract message reference — the algorithm only interacts with this trait, decoupled from warp's concrete types.
pub trait MessageRef {
    type Id: Clone + Eq + Hash;
    type CallId: Clone + Eq + Hash;

    fn id(&self) -> Self::Id;
    fn role(&self) -> Role;

    /// Whether the user message carries a compaction-trigger marker (opencode `parts.some(p => p.type === "compaction")`).
    fn is_compaction_marker(&self) -> bool;

    /// Whether the assistant message is itself a summary (opencode `info.summary === true`).
    fn is_summary(&self) -> bool;

    /// Estimated token count for a single message — implementations can use `serde_json` + `super::token::estimate`.
    fn estimate_size(&self) -> usize;

    /// All tool outputs within this message (used for prune); only assistant messages have these.
    fn tool_outputs(&self) -> Vec<ToolOutputRef<Self::CallId>>;

    /// The tool calls this message opens. Used by `split_turn` so a cut never separates a
    /// call from its result (#1061). Default: none.
    fn tool_call_ids(&self) -> Vec<Self::CallId> {
        Vec::new()
    }

    /// The tool calls this message answers. Default: the call ids of [`Self::tool_outputs`].
    fn tool_result_call_ids(&self) -> Vec<Self::CallId> {
        self.tool_outputs()
            .into_iter()
            .map(|output| output.call_id)
            .collect()
    }
}

/// For every index `i` of `messages`, whether a cut at `i` (head `..i`, tail `i..`) keeps each
/// tool call together with its result: no call before `i` has its result at or after `i`
/// (#1061).
///
/// In this fork a `ToolCall` and its `ToolCallResult` are separate messages, unlike opencode,
/// where they are parts of one assistant message and a message-level cut cannot split them.
/// A cut between them leaves the summarize request a call without its result, and after the
/// commit hides the head, every later request an orphan result.
///
/// A result is paired with the nearest preceding still-unanswered call with the same id, not
/// with "the" call of that id: providers reuse ids across steps (Kimi's `functions.<name>:0`),
/// and pairing a late result with the first call of the conversation would make every cut in
/// between unsafe, so every compaction would summarize everything. A call that never got a
/// result, and a result with no unanswered call before it, constrain nothing. (Safe on the
/// deferred tool-loop path: the BYOP preflight records every result of the step before the
/// summary request is built, so a call left unanswered there has no result that could later
/// land in the tail as an orphan.)
pub fn tool_pair_safe_cuts<M: MessageRef>(messages: &[M]) -> Vec<bool> {
    use std::collections::HashMap;

    let n = messages.len();
    let mut unanswered: HashMap<M::CallId, Vec<usize>> = HashMap::new();
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for (i, msg) in messages.iter().enumerate() {
        // Results first: a message never answers a call it opens itself.
        for id in msg.tool_result_call_ids() {
            if let Some(call) = unanswered.get_mut(&id).and_then(Vec::pop) {
                pairs.push((call, i));
            }
        }
        for id in msg.tool_call_ids() {
            unanswered.entry(id).or_default().push(i);
        }
    }
    // A pair (call at c, result at r > c) makes every cut in c+1..=r unsafe. Difference
    // array over cut positions 0..=n.
    let mut delta = vec![0i64; n + 2];
    for (call, result) in pairs {
        delta[call + 1] += 1;
        delta[result + 1] -= 1;
    }
    let mut open = 0i64;
    (0..n)
        .map(|i| {
            open += delta[i];
            open == 0
        })
        .collect()
}

/// Corresponds to the `compaction.ts:76-80` type.
#[derive(Debug, Clone)]
pub struct Turn<Id> {
    pub start: usize,
    pub end: usize,
    pub id: Id,
}

/// `compaction.ts:82-85`.
#[derive(Debug, Clone)]
pub struct Tail<Id> {
    pub start: usize,
    pub id: Id,
}

/// `select`'s return value: `head` is the range to send to the summarization LLM, `tail_start_id` is where the kept segment starts.
#[derive(Debug, Clone)]
pub struct SelectResult<Id> {
    pub head_end: usize,
    pub tail_start_id: Option<Id>,
}

/// `compaction.ts:141-157`。
pub fn turns<M: MessageRef>(messages: &[M]) -> Vec<Turn<M::Id>> {
    let mut result: Vec<Turn<M::Id>> = Vec::new();
    let n = messages.len();
    for (i, msg) in messages.iter().enumerate() {
        if msg.role() != Role::User {
            continue;
        }
        if msg.is_compaction_marker() {
            continue;
        }
        result.push(Turn {
            start: i,
            end: n,
            id: msg.id(),
        });
    }
    let len = result.len();
    if len > 1 {
        for i in 0..len - 1 {
            result[i].end = result[i + 1].start;
        }
    }
    result
}

/// `compaction.ts:159-182` splitTurn — finds the first cut point within a turn that fits into the budget.
fn split_turn<M, EstFn>(
    messages: &[M],
    turn: &Turn<M::Id>,
    budget: usize,
    estimate: &EstFn,
    safe_cuts: &[bool],
) -> Option<Tail<M::Id>>
where
    M: MessageRef,
    EstFn: Fn(&[M]) -> usize,
{
    if budget == 0 {
        return None;
    }
    if turn.end.saturating_sub(turn.start) <= 1 {
        return None;
    }
    // Divergence from opencode (#1061): skip cut points that would separate a tool call from
    // its result; see [`tool_pair_safe_cuts`].
    let mut start = turn.start + 1;
    while start < turn.end {
        if !safe_cuts[start] {
            start += 1;
            continue;
        }
        let size = estimate(&messages[start..turn.end]);
        if size > budget {
            start += 1;
            continue;
        }
        return Some(Tail {
            start,
            id: messages[start].id(),
        });
    }
    None
}

/// `compaction.ts:244-293` select — cuts out the head/tail.
///
/// `estimate_slice` corresponds to opencode's `estimate({ messages: slice, model })`.
/// Passed in by the caller because it decides how to serialize the message list
/// (JSON) before using `Token.estimate`.
pub fn select<M, EstFn>(
    messages: &[M],
    cfg: &CompactionConfig,
    model: ModelLimit,
    estimate_slice: EstFn,
) -> SelectResult<M::Id>
where
    M: MessageRef,
    EstFn: Fn(&[M]) -> usize,
{
    let limit = cfg.tail_turns;
    if limit == 0 {
        return SelectResult {
            head_end: messages.len(),
            tail_start_id: None,
        };
    }
    let usable_tokens = usable(cfg, model);
    let budget = cfg.preserve_recent_budget(usable_tokens);
    let all = turns(messages);
    if all.is_empty() {
        return SelectResult {
            head_end: messages.len(),
            tail_start_id: None,
        };
    }
    let recent_start = all.len().saturating_sub(limit);
    let recent: Vec<&Turn<M::Id>> = all[recent_start..].iter().collect();
    let sizes: Vec<usize> = recent
        .iter()
        .map(|t| estimate_slice(&messages[t.start..t.end]))
        .collect();

    // #1061: a cut must never separate a tool call from its result, at a turn boundary as
    // well as inside a turn.
    let safe_cuts = tool_pair_safe_cuts(messages);
    let mut total: usize = 0;
    let mut keep: Option<Tail<M::Id>> = None;
    for i in (0..recent.len()).rev() {
        let turn = recent[i];
        let size = sizes[i];
        if total + size <= budget {
            total += size;
            // A boundary that would split a pair is not a cut point; the turn still counts
            // toward the kept tail, which may then start at an older (safe) boundary.
            if safe_cuts[turn.start] {
                keep = Some(Tail {
                    start: turn.start,
                    id: turn.id.clone(),
                });
            }
            continue;
        }
        let remaining = budget.saturating_sub(total);
        let split = split_turn(messages, turn, remaining, &estimate_slice, &safe_cuts);
        if split.is_some() {
            keep = split;
        }
        // Note opencode's implementation: break as soon as size first exceeds the
        // budget, regardless of whether splitTurn found something — no earlier turn
        // is tried after that.
        break;
    }

    match keep {
        None => SelectResult {
            head_end: messages.len(),
            tail_start_id: None,
        },
        Some(t) if t.start == 0 => SelectResult {
            head_end: messages.len(),
            tail_start_id: None,
        },
        Some(t) => SelectResult {
            head_end: t.start,
            tail_start_id: Some(t.id),
        },
    }
}

/// `compaction.ts:297-341` prune decision — returns the (message_id, tool_call_id)
/// pairs that should be marked `compacted`.
///
/// The caller writes these into `CompactionState.markers` (the actual protobuf
/// message is left untouched).
pub fn prune_decisions<M: MessageRef>(messages: &[M]) -> Vec<(M::Id, M::CallId)> {
    let mut total: usize = 0;
    let mut pruned: usize = 0;
    let mut to_prune: Vec<(M::Id, M::CallId)> = Vec::new();
    let mut user_turns_seen: usize = 0;

    'outer: for msg in messages.iter().rev() {
        if msg.role() == Role::User {
            user_turns_seen += 1;
        }
        // Keeps at least the most recent 2 user turns untouched (opencode `if (turns < 2) continue`).
        if user_turns_seen < 2 {
            continue;
        }
        // Already a summary boundary — stop looking further back.
        if msg.role() == Role::Assistant && msg.is_summary() {
            break 'outer;
        }
        let outputs = msg.tool_outputs();
        for tp in outputs.into_iter().rev() {
            if !tp.completed {
                continue;
            }
            if PRUNE_PROTECTED_TOOLS.contains(&tp.tool_name.as_str()) {
                continue;
            }
            if tp.already_compacted {
                break 'outer;
            }
            let estimate = tp.output_size;
            total += estimate;
            if total <= PRUNE_PROTECT {
                continue;
            }
            pruned += estimate;
            to_prune.push((msg.id(), tp.call_id));
        }
    }

    if pruned > PRUNE_MINIMUM {
        to_prune
    } else {
        Vec::new()
    }
}
