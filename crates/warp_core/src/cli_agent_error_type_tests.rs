use super::*;

#[test]
fn the_canonical_cancellation_spelling_is_the_one_the_publisher_emits() {
    // Pins the wire byte sequence. `crates/warp_tui/src/cli_agent_osc_event_publisher.rs`
    // emits this value for `ConversationStatus::Cancelled`, matching the pinned
    // oracle at `42effe840`. If this assertion ever has to change, the protocol
    // changed and every plugin matching on it has to be told.
    assert_eq!(CANCELLED, "cancelled");
}

#[test]
fn cancellation_is_recognised() {
    assert_eq!(
        CLIAgentErrorType::from_wire("cancelled"),
        CLIAgentErrorType::Cancelled
    );
    assert!(CLIAgentErrorType::from_wire("cancelled").is_cancellation());
}

#[test]
fn cancellation_tolerates_spelling_case_and_padding() {
    for value in [
        "canceled",
        "Cancelled",
        "CANCELLED",
        "Canceled",
        "  cancelled  ",
        "\tcancelled\n",
    ] {
        assert!(
            CLIAgentErrorType::from_wire(value).is_cancellation(),
            "{value:?} should classify as a cancellation"
        );
    }
}

#[test]
fn everything_else_stays_a_failure() {
    // The safe default. An unrecognised classification is "some failure", never
    // "benign" -- a consumer that guessed otherwise would hide real errors.
    for value in ["rate_limit", "error", "", "cancel", "cancellation", "oom"] {
        let classified = CLIAgentErrorType::from_wire(value);
        assert_eq!(
            classified,
            CLIAgentErrorType::Other(value),
            "{value:?} should not be special-cased"
        );
        assert!(!classified.is_cancellation());
    }
}

#[test]
fn wire_round_trip_normalises_cancellation_and_preserves_everything_else() {
    assert_eq!(
        CLIAgentErrorType::from_wire("  CANCELED ").as_wire_str(),
        CANCELLED,
        "a relay must not pass a variant spelling on as if it were canonical"
    );
    assert_eq!(
        CLIAgentErrorType::from_wire("rate_limit").as_wire_str(),
        "rate_limit"
    );
}
