//! Authenticated terminal-session TUI surface.
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use async_channel::Sender;
use instant::Instant;
use parking_lot::FairMutex;
use warp::editor::{CodeEditorModel, CodeEditorModelEvent};
use warp::settings::{
    AISettings, AISettingsChangedEvent, AppEditorSettings, TuiStatuslineConfig, TuiStatuslineItem,
};
use warp::tui_export::{
    AIAgentActionId, AIAgentActionResultType, AIAgentContext, AIAgentExchangeId,
    AIAgentPtyWriteMode, AIConversation, AIConversationId, AcceptSlashCommandOrSavedPrompt,
    ActiveSession, ActiveSessionEvent, AgentConversationEntryId, AgentConversationListEntryState,
    AgentConversationsModel, AgentInteractionMetadata, AgentViewEntryOrigin, BlockId,
    BlocklistAIActionEvent, BlocklistAIActionModel, BlocklistAIContextModel, BlocklistAIController,
    BlocklistAIHistoryEvent, BlocklistAIHistoryModel, BlocklistAIInputModel, CLISubagentController,
    CLISubagentEvent, CLISubagentTarget, COMMAND_REGISTRY, FORK_PREFIX, PRE_REWIND_PREFIX,
    CancellationReason, ChangelogModel, tui_conversation_actions_in_order,
    ChangelogRequestType, LoadedConversationData, CommandExecutionSource, ConversationFileExport,
    ConversationSelection, ConversationSelectionHandle,
    ExecuteCommandEvent, GitRepoModels, GitRepoStatusModel,
    GitStatusMetadata, LLMId, LLMPreferences, LLMPreferencesEvent,
    LOCAL_SKILLS_REMOTE_EXECUTION_ERROR_MESSAGE, ModelEvent, ParsedSlashCommandInput,
    PersistenceWriter, PtyIntent, PtyIntentEvent, RepoDetectionSessionType, RepoDetectionSource,
    ServerConversationToken, ShellCommandExecutorEvent, SizeInfo, SizeUpdate, SkillReference,
    SlashCommandKind, SlashCommandSelectionBehavior,
    StaticCommand, TerminalModel, TerminalSurface,
    AgentViewState, TerminalSurfaceInit, TuiMcpAction, TuiMcpManager, TuiSlashCommandDataSource,
    TuiSlashCommandDataSourceArgs, TuiZeroStateDataSource, UserTakeOverReason,
    WAKEUP_THROTTLE_PERIOD, block_context_from_terminal_model, build_slash_command_mixer,
    detect_possible_git_repo, export_conversation_markdown, log_out_tui,
    maybe_build_ai_query_upsert_event, prepare_conversation_block_restoration,
    record_autodetection_toggle_from_slash_command, record_saved_prompt_accepted,
    record_static_slash_command_accepted, saved_prompt_text_for_id,
    slash_command_selection_behavior, throttle,
    tui_completion_session_context, tui_fetch_completions,
    ClientProfileId, tui_set_active_profile,
};
use warp_core::features::FeatureFlag;
use warp_core::settings::Setting;
use warp_editor::model::CoreEditorModel;
use crate::report_error::report_error;
use warp_util::local_or_remote_path::LocalOrRemotePath;
use warpui::SingletonEntity;
use warpui_core::r#async::{SpawnedFutureHandle, Timer};
use warpui_core::elements::MouseStateHandle;
use warpui_core::elements::tui::{
    TuiChildView, TuiConstrainedBox, TuiContainer, TuiElement, TuiFlex, TuiHoverable, TuiSize,
    TuiText,
};
use warpui_core::keymap::macros::*;
use warpui_core::keymap::{self, EditableBinding, FixedBinding};
use warpui_core::platform::TerminationMode;
use warpui_core::{
    AppContext, Entity, EntityId, ModelHandle, TuiView, TypedActionView, ViewContext, ViewHandle,
};

use crate::agent_block::TuiBlockingChild;
use crate::alt_screen_view::AltScreenElement;
use crate::attachment_bar::{
    FOCUS_ATTACHMENTS_BINDING_NAME, TuiAttachmentBar, TuiAttachmentBarEvent, TuiAttachmentModel,
    TuiAttachmentPasteDisposition,
};
use crate::clipboard::copy_to_clipboard;
use crate::conversation_menu::{TuiConversationMenuEvent, TuiConversationMenuModel};
use crate::conversation_selection::TuiConversationSelection;
use crate::editor_interaction::TuiEditorCommand;
use crate::exit_confirmation::{CTRL_C_EXIT_WINDOW, ExitConfirmation};
use crate::inline_menu::{MAX_INLINE_MENU_ROWS, TuiInlineMenu, active_inline_menu};
use crate::input::view::TuiInputAction;
use crate::input::{TuiInputView, TuiInputViewEvent};
use crate::input_hints;
use crate::input_mode_policy::{self, TuiInputModePolicy};
use crate::input_suggestions_mode::TuiInputSuggestionsModeModel;
use crate::keybindings::{
    ATTACHMENTS_AVAILABLE_FLAG, CONTEXTUAL_PLAN_TOGGLE_BINDING_NAME,
    KEYBOARD_ENHANCEMENT_AVAILABLE_FLAG, PLAN_TOGGLE_AVAILABLE_FLAG, PLAN_TOGGLE_BINDING_NAME,
    TUI_BINDING_GROUP,
};
use crate::mcp_menu::{TuiMcpMenuEvent, TuiMcpMenuModel};
use crate::completions_menu::{
    TuiAcceptedCompletion, TuiCompletionsMenuEvent, TuiCompletionsMenuModel,
};
use ai::agent::action_result::RequestFileEditsResult;

use crate::api_keys_menu::{TuiApiKeysMenuEvent, TuiApiKeysMenuModel};
use crate::exchange_menu::{TuiExchangeMenuAction, TuiExchangeMenuEvent, TuiExchangeMenuModel};
use crate::tui_diff_storage::revert_file_diffs;
use crate::tui_revert_registry::TuiFileEditRevertRegistry;
use crate::profile_menu::{TuiProfileMenuEvent, TuiProfileMenuModel};
use crate::prompts_menu::{TuiPromptsMenuEvent, TuiPromptsMenuModel};
use crate::model_menu::{TuiModelMenuEvent, TuiModelMenuModel};
use crate::platform::reveal_path_in_file_manager;
use crate::prompt_history_menu::{TuiPromptHistoryMenuEvent, TuiPromptHistoryMenuModel};
use crate::resume::TuiExitSummaryHandle;
use crate::session_registry::TuiSessions;
use crate::skills_menu::{TuiSkillMenuEvent, TuiSkillMenuModel};
use crate::slash_commands::TuiSlashCommandModel;
use crate::statusline_config_view::{TuiStatuslineConfigEvent, TuiStatuslineConfigView};
use crate::terminal_content_element::TuiTerminalContentElement;
use crate::terminal_use::{
    TerminalUseInterruptAction, TuiInputTarget, hide_agent_requested_command_from_top_level,
    inline_process_owns_input, terminal_use_conversation_to_resume, terminal_use_interrupt_action,
    tui_input_target,
};
use crate::transcript_view::{TuiTranscriptView, TuiTranscriptViewEvent};
use crate::transient_hint::{TransientHint, TransientHintTone};
use crate::tui_builder::TuiUiBuilder;
use crate::tui_cli_subagent_view::{
    ALLOW_BLOCKED_ACTION_KEY_BINDING, HAND_BACK_KEY_BINDING, REJECT_BLOCKED_ACTION_KEY_BINDING,
    TuiCLISubagentView,
};
use crate::ui::{compact_footer_path, conversation_restore_failed, conversation_restoring};
use crate::usage::render_context_usage_entry;
use crate::warping_indicator::{render_response_summary, render_warping_indicator_row};
use crate::zero_state::TuiZeroStateView;
mod input_detection;

use self::input_detection::InputDetectionState;

/// Width used before the first layout pass pushes the real terminal width into the editor.
const INITIAL_INPUT_WIDTH: u16 = 80;
const INLINE_MENU_TOP_PADDING_ROWS: u16 = 1;
const MAX_INPUT_TEXT_ROWS: u16 = 6;
const AUTO_APPROVE_FEEDBACK_DURATION: Duration = Duration::from_secs(3);

/// The footer hint shown while the ctrl-c exit confirmation is armed.
const CTRL_C_EXIT_HINT: &str = "ctrl-c again to exit";
const STARTING_SHELL_HINT: &str = "Starting shell...";
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
pub(crate) const SESSION_COMPOSER_OWNS_INPUT_FLAG: &str = "TuiSessionComposerOwnsInput";
pub(crate) const TRIGGER_COMPLETIONS_BINDING_NAME: &str = "tui:session:trigger_completions";
pub(crate) const PASTE_IMAGE_BINDING_NAME: &str = "tui:session:paste_image";
pub(crate) const AUTO_APPROVE_TOGGLE_BINDING_NAME: &str = "tui:session:toggle_auto_approve";

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
const COMPACT_AND_REQUIRES_CONVERSATION_HINT: &str = "/compact-and requires an active conversation";
const QUEUE_REQUIRES_CONVERSATION_HINT: &str = "/queue requires an active conversation";
const QUEUE_REQUIRES_PROMPT_HINT: &str = "/queue requires a prompt argument";
const QUEUE_QUEUED_HINT: &str = "Queued — will send when the current turn finishes";
const FORK_REQUIRES_CONVERSATION_HINT: &str = "/fork requires an active conversation";
const FORK_FAILED_HINT: &str = "Failed to fork the conversation";
const FORKED_HINT: &str = "Forked conversation";
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
const STATUSLINE_PERSISTENCE_FAILED_HINT: &str = "Could not save the statusline configuration.";
const COPY_SELECTION_HINT: &str = "copied to clipboard";
const COPY_FAILED_HINT: &str = "failed to copy to clipboard";
const LOG_BUNDLE_FAILED_HINT: &str = "Failed to create log bundle (check logs)";
const NLD_ENABLED_HINT: &str = "Natural language detection enabled.";
const NLD_DISABLED_HINT: &str = "Natural language detection disabled.";
const NLD_PERSISTENCE_FAILED_HINT: &str = "Could not save the natural language detection setting.";
const VIM_MODE_ENABLED_HINT: &str = "Vim mode enabled.";
const VIM_MODE_DISABLED_HINT: &str = "Vim mode disabled.";
const VIM_MODE_PERSISTENCE_FAILED_HINT: &str = "Could not save the vim mode setting.";
const COST_NO_ACTIVE_CONVERSATION_HINT: &str =
    "Cannot show conversation cost: no active conversation";
const COST_EMPTY_CONVERSATION_HINT: &str = "Cannot show conversation cost: conversation is empty";
const COST_CONVERSATION_IN_PROGRESS_HINT: &str =
    "Cannot show conversation cost: conversation is in progress";

fn log_bundle_success_message(path: &Path) -> String {
    format!("Log bundle saved to {}", path.display())
}

fn raw_prompt_if_not_blank(input: &str) -> Option<&str> {
    (!input.trim().is_empty()).then_some(input)
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

/// One resolved item in the footer's configured presentation order.
enum FooterSegment {
    /// Vim mode label (NOR/INS/VIS/V-L/REP), shown when vim mode is enabled.
    /// Always the leading segment when present, ahead of shell-mode/model.
    Vim(&'static str),
    ShellMode,
    ActiveIndicator(&'static str),
    Model(Box<dyn TuiElement>),
    WorkingDirectory(String),
    GitBranch(String),
    /// The selected conversation's context-window usage. BYOP has no cloud
    /// credits/cost, so unlike upstream's clickable credits⇄cost toggle this
    /// wraps Zap's informational context-% entry (`crate::usage`).
    ContextWindowUsage(Box<dyn TuiElement>),
    GitDiff { additions: usize, deletions: usize },
}

impl FooterSegment {
    fn separator_to(&self, next: &Self) -> &'static str {
        match (self, next) {
            // A leading vim indicator is joined to the shell-mode/model
            // segment right after it with a plain space, matching the
            // shell-mode-to-cwd relationship below.
            (Self::Vim(_), Self::ShellMode | Self::Model(_)) => " ",
            // Only a shell-mode label directly preceding the working directory
            // gets a plain space; a leading Model label falls through to the
            // default " • " below so model and cwd don't visually run
            // together (fixes a missing divider present in an earlier
            // revision of this port — see warp upstream 311deab98).
            (Self::ShellMode, Self::WorkingDirectory(_)) => " ",
            (Self::WorkingDirectory(_), Self::GitBranch(_)) => " ↬ ",
            (
                Self::Vim(_)
                | Self::ShellMode
                | Self::ActiveIndicator(_)
                | Self::Model(_)
                | Self::WorkingDirectory(_)
                | Self::GitBranch(_)
                | Self::ContextWindowUsage(_)
                | Self::GitDiff { .. },
                Self::Vim(_)
                | Self::ShellMode
                | Self::ActiveIndicator(_)
                | Self::Model(_)
                | Self::WorkingDirectory(_)
                | Self::GitBranch(_)
                | Self::ContextWindowUsage(_)
                | Self::GitDiff { .. },
            ) => " • ",
        }
    }
}

/// Resolved segments for the footer's left-aligned status row.
struct FooterSegments {
    ordered: Vec<FooterSegment>,
}

/// Builds the status row from resolved segments. Working directory follows a
/// leading model or shell-mode label with a plain space; an immediately
/// following branch uses the existing ` ↬ ` relationship marker. Every other
/// adjacent pair uses ` • `, and the first item never receives a separator.
/// Every child truncates to a single row, so the row lays out one row tall.
fn render_status_footer_row(segments: FooterSegments, builder: &TuiUiBuilder) -> TuiFlex {
    let muted = builder.muted_text_style();
    let mut row = TuiFlex::row();
    let mut segments = segments.ordered.into_iter().peekable();
    while let Some(segment) = segments.next() {
        let separator = segments.peek().map(|next| segment.separator_to(next));
        match segment {
            FooterSegment::Vim(label) => {
                row = row.child(
                    TuiText::new(label)
                        .with_style(builder.accent_border_style())
                        .truncate()
                        .finish(),
                );
            }
            FooterSegment::ShellMode => {
                row = row.child(
                    TuiText::new(SHELL_MODE_HINT)
                        .with_style(builder.shell_mode_accent_style())
                        .truncate()
                        .finish(),
                );
            }
            FooterSegment::ActiveIndicator(label) => {
                row = row.child(
                    TuiText::new(label)
                        .with_style(builder.success_glyph_style())
                        .truncate()
                        .finish(),
                );
            }
            FooterSegment::Model(model) | FooterSegment::ContextWindowUsage(model) => {
                row = row.child(model);
            }
            FooterSegment::WorkingDirectory(cwd) | FooterSegment::GitBranch(cwd) => {
                row = row.child(TuiText::new(cwd).with_style(muted).truncate().finish());
            }
            FooterSegment::GitDiff {
                additions,
                deletions,
            } => {
                if additions > 0 {
                    row = row.child(
                        TuiText::new(format!("+{additions}"))
                            .with_style(builder.diff_added_style())
                            .truncate()
                            .finish(),
                    );
                }
                if deletions > 0 {
                    if additions > 0 {
                        row = row.child(TuiText::new(" ").truncate().finish());
                    }
                    row = row.child(
                        TuiText::new(format!("-{deletions}"))
                            .with_style(builder.diff_removed_style())
                            .truncate()
                            .finish(),
                    );
                }
            }
        }
        if let Some(separator) = separator {
            row = row.child(
                TuiText::new(separator)
                    .with_style(muted)
                    .truncate()
                    .finish(),
            );
        }
    }

    row
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
    ToggleResponseSummaryVisibility,
    /// Click on the footer's active-model label: toggles the inline model
    /// picker (the same menu `/model` surfaces).
    ToggleModelMenu,
    /// Toggle per-conversation auto approve.
    ToggleAutoApprove { show_feedback: bool },
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
}

/// The authenticated terminal/session surface rendered inside [`RootTuiView`].
pub(crate) struct TuiTerminalSessionView {
    transcript: ViewHandle<TuiTranscriptView>,
    input_view: ViewHandle<TuiInputView>,
    attachment_bar: ViewHandle<TuiAttachmentBar>,
    inline_menus: Vec<TuiInlineMenu>,
    suggestions_mode: ModelHandle<TuiInputSuggestionsModeModel>,
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
    /// In-flight Tab-completion fetch from the shared completer engine.
    completions_fetch: Option<SpawnedFutureHandle>,
    skills_menu: ModelHandle<TuiSkillMenuModel>,
    mcp_menu: ModelHandle<TuiMcpMenuModel>,
    slash_commands_source: ModelHandle<TuiSlashCommandDataSource>,
    conversation_selection: ConversationSelectionHandle,
    ai_action_model: ModelHandle<BlocklistAIActionModel>,
    ai_controller: ModelHandle<BlocklistAIController>,
    cli_subagent_controller: ModelHandle<CLISubagentController>,
    cli_subagent_views: HashMap<BlockId, ViewHandle<TuiCLISubagentView>>,
    /// Read by the footer for the active session's working directory.
    active_session: ModelHandle<ActiveSession>,
    /// Repository currently containing the active session's working directory.
    current_repo_path: Option<LocalOrRemotePath>,
    /// Watcher-backed branch and uncommitted diff metadata for the footer.
    git_repo_status: Option<ModelHandle<GitRepoStatusModel>>,
    /// This view's surface id, used to resolve the active model for the footer
    /// the same way the request path does.
    terminal_surface_id: EntityId,
    /// Armed by a ctrl-c press; a second press while armed exits the TUI.
    /// The footer shows [`CTRL_C_EXIT_HINT`] while armed.
    exit_confirmation: ExitConfirmation,
    /// Last-response exchanges whose completed summary has been hidden with
    /// `/cost`. A later response has a new exchange ID and starts visible,
    /// matching the GUI's per-last-block state.
    hidden_response_summary_exchange_ids: HashSet<AIAgentExchangeId>,
    /// Hover state for the footer's clickable active-model label, owned here
    /// (not created inline during render) so it survives element-tree rebuilds,
    /// following the GUI's `MouseStateHandle` pattern.
    model_label_hover: MouseStateHandle,
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
    auto_approve_mouse: MouseStateHandle,
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
    zero_state_view: ViewHandle<TuiZeroStateView>,
}

/// Registers the session surface's keybindings. Called once at TUI startup
/// from `keybindings::init`. Ctrl-c is a fixed (non-remappable) binding,
/// mirroring peer agent CLIs that treat it as reserved.
pub(crate) fn init(app: &mut AppContext) {
    let view_context = id!(TuiTerminalSessionView::ui_name());
    // Ctrl-c is a reserved fixed binding on the session surface (cancel /
    // clear / exit), mirroring peer agent CLIs.
    app.register_fixed_bindings([
        FixedBinding::new(
            "ctrl-c",
            TuiTerminalSessionAction::Interrupt,
            view_context.clone(),
        )
        .with_group(TUI_BINDING_GROUP),
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
            PLAN_TOGGLE_BINDING_NAME,
            "Toggle the latest plan",
            TuiTerminalSessionAction::TogglePlan,
        )
        .with_context_predicate(view_context)
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
                & id!(SESSION_COMPOSER_OWNS_INPUT_FLAG),
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
                & id!(SESSION_COMPOSER_OWNS_INPUT_FLAG),
        )
        .with_group(TUI_BINDING_GROUP)
        .with_key_binding("alt-v"),
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
                view.update(ctx, |view, ctx| view.focus(ctx));
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
                    Self::focus_blocking_child(blocker, ctx);
                } else if let Some(statusline_config_view) = self.statusline_config_view.as_ref() {
                    statusline_config_view.update(ctx, |view, ctx| view.focus(ctx));
                } else {
                    ctx.focus(&self.input_view);
                }
            }
            TuiInputTarget::Pty => {
                ctx.focus_self();
            }
        }
    }

    fn focus_current_owner_if_active(&mut self, ctx: &mut ViewContext<Self>) {
        if self.is_focused_session(ctx) {
            self.focus_current_owner(ctx);
        }
    }

    fn focus_input_if_active(&self, ctx: &mut ViewContext<Self>) {
        if self.is_focused_session(ctx) {
            ctx.focus(&self.input_view);
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
                self.input_view
                    .update(ctx, |input, ctx| input.exit_shell_mode(ctx));
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

    /// Approves the agent's pending action on the long-running command it's
    /// driving -- the keyboard equivalent of clicking "[Allow]" in
    /// `TuiCLISubagentView` (`TuiCLISubagentViewAction::Allow`'s handler).
    fn allow_blocked_lrc_action(&mut self, ctx: &mut ViewContext<Self>) {
        let Some(target) = self.active_agent_blocked_target(ctx) else {
            return;
        };
        self.ai_action_model.update(ctx, |action_model, ctx| {
            action_model.execute_next_action_for_user(target.conversation_id, ctx);
        });
    }

    /// Rejects the agent's pending action on the long-running command it's
    /// driving, without taking over the command -- the keyboard equivalent of
    /// clicking "[Reject]" in `TuiCLISubagentView`
    /// (`TuiCLISubagentViewAction::Reject`'s handler). Mirrors the GUI's
    /// `RejectBlockedAction { should_user_take_over: false }`: the front
    /// pending action for the conversation is cancelled (same action
    /// `execute_next_action_for_user` would have run), and control stays with
    /// the agent.
    fn reject_blocked_lrc_action(&mut self, ctx: &mut ViewContext<Self>) {
        let Some(target) = self.active_agent_blocked_target(ctx) else {
            return;
        };
        let conversation_id = target.conversation_id;
        self.ai_action_model.update(ctx, |action_model, ctx| {
            let Some(action_id) = action_model
                .get_pending_actions_for_conversation(&conversation_id)
                .next()
                .map(|action| action.id.clone())
            else {
                return;
            };
            action_model.cancel_action_with_id(
                conversation_id,
                &action_id,
                CancellationReason::ManuallyCancelled,
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
        let active_session =
            ctx.add_model(|ctx| ActiveSession::new(sessions.clone(), model_events.clone(), ctx));
        let model_for_conversation_selection = model.clone();
        let conversation_selection = ctx.add_model(|ctx| {
            Box::new(TuiConversationSelection::new(
                terminal_surface_id,
                model_for_conversation_selection,
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
                ctx,
            )
        });
        let ai_input_model = ctx.add_model(|ctx| {
            BlocklistAIInputModel::new_tui(
                model.clone(),
                context_model.clone(),
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
            TuiModelMenuModel::new(input_editor_model.clone(), suggestions_mode.clone(), ctx)
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
        let mcp_menu = ctx.add_model(|ctx| TuiMcpMenuModel::new(suggestions_mode.clone(), ctx));
        ctx.subscribe_to_model(&mcp_menu, |_, _, event, ctx| {
            let TuiMcpMenuEvent::Updated = event;
            ctx.notify();
        });
        let prompt_history_menu = ctx.add_model(|ctx| {
            TuiPromptHistoryMenuModel::new(
                input_editor_model.clone(),
                suggestions_mode.clone(),
                terminal_surface_id,
                ctx,
            )
        });
        ctx.subscribe_to_model(&prompt_history_menu, |_, _, event, ctx| {
            let TuiPromptHistoryMenuEvent::Updated = event;
            ctx.notify();
        });
        let completions_menu =
            ctx.add_model(|_| TuiCompletionsMenuModel::new(suggestions_mode.clone()));
        ctx.subscribe_to_model(&completions_menu, |_, _, _: &TuiCompletionsMenuEvent, ctx| {
            ctx.notify();
        });
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
        // confirmation; the ctrl-c buffer clear leaves the buffer empty, so the
        // window it arms survives its own clear.
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
            }
            view.handle_input_content_changed(origin.from_user(), ctx);
            ctx.notify();
        });

        let editor_for_selection = input_editor_model.clone();
        let transcript_for_selection = transcript.clone();
        ctx.subscribe_to_model(&input_editor_model, move |_, _, event, ctx| {
            if !matches!(event, CodeEditorModelEvent::SelectionChanged) {
                return;
            }

            let has_selection = !editor_for_selection
                .as_ref(ctx)
                .buffer_selection_model()
                .as_ref(ctx)
                .first_selection_is_single_cursor();
            if has_selection {
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
            TuiInlineMenu::new(prompt_history_menu.clone()),
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
            TuiInputViewEvent::AcceptedPromptHistory(text) => {
                view.handle_accepted_prompt_history(text.clone(), ctx);
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
        ctx.subscribe_to_model(&ai_input_model, |_, _, _, ctx| ctx.notify());
        ctx.subscribe_to_model(&suggestions_mode, |_, _, _, ctx| ctx.notify());
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
                view.resume_after_user_controlled_command(&completed.block_id, ctx);
                view.update_process_input_focus(ctx);
                ctx.notify();
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
                ctx.notify();
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
        ctx.subscribe_to_model(&active_session, |view, _, event, ctx| match event {
            ActiveSessionEvent::UpdatedPwd => {
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
        let zero_state_view =
            ctx.add_tui_view(|ctx| TuiZeroStateView::new(active_session.clone(), ctx));
        Self {
            transcript,
            input_view,
            attachment_bar,
            inline_menus,
            suggestions_mode,
            conversation_menu,
            model_menu,
            completions_menu,
            profile_menu,
            prompts_menu,
            exchange_menu,
            api_keys_menu,
            queued_follow_up: None,
            completions_fetch: None,
            skills_menu,
            mcp_menu,
            slash_commands_source,
            conversation_selection,
            ai_action_model: action_model,
            ai_controller,
            cli_subagent_controller,
            cli_subagent_views: HashMap::new(),
            active_session,
            current_repo_path: None,
            git_repo_status: None,
            terminal_surface_id,
            exit_confirmation: ExitConfirmation::default(),
            hidden_response_summary_exchange_ids: HashSet::new(),
            model_label_hover: MouseStateHandle::default(),
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
            auto_approve_mouse: MouseStateHandle::default(),
            conversation_restore_state: ConversationRestoreState::Idle,
            next_restore_request_id: 0,
            exit_summary,
            active_blocker_view_id: None,
            statusline_config_view: None,
            zero_state_view,
        }
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
            BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, ctx| match &target {
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
                    "The Warp TUI only supports Oz/Warp conversations.".to_owned(),
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
            self.update_process_input_focus(ctx);
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
                | BlocklistAIHistoryEvent::UpdatedAutoexecuteOverride { .. }
        ) {
            ctx.notify();
        }

        if matches!(
            event,
            BlocklistAIHistoryEvent::RestoredConversations { .. }
        ) {
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

    /// Displays success-colored feedback in the transient footer slot.
    fn show_copy_hint(&mut self, ctx: &mut ViewContext<Self>) {
        self.show_success_hint(COPY_SELECTION_HINT.to_owned(), ctx);
    }

    /// Handles a ctrl-c press: a second press within [`CTRL_C_EXIT_WINDOW`]
    /// exits the TUI; otherwise one contextual action runs — cancel the running
    /// conversation if there is one, else clear the input — and the exit
    /// confirmation is (re-)armed, surfacing [`CTRL_C_EXIT_HINT`] in the footer.
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
        if self.handle_terminal_use_interrupt(ctx) {
            self.exit_confirmation.disarm();
            ctx.notify();
            return;
        }
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
    fn cancel_active_conversation(&mut self, ctx: &mut ViewContext<Self>) -> bool {
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
            .auto_approve_mouse
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
            self.auto_approve_mouse.clone(),
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

    /// Builds the configured statusline under the input box. Normal mode uses
    /// the persisted item order and visibility (`/statusline`); shell mode
    /// always leads with its mode label and only resolves configured
    /// working-directory and git items. A replacing hint — the ctrl-c exit
    /// confirmation while armed, the conversation-list loading hint, or an
    /// active transient notice — occupies the whole row instead. An empty
    /// resolved configuration consumes no row.
    fn render_footer(&self, ctx: &AppContext) -> TuiFlex {
        let builder = TuiUiBuilder::from_app(ctx);
        let muted = builder.muted_text_style();

        // Replacing hints occupy the entire status row, in the existing
        // priority order: ctrl-c → loading → transient.
        if self.exit_confirmation.is_armed() {
            return TuiFlex::row().child(
                TuiText::new(CTRL_C_EXIT_HINT)
                    .with_style(muted)
                    .truncate()
                    .finish(),
            );
        }
        if matches!(
            &self.conversation_restore_state,
            ConversationRestoreState::Loading {
                origin: TuiConversationRestoreOrigin::ConversationList,
                ..
            }
        ) {
            return TuiFlex::row().child(
                TuiText::new(LOADING_CONVERSATION_HINT)
                    .with_style(muted)
                    .truncate()
                    .finish(),
            );
        }
        if let Some((transient, tone)) = self.transient_hint.current() {
            let style = match tone {
                TransientHintTone::Muted => muted,
                TransientHintTone::Success => builder.success_glyph_style(),
            };
            return TuiFlex::row().child(
                TuiText::new(transient)
                    .with_style(style)
                    .truncate()
                    .finish(),
            );
        }
        let shell_mode = self.is_shell_mode(ctx);
        let config = AISettings::as_ref(ctx).tui_statusline.normalized();
        let git_metadata = self.git_status_metadata(ctx);
        let mut ordered = Vec::new();
        if let Some(vim_label) = self.vim_mode_indicator(ctx) {
            ordered.push(FooterSegment::Vim(vim_label));
        }
        if shell_mode {
            ordered.push(FooterSegment::ShellMode);
        }
        for item in config.order.iter().copied() {
            if !config.is_enabled(item) {
                continue;
            }
            let segment = match item {
                TuiStatuslineItem::AutoApprove => (!shell_mode
                    && self
                        .conversation_selection
                        .as_ref(ctx)
                        .pending_query_autoexecute_override(ctx)
                        .is_autoexecute_any_action())
                .then_some(FooterSegment::ActiveIndicator("Auto-approve")),
                // Zap has no persistent "auto-queue" mode (`/queue` holds a
                // single specific prompt instead — see `TuiStatuslineItem`'s
                // doc comment), so this indicates a queued follow-up prompt.
                TuiStatuslineItem::AutoQueue => (!shell_mode && self.queued_follow_up.is_some())
                    .then_some(FooterSegment::ActiveIndicator("Queued")),
                TuiStatuslineItem::Model => (!shell_mode).then(|| {
                    let model_name = LLMPreferences::as_ref(ctx)
                        .get_active_base_model(ctx, Some(self.terminal_surface_id))
                        .display_name
                        .clone();
                    // The active-model label is clickable: a left click
                    // toggles the inline model picker (the same menu
                    // `/model` surfaces). The hover state lives on a
                    // retained [`MouseStateHandle`] so it survives
                    // element-tree rebuilds, and the click dispatches a
                    // typed action since the element pass only has an
                    // immutable [`AppContext`] — mirroring the usage entry.
                    let model_label_hovered = self
                        .model_label_hover
                        .lock()
                        .is_ok_and(|state| state.is_hovered());
                    let model_label_style = if model_label_hovered {
                        builder.primary_text_style()
                    } else {
                        builder.muted_text_style()
                    };
                    FooterSegment::Model(
                        TuiHoverable::new(
                            self.model_label_hover.clone(),
                            TuiText::new(model_name)
                                .with_style(model_label_style)
                                .truncate()
                                .finish(),
                        )
                        .on_click(|event_ctx, _| {
                            event_ctx
                                .dispatch_typed_action(TuiTerminalSessionAction::ToggleModelMenu);
                        })
                        .finish(),
                    )
                }),
                TuiStatuslineItem::WorkingDirectory => self
                    .current_working_directory(ctx)
                    .map(|cwd| FooterSegment::WorkingDirectory(compact_footer_path(&cwd))),
                TuiStatuslineItem::GitBranch => git_metadata
                    .map(|metadata| FooterSegment::GitBranch(metadata.current_branch_name.clone())),
                TuiStatuslineItem::GitDiffStatus => git_metadata.and_then(|metadata| {
                    let stats = metadata.stats_against_head;
                    (stats.total_additions > 0 || stats.total_deletions > 0).then_some(
                        FooterSegment::GitDiff {
                            additions: stats.total_additions,
                            deletions: stats.total_deletions,
                        },
                    )
                }),
                // Selected conversation's context-window occupancy, hidden
                // until any usage has been reported (and hidden in shell
                // mode, where it is stale AI-conversation metadata). BYOP
                // has no cloud credits/cost, so this reuses Zap's existing
                // informational context-% entry (`crate::usage`) rather than
                // upstream's clickable credits⇄cost toggle.
                TuiStatuslineItem::ContextWindowUsage => (!shell_mode)
                    .then(|| self.selected_conversation_context_usage(ctx))
                    .flatten()
                    .map(|fraction| {
                        FooterSegment::ContextWindowUsage(render_context_usage_entry(
                            fraction, ctx,
                        ))
                    }),
            };
            if let Some(segment) = segment {
                ordered.push(segment);
            }
        }
        render_status_footer_row(FooterSegments { ordered }, &builder)
    }

    /// Returns a brief vim mode label for the footer (NOR/INS/VIS/V-L/REP)
    /// when vim mode is enabled, or `None` when vim mode is disabled.
    fn vim_mode_indicator(&self, ctx: &AppContext) -> Option<&'static str> {
        use vim::vim::{MotionType, VimMode};
        let mode = self.input_view.as_ref(ctx).vim_mode(ctx)?;
        match mode {
            VimMode::Normal => Some("NOR"),
            VimMode::Visual(MotionType::Charwise) => Some("VIS"),
            VimMode::Visual(MotionType::Linewise) => Some("V-L"),
            VimMode::Replace => Some("REP"),
            // Insert mode is shown with a label, matching the GUI vim status indicator.
            VimMode::Insert => Some("INS"),
        }
    }

    /// Updates the watcher-backed git-status subscription after repository
    /// detection completes for the active working directory.
    fn update_git_status_subscription(
        &mut self,
        repo_path: Option<LocalOrRemotePath>,
        ctx: &mut ViewContext<Self>,
    ) {
        if self.current_repo_path == repo_path && self.git_repo_status.is_some() {
            return;
        }
        self.current_repo_path = repo_path.clone();
        self.git_repo_status = None;

        let Some(repo_path) = repo_path else {
            ctx.notify();
            return;
        };
        // Zap's git-status singleton keys on a local `&Path` (BYOP has no remote repos).
        let local_repo_path = match &repo_path {
            LocalOrRemotePath::Local(path) => path.clone(),
            LocalOrRemotePath::Remote(_) => {
                ctx.notify();
                return;
            }
        };
        match GitRepoModels::handle(ctx)
            .update(ctx, |models, ctx| models.subscribe(&local_repo_path, ctx))
        {
            Ok(handle) => {
                ctx.subscribe_to_model(&handle, |_, _, _, ctx| ctx.notify());
                self.git_repo_status = Some(handle);
            }
            Err(error) => {
                log::warn!("Unable to subscribe TUI footer to git status: {error}");
            }
        }
        ctx.notify();
    }

    fn git_status_metadata<'a>(&self, ctx: &'a AppContext) -> Option<&'a GitStatusMetadata> {
        self.git_repo_status.as_ref()?.as_ref(ctx).metadata()
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

    /// Tab-completion entry point. When the completions popup is already open,
    /// Tab cycles to the next candidate; otherwise it fetches candidates for the
    /// token under the cursor from the shared completer engine and opens the
    /// popup. Completion operates on the existing buffer, so (unlike the other
    /// inline menus) it does not clear or take over the input.
    fn trigger_completions(&mut self, ctx: &mut ViewContext<Self>) {
        if self.completions_menu.as_ref(ctx).is_open(ctx) {
            self.completions_menu
                .update(ctx, |menu, ctx| menu.select_next(ctx));
            ctx.notify();
            return;
        }
        if let Some(future) = self.completions_fetch.take() {
            future.abort();
        }
        let buffer_text = self.input_buffer_text(ctx);
        // Complete at end of buffer: the TUI composer keeps the cursor at the
        // tail for the common single-line case.
        let cursor_pos = buffer_text.len();
        if buffer_text[..cursor_pos].trim().is_empty() {
            return;
        }
        let Some(current_working_directory) = self.current_working_directory(ctx) else {
            return;
        };
        let Some(completion_context) = tui_completion_session_context(
            self.active_session.as_ref(ctx),
            current_working_directory,
            ctx,
        ) else {
            return;
        };
        let completion_session = completion_context.session.clone();
        self.completions_fetch = Some(ctx.spawn_abortable(
            async move {
                tui_fetch_completions(buffer_text.clone(), cursor_pos, completion_context).await
            },
            move |view, results, ctx| {
                view.completions_fetch = None;
                let Some(results) = results else {
                    return;
                };
                let rows = results
                    .candidates
                    .into_iter()
                    .map(|candidate| {
                        (candidate.display, candidate.replacement, candidate.description)
                    })
                    .collect::<Vec<_>>();
                view.completions_menu.update(ctx, |menu, ctx| {
                    menu.show(rows, results.replacement_span, ctx);
                });
                ctx.notify();
            },
            move |_, _| {
                completion_session.cancel_active_commands();
            },
        ));
    }

    /// Applies an accepted completion: replaces the completed span in the input
    /// buffer with the chosen replacement text.
    fn handle_accepted_completion(
        &mut self,
        completion: TuiAcceptedCompletion,
        ctx: &mut ViewContext<Self>,
    ) {
        let buffer_text = self.input_buffer_text(ctx);
        let TuiAcceptedCompletion { replacement, span } = completion;
        let Some(new_text) =
            crate::completions_menu::apply_completion_replacement(&buffer_text, &replacement, &span)
        else {
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
            self.input_view
                .update(ctx, |input, ctx| input.exit_shell_mode(ctx));
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

        ctx.emit(TuiTerminalSessionEvent::ExecuteCommand(Box::new(
            ExecuteCommandEvent {
                command: command.to_owned(),
                session_id,
                workflow_id: None,
                workflow_command: None,
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
        // `fork_conversation[_at_exchange]` copies the tasks under a new
        // conversation id and inserts the fork into the history model in memory.
        // `/fork-from` forks up to the chosen exchange (fork_from_exact_exchange
        // = false extends through the selected response, matching the GUI).
        let fork_result = BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, ctx| {
            match fork_from_exchange {
                Some(exchange_id) => {
                    history.fork_conversation_at_exchange(&source, exchange_id, false, FORK_PREFIX, ctx)
                }
                None => history.fork_conversation(&source, FORK_PREFIX, ctx),
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
                self.queued_follow_up = normalize_optional_prompt(initial_prompt).map(|prompt| {
                    TuiQueuedFollowUp {
                        conversation_id: forked_id,
                        prompt,
                        seen_in_progress: false,
                    }
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
    fn open_exchange_menu(
        &mut self,
        action: TuiExchangeMenuAction,
        ctx: &mut ViewContext<Self>,
    ) {
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
        self.exchange_menu.update(ctx, |menu, ctx| menu.dismiss(ctx));
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
    fn rewind_to_exchange(
        &mut self,
        exchange_id: AIAgentExchangeId,
        ctx: &mut ViewContext<Self>,
    ) {
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
                if let Err(error) =
                    history.fork_conversation(&conversation, PRE_REWIND_PREFIX, ctx)
                {
                    log::warn!("Failed to save pre-rewind backup of {conversation_id}: {error}");
                }
            }
        });
        // Truncate the conversation at the chosen exchange.
        let removed_exchange_ids = match BlocklistAIHistoryModel::handle(ctx)
            .update(ctx, |history, ctx| {
                history.truncate_conversation_from_exchange(conversation_id, exchange_id, ctx)
            }) {
            Ok(removed) => removed,
            Err(error) => {
                log::warn!("Failed to truncate conversation {conversation_id} for rewind: {error}");
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
            let diffs = TuiFileEditRevertRegistry::handle(ctx)
                .update(ctx, |registry, _| registry.take_diffs(&conversation_id, action_id));
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
        TuiMcpManager::handle(ctx).update(ctx, |model, ctx| {
            model.apply_action(action, ctx);
        });
        ctx.notify();
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

    /// Fills the accepted prompt-history prompt into the input and submits it
    /// immediately, matching the GUI's accept-a-prompt-from-history behavior.
    /// The menu has already closed itself.
    fn handle_accepted_prompt_history(&mut self, text: String, ctx: &mut ViewContext<Self>) {
        self.input_view.update(ctx, |input, ctx| {
            input.set_text(&text, ctx);
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
            SlashCommandKind::Agent | SlashCommandKind::New => {
                if !self
                    .ai_context_model
                    .as_ref(ctx)
                    .can_start_new_conversation()
                {
                    self.show_transient_hint(NEW_CONVERSATION_COMMAND_RUNNING_HINT.to_owned(), ctx);
                    return;
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
            SlashCommandKind::Cost => {
                self.input_view.update(ctx, |input, ctx| input.clear(ctx));
                ctx.dispatch_typed_action_deferred(
                    TuiTerminalSessionAction::ToggleResponseSummaryVisibility,
                );
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
                self.input_view.update(ctx, |input, ctx| input.clear(ctx));
                self.mcp_menu.update(ctx, |menu, ctx| menu.open(ctx));
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
            SlashCommandKind::EnableNaturalLanguageDetection => {
                self.set_nld_enabled(true, command.name, ctx);
            }
            SlashCommandKind::DisableNaturalLanguageDetection => {
                self.set_nld_enabled(false, command.name, ctx);
            }
            SlashCommandKind::VimMode => {
                self.toggle_vim_mode(command.name, ctx);
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
            | SlashCommandKind::Usage
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
                self.persist_statusline_config(config.clone(), ctx);
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
        ctx: &mut ViewContext<Self>,
    ) {
        let result = AISettings::handle(ctx).update(ctx, |settings, ctx| {
            settings.tui_statusline.set_value(config.normalized(), ctx)
        });
        self.statusline_config_view = None;
        self.focus_current_owner_if_active(ctx);
        match result {
            Ok(()) => self.show_success_hint(STATUSLINE_SAVED_HINT.to_owned(), ctx),
            Err(error) => {
                log::warn!("Failed to persist the TUI statusline config: {error}");
                self.show_transient_hint(STATUSLINE_PERSISTENCE_FAILED_HINT.to_owned(), ctx);
            }
        }
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
        let result = AppEditorSettings::handle(ctx)
            .update(ctx, |settings, ctx| settings.vim_mode.set_value(enabled, ctx));
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
        if self.active_agent_blocked_target(ctx).is_some() {
            context.set.insert(SESSION_CAN_ALLOW_BLOCKED_LRC_ACTION_FLAG);
            context.set.insert(SESSION_CAN_REJECT_BLOCKED_LRC_ACTION_FLAG);
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
            context.set.insert(SESSION_COMPOSER_OWNS_INPUT_FLAG);
            if self.attachment_bar.as_ref(ctx).should_render(ctx) {
                context.set.insert(ATTACHMENTS_AVAILABLE_FLAG);
            }
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
                    TuiConversationRestoreOrigin::ConversationList
                    | TuiConversationRestoreOrigin::Fork,
                ..
            } => {}
            ConversationRestoreState::Failed(message) => {
                return conversation_restore_failed(message);
            }
            ConversationRestoreState::Idle => {}
        }
        let builder = TuiUiBuilder::from_app(ctx);
        // While a full-screen (alt-screen) app is active, hand the whole pane to
        // it: render its grid and forward input, instead of the block UI.
        let (alt_screen_active, input_target, user_owns_running_command) = {
            let terminal_model = self.terminal_model.lock();
            (
                terminal_model.is_alt_screen_active(),
                tui_input_target(&terminal_model),
                inline_process_owns_input(&terminal_model),
            )
        };
        if alt_screen_active {
            return TuiTerminalContentElement::new(
                self.terminal_resize_tx.clone(),
                AltScreenElement::new(self.terminal_model.clone()).finish(),
            )
            .with_pty_input(self.terminal_model.clone())
            .finish();
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
        if self.transcript.as_ref(ctx).is_empty() {
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
                        "Burning in"
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
        // input's slot so the interrupt affordance stays discoverable. Gated
        // on the user-controlled-command predicate, not the broader PTY input
        // target: visible startup-script execution also routes input to the
        // PTY but is not a command the user should be told to interrupt.
        // (Agent-driven terminal use keeps the composer, and its control
        // hints come from the CLI-subagent status line.)
        if !blocker_active && user_owns_running_command {
            content = content.child(
                TuiContainer::new(
                    TuiText::new(input_hints::LONG_RUNNING_COMMAND_HINT)
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
                        .with_border_style(border_style)
                        .finish(),
                )
                .with_max_rows(MAX_INPUT_TEXT_ROWS + 2)
                .finish(),
            );
            let footer = self.render_footer(ctx).finish();
            content = content.child(TuiConstrainedBox::new(footer).with_max_rows(1).finish());
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
    fn handle_typeahead_event(&mut self, ctx: &mut ViewContext<Self>) {
        let typeahead = self.terminal_model.lock().take_typeahead_for_input();
        if let Some((text, previously_inserted)) = typeahead {
            self.input_view.update(ctx, |input, ctx| {
                input.insert_typeahead_text(previously_inserted, &text, ctx);
            });
        }
        ctx.notify();
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
            TuiTerminalSessionAction::AllowBlockedLrcAction => {
                self.allow_blocked_lrc_action(ctx)
            }
            TuiTerminalSessionAction::RejectBlockedLrcAction => {
                self.reject_blocked_lrc_action(ctx)
            }
            TuiTerminalSessionAction::ToggleResponseSummaryVisibility => {
                self.toggle_response_summary_visibility(ctx)
            }
            TuiTerminalSessionAction::ToggleModelMenu => self.toggle_model_menu(ctx),
            TuiTerminalSessionAction::ToggleAutoApprove { show_feedback } => {
                self.toggle_auto_approve(*show_feedback, ctx)
            }
            TuiTerminalSessionAction::ForwardUserPtyBytes(bytes) => {
                // Raw passthrough: the bytes are already the app's escape
                // sequence, so write them to the PTY unmodified.
                ctx.emit(TuiTerminalSessionEvent::WriteUserInput(Cow::Owned(
                    bytes.clone(),
                )));
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
                self.trigger_completions(ctx);
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
