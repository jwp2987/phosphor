use std::time::Duration;

use async_channel::unbounded;
use warpui::App;

use super::*;
use crate::ai::agent::AIAgentAction;
use crate::ai::agent::conversation::AIConversationId;
use crate::ai::agent::task::TaskId;
use crate::settings::AgentProvider;
use crate::terminal::event::BlockWorkingDirectoryUpdatedEvent;
use crate::terminal::model::block::BlockMetadata;
use crate::terminal::model::session::Sessions;
use crate::terminal::model::terminal_model::BlockIndex;
use crate::test_util::terminal::initialize_app_for_terminal_view;

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
        // #597: the PowerShell wrapper is `Warp-`, matching the function pwsh.ps1
        // actually defines. Re-specified from `Zap-`, which named nothing.
        "Warp-Run-GeneratorCommand 42 'ssh host' -ErrorAction Ignore",
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
        "Warp-Run-GeneratorCommand 42 'git status' -ErrorAction Ignore",
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
    // A model that cannot drive snapshots is waited for, as before #855.
    assert_eq!(
        action_result_delay_for_requested_command(true, false),
        ActionResultDelay::UntilCompletion
    );
    assert_eq!(
        action_result_delay_for_requested_command(false, false),
        ActionResultDelay::Default
    );
}

/// The snapshot-mode decision (#855): only a waited command changes with the model.
#[test]
fn snapshot_capable_models_get_a_bounded_stall_aware_wait() {
    assert_eq!(
        action_result_delay_for_requested_command(true, true),
        ActionResultDelay::OnCompletionOrStall {
            timeout: ShellCommandExecutor::MAX_AGENT_DELAY_DURATION
        }
    );
    assert_eq!(
        ShellCommandExecutor::MAX_AGENT_DELAY_DURATION,
        Duration::from_secs(120)
    );
    // An unwaited command is the same for every model.
    assert_eq!(
        action_result_delay_for_requested_command(false, true),
        ActionResultDelay::Default
    );
}

#[test]
fn a_stalled_snapshot_is_marked_preempted() {
    let delay = ActionResultDelay::OnCompletionOrStall {
        timeout: Duration::from_secs(120),
    };
    assert!(compute_is_preempted(WakeReason::Stalled, delay));
    assert!(compute_is_preempted(WakeReason::Timeout, delay));
    assert!(compute_is_preempted(WakeReason::ForceRefresh, delay));
    assert!(!compute_is_preempted(WakeReason::BlockFinished, delay));
}

#[test]
fn snapshot_mode_follows_the_model_setting_then_the_default() {
    let mut provider = AgentProvider::new_empty();
    provider.id = "p1".to_owned();
    provider.base_url = "https://api.example.com/v1".to_owned();
    let mut big = crate::settings::AgentProviderModel::from_id("claude-sonnet-4-5".to_owned());
    let small = crate::settings::AgentProviderModel::from_id("deepseek-v4-flash".to_owned());
    provider.models = vec![big.clone(), small];
    let providers = [provider.clone()];

    assert!(handles_command_snapshots_for(
        &providers,
        "byop:p1:claude-sonnet-4-5"
    ));
    assert!(!handles_command_snapshots_for(
        &providers,
        "byop:p1:deepseek-v4-flash"
    ));
    // Not BYOP, unknown provider, unknown model: the fork's previous behaviour.
    assert!(!handles_command_snapshots_for(&providers, "auto"));
    assert!(!handles_command_snapshots_for(
        &providers,
        "byop:other:claude-sonnet-4-5"
    ));
    assert!(!handles_command_snapshots_for(&providers, "byop:p1:nope"));

    // The explicit per-model setting wins in both directions.
    big.handles_command_snapshots = Some(false);
    provider.models[0] = big;
    provider.models[1].handles_command_snapshots = Some(true);
    let providers = [provider];
    assert!(!handles_command_snapshots_for(
        &providers,
        "byop:p1:claude-sonnet-4-5"
    ));
    assert!(handles_command_snapshots_for(
        &providers,
        "byop:p1:deepseek-v4-flash"
    ));
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
/// the closing `}` must not get concatenated onto it.
/// Concatenating to `PY)` means the shell never sees the terminator, gets stuck at
/// PS2, and the command never finishes.
#[test]
fn multiline_heredoc_keeps_delimiter_and_closer_on_their_own_lines() {
    let command = "python3 - <<'PY'\nprint('ok')\nPY";

    for shell in [ShellType::Bash, ShellType::Zsh] {
        let wrapped = wrap_command_without_pager(Some(shell), command);
        let lines: Vec<&str> = wrapped.lines().collect();

        // The terminator stays alone on its own line; the line after it opens with the closing
        // brace of the command group (the wrapper's restore suffix follows it on that line).
        let terminator = lines
            .iter()
            .position(|line| *line == "PY")
            .unwrap_or_else(|| panic!("heredoc terminator got polluted: {wrapped}"));
        assert!(
            lines[terminator + 1].starts_with("} "),
            "closer not on the line after the terminator: {wrapped}"
        );
        assert_eq!(
            lines[lines.len() - 1],
            "}",
            "closing brace isn't alone on its own line: {wrapped}"
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

    assert!(wrap_command_without_pager(Some(ShellType::Bash), command).ends_with("\n}"));
    assert!(
        wrap_command_without_pager(Some(ShellType::Fish), command)
            .contains(&format!("\n{command}\nset -l __phosphor_ap_st"))
    );
    assert!(
        wrap_command_without_pager(Some(ShellType::PowerShell), command)
            .contains("\n}\n$global:LASTEXITCODE")
    );
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

    assert!(wrap_command_without_pager(Some(ShellType::Bash), command).ends_with("\n}"));
    assert!(
        wrap_command_without_pager(Some(ShellType::Fish), command)
            .contains(&format!("\n{command}\nset -l __phosphor_ap_st"))
    );
    assert!(
        wrap_command_without_pager(Some(ShellType::PowerShell), command)
            .contains("\n}\n$global:LASTEXITCODE")
    );
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
                    eviction_stamp: Default::default(),
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
                    eviction_stamp: Default::default(),
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

/// #615, half one: the *predicate* `write_skips_pty_permission_check` must be false when the
/// block id did not resolve. The bug was `is_none_or`, which fused "not found" with "finished"
/// and returned true for both -- reachable under tmux, where Warp's block model does not track
/// the pane and the lookup misses, so the profile's "Interact with running commands" setting
/// was bypassed entirely.
///
/// **Scope, stated precisely, because the previous version of this comment overclaimed.** This
/// test pins the predicate at `shell_command.rs`'s `write_skips_pty_permission_check` and
/// nothing else: reverting that function's body to `block_finished.is_none_or(|f| f)` turns it
/// red, but reverting the *call site* in `should_autoexecute` back to an inline `is_none_or`
/// leaves it GREEN -- the helper would merely become unreferenced, and `lib.rs`'s crate-level
/// `#![allow(dead_code)]` silences that. The call-site wiring is pinned by
/// `unresolved_block_write_falls_through_to_the_pty_permission_check` below; neither test alone
/// covers the fix.
#[test]
fn unresolved_block_does_not_skip_the_pty_permission_check() {
    // Block id did not resolve: writes to a live pty, so it must fall through and ask.
    assert!(
        !write_skips_pty_permission_check(None),
        "a missing block must not bypass the write_to_pty permission check (#615)"
    );

    // Still running: also a live pty, also must ask.
    assert!(
        !write_skips_pty_permission_check(Some(false)),
        "a still-running block must not bypass the write_to_pty permission check"
    );

    // Present and finished: nothing new is executed, the buffered output is returned.
    assert!(
        write_skips_pty_permission_check(Some(true)),
        "a finished block returns buffered output and needs no permission check"
    );
}

/// #615, half two: the *call site*. `should_autoexecute` must actually route the
/// `WriteToLongRunningShellCommand` arm through `write_skips_pty_permission_check`, so that an
/// unresolved block id falls through to the profile's `write_to_pty` permission instead of
/// short-circuiting to `true`.
///
/// This is the test the fix was missing. Reverting `shell_command.rs`'s call site to the
/// original `block.is_none_or(|b| b.finished())` -- with or without the helper still present --
/// makes this assertion fail, because the synthesised `BlockId` is never registered in the mock
/// terminal model, so the lookup misses and `is_none_or` yields `true`.
///
/// The precondition assert is deliberate: if the fixture ever resolved `write_to_pty` to
/// `AlwaysAllow`, `should_autoexecute` would return `true` for a legitimate reason and the
/// assertion below would fail for the wrong reason. Fail loudly on the fixture instead.
#[test]
fn unresolved_block_write_falls_through_to_the_pty_permission_check() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);

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

        let pty_permission = app.read(|ctx| {
            BlocklistAIPermissions::as_ref(ctx)
                .get_write_to_pty_setting(ctx, Some(terminal_view_id))
        });
        assert_eq!(
            pty_permission,
            WriteToPtyPermission::AlwaysAsk,
            "fixture precondition: the default test profile must ask before writing to a pty, \
             otherwise the assertion below would pass for the wrong reason"
        );

        // A fresh id that was never registered with the terminal model: the
        // lookup misses, which is the tmux case #615 was about.
        let action = AIAgentAction {
            id: AIAgentActionId::from("write-to-lrc".to_owned()),
            task_id: TaskId::new("terminal-use-task".to_owned()),
            action: AIAgentActionType::WriteToLongRunningShellCommand {
                block_id: BlockId::new(),
                input: b"input".to_vec().into(),
                mode: AIAgentPtyWriteMode::Raw,
            },
            requires_result: true,
        };
        let conversation_id = AIConversationId::new();

        let autoexecuted = executor.update(&mut app, |executor, ctx| {
            executor.should_autoexecute(
                ExecuteActionInput {
                    action: &action,
                    conversation_id,
                },
                ctx,
            )
        });

        assert!(
            !autoexecuted,
            "an unresolved block id must fall through to the write_to_pty permission (#615), \
             not short-circuit to auto-execution"
        );
    });
}

/// Issue #850, the "result reaches the model" half: an approved, `wait_until_completion` agent
/// command that the user interrupts with ctrl-c must still resolve its tool call -- with the
/// interrupted exit status (130, what a shell reports for SIGINT) and the output captured up to
/// the interrupt -- rather than leaving the model waiting or reporting a cancellation. The
/// interrupt itself is just a ^C on the PTY (see `TerminalView::ctrl_c`), so what the executor
/// sees is the command's block finishing with 130; this drives exactly that.
#[test]
fn interrupted_requested_command_reports_exit_130_and_captured_output() {
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

        let action_id = AIAgentActionId::from("sleep-forever".to_owned());
        {
            let mut model = terminal_model.lock();
            model.simulate_long_running_block("sleep 100000", "tick 1\r\n");
            model
                .block_list_mut()
                .active_block_mut()
                .set_agent_interaction_mode_for_requested_command(
                    action_id.clone(),
                    None,
                    AIConversationId::new(),
                );
        }

        let result_future = executor.update(&mut app, |executor, _ctx| {
            executor.action_result_future(
                BlockSelector::RequestedCommandId(action_id.clone()),
                ActionResultDelay::UntilCompletion,
            )
        });

        // The ^C reached the foreground process group; the shell reports 130 and precmd fires.
        terminal_model.lock().finish_block_with_exit_code(130);
        model_event_dispatcher.update(&mut app, |_dispatcher, ctx| {
            ctx.emit(ModelEvent::BlockMetadataReceived(
                BlockMetadataReceivedEvent {
                    block_metadata: BlockMetadata::new(None, Some("/tmp".to_string())),
                    block_index: BlockIndex::zero(),
                    eviction_stamp: Default::default(),
                    is_after_in_band_command: false,
                    is_done_bootstrapping: true,
                },
            ));
        });

        let result = result_future.await;
        match action_result_for_requested_command("sleep 100000".to_owned(), result) {
            AIAgentActionResultType::RequestCommandOutput(
                RequestCommandOutputResult::Completed {
                    command,
                    output,
                    exit_code,
                    ..
                },
            ) => {
                assert_eq!(command, "sleep 100000");
                assert_eq!(exit_code, ExitCode::from(130));
                assert!(
                    output.contains("tick 1"),
                    "the output captured before the interrupt must reach the model: {output:?}"
                );
            }
            other => panic!(
                "an interrupted command must report a completed result with its exit status, \
                 got {other:?}"
            ),
        }
    });
}

/// Builds a `ShellCommandExecutor` over a mock terminal model whose active block is the agent's
/// `sleep 100000`, written to the shell but not yet preexec'd, and records every
/// `CancelExecution` the executor emits.
fn executor_with_unpreexeced_agent_command(
    app: &mut App,
    action_id: &AIAgentActionId,
) -> (
    ModelHandle<ShellCommandExecutor>,
    Arc<FairMutex<TerminalModel>>,
    std::rc::Rc<std::cell::RefCell<Vec<AIAgentActionId>>>,
) {
    let sessions = app.add_model(|_| Sessions::new_for_test());
    let (_model_events_tx, model_events_rx) = unbounded();
    let model_event_dispatcher =
        app.add_model(|ctx| ModelEventDispatcher::new(model_events_rx, sessions.clone(), ctx));
    let active_session = app
        .add_model(|ctx| ActiveSession::new(sessions.clone(), model_event_dispatcher.clone(), ctx));
    let terminal_model = Arc::new(FairMutex::new(TerminalModel::mock(None, None)));
    let executor = app.add_model(|ctx| {
        ShellCommandExecutor::new(
            active_session,
            terminal_model.clone(),
            &model_event_dispatcher,
            EntityId::new(),
            ctx,
        )
    });
    {
        let mut model = terminal_model.lock();
        let active_block = model.block_list_mut().active_block_mut();
        active_block.start();
        active_block.set_agent_interaction_mode_for_requested_command(
            action_id.clone(),
            None,
            AIConversationId::new(),
        );
        assert!(
            active_block.is_command_grid_active(),
            "precondition: written but not yet preexec'd"
        );
        assert!(active_block.is_agent_requested_command_in_flight());
    }
    let interrupts = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let recorded = interrupts.clone();
    app.update(|ctx| {
        ctx.subscribe_to_model(&executor, move |_, event, _| {
            if let ShellCommandExecutorEvent::CancelExecution { action_id } = event {
                recorded.borrow_mut().push(action_id.clone());
            }
        });
    });
    (executor, terminal_model, interrupts)
}

/// Issue #850, pre-preexec window: stopping the conversation (`ManuallyCancelled`) while the
/// agent's command is written but not yet preexec'd used to drop the tool call and leave the
/// command to run orphaned. The executor must now ask the terminal to interrupt it.
#[test]
fn manual_cancel_of_an_in_flight_agent_command_asks_for_an_interrupt() {
    App::test((), |mut app| async move {
        let action_id = AIAgentActionId::from("sleep-forever".to_owned());
        let (executor, _terminal_model, interrupts) =
            executor_with_unpreexeced_agent_command(&mut app, &action_id);

        executor.update(&mut app, |executor, ctx| {
            executor.cancel_execution(&action_id, Some(CancellationReason::ManuallyCancelled), ctx);
        });

        assert_eq!(*interrupts.borrow(), vec![action_id]);
    });
}

/// Other cancellation reasons (here a follow-up) keep the pin's behaviour: the tool call is
/// dropped but the process is left alone.
#[test]
fn non_manual_cancel_of_an_agent_command_does_not_interrupt_it() {
    App::test((), |mut app| async move {
        let action_id = AIAgentActionId::from("sleep-forever".to_owned());
        let (executor, _terminal_model, interrupts) =
            executor_with_unpreexeced_agent_command(&mut app, &action_id);

        executor.update(&mut app, |executor, ctx| {
            executor.cancel_execution(
                &action_id,
                Some(CancellationReason::FollowUpSubmitted {
                    is_for_same_conversation: true,
                }),
                ctx,
            );
        });

        assert!(interrupts.borrow().is_empty());
    });
}

/// A command the user already holds (e.g. `stop_local_agent_conversation` marked it `Stop`
/// before cancelling, and sends its own ^C) must not get a second interrupt.
#[test]
fn manual_cancel_of_a_user_held_agent_command_does_not_interrupt_again() {
    App::test((), |mut app| async move {
        let action_id = AIAgentActionId::from("sleep-forever".to_owned());
        let (executor, terminal_model, interrupts) =
            executor_with_unpreexeced_agent_command(&mut app, &action_id);
        terminal_model
            .lock()
            .block_list_mut()
            .active_block_mut()
            .set_user_control_with_stop_reason();

        executor.update(&mut app, |executor, ctx| {
            executor.cancel_execution(&action_id, Some(CancellationReason::ManuallyCancelled), ctx);
        });

        assert!(interrupts.borrow().is_empty());
    });
}

/// #855: when Phosphor closed a pager, the model is told the output may be incomplete, and
/// only a finished command's output carries the note.
#[test]
fn pager_closed_note_is_appended_to_finished_output_only() {
    crate::i18n::init(Some("en"));
    let finished = ActionResult::CommandFinished {
        block_id: BlockId::new(),
        output: "first screen".to_owned(),
        exit_code: 0.into(),
        start_ts: None,
        completed_ts: None,
    };
    let ActionResult::CommandFinished { output, .. } = with_pager_closed_note(finished) else {
        panic!("a finished command stays finished");
    };
    assert!(output.starts_with("first screen\n"), "{output}");
    assert!(output.contains("Phosphor closed it"), "{output}");
    assert!(output.contains("--no-pager"), "{output}");

    assert!(matches!(
        with_pager_closed_note(ActionResult::Cancelled),
        ActionResult::Cancelled
    ));
}
