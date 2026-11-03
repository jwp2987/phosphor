use super::{ActionExecution, AnyActionExecution, ExecuteActionInput, PreprocessActionInput};
use crate::ai::mcp::TemplatableMCPServerManager;
use crate::terminal::model::session::active_session::ActiveSession;
use futures::{FutureExt, future::BoxFuture};
use warpui::{Entity, EntityId, ModelContext, ModelHandle};

use crate::ai::{
    agent::{AIAgentActionResultType, ReadMCPResourceResult},
    blocklist::{
        BlocklistAIPermissions,
        action_model::{AIAgentAction, AIAgentActionType},
    },
};
use warpui::SingletonEntity;

pub struct ReadMCPResourceExecutor {
    _active_session: ModelHandle<ActiveSession>,
    #[cfg_attr(target_family = "wasm", expect(unused))]
    terminal_view_id: EntityId,
}

impl ReadMCPResourceExecutor {
    pub fn new(_active_session: ModelHandle<ActiveSession>, terminal_view_id: EntityId) -> Self {
        Self {
            _active_session,
            terminal_view_id,
        }
    }

    #[cfg_attr(target_family = "wasm", allow(unused_variables), allow(dead_code))]
    pub(super) fn should_autoexecute(
        &self,
        input: ExecuteActionInput,
        ctx: &mut ModelContext<Self>,
    ) -> bool {
        #[cfg(target_family = "wasm")]
        {
            false
        }

        {
            let ExecuteActionInput {
                action:
                    AIAgentAction {
                        action:
                            AIAgentActionType::ReadMCPResource {
                                server_id,
                                name,
                                uri,
                                ..
                            },
                        ..
                    },
                conversation_id,
            } = input
            else {
                return false;
            };

            BlocklistAIPermissions::as_ref(ctx).can_read_mcp_resource(
                server_id.as_ref(),
                name.as_str(),
                uri.as_deref(),
                &conversation_id,
                Some(self.terminal_view_id),
                ctx,
            )
        }
    }

    #[cfg_attr(target_family = "wasm", allow(unused_variables))]
    pub(super) fn execute(
        &mut self,
        input: ExecuteActionInput,
        ctx: &mut ModelContext<Self>,
    ) -> impl Into<AnyActionExecution> + use<> {
        #[cfg(target_family = "wasm")]
        {
            ActionExecution::<()>::InvalidAction
        }

        {
            let ExecuteActionInput { action, .. } = input;
            let AIAgentAction {
                action:
                    AIAgentActionType::ReadMCPResource {
                        server_id,
                        name,
                        uri,
                    },
                ..
            } = action
            else {
                return ActionExecution::InvalidAction;
            };

            let templatable_mcp_client = TemplatableMCPServerManager::as_ref(ctx);

            let resource = match uri {
                Some(uri) => templatable_mcp_client
                    .resources()
                    .find(|resource| &resource.uri == uri),
                None => templatable_mcp_client
                    .resources()
                    .find(|resource| &resource.name == name),
            };

            let Some(resource) = resource else {
                return ActionExecution::Sync(AIAgentActionResultType::ReadMCPResource(
                    ReadMCPResourceResult::Error("MCP server resource not found".to_owned()),
                ));
            };

            let uri = resource.uri.clone();

            // The server the call named, when it named one (`tools::mcp::parse_read_resource`
            // resolves it); otherwise any server that has the URI.
            let Some(reconnecting_peer) =
                templatable_mcp_client.server_with_resource(resource, *server_id)
            else {
                return ActionExecution::Sync(AIAgentActionResultType::ReadMCPResource(
                    ReadMCPResourceResult::Error("MCP server for resource not found".to_owned()),
                ));
            };

            // Bounded like a tool call: `read_resource` itself has no deadline, and a server
            // that accepts the request and never answers would otherwise hold the turn until
            // the user stops it.
            let timeout = reconnecting_peer.tool_call_timeout();
            ActionExecution::new_async(
                async move {
                    let read = reconnecting_peer
                        // `ReadResourceRequestParams` (was `…Param`) is
                        // `#[non_exhaustive]` in rmcp 1.x; use its constructor.
                        .read_resource(rmcp::model::ReadResourceRequestParams::new(uri));
                    let deadline = warpui::r#async::Timer::after(timeout);
                    futures::pin_mut!(read, deadline);
                    match futures::future::select(read, deadline).await {
                        futures::future::Either::Left((result, _)) => result,
                        futures::future::Either::Right(_) => {
                            Err(read_resource_timeout_error(timeout))
                        }
                    }
                },
                |res, _ctx| handle_read_resource_result(res),
            )
        }
    }

    pub(super) fn preprocess_action(
        &mut self,
        _action: PreprocessActionInput,
        _ctx: &mut ModelContext<Self>,
    ) -> BoxFuture<'static, ()> {
        futures::future::ready(()).boxed()
    }
}

impl Entity for ReadMCPResourceExecutor {
    type Event = ();
}

/// The error the model sees when a resource read ran out of time. Says the outcome is unknown
/// and not to retry on its own, like a timed-out tool call (`mcp::tool_call`).
pub(crate) fn read_resource_timeout_error(timeout: std::time::Duration) -> rmcp::ServiceError {
    rmcp::ServiceError::McpError(rmcp::model::ErrorData::internal_error(
        format!(
            "MCP resource read timed out after {} seconds without an answer from the server. \
             The server may be stuck; do not retry automatically.",
            timeout.as_secs()
        ),
        None,
    ))
}

/// Handles the result of a read_resource request, converting it to an AIAgentActionResultType.
fn handle_read_resource_result(
    res: Result<rmcp::model::ReadResourceResult, rmcp::ServiceError>,
) -> AIAgentActionResultType {
    let action_result = match res {
        Ok(response) => ReadMCPResourceResult::Success {
            resource_contents: response.contents,
        },
        Err(e) => ReadMCPResourceResult::Error(e.to_string()),
    };
    AIAgentActionResultType::ReadMCPResource(action_result)
}
