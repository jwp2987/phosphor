use super::allowed_for_session;

/// #1982: a query prediction sends the input buffer to the model provider at every typing pause.
/// In an ssh session, and for a pane whose session is not known, it is requested only when the
/// user turned `natural_language_autosuggestions_in_ssh_enabled` on.
#[test]
fn a_query_prediction_is_not_requested_in_an_ssh_session_unless_allowed() {
    let in_ssh = Some(true);
    let local = Some(false);
    let unknown = None;

    assert!(allowed_for_session(local, false));
    assert!(!allowed_for_session(in_ssh, false));
    assert!(
        !allowed_for_session(unknown, false),
        "not knowing the session must not send the buffer"
    );

    for session in [local, in_ssh, unknown] {
        assert!(allowed_for_session(session, true));
    }
}

/// #2094: the same rule gates prompt suggestions (the finished command and a summary of its
/// output) and next-command suggestions (recent commands, the typed prefix). The table is the
/// one decision all three features share; each passes its own `..._in_ssh_enabled` setting.
#[test]
fn every_active_ai_request_shares_the_ssh_rule() {
    // (session, the feature's in-ssh setting) -> request allowed
    let table = [
        (Some(false), false, true),
        (Some(true), false, false),
        (None, false, false),
        (Some(false), true, true),
        (Some(true), true, true),
        (None, true, true),
    ];
    for (in_ssh_session, allowed_in_ssh, want) in table {
        assert_eq!(
            allowed_for_session(in_ssh_session, allowed_in_ssh),
            want,
            "session {in_ssh_session:?}, allowed in ssh {allowed_in_ssh}"
        );
    }
}

/// #2097: a passive code diff (file contents and the finished block, started by a prompt
/// suggestion, nobody asked) is gated by the same decision with its own setting,
/// `code_suggestions_in_ssh_enabled`. `maybe_generate_passive_code_diff` passes
/// `active_session.session(ctx).map(is_ssh_session)`: a pane with no session is `None`.
#[test]
fn a_passive_code_diff_is_not_requested_in_an_ssh_session_unless_allowed() {
    let code_suggestions_in_ssh_enabled = false;
    assert!(allowed_for_session(
        Some(false),
        code_suggestions_in_ssh_enabled
    ));
    assert!(!allowed_for_session(
        Some(true),
        code_suggestions_in_ssh_enabled
    ));
    assert!(
        !allowed_for_session(None, code_suggestions_in_ssh_enabled),
        "not knowing the session must not read and send files"
    );

    let code_suggestions_in_ssh_enabled = true;
    for session in [Some(false), Some(true), None] {
        assert!(allowed_for_session(
            session,
            code_suggestions_in_ssh_enabled
        ));
    }
}
