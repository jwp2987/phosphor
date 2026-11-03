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
        // which builds `tail -n N '{escaped_path}'` and executes it via TestCommandExecutor
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

fn session_with_known_names(shell_type: ShellType) -> Arc<Session> {
    let info = SessionInfo::new_for_test()
        .with_shell_type(shell_type)
        .with_aliases(HashMap::from([(SmolStr::from("ll"), "ls -l".to_owned())]))
        .with_function_names(names(&["myfunc"]))
        .with_builtins(names(&["cd", "echo"]));
    Arc::new(Session::new(info, RecordingCommandExecutor::failing()))
}

#[test]
fn knows_command_word_covers_aliases_functions_builtins_and_path_executables() {
    App::test((), |_app| async move {
        let session = session_with_known_names(ShellType::Bash);
        session.set_external_commands(["git"]);
        for known in ["ll", "myfunc", "cd", "git"] {
            assert_eq!(session.knows_command_word(known), Some(true), "{known}");
        }
        for unknown in ["summarize", "frobnicate", "tell"] {
            assert_eq!(
                session.knows_command_word(unknown),
                Some(false),
                "{unknown}"
            );
        }
    });
}

#[test]
fn knows_command_word_declines_to_answer_without_session_data() {
    App::test((), |_app| async move {
        // The $PATH scan has not landed.
        let session = session_with_known_names(ShellType::Bash);
        assert_eq!(session.knows_command_word("summarize"), None);

        // The bootstrap carried no builtin list.
        let info = SessionInfo::new_for_test().with_shell_type(ShellType::Zsh);
        let session = Arc::new(Session::new(info, RecordingCommandExecutor::failing()));
        session.set_external_commands(["git"]);
        assert_eq!(session.knows_command_word("summarize"), None);
    });
}

#[test]
fn knows_command_word_goes_stale_after_a_command_that_may_change_the_command_set() {
    App::test((), |_app| async move {
        // Commands that cannot add a known command leave the snapshot trusted.
        let session = session_with_known_names(ShellType::Bash);
        session.set_external_commands(["git"]);
        for harmless in [
            "ls -la",
            "git status",
            "echo hello world",
            "grep -r foo src",
        ] {
            session.note_user_command(harmless);
            assert!(session.command_data_is_current(), "{harmless}");
            assert_eq!(
                session.knows_command_word("newtool"),
                Some(false),
                "{harmless}"
            );
        }
        // Each of these may have installed a tool, changed PATH or defined a function/alias,
        // after which `newtool` can no longer be called unknown.
        for changing in [
            "pip install newtool",
            "sudo apt install newtool",
            "cargo install newtool",
            "npm i -g newtool",
            "brew install newtool",
            "source venv/bin/activate",
            ". ./env.sh",
            "export PATH=$HOME/bin:$PATH",
            "PATH=/opt/x:$PATH true",
            "alias nt=newtool",
            "cd ~/project",
            "make && ls",
            "eval \"$(direnv export bash)\"",
        ] {
            let session = session_with_known_names(ShellType::Bash);
            session.set_external_commands(["git"]);
            assert_eq!(session.knows_command_word("newtool"), Some(false));
            session.note_user_command(changing);
            assert!(!session.command_data_is_current(), "{changing}");
            // Stale data stops an unlisted name being called unknown ...
            assert_eq!(session.knows_command_word("newtool"), None, "{changing}");
            // ... but never un-knows a name the snapshot has (#933): an installed command, alias,
            // function or builtin stays known, so `nice grep this the file` after a `make` still
            // reads as a command.
            assert_eq!(session.knows_command_word("ll"), Some(true), "{changing}");
            assert_eq!(session.knows_command_word("git"), Some(true), "{changing}");
        }
    });
}

// #1019: the bootstrap `$PATH` scan is a one-time snapshot; a program installed afterwards must
// still become a known command once the user types it.
#[cfg(unix)]
mod late_commands {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn session_with_path(path: &std::path::Path) -> Arc<Session> {
        let info = SessionInfo::new_for_test()
            .with_shell_type(ShellType::Bash)
            .with_builtins(names(&["cd", "echo"]))
            .with_path(Some(path.display().to_string()));
        let session = Arc::new(Session::new(info, RecordingCommandExecutor::failing()));
        session.set_external_commands(["git"]);
        session
    }

    fn install(dir: &std::path::Path, name: &str, mode: u32) {
        let file = dir.join(name);
        std::fs::write(&file, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode)).unwrap();
    }

    #[test]
    fn an_executable_installed_after_bootstrap_becomes_a_known_command() {
        App::test((), |_app| async move {
            let dir = tempfile::tempdir().unwrap();
            let session = session_with_path(dir.path());
            session.discover_command_word("zorkle");
            assert_eq!(session.knows_command_word("zorkle"), Some(false));
            assert!(!session.top_level_commands().any(|c| c == "zorkle"));

            install(dir.path(), "zorkle", 0o755);
            // The earlier miss is remembered until a command completes ...
            session.discover_command_word("zorkle");
            assert_eq!(session.knows_command_word("zorkle"), Some(false));
            // ... until a command that may have installed something runs.
            session.note_user_command("chmod +x ~/bin/zorkle");
            session.discover_command_word("zorkle");
            assert_eq!(session.knows_command_word("zorkle"), Some(true));
            assert!(session.top_level_commands().any(|c| c == "zorkle"));
            // Bootstrap-known names are untouched.
            assert_eq!(session.knows_command_word("git"), Some(true));
        });
    }

    /// #1381: a miss is not a 3 s timer. It holds as long as nothing ran, however long that is,
    /// and ends at the next completed command, even a harmless-looking one (a build or an
    /// install script can create an executable).
    #[test]
    fn a_miss_lasts_until_the_next_command_completes_not_for_a_fixed_time() {
        App::test((), |_app| async move {
            let dir = tempfile::tempdir().unwrap();
            let session = session_with_path(dir.path());
            session.discover_command_word("zorkle");
            install(dir.path(), "zorkle", 0o755);
            assert!(!session.command_word_needs_probe("zorkle"));
            session.note_user_command("make-it-somehow");
            assert!(session.command_word_needs_probe("zorkle"));
            session.discover_command_word("zorkle");
            assert_eq!(session.knows_command_word("zorkle"), Some(true));
        });
    }

    #[test]
    fn needs_probe_is_a_pure_cache_check_that_the_background_probe_satisfies() {
        App::test((), |_app| async move {
            let dir = tempfile::tempdir().unwrap();
            install(dir.path(), "zorkle", 0o755);
            let session = session_with_path(dir.path());
            // Asking (the Enter path) does not stat, record a miss, or find the program ...
            assert!(session.command_word_needs_probe("zorkle"));
            assert!(session.command_word_needs_probe("zorkle"));
            assert!(!session.top_level_commands().any(|c| c == "zorkle"));
            // ... known and non-candidate words never need one ...
            assert!(!session.command_word_needs_probe("git"));
            assert!(!session.command_word_needs_probe("./zorkle"));
            // ... and the probe (run off-thread by Enter) finds it and settles the question.
            session.probe_command_word("zorkle");
            assert!(!session.command_word_needs_probe("zorkle"));
            assert_eq!(session.knows_command_word("zorkle"), Some(true));

            // A miss is cached, so the same unknown word is not probed again.
            assert!(session.command_word_needs_probe("nope"));
            session.probe_command_word("nope");
            assert!(!session.command_word_needs_probe("nope"));
        });
    }

    #[test]
    fn only_executable_files_named_by_a_bare_word_are_found() {
        App::test((), |_app| async move {
            let dir = tempfile::tempdir().unwrap();
            install(dir.path(), "plainfile", 0o644);
            std::fs::create_dir(dir.path().join("adir")).unwrap();
            install(dir.path(), "real", 0o755);
            let session = session_with_path(dir.path());
            let outside = dir.path().join("real").display().to_string();
            for word in [
                "plainfile",
                "adir",
                "./real",
                "../real",
                outside.as_str(),
                "re/al",
                "-real",
                "",
                "$(real)",
            ] {
                session.discover_command_word(word);
            }
            assert!(
                !session
                    .top_level_commands()
                    .any(|c| c == "real" || c == outside)
            );
            assert_eq!(session.knows_command_word("plainfile"), Some(false));
            session.discover_command_word("real");
            assert_eq!(session.knows_command_word("real"), Some(true));
        });
    }

    #[test]
    fn the_buffer_form_looks_up_a_finished_first_word_only() {
        App::test((), |_app| async move {
            let dir = tempfile::tempdir().unwrap();
            install(dir.path(), "zorkle", 0o755);
            let session = session_with_path(dir.path());
            session.discover_command_word_in_buffer("zorkle");
            session.discover_command_word_in_buffer("zork");
            assert_eq!(session.knows_command_word("zorkle"), Some(false));
            session.discover_command_word_in_buffer("  zorkle is great");
            assert_eq!(session.knows_command_word("zorkle"), Some(true));
        });
    }

    #[test]
    fn a_directory_added_to_path_in_the_session_is_searched() {
        App::test((), |_app| async move {
            let bootstrap_dir = tempfile::tempdir().unwrap();
            let new_dir = tempfile::tempdir().unwrap();
            install(new_dir.path(), "zorkle2", 0o755);
            let session = session_with_path(bootstrap_dir.path());
            session.discover_command_word("zorkle2");
            assert_eq!(session.knows_command_word("zorkle2"), Some(false));
            session.note_user_command(&format!("export PATH={}:$PATH", new_dir.path().display()));
            session.discover_command_word("zorkle2");
            assert_eq!(session.knows_command_word("zorkle2"), Some(true));
        });
    }

    #[test]
    fn remote_sessions_are_never_probed_locally() {
        App::test((), |_app| async move {
            let dir = tempfile::tempdir().unwrap();
            install(dir.path(), "zorkle", 0o755);
            let info = SessionInfo::new_for_test()
                .with_session_type(BootstrapSessionType::WarpifiedRemote)
                .with_shell_type(ShellType::Bash)
                .with_builtins(names(&["cd"]))
                .with_path(Some(dir.path().display().to_string()));
            let session = Arc::new(Session::new(info, RecordingCommandExecutor::failing()));
            session.set_external_commands(["git"]);
            session.discover_command_word("zorkle");
            assert!(!session.top_level_commands().any(|c| c == "zorkle"));
        });
    }

    #[test]
    fn path_assignments_yield_only_literal_absolute_directories() {
        let dirs = |command: &str| super::super::path_dirs_assigned_in(command, Some("/home/u"));
        let p = std::path::PathBuf::from;
        assert_eq!(
            dirs("export PATH=$HOME/bin:/opt/x:$PATH"),
            vec![p("/home/u/bin"), p("/opt/x")]
        );
        assert_eq!(
            dirs("PATH=\"~/.local/bin:/usr/bin\" tool"),
            vec![p("/home/u/.local/bin"), p("/usr/bin")]
        );
        assert_eq!(dirs("export PATH=${HOME}/b; ls"), vec![p("/home/u/b")]);
        // Relative, expanded-by-the-shell and other variables are not literal.
        assert!(dirs("export PATH=bin:$(pwd)/x:$HOMEX/y:$OTHER/z").is_empty());
        assert!(dirs("export MANPATH=/m GOPATH=/g").is_empty());
        assert!(super::super::path_dirs_assigned_in("export PATH=~/b", None).is_empty());
    }
}

/// A nushell session's Bootstrapped hook is turned into the same session data the other shells
/// produce: its commands are what the completer and the input classifier know about, so
/// `ls | where size > 1mb` is routed to the shell because `ls` and `where` are builtins.
#[test]
fn nu_bootstrapped_value_populates_the_session_the_classifier_and_completer_read() {
    use crate::terminal::model::ansi;

    let init_shell_value = ansi::InitShellValue {
        shell: "nu".into(),
        ..Default::default()
    };
    let bootstrapped_value = ansi::BootstrappedValue {
        shell: "nu".into(),
        shell_version: Some("0.116.1".into()),
        builtins: Some("ls\nwhere\neach\nopen\nget\nselect\nhttp\nstr".into()),
        function_names: Some("my-command\ndeploy".into()),
        aliases: Some("ll -> ls -l\ngs -> git status".into()),
        env_var_names: Some("PATH\nHOME\nMY VAR".into()),
        shell_options: Some(HashSet::from(["ignore_space_prefixed".to_owned()])),
        ..Default::default()
    };
    let info = SessionInfo::create_pending(
        ShellType::from_name("nu").unwrap(),
        init_shell_value,
        None,
        None,
        None,
        false,
        None,
    )
    .merge_from_bootstrapped_value(bootstrapped_value, false);

    assert_eq!(info.shell.shell_type(), ShellType::Nu);
    for builtin in ["ls", "where", "each", "open", "get", "select", "http"] {
        assert!(
            info.builtins.contains(builtin),
            "{builtin} is not a builtin"
        );
    }
    assert!(info.function_names.contains("deploy"));
    assert_eq!(info.aliases.get("ll").map(String::as_str), Some("ls -l"));
    assert_eq!(
        info.aliases.get("gs").map(String::as_str),
        Some("git status")
    );
    // One name per line, so a name with a space survives.
    assert!(info.environment_variable_names.contains("MY VAR"));
    assert!(info.environment_variable_names.contains("PATH"));
    // The history option the bootstrap reported decides whether space-led lines are recorded.
    assert!(!info.shell.should_add_command_to_history(" ls"));
    assert!(info.shell.should_add_command_to_history("ls"));
    assert!(info.shell.supports_autocd());
}

/// Like bash, zsh and fish, nushell reports its function and builtin names at bootstrap, so no
/// second in-band enumeration is run.
#[test]
fn nu_enumerates_functions_and_builtins_during_bootstrap() {
    App::test((), |_app| async move {
        let executor = RecordingCommandExecutor::succeeding("should_never_be_read\n");
        let session = session_for_shell(ShellType::Nu, executor.clone());

        session.load_all_function_names().await;
        session.load_all_builtins().await;

        assert!(executor.recorded().is_empty());
        assert_eq!(sorted(session.function_names()), vec!["prompt"]);
        assert_eq!(sorted(session.builtin_names()), vec!["Get-Item"]);
    });
}

/// Completion quoting follows the session's shell: a nushell session quotes, never
/// backslash-escapes (#984).
#[test]
fn nu_session_quotes_paths_in_the_nushell_dialect() {
    let info = SessionInfo::new_for_test().with_shell_type(ShellType::Nu);
    let session = Session::new(info, Arc::new(TestCommandExecutor::default()));
    assert_eq!(session.shell_dialect(), warp_util::path::ShellDialect::Nu);
    assert_eq!(
        session.shell_dialect().escape("dir with spaces/"),
        "\"dir with spaces/\""
    );
}

#[cfg(feature = "local_tty")]
mod control_master_fallback {
    use std::path::PathBuf;
    use std::sync::Arc;

    use super::super::command_executor::RemoteCommandExecutor;
    use super::super::command_executor::testing::{
        TestCommandExecutor, TestRemoteServerKindExecutor,
    };
    use super::super::{Session, SessionInfo};

    fn is_control_master_executor(session: &Session) -> bool {
        session
            .command_executor
            .read()
            .as_ref()
            .as_any()
            .downcast_ref::<RemoteCommandExecutor>()
            .is_some()
    }

    #[test]
    fn losing_the_remote_server_swaps_an_ssh_session_to_the_control_master() {
        let info = SessionInfo::new_for_test().with_ssh_socket_path(PathBuf::from("/tmp/ctl.sock"));
        let session = Session::new(info, Arc::new(TestRemoteServerKindExecutor::default()));
        assert!(!is_control_master_executor(&session));

        assert!(session.fall_back_to_control_master_executor());
        assert!(is_control_master_executor(&session));

        // A second disconnect-style event is a no-op, not a second swap.
        assert!(!session.fall_back_to_control_master_executor());
        assert!(is_control_master_executor(&session));
    }

    #[test]
    fn a_non_helper_executor_is_never_swapped_to_the_control_master() {
        let info = SessionInfo::new_for_test().with_ssh_socket_path(PathBuf::from("/tmp/ctl.sock"));
        let session = Session::new(info, Arc::new(TestCommandExecutor::default()));
        assert!(!session.fall_back_to_control_master_executor());
        assert!(!is_control_master_executor(&session));
        assert!(
            session
                .command_executor
                .read()
                .as_ref()
                .as_any()
                .downcast_ref::<TestCommandExecutor>()
                .is_some()
        );
    }

    #[test]
    fn a_non_ssh_session_keeps_its_executor() {
        let session = Session::new(
            SessionInfo::new_for_test(),
            Arc::new(TestCommandExecutor::default()),
        );
        assert!(!session.fall_back_to_control_master_executor());
        assert!(!is_control_master_executor(&session));
    }
}

#[test]
fn git_corrections_skipped_only_for_existing_dirs_outside_a_repo() {
    let tmp = tempfile::tempdir().unwrap();
    let plain = tmp.path().join("plain");
    std::fs::create_dir(&plain).unwrap();
    assert!(super::local_dir_is_outside_git_repo(&plain));

    // Inside a repo (including nested directories) git must still be consulted.
    let repo = tmp.path().join("repo");
    let nested = repo.join("a/b");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::create_dir(repo.join(".git")).unwrap();
    assert!(!super::local_dir_is_outside_git_repo(&repo));
    assert!(!super::local_dir_is_outside_git_repo(&nested));

    // A worktree/submodule `.git` file counts too.
    let wt = tmp.path().join("wt");
    std::fs::create_dir(&wt).unwrap();
    std::fs::write(wt.join(".git"), "gitdir: elsewhere").unwrap();
    assert!(!super::local_dir_is_outside_git_repo(&wt));

    // Paths we can't inspect fall back to running git.
    assert!(!super::local_dir_is_outside_git_repo(&tmp.path().join("missing")));
}

/// #1203: nushell's sqlite history (its default `file_format`) is read as the shell wrote it,
/// oldest first, multi-line entries intact; a file that is not such a database yields nothing.
#[test]
fn nu_sqlite_history_is_read_in_order() {
    let tmp = tempfile::tempdir().unwrap();
    let database = tmp.path().join("history.sqlite3");
    let database = database.to_str().unwrap().to_owned();
    {
        let mut connection =
            <diesel::sqlite::SqliteConnection as diesel::Connection>::establish(&database).unwrap();
        diesel::connection::SimpleConnection::batch_execute(
            &mut connection,
            "CREATE TABLE history (id integer primary key autoincrement, command_line text not \
             null, start_timestamp integer, session_id integer, hostname text, cwd text, \
             duration_ms integer, exit_status integer, more_info text) strict;
             INSERT INTO history (command_line, exit_status) VALUES ('ls | where size > 1kb', 0), \
             ('git status', 128), ('def f [] {' || char(10) || ' echo multi' || char(10) || '}', 0);",
        )
        .unwrap();
    }
    assert_eq!(
        super::read_nu_sqlite_history(&database),
        Some(vec![
            "ls | where size > 1kb".to_owned(),
            "git status".to_owned(),
            "def f [] {\n echo multi\n}".to_owned(),
        ])
    );
    let plain = tmp.path().join("history.txt");
    std::fs::write(&plain, "ls\n").unwrap();
    assert_eq!(
        super::read_nu_sqlite_history(plain.to_str().unwrap()),
        None,
        "not a database: nothing is loaded, the plaintext path handles it"
    );
}

#[test]
fn deferred_name_enumeration_is_redundant_only_for_a_local_powershell_session() {
    App::test((), |_app| async move {
        // #1230: a local PowerShell session would start two `pwsh -NoProfile` processes to list
        // what the Bootstrapped hook already carried.
        let local = |shell_type| {
            let info = SessionInfo::new_for_test().with_shell_type(shell_type);
            Session::new(
                info,
                Arc::new(super::command_executor::LocalCommandExecutor::new(None, shell_type)),
            )
        };
        assert!(local(ShellType::PowerShell).deferred_name_enumeration_is_redundant());
        assert!(!local(ShellType::Bash).deferred_name_enumeration_is_redundant());

        // Any other executor (in-band, remote, ...) keeps the enumeration.
        let session = session_for_shell(
            ShellType::PowerShell,
            RecordingCommandExecutor::succeeding(""),
        );
        assert!(!session.deferred_name_enumeration_is_redundant());
    });
}

/// #1239: a subshell Phosphorized inside an ssh session (`sudo -s`, `ksu`) runs on that session's
/// host, but the remote-server manager only ever names the ssh session itself. The subshell takes
/// the host id of its nearest ancestor that has one, and follows that ancestor's connection.
mod subshell_remote_host {
    use std::path::PathBuf;

    use warp_core::HostId;

    use super::*;

    const SSH: u64 = 100;
    const SUDO: u64 = 101;
    const SU: u64 = 102;
    const OTHER: u64 = 200;

    fn host() -> HostId {
        HostId::new("host-1239".to_owned())
    }

    /// `ssh` -> `sudo -s` -> `su -`, plus an unrelated remote session.
    fn sessions() -> Sessions {
        let mut sessions = Sessions::new_for_test();
        sessions.register_session_for_test(
            SessionInfo::new_for_test()
                .with_id(SSH)
                .with_ssh_socket_path(PathBuf::from("/tmp/ctl.sock")),
        );
        sessions.register_session_for_test(
            SessionInfo::new_for_test()
                .with_id(SUDO)
                .with_session_type(BootstrapSessionType::WarpifiedRemote)
                .with_spawning_session_id(SSH.into())
                .with_subshell_spawned_by("sudo -s"),
        );
        sessions.register_session_for_test(
            SessionInfo::new_for_test()
                .with_id(SU)
                .with_session_type(BootstrapSessionType::WarpifiedRemote)
                .with_spawning_session_id(SUDO.into())
                .with_subshell_spawned_by("su -"),
        );
        sessions.register_session_for_test(
            SessionInfo::new_for_test()
                .with_id(OTHER)
                .with_session_type(BootstrapSessionType::WarpifiedRemote),
        );
        sessions
    }

    fn host_of(sessions: &Sessions, id: u64) -> Option<HostId> {
        sessions.get(id.into()).unwrap().remote_host_id()
    }

    #[test]
    fn a_subshell_inherits_the_host_of_the_nearest_connected_ancestor() {
        let sessions = sessions();
        assert_eq!(sessions.inherited_remote_host_id(Some(SSH.into())), None);

        sessions.get(SSH.into()).unwrap().set_remote_host_id(Some(host()));
        // The direct child and the grandchild both resolve to the ssh session's host.
        assert_eq!(sessions.inherited_remote_host_id(Some(SSH.into())), Some(host()));
        assert_eq!(sessions.inherited_remote_host_id(Some(SUDO.into())), Some(host()));
        // No chain, or a chain that leads to a session without a host, resolves to nothing.
        assert_eq!(sessions.inherited_remote_host_id(None), None);
        assert_eq!(sessions.inherited_remote_host_id(Some(OTHER.into())), None);
    }

    #[test]
    fn the_ssh_sessions_connection_events_reach_its_subshells_only() {
        let sessions = sessions();
        sessions.set_remote_host_id_for_descendants(SSH.into(), Some(host()));
        assert_eq!(host_of(&sessions, SUDO), Some(host()));
        assert_eq!(host_of(&sessions, SU), Some(host()));
        // The root is the caller's business, and an unrelated session is untouched.
        assert_eq!(host_of(&sessions, SSH), None);
        assert_eq!(host_of(&sessions, OTHER), None);

        // A lost connection clears the whole chain below the ssh session.
        sessions.set_remote_host_id_for_descendants(SSH.into(), None);
        assert_eq!(host_of(&sessions, SUDO), None);
        assert_eq!(host_of(&sessions, SU), None);
    }

    #[test]
    fn a_cycle_in_the_spawning_chain_terminates() {
        let mut sessions = Sessions::new_for_test();
        sessions.register_session_for_test(
            SessionInfo::new_for_test()
                .with_id(SUDO)
                .with_session_type(BootstrapSessionType::WarpifiedRemote)
                .with_spawning_session_id(SU.into()),
        );
        sessions.register_session_for_test(
            SessionInfo::new_for_test()
                .with_id(SU)
                .with_session_type(BootstrapSessionType::WarpifiedRemote)
                .with_spawning_session_id(SUDO.into()),
        );
        assert_eq!(sessions.inherited_remote_host_id(Some(SUDO.into())), None);
        sessions.set_remote_host_id_for_descendants(OTHER.into(), Some(host()));
        assert_eq!(host_of(&sessions, SUDO), None);
    }
}

/// #1242: a remote history is read as its newest lines, not whole, unless the override says so.
#[test]
fn remote_history_is_read_as_a_bounded_tail() {
    use super::{remote_history_max_lines_from_env, remote_history_read_command};

    assert_eq!(remote_history_max_lines_from_env(None), Some(50_000));
    assert_eq!(remote_history_max_lines_from_env(Some("abc")), Some(50_000));
    assert_eq!(remote_history_max_lines_from_env(Some(" 1000 ")), Some(1000));
    assert_eq!(remote_history_max_lines_from_env(Some("0")), None);

    // The path is passed through already quoted; the command never re-interprets it.
    let quoted = "'/home/u/.bash_history'";
    assert_eq!(
        remote_history_read_command(quoted, Some(50_000)),
        "tail -n 50000 '/home/u/.bash_history'"
    );
    assert_eq!(
        remote_history_read_command(quoted, None),
        "cat '/home/u/.bash_history'"
    );
}

/// #1230: a local PowerShell session lists the executables on its PATH itself; the shell path
/// below does not exist, so only the native scan can produce them.
#[cfg(unix)]
#[test]
fn local_powershell_session_lists_path_executables_without_running_pwsh() {
    use std::os::unix::fs::PermissionsExt as _;

    App::test((), |_app| async move {
        let bin = tempfile::tempdir().unwrap();
        for (name, mode) in [("my-tool", 0o755), ("not-exec.txt", 0o644)] {
            let path = bin.path().join(name);
            std::fs::write(&path, "#!/bin/sh\n").unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
        }
        let info = SessionInfo::new_for_test()
            .with_shell_type(ShellType::PowerShell)
            .with_path(Some(bin.path().display().to_string()));
        let session = Session::new(
            info,
            Arc::new(super::command_executor::LocalCommandExecutor::new(
                Some("/nonexistent/pwsh".into()),
                ShellType::PowerShell,
            )),
        );

        session.load_external_commands().await;

        assert!(session.has_loaded_external_commands());
        let commands = sorted(session.top_level_commands());
        assert!(commands.contains(&"my-tool"));
        assert!(!commands.contains(&"not-exec.txt"));
    });
}

/// #1289: fails once the way a request does when the remote-server connection drops under it,
/// and the session's executor is swapped while it is failing.
#[derive(Debug)]
struct DropsAndSwapsExecutor {
    session: std::sync::OnceLock<Arc<Session>>,
    replacement: Arc<RecordingCommandExecutor>,
}

#[async_trait]
impl CommandExecutor for DropsAndSwapsExecutor {
    async fn execute_command(
        &self,
        _command: &str,
        _shell: &Shell,
        _current_directory_path: Option<&str>,
        _environment_variables: Option<HashMap<String, String>>,
        _execute_command_options: ExecuteCommandOptions,
    ) -> anyhow::Result<CommandOutput> {
        self.session
            .get()
            .expect("session set")
            .set_command_executor(self.replacement.clone());
        anyhow::bail!("Remote command failed: the connection was lost")
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn supports_parallel_command_execution(&self) -> bool {
        true
    }
}

#[test]
fn a_remote_command_caught_by_an_executor_swap_runs_on_the_new_executor() {
    App::test((), |_app| async move {
        let replacement = RecordingCommandExecutor::succeeding("x\n");
        let dropping = Arc::new(DropsAndSwapsExecutor {
            session: Default::default(),
            replacement: replacement.clone(),
        });
        let info = SessionInfo::new_for_test()
            .with_session_type(BootstrapSessionType::WarpifiedRemote)
            .with_shell_type(ShellType::Bash);
        let session = Arc::new(Session::new(info, dropping.clone()));
        dropping.session.set(session.clone()).unwrap();

        let output = session
            .execute_command("ls", None, None, ExecuteCommandOptions::default())
            .await
            .expect("the retry on the swapped-in executor must serve the command");
        assert_eq!(output.stdout, b"x\n");
        assert_eq!(replacement.recorded(), vec!["ls".to_owned()]);
    });
}

#[test]
fn a_local_command_that_fails_is_not_retried() {
    App::test((), |_app| async move {
        let replacement = RecordingCommandExecutor::succeeding("x\n");
        let dropping = Arc::new(DropsAndSwapsExecutor {
            session: Default::default(),
            replacement: replacement.clone(),
        });
        // Local session: no retry, the error is the caller's to handle.
        let session = Arc::new(Session::new(SessionInfo::new_for_test(), dropping.clone()));
        dropping.session.set(session.clone()).unwrap();
        assert!(
            session
                .execute_command("ls", None, None, ExecuteCommandOptions::default())
                .await
                .is_err()
        );
        assert!(replacement.recorded().is_empty());
    });
}

/// Errors on the first `failures` calls (a dropped remote connection), then lists names.
#[derive(Debug)]
struct ErrorsThenNamesExecutor {
    failures: Mutex<usize>,
}

#[async_trait]
impl CommandExecutor for ErrorsThenNamesExecutor {
    async fn execute_command(
        &self,
        _command: &str,
        _shell: &Shell,
        _current_directory_path: Option<&str>,
        _environment_variables: Option<HashMap<String, String>>,
        _execute_command_options: ExecuteCommandOptions,
    ) -> anyhow::Result<CommandOutput> {
        {
            let mut failures = self.failures.lock().unwrap();
            if *failures > 0 {
                *failures -= 1;
                anyhow::bail!("Remote command failed: the connection was lost");
            }
        }
        Ok(CommandOutput {
            stdout: b"Invoke-Custom\n".to_vec(),
            stderr: Vec::new(),
            status: CommandExitStatus::Success,
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

/// #1289: a transport error while enumerating deferred names must not be kept as an empty set
/// for the rest of the session; the load is retried and the names arrive.
#[test]
fn a_transport_error_does_not_make_the_deferred_names_permanently_empty() {
    App::test((), |_app| async move {
        let executor = Arc::new(ErrorsThenNamesExecutor {
            failures: Mutex::new(1),
        });
        let info = SessionInfo::new_for_test()
            .with_shell_type(ShellType::PowerShell)
            .with_function_names(names(&["prompt"]));
        let session = Arc::new(Session::new(info, executor));

        session.load_all_function_names().await;

        assert_eq!(
            sorted(session.function_names()),
            vec!["Invoke-Custom", "prompt"],
            "the retry after the dropped request must have stored the names"
        );
    });
}
