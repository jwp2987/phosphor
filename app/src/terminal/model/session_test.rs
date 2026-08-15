use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use smol_str::SmolStr;
use warp_completer::completer::{CommandExitStatus, CommandOutput};
use warpui::{
    elements::Empty, platform::WindowStyle, App, AppContext, Element, Entity, ModelHandle,
    TypedActionView, View, ViewContext,
};

use crate::terminal::shell::{Shell, ShellType};

use super::command_executor::testing::TestCommandExecutor;
use super::{
    BootstrapSessionType, CommandExecutor, ExecuteCommandOptions, Session, SessionId, SessionInfo,
    Sessions, SessionsEvent,
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
// tests of its own, so these are new rather than ported. They pin down the two
// properties the port has to hold: the loaders are a no-op for every shell
// whose bootstrap already reports the complete set, and for the one shell that
// needs them the deferred names reach `function_names` / `builtin_names` /
// `top_level_commands` without duplicating the bootstrap snapshot.

/// A `CommandExecutor` that records what it was asked to run and answers with
/// canned output, so the deferred loaders can be driven without a live shell.
#[derive(Debug)]
struct RecordingCommandExecutor {
    stdout: String,
    status: CommandExitStatus,
    commands: Mutex<Vec<String>>,
}

impl RecordingCommandExecutor {
    fn succeeding(stdout: &str) -> Arc<Self> {
        Arc::new(Self {
            stdout: stdout.to_owned(),
            status: CommandExitStatus::Success,
            commands: Mutex::new(Vec::new()),
        })
    }

    fn failing() -> Arc<Self> {
        Arc::new(Self {
            stdout: String::new(),
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
        // did not carry should be added.
        let executor = RecordingCommandExecutor::succeeding("prompt\nInvoke-Custom\nStart-Thing\n");
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
        let executor = RecordingCommandExecutor::succeeding("Get-Item\nGet-Custom\n");
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

        assert_eq!(sorted(session.function_names()), vec!["prompt"]);
        assert_eq!(sorted(session.builtin_names()), vec!["Get-Item"]);
    });
}
