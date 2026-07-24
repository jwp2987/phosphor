//! Public app APIs used by the `warp_tui` frontend (Zap-adapted).
//!
//! This is the seam between the `warp_tui` crate and the `warp` app crate. It is
//! the Zap-adapted subset of upstream warp's `tui_export`: the cloud/orchestration
//! re-exports are dropped (Zap has no cloud agent), and the app-crate features Zap
//! has not yet ported (conversation selection/restoration, diff storage, git-repo
//! model, the newer slash-command/skills/model-picker TUI types) are omitted for
//! now. Per the "match Warp minus cloud" north star (see docs/DESIGN-ZAP-FORK.md),
//! those non-cloud gaps are to be built/ported and re-added here — not left out
//! permanently. See specs/warp-oss-sync/SCOPE.md.

pub use ::ai::agent::action::{AskUserQuestionItem, AskUserQuestionOption, AskUserQuestionType};
pub use ::ai::agent::action_result::AskUserQuestionAnswerItem;
pub use ::ai::agent::ask_user_question_session::{
    AskUserQuestionAction, AskUserQuestionEffect, AskUserQuestionPhase, AskUserQuestionSession,
};
pub use repo_metadata::repositories::RepoDetectionSource;
use warp_completer::completer::{CompletionContext as _, TopLevelCommandCaseSensitivity};
use warp_completer::signatures::CommandRegistry;

pub use crate::ai::agent::api::ServerConversationToken;
pub use crate::ai::agent::conversation::{
    AIConversation, AIConversationAutoexecuteMode, AIConversationId, ConversationStatus, TodoStatus,
};
pub use crate::ai::agent::task::TaskId;
pub use crate::ai::agent::todos::AIAgentTodoList;
pub use crate::ai::agent::{
    AIAgentAction, AIAgentActionId, AIAgentActionResult, AIAgentActionResultType,
    AIAgentActionType, AIAgentContext, AIAgentExchangeId, AIAgentInput, AIAgentOutput,
    AIAgentOutputMessage, AIAgentOutputMessageType, AIAgentPtyWriteMode, AIAgentText,
    AIAgentTextSection, AIAgentTodo, AIAgentTodoId, AgentOutputImage, AgentOutputImageLayout,
    AgentOutputMermaidDiagram, AgentOutputTable, AskUserQuestionResult, CancellationReason,
    FileGlobV2Result, GrepResult, ImageContext, MessageId, ReceivedMessageDisplay,
    RenderableAIError, RequestCommandOutputResult, ServerOutputId, Shared, ShellCommandDelay,
    SuggestNewConversationResult, SummarizationType, TodoOperation, UserQueryMode,
};
pub use crate::ai::agent_conversations_model::{
    AgentConversationsModel, AgentConversationsModelEvent, AgentManagementFilters,
    AgentRunDisplayStatus, HarnessFilter,
};
pub use crate::ai::conversation_entry::{
    AgentConversationDisplayData, AgentConversationEntry, AgentConversationEntryId,
    AgentConversationIdentity, AgentConversationListEntryState, AgentConversationListPolicy,
    AgentConversationQueryResult, query_conversation_entries,
};
pub use warp_cli::agent::Harness;
pub use crate::ai::blocklist::action_model::{
    AIActionStatus, BlocklistAIActionEvent, BlocklistAIActionModel, NewConversationDecision,
    ShellCommandExecutor, ShellCommandExecutorEvent,
};
pub use crate::ai::blocklist::agent_view::{
    AgentViewController, AgentViewDisplayMode, AgentViewEntryOrigin, EnterAgentViewError,
    EphemeralMessageModel,
};
pub use crate::ai::blocklist::block::cli_controller::{
    CLISubagentController, CLISubagentEvent, CLISubagentTarget, LongRunningCommandControlState,
    UserTakeOverReason,
};
pub use crate::ai::blocklist::context_model::{
    AttachmentType, BlocklistAIContextEvent, BlocklistAIContextModel, PendingQueryState,
    block_context_from_terminal_model,
};
pub use crate::ai::blocklist::conversation_selection::{
    ConversationSelection, ConversationSelectionEvent, ConversationSelectionHandle,
};
pub use crate::ai::blocklist::block::view_impl::common::format_elapsed_seconds;
pub use crate::ai::blocklist::controller::BlocklistAIController;
pub use crate::ai::blocklist::input_model::{BlocklistAIInputModel, InputConfig, InputType};
pub use crate::ai::blocklist::view_util::format_credits;
pub use crate::ai::blocklist::block::model::{
    AIBlockModel, AIBlockModelHelper, AIBlockModelImpl, AIBlockOutputStatus, AIRequestType,
    OutputStatusUpdateCallback,
};
pub use crate::ai::blocklist::history_model::{
    AIQueryHistory, BlocklistAIHistoryEvent, BlocklistAIHistoryModel,
};
pub use crate::ai::llms::{LLMId, LLMInfo, LLMPreferences, LLMPreferencesEvent};
pub use crate::ai::option_snapshot::{
    OptionBadge, OptionFooter, OptionRow, OptionSnapshot, OptionSourceStatus,
};
pub use crate::ai::skills::{SkillManager, SkillReference};
pub use crate::appearance::Appearance;
pub use crate::tui::log_out_tui;
pub use crate::banner::BannerState;
pub use crate::changelog_model::{
    ChangelogModel, ChangelogRequestType, ChangelogState, Event as ChangelogModelEvent,
};
pub use crate::code::DiffResult;
pub use crate::completer::SessionContext;
pub use crate::persistence::PersistenceWriter;
pub use crate::ai::blocklist::inline_action::code_diff_view::{DiffSessionType, FileDiff};
pub use crate::search::slash_command_menu::static_commands::commands::{
    self as slash_commands, COMMAND_REGISTRY,
};
pub use crate::terminal::input::slash_commands::{
    AcceptSlashCommandOrSavedPrompt, SlashCommandDataSource, SlashCommandMixer,
    SlashCommandSelectionBehavior, TuiSlashCommandDataSource, TuiSlashCommandDataSourceArgs,
    UpdatedActiveCommands, build_slash_command_mixer, should_close_slash_command_menu_for_exact_match,
    slash_command_query, slash_command_selection_behavior,
};
pub use crate::terminal::input::slash_command_model::ParsedSlashCommandInput;
pub use crate::search::slash_command_menu::{SlashCommandId, StaticCommand};
pub use crate::terminal::alt_screen::{should_intercept_mouse, should_intercept_scroll};
pub use crate::terminal::color::{Colors as TerminalColors, List as TerminalColorList};
pub use crate::terminal::history::up_arrow::prompt_history_for_terminal_view;
pub use crate::terminal::event::AfterBlockCompletedEvent;
pub use crate::terminal::input::CommandExecutionSource;
pub use crate::terminal::input::decorations::parse_current_commands_and_tokens;
pub use crate::terminal::local_tty::TerminalManager as LocalTtyTerminalManager;
pub use crate::terminal::model::block::{AgentInteractionMetadata, Block, BlockId};
pub use crate::terminal::model::blockgrid::BlockGrid;
pub use crate::terminal::model::blocks::{
    BlockHeight, BlockHeightItem, BlockHeightSummary, BlockList, RichContentItem, TotalIndex,
};
pub use crate::terminal::model::escape_sequences::{KeystrokeWithDetails, ToEscapeSequence};
pub use crate::terminal::model::grid::grid_handler::{GridHandler, TermMode};
pub use crate::terminal::model::rich_content::RichContentType;
pub use crate::terminal::model::session::Sessions;
pub use crate::terminal::model::session::active_session::{ActiveSession, ActiveSessionEvent};
pub use crate::terminal::model::terminal_model::BlockIndex;
pub use crate::terminal::model_events::{ModelEvent, ModelEventDispatcher};
pub use crate::terminal::shared_session::IsSharedSessionCreator;
pub use crate::terminal::view::{ExecuteCommandEvent, WAKEUP_THROTTLE_PERIOD};
pub use crate::terminal::{
    BlockPadding, ShellLaunchData, SizeInfo, SizeUpdate,
    TerminalManager as TerminalManagerTrait, TerminalModel,
};
pub use crate::themes::default_themes::{dark_theme, light_theme};
pub use crate::throttle::throttle;
pub use crate::util::image::{
    MAX_IMAGE_COUNT_FOR_QUERY, MAX_IMAGE_SIZE_BYTES, ProcessImageResult,
    is_supported_image_mime_type, process_image_for_agent,
};

/// Builds the live-shell completion context used to parse TUI input for NLD.
pub fn tui_completion_session_context(
    active_session: &ActiveSession,
    current_working_directory: String,
    app: &warpui::AppContext,
) -> Option<SessionContext> {
    let session = active_session.session(app)?;
    let current_working_directory =
        session.convert_directory_to_typed_path_buf(current_working_directory);
    Some(SessionContext::new(
        session,
        CommandRegistry::global_instance(),
        current_working_directory,
        app,
    ))
}

/// Returns whether `command` exactly matches a top-level command available in
/// the TUI's live shell completion context.
pub fn tui_completion_context_has_exact_command(
    completion_context: &SessionContext,
    command: &str,
) -> bool {
    let case_sensitivity = completion_context.command_case_sensitivity();
    let is_live_shell_command =
        completion_context
            .top_level_commands()
            .any(|candidate| match case_sensitivity {
                TopLevelCommandCaseSensitivity::CaseSensitive => candidate == command,
                TopLevelCommandCaseSensitivity::CaseInsensitive => {
                    candidate.eq_ignore_ascii_case(command)
                }
            });
    if is_live_shell_command {
        return true;
    }

    #[cfg(feature = "completions_v2")]
    {
        completion_context
            .command_registry()
            .get_signature(command)
            .is_some()
    }
    #[cfg(not(feature = "completions_v2"))]
    {
        completion_context
            .command_registry()
            .signature_from_line(command, case_sensitivity)
            .is_some()
    }
}

/// Returns whether cloud conversation metadata failed to load.
///
/// BYOP has no cloud conversation metadata, so this is always `false` — the conversation
/// menu never surfaces a cloud-metadata warning.
pub fn agent_conversations_cloud_metadata_load_failed(_app: &warpui::AppContext) -> bool {
    false
}
