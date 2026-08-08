mod saved_prompts;
mod zero_state;

use ai::skills::SkillProvider;
pub(crate) use saved_prompts::*;
use warp_core::features::FeatureFlag;
pub use zero_state::*;

use std::collections::HashMap;
use std::path::PathBuf;

use fuzzy_match::FuzzyMatchResult;
use ordered_float::OrderedFloat;
use repo_metadata::repositories::DetectedRepositories;
use warp_core::ui::appearance::Appearance;
use warpui::fonts::FamilyId;
use warpui::{AppContext, Entity, EntityId, ModelContext, ModelHandle, SingletonEntity};

use crate::ai::blocklist::{BlocklistAIHistoryEvent, BlocklistAIHistoryModel};
use crate::ai::skills::{SkillDescriptor, SkillManager};
use crate::search::data_source::{Query, QueryResult};
use crate::search::mixer::DataSourceRunErrorWrapper;
use crate::search::slash_command_menu::fuzzy_match::SlashCommandFuzzyMatchResult;
use crate::search::slash_command_menu::static_commands::Availability;
use crate::terminal::cli_agent_sessions::{
    CLIAgentInputState, CLIAgentSessionsModel, CLIAgentSessionsModelEvent,
};
use crate::terminal::model::session::SessionType;
use warp_core::ui::Icon as WarpIcon;

use super::AcceptSlashCommandOrSavedPrompt;
use crate::{
    ai::blocklist::{
        agent_view::{AgentViewController, AgentViewControllerEvent},
        block::cli_controller::{CLISubagentController, CLISubagentEvent},
    },
    search::{
        slash_command_menu::{
            static_commands::commands::COMMAND_REGISTRY, SlashCommandId, StaticCommand,
        },
        SyncDataSource,
    },
    settings::{AISettings, AISettingsChangedEvent, InputSettings, InputSettingsChangedEvent},
    terminal::model::session::active_session::{ActiveSession, ActiveSessionEvent},
};

pub struct DataSourceArgs {
    pub active_session: ModelHandle<ActiveSession>,
    pub agent_view_controller: ModelHandle<AgentViewController>,
    pub cli_subagent_controller: ModelHandle<CLISubagentController>,
    pub terminal_view_id: EntityId,
}

/// Construction arguments for the ratatui TUI surface, which has no agent-view controller
/// (the TUI is a single always-agent-view surface).
pub struct TuiSlashCommandDataSourceArgs {
    pub active_session: ModelHandle<ActiveSession>,
    pub cli_subagent_controller: ModelHandle<CLISubagentController>,
    pub terminal_view_id: EntityId,
}

/// Type alias for the TUI's slash-command data source. Zap keeps a single concrete
/// `SlashCommandDataSource` (parameterized on whether an agent-view controller is present)
/// rather than Warp's trait + gui/tui split.
pub type TuiSlashCommandDataSource = SlashCommandDataSource;

pub struct SlashCommandDataSource {
    active_session: ModelHandle<ActiveSession>,
    /// `None` on the TUI surface, which has no agent-view controller and is always treated as
    /// agent-view-active.
    agent_view_controller: Option<ModelHandle<AgentViewController>>,
    cli_subagent_controller: ModelHandle<CLISubagentController>,
    terminal_view_id: EntityId,
    active_commands_by_id: HashMap<SlashCommandId, StaticCommand>,
    active_repo_root: Option<PathBuf>,
}

impl SlashCommandDataSource {
    pub fn new(args: DataSourceArgs, ctx: &mut ModelContext<Self>) -> Self {
        let DataSourceArgs {
            active_session,
            agent_view_controller,
            cli_subagent_controller,
            terminal_view_id,
        } = args;
        Self::new_inner(
            active_session,
            Some(agent_view_controller),
            cli_subagent_controller,
            terminal_view_id,
            ctx,
        )
    }

    /// Constructs a data source for the ratatui TUI surface, which has no agent-view controller.
    pub fn new_tui(args: TuiSlashCommandDataSourceArgs, ctx: &mut ModelContext<Self>) -> Self {
        let TuiSlashCommandDataSourceArgs {
            active_session,
            cli_subagent_controller,
            terminal_view_id,
        } = args;
        Self::new_inner(
            active_session,
            None,
            cli_subagent_controller,
            terminal_view_id,
            ctx,
        )
    }

    fn new_inner(
        active_session: ModelHandle<ActiveSession>,
        agent_view_controller: Option<ModelHandle<AgentViewController>>,
        cli_subagent_controller: ModelHandle<CLISubagentController>,
        terminal_view_id: EntityId,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        ctx.subscribe_to_model(&active_session, |me, event, ctx| match event {
            ActiveSessionEvent::UpdatedPwd | ActiveSessionEvent::Bootstrapped => {
                me.recompute_active_commands(ctx);
            }
        });
        ctx.subscribe_to_model(&cli_subagent_controller, |me, event, ctx| {
            if let CLISubagentEvent::SpawnedSubagent { .. }
            | CLISubagentEvent::FinishedSubagent { .. }
            | CLISubagentEvent::UpdatedControl { .. } = event
            {
                me.recompute_active_commands(ctx);
            }
        });
        if let Some(agent_view_controller) = &agent_view_controller {
            ctx.subscribe_to_model(agent_view_controller, |me, event, ctx| match event {
                AgentViewControllerEvent::EnteredAgentView { .. }
                | AgentViewControllerEvent::ExitedAgentView { .. } => {
                    me.recompute_active_commands(ctx);
                }
                _ => (),
            });
        }
        ctx.subscribe_to_model(&AISettings::handle(ctx), |me, event, ctx| {
            if matches!(event, AISettingsChangedEvent::IsAnyAIEnabled { .. }) {
                me.recompute_active_commands(ctx);
            }
        });
        ctx.subscribe_to_model(&InputSettings::handle(ctx), |me, event, ctx| {
            if matches!(
                event,
                InputSettingsChangedEvent::EnableSlashCommandsInTerminal { .. }
            ) {
                me.recompute_active_commands(ctx);
            }
        });
        // `Availability::ACTIVE_CONVERSATION` is derived from
        // `BlocklistAIHistoryModel::active_conversation` (see
        // `recompute_active_commands`), so the active-command set goes stale the
        // moment a conversation is selected or cleared unless we recompute here.
        //
        // The fork had every other recompute trigger but dropped this one, so
        // ACTIVE_CONVERSATION stayed latched at whatever it was when the data
        // source was constructed -- normally "no conversation". Every command
        // gated on that bit (`/fork`, `/fork-and-compact`) was therefore absent
        // from `active_commands_by_id`, so `parse_slash_command` never matched
        // it, `detect_command` returned `None`, and the input fell through to
        // the generic prompt path -- where it was queued instead of executing.
        //
        // That is why the failure looked like an inverted queueing predicate:
        // the queueing code is correct and never even saw a slash command.
        // Ported from the pin, `data_source/core.rs`. Issue #441.
        ctx.subscribe_to_model(&BlocklistAIHistoryModel::handle(ctx), |me, event, ctx| {
            if matches!(
                event,
                BlocklistAIHistoryEvent::SetActiveConversation { .. }
                    | BlocklistAIHistoryEvent::ClearedActiveConversation { .. }
            ) {
                me.recompute_active_commands(ctx);
            }
        });

        ctx.subscribe_to_model(
            &CLIAgentSessionsModel::handle(ctx),
            move |me, event, ctx| {
                if let CLIAgentSessionsModelEvent::InputSessionChanged {
                    terminal_view_id: event_terminal_view_id,
                    ..
                } = event
                {
                    if *event_terminal_view_id == terminal_view_id {
                        me.recompute_active_commands(ctx);
                    }
                }
            },
        );

        let mut me = Self {
            active_session,
            agent_view_controller,
            cli_subagent_controller,
            terminal_view_id,
            active_commands_by_id: Default::default(),
            active_repo_root: None,
        };
        me.recompute_active_commands(ctx);
        me
    }

    /// Slash commands that are available in CLI agent rich input mode.
    /// Add command names here to make them accessible when composing prompts
    /// for a running CLI agent (Claude Code, Codex, etc.).
    const CLI_AGENT_INPUT_ALLOWED_COMMANDS: &[&str] = &["/prompts", "/skills"];

    /// Computes the `Availability` bits for the current session state. Shared by
    /// `recompute_active_commands` (which rebuilds the whole active-command set) and
    /// `command_is_active` (which answers for a single command without waiting for the
    /// next recompute pass).
    fn session_context(&self, ctx: &AppContext) -> Availability {
        let mut session_context = Availability::empty();

        let is_agent_view_active = self.is_agent_view_active(ctx);
        if !FeatureFlag::AgentView.is_enabled() {
            // When the AgentView feature flag is disabled, set both view bits so that
            // either view requirement is satisfied (but other requirements like
            // REPOSITORY and LOCAL still apply).
            session_context |= Availability::AGENT_VIEW | Availability::TERMINAL_VIEW;
        } else if is_agent_view_active {
            session_context |= Availability::AGENT_VIEW;
        } else {
            session_context |= Availability::TERMINAL_VIEW;
        }

        let is_local = self
            .active_session
            .as_ref(ctx)
            .session_type(ctx)
            .is_some_and(|st| st == SessionType::Local);
        if is_local {
            session_context |= Availability::LOCAL;
        }

        // Derive REPOSITORY from the *live* working directory rather than the
        // cached `active_repo_root`. The cache is only refreshed once async git
        // detection resolves and calls `set_active_repo_root`, but a `cd` is
        // reflected in the live cwd immediately — keying off the cache left
        // REPOSITORY-gated commands (e.g. `/open-code-review`) available in the
        // stale window after leaving a repo, until detection caught up.
        // `active_repo_root` is retained solely as the recompute trigger that
        // re-runs this once detection caches a newly-entered repo's root.
        // Ported from the pin's `SlashCommandDataSource::base_availability` /
        // `cwd_is_in_repository` (`data_source/core.rs`). Issue #342.
        if is_local && self.cwd_is_in_repository(ctx) {
            session_context |= Availability::REPOSITORY;
        }

        if !self
            .cli_subagent_controller
            .as_ref(ctx)
            .is_agent_in_control()
        {
            session_context |= Availability::NO_LRC_CONTROL;
        }

        let has_active_conversation = if is_agent_view_active {
            // There is always an active conversation in the agent view.
            true
        } else {
            BlocklistAIHistoryModel::as_ref(ctx)
                .active_conversation(self.terminal_view_id)
                .is_some()
        };
        if has_active_conversation {
            session_context |= Availability::ACTIVE_CONVERSATION;
        }

        if AISettings::as_ref(ctx).is_any_ai_enabled(ctx) {
            session_context |= Availability::AI_ENABLED;
        }

        session_context
    }

    /// Whether `command` would currently be included in `active_commands()`, computed
    /// fresh from session state rather than read from the (possibly stale until the next
    /// `recompute_active_commands`) cached set. Used by tests that need a same-tick
    /// answer for a single command.
    pub(crate) fn command_is_active(&self, command: &StaticCommand, ctx: &AppContext) -> bool {
        let is_cli_agent_input = self.is_cli_agent_input_open(ctx);
        let is_tui_surface = self.agent_view_controller.is_none();
        command.is_active(self.session_context(ctx))
            && (!is_cli_agent_input
                || Self::CLI_AGENT_INPUT_ALLOWED_COMMANDS.contains(&command.name))
            && (!is_tui_surface || command.supports_tui())
            && (is_tui_surface || command.supports_gui())
    }

    fn recompute_active_commands(&mut self, ctx: &mut ModelContext<Self>) {
        let is_cli_agent_input = self.is_cli_agent_input_open(ctx);
        let session_context = self.session_context(ctx);

        // The ratatui TUI surface (no agent-view controller) can only execute the
        // commands gated by `supports_tui`. Filtering here keeps the TUI slash
        // menu honest — it stops advertising GUI-only commands that would no-op
        // (or just insert text) when selected. As commands are ported to the TUI
        // and added to `supports_tui`, they appear here automatically.
        let is_tui_surface = self.agent_view_controller.is_none();
        let old_active_command_count = self.active_commands_by_id.len();
        self.active_commands_by_id = HashMap::from_iter(
            COMMAND_REGISTRY
                .all_commands_by_id()
                .filter(|(_, command)| command.is_active(session_context))
                // When CLI agent input is open, restrict to the explicit allowlist.
                .filter(|(_, command)| {
                    !is_cli_agent_input
                        || Self::CLI_AGENT_INPUT_ALLOWED_COMMANDS.contains(&command.name)
                })
                // On the TUI, only surface commands that actually execute there.
                .filter(|(_, command)| !is_tui_surface || command.supports_tui())
                // ...and the reciprocal, which was missing: on the GUI, drop the
                // TUI-only commands. Without it they stayed selectable in the GUI
                // menu, where executing one hits the TUI-only `debug_assert!` in
                // `SlashCommandExecutor` (a panic in debug, a silent no-op in
                // release). The oracle filters both surfaces off one field; this
                // fork derives them from the command name.
                .filter(|(_, command)| is_tui_surface || command.supports_gui())
                .map(|(id, command)| (id, command.clone())),
        );

        // This is an imperfect heuristic, but better than re-firing unnecessarily.
        //
        // If it actually matters, we can update it.
        if self.active_commands_by_id.len() != old_active_command_count {
            ctx.emit(UpdatedActiveCommands);
        }
    }

    /// Update the active repository root for this terminal. Called by the parent when
    /// the terminal navigates into or out of a git repository.
    ///
    /// This no longer feeds `Availability::REPOSITORY` directly (see
    /// `cwd_is_in_repository`) — it only exists to trigger a recompute once async
    /// git detection resolves and caches a newly-entered repo's root, so the active
    /// command set picks up the change even though the live cwd itself didn't move.
    pub fn set_active_repo_root(
        &mut self,
        repo_root: Option<PathBuf>,
        ctx: &mut ModelContext<Self>,
    ) {
        if self.active_repo_root != repo_root {
            self.active_repo_root = repo_root;
            self.recompute_active_commands(ctx);
        }
    }

    /// Whether the active session's current working directory is inside a detected git
    /// repository. Uses the live cwd (via `DetectedRepositories::get_root_for_path`)
    /// rather than the cached `active_repo_root`, so `Availability::REPOSITORY` updates
    /// immediately on `cd` without waiting for async repo detection to resolve and call
    /// `set_active_repo_root`.
    ///
    /// Ported from the pin's `SlashCommandDataSource::cwd_is_in_repository`
    /// (`data_source/core.rs`), which fixed the same stale-cache regression. Issue #342.
    fn cwd_is_in_repository(&self, ctx: &AppContext) -> bool {
        let Some(cwd) = self.active_session.as_ref(ctx).current_working_directory() else {
            return false;
        };

        // Repo detection converts the shell-native cwd (e.g. the Git
        // Bash/MSYS2/WSL "/c/Users/..." form) to an OS-native path via
        // `ShellLaunchData` before caching the repo root (see the
        // `detect_possible_git_repo` call site in `terminal/view.rs`). The live
        // cwd must go through the same conversion so it can match those cached
        // roots. Fall back to the raw path when no session/launch-data
        // conversion applies (the common native-shell case, where the
        // conversion is already a no-op).
        let path = self
            .active_session
            .as_ref(ctx)
            .session(ctx)
            .and_then(|session| {
                session
                    .launch_data()
                    .and_then(|data| data.maybe_convert_absolute_path(cwd))
            })
            .unwrap_or_else(|| PathBuf::from(cwd));

        DetectedRepositories::as_ref(ctx)
            .get_root_for_path(&path)
            .is_some()
    }

    pub fn active_commands(&self) -> impl Iterator<Item = (&SlashCommandId, &StaticCommand)> {
        self.active_commands_by_id.iter()
    }

    /// Returns the active session handle. Used by the input-parsing helpers to resolve the
    /// current working directory for skill matching.
    pub fn active_session(&self) -> &ModelHandle<ActiveSession> {
        &self.active_session
    }

    pub fn is_agent_view_active(&self, ctx: &AppContext) -> bool {
        // The TUI surface has no agent-view controller; it is always agent-view-active.
        self.agent_view_controller
            .as_ref()
            .is_none_or(|controller| controller.as_ref(ctx).is_active())
    }

    /// Returns whether this surface routes AI work (and thus local skills) to a local
    /// execution host. Remote (e.g. SSH) sessions cannot run local skills. A BYOP fork
    /// with no active session defaults to local.
    pub fn local_skills_available(&self, ctx: &AppContext) -> bool {
        self.active_session
            .as_ref(ctx)
            .session(ctx)
            .is_none_or(|session| session.is_local())
    }

    /// Returns `true` if the CLI agent rich input is currently open for this terminal.
    pub fn is_cli_agent_input_open(&self, ctx: &AppContext) -> bool {
        CLIAgentSessionsModel::as_ref(ctx).is_input_open(self.terminal_view_id)
    }

    /// Returns the supported skill providers for the active CLI agent, or `None` if
    /// CLI agent input is not open (meaning no filtering should be applied).
    pub fn active_cli_agent_providers(
        &self,
        ctx: &AppContext,
    ) -> Option<&'static [ai::skills::SkillProvider]> {
        CLIAgentSessionsModel::as_ref(ctx)
            .session(self.terminal_view_id)
            .filter(|s| matches!(s.input_state, CLIAgentInputState::Open { .. }))
            .map(|s| s.agent.supported_skill_providers())
    }
}

impl SyncDataSource for SlashCommandDataSource {
    type Action = AcceptSlashCommandOrSavedPrompt;

    fn run_query(
        &self,
        query: &Query,
        app: &warpui::AppContext,
    ) -> Result<Vec<QueryResult<Self::Action>>, DataSourceRunErrorWrapper> {
        if query.text.is_empty() {
            return Ok(vec![]);
        }

        let query_text = query.text.trim().to_lowercase();

        let mut results = Vec::new();

        /// Multiplier to ensure static commands always appear at the top of the match results.
        const SCORE_MULTIPLIER: OrderedFloat<f64> = OrderedFloat(1000.0);

        for (id, command) in self.active_commands_by_id.iter() {
            if let Some(fuzzy_result) = SlashCommandFuzzyMatchResult::try_match(
                &query_text,
                command.name,
                None, // Don't match on description for slash commands.
            ) {
                let score = fuzzy_result.score();

                // Only include results with score > 25 once the user has started typing a query and is past the first character
                if query_text.len() > 1 && score <= 25.0 {
                    continue;
                }

                // Boost prefix matches so that closer matches (e.g. "new" → "/new")
                // rank above longer fuzzy matches (e.g. "new" → "/create-new-project").
                let prefix_boost = prefix_match_bonus(&query_text, command.name);

                results.push(QueryResult::from(
                    InlineItem::from_slash_command(id, command, app)
                        .with_name_match_result(fuzzy_result.name_match_result)
                        .with_description_match_result(fuzzy_result.description_match_result)
                        .with_score(
                            OrderedFloat(score) * SCORE_MULTIPLIER
                                + OrderedFloat(prefix_boost) * SCORE_MULTIPLIER
                                // Boost commands with shorter names, if match result is otherwise
                                // equal.
                                + OrderedFloat(1. / command.name.len() as f64),
                        ),
                ));
            }
        }

        // Also search skills — when CLI agent input is open, filter to natively supported providers.
        // Skills are invoked by the agent, so they're hidden entirely when AI is globally off.
        if FeatureFlag::ListSkills.is_enabled() && AISettings::as_ref(app).is_any_ai_enabled(app) {
            let cli_agent_providers = self.active_cli_agent_providers(app);
            let cwd = self.active_session.as_ref(app).current_working_directory();
            let cwd_path = cwd.as_ref().map(std::path::Path::new);
            let skills = SkillManager::handle(app)
                .as_ref(app)
                .get_skills_for_working_directory(cwd_path, app);

            let skill_manager = SkillManager::as_ref(app);
            for mut skill in skills {
                // In CLI agent input mode, only show skills that exist in a supported
                // provider folder. We check all paths (not just the deduplicated
                // provider) because deduplication may have picked a higher-priority
                // provider even when the skill also exists in the CLI agent's folder.
                if let Some(providers) = &cli_agent_providers {
                    if !skill_manager.skill_exists_for_any_provider(&skill, providers) {
                        continue;
                    }
                    // Re-map the provider to the best supported one so the icon
                    // reflects the active CLI agent's native provider.
                    skill.provider = skill_manager.best_supported_provider(&skill, providers);
                }
                if let Some(fuzzy_result) = SlashCommandFuzzyMatchResult::try_match(
                    &query_text,
                    &skill.name,
                    Some(&skill.description),
                ) {
                    let score = fuzzy_result.score();

                    // Only include results with score > 25 once the user has started typing a query
                    if query_text.len() > 1 && score <= 25.0 {
                        continue;
                    }

                    let prefix_boost = prefix_match_bonus(&query_text, &skill.name);

                    results.push(QueryResult::from(
                        InlineItem::from_skill(&skill, app)
                            .with_name_match_result(fuzzy_result.name_match_result)
                            .with_description_match_result(fuzzy_result.description_match_result)
                            .with_score(
                                OrderedFloat(score) * SCORE_MULTIPLIER
                                    + OrderedFloat(prefix_boost) * SCORE_MULTIPLIER
                                    + OrderedFloat(1. / skill.name.len() as f64),
                            ),
                    ));
                }
            }
        }

        Ok(results)
    }
}

/// Computes a bonus score for slash command matches where the query is a prefix
/// of the command name. This ensures closer matches (e.g., "new" → "/new") rank
/// above longer fuzzy matches (e.g., "new" → "/figma-create-new-file").
///
/// Returns a value in `[0.0, 100.0]` based on the query's coverage of the name.
/// An exact match yields the maximum bonus of 100; partial prefix matches yield
/// a proportionally smaller bonus.
fn prefix_match_bonus(query: &str, name: &str) -> f64 {
    let name_lower = name.to_lowercase();
    let name_stripped = name_lower.strip_prefix('/').unwrap_or(&name_lower);
    if name_stripped.starts_with(query) {
        // coverage = 1.0 for exact match, smaller for partial prefix match.
        let coverage = query.len() as f64 / name_stripped.len() as f64;
        coverage * 100.0
    } else {
        0.0
    }
}

#[derive(Debug, Clone, Copy)]
pub struct UpdatedActiveCommands;

impl Entity for SlashCommandDataSource {
    type Event = UpdatedActiveCommands;
}

#[derive(Debug, Clone)]
pub struct InlineItem {
    pub action: AcceptSlashCommandOrSavedPrompt,
    pub icon_path: &'static str,
    pub name: String,
    pub description: Option<String>,
    pub font_family: FamilyId,
    pub name_match_result: Option<FuzzyMatchResult>,
    pub description_match_result: Option<FuzzyMatchResult>,
    pub score: OrderedFloat<f64>,
}

impl InlineItem {
    fn from_slash_command(
        command_id: &SlashCommandId,
        command: &StaticCommand,
        app: &AppContext,
    ) -> Self {
        let appearance = Appearance::as_ref(app);
        Self {
            action: AcceptSlashCommandOrSavedPrompt::SlashCommand { id: *command_id },
            icon_path: command.icon_path,
            name: command.name.to_owned(),
            description: Some(command.description.to_owned()),
            font_family: appearance.monospace_font_family(),
            name_match_result: None,
            description_match_result: None,
            score: OrderedFloat(f64::MIN),
        }
    }

    pub(super) fn from_skill(skill: &SkillDescriptor, app: &AppContext) -> Self {
        let appearance = Appearance::handle(app).as_ref(app);
        // Use icon_override if set (e.g. Figma skills), otherwise derive from provider.
        let icon = if let Some(override_icon) = skill.icon_override {
            override_icon
        } else {
            match skill.provider {
                SkillProvider::Zap => WarpIcon::Zap,
                SkillProvider::Claude => WarpIcon::ClaudeLogo,
                SkillProvider::Codex => WarpIcon::OpenAILogo,
                SkillProvider::Gemini => WarpIcon::GeminiLogo,
                SkillProvider::Droid => WarpIcon::DroidLogo,
                SkillProvider::OpenCode => WarpIcon::OpenCodeLogo,
                _ => WarpIcon::Zap,
            }
        };

        Self {
            action: AcceptSlashCommandOrSavedPrompt::Skill {
                reference: skill.reference.clone(),
                name: skill.name.clone(),
            },
            icon_path: icon.into(),
            name: format!("/{}", &skill.name),
            description: Some(skill.description.clone()),
            font_family: appearance.monospace_font_family(),
            name_match_result: None,
            description_match_result: None,
            score: OrderedFloat(f64::MIN),
        }
    }

    fn with_name_match_result(mut self, result: Option<FuzzyMatchResult>) -> Self {
        self.name_match_result = result;
        self
    }

    fn with_description_match_result(mut self, result: Option<FuzzyMatchResult>) -> Self {
        self.description_match_result = result;
        self
    }

    fn with_score(mut self, score: OrderedFloat<f64>) -> Self {
        self.score = score;
        self
    }
}

#[cfg(test)]
#[path = "mod_test.rs"]
mod tests;
