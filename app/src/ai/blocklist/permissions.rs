use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use crate::{
    ai::{
        agent::{FileEdit, conversation::AIConversationId},
        execution_profiles::{
            AIExecutionProfile, ActionPermission, AskUserQuestionPermission, WriteToPtyPermission,
            profiles::{AIExecutionProfilesModel, ClientProfileId},
        },
    },
    report_if_error,
    settings::{AISettings, AgentModeCodingPermissionsType, AgentModeCommandExecutionPredicate},
    workspaces::{user_workspaces::UserWorkspaces, workspace::AiAutonomySettings},
};
use warp_core::execution_mode::AppExecutionMode;

use crate::ai::paths::host_native_absolute_path;
use crate::terminal::ShellLaunchData;

use super::protected_paths::is_protected_write_path;
#[cfg(not(target_family = "wasm"))]
use crate::ai::mcp::TemplatableMCPServerManager;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use warp_completer::parsers::simple::{
    command_without_leading_env_vars, decompose_command, executed_commands, unquoted_command_parts,
};
use warp_core::user_preferences::GetUserPreferences;
use warp_core::{features::FeatureFlag, settings::Setting};
use warp_util::path::EscapeChar;
use warpui::{AppContext, Entity, EntityId, ModelContext, SingletonEntity};

use super::BlocklistAIHistoryModel;

/// Whether or not a command can be auto-executed, along with a detailed reason.
#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
pub enum CommandExecutionPermission {
    Allowed(CommandExecutionPermissionAllowedReason),
    Denied(CommandExecutionPermissionDeniedReason),
}

/// Why a command can be auto-executed.
#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
pub enum CommandExecutionPermissionAllowedReason {
    Dispatched,
    ExplicitlyAllowlisted,
    IsReadOnlyAndSettingEnabled,
    AgentDecided,
    AlwaysAllowed,
    RunToCompletion,
}

/// Why a command can't be auto-executed.
#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
pub enum CommandExecutionPermissionDeniedReason {
    AutonomyForceDisabled,
    AlwaysAskEnabled,
    ExplicitlyDenylisted,
    ContainsRedirection,
    Inconclusive,
    AgentDecided,
    /// A denylist applies, but some command the line executes cannot be determined without
    /// running it (`$CMD`, `$(which rm)`, a glob, `python -c` code, `xargs env`, PowerShell
    /// outside the analysed subset, unparseable input), so the denylist cannot vouch for it.
    /// Fails closed: the user confirms. See
    /// `warp_completer::parsers::simple::executed_commands`.
    UnresolvedCommandWord,
}

impl CommandExecutionPermission {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed(..))
    }
}

/// Whether or not a file can be auto-read, along with a detailed reason.
#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
pub enum FileReadPermission {
    Allowed(FileReadPermissionAllowedReason),
    Denied(FileReadPermissionDeniedReason),
}

/// Why a file can be auto-read.
#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
pub enum FileReadPermissionAllowedReason {
    Dispatched,
    AlreadyReadInConvo,
    ExplicitlyAllowlisted,
    AutoreadSettingEnabled,
    AgentDecided,
    RunToCompletion,
}

/// Why a file can't be auto-read.
#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
pub enum FileReadPermissionDeniedReason {
    AutonomyForceDisabled,
    AlwaysAskEnabled,
    Inconclusive,
    AgentDecided,
}

impl FileReadPermission {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed(..))
    }
}

/// Whether or not a file can be auto-written, along with a detailed reason.
#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
pub enum FileWritePermission {
    Allowed(FileWritePermissionAllowedReason),
    Denied(FileWritePermissionDeniedReason),
}

impl FileWritePermission {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed(..))
    }
}

/// Why a file can be written automatically.
#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
pub enum FileWritePermissionAllowedReason {
    Dispatched,
    AgentDecided,
    AutowriteSettingEnabled,
    RunToCompletion,
}

/// Why a file can't be written automatically.
#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
pub enum FileWritePermissionDeniedReason {
    AutonomyForceDisabled,
    AlwaysAskEnabled,
    Inconclusive,
    AgentDecided,
    /// The path is on the protected list (`blocklist::protected_paths`: MCP and agent
    /// configs, this app's settings, ssh, shell startup files, git hooks, ...) and must never
    /// be written without the user's explicit confirmation, whatever the autonomy settings.
    ProtectedPath,
}

/// Describes permissions that Agent Mode has, backed by [`AISettings`].
pub struct BlocklistAIPermissions {
    /// A set of one-off files that the user has allowed Agent Mode
    /// to read for the duration of a given conversation.
    ///
    /// TODO: remove this once AM doesn't re-request access to the same file in a given convo.
    temporary_file_permissions: HashMap<AIConversationId, HashSet<PathBuf>>,
}

impl BlocklistAIPermissions {
    pub fn new(ctx: &mut ModelContext<Self>) -> Self {
        // Migrate the old `AgentModeAutoReadFiles` setting to the new [`AgentModeCodingPermissionsType`].
        if let Some(can_read_files) = ctx
            .private_user_preferences()
            .read_value("AgentModeAutoReadFiles")
            .ok()
            .flatten()
            .and_then(|s| s.parse().ok())
        {
            if let Err(e) = ctx
                .private_user_preferences()
                .remove_value("AgentModeAutoReadFiles")
            {
                log::error!("Failed to remove old AgentModeAutoReadFiles user pref: {e}");
            }
            if can_read_files {
                report_if_error!(AISettings::handle(ctx).update(ctx, |settings, ctx| {
                    settings
                        .agent_mode_coding_permissions
                        .set_value(AgentModeCodingPermissionsType::AlwaysAllowReading, ctx)
                }));
            }
        }

        Self {
            temporary_file_permissions: Default::default(),
        }
    }

    /// Returns the active permissions profile, accounting for any enterprise overrides.
    pub fn permissions_profile_for_id(
        &self,
        ctx: &AppContext,
        profile_id: ClientProfileId,
    ) -> AIExecutionProfile {
        let profiles_model = AIExecutionProfilesModel::as_ref(ctx);
        let profile = profiles_model
            .get_profile_by_id(profile_id, ctx)
            .unwrap_or_else(|| profiles_model.default_profile(ctx));
        let profile_data = profile.data();

        AIExecutionProfile {
            // Some fields may have an enterprise override.
            apply_code_diffs: self.get_apply_code_diffs_setting_for_profile(ctx, profile_id),
            read_files: self.get_read_files_setting_for_profile(ctx, profile_id),
            execute_commands: self.get_execute_commands_setting_for_profile(ctx, profile_id),
            mcp_permissions: self.get_mcp_permissions_setting_for_profile(ctx, profile_id),
            write_to_pty: self.get_write_to_pty_setting_for_profile(ctx, profile_id),
            command_allowlist: self.get_execute_commands_allowlist_for_profile(ctx, profile_id),
            command_denylist: self.get_execute_commands_denylist_for_profile(ctx, profile_id),
            directory_allowlist: self.get_read_files_allowlist_for_profile(ctx, profile_id),
            mcp_allowlist: self.get_mcp_allowlist_for_profile(ctx, profile_id),
            mcp_denylist: self.get_mcp_denylist_for_profile(ctx, profile_id),
            computer_use: self.get_computer_use_setting_for_profile(ctx, profile_id),
            ask_user_question: self.get_ask_user_question_setting_for_profile(ctx, profile_id),

            // Some fields are read directly from the profile.
            name: profile_data.name.clone(),
            is_default_profile: profile_data.is_default_profile,
            base_model: profile_data.base_model.clone(),
            coding_model: profile_data.coding_model.clone(),
            cli_agent_model: profile_data.cli_agent_model.clone(),
            computer_use_model: profile_data.computer_use_model.clone(),
            title_model: profile_data.title_model.clone(),
            active_ai_model: profile_data.active_ai_model.clone(),
            next_command_model: profile_data.next_command_model.clone(),
            context_window_limit: profile_data.context_window_limit,
            autosync_plans_to_warp_drive: profile_data.autosync_plans_to_warp_drive,
            web_search_enabled: profile_data.web_search_enabled,
            codebase_context_enabled: profile_data.codebase_context_enabled,
            prompt_overrides: profile_data.prompt_overrides.clone(),
        }
    }

    pub fn active_permissions_profile(
        &self,
        ctx: &AppContext,
        terminal_view_id: Option<EntityId>,
    ) -> AIExecutionProfile {
        let active_profile =
            AIExecutionProfilesModel::as_ref(ctx).active_profile(terminal_view_id, ctx);
        self.permissions_profile_for_id(ctx, *active_profile.id())
    }

    /// Returns the applicable workspace autonomy settings based on execution mode.
    /// In sandboxed mode, returns settings derived from the sandboxed agent config.
    /// In unsandboxed mode, returns the standard AI autonomy settings.
    fn workspace_autonomy_settings(ctx: &AppContext) -> AiAutonomySettings {
        if AppExecutionMode::as_ref(ctx).is_sandboxed() {
            let sandboxed = UserWorkspaces::as_ref(ctx).sandboxed_agent_settings();
            AiAutonomySettings {
                execute_commands_denylist: sandboxed.and_then(|s| s.execute_commands_denylist),
                ..Default::default()
            }
        } else {
            UserWorkspaces::as_ref(ctx).ai_autonomy_settings()
        }
    }

    pub fn get_apply_code_diffs_setting_for_profile(
        &self,
        ctx: &AppContext,
        profile_id: ClientProfileId,
    ) -> ActionPermission {
        let autonomy_settings = Self::workspace_autonomy_settings(ctx);
        let apply_code_diffs_workspace_setting = autonomy_settings.apply_code_diffs_setting;

        apply_code_diffs_workspace_setting.unwrap_or_else(|| {
            let profiles_model = AIExecutionProfilesModel::as_ref(ctx);
            profiles_model
                .get_profile_by_id(profile_id, ctx)
                .unwrap_or_else(|| profiles_model.default_profile(ctx))
                .data()
                .apply_code_diffs
        })
    }

    /// Returns what the current setting is for applying code diffs,
    /// based on the workspace setting and the active profile.
    pub fn get_apply_code_diffs_setting(
        &self,
        ctx: &AppContext,
        terminal_view_id: Option<EntityId>,
    ) -> ActionPermission {
        let active_profile =
            AIExecutionProfilesModel::as_ref(ctx).active_profile(terminal_view_id, ctx);

        self.get_apply_code_diffs_setting_for_profile(ctx, *active_profile.id())
    }

    pub fn get_read_files_setting_for_profile(
        &self,
        ctx: &AppContext,
        profile_id: ClientProfileId,
    ) -> ActionPermission {
        let autonomy_settings = Self::workspace_autonomy_settings(ctx);
        let read_files_workspace_setting = autonomy_settings.read_files_setting;

        read_files_workspace_setting.unwrap_or_else(|| {
            let profiles_model = AIExecutionProfilesModel::as_ref(ctx);
            profiles_model
                .get_profile_by_id(profile_id, ctx)
                .unwrap_or_else(|| profiles_model.default_profile(ctx))
                .data()
                .read_files
        })
    }

    /// Returns what the current setting is for reading files,
    /// based on the workspace setting and the active profile.
    pub fn get_read_files_setting(
        &self,
        ctx: &AppContext,
        terminal_view_id: Option<EntityId>,
    ) -> ActionPermission {
        let active_profile =
            AIExecutionProfilesModel::as_ref(ctx).active_profile(terminal_view_id, ctx);
        self.get_read_files_setting_for_profile(ctx, *active_profile.id())
    }

    pub fn get_read_files_allowlist_for_profile(
        &self,
        ctx: &AppContext,
        profile_id: ClientProfileId,
    ) -> Vec<PathBuf> {
        let autonomy_settings = Self::workspace_autonomy_settings(ctx);
        let read_files_workspace_allowlist = autonomy_settings.read_files_allowlist;

        read_files_workspace_allowlist.unwrap_or_else(|| {
            let profiles_model = AIExecutionProfilesModel::as_ref(ctx);
            profiles_model
                .get_profile_by_id(profile_id, ctx)
                .unwrap_or_else(|| profiles_model.default_profile(ctx))
                .data()
                .directory_allowlist
                .clone()
        })
    }

    /// Returns an allowlist of paths that AM should be able to auto-read.
    /// Note that the caller is responsible for deciding how the workspace's/user's settings
    /// should affect how this gets used, if at all.
    pub fn get_read_files_allowlist(
        &self,
        ctx: &AppContext,
        terminal_view_id: Option<EntityId>,
    ) -> Vec<PathBuf> {
        let active_profile =
            AIExecutionProfilesModel::as_ref(ctx).active_profile(terminal_view_id, ctx);
        self.get_read_files_allowlist_for_profile(ctx, *active_profile.id())
    }

    pub fn get_execute_commands_setting_for_profile(
        &self,
        ctx: &AppContext,
        profile_id: ClientProfileId,
    ) -> ActionPermission {
        let autonomy_settings = Self::workspace_autonomy_settings(ctx);
        let execute_commands_workspace_setting = autonomy_settings.execute_commands_setting;

        execute_commands_workspace_setting.unwrap_or_else(|| {
            let profiles_model = AIExecutionProfilesModel::as_ref(ctx);
            profiles_model
                .get_profile_by_id(profile_id, ctx)
                .unwrap_or_else(|| profiles_model.default_profile(ctx))
                .data()
                .execute_commands
        })
    }

    /// Returns what the current setting is for executing commands,
    /// based on the workspace setting and the active profile.
    pub fn get_execute_commands_setting(
        &self,
        ctx: &AppContext,
        terminal_view_id: Option<EntityId>,
    ) -> ActionPermission {
        let active_profile =
            AIExecutionProfilesModel::as_ref(ctx).active_profile(terminal_view_id, ctx);
        self.get_execute_commands_setting_for_profile(ctx, *active_profile.id())
    }

    pub fn get_execute_commands_allowlist_for_profile(
        &self,
        ctx: &AppContext,
        profile_id: ClientProfileId,
    ) -> Vec<AgentModeCommandExecutionPredicate> {
        let autonomy_settings = Self::workspace_autonomy_settings(ctx);
        let execute_commands_workspace_allowlist = autonomy_settings.execute_commands_allowlist;

        execute_commands_workspace_allowlist.unwrap_or_else(|| {
            let profiles_model = AIExecutionProfilesModel::as_ref(ctx);
            profiles_model
                .get_profile_by_id(profile_id, ctx)
                .unwrap_or_else(|| profiles_model.default_profile(ctx))
                .data()
                .command_allowlist
                .clone()
        })
    }

    /// Returns an allowlist of command regexes that AM should be able to auto-execute.
    /// Note that the caller is responsible for deciding how the workspace's/user's settings
    /// should affect how this gets used, if at all.
    pub fn get_execute_commands_allowlist(
        &self,
        ctx: &AppContext,
        terminal_view_id: Option<EntityId>,
    ) -> Vec<AgentModeCommandExecutionPredicate> {
        let active_profile =
            AIExecutionProfilesModel::as_ref(ctx).active_profile(terminal_view_id, ctx);
        self.get_execute_commands_allowlist_for_profile(ctx, *active_profile.id())
    }

    pub fn get_execute_commands_denylist_for_profile(
        &self,
        ctx: &AppContext,
        profile_id: ClientProfileId,
    ) -> Vec<AgentModeCommandExecutionPredicate> {
        let autonomy_settings = Self::workspace_autonomy_settings(ctx);
        let profiles_model = AIExecutionProfilesModel::as_ref(ctx);
        let user_denylist = profiles_model
            .get_profile_by_id(profile_id, ctx)
            .unwrap_or_else(|| profiles_model.default_profile(ctx))
            .data()
            .command_denylist
            .clone();

        // A workspace-level denylist *adds to* the profile's own denylist rather than
        // replacing it: an org policy must never be able to silently drop entries the
        // user configured locally. Duplicates are collapsed so a predicate that appears
        // in both lists is reported once.
        match autonomy_settings.execute_commands_denylist {
            Some(org_denylist) => {
                let mut merged = org_denylist;
                for item in user_denylist {
                    if !merged.contains(&item) {
                        merged.push(item);
                    }
                }
                merged
            }
            None => user_denylist,
        }
    }

    /// Returns only the workspace-level (org policy) command denylist, without the
    /// active profile's own entries merged in.
    pub fn get_org_execute_commands_denylist(
        ctx: &AppContext,
    ) -> Vec<AgentModeCommandExecutionPredicate> {
        Self::workspace_autonomy_settings(ctx)
            .execute_commands_denylist
            .unwrap_or_default()
    }

    /// Returns a denylist of command regexes that AM should not auto-execute.
    /// Note that the caller is responsible for deciding how the workspace's/user's settings
    /// should affect how this gets used, if at all.
    pub fn get_execute_commands_denylist(
        &self,
        ctx: &AppContext,
        terminal_view_id: Option<EntityId>,
    ) -> Vec<AgentModeCommandExecutionPredicate> {
        let active_profile =
            AIExecutionProfilesModel::as_ref(ctx).active_profile(terminal_view_id, ctx);
        self.get_execute_commands_denylist_for_profile(ctx, *active_profile.id())
    }

    pub fn get_write_to_pty_setting_for_profile(
        &self,
        ctx: &AppContext,
        profile_id: ClientProfileId,
    ) -> WriteToPtyPermission {
        let autonomy_settings = Self::workspace_autonomy_settings(ctx);
        let write_to_pty_workspace_setting = autonomy_settings.write_to_pty_setting;

        write_to_pty_workspace_setting.unwrap_or_else(|| {
            let profiles_model = AIExecutionProfilesModel::as_ref(ctx);
            profiles_model
                .get_profile_by_id(profile_id, ctx)
                .unwrap_or_else(|| profiles_model.default_profile(ctx))
                .data()
                .write_to_pty
        })
    }

    pub fn get_write_to_pty_setting(
        &self,
        ctx: &AppContext,
        terminal_view_id: Option<EntityId>,
    ) -> WriteToPtyPermission {
        let active_profile =
            AIExecutionProfilesModel::as_ref(ctx).active_profile(terminal_view_id, ctx);
        self.get_write_to_pty_setting_for_profile(ctx, *active_profile.id())
    }

    pub fn can_write_to_pty(
        &self,
        conversation_id: &AIConversationId,
        terminal_view_id: Option<EntityId>,
        ctx: &AppContext,
    ) -> WriteToPtyPermission {
        if BlocklistAIHistoryModel::as_ref(ctx)
            .conversation(conversation_id)
            .is_some_and(|convo| convo.autoexecute_any_action())
        {
            return WriteToPtyPermission::AlwaysAllow;
        }
        self.get_write_to_pty_setting(ctx, terminal_view_id)
    }

    pub fn get_mcp_permissions_setting_for_profile(
        &self,
        ctx: &AppContext,
        profile_id: ClientProfileId,
    ) -> ActionPermission {
        // TODO: allow a workspace override on MCP permissions.

        let profiles_model = AIExecutionProfilesModel::as_ref(ctx);
        profiles_model
            .get_profile_by_id(profile_id, ctx)
            .unwrap_or_else(|| profiles_model.default_profile(ctx))
            .data()
            .mcp_permissions
    }

    pub fn get_mcp_permissions_setting(
        &self,
        ctx: &AppContext,
        terminal_view_id: Option<EntityId>,
    ) -> ActionPermission {
        let active_profile =
            AIExecutionProfilesModel::as_ref(ctx).active_profile(terminal_view_id, ctx);
        self.get_mcp_permissions_setting_for_profile(ctx, *active_profile.id())
    }

    pub fn get_mcp_allowlist_for_profile(
        &self,
        ctx: &AppContext,
        profile_id: ClientProfileId,
    ) -> Vec<uuid::Uuid> {
        // TODO: allow a workspace override on MCP allowlist.

        let profiles_model = AIExecutionProfilesModel::as_ref(ctx);
        profiles_model
            .get_profile_by_id(profile_id, ctx)
            .unwrap_or_else(|| profiles_model.default_profile(ctx))
            .data()
            .mcp_allowlist
            .clone()
    }

    pub fn get_mcp_allowlist(
        &self,
        ctx: &AppContext,
        terminal_view_id: Option<EntityId>,
    ) -> Vec<uuid::Uuid> {
        let active_profile =
            AIExecutionProfilesModel::as_ref(ctx).active_profile(terminal_view_id, ctx);
        self.get_mcp_allowlist_for_profile(ctx, *active_profile.id())
    }

    pub fn get_mcp_denylist_for_profile(
        &self,
        ctx: &AppContext,
        profile_id: ClientProfileId,
    ) -> Vec<uuid::Uuid> {
        // TODO: allow a workspace override on MCP denylist.

        let profiles_model = AIExecutionProfilesModel::as_ref(ctx);
        profiles_model
            .get_profile_by_id(profile_id, ctx)
            .unwrap_or_else(|| profiles_model.default_profile(ctx))
            .data()
            .mcp_denylist
            .clone()
    }

    pub fn get_mcp_denylist(
        &self,
        ctx: &AppContext,
        terminal_view_id: Option<EntityId>,
    ) -> Vec<uuid::Uuid> {
        let active_profile =
            AIExecutionProfilesModel::as_ref(ctx).active_profile(terminal_view_id, ctx);
        self.get_mcp_denylist_for_profile(ctx, *active_profile.id())
    }

    pub fn get_web_search_enabled_for_profile(
        &self,
        ctx: &AppContext,
        profile_id: ClientProfileId,
    ) -> bool {
        let profiles_model = AIExecutionProfilesModel::as_ref(ctx);
        profiles_model
            .get_profile_by_id(profile_id, ctx)
            .unwrap_or_else(|| profiles_model.default_profile(ctx))
            .data()
            .web_search_enabled
    }

    pub fn get_web_search_enabled(
        &self,
        ctx: &AppContext,
        terminal_view_id: Option<EntityId>,
    ) -> bool {
        let active_profile =
            AIExecutionProfilesModel::as_ref(ctx).active_profile(terminal_view_id, ctx);
        self.get_web_search_enabled_for_profile(ctx, *active_profile.id())
    }

    pub fn get_codebase_context_enabled_for_profile(
        &self,
        ctx: &AppContext,
        profile_id: ClientProfileId,
    ) -> bool {
        let profiles_model = AIExecutionProfilesModel::as_ref(ctx);
        profiles_model
            .get_profile_by_id(profile_id, ctx)
            .unwrap_or_else(|| profiles_model.default_profile(ctx))
            .data()
            .codebase_context_enabled
    }

    pub fn get_codebase_context_enabled(
        &self,
        ctx: &AppContext,
        terminal_view_id: Option<EntityId>,
    ) -> bool {
        let active_profile =
            AIExecutionProfilesModel::as_ref(ctx).active_profile(terminal_view_id, ctx);
        self.get_codebase_context_enabled_for_profile(ctx, *active_profile.id())
    }

    pub fn get_computer_use_setting_for_profile(
        &self,
        ctx: &AppContext,
        profile_id: ClientProfileId,
    ) -> crate::ai::execution_profiles::ComputerUsePermission {
        let autonomy_settings = Self::workspace_autonomy_settings(ctx);
        let computer_use_workspace_setting = autonomy_settings.computer_use_setting;

        computer_use_workspace_setting.unwrap_or_else(|| {
            let profiles_model = AIExecutionProfilesModel::as_ref(ctx);
            profiles_model
                .get_profile_by_id(profile_id, ctx)
                .unwrap_or_else(|| profiles_model.default_profile(ctx))
                .data()
                .computer_use
        })
    }

    pub fn get_computer_use_setting(
        &self,
        ctx: &AppContext,
        terminal_view_id: Option<EntityId>,
    ) -> crate::ai::execution_profiles::ComputerUsePermission {
        let active_profile =
            AIExecutionProfilesModel::as_ref(ctx).active_profile(terminal_view_id, ctx);
        self.get_computer_use_setting_for_profile(ctx, *active_profile.id())
    }

    pub fn get_ask_user_question_setting_for_profile(
        &self,
        ctx: &AppContext,
        profile_id: ClientProfileId,
    ) -> AskUserQuestionPermission {
        let profiles_model = AIExecutionProfilesModel::as_ref(ctx);
        profiles_model
            .get_profile_by_id(profile_id, ctx)
            .unwrap_or_else(|| profiles_model.default_profile(ctx))
            .data()
            .ask_user_question
    }

    pub fn get_ask_user_question_setting(
        &self,
        ctx: &AppContext,
        terminal_view_id: Option<EntityId>,
    ) -> AskUserQuestionPermission {
        let active_profile =
            AIExecutionProfilesModel::as_ref(ctx).active_profile(terminal_view_id, ctx);
        self.get_ask_user_question_setting_for_profile(ctx, *active_profile.id())
    }

    /// Returns whether or not Agent Mode can auto-read the given files.
    pub fn can_read_files_with_conversation(
        &self,
        conversation_id: &AIConversationId,
        paths: Vec<PathBuf>,
        terminal_view_id: Option<EntityId>,
        ctx: &AppContext,
    ) -> FileReadPermission {
        if BlocklistAIHistoryModel::as_ref(ctx)
            .conversation(conversation_id)
            .is_some_and(|convo| convo.autoexecute_any_action())
        {
            return FileReadPermission::Allowed(FileReadPermissionAllowedReason::RunToCompletion);
        }

        self.can_read_files(Some(conversation_id), paths, terminal_view_id, ctx)
    }

    /// Returns whether or not Zap can auto-read the given files (e.g. for codebase indexing).
    pub fn can_read_files(
        &self,
        conversation_id: Option<&AIConversationId>,
        paths: Vec<PathBuf>,
        terminal_view_id: Option<EntityId>,
        ctx: &AppContext,
    ) -> FileReadPermission {
        if paths.is_empty() {
            // We can vacuously read 0 files.
            return FileReadPermission::Allowed(
                FileReadPermissionAllowedReason::ExplicitlyAllowlisted,
            );
        }

        // Check if we've already been given permission to read these files in this conversation.
        if let Some(temp_permissions) =
            conversation_id.and_then(|id| self.temporary_file_permissions.get(id))
        {
            if paths.iter().all(|path| {
                temp_permissions
                    .iter()
                    .any(|allowed| path.starts_with(allowed))
            }) {
                return FileReadPermission::Allowed(
                    FileReadPermissionAllowedReason::AlreadyReadInConvo,
                );
            }
        }

        match self.get_read_files_setting(ctx, terminal_view_id) {
            ActionPermission::AgentDecides | ActionPermission::Unknown => {
                // For now, we always read files. We don't ask the user for permission.
                FileReadPermission::Allowed(FileReadPermissionAllowedReason::AgentDecided)
            }
            ActionPermission::AlwaysAllow => {
                FileReadPermission::Allowed(FileReadPermissionAllowedReason::AutoreadSettingEnabled)
            }
            ActionPermission::AlwaysAsk => {
                let allowlisted_paths = self.get_read_files_allowlist(ctx, terminal_view_id);
                if paths
                    .iter()
                    .all(|p| allowlisted_paths.iter().any(|dir| p.starts_with(dir)))
                {
                    FileReadPermission::Allowed(
                        FileReadPermissionAllowedReason::ExplicitlyAllowlisted,
                    )
                } else {
                    FileReadPermission::Denied(FileReadPermissionDeniedReason::AlwaysAskEnabled)
                }
            }
        }
    }

    /// Returns whether or not Agent Mode can automatically write to files.
    pub fn can_write_files(
        &self,
        conversation_id: &AIConversationId,
        paths: &[PathBuf],
        terminal_view_id: Option<EntityId>,
        ctx: &AppContext,
    ) -> FileWritePermission {
        // Protected paths are always denied, regardless of autonomy settings.
        if let Some(denied) = check_protected_write_paths(paths) {
            return denied;
        }

        if BlocklistAIHistoryModel::as_ref(ctx)
            .conversation(conversation_id)
            .is_some_and(|convo| convo.autoexecute_any_action())
        {
            return FileWritePermission::Allowed(FileWritePermissionAllowedReason::RunToCompletion);
        }

        self.determine_write_permissions_from_active_profile(terminal_view_id, ctx)
    }

    /// Returns whether Agent Mode can automatically apply `file_edits`.
    ///
    /// This is the entry point for a batch of agent file edits; prefer it over
    /// [`Self::can_write_files`], which trusts its caller to have named every path. It feeds
    /// the protected-path guard every path the batch writes or removes — including a V4A
    /// rename's `move_to` destination — in both the spelling the model emitted and the
    /// absolute spelling the writer resolves it to. See [`file_edit_guard_paths`].
    pub fn can_apply_file_edits(
        &self,
        conversation_id: &AIConversationId,
        file_edits: &[FileEdit],
        shell: &Option<ShellLaunchData>,
        current_working_directory: &Option<String>,
        terminal_view_id: Option<EntityId>,
        ctx: &AppContext,
    ) -> FileWritePermission {
        let paths = file_edit_guard_paths(file_edits, shell, current_working_directory);
        self.can_write_files(conversation_id, &paths, terminal_view_id, ctx)
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn can_call_mcp_tool(
        &self,
        server_id: Option<&uuid::Uuid>,
        name: &str,
        conversation_id: &AIConversationId,
        terminal_view_id: Option<EntityId>,
        ctx: &AppContext,
    ) -> bool {
        let templatable_manager = TemplatableMCPServerManager::as_ref(ctx);

        // Try resolving via server UUID first, then fall back to tool-name lookup.
        // On recent clients, the server UUID should always be set - we should eventually
        // require the server UUID.
        let mut uuid_of_mcp_server =
            server_id.and_then(|id| templatable_manager.get_template_uuid(*id));

        // Prefer templatable MCP servers over legacy when a tool name exists in both.
        // Fall back to legacy behavior if templatable lookup fails or is disabled.
        if uuid_of_mcp_server.is_none() {
            uuid_of_mcp_server = templatable_manager
                .server_from_tool(name.to_string())
                .copied()
                .and_then(|installation_uuid| {
                    templatable_manager.get_template_uuid(installation_uuid)
                });
        }

        self.can_use_mcp_server(conversation_id, uuid_of_mcp_server, terminal_view_id, ctx)
    }

    /// Returns whether or not Agent Mode can automatically read the given MCP resource.
    #[cfg(not(target_family = "wasm"))]
    pub fn can_read_mcp_resource(
        &self,
        server_id: Option<&uuid::Uuid>,
        name: &str,
        uri: Option<&str>,
        conversation_id: &AIConversationId,
        terminal_view_id: Option<EntityId>,
        ctx: &AppContext,
    ) -> bool {
        let templatable_manager = TemplatableMCPServerManager::as_ref(ctx);

        // Try resolving via server UUID first, then fall back to resource name/URI lookup.
        // On recent clients, the server UUID should always be set - we should eventually
        // require the server UUID.
        let mut uuid_of_mcp_server =
            server_id.and_then(|id| templatable_manager.get_template_uuid(*id));

        // Prefer templatable MCP servers over legacy when a resource name exists in both.
        // Fall back to legacy behavior if templatable lookup fails or is disabled.
        if uuid_of_mcp_server.is_none() {
            uuid_of_mcp_server = templatable_manager
                .server_from_resource(name, uri)
                .copied()
                .and_then(|installation_uuid| {
                    templatable_manager.get_template_uuid(installation_uuid)
                });
        }

        self.can_use_mcp_server(conversation_id, uuid_of_mcp_server, terminal_view_id, ctx)
    }

    /// Checks whether the given MCP server (identified by its template UUID) is permitted
    /// to be used based on the current MCP permission setting and allowlist/denylist.
    #[cfg(not(target_family = "wasm"))]
    fn can_use_mcp_server(
        &self,
        conversation_id: &AIConversationId,
        uuid_of_mcp_server: Option<uuid::Uuid>,
        terminal_view_id: Option<EntityId>,
        ctx: &AppContext,
    ) -> bool {
        if BlocklistAIHistoryModel::as_ref(ctx)
            .conversation(conversation_id)
            .is_some_and(|convo| convo.autoexecute_any_action())
        {
            return true;
        }

        let allowlisted = uuid_of_mcp_server
            .is_some_and(|uid| self.get_mcp_allowlist(ctx, terminal_view_id).contains(&uid));
        let denylisted = uuid_of_mcp_server
            .is_some_and(|uid| self.get_mcp_denylist(ctx, terminal_view_id).contains(&uid));

        match self.get_mcp_permissions_setting(ctx, terminal_view_id) {
            ActionPermission::AgentDecides | ActionPermission::Unknown => {
                allowlisted && !denylisted
            }
            ActionPermission::AlwaysAllow => !denylisted,
            ActionPermission::AlwaysAsk => allowlisted && !denylisted,
        }
    }

    // Helper function to evaluate the active profile + workspace settings.
    fn determine_write_permissions_from_active_profile(
        &self,
        terminal_view_id: Option<EntityId>,
        ctx: &AppContext,
    ) -> FileWritePermission {
        match self.get_apply_code_diffs_setting(ctx, terminal_view_id) {
            ActionPermission::AgentDecides | ActionPermission::Unknown => {
                FileWritePermission::Denied(FileWritePermissionDeniedReason::AgentDecided)
            }
            ActionPermission::AlwaysAllow => FileWritePermission::Allowed(
                FileWritePermissionAllowedReason::AutowriteSettingEnabled,
            ),
            ActionPermission::AlwaysAsk => {
                FileWritePermission::Denied(FileWritePermissionDeniedReason::AlwaysAskEnabled)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    /// Returns whether or not Agent Mode can auto-execute the given command.
    pub fn can_autoexecute_command(
        &self,
        conversation_id: &AIConversationId,
        command: &str,
        escape_char: EscapeChar,
        is_read_only: bool,
        is_risky: Option<bool>,
        terminal_view_id: Option<EntityId>,
        ctx: &AppContext,
    ) -> CommandExecutionPermission {
        // Normalize line continuations based on shell type.
        // POSIX shells (bash/zsh/fish) use backslash, PowerShell uses backtick.
        //
        // A line continuation is *removed*, not turned into a separator: `r\<newline>m` runs
        // `rm`, exactly as `r\m` does. Substituting a space here instead produced `r m`, whose
        // first word is `r`, so every `rm` rule stopped matching and a bare newline became a
        // one-character denylist bypass — including for the `r\m` form the doc comment on
        // `denylist_match_candidates` claims as handled, because this rewrite runs first and
        // the parser never sees the continuation. Pinned by
        // `test_can_autoexecute_command_denylist_matches_line_continuations`.
        let normalized_command = match escape_char {
            EscapeChar::Backslash => command.replace("\\\n", ""),
            EscapeChar::Backtick => command.replace("`\n", ""),
        };

        // The command string might be composed of multiple commands so let's
        // break it up first.
        let (commands, contains_redirection) = decompose_command(&normalized_command, escape_char);
        // Match denylist predicates against every shell-equivalent spelling of each
        // subcommand, not just the one the model typed. See `denylist_match_candidates`.
        let mut denylist_candidates_per_command = commands
            .iter()
            .map(|command| denylist_match_candidates(command, escape_char))
            .collect::<Vec<_>>();

        // `decompose_command` is shared with command x-ray and the allowlist and is not a
        // shell parser: it can hide the command word behind a redirect (`>/dev/null rm`,
        // `rm>/dev/null`), a brace expansion (`{rm,-rf,~}`), a control-flow keyword
        // (`if …; then rm …; fi`) or a wrapper (`sudo`, `env`, `find -exec`, `sh -c`). So the
        // denylist is *also* matched against every command a shell-accurate analysis says the
        // line executes (#678). Additive, like everything else here: it can only deny more.
        let executed = executed_commands(&normalized_command, escape_char);
        denylist_candidates_per_command
            .push(with_flattened_line_breaks(executed.policy_spellings()));
        let command_words_resolved = executed.is_fully_resolved();

        // Local auto-approve may bypass the user-configured denylist, but workspace policy must
        // always be evaluated. Sandboxed processes use a separate organization-managed denylist
        // that cannot be bypassed.
        let auto_approve_enabled = BlocklistAIHistoryModel::as_ref(ctx)
            .conversation(conversation_id)
            .is_some_and(|convo| convo.autoexecute_any_action());
        let bypass_user_denylist = auto_approve_enabled
            && !AppExecutionMode::as_ref(ctx).is_sandboxed()
            && *AISettings::as_ref(ctx).auto_approve_bypasses_command_denylist;

        // The denylist takes precedence over the remaining conditions.
        let denylist = if bypass_user_denylist {
            // Auto-approve may bypass the user denylist, but the organization denylist
            // must always be enforced.
            Self::get_org_execute_commands_denylist(ctx)
        } else {
            // Without the bypass, enforce both the organization and user denylists.
            self.get_execute_commands_denylist(ctx, terminal_view_id)
        };
        if denylist_candidates_per_command.iter().any(|candidates| {
            candidates
                .iter()
                .any(|c| denylist.iter().any(|d| d.matches(c)))
        }) {
            return CommandExecutionPermission::Denied(
                CommandExecutionPermissionDeniedReason::ExplicitlyDenylisted,
            );
        }

        // Fail closed. If some executed command word could not be determined statically, "no
        // denylist rule matched" means nothing — the unknown word might be `rm`. Every path
        // below can auto-approve, so none of them may run on an unverified denylist.
        //
        // Gated on the denylist being non-empty because that is the only case in which the
        // parse changes the outcome: with no rules there is nothing an unknown word could
        // match, and the remaining decisions (auto-approve, AlwaysAllow, the model's own
        // read-only/risk verdicts) never consulted the parse. The allowlist *does* consult it,
        // and is gated separately below.
        if !command_words_resolved && !denylist.is_empty() {
            return CommandExecutionPermission::Denied(
                CommandExecutionPermissionDeniedReason::UnresolvedCommandWord,
            );
        }

        if auto_approve_enabled {
            return CommandExecutionPermission::Allowed(
                CommandExecutionPermissionAllowedReason::RunToCompletion,
            );
        }

        match self.get_execute_commands_setting(ctx, terminal_view_id) {
            ActionPermission::AgentDecides | ActionPermission::Unknown => {
                if FeatureFlag::AgentDecidesCommandExecution.is_enabled() && is_risky == Some(false)
                {
                    return CommandExecutionPermission::Allowed(
                        CommandExecutionPermissionAllowedReason::AgentDecided,
                    );
                }

                if contains_redirection {
                    return CommandExecutionPermission::Denied(
                        CommandExecutionPermissionDeniedReason::ContainsRedirection,
                    );
                }

                // An allowlist match vouches for the text `decompose_command` produced, which
                // is only trustworthy when every executed command word was resolved; otherwise
                // a match could approve a command the text does not show. Never *widens* the
                // allowlist: it can only withhold a match.
                let allowlist = self.get_execute_commands_allowlist(ctx, terminal_view_id);
                if command_words_resolved
                    && commands.iter().all(|command| {
                        allowlist
                            .iter()
                            .any(|allowlist_item| allowlist_item.matches(command))
                    })
                {
                    return CommandExecutionPermission::Allowed(
                        CommandExecutionPermissionAllowedReason::ExplicitlyAllowlisted,
                    );
                }

                // For now, the heuristic is if the command is read only or if we're executing
                // a plan. Otherwise, we don't want to autoexecute.
                if is_read_only {
                    CommandExecutionPermission::Allowed(
                        CommandExecutionPermissionAllowedReason::AgentDecided,
                    )
                } else {
                    CommandExecutionPermission::Denied(
                        CommandExecutionPermissionDeniedReason::AgentDecided,
                    )
                }
            }
            ActionPermission::AlwaysAllow => CommandExecutionPermission::Allowed(
                CommandExecutionPermissionAllowedReason::AlwaysAllowed,
            ),
            ActionPermission::AlwaysAsk => {
                let allowlist = self.get_execute_commands_allowlist(ctx, terminal_view_id);

                // Only trustworthy when every executed command word was resolved; see the
                // `AgentDecides` arm above.
                if command_words_resolved
                    && commands.iter().all(|command| {
                        allowlist
                            .iter()
                            .any(|allowlist_item| allowlist_item.matches(command))
                    })
                {
                    CommandExecutionPermission::Allowed(
                        CommandExecutionPermissionAllowedReason::ExplicitlyAllowlisted,
                    )
                } else {
                    CommandExecutionPermission::Denied(
                        CommandExecutionPermissionDeniedReason::AlwaysAskEnabled,
                    )
                }
            }
        }
    }

    // The four command allow/denylist mutators below write the **default execution profile**,
    // which is where `get_execute_commands_{allow,deny}list_for_profile` reads from a few
    // hundred lines above. They used to write `AISettings.agent_mode_command_execution_*`
    // instead — a store no permission decision consults.
    //
    // That was not a de-clouding regression: it is inherited verbatim from this fork's base
    // (`0dbd3d567`, the initial public release), which predates upstream's move of these
    // lists into execution profiles. The pin has already made the move
    // (`42effe840:app/src/ai/blocklist/permissions.rs:997-1050`), so this is a **port catch-up
    // to the pin, not a divergence** — the bodies below are the pin's.
    //
    // The bug was latent, which is the only reason it never shipped as a user-visible
    // failure: all four call sites hang off settings-page widgets that are constructed but
    // never rendered (`settings_view/ai_page.rs:787-848` build the two editors and
    // `:3222-3230` handle `RemoveFromCommandExecution{Allow,Deny}list`, which nothing
    // dispatches). The list editors the user actually sees write the profile directly
    // (`ai_page.rs:1430,1469,3438,3447`, `ai/execution_profiles/editor/mod.rs:1590-1608`).
    // Latent is not the same as harmless: had either editor been rendered, a user adding an
    // entry to the *denylist* would have been shown a rule that was never enforced.
    //
    // `AISettings.agent_mode_command_execution_*` is now read-only from this module's point
    // of view. It is still read once, by `create_default_from_legacy_settings`
    // (`ai/execution_profiles/mod.rs:473-479`), which seeds the default profile from the
    // legacy settings keys on first run — so those keys keep working as the TOML-level
    // migration source they are, and stop being a second, unenforced store.
    //
    // Covered by `test_command_autoexecution_mutators_reach_enforcement`.

    /// Allows Agent Mode to auto-execute commands that match `command`.
    ///
    /// The denylist (see [`Self::add_command_to_autoexecution_denylist`])
    /// takes precedence over the allowlist.
    pub fn add_command_to_autoexecution_allowlist(
        &mut self,
        command: AgentModeCommandExecutionPredicate,
        ctx: &mut ModelContext<Self>,
    ) -> Result<()> {
        AIExecutionProfilesModel::handle(ctx).update(ctx, |profiles, ctx| {
            let profile_id = profiles.default_profile_id();
            profiles.add_to_command_allowlist(profile_id, &command, ctx);
        });
        Ok(())
    }

    /// Removes `command` from the auto-execution allowlist.
    ///
    /// See [`Self::add_command_to_autoexecution_allowlist`] for more about the allowlist.
    pub fn remove_command_from_autoexecution_allowlist(
        &mut self,
        command: &AgentModeCommandExecutionPredicate,
        ctx: &mut ModelContext<Self>,
    ) -> Result<()> {
        AIExecutionProfilesModel::handle(ctx).update(ctx, |profiles, ctx| {
            let profile_id = profiles.default_profile_id();
            profiles.remove_from_command_allowlist(profile_id, command, ctx);
        });
        Ok(())
    }

    /// Forces Agent Mode to ask for user consent before executing commands that match `command`.
    pub fn add_command_to_autoexecution_denylist(
        &mut self,
        command: AgentModeCommandExecutionPredicate,
        ctx: &mut ModelContext<Self>,
    ) -> Result<()> {
        AIExecutionProfilesModel::handle(ctx).update(ctx, |profiles, ctx| {
            let profile_id = profiles.default_profile_id();
            profiles.add_to_command_denylist(profile_id, &command, ctx);
        });
        Ok(())
    }

    /// Removes `command` from the auto-execution denylist.
    ///
    /// See [`Self::add_command_to_autoexecution_denylist`] for more about the denylist.
    pub fn remove_command_from_denylist(
        &mut self,
        command: &AgentModeCommandExecutionPredicate,
        ctx: &mut ModelContext<Self>,
    ) -> Result<()> {
        AIExecutionProfilesModel::handle(ctx).update(ctx, |profiles, ctx| {
            let profile_id = profiles.default_profile_id();
            profiles.remove_from_command_denylist(profile_id, command, ctx);
        });
        Ok(())
    }

    /// Sets whether or not readonly commands can be auto-executed by Agent Mode.
    pub fn set_should_autoexecute_readonly_commands(
        &mut self,
        enabled: bool,
        ctx: &mut ModelContext<Self>,
    ) -> Result<()> {
        AISettings::handle(ctx).update(ctx, |settings, ctx| {
            settings
                .agent_mode_execute_read_only_commands
                .set_value(enabled, ctx)
                .map(|_| ())?;

            // If enabling, no need to show the file speedbump since
            // that setting will be superseded by this setting.
            if enabled {
                settings
                    .should_show_agent_mode_autoread_files_speedbump
                    .set_value(false, ctx)?;
            }

            settings
                .should_show_agent_mode_autoexecute_readonly_commands_speedbump
                .set_value(false, ctx)
        })
    }

    /// Sets whether or not we should always allow writing to the PTY.
    pub fn set_always_allow_write_to_pty(
        &mut self,
        enabled: bool,
        terminal_view_id: EntityId,
        ctx: &mut ModelContext<Self>,
    ) -> Result<()> {
        let permission = if enabled {
            WriteToPtyPermission::AlwaysAllow
        } else {
            WriteToPtyPermission::AlwaysAsk
        };
        let active_profile =
            AIExecutionProfilesModel::as_ref(ctx).active_profile(Some(terminal_view_id), ctx);
        AIExecutionProfilesModel::handle(ctx).update(ctx, |profiles_model, ctx| {
            profiles_model.set_write_to_pty(*active_profile.id(), &permission, ctx);
        });
        Ok(())
    }

    /// Sets whether or not we should always allow reading files.
    pub fn set_always_allow_read_files(
        &mut self,
        enabled: bool,
        terminal_view_id: EntityId,
        ctx: &mut ModelContext<Self>,
    ) -> Result<()> {
        let permissions = if enabled {
            ActionPermission::AlwaysAllow
        } else {
            ActionPermission::AlwaysAsk
        };

        let active_profile =
            AIExecutionProfilesModel::as_ref(ctx).active_profile(Some(terminal_view_id), ctx);
        AIExecutionProfilesModel::handle(ctx).update(ctx, |profiles_model, ctx| {
            profiles_model.set_read_files(*active_profile.id(), &permissions, ctx);
        });
        Ok(())
    }

    /// Sets permissions that Agent Mode has for coding tasks.
    pub fn set_coding_permissions(
        &mut self,
        permissions: AgentModeCodingPermissionsType,
        ctx: &mut ModelContext<Self>,
    ) -> Result<()> {
        AISettings::handle(ctx).update(ctx, |settings, ctx| {
            settings
                .agent_mode_coding_permissions
                .set_value(permissions, ctx)
                .map(|_| ())?;

            settings
                .should_show_agent_mode_autoread_files_speedbump
                .set_value(false, ctx)
        })
    }

    /// Adds a filepath that Agent Mode can read for coding tasks without additional permissions.
    /// Used in conjunction with [`AgentModeCodingPermissionsType::AllowReadingSpecificFiles`].
    ///
    /// This does not do any validation on the filepath; callers should ensure the filepath is valid.
    pub fn add_filepath_to_code_read_allowlist(
        &mut self,
        filepath: PathBuf,
        ctx: &mut ModelContext<Self>,
    ) -> Result<()> {
        let mut allowlist = AISettings::as_ref(ctx)
            .agent_mode_coding_file_read_allowlist
            .clone();
        allowlist.push(filepath);
        AISettings::handle(ctx).update(ctx, |settings, ctx| {
            settings
                .agent_mode_coding_file_read_allowlist
                .set_value(allowlist, ctx)
        })
    }

    /// Counterpart to [`Self::add_filepath_to_code_read_allowlist`].
    pub fn remove_filepath_from_code_read_allowlist(
        &mut self,
        filepath: PathBuf,
        ctx: &mut ModelContext<Self>,
    ) -> Result<()> {
        let mut allowlist = AISettings::as_ref(ctx)
            .agent_mode_coding_file_read_allowlist
            .clone();
        allowlist.retain(|p| p != &filepath);
        AISettings::handle(ctx).update(ctx, |settings, ctx| {
            settings
                .agent_mode_coding_file_read_allowlist
                .set_value(allowlist, ctx)
        })
    }

    /// Gives Agent Mode temporary access to the provided `files`.
    /// The permissions are scoped to the given conversation.
    pub fn add_temporary_file_read_permissions<P: Into<PathBuf>>(
        &mut self,
        conversation_id: AIConversationId,
        files: impl IntoIterator<Item = P>,
    ) {
        self.temporary_file_permissions
            .entry(conversation_id)
            .or_default()
            .extend(files.into_iter().map(Into::into));
    }

    /// Returns whether the agent can ask the user a question in the given conversation.
    pub fn can_ask_user_question(
        &self,
        conversation_id: &AIConversationId,
        terminal_view_id: Option<EntityId>,
        ctx: &AppContext,
    ) -> bool {
        // openWarp change: auto-approve (ctrl+shift+i) only auto-passes execution
        // tools like shell/edit; ask_user_question always requires surfacing to the
        // user, so the model asking a question doesn't get silently swallowed.
        // Only skipped when explicitly set to `Never`.
        let _ = conversation_id;
        match self.get_ask_user_question_setting(ctx, terminal_view_id) {
            AskUserQuestionPermission::Never => false,
            AskUserQuestionPermission::AskExceptInAutoApprove
            | AskUserQuestionPermission::Unknown
            | AskUserQuestionPermission::AlwaysAsk => true,
        }
    }
}

/// Every path a batch of agent file edits writes to or removes, in every spelling the
/// protected-path guard must see.
///
/// # Why the destination
///
/// A V4A edit with `move_to` writes the destination and removes the source
/// (`diff_application.rs`, `apply_v4a_update` -> `DiffType::Update { rename }` ->
/// `rename_and_save`). `FileEdit::file()` names only the source, and this list used to be
/// built from it alone, so renaming an innocuous file onto `~/.claude.json` or `.mcp.json`
/// was auto-approved under an auto-write setting while the guard never saw the path being
/// written. [`FileEdit::written_paths`] names both ends.
///
/// # Why two spellings of each path
///
/// The writer resolves every path — source and destination alike — through
/// [`host_native_absolute_path`]: tilde expansion, a join against the session cwd, lexical
/// normalisation. The guard is given that resolved spelling so it judges the file actually
/// written; e.g. `config.toml` from a cwd of `~/.codex` is `~/.codex/config.toml`, which no
/// check of the raw string can recognise. The raw spelling is kept as well, so resolution can
/// only ever add denials, never remove one the raw path already triggered (a remote or WSL
/// session's resolved spelling need not look like a local home path).
///
/// Both are lexical. Symlinks are not followed for the destination, exactly as they are not
/// for the source — see the residue notes in [`super::protected_paths`].
pub(crate) fn file_edit_guard_paths(
    file_edits: &[FileEdit],
    shell: &Option<ShellLaunchData>,
    current_working_directory: &Option<String>,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for path in file_edits.iter().flat_map(|edit| edit.written_paths()) {
        let resolved = host_native_absolute_path(path, shell, current_working_directory);
        if resolved != path {
            paths.push(PathBuf::from(resolved));
        }
        paths.push(PathBuf::from(path));
    }
    paths
}

/// Returns `Some(Denied(ProtectedPath))` if any of the given paths are protected and must
/// never be written without the user's explicit, per-action confirmation — regardless of
/// autonomy settings, run-to-completion, or the LRC tag-in override.
/// Returns `None` if no paths are protected.
///
/// The protected set (MCP configs, this app's settings and profile store, other agents'
/// configs and hooks, skills and prompt templates, SSH, shell startup files, git hooks and
/// config, `.envrc`, VS Code tasks) and how it is matched are defined in ONE place:
/// [`super::protected_paths`]. Read its module docs before changing either.
///
/// What this function is given matters as much as the list: callers must pass every path a
/// write touches, in the spelling the writer will use. For agent file edits that is
/// [`BlocklistAIPermissions::can_apply_file_edits`] / [`file_edit_guard_paths`], which pass a
/// V4A rename's destination as well as its source, both raw and resolved against the
/// session's shell and cwd (#682).
fn check_protected_write_paths(paths: &[PathBuf]) -> Option<FileWritePermission> {
    if paths.iter().any(|path| is_protected_write_path(path)) {
        Some(FileWritePermission::Denied(
            FileWritePermissionDeniedReason::ProtectedPath,
        ))
    } else {
        None
    }
}

/// Whether any of `file_edits` writes, removes or renames onto a protected path.
///
/// Used where the executor must refuse a stand-in confirmation (the LRC tag-in override):
/// a protected write needs the user's own click, not an inferred one.
pub(crate) fn file_edits_touch_protected_path(
    file_edits: &[FileEdit],
    shell: &Option<ShellLaunchData>,
    current_working_directory: &Option<String>,
) -> bool {
    check_protected_write_paths(&file_edit_guard_paths(
        file_edits,
        shell,
        current_working_directory,
    ))
    .is_some()
}

impl Entity for BlocklistAIPermissions {
    type Event = ();
}

impl SingletonEntity for BlocklistAIPermissions {}

/// Returns true iff Agent Mode autonomy features are allowed on this client.
/// Granular permissions still need to be checked for specific autonomy features
/// (e.g. whether a command is auto-executable).
pub fn is_agent_mode_autonomy_allowed(ctx: &AppContext) -> bool {
    crate::UserWorkspaces::as_ref(ctx).is_ai_autonomy_allowed()
}

/// Every spelling of `command` that the denylist must be matched against.
///
/// # Why this exists (deliberate divergence from the pin, `42effe840`)
///
/// A denylist entry is an *anchored* regex (`AgentModeCommandExecutionPredicate`) matched
/// against command text produced by `decompose_command`, which slices the raw source by span
/// and therefore returns the text **exactly as the model typed it, quotes included**. So an
/// entry of `rm .*` matches `rm -rf ~` but not `"rm" -rf ~`, `'rm' -rf ~`, `r"m" -rf ~` or
/// `\rm -rf ~` — spellings the shell executes identically. One quote character defeats every
/// user- and org-configured denylist rule.
///
/// This is pin-parity behaviour: upstream has the same hole. Fixing it here is therefore an
/// intentional divergence, **not** a port regression. *Do not revert this when moving the pin*
/// unless upstream has fixed it too; a re-pin that restores the pinned text verbatim silently
/// reopens the bypass.
///
/// # Why the normalisation lives here and not in `warp_completer`
///
/// `decompose_command`'s output is also what command x-ray shows the user, what error
/// underlining consumes, and what the *allow*list is matched against. Normalising at the
/// parser would change all three. Two of those changes are actively bad: widening an allowlist
/// match is the unsafe direction (it grants execution rather than withholding it), and a
/// denylist that normalises differently from the text shown to the user is its own hazard.
/// So the completer keeps returning text as typed for every existing consumer, gains one
/// additive API (`unquoted_command_parts`) because the unquoting logic belongs with the
/// lexer and not re-implemented inside the security layer, and only the deny decision here
/// looks at the extra spellings.
///
/// # Fail-closed by construction
///
/// The as-typed text is always the first candidate, and candidates are only ever *added*.
/// This function can therefore deny strictly more than before and never less — including when
/// the command name cannot be resolved at all (`$(echo rm) -rf ~`), where the as-typed text is
/// still matched rather than being silently dropped from the decision. This also repairs a
/// smaller fail-open in the previous version, which *replaced* the raw text with its
/// env-var-stripped form and so stopped matching rules written against the prefix.
///
/// # Handled
///
/// Every entry below has a test; the test is named on the entry. An entry without a named
/// test does not belong in this list. The first revision of this comment listed one thing it
/// did not do (`X=1`, unqualified — see `FOO=a=b` in the residue) and omitted five more from
/// the residue, two of which were one- or two-character bypasses, so the list read as a
/// coverage claim that the code did not honour.
///
/// - `"rm" -rf ~`, `'rm' -rf ~` — fully quoted command name.
///   (`test_can_autoexecute_command_denylist_matches_quoted_command_names`)
/// - `"r"m -rf ~`, `r"m" -rf ~`, `'r'"m" -rf ~` — adjacent concatenation of quoted segments.
///   (`test_can_autoexecute_command_denylist_matches_quoted_command_names`)
/// - `r\m -rf ~` — mid-word backslash escape of an *ordinary* character; the completer's
///   parser already drops the backslash.
///   (`test_can_autoexecute_command_denylist_matches_quoted_command_names`)
/// - `r\<newline>m -rf ~` and the PowerShell `` r`<newline>m `` — a line continuation *inside*
///   a word. `can_autoexecute_command` deletes the continuation before parsing, and the parser
///   drops it again for the unquoted view.
///   (`test_can_autoexecute_command_denylist_matches_line_continuations`)
/// - `\rm -rf ~` — leading escape char, which the parser deliberately *keeps* so `\ls` can
///   defeat an alias; stripped again here for matching only.
///   (`test_can_autoexecute_command_denylist_matches_quoted_command_names`)
/// - `$'rm' -rf ~`, `$"rm" -rf ~` — leading `$` before a quoted segment.
///   (`test_can_autoexecute_command_denylist_matches_quoted_command_names`)
/// - `X=1 rm file` and `FOO=a=b rm file` — leading env-var assignments, values containing
///   `=` included, and quoted ones (`X="1" rm file`). `"X"=1 rm file` is stripped too, and
///   that one is an accepted *over*-match rather than a fix: bash, dash and zsh all treat it
///   as a program literally named `X=1`, not as an assignment. Stripping it can only ever
///   deny more, so it is left alone.
///   (`test_can_autoexecute_command_denylist_matches_env_prefixed_commands`,
///   `test_can_autoexecute_command_denylist_matches_quoted_command_names`)
/// - quoting anywhere in the *arguments*, e.g. `rm "-rf" ~`, since all parts are unquoted.
///   (`test_can_autoexecute_command_denylist_matches_quoted_command_names`)
/// - a line break carried *inside* one command, by quoting or by escaping — `rm -rf ~ "\nx"`,
///   `rm -rf ~ 'x\ny'`. Rule regexes are anchored as `^{rule}$` and matched by the `regex`
///   crate, where `.` does **not** match `\n` and `$` is end-of-*haystack*, not end-of-line,
///   so one newline in a trailing argument used to defeat every rule ending in `.*` for every
///   command. A line-break-flattened spelling of each candidate is therefore added.
///   (`test_can_autoexecute_command_denylist_matches_embedded_newlines`)
/// - any combination of the above, and the same forms inside `$(...)`/backtick subshells,
///   because `decompose_command` already hands each subcommand here separately.
///   (`test_can_autoexecute_command_denylist_matches_quoted_command_names`)
///
/// # Handled by the shell-accurate analysis instead (#678)
///
/// These used to be listed below as residue because this function receives text from
/// `decompose_command` and cannot repair a parse that has already lost the command word.
/// They are now closed in `can_autoexecute_command` by also matching the denylist against
/// `warp_completer::parsers::simple::executed_commands`, a separate analysis that exists
/// precisely so that `decompose_command` — shared with command x-ray, error underlining and
/// the allowlist — keeps its tokenisation. Every entry is covered by
/// `test_can_autoexecute_command_denylist_sees_every_executed_command_word` and, in more
/// depth, by `command_words_test.rs`.
///
/// - Redirection glued to, or preceding, the command name: `rm>/dev/null -rf ~`,
///   `>/dev/null rm -rf ~`, `2>&1 rm`, `&>x rm`, `{fd}>x rm`.
/// - Brace expansion forming the command: `{rm,-rf,~}`, `{r,}m -rf ~`.
/// - Control-flow and grouping: `if`/`then`/`elif`/`else`, `while`/`until`/`for`/`select`
///   `… do …; done`, `case … in x) …;; esac`, `{ …; }`, `( … )`, `! cmd`, `time -p cmd`,
///   `[[ … ]] && cmd`, function bodies.
/// - Command prefixes with their own option grammar: `env` (including `env -S`), `command`,
///   `builtin`, `exec`, `nice`, `nohup`, `sudo`, `doas`, `timeout`, `stdbuf`, `setsid`,
///   `ionice`, `taskset`, `chrt`, `flock`, `xargs`, `watch`, `su -c`, `script -c`; and the
///   commands whose *arguments* are commands: `eval`, `sh -c`/`bash -c`/…, `find -exec`,
///   `alias x=…`, `trap '…' SIG`. An option outside a wrapper's table makes every suffix a
///   candidate rather than guessing.
/// - ANSI-C escape decoding inside `$'...'`: `$'\x72m'`, `$'\162m'`.
/// - bash array assignments as a prefix: `FOO=(a b) rm file.txt`.
/// - Equivalent names: `/bin/rm`, `./rm`, zsh's `=rm`, `rm.exe`, and upper- or mixed-case
///   `RM` (case-insensitive file systems on macOS and Windows), plus PowerShell's aliases
///   (`ri`, `del`, `erase`, `rd`, `rmdir` for `Remove-Item`, …) and `source` for `.`.
/// - Same-line aliases (`alias r=rm; r -rf ~`) and function bodies.
/// - Commands in git config and environment: `git -c core.pager=…`, `alias.x=!…`,
///   `credential.helper=!…`, `*.textconv`/`*.cmd`/…, `git config <key> <cmd>`,
///   `rebase --exec`, `bisect run`, `submodule foreach`, `filter-branch --*-filter`,
///   `difftool -x`; `GIT_EXTERNAL_DIFF`, `GIT_PAGER`/`PAGER`, `EDITOR`/`VISUAL`,
///   `GIT_SSH_COMMAND`, `PROMPT_COMMAND`, `PS1` substitutions and the like, whether as a
///   prefix, standalone, via `export` or via `env`.
/// - Other shells' spellings: comments are also read as commands (zsh without
///   `interactive_comments` runs them), `((…))` also as commands (dash, fish), zsh `;|`,
///   `- cmd` and `repeat N cmd`, fish `and`/`or`/`not`.
///
/// # Fail-closed where the analysis cannot decide
///
/// These make `executed_commands` report the line unresolved, and `can_autoexecute_command`
/// then returns `Denied(UnresolvedCommandWord)` whenever a denylist applies, and withholds
/// allowlist approval regardless. "No rule matched" is never read from an analysis that could
/// not see the command. (`test_can_autoexecute_command_fails_closed_on_unresolved_command_words`,
/// `test_can_autoexecute_command_denylist_follows_indirect_execution`)
///
/// - A command word only known at run time: `$R`, `${R:-rm}`, `${!R}`, `$(echo rm)`,
///   `` `which rm` ``, globs (`/bin/r?`), history expansion (`!rm`, `^a^b`, `fc`, zsh `r`),
///   `hash -p`, and non-ASCII names (zero-width, bidi and look-alike characters).
/// - Code handed to an interpreter inline or on stdin: `python -c`, `perl -e`, `ruby -e`,
///   `node -e`/`-p`, `deno eval`, `php -r`, `lua -e`, `osascript -e`, `gdb -ex`, editor `-c`
///   commands, `awk` programs using `system()` or pipes, GNU `sed`'s `e`, and a shell or
///   interpreter with no script operand (`bash`, `sh -s`, `… | python3`, `python3 - <<EOF`).
/// - Commands built from input: `xargs` whose input would become the command (`xargs env`,
///   `xargs sh -c`, `-I` placeholders in the command word), `find -exec {}`, GNU `parallel`.
/// - Code loaded from elsewhere: `LD_PRELOAD`, `DYLD_INSERT_LIBRARIES`, `BASH_ENV`, `ENV`,
///   `NODE_OPTIONS`, `PERL5OPT`, `RUBYOPT`, `GIT_CONFIG_*`, `GIT_EXEC_PATH`, and git's
///   `include.path`, `includeIf.*`, `core.hooksPath`, `--config-env`, `--exec-path=`.
/// - `eval "$X"`, `sh -c "$X"`, dynamic aliases, trap actions and git config values.
/// - PowerShell outside a simple subset: script blocks and hashtables (`{ … }`), .NET type
///   access and static calls (`[…]`, `::`), method calls (`$f.Delete()`), `@( … )`,
///   here-strings, block comments, `& $x`/`& ( … )`/`. $x` invocation of a computed command,
///   `Set-Alias`/`New-Alias`, `Add-Type`, `New-Object`, `Invoke-Command`, `Start-Job`,
///   `-EncodedCommand`, `--%`.
/// - Input that does not parse, and input past the caps (64 KiB, 4096 commands, nesting,
///   brace-expansion size).
///
/// # Not handled (explicit residue, not an oversight)
///
/// - Code in *files*: `bash script.sh`, `python app.py`, `make`, `npm run`, `cargo run`,
///   git hooks already in the repository, an `awk -f`/`sed -f` script. Running a file is
///   indistinguishable, textually, from running any other program.
/// - Aliases and functions defined *before* this command line, in the user's shell, and
///   git aliases already in the user's config (`git x`).
/// - Remote and container execution (`ssh host rm …`, `docker run … rm`, `kubectl exec`):
///   the remote command is not a local command word. `ssh` is on the default denylist.
/// - A rule written against a full path (`/bin/rm .*`) is not matched by `rm`, and a copy or
///   link of a program under another name is invisible; `PATH` manipulation likewise.
/// - Programs whose own configuration runs commands that are not in the command line
///   (`less`'s `!`, an editor's config, a tool reading a config file).
/// - Environment variables outside the lists in `command_words.rs` that some program
///   happens to execute.
///
/// Treat the denylist as defence in depth, not as a boundary: it matches text, and a program
/// is free to do anything once it runs.
fn denylist_match_candidates(command: &str, escape_char: EscapeChar) -> Vec<String> {
    fn push(candidates: &mut Vec<String>, candidate: String) {
        if !candidate.is_empty() && !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    }

    // As typed, always first: whatever is added below, the denylist must never match *less*
    // than it did before this function existed.
    let mut candidates = vec![command.to_string()];

    // Leading env-var assignments stripped, still as typed: `X=1 rm file.txt` -> `rm file.txt`.
    if let Some(stripped) = command_without_leading_env_vars(command, escape_char) {
        push(&mut candidates, stripped);
    }

    // The shell's own view: quoting and escaping removed from every part.
    if let Some(parts) = unquoted_command_parts(command, escape_char) {
        if !parts.is_empty() {
            push(&mut candidates, parts.join(" "));

            // Two prefixes survive unquoting and still name the same program: a leading escape
            // char (`\rm`, which the parser keeps so alias-escaping round-trips) and a leading
            // `$` left behind by `$'rm'` / `$"rm"`.
            let escape = match escape_char {
                EscapeChar::Backslash => '\\',
                EscapeChar::Backtick => '`',
            };
            let unprefixed = parts[0]
                .trim_start_matches(|c| c == escape || c == '$')
                .to_string();
            if unprefixed != parts[0] {
                let mut unprefixed_parts = parts;
                unprefixed_parts[0] = unprefixed;
                push(&mut candidates, unprefixed_parts.join(" "));
            }
        }
    }

    // A line break that survives into a single command's text defeats every rule ending in
    // `.*`. Rules are compiled as `^{rule}$` by `AgentModeCommandExecutionPredicate`, and in
    // the `regex` crate `.` does not match `\n` while `$` anchors to the end of the *haystack*,
    // not the end of a line. So `rm -rf ~ "<newline>x"` — one harmless extra argument — was
    // matched by no `rm .*` rule, for every command, not just `rm`. Flattening line breaks to
    // spaces gives those rules something to match. Additive like everything else here, so it
    // can only deny more.
    with_flattened_line_breaks(candidates)
}

/// `candidates`, plus a line-break-flattened spelling of each one that carries a `\n`.
///
/// Rules are compiled as `^{rule}$` by `AgentModeCommandExecutionPredicate`, and in the
/// `regex` crate `.` does not match `\n` while `$` anchors to the end of the *haystack*, so a
/// newline in one argument defeats every rule ending in `.*`. Additive: it can only deny more.
fn with_flattened_line_breaks(mut candidates: Vec<String>) -> Vec<String> {
    let flattened = candidates
        .iter()
        .filter(|candidate| candidate.contains('\n'))
        .map(|candidate| candidate.replace('\n', " "))
        .collect::<Vec<_>>();
    for candidate in flattened {
        if !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    }
    candidates
}

#[cfg(test)]
#[path = "permissions_test.rs"]
mod tests;
