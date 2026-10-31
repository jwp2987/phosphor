use super::*;

fn completed(exit_code: i32, interrupted: bool) -> AIAgentActionResultType {
    AIAgentActionResultType::RequestCommandOutput(RequestCommandOutputResult::Completed {
        block_id: BlockId::new(),
        command: "cmd".to_owned(),
        output: "out".to_owned(),
        exit_code: ExitCode::from(exit_code),
        start_ts: None,
        completed_ts: None,
        interrupted,
    })
}

/// #1178: a command that exits 130 by itself is an ordinary result: the agent continues.
/// Fails on the old code, where any 130 was cancelled and the turn ended silently.
#[test]
fn self_exited_130_is_not_cancelled_and_triggers_a_follow_up() {
    let result = completed(130, false);
    assert!(!result.is_cancelled());
    assert!(!result.is_cancelled_during_requested_command_execution());
    assert!(result.should_trigger_request_upon_completion());
}

/// #1178: a ^C from the user or Phosphor (stop button, Escape, a ^C the agent wrote) still
/// cancels, so no follow-up is sent.
#[test]
fn interrupted_130_is_cancelled_and_sends_no_follow_up() {
    let result = completed(130, true);
    assert!(result.is_cancelled());
    assert!(result.is_cancelled_during_requested_command_execution());
    assert!(!result.should_trigger_request_upon_completion());
}

#[test]
fn other_exit_codes_are_never_cancelled() {
    assert!(!completed(0, false).is_cancelled());
    assert!(!completed(1, true).is_cancelled());
    assert!(completed(1, true).should_trigger_request_upon_completion());
}
