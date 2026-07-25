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

// Test-only helpers for the `warp_tui` test suite (BYOP-adapted; cloud singletons dropped).
#[cfg(any(test, feature = "test-util"))]
pub use crate::suggestions::ignored_suggestions_model::IgnoredSuggestionsModel;
#[cfg(any(test, feature = "test-util"))]
pub use crate::tui_test_support::{
    blocklist_ai_history_model_with_queries, queue_tui_permission_action,
    register_tui_session_view_test_singletons,
};

pub use ::ai::agent::action::{AskUserQuestionItem, AskUserQuestionOption, AskUserQuestionType};
pub use ::ai::agent::action_result::AskUserQuestionAnswerItem;
pub use ::ai::agent::ask_user_question_session::{
    AskUserQuestionAction, AskUserQuestionEffect, AskUserQuestionPhase, AskUserQuestionSession,
};
pub use repo_metadata::repositories::RepoDetectionSource;
pub use crate::util::repo_detection::{detect_possible_git_repo, RepoDetectionSessionType};
use warp_completer::completer::{
    suggestions as completer_suggestions, CompleterOptions, CompletionContext as _,
    TopLevelCommandCaseSensitivity,
};
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
    AgentViewController, AgentViewDisplayMode, AgentViewEntryOrigin, AgentViewState,
    EnterAgentViewError, EphemeralMessageModel,
};
pub use crate::ai::blocklist::block::cli_controller::{
    CLISubagentController, CLISubagentEvent, CLISubagentTarget, LongRunningCommandControlState,
    UserTakeOverReason,
};
pub use crate::ai::blocklist::context_model::{
    AttachmentType, BlocklistAIContextEvent, BlocklistAIContextModel, PendingAttachmentSummary,
    PendingQueryState, block_context_from_terminal_model,
};
pub use crate::ai::blocklist::conversation_selection::{
    ConversationSelection, ConversationSelectionEvent, ConversationSelectionHandle,
};
pub use crate::ai::conversation_export::{export_conversation_markdown, ConversationFileExport};
pub use crate::terminal::conversation_restoration::{
    prepare_conversation_block_restoration, ConversationBlockRestorationPlan,
};
pub use crate::terminal::view::blocklist_filter::should_show_task_in_blocklist;
pub use crate::ai::blocklist::diff_storage::{
    changed_lines_from_op, DiffStorage, DiffStorageHelper, FileSnapshot, RegisteredDiffStorage,
    SaveFuture, UpdatedFileState,
};
pub use crate::ai::blocklist::input_mode_policy::{
    InputModePolicy, InputModePolicyHandle, PolicyConfigUpdate,
};
pub use crate::ai::blocklist::block::view_impl::common::format_elapsed_seconds;
pub use crate::ai::blocklist::controller::BlocklistAIController;
pub use crate::ai::blocklist::input_model::{BlocklistAIInputModel, InputConfig, InputType};
pub use crate::ai::blocklist::view_util::format_credits;
pub use crate::ai::blocklist::view_util::{
    failed_output_presentation, should_show_failed_output_usage_notice, FailedOutputPresentation,
    FAILED_OUTPUT_USAGE_NOTICE_TEXT,
};
pub use crate::ai::blocklist::block::model::{
    AIBlockModel, AIBlockModelHelper, AIBlockModelImpl, AIBlockOutputStatus, AIRequestType,
    OutputStatusUpdateCallback,
};
pub use crate::ai::blocklist::history_model::{
    AIQueryHistory, BlocklistAIHistoryEvent, BlocklistAIHistoryModel, LoadedConversationData,
};
pub use crate::ai::blocklist::persistence::maybe_build_ai_query_upsert_event;
pub use crate::ai::llms::{LLMId, LLMInfo, LLMPreferences, LLMPreferencesEvent};
pub use crate::ai::option_snapshot::{
    OptionBadge, OptionFooter, OptionRow, OptionSnapshot, OptionSourceStatus,
};
pub use crate::ai::skills::{SkillManager, SkillReference};
pub use crate::terminal::input::skills::{
    AcceptSkill, SelectableSkill, LOCAL_SKILLS_REMOTE_EXECUTION_ERROR_MESSAGE, query_selectable_skills,
};
pub use crate::code_review::git_status_update::{
    GitRepoModels, GitRepoStatusModel, GitStatusMetadata,
};
pub use crate::appearance::Appearance;
pub use crate::tui::log_out_tui;
pub use crate::tui::{
    TuiMcpAction, TuiMcpConfigState, TuiMcpManager, TuiMcpManagerEvent, TuiMcpServerId,
    TuiMcpServerSnapshot, TuiMcpServerStatus, TuiMcpSnapshot, TuiMcpTransport,
};
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
    TuiZeroStateDataSource, UpdatedActiveCommands, build_slash_command_mixer,
    record_autodetection_toggle_from_slash_command, record_saved_prompt_accepted,
    record_static_slash_command_accepted, saved_prompt_text_for_id,
    should_close_slash_command_menu_for_exact_match, slash_command_query,
    slash_command_selection_behavior,
};
pub use crate::terminal::input::slash_command_model::{
    DetectedCommand, DetectedSkillCommand, ParsedSlashCommandInput,
};
pub use crate::search::slash_command_menu::{SlashCommandId, SlashCommandKind, StaticCommand};
pub use crate::terminal::alt_screen::{should_intercept_mouse, should_intercept_scroll};
pub use crate::terminal::color::{Colors as TerminalColors, List as TerminalColorList};
pub use crate::terminal::history::up_arrow::prompt_history_for_terminal_view;
pub use crate::terminal::event::AfterBlockCompletedEvent;
pub use crate::terminal::input::CommandExecutionSource;
pub use crate::terminal::input::decorations::parse_current_commands_and_tokens;
pub use crate::terminal::input::models::{query_model_picker_choices, ModelPickerChoice};
pub use crate::terminal::local_tty::TerminalManager as LocalTtyTerminalManager;
pub use crate::terminal::local_tty::terminal_manager::TerminalManagerInit;
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
    PtyIntent, PtyIntentEvent, TerminalSurface, TerminalSurfaceInit, TerminalSurfaceResult,
};
pub use crate::terminal::{
    BlockPadding, BlockSpacing, ShellLaunchData, SizeInfo, SizeUpdate,
    TerminalManager as TerminalManagerTrait, TerminalModel,
};
pub use crate::themes::default_themes::{dark_theme, light_theme};
pub use crate::throttle::throttle;
pub use crate::util::image::{
    MAX_IMAGE_COUNT_FOR_QUERY, MAX_IMAGE_SIZE_BYTES, MIME_SNIFF_BYTES, ProcessImageResult,
    infer_mime_type, is_supported_image_mime_type, process_image_for_agent,
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

/// One shell command/path completion candidate for the TUI Tab-completion popup.
pub struct TuiCompletionCandidate {
    pub display: String,
    pub replacement: String,
    pub description: Option<String>,
}

/// The result of a TUI completion fetch: candidates plus the byte span in the
/// (pre-cursor) input line that an accepted candidate replaces.
pub struct TuiCompletionResults {
    pub candidates: Vec<TuiCompletionCandidate>,
    pub replacement_span: std::ops::Range<usize>,
}

/// Fetches shell command/path completion candidates from the shared completer
/// engine for `line` at byte offset `pos`. Async and potentially I/O-bound (it
/// may hit the filesystem / shell), so callers must run it off the UI thread.
///
/// Mirrors the GUI's completion fetch: the engine already matches candidates
/// against the token under the cursor, so the returned list is ready to display.
pub async fn tui_fetch_completions(
    line: String,
    pos: usize,
    completion_context: SessionContext,
) -> Option<TuiCompletionResults> {
    let results = completer_suggestions(
        &line,
        pos,
        None,
        CompleterOptions::default(),
        &completion_context,
    )
    .await?;
    if results.suggestions.is_empty() {
        return None;
    }
    let replacement_span: std::ops::Range<usize> = results.replacement_span.into();
    let candidates = results
        .suggestions
        .into_iter()
        .filter(|matched| !matched.suggestion.is_hidden)
        .map(|matched| TuiCompletionCandidate {
            display: matched.display().to_string(),
            replacement: matched.replacement().to_string(),
            description: matched.description(),
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return None;
    }
    Some(TuiCompletionResults {
        candidates,
        replacement_span,
    })
}

pub use crate::ai::execution_profiles::profiles::ClientProfileId;

/// One agent execution profile for the TUI `/profile` picker.
pub struct TuiProfileEntry {
    pub id: ClientProfileId,
    pub display_name: String,
    pub is_active: bool,
}

/// Lists the agent execution profiles, marking the one active for
/// `terminal_view_id`. The default profile is always first (per
/// `get_all_profile_ids`).
pub fn tui_list_profiles(
    app: &warpui::AppContext,
    terminal_view_id: warpui::EntityId,
) -> Vec<TuiProfileEntry> {
    use crate::cloud_object::model::generic_string_model::StringModel;
    use warpui::SingletonEntity as _;
    let profiles = crate::ai::execution_profiles::profiles::AIExecutionProfilesModel::as_ref(app);
    let active_id = *profiles.active_profile(Some(terminal_view_id), app).id();
    profiles
        .get_all_profile_ids()
        .into_iter()
        .filter_map(|id| {
            let info = profiles.get_profile_by_id(id, app)?;
            Some(TuiProfileEntry {
                id,
                display_name: info.data().display_name(),
                is_active: id == active_id,
            })
        })
        .collect()
}

/// Switches the active agent execution profile for `terminal_view_id`, mirroring
/// the GUI's inline profile selector: set the active profile AND drop the
/// pane-level LLM override so the newly-active profile's model can take effect.
pub fn tui_set_active_profile(
    ctx: &mut warpui::AppContext,
    terminal_view_id: warpui::EntityId,
    profile_id: ClientProfileId,
) {
    use warpui::SingletonEntity as _;
    crate::ai::execution_profiles::profiles::AIExecutionProfilesModel::handle(ctx).update(
        ctx,
        |profiles, ctx| {
            profiles.set_active_profile(terminal_view_id, profile_id, ctx);
        },
    );
    crate::ai::llms::LLMPreferences::handle(ctx).update(ctx, |prefs, ctx| {
        prefs.remove_llm_override(terminal_view_id, ctx);
    });
}

/// Returns whether cloud conversation metadata failed to load.
///
/// BYOP has no cloud conversation metadata, so this is always `false` — the conversation
/// menu never surfaces a cloud-metadata warning.
pub fn agent_conversations_cloud_metadata_load_failed(_app: &warpui::AppContext) -> bool {
    false
}
