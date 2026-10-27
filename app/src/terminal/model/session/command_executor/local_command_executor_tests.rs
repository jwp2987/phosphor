#[cfg(unix)]
mod unix {
    use std::collections::HashMap;
    use std::fs::{self, File, OpenOptions};
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    use std::path::Path;
    use std::time::Duration;

    use async_io::Timer;
    use futures_util::future::{AbortHandle, Abortable, Aborted};
    use instant::Instant;
    use nix::sys::signal::kill;
    use nix::sys::stat::Mode;
    use nix::unistd::{Pid, mkfifo};

    use super::super::*;

    const TIMEOUT: Duration = Duration::from_secs(5);

    fn executor() -> LocalCommandExecutor {
        LocalCommandExecutor::new(Some("/bin/bash".into()), ShellType::Bash)
    }

    fn descendant_command() -> &'static str {
        "/bin/sh -c 'printf %s \"$$\" > \"$READY_FILE\"; \
         IFS= read -r _ < \"$RELEASE_FIFO\"; \
         printf descendant-ran > \"$SIDE_EFFECT_FILE\"' \
         </dev/null >/dev/null 2>&1 & wait"
    }

    fn detached_descendant_command() -> &'static str {
        "/bin/sh -c 'printf %s \"$$\" > \"$READY_FILE\"; \
         IFS= read -r _ < \"$RELEASE_FIFO\"; \
         printf descendant-ran > \"$SIDE_EFFECT_FILE\"' \
         </dev/null >/dev/null 2>&1 &"
    }

    fn command_environment(temp_dir: &Path) -> HashMap<String, String> {
        HashMap::from([
            (
                "READY_FILE".into(),
                temp_dir.join("ready").to_string_lossy().into_owned(),
            ),
            (
                "RELEASE_FIFO".into(),
                temp_dir.join("release-fifo").to_string_lossy().into_owned(),
            ),
            (
                "SIDE_EFFECT_FILE".into(),
                temp_dir.join("side-effect").to_string_lossy().into_owned(),
            ),
        ])
    }

    fn create_release_fifo(temp_dir: &Path) -> File {
        let path = temp_dir.join("release-fifo");
        mkfifo(&path, Mode::S_IRUSR | Mode::S_IWUSR).expect("create release FIFO");
        OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)
            .expect("open release FIFO")
    }

    async fn wait_for_file(path: &Path) {
        let deadline = Instant::now() + TIMEOUT;
        while !path.exists() {
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {}",
                path.display()
            );
            Timer::after(Duration::from_millis(10)).await;
        }
    }
    async fn wait_for_process_exit(pid: Pid) {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            match kill(pid, None) {
                Err(nix::errno::Errno::ESRCH) => return,
                Ok(()) | Err(nix::errno::Errno::EPERM) => {}
                Err(error) => panic!("failed to inspect descendant {pid}: {error}"),
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for descendant {pid} to exit"
            );
            Timer::after(Duration::from_millis(10)).await;
        }
    }

    async fn wait_for_descendant_pid(path: &Path) -> Pid {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Ok(contents) = fs::read_to_string(path)
                && let Ok(pid) = contents.parse()
            {
                return Pid::from_raw(pid);
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for descendant PID in {}",
                path.display()
            );
            Timer::after(Duration::from_millis(10)).await;
        }
    }

    #[test]
    fn dropping_command_future_kills_descendant_process() {
        futures_lite::future::block_on(async {
            let temp_dir = tempfile::tempdir().expect("create temp dir");
            let mut release_fifo = create_release_fifo(temp_dir.path());
            let ready_file = temp_dir.path().join("ready");
            let side_effect_file = temp_dir.path().join("side-effect");
            let executor = executor();
            let (abort_handle, abort_registration) = AbortHandle::new_pair();
            let command = executor.execute_local_command(
                descendant_command(),
                None,
                Some(command_environment(temp_dir.path())),
                ExecuteCommandOptions::default(),
            );
            let command = Abortable::new(command, abort_registration);

            let task_executor = async_executor::LocalExecutor::new();
            task_executor
                .run(async {
                    let command_task = task_executor.spawn(command);
                    let descendant_pid = wait_for_descendant_pid(&ready_file).await;

                    abort_handle.abort();
                    assert!(matches!(command_task.await, Err(Aborted)));

                    writeln!(release_fifo, "continue").expect("release descendant");
                    wait_for_process_exit(descendant_pid).await;
                    assert!(
                        !side_effect_file.exists(),
                        "canceled descendant unexpectedly created {}",
                        side_effect_file.display()
                    );
                })
                .await;
        });
    }

    #[test]
    fn cancel_active_commands_kills_descendant_process() {
        futures_lite::future::block_on(async {
            let temp_dir = tempfile::tempdir().expect("create temp dir");
            let mut release_fifo = create_release_fifo(temp_dir.path());
            let ready_file = temp_dir.path().join("ready");
            let side_effect_file = temp_dir.path().join("side-effect");
            let executor = executor();
            let command = executor.execute_local_command(
                descendant_command(),
                None,
                Some(command_environment(temp_dir.path())),
                ExecuteCommandOptions::default(),
            );

            let task_executor = async_executor::LocalExecutor::new();
            task_executor
                .run(async {
                    let command_task = task_executor.spawn(command);
                    let descendant_pid = wait_for_descendant_pid(&ready_file).await;

                    executor.cancel_active_commands();
                    let _ = command_task.await;

                    writeln!(release_fifo, "continue").expect("release descendant");
                    wait_for_process_exit(descendant_pid).await;
                    assert!(
                        !side_effect_file.exists(),
                        "canceled descendant unexpectedly created {}",
                        side_effect_file.display()
                    );
                })
                .await;
        });
    }

    #[test]
    fn completed_command_is_not_canceled_later() {
        futures_lite::future::block_on(async {
            let temp_dir = tempfile::tempdir().expect("create temp dir");
            let mut release_fifo = create_release_fifo(temp_dir.path());
            let ready_file = temp_dir.path().join("ready");
            let side_effect_file = temp_dir.path().join("side-effect");
            let executor = executor();

            executor
                .execute_local_command(
                    detached_descendant_command(),
                    None,
                    Some(command_environment(temp_dir.path())),
                    ExecuteCommandOptions::default(),
                )
                .await
                .expect("complete wrapper command");
            wait_for_file(&ready_file).await;

            executor.cancel_active_commands();
            writeln!(release_fifo, "continue").expect("release descendant");
            wait_for_file(&side_effect_file).await;
            assert_eq!(
                fs::read_to_string(side_effect_file).expect("read side effect"),
                "descendant-ran"
            );
        });
    }

    /// Commands Phosphor runs for itself are POSIX syntax. nushell cannot parse them, so for a
    /// nushell session they go through bash; the nushell binary is not even started.
    #[test]
    fn nu_session_commands_are_posix_syntax_run_by_bash() {
        futures_lite::future::block_on(async {
            let executor =
                LocalCommandExecutor::new(Some("/nonexistent/path/to/nu".into()), ShellType::Nu);
            let output = executor
                .execute_local_command(
                    "echo \"$((1 + 2)) $FOO\" && test -d /tmp",
                    Some("/tmp"),
                    Some(HashMap::from([("FOO".to_owned(), "bar baz".to_owned())])),
                    ExecuteCommandOptions::default(),
                )
                .await
                .expect("a POSIX command should run for a nushell session");
            assert!(output.success());
            assert_eq!(String::from_utf8_lossy(output.output()).trim(), "3 bar baz");

            // A syntax error is the shell's, not a nushell parse error about `$((`.
            let failing = executor
                .execute_local_command("exit 4", None, None, ExecuteCommandOptions::default())
                .await
                .expect("the command ran");
            assert!(!failing.success());
        });
    }

    /// The login-shell variant is the user's own shell, so it takes nushell syntax.
    #[test]
    fn nu_login_shell_commands_run_in_nushell() {
        let Some(nu) = [
            "/home/linuxbrew/.linuxbrew/bin/nu",
            "/opt/homebrew/bin/nu",
            "/usr/local/bin/nu",
        ]
        .into_iter()
        .map(std::path::PathBuf::from)
        .chain(
            std::env::var_os("PATH")
                .map(|path| {
                    std::env::split_paths(&path)
                        .map(|dir| dir.join("nu"))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        )
        .find(|candidate| candidate.is_file()) else {
            eprintln!("skipping: nu is not installed");
            return;
        };
        futures_lite::future::block_on(async {
            let home = tempfile::tempdir().expect("create temp dir");
            // Existing (empty) config files: a missing one makes nushell announce that it
            // created it, on stdout.
            let config_dir = home.path().join("cfg/nushell");
            fs::create_dir_all(&config_dir).expect("create config dir");
            fs::write(config_dir.join("env.nu"), "").expect("write env.nu");
            fs::write(config_dir.join("config.nu"), "").expect("write config.nu");
            let executor = LocalCommandExecutor::new(Some(nu), ShellType::Nu);
            let output = executor
                .execute_local_command_in_login_shell(
                    "print -n ($env.FOO | str length)",
                    None,
                    Some(HashMap::from([
                        ("FOO".to_owned(), "bar".to_owned()),
                        ("HOME".to_owned(), home.path().display().to_string()),
                        (
                            "XDG_CONFIG_HOME".to_owned(),
                            home.path().join("cfg").display().to_string(),
                        ),
                    ])),
                )
                .await
                .expect("nu should run");
            assert_eq!(String::from_utf8_lossy(output.output()), "3");
        });
    }
}
