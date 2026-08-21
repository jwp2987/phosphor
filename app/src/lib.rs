// Suppress warnings about rustdoc style.
#![allow(clippy::doc_lazy_continuation)]
// Orphaned code left over from upstream Zap trimming is temporarily kept; dead_code warnings are
// suppressed globally. Absent from the pin (`42effe840:app/src/lib.rs` has no such attribute).
//
// Before removing this: it is not a three-module problem. 84 of the 108 module declarations below
// are private, so lifting the blanket exposes `dead_code` across the whole private half of the
// crate, not just `ui_components` / `uri` / `workspaces`. `uri::open_window_with_action`
// (`uri/mod.rs`) is one confirmed orphan; the total is only knowable from a build, so size it with
// one before swapping the blanket for per-site `#[allow(dead_code)]`.
#![allow(dead_code)]

mod ai;
mod alloc;
mod antivirus;
#[cfg(target_os = "macos")]
mod app_menus;
mod app_services;
mod app_state;
mod auth;
mod autoupdate;
mod banner;
mod changelog_model;
mod chip_configurator;
mod cloud_object;
mod code;
mod code_review;
mod coding_entrypoints;
mod coding_panel_enablement_state;
mod command_palette;
mod completer;
#[allow(dead_code)]
mod context_chips;
#[cfg(enable_crash_recovery)]
mod crash_recovery;
#[cfg(feature = "crash_reporting")]
mod crash_reporting;
mod debounce;
mod debug_dump;
mod default_terminal;
mod drive;
#[cfg(windows)]
mod dynamic_libraries;
mod env_vars;
mod experiments;
mod external_secrets;
#[cfg(target_family = "wasm")]
mod font_fallback;
mod global_resource_handles;
mod gpu_state;
pub mod i18n;
mod input_classifier;
mod interval_timer;
mod linear;
#[cfg(feature = "local_fs")]
mod local_control;
mod local_managed_secrets;
mod login_item;
mod menu;
mod modal;
mod network;
mod notebooks;
mod notification;
mod notifications;
mod palette;
mod persistence;
mod platform;
#[cfg(feature = "plugin_host")]
mod plugin;
mod prefix;
#[cfg(target_os = "macos")]
mod preview_config_migration;
mod pricing;
mod profiling;
mod projects;
mod prompt;
mod quit_warning;
#[allow(dead_code)]
mod remote_server;
mod resource_limits;
mod safe_triangle;
mod search_bar;
mod server;
mod server_time;
mod session_management;
mod shell_indicator;
mod skill_manager;
mod suggestions;
mod system;
mod tab;
#[cfg(test)]
mod test_util;
mod throttle;
mod tips;
mod tracing;
mod ui_components;
mod undo_close;
mod uri;
mod user_config;
pub mod util;
mod view_components;
mod vim_registers;
mod voice;
mod voltron;
mod warp_managed_paths_watcher;
#[cfg(target_family = "wasm")]
mod wasm_nux_dialog;
mod window_settings;
mod word_block_editor;
mod workspaces;

// PLEASE DO NOT ADD MORE PUBLIC MODULES!
//
// Any modules which we make public outside of the `warp` crate lose dead code
// checking support, as the compiler cannot make any assumptions about whether
// or not the function/type is used by another crate that pulls in this one as
// a dependency.
//
// If you feel the need to export a module so that a type or function within it
// can be used by an integration test, you should define a new assertion function
// in the warp::integration_testing::assertions module (or a sub-module).  These
// functions will allow us to keep types internal to this crate and expose a
// simpler API for integration tests to consume.
pub mod ai_assistant;
pub mod appearance;
pub mod channel;
pub mod editor;
pub mod features;
pub mod input_suggestions;
#[cfg(feature = "integration_tests")]
pub mod integration_testing;
pub mod keyboard;
pub mod launch_configs;
pub mod pane_group;
pub mod resource_center;
pub mod root_view;
pub mod search;
pub mod settings;
pub mod settings_view;
pub mod tab_configs;
pub mod terminal;
// Facade re-exporting app-crate + workspace types to the warp_tui front-end.
// Feature-gated; Zap-adapted subset (cloud re-exports removed). See
// specs/warp-oss-sync/SCOPE.md.
#[cfg(feature = "tui")]
pub mod tui_export;
// Test-only app initialization used by the external `warp_tui` crate's test suite.
// Gated on `test-util` so it only compiles into dev/test builds (never the GUI or
// the shipping TUI binary). See specs/warp-oss-sync/SCOPE.md.
#[cfg(all(feature = "tui", any(test, feature = "test-util")))]
pub mod tui_test_support;
// App-side support for the headless warp_tui front-end (BYOP login model, etc.).
#[cfg(feature = "tui")]
pub mod tui;
#[cfg(feature = "tui")]
pub use crate::tui::{TuiLoginEvent, TuiLoginModel, TuiLoginPhase, log_out_tui};
pub mod themes;
#[cfg(not(target_family = "wasm"))]
use crate::ai::aws_credentials::AwsCredentialRefresher as _;
#[cfg(not(target_family = "wasm"))]
use crate::ai::tui_api_keys::TuiApiKeyRefresher as _;
use crate::ai::mcp::FileBasedMCPManager;
use crate::ai::mcp::FileMCPWatcher;
use crate::notebooks::link::is_openable_url_scheme;
use crate::uri::web_intent_parser::maybe_rewrite_web_url_to_intent;

use ::ai::project_context::model::ProjectContextModel;
pub use ai::agent::{todos::AIAgentTodoList, AIAgentActionResultType, FileEdit, TodoOperation};
use ai::agent::conversation::AIConversationId;
use ai::agent_conversations_model::AgentConversationsModel;
use ai::blocklist::agent_view::orchestration_pill_bar_model::OrchestrationPillBarModel;
use ai::blocklist::{BlocklistAIHistoryModel, BlocklistAIPermissions};
use ai::execution_profiles::editor::ExecutionProfileEditorManager;
use ai::execution_profiles::profiles::AIExecutionProfilesModel;
use ai::persisted_workspace::PersistedWorkspace;
// The codebase embedding index (Delta D2c). `::ai` is the crate; `ai` here is
// this crate's own `app/src/ai` module, hence the leading `::`.
#[cfg(feature = "local_fs")]
use ::ai::index::full_source_code_embedding::manager::{
    CodebaseIndexManager, CodebaseIndexManagerConfig,
};
use auth::AuthStateProvider;
use auth::{AuthManager, AuthState};
use code::editor_management::CodeManager;
use code::opened_files::OpenedFilesModel;
use code_review::GlobalCodeReviewModel;
use quit_warning::UnsavedStateSummary;
// Zap (localization, Phase 4): `ServerVoiceTranscriber` used to be injected as the default VoiceTranscriber; now it goes through `VoiceTranscriber::disabled()`. The same-named import is temporarily withheld.
#[cfg(feature = "local_fs")]
use settings::import::model::ImportedConfigModel;
use voice::transcriber::VoiceTranscriber;
use warp_cli::GlobalOptions;
use warp_cli::{agent::AgentCommand, CliCommand};

#[cfg(feature = "local_fs")]
use repo_metadata::{
    repositories::DetectedRepositories, watcher::DirectoryWatcher, RepoMetadataModel,
};
#[cfg(feature = "local_fs")]
use watcher::HomeDirectoryWatcher;

use settings_view::pane_manager::SettingsPaneManager;
use terminal::general_settings::GeneralSettings;
use terminal::keys_settings::KeysSettings;
#[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
use terminal::local_shell::LocalShellState;
pub use util::bindings::cmd_or_ctrl_shift;
pub mod workflows;
pub mod workspace;

#[cfg(feature = "integration_tests")]
pub use persistence::testing as sqlite_testing;

use ::settings::{Setting, ToggleableSetting};
pub use warp_core::errors::{report_error, report_if_error};

#[cfg(feature = "plugin_host")]
pub use plugin::{run_plugin_host, PLUGIN_HOST_FLAG};
use warp_core::user_preferences::GetUserPreferences as _;
use warpui::modals::{AlertDialogWithCallbacks, AppModalCallback};
use warpui::platform::app::ApproveTerminateResult;
use window_settings::WindowSettings;
use workflows::manager::WorkflowManager;

use crate::ai::ambient_agents::github_auth_notifier::GitHubAuthNotifier;
use crate::ai::document::ai_document_model::AIDocumentModel;
use crate::ai::facts::manager::AIFactManager;
use crate::ai::llms::LLMPreferences;
use crate::ai::mcp::MCPGalleryManager;
use crate::ai::mcp::TemplatableMCPServerManager;
use crate::ai::outline::RepoOutlines;
use crate::ai::restored_conversations::RestoredAgentConversations;
use crate::ai::skills::SkillManager;
use crate::ai::AIRequestUsageModel;
use crate::autoupdate::{AutoupdateState, RelaunchModel};
use crate::changelog_model::ChangelogModel;
use crate::cloud_object::model::actions::ObjectActions;
use crate::cloud_object::model::view::ObjectStoreViewModel;
use crate::cloud_object::update_manager::UpdateManager;
use crate::code::global_buffer_model::GlobalBufferModel;
#[cfg(feature = "local_fs")]
use crate::code::language_server_shutdown_manager::LanguageServerShutdownManager;
use crate::context_chips::prompt::Prompt;
use crate::default_terminal::DefaultTerminal;
use crate::drive::export::ExportManager;
use crate::env_vars::manager::EnvVarCollectionManager;
use crate::gpu_state::GPUState;
use crate::network::NetworkStatus;
use crate::notebooks::editor::keys::NotebookKeybindings;
use crate::notebooks::manager::NotebookManager;
use crate::notebooks::NotebookObject;
use crate::palette::PaletteMode;
use crate::persistence::PersistenceWriter;
use crate::projects::ProjectManagementModel;
use crate::server::experiments::ServerExperiments;
use crate::session_management::{RunningSessionSummary, SessionNavigationData};
use crate::settings::manager::SettingsManager;
use crate::settings::{AccessibilitySettings, ScrollSettings, SelectionSettings};
use crate::settings_view::keybindings::KeybindingChangedNotifier;
use crate::settings_view::DisplayCount;
use crate::suggestions::ignored_suggestions_model::IgnoredSuggestionsModel;
use crate::system::SystemStats;
use crate::terminal::cli_agent_sessions::CLIAgentSessionsModel;
use crate::terminal::keys::TerminalKeybindings;
use crate::terminal::resizable_data::ResizableData;
use crate::terminal::view::inline_banner::ByoLlmAuthBannerSessionState;
use crate::terminal::{AudibleBell, History};
use crate::undo_close::UndoCloseStack;
use crate::user_config::WarpConfig;
use crate::vim_registers::VimRegisters;
use crate::warp_managed_paths_watcher::{ensure_warp_watch_roots_exist, WarpManagedPathsWatcher};
use crate::workflows::aliases::WorkflowAliases;
use crate::workflows::local_workflows::LocalWorkflows;
use crate::workspace::{ActiveSession, OneTimeModalModel, ToastStack};
use crate::workspaces::user_profiles::UserProfiles;
#[cfg(feature = "local_tty")]
use anyhow::Context;
use anyhow::{anyhow, Result};
use appearance::{Appearance, AppearanceManager};
use channel::ChannelState;
use interval_timer::IntervalTimer;
use itertools::Itertools;
use rust_embed::RustEmbed;
use settings::{ExtraMetaKeys, PrivacySettings};
use std::borrow::Cow;
use std::collections::HashSet;
use std::ops::Deref;
use std::sync::Arc;
use terminal::input;
use terminal::session_settings::SessionSettings;
use url::Url;
use warp_core::execution_mode::{AppExecutionMode, ExecutionMode};
use warp_managed_secrets::ManagedSecretManager;
use workspace::sync_inputs::SyncedInputState;

use warpui::{integration::TestDriver, App, AssetProvider, Event};

use self::features::FeatureFlag;
use crate::app_state::AppState;
use crate::cloud_object::model::persistence::ObjectStoreModel;
use crate::drive::ObjectTypeAndId;
use crate::experiments::ImprovedPaletteSearch;
pub use crate::global_resource_handles::{GlobalResourceHandles, GlobalResourceHandlesProvider};
use crate::notification::NotificationContext;
use crate::root_view::{
    quake_mode_window_id, quake_mode_window_is_open, OpenFromRestoredArg, OpenPath,
};
pub use crate::server::telemetry::{
    AgentModeEntrypoint, AgentModeEntrypointSelectionType, TelemetryEvent,
    // Re-exported for `remote_server::codebase_index_model`, which must not
    // import from `crate::server::` directly (`script/check_cloud_boundary`).
    // These are telemetry *shapes*, not a cloud dependency --
    // `send_telemetry_from_ctx!` is already a compiled-out no-op in this fork.
    RemoteCodebaseAutoIndexTrigger, RemoteCodebaseIndexStatusTelemetrySource,
};
use crate::server::telemetry::{AppStartupInfo, CloseTarget, PaletteSource};
use crate::terminal::CustomSecretRegexUpdater;
use crate::util::bindings::is_binding_cross_platform;
use crate::workspace::{PaneViewLocator, Workspace, WorkspaceAction};
use crate::workspaces::user_workspaces::UserWorkspaces;
use warp_logging::{LogDestination, LogFrontend};

// Re-export the send_telemetry_from_ctx macro at the crate root level
pub use warp_core::send_telemetry_from_app_ctx;
pub use warp_core::send_telemetry_from_ctx;
pub use warp_core::send_telemetry_on_executor;
pub use warp_core::send_telemetry_sync_from_app_ctx;
pub use warp_core::send_telemetry_sync_from_ctx;

// Re-export the safe logging macros at the crate root level for backwards compatibility
pub use warp_core::{safe_debug, safe_error, safe_info, safe_warn};

use crate::antivirus::AntivirusInfo;
#[cfg(feature = "local_fs")]
use warp_files::FileModel;
use warpui::platform::TerminationMode;
use warpui::windowing::state::ApplicationStage;
use warpui::{AppContext, SingletonEntity, WindowId};

#[derive(Clone, Copy, RustEmbed)]
#[folder = "assets"]
#[include = "bundled/**"] // Should be kept in sync with BUNDLED_ASSETS_DIR.
#[include = "async/**"] // Should be kept in sync with ASYNC_ASSETS_DIR.
#[cfg_attr(target_family = "wasm", exclude = "async/**")]
// Excludes take precedence.
// Standalone CLI builds (the `oz` tarball) are headless and never render the
// onboarding/theme imagery in `async/`, so we exclude those bytes from the
// embedded asset set to keep the CLI binary small — mirroring the carve-out
// already applied for the WASM target above.
#[cfg_attr(feature = "standalone", exclude = "async/**")]
pub struct Assets;

pub static ASSETS: Assets = Assets;

/// Launch mode for how to start up Zap.
#[allow(clippy::large_enum_variant)]
pub enum LaunchMode {
    /// Run the regular GUI application.
    App {
        args: warp_cli::AppArgs,
        /// API key for server authentication, if provided via `--api-key` or `WARP_API_KEY`.
        /// Only used on dogfood channels.
        api_key: Option<String>,
    },

    /// Run the Zap command-line SDK.
    CommandLine {
        command: warp_cli::CliCommand,
        global_options: GlobalOptions,
        debug: bool,
        /// Whether this CLI invocation is running in a sandboxed environment.
        is_sandboxed: bool,
        /// Override for computer use permission from CLI flags. If None, uses default behavior.
        computer_use_override: Option<bool>,
    },
    /// Run a test - this may be an integration test or an eval.
    Test {
        driver: Box<Option<TestDriver>>,
        is_integration_test: bool,
    },

    /// Remote server proxy — bridges SSH stdio to the daemon's Unix socket.
    /// This is a short-lived process that runs for the lifetime of an SSH session.
    RemoteServerProxy,

    /// Remote server daemon — long-lived headless process serving remote
    /// connections via a Unix domain socket.
    RemoteServerDaemon,

    /// Run the headless terminal UI (the `warp-tui` binary in the `warp_tui`
    /// crate). Reuses the full `initialize_app` bootstrap, then mounts the TUI
    /// instead of the GUI.
    Tui {
        /// Builds the root TUI view and starts the TUI driver. Runs after
        /// `initialize_app`; supplied by [`run_tui`]. Carried in the variant so
        /// it stays scoped to this mode.
        mount: TuiMountFn,
        /// API key for server authentication, if provided via `--api-key` or
        /// `WARP_API_KEY`. Only used on dogfood channels.
        api_key: Option<String>,
    },
}

/// The headless TUI front-end's mount callback, carried by [`LaunchMode::Tui`].
/// Supplied to [`run_tui`] by the `warp_tui` crate; it runs after
/// `initialize_app` to build the root TUI view and start the TUI driver.
pub type TuiMountFn = Box<dyn FnOnce(&mut warpui::AppContext)>;

impl LaunchMode {
    fn args(&self) -> Cow<'_, warp_cli::AppArgs> {
        match self {
            LaunchMode::App { args, .. } => Cow::Borrowed(args),
            LaunchMode::CommandLine { .. }
            | LaunchMode::Test { .. }
            | LaunchMode::RemoteServerProxy
            | LaunchMode::RemoteServerDaemon
            | LaunchMode::Tui { .. } => Cow::Owned(warp_cli::AppArgs::default()),
        }
    }

    /// Returns `true` if this process is running an integration test.
    fn is_integration_test(&self) -> bool {
        match self {
            LaunchMode::Test {
                is_integration_test,
                ..
            } => *is_integration_test,
            LaunchMode::App { .. }
            | LaunchMode::CommandLine { .. }
            | LaunchMode::RemoteServerProxy
            | LaunchMode::RemoteServerDaemon
            | LaunchMode::Tui { .. } => false,
        }
    }

    fn take_test_driver(&mut self) -> Option<TestDriver> {
        match self {
            LaunchMode::Test { driver, .. } => driver.take(),
            LaunchMode::App { .. }
            | LaunchMode::CommandLine { .. }
            | LaunchMode::RemoteServerProxy
            | LaunchMode::RemoteServerDaemon
            | LaunchMode::Tui { .. } => None,
        }
    }

    /// Add an URL to open. Only supported for [`LaunchMode::App`]
    #[allow(dead_code)]
    fn add_url(&mut self, url: Url) {
        if let LaunchMode::App { args, .. } = self {
            args.urls.push(url);
        }
    }

    fn execution_mode(&self) -> ExecutionMode {
        match self {
            LaunchMode::App { .. } => ExecutionMode::App,
            LaunchMode::CommandLine { .. } => ExecutionMode::Sdk,
            LaunchMode::Test { .. } => ExecutionMode::App,
            LaunchMode::Tui { .. } => ExecutionMode::Tui,
            // RemoteServerProxy and RemoteServerDaemon don't use execution
            // mode, but Sdk is the closest match (headless, no GUI).
            LaunchMode::RemoteServerProxy | LaunchMode::RemoteServerDaemon => ExecutionMode::Sdk,
        }
    }

    fn is_sandboxed(&self) -> bool {
        match self {
            LaunchMode::CommandLine { is_sandboxed, .. } => *is_sandboxed,
            LaunchMode::App { .. }
            | LaunchMode::Test { .. }
            | LaunchMode::RemoteServerProxy
            | LaunchMode::RemoteServerDaemon
            | LaunchMode::Tui { .. } => false,
        }
    }

    /// Returns `true` if Zap should run headlessly, without a visible UI.
    ///
    /// The TUI is headless in this sense (no GUI window): it builds a headless
    /// app and then mounts its own terminal-UI driver.
    fn is_headless(&self) -> bool {
        match self {
            LaunchMode::CommandLine { command, .. } => match command {
                CliCommand::Agent(AgentCommand::Run(args)) => !args.gui,
                _ => true,
            },
            LaunchMode::RemoteServerProxy
            | LaunchMode::RemoteServerDaemon
            | LaunchMode::Tui { .. } => true,
            LaunchMode::App { .. } | LaunchMode::Test { .. } => false,
        }
    }

    /// Returns `true` if this process can build and sync codebase indices.
    ///
    /// Ported from the pin (`02b53fcd8:app/src/lib.rs:566`). Two arms differ,
    /// both because of what this fork's `LaunchMode` is rather than a policy
    /// choice:
    ///
    /// * `RemoteServerDaemon` is a unit variant here and never reaches
    ///   `initialize_app` — the daemon bootstraps itself in
    ///   `remote_server::run_daemon_app`, which registers its own
    ///   `CodebaseIndexManager`. This arm is therefore unreachable in practice
    ///   and answers `false` rather than duplicating the daemon's own
    ///   `FeatureFlag::RemoteCodebaseIndexing` check in a second place, where
    ///   the two could drift.
    /// * The `Tui` arm keeps the pin's `false` and the pin's reason verbatim.
    #[cfg_attr(target_family = "wasm", allow(dead_code))]
    fn supports_indexing(&self) -> bool {
        match self {
            LaunchMode::CommandLine { command, .. } => {
                matches!(command, CliCommand::Agent(AgentCommand::Run(_)))
            }
            LaunchMode::App { .. } | LaunchMode::Test { .. } => true,
            LaunchMode::RemoteServerProxy | LaunchMode::RemoteServerDaemon => false,
            // Codebase indexing stays off for the TUI until it has deferred
            // persisted-index restore and multi-process-safe snapshot writes
            // (the GUI may run concurrently against the same data dir).
            // Project rules/skills discovery does not depend on this; see
            // `PersistedWorkspace::new`.
            LaunchMode::Tui { .. } => false,
        }
    }

    /// Whether or not to start a crash recovery process (on platforms that support it).
    #[cfg(enable_crash_recovery)]
    pub(crate) fn crash_recovery_enabled(&self) -> bool {
        match self {
            LaunchMode::App { .. } => true,
            LaunchMode::CommandLine { .. }
            | LaunchMode::Test { .. }
            | LaunchMode::RemoteServerProxy
            | LaunchMode::RemoteServerDaemon
            | LaunchMode::Tui { .. } => false,
        }
    }

    /// Whether local crash reporting needs to be initialized in `init_common`.
    #[cfg_attr(not(feature = "crash_reporting"), allow(dead_code))]
    fn needs_crash_reporting(&self) -> bool {
        match self {
            LaunchMode::App { .. }
            | LaunchMode::CommandLine { .. }
            | LaunchMode::Test { .. }
            | LaunchMode::RemoteServerDaemon
            | LaunchMode::RemoteServerProxy
            | LaunchMode::Tui { .. } => true,
        }
    }

    /// Whether profiling and tracing should be initialized in `init_common`.
    fn needs_profiling(&self) -> bool {
        match self {
            LaunchMode::App { .. }
            | LaunchMode::CommandLine { .. }
            | LaunchMode::Test { .. }
            | LaunchMode::RemoteServerDaemon
            | LaunchMode::RemoteServerProxy
            | LaunchMode::Tui { .. } => true,
        }
    }

    /// Log destination for this mode.
    fn log_destination(&self) -> Option<LogDestination> {
        match self {
            LaunchMode::CommandLine { debug, .. } => {
                if *debug {
                    Some(LogDestination::Stderr)
                } else {
                    Some(LogDestination::File)
                }
            }
            // Proxy must log to stderr because stdout is the protocol channel.
            LaunchMode::RemoteServerProxy => Some(LogDestination::Stderr),
            LaunchMode::RemoteServerDaemon => Some(LogDestination::File),
            // GUI always writes to a log file, regardless of whether stdout is a tty.
            //
            // When `None` is passed, `warp_logging`'s check is
            // `!stdout_is_a_tty && !in_ci && !integration_test`
            // (crates/warp_logging/src/native.rs:517). When launched from a terminal via
            // `./script/run`, stdout is a tty, so all logs would get smeared onto the
            // terminal, mixed in with the foreground program's output — unreadable.
            // A GUI app shouldn't be treating logs as UI output anyway.
            //
            // To temporarily view live logs: `ZAP_LOG_STDOUT=1 ./script/run`.
            LaunchMode::App { .. } => {
                if std::env::var_os("ZAP_LOG_STDOUT").is_some() {
                    Some(LogDestination::Stderr)
                } else {
                    Some(LogDestination::File)
                }
            }
            // Tests keep `None`: the CI / integration branch's check must still apply as before.
            LaunchMode::Test { .. } => None,
            // The TUI uses the alt screen (stdout is hidden), so it logs to a
            // file like the GUI; the warp_tui front-end has its own stdout
            // escape hatch (ZAP_LOG_STDOUT).
            LaunchMode::Tui { .. } => Some(LogDestination::File),
        }
    }

    /// Log frontend for this mode. Selects the log subdirectory and rotation
    /// policy (`crates/warp_logging`); the TUI gets its own subdirectory
    /// (`warp-cli`) distinct from both the GUI and the `oz` CLI subdirectory,
    /// so a long-running TUI session's logs don't compete with CLI
    /// invocations for the same rotation slots.
    fn log_frontend(&self) -> LogFrontend {
        match self {
            LaunchMode::Tui { .. } => LogFrontend::Tui,
            LaunchMode::App { .. } | LaunchMode::Test { .. } => LogFrontend::Gui,
            LaunchMode::CommandLine { .. }
            | LaunchMode::RemoteServerProxy
            | LaunchMode::RemoteServerDaemon => LogFrontend::Cli,
        }
    }

    #[cfg(any(test, feature = "test-util"))]
    pub(crate) fn new_for_unit_test() -> Self {
        LaunchMode::Test {
            driver: Box::new(None),
            is_integration_test: false,
        }
    }

    /// A CLI launch mode for fixtures that must model the CLI's permission
    /// defaults rather than the GUI's.
    ///
    /// `new_for_unit_test` yields `LaunchMode::Test`, which routes
    /// `AIExecutionProfilesModel::new` to the GUI default profile
    /// (`execute_commands`/`write_to_pty`: `AlwaysAsk`). A TUI CLI subagent runs
    /// under `DefaultProfileState::Cli` instead -- deliberately more permissive,
    /// see the comment on that arm -- so a fixture built on the Test mode does
    /// not model what it claims to.
    #[cfg(any(test, feature = "test-util"))]
    pub(crate) fn new_for_cli_unit_test() -> Self {
        LaunchMode::CommandLine {
            command: warp_cli::CliCommand::Whoami,
            global_options: Default::default(),
            debug: false,
            is_sandboxed: false,
            computer_use_override: None,
        }
    }
}

/// Extracts the raw `--api-key` / `WARP_API_KEY` value carried by a launch mode, if any.
/// Whether that value is actually *honored* (e.g. the dogfood-channel gate applied at the
/// `initialize_app` call site) is a separate concern; this is pure field extraction so it
/// can be unit-tested without standing up a full `AppContext`.
fn api_key_from_launch_mode(launch_mode: &LaunchMode) -> Option<String> {
    match launch_mode {
        LaunchMode::CommandLine { global_options, .. } => global_options.api_key.clone(),
        LaunchMode::App { api_key, .. } | LaunchMode::Tui { api_key, .. } => api_key.clone(),
        LaunchMode::Test { .. }
        | LaunchMode::RemoteServerProxy
        | LaunchMode::RemoteServerDaemon => None,
    }
}

impl AssetProvider for Assets {
    fn get(&self, path: &str) -> Result<Cow<'_, [u8]>> {
        <Assets as RustEmbed>::get(path)
            .map(|f| f.data)
            .ok_or_else(|| anyhow!("no asset exists at path {}", path))
    }
}

/// If the given event is a key down event containing alt modifiers, and those
/// alt modifiers should be treated as meta keys, then remove the alts and
/// prefix the keys with an escape. See WAR-472.
fn apply_extra_meta_keys(event: &mut Event, extra_metas: ExtraMetaKeys) {
    if let Event::KeyDown {
        keystroke, details, ..
    } = event
    {
        let left_as_meta = extra_metas.left_alt && details.left_alt;
        let right_as_meta = extra_metas.right_alt && details.right_alt;
        if left_as_meta || right_as_meta {
            let side = match (left_as_meta, right_as_meta) {
                (true, true) => "left+right alt",
                (true, false) => "left alt",
                (false, true) => "right alt",
                (false, false) => unreachable!(),
            };
            log::info!("Treating {side} as meta");
            keystroke.alt = false;
            keystroke.meta = true;
        }
    }
}

fn apply_scroll_multiplier(event: &mut Event, app: &AppContext) {
    if let Event::ScrollWheel { delta, precise, .. } = event {
        if !*precise {
            let scroll_multiplier = *ScrollSettings::as_ref(app).mouse_scroll_multiplier.value();
            *delta *= scroll_multiplier;
        }
    }
}

/// Runs the app. If a subcommand was requested, it'll be run instead of the main application.
pub fn run() -> Result<()> {
    // POSIX locale fallback: when LANG/LC_* are all unset, give C/Rust libraries that
    // depend on these env vars (chrono number formatting, libc strftime, etc.) a
    // reasonable UTF-8 default. Deliberately skipped on Windows — the Windows API
    // (`GetUserPreferredUILanguages`) is the real source of truth for the UI locale there,
    // and forcing `LANG=en_US.UTF-8` would make `DesktopLanguageRequester` always return
    // `en` regardless of the user's chosen UI language, skewing the CJK Han glyph fallback
    // (Japanese UI would end up with Simplified Chinese glyphs instead).
    // On macOS, launching the app bundle from the desktop also has no environment
    // variables set, but its `DesktopLanguageRequester` returns the `LANG` env var
    // directly (without consulting the system) if it happens to be set, so it needs
    // to be skipped here too.
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    if std::env::var_os("LANG").is_none()
        && std::env::var_os("LC_ALL").is_none()
        && std::env::var_os("LC_CTYPE").is_none()
    {
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("LANG", "en_US.UTF-8") };
    }

    // Perform any necessary platform-specific initialization.
    platform::init();

    // i18n must be initialized before any UI `t!()` call; it starts with the system
    // locale, then gets overridden by LanguageSettings once settings finish loading.
    // Safe against OnceLock re-entrancy.
    i18n::init(None);

    // Ensure feature flags are initialized before parsing command-line arguments.
    init_feature_flags();

    // The bundled Warp Control wrapper injects the hidden `--warpctrl` flag, which
    // selects a separate argument parser and never falls through to the normal
    // Oz/GUI parser below. Oz subcommands are part of that normal parser and
    // therefore do not require a separate mode flag.
    if let Some(control_args) = warp_cli::local_control::ControlArgs::from_control_mode_env() {
        #[cfg(windows)]
        warp_util::windows::attach_to_parent_console();
        warp_cli::local_control::run_and_exit(control_args);
    }

    // Parse command-line arguments.
    let args = warp_cli::Args::from_env();

    if let Some(command) = args.command() {
        #[cfg(windows)]
        if command.prints_to_stdout() {
            // We attach a console to ensure that all standard output gets printed correctly.
            warp_util::windows::attach_to_parent_console();
        }
        match command {
            // Worker re-execs (terminal server, plugin host, remote server, …)
            // are dispatched by the shared helper so the headless TUI front-end
            // can reuse it via `run_tui_worker_if_requested`.
            warp_cli::Command::Worker(worker) => return run_worker_command(worker),
            warp_cli::Command::Completions { shell } => {
                return warp_cli::completions::generate_to_stdout(*shell);
            }
            warp_cli::Command::CommandLine(cmd) => {
                let (is_sandboxed, computer_use_override) = match cmd.as_ref() {
                    warp_cli::CliCommand::Agent(warp_cli::agent::AgentCommand::Run(run_args)) => (
                        run_args.sandboxed,
                        run_args.computer_use.computer_use_override(),
                    ),
                    _ => (false, None),
                };

                return run_internal(LaunchMode::CommandLine {
                    command: cmd.as_ref().clone(),
                    global_options: GlobalOptions {
                        output_format: args.output_format(),
                        api_key: args.api_key().cloned(),
                    },
                    debug: args.debug(),
                    is_sandboxed,
                    computer_use_override,
                });
            }
            warp_cli::Command::DumpDebugInfo => {
                return debug_dump::run();
            }
        }
    }

    // If running as a standalone CLI binary or invoked as "oz", print help
    // instead of launching the GUI app.
    let is_cli_binary = cfg!(feature = "standalone")
        || warp_cli::binary_name().is_some_and(|name| name.starts_with("oz"))
        || std::env::var_os("WARP_CLI_MODE").is_some();
    if is_cli_binary {
        warp_cli::Args::clap_command().print_help()?;
        return Ok(());
    }

    let api_key = args.api_key().cloned();
    run_internal(LaunchMode::App {
        args: args.into_app_args(),
        api_key,
    })
}

/// Runs an integration test using the provided test driver.
pub fn run_integration_test(driver: TestDriver) -> Result<()> {
    let is_integration_test = std::env::var("WARP_INTEGRATION").is_ok();
    let launch = LaunchMode::Test {
        driver: Box::new(Some(driver)),
        is_integration_test,
    };
    run_internal(launch)
}

/// Dispatches a worker re-exec (terminal server, plugin host, remote server,
/// minidump server, ripgrep search). Shared by [`run`] and the headless TUI's
/// [`run_tui_worker_if_requested`]: the single binary may be re-exec'd as a
/// worker and must run it instead of starting the main app / another TUI.
fn run_worker_command(worker: &warp_cli::WorkerCommand) -> Result<()> {
    match worker {
        #[cfg(all(feature = "local_tty", unix))]
        warp_cli::WorkerCommand::TerminalServer(args) => {
            // Ideally the terminal server would be a separate binary, but a
            // single binary is easier to distribute, so we run its event loop
            // here as the closest approximation.
            crate::terminal::local_tty::server::run_terminal_server(args);
            Ok(())
        }
        #[cfg(feature = "plugin_host")]
        warp_cli::WorkerCommand::PluginHost { .. } => crate::run_plugin_host(),
        #[cfg(feature = "local_tty")]
        warp_cli::WorkerCommand::MinidumpServer { socket_name } => {
            cfg_if::cfg_if! {
                if #[cfg(all(linux_or_windows, feature = "crash_reporting"))] {
                    crate::crash_reporting::run_minidump_server(socket_name)
                } else {
                    let _ = socket_name;
                    panic!("The minidump server is not supported on this platform");
                }
            }
        }
        #[cfg(not(target_family = "wasm"))]
        warp_cli::WorkerCommand::RemoteServerProxy(args) => {
            init_common(&LaunchMode::RemoteServerProxy, None)?;
            crate::remote_server::run_proxy(args.identity_key.clone())
        }
        #[cfg(not(target_family = "wasm"))]
        warp_cli::WorkerCommand::RemoteServerDaemon(args) => {
            init_common(&LaunchMode::RemoteServerDaemon, None)?;
            crate::remote_server::run_daemon(args.identity_key.clone())
        }
        #[cfg(not(target_family = "wasm"))]
        warp_cli::WorkerCommand::RipgrepSearch {
            parent,
            ignore_case,
            multiline,
            pattern,
            paths,
        } => {
            warp_ripgrep::search::run_search_subprocess(
                std::slice::from_ref(pattern),
                paths.clone(),
                *ignore_case,
                *multiline,
                parent.pid,
            )
            .map_err(|err| anyhow!(err.to_string()))?;
            Ok(())
        }
        #[cfg(not(any(
            feature = "local_tty",
            feature = "plugin_host",
            not(target_family = "wasm")
        )))]
        worker => {
            // On wasm, specifically, we should fail spectacularly if we get here.
            #[cfg(target_family = "wasm")]
            panic!("Worker process not supported on WASM: {worker:?}")
        }
    }
}

/// Runs the headless TUI front-end (the `warp-tui` binary in the `warp_tui`
/// crate). Bootstraps the real (headless) app and then runs `mount`, which
/// builds the root TUI view and starts the non-blocking TUI driver.
///
/// `mount` is supplied by the `warp_tui` crate (which owns the concrete root
/// view plus the window/driver bootstrap), so `warp` never depends on
/// `warp_tui`.
#[cfg(feature = "tui")]
pub fn run_tui(api_key: Option<String>, mount: TuiMountFn) -> Result<()> {
    run_internal(LaunchMode::Tui { mount, api_key })
}

/// Dispatches a worker command when the current executable was re-invoked for
/// one. The TUI's single binary may be re-exec'd as a worker (e.g. the terminal
/// server); this runs it instead of recursively launching another TUI.
#[cfg(feature = "tui")]
pub fn run_tui_worker_if_requested() -> Option<Result<()>> {
    // Worker spawners always put the worker mode in argv[1]. Do not scan later
    // arguments because a TUI prompt value may legitimately match a worker name.
    let is_worker = std::env::args()
        .nth(1)
        .is_some_and(|arg| warp_cli::is_worker_invocation(&arg));
    if !is_worker {
        return None;
    }

    init_feature_flags();
    let args = warp_cli::Args::from_env();
    let Some(warp_cli::Command::Worker(worker)) = args.command() else {
        return Some(Err(anyhow!(
            "Recognized a Phosphor worker invocation, but failed to parse its worker command"
        )));
    };
    Some(run_worker_command(worker))
}

/// Shared early initialization for **every** process type (app, CLI, proxy,
/// daemon).  Every step in this function runs for all modes, including
/// lightweight ones like Proxy.  Think carefully before adding here — if
/// the step is only needed by the full app, add it to `run_internal`
/// instead.
fn init_common(launch_mode: &LaunchMode, timer: Option<&mut IntervalTimer>) -> Result<()> {
    #[cfg(windows)]
    dynamic_libraries::configure_library_loading();

    if launch_mode.needs_profiling() {
        profiling::init();
    }

    // The `run` function already initializes feature flags, but ensure they're initialized here
    // for other entrypoints.
    init_feature_flags();

    if launch_mode.needs_profiling() {
        tracing::init()?;
    }

    let log_destination = launch_mode.log_destination();

    cfg_if::cfg_if! {
        if #[cfg(enable_crash_recovery)] {
            if crash_recovery::is_crash_recovery_process(launch_mode.args().as_ref()) {
                warp_logging::init_for_crash_recovery_process()?;
            } else {
                warp_logging::init(warp_logging::LogConfig {
                    frontend: launch_mode.log_frontend(),
                    log_destination,
                    ..Default::default()
                })?;
            }
        } else {
            warp_logging::init(warp_logging::LogConfig {
                frontend: launch_mode.log_frontend(),
                log_destination,
                ..Default::default()
            })?;
        }
    }

    if let Some(timer) = timer {
        timer.mark_interval_end("LOG_FILE_SETUP_COMPLETE");
    }

    // Claim a background-only process type before anything else can reach
    // AppKit, so a headless launch never acquires a Dock tile.
    #[cfg(target_os = "macos")]
    if launch_mode.is_headless()
        && let Err(e) = platform::mac::mark_process_as_background_only()
    {
        log::warn!("Failed to mark process as background-only: {e:#}");
    }

    // Adjust resource limits early, before doing other work, to ensure that
    // any children we spawn (like the terminal server) inherit our adjusted
    // rlimits.
    resource_limits::adjust_resource_limits();

    // Configure rustls to use its default crypto provider.  This MUST be called
    // before making any network requests that use TLS, otherwise rustls will
    // panic.
    #[cfg(not(target_family = "wasm"))]
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("must be able to initialize crypto provider for TLS support");

    Ok(())
}

/// Runs the app.
///
/// Note that every initialization step in this function is specific to the GUI app and Oz. If you want
/// to add setup steps that should be generic to all launch modes (e.g. remote server). It should be added
/// in init_common instead.
fn run_internal(mut launch_mode: LaunchMode) -> Result<()> {
    let mut timer = IntervalTimer::new();

    // i18n must be initialized before any UI `t!()` call. The GUI entry (`run`)
    // does this early, before arg parsing; but the Test (`run_integration_test`)
    // and TUI entries reach `run_internal` without passing through `run`, so
    // without this their UI would render raw fluent keys (e.g. a Settings tab
    // titled `settings-title`, or a command palette whose action labels never
    // match a human-readable search). `init` is idempotent (OnceLock), so the
    // GUI's earlier call is undisturbed. Workers that render no UI take the
    // `init_common` path directly and intentionally skip this.
    i18n::init(None);

    init_common(&launch_mode, Some(&mut timer))?;

    // SQLite prewarm: kick off init_db() (connection + migration) on a background
    // thread before app_builder.run() is called, so SQLite initialization runs
    // concurrently with the subsequent winit / wgpu initialization. The main thread
    // picks up the prewarmed connection when it reaches persistence::initialize.
    // Only needed for LaunchMode::App (CLI / Worker / Test take other paths that
    // don't call persistence::initialize inside initialize_app).
    if matches!(launch_mode, LaunchMode::App { .. }) {
        log::info!("Triggering SQLite prewarm in background...");
        crate::persistence::prewarm_db_in_background();
    }

    // For wasm builds we have this special case to parse out the intent
    // from the url that is used to visite the app on web.
    #[cfg(target_family = "wasm")]
    {
        use uri::web_intent_parser;
        if let Some(intent) = web_intent_parser::parse_web_intent_from_current_url() {
            launch_mode.add_url(intent);
        }
        web_intent_parser::set_context_flags_from_current_url();
    }

    // Collect run_internal() errors that occur before app initialization; log them once local crash reporting has been initialized.
    #[cfg_attr(
        not(all(
            feature = "release_bundle",
            any(
                windows,
                any(target_os = "linux", target_os = "freebsd", target_os = "macos")
            )
        )),
        expect(unused_mut)
    )]
    let mut pre_init_errors: Vec<anyhow::Error> = Vec::new();

    #[cfg(all(
        feature = "release_bundle",
        any(target_os = "linux", target_os = "freebsd")
    ))]
    if let LaunchMode::App { .. } = launch_mode {
        match app_services::linux::pass_startup_args_to_existing_instance(
            launch_mode.args().as_ref(),
        ) {
            // If we were able to contact an existing application instance, quit -
            // we only want to run a single instance of Zap at a time.
            Ok(_) => std::process::exit(0),
            // If Zap isn't already running, we're good to go.
            Err(app_services::linux::StartupArgsForwardingError::NoExistingInstance) => {}
            // If we just finished an auto-update, we should continue running.
            Err(app_services::linux::StartupArgsForwardingError::IgnoredAfterAutoUpdate) => {}
            // If we were unable to perform the forwarding for an unknown reason,
            // it's better to run a second instance than potentially end up in a
            // state where Zap refuses to run even a first instance.
            Err(err) => {
                let err = anyhow::Error::from(err).context("Failed to forward startup args");
                log::error!("{err:#}");
                pre_init_errors.push(err);
            }
        }
    }

    #[cfg(all(feature = "release_bundle", windows))]
    if let LaunchMode::App { .. } = launch_mode {
        match app_services::windows::pass_startup_args_to_existing_instance(
            launch_mode.args().as_ref(),
        ) {
            // If we were able to contact an existing application instance, quit -
            // we only want to run a single instance of Zap at a time.
            Ok(_) => std::process::exit(0),
            // If Zap isn't already running, we're good to go.
            Err(app_services::windows::StartupArgsForwardingError::NoExistingInstance) => {}
            // If we just finished an auto-update, we should continue running.
            Err(app_services::windows::StartupArgsForwardingError::IgnoredAfterAutoUpdate) => {}
            // If we were unable to perform the forwarding for an unknown reason,
            // it's better to run a second instance than potentially end up in a
            // state where Zap refuses to run even a first instance.
            Err(err) => {
                let err = anyhow::Error::from(err).context("Failed to forward startup args");
                log::error!("{err:#}");
                pre_init_errors.push(err);
            }
        }
    }

    // Unlike Finder/Dock launches (deduplicated automatically by LaunchServices), the bundled
    // `phosphor-oss` shell-integration script bypasses LaunchServices by directly exec-ing the main
    // binary, so it needs the same explicit single-instance check Linux/Windows already have.
    #[cfg(all(feature = "release_bundle", target_os = "macos"))]
    if let LaunchMode::App { .. } = launch_mode {
        match app_services::mac::pass_startup_args_to_existing_instance(
            launch_mode.args().as_ref(),
        ) {
            // If we were able to contact an existing application instance, quit -
            // we only want to run a single instance of Zap at a time.
            Ok(_) => std::process::exit(0),
            // If Zap isn't already running, we're good to go.
            Err(app_services::mac::StartupArgsForwardingError::NoExistingInstance) => {}
            // If we just finished an auto-update, we should continue running.
            Err(app_services::mac::StartupArgsForwardingError::IgnoredAfterAutoUpdate) => {}
            // If we were unable to perform the forwarding for an unknown reason,
            // it's better to run a second instance than potentially end up in a
            // state where Zap refuses to run even a first instance.
            Err(err) => {
                let err = anyhow::Error::from(err).context("Failed to forward startup args");
                log::error!("{err:#}");
                pre_init_errors.push(err);
            }
        }
    }

    // Sets up a Job Object that we associate with the Zap process to handle
    // shared fate with its child processes. This should be called before we
    // start spawning any child processes.
    #[cfg(windows)]
    command::windows::init();

    let private_preferences = settings::init_private_user_preferences();
    let (public_preferences, startup_toml_parse_error) = settings::init_public_user_preferences();

    // When the SettingsFile feature flag is enabled, public settings live in
    // the TOML-backed store. When disabled, they live in the platform-native
    // store (same backend as private). Use the correct one for pre-app reads.
    #[cfg_attr(
        not(any(enable_crash_recovery, any(target_os = "linux", target_os = "freebsd"))),
        expect(unused)
    )]
    let prefs_for_public_settings: &dyn warpui_extras::user_preferences::UserPreferences =
        if FeatureFlag::SettingsFile.is_enabled() {
            public_preferences.as_ref()
        } else {
            private_preferences.deref()
        };

    #[cfg(enable_crash_recovery)]
    let crash_recovery =
        crash_recovery::CrashRecovery::new(&launch_mode, prefs_for_public_settings);

    // Set up the pty spawner before doing any meaningful work. We want to
    // ensure that the process is in the cleanest possible state (minimal opened
    // files, modified signal handlers, etc.) to avoid unexpected effects on
    // spawned ptys.
    #[cfg(feature = "local_tty")]
    let pty_spawner =
        terminal::local_tty::spawner::PtySpawner::new().context("Failed to create pty spawner")?;

    let mut app_builder = if launch_mode.is_headless() {
        warpui::platform::AppBuilder::new_headless(
            app_callbacks(launch_mode.is_integration_test()),
            Box::new(ASSETS),
            launch_mode.take_test_driver(),
        )
    } else {
        warpui::platform::AppBuilder::new(
            app_callbacks(launch_mode.is_integration_test()),
            Box::new(ASSETS),
            launch_mode.take_test_driver(),
        )
    };

    // A headless invocation has no Dock presence, so it performs no Dock-visible
    // setup at all (Dock icon, Dock menu, menu bar).
    #[cfg(target_os = "macos")]
    if !launch_mode.is_headless() {
        use warpui::platform::mac::AppExt;

        let activate_on_launch = !launch_mode.is_integration_test()
            || std::env::var("WARPUI_USE_REAL_DISPLAY_IN_INTEGRATION_TESTS").is_ok();
        app_builder.set_activate_on_launch(activate_on_launch);

        let dev_icon = ASSETS.get("bundled/png/local.png")?;
        app_builder.set_dev_icon(dev_icon);

        app_builder.set_menu_bar_builder(app_menus::menu_bar);
        app_builder.set_dock_menu_builder(|_| app_menus::dock_menu());
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    {
        use crate::settings::ForceX11;
        use warpui::platform::linux::{self, AppBuilderExt};

        app_builder.set_window_class(ChannelState::app_id().to_string());

        let force_x11 = ForceX11::read_from_preferences(prefs_for_public_settings)
            .unwrap_or(ForceX11::default_value());
        // Force use of wayland if the user has passed the `WARP_ENABLE_WAYLAND` env var.
        let allow_wayland = linux::is_wayland_env_var_set() || !force_x11;
        app_builder.force_x11(!allow_wayland);
    }

    #[cfg(target_os = "windows")]
    {
        use warpui::platform::windows::AppBuilderExt;
        app_builder.set_app_user_model_id(ChannelState::app_id().to_string());

        // Only use DXC for DirectX shader compilation if we're not running in a Parallels VM
        // Parallels VMs can have issues with DXC shader compilation
        let is_parallels_vm = crate::util::vm_detection::is_running_in_windows_parallels_vm();
        if !is_parallels_vm {
            log::info!("Using DXC for DirectX shader compilation");
            use warpui::platform::windows::DXCPath;

            app_builder.use_dxc_for_directx_shader_compilation(DXCPath {
                dxc_path: "dxcompiler.dll".to_string(),
                dxil_path: "dxil.dll".to_string(),
            });
        } else {
            log::info!("Skipping DXC for DirectX shader compilation; running in a Parallels VM");
        }
    }

    // Override any bindings that have a `Custom` trigger to a `Keystroke`-based trigger. In theory,
    // this should be a noop on Mac (since the keystrokes registered via the  Mac menus first
    // intercept the binding), but just to be safe we only enable this in cases where we don't
    // include mac menus.
    #[cfg(not(target_os = "macos"))]
    app_builder.convert_custom_triggers_to_keystroke_triggers(
        crate::util::bindings::custom_tag_to_keystroke,
    );

    #[cfg(target_os = "macos")]
    app_builder.register_default_keystroke_triggers_for_custom_actions(
        crate::util::bindings::custom_tag_to_keystroke,
    );

    app_builder.run(move |ctx| {
        #[cfg(not(target_family = "wasm"))]
        // Rotate the log files in the background.
        ctx.background_executor()
            .spawn(warp_logging::rotate_log_files())
            .detach();

        ctx.add_singleton_model(|ctx| {
            AppExecutionMode::new(
                launch_mode.execution_mode(),
                launch_mode.is_sandboxed(),
                ctx,
            )
        });
        #[cfg(feature = "crash_reporting")]
        crate::crash_reporting::set_client_type_tag(launch_mode.execution_mode().client_id());

        // Add the terminal server singleton to the application.
        #[cfg(feature = "local_tty")]
        ctx.add_singleton_model(move |_ctx| pty_spawner);

        // Register user preferences.  This must be done before initializing
        // feature flags or experiments, both of which check user preferences for
        // overrides.
        ctx.add_singleton_model(move |_ctx| ::settings::PublicPreferences::new(public_preferences));
        ctx.add_singleton_model(move |_ctx| private_preferences);
        let startup_toml_parse_error = startup_toml_parse_error;

        // Tell the settings crate whether the TOML settings file is active.
        // This must happen after preferences are registered but before settings
        // are initialized, so the routing logic picks the correct backend.
        ::settings::set_settings_file_enabled(FeatureFlag::SettingsFile.is_enabled());

        #[cfg(enable_crash_recovery)]
        ctx.add_singleton_model(move |_ctx| crash_recovery);

        #[cfg(feature = "plugin_host")]
        ctx.add_singleton_model(move |ctx| {
            plugin::PluginHost::new(ctx).expect("Could not instantiate PluginHost")
        });
        let app_state = initialize_app(
            &launch_mode,
            timer,
            startup_toml_parse_error,
            ctx,
            pre_init_errors,
        );

        if ImprovedPaletteSearch::improved_search_enabled(ctx) {
            FeatureFlag::UseTantivySearch.set_enabled(true);
        }

        // The TUI front-end reuses the full `initialize_app` bootstrap above (so
        // Appearance, settings, MCP, etc. exist), then mounts the TUI (via
        // `crate::tui::init`) instead of the GUI/CLI `launch()` path.
        match launch_mode {
            #[cfg(feature = "tui")]
            LaunchMode::Tui { mount, .. } => crate::tui::init(mount, ctx),
            #[cfg(not(feature = "tui"))]
            LaunchMode::Tui { .. } => {
                unreachable!("the `tui` launch mode requires the `tui` feature")
            }
            other => launch(ctx, app_state, other),
        }
    })
}

pub struct UpdateQuakeModeEventArg {
    active_window_id: Option<WindowId>,
}

fn initialize_app(
    launch_mode: &LaunchMode,
    mut timer: IntervalTimer,
    startup_toml_parse_error: Option<warpui_extras::user_preferences::Error>,
    ctx: &mut warpui::AppContext,
    pre_init_errors: impl IntoIterator<Item = anyhow::Error>,
) -> Option<AppState> {
    // Warning: errors that occur here, before crash_reporting::init, can only be
    // written to the local log. Only crash-reporting dependencies should be
    // initialized here; failures in other work should be pushed into pre_init_errors.
    let data_domain = ChannelState::data_domain();

    // Register an implementation of the secure storage service.
    cfg_if::cfg_if! {
        if #[cfg(feature = "integration_tests")] {
            warpui_extras::secure_storage::register_noop(&data_domain, ctx);
        } else if #[cfg(any(target_os = "linux", target_os = "freebsd"))] {
            warpui_extras::secure_storage::register_with_fallback(&data_domain, warp_core::paths::state_dir(), ctx)
        } else if #[cfg(target_os = "windows")] {
            warpui_extras::secure_storage::register_with_dir(&data_domain, warp_core::paths::state_dir(), ctx)
        } else {
            warpui_extras::secure_storage::register(&data_domain, ctx);
        }
    }

    // One-time migration: give Preview its own config directory by
    // symlinking contents from the shared ~/.warp location. Must run
    // before ensure_warp_watch_roots_exist() creates the new directory.
    #[cfg(target_os = "macos")]
    preview_config_migration::migrate_preview_config_dir_if_needed();

    ensure_warp_watch_roots_exist();
    ctx.add_singleton_model(WarpManagedPathsWatcher::new);

    ctx.add_singleton_model(WarpConfig::new);
    ctx.add_singleton_model(|_ctx| SettingsManager::default());

    let user_defaults_on_startup = settings::init(startup_toml_parse_error, ctx);
    timer.mark_interval_end("READ_USER_DEFAULTS_AND_INITIALIZE_SETTINGS");

    if FeatureFlag::UIZoom.is_enabled() {
        ctx.set_zoom_factor(WindowSettings::as_ref(ctx).zoom_level.as_zoom_factor());
    }

    // Extract API key from command line options, if applicable. `App`/`Tui` are gated to
    // dogfood channels; `CommandLine` is not.
    let api_key = match launch_mode {
        LaunchMode::CommandLine { .. } => api_key_from_launch_mode(launch_mode),
        LaunchMode::App { .. } | LaunchMode::Tui { .. } if ChannelState::channel().is_dogfood() => {
            api_key_from_launch_mode(launch_mode)
        }
        _ => None,
    };
    let api_key = if FeatureFlag::APIKeyAuthentication.is_enabled() {
        api_key
    } else {
        None
    };

    let auth_state = Arc::new(AuthState::initialize(ctx, api_key));
    timer.mark_interval_end("AUTH_MANAGER_SET_USER");

    let update_http_client = Arc::new(http_client::Client::new());

    // Zap: the AuthStateProvider singleton is kept only so legacy call sites can read the local placeholder user state.
    ctx.add_singleton_model(|_ctx| AuthStateProvider::new(auth_state.clone()));

    // Zap Wave 3-1: AuthManager has been localized into a stub; server_api / auth_client are no longer injected.
    ctx.add_singleton_model(AuthManager::new);

    ctx.add_singleton_model(|_ctx| GPUState::new());

    PrivacySettings::register_singleton(ctx);

    // Second phase of settings init: the one-shot settings migrations and the
    // seeding of the default secret-redaction regexes. Both hung off the pin's
    // server round-trip in `auth/auth_manager.rs`, which this fork deleted; both
    // need `AuthStateProvider` and `PrivacySettings`, so this is the earliest
    // point they can run. See the function's own comment for why startup is the
    // right trigger and why re-running it cannot clobber the user's edits.
    settings::run_startup_settings_initialization(ctx);

    // If any part of sqlite initialization fails, we just don't do session restoration (i.e.
    // feature degradation).
    let (sqlite_data, writer_handles) = persistence::initialize(ctx);
    timer.mark_interval_end("SQLITE_INITIALIZED");

    let persistence_writer = PersistenceWriter::new(writer_handles);

    let model_event_sender = persistence_writer.sender();

    let tips_handle = ctx.add_model(|_| user_defaults_on_startup.tips_data);
    let user_default_shell_unsupported_banner_model_handle =
        ctx.add_model(|_| user_defaults_on_startup.user_default_shell_unsupported_banner_state);
    // Extract the full-file parse error (if any) before the settings_file_error
    // value is moved below. Only FileParseFailed gates the broken-file guard
    // in `initialize_preferences_syncer`; InvalidSettings means TOML
    // parsed but individual values were wrong, which doesn't mean local
    // state is unusable.
    let startup_toml_parse_error_for_syncer = user_defaults_on_startup
        .settings_file_error
        .as_ref()
        .and_then(|err| match err {
            settings::SettingsFileError::FileParseFailed(msg) => Some(msg.clone()),
            settings::SettingsFileError::InvalidSettings(_) => None,
            // Unknown keys mean the file parsed and the loader was happy with
            // every value it did recognize, so local state is fine.
            settings::SettingsFileError::UnknownKeys(_) => None,
        });
    let settings_file_error = user_defaults_on_startup.settings_file_error;
    ctx.add_singleton_model(move |_ctx| {
        GlobalResourceHandlesProvider::new(GlobalResourceHandles {
            model_event_sender,
            tips_completed: tips_handle,
            user_default_shell_unsupported_banner_model_handle,
            settings_file_error,
        })
    });

    let (
        cloud_objects,
        cached_workspaces,
        current_workspace_uid,
        app_state,
        command_history,
        restored_user_profiles,
        time_of_next_force_object_refresh,
        object_actions,
        experiments,
        ai_queries,
        nld_prompts,
        persisted_workspaces,
        workspace_language_servers,
        multi_agent_conversations,
        persisted_projects,
        persisted_project_rules,
        persisted_ignored_suggestions,
        persisted_mcp_server_installations,
        mcp_servers_to_restore,
    ) = sqlite_data
        .map(|sqlite_data| {
            (
                sqlite_data.cloud_objects,
                sqlite_data.workspaces,
                sqlite_data.current_workspace_uid,
                Some(sqlite_data.app_state),
                sqlite_data.command_history,
                sqlite_data.user_profiles,
                sqlite_data.time_of_next_force_object_refresh,
                sqlite_data.object_actions,
                sqlite_data.experiments,
                sqlite_data.ai_queries,
                sqlite_data.nld_prompts,
                sqlite_data.codebase_indices,
                sqlite_data.workspace_language_servers,
                sqlite_data.multi_agent_conversations,
                sqlite_data.projects,
                sqlite_data.project_rules,
                sqlite_data.ignored_suggestions,
                sqlite_data.mcp_server_installations,
                sqlite_data.mcp_servers_to_restore,
            )
        })
        .unwrap_or_else(|| {
            // One `Default::default()` per binding above; adding a field to `PersistedData`
            // means adding one here too, or the tuple arms stop agreeing on arity.
            (
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
            )
        });

    // Initialize a global model to track server-side experiment state.
    // This depends on the [`GlobalResourceHandlesProvider`] and so it must
    // be initialized after it.
    ctx.add_singleton_model(|ctx| ServerExperiments::new_from_cache(experiments, ctx));

    ctx.add_singleton_model(AIRequestUsageModel::new);

    ctx.add_singleton_model(|_| UserWorkspaces::new(cached_workspaces, current_workspace_uid));

    // Initialize ApiKeyManager after UserWorkspaces so it can subscribe to workspace/settings changes
    ctx.add_singleton_model(|ctx| {
        #[cfg_attr(target_family = "wasm", allow(unused_mut))]
        let mut manager = ::ai::api_keys::ApiKeyManager::new(ctx);
        // Reload keys when another process writes them (see
        // `ai::tui_api_keys`). Safe here because `WarpManagedPathsWatcher` is
        // registered as a singleton earlier in this function. Unlike the pin
        // this is not gated on `LaunchMode::Tui`: the GUI and TUI share one
        // app id and therefore one keyring namespace, so a GUI process goes
        // stale on a TUI-side write exactly as a TUI process does.
        #[cfg(not(target_family = "wasm"))]
        manager.subscribe_to_tui_api_key_changes(ctx);
        #[cfg(not(target_family = "wasm"))]
        manager.subscribe_to_settings_changes(ctx);
        manager
    });

    // The API key for custom Agent Providers is stored in secure storage via its own
    // singleton, decoupled from ApiKeyManager (which forwards BYOK to warp-server).
    ctx.add_singleton_model(|ctx| {
        #[cfg_attr(target_family = "wasm", allow(unused_mut))]
        let mut secrets = crate::ai::agent_providers::AgentProviderSecrets::new(ctx);
        // This -- not `ApiKeyManager` -- is the store both of this fork's key
        // editors write (GUI Settings > AI, and the TUI's `/api-keys` picker),
        // and they share one keyring namespace, so it needs the same
        // cross-process reload as `ApiKeyManager` above.
        #[cfg(not(target_family = "wasm"))]
        secrets.subscribe_to_tui_api_key_changes(ctx);
        secrets
    });

    // Issue #72: the global HTTP proxy's Basic Auth password goes through the OS keychain.
    // Reapply immediately after registering, so the empty-string placeholder global slot
    // set during settings::init gets overwritten with the real password.
    ctx.add_singleton_model(crate::settings::network_secrets::ProxyCredentials::new);
    crate::settings::reapply_network_settings_preserving_password(ctx);
    // Subscribe to password changes (when the UI writes one) and re-push the global slot in sync.
    ctx.subscribe_to_model(
        &crate::settings::network_secrets::ProxyCredentials::handle(ctx),
        |_model, _event, ctx| {
            crate::settings::reapply_network_settings_preserving_password(ctx);
        },
    );

    ctx.add_singleton_model(AntivirusInfo::new);

    cfg_if::cfg_if! {
        if #[cfg(feature = "crash_reporting")] {
            let is_crash_reporting_enabled = crash_reporting::init(ctx);
        } else {
            let is_crash_reporting_enabled = false;
        }
    }
    for err in pre_init_errors {
        log::error!("pre-init error: {err:#}");
    }
    timer.mark_interval_end("INIT_CRASH_REPORTING");

    if let LaunchMode::App { .. } = launch_mode {
        autoupdate::check_and_report_update_errors(ctx);
    }

    ctx.set_fallback_font_source_provider(|url| ::asset_cache::url_source(url));

    ctx.set_default_binding_validator(is_binding_cross_platform);

    // On macOS this deletes `Contents/MacOS/old` from inside the installed app
    // bundle, so it runs behind the same `can_autoupdate` guard as the rest of
    // the autoupdate machinery: an execution mode that never autoupdates must
    // not mutate that bundle. The bundled CLI runs the GUI executable from
    // inside the app bundle, so without this it would rewrite a bundle it does
    // not own.
    if FeatureFlag::Autoupdate.is_enabled() && AppExecutionMode::as_ref(ctx).can_autoupdate() {
        // Before: remove_old_executable() was called synchronously to clean up the
        // old executable left over from the previous auto-update. Now: moved onto
        // background_executor — across platforms only macOS actually does anything
        // (Linux/Windows are no-ops), and even on macOS it's just fs::remove_dir_all,
        // with zero dependency on subsequent main-thread logic. Failure was already
        // just a log::error, so backgrounding it doesn't lose any information.
        ctx.background_executor()
            .spawn(async {
                if let Err(e) = autoupdate::remove_old_executable() {
                    log::error!("Failed to remove old executable: {e:?}");
                }
            })
            .detach();
    }

    experiments::init(ctx);

    // Initialize timestamp for session id and last active event
    App::record_last_active_timestamp();

    ctx.add_singleton_model(|_| SettingsPaneManager::new());
    ctx.add_singleton_model(|_| AIFactManager::new());
    ctx.add_singleton_model(|_| ExecutionProfileEditorManager::default());
    ctx.add_singleton_model(|_| pricing::PricingInfoModel::new());
    ctx.add_singleton_model(|_| {
        ManagedSecretManager::new(
            Arc::new(local_managed_secrets::DisabledManagedSecretsClient),
            auth_state.clone(),
        )
    });

    #[cfg(target_os = "macos")]
    if !launch_mode.is_headless() {
        AppearanceManager::as_ref(ctx).set_app_icon(ctx);
    }

    #[cfg(feature = "local_tty")]
    terminal::available_shells::register(ctx);

    // Add truly global actions that don't depend on the existence of any view here
    ctx.add_global_action("app:toggle_user_ps1", move |_args: &(), ctx| {
        SessionSettings::handle(ctx).update(ctx, |session_settings, ctx| {
            report_if_error!(session_settings.honor_ps1.toggle_and_save_value(ctx));
        });
    });
    ctx.add_global_action("app:toggle_copy_on_select", move |_args: &(), ctx| {
        SelectionSettings::handle(ctx).update(ctx, |selection_settings, ctx| {
            report_if_error!(selection_settings.copy_on_select.toggle_and_save_value(ctx));
        });
    });

    ctx.add_singleton_model(|_ctx| SyncedInputState::new());

    ctx.add_singleton_model(remote_server::manager::RemoteServerManager::new);
    // Client-side mirror of every remote host's codebase-index state, and the
    // thing that requests indexing when the user navigates into a remote git
    // repo. Registered right after the manager, as the pin does
    // (`02b53fcd8:app/src/lib.rs:1758`), because its constructor subscribes to
    // it; `CodeSettings`, its other subscription, is registered with the
    // settings groups well before this point.
    //
    // Without this registration the daemon leg is inert on the client: nothing
    // consumes `CodebaseIndexStatusesSnapshot` and nothing ever asks a daemon
    // to index.
    #[cfg(not(target_family = "wasm"))]
    ctx.add_singleton_model(remote_server::codebase_index_model::RemoteCodebaseIndexModel::new);
    // Zap Wave 6-1: the `remote_server::wire_auth_token_rotation(ctx)` call was physically
    // removed along with the server API token rotation event and the
    // `wire_auth_token_rotation` function itself.

    log::info!(
        "Starting warp with channel state {} and version {:?}",
        ChannelState::debug_str(),
        ChannelState::app_version()
    );

    // Teach our app that sometimes option means meta.
    ctx.set_event_munger(move |event, ctx| {
        let extra_meta_keys = *KeysSettings::as_ref(ctx).extra_meta_keys;
        apply_extra_meta_keys(event, extra_meta_keys);
        apply_scroll_multiplier(event, ctx);
    });

    // Rewrite recognized Zap web URLs (sessions, Drive, settings, home) into local
    // intent URLs when possible so they open directly in the desktop app.
    //
    // This callback is the last thing to touch a URL before the platform delegate hands it to
    // the OS, so the rewrite must never *escalate* a link: a rewrite that produced a `file:` or
    // a custom scheme would launder an otherwise-harmless link straight into an OS handler.
    // Only a rewrite whose result is still openable is used; anything else is discarded and the
    // original string passes through.
    //
    // Note the asymmetry, which is a real limitation and not an oversight: the callback returns
    // `String`, so it *cannot veto* an open. A URL that arrives here already carrying a
    // disallowed scheme is logged and passed through -- blocking has to happen at the call site,
    // as `NotebookLinks::resolve`/`open` now do for notebook content. Making this a genuine
    // app-wide chokepoint needs `set_before_open_url` to take a `-> Option<String>` handler,
    // which is a `warpui_core` change.
    ctx.set_before_open_url(|url_str, _ctx| {
        let Ok(url) = Url::parse(url_str) else {
            return url_str.to_owned();
        };

        if !is_openable_url_scheme(&url) {
            log::warn!(
                "Opening a URL whose scheme is outside the openable set: {:?}",
                url.scheme()
            );
        }

        match maybe_rewrite_web_url_to_intent(&url) {
            Some(intent) if is_openable_url_scheme(&intent) => intent.to_string(),
            Some(intent) => {
                log::warn!(
                    "Discarding web-URL rewrite that produced a non-openable scheme: {:?}",
                    intent.scheme()
                );
                url_str.to_owned()
            }
            None => url_str.to_owned(),
        }
    });

    ctx.set_a11y_verbosity(*AccessibilitySettings::as_ref(ctx).a11y_verbosity);

    #[cfg(enable_crash_recovery)]
    ctx.on_draw_frame_error(|ctx, window_id| {
        crash_recovery::CrashRecovery::handle(ctx).update(ctx, |crash_recovery, _ctx| {
            crash_recovery.on_draw_frame_error(window_id);
        });
    });

    let user_is_logged_in = auth_state.is_logged_in();

    if user_is_logged_in {
        // Zap's local auth facade already loads the identity snapshot during
        // `AuthState::initialize`. The startup phase no longer triggers an extra
        // cloud token refresh / auth refresh.

        // Set the first frame callback to record the app's startup time.
        // This is only sent for logged-in users so that new users don't skew performance metrics.
        let is_screen_reader_enabled = ctx.is_screen_reader_enabled();
        let from_relaunch = launch_mode.args().finish_update;
        ctx.on_first_frame_drawn(move |ctx| {
            let timing_data = IntervalTimer::handle(ctx).update(ctx, |timer, _| {
                timer.mark_interval_end("FIRST_FRAME_DRAWN");
                // Local tuning escape hatch: when WARP_STARTUP_TRACE=1, print the full
                // startup timing table to stderr. Doesn't affect telemetry logic; for
                // developer use only.
                timer.print_trace_to_stderr_if_enabled();
                timer.compute_stats()
            });
            let event = TelemetryEvent::AppStartup(AppStartupInfo {
                is_session_restoration_on: user_defaults_on_startup.should_restore_session,
                is_screen_reader_enabled,
                from_relaunch,
                is_crash_reporting_enabled,
                timing_data,
            });

            GPUState::handle(ctx).update(ctx, |gpu_state, ctx| {
                gpu_state
                    .set_has_lower_power_gpu(warpui::rendering::is_low_power_gpu_available(), ctx);
            });

            for window_id in ctx.window_ids().collect_vec() {
                SettingsPaneManager::handle(ctx)
                    .read(ctx, |model, _| model.settings_view(window_id))
                    .update(ctx, |settings, ctx| {
                        settings.refresh_preferred_graphics_backend_dropdown(ctx);
                    })
            }

            send_telemetry_from_app_ctx!(event, ctx);
        });

        #[cfg(enable_crash_recovery)]
        ctx.on_frame_drawn(|ctx, window_id| {
            crash_recovery::CrashRecovery::handle(ctx).update(ctx, |crash_recovery, ctx| {
                crash_recovery.on_frame_drawn(window_id, ctx);
            });
        })
    } else {
        // If the app was opened while logged out, record an event for measuring new users.
        // This is sent immediately in case they quit the app on the signup screen.
        send_telemetry_sync_from_app_ctx!(TelemetryEvent::LoggedOutStartup, ctx);
        // Logged-out users (the majority in the BYOP scenario) also need to be able to
        // view startup timing, so print the WARP_STARTUP_TRACE table once after the
        // first frame. Sends no telemetry and doesn't affect logic.
        ctx.on_first_frame_drawn(move |ctx| {
            IntervalTimer::handle(ctx).update(ctx, |timer, _| {
                timer.mark_interval_end("FIRST_FRAME_DRAWN");
                timer.print_trace_to_stderr_if_enabled();
            });
        });
    }

    #[cfg(not(target_family = "wasm"))]
    {
        ctx.add_singleton_model(DirectoryWatcher::new);
        // Register the skill provider directories as force-included paths so
        // the gitignore-pruning watch descend filter still watches gitignored
        // skill directories (e.g. `.agents/skills`) for `Repository`
        // subscribers (LSP, MCP). Registered before any repository begins
        // watching so it gates descent on the very first registration.
        DirectoryWatcher::handle(ctx).update(ctx, |watcher, _| {
            watcher.register_force_included_paths(
                ::ai::skills::SKILL_PROVIDER_DEFINITIONS
                    .iter()
                    .map(|provider| provider.skills_path.clone()),
            );
        });
        ctx.add_singleton_model(|_| DetectedRepositories::default());
        if let Some(home_dir) = dirs::home_dir() {
            ctx.add_singleton_model(|ctx| HomeDirectoryWatcher::new(home_dir, ctx));
        } else {
            log::info!("Home directory not found; skipping HomeDirectoryWatcher registration");
        }
    }

    #[cfg(feature = "local_fs")]
    {
        let imported_config_model = ctx.add_singleton_model(ImportedConfigModel::new);

        if FeatureFlag::SettingsImport.is_enabled()
            && ChannelState::channel() != warp_core::channel::Channel::Integration
        {
            imported_config_model.update(ctx, |model, ctx| {
                model.search_for_settings_to_import(ctx);
            });
        }

        ctx.add_singleton_model(|ctx| {
            let model = RepoMetadataModel::new(ctx);

            // Force-include project skill-provider directories even when
            // gitignored, and register them as standing-query targets so the
            // skill watcher's `StandingQueryResultsUpdated` subscription
            // (app/src/ai/skills/file_watchers/skill_watcher.rs) sees skill
            // files as soon as they're discovered, without waiting on a full
            // `RepositoryUpdated` tree rebuild.
            model.register_force_included_paths(
                ::ai::skills::SKILL_PROVIDER_DEFINITIONS
                    .iter()
                    .map(|provider| provider.skills_path.clone()),
                ctx,
            );
            model.set_project_skill_provider_paths(
                ::ai::skills::SKILL_PROVIDER_DEFINITIONS
                    .iter()
                    .map(|provider| provider.skills_path.clone()),
                ctx,
            );

            // Subscribe to RemoteServerManager push events so that remote repo
            // metadata snapshots and incremental updates populate the remote
            // sub-model and trigger RepoMetadataEvent emissions.
            {
                use remote_server::manager::{RemoteServerManager, RemoteServerManagerEvent};
                let mgr = RemoteServerManager::handle(ctx);
                ctx.subscribe_to_model(&mgr, |me, event, ctx| match event {
                    RemoteServerManagerEvent::RepoMetadataSnapshot { host_id, update } => {
                        me.insert_remote_snapshot(host_id.clone(), update, ctx);
                    }
                    RemoteServerManagerEvent::RepoMetadataUpdated { host_id, update }
                    | RemoteServerManagerEvent::RepoMetadataDirectoryLoaded { host_id, update } => {
                        me.apply_remote_incremental_update(host_id, update, ctx);
                    }
                    RemoteServerManagerEvent::HostDisconnected { host_id } => {
                        me.remove_remote_repositories_for_host(host_id, ctx);
                    }
                    _ => {}
                });
            }

            model
        });
    }

    {
        use code_review::git_status_update::GitStatusUpdateModel;
        ctx.add_singleton_model(|_| GitStatusUpdateModel::new());
    }

    ctx.add_singleton_model(|ctx| {
        ProjectManagementModel::new(persisted_projects, persistence_writer.sender(), ctx)
    });

    ctx.add_singleton_model(move |_| History::new(command_history));

    ctx.add_singleton_model(CustomSecretRegexUpdater::new);

    // Register initial keybindings prior to creating menus
    ai::init(ctx);
    app_services::init(ctx);
    // // TODO: Temporarily disabling keybindings for WASM builds. Will be implemented in future WASM support.
    #[cfg(not(target_family = "wasm"))]
    code::editor::find::view::init(ctx);
    workspace::init(ctx);
    pane_group::init(ctx);
    terminal::init(ctx);
    input::init(ctx);
    editor::init(ctx);
    onboarding::set_localizer(|key| crate::i18n::t_or(key, key));
    onboarding::init(ctx);
    menu::init(ctx);
    tips::tip_view::init(ctx);
    launch_configs::init(ctx);
    workflows::init(ctx);
    notifications::init(ctx);
    themes::theme_chooser::init(ctx);
    themes::theme_creator_modal::init(ctx);
    themes::theme_deletion_modal::init(ctx);
    root_view::init(ctx);
    voltron::init(ctx);
    auth::init(ctx);
    crate::view_components::find::init(ctx);
    prompt::editor_modal::init(ctx);
    ai::blocklist::agent_view::editor::init(ctx);
    undo_close::init(ctx);
    tab_configs::new_worktree_modal::init(ctx);
    tab_configs::params_modal::init(ctx);
    ai::blocklist::init(ctx);
    ai::blocklist::block::status_bar::init(ctx);
    drive::index::init(ctx);
    ai_assistant::panel::init(ctx);
    // Zap Wave 7-2: `settings_view::update_environment_form::init` was physically removed
    // along with the cloud ambient agent's main subsystem.
    env_vars::env_var_collection_block::init(ctx);
    terminal::ssh::install_tmux::init(ctx);
    terminal::ssh::warpify::init(ctx);
    terminal::ssh::error::init(ctx);
    context_chips::display_menu::init(ctx);
    context_chips::node_version_popup::init(ctx);
    env_vars::view::env_var_collection::init(ctx);
    ai::agent::todos::popup::init(ctx);
    coding_entrypoints::project_buttons::init(ctx);
    if FeatureFlag::CodeReviewSaveChanges.is_enabled() {
        code_review::init(ctx);
    }

    timer.mark_interval_end("SUBSYSTEM_INITS_DONE");

    // Register the CLI agent installation status model (asynchronously scans PATH in the background, then auto-syncs per-agent settings once done).
    ctx.add_singleton_model(crate::terminal::cli_agent::CLIAgentInstallModel::new);

    let display_count = ctx.windows().display_count();
    ctx.add_singleton_model(|_| DisplayCount(display_count));

    ctx.add_singleton_model(|_| RelaunchModel::new());
    ctx.add_singleton_model(|_| ChangelogModel::new(update_http_client.clone()));
    ctx.add_singleton_model(|_| GitHubAuthNotifier::new());
    ctx.add_singleton_model(|_| NetworkStatus::new());
    ctx.add_singleton_model(|_| SystemStats::new());
    ctx.add_singleton_model(|_| KeybindingChangedNotifier::new());
    ctx.add_singleton_model(|_| search::command_palette::SelectedItems::new());
    ctx.add_singleton_model(search::files::model::FileSearchModel::new);
    ctx.add_singleton_model(|_| VimRegisters::new());
    ctx.add_singleton_model(UndoCloseStack::new);
    ctx.add_singleton_model(|_| ToastStack);
    ctx.add_singleton_model(|_| GlobalCodeReviewModel);
    ctx.add_singleton_model(workspace::OneTimeModalModel::new);
    ctx.add_singleton_model(
        workspace::bonus_grant_notification_model::BonusGrantNotificationModel::new,
    );
    #[cfg(feature = "local_fs")]
    ctx.add_singleton_model(FileModel::new);
    ctx.add_singleton_model(|ctx| {
        let model = GlobalBufferModel::new(ctx);
        // Client app: subscribe to RemoteServerManager's buffer push events. The daemon
        // doesn't do this step (it doesn't register RemoteServerManager), so this can't
        // be moved into GlobalBufferModel::new. RemoteServerManager was already
        // registered earlier.
        #[cfg(feature = "local_tty")]
        if FeatureFlag::SshRemoteServer.is_enabled() {
            GlobalBufferModel::subscribe_to_remote_server_manager(ctx);
        }
        model
    });
    #[cfg(windows)]
    ctx.add_singleton_model(util::traffic_lights::windows::RendererState::new);
    #[cfg(feature = "local_fs")]
    ctx.add_singleton_model(|_| LanguageServerShutdownManager::new());

    #[cfg(feature = "voice_input")]
    ctx.add_singleton_model(voice_input::VoiceInput::new);
    ctx.add_singleton_model(|_| {
        // Zap (localization, Phase 4): the default used to inject `ServerVoiceTranscriber`,
        // which went through cloud Wispr STT. Cloud voice transcription isn't available in
        // the localized scenario, so this now uses `disabled()`, making the higher-level
        // `transcriber()` return None. The voice-input UI now only captures audio without
        // transcribing it (a local STT can be wired in later).
        VoiceTranscriber::disabled()
    });

    timer.mark_interval_end("CORE_SINGLETONS_REGISTERED");

    let notebooks = cloud_objects
        .iter()
        .filter_map(|object| {
            let notebook: Option<&NotebookObject> = object.into();
            notebook
        })
        .cloned()
        .collect::<Vec<_>>();

    let object_store_model = ctx.add_singleton_model(|_ctx| {
        ObjectStoreModel::new(
            persistence_writer.sender(),
            cloud_objects,
            time_of_next_force_object_refresh,
        )
    });

    // Zap (Wave 4): after SyncQueue was fully removed, there is no more
    // `unsynced_actions` / `objects_with_pending_changes` tracking; a local write is
    // now considered "done".
    let _ = (&object_store_model, &object_actions);
    // The `ObjectTypeAndId` import is kept so other modules in the same crate can access it via the `crate::` path.
    let _: Option<ObjectTypeAndId> = None;

    timer.mark_interval_end("CLOUD_MODEL_INITIALIZED");

    {
        let conversations = &multi_agent_conversations;
        // #256 item 2 / #312: the NLD-classification consumer (`input_model.rs`'s
        // `detect_and_set_input_type`, gated on `FeatureFlag::NldPromptHistoryMatch`) is now fed
        // by a real snapshot. `nld_prompts` comes from `sqlite.rs`'s startup read, which is
        // itself gated on the same flag -- off by default here and at the pin -- so this is an
        // empty vec in a default build, exactly as the hard-coded `vec![]` was, and non-empty
        // the moment the flag is turned on.
        //
        // The comment this replaces said the read was "superseded by #336/#337/#331". That was
        // a misread: those three (fork_conversation preserve_task_ids, the persisted summary
        // column, the ask-user-question speedbump setting) supersede items 1/3/4 of #256, not
        // its item 2, and all three are closed. Nothing superseded this read; it was simply
        // unported.
        ctx.add_singleton_model(move |_| {
            BlocklistAIHistoryModel::new(ai_queries, nld_prompts, conversations)
        });
    }
    // Seed the orchestration pin set from persisted conversation data before
    // `multi_agent_conversations` is consumed by `RestoredAgentConversations::new`
    // below. Each conversation's `AgentConversationData.pinned` is the source of
    // truth; the singleton mirrors them in memory for fast cross-pane lookups.
    let initial_pinned_conversations: HashSet<AIConversationId> = multi_agent_conversations
        .iter()
        .filter_map(|conv| {
            let data = serde_json::from_str::<crate::persistence::model::AgentConversationData>(
                &conv.conversation.conversation_data,
            )
            .ok()?;
            if !data.pinned {
                return None;
            }
            AIConversationId::try_from(conv.conversation.conversation_id.clone()).ok()
        })
        .collect();
    // Cross-pane UI state for the orchestration pill bar. Registered after
    // the history model since it subscribes to history events.
    ctx.add_singleton_model(move |ctx| {
        OrchestrationPillBarModel::new(initial_pinned_conversations, ctx)
    });
    // Per-conversation queued prompts. Registered after the history model since it subscribes to
    // history events for cleanup.
    ctx.add_singleton_model(ai::blocklist::QueuedQueryModel::new);
    {
        let (restored, failed_to_restore) =
            RestoredAgentConversations::new(multi_agent_conversations);
        // Clean up persisted conversations that can't be converted from sqlite, to avoid retrying and logging a warning on every startup.
        if !failed_to_restore.is_empty() {
            if let Some(sender) =
                crate::global_resource_handles::GlobalResourceHandlesProvider::as_ref(ctx)
                    .get()
                    .model_event_sender
                    .as_ref()
            {
                if let Err(e) = sender.send(
                    crate::persistence::ModelEvent::DeleteMultiAgentConversations {
                        conversation_ids: failed_to_restore,
                    },
                ) {
                    log::error!(
                        "Failed to purge unconvertible persisted conversations from sqlite: {e:?}"
                    );
                }
            }
        }
        ctx.add_singleton_model(move |_| restored);
    }
    ctx.add_singleton_model(|_| CLIAgentSessionsModel::new());
    ctx.add_singleton_model(BlocklistAIPermissions::new);
    // Notification center singleton model: must be registered after BlocklistAIHistoryModel
    // and CLIAgentSessionsModel, because its constructor subscribes to both models.
    ctx.add_singleton_model(crate::notifications::model::NotificationsModel::new);

    // Gated the way the pin gates it (`02b53fcd8:app/src/lib.rs:2134`): a launch
    // mode that cannot index still gets the singleton — several callers reach
    // for it unconditionally — but with indexing off.
    if launch_mode.supports_indexing() {
        ctx.add_singleton_model(RepoOutlines::new);
    } else {
        ctx.add_singleton_model(|ctx| RepoOutlines::new_with_indexing_enabled(false, ctx));
    }

    ctx.add_singleton_model(|_| UserProfiles::new(restored_user_profiles));

    ctx.add_singleton_model(|_| ObjectActions::new(object_actions));

    ctx.add_singleton_model(|_| AudibleBell::new());

    // Zap: UpdateManager is now only responsible for local cloud-object in-memory/SQLite sync; a cloud client is no longer injected.
    ctx.add_singleton_model(|ctx| UpdateManager::new(persistence_writer.sender(), ctx));

    let toml_file_path = settings::user_preferences_toml_file_path();
    // Zap (localization, Phase 5): `PreferencesSyncer` has been physically removed. The
    // old syncer handled two-way sync between local settings.toml and cloud preferences;
    // in the localized scenario, only local toml loading is kept.
    let _ = toml_file_path;
    let _ = startup_toml_parse_error_for_syncer;

    // LogManager must be registered before any subsystem (e.g. MCP, LSP) that creates file-based loggers.
    ctx.add_singleton_model(|_| simple_logger::manager::LogManager::new());

    let running_mcp_servers = app_state
        .as_ref()
        .map(|app_state| app_state.running_mcp_servers.as_slice())
        .unwrap_or(&[]);

    // FileMCPWatcher must be registered before FileBasedMCPManager, which subscribes to it.
    ctx.add_singleton_model(FileMCPWatcher::new);
    ctx.add_singleton_model(FileBasedMCPManager::new);

    // TemplatableMCPServerManager must be registered after UpdateManager and MCPServerManager so it can migrate legacy MCPs on start up
    // It should also be registered after FileBasedMCPManager so it can receive file-based server updates.
    ctx.add_singleton_model(|ctx| {
        TemplatableMCPServerManager::new(
            persisted_mcp_server_installations,
            mcp_servers_to_restore,
            running_mcp_servers,
            ctx,
        )
    });

    // MCPGalleryManager subscribes to UpdateManager so that it can be notified when gallery items are updated locally.
    // The registration of this singleton must be after UpdateManager is registered.
    ctx.add_singleton_model(MCPGalleryManager::new);

    // SkillManager is used to cache SKILL.md files for all active terminal views and their working directories
    ctx.add_singleton_model(SkillManager::new);

    // RemoteAgentContext reconciles connected SSH hosts' `RemoteAgentContextSnapshot`
    // pushes into SkillManager's per-host bundled/home-skill catalogs (#487). Registered
    // after both SkillManager and RemoteServerManager, which it subscribes to and updates.
    #[cfg(all(not(target_family = "wasm"), feature = "local_fs"))]
    ctx.add_singleton_model(crate::ai::remote_agent_context::RemoteAgentContext::new);

    // ObjectStoreViewModel subscribes to UpdateManager so that it can be notified when objects are
    // created or mutated in the local object store.
    ctx.add_singleton_model(ObjectStoreViewModel::new);

    // AIDocumentModel subscribes to UpdateManager so that it can be notified when notebooks are created locally.
    ctx.add_singleton_model(AIDocumentModel::new);

    // AgentConversationsModel subscribes to UpdateManager events that still flow through the local updater.
    ctx.add_singleton_model(AgentConversationsModel::new);

    // ByoLlmAuthBannerSessionState tracks dismissal of the BYO LLM auth banner (e.g., AWS Bedrock login).
    ctx.add_singleton_model(ByoLlmAuthBannerSessionState::new);

    ctx.add_singleton_model(ExportManager::new);
    ctx.add_singleton_model(|ctx| NotebookManager::new(notebooks, ctx));
    ctx.add_singleton_model(|_| CodeManager::default());
    ctx.add_singleton_model(|_| OpenedFilesModel::new());
    ctx.add_singleton_model(NotebookKeybindings::new);
    ctx.add_singleton_model(TerminalKeybindings::new);
    ctx.add_singleton_model(|_| ActiveSession::default());
    // Zap (localization, Phase 2d-4a-1): the old `Listener` singleton handled the cloud
    // cloud_objects RTC WebSocket. After 2b-1, `start_listener` was already a no-op, so
    // this pass physically removed the entire file along with the singleton registration.

    #[cfg(all(not(target_family = "wasm"), feature = "local_tty"))]
    {
        ctx.add_singleton_model(LocalShellState::new);
        ctx.add_singleton_model(system::SystemInfo::new);
    }

    // Add a singleton model that holds the current prompt configuration.
    ctx.add_singleton_model(Prompt::new);

    // Add a singleton model for resizable modals whose size should be persisted through restarts.
    ctx.add_singleton_model(|_| ResizableData::default());

    ctx.add_singleton_model(EnvVarCollectionManager::new);
    ctx.add_singleton_model(WorkflowManager::new);

    AutoupdateState::register(ctx, update_http_client.clone());

    ctx.add_singleton_model(LocalWorkflows::new);

    ctx.add_singleton_model(LLMPreferences::new);

    ctx.add_singleton_model(|ctx| {
        ai::agent_tips::AITipModel::<ai::AgentTip>::new_for_agent_tips(ctx)
    });

    timer.mark_interval_end("SINGLETON_MODELS_REGISTERED");

    ctx.add_singleton_model(move |_| timer);

    let is_ssh_tmux_wrapper_enabled = ctx
        .private_user_preferences()
        .read_value("SshTmuxWrapperOverride")
        .ok()
        .flatten()
        .and_then(|s| s.parse().ok());

    if let Some(is_ssh_tmux_wrapper_enabled) = is_ssh_tmux_wrapper_enabled {
        FeatureFlag::SSHTmuxWrapper.set_user_preference(is_ssh_tmux_wrapper_enabled);
    }

    ctx.add_singleton_model(|ctx| AIExecutionProfilesModel::new(launch_mode, ctx));

    ctx.add_singleton_model(DefaultTerminal::new);

    // The index manager and `PersistedWorkspace` both restore from the same rows,
    // and `add_singleton_model` takes ownership of what its closure captures, so
    // the manager gets its own copy.
    #[cfg(feature = "local_fs")]
    let persisted_workspaces_for_index = persisted_workspaces.clone();
    // Resolved outside the closure so it does not borrow `launch_mode`.
    #[cfg(feature = "local_fs")]
    let launch_mode_supports_indexing = launch_mode.supports_indexing();

    // Codebase embedding index. Restored with the subsystem (Delta D2c);
    // registered before `PersistedWorkspace`, which subscribes to it.
    //
    // Differences from the pin's registration
    // (`02b53fcd8:app/src/lib.rs:2380`), all forced by things this fork does not
    // have:
    //
    // * The store client is `crate::ai::codebase_embeddings::build_store_client`
    //   — a local vector store plus the user's own embedding provider — where
    //   the pin passed `server_api_provider.as_ref(ctx).get()`. Because that
    //   store writes through the app's persistence channel, it takes
    //   `persistence_writer.sender()` here: `PersistenceWriter` is not
    //   registered as a singleton until further down, so a client that looked
    //   it up in the context panicked on every startup.
    // * `daemon_codebase_index_snapshot_storage` has no counterpart here: this
    //   fork's `LaunchMode::RemoteServerDaemon` never reaches `initialize_app`
    //   (see `LaunchMode::supports_indexing`). The daemon registers its own
    //   manager, with its own snapshot storage and its own store client, in
    //   `remote_server::run_daemon_app`.
    //
    // A launch mode that does not support indexing still gets a manager, one
    // that no-ops, rather than no manager at all: `PersistedWorkspace` and the
    // settings page both call `CodebaseIndexManager::handle(ctx)`
    // unconditionally, and an unregistered singleton panics.
    #[cfg(feature = "local_fs")]
    ctx.add_singleton_model(|ctx| {
        let indexing_enabled =
            launch_mode_supports_indexing && FeatureFlag::FullSourceCodeEmbedding.is_enabled();
        let should_restore_indices = indexing_enabled
            && UserWorkspaces::as_ref(ctx).is_codebase_context_enabled(ctx);
        let indices_to_restore = if should_restore_indices {
            persisted_workspaces_for_index.clone()
        } else {
            vec![]
        };

        let codebase_limits = AIRequestUsageModel::as_ref(ctx).codebase_context_limits();
        let codebase_index_config = CodebaseIndexManagerConfig::new(
            indices_to_restore,
            codebase_limits.max_indices_allowed,
            codebase_limits.max_files_per_repo,
            codebase_limits.embedding_generation_batch_size,
            crate::ai::codebase_embeddings::build_store_client(ctx, persistence_writer.sender()),
            indexing_enabled,
        );

        CodebaseIndexManager::new_with_config(codebase_index_config, ctx)
    });

    // The consumer for that index. Registered unconditionally -- every build carries
    // the handle type on `RequestParams`, and an unregistered singleton panics on
    // access -- but it must come *after* the manager above, because on `local_fs` it
    // subscribes to it during construction.
    ctx.add_singleton_model(crate::ai::codebase_retrieval::CodebaseRetrievalController::new);

    // Hand the remote-server manager what a daemon needs to index on its own
    // host, and refresh it whenever the user edits their providers or changes
    // their mind about codebase indexing. Registered here, after `AISettings`
    // and `AIRequestUsageModel`, because `remote_client_preferences` reads
    // both; `RemoteServerManager` itself was registered much earlier, with no
    // sessions yet, so nothing has connected in between.
    //
    // Both settings groups are subscribed because the credential gate inside
    // `remote_client_preferences` is `should_use_codebase_indexing`, and that
    // reads `UserWorkspaces::is_codebase_context_enabled` = the global AI
    // toggle (`AISettings`) AND `CodeSettings::codebase_context_enabled`.
    // Subscribing to only one of them would leave the other half of the
    // predicate without its invalidation event: turning codebase indexing off
    // would not retract the API key already sent to connected daemons, and
    // turning it on would not deliver one until the next restart — so the
    // disclosure in `settings-code-remote-indexed-folders-desc` ("sent when you
    // turn this on") would be false in both directions.
    // `update_client_preferences` pushes to every connected client and no-ops
    // when nothing changed, so subscribing to every settings event is cheap and
    // the retraction reaches live sessions.
    #[cfg(all(feature = "local_fs", not(target_family = "wasm")))]
    {
        use crate::ai::codebase_embeddings::remote_client_preferences;
        use remote_server::manager::RemoteServerManager;

        fn refresh_remote_client_preferences(ctx: &mut AppContext) {
            let preferences = remote_client_preferences(ctx);
            RemoteServerManager::handle(ctx).update(ctx, |manager, _| {
                manager.update_client_preferences(preferences);
            });
        }

        refresh_remote_client_preferences(ctx);
        ctx.subscribe_to_model(&crate::settings::AISettings::handle(ctx), |_, _, ctx| {
            refresh_remote_client_preferences(ctx);
        });
        ctx.subscribe_to_model(&crate::settings::CodeSettings::handle(ctx), |_, _, ctx| {
            refresh_remote_client_preferences(ctx);
        });
    }

    ctx.add_singleton_model(|ctx| {
        ProjectContextModel::new_from_persisted(persisted_project_rules, ctx)
    });
    // Index global rules (e.g. ~/.agents/AGENTS.md) on a background task so
    // they are available to subsequent agent queries. #575.
    ProjectContextModel::handle(ctx).update(ctx, |me, ctx| me.index_global_rules(ctx));
    ctx.add_singleton_model(|ctx| {
        crate::ai::project_rules_persister::ProjectRulesPersister::new(
            persistence_writer.sender(),
            ctx,
        )
    });
    // Recently-used repository roots. Registered after `ProjectContextModel`
    // because `user_added_workspace` drives a project-rules scan through it, and
    // after `RepoMetadataModel` for the same reason.
    ctx.add_singleton_model(|ctx| {
        PersistedWorkspace::new(
            persisted_workspaces,
            workspace_language_servers,
            persistence_writer.sender(),
            ctx,
        )
    });
    ctx.add_singleton_model(move |_| persistence_writer);

    ctx.add_singleton_model(input_classifier::InputClassifierModel::new);

    ctx.add_singleton_model(move |_| IgnoredSuggestionsModel::new(persisted_ignored_suggestions));

    // Subscribe WorkflowAliases to the UpdateManager so that it can be notified when objects are
    // trashed.
    WorkflowAliases::handle(ctx).update(ctx, |aliases, ctx| {
        aliases.connect(ctx);
    });

    // When running natively, add the http server singleton to the application.
    #[cfg(not(target_family = "wasm"))]
    ctx.add_singleton_model(move |ctx| {
        let routers = vec![
            app_installation_detection::make_router(),
            profiling::make_router(),
        ];
        http_server::HttpServer::new(routers, ctx)
    });

    #[cfg(feature = "local_fs")]
    if matches!(
        launch_mode,
        LaunchMode::App { .. } | LaunchMode::Test { .. }
    ) && FeatureFlag::WarpControlCli.is_enabled()
    {
        ctx.add_singleton_model(local_control::LocalControlBridge::new);
        ctx.add_singleton_model(local_control::LocalControlServer::new);
    }

    app_state
}

fn app_callbacks(is_integration_test: bool) -> warpui::platform::AppCallbacks {
    warpui::platform::AppCallbacks {
        on_internet_reachability_changed: Some(Box::new(move |reachable, ctx| {
            NetworkStatus::handle(ctx)
                .update(ctx, move |me, ctx| me.reachability_changed(reachable, ctx));
        })),
        on_become_active: None,
        on_screen_changed: Some(Box::new(move |ctx| {
            ctx.dispatch_global_action(
                "root_view:move_quake_mode_window_from_screen_change",
                &KeysSettings::as_ref(ctx)
                    .quake_mode_settings
                    .value()
                    .clone(),
            );

            let new_display_count = ctx.windows().display_count();
            DisplayCount::handle(ctx).update(ctx, |display_count, ctx| {
                display_count.0 = new_display_count;
                ctx.notify();
            });
        })),
        on_cpu_awakened: Some(Box::new(move |ctx| {
            SystemStats::handle(ctx).update(ctx, move |system, ctx| {
                log::info!("System has returned from sleep");
                system.dispatch_cpu_was_awakened(ctx);
            });
        })),
        on_cpu_will_sleep: Some(Box::new(move |ctx| {
            SystemStats::handle(ctx).update(ctx, move |system, ctx| {
                log::info!("System is going to sleep...");
                system.dispatch_cpu_will_sleep(ctx);
            });
        })),
        on_resigned_active: Some(Box::new(move |ctx| {
            let active_window_id = ctx.windows().active_window();
            let update_quake_mode_arg = UpdateQuakeModeEventArg { active_window_id };

            #[cfg(feature = "voice_input")]
            {
                if let voice_input::VoiceInputState::Listening { enabled_from, .. } =
                    voice_input::VoiceInput::as_ref(ctx).state()
                {
                    // Abort the voice input if it's toggled from a key press, as we cannot listen to key events
                    // if the user is focused on a different app - we could miss the release of the key.
                    if matches!(
                        *enabled_from,
                        voice_input::VoiceInputToggledFrom::Key { .. }
                    ) {
                        ctx.dispatch_global_action("root_view:abort_voice_input", &());
                    }
                }
            }
            ctx.dispatch_global_action("root_view:update_quake_mode_state", &update_quake_mode_arg);
        })),
        on_will_terminate: Some(Box::new(move |ctx| {
            NotebookManager::handle(ctx).update(ctx, |manager, ctx| {
                // Notebooks are only saved periodically, so ensure that any pending changes have
                // been sent to the writer thread before terminating.
                manager.close_notebooks(ctx);
            });

            PersistenceWriter::handle(ctx).update(ctx, |writer, _ctx| {
                writer.terminate();
            });

            // We want to tear down the terminal server before relaunching for
            // autoupdate, to ensure we're not running any extra Zap processes
            // when we bring up the new process.  Additionally, this must occur
            // after terminating the persistence writer, so we don't keep track
            // of the fact that the shell sessions terminated.
            #[cfg(feature = "local_tty")]
            terminal::local_tty::spawner::PtySpawner::handle(ctx).update(ctx, |pty_spawner, _| {
                pty_spawner.prepare_for_app_termination();
            });

            #[cfg(all(feature = "local_tty", windows))]
            terminal::local_tty::shutdown_all_pty_event_loops(ctx);

            // Tear down app services before spawning the new process, to
            // ensure that the new process doesn't find the old process while
            // attempting to enforce our single-instance policy on Linux.
            app_services::teardown(ctx);
            autoupdate::spawn_child_if_necessary(ctx);

            // Tear down any application profilers that are running, writing
            // results to disk.
            profiling::teardown();

            #[cfg(enable_crash_recovery)]
            crash_recovery::CrashRecovery::handle(ctx).update(ctx, |crash_recovery, _ctx| {
                crash_recovery.teardown();
            });

            // Tear down crash reporting as the last thing we do before the application
            // terminates.
            #[cfg(feature = "crash_reporting")]
            crash_reporting::uninit_crash_reporting();
        })),
        on_should_close_window: Some(Box::new(move |window_id, ctx| {
            let general_settings = GeneralSettings::as_ref(ctx);
            // On Linux or Windows, if we're about to close the final window, we should quit the app instead.
            // On Mac, we do this conditionally based on a user setting.
            let quit_on_last_window_closed =
                cfg!(any(target_os = "linux", target_os = "freebsd", windows))
                    || *general_settings.quit_on_last_window_closed;
            if ctx.window_ids().count() == 1 && quit_on_last_window_closed {
                log::info!("No windows left, terminating app");
                ctx.terminate_app(TerminationMode::Cancellable, None);
                return ApproveTerminateResult::Cancel;
            }

            let summary = UnsavedStateSummary::for_window(window_id, ctx);

            send_telemetry_from_app_ctx!(
                TelemetryEvent::UserInitiatedClose {
                    initiated_on: CloseTarget::Window,
                },
                ctx
            );

            // Don't show dialog on integration test. Machine can't press buttons.
            if !is_integration_test && summary.should_display_warning(ctx) {
                let shown = summary
                    .dialog()
                    .on_confirm(move |ctx| {
                        ctx.windows()
                            .close_window(window_id, TerminationMode::ForceTerminate);
                    })
                    .on_cancel(move |ctx| {
                        on_close_window_cancelled(window_id, false, ctx);
                    })
                    .on_show_processes(move |ctx| {
                        on_close_window_cancelled(window_id, true, ctx);
                    })
                    .show(ctx);
                if shown {
                    ApproveTerminateResult::Cancel
                } else {
                    ApproveTerminateResult::Terminate
                }
            } else {
                ApproveTerminateResult::Terminate
            }
        })),
        on_should_terminate_app: Some(Box::new(move |ctx| {
            send_telemetry_from_app_ctx!(
                TelemetryEvent::UserInitiatedClose {
                    initiated_on: CloseTarget::App,
                },
                ctx
            );

            // If there's a pending autoupdate, apply that before showing the unsaved changes
            // dialog. We apply the update first so that the dialog can force-terminate.
            let applying_update = autoupdate::apply_pending_update(ctx, |ctx| {
                // Once the deferred update is applied, re-terminate the app. This termination is
                // cancellable so that we still show the unsaved changes dialog.
                log::info!("Deferred autoupdate applied, terminating app");
                ctx.terminate_app(TerminationMode::Cancellable, None);
            });
            if applying_update {
                return ApproveTerminateResult::Cancel;
            }

            let summary = UnsavedStateSummary::for_app(ctx);
            // Don't show dialog on integration test. Machine can't press buttons.
            if !is_integration_test && summary.should_display_warning(ctx) {
                let shown = summary
                    .dialog()
                    .on_confirm(|ctx| ctx.terminate_app(TerminationMode::ForceTerminate, None))
                    .on_show_processes(|ctx| on_close_app_cancelled(true, ctx))
                    .on_cancel(|ctx| on_close_app_cancelled(false, ctx))
                    .show(ctx);
                if shown {
                    return ApproveTerminateResult::Cancel;
                }
            }

            ApproveTerminateResult::Terminate
        })),
        on_disable_warning_modal: Some(Box::new(move |ctx| {
            GeneralSettings::handle(ctx).update(ctx, |general_settings, ctx| {
                report_if_error!(general_settings
                    .show_warning_before_quitting
                    .toggle_and_save_value(ctx));
            });
            send_telemetry_from_app_ctx!(TelemetryEvent::QuitModalDisabled, ctx);
        })),
        on_notification_clicked: Some(Box::new(move |notification_response, ctx| {
            if let Some(notification_data) = notification_response.data() {
                let context: serde_json::Result<NotificationContext> =
                    serde_json::from_str(notification_data);
                if let Ok(NotificationContext::BlockOrigin {
                    window_id,
                    pane_group_id,
                    pane_id,
                }) = context
                {
                    // Ensure the window ID exists, if so dispatch an action to focus
                    // the correct pane.
                    if ctx.window_ids().contains(&window_id) {
                        if let Some(root_view_id) = ctx.root_view_id(window_id) {
                            ctx.dispatch_action(
                                window_id,
                                &[root_view_id],
                                "root_view:handle_notification_click",
                                &PaneViewLocator {
                                    pane_group_id,
                                    pane_id,
                                },
                                log::Level::Info,
                            );
                        }
                    }
                }
            }
        })),
        on_new_window_requested: Some(Box::new(move |ctx| {
            // This one is called when the app is requested to open a new window,
            // e.g. clicking on the Dock icon. It is NOT called from the New Window
            // menu item.
            App::record_last_active_timestamp();
            ctx.dispatch_global_action("root_view:open_new", &());
            ctx.dispatch_global_action("workspace:save_app", &());
        })),
        on_open_urls: Some(Box::new(move |urls, ctx| {
            for url in &urls {
                let parsed_url = Url::parse(url);
                match parsed_url {
                    Ok(url) => uri::handle_incoming_uri(&url, ctx),
                    Err(e) => log::warn!("Unable to parse received url: {e}"),
                }
            }
        })),
        on_os_appearance_changed: Some(Box::new(move |ctx| {
            AppearanceManager::handle(ctx).update(ctx, |appearance_manager, ctx| {
                appearance_manager.refresh_theme_state(ctx);
            });
        })),
        on_active_window_changed: Some(Box::new(move |ctx| {
            let windowing_model = ctx.windows();
            let active_window_id = windowing_model.active_window();
            let key_window_is_modal_panel = windowing_model.key_window_is_modal_panel();

            if !key_window_is_modal_panel {
                let update_quake_mode_arg = UpdateQuakeModeEventArg { active_window_id };
                ctx.dispatch_global_action(
                    "root_view:update_quake_mode_state",
                    &update_quake_mode_arg,
                );
            }

            if let Some(active_window_id) = active_window_id {
                OneTimeModalModel::handle(ctx).update(ctx, |model, ctx| {
                    model.update_target_window_id(active_window_id, ctx);
                });
            }

            ctx.dispatch_global_action("workspace:save_app", &());
        })),
        on_window_will_close: Some(Box::new(move |closed_window_data, ctx| {
            if ctx.windows().stage() == ApplicationStage::Terminating {
                return;
            }

            if let Some(window_data) = closed_window_data {
                UndoCloseStack::handle(ctx).update(ctx, |stack, ctx| {
                    stack.handle_window_closed(window_data, ctx);
                });
            }
            ctx.dispatch_global_action("workspace:save_app", &());
        })),
        on_window_moved: Some(Box::new(move |ctx| {
            // During startup, winit fires several move/resize events in a row; save_app is meaningless at this stage and slows down startup.
            if ctx.windows().stage() == ApplicationStage::Starting {
                return;
            }
            ctx.dispatch_global_action("workspace:save_app", &());
        })),
        on_window_resized: Some(Box::new(move |ctx| {
            if ctx.windows().stage() == ApplicationStage::Starting {
                return;
            }
            ctx.dispatch_global_action("workspace:save_app", &());
        })),
        ..Default::default()
    }
}

/// Focuses the active window or if there isn't one then a window with a running process
/// and then shows the native modal.
fn focus_running_window_and_show_native_modal(
    sessions_summary: RunningSessionSummary,
    dialog_with_callbacks: AlertDialogWithCallbacks<AppModalCallback>,
    ctx: &mut AppContext,
) {
    let windowing_model = ctx.windows();
    let active_window_id = windowing_model.active_window();
    // Show the nav palette in the active window. If there is no active window,
    // arbitrarily pick one of the windows having a running process.
    let window_id_to_focus = active_window_id.unwrap_or_else(|| {
        *sessions_summary
            .windows_running()
            .iter()
            .next()
            .expect("already checked len > 0")
    });
    ctx.windows().show_window_and_focus_app(window_id_to_focus);
    if let Some(workspaces) = ctx.views_of_type::<Workspace>(window_id_to_focus) {
        if let Some(handle) = workspaces.first() {
            handle.update(ctx, |view, ctx| {
                view.show_native_modal(dialog_with_callbacks, ctx);
            });
        }
    }
}

fn on_close_app_cancelled(open_navigation_palette: bool, ctx: &mut AppContext) {
    autoupdate::cancel_relaunch(ctx);

    send_telemetry_from_app_ctx!(
        TelemetryEvent::QuitModalCancel {
            nav_palette: open_navigation_palette,
            modal_for: CloseTarget::App,
        },
        ctx
    );

    let sessions = SessionNavigationData::all_sessions(ctx).collect_vec();
    let sessions_summary = RunningSessionSummary::new(&sessions);

    // If open_navigation_palette is false, return early. Otherwise, we honor the open_navigation_palette
    // param which is true if the user clicked the modal button for that. However, if the running
    // processes in this window have finished since the modal popped, there is nothing to do now and we
    // can return early
    if !open_navigation_palette || sessions_summary.long_running_cmds.is_empty() {
        return;
    }

    let windowing_model = ctx.windows();
    let active_window_id = windowing_model.active_window();
    // show the nav palette in the active window. if there is no active window,
    // arbitrarily pick one of the windows having a running process
    let window_id_to_focus = active_window_id.unwrap_or_else(|| {
        *sessions_summary
            .windows_running()
            .iter()
            .next()
            .expect("already checked len > 0")
    });

    windowing_model.show_window_and_focus_app(window_id_to_focus);

    // open the nav palette in the selected window
    if let Some(workspaces) = ctx.views_of_type::<Workspace>(window_id_to_focus) {
        if let Some(handle) = workspaces.first() {
            ctx.dispatch_typed_action_for_view(
                window_id_to_focus,
                handle.id(),
                &WorkspaceAction::OpenPalette {
                    mode: PaletteMode::Navigation,
                    source: PaletteSource::QuitModal,
                    query: Some("running".to_owned()),
                },
            );
        }
    }
}

fn on_close_window_cancelled(
    window_id: WindowId,
    open_navigation_palette: bool,
    ctx: &mut AppContext,
) {
    send_telemetry_from_app_ctx!(
        TelemetryEvent::QuitModalCancel {
            nav_palette: open_navigation_palette,
            modal_for: CloseTarget::Window,
        },
        ctx
    );

    let sessions = SessionNavigationData::all_sessions(ctx).collect_vec();
    let sessions_summary = RunningSessionSummary::new(&sessions);
    let num_processes_in_window = sessions_summary.processes_in_window(&window_id).len();

    // If open_navigation_palette is false, return early. Otherwise, we honor the
    // open_navigation_palette param which is true if the user clicked the modal
    // button for that. However, if the running processes in this window have finished
    // since the modal popped, there is nothing to do now and we can return early
    if !open_navigation_palette || num_processes_in_window == 0 {
        return;
    }

    ctx.windows().show_window_and_focus_app(window_id);

    // if we haven't returned early, it means open_navigation_palette is true as the
    // user pressed the modal button for opening the navigation palette to show their
    // running processes
    if let Some(workspaces) = ctx.views_of_type::<Workspace>(window_id) {
        if let Some(handle) = workspaces.first() {
            ctx.dispatch_typed_action_for_view(
                window_id,
                handle.id(),
                &WorkspaceAction::OpenPalette {
                    mode: PaletteMode::Navigation,
                    source: PaletteSource::QuitModal,
                    query: Some("running".to_owned()),
                },
            );
        }
    }
}

fn launch(ctx: &mut warpui::AppContext, app_state: Option<AppState>, launch_mode: LaunchMode) {
    IntervalTimer::handle(ctx).update(ctx, |timer, _ctx| {
        timer.mark_interval_end("APP_LAUNCHED");
    });

    keyboard::load_custom_keybindings(ctx);

    IntervalTimer::handle(ctx).update(ctx, |timer, _ctx| {
        timer.mark_interval_end("KEYBINDINGS_LOADED");
    });

    // For now, we only specify application-level fallback fonts on web.
    #[cfg(target_family = "wasm")]
    ctx.set_fallback_font_fn(font_fallback::fallback_font_fn);

    match &launch_mode {
        LaunchMode::App { .. } | LaunchMode::Test { .. } => {
            // Attempt to restore windows from the persisted application state.
            let arg = OpenFromRestoredArg { app_state };
            ctx.dispatch_global_action("root_view:open_from_restored", &arg);

            // Process any URLs that were provided on the command line (which may be
            // file:// URLs or ones using our custom URL scheme).
            for url in launch_mode.args().urls.iter() {
                uri::handle_incoming_uri(url, ctx);
            }

            // If, after session restoration and command-line argument handling, we
            // haven't opened any windows, open a new window.
            if ctx.window_ids().count() == 0 {
                ctx.dispatch_global_action("root_view:open_new", &());
            }

            IntervalTimer::handle(ctx).update(ctx, |timer, _| {
                timer.mark_interval_end("WINDOWS_CREATED");
            });

            // TODO(ben): We should skip this for LaunchMode::Test.
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            {
                use crate::login_item::maybe_register_app_as_login_item;
                use crate::terminal::general_settings::GeneralSettingsChangedEvent;
                // Note that we put this here because it depends on settings already having been initialized.
                ctx.subscribe_to_model(&GeneralSettings::handle(ctx), |_, event, ctx| {
                    if matches!(event, GeneralSettingsChangedEvent::LoginItem { .. }) {
                        maybe_register_app_as_login_item(ctx);
                    }
                });
                maybe_register_app_as_login_item(ctx);
            }
        }
        #[cfg_attr(target_family = "wasm", allow(unused_variables))]
        LaunchMode::CommandLine {
            command,
            global_options,
            ..
        } => {
            cfg_if::cfg_if! {
                if #[cfg(target_family = "wasm")] {
                    panic!("Cannot execute CLI command {command:?} on the web");
                } else {
                    if let Err(err) = crate::ai::agent_sdk::run(ctx, command.clone(), global_options.clone()) {
                        eprintln!("{err:#}");
                        report_error!(err);
                        std::process::exit(1);
                    }
                }
            }
        }
        // RemoteServerProxy and RemoteServerDaemon never go through
        // run_internal / launch; they call init_common directly and then
        // their own entry points.
        LaunchMode::RemoteServerProxy | LaunchMode::RemoteServerDaemon => {
            log::error!("Proxy/Daemon modes should not use the launch() path");
            std::process::exit(1);
        }
        LaunchMode::Tui { .. } => {
            unreachable!("LaunchMode::Tui is dispatched to crate::tui before launch()")
        }
    }
}

/// Initializes the logger before running tests.
///
/// The `ctor` attribute here means that this runs BEFORE main(), whenever the
/// binary is executed. For this reason, we need to ensure that this function
/// only exists within unit test code. Production bundles and integration tests
/// also initialize the logging system, and initializing it twice causes a panic.
///
/// Additionally, we must not write anything to stdout in this function, as it
/// can interfere with test harnesses collecting the set of tests to run. (This
/// is why we're not simply calling the init() function above.)
#[ctor::ctor]
#[cfg(test)]
fn init_logging_for_unit_tests_glue() {
    // Initialize terminal-friendly logging for tests from the shared logger crate.
    warp_logging::init_logging_for_unit_tests();
}

/// Mark all features which should be enabled on the current channel as enabled.
/// This sets global feature flag state and should never be called in a unit test.
pub fn init_feature_flags() {
    for flag in enabled_features() {
        flag.set_enabled(true);
    }
    features::mark_initialized();
}

/// Returns all feature flags which should be enabled in the current channel.
pub fn enabled_features() -> HashSet<FeatureFlag> {
    // Enable features overridden for the given channel.
    let mut flags = ChannelState::additional_features();

    // Enable flags for release builds.
    //
    // `is_release_bundle()` is `cfg!(feature = "release_bundle")` — set by `script/bundle`,
    // i.e. "packaged for distribution", NOT "compiled with --release". That distinction
    // surprised the maintainer and cost a long debugging session: `script/run --release`
    // produced a build with `debug_assertions` off (so the dev-only block below did not
    // fire) AND no bundle feature (so this did not either), leaving
    // `FeatureFlag::SshRemoteServer` off. SSH sessions silently fell back to the legacy
    // path, the remote-server install prompt never appeared, and the agent's file tools
    // refused on every remote host — with no log line saying why.
    //
    // A release-profile build is a build someone actually runs, so it now gets the release
    // flags too. This is only safe because `FeatureFlag::Autoupdate` was removed from
    // `RELEASE_FLAGS` (see its comment): a dev build that inherited autoupdate could have
    // tried to replace the binary you just compiled. Everything left in the list is
    // display/rendering or locally-scoped (Changelog, CrashReporting — which uploads
    // nothing here, only installs a local panic hook — ImeMarkedText, markdown tables,
    // SshRemoteServer).
    if ChannelState::is_release_bundle() || !cfg!(debug_assertions) {
        flags.extend(features::RELEASE_FLAGS);
    }

    // SSH remote-server: release bundles enable this via RELEASE_FLAGS, but a dev
    // source build (`cargo run`) isn't a release bundle, so the flag would always
    // stay off — SSH sessions would always fall back to the legacy path, the
    // remote-server transport wouldn't activate, and the dev-mode auto-build-and-upload
    // binary (see ssh_transport.rs) would never get a chance to trigger. This explicitly
    // enables it in debug builds so remote file opening / buffer-sync can be tested
    // during development. Windows doesn't yet support the remote-server binary, so it's
    // excluded here too, consistent with RELEASE_FLAGS' cfg.
    #[cfg(all(debug_assertions, not(windows)))]
    flags.insert(FeatureFlag::SshRemoteServer);

    // Issue #72: the HTTP proxy settings page. Not gated by channel — enabled by
    // default on all channels including phosphor-oss, as a basic capability for
    // corporate VPN / company proxy scenarios.
    flags.insert(FeatureFlag::HttpProxySettings);

    let extra_flags: &[FeatureFlag] = &[
        #[cfg(feature = "autoupdate")]
        FeatureFlag::Autoupdate,
        #[cfg(feature = "changelog")]
        FeatureFlag::Changelog,
        #[cfg(feature = "crash_reporting")]
        FeatureFlag::CrashReporting,
        #[cfg(feature = "record_app_active_events")]
        FeatureFlag::RecordAppActiveEvents,
        #[cfg(feature = "runtime_feature_flags")]
        FeatureFlag::RuntimeFeatureFlags,
        #[cfg(feature = "sequential_storage")]
        FeatureFlag::SequentialStorage,
        #[cfg(feature = "in_band_generators_ssh")]
        FeatureFlag::InBandGeneratorsForSSH,
        #[cfg(feature = "run_generators_with_cmd_exe")]
        FeatureFlag::RunGeneratorsWithCmdExe,
        #[cfg(feature = "windows_high_performance_gpu_default")]
        FeatureFlag::WindowsHighPerformanceGpuDefault,
        #[cfg(feature = "ligatures")]
        FeatureFlag::Ligatures,
        #[cfg(feature = "selectable_prompt")]
        FeatureFlag::SelectablePrompt,
        #[cfg(feature = "agent_mode")]
        FeatureFlag::AgentMode,
        #[cfg(feature = "resize_fix")]
        FeatureFlag::ResizeFix,
        #[cfg(feature = "richtext_multiselect")]
        FeatureFlag::RichTextMultiselect,
        #[cfg(feature = "default_waterfall_mode")]
        FeatureFlag::DefaultWaterfallMode,
        #[cfg(feature = "settings_file")]
        FeatureFlag::SettingsFile,
        #[cfg(feature = "settings_import")]
        FeatureFlag::SettingsImport,
        #[cfg(feature = "rect_selection")]
        FeatureFlag::RectSelection,
        #[cfg(feature = "alacritty_settings_import")]
        FeatureFlag::AlacrittySettingsImport,
        #[cfg(feature = "dynamic_workflow_enums")]
        FeatureFlag::DynamicWorkflowEnums,
        #[cfg(feature = "shared_with_me")]
        FeatureFlag::SharedWithMe,
        #[cfg(feature = "am_workflows")]
        FeatureFlag::AgentModeWorkflows,
        #[cfg(feature = "ai_rules")]
        FeatureFlag::AIRules,
        #[cfg(feature = "ssh_tmux_wrapper")]
        FeatureFlag::SSHTmuxWrapper,
        #[cfg(feature = "less_horizontal_terminal_padding")]
        FeatureFlag::LessHorizontalTerminalPadding,
        #[cfg(feature = "shell_selector")]
        FeatureFlag::ShellSelector,
        #[cfg(feature = "block_toolbelt_save_as_workflow")]
        FeatureFlag::BlockToolbeltSaveAsWorkflow,
        // Zap Wave 7-2: the `CloudEnvironments` FeatureFlag was physically removed along
        // with the cloud ambient agent's main subsystem — the `warp environment`
        // subcommand + `--environment` argument were retired at the same time.
        #[cfg(all(feature = "simulate_github_unauthed", debug_assertions))]
        FeatureFlag::SimulateGithubUnauthed,
        #[cfg(feature = "full_screen_zen_mode")]
        FeatureFlag::FullScreenZenMode,
        #[cfg(feature = "minimalist_ui")]
        FeatureFlag::MinimalistUI,
        #[cfg(feature = "remove_alt_screen_padding")]
        FeatureFlag::RemoveAltScreenPadding,
        #[cfg(feature = "avatar_in_tab_bar")]
        FeatureFlag::AvatarInTabBar,
        #[cfg(feature = "workflow_aliases")]
        FeatureFlag::WorkflowAliases,
        #[cfg(feature = "ssh_drag_and_drop")]
        FeatureFlag::SshDragAndDrop,
        #[cfg(feature = "drag_tabs_to_windows")]
        FeatureFlag::DragTabsToWindows,
        #[cfg(feature = "cycle_next_command_suggestion")]
        FeatureFlag::CycleNextCommandSuggestion,
        #[cfg(feature = "multi_workspace")]
        FeatureFlag::MultiWorkspace,
        #[cfg(feature = "osc_hyperlinks")]
        FeatureFlag::OscHyperlinks,
        #[cfg(feature = "ime_marked_text")]
        FeatureFlag::ImeMarkedText,
        #[cfg(feature = "partial_next_command_suggestions")]
        FeatureFlag::PartialNextCommandSuggestions,
        #[cfg(feature = "iterm_images")]
        FeatureFlag::ITermImages,
        #[cfg(feature = "validate_autosuggestions")]
        FeatureFlag::ValidateAutosuggestions,
        #[cfg(feature = "prompt_suggestions_via_maa")]
        FeatureFlag::PromptSuggestionsViaMAA,
        #[cfg(feature = "clear_autosuggestion_on_escape")]
        FeatureFlag::ClearAutosuggestionOnEscape,
        #[cfg(feature = "autoupdate_ui_revamp")]
        FeatureFlag::AutoupdateUIRevamp,
        #[cfg(all(not(windows), feature = "kitty_images"))]
        FeatureFlag::KittyImages,
        #[cfg(feature = "warp_packs")]
        FeatureFlag::WarpPacks,
        #[cfg(feature = "default_adeberry_theme")]
        FeatureFlag::DefaultAdeberryTheme,
        #[cfg(feature = "agent_mode_primary_xml")]
        FeatureFlag::AgentModePrimaryXML,
        #[cfg(feature = "agent_mode_pre_plan_xml")]
        FeatureFlag::AgentModePrePlanXML,
        #[cfg(feature = "agent_onboarding")]
        FeatureFlag::AgentOnboarding,
        #[cfg(feature = "suggested_rules")]
        FeatureFlag::SuggestedRules,
        #[cfg(feature = "suggested_agent_mode_workflows")]
        FeatureFlag::SuggestedAgentModeWorkflows,
        #[cfg(feature = "command_correction_key")]
        FeatureFlag::CommandCorrectionKey,
        #[cfg(feature = "predict_am_queries")]
        FeatureFlag::PredictAMQueries,
        #[cfg(feature = "use_tantivy_search")]
        FeatureFlag::UseTantivySearch,
        #[cfg(feature = "grep_tool")]
        FeatureFlag::GrepTool,
        #[cfg(feature = "mcp_server")]
        FeatureFlag::McpServer,
        #[cfg(feature = "mcp_debugging_ids")]
        FeatureFlag::McpDebuggingIds,
        #[cfg(feature = "markdown_tables")]
        FeatureFlag::MarkdownTables,
        #[cfg(feature = "blocklist_markdown_table_rendering")]
        FeatureFlag::BlocklistMarkdownTableRendering,
        #[cfg(feature = "blocklist_markdown_images")]
        FeatureFlag::BlocklistMarkdownImages,
        #[cfg(feature = "markdown_mermaid")]
        FeatureFlag::MarkdownMermaid,
        #[cfg(feature = "editable_markdown_mermaid")]
        FeatureFlag::EditableMarkdownMermaid,
        #[cfg(feature = "image_as_context")]
        FeatureFlag::ImageAsContext,
        #[cfg(feature = "msys2_shells")]
        FeatureFlag::MSYS2Shells,
        #[cfg(feature = "file_retrieval_tools")]
        FeatureFlag::FileRetrievalTools,
        #[cfg(feature = "reload_stale_conversation_files")]
        FeatureFlag::ReloadStaleConversationFiles,
        #[cfg(feature = "retry_truncated_code_responses")]
        FeatureFlag::RetryTruncatedCodeResponses,
        #[cfg(feature = "read_image_files")]
        FeatureFlag::ReadImageFiles,
        #[cfg(feature = "ai_context_menu")]
        FeatureFlag::AIContextMenuEnabled,
        #[cfg(feature = "at_menu_outside_of_ai_mode")]
        FeatureFlag::AtMenuOutsideOfAIMode,
        #[cfg(feature = "ai_resume_button")]
        FeatureFlag::AIResumeButton,
        #[cfg(feature = "figma_detection")]
        FeatureFlag::FigmaDetection,
        #[cfg(feature = "agent_decides_command_execution")]
        FeatureFlag::AgentDecidesCommandExecution,
        #[cfg(feature = "context_line_review_comments")]
        FeatureFlag::ContextLineReviewComments,
        #[cfg(feature = "nld_fasttext_model")]
        FeatureFlag::NLDClassifierModelEnabled,
        #[cfg(feature = "fast_forward_autoexecute_button")]
        FeatureFlag::FastForwardAutoexecuteButton,
        #[cfg(feature = "code_find_replace")]
        FeatureFlag::CodeFindReplace,
        #[cfg(feature = "command_palette_file_search")]
        FeatureFlag::CommandPaletteFileSearch,
        #[cfg(feature = "ai_context_menu_commands")]
        FeatureFlag::AIContextMenuCommands,
        #[cfg(feature = "ai_context_menu_code")]
        FeatureFlag::AIContextMenuCode,
        #[cfg(feature = "expand_edit_to_pane")]
        FeatureFlag::ExpandEditToPane,
        #[cfg(feature = "fallback_model_load_output_messaging")]
        FeatureFlag::FallbackModelLoadOutputMessaging,
        #[cfg(feature = "tab_close_button_on_left")]
        FeatureFlag::TabCloseButtonOnLeft,
        #[cfg(feature = "profiles_design_revamp")]
        FeatureFlag::ProfilesDesignRevamp,
        #[cfg(feature = "changed_lines_only_apply_diff_result")]
        FeatureFlag::ChangedLinesOnlyApplyDiffResult,
        #[cfg(feature = "linked_code_blocks")]
        FeatureFlag::LinkedCodeBlocks,
        #[cfg(feature = "tabbed_editor_view")]
        FeatureFlag::TabbedEditorView,
        #[cfg(feature = "undo_closed_panes")]
        FeatureFlag::UndoClosedPanes,
        #[cfg(feature = "multi_profile")]
        FeatureFlag::MultiProfile,
        #[cfg(feature = "conversation_artifacts")]
        FeatureFlag::ConversationArtifacts,
        #[cfg(feature = "sync_ambient_plans")]
        FeatureFlag::SyncAmbientPlans,
        #[cfg(feature = "get_started_tab")]
        FeatureFlag::GetStartedTab,
        #[cfg(feature = "welcome_tab")]
        FeatureFlag::WelcomeTab,
        #[cfg(feature = "projects")]
        FeatureFlag::Projects,
        #[cfg(feature = "drive_objects_as_context")]
        FeatureFlag::DriveObjectsAsContext,
        #[cfg(feature = "pr_comments_slash_command")]
        FeatureFlag::PRCommentsSlashCommand,
        #[cfg(feature = "pr_comments_v2")]
        FeatureFlag::PRCommentsV2,
        #[cfg(feature = "pr_comments_skill")]
        FeatureFlag::PRCommentsSkill,
        #[cfg(feature = "selection_as_context")]
        FeatureFlag::SelectionAsContext,
        #[cfg(feature = "code_mode_chip")]
        FeatureFlag::CodeModeChip,
        #[cfg(feature = "github_pr_prompt_chip")]
        FeatureFlag::GithubPrPromptChip,
        #[cfg(feature = "create_project_flow")]
        FeatureFlag::CreateProjectFlow,
        #[cfg(feature = "vim_code_editor")]
        FeatureFlag::VimCodeEditor,
        #[cfg(feature = "allow_opening_file_links_using_editor_env")]
        FeatureFlag::AllowOpeningFileLinksUsingEditorEnv,
        #[cfg(feature = "nld_improvements")]
        FeatureFlag::NldImprovements,
        #[cfg(feature = "revert_diff_hunk")]
        FeatureFlag::RevertDiffHunk,
        #[cfg(feature = "code_review_save_changes")]
        FeatureFlag::CodeReviewSaveChanges,
        #[cfg(feature = "file_tree")]
        FeatureFlag::FileTree,
        #[cfg(feature = "allow_ignoring_input_suggestions")]
        FeatureFlag::AllowIgnoringInputSuggestions,
        // Zap (localization): the cloud entry points for the ambient agent / agent
        // management view have been physically retired. Running BYOP agents locally
        // doesn't depend on these entry points.
        #[cfg(feature = "code_launch_modal")]
        FeatureFlag::CodeLaunchModal,
        #[cfg(feature = "api_key_authentication")]
        FeatureFlag::APIKeyAuthentication,
        #[cfg(feature = "api_key_management")]
        FeatureFlag::APIKeyManagement,
        #[cfg(feature = "mcp_oauth")]
        FeatureFlag::McpOauth,
        #[cfg(feature = "file_based_mcp")]
        FeatureFlag::FileBasedMcp,
        #[cfg(feature = "diff_set_as_context")]
        FeatureFlag::DiffSetAsContext,
        #[cfg(feature = "discard_per_file_and_all_changes")]
        FeatureFlag::DiscardPerFileAndAllChanges,
        #[cfg(feature = "stage_changes")]
        FeatureFlag::StageChanges,
        #[cfg(feature = "summarization_cancellation_confirmation")]
        FeatureFlag::SummarizationCancellationConfirmation,
        #[cfg(feature = "code_review_find")]
        FeatureFlag::CodeReviewFind,
        #[cfg(feature = "ui_zoom")]
        FeatureFlag::UIZoom,
        #[cfg(feature = "auto_open_code_review_pane")]
        FeatureFlag::AutoOpenCodeReviewPane,
        #[cfg(feature = "inline_code_review")]
        FeatureFlag::InlineCodeReview,
        #[cfg(feature = "summarize_conversation_command")]
        FeatureFlag::SummarizationConversationCommand,
        #[cfg(feature = "mcp_grouped_server_context")]
        FeatureFlag::MCPGroupedServerContext,
        #[cfg(feature = "web_search_ui")]
        FeatureFlag::WebSearchUI,
        #[cfg(feature = "web_fetch_ui")]
        FeatureFlag::WebFetchUI,
        #[cfg(feature = "fork_from_command")]
        FeatureFlag::ForkFromCommand,
        #[cfg(feature = "context_window_usage_v2")]
        FeatureFlag::ContextWindowUsageV2,
        #[cfg(feature = "global_search")]
        FeatureFlag::GlobalSearch,
        #[cfg(feature = "embedded_code_review_comments")]
        FeatureFlag::EmbeddedCodeReviewComments,
        #[cfg(feature = "file_and_diff_set_comments")]
        FeatureFlag::FileAndDiffSetComments,
        #[cfg(feature = "revert_to_checkpoints")]
        FeatureFlag::RevertToCheckpoints,
        #[cfg(feature = "rewind_slash_command")]
        FeatureFlag::RewindSlashCommand,
        #[cfg(feature = "agent_view")]
        FeatureFlag::AgentView,
        #[cfg(feature = "agent_view_block_context")]
        FeatureFlag::AgentViewBlockContext,
        #[cfg(feature = "v4a_file_diffs")]
        FeatureFlag::V4AFileDiffs,
        #[cfg(feature = "interactive_conversation_management_view")]
        FeatureFlag::InteractiveConversationManagementView,
        #[cfg(feature = "agent_tips")]
        FeatureFlag::AgentTips,
        #[cfg(feature = "agent_mode_computer_use")]
        FeatureFlag::AgentModeComputerUse,
        #[cfg(feature = "local_computer_use")]
        FeatureFlag::LocalComputerUse,
        #[cfg(feature = "agent_toolbar_editor")]
        FeatureFlag::AgentToolbarEditor,
        #[cfg(feature = "configurable_toolbar")]
        FeatureFlag::ConfigurableToolbar,
        #[cfg(feature = "agent_view_prompt_chip")]
        FeatureFlag::AgentViewPromptChip,
        #[cfg(feature = "classic_completions")]
        FeatureFlag::ClassicCompletions,
        #[cfg(feature = "force_classic_completions")]
        FeatureFlag::ForceClassicCompletions,
        #[cfg(feature = "agent_view_conversation_list_view")]
        FeatureFlag::AgentViewConversationListView,
        #[cfg(feature = "inline_history_menu")]
        FeatureFlag::InlineHistoryMenu,
        #[cfg(feature = "inline_repo_menu")]
        FeatureFlag::InlineRepoMenu,
        #[cfg(feature = "summarization_via_message_replacement")]
        FeatureFlag::SummarizationViaMessageReplacement,
        #[cfg(feature = "pluggable_notifications")]
        FeatureFlag::PluggableNotifications,
        #[cfg(feature = "list_skills")]
        FeatureFlag::ListSkills,
        #[cfg(feature = "ask_user_question")]
        FeatureFlag::AskUserQuestion,
        #[cfg(feature = "inline_profile_selector")]
        FeatureFlag::InlineProfileSelector,
        #[cfg(feature = "oz_platform_skills")]
        FeatureFlag::OzPlatformSkills,
        #[cfg(feature = "bundled_skills")]
        FeatureFlag::BundledSkills,
        #[cfg(feature = "open_warp_launch_modal")]
        FeatureFlag::ZapLaunchModal,
        #[cfg(feature = "new_tab_styling")]
        FeatureFlag::NewTabStyling,
        #[cfg(feature = "skill_arguments")]
        FeatureFlag::SkillArguments,
        #[cfg(feature = "active_conversation_requires_interaction")]
        FeatureFlag::ActiveConversationRequiresInteraction,
        #[cfg(feature = "conversations_as_context")]
        FeatureFlag::ConversationsAsContext,
        #[cfg(feature = "incremental_auto_reload")]
        FeatureFlag::IncrementalAutoReload,
        #[cfg(feature = "pending_user_query_indicator")]
        FeatureFlag::PendingUserQueryIndicator,
        #[cfg(feature = "queue_slash_command")]
        FeatureFlag::QueueSlashCommand,
        #[cfg(feature = "kitty_keyboard_protocol")]
        FeatureFlag::KittyKeyboardProtocol,
        #[cfg(feature = "inline_menu_headers")]
        FeatureFlag::InlineMenuHeaders,
        #[cfg(feature = "restore_prompt_on_inline_model_selector_search")]
        FeatureFlag::RestorePromptOnInlineModelSelectorSearch,
        #[cfg(feature = "directory_tab_colors")]
        FeatureFlag::DirectoryTabColors,
        #[cfg(feature = "open_warp_new_settings_modes")]
        FeatureFlag::ZapNewSettingsModes,
        #[cfg(feature = "hoa_code_review")]
        FeatureFlag::HoaCodeReview,
        #[cfg(feature = "vertical_tabs")]
        FeatureFlag::VerticalTabs,
        #[cfg(feature = "async_find")]
        FeatureFlag::AsyncFind,
        #[cfg(feature = "background_computer_use")]
        FeatureFlag::BackgroundComputerUse,
        #[cfg(feature = "codex_plugin")]
        FeatureFlag::CodexPlugin,
        #[cfg(feature = "grouped_tabs")]
        FeatureFlag::GroupedTabs,
        #[cfg(feature = "pinned_tabs")]
        FeatureFlag::PinnedTabs,
        #[cfg(feature = "queued_prompts_v2")]
        FeatureFlag::QueuedPromptsV2,
        #[cfg(feature = "remote_codebase_indexing")]
        FeatureFlag::RemoteCodebaseIndexing,
        #[cfg(feature = "terminal_lifecycle_recovery")]
        FeatureFlag::TerminalLifecycleRecovery,
        #[cfg(feature = "vertical_tabs_summary_mode")]
        FeatureFlag::VerticalTabsSummaryMode,
        #[cfg(feature = "tab_configs")]
        FeatureFlag::TabConfigs,
        #[cfg(feature = "agent_harness")]
        FeatureFlag::AgentHarness,
        #[cfg(feature = "hoa_notifications")]
        FeatureFlag::HOANotifications,
        #[cfg(feature = "open_code_notifications")]
        FeatureFlag::OpenCodeNotifications,
        #[cfg(feature = "cli_agent_rich_input")]
        FeatureFlag::CLIAgentRichInput,
        #[cfg(feature = "transfer_control_tool")]
        FeatureFlag::TransferControlTool,
        #[cfg(feature = "warpify_footer")]
        FeatureFlag::WarpifyFooter,
        #[cfg(feature = "solo_user_byok")]
        FeatureFlag::SoloUserByok,
        #[cfg(feature = "hoa_onboarding_flow")]
        FeatureFlag::HOAOnboardingFlow,
        #[cfg(feature = "git_operations_in_code_review")]
        FeatureFlag::GitOperationsInCodeReview,
        #[cfg(feature = "hoa_remote_control")]
        FeatureFlag::HOARemoteControl,
        #[cfg(feature = "codex_notifications")]
        FeatureFlag::CodexNotifications,
        #[cfg(feature = "trim_trailing_blank_lines")]
        FeatureFlag::TrimTrailingBlankLines,
        #[cfg(feature = "configurable_context_window")]
        FeatureFlag::ConfigurableContextWindow,
    ];
    flags.extend(extra_flags.iter().copied());

    // Unstable feature switch: unstable features not yet officially released can be
    // explicitly enabled in release builds via the `ZAP_UNSTABLE_FEATURES` env var. The
    // value is a comma-separated list of unstable feature names (snake_case), or
    // `all` / `*` to enable everything at once.
    //
    // This previously claimed "dev builds already auto-enable all current unstable
    // features via the debug_assertions path". That is false, and it contradicts the
    // doc block below. `enabled_features()` has exactly one `debug_assertions`
    // insertion -- `FeatureFlag::SshRemoteServer` -- so a `cargo run` does NOT enable
    // the flags listed in `UNSTABLE_FEATURES`. This env var is the only enable path
    // for them, in dev builds and release builds alike.
    if let Ok(raw) = std::env::var("ZAP_UNSTABLE_FEATURES") {
        let normalized = raw.trim().to_ascii_lowercase();
        let enable_all = matches!(normalized.as_str(), "all" | "*");
        let requested: HashSet<&str> = normalized
            .split(|c: char| c == ',' || c.is_whitespace())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        for (name, flag) in UNSTABLE_FEATURES {
            if enable_all || requested.contains(name) {
                flags.insert(*flag);
            }
        }
    }

    flags
}

/// Mapping from unstable feature names accepted by `ZAP_UNSTABLE_FEATURES` to
/// FeatureFlag. Features registered here are hidden by default and appear only
/// once the corresponding token is set.
///
/// Note this is an *additional* enable path, not the only one, and it is not
/// implied by a debug build: `enabled_features()` has no blanket
/// `debug_assertions` branch that turns these on (it enables only
/// `SshRemoteServer` that way), so a `cargo run` build sees exactly what a
/// release build does unless the variable or a cargo feature says otherwise.
/// `docs/pin-migration.md` Phase 6.7 counts this list as one of the three paths
/// by which a flag can be reachable in a normal GUI build.
const UNSTABLE_FEATURES: &[(&str, FeatureFlag)] = &[
    (
        "windows_high_performance_gpu_default",
        FeatureFlag::WindowsHighPerformanceGpuDefault,
    ),
    // The Gemini CLI extension install/update chip, gated on
    // `FeatureFlag::GeminiNotifications`.
    //
    // **This is not fork drift, so it deliberately does NOT go in
    // `app/Cargo.toml`'s `default`.** Upstream has no `gemini_notifications`
    // cargo feature either -- not at `02b53fcd8` and not at `42effe840` -- and
    // keeps the flag in `DOGFOOD_FLAGS` alone. Its own `specs/APP-4067/TECH.md`
    // still lists "Promote `GeminiNotifications` from dogfood -- after
    // validation" as an open follow-up. Its `HOANotifications`,
    // `OpenCodeNotifications` and `CodexNotifications` siblings are default-on
    // upstream too, so the asymmetry is upstream's, not this fork's, and
    // matching the siblings would ship the chip further than upstream has.
    //
    // What IS fork-specific is that dogfood-only means unreachable by anyone
    // here. Upstream's dogfood channel is a GUI build, so its team can exercise
    // the chip and eventually promote it. In this fork `bin/phosphor_oss.rs` is
    // the only GUI binary and it adds `DEBUG_FLAGS` alone; `DOGFOOD_FLAGS`
    // reaches no binary at all. (Corrected: this comment used to say it reached
    // `warp_tui`'s `dev`/`local` binaries. Those files are never compiled --
    // `crates/warp_tui/Cargo.toml` sets `autobins = false` with a single declared
    // `[[bin]]`, and they import a crate that is not in the workspace -- so the
    // only readers left are the schema generators.) So the validation upstream is
    // waiting on could never happen here, and `GeminiPluginManager` plus its six
    // `cli-agent-plugin-gemini-*` catalogue strings would stay permanently dead.
    //
    // This entry restores that validation path and nothing more: it is off
    // unless `ZAP_UNSTABLE_FEATURES=gemini_notifications` is set. The rest of
    // the path is present and was traced end to end -- the install command
    // matches the published `warpdotdev/gemini-cli-warp` extension, whose
    // manifest `name` (`gemini-warp`) and `version` (`1.0.0`) match
    // `EXTENSION_NAME` and `MINIMUM_PLUGIN_VERSION`, and the consumer is the
    // local OSC 777 listener, which already accepts `CLIAgent::Gemini` under
    // `HOANotifications` alone. See TODO.md, issue #594.
    ("gemini_notifications", FeatureFlag::GeminiNotifications),
    // The six flags below gate live, shipped code but sat in `DOGFOOD_FLAGS` with
    // no reader: upstream's channel binaries pass that list to
    // `with_additional_features`, and this fork's equivalents
    // (`crates/warp_tui/src/bin/{dev,local,preview,stable}.rs`) never compile --
    // `autobins = false`, one declared `[[bin]]`, and they import a crate absent
    // from the workspace. `bin/phosphor_oss.rs` adds `DEBUG_FLAGS` alone. So the
    // code was unreachable in every binary this repo actually builds, including
    // the entire `warpctrl` local-control surface and its bundled skill.
    //
    // They are registered here rather than promoted to `RELEASE_FLAGS` or to
    // `app/Cargo.toml`'s `default` on purpose. Promotion would *ship* six
    // features that are unfinished or unvalidated here (none of them is
    // default-on at the pin either); this gives a user or a developer an enable
    // path they can actually take -- `ZAP_UNSTABLE_FEATURES=warp_control_cli`,
    // or `=all` -- while the default stays exactly as it is today: off. Nothing
    // about a normal build changes.
    (
        "full_source_code_embedding",
        FeatureFlag::FullSourceCodeEmbedding,
    ),
    (
        "codebase_index_persistence",
        FeatureFlag::CodebaseIndexPersistence,
    ),
    ("warp_control_cli", FeatureFlag::WarpControlCli),
    (
        "jupyter_notebook_rendering",
        FeatureFlag::JupyterNotebookRendering,
    ),
    (
        "multi_level_orchestration",
        FeatureFlag::MultiLevelOrchestration,
    ),
    ("local_docker_sandbox", FeatureFlag::LocalDockerSandbox),
];

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
