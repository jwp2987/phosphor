//! The scripted user. Every op goes through the entry point the UI uses:
//!
//! | op | entry point |
//! |---|---|
//! | Approve | `BlocklistAIActionModel::execute_next_action_for_user` (the AI block's Enter / the Run button) |
//! | Reject | `cancel_action_with_id(.., ManuallyCancelled)` (the Reject button, `controller_tests.rs`) |
//! | Edit | `set_pending_command_text` then Approve |
//! | AnswerQuestion / SkipQuestion | stage the answers on `AskUserQuestionExecutor`, then `execute_action` by id (the question card) |
//! | CtrlC / Stop | `cancel_conversation_progress(.., ManuallyCancelled)` (the status bar's Ctrl-C / Stop) |
//! | FollowUp | `QueuedQueryModel::append(PendingApprovalFollowUp)` while blocked, else a plain submit |
//! | ModeSwitch / Escape | no controller entry point exists in a pty-less rig; recorded as no-ops |
//! | TakeOver / GiveBack | `TerminalModel` hand-over, covered by `prompt_tests.rs` (needs a pty block) |
//! | CloseTab | `discard_pending_work_for_close` |
//! | Wait | pump |

use std::time::Duration;

use ai::agent::action_result::AskUserQuestionAnswerItem;
use fake_llm::scenario::{Op, Timing};
use warpui::App;

use crate::ai::agent::{AIAgentActionType, CancellationReason};
use crate::ai::blocklist::{QueuedQuery, QueuedQueryModel, QueuedQueryOrigin};

use super::oracle::Knobs;
use super::rig::{Rig, pump_for, settle_until};
use warpui::SingletonEntity;

/// What a step did, for the coverage table and the failure message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepRecord {
    pub op: &'static str,
    pub timing: &'static str,
    /// The tool kind the op acted on, if it acted on an action.
    pub tool: Option<&'static str>,
    /// The front action's stage when the op ran (`blocked`, `queued`, `running`, ...), if any.
    pub stage: Option<&'static str>,
    /// `false` = the trigger never came (no card appeared, nothing streaming), so the step was
    /// skipped.
    pub performed: bool,
}

pub fn op_name(op: &Op) -> &'static str {
    match op {
        Op::Approve => "approve",
        Op::Reject => "reject",
        Op::Edit { .. } => "edit",
        Op::AnswerQuestion { .. } => "answer",
        Op::SkipQuestion => "skip",
        Op::CtrlC => "ctrl_c",
        Op::Stop => "stop",
        Op::FollowUp { .. } => "follow_up",
        Op::ModeSwitch => "mode_switch",
        Op::Escape => "escape",
        Op::TakeOver => "take_over",
        Op::GiveBack => "give_back",
        Op::CloseTab => "close_tab",
        Op::Wait { .. } => "wait",
    }
}

pub fn timing_name(timing: Timing) -> &'static str {
    match timing {
        Timing::Fast => "fast",
        Timing::Human => "human",
        Timing::DuringStream => "during_stream",
        Timing::AfterEnd => "after_end",
        Timing::OnCard => "on_card",
        Timing::Never => "never",
    }
}

/// Waits for the step's trigger. `false` = it never came within the budget.
pub async fn await_trigger(app: &mut App, rig: &mut Rig, timing: Timing, knobs: &Knobs) -> bool {
    match timing {
        Timing::Never => false,
        Timing::Fast => {
            pump_for(
                app,
                Duration::from_millis(10 + (rig.player.scenario().seed % 16)),
            )
            .await;
            true
        }
        Timing::Human => {
            let ms = 400 + rig.player.scenario().seed % 2100;
            pump_for(app, knobs.scale(Duration::from_millis(ms))).await;
            true
        }
        Timing::DuringStream => {
            settle_until(app, knobs.trigger_budget, |app| {
                rig.observe(app).stream_in_flight
            })
            .await
        }
        Timing::AfterEnd => {
            settle_until(app, knobs.trigger_budget, |app| {
                let obs = rig.observe(app);
                rig.player.turns_used() > 0 && !obs.stream_in_flight
            })
            .await
        }
        Timing::OnCard => {
            settle_until(app, knobs.trigger_budget, |app| {
                !rig.observe(app).cards.is_empty()
            })
            .await
        }
    }
}

/// Performs `op`. Returns the tool kind it acted on.
pub fn perform(app: &mut App, rig: &mut Rig, op: &Op) -> Option<&'static str> {
    rig.note_commands(app);
    let conversation_id = rig.conversation_id;
    let front = rig.front_action(app);
    let kind = front
        .as_ref()
        .map(|a| crate::ai::blocklist::action_model::fuzz_action_kind(&a.action));
    match op {
        Op::Approve => {
            rig.action_model.update(app, |model, ctx| {
                model.execute_next_action_for_user(conversation_id, ctx);
            });
        }
        Op::Edit { text } => {
            if let Some(action) = &front {
                if matches!(
                    action.action,
                    AIAgentActionType::RequestCommandOutput { .. }
                ) {
                    let id = action.id.clone();
                    let text = text.clone();
                    rig.action_model.update(app, |model, _| {
                        model.set_pending_command_text(conversation_id, &id, text);
                    });
                    // An edited command no longer names a scenario command: it is a quick one.
                    rig.cmd_keys.remove(&id);
                }
            }
            rig.action_model.update(app, |model, ctx| {
                model.execute_next_action_for_user(conversation_id, ctx);
            });
        }
        Op::Reject => {
            if let Some(action) = &front {
                let id = action.id.clone();
                rig.action_model.update(app, |model, ctx| {
                    model.cancel_action_with_id(
                        conversation_id,
                        &id,
                        CancellationReason::ManuallyCancelled,
                        ctx,
                    );
                });
            }
        }
        Op::AnswerQuestion { choice } => answer(app, rig, Some(*choice)),
        Op::SkipQuestion => answer(app, rig, None),
        Op::CtrlC | Op::Stop => {
            rig.controller.update(app, |controller, ctx| {
                controller.cancel_conversation_progress(
                    conversation_id,
                    CancellationReason::ManuallyCancelled,
                    ctx,
                );
            });
        }
        Op::FollowUp { text } => {
            let blocked = rig
                .observe(app)
                .status
                .as_ref()
                .is_some_and(|status| status.is_blocked());
            if blocked {
                let row =
                    QueuedQuery::new(text.clone(), QueuedQueryOrigin::PendingApprovalFollowUp);
                QueuedQueryModel::handle(app).update(app, |model, ctx| {
                    model.append(conversation_id, row, ctx);
                });
            } else {
                rig.send(app, text);
            }
        }
        Op::CloseTab => {
            rig.controller.update(app, |controller, ctx| {
                controller.discard_pending_work_for_close(conversation_id, ctx);
            });
        }
        Op::ModeSwitch | Op::Escape | Op::TakeOver | Op::GiveBack | Op::Wait { .. } => {}
    }
    kind
}

/// Answers (or skips) the front question the way the question card does: stage, then execute by
/// id. Tolerates either flow for `ask_user_question`: a card that needs no approval step first
/// (the card appears; the answer completes it) and one that does; the front action is simply
/// whatever is pending.
fn answer(app: &mut App, rig: &mut Rig, choice: Option<usize>) {
    let conversation_id = rig.conversation_id;
    let Some(action) = rig.front_action(app) else {
        return;
    };
    let AIAgentActionType::AskUserQuestion { questions } = &action.action else {
        return;
    };
    let answers: Vec<AskUserQuestionAnswerItem> = questions
        .iter()
        .map(|question| match choice {
            None => AskUserQuestionAnswerItem::Skipped {
                question_id: question.question_id.clone(),
            },
            Some(choice) => {
                let options = question.multiple_choice_options().unwrap_or(&[]);
                let selected = options
                    .get(choice.min(options.len().saturating_sub(1)))
                    .map(|option| vec![option.label.clone()])
                    .unwrap_or_default();
                AskUserQuestionAnswerItem::Answered {
                    question_id: question.question_id.clone(),
                    selected_options: selected,
                    other_text: String::new(),
                }
            }
        })
        .collect();
    let id = action.id.clone();
    let executor = rig
        .action_model
        .read(app, |model, ctx| model.ask_user_question_executor(ctx));
    executor.update(app, |executor, _| {
        executor.stage_answers(id.clone(), answers)
    });
    rig.action_model.update(app, |model, ctx| {
        model.execute_action(&id, conversation_id, ctx);
    });
}
