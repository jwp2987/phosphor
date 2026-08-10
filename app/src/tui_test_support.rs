//! Test-only app initialization used by the external `warp_tui` crate.
//!
//! Zap-adapted port of upstream warp's `tui_test_support`: the cloud/orchestration
//! singletons (telemetry, `ServerApiProvider`, `AuthManager` cloud state,
//! `CloudModel`, harness/self-hosted-worker/orchestration models, codebase index)
//! are dropped because Zap is BYOP and has no cloud agent. The BYOP-relevant
//! singletons the TUI session view subtree constructs are kept, using Zap's
//! constructors. See specs/warp-oss-sync/SCOPE.md.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::Arc;

use ai::api_keys::ApiKeyManager;
use chrono::{Duration, Local};
use warp_core::SessionId;
use warp_core::execution_mode::{AppExecutionMode, ExecutionMode};
use warpui::{AppContext, ModelContext, ModelHandle, SingletonEntity as _};

use crate::LaunchMode;
use crate::ai::agent::conversation::AIConversationId;
use crate::ai::agent::{AIAgentAction, AIAgentExchangeId};
use crate::ai::agent_conversations_model::AgentConversationsModel;
use crate::ai::blocklist::history_model::AIQueryHistoryOutputStatus;
use crate::ai::blocklist::persistence::{PersistedAIInput, PersistedAIInputType};
use crate::ai::blocklist::{
    BlocklistAIActionModel, BlocklistAIHistoryModel, BlocklistAIPermissions,
};
use crate::ai::document::ai_document_model::AIDocumentModel;
use crate::ai::execution_profiles::profiles::AIExecutionProfilesModel;
use crate::ai::llms::{LLMId, LLMPreferences};
use crate::ai::mcp::TemplatableMCPServerManager;
use crate::ai::skills::SkillManager;
use crate::auth::{AuthManager, AuthStateProvider};
use crate::network::NetworkStatus;
use crate::settings::manager::SettingsManager;
use crate::settings::{AISettings, PrivacySettings, init_and_register_user_preferences};
use crate::terminal::cli_agent_sessions::CLIAgentSessionsModel;
use crate::terminal::model::session::active_session::ActiveSession;
use crate::terminal::model::session::command_executor::NoOpCommandExecutor;
use crate::terminal::model::session::{
    BootstrapSessionType, HostInfo, IsLegacySSHSession, Session, SessionInfo, Sessions,
};
use crate::terminal::model_events::ModelEventDispatcher;
use crate::terminal::shell::{Shell, ShellType};
use crate::terminal::{History, HistoryEntry, HistoryEvent};
use crate::user_config::WarpConfig;
use crate::workspaces::user_workspaces::UserWorkspaces;

/// Builds a history model with persisted AI queries for TUI tests.
pub fn blocklist_ai_history_model_with_queries(queries: Vec<String>) -> BlocklistAIHistoryModel {
    let start_time = Local::now();
    let persisted_queries = queries
        .into_iter()
        .enumerate()
        .map(|(index, text)| PersistedAIInput {
            exchange_id: AIAgentExchangeId::new(),
            conversation_id: AIConversationId::new(),
            start_ts: start_time + Duration::milliseconds(index as i64),
            inputs: vec![PersistedAIInputType::Query {
                text,
                context: Default::default(),
                referenced_attachments: Default::default(),
            }],
            output_status: AIQueryHistoryOutputStatus::Completed,
            working_directory: None,
            model_id: LLMId::from("test-model"),
            coding_model_id: LLMId::from("test-model"),
        })
        .collect();

    // Zap's `new` takes `(persisted_queries, multi_agent_conversations)` (no cloud
    // sync-state arg that upstream passes as `Vec::new()`).
    BlocklistAIHistoryModel::new(persisted_queries, vec![], &[])
}

/// Queues an action as the active confirmation request for a TUI view test.
///
/// Zap has no `queue_confirmation_action`; the equivalent is `queue_actions`,
/// which runs the preprocess→dispatch pipeline and leaves confirmation-requiring
/// actions in a pending-confirmation state. Exposed for tests via
/// [`BlocklistAIActionModel::queue_action_for_test`].
pub fn queue_tui_permission_action(
    action_model: &mut BlocklistAIActionModel,
    action: AIAgentAction,
    conversation_id: AIConversationId,
    ctx: &mut ModelContext<BlocklistAIActionModel>,
) {
    action_model.queue_confirmation_action(action, conversation_id, ctx);
}

/// Registers the singletons the action-execution pipeline reads, for tests that
/// drive actions without standing up a full session view.
///
/// `should_autoexecute` reaches `BlocklistAIPermissions`, which in turn reads the
/// execution-profile settings backed by the user preferences. Deliberately NOT
/// folded into `add_test_action_model`: `register_tui_session_view_test_singletons`
/// registers both of these unguarded, and `add_singleton_model` panics on a
/// duplicate -- doing it there breaks every test that uses the full harness.
pub fn register_tui_action_execution_test_singletons(app: &mut warpui::App) {
    app.update(init_and_register_user_preferences);
    app.add_singleton_model(BlocklistAIPermissions::new);
}

/// Registers the app models required to construct full TUI session views in tests.
///
/// Registration order mirrors model subscription dependencies. Cloud/orchestration
/// singletons present upstream are dropped (Zap is BYOP).
pub fn register_tui_session_view_test_singletons(app: &mut warpui::App) {
    app.add_singleton_model(|ctx| AppExecutionMode::new(ExecutionMode::App, false, ctx));
    app.update(init_and_register_user_preferences);
    app.add_singleton_model(|_| SettingsManager::default());
    app.add_singleton_model(WarpConfig::mock);
    app.update(|ctx| {
        warpui_extras::secure_storage::register_noop("test", ctx);
    });
    // Registers all `define_settings_group!` settings (TuiAutoupdate/Input/Code/Font/…,
    // and AISettings itself), read across the session-view subtree and by
    // `TuiAutoupdater::register`.
    app.update(crate::settings::register_all_settings);
    app.add_singleton_model(ApiKeyManager::new);

    app.add_singleton_model(|_| NetworkStatus::new());
    app.add_singleton_model(|_| AuthStateProvider::new_for_test());
    app.add_singleton_model(AuthManager::new_for_test);
    app.add_singleton_model(PrivacySettings::mock);
    app.add_singleton_model(|ctx| UserWorkspaces::mock(vec![], ctx));
    // Guarded: many isolated view tests register a mock Appearance themselves
    // before provisioning the full harness, so tolerate a pre-registered one.
    if !app.read(|ctx| ctx.has_singleton_model::<crate::appearance::Appearance>()) {
        app.add_singleton_model(|_| crate::appearance::Appearance::mock());
    }

    app.add_singleton_model(|_| TemplatableMCPServerManager::default());
    app.add_singleton_model(crate::ai::agent_providers::AgentProviderSecrets::new);
    app.add_singleton_model(LLMPreferences::new);
    app.add_singleton_model(BlocklistAIPermissions::new);
    // Registered before AIExecutionProfilesModel, whose `new` reads this singleton.
    app.add_singleton_model(crate::cloud_object::model::persistence::ObjectStoreModel::mock);
    app.add_singleton_model(|ctx| {
        AIExecutionProfilesModel::new(&LaunchMode::new_for_unit_test(), ctx)
    });
    app.add_singleton_model(|_| AIDocumentModel::new_for_test());

    app.add_singleton_model(|_| BlocklistAIHistoryModel::default());
    // Shell-command history. The TUI session view reads this since the up-arrow
    // menu was merged to cover prompts AND commands; before that it only ever
    // touched BlocklistAIHistoryModel, so the fixture did not provision it.
    //
    // Guarded, like Appearance above: `provision_session` below also registers
    // History on demand, and a second unconditional registration panics with
    // "add_singleton_model() was called twice" -- the failure mode that took
    // out 355 tests when the settings-registry work started registering groups
    // the test helpers were already registering by hand.
    if !app.read(|ctx| ctx.has_singleton_model::<History>()) {
        app.add_singleton_model(|_| History::default());
    }
    // Accepting a command from the history menu writes the resulting block
    // through the persistence writer, so the session view reads this singleton.
    // `PersistenceWriter::new(None)` is the no-op form -- no writer thread and
    // no channel -- which is what production also constructs when persistence
    // is unavailable, so tests exercise the real code path without touching a
    // database.
    if !app.read(|ctx| ctx.has_singleton_model::<crate::persistence::PersistenceWriter>()) {
        app.add_singleton_model(|_| crate::persistence::PersistenceWriter::new(None));
    }
    app.add_singleton_model(crate::ai::blocklist::QueuedQueryModel::new);
    app.add_singleton_model(|_| CLIAgentSessionsModel::new());
    app.add_singleton_model(AgentConversationsModel::new);
    let global_resources = crate::GlobalResourceHandles::mock(app);
    app.add_singleton_model(move |_| {
        crate::GlobalResourceHandlesProvider::new(global_resources.clone())
    });

    app.add_singleton_model(|_| {
        crate::changelog_model::ChangelogModel::new(std::sync::Arc::new(http_client::Client::new()))
    });
    app.add_singleton_model(crate::tui::TuiMcpManager::new_for_test);
    app.add_singleton_model(|_| ::ai::project_context::model::ProjectContextModel::default());

    app.add_singleton_model(|_| repo_metadata::repositories::DetectedRepositories::default());
    app.add_singleton_model(watcher::HomeDirectoryWatcher::new_for_test);
    app.add_singleton_model(repo_metadata::watcher::DirectoryWatcher::new);
    #[cfg(feature = "local_fs")]
    app.add_singleton_model(repo_metadata::RepoMetadataModel::new);
    app.add_singleton_model(
        crate::warp_managed_paths_watcher::WarpManagedPathsWatcher::new_for_testing,
    );
    app.add_singleton_model(crate::workflows::local_workflows::LocalWorkflows::new);
    app.add_singleton_model(SkillManager::new);
}

/// Registers seeded command history and an active session for focused TUI
/// history tests (issue #387).
///
/// Ported from the pinned Warp oracle (`02b53fcd8`) `app/src/tui_test_support.rs`.
/// Adapted: the pin drives "active" through a replayed `Precmd` model event;
/// this fork's `ModelEventDispatcher` exposes `set_active_session_id` directly
/// ("for use in unit tests where there's no `Precmd` event"), so this skips the
/// event-replay plumbing.
pub fn add_tui_history_test_models(
    commands: Vec<String>,
    ctx: &mut AppContext,
) -> (
    ModelHandle<ActiveSession>,
    SessionId,
    impl Future<Output = ()> + use<>,
) {
    let session_id = SessionId::from(1);
    let session_info = SessionInfo {
        session_id,
        shell: Shell::new(ShellType::Zsh, None, None, HashSet::new(), None),
        launch_data: None,
        histfile: None,
        user: "test-user".to_owned(),
        hostname: "test-host".to_owned(),
        subshell_info: None,
        path: None,
        environment_variable_names: HashSet::new(),
        aliases: HashMap::new(),
        abbreviations: HashMap::new(),
        function_names: HashSet::new(),
        builtins: HashSet::new(),
        keywords: Vec::new(),
        is_legacy_ssh_session: IsLegacySSHSession::No,
        home_dir: None,
        editor: None,
        session_type: BootstrapSessionType::Local,
        host_info: HostInfo::default(),
        tmux_control_mode: false,
        wsl_name: None,
        spawning_session_id: None,
    };
    let session = Arc::new(Session::new(
        session_info,
        Arc::new(NoOpCommandExecutor::default()),
    ));

    let history = if ctx.has_singleton_model::<History>() {
        History::handle(ctx)
    } else {
        ctx.add_singleton_model(|_| History::default())
    };
    let (history_initialized_tx, history_initialized_rx) = async_channel::bounded(1);
    ctx.subscribe_to_model(&history, move |_, event, _| match event {
        HistoryEvent::Initialized(id) if *id == session_id => {
            let _ = history_initialized_tx.try_send(());
        }
        HistoryEvent::Initialized(_) => {}
    });
    history.update(ctx, |history, ctx| {
        history.init_session_with(session, async move { commands }, ctx);
    });

    let sessions = ctx.add_model(|_| Sessions::new_for_test());
    let (_events_tx, events_rx) = async_channel::unbounded();
    let model_events =
        ctx.add_model(|ctx| ModelEventDispatcher::new(events_rx, sessions.clone(), ctx));
    model_events.update(ctx, |dispatcher, _| {
        dispatcher.set_active_session_id(session_id);
    });
    let active_session = ctx.add_model(|ctx| ActiveSession::new(sessions, model_events, ctx));

    let initialized = async move {
        history_initialized_rx
            .recv()
            .await
            .expect("history initialization should complete");
    };
    (active_session, session_id, initialized)
}

/// Appends a command to the history used by TUI tests.
pub fn append_tui_history_test_command(
    session_id: SessionId,
    command: String,
    ctx: &mut AppContext,
) {
    History::handle(ctx).update(ctx, |history, _| {
        let mut entry = HistoryEntry::command_only(command);
        entry.session_id = Some(session_id);
        history.append_commands(session_id, vec![entry]);
    });
}

/// Registers the minimal settings dependencies [`crate::terminal::history::up_arrow`]'s
/// shared combiner reads (`AISettings`, feature-flag-gated agent mode) for
/// focused TUI history-menu tests that don't need the full
/// [`register_tui_session_view_test_singletons`] fixture.
pub fn register_tui_input_mode_test_settings(ctx: &mut AppContext) {
    if ctx.has_singleton_model::<AISettings>() {
        return;
    }
    init_and_register_user_preferences(ctx);
    ctx.add_singleton_model(|_| SettingsManager::default());
    ctx.add_singleton_model(WarpConfig::mock);
    warpui_extras::secure_storage::register_noop("test", ctx);
    ctx.add_singleton_model(|_| AuthStateProvider::new_for_test());
    // Registers every `define_settings_group!` group, including `AISettings`.
    crate::settings::register_all_settings(ctx);
}
