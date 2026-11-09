//! Runs one [`Scenario`] against the rig and checks the oracle after every step.
//!
//! One case = one `App::test`. Everything the case needs (model id, seam script, log mark) is
//! per-case, so cases never share state, in this process or across nextest processes.

use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use fake_llm::scenario::{Op, Scenario};
use warpui::App;

use crate::ai::agent::AIAgentActionType;
use crate::ai::agent::conversation::ConversationStatus;
use crate::ai::blocklist::action_model::{PendingStage, fuzz_action_kind};
use crate::features::FeatureFlag;

use super::actor::{self, StepRecord, op_name, timing_name};
use super::log_capture;
use super::oracle::{self, Knobs, Oracle, Violation};
use super::rig::{self, Obs, Rig, pump_for, settle_until};

/// What a passing case did, for the coverage table.
#[derive(Debug, Default, Clone)]
pub struct CaseReport {
    pub steps: Vec<StepRecord>,
    /// Chaos kinds of the turns that answered a request, with the index of that request.
    pub chaos: Vec<(&'static str, usize)>,
    pub requests: usize,
    /// The conversation's status when the case ended.
    pub final_status: Option<ConversationStatus>,
    /// Tool messages (one per answered call) in the last request the model received.
    pub tool_messages_in_last_request: usize,
}

/// A failed case: the broken invariant and the scenario that breaks it, as schema JSON so the
/// failure message is a replayable corpus entry.
#[derive(Debug, Clone)]
pub struct Failure {
    pub violation: Violation,
    pub scenario_json: String,
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "agent fuzz invariant broken: {}\nscenario (save as agent_fuzz/corpus/<name>.json to replay):\n{}",
            self.violation, self.scenario_json
        )
    }
}

/// Whether the idle timeouts are short enough for a `stall` case to end in test time. The script
/// sets both; a bare `cargo test` does not, and the strategies then leave stalls out.
pub fn stall_cases_supported() -> bool {
    let short = |name: &str| {
        std::env::var(name)
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .is_some_and(|secs| secs <= 3)
    };
    short("PHOSPHOR_BYOP_STREAM_IDLE_TIMEOUT_SECS")
        && short("PHOSPHOR_BYOP_STREAM_FIRST_EVENT_TIMEOUT_SECS")
}

/// Runs `scenario` with the safe read-only allowlist OFF (the shipped default; the gate).
pub fn run_case(scenario: &Scenario) -> Result<CaseReport, Failure> {
    run_case_with(scenario, false)
}

/// Runs `scenario`; panics inside the app become an I6 violation. `safe_allowlist` turns the
/// safe read-only command allowlist ON (the scenario's `fake-cmd cN` and `echo` lines then run
/// without an approval card).
pub fn run_case_with(scenario: &Scenario, safe_allowlist: bool) -> Result<CaseReport, Failure> {
    let result: Arc<Mutex<Option<Result<CaseReport, Violation>>>> = Arc::default();
    let slot = result.clone();
    let owned = scenario.clone();
    let outcome = std::panic::catch_unwind(AssertUnwindSafe(move || {
        App::test((), |mut app| async move {
            let verdict = drive(&mut app, &owned, safe_allowlist).await;
            *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(verdict);
        });
    }));
    let verdict = match outcome {
        Ok(()) => result
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .unwrap_or_else(|| {
                Err(Violation {
                    invariant: "I6",
                    detail: "the case produced no verdict".to_owned(),
                })
            }),
        Err(payload) => Err(Violation {
            invariant: "I6",
            detail: format!("panic: {}", panic_text(payload.as_ref())),
        }),
    };
    verdict.map_err(|violation| Failure {
        violation,
        scenario_json: scenario.to_json(),
    })
}

fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "<non-string panic>".to_owned())
}

async fn drive(
    app: &mut App,
    scenario: &Scenario,
    safe_allowlist: bool,
) -> Result<CaseReport, Violation> {
    // The input queues a follow-up behind a running or blocked turn only with these on.
    let _queue_slash = FeatureFlag::QueueSlashCommand.override_enabled(true);
    let _queue_v2 = FeatureFlag::QueuedPromptsV2.override_enabled(true);
    let watch = log_capture::PanicWatch::start();
    let knobs = Knobs::from_env();
    let capture = log_capture::install();
    let mark = log_capture::mark();
    let mut rig = rig::setup(app, scenario, safe_allowlist);
    let mut oracle = Oracle::default();
    let mut report = CaseReport::default();

    rig.send(app, "start the task");
    watch(
        app,
        &mut rig,
        &mut oracle,
        &knobs,
        Duration::from_millis(80),
    )
    .await?;

    for step in &scenario.actor {
        let triggered = actor::await_trigger(app, &mut rig, step.timing, &knobs).await;
        let mut record = StepRecord {
            op: op_name(&step.op),
            timing: timing_name(step.timing),
            tool: None,
            stage: None,
            performed: triggered,
        };
        if triggered {
            rig.note_commands(app);
            let obs = rig.observe(app);
            record.stage = obs.snapshot.first().map(|s| stage_name(s.stage));
            record.tool = rig.front_action(app).map(|a| fuzz_action_kind(&a.action));
            actor::perform(app, &mut rig, &step.op);
            if let Op::Wait { ms } = &step.op {
                pump_for(app, knobs.scale(Duration::from_millis(*ms))).await;
            }
            if matches!(step.op, Op::CtrlC | Op::Stop) {
                let stopped = settle_until(app, knobs.terminal_after_stop, |app| {
                    Oracle::stopped(&rig.observe(app)).is_ok()
                })
                .await;
                if !stopped {
                    Oracle::stopped(&rig.observe(app))?;
                }
            }
        }
        report.steps.push(record);
        watch(
            app,
            &mut rig,
            &mut oracle,
            &knobs,
            Duration::from_millis(60),
        )
        .await?;
    }

    closing_user(app, &mut rig, &mut oracle, &knobs).await?;

    // I3: queued rows.
    let obs = rig.observe(app);
    if !obs.rows.is_empty() {
        let unlocked = settle_until(app, knobs.unlock, |app| {
            Oracle::rows_unlocked(&rig.observe(app)).is_ok()
        })
        .await;
        if !unlocked {
            Oracle::rows_unlocked(&rig.observe(app))?;
        }
        if matches!(
            obs.status,
            Some(crate::ai::agent::conversation::ConversationStatus::Success)
        ) {
            let sent = settle_until(app, knobs.send, |app| {
                Oracle::rows_sent(&rig.observe(app), &rig.seam.requests()).is_ok()
            })
            .await;
            if !sent {
                Oracle::rows_sent(&rig.observe(app), &rig.seam.requests())?;
            }
        }
    }

    // I4, I6.
    let requests = rig.seam.requests();
    let emitted = rig.player.emitted();
    oracle::check_tool_messages(&requests, &emitted)?;
    oracle::check_logs(mark, &emitted, capture, &watch.panics())?;

    report.requests = requests.len();
    report.final_status = rig.observe(app).status;
    report.tool_messages_in_last_request =
        requests.last().map_or(0, |r| r.tool_response_ids().len());
    for turn in &emitted {
        for kind in &turn.chaos {
            report.chaos.push((*kind, turn.request_index));
        }
    }
    Ok(report)
}

fn stage_name(stage: PendingStage) -> &'static str {
    match stage {
        PendingStage::Preprocessing => "preprocessing",
        PendingStage::Queued => "queued",
        PendingStage::Blocked => "blocked",
        PendingStage::Running => "running",
    }
}

/// Lets the app run for `span`, finishing commands and checking I1 / I7 on every look.
async fn watch(
    app: &mut App,
    rig: &mut Rig,
    oracle: &mut Oracle,
    knobs: &Knobs,
    span: Duration,
) -> Result<(), Violation> {
    let deadline = Instant::now() + span;
    loop {
        pump_for(app, Duration::from_millis(5)).await;
        rig.note_commands(app);
        rig.drive_commands(app, knobs.stall_after);
        let obs = rig.observe(app);
        oracle.tick(rig, &obs, knobs)?;
        if Instant::now() >= deadline {
            return Ok(());
        }
    }
}

/// After the script: the user answers every card, then the conversation must end (I2a).
async fn closing_user(
    app: &mut App,
    rig: &mut Rig,
    oracle: &mut Oracle,
    knobs: &Knobs,
) -> Result<(), Violation> {
    // A failing turn retries with backoff (0.5 s, 1 s, 2 s): allow for it.
    let failing_turns = rig
        .player
        .scenario()
        .turns
        .iter()
        .filter(|t| t.chaos.as_ref().is_some_and(|c| !c.kinds().is_empty()))
        .count() as u32;
    let budget = knobs.terminal_after_answers + Duration::from_secs(4) * failing_turns;
    let deadline = Instant::now() + budget;
    loop {
        watch(app, rig, oracle, knobs, Duration::from_millis(30)).await?;
        let obs: Obs = rig.observe(app);
        if !obs.cards.is_empty() {
            answer_front_card(app, rig);
            continue;
        }
        // A reply still being retried or streamed is not "settled".
        let quiet = obs.snapshot.is_empty() && !obs.stream_in_flight;
        if quiet && obs.is_terminal() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Oracle::settled_terminal(&obs);
        }
    }
}

fn answer_front_card(app: &mut App, rig: &mut Rig) {
    let is_question = rig
        .front_action(app)
        .is_some_and(|a| matches!(a.action, AIAgentActionType::AskUserQuestion { .. }));
    let op = if is_question {
        Op::AnswerQuestion { choice: 0 }
    } else {
        Op::Approve
    };
    actor::perform(app, rig, &op);
}
