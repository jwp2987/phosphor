use super::{CompactionMarker, SummaryStreamStatus, compaction_marker};
use crate::ai::byop_compaction::state::{CompactionState, CompletedCompaction};

fn state_with_summary(assistant_msg_id: &str, auto: bool) -> CompactionState {
    let mut state = CompactionState::default();
    state.push_completed(CompletedCompaction {
        user_msg_id: "user-1".to_owned(),
        assistant_msg_id: assistant_msg_id.to_owned(),
        head_message_ids: vec!["old-1".to_owned()],
        tail_start_id: None,
        summary_text: Some("## Goal\n...".to_owned()),
        auto,
        overflow: auto,
    });
    state
}

#[test]
fn committed_summary_is_a_marker_with_its_recorded_trigger() {
    let auto = state_with_summary("summary-1", true);
    assert_eq!(
        compaction_marker(
            Some(&auto),
            "summary-1",
            None,
            SummaryStreamStatus::Complete
        ),
        Some(CompactionMarker::Compacted { auto: true })
    );
    let manual = state_with_summary("summary-1", false);
    assert_eq!(
        compaction_marker(
            Some(&manual),
            "summary-1",
            None,
            SummaryStreamStatus::Complete
        ),
        Some(CompactionMarker::Compacted { auto: false })
    );
}

#[test]
fn committed_summary_wins_over_the_exchange_input_and_stream_status() {
    // A restored summary can share an exchange with other output, and the record is the
    // authority on how it was triggered.
    let state = state_with_summary("summary-1", false);
    assert_eq!(
        compaction_marker(
            Some(&state),
            "summary-1",
            Some(true),
            SummaryStreamStatus::Streaming
        ),
        Some(CompactionMarker::Compacted { auto: false })
    );
}

#[test]
fn ordinary_agent_text_is_not_a_marker() {
    let state = state_with_summary("summary-1", true);
    // Another message in a conversation that has compacted.
    assert_eq!(
        compaction_marker(
            Some(&state),
            "answer-2",
            None,
            SummaryStreamStatus::Complete
        ),
        None
    );
    // A conversation that never compacted, or one that cannot be resolved.
    assert_eq!(
        compaction_marker(
            Some(&CompactionState::default()),
            "answer-2",
            None,
            SummaryStreamStatus::Complete
        ),
        None
    );
    assert_eq!(
        compaction_marker(None, "answer-2", None, SummaryStreamStatus::Streaming),
        None
    );
}

#[test]
fn summarize_exchange_is_in_progress_while_streaming() {
    for overflow in [true, false] {
        assert_eq!(
            compaction_marker(None, "m", Some(overflow), SummaryStreamStatus::Streaming),
            Some(CompactionMarker::InProgress)
        );
    }
}

#[test]
fn summarize_exchange_that_finished_uses_its_overflow_flag_before_commit() {
    // The commit runs after the stream finishes; until it lands (or if no conversation is
    // resolvable), the request's overflow flag says which trigger it was.
    assert_eq!(
        compaction_marker(
            Some(&CompactionState::default()),
            "m",
            Some(true),
            SummaryStreamStatus::Complete
        ),
        Some(CompactionMarker::Compacted { auto: true })
    );
    assert_eq!(
        compaction_marker(None, "m", Some(false), SummaryStreamStatus::Complete),
        Some(CompactionMarker::Compacted { auto: false })
    );
}

#[test]
fn summarize_exchange_stopped_before_commit_is_interrupted() {
    assert_eq!(
        compaction_marker(None, "m", Some(true), SummaryStreamStatus::Stopped),
        Some(CompactionMarker::Interrupted)
    );
}

#[test]
fn labels_distinguish_every_marker_state() {
    crate::i18n::init(Some("en"));
    assert_eq!(
        CompactionMarker::InProgress.label(),
        "Compacting conversation…"
    );
    assert_eq!(
        CompactionMarker::Compacted { auto: true }.label(),
        "Conversation compacted automatically"
    );
    assert_eq!(
        CompactionMarker::Compacted { auto: false }.label(),
        "Conversation compacted"
    );
    assert_eq!(
        CompactionMarker::Interrupted.label(),
        "Conversation compaction stopped"
    );
}
