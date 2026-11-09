//! llmposter bridging (L0 and, later, L2).
//!
//! llmposter (AGPL-3.0-or-later, test-only) is the HTTP mock: it serves the OpenAI chat-completions
//! SSE dialect, chunks text by characters, injects HTTP errors, and truncates or drops a stream
//! after N frames. This module turns a [`Scenario`]'s turns into llmposter fixtures so those
//! behaviours are driven from the same JSON schema as everything else.
//!
//! What llmposter cannot do, and our own code does instead (see `wire.rs`):
//! * cut the byte stream anywhere (inside UTF-8, inside a JSON string, between `data:` and
//!   `\n\n`): it only splits text by characters;
//! * stream a tool call's arguments as many deltas, or send malformed arguments: its tool-call
//!   arguments are a `serde_json::Value`, always valid, always sent whole;
//! * choose tool-call ids (it numbers them `call_llmposter_N`).

use llmposter::fixture::{FailureConfig, Fixture, ToolCall};
use llmposter::{MockServer, ServerBuilder};

use crate::scenario::{Chaos, Reply, Scenario, StatusKind, Turn};

/// Name of the llmposter state machine that orders a scenario's turns.
const MACHINE: &str = "fake_llm_turns";

fn state_name(i: usize) -> String {
    if i == 0 {
        String::new()
    } else {
        format!("t{i}")
    }
}

/// One fixture per turn, chained by llmposter's scenario state machine: turn `i` answers the
/// `i`th request, whatever it contains. (llmposter cannot see a tool-result id, so the `Match`
/// variants other than `Nth` and `Default` are an L1 feature; L0 uses request order.) A final
/// catch-all answers `END`, matching the app seam's rule that the fake never hangs the app.
pub fn fixtures(scenario: &Scenario) -> Vec<Fixture> {
    let mut out: Vec<Fixture> = scenario
        .turns
        .iter()
        .enumerate()
        .map(|(i, turn)| {
            fixture_for_turn(turn).with_scenario(
                MACHINE,
                Some(&state_name(i)),
                Some(&state_name(i + 1)),
            )
        })
        .collect();
    out.push(
        Fixture::new()
            .respond_with_content("END")
            .with_streaming(Some(0), Some(64))
            .as_catch_all(),
    );
    out
}

/// A running llmposter server for `scenario`, on an ephemeral loopback port.
pub async fn serve(scenario: &Scenario) -> MockServer {
    ServerBuilder::new()
        .fixtures(fixtures(scenario))
        .build()
        .await
        .expect("llmposter server should start")
}

/// The fixture for one turn (no scenario-state wiring).
pub fn fixture_for_turn(turn: &Turn) -> Fixture {
    let chaos = turn.chaos.clone().unwrap_or_default();
    if let Some(status) = &chaos.status {
        let (code, message) = match status {
            StatusKind::Http { code } => (*code, status_message(*code)),
            StatusKind::Html => (502, "<html><body>Bad Gateway</body></html>".to_owned()),
        };
        return Fixture::new().with_error(code.clamp(400, 599), &message);
    }
    if chaos.context_overflow {
        return Fixture::new().with_error(
            400,
            "This model's maximum context length is 8192 tokens. However, your messages \
             resulted in 9001 tokens. Please reduce the length of the messages.",
        );
    }

    let mut fixture = Fixture::new();
    fixture = match &turn.reply {
        Reply::Text { text } | Reply::Reasoning { text } => fixture.respond_with_content(text),
        Reply::Empty => fixture.respond_with_content(""),
        Reply::ToolCalls { calls } | Reply::Mixed { calls, .. } => {
            fixture.respond_with_tool_calls(
                calls
                    .iter()
                    .map(|call| ToolCall {
                        name: call.name.clone(),
                        // llmposter requires a JSON object; the invalid shapes are wire.rs's job.
                        arguments: serde_json::from_str(&call.render_args(&Default::default()))
                            .ok()
                            .filter(serde_json::Value::is_object)
                            .unwrap_or_else(|| serde_json::json!({})),
                    })
                    .collect(),
            )
        }
    };

    // Streaming is requested by the client (`stream: true`); the fixture picks the pacing.
    let chunk = match chaos.split {
        Some(crate::scenario::Split::Bytes { n }) => n.max(1),
        _ => 7,
    };
    fixture = fixture.with_streaming(Some(chaos.delay_ms), Some(chunk));
    if chaos.disconnect_after.is_some() || chaos.stall_after.is_some() || chaos.never_end {
        fixture = fixture.with_failure(failure_for(&chaos));
    }
    fixture
}

fn failure_for(chaos: &Chaos) -> FailureConfig {
    FailureConfig {
        // llmposter counts frames including the preamble; both "drop" and "stall" end the stream
        // early, which the client sees as an unterminated SSE body.
        truncate_after_frames: chaos
            .disconnect_after
            .or(chaos.stall_after)
            // never_end: cut before the terminal frames of a short reply.
            .or(chaos.never_end.then_some(2)),
        ..FailureConfig::default()
    }
}

fn status_message(code: u16) -> String {
    match code {
        429 => "Rate limit reached for requests".to_owned(),
        529 => "Overloaded".to_owned(),
        503 => "The server is overloaded or not ready yet.".to_owned(),
        500 => "The server had an error while processing your request".to_owned(),
        other => format!("fake error {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::{Match, Reply};

    #[test]
    fn one_fixture_per_turn_plus_the_catch_all() {
        let mut s = Scenario::new(1);
        for n in 0..3 {
            s.turns.push(Turn {
                when: Match::Nth { n },
                reply: Reply::Text {
                    text: format!("t{n}"),
                },
                chaos: None,
            });
        }
        assert_eq!(fixtures(&s).len(), 4);
    }
}
