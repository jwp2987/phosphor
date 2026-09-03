use super::{LongRunningCommandControlState, UserTakeOverReason};

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
