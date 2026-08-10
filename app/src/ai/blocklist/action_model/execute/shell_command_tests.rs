use std::time::Duration;

use async_channel::unbounded;
use warpui::App;

use super::*;
use crate::terminal::event::BlockWorkingDirectoryUpdatedEvent;
use crate::terminal::model::block::BlockMetadata;
use crate::terminal::model::session::Sessions;
use crate::terminal::model::terminal_model::BlockIndex;

#[test]
fn detects_interactive_session_commands_across_platforms() {
    for command in [
        "ssh root@example.com",
        "command ssh localhost",
        "ssh.exe -p 2222 root@example.com",
        "/usr/bin/ssh host",
        r#""C:\Windows\System32\OpenSSH\ssh.exe" -p 22 host"#,
        r#"& "C:\Program Files\OpenSSH\ssh.exe" host"#,
        "warp_run_generator_command 42 'ssh host'",
        " warp_run_generator_command 42 'ssh host'",
        "Zap-Run-GeneratorCommand 42 'ssh host' -ErrorAction Ignore",
        r#"warp_run_generator_command 42 '"C:\Windows\System32\OpenSSH\ssh.exe" host'"#,
        "gcloud compute ssh --zone us-west1-a my-instance",
        "eb ssh --profile my-profile my-env",
        "doctl compute ssh --region nyc1 my-droplet",
        "mosh root@example.com",
        "sftp root@example.com",
        "telnet example.com",
    ] {
        assert_eq!(
            command_starts_non_terminating_session(command),
            true,
            "{command}"
        );
    }
}

#[test]
fn does_not_detect_unrelated_or_non_interactive_ssh_commands() {
    for command in [
        "",
        "echo ssh",
        "git status",
        "ssh-add-key",
        "ssh -T user@host",
        "ssh -v user@host -W localhost:22",
        "ssh user@host ls",
        "ssh.exe user@host ls",
        r#""C:\Windows\System32\OpenSSH\ssh.exe" user@host ls"#,
        r#"& "C:\Program Files\OpenSSH\ssh.exe" user@host ls"#,
        "warp_run_generator_command 42 'ssh user@host ls'",
        "Zap-Run-GeneratorCommand 42 'git status' -ErrorAction Ignore",
        "rsync myfile.txt ssh://user@server.com",
        // Characters are still stuck to the closing quote; tokenize is deliberately
        // rejected to avoid mis-cutting this into `ssh` and then misjudging
        // `ssh hello-world` as an interactive session.
        r#""ssh"hello-world"#,
        // An unclosed quote is likewise rejected for tokenizing.
        r#""ssh hello world"#,
    ] {
        assert_eq!(
            command_starts_non_terminating_session(command),
            false,
            "{command}"
        );
    }
}

#[test]
fn shortens_on_completion_delay_for_interactive_sessions() {
    assert_eq!(
        effective_read_shell_command_delay("ssh host", Some(ShellCommandDelay::OnCompletion)),
        ActionResultDelay::OnCompletion {
            timeout: ShellCommandExecutor::MAX_WAIT_DURATION
        }
    );
    assert_eq!(
        effective_read_shell_command_delay(
            r#"& "C:\Program Files\OpenSSH\ssh.exe" host"#,
            Some(ShellCommandDelay::OnCompletion)
        ),
        ActionResultDelay::OnCompletion {
            timeout: ShellCommandExecutor::MAX_WAIT_DURATION
        }
    );
    assert_eq!(
        effective_read_shell_command_delay(
            "warp_run_generator_command 42 'ssh host'",
            Some(ShellCommandDelay::OnCompletion)
        ),
        ActionResultDelay::OnCompletion {
            timeout: ShellCommandExecutor::MAX_WAIT_DURATION
        }
    );
    assert_eq!(
        effective_read_shell_command_delay("mosh host", None),
        ActionResultDelay::OnCompletion {
            timeout: ShellCommandExecutor::MAX_WAIT_DURATION
        }
    );
}

#[test]
fn preserves_explicit_or_non_interactive_read_delays() {
    assert_eq!(
        effective_read_shell_command_delay(
            "ssh host",
            Some(ShellCommandDelay::Duration(Duration::from_secs(8)))
        ),
        ActionResultDelay::Duration(Duration::from_secs(8))
    );
    assert_eq!(
        effective_read_shell_command_delay("git status", Some(ShellCommandDelay::OnCompletion)),
        ActionResultDelay::OnCompletion {
            timeout: ShellCommandExecutor::MAX_AGENT_DELAY_DURATION
        }
    );
    assert_eq!(
        effective_read_shell_command_delay("git status", None),
        ActionResultDelay::Default
    );
}

#[test]
fn requested_command_wait_until_completion_does_not_use_snapshot_timeout() {
    assert_eq!(
        action_result_delay_for_requested_command(true),
        ActionResultDelay::UntilCompletion
    );
    assert_eq!(
        action_result_delay_for_requested_command(false),
        ActionResultDelay::Default
    );
}

#[test]
fn preemption_logic_covers_until_completion_timeout() {
    use ActionResultDelay::{Default, Duration as DurationDelay, OnCompletion, UntilCompletion};
    use WakeReason::*;

    // BlockFinished is never a preemption — it's the signal that the command truly finished.
    assert!(!compute_is_preempted(BlockFinished, UntilCompletion));
    assert!(!compute_is_preempted(BlockFinished, Default));
    assert!(!compute_is_preempted(
        BlockFinished,
        OnCompletion {
            timeout: Duration::from_secs(1)
        }
    ));

    // ForceRefresh is always a preemption, regardless of delay.
    assert!(compute_is_preempted(ForceRefresh, UntilCompletion));
    assert!(compute_is_preempted(ForceRefresh, Default));

    // Timeout + OnCompletion / UntilCompletion is a preemption.
    assert!(compute_is_preempted(
        Timeout,
        OnCompletion {
            timeout: Duration::from_secs(1)
        }
    ));
    // #138: the pager-hang fallback timeout must be marked as a preemption, so the
    // server doesn't misread it as "command completed".
    assert!(compute_is_preempted(Timeout, UntilCompletion));

    // Timeout + Default / Duration is not a preemption — the agent already expects to get an intermediate snapshot.
    assert!(!compute_is_preempted(Timeout, Default));
    assert!(!compute_is_preempted(
        Timeout,
        DurationDelay(Duration::from_secs(1))
    ));
}

/// Reproduces the hang case: a heredoc terminator must stay alone on its own line;
/// the closing `)` must not get concatenated onto it.
/// Concatenating to `PY)` means the shell never sees the terminator, gets stuck at
/// PS2, and the command never finishes.
#[test]
fn multiline_heredoc_keeps_delimiter_and_closer_on_their_own_lines() {
    let command = "python3 - <<'PY'\nprint('ok')\nPY";

    for shell in [ShellType::Bash, ShellType::Zsh] {
        let wrapped = wrap_command_without_pager(Some(shell), command);
        let lines: Vec<&str> = wrapped.lines().collect();

        assert_eq!(
            lines[lines.len() - 2],
            "PY",
            "heredoc terminator got polluted: {wrapped}"
        );
        assert_eq!(
            lines[lines.len() - 1],
            ")",
            "closing paren isn't alone on its own line: {wrapped}"
        );
        assert!(
            wrapped.contains("PAGER=cat"),
            "pager suppression lost: {wrapped}"
        );
    }
}

/// Same class of bug: when a command ends in a trailing `#` comment, the closing
/// token would get commented out.
#[test]
fn multiline_trailing_comment_does_not_swallow_closer() {
    let command = "echo start\necho done # trailing comment";

    assert!(wrap_command_without_pager(Some(ShellType::Bash), command).ends_with("\n)"));
    assert!(wrap_command_without_pager(Some(ShellType::Fish), command).ends_with("\nend"));
    assert!(wrap_command_without_pager(Some(ShellType::PowerShell), command).ends_with("\n}"));
}

/// A genuinely single-line command ending in a `#` comment is the same failure class
/// as the multi-line heredoc case: concatenating the closer onto the same line lets
/// the comment swallow it (`ls -la # comment)` never closes the subshell). Regression
/// test for that gap in the original fix -- this command contains no `\n` at all, so
/// it must still switch to the wrapped (closer-on-its-own-line) form.
#[test]
fn single_line_trailing_comment_does_not_swallow_closer() {
    let command = "ls -la  # clean listing";
    assert!(!command.contains('\n'));

    assert!(wrap_command_without_pager(Some(ShellType::Bash), command).ends_with("\n)"));
    assert!(wrap_command_without_pager(Some(ShellType::Fish), command).ends_with("\nend"));
    assert!(wrap_command_without_pager(Some(ShellType::PowerShell), command).ends_with("\n}"));
}

/// Single-line commands must keep their original single-line shape:
/// `bytes_to_execute_command` replaces `\n` with `\r` on shells that don't support
/// bracketed paste, which would split one command into multiple blocks if we added
/// extra newlines.
#[test]
fn single_line_command_stays_on_one_line() {
    let command = "cargo check";

    for shell in [
        ShellType::Bash,
        ShellType::Zsh,
        ShellType::Fish,
        ShellType::PowerShell,
    ] {
        let wrapped = wrap_command_without_pager(Some(shell), command);
        assert!(
            !wrapped.contains('\n'),
            "{shell:?} single-line command got split: {wrapped}"
        );
        assert!(
            wrapped.contains(command),
            "{shell:?} command got lost: {wrapped}"
        );
    }
}

/// An unknown shell can't be decorated safely, so it's passed through as-is.
#[test]
fn unknown_shell_passes_command_through() {
    let command = "python3 - <<'PY'\nprint('ok')\nPY";
    assert_eq!(wrap_command_without_pager(None, command), command);
}

/// Regression test: an earlier version of this function trimmed trailing whitespace
/// unconditionally before branching on shell type, which mutated the "pass through as-is"
/// guarantee for an unrecognized shell -- the None path must return the exact original
/// bytes, trailing whitespace included.
#[test]
fn unknown_shell_passthrough_preserves_trailing_whitespace() {
    let command = "echo hi   \n\n";
    assert_eq!(wrap_command_without_pager(None, command), command);
}

/// Locks in the contract that `ShellCommandExecutor`'s requested-command finish
/// detector reacts only to `BlockMetadataReceived` (precmd) and not to
/// `BlockWorkingDirectoryUpdated` (OSC 7). The detector relies on
/// `BlockMetadataReceived` firing exactly once per block; OSC 7 can fire many
/// times per block, so wiring it into the detector would resolve the wait
/// future before the requested command actually finishes.
#[test]
fn block_working_directory_updated_does_not_drain_finish_senders() {
    App::test((), |mut app| async move {
        let terminal_view_id = EntityId::new();
        let sessions = app.add_model(|_| Sessions::new_for_test());
        let (_model_events_tx, model_events_rx) = unbounded();
        let model_event_dispatcher =
            app.add_model(|ctx| ModelEventDispatcher::new(model_events_rx, sessions.clone(), ctx));
        let active_session = app.add_model(|ctx| {
            ActiveSession::new(sessions.clone(), model_event_dispatcher.clone(), ctx)
        });
        let terminal_model = Arc::new(FairMutex::new(TerminalModel::mock(None, None)));
        let executor = app.add_model(|ctx| {
            ShellCommandExecutor::new(
                active_session,
                terminal_model.clone(),
                &model_event_dispatcher,
                terminal_view_id,
                ctx,
            )
        });

        let block_id = BlockId::new();
        let selector = BlockSelector::Id(block_id);
        let (tx, _rx) = oneshot::channel::<()>();
        executor.update(&mut app, |executor, _ctx| {
            executor.block_finished_senders.insert(selector, tx);
        });
        assert_eq!(
            app.read(|ctx| executor.as_ref(ctx).block_finished_senders.len()),
            1
        );

        // OSC 7 update — must NOT drain or resolve the finish sender.
        model_event_dispatcher.update(&mut app, |_dispatcher, ctx| {
            ctx.emit(ModelEvent::BlockWorkingDirectoryUpdated(
                BlockWorkingDirectoryUpdatedEvent {
                    block_metadata: BlockMetadata::new(None, Some("/tmp/new".to_string())),
                    block_index: BlockIndex::zero(),
                    is_for_in_band_command: false,
                    is_done_bootstrapping: true,
                },
            ));
        });
        assert_eq!(
            app.read(|ctx| executor.as_ref(ctx).block_finished_senders.len()),
            1,
            "BlockWorkingDirectoryUpdated must not touch block_finished_senders — \
             that map is reserved for precmd (BlockMetadataReceived)"
        );

        // Precmd event — the senders map should be drained (and since the
        // block isn't in the terminal model, the sender is dropped).
        model_event_dispatcher.update(&mut app, |_dispatcher, ctx| {
            ctx.emit(ModelEvent::BlockMetadataReceived(
                BlockMetadataReceivedEvent {
                    block_metadata: BlockMetadata::new(None, Some("/tmp/precmd".to_string())),
                    block_index: BlockIndex::zero(),
                    is_after_in_band_command: false,
                    is_done_bootstrapping: true,
                },
            ));
        });
        assert_eq!(
            app.read(|ctx| executor.as_ref(ctx).block_finished_senders.len()),
            0,
            "BlockMetadataReceived should drain the finish senders"
        );
    });
}
