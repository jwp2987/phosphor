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

/// What one scripted reply emitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmittedTurn {
    /// Index of the request this reply answered.
    pub request_index: usize,
    /// The turn that answered (`None` = the implicit `END`).
    pub turn: Option<usize>,
    /// The call ids as put on the wire, in order. A `Missing` id is recorded as genai assigns it
    /// (`call_<index>`); an empty literal id is recorded as "" (the app invents a uuid).
    pub call_ids: Vec<String>,
    /// Chaos kinds in force for this reply (the I6 allowlist keys on them).
    pub chaos: Vec<&'static str>,
    /// The reply left the stream unfinished or failed it, so no calls were committed.
    pub aborted: bool,
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
                    aborted: false,
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
    // Calls count as emitted only if the stream delivered them intact to the End.
    let aborted = seam.open_error.is_some()
        || !seam.end
        || seam
            .events
            .iter()
            .any(|e| matches!(e, SeamEvent::Fail(_) | SeamEvent::Stall));
    let record = EmittedTurn {
        request_index,
        turn: Some(turn_index),
        call_ids: if aborted { Vec::new() } else { ids },
        chaos: chaos.kinds(),
        aborted,
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
        assert!(emitted[0].aborted);
        assert!(emitted[0].call_ids.is_empty());
    }
}
