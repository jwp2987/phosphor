//! Public app APIs used by the `warp_tui` frontend (Zap-adapted).
//!
//! This is the seam between the `warp_tui` crate and the `warp` app crate. It is
//! the Zap-adapted subset of upstream warp's `tui_export`: the cloud/orchestration
//! re-exports are dropped (Zap has no cloud agent), and the app-crate features Zap
//! has not yet ported (conversation selection/restoration, diff storage, git-repo
//! model, the newer slash-command/skills/model-picker TUI types) are omitted for
//! now. Per the "match Warp minus cloud" north star (see docs/DESIGN-PHOSPHOR-FORK.md),
//! those non-cloud gaps are to be built/ported and re-added here — not left out
//! permanently. See specs/warp-oss-sync/SCOPE.md.

mod history;

// Test-only helpers for the `warp_tui` test suite (BYOP-adapted; cloud singletons dropped).
#[cfg(any(test, feature = "test-util"))]
pub use crate::suggestions::ignored_suggestions_model::IgnoredSuggestionsModel;
#[cfg(any(test, feature = "test-util"))]
pub use crate::tui_test_support::{
    add_tui_history_test_models, append_tui_history_test_command,
    blocklist_ai_history_model_with_queries, queue_tui_permission_action,
    register_tui_action_execution_test_singletons, register_tui_input_mode_test_settings,
    register_tui_session_view_test_singletons,
};

pub use self::history::{TuiUpArrowHistoryItem, TuiUpArrowHistoryItemKind, tui_up_arrow_history};
pub use crate::util::repo_detection::{RepoDetectionSessionType, detect_possible_git_repo};
pub use ::ai::agent::action::{AskUserQuestionItem, AskUserQuestionOption, AskUserQuestionType};
pub use ::ai::agent::action_result::AskUserQuestionAnswerItem;
pub use ::ai::agent::ask_user_question_session::{
    AskUserQuestionAction, AskUserQuestionEffect, AskUserQuestionPhase, AskUserQuestionSession,
    QuestionDraft,
};
pub use repo_metadata::repositories::RepoDetectionSource;
use warp_completer::completer::{
    CompleterOptions, CompletionContext as _, EngineFileType, TopLevelCommandCaseSensitivity,
    suggestions as completer_suggestions,
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
    RejectedToolCallKind, RenderableAIError, RequestCommandOutputResult, ServerOutputId, Shared,
    ShellCommandDelay, StartAgentExecutionMode, SuggestNewConversationResult, SummarizationType,
    TodoOperation, UserQueryMode, rejected_tool_call_text,
};
pub use crate::ai::agent_conversations_model::{
    AgentConversationsModel, AgentConversationsModelEvent, AgentManagementFilters,
    AgentRunDisplayStatus, HarnessFilter,
};
pub use crate::ai::blocklist::action_model::{
    AIActionStatus, BlocklistAIActionEvent, BlocklistAIActionModel, NewConversationDecision,
    ShellCommandExecutor, ShellCommandExecutorEvent, StartAgentExecutor, StartAgentExecutorEvent,
    StartAgentOutcome, StartAgentRequest, StartAgentRequestId,
};
pub use crate::ai::blocklist::agent_view::{
    AgentViewController, AgentViewDisplayMode, AgentViewEntryOrigin, AgentViewState,
    EnterAgentViewError, EphemeralMessageModel,
};
pub use crate::ai::blocklist::block::cli_controller::{
    CLISubagentController, CLISubagentEvent, CLISubagentTarget, LongRunningCommandControlState,
    UserTakeOverReason,
};
pub use crate::ai::blocklist::block::model::{
    AIBlockModel, AIBlockModelHelper, AIBlockModelImpl, AIBlockOutputStatus, AIRequestType,
    OutputStatusUpdateCallback,
};
pub use crate::ai::blocklist::block::view_impl::common::format_elapsed_seconds;
pub use crate::ai::blocklist::context_model::{
    AttachmentType, BlocklistAIContextEvent, BlocklistAIContextModel, PendingAttachmentSummary,
    PendingQueryState, block_context_from_terminal_model,
};
pub use crate::ai::blocklist::controller::BlocklistAIController;
pub use crate::ai::blocklist::conversation_selection::{
    ConversationSelection, ConversationSelectionEvent, ConversationSelectionHandle,
};
pub use crate::ai::blocklist::diff_storage::{
    DiffStorage, DiffStorageHelper, FileSnapshot, RegisteredDiffStorage, SaveFuture,
    UpdatedFileState, changed_lines_from_op,
};
pub use crate::ai::blocklist::history_model::{
    AIQueryHistory, BlocklistAIHistoryEvent, BlocklistAIHistoryModel, ConversationStatusUpdate,
    FORK_PREFIX, LoadedConversationData, PRE_REWIND_PREFIX,
};
pub use crate::ai::blocklist::inline_action::code_diff_view::{
    DiffSessionType, FileDiff, convert_file_edits_to_file_diffs,
};
pub use crate::ai::blocklist::input_mode_policy::{
    InputModePolicy, InputModePolicyHandle, PolicyConfigUpdate,
};
pub use crate::ai::blocklist::input_model::{BlocklistAIInputModel, InputConfig, InputType};
// Local-only orchestration topology helpers (no remote-worker execution path in
// this fork -- see `orchestration_topology`'s module doc). Feeds the TUI's
// orchestration tab bar snapshot; kept separate from the (unported)
// cloud-runner "RunAgents" orchestration family.
pub use crate::ai::blocklist::orchestration_topology::{
    OrchestrationParticipantKind, OrderedOrchestrationDescendant,
    descendant_conversations_in_pill_order, orchestration_root_conversation_id,
    orchestrator_agent_id_for_conversation, resolve_orchestration_participant,
};
pub use crate::ai::blocklist::permissions::BlocklistAIPermissions;
pub use crate::ai::blocklist::persistence::maybe_build_ai_query_upsert_event;
pub use crate::ai::blocklist::view_util::format_credits;
pub use crate::ai::blocklist::view_util::{
    FAILED_OUTPUT_USAGE_NOTICE_TEXT, FailedOutputPresentation, failed_output_presentation,
    should_show_failed_output_usage_notice,
};
pub use crate::ai::conversation_entry::{
    AgentConversationDisplayData, AgentConversationEntry, AgentConversationEntryId,
    AgentConversationIdentity, AgentConversationListEntryState, AgentConversationListPolicy,
    AgentConversationQueryResult, query_conversation_entries,
};
pub use crate::ai::conversation_export::{ConversationFileExport, export_conversation_markdown};
pub use crate::ai::llms::{LLMId, LLMInfo, LLMPreferences, LLMPreferencesEvent};
pub use crate::ai::option_snapshot::{
    OptionBadge, OptionFooter, OptionRow, OptionSnapshot, OptionSourceStatus,
};
// Lets `--set-provider-api-key` / `--clear-provider-api-key` tell already-running
// Zap processes to re-read the shared keyring after it persists a key.
#[cfg(not(target_family = "wasm"))]
pub use crate::ai::tui_api_keys::notify_tui_api_keys_changed;
pub use crate::ai::skills::{SkillManager, SkillReference};
pub use crate::ai::usage_cost::{UsageCostOutcome, context_usage_report, conversation_cost_report};
pub use crate::appearance::Appearance;
pub use crate::banner::BannerState;
pub use crate::changelog_model::{
    ChangelogModel, ChangelogRequestType, ChangelogState, Event as ChangelogModelEvent,
};
pub use crate::code::DiffResult;
pub use crate::code_review::git_status_update::{
    GitRepoModels, GitRepoStatusModel, GitStatusMetadata,
};
pub use crate::completer::SessionContext;
pub use crate::persistence::PersistenceWriter;
pub use crate::prefix::longest_common_prefix;
pub use crate::search::slash_command_menu::static_commands::commands::{
    self as slash_commands, COMMAND_REGISTRY,
};
pub use crate::search::slash_command_menu::{SlashCommandId, SlashCommandKind, StaticCommand};
pub use crate::terminal::alt_screen::{should_intercept_mouse, should_intercept_scroll};
pub use crate::terminal::color::{Colors as TerminalColors, List as TerminalColorList};
pub use crate::terminal::conversation_restoration::{
    ConversationBlockRestorationPlan, prepare_conversation_block_restoration,
};
pub use crate::terminal::event::AfterBlockCompletedEvent;
pub use crate::terminal::history::up_arrow::{
    UpArrowHistoryConfig, prompt_history_for_terminal_view,
};
pub use crate::terminal::history::{History, HistoryEvent, LinkedWorkflowData};
pub use crate::terminal::input::CommandExecutionSource;
pub use crate::terminal::input::decorations::parse_current_commands_and_tokens;
pub use crate::terminal::input::models::{ModelPickerChoice, query_model_picker_choices};
pub use crate::terminal::input::skills::{
    AcceptSkill, LOCAL_SKILLS_REMOTE_EXECUTION_ERROR_MESSAGE, SelectableSkill,
    query_selectable_skills,
};
pub use crate::terminal::input::slash_command_model::{
    DetectedCommand, DetectedSkillCommand, ParsedSlashCommandInput,
};
// TUI child-agent launch seam (`#325`/`/orchestrate`'s local-harness machinery,
// reached through `pane_group::pane`'s narrow wrappers -- see the module doc
// comment above `TuiPreparedChildAgentLaunch` in `pane_group/pane/mod.rs` for
// why this indirection exists instead of a direct re-export of
// `local_harness_launch`, which stays `pub(super)`-private to that module).
#[cfg(not(target_family = "wasm"))]
pub use crate::ai::ambient_agents::AmbientAgentTaskId;
#[cfg(not(target_family = "wasm"))]
pub use crate::pane_group::pane::{
    TuiPreparedChildAgentLaunch, prepare_tui_child_agent_launch, tui_compose_child_agent_prompt,
    tui_split_orchestrate_tasks,
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
pub use crate::terminal::view::blocklist_filter::should_show_task_in_blocklist;
pub use crate::terminal::view::{ExecuteCommandEvent, WAKEUP_THROTTLE_PERIOD};
pub use crate::terminal::{
    BlockPadding, BlockSpacing, ShellLaunchData, SizeInfo, SizeUpdate,
    TerminalManager as TerminalManagerTrait, TerminalModel,
};
pub use crate::terminal::{
    PtyIntent, PtyIntentEvent, TerminalSurface, TerminalSurfaceInit, TerminalSurfaceResult,
};
pub use crate::themes::default_themes::{dark_theme, light_theme};
pub use crate::throttle::throttle;
pub use crate::tui::log_out_tui;
pub use crate::tui::{
    TuiMcpAction, TuiMcpConfigState, TuiMcpManager, TuiMcpManagerEvent, TuiMcpServerId,
    TuiMcpServerSnapshot, TuiMcpServerStatus, TuiMcpSnapshot, TuiMcpTransport,
};
pub use crate::util::image::{
    MAX_IMAGE_COUNT_FOR_QUERY, MAX_IMAGE_SIZE_BYTES, MIME_SNIFF_BYTES, ProcessImageResult,
    infer_mime_type, is_supported_image_mime_type, process_image_for_agent,
};
pub use warp_cli::agent::Harness;

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
    /// Whether this candidate names a directory. Directory completions must
    /// not get a trailing space appended on accept (the user is expected to
    /// keep typing into the directory), unlike file/command completions.
    pub is_directory: bool,
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
            is_directory: matched.suggestion.file_type == Some(EngineFileType::Directory),
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

/// One saved prompt for the TUI `/prompts` picker.
pub struct TuiPromptEntry {
    /// The prompt's display name.
    pub name: String,
    /// The prompt's query text, inserted into the input on acceptance.
    pub content: String,
}

/// Lists the user's saved prompts (Zap Drive "Agent Mode" workflows), newest
/// list order preserved. Command workflows are excluded — they belong to the
/// command palette, not the prompt library — mirroring the GUI
/// `PromptsMenuDataSource`.
///
/// The GUI opens a workflow info box on selection so arguments can be filled in;
/// the TUI port inserts the raw query text instead, leaving any `{{argument}}`
/// placeholders in place for the user to edit. See
/// `TuiTerminalSessionView::handle_accepted_prompt`.
pub fn tui_list_prompts(app: &warpui::AppContext) -> Vec<TuiPromptEntry> {
    use crate::cloud_object::model::persistence::ObjectStoreModel;
    use warpui::SingletonEntity as _;
    ObjectStoreModel::as_ref(app)
        .get_all_active_workflows()
        .filter(|workflow| !workflow.model().data.is_command_workflow())
        .map(|workflow| {
            let data = &workflow.model().data;
            TuiPromptEntry {
                name: data.name().to_owned(),
                content: data.content().to_owned(),
            }
        })
        .collect()
}

/// One user-query exchange for the TUI `/fork-from` and `/rewind` exchange
/// pickers.
pub struct TuiConversationExchange {
    pub id: crate::ai::agent::AIAgentExchangeId,
    /// The user's query text for this exchange, used as the picker row label.
    pub query_text: String,
}

/// Lists the user-query exchanges of `conversation_id` in chronological order
/// (oldest first), mirroring the GUI `UserQueryDataSource`. Only root-task
/// exchanges that carry a user query are included. Empty if the conversation is
/// not in memory.
pub fn tui_list_conversation_exchanges(
    app: &warpui::AppContext,
    conversation_id: crate::ai::agent::conversation::AIConversationId,
) -> Vec<TuiConversationExchange> {
    use crate::ai::blocklist::history_model::BlocklistAIHistoryModel;
    use warpui::SingletonEntity as _;
    let Some(conversation) = BlocklistAIHistoryModel::as_ref(app).conversation(&conversation_id)
    else {
        return Vec::new();
    };
    conversation
        .root_task_exchanges()
        .filter(|exchange| exchange.has_user_query())
        .map(|exchange| TuiConversationExchange {
            id: exchange.id,
            query_text: exchange.format_input_for_copy(),
        })
        .collect()
}

/// One agent action paired with the exchange it belongs to.
pub struct TuiActionExchange {
    pub action_id: crate::ai::agent::AIAgentActionId,
    pub exchange_id: crate::ai::agent::AIAgentExchangeId,
}

/// Lists every action in `conversation_id` paired with its exchange, in
/// chronological order (oldest first, grouped by task). Used by `/rewind` to
/// find — and revert, newest-first — the file-edit actions in the exchanges it
/// truncates. Empty if the conversation is not in memory.
pub fn tui_conversation_actions_in_order(
    app: &warpui::AppContext,
    conversation_id: crate::ai::agent::conversation::AIConversationId,
) -> Vec<TuiActionExchange> {
    use crate::ai::blocklist::history_model::BlocklistAIHistoryModel;
    use warpui::SingletonEntity as _;
    let Some(conversation) = BlocklistAIHistoryModel::as_ref(app).conversation(&conversation_id)
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (_task_id, exchanges) in conversation.all_exchanges_by_task() {
        for exchange in exchanges {
            let exchange_id = exchange.id;
            let Some(output) = exchange.output_status.output() else {
                continue;
            };
            for action in output.get().actions() {
                out.push(TuiActionExchange {
                    action_id: action.id.clone(),
                    exchange_id,
                });
            }
        }
    }
    out
}

/// Returns whether cloud conversation metadata failed to load.
///
/// BYOP has no cloud conversation metadata, so this is always `false` — the conversation
/// menu never surfaces a cloud-metadata warning.
pub fn agent_conversations_cloud_metadata_load_failed(_app: &warpui::AppContext) -> bool {
    false
}

/// One user-configured BYOP agent provider for the TUI `/api-keys` picker.
///
/// Deliberately a flat DTO (not the raw `AgentProvider`) so `warp_tui` only sees what its
/// picker needs to render and act on, mirroring `TuiProfileEntry` / `TuiPromptEntry`.
#[derive(Clone)]
pub struct TuiApiKeyProvider {
    pub provider_id: String,
    /// Falls back to `provider_id` when the user hasn't named the provider yet.
    pub display_name: String,
    pub api_type_label: &'static str,
    /// Whether a non-empty API key is currently stored for this provider.
    pub has_key: bool,
}

/// Lists every user-configured custom agent provider — arbitrary BYOP endpoints, not a fixed
/// catalog — paired with whether it currently has an API key stored. Unlike
/// `agent_providers::build_byop_llm_infos`, every configured provider is included regardless of
/// `AgentProvider::is_usable()`: a provider without models yet (or temporarily disabled) can
/// still have a key added to it here, so key management isn't gated behind first configuring
/// models.
pub fn tui_list_agent_provider_keys(app: &warpui::AppContext) -> Vec<TuiApiKeyProvider> {
    use crate::ai::agent_providers::AgentProviderSecrets;
    use crate::settings::AISettings;
    use settings::Setting as _;
    use warpui::SingletonEntity as _;

    let providers = AISettings::as_ref(app).agent_providers.value().clone();
    let secrets = AgentProviderSecrets::as_ref(app);
    providers
        .into_iter()
        .map(|provider| {
            let display_name = if provider.name.trim().is_empty() {
                provider.id.clone()
            } else {
                provider.name.clone()
            };
            let has_key = secrets.get(&provider.id).is_some_and(|key| !key.is_empty());
            TuiApiKeyProvider {
                provider_id: provider.id,
                display_name,
                api_type_label: provider.api_type.display_name(),
                has_key,
            }
        })
        .collect()
}

/// Sets (or, if `api_key` is empty, clears) the stored API key for `provider_id`. Reuses
/// `AgentProviderSecrets` — the same secure-storage-backed singleton the GUI Settings AI page
/// writes to via `AISettingsPageAction::UpdateAgentProviderApiKey` — so a key set from the TUI
/// is immediately visible in the GUI and vice versa.
pub fn tui_set_agent_provider_api_key(
    app: &mut warpui::AppContext,
    provider_id: &str,
    api_key: String,
) {
    use crate::ai::agent_providers::AgentProviderSecrets;
    use warpui::SingletonEntity as _;

    AgentProviderSecrets::handle(app).update(app, |secrets, ctx| {
        secrets.set(provider_id, api_key, ctx);
    });
}

/// Clears the stored API key for `provider_id`, if any.
pub fn tui_clear_agent_provider_api_key(app: &mut warpui::AppContext, provider_id: &str) {
    use crate::ai::agent_providers::AgentProviderSecrets;
    use warpui::SingletonEntity as _;

    AgentProviderSecrets::handle(app).update(app, |secrets, ctx| {
        secrets.remove(provider_id, ctx);
    });
}

/// Whether `id` is a BYOP model whose provider currently has a *connected* key: the provider
/// isn't effectively disabled (`AgentProvider::is_usable()` — missing endpoint, no enabled
/// models, or explicitly turned off) AND a non-empty API key is stored for it. Backs the TUI
/// model picker's "(key connected)" indicator (mirrors upstream Warp's
/// `cd45ebb6f` / the GUI model picker's `Icon::Key` treatment in
/// `terminal/input/models/data_source.rs`, but against this fork's arbitrary-provider BYOP
/// model instead of upstream's fixed 4-provider `ApiKeyManager`).
///
/// Non-BYOP ids (and BYOP ids whose provider has since been removed) return `false`.
pub fn tui_agent_provider_has_connected_key(app: &warpui::AppContext, id: &LLMId) -> bool {
    use crate::ai::agent_providers::{AgentProviderSecrets, llm_id};
    use crate::settings::AISettings;
    use settings::Setting as _;
    use warpui::SingletonEntity as _;

    let Some((provider_id, _model_id)) = llm_id::decode(id) else {
        return false;
    };
    let providers = AISettings::as_ref(app).agent_providers.value();
    let Some(provider) = providers.iter().find(|p| p.id == provider_id) else {
        return false;
    };
    if !provider.is_usable() {
        return false;
    }
    AgentProviderSecrets::as_ref(app)
        .get(&provider_id)
        .is_some_and(|key| !key.is_empty())
}

/// Resolves the user-facing name for an MCP server from its installation/template
/// UUID. Returns `None` when the server is unknown (e.g. a legacy/flat MCP call
/// with no server id, or the server is not installed). Used by the TUI to surface
/// tool/server identity in permission cards and transcript labels.
pub fn mcp_server_name_for_id(uuid: &uuid::Uuid, app: &warpui::AppContext) -> Option<String> {
    crate::ai::mcp::TemplatableMCPServerManager::get_mcp_name(uuid, app)
}
