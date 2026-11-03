use std::{collections::HashMap, sync::Arc};

use super::response_stream::RecoveryBudget;
use warp_core::features::FeatureFlag;
use warpui::{AppContext, ModelContext, SingletonEntity};

use crate::{
    ai::{
        agent::{
            conversation::AIConversationId, AIAgentAttachment, AIAgentContext, AIAgentInput,
            CloneRepositoryURL, EntrypointType, RequestMetadata, UserQueryMode,
        },
        blocklist::{
            agent_view::AgentViewEntryOrigin,
            context_model::{context_and_files_from_read, PendingFile, ReadAttachment},
        },
    },
    search::slash_command_menu::static_commands::commands,
    terminal::input::slash_commands::SlashCommandTrigger,
    BlocklistAIHistoryModel,
};

use super::{
    add_pending_file_attachments, input_context_for_request, parse_context_attachments,
    BlocklistAIController, BlocklistAIControllerEvent, QueuedSend, RequestInput,
};

/// A skill invocation waiting for its attached files to be read off the UI thread (#1314).
/// Everything but the attachments' content is resolved when the skill is invoked.
pub(super) struct SkillSend {
    request: SlashCommandRequest,
    is_queued_prompt: bool,
    conversation_id: Option<AIConversationId>,
    /// The request context without the attachments (they are appended last, as
    /// `input_context_for_request` would).
    base_context: Arc<[AIAgentContext]>,
}

impl SkillSend {
    pub(super) fn conversation_id(&self) -> Option<AIConversationId> {
        self.conversation_id
    }

    /// Sends the skill invocation with the attachments read for it.
    pub(super) fn finish(
        self,
        controller: &mut BlocklistAIController,
        read: Vec<ReadAttachment>,
        ctx: &mut ModelContext<BlocklistAIController>,
    ) {
        let (attachment_context, file_attachments, failed) = context_and_files_from_read(read);
        if !failed.is_empty() {
            controller.notify_attachment_read_failures(&failed, ctx);
        }
        let context = with_attachment_context(&self.base_context, attachment_context);
        self.request.finish_send(
            controller,
            self.is_queued_prompt,
            self.conversation_id,
            context,
            file_attachments,
            ctx,
        );
    }
}

/// `base` followed by `attachment_context`: the order `input_context_for_request` produces when
/// it is handed the attachment context directly.
fn with_attachment_context(
    base: &[AIAgentContext],
    attachment_context: Vec<AIAgentContext>,
) -> Arc<[AIAgentContext]> {
    if attachment_context.is_empty() {
        return base.into();
    }
    base.iter().cloned().chain(attachment_context).collect()
}

pub enum SlashCommandRequest {
    CreateNewProject {
        query: String,
    },
    CloneRepository {
        url: String,
    },
    InitProjectRules {
        arguments: Option<String>,
    },
    Summarize {
        prompt: Option<String>,
        /// Zap BYOP local session compaction: whether this summary was
        /// auto-triggered by a token-overflow. The
        /// chat_stream::SummarizeConversation branch uses this to decide the
        /// follow-up copy (the overflow path appends a "previous request exceeded
        /// ..." explanation). False when manually triggered by /compact
        /// /compact-and; true on the auto-trigger path.
        overflow: bool,
    },
    FetchReviewComments {
        repo_path: String,
    },
    /// Invoke a skill.
    InvokeSkill {
        skill: ai::skills::ParsedSkill,
        user_query: Option<String>,
    },
}

impl SlashCommandRequest {
    /// Parses user input into a SlashCommandRequest for slash commands that are handled
    /// via the AI query flow (as opposed to action-based slash commands handled in input.rs).
    pub fn from_query(query: &str) -> Option<SlashCommandRequest> {
        if query == commands::INIT_NAME {
            return Some(Self::InitProjectRules { arguments: None });
        }
        if let Some(arguments) = query
            .strip_prefix(commands::INIT_NAME)
            .and_then(|query| query.strip_prefix(' '))
        {
            return Some(Self::InitProjectRules {
                arguments: Some(arguments.to_string()),
            });
        }

        // Check if query starts with /compact and route to summarize conversation
        if let Some(prompt) = query.strip_prefix(commands::COMPACT.name) {
            return Some(Self::Summarize {
                prompt: prompt.strip_prefix(' ').map(String::from),
                overflow: false, // The text-input path is only for manual /compact, never an auto overflow
            });
        }

        None
    }

    pub(super) fn send_request(
        self,
        controller: &mut BlocklistAIController,
        is_queued_prompt: bool,
        conversation_id_override: Option<AIConversationId>,
        ctx: &mut ModelContext<BlocklistAIController>,
    ) {
        // A fired queued prompt carries the conversation it was queued on; use it directly
        // instead of re-deriving from the current UI selection (which may point at a different
        // conversation, or at none at all, when the row fires). Falls back to the selection for
        // direct sends.
        let conversation_id =
            conversation_id_override.or_else(|| self.conversation_id(controller, ctx));
        // For skill invocations, include user-attached context (images, blocks, and selected
        // text) so the skill's agent sees the same attachments a non-slash-command user query
        // would. Other slash commands continue to pass `false` to preserve existing behavior.
        let is_invoke_skill = matches!(self, Self::InvokeSkill { .. });
        let context = input_context_for_request(
            is_invoke_skill,
            controller.context_model.as_ref(ctx),
            controller.active_session.as_ref(ctx),
            conversation_id,
            vec![],
            ctx,
        );
        if is_invoke_skill {
            // Skill invocations aren't queue-aware (unlike `input_for_query`'s prompt
            // attachments), so this always reads the live-staged attachments. Their content is
            // read off the UI thread, in order with every other submit (#1314).
            let attachments = controller
                .context_model
                .as_ref(ctx)
                .pending_attachments()
                .to_vec();
            controller.schedule_send(
                QueuedSend::Skill(SkillSend {
                    request: self,
                    is_queued_prompt,
                    conversation_id,
                    base_context: context,
                }),
                attachments,
                /*direct_staged*/ true,
                ctx,
            );
            return;
        }
        self.finish_send(
            controller,
            is_queued_prompt,
            conversation_id,
            context,
            vec![],
            ctx,
        );
    }

    /// The second half of `send_request`, once any attachment content has been read.
    fn finish_send(
        self,
        controller: &mut BlocklistAIController,
        is_queued_prompt: bool,
        conversation_id: Option<AIConversationId>,
        context: Arc<[AIAgentContext]>,
        file_attachments: Vec<PendingFile>,
        ctx: &mut ModelContext<BlocklistAIController>,
    ) {
        let is_invoke_skill = matches!(self, Self::InvokeSkill { .. });
        let entrypoint = self.entrypoint();
        let is_summarize = matches!(self, Self::Summarize { .. });
        let inputs = self.input(
            context,
            file_attachments,
            controller.context_model.as_ref(ctx),
            ctx,
        );
        if inputs.is_empty() {
            return;
        }

        // If no existing conversation, create a new one.
        // When AgentView is enabled, enter agent view which creates the conversation
        // and ensures AI blocks render correctly in the agent view.
        let Some(conversation_id) = conversation_id.or_else(|| {
            if FeatureFlag::AgentView.is_enabled() {
                controller.context_model.update(ctx, |context_model, ctx| {
                    context_model
                        .try_start_new_conversation(
                            AgentViewEntryOrigin::SlashCommand {
                                trigger: SlashCommandTrigger::input(),
                            },
                            ctx,
                        )
                        .ok()
                })
            } else {
                Some(controller.start_new_conversation_for_request(ctx).id())
            }
        }) else {
            log::error!("Failed to get conversation ID for slash command request");
            return;
        };

        let Some(conversation) =
            BlocklistAIHistoryModel::as_ref(ctx).conversation(&conversation_id)
        else {
            return;
        };

        let request_input = RequestInput::for_task(
            inputs,
            conversation.get_root_task_id().clone(),
            &controller.active_session,
            controller.get_current_response_initiator(),
            conversation_id,
            controller.terminal_view_id,
            ctx,
        );
        let model_id = request_input.model_id.clone();

        match controller.send_request_input(
            request_input,
            Some(RequestMetadata {
                is_autodetected_user_query: false,
                entrypoint,
                is_auto_resume_after_error: false,
            }),
            /*default_to_follow_up_on_success*/ true,
            RecoveryBudget::fresh(),
            is_queued_prompt,
            ctx,
        ) {
            Ok((_, stream_id)) => {
                // Skill invocations now consume user-attached context (images, blocks, and
                // selected text) the same way regular user queries do. `send_request_input`
                // only clears that context for `AIAgentInput::UserQuery`, so we mirror its
                // reset here for `InvokeSkill` to avoid pending attachments sticking around
                // and getting re-sent on subsequent messages.
                if is_invoke_skill {
                    controller.context_model.update(ctx, |context_model, ctx| {
                        context_model.reset_context_to_default(ctx);
                    });
                }
                // Emit SentRequest event to trigger buffer clearing
                if is_summarize {
                    ctx.emit(BlocklistAIControllerEvent::SentRequest {
                        contains_user_query: true,
                        is_queued_prompt,
                        model_id,
                        stream_id,
                    });
                }
            }
            Err(e) if e.downcast_ref::<super::SuspendedForCloseError>().is_some() => {
                log::debug!("{e}")
            }
            Err(e) => log::error!("Failed to send agent slash command request: {e:?}"),
        }
    }

    pub(super) fn conversation_id(
        &self,
        controller: &BlocklistAIController,
        app: &AppContext,
    ) -> Option<AIConversationId> {
        match self {
            Self::Summarize { .. }
            | Self::InvokeSkill { .. }
            | Self::FetchReviewComments { .. } => controller
                .context_model
                .as_ref(app)
                .selected_conversation_id(app),
            _ => None,
        }
    }

    fn input(
        self,
        context: Arc<[AIAgentContext]>,
        file_attachments: Vec<PendingFile>,
        context_model: &crate::ai::blocklist::BlocklistAIContextModel,
        app: &AppContext,
    ) -> Vec<AIAgentInput> {
        match self {
            SlashCommandRequest::CreateNewProject { query } => {
                vec![AIAgentInput::CreateNewProject { query, context }]
            }
            SlashCommandRequest::CloneRepository { url } => {
                vec![AIAgentInput::CloneRepository {
                    clone_repo_url: CloneRepositoryURL::new(url),
                    context,
                }]
            }
            SlashCommandRequest::InitProjectRules { arguments } => vec![AIAgentInput::UserQuery {
                query: crate::ai::agent_providers::prompt_renderer::render_init_project_command(
                    arguments.as_deref(),
                ),
                context,
                static_query_type: None,
                referenced_attachments: HashMap::<String, AIAgentAttachment>::new(),
                user_query_mode: UserQueryMode::Normal,
                running_command: None,
                intended_agent: None,
            }],
            SlashCommandRequest::Summarize { prompt, overflow } => {
                vec![AIAgentInput::SummarizeConversation { prompt, overflow }]
            }
            SlashCommandRequest::FetchReviewComments { repo_path } => {
                vec![AIAgentInput::FetchReviewComments { repo_path, context }]
            }
            SlashCommandRequest::InvokeSkill { skill, user_query } => {
                let user_query = if FeatureFlag::SkillArguments.is_enabled() {
                    user_query
                        .map(|query| query.trim().to_string())
                        .filter(|query| !query.is_empty())
                        .map(|query| {
                            let mut referenced_attachments =
                                parse_context_attachments(&query, context_model, app);
                            add_pending_file_attachments(
                                &mut referenced_attachments,
                                file_attachments,
                            );
                            crate::ai::agent::InvokeSkillUserQuery {
                                referenced_attachments,
                                query,
                            }
                        })
                } else {
                    None
                };
                vec![AIAgentInput::InvokeSkill {
                    skill,
                    user_query,
                    context,
                }]
            }
        }
    }

    fn entrypoint(&self) -> EntrypointType {
        match self {
            SlashCommandRequest::CloneRepository { .. } => EntrypointType::CloneRepository,
            SlashCommandRequest::InitProjectRules { .. } => EntrypointType::InitProjectRules,
            SlashCommandRequest::CreateNewProject { .. }
            | SlashCommandRequest::Summarize { .. }
            | SlashCommandRequest::FetchReviewComments { .. }
            | SlashCommandRequest::InvokeSkill { .. } => EntrypointType::UserInitiated,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::with_attachment_context;
    use crate::ai::agent::AIAgentContext;

    /// A deferred skill send appends its attachment context after the base context, exactly
    /// where `input_context_for_request` would have put it (#1314).
    #[test]
    fn attachment_context_is_appended_after_the_base_context() {
        let base = vec![
            AIAgentContext::SelectedText("base-1".to_owned()),
            AIAgentContext::SelectedText("base-2".to_owned()),
        ];
        let attachments = vec![AIAgentContext::SelectedText("attached".to_owned())];
        let merged = with_attachment_context(&base, attachments);
        assert_eq!(
            merged.as_ref(),
            &[
                AIAgentContext::SelectedText("base-1".to_owned()),
                AIAgentContext::SelectedText("base-2".to_owned()),
                AIAgentContext::SelectedText("attached".to_owned()),
            ]
        );
        assert_eq!(
            with_attachment_context(&base, vec![]).as_ref(),
            base.as_slice()
        );
    }
}
