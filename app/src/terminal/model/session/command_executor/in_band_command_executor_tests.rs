use futures_util::future::{AbortHandle, Abortable, Aborted};
use warpui::App;

use crate::terminal::model::session::ExecuteCommandOptions;

use super::*;

impl InBandCommandExecutor {
    /// Returns an `Option` containing the ID of the actively running command.
    pub fn running_command_id(&self) -> Option<String> {
        self.running_command
            .lock()
            .as_ref()
            .map(|command_info| command_info.id.clone())
    }

    /// Returns a `HashSet` containing IDs of all pending commands (i.e. commands that are queued
    /// for execution but not executing yet).
    pub fn pending_command_ids(&self) -> Vec<String> {
        Vec::from_iter(
            self.pending_commands
                .lock()
                .iter()
                .map(|command_info| &command_info.id)
                .cloned(),
        )
    }
}

/// Returns a closure that asserts the given `Result<CommandOutput>` is `Ok(..)` and contains
/// `CommandOutput` with the given `expected_output` and `expected_success` values.
fn assert_command_output_result_fn(
    expected_output: &'static str,
    expected_success: bool,
) -> impl FnOnce(Result<CommandOutput>) {
    move |result| {
        assert!(result.is_ok());

        let output = result.unwrap();
        assert_eq!(output.success(), expected_success);
        assert_eq!(output.output(), expected_output.as_bytes());
    }
}

async fn execute_test_command<F>(
    executor: Arc<InBandCommandExecutor>,
    command: &'static str,
    assert_result_fn: F,
) where
    F: FnOnce(Result<CommandOutput>) + Send + 'static,
{
    let shell = Shell::new(ShellType::Zsh, None, None, Default::default(), None);
    let test_command_result = executor
        .execute_command(
            command,
            &shell,
            /*current_directory_path=*/ None,
            /*environment_variables=*/ None,
            ExecuteCommandOptions::default(),
        )
        .await;

    assert_result_fn(test_command_result);
}

#[test]
fn test_emits_successful_command_output() {
    App::test((), |_app| async move {
        let (executor_command_tx, _) = async_channel::unbounded();
        let (in_band_command_cancelled_tx, _) = async_channel::unbounded();
        let executor = Arc::new(InBandCommandExecutor::new(
            executor_command_tx,
            in_band_command_cancelled_tx,
        ));

        // TODO(zachbai): Figure out how to make these tests work without
        // requiring a single-threaded executor (e.g.: running on
        // app.background_executor()).
        let task_executor = async_executor::LocalExecutor::new();

        let execute_command_future = task_executor.spawn(execute_test_command(
            executor.clone(),
            "echo foo",
            assert_command_output_result_fn("foo", true),
        ));
        let handle_command_output_future = task_executor.spawn(async move {
            let pending_command_ids = executor.pending_command_ids();
            assert!(pending_command_ids.is_empty());

            let test_command_id = executor
                .running_command_id()
                .expect("Executor should be running test command.");
            executor.handle_executed_command_event(ExecutedExecutorCommandEvent {
                command_id: test_command_id,
                exit_code: 0,
                output: "foo".as_bytes().to_vec(),
            });
        });

        task_executor
            .run(async move {
                execute_command_future.await;
                handle_command_output_future.await;
            })
            .await;
    });
}

#[test]
fn test_emits_error_command_output() {
    App::test((), |_app| async move {
        let (executor_command_tx, _) = async_channel::unbounded();
        let (in_band_command_cancelled_tx, _) = async_channel::unbounded();
        let executor = Arc::new(InBandCommandExecutor::new(
            executor_command_tx,
            in_band_command_cancelled_tx,
        ));

        // TODO(zachbai): Figure out how to make these tests work without
        // requiring a single-threaded executor (e.g.: running on
        // app.background_executor()).
        let task_executor = async_executor::LocalExecutor::new();

        let execute_command_future = task_executor.spawn(execute_test_command(
            executor.clone(),
            "echo bar",
            assert_command_output_result_fn("failed!", false),
        ));
        let handle_command_output_future = task_executor.spawn(async move {
            let pending_command_ids = executor.pending_command_ids();
            assert!(pending_command_ids.is_empty());

            let test_command_id = executor
                .running_command_id()
                .expect("Executor should be running test command.");
            executor.handle_executed_command_event(ExecutedExecutorCommandEvent {
                command_id: test_command_id,
                exit_code: 1,
                output: "failed!".as_bytes().to_vec(),
            });
        });

        task_executor
            .run(async move {
                execute_command_future.await;
                handle_command_output_future.await;
            })
            .await;
    });
}

#[test]
fn test_runs_commands_serially() {
    App::test((), |_app| async move {
        let (executor_command_tx, _) = async_channel::unbounded();
        let (in_band_command_cancelled_tx, _) = async_channel::unbounded();
        let executor = Arc::new(InBandCommandExecutor::new(
            executor_command_tx,
            in_band_command_cancelled_tx,
        ));

        // TODO(zachbai): Figure out how to make these tests work without
        // requiring a single-threaded executor (e.g.: running on
        // app.background_executor()).
        let task_executor = async_executor::LocalExecutor::new();

        let first_execute_command_future = task_executor.spawn(execute_test_command(
            executor.clone(),
            "echo foo",
            assert_command_output_result_fn("foo", true),
        ));
        let second_execute_command_future = task_executor.spawn(execute_test_command(
            executor.clone(),
            "echo bar",
            assert_command_output_result_fn("bar", true),
        ));

        let handle_command_output_future = task_executor.spawn(async move {
            // "echo bar" is pending.
            let pending_command_ids = executor.pending_command_ids();
            assert_eq!(pending_command_ids.len(), 1);

            // "echo foo" is running.
            let test_command_id = executor
                .running_command_id()
                .expect("Executor should be running test command.");
            executor.handle_executed_command_event(ExecutedExecutorCommandEvent {
                command_id: test_command_id,
                exit_code: 0,
                output: "foo".as_bytes().to_vec(),
            });

            // "echo bar" is now running.
            assert_eq!(
                pending_command_ids[0],
                executor.running_command_id().unwrap()
            );
            executor.handle_executed_command_event(ExecutedExecutorCommandEvent {
                command_id: pending_command_ids[0].clone(),
                exit_code: 0,
                output: "bar".as_bytes().to_vec(),
            });
        });

        task_executor
            .run(async move {
                first_execute_command_future.await;
                second_execute_command_future.await;
                handle_command_output_future.await;
            })
            .await;
    });
}

#[test]
fn test_runs_commands_serially_after_failed_command() {
    App::test((), |_app| async move {
        let (executor_command_tx, _) = async_channel::unbounded();
        let (in_band_command_cancelled_tx, _) = async_channel::unbounded();
        let executor = Arc::new(InBandCommandExecutor::new(
            executor_command_tx,
            in_band_command_cancelled_tx,
        ));

        // TODO(zachbai): Figure out how to make these tests work without
        // requiring a single-threaded executor (e.g.: running on
        // app.background_executor()).
        let task_executor = async_executor::LocalExecutor::new();

        let first_execute_command_future = task_executor.spawn(execute_test_command(
            executor.clone(),
            "echo foo",
            assert_command_output_result_fn("foo", true),
        ));
        let second_execute_command_future = task_executor.spawn(execute_test_command(
            executor.clone(),
            "echo bar",
            assert_command_output_result_fn("bar", false),
        ));
        let third_execute_command_future = task_executor.spawn(execute_test_command(
            executor.clone(),
            "echo baz",
            assert_command_output_result_fn("baz", true),
        ));

        let handle_command_output_future = task_executor.spawn(async move {
            // "echo bar" is pending.
            let pending_command_ids = executor.pending_command_ids();
            assert_eq!(pending_command_ids.len(), 2);

            // "echo foo" is running.
            let test_command_id = executor
                .running_command_id()
                .expect("Executor should be running test command.");
            executor.handle_executed_command_event(ExecutedExecutorCommandEvent {
                command_id: test_command_id,
                exit_code: 0,
                output: "foo".as_bytes().to_vec(),
            });

            // "echo bar" is now running.
            assert_eq!(
                pending_command_ids[0],
                executor.running_command_id().unwrap()
            );
            executor.handle_executed_command_event(ExecutedExecutorCommandEvent {
                command_id: pending_command_ids[0].clone(),
                exit_code: 1,
                output: "bar".as_bytes().to_vec(),
            });

            // "echo baz" is now running.
            assert_eq!(
                pending_command_ids[1],
                executor.running_command_id().unwrap()
            );
            executor.handle_executed_command_event(ExecutedExecutorCommandEvent {
                command_id: pending_command_ids[1].clone(),
                exit_code: 0,
                output: "baz".as_bytes().to_vec(),
            });
        });

        task_executor
            .run(async move {
                first_execute_command_future.await;
                second_execute_command_future.await;
                third_execute_command_future.await;
                handle_command_output_future.await;
            })
            .await;
    });
}

#[test]
fn test_clears_running_and_pending_commands_on_cancellation() {
    App::test((), |_app| async move {
        fn assert_failed(result: Result<CommandOutput>) {
            assert!(result.is_err());
        }

        let (executor_command_tx, _) = async_channel::unbounded();
        let (in_band_command_cancelled_tx, _) = async_channel::unbounded();
        let executor = Arc::new(InBandCommandExecutor::new(
            executor_command_tx,
            in_band_command_cancelled_tx,
        ));

        // TODO(zachbai): Figure out how to make these tests work without
        // requiring a single-threaded executor (e.g.: running on
        // app.background_executor()).
        let task_executor = async_executor::LocalExecutor::new();

        let first_execute_command_future = task_executor.spawn(execute_test_command(
            executor.clone(),
            "echo foo",
            assert_failed,
        ));
        let second_execute_command_future = task_executor.spawn(execute_test_command(
            executor.clone(),
            "echo bar",
            assert_failed,
        ));
        let third_execute_command_future = task_executor.spawn(execute_test_command(
            executor.clone(),
            "echo baz",
            assert_failed,
        ));

        let handle_command_output_future = task_executor.spawn(async move {
            executor.cancel_active_commands();

            assert_eq!(executor.running_command_id(), None);
            assert_eq!(executor.pending_command_ids().len(), 0);
        });

        task_executor
            .run(async move {
                first_execute_command_future.await;
                second_execute_command_future.await;
                third_execute_command_future.await;
                handle_command_output_future.await;
            })
            .await;
    });
}

#[test]
fn test_commands_are_cleared_if_execute_command_future_is_aborted() {
    App::test((), |_app| async move {
        fn assert_failed(result: Result<CommandOutput>) {
            assert!(result.is_err());
        }

        let (executor_command_tx, _) = async_channel::unbounded();
        let (in_band_command_cancelled_tx, _) = async_channel::unbounded();
        let executor = Arc::new(InBandCommandExecutor::new(
            executor_command_tx,
            in_band_command_cancelled_tx,
        ));

        // TODO(zachbai): Figure out how to make these tests work without
        // requiring a single-threaded executor (e.g.: running on
        // app.background_executor()).
        let task_executor = async_executor::LocalExecutor::new();

        let executor_clone = executor.clone();
        let execute_commands_future = async move {
            execute_test_command(executor_clone.clone(), "echo foo", assert_failed).await;
            execute_test_command(executor_clone.clone(), "echo bar", assert_failed).await;
            execute_test_command(executor_clone.clone(), "echo baz", assert_failed).await;
        };

        let (handle, registration) = AbortHandle::new_pair();
        let execute_command_future =
            task_executor.spawn(Abortable::new(execute_commands_future, registration));
        // Abort the future where the commands are executing.
        handle.abort();

        let handle_command_output_future = task_executor.spawn(async move {
            assert_eq!(executor.running_command_id(), None);
            assert_eq!(executor.pending_command_ids().len(), 0);
        });

        task_executor
            .run(async move {
                assert_eq!(execute_command_future.await, Err(Aborted));
                handle_command_output_future.await;
            })
            .await;
    });
}

#[test]
fn test_commands_are_cleared_if_controller_cancels_command() {
    App::test((), |_app| async move {
        let (executor_command_tx, _) = async_channel::unbounded();
        let (in_band_command_cancelled_tx, _) = async_channel::unbounded();
        let executor = Arc::new(InBandCommandExecutor::new(
            executor_command_tx,
            in_band_command_cancelled_tx,
        ));

        let task_executor = async_executor::LocalExecutor::new();

        let execute_command_future = task_executor.spawn(execute_test_command(
            executor.clone(),
            "echo foo",
            |result| {
                let output = result.expect("Expect result to be ok");
                assert_eq!(output.status, CommandExitStatus::Failure)
            },
        ));

        let cancel_command_future = task_executor.spawn(async move {
            let current_running_id = executor
                .running_command_id()
                .expect("There should be a running command");
            executor.handle_cancelled_in_band_command_event(InBandCommandCancelledEvent {
                command_id: current_running_id,
            });
            assert_eq!(executor.running_command_id(), None);
        });

        task_executor
            .run(async move {
                execute_command_future.await;
                cancel_command_future.await;
            })
            .await;
    });
}

#[test]
fn test_commands_are_not_cleared_if_controller_cancels_different_command() {
    App::test((), |_app| async move {
        let (executor_command_tx, _) = async_channel::unbounded();
        let (in_band_command_cancelled_tx, _) = async_channel::unbounded();
        let executor = Arc::new(InBandCommandExecutor::new(
            executor_command_tx,
            in_band_command_cancelled_tx,
        ));

        let task_executor = async_executor::LocalExecutor::new();

        let execute_command_future = task_executor.spawn(execute_test_command(
            executor.clone(),
            "echo foo",
            assert_command_output_result_fn("foo", true),
        ));

        let cancel_command_future = task_executor.spawn(async move {
            let current_running_id = executor
                .running_command_id()
                .expect("There should be a running command");

            let a_different_id = format!("{current_running_id}_but_not_really");
            executor.handle_cancelled_in_band_command_event(InBandCommandCancelledEvent {
                command_id: a_different_id,
            });
            assert!(executor
                .running_command_id()
                .is_some_and(|id| id == current_running_id));

            // Resolving the command to unblock the test
            executor.handle_executed_command_event(ExecutedExecutorCommandEvent {
                command_id: current_running_id,
                exit_code: 0,
                output: "foo".as_bytes().to_vec(),
            })
        });

        task_executor
            .run(async move {
                execute_command_future.await;
                cancel_command_future.await;
            })
            .await;
    });
}

#[test]
fn test_cancelling_command_queues_up_next_command_command() {
    App::test((), |_app| async move {
        let (executor_command_tx, _) = async_channel::unbounded();
        let (in_band_command_cancelled_tx, _) = async_channel::unbounded();
        let executor = Arc::new(InBandCommandExecutor::new(
            executor_command_tx,
            in_band_command_cancelled_tx,
        ));

        let task_executor = async_executor::LocalExecutor::new();

        let execute_first_command_future = task_executor.spawn(execute_test_command(
            executor.clone(),
            "echo foo",
            |result| {
                let output = result.expect("Expect result to be ok");
                assert_eq!(output.status, CommandExitStatus::Failure)
            },
        ));
        let execute_second_command_future = task_executor.spawn(execute_test_command(
            executor.clone(),
            "echo bar",
            assert_command_output_result_fn("bar", true),
        ));

        let cancel_command_future = task_executor.spawn(async move {
            let current_running_id = executor
                .running_command_id()
                .expect("There should be a running command");

            let pending_command_id = executor
                .pending_command_ids()
                .first()
                .expect("There should be at least one pending command")
                .to_string();
            executor.handle_cancelled_in_band_command_event(InBandCommandCancelledEvent {
                command_id: current_running_id,
            });
            assert!(executor
                .running_command_id()
                .is_some_and(|id| id == pending_command_id));

            // Resolving the command to unblock the test
            executor.handle_executed_command_event(ExecutedExecutorCommandEvent {
                command_id: pending_command_id,
                exit_code: 0,
                output: "bar".as_bytes().to_vec(),
            })
        });

        task_executor
            .run(async move {
                execute_first_command_future.await;
                execute_second_command_future.await;
                cancel_command_future.await;
            })
            .await;
    });
}

/// A nushell single-quoted string cannot contain `'`, and the command is POSIX syntax full of
/// them, so it is passed as a raw string whose `#` count beats any `'#` inside it. The leading
/// space keeps the call out of nushell's history (`history.ignore_space_prefixed`).
#[test]
fn test_nu_generator_call_is_a_space_led_raw_string() {
    App::test((), |_app| async move {
        let (executor_command_tx, executor_command_rx) = async_channel::unbounded();
        let (in_band_command_cancelled_tx, _) = async_channel::unbounded();
        let executor = Arc::new(InBandCommandExecutor::new(
            executor_command_tx,
            in_band_command_cancelled_tx,
        ));
        let task_executor = async_executor::LocalExecutor::new();
        let command = "echo 'it'\"'\"'s'; echo '#' \"$HOME\" (not nu) \\";
        let shell = Shell::new(ShellType::Nu, None, None, Default::default(), None);
        let execute_future = {
            let executor = executor.clone();
            task_executor.spawn(async move {
                let _ = executor
                    .execute_command(
                        command,
                        &shell,
                        /*current_directory_path=*/ None,
                        /*environment_variables=*/ None,
                        ExecuteCommandOptions::default(),
                    )
                    .await;
            })
        };
        let check_future = task_executor.spawn(async move {
            let event = executor_command_rx.recv().await.unwrap();
            let ExecutorCommandEvent::ExecuteCommand { command: sent, .. } = event else {
                panic!("expected an ExecuteCommand event");
            };
            assert_eq!(sent.shell_type, ShellType::Nu);
            assert_eq!(
                sent.command,
                format!(
                    " {POSIX_GENERATOR_WRAPPER} {} r##'{command}'##",
                    sent.command_id
                )
            );
            // It is still recognised as an in-band command, whatever the leading space.
            assert!(is_in_band_command(&sent.command));
        });
        task_executor
            .run(async move {
                check_future.await;
            })
            .await;
        drop(execute_future);
    });
}

/// Builds an executor with an inert pty side, for tests that drive the queue by hand.
fn test_executor() -> Arc<InBandCommandExecutor> {
    let (executor_command_tx, _) = async_channel::unbounded();
    let (in_band_command_cancelled_tx, _) = async_channel::unbounded();
    Arc::new(InBandCommandExecutor::new(
        executor_command_tx,
        in_band_command_cancelled_tx,
    ))
}

#[test]
fn test_command_ids_are_unique_within_one_microsecond() {
    // Fails on the timestamp-only id: a burst of ids inside one clock tick collided, so one
    // job's cancel (or a cancelled command's late output) could hit another job's command.
    let executor = test_executor();
    let ids: Vec<String> = (0..10_000).map(|_| executor.next_command_id()).collect();
    let unique: std::collections::HashSet<&String> = ids.iter().collect();
    assert_eq!(unique.len(), ids.len());
    assert!(ids
        .windows(2)
        .all(|w| w[0].parse::<i64>().unwrap() < w[1].parse::<i64>().unwrap()));
}

#[test]
fn test_aborting_one_job_leaves_another_jobs_command_pending_and_resolvable() {
    // A superseded decoration parse aborts only its own `execute_command` future; a Tab
    // completion's command queued behind it must survive and still resolve (#1352).
    App::test((), |_app| async move {
        let executor = test_executor();
        let task_executor = async_executor::LocalExecutor::new();

        // The decoration parse's command, aborted later.
        let (abort_handle, abort_registration) = AbortHandle::new_pair();
        let parse_executor = executor.clone();
        let parse_future = task_executor.spawn(Abortable::new(
            execute_test_command(parse_executor, "parse", |_| {}),
            abort_registration,
        ));
        // The Tab completion's command.
        let completion_future = task_executor.spawn(execute_test_command(
            executor.clone(),
            "complete",
            assert_command_output_result_fn("done", true),
        ));

        let driver = task_executor.spawn(async move {
            let parse_id = executor
                .running_command_id()
                .expect("parse command running");
            let pending = executor.pending_command_ids();
            assert_eq!(pending.len(), 1);
            let completion_id = pending[0].clone();

            abort_handle.abort();
            // Let the aborted future drop (running its on_cancel).
            async_io_yield().await;

            // Only the parse's command is gone; the completion's command now runs.
            assert_eq!(executor.running_command_id(), Some(completion_id.clone()));
            assert!(executor.pending_command_ids().is_empty());

            // The aborted command's late output is dropped, not given to the completion.
            executor.handle_executed_command_event(ExecutedExecutorCommandEvent {
                command_id: parse_id,
                exit_code: 0,
                output: "stale".as_bytes().to_vec(),
            });
            assert_eq!(executor.running_command_id(), Some(completion_id.clone()));

            executor.handle_executed_command_event(ExecutedExecutorCommandEvent {
                command_id: completion_id,
                exit_code: 0,
                output: "done".as_bytes().to_vec(),
            });
        });

        task_executor
            .run(async move {
                assert!(parse_future.await.is_err(), "the parse job was aborted");
                completion_future.await;
                driver.await;
            })
            .await;
    });
}

/// Yields to the executor once so a just-aborted sibling task gets polled and dropped.
async fn async_io_yield() {
    let mut yielded = false;
    futures_util::future::poll_fn(|cx| {
        if yielded {
            std::task::Poll::Ready(())
        } else {
            yielded = true;
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        }
    })
    .await
}

#[test]
fn test_late_output_of_a_cancelled_running_command_is_dropped() {
    App::test((), |_app| async move {
        let executor = test_executor();
        let task_executor = async_executor::LocalExecutor::new();

        let first =
            task_executor.spawn(execute_test_command(executor.clone(), "first", |result| {
                let output = result.expect("ok");
                assert_eq!(output.status, CommandExitStatus::Failure);
            }));
        let second = task_executor.spawn(execute_test_command(
            executor.clone(),
            "second",
            assert_command_output_result_fn("second-out", true),
        ));

        let driver = task_executor.spawn(async move {
            let first_id = executor.running_command_id().unwrap();
            executor.handle_cancelled_in_band_command_event(InBandCommandCancelledEvent {
                command_id: first_id.clone(),
            });
            let second_id = executor.running_command_id().expect("second now runs");
            assert_ne!(first_id, second_id);

            // The cancelled command's output arrives late: it must not resolve `second`.
            executor.handle_executed_command_event(ExecutedExecutorCommandEvent {
                command_id: first_id,
                exit_code: 0,
                output: "first-out".as_bytes().to_vec(),
            });
            assert_eq!(executor.running_command_id(), Some(second_id.clone()));

            executor.handle_executed_command_event(ExecutedExecutorCommandEvent {
                command_id: second_id,
                exit_code: 0,
                output: "second-out".as_bytes().to_vec(),
            });
        });

        task_executor
            .run(async move {
                first.await;
                second.await;
                driver.await;
            })
            .await;
    });
}

#[test]
fn test_abort_callbacks_do_not_cancel_the_whole_session() {
    // Source guard: the abort callbacks of the decoration/detection parse, autosuggestion and
    // Tab-completion jobs once called the session-wide `cancel_active_commands()`, killing every
    // other job's in-flight in-band command (#1352). Superseding a job drops its future, which
    // cancels exactly its own commands. The only genuine session-wide cancel in `input` is
    // none; in `view.rs` it is "the user ran a command" (checked below to still exist).
    fn code_calls(src: &str) -> usize {
        src.lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .filter(|l| l.contains(".cancel_active_commands()"))
            .count()
    }
    assert_eq!(code_calls(include_str!("../../../input/decorations.rs")), 0);
    assert_eq!(code_calls(include_str!("../../../input.rs")), 0);
    assert_eq!(code_calls(include_str!("../../../view.rs")), 1);
}
