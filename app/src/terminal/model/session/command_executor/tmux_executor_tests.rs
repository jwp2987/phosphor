use crate::terminal::{model::session::ExecuteCommandOptions, shell::ShellType};

use super::*;
use warpui::App;

async fn execute_test_command<F>(
    executor: Arc<TmuxCommandExecutor>,
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

#[test]
fn test_emits_successful_command_output() {
    App::test((), |_app| async move {
        // The receiver must stay alive: dispatch to the PTY controller now fails closed,
        // so a dropped receiver means the command is never registered at all.
        let (executor_command_tx, _executor_command_rx) = async_channel::unbounded();
        let executor = Arc::new(TmuxCommandExecutor::new(executor_command_tx));

        let task_executor = async_executor::LocalExecutor::new();

        let execute_command_future = task_executor.spawn(execute_test_command(
            executor.clone(),
            "echo foo",
            assert_command_output_result_fn("foo", true),
        ));
        let executor_for_output = executor.clone();
        let handle_command_output_future = task_executor.spawn(async move {
            let test_command_id = executor_for_output
                .in_flight_commands
                .lock()
                .keys()
                .next()
                .expect("Executor should be running test command.")
                .to_string();
            executor_for_output.handle_executed_command_event(ExecutedExecutorCommandEvent {
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

        assert!(
            executor.in_flight_commands.lock().is_empty(),
            "a command that reported its output must not stay registered"
        );
    });
}

/// A command whose awaiting future is dropped — which is what every keystroke does to
/// the previous autosuggestion/completion probe — must not leave a registration behind.
/// The entry holds the channel's only sender, so a stale one is not merely wasted memory:
/// it keeps a channel open that nothing will ever send on.
#[test]
fn test_forgets_command_when_awaiting_future_is_cancelled() {
    App::test((), |_app| async move {
        let (executor_command_tx, _executor_command_rx) = async_channel::unbounded();
        let executor = Arc::new(TmuxCommandExecutor::new(executor_command_tx));
        let shell = Shell::new(ShellType::Zsh, None, None, Default::default(), None);

        let mut future = Box::pin(executor.execute_command(
            "echo foo",
            &shell,
            /*current_directory_path=*/ None,
            /*environment_variables=*/ None,
            ExecuteCommandOptions::default(),
        ));

        // One poll dispatches the command and parks the future on its output channel.
        assert!(
            futures::poll!(future.as_mut()).is_pending(),
            "the command has not reported yet, so the future must still be pending"
        );
        assert_eq!(
            executor.in_flight_commands.lock().len(),
            1,
            "an in-flight command should be registered"
        );

        drop(future);

        assert!(
            executor.in_flight_commands.lock().is_empty(),
            "cancelling the awaiting future must drop the registration"
        );
    });
}

/// Dispatch to the PTY controller can fail (the channel is gone once the session is
/// tearing down). The command then never reaches tmux, so no output event will ever carry
/// its id — parking the caller on that channel would hang it for the rest of the session,
/// because the sender it is waiting on is the one this executor is holding.
#[test]
fn test_failed_dispatch_reports_an_error_and_registers_nothing() {
    App::test((), |_app| async move {
        // Dropping the receiver closes the channel, so `try_send` fails.
        let (executor_command_tx, executor_command_rx) = async_channel::unbounded();
        drop(executor_command_rx);
        let executor = Arc::new(TmuxCommandExecutor::new(executor_command_tx));
        let shell = Shell::new(ShellType::Zsh, None, None, Default::default(), None);

        let result = executor
            .execute_command(
                "echo foo",
                &shell,
                /*current_directory_path=*/ None,
                /*environment_variables=*/ None,
                ExecuteCommandOptions::default(),
            )
            .await;

        assert!(
            result.is_err(),
            "a command that never reached tmux must fail rather than wait forever"
        );
        assert!(
            executor.in_flight_commands.lock().is_empty(),
            "a command that was never dispatched must not stay registered"
        );
    });
}
