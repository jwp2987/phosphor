//! The invariants, checked against the observable state of the rig (never against the code under
//! test's own bookkeeping where an independent view exists).
//!
//! | id | invariant |
//! |---|---|
//! | I1 | every blocked action has an actionable card within delta; nothing pending sits neither blocked-with-card, runnable nor running for longer than delta; every tool call of the last finished reply has an action-model status or a result within delta (#1946) |
//! | I2a | once the script is used up and the user has answered every card, the conversation is terminal within 5 s |
//! | I2b | Stop, and Ctrl-C, leave it terminal within 2 s, nothing pending / running, no stream in flight. The one exception is a Ctrl-C typed into a running agent command: that is the command's interrupt and the turn goes on (#850, owner decision 2026-11-10). A Ctrl-C with any other action running (a read, a grep) is a stop like any other |
//! | I2c | that Ctrl-C: the command it was typed into stops running within 2 s, and the action that waited behind it for the user's answer (a question; a command, with the allowlist off) is still held then: the key cancelled the command, not the turn. How the step ends afterwards is the ordinary end-of-step rule (results go back to the model when any result finished; a step whose only result is the interrupted command ends `Cancelled`, #1178); the cards that follow are the closing user's, and I2a still ends the turn |
//! | I3 | when the conversation is terminal, locked queued rows unlock within 2 s; unlocked rows of a conversation that succeeded are sent within 3 s (a cancel restores rows to the input instead: #690 / #1026, kept by #1799) |
//! | I4 | each request carries exactly one tool message per emitted call id, in order, none extra and none orphaned (duplicates renamed `id#2`); the last request holds the calls of the replies to the requests before it, except a reply whose stream the user cut (a message sent, Stop or Ctrl-C while it was still in flight): that one is whole or absent |
//! | I5 | `prompt_tests.rs`: a command is handed to the user only if it reads the tty |
//! | I6 | no panic (any thread) and no ERROR log beyond the allowance per chaos kind and count (`error_allowance`) |
//! | I7 | no card takes accept keys for an action that is done or cancelled (read from the card's own key routing, `AIBlock::pending_command_card_view_id`, not from the action status) |
//! | I8 | the model is asked only when it should be, counted on the requests the seam received (#1942, #1896, #1901): (a) after a Stop, or a Ctrl-C with no action running, no request is opened until the user acts again; (b) a message the user sent is the newest message of at most one request per submit (plus one per failed reply, which is retried), and a plain submit in a run without a stop reaches the model; (c) no request repeats an earlier one, or goes back to a prefix of it, unless a reply in between failed. A Ctrl-C typed into a running agent command is not a stop in (a) or in (b): the turn goes on, so its results may go back to the model and no message may be lost to it |

use std::collections::{BTreeSet, HashMap, HashSet};
use std::time::{Duration, Instant};

use fake_llm::scenario::{Reply, Scenario};

use crate::ai::agent_providers::fake_stream_seam::{RecordedRequest, RecordedRole};
use crate::ai::blocklist::QueuedQueryOrigin;
use crate::ai::blocklist::action_model::PendingStage;

use super::log_capture::{self, Mark};
use super::player::EmittedTurn;
use super::rig::{Obs, Rig, Submitted};

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
    /// I8a: while the user's stop stands, the most requests the seam may have received.
    request_limit: Option<usize>,
    /// I8a: one request that was sent before a stop but may not have reached the seam by then
    /// (a request is recorded when its stream opens, a moment after it is sent), carried over
    /// while it has not shown up.
    late_request_slack: usize,
    /// The tab was closed: the rig's close discards the finished results on purpose
    /// (`discard_pending_work_for_close`), so the "status or result" half of I1 no longer holds.
    tab_closed: bool,
    /// I4: requests whose reply stream the user cut (see [`Oracle::reply_cut`]).
    cut_replies: BTreeSet<usize>,
}

impl Oracle {
    /// I8a: the user pressed Stop (or Ctrl-C with no command running). `requests_before` and
    /// `stream_was_in_flight` are from just before the key. From here until the user acts again
    /// the model must not be asked anything: at most the one request that was already on its way.
    pub fn user_stopped(&mut self, requests_before: usize, stream_was_in_flight: bool) {
        if self.request_limit.is_some() {
            return;
        }
        let slack = usize::from(stream_was_in_flight).max(self.late_request_slack);
        self.request_limit = Some(requests_before + slack);
    }

    /// The user did something that may ask the model again (an answer to a card, a message):
    /// the stop no longer stands.
    pub fn user_acted(&mut self, requests_now: usize) {
        if let Some(limit) = self.request_limit.take() {
            self.late_request_slack = limit.saturating_sub(requests_now).min(1);
        }
    }

    pub fn tab_closed(&mut self) {
        self.tab_closed = true;
    }

    /// I4: the user sent a message, pressed Stop, or pressed Ctrl-C with no command running
    /// while the reply to request `request_index` was still in flight. That cancels the stream.
    /// A reply's tool calls are committed in one piece when the stream loop has gone through
    /// all of them (`chat_stream.rs`, the single `final_messages` yield), and a tool the app
    /// runs inside the stream (websearch, webfetch) awaits its network call before that, so a
    /// reply the script emitted in full may never have been committed. Later requests then hold
    /// nothing of it, or only the calls whose cards the app had already shown while the reply
    /// streamed ([`EmittedTurn::early_call_ids`]); that is the user's cancel, not a lost call.
    pub fn reply_cut(&mut self, request_index: usize) {
        self.cut_replies.insert(request_index);
    }

    /// The requests whose reply the user cut (for [`check_tool_messages`]).
    pub fn cut_replies(&self) -> &BTreeSet<usize> {
        &self.cut_replies
    }

    /// I8a, on every look.
    fn check_request_limit(&self, rig: &Rig) -> Result<(), Violation> {
        let Some(limit) = self.request_limit else {
            return Ok(());
        };
        if rig.seam.request_count() <= limit {
            return Ok(());
        }
        let requests = rig.seam.requests();
        let Some(extra) = requests.get(limit) else {
            return Ok(());
        };
        let newest = extra.newest_message();
        Err(violation(
            "I8",
            format!(
                "a request was opened after the user stopped the conversation: {} request(s), \
                 {limit} allowed; request {} ends with a {:?} message (tool results {:?})",
                requests.len(),
                extra.index,
                newest.map(|m| m.role),
                newest
                    .map(|m| m.tool_response_ids.clone())
                    .unwrap_or_default(),
            ),
        ))
    }

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
        // I1, the calls nothing holds (#1946): a reply that finished cleanly had tool calls, and
        // one of them is in no queue, has no card state and no result. While a stream is in
        // flight the calls of the reply before it are being answered (or its own are not queued
        // yet), so the clock only runs while nothing is streaming.
        let unsettled: HashSet<String> = if self.tab_closed || obs.stream_in_flight {
            HashSet::new()
        } else {
            obs.unsettled_calls
                .iter()
                .map(|id| format!("unsettled:{id:?}"))
                .collect()
        };
        self.since
            .retain(|key, _| !key.starts_with("unsettled:") || unsettled.contains(key));
        for key in unsettled {
            if self.holds_for(key.clone(), true, knobs.delta, now) {
                return Err(violation(
                    "I1",
                    format!(
                        "a tool call of the last reply ({key}) has no status in the action model \
                         and no result in the conversation for over {:?}: its card can never \
                         settle (conversation is {:?})",
                        knobs.delta, obs.status
                    ),
                ));
            }
        }
        // I8a.
        self.check_request_limit(rig)?;
        Ok(())
    }

    /// I2b: after a Stop, and after a Ctrl-C that is not I2c's.
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

    /// I2c, first half: after a Ctrl-C typed into a running agent command. That Ctrl-C is the
    /// command's interrupt, not a stop of the conversation (#850, owner decision 2026-11-10:
    /// "interrupts the command and the turn goes on"), so I2b does not apply; what must hold is
    /// that the interrupt took: of the commands that were running at the key, at least one no
    /// longer runs (a command runs alone, so that is the command). `was_running` is the
    /// commands the key was typed into (`Rig::running_commands`), from just before the key.
    /// The second half is [`Oracle::turn_goes_on`].
    pub fn interrupted(
        obs: &Obs,
        was_running: &[crate::ai::agent::AIAgentActionId],
    ) -> Result<(), Violation> {
        let still_running: Vec<_> = obs
            .snapshot
            .iter()
            .filter(|s| s.stage == PendingStage::Running && was_running.contains(&s.action_id))
            .map(|s| s.action_id.clone())
            .collect();
        if still_running.len() < was_running.len() {
            Ok(())
        } else {
            Err(violation(
                "I2c",
                format!(
                    "Ctrl-C into a running command interrupted nothing: {still_running:?} still \
                     running (conversation is {:?})",
                    obs.status
                ),
            ))
        }
    }

    /// I2c, second half: the turn goes on. `obs` is the look at which the interrupted command
    /// was seen to have stopped; `waited_for_the_user` is the action that waited behind it at
    /// the key and can only end by the user answering it ([`waits_for_the_user`]). Nobody has
    /// answered it since (the scripted user's next step comes after this check), so it must
    /// still be held: gone means the Ctrl-C cancelled the rest of the turn with the command,
    /// which is what a Stop does and what this key must not do.
    ///
    /// Nothing is asked when no such action waited: a command that ran alone ends its step by
    /// the ordinary rule (a request when an earlier result of the step finished, `Cancelled`
    /// with the reason shown when the interrupted command is all there is, #1178), and an
    /// action that needs no answer (a read) may have run and finished by now.
    pub fn turn_goes_on(
        obs: &Obs,
        waited_for_the_user: Option<&crate::ai::agent::AIAgentActionId>,
    ) -> Result<(), Violation> {
        let Some(waiting) = waited_for_the_user else {
            return Ok(());
        };
        if obs.snapshot.iter().any(|s| &s.action_id == waiting) {
            return Ok(());
        }
        Err(violation(
            "I2c",
            format!(
                "Ctrl-C into a running command also dropped {waiting:?}, which waited behind it \
                 for the user's answer: the key interrupts the command and the turn goes on \
                 (conversation is {:?}, held: {:?})",
                obs.status, obs.snapshot
            ),
        ))
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

/// For I2c ([`Oracle::turn_goes_on`]): the action next in line behind whatever runs now, when
/// it is one that only the user can end. `obs` is from just before the key.
///
/// * A question always waits for an answer.
/// * A command waits for approval with the safe allowlist off (the shipped default: every
///   command asks). With it on a listed command runs by itself, so it is not judged.
/// * Anything else (a read, a grep, a hand-over) may run and finish without the user.
///
/// Only the first queued action counts: the ones behind it wait for it, not for the command.
pub fn waits_for_the_user(
    obs: &Obs,
    safe_allowlist: bool,
) -> Option<crate::ai::agent::AIAgentActionId> {
    let next = obs
        .snapshot
        .iter()
        .find(|s| s.stage == PendingStage::Queued)?;
    let needs_the_user = match next.kind {
        Some("ask_user_question") => true,
        Some("run_shell_command") => !safe_allowlist,
        _ => false,
    };
    needs_the_user.then(|| next.action_id.clone())
}

/// I4 over every request the model received. `cut`: the requests whose reply stream the user
/// cut ([`Oracle::reply_cut`]).
pub fn check_tool_messages(
    requests: &[RecordedRequest],
    emitted: &[EmittedTurn],
    cut: &BTreeSet<usize>,
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
    //
    // Only the replies to EARLIER requests can be in the last request. The reply to the last
    // request itself is in no request at all: either it had no calls, or the turn ended before
    // its results went back (a Stop sends nothing, #1942; a card nobody answered). Zipping it
    // in shifted every pair by one and reported the last request as holding the wrong calls.
    let Some(last) = requests.last() else {
        return Ok(());
    };
    let in_request: Vec<&Vec<String>> = last
        .messages
        .iter()
        .filter(|m| m.role == RecordedRole::Assistant && !m.tool_call_ids.is_empty())
        .map(|m| &m.tool_call_ids)
        .collect();
    // The replies that can have put calls into the conversation: a finished reply with calls,
    // and a reply whose stream broke after it delivered calls the app shows early.
    let scripted: Vec<&EmittedTurn> = emitted
        .iter()
        .filter(|t| t.request_index < last.index)
        .filter(|t| {
            if t.aborted {
                !t.early_call_ids.is_empty()
            } else {
                !t.call_ids.is_empty()
            }
        })
        .collect();
    // Every reply must be in the request whole and in place, with two named exceptions, both
    // for a reply that never reached its commit:
    //
    // * a reply the user CUT (`cut`) is whole (the cancel came after the commit), or absent,
    //   or there with only its early calls;
    // * a reply whose stream BROKE (`aborted`: a disconnect, a stall, no terminal `End`) is
    //   absent, or there with only its early calls. It is never whole.
    //
    // "Its early calls" is a leading part of `EmittedTurn::early_call_ids`: the cards the app
    // showed while the reply streamed stay in the conversation, and the next request answers
    // them with a cancelled result. When the request's calls fit neither form, such a reply is
    // passed over and the same calls are held against the reply before it. A reply that is
    // neither cut nor aborted is never passed over, and calls that fit no reply at all fail.
    let mut scripted = scripted.into_iter().rev();
    for got in in_request.iter().rev() {
        let mut passed_over: Vec<&EmittedTurn> = Vec::new();
        loop {
            let Some(turn) = scripted.next() else {
                if passed_over.is_empty() {
                    // More call groups in the request than replies on record: not judged,
                    // as before.
                    return Ok(());
                }
                return Err(in_no_reply(got, &passed_over, cut));
            };
            let was_cut = cut.contains(&turn.request_index);
            if !turn.aborted && same_calls(got, &uniquify(&turn.call_ids)) {
                break;
            }
            if (turn.aborted || was_cut) && is_leading_part(got, &turn.early_call_ids) {
                break;
            }
            if !turn.aborted && !was_cut {
                return Err(wrong_calls(got, turn));
            }
            passed_over.push(turn);
        }
    }
    Ok(())
}

/// `got` is `want`, id for id ("" in `want` = an id the app invented).
fn same_calls(got: &[String], want: &[String]) -> bool {
    got.len() == want.len() && got.iter().zip(want).all(|(g, w)| w.is_empty() || g == w)
}

/// `got` is a non-empty leading part of `early` (or all of it).
fn is_leading_part(got: &[String], early: &[String]) -> bool {
    !got.is_empty() && got.len() <= early.len() && same_calls(got, &early[..got.len()])
}

/// The calls of a reply that finished and that the user did not cut are not in the request in
/// its place.
fn wrong_calls(got: &[String], turn: &EmittedTurn) -> Violation {
    violation(
        "I4",
        format!(
            "the request holds calls {got:?}, but the model emitted {:?} (expected {:?}) in its \
             reply to request {}, which finished and was not cut by the user",
            turn.call_ids,
            uniquify(&turn.call_ids),
            turn.request_index
        ),
    )
}

/// The request holds calls that fit none of the replies they were held against. Every reply in
/// `passed_over` (newest first) was cut or broke, and is named with the forms it may take, so
/// the message says by itself whether the cut was on record.
fn in_no_reply(got: &[String], passed_over: &[&EmittedTurn], cut: &BTreeSet<usize>) -> Violation {
    let replies: Vec<String> = passed_over
        .iter()
        .map(|turn| {
            if turn.aborted {
                format!(
                    "the reply to request {} (its stream broke: absent, or a leading part of \
                     the calls shown early {:?})",
                    turn.request_index, turn.early_call_ids
                )
            } else {
                debug_assert!(cut.contains(&turn.request_index));
                format!(
                    "the reply to request {} (cut by the user: whole {:?}, or absent, or a \
                     leading part of the calls shown early {:?})",
                    turn.request_index,
                    uniquify(&turn.call_ids),
                    turn.early_call_ids
                )
            }
        })
        .collect();
    violation(
        "I4",
        format!(
            "the request holds calls {got:?}, which are in no reply. Held against, newest \
             first: {}; no earlier reply is left",
            replies.join("; ")
        ),
    )
}

/// A user text I8b can follow through the requests: plain lowercase words, long enough not to
/// occur by chance. Anything else (empty, whitespace the input trims, quotes, a slash command) is
/// left to the other invariants.
fn trackable(text: &str) -> Option<&str> {
    let text = text.trim();
    let plain = text.chars().all(|c| c.is_ascii_lowercase() || c == ' ');
    (plain && text.len() >= 8 && !text.contains("  ")).then_some(text)
}

/// A reply with no text and no tool call: the app treats it like a failed one.
fn says_nothing(reply: &Reply) -> bool {
    match reply {
        Reply::Empty | Reply::Reasoning { .. } => true,
        Reply::Text { text } => text.trim().is_empty(),
        Reply::ToolCalls { calls } => calls.is_empty(),
        Reply::Mixed { text, calls } => text.trim().is_empty() && calls.is_empty(),
    }
}

/// I8b and I8c, over every request the model received, at the end of a case.
///
/// `submitted`: every message the user sent. `stopped`: the user pressed Stop / Ctrl-C or closed
/// the tab at some point (a message may then legitimately never reach the model: a stop discards
/// what was not sent yet).
pub fn check_requests(
    requests: &[RecordedRequest],
    emitted: &[EmittedTurn],
    scenario: &Scenario,
    submitted: &[Submitted],
    stopped: bool,
) -> Result<(), Violation> {
    // A reply that failed, never ended, or said nothing: the app may ask again with the same
    // request (the in-stream retry, the stream-level retry, a resume). A request with no record
    // of its reply is treated the same way.
    let reply_failed = |request_index: usize| {
        let mut answers = emitted
            .iter()
            .filter(|turn| turn.request_index == request_index)
            .peekable();
        answers.peek().is_none()
            || answers.any(|turn| {
                turn.aborted
                    || turn
                        .turn
                        .and_then(|k| scenario.turns.get(k))
                        .is_some_and(|t| says_nothing(&t.reply))
            })
    };

    // I8c: the conversation only moves forward. A request equal to an earlier one, or to a
    // prefix of it, asks the model the same thing again (a double send) or forgets messages.
    // Not judged for a request that ends with a text the user sent more than once: the user
    // repeating a message is the user's doing, and I8b counts those.
    let sent_more_than_once = |text: &str| {
        submitted
            .iter()
            .filter(|s| s.text.trim() == text.trim())
            .count()
            > 1
    };
    // Requests are compared without the environment tail (the `<environment_context>` block the
    // app appends as the last message of every request): it is not part of the conversation,
    // and with it in place a request that went back to a prefix was never seen as one.
    for (j, later) in requests.iter().enumerate() {
        let ends_with_a_repeated_message = later.newest_message().is_some_and(|newest| {
            newest.role == RecordedRole::User
                && submitted
                    .iter()
                    .any(|s| sent_more_than_once(&s.text) && newest.text.contains(s.text.trim()))
        });
        if ends_with_a_repeated_message {
            continue;
        }
        let later_messages = later.messages_without_environment_tail();
        for (i, earlier) in requests.iter().enumerate().take(j) {
            let earlier_messages = earlier.messages_without_environment_tail();
            let repeats = later_messages.len() <= earlier_messages.len()
                && earlier_messages[..later_messages.len()] == later_messages[..];
            if repeats && !(i..j).any(&reply_failed) {
                return Err(violation(
                    "I8",
                    format!(
                        "request {j} ({} message(s)) repeats request {i} ({} message(s)) or a \
                         prefix of it, and no reply in between failed: the model was asked the \
                         same thing twice",
                        later_messages.len(),
                        earlier_messages.len()
                    ),
                ));
            }
        }
    }

    // I8b: one submit, one request that ends with it ("ends with": its newest message, the
    // one before the environment tail. Read off the request's very last message, which is
    // always the tail, this half never judged a request of the rig).
    let texts: Vec<&str> = submitted
        .iter()
        .filter_map(|s| trackable(&s.text))
        .collect();
    let mut seen: Vec<&str> = Vec::new();
    for text in texts.iter().copied() {
        if seen.contains(&text) {
            continue;
        }
        seen.push(text);
        // Two different texts where one contains the other cannot be told apart.
        if texts
            .iter()
            .any(|other| *other != text && (other.contains(text) || text.contains(*other)))
        {
            continue;
        }
        let submits = texts.iter().filter(|other| **other == text).count();
        let ending_with: Vec<usize> = requests
            .iter()
            .filter(|request| {
                request
                    .newest_message()
                    .is_some_and(|m| m.role == RecordedRole::User && m.text.contains(text))
            })
            .map(|request| request.index)
            .collect();
        let retried = ending_with.iter().filter(|i| reply_failed(**i)).count();
        if ending_with.len() > submits + retried {
            return Err(violation(
                "I8",
                format!(
                    "the user sent {text:?} {submits} time(s), but {} requests end with it \
                     (requests {ending_with:?}; {retried} of their replies failed and were \
                     retried): one message opened more than one request",
                    ending_with.len()
                ),
            ));
        }
        // The lost message: a plain submit (not queued behind a turn) in a run nobody stopped
        // must be in some request.
        let plain_submit = submitted
            .iter()
            .any(|s| !s.queued && trackable(&s.text) == Some(text));
        let reached_the_model = requests.iter().any(|request| {
            request
                .messages
                .iter()
                .any(|m| m.role == RecordedRole::User && m.text.contains(text))
        });
        if plain_submit && !stopped && !reached_the_model {
            return Err(violation(
                "I8",
                format!(
                    "the user sent {text:?} and never stopped the conversation, but no request \
                     carries it: the message was lost"
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
        assert!(check_tool_messages(&[ok], &[], &BTreeSet::new()).is_ok());
        let missing = request(vec![
            msg(RecordedRole::User, &[], &[]),
            msg(RecordedRole::Assistant, &["a", "b"], &[]),
            msg(RecordedRole::Tool, &[], &["a"]),
            msg(RecordedRole::User, &[], &[]),
        ]);
        assert_eq!(
            check_tool_messages(&[missing], &[], &BTreeSet::new())
                .unwrap_err()
                .invariant,
            "I4"
        );
    }

    #[test]
    fn an_orphan_and_a_reorder_fail() {
        let orphan = request(vec![
            msg(RecordedRole::User, &[], &[]),
            msg(RecordedRole::Tool, &[], &["zz"]),
        ]);
        assert!(check_tool_messages(&[orphan], &[], &BTreeSet::new()).is_err());
        let reordered = request(vec![
            msg(RecordedRole::Assistant, &["a", "b"], &[]),
            msg(RecordedRole::Tool, &[], &["b"]),
            msg(RecordedRole::Tool, &[], &["a"]),
        ]);
        assert!(check_tool_messages(&[reordered], &[], &BTreeSet::new()).is_err());
    }

    /// The cross-check lines the last request up with the replies to the requests BEFORE it.
    /// The reply to the last request is in no request (here: the user stopped the turn before
    /// its results went back, #1942). Fails on the old check, which zipped that reply in and
    /// said "the request holds [c0], but the model emitted [c1]".
    #[test]
    fn the_reply_to_the_last_request_is_not_expected_in_it() {
        let emitted_for = |request_index: usize, call: &str| EmittedTurn {
            request_index,
            turn: Some(request_index),
            call_ids: vec![call.to_owned()],
            chaos: Vec::new(),
            aborted: false,
            early_call_ids: vec![call.to_owned()],
        };
        let requests = vec![
            RecordedRequest {
                index: 0,
                model_id: "m".into(),
                messages: vec![msg(RecordedRole::User, &[], &[])],
                tools_offered: Vec::new(),
            },
            RecordedRequest {
                index: 1,
                model_id: "m".into(),
                messages: vec![
                    msg(RecordedRole::User, &[], &[]),
                    msg(RecordedRole::Assistant, &["c0"], &[]),
                    msg(RecordedRole::Tool, &[], &["c0"]),
                ],
                tools_offered: Vec::new(),
            },
        ];
        let emitted = [emitted_for(0, "c0"), emitted_for(1, "c1")];
        assert!(check_tool_messages(&requests, &emitted, &BTreeSet::new()).is_ok());

        // Still caught: the last request holds a call the earlier reply never made.
        let wrong = [emitted_for(0, "other"), emitted_for(1, "c1")];
        assert_eq!(
            check_tool_messages(&requests, &wrong, &BTreeSet::new())
                .unwrap_err()
                .invariant,
            "I4"
        );
    }

    /// The round 10 failure of both paced fuzz tests (#2010): request 0's reply runs a command,
    /// the user sends a message (request 1), the reply to it asks for a websearch, and the user
    /// sends another message while the app is still running that search inside the stream. The
    /// second message cancels the stream before the reply's calls are committed, so request 2
    /// holds only the first reply's call. Fails on the check as it was ("the request holds
    /// [c0], but the model emitted [c1]").
    #[test]
    fn a_reply_the_user_cut_may_be_absent_from_later_requests_and_no_other_reply_may() {
        let emitted_for = |request_index: usize, call: &str| EmittedTurn {
            request_index,
            turn: Some(request_index),
            call_ids: vec![call.to_owned()],
            chaos: Vec::new(),
            aborted: false,
            early_call_ids: vec![call.to_owned()],
        };
        let last = |calls: &[&str]| {
            let mut messages = vec![msg(RecordedRole::User, &[], &[])];
            for call in calls {
                messages.push(msg(RecordedRole::Assistant, &[*call], &[]));
                messages.push(msg(RecordedRole::Tool, &[], &[*call]));
            }
            messages.push(msg(RecordedRole::User, &[], &[]));
            vec![RecordedRequest {
                index: 2,
                model_id: "m".into(),
                messages,
                tools_offered: Vec::new(),
            }]
        };
        let emitted = [
            emitted_for(0, "c0"),
            emitted_for(1, "c1"),
            emitted_for(2, "c2"),
        ];
        let cut_1 = BTreeSet::from([1]);
        let none = BTreeSet::new();

        // The cut reply is absent: fine. It is whole: fine too (the cancel came after the commit).
        assert!(check_tool_messages(&last(&["c0"]), &emitted, &cut_1).is_ok());
        assert!(check_tool_messages(&last(&["c0", "c1"]), &emitted, &cut_1).is_ok());

        // Not cut: its absence is a lost reply, as before.
        let err = check_tool_messages(&last(&["c0"]), &emitted, &none).unwrap_err();
        assert_eq!(err.invariant, "I4");
        assert!(err.detail.contains("\"c1\""), "{}", err.detail);

        // A cut excuses that reply only. Request 3 after replies 0, 1 (cut) and 2: without
        // reply 1 it passes, without reply 2 it fails.
        let request_3 = |calls: &[&str]| {
            let mut requests = last(calls);
            requests[0].index = 3;
            requests
        };
        assert!(check_tool_messages(&request_3(&["c0", "c2"]), &emitted, &cut_1).is_ok());
        let err = check_tool_messages(&request_3(&["c0", "c1"]), &emitted, &cut_1).unwrap_err();
        assert_eq!(err.invariant, "I4");
        assert!(err.detail.contains("\"c2\""), "{}", err.detail);

        // Calls that match no reply fail even when every reply before them was cut.
        let all_cut = BTreeSet::from([0, 1]);
        let err = check_tool_messages(&last(&["zz"]), &emitted, &all_cut).unwrap_err();
        assert_eq!(err.invariant, "I4");
        assert!(err.detail.contains("\"zz\""), "{}", err.detail);
    }

    /// One reply, as the player records it: `calls` on the wire, `early` the leading run the
    /// app may show before the commit, `aborted` = the stream broke (no calls committed).
    fn reply(request_index: usize, calls: &[&str], early: &[&str], aborted: bool) -> EmittedTurn {
        let owned = |ids: &[&str]| ids.iter().map(|id| id.to_string()).collect::<Vec<_>>();
        EmittedTurn {
            request_index,
            turn: Some(request_index),
            call_ids: if aborted { Vec::new() } else { owned(calls) },
            chaos: Vec::new(),
            aborted,
            early_call_ids: owned(early),
        }
    }

    /// A request with one assistant message per group of calls, each answered in order.
    fn request_holding(index: usize, groups: &[&[&str]]) -> Vec<RecordedRequest> {
        let mut messages = vec![msg(RecordedRole::User, &[], &[])];
        for group in groups {
            messages.push(msg(RecordedRole::Assistant, group, &[]));
            for call in *group {
                messages.push(msg(RecordedRole::Tool, &[], &[*call]));
            }
        }
        messages.push(msg(RecordedRole::User, &[], &[]));
        vec![RecordedRequest {
            index,
            model_id: "m".into(),
            messages,
            tools_offered: Vec::new(),
        }]
    }

    /// The round 11 failure of both paced fuzz tests (build/r11a, the round 10 message again):
    /// the reply to request 0 delivers one command call and the connection drops. The app had
    /// shown the call's card, so the call stays in the conversation and is answered as
    /// cancelled. The user sends a message (request 1), its reply asks for a websearch, and a
    /// second message cuts that reply while the search is out. Request 2 then holds the call
    /// of the BROKEN reply and nothing of the cut one. Fails on the check as it was: it kept
    /// no record of what a broken reply delivered, passed over the cut reply, found no reply
    /// left and said "the request holds [c0], but the model emitted [c1]".
    #[test]
    fn a_broken_reply_leaves_the_calls_it_showed_early_and_nothing_else() {
        let emitted = [
            reply(0, &["c0"], &["c0"], true),
            reply(1, &["c1"], &[], false),
        ];
        let cut_1 = BTreeSet::from([1]);
        let none = BTreeSet::new();
        assert!(check_tool_messages(&request_holding(2, &[&["c0"]]), &emitted, &cut_1).is_ok());
        // The search reply made it in after all (the cut came after its commit).
        assert!(
            check_tool_messages(&request_holding(2, &[&["c0"], &["c1"]]), &emitted, &cut_1).is_ok()
        );

        // Still a lost reply when the user did not cut reply 1.
        let err =
            check_tool_messages(&request_holding(2, &[&["c0"]]), &emitted, &none).unwrap_err();
        assert_eq!(err.invariant, "I4");
        assert!(err.detail.contains("\"c1\""), "{}", err.detail);

        // A broken reply that delivered no call explains nothing: the same request fails, and
        // the message names the replies it was held against.
        let delivered_nothing = [reply(0, &["c0"], &[], true), reply(1, &["c1"], &[], false)];
        let err = check_tool_messages(&request_holding(2, &[&["c0"]]), &delivered_nothing, &cut_1)
            .unwrap_err();
        assert_eq!(err.invariant, "I4");
        assert!(err.detail.contains("in no reply"), "{}", err.detail);
        assert!(err.detail.contains("cut by the user"), "{}", err.detail);

        // A broken reply between two finished ones: a leading part of what it delivered may sit
        // between their calls (it used to shift the pairs and fail), or nothing; a call it
        // delivered LATER without the ones before it may not, and it is never there whole
        // beyond what it showed early.
        let emitted = [
            reply(0, &["a"], &["a"], false),
            reply(1, &["p", "q", "w"], &["p", "q"], true),
            reply(2, &["b"], &["b"], false),
        ];
        for ok in [
            &[&["a"][..], &["p", "q"][..], &["b"][..]][..],
            &[&["a"][..], &["p"][..], &["b"][..]][..],
            &[&["a"][..], &["b"][..]][..],
        ] {
            assert!(
                check_tool_messages(&request_holding(3, ok), &emitted, &none).is_ok(),
                "{ok:?}"
            );
        }
        for bad in [
            &[&["a"][..], &["q"][..], &["b"][..]][..],
            &[&["a"][..], &["p", "q", "w"][..], &["b"][..]][..],
            &[&["a"][..], &["zz"][..], &["b"][..]][..],
        ] {
            let err = check_tool_messages(&request_holding(3, bad), &emitted, &none).unwrap_err();
            assert_eq!(err.invariant, "I4", "{bad:?}");
        }
        // The finished replies around it are held as strictly as before.
        let err = check_tool_messages(
            &request_holding(3, &[&["a"], &["p"], &["other"]]),
            &emitted,
            &none,
        )
        .unwrap_err();
        assert!(err.detail.contains("\"b\""), "{}", err.detail);
    }

    /// A reply the user cut may also be there in part: the cards shown while it streamed stay
    /// (a command call's card, then the cut lands while the reply's websearch is out). Only a
    /// leading part of the early calls, and only for a reply that was cut.
    #[test]
    fn a_cut_reply_may_be_there_with_only_the_calls_it_showed_early() {
        let emitted = [
            reply(0, &["c0"], &["c0"], false),
            reply(1, &["x", "w"], &["x"], false),
        ];
        let cut_1 = BTreeSet::from([1]);
        let none = BTreeSet::new();
        for ok in [
            &[&["c0"][..], &["x"][..]][..],
            &[&["c0"][..], &["x", "w"][..]][..],
            &[&["c0"][..]][..],
        ] {
            assert!(
                check_tool_messages(&request_holding(2, ok), &emitted, &cut_1).is_ok(),
                "{ok:?}"
            );
        }
        // The search call without the command call before it is not a form a cut leaves.
        let err = check_tool_messages(&request_holding(2, &[&["c0"], &["w"]]), &emitted, &cut_1)
            .unwrap_err();
        assert_eq!(err.invariant, "I4");
        // Not cut: in part is a lost call.
        let err = check_tool_messages(&request_holding(2, &[&["c0"], &["x"]]), &emitted, &none)
            .unwrap_err();
        assert_eq!(err.invariant, "I4");
        assert!(err.detail.contains("\"w\""), "{}", err.detail);
    }

    #[test]
    fn i2c_holds_once_the_interrupted_action_no_longer_runs() {
        use crate::ai::agent::AIAgentActionId;
        use crate::ai::agent::conversation::AIConversationId;
        use crate::ai::blocklist::action_model::PendingActionSnapshot;

        let conversation_id = AIConversationId::new();
        let held = |id: &str, stage: PendingStage| PendingActionSnapshot {
            conversation_id,
            action_id: AIAgentActionId::from(id.to_owned()),
            kind: Some("run_shell_command"),
            stage,
        };
        let obs = |snapshot: Vec<PendingActionSnapshot>| Obs {
            status: None,
            stream_in_flight: false,
            snapshot,
            cards: Vec::new(),
            routed: Vec::new(),
            rows: Vec::new(),
            unsettled_calls: Vec::new(),
        };
        let was_running = [AIAgentActionId::from("c0".to_owned())];
        // Still running: the interrupt did nothing.
        let stuck = obs(vec![
            held("c0", PendingStage::Running),
            held("c1", PendingStage::Queued),
        ]);
        assert_eq!(
            Oracle::interrupted(&stuck, &was_running)
                .unwrap_err()
                .invariant,
            "I2c"
        );
        // Interrupted; the next action of the reply shows its card. Not I2b's business.
        let moved_on = obs(vec![held("c1", PendingStage::Blocked)]);
        assert!(Oracle::interrupted(&moved_on, &was_running).is_ok());
        assert!(Oracle::stopped(&moved_on).is_err());
    }

    /// Owner decision 2026-11-10 (#850, the question left open on #1942): a Ctrl-C typed into a
    /// running agent command interrupts the command and the turn goes on. I2c's second half
    /// holds the turn to that: the action that waited behind the command for the user's answer
    /// is still held when the command has stopped. A turn the key cancelled whole (what a Stop
    /// does) passes I2b and the first half of I2c, and fails here.
    #[test]
    fn i2c_the_turn_goes_on_after_a_ctrl_c_into_a_running_command() {
        use crate::ai::agent::AIAgentActionId;
        use crate::ai::agent::conversation::{AIConversationId, ConversationStatus};
        use crate::ai::blocklist::action_model::PendingActionSnapshot;

        let conversation_id = AIConversationId::new();
        let held =
            |id: &str, kind: Option<&'static str>, stage: PendingStage| PendingActionSnapshot {
                conversation_id,
                action_id: AIAgentActionId::from(id.to_owned()),
                kind,
                stage,
            };
        let obs = |status: Option<ConversationStatus>, snapshot: Vec<PendingActionSnapshot>| Obs {
            status,
            stream_in_flight: false,
            snapshot,
            cards: Vec::new(),
            routed: Vec::new(),
            rows: Vec::new(),
            unsettled_calls: Vec::new(),
        };
        let id = |name: &str| AIAgentActionId::from(name.to_owned());
        let was_running = [id("c0")];

        // At the key: the command runs, a question waits behind it.
        let before = obs(
            Some(ConversationStatus::InProgress),
            vec![
                held("q1", Some("ask_user_question"), PendingStage::Queued),
                held("c0", None, PendingStage::Running),
            ],
        );
        let waiting = waits_for_the_user(&before, false);
        assert_eq!(waiting, Some(id("q1")));

        // The turn went on: the command is gone, the question shows its card.
        let went_on = obs(
            Some(ConversationStatus::Blocked {
                blocked_action: String::new(),
            }),
            vec![held("q1", Some("ask_user_question"), PendingStage::Blocked)],
        );
        assert!(Oracle::interrupted(&went_on, &was_running).is_ok());
        assert!(Oracle::turn_goes_on(&went_on, waiting.as_ref()).is_ok());

        // The key stopped the whole turn: the command is gone and so is the question. I2b would
        // call this fine, and so would the first half of I2c.
        let stopped = obs(Some(ConversationStatus::Cancelled), Vec::new());
        assert!(Oracle::stopped(&stopped).is_ok());
        assert!(Oracle::interrupted(&stopped, &was_running).is_ok());
        let broken = Oracle::turn_goes_on(&stopped, waiting.as_ref()).unwrap_err();
        assert_eq!(broken.invariant, "I2c");
        assert!(broken.detail.contains("q1"), "{}", broken.detail);

        // A command that ran alone: nothing waited, so nothing is asked of the turn here (its
        // step ends by the ordinary rule, and I2a still has to see it end).
        let alone = obs(
            Some(ConversationStatus::InProgress),
            vec![held("c0", None, PendingStage::Running)],
        );
        assert_eq!(waits_for_the_user(&alone, false), None);
        assert!(Oracle::turn_goes_on(&stopped, None).is_ok());

        // What waits for the user: a question always, a command only while every command asks
        // (the allowlist off), a read never. Only the first in line counts.
        let command_next = obs(
            None,
            vec![
                held("c1", Some("run_shell_command"), PendingStage::Queued),
                held("q2", Some("ask_user_question"), PendingStage::Queued),
                held("c0", None, PendingStage::Running),
            ],
        );
        assert_eq!(waits_for_the_user(&command_next, false), Some(id("c1")));
        assert_eq!(waits_for_the_user(&command_next, true), None);
        let read_next = obs(
            None,
            vec![
                held("r1", Some("read_files"), PendingStage::Queued),
                held("q2", Some("ask_user_question"), PendingStage::Queued),
                held("c0", None, PendingStage::Running),
            ],
        );
        assert_eq!(waits_for_the_user(&read_next, false), None);
    }

    /// I8b and I8c look past the environment tail the app appends to every request. With the
    /// tail in place the old checks read it as the newest message: a message that opened two
    /// requests, and a request that went back to a prefix, both passed.
    #[test]
    fn i8_is_judged_on_the_messages_before_the_environment_tail() {
        let scenario = Scenario::new(1);
        let tail =
            || user("<environment_context>\n  Today's date: 2026-11-09\n</environment_context>");
        let assistant = msg(RecordedRole::Assistant, &[], &[]);
        let submitted = [sent(START, false), sent(MORE, false)];

        let ok = numbered(vec![
            vec![user(START), tail()],
            vec![user(START), assistant.clone(), user(MORE), tail()],
        ]);
        assert!(check_requests(&ok, &replies(2, &[]), &scenario, &submitted, false).is_ok());

        // One message, two requests (I8b).
        let twice = numbered(vec![
            vec![user(START), tail()],
            vec![user(START), assistant.clone(), user(MORE), tail()],
            vec![
                user(START),
                assistant.clone(),
                msg(RecordedRole::Assistant, &[], &[]),
                user(MORE),
                tail(),
            ],
        ]);
        let broken =
            check_requests(&twice, &replies(3, &[]), &scenario, &submitted, false).unwrap_err();
        assert_eq!(broken.invariant, "I8");
        assert!(
            broken.detail.contains("more than one request"),
            "{}",
            broken.detail
        );

        // Back to a prefix of an earlier request (I8c).
        let only_start = [sent(START, false)];
        let backwards = numbered(vec![
            vec![user(START), tail()],
            vec![
                user(START),
                msg(RecordedRole::Assistant, &["a"], &[]),
                msg(RecordedRole::Tool, &[], &["a"]),
                tail(),
            ],
            vec![
                user(START),
                msg(RecordedRole::Assistant, &["a"], &[]),
                tail(),
            ],
        ]);
        let broken = check_requests(&backwards, &replies(3, &[]), &scenario, &only_start, false)
            .unwrap_err();
        assert_eq!(broken.invariant, "I8");
        assert!(broken.detail.contains("repeats"), "{}", broken.detail);
    }

    #[test]
    fn duplicates_are_renamed_like_the_app_does() {
        let ids: Vec<String> = ["c", "c", "c#2"].iter().map(|s| s.to_string()).collect();
        assert_eq!(uniquify(&ids), vec!["c", "c#2", "c#2#2"]);
    }

    // ---- I8 --------------------------------------------------------------------------------

    fn user(text: &str) -> RecordedMessage {
        RecordedMessage {
            role: RecordedRole::User,
            text: text.to_owned(),
            tool_call_ids: Vec::new(),
            tool_response_ids: Vec::new(),
        }
    }

    fn numbered(messages: Vec<Vec<RecordedMessage>>) -> Vec<RecordedRequest> {
        messages
            .into_iter()
            .enumerate()
            .map(|(index, messages)| RecordedRequest {
                index,
                model_id: "m".into(),
                messages,
                tools_offered: Vec::new(),
            })
            .collect()
    }

    /// One delivered reply per request (the implicit `END`), except the listed failed ones.
    fn replies(count: usize, failed: &[usize]) -> Vec<EmittedTurn> {
        (0..count)
            .map(|request_index| EmittedTurn {
                request_index,
                turn: None,
                call_ids: Vec::new(),
                chaos: Vec::new(),
                aborted: failed.contains(&request_index),
                early_call_ids: Vec::new(),
            })
            .collect()
    }

    fn sent(text: &str, queued: bool) -> Submitted {
        Submitted {
            text: text.to_owned(),
            queued,
        }
    }

    const START: &str = "start the task";
    const MORE: &str = "and then list the files";

    #[test]
    fn i8_a_stop_freezes_the_request_count_until_the_user_acts() {
        let mut oracle = Oracle::default();
        // Nothing was streaming: not one more request.
        oracle.user_stopped(3, false);
        assert_eq!(oracle.request_limit, Some(3));
        // A second stop does not move the line.
        oracle.user_stopped(5, true);
        assert_eq!(oracle.request_limit, Some(3));
        oracle.user_acted(3);
        assert_eq!(oracle.request_limit, None);
        assert_eq!(oracle.late_request_slack, 0);
        // A stream was in flight: its own request may still reach the seam, once.
        oracle.user_stopped(4, true);
        assert_eq!(oracle.request_limit, Some(5));
        // It never showed up: the allowance carries to the next stop.
        oracle.user_acted(4);
        assert_eq!(oracle.late_request_slack, 1);
        oracle.user_stopped(6, false);
        assert_eq!(oracle.request_limit, Some(7));
    }

    /// A Reject is not a stop (#1942, owner decision 2026-11-09): rejecting a card after an
    /// earlier result of the step finished sends the results to the model at once. The runner
    /// reports a Reject as the user acting, never as a stop, so it draws no line of its own and
    /// lifts one an earlier Stop drew; the follow-up it opens is not an I8a finding.
    #[test]
    fn i8_a_reject_never_freezes_the_request_count() {
        let mut oracle = Oracle::default();
        // A Reject on its own: no line.
        oracle.user_acted(2);
        assert_eq!(oracle.request_limit, None);
        assert_eq!(oracle.late_request_slack, 0);
        // A Stop, then a Reject of a later card: the stop no longer stands.
        oracle.user_stopped(2, false);
        assert_eq!(oracle.request_limit, Some(2));
        oracle.user_acted(2);
        assert_eq!(oracle.request_limit, None);
    }

    #[test]
    fn i8_one_message_one_request() {
        let scenario = Scenario::new(1);
        let submitted = [sent(START, false), sent(MORE, false)];
        let assistant = msg(RecordedRole::Assistant, &[], &[]);
        // The ordinary run: each message ends exactly one request.
        let ok = numbered(vec![
            vec![user(START)],
            vec![user(START), assistant.clone(), user(MORE)],
        ]);
        assert!(check_requests(&ok, &replies(2, &[]), &scenario, &submitted, false).is_ok());

        // The follow-up opened two requests (the second with one more message, so it is not a
        // plain repeat): one message, two requests.
        let twice = numbered(vec![
            vec![user(START)],
            vec![user(START), assistant.clone(), user(MORE)],
            vec![
                user(START),
                assistant.clone(),
                msg(RecordedRole::Assistant, &[], &[]),
                user(MORE),
            ],
        ]);
        let broken =
            check_requests(&twice, &replies(3, &[]), &scenario, &submitted, false).unwrap_err();
        assert_eq!(broken.invariant, "I8");
        // ... unless the first of them failed and was retried.
        assert!(check_requests(&twice, &replies(3, &[1]), &scenario, &submitted, false).is_ok());
        // ... or the user really sent it twice.
        let sent_twice = [sent(START, false), sent(MORE, false), sent(MORE, false)];
        assert!(check_requests(&twice, &replies(3, &[]), &scenario, &sent_twice, false).is_ok());
    }

    #[test]
    fn i8_a_repeated_request_fails_unless_a_reply_in_between_failed() {
        let scenario = Scenario::new(1);
        let submitted = [sent(START, false)];
        let tool_step = vec![
            user(START),
            msg(RecordedRole::Assistant, &["a"], &[]),
            msg(RecordedRole::Tool, &[], &["a"]),
        ];
        let repeated = numbered(vec![
            vec![user(START)],
            tool_step.clone(),
            tool_step.clone(),
        ]);
        let broken =
            check_requests(&repeated, &replies(3, &[]), &scenario, &submitted, false).unwrap_err();
        assert_eq!(broken.invariant, "I8");
        assert!(broken.detail.contains("repeats"), "{}", broken.detail);
        // A retry after a failed reply is the same request again, by design.
        assert!(check_requests(&repeated, &replies(3, &[1]), &scenario, &submitted, false).is_ok());
        // Going back to a prefix of an earlier request is a repeat too.
        let backwards = numbered(vec![
            vec![user(START)],
            tool_step.clone(),
            vec![user(START)],
        ]);
        assert!(
            check_requests(&backwards, &replies(3, &[]), &scenario, &submitted, false).is_err()
        );
    }

    #[test]
    fn i8_a_plain_submit_that_reaches_no_request_is_a_lost_message() {
        let scenario = Scenario::new(1);
        let requests = numbered(vec![vec![user(START)]]);
        let emitted = replies(1, &[]);
        let lost = [sent(START, false), sent(MORE, false)];
        let broken = check_requests(&requests, &emitted, &scenario, &lost, false).unwrap_err();
        assert_eq!(broken.invariant, "I8");
        assert!(broken.detail.contains("lost"), "{}", broken.detail);
        // A stop may discard a message that was not sent yet; a queued one is handed back to the
        // input when its turn is cancelled (I3 covers the clean finish).
        assert!(check_requests(&requests, &emitted, &scenario, &lost, true).is_ok());
        let queued = [sent(START, false), sent(MORE, true)];
        assert!(check_requests(&requests, &emitted, &scenario, &queued, false).is_ok());
        // Texts the input may rewrite (trimmed, quoted, empty) are not followed.
        let odd = [
            sent(START, false),
            sent("  ", false),
            sent("say \"hi\"", false),
        ];
        assert!(check_requests(&requests, &emitted, &scenario, &odd, false).is_ok());
    }
}
