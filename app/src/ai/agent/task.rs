pub mod helper;
pub mod transaction;

use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    ops::Deref,
};

use ai::skills::SkillPathOrigin;
use chrono::DateTime;
use field_mask::{FieldMaskError, FieldMaskOperation};
use helper::{MessageExt, SubagentExt, ToolCallExt};
use itertools::Itertools;
use prost_types::FieldMask;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use warp_multi_agent_api::{
    self as api,
    message::{tool_call::subagent::Metadata, Message},
};

use crate::{
    ai::{
        agent::comment::CodeReview,
        document::ai_document_model::{AIDocumentId, AIDocumentVersion},
    },
    server::datetime_ext::DateTimeExt,
    terminal::model::block::BlockId,
    AIAgentTodoList,
};

use super::{
    api::{
        convert_conversation::convert_tool_call_result_to_input, user_inputs_from_messages,
        ConversionParams, ConvertAPIMessageToClientOutputMessage,
    },
    conversation::{context_in_exchanges, update_todo_list_from_todo_op},
    util::{parse_markdown_into_text_and_code_sections, IncrementalTextSections},
    AIAgentContext, AIAgentExchange, AIAgentExchangeId, AIAgentOutput, AIAgentOutputMessage,
    AIAgentOutputMessageType, AIAgentOutputStatus, AIAgentTextSection, MaybeAIAgentOutputMessage,
    MessageId, MessageToAIAgentOutputMessageError, Shared,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(String);

impl TaskId {
    pub fn new(id: String) -> Self {
        TaskId(id)
    }
}

impl From<TaskId> for String {
    fn from(id: TaskId) -> Self {
        id.0
    }
}

impl Deref for TaskId {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum UpdateTaskError {
    #[error("Task never initialized with CreateTask client action.")]
    TaskNotInitialized,
    #[error("Message not found")]
    MessageNotFound,
    #[error("Field mask operation failed: {0:#}")]
    FieldMask(#[from] FieldMaskError),
    #[error("Exchange not found.")]
    ExchangeNotFound,
    #[error("Attempted to update already-finished output.")]
    OutputAlreadyFinished,
    #[error("Attempted to update output that was never initialized.")]
    OutputNeverInitialized,
    #[error("Failed to convert API message to client type: {0}")]
    ConversionError(#[from] MessageToAIAgentOutputMessageError),
}

#[derive(Debug, thiserror::Error)]
pub enum ExtractMessagesError {
    #[error("Task never initialized with CreateTask client action.")]
    TaskNotInitialized,
    #[error("First message not found: {0}")]
    FirstMessageNotFound(String),
    #[error("Last message not found: {0}")]
    LastMessageNotFound(String),
    #[error("Invalid range: first message appears after last message")]
    InvalidRange,
    #[error("Checksum mismatch: expected {expected} messages, found {actual}")]
    ChecksumMismatch { expected: u32, actual: u32 },
}

#[derive(Debug, thiserror::Error)]
pub enum UpgradeOptimisticTaskError {
    #[error("Attempted to upgrade optimistic root task with parent.")]
    RootWithUnexpectedParent,
    #[error("Attempted to upgrade optimistic CLI subagent task with no parent.")]
    CLISubagentMissingParent,
    #[error(
        "Attempted to upgrade optimistic CLI subagent task for subtask with no CLI subagent call."
    )]
    CLISubagentMissingSubagentCall,
    #[error("Attempted to upgrade task with server data.")]
    UnexpectedUpgrade,
}

#[derive(Debug, Clone)]
pub(super) struct SubagentParams {
    pub(super) tool_call_id: String,
    pub(super) call: api::message::tool_call::Subagent,
}

#[derive(Debug, Clone)]
struct ServerTask {
    source: api::Task,
    subagent_params: Option<SubagentParams>,
}

mod optimistic {
    use crate::terminal::model::block::BlockId;

    #[derive(Debug, Clone)]
    pub(super) struct CLIAgentSubtask {
        pub(super) block_id: BlockId,
    }

    #[derive(Debug, Clone)]
    pub(super) enum Task {
        Root,
        CLIAgent(CLIAgentSubtask),
    }

    impl Task {
        pub(super) fn is_root(&self) -> bool {
            matches!(self, Task::Root)
        }

        pub(super) fn is_cli_subagent(&self) -> bool {
            matches!(self, Task::CLIAgent(..))
        }
    }
}
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
enum TaskImpl {
    Server(ServerTask),
    Optimistic(optimistic::Task),
}

impl TaskImpl {
    fn server_data(&self) -> Option<&ServerTask> {
        match &self {
            TaskImpl::Server(data) => Some(data),
            TaskImpl::Optimistic(_) => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Task {
    id: TaskId,
    data: TaskImpl,
    /// List of `AIAgentExchange`s corresponding to messages contained in this task.
    exchanges: Vec<AIAgentExchange>,
    /// Incremental Markdown state for this task's text messages that are still streaming,
    /// keyed by message id, so each appended chunk re-parses only what it can have changed
    /// (#849). Dropped when the exchange finishes.
    ///
    /// The state assumes the message's text only ever grows at its end. Any code that rewrites
    /// a message's text any other way — replaces it, re-adds the id, splices it — must remove
    /// that message's entry, as `upsert_message`, `update_exchange_from_messages`,
    /// `remove_messages` and `splice_messages` do. The fingerprint `IncrementalTextSections`
    /// checks covers only the length and last 64 bytes before its commit point, so it will not
    /// catch every rewrite.
    streaming_text: HashMap<String, IncrementalTextSections>,
}

impl Task {
    pub(super) fn new_optimistic_root() -> Self {
        Self {
            id: TaskId::new(Uuid::new_v4().to_string()),
            data: TaskImpl::Optimistic(optimistic::Task::Root),
            exchanges: vec![],
            streaming_text: HashMap::new(),
        }
    }

    pub(super) fn new_optimistic_cli_agent_subtask(block_id: BlockId) -> Self {
        Self {
            id: TaskId::new(Uuid::new_v4().to_string()),
            data: TaskImpl::Optimistic(optimistic::Task::CLIAgent(optimistic::CLIAgentSubtask {
                block_id,
            })),
            exchanges: vec![],
            streaming_text: HashMap::new(),
        }
    }

    /// Zap BYOP-only: when the agent's self-initiated LRC receives a snapshot, this
    /// directly creates a Server-backed subagent task in the conversation.
    ///
    /// Cannot reuse `new_optimistic_cli_agent_subtask`: the `TaskImpl::Optimistic` it
    /// produces returns `TaskNotInitialized` outright from entry points that require a
    /// source, such as `add_messages` (`task.rs:649-651`) / `try_get_source`
    /// (`task.rs:627-631`). The upstream path relies on the server returning
    /// `ApplyClientAction::CreateTask` to upgrade optimistic to server; BYOP has no
    /// server, so it needs to construct an `api::Task` locally as the source, with the
    /// task Server-backed from the start.
    ///
    /// `subagent_params` uses a synthetic `Subagent { command_id, ... }`, so that queries
    /// like `is_subagent_task_finished` don't all fall into the `SubagentTaskNotFound`
    /// branch. `tool_call_id` uses a new uuid; there is in fact no corresponding ToolCall
    /// in root.messages, so no ToolCallResult can ever finish this task and
    /// `is_subagent_task_finished` reports `Ok(false)` (not finished) while the LRC is
    /// running. Completion has to be recorded explicitly instead: the `BlockCompleted` hook
    /// in `cli_controller.rs` calls `AIConversation::finish_cli_subagent_task_for_completed_block`,
    /// which for this task delegates to `finish_byop_silent_cli_subtask`; that clears
    /// `optimistic_cli_subagent_subtask_id` and makes `is_subagent_task_finished`
    /// report `Ok(true)`. Nothing else does; without it `has_active_subagent()` stays true
    /// for the rest of the conversation.
    pub(super) fn new_byop_silent_cli_subtask(block_id: BlockId, parent_task_id: String) -> Self {
        let task_id_str = Uuid::new_v4().to_string();
        let subagent_call = api::message::tool_call::Subagent {
            task_id: task_id_str.clone(),
            payload: String::new(),
            metadata: Some(api::message::tool_call::subagent::Metadata::Cli(
                api::message::tool_call::subagent::CliSubagent {
                    command_id: String::from(block_id.clone()),
                },
            )),
        };
        let tool_call_id = Uuid::new_v4().to_string();
        let api_task = api::Task {
            id: task_id_str.clone(),
            description: String::new(),
            dependencies: Some(api::task::Dependencies { parent_task_id }),
            messages: vec![],
            summary: String::new(),
            server_data: String::new(),
        };
        Self {
            id: TaskId(task_id_str),
            data: TaskImpl::Server(ServerTask {
                source: api_task,
                subagent_params: Some(SubagentParams {
                    call: subagent_call,
                    tool_call_id,
                }),
            }),
            exchanges: vec![],
            streaming_text: HashMap::new(),
        }
    }

    #[allow(clippy::unwrap_in_result)]
    pub(super) fn into_server_created_task(
        mut self,
        task: api::Task,
        parent_task: Option<&api::Task>,
        current_todo_list: Option<&AIAgentTodoList>,
        active_code_review: Option<&CodeReview>,
        skill_path_origin: &SkillPathOrigin,
    ) -> Result<Self, UpgradeOptimisticTaskError> {
        match self.data {
            TaskImpl::Optimistic(optimistic::Task::Root) => {
                if parent_task.is_some() {
                    return Err(UpgradeOptimisticTaskError::RootWithUnexpectedParent);
                }
                self.id = TaskId::new(task.id.clone());
                self.data = TaskImpl::Server(ServerTask {
                    source: task,
                    subagent_params: None,
                })
            }
            TaskImpl::Optimistic(optimistic::Task::CLIAgent(_)) => {
                let Some(parent_task) = parent_task else {
                    return Err(UpgradeOptimisticTaskError::CLISubagentMissingParent);
                };

                let Some((subagent_call, subagent_tool_call_id)) =
                    parent_task.messages.iter().find_map(|message| {
                        let tool_call = message.tool_call()?;
                        let subagent_call = tool_call.subagent()?;
                        (subagent_call.task_id == task.id && subagent_call.is_cli())
                            .then(|| (subagent_call.clone(), tool_call.tool_call_id.clone()))
                    })
                else {
                    return Err(UpgradeOptimisticTaskError::CLISubagentMissingSubagentCall);
                };

                self.id = TaskId::new(task.id.clone());
                self.data = TaskImpl::Server(ServerTask {
                    source: task,
                    subagent_params: Some(SubagentParams {
                        call: subagent_call,
                        tool_call_id: subagent_tool_call_id,
                    }),
                })
            }
            TaskImpl::Server(_) => return Err(UpgradeOptimisticTaskError::UnexpectedUpgrade),
        };

        let messages = self.source().expect("exists").messages.clone();
        if let Some(exchange_id) = self.exchanges.last().map(|exchange| exchange.id) {
            if let Err(e) = self.update_exchange_from_messages(
                messages,
                exchange_id,
                current_todo_list,
                active_code_review,
                skill_path_origin,
                false,
            ) {
                log::error!(
                    "Failed to update last exchange from messages upon converting to a server created task: {e:?}"
                );
            }
        }
        Ok(self)
    }

    pub(super) fn new_restored_root(
        task: api::Task,
        restored_exchanges: impl Iterator<Item = AIAgentExchange>,
    ) -> Self {
        let mut restored_exchanges = restored_exchanges.collect_vec();
        restored_exchanges.sort_by_key(|exchange| exchange.start_time);

        Self {
            id: TaskId(task.id.clone()),
            data: TaskImpl::Server(ServerTask {
                source: task,
                subagent_params: None,
            }),
            exchanges: restored_exchanges,
            streaming_text: HashMap::new(),
        }
    }

    pub(super) fn new_subtask(
        subtask: api::Task,
        parent_task: &api::Task,
        existing_exchange: &AIAgentExchange,
        current_todo_list: Option<&AIAgentTodoList>,
        current_comment_state: Option<&CodeReview>,
        skill_path_origin: &SkillPathOrigin,
        should_convert_input_messages: bool,
    ) -> Self {
        let subagent_call_and_id = parent_task.messages.iter().find_map(|message| {
            let tool_call = message.tool_call()?;
            let subagent_call = tool_call.subagent()?;
            (subagent_call.task_id == subtask.id)
                .then(|| (subagent_call.clone(), tool_call.tool_call_id.clone()))
        });

        let mut new_exchange = AIAgentExchange {
            id: AIAgentExchangeId::new(),
            input: vec![],
            output_status: AIAgentOutputStatus::Streaming { output: None },
            added_message_ids: Default::default(),
            start_time: DateTime::now().into(),
            finish_time: None,
            time_to_first_token_ms: None,
            working_directory: existing_exchange.working_directory.clone(),
            model_id: existing_exchange.model_id.clone(),
            coding_model_id: existing_exchange.coding_model_id.clone(),
            cli_agent_model_id: existing_exchange.cli_agent_model_id.clone(),
            computer_use_model_id: existing_exchange.computer_use_model_id.clone(),
            request_cost: None,
            response_initiator: existing_exchange.response_initiator.clone(),
        };
        new_exchange
            .init_output(
                existing_exchange
                    .output_status
                    .output()
                    .expect("exists")
                    .get()
                    .server_output_id
                    .clone()
                    .expect("has output id"),
            )
            .expect("Exchange output is in streaming state.");

        let messages_clone = subtask.messages.clone();
        let new_exchange_id = new_exchange.id;
        let mut me = Self {
            id: TaskId(subtask.id.clone()),
            exchanges: vec![new_exchange],
            streaming_text: HashMap::new(),
            data: TaskImpl::Server(ServerTask {
                source: subtask,
                subagent_params: subagent_call_and_id
                    .map(|(call, tool_call_id)| SubagentParams { call, tool_call_id }),
            }),
        };
        me.update_exchange_from_messages(
            messages_clone,
            new_exchange_id,
            current_todo_list,
            current_comment_state,
            skill_path_origin,
            should_convert_input_messages,
        )
        .expect("Exchange exists and output is in 'streaming' state.");
        me
    }

    pub(super) fn new_restored_subtask(
        subtask: api::Task,
        parent_task: &api::Task,
        restored_exchanges: Vec<AIAgentExchange>,
    ) -> Self {
        let subagent_call_and_id = parent_task.messages.iter().find_map(|message| {
            let tool_call = message.tool_call()?;
            let subagent_call = tool_call.subagent()?;
            (subagent_call.task_id == subtask.id)
                .then(|| (subagent_call.clone(), tool_call.tool_call_id.clone()))
        });

        Self {
            id: TaskId(subtask.id.clone()),
            exchanges: restored_exchanges,
            streaming_text: HashMap::new(),
            data: TaskImpl::Server(ServerTask {
                source: subtask,
                subagent_params: subagent_call_and_id
                    .map(|(call, tool_call_id)| SubagentParams { call, tool_call_id }),
            }),
        }
    }

    /// Creates a new subtask from an api::Task and the parent task for moved messages.
    ///
    /// This is used by `MoveMessagesToNewTask` to create a task for holding moved
    /// messages. The parent_task should already contain the replacement messages
    /// (including the subagent call referencing this subtask) so that we can look
    /// up the subagent_params.
    pub(super) fn new_moved_messages_subtask(subtask: api::Task, parent_task: &api::Task) -> Self {
        let subagent_call_and_id = parent_task.messages.iter().find_map(|message| {
            let tool_call = message.tool_call()?;
            let subagent_call = tool_call.subagent()?;
            (subagent_call.task_id == subtask.id)
                .then(|| (subagent_call.clone(), tool_call.tool_call_id.clone()))
        });

        Self {
            id: TaskId(subtask.id.clone()),
            exchanges: vec![],
            streaming_text: HashMap::new(),
            data: TaskImpl::Server(ServerTask {
                source: subtask,
                subagent_params: subagent_call_and_id
                    .map(|(call, tool_call_id)| SubagentParams { call, tool_call_id }),
            }),
        }
    }

    pub(super) fn subagent_params(&self) -> Option<&SubagentParams> {
        self.data
            .server_data()
            .and_then(|data| data.subagent_params.as_ref())
    }

    pub fn cli_subagent_block_id(&self) -> Option<BlockId> {
        match &self.data {
            TaskImpl::Server(server_data) => server_data
                .subagent_params
                .as_ref()
                .map(|params| &params.call)
                .and_then(|call| match &call.metadata {
                    Some(Metadata::Cli(call)) => Some(call.command_id.clone().into()),
                    Some(Metadata::Research(_)) => None,
                    Some(Metadata::Advice(_)) => None,
                    Some(Metadata::ComputerUse(_)) => None,
                    Some(Metadata::Summarization(_)) => None,
                    Some(Metadata::ConversationSearch(_)) => None,
                    Some(Metadata::WarpDocumentationSearch(_)) => None,
                    None => None,
                }),
            TaskImpl::Optimistic(optimistic::Task::CLIAgent(subtask)) => {
                Some(subtask.block_id.clone())
            }
            TaskImpl::Optimistic(optimistic::Task::Root) => None,
        }
    }

    pub(super) fn append_new_exchange(
        &mut self,
        existing_exchange: &AIAgentExchange,
    ) -> AIAgentExchangeId {
        let mut new_exchange = AIAgentExchange {
            id: AIAgentExchangeId::new(),
            input: vec![],
            output_status: AIAgentOutputStatus::Streaming { output: None },
            added_message_ids: Default::default(),
            start_time: DateTime::now().into(),
            finish_time: None,
            time_to_first_token_ms: None,
            working_directory: existing_exchange.working_directory.clone(),
            model_id: existing_exchange.model_id.clone(),
            coding_model_id: existing_exchange.coding_model_id.clone(),
            cli_agent_model_id: existing_exchange.cli_agent_model_id.clone(),
            computer_use_model_id: existing_exchange.computer_use_model_id.clone(),
            request_cost: None,
            response_initiator: existing_exchange.response_initiator.clone(),
        };
        new_exchange
            .init_output(
                existing_exchange
                    .output_status
                    .output()
                    .expect("exists")
                    .get()
                    .server_output_id
                    .clone()
                    .expect("has output id"),
            )
            .expect("Output is initialized as streaming.");

        let new_exchange_id = new_exchange.id;
        self.exchanges.push(new_exchange);
        new_exchange_id
    }

    pub fn id(&self) -> &TaskId {
        &self.id
    }

    pub fn parent_id(&self) -> Option<TaskId> {
        self.source()
            .and_then(|source| source.dependencies.as_ref())
            .map(|dependencies| TaskId(dependencies.parent_task_id.clone()))
    }

    pub fn is_root_task(&self) -> bool {
        match &self.data {
            TaskImpl::Server(server_data) => server_data
                .source
                .dependencies
                .as_ref()
                .is_none_or(|deps| deps.parent_task_id.is_empty()),
            TaskImpl::Optimistic(task) => task.is_root(),
        }
    }

    pub fn is_cli_subagent(&self) -> bool {
        match &self.data {
            TaskImpl::Server(server_data) => server_data
                .subagent_params
                .as_ref()
                .is_some_and(|params| params.call.is_cli()),
            TaskImpl::Optimistic(task) => task.is_cli_subagent(),
        }
    }

    pub fn is_advice_subagent(&self) -> bool {
        match &self.data {
            TaskImpl::Server(server_data) => server_data
                .subagent_params
                .as_ref()
                .is_some_and(|params| params.call.is_advice()),
            TaskImpl::Optimistic(_) => false,
        }
    }

    pub fn is_computer_use_subagent(&self) -> bool {
        match &self.data {
            TaskImpl::Server(server_data) => server_data
                .subagent_params
                .as_ref()
                .is_some_and(|params| params.call.is_computer_use()),
            TaskImpl::Optimistic(_) => false,
        }
    }

    pub fn is_conversation_search_subagent(&self) -> bool {
        match &self.data {
            TaskImpl::Server(server_data) => server_data
                .subagent_params
                .as_ref()
                .is_some_and(|params| params.call.is_conversation_search()),
            TaskImpl::Optimistic(_) => false,
        }
    }

    pub fn is_warp_documentation_search_subagent(&self) -> bool {
        match &self.data {
            TaskImpl::Server(server_data) => server_data
                .subagent_params
                .as_ref()
                .is_some_and(|params| params.call.is_warp_documentation_search()),
            TaskImpl::Optimistic(_) => false,
        }
    }

    pub fn description(&self) -> &str {
        self.source()
            .map(|source| source.description.as_str())
            .unwrap_or("")
    }

    pub fn exchanges(&self) -> impl Iterator<Item = &AIAgentExchange> {
        self.exchanges.iter()
    }

    pub fn exchange(&self, exchange_id: AIAgentExchangeId) -> Option<&AIAgentExchange> {
        self.exchanges
            .iter()
            .find(|exchange| exchange.id == exchange_id)
    }

    pub(super) fn exchange_mut(
        &mut self,
        exchange_id: AIAgentExchangeId,
    ) -> Option<&mut AIAgentExchange> {
        self.exchanges
            .iter_mut()
            .find(|exchange| exchange.id == exchange_id)
    }

    pub fn last_exchange(&self) -> Option<&AIAgentExchange> {
        self.exchanges.last()
    }

    pub fn exchanges_len(&self) -> usize {
        self.exchanges.len()
    }

    pub fn exchanges_reversed(&self) -> impl Iterator<Item = &AIAgentExchange> {
        self.exchanges.iter().rev()
    }

    pub fn source(&self) -> Option<&api::Task> {
        self.try_get_source().ok()
    }

    pub fn messages(&self) -> impl Iterator<Item = &api::Message> {
        self.source()
            .into_iter()
            .flat_map(|source| source.messages.iter())
    }

    /// Returns all the `AIAgentContext` objects attached messages in this conversation.
    pub fn all_contexts(&self) -> impl Iterator<Item = &AIAgentContext> {
        context_in_exchanges(self.exchanges())
    }

    pub fn initial_working_directory(&self) -> Option<String> {
        self.source()
            .and_then(Self::api_task_initial_working_directory)
    }

    pub fn api_task_initial_working_directory(task: &api::Task) -> Option<String> {
        task.messages
            .iter()
            .find_map(|message| {
                message.message.as_ref().and_then(|content| {
                    let context = match content {
                        Message::UserQuery(user_query) => user_query.context.as_ref(),
                        Message::ToolCallResult(tool_call_result) => {
                            tool_call_result.context.as_ref()
                        }
                        Message::SystemQuery(system_query) => system_query.context.as_ref(),
                        _ => None,
                    };

                    context
                        .and_then(|ctx| ctx.directory.as_ref())
                        .map(|dir| dir.pwd.clone())
                })
            })
            .filter(|pwd| !pwd.is_empty())
    }

    pub(super) fn update_description(&mut self, description: String) {
        let Ok(source) = self.try_get_source_mut() else {
            return;
        };
        source.description = description;
    }

    pub(super) fn update_task_server_data(&mut self, new_server_data: String) {
        let Ok(source) = self.try_get_source_mut() else {
            return;
        };
        source.server_data = new_server_data;
    }

    pub(super) fn append_exchange(&mut self, exchange: AIAgentExchange) {
        self.exchanges.push(exchange);
    }

    fn try_get_source(&self) -> Result<&api::Task, UpdateTaskError> {
        let TaskImpl::Server(ServerTask { source, .. }) = &self.data else {
            return Err(UpdateTaskError::TaskNotInitialized);
        };
        Ok(source)
    }

    fn try_get_source_mut(&mut self) -> Result<&mut api::Task, UpdateTaskError> {
        let TaskImpl::Server(ServerTask { source, .. }) = &mut self.data else {
            return Err(UpdateTaskError::TaskNotInitialized);
        };
        Ok(source)
    }

    pub(super) fn add_messages(
        &mut self,
        messages: Vec<api::Message>,
        exchange_id: AIAgentExchangeId,
        current_todo_list: Option<&AIAgentTodoList>,
        current_comments: Option<&CodeReview>,
        skill_path_origin: &SkillPathOrigin,
        should_convert_input_messages: bool,
    ) -> Result<(), UpdateTaskError> {
        if self.source().is_none() {
            return Err(UpdateTaskError::TaskNotInitialized);
        }
        self.update_exchange_from_messages(
            messages.clone(),
            exchange_id,
            current_todo_list,
            current_comments,
            skill_path_origin,
            should_convert_input_messages,
        )?;
        self.try_get_source_mut()?.messages.extend(messages);
        Ok(())
    }

    pub(super) fn append_source_messages(
        &mut self,
        messages: Vec<api::Message>,
    ) -> Result<(), UpdateTaskError> {
        self.try_get_source_mut()?.messages.extend(messages);
        Ok(())
    }

    pub(super) fn remove_source_messages_by_ids(
        &mut self,
        message_ids: &std::collections::HashSet<String>,
    ) -> Result<(), UpdateTaskError> {
        self.try_get_source_mut()?
            .messages
            .retain(|message| !message_ids.contains(&message.id));
        Ok(())
    }

    pub(super) fn upsert_message(
        &mut self,
        message: api::Message,
        exchange_id: AIAgentExchangeId,
        current_todo_list: Option<&AIAgentTodoList>,
        current_comments: Option<&CodeReview>,
        skill_path_origin: &SkillPathOrigin,
        mask: FieldMask,
        should_convert_input_messages: bool,
    ) -> Result<&api::Message, UpdateTaskError> {
        let Some((idx, existing_message)) = self
            .try_get_source()?
            .messages
            .iter()
            .enumerate()
            .find(|(_, m)| message.id == m.id)
        else {
            self.add_messages(
                vec![message.clone()],
                exchange_id,
                current_todo_list,
                current_comments,
                skill_path_origin,
                should_convert_input_messages,
            )?;
            return self
                .try_get_source()?
                .messages
                .last()
                .ok_or(UpdateTaskError::MessageNotFound);
        };
        let updated_message =
            FieldMaskOperation::update(&api::MESSAGE_DESCRIPTOR, existing_message, &message, mask)
                .apply()
                .map_err(UpdateTaskError::from)?;
        // This replaces the message's sections with a full conversion, which incremental state
        // kept for earlier appends no longer describes.
        self.streaming_text.remove(&updated_message.id);

        let id = self.id.clone();
        let exchange_to_update = self
            .exchange_mut(exchange_id)
            .ok_or(UpdateTaskError::ExchangeNotFound)?;
        exchange_to_update.upsert_output_for_message(
            &id,
            &updated_message,
            current_todo_list,
            current_comments,
            skill_path_origin,
        )?;

        // Task message updates can carry tool call result updates with them,
        // so we need to convert any tool call results and update the exchange accordingly
        // (this is necessary for session sharing, where the tool call input has not already been
        // optimistically inserted into the exchange)
        if should_convert_input_messages {
            if let Some(tool_call_result) = message.tool_call_result() {
                let mut document_versions: HashMap<AIDocumentId, AIDocumentVersion> =
                    HashMap::new();
                if let Some(input) = convert_tool_call_result_to_input(
                    &id,
                    tool_call_result,
                    &HashMap::new(),
                    &mut document_versions,
                ) {
                    if let Some(action_result) = input.action_result() {
                        if let Some(existing_result) =
                            exchange_to_update.input.iter_mut().find(|existing_input| {
                                existing_input
                                    .action_result()
                                    .is_some_and(|existing_result| {
                                        existing_result.id == action_result.id
                                    })
                            })
                        {
                            *existing_result = input;
                        }
                    } else {
                        exchange_to_update.input.push(input)
                    }
                }
            }
        }

        let source = self.try_get_source_mut()?;
        source.messages[idx] = updated_message;
        Ok(&source.messages[idx])
    }

    pub(super) fn append_to_message_content(
        &mut self,
        message: api::Message,
        exchange_id: AIAgentExchangeId,
        current_todo_list: Option<&AIAgentTodoList>,
        current_comments: Option<&CodeReview>,
        skill_path_origin: &SkillPathOrigin,
        mask: FieldMask,
    ) -> Result<&api::Message, UpdateTaskError> {
        let Some((idx, existing_message)) = self
            .try_get_source()?
            .messages
            .iter()
            .enumerate()
            .find(|(_, m)| message.id == m.id)
        else {
            log::error!("Message not found for append client action.");
            return Err(UpdateTaskError::MessageNotFound);
        };
        // A pure text/reasoning append extends the stored text in place. The general field-mask
        // append transcodes the whole message to a dynamic message and back and rebuilds the
        // string with `format!`, which costs O(message) per chunk (#849).
        let in_place = is_in_place_text_append(existing_message, &message, &mask);
        let (mut updated_message, rollback_len) = if in_place {
            if !self
                .exchanges
                .iter()
                .any(|exchange| exchange.id == exchange_id)
            {
                return Err(UpdateTaskError::ExchangeNotFound);
            }
            let mut updated = std::mem::take(&mut self.try_get_source_mut()?.messages[idx]);
            let previous_len = push_appended_text(&mut updated, &message);
            (updated, Some(previous_len))
        } else {
            let updated = FieldMaskOperation::append(
                &api::MESSAGE_DESCRIPTOR,
                existing_message,
                &message,
                mask,
            )
            .apply()
            .map_err(UpdateTaskError::from)?;
            (updated, None)
        };

        let id = self.id.clone();
        let upserted = match self
            .exchanges
            .iter_mut()
            .find(|exchange| exchange.id == exchange_id)
        {
            None => Err(UpdateTaskError::ExchangeNotFound),
            Some(exchange_to_update) => {
                // A streamed answer or reasoning grows by appends, so its sections are brought
                // up to date incrementally rather than re-derived from the whole text on every
                // chunk (#849). The text is moved out while the rest of the message is
                // converted, so that conversion neither copies nor parses it.
                let streaming = matches!(
                    exchange_to_update.output_status,
                    AIAgentOutputStatus::Streaming { output: Some(_) }
                );
                match streaming
                    .then(|| take_streamed_text(&mut updated_message))
                    .flatten()
                {
                    Some(text) => {
                        let parser = self
                            .streaming_text
                            .entry(updated_message.id.clone())
                            .or_default();
                        let upserted = exchange_to_update.upsert_streamed_text_output(
                            &id,
                            &updated_message,
                            &text,
                            parser,
                            current_todo_list,
                            current_comments,
                            skill_path_origin,
                        );
                        restore_streamed_text(&mut updated_message, text);
                        upserted
                    }
                    None => exchange_to_update.upsert_output_for_message(
                        &id,
                        &updated_message,
                        current_todo_list,
                        current_comments,
                        skill_path_origin,
                    ),
                }
            }
        };
        if let Err(err) = upserted {
            // The in-place path took the stored message out; put it back as it was.
            if let Some(previous_len) = rollback_len {
                truncate_streamed_text(&mut updated_message, previous_len);
                self.try_get_source_mut()?.messages[idx] = updated_message;
            }
            return Err(err);
        }

        let source = self.try_get_source_mut()?;
        source.messages[idx] = updated_message;
        Ok(&source.messages[idx])
    }

    /// Replaces the incrementally maintained sections of this task's streamed text messages with
    /// a full conversion of their final text, and drops the incremental state. Called when an
    /// exchange stops streaming, so what stays on screen is exactly what a restored conversation
    /// would show (#849).
    pub(super) fn finalize_streamed_text(&mut self) {
        if self.streaming_text.is_empty() {
            return;
        }
        let streamed = std::mem::take(&mut self.streaming_text);
        let Ok(source) = self.try_get_source() else {
            return;
        };
        for message in &source.messages {
            if !streamed.contains_key(&message.id) {
                continue;
            }
            let text = match &message.message {
                Some(Message::AgentOutput(output)) => output.text.as_str(),
                Some(Message::AgentReasoning(reasoning)) => reasoning.reasoning.as_str(),
                _ => continue,
            };
            for exchange in &self.exchanges {
                let AIAgentOutputStatus::Streaming {
                    output: Some(output),
                } = &exchange.output_status
                else {
                    continue;
                };
                let mut output = output.get_mut();
                if let Some(sections) = output
                    .messages
                    .iter_mut()
                    .find(|m| m.id.0 == message.id)
                    .and_then(|m| text_sections_mut(&mut m.message))
                {
                    *sections = parse_markdown_into_text_and_code_sections(text);
                }
            }
        }
    }

    pub(super) fn remove_exchange(
        &mut self,
        exchange_id: AIAgentExchangeId,
    ) -> Option<AIAgentExchange> {
        if let Some(index) = self
            .exchanges
            .iter()
            .position(|exchange| exchange.id == exchange_id)
        {
            Some(self.exchanges.remove(index))
        } else {
            None
        }
    }

    /// Truncates all exchanges starting from the given exchange ID (inclusive).
    pub(super) fn truncate_exchanges_from(&mut self, from_exchange_id: AIAgentExchangeId) {
        if let Some(index) = self
            .exchanges
            .iter()
            .position(|exchange| exchange.id == from_exchange_id)
        {
            self.exchanges.truncate(index);
        }
    }

    /// Assigns fresh exchange IDs to all exchanges in this task.
    /// Used when forking conversations to avoid ID collisions with persisted blocks.
    pub(super) fn reassign_exchange_ids(&mut self) {
        for exchange in &mut self.exchanges {
            exchange.id = AIAgentExchangeId::new();
        }
    }

    /// Removes messages with the given IDs from the task source.
    pub(super) fn remove_messages(&mut self, message_ids: &HashSet<MessageId>) {
        self.streaming_text
            .retain(|id, _| !message_ids.contains(&MessageId::new(id.clone())));
        match self.try_get_source_mut() {
            Ok(source) => {
                source
                    .messages
                    .retain(|m| !message_ids.contains(&MessageId::new(m.id.clone())));
            }
            Err(e) => {
                log::warn!("Failed to get mutable source for removing messages: {e:?}");
            }
        }
    }

    /// Splices a range of messages into the task, returning the replaced messages.
    ///
    /// This finds the range from `first_message_id` to `last_message_id` (inclusive),
    /// validates that the range contains `expected_message_count` messages, removes
    /// those messages, inserts `replacement_messages` at the same position, and
    /// returns the extracted messages.
    ///
    /// Note: This only modifies the task's proto message list. It does NOT modify
    /// the exchange's client representation, so the UI remains unchanged during a
    /// live session.
    pub(super) fn splice_messages(
        &mut self,
        first_message_id: &str,
        last_message_id: &str,
        expected_message_count: u32,
        replacement_messages: Vec<api::Message>,
    ) -> Result<Vec<api::Message>, ExtractMessagesError> {
        // A replacement may reuse the id of a message that was streaming; incremental state for
        // the old text must not be applied to it (#849).
        self.streaming_text.clear();
        let source = match self.try_get_source_mut() {
            Ok(s) => s,
            Err(_) => return Err(ExtractMessagesError::TaskNotInitialized),
        };

        // Find the index of the first message.
        let first_idx = source
            .messages
            .iter()
            .position(|m| m.id == first_message_id)
            .ok_or_else(|| {
                ExtractMessagesError::FirstMessageNotFound(first_message_id.to_string())
            })?;

        // Find the index of the last message.
        let last_idx = source
            .messages
            .iter()
            .position(|m| m.id == last_message_id)
            .ok_or_else(|| {
                ExtractMessagesError::LastMessageNotFound(last_message_id.to_string())
            })?;

        // Validate that first comes before or equals last.
        if first_idx > last_idx {
            return Err(ExtractMessagesError::InvalidRange);
        }

        // Calculate the actual message count in the range (inclusive).
        let actual_count = (last_idx - first_idx + 1) as u32;
        if actual_count != expected_message_count {
            return Err(ExtractMessagesError::ChecksumMismatch {
                expected: expected_message_count,
                actual: actual_count,
            });
        }

        // Drain the messages from the range.
        let extracted: Vec<api::Message> = source.messages.drain(first_idx..=last_idx).collect();

        // Insert the replacement messages at the same position.
        source
            .messages
            .splice(first_idx..first_idx, replacement_messages);

        Ok(extracted)
    }

    fn update_exchange_from_messages(
        &mut self,
        messages: Vec<api::Message>,
        exchange_id: AIAgentExchangeId,
        current_todo_list: Option<&AIAgentTodoList>,
        active_code_review: Option<&CodeReview>,
        skill_path_origin: &SkillPathOrigin,
        should_convert_input_messages: bool,
    ) -> Result<(), UpdateTaskError> {
        // These messages get a full conversion below. A message re-added under an id that was
        // streaming (a resumed stream replaying it with new text) must not later be extended
        // from incremental state that describes its old text (#849).
        for message in &messages {
            self.streaming_text.remove(&message.id);
        }
        let exchange = self
            .exchange_mut(exchange_id)
            .ok_or(UpdateTaskError::ExchangeNotFound)?;
        exchange
            .added_message_ids
            .extend(messages.iter().map(|m| MessageId::new(m.id.clone())));

        if should_convert_input_messages {
            let user_inputs = user_inputs_from_messages(&messages);

            for input in user_inputs.into_iter() {
                // If the input is an ActionResult with an action ID that already exists,
                // replace the existing one (to handle updates to long-running commands).
                if let Some(action_result) = input.action_result() {
                    if let Some(existing_result) =
                        exchange.input.iter_mut().find(|existing_input| {
                            existing_input
                                .action_result()
                                .is_some_and(|existing_result| {
                                    existing_result.id == action_result.id
                                })
                        })
                    {
                        *existing_result = input;
                        continue;
                    }
                }

                exchange.input.push(input);
            }
        }

        let output = exchange.get_streaming_output()?;
        let output_messages: Result<Vec<AIAgentOutputMessage>, MessageToAIAgentOutputMessageError> =
            messages
                .into_iter()
                .filter_map(|m| {
                    match m.to_client_output_message(ConversionParams {
                        task_id: &self.id,
                        current_todo_list,
                        active_code_review,
                        skill_path_origin,
                    }) {
                        Ok(MaybeAIAgentOutputMessage::Message(m)) => Some(Ok(m)),
                        Ok(MaybeAIAgentOutputMessage::NoClientRepresentation) => None,
                        Err(e) => Some(Err(e)),
                    }
                })
                .collect();
        output.get_mut().messages.extend(output_messages?);
        Ok(())
    }
}

/// Derives todo lists from tasks by replaying UpdateTodos operations in message order.
pub fn derive_todo_lists_from_root_task(root_task: &Task) -> Vec<AIAgentTodoList> {
    let mut todo_lists = Vec::new();

    // Sort messages by their index in the task (messages are already in order within each task)
    // For simplicity, we'll iterate through messages and apply UpdateTodos operations
    for message in root_task.messages() {
        if let Some(api::message::Message::UpdateTodos(update)) = &message.message {
            if let Some(operation) = &update.operation {
                update_todo_list_from_todo_op(&mut todo_lists, operation.clone());
            }
        }
    }

    todo_lists
}

/// Whether `mask` appends `delta`'s text to `stored`'s and nothing else, so the append can be
/// done in place: the result is `stored` with that one string extended, exactly what the general
/// field-mask append produces for it.
fn is_in_place_text_append(stored: &api::Message, delta: &api::Message, mask: &FieldMask) -> bool {
    let [path] = mask.paths.as_slice() else {
        return false;
    };
    matches!(
        (
            path.as_str(),
            stored.message.as_ref(),
            delta.message.as_ref()
        ),
        (
            "agent_output.text",
            Some(Message::AgentOutput(_)),
            Some(Message::AgentOutput(_))
        ) | (
            "agent_reasoning.reasoning",
            Some(Message::AgentReasoning(_)),
            Some(Message::AgentReasoning(_))
        )
    )
}

/// Appends `delta`'s text to `stored`'s for an [`is_in_place_text_append`] append, returning the
/// stored text's previous length.
fn push_appended_text(stored: &mut api::Message, delta: &api::Message) -> usize {
    match (stored.message.as_mut(), delta.message.as_ref()) {
        (Some(Message::AgentOutput(stored)), Some(Message::AgentOutput(delta))) => {
            let previous_len = stored.text.len();
            stored.text.push_str(&delta.text);
            previous_len
        }
        (Some(Message::AgentReasoning(stored)), Some(Message::AgentReasoning(delta))) => {
            let previous_len = stored.reasoning.len();
            stored.reasoning.push_str(&delta.reasoning);
            previous_len
        }
        // Not reached: callers check `is_in_place_text_append` first.
        _ => 0,
    }
}

/// Undoes [`push_appended_text`].
fn truncate_streamed_text(message: &mut api::Message, len: usize) {
    match message.message.as_mut() {
        Some(Message::AgentOutput(output)) => output.text.truncate(len),
        Some(Message::AgentReasoning(reasoning)) => reasoning.reasoning.truncate(len),
        _ => {}
    }
}

/// Moves the streamed text out of an agent output or reasoning message, leaving it empty.
fn take_streamed_text(message: &mut api::Message) -> Option<String> {
    match message.message.as_mut()? {
        Message::AgentOutput(output) => Some(std::mem::take(&mut output.text)),
        Message::AgentReasoning(reasoning) => Some(std::mem::take(&mut reasoning.reasoning)),
        _ => None,
    }
}

/// Puts back text taken by [`take_streamed_text`].
fn restore_streamed_text(message: &mut api::Message, text: String) {
    match message.message.as_mut() {
        Some(Message::AgentOutput(output)) => output.text = text,
        Some(Message::AgentReasoning(reasoning)) => reasoning.reasoning = text,
        _ => {}
    }
}

/// The text sections of an output message converted from an agent output or reasoning message.
fn text_sections_mut(
    message: &mut AIAgentOutputMessageType,
) -> Option<&mut Vec<AIAgentTextSection>> {
    match message {
        AIAgentOutputMessageType::Text(text) | AIAgentOutputMessageType::Reasoning { text, .. } => {
            Some(&mut text.sections)
        }
        _ => None,
    }
}

impl AIAgentExchange {
    /// [`Self::upsert_output_for_message`] for an agent output or reasoning message whose text
    /// is being streamed: `message` has had its text moved out into `text`, and the sections for
    /// `text` are brought up to date by `parser`, reusing the ones already in the output (#849).
    #[allow(clippy::too_many_arguments)]
    fn upsert_streamed_text_output(
        &self,
        task_id: &TaskId,
        message: &api::Message,
        text: &str,
        parser: &mut IncrementalTextSections,
        todo_list: Option<&AIAgentTodoList>,
        comments: Option<&CodeReview>,
        skill_path_origin: &SkillPathOrigin,
    ) -> Result<(), UpdateTaskError> {
        let AIAgentOutputStatus::Streaming {
            output: Some(output),
        } = &self.output_status
        else {
            return Ok(());
        };
        let mut output = output.get_mut();
        let message_idx = output.messages.iter().position(|m| m.id.0 == message.id);

        match message.clone().to_client_output_message(ConversionParams {
            current_todo_list: todo_list,
            active_code_review: comments,
            task_id,
            skill_path_origin,
        })? {
            MaybeAIAgentOutputMessage::Message(mut m) => {
                let mut sections = match message_idx {
                    Some(idx) => text_sections_mut(&mut output.messages[idx].message)
                        .map(std::mem::take)
                        .unwrap_or_default(),
                    None => Vec::new(),
                };
                parser.update(text, &mut sections);
                if let Some(target) = text_sections_mut(&mut m.message) {
                    *target = sections;
                }
                output.extend_citations(m.citations.clone());
                if let Some(message_idx) = message_idx {
                    output.messages[message_idx] = m;
                } else {
                    output.messages.push(m);
                }
            }
            MaybeAIAgentOutputMessage::NoClientRepresentation => {
                log::warn!(
                    "Tried to update output for message which no longer has a client representation"
                );
            }
        }
        Ok(())
    }

    /// Upserts the output for a specific message.
    /// Note: this means updates will insert a new entry after previously added entries.
    fn upsert_output_for_message(
        &self,
        task_id: &TaskId,
        task_message: &api::Message,
        todo_list: Option<&AIAgentTodoList>,
        comments: Option<&CodeReview>,
        skill_path_origin: &SkillPathOrigin,
    ) -> Result<(), UpdateTaskError> {
        if let AIAgentOutputStatus::Streaming {
            output: Some(output),
        } = &self.output_status
        {
            let mut output = output.get_mut();
            let message_idx = output
                .messages
                .iter()
                .position(|m| m.id.0 == task_message.id);

            match task_message
                .clone()
                .to_client_output_message(ConversionParams {
                    current_todo_list: todo_list,
                    active_code_review: comments,
                    task_id,
                    skill_path_origin,
                })? {
                MaybeAIAgentOutputMessage::Message(m) => {
                    // Extract citations from the message and add to the output citations
                    output.extend_citations(m.citations.clone());
                    // Upsert behavior: update the message if it exists, otherwise add it to the end of the list.
                    if let Some(message_idx) = message_idx {
                        output.messages[message_idx] = m;
                    } else {
                        output.messages.push(m);
                    }
                }
                MaybeAIAgentOutputMessage::NoClientRepresentation => {
                    log::warn!(
                        "Tried to update output for message which no longer has a client representation"
                    );
                }
            }
        }

        Ok(())
    }

    /// Retrieves the output if it is currently being streamed.
    fn get_streaming_output(&self) -> Result<Shared<AIAgentOutput>, UpdateTaskError> {
        match &self.output_status {
            AIAgentOutputStatus::Streaming {
                output: Some(output),
            } => Ok(output.get_owned()),
            AIAgentOutputStatus::Streaming { output: None } => {
                Err(UpdateTaskError::OutputNeverInitialized)
            }
            _ => Err(UpdateTaskError::OutputAlreadyFinished),
        }
    }
}

#[cfg(test)]
#[path = "task_tests.rs"]
mod tests;
