use std::path::PathBuf;

use crate::cloud_object::UniquePer;
use crate::settings::AISettings;
use crate::{
    cloud_object::{
        model::{
            generic_string_model::{GenericStringModel, GenericStringObjectId, StringModel},
            json_model::{JsonModel, JsonSerializer},
        },
        GenericStoredObject, GenericStringObjectFormat, GenericStringObjectUniqueKey,
        JsonObjectType,
    },
    settings::{
        AgentModeCommandExecutionPredicate, DEFAULT_COMMAND_EXECUTION_ALLOWLIST,
        DEFAULT_COMMAND_EXECUTION_DENYLIST,
    },
};
use serde::{Deserialize, Serialize};
use warp_core::channel::ChannelState;
use warp_core::features::FeatureFlag;
use warpui::{AppContext, SingletonEntity};

use super::llms::{LLMContextWindow, LLMId, LLMPreferences};

pub const PROFILE_NAME_MAX_LENGTH: usize = 50;

pub mod editor;
pub mod model_menu_items;
pub mod profiles;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionPermission {
    AgentDecides,
    AlwaysAllow,
    AlwaysAsk,

    // This is intended to catch deserialization errors whenever we add new variants to this enum. Say we
    // want to add a "Never" variant. Without this catch-all, old clients wouldn't be able to deserialize
    // a "Never" into one of the existing options.
    #[serde(other)]
    Unknown,
}

impl ActionPermission {
    pub fn description(&self) -> &'static str {
        match self {
            ActionPermission::AgentDecides | ActionPermission::Unknown => "The Agent chooses the safest path: acting on its own when confident, and asking for approval when uncertain.",
            ActionPermission::AlwaysAllow => "Give the Agent full autonomy  — no manual approval ever required.",
            ActionPermission::AlwaysAsk => "Require explicit approval before the Agent takes any action.",
        }
    }

    pub fn is_always_ask(&self) -> bool {
        matches!(self, Self::AlwaysAsk)
    }

    pub fn is_always_allow(&self) -> bool {
        matches!(self, Self::AlwaysAllow)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WriteToPtyPermission {
    // This is for backwards compatibility with the old "Never" value.
    #[serde(alias = "Never")]
    AlwaysAllow,
    #[default]
    AlwaysAsk,
    AskOnFirstWrite,

    // This is intended to catch deserialization errors whenever we add new variants to this enum.
    #[serde(other)]
    Unknown,
}

impl WriteToPtyPermission {
    pub fn description(&self) -> &'static str {
        match self {
            WriteToPtyPermission::AlwaysAllow => ActionPermission::AlwaysAllow.description(),
            WriteToPtyPermission::AskOnFirstWrite => {
                "The agent will ask for permission the first time it needs to interact with a running command. After that, it will continue automatically for the rest of that command."
            }
            WriteToPtyPermission::AlwaysAsk => "The agent will always ask for permission to interact with a running command.",
            WriteToPtyPermission::Unknown => ActionPermission::Unknown.description(),
        }
    }

    pub fn is_always_allow(&self) -> bool {
        matches!(self, Self::AlwaysAllow)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComputerUsePermission {
    #[default]
    Never,
    AlwaysAsk,
    AlwaysAllow,

    // This is intended to catch deserialization errors whenever we add new variants to this enum.
    #[serde(other)]
    Unknown,
}

impl ComputerUsePermission {
    pub fn description(&self) -> &'static str {
        match self {
            ComputerUsePermission::Never => {
                "Computer use tools are disabled and will not be available to the Agent."
            }
            ComputerUsePermission::AlwaysAsk => {
                "Require explicit approval before the Agent uses computer use tools."
            }
            ComputerUsePermission::AlwaysAllow => {
                "Give the Agent full autonomy to use computer use tools without approval."
            }
            ComputerUsePermission::Unknown => "Unknown setting.",
        }
    }

    pub fn is_enabled(&self) -> bool {
        !matches!(self, Self::Never | Self::Unknown)
    }

    pub fn is_always_allow(&self) -> bool {
        matches!(self, Self::AlwaysAllow)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AskUserQuestionPermission {
    /// Never pause; skip questions and continue with best judgment.
    Never,
    /// Equivalent to `AlwaysAsk` in openWarp: auto-approve mode no longer silently
    /// skips user questions; it only auto-approves execution-type tools (shell/edit).
    /// The variant name is kept for compatibility with already-serialized profiles.
    #[default]
    AskExceptInAutoApprove,
    /// Always pause and wait for the user to answer before continuing, even in auto-approve mode.
    AlwaysAsk,

    // This is intended to catch deserialization errors whenever we add new variants to this enum.
    #[serde(other)]
    Unknown,
}

impl AskUserQuestionPermission {
    pub fn description(&self) -> &'static str {
        match self {
            AskUserQuestionPermission::AskExceptInAutoApprove
            | AskUserQuestionPermission::Unknown => {
                "The Agent may ask a question and will pause for your response, even when auto-approve is on (auto-approve only applies to shell/edit tools)."
            }
            AskUserQuestionPermission::Never => {
                "The Agent will not ask questions and will continue with its best judgment."
            }
            AskUserQuestionPermission::AlwaysAsk => {
                "The Agent may ask a question and will pause for your response even when auto-approve is on."
            }
        }
    }
}

/// Built-in system-prompt template families selectable per model slot.
///
/// Each name maps to `system/<name>.j2` under the prompt template dir (or the
/// embedded copy when no override dir is set). Kept in sync with the `EMBEDDED`
/// table in [`crate::ai::agent_providers::prompt_renderer`]. `default` is the
/// generic fallback; `local`/`lean` are the short prompts for small local models.
pub const BUILTIN_PROMPT_TEMPLATES: &[&str] = &[
    "default",
    "anthropic",
    "gpt",
    "beast",
    "codex",
    "gemini",
    "kimi",
    "trinity",
    "local",
    "lean",
    // Example task-oriented prompt (not auto-picked by model family; opt in per slot).
    "troubleshooting",
];

/// Where a model slot's system prompt comes from when it is not left on `Auto`.
///
/// `None` (the absence of a `PromptSource`) means Auto — pick by model family, the
/// historical behavior. A `Builtin` pins one of [`BUILTIN_PROMPT_TEMPLATES`]; a
/// `CustomFile` points at a user file *relative to the prompt template directory*
/// (settings → AI → Prompt template directory), rendered through the same minijinja
/// environment as the built-ins so `{% include "partials/..." %}` still works.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PromptSource {
    /// A built-in template family (e.g. `"lean"`), resolved to `system/<name>.j2`.
    Builtin(String),
    /// A user file path relative to the prompt template directory.
    CustomFile(String),
}

impl PromptSource {
    /// The minijinja template name for a builtin (`system/<name>.j2`), or `None`
    /// for a custom file (which is loaded from disk instead of the env table).
    pub fn builtin_template_name(&self) -> Option<String> {
        match self {
            PromptSource::Builtin(name) => Some(format!("system/{name}.j2")),
            PromptSource::CustomFile(_) => None,
        }
    }
}

/// Per-model-slot system prompt overrides for a profile.
///
/// A profile lets the user pick a different model for each role (base / coding /
/// cli_agent / computer_use / title / active_ai / next_command); this mirrors those
/// slots so each can carry its own system prompt. `None` on a slot = Auto.
///
/// `#[serde(default)]` (plus the field-level default on the profile) makes this
/// backward-compatible: profiles serialized before this field existed deserialize
/// with every slot on Auto.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfilePromptOverrides {
    // Agent slots — the system prompt is picked by model family, so these accept
    // a `Builtin(family)` (default / anthropic / lean / …) as well as a custom file.
    pub base: Option<PromptSource>,
    pub coding: Option<PromptSource>,
    pub cli_agent: Option<PromptSource>,
    pub computer_use: Option<PromptSource>,
    // Auxiliary prompts — each is a single fixed built-in template, so only a
    // custom file (or Auto) is meaningful. `active_ai` is intentionally split into
    // its four distinct sub-prompts; the one model slot drives all of them, but
    // each has its own overridable prompt.
    pub title: Option<PromptSource>,
    pub prompt_suggestions: Option<PromptSource>,
    pub nld_predict: Option<PromptSource>,
    pub relevant_files: Option<PromptSource>,
    pub workflow_metadata: Option<PromptSource>,
    pub next_command: Option<PromptSource>,
}

/// Identifies one per-prompt override slot in [`ProfilePromptOverrides`]. Lets the
/// profile editor and the setter agree on which prompt a UI action targets, and
/// which built-in it maps to for custom-file discovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptSlot {
    // Agent slots — built-in is picked by model family (family picker + custom file).
    Base,
    Coding,
    CliAgent,
    ComputerUse,
    // Auxiliary prompts — one fixed built-in each (custom file or Auto only).
    Title,
    PromptSuggestions,
    NldPredict,
    RelevantFiles,
    WorkflowMetadata,
    NextCommand,
}

impl PromptSlot {
    /// Every slot, in the order they should appear in the editor.
    pub const ALL: [PromptSlot; 10] = [
        PromptSlot::Base,
        PromptSlot::Coding,
        PromptSlot::CliAgent,
        PromptSlot::ComputerUse,
        PromptSlot::Title,
        PromptSlot::PromptSuggestions,
        PromptSlot::NldPredict,
        PromptSlot::RelevantFiles,
        PromptSlot::WorkflowMetadata,
        PromptSlot::NextCommand,
    ];

    /// Mutable access to this slot's field in the overrides struct.
    pub fn select_mut<'a>(
        &self,
        o: &'a mut ProfilePromptOverrides,
    ) -> &'a mut Option<PromptSource> {
        match self {
            PromptSlot::Base => &mut o.base,
            PromptSlot::Coding => &mut o.coding,
            PromptSlot::CliAgent => &mut o.cli_agent,
            PromptSlot::ComputerUse => &mut o.computer_use,
            PromptSlot::Title => &mut o.title,
            PromptSlot::PromptSuggestions => &mut o.prompt_suggestions,
            PromptSlot::NldPredict => &mut o.nld_predict,
            PromptSlot::RelevantFiles => &mut o.relevant_files,
            PromptSlot::WorkflowMetadata => &mut o.workflow_metadata,
            PromptSlot::NextCommand => &mut o.next_command,
        }
    }

    /// This slot's current override.
    pub fn get<'a>(&self, o: &'a ProfilePromptOverrides) -> &'a Option<PromptSource> {
        match self {
            PromptSlot::Base => &o.base,
            PromptSlot::Coding => &o.coding,
            PromptSlot::CliAgent => &o.cli_agent,
            PromptSlot::ComputerUse => &o.computer_use,
            PromptSlot::Title => &o.title,
            PromptSlot::PromptSuggestions => &o.prompt_suggestions,
            PromptSlot::NldPredict => &o.nld_predict,
            PromptSlot::RelevantFiles => &o.relevant_files,
            PromptSlot::WorkflowMetadata => &o.workflow_metadata,
            PromptSlot::NextCommand => &o.next_command,
        }
    }

    /// Agent slots pick their built-in by model family, so they offer the full
    /// [`BUILTIN_PROMPT_TEMPLATES`] list; auxiliary prompts have a single built-in.
    pub fn is_agent_slot(&self) -> bool {
        matches!(
            self,
            PromptSlot::Base | PromptSlot::Coding | PromptSlot::CliAgent | PromptSlot::ComputerUse
        )
    }
}

/// Core data structure representing an AI execution profile, which includes model configuration,
/// behavior settings, and permissions.
///
/// NOTE: `planning_model` was removed after planning via subagent was deprecated; serialized legacy
/// profiles may include a `planning_model` field and this field name should remain reserved
/// indefinitely.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AIExecutionProfile {
    pub name: String,
    pub is_default_profile: bool,
    pub apply_code_diffs: ActionPermission,
    pub read_files: ActionPermission,

    pub execute_commands: ActionPermission,
    pub write_to_pty: WriteToPtyPermission,
    pub mcp_permissions: ActionPermission,
    pub ask_user_question: AskUserQuestionPermission,

    /// Always ask for permission for these commands
    pub command_denylist: Vec<AgentModeCommandExecutionPredicate>,

    /// When the execute_commands is set to AlwaysAsk, autoexecute these commands
    pub command_allowlist: Vec<AgentModeCommandExecutionPredicate>,

    /// When the read_files is set to AlwaysAsk, autoread from these directories
    pub directory_allowlist: Vec<PathBuf>,

    pub mcp_allowlist: Vec<uuid::Uuid>,
    pub mcp_denylist: Vec<uuid::Uuid>,

    pub computer_use: ComputerUsePermission,

    pub base_model: Option<LLMId>,
    pub coding_model: Option<LLMId>,
    pub cli_agent_model: Option<LLMId>,
    pub computer_use_model: Option<LLMId>,
    /// Model used to generate conversation titles. Falls back to `base_model` when `None`.
    pub title_model: Option<LLMId>,
    /// Model used by active AI (prompt suggestions / NLD / relevant files).
    /// Falls back to `base_model` when `None`. Prefer a small/fast/cheap BYOP model.
    pub active_ai_model: Option<LLMId>,
    /// Model used for Next Command (grey autocompletion / zero-state suggestions).
    /// Falls back to `base_model` when `None`. Latency-sensitive; prefer the
    /// cheapest/fastest BYOP model.
    pub next_command_model: Option<LLMId>,

    pub context_window_limit: Option<u32>,

    /// Whether plans created by the agent should be automatically synced to Zap Drive
    pub autosync_plans_to_warp_drive: bool,

    /// Whether the agent may use web search when helpful for completing tasks
    pub web_search_enabled: bool,

    /// Per-model-slot system prompt overrides. Absent field / all-`None` slots =
    /// Auto (pick by model family), the historical behavior. See [`PromptSource`].
    pub prompt_overrides: ProfilePromptOverrides,
}

impl Default for AIExecutionProfile {
    fn default() -> Self {
        Self {
            name: Default::default(),
            is_default_profile: false,
            apply_code_diffs: ActionPermission::AgentDecides,
            read_files: ActionPermission::AgentDecides,
            execute_commands: ActionPermission::AlwaysAsk,
            write_to_pty: WriteToPtyPermission::AlwaysAsk,
            mcp_permissions: ActionPermission::AgentDecides,
            ask_user_question: AskUserQuestionPermission::AskExceptInAutoApprove,
            command_denylist: DEFAULT_COMMAND_EXECUTION_DENYLIST.clone(),
            command_allowlist: Vec::new(),
            directory_allowlist: Vec::new(),
            mcp_allowlist: Vec::new(),
            mcp_denylist: Vec::new(),
            computer_use: ComputerUsePermission::Never,
            base_model: None,
            coding_model: None,
            cli_agent_model: None,
            computer_use_model: None,
            title_model: None,
            active_ai_model: None,
            next_command_model: None,
            context_window_limit: None,
            autosync_plans_to_warp_drive: false,
            web_search_enabled: true,
            prompt_overrides: ProfilePromptOverrides::default(),
        }
    }
}

impl AIExecutionProfile {
    /// Resolve the per-slot prompt override for an agent request whose operative
    /// model is `model`.
    ///
    /// The BYOP chat path renders a single system prompt from `params.model`, so
    /// we bridge role → slot by matching `model` against the profile's configured
    /// slot models (most specific first, then `base` as the fallback slot). When
    /// two slots point at the *same* model we can't tell them apart from the model
    /// alone; the more specific slot wins, which is the sensible default. Returns
    /// `None` (= Auto) when nothing matches or the matched slot is on Auto.
    pub fn agent_prompt_override_for_model(&self, model: &LLMId) -> Option<&PromptSource> {
        let m = Some(model.clone());
        if self.computer_use_model == m {
            if let Some(s) = self.prompt_overrides.computer_use.as_ref() {
                return Some(s);
            }
        }
        if self.cli_agent_model == m {
            if let Some(s) = self.prompt_overrides.cli_agent.as_ref() {
                return Some(s);
            }
        }
        if self.coding_model == m {
            if let Some(s) = self.prompt_overrides.coding.as_ref() {
                return Some(s);
            }
        }
        // `base` is both the base-model slot and the fallback for anything else.
        self.prompt_overrides.base.as_ref()
    }

    pub fn create_default_from_legacy_settings(app: &AppContext) -> Self {
        // Note that the legacy "Autonomy" and "Code Access" settings are not imported here.
        // The "Code Access" setting defaulted to "Always Ask", which is the most restrictive, so
        // it's impossible for us to infer some hesitancy about autonomy from the setting and we should
        // ignore it. The same applies to "Autonomy".
        let ai_settings = AISettings::as_ref(app);
        Self {
            name: "Default".to_string(),
            is_default_profile: true,
            command_denylist: ai_settings.agent_mode_command_execution_denylist.clone(),
            // We initialize the command allowlist to be anything the user added, excluding all
            // the pre-populated defaults.
            command_allowlist: ai_settings
                .agent_mode_command_execution_allowlist
                .iter()
                .filter(|cmd| !DEFAULT_COMMAND_EXECUTION_ALLOWLIST.contains(cmd))
                .cloned()
                .collect(),
            directory_allowlist: ai_settings.agent_mode_coding_file_read_allowlist.clone(),
            ..Default::default()
        }
    }

    #[cfg(feature = "agent_mode_evals")]
    pub fn create_agent_mode_eval_profile() -> Self {
        Self {
            name: "Agent Mode Eval".to_string(),
            is_default_profile: false,
            apply_code_diffs: ActionPermission::AlwaysAllow,
            read_files: ActionPermission::AlwaysAllow,
            execute_commands: ActionPermission::AlwaysAllow,
            write_to_pty: WriteToPtyPermission::AlwaysAllow,
            mcp_permissions: ActionPermission::AlwaysAllow,
            ask_user_question: AskUserQuestionPermission::Never,
            command_denylist: Vec::new(),
            command_allowlist: Vec::new(),
            directory_allowlist: Vec::new(),
            mcp_allowlist: Vec::new(),
            mcp_denylist: Vec::new(),
            computer_use: ComputerUsePermission::Never,
            base_model: None,
            coding_model: None,
            cli_agent_model: None,
            computer_use_model: None,
            title_model: None,
            active_ai_model: None,
            next_command_model: None,
            context_window_limit: None,
            autosync_plans_to_warp_drive: false,
            web_search_enabled: true,
            prompt_overrides: ProfilePromptOverrides::default(),
        }
    }

    /// This creates a CLI-specific profile that will never ask the user for permission,
    /// since we cannot do so in a non-interactive setting.
    pub fn create_default_cli_profile(
        is_sandboxed: bool,
        computer_use_override: Option<bool>,
    ) -> Self {
        let command_denylist = if is_sandboxed {
            Vec::new()
        } else {
            DEFAULT_COMMAND_EXECUTION_DENYLIST.to_vec()
        };

        let computer_use_permission = match computer_use_override {
            Some(true) => {
                if is_sandboxed || FeatureFlag::LocalComputerUse.is_enabled() {
                    ComputerUsePermission::AlwaysAllow
                } else {
                    ComputerUsePermission::Never
                }
            }
            Some(false) => ComputerUsePermission::Never,
            None => {
                if is_sandboxed && ChannelState::channel().is_dogfood() {
                    ComputerUsePermission::AlwaysAllow
                } else {
                    ComputerUsePermission::Never
                }
            }
        };

        Self {
            name: "Default (CLI)".to_owned(),
            is_default_profile: true,
            apply_code_diffs: ActionPermission::AlwaysAllow,
            read_files: ActionPermission::AlwaysAllow,
            execute_commands: ActionPermission::AlwaysAllow,
            mcp_permissions: ActionPermission::AlwaysAllow,
            write_to_pty: WriteToPtyPermission::AlwaysAllow,
            ask_user_question: AskUserQuestionPermission::Never,
            command_denylist,
            command_allowlist: DEFAULT_COMMAND_EXECUTION_ALLOWLIST.to_vec(),
            directory_allowlist: Vec::new(),
            mcp_allowlist: Vec::new(),
            mcp_denylist: Vec::new(),
            computer_use: computer_use_permission,
            base_model: None,
            coding_model: None,
            cli_agent_model: None,
            computer_use_model: None,
            title_model: None,
            active_ai_model: None,
            next_command_model: None,
            context_window_limit: None,
            autosync_plans_to_warp_drive: FeatureFlag::SyncAmbientPlans.is_enabled(),
            web_search_enabled: true,
            prompt_overrides: ProfilePromptOverrides::default(),
        }
    }
}

impl AIExecutionProfile {
    pub fn configurable_context_window(&self, app: &AppContext) -> Option<LLMContextWindow> {
        let prefs = LLMPreferences::as_ref(app);
        let cw = self
            .base_model
            .as_ref()
            .and_then(|id| prefs.get_llm_info(id))
            .map(|info| info.context_window.clone())
            .unwrap_or_else(|| prefs.get_default_base_model().context_window.clone());
        if cw.is_configurable && cw.max > 0 {
            Some(cw)
        } else {
            None
        }
    }

    pub fn context_window_display_value(&self, app: &AppContext) -> Option<u32> {
        let cw = self.configurable_context_window(app)?;
        Some(self.context_window_limit.unwrap_or(cw.default_max))
    }
}

pub type AIExecutionProfileObject =
    GenericStoredObject<GenericStringObjectId, AIExecutionProfileObjectModel>;
pub type AIExecutionProfileObjectModel = GenericStringModel<AIExecutionProfile, JsonSerializer>;

impl StringModel for AIExecutionProfile {
    type StoredObjectType = AIExecutionProfileObject;

    fn model_type_name(&self) -> &'static str {
        "AIExecutionProfile"
    }

    fn should_enforce_revisions() -> bool {
        true
    }

    fn model_format() -> GenericStringObjectFormat {
        GenericStringObjectFormat::Json(JsonObjectType::AIExecutionProfile)
    }

    fn should_show_activity_toasts() -> bool {
        false
    }

    fn warn_if_unsaved_at_quit() -> bool {
        true
    }

    fn display_name(&self) -> String {
        // Handles case where default profile was previously created and named "Untitled"
        if self.is_default_profile {
            "Default".to_string()
        } else if self.name.trim().is_empty() {
            "Untitled".to_string()
        } else {
            self.name.clone()
        }
    }

    fn should_clear_on_unique_key_conflict(&self) -> bool {
        true
    }

    fn uniqueness_key(&self) -> Option<GenericStringObjectUniqueKey> {
        // We want to prevent the creation of several default profiles per user. If it's not the default
        // profile, then there can be many.
        self.is_default_profile
            .then_some(GenericStringObjectUniqueKey {
                key: "default".to_string(),
                unique_per: UniquePer::User,
            })
    }

    fn renders_in_warp_drive(&self) -> bool {
        false
    }
}

impl JsonModel for AIExecutionProfile {
    fn json_object_type() -> JsonObjectType {
        JsonObjectType::AIExecutionProfile
    }
}

#[cfg(test)]
mod prompt_override_tests {
    use super::*;

    fn model(s: &str) -> LLMId {
        LLMId::from(s)
    }

    #[test]
    fn resolves_slot_by_model_identity() {
        let mut p = AIExecutionProfile::default();
        p.base_model = Some(model("byop:p:base"));
        p.coding_model = Some(model("byop:p:coding"));
        p.prompt_overrides.base = Some(PromptSource::Builtin("lean".into()));
        p.prompt_overrides.coding = Some(PromptSource::Builtin("beast".into()));

        assert_eq!(
            p.agent_prompt_override_for_model(&model("byop:p:coding")),
            Some(&PromptSource::Builtin("beast".into())),
        );
        assert_eq!(
            p.agent_prompt_override_for_model(&model("byop:p:base")),
            Some(&PromptSource::Builtin("lean".into())),
        );
    }

    #[test]
    fn unmatched_model_falls_back_to_base_slot() {
        let mut p = AIExecutionProfile::default();
        p.base_model = Some(model("byop:p:base"));
        p.prompt_overrides.base = Some(PromptSource::Builtin("lean".into()));

        // A model that matches no configured slot still gets the base override,
        // which doubles as the fallback slot.
        assert_eq!(
            p.agent_prompt_override_for_model(&model("byop:p:whatever")),
            Some(&PromptSource::Builtin("lean".into())),
        );
    }

    #[test]
    fn specific_slot_without_override_falls_through_to_base() {
        let mut p = AIExecutionProfile::default();
        p.base_model = Some(model("byop:p:base"));
        p.coding_model = Some(model("byop:p:coding"));
        // coding slot left on Auto, base pinned.
        p.prompt_overrides.base = Some(PromptSource::CustomFile("mine.j2".into()));

        assert_eq!(
            p.agent_prompt_override_for_model(&model("byop:p:coding")),
            Some(&PromptSource::CustomFile("mine.j2".into())),
        );
    }

    #[test]
    fn auto_when_no_overrides_set() {
        let p = AIExecutionProfile::default();
        assert_eq!(p.agent_prompt_override_for_model(&model("byop:p:x")), None);
    }

    #[test]
    fn deserializes_legacy_profile_without_prompt_overrides() {
        // Profiles serialized before this field existed must still load, with
        // every slot defaulting to Auto (`None`). The container-level
        // `#[serde(default)]` lets `{}` fill in every field.
        let p: AIExecutionProfile = serde_json::from_str("{}").unwrap();
        assert_eq!(p.prompt_overrides, ProfilePromptOverrides::default());
        assert!(p.prompt_overrides.base.is_none());
        assert!(p.prompt_overrides.title.is_none());
    }

    #[test]
    fn prompt_slot_select_mut_and_get_agree_for_every_slot() {
        // Each slot must map to its own distinct field (no copy/paste aliasing).
        for (i, slot) in PromptSlot::ALL.iter().enumerate() {
            let mut o = ProfilePromptOverrides::default();
            let marker = PromptSource::CustomFile(format!("slot-{i}.j2"));
            *slot.select_mut(&mut o) = Some(marker.clone());
            assert_eq!(slot.get(&o), &Some(marker.clone()));
            // Exactly one field is set.
            let set_count = PromptSlot::ALL
                .iter()
                .filter(|s| s.get(&o).is_some())
                .count();
            assert_eq!(set_count, 1, "slot {slot:?} leaked into another field");
        }
    }

    #[test]
    fn prompt_slot_agent_classification() {
        assert!(PromptSlot::Base.is_agent_slot());
        assert!(PromptSlot::ComputerUse.is_agent_slot());
        assert!(!PromptSlot::Title.is_agent_slot());
        assert!(!PromptSlot::NldPredict.is_agent_slot());
    }

    #[test]
    fn prompt_overrides_round_trip_through_serde() {
        let mut p = AIExecutionProfile::default();
        p.prompt_overrides.base = Some(PromptSource::Builtin("lean".into()));
        p.prompt_overrides.title = Some(PromptSource::CustomFile("t.md".into()));

        let json = serde_json::to_string(&p).unwrap();
        let back: AIExecutionProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.prompt_overrides, p.prompt_overrides);
    }
}
