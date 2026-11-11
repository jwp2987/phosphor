use std::time::Duration;

use warpui::input_order::{TextShown, assert_line_withholds};

use super::log_lines;

const LINE: &str = "Token for hunter2pw? [y/N] ";

/// #2100: the hand-over line holds the cursor line's text only with the text opt-in and never
/// during a secret read. With the opt-in (the harness sets it) it is the line as it was, which
/// script/realapp/appci/selftest.py feeds to `lrc_takeover_verdict` and `handover_seen`.
#[test]
fn the_hand_over_line_withholds_the_cursor_line() {
    let line = |shown| log_lines::handed_to_user(Some(LINE), Duration::ZERO, shown);
    assert_eq!(
        line(TextShown::fixed(false, false)),
        "interactive prompt Some(<27 chars>) unchanged for 0ns, foreground reading the tty; \
         handing the command to the user"
    );
    assert_eq!(
        log_lines::handed_to_user(
            Some("Continue? [y/N] "),
            Duration::ZERO,
            TextShown::fixed(true, false)
        ),
        "interactive prompt Some(\"Continue? [y/N] \") unchanged for 0ns, foreground reading \
         the tty; handing the command to the user"
    );
    assert_line_withholds(line, &["hunter2pw", "Token"]);
}

#[test]
fn the_take_over_notice_line_withholds_the_cursor_line() {
    let line = |shown| log_lines::take_over_notice(Some(LINE), Duration::from_secs(3), shown);
    assert_eq!(
        line(TextShown::fixed(false, false)),
        "prompt-shaped line Some(<27 chars>) unchanged for 3s, tty read unknown; showing a \
         take-over notice (the command stays with the agent)"
    );
    assert_line_withholds(line, &["hunter2pw", "Token"]);
}

#[test]
fn the_left_with_agent_line_withholds_the_cursor_line() {
    let what = "the foreground is not reading the tty";
    let line = |shown| log_lines::left_with_agent(Some(LINE), Duration::from_secs(3), what, shown);
    assert_eq!(
        line(TextShown::fixed(false, false)),
        "prompt-shaped line Some(<27 chars>) unchanged for 3s, but the foreground is not \
         reading the tty; leaving the command with the agent"
    );
    assert_line_withholds(line, &["hunter2pw", "Token"]);
    assert_eq!(
        log_lines::left_with_agent(
            None,
            Duration::from_secs(3),
            what,
            TextShown::fixed(false, false)
        ),
        "prompt-shaped line None unchanged for 3s, but the foreground is not reading the tty; \
         leaving the command with the agent"
    );
}
