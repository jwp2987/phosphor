use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use settings::Setting as _;
use smol_str::SmolStr;
use warp_completer::completer::{CommandExitStatus, CommandOutput};
use warpui::{
    elements::Empty, platform::WindowStyle, App, AppContext, Element, Entity, ModelHandle,
    SingletonEntity, TypedActionView, View, ViewContext,
};

use crate::terminal::shell::{Shell, ShellType};

use super::command_executor::testing::TestCommandExecutor;
use super::{
    BootstrapSessionType, CommandExecutor, ExecuteCommandOptions, Session, SessionId, SessionInfo,
    SessionType, Sessions, SessionsEvent,
};

struct TestView {
    events: Vec<SessionsEvent>,
}

impl Entity for TestView {
    type Event = usize;
}

impl View for TestView {
    fn render<'a>(&self, _: &AppContext) -> Box<dyn Element> {
        Empty::new().finish()
    }

    fn ui_name() -> &'static str {
        "TestView"
    }
}

impl TypedActionView for TestView {
    type Action = ();
}

impl TestView {
    fn new(model: ModelHandle<Sessions>, ctx: &mut ViewContext<Self>) -> Self {
        ctx.subscribe_to_model(&model, |me, _, event, _| {
            me.events.push(event.to_owned());
        });
        Self { events: Vec::new() }
    }
}

#[test]
fn test_set_env_var_emits_event() {
    App::test((), |mut app| async move {
        let model_handle = app.add_model(|_| Sessions::new_for_test());
        let session_id: SessionId = 0.into();
        let (_, view_handle) = app.add_window(WindowStyle::NotStealFocus, |ctx| {
            TestView::new(model_handle.clone(), ctx)
        });
        view_handle.read(&app, |view, _ctx| {
            assert!(view.events.is_empty());
        });
        model_handle.update(&mut app, |sessions, ctx| {
            let new_vars = HashMap::from_iter([("foo".to_string(), "bar".to_string())]);
            sessions.set_env_vars_for_session(session_id, new_vars, ctx)
        });

        view_handle.read(&app, |view, _ctx| {
            assert_eq!(view.events.len(), 1);
            let expected_session_id = session_id;
            let event = view.events.first().expect("checked length already");
            if let SessionsEvent::EnvironmentVariablesUpdated { session_id } = event {
                assert_eq!(*session_id, expected_session_id);
            } else {
                assert!(matches!(
                    event,
                    SessionsEvent::EnvironmentVariablesUpdated { .. }
                ));
            }
        });
    });
}

#[test]
fn test_set_env_var_emits_no_event_when_no_change() {
    App::test((), |mut app| async move {
        let model_handle = app.add_model(|_| Sessions::new_for_test());
        let session_id: SessionId = 0.into();
        let (_, view_handle) = app.add_window(WindowStyle::NotStealFocus, |ctx| {
            TestView::new(model_handle.clone(), ctx)
        });
        view_handle.read(&app, |view, _ctx| {
            assert!(view.events.is_empty());
        });
        model_handle.update(&mut app, |sessions, ctx| {
            let new_vars = HashMap::from_iter([("foo".to_string(), "bar".to_string())]);
            sessions.set_env_vars_for_session(session_id, new_vars, ctx)
        });

        view_handle.read(&app, |view, _ctx| {
            assert_eq!(view.events.len(), 1);
        });

        model_handle.update(&mut app, |sessions, ctx| {
            let new_vars = HashMap::from_iter([("foo".to_string(), "bar".to_string())]);
            sessions.set_env_vars_for_session(session_id, new_vars, ctx)
        });

        view_handle.read(&app, |view, _ctx| {
            assert_eq!(view.events.len(), 1);
        });
    });
}

// Ported from warp/master `app/src/terminal/model/session_tests.rs`. Assertions
// are unchanged from Warp.

#[test]
fn test_malicious_histfile_path_does_not_execute_injected_commands() {
    App::test((), |_app| async move {
        // If escaping is missing, `touch /tmp/warp_injection_test` would execute
        // as a side effect of reading history.
        let marker = "/tmp/warp_injection_test";
        // Clean up in case a previous broken run left the marker.
        let _ = std::fs::remove_file(marker);

        let malicious_histfile = format!("/tmp/x'; touch {marker}; echo '");

        let session_info = SessionInfo::new_for_test()
            .with_session_type(BootstrapSessionType::WarpifiedRemote)
            .with_histfile(Some(malicious_histfile));
        let session = Session::new(session_info, Arc::new(TestCommandExecutor::default()));

        // read_history for a WarpifiedRemote session calls read_history_from_file,
        // which builds `cat '{escaped_path}'` and executes it via TestCommandExecutor
        let _ = session.read_history(false).await;

        assert!(
            !std::path::Path::new(marker).exists(),
            "Injected command executed \u{2014} escaping regression!"
        );
    });
}

#[cfg(not(windows))]
#[test]
fn can_resolve_cwd_to_native_path_accepts_posix_path() {
    let session = Session::test();
    assert!(session.can_resolve_cwd_to_native_path("/Users/foo/bar"));
}

#[cfg(windows)]
#[test]
fn can_resolve_cwd_to_native_path_accepts_windows_drive_path() {
    let session = Session::test();
    assert!(session.can_resolve_cwd_to_native_path(r"E:\CLAUDE-BASE"));
}

#[cfg(windows)]
#[test]
fn can_resolve_cwd_to_native_path_rejects_unix_encoded_path_on_windows() {
    let session_info =
        SessionInfo::new_for_test().with_shell_type(crate::terminal::shell::ShellType::Bash);
    let session = Session::new(session_info, Arc::new(TestCommandExecutor::default()));
    assert!(!session.can_resolve_cwd_to_native_path("/E:/CLAUDE-BASE"));
}

#[cfg(windows)]
#[test]
fn powershell_read_command_embeds_escaped_path_without_args() {
    use std::ffi::{OsStr, OsString};

    use super::powershell_read_all_text_command;

    // The path is embedded directly inside a single-quoted PowerShell literal.
    let raw = r"C:\Users\dev\AppData\Roaming\Microsoft\Windows\PowerShell\PSReadLine\ConsoleHost_history.txt";
    let command = powershell_read_all_text_command(OsStr::new(raw));
    assert_eq!(
        command,
        OsString::from(format!("[System.IO.File]::ReadAllText('{raw}')"))
    );

    // A single quote in the path is doubled so it can't terminate the literal.
    let command = powershell_read_all_text_command(OsStr::new(r"C:\o'brien\history.txt"));
    assert_eq!(
        command,
        OsString::from(r"[System.IO.File]::ReadAllText('C:\o''brien\history.txt')")
    );
}

// --- Deferred function/builtin name sets (#586) -----------------------------
//
// The pin (`02b53fcd8` and `42effe840` alike) ships this machinery with no
// tests of its own, so these are new rather than ported. They pin down the
// properties the port has to hold: the loaders are a no-op for every shell
// whose bootstrap already reports the complete set; for the one shell that
// needs them the deferred names reach `function_names` / `builtin_names` /
// `top_level_commands` without duplicating the bootstrap snapshot; and a run
// that reports failure contributes nothing, however much it wrote to stdout
// first.

/// A `CommandExecutor` that records what it was asked to run and answers with
/// canned output, so the deferred loaders can be driven without a live shell.
#[derive(Debug)]
struct RecordingCommandExecutor {
    stdout: String,
    status: CommandExitStatus,
    commands: Mutex<Vec<String>>,
}

/// The stdout of a PowerShell enumeration that failed partway through.
///
/// It is non-empty on purpose, and that is the whole point of it.
/// `load_deferred_name_set` decodes `CommandOutput::to_string`, which reads
/// **stdout** whatever the exit status, so an empty-stdout failure fixture
/// cannot tell the exit-status guard apart from its absence: the parse arm
/// would run on `""`, add nothing, and every assertion would still hold. The
/// pairing modelled here — bytes on stdout *and* a failure status — is what the
/// executors that carry this command actually produce: `LocalCommandExecutor`
/// pipes stdout and the exit status apart, and `RemoteServerExecutor` forwards
/// both as the remote reported them. On a pty-backed session PowerShell's error
/// record shares the success stream, which is how the prose below lands in the
/// same buffer as the names, wrapped to the terminal width as a real console
/// wraps it.
///
/// Every line is name-shaped, because the loader keeps *whole lines*:
/// `Invoke-Partial` and `Get-Bogus` are indistinguishable from a successful
/// enumeration's output, and the diagnostic lines lead with `Get-Command`, a
/// genuine cmdlet. Delete the exit-status guard and all of them become
/// completion candidates — which is precisely the regression this fixture
/// exists to see.
const FAILED_ENUMERATION_STDOUT: &str = "\
Get-Command : The term 'Get-Bogus' is not recognized as the name of a cmdlet,
function, script file, or operable program. Check the spelling of the name, or
if a path was included, verify that the path is correct and try again.
At line:1 char:11
+ $names = Get-Command -CommandType Function | Where-Object { -not $_ ...
+          ~~~~~~~~~~~
    + CategoryInfo          : ObjectNotFound: (Get-Bogus:String) [Get-Command],
   CommandNotFoundException
Invoke-Partial
Get-Bogus
";

impl RecordingCommandExecutor {
    fn succeeding(stdout: &str) -> Arc<Self> {
        Arc::new(Self {
            stdout: stdout.to_owned(),
            status: CommandExitStatus::Success,
            commands: Mutex::new(Vec::new()),
        })
    }

    /// A command that failed *after* writing to stdout — see
    /// [`FAILED_ENUMERATION_STDOUT`] for why the stdout must not be empty.
    fn failing() -> Arc<Self> {
        Arc::new(Self {
            stdout: FAILED_ENUMERATION_STDOUT.to_owned(),
            status: CommandExitStatus::Failure,
            commands: Mutex::new(Vec::new()),
        })
    }

    fn recorded(&self) -> Vec<String> {
        self.commands.lock().unwrap().clone()
    }
}

#[async_trait]
impl CommandExecutor for RecordingCommandExecutor {
    async fn execute_command(
        &self,
        command: &str,
        _shell: &Shell,
        _current_directory_path: Option<&str>,
        _environment_variables: Option<HashMap<String, String>>,
        _execute_command_options: ExecuteCommandOptions,
    ) -> anyhow::Result<CommandOutput> {
        self.commands.lock().unwrap().push(command.to_owned());
        Ok(CommandOutput {
            stdout: self.stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
            status: match self.status {
                CommandExitStatus::Success => CommandExitStatus::Success,
                CommandExitStatus::Failure => CommandExitStatus::Failure,
            },
            exit_code: None,
        })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn supports_parallel_command_execution(&self) -> bool {
        false
    }
}

fn names(values: &[&str]) -> HashSet<SmolStr> {
    values.iter().map(|value| SmolStr::from(*value)).collect()
}

fn session_for_shell(
    shell_type: ShellType,
    executor: Arc<RecordingCommandExecutor>,
) -> Arc<Session> {
    let info = SessionInfo::new_for_test()
        .with_shell_type(shell_type)
        .with_function_names(names(&["prompt"]))
        .with_builtins(names(&["Get-Item"]));
    Arc::new(Session::new(info, executor))
}

fn sorted<'a>(values: impl Iterator<Item = &'a str>) -> Vec<&'a str> {
    let mut values: Vec<&str> = values.collect();
    values.sort_unstable();
    values
}

#[test]
fn deferred_function_names_are_merged_without_duplicating_the_bootstrap_set() {
    App::test((), |_app| async move {
        // The bootstrap snapshot already carried `prompt`; only the two names it
        // did not carry should be added. The blank line is deliberate — a
        // pty-captured enumeration picks up stray newlines, and without the
        // loader's `!name.is_empty()` filter it would arrive as an empty
        // completion candidate, so this is what makes that filter observable.
        let executor =
            RecordingCommandExecutor::succeeding("prompt\n\nInvoke-Custom\nStart-Thing\n");
        let session = session_for_shell(ShellType::PowerShell, executor.clone());

        session.load_all_function_names().await;

        assert_eq!(
            sorted(session.function_names()),
            vec!["Invoke-Custom", "Start-Thing", "prompt"]
        );
        assert!(sorted(session.top_level_commands()).contains(&"Invoke-Custom"));
        assert_eq!(
            executor.recorded(),
            vec![ShellType::PowerShell
                .shell_command_to_get_all_functions()
                .expect("PowerShell enumerates functions asynchronously")
                .to_owned()]
        );
    });
}

#[test]
fn deferred_builtin_names_are_merged_without_duplicating_the_bootstrap_set() {
    App::test((), |_app| async move {
        // As with the function names above, the blank line keeps the loader's
        // empty-line filter observable rather than assumed.
        let executor = RecordingCommandExecutor::succeeding("Get-Item\n\nGet-Custom\n");
        let session = session_for_shell(ShellType::PowerShell, executor.clone());

        session.load_all_builtins().await;

        assert_eq!(
            sorted(session.builtin_names()),
            vec!["Get-Custom", "Get-Item"]
        );
        assert!(sorted(session.top_level_commands()).contains(&"Get-Custom"));
        assert_eq!(
            executor.recorded(),
            vec![ShellType::PowerShell
                .shell_command_to_get_all_builtins()
                .expect("PowerShell enumerates builtins asynchronously")
                .to_owned()]
        );
    });
}

#[test]
fn deferred_name_loaders_run_no_command_for_shells_that_report_at_bootstrap() {
    App::test((), |_app| async move {
        for shell_type in [ShellType::Bash, ShellType::Zsh, ShellType::Fish] {
            let executor = RecordingCommandExecutor::succeeding("should_never_be_read\n");
            let session = session_for_shell(shell_type, executor.clone());

            session.load_all_function_names().await;
            session.load_all_builtins().await;

            assert!(
                executor.recorded().is_empty(),
                "{shell_type:?} enumerates functions and builtins during bootstrap; \
                 a second in-band command would be pure overhead"
            );
            assert_eq!(sorted(session.function_names()), vec!["prompt"]);
            assert_eq!(sorted(session.builtin_names()), vec!["Get-Item"]);
        }
    });
}

#[test]
fn deferred_name_set_is_loaded_at_most_once_per_session() {
    App::test((), |_app| async move {
        let executor = RecordingCommandExecutor::succeeding("Invoke-Custom\n");
        let session = session_for_shell(ShellType::PowerShell, executor.clone());

        session.load_all_function_names().await;
        session.load_all_function_names().await;

        // Sequential callers only. Dropping the shared-future cell entirely
        // shows up here as a second recorded command, but replacing it with a
        // "has `storage` been set yet?" check would not: this executor answers
        // without ever yielding, so the first load always finishes before the
        // second starts. Covering *concurrent* callers needs a fixture that can
        // be held pending mid-command; see the note in `load_deferred_name_set`
        // for the property that would then be under test.
        assert_eq!(executor.recorded().len(), 1);
        assert_eq!(
            sorted(session.function_names()),
            vec!["Invoke-Custom", "prompt"]
        );
    });
}

#[test]
fn a_failed_enumeration_leaves_the_bootstrap_names_intact() {
    App::test((), |_app| async move {
        let executor = RecordingCommandExecutor::failing();
        let session = session_for_shell(ShellType::PowerShell, executor.clone());

        session.load_all_function_names().await;
        session.load_all_builtins().await;

        // Both loaders really did reach the shell. Without this the rest of the
        // test would pass just as well on a session that never enumerated at
        // all, which is not the property being pinned down.
        assert_eq!(
            executor.recorded(),
            vec![
                ShellType::PowerShell
                    .shell_command_to_get_all_functions()
                    .expect("PowerShell enumerates functions asynchronously")
                    .to_owned(),
                ShellType::PowerShell
                    .shell_command_to_get_all_builtins()
                    .expect("PowerShell enumerates builtins asynchronously")
                    .to_owned(),
            ]
        );

        assert_eq!(sorted(session.function_names()), vec!["prompt"]);
        assert_eq!(sorted(session.builtin_names()), vec!["Get-Item"]);
        // Nothing at all from the failed run, not even by way of
        // `top_level_commands`, which merges both deferred sets.
        assert_eq!(
            sorted(session.top_level_commands()),
            vec!["Get-Item", "prompt"]
        );

        // Checked line by line as well, so a regression names the text that
        // leaked instead of only reporting a set that no longer matches.
        for line in FAILED_ENUMERATION_STDOUT.lines() {
            assert!(
                !session.top_level_commands().any(|command| command == line),
                "a failed enumeration's stdout became a completion candidate: {line:?}"
            );
        }
    });
}

// --- Host registry wiring (`docs/design/moth-parliament.md`, "Requirement 5
// needs a surface, and a registry that does not exist") ---------------------

/// Breaks if the `record_host_reached(session, host_id.clone(), *sid, ctx)`
/// call were removed from the `SessionConnected`/`SessionReconnected` arms in
/// `Sessions::new`'s `RemoteServerManager` subscription, or if
/// `record_host_reached` itself stopped calling
/// `HostRegistryModel::record_reached` (e.g. passing `None` for `host_id`).
///
/// Drives `record_host_reached` directly rather than through a real
/// `RemoteServerManagerEvent::SessionConnected` dispatch: that would require
/// forcing `FeatureFlag::SshRemoteServer` on and standing up a real
/// `RemoteServerManager` connection in a test harness, which nothing else in
/// this codebase does. `record_host_reached` is generic over the caller's
/// model type for exactly this reason (see its doc comment), so a `Sessions`
/// update closure supplies a perfectly good `ModelContext` on its own; the
/// two call sites themselves are a two-line, directly-inspectable diff.
#[cfg(feature = "local_tty")]
#[test]
fn host_reached_records_host_id_in_registry() {
    use crate::remote_server::host_registry::HostRegistryModel;
    use crate::remote_server::manager::RemoteServerManager;
    use crate::terminal::model::terminal_model::SubshellInitializationInfo;
    use crate::terminal::ssh::util::InteractiveSshCommand;
    use crate::terminal::warpify::settings::WarpifySettings;
    use warp_core::HostId;

    App::test((), |mut app| async move {
        crate::test_util::settings::initialize_settings_for_tests(&mut app);
        app.add_singleton_model(HostRegistryModel::new);
        app.add_singleton_model(RemoteServerManager::new);

        let mut session_info = SessionInfo::new_for_test();
        session_info.subshell_info = Some(SubshellInitializationInfo {
            spawning_command: "ssh build-box".to_string(),
            was_triggered_by_rc_file_snippet: false,
            env_var_collection_name: None,
            ssh_connection_info: Some(InteractiveSshCommand {
                host: Some("build-box".to_string()),
                port: None,
            }),
        });
        let session = Session::new(session_info, Arc::new(TestCommandExecutor::default()));
        let host_id = HostId::new("host-abc".to_string());

        let sessions_handle = app.add_model(|_| Sessions::new_for_test());
        sessions_handle.update(&mut app, |_sessions, ctx| {
            super::record_host_reached(
                &session,
                host_id.clone(),
                SessionId::from(1),
                Some("1.2.3".to_string()),
                ctx,
            );
        });

        app.read(|ctx| {
            let entry = HostRegistryModel::as_ref(ctx)
                .host("build-box")
                .expect("recording a reached host must create/update its registry entry");
            assert_eq!(entry.host_id, Some(host_id.clone()));
            assert!(
                entry.last_reached_at.is_some(),
                "record_reached must stamp last_reached_at"
            );
            assert_eq!(
                entry.install_state,
                crate::remote_server::host_registry::HostInstallState::Installed {
                    version: Some("1.2.3".to_string())
                },
                "a Some(server_version) must be recorded as the real installed version"
            );

            // Round-trips through settings, not just `HostRegistryModel`'s
            // own in-memory cache.
            let persisted = WarpifySettings::as_ref(ctx)
                .remote_host_registry_entries
                .value()
                .iter()
                .find(|entry| entry.target == "build-box")
                .expect("build-box must have a persisted registry entry")
                .clone();
            assert_eq!(persisted.host_id.as_deref(), Some("host-abc"));
        });
    });
}

/// Breaks if `record_host_reached` recorded a target derived from the
/// reported hostname/user instead of the parsed `ssh` destination (which
/// would silently create a second, disagreeing entry for a host that
/// already exists under its `warpify.ssh.remote_hosts` spelling), or if it
/// stopped being a no-op when the session's bootstrap never parsed an `ssh`
/// invocation at all.
#[cfg(feature = "local_tty")]
#[test]
fn host_reached_is_a_noop_without_a_parsed_ssh_destination() {
    use crate::remote_server::host_registry::HostRegistryModel;
    use crate::remote_server::manager::RemoteServerManager;
    use warp_core::HostId;

    App::test((), |mut app| async move {
        crate::test_util::settings::initialize_settings_for_tests(&mut app);
        app.add_singleton_model(HostRegistryModel::new);
        app.add_singleton_model(RemoteServerManager::new);

        // A local (non-SSH) session bootstrap: `subshell_info` stays `None`.
        let session_info = SessionInfo::new_for_test();
        let session = Session::new(session_info, Arc::new(TestCommandExecutor::default()));
        let host_id = HostId::new("host-abc".to_string());

        let sessions_handle = app.add_model(|_| Sessions::new_for_test());
        sessions_handle.update(&mut app, |_sessions, ctx| {
            super::record_host_reached(&session, host_id, SessionId::from(1), None, ctx);
        });

        app.read(|ctx| {
            assert_eq!(
                HostRegistryModel::as_ref(ctx).hosts().count(),
                0,
                "a session with no parsed ssh target must not fabricate a registry entry"
            );
        });
    });
}

/// `record_host_reached(.., None, ..)` -- the `SessionReconnected` call site's shape -- must
/// still record reachability but must not touch install state at all, since a reconnect
/// carries no new version information (see the doc comment on `record_host_reached`).
/// Breaks if a `None` `server_version` were ever recorded as `Installed { version: None }`
/// (clobbering a previously-known version) rather than leaving `install_state` untouched.
#[cfg(feature = "local_tty")]
#[test]
fn host_reached_without_a_server_version_does_not_touch_install_state() {
    use crate::remote_server::host_registry::{HostInstallState, HostRegistryModel};
    use crate::remote_server::manager::RemoteServerManager;
    use crate::terminal::model::terminal_model::SubshellInitializationInfo;
    use crate::terminal::ssh::util::InteractiveSshCommand;
    use warp_core::HostId;

    App::test((), |mut app| async move {
        crate::test_util::settings::initialize_settings_for_tests(&mut app);
        app.add_singleton_model(HostRegistryModel::new);
        app.add_singleton_model(RemoteServerManager::new);

        let mut session_info = SessionInfo::new_for_test();
        session_info.subshell_info = Some(SubshellInitializationInfo {
            spawning_command: "ssh build-box".to_string(),
            was_triggered_by_rc_file_snippet: false,
            env_var_collection_name: None,
            ssh_connection_info: Some(InteractiveSshCommand {
                host: Some("build-box".to_string()),
                port: None,
            }),
        });
        let session = Session::new(session_info, Arc::new(TestCommandExecutor::default()));
        let host_id = HostId::new("host-abc".to_string());

        HostRegistryModel::handle(&app).update(&mut app, |registry, ctx| {
            registry.record_install_state(
                "build-box",
                HostInstallState::Installed {
                    version: Some("9.9.9".to_string()),
                },
                ctx,
            );
        });

        let sessions_handle = app.add_model(|_| Sessions::new_for_test());
        sessions_handle.update(&mut app, |_sessions, ctx| {
            super::record_host_reached(&session, host_id, SessionId::from(1), None, ctx);
        });

        app.read(|ctx| {
            let entry = HostRegistryModel::as_ref(ctx)
                .host("build-box")
                .expect("recording a reached host must create/update its registry entry");
            assert_eq!(
                entry.install_state,
                HostInstallState::Installed {
                    version: Some("9.9.9".to_string())
                },
                "a None server_version must never clobber a previously-recorded version"
            );
        });
    });
}

/// A host reached with NO reported version and NO prior install state must still record
/// `Installed { version: None }`, not stay `Unknown`. Completing the handshake is itself
/// proof the daemon is installed there, and a dashboard that showed `Unknown` for a host
/// the app is actively talking to would be lying about a live connection.
///
/// This is the ordering the sibling test above does not cover. Today `SessionConnected`
/// (with a version) always precedes `SessionReconnected` (without one), so the gap is
/// unreachable in production -- but that is a property of emission order in `manager.rs`,
/// not of this function, and nothing else asserts it. Breaks if `record_host_reached`
/// returns to skipping `record_install_state` whenever `server_version` is `None`.
#[cfg(feature = "local_tty")]
#[test]
fn host_reached_without_a_server_version_still_records_installed_when_nothing_is_known() {
    use crate::remote_server::host_registry::{HostInstallState, HostRegistryModel};
    use crate::remote_server::manager::RemoteServerManager;
    use crate::terminal::model::terminal_model::SubshellInitializationInfo;
    use crate::terminal::ssh::util::InteractiveSshCommand;
    use warp_core::HostId;

    App::test((), |mut app| async move {
        crate::test_util::settings::initialize_settings_for_tests(&mut app);
        app.add_singleton_model(HostRegistryModel::new);
        app.add_singleton_model(RemoteServerManager::new);

        let mut session_info = SessionInfo::new_for_test();
        session_info.subshell_info = Some(SubshellInitializationInfo {
            spawning_command: "ssh build-box".to_string(),
            was_triggered_by_rc_file_snippet: false,
            env_var_collection_name: None,
            ssh_connection_info: Some(InteractiveSshCommand {
                host: Some("build-box".to_string()),
                port: None,
            }),
        });
        let session = Session::new(session_info, Arc::new(TestCommandExecutor::default()));

        // Deliberately no `record_install_state` first: this host has never been probed.
        let sessions_handle = app.add_model(|_| Sessions::new_for_test());
        sessions_handle.update(&mut app, |_sessions, ctx| {
            super::record_host_reached(
                &session,
                HostId::new("host-abc".to_string()),
                SessionId::from(1),
                None,
                ctx,
            );
        });

        app.read(|ctx| {
            let entry = HostRegistryModel::as_ref(ctx)
                .host("build-box")
                .expect("recording a reached host must create/update its registry entry");
            assert_eq!(
                entry.install_state,
                HostInstallState::Installed { version: None },
                "reaching a host proves the daemon is installed, even with no version"
            );
        });
    });
}

/// An empty `server_version` string (the proto3 default) must record `Installed { version:
/// None }`, never an empty-string version -- the type's own convention for "installed but
/// version not known". Breaks if the `is_empty` guard in `record_host_reached` were removed.
#[cfg(feature = "local_tty")]
#[test]
fn host_reached_with_empty_server_version_records_none() {
    use crate::remote_server::host_registry::{HostInstallState, HostRegistryModel};
    use crate::remote_server::manager::RemoteServerManager;
    use crate::terminal::model::terminal_model::SubshellInitializationInfo;
    use crate::terminal::ssh::util::InteractiveSshCommand;
    use warp_core::HostId;

    App::test((), |mut app| async move {
        crate::test_util::settings::initialize_settings_for_tests(&mut app);
        app.add_singleton_model(HostRegistryModel::new);
        app.add_singleton_model(RemoteServerManager::new);

        let mut session_info = SessionInfo::new_for_test();
        session_info.subshell_info = Some(SubshellInitializationInfo {
            spawning_command: "ssh build-box".to_string(),
            was_triggered_by_rc_file_snippet: false,
            env_var_collection_name: None,
            ssh_connection_info: Some(InteractiveSshCommand {
                host: Some("build-box".to_string()),
                port: None,
            }),
        });
        let session = Session::new(session_info, Arc::new(TestCommandExecutor::default()));
        let host_id = HostId::new("host-abc".to_string());

        let sessions_handle = app.add_model(|_| Sessions::new_for_test());
        sessions_handle.update(&mut app, |_sessions, ctx| {
            super::record_host_reached(
                &session,
                host_id,
                SessionId::from(1),
                Some(String::new()),
                ctx,
            );
        });

        app.read(|ctx| {
            let entry = HostRegistryModel::as_ref(ctx)
                .host("build-box")
                .expect("recording a reached host must create/update its registry entry");
            assert_eq!(
                entry.install_state,
                HostInstallState::Installed { version: None },
                "an empty server_version must record Installed with version: None"
            );
        });
    });
}

/// A `Remote` session must be treated the same as `WarpifiedRemote` by
/// `is_subshell_or_ssh` -- it is exactly as far from "a plain local top-level shell" as a
/// discovered-remote session is. Breaks if the `SessionType::Remote` arm is dropped from the
/// match inside `is_subshell_or_ssh` (a non-exhaustive-match compile error) or wired to
/// `false`.
#[test]
fn remote_session_type_counts_as_subshell_or_ssh() {
    let session = Session::new(
        SessionInfo::new_for_test(),
        Arc::new(TestCommandExecutor::default()),
    );
    session.set_session_type_for_test(SessionType::Remote { host_id: None });
    assert!(session.is_subshell_or_ssh());
}

/// `set_remote_host_id` must update a `Remote` session's `host_id` the same way it already
/// does a `WarpifiedRemote` session's. Breaks if the `SessionType::Remote` arm is dropped
/// from its match (a non-exhaustive-match compile error) or left unwired, in which case the
/// `host_id` would stay `None` forever.
#[test]
fn set_remote_host_id_updates_remote_session_type() {
    let session = Session::new(
        SessionInfo::new_for_test(),
        Arc::new(TestCommandExecutor::default()),
    );
    session.set_session_type_for_test(SessionType::Remote { host_id: None });

    let host_id = warp_core::HostId::new("host-1".to_string());
    session.set_remote_host_id(Some(host_id.clone()));

    assert_eq!(
        session.session_type(),
        SessionType::Remote {
            host_id: Some(host_id)
        }
    );
}

/// A `Remote` session's `read_history` must never pipe a command through a live shell to
/// read history -- there is none to pipe into (the daemon owns the pty). Breaks if the
/// `SessionType::Remote` arm of `read_history` is changed to call
/// `read_history_for_remote_session` (the `WarpifiedRemote` in-band `cat` path) instead of
/// `read_history_for_remote_server_session` (the out-of-band RPC path).
#[test]
fn read_history_for_remote_session_type_injects_no_shell_command() {
    App::test((), |_app| async move {
        let executor = RecordingCommandExecutor::succeeding("some_history_line\n");
        let info = SessionInfo::new_for_test().with_histfile(Some("/tmp/history".to_string()));
        let session = Session::new(info, executor.clone());
        session.set_session_type_for_test(SessionType::Remote { host_id: None });

        let history = session.read_history(false).await;

        assert!(
            history.is_empty(),
            "no remote-server client is wired for this test session, so the RPC read must \
             fail closed rather than silently succeed"
        );
        assert!(
            executor.recorded().is_empty(),
            "a Remote session must never pipe a `cat` command through a live shell to read \
             history"
        );
    });
}
