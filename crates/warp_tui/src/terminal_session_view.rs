//! Authenticated terminal-session TUI surface.
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use crate::report_error::report_error;
use async_channel::Sender;
use instant::Instant;
use parking_lot::FairMutex;
use warp::editor::{CodeEditorModel, CodeEditorModelEvent};
use warp::settings::{
    AISettings, AISettingsChangedEvent, AppEditorSettings, SettingsFileError, TuiStatuslineConfig,
    TuiTheme, TuiThemeSettings,
};
use warp::tui_export::{
    AIAgentAction, AIAgentActionId, AIAgentActionResultType, AIAgentContext, AIAgentExchangeId,
    AIAgentPtyWriteMode, AIConversation, AIConversationAutoexecuteMode, AIConversationId,
    AcceptSlashCommandOrSavedPrompt, ActiveSession, ActiveSessionEvent,
    AgentConversationEntryId, AgentConversationListEntryState, AgentConversationsModel,
    AgentInteractionMetadata, AgentViewEntryOrigin, AgentViewState, Appearance, BlockId,
    BlocklistAIActionEvent, BlocklistAIActionModel, BlocklistAIContextModel,
    BlocklistAIController, BlocklistAIHistoryEvent, BlocklistAIHistoryModel,
    BlocklistAIInputModel, CLISubagentController, CLISubagentEvent, CLISubagentTarget,
    COMMAND_REGISTRY, CancellationReason, ChangelogModel, ChangelogRequestType, ClientProfileId,
    CommandExecutionSource, ConversationFileExport, ConversationSelection,
    ConversationSelectionHandle, ExecuteCommandEvent, FORK_PREFIX, ForkConversationError,
    GitHubRepoModel,
    GitRepoStatusModel, LLMId, LLMPreferences,
    LLMPreferencesEvent, LOCAL_SKILLS_REMOTE_EXECUTION_ERROR_MESSAGE, LinkedWorkflowData,
    LoadedConversationData, ModelEvent, PRE_REWIND_PREFIX, ParsedSlashCommandInput,
    PersistenceWriter, PtyIntent, PtyIntentEvent, RepoDetectionSessionType, RepoDetectionSource,
    ServerConversationToken, SessionsEvent, ShellCommandExecutorEvent, SizeInfo, SizeUpdate,
    SkillReference, SlashCommandKind, SlashCommandSelectionBehavior, StaticCommand,
    TerminalModel, TerminalSurface, TerminalSurfaceInit, TuiMcpAction, TuiMcpManager,
    TuiMcpServerId, TuiMcpVariableValue, TuiSlashCommandDataSource,
    TuiSlashCommandDataSourceArgs, TuiUpArrowHistoryItemKind, TuiZeroStateDataSource,
    UsageCostOutcome, UserTakeOverReason, WAKEUP_THROTTLE_PERIOD, WarpConfig,
    WarpConfigUpdateEvent, block_context_from_terminal_model, build_slash_command_mixer,
    context_usage_report, conversation_cost_report, detect_possible_git_repo,
    export_conversation_markdown, loaded_subtree_rollup, log_out_tui,
    maybe_build_ai_query_upsert_event,
    prepare_conversation_block_restoration, record_autodetection_toggle_from_slash_command,
    record_saved_prompt_accepted, record_static_slash_command_accepted, saved_prompt_text_for_id,
    slash_command_selection_behavior, throttle, tui_conversation_actions_in_order,
    tui_set_active_profile,
};
use warp_core::channel::ChannelState;
use warp_core::features::FeatureFlag;
use warp_core::settings::Setting;
use warp_editor::model::CoreEditorModel;
use warp_util::local_or_remote_path::LocalOrRemotePath;
use warpui::SingletonEntity;
use warpui_core::r#async::{SpawnedFutureHandle, Timer};
use warpui_core::elements::MouseStateHandle;
use warpui_core::elements::tui::{
    TuiChildView, TuiConstrainedBox, TuiContainer, TuiElement, TuiFlex, TuiHoverable,
    TuiSelectionHandle, TuiSize, TuiText, TuiViewportedListState,
};
use warpui_core::keymap::macros::*;
use warpui_core::keymap::{self, EditableBinding, FixedBinding};
use warpui_core::platform::TerminationMode;
use warpui_core::runtime::background_luminance;
use warpui_core::{
    AppContext, Entity, EntityId, ModelHandle, TuiView, TypedActionView, ViewContext, ViewHandle,
};

use crate::agent_block::TuiBlockingChild;
use crate::alt_screen_view::AltScreenElement;
use crate::attachment_bar::{
    FOCUS_ATTACHMENTS_BINDING_NAME, TuiAttachmentBar, TuiAttachmentBarEvent, TuiAttachmentModel,
    TuiAttachmentPasteDisposition,
};
use crate::cli_agent_osc_event_publisher::{
    CliAgentOscEventPublisher, host_supports_cli_agent_notifications,
};
use crate::clipboard::copy_to_clipboard;
use crate::completions_menu::{
    TuiAcceptedCompletion, TuiCompletionsMenuEvent, TuiCompletionsMenuModel,
};
use crate::conversation_menu::{TuiConversationMenuEvent, TuiConversationMenuModel};
use crate::conversation_selection::TuiConversationSelection;
use crate::editor_interaction::TuiEditorCommand;
use crate::exit_confirmation::{CTRL_C_EXIT_WINDOW, ExitConfirmation};
use crate::inline_menu::{MAX_INLINE_MENU_ROWS, TuiInlineMenu, active_inline_menu};
use crate::input::view::TuiInputAction;
use crate::input::{TuiInputView, TuiInputViewEvent};
use crate::input_hints;
use crate::input_mode_policy::{self, TuiInputModePolicy};
use crate::input_suggestions_mode::{TuiInputSuggestionsMode, TuiInputSuggestionsModeModel};
use crate::keybindings::{
    ATTACHMENTS_AVAILABLE_FLAG, CONTEXTUAL_PLAN_TOGGLE_BINDING_NAME,
    KEYBOARD_ENHANCEMENT_AVAILABLE_FLAG, PLAN_TOGGLE_AVAILABLE_FLAG, PLAN_TOGGLE_BINDING_NAME,
    TUI_BINDING_GROUP, binding_hint,
};
use crate::mcp_install_flow::{
    TuiMcpInstallFlowAction, TuiMcpInstallFlowEvent, TuiMcpInstallFlowModel,
};
use crate::link::TuiLink;
use crate::mcp_menu::{TuiMcpMenuEvent, TuiMcpMenuModel};
use crate::orchestration_model::TuiOrchestrationModel;
use crate::orchestration_tab_bar::{
    ORCHESTRATION_TAB_BAR_FOCUSED_FLAG, TuiOrchestrationSnapshot,
    TuiOrchestrationTabNavigationAction, orchestration_tab_bar_config,
    register_orchestration_surface_bindings, render_orchestration_child_selected_tab_footer,
    render_orchestration_tab_footer,
};
use crate::read_only_menu::TuiReadOnlyMenuKind;
use ai::agent::action_result::RequestFileEditsResult;

use crate::api_keys_menu::{TuiApiKeysMenuEvent, TuiApiKeysMenuModel};
use crate::exchange_menu::{TuiExchangeMenuAction, TuiExchangeMenuEvent, TuiExchangeMenuModel};
use crate::model_menu::{TuiModelMenuEvent, TuiModelMenuModel};
use crate::pane_group::TuiPaneGroup;
use crate::platform::reveal_path_in_file_manager;
use crate::profile_menu::{TuiProfileMenuEvent, TuiProfileMenuModel};
use crate::prompt_and_command_history_menu::{
    TuiPromptAndCommandHistoryMenuEvent, TuiPromptAndCommandHistoryMenuModel,
};
use crate::prompts_menu::{TuiPromptsMenuEvent, TuiPromptsMenuModel};
use crate::resume::TuiExitSummaryHandle;
use crate::session_registry::TuiSessions;
use crate::skills_menu::{TuiSkillMenuEvent, TuiSkillMenuModel};
use crate::slash_commands::TuiSlashCommandModel;
use crate::statusline_config_view::{TuiStatuslineConfigEvent, TuiStatuslineConfigView};
use crate::tab_bar::{TuiTabBarConfig, TuiTabBarView};
use crate::terminal_background::TuiHostTerminalBackground;
use crate::terminal_content_element::TuiTerminalContentElement;
use crate::terminal_use::{
    TerminalUseInterruptAction, TuiInputTarget, hide_agent_requested_command_from_top_level,
    inline_process_owns_input, terminal_use_conversation_to_resume, terminal_use_interrupt_action,
    tui_input_target,
};
use crate::transcript_view::{TuiTranscriptView, TuiTranscriptViewEvent};
use crate::transient_hint::TransientHint;
use crate::tui_builder::TuiUiBuilder;
use crate::tui_cli_subagent_view::{
    ALLOW_BLOCKED_ACTION_KEY_BINDING, HAND_BACK_KEY_BINDING, REJECT_BLOCKED_ACTION_KEY_BINDING,
    TuiCLISubagentView,
};
use crate::tui_diff_storage::revert_file_diffs;
use crate::tui_revert_registry::TuiFileEditRevertRegistry;
use crate::ui::{abbreviate_home_prefix, conversation_restore_failed, conversation_restoring};
use crate::warping_indicator::{render_response_summary, render_warping_indicator_row};
use crate::zero_state::TuiZeroStateView;
use crate::zero_state_animation::{
    ZeroStateAnimationConfig, ZeroStateAnimationConfigEvent, ZeroStateAnimationLoadFailure,
    ZeroStateInteractionHandle,
};
mod completions;
mod input_detection;
mod shortcuts;
pub(crate) mod state;
mod status_menu;
mod statusline;
mod todo_menu;

use self::input_detection::InputDetectionState;
use self::state::TuiTerminalSessionStateModel;

/// Width used before the first layout pass pushes the real terminal width into the editor.
const INITIAL_INPUT_WIDTH: u16 = 80;
const INLINE_MENU_TOP_PADDING_ROWS: u16 = 1;
/// Rows a read-only menu may occupy above the input before it starts
/// scrolling inside its own viewport.
const MAX_READ_ONLY_MENU_ROWS: u16 = 10;
const MAX_INPUT_TEXT_ROWS: u16 = 6;
/// Top and bottom border rows plus one padding row inside each border.
const BORDERED_INPUT_CHROME_ROWS: u16 = 4;
const AUTO_APPROVE_FEEDBACK_DURATION: Duration = Duration::from_secs(3);

/// The footer hint shown while the ctrl-c exit confirmation is armed.
const CTRL_C_EXIT_HINT: &str = "ctrl-c again to exit";
/// The footer hint shown while the ctrl-c kill-child window is armed.
/// Replaces the exit hint when viewing a child agent conversation.
pub(crate) const CTRL_C_KILL_CHILD_HINT: &str = "ctrl-c again to kill child agent";
/// The footer hint shown while the agent is manually tagged into a running
/// command, telling the user how to give input back to the command. Ported
/// from the pin (`02b53fcd8`, `RUNNING_COMMAND_DETACH_HINT`) as part of #390 --
/// `handle_terminal_use_interrupt` below gives ctrl-c the same detach priority
/// as the pin so the hint's wording stays accurate.
const RUNNING_COMMAND_DETACH_HINT: &str = "ctrl-c to return to command";
const STARTING_SHELL_HINT: &str = "Starting shell...";
const SETTINGS_PARSE_FAILED_HINT: &str = "Settings failed to load: invalid syntax.";
const SETTINGS_INVALID_VALUES_HINT: &str = "Settings failed to load: invalid values.";
/// Distinct from the two above: nothing failed to load. The file is fine, it
/// just also contains lines naming settings this build doesn't have, and those
/// lines do nothing. The keys themselves are in the log.
const SETTINGS_UNKNOWN_KEYS_HINT: &str = "Settings file has unrecognized keys; they're ignored.";

/// One-line summary for the footer's transient error slot. The full detail
/// (the parse error, the list of rejected keys, or the list of unrecognized
/// keys) stays in the log, written by `settings::init` and
/// `settings::settings_file_diagnostics`.
fn settings_file_error_hint(error: &SettingsFileError) -> &'static str {
    match error {
        SettingsFileError::FileParseFailed(_) => SETTINGS_PARSE_FAILED_HINT,
        SettingsFileError::InvalidSettings(_) => SETTINGS_INVALID_VALUES_HINT,
        SettingsFileError::UnknownKeys(_) => SETTINGS_UNKNOWN_KEYS_HINT,
    }
}

fn todo_menu_is_open(mode: TuiInputSuggestionsMode) -> bool {
    matches!(
        mode,
        TuiInputSuggestionsMode::ReadOnlyMenu(TuiReadOnlyMenuKind::Todos)
    )
}

/// Fallback strings for the `/status` status menu.
const STATUS_UNAVAILABLE: &str = "\u{2014}"; // em dash
const STATUS_UNTITLED_SESSION: &str = "Untitled";
const STATUS_DEV_BUILD: &str = "dev build";
const SESSION_CAN_CANCEL_RESTORE_FLAG: &str = "TuiSessionCanCancelRestore";
const SESSION_CAN_HAND_BACK_CONTROL_FLAG: &str = "TuiSessionCanHandBackControl";
/// Set while the agent is tagged into a long-running command and blocked on
/// approval to write to it (`LongRunningCommandControlState::is_agent_blocked`).
/// Previously the only way to unblock it was clicking the "[Allow]" text in
/// `TuiCLISubagentView` -- no keyboard path existed at all.
const SESSION_CAN_ALLOW_BLOCKED_LRC_ACTION_FLAG: &str = "TuiSessionCanAllowBlockedLrcAction";
/// Set while the agent is tagged into a long-running command and blocked on
/// approval to write to it, same as [`SESSION_CAN_ALLOW_BLOCKED_LRC_ACTION_FLAG`]
/// -- gates the reject keyboard path, the equivalent of clicking "[Reject]" in
/// `TuiCLISubagentView`. Mirrors the GUI's `RejectBlockedAction { should_user_take_over: false }`.
const SESSION_CAN_REJECT_BLOCKED_LRC_ACTION_FLAG: &str = "TuiSessionCanRejectBlockedLrcAction";
/// Set while the active block is a user-controlled long-running command the
/// agent is not yet tagged into, gating the keyboard path that hands the
/// agent a manual prompt for it (mirrors the GUI's "Ask agent" affordance).
const SESSION_CAN_ATTACH_AGENT_TO_RUNNING_COMMAND_FLAG: &str =
    "TuiSessionCanAttachAgentToRunningCommand";
/// Set while the agent is manually tagged into the active running command and
/// the composer owns input, gating the keyboard path that discards the
/// unsent prompt and returns input to the running command.
const SESSION_CAN_DETACH_AGENT_FROM_RUNNING_COMMAND_FLAG: &str =
    "TuiSessionCanDetachAgentFromRunningCommand";
pub(crate) const SESSION_COMPOSER_SHORTCUTS_ACTIVE_FLAG: &str = "TuiSessionComposerShortcutsActive";
pub(crate) const TRIGGER_COMPLETIONS_BINDING_NAME: &str = "tui:session:trigger_completions";
pub(crate) const PASTE_IMAGE_BINDING_NAME: &str = "tui:session:paste_image";
pub(crate) const AUTO_APPROVE_TOGGLE_BINDING_NAME: &str = "tui:session:toggle_auto_approve";
pub(crate) const ATTACH_AGENT_TO_RUNNING_COMMAND_BINDING_NAME: &str =
    "tui:session:attach_agent_to_running_command";
pub(crate) const DETACH_AGENT_FROM_RUNNING_COMMAND_BINDING_NAME: &str =
    "tui:session:detach_agent_from_running_command";

/// Events emitted by the TUI terminal session surface.
pub(crate) enum TuiTerminalSessionEvent {
    ExecuteCommand(Box<ExecuteCommandEvent>),
    InterruptPty,
    WriteAgentInput {
        bytes: Cow<'static, [u8]>,
        mode: AIAgentPtyWriteMode,
    },
    WriteUserInput(Cow<'static, [u8]>),
    Resize(SizeUpdate),
}

impl PtyIntentEvent for TuiTerminalSessionEvent {
    fn pty_intent(&self) -> Option<PtyIntent> {
        match self {
            Self::ExecuteCommand(event) => Some(PtyIntent::ExecuteCommand((**event).clone())),
            Self::InterruptPty => Some(PtyIntent::Interrupt),
            Self::WriteAgentInput { bytes, mode } => Some(PtyIntent::WriteAgentInput {
                bytes: bytes.clone(),
                mode: *mode,
            }),
            Self::WriteUserInput(bytes) => Some(PtyIntent::WriteBytes(bytes.clone())),
            Self::Resize(size_update) => Some(PtyIntent::Resize(*size_update)),
        }
    }
}

/// Transient hint shown when a shell command is rejected because the PTY is
/// already running a command.
const COMMAND_ALREADY_RUNNING_HINT: &str = "cannot run — command already running";
const NEW_CONVERSATION_COMMAND_RUNNING_HINT: &str =
    "cannot start new conversation while terminal command is running";
const SWITCH_COMMAND_RUNNING_HINT: &str =
    "Cannot switch conversations while a command is in progress.";
const SWITCH_CONVERSATION_RUNNING_HINT: &str =
    "Cannot switch conversations while the current conversation is in progress.";
const SWITCH_LOADING_HINT: &str = "Another conversation is already loading.";
const SWITCH_UNAVAILABLE_HINT: &str = "That conversation is no longer available.";
const LOADING_CONVERSATION_HINT: &str = "Loading conversation…";
/// Shown in the footer when the configured zero-state ASCII art fails to
/// load at startup (falls back to the built-in mark) or on reload (keeps the
/// previous object). See `zero_state_animation_config`'s `AsciiArtError` and
/// `show_zero_state_ascii_load_failure` below. Ported from the pin
/// (`02b53fcd8` — see `ORACLE.md`) as part of #384.
const ZERO_STATE_ASCII_INITIAL_LOAD_FAILED_HINT: &str =
    "Could not load custom ASCII art. Using the built-in mark.";
const ZERO_STATE_ASCII_RELOAD_FAILED_HINT: &str =
    "Could not reload custom ASCII art. Keeping the current object.";

fn zero_state_ascii_load_failure_hint(failure: ZeroStateAnimationLoadFailure) -> &'static str {
    match failure {
        ZeroStateAnimationLoadFailure::InitialLoad => ZERO_STATE_ASCII_INITIAL_LOAD_FAILED_HINT,
        ZeroStateAnimationLoadFailure::Reload => ZERO_STATE_ASCII_RELOAD_FAILED_HINT,
    }
}
const COMPACT_AND_REQUIRES_CONVERSATION_HINT: &str = "/compact-and requires an active conversation";
const QUEUE_REQUIRES_CONVERSATION_HINT: &str = "/queue requires an active conversation";
const QUEUE_REQUIRES_PROMPT_HINT: &str = "/queue requires a prompt argument";
const QUEUE_QUEUED_HINT: &str = "Queued — will send when the current turn finishes";
const FORK_REQUIRES_CONVERSATION_HINT: &str = "/fork requires an active conversation";
/// Distinct from [`FORK_REQUIRES_CONVERSATION_HINT`]: a conversation IS selected,
/// it just has nothing in it yet. `fork_conversation` reports this as
/// `ForkConversationError::EmptyConversation`, and without this arm it fell through
/// to the generic [`FORK_FAILED_HINT`], which reads like a malfunction rather than a
/// refusal. The pin distinguishes the two the same way.
const FORK_EMPTY_CONVERSATION_HINT: &str = "Nothing to fork \u{2014} start a conversation first.";
const FORK_FAILED_HINT: &str = "Failed to fork the conversation";
const FORKED_HINT: &str = "Forked conversation";
const ORCHESTRATE_REQUIRES_CONVERSATION_HINT: &str = "/orchestrate requires an active conversation";
const ORCHESTRATE_REQUIRES_TASK_HINT: &str =
    "Please describe at least one task after /orchestrate (separate multiple tasks with ';')";
const EXCHANGE_MENU_REQUIRES_CONVERSATION_HINT: &str = "No active conversation to choose from";
const REWIND_FAILED_HINT: &str = "Failed to rewind the conversation";
// A rewind truncates the conversation, reverts file edits made this session
// back to their pre-edit content, and saves a pre-rewind backup conversation.
const REWOUND_HINT: &str = "Rewound conversation and reverted file edits";

/// Footer label shown while the input is in `!` shell mode. The how-to-exit
/// guidance lives in the input's placeholder ghost text, so the footer only
/// names the mode.
const SHELL_MODE_HINT: &str = "shell mode";
const STATUSLINE_SAVED_HINT: &str = "Statusline configuration saved.";
const STATUSLINE_RESET_HINT: &str = "Statusline reset to defaults.";
const STATUSLINE_PERSISTENCE_FAILED_HINT: &str = "Could not save the statusline configuration.";
const COPY_SELECTION_HINT: &str = "copied to clipboard";
const COPY_FAILED_HINT: &str = "failed to copy to clipboard";
const COPY_DEBUGGING_ID_HINT: &str = "Debugging id copied — attach it to your Phosphor issue";
const COPY_DEBUGGING_ID_NO_TOKEN_HINT: &str = "No debugging id for this conversation yet.";
const LOG_BUNDLE_FAILED_HINT: &str = "Failed to create log bundle (check logs)";
const AUTO_APPROVE_ENABLED_HINT: &str = "Auto approve on";
const AUTO_APPROVE_DISABLED_HINT: &str = "Auto approve off";
const NLD_ENABLED_HINT: &str = "Natural language detection enabled.";
const NLD_DISABLED_HINT: &str = "Natural language detection disabled.";
const NLD_PERSISTENCE_FAILED_HINT: &str = "Could not save the natural language detection setting.";
const VIM_MODE_ENABLED_HINT: &str = "Vim mode enabled.";
const VIM_MODE_DISABLED_HINT: &str = "Vim mode disabled.";
const VIM_MODE_PERSISTENCE_FAILED_HINT: &str = "Could not save the vim mode setting.";
const THEME_INVALID_ARGUMENT_HINT: &str = "Theme must be auto, light, or dark.";
const THEME_PERSISTENCE_FAILED_HINT: &str = "Could not save the theme setting.";
const COST_NO_ACTIVE_CONVERSATION_HINT: &str =
    "Cannot show conversation cost: no active conversation";
const COST_EMPTY_CONVERSATION_HINT: &str = "Cannot show conversation cost: conversation is empty";
const COST_CONVERSATION_IN_PROGRESS_HINT: &str =
    "Cannot show conversation cost: conversation is in progress";

fn log_bundle_success_message(path: &Path) -> String {
    format!("Log bundle saved to {}", path.display())
}

/// Whether Tab should focus the image-attachment bar instead of completing
/// shell input under the cursor. Shell mode reserves Tab for completion even
/// when attachments would otherwise render, so the two bindings stay mutually
/// exclusive (see the `FOCUS_ATTACHMENTS_BINDING_NAME` / `TRIGGER_COMPLETIONS_BINDING_NAME`
/// context predicates registered above, which key off `ATTACHMENTS_AVAILABLE_FLAG`).
fn attachment_focus_available(is_shell_mode: bool, attachments_should_render: bool) -> bool {
    !is_shell_mode && attachments_should_render
}

fn raw_prompt_if_not_blank(input: &str) -> Option<&str> {
    (!input.trim().is_empty()).then_some(input)
}
fn format_status_conversation_id(conversation_id: Option<AIConversationId>) -> String {
    conversation_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| "None".to_owned())
}
fn cost_command_unavailable_hint(
    selected_conversation: Option<(bool, bool)>,
) -> Option<&'static str> {
    match selected_conversation {
        None => Some(COST_NO_ACTIVE_CONVERSATION_HINT),
        Some((true, _)) => Some(COST_EMPTY_CONVERSATION_HINT),
        Some((false, false)) => Some(COST_CONVERSATION_IN_PROGRESS_HINT),
        Some((false, true)) => None,
    }
}





/// The Enter-key hint for an MCP row's primary action, or `None` when the
/// action is not one Enter performs from the menu.
fn mcp_primary_action_hint(action: TuiMcpAction) -> Option<&'static str> {
    match action {
        TuiMcpAction::Enable(_) => Some("to install and enable"),
        TuiMcpAction::Start(_) => Some("to start"),
        TuiMcpAction::Stop(_) => Some("to stop"),
        TuiMcpAction::Retry(_) => Some("to retry"),
        TuiMcpAction::ReopenAuthorization(_) => Some("to authenticate"),
        TuiMcpAction::LogOut(_) => None,
    }
}

/// The `/mcp` menu's controls row, which replaces the statusline while the menu
/// is open. Each control is omitted when the selected row cannot perform it.
fn render_mcp_menu_footer(
    builder: &TuiUiBuilder,
    primary_action: Option<TuiMcpAction>,
    can_log_out: bool,
) -> TuiFlex {
    let mut spans = Vec::new();
    if let Some(hint) = primary_action.and_then(mcp_primary_action_hint) {
        spans.extend([
            ("Enter".to_owned(), builder.primary_text_style()),
            (format!(" {hint}  "), builder.muted_text_style()),
        ]);
    }
    if can_log_out {
        spans.extend([
            ("Ctrl+R".to_owned(), builder.primary_text_style()),
            (
                " to log out & remove credentials  ".to_owned(),
                builder.muted_text_style(),
            ),
        ]);
    }
    spans.extend([
        ("Esc".to_owned(), builder.primary_text_style()),
        (" to close".to_owned(), builder.muted_text_style()),
    ]);
    TuiFlex::row().child(TuiText::from_spans(spans).truncate().finish())
}

/// The install flow's controls row, which replaces the statusline while the
/// flow is collecting values. Escape cancels, leaving nothing installed.
fn render_mcp_install_footer(
    builder: &TuiUiBuilder,
    primary_action_hint: Option<&'static str>,
) -> TuiFlex {
    let mut spans = Vec::new();
    if let Some(hint) = primary_action_hint {
        spans.extend([
            ("Enter".to_owned(), builder.primary_text_style()),
            (format!(" {hint}  "), builder.muted_text_style()),
        ]);
    }
    spans.extend([
        ("Esc".to_owned(), builder.primary_text_style()),
        (" to cancel".to_owned(), builder.muted_text_style()),
    ]);
    TuiFlex::row().child(TuiText::from_spans(spans).truncate().finish())
}

/// Entry point that requested conversation restoration.
#[derive(Clone, Copy, Debug)]
pub(crate) enum TuiConversationRestoreOrigin {
    Startup,
    ConversationList,
    /// Switching the surface to a newly forked conversation (`/fork`).
    Fork,
}

impl TuiConversationRestoreOrigin {
    fn agent_view_origin(self) -> AgentViewEntryOrigin {
        match self {
            // RestoreExistingConversation explicitly covers startup restore and forking.
            Self::Startup | Self::ConversationList | Self::Fork => {
                AgentViewEntryOrigin::RestoreExistingConversation
            }
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum TuiConversationRestoreTarget {
    Local(AIConversationId),
    Server(ServerConversationToken),
}

/// A prompt held to be submitted after a conversation's current turn finishes.
/// Used by `/compact-and` (after the summarize turn) and `/queue` (after the
/// in-flight turn). Mirrors the GUI's
/// `send_user_query_after_next_conversation_finished` callback.
///
/// `seen_in_progress` gates the send on having actually observed the turn
/// running: `/compact-and` arms it `false` and waits for the summarize turn it
/// just triggered to start; `/queue` arms it `true` because the target turn is
/// already in progress.
#[derive(Clone, Debug)]
struct TuiQueuedFollowUp {
    conversation_id: AIConversationId,
    prompt: String,
    seen_in_progress: bool,
}

/// What to do on the freshly forked conversation after switching to it.
#[derive(Clone, Debug)]
enum PostForkAction {
    /// `/fork`: send the optional initial prompt immediately.
    SendPrompt(Option<String>),
    /// `/fork-and-compact`: summarize the fork, then send the optional prompt
    /// once the summarize turn finishes (reusing the `/compact-and` machinery).
    CompactThenPrompt(Option<String>),
}

/// Trims a slash-command argument and drops it when empty, so a bare command
/// (or one with only whitespace) doesn't seed a forked conversation with an
/// empty query.
fn normalize_optional_prompt(prompt: Option<String>) -> Option<String> {
    prompt
        .map(|prompt| prompt.trim().to_owned())
        .filter(|prompt| !prompt.is_empty())
}

#[derive(Default)]
enum ConversationRestoreState {
    #[default]
    Idle,
    Loading {
        origin: TuiConversationRestoreOrigin,
        request_id: u64,
        future: Option<SpawnedFutureHandle>,
    },
    Failed(String),
}
fn export_file_success_message(export: &ConversationFileExport) -> String {
    let path = export.path().display();
    if export.overwrote_existing() {
        format!("Conversation exported to {path} (overwrote existing file)")
    } else {
        format!("Conversation exported to {path}")
    }
}

/// Typed actions handled by [`TuiTerminalSessionView`].
#[derive(Debug, Clone)]
pub(crate) enum TuiTerminalSessionAction {
    /// Ctrl-c anywhere in the session surface: cancel the running
    /// conversation, else clear the input; a second press within
    /// [`CTRL_C_EXIT_WINDOW`] exits the TUI.
    Interrupt,
    /// Cancel an in-flight conversation restore.
    CancelRestore,
    /// Return a user-controlled terminal-use command to the agent.
    HandBackTerminalUseControl,
    /// Tags the agent into the active user-controlled running command,
    /// switching the composer to a manual prompt for it.
    AttachAgentToRunningCommand,
    /// Detaches the agent from the active running command, discarding any
    /// unsent prompt and returning input to the running command.
    DetachAgentFromRunningCommand,
    /// Approve the agent's pending action on a long-running command it's
    /// driving (the keyboard path for the "[Allow]" affordance in
    /// `TuiCLISubagentView`, which was previously mouse-only).
    AllowBlockedLrcAction,
    /// Reject the agent's pending action on a long-running command it's
    /// driving, without taking over the command (the keyboard path for the
    /// "[Reject]" affordance in `TuiCLISubagentView`). Mirrors the GUI's
    /// `CLISubagentAction::RejectBlockedAction { should_user_take_over: false }`.
    RejectBlockedLrcAction,
    /// Toggle the completed-response summary for the selected conversation.
    ///
    /// Retained but currently unbound. Upstream Warp's TUI `/cost` dispatches this; here
    /// `/cost` reports token spend at the user's configured provider rates instead (see the
    /// `SlashCommandKind::Cost` arm), because the summary's money half is Warp's server-computed
    /// `block_credits`, which is structurally zero under BYOP. The action, its handler and
    /// `toggle_response_summary_visibility` are deliberately kept rather than deleted — the
    /// summary's duration half is still live and still rendered, so a future keybinding or
    /// command can drive this without re-porting the mechanism (AGENTS §5.10).
    #[allow(dead_code)]
    ToggleResponseSummaryVisibility,
    /// Toggle the selected conversation's active TODO list above the input.
    ToggleTodoMenu,
    /// Click on the footer's active-model label: toggles the inline model
    /// picker (the same menu `/model` surfaces).
    ToggleModelMenu,
    /// Toggle per-conversation auto approve.
    ToggleAutoApprove { show_feedback: bool },
    /// Open a URL from an interactive statusline item.
    OpenUrl(String),
    /// Raw user bytes to forward to the foreground PTY process.
    ForwardUserPtyBytes(Vec<u8>),
    /// Ctrl-d while the prompt is focused: exit the TUI immediately when the
    /// prompt is empty, else delete the next character.
    Eof,
    /// Toggle the latest exposed inline plan.
    TogglePlan,
    /// Move focus from the prompt input into the attachment bar.
    FocusAttachments,
    /// Paste host clipboard text or attach image data and image paths.
    PasteFromClipboard,
    /// Tab: open the shell command/path completion popup for the token under the
    /// cursor, or cycle to the next candidate when it is already open.
    TriggerCompletions,
    /// Left-click on the inline menu at absolute snapshot index `index`:
    /// selects and accepts that row.
    InlineMenuMouseAcceptRow(usize),
    /// Scroll-wheel over the inline menu: scrolls the viewport by `delta` rows
    /// without changing the selection.
    InlineMenuMouseScrollBy(isize),
    /// A drag selection started inside the shared read-only menu
    /// (shortcuts/status).
    ReadOnlyMenuSelectionStarted,
    /// A non-empty read-only menu selection completed.
    ReadOnlyMenuSelectionEnded(String),
    /// Return keyboard focus from the orchestration tabs to the session's
    /// default interaction target.
    FocusDefaultInteractionTarget,
    /// Return to the main/root orchestration agent and focus its input.
    ///
    /// When a child tab is selected, switches the focused session to the
    /// root/main agent; when the root is already selected, only clears tab
    /// focus and restores input focus.
    FocusMainOrchestrationTab,
    /// Navigate the orchestration tabs using their semantic order.
    NavigateOrchestrationTabs(TuiOrchestrationTabNavigationAction),
}

/// The authenticated terminal/session surface rendered inside [`RootTuiView`].
pub(crate) struct TuiTerminalSessionView {
    transcript: ViewHandle<TuiTranscriptView>,
    input_view: ViewHandle<TuiInputView>,
    attachment_bar: ViewHandle<TuiAttachmentBar>,
    inline_menus: Vec<TuiInlineMenu>,
    suggestions_mode: ModelHandle<TuiInputSuggestionsModeModel>,
    /// Retained selection state for the shared read-only menu (shortcuts/status/todos),
    /// so a mouse-drag selection over it survives across re-renders.
    read_only_menu_selection: TuiSelectionHandle,
    /// Retained scroll state for the shared read-only menu, so wheel scrolling
    /// survives the element-tree rebuild every re-render performs.
    read_only_menu_viewport: TuiViewportedListState,
    /// The selected conversation and active TODO-list generation currently
    /// displayed by an open TODO menu.
    open_todo_menu_list_key: Option<(AIConversationId, usize)>,
    /// Resolves a fresh session-interaction projection on demand; only
    /// consumed by the `?`-opened shortcuts menu today.
    session_state: TuiTerminalSessionStateModel,
    conversation_menu: ModelHandle<TuiConversationMenuModel>,
    model_menu: ModelHandle<TuiModelMenuModel>,
    completions_menu: ModelHandle<TuiCompletionsMenuModel>,
    profile_menu: ModelHandle<TuiProfileMenuModel>,
    prompts_menu: ModelHandle<TuiPromptsMenuModel>,
    exchange_menu: ModelHandle<TuiExchangeMenuModel>,
    api_keys_menu: ModelHandle<TuiApiKeysMenuModel>,
    /// A follow-up prompt queued by `/compact-and`, submitted once the
    /// summarize exchange on its conversation completes. See
    /// [`Self::maybe_send_queued_follow_up`].
    queued_follow_up: Option<TuiQueuedFollowUp>,
    /// In-flight Tab-completion fetch from the shared completer engine, plus
    /// the request generation and menu snapshot used to detect staleness.
    completion_request: completions::CompletionRequestState,
    skills_menu: ModelHandle<TuiSkillMenuModel>,
    mcp_menu: ModelHandle<TuiMcpMenuModel>,
    mcp_install_flow: ModelHandle<TuiMcpInstallFlowModel>,
    slash_commands_source: ModelHandle<TuiSlashCommandDataSource>,
    conversation_selection: ConversationSelectionHandle,
    ai_action_model: ModelHandle<BlocklistAIActionModel>,
    /// Set only for the root TUI session, and only when the hosting terminal
    /// advertises the CLI-agent protocol. See
    /// [`Self::enable_cli_agent_osc_event_publishing`].
    cli_agent_osc_event_publisher: Option<ModelHandle<CliAgentOscEventPublisher>>,
    ai_controller: ModelHandle<BlocklistAIController>,
    cli_subagent_controller: ModelHandle<CLISubagentController>,
    cli_subagent_views: HashMap<BlockId, ViewHandle<TuiCLISubagentView>>,
    /// Read by the footer for the active session's working directory.
    active_session: ModelHandle<ActiveSession>,
    /// Repository currently containing the active session's working directory.
    current_repo_path: Option<LocalOrRemotePath>,
    /// Watcher-backed branch and uncommitted diff metadata for the footer.
    git_repo_status: Option<ModelHandle<GitRepoStatusModel>>,
    /// GitHub metadata for the current repository, including current-branch PR
    /// info. Retained only while the `GitHubPullRequest` statusline item is
    /// enabled -- see `update_github_status_subscription`.
    github_repo: Option<ModelHandle<GitHubRepoModel>>,
    /// This view's surface id, used to resolve the active model for the footer
    /// the same way the request path does.
    terminal_surface_id: EntityId,
    /// Armed by a ctrl-c press; a second press while armed exits the TUI.
    /// The footer shows [`CTRL_C_EXIT_HINT`] while armed.
    exit_confirmation: ExitConfirmation,
    /// When set, the `exit_confirmation` window was armed to kill this child
    /// rather than exit the TUI. The footer shows [`CTRL_C_KILL_CHILD_HINT`]
    /// while armed, and a second ctrl-c within the window kills the child.
    child_kill_armed_conversation: Option<AIConversationId>,
    /// Last-response exchanges whose completed summary has been hidden with
    /// `/cost`. A later response has a new exchange ID and starts visible,
    /// matching the GUI's per-last-block state.
    hidden_response_summary_exchange_ids: HashSet<AIAgentExchangeId>,
    /// Hover state for the footer's clickable active-model label, owned here
    /// (not created inline during render) so it survives element-tree rebuilds,
    /// following the GUI's `MouseStateHandle` pattern.
    model_label_hover: MouseStateHandle,
    /// Hover state for the footer's clickable GitHub pull-request link.
    github_pr_link: TuiLink,
    /// Hover and click state for the configured TODO statusline control.
    todo_list_mouse: MouseStateHandle,
    keyboard_enhancement_supported: bool,
    ai_context_model: ModelHandle<BlocklistAIContextModel>,
    ai_input_model: ModelHandle<BlocklistAIInputModel>,
    input_detection: InputDetectionState,
    terminal_model: Arc<FairMutex<TerminalModel>>,
    /// Last dimensions applied to the terminal model and PTY.
    size_info: SizeInfo,
    /// Reports the area allocated to whichever element displays PTY content
    /// (the block-list content column or the full-screen alt-screen grid).
    /// This layout→channel→view pathway is the GUI's terminal-resize prior
    /// art (`TerminalSizeElement::after_layout` → `resize_tx` →
    /// `after_terminal_view_layout`): layout lacks a `ViewContext`, so the
    /// settled size is handed off to a view-side handler to apply.
    terminal_resize_tx: Sender<TuiSize>,
    /// Transient notice shown in the footer's hint slot (e.g. a rejected
    /// shell submission).
    transient_hint: TransientHint,
    auto_approve_feedback_conversation_id: Option<AIConversationId>,
    auto_approve_feedback_timer: Option<SpawnedFutureHandle>,
    /// Retained mouse state for the *footer's* clickable auto-approve control
    /// (`render_auto_approve_statusline`). Kept separate from
    /// [`Self::warping_auto_approve_mouse`] so the two controls -- which can
    /// be on screen at the same time -- never share hover/armed-click state:
    /// pressing one must not arm or cancel the other. Matches the pin's
    /// `footer_auto_approve_mouse`/`warping_auto_approve_mouse` split.
    footer_auto_approve_mouse: MouseStateHandle,
    /// Retained mouse state for the auto-approve control inside the warping
    /// indicator row (`render_warping_indicator`). See
    /// [`Self::footer_auto_approve_mouse`].
    warping_auto_approve_mouse: MouseStateHandle,
    conversation_restore_state: ConversationRestoreState,
    next_restore_request_id: u64,
    exit_summary: TuiExitSummaryHandle,
    /// The view id of the blocker currently holding focus, tracked only to
    /// detect blocker transitions in [`Self::sync_blocker_focus`]. Input
    /// visibility itself is derived at render time, never stored.
    active_blocker_view_id: Option<EntityId>,
    /// The `/statusline` configuration picker, mounted in place of the input
    /// box while open.
    statusline_config_view: Option<ViewHandle<TuiStatuslineConfigView>>,
    /// Session-owned zero-state drag/flick state, kept outside the view so it
    /// survives the element rebuild every repaint performs.
    zero_state_interaction: ZeroStateInteractionHandle,
    zero_state_view: ViewHandle<TuiZeroStateView>,
    /// Workflow metadata for a command accepted from the up-arrow
    /// prompt-and-command history menu (issue #387), consumed the next time
    /// [`Self::execute_user_command`] runs so `ExecuteCommandEvent` carries it
    /// through to execution. `None` for an ordinary typed shell submission.
    pending_history_command_workflow_data: Option<LinkedWorkflowData>,
    /// Retained child view rendering this session's orchestration tab bar.
    /// Empty (no tabs) outside an orchestration tree.
    orchestration_tab_bar: ViewHandle<TuiTabBarView>,
    /// Whether keyboard focus is on the orchestration tabs rather than the
    /// session's normal input target.
    orchestration_tabs_focused: bool,
    /// Set while the composer's current AI lock was installed to give the
    /// agent a running command's input (manual attach, an auto-spawned CLI
    /// subagent, or a prompt sent to one) rather than by the user's own
    /// Ctrl+Shift+I toggle -- installed via [`Self::lock_for_agent_control`].
    /// Gates [`Self::reset_after_agent_control`] so a completing block only
    /// restores autodetection when this composer is the one that locked it --
    /// otherwise an unrelated block completing elsewhere would clobber a
    /// genuine user-forced lock. Local stand-in for
    /// the pin's `InputTypeAutoDetectionSource::AgentTerminalControl`
    /// (`02b53fcd8`): this fork's shared `BlocklistAIInputModel` intentionally
    /// carries only one autodetection-source variant (`HistoryMatch`, see
    /// `app/src/ai/blocklist/input_model.rs`), and threading a second variant
    /// through `set_input_config` end-to-end is the much larger, separately
    /// tracked #399/#254 item d, not this issue's scope.
    agent_terminal_control_lock: bool,
}

/// Registers the session surface's keybindings. Called once at TUI startup
/// from `keybindings::init`. Ctrl-c is a fixed (non-remappable) binding,
/// mirroring peer agent CLIs that treat it as reserved.
pub(crate) fn init(app: &mut AppContext) {
    let view_context = id!(TuiTerminalSessionView::ui_name());
    // Ctrl-c is a reserved fixed binding on the session surface (cancel /
    // clear / exit), mirroring peer agent CLIs. Registered together with the
    // orchestration tab bar's own ctrl-c interrupt binding and its tab
    // navigation bindings, since both are scoped to this same view context.
    register_orchestration_surface_bindings(
        app,
        view_context.clone(),
        TuiTerminalSessionAction::Interrupt,
        TuiTerminalSessionAction::NavigateOrchestrationTabs,
    );
    app.register_fixed_bindings([
        FixedBinding::new(
            "ctrl-d",
            TuiTerminalSessionAction::Eof,
            id!(TuiInputView::ui_name()),
        )
        .with_group(TUI_BINDING_GROUP),
        FixedBinding::new(
            "escape",
            TuiTerminalSessionAction::CancelRestore,
            id!(SESSION_CAN_CANCEL_RESTORE_FLAG),
        )
        .with_group(TUI_BINDING_GROUP),
        FixedBinding::new(
            HAND_BACK_KEY_BINDING,
            TuiTerminalSessionAction::HandBackTerminalUseControl,
            id!(SESSION_CAN_HAND_BACK_CONTROL_FLAG),
        )
        .with_group(TUI_BINDING_GROUP),
        FixedBinding::new(
            ALLOW_BLOCKED_ACTION_KEY_BINDING,
            TuiTerminalSessionAction::AllowBlockedLrcAction,
            id!(SESSION_CAN_ALLOW_BLOCKED_LRC_ACTION_FLAG),
        )
        .with_group(TUI_BINDING_GROUP),
        FixedBinding::new(
            REJECT_BLOCKED_ACTION_KEY_BINDING,
            TuiTerminalSessionAction::RejectBlockedLrcAction,
            id!(SESSION_CAN_REJECT_BLOCKED_LRC_ACTION_FLAG),
        )
        .with_group(TUI_BINDING_GROUP),
    ]);
    app.register_editable_bindings([
        EditableBinding::new(
            AUTO_APPROVE_TOGGLE_BINDING_NAME,
            "Toggle auto approve",
            TuiTerminalSessionAction::ToggleAutoApprove {
                show_feedback: true,
            },
        )
        .with_context_predicate(view_context.clone())
        .with_group(TUI_BINDING_GROUP)
        .with_key_binding("ctrl-shift-I"),
        EditableBinding::new(
            ATTACH_AGENT_TO_RUNNING_COMMAND_BINDING_NAME,
            "Use the agent with the running command",
            TuiTerminalSessionAction::AttachAgentToRunningCommand,
        )
        .with_context_predicate(
            (id!(TuiInputView::ui_name()) | view_context.clone())
                & id!(SESSION_CAN_ATTACH_AGENT_TO_RUNNING_COMMAND_FLAG),
        )
        .with_group(TUI_BINDING_GROUP)
        .with_key_binding("ctrl-shift-enter"),
        EditableBinding::new(
            DETACH_AGENT_FROM_RUNNING_COMMAND_BINDING_NAME,
            "Return control to the running command",
            TuiTerminalSessionAction::DetachAgentFromRunningCommand,
        )
        .with_context_predicate(
            (id!(TuiInputView::ui_name()) | view_context.clone())
                & id!(SESSION_CAN_DETACH_AGENT_FROM_RUNNING_COMMAND_FLAG),
        )
        .with_group(TUI_BINDING_GROUP)
        .with_key_binding("escape"),
        EditableBinding::new(
            PLAN_TOGGLE_BINDING_NAME,
            "Toggle the latest plan",
            TuiTerminalSessionAction::TogglePlan,
        )
        .with_context_predicate(view_context.clone())
        .with_group(TUI_BINDING_GROUP)
        .with_key_binding("ctrl-shift-P"),
        EditableBinding::new(
            CONTEXTUAL_PLAN_TOGGLE_BINDING_NAME,
            "Toggle the latest visible plan",
            TuiTerminalSessionAction::TogglePlan,
        )
        .with_context_predicate(
            (id!(TuiInputView::ui_name()) | id!(TuiTerminalSessionView::ui_name()))
                & id!(PLAN_TOGGLE_AVAILABLE_FLAG)
                & !id!(KEYBOARD_ENHANCEMENT_AVAILABLE_FLAG),
        )
        .with_group(TUI_BINDING_GROUP)
        .with_key_binding("ctrl-p"),
        EditableBinding::new(
            FOCUS_ATTACHMENTS_BINDING_NAME,
            "Focus image attachments",
            TuiTerminalSessionAction::FocusAttachments,
        )
        .with_context_predicate(
            (id!(TuiInputView::ui_name()) | id!(TuiTerminalSessionView::ui_name()))
                & id!(ATTACHMENTS_AVAILABLE_FLAG),
        )
        .with_group(TUI_BINDING_GROUP)
        .with_key_binding("tab"),
        // Tab completes the token under the cursor when there are no image
        // attachments to focus (which reserve Tab). Gated as the mutually
        // exclusive complement of the FocusAttachments binding above.
        EditableBinding::new(
            TRIGGER_COMPLETIONS_BINDING_NAME,
            "Complete the command or path under the cursor",
            TuiTerminalSessionAction::TriggerCompletions,
        )
        .with_context_predicate(
            (id!(TuiInputView::ui_name()) | id!(TuiTerminalSessionView::ui_name()))
                & !id!(ATTACHMENTS_AVAILABLE_FLAG),
        )
        .with_group(TUI_BINDING_GROUP)
        .with_key_binding("tab"),
        EditableBinding::new(
            PASTE_IMAGE_BINDING_NAME,
            "Paste from the clipboard",
            TuiTerminalSessionAction::PasteFromClipboard,
        )
        .with_context_predicate(
            (id!(TuiInputView::ui_name()) | id!(TuiTerminalSessionView::ui_name()))
                & id!(SESSION_COMPOSER_SHORTCUTS_ACTIVE_FLAG),
        )
        .with_group(TUI_BINDING_GROUP)
        .with_key_binding("ctrl-v"),
        #[cfg(windows)]
        EditableBinding::new(
            PASTE_IMAGE_BINDING_NAME,
            "Paste from the clipboard",
            TuiTerminalSessionAction::PasteFromClipboard,
        )
        .with_context_predicate(
            (id!(TuiInputView::ui_name()) | id!(TuiTerminalSessionView::ui_name()))
                & id!(SESSION_COMPOSER_SHORTCUTS_ACTIVE_FLAG),
        )
        .with_group(TUI_BINDING_GROUP)
        .with_key_binding("alt-v"),
    ]);

    let tab_context = view_context & id!(ORCHESTRATION_TAB_BAR_FOCUSED_FLAG);
    app.register_editable_bindings([
        EditableBinding::new(
            "tui:orchestration_tabs:focus_input",
            "Return focus to the session input",
            TuiTerminalSessionAction::FocusDefaultInteractionTarget,
        )
        .with_context_predicate(tab_context.clone())
        .with_group(TUI_BINDING_GROUP)
        .with_key_binding("down"),
        EditableBinding::new(
            "tui:orchestration_tabs:focus_input",
            "Return focus to the session input",
            TuiTerminalSessionAction::FocusDefaultInteractionTarget,
        )
        .with_context_predicate(tab_context.clone())
        .with_group(TUI_BINDING_GROUP)
        .with_key_binding("shift-down"),
        EditableBinding::new(
            "tui:orchestration_tabs:focus_main",
            "Return to the main agent and focus its input",
            TuiTerminalSessionAction::FocusMainOrchestrationTab,
        )
        .with_context_predicate(tab_context)
        .with_group(TUI_BINDING_GROUP)
        .with_key_binding("escape"),
    ]);
}

impl TuiTerminalSessionView {
    /// Selects the sole input destination for the current terminal lifecycle
    /// state. The result drives focus, rendering, and event routing together.
    fn input_target(&self) -> TuiInputTarget {
        let terminal_model = self.terminal_model.lock();
        tui_input_target(&terminal_model)
    }

    fn update_process_input_focus(&mut self, ctx: &mut ViewContext<Self>) {
        self.focus_current_owner_if_active(ctx);
    }

    fn focus_blocking_child(blocker: TuiBlockingChild, ctx: &mut ViewContext<Self>) {
        match blocker {
            TuiBlockingChild::AskQuestion(view) => {
                // `TuiAskQuestionView` has no public `focus`; it routes focus to its
                // option selector from `on_focus` (tui_ask_question_view.rs:621), and
                // only when it is still awaiting answers. Focus the view and let that
                // handler run, rather than reaching past it into the selector.
                ctx.focus(&view);
            }
            TuiBlockingChild::Permission(view) => {
                view.update(ctx, |view, ctx| view.focus(ctx));
            }
        }
    }

    fn focus_current_owner(&mut self, ctx: &mut ViewContext<Self>) {
        match self.input_target() {
            TuiInputTarget::Disabled | TuiInputTarget::AgentEditor => {
                if let Some(blocker) = self.active_blocking_child(ctx) {
                    self.orchestration_tabs_focused = false;
                    Self::focus_blocking_child(blocker, ctx);
                } else if let Some(statusline_config_view) = self.statusline_config_view.as_ref() {
                    self.orchestration_tabs_focused = false;
                    statusline_config_view.update(ctx, |view, ctx| view.focus(ctx));
                } else if self.orchestration_tabs_focused {
                    ctx.focus_self();
                } else {
                    ctx.focus(&self.input_view);
                }
            }
            TuiInputTarget::Pty => {
                self.orchestration_tabs_focused = false;
                ctx.focus_self();
            }
        }
    }

    fn focus_current_owner_if_active(&mut self, ctx: &mut ViewContext<Self>) {
        if self.is_focused_session(ctx) {
            let tabs_were_focused = self.orchestration_tabs_focused;
            self.focus_current_owner(ctx);
            if tabs_were_focused && !self.orchestration_tabs_focused {
                self.refresh_orchestration_tab_bar(ctx);
                ctx.notify();
            }
        }
    }

    fn focus_input_if_active(&self, ctx: &mut ViewContext<Self>) {
        if self.is_focused_session(ctx) {
            ctx.focus(&self.input_view);
        }
    }

    /// Restores a child conversation's persisted transcript onto this fresh
    /// background child surface.
    ///
    /// This mirrors the surface-restoration half of
    /// [`Self::replace_conversation_surface`] but for a newly created child
    /// session that has no previous conversation to clear. It reuses the shared
    /// block-restoration plan, action-result restoration, history association,
    /// and transcript restoration. It deliberately does **not** relaunch the
    /// child or resend its prompt — the child keeps its persisted status.
    ///
    /// Ported from `40ac1d4b1`; the pin's "or create a server task" clause is
    /// moot here, this fork has no task creation to avoid.
    pub(crate) fn restore_orchestrated_child_conversation(
        &mut self,
        conversation: AIConversation,
        ctx: &mut ViewContext<Self>,
    ) {
        let conversation_id = conversation.id();
        let restoration_plan = {
            let mut terminal_model = self.terminal_model.lock();
            prepare_conversation_block_restoration(&conversation, &mut terminal_model)
        };

        self.ai_action_model.update(ctx, |actions, _| {
            actions.restore_action_results_from_exchanges(restoration_plan.exchanges().collect());
        });

        BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, ctx| {
            history.restore_conversations(self.terminal_surface_id, vec![conversation], ctx);
        });

        self.transcript.update(ctx, |transcript, ctx| {
            transcript.restore_conversation(conversation_id, restoration_plan, ctx);
        });

        BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, ctx| {
            history.set_active_conversation_id(conversation_id, self.terminal_surface_id, ctx);
        });

        self.conversation_selection.update(ctx, |selection, ctx| {
            selection.select_existing_conversation(
                conversation_id,
                AgentViewEntryOrigin::RestoreExistingConversation,
                ctx,
            );
        });

        ctx.notify();
    }

    /// Resolves live semantic orchestration state for this session.
    fn compute_orchestration_tab_snapshot(
        &self,
        ctx: &AppContext,
    ) -> Option<TuiOrchestrationSnapshot> {
        if !ctx.has_singleton_model::<TuiOrchestrationModel>()
            || !ctx.has_singleton_model::<TuiSessions>()
        {
            return None;
        }
        let selected_conversation_id = self
            .conversation_selection
            .as_ref(ctx)
            .selected_conversation_id(ctx)?;
        TuiOrchestrationModel::as_ref(ctx).snapshot(selected_conversation_id, ctx)
    }

    /// Refreshes this session's retained bar from live semantic state.
    pub(crate) fn refresh_orchestration_tab_state(&mut self, ctx: &mut ViewContext<Self>) {
        let snapshot = self.compute_orchestration_tab_snapshot(ctx);
        let tabs_were_available = self.orchestration_tab_bar.as_ref(ctx).has_tabs();
        if let Some(snapshot) = snapshot.as_ref() {
            let builder = TuiUiBuilder::from_app(ctx);
            self.sync_orchestration_tab_bar(snapshot, &builder, ctx);
        } else {
            self.clear_orchestration_tab_bar(ctx);
        }
        let tabs_are_available = self.orchestration_tab_bar.as_ref(ctx).has_tabs();
        let availability_changed = tabs_were_available != tabs_are_available;
        let mut focus_changed = false;
        if !tabs_are_available && self.orchestration_tabs_focused {
            self.orchestration_tabs_focused = false;
            focus_changed = true;
            self.focus_current_owner(ctx);
        }
        // Disarm the child-kill window when the child is no longer reachable.
        if !tabs_are_available && self.child_kill_armed_conversation.is_some() {
            self.exit_confirmation.disarm();
            self.child_kill_armed_conversation = None;
            focus_changed = true;
        }
        if availability_changed || focus_changed {
            ctx.notify();
        }
    }

    /// If the orchestration snapshot shows a child conversation selected (not
    /// the tree root), returns that child's conversation id. Used by the
    /// unfocused-bar armed-kill window while viewing a child conversation.
    fn is_child_conversation_selected(&self, ctx: &AppContext) -> Option<AIConversationId> {
        let snapshot = self.compute_orchestration_tab_snapshot(ctx)?;
        (snapshot.selected_conversation_id != snapshot.root_conversation_id)
            .then_some(snapshot.selected_conversation_id)
    }

    /// The kill target while the bar is focused, with its loaded-descendant
    /// count: a selected child tab of the rendered level, or the drilled-in
    /// anchor itself when it occupies the main-tab slot (anchor != root). The
    /// root tab is never a kill target. Drives the bar-focused single-press
    /// kill path and its footer.
    fn bar_focused_kill_target(&self, ctx: &AppContext) -> Option<(AIConversationId, usize)> {
        let snapshot = self.compute_orchestration_tab_snapshot(ctx)?;
        if snapshot.selected_conversation_id != snapshot.anchor_conversation_id {
            let nested_descendants = snapshot
                .children
                .iter()
                .find(|child| child.conversation_id == snapshot.selected_conversation_id)
                .and_then(|child| child.subtree_rollup.as_ref())
                .map(|rollup| rollup.descendant_count)
                .unwrap_or_default();
            return Some((snapshot.selected_conversation_id, nested_descendants));
        }
        // A drilled-in anchor only exists under multi-level orchestration
        // (flag off keeps anchor == root), so single-press subtree kill
        // cannot reach flag-off trees.
        if snapshot.anchor_conversation_id == snapshot.root_conversation_id {
            return None;
        }
        let nested_descendants = loaded_subtree_rollup(
            BlocklistAIHistoryModel::as_ref(ctx),
            snapshot.anchor_conversation_id,
        )
        .map(|rollup| rollup.descendant_count)
        .unwrap_or_default();
        Some((snapshot.anchor_conversation_id, nested_descendants))
    }

    /// Kills a child agent and (with multi-level orchestration enabled) its
    /// entire loaded subtree, deepest-first: cancels in-flight work, deletes
    /// the conversations from history, drops their retained TUI sessions, and
    /// returns focus to the root orchestration agent.
    fn kill_child_agent(&mut self, conversation_id: AIConversationId, ctx: &mut ViewContext<Self>) {
        // Clear any armed kill or exit window.
        self.exit_confirmation.disarm();
        self.child_kill_armed_conversation = None;
        // Return tab bar to unfocused state before the session is removed so
        // the focus fall-back lands on the right surface.
        self.orchestration_tabs_focused = false;
        // Resolve the root session id BEFORE the kill clears the snapshot.
        // We bypass `focus_conversation_session` because after the child is
        // deleted the parent is no longer an orchestration root (no children),
        // and that helper gates on the root check.
        let root_session_id = self
            .compute_orchestration_tab_snapshot(ctx)
            .and_then(|snapshot| {
                let history = BlocklistAIHistoryModel::as_ref(ctx);
                TuiSessions::as_ref(ctx)
                    .session_ids_by_conversation(history)
                    .get(&snapshot.root_conversation_id)
                    .copied()
            });
        // Cancel + delete + remove sessions via the orchestration model.
        TuiOrchestrationModel::handle(ctx).update(ctx, |model, ctx| {
            model.kill_child_agent_subtree(conversation_id, ctx);
        });
        // Focus the root session directly using the pre-kill resolved id.
        if let Some(session_id) = root_session_id {
            TuiSessions::handle(ctx).update(ctx, |sessions, ctx| {
                sessions.focus_session(session_id, ctx);
            });
        } else {
            self.set_orchestration_tab_focus(false, ctx);
        }
    }

    /// Applies tab-focus mode, synchronizes presentation, and resolves the focus owner.
    pub(crate) fn set_orchestration_tab_focus(
        &mut self,
        focused: bool,
        ctx: &mut ViewContext<Self>,
    ) {
        self.orchestration_tabs_focused = focused;
        self.focus_current_owner(ctx);
        self.refresh_orchestration_tab_bar(ctx);
        ctx.notify();
    }

    /// Recomputes the retained tab bar's configuration from live state,
    /// without touching focus.
    fn refresh_orchestration_tab_bar(&self, ctx: &mut ViewContext<Self>) {
        if let Some(snapshot) = self.compute_orchestration_tab_snapshot(ctx) {
            let builder = TuiUiBuilder::from_app(ctx);
            self.sync_orchestration_tab_bar(&snapshot, &builder, ctx);
        }
    }

    fn switch_to_orchestration_tab(
        &mut self,
        key: Option<String>,
        keep_tab_focus: bool,
        ctx: &mut ViewContext<Self>,
    ) {
        let Some(conversation_id) = key.and_then(|key| AIConversationId::try_from(key).ok()) else {
            return;
        };
        self.switch_to_orchestration_conversation(conversation_id, keep_tab_focus, ctx);
    }

    /// Switches to the retained session that owns an orchestration conversation.
    fn switch_to_orchestration_conversation(
        &mut self,
        conversation_id: AIConversationId,
        keep_tab_focus: bool,
        ctx: &mut ViewContext<Self>,
    ) {
        let session_id = TuiOrchestrationModel::handle(ctx).update(ctx, |model, ctx| {
            model.focus_conversation_session(conversation_id, ctx)
        });
        let Some(session_id) = session_id else {
            return;
        };
        if session_id.surface_id() == self.terminal_surface_id {
            self.refresh_orchestration_tab_state(ctx);
            self.set_orchestration_tab_focus(keep_tab_focus, ctx);
            return;
        }
        self.orchestration_tabs_focused = false;
        ctx.notify();
        TuiSessions::set_orchestration_tab_focus(session_id, keep_tab_focus, ctx);
    }

    /// Synchronizes the retained tab child view from current orchestration state.
    fn sync_orchestration_tab_bar(
        &self,
        snapshot: &TuiOrchestrationSnapshot,
        builder: &TuiUiBuilder,
        ctx: &mut ViewContext<Self>,
    ) {
        let config =
            orchestration_tab_bar_config(snapshot, self.orchestration_tabs_focused, builder);
        self.set_orchestration_tab_bar_config(config, ctx);
    }

    fn clear_orchestration_tab_bar(&self, ctx: &mut ViewContext<Self>) {
        self.set_orchestration_tab_bar_config(TuiTabBarConfig::new(Vec::new()), ctx);
    }

    fn set_orchestration_tab_bar_config(
        &self,
        config: TuiTabBarConfig,
        ctx: &mut ViewContext<Self>,
    ) {
        let result = self
            .orchestration_tab_bar
            .update(ctx, |tab_bar, ctx| tab_bar.set_config(config, ctx));
        if let Err(error) = result {
            report_error!(
                anyhow::Error::new(error)
                    .context("Failed to update orchestration tab bar configuration")
            );
        }
    }

    fn resume_after_user_controlled_command(
        &mut self,
        block_id: &BlockId,
        ctx: &mut ViewContext<Self>,
    ) {
        let conversation_id = {
            let terminal_model = self.terminal_model.lock();
            terminal_use_conversation_to_resume(&terminal_model, block_id)
        };
        let Some(conversation_id) = conversation_id else {
            return;
        };
        let resume_context = {
            let terminal_model = self.terminal_model.lock();
            block_context_from_terminal_model(&terminal_model, block_id, false)
                .map(Box::new)
                .map(AIAgentContext::Block)
                .into_iter()
                .collect()
        };
        self.ai_controller.update(ctx, |controller, ctx| {
            controller.resume_conversation(
                conversation_id,
                /*can_attempt_resume_on_error*/ true,
                /*is_auto_resume_after_error*/ false,
                resume_context,
                ctx,
            );
        });
    }

    /// Handles any block completing. Restores the composer's agent-control
    /// lock (a no-op unless this composer installed one -- see
    /// [`Self::agent_terminal_control_lock`]) before resuming an
    /// agent-originated long-running command's conversation, so a manually
    /// tagged-in command that simply finishes on its own -- never detached via
    /// escape or ctrl-c -- still unlocks the composer instead of leaving it
    /// stuck in AI mode. Ported from the pin's `handle_block_completed`
    /// (`02b53fcd8`) for #390.
    fn handle_block_completed(&mut self, block_id: &BlockId, ctx: &mut ViewContext<Self>) {
        self.reset_after_agent_control(ctx);
        self.resume_after_user_controlled_command(block_id, ctx);
        self.update_process_input_focus(ctx);
        ctx.notify();
    }

    fn detach_cli_subagent_view(
        &mut self,
        block_id: &BlockId,
        initial_requested_command_action_id: Option<&AIAgentActionId>,
        ctx: &mut ViewContext<Self>,
    ) {
        if let Some(view) = self.cli_subagent_views.remove(block_id) {
            self.transcript.update(ctx, |transcript, ctx| {
                transcript.detach_cli_subagent(initial_requested_command_action_id, view.id(), ctx);
            });
        }
        self.focus_input_if_active(ctx);
    }
    fn handle_cli_subagent_event(&mut self, event: &CLISubagentEvent, ctx: &mut ViewContext<Self>) {
        match event {
            CLISubagentEvent::SpawnedSubagent {
                block_id,
                initial_requested_command_action_id,
                ..
            } => {
                hide_agent_requested_command_from_top_level(
                    &self.terminal_model,
                    initial_requested_command_action_id.as_ref(),
                );
                self.lock_for_agent_control(ctx);
                if let Some(target) = self
                    .cli_subagent_controller
                    .as_ref(ctx)
                    .target_for_block(block_id)
                {
                    let controller = self.cli_subagent_controller.clone();
                    let action_model = self.ai_action_model.clone();
                    let terminal_model = self.terminal_model.clone();
                    let view = ctx.add_typed_action_tui_view(|ctx| {
                        TuiCLISubagentView::new(
                            target,
                            controller,
                            action_model,
                            terminal_model,
                            ctx,
                        )
                    });
                    self.transcript.update(ctx, |transcript, ctx| {
                        transcript.attach_cli_subagent(
                            initial_requested_command_action_id.as_ref(),
                            view.clone(),
                            ctx,
                        );
                    });
                    self.cli_subagent_views.insert(block_id.clone(), view);
                }
            }
            CLISubagentEvent::FinishedSubagent {
                block_id,
                initial_requested_command_action_id,
                ..
            } => {
                self.detach_cli_subagent_view(
                    block_id,
                    initial_requested_command_action_id.as_ref(),
                    ctx,
                );
                // `SpawnedSubagent` locked the input to AI while the agent owned
                // this terminal-use block; now that it's finished, restore the
                // setting-derived state so the next prompt can resume
                // autodetection. Mirrors the pin's `reset_after_agent_control`
                // call at the same site (`02b53fcd8`).
                self.reset_after_agent_control(ctx);
            }
            CLISubagentEvent::UpdatedControl { .. }
            | CLISubagentEvent::UpdatedInstruction { .. }
            | CLISubagentEvent::UpdatedLastSnapshot { .. }
            | CLISubagentEvent::ToggledHideResponses => {}
            CLISubagentEvent::ControlHandedBackAfterTransfer => {
                let executor = self.ai_action_model.as_ref(ctx).shell_command_executor(ctx);
                executor.update(ctx, |executor, _| {
                    executor.notify_control_handed_back();
                });
            }
        }
        self.update_process_input_focus(ctx);
        ctx.notify();
    }

    fn handle_terminal_use_interrupt(&mut self, ctx: &mut ViewContext<Self>) -> bool {
        // A manually tagged-in agent takes ctrl-c as "give input back to the
        // command" ahead of any other interrupt handling, discarding whatever
        // unsent prompt the composer holds. Matches the pin's priority order
        // in `handle_terminal_use_interrupt` (`02b53fcd8`) -- without this,
        // ctrl-c would fall through to the TUI's own exit-confirmation arm
        // instead of detaching, which is also why `RUNNING_COMMAND_DETACH_HINT`
        // advertises ctrl-c specifically.
        if self.try_detach_agent_from_running_command(ctx) {
            return true;
        }
        let control_state = self
            .cli_subagent_controller
            .as_ref(ctx)
            .active_target()
            .map(|target| target.control_state);
        let Some(action) = terminal_use_interrupt_action(
            control_state.as_ref(),
            self.input_target().pty_owns_input(),
        ) else {
            return false;
        };
        match action {
            TerminalUseInterruptAction::TakeControl => {
                self.cli_subagent_controller.update(ctx, |controller, ctx| {
                    controller.switch_control_to_user(
                        // A live interrupt keeps the conversation alive so it resumes once the
                        // interrupted command completes.
                        UserTakeOverReason::Stop {
                            should_auto_resume: true,
                        },
                        ctx,
                    );
                });
                self.update_process_input_focus(ctx);
                true
            }
            TerminalUseInterruptAction::InterruptCommand => {
                ctx.emit(TuiTerminalSessionEvent::InterruptPty);
                true
            }
        }
    }

    fn hand_back_terminal_use_control(&mut self, ctx: &mut ViewContext<Self>) {
        if self.active_user_controlled_target(ctx).is_none() {
            return;
        }
        self.cli_subagent_controller.update(ctx, |controller, ctx| {
            controller.handoff_active_command_control_to_agent(ctx);
        });
        self.update_process_input_focus(ctx);
    }

    /// Whether the active block is a user-controlled running command the
    /// agent is not yet tagged into and could be manually invited into.
    fn can_attach_agent_to_running_command(&self) -> bool {
        self.terminal_model
            .lock()
            .block_list()
            .active_block()
            .is_eligible_to_tag_in_agent()
    }

    /// Tags the agent into the active user-controlled running command,
    /// switching the composer to a manual prompt for it. Returns `false` when
    /// the active block is not eligible (already tagged in, not long-running,
    /// or not bootstrapped).
    fn try_attach_agent_to_running_command(&mut self, ctx: &mut ViewContext<Self>) -> bool {
        let did_attach = {
            let mut terminal_model = self.terminal_model.lock();
            let active_block = terminal_model.block_list_mut().active_block_mut();
            if !active_block.is_eligible_to_tag_in_agent() {
                false
            } else {
                active_block.set_is_agent_tagged_in(true);
                true
            }
        };
        if !did_attach {
            return false;
        }
        self.input_view.update(ctx, |input, ctx| input.clear(ctx));
        self.lock_for_agent_control(ctx);
        self.update_process_input_focus(ctx);
        ctx.notify();
        true
    }

    /// Locks the composer to Agent mode and records that this composer (not
    /// the user's own Ctrl+Shift+I toggle) installed the lock -- see
    /// [`Self::agent_terminal_control_lock`]'s doc comment for the full
    /// rationale. Both call sites that give the agent control of a running
    /// command (manual attach above, and `CLISubagentEvent::SpawnedSubagent`)
    /// go through this so [`Self::reset_after_agent_control`] can restore
    /// autodetection once that control ends. Local stand-in for the pin's
    /// `TuiInputView::lock_for_agent_control` (`02b53fcd8`), which threads a
    /// dedicated `InputTypeAutoDetectionSource::AgentTerminalControl` through
    /// `BlocklistAIInputModel` instead of this view-local bool -- see this
    /// field's doc comment for why that is out of scope (#399/#254 item d).
    fn lock_for_agent_control(&mut self, ctx: &mut ViewContext<Self>) {
        self.input_view
            .update(ctx, |input, ctx| input.exit_shell_mode(ctx));
        self.agent_terminal_control_lock = true;
    }

    /// Detaches the agent from the active running command, discarding any
    /// unsent prompt and returning input to the running command. Returns
    /// `false` when no active block has a manually attached agent.
    fn try_detach_agent_from_running_command(&mut self, ctx: &mut ViewContext<Self>) -> bool {
        let did_detach = {
            let mut terminal_model = self.terminal_model.lock();
            let active_block = terminal_model.block_list_mut().active_block_mut();
            if !active_block.is_agent_tagged_in() {
                false
            } else {
                active_block.set_is_agent_tagged_in(false);
                true
            }
        };
        if !did_detach {
            return false;
        }
        // `input.clear()` already resets to the setting-derived agent mode
        // (`TuiInputView::clear` -> `reset_to_default_agent_mode`), so this
        // just clears the bookkeeping that would otherwise make a later,
        // unrelated block completion re-run that reset redundantly.
        self.agent_terminal_control_lock = false;
        self.input_view.update(ctx, |input, ctx| input.clear(ctx));
        self.update_process_input_focus(ctx);
        ctx.notify();
        true
    }

    /// Restores the setting-derived agent mode once this composer's own
    /// agent-terminal-control lock has ended, so the next prompt can resume
    /// autodetection instead of staying hard-locked to AI forever. A no-op
    /// when this composer never held that lock (e.g. an unrelated block
    /// completed, or the user's own Ctrl+Shift+I toggle is in effect) --
    /// [`Self::agent_terminal_control_lock`]'s doc comment has the full
    /// rationale. Ported from the pin's `TuiInputView::reset_after_agent_control`
    /// (`02b53fcd8`), adapted to this fork's local lock-provenance flag instead
    /// of a `last_ai_autodetection_source` comparison.
    fn reset_after_agent_control(&mut self, ctx: &mut ViewContext<Self>) {
        if !self.agent_terminal_control_lock {
            return;
        }
        self.agent_terminal_control_lock = false;
        self.input_view
            .update(ctx, |input, ctx| input.reset_to_default_agent_mode(ctx));
    }

    /// Builds the `/status` menu's content from live session data. Drops the
    /// oracle's `org`/`email` account fields -- this fork is BYOP with no
    /// cloud account or sign-in, so there is nothing truthful to put there.
    fn compute_status_info(&self, ctx: &AppContext) -> status_menu::TuiStatusInfo {
        let cwd = self
            .active_session
            .as_ref(ctx)
            .current_working_directory()
            .map(|cwd| abbreviate_home_prefix(cwd))
            .or_else(|| {
                std::env::current_dir()
                    .ok()
                    .map(|cwd| abbreviate_home_prefix(&cwd.display().to_string()))
            });
        let session_name = self
            .conversation_selection
            .as_ref(ctx)
            .selected_conversation(ctx)
            .and_then(|conversation| conversation.title())
            .unwrap_or_else(|| STATUS_UNTITLED_SESSION.to_owned());
        let conversation_id = self
            .conversation_selection
            .as_ref(ctx)
            .selected_conversation_id(ctx);
        let version = ChannelState::app_version()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| STATUS_DEV_BUILD.to_owned());
        status_menu::TuiStatusInfo {
            version,
            session: session_name,
            conversation_id: format_status_conversation_id(conversation_id),
            working_directory: cwd.unwrap_or_else(|| STATUS_UNAVAILABLE.to_owned()),
        }
    }

    fn active_agent_controlled_target(&self, ctx: &AppContext) -> Option<CLISubagentTarget> {
        self.cli_subagent_controller
            .as_ref(ctx)
            .active_target()
            .filter(|target| target.control_state.is_agent_in_control())
    }

    fn active_user_controlled_target(&self, ctx: &AppContext) -> Option<CLISubagentTarget> {
        self.cli_subagent_controller
            .as_ref(ctx)
            .active_target()
            .filter(|target| target.control_state.is_user_in_control())
    }

    fn active_agent_blocked_target(&self, ctx: &AppContext) -> Option<CLISubagentTarget> {
        self.cli_subagent_controller
            .as_ref(ctx)
            .active_target()
            .filter(|target| target.control_state.is_agent_blocked())
    }

    /// The specific action `target`'s `TuiCLISubagentView` is displaying as
    /// blocked, if that view is still registered. Looking this up (rather
    /// than guessing at the conversation's first pending action) keeps the
    /// keyboard shortcut targeting the exact action the card shows, matching
    /// the mouse-click path in `TuiCLISubagentView`.
    fn blocked_action_for_target(
        &self,
        target: &CLISubagentTarget,
        app: &AppContext,
    ) -> Option<AIAgentAction> {
        self.cli_subagent_views
            .get(&target.block_id)?
            .read(app, |view, app| view.blocked_action(app))
    }

    /// Approves the agent's pending action on the long-running command it's
    /// driving -- the keyboard equivalent of clicking "[Allow]" in
    /// `TuiCLISubagentView` (`TuiCLISubagentViewAction::Allow`'s handler).
    fn allow_blocked_lrc_action(&mut self, ctx: &mut ViewContext<Self>) {
        let Some(target) = self.active_agent_blocked_target(ctx) else {
            return;
        };
        let Some(blocked_action) = self.blocked_action_for_target(&target, ctx) else {
            return;
        };
        let conversation_id = target.conversation_id;
        self.ai_action_model.update(ctx, |action_model, ctx| {
            crate::tui_cli_subagent_view::execute_blocked_action(
                action_model,
                conversation_id,
                &blocked_action,
                ctx,
            );
        });
    }

    /// Rejects the agent's pending action on the long-running command it's
    /// driving, without taking over the command -- the keyboard equivalent of
    /// clicking "[Reject]" in `TuiCLISubagentView`
    /// (`TuiCLISubagentViewAction::Reject`'s handler). Mirrors the GUI's
    /// `RejectBlockedAction { should_user_take_over: false }`: the specific
    /// displayed action is cancelled and control stays with the agent.
    fn reject_blocked_lrc_action(&mut self, ctx: &mut ViewContext<Self>) {
        let Some(target) = self.active_agent_blocked_target(ctx) else {
            return;
        };
        let Some(blocked_action) = self.blocked_action_for_target(&target, ctx) else {
            return;
        };
        let conversation_id = target.conversation_id;
        self.ai_action_model.update(ctx, |action_model, ctx| {
            crate::tui_cli_subagent_view::cancel_blocked_action(
                action_model,
                conversation_id,
                &blocked_action,
                ctx,
            );
        });
    }

    fn send_terminal_use_prompt(&mut self, input: &str, ctx: &mut ViewContext<Self>) -> bool {
        let Some(prompt) = raw_prompt_if_not_blank(input) else {
            return false;
        };
        let Some(target) = self.active_agent_controlled_target(ctx) else {
            return false;
        };
        let prompt = prompt.to_owned();
        let block_id = target.block_id;
        let conversation_id = target.conversation_id;
        let previous_instruction = self.cli_subagent_controller.update(ctx, |controller, ctx| {
            controller.set_latest_instruction(block_id.clone(), prompt.clone(), ctx)
        });
        self.input_view.update(ctx, |input, ctx| input.clear(ctx));
        ctx.notify();

        // Zap's send_user_query_in_conversation returns () and always dispatches, so there is
        // no failure path to undo (Warp restored the instruction / input text on failure here).
        let _ = previous_instruction;
        self.ai_controller.update(ctx, |controller, ctx| {
            controller.send_user_query_in_conversation(prompt.clone(), conversation_id, None, ctx)
        });
        true
    }

    /// Builds the transcript-capable terminal surface for a manager-backed session.
    pub(crate) fn new(
        surface_init: TerminalSurfaceInit,
        exit_summary: TuiExitSummaryHandle,
        keyboard_enhancement_supported: bool,
        initial_settings_file_error: Option<SettingsFileError>,
        default_autoexecute_mode: AIConversationAutoexecuteMode,
        ctx: &mut ViewContext<Self>,
    ) -> Self {
        let TerminalSurfaceInit {
            model,
            sessions,
            model_events,
            wakeups_rx,
            size_info,
            ..
        } = surface_init;
        let (terminal_resize_tx, terminal_resize_rx) = async_channel::unbounded();
        model
            .lock()
            .block_list_mut()
            // Zap models transcript scoping as AgentViewState (warp renamed it
            // TranscriptScope in a later refactor). warp's Unfiltered ("show all
            // blocks") maps to Zap's Inactive (no conversation filtering).
            .set_agent_view_state(AgentViewState::Inactive);

        let terminal_surface_id: EntityId = ctx.view_id();
        // Kept alive past the `BlocklistAIContextModel::new` move below so the
        // shell-completion warm-up can subscribe to session bootstraps.
        let sessions_for_completions = sessions.clone();
        let active_session =
            ctx.add_model(|ctx| ActiveSession::new(sessions.clone(), model_events.clone(), ctx));
        let model_for_conversation_selection = model.clone();
        let conversation_selection = ctx.add_model(|ctx| {
            Box::new(TuiConversationSelection::new(
                terminal_surface_id,
                model_for_conversation_selection,
                default_autoexecute_mode,
                ctx,
            )) as Box<dyn ConversationSelection>
        });
        let context_model = ctx.add_model(|ctx| {
            BlocklistAIContextModel::new(
                sessions,
                &model_events,
                model.clone(),
                terminal_surface_id,
                // The TUI has no agent-view controller.
                None,
                // #343: previously discarded, so `context_model`'s new-conversation creation
                // always failed on the TUI (it only ever tried `agent_view_controller`, which is
                // always `None` here). This is the same handle used for every other
                // conversation-selection operation on this surface (`view.conversation_selection`
                // below).
                conversation_selection.clone(),
                ctx,
            )
        });
        let ai_input_model = ctx.add_model(|ctx| {
            BlocklistAIInputModel::new_tui(
                model.clone(),
                context_model.clone(),
                Rc::new(TuiInputModePolicy),
                terminal_surface_id,
                ctx,
            )
        });
        // Zap's BlocklistAIActionModel does not use a GetRelevantFilesController
        // (that controller is cloud/embedding-backed — "search codebase" via the
        // server embedding index — which BYOP Zap does not have). Dropped here to
        // match Zap's action-model constructor.
        let action_model = ctx.add_model(|ctx| {
            BlocklistAIActionModel::new(
                model.clone(),
                active_session.clone(),
                &model_events,
                terminal_surface_id,
                ctx,
            )
        });
        let ai_controller = ctx.add_model(|ctx| {
            BlocklistAIController::new(
                ai_input_model.clone(),
                context_model.clone(),
                action_model.clone(),
                active_session.clone(),
                // The TUI has no agent-view controller.
                None,
                model.clone(),
                terminal_surface_id,
                ctx,
            )
        });
        let cli_subagent_controller = ctx.add_model(|ctx| {
            CLISubagentController::new(
                &ai_controller,
                &action_model,
                None,
                model.clone(),
                &model_events,
                terminal_surface_id,
                ctx,
            )
        });
        ctx.subscribe_to_model(&cli_subagent_controller, |view, _, event, ctx| {
            view.handle_cli_subagent_event(event, ctx);
        });
        let transcript = ctx.add_typed_action_tui_view(|ctx| {
            TuiTranscriptView::new(
                terminal_surface_id,
                model.clone(),
                action_model.clone(),
                &model_events,
                ctx,
            )
        });
        // Input visibility and focus derive from the front-of-queue blocker;
        // re-derive on every action-queue transition (queued, blocked,
        // finished). No suppression flag is stored.
        ctx.subscribe_to_model(&action_model, |view, _, _, ctx| {
            view.sync_blocker_focus(ctx);
        });
        let input_editor_model =
            ctx.add_model(|ctx| CodeEditorModel::new_tui(INITIAL_INPUT_WIDTH, ctx));
        let suggestions_mode = ctx.add_model(|_| TuiInputSuggestionsModeModel::new());
        let slash_commands_source = ctx.add_model(|ctx| {
            TuiSlashCommandDataSource::new_tui(
                TuiSlashCommandDataSourceArgs {
                    active_session: active_session.clone(),
                    cli_subagent_controller: cli_subagent_controller.clone(),
                    terminal_view_id: terminal_surface_id,
                },
                ctx,
            )
        });
        let zero_state_source = TuiZeroStateDataSource::new(&slash_commands_source);
        let slash_commands_mixer = ctx.add_model(|ctx| {
            build_slash_command_mixer(slash_commands_source.clone(), zero_state_source, ctx)
        });
        let slash_commands = ctx.add_model(|ctx| {
            TuiSlashCommandModel::new(
                input_editor_model.clone(),
                suggestions_mode.clone(),
                slash_commands_source.clone(),
                slash_commands_mixer,
                conversation_selection.clone(),
                ctx,
            )
        });
        ctx.subscribe_to_model(&slash_commands, |_, _, _, ctx| ctx.notify());
        let window_id = ctx.window_id();
        let conversation_menu = ctx.add_model(|ctx| {
            TuiConversationMenuModel::new(
                input_editor_model.clone(),
                suggestions_mode.clone(),
                conversation_selection.clone(),
                window_id,
                ctx,
            )
        });
        ctx.subscribe_to_model(&conversation_menu, |view, _, event, ctx| match event {
            TuiConversationMenuEvent::Updated => ctx.notify(),
            TuiConversationMenuEvent::CloudMetadataUnavailable => {
                view.show_transient_hint(
                    "Could not load cloud conversations. Showing local conversations only."
                        .to_owned(),
                    ctx,
                );
            }
        });
        let model_menu = ctx.add_model(|ctx| {
            TuiModelMenuModel::new(
                input_editor_model.clone(),
                suggestions_mode.clone(),
                terminal_surface_id,
                ctx,
            )
        });
        ctx.subscribe_to_model(&model_menu, |_, _, _: &TuiModelMenuEvent, ctx| {
            ctx.notify();
        });
        let skills_menu = ctx.add_model(|ctx| {
            TuiSkillMenuModel::new(
                input_editor_model.clone(),
                suggestions_mode.clone(),
                active_session.clone(),
                slash_commands_source.clone(),
                terminal_surface_id,
                ctx,
            )
        });
        ctx.subscribe_to_model(&skills_menu, |_, _, _: &TuiSkillMenuEvent, ctx| {
            ctx.notify();
        });
        let mcp_menu = ctx.add_model(|ctx| {
            TuiMcpMenuModel::new(input_editor_model.clone(), suggestions_mode.clone(), ctx)
        });
        ctx.subscribe_to_model(&mcp_menu, |_, _, event, ctx| {
            let TuiMcpMenuEvent::Updated = event;
            ctx.notify();
        });
        let mcp_install_flow = ctx.add_model(|_| {
            TuiMcpInstallFlowModel::new(input_editor_model.clone(), suggestions_mode.clone())
        });
        ctx.subscribe_to_model(&mcp_install_flow, |view, _, event, ctx| match event {
            TuiMcpInstallFlowEvent::Updated => ctx.notify(),
            // Cancelling or finishing the flow returns to the catalog it was
            // started from, so the user does not lose their place.
            TuiMcpInstallFlowEvent::Dismissed => {
                view.mcp_menu.update(ctx, |menu, ctx| menu.open(ctx));
                ctx.notify();
            }
        });
        let prompt_and_command_history_menu = ctx.add_model(|ctx| {
            TuiPromptAndCommandHistoryMenuModel::new(
                input_editor_model.clone(),
                ai_input_model.clone(),
                suggestions_mode.clone(),
                active_session.clone(),
                terminal_surface_id,
                ctx,
            )
        });
        ctx.subscribe_to_model(&prompt_and_command_history_menu, |_, _, event, ctx| {
            let TuiPromptAndCommandHistoryMenuEvent::Updated = event;
            ctx.notify();
        });
        let completions_menu =
            ctx.add_model(|_| TuiCompletionsMenuModel::new(suggestions_mode.clone()));
        ctx.subscribe_to_model(
            &completions_menu,
            |_, _, _: &TuiCompletionsMenuEvent, ctx| {
                ctx.notify();
            },
        );
        let profile_menu = ctx.add_model(|ctx| {
            TuiProfileMenuModel::new(
                input_editor_model.clone(),
                suggestions_mode.clone(),
                terminal_surface_id,
                ctx,
            )
        });
        ctx.subscribe_to_model(&profile_menu, |_, _, _: &TuiProfileMenuEvent, ctx| {
            ctx.notify();
        });
        let prompts_menu = ctx.add_model(|ctx| {
            TuiPromptsMenuModel::new(input_editor_model.clone(), suggestions_mode.clone(), ctx)
        });
        ctx.subscribe_to_model(&prompts_menu, |_, _, _: &TuiPromptsMenuEvent, ctx| {
            ctx.notify();
        });
        let exchange_menu = ctx.add_model(|ctx| {
            TuiExchangeMenuModel::new(input_editor_model.clone(), suggestions_mode.clone(), ctx)
        });
        ctx.subscribe_to_model(&exchange_menu, |_, _, _: &TuiExchangeMenuEvent, ctx| {
            ctx.notify();
        });
        let api_keys_menu = ctx.add_model(|ctx| {
            TuiApiKeysMenuModel::new(input_editor_model.clone(), suggestions_mode.clone(), ctx)
        });
        ctx.subscribe_to_model(&api_keys_menu, |_, _, _: &TuiApiKeysMenuEvent, ctx| {
            ctx.notify();
        });
        // The footer's conversations callout depends on whether the input is
        // empty, so content changes must invalidate this parent view as well as
        // the input child. Typing after ctrl-c also disarms the pending exit
        // confirmation (and any child-kill window); the ctrl-c buffer clear
        // leaves the buffer empty, so the window it arms survives its own clear.
        let editor_for_footer = input_editor_model.clone();
        ctx.subscribe_to_model(&input_editor_model, move |view, _, event, ctx| {
            let CodeEditorModelEvent::ContentChanged { origin } = event else {
                return;
            };
            let is_empty = editor_for_footer
                .as_ref(ctx)
                .content()
                .as_ref(ctx)
                .is_empty();
            if !is_empty {
                view.exit_confirmation.disarm();
                view.child_kill_armed_conversation = None;
            }
            view.handle_input_content_changed(origin.from_user(), ctx);
            ctx.notify();
        });

        let editor_for_selection = input_editor_model.clone();
        let transcript_for_selection = transcript.clone();
        ctx.subscribe_to_model(&input_editor_model, move |view, _, event, ctx| {
            if !matches!(event, CodeEditorModelEvent::SelectionChanged) {
                return;
            }
            view.handle_completion_editor_changed(ctx);

            let has_selection = !editor_for_selection
                .as_ref(ctx)
                .buffer_selection_model()
                .as_ref(ctx)
                .first_selection_is_single_cursor();
            if has_selection {
                view.read_only_menu_selection.clear();
                transcript_for_selection.update(ctx, |transcript, ctx| {
                    transcript.clear_selection(ctx);
                });
            }
        });

        let input_mode_for_input_view = ai_input_model.clone();
        let inline_menus = vec![
            TuiInlineMenu::new(slash_commands.clone()),
            TuiInlineMenu::new(conversation_menu.clone()),
            TuiInlineMenu::new(model_menu.clone()),
            TuiInlineMenu::new(skills_menu.clone()),
            TuiInlineMenu::new(mcp_menu.clone()),
            TuiInlineMenu::new(mcp_install_flow.clone()),
            TuiInlineMenu::new(prompt_and_command_history_menu.clone()),
            TuiInlineMenu::new(completions_menu.clone()),
            TuiInlineMenu::new(profile_menu.clone()),
            TuiInlineMenu::new(prompts_menu.clone()),
            TuiInlineMenu::new(exchange_menu.clone()),
            TuiInlineMenu::new(api_keys_menu.clone()),
        ];
        let inline_menus_for_input = inline_menus.clone();
        let suggestions_mode_for_input = suggestions_mode.clone();
        let transcript_for_input = transcript.clone();
        let terminal_model_for_input = model.clone();
        let input_editor_for_input = input_editor_model.clone();
        let input_view = ctx.add_typed_action_tui_view(move |ctx| {
            TuiInputView::new(
                input_editor_for_input,
                input_mode_for_input_view,
                suggestions_mode_for_input,
                inline_menus_for_input,
                transcript_for_input,
                // No orchestration tab bar in Zap: focus never moves up to tabs.
                |_| false,
                ctx,
            )
            .with_inline_menu_actions_allowed(move |_| {
                let terminal_model = terminal_model_for_input.lock();
                tui_input_target(&terminal_model).agent_editor_owns_input()
            })
            .with_keyboard_enhancement_supported(keyboard_enhancement_supported)
        });
        let attachment_model = ctx.add_model(|ctx| {
            TuiAttachmentModel::new(
                context_model.clone(),
                ai_input_model.clone(),
                input_editor_model,
                active_session.clone(),
                terminal_surface_id,
                ctx,
            )
        });
        let attachment_bar =
            ctx.add_typed_action_tui_view(|ctx| TuiAttachmentBar::new(attachment_model, ctx));
        ctx.subscribe_to_view(&attachment_bar, |view, _, event, ctx| {
            view.handle_attachment_bar_event(event, ctx);
        });

        ctx.subscribe_to_view(&transcript, |view, _, event, ctx| match event {
            TuiTranscriptViewEvent::SelectionStarted => {
                view.read_only_menu_selection.clear();
                view.input_view
                    .update(ctx, |input, ctx| input.clear_selection(ctx));
            }
            TuiTranscriptViewEvent::SelectionEnded(text) => match copy_to_clipboard(text) {
                Ok(()) => view.show_copy_hint(ctx),
                Err(error) => {
                    log::warn!("Failed to copy TUI selection: {error}");
                    view.show_transient_hint(COPY_FAILED_HINT.to_owned(), ctx);
                }
            },
            TuiTranscriptViewEvent::BlockingStateChanged => {
                view.sync_blocker_focus(ctx);
            }
            TuiTranscriptViewEvent::PermissionReplacementGuidanceSubmitted {
                conversation_id,
                text,
            } => {
                view.ai_controller.update(ctx, |controller, ctx| {
                    controller.send_user_query_in_conversation(
                        text.clone(),
                        *conversation_id,
                        None,
                        ctx,
                    );
                });
            }
        });

        ctx.subscribe_to_view(&input_view, |view, _, event, ctx| match event {
            TuiInputViewEvent::Submitted(text) => view.handle_submitted(text.clone(), ctx),
            TuiInputViewEvent::Pasted(text) => view.handle_pasted(text.clone(), ctx),
            TuiInputViewEvent::BackspaceAtEmptyInput => {
                view.attachment_bar
                    .update(ctx, |bar, ctx| bar.remove_selected(ctx));
            }
            TuiInputViewEvent::AcceptedSlashCommand(action) => {
                view.handle_accepted_slash_command(action, ctx);
            }
            TuiInputViewEvent::AcceptedConversation(entry_id) => {
                view.handle_accepted_conversation(*entry_id, ctx);
            }
            TuiInputViewEvent::AcceptedModel(id) => {
                view.handle_accepted_model(id, ctx);
            }
            TuiInputViewEvent::AcceptedMcp(action) => {
                view.handle_accepted_mcp_action(*action, ctx);
            }
            TuiInputViewEvent::AcceptedMcpInstall(action) => {
                view.handle_accepted_mcp_install_action(action.clone(), ctx);
            }
            TuiInputViewEvent::AcceptedPromptAndCommandHistory(text, kind) => {
                view.handle_accepted_prompt_and_command_history(text.clone(), kind.clone(), ctx);
            }
            TuiInputViewEvent::AcceptedCompletion(completion) => {
                view.handle_accepted_completion(completion.clone(), ctx);
            }
            TuiInputViewEvent::AcceptedProfile(profile_id) => {
                view.handle_accepted_profile(*profile_id, ctx);
            }
            TuiInputViewEvent::AcceptedPrompt(text) => {
                view.handle_accepted_prompt(text.clone(), ctx);
            }
            TuiInputViewEvent::AcceptedExchange(exchange_id, action) => {
                view.handle_accepted_exchange(*exchange_id, *action, ctx);
            }
            // No orchestration tab bar in Zap: nothing above the input to focus.
            TuiInputViewEvent::ClipboardCopySucceeded => view.show_copy_hint(ctx),
            TuiInputViewEvent::ClipboardCopyFailed => {
                view.show_transient_hint(COPY_FAILED_HINT.to_owned(), ctx);
            }
            TuiInputViewEvent::MoveFocusUp => {}
            // The vim mode changed - re-render so the footer indicator (NOR/VIS/REP)
            // updates. The indicator is rendered by this view's render_footer, not
            // by TuiInputView itself, so a notify from TuiInputView alone is not
            // sufficient to update the parent's element tree.
            TuiInputViewEvent::VimModeChanged => ctx.notify(),
        });
        ctx.subscribe_to_model(&action_model, |view, action_model, event, ctx| {
            let BlocklistAIActionEvent::FinishedAction { action_id, .. } = event else {
                return;
            };
            let finished_asking_question = action_model
                .as_ref(ctx)
                .get_action_result(action_id)
                .is_some_and(|result| {
                    matches!(&result.result, AIAgentActionResultType::AskUserQuestion(_))
                });
            if finished_asking_question {
                view.refocus_input_after_question(ctx);
            }
        });
        // The input box border color and the footer's shell-mode hint depend
        // on the input mode.
        ctx.subscribe_to_model(&ai_input_model, |view, _, _, ctx| {
            view.handle_completion_editor_changed(ctx);
            ctx.notify();
        });
        ctx.subscribe_to_model(&suggestions_mode, |view, _, event, ctx| {
            view.read_only_menu_selection.clear();
            view.open_todo_menu_list_key = match event.mode.read_only_menu() {
                Some(TuiReadOnlyMenuKind::Todos) => view.active_todo_menu_list_key(ctx),
                Some(TuiReadOnlyMenuKind::Shortcuts | TuiReadOnlyMenuKind::Status) | None => None,
            };
            let scroll_top = event
                .mode
                .read_only_menu()
                .map(|kind| view.read_only_menu_initial_scroll_top(kind, ctx))
                .unwrap_or_default();
            view.read_only_menu_viewport
                .scroll_to_rows_from_top(scroll_top);
            ctx.notify();
        });
        // The warping indicator between the transcript and the input box
        // tracks the selected conversation: re-render when its status changes
        // or an exchange starts (the elapsed counter's anchor) on this
        // surface, and when the selected conversation changes.
        ctx.subscribe_to_model(
            &BlocklistAIHistoryModel::handle(ctx),
            |view, _, event, ctx| view.handle_history_event(event, ctx),
        );
        ctx.subscribe_to_model(&conversation_selection, |view, _, _, ctx| {
            view.refresh_exit_summary(ctx);
            view.sync_open_todo_menu_list(ctx);
            ctx.notify();
        });

        // Trigger the changelog fetch once at startup so `TuiZeroStateView`
        // has data to display.  The re-render subscription lives in the view.
        ChangelogModel::handle(ctx).update(ctx, |changelog, ctx| {
            changelog.check_for_changelog(ChangelogRequestType::WindowLaunch, ctx);
        });

        // Bridge shared shell-tool executor events into terminal-manager PTY intents.
        let shell_command_executor = action_model.as_ref(ctx).shell_command_executor(ctx);
        let model_for_shell_events = model.clone();
        ctx.subscribe_to_model(&shell_command_executor, move |view, _, event, ctx| {
            view.handle_shell_command_executor_event(event, &model_for_shell_events, ctx);
        });

        // These events update block metadata or grids the transcript reads.
        // PTY output redraws are driven by `wakeups_rx` below.
        ctx.subscribe_to_model(&model_events, |view, _, event, ctx| match event {
            ModelEvent::BlockCompleted(completed) => {
                view.handle_block_completed(&completed.block_id, ctx);
            }
            ModelEvent::AfterBlockStarted { .. } => {
                view.update_process_input_focus(ctx);
                ctx.notify();
            }
            ModelEvent::VisibleBootstrapBlock | ModelEvent::BootstrapPrecmdDone => {
                view.update_process_input_focus(ctx);
                ctx.notify();
            }
            ModelEvent::Typeahead => view.handle_typeahead_event(ctx),
            // Defensive retry: a session whose bootstrap warm-up was skipped
            // (or raced the subscription below) still gets its PATH executables
            // loaded after the first command finishes.
            ModelEvent::AfterBlockCompleted(_) => {
                view.ensure_external_commands_are_warming(ctx);
            }
            ModelEvent::BlockMetadataReceived(_)
            | ModelEvent::BackgroundBlockStarted
            | ModelEvent::TerminalClear
            | ModelEvent::PromptUpdated
            | ModelEvent::Handler(_)
            | ModelEvent::FinishUpdate(_) => ctx.notify(),
            _ => {}
        });
        // The footer shows the active model and working directory: re-render
        // when the active model or its display name changes, or when the
        // session's working directory changes. (The context-usage entry
        // re-renders via the conversation-model events above.) Also re-render
        // when the configured statusline items/order change (`/statusline`
        // save or settings-file hot reload).
        ctx.subscribe_to_model(&AISettings::handle(ctx), |view, _, event, ctx| {
            if matches!(event, AISettingsChangedEvent::AIAutoDetectionEnabled { .. }) {
                view.schedule_input_detection(ctx);
            }
            if matches!(event, AISettingsChangedEvent::TuiStatusline { .. }) {
                // The GitHub subscription is only held while its statusline
                // item is enabled, so it has to follow the configuration.
                view.update_github_status_subscription(ctx);
                ctx.notify();
            }
        });
        // A failed settings hot-reload gets the same transient footer slot as
        // the startup failure surfaced at the end of this constructor; the
        // detailed diagnostics stay in the log (`settings::init`).
        ctx.subscribe_to_model(&WarpConfig::handle(ctx), |view, _, event, ctx| {
            if let WarpConfigUpdateEvent::SettingsErrors(error) = event {
                view.show_settings_file_error(error, ctx);
            }
        });
        ctx.subscribe_to_model(&LLMPreferences::handle(ctx), |_, _, event, ctx| {
            if matches!(
                event,
                LLMPreferencesEvent::UpdatedAvailableLLMs
                    | LLMPreferencesEvent::UpdatedActiveAgentModeLLM
            ) {
                ctx.notify();
            }
        });
        // Warm the completion sources as soon as the active session's shell has
        // bootstrapped, so the first Tab press does not pay for a cold engine.
        ctx.subscribe_to_model(
            &sessions_for_completions,
            |view, sessions, event, ctx| match event {
                SessionsEvent::SessionBootstrapped(bootstrap_event)
                    if view.active_session.as_ref(ctx).session_id(ctx)
                        == Some(bootstrap_event.session_id) =>
                {
                    let Some(session) = sessions.as_ref(ctx).get(bootstrap_event.session_id) else {
                        report_error!(
                            "Could not find active TUI session after its bootstrap event",
                            extra: { "session_id" => ?bootstrap_event.session_id }
                        );
                        return;
                    };
                    view.abort_shell_completion(ctx);
                    view.warm_shell_completion_sources(session, ctx);
                }
                SessionsEvent::SessionBootstrapped(_)
                | SessionsEvent::SessionInitialized { .. }
                | SessionsEvent::EnvironmentVariablesUpdated { .. } => {}
            },
        );
        ctx.subscribe_to_model(&active_session, |view, _, event, ctx| match event {
            ActiveSessionEvent::UpdatedPwd => {
                view.abort_shell_completion(ctx);
                // Run repo detection so project rules and skills follow the
                // session's working directory (the GUI's equivalent lives in
                // `TerminalView::apply_block_metadata_update`). The first
                // post-bootstrap precmd metadata transitions the cwd from
                // `None` to `Some`, so this also covers the launch directory.
                let Some(cwd) = view
                    .active_session
                    .as_ref(ctx)
                    .current_working_directory()
                    .cloned()
                else {
                    view.slash_commands_source.update(ctx, |source, ctx| {
                        source.set_active_repo_root(None, ctx);
                    });
                    view.update_git_status_subscription(None, ctx);
                    ctx.notify();
                    return;
                };
                let detection = detect_possible_git_repo(
                    RepoDetectionSessionType::Local,
                    &cwd,
                    RepoDetectionSource::TerminalNavigation,
                    ctx,
                );
                ctx.spawn(detection, move |view, repo_path, ctx| {
                    if view.active_session.as_ref(ctx).current_working_directory() != Some(&cwd) {
                        return;
                    }
                    view.update_git_status_subscription(repo_path.clone(), ctx);
                    let repo_root = repo_path
                        .as_ref()
                        .and_then(|path| path.to_local_path())
                        .map(ToOwned::to_owned);
                    view.slash_commands_source.update(ctx, |source, ctx| {
                        source.set_active_repo_root(repo_root, ctx);
                    });
                });
                ctx.notify();
            }
            ActiveSessionEvent::Bootstrapped => {}
        });
        // (Zap has no ConversationUsageMetadataUpdated event — the footer shows a local
        // context-% instead of cloud usage totals, so no usage-metadata subscription.)

        // A wakeup is also how a running block becomes visible: its height is 0
        // until the long-running render-delay timer fires and sends a wakeup
        // (see `Block::wakeup_after_delay`). Heights are otherwise only
        // recomputed when PTY bytes arrive, so a silent command (e.g. `sleep`)
        // would stay invisible until it finishes. Mirror the GUI's
        // `handle_terminal_wakeup` by throttling the stream and refreshing
        // live block heights here.
        ctx.spawn_stream_local(
            throttle(WAKEUP_THROTTLE_PERIOD, wakeups_rx),
            |view, _, ctx| {
                view.handle_terminal_wakeup(ctx);
            },
            |_, _| {},
        );
        ctx.spawn_stream_local(terminal_resize_rx, Self::handle_terminal_resize, |_, _| {});
        let zero_state_interaction = ZeroStateInteractionHandle::default();
        let zero_state_view = {
            let interaction = zero_state_interaction.clone();
            let zero_state_active_session = active_session.clone();
            ctx.add_tui_view(move |ctx| {
                TuiZeroStateView::new(zero_state_active_session, interaction, ctx)
            })
        };
        let orchestration_tab_bar = ctx.add_typed_action_tui_view(|_| TuiTabBarView::empty());
        // Surface a zero-state ASCII-art load/reload failure (bad or missing
        // `TuiZeroStateObject::AsciiFile`) as a footer hint rather than
        // silently falling back to the built-in mark. Ported from the pin
        // (`02b53fcd8`) as part of #384.
        let zero_state_animation_config = ZeroStateAnimationConfig::handle(ctx);
        let initial_zero_state_load_failure =
            zero_state_animation_config.as_ref(ctx).load_failure();
        ctx.subscribe_to_model(
            &zero_state_animation_config,
            |view, _, event, ctx| match event {
                ZeroStateAnimationConfigEvent::Updated => {}
                ZeroStateAnimationConfigEvent::LoadFailed(failure) => {
                    view.show_zero_state_ascii_load_failure(*failure, ctx);
                }
            },
        );
        // A lightweight resolver holding only weak references, used to build a
        // fresh `TuiTerminalSessionState` on demand (e.g. for the `?`-opened
        // shortcuts menu) without a persistent subscription -- its `Entity`
        // event type is `()`, so there is nothing to subscribe to anyway.
        let session_state = TuiTerminalSessionStateModel::new(
            &model,
            &cli_subagent_controller,
            &transcript,
            &ai_input_model,
            &suggestions_mode,
        );
        // Read-only menus render top-anchored, not bottom-anchored like the
        // transcript, so the retained viewport starts pinned to row 0.
        let read_only_menu_viewport = TuiViewportedListState::new_at_end();
        read_only_menu_viewport.scroll_to_rows_from_top(0);
        let mut view = Self {
            transcript,
            input_view,
            attachment_bar,
            inline_menus,
            suggestions_mode,
            read_only_menu_selection: TuiSelectionHandle::default(),
            read_only_menu_viewport,
            open_todo_menu_list_key: None,
            session_state,
            conversation_menu,
            model_menu,
            completions_menu,
            profile_menu,
            prompts_menu,
            exchange_menu,
            api_keys_menu,
            queued_follow_up: None,
            completion_request: completions::CompletionRequestState::default(),
            skills_menu,
            mcp_menu,
            mcp_install_flow,
            slash_commands_source,
            conversation_selection,
            ai_action_model: action_model,
            cli_agent_osc_event_publisher: None,
            ai_controller,
            cli_subagent_controller,
            cli_subagent_views: HashMap::new(),
            active_session,
            current_repo_path: None,
            git_repo_status: None,
            github_repo: None,
            terminal_surface_id,
            exit_confirmation: ExitConfirmation::default(),
            hidden_response_summary_exchange_ids: HashSet::new(),
            model_label_hover: MouseStateHandle::default(),
            github_pr_link: TuiLink::default(),
            todo_list_mouse: MouseStateHandle::default(),
            keyboard_enhancement_supported,
            ai_context_model: context_model,
            ai_input_model,
            input_detection: InputDetectionState::default(),
            terminal_model: model,
            size_info,
            terminal_resize_tx,
            transient_hint: TransientHint::default(),
            auto_approve_feedback_conversation_id: None,
            auto_approve_feedback_timer: None,
            footer_auto_approve_mouse: MouseStateHandle::default(),
            warping_auto_approve_mouse: MouseStateHandle::default(),
            conversation_restore_state: ConversationRestoreState::Idle,
            next_restore_request_id: 0,
            exit_summary,
            active_blocker_view_id: None,
            statusline_config_view: None,
            zero_state_interaction,
            zero_state_view,
            pending_history_command_workflow_data: None,
            orchestration_tab_bar,
            orchestration_tabs_focused: false,
            child_kill_armed_conversation: None,
            agent_terminal_control_lock: false,
        };
        if let Some(failure) = initial_zero_state_load_failure {
            view.show_zero_state_ascii_load_failure(failure, ctx);
        }
        // Late-subscriber path: the session may already have bootstrapped
        // before the subscription above was installed.
        if let Some(session) = view.active_session.as_ref(ctx).session(ctx) {
            view.warm_shell_completion_sources(session, ctx);
        }
        if let Some(error) = initial_settings_file_error {
            view.show_settings_file_error(&error, ctx);
        }
        view
    }

    /// Enables CLI-agent lifecycle notifications for the root TUI session.
    ///
    /// A no-op unless the hosting terminal advertises the CLI-agent protocol
    /// (`WARP_CLI_AGENT_PROTOCOL_VERSION` + `WARP_CLIENT_VERSION`), so a TUI
    /// started outside a Phosphor pane never writes OSC 777 into a terminal
    /// that would render it as garbage.
    pub(crate) fn enable_cli_agent_osc_event_publishing(&mut self, ctx: &mut ViewContext<Self>) {
        if self.cli_agent_osc_event_publisher.is_some() || !host_supports_cli_agent_notifications()
        {
            return;
        }
        let terminal_surface_id = self.terminal_surface_id;
        let active_session = self.active_session.clone();
        let conversation_selection = self.conversation_selection.clone();
        let action_model = self.ai_action_model.clone();
        let publisher = ctx.add_model(|ctx| {
            CliAgentOscEventPublisher::new(
                terminal_surface_id,
                active_session,
                conversation_selection,
                &action_model,
                ctx,
            )
        });
        publisher.as_ref(ctx).publish_session_start(ctx);
        self.cli_agent_osc_event_publisher = Some(publisher);
    }

    /// The active front-of-queue blocking interaction, if any.
    fn active_blocking_child(&self, ctx: &AppContext) -> Option<TuiBlockingChild> {
        self.transcript.as_ref(ctx).active_blocking_child(ctx)
    }

    /// Activates this session after the registry has made it authoritative.
    pub(crate) fn activate(&mut self, ctx: &mut ViewContext<Self>) {
        self.focus_current_owner(ctx);
        self.write_exit_summary(ctx);
        ctx.notify();
    }

    /// Whether this view projects the focused session.
    fn is_focused_session(&self, ctx: &AppContext) -> bool {
        TuiSessions::as_ref(ctx)
            .focused_session_id()
            .is_some_and(|id| id.surface_id() == self.terminal_surface_id)
    }

    /// Reconciles focus with the derived blocker: a newly active blocker is
    /// focused (handing off directly between consecutive blockers with no
    /// intermediate editable input), and focus returns to the input when the
    /// last blocker resolves. Nothing here writes to the input model, so its
    /// draft/cursor/selection are untouched.
    fn sync_blocker_focus(&mut self, ctx: &mut ViewContext<Self>) {
        let blocker = self.active_blocking_child(ctx);
        let blocker_view_id = blocker.as_ref().map(TuiBlockingChild::id);
        if blocker_view_id != self.active_blocker_view_id {
            self.active_blocker_view_id = blocker_view_id;
            self.focus_current_owner_if_active(ctx);
        }
        ctx.notify();
    }

    /// Reclaims focus for the composer after a question-type blocker
    /// finishes, but only if no other blocker has already claimed it for
    /// itself (e.g. a second blocker was created and focused in the same
    /// tick, before this ran). `active_blocker_view_id` is the same
    /// "current blocker" bookkeeping [`Self::sync_blocker_focus`] uses to
    /// avoid stomping a newer focus target.
    fn refocus_input_after_question(&mut self, ctx: &mut ViewContext<Self>) {
        if self.active_blocker_view_id.is_none() {
            ctx.focus(&self.input_view);
        }
    }

    /// Restores an Oz conversation into the TUI's sole conversation surface.
    pub(crate) fn restore_conversation(
        &mut self,
        target: TuiConversationRestoreTarget,
        origin: TuiConversationRestoreOrigin,
        ctx: &mut ViewContext<Self>,
    ) {
        if self.is_conversation_restore_loading() {
            return;
        }
        self.next_restore_request_id = self.next_restore_request_id.wrapping_add(1);
        let request_id = self.next_restore_request_id;
        self.conversation_restore_state = ConversationRestoreState::Loading {
            origin,
            request_id,
            future: None,
        };

        ctx.notify();
        let future =
            BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, _ctx| match &target {
                TuiConversationRestoreTarget::Local(conversation_id) => {
                    history.load_conversation_data(*conversation_id)
                }
                TuiConversationRestoreTarget::Server(server_token) => {
                    history.load_conversation_by_server_token(server_token)
                }
            });

        let future_handle = ctx.spawn(future, move |view, result, ctx| {
            view.handle_conversation_restore_result(target, origin, request_id, result, ctx);
        });
        match &mut self.conversation_restore_state {
            ConversationRestoreState::Loading {
                request_id: active_request_id,
                future,
                ..
            } if *active_request_id == request_id => {
                *future = Some(future_handle);
            }
            ConversationRestoreState::Idle
            | ConversationRestoreState::Failed(_)
            | ConversationRestoreState::Loading { .. } => future_handle.abort(),
        }
    }

    /// Validates a completed load before starting synchronous surface replacement.
    fn handle_conversation_restore_result(
        &mut self,
        target: TuiConversationRestoreTarget,
        origin: TuiConversationRestoreOrigin,
        request_id: u64,
        result: Option<LoadedConversationData>,
        ctx: &mut ViewContext<Self>,
    ) {
        if !self.is_current_restore_request(request_id) {
            return;
        }

        let conversation = match result {
            Some(LoadedConversationData::Oz(conversation)) => conversation,
            Some(LoadedConversationData::CLIAgent(_)) => {
                self.fail_conversation_restore(
                    request_id,
                    "The Phosphor TUI only supports Phosphor Agent conversations.".to_owned(),
                    ctx,
                );
                return;
            }
            None => {
                self.fail_conversation_restore(
                    request_id,
                    "The conversation could not be loaded.".to_owned(),
                    ctx,
                );
                return;
            }
        };

        let matches_target = match &target {
            TuiConversationRestoreTarget::Local(conversation_id) => {
                conversation.id() == *conversation_id
            }
            TuiConversationRestoreTarget::Server(server_token) => {
                conversation.server_conversation_token() == Some(server_token)
            }
        };
        if !matches_target {
            self.fail_conversation_restore(
                request_id,
                "The restored conversation did not match the requested conversation.".to_owned(),
                ctx,
            );
            return;
        }

        self.replace_conversation_surface(*conversation, origin, ctx);
    }

    /// Discards the retained child-agent sessions of a previously restored
    /// parent tree before a different parent replaces it, without cancelling or
    /// deleting the underlying local processes.
    fn discard_replaced_child_agent_sessions(
        &self,
        previous_parent_conversation_id: AIConversationId,
        ctx: &mut ViewContext<Self>,
    ) {
        if !ctx.has_singleton_model::<TuiOrchestrationModel>()
            || !ctx.has_singleton_model::<TuiSessions>()
        {
            return;
        }
        TuiOrchestrationModel::handle(ctx).update(ctx, |model, ctx| {
            model.discard_restored_descendant_sessions(previous_parent_conversation_id, ctx);
        });
    }

    /// Eagerly materializes retained TUI sessions for every supported,
    /// locally-known descendant of the just-restored parent conversation, so
    /// restored child agents appear in the orchestration tab bar. Both restore
    /// entry points (startup `--resume` and the conversations menu) converge
    /// here, so both restore the same descendant tree.
    fn restore_child_agent_sessions(
        &self,
        parent_conversation_id: AIConversationId,
        ctx: &mut ViewContext<Self>,
    ) {
        if !ctx.has_singleton_model::<TuiOrchestrationModel>()
            || !ctx.has_singleton_model::<TuiSessions>()
        {
            return;
        }
        let Some(root_session_id) =
            TuiSessions::as_ref(ctx).session_id_for_surface(self.terminal_surface_id)
        else {
            return;
        };
        TuiOrchestrationModel::handle(ctx).update(ctx, |model, ctx| {
            model.restore_descendant_sessions(parent_conversation_id, root_session_id, ctx);
        });
    }

    /// Replaces the visible conversation and completes the restore state transition.
    fn replace_conversation_surface(
        &mut self,
        conversation: AIConversation,
        origin: TuiConversationRestoreOrigin,
        ctx: &mut ViewContext<Self>,
    ) {
        let previous_conversation_id = self
            .conversation_selection
            .as_ref(ctx)
            .selected_conversation_id(ctx);
        if let Some(previous_conversation_id) = previous_conversation_id {
            // When a different parent replaces the current tree, drop the prior
            // tree's retained child-session projections first (idempotent for a
            // re-restore of the same parent, which is left untouched here and
            // deduplicated by `restore_descendant_sessions`).
            if previous_conversation_id != conversation.id() {
                self.discard_replaced_child_agent_sessions(previous_conversation_id, ctx);
            }

            self.transcript.update(ctx, |transcript, ctx| {
                transcript.clear_for_replacement(ctx);
            });

            self.terminal_model
                .lock()
                .block_list_mut()
                .remove_command_blocks_for_conversation(previous_conversation_id);

            self.ai_action_model.update(ctx, |actions, _| {
                actions.clear_restored_action_results();
            });

            BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, ctx| {
                history.clear_conversations_in_terminal_view(self.terminal_surface_id, ctx);
            });
        }

        let conversation_id = conversation.id();
        let restoration_plan = {
            let mut terminal_model = self.terminal_model.lock();
            prepare_conversation_block_restoration(&conversation, &mut terminal_model)
        };

        self.ai_action_model.update(ctx, |actions, _| {
            actions.restore_action_results_from_exchanges(restoration_plan.exchanges().collect());
        });

        BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, ctx| {
            history.restore_conversations(self.terminal_surface_id, vec![conversation], ctx);
        });

        self.transcript.update(ctx, |transcript, ctx| {
            transcript.restore_conversation(conversation_id, restoration_plan, ctx);
        });

        BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, ctx| {
            history.set_active_conversation_id(conversation_id, self.terminal_surface_id, ctx);
        });

        self.conversation_selection.update(ctx, |selection, ctx| {
            selection.select_existing_conversation(
                conversation_id,
                origin.agent_view_origin(),
                ctx,
            );
        });

        // Restore the parent's child-agent tree so restored children appear in
        // the orchestration tab bar. The parent stays focused; children are
        // materialized as background sessions.
        self.restore_child_agent_sessions(conversation_id, ctx);

        self.conversation_restore_state = ConversationRestoreState::Idle;
        self.refresh_exit_summary(ctx);
        self.focus_input_if_active(ctx);
        ctx.notify();
    }

    fn is_current_restore_request(&self, request_id: u64) -> bool {
        matches!(
            &self.conversation_restore_state,
            ConversationRestoreState::Loading {
                request_id: active_request_id,
                ..
            } if *active_request_id == request_id
        )
    }

    fn is_conversation_restore_loading(&self) -> bool {
        matches!(
            &self.conversation_restore_state,
            ConversationRestoreState::Loading { .. }
        )
    }

    fn cancel_conversation_restore(&mut self, ctx: &mut ViewContext<Self>) -> bool {
        let state = std::mem::take(&mut self.conversation_restore_state);
        let ConversationRestoreState::Loading { future, .. } = state else {
            self.conversation_restore_state = state;
            return false;
        };
        if let Some(future) = future {
            future.abort();
        }
        self.next_restore_request_id = self.next_restore_request_id.wrapping_add(1);
        self.focus_input_if_active(ctx);
        ctx.notify();
        true
    }

    fn fail_conversation_restore(
        &mut self,
        request_id: u64,
        message: String,
        ctx: &mut ViewContext<Self>,
    ) {
        let origin = match &self.conversation_restore_state {
            ConversationRestoreState::Loading {
                origin,
                request_id: active_request_id,
                ..
            } if *active_request_id == request_id => *origin,
            ConversationRestoreState::Idle
            | ConversationRestoreState::Failed(_)
            | ConversationRestoreState::Loading { .. } => return,
        };
        match origin {
            TuiConversationRestoreOrigin::Startup => {
                self.conversation_restore_state = ConversationRestoreState::Failed(message);
            }
            // Fork switches the surface synchronously (no async restore load), so
            // this failure path is not reached for it; handle it like the list
            // switch for completeness.
            TuiConversationRestoreOrigin::ConversationList | TuiConversationRestoreOrigin::Fork => {
                self.conversation_restore_state = ConversationRestoreState::Idle;
                self.show_transient_hint(message, ctx);
                self.focus_input_if_active(ctx);
            }
        }
        ctx.notify();
    }

    fn refresh_exit_summary(&self, ctx: &AppContext) {
        if !self.is_focused_session(ctx) {
            return;
        }
        self.write_exit_summary(ctx);
    }

    fn write_exit_summary(&self, ctx: &AppContext) {
        let token = self
            .conversation_selection
            .as_ref(ctx)
            .selected_conversation(ctx)
            .filter(|conversation| !conversation.is_empty())
            .and_then(|conversation| conversation.server_conversation_token())
            .cloned();
        self.exit_summary.set_token(token);
    }

    /// Applies a laid-out terminal content size to the terminal model and PTY.
    /// TUI counterpart of the GUI's `after_terminal_view_layout`
    /// (`app/src/terminal/view.rs`): consumes the after-layout resize channel
    /// and commits the resize with a `ViewContext`. Fed by the
    /// [`TuiTerminalContentElement`] wrapping the block-list content column or the
    /// alt-screen grid, so the PTY tracks whichever region PTY content
    /// currently occupies.
    fn handle_terminal_resize(&mut self, size: TuiSize, ctx: &mut ViewContext<Self>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        let size_update = SizeUpdate::from_cell_dimensions(
            self.size_info,
            usize::from(size.height),
            usize::from(size.width),
        );
        if !size_update.rows_or_columns_changed() {
            return;
        }

        self.terminal_model.lock().resize(size_update);
        self.size_info = size_update.new_size();
        ctx.emit(TuiTerminalSessionEvent::Resize(size_update));
        ctx.notify();
    }
    /// Refreshes terminal model geometry and redraws only when this session is visible.
    fn handle_terminal_wakeup(&mut self, ctx: &mut ViewContext<Self>) -> bool {
        {
            let mut model = self.terminal_model.lock();
            if !model.is_alt_screen_active() {
                model.block_list_mut().update_background_block_height();
                model.block_list_mut().update_active_block_height();
            }
        }
        let is_focused = self.is_focused_session(ctx);
        if is_focused {
            // A redraw must not re-run the full focus reconciliation: doing so
            // re-focuses whatever `focus_current_owner` picks, which stomps any
            // nested focus an interaction surface has delegated to a child (the
            // statusline picker's rows, a blocker's option selector). The only
            // case a wakeup has to correct is a command that became
            // long-running while the hidden composer still held focus — editor
            // keys such as Enter would otherwise be intercepted instead of
            // reaching the running process.
            let pty_owns_input = self.input_target().pty_owns_input();
            if pty_owns_input && self.input_view.is_focused(ctx) {
                self.focus_current_owner(ctx);
            }
            ctx.notify();
        }
        is_focused
    }

    /// Re-renders on history events that can change the warping indicator:
    /// the selected conversation's status changing, or an exchange starting
    /// (which re-anchors the elapsed counter) on this surface.
    fn handle_history_event(
        &mut self,
        event: &BlocklistAIHistoryEvent,
        ctx: &mut ViewContext<Self>,
    ) {
        if event
            .terminal_view_id()
            .is_some_and(|id| id != self.terminal_surface_id)
        {
            return;
        }
        if let Some(persistence_event) =
            maybe_build_ai_query_upsert_event(event, self.terminal_surface_id, false, ctx)
            && let Some(model_event_sender) = PersistenceWriter::handle(ctx).as_ref(ctx).sender()
        {
            let _ = ctx.spawn(
                async move { model_event_sender.send(persistence_event) },
                |_, result, _| {
                    if let Err(error) = result {
                        report_error!(
                            anyhow::Error::new(error)
                                .context("Error sending TUI upsert AI query event")
                        );
                    }
                },
            );
        }
        if matches!(
            event,
            BlocklistAIHistoryEvent::AppendedExchange { .. }
                | BlocklistAIHistoryEvent::UpdatedStreamingExchange { .. }
                | BlocklistAIHistoryEvent::UpdatedConversationStatus { .. }
                | BlocklistAIHistoryEvent::UpdatedTodoList { .. }
                | BlocklistAIHistoryEvent::UpdatedAutoexecuteOverride { .. }
        ) {
            ctx.notify();
        }
        if matches!(event, BlocklistAIHistoryEvent::UpdatedTodoList { .. }) {
            self.sync_open_todo_menu_list(ctx);
        }

        if matches!(event, BlocklistAIHistoryEvent::RestoredConversations { .. }) {
            self.refresh_exit_summary(ctx);
        }
        match event {
            BlocklistAIHistoryEvent::UpdatedConversationStatus {
                conversation_id, ..
            } => {
                self.maybe_send_queued_follow_up(*conversation_id, ctx);
            }
            BlocklistAIHistoryEvent::RemoveConversation {
                conversation_id, ..
            }
            | BlocklistAIHistoryEvent::DeletedConversation {
                conversation_id, ..
            } => {
                // Drop a queued /compact-and follow-up if its conversation is gone.
                if self
                    .queued_follow_up
                    .as_ref()
                    .is_some_and(|f| f.conversation_id == *conversation_id)
                {
                    self.queued_follow_up = None;
                }
                // Drop any recorded file-edit diffs for the gone conversation.
                let removed_conversation_id = *conversation_id;
                TuiFileEditRevertRegistry::handle(ctx).update(ctx, |registry, _| {
                    registry.forget_conversation(&removed_conversation_id);
                });
                self.cli_subagent_views
                    .retain(|_, view| view.as_ref(ctx).conversation_id() != *conversation_id);
            }
            BlocklistAIHistoryEvent::ClearedConversationsInTerminalView { .. } => {
                self.cli_subagent_views.clear();
            }
            _ => {}
        }
    }

    /// Drives the `/compact-and` follow-up state machine on each conversation
    /// status change. Once the summarize turn has been observed running
    /// (`seen_in_progress`), the next terminal status submits the follow-up on
    /// success, or restores it to the input on error/cancel so it isn't lost.
    fn maybe_send_queued_follow_up(
        &mut self,
        conversation_id: AIConversationId,
        ctx: &mut ViewContext<Self>,
    ) {
        let Some(follow_up) = self.queued_follow_up.as_ref() else {
            return;
        };
        if follow_up.conversation_id != conversation_id {
            return;
        }
        let Some(status) = BlocklistAIHistoryModel::as_ref(ctx)
            .conversation(&conversation_id)
            .map(|conversation| conversation.status().clone())
        else {
            return;
        };
        if status.is_in_progress() || status.is_blocked() {
            // The summarize turn (or a blocked action within it) is running.
            if let Some(follow_up) = self.queued_follow_up.as_mut() {
                follow_up.seen_in_progress = true;
            }
            return;
        }
        // Terminal status. Ignore the pre-summarize idle state we may see before
        // the turn starts; only fire once we've actually observed it in progress.
        if !follow_up.seen_in_progress {
            return;
        }
        let follow_up = self.queued_follow_up.take().expect("checked above");
        if status.is_error() || status.is_cancelled() {
            // Don't lose the prompt: drop it back into the input for the user,
            // but never clobber whatever they may have started typing.
            let input_is_empty = self.input_view.as_ref(ctx).is_empty(ctx);
            if input_is_empty {
                self.input_view.update(ctx, |input, ctx| {
                    input.set_text(&follow_up.prompt, ctx);
                });
            }
            return;
        }
        // We're inside a BlocklistAIHistoryModel event dispatch; `send_prompt`
        // updates that same model, so defer it to the next tick to avoid a
        // reentrant update.
        let prompt = follow_up.prompt;
        ctx.spawn(Timer::after(Duration::ZERO), move |view, _, ctx| {
            view.send_prompt(prompt, ctx);
        });
    }

    fn show_auto_approve_feedback(&mut self, ctx: &mut ViewContext<Self>) {
        self.auto_approve_feedback_conversation_id = self
            .conversation_selection
            .as_ref(ctx)
            .selected_conversation_id(ctx);
        let timer = ctx.spawn(
            Timer::after(AUTO_APPROVE_FEEDBACK_DURATION),
            |view, _, ctx| {
                view.auto_approve_feedback_conversation_id = None;
                view.auto_approve_feedback_timer = None;
                ctx.notify();
            },
        );
        if let Some(previous_timer) = self.auto_approve_feedback_timer.replace(timer) {
            previous_timer.abort();
        }
        ctx.notify();
    }

    fn clear_auto_approve_feedback(&mut self, ctx: &mut ViewContext<Self>) {
        self.auto_approve_feedback_conversation_id = None;
        if let Some(timer) = self.auto_approve_feedback_timer.take() {
            timer.abort();
        }
        ctx.notify();
    }

    fn toggle_auto_approve(&mut self, show_feedback: bool, ctx: &mut ViewContext<Self>) {
        self.conversation_selection.update(ctx, |selection, ctx| {
            selection.toggle_pending_query_autoexecute(ctx);
        });
        if show_feedback {
            self.show_auto_approve_feedback(ctx);
            let enabled = self
                .conversation_selection
                .as_ref(ctx)
                .pending_query_autoexecute_override(ctx)
                .is_autoexecute_any_action();
            self.show_success_hint(
                if enabled {
                    AUTO_APPROVE_ENABLED_HINT
                } else {
                    AUTO_APPROVE_DISABLED_HINT
                }
                .to_owned(),
                ctx,
            );
        } else {
            self.clear_auto_approve_feedback(ctx);
        }
    }

    fn handle_pasted(&mut self, text: String, ctx: &mut ViewContext<Self>) {
        let disposition = self
            .attachment_bar
            .update(ctx, |bar, ctx| bar.try_attach_paste(text.clone(), ctx));
        if disposition == TuiAttachmentPasteDisposition::NotHandled {
            self.input_view
                .update(ctx, |input, ctx| input.insert_pasted_text(&text, ctx));
        }
    }

    fn handle_attachment_bar_event(
        &mut self,
        event: &TuiAttachmentBarEvent,
        ctx: &mut ViewContext<Self>,
    ) {
        match event {
            TuiAttachmentBarEvent::AbortInputDetection => self.abort_input_detection(ctx),
            TuiAttachmentBarEvent::RequestInputDetection => self.schedule_input_detection(ctx),
            TuiAttachmentBarEvent::RestorePastedText(text) => {
                self.input_view
                    .update(ctx, |input, ctx| input.insert_pasted_text(text, ctx));
            }
            TuiAttachmentBarEvent::ShowHint(text) => {
                self.show_transient_hint(text.clone(), ctx);
            }
            TuiAttachmentBarEvent::ReturnFocus => ctx.focus(&self.input_view),
        }
        ctx.notify();
    }

    /// Displays `text` in the footer's hint slot for the transient-hint
    /// duration, then reverts to the persistent content.
    fn show_transient_hint(&mut self, text: String, ctx: &mut ViewContext<Self>) {
        self.transient_hint
            .show(text, ctx, |view| &mut view.transient_hint);
    }

    /// Displays success-colored feedback in the transient footer slot.
    fn show_success_hint(&mut self, text: String, ctx: &mut ViewContext<Self>) {
        self.transient_hint
            .show_success(text, ctx, |view| &mut view.transient_hint);
    }

    /// Displays error-colored feedback in the transient footer slot.
    fn show_error_hint(&mut self, text: String, ctx: &mut ViewContext<Self>) {
        self.transient_hint
            .show_error(text, ctx, |view| &mut view.transient_hint);
    }

    /// Surfaces a zero-state ASCII-art load/reload failure (see
    /// `zero_state_animation_config::AsciiArtError`) as an error-colored
    /// transient footer hint. Ported from the pin (`02b53fcd8`) as part of
    /// #384.
    fn show_zero_state_ascii_load_failure(
        &mut self,
        failure: ZeroStateAnimationLoadFailure,
        ctx: &mut ViewContext<Self>,
    ) {
        self.show_error_hint(zero_state_ascii_load_failure_hint(failure).to_owned(), ctx);
    }

    /// Surfaces a settings-file load or reload failure as an error-colored
    /// footer hint. Ported from the pin's `show_settings_file_error`.
    fn show_settings_file_error(&mut self, error: &SettingsFileError, ctx: &mut ViewContext<Self>) {
        self.show_error_hint(settings_file_error_hint(error).to_owned(), ctx);
    }

    /// Surfaces a `/usage` or `/cost` result. Mirrors the GUI's toast split: a plain transient
    /// hint when the command could not report anything, success-colored for the report itself.
    fn show_usage_cost_outcome(&mut self, outcome: UsageCostOutcome, ctx: &mut ViewContext<Self>) {
        let message = outcome.message().to_owned();
        if outcome.is_unavailable() {
            self.show_transient_hint(message, ctx);
        } else {
            self.show_success_hint(message, ctx);
        }
    }

    /// Displays success-colored feedback in the transient footer slot.
    fn show_copy_hint(&mut self, ctx: &mut ViewContext<Self>) {
        self.show_success_hint(COPY_SELECTION_HINT.to_owned(), ctx);
    }

    /// Handles a ctrl-c press.
    ///
    /// Priority order:
    /// 1. Cancel in-flight conversation restore.
    /// 2. Dismiss an open read-only sheet, then handle terminal-use takeover.
    /// 3. **Kill-child path (tab-bar focused + child tab selected):** a single
    ///    ctrl-c immediately kills the selected child agent and returns focus to
    ///    the root/main orchestration agent.
    /// 4. **Kill-child path (viewing a child conversation without tab focus):**
    ///    the first ctrl-c arms a 1-second kill window and shows a child-kill
    ///    footer hint; a second ctrl-c within the window kills the child agent.
    /// 5. **Exit path (root/main agent or no orchestration):** a second press
    ///    within [`CTRL_C_EXIT_WINDOW`] exits the TUI; otherwise one contextual
    ///    action runs — cancel the running conversation if there is one, else
    ///    clear the input — and the exit confirmation is (re-)armed, surfacing
    ///    [`CTRL_C_EXIT_HINT`] in the footer.
    fn handle_interrupt(&mut self, ctx: &mut ViewContext<Self>) {
        if self.cancel_conversation_restore(ctx) {
            return;
        }
        if matches!(
            &self.conversation_restore_state,
            ConversationRestoreState::Failed(_)
        ) {
            ctx.terminate_app(TerminationMode::ForceTerminate, None);
            return;
        }
        // ctrl-c dismisses an open read-only sheet (the `?` shortcuts sheet, the
        // `/status` menu) before it does anything else, exactly as escape does.
        // Without this the sheet stays painted over the session while the
        // interrupt takes control of a running command underneath it, and only
        // a second, unrelated keystroke clears it. Ported from the pin
        // (`02b53fcd8:crates/warp_tui/src/terminal_session_view.rs`), where this
        // block sits in the same position; it was missing here.
        self.suggestions_mode.update(ctx, |mode, ctx| {
            if let Some(kind) = mode.mode().read_only_menu() {
                mode.close_if_active(TuiInputSuggestionsMode::ReadOnlyMenu(kind), ctx);
            }
        });
        if self.handle_terminal_use_interrupt(ctx) {
            self.exit_confirmation.disarm();
            self.child_kill_armed_conversation = None;
            ctx.notify();
            return;
        }

        // Path 1: tab-bar focused + killable tab selected (a level child, or
        // the drilled-in anchor occupying the main-tab slot) → single ctrl-c
        // kills that agent and its loaded subtree, per kill_child_agent. The
        // root tab is never a kill target, matching the footers.
        if self.orchestration_tabs_focused
            && let Some((child_id, _)) = self.bar_focused_kill_target(ctx)
        {
            self.kill_child_agent(child_id, ctx);
            return;
        }

        // Path 2: tab-bar not focused, viewing a child conversation.
        // First ctrl-c arms the kill window; second within ~1s kills the child.
        if !self.orchestration_tabs_focused
            && let Some(child_id) = self.is_child_conversation_selected(ctx)
        {
            let now = Instant::now();
            if self.child_kill_armed_conversation == Some(child_id)
                && self.exit_confirmation.should_exit(now)
            {
                // Second ctrl-c: kill the child and return to main agent.
                self.kill_child_agent(child_id, ctx);
                return;
            }
            // First ctrl-c: arm the kill window with the child-specific hint.
            self.child_kill_armed_conversation = Some(child_id);
            let window_expires_at = self.exit_confirmation.arm(now);
            ctx.spawn(Timer::after(CTRL_C_EXIT_WINDOW), move |view, _, ctx| {
                if view.exit_confirmation.disarm_expired(window_expires_at) {
                    view.child_kill_armed_conversation = None;
                    ctx.notify();
                }
            });
            ctx.notify();
            return;
        }

        // Path 3 (original): root/main agent or no orchestration.
        // Ensure any stale kill window is cleared before the normal exit path.
        self.child_kill_armed_conversation = None;
        let now = Instant::now();
        if self.exit_confirmation.should_exit(now) {
            ctx.terminate_app(TerminationMode::ForceTerminate, None);
            return;
        }

        if !self.cancel_active_conversation(ctx) {
            self.input_view.update(ctx, |input, ctx| input.clear(ctx));
        }

        // Arm (or re-arm) the confirmation, and disarm + repaint when the
        // window lapses. A re-arm supersedes this (now stale) timer, making
        // its `disarm_expired` a no-op rather than clearing the newer window.
        let window_expires_at = self.exit_confirmation.arm(now);
        ctx.spawn(Timer::after(CTRL_C_EXIT_WINDOW), move |view, _, ctx| {
            if view.exit_confirmation.disarm_expired(window_expires_at) {
                ctx.notify();
            }
        });
        ctx.notify();
    }

    /// Handles ctrl-d while the prompt is focused. Unlike ctrl-c, ctrl-d exits
    /// immediately when the prompt is empty; otherwise it keeps its editing
    /// role of deleting the next character.
    fn handle_eof(&mut self, ctx: &mut ViewContext<Self>) {
        if self.input_view.as_ref(ctx).is_empty(ctx) {
            ctx.terminate_app(TerminationMode::ForceTerminate, None);
        } else {
            self.input_view.update(ctx, |input, ctx| {
                input.handle_action(
                    &TuiInputAction::EditorCommand(TuiEditorCommand::DeleteForward),
                    ctx,
                );
            });
        }
    }

    /// Cancels the surface's running conversation (in-flight stream or pending
    /// tool actions), returning whether there was one to cancel.
    pub(crate) fn cancel_active_conversation(&mut self, ctx: &mut ViewContext<Self>) -> bool {
        let terminal_surface_id = ctx.view_id();
        self.ai_controller.update(ctx, |controller, ctx| {
            let conversation_id = BlocklistAIHistoryModel::as_ref(ctx)
                .active_conversation(terminal_surface_id)
                // A brand-new conversation reports `InProgress` before any
                // exchange exists; there is nothing to cancel yet.
                .filter(|conversation| !conversation.is_empty())
                .filter(|conversation| {
                    let status = conversation.status();
                    status.is_in_progress() || status.is_blocked()
                })
                .map(|conversation| conversation.id());
            let Some(conversation_id) = conversation_id else {
                return false;
            };
            controller.cancel_conversation_progress(
                conversation_id,
                CancellationReason::ManuallyCancelled,
                ctx,
            );
            true
        })
    }

    fn render_warping_indicator(
        &self,
        label: &'static str,
        elapsed: Duration,
        conversation_id: AIConversationId,
        ctx: &AppContext,
    ) -> Box<dyn TuiElement> {
        let builder = TuiUiBuilder::from_app(ctx);
        let is_hovered = self
            .warping_auto_approve_mouse
            .lock()
            .is_ok_and(|state| state.is_hovered());
        let style = if is_hovered {
            builder.primary_text_style()
        } else if self.auto_approve_feedback_conversation_id == Some(conversation_id) {
            builder.success_glyph_style()
        } else {
            builder.muted_text_style()
        };
        let enabled = self
            .conversation_selection
            .as_ref(ctx)
            .pending_query_autoexecute_override(ctx)
            .is_autoexecute_any_action();
        let auto_approve = TuiHoverable::new(
            self.warping_auto_approve_mouse.clone(),
            TuiText::new(format!(
                "▶▶ Auto approve {}",
                if enabled { "on" } else { "off" }
            ))
            .with_style(style)
            .truncate()
            .finish(),
        )
        .on_click(|event_ctx, _| {
            event_ctx.dispatch_typed_action(TuiTerminalSessionAction::ToggleAutoApprove {
                show_feedback: false,
            });
        })
        .finish();
        render_warping_indicator_row(label, elapsed, auto_approve, ctx)
    }





    /// Mirrors the GUI `/cost` eligibility checks, then toggles the selected
    /// conversation's completed-response summary.
    fn toggle_response_summary_visibility(&mut self, ctx: &mut ViewContext<Self>) {
        let selected_conversation = self
            .conversation_selection
            .as_ref(ctx)
            .selected_conversation(ctx)
            .map(|conversation| {
                (
                    conversation.latest_exchange().map(|exchange| exchange.id),
                    conversation.is_empty(),
                    conversation.status().is_done(),
                )
            });
        if let Some(hint) = cost_command_unavailable_hint(
            selected_conversation.map(|(_, is_empty, is_done)| (is_empty, is_done)),
        ) {
            self.show_transient_hint(hint.to_owned(), ctx);
            return;
        }
        let Some((Some(exchange_id), _, _)) = selected_conversation else {
            self.show_transient_hint(COST_NO_ACTIVE_CONVERSATION_HINT.to_owned(), ctx);
            return;
        };
        self.toggle_response_summary_visibility_for_exchange(exchange_id);
        ctx.notify();
    }
    fn toggle_response_summary_visibility_for_exchange(&mut self, exchange_id: AIAgentExchangeId) {
        if !self
            .hidden_response_summary_exchange_ids
            .remove(&exchange_id)
        {
            self.hidden_response_summary_exchange_ids
                .insert(exchange_id);
        }
    }

    fn render_response_summary_for_exchange(
        &self,
        exchange_id: AIAgentExchangeId,
        duration: Duration,
        block_credits: Option<f32>,
        ctx: &AppContext,
    ) -> Option<Box<dyn TuiElement>> {
        (!self
            .hidden_response_summary_exchange_ids
            .contains(&exchange_id))
        .then(|| render_response_summary(duration, block_credits, ctx))
    }

    fn has_active_todo_list(&self, ctx: &AppContext) -> bool {
        self.active_todo_menu_list_key(ctx).is_some()
    }

    /// Identifies the TODO list an open TODO menu is currently showing: the
    /// selected conversation plus how many lists it has accumulated. A new list
    /// (the agent replanning) bumps the count, which is what tells
    /// [`Self::sync_open_todo_menu_list`] to re-anchor the scroll position.
    fn active_todo_menu_list_key(&self, ctx: &AppContext) -> Option<(AIConversationId, usize)> {
        let selection = self.conversation_selection.as_ref(ctx);
        let conversation_id = selection.selected_conversation_id(ctx)?;
        let conversation = selection.selected_conversation(ctx)?;
        conversation
            .active_todo_list()
            .filter(|todo_list| !todo_list.is_empty())?;
        Some((conversation_id, conversation.todo_lists().len()))
    }

    /// Where a freshly opened read-only menu anchors its viewport. The TODO
    /// menu skips past the completed rows (and the section title) so the
    /// in-progress task is the first thing visible; the other menus start at
    /// the top.
    fn read_only_menu_initial_scroll_top(
        &self,
        kind: TuiReadOnlyMenuKind,
        ctx: &AppContext,
    ) -> usize {
        match kind {
            TuiReadOnlyMenuKind::Shortcuts | TuiReadOnlyMenuKind::Status => 0,
            TuiReadOnlyMenuKind::Todos => self
                .conversation_selection
                .as_ref(ctx)
                .selected_conversation(ctx)
                .and_then(AIConversation::active_todo_list)
                .filter(|todo_list| !todo_list.pending_items().is_empty())
                .map(|todo_list| todo_list.completed_items().len().saturating_add(1))
                .unwrap_or_default(),
        }
    }

    fn close_todo_menu_if_unavailable(&mut self, ctx: &mut ViewContext<Self>) {
        if self.has_active_todo_list(ctx) {
            return;
        }
        self.suggestions_mode.update(ctx, |mode, ctx| {
            mode.close_if_active(
                TuiInputSuggestionsMode::ReadOnlyMenu(TuiReadOnlyMenuKind::Todos),
                ctx,
            );
        });
    }

    /// Keeps an open TODO menu pointed at the live list: re-anchors the scroll
    /// position when the agent starts a new list, and closes the menu when the
    /// list it was showing goes away. Item-level edits within the same list
    /// leave the user's scroll position alone.
    fn sync_open_todo_menu_list(&mut self, ctx: &mut ViewContext<Self>) {
        if !todo_menu_is_open(self.suggestions_mode.as_ref(ctx).mode()) {
            self.open_todo_menu_list_key = None;
            return;
        }
        let key = self.active_todo_menu_list_key(ctx);
        let Some(key) = key else {
            self.open_todo_menu_list_key = None;
            self.close_todo_menu_if_unavailable(ctx);
            return;
        };
        if self.open_todo_menu_list_key.as_ref() != Some(&key) {
            self.open_todo_menu_list_key = Some(key);
            let scroll_top =
                self.read_only_menu_initial_scroll_top(TuiReadOnlyMenuKind::Todos, ctx);
            self.read_only_menu_viewport
                .scroll_to_rows_from_top(scroll_top);
        }
    }

    /// Toggles the read-only TODO menu above the input, from the footer's
    /// clickable to-do progress control. A conversation with no active list has
    /// nothing to show, so the toggle is inert.
    fn toggle_todo_menu(&mut self, ctx: &mut ViewContext<Self>) {
        if !self.has_active_todo_list(ctx) {
            return;
        }
        let todo_mode = TuiInputSuggestionsMode::ReadOnlyMenu(TuiReadOnlyMenuKind::Todos);
        self.suggestions_mode.update(ctx, |mode, ctx| {
            if mode.mode() == todo_mode {
                mode.close_if_active(todo_mode, ctx);
            } else {
                mode.set_mode(todo_mode, ctx);
            }
        });
        self.sync_open_todo_menu_list(ctx);
    }

    /// Toggles the inline model picker from the footer's active-model label —
    /// the same menu `/model` surfaces. The model's existing open/dismiss paths
    /// preserve active-menu arbitration, input cleanup, and selection handling.
    fn toggle_model_menu(&mut self, ctx: &mut ViewContext<Self>) {
        self.model_menu.update(ctx, |menu, ctx| {
            if menu.is_open(ctx) {
                menu.dismiss(ctx);
            } else {
                menu.open(ctx);
            }
        });
    }

    /// The selected conversation's context-window occupancy (0.0–1.0), or
    /// `None` (entry hidden) until any usage has been reported.
    fn selected_conversation_context_usage(&self, ctx: &AppContext) -> Option<f32> {
        let fraction = self
            .conversation_selection
            .as_ref(ctx)
            .selected_conversation(ctx)?
            .context_window_usage();
        (fraction > 0.0).then_some(fraction)
    }

    /// The session's working directory. The cwd only arrives once shell
    /// metadata flows (warpified sessions); until then fall back to the
    /// process cwd the TUI's shell was spawned with.
    fn current_working_directory(&self, ctx: &AppContext) -> Option<String> {
        self.active_session
            .as_ref(ctx)
            .current_working_directory()
            .cloned()
            .or_else(|| {
                std::env::current_dir()
                    .ok()
                    .map(|cwd| cwd.to_string_lossy().into_owned())
            })
    }

    /// Current input buffer text.
    fn input_buffer_text(&self, ctx: &AppContext) -> String {
        let editor = self.input_view.as_ref(ctx).model().as_ref(ctx);
        let buffer = editor.content().as_ref(ctx);
        if buffer.is_empty() {
            String::new()
        } else {
            buffer.text().into_string()
        }
    }

    // Tab-completion's entry point (request_shell_completion), staleness
    // tracking (abort_shell_completion, handle_completion_editor_changed),
    // and live-as-you-type result handling now live in
    // terminal_session_view/completions.rs (#390).

    /// Applies an accepted completion: replaces the completed span in the input
    /// buffer with the chosen replacement text.
    fn handle_accepted_completion(
        &mut self,
        completion: TuiAcceptedCompletion,
        ctx: &mut ViewContext<Self>,
    ) {
        let buffer_text = self.input_buffer_text(ctx);
        let TuiAcceptedCompletion {
            replacement,
            span,
            append_space,
        } = completion;
        let Some(new_text) = crate::completions_menu::apply_completion_replacement(
            &buffer_text,
            &replacement,
            &span,
            append_space,
        ) else {
            return;
        };
        self.input_view
            .update(ctx, |input, ctx| input.set_text(&new_text, ctx));
        ctx.notify();
    }

    /// Whether the input is in detected or explicitly locked shell mode.
    fn is_shell_mode(&self, ctx: &AppContext) -> bool {
        input_mode_policy::is_shell_mode(self.ai_input_model.as_ref(ctx))
    }

    /// Routes a submission to shell execution or the agent conversation based
    /// on the input mode.
    fn handle_submitted(&mut self, text: String, ctx: &mut ViewContext<Self>) {
        // A stale editor frame must not submit into a shell that is still
        // bootstrapping or has handed input to a foreground process.
        if !self.input_target().agent_editor_owns_input() {
            return;
        }
        if !matches!(
            self.conversation_restore_state,
            ConversationRestoreState::Idle
        ) {
            return;
        }
        if self.send_terminal_use_prompt(&text, ctx) {
            self.lock_for_agent_control(ctx);
        } else if self.is_shell_mode(ctx) {
            self.execute_user_command(&text, ctx);
        } else {
            self.handle_submitted_input(&text, ctx);
        }
        ctx.notify();
    }

    /// Executes `command` in the session's PTY as a plain user command.
    ///
    /// Mirrors the GUI's shell-mode submission: rejected while the agent holds
    /// the PTY with an active long-running command (the input keeps its text
    /// and a transient hint is shown), and an in-progress conversation is
    /// cancelled when the command runs. On success the input clears and exits
    /// shell mode back to agent input.
    fn execute_user_command(&mut self, command: &str, ctx: &mut ViewContext<Self>) {
        // A whitespace-only command is a no-op; stay in shell mode. The command
        // itself is sent to the PTY untrimmed, exactly as typed.
        if command.trim().is_empty() {
            return;
        }

        // Keep the lock scope to these reads only (see the terminal-model
        // locking guidance).
        let (is_pty_busy, session_id) = {
            let terminal_model = self.terminal_model.lock();
            let block_list = terminal_model.block_list();
            let active_block = block_list.active_block();
            let is_pty_busy = !block_list.is_bootstrapped()
                || (active_block.is_active_and_long_running()
                    && !active_block.is_in_band_command_block());
            (is_pty_busy, active_block.session_id())
        };
        let Some(session_id) = session_id else {
            log::warn!("Unable to execute TUI user command: no active session");
            return;
        };
        if is_pty_busy {
            self.show_transient_hint(COMMAND_ALREADY_RUNNING_HINT.to_owned(), ctx);
            return;
        }

        // Executing a shell command cancels an in-progress conversation
        // (mirrors the GUI; the running command above is left untouched).
        if let Some(conversation_id) = self
            .conversation_selection
            .as_ref(ctx)
            .selected_conversation_id(ctx)
        {
            let is_in_progress = BlocklistAIHistoryModel::as_ref(ctx)
                .conversation(&conversation_id)
                .is_some_and(|conversation| conversation.status().is_in_progress());
            if is_in_progress {
                self.ai_controller.update(ctx, |controller, ctx| {
                    controller.cancel_conversation_progress(
                        conversation_id,
                        CancellationReason::UserCommandExecuted,
                        ctx,
                    );
                });
            }
        }

        // Issue #387: a command accepted from the up-arrow history menu
        // carries this through from `handle_accepted_prompt_and_command_history`;
        // an ordinarily typed command leaves it `None`.
        let (workflow_id, workflow_command) =
            match self.pending_history_command_workflow_data.take() {
                Some(LinkedWorkflowData::Id(id)) => (Some(id), None),
                Some(LinkedWorkflowData::Command(command)) => (None, Some(command)),
                None => (None, None),
            };
        ctx.emit(TuiTerminalSessionEvent::ExecuteCommand(Box::new(
            ExecuteCommandEvent {
                command: command.to_owned(),
                session_id,
                workflow_id,
                workflow_command,
                should_add_command_to_history: true,
                source: CommandExecutionSource::User,
            },
        )));

        // The submission was accepted: clear the input and return to the
        // setting-derived agent default.
        self.input_view
            .update(ctx, |input_view, ctx| input_view.clear(ctx));
    }

    /// Sends a prompt to the TUI session's eagerly selected conversation.
    fn send_prompt(&mut self, prompt: String, ctx: &mut ViewContext<Self>) {
        let active_long_running_block_id = {
            let terminal_model = self.terminal_model.lock();
            let active_block = terminal_model.block_list().active_block();
            active_block
                .is_active_and_long_running()
                .then(|| active_block.id().clone())
        };
        let Some(conversation_id) = self
            .conversation_selection
            .as_ref(ctx)
            .selected_conversation_id(ctx)
        else {
            report_error!("TUI prompt submitted without an eagerly selected conversation");
            return;
        };
        // Zap's send_user_query_in_conversation returns (), not a dispatched bool.
        self.ai_controller.update(ctx, |controller, ctx| {
            controller.send_user_query_in_conversation(prompt.clone(), conversation_id, None, ctx)
        });
        // The pin gates this on the controller's `dispatched` return value.
        // This fork's controller returns `()` and always dispatches (see the
        // comment above and its twin in `send_subagent_prompt`), so the
        // notification is unconditional here rather than dropping a real
        // submission on a bool that does not exist.
        if let Some(publisher) = &self.cli_agent_osc_event_publisher {
            publisher
                .as_ref(ctx)
                .publish_prompt_submit(prompt.clone(), ctx);
        }
        if let Some(block_id) = active_long_running_block_id {
            self.cli_subagent_controller.update(ctx, |controller, ctx| {
                controller.set_latest_instruction(block_id, prompt, ctx);
            });
        }
    }

    fn handle_submitted_input(&mut self, input: &str, ctx: &mut ViewContext<Self>) {
        if self.is_conversation_restore_loading() {
            return;
        }
        match self
            .slash_commands_source
            .as_ref(ctx)
            .parse_input(input, ctx)
        {
            ParsedSlashCommandInput::SlashCommand(detected_command) => {
                self.execute_tui_slash_command(
                    &detected_command.command,
                    detected_command.argument.as_ref(),
                    ctx,
                );
            }
            ParsedSlashCommandInput::SkillCommand(detected_skill) => {
                self.execute_skill_command(detected_skill.reference, detected_skill.argument, ctx);
            }
            ParsedSlashCommandInput::None | ParsedSlashCommandInput::Composing { .. } => {
                let prompt = raw_prompt_if_not_blank(input);
                self.input_view.update(ctx, |input_view, ctx| {
                    input_view.clear(ctx);
                });
                if let Some(prompt) = prompt {
                    self.send_prompt(prompt.to_owned(), ctx);
                }
            }
        }
    }

    fn execute_skill_command(
        &mut self,
        reference: SkillReference,
        user_query: Option<String>,
        ctx: &mut ViewContext<Self>,
    ) {
        if !self
            .slash_commands_source
            .as_ref(ctx)
            .local_skills_available(ctx)
        {
            self.show_transient_hint(LOCAL_SKILLS_REMOTE_EXECUTION_ERROR_MESSAGE.to_owned(), ctx);
            return;
        }
        let result = self.ai_controller.update(ctx, |controller, ctx| {
            controller.send_invoke_skill_request(reference, user_query, ctx)
        });
        match result {
            Ok(()) => {
                self.input_view.update(ctx, |input, ctx| input.clear(ctx));
            }
            Err(error) => {
                self.show_transient_hint(error.to_string(), ctx);
            }
        }
    }

    fn handle_accepted_slash_command(
        &mut self,
        action: &AcceptSlashCommandOrSavedPrompt,
        ctx: &mut ViewContext<Self>,
    ) {
        match action {
            AcceptSlashCommandOrSavedPrompt::SlashCommand { id } => {
                let Some(command) = COMMAND_REGISTRY.get_command(id) else {
                    log::debug!("TUI slash command selection is not supported yet: {id:?}");
                    ctx.notify();
                    return;
                };
                self.select_tui_slash_command(command, ctx);
            }
            AcceptSlashCommandOrSavedPrompt::SavedPrompt { id } => {
                let Some(prompt) = saved_prompt_text_for_id(id, ctx) else {
                    log::warn!("Tried to insert saved prompt for id {id:?} but it does not exist");
                    return;
                };
                self.input_view.update(ctx, |input, ctx| {
                    input.set_text(&prompt, ctx);
                });
                record_saved_prompt_accepted(true, ctx);
            }
            AcceptSlashCommandOrSavedPrompt::Skill { name, .. } => {
                self.input_view.update(ctx, |input, ctx| {
                    input.set_text(&format!("/{name} "), ctx);
                });
            }
        }
        ctx.notify();
    }

    fn handle_accepted_conversation(
        &mut self,
        entry_id: AgentConversationEntryId,
        ctx: &mut ViewContext<Self>,
    ) {
        if self.is_conversation_restore_loading() {
            self.show_transient_hint(SWITCH_LOADING_HINT.to_owned(), ctx);
            return;
        }
        if !self
            .ai_context_model
            .as_ref(ctx)
            .can_start_new_conversation()
        {
            self.show_transient_hint(SWITCH_COMMAND_RUNNING_HINT.to_owned(), ctx);
            return;
        }
        let current_conversation_is_busy = self
            .conversation_selection
            .as_ref(ctx)
            .selected_conversation(ctx)
            .is_some_and(|conversation| {
                !conversation.is_empty() && !conversation.status().is_done()
            });
        if current_conversation_is_busy {
            self.show_transient_hint(SWITCH_CONVERSATION_RUNNING_HINT.to_owned(), ctx);
            return;
        }

        let Some(entry) = AgentConversationsModel::as_ref(ctx).get_entry_by_id(&entry_id, ctx)
        else {
            self.show_transient_hint(SWITCH_UNAVAILABLE_HINT.to_owned(), ctx);
            return;
        };
        if self
            .conversation_selection
            .as_ref(ctx)
            .classify_entry(&entry, ctx)
            != AgentConversationListEntryState::Available
        {
            self.show_transient_hint(SWITCH_UNAVAILABLE_HINT.to_owned(), ctx);
            return;
        }
        let target = match (
            entry.identity.local_conversation_id,
            entry.identity.server_conversation_token,
        ) {
            (Some(conversation_id), _) => TuiConversationRestoreTarget::Local(conversation_id),
            (None, Some(server_token)) => TuiConversationRestoreTarget::Server(server_token),
            (None, None) => {
                self.show_transient_hint(SWITCH_UNAVAILABLE_HINT.to_owned(), ctx);
                return;
            }
        };

        self.conversation_menu
            .update(ctx, |menu, ctx| menu.dismiss(ctx));
        self.restore_conversation(target, TuiConversationRestoreOrigin::ConversationList, ctx);
    }

    /// Forks the selected conversation and switches the surface to the fork
    /// (`/fork`). Because the TUI is single-surface, this always replaces the
    /// visible conversation — the equivalent of the GUI's `CurrentPane`
    /// destination, not its split-pane/new-tab options. The source conversation
    /// stays in history (reachable via `/conversations`). `initial_prompt`, if
    /// present, is sent to the fork after switching.
    fn fork_current_conversation(
        &mut self,
        fork_from_exchange: Option<AIAgentExchangeId>,
        post_fork: PostForkAction,
        ctx: &mut ViewContext<Self>,
    ) {
        if self.is_conversation_restore_loading() {
            self.show_transient_hint(SWITCH_LOADING_HINT.to_owned(), ctx);
            return;
        }
        if !self
            .ai_context_model
            .as_ref(ctx)
            .can_start_new_conversation()
        {
            self.show_transient_hint(SWITCH_COMMAND_RUNNING_HINT.to_owned(), ctx);
            return;
        }
        // Forking replaces the surface, so refuse while the source turn is live
        // (mirrors the conversation-switch guard).
        let current_conversation_is_busy = self
            .conversation_selection
            .as_ref(ctx)
            .selected_conversation(ctx)
            .is_some_and(|conversation| {
                !conversation.is_empty() && !conversation.status().is_done()
            });
        if current_conversation_is_busy {
            self.show_transient_hint(SWITCH_CONVERSATION_RUNNING_HINT.to_owned(), ctx);
            return;
        }
        let Some(source) = self
            .conversation_selection
            .as_ref(ctx)
            .selected_conversation(ctx)
            .cloned()
        else {
            self.show_transient_hint(FORK_REQUIRES_CONVERSATION_HINT.to_owned(), ctx);
            return;
        };
        // An empty conversation is a refusal, not a failure. Check it up front with the
        // typed validator rather than inspecting the `anyhow::Error` that
        // `fork_conversation` returns, matching the pin
        // (`42effe840:crates/warp_tui/src/terminal_session_view.rs:4413`). Without this the
        // case fell through to the generic `FORK_FAILED_HINT`, which reads like a
        // malfunction rather than a refusal.
        if let Err(ForkConversationError::EmptyConversation) =
            BlocklistAIHistoryModel::validate_fork_source(&source)
        {
            self.show_transient_hint(FORK_EMPTY_CONVERSATION_HINT.to_owned(), ctx);
            return;
        }
        // `fork_conversation[_at_exchange]` copies the tasks under a new
        // conversation id and inserts the fork into the history model in memory.
        // `/fork-from` forks up to the chosen exchange (fork_from_exact_exchange
        // = false extends through the selected response, matching the GUI).
        let fork_result = BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, ctx| {
            match fork_from_exchange {
                Some(exchange_id) => history.fork_conversation_at_exchange(
                    &source,
                    exchange_id,
                    false,
                    FORK_PREFIX,
                    ctx,
                ),
                None => history.fork_conversation(
                    &source,
                    FORK_PREFIX,
                    true, /* preserve_task_ids */
                    None,
                    ctx,
                ),
            }
        });
        let forked = match fork_result {
            Ok(forked) => forked,
            Err(error) => {
                log::error!("TUI conversation forking failed: {error}");
                self.show_transient_hint(FORK_FAILED_HINT.to_owned(), ctx);
                return;
            }
        };
        let forked_id = forked.id();
        self.replace_conversation_surface(forked, TuiConversationRestoreOrigin::Fork, ctx);
        self.show_success_hint(FORKED_HINT.to_owned(), ctx);
        // The fork is now the selected conversation, so `send_prompt` targets it.
        match post_fork {
            PostForkAction::SendPrompt(initial_prompt) => {
                if let Some(prompt) = normalize_optional_prompt(initial_prompt) {
                    self.send_prompt(prompt, ctx);
                }
            }
            PostForkAction::CompactThenPrompt(initial_prompt) => {
                // Arm the follow-up before triggering the summarize so its
                // InProgress transition is observed (see maybe_send_queued_follow_up).
                self.queued_follow_up =
                    normalize_optional_prompt(initial_prompt).map(|prompt| TuiQueuedFollowUp {
                        conversation_id: forked_id,
                        prompt,
                        seen_in_progress: false,
                    });
                self.send_prompt(
                    warp::tui_export::slash_commands::COMPACT.name.to_owned(),
                    ctx,
                );
            }
        }
    }

    /// Opens the exchange picker for `/fork-from` or `/rewind` over the selected
    /// conversation's user queries.
    fn open_exchange_menu(&mut self, action: TuiExchangeMenuAction, ctx: &mut ViewContext<Self>) {
        let Some(conversation_id) = self
            .conversation_selection
            .as_ref(ctx)
            .selected_conversation_id(ctx)
        else {
            self.show_transient_hint(EXCHANGE_MENU_REQUIRES_CONVERSATION_HINT.to_owned(), ctx);
            return;
        };
        self.exchange_menu
            .update(ctx, |menu, ctx| menu.open(action, conversation_id, ctx));
    }

    /// Routes an accepted exchange from the picker to the fork or rewind path.
    fn handle_accepted_exchange(
        &mut self,
        exchange_id: AIAgentExchangeId,
        action: TuiExchangeMenuAction,
        ctx: &mut ViewContext<Self>,
    ) {
        self.exchange_menu
            .update(ctx, |menu, ctx| menu.dismiss(ctx));
        match action {
            TuiExchangeMenuAction::ForkFrom => {
                // Fork up to the chosen exchange; no initial prompt (matches the GUI).
                self.fork_current_conversation(
                    Some(exchange_id),
                    PostForkAction::SendPrompt(None),
                    ctx,
                );
            }
            TuiExchangeMenuAction::Rewind => {
                self.rewind_to_exchange(exchange_id, ctx);
            }
        }
    }

    /// Rewinds the selected conversation back to `exchange_id`: truncates the
    /// conversation history at that point and re-renders the surface. A
    /// pre-rewind backup conversation is saved first so nothing is lost.
    ///
    /// File edits made after the rewind point are undone by writing each edit's
    /// pre-edit content back (see [`crate::tui_revert_registry`]), for edits made
    /// in the current session. Edits from a restored conversation carry no base
    /// content and cannot be reverted (same limitation as the GUI); the pre-rewind
    /// backup preserves the full history either way.
    fn rewind_to_exchange(&mut self, exchange_id: AIAgentExchangeId, ctx: &mut ViewContext<Self>) {
        let Some(conversation_id) = self
            .conversation_selection
            .as_ref(ctx)
            .selected_conversation_id(ctx)
        else {
            self.show_transient_hint(EXCHANGE_MENU_REQUIRES_CONVERSATION_HINT.to_owned(), ctx);
            return;
        };
        // Before truncating (which removes the exchanges), snapshot the applied
        // file-edit actions in chronological order — those that were recorded in
        // the revert registry this session AND resolved successfully — paired
        // with the exchange they belong to.
        let revert_candidates: Vec<(AIAgentActionId, AIAgentExchangeId)> = {
            let registered: HashSet<AIAgentActionId> = TuiFileEditRevertRegistry::as_ref(ctx)
                .action_ids_for(&conversation_id)
                .into_iter()
                .collect();
            let action_model = self.ai_action_model.as_ref(ctx);
            tui_conversation_actions_in_order(ctx, conversation_id)
                .into_iter()
                .filter(|entry| registered.contains(&entry.action_id))
                .filter(|entry| {
                    matches!(
                        action_model
                            .get_action_result(&entry.action_id)
                            .map(|result| &result.result),
                        Some(AIAgentActionResultType::RequestFileEdits(
                            RequestFileEditsResult::Success { .. }
                        ))
                    )
                })
                .map(|entry| (entry.action_id, entry.exchange_id))
                .collect()
        };
        // Interrupt any in-progress turn on this conversation before truncating.
        self.ai_controller.update(ctx, |controller, ctx| {
            controller.cancel_conversation_progress(
                conversation_id,
                CancellationReason::Reverted,
                ctx,
            );
        });
        // Save a pre-rewind backup so the full history can be recovered later.
        BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, ctx| {
            if let Some(conversation) = history.conversation(&conversation_id).cloned() {
                if let Err(error) = history.fork_conversation(
                    &conversation,
                    PRE_REWIND_PREFIX,
                    false, /* preserve_task_ids */
                    None,
                    ctx,
                ) {
                    log::warn!("Failed to save pre-rewind backup of {conversation_id}: {error}");
                }
            }
        });
        // Truncate the conversation at the chosen exchange.
        let removed_exchange_ids =
            match BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, ctx| {
                history.truncate_conversation_from_exchange(conversation_id, exchange_id, ctx)
            }) {
                Ok(removed) => removed,
                Err(error) => {
                    log::warn!(
                        "Failed to truncate conversation {conversation_id} for rewind: {error}"
                    );
                    self.show_transient_hint(REWIND_FAILED_HINT.to_owned(), ctx);
                    return;
                }
            };
        // Revert the file edits in the removed exchanges, newest-first so that
        // repeated edits to the same file unwind back to the original content.
        let mut actions_to_revert: Vec<AIAgentActionId> = revert_candidates
            .into_iter()
            .filter(|(_, exchange)| removed_exchange_ids.contains(exchange))
            .map(|(action_id, _)| action_id)
            .collect();
        actions_to_revert.reverse();
        for action_id in &actions_to_revert {
            let diffs = TuiFileEditRevertRegistry::handle(ctx).update(ctx, |registry, _| {
                registry.take_diffs(&conversation_id, action_id)
            });
            if let Some(diffs) = diffs {
                revert_file_diffs(&diffs, ctx);
            }
        }
        // Re-render the (now truncated) conversation on the surface.
        // `replace_conversation_surface` re-restores action results from the
        // truncated exchanges, so no separate action-result cleanup is needed.
        let Some(truncated) = BlocklistAIHistoryModel::as_ref(ctx)
            .conversation(&conversation_id)
            .cloned()
        else {
            self.show_transient_hint(REWIND_FAILED_HINT.to_owned(), ctx);
            return;
        };
        self.replace_conversation_surface(truncated, TuiConversationRestoreOrigin::Fork, ctx);
        self.show_success_hint(REWOUND_HINT.to_owned(), ctx);
    }

    fn handle_accepted_model(&mut self, id: &LLMId, ctx: &mut ViewContext<Self>) {
        let terminal_view_id = ctx.view_id();
        // Mirror the GUI's model picker: set the pane-level Agent-Mode LLM
        // override AND write byop_last_used_model_id. The active-model
        // resolution (`get_preferred_base_model`) prioritizes the terminal
        // override + last_used over the profile's base_model, so updating only
        // the profile (the previous behavior via `update_active_profile_base_model`)
        // was silently masked and the selection never took effect.
        LLMPreferences::handle(ctx).update(ctx, |preferences, ctx| {
            preferences.update_preferred_agent_mode_llm(id, terminal_view_id, ctx);
        });
        self.model_menu.update(ctx, |menu, ctx| menu.dismiss(ctx));
    }

    fn handle_accepted_profile(
        &mut self,
        profile_id: ClientProfileId,
        ctx: &mut ViewContext<Self>,
    ) {
        let terminal_view_id = ctx.view_id();
        // Mirror the GUI's inline profile selector: switch the active profile
        // and drop the pane LLM override so the new profile's model applies.
        tui_set_active_profile(ctx, terminal_view_id, profile_id);
        self.profile_menu.update(ctx, |menu, ctx| menu.dismiss(ctx));
    }
    fn handle_accepted_mcp_action(&mut self, action: TuiMcpAction, ctx: &mut ViewContext<Self>) {
        match action {
            TuiMcpAction::Enable(id) => {
                let request = TuiMcpManager::handle(ctx)
                    .update(ctx, |model, ctx| model.prepare_install(id, ctx));
                match request {
                    Ok(request) => {
                        self.mcp_menu.update(ctx, |menu, ctx| menu.dismiss(ctx));
                        if request.variables.is_empty() {
                            self.install_and_enable_mcp(request.id, request.name, Vec::new(), ctx);
                        } else if !self
                            .mcp_install_flow
                            .update(ctx, |flow, ctx| flow.start(request, ctx))
                        {
                            self.show_error_hint(
                                "Unable to open the MCP installation flow".to_owned(),
                                ctx,
                            );
                        }
                    }
                    Err(message) => self.show_error_hint(message, ctx),
                }
            }
            TuiMcpAction::Start(_)
            | TuiMcpAction::Stop(_)
            | TuiMcpAction::Retry(_)
            | TuiMcpAction::LogOut(_)
            | TuiMcpAction::ReopenAuthorization(_) => {
                TuiMcpManager::handle(ctx).update(ctx, |model, ctx| {
                    model.apply_action(action, ctx);
                });
            }
        }
        ctx.notify();
    }

    fn handle_accepted_mcp_install_action(
        &mut self,
        action: TuiMcpInstallFlowAction,
        ctx: &mut ViewContext<Self>,
    ) {
        match action {
            TuiMcpInstallFlowAction::ProvideValue { key, value } => {
                let result = self
                    .mcp_install_flow
                    .update(ctx, |flow, ctx| flow.apply_value(key, value, ctx));
                match result {
                    Ok(Some(completion)) => {
                        self.mcp_install_flow
                            .update(ctx, |flow, ctx| flow.dismiss(ctx));
                        self.install_and_enable_mcp(
                            completion.id,
                            completion.name,
                            completion.values,
                            ctx,
                        );
                    }
                    Ok(None) => {}
                    Err(message) => self.show_error_hint(message, ctx),
                }
            }
        }
        ctx.notify();
    }

    fn install_and_enable_mcp(
        &mut self,
        id: TuiMcpServerId,
        name: String,
        values: Vec<TuiMcpVariableValue>,
        ctx: &mut ViewContext<Self>,
    ) {
        let result = TuiMcpManager::handle(ctx).update(ctx, |manager, ctx| {
            manager.install_and_enable(id, values, ctx)
        });
        match result {
            Ok(_) => self.show_success_hint(format!("{name} installed and starting"), ctx),
            Err(message) => self.show_error_hint(message, ctx),
        }
    }

    /// Handles a mouse-click accept on the inline menu: selects the row at
    /// `index` in the active menu and dispatches the result through the same
    /// path as keyboard-based acceptance.
    fn handle_inline_menu_mouse_accept(&mut self, index: usize, ctx: &mut ViewContext<Self>) {
        let mode = self.suggestions_mode.as_ref(ctx).mode();
        let Some(menu) = active_inline_menu(&self.inline_menus, mode, ctx) else {
            return;
        };
        // Guard: only fire accept when select_by_snapshot_index confirms the
        // selection was made. The default no-op impl returns false, preventing
        // a future menu that omits the override from silently accepting
        // whatever row happened to be keyboard-selected.
        if !menu.select_by_snapshot_index(index, ctx) {
            return;
        }
        let Some(accepted) = menu.accept(ctx) else {
            return;
        };
        self.input_view.update(ctx, |input, ctx| {
            input.route_inline_menu_acceptance(accepted, ctx);
        });
    }

    /// Fills the accepted history row into the input and submits it
    /// immediately, matching the GUI's accept-from-history behavior. The menu
    /// has already closed itself, and — for a prompt or command row — has
    /// already set the input's live [`InputType`] to match the row's kind, so
    /// [`Self::handle_submitted`]'s existing shell-vs-agent routing (driven by
    /// that same input type) sends it down the right path unmodified.
    ///
    /// A command row's linked workflow data is stashed for
    /// [`Self::execute_user_command`] to attach to the resulting
    /// `ExecuteCommandEvent`, preserving workflow metadata end-to-end (issue
    /// #387).
    fn handle_accepted_prompt_and_command_history(
        &mut self,
        text: String,
        kind: TuiUpArrowHistoryItemKind,
        ctx: &mut ViewContext<Self>,
    ) {
        // The accepted row's kind — not whatever mode the composer happened to
        // be in — decides how the text is submitted. `handle_submitted` routes
        // on `is_shell_mode`, so recalling a command while the composer sits in
        // agent mode would otherwise send that command to the model as a prompt
        // instead of executing it (and vice versa for a recalled prompt).
        self.pending_history_command_workflow_data = self.input_view.update(ctx, |input, ctx| {
            input.set_text(&text, ctx);
            match kind {
                TuiUpArrowHistoryItemKind::Prompt => {
                    input.exit_shell_mode(ctx);
                    None
                }
                TuiUpArrowHistoryItemKind::Command {
                    linked_workflow_data,
                } => {
                    input.enter_shell_mode(ctx);
                    linked_workflow_data
                }
            }
        });
        self.handle_submitted(text, ctx);
    }

    /// Inserts the accepted saved prompt's query text into the input for the
    /// user to edit and submit. Unlike prompt-history (which submits
    /// immediately), this mirrors the GUI's `/prompts` flow of dropping the
    /// prompt into the composer so any `{{argument}}` placeholders can be filled
    /// in before sending. Dismiss first — it clears the input buffer — then fill.
    fn handle_accepted_prompt(&mut self, text: String, ctx: &mut ViewContext<Self>) {
        self.prompts_menu.update(ctx, |menu, ctx| menu.dismiss(ctx));
        self.input_view.update(ctx, |input, ctx| {
            input.set_text(&text, ctx);
        });
    }

    fn select_tui_slash_command(&mut self, command: &StaticCommand, ctx: &mut ViewContext<Self>) {
        match slash_command_selection_behavior(command) {
            SlashCommandSelectionBehavior::InsertCommandText(text) => {
                self.input_view.update(ctx, |input, ctx| {
                    input.set_text(&text, ctx);
                });
            }
            SlashCommandSelectionBehavior::Execute => {
                self.execute_tui_slash_command(command, None, ctx);
            }
        }
    }

    fn execute_tui_slash_command(
        &mut self,
        command: &StaticCommand,
        argument: Option<&String>,
        ctx: &mut ViewContext<Self>,
    ) {
        if !command.supports_tui() {
            log::debug!(
                "TUI slash command selection is not supported yet: {}",
                command.name
            );
            return;
        }

        match command.kind() {
            // `/clear` is a TUI-only alias for `/agent`/`/new` (see `SlashCommandKind::Clear`'s
            // doc comment): clearing the transcript and starting a new conversation.
            SlashCommandKind::Agent | SlashCommandKind::New | SlashCommandKind::Clear => {
                if !self
                    .ai_context_model
                    .as_ref(ctx)
                    .can_start_new_conversation()
                {
                    self.show_transient_hint(NEW_CONVERSATION_COMMAND_RUNNING_HINT.to_owned(), ctx);
                    return;
                }
                // Starting a new conversation abandons the current one; take its
                // whole orchestration subtree down with it rather than leaving
                // orphaned child agents (and, here, their hidden PTY sessions)
                // running with no bar left to reach them from.
                if let Some(conversation_id) = self
                    .conversation_selection
                    .as_ref(ctx)
                    .selected_conversation_id(ctx)
                {
                    TuiOrchestrationModel::handle(ctx).update(ctx, |model, ctx| {
                        model.kill_descendant_agents(conversation_id, ctx);
                    });
                }
                self.cancel_active_conversation(ctx);
                let terminal_surface_id = ctx.view_id();
                self.transcript.update(ctx, |transcript, ctx| {
                    transcript.clear_for_new_conversation(ctx);
                });
                BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, ctx| {
                    history.clear_conversations_in_terminal_view(terminal_surface_id, ctx);
                });
                self.conversation_selection.update(ctx, |selection, ctx| {
                    selection.select_new_conversation(AgentViewEntryOrigin::Tui, ctx);
                });
                if let Some(prompt) = argument
                    .map(|argument| argument.trim())
                    .filter(|argument| !argument.is_empty())
                {
                    self.send_prompt(prompt.to_owned(), ctx);
                }
                self.input_view.update(ctx, |input, ctx| input.clear(ctx));
                record_static_slash_command_accepted(command.name, true, ctx);
            }
            SlashCommandKind::Conversations => {
                self.conversation_menu
                    .update(ctx, |menu, ctx| menu.open(ctx));
                record_static_slash_command_accepted(command.name, true, ctx);
            }
            SlashCommandKind::AutoApprove => {
                self.toggle_auto_approve(true, ctx);
                self.input_view.update(ctx, |input, ctx| input.clear(ctx));
                record_static_slash_command_accepted(command.name, true, ctx);
            }
            SlashCommandKind::Statusline => {
                self.open_statusline_config(command.name, ctx);
            }
            SlashCommandKind::ResetStatusline => {
                self.reset_statusline(command.name, ctx);
            }
            SlashCommandKind::Usage => {
                // Same report as the GUI's `/usage`, off the same context-window fraction the
                // statusline entry already renders (`selected_conversation_context_usage`).
                let outcome = context_usage_report(
                    self.conversation_selection
                        .as_ref(ctx)
                        .selected_conversation(ctx),
                    ctx,
                );
                self.show_usage_cost_outcome(outcome, ctx);
                self.input_view.update(ctx, |input, ctx| input.clear(ctx));
                record_static_slash_command_accepted(command.name, true, ctx);
            }
            SlashCommandKind::Cost => {
                // Sanctioned BYOP divergence (AGENTS §5.10): upstream's TUI `/cost` toggles
                // the per-exchange response summary, whose money half is Warp's server-computed
                // `block_credits` — structurally absent here. `/cost` now reports token spend
                // at the user's own configured provider rates instead. The response-summary
                // toggle itself is kept intact (`toggle_response_summary_visibility`,
                // `TuiTerminalSessionAction::ToggleResponseSummaryVisibility`) rather than
                // deleted, so nothing is lost — it is simply no longer what `/cost` means.
                let outcome = conversation_cost_report(
                    self.conversation_selection
                        .as_ref(ctx)
                        .selected_conversation(ctx),
                    ctx,
                );
                self.show_usage_cost_outcome(outcome, ctx);
                self.input_view.update(ctx, |input, ctx| input.clear(ctx));
                record_static_slash_command_accepted(command.name, true, ctx);
            }
            SlashCommandKind::Model => {
                self.model_menu.update(ctx, |menu, ctx| menu.open(ctx));
                record_static_slash_command_accepted(command.name, true, ctx);
            }
            SlashCommandKind::ApiKeys => {
                // `/api-keys`: opens the BYOP provider key manager. Not conversation-scoped
                // (unlike /fork-from and /rewind), so no conversation-selection guard is needed.
                self.api_keys_menu.update(ctx, |menu, ctx| menu.open(ctx));
                record_static_slash_command_accepted(command.name, true, ctx);
            }
            SlashCommandKind::Profile => {
                self.profile_menu.update(ctx, |menu, ctx| menu.open(ctx));
                record_static_slash_command_accepted(command.name, true, ctx);
            }
            SlashCommandKind::Prompts => {
                self.prompts_menu.update(ctx, |menu, ctx| menu.open(ctx));
                record_static_slash_command_accepted(command.name, true, ctx);
            }
            SlashCommandKind::InvokeSkill => {
                if !FeatureFlag::ListSkills.is_enabled() {
                    return;
                }
                self.skills_menu.update(ctx, |menu, ctx| menu.open(ctx));
                record_static_slash_command_accepted(command.name, true, ctx);
            }
            SlashCommandKind::Mcp => {
                // The menu clears the input itself: it owns the line as its
                // search field for as long as it is open.
                self.mcp_menu.update(ctx, |menu, ctx| menu.open(ctx));
                record_static_slash_command_accepted(command.name, true, ctx);
            }
            SlashCommandKind::Status => {
                self.input_view.update(ctx, |input, ctx| input.clear(ctx));
                self.suggestions_mode.update(ctx, |mode, ctx| {
                    mode.set_mode(
                        TuiInputSuggestionsMode::ReadOnlyMenu(TuiReadOnlyMenuKind::Status),
                        ctx,
                    );
                });
                record_static_slash_command_accepted(command.name, true, ctx);
            }
            SlashCommandKind::Exit => {
                record_static_slash_command_accepted(command.name, true, ctx);
                ctx.terminate_app(TerminationMode::ForceTerminate, None);
            }
            SlashCommandKind::Logout => {
                record_static_slash_command_accepted(command.name, true, ctx);
                log_out_tui(ctx);
            }
            SlashCommandKind::ViewLogs => {
                self.input_view.update(ctx, |input, ctx| input.clear(ctx));
                ctx.spawn(
                    async move {
                        tokio::task::spawn_blocking(|| {
                            let path = warp_logging::create_log_bundle_zip(
                                warp_logging::LogBundleExtras::default(),
                            )?;
                            reveal_path_in_file_manager(&path);
                            Ok::<_, anyhow::Error>(path)
                        })
                        .await
                    },
                    |me, result, ctx| match result {
                        Ok(Ok(path)) => {
                            me.show_success_hint(log_bundle_success_message(&path), ctx);
                        }
                        Ok(Err(error)) => {
                            report_error!(error.context("Failed to create TUI log bundle"));
                            me.show_transient_hint(LOG_BUNDLE_FAILED_HINT.to_owned(), ctx);
                        }
                        Err(error) => {
                            report_error!(
                                anyhow::Error::new(error).context("TUI log bundle task failed")
                            );
                            me.show_transient_hint(LOG_BUNDLE_FAILED_HINT.to_owned(), ctx);
                        }
                    },
                );
                record_static_slash_command_accepted(command.name, true, ctx);
            }
            SlashCommandKind::CreateNewProject => {
                let Some(query) = argument
                    .map(|argument| argument.trim())
                    .filter(|argument| !argument.is_empty())
                else {
                    self.show_transient_hint(
                        "Please describe the project you want to create after /create-new-project"
                            .to_owned(),
                        ctx,
                    );
                    return;
                };
                self.ai_controller.update(ctx, |controller, ctx| {
                    controller.send_create_new_project_request(query.to_owned(), ctx);
                });
                self.input_view.update(ctx, |input, ctx| input.clear(ctx));
                record_static_slash_command_accepted(command.name, true, ctx);
            }
            SlashCommandKind::ExportToClipboard => {
                if let Some(conversation) = self
                    .conversation_selection
                    .as_ref(ctx)
                    .selected_conversation(ctx)
                {
                    let markdown =
                        conversation.export_to_markdown(Some(self.ai_action_model.as_ref(ctx)));
                    match copy_to_clipboard(&markdown) {
                        Ok(()) => {
                            self.show_success_hint(
                                "Conversation copied to clipboard".to_owned(),
                                ctx,
                            );
                        }
                        Err(error) => {
                            log::warn!("Failed to export TUI conversation: {error}");
                            self.show_transient_hint(COPY_FAILED_HINT.to_owned(), ctx);
                        }
                    }
                } else {
                    self.show_transient_hint("No active conversation to export".to_owned(), ctx);
                }
                self.input_view.update(ctx, |input, ctx| input.clear(ctx));
                record_static_slash_command_accepted(command.name, true, ctx);
            }
            SlashCommandKind::CopyDebuggingId => {
                let debugging_payload = self
                    .conversation_selection
                    .as_ref(ctx)
                    .selected_conversation(ctx)
                    .and_then(|conversation| conversation.debugging_server_conversation_token())
                    .map(|token| token.debugging_payload(None));
                match debugging_payload {
                    Some(debugging_payload) => match copy_to_clipboard(&debugging_payload) {
                        Ok(()) => {
                            self.show_success_hint(COPY_DEBUGGING_ID_HINT.to_owned(), ctx);
                        }
                        Err(error) => {
                            log::warn!("Failed to copy TUI debugging information: {error}");
                            self.show_error_hint(COPY_FAILED_HINT.to_owned(), ctx);
                        }
                    },
                    None => {
                        self.show_error_hint(COPY_DEBUGGING_ID_NO_TOKEN_HINT.to_owned(), ctx);
                    }
                }
                self.input_view.update(ctx, |input, ctx| input.clear(ctx));
                record_static_slash_command_accepted(command.name, true, ctx);
            }
            SlashCommandKind::ExportToFile => {
                let Some(conversation) = self
                    .conversation_selection
                    .as_ref(ctx)
                    .selected_conversation(ctx)
                else {
                    self.show_transient_hint("No active conversation to export".to_owned(), ctx);
                    return;
                };
                let title = conversation.title();
                let markdown =
                    conversation.export_to_markdown(Some(self.ai_action_model.as_ref(ctx)));
                let current_directory = self
                    .active_session
                    .as_ref(ctx)
                    .current_working_directory()
                    .cloned();
                match export_conversation_markdown(
                    current_directory.as_deref(),
                    argument.map(String::as_str),
                    title.as_deref(),
                    &markdown,
                ) {
                    Ok(export) => {
                        self.show_success_hint(export_file_success_message(&export), ctx);
                    }
                    Err(error) => {
                        let message = error.user_message();
                        let path = error.path().to_path_buf();
                        report_error!(
                            anyhow::Error::new(error)
                                .context("Failed to write TUI conversation to file"),
                            extra: { "path" => %path.display() }
                        );
                        self.show_transient_hint(message, ctx);
                    }
                }
                self.input_view.update(ctx, |input, ctx| input.clear(ctx));
                record_static_slash_command_accepted(command.name, true, ctx);
            }
            SlashCommandKind::Compact | SlashCommandKind::Plan | SlashCommandKind::Init => {
                // These are handled server-side by `SlashCommandRequest::from_query`:
                // sending the literal command (plus any argument) as a user query lets
                // the BYOP controller intercept and expand it (`/compact` → summarize,
                // `/init` → render the init-project prompt). Mirrors the GUI, which
                // routes the same text through the same `from_query` path.
                self.input_view.update(ctx, |input, ctx| input.clear(ctx));
                let command_name = command.name;
                let prompt = argument
                    .map(|argument| {
                        if argument.is_empty() {
                            command_name.to_owned()
                        } else {
                            format!("{command_name} {argument}")
                        }
                    })
                    .unwrap_or_else(|| command_name.to_owned());
                self.send_prompt(prompt, ctx);
                record_static_slash_command_accepted(command_name, true, ctx);
            }
            SlashCommandKind::CompactAnd => {
                // `/compact-and <prompt>`: summarize the conversation, then send
                // <prompt> once the summarize turn finishes. Mirrors the GUI's
                // `summarize_active_ai_conversation(prompt: None, initial_prompt)`.
                self.input_view.update(ctx, |input, ctx| input.clear(ctx));
                let Some(conversation_id) = self
                    .conversation_selection
                    .as_ref(ctx)
                    .selected_conversation_id(ctx)
                else {
                    self.show_transient_hint(
                        COMPACT_AND_REQUIRES_CONVERSATION_HINT.to_owned(),
                        ctx,
                    );
                    return;
                };
                // Arm the follow-up before triggering the summarize so the turn's
                // InProgress transition is observed (see maybe_send_queued_follow_up).
                let follow_up = argument
                    .map(String::as_str)
                    .map(str::trim)
                    .filter(|prompt| !prompt.is_empty());
                self.queued_follow_up = follow_up.map(|prompt| TuiQueuedFollowUp {
                    conversation_id,
                    prompt: prompt.to_owned(),
                    seen_in_progress: false,
                });
                // A bare "/compact" routes through SlashCommandRequest::from_query
                // to a summarize with no custom instruction (prompt: None), exactly
                // the summarize half of the GUI's /compact-and.
                self.send_prompt(
                    warp::tui_export::slash_commands::COMPACT.name.to_owned(),
                    ctx,
                );
                record_static_slash_command_accepted(command.name, true, ctx);
            }
            SlashCommandKind::Fork => {
                // `/fork [prompt]`: branch the current conversation and switch to
                // the copy, optionally seeding it with [prompt]. Mirrors the GUI's
                // /fork (input/slash_commands/mod.rs) with the single-surface
                // CurrentPane destination.
                self.input_view.update(ctx, |input, ctx| input.clear(ctx));
                self.fork_current_conversation(
                    None,
                    PostForkAction::SendPrompt(argument.cloned()),
                    ctx,
                );
                record_static_slash_command_accepted(command.name, true, ctx);
            }
            SlashCommandKind::ForkAndCompact => {
                // `/fork-and-compact [prompt]`: branch, then summarize the fork,
                // then send [prompt] after the summarize finishes. Mirrors the
                // GUI's /fork-and-compact (fork with summarize_after_fork: true).
                self.input_view.update(ctx, |input, ctx| input.clear(ctx));
                self.fork_current_conversation(
                    None,
                    PostForkAction::CompactThenPrompt(argument.cloned()),
                    ctx,
                );
                record_static_slash_command_accepted(command.name, true, ctx);
            }
            SlashCommandKind::ForkFrom => {
                // `/fork-from`: pick an earlier exchange to fork the conversation
                // from. Opens the exchange picker; the fork happens on accept.
                self.input_view.update(ctx, |input, ctx| input.clear(ctx));
                self.open_exchange_menu(TuiExchangeMenuAction::ForkFrom, ctx);
                record_static_slash_command_accepted(command.name, true, ctx);
            }
            SlashCommandKind::Rewind => {
                // `/rewind`: pick an earlier exchange to roll the conversation
                // back to. Opens the exchange picker; the rewind happens on accept.
                self.input_view.update(ctx, |input, ctx| input.clear(ctx));
                self.open_exchange_menu(TuiExchangeMenuAction::Rewind, ctx);
                record_static_slash_command_accepted(command.name, true, ctx);
            }
            SlashCommandKind::Queue => {
                // `/queue <prompt>`: if the conversation is mid-turn, hold <prompt>
                // and send it after that turn finishes; otherwise send it now.
                // Mirrors the GUI's /queue (input/slash_commands/mod.rs). The GUI
                // also renders an interactive pending block with a "Send now"
                // button; the TUI conveys the same intent with a transient hint.
                self.input_view.update(ctx, |input, ctx| input.clear(ctx));
                let Some(conversation_id) = self
                    .conversation_selection
                    .as_ref(ctx)
                    .selected_conversation_id(ctx)
                else {
                    self.show_transient_hint(QUEUE_REQUIRES_CONVERSATION_HINT.to_owned(), ctx);
                    return;
                };
                let prompt = argument
                    .map(String::as_str)
                    .map(str::trim)
                    .filter(|prompt| !prompt.is_empty());
                let Some(prompt) = prompt else {
                    self.show_transient_hint(QUEUE_REQUIRES_PROMPT_HINT.to_owned(), ctx);
                    return;
                };
                let is_in_progress = BlocklistAIHistoryModel::as_ref(ctx)
                    .conversation(&conversation_id)
                    .is_some_and(|conversation| {
                        let status = conversation.status();
                        status.is_in_progress() || status.is_blocked()
                    });
                if is_in_progress {
                    // Target turn is already running, so arm `seen_in_progress`
                    // true — the next terminal status triggers the send.
                    self.queued_follow_up = Some(TuiQueuedFollowUp {
                        conversation_id,
                        prompt: prompt.to_owned(),
                        seen_in_progress: true,
                    });
                    self.show_success_hint(QUEUE_QUEUED_HINT.to_owned(), ctx);
                } else {
                    self.send_prompt(prompt.to_owned(), ctx);
                }
                record_static_slash_command_accepted(command.name, true, ctx);
            }
            // `/orchestrate` deliberately keeps `kind() == Other` rather than getting a
            // dedicated `SlashCommandKind` (see `commands::ORCHESTRATE`'s doc comment), so
            // it is name-guarded here instead of being its own match pattern -- mirroring
            // the GUI's `orchestrate if command.name == commands::ORCHESTRATE.name` arm in
            // `execute_slash_command` (`app/src/terminal/input/slash_commands/mod.rs`).
            // TUI counterpart of the GUI's `pane_group::pane::terminal_pane::spawn_local_child_agents`:
            // spawns one hidden local child agent per `;`-separated task through
            // `TuiPaneGroup::spawn_local_child_agents` so children reach the orchestration
            // tab bar (`TuiOrchestrationModel::snapshot`) the same way the GUI's spawned
            // children reach the pill bar.
            SlashCommandKind::Other
                if command.name == warp::tui_export::slash_commands::ORCHESTRATE.name =>
            {
                let Some(parent_conversation_id) = self
                    .conversation_selection
                    .as_ref(ctx)
                    .selected_conversation_id(ctx)
                else {
                    self.show_transient_hint(
                        ORCHESTRATE_REQUIRES_CONVERSATION_HINT.to_owned(),
                        ctx,
                    );
                    return;
                };
                let Some(argument) = argument
                    .map(|argument| argument.trim())
                    .filter(|argument| !argument.is_empty())
                else {
                    self.show_transient_hint(ORCHESTRATE_REQUIRES_TASK_HINT.to_owned(), ctx);
                    return;
                };
                self.input_view.update(ctx, |input, ctx| input.clear(ctx));
                let window_id = ctx.window_id();
                let sessions = TuiSessions::handle(ctx);
                TuiPaneGroup::handle(ctx).update(ctx, |pane_group, ctx| {
                    pane_group.spawn_local_child_agents(
                        &sessions,
                        window_id,
                        parent_conversation_id,
                        argument,
                        ctx,
                    );
                });
                record_static_slash_command_accepted(command.name, true, ctx);
            }
            SlashCommandKind::NaturalLanguageDetection => {
                let enabled = !AISettings::as_ref(ctx).is_ai_autodetection_enabled(ctx);
                self.set_nld_enabled(enabled, command.name, ctx);
            }
            SlashCommandKind::VimMode => {
                self.toggle_vim_mode(command.name, ctx);
            }
            SlashCommandKind::Theme => {
                self.toggle_theme(command.name, argument.map(String::as_str), ctx);
            }
            SlashCommandKind::CloudAgent
            | SlashCommandKind::AddMcp
            | SlashCommandKind::CreateEnvironment
            | SlashCommandKind::CreateDockerSandbox
            | SlashCommandKind::EditSkill
            | SlashCommandKind::AddPrompt
            | SlashCommandKind::AddRule
            | SlashCommandKind::Edit
            | SlashCommandKind::RenameTab
            | SlashCommandKind::RenameConversation
            | SlashCommandKind::SetTabColor
            | SlashCommandKind::MoveToCloud
            | SlashCommandKind::OpenCodeReview
            | SlashCommandKind::Index
            | SlashCommandKind::OpenProjectRules
            | SlashCommandKind::OpenMcpServers
            | SlashCommandKind::OpenSettingsFile
            | SlashCommandKind::Changelog
            | SlashCommandKind::Feedback
            | SlashCommandKind::OpenRepo
            | SlashCommandKind::OpenRules
            | SlashCommandKind::Host
            | SlashCommandKind::Harness
            | SlashCommandKind::Environment
            | SlashCommandKind::Orchestrate
            | SlashCommandKind::ContinueLocally
            | SlashCommandKind::RemoteControl
            // BYOP: Zap commands with no upstream kind (e.g. /pr-comments) are not
            // TUI-executable (`supports_tui()` gates them out before this match).
            | SlashCommandKind::Other => {
                debug_assert!(
                    false,
                    "Attempted to execute GUI-only slash command in the TUI: {}",
                    command.name
                );
            }
        }
    }

    /// Mounts the `/statusline` configuration picker in place of the input
    /// box. No-ops while a blocker or an already-open picker owns the slot.
    fn open_statusline_config(&mut self, command_name: &'static str, ctx: &mut ViewContext<Self>) {
        if self.active_blocking_child(ctx).is_some() || self.statusline_config_view.is_some() {
            return;
        }
        let config = AISettings::as_ref(ctx).tui_statusline.normalized();
        let statusline_config_view =
            ctx.add_typed_action_tui_view(|ctx| TuiStatuslineConfigView::new(config, ctx));
        ctx.subscribe_to_view(&statusline_config_view, |view, _, event, ctx| {
            view.handle_statusline_config_event(event, ctx);
        });
        self.statusline_config_view = Some(statusline_config_view);
        self.input_view.update(ctx, |input, ctx| input.clear(ctx));
        self.focus_current_owner_if_active(ctx);
        record_static_slash_command_accepted(command_name, true, ctx);
        ctx.notify();
    }

    fn handle_statusline_config_event(
        &mut self,
        event: &TuiStatuslineConfigEvent,
        ctx: &mut ViewContext<Self>,
    ) {
        match event {
            TuiStatuslineConfigEvent::Saved(config) => {
                self.persist_statusline_config(config.clone(), STATUSLINE_SAVED_HINT, ctx);
            }
            TuiStatuslineConfigEvent::Cancelled => {
                self.statusline_config_view = None;
                self.focus_current_owner_if_active(ctx);
                ctx.notify();
            }
            TuiStatuslineConfigEvent::LayoutChanged => ctx.notify(),
        }
    }

    fn persist_statusline_config(
        &mut self,
        config: TuiStatuslineConfig,
        success_hint: &'static str,
        ctx: &mut ViewContext<Self>,
    ) {
        let result = AISettings::handle(ctx).update(ctx, |settings, ctx| {
            settings.tui_statusline.set_value(config.normalized(), ctx)
        });
        self.statusline_config_view = None;
        self.focus_current_owner_if_active(ctx);
        match result {
            Ok(()) => self.show_success_hint(success_hint.to_owned(), ctx),
            Err(error) => {
                log::warn!("Failed to persist the TUI statusline config: {error}");
                self.show_transient_hint(STATUSLINE_PERSISTENCE_FAILED_HINT.to_owned(), ctx);
            }
        }
    }

    /// `/reset-statusline`: restores the default item set and ordering without
    /// opening the `/statusline` picker.
    fn reset_statusline(&mut self, command_name: &'static str, ctx: &mut ViewContext<Self>) {
        self.input_view.update(ctx, |input, ctx| input.clear(ctx));
        self.persist_statusline_config(TuiStatuslineConfig::default(), STATUSLINE_RESET_HINT, ctx);
        record_static_slash_command_accepted(command_name, true, ctx);
    }

    /// Persists the natural-language-detection (NLD) setting to `enabled`, reports the
    /// toggle via telemetry, and surfaces a confirmation hint. Shared by the
    /// `/enable-natural-language-detection` and `/disable-natural-language-detection`
    /// TUI slash commands so the two execution paths stay in sync.
    fn set_nld_enabled(
        &mut self,
        enabled: bool,
        command_name: &'static str,
        ctx: &mut ViewContext<Self>,
    ) {
        self.input_view.update(ctx, |input, ctx| input.clear(ctx));
        let result = AISettings::handle(ctx).update(ctx, |settings, ctx| {
            settings
                .ai_autodetection_enabled_internal
                .set_value(enabled, ctx)
        });
        match result {
            Ok(()) => {
                record_autodetection_toggle_from_slash_command(enabled, ctx);
                let hint = if enabled {
                    NLD_ENABLED_HINT
                } else {
                    NLD_DISABLED_HINT
                };
                self.show_success_hint(hint.to_owned(), ctx);
            }
            Err(error) => {
                if enabled {
                    log::warn!("Failed to enable TUI natural language detection: {error}");
                } else {
                    log::warn!("Failed to disable TUI natural language detection: {error}");
                }
                self.show_transient_hint(NLD_PERSISTENCE_FAILED_HINT.to_owned(), ctx);
            }
        }
        record_static_slash_command_accepted(command_name, true, ctx);
    }

    /// Toggles and persists vim mode (`text_editing.vim_mode_enabled`, shared
    /// with the GUI editor's Settings > Text Editing toggle), and surfaces a
    /// confirmation hint.
    fn toggle_vim_mode(&mut self, command_name: &'static str, ctx: &mut ViewContext<Self>) {
        self.input_view.update(ctx, |input, ctx| input.clear(ctx));
        // Guard: AppEditorSettings may be absent in lightweight test contexts.
        // Without it, the toggle cannot persist, so surface a transient hint
        // instead of panicking on an unregistered singleton.
        if !ctx.has_singleton_model::<AppEditorSettings>() {
            log::warn!("TUI vim mode toggle ignored: AppEditorSettings not registered");
            self.show_transient_hint(VIM_MODE_PERSISTENCE_FAILED_HINT.to_owned(), ctx);
            record_static_slash_command_accepted(command_name, true, ctx);
            return;
        }
        let enabled = !AppEditorSettings::as_ref(ctx).vim_mode_enabled();
        let result = AppEditorSettings::handle(ctx).update(ctx, |settings, ctx| {
            settings.vim_mode.set_value(enabled, ctx)
        });
        match result {
            Ok(()) => {
                if enabled {
                    // Reset to insert mode when enabling, so the user starts
                    // in the familiar editing state.
                    self.input_view
                        .update(ctx, |input, ctx| input.reset_vim_to_insert(ctx));
                }
                let hint = if enabled {
                    VIM_MODE_ENABLED_HINT
                } else {
                    VIM_MODE_DISABLED_HINT
                };
                self.show_success_hint(hint.to_owned(), ctx);
            }
            Err(error) => {
                if enabled {
                    log::warn!("Failed to enable TUI vim mode: {error}");
                } else {
                    log::warn!("Failed to disable TUI vim mode: {error}");
                }
                self.show_transient_hint(VIM_MODE_PERSISTENCE_FAILED_HINT.to_owned(), ctx);
            }
        }
        record_static_slash_command_accepted(command_name, true, ctx);
    }

    fn toggle_theme(
        &mut self,
        command_name: &'static str,
        argument: Option<&str>,
        ctx: &mut ViewContext<Self>,
    ) {
        self.input_view.update(ctx, |input, ctx| input.clear(ctx));
        let Some(theme) = argument.and_then(|argument| argument.trim().parse::<TuiTheme>().ok())
        else {
            self.show_transient_hint(THEME_INVALID_ARGUMENT_HINT.to_owned(), ctx);
            record_static_slash_command_accepted(command_name, true, ctx);
            return;
        };
        let result = TuiThemeSettings::handle(ctx)
            .update(ctx, |settings, ctx| settings.theme.set_value(theme, ctx));
        match result {
            Ok(()) => {
                // `TuiHostTerminalBackground` is only registered by the real TUI session
                // (`session.rs::run`); it is absent in lightweight test contexts, where the
                // host background is simply unknown and `background_luminance` falls back to
                // dark (matching `TuiUiBuilder::from_app`'s identical guard).
                let terminal_background = ctx
                    .has_singleton_model::<TuiHostTerminalBackground>()
                    .then(|| TuiHostTerminalBackground::as_ref(ctx).terminal_background())
                    .flatten();
                let resolved =
                    theme.resolve_for_background(background_luminance(terminal_background));
                Appearance::handle(ctx).update(ctx, |appearance, ctx| {
                    appearance.set_theme(resolved, ctx);
                });
                let hint = match theme {
                    TuiTheme::Auto => format!(
                        "Theme set to auto mode (currently {}).",
                        TuiTheme::from(Appearance::as_ref(ctx).theme()).display_name()
                    ),
                    TuiTheme::Light | TuiTheme::Dark => {
                        format!("Theme set to {} mode.", theme.display_name())
                    }
                };
                self.show_success_hint(hint, ctx);
            }
            Err(error) => {
                log::warn!("Failed to save TUI theme selection: {error}");
                self.show_transient_hint(THEME_PERSISTENCE_FAILED_HINT.to_owned(), ctx);
            }
        }
        record_static_slash_command_accepted(command_name, true, ctx);
    }

    /// Bridges shared shell-tool executor events into terminal-manager PTY intents.
    fn handle_shell_command_executor_event(
        &mut self,
        event: &ShellCommandExecutorEvent,
        model: &Arc<FairMutex<TerminalModel>>,
        ctx: &mut ViewContext<Self>,
    ) {
        match event {
            ShellCommandExecutorEvent::ExecuteCommand { action_id, command } => {
                let Some((session_id, conversation_id)) = (|| {
                    let model = model.lock();
                    let session_id = model.block_list().active_block().session_id()?;
                    let conversation_id = BlocklistAIHistoryModel::as_ref(ctx)
                        .conversation_id_for_action(action_id, ctx.view_id())?;
                    Some((session_id, conversation_id))
                })() else {
                    log::warn!(
                        "Unable to execute TUI agent-requested command for action {action_id:?}"
                    );
                    return;
                };

                ctx.emit(TuiTerminalSessionEvent::ExecuteCommand(Box::new(
                    ExecuteCommandEvent {
                        command: command.clone(),
                        session_id,
                        workflow_id: None,
                        workflow_command: None,
                        should_add_command_to_history: true,
                        source: CommandExecutionSource::AI {
                            metadata: AgentInteractionMetadata::new_hidden(
                                action_id.clone(),
                                conversation_id,
                            ),
                        },
                    },
                )));
            }
            ShellCommandExecutorEvent::WriteToPty { input, mode } => {
                ctx.emit(TuiTerminalSessionEvent::WriteAgentInput {
                    bytes: Cow::Owned(input.to_vec()),
                    mode: *mode,
                });
            }
            ShellCommandExecutorEvent::CancelExecution => {
                ctx.emit(TuiTerminalSessionEvent::InterruptPty);
            }
            ShellCommandExecutorEvent::TransferControlToUser {
                action_id: _,
                reason,
            } => {
                let reason = reason.clone();
                self.cli_subagent_controller.update(ctx, |controller, ctx| {
                    controller.switch_control_to_user(
                        UserTakeOverReason::TransferFromAgent { reason },
                        ctx,
                    );
                });
            }
        }
    }
}

impl Entity for TuiTerminalSessionView {
    type Event = TuiTerminalSessionEvent;
}

impl TuiView for TuiTerminalSessionView {
    fn ui_name() -> &'static str {
        "TuiTerminalSessionView"
    }

    fn child_view_ids(&self, _ctx: &AppContext) -> Vec<EntityId> {
        let mut child_view_ids = vec![
            self.transcript.id(),
            self.input_view.id(),
            self.attachment_bar.id(),
            self.zero_state_view.id(),
        ];
        if let Some(statusline_config_view) = self.statusline_config_view.as_ref() {
            child_view_ids.push(statusline_config_view.id());
        }
        child_view_ids
    }

    fn keymap_context(&self, ctx: &AppContext) -> keymap::Context {
        let mut context = Self::default_keymap_context();
        if self.is_conversation_restore_loading() {
            context.set.insert(SESSION_CAN_CANCEL_RESTORE_FLAG);
        }
        if self.active_user_controlled_target(ctx).is_some() {
            context.set.insert(SESSION_CAN_HAND_BACK_CONTROL_FLAG);
        }
        if self.can_attach_agent_to_running_command() {
            context
                .set
                .insert(SESSION_CAN_ATTACH_AGENT_TO_RUNNING_COMMAND_FLAG);
        }
        if self
            .terminal_model
            .lock()
            .block_list()
            .active_block()
            .is_agent_tagged_in()
            && self.input_target().agent_editor_owns_input()
            && !self.suggestions_mode.as_ref(ctx).mode().is_visible()
        {
            context
                .set
                .insert(SESSION_CAN_DETACH_AGENT_FROM_RUNNING_COMMAND_FLAG);
        }
        if self.active_agent_blocked_target(ctx).is_some() {
            context
                .set
                .insert(SESSION_CAN_ALLOW_BLOCKED_LRC_ACTION_FLAG);
            context
                .set
                .insert(SESSION_CAN_REJECT_BLOCKED_LRC_ACTION_FLAG);
        }
        if self.transcript.as_ref(ctx).has_toggleable_plan(ctx) {
            context.set.insert(PLAN_TOGGLE_AVAILABLE_FLAG);
        }
        if self.keyboard_enhancement_supported {
            context.set.insert(KEYBOARD_ENHANCEMENT_AVAILABLE_FLAG);
        }
        if self.input_target().agent_editor_owns_input()
            && !self.suggestions_mode.as_ref(ctx).mode().is_visible()
        {
            context.set.insert(SESSION_COMPOSER_SHORTCUTS_ACTIVE_FLAG);
            if attachment_focus_available(
                self.is_shell_mode(ctx),
                self.attachment_bar.as_ref(ctx).should_render(ctx),
            ) {
                context.set.insert(ATTACHMENTS_AVAILABLE_FLAG);
            }
        }
        if self.orchestration_tabs_focused {
            context.set.insert(ORCHESTRATION_TAB_BAR_FOCUSED_FLAG);
        }
        context
    }

    fn render(&self, ctx: &AppContext) -> Box<dyn TuiElement> {
        match &self.conversation_restore_state {
            ConversationRestoreState::Loading {
                origin: TuiConversationRestoreOrigin::Startup,
                ..
            } => return conversation_restoring(ctx),
            ConversationRestoreState::Loading {
                origin:
                    TuiConversationRestoreOrigin::ConversationList | TuiConversationRestoreOrigin::Fork,
                ..
            } => {}
            ConversationRestoreState::Failed(message) => {
                return conversation_restore_failed(message);
            }
            ConversationRestoreState::Idle => {}
        }
        let builder = TuiUiBuilder::from_app(ctx);
        // While a full-screen (alt-screen) app is active, hand the pane to it:
        // render its grid instead of the block UI.
        let (alt_screen_active, input_target, user_owns_running_command) = {
            let terminal_model = self.terminal_model.lock();
            (
                terminal_model.is_alt_screen_active(),
                tui_input_target(&terminal_model),
                inline_process_owns_input(&terminal_model),
            )
        };
        if alt_screen_active {
            self.zero_state_interaction.set_visible(false);
            let terminal_content = TuiTerminalContentElement::new(
                self.terminal_resize_tx.clone(),
                AltScreenElement::new(self.terminal_model.clone()).finish(),
            );
            // Only a full-screen app the USER is driving gets keystrokes forwarded.
            // Under agent control the composer owns input, so wiring the PTY here
            // would race the agent for every key.
            let terminal_content = if input_target.pty_owns_input() {
                terminal_content.with_pty_input(self.terminal_model.clone())
            } else {
                terminal_content
            };
            let mut content = TuiFlex::column().flex_child(terminal_content.finish());
            // A user-controlled full-screen app still advertises manual agent
            // attachment, matching the non-alt-screen hint row below. Gated on
            // the same user-controlled-command predicate: the composer isn't
            // rendered here, so this is the only attach affordance visible
            // while the alternate screen owns the pane. Mirrors the pin's
            // alt-screen branch in this same function (`02b53fcd8`).
            if input_target.pty_owns_input()
                && user_owns_running_command
                && let Some(hint) = self.running_command_hint(ctx)
            {
                content = content.child(
                    TuiContainer::new(
                        TuiText::new(hint)
                            .with_style(builder.muted_text_style())
                            .truncate()
                            .finish(),
                    )
                    .with_padding_x(2)
                    .with_padding_bottom(1)
                    .finish(),
                );
            }
            // ...and only a user-driven app gets the WHOLE pane. When the agent is
            // driving the full-screen command the user still has to be able to talk
            // to it, so the alternate screen takes the output region and the normal
            // composer chrome renders beneath it -- as the pin's alt-screen branch
            // in this same function does. Handing the pane over unconditionally, as
            // this fork did, left no way to reach the agent until the app exited.
            if input_target.agent_editor_owns_input() {
                let agent_area = self.append_composer_and_footer(TuiFlex::column(), &builder, ctx);
                content = content.child(
                    TuiContainer::new(agent_area.finish())
                        .with_padding_x(2)
                        .with_padding_bottom(1)
                        .finish(),
                );
            }
            return content.finish();
        }

        let inline_menu = input_target
            .agent_editor_owns_input()
            .then(|| {
                active_inline_menu(
                    &self.inline_menus,
                    self.suggestions_mode.as_ref(ctx).mode(),
                    ctx,
                )
                .and_then(|menu| {
                    menu.render_with_interaction(
                        ctx,
                        |index, event_ctx, _| {
                            event_ctx.dispatch_typed_action(
                                TuiTerminalSessionAction::InlineMenuMouseAcceptRow(index),
                            );
                        },
                        |delta, event_ctx, _| {
                            event_ctx.dispatch_typed_action(
                                TuiTerminalSessionAction::InlineMenuMouseScrollBy(delta),
                            );
                        },
                    )
                })
            })
            .flatten();
        // Ctrl-c (cancel/clear/exit) is handled by the keymap pass via the
        // fixed binding registered in [`Self::init`], so no element-level key
        // handling is needed here.
        //
        // While the transcript has nothing to show, the zero state fills its
        // slot; the first accepted submission produces a visible block, which
        // swaps the transcript back in.
        let mut content = TuiFlex::column();
        let transcript_is_empty = self.transcript.as_ref(ctx).is_empty();
        self.zero_state_interaction.set_visible(transcript_is_empty);
        if transcript_is_empty {
            content = content.flex_child(TuiChildView::new(&self.zero_state_view).finish());
        } else {
            content = content.flex_child(TuiChildView::new(&self.transcript).finish());
        }

        // While a `RunAgents` card (or another blocking interaction) is the
        // active front-of-queue blocker, the input box, inline menus, normal
        // footer, and the warping/summary row are omitted; the blocker
        // renders its own action hints in their place. Visibility is derived
        // fresh each pass — no stored suppression flag — and the hidden
        // input model is never written to, so its draft/cursor/selection/
        // scroll survive untouched.
        let blocker_active = self.active_blocking_child(ctx).is_some();
        if !blocker_active && matches!(input_target, TuiInputTarget::Disabled) {
            content = content.child(
                TuiContainer::new(
                    TuiText::new(STARTING_SHELL_HINT)
                        .with_style(builder.muted_text_style())
                        .truncate()
                        .finish(),
                )
                .with_padding_top(1)
                .finish(),
            );
        }

        // While the selected conversation is in progress (the GUI warping
        // indicator's core condition), the animated warping indicator sits
        // between the transcript and the input box. Hide it while a process
        // owns input or a blocker is active: user takeover intentionally leaves
        // the conversation in progress, and blockers render their own status
        // and actions. Its elapsed counter is anchored to the latest exchange's
        // start so animation survives element-tree rebuilds; the conversation's
        // final status update re-renders the view without it.
        let selected_conversation = self
            .conversation_selection
            .as_ref(ctx)
            .selected_conversation_id(ctx)
            .and_then(|conversation_id| {
                BlocklistAIHistoryModel::as_ref(ctx).conversation(&conversation_id)
            })
            .filter(|_| {
                !blocker_active
                    && self.statusline_config_view.is_none()
                    && input_target.agent_editor_owns_input()
            });
        if let Some(conversation) = selected_conversation {
            if conversation.status().is_in_progress() {
                let warping_elapsed = conversation
                    .latest_exchange()
                    .and_then(|exchange| exchange.time_since_start());
                if let Some(elapsed) = warping_elapsed {
                    let label = if conversation.is_summarizing() {
                        "Summarizing conversation"
                    } else {
                        "Phosphorizing"
                    };
                    content = content.child(
                        TuiContainer::new(self.render_warping_indicator(
                            label,
                            elapsed,
                            conversation.id(),
                            ctx,
                        ))
                        .with_padding_top(1)
                        .finish(),
                    );
                }
            } else {
                // Once the response completes, the indicator's slot rests on
                // the last response's summary: `∷ {duration} • {credits}`.
                // Wall-to-wall duration is only available once the block's
                // final exchange finished, which also keeps the row hidden
                // for brand-new conversations.
                let wall_to_wall = conversation
                    .wall_to_wall_response_time_since_last_query()
                    .and_then(|ms| u64::try_from(ms).ok())
                    .map(Duration::from_millis);
                if let (Some(duration), Some(exchange_id)) = (
                    wall_to_wall,
                    conversation.latest_exchange().map(|exchange| exchange.id),
                ) && let Some(summary) = self.render_response_summary_for_exchange(
                    exchange_id,
                    duration,
                    conversation.credits_spent_for_last_block(),
                    ctx,
                ) {
                    content =
                        content.child(TuiContainer::new(summary).with_padding_top(1).finish());
                }
            }
        }
        // While a user-controlled long-running command owns input, the input
        // box and footer stay hidden; a one-line ghosted hint row takes the
        // input's slot when manual attachment is available. Gated on the
        // user-controlled-command predicate, not the broader PTY input
        // target: visible startup-script execution also routes input to the
        // PTY but does not support agent attachment. (Agent-driven terminal
        // use keeps the composer, and its control hints come from the
        // CLI-subagent status line.)
        if !blocker_active
            && user_owns_running_command
            && let Some(hint) = self.running_command_hint(ctx)
        {
            content = content.child(
                TuiContainer::new(
                    TuiText::new(hint)
                        .with_style(builder.muted_text_style())
                        .truncate()
                        .finish(),
                )
                .with_padding_top(1)
                .finish(),
            );
        }
        if !blocker_active
            && let Some(statusline_config_view) = self.statusline_config_view.as_ref()
        {
            content = content.child(
                TuiContainer::new(TuiChildView::new(statusline_config_view).finish())
                    .with_padding_top(1)
                    .finish(),
            );
        }
        if !blocker_active
            && self.statusline_config_view.is_none()
            && (input_target.agent_editor_owns_input()
                || matches!(input_target, TuiInputTarget::Disabled))
        {
            if let (true, Some(menu)) = (input_target.agent_editor_owns_input(), inline_menu) {
                content = content.child(
                    TuiConstrainedBox::new(
                        TuiContainer::new(menu)
                            .with_padding_top(INLINE_MENU_TOP_PADDING_ROWS)
                            .finish(),
                    )
                    .with_max_rows(MAX_INLINE_MENU_ROWS + INLINE_MENU_TOP_PADDING_ROWS)
                    .finish(),
                );
            }
            if let Some(kind) = self.suggestions_mode.as_ref(ctx).mode().read_only_menu() {
                let menu = match kind {
                    TuiReadOnlyMenuKind::Shortcuts => {
                        self.session_state.resolve(ctx).ok().map(|state| {
                            let keymap_context = self.keymap_context(ctx);
                            shortcuts::menu(&state, &keymap_context, &builder, ctx)
                        })
                    }
                    TuiReadOnlyMenuKind::Status => {
                        Some(status_menu::menu(self.compute_status_info(ctx), &builder))
                    }
                    TuiReadOnlyMenuKind::Todos => self
                        .conversation_selection
                        .as_ref(ctx)
                        .selected_conversation(ctx)
                        .and_then(|conversation| {
                            todo_menu::active_todo_menu(conversation, &builder)
                        }),
                };
                if let Some(menu) = menu {
                    let menu = menu.render_with_viewport(
                        self.read_only_menu_selection.clone(),
                        self.read_only_menu_viewport.clone(),
                        &builder,
                        |event_ctx, _| {
                            event_ctx.dispatch_typed_action(
                                TuiTerminalSessionAction::ReadOnlyMenuSelectionStarted,
                            );
                        },
                        |text, event_ctx, _| {
                            event_ctx.dispatch_typed_action(
                                TuiTerminalSessionAction::ReadOnlyMenuSelectionEnded(text),
                            );
                        },
                    );
                    content = content.child(
                        TuiConstrainedBox::new(
                            TuiContainer::new(menu)
                                .with_padding_top(INLINE_MENU_TOP_PADDING_ROWS)
                                .finish(),
                        )
                        .with_max_rows(MAX_READ_ONLY_MENU_ROWS + INLINE_MENU_TOP_PADDING_ROWS)
                        .finish(),
                    );
                }
            }
            content = self.append_composer_and_footer(content, &builder, ctx);
        }
        let content = content.finish();
        let terminal_content =
            TuiTerminalContentElement::new(self.terminal_resize_tx.clone(), content);
        let terminal_content = if input_target.pty_owns_input() {
            terminal_content.with_pty_input(self.terminal_model.clone())
        } else {
            terminal_content
        };

        // The terminal-content wrapper sits inside the horizontal padding so
        // the PTY's columns match the width block content actually renders at
        // (the GUI wraps its view root, but its padding is sub-cell; here it is
        // 4 whole columns).
        TuiContainer::new(terminal_content.finish())
            .with_padding_x(2)
            .with_padding_top(2)
            .with_padding_bottom(1)
            .finish()
    }
}

impl TuiTerminalSessionView {
    /// Ghosted hint advertising the live keybinding that manually attaches
    /// the agent to a user-controlled long-running command. Ported from the
    /// pin's `running_command_hint` (`02b53fcd8`).
    fn running_command_hint(&self, ctx: &AppContext) -> Option<String> {
        let context = self.keymap_context(ctx);
        let attach_key = binding_hint(ATTACH_AGENT_TO_RUNNING_COMMAND_BINDING_NAME, &context, ctx);
        input_hints::long_running_command_hint(attach_key.as_deref())
    }

    /// Appends the agent composer's chrome -- attachment bar, input box, footer --
    /// to `content`.
    ///
    /// Shared by the normal block UI and the agent-controlled alternate-screen path
    /// so the two cannot drift: the composer a user sees under a full-screen agent
    /// command is the same composer, not an approximation of it.
    fn append_composer_and_footer(
        &self,
        mut content: TuiFlex,
        builder: &TuiUiBuilder,
        ctx: &AppContext,
    ) -> TuiFlex {
        let border_style = if self.is_shell_mode(ctx) {
            builder.shell_mode_accent_style()
        } else {
            builder.accent_border_style()
        };
        if self.attachment_bar.as_ref(ctx).should_render(ctx) {
            content = content.child(
                TuiConstrainedBox::new(
                    TuiContainer::new(TuiChildView::new(&self.attachment_bar).finish())
                        .with_padding_x(1)
                        .finish(),
                )
                .with_max_rows(1)
                .finish(),
            );
        }
        content = content.child(
            TuiConstrainedBox::new(
                TuiContainer::new(TuiChildView::new(&self.input_view).finish())
                    .with_padding_x(1)
                    .with_padding_y(1)
                    .with_border_style(border_style)
                    .finish(),
            )
            .with_max_rows(MAX_INPUT_TEXT_ROWS + BORDERED_INPUT_CHROME_ROWS)
            .finish(),
        );
        let footer = if self.orchestration_tabs_focused {
            self.render_orchestration_tab_footer(builder, ctx)
        } else {
            self.render_footer(ctx).finish()
        };
        content.child(TuiConstrainedBox::new(footer).with_max_rows(1).finish())
    }

    /// Footer shown while orchestration tabs own keyboard focus.
    ///
    /// The dispatch above is new here, not part of the ported commit: this
    /// fork built `render_orchestration_tab_footer` but never routed the
    /// composer footer through it, so while the `Agents:` bar had focus the
    /// row still showed the ordinary session footer. The pin routes it in
    /// `append_composer_and_footer`'s equivalent; without the routing the
    /// kill hint below would be unreachable.
    fn render_orchestration_tab_footer(
        &self,
        builder: &TuiUiBuilder,
        ctx: &AppContext,
    ) -> Box<dyn TuiElement> {
        // Show the kill hint when a killable tab is selected -- a level child
        // or the drilled-in anchor -- so the user knows that a single ctrl-c
        // will terminate that agent (naming its nested blast radius when it
        // orchestrates a subtree).
        if let Some((_, nested_descendants)) = self.bar_focused_kill_target(ctx) {
            render_orchestration_child_selected_tab_footer(builder, nested_descendants)
        } else {
            render_orchestration_tab_footer(builder)
        }
    }

    fn handle_typeahead_event(&mut self, ctx: &mut ViewContext<Self>) {
        let typeahead = self.terminal_model.lock().take_typeahead_for_input();
        if let Some((text, previously_inserted)) = typeahead {
            self.input_view.update(ctx, |input, ctx| {
                input.insert_typeahead_text(previously_inserted, &text, ctx);
            });
        }
        ctx.notify();
    }

    fn forward_user_pty_bytes(&self, bytes: &[u8], ctx: &mut ViewContext<Self>) {
        // Raw passthrough: the bytes are already the app's escape sequence.
        // Recheck control at the final write boundary in case the element
        // tree predates an agent takeover, so stale bytes typed before the
        // agent tagged in or took control over the running command are
        // dropped instead of leaking into the PTY after control changed hands.
        let composer_owns_input = self
            .terminal_model
            .lock()
            .block_list()
            .active_block()
            .is_agent_in_control_or_tagged_in();
        if composer_owns_input {
            return;
        }
        ctx.emit(TuiTerminalSessionEvent::WriteUserInput(Cow::Owned(
            bytes.to_vec(),
        )));
    }

    /// Types a prepared local child-agent harness command into this session's
    /// PTY and presses Enter, as though the user had typed it. Mirrors the
    /// GUI's `TerminalView::start_local_child_harness_process`
    /// (`app/src/terminal/view/agent_view.rs`); driven by
    /// [`crate::pane_group::TuiPaneGroup`] for a session created hidden
    /// (unfocused) specifically to run the command in the background.
    pub(crate) fn write_child_harness_command(
        &mut self,
        command: &str,
        ctx: &mut ViewContext<Self>,
    ) {
        ctx.emit(TuiTerminalSessionEvent::WriteUserInput(Cow::Owned(
            command.as_bytes().to_vec(),
        )));
        ctx.emit(TuiTerminalSessionEvent::WriteUserInput(Cow::Owned(vec![
            b'\r',
        ])));
    }
}

impl TypedActionView for TuiTerminalSessionView {
    type Action = TuiTerminalSessionAction;

    fn handle_action(&mut self, action: &TuiTerminalSessionAction, ctx: &mut ViewContext<Self>) {
        match action {
            TuiTerminalSessionAction::Interrupt => self.handle_interrupt(ctx),
            TuiTerminalSessionAction::Eof => self.handle_eof(ctx),
            TuiTerminalSessionAction::CancelRestore => {
                self.cancel_conversation_restore(ctx);
            }
            TuiTerminalSessionAction::HandBackTerminalUseControl => {
                self.hand_back_terminal_use_control(ctx)
            }
            TuiTerminalSessionAction::AttachAgentToRunningCommand => {
                let _ = self.try_attach_agent_to_running_command(ctx);
            }
            TuiTerminalSessionAction::DetachAgentFromRunningCommand => {
                let _ = self.try_detach_agent_from_running_command(ctx);
            }
            TuiTerminalSessionAction::AllowBlockedLrcAction => self.allow_blocked_lrc_action(ctx),
            TuiTerminalSessionAction::RejectBlockedLrcAction => self.reject_blocked_lrc_action(ctx),
            TuiTerminalSessionAction::ToggleResponseSummaryVisibility => {
                self.toggle_response_summary_visibility(ctx)
            }
            TuiTerminalSessionAction::ToggleTodoMenu => self.toggle_todo_menu(ctx),
            TuiTerminalSessionAction::ToggleModelMenu => self.toggle_model_menu(ctx),
            TuiTerminalSessionAction::ToggleAutoApprove { show_feedback } => {
                self.toggle_auto_approve(*show_feedback, ctx)
            }
            TuiTerminalSessionAction::OpenUrl(url) => ctx.open_url(url),
            TuiTerminalSessionAction::ForwardUserPtyBytes(bytes) => {
                self.forward_user_pty_bytes(bytes, ctx);
            }
            TuiTerminalSessionAction::TogglePlan => {
                self.transcript
                    .update(ctx, |transcript, ctx| transcript.toggle_latest_plan(ctx));
            }
            TuiTerminalSessionAction::FocusAttachments => {
                if self.attachment_bar.as_ref(ctx).should_render(ctx) {
                    ctx.focus(&self.attachment_bar);
                }
            }
            TuiTerminalSessionAction::PasteFromClipboard => {
                self.attachment_bar
                    .update(ctx, |bar, ctx| bar.paste_from_clipboard(ctx));
            }
            TuiTerminalSessionAction::TriggerCompletions => {
                self.request_shell_completion(ctx);
            }
            TuiTerminalSessionAction::InlineMenuMouseAcceptRow(index) => {
                self.handle_inline_menu_mouse_accept(*index, ctx);
            }
            TuiTerminalSessionAction::InlineMenuMouseScrollBy(delta) => {
                let mode = self.suggestions_mode.as_ref(ctx).mode();
                if let Some(menu) = active_inline_menu(&self.inline_menus, mode, ctx) {
                    menu.scroll_by_delta(*delta, ctx);
                    ctx.notify();
                }
            }
            TuiTerminalSessionAction::ReadOnlyMenuSelectionStarted => {
                self.transcript
                    .update(ctx, |transcript, ctx| transcript.clear_selection(ctx));
                self.input_view
                    .update(ctx, |input, ctx| input.clear_selection(ctx));
            }
            TuiTerminalSessionAction::ReadOnlyMenuSelectionEnded(text) => {
                match copy_to_clipboard(text) {
                    Ok(()) => self.show_copy_hint(ctx),
                    Err(error) => {
                        log::warn!("Failed to copy TUI read-only menu selection: {error}");
                        self.show_transient_hint(COPY_FAILED_HINT.to_owned(), ctx);
                    }
                }
            }
            TuiTerminalSessionAction::FocusDefaultInteractionTarget => {
                self.set_orchestration_tab_focus(false, ctx)
            }
            TuiTerminalSessionAction::FocusMainOrchestrationTab => {
                let root_key = self.orchestration_tab_bar.as_ref(ctx).tree_root_key();
                if let Some(key) = root_key {
                    self.switch_to_orchestration_tab(Some(key), false, ctx);
                } else {
                    self.set_orchestration_tab_focus(false, ctx);
                }
            }
            TuiTerminalSessionAction::NavigateOrchestrationTabs(action) => {
                let key = action.target(self.orchestration_tab_bar.as_ref(ctx), ctx);
                self.switch_to_orchestration_tab(key, true, ctx);
            }
        }
    }
}

impl TerminalSurface for TuiTerminalSessionView {
    fn on_shell_determined(&mut self, ctx: &mut ViewContext<Self>) {
        ctx.notify();
    }

    fn on_pty_spawn_failed(&mut self, error: anyhow::Error, ctx: &mut ViewContext<Self>) {
        report_error!(error.context("TUI PTY spawn failed"));
        ctx.notify();
    }
}

#[cfg(test)]
#[path = "terminal_session_view_tests.rs"]
mod tests;
