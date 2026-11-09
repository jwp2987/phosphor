//! The invariants, checked against the observable state of the rig (never against the code under
//! test's own bookkeeping where an independent view exists).
//!
//! | id | invariant |
//! |---|---|
//! | I1 | every blocked action has an actionable card within delta; nothing pending sits neither blocked-with-card, runnable nor running for longer than delta |
//! | I2a | once the script is used up and the user has answered every card, the conversation is terminal within 5 s |
//! | I2b | Ctrl-C / Stop leaves it terminal within 2 s, nothing pending / running, no stream in flight |
//! | I3 | when the conversation is terminal, locked queued rows unlock within 2 s; unlocked rows of a conversation that succeeded are sent within 3 s (a cancel restores rows to the input instead: #690 / #1026, kept by #1799) |
//! | I4 | each request carries exactly one tool message per emitted call id, in order, none extra and none orphaned (duplicates renamed `id#2`) |
//! | I5 | `prompt_tests.rs`: a command is handed to the user only if it reads the tty |
//! | I6 | no panic (any thread) and no ERROR log beyond the allowance per chaos kind and count (`error_allowance`) |
//! | I7 | no card takes accept keys for an action that is done or cancelled (read from the card's own key routing, `AIBlock::pending_command_card_view_id`, not from the action status) |

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::ai::agent_providers::fake_stream_seam::{RecordedRequest, RecordedRole};
use crate::ai::blocklist::QueuedQueryOrigin;
use crate::ai::blocklist::action_model::PendingStage;

use super::log_capture::{self, Mark};
use super::player::EmittedTurn;
use super::rig::{Obs, Rig};

/// A broken invariant. `Display` is the failure message; the runner appends the scenario JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub invariant: &'static str,
    pub detail: String,
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.invariant, self.detail)
    }
}

fn violation(invariant: &'static str, detail: impl Into<String>) -> Violation {
    Violation {
        invariant,
        detail: detail.into(),
    }
}

/// Time budgets. `AGENT_FUZZ_TIME_SCALE` (default 1.0) stretches every wait for a slow box;
/// `AGENT_FUZZ_HUMAN_SCALE` (default 0.05) shrinks the actor's "human" delays (400-2500 ms).
#[derive(Debug, Clone)]
pub struct Knobs {
    pub delta: Duration,
    pub terminal_after_answers: Duration,
    pub terminal_after_stop: Duration,
    pub unlock: Duration,
    pub send: Duration,
    pub trigger_budget: Duration,
    pub stall_after: Duration,
    pub human_scale: f64,
}

impl Knobs {
    pub fn from_env() -> Self {
        let time = env_f64("AGENT_FUZZ_TIME_SCALE", 1.0).max(0.1);
        let scaled = |ms: u64| Duration::from_millis((ms as f64 * time) as u64);
        Self {
            delta: scaled(2_000),
            terminal_after_answers: scaled(5_000),
            terminal_after_stop: scaled(2_000),
            unlock: scaled(2_000),
            send: scaled(3_000),
            trigger_budget: scaled(3_000),
            stall_after: scaled(300),
            human_scale: env_f64("AGENT_FUZZ_HUMAN_SCALE", 0.05).clamp(0.0, 1.0),
        }
    }

    pub fn scale(&self, d: Duration) -> Duration {
        d.mul_f64(self.human_scale)
    }
}

fn env_f64(name: &str, default: f64) -> f64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Remembers since when a bad condition has held, so delta applies.
#[derive(Default)]
pub struct Oracle {
    since: HashMap<String, Instant>,
}

impl Oracle {
    fn holds_for(&mut self, key: String, bad: bool, delta: Duration, now: Instant) -> bool {
        if !bad {
            self.since.remove(&key);
            return false;
        }
        let first = *self.since.entry(key).or_insert(now);
        now.duration_since(first) > delta
    }

    /// I1 and I7: called after every actor step and on every poll of a wait.
    pub fn tick(&mut self, rig: &Rig, obs: &Obs, knobs: &Knobs) -> Result<(), Violation> {
        let now = Instant::now();
        let any_front = obs.snapshot.iter().any(|s| {
            matches!(
                s.stage,
                PendingStage::Blocked
                    | PendingStage::Running
                    | PendingStage::Preprocessing
                    | PendingStage::Parked
            )
        });
        for s in &obs.snapshot {
            let id = format!("{:?}", s.action_id);
            match s.stage {
                PendingStage::Blocked => {
                    let has_card = obs.cards.contains(&s.action_id)
                        || (s.kind == Some("transfer_shell_command_control_to_user")
                            && rig.lrc_alive);
                    if self.holds_for(format!("blocked:{id}"), !has_card, knobs.delta, now) {
                        return Err(violation(
                            "I1",
                            format!(
                                "action {id} ({}) is blocked with no actionable card for over {:?}",
                                s.kind.unwrap_or("?"),
                                knobs.delta
                            ),
                        ));
                    }
                }
                PendingStage::Preprocessing => {
                    if self.holds_for(format!("pre:{id}"), true, knobs.delta, now) {
                        return Err(violation("I1", format!("action {id} stuck preprocessing")));
                    }
                }
                PendingStage::Parked => {
                    // A Preprocessing action with no Pending batch ahead must become
                    // pending/blocked in the same event cascade; holding past delta is a
                    // lost hand-off (#1859).
                    if self.holds_for(format!("parked:{id}"), true, knobs.delta, now) {
                        return Err(violation(
                            "I1",
                            format!(
                                "action {id} finished preprocessing with no batch ahead but never became pending or blocked"
                            ),
                        ));
                    }
                }
                PendingStage::Queued => {
                    if self.holds_for(format!("queued:{id}"), !any_front, knobs.delta, now) {
                        return Err(violation(
                            "I1",
                            format!(
                                "action {id} is queued behind nothing: no front action is blocked, running or preprocessing"
                            ),
                        ));
                    }
                }
                PendingStage::Running => {}
            }
        }
        // I7, on the card's own key routing: a card may take accept keys only for an action the
        // model still holds. Once nothing is held, no card routes keys at all.
        for routed in &obs.routed {
            if !obs.snapshot.iter().any(|s| &s.action_id == routed) {
                return Err(violation(
                    "I7",
                    format!(
                        "a card still takes accept keys for {routed:?}, which is no longer pending (held: {:?})",
                        obs.snapshot
                    ),
                ));
            }
        }
        Ok(())
    }

    /// I2b: after a Ctrl-C / Stop.
    pub fn stopped(obs: &Obs) -> Result<(), Violation> {
        let mut problems = Vec::new();
        if !obs.is_terminal() {
            problems.push(format!("conversation is {:?}, not terminal", obs.status));
        }
        if !obs.snapshot.is_empty() {
            problems.push(format!(
                "{} action(s) still held: {:?}",
                obs.snapshot.len(),
                obs.snapshot
            ));
        }
        if obs.stream_in_flight {
            problems.push("a response stream is still in flight".to_owned());
        }
        if problems.is_empty() {
            Ok(())
        } else {
            Err(violation("I2b", problems.join("; ")))
        }
    }

    /// I2a: the script is used up and nobody owes the app an answer.
    pub fn settled_terminal(obs: &Obs) -> Result<(), Violation> {
        if obs.is_terminal() {
            Ok(())
        } else {
            Err(violation(
                "I2a",
                format!(
                    "script used up and every card answered, but the conversation is {:?} (held: {:?}, stream in flight: {})",
                    obs.status, obs.snapshot, obs.stream_in_flight
                ),
            ))
        }
    }

    /// I3, first half: no row is still locked.
    pub fn rows_unlocked(obs: &Obs) -> Result<(), Violation> {
        match obs.rows.iter().find(|row| {
            matches!(
                row.origin,
                QueuedQueryOrigin::PendingApprovalFollowUp | QueuedQueryOrigin::PendingLrcAutoQueue
            )
        }) {
            Some(row) => Err(violation(
                "I3",
                format!(
                    "queued row {:?} is still locked ({:?}) after the conversation ended",
                    row.text, row.origin
                ),
            )),
            None => Ok(()),
        }
    }

    /// I3, second half: after a clean finish, every unlocked row was sent (its text is the last
    /// user message of some request) or is gone.
    pub fn rows_sent(obs: &Obs, requests: &[RecordedRequest]) -> Result<(), Violation> {
        for row in &obs.rows {
            let sent = requests.iter().any(|r| {
                r.last_user_text()
                    .is_some_and(|t| t.contains(row.text.as_str()))
            });
            if !sent {
                return Err(violation(
                    "I3",
                    format!(
                        "unlocked queued row {:?} was never sent after the conversation succeeded",
                        row.text
                    ),
                ));
            }
        }
        Ok(())
    }
}

/// I4 over every request the model received.
pub fn check_tool_messages(
    requests: &[RecordedRequest],
    emitted: &[EmittedTurn],
) -> Result<(), Violation> {
    for request in requests {
        let mut seen_calls: Vec<&str> = Vec::new();
        let messages = &request.messages;
        let mut i = 0;
        while i < messages.len() {
            let message = &messages[i];
            if message.role == RecordedRole::Assistant && !message.tool_call_ids.is_empty() {
                let expected = &message.tool_call_ids;
                let mut distinct = expected.clone();
                distinct.sort();
                distinct.dedup();
                if distinct.len() != expected.len() {
                    return Err(violation(
                        "I4",
                        format!(
                            "request {}: assistant message repeats a call id: {expected:?}",
                            request.index
                        ),
                    ));
                }
                let mut j = i + 1;
                let mut got: Vec<&str> = Vec::new();
                while j < messages.len() && messages[j].role != RecordedRole::Assistant {
                    if messages[j].role == RecordedRole::Tool {
                        got.extend(messages[j].tool_response_ids.iter().map(String::as_str));
                    }
                    j += 1;
                }
                let want: Vec<&str> = expected.iter().map(String::as_str).collect();
                if got != want {
                    return Err(violation(
                        "I4",
                        format!(
                            "request {}: tool messages {got:?} do not answer the emitted calls {want:?} one-to-one in order",
                            request.index
                        ),
                    ));
                }
                seen_calls.extend(want);
            } else if message.role == RecordedRole::Tool {
                for id in &message.tool_response_ids {
                    if !seen_calls.contains(&id.as_str()) {
                        return Err(violation(
                            "I4",
                            format!(
                                "request {}: tool message for {id:?} answers no earlier call (orphan)",
                                request.index
                            ),
                        ));
                    }
                }
            }
            i += 1;
        }
    }

    // Cross-check the last request against what the script emitted, newest first. Ids the
    // provider omitted or left empty (the app invents them) are wildcards; repeats become `id#n`.
    let Some(last) = requests.last() else {
        return Ok(());
    };
    let in_request: Vec<&Vec<String>> = last
        .messages
        .iter()
        .filter(|m| m.role == RecordedRole::Assistant && !m.tool_call_ids.is_empty())
        .map(|m| &m.tool_call_ids)
        .collect();
    let scripted: Vec<&Vec<String>> = emitted
        .iter()
        .filter(|t| !t.aborted && !t.call_ids.is_empty())
        .map(|t| &t.call_ids)
        .collect();
    for (got, sent) in in_request.iter().rev().zip(scripted.iter().rev()) {
        let want = uniquify(sent);
        let ok =
            got.len() == want.len() && got.iter().zip(&want).all(|(g, w)| w.is_empty() || g == w);
        if !ok {
            return Err(violation(
                "I4",
                format!(
                    "the request holds calls {got:?}, but the model emitted {sent:?} (expected {want:?})"
                ),
            ));
        }
    }
    Ok(())
}

/// The app's duplicate rule, restated independently: a repeat of an id seen earlier in the same
/// reply becomes `<id>#2`, `#3`, ... (`chat_stream::unique_call_ids`).
fn uniquify(ids: &[String]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for id in ids {
        if id.is_empty() || !seen.contains(id) {
            seen.push(id.clone());
            continue;
        }
        let mut n = 2;
        loop {
            let candidate = format!("{id}#{n}");
            if !seen.contains(&candidate) {
                seen.push(candidate);
                break;
            }
            n += 1;
        }
    }
    seen
}

/// The ERROR lines one occurrence of a chaos kind may log, keyed by kind. A turn that fails on
/// purpose logs the open / stream failure and the recovery bookkeeping; anything beyond this is
/// a new ERROR and must be looked at (then either fixed or added here with its reason).
///
/// * `status`, `context_overflow`: "open stream failed" (+ one for the retry that hits it again)
/// * `disconnect`: the mid-stream failure, and the "ended without a terminal End" guard
/// * `stall`: the idle-timeout line, and the same guard
/// * `never_end`: the "ended without a terminal End" guard
/// * `split`, `delay`: not failures, no ERROR allowed
pub fn error_allowance(kind: &str) -> usize {
    match kind {
        "status" | "context_overflow" => 2,
        "disconnect" | "stall" => 2,
        "never_end" => 1,
        _ => 0,
    }
}

/// I6: ERROR lines since `mark`, against the per-chaos-kind allowance (kind x count of turns it
/// was in force for). A run with no failing chaos may log none. `panics` are panics seen on any
/// thread (`log_capture::PanicWatch`): never allowed.
pub fn check_logs(
    mark: Mark,
    emitted: &[EmittedTurn],
    capture_available: bool,
    panics: &[String],
) -> Result<(), Violation> {
    if !panics.is_empty() {
        return Err(violation("I6", format!("panic: {}", panics.join("; "))));
    }
    if !capture_available {
        return Ok(());
    }
    let allowance: usize = emitted
        .iter()
        .flat_map(|turn| turn.chaos.iter())
        .map(|kind| error_allowance(kind))
        .sum();
    let errors = log_capture::errors_since(mark);
    if errors.len() > allowance {
        return Err(violation(
            "I6",
            format!(
                "{} ERROR log line(s), {allowance} allowed by the chaos kinds in play ({:?}):\n  {}",
                errors.len(),
                emitted
                    .iter()
                    .flat_map(|t| t.chaos.iter())
                    .collect::<Vec<_>>(),
                errors.join("\n  ")
            ),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::agent_providers::fake_stream_seam::RecordedMessage;

    fn msg(role: RecordedRole, calls: &[&str], responses: &[&str]) -> RecordedMessage {
        RecordedMessage {
            role,
            text: String::new(),
            tool_call_ids: calls.iter().map(|s| s.to_string()).collect(),
            tool_response_ids: responses.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn request(messages: Vec<RecordedMessage>) -> RecordedRequest {
        RecordedRequest {
            index: 1,
            model_id: "m".into(),
            messages,
            tools_offered: Vec::new(),
        }
    }

    #[test]
    fn answered_calls_pass_and_a_missing_result_fails() {
        let ok = request(vec![
            msg(RecordedRole::User, &[], &[]),
            msg(RecordedRole::Assistant, &["a", "b"], &[]),
            msg(RecordedRole::Tool, &[], &["a"]),
            msg(RecordedRole::Tool, &[], &["b"]),
        ]);
        assert!(check_tool_messages(&[ok], &[]).is_ok());
        let missing = request(vec![
            msg(RecordedRole::User, &[], &[]),
            msg(RecordedRole::Assistant, &["a", "b"], &[]),
            msg(RecordedRole::Tool, &[], &["a"]),
            msg(RecordedRole::User, &[], &[]),
        ]);
        assert_eq!(
            check_tool_messages(&[missing], &[]).unwrap_err().invariant,
            "I4"
        );
    }

    #[test]
    fn an_orphan_and_a_reorder_fail() {
        let orphan = request(vec![
            msg(RecordedRole::User, &[], &[]),
            msg(RecordedRole::Tool, &[], &["zz"]),
        ]);
        assert!(check_tool_messages(&[orphan], &[]).is_err());
        let reordered = request(vec![
            msg(RecordedRole::Assistant, &["a", "b"], &[]),
            msg(RecordedRole::Tool, &[], &["b"]),
            msg(RecordedRole::Tool, &[], &["a"]),
        ]);
        assert!(check_tool_messages(&[reordered], &[]).is_err());
    }

    #[test]
    fn duplicates_are_renamed_like_the_app_does() {
        let ids: Vec<String> = ["c", "c", "c#2"].iter().map(|s| s.to_string()).collect();
        assert_eq!(uniquify(&ids), vec!["c", "c#2", "c#2#2"]);
    }
}
