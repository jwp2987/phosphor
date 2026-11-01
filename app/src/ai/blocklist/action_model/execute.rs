pub(super) mod agent_shell_wrap;
pub(super) mod ask_user_question;
pub(super) mod call_mcp_tool;
pub(super) mod create_documents;
pub(super) mod edit_documents;
pub(super) mod file_glob;
pub(super) mod grep;
pub(super) mod read_documents;
pub(super) mod read_files;
pub(super) mod read_mcp_resource;
pub(super) mod read_skill;
pub(super) mod request_computer_use;
pub(super) mod request_file_edits;
pub(super) mod send_message;
pub(super) mod shell_command;
pub(super) mod start_agent;
pub(super) mod suggest_new_conversation;
pub(super) mod suggest_prompt;
pub(super) mod use_computer;

use ai::agent::action_result::{InsertReviewCommentsResult, RequestCommandOutputResult};
pub use ask_user_question::AskUserQuestionExecutor;
use call_mcp_tool::CallMCPToolExecutor;
pub(crate) use call_mcp_tool::coerce_integer_args;
use create_documents::CreateDocumentsExecutor;
use edit_documents::EditDocumentsExecutor;
use file_glob::FileGlobExecutor;
use futures::{FutureExt, future::BoxFuture};
use grep::GrepExecutor;
use parking_lot::FairMutex;
use read_documents::ReadDocumentsExecutor;
pub(super) use read_files::ReadFilesExecutor;
use read_mcp_resource::ReadMCPResourceExecutor;
use read_skill::ReadSkillExecutor;
use request_computer_use::RequestComputerUseExecutor;
pub(crate) use request_file_edits::FileReadResult;
pub(crate) use request_file_edits::MalformedFinalLineProxyEvent;
pub(crate) use request_file_edits::apply_edits;
pub use request_file_edits::{
    EditAcceptAndContinueClickedEvent, EditAcceptClickedEvent, EditResolvedEvent, EditStats,
    RequestFileEditsExecutor, RequestFileEditsFormatKind, RequestFileEditsTelemetryEvent,
};
pub use send_message::SendMessageToAgentExecutor;
use serde::{Deserialize, Serialize};
pub(crate) use shell_command::pty_write_for_target_command;
pub use shell_command::{ShellCommandExecutor, ShellCommandExecutorEvent};
pub use start_agent::{
    StartAgentExecutor, StartAgentExecutorEvent, StartAgentOutcome, StartAgentRequest,
    StartAgentRequestId,
};
pub use suggest_new_conversation::NewConversationDecision;
use suggest_new_conversation::SuggestNewConversationExecutor;
pub use suggest_prompt::{PromptSuggestionExecutor, PromptSuggestionExecutorEvent};
use use_computer::UseComputerExecutor;
use warp_core::{execution_mode::AppExecutionMode, features::FeatureFlag};

#[cfg(feature = "local_fs")]
use crate::util::openable_file_type::is_binary_file;
#[cfg(feature = "local_fs")]
use futures::AsyncReadExt;
use std::{any::Any, path::PathBuf, pin::Pin, sync::Arc};
#[cfg(feature = "local_fs")]
use warp_files::{FileModel, TextFileReadResult};
#[cfg(feature = "local_fs")]
use warp_util::file::FileLoadError;
#[cfg(feature = "local_fs")]
use warp_util::file_type::is_buffer_binary;
use warpui::{
    AppContext, Entity, EntityId, ModelContext, ModelHandle, SingletonEntity,
    r#async::{Spawnable, SpawnableOutput, SpawnedFutureHandle},
};

#[cfg(feature = "local_fs")]
use crate::util::image::{
    ProcessImageResult, is_supported_image_mime_type, process_image_for_agent,
};
#[cfg(feature = "local_fs")]
use mime_guess::from_path;

#[cfg(feature = "local_fs")]
use crate::ai::{agent::AnyFileContent, paths::host_native_absolute_path};
use crate::{
    BlocklistAIHistoryModel,
    ai::{
        agent::{
            AIAgentAction, AIAgentActionId, AIAgentActionResult, AIAgentActionResultType,
            AIAgentActionType, CancellationReason, FileContext, FileLocations, ReadFilesFailedFile,
            RequestFileEditsResult, ServerOutputId, conversation::AIConversationId,
        },
        ambient_agents::AmbientAgentTaskId,
    },
    terminal::{
        ShellLaunchData, TerminalModel,
        model::session::{
            ExecuteCommandOptions, Session, active_session::ActiveSession,
            command_executor::shell_quote_arg,
        },
        model_events::ModelEventDispatcher,
        shell::ShellType,
    },
};

/// Types of actions that can be executed in parallel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ParallelExecutionPolicy {
    /// Read-only actions that only inspect local context and may be safely coalesced into the
    /// same execution phase when the underlying runtime supports it.
    ReadOnlyLocalContext,
}

/// Whether an action is running serially or in parallel with other actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RunningActionPhase {
    /// A barrier action that must run by itself.
    Serial,
    /// A phase where several actions from the same compatibility group may be in flight together.
    Parallel(ParallelExecutionPolicy),
}

#[derive(Debug, Clone, Copy)]
struct ExecuteActionInput<'a> {
    action: &'a AIAgentAction,
    conversation_id: AIConversationId,
}

#[derive(Debug, Clone, Copy)]
struct PreprocessActionInput<'a> {
    action: &'a AIAgentAction,
    conversation_id: AIConversationId,
}

type AsyncExecuteActionFn<T> = Pin<Box<dyn Spawnable<Output = T>>>;
type OnCompleteFn<T> = Box<dyn FnOnce(T, &mut AppContext) -> AIAgentActionResultType>;

enum ActionExecution<T: SpawnableOutput> {
    Async {
        execute_future: AsyncExecuteActionFn<T>,
        on_complete: OnCompleteFn<T>,
    },
    Sync(AIAgentActionResultType),
    NotReady,
    InvalidAction,
}

impl<T: SpawnableOutput> ActionExecution<T> {
    fn new_async(
        execute_future: impl Spawnable<Output = T>,
        on_complete: impl FnOnce(T, &mut AppContext) -> AIAgentActionResultType + 'static,
    ) -> Self {
        Self::Async {
            execute_future: Box::pin(execute_future),
            on_complete: Box::new(on_complete),
        }
    }
}

/// A trait implemented by all types that implement [`Any`] and [`SpawnableOutput`].
trait AnySpawnableOutput: Any + SpawnableOutput {}
impl<T> AnySpawnableOutput for T where T: Any + SpawnableOutput {}

type AnyAsyncExecuteActionFn = Pin<Box<dyn Spawnable<Output = Box<dyn AnySpawnableOutput>>>>;
type AnyOnCompleteFn = Box<dyn FnOnce(Box<dyn Any>, &mut AppContext) -> AIAgentActionResultType>;

enum AnyActionExecution {
    Async {
        execute_future: AnyAsyncExecuteActionFn,
        on_complete: AnyOnCompleteFn,
    },
    Sync(AIAgentActionResultType),
    NotReady,
    InvalidAction,
}

// Hand-written because the `Async` variant holds a boxed future and a boxed
// `FnOnce`, neither of which is `Debug`. Tests that match on an executor's
// result need to be able to name the variant they got instead in a failure
// message; without this they can only report "the match arm did not fire".
impl std::fmt::Debug for AnyActionExecution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Async { .. } => f.write_str("Async { .. }"),
            Self::Sync(result) => f.debug_tuple("Sync").field(result).finish(),
            Self::NotReady => f.write_str("NotReady"),
            Self::InvalidAction => f.write_str("InvalidAction"),
        }
    }
}

impl<T> From<ActionExecution<T>> for AnyActionExecution
where
    T: Send + 'static,
{
    fn from(value: ActionExecution<T>) -> Self {
        match value {
            ActionExecution::Async {
                execute_future,
                on_complete,
            } => AnyActionExecution::Async {
                execute_future: Box::pin(async move {
                    let result = execute_future.await;
                    Box::new(result) as Box<dyn AnySpawnableOutput>
                }),
                on_complete: Box::new(move |result, app| {
                    on_complete(*result.downcast::<T>().expect("Type is correct."), app)
                }),
            },
            ActionExecution::Sync(result) => AnyActionExecution::Sync(result),
            ActionExecution::NotReady => AnyActionExecution::NotReady,
            ActionExecution::InvalidAction => AnyActionExecution::InvalidAction,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum NotExecutedReason {
    NotReady,
    NeedsConfirmation,
    WaitingOnSharer,
}

impl NotExecutedReason {
    pub fn needs_confirmation(&self) -> bool {
        matches!(self, Self::NeedsConfirmation)
    }
}

/// Who is asking for a pending action to run.
///
/// This replaces a bare `is_user_initiated: bool`. The bool had exactly two producers at the
/// pin (`42effe840:app/src/ai/blocklist/action_model.rs:767,782`), both of them genuine user
/// clicks, so a bool was enough there. This fork once added a third: the LRC tag-in
/// auto-accept, which passed `true` for something the user never clicked and so gave it the
/// full authority of a click (#617). That path is gone (#1070), but the split stays: a new
/// caller has to name which one it is instead of inheriting the strongest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ActionInitiator {
    /// The agent's own turn: actions drain from the queue in order and every confirmation is
    /// decided by `should_autoexecute` / the execution profile.
    Agent,
    /// The user clicked Accept (or Run, or answered the question) on this exact action's
    /// confirmation UI. This is a real, specific, per-action approval.
    User,
}

impl ActionInitiator {
    /// Whether this initiator dequeues one specific action out of band, rather than draining
    /// the queue in order. Those paths execute one action at a time so interactive
    /// confirmations do not overlap in the UI.
    pub(super) fn is_out_of_band(self) -> bool {
        !matches!(self, Self::Agent)
    }

    /// Whether this initiator may stand in for the confirmation prompt on `action`.
    ///
    /// `User` may, for anything: it *is* the answer to the prompt. `Agent` never may: on the
    /// agent's own turn `should_autoexecute` and the execution profile are the only deciders.
    ///
    /// There is deliberately no third answer. The LRC alt-screen tag-in used to stand in for
    /// every confirmation except computer use, on the theory that the Accept button is not
    /// rendered while a full-screen program owns the screen. It is: the CLI subagent view is
    /// mounted over the alt screen on a tag-in and renders Allow/Reject for every tool a
    /// tag-in turn is offered (`chat_stream::CLI_AGENT_TOOLS`). The override only ever
    /// changed anything when the profile had said no (#617, #1070).
    pub(super) fn can_stand_in_for_confirmation(self, _action: &AIAgentActionType) -> bool {
        match self {
            Self::Agent => false,
            Self::User => true,
        }
    }

    /// [`Self::can_stand_in_for_confirmation`], additionally refusing every stand-in except
    /// a real click when the action writes a protected path.
    ///
    /// A write to `~/.ssh/authorized_keys`, an MCP config, a shell rc file or this app's own
    /// settings is exactly how an agent escalates itself, so only the user's own confirmation
    /// may approve it.
    pub(super) fn can_stand_in_for_confirmation_of_write(
        self,
        action: &AIAgentActionType,
        writes_protected_path: bool,
    ) -> bool {
        if writes_protected_path && self != Self::User {
            return false;
        }
        self.can_stand_in_for_confirmation(action)
    }
}

/// Result type for `BlocklistAIActionExecutor::try_to_execute_action`.
#[derive(Debug)]
pub(super) enum TryExecuteResult {
    ExecutedSync,
    ExecutedAsync,
    NotExecuted {
        reason: NotExecutedReason,
        action: Box<AIAgentAction>,
    },
}

#[derive(Clone)]
struct AsyncExecutingAction {
    action: AIAgentAction,
    /// The conversation this action belongs to so cancellation and follow-up scheduling remain
    /// scoped even when several conversations have async actions in flight.
    conversation_id: AIConversationId,
    /// The spawned execution, aborted when the action is cancelled so a Stop really stops the
    /// work (a grep over a large tree, an MCP call, a remote read over the shared ssh
    /// connection, #1241) instead of only discarding its result. `None` in tests that register
    /// an action without spawning it.
    execution: Option<SpawnedFutureHandle>,
    /// Set when a Stop arrived while this file-edit action was saving. The writes are spawned by
    /// `FileModel` and cannot be recalled, so the action is not reported Cancelled while they
    /// may still land (#1277): it keeps running until its real result arrives, or until
    /// [`FILE_EDIT_CANCEL_GRACE`] passes. Holds the reason of the Stop.
    cancel_pending: Option<Option<CancellationReason>>,
}

/// Result text for a file-edit action that was stopped mid-save and never reported an outcome.
pub(super) const FILE_EDIT_STOPPED_MID_SAVE: &str = "The user stopped the agent while these edits were being saved, and the save did not report back. The files may or may not have been written; read them again before editing them.";

/// How long a Stop waits for an in-flight file-edit save to report its real outcome before the
/// action is ended with an explicit "outcome unknown" result.
const FILE_EDIT_CANCEL_GRACE: std::time::Duration = std::time::Duration::from_secs(5);

impl AsyncExecutingAction {
    fn is_file_edit_save_in_flight(&self) -> bool {
        // `execution` is set once `execute` has run, i.e. once the save was dispatched.
        matches!(
            self.action.action,
            AIAgentActionType::RequestFileEdits { .. }
        ) && self.execution.is_some()
    }

    fn is_shell_command_action(&self) -> bool {
        matches!(
            self.action.action,
            AIAgentActionType::RequestCommandOutput { .. }
                | AIAgentActionType::WriteToLongRunningShellCommand { .. }
                | AIAgentActionType::ReadShellCommandOutput { .. }
        )
    }
}

pub struct BlocklistAIActionExecutor {
    shell_command_executor: ModelHandle<ShellCommandExecutor>,
    read_files_executor: ModelHandle<ReadFilesExecutor>,
    request_file_edits_executor: ModelHandle<RequestFileEditsExecutor>,
    grep_executor: ModelHandle<GrepExecutor>,
    file_glob_executor: ModelHandle<FileGlobExecutor>,
    read_mcp_resource_executor: ModelHandle<ReadMCPResourceExecutor>,
    call_mcp_tool_executor: ModelHandle<CallMCPToolExecutor>,
    suggest_new_conversation_executor: ModelHandle<SuggestNewConversationExecutor>,
    suggest_prompt_executor: ModelHandle<PromptSuggestionExecutor>,
    read_documents_executor: ModelHandle<ReadDocumentsExecutor>,
    edit_documents_executor: ModelHandle<EditDocumentsExecutor>,
    create_documents_executor: ModelHandle<CreateDocumentsExecutor>,
    use_computer_executor: ModelHandle<UseComputerExecutor>,
    request_computer_use_executor: ModelHandle<RequestComputerUseExecutor>,
    read_skill_executor: ModelHandle<ReadSkillExecutor>,
    ask_user_question_executor: ModelHandle<AskUserQuestionExecutor>,
    send_message_executor: ModelHandle<SendMessageToAgentExecutor>,
    /// The actions currently executing asynchronously, keyed by action ID.
    /// We track them per action rather than as a single slot so multiple actions from the same
    /// parallel phase can complete independently.
    async_executing_actions: std::collections::HashMap<AIAgentActionId, AsyncExecutingAction>,

    /// Reference to the terminal model for checking session sharing state.
    terminal_model: Arc<FairMutex<TerminalModel>>,
}

impl BlocklistAIActionExecutor {
    pub fn new(
        terminal_model: Arc<FairMutex<TerminalModel>>,
        active_session: ModelHandle<ActiveSession>,
        model_event_dispatcher: &ModelHandle<ModelEventDispatcher>,
        terminal_view_id: EntityId,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        let read_files_executor =
            ctx.add_model(|_| ReadFilesExecutor::new(active_session.clone(), terminal_view_id));
        let shell_command_executor = ctx.add_model(|ctx| {
            ShellCommandExecutor::new(
                active_session.clone(),
                terminal_model.clone(),
                model_event_dispatcher,
                terminal_view_id,
                ctx,
            )
        });
        let request_file_edits_executor = ctx.add_model(|ctx| {
            RequestFileEditsExecutor::new(active_session.clone(), terminal_view_id, ctx)
        });
        let grep_executor =
            ctx.add_model(|_| GrepExecutor::new(active_session.clone(), terminal_view_id));
        let file_glob_executor =
            ctx.add_model(|_| FileGlobExecutor::new(active_session.clone(), terminal_view_id));
        let read_mcp_resource_executor = ctx
            .add_model(|_| ReadMCPResourceExecutor::new(active_session.clone(), terminal_view_id));
        let call_mcp_tool_executor =
            ctx.add_model(|_| CallMCPToolExecutor::new(active_session.clone(), terminal_view_id));
        let suggest_new_conversation_executor =
            ctx.add_model(|_| SuggestNewConversationExecutor::new());
        let suggest_prompt_executor = ctx.add_model(|_| PromptSuggestionExecutor::new());
        let read_documents_executor = ctx.add_model(|_| ReadDocumentsExecutor::new());
        let edit_documents_executor = ctx.add_model(|_| EditDocumentsExecutor::new());
        let create_documents_executor = ctx
            .add_model(|_| CreateDocumentsExecutor::new(active_session.clone(), terminal_view_id));
        let use_computer_executor = ctx.add_model(|_| UseComputerExecutor::new());
        let request_computer_use_executor =
            ctx.add_model(|_| RequestComputerUseExecutor::new(terminal_view_id));
        let read_skill_executor = ctx.add_model(|_| ReadSkillExecutor::new(active_session.clone()));
        let ask_user_question_executor =
            ctx.add_model(|_| AskUserQuestionExecutor::new(terminal_view_id));
        let send_message_executor = ctx.add_model(|_| SendMessageToAgentExecutor::new());
        Self {
            shell_command_executor,
            read_files_executor,
            request_file_edits_executor,
            grep_executor,
            file_glob_executor,
            read_mcp_resource_executor,
            call_mcp_tool_executor,
            suggest_new_conversation_executor,
            suggest_prompt_executor,
            read_documents_executor,
            edit_documents_executor,
            create_documents_executor,
            use_computer_executor,
            request_computer_use_executor,
            async_executing_actions: Default::default(),
            terminal_model,
            read_skill_executor,
            ask_user_question_executor,
            send_message_executor,
        }
    }

    pub fn async_executing_action(&self, action_id: &AIAgentActionId) -> Option<&AIAgentAction> {
        self.async_executing_actions
            .get(action_id)
            .map(|running| &running.action)
    }

    pub fn shell_command_executor(&self) -> &ModelHandle<ShellCommandExecutor> {
        &self.shell_command_executor
    }

    pub fn request_file_edits_executor(&self) -> &ModelHandle<RequestFileEditsExecutor> {
        &self.request_file_edits_executor
    }

    pub fn suggest_new_conversation_executor(
        &self,
    ) -> &ModelHandle<SuggestNewConversationExecutor> {
        &self.suggest_new_conversation_executor
    }

    pub fn suggest_prompt_executor(&self) -> &ModelHandle<PromptSuggestionExecutor> {
        &self.suggest_prompt_executor
    }

    pub fn action_phase(&self, action: &AIAgentAction, ctx: &AppContext) -> RunningActionPhase {
        match &action.action {
            AIAgentActionType::ReadFiles(..) | AIAgentActionType::ReadSkill(_) => {
                RunningActionPhase::Parallel(ParallelExecutionPolicy::ReadOnlyLocalContext)
            }
            AIAgentActionType::Grep { .. }
                if self.grep_executor.as_ref(ctx).can_execute_in_parallel(ctx) =>
            {
                RunningActionPhase::Parallel(ParallelExecutionPolicy::ReadOnlyLocalContext)
            }
            AIAgentActionType::FileGlob { .. } | AIAgentActionType::FileGlobV2 { .. }
                if self
                    .file_glob_executor
                    .as_ref(ctx)
                    .can_execute_in_parallel(ctx) =>
            {
                RunningActionPhase::Parallel(ParallelExecutionPolicy::ReadOnlyLocalContext)
            }
            _ => RunningActionPhase::Serial,
        }
    }

    pub fn ask_user_question_executor(&self) -> &ModelHandle<AskUserQuestionExecutor> {
        &self.ask_user_question_executor
    }

    pub fn send_message_executor(&self) -> &ModelHandle<SendMessageToAgentExecutor> {
        &self.send_message_executor
    }

    pub fn set_ambient_agent_task_id(
        &self,
        id: Option<AmbientAgentTaskId>,
        ctx: &mut ModelContext<Self>,
    ) {
        // Two executors consume the ambient task id: `RequestComputerUseExecutor` tags its
        // approval telemetry with it, and `SendMessageToAgentExecutor` uses it as a
        // sender-run-id fallback.
        self.request_computer_use_executor
            .update(ctx, |executor, _| {
                executor.set_ambient_agent_task_id(id);
            });
        self.send_message_executor.update(ctx, |executor, _| {
            executor.set_ambient_agent_task_id(id);
        });
    }

    pub fn preprocess_action(
        &self,
        action: &AIAgentAction,
        conversation_id: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) -> BoxFuture<'static, ()> {
        // In view-only mode, we do not need to perform any preprocessing work.
        if self.is_shared_session_viewer() {
            return futures::future::ready(()).boxed();
        }

        let input = PreprocessActionInput {
            action,
            conversation_id,
        };

        match &action.action {
            AIAgentActionType::RequestCommandOutput { .. }
            | AIAgentActionType::WriteToLongRunningShellCommand { .. }
            | AIAgentActionType::ReadShellCommandOutput { .. }
            | AIAgentActionType::TransferShellCommandControlToUser { .. } => self
                .shell_command_executor
                .update(ctx, |executor, ctx| executor.preprocess_action(input, ctx)),
            AIAgentActionType::ReadFiles(..) => self
                .read_files_executor
                .update(ctx, |executor, ctx| executor.preprocess_action(input, ctx)),
            AIAgentActionType::Grep { .. } => self
                .grep_executor
                .update(ctx, |executor, ctx| executor.preprocess_action(input, ctx)),
            AIAgentActionType::FileGlob { .. } | AIAgentActionType::FileGlobV2 { .. } => self
                .file_glob_executor
                .update(ctx, |executor, ctx| executor.preprocess_action(input, ctx)),
            AIAgentActionType::CallMCPTool { .. } => self
                .call_mcp_tool_executor
                .update(ctx, |executor, ctx| executor.preprocess_action(input, ctx)),
            AIAgentActionType::ReadMCPResource { .. } => self
                .read_mcp_resource_executor
                .update(ctx, |executor, ctx| executor.preprocess_action(input, ctx)),
            // Normally, requested file edits are not handled by the executor. However, when performing a task autonomously,
            // the executor is responsible for auto-approving diffs.
            AIAgentActionType::RequestFileEdits { .. } => self
                .request_file_edits_executor
                .update(ctx, |executor, ctx| executor.preprocess_action(input, ctx)),
            AIAgentActionType::OpenCodeReview => futures::future::ready(()).boxed(),
            AIAgentActionType::InsertCodeReviewComments { .. } => {
                futures::future::ready(()).boxed()
            }
            AIAgentActionType::SuggestNewConversation { .. } => self
                .suggest_new_conversation_executor
                .update(ctx, |executor, ctx| executor.preprocess_action(input, ctx)),
            AIAgentActionType::SuggestPrompt { .. } => self
                .suggest_prompt_executor
                .update(ctx, |executor, ctx| executor.preprocess_action(input, ctx)),
            AIAgentActionType::ReadDocuments(_) => self
                .read_documents_executor
                .update(ctx, |executor, ctx| executor.preprocess_action(input, ctx)),
            AIAgentActionType::EditDocuments(_) => self
                .edit_documents_executor
                .update(ctx, |executor, ctx| executor.preprocess_action(input, ctx)),
            AIAgentActionType::CreateDocuments(_) => self
                .create_documents_executor
                .update(ctx, |executor, ctx| executor.preprocess_action(input, ctx)),
            AIAgentActionType::UseComputer(_) => self
                .use_computer_executor
                .update(ctx, |executor, ctx| executor.preprocess_action(input, ctx)),
            AIAgentActionType::RequestComputerUse(_) => self
                .request_computer_use_executor
                .update(ctx, |executor, ctx| executor.preprocess_action(input, ctx)),
            AIAgentActionType::ReadSkill(_) => self
                .read_skill_executor
                .update(ctx, |executor, ctx| executor.preprocess_action(input, ctx)),
            AIAgentActionType::AskUserQuestion { .. } => self
                .ask_user_question_executor
                .update(ctx, |executor, ctx| executor.preprocess_action(input, ctx)),
            AIAgentActionType::SendMessageToAgent { .. } => self
                .send_message_executor
                .update(ctx, |executor, ctx| executor.preprocess_action(input, ctx)),
        }
    }

    /// Returns `None` if the action was executed (and thereby consumed).
    ///
    /// If the executor cannot execute the action at this time, returns a result indicating why.
    pub fn try_to_execute_action(
        &mut self,
        action: AIAgentAction,
        conversation_id: AIConversationId,
        initiator: ActionInitiator,
        ctx: &mut ModelContext<Self>,
    ) -> TryExecuteResult {
        log::info!(
            "[byop-diag] executor.try_to_execute_action: enter action_id={:?} \
             initiator={initiator:?}",
            action.id
        );
        // We should never actually execute actions in view-only mode.
        if self.is_shared_session_viewer() {
            log::info!(
                "[byop-diag] executor: NotExecuted (shared_session_viewer) action_id={:?}",
                action.id
            );
            return TryExecuteResult::NotExecuted {
                reason: NotExecutedReason::WaitingOnSharer,
                action: Box::new(action),
            };
        }

        let input = ExecuteActionInput {
            action: &action,
            conversation_id,
        };
        let can_auto_execute = self.should_autoexecute(input, ctx);
        let is_agent_autonomous = AppExecutionMode::as_ref(ctx).is_autonomous();
        log::info!(
            "[byop-diag] executor: action_id={:?} can_auto_execute={can_auto_execute} \
             is_agent_autonomous={is_agent_autonomous}",
            action.id
        );

        // Whether this caller may answer the confirmation prompt on this caller's behalf.
        // See `ActionInitiator::can_stand_in_for_confirmation`.
        //
        // A write to a protected path (MCP/agent configs, this app's settings, ssh, shell rc,
        // git hooks, ...: `blocklist::protected_paths`) is never stood in for by anything but
        // the user's own click. `should_autoexecute` already refuses it on the autonomy path
        // (#682).
        let writes_protected_path = self
            .request_file_edits_executor
            .as_ref(ctx)
            .writes_protected_path(input, ctx);
        let confirmation_stood_in_for =
            initiator.can_stand_in_for_confirmation_of_write(&action.action, writes_protected_path);

        // The agent cannot auto execute and either:
        // - the agent is interactive, OR
        // - the agent is autonomous and the action was not requesting command output
        let needs_confirmation = !(confirmation_stood_in_for
            || can_auto_execute
            || (is_agent_autonomous && action.action.is_request_command_output()));
        if needs_confirmation {
            log::info!(
                "[byop-diag] executor: NotExecuted (NeedsConfirmation) action_id={:?} \
                 initiator={initiator:?} can_auto_execute={can_auto_execute}",
                action.id
            );
            return TryExecuteResult::NotExecuted {
                action: Box::new(action),
                reason: NotExecutedReason::NeedsConfirmation,
            };
        } else if !initiator.is_out_of_band() && !can_auto_execute && is_agent_autonomous {
            // It must be the case that the autonomous agent is requesting a denylisted command.
            if let AIAgentActionType::RequestCommandOutput { command, .. } = &action.action {
                let action_id = action.id.clone();
                let result = AIAgentActionResultType::RequestCommandOutput(
                    RequestCommandOutputResult::Denylisted {
                        command: command.clone(),
                    },
                );

                ctx.emit(BlocklistAIActionExecutorEvent::ExecutingAction {
                    action_id: action_id.clone(),
                });
                ctx.emit(BlocklistAIActionExecutorEvent::FinishedAction {
                    result: Arc::new(AIAgentActionResult {
                        id: action_id,
                        task_id: action.task_id.clone(),
                        result,
                    }),
                    conversation_id,
                    cancellation_reason: None,
                });

                return TryExecuteResult::ExecutedSync;
            }
        }

        let action_clone = action.clone();
        let execution = match &action.action {
            AIAgentActionType::RequestCommandOutput { .. }
            | AIAgentActionType::WriteToLongRunningShellCommand { .. }
            | AIAgentActionType::ReadShellCommandOutput { .. }
            | AIAgentActionType::TransferShellCommandControlToUser { .. } => self
                .shell_command_executor
                .update(ctx, |executor, ctx| executor.execute(input, ctx))
                .into(),
            AIAgentActionType::OpenCodeReview => {
                ctx.emit(BlocklistAIActionExecutorEvent::OpenCodeReview(action.id));
                ActionExecution::<()>::Sync(AIAgentActionResultType::OpenCodeReview).into()
            }
            AIAgentActionType::InsertCodeReviewComments {
                repo_path,
                comments,
                base_branch,
            } => {
                if FeatureFlag::PRCommentsSlashCommand.is_enabled() {
                    ctx.emit(BlocklistAIActionExecutorEvent::InsertCodeReviewComments {
                        action_id: action.id,
                        repo_path: repo_path.clone(),
                        comments: comments.clone(),
                        base_branch: base_branch.clone(),
                    });
                }
                ActionExecution::<()>::Sync(AIAgentActionResultType::InsertReviewComments(
                    InsertReviewCommentsResult::Success {
                        repo_path: repo_path.to_string_lossy().to_string(),
                    },
                ))
                .into()
            }
            AIAgentActionType::ReadFiles(..) => self
                .read_files_executor
                .update(ctx, |executor, ctx| executor.execute(input, ctx))
                .into(),
            AIAgentActionType::Grep { .. } => self
                .grep_executor
                .update(ctx, |executor, ctx| executor.execute(input, ctx))
                .into(),
            AIAgentActionType::FileGlob { .. } | AIAgentActionType::FileGlobV2 { .. } => self
                .file_glob_executor
                .update(ctx, |executor, ctx| executor.execute(input, ctx))
                .into(),
            AIAgentActionType::CallMCPTool { .. } => self
                .call_mcp_tool_executor
                .update(ctx, |executor, ctx| executor.execute(input, ctx))
                .into(),
            AIAgentActionType::ReadMCPResource { .. } => self
                .read_mcp_resource_executor
                .update(ctx, |executor, ctx| executor.execute(input, ctx))
                .into(),
            // Normally, requested file edits are not handled by the executor. However, when performing a task autonomously,
            // the executor is responsible for auto-approving diffs.
            AIAgentActionType::RequestFileEdits { .. } => self
                .request_file_edits_executor
                .update(ctx, |executor, ctx| executor.execute(input, ctx))
                .into(),
            AIAgentActionType::SuggestNewConversation { .. } => self
                .suggest_new_conversation_executor
                .update(ctx, |executor, ctx| executor.execute(input, ctx))
                .into(),
            AIAgentActionType::SuggestPrompt { .. } => self
                .suggest_prompt_executor
                .update(ctx, |executor, ctx| executor.execute(input, ctx))
                .into(),
            AIAgentActionType::ReadDocuments(_) => self
                .read_documents_executor
                .update(ctx, |executor, ctx| executor.execute(input, ctx))
                .into(),
            AIAgentActionType::EditDocuments(_) => self
                .edit_documents_executor
                .update(ctx, |executor, ctx| executor.execute(input, ctx))
                .into(),
            AIAgentActionType::CreateDocuments(_) => self
                .create_documents_executor
                .update(ctx, |executor, ctx| {
                    executor.execute(input, conversation_id, ctx)
                })
                .into(),
            AIAgentActionType::UseComputer(_) => self
                .use_computer_executor
                .update(ctx, |executor, ctx| executor.execute(input, ctx))
                .into(),
            AIAgentActionType::RequestComputerUse(_) => self
                .request_computer_use_executor
                .update(ctx, |executor, ctx| executor.execute(input, ctx))
                .into(),
            AIAgentActionType::ReadSkill(_) => self
                .read_skill_executor
                .update(ctx, |executor, ctx| executor.execute(input, ctx))
                .into(),
            AIAgentActionType::AskUserQuestion { .. } => self
                .ask_user_question_executor
                .update(ctx, |executor, ctx| executor.execute(input, ctx))
                .into(),
            AIAgentActionType::SendMessageToAgent { .. } => self
                .send_message_executor
                .update(ctx, |executor, ctx| executor.execute(input, ctx))
                .into(),
        };

        let action_id = action_clone.id.clone();
        log::info!(
            "[byop-diag] executor: dispatched action_id={action_id:?} → execution variant: {}",
            match &execution {
                AnyActionExecution::NotReady => "NotReady",
                AnyActionExecution::InvalidAction => "InvalidAction",
                AnyActionExecution::Async { .. } => "Async",
                AnyActionExecution::Sync(_) => "Sync",
            }
        );
        match execution {
            AnyActionExecution::NotReady => TryExecuteResult::NotExecuted {
                reason: NotExecutedReason::NotReady,
                action: Box::new(action_clone),
            },
            AnyActionExecution::InvalidAction => {
                log::error!(
                    "[byop-diag] executor: InvalidAction action_id={action_id:?} \
                     (wrong executor — wrong tool dispatch)"
                );
                debug_assert!(false, "Tried to execute AIAgentAction with wrong executor.");
                TryExecuteResult::NotExecuted {
                    reason: NotExecutedReason::NotReady,
                    action: Box::new(action_clone),
                }
            }
            AnyActionExecution::Async {
                execute_future,
                on_complete,
            } => {
                ctx.emit(BlocklistAIActionExecutorEvent::ExecutingAction {
                    action_id: action_id.clone(),
                });
                let completing_action_id = action_id.clone();
                let execution = ctx.spawn(execute_future, move |me, result, ctx| {
                    let Some(running) = me.async_executing_actions.remove(&completing_action_id)
                    else {
                        return;
                    };
                    let result = on_complete(result, ctx);
                    ctx.emit(BlocklistAIActionExecutorEvent::FinishedAction {
                        result: Arc::new(AIAgentActionResult {
                            id: completing_action_id,
                            task_id: running.action.task_id,
                            result,
                        }),
                        conversation_id: running.conversation_id,
                        cancellation_reason: running.cancel_pending.flatten(),
                    });
                });
                // Registered after the spawn so the handle can be kept; the future cannot have
                // completed in between, since it only runs once this update returns.
                self.async_executing_actions.insert(
                    action_id,
                    AsyncExecutingAction {
                        action: action_clone,
                        conversation_id,
                        execution: Some(execution),
                        cancel_pending: None,
                    },
                );
                TryExecuteResult::ExecutedAsync
            }
            AnyActionExecution::Sync(action_result) => {
                ctx.emit(BlocklistAIActionExecutorEvent::ExecutingAction {
                    action_id: action_id.clone(),
                });
                ctx.emit(BlocklistAIActionExecutorEvent::FinishedAction {
                    result: Arc::new(AIAgentActionResult {
                        id: action_id,
                        task_id: action.task_id,
                        result: action_result,
                    }),
                    conversation_id,
                    cancellation_reason: None,
                });
                TryExecuteResult::ExecutedSync
            }
        }
    }

    pub fn can_autoexecute_action(
        &self,
        action: &AIAgentAction,
        conversation_id: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) -> bool {
        self.should_autoexecute(
            ExecuteActionInput {
                action,
                conversation_id,
            },
            ctx,
        )
    }

    /// Test-only: records `action` as executing asynchronously without spawning it.
    #[cfg(test)]
    pub(crate) fn insert_async_executing_action_for_test(
        &mut self,
        action: AIAgentAction,
        conversation_id: AIConversationId,
        execution: Option<SpawnedFutureHandle>,
    ) {
        self.async_executing_actions.insert(
            action.id.clone(),
            AsyncExecutingAction {
                action,
                conversation_id,
                execution,
                cancel_pending: None,
            },
        );
    }

    pub fn cancel_running_async_action(
        &mut self,
        action_id: &AIAgentActionId,
        reason: Option<CancellationReason>,
        ctx: &mut ModelContext<Self>,
    ) {
        // A viewer should not be able to cancel an action.
        if self.is_shared_session_viewer() {
            return;
        }
        if let Some(running) = self.async_executing_actions.get_mut(action_id) {
            if running.is_file_edit_save_in_flight() {
                // The write may already be landing and cannot be recalled, so reporting
                // Cancelled could be a lie. Wait (bounded) for the real outcome instead.
                if running.cancel_pending.is_none() {
                    running.cancel_pending = Some(reason);
                    let action_id = action_id.clone();
                    ctx.spawn(
                        async {
                            let _ = warpui::r#async::Timer::after(FILE_EDIT_CANCEL_GRACE).await;
                        },
                        move |me, _, ctx| me.end_unfinished_file_edit(&action_id, ctx),
                    );
                }
                return;
            }
        }
        if let Some(running) = self.async_executing_actions.remove(action_id) {
            if running.is_shell_command_action() {
                // Not aborted: the command keeps running in the shell whatever happens to this
                // future, and the executor's own cancel (an interrupt for a manual stop, the
                // sender cleanup) is what stopping it means. The future ends on its own once
                // its senders are dropped.
                self.shell_command_executor.update(ctx, |executor, ctx| {
                    executor.cancel_execution(&running.action.id, reason, ctx);
                });
            } else if let Some(execution) = &running.execution {
                // Stop the work, not just its result: a cancelled read, search, MCP call or
                // remote request would otherwise run to completion and be thrown away on
                // arrival. Over ssh that is remote work streaming back over the shared
                // connection (#1241). The completion callback never runs for an aborted future,
                // and the cancelled result below is the action's one result.
                execution.abort();
            }
            ctx.emit(BlocklistAIActionExecutorEvent::FinishedAction {
                result: Arc::new(AIAgentActionResult {
                    id: running.action.id.clone(),
                    task_id: running.action.task_id,
                    result: running.action.action.cancelled_result(),
                }),
                conversation_id: running.conversation_id,
                cancellation_reason: reason,
            });
        }
    }

    /// Ends a file-edit action whose save did not report back within [`FILE_EDIT_CANCEL_GRACE`]
    /// of a Stop, with a result that says the outcome is unknown instead of claiming Cancelled.
    fn end_unfinished_file_edit(
        &mut self,
        action_id: &AIAgentActionId,
        ctx: &mut ModelContext<Self>,
    ) {
        let Some(running) = self.async_executing_actions.remove(action_id) else {
            // The real result arrived in time.
            return;
        };
        if let Some(execution) = &running.execution {
            execution.abort();
        }
        ctx.emit(BlocklistAIActionExecutorEvent::FinishedAction {
            result: Arc::new(AIAgentActionResult {
                id: running.action.id.clone(),
                task_id: running.action.task_id,
                result: AIAgentActionResultType::RequestFileEdits(
                    RequestFileEditsResult::DiffApplicationFailed {
                        error: FILE_EDIT_STOPPED_MID_SAVE.to_owned(),
                    },
                ),
            }),
            conversation_id: running.conversation_id,
            cancellation_reason: running.cancel_pending.flatten(),
        });
    }

    /// Drops executor-held per-action state now that the action has reached a
    /// terminal result. Called from the action model's terminal-result choke
    /// point (`handle_action_result`), which every outcome — success, failure,
    /// or cancellation via any path — funnels through.
    ///
    /// Participation is per-executor opt-in: an executor may only be added
    /// here if its per-action state is never read after the action's terminal
    /// result. Prepared file edits qualify (consumed by execute, dead
    /// otherwise). Do NOT add state that outlives completion, e.g. the
    /// long-running-command state (the command outlives its action's snapshot
    /// result).
    pub(super) fn discard_action_state(
        &mut self,
        action_id: &AIAgentActionId,
        ctx: &mut ModelContext<Self>,
    ) {
        self.request_file_edits_executor.update(ctx, |executor, _| {
            executor.discard_pending(action_id);
        });
    }

    pub fn cancel_all_running_async_actions_for_conversation(
        &mut self,
        conversation_id: AIConversationId,
        reason: Option<CancellationReason>,
        ctx: &mut ModelContext<Self>,
    ) {
        let action_ids = self
            .async_executing_actions
            .iter()
            .filter_map(|(action_id, running)| {
                (running.conversation_id == conversation_id).then_some(action_id.clone())
            })
            .collect::<Vec<_>>();
        for action_id in action_ids {
            self.cancel_running_async_action(&action_id, reason, ctx);
        }
    }

    fn should_autoexecute(&self, input: ExecuteActionInput, ctx: &mut ModelContext<Self>) -> bool {
        match input.action.action {
            AIAgentActionType::RequestCommandOutput { .. }
            | AIAgentActionType::WriteToLongRunningShellCommand { .. }
            | AIAgentActionType::ReadShellCommandOutput { .. }
            | AIAgentActionType::TransferShellCommandControlToUser { .. } => self
                .shell_command_executor
                .update(ctx, |executor, ctx| executor.should_autoexecute(input, ctx)),
            AIAgentActionType::ReadFiles(_) => self
                .read_files_executor
                .update(ctx, |executor, ctx| executor.should_autoexecute(input, ctx)),
            AIAgentActionType::RequestFileEdits { .. } => self
                .request_file_edits_executor
                .update(ctx, |executor, ctx| executor.should_autoexecute(input, ctx)),
            AIAgentActionType::Grep { .. } => self
                .grep_executor
                .update(ctx, |executor, ctx| executor.should_autoexecute(input, ctx)),
            AIAgentActionType::FileGlob { .. } | AIAgentActionType::FileGlobV2 { .. } => self
                .file_glob_executor
                .update(ctx, |executor, ctx| executor.should_autoexecute(input, ctx)),
            AIAgentActionType::CallMCPTool { .. } => self
                .call_mcp_tool_executor
                .update(ctx, |executor, ctx| executor.should_autoexecute(input, ctx)),
            AIAgentActionType::ReadMCPResource { .. } => self
                .read_mcp_resource_executor
                .update(ctx, |executor, ctx| executor.should_autoexecute(input, ctx)),
            AIAgentActionType::OpenCodeReview => true,
            AIAgentActionType::InsertCodeReviewComments { .. } => true,
            AIAgentActionType::SuggestNewConversation { .. } => self
                .suggest_new_conversation_executor
                .update(ctx, |executor, ctx| executor.should_autoexecute(input, ctx)),
            AIAgentActionType::SuggestPrompt { .. } => self
                .suggest_prompt_executor
                .update(ctx, |executor, ctx| executor.should_autoexecute(input, ctx)),
            AIAgentActionType::ReadDocuments(_) => self
                .read_documents_executor
                .update(ctx, |executor, ctx| executor.should_autoexecute(input, ctx)),
            AIAgentActionType::EditDocuments(_) => self
                .edit_documents_executor
                .update(ctx, |executor, ctx| executor.should_autoexecute(input, ctx)),
            AIAgentActionType::CreateDocuments(_) => self
                .create_documents_executor
                .update(ctx, |executor, ctx| executor.should_autoexecute(input, ctx)),
            AIAgentActionType::UseComputer(_) => self
                .use_computer_executor
                .update(ctx, |executor, ctx| executor.should_autoexecute(input, ctx)),
            AIAgentActionType::RequestComputerUse(_) => self
                .request_computer_use_executor
                .update(ctx, |executor, ctx| executor.should_autoexecute(input, ctx)),
            AIAgentActionType::ReadSkill(_) => self
                .read_skill_executor
                .update(ctx, |executor, ctx| executor.should_autoexecute(input, ctx)),
            AIAgentActionType::AskUserQuestion { .. } => self
                .ask_user_question_executor
                .update(ctx, |executor, ctx| executor.should_autoexecute(input, ctx)),
            AIAgentActionType::SendMessageToAgent { .. } => self
                .send_message_executor
                .update(ctx, |executor, ctx| executor.should_autoexecute(input, ctx)),
        }
    }

    fn is_shared_session_viewer(&self) -> bool {
        self.terminal_model.lock().is_shared_session_viewer()
    }
}
impl Entity for BlocklistAIActionExecutor {
    type Event = BlocklistAIActionExecutorEvent;
}

pub enum BlocklistAIActionExecutorEvent {
    /// Emitted when an action is execution starts.
    ExecutingAction {
        action_id: AIAgentActionId,
    },

    /// Emitted when an action has finished.
    FinishedAction {
        result: Arc<AIAgentActionResult>,
        conversation_id: AIConversationId,
        /// The reason for cancellation, if this action was cancelled.
        cancellation_reason: Option<CancellationReason>,
    },

    OpenCodeReview(AIAgentActionId),
    InsertCodeReviewComments {
        action_id: AIAgentActionId,
        repo_path: PathBuf,
        comments: Vec<ai::agent::action::InsertReviewComment>,
        base_branch: Option<String>,
    },
}

/// Per-file byte limit for [`read_local_file_context`]. Binary files larger
/// than this are skipped; text files are truncated at this limit.
#[cfg(feature = "local_fs")]
const MAX_FILE_READ_BYTES: usize = 1_000_000;

/// The results of a [`read_local_file_context`] call.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct ReadFileContextResult {
    /// [`FileContext`] data for all files that could be read.
    pub file_contexts: Vec<FileContext>,

    /// Requested files that could not be read, each paired with a reason-specific
    /// failure message (missing, too large, or unprocessable).
    pub failed_files: Vec<ReadFilesFailedFile>,
}

/// Builds a single, reason-accurate summary from a batch of file-read failures,
/// one `path: reason` entry per file. Shared by the agent-tool consumers so they
/// all surface the same per-file reason instead of a flat "do not exist" list.
pub fn describe_failed_files(failed_files: &[ReadFilesFailedFile]) -> String {
    failed_files
        .iter()
        .map(|failed_file| format!("{}: {}", failed_file.path, failed_file.message))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Total content budget of one agent `read_files` call. Without a cap a large batch only
/// stopped at the 64 MB protocol limit (and then a 120 s timeout); with it, files past the
/// budget come back as explicit per-file failures (#1278).
pub const AGENT_READ_FILES_BATCH_BYTES: usize = 4_000_000;

/// Most files one agent `read_files` call reads; the rest are reported, not dropped (#1278).
pub const AGENT_READ_FILES_MAX_FILES: usize = 100;

/// Splits an agent `read_files` batch at [`AGENT_READ_FILES_MAX_FILES`]. The second value is
/// a failed-file entry saying how many files were not read, `None` when everything fits.
pub fn cap_read_batch(
    mut locations: Vec<FileLocations>,
    max_files: usize,
) -> (Vec<FileLocations>, Option<ReadFilesFailedFile>) {
    if locations.len() <= max_files {
        return (locations, None);
    }
    let skipped = locations.split_off(max_files);
    let note = ReadFilesFailedFile {
        path: format!("({} more files)", skipped.len()),
        message: format!(
            "{} more files not read: one read_files call reads at most {max_files} files. \
             Request these in a separate call: {}.",
            skipped.len(),
            skipped
                .iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    (locations, Some(note))
}

/// Turns a remote server's file contexts into agent file contexts plus per-file failures. An
/// entry that carries no content used to vanish from both lists, so the model never learned
/// the file was not read (#1278).
pub fn convert_remote_file_contexts(
    file_contexts: Vec<remote_server::proto::FileContextProto>,
) -> (Vec<FileContext>, Vec<ReadFilesFailedFile>) {
    let mut files = Vec::new();
    let mut failed = Vec::new();
    for fc in file_contexts {
        let Some(content) = fc.content else {
            failed.push(ReadFilesFailedFile {
                path: fc.file_name,
                message: "The remote host returned no content for this file; it was not read."
                    .to_string(),
            });
            continue;
        };
        let content = match content {
            remote_server::proto::file_context_proto::Content::TextContent(text) => {
                AnyFileContent::StringContent(text)
            }
            remote_server::proto::file_context_proto::Content::BinaryContent(bytes) => {
                AnyFileContent::BinaryContent(bytes)
            }
        };
        let line_range = match (fc.line_range_start, fc.line_range_end) {
            (Some(start), Some(end)) => Some(start as usize..end as usize),
            _ => None,
        };
        let last_modified = fc
            .last_modified_epoch_millis
            .map(|ms| std::time::UNIX_EPOCH + std::time::Duration::from_millis(ms));
        files.push(FileContext {
            file_name: fc.file_name,
            content,
            line_range,
            last_modified,
            line_count: fc.line_count as usize,
        });
    }
    (files, failed)
}

/// Reads the content of the given files at the given `FileLocations`.
///
/// If any files cannot be read, they are included in the `failed_files` field of the
/// result, each carrying the reason it failed.
///
/// Binary files larger than the per-file byte limit are skipped and reported as oversized.
/// Text files are truncated at the per-file limit via line streaming.
/// If `max_file_bytes` is provided, it overrides the default per-file limit
/// ([`MAX_FILE_READ_BYTES`]). Pass `None` to use the default.
/// If `max_batch_bytes` is provided, the cumulative content of all files is capped at that
/// budget; once exceeded, remaining files are reported as oversized.
#[cfg_attr(not(feature = "local_fs"), allow(unused_variables))]
pub async fn read_local_file_context(
    file_names: &[FileLocations],
    current_working_directory: Option<String>,
    shell: Option<ShellLaunchData>,
    max_file_bytes: Option<usize>,
    max_batch_bytes: Option<usize>,
) -> anyhow::Result<ReadFileContextResult> {
    #[cfg(not(feature = "local_fs"))]
    return Err(anyhow::anyhow!(
        "Can't read files when not on a local filesystem"
    ));

    #[cfg(feature = "local_fs")]
    {
        let mut result = ReadFileContextResult {
            file_contexts: Vec::new(),
            failed_files: Vec::new(),
        };

        let mut batch_bytes_remaining = max_batch_bytes;

        for file in file_names {
            let absolute_file_path = PathBuf::from(host_native_absolute_path(
                &file.name,
                &shell,
                &current_working_directory,
            ));

            // The batch budget is spent: say so for this file instead of reading it with a
            // zero-byte allowance and returning an empty or silently cut entry (#1278).
            if batch_bytes_remaining == Some(0) {
                result.failed_files.push(ReadFilesFailedFile {
                    path: absolute_file_path.to_string_lossy().to_string(),
                    message: "Not read: this call's total size limit was reached. Request \
                              this file in a separate read_files call."
                        .to_string(),
                });
                continue;
            }

            let metadata = match async_fs::metadata(&absolute_file_path).await {
                Ok(m) => m,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    result.failed_files.push(ReadFilesFailedFile {
                        path: absolute_file_path.to_string_lossy().to_string(),
                        message: "File does not exist".to_string(),
                    });
                    continue;
                }
                // One unreadable path (permission denied, a directory, ...) is that file's
                // failure, not the batch's: the files already read still go to the model.
                Err(e) => {
                    result.failed_files.push(ReadFilesFailedFile {
                        path: absolute_file_path.to_string_lossy().to_string(),
                        message: format!("Could not read file: {e}"),
                    });
                    continue;
                }
            };
            let last_modified = metadata.modified().ok();
            let file_size = metadata.len() as usize;
            let path_str = absolute_file_path.to_string_lossy().to_string();

            // Effective byte budget: the tighter of per-file and remaining batch.
            let per_file_limit = max_file_bytes.unwrap_or(MAX_FILE_READ_BYTES);
            let effective_max = match batch_bytes_remaining {
                Some(remaining) => per_file_limit.min(remaining),
                None => per_file_limit,
            };

            // Decide text vs binary before opening the file. Extension-based
            // detection alone is wrong for extensionless text files (e.g. shell
            // scripts named `bundle`, `run`), so for the ambiguous case we fall
            // back to content-based inspection of the first chunk. The binary
            // path below still acts as a safety net via
            // `TextFileReadResult::NotText` if the text reader trips on invalid
            // UTF-8.
            if !should_read_as_binary(&absolute_file_path).await {
                let text_read = match FileModel::read_text_file(
                    &absolute_file_path,
                    effective_max,
                    &file.lines,
                    last_modified,
                )
                .await
                {
                    Ok(text_read) => text_read,
                    Err(e) => {
                        result.failed_files.push(ReadFilesFailedFile {
                            path: path_str,
                            message: format!("Could not read file: {e}"),
                        });
                        continue;
                    }
                };
                match text_read {
                    TextFileReadResult::Segments {
                        segments,
                        bytes_read,
                    } => {
                        if let Some(remaining) = &mut batch_bytes_remaining {
                            *remaining = remaining.saturating_sub(bytes_read);
                        }
                        result
                            .file_contexts
                            .extend(segments.into_iter().map(|seg| FileContext {
                                file_name: seg.file_name,
                                content: AnyFileContent::StringContent(seg.content),
                                line_range: seg.line_range,
                                last_modified: seg.last_modified,
                                line_count: seg.line_count,
                            }));
                        continue;
                    }
                    TextFileReadResult::NotText => {
                        // Fall through to binary path below.
                    }
                }
            }

            // Binary path (either detected as binary, or text reading failed).
            let binary_read = match read_binary_file_context(
                &absolute_file_path,
                effective_max,
                file_size,
                last_modified,
            )
            .await
            {
                Ok(binary_read) => binary_read,
                Err(e) => {
                    result.failed_files.push(ReadFilesFailedFile {
                        path: path_str,
                        message: format!("Could not read file: {e}"),
                    });
                    continue;
                }
            };
            match binary_read {
                BinaryFileReadResult::Context {
                    file_context,
                    bytes_read,
                } => {
                    if let Some(remaining) = &mut batch_bytes_remaining {
                        *remaining = remaining.saturating_sub(bytes_read);
                    }
                    result.file_contexts.push(file_context);
                }
                BinaryFileReadResult::NotFound => result.failed_files.push(ReadFilesFailedFile {
                    path: path_str,
                    message: "File does not exist".to_string(),
                }),
                BinaryFileReadResult::TooLarge {
                    size_bytes,
                    limit_bytes,
                } => result.failed_files.push(ReadFilesFailedFile {
                    path: path_str,
                    message: format!(
                        "File is too large to read ({} > {} limit). Downscale/compress it or read a smaller copy.",
                        format_mb(size_bytes),
                        format_mb(limit_bytes)
                    ),
                }),
                BinaryFileReadResult::ProcessingFailed { detail } => {
                    result.failed_files.push(ReadFilesFailedFile {
                        path: path_str,
                        message: format!("File could not be processed as an image: {detail}"),
                    })
                }
            }
        }

        Ok(result)
    }
}

/// Returns `true` if the file at `path` should be read via the binary code
/// path in [`read_local_file_context`], `false` if it should be read as text.
///
/// Uses extension-based detection as a fast path and falls back to content
/// inspection (reading the first 1 KiB) for extensionless files whose names
/// don't match any known text or binary pattern. Without the content-based
/// fallback, extensionless text files (e.g. shell scripts named `bundle`)
/// would be incorrectly classified as binary and returned to the agent as
/// raw bytes instead of UTF-8 text.
#[cfg(feature = "local_fs")]
async fn should_read_as_binary(path: &std::path::Path) -> bool {
    // Fast path: extension/filename clearly indicates text.
    if !is_binary_file(path) {
        return false;
    }
    // Fast path: file has a known binary extension (e.g. `.png`, `.exe`).
    if path.extension().is_some() {
        return true;
    }
    // Extensionless file with an unknown basename. Inspect the first chunk of
    // the file to decide. Treat open/read errors as binary so the binary path
    // takes over and reports a consistent error.
    is_file_content_binary_async(path).await
}

/// Async sibling of [`warp_util::file_type::is_file_content_binary`]. Reads
/// the first 1 KiB of `path` asynchronously and returns `true` if the content
/// looks binary according to [`is_buffer_binary`]. Returns `true` on any I/O
/// error so callers default to the binary code path. Kept local to this
/// module so `warp_util` doesn't need to grow an `async_fs` dependency.
#[cfg(feature = "local_fs")]
async fn is_file_content_binary_async(path: &std::path::Path) -> bool {
    const CHUNK_SIZE: usize = 1024;

    let Ok(mut file) = async_fs::File::open(path).await else {
        return true;
    };
    let mut buffer = [0u8; CHUNK_SIZE];
    let Ok(n) = file.read(&mut buffer).await else {
        return true;
    };
    is_buffer_binary(&buffer[..n])
}

/// Renders a byte count in megabytes with one decimal place (e.g. `3.5 MB`),
/// matching the units used in the "too large" failure message.
#[cfg(feature = "local_fs")]
fn format_mb(bytes: usize) -> String {
    format!("{:.1} MB", bytes as f64 / 1_000_000.0)
}

#[cfg(feature = "local_fs")]
enum BinaryFileReadResult {
    /// Successfully read as binary.
    Context {
        file_context: FileContext,
        bytes_read: usize,
    },
    /// The file does not exist on disk.
    NotFound,
    /// The file exists but exceeds the effective byte limit.
    TooLarge {
        size_bytes: usize,
        limit_bytes: usize,
    },
    /// The file could not be processed (e.g. an image decode/resize failed, or
    /// the processed image is still too large to send).
    ProcessingFailed { detail: String },
}

/// Reads a binary file, applying image processing when applicable.
#[cfg(feature = "local_fs")]
async fn read_binary_file_context(
    path: &std::path::Path,
    max_bytes: usize,
    file_size: usize,
    last_modified: Option<std::time::SystemTime>,
) -> anyhow::Result<BinaryFileReadResult> {
    if file_size > max_bytes {
        return Ok(BinaryFileReadResult::TooLarge {
            size_bytes: file_size,
            limit_bytes: max_bytes,
        });
    }

    let content = match read_file_as_binary(path).await {
        Ok(content) => content,
        Err(FileLoadError::DoesNotExist) => return Ok(BinaryFileReadResult::NotFound),
        Err(FileLoadError::IOError(e)) => return Err(anyhow::anyhow!(e)),
    };

    let mime_type = from_path(path).first_or_octet_stream().to_string();
    let processed_content = if is_supported_image_mime_type(&mime_type) {
        match process_image_for_agent(&content) {
            ProcessImageResult::Success { data } => Some(data),
            ProcessImageResult::TooLarge => {
                log::warn!("Image file too large after processing: {}", path.display());
                return Ok(BinaryFileReadResult::ProcessingFailed {
                    detail: "the image is too large to send even after resizing".to_string(),
                });
            }
            ProcessImageResult::Error(err) => {
                log::warn!("Error processing image file {}: {err:?}", path.display());
                return Ok(BinaryFileReadResult::ProcessingFailed {
                    detail: err.to_string(),
                });
            }
        }
    } else {
        None
    };

    let final_content = processed_content.unwrap_or(content);
    if final_content.len() > max_bytes {
        return Ok(BinaryFileReadResult::TooLarge {
            size_bytes: final_content.len(),
            limit_bytes: max_bytes,
        });
    }

    let bytes_read = final_content.len();
    Ok(BinaryFileReadResult::Context {
        file_context: FileContext::new(
            path.to_string_lossy().to_string(),
            AnyFileContent::BinaryContent(final_content),
            None,
            last_modified,
        ),
        bytes_read,
    })
}

/// Builds the "is this a file?" probe with the path passed as data, not shell syntax.
fn build_is_file_path_command(path: &str, shell_type: ShellType) -> String {
    let escaped_path = shell_quote_arg(path, shell_type);
    if shell_type == ShellType::PowerShell {
        format!(
            "if (Microsoft.PowerShell.Management\\Test-Path -PathType Leaf {escaped_path}) {{ exit 0 }} else {{ exit 1 }}"
        )
    } else {
        format!("test -f {escaped_path}")
    }
}

/// Builds the git-repository probe with the path passed as data, not shell syntax.
fn build_is_git_repository_command(absolute_path: &str, shell_type: ShellType) -> String {
    format!(
        "git -C {} rev-parse",
        shell_quote_arg(absolute_path, shell_type)
    )
}

/// Returns true if the given path is a regular file on the session's filesystem.
/// Runs a shell command on the session so it works for both local and remote sessions.
async fn is_file_path(path: &str, session: &Session) -> bool {
    let command = build_is_file_path_command(path, session.shell().shell_type().executor_syntax());
    session
        .execute_command(&command, None, None, ExecuteCommandOptions::default())
        .await
        .map(|output| output.success())
        .unwrap_or(false)
}

/// How long a "is this directory in a git repository?" answer is reused.
///
/// An agent turn typically fires several grep/glob calls against the same repo within a
/// second or two; each uncached probe is a full command round trip (a whole ssh RTT on a
/// remote session). Kept short so `git init` / deleting `.git` is noticed almost at once.
const GIT_REPO_PROBE_TTL: std::time::Duration = std::time::Duration::from_secs(5);
/// Hard bound on cached probes so a long agent run over many directories cannot grow it.
const GIT_REPO_PROBE_CACHE_MAX: usize = 256;

type GitRepoProbeCache =
    std::collections::HashMap<(warp_core::SessionId, String), (std::time::Instant, bool)>;

static GIT_REPO_PROBE_CACHE: std::sync::LazyLock<parking_lot::Mutex<GitRepoProbeCache>> =
    std::sync::LazyLock::new(Default::default);

fn cached_git_repo_probe(
    cache: &GitRepoProbeCache,
    key: &(warp_core::SessionId, String),
    now: std::time::Instant,
) -> Option<bool> {
    let (at, value) = cache.get(key)?;
    (now.saturating_duration_since(*at) < GIT_REPO_PROBE_TTL).then_some(*value)
}

fn store_git_repo_probe(
    cache: &mut GitRepoProbeCache,
    key: (warp_core::SessionId, String),
    value: bool,
    now: std::time::Instant,
) {
    if cache.len() >= GIT_REPO_PROBE_CACHE_MAX {
        cache.retain(|_, (at, _)| now.saturating_duration_since(*at) < GIT_REPO_PROBE_TTL);
        if cache.len() >= GIT_REPO_PROBE_CACHE_MAX {
            cache.clear();
        }
    }
    cache.insert(key, (now, value));
}

/// Returns true if git is installed and the given path is in a git repository.
///
/// Successful answers are cached per (session, path) for [`GIT_REPO_PROBE_TTL`]; errors are
/// never cached.
async fn is_git_repository(absolute_path: &str, session: &Session) -> anyhow::Result<bool> {
    let key = (session.id(), absolute_path.to_owned());
    if let Some(hit) = cached_git_repo_probe(
        &GIT_REPO_PROBE_CACHE.lock(),
        &key,
        std::time::Instant::now(),
    ) {
        return Ok(hit);
    }
    let git_command = build_is_git_repository_command(
        absolute_path,
        session.shell().shell_type().executor_syntax(),
    );
    let command_output = session
        .execute_command(
            git_command.as_str(),
            None,
            None,
            ExecuteCommandOptions::default(),
        )
        .await?;
    let result = command_output.success();
    store_git_repo_probe(
        &mut GIT_REPO_PROBE_CACHE.lock(),
        key,
        result,
        std::time::Instant::now(),
    );
    Ok(result)
}

fn get_server_output_id(
    conversation_id: AIConversationId,
    ctx: &mut AppContext,
) -> Option<ServerOutputId> {
    BlocklistAIHistoryModel::as_ref(ctx)
        .conversation(&conversation_id)?
        .latest_exchange()?
        .output_status
        .server_output_id()
}

#[cfg(feature = "local_fs")]
async fn read_file_as_binary(file_path: &std::path::Path) -> Result<Vec<u8>, FileLoadError> {
    if !FileModel::file_exists(file_path).await {
        return Err(FileLoadError::DoesNotExist);
    }

    async_fs::read(file_path).await.map_err(FileLoadError::from)
}

/// Expands a leading `~` in a model-supplied path against the home of the host the session's
/// shell runs on. A local session is returned unchanged (the ordinary local expansion happens
/// later); a remote one must not expand against the LOCAL home (#1278). A remote session whose
/// home is unknown is also returned unchanged.
pub(super) fn expand_session_tilde(
    active_session: &ActiveSession,
    path: &str,
    ctx: &AppContext,
) -> String {
    if !is_remote_session(active_session, ctx) {
        return path.to_owned();
    }
    crate::ai::paths::expand_tilde_against(
        path,
        remote_session_home(active_session, ctx).as_deref(),
    )
}

fn is_remote_session(active_session: &ActiveSession, ctx: &AppContext) -> bool {
    matches!(
        active_session.session_type(ctx),
        Some(crate::terminal::model::session::SessionType::WarpifiedRemote { .. })
    )
}

fn remote_session_home(active_session: &ActiveSession, ctx: &AppContext) -> Option<String> {
    active_session
        .session(ctx)
        .and_then(|session| session.home_dir().map(str::to_owned))
}

/// Whether `path` starts with a `~` that [`expand_session_tilde`] cannot resolve: a remote
/// session whose home is unknown. The permission check would then judge the LOCAL home's
/// path while the action runs on the remote host, so callers must not auto-run (#1288).
pub(super) fn session_tilde_unresolved(
    active_session: &ActiveSession,
    path: &str,
    ctx: &AppContext,
) -> bool {
    crate::ai::paths::path_needs_home(path)
        && is_remote_session(active_session, ctx)
        && remote_session_home(active_session, ctx).is_none_or(|home| home.is_empty())
}

#[cfg(all(test, feature = "local_fs"))]
#[path = "execute_tests.rs"]
mod tests;

/// Inline because the natural home, `execute_tests.rs`, is gated on
/// `feature = "local_fs"` and these assertions are about a pure decision function that has
/// nothing to do with the filesystem.
#[cfg(test)]
mod initiator_tests {
    use super::ActionInitiator;
    use crate::ai::agent::{
        AIAgentActionType, FileEdit, RequestComputerUseRequest, UseComputerRequest,
    };
    use crate::ai::blocklist::permissions::file_edits_touch_protected_path;
    use ai::diff_validation::ParsedDiff;

    fn use_computer() -> AIAgentActionType {
        AIAgentActionType::UseComputer(UseComputerRequest {
            action_summary: "Click the button".to_owned(),
            actions: Vec::new(),
            screenshot_params: None,
        })
    }

    fn request_computer_use() -> AIAgentActionType {
        AIAgentActionType::RequestComputerUse(RequestComputerUseRequest {
            task_summary: "Drive the browser".to_owned(),
            screenshot_params: None,
        })
    }

    /// The questions are irrelevant to the verdict: what matters is that the action's
    /// "confirmation" is the user's answer, whatever it asks.
    fn ask_user_question() -> AIAgentActionType {
        AIAgentActionType::AskUserQuestion {
            questions: Vec::new(),
        }
    }

    fn shell_command() -> AIAgentActionType {
        AIAgentActionType::RequestCommandOutput {
            command: "free -h".to_owned(),
            is_read_only: Some(true),
            is_risky: Some(false),
            wait_until_completion: true,
            uses_pager: Some(false),
            rationale: None,
            citations: Vec::new(),
        }
    }

    /// The agent never answers its own confirmation prompt. This is the arm that makes
    /// `should_autoexecute` and the execution profile the deciders on the ordinary path.
    #[test]
    fn the_agent_never_stands_in_for_a_confirmation() {
        assert!(!ActionInitiator::Agent.can_stand_in_for_confirmation(&shell_command()));
        assert!(!ActionInitiator::Agent.can_stand_in_for_confirmation(&use_computer()));
        assert!(!ActionInitiator::Agent.can_stand_in_for_confirmation(&request_computer_use()));
    }

    /// A real click on this action's own Accept button is the confirmation, so it stands in
    /// for anything. Narrowing this arm would strand an approved `use_computer` in a
    /// confirmation loop the user cannot escape by clicking Accept again.
    #[test]
    fn a_user_click_stands_in_for_any_confirmation() {
        assert!(ActionInitiator::User.can_stand_in_for_confirmation(&shell_command()));
        assert!(ActionInitiator::User.can_stand_in_for_confirmation(&use_computer()));
        assert!(ActionInitiator::User.can_stand_in_for_confirmation(&request_computer_use()));
    }

    /// The question's confirmation is the user's answer, which only its own view collects.
    #[test]
    fn only_the_user_answers_a_question() {
        assert!(
            ActionInitiator::User.can_stand_in_for_confirmation(&ask_user_question()),
            "the question view submits the answers and then executes as the user"
        );
        assert!(!ActionInitiator::Agent.can_stand_in_for_confirmation(&ask_user_question()));
    }

    fn protected_move() -> AIAgentActionType {
        AIAgentActionType::RequestFileEdits {
            file_edits: vec![FileEdit::Edit(ParsedDiff::V4AEdit {
                file: Some("notes.md".to_owned()),
                move_to: Some("~/.claude.json".to_owned()),
                hunks: Vec::new(),
            })],
            title: None,
        }
    }

    /// A protected write needs the user's own click (#682); the agent is refused.
    #[test]
    fn only_a_user_click_approves_a_protected_write() {
        let edit = protected_move();
        assert!(!ActionInitiator::Agent.can_stand_in_for_confirmation_of_write(&edit, true));
        assert!(
            ActionInitiator::User.can_stand_in_for_confirmation_of_write(&edit, true),
            "the user's own click on the edit is the explicit confirmation"
        );
    }

    /// The executor feeds that verdict from the same guard `should_autoexecute` uses, so the
    /// destination of a move — not only its source — must register as protected.
    #[test]
    fn a_move_onto_a_protected_path_is_detected() {
        let AIAgentActionType::RequestFileEdits { file_edits, .. } = protected_move() else {
            unreachable!()
        };
        let session = crate::ai::blocklist::SessionContext::new_local_with_cwd_for_test("/project");
        assert!(file_edits_touch_protected_path(&file_edits, &session));

        let ordinary = [FileEdit::Edit(ParsedDiff::V4AEdit {
            file: Some("src/a.rs".to_owned()),
            move_to: Some("src/b.rs".to_owned()),
            hunks: Vec::new(),
        })];
        assert!(!file_edits_touch_protected_path(&ordinary, &session));
    }

    /// A user accept dequeues one specific action rather than draining the queue, so it keeps
    /// the one-at-a-time serialisation in `start_pending_action_by_id`.
    #[test]
    fn a_user_accept_serialises() {
        assert!(ActionInitiator::User.is_out_of_band());
        assert!(!ActionInitiator::Agent.is_out_of_band());
    }
}

#[cfg(test)]
mod git_repo_probe_cache_tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn key(dir: &str) -> (warp_core::SessionId, String) {
        (warp_core::SessionId::default(), dir.to_owned())
    }

    #[test]
    fn hit_within_ttl_miss_after() {
        let mut cache = GitRepoProbeCache::new();
        let t0 = Instant::now();
        store_git_repo_probe(&mut cache, key("/r"), true, t0);
        assert_eq!(
            cached_git_repo_probe(&cache, &key("/r"), t0 + Duration::from_secs(1)),
            Some(true)
        );
        assert_eq!(
            cached_git_repo_probe(&cache, &key("/r"), t0 + GIT_REPO_PROBE_TTL),
            None
        );
        assert_eq!(cached_git_repo_probe(&cache, &key("/other"), t0), None);
    }

    #[test]
    fn negative_results_are_cached_too() {
        let mut cache = GitRepoProbeCache::new();
        let t0 = Instant::now();
        store_git_repo_probe(&mut cache, key("/n"), false, t0);
        assert_eq!(cached_git_repo_probe(&cache, &key("/n"), t0), Some(false));
    }

    #[test]
    fn cache_is_bounded() {
        let mut cache = GitRepoProbeCache::new();
        let t0 = Instant::now();
        for i in 0..(GIT_REPO_PROBE_CACHE_MAX * 3) {
            store_git_repo_probe(&mut cache, key(&format!("/d{i}")), true, t0);
        }
        assert!(cache.len() <= GIT_REPO_PROBE_CACHE_MAX);
    }
}
