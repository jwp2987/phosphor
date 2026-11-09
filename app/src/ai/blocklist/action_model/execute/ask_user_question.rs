use std::collections::HashMap;

use crate::ai::agent::{AIAgentActionId, AIAgentActionResultType, AIAgentActionType};
use crate::ai::blocklist::BlocklistAIPermissions;
use ai::agent::action_result::{AskUserQuestionAnswerItem, AskUserQuestionResult};
use futures::{FutureExt, future::BoxFuture};
use warpui::{Entity, EntityId, ModelContext, SingletonEntity};

use super::{ActionExecution, AnyActionExecution, ExecuteActionInput, PreprocessActionInput};

pub enum AskUserQuestionDecision {
    Completed(Vec<AskUserQuestionAnswerItem>),
    Cancelled,
}

/// The tool result for an `ask_user_question` call with no question this app can show.
///
/// `convert_from` keeps only multiple-choice questions, so a call whose questions were all of
/// another type (or that had none) arrives here with an empty list. There is nothing for a card
/// to render and nothing for the user to answer: waiting for an answer would hang the turn
/// behind a "Preparing question..." that never resolves (#1797).
pub(crate) const NO_ANSWERABLE_QUESTIONS_ERROR: &str = concat!(
    "ask_user_question had no question this client can show: only multiple-choice questions ",
    "are supported. Ask again with multiple-choice questions, or ask in plain text."
);

pub struct AskUserQuestionExecutor {
    result_rx: (
        async_channel::Sender<AskUserQuestionDecision>,
        async_channel::Receiver<AskUserQuestionDecision>,
    ),
    /// Answers the question card submitted, per action, waiting for that action's execution
    /// (#1797). The card stages its answers here and then executes the action by id; `execute`
    /// takes them and finishes synchronously.
    ///
    /// Keyed by action, not a slot in the shared channel above: `execute` drains that channel
    /// on entry so a stale decision from an earlier card cannot be taken for this one (#1278),
    /// and that drain also swallowed the answer the card had just written, leaving the action
    /// running on an empty channel forever. An answer keyed by its action cannot be taken by
    /// any other action, so nothing needs draining.
    staged_answers: HashMap<AIAgentActionId, Vec<AskUserQuestionAnswerItem>>,
    terminal_view_id: EntityId,
}

impl AskUserQuestionExecutor {
    pub fn new(terminal_view_id: EntityId) -> Self {
        Self {
            result_rx: async_channel::unbounded(),
            staged_answers: HashMap::new(),
            terminal_view_id,
        }
    }

    /// Whether `ask_user_question` carries no question a card can show (see
    /// [`NO_ANSWERABLE_QUESTIONS_ERROR`]). Such a call never waits for the user.
    pub(crate) fn has_no_answerable_questions(action: &AIAgentActionType) -> bool {
        matches!(action, AIAgentActionType::AskUserQuestion { questions } if questions.is_empty())
    }

    pub(super) fn should_autoexecute(
        &self,
        input: ExecuteActionInput,
        ctx: &mut ModelContext<Self>,
    ) -> bool {
        // Nothing to show and nothing to answer: resolve it at once with an error result
        // (`execute`) rather than leave it pending with no card (#1797, #1799).
        if Self::has_no_answerable_questions(&input.action.action) {
            return true;
        }
        !BlocklistAIPermissions::as_ref(ctx).can_ask_user_question(
            &input.conversation_id,
            Some(self.terminal_view_id),
            ctx,
        )
    }

    pub(super) fn execute(
        &mut self,
        input: ExecuteActionInput,
        ctx: &mut ModelContext<Self>,
    ) -> impl Into<AnyActionExecution> + use<> {
        let questions = match &input.action.action {
            AIAgentActionType::AskUserQuestion { questions } => questions,
            _ => {
                return ActionExecution::InvalidAction;
            }
        };

        if questions.is_empty() {
            log::warn!(
                "ask_user_question {:?} has no question this client can show; returning an \
                 error result instead of waiting for an answer (#1797)",
                input.action.id
            );
            return ActionExecution::Sync(AIAgentActionResultType::AskUserQuestion(
                AskUserQuestionResult::Error(NO_ANSWERABLE_QUESTIONS_ERROR.to_owned()),
            ));
        }

        if self.should_autoexecute(input, ctx) {
            let question_ids = questions
                .iter()
                .map(|question| question.question_id.clone())
                .collect();
            return ActionExecution::Sync(AIAgentActionResultType::AskUserQuestion(
                AskUserQuestionResult::SkippedByAutoApprove { question_ids },
            ));
        }

        // The card's answer, staged by `stage_answers` just before it executed this action.
        if let Some(answers) = self.staged_answers.remove(&input.action.id) {
            return ActionExecution::Sync(AIAgentActionResultType::AskUserQuestion(
                AskUserQuestionResult::Success { answers },
            ));
        }

        // No staged answer. `BlocklistAIActionModel` refuses a user accept of a question with
        // none (`question_accept_has_no_answer`), so no production path gets here; this waits
        // on the decision channel as before.
        let receiver = self.result_rx.1.clone();
        // The channel is shared by every card this executor ever runs. A Stop that raced an
        // answer (or a `cancel` with nothing waiting) leaves a decision queued; without this
        // drain the NEXT card would consume it as if the user had just answered (#1278).
        while receiver.try_recv().is_ok() {}
        ActionExecution::new_async(
            async move { receiver.recv().await },
            |result, _ctx| match result {
                Ok(AskUserQuestionDecision::Completed(answers)) => {
                    AIAgentActionResultType::AskUserQuestion(AskUserQuestionResult::Success {
                        answers,
                    })
                }
                Ok(AskUserQuestionDecision::Cancelled) => {
                    AIAgentActionResultType::AskUserQuestion(AskUserQuestionResult::Cancelled)
                }
                Err(_) => {
                    AIAgentActionResultType::AskUserQuestion(AskUserQuestionResult::Cancelled)
                }
            },
        )
    }

    pub(super) fn preprocess_action(
        &mut self,
        _action: PreprocessActionInput,
        _ctx: &mut ModelContext<Self>,
    ) -> BoxFuture<'static, ()> {
        futures::future::ready(()).boxed()
    }

    /// Stages the question card's `answers` for `action_id`. The card then executes the action
    /// by id, and `execute` returns them as its result (#1797).
    pub fn stage_answers(
        &mut self,
        action_id: AIAgentActionId,
        answers: Vec<AskUserQuestionAnswerItem>,
    ) {
        self.staged_answers.insert(action_id, answers);
    }

    /// Whether answers are staged for `action_id` (see [`Self::stage_answers`]).
    pub fn has_staged_answers(&self, action_id: &AIAgentActionId) -> bool {
        self.staged_answers.contains_key(action_id)
    }

    /// Drops answers staged for `action_id` whose execution never took them (the action was
    /// cancelled or refused), so they cannot outlive it.
    pub(super) fn discard_staged_answers(&mut self, action_id: &AIAgentActionId) {
        self.staged_answers.remove(action_id);
    }

    pub fn complete(&self, answers: Vec<AskUserQuestionAnswerItem>) {
        let _ = self
            .result_rx
            .0
            .try_send(AskUserQuestionDecision::Completed(answers));
    }

    pub fn cancel(&self) {
        let _ = self
            .result_rx
            .0
            .try_send(AskUserQuestionDecision::Cancelled);
    }
}

#[cfg(test)]
impl Default for AskUserQuestionExecutor {
    fn default() -> Self {
        Self::new(EntityId::new())
    }
}

impl Entity for AskUserQuestionExecutor {
    type Event = ();
}

#[cfg(test)]
#[path = "ask_user_question_tests.rs"]
mod tests;
