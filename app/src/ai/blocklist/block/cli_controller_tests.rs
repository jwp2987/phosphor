use super::{
    AIAgentActionId, AIConversationId, LongRunningCommandControlState, TaskId, TerminalModel,
    UserTakeOverReason, needs_byop_monitor_upgrade,
};

// Blocks persisted before `should_auto_resume` stored `Stop` as a bare unit variant. These must
// still deserialize (with resume disabled) so restoring an older session doesn't drop the block's
// AI metadata wholesale.
#[test]
fn legacy_stop_reason_deserializes_with_resume_disabled() {
    let reason: UserTakeOverReason = serde_json::from_str("\"Stop\"").unwrap();
    assert_eq!(
        reason,
        UserTakeOverReason::Stop {
            should_auto_resume: false
        }
    );

    let state: LongRunningCommandControlState =
        serde_json::from_str(r#"{"User":{"reason":"Stop"}}"#).unwrap();
    assert_eq!(
        state,
        LongRunningCommandControlState::User {
            reason: UserTakeOverReason::Stop {
                should_auto_resume: false
            }
        }
    );
}

#[test]
fn stop_reason_round_trips() {
    for should_auto_resume in [true, false] {
        let reason = UserTakeOverReason::Stop { should_auto_resume };
        let json = serde_json::to_string(&reason).unwrap();
        assert_eq!(
            serde_json::from_str::<UserTakeOverReason>(&json).unwrap(),
            reason
        );
    }
}

// `BlockedOnInput` is the password-prompt hand-over. The agent's own tool call is still in
// flight and is waiting on this very command's result, so unlike `Manual` and `Stop` it must
// not cancel the conversation, and the command must go back to the agent once it finishes.
#[test]
fn blocked_on_input_keeps_the_conversation_alive_and_resumes_it() {
    let reason = UserTakeOverReason::BlockedOnInput;
    assert!(!reason.should_cancel_conversation());
    assert!(reason.should_auto_resume());
    assert!(reason.is_blocked_on_input());
    assert!(!reason.is_stop());
    assert!(!reason.is_transfer_from_agent());
    assert_eq!(reason.transfer_reason(), None);

    assert!(
        LongRunningCommandControlState::User {
            reason: UserTakeOverReason::BlockedOnInput
        }
        .should_auto_resume()
    );

    // The user-initiated take-overs are the ones that cancel; the agent-initiated transfer
    // shares `BlockedOnInput`'s "leave the conversation alone" behaviour.
    assert!(UserTakeOverReason::Manual.should_cancel_conversation());
    assert!(
        UserTakeOverReason::Stop {
            should_auto_resume: true
        }
        .should_cancel_conversation()
    );
    assert!(
        !UserTakeOverReason::TransferFromAgent {
            reason: "enter password".to_owned()
        }
        .should_cancel_conversation()
    );
}

// The reason is persisted with the block's AI metadata, so a session restored while the prompt
// is still up must come back user-controlled instead of dropping the metadata wholesale, the
// same failure the legacy `Stop` case above guards against.
#[test]
fn blocked_on_input_reason_round_trips() {
    let reason = UserTakeOverReason::BlockedOnInput;
    let json = serde_json::to_string(&reason).unwrap();
    assert_eq!(json, "\"BlockedOnInput\"");
    assert_eq!(
        serde_json::from_str::<UserTakeOverReason>(&json).unwrap(),
        reason
    );

    let state: LongRunningCommandControlState =
        serde_json::from_str(r#"{"User":{"reason":"BlockedOnInput"}}"#).unwrap();
    assert_eq!(
        state,
        LongRunningCommandControlState::User {
            reason: UserTakeOverReason::BlockedOnInput
        }
    );
}

/// A running agent-requested command with no control state yet: the window before the BYOP
/// snapshot upgrade installs a subagent.
fn agent_requested_long_running_block(conversation_id: AIConversationId) -> TerminalModel {
    let mut model = TerminalModel::mock(None, None);
    model.simulate_long_running_block("sudo apt update", "[sudo] password for user:");
    model
        .block_list_mut()
        .active_block_mut()
        .set_agent_interaction_mode_for_requested_command(
            AIAgentActionId::from("requested-command".to_owned()),
            None,
            conversation_id,
        );
    model
}

// The original BYOP upgrade window is unchanged: agent-requested, no control state yet.
#[test]
fn byop_monitor_upgrade_runs_before_any_control_state_exists() {
    let model = agent_requested_long_running_block(AIConversationId::new());
    let block = model.block_list().active_block();
    assert!(block.long_running_control_state().is_none());
    assert!(needs_byop_monitor_upgrade(block));
}

// A password-prompt hand-over lands about a second into the command, before the first
// snapshot. The block must NOT get a subagent: its auto-resume on completion depends on having
// none (see `needs_byop_monitor_upgrade`'s doc -- with a subagent, the inline-view completion
// path and `FinishedSubagent`'s queued-prompt delivery both cancel the resume).
#[test]
fn byop_monitor_upgrade_skips_a_block_handed_to_the_user_at_a_password_prompt() {
    let mut model = agent_requested_long_running_block(AIConversationId::new());
    let block = model.block_list_mut().active_block_mut();
    block
        .take_over_control_for_user(UserTakeOverReason::BlockedOnInput)
        .expect("the password-prompt hand-over is admitted before a control state exists");
    assert!(!needs_byop_monitor_upgrade(block));
}

// Handing the password prompt back gives an `Agent` state with no subagent. Upgrading it would
// route the agent's follow-ups into the hidden silent subtask, so it is skipped too.
#[test]
fn byop_monitor_upgrade_skips_a_handed_back_block() {
    let mut model = agent_requested_long_running_block(AIConversationId::new());
    let block = model.block_list_mut().active_block_mut();
    block
        .take_over_control_for_user(UserTakeOverReason::BlockedOnInput)
        .unwrap();
    block.handoff_control_to_agent().unwrap();
    assert!(block.is_agent_in_control());
    assert!(!needs_byop_monitor_upgrade(block));
}

// A snapshot handled after its block already finished must not upgrade it: nothing would ever
// finish the resulting silent subtask, and `has_active_subagent()` would stick true.
#[test]
fn byop_monitor_upgrade_skips_a_finished_block() {
    let mut model = agent_requested_long_running_block(AIConversationId::new());
    let block = model.block_list_mut().active_block_mut();
    assert!(needs_byop_monitor_upgrade(block));
    block.finish(0);
    assert!(block.finished());
    assert!(!needs_byop_monitor_upgrade(block));
}

// A block that already has an agent monitor is never upgraded again.
#[test]
fn byop_monitor_upgrade_skips_an_already_monitored_block() {
    let conversation_id = AIConversationId::new();
    let mut model = agent_requested_long_running_block(conversation_id);
    let block = model.block_list_mut().active_block_mut();
    block
        .set_agent_interaction_mode_for_agent_monitored_command(
            &TaskId::new("existing-task".to_owned()),
            conversation_id,
        )
        .unwrap();
    assert!(!needs_byop_monitor_upgrade(block));
}

// A stop/rewind (`set_user_control_with_stop_reason`) or a manual take-over leaves a control
// state, so the upgrade keeps skipping the block and no new subagent re-seizes a command the
// user stopped. Note the first sub-case also depends on `set_user_control_with_stop_reason`
// installing a `Stop` state when there was none (interaction_mode.rs); this test does not
// cancel a conversation, it only sets the block's control state.
#[test]
fn byop_monitor_upgrade_skips_a_stopped_or_taken_over_block() {
    // Stop before any control state existed.
    let mut model = agent_requested_long_running_block(AIConversationId::new());
    let block = model.block_list_mut().active_block_mut();
    block.set_user_control_with_stop_reason();
    assert!(matches!(
        block.long_running_control_state(),
        Some(LongRunningCommandControlState::User {
            reason: UserTakeOverReason::Stop { .. }
        })
    ));
    assert!(!needs_byop_monitor_upgrade(block));

    // Stop after a password-prompt hand-over was handed back.
    let mut model = agent_requested_long_running_block(AIConversationId::new());
    let block = model.block_list_mut().active_block_mut();
    block
        .take_over_control_for_user(UserTakeOverReason::BlockedOnInput)
        .unwrap();
    block.handoff_control_to_agent().unwrap();
    block.set_user_control_with_stop_reason();
    assert!(!needs_byop_monitor_upgrade(block));

    // Manual take-over after the hand-back.
    let mut model = agent_requested_long_running_block(AIConversationId::new());
    let block = model.block_list_mut().active_block_mut();
    block
        .take_over_control_for_user(UserTakeOverReason::BlockedOnInput)
        .unwrap();
    block.handoff_control_to_agent().unwrap();
    block
        .take_over_control_for_user(UserTakeOverReason::Manual)
        .unwrap();
    assert!(!needs_byop_monitor_upgrade(block));
}

// A user-typed command is never upgraded by the agent-requested fallback.
#[test]
fn byop_monitor_upgrade_skips_a_user_command() {
    let mut model = TerminalModel::mock(None, None);
    model.simulate_long_running_block("sleep 100", "");
    assert!(!needs_byop_monitor_upgrade(
        model.block_list().active_block()
    ));
}

// The warpify path (`ssh`, `docker run`, ...) is the scenario this predicate must keep saying
// yes to right up until `TerminalModel::blocks::reinit_shell` finishes the block: that race is
// why the snapshot-triggered fallback alone never reaches these commands (see TODO.md and
// `CLISubagentController::upgrade_warpify_block_if_agent_driven`'s doc comment).
// `needs_byop_monitor_upgrade` doesn't look at command text at all, so this pins the two states
// on either side of that race using a command warpify actually classifies as an interactive
// subshell, rather than re-deriving the same result the generic tests above already cover.
#[test]
fn byop_monitor_upgrade_is_eligible_for_a_warpify_command_until_it_finishes() {
    let mut model = TerminalModel::mock(None, None);
    model.simulate_long_running_block("ssh build-box", "");
    model
        .block_list_mut()
        .active_block_mut()
        .set_agent_interaction_mode_for_requested_command(
            AIAgentActionId::from("agent-ssh".to_owned()),
            None,
            AIConversationId::new(),
        );
    let block = model.block_list().active_block();
    assert!(
        needs_byop_monitor_upgrade(block),
        "an agent-requested ssh command must be upgrade-eligible before the warpify bootstrap \
         flow finishes its block"
    );

    // `reinit_shell` finishes the active block with exit code 0 the instant the remote/nested
    // shell's `init_shell` DCS confirms it started -- typically well under the 2s
    // `MAX_WAIT_DURATION` an agent-requested command's own `RequestCommandOutput` snapshot delay
    // waits before ever checking whether the block is still running. This is why the
    // snapshot-triggered fallback alone can't reach this block: it only ever sees `Completed`,
    // never `LongRunningCommandSnapshot`.
    let block = model.block_list_mut().active_block_mut();
    block.finish(0);
    assert!(
        !needs_byop_monitor_upgrade(block),
        "once the block is finished, the ordinary fallback correctly refuses it (see \
         byop_monitor_upgrade_skips_a_finished_block) -- which is exactly why the warpify \
         bootstrap trigger must install the control state BEFORE this point, not rely on it"
    );
}

// Warpify's own auto-bootstrap and banner are separately suppressed for agent commands
// (`TerminalView`'s `has_ai_metadata` checks), but the upgrade added for the warpify path must
// independently refuse a block with no agent metadata at all -- the user's own `ssh`/`docker run`
// must never gain a control state or a floating CLI subagent view.
#[test]
fn byop_monitor_upgrade_skips_a_users_own_warpify_command() {
    let mut model = TerminalModel::mock(None, None);
    model.simulate_long_running_block("ssh build-box", "");
    assert!(!needs_byop_monitor_upgrade(
        model.block_list().active_block()
    ));
}
