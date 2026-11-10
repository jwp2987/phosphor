use super::*;

fn facts(
    is_bootstrapped: bool,
    has_pending_ssh_session: bool,
    is_transcript_viewer: bool,
    was_ever_visible: bool,
) -> DroppedViewFacts {
    DroppedViewFacts {
        is_bootstrapped,
        has_pending_ssh_session,
        is_transcript_viewer,
        was_ever_visible,
    }
}

/// #1983: a bootstrapped shell session that is closed is not an abandoned session.
#[test]
fn a_bootstrapped_session_logs_nothing() {
    assert_eq!(
        abandoned_before_bootstrap_level(facts(true, false, false, true)),
        None
    );
    assert_eq!(
        abandoned_before_bootstrap_level(facts(true, false, false, false)),
        None
    );
}

/// The existing rule, kept: a shell session dropped before bootstrap is an error when it was
/// ever on screen and a warning when it never was.
#[test]
fn a_shell_session_dropped_before_bootstrap_logs_by_visibility() {
    assert_eq!(
        abandoned_before_bootstrap_level(facts(false, false, false, true)),
        Some(log::Level::Error)
    );
    assert_eq!(
        abandoned_before_bootstrap_level(facts(false, false, false, false)),
        Some(log::Level::Warn)
    );
}

/// The existing rule, kept: an ssh wrapper session whose `InitShell` never came is abandoned even
/// though the local shell under it is bootstrapped.
#[test]
fn a_pending_ssh_session_logs_even_when_the_local_shell_is_bootstrapped() {
    assert_eq!(
        abandoned_before_bootstrap_level(facts(true, true, false, true)),
        Some(log::Level::Error)
    );
    assert_eq!(
        abandoned_before_bootstrap_level(facts(true, true, false, false)),
        Some(log::Level::Warn)
    );
}

/// #1983: a transcript viewer or loading pane never has a shell, so its block list never
/// bootstraps. Closing one used to log the line at ERROR; it is not a session.
#[test]
fn a_transcript_viewer_is_never_an_abandoned_session() {
    for is_bootstrapped in [false, true] {
        for has_pending_ssh_session in [false, true] {
            for was_ever_visible in [false, true] {
                assert_eq!(
                    abandoned_before_bootstrap_level(facts(
                        is_bootstrapped,
                        has_pending_ssh_session,
                        true,
                        was_ever_visible
                    )),
                    None
                );
            }
        }
    }
}
