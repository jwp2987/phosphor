//! Scenario -> seam script. Turns a [`Scenario`]'s turns into the `SeamTurn` each request gets,
//! and keeps the record of what was emitted (call ids per turn) that the I4 oracle compares the
//! following request against.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use fake_llm::scenario::{CallId, Chaos, Match, Reply, Scenario, StatusKind, Turn};

use crate::ai::agent_providers::fake_stream_seam::{
    RecordedRequest, RecordedRole, SeamError, SeamEvent, SeamTurn,
};
use crate::ai::agent_providers::tools;

/// How a scripted reply's stream ends. It decides what the app commits of the reply
/// (`chat_stream.rs`, `generate_byop_output`): the commit is the single `final_messages` yield
/// after the stream loop, and only two of the three ways out of that loop reach it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyEnd {
    /// The terminal `End` was delivered. The loop ends, the commit runs (it awaits a websearch
    /// or webfetch of the reply first), then `Finished`. Calls are taken from the `End`'s
    /// captured content, so a repeated id is renamed `id#2`.
    Finished,
    /// The open failed, or the stream failed or stalled part-way (`status`, `context_overflow`,
    /// `disconnect`, `stall`). The loop leaves by `yield Err; return`: NO commit. Only the
    /// cards shown while the reply streamed are in the conversation.
    Broke,
    /// The body closed with no `End` and no error (`never_end` alone). The loop ends as if the
    /// reply were complete, so the commit yields every call of the reply, and only then is the
    /// missing `End` reported as an error ("the partial content is not lost ...
    /// `final_messages` above has been flushed", the `end_count == 0` guard). There is no
    /// captured content, so calls are the stream's chunks accumulated by id: a repeated id is
    /// ONE call (the later chunk replaces the earlier), not renamed.
    ///
    /// Committed is not run (#2048, owner decision 2026-11-10): the tools the stream runs by
    /// itself (websearch, webfetch, todowrite, the codebase tools) are answered "not executed"
    /// and nothing is awaited, and the call that kept a repeated id is answered with the names
    /// of the calls it replaced. Every call still has its place and one result, so what I4
    /// sees of such a reply is what it saw before.
    ClosedEarly,
}

/// What one scripted reply emitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmittedTurn {
    /// Index of the request this reply answered.
    pub request_index: usize,
    /// The turn that answered (`None` = the implicit `END`).
    pub turn: Option<usize>,
    /// The calls the app commits when nobody cuts the stream, in order; empty for
    /// [`ReplyEnd::Broke`] (nothing is committed).
    ///
    /// * [`ReplyEnd::Finished`]: the ids as put on the wire. A `Missing` id is recorded as
    ///   genai assigns it (`call_<index>`); an empty literal id is recorded as "" (the app
    ///   invents a uuid); a repeated id is listed again (I4 renames it as the app does).
    /// * [`ReplyEnd::ClosedEarly`]: the same, with a repeated id listed once, at its first
    ///   place (the app accumulates chunks by id and there is no `End` to tell two calls with
    ///   one id apart). "" is never merged: each empty id gets its own invented id.
    pub call_ids: Vec<String>,
    /// Chaos kinds in force for this reply (the I6 allowlist keys on them).
    pub chaos: Vec<&'static str>,
    /// How the stream ended: whether the reply's calls were committed, and in which form.
    pub end: ReplyEnd,
    /// The calls the app may have put into the conversation BEFORE the reply was committed: it
    /// shows a call's card as soon as the call's chunk parses (`chat_stream.rs`, the
    /// `ToolCallChunk` arm), as a tool-call message of its own, and that message stays when the
    /// stream then breaks or is cancelled (the next request answers it with a cancelled
    /// result). These are the ids the stream delivered, in order, up to the first call of a
    /// tool that gets no early card: a call is shown early only when every call before it was.
    /// An id repeated in the reply is listed once (a repeat updates the same card); "" is an id
    /// the app invents. Since #2054 the app shows no NEW card after a chunk that may repeat an id
    /// (`chat_stream::may_repeat_call_id`), so that the repeat, renamed at the `End`, is recorded
    /// where it was emitted; this list does not model that and stays the upper bound. The app may have shown any leading part of this list (a call whose
    /// arguments do not parse, or that is refused at dispatch, also ends the run), so I4 accepts
    /// a prefix of it for a reply that broke or was cut. Empty when nothing was delivered.
    pub early_call_ids: Vec<String>,
}

impl EmittedTurn {
    /// The reply did not end with an `End`: the app reports an error for it and may ask again
    /// (a retry of the same request, or a resume). Says nothing about the commit: see
    /// [`ReplyEnd`].
    pub fn aborted(&self) -> bool {
        self.end != ReplyEnd::Finished
    }
}

/// The tools whose calls get no card while the reply streams (`chat_stream.rs`, the
/// `ToolCallChunk` arm lists them: the web tools and the codebase tools have their own loading
/// frames, `todowrite` is turned into a todo update, `read_skill` binds only final arguments).
fn gets_no_early_card(tool: &str) -> bool {
    [
        tools::webfetch::TOOL_NAME,
        tools::websearch::TOOL_NAME,
        tools::codebase::TOOL_NAME,
        tools::get_relevant_files::TOOL_NAME,
        tools::todowrite::TOOL_NAME,
        tools::skill::READ_SKILL.name,
    ]
    .contains(&tool)
}

/// [`EmittedTurn::call_ids`] of a reply whose body closed with no `End`: the app keeps one call
/// per id, at the place the id first came (`tool_order` / `tool_bufs` in the stream loop).
fn merged_by_id(ids: Vec<String>) -> Vec<String> {
    let mut merged: Vec<String> = Vec::new();
    for id in ids {
        if id.is_empty() || !merged.contains(&id) {
            merged.push(id);
        }
    }
    merged
}

/// [`EmittedTurn::early_call_ids`] for the events a stream delivers.
fn early_call_ids(events: &[SeamEvent]) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for event in events {
        match event {
            SeamEvent::ToolCall { id, name, .. } => {
                if gets_no_early_card(name) {
                    break;
                }
                if id.is_empty() || !ids.contains(id) {
                    ids.push(id.clone());
                }
            }
            // Nothing is delivered after a failure or a stall.
            SeamEvent::Fail(_) | SeamEvent::Stall => break,
            SeamEvent::Text(_) | SeamEvent::Reasoning(_) | SeamEvent::Wait(_) => {}
        }
    }
    ids
}

#[derive(Default)]
pub struct Emitted {
    pub turns: Vec<EmittedTurn>,
}

#[derive(Clone)]
pub struct Player {
    scenario: Arc<Scenario>,
    emitted: Arc<Mutex<Emitted>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Player {
    pub fn new(scenario: Scenario) -> Self {
        Self {
            scenario: Arc::new(scenario),
            emitted: Arc::default(),
        }
    }

    pub fn scenario(&self) -> &Scenario {
        &self.scenario
    }

    pub fn emitted(&self) -> Vec<EmittedTurn> {
        lock(&self.emitted).turns.clone()
    }

    /// How many scripted turns have answered a request (the implicit `END` does not count).
    pub fn turns_used(&self) -> usize {
        lock(&self.emitted)
            .turns
            .iter()
            .filter(|t| t.turn.is_some())
            .count()
    }

    /// The seam script: picks the first turn matching the request.
    pub fn answer(&self, request: &RecordedRequest) -> SeamTurn {
        let picked = self
            .scenario
            .turns
            .iter()
            .position(|turn| matches(&turn.when, request));
        let (seam_turn, record) = match picked {
            Some(i) => render_turn(&self.scenario, i, request.index),
            None => (
                SeamTurn::text("END"),
                EmittedTurn {
                    request_index: request.index,
                    turn: None,
                    call_ids: Vec::new(),
                    chaos: Vec::new(),
                    end: ReplyEnd::Finished,
                    early_call_ids: Vec::new(),
                },
            ),
        };
        lock(&self.emitted).turns.push(record);
        seam_turn
    }
}

fn matches(when: &Match, request: &RecordedRequest) -> bool {
    match when {
        Match::Nth { n } => request.index == *n as usize,
        // The newest message of the conversation: a request's very last message is the
        // environment tail the app appends, never a tool message.
        Match::LastToolResult { id } => request.newest_message().is_some_and(|m| {
            m.role == RecordedRole::Tool && m.tool_response_ids.iter().any(|r| r == id)
        }),
        Match::UserContains { token } => request
            .last_user_text()
            .is_some_and(|text| text.contains(token.as_str())),
        Match::Default => true,
    }
}

/// Resolves a reply's call ids in order.
fn resolve_ids(scenario: &Scenario, turn_index: usize, turn: &Turn) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for (k, call) in turn.reply.calls().iter().enumerate() {
        let id = match &call.id {
            CallId::Fresh => format!("call_{}_{turn_index}_{k}", scenario.seed % 10_000),
            CallId::Literal { id } => id.clone(),
            CallId::Missing => format!("call_{k}"),
            CallId::DuplicateOf { k: of } => ids
                .get((*of).min(ids.len().saturating_sub(1)))
                .cloned()
                .unwrap_or_else(|| format!("call_{k}")),
        };
        ids.push(id);
    }
    ids
}

fn render_turn(
    scenario: &Scenario,
    turn_index: usize,
    request_index: usize,
) -> (SeamTurn, EmittedTurn) {
    let turn = &scenario.turns[turn_index];
    let chaos = turn.chaos.clone().unwrap_or_default();
    let ids = resolve_ids(scenario, turn_index, turn);
    let cmd_ids: BTreeMap<String, String> = BTreeMap::new();

    let mut events: Vec<SeamEvent> = Vec::new();
    let pieces = if chaos.split.is_some() { 5 } else { 2 };
    let push_text = |events: &mut Vec<SeamEvent>, text: &str| {
        for piece in split_chars(text, pieces) {
            events.push(SeamEvent::Text(piece));
        }
    };
    match &turn.reply {
        Reply::Text { text } => push_text(&mut events, text),
        Reply::Reasoning { text } => events.push(SeamEvent::Reasoning(text.clone())),
        Reply::Empty => {}
        Reply::ToolCalls { calls } | Reply::Mixed { calls, .. } => {
            if let Reply::Mixed { text, .. } = &turn.reply {
                push_text(&mut events, text);
            }
            for (call, id) in calls.iter().zip(&ids) {
                events.push(SeamEvent::ToolCall {
                    id: id.clone(),
                    name: call.name.clone(),
                    args: call.render_args(&cmd_ids),
                });
            }
        }
    }

    let mut seam = SeamTurn {
        open_error: None,
        events,
        end: true,
    };
    apply_chaos(&mut seam, &chaos);
    // Which way the app's stream loop is left (see `ReplyEnd`): an error or a stall anywhere
    // means no commit; a body that just stops is committed as received; an `End` is the
    // ordinary commit.
    let broke = seam.open_error.is_some()
        || seam
            .events
            .iter()
            .any(|e| matches!(e, SeamEvent::Fail(_) | SeamEvent::Stall));
    let end = if broke {
        ReplyEnd::Broke
    } else if seam.end {
        ReplyEnd::Finished
    } else {
        ReplyEnd::ClosedEarly
    };
    // A failed open delivers no event at all.
    let early_call_ids = if seam.open_error.is_some() {
        Vec::new()
    } else {
        early_call_ids(&seam.events)
    };
    let record = EmittedTurn {
        request_index,
        turn: Some(turn_index),
        call_ids: match end {
            ReplyEnd::Finished => ids,
            ReplyEnd::ClosedEarly => merged_by_id(ids),
            ReplyEnd::Broke => Vec::new(),
        },
        chaos: chaos.kinds(),
        end,
        early_call_ids,
    };
    (seam, record)
}

fn split_chars(text: &str, parts: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return Vec::new();
    }
    let parts = parts.clamp(1, chars.len());
    let len = chars.len().div_ceil(parts);
    chars.chunks(len).map(|c| c.iter().collect()).collect()
}

/// Event-level chaos. (Byte-level cuts are L0's: this layer replaces the transport.)
fn apply_chaos(turn: &mut SeamTurn, chaos: &Chaos) {
    if let Some(status) = &chaos.status {
        let (status, body) = match status {
            StatusKind::Http { code } => (*code, format!("fake error {code}")),
            StatusKind::Html => (502, "<html><body>Bad Gateway</body></html>".to_owned()),
        };
        turn.open_error = Some(SeamError::Http { status, body });
        return;
    }
    if chaos.context_overflow {
        turn.open_error = Some(SeamError::Http {
            status: 400,
            body: "This model's maximum context length is 8192 tokens. However, your messages \
                   resulted in 9001 tokens. Please reduce the length of the messages."
                .to_owned(),
        });
        return;
    }
    if chaos.delay_ms > 0 {
        let pause = Duration::from_millis(chaos.delay_ms);
        turn.events = turn
            .events
            .drain(..)
            .flat_map(|event| [SeamEvent::Wait(pause), event])
            .collect();
    }
    if let Some(n) = chaos.disconnect_after {
        turn.events.truncate(n as usize);
        turn.events.push(SeamEvent::Fail(SeamError::Transport(
            "connection reset by peer".to_owned(),
        )));
        turn.end = false;
    } else if let Some(n) = chaos.stall_after {
        turn.events.truncate(n as usize);
        turn.events.push(SeamEvent::Stall);
        turn.end = false;
    }
    if chaos.never_end {
        turn.end = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fake_llm::scenario::{Args, Call};

    fn request(index: usize) -> RecordedRequest {
        RecordedRequest {
            index,
            model_id: "m".to_owned(),
            messages: Vec::new(),
            tools_offered: Vec::new(),
        }
    }

    fn scenario_with(reply: Reply, chaos: Option<Chaos>) -> Scenario {
        let mut s = Scenario::new(42);
        s.turns.push(Turn {
            when: Match::Nth { n: 0 },
            reply,
            chaos,
        });
        s
    }

    #[test]
    fn unmatched_requests_get_end() {
        let player = Player::new(scenario_with(Reply::Empty, None));
        let turn = player.answer(&request(5));
        assert_eq!(turn, SeamTurn::text("END"));
        assert_eq!(player.turns_used(), 0);
    }

    #[test]
    fn missing_and_duplicate_ids_resolve_like_the_wire() {
        let calls = vec![
            Call::new(CallId::Fresh, "grep", Args::Valid),
            Call::new(CallId::Missing, "grep", Args::Valid),
            Call::new(CallId::DuplicateOf { k: 0 }, "grep", Args::Valid),
        ];
        let player = Player::new(scenario_with(Reply::ToolCalls { calls }, None));
        player.answer(&request(0));
        let emitted = player.emitted();
        let ids = &emitted[0].call_ids;
        assert_eq!(ids[1], "call_1");
        assert_eq!(ids[2], ids[0]);
    }

    #[test]
    fn a_disconnect_aborts_the_turn() {
        let chaos = Chaos {
            disconnect_after: Some(1),
            ..Chaos::default()
        };
        let calls = vec![Call::new(CallId::Fresh, "grep", Args::Valid)];
        let player = Player::new(scenario_with(Reply::ToolCalls { calls }, Some(chaos)));
        player.answer(&request(0));
        let emitted = player.emitted();
        assert_eq!(emitted[0].end, ReplyEnd::Broke);
        assert!(emitted[0].aborted());
        assert!(emitted[0].call_ids.is_empty());
        // The call was delivered before the connection dropped: the app may have shown it.
        assert_eq!(emitted[0].early_call_ids.len(), 1);
        assert!(!emitted[0].early_call_ids[0].is_empty());
    }

    /// What a reply may leave behind before it is committed: the delivered calls up to the
    /// first one whose tool gets no early card, a repeated id once, nothing after a failure
    /// and nothing at all when the open failed.
    #[test]
    fn early_calls_are_the_delivered_run_before_the_first_tool_without_an_early_card() {
        let literal = |id: &str| CallId::Literal { id: id.to_owned() };
        let calls = vec![
            Call::new(literal("a"), "grep", Args::Valid),
            Call::new(literal("a"), "grep", Args::Valid),
            Call::new(literal("b"), "grep", Args::Valid),
            Call::new(literal("w"), "websearch", Args::Valid),
            Call::new(literal("c"), "grep", Args::Valid),
        ];
        let whole = Player::new(scenario_with(
            Reply::ToolCalls {
                calls: calls.clone(),
            },
            None,
        ));
        whole.answer(&request(0));
        let emitted = whole.emitted();
        assert_eq!(emitted[0].end, ReplyEnd::Finished);
        assert_eq!(emitted[0].call_ids, ["a", "a", "b", "w", "c"]);
        assert_eq!(emitted[0].early_call_ids, ["a", "b"]);

        // Cut after the first event: only that call was delivered.
        let cut = Chaos {
            disconnect_after: Some(1),
            ..Chaos::default()
        };
        let broken = Player::new(scenario_with(
            Reply::ToolCalls {
                calls: calls.clone(),
            },
            Some(cut),
        ));
        broken.answer(&request(0));
        let emitted = broken.emitted();
        assert_eq!(emitted[0].end, ReplyEnd::Broke);
        assert_eq!(emitted[0].early_call_ids, ["a"]);

        // A reply that starts with a search shows nothing early.
        let search_first = vec![
            Call::new(literal("w"), "websearch", Args::Valid),
            Call::new(literal("a"), "grep", Args::Valid),
        ];
        let player = Player::new(scenario_with(
            Reply::ToolCalls {
                calls: search_first,
            },
            None,
        ));
        player.answer(&request(0));
        assert!(player.emitted()[0].early_call_ids.is_empty());

        // A failed open delivers nothing.
        let refused = Chaos {
            context_overflow: true,
            ..Chaos::default()
        };
        let player = Player::new(scenario_with(Reply::ToolCalls { calls }, Some(refused)));
        player.answer(&request(0));
        let emitted = player.emitted();
        assert_eq!(emitted[0].end, ReplyEnd::Broke);
        assert!(emitted[0].early_call_ids.is_empty());
    }

    /// The three ways a reply's stream ends, as the app's stream loop tells them apart
    /// (`ReplyEnd`). The round 12 failure of both random fuzz tests (build/r12a) was a reply
    /// that only lacked its `End`: the app commits such a reply as received, and the record
    /// said "nothing was committed". Its calls here are the ones of that scenario.
    #[test]
    fn a_body_that_closes_with_no_end_is_committed_and_a_broken_stream_is_not() {
        let literal = |id: &str| CallId::Literal { id: id.to_owned() };
        let calls = vec![
            Call::new(literal("call_fixed"), "not_a_real_tool", Args::EmptyObject),
            Call::new(literal("w"), "websearch", Args::Valid),
            Call::new(
                literal("call_fixed"),
                "read_shell_command_output",
                Args::Valid,
            ),
        ];
        let record = |chaos: Option<Chaos>| {
            let player = Player::new(scenario_with(
                Reply::ToolCalls {
                    calls: calls.clone(),
                },
                chaos,
            ));
            player.answer(&request(0));
            player.emitted().remove(0)
        };

        // With its `End`: every call, the repeat still listed (I4 renames it `call_fixed#2`).
        let finished = record(None);
        assert_eq!(finished.end, ReplyEnd::Finished);
        assert!(!finished.aborted());
        assert_eq!(finished.call_ids, ["call_fixed", "w", "call_fixed"]);

        // No `End`, no error: committed, one call per id, in the order the ids first came.
        let closed_early = record(Some(Chaos {
            never_end: true,
            ..Chaos::default()
        }));
        assert_eq!(closed_early.end, ReplyEnd::ClosedEarly);
        assert!(closed_early.aborted());
        assert_eq!(closed_early.call_ids, ["call_fixed", "w"]);
        assert_eq!(closed_early.early_call_ids, ["call_fixed"]);

        // An error before the body closes: no commit, whatever `never_end` says.
        let broke = record(Some(Chaos {
            never_end: true,
            disconnect_after: Some(2),
            ..Chaos::default()
        }));
        assert_eq!(broke.end, ReplyEnd::Broke);
        assert!(broke.call_ids.is_empty());
        assert_eq!(broke.early_call_ids, ["call_fixed"]);

        // Ids the app invents are never merged.
        assert_eq!(
            merged_by_id(vec![String::new(), "a".into(), String::new(), "a".into()]),
            ["", "a", ""]
        );
    }
}
