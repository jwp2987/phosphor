//! Local BYOP executor for `AIAgentActionType::SendMessageToAgent`.
//!
//! Not a port of the pin's `app/src/ai/blocklist/action_model/execute/send_message.rs`,
//! which posts to Warp's servers via `crate::server::server_api::ServerApiProvider`
//! (`SendAgentMessageRequest`/`SendAgentMessageResponse`) -- cloud, out of
//! scope for this fork. This is new local design: it delivers through the
//! same on-disk mailbox (`warp_cli::agent_mailbox`) that `oz agent message
//! send`/`list` expose to spawned child processes, so a Zap-native agent
//! conversation's `SendMessageToAgent` tool call and a raw `/orchestrate`
//! child's CLI call are two producers/consumers of the same local channel.
//! See `DECLINED.md`'s `#325` row.
use futures::{FutureExt as _, future::BoxFuture};
use warp_core::send_telemetry_from_ctx;
use warpui::{AppContext, Entity, ModelContext, SingletonEntity};

use super::{ActionExecution, ExecuteActionInput, PreprocessActionInput};
use crate::BlocklistAIHistoryModel;
use crate::ai::agent::conversation::AIConversationId;
use crate::ai::agent::{AIAgentActionResultType, AIAgentActionType, SendMessageToAgentResult};
use crate::ai::ambient_agents::AmbientAgentTaskId;
use crate::ai::blocklist::telemetry::{
    BlocklistOrchestrationTelemetryEvent, TeamAgentCommunicationFailedEvent,
    TeamAgentCommunicationFailureReason, TeamAgentCommunicationKind,
    TeamAgentCommunicationTransport, TeamAgentOrchestrationVersion,
};

/// Resolves the sending run's own ID: the current conversation's `run_id`
/// (set once a Zap-native agent conversation has a local task ID) falling
/// back to the ambient task ID the driver pushed down via
/// `set_ambient_agent_task_id` for conversations that haven't recorded one
/// yet. Mirrors the pin's `sender_run_id_and_task_id_for_send`, minus the
/// cloud `task_id`-vs-`AIClient` request-routing distinction that has no
/// local equivalent.
fn sender_run_id_for_send(
    conversation_id: AIConversationId,
    ambient_agent_task_id: Option<AmbientAgentTaskId>,
    ctx: &AppContext,
) -> String {
    BlocklistAIHistoryModel::as_ref(ctx)
        .conversation(&conversation_id)
        .and_then(|conversation| conversation.run_id())
        .or_else(|| ambient_agent_task_id.map(|task_id| task_id.to_string()))
        .unwrap_or_default()
}

pub struct SendMessageToAgentExecutor {
    ambient_agent_task_id: Option<AmbientAgentTaskId>,
}

impl SendMessageToAgentExecutor {
    pub fn new() -> Self {
        Self {
            ambient_agent_task_id: None,
        }
    }

    pub fn set_ambient_agent_task_id(&mut self, id: Option<AmbientAgentTaskId>) {
        self.ambient_agent_task_id = id;
    }

    pub(super) fn should_autoexecute(
        &self,
        _input: ExecuteActionInput,
        _ctx: &mut ModelContext<Self>,
    ) -> bool {
        // Matches the pin: agent-to-agent messages send without a user
        // confirmation prompt, same as every other read-only-to-the-user
        // orchestration signal.
        true
    }

    pub(super) fn execute(
        &mut self,
        input: ExecuteActionInput,
        ctx: &mut ModelContext<Self>,
    ) -> ActionExecution<()> {
        let (addresses, subject, message) = match &input.action.action {
            AIAgentActionType::SendMessageToAgent {
                addresses,
                subject,
                message,
            } => (addresses.clone(), subject.clone(), message.clone()),
            _ => return ActionExecution::InvalidAction,
        };

        let conversation_id = input.conversation_id;
        let sender_run_id =
            sender_run_id_for_send(conversation_id, self.ambient_agent_task_id, ctx);

        if addresses.is_empty() {
            send_telemetry_from_ctx!(
                BlocklistOrchestrationTelemetryEvent::TeamAgentCommunicationFailed(
                    TeamAgentCommunicationFailedEvent {
                        communication_kind: TeamAgentCommunicationKind::Message,
                        transport: TeamAgentCommunicationTransport::Local,
                        orchestration_version: TeamAgentOrchestrationVersion::V2,
                        failure_reason: TeamAgentCommunicationFailureReason::NoTargets,
                        source_conversation_id: conversation_id,
                        source_run_id: (!sender_run_id.is_empty()).then(|| sender_run_id.clone()),
                        target_count: Some(0),
                        lifecycle_event_type: None,
                        error_message: None,
                    }
                ),
                ctx
            );
            return ActionExecution::Sync(AIAgentActionResultType::SendMessageToAgent(
                SendMessageToAgentResult::Error(
                    "No recipient agent IDs were provided.".to_string(),
                ),
            ));
        }

        let root = warp_cli::agent_mailbox::mailbox_root();
        // Every address is attempted, and a failure on one does not cancel the rest.
        //
        // The pin makes a single server call for the whole address list and reads
        // `response.message_ids` back (`42effe840:send_message.rs:186`), so partial
        // delivery is not a state it can be in. The local mailbox writes one address at
        // a time, so it is: stopping at the first `Err` used to leave the later
        // recipients unwritten *and* report the whole call as a flat failure, dropping
        // the ids that did land. The model's only recovery from a flat failure is to
        // resend to the full list, which duplicates the message for everyone who
        // already received it while the addresses after the failure still get nothing.
        let mut delivered: Vec<(String, String)> = Vec::with_capacity(addresses.len());
        let mut failures: Vec<(String, String)> = Vec::new();
        for address in &addresses {
            match warp_cli::agent_mailbox::send_message(
                &root,
                &sender_run_id,
                address,
                &subject,
                &message,
            ) {
                Ok(sent) => delivered.push((address.clone(), sent.message_id)),
                Err(err) => failures.push((address.clone(), err.to_string())),
            }
        }

        // `SendMessageToAgentResult` (crates/ai) has only `Success { message_id }` and
        // `Error(String)` -- there is no partial variant to return, and inventing one
        // would diverge from the pin's action-result shape. So a partial delivery is
        // reported as an error whose text names both halves: which addresses already
        // hold the message (with their ids) and which did not, so a retry can target
        // only the latter instead of re-sending to everyone.
        let delivery_error = (!failures.is_empty()).then(|| {
            let failed = failures
                .iter()
                .map(|(address, error)| format!("{address} ({error})"))
                .collect::<Vec<_>>()
                .join(", ");
            if delivered.is_empty() {
                format!("Failed to deliver to any recipient: {failed}.")
            } else {
                let sent = delivered
                    .iter()
                    .map(|(address, message_id)| format!("{address} (message id {message_id})"))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "Partially delivered: {}/{} recipients received the message -- {sent}. \
                     Failed: {failed}. Do not resend to the recipients listed as delivered.",
                    delivered.len(),
                    addresses.len()
                )
            }
        });

        let result = match delivery_error {
            None => SendMessageToAgentResult::Success {
                message_id: delivered
                    .into_iter()
                    .next()
                    .map(|(_, message_id)| message_id)
                    .unwrap_or_default(),
            },
            Some(error) => {
                send_telemetry_from_ctx!(
                    BlocklistOrchestrationTelemetryEvent::TeamAgentCommunicationFailed(
                        TeamAgentCommunicationFailedEvent {
                            communication_kind: TeamAgentCommunicationKind::Message,
                            transport: TeamAgentCommunicationTransport::Local,
                            orchestration_version: TeamAgentOrchestrationVersion::V2,
                            failure_reason: TeamAgentCommunicationFailureReason::RequestFailed,
                            source_conversation_id: conversation_id,
                            source_run_id: (!sender_run_id.is_empty())
                                .then(|| sender_run_id.clone()),
                            target_count: Some(addresses.len()),
                            lifecycle_event_type: None,
                            error_message: Some(error.clone()),
                        }
                    ),
                    ctx
                );
                log::warn!(
                    "Failed to deliver local agent message: conversation_id={conversation_id:?} \
                     sender_run_id={sender_run_id:?} target_agent_ids={addresses:?} \
                     subject={subject:?} delivered={}/{} error={error}",
                    delivered.len(),
                    addresses.len()
                );
                SendMessageToAgentResult::Error(error)
            }
        };

        ActionExecution::Sync(AIAgentActionResultType::SendMessageToAgent(result))
    }

    pub(super) fn preprocess_action(
        &mut self,
        _action: PreprocessActionInput,
        _ctx: &mut ModelContext<Self>,
    ) -> BoxFuture<'static, ()> {
        futures::future::ready(()).boxed()
    }
}

impl Default for SendMessageToAgentExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl Entity for SendMessageToAgentExecutor {
    type Event = ();
}

#[cfg(test)]
#[path = "send_message_tests.rs"]
mod tests;
