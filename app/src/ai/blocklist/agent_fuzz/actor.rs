//! The scripted user. Every op goes through the entry point the UI uses:
//!
//! | op | entry point |
//! |---|---|
//! | Approve | `BlocklistAIActionModel::execute_next_action_for_user` (the AI block's Enter / the Run button) |
//! | Reject | `cancel_action_with_id(.., Rejected)` (the Reject button, `controller_tests.rs`): not a stop, the step's results go back to the model at once when an earlier one finished (#1942) |
//! | Edit | `set_pending_command_text` then Approve |
//! | AnswerQuestion / SkipQuestion | stage the answers on `AskUserQuestionExecutor`, then `execute_action` by id (the question card) |
//! | CtrlC | with no agent command running: the view's real Ctrl-C key handling (`TerminalView::press_ctrl_c_for_test` -> `handle_ctrl_c_input_event`). That handling interrupts a running action that is not a command (a read, a search) and leaves the turn going (#2034, `BlocklistAIActionModel::interrupt_running_actions`); with nothing running it stops the turn. Into a running agent command: the ^C goes to the command and nowhere else, as in a pane with a pty (`TerminalView::interrupt_agent_command`, #850); the rig has no pty, so it ends the command the way the ^C would (`Rig::interrupt_commands`: exit 130, interrupted) |
//! | Stop | `cancel_conversation_progress(.., ManuallyCancelled)`: what the status bar's Stop button does while a reply streams, a card waits or a tool runs (#2034). (In the app the button on a running agent COMMAND takes the view's Ctrl-C routing and interrupts the command; the rig has no pty and does not model that.) |
//! | FollowUp | typed into the input, `maybe_queue_input_for_in_progress_conversation` decides; if it does not queue, a plain submit |
//! | ModeSwitch / Escape | no controller entry point exists in a pty-less rig; recorded as no-ops |
//! | TakeOver / GiveBack | `TerminalModel` hand-over, covered by `prompt_tests.rs` (needs a pty block) |
//! | CloseTab | `discard_pending_work_for_close` |
//! | Wait | pump |

use std::time::Duration;

use ai::agent::action_result::AskUserQuestionAnswerItem;
use fake_llm::scenario::{Op, Timing};
use warpui::App;

use crate::ai::agent::{AIAgentActionType, CancellationReason};
use crate::ai::blocklist::action_model::PendingStage;

use super::oracle::Knobs;
use super::rig::{Rig, Submitted, pump_for, settle_until};

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
                        CancellationReason::Rejected,
                        ctx,
                    );
                });
            }
        }
        Op::AnswerQuestion { choice } => answer(app, rig, Some(*choice)),
        Op::SkipQuestion => answer(app, rig, None),
        // Ctrl-C goes through the view's real key handling (blocked-turn cancel, the interrupt
        // of a running tool (#2034), agent-view exit rules); Stop is the controller's stop of
        // the conversation, which is what the status bar's button comes to in every state the
        // rig can be in (see the table above).
        //
        // A Ctrl-C typed while an agent command runs is that command's interrupt (#850, owner
        // decision 2026-11-10: the command is interrupted and the turn goes on). In the app the
        // view finds the command's block and writes the ^C to the pty, nothing else. The rig
        // has no pty and no block: the view's handler would not find the command and would take
        // the status bar's cancel instead, which is the Stop path. So for that one case the rig
        // delivers the ^C itself.
        Op::CtrlC => {
            let commands = rig.running_commands(app);
            if commands.is_empty() {
                let terminal = rig.terminal.clone();
                terminal.update(app, |view, ctx| view.press_ctrl_c_for_test(ctx));
            } else {
                rig.interrupt_commands(app, &commands);
            }
        }
        Op::Stop => {
            rig.controller.update(app, |controller, ctx| {
                controller.cancel_conversation_progress(
                    conversation_id,
                    CancellationReason::ManuallyCancelled,
                    ctx,
                );
            });
        }
        Op::FollowUp { text } => {
            // The input decides: queue behind a running or blocked turn, else submit.
            let terminal = rig.terminal.clone();
            let text = text.clone();
            let queued = terminal.update(app, |view, ctx| {
                view.type_and_try_queue_for_test(&text, ctx)
            });
            if queued {
                rig.submitted.push(Submitted {
                    text: text.clone(),
                    queued: true,
                });
            } else {
                rig.send(app, &text);
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
    // Like the real card: it only takes answers while the question is blocked AND its card is
    // up. Staging an answer on a queued (or cardless) question would let the drain complete it
    // without a card and mask an I1 violation.
    let obs = rig.observe(app);
    let blocked = obs
        .snapshot
        .iter()
        .any(|s| s.action_id == action.id && s.stage == PendingStage::Blocked);
    if !blocked || !obs.cards.contains(&action.id) {
        return;
    }
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
    // As the card does after a submit (`ask_user_question_view.rs`): drop whatever the
    // execution did not take, so a refused or cancelled question leaves nothing staged.
    executor.update(app, |executor, _| executor.discard_staged_answers(&id));
}
