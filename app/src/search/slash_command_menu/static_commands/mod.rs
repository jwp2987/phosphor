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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SlashCommandKind {
    Agent,
    CloudAgent,
    AddMcp,
    AutoApprove,
    Mcp,
    /// `/status`: opens the session-status overlay. Unlike the oracle, carries no
    /// `org`/`email` account fields -- see [`crate::search::slash_command_menu::static_commands::commands::STATUS`].
    Status,
    ViewLogs,
    /// `/natural-language-detection`: one toggle, matching the oracle. Warp's older
    /// `/enable-…` / `/disable-…` pair was collapsed upstream and neither name was ever
    /// mapped in [`StaticCommand::kind`] here, so no reachable command is lost.
    NaturalLanguageDetection,
    Exit,
    /// `/logout`: kept for source parity with the oracle's `SlashCommandKind`, but
    /// deliberately never produced by [`StaticCommand::kind`] and never registered in
    /// [`commands::all_commands`]. BYOP has no Warp account to log out of --
    /// `crate::tui::log_out_tui` (its dispatch target) is a documented no-op for the same
    /// reason (see `app/src/tui/mod.rs`), so registering this command would surface a row
    /// in the `/` menu that does nothing when selected. See #338, DECLINED.md.
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
    /// `/rename-conversation`: kept for source parity with the oracle, but deliberately never
    /// produced by [`StaticCommand::kind`] and never registered. The oracle's rename backend
    /// (`ai::conversation_rename::rename_conversation`) requires a synced
    /// `server_conversation_token` and reports "hasn't synced to the cloud yet" on failure --
    /// it is cloud-coupled, not a local rename. The fork's `BlocklistAIHistoryModel` carries
    /// matching `begin_conversation_rename`/`complete_conversation_rename`/
    /// `fail_conversation_rename` methods, but nothing in the fork calls them; wiring
    /// `/rename-conversation` requires designing a local/BYOP rename path first, which is a
    /// feature decision, not registry wiring. See #147.
    RenameConversation,
    SetTabColor,
    Statusline,
    /// `/reset-statusline`: TUI-only, restores `TuiStatuslineConfig::default()`.
    ResetStatusline,
    /// `/theme`: TUI-only, sets `TuiTheme` (auto/light/dark). See #147.
    Theme,
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
    /// `/clear`: TUI-only alias for `/agent`/`/new` (clears the transcript and starts a new
    /// conversation). Shares `Agent`/`New`'s dispatch arm in
    /// `TuiTerminalSessionView::execute_tui_slash_command`.
    Clear,
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
    /// `/copy-debugging-id`: copies an identifier for the current conversation for the user
    /// to attach to a Phosphor issue.
    CopyDebuggingId,
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
            "/status" => SlashCommandKind::Status,
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
            "/reset-statusline" => SlashCommandKind::ResetStatusline,
            "/theme" => SlashCommandKind::Theme,
            "/auto-approve" => SlashCommandKind::AutoApprove,
            "/natural-language-detection" => SlashCommandKind::NaturalLanguageDetection,
            "/exit" => SlashCommandKind::Exit,
            "/view-logs" => SlashCommandKind::ViewLogs,
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
            "/clear" => SlashCommandKind::Clear,
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
            "/copy-debugging-id" => SlashCommandKind::CopyDebuggingId,
            "/api-keys" => SlashCommandKind::ApiKeys,
            "/vim-mode" => SlashCommandKind::VimMode,
            "/usage" => SlashCommandKind::Usage,
            "/cost" => SlashCommandKind::Cost,
            _ => SlashCommandKind::Other,
        }
    }

    /// Whether this command executes on the GUI surface.
    ///
    /// The oracle carries a `supported_surfaces` field per command and filters both
    /// front-ends through it (`SlashCommandSurfaces::supports_gui`). This fork derives
    /// surfaces from the command name instead (see [`Self::supports_tui`]) and, until
    /// now, only ever derived the TUI half -- so every TUI-only command stayed in the
    /// GUI's active set, and `SlashCommandExecutor` could do nothing about it but
    /// `debug_assert!(false, "Attempted to execute TUI-only slash command in the GUI")`.
    /// That assertion fires for real: `test_submit_queued_prompt_detects_slash_command`
    /// walks `active_commands()` (a `HashMap`, so iteration order varies run to run) and
    /// panicked whenever it happened to land on `/clear` first.
    pub fn supports_gui(&self) -> bool {
        !self.is_tui_only()
    }

    /// Commands that exist only on the ratatui TUI surface.
    ///
    /// Deliberately the exact set `SlashCommandExecutor`'s TUI-only guard enumerates:
    /// those are the commands this fork declares must never reach GUI execution, so
    /// filtering precisely them out of the GUI's set makes that guard unreachable
    /// rather than merely unlikely.
    ///
    /// This is NOT the oracle's full `TuiOnly` list, and copying that list would be a
    /// regression here: the oracle classifies `/status` as TUI-only, but this fork has no
    /// `/status` command at all. Surfaces are a per-fork fact, not something to inherit
    /// wholesale.
    pub fn is_tui_only(&self) -> bool {
        matches!(
            self.name,
            "/statusline"
                | "/reset-statusline"
                | "/auto-approve"
                | "/natural-language-detection"
                | "/exit"
                | "/mcp"
                | "/status"
                | "/view-logs"
                | "/clear"
                | "/theme"
        )
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
                | "/clear"
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
                | "/orchestrate"
                | "/rewind"
                | "/conversations"
                | "/export-to-clipboard"
                | "/export-to-file"
                | "/copy-debugging-id"
                | "/api-keys"
                | "/statusline"
                | "/reset-statusline"
                | "/vim-mode"
                | "/auto-approve"
                | "/natural-language-detection"
                | "/exit"
                | "/mcp"
                | "/status"
                | "/view-logs"
                | "/theme"
                // Both report on local BYOP data (context window, provider token counts x
                // the user's own rates) and open no GUI pane, so AGENTS §5.9 requires them
                // on the TUI too.
                | "/usage"
                | "/cost"
        )
    }
}

#[cfg(test)]
#[path = "mod_test.rs"]
mod tests;
