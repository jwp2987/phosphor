use std::time::Duration;

use instant::Instant;
use warp_core::command::ExitCode;

use super::*;

fn memory_with(
    text: &str,
    route: SubmitRoute,
    reason: RouteReason,
    now: Instant,
) -> MisrouteMemory {
    let mut memory = MisrouteMemory::default();
    memory.record_submit(text, route, reason, now);
    memory
}

#[test]
fn autodetected_shell_with_127_offers_send_to_agent() {
    let now = Instant::now();
    let mut memory = memory_with(
        "what is a monad",
        SubmitRoute::Shell,
        RouteReason::Autodetected,
        now,
    );
    assert_eq!(memory.offer(now), None, "no exit code yet, no offer");
    assert!(memory.note_command_finished("what is a monad", ExitCode::from(127), now));
    assert_eq!(
        memory.offer(now),
        Some(MisrouteOffer::SendToAgent {
            text: "what is a monad".to_owned()
        })
    );
}

#[test]
fn autodetected_shell_with_other_exit_codes_offers_nothing() {
    let now = Instant::now();
    for code in [0, 1, 2, 126, 130] {
        let mut memory = memory_with(
            "ls nope",
            SubmitRoute::Shell,
            RouteReason::Autodetected,
            now,
        );
        memory.note_command_finished("ls nope", ExitCode::from(code), now);
        assert_eq!(memory.offer(now), None, "exit code {code}");
    }
}

#[test]
fn explicit_and_locked_routing_never_offer() {
    let now = Instant::now();
    for reason in [RouteReason::Explicit, RouteReason::Locked] {
        let mut shell = memory_with("nosuchcmd", SubmitRoute::Shell, reason, now);
        shell.note_command_finished("nosuchcmd", ExitCode::from(127), now);
        assert_eq!(shell.offer(now), None, "{reason:?} shell");

        let ai = memory_with("ls -la", SubmitRoute::Ai, reason, now);
        assert_eq!(ai.offer(now), None, "{reason:?} ai");
    }
}

#[test]
fn exit_code_for_a_different_command_is_ignored() {
    let now = Instant::now();
    let mut memory = memory_with(
        "foo bar",
        SubmitRoute::Shell,
        RouteReason::Autodetected,
        now,
    );
    assert!(!memory.note_command_finished("something else", ExitCode::from(127), now));
    assert_eq!(memory.offer(now), None);
    // Whitespace around the command is not a difference.
    assert!(memory.note_command_finished("  foo bar\n", ExitCode::from(127), now));
    assert!(memory.offer(now).is_some());
}

#[test]
fn exit_code_does_not_attach_to_an_ai_submit() {
    let now = Instant::now();
    let mut memory = memory_with("ls", SubmitRoute::Ai, RouteReason::Autodetected, now);
    assert!(!memory.note_command_finished("ls", ExitCode::from(127), now));
}

#[test]
fn autodetected_ai_offers_run_as_shell_within_the_window() {
    let now = Instant::now();
    let memory = memory_with(
        "  git status ",
        SubmitRoute::Ai,
        RouteReason::Autodetected,
        now,
    );
    let expected = Some(MisrouteOffer::RunAsShell {
        text: "git status".to_owned(),
    });
    assert_eq!(memory.offer(now), expected);
    assert_eq!(memory.offer(now + Duration::from_secs(5)), expected);
    assert_eq!(
        memory.offer(now + OFFER_WINDOW + Duration::from_secs(1)),
        None
    );
}

#[test]
fn editing_dismisses_but_emptying_does_not() {
    let now = Instant::now();
    let mut memory = memory_with(
        "git status",
        SubmitRoute::Ai,
        RouteReason::Autodetected,
        now,
    );
    assert!(
        !memory.note_buffer_edited(true),
        "an empty buffer is not new input"
    );
    assert!(memory.offer(now).is_some());
    assert!(memory.note_buffer_edited(false));
    assert_eq!(memory.offer(now), None);

    let mut shell = memory_with("foo", SubmitRoute::Shell, RouteReason::Autodetected, now);
    shell.note_command_finished("foo", ExitCode::from(127), now);
    assert!(shell.note_buffer_edited(false));
    assert_eq!(shell.offer(now), None);
}

#[test]
fn next_submit_replaces_the_offer() {
    let now = Instant::now();
    let mut memory = memory_with("foo", SubmitRoute::Shell, RouteReason::Autodetected, now);
    memory.note_command_finished("foo", ExitCode::from(127), now);
    memory.record_submit("ls", SubmitRoute::Shell, RouteReason::Autodetected, now);
    assert_eq!(memory.offer(now), None);
    memory.record_submit("   ", SubmitRoute::Ai, RouteReason::Autodetected, now);
    assert_eq!(memory.offer(now), None, "blank submits are not remembered");
}

#[test]
fn taking_the_offer_is_one_shot() {
    let now = Instant::now();
    let mut memory = memory_with(
        "git status",
        SubmitRoute::Ai,
        RouteReason::Autodetected,
        now,
    );
    assert_eq!(
        memory.take_offer(now),
        Some(MisrouteOffer::RunAsShell {
            text: "git status".to_owned()
        })
    );
    assert_eq!(memory.take_offer(now), None);
    assert_eq!(memory.offer(now), None);
}

#[test]
fn recovery_resubmit_recorded_as_locked_is_not_offered_again() {
    // The re-submit runs with the input locked, so what it records carries `Locked`.
    let now = Instant::now();
    let mut memory = memory_with(
        "what is a monad",
        SubmitRoute::Shell,
        RouteReason::Autodetected,
        now,
    );
    memory.note_command_finished("what is a monad", ExitCode::from(127), now);
    let Some(MisrouteOffer::SendToAgent { text }) = memory.take_offer(now) else {
        panic!("expected an offer");
    };
    memory.record_submit(&text, SubmitRoute::Ai, RouteReason::Locked, now);
    assert_eq!(memory.offer(now), None);
}

#[test]
fn only_the_autodetected_input_entry_keeps_the_offer() {
    use crate::ai::blocklist::agent_view::AgentViewEntryOrigin;

    assert!(agent_view_entry_keeps_offer(&AgentViewEntryOrigin::Input {
        was_prompt_autodetected: true
    }));
    assert!(!agent_view_entry_keeps_offer(
        &AgentViewEntryOrigin::Input {
            was_prompt_autodetected: false
        }
    ));
    assert!(!agent_view_entry_keeps_offer(
        &AgentViewEntryOrigin::MisrouteRecovery
    ));
    assert!(!agent_view_entry_keeps_offer(&AgentViewEntryOrigin::Cli));
}

#[test]
fn multi_line_text_is_never_offered_as_shell() {
    let now = Instant::now();
    let memory = memory_with(
        "echo a\necho b",
        SubmitRoute::Ai,
        RouteReason::Autodetected,
        now,
    );
    assert_eq!(memory.offer(now), None);
}

#[test]
fn send_to_agent_offer_expires_from_when_the_command_finished() {
    let now = Instant::now();
    let mut memory = memory_with("foo", SubmitRoute::Shell, RouteReason::Autodetected, now);
    let finished = now + Duration::from_secs(300);
    memory.note_command_finished("foo", ExitCode::from(127), finished);
    assert!(memory.offer(finished).is_some());
    assert_eq!(
        memory.offer_expires_at(finished),
        Some(finished + OFFER_WINDOW)
    );
    assert_eq!(
        memory.offer(finished + OFFER_WINDOW + Duration::from_secs(1)),
        None
    );
}

#[test]
fn expiry_is_reported_only_while_an_offer_exists() {
    let now = Instant::now();
    let memory = memory_with(
        "git status",
        SubmitRoute::Ai,
        RouteReason::Autodetected,
        now,
    );
    assert_eq!(memory.offer_expires_at(now), Some(now + OFFER_WINDOW));
    assert_eq!(
        memory.offer_expires_at(now + OFFER_WINDOW + Duration::from_secs(1)),
        None
    );
}
