use futures::{FutureExt, future::BoxFuture};
use warpui::{Entity, ModelContext};

use crate::ai::agent::{
    AIAgentAction, AIAgentActionResultType, AIAgentActionType, SuggestNewConversationResult,
};

use super::{ActionExecution, AnyActionExecution, ExecuteActionInput, PreprocessActionInput};

/// Whether the client accepted or rejected the new conversation. We make this a separate type from
/// `SuggestNewConversationResult` for more ergonomic threading of the message_id through
/// the various layers of action handling.
pub enum NewConversationDecision {
    Accept,
    Reject,
}

pub struct SuggestNewConversationExecutor {
    suggest_new_conversation_result_rx: (
        async_channel::Sender<NewConversationDecision>,
        async_channel::Receiver<NewConversationDecision>,
    ),
}

impl SuggestNewConversationExecutor {
    pub fn new() -> Self {
        Self {
            suggest_new_conversation_result_rx: async_channel::unbounded(),
        }
    }

    pub(super) fn should_autoexecute(
        &self,
        _input: ExecuteActionInput,
        _ctx: &mut ModelContext<Self>,
    ) -> bool {
        false
    }

    pub(super) fn execute(
        &mut self,
        input: ExecuteActionInput,
        _ctx: &mut ModelContext<Self>,
    ) -> impl Into<AnyActionExecution> + use<> {
        let AIAgentAction {
            action: AIAgentActionType::SuggestNewConversation { message_id },
            ..
        } = input.action
        else {
            return ActionExecution::InvalidAction;
        };

        let message_id = message_id.clone();
        let receiver = self.suggest_new_conversation_result_rx.clone().1;
        // The UI queues the user's decision BEFORE it asks for the action to execute, so a decision
        // already waiting here is this call's and must be kept (draining it, as an earlier version
        // did, turned every Accept into the Reject below). Only when nobody has decided is the
        // fast-fail Reject queued.
        //
        // Zap: there's no "Start a new conversation?" confirmation modal view
        // component, so this fast-fails with Reject to prevent the BYOP loop from
        // hanging forever waiting on the receiver. The model seeing Rejected is fine
        // and doesn't affect the main flow. The Reject is sent synchronously so the
        // following receiver.recv() gets it immediately.
        if receiver.is_empty() {
            let _ = self
                .suggest_new_conversation_result_rx
                .0
                .try_send(NewConversationDecision::Reject);
        }
        // Once this call has its decision, anything else still queued is stale (a late second
        // click): drop it so it cannot be taken for the next card's decision (#1278).
        let decision = async move {
            let decision = receiver.recv().await;
            while receiver.try_recv().is_ok() {}
            decision
        };
        ActionExecution::new_async(decision, move |result, _ctx| match result {
            Ok(NewConversationDecision::Accept) => AIAgentActionResultType::SuggestNewConversation(
                SuggestNewConversationResult::Accepted { message_id },
            ),
            Ok(NewConversationDecision::Reject) => AIAgentActionResultType::SuggestNewConversation(
                SuggestNewConversationResult::Rejected,
            ),
            Err(_) => AIAgentActionResultType::SuggestNewConversation(
                SuggestNewConversationResult::Cancelled,
            ),
        })
    }

    pub(super) fn preprocess_action(
        &mut self,
        _action: PreprocessActionInput,
        _ctx: &mut ModelContext<Self>,
    ) -> BoxFuture<'static, ()> {
        futures::future::ready(()).boxed()
    }

    pub fn complete_suggest_new_conversation_action(&self, decision: NewConversationDecision) {
        let _ = self.suggest_new_conversation_result_rx.0.try_send(decision);
    }
}

impl Default for SuggestNewConversationExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl Entity for SuggestNewConversationExecutor {
    type Event = ();
}

#[cfg(test)]
#[path = "suggest_new_conversation_tests.rs"]
mod tests;
