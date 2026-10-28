//! Phase 1 unit tests — covering pure functions (token / overflow / prompt / config / algorithm).
//!
//! e2e integration tests will be added once Phase 3 (state + message_view) lands.

use super::algorithm::{prune_decisions, select, turns, MessageRef, Role, ToolOutputRef};
use super::commit::commit_summarization;
use super::config::{model_limit_from_parts, CompactionConfig};
use super::consts::*;
use super::overflow::{is_overflow, usable, ModelLimit, TokenCounts};
use super::prompt::{build_continue_message, build_prompt, SUMMARY_TEMPLATE};
use super::token::estimate;
use crate::ai::agent::conversation::{AIConversation, AIConversationId};
use warp_multi_agent_api as api;

// -- token ---------------------------------------------------------------

#[test]
fn token_estimate_empty() {
    assert_eq!(estimate(""), 0);
}

#[test]
fn token_estimate_short() {
    // "hello world" = 11 chars → round(11/4) = 3
    assert_eq!(estimate("hello world"), 3);
}

#[test]
fn token_estimate_aligned() {
    assert_eq!(estimate(&"a".repeat(40)), 10);
    assert_eq!(estimate(&"a".repeat(41)), 10); // 41/4 = 10.25 → 10 (banker's rounding doesn't matter here)
    assert_eq!(estimate(&"a".repeat(42)), 11); // 42/4 = 10.5 → 11
}

// -- overflow ------------------------------------------------------------

fn cfg_default() -> CompactionConfig {
    CompactionConfig::default()
}

#[test]
fn usable_with_input_limit() {
    let cfg = cfg_default();
    let model = ModelLimit {
        context: 200_000,
        input: 180_000,
        max_output: 8_000,
    };
    // reserved = min(20_000, 8_000) = 8_000
    // usable = max(0, 180_000 - 8_000) = 172_000
    assert_eq!(usable(&cfg, model), 172_000);
}

/// The regression this whole helper exists for: a 1M-window model must not be compacted at
/// the hardcoded 172_000. Numbers are the ones observed in zap.log on 2026-08-14
/// (`window=1000000`, `usable=172000`).
#[test]
fn model_limit_uses_configured_window_not_fallback() {
    let cfg = cfg_default();
    let model = model_limit_from_parts(1_000_000, 64_000);
    assert_eq!(model.context, 1_000_000);
    assert_eq!(usable(&cfg, model), 936_000); // 1_000_000 - 64_000
    assert_ne!(
        usable(&cfg, model),
        172_000,
        "regressed to ModelLimit::FALLBACK"
    );
}

/// `max_output_tokens: 0` means "unspecified", not "no output" — leaving it at 0 would make
/// `usable == context`, i.e. only compact once the prompt already fills the window.
#[test]
fn model_limit_unspecified_max_output_keeps_headroom() {
    let cfg = cfg_default();
    let model = model_limit_from_parts(1_000_000, 0);
    assert_eq!(model.max_output, COMPACTION_BUFFER);
    assert_eq!(usable(&cfg, model), 980_000);
}

/// An unknown window must fall back, not disable compaction: `usable`/`is_overflow`
/// short-circuit on `context == 0`, so a passthrough would mean auto compaction never fires.
#[test]
fn model_limit_unknown_window_falls_back() {
    let cfg = cfg_default();
    let model = model_limit_from_parts(0, 8_000);
    assert_eq!(model.context, ModelLimit::FALLBACK.context);
    assert_eq!(usable(&cfg, model), 172_000);
}

#[test]
fn usable_without_input_limit() {
    let cfg = cfg_default();
    let model = ModelLimit {
        context: 200_000,
        input: 0,
        max_output: 8_000,
    };
    // Takes the second branch: context - max_output = 192_000
    assert_eq!(usable(&cfg, model), 192_000);
}

#[test]
fn usable_zero_context() {
    let cfg = cfg_default();
    let model = ModelLimit {
        context: 0,
        input: 0,
        max_output: 0,
    };
    assert_eq!(usable(&cfg, model), 0);
}

#[test]
fn usable_respects_cfg_reserved_override() {
    let mut cfg = cfg_default();
    cfg.reserved = Some(50_000);
    let model = ModelLimit {
        context: 200_000,
        input: 180_000,
        max_output: 8_000,
    };
    // reserved overridden to 50_000 → 180_000 - 50_000 = 130_000
    assert_eq!(usable(&cfg, model), 130_000);
}

#[test]
fn is_overflow_auto_off() {
    let mut cfg = cfg_default();
    cfg.auto = false;
    let model = ModelLimit {
        context: 200_000,
        input: 180_000,
        max_output: 8_000,
    };
    let tokens = TokenCounts {
        total: 999_999,
        ..Default::default()
    };
    assert!(!is_overflow(&cfg, tokens, model));
}

#[test]
fn is_overflow_at_threshold() {
    let cfg = cfg_default();
    let model = ModelLimit {
        context: 200_000,
        input: 180_000,
        max_output: 8_000,
    };
    let usable_n = usable(&cfg, model);
    let tokens = TokenCounts {
        total: usable_n,
        ..Default::default()
    };
    assert!(is_overflow(&cfg, tokens, model));
    let tokens_below = TokenCounts {
        total: usable_n - 1,
        ..Default::default()
    };
    assert!(!is_overflow(&cfg, tokens_below, model));
}

#[test]
fn token_counts_count_uses_total_when_present() {
    let t = TokenCounts {
        total: 100,
        input: 50,
        output: 60,
        cache_read: 10,
        cache_write: 5,
    };
    assert_eq!(t.count(), 100); // total takes priority
}

#[test]
fn token_counts_count_sums_when_total_zero() {
    let t = TokenCounts {
        total: 0,
        input: 50,
        output: 60,
        cache_read: 10,
        cache_write: 5,
    };
    assert_eq!(t.count(), 125);
}

// -- preserve_recent_budget ----------------------------------------------

#[test]
fn preserve_recent_budget_default_formula() {
    let cfg = cfg_default();
    // usable=80_000 → 80_000/4 = 20_000 → max(2_000, 20_000)=20_000 → min(8_000, 20_000) = 8_000
    assert_eq!(
        cfg.preserve_recent_budget(80_000),
        MAX_PRESERVE_RECENT_TOKENS
    );
    // usable=4_000 → 1_000 → max(2_000, 1_000)=2_000 → min(8_000, 2_000)=2_000
    assert_eq!(
        cfg.preserve_recent_budget(4_000),
        MIN_PRESERVE_RECENT_TOKENS
    );
    // usable=20_000 → 5_000 → max(2_000, 5_000)=5_000 → min(8_000, 5_000)=5_000
    assert_eq!(cfg.preserve_recent_budget(20_000), 5_000);
}

#[test]
fn preserve_recent_budget_override() {
    let mut cfg = cfg_default();
    cfg.preserve_recent_tokens = Some(12_345);
    assert_eq!(cfg.preserve_recent_budget(80_000), 12_345);
}

// -- prompt --------------------------------------------------------------

#[test]
fn summary_template_contains_all_sections() {
    let must = [
        "## Goal",
        "## Constraints & Preferences",
        "## Progress",
        "### Done",
        "### In Progress",
        "### Blocked",
        "## Key Decisions",
        "## Next Steps",
        "## Critical Context",
        "## Relevant Files",
        "Rules:",
        "<template>",
        "</template>",
    ];
    for m in must {
        assert!(SUMMARY_TEMPLATE.contains(m), "missing section: {m}");
    }
}

#[test]
fn build_prompt_no_previous() {
    let s = build_prompt(None, &[]);
    assert!(s.starts_with("Create a new anchored summary from the conversation history above."));
    assert!(s.contains(SUMMARY_TEMPLATE));
}

#[test]
fn build_prompt_with_previous() {
    let s = build_prompt(Some("OLD-SUMMARY"), &[]);
    assert!(s.starts_with("Update the anchored summary below"));
    assert!(s.contains("<previous-summary>\nOLD-SUMMARY\n</previous-summary>"));
    assert!(s.contains(SUMMARY_TEMPLATE));
}

#[test]
fn build_prompt_with_plugin_context() {
    let ctx = vec!["EXTRA1".to_string(), "EXTRA2".to_string()];
    let s = build_prompt(None, &ctx);
    assert!(s.contains("EXTRA1"));
    assert!(s.contains("EXTRA2"));
}

#[test]
fn continue_message_overflow_branch() {
    let s = build_continue_message(true);
    assert!(s.contains("previous request exceeded"));
    assert!(s.contains("Continue if you have next steps"));
}

#[test]
fn continue_message_normal_branch() {
    let s = build_continue_message(false);
    assert!(!s.contains("previous request exceeded"));
    assert!(s.starts_with("Continue if you have next steps"));
}

// -- algorithm: turns / select / prune ----------------------------------

/// Mock message implementation for tests.
#[derive(Debug, Clone)]
struct M {
    id: u32,
    role: Role,
    /// Whether the user message carries a compaction marker
    is_compaction: bool,
    /// Whether the assistant message is a summary
    is_summary: bool,
    size: usize,
    tools: Vec<ToolOutputRef<u32>>,
    /// Tool calls this message opens (a `ToolCall` message).
    calls: Vec<u32>,
}

impl M {
    fn user(id: u32, size: usize) -> Self {
        Self {
            id,
            role: Role::User,
            is_compaction: false,
            is_summary: false,
            size,
            tools: vec![],
            calls: vec![],
        }
    }
    fn user_compaction(id: u32) -> Self {
        Self {
            id,
            role: Role::User,
            is_compaction: true,
            is_summary: false,
            size: 0,
            tools: vec![],
            calls: vec![],
        }
    }
    fn assistant(id: u32, size: usize) -> Self {
        Self {
            id,
            role: Role::Assistant,
            is_compaction: false,
            is_summary: false,
            size,
            tools: vec![],
            calls: vec![],
        }
    }
    fn summary(id: u32) -> Self {
        Self {
            id,
            role: Role::Assistant,
            is_compaction: false,
            is_summary: true,
            size: 100,
            tools: vec![],
            calls: vec![],
        }
    }
    fn assistant_with_tools(id: u32, size: usize, tools: Vec<ToolOutputRef<u32>>) -> Self {
        Self {
            id,
            role: Role::Assistant,
            is_compaction: false,
            is_summary: false,
            size,
            tools,
            calls: vec![],
        }
    }
    /// An assistant `ToolCall` message opening `call_id`.
    fn tool_call(id: u32, size: usize, call_id: u32) -> Self {
        Self {
            id,
            role: Role::Assistant,
            is_compaction: false,
            is_summary: false,
            size,
            tools: vec![],
            calls: vec![call_id],
        }
    }
    /// A `ToolCallResult` message answering `call_id`.
    fn tool_result(id: u32, size: usize, call_id: u32) -> Self {
        Self {
            id,
            role: Role::Tool,
            is_compaction: false,
            is_summary: false,
            size,
            tools: vec![ToolOutputRef {
                call_id,
                tool_name: "run_shell_command".to_string(),
                output_size: size,
                completed: true,
                already_compacted: false,
            }],
            calls: vec![],
        }
    }
}

impl MessageRef for M {
    type Id = u32;
    type CallId = u32;
    fn id(&self) -> u32 {
        self.id
    }
    fn role(&self) -> Role {
        self.role
    }
    fn is_compaction_marker(&self) -> bool {
        self.is_compaction
    }
    fn is_summary(&self) -> bool {
        self.is_summary
    }
    fn estimate_size(&self) -> usize {
        self.size
    }
    fn tool_outputs(&self) -> Vec<ToolOutputRef<u32>> {
        self.tools.clone()
    }
    fn tool_call_ids(&self) -> Vec<u32> {
        self.calls.clone()
    }
}

fn sum_size(slice: &[M]) -> usize {
    slice.iter().map(|m| m.size).sum()
}

#[test]
fn turns_basic() {
    let msgs = vec![
        M::user(1, 10),
        M::assistant(2, 20),
        M::user(3, 10),
        M::assistant(4, 30),
        M::user(5, 10),
    ];
    let t = turns(&msgs);
    assert_eq!(t.len(), 3);
    assert_eq!(t[0].start, 0);
    assert_eq!(t[0].end, 2);
    assert_eq!(t[1].start, 2);
    assert_eq!(t[1].end, 4);
    assert_eq!(t[2].start, 4);
    assert_eq!(t[2].end, 5);
}

#[test]
fn turns_skips_compaction_marker() {
    let msgs = vec![
        M::user(1, 10),
        M::assistant(2, 20),
        M::user_compaction(99), // Doesn't count as a turn
        M::assistant(3, 30),
        M::user(4, 10),
    ];
    let t = turns(&msgs);
    assert_eq!(t.len(), 2);
    assert_eq!(t[0].id, 1);
    assert_eq!(t[1].id, 4);
}

#[test]
fn turns_empty() {
    let msgs: Vec<M> = vec![];
    assert_eq!(turns(&msgs).len(), 0);
}

#[test]
fn select_keeps_recent_turns_within_budget() {
    let msgs = vec![
        M::user(1, 100),
        M::assistant(2, 100), // turn1 size 200
        M::user(3, 100),
        M::assistant(4, 100), // turn2 size 200
        M::user(5, 100),
        M::assistant(6, 100), // turn3 size 200
    ];
    let cfg = CompactionConfig {
        tail_turns: 2,
        preserve_recent_tokens: Some(500), // Enough to fit the last 2 turns (200 each)
        ..Default::default()
    };
    let model = ModelLimit::FALLBACK;
    let r = select(&msgs, &cfg, model, sum_size);
    // The tail starts at the 2nd turn's user (idx=2), head_end=2
    assert_eq!(r.head_end, 2);
    assert_eq!(r.tail_start_id, Some(3));
}

#[test]
fn select_split_turn_when_over_budget() {
    let msgs = vec![
        M::user(1, 100),
        M::user(2, 100), // turn 2 contains 5 messages totaling 500
        M::assistant(3, 100),
        M::assistant(4, 100),
        M::assistant(5, 100),
        M::assistant(6, 100),
    ];
    let cfg = CompactionConfig {
        tail_turns: 1,
        preserve_recent_tokens: Some(250), // Can't fit turn2 as a whole (500), triggering splitTurn
        ..Default::default()
    };
    let model = ModelLimit::FALLBACK;
    let r = select(&msgs, &cfg, model, sum_size);
    // splitTurn searches starting at turn2.start+1=2: messages[2..6]=400 > 250, [3..6]=300>250, [4..6]=200<=250 → start=4
    assert_eq!(r.head_end, 4);
    assert_eq!(r.tail_start_id, Some(5));
}

#[test]
fn select_returns_full_when_no_turns() {
    let msgs: Vec<M> = vec![];
    let cfg = CompactionConfig::default();
    let r = select(&msgs, &cfg, ModelLimit::FALLBACK, sum_size);
    assert_eq!(r.head_end, 0);
    assert_eq!(r.tail_start_id, None);
}

#[test]
fn select_tail_turns_zero_keeps_full() {
    let msgs = vec![M::user(1, 100), M::assistant(2, 100)];
    let cfg = CompactionConfig {
        tail_turns: 0,
        ..Default::default()
    };
    let r = select(&msgs, &cfg, ModelLimit::FALLBACK, sum_size);
    assert_eq!(r.head_end, msgs.len());
    assert_eq!(r.tail_start_id, None);
}

// -- select never separates a tool call from its result (#1061) ------------
//
// A `ToolCall` and its `ToolCallResult` are separate messages here (one assistant message in
// opencode). A cut between them gives the summarize request a call without its result and,
// once the head is hidden, every later request an orphan result.

#[test]
fn tool_pair_safe_cuts_marks_cuts_inside_a_pair_unsafe() {
    use super::algorithm::tool_pair_safe_cuts;
    let msgs = vec![
        M::user(1, 10),            // 0
        M::tool_call(2, 10, 20),   // 1
        M::tool_result(3, 10, 20), // 2: between call 20 and its result
        M::assistant(4, 10),       // 3
        M::tool_call(5, 10, 21),   // 4: parallel calls 21 and 22
        M::tool_call(6, 10, 22),   // 5: 21 is open
        M::tool_result(7, 10, 21), // 6: 22 is open
        M::tool_result(8, 10, 22), // 7: 22 is open
        M::tool_call(9, 10, 23),   // 8: a call that never got a result constrains nothing
        M::assistant(10, 10),      // 9
    ];
    assert_eq!(
        tool_pair_safe_cuts(&msgs),
        vec![
            true, true, false, true, true, false, false, false, true, true
        ]
    );
}

/// The oversized turn is a tool loop. The first suffix that fits the budget starts at the
/// first call's *result*; the cut must move on to the next call instead.
#[test]
fn select_split_turn_never_cuts_between_a_call_and_its_result() {
    let msgs = vec![
        M::user(1, 100),
        M::tool_call(2, 100, 20),
        M::tool_result(3, 100, 20),
        M::tool_call(4, 10, 21),
        M::tool_result(5, 100, 21),
    ];
    let cfg = CompactionConfig {
        tail_turns: 1,
        preserve_recent_tokens: Some(250),
        ..Default::default()
    };
    let r = select(&msgs, &cfg, ModelLimit::FALLBACK, sum_size);
    // [1..5)=310 > 250; [2..5)=210 fits but starts at the result of call 20 (the old cut);
    // [3..5)=110 fits and starts at a call.
    assert_eq!(
        r.head_end, 3,
        "the cut must not leave result 3 without call 2"
    );
    assert_eq!(r.tail_start_id, Some(4));
}

/// Parallel calls: every cut inside the call/result group is unsafe, so when no safe cut fits
/// the budget the whole turn goes to the summarizer rather than a split pair.
#[test]
fn select_split_turn_keeps_parallel_calls_together() {
    let msgs = vec![
        M::user(1, 100),
        M::tool_call(2, 50, 20),
        M::tool_call(3, 50, 21),
        M::tool_result(4, 100, 20),
        M::tool_result(5, 100, 21),
    ];
    let cfg = CompactionConfig {
        tail_turns: 1,
        preserve_recent_tokens: Some(260),
        ..Default::default()
    };
    let r = select(&msgs, &cfg, ModelLimit::FALLBACK, sum_size);
    // The old cut was at index 2 ([2..5)=250 fits): call 20 in the head, its result in the
    // tail.
    assert_eq!(r.head_end, msgs.len());
    assert_eq!(r.tail_start_id, None);
}

// -- prune ---------------------------------------------------------------

fn tool_output(call_id: u32, name: &str, size: usize) -> ToolOutputRef<u32> {
    ToolOutputRef {
        call_id,
        tool_name: name.to_string(),
        output_size: size,
        completed: true,
        already_compacted: false,
    }
}

#[test]
fn prune_below_minimum_returns_empty() {
    // Only a small amount of tool output, under PRUNE_MINIMUM (20_000)
    let msgs = vec![
        M::user(1, 10),
        M::assistant_with_tools(2, 0, vec![tool_output(101, "bash", 5_000)]),
        M::user(3, 10),
        M::assistant_with_tools(4, 0, vec![tool_output(102, "bash", 5_000)]),
        M::user(5, 10),
    ];
    let r = prune_decisions(&msgs);
    assert_eq!(r.len(), 0);
}

#[test]
fn prune_skips_protected_skill_tool() {
    // A large skill tool + a large bash tool; skill is protected and never gets pruned, and bash within PRUNE_PROTECT isn't trimmed either
    let msgs = vec![
        M::user(1, 10),
        M::assistant_with_tools(
            2,
            0,
            vec![
                tool_output(101, "skill", 50_000), // skip
                tool_output(102, "bash", 30_000),
            ],
        ),
        M::user(3, 10),
        M::assistant_with_tools(4, 0, vec![tool_output(103, "bash", 30_000)]),
        M::user(5, 10),
    ];
    let r = prune_decisions(&msgs);
    // Only trims the part beyond PRUNE_PROTECT (40_000), and only when the total
    // trimmed amount exceeds PRUNE_MINIMUM (20_000).
    // The most recent 2 user turns stay untouched: turn5..end / turn3..turn5 are
    // both kept.
    // Here the 1st turn contains bash 30_000 (over PROTECT), so it should be
    // trimmed.
    // Cumulative total: 30_000 (bash 102) + 30_000 (bash 103, but skipped since
    // protected within turn3)...
    // Note: user_turns_seen becomes 1 upon encountering user(5), then 2 at user(3),
    // and continues looking at earlier messages.
    // assistant(4)'s tools are within turn3 — does it still `continue` when
    // user_turns_seen=2?
    //   Loop: idx=4 user(5), seen=1 → continue
    //         idx=3 assistant(4), seen=1 → continue
    //         idx=2 user(3), seen=2 → enters processing
    //         idx=1 assistant(2), seen=2 → processes tools (skill skip; bash
    //           30_000 → total=30_000 > PROTECT(40_000)? no, 30<40 → continue)
    //         idx=0 user(1), seen=3 → processes (no tools)
    // total=30_000, pruned=0, below PRUNE_MINIMUM → returns empty
    assert_eq!(r.len(), 0);
}

#[test]
fn prune_reaches_minimum_returns_decisions() {
    // Constructs a large enough tool output to trigger prune
    let big_tool = |id: u32| tool_output(id, "bash", 25_000);
    let msgs = vec![
        M::user(1, 10),
        M::assistant_with_tools(2, 0, vec![big_tool(101), big_tool(102), big_tool(103)]),
        M::user(3, 10),
        M::assistant(4, 0),
        M::user(5, 10),
    ];
    let r = prune_decisions(&msgs);
    // Iterating in reverse:
    //  idx=4 user(5) seen=1 continue
    //  idx=3 assistant(4) seen=1 continue
    //  idx=2 user(3) seen=2 continue (no tools)
    //  idx=1 assistant(2) seen=2, tools in reverse: 103 → total=25_000 < 40_000 continue;
    //                                                102 → total=50_000 > 40_000 → pruned=25_000, push (2,102)
    //                                                101 → total=75_000 > 40_000 → pruned=50_000, push (2,101)
    //  idx=0 user(1) seen=3 continue
    // pruned=50_000 > PRUNE_MINIMUM(20_000) → returns [(2,102),(2,101)]
    assert_eq!(r.len(), 2);
    assert!(r.contains(&(2, 102)));
    assert!(r.contains(&(2, 101)));
}

#[test]
fn prune_stops_at_summary_boundary() {
    let big_tool = |id: u32| tool_output(id, "bash", 50_000);
    let msgs = vec![
        M::user(1, 10),
        M::assistant_with_tools(2, 0, vec![big_tool(101)]),
        M::summary(3), // boundary
        M::user(4, 10),
        M::assistant(5, 0),
        M::user(6, 10),
    ];
    let r = prune_decisions(&msgs);
    // When seen=2 hits summary(3), it breaks right away and never processes idx=1's big_tool
    assert_eq!(r.len(), 0);
}

#[test]
fn prune_stops_at_already_compacted() {
    let mut already = tool_output(101, "bash", 50_000);
    already.already_compacted = true;
    let msgs = vec![
        M::user(1, 10),
        M::assistant_with_tools(2, 0, vec![already, tool_output(102, "bash", 50_000)]),
        M::user(3, 10),
        M::assistant(4, 0),
        M::user(5, 10),
    ];
    // tools in reverse: 102 size=50_000 → total=50_000 > 40_000 → pruned=50_000, push (2,102)
    //                   101 already_compacted → break outer
    let r = prune_decisions(&msgs);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0], (2, 102));
}

// -- commit --------------------------------------------------------------

fn ts(seconds: i64) -> prost_types::Timestamp {
    prost_types::Timestamp { seconds, nanos: 0 }
}

fn user_query(id: &str, task_id: &str, request_id: &str, seconds: i64) -> api::Message {
    api::Message {
        id: id.to_string(),
        task_id: task_id.to_string(),
        server_message_data: String::new(),
        citations: vec![],
        fetched_memories: vec![],
        message: Some(api::message::Message::UserQuery(api::message::UserQuery {
            query: format!("query-{id}"),
            context: None,
            mode: None,
            referenced_attachments: Default::default(),
            intended_agent: Default::default(),
        })),
        request_id: request_id.to_string(),
        timestamp: Some(ts(seconds)),
    }
}

fn invoke_skill(id: &str, task_id: &str, request_id: &str, seconds: i64) -> api::Message {
    api::Message {
        id: id.to_string(),
        task_id: task_id.to_string(),
        server_message_data: String::new(),
        citations: vec![],
        fetched_memories: vec![],
        message: Some(api::message::Message::InvokeSkill(
            api::message::InvokeSkill::default(),
        )),
        request_id: request_id.to_string(),
        timestamp: Some(ts(seconds)),
    }
}

fn agent_output(id: &str, task_id: &str, request_id: &str, seconds: i64) -> api::Message {
    api::Message {
        id: id.to_string(),
        task_id: task_id.to_string(),
        server_message_data: String::new(),
        citations: vec![],
        fetched_memories: vec![],
        message: Some(api::message::Message::AgentOutput(
            api::message::AgentOutput {
                text: format!("output-{id}"),
            },
        )),
        request_id: request_id.to_string(),
        timestamp: Some(ts(seconds)),
    }
}

fn conversation_with_messages(messages: Vec<api::Message>) -> AIConversation {
    let task = api::Task {
        id: "root".to_string(),
        messages,
        dependencies: None,
        description: String::new(),
        summary: String::new(),
        server_data: String::new(),
    };
    AIConversation::new_restored(AIConversationId::new(), vec![task], None).unwrap()
}

/// Issue #777: a run started with `/skill` has no `UserQuery` message at all — the
/// backward anchor search must recognize `InvokeSkill` as a user turn too, or it falls
/// back to a synthetic id that matches no real message and the summary is never
/// re-inserted into later requests.
#[test]
fn commit_summarization_anchors_on_skill_invocation() {
    let mut conversation = conversation_with_messages(vec![
        invoke_skill("s1", "root", "r1", 1),
        agent_output("a1", "root", "r1", 2),
    ]);
    let cfg = CompactionConfig {
        tail_turns: 1,
        preserve_recent_tokens: Some(1_000),
        ..Default::default()
    };

    assert!(commit_summarization(&mut conversation, false, &cfg));
    let completed = conversation.compaction_state.completed().last().unwrap();
    assert_eq!(completed.user_msg_id, "s1");
    assert_eq!(completed.assistant_msg_id, "a1");
}

#[test]
fn commit_summarization_records_head_message_ids() {
    let mut conversation = conversation_with_messages(vec![
        user_query("u1", "root", "r1", 1),
        agent_output("a1", "root", "r1", 2),
        user_query("u2", "root", "r2", 3),
        agent_output("a2", "root", "r2", 4),
        user_query("u3", "root", "r3", 5),
        agent_output("a3", "root", "r3", 6),
    ]);
    let cfg = CompactionConfig {
        tail_turns: 1,
        preserve_recent_tokens: Some(1_000),
        ..Default::default()
    };

    assert!(commit_summarization(&mut conversation, false, &cfg));
    let completed = conversation.compaction_state.completed().last().unwrap();
    assert_eq!(completed.user_msg_id, "u3");
    assert_eq!(completed.assistant_msg_id, "a3");
    assert_eq!(completed.tail_start_id.as_deref(), Some("u3"));
    assert_eq!(completed.head_message_ids, ["u1", "a1", "u2", "a2"]);
}

// -- message_view ----------------------------------------------------------

/// Issue #777: a skill invocation is replayed to the model as the user's turn (see
/// `chat_stream::compose_invoke_skill_text`), so `WarpMessageView::role()` must report
/// `Role::User` for it too — otherwise a conversation starting with `/skill` has no
/// user-role message at all, and turn/tail selection (used by `select`/`prune_decisions`)
/// misbehaves.
#[test]
fn warp_message_view_role_treats_invoke_skill_as_user() {
    use super::message_view::{build_tool_name_lookup, project};
    use super::state::CompactionState;

    let msg = invoke_skill("s1", "root", "r1", 1);
    let msgs = vec![&msg];
    let state = CompactionState::default();
    let tool_names = build_tool_name_lookup(msgs.iter().copied());
    let views = project(&msgs, &state, &tool_names);
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].role(), Role::User);
}

fn tool_call(id: &str, task_id: &str, request_id: &str, seconds: i64) -> api::Message {
    api::Message {
        id: id.to_string(),
        task_id: task_id.to_string(),
        server_message_data: String::new(),
        citations: vec![],
        fetched_memories: vec![],
        message: Some(api::message::Message::ToolCall(api::message::ToolCall {
            tool_call_id: format!("call-{id}"),
            tool: None,
        })),
        request_id: request_id.to_string(),
        timestamp: Some(ts(seconds)),
    }
}

/// Auto compaction is deferred while the finished turn asked for tool calls (a summarize
/// request sent then cancels the agent loop); only the turn's own messages count.
#[test]
fn turn_has_tool_calls_only_counts_the_new_turn() {
    use super::overflow::turn_has_tool_calls;

    let msgs = vec![
        user_query("u1", "root", "r1", 1),
        tool_call("t-old", "root", "r1", 2),
        agent_output("a1", "root", "r2", 3),
        tool_call("t-new", "root", "r2", 4),
    ];
    // The finished turn added a tool call.
    assert!(turn_has_tool_calls(&msgs, |id| id == "a1" || id == "t-new"));
    // The finished turn is a plain answer; an older turn's tool call must not defer it.
    assert!(!turn_has_tool_calls(&msgs, |id| id == "a1"));
    assert!(!turn_has_tool_calls(&msgs, |_| false));
}

#[test]
fn auto_compaction_is_limited_to_one_per_turn() {
    use super::{auto_compaction_decision as d, AutoCompactionDecision as D};
    assert_eq!(d(false, false), D::None);
    assert_eq!(d(false, true), D::None);
    assert_eq!(d(true, false), D::Compact);
    // Second overflow in the same turn: fail rather than loop.
    assert_eq!(d(true, true), D::GiveUp);
}

// -- stream-finished policy (#959) ---------------------------------------
//
// `plan_after_stream_finished` is the whole auto-compaction decision the controller's
// `handle_response_stream_finished` acts on. It is tested here as a pure function because
// that handler needs a live provider stream (token usage, task messages, exchanges) to drive.

fn turn(requested_tool_calls: bool, overflowing: Option<bool>) -> super::FinishedStream {
    super::FinishedStream::Turn {
        requested_tool_calls,
        overflowing,
    }
}

/// A turn that ends asking for tool calls never starts a summary, however full the context:
/// its calls are queued only after the finish handler returns, and a summary sent first
/// cancelled them and halted the task (the original #959 defect).
#[test]
fn tool_call_turn_over_threshold_defers_compaction() {
    use super::{AutoCompactionDecision as D, plan_after_stream_finished as plan};
    for already in [false, true] {
        let p = plan(turn(true, Some(true)), already);
        assert_eq!(p.decision, D::None, "already_compacted={already}");
        assert!(!p.resume_with_continue_message);
        assert!(!p.restore_turn_allowance);
    }
}

/// The next tool-free turn end of that task picks the deferred compaction up.
#[test]
fn tool_free_turn_over_threshold_compacts() {
    use super::{AutoCompactionDecision as D, plan_after_stream_finished as plan};
    let p = plan(turn(false, Some(true)), false);
    assert_eq!(p.decision, D::Compact);
    // The continue message is not sent from the turn itself; it follows the summary.
    assert!(!p.resume_with_continue_message);
}

/// The automatic summary resumes the task with the continue message, so the turn does not
/// end on the summary text (the round-16 symptom on 8d9678624).
#[test]
fn committed_auto_summary_resumes_the_task() {
    use super::{AutoCompactionDecision as D, FinishedStream, plan_after_stream_finished as plan};
    let p = plan(
        FinishedStream::Summary {
            auto: true,
            committed: true,
        },
        true,
    );
    assert!(p.resume_with_continue_message);
    // The summary stream never triggers another summary (recursion guard).
    assert_eq!(p.decision, D::None);
    // The continue request is built from the compacted history, so it carries on the task.
    assert!(build_continue_message(false).starts_with("Continue if you have next steps"));
}

/// A summary that found nothing to commit has nothing to continue from.
#[test]
fn uncommitted_auto_summary_does_not_resume() {
    use super::{FinishedStream, plan_after_stream_finished as plan};
    let p = plan(
        FinishedStream::Summary {
            auto: true,
            committed: false,
        },
        true,
    );
    assert!(!p.resume_with_continue_message);
}

/// Manual `/compact` (sent with overflow=false) only summarizes: no continue, no new turn.
#[test]
fn manual_compact_does_not_resume_or_recompact() {
    use super::{AutoCompactionDecision as D, FinishedStream, plan_after_stream_finished as plan};
    for already in [false, true] {
        let p = plan(
            FinishedStream::Summary {
                auto: false,
                committed: true,
            },
            already,
        );
        assert!(!p.resume_with_continue_message);
        assert_eq!(p.decision, D::None);
        assert!(!p.restore_turn_allowance);
    }
}

/// End to end through the policy: a tool loop crosses the threshold, keeps running, the
/// tool-free turn end compacts, the summary resumes the task, the resumed turn comes in under
/// the threshold — and a later overflow in the same long task compacts again rather than
/// failing with GiveUp, because the first compaction worked.
#[test]
fn long_task_sequence_compacts_resumes_and_can_compact_again() {
    use super::{AutoCompactionDecision as D, FinishedStream, plan_after_stream_finished as plan};
    let mut compacted_this_turn = false;
    let step = |stream: FinishedStream, compacted: &mut bool| {
        let p = plan(stream, *compacted);
        if p.restore_turn_allowance {
            *compacted = false;
        }
        if p.decision == D::Compact {
            *compacted = true;
        }
        p
    };

    // Tool loop over the threshold: no summary, the calls run.
    assert_eq!(
        step(turn(true, Some(true)), &mut compacted_this_turn).decision,
        D::None
    );
    // Tool-free turn end: compact.
    assert_eq!(
        step(turn(false, Some(true)), &mut compacted_this_turn).decision,
        D::Compact
    );
    // The summary commits and resumes the task.
    assert!(
        step(
            FinishedStream::Summary {
                auto: true,
                committed: true,
            },
            &mut compacted_this_turn,
        )
        .resume_with_continue_message
    );
    // The continued turn is under the threshold: the allowance comes back.
    let p = step(turn(true, Some(false)), &mut compacted_this_turn);
    assert!(p.restore_turn_allowance);
    assert!(!compacted_this_turn);
    // The task grows past the threshold again later: compact again, not GiveUp.
    assert_eq!(
        step(turn(false, Some(true)), &mut compacted_this_turn).decision,
        D::Compact
    );
}

/// A summary that is itself still over the threshold must not loop: the first turn after it
/// reports overflow with the allowance spent, which fails the turn instead.
#[test]
fn still_over_after_compaction_gives_up() {
    use super::{AutoCompactionDecision as D, plan_after_stream_finished as plan};
    let p = plan(turn(false, Some(true)), true);
    assert_eq!(p.decision, D::GiveUp);
    assert!(!p.restore_turn_allowance);
}

/// No usage reported: nothing is decided and the allowance is not restored on a guess.
#[test]
fn unknown_usage_changes_nothing() {
    use super::{AutoCompactionDecision as D, plan_after_stream_finished as plan};
    for (tools, already) in [(false, false), (false, true), (true, true)] {
        let p = plan(turn(tools, None), already);
        assert_eq!(p.decision, D::None);
        assert!(!p.restore_turn_allowance);
        assert!(!p.resume_with_continue_message);
    }
}

/// Under the threshold with no compaction this turn: nothing at all.
#[test]
fn under_threshold_turn_is_a_no_op() {
    use super::{AutoCompactionDecision as D, plan_after_stream_finished as plan};
    for tools in [false, true] {
        let p = plan(turn(tools, Some(false)), false);
        assert_eq!(p.decision, D::None);
        assert!(!p.restore_turn_allowance);
        assert!(!p.resume_with_continue_message);
    }
}

// -- tool-loop compaction (#1059) ----------------------------------------
//
// An agent loop whose every step asks for another tool call never reaches a tool-free turn
// end, so before #1059 it was never compacted and each follow-up grew until the provider
// rejected it. `TurnCompactionState` is the bookkeeping the controller keeps per
// conversation; these tests drive exactly the methods the controller calls, in the order it
// calls them: `on_stream_finished` (handle_response_stream_finished), `on_tool_results_ready`
// (send_follow_up_for_conversation), `on_tool_results_abandoned` (stop, reject, close,
// stashed request) and `on_new_user_turn` (send_query).
//
// Scenario from the issue: context window 32_000, every step reports 20_000 prompt tokens
// and asks for another tool call.

fn summary_stream(auto: bool, committed: bool) -> super::FinishedStream {
    super::FinishedStream::Summary { auto, committed }
}

/// The issue's scenario really is over the threshold: 20_000 of a 32_000 window with the
/// default reserve. (Guards the numbers the sequence tests below stand for.)
#[test]
fn issue_scenario_step_is_over_the_threshold() {
    let cfg = cfg_default();
    let model = model_limit_from_parts(32_000, 0);
    let counts = TokenCounts {
        total: 20_000,
        ..Default::default()
    };
    assert!(is_overflow(&cfg, counts, model));
}

/// A tool-call step over the threshold still never summarizes on the spot (#959), but it
/// marks its results to be sent with the automatic summary.
#[test]
fn tool_call_turn_over_threshold_compacts_with_its_results() {
    use super::{AutoCompactionDecision as D, plan_after_stream_finished as plan};
    let p = plan(turn(true, Some(true)), false);
    assert_eq!(p.decision, D::None, "the calls must not be cancelled");
    assert!(p.compact_after_tool_results);
    assert!(!p.resume_with_continue_message);
}

/// Only an over-threshold tool-call step with the allowance unspent defers a compaction.
#[test]
fn compaction_is_deferred_only_for_an_over_threshold_tool_step() {
    use super::plan_after_stream_finished as plan;
    // Allowance spent (the summary itself is still over): no second summary, no loop.
    assert!(!plan(turn(true, Some(true)), true).compact_after_tool_results);
    // Under the threshold, unknown usage, or a tool-free turn (which compacts directly).
    for already in [false, true] {
        assert!(!plan(turn(true, Some(false)), already).compact_after_tool_results);
        assert!(!plan(turn(true, None), already).compact_after_tool_results);
        assert!(!plan(turn(false, Some(true)), already).compact_after_tool_results);
    }
    for auto in [false, true] {
        for committed in [false, true] {
            assert!(!plan(summary_stream(auto, committed), false).compact_after_tool_results);
        }
    }
}

/// The whole tool-loop sequence: step over the threshold → deferred; results ready → one
/// request carrying the results and the automatic summary; the summary commits → continue
/// message; the continued step is under the threshold → allowance back; the loop grows past
/// the threshold again → compacts again. A loop that never pauses keeps compacting for as
/// long as each compaction helps.
#[test]
fn never_ending_tool_loop_compacts_each_time_it_fills_the_window() {
    use super::{AutoCompactionDecision as D, ToolResultsRequest as R, TurnCompactionState};
    let mut state = TurnCompactionState::default();

    for round in 0..4 {
        // A few steps under the threshold: plain follow-ups.
        for _ in 0..2 {
            let p = state.on_stream_finished(turn(true, Some(false)));
            assert_eq!(p.decision, D::None);
            assert!(!p.compact_after_tool_results);
            assert_eq!(
                state.on_tool_results_ready(true),
                R::FollowUp,
                "round {round}"
            );
        }
        // The step that crosses the threshold, still asking for a tool call.
        let p = state.on_stream_finished(turn(true, Some(true)));
        assert_eq!(p.decision, D::None, "round {round}: the calls run");
        assert!(p.compact_after_tool_results, "round {round}");
        assert!(state.compaction_deferred_to_tool_results());
        // Its results are ready: they go out with the summary, and the allowance is spent.
        assert_eq!(
            state.on_tool_results_ready(true),
            R::SummarizeWithResults,
            "round {round}"
        );
        assert!(state.auto_compacted());
        assert!(!state.compaction_deferred_to_tool_results());
        // The summary commits: the task resumes with the continue message.
        let p = state.on_stream_finished(summary_stream(true, true));
        assert!(p.resume_with_continue_message, "round {round}");
        assert_eq!(p.decision, D::None);
        // The continued step is back under the threshold: the compaction worked.
        let p = state.on_stream_finished(turn(true, Some(false)));
        assert!(p.restore_turn_allowance, "round {round}");
        assert!(!state.auto_compacted());
        assert_eq!(state.on_tool_results_ready(true), R::FollowUp);
    }
    assert!(state.is_idle());
}

/// Before #1059 the same loop was never compacted: the tool-call step's plan carried nothing
/// to act on, and every step's results went out as a growing plain follow-up. (Asserts the
/// controller-visible difference: the deferral, not just the absence of a decision.)
#[test]
fn tool_loop_over_threshold_is_not_left_uncompacted() {
    use super::{ToolResultsRequest as R, TurnCompactionState};
    let mut state = TurnCompactionState::default();
    let mut summaries = 0;
    for _ in 0..10 {
        state.on_stream_finished(turn(true, Some(true)));
        if state.on_tool_results_ready(true) == R::SummarizeWithResults {
            summaries += 1;
            state.on_stream_finished(summary_stream(true, true));
            // Each compaction brings the next step under the threshold.
            state.on_stream_finished(turn(true, Some(false)));
            state.on_tool_results_ready(true);
        }
    }
    assert_eq!(summaries, 10, "every over-threshold step compacts");
}

/// A summary that is itself still over the threshold cannot loop: with the allowance spent,
/// the next over-threshold tool step is not compacted again (its calls run and its results go
/// out as a plain follow-up), and the loop's first tool-free turn end still over gives up.
#[test]
fn tool_loop_summary_still_over_does_not_loop_and_gives_up_at_the_turn_end() {
    use super::{AutoCompactionDecision as D, ToolResultsRequest as R, TurnCompactionState};
    let mut state = TurnCompactionState::default();
    state.on_stream_finished(turn(true, Some(true)));
    assert_eq!(state.on_tool_results_ready(true), R::SummarizeWithResults);
    assert!(
        state
            .on_stream_finished(summary_stream(true, true))
            .resume_with_continue_message
    );

    for _ in 0..3 {
        let p = state.on_stream_finished(turn(true, Some(true)));
        assert_eq!(p.decision, D::None, "the calls are never cancelled");
        assert!(!p.compact_after_tool_results, "no second summary this turn");
        assert_eq!(state.on_tool_results_ready(true), R::FollowUp);
    }
    assert_eq!(
        state.on_stream_finished(turn(false, Some(true))).decision,
        D::GiveUp
    );
}

/// A summary that found nothing to commit doesn't resume, and leaves no deferral behind to
/// fire again on the next follow-up.
#[test]
fn uncommitted_tool_loop_summary_leaves_no_deferral() {
    use super::{ToolResultsRequest as R, TurnCompactionState};
    let mut state = TurnCompactionState::default();
    state.on_stream_finished(turn(true, Some(true)));
    assert_eq!(state.on_tool_results_ready(true), R::SummarizeWithResults);
    assert!(
        !state
            .on_stream_finished(summary_stream(true, false))
            .resume_with_continue_message
    );
    assert!(!state.compaction_deferred_to_tool_results());
    assert_eq!(state.on_tool_results_ready(true), R::FollowUp);
}

/// Stop / Ctrl-C (or a pane close, or a stashed user request taking the results) while the
/// step's tools run: the deferral is dropped, so it can't summarize some unrelated later
/// follow-up. The allowance is untouched (nothing was summarized).
#[test]
fn stop_before_the_results_drops_the_deferral() {
    use super::{ToolResultsRequest as R, TurnCompactionState};
    let mut state = TurnCompactionState::default();
    state.on_stream_finished(turn(true, Some(true)));
    state.on_tool_results_abandoned();
    assert!(!state.compaction_deferred_to_tool_results());
    assert!(!state.auto_compacted());
    assert!(state.is_idle());
    assert_eq!(state.on_tool_results_ready(true), R::FollowUp);
}

/// Stop during the deferred summary itself: the results were recorded before the summary
/// request was built, so there is nothing to lose; the state holds no deferral, only the
/// spent allowance, and the user's next query starts clean.
#[test]
fn stop_during_the_deferred_summary_leaves_a_clean_state() {
    use super::{ToolResultsRequest as R, TurnCompactionState};
    let mut state = TurnCompactionState::default();
    state.on_stream_finished(turn(true, Some(true)));
    assert_eq!(state.on_tool_results_ready(true), R::SummarizeWithResults);
    // The stopped summary stream never reaches `on_stream_finished` (not a clean finish).
    state.on_tool_results_abandoned();
    assert!(!state.compaction_deferred_to_tool_results());
    assert!(state.auto_compacted());
    state.on_new_user_turn();
    assert!(state.is_idle());
}

/// The user rejects the step's only tool call: no follow-up carries its result, so the
/// controller abandons the deferral.
#[test]
fn rejected_tool_call_drops_the_deferral() {
    use super::TurnCompactionState;
    let mut state = TurnCompactionState::default();
    state.on_stream_finished(turn(true, Some(true)));
    state.on_tool_results_abandoned();
    assert!(state.is_idle());
}

/// Results of a subagent's task (a CLI/LRC subagent's own loop) never carry the summary:
/// they go as a plain follow-up, the deferral is dropped, and the allowance is not spent.
#[test]
fn subagent_task_results_go_as_a_plain_follow_up() {
    use super::{ToolResultsRequest as R, TurnCompactionState};
    let mut state = TurnCompactionState::default();
    state.on_stream_finished(turn(true, Some(true)));
    assert_eq!(state.on_tool_results_ready(false), R::FollowUp);
    assert!(!state.compaction_deferred_to_tool_results());
    assert!(!state.auto_compacted());
}

/// Manual `/compact` while a loop is mid-step: the slash command cancels the conversation's
/// progress first (abandoning the deferral), and its own summary neither resumes the task nor
/// leaves a deferral, so the step's results are not summarized a second time.
#[test]
fn manual_compact_mid_loop_supersedes_the_deferral() {
    use super::{ToolResultsRequest as R, TurnCompactionState};
    let mut state = TurnCompactionState::default();
    state.on_stream_finished(turn(true, Some(true)));
    state.on_tool_results_abandoned(); // cancel_conversation_progress
    let p = state.on_stream_finished(summary_stream(false, true));
    assert!(!p.resume_with_continue_message);
    assert!(!state.compaction_deferred_to_tool_results());
    assert_eq!(state.on_tool_results_ready(true), R::FollowUp);
}

/// A deferral lives only until the next finished stream: if anything else finished first,
/// that stream's own plan replaces it.
#[test]
fn a_later_finished_stream_replaces_the_deferral() {
    use super::TurnCompactionState;
    let mut state = TurnCompactionState::default();
    state.on_stream_finished(turn(true, Some(true)));
    state.on_stream_finished(turn(true, Some(false)));
    assert!(!state.compaction_deferred_to_tool_results());
    assert!(state.is_idle());
}

/// A new user query starts a fresh turn: allowance back, no deferral.
#[test]
fn new_user_turn_resets_the_tool_loop_state() {
    use super::{ToolResultsRequest as R, TurnCompactionState};
    let mut state = TurnCompactionState::default();
    state.on_stream_finished(turn(true, Some(true)));
    assert_eq!(state.on_tool_results_ready(true), R::SummarizeWithResults);
    state.on_new_user_turn();
    assert!(state.is_idle());
    // The next over-threshold step may compact again.
    assert!(
        state
            .on_stream_finished(turn(true, Some(true)))
            .compact_after_tool_results
    );
}

/// The tool-free path is unchanged by #1059: it compacts directly and spends the allowance.
#[test]
fn tool_free_turn_still_compacts_directly() {
    use super::{AutoCompactionDecision as D, TurnCompactionState};
    let mut state = TurnCompactionState::default();
    let p = state.on_stream_finished(turn(false, Some(true)));
    assert_eq!(p.decision, D::Compact);
    assert!(!p.compact_after_tool_results);
    assert!(state.auto_compacted());
}
