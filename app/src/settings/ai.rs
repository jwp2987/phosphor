//! Settings for Blocklist AI.
//!
//! These settings are currently used to configure the underlying model/API used to power the AI
//! UX, as well as small UX configurations.

use std::collections::HashMap;
use std::path::PathBuf;

use indexmap::IndexMap;

use crate::ai::request_usage_model::RequestLimitInfo;
use crate::report_if_error;
use crate::terminal::CLIAgent;
use crate::workspaces::user_workspaces::UserWorkspaces;
use cfg_if::cfg_if;
use chrono::{DateTime, Utc};
use lazy_static::lazy_static;
use regex::Regex;
use warpui::platform::OperatingSystem;
use warpui::{
    AppContext, Entity, ModelContext, SingletonEntity, UpdateModel, platform::keyboard::KeyCode,
};

use settings::{
    RespectUserSyncSetting, Setting, SupportedPlatforms, SyncToCloud, define_settings_group,
};
use warp_core::execution_mode::AppExecutionMode;
use warp_core::features::FeatureFlag;

use serde::{Deserialize, Serialize, de::Deserializer};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

pub enum FocusedTerminalInfoEvent {
    TerminalInfoUpdated,
}

/// Singleton model that is used to track the remote sessions in the terminal.
/// Useful for organizations that have restrictions on using AI in sessions in
/// remote sessions.
#[derive(Default, Clone, Debug)]
pub struct FocusedTerminalInfo {
    contains_any_remote_blocks: bool,
    contains_any_restored_remote_blocks: bool,
}

impl FocusedTerminalInfo {
    pub fn new(_: &mut ModelContext<Self>) -> Self {
        Self {
            contains_any_remote_blocks: false,
            contains_any_restored_remote_blocks: false,
        }
    }

    pub fn contains_any_remote_blocks(&self) -> bool {
        self.contains_any_remote_blocks
    }

    pub fn contains_any_restored_remote_blocks(&self) -> bool {
        self.contains_any_restored_remote_blocks
    }

    /// Updates both remote blocks and restored blocks status in a single atomic operation.
    /// Only emits a TerminalInfoUpdated event if either value changes.
    /// Returns true if the event was emitted.
    pub fn update(
        &mut self,
        contains_any_remote_blocks: bool,
        contains_any_restored_remote_blocks: bool,
        ctx: &mut ModelContext<Self>,
    ) -> bool {
        let remote_changed = self.contains_any_remote_blocks != contains_any_remote_blocks;
        let restored_changed =
            self.contains_any_restored_remote_blocks != contains_any_restored_remote_blocks;

        if remote_changed || restored_changed {
            self.contains_any_remote_blocks = contains_any_remote_blocks;
            self.contains_any_restored_remote_blocks = contains_any_restored_remote_blocks;
            ctx.emit(FocusedTerminalInfoEvent::TerminalInfoUpdated);
            return true;
        }

        false
    }
}

impl Entity for FocusedTerminalInfo {
    type Event = FocusedTerminalInfoEvent;
}

impl SingletonEntity for FocusedTerminalInfo {}

#[derive(
    Default,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    PartialEq,
    Copy,
    Clone,
    EnumIter,
    schemars::JsonSchema,
    settings_value::SettingsValue,
)]
#[schemars(
    description = "Physical key used to toggle voice input.",
    rename_all = "snake_case"
)]
pub enum VoiceInputToggleKey {
    #[default]
    #[schemars(description = "No toggle key assigned.")]
    None,
    /// Fn key is default toggle key for Mac, when the feature is toggled on.
    #[schemars(description = "Fn key.")]
    Fn,
    /// Alt or Option key (left side).
    #[schemars(description = "Alt or Option key (left side).")]
    AltLeft,
    /// Alt or Option key (right side). Used as default toggle
    /// key for Windows and Linux, , when the feature is toggled on.
    #[schemars(description = "Alt or Option key (right side).")]
    AltRight,
    #[schemars(description = "Control key (left side).")]
    ControlLeft,
    #[schemars(description = "Control key (right side).")]
    ControlRight,
    /// The Windows, ⌘, Command, or other OS symbol key.
    #[schemars(description = "Super, Windows, or Command key (left side).")]
    SuperLeft,
    /// The Windows, ⌘, Command, or other OS symbol key.
    #[schemars(description = "Super, Windows, or Command key (right side).")]
    SuperRight,
    #[schemars(description = "Shift key (left side).")]
    ShiftLeft,
    #[schemars(description = "Shift key (right side).")]
    ShiftRight,
}

settings::macros::implement_setting_for_enum!(
    VoiceInputToggleKey,
    AISettings,
    SupportedPlatforms::DESKTOP,
    // Never sync to cloud to allow users to use different toggle keys on different devices,
    // especially given platform differences.
    SyncToCloud::Never,
    private: false,
    toml_path: "agents.voice.voice_input_toggle_key",
    description: "The key used to toggle voice input.",
);

impl VoiceInputToggleKey {
    pub fn all_possible_values() -> Vec<VoiceInputToggleKey> {
        let all_keys = VoiceInputToggleKey::iter().collect();
        match OperatingSystem::get() {
            OperatingSystem::Mac => all_keys,
            // For non-Mac platforms, we exclude the `Fn` key since it may not be correctly identified by winit.
            // In particular, we saw it is unidentified for a ThinkPad with a standard keyboard.
            OperatingSystem::Windows | OperatingSystem::Linux | OperatingSystem::Other(_) => {
                all_keys
                    .into_iter()
                    .filter(|key| *key != VoiceInputToggleKey::Fn)
                    .collect()
            }
        }
    }

    /// Display name for choosing key from the AI settings page.
    pub fn display_name(&self) -> &'static str {
        // We use the underlying host OS to determine the correct key name to display.
        let (super_key_name, alt_key_name): (&'static str, &'static str) =
            match OperatingSystem::get() {
                OperatingSystem::Mac => ("Command", "Option"),
                OperatingSystem::Windows => ("Windows", "Alt"),
                OperatingSystem::Linux | OperatingSystem::Other(_) => ("Super", "Alt"),
            };

        match self {
            VoiceInputToggleKey::None => "None",
            VoiceInputToggleKey::Fn => "Fn",
            VoiceInputToggleKey::AltLeft => {
                Box::leak(format!("{alt_key_name} (Left)").into_boxed_str())
            }
            VoiceInputToggleKey::AltRight => {
                Box::leak(format!("{alt_key_name} (Right)").into_boxed_str())
            }
            VoiceInputToggleKey::ControlLeft => "Control (Left)",
            VoiceInputToggleKey::ControlRight => "Control (Right)",
            VoiceInputToggleKey::SuperLeft => {
                Box::leak(format!("{super_key_name} (Left)").into_boxed_str())
            }
            VoiceInputToggleKey::SuperRight => {
                Box::leak(format!("{super_key_name} (Right)").into_boxed_str())
            }
            VoiceInputToggleKey::ShiftLeft => "Shift (Left)",
            VoiceInputToggleKey::ShiftRight => "Shift (Right)",
        }
    }

    pub fn to_key_code(&self) -> Option<KeyCode> {
        match self {
            VoiceInputToggleKey::None => None,
            VoiceInputToggleKey::Fn => Some(KeyCode::Fn),
            VoiceInputToggleKey::AltLeft => Some(KeyCode::AltLeft),
            VoiceInputToggleKey::AltRight => Some(KeyCode::AltRight),
            VoiceInputToggleKey::ControlLeft => Some(KeyCode::ControlLeft),
            VoiceInputToggleKey::ControlRight => Some(KeyCode::ControlRight),
            VoiceInputToggleKey::SuperLeft => Some(KeyCode::SuperLeft),
            VoiceInputToggleKey::SuperRight => Some(KeyCode::SuperRight),
            VoiceInputToggleKey::ShiftLeft => Some(KeyCode::ShiftLeft),
            VoiceInputToggleKey::ShiftRight => Some(KeyCode::ShiftRight),
        }
    }

    /// Converts the voice input toggle key to a Keystroke representation.
    /// Since these are standalone modifier keys, we construct the Keystroke directly
    /// rather than using `parse()` (which always requires a non-modifier key to be included).
    pub fn keystroke(&self) -> Option<warpui::keymap::Keystroke> {
        use warpui::keymap::Keystroke;

        let keystroke = match self {
            VoiceInputToggleKey::None => return None,
            VoiceInputToggleKey::Fn => Keystroke {
                key: "fn".to_string(),
                ..Default::default()
            },
            VoiceInputToggleKey::AltLeft | VoiceInputToggleKey::AltRight => Keystroke {
                alt: true,
                ..Default::default()
            },
            VoiceInputToggleKey::ControlLeft | VoiceInputToggleKey::ControlRight => Keystroke {
                ctrl: true,
                ..Default::default()
            },
            VoiceInputToggleKey::SuperLeft | VoiceInputToggleKey::SuperRight => Keystroke {
                cmd: true,
                ..Default::default()
            },
            VoiceInputToggleKey::ShiftLeft | VoiceInputToggleKey::ShiftRight => Keystroke {
                shift: true,
                ..Default::default()
            },
        };
        Some(keystroke)
    }

    pub fn tooltip_message(&self) -> String {
        match self.keystroke() {
            Some(keystroke) => {
                let symbol = keystroke.displayed();
                let side = match self {
                    VoiceInputToggleKey::AltLeft
                    | VoiceInputToggleKey::ControlLeft
                    | VoiceInputToggleKey::SuperLeft
                    | VoiceInputToggleKey::ShiftLeft => Some("Left"),
                    VoiceInputToggleKey::AltRight
                    | VoiceInputToggleKey::ControlRight
                    | VoiceInputToggleKey::SuperRight
                    | VoiceInputToggleKey::ShiftRight => Some("Right"),
                    VoiceInputToggleKey::None | VoiceInputToggleKey::Fn => None,
                };
                let key_name = match side {
                    Some(side) => format!("{side} {symbol}"),
                    None => symbol,
                };
                format!("Voice input (hold {key_name} key)")
            }
            None => "Voice input".to_string(),
        }
    }

    pub fn is_none(&self) -> bool {
        matches!(self, VoiceInputToggleKey::None)
    }
}

/// The default mode for new terminal sessions.
#[derive(
    Default,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    PartialEq,
    Copy,
    Clone,
    EnumIter,
    schemars::JsonSchema,
    settings_value::SettingsValue,
)]
#[schemars(
    description = "Default mode for new sessions.",
    rename_all = "snake_case"
)]
pub enum DefaultSessionMode {
    /// New sessions start in the terminal mode (default).
    #[default]
    Terminal,
    /// New sessions start in agent view.
    Agent,
    /// New sessions start in cloud (ambient) agent mode.
    AmbientAgent,
    /// New sessions open a user-defined tab config.
    /// The specific config is identified by the companion `default_tab_config_path` setting.
    TabConfig,
    /// New sessions open in a local Docker sandbox.
    /// Requires the `LocalDockerSandbox` feature flag; falls back to `Terminal` when disabled.
    DockerSandbox,
}

settings::macros::implement_setting_for_enum!(
    DefaultSessionMode,
    AISettings,
    SupportedPlatforms::ALL,
    SyncToCloud::Globally(RespectUserSyncSetting::Yes),
    private: false,
    toml_path: "general.default_session_mode",
    description: "The default mode for new terminal sessions.",
);

impl DefaultSessionMode {
    /// Display name for the settings dropdown.
    pub fn display_name(&self) -> &'static str {
        match self {
            DefaultSessionMode::Terminal => "Terminal",
            DefaultSessionMode::Agent => "Agent",
            DefaultSessionMode::AmbientAgent => "Ambient Agent",
            DefaultSessionMode::TabConfig => "Tab Config",
            DefaultSessionMode::DockerSandbox => "Local Docker Sandbox",
        }
    }
}

/// Controls how agent thinking/reasoning traces are displayed after streaming.
#[derive(
    Default,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    PartialEq,
    Copy,
    Clone,
    EnumIter,
    schemars::JsonSchema,
    settings_value::SettingsValue,
)]
#[schemars(
    description = "Controls how agent thinking is displayed after streaming.",
    rename_all = "snake_case"
)]
pub enum ThinkingDisplayMode {
    /// Show reasoning blocks while streaming, then collapse them when complete (default).
    #[default]
    ShowAndCollapse,
    /// Always keep reasoning blocks expanded, even after streaming finishes.
    AlwaysShow,
    /// Never show reasoning blocks.
    NeverShow,
}

settings::macros::implement_setting_for_enum!(
    ThinkingDisplayMode,
    AISettings,
    SupportedPlatforms::ALL,
    SyncToCloud::Globally(RespectUserSyncSetting::Yes),
    private: false,
    toml_path: "agents.warp_agent.other.thinking_display_mode",
    description: "Controls how agent thinking traces are displayed after streaming.",
);

impl ThinkingDisplayMode {
    /// Display name for the settings dropdown.
    pub fn display_name(&self) -> &'static str {
        match self {
            ThinkingDisplayMode::ShowAndCollapse => "Show & collapse",
            ThinkingDisplayMode::AlwaysShow => "Always show",
            ThinkingDisplayMode::NeverShow => "Never show",
        }
    }

    pub fn command_palette_description(&self) -> String {
        match self {
            ThinkingDisplayMode::ShowAndCollapse => {
                crate::t!("agent-thinking-display-show-collapse")
            }
            ThinkingDisplayMode::AlwaysShow => crate::t!("agent-thinking-display-always-show"),
            ThinkingDisplayMode::NeverShow => crate::t!("agent-thinking-display-never-show"),
        }
    }

    pub fn should_render(&self) -> bool {
        !matches!(self, ThinkingDisplayMode::NeverShow)
    }

    pub fn should_keep_expanded(&self) -> bool {
        matches!(self, ThinkingDisplayMode::AlwaysShow)
    }
}

/// Controls how child-agent message bodies are displayed (#329).
#[derive(
    Default,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    PartialEq,
    Copy,
    Clone,
    EnumIter,
    schemars::JsonSchema,
    settings_value::SettingsValue,
)]
#[schemars(
    description = "Controls how child-agent messages are displayed.",
    rename_all = "snake_case"
)]
pub enum OrchestrationMessageDisplayMode {
    /// Show child-agent messages while streaming, then collapse them.
    ShowAndCollapse,
    /// Keep child-agent message bodies expanded.
    AlwaysShow,
    /// Keep child-agent message bodies collapsed.
    #[default]
    AlwaysCollapse,
}

settings::macros::implement_setting_for_enum!(
    OrchestrationMessageDisplayMode,
    AISettings,
    SupportedPlatforms::ALL,
    SyncToCloud::Globally(RespectUserSyncSetting::Yes),
    private: false,
    toml_path: "agents.warp_agent.other.orchestration_message_display_mode",
    description: "Controls how child-agent messages are displayed.",
);

impl OrchestrationMessageDisplayMode {
    /// Display name for the settings dropdown.
    pub fn display_name(&self) -> &'static str {
        match self {
            OrchestrationMessageDisplayMode::ShowAndCollapse => "Show & collapse",
            OrchestrationMessageDisplayMode::AlwaysShow => "Always show",
            OrchestrationMessageDisplayMode::AlwaysCollapse => "Always collapse",
        }
    }

    pub fn command_palette_description(&self) -> &'static str {
        match self {
            OrchestrationMessageDisplayMode::ShowAndCollapse => {
                "Set child-agent message display: show & collapse"
            }
            OrchestrationMessageDisplayMode::AlwaysShow => {
                "Set child-agent message display: always show"
            }
            OrchestrationMessageDisplayMode::AlwaysCollapse => {
                "Set child-agent message display: always collapse"
            }
        }
    }

    /// Whether child-agent message bodies should expand while streaming.
    pub fn should_expand_agent_message_body(&self) -> bool {
        matches!(
            self,
            OrchestrationMessageDisplayMode::ShowAndCollapse
                | OrchestrationMessageDisplayMode::AlwaysShow
        )
    }

    /// Whether child-agent message bodies should collapse after streaming.
    pub fn should_collapse_agent_message_body_on_finish(&self) -> bool {
        matches!(self, OrchestrationMessageDisplayMode::ShowAndCollapse)
    }
}

/// Default behavior when the user submits a new prompt while the agent is still responding.
///
/// This is the *default* used when a conversation has no explicit auto-queue
/// override. Per-conversation overrides live on `QueuedQueryModel` and take
/// precedence over this setting.
#[derive(
    Default,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    PartialEq,
    Copy,
    Clone,
    EnumIter,
    schemars::JsonSchema,
    settings_value::SettingsValue,
)]
#[schemars(
    description = "Default behavior when submitting a new prompt while the agent is still responding.",
    rename_all = "snake_case"
)]
pub enum PromptSubmissionMode {
    /// Cancel the in-flight response and submit the new prompt immediately
    /// (default).
    #[default]
    Interrupt,
    /// Hold the new prompt until the in-flight response finishes, then submit.
    Queue,
}

settings::macros::implement_setting_for_enum!(
    PromptSubmissionMode,
    AISettings,
    SupportedPlatforms::ALL,
    SyncToCloud::Globally(RespectUserSyncSetting::Yes),
    private: false,
    toml_path: "agents.warp_agent.other.default_prompt_submission_mode",
    description: "Default behavior when submitting a new prompt while the agent is still responding.",
    feature_flag: FeatureFlag::QueueSlashCommand,
);

impl PromptSubmissionMode {
    /// Display name for the settings dropdown.
    pub fn display_name(&self) -> &'static str {
        match self {
            PromptSubmissionMode::Interrupt => "Interrupt response",
            PromptSubmissionMode::Queue => "Queue until response finishes",
        }
    }

    pub fn command_palette_description(&self) -> &'static str {
        match self {
            PromptSubmissionMode::Interrupt => "Set default prompt submission: interrupt response",
            PromptSubmissionMode::Queue => {
                "Set default prompt submission: queue until response finishes"
            }
        }
    }
}

/// What happens when a prompt is submitted while an agent controls an agent-requested
/// long-running command (LRC).
///
/// Only consulted when [`PromptSubmissionMode`] is `Interrupt`: in `Queue` mode
/// prompts always queue until the full response finishes, so this setting is
/// hidden and ignored.
#[derive(
    Default,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    PartialEq,
    Copy,
    Clone,
    EnumIter,
    schemars::JsonSchema,
    settings_value::SettingsValue,
)]
#[schemars(
    description = "What happens when a prompt is submitted while an agent controls an agent-requested long-running command.",
    rename_all = "snake_case"
)]
pub enum LongRunningCommandSubmissionMode {
    /// Send the prompt to the agent immediately, steering it mid-command.
    SendImmediately,
    /// Queue the prompt and send it to the agent when the command finishes
    /// (default).
    #[default]
    QueueUntilCommandCompletes,
}

settings::macros::implement_setting_for_enum!(
    LongRunningCommandSubmissionMode,
    AISettings,
    SupportedPlatforms::ALL,
    SyncToCloud::Globally(RespectUserSyncSetting::Yes),
    private: false,
    toml_path: "agents.warp_agent.other.long_running_command_submission_mode",
    description: "What happens when a prompt is submitted while an agent controls an agent-requested long-running command.",
    feature_flag: FeatureFlag::QueueSlashCommand,
);

impl LongRunningCommandSubmissionMode {
    /// Display name for the settings dropdown.
    pub fn display_name(&self) -> &'static str {
        match self {
            LongRunningCommandSubmissionMode::SendImmediately => "Send immediately",
            LongRunningCommandSubmissionMode::QueueUntilCommandCompletes => {
                "Queue until command finishes"
            }
        }
    }

    pub fn command_palette_description(&self) -> &'static str {
        match self {
            LongRunningCommandSubmissionMode::SendImmediately => {
                "Set long-running command submission: send immediately"
            }
            LongRunningCommandSubmissionMode::QueueUntilCommandCompletes => {
                "Set long-running command submission: queue until command finishes"
            }
        }
    }
}

/// One configurable item in the TUI statusline.
///
/// Ported from warp/master's `TuiStatuslineItem`. Upstream also carries a `CreditUsage`
/// variant tied to Warp's cloud credits/billing system; Zap has no credits system (it
/// already replaced Warp's credits UI with a local BYOP context-window-usage footer), so
/// that variant is dropped and `ContextWindowUsage` covers the equivalent local-only need.
///
/// `AutoQueue` is kept but has different backing semantics than upstream: upstream's
/// variant reflects a persistent "auto-queue next prompt" *mode* toggle backed by
/// `QueuedQueryModel`, a feature not yet ported to Zap. Zap's `/queue` is a one-shot
/// "hold this specific prompt until the current turn finishes" action instead, so this
/// item indicates whether such a queued follow-up is currently pending.
#[derive(
    Debug,
    serde::Serialize,
    serde::Deserialize,
    PartialEq,
    Eq,
    Copy,
    Clone,
    Hash,
    EnumIter,
    schemars::JsonSchema,
    settings_value::SettingsValue,
)]
#[schemars(
    description = "A configurable item in the Zap Agent CLI statusline.",
    rename_all = "snake_case"
)]
#[serde(rename_all = "snake_case")]
pub enum TuiStatuslineItem {
    AutoApprove,
    AutoQueue,
    Model,
    WorkingDirectory,
    GitBranch,
    GitDiffStatus,
    /// Current-branch GitHub pull request, resolved through the local `gh` CLI
    /// (`GitHubRepoModel`) -- no Warp backend is involved.
    GitHubPullRequest,
    ContextWindowUsage,
    Date,
    #[schemars(rename = "time_12_hour")]
    Time12Hour,
    #[schemars(rename = "time_24_hour")]
    Time24Hour,
    AgentTodoList,
}

impl TuiStatuslineItem {
    pub const ALL: [Self; 12] = [
        Self::AutoApprove,
        Self::AutoQueue,
        Self::Model,
        Self::WorkingDirectory,
        Self::GitBranch,
        Self::GitDiffStatus,
        Self::GitHubPullRequest,
        Self::ContextWindowUsage,
        Self::Date,
        Self::Time12Hour,
        Self::Time24Hour,
        Self::AgentTodoList,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::AutoApprove => "Auto-approve indicator",
            Self::AutoQueue => "Queued follow-up prompt indicator",
            Self::Model => "Model",
            Self::WorkingDirectory => "Working directory",
            Self::GitBranch => "Git branch",
            Self::GitDiffStatus => "Git diff status",
            Self::GitHubPullRequest => "GitHub pull request",
            Self::ContextWindowUsage => "Context window usage",
            Self::Date => "Date",
            Self::Time12Hour => "Time (12 hour format)",
            Self::Time24Hour => "Time (24 hour format)",
            Self::AgentTodoList => "Agent to-do list",
        }
    }
}

/// Ordered and enabled items in the TUI statusline.
#[derive(
    Debug,
    serde::Serialize,
    serde::Deserialize,
    PartialEq,
    Eq,
    Clone,
    schemars::JsonSchema,
    settings_value::SettingsValue,
)]
pub struct TuiStatuslineConfig {
    pub order: Vec<TuiStatuslineItem>,
    pub enabled: Vec<TuiStatuslineItem>,
}

impl Default for TuiStatuslineConfig {
    fn default() -> Self {
        Self {
            order: TuiStatuslineItem::ALL.to_vec(),
            enabled: vec![
                TuiStatuslineItem::AutoApprove,
                TuiStatuslineItem::Model,
                TuiStatuslineItem::WorkingDirectory,
                TuiStatuslineItem::GitBranch,
                TuiStatuslineItem::GitDiffStatus,
            ],
        }
    }
}

impl TuiStatuslineConfig {
    /// Returns a complete, duplicate-free catalog and a valid enabled subset.
    pub fn normalized(&self) -> Self {
        let mut order = Vec::with_capacity(TuiStatuslineItem::ALL.len());
        for item in self.order.iter().copied().chain(TuiStatuslineItem::ALL) {
            // A persisted order may name an item that is no longer in the
            // catalog; `ALL` is the authority on what the statusline can show.
            if TuiStatuslineItem::ALL.contains(&item) && !order.contains(&item) {
                order.push(item);
            }
        }

        let mut enabled = Vec::with_capacity(self.enabled.len());
        for item in self.enabled.iter().copied() {
            if order.contains(&item) && !enabled.contains(&item) {
                enabled.push(item);
            }
        }

        Self { order, enabled }
    }

    pub fn is_enabled(&self, item: TuiStatuslineItem) -> bool {
        self.enabled.contains(&item)
    }
}

/// Tracks the state of the quota reset banner
#[derive(
    Debug,
    Serialize,
    Deserialize,
    Clone,
    PartialEq,
    Default,
    schemars::JsonSchema,
    settings_value::SettingsValue,
)]
#[schemars(description = "State of the quota reset banner.")]
pub struct BannerState {
    #[serde(default)]
    #[schemars(description = "Whether the banner has been dismissed.")]
    pub dismissed: bool,
}

/// Tracks information about a single billing cycle for AI request usage
#[derive(
    Debug,
    Serialize,
    Deserialize,
    Clone,
    PartialEq,
    schemars::JsonSchema,
    settings_value::SettingsValue,
)]
#[schemars(description = "Information about a single billing cycle.")]
pub struct CycleInfo {
    /// End date of the billing cycle
    #[schemars(description = "End date of the billing cycle.")]
    pub end_date: DateTime<Utc>,
    /// Whether the quota was exceeded in this cycle
    #[schemars(description = "Whether the usage quota was exceeded in this cycle.")]
    pub was_quota_exceeded: bool,
    /// State of the quota reset banner
    #[schemars(description = "State of the quota reset banner for this cycle.")]
    pub banner_state: BannerState,
}

#[derive(
    Debug,
    Serialize,
    Deserialize,
    Clone,
    Default,
    PartialEq,
    schemars::JsonSchema,
    settings_value::SettingsValue,
)]
#[schemars(description = "AI usage quota information across billing cycles.")]
pub struct AIRequestQuotaInfo {
    /// History of billing cycles and their usage.
    ///
    /// Note that these are only populated going forward from when this setting
    /// was introduced.
    #[schemars(description = "History of billing cycles and their quota usage.")]
    pub cycle_history: Vec<CycleInfo>,
}

#[derive(
    Debug,
    Serialize,
    Deserialize,
    Clone,
    Copy,
    Default,
    PartialEq,
    EnumIter,
    schemars::JsonSchema,
    settings_value::SettingsValue,
)]
#[schemars(
    description = "File read permission level for the agent.",
    rename_all = "snake_case"
)]
pub enum AgentModeCodingPermissionsType {
    /// Agent Mode must ask for explicit permission for any type of file read.
    #[default]
    AlwaysAskBeforeReading,
    /// Agent Mode can always read files without explicit consent.
    AlwaysAllowReading,
    /// Agent Mode can only read certain files without explicit consent.
    ///
    /// The specific filepaths are backed by the
    /// [`AISettings::agent_mode_coding_file_read_allowlist`] setting.
    AllowReadingSpecificFiles,
}

/// Predicate types to match commands that can be executed by Agent Mode.
#[derive(Debug, Serialize, Deserialize, Clone)]
enum AgentModeCommandExecutionPredicateType {
    /// A regex with start (`^`) and end (`$`) anchors.
    ///
    /// We want regex rules to apply to the entire cmd string so we anchor them
    /// (there isn't any efficient way to apply to the entire cmd string at match-time).
    #[serde(with = "serde_regex")]
    AnchoredRegex(Regex),
}

impl AgentModeCommandExecutionPredicateType {
    fn new_regex(regex: &str) -> Result<Self, regex::Error> {
        // Redundant anchors aren't a problem so we can unconditionally add them.
        let anchored_regex = Regex::new(&format!("^{regex}$"))?;
        Ok(Self::AnchoredRegex(anchored_regex))
    }

    fn matches(&self, cmd: &str) -> bool {
        match self {
            Self::AnchoredRegex(regex) => regex.is_match(cmd),
        }
    }
}

impl PartialEq for AgentModeCommandExecutionPredicateType {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::AnchoredRegex(a), Self::AnchoredRegex(b)) => {
                // Indexing should be safe since they're guaranteed to have at least
                // the anchors around them.
                let a_unanchored = &a.as_str()[1..a.as_str().len() - 1];
                let b_unanchored = &b.as_str()[1..b.as_str().len() - 1];
                a_unanchored == b_unanchored
            }
        }
    }
}

impl std::fmt::Display for AgentModeCommandExecutionPredicateType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AnchoredRegex(regex) => {
                write!(f, "{}", &regex.as_str()[1..regex.as_str().len() - 1])
            }
        }
    }
}

/// A wrapper around [`AgentModeCommandExecutionPredicateType`] to enforce
/// the use of the provided constructors rather than direct construction of the variants.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(transparent)]
pub struct AgentModeCommandExecutionPredicate(AgentModeCommandExecutionPredicateType);

impl schemars::JsonSchema for AgentModeCommandExecutionPredicate {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("AgentModeCommandExecutionPredicate")
    }

    fn json_schema(r#gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
        // In the settings file, predicates are serialized as plain regex strings.
        r#gen.subschema_for::<String>()
    }
}

impl AgentModeCommandExecutionPredicate {
    pub fn new_regex(regex: &str) -> Result<Self, regex::Error> {
        Ok(Self(AgentModeCommandExecutionPredicateType::new_regex(
            regex,
        )?))
    }

    pub fn matches(&self, cmd: &str) -> bool {
        self.0.matches(cmd)
    }
}

impl std::fmt::Display for AgentModeCommandExecutionPredicate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl settings_value::SettingsValue for AgentModeCommandExecutionPredicate {
    fn to_file_value(&self) -> serde_json::Value {
        serde_json::Value::String(self.to_string())
    }

    fn from_file_value(value: &serde_json::Value) -> Option<Self> {
        value.as_str().and_then(|s| Self::new_regex(s).ok())
    }
}

lazy_static! {
    // Matches optional args / options for a top-level command.
    static ref OPTIONAL_ARGS_REGEX: Regex = Regex::new(r"(\s.*)?").expect("Can parse optional args regex");
}

cfg_if! {
    // Compiling the regexes for the default command execution allowlist/denylist can be slow
    // in an unoptimized build, so we use empty lists in unit tests.
    if #[cfg(test)] {
        lazy_static! {
            pub static ref DEFAULT_COMMAND_EXECUTION_ALLOWLIST: Vec<AgentModeCommandExecutionPredicate> = vec![];
            pub static ref DEFAULT_COMMAND_EXECUTION_DENYLIST: Vec<AgentModeCommandExecutionPredicate> = vec![];
        }
    } else {
        lazy_static! {
            pub static ref DEFAULT_COMMAND_EXECUTION_ALLOWLIST: Vec<AgentModeCommandExecutionPredicate> = vec![
                AgentModeCommandExecutionPredicate::new_regex(&format!("cat{}", OPTIONAL_ARGS_REGEX.as_str())).expect("Can parse default cat rule into regex"),
                AgentModeCommandExecutionPredicate::new_regex(&format!("echo{}", OPTIONAL_ARGS_REGEX.as_str())).expect("Can parse default echo rule into regex"),
                AgentModeCommandExecutionPredicate::new_regex("find .*").expect("Can parse default find rule into regex"),
                AgentModeCommandExecutionPredicate::new_regex(&format!("grep{}", OPTIONAL_ARGS_REGEX.as_str())).expect("Can parse default grep rule into regex"),
                AgentModeCommandExecutionPredicate::new_regex(&format!("ls{}", OPTIONAL_ARGS_REGEX.as_str())).expect("Can parse default ls rule into regex"),
                AgentModeCommandExecutionPredicate::new_regex("which .*").expect("Can parse default which rule into regex"),
            ];

            pub static ref DEFAULT_COMMAND_EXECUTION_DENYLIST: Vec<AgentModeCommandExecutionPredicate> = vec![
                AgentModeCommandExecutionPredicate::new_regex(&format!("bash{}", OPTIONAL_ARGS_REGEX.as_str())).expect("Can parse default bash rule into regex"),
                AgentModeCommandExecutionPredicate::new_regex(&format!("fish{}", OPTIONAL_ARGS_REGEX.as_str())).expect("Can parse default fish rule into regex"),
                AgentModeCommandExecutionPredicate::new_regex(&format!("pwsh{}", OPTIONAL_ARGS_REGEX.as_str())).expect("Can parse default pwsh rule into regex"),
                AgentModeCommandExecutionPredicate::new_regex(&format!("sh{}", OPTIONAL_ARGS_REGEX.as_str())).expect("Can parse default sh rule into regex"),
                AgentModeCommandExecutionPredicate::new_regex(&format!("zsh{}", OPTIONAL_ARGS_REGEX.as_str())).expect("Can parse default zsh rule into regex"),
                AgentModeCommandExecutionPredicate::new_regex(&format!("curl{}", OPTIONAL_ARGS_REGEX.as_str())).expect("Can parse default curl rule into regex"),
                AgentModeCommandExecutionPredicate::new_regex(&format!("eval{}", OPTIONAL_ARGS_REGEX.as_str())).expect("Can parse default eval rule into regex"),
                AgentModeCommandExecutionPredicate::new_regex(&format!("exec{}", OPTIONAL_ARGS_REGEX.as_str())).expect("Can parse default exec rule into regex"),
                AgentModeCommandExecutionPredicate::new_regex(&format!("source{}", OPTIONAL_ARGS_REGEX.as_str())).expect("Can parse default source rule into regex"),
                AgentModeCommandExecutionPredicate::new_regex(&format!("wget{}", OPTIONAL_ARGS_REGEX.as_str())).expect("Can parse default wget rule into regex"),
                AgentModeCommandExecutionPredicate::new_regex(&format!("dig{}", OPTIONAL_ARGS_REGEX.as_str())).expect("Can parse default dig rule into regex"),
                AgentModeCommandExecutionPredicate::new_regex(&format!("nslookup{}", OPTIONAL_ARGS_REGEX.as_str())).expect("Can parse default nslookup rule into regex"),
                AgentModeCommandExecutionPredicate::new_regex(&format!("host{}", OPTIONAL_ARGS_REGEX.as_str())).expect("Can parse default host rule into regex"),
                AgentModeCommandExecutionPredicate::new_regex(&format!("ssh{}", OPTIONAL_ARGS_REGEX.as_str())).expect("Can parse default ssh rule into regex"),
                AgentModeCommandExecutionPredicate::new_regex(&format!("scp{}", OPTIONAL_ARGS_REGEX.as_str())).expect("Can parse default scp rule into regex"),
                AgentModeCommandExecutionPredicate::new_regex(&format!("rsync{}", OPTIONAL_ARGS_REGEX.as_str())).expect("Can parse default rsync rule into regex"),
                AgentModeCommandExecutionPredicate::new_regex(&format!("telnet{}", OPTIONAL_ARGS_REGEX.as_str())).expect("Can parse default telnet rule into regex"),
                AgentModeCommandExecutionPredicate::new_regex(&format!("rm{}", OPTIONAL_ARGS_REGEX.as_str())).expect("Can parse default rm rule into regex"),
            ];
        }
    }
}

// ---------------------------------------------------------------------------
// Custom agent provider configuration (in-process provider)
// ---------------------------------------------------------------------------

/// The protocol types supported by agent providers.
///
/// Phase one only supports the OpenAI-compatible protocol (suitable for OpenAI, DeepSeek,
/// Zhipu GLM, Moonshot, the DashScope-OpenAI-compatible endpoint for Qwen, SiliconFlow,
/// OpenRouter, any OpenAI-compatible local service, etc). Anthropic, Google, and Bedrock can be
/// added here later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum AgentProviderKind {
    /// The OpenAI-compatible Chat Completions / `/v1/models` protocol.
    #[default]
    OpenAiCompatible,
}

/// The actual API protocol type used by a BYOP provider — explicitly specified, and mapped
/// one-to-one by chat_stream via genai's `ServiceTargetResolver` to the corresponding
/// `AdapterKind`, completely bypassing the default "infer from model name" behavior to avoid
/// misidentification.
///
/// **Note**: this is a finer-grained dimension relative to [`AgentProviderKind`].
/// `AgentProviderKind` currently only has `OpenAiCompatible` (semantically "user-managed
/// endpoint"); `AgentProviderApiType` determines which native protocol genai uses to
/// serialize requests / parse responses.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum AgentProviderApiType {
    /// OpenAI Chat Completions (`POST /v1/chat/completions`).
    /// Suitable for: official OpenAI, DeepSeek, SiliconFlow, OpenRouter, Zhipu GLM, Moonshot,
    /// DashScope-OpenAI-compatible, local vLLM/llama.cpp, etc.
    #[default]
    OpenAi,
    /// The OpenAI Responses API (`POST /v1/responses`).
    /// Suitable for: newer models like GPT-5 / Codex / Pro.
    OpenAiResp,
    /// Google Gemini's native protocol (generativelanguage.googleapis.com).
    Gemini,
    /// The Anthropic Messages API native protocol (`POST /v1/messages`, defaults to
    /// `api.anthropic.com/v1/`).
    Anthropic,
    /// Ollama's native protocol (local or self-hosted Ollama).
    Ollama,
    /// DeepSeek's native protocol. Compared to OpenAI-compatible: multi-turn thinking mode must
    /// carry the `reasoning_content` field back to the server (otherwise a 400 is returned);
    /// only the genai DeepSeek adapter handles this non-standard field. Thinking-mode models
    /// like `deepseek-reasoner` / `deepseek-v4-flash` must select this type; plain chat models
    /// (`deepseek-chat`) also work fine with OpenAI selected.
    DeepSeek,
    /// Google Vertex AI (`{location}-aiplatform.googleapis.com`). Serves both Gemini
    /// (`publishers/google`) and Claude (`publishers/anthropic`) models, routed by model name.
    /// Unlike the other types the "api key" is a short-lived GCP OAuth2 bearer token, minted
    /// from Application Default Credentials (gcloud) or a service-account JSON; the endpoint is
    /// built from `vertex_project` + `vertex_location` rather than `base_url`.
    Vertex,
}

/// Provider-level reasoning effort (thinking depth) preference.
///
/// Semantics:
/// - `Auto` (default): does not pass an effort value to genai. The OpenAI / Anthropic adapters
///   infer it automatically from the model name suffix (`-low` / `-high` / `-zero`, etc.);
///   Gemini / DeepSeek do not infer it.
/// - `Off`: explicitly sends `none` for models that support reasoning, disabling the chain of
///   thought.
/// - Other levels: the client first checks with `reasoning::model_supports_reasoning` and
///   **only injects the value when the model supports it**, to avoid injecting a thinking
///   parameter into older models like claude-3-5-haiku / gpt-4o / gemini-1.5-pro and getting a
///   400 from upstream.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
    EnumIter,
)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningEffortSetting {
    #[default]
    Auto,
    Off,
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
    Max,
}

impl ReasoningEffortSetting {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Off => "Off",
            Self::Minimal => "Minimal",
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::XHigh => "XHigh",
            Self::Max => "Max",
        }
    }

    /// Converts to genai's `ReasoningEffort`. `Auto` returns None (the caller should not set it).
    pub fn to_genai(self) -> Option<genai::chat::ReasoningEffort> {
        use genai::chat::ReasoningEffort as GE;
        Some(match self {
            Self::Auto => return None,
            Self::Off => GE::Zero,
            Self::Minimal => GE::Minimal,
            Self::Low => GE::Low,
            Self::Medium => GE::Medium,
            Self::High => GE::High,
            Self::XHigh => GE::XHigh,
            Self::Max => GE::Max,
        })
    }
}

impl AgentProviderApiType {
    /// The display text for the settings UI dropdown.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::OpenAi => "OpenAI",
            Self::OpenAiResp => "OpenAI-Response",
            Self::Gemini => "Gemini",
            Self::Anthropic => "Anthropic",
            Self::Ollama => "Ollama",
            Self::DeepSeek => "DeepSeek",
            Self::Vertex => "Vertex AI",
        }
    }

    /// Reverse-parses a Debug-format name (`OpenAi` / `DeepSeek` etc), used to hydrate composite
    /// `<api_type>:<model_id>` keys such as those in BYOPLastUsedReasoningMap. Returns None for
    /// an unrecognized string.
    pub fn from_debug_str(s: &str) -> Option<Self> {
        Some(match s {
            "OpenAi" => Self::OpenAi,
            "OpenAiResp" => Self::OpenAiResp,
            "Gemini" => Self::Gemini,
            "Anthropic" => Self::Anthropic,
            "Ollama" => Self::Ollama,
            "DeepSeek" => Self::DeepSeek,
            "Vertex" => Self::Vertex,
            _ => return None,
        })
    }

    /// The default endpoint used when the user hasn't filled in a base_url. The UI can call
    /// this method to pre-fill a value when creating a new provider / switching ApiType, which
    /// is friendly to new users.
    ///
    /// **Must end with `/`**: internally, genai 0.6.x's adapters concatenate the service path
    /// using `format!("{base_url}messages")` / `Url::join`; a missing trailing `/` produces a
    /// garbled address (for Anthropic it's concatenated directly as `.devmessages`) or has the
    /// last path segment eaten by `Url::join`. The client's `build_client` also has a fallback
    /// that appends `/`, but an explicit trailing `/` is still required here, so that even if
    /// the client-side fallback is bypassed, the UI's pre-filled value written to settings.toml
    /// is still correct.
    pub fn default_base_url(&self) -> &'static str {
        match self {
            Self::OpenAi => "https://api.openai.com/v1/",
            Self::OpenAiResp => "https://api.openai.com/v1/",
            Self::Gemini => "https://generativelanguage.googleapis.com/v1beta/",
            Self::Anthropic => "https://api.anthropic.com/v1/",
            Self::Ollama => "http://localhost:11434/",
            Self::DeepSeek => "https://api.deepseek.com/v1/",
            // Vertex has no static base URL — the endpoint is built from vertex_project +
            // vertex_location (see AgentProvider::resolved_base_url). The UI hides base_url for
            // this type and collects project/location instead.
            Self::Vertex => "",
        }
    }

    /// Whether this provider type derives its endpoint from `vertex_project`/`vertex_location`
    /// rather than a user-entered `base_url`, and mints an OAuth2 bearer instead of using a
    /// static api key. Kept as a method so match sites read intent rather than a bare
    /// `== Vertex`.
    pub fn is_vertex(&self) -> bool {
        matches!(self, Self::Vertex)
    }
}

/// A single user-defined agent provider configuration.
///
/// `api_key` is **not** stored here — it lives in the `AgentProviderSecrets` singleton (secure
/// storage), associated via `id`. This way the settings file (settings.toml) never leaks
/// sensitive information.
// `Eq` is deliberately absent (it used to be derived alongside `PartialEq`): `token_price`
// carries `f64` USD rates, and a decimal is the only unit a user can enter without converting
// by hand. Nothing compares providers as `Eq` — the derive was incidental.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentProvider {
    /// The provider's unique ID, generated on first creation and persisted in settings as the
    /// key associating it with its secret.
    #[serde(default = "AgentProvider::default_id")]
    pub id: String,

    /// The display name the user gave this provider (e.g. "DeepSeek Official", "Local Ollama").
    pub name: String,

    /// The protocol type, currently fixed to OpenAI-compatible (semantically "user-managed
    /// endpoint"). The actual request/response serialization protocol is determined by
    /// [`AgentProvider::api_type`].
    #[serde(default)]
    pub kind: AgentProviderKind,

    /// The explicitly specified API protocol type (OpenAI / OpenAI-Response / Gemini /
    /// Anthropic / Ollama). Old configs (lacking this field) deserialize to `OpenAi` for
    /// backward-compatible semantics.
    #[serde(default)]
    pub api_type: AgentProviderApiType,

    /// The API base URL, e.g. `https://api.deepseek.com/v1`, `http://localhost:11434`.
    /// Should not have a trailing slash, but the code tolerates it.
    pub base_url: String,

    /// The list of models the user has configured to be exposed for the agent to pick from.
    /// Each entry contains both `name` (display name) and `id` (the value sent to upstream in
    /// the API's model field).
    #[serde(default)]
    pub models: Vec<AgentProviderModel>,

    /// Extra HTTP request headers, merged into the request one by one when sending to the
    /// upstream provider. Used for gateways that require additional routing headers (e.g.
    /// Portkey's `x-portkey-provider`).
    /// `api_key` still goes through the standard `Authorization: Bearer` path.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_headers: Vec<(String, String)>,

    /// Vertex AI only: the GCP project id (e.g. `my-project-123`). Ignored for other api types.
    /// A `base_url` cannot carry this, so it is stored as its own field; the endpoint is built
    /// by [`AgentProvider::resolved_base_url`].
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub vertex_project: String,

    /// Vertex AI only: the GCP location / region (e.g. `us-east5`, `global`). Empty falls back
    /// to `global`. Ignored for other api types.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub vertex_location: String,

    /// Whether this provider is temporarily excluded from the model picker without deleting
    /// its configuration or stored API key. Unlike removal, toggling this back off instantly
    /// restores the provider exactly as it was. See [`AgentProvider::is_usable`].
    #[serde(default, skip_serializing_if = "is_false")]
    pub disabled: bool,

    /// The provider-wide default token price used by `/cost`, applied to every model of this
    /// provider that does not carry its own [`AgentProviderModel::token_price`].
    ///
    /// `None` means "no rate configured": `/cost` then reports token counts only and says so,
    /// rather than substituting a guessed rate or a misleading `$0.00`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_price: Option<TokenPrice>,
}

/// User-configured token prices, in **US dollars per one million tokens** — the unit every
/// provider publishes its price list in, so the numbers can be copied across verbatim.
///
/// BYOP divergence from Warp, and why it is acceptable: Warp bills through a hosted
/// subscription and its server returns an authoritative `cost_in_cents` per request, so its
/// client never needs a price table. This fork has no billing relationship with the user's
/// provider — it only sees token counts on the wire. Shipping a built-in price table would go
/// stale silently and produce confidently wrong money figures, so the rate is the user's to
/// state: they are the one holding the invoice.
///
/// Serializes to toml like:
/// ```toml
/// [agent_providers.token_price]
/// input_usd_per_million_tokens = 3.0
/// output_usd_per_million_tokens = 15.0
/// cache_read_usd_per_million_tokens = 0.3
/// cache_write_usd_per_million_tokens = 3.75
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TokenPrice {
    /// USD per million *uncached* input (prompt) tokens.
    pub input_usd_per_million_tokens: f64,

    /// USD per million output (completion) tokens.
    pub output_usd_per_million_tokens: f64,

    /// USD per million input tokens served from the provider's prompt cache. `None` falls back
    /// to [`Self::input_usd_per_million_tokens`]; `/cost` says when it did, because for
    /// providers that discount cache reads heavily (Anthropic bills them at 0.1x) that
    /// fallback over-reports.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_usd_per_million_tokens: Option<f64>,

    /// USD per million input tokens written into the provider's prompt cache. `None` falls
    /// back to [`Self::input_usd_per_million_tokens`], with the same caveat as
    /// [`Self::cache_read_usd_per_million_tokens`] (Anthropic bills cache writes at 1.25x).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write_usd_per_million_tokens: Option<f64>,
}

impl TokenPrice {
    /// Builds a price from the two rates the settings UI exposes, or `None` when neither was
    /// entered. A rate of exactly `0.0` is kept — a genuinely free endpoint is a real answer,
    /// distinct from "unconfigured", and `/cost` labels the two differently.
    pub fn from_input_output(
        input_usd_per_million_tokens: Option<f64>,
        output_usd_per_million_tokens: Option<f64>,
    ) -> Option<Self> {
        if input_usd_per_million_tokens.is_none() && output_usd_per_million_tokens.is_none() {
            return None;
        }
        Some(Self {
            input_usd_per_million_tokens: input_usd_per_million_tokens.unwrap_or(0.0),
            output_usd_per_million_tokens: output_usd_per_million_tokens.unwrap_or(0.0),
            cache_read_usd_per_million_tokens: None,
            cache_write_usd_per_million_tokens: None,
        })
    }

    /// The rate charged for cache-read input tokens, and whether it was configured explicitly
    /// (`false` means it fell back to the plain input rate).
    pub fn cache_read_rate(&self) -> (f64, bool) {
        match self.cache_read_usd_per_million_tokens {
            Some(rate) => (rate, true),
            None => (self.input_usd_per_million_tokens, false),
        }
    }

    /// The rate charged for cache-write input tokens, and whether it was configured explicitly
    /// (`false` means it fell back to the plain input rate).
    pub fn cache_write_rate(&self) -> (f64, bool) {
        match self.cache_write_usd_per_million_tokens {
            Some(rate) => (rate, true),
            None => (self.input_usd_per_million_tokens, false),
        }
    }
}

impl settings_value::SettingsValue for TokenPrice {}

impl AgentProvider {
    fn default_id() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    /// Constructs a new, empty provider. `disabled` starts `false` (not explicitly turned
    /// off) -- see [`AgentProvider::effectively_disabled`] for why it still shows up as
    /// disabled in the UI until it's configured.
    pub fn new_empty() -> Self {
        Self {
            id: Self::default_id(),
            name: String::new(),
            kind: AgentProviderKind::default(),
            api_type: AgentProviderApiType::default(),
            base_url: String::new(),
            models: Vec::new(),
            extra_headers: Vec::new(),
            vertex_project: String::new(),
            vertex_location: String::new(),
            disabled: false,
            token_price: None,
        }
    }

    /// The endpoint base URL genai should target, accounting for provider-type specifics.
    ///
    /// For [`AgentProviderApiType::Vertex`] this is derived from `vertex_project` +
    /// `vertex_location` into the aiplatform URL genai's Vertex adapter expects:
    /// `https://{location}-aiplatform.googleapis.com/v1/projects/{project}/locations/{location}/`
    /// (falling back to the `global` host/location when no location is set). For every other
    /// type it is just `base_url` unchanged, so existing behavior is untouched.
    pub fn resolved_base_url(&self) -> String {
        if self.api_type.is_vertex() {
            vertex_endpoint_url(&self.vertex_project, &self.vertex_location)
        } else {
            self.base_url.clone()
        }
    }

    /// Whether this provider has a configured endpoint: `vertex_project` for
    /// [`AgentProviderApiType::Vertex`], `base_url` otherwise.
    fn has_endpoint(&self) -> bool {
        if self.api_type.is_vertex() {
            !self.vertex_project.trim().is_empty()
        } else {
            !self.base_url.trim().is_empty()
        }
    }

    /// Validates the fields required for this provider's api type, catching problems that
    /// [`Self::has_endpoint`]/[`Self::is_usable`] would otherwise swallow silently -- an
    /// invalid provider just never shows up in the model picker, with no indication why.
    ///
    /// Currently the only case: [`AgentProviderApiType::Vertex`] requires a non-empty
    /// `vertex_project`, since [`vertex_endpoint_url`] interpolates it directly into the URL
    /// path (`.../projects/{project}/locations/.../`); an empty project produces a malformed
    /// `.../projects//locations/.../` endpoint. Save-time callers (`ai_page.rs`) surface the
    /// returned message as an error toast instead of persisting an unusable provider without
    /// feedback.
    pub fn validation_error(&self) -> Option<String> {
        if self.api_type.is_vertex() && self.vertex_project.trim().is_empty() {
            return Some(crate::t!(
                "settings-agent-providers-vertex-project-required"
            ));
        }
        None
    }

    /// Whether this provider should be treated as off: either the user explicitly disabled
    /// it, every model it has (including the case of having none at all) is unable to serve
    /// anything, or it has no configured endpoint. All three are deliberately *computed*,
    /// live checks rather than a flag set once at creation -- a freshly added provider
    /// starts this way automatically, and the moment it's given a usable model and an
    /// endpoint it graduates back to enabled on its own, no separate "Enable" click needed.
    /// A provider with at least one enabled model and an endpoint stays enabled until
    /// someone explicitly disables it; this never auto-flips `disabled` itself.
    ///
    /// `models.iter().all(...)` is vacuously `true` for an empty list, so this also covers
    /// the no-models-yet case without a separate check -- and it means bulk-disabling every
    /// individual model (e.g. "Disable shown" with no search filter) is equivalent to
    /// disabling the provider itself, rather than leaving it looking active while
    /// contributing zero models to the picker.
    ///
    /// This is the single predicate the Settings UI uses to decide whether a provider card
    /// belongs in the main list or the collapsed "Disabled providers" section -- it must
    /// agree with [`Self::is_usable`] on "no endpoint" or a provider can render as active in
    /// Settings while being silently excluded from the model picker.
    pub fn effectively_disabled(&self) -> bool {
        self.disabled || self.models.iter().all(|m| m.disabled) || !self.has_endpoint()
    }

    /// Whether this provider has an endpoint, at least one enabled model, and isn't
    /// disabled — i.e. whether it should show up in the model picker. Used by both
    /// `build_byop_llm_infos` (building the picker) and `lookup_byop` (resolving an
    /// already-selected model at request time), so the two paths can never disagree about
    /// which providers are live. Equivalent to `!effectively_disabled()`, kept as a
    /// separately-named predicate since "usable" reads better at its call sites than a
    /// double negative would.
    pub fn is_usable(&self) -> bool {
        !self.effectively_disabled()
    }
}

/// Builds the Vertex AI aiplatform endpoint URL from a project id and location.
///
/// Mirrors genai's `VertexAdapter::default_endpoint`: a set location routes to the regional
/// host `{location}-aiplatform.googleapis.com/.../locations/{location}/`, while an empty
/// location falls back to the global host `aiplatform.googleapis.com/.../locations/global/`.
/// Always ends with `/` (genai appends `publishers/...` onto it).
pub fn vertex_endpoint_url(project: &str, location: &str) -> String {
    let project = project.trim();
    // GCP region ids are lowercase and are interpolated straight into the host
    // (`{location}-aiplatform.googleapis.com`). Normalize case so a value entered
    // as "Global" or "US-EAST5" (as GCP consoles often display them) still routes
    // to a valid host instead of a bogus one.
    let location = location.trim().to_ascii_lowercase();
    if location.is_empty() || location == "global" {
        format!("https://aiplatform.googleapis.com/v1/projects/{project}/locations/global/")
    } else {
        format!(
            "https://{location}-aiplatform.googleapis.com/v1/projects/{project}/locations/{location}/"
        )
    }
}

/// Which native provider family a Vertex model routes to. Vertex serves both
/// Gemini (`publishers/google`) and Claude (`publishers/anthropic`) models; the
/// reasoning tiers and attachment caps follow that family. Kept in one place so
/// the two capability surfaces can't drift apart.
pub fn vertex_model_family(model_id: &str) -> AgentProviderApiType {
    if model_id.to_ascii_lowercase().contains("claude") {
        AgentProviderApiType::Anthropic
    } else {
        AgentProviderApiType::Gemini
    }
}

impl settings_value::SettingsValue for AgentProvider {}

/// A single model entry: `name` is the display name the user sees in the model picker,
/// `id` is the actual `model` field value sent to the upstream OpenAI-compatible API.
///
/// Serializes to toml like:
/// ```toml
/// [[agent_providers.models]]
/// name = "DS V3 General"
/// id   = "deepseek-chat"
/// ```
///
/// Deserialization is backward compatible with the old format `models = ["deepseek-chat",
/// "deepseek-coder"]` (each string treated as `{ name = id, id = id }`), so existing users can
/// upgrade painlessly.
// `Eq` is deliberately absent here for the same reason as on [`AgentProvider`]: `token_price`
// carries `f64` USD rates.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct AgentProviderModel {
    pub name: String,
    pub id: String,

    /// Context window (tokens). Source: filled in by the user, or auto-populated from
    /// models.dev. 0 means unknown — chat_stream falls back to doing no active truncation,
    /// leaving errors entirely to the upstream service.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub context_window: u32,

    /// Maximum output tokens per request. 0 means unspecified.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub max_output_tokens: u32,

    /// Whether reasoning (thinking/CoT) output is supported.
    #[serde(default, skip_serializing_if = "is_false")]
    pub reasoning: bool,

    /// Whether function/tool calling is supported.
    /// Defaults to `true` — when upgrading old configs or when the user manually adds a new
    /// model, tools should not be disabled by default; models that don't support tool calling
    /// get an explicit `false` from models.dev data.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub tool_call: bool,

    // ----- Multi-modal attachment capability, tri-state semantics:
    // - `None` (toml field absent) = Auto: inferred at runtime via the models.dev catalog ->
    //   substring fallback
    // - `Some(true)` = Force-On: user forces it on, bypassing inference
    // - `Some(false)` = Force-Off: user forces it off
    //
    // The field is deliberately named `image` rather than `vision`, matching models.dev's
    // `modalities.input: ["image"]` literally, keeping the semantics as narrow and unambiguous
    // as possible (to avoid users mistakenly thinking vision = image+pdf+...).
    /// Whether image input is supported (image/* MIME).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<bool>,
    /// Whether PDF document input is supported (application/pdf).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pdf: Option<bool>,
    /// Whether audio input is supported (audio/* MIME).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<bool>,

    /// Whether this specific model is excluded from the model picker, independent of its
    /// provider's own `AgentProvider::disabled`. Lets a provider with a large catalog (some
    /// have 200-300 models) be curated down to the handful actually wanted, without deleting
    /// the rest -- "Fetch from API" / "Sync from models.dev" won't re-add them either, since
    /// they're matched by id and skipped when already present.
    #[serde(default, skip_serializing_if = "is_false")]
    pub disabled: bool,

    /// This model's own token price, overriding [`AgentProvider::token_price`] whole when set.
    /// The override is all-or-nothing rather than per-field, so a model's price table is read
    /// exactly as it was entered instead of silently mixing two sources.
    ///
    /// `None` means "fall back to the provider default"; if that is `None` too, `/cost` reports
    /// token counts without a money figure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_price: Option<TokenPrice>,
}

fn is_zero_u32(v: &u32) -> bool {
    *v == 0
}
fn is_false(v: &bool) -> bool {
    !*v
}
fn is_true(v: &bool) -> bool {
    *v
}
fn default_true() -> bool {
    true
}

impl AgentProviderModel {
    pub fn from_id(id: String) -> Self {
        // Trimmed because the id is sent verbatim as the wire `model` field: a stray space
        // in `settings.toml` (or typed into the settings UI, which reaches here through
        // `AddAgentProviderModel`) produced `"model":"gpt-oss:20b "` on every request. It
        // happened to work against Ollama; endpoints that match the model id exactly reject
        // it, and the cause is invisible in any UI that renders the name with padding.
        let id = id.trim().to_owned();
        Self {
            name: id.clone(),
            id,
            context_window: 0,
            max_output_tokens: 0,
            reasoning: false,
            tool_call: true,
            image: None,
            pdf: None,
            audio: None,
            disabled: false,
            token_price: None,
        }
    }

    /// The price to bill this model's tokens at: its own [`Self::token_price`] if set,
    /// otherwise the provider-wide default. `None` when neither is configured.
    pub fn resolved_token_price(&self, provider: &AgentProvider) -> Option<TokenPrice> {
        self.token_price.or(provider.token_price)
    }
}

impl<'de> Deserialize<'de> for AgentProviderModel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Either {
            Plain(String),
            Full {
                #[serde(default)]
                name: String,
                id: String,
                #[serde(default)]
                context_window: u32,
                #[serde(default)]
                max_output_tokens: u32,
                #[serde(default)]
                reasoning: bool,
                #[serde(default = "default_true")]
                tool_call: bool,
                #[serde(default)]
                image: Option<bool>,
                #[serde(default)]
                pdf: Option<bool>,
                #[serde(default)]
                audio: Option<bool>,
                #[serde(default)]
                disabled: bool,
                #[serde(default)]
                token_price: Option<TokenPrice>,
            },
        }
        match Either::deserialize(deserializer)? {
            Either::Plain(id) => Ok(AgentProviderModel::from_id(id)),
            Either::Full {
                name,
                id,
                context_window,
                max_output_tokens,
                reasoning,
                tool_call,
                image,
                pdf,
                audio,
                disabled,
                token_price,
            } => {
                // Same normalization as `from_id` (which the `Plain` arm above goes
                // through): the id reaches the provider as the wire `model` field, so
                // surrounding whitespace in the config must not survive loading. A
                // whitespace-only `name` counts as absent, as an empty one already did.
                let id = id.trim().to_owned();
                let name = name.trim();
                let name = if name.is_empty() {
                    id.clone()
                } else {
                    name.to_owned()
                };
                Ok(AgentProviderModel {
                    name,
                    id,
                    context_window,
                    max_output_tokens,
                    reasoning,
                    tool_call,
                    image,
                    pdf,
                    audio,
                    disabled,
                    token_price,
                })
            }
        }
    }
}

impl settings_value::SettingsValue for AgentProviderModel {}

/// Keys are regex patterns (insertion-ordered), values are serialized CLIAgent names (e.g. "Claude").
/// An empty string value means "Any CLI Agent" (CLIAgent::Unknown).
///
/// Uses `IndexMap` to preserve insertion order so the settings UI list is deterministic.
/// Supports backward-compatible deserialization from the legacy `Vec<String>` format,
/// where each string is converted to a key with an empty agent value.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ToolbarCommandMap(IndexMap<String, String>);

impl ToolbarCommandMap {
    pub(crate) fn new(map: IndexMap<String, String>) -> Self {
        Self(map)
    }
}

impl<'de> Deserialize<'de> for ToolbarCommandMap {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum MapOrVec {
            Map(IndexMap<String, String>),
            Vec(Vec<String>),
        }

        match MapOrVec::deserialize(deserializer) {
            Ok(MapOrVec::Map(map)) => Ok(ToolbarCommandMap::new(map)),
            Ok(MapOrVec::Vec(vec)) => {
                let map = vec
                    .into_iter()
                    .map(|pattern| (pattern, String::new()))
                    .collect();
                Ok(ToolbarCommandMap::new(map))
            }
            Err(e) => Err(e),
        }
    }
}

impl schemars::JsonSchema for ToolbarCommandMap {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("ToolbarCommandMap")
    }

    fn json_schema(r#gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
        r#gen.subschema_for::<HashMap<String, String>>()
    }
}

impl std::ops::Deref for ToolbarCommandMap {
    type Target = IndexMap<String, String>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl settings_value::SettingsValue for ToolbarCommandMap {
    fn to_file_value(&self) -> serde_json::Value {
        serde_json::to_value(&self.0).unwrap_or_default()
    }

    fn from_file_value(value: &serde_json::Value) -> Option<Self> {
        // Try map format first (using from_value to preserve insertion order), then legacy array format.
        if value.is_object() {
            if let Ok(map) = serde_json::from_value::<IndexMap<String, String>>(value.clone()) {
                return Some(ToolbarCommandMap::new(map));
            }
        }
        if let Some(arr) = value.as_array() {
            let result: IndexMap<String, String> = arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| (s.to_string(), String::new())))
                .collect();
            return Some(ToolbarCommandMap::new(result));
        }
        None
    }
}

/// Persistently remembers "the last reasoning effort level used for a given (api_type, model)".
/// Key form: `<api_type>:<model_id>`, e.g. `DeepSeek:deepseek-v4-pro`.
/// The value is the `ReasoningEffortSetting` enum (serde_json snake_case).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BYOPLastUsedReasoningMap(pub IndexMap<String, ReasoningEffortSetting>);

impl BYOPLastUsedReasoningMap {
    pub fn new(map: IndexMap<String, ReasoningEffortSetting>) -> Self {
        Self(map)
    }

    /// Builds the key: `<api_type>:<model_id>`. api_type uses its Debug format to produce a
    /// PascalCase name like `DeepSeek`, independent of ReasoningEffortSetting's serde form.
    pub fn make_key(api_type: AgentProviderApiType, model_id: &str) -> String {
        format!("{api_type:?}:{model_id}")
    }
}

impl std::ops::Deref for BYOPLastUsedReasoningMap {
    type Target = IndexMap<String, ReasoningEffortSetting>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl schemars::JsonSchema for BYOPLastUsedReasoningMap {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "BYOPLastUsedReasoningMap".into()
    }

    fn json_schema(r#gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
        r#gen.subschema_for::<HashMap<String, String>>()
    }
}

impl settings_value::SettingsValue for BYOPLastUsedReasoningMap {
    fn to_file_value(&self) -> serde_json::Value {
        serde_json::to_value(&self.0).unwrap_or_default()
    }

    fn from_file_value(value: &serde_json::Value) -> Option<Self> {
        if value.is_object() {
            if let Ok(map) =
                serde_json::from_value::<IndexMap<String, ReasoningEffortSetting>>(value.clone())
            {
                return Some(Self::new(map));
            }
        }
        None
    }
}

/// Per-agent settings: controls the toolbar, new-tab menu, and titlebar visibility for a single
/// CLI agent. The key is the CLIAgent serialized name (e.g. "Claude", "Gemini").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PerAgentSettings {
    /// Whether to show the coding agent toolbar at the bottom of the terminal input.
    #[serde(default = "default_true_bool")]
    pub toolbar: bool,
    /// Whether to show a quick-launch entry for this agent in the new-tab menu.
    #[serde(default = "default_true_bool", alias = "tab_menu")]
    pub tabmenu: bool,
    /// Whether to show a quick-launch button for this agent on the right side of the titlebar.
    #[serde(default)]
    pub titlebar: bool,
}

fn default_true_bool() -> bool {
    true
}

impl PerAgentSettings {
    /// Returns the default value for the given agent. titlebar defaults to on for
    /// Claude/Codex/Gemini/Antigravity.
    pub fn default_for(agent: CLIAgent) -> Self {
        let titlebar = matches!(
            agent,
            CLIAgent::Claude | CLIAgent::Codex | CLIAgent::Gemini | CLIAgent::Antigravity
        );
        Self {
            toolbar: true,
            tabmenu: true,
            titlebar,
        }
    }
}

impl Default for PerAgentSettings {
    fn default() -> Self {
        Self {
            toolbar: true,
            tabmenu: true,
            titlebar: false,
        }
    }
}

impl settings_value::SettingsValue for PerAgentSettings {}

define_settings_group!(AISettings, settings: [
    // Legacy setting. The Zap Agent is now always enabled; don't use this field to determine
    // enablement status.
    is_any_ai_enabled: IsAnyAIEnabled {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::No),
        private: false,
        toml_path: "agents.warp_agent.is_any_ai_enabled",
        description: "Controls whether all AI features are enabled.",
    },
    // This field should not be referenced directly to lookup active AI enablement -- use the
    // `is_active_ai_enabled()` getter.
    is_active_ai_enabled_internal: IsActiveAIEnabled {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::No),
        private: false,
        toml_path: "agents.warp_agent.active_ai.enabled",
        description: "Controls whether proactive AI features like suggestions are enabled.",
    },
    // This field should not be referenced directly to lookup autodetection enablement -- use the
    // `is_ai_autodetection_enabled()` getter.
    ai_autodetection_enabled_internal: AIAutoDetectionEnabled {
        type: bool,
        // Opt-in, matching the pinned oracle. A fresh user who has never touched
        // this has natural-language detection OFF; the fork had drifted to `true`.
        default: false,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.warp_agent.input.ai_auto_detection_enabled",
        description: "Controls whether AI automatically detects natural language input.",
    },
    // This field should not be referenced directly -- use the
    // `is_nld_in_terminal_enabled()` getter.
    // Controls whether natural language detection is enabled in the terminal input.
    //
    // This is only used when `FeatureFlag::AgentView` is enabled.
    nld_in_terminal_enabled_internal: NLDInTerminalEnabled {
        // openWarp: NLD in terminal defaults to on. When the HeuristicClassifier detects
        // CJK / natural language, it automatically switches to AI input — this is what lets
        // openWarp's Chinese-speaking users type Chinese directly in the terminal as a prompt.
        // Upstream defaults to false because on the cloud path the user first enters AgentView
        // fullscreen, and automatic switching isn't expected in terminal mode; openWarp has no
        // cloud fullscreen entry point, so the terminal is the primary input area.
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.warp_agent.input.nld_in_terminal_enabled",
        description: "Controls whether natural language detection is enabled in the terminal input.",
    },
    autodetection_command_denylist: AICommandDenylist {
        type: String,
        default: String::new(),
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.warp_agent.input.ai_command_denylist",
        description: "Commands to exclude from AI natural language autodetection.",
    },
    // This field should not be referenced directly to lookup intelligent autosuggestion enablement
    // -- use the `is_intelligent_autosuggestions_enabled()` getter.
    intelligent_autosuggestions_enabled_internal: IntelligentAutosuggestionsEnabled {
        type: bool,
        default: true, // TODO(roland): revisit this when launched to stable
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.warp_agent.active_ai.intelligent_autosuggestions_enabled",
        description: "Controls whether AI-powered intelligent autosuggestions are enabled.",
    }
    // This field should not be referenced directly to lookup Prompt Suggestions
    // enablement -- use the `is_prompt_suggestions_enabled()` getter.
    // Note that AgentModeQuerySuggestionsEnabled is a legacy name (the feature was initially named Agent
    // Mode Query Suggestions), however, we do not want to change the name of the setting key to avoid
    // breaking existing user settings.
    prompt_suggestions_enabled_internal: AgentModeQuerySuggestionsEnabled {
        type: bool,
        default: true, // TODO(advait): revisit this when launched to stable
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.warp_agent.active_ai.agent_mode_query_suggestions_enabled",
        description: "Controls whether prompt suggestions are shown in agent mode.",
    }

    // This field should not be referenced directly to lookup Code Suggestions
    // enablement -- use the `is_code_suggestions_enabled()` getter.
    code_suggestions_enabled_internal: CodeSuggestionsEnabled {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.warp_agent.active_ai.code_suggestions_enabled",
        description: "Controls whether AI code suggestions are enabled.",
    }
    // This field should not be referenced directly to lookup natural language autosuggestions
    // enablement -- use the `is_natural_language_autosuggestions_enabled()` getter.
    // This feature refers to ghosted text for AI input queries.
    natural_language_autosuggestions_enabled_internal: NaturalLanguageAutosuggestionsEnabled {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.warp_agent.active_ai.natural_language_autosuggestions_enabled",
        description: "Controls whether ghosted text autosuggestions are shown for AI input queries.",
        feature_flag: FeatureFlag::PredictAMQueries,
    }
    // This field should not be referenced directly to lookup git operations AI autogen
    // enablement -- use the `is_git_operations_autogen_enabled()` getter.
    git_operations_autogen_enabled_internal: GitOperationsAutogenEnabled {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.warp_agent.active_ai.git_operations_autogen_enabled",
        description: "Controls whether AI auto-generates commit messages and PR title/body in the code review dialogs.",
    }
    // This field should not be referenced directly to lookup Rule Suggestions
    // enablement -- use the `is_rule_suggestions_enabled()` getter.
    rule_suggestions_enabled_internal: RuleSuggestionsEnabled {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.warp_agent.active_ai.rule_suggestions_enabled",
        description: "Controls whether the agent suggests rules to save after responses.",
        feature_flag: FeatureFlag::SuggestedRules,
    }
    // This field should not be referenced directly to lookup Voice AI enablement -- use the
    // `is_voice_input_enabled()` getter.
    voice_input_enabled_internal: VoiceInputEnabled {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::DESKTOP,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.voice.voice_input_enabled",
        description: "Controls whether voice input is enabled for AI interactions.",
    },
    // The number of times the user has entered Agent Mode.
    // Not a user-visible setting. We model it so we can show the voice input new feature popup
    // the correct number of times.
    entered_agent_mode_num_times: EnteredAgentModeNumTimes {
        type: usize,
        default: 0,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: true,
    },
    // Whether or not the user has manually dismissed the voice input new feature popup.
    dismissed_voice_input_new_feature_popup: DismissedVoiceInputNewFeaturePopup {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::DESKTOP,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: true,
    },
    // This field is used to store the key used for voice input toggling.
    // Note this is not the named key, but rather corresponds to the physical key.
    voice_input_toggle_key: VoiceInputToggleKey,
    // Ordered visibility configuration for the TUI's bottom statusline.
    // TUI-only and local so separate devices can use different terminal layouts.
    //
    // Upstream tags this `surface: settings::SettingSurfaces::TUI`; Zap's settings macro
    // predates that concept (see app/src/settings/tui_autoupdate.rs), so it's just a regular
    // shared-settings entry here — behaviorally the same since it's only ever read by the TUI.
    tui_statusline: TuiStatusline {
        type: TuiStatuslineConfig,
        default: TuiStatuslineConfig::default(),
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Never,
        private: false,
        toml_path: "agents.statusline",
        description: "Controls the order and visibility of Zap Agent CLI statusline items.",
    },
    // This is not a user-visible setting - it's merely a one-time flag to track if the user has
    // explicitly interacted with voice input. We use this to determine whether we should show a toast
    // to inform the user about voice input and auto-set the keybinding.
    explicitly_interacted_with_voice: ExplicitlyInteractedWithVoice {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::DESKTOP,
        // Never sync to cloud to keep state separate across devices, since microphone access is per-device.
        sync_to_cloud: SyncToCloud::Never,
        private: true,
    },
    // Predicates that Agent Mode can use to decide if it can execute
    // a command without explicit user consent.
    //
    // Prefer [`BlocklistAIPermissions::can_autoexecute_command`] to
    // interpret this allowlist.
    agent_mode_command_execution_allowlist: AgentModeCommandExecutionAllowlist {
        type: Vec<AgentModeCommandExecutionPredicate>,
        default: DEFAULT_COMMAND_EXECUTION_ALLOWLIST.clone(),
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.profiles.agent_mode_command_execution_allowlist",
        description: "Commands that the agent can execute without explicit permission.",
    },
    // Predicates that Agent Mode can use to decide if a command must
    // be executed by the user.
    //
    // Prefer [`BlocklistAIPermissions::can_autoexecute_command`] to
    // interpret this denylist.
    agent_mode_command_execution_denylist: AgentModeCommandExecutionDenylist {
        type: Vec<AgentModeCommandExecutionPredicate>,
        default: DEFAULT_COMMAND_EXECUTION_DENYLIST.clone(),
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.profiles.agent_mode_command_execution_denylist",
        description: "Commands that the agent must always ask before executing.",
    },
    // Enabled iff Agent Mode can execute readonly commands without explicit user consent.
    //
    // Prefer [`BlocklistAIPermissions::can_autoexecute_command`] to
    // interpret this setting.
    agent_mode_execute_read_only_commands: AgentModeExecuteReadonlyCommands {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.profiles.agent_mode_execute_readonly_commands",
        description: "Whether the agent can auto-execute read-only commands without asking.",
    },
    // Determines coding permissions that Agent Mode has.
    // Note that if Agent Mode has permissions to execute readonly commands,
    // that automatically gives Agent Mode the ability to also _read_ files for coding
    // tasks, including codebase search.
    //
    // Prefer [`BlocklistAIPermissions::can_read_file`] to interpret this setting.
    agent_mode_coding_permissions: AgentModeCodingPermissions {
        type: AgentModeCodingPermissionsType,
        default: AgentModeCodingPermissionsType::default(),
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.profiles.agent_mode_coding_permissions",
        description: "The file read permission level for the agent.",
    }
    // Specific filepaths that Agent Mode can read without asking for additional permissions.
    // These should be persisted as absolute filepaths to avoid ambiguity.
    //
    // This is used in conjunction with [`AgentModeCodingPermissionsType::AllowReadingSpecificFiles`]
    // but modelled as a separate setting because it is not cloud-synced.
    //
    // Prefer [`BlocklistAIPermissions::can_read_file`] to interpret this setting.
    agent_mode_coding_file_read_allowlist: AgentModeCodingFileReadAllowlist {
        type: Vec<PathBuf>,
        default: vec![],
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Never,
        private: false,
        toml_path: "agents.profiles.agent_mode_coding_file_read_allowlist",
        description: "File paths the agent can read without asking for permission.",
    }
    // Whether or not the profile-level command autoexecution speedbump has been shown.
    //
    // Not a user-visible setting - we model it as a setting so we can track how often
    // it's shown across devices.
    has_shown_agent_mode_profile_command_autoexecution_speedbump: HasShownAgentModeProfileCommandAutoexecutionSpeedbump {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: true,
    }
    // Whether or not we should show the speedbump for auto-executing readonly cmds.
    //
    // Not a user-visible settings - we model it as a setting so we can track how often
    // it's shown across devices.
    should_show_agent_mode_autoexecute_readonly_commands_speedbump: ShouldShowAgentModeModelExecuteReadonlyCommandsSpeedbump {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: true,
    }
    // Whether or not we should show the speedbump for auto-writing to the PTY.
    //
    // Not a user-visible settings - we model it as a setting so we can track how often
    // it's shown across devices.
    should_show_agent_mode_write_to_pty_speedbump: ShouldShowAgentModeWriteToPtySpeedbump {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: true,
    }
    // Whether or not we should show the speedbump for auto-reading files.
    //
    // Not a user-visible settings - we model it as a setting so we can track how often
    // it's shown across devices.
    should_show_agent_mode_autoread_files_speedbump: ShouldShowAgentModeCodingReadPermissionsNudge {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: true,
    }
    // Whether or not we should show the one-shot speedbump on Ask-User-Question cards.
    //
    // Not a user-visible setting - we model it as a setting so we can track state.
    // Intentionally NOT cloud-synced: we want users to see the first-time nudge on
    // each fresh device, and we avoid a cloud-sync race that would make the flag
    // silently stay `false` on new devices after being consumed once elsewhere.
    should_show_agent_mode_ask_user_question_speedbump: ShouldShowAgentModeAskUserQuestionSpeedbump {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Never,
        private: true,
    }
    // Whether to use locally loaded AWS credentials for Bedrock-enabled requests.
    aws_bedrock_credentials_enabled: AwsBedrockCredentialsEnabled {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::DESKTOP,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "cloud_platform.third_party_api_keys.aws_bedrock_credentials_enabled",
        description: "Whether Zap should use your local AWS credentials for Bedrock-enabled requests.",
    }
    // Whether to automatically run the AWS login command when Bedrock credentials are expired.
    //
    // When true, the configured login command will be run automatically without asking.
    // When false (default), a prompt will be shown asking for permission.
    aws_bedrock_auto_login: AwsBedrockAutoLogin {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::DESKTOP,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "cloud_platform.third_party_api_keys.aws_bedrock_auto_login",
        description: "Whether to automatically run the AWS login command when Bedrock credentials expire.",
    }
    // Command to run to refresh AWS credentials when using Bedrock auto-login.
    aws_bedrock_auth_refresh_command: AwsBedrockAuthRefreshCommand {
        type: String,
        default: "aws login".to_string(),
        supported_platforms: SupportedPlatforms::DESKTOP,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "cloud_platform.third_party_api_keys.aws_bedrock_auth_refresh_command",
        description: "The command to run to refresh AWS credentials for Bedrock.",
    }
    // AWS profile name to use when loading credentials from the local AWS credential/config chain.
    aws_bedrock_profile: AwsBedrockProfile {
        type: String,
        default: "default".to_string(),
        supported_platforms: SupportedPlatforms::DESKTOP,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "cloud_platform.third_party_api_keys.aws_bedrock_profile",
        description: "The AWS profile name to use for Bedrock credentials.",
    }
    // Whether the AWS Bedrock login banner has been permanently dismissed.
    //
    // Not a user-visible setting - we model it as a setting so we can track state.
    aws_bedrock_login_banner_dismissed: AwsBedrockLoginBannerDismissed {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::DESKTOP,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: true,
    }
    // Whether or not the user wants agent mode requests to use their saved rules.
    memory_enabled: MemoryEnabled {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.knowledge.rules_enabled",
        description: "Whether the agent uses your saved rules during requests.",
    }
    // Whether zap drive context should be included in AI requests
    warp_drive_context_enabled: WarpDriveContextEnabled {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.knowledge.warp_drive_context_enabled",
        description: "Whether Library context is included in AI requests.",
    }

    // Whether the agent mode setup banner has been shown for a given repo path.
    // Once shown, it will not be shown again for that repo.
    //
    // Not a user-visible settings - we model it as a setting so we can track state.
    agent_mode_setup_banner_shown_for_repo_paths: AgentModeSetupBannerShownForRepoPaths {
        type: Vec<PathBuf>,
        default: vec![],
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Never,
        private: true,
    }

    // Information about AI request quotas and usage across billing cycles
    ai_request_quota_info: AIRequestQuotaInfoSetting {
        type: AIRequestQuotaInfo,
        default: AIRequestQuotaInfo::default(),
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: true,
    },

    // Whether or not we should show the speedbump for showing code suggestion banners.
    // This includes both passive code diffs and suggested prompts (passive unit tests).
    //
    // Not a user-visible settings - we model it as a setting so we can track if the speedbump has already been shown or not.
    show_code_suggestion_speedbump: ShouldShowCodeSuggestionSpeedbump {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: true,
    }

    mcp_execution_path: MCPExecutionPath {
        type: Option<String>,
        default: None,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Never,
        private: true,
    },

    // This is not a user-visible setting - its merely a one-time flag to track if the agents 3 launch modal
    // has been shown to the user.
    //
    // We model it as a setting so it's only shown once to a given user regardless of the number of
    // devices they use.
    did_check_to_trigger_agents_3_launch_modal: DidShowAgents3LaunchModal {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::No),
        private: true,
    }

    // Whether or not the user has enabled the ability to use Zap credits even when providing
    // their own LLM provider API key.
    can_use_warp_credits_with_byok: CanUseWarpCreditsWithByok {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "cloud_platform.third_party_api_keys.can_use_warp_credits_with_byok",
        description: "Whether Zap credits can be used even when providing your own API key.",
    }

    should_render_use_agent_footer_for_user_commands: ShouldRenderUseAgentToolbarForUserCommands {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.warp_agent.other.should_render_use_agent_toolbar_for_user_commands",
        description: "Whether to show the \"Use Agent\" footer for terminal commands.",
    }

    // Whether to render the CLI agent footer for commands like Claude, Codex, Gemini, etc.
    // This is independent of the "Use Agent" footer setting.
    should_render_cli_agent_footer: ShouldRenderCLIAgentToolbar {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.third_party.should_render_cli_agent_toolbar",
        description: "Whether to show the CLI agent footer for coding agent commands.",
    }
    // When enabled and a CLI agent session has a plugin listener, rich input
    // auto-closes when the session enters a Blocked state (the agent requires
    // direct keyboard interaction) and auto-opens when it leaves Blocked.
    auto_toggle_rich_input: AutoToggleRichInput {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.third_party.auto_toggle_composer",
        description: "Whether CLI agent Rich Input automatically closes and reopens based on the agent's blocked state.",
    }

    // When enabled and a CLI agent session has a plugin listener, rich input
    // auto-opens once when the session starts or when the listener is registered.
    auto_open_rich_input_on_cli_agent_start: AutoOpenRichInputOnCLIAgentStart {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.third_party.auto_open_composer_on_cli_agent_start",
        description: "Whether CLI agent Rich Input automatically opens when a CLI agent session starts.",
    }

    // When enabled and a CLI agent session does NOT have a plugin listener,
    // rich input auto-closes after the user submits a prompt.
    // When the plugin IS present, this setting has no effect (auto-show/hide
    // from auto_toggle_rich_input handles rich input lifecycle).
    auto_dismiss_rich_input_after_submit: AutoDismissRichInputAfterSubmit {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.third_party.auto_dismiss_composer_after_submit",
        description: "Whether CLI agent Rich Input automatically closes after the user submits a prompt.",
    }

    // When enabled, the Rich Input editor submits on Ctrl+Enter instead of Enter.
    // Enter inserts a newline; Ctrl+Enter submits.
    submit_on_ctrl_enter: SubmitRichInputOnCtrlEnter {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.third_party.submit_on_ctrl_enter",
        description: "When enabled, the Rich Input editor submits on Ctrl+Enter instead of Enter. Enter inserts a newline.",
    }

    // Maps custom toolbar command regex patterns to specific CLI agents.
    // Keys are regex patterns matched against the full command string.
    // Values are serialized CLIAgent names (empty string = any agent).
    // Supports migration from the legacy Vec<String> format.
    cli_agent_footer_enabled_commands: CLIAgentToolbarEnabledCommands {
        type: ToolbarCommandMap,
        default: ToolbarCommandMap::default(),
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.third_party.cli_agent_toolbar_enabled_commands",
        max_table_depth: 1,
        description: "Maps custom toolbar command patterns to specific CLI agents.",
    }

    // This is not a user-visible setting - it tracks whether a paid user has dismissed the
    // agent management help page by clicking "View Agents".
    //
    // When false and user is on a paid plan, the help page is shown.
    // When true, the help page is hidden (user dismissed it).
    // Free users never see the help page by default regardless of this setting.
    did_dismiss_cloud_setup_guide: DidDismissAgentManagementHelpPage {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: true,
    }

    // This is not a user-visible setting - it tracks whether the FTU model picker callout
    // has been shown to the user. We set this to `true` as soon as the callout is first
    // displayed (not when it's dismissed), so it never re-appears.
    //
    // Note: this setting was originally named "dismissed" but we now use it to mean "shown".
    // We kept the same setting key so that users who already dismissed the callout on an
    // older client don't see it again.
    ftu_model_callout_dismissed: FtuModelCalloutDismissed {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: true,
    }

    // Tracks whether we've done the one-time auto-open of the conversation list for discoverability.
    // Once set to true, the conversation list visibility will be restored from workspace state.
    has_auto_opened_conversation_list: HasAutoOpenedConversationList {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: true,
    }

    // Whether the ambient agent trial widget has been dismissed by the user.
    //
    // Not a user-visible setting - we model it as a setting so we can track state.
    ambient_agent_trial_widget_dismissed: AmbientAgentTrialWidgetDismissed {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: true,
    }

    // The raw stored default mode for new sessions. Use `default_session_mode()` to retrieve the
    // effective value, which is gated on AI availability.
    default_session_mode_internal: DefaultSessionMode,

    // The file path of the tab config used when default_session_mode_internal is TabConfig.
    // Only read when mode is TabConfig; ignored for all other modes.
    // Machine-local (tab config paths vary per machine), so never synced to cloud.
    default_tab_config_path: DefaultTabConfigPath {
        type: String,
        default: String::new(),
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Never,
        private: false,
        toml_path: "general.default_tab_config_path",
    }

    // Whether file-based MCP servers from third-party AI tools (e.g. Claude, Codex) should
    // be automatically detected and spawned. Zap-native config files (.warp/.mcp.json) are
    // always detected and spawned, regardless of this setting.
    file_based_mcp_enabled: FileBasedMcpEnabled {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::DESKTOP,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.mcp_servers.file_based_mcp_enabled",
        description: "Whether third-party file-based MCP servers are automatically detected.",
    }

    // Controls how agent thinking/reasoning traces are displayed.
    thinking_display_mode: ThinkingDisplayMode,

    // Controls how child-agent (orchestration) message bodies are displayed (#329).
    orchestration_message_display_mode: OrchestrationMessageDisplayMode,

    // Default behavior when the user submits a new prompt while the agent is still
    // responding. Per-conversation overrides live on `QueuedQueryModel`; this
    // setting is the fallback used when a conversation has no explicit override.
    default_prompt_submission_mode: PromptSubmissionMode,

    // What happens when a prompt is submitted while an agent controls an agent-requested
    // long-running command. Only consulted when `default_prompt_submission_mode` is `Interrupt`;
    // per-LRC manual overrides live on `QueuedQueryModel`.
    long_running_command_submission_mode: LongRunningCommandSubmissionMode,

    // Whether agent-executed shell commands should be included in command history
    // (up-arrow, Ctrl-R search, inline history menu).
    // When false, commands run by the AI agent are excluded from history.
    include_agent_commands_in_history: IncludeAgentCommandsInHistory {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.warp_agent.input.include_agent_commands_in_history",
        description: "Whether agent-executed commands are included in command history.",
    }

    // Whether fast forward / auto-approve can run commands that match the command denylist.
    auto_approve_bypasses_command_denylist: AutoApproveBypassesCommandDenylist {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.warp_agent.other.auto_approve_bypasses_command_denylist",
        description: "Whether auto-approve bypasses the command denylist.",
    }

    // Controls whether the conversation history view appears in the tools panel.
    show_conversation_history: ShowConversationHistory {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.warp_agent.other.show_conversation_history",
        description: "Whether conversation history appears in the tools panel.",
    }


    // Controls whether agent notifications (mailbox button, toasts, notification items) are shown.
    show_agent_notifications: ShowAgentNotifications {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.warp_agent.other.show_agent_notifications",
        description: "Whether agent notifications are shown.",
    }

    // Zap T1-2: completed tool cards are hidden by default (aligned with opencode TUI's
    // showDetails behavior). When true, cards such as RequestCommandOutput / ReadFiles /
    // Grep / FileGlob / RequestFileEdits whose status.is_done() is true are hidden by default,
    // keeping only in-progress + error cards, so new content in long sessions isn't buried
    // under a pile of historical cards. The folded state can be toggled from the appearance
    // settings panel.
    hide_completed_tool_cards: HideCompletedToolCards {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.warp_agent.appearance.hide_completed_tool_cards",
        description: "When true, completed tool action cards (read files, grep, search codebase, requested commands, etc.) are hidden after they finish. In-progress and errored cards are always shown. Useful for long sessions to keep focus on the latest activity.",
    }

    // Not a user-visible setting - it tracks which one-time feature-intro popovers
    // (see `FEATURE_INTROS`) the user has already seen, keyed by the feature-intro's
    // stable id key.
    //
    // Modeled as a globally-synced setting (not respecting the user's sync setting) so
    // each feature is announced at most once per user, regardless of how many devices
    // they use. A feature is considered seen when its id is present and mapped to `true`.
    seen_feature_intro_ids: SeenFeatureIntroIds {
        type: HashMap<String, bool>,
        default: HashMap::default(),
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::No),
        private: true,
    }

    // Per-agent, per-host tracking of whether the user dismissed the plugin install chip.
    // Keys are "<agent_prefix>" for local sessions or "<agent_prefix>@<host>" for remote.
    // Local-only so dismissal doesn't sync across devices.
    plugin_install_chip_dismissed_map: PluginInstallChipDismissedMap {
        type: HashMap<String, bool>,
        default: HashMap::default(),
        supported_platforms: SupportedPlatforms::DESKTOP,
        sync_to_cloud: SyncToCloud::Never,
        private: true,
    }

    // Per-agent, per-host tracking of the MINIMUM_PLUGIN_VERSION for which the user
    // dismissed the plugin update chip. Empty/absent means not dismissed.
    // Keys are "<agent_prefix>" for local sessions or "<agent_prefix>@<host>" for remote.
    // Local-only so dismissal doesn't sync across devices.
    plugin_update_chip_dismissed_for_version_map: PluginUpdateChipDismissedForVersionMap {
        type: HashMap<String, String>,
        default: HashMap::default(),
        supported_platforms: SupportedPlatforms::DESKTOP,
        sync_to_cloud: SyncToCloud::Never,
        private: true,
    }

    // The user's custom agent provider list. Phase one only supports the OpenAI-compatible
    // protocol.
    //
    // Note: a provider's `api_key` is not persisted here — see `AgentProviderSecrets`.
    agent_providers: AgentProviders {
        type: Vec<AgentProvider>,
        default: Vec::new(),
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Never,
        private: false,
        toml_path: "agents.warp_agent.providers",
        description: "User-configured custom Agent providers (OpenAI-compatible).",
    }

    // models.dev catalog provider ids where the user has flipped the "Quick add" chip row's
    // default visibility (see `models_dev::is_common_provider` / `effectively_visible`):
    // membership here hides an otherwise-common provider, or pins an otherwise-uncommon one
    // visible. Entries here were never added as a real configured provider -- unlike
    // `agent_providers`, this only affects the browse row.
    catalog_provider_visibility_overrides: CatalogProviderVisibilityOverrides {
        type: Vec<String>,
        default: Vec::new(),
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Never,
        private: false,
        toml_path: "agents.warp_agent.catalog_provider_visibility_overrides",
        description: "Quick-add catalog provider ids with flipped default visibility.",
    }

    // Zap BYOP local conversation compaction — 1:1 aligned with opencode's
    // `Config.compaction.auto`.
    // When true, summarization is triggered automatically on token overflow; when false, it's
    // only triggered manually via /compact /compact-and.
    byop_compaction_auto: ByopCompactionAuto {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.byop_compaction.auto",
        description: "Enable BYOP automatic conversation compaction on context overflow.",
    }

    // Zap BYOP local conversation compaction — 1:1 aligned with opencode's
    // `Config.compaction.prune`.
    // When true, old tool output is cleared (replaced with a placeholder) before every LLM
    // request.
    byop_compaction_prune: ByopCompactionPrune {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.byop_compaction.prune",
        description: "Auto-prune older tool outputs to free BYOP context.",
    }

    // Zap BYOP local conversation compaction — 1:1 aligned with opencode's
    // `Config.compaction.tail_turns` (defaults to 2).
    // Keeps the most recent N user turns as the tail; everything before that goes into the
    // head for the summarization LLM. 0 disables compaction.
    byop_compaction_tail_turns: ByopCompactionTailTurns {
        type: u32,
        default: 2u32,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.byop_compaction.tail_turns",
        description: "Number of recent user turns to keep verbatim during compaction.",
    }

    // Zap BYOP local conversation compaction — 1:1 aligned with
    // `Config.compaction.preserve_recent_tokens`.
    // 0 = compute automatically via the formula (min(MAX=8000, max(MIN=2000, usable * 0.25)));
    // > 0 forces an override.
    byop_compaction_preserve_recent_tokens: ByopCompactionPreserveRecentTokens {
        type: u32,
        default: 0u32,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.byop_compaction.preserve_recent_tokens",
        description: "Override the recent-tokens preservation budget (0 = auto).",
    }

    // Zap BYOP local conversation compaction — 1:1 aligned with `Config.compaction.reserved`.
    // When determining overflow, usable = input_limit - reserved. 0 = computed automatically as
    // min(20_000, max_output).
    byop_compaction_reserved: ByopCompactionReserved {
        type: u32,
        default: 0u32,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.byop_compaction.reserved",
        description: "Reserved buffer tokens for compaction overflow check (0 = auto).",
    }

    // Zap BYOP local conversation compaction — a dedicated summarization model (optional).
    // When set, the summarization LLM call uses this provider+model instead of the current
    // conversation model.
    // Leaving both fields empty = use the conversation's current model.
    byop_compaction_model_provider_id: ByopCompactionModelProviderId {
        type: String,
        default: String::new(),
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.byop_compaction.model.provider_id",
        description: "Optional dedicated provider id for compaction LLM calls.",
    }

    byop_compaction_model_id: ByopCompactionModelId {
        type: String,
        default: String::new(),
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.byop_compaction.model.model_id",
        description: "Optional dedicated model id for compaction LLM calls.",
    }

    // Zap: hot-reload directory for the system prompt template.
    // Empty string = use the built-in template compiled into the binary via `include_str!`
    // (default, zero runtime IO).
    // A directory value = re-read from that directory by template name (a relative path like
    // `system/local.j2`) on every render; edits to the template take effect as soon as they're
    // saved, without recompiling the `app` crate (800k lines — changing one prompt line would
    // otherwise require a full relink).
    // Missing files / syntax errors fall back individually to the built-in version without
    // panicking.
    //
    // sync_to_cloud is Never: this is a local filesystem path, and syncing it to another
    // machine would just point to a nonexistent directory and silently fall back; it also
    // avoids bringing a writable entry point like a "prompt source path" into cloud config.
    // private is false: consistent with the neighboring byop_* settings, this lands in
    // settings.toml for easy manual editing (private: true would instead store it in
    // PrivatePreferences and filter it out of the TOML enumeration).
    // The `ZAP_PROMPT_DIR` environment variable takes priority over this setting (temporary
    // debugging overrides persisted config).
    // Consumer: see `ai::agent_providers::prompt_renderer::set_override_dir`.
    prompt_template_dir: PromptTemplateDir {
        type: String,
        default: String::new(),
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Never,
        private: false,
        toml_path: "agents.warp_agent.prompt_template_dir",
        description: "Directory to hot-load system prompt templates from. Empty uses the built-in templates.",
    }

    // Zap BYOP model + reasoning-depth persistence (written immediately after a picker switch,
    // carried over to new tabs/restarts).
    // The model uses its LLMId string form; an empty string = no last_used, falls back to the
    // profile default.
    byop_last_used_model_id: ByopLastUsedModelId {
        type: String,
        default: String::new(),
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.byop.last_used_model_id",
        description: "Last selected BYOP model id (picker hydrates new tabs/sessions from this).",
    }

    // Zap BYOP per-(api_type, model) reasoning-depth memory.
    // key = `<api_type>:<model_id>`, value = ReasoningEffortSetting. Written on picker switch.
    byop_last_used_reasoning: ByopLastUsedReasoning {
        type: BYOPLastUsedReasoningMap,
        default: BYOPLastUsedReasoningMap::default(),
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "agents.byop.last_used_reasoning",
        max_table_depth: 1,
        description: "Per-(api_type, model) reasoning effort memory for BYOP picker.",
    }

    // Per-agent settings: controls the toolbar and new-tab menu visibility for a single CLI
    // agent. The key is the result of CLIAgent::to_serialized_name().
    cli_agent_per_agent_settings: CLIAgentPerAgentSettings {
        type: HashMap<String, PerAgentSettings>,
        default: HashMap::new(),
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Never,
        private: false,
        toml_path: "agents.third_party.per_agent",
        max_table_depth: 1,
        description: "Per-agent visibility settings for toolbar and tab menu.",
    }

    // Whether at least one CLI agent installation scan has completed.
    // The first time the third-party agent settings page is opened, if this flag is false, a
    // sync is triggered automatically.
    cli_agent_scan_completed: CLIAgentScanCompleted {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Never,
        private: true,
    }
]);

impl AISettings {
    pub fn register_and_subscribe_to_events(app: &mut AppContext) {
        Self::register(app);
        app.add_singleton_model(FocusedTerminalInfo::new);
        CompiledCommandsForCodingAgentToolbar::register(app);

        app.update_model(&Self::handle(app), |_me, ctx| {
            ctx.subscribe_to_model(&FocusedTerminalInfo::handle(ctx), |_me, event, ctx| {
                if matches!(event, FocusedTerminalInfoEvent::TerminalInfoUpdated) {
                    // Pipe the event so that any view that listens for settings changes will be notified.
                    ctx.emit(AISettingsChangedEvent::IsAnyAIEnabled {
                        change_event_reason: ChangeEventReason::LocalChange,
                    });
                }
            });
        });
    }

    pub fn is_any_ai_enabled(&self, _app: &AppContext) -> bool {
        // Zap no longer allows disabling the Zap Agent via settings. A persisted
        // `agents.warp_agent.is_any_ai_enabled = false` in an old config file is ignored.
        true
    }

    pub fn is_orchestration_enabled(&self, app: &AppContext) -> bool {
        self.is_any_ai_enabled(app)
    }

    pub fn default_session_mode(&self, app: &AppContext) -> DefaultSessionMode {
        let mode = *self.default_session_mode_internal.value();
        match mode {
            // Terminal and TabConfig don't require AI.
            DefaultSessionMode::Terminal | DefaultSessionMode::TabConfig => mode,
            // Agent and AmbientAgent require AI to be enabled.
            DefaultSessionMode::Agent | DefaultSessionMode::AmbientAgent => {
                if self.is_any_ai_enabled(app) {
                    mode
                } else {
                    DefaultSessionMode::Terminal
                }
            }
            // DockerSandbox is gated on its feature flag; fall back to Terminal
            // when disabled so a stale stored value doesn't wedge the user.
            DefaultSessionMode::DockerSandbox => {
                if FeatureFlag::LocalDockerSandbox.is_enabled() {
                    mode
                } else {
                    DefaultSessionMode::Terminal
                }
            }
        }
    }

    /// Returns the stored default tab config path (only meaningful when mode is `TabConfig`).
    pub fn default_tab_config_path(&self) -> &str {
        &self.default_tab_config_path
    }

    /// Looks up the `TabConfig` matching the stored `default_tab_config_path`.
    /// Returns `None` if the path is empty or no loaded config matches.
    pub fn resolved_default_tab_config(
        &self,
        app: &AppContext,
    ) -> Option<crate::tab_configs::TabConfig> {
        let path_str = self.default_tab_config_path.as_str();
        if path_str.is_empty() {
            return None;
        }
        let path = std::path::Path::new(path_str);
        crate::user_config::WarpConfig::as_ref(app)
            .tab_configs()
            .iter()
            .find(|config| config.source_path.as_deref().is_some_and(|p| p == path))
            .cloned()
    }

    pub fn is_active_ai_enabled(&self, app: &warpui::AppContext) -> bool {
        self.is_any_ai_enabled(app)
            && *self.is_active_ai_enabled_internal
            && AppExecutionMode::as_ref(app).allows_active_ai()
    }

    pub fn is_prompt_suggestions_enabled(&self, app: &warpui::AppContext) -> bool {
        self.is_active_ai_enabled(app) && *self.prompt_suggestions_enabled_internal
    }

    pub fn is_rule_suggestions_enabled(&self, app: &warpui::AppContext) -> bool {
        self.is_active_ai_enabled(app) && *self.rule_suggestions_enabled_internal
    }

    pub fn is_code_suggestions_enabled(&self, app: &warpui::AppContext) -> bool {
        self.is_active_ai_enabled(app) && *self.code_suggestions_enabled_internal
    }

    pub fn is_natural_language_autosuggestions_enabled(&self, app: &warpui::AppContext) -> bool {
        self.is_active_ai_enabled(app) && *self.natural_language_autosuggestions_enabled_internal
    }

    pub fn is_git_operations_autogen_enabled(&self, app: &warpui::AppContext) -> bool {
        self.is_active_ai_enabled(app) && *self.git_operations_autogen_enabled_internal
    }

    pub fn is_intelligent_autosuggestions_enabled(&self, app: &warpui::AppContext) -> bool {
        self.is_active_ai_enabled(app) && *self.intelligent_autosuggestions_enabled_internal
    }

    pub fn is_voice_input_enabled(&self, app: &warpui::AppContext) -> bool {
        // Voice input is conditionally-compiled because it requires additional dependencies on some platforms.
        cfg!(feature = "voice_input")
            && self.is_any_ai_enabled(app)
            && *self.voice_input_enabled_internal
    }

    /// Returns `true` if input autodetection is enabled.
    ///
    /// If `FeatureFlag::AgentView` is enabled, this specifically gates NLD enablement in the agent
    /// view only.
    pub fn is_ai_autodetection_enabled(&self, app: &warpui::AppContext) -> bool {
        self.is_any_ai_enabled(app) && *self.ai_autodetection_enabled_internal
    }

    /// Returns `true` if NLD is enabled in the terminal.
    ///
    /// This is only used when `FeatureFlag::AgentView` is enabled.
    /// If the user has not explicitly set this setting, it defaults to the value of
    /// `ai_autodetection_enabled_internal`.
    pub fn is_nld_in_terminal_enabled(&self, app: &warpui::AppContext) -> bool {
        self.is_any_ai_enabled(app) && *self.nld_in_terminal_enabled_internal
    }

    pub fn is_memory_enabled(&self, app: &warpui::AppContext) -> bool {
        self.is_any_ai_enabled(app) && *self.memory_enabled
    }

    pub fn is_warp_drive_context_enabled(&self, app: &warpui::AppContext) -> bool {
        self.is_any_ai_enabled(app) && *self.warp_drive_context_enabled
    }

    pub fn is_file_based_mcp_enabled(&self, app: &warpui::AppContext) -> bool {
        if !FeatureFlag::FileBasedMcp.is_enabled() || !self.is_any_ai_enabled(app) {
            return false;
        }
        // NOTE: we intentionally do not force-enable this in autonomous agent runs. Previously
        // we auto-spawned file-based MCPs in autonomous execution, but that bypassed
        // the user's explicit opt-in and let any MCP config checked into a repo run
        // arbitrary commands as part of an agent run. Respecting the toggle
        // closes that attack surface; agents that need project-scoped MCP
        // servers should surface an explicit, auditable opt-in. A more robust
        // solution (e.g. per-environment allowlisting, signed configs) should be
        // explored in the future.
        *self.file_based_mcp_enabled
    }

    /// Determines whether a quota reset banner should be displayed to the user.
    ///
    /// The banner should be shown if the most recent completed billing cycle had
    /// quota exceeded and the banner was not manually dismissed.
    pub fn should_display_quota_reset_banner(&self) -> bool {
        let quota_info = &self.ai_request_quota_info;

        let most_recent_completed_cycle = quota_info
            .cycle_history
            .iter()
            .rev()
            .find(|cycle| cycle.end_date < Utc::now());

        if let Some(cycle) = most_recent_completed_cycle {
            if cycle.was_quota_exceeded && !cycle.banner_state.dismissed {
                return true;
            }
        }

        false
    }

    /// Marks the banner as dismissed for all completed cycles.
    pub fn mark_quota_banner_as_dismissed(&mut self, ctx: &mut ModelContext<Self>) {
        let mut cycle_history = self.ai_request_quota_info.cycle_history.clone();

        for cycle in cycle_history.iter_mut() {
            if cycle.end_date < Utc::now() {
                cycle.banner_state.dismissed = true;
            }
        }

        report_if_error!(
            self.ai_request_quota_info
                .set_value(AIRequestQuotaInfo { cycle_history }, ctx)
        );
    }

    /// Updates the quota info based on the latest RequestLimitInfo.
    ///
    /// This method finds or creates the appropriate CycleInfo based on the
    /// request_limit_info's next refresh time and updates its fields accordingly.
    pub fn update_quota_info(
        &mut self,
        request_limit_info: &RequestLimitInfo,
        ctx: &mut ModelContext<Self>,
    ) {
        // Convert ServerTimestamp to DateTime<Utc>
        let next_refresh_time = request_limit_info.next_refresh_time.utc();
        let now = Utc::now();

        // Check if request_limit_info has unlimited requests
        let is_quota_exceeded = !request_limit_info.is_unlimited
            && request_limit_info.num_requests_used_since_refresh >= request_limit_info.limit;

        let mut cycle_history = self.ai_request_quota_info.cycle_history.clone();

        // Track if we updated an existing cycle
        let mut updated_existing_cycle = false;

        // Find or create a cycle that matches the current period
        if let Some(current_cycle) = cycle_history.last_mut() {
            if now <= current_cycle.end_date {
                // Update existing cycle
                current_cycle.was_quota_exceeded = is_quota_exceeded;
                updated_existing_cycle = true;
            }
        }

        // Only create a new cycle if we didn't update an existing one
        if !updated_existing_cycle {
            // Create a new cycle
            let new_cycle = CycleInfo {
                end_date: next_refresh_time,
                was_quota_exceeded: is_quota_exceeded,
                banner_state: BannerState::default(),
            };

            cycle_history.push(new_cycle);
        }

        report_if_error!(
            self.ai_request_quota_info
                .set_value(AIRequestQuotaInfo { cycle_history }, ctx)
        );
    }

    pub fn is_command_denylist_editable(&self, app: &AppContext) -> bool {
        let set_by_workspace = UserWorkspaces::as_ref(app)
            .ai_autonomy_settings()
            .has_override_for_execute_commands_denylist();

        self.is_any_ai_enabled(app) && !set_by_workspace
    }

    pub fn is_command_allowlist_editable(&self, app: &AppContext) -> bool {
        let set_by_workspace = UserWorkspaces::as_ref(app)
            .ai_autonomy_settings()
            .has_override_for_execute_commands_allowlist();

        self.is_any_ai_enabled(app) && !set_by_workspace
    }

    pub fn is_directory_allowlist_editable(&self, app: &AppContext) -> bool {
        let set_by_workspace = UserWorkspaces::as_ref(app)
            .ai_autonomy_settings()
            .has_override_for_read_files_allowlist();

        self.is_any_ai_enabled(app) && !set_by_workspace
    }

    pub fn is_execute_commands_permissions_editable(&self, app: &AppContext) -> bool {
        let set_by_workspace = UserWorkspaces::as_ref(app)
            .ai_autonomy_settings()
            .has_override_for_execute_commands();

        self.is_any_ai_enabled(app) && !set_by_workspace
    }

    pub fn is_write_to_pty_permissions_editable(&self, app: &AppContext) -> bool {
        let set_by_workspace = UserWorkspaces::as_ref(app)
            .ai_autonomy_settings()
            .has_override_for_write_to_pty();
        self.is_any_ai_enabled(app) && !set_by_workspace
    }

    pub fn is_computer_use_permissions_editable(&self, app: &AppContext) -> bool {
        let set_by_workspace = UserWorkspaces::as_ref(app)
            .ai_autonomy_settings()
            .has_override_for_computer_use();
        self.is_any_ai_enabled(app) && !set_by_workspace
    }

    pub fn is_read_files_permissions_editable(&self, app: &AppContext) -> bool {
        let set_by_workspace = UserWorkspaces::as_ref(app)
            .ai_autonomy_settings()
            .has_override_for_read_files();

        self.is_any_ai_enabled(app) && !set_by_workspace
    }

    pub fn is_code_diffs_permissions_editable(&self, app: &AppContext) -> bool {
        let set_by_workspace = UserWorkspaces::as_ref(app)
            .ai_autonomy_settings()
            .has_override_for_code_diffs();

        self.is_any_ai_enabled(app) && !set_by_workspace
    }

    pub fn is_ask_user_question_permissions_editable(&self, app: &AppContext) -> bool {
        self.is_any_ai_enabled(app)
    }

    pub fn is_mcp_permission_editable(&self, app: &AppContext) -> bool {
        // TODO: Allow workspace overrides on MCP permissions.
        self.is_any_ai_enabled(app)
    }

    pub fn show_code_suggestion_speedbump(&self, app: &AppContext) -> bool {
        self.is_any_ai_enabled(app) && *self.show_code_suggestion_speedbump
    }

    /// Handles first-time voice input setup when user clicks the voice button.
    ///
    /// If the user hasn't explicitly interacted with voice yet:
    /// - Sets the default voice input toggle key based on the OS
    /// - Marks `explicitly_interacted_with_voice` as true
    /// - Returns `Some(toggle_key)` so the caller can show a toast
    ///
    /// If the user has already interacted with voice, returns `None`.
    pub fn maybe_setup_first_time_voice(
        &mut self,
        ctx: &mut ModelContext<Self>,
    ) -> Option<VoiceInputToggleKey> {
        if *self.explicitly_interacted_with_voice.value() {
            return None;
        }

        let voice_input_toggle_key = match OperatingSystem::get() {
            OperatingSystem::Mac => VoiceInputToggleKey::Fn,
            OperatingSystem::Windows | OperatingSystem::Linux | OperatingSystem::Other(_) => {
                VoiceInputToggleKey::AltRight
            }
        };

        report_if_error!(
            self.voice_input_toggle_key
                .set_value(voice_input_toggle_key, ctx)
        );

        report_if_error!(self.explicitly_interacted_with_voice.set_value(true, ctx));

        Some(voice_input_toggle_key)
    }

    pub fn add_cli_agent_footer_enabled_command(
        &mut self,
        command: &str,
        ctx: &mut ModelContext<Self>,
    ) {
        let command = command.trim();
        if command.is_empty() {
            return;
        }
        if self
            .cli_agent_footer_enabled_commands
            .value()
            .contains_key(command)
        {
            return;
        }

        let mut map = self.cli_agent_footer_enabled_commands.value().0.clone();
        map.insert(command.to_string(), String::new());
        report_if_error!(
            self.cli_agent_footer_enabled_commands
                .set_value(ToolbarCommandMap::new(map), ctx)
        );
    }

    pub fn remove_cli_agent_footer_enabled_command(
        &mut self,
        command: &str,
        ctx: &mut ModelContext<Self>,
    ) {
        let command = command.trim();
        let mut map = self.cli_agent_footer_enabled_commands.value().0.clone();
        map.shift_remove(command);
        report_if_error!(
            self.cli_agent_footer_enabled_commands
                .set_value(ToolbarCommandMap::new(map), ctx)
        );
    }

    pub fn set_cli_agent_for_command(
        &mut self,
        pattern: &str,
        agent: Option<CLIAgent>,
        ctx: &mut ModelContext<Self>,
    ) {
        let mut map = self.cli_agent_footer_enabled_commands.value().0.clone();
        if !map.contains_key(pattern) {
            return;
        }
        let value = agent.map(|a| a.to_serialized_name()).unwrap_or_default();
        map.insert(pattern.to_string(), value);
        report_if_error!(
            self.cli_agent_footer_enabled_commands
                .set_value(ToolbarCommandMap::new(map), ctx)
        );
    }

    /// Whether the feature-intro popover with the given id key has been seen.
    pub fn is_feature_intro_seen(&self, key: &str) -> bool {
        self.seen_feature_intro_ids
            .get(key)
            .copied()
            .unwrap_or(false)
    }

    /// Records that the feature-intro popover with the given id key has been seen,
    /// so it is never shown again. No-op if already recorded.
    pub fn mark_feature_intro_seen(&mut self, key: &str, ctx: &mut ModelContext<Self>) {
        if self.is_feature_intro_seen(key) {
            return;
        }
        let mut map = self.seen_feature_intro_ids.clone();
        map.insert(key.to_owned(), true);
        report_if_error!(self.seen_feature_intro_ids.set_value(map, ctx));
    }

    /// Whether the plugin install chip was dismissed for the given agent/host.
    pub fn is_plugin_install_chip_dismissed(&self, key: &str) -> bool {
        self.plugin_install_chip_dismissed_map
            .get(key)
            .copied()
            .unwrap_or(false)
    }

    /// Mark the plugin install chip as dismissed for the given agent/host.
    pub fn dismiss_plugin_install_chip(&mut self, key: &str, ctx: &mut ModelContext<Self>) {
        let mut map = self.plugin_install_chip_dismissed_map.clone();
        map.insert(key.to_owned(), true);
        report_if_error!(self.plugin_install_chip_dismissed_map.set_value(map, ctx));
    }

    /// Returns the minimum plugin version for which the update chip was dismissed
    /// for the given agent/host, or an empty string if not dismissed.
    pub fn plugin_update_chip_dismissed_version(&self, key: &str) -> &str {
        self.plugin_update_chip_dismissed_for_version_map
            .get(key)
            .map(String::as_str)
            .unwrap_or("")
    }

    /// Record that the user dismissed the update chip for the given agent/host at
    /// the specified minimum version.
    pub fn dismiss_plugin_update_chip(
        &mut self,
        key: &str,
        version: String,
        ctx: &mut ModelContext<Self>,
    ) {
        let mut map = self.plugin_update_chip_dismissed_for_version_map.clone();
        map.insert(key.to_owned(), version);
        report_if_error!(
            self.plugin_update_chip_dismissed_for_version_map
                .set_value(map, ctx)
        );
    }

    // ── Per-agent settings ──

    /// Queries whether the toolbar is enabled for a given CLI agent. Falls back to the agent's
    /// default value if not present in the per-agent settings.
    pub fn is_cli_agent_toolbar_enabled(&self, agent: CLIAgent) -> bool {
        if matches!(agent, CLIAgent::Unknown) {
            return true;
        }
        self.cli_agent_per_agent_settings
            .get(agent.to_serialized_name().as_str())
            .map(|s| s.toolbar)
            .unwrap_or_else(|| PerAgentSettings::default_for(agent).toolbar)
    }

    /// Queries whether a given CLI agent is shown in the new-tab menu. Falls back to the
    /// agent's default value if not present in the per-agent settings.
    pub fn is_cli_agent_tab_menu_enabled(&self, agent: CLIAgent) -> bool {
        if matches!(agent, CLIAgent::Unknown) {
            return false;
        }
        self.cli_agent_per_agent_settings
            .get(agent.to_serialized_name().as_str())
            .map(|s| s.tabmenu)
            .unwrap_or_else(|| PerAgentSettings::default_for(agent).tabmenu)
    }

    /// Queries whether the titlebar button is enabled for a given CLI agent. Falls back to the
    /// agent's default value if not present in the per-agent settings.
    pub fn is_cli_agent_titlebar_enabled(&self, agent: CLIAgent) -> bool {
        if matches!(agent, CLIAgent::Unknown) {
            return false;
        }
        self.cli_agent_per_agent_settings
            .get(agent.to_serialized_name().as_str())
            .map(|s| s.titlebar)
            .unwrap_or_else(|| PerAgentSettings::default_for(agent).titlebar)
    }

    /// Sets the toolbar-enabled state for a single agent.
    pub fn set_cli_agent_toolbar(
        &mut self,
        agent: CLIAgent,
        enabled: bool,
        ctx: &mut ModelContext<Self>,
    ) {
        let key = agent.to_serialized_name();
        let mut map = self.cli_agent_per_agent_settings.clone();
        map.entry(key)
            .and_modify(|s| s.toolbar = enabled)
            .or_insert_with(|| PerAgentSettings {
                toolbar: enabled,
                ..PerAgentSettings::default_for(agent)
            });
        report_if_error!(self.cli_agent_per_agent_settings.set_value(map, ctx));
    }

    /// Sets the tab-menu-enabled state for a single agent.
    pub fn set_cli_agent_tab_menu(
        &mut self,
        agent: CLIAgent,
        enabled: bool,
        ctx: &mut ModelContext<Self>,
    ) {
        let key = agent.to_serialized_name();
        let mut map = self.cli_agent_per_agent_settings.clone();
        map.entry(key)
            .and_modify(|s| s.tabmenu = enabled)
            .or_insert_with(|| PerAgentSettings {
                tabmenu: enabled,
                ..PerAgentSettings::default_for(agent)
            });
        report_if_error!(self.cli_agent_per_agent_settings.set_value(map, ctx));
    }

    /// Sets the titlebar-button-enabled state for a single agent.
    pub fn set_cli_agent_titlebar(
        &mut self,
        agent: CLIAgent,
        enabled: bool,
        ctx: &mut ModelContext<Self>,
    ) {
        let key = agent.to_serialized_name();
        let mut map = self.cli_agent_per_agent_settings.clone();
        map.entry(key)
            .and_modify(|s| s.titlebar = enabled)
            .or_insert_with(|| PerAgentSettings {
                titlebar: enabled,
                ..PerAgentSettings::default_for(agent)
            });
        report_if_error!(self.cli_agent_per_agent_settings.set_value(map, ctx));
    }

    /// Syncs per-agent settings based on installation scan results.
    /// - Newly detected agents get default values written (toolbar=true, tabmenu=true)
    /// - Uninstalled agents are removed from settings
    /// - Marks the scan as completed
    pub fn sync_per_agent_from_scan(
        &mut self,
        installed: &HashMap<CLIAgent, bool>,
        ctx: &mut ModelContext<Self>,
    ) {
        let installed_agents: Vec<CLIAgent> = installed
            .iter()
            .filter(|(a, v)| **v && !matches!(a, CLIAgent::Unknown))
            .map(|(a, _)| *a)
            .collect();
        let installed_names: std::collections::HashSet<String> = installed_agents
            .iter()
            .map(|a| a.to_serialized_name())
            .collect();

        let mut per_agent = self.cli_agent_per_agent_settings.clone();

        for agent in &installed_agents {
            per_agent
                .entry(agent.to_serialized_name())
                .or_insert_with(|| PerAgentSettings::default_for(*agent));
        }

        // Uninstalled agents -> removed
        per_agent.retain(|name, _| installed_names.contains(name.as_str()));

        let changed = &per_agent != self.cli_agent_per_agent_settings.value();
        if changed {
            report_if_error!(self.cli_agent_per_agent_settings.set_value(per_agent, ctx));
        }

        if !*self.cli_agent_scan_completed.value() {
            report_if_error!(self.cli_agent_scan_completed.set_value(true, ctx));
        }
    }

    /// Returns whether at least one CLI agent installation scan has completed.
    pub fn is_cli_agent_scan_completed(&self) -> bool {
        *self.cli_agent_scan_completed.value()
    }
}

/// Singleton model that caches compiled regexes for the `cli_agent_footer_enabled_commands`
/// setting. Each entry pairs a compiled regex with the CLI agent it maps to.
pub struct CompiledCommandsForCodingAgentToolbar {
    regexes: Vec<(Regex, CLIAgent)>,
}

impl CompiledCommandsForCodingAgentToolbar {
    fn parse(app: &AppContext) -> Vec<(Regex, CLIAgent)> {
        AISettings::as_ref(app)
            .cli_agent_footer_enabled_commands
            .value()
            .iter()
            .filter_map(|(pattern, agent_name)| {
                let regex = Regex::new(pattern).ok()?;
                let agent = CLIAgent::from_serialized_name(agent_name);
                Some((regex, agent))
            })
            .collect()
    }

    fn register(app: &mut AppContext) {
        let handle = app.add_singleton_model(|ctx| Self {
            regexes: Self::parse(ctx),
        });
        let ai_settings = AISettings::handle(app);
        app.subscribe_to_model(&ai_settings, move |_, event, ctx| {
            if matches!(
                event,
                AISettingsChangedEvent::CLIAgentToolbarEnabledCommands { .. }
            ) {
                let regexes = Self::parse(ctx);
                handle.update(ctx, |me, _| {
                    me.regexes = regexes;
                });
            }
        });
    }

    /// Returns the CLI agent assigned to the first matching pattern, or `None`
    /// if no pattern matches the command.
    pub fn matched_agent(app: &AppContext, command: &str) -> Option<CLIAgent> {
        Self::as_ref(app)
            .regexes
            .iter()
            .find(|(regex, _)| regex.is_match(command))
            .map(|(_, agent)| *agent)
    }
}

impl Entity for CompiledCommandsForCodingAgentToolbar {
    type Event = ();
}

impl SingletonEntity for CompiledCommandsForCodingAgentToolbar {}

#[cfg(test)]
#[path = "ai_tests.rs"]
mod tests;
