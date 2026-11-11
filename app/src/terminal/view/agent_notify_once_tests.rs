use super::{Episode, NotifiedEpisodes, every_event_from, status_notifies};
use crate::ai::agent::conversation::ConversationStatus;

fn blocked(action: &str) -> Episode {
    Episode {
        state: "blocked",
        marker: Some(action.to_string()),
    }
}

fn ended(state: &'static str, exchange: &str) -> Episode {
    Episode {
        state,
        marker: Some(exchange.to_string()),
    }
}

/// #2140: the status change and the AI block's event name the same blocked action; one
/// notification, however many events repeat it (a pane with several AI blocks re-emits it once
/// per block).
#[test]
fn one_blocked_action_is_notified_for_once() {
    let mut seen = NotifiedEpisodes::<u32>::default();
    assert!(seen.should_send(1, blocked("call_a"), false));
    assert!(!seen.should_send(1, blocked("call_a"), false));
    assert!(!seen.should_send(1, blocked("call_a"), false));
}

/// The next action the agent waits on is a new episode, with or without a status change between.
#[test]
fn the_next_blocked_action_is_notified_for_again() {
    let mut seen = NotifiedEpisodes::<u32>::default();
    assert!(seen.should_send(1, blocked("call_a"), false));
    assert!(seen.should_send(1, blocked("call_b"), false));
    assert!(!seen.should_send(1, blocked("call_b"), false));
}

/// Answered, the conversation runs on (a status that does not notify) and is blocked again on
/// an action with the same id (a retried call): told again.
#[test]
fn a_status_that_does_not_notify_ends_the_episode() {
    let mut seen = NotifiedEpisodes::<u32>::default();
    assert!(seen.should_send(1, blocked("call_a"), false));
    seen.episode_ended(&1);
    assert_eq!(seen.remembered(), 0);
    assert!(seen.should_send(1, blocked("call_a"), false));
}

/// A finished turn arrives as the status change and as the AI block's `Finished`: one
/// notification. Blocked and then ended on the same exchange are two episodes.
#[test]
fn one_finished_turn_is_notified_for_once_and_is_not_its_blocked_episode() {
    let mut seen = NotifiedEpisodes::<u32>::default();
    assert!(seen.should_send(1, blocked("call_a"), false));
    assert!(seen.should_send(1, ended("success", "exchange_1"), false));
    assert!(!seen.should_send(1, ended("success", "exchange_1"), false));
    assert!(seen.should_send(1, ended("error", "exchange_1"), false));
    assert!(seen.should_send(1, ended("success", "exchange_2"), false));
}

/// Two conversations of one pane do not hide each other's notifications, in any order of
/// their events.
#[test]
fn conversations_are_kept_apart() {
    let mut seen = NotifiedEpisodes::<u32>::default();
    assert!(seen.should_send(1, blocked("call_a"), false));
    assert!(seen.should_send(2, blocked("call_a"), false));
    assert!(!seen.should_send(1, blocked("call_a"), false));
    assert!(!seen.should_send(2, blocked("call_a"), false));
    seen.episode_ended(&1);
    assert!(seen.should_send(1, blocked("call_a"), false));
    assert!(!seen.should_send(2, blocked("call_a"), false));
}

/// An episode with no action or exchange to name is the status alone.
#[test]
fn an_episode_without_a_marker_is_its_status() {
    let bare = |state| Episode {
        state,
        marker: None,
    };
    let mut seen = NotifiedEpisodes::<u32>::default();
    assert!(seen.should_send(1, bare("blocked"), false));
    assert!(!seen.should_send(1, bare("blocked"), false));
    assert!(seen.should_send(1, bare("error"), false));
}

/// The kill switch sends for every event, as before #2140.
#[test]
fn the_kill_switch_sends_for_every_event() {
    let mut seen = NotifiedEpisodes::<u32>::default();
    assert!(seen.should_send(1, blocked("call_a"), true));
    assert!(seen.should_send(1, blocked("call_a"), true));
    assert!(!every_event_from(None));
    assert!(!every_event_from(Some("")));
    assert!(!every_event_from(Some("0")));
    assert!(every_event_from(Some("1")));
    assert!(every_event_from(Some(" 1 ")));
}

/// The three statuses the view notifies for, as before #2140; every other one ends the episode.
#[test]
fn only_blocked_success_and_error_notify() {
    assert!(status_notifies(&ConversationStatus::Success));
    assert!(status_notifies(&ConversationStatus::Error));
    assert!(status_notifies(&ConversationStatus::Blocked {
        blocked_action: "ask".to_string(),
    }));
    for quiet in [
        ConversationStatus::InProgress,
        ConversationStatus::TransientError,
        ConversationStatus::Cancelled,
        ConversationStatus::WaitingForEvents,
    ] {
        assert!(!status_notifies(&quiet), "{quiet:?}");
    }
}
