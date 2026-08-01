pub mod bindings;
pub mod commands;

use bitflags::bitflags;
pub use commands::SlashCommandId;

bitflags! {
    /// Specifies the requirements for a slash command to be available.
    ///
    /// Each flag represents a requirement that the session context must satisfy. The command is
    /// available when the session supports *all* of the command's requirement flags.
    ///
    /// A few common cases:
    /// * If neither [`Self::AGENT_VIEW`] nor [`Self::TERMINAL_VIEW`] is set, the command is available in all modes.
    ///   A command should *not* set both flags to be available in both modes - this results in requirements that cannot be satisfied.
    /// * Most `/fork`-like slash commands require [`Self::NO_LRC_CONTROL`] and [`Self::ACTIVE_CONVERSATION`]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Availability: u8 {
        /// No requirements — always available.
        const ALWAYS = 0;
        /// Requires the agent view.
        const AGENT_VIEW = 1 << 0;
        /// Requires the terminal view.
        const TERMINAL_VIEW = 1 << 1;
        /// Requires a local session (not available in remote/cloud sessions).
        const LOCAL = 1 << 2;
        /// Requires a git repository.
        const REPOSITORY = 1 << 3;
        /// Requires that the agent is not currently in control of a long-running command.
        const NO_LRC_CONTROL = 1 << 4;
        /// Requires an active AI conversation.
        const ACTIVE_CONVERSATION = 1 << 5;
        /// Requires AI to be globally enabled.
        const AI_ENABLED = 1 << 7;
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Argument {
    pub hint_text: Option<&'static str>,
    pub is_optional: bool,
    /// If `true`, selecting the slash command from the menu (or via keybinding) will execute the
    /// slash command with no arguments.
    ///
    /// If `false`, selecting the slash command from the menu (or via keybinding) inserts the
    /// slash command into the input.
    ///
    /// Set this based on whether or not you want you think a user should always have the option to
    /// supply an argument.
    pub should_execute_on_selection: bool,
}

impl Argument {
    pub(super) fn optional() -> Self {
        Self {
            is_optional: true,
            ..Default::default()
        }
    }

    pub(super) fn required() -> Self {
        Self {
            is_optional: false,
            ..Default::default()
        }
    }

    pub(super) fn with_hint_text(mut self, text: &'static str) -> Self {
        self.hint_text = Some(text);
        self
    }

    pub(super) fn with_execute_on_selection(mut self) -> Self {
        self.should_execute_on_selection = true;
        self
    }
}

/// A hint describing a slash command's argument, surfaced inline as the user types.
#[derive(Debug, Clone)]
pub struct SlashCommandArgumentHint {
    /// The command name plus a trailing space; callers match this against the current input.
    pub input_prefix: String,
    pub text: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticCommand {
    pub name: &'static str,
    pub description: &'static str,
    pub icon_path: &'static str,
    /// Specifies the requirements for this command to be available. See [`Availability`].
    pub availability: Availability,
    /// Whether this command requires AI mode when executed.
    /// If true, AI mode will be activated when the command is accepted.
    pub auto_enter_ai_mode: bool,
    pub argument: Option<Argument>,
}

/// Stable classification of a static slash command, used by the TUI to dispatch a selected
/// command to its handler.
///
/// Ported from Warp OSS, where it is a `kind` field on every `StaticCommand`. Zap derives it
/// from the command name via [`StaticCommand::kind`] instead, so GUI command definitions do
/// not need to carry a TUI-only field. `Other` covers Zap commands with no upstream kind
/// (e.g. `/pr-comments`); such commands are not TUI-executable so their kind is never consumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlashCommandKind {
    Agent,
    CloudAgent,
    AddMcp,
    AutoApprove,
    Mcp,
    ViewLogs,
    EnableNaturalLanguageDetection,
    DisableNaturalLanguageDetection,
    Exit,
    Logout,
    CreateEnvironment,
    CreateDockerSandbox,
    CreateNewProject,
    EditSkill,
    InvokeSkill,
    AddPrompt,
    AddRule,
    Edit,
    RenameTab,
    RenameConversation,
    SetTabColor,
    Statusline,
    Fork,
    MoveToCloud,
    OpenCodeReview,
    Index,
    Init,
    OpenProjectRules,
    OpenMcpServers,
    OpenSettingsFile,
    Changelog,
    Feedback,
    OpenRepo,
    OpenRules,
    New,
    Model,
    Host,
    Harness,
    Environment,
    Profile,
    Plan,
    Orchestrate,
    Compact,
    CompactAnd,
    Queue,
    ForkAndCompact,
    ForkFrom,
    ContinueLocally,
    Usage,
    RemoteControl,
    Cost,
    Conversations,
    Prompts,
    Rewind,
    ExportToClipboard,
    ExportToFile,
    /// `/api-keys`: opens the BYOP provider API-key manager. Fork-native -- this fork's entire
    /// identity is BYOP, so unlike most `Other`-kind Zap additions this one *is* TUI-executable
    /// and needs its own dispatch arm, not upstream Warp's hardcoded-~4-provider
    /// `/add-api-key` / `/clear-provider-api-key`.
    ApiKeys,
    VimMode,
    /// A Zap command with no upstream `SlashCommandKind` (e.g. `/pr-comments`).
    Other,
}

impl StaticCommand {
    pub fn matches_filter(&self, filter_text: &str) -> bool {
        if filter_text.is_empty() {
            return true;
        }

        let filter_lower = filter_text.to_lowercase();
        self.name
            .to_lowercase()
            .get(1..)
            .unwrap_or("")
            .starts_with(&filter_lower)
    }

    pub fn is_active(&self, session_context: Availability) -> bool {
        session_context.contains(self.availability)
    }

    /// The argument hint for this command, if it declares one. The `input_prefix` is the
    /// command name plus a trailing space, so callers can match it against the current input.
    pub fn argument_hint(&self) -> Option<SlashCommandArgumentHint> {
        let text = self.argument.as_ref()?.hint_text?;
        Some(SlashCommandArgumentHint {
            input_prefix: format!("{} ", self.name),
            text,
        })
    }

    /// Classifies this command for TUI dispatch. Derived from the command name (Warp OSS carries
    /// this as a per-command `kind` field; Zap keeps it out of GUI definitions). Unmapped Zap
    /// commands return [`SlashCommandKind::Other`].
    pub fn kind(&self) -> SlashCommandKind {
        match self.name {
            "/agent" => SlashCommandKind::Agent,
            "/add-mcp" => SlashCommandKind::AddMcp,
            "/mcp" => SlashCommandKind::Mcp,
            "/create-environment" => SlashCommandKind::CreateEnvironment,
            "/docker-sandbox" => SlashCommandKind::CreateDockerSandbox,
            "/create-new-project" => SlashCommandKind::CreateNewProject,
            "/open-skill" => SlashCommandKind::EditSkill,
            "/skills" => SlashCommandKind::InvokeSkill,
            "/add-prompt" => SlashCommandKind::AddPrompt,
            "/add-rule" => SlashCommandKind::AddRule,
            "/open-file" => SlashCommandKind::Edit,
            "/rename-tab" => SlashCommandKind::RenameTab,
            "/set-tab-color" => SlashCommandKind::SetTabColor,
            "/statusline" => SlashCommandKind::Statusline,
            "/fork" => SlashCommandKind::Fork,
            "/handoff" => SlashCommandKind::MoveToCloud,
            "/open-code-review" => SlashCommandKind::OpenCodeReview,
            "/index" => SlashCommandKind::Index,
            "/init" => SlashCommandKind::Init,
            "/open-project-rules" => SlashCommandKind::OpenProjectRules,
            "/open-mcp-servers" => SlashCommandKind::OpenMcpServers,
            "/open-settings-file" => SlashCommandKind::OpenSettingsFile,
            "/changelog" => SlashCommandKind::Changelog,
            "/open-repo" => SlashCommandKind::OpenRepo,
            "/open-rules" => SlashCommandKind::OpenRules,
            "/new" => SlashCommandKind::New,
            "/model" => SlashCommandKind::Model,
            "/profile" => SlashCommandKind::Profile,
            "/compact" => SlashCommandKind::Compact,
            "/compact-and" => SlashCommandKind::CompactAnd,
            "/queue" => SlashCommandKind::Queue,
            "/fork-and-compact" => SlashCommandKind::ForkAndCompact,
            "/fork-from" => SlashCommandKind::ForkFrom,
            "/conversations" => SlashCommandKind::Conversations,
            "/prompts" => SlashCommandKind::Prompts,
            "/rewind" => SlashCommandKind::Rewind,
            "/export-to-clipboard" => SlashCommandKind::ExportToClipboard,
            "/export-to-file" => SlashCommandKind::ExportToFile,
            "/api-keys" => SlashCommandKind::ApiKeys,
            "/vim-mode" => SlashCommandKind::VimMode,
            _ => SlashCommandKind::Other,
        }
    }

    /// Whether this command is implemented on the ratatui TUI surface. Mirrors the
    /// `GuiAndTui`/`TuiOnly` surface classification from Warp OSS for the commands Zap ships.
    pub fn supports_tui(&self) -> bool {
        matches!(
            self.name,
            "/agent"
                | "/create-new-project"
                | "/skills"
                | "/new"
                | "/init"
                | "/model"
                | "/profile"
                | "/prompts"
                | "/compact"
                | "/compact-and"
                | "/queue"
                | "/fork"
                | "/fork-and-compact"
                | "/fork-from"
                | "/rewind"
                | "/conversations"
                | "/export-to-clipboard"
                | "/export-to-file"
                | "/api-keys"
                | "/statusline"
                | "/vim-mode"
        )
    }
}

#[cfg(test)]
#[path = "mod_test.rs"]
mod tests;
